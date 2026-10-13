//! 目标树智能拆解编排层（源 A 入口 → 统一求解管线）
//!
//! 实现《整合设计方案》§2.5 统一规划流水线中与纯计算相关的阶段（本批次）：
//! ② 装配 PlanSnapshot（[`solve`] 内唯一触碰数据库的环节，只读）→
//! ③ 意图解析（[`adapt_request`]：源 A 六层配置 → UnifiedPlanRequest，字段树
//! 按 §2.6.1 映射）→ ⑤ A1 约束编译（constraints::compile_constraints）→
//! ⑥ 模式选择（F-4 映射表 + auto 规则推断，[`resolve_pattern`]）→
//! ⑦ A2 可行性判定（不可行时**仍返回**携带 remedies 的 SplitPlan，对应
//! E-INFEASIBLE 的降级行为）→ ⑩ A3 加权公平分配 + A3.1 迟滞守恒补偿
//! （拆解场景 old_placements 为空切片，天然直通）→ ⑪ A4 DAG 调度（拆解
//! 场景无既有依赖时直通顺序排布）→ ⑫ 序列优化（阶段切分 · 复习注入）→
//! ⑬ 语义生成（模板渲染，占位符白名单，未识别占位符原样保留）→
//! ⑭ 不变式校验（INV-1 守恒显式校验，违反返回 Err；其余记 warning）→
//! ⑮ 理由链（rationale）→ ⑯ plan_hash 摘要（[`compute_plan_hash`]）。
//! ⑰ 一致性校验、⑱ 事务应用、⑲ 反馈登记属 Phase 2/3，本批次不做。
//!
//! **本批次不做任何数据库写入**（dry_run 语义；split_plans 落库由 Phase 2
//! 命令层执行），[`solve`] 接收 pool 仅供 [`load_snapshot`] 只读装配。
//!
//! legacy 兼容：`req.legacy` 存在时走旧语义直通（[`solve_legacy`]），按
//! strategy 复刻旧 smart_split 三种策略的算术（含错误语义），产出等价的
//! 均匀方案，保证 Phase 2 命令层可无损切换。
//!
//! plan_hash（F-8/S-4）：`plan_id` 是实体主键、`plan_hash` 是内容摘要，二者
//! 不等价。摘要覆盖 goal / 口径 / 逐项内容，**不含** plan_id 与 generated_at
//! （同一输入两次求解必须产生相同摘要，NFR-C2 / AC-06）。项目未引入 sha2
//! 依赖，SHA-256 以纯函数就地实现（[`sha256_hex`]），输出格式
//! `sha256:<64 hex>`。

use std::collections::{HashMap, HashSet};

use chrono::{Datelike, Duration, NaiveDate};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::db::models::Goal;
use crate::error::{AppError, AppResult};
use crate::services::planner::allocator::{allocate_weights, apply_hysteresis, DayQuota};
use crate::services::planner::constraints::{
    compile_constraints, CompiledSpec, DEFAULT_WINDOW_CAP_DAYS,
};
use crate::services::planner::dag_scheduler::{schedule_dag, PlacementReason, SchedTask};
use crate::services::planner::feasibility::{build_remedies, check_feasibility};
use crate::services::planner::snapshot::{load_snapshot, DaySlot, HorizonSpan, PlanSnapshot};
use crate::services::planner::types::{
    AppliedRule, ConstraintSpec, ConstraintsLayer, FeasibilityReport, FeasibilityStatus,
    HorizonInput, LegacySpec, Objective, PhaseSpec, PlanDiagnostics, PlanItem, PlanSummary,
    PlanWarning, PlanningSpec, ResolvedHorizon, ResolvedPlanMeta, ReviewSpec, Scope,
    SmartSplitRequest, SplitPattern, SplitPlan, SplitShape, TaskType, UnifiedPlanRequest,
    ENGINE_VERSION, MIN_PHASES,
};
use crate::util::parse_date_or_param;

/// 默认学习任务命名模板（与旧引擎「目标名 - 第N天」对齐）
const DEFAULT_NAMING_TEMPLATE: &str = "{goal} - 第{n}天";
/// 默认复习任务命名模板
const DEFAULT_REVIEW_TEMPLATE: &str = "{goal} - 复习(第{n}天)";
/// 复习量系数缺省值（review.review_ratio 未提供时；文档未定义缺省，取 0.5）
const DEFAULT_REVIEW_RATIO: f64 = 0.5;
/// 守恒校验容差（INV-1：误差 < 1e-6）
const CONSERVATION_EPS: f64 = 1e-6;
/// 配额零保护阈值
const QTY_EPS: f64 = 1e-9;

// ============================================================
// SHA-256（纯函数；FIPS 180-4）
// ============================================================

/// SHA-256 轮常量表
const SHA256_K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
    0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
    0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
    0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
    0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
    0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
    0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
    0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
    0xc67178f2,
];

/// SHA-256 摘要（输出小写 hex；标准测试向量见单元测试）
fn sha256_hex(data: &[u8]) -> String {
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    // 填充：0x80 + 零 + 64 位大端比特长度（至 56 (mod 64)）
    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for block in msg.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (i, word) in w.iter_mut().take(16).enumerate() {
            *word = u32::from_be_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d) = (h[0], h[1], h[2], h[3]);
        let (mut e, mut f, mut g, mut z) = (h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = z
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(SHA256_K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            z = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (hi, val) in h.iter_mut().zip([a, b, c, d, e, f, g, z]) {
            *hi = hi.wrapping_add(val);
        }
    }
    h.iter().map(|x| format!("{:08x}", x)).collect()
}

// ============================================================
// ⑯ plan_hash 摘要
// ============================================================

/// plan_hash 摘要（格式 `sha256:<64 hex>`）
///
/// 规范化内容：`v2|goal_id|api_version|pattern|total_qty|start|end|条目数`
/// + 逐项 `|seq;name;date;qty;task_type;phase`。排除 plan_id / generated_at /
/// description / rationale（内容派生项或每次生成项），保证同一输入产出相同
/// 摘要（⑰ 提交一致性校验的前提）。
pub fn compute_plan_hash(
    goal_id: &str,
    api_version: u32,
    pattern: SplitPattern,
    total_qty: f64,
    start: NaiveDate,
    end: NaiveDate,
    items: &[PlanItem],
) -> String {
    let mut canonical = format!(
        "v2|{}|{}|{:?}|{:.6}|{}|{}|{}",
        goal_id,
        api_version,
        pattern,
        total_qty,
        start.format("%Y-%m-%d"),
        end.format("%Y-%m-%d"),
        items.len()
    );
    for it in items {
        canonical.push_str(&format!(
            "|{};{};{};{:.6};{:?};{}",
            it.seq,
            it.name,
            it.plan_date.format("%Y-%m-%d"),
            it.plan_qty,
            it.task_type,
            it.phase.clone().unwrap_or_default()
        ));
    }
    format!("sha256:{}", sha256_hex(canonical.as_bytes()))
}

// ============================================================
// ⑬ 语义生成：模板渲染
// ============================================================

/// 模板占位符上下文（白名单：{goal}{phase}{n}{scope}{cum}{total}{unit}{hours}{date}）
pub struct TemplateCtx<'a> {
    /// 目标名
    pub goal: &'a str,
    /// 阶段名（非阶段化时为 None）
    pub phase: Option<&'a str>,
    /// 任务序号
    pub n: u32,
    /// 范围描述（大纲条目或累计量区间）
    pub scope: String,
    /// 完成本任务后的累计量
    pub cum: String,
    /// 总量
    pub total: String,
    /// 单位
    pub unit: &'a str,
    /// 预估时长（未设置时为空串）
    pub hours: String,
    /// 计划日期
    pub date: String,
}

/// 渲染命名/描述模板
///
/// 白名单占位符替换为上下文值；**未识别的占位符原样保留**（不报错）；
/// 未闭合的 `{` 亦原样保留。
pub fn render_template(template: &str, ctx: &TemplateCtx<'_>) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('}') {
            Some(end) => {
                let key = &after[..end];
                match key {
                    "goal" => out.push_str(ctx.goal),
                    "phase" => out.push_str(ctx.phase.unwrap_or("")),
                    "n" => out.push_str(&ctx.n.to_string()),
                    "scope" => out.push_str(&ctx.scope),
                    "cum" => out.push_str(&ctx.cum),
                    "total" => out.push_str(&ctx.total),
                    "unit" => out.push_str(ctx.unit),
                    "hours" => out.push_str(&ctx.hours),
                    "date" => out.push_str(&ctx.date),
                    // 未识别占位符：原样保留 "{key}"
                    _ => out.push_str(&rest[start..start + 1 + end + 1]),
                }
                rest = &after[end + 1..];
            }
            None => {
                // 未闭合的 '{'：保留从该 '{' 起的剩余原文（此前内容已输出）
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// 数量展示：整数直接显示，小数保留 1 位（0.5 → "0.5"、10.0 → "10"）
fn fmt_qty(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 {
        format!("{}", v.round() as i64)
    } else {
        format!("{:.1}", v)
    }
}

/// 范围描述（{scope} 占位符的取值）
///
/// 有大纲：按累计量占比把大纲条目线性映射到任务区间（第 k 个任务覆盖
/// 累计量 [prev, cur] 对应的条目段，多点用「、」连接）；无大纲：累计量
/// 区间字符串（如 "1~10页"）。
fn outline_scope(outline: &[String], prev: f64, cur: f64, total: f64, unit: &str) -> String {
    if outline.is_empty() || total <= 0.0 {
        return format!(
            "{}~{}{}",
            fmt_qty((prev + 1.0).max(1.0)),
            fmt_qty(cur.max(prev + 1.0)),
            unit
        );
    }
    let n = outline.len() as f64;
    let start = ((prev / total).clamp(0.0, 1.0) * n).floor() as usize;
    let end = (((cur / total).clamp(0.0, 1.0) * n).ceil() as usize)
        .saturating_sub(1)
        .min(outline.len() - 1);
    let end = end.max(start);
    outline[start..=end].join("、")
}

// ============================================================
// ⑥ 模式选择（F-4 映射表 + auto 规则推断）
// ============================================================

/// 拆解模式标签（与 SplitPattern 的 SCREAMING_SNAKE_CASE 序列化一致）
fn pattern_label(pattern: SplitPattern) -> &'static str {
    match pattern {
        SplitPattern::Even => "EVEN",
        SplitPattern::CapacityBound => "CAPACITY_BOUND",
        SplitPattern::Phased => "PHASED",
        SplitPattern::SpacedRepetition => "SPACED_REPETITION",
        SplitPattern::MilestoneBackward => "MILESTONE_BACKWARD",
        SplitPattern::Hybrid => "HYBRID",
    }
}

/// 解析实际拆解模式
///
/// 显式 shape 按 F-4 映射表单向映射（`milestone` → `Phased` + 里程碑标记位，
/// 标记位经 `stage_hint`/`phase` 承载）；`auto` 由简化规则引擎推断
/// （优先级：复习+阶段 → HYBRID；复习 → SPACED_REPETITION；阶段 → PHASED；
/// 目标函数 MaximizeThroughput / MinimizeDailyLoad → CAPACITY_BOUND；
/// 其余 → EVEN）。
pub fn resolve_pattern(
    shape: SplitShape,
    objective: Objective,
    phases: Option<&[PhaseSpec]>,
    review: Option<&ReviewSpec>,
) -> SplitPattern {
    match shape {
        SplitShape::Even => return SplitPattern::Even,
        SplitShape::FrontLoaded | SplitShape::BackLoaded => {
            return SplitPattern::CapacityBound
        }
        SplitShape::Phased => return SplitPattern::Phased,
        SplitShape::SpacedRepetition => return SplitPattern::SpacedRepetition,
        // F-4：milestone → PHASED + milestone 标记（保留里程碑倒推语义）
        SplitShape::Milestone => return SplitPattern::Phased,
        SplitShape::Auto => {}
    }
    let review_on = review
        .map(|r| r.enabled.unwrap_or(false) && !r.intervals.is_empty())
        .unwrap_or(false);
    let phased = phases.map(|p| p.len() >= MIN_PHASES).unwrap_or(false);
    if review_on && phased {
        return SplitPattern::Hybrid;
    }
    if review_on {
        return SplitPattern::SpacedRepetition;
    }
    if phased {
        return SplitPattern::Phased;
    }
    match objective {
        Objective::MaximizeThroughput | Objective::MinimizeDailyLoad => {
            SplitPattern::CapacityBound
        }
        _ => SplitPattern::Even,
    }
}

// ============================================================
// 权重整形（阶段切分 + 前重后轻曲线）
// ============================================================

/// 阶段日段切分（纯函数，确定性）
///
/// 段长 ∝ ratio / pace（节奏越快天数越少），配合段内权重 × pace，使各阶段
/// 工作量占比精确等于 ratio；日数分配用最大余数法保证 Σ段长 == day_count。
/// 返回 (阶段名, 起始日索引, 段长, 节奏)；phases 无效（< 2 个或日数不足）
/// 返回 None（调用方退化为非阶段化）。
fn phase_segments(
    phases: &[PhaseSpec],
    day_count: usize,
) -> Option<Vec<(String, usize, usize, f64)>> {
    if phases.len() < MIN_PHASES || day_count < phases.len() {
        return None;
    }
    let n = phases.len();
    let paces: Vec<f64> = phases
        .iter()
        .map(|p| p.pace.unwrap_or(1.0).max(1e-6))
        .collect();
    let raw: Vec<f64> = phases
        .iter()
        .zip(&paces)
        .map(|(p, pace)| p.ratio / pace)
        .collect();
    let raw_sum: f64 = raw.iter().sum();
    if !raw_sum.is_finite() || raw_sum <= 0.0 {
        return None;
    }
    // 每阶段保底 1 天，余量按 raw 比例 + 最大余数法分配
    let extra = day_count - n;
    let mut lens = vec![1usize; n];
    let mut fracs = vec![0.0f64; n];
    for i in 0..n {
        let q = extra as f64 * raw[i] / raw_sum;
        lens[i] += q.floor() as usize;
        fracs[i] = q - q.floor();
    }
    let mut rem = extra - (lens.iter().sum::<usize>() - n);
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&x, &y| {
        fracs[y]
            .partial_cmp(&fracs[x])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut k = 0;
    while rem > 0 {
        lens[order[k % n]] += 1;
        rem -= 1;
        k += 1;
        if k > day_count {
            break; // 防御
        }
    }
    let mut result = Vec::with_capacity(n);
    let mut start = 0usize;
    for (p, (&len, &pace)) in phases.iter().zip(lens.iter().zip(paces.iter())) {
        result.push((p.name.clone(), start, len, pace));
        start += len;
    }
    Some(result)
}

/// 按模式对可用日权重整形（纯函数，返回整形后的可用日序列）
///
/// - `CapacityBound`：前重后轻 w_i ∝ (H-i) / 后重前轻 w_i ∝ (i+1)；
/// - `Phased` / `Hybrid` / `MilestoneBackward`：段内权重 × pace；
/// - 其余：权重透传（EVEN / SPACED_REPETITION）。
fn shape_slots(
    compiled: &CompiledSpec,
    pattern: SplitPattern,
    front_loaded: bool,
    back_loaded: bool,
    segments: Option<&[(String, usize, usize, f64)]>,
) -> Vec<DaySlot> {
    let mut slots: Vec<DaySlot> = compiled
        .days
        .iter()
        .filter(|d| d.available && !d.is_rest)
        .cloned()
        .collect();
    let h = slots.len();
    if h == 0 {
        return slots;
    }
    match pattern {
        SplitPattern::CapacityBound => {
            for (i, s) in slots.iter_mut().enumerate() {
                let factor = if front_loaded && !back_loaded {
                    (h - i) as f64
                } else if back_loaded && !front_loaded {
                    (i + 1) as f64
                } else {
                    1.0
                };
                s.weight *= factor;
            }
        }
        SplitPattern::Phased | SplitPattern::Hybrid | SplitPattern::MilestoneBackward => {
            if let Some(segs) = segments {
                let mut idx = 0usize;
                for (_, _, len, pace) in segs {
                    for _ in 0..*len {
                        if let Some(s) = slots.get_mut(idx) {
                            s.weight *= pace;
                        }
                        idx += 1;
                    }
                }
            }
        }
        _ => {}
    }
    slots
}

// ============================================================
// 草案合并（min_chunk_qty）
// ============================================================

/// 连续小配额日向前合并为一个任务草案
///
/// 当前累计量 < min_chunk 时并入下一日（受 max_daily_qty 约束：合并会超
/// 上限则提前切分，尾差记 W-CHUNK-UNDERSIZED）；min_chunk 未设置时每天
/// 一个任务（与旧引擎一致）。返回 (首个日索引, 合并量) 列表，Σ合并量
/// 与配额总量严格相等。
fn merge_drafts(quotas: &[DayQuota], min_chunk: f64, max_qty: Option<f64>) -> Vec<(usize, f64)> {
    let mut drafts: Vec<(usize, f64)> = Vec::new();
    let mut cur: Option<(usize, f64)> = None;
    for (i, q) in quotas.iter().enumerate() {
        if q.qty <= QTY_EPS {
            continue;
        }
        match cur {
            None => cur = Some((i, q.qty)),
            Some((first_day, acc)) => {
                let needs = acc < min_chunk - QTY_EPS;
                let fits = max_qty.map_or(true, |m| acc + q.qty <= m + QTY_EPS);
                if needs && fits {
                    cur = Some((first_day, acc + q.qty));
                } else {
                    drafts.push((first_day, acc));
                    cur = Some((i, q.qty));
                }
            }
        }
    }
    if let Some((first_day, acc)) = cur {
        drafts.push((first_day, acc));
    }
    drafts
}

// ============================================================
// ③ 请求适配（源 A → UnifiedPlanRequest，§2.6.1）
// ============================================================

/// 合并约束层（A 的 hard/soft → 统一 ConstraintSpec；§2.6.1 字段级适配表）
fn merge_constraints(layer: &ConstraintsLayer) -> ConstraintSpec {
    let hard = &layer.hard;
    let soft = &layer.soft;
    ConstraintSpec {
        // A 侧无此字段（B override / goal 回落链由 compile_constraints 走）
        daily_capacity: None,
        intensity: None,
        respect_locks: None,
        max_daily_qty: hard.max_daily_qty,
        max_daily_hours: hard.max_daily_hours,
        max_daily_tasks: hard.max_tasks_per_day.map(|v| v as f64),
        min_chunk_qty: hard.min_chunk_qty,
        global_daily_task_budget: hard.global_daily_task_budget,
        blackout_dates: hard.blackout_dates.clone(),
        blackout_ranges: hard.blackout_ranges.clone(),
        allowed_weekdays: hard.allowed_weekdays.clone(),
        weekend_factor: soft.weekend_factor,
        phase_factors: soft.phase_factors.clone(),
        prefer_skip_dates: soft.prefer_skip_dates.clone(),
        interleave: soft.interleave,
    }
}

/// 源 A 入口适配为内核统一入参（§2.6.1 字段树，源 A 侧）
///
/// - `scope`：`allow_sub_goals=true` → Subtree，否则 Single（B 侧字段
///   Phase 3 接入，goal_ids 先留 None）；
/// - legacy 存在时**完全等价旧 smart_split 语义**：legacy 的 7 个字段
///   优先于 intent 层（deadline/end_date → horizon，total_qty → 覆盖，
///   daily_capacity → 容量覆盖）。
pub fn adapt_request(req: &SmartSplitRequest) -> UnifiedPlanRequest {
    let (horizon, total_qty_override, legacy_capacity) = match &req.legacy {
        Some(legacy) => {
            let intent_horizon = req.intent.horizon.as_ref();
            let horizon = HorizonInput {
                start_date: legacy
                    .start_date
                    .clone()
                    .or_else(|| intent_horizon.and_then(|h| h.start_date.clone())),
                end_date: legacy
                    .deadline
                    .clone()
                    .or_else(|| legacy.end_date.clone())
                    .or_else(|| intent_horizon.and_then(|h| h.end_date.clone())),
            };
            (
                Some(horizon),
                legacy.total_qty.or(req.intent.total_qty_override),
                legacy.daily_capacity,
            )
        }
        None => (
            req.intent.horizon.clone(),
            req.intent.total_qty_override,
            None,
        ),
    };
    let mut constraints = merge_constraints(&req.constraints);
    if req.legacy.is_some() {
        constraints.daily_capacity = legacy_capacity;
    }
    UnifiedPlanRequest {
        goal_id: req.goal_id.clone(),
        scope: if req.structure.allow_sub_goals.unwrap_or(false) {
            Scope::Subtree
        } else {
            Scope::Single
        },
        goal_ids: None,
        api_version: req.api_version,
        mode: req.mode,
        horizon,
        objective: req.intent.objective,
        buffer_days: req.intent.buffer_days,
        total_qty_override,
        unit_override: req.intent.unit_override.clone(),
        constraints,
        structure: req.structure.clone(),
        semantic: req.semantic.clone(),
        learning: req.learning.clone(),
        planning: PlanningSpec {
            excluded_task_ids: Vec::new(),
            expected_plan_hash: None,
            dry_run: req.dry_run,
        },
        legacy: req.legacy.clone(),
    }
}

// ============================================================
// legacy 旧语义直通（等价复刻 split_service 三策略算术）
// ============================================================

/// legacy 路径的单任务规格（日期 / 数量 / 预估时长）
struct LegacyItemSpec {
    date: NaiveDate,
    qty: f64,
    estimated_hours: Option<f64>,
}

/// 解析截止日期（legacy.deadline 覆盖后的口径）
fn parse_deadline(goal: &Goal) -> AppResult<NaiveDate> {
    let deadline_str = goal
        .deadline
        .as_ref()
        .ok_or_else(|| AppError::Param("目标未设置截止日期，无法拆解".into()))?;
    parse_date_or_param(deadline_str, "截止日期")
}

/// legacy 三策略的任务规格生成（纯函数，算术与旧 smart_split 逐分支等价）
///
/// - `by_deadline`：floor 均分 + 余数分前几天（从明天起）；
/// - `by_capacity`：任务数 = ceil(total/cap)，每个 = min(剩余, cap)，
///   `estimated_hours = Some(cap)`；
/// - `by_date_range`：`per_day_qty` 给定时任务数 = ceil(total/per_day)
///   （`estimated_hours = Some(per_day)`，与旧行为一致）；否则按天数均分；
/// - 余数舍去场景（总量过小）产出空序列，由调用方以标记（不 Err）容忍。
fn legacy_item_specs(
    legacy: &LegacySpec,
    goal: &Goal,
    today: NaiveDate,
) -> AppResult<Vec<LegacyItemSpec>> {
    // 覆盖后的临时 Goal 副本（不写回）
    let mut effective = goal.clone();
    if let Some(qty) = legacy
        .total_qty
        .filter(|v| v.is_finite() && *v >= 0.0)
    {
        effective.total_qty = qty;
    }
    if let Some(deadline) = &legacy.deadline {
        effective.deadline = Some(deadline.clone());
    }
    let total = effective.total_qty;

    match legacy.strategy.as_str() {
        "by_deadline" => {
            let deadline = parse_deadline(&effective)?;
            let remaining_days = (deadline - today).num_days();
            if remaining_days < 1 {
                return Err(AppError::Business(format!(
                    "截止日期剩余天数不足（{}天），至少需要留出1天",
                    remaining_days
                )));
            }
            if total <= 0.0 {
                return Err(AppError::Business("目标总量必须大于0才能自动拆解".into()));
            }
            let base = (total / remaining_days as f64).floor();
            let remainder = (total - base * remaining_days as f64).round() as i64;
            let mut specs = Vec::new();
            for i in 0..remaining_days {
                let qty = if i < remainder { base + 1.0 } else { base };
                if qty <= 0.0 {
                    continue;
                }
                specs.push(LegacyItemSpec {
                    date: today + Duration::days(i + 1),
                    qty,
                    estimated_hours: None,
                });
            }
            Ok(specs)
        }
        "by_capacity" => {
            let capacity = legacy
                .daily_capacity
                .filter(|c| c.is_finite() && *c > 0.0)
                .or(effective
                    .daily_capacity
                    .filter(|c| c.is_finite() && *c > 0.0))
                .ok_or_else(|| {
                    AppError::Param(
                        "未设置每日可用时长（daily_capacity），无法按时间预算拆解".into(),
                    )
                })?;
            let deadline = parse_deadline(&effective)?;
            let remaining_days = (deadline - today).num_days();
            if remaining_days < 1 {
                return Err(AppError::Business(format!(
                    "截止日期剩余天数不足（{}天），至少需要留出1天",
                    remaining_days
                )));
            }
            if total <= 0.0 {
                return Err(AppError::Business("目标总量必须大于0才能自动拆解".into()));
            }
            let num_tasks = (total / capacity).ceil() as i64;
            if num_tasks == 0 {
                return Err(AppError::Business("计算任务数为0，无法拆解".into()));
            }
            if num_tasks > remaining_days {
                return Err(AppError::Business(format!(
                    "按每日{}{}拆解需要{}天，但截止日期前仅剩{}天，请调整每日可用时长或截止日期",
                    capacity, effective.unit, num_tasks, remaining_days
                )));
            }
            let mut specs = Vec::new();
            let mut allocated = 0.0;
            for i in 0..num_tasks {
                let remaining = total - allocated;
                let qty = remaining.min(capacity);
                allocated += qty;
                specs.push(LegacyItemSpec {
                    date: today + Duration::days(i + 1),
                    qty,
                    estimated_hours: Some(capacity),
                });
            }
            Ok(specs)
        }
        "by_date_range" => {
            let start_str = legacy.start_date.as_deref().ok_or_else(|| {
                AppError::Param("自定义日期范围策略必须指定起始日期".into())
            })?;
            let end_str = legacy.end_date.as_deref().ok_or_else(|| {
                AppError::Param("自定义日期范围策略必须指定结束日期".into())
            })?;
            let start = parse_date_or_param(start_str, "起始日期")?;
            let end = parse_date_or_param(end_str, "结束日期")?;
            if end < start {
                return Err(AppError::Param("结束日期不能早于起始日期".into()));
            }
            if total <= 0.0 {
                return Err(AppError::Business("目标总量必须大于0才能拆解".into()));
            }
            let span_days = (end - start).num_days() + 1;
            if span_days < 1 {
                return Err(AppError::Business(format!(
                    "日期范围内仅有{}天，至少需要1天",
                    span_days
                )));
            }
            match legacy
                .per_day_qty
                .filter(|q| q.is_finite() && *q > 0.0)
            {
                Some(qty) => {
                    let num_tasks = (total / qty).ceil() as i64;
                    if num_tasks == 0 {
                        return Err(AppError::Business("计算任务数为0，无法拆解".into()));
                    }
                    if num_tasks > span_days {
                        return Err(AppError::Business(format!(
                            "按每日{}{}拆解需要{}个任务，但日期范围内仅有{}天，请调整每日数量或日期范围",
                            qty, effective.unit, num_tasks, span_days
                        )));
                    }
                    let mut specs = Vec::new();
                    let mut allocated = 0.0;
                    for i in 0..num_tasks {
                        let remaining = total - allocated;
                        let q = remaining.min(qty);
                        allocated += q;
                        specs.push(LegacyItemSpec {
                            date: start + Duration::days(i),
                            qty: q,
                            estimated_hours: Some(qty),
                        });
                    }
                    Ok(specs)
                }
                None => {
                    let base = (total / span_days as f64).floor();
                    let remainder = (total - base * span_days as f64).round() as i64;
                    let mut specs = Vec::new();
                    for i in 0..span_days {
                        let qty = if i < remainder { base + 1.0 } else { base };
                        if qty <= 0.0 {
                            continue;
                        }
                        specs.push(LegacyItemSpec {
                            date: start + Duration::days(i),
                            qty,
                            estimated_hours: None,
                        });
                    }
                    Ok(specs)
                }
            }
        }
        other => Err(AppError::Param(format!(
            "未知的拆解策略: {}（支持: by_deadline | by_capacity | by_date_range）",
            other
        ))),
    }
}

// ============================================================
// 方案组装（summary + plan_hash + SplitPlan）
// ============================================================

/// 组装统一输出 SplitPlan
///
/// - 守恒（INV-1）：Σ 学习任务 plan_qty（排除 review）== total_qty；
///   `enforce_conservation=true`（新路径）时违反返回 Err；legacy 路径仅
///   标记（旧引擎无 INV-1，极小总量场景可能全部舍入为 0）；
/// - summary 汇总各类计数、守恒标记与日期跨度；
/// - plan_hash 在组装尾段计算（内容摘要，排除每次生成项）。
#[allow(clippy::too_many_arguments)]
fn assemble_plan(
    goal_id: &str,
    api_version: u32,
    items: Vec<PlanItem>,
    total_qty: f64,
    pattern: SplitPattern,
    objective: Objective,
    horizon: ResolvedHorizon,
    unit: String,
    feasibility: FeasibilityReport,
    diagnostics: PlanDiagnostics,
    enforce_conservation: bool,
) -> AppResult<SplitPlan> {
    let learn_sum: f64 = items
        .iter()
        .filter(|i| i.task_type == TaskType::Learn)
        .map(|i| i.plan_qty)
        .sum();
    let review_sum: f64 = items
        .iter()
        .filter(|i| i.task_type == TaskType::Review)
        .map(|i| i.plan_qty)
        .sum();
    let conservation_ok = (learn_sum - total_qty).abs() < CONSERVATION_EPS;
    if enforce_conservation && !conservation_ok {
        return Err(AppError::Internal(format!(
            "守恒不变式（INV-1）被破坏：Σ学习任务量 {} != 总量 {}",
            learn_sum, total_qty
        )));
    }
    let learn_count = items
        .iter()
        .filter(|i| i.task_type == TaskType::Learn)
        .count() as u32;
    let review_count = items
        .iter()
        .filter(|i| i.task_type == TaskType::Review)
        .count() as u32;
    let date_span = match (items.first(), items.last()) {
        (Some(f), Some(l)) => Some((f.plan_date, l.plan_date)),
        _ => None,
    };
    let summary = PlanSummary {
        task_count: items.len() as u32,
        learn_task_count: learn_count,
        review_task_count: review_count,
        practice_task_count: 0,
        total_allocated_qty: learn_sum,
        review_qty_total: review_sum,
        conservation_ok,
        date_span,
        explanation: Some(format!(
            "共生成 {} 个任务（学习 {} · 复习 {}），学习量合计 {}，守恒校验{}",
            items.len(),
            learn_count,
            review_count,
            fmt_qty(learn_sum),
            if conservation_ok { "通过" } else { "未通过" }
        )),
    };
    let plan_hash = compute_plan_hash(
        goal_id,
        api_version,
        pattern,
        total_qty,
        horizon.start,
        horizon.end,
        &items,
    );
    Ok(SplitPlan {
        plan_id: Uuid::new_v4(),
        goal_id: goal_id.to_string(),
        api_version,
        generated_at: chrono::Local::now().naive_local(),
        engine_version: ENGINE_VERSION.to_string(),
        plan_hash: Some(plan_hash),
        resolved: ResolvedPlanMeta {
            pattern,
            objective,
            horizon,
            total_qty,
            unit,
        },
        feasibility,
        diagnostics,
        items,
        summary,
    })
}

// ============================================================
// 求解管线
// ============================================================

/// 视界预解析（与 compile_constraints 的回落链一致：A 请求 > goal.deadline
/// > today + 30；start 恒不低于 today + 1），供快照装配确定查询窗口。
fn resolve_span(
    unified: &UnifiedPlanRequest,
    goal: &Goal,
    today: NaiveDate,
) -> AppResult<HorizonSpan> {
    let tomorrow = today + Duration::days(1);
    let (req_start, req_end) = match &unified.horizon {
        Some(h) => (
            h.start_date
                .as_deref()
                .map(|s| parse_date_or_param(s, "时间范围起始"))
                .transpose()?,
            h.end_date
                .as_deref()
                .map(|s| parse_date_or_param(s, "时间范围结束"))
                .transpose()?,
        ),
        None => (None, None),
    };
    let goal_deadline = goal
        .deadline
        .as_deref()
        .map(|s| parse_date_or_param(s, "截止"))
        .transpose()?;
    let end = req_end
        .or(goal_deadline)
        .unwrap_or(today + Duration::days(DEFAULT_WINDOW_CAP_DAYS))
        .max(tomorrow);
    let start = req_start.map_or(tomorrow, |s| s.max(tomorrow));
    if end < start {
        return Err(AppError::Param(
            "规划视界开始日期不能晚于结束日期".into(),
        ));
    }
    Ok(HorizonSpan { start, end })
}

/// 统一求解入口（源 A：SmartSplitRequest → SplitPlan）
///
/// 管线：validate → adapt_request → 装配快照（唯一 DB 读）→ A1 编译 →
/// 分流：legacy → 旧语义直通；否则 → 新引擎（A2 → 模式 → A3 → A4 →
/// 序列/语义 → 校验 → plan_hash）。
pub async fn solve(pool: &SqlitePool, req: &SmartSplitRequest) -> AppResult<SplitPlan> {
    // 防御性复验（命令层会先执行 validate + validate_request）
    req.validate_request()?;
    let unified = adapt_request(req);
    let today = chrono::Local::now().date_naive();

    // 目标存在性（E-NOTFOUND）+ 视界预解析（装配快照需先确定窗口）
    let goal: Goal = sqlx::query_as("SELECT * FROM goals WHERE id = ?")
        .bind(&req.goal_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("目标不存在".into()))?;
    let span = resolve_span(&unified, &goal, today)?;
    let snapshot = load_snapshot(pool, &req.goal_id, &span).await?;
    let compiled = compile_constraints(&unified, &goal, &snapshot)?;

    if req.legacy.is_some() {
        solve_legacy(req, &goal, &snapshot, &compiled)
    } else {
        solve_new(req, &unified, &snapshot, &compiled)
    }
}

/// 新引擎求解（纯计算；dry_run 语义，无 DB 写入）
fn solve_new(
    req: &SmartSplitRequest,
    unified: &UnifiedPlanRequest,
    snapshot: &PlanSnapshot,
    compiled: &CompiledSpec,
) -> AppResult<SplitPlan> {
    let mut warnings: Vec<PlanWarning> = Vec::new();
    let mut applied_rules: Vec<AppliedRule> = Vec::new();

    // ---------- ⑥ 模式选择 ----------
    let shape = req.structure.shape.unwrap_or_default();
    let pattern = resolve_pattern(
        shape,
        unified.objective,
        req.structure.phases.as_deref(),
        req.structure.review.as_ref(),
    );
    applied_rules.push(AppliedRule {
        id: "F-04".to_string(),
        dimension: "structure".to_string(),
        note: format!(
            "拆解形状 {:?} → 模式 {}",
            shape,
            pattern_label(pattern)
        ),
    });

    // ---------- 权重整形（阶段段切分 + 曲线） ----------
    let avail_count = compiled
        .days
        .iter()
        .filter(|d| d.available && !d.is_rest)
        .count();
    let segments = match pattern {
        SplitPattern::Phased | SplitPattern::Hybrid | SplitPattern::MilestoneBackward => req
            .structure
            .phases
            .as_deref()
            .and_then(|ps| phase_segments(ps, avail_count)),
        _ => None,
    };
    if segments.is_some() {
        applied_rules.push(AppliedRule {
            id: "PHASE-SEGMENT".to_string(),
            dimension: "structure".to_string(),
            note: "按阶段 ratio/pace 切分日段并施加节奏权重（段长 ∝ ratio/pace）".to_string(),
        });
    }
    if compiled
        .days
        .iter()
        .filter(|d| d.available)
        .all(|d| d.weight <= 0.0)
    {
        warnings.push(PlanWarning {
            code: "W-ALL-ZERO".to_string(),
            date: None,
            message: "所有可用日权重为 0，已退化为均匀分配".to_string(),
        });
    }
    let front_loaded = shape == SplitShape::FrontLoaded
        || (shape == SplitShape::Auto && unified.objective == Objective::MaximizeThroughput);
    let back_loaded = shape == SplitShape::BackLoaded;
    let slots = shape_slots(
        compiled,
        pattern,
        front_loaded,
        back_loaded,
        segments.as_deref(),
    );

    // ---------- ⑦ A2 可行性判定（不可行仍产出方案 + remedies，E-INFEASIBLE 降级） ----------
    let mut feasibility = check_feasibility(compiled, snapshot, compiled.total_qty);
    if feasibility.status == FeasibilityStatus::Infeasible {
        feasibility.remedies = build_remedies(compiled, snapshot, &feasibility);
    }
    if feasibility.status == FeasibilityStatus::FeasibleWithWarnings {
        warnings.push(PlanWarning {
            code: "W-TIGHT".to_string(),
            date: None,
            message: format!(
                "计划较为紧凑（松弛度 {:.2}），建议留出缓冲",
                feasibility.slack_ratio
            ),
        });
    }

    // ---------- ⑩ A3 加权公平分配（+ A3.1 迟滞：拆解场景 old_placements 为空 → 直通） ----------
    let quotas = allocate_weights(&slots, compiled.total_qty, compiled.max_daily_qty)?;
    let quotas = apply_hysteresis(quotas, &[], &compiled.smoothing);

    // ---------- 草案合并（min_chunk_qty） ----------
    let min_chunk = compiled.min_chunk_qty.unwrap_or(0.0);
    let drafts = merge_drafts(&quotas, min_chunk, compiled.max_daily_qty);
    let sched_tasks: Vec<SchedTask> = drafts
        .iter()
        .enumerate()
        .map(|(k, (_, cost))| SchedTask {
            id: format!("draft-{}", k),
            cost: *cost,
            plan_date: None,
            sort_order: k as i64,
        })
        .collect();

    // ---------- ⑪ A4 DAG 调度（拆解场景无既有依赖 → 直通顺序排布） ----------
    let placements = schedule_dag(
        &sched_tasks,
        &HashMap::new(),
        &quotas,
        snapshot.today,
        &HashSet::new(),
    )?;

    // ---------- ⑫⑬ 序列优化 + 语义生成 ----------
    // 日索引 → 阶段名映射（与 shape_slots 的段切分同源，确定性一致）
    let phase_of_day: Vec<Option<String>> = {
        let mut v: Vec<Option<String>> = vec![None; avail_count];
        if let Some(segs) = &segments {
            for (name, start, len, _) in segs {
                for i in *start..(*start + *len) {
                    if i < v.len() {
                        v[i] = Some(name.clone());
                    }
                }
            }
        }
        v
    };
    let naming_tpl = req
        .semantic
        .naming_template
        .as_deref()
        .unwrap_or(DEFAULT_NAMING_TEMPLATE);
    let desc_tpl = req.semantic.description_template.as_deref();
    let review_tpl = req
        .semantic
        .review_naming_template
        .as_deref()
        .unwrap_or(DEFAULT_REVIEW_TEMPLATE);
    let goal_name = snapshot.root_goal.name.as_str();
    let total = compiled.total_qty;

    // 排序键：(日期, learn<review, 学习序号, 复习间隔)
    let mut tagged: Vec<((NaiveDate, u8, u32, u32), PlanItem)> = Vec::new();
    // 学习任务元信息（供复习注入引用）
    struct LearnMeta {
        date: NaiveDate,
        qty: f64,
        n: u32,
        cum: f64,
        phase: Option<String>,
        scope: String,
    }
    let mut learn_meta: Vec<LearnMeta> = Vec::new();
    let mut tasks_per_date: HashMap<NaiveDate, u32> = HashMap::new();
    let mut cum = 0.0f64;

    for (k, (first_day, cost)) in drafts.iter().enumerate() {
        let draft_id = format!("draft-{}", k);
        let placement = placements
            .iter()
            .find(|p| p.task_id == draft_id)
            .ok_or_else(|| AppError::Internal("A4 调度丢失任务落位".into()))?;
        let n = (k + 1) as u32;
        cum += cost;
        let phase = phase_of_day.get(*first_day).cloned().flatten();
        let scope = outline_scope(
            &req.semantic.outline,
            cum - cost,
            cum,
            total,
            &compiled.unit,
        );
        let ctx = TemplateCtx {
            goal: goal_name,
            phase: phase.as_deref(),
            n,
            scope: scope.clone(),
            cum: fmt_qty(cum),
            total: fmt_qty(total),
            unit: &compiled.unit,
            hours: String::new(),
            date: placement.date.format("%Y-%m-%d").to_string(),
        };
        let item = PlanItem {
            seq: 0,
            name: render_template(naming_tpl, &ctx),
            description: desc_tpl.map(|t| render_template(t, &ctx)),
            plan_date: placement.date,
            plan_qty: *cost,
            unit: compiled.unit.clone(),
            estimated_hours: None,
            task_type: TaskType::Learn,
            phase: phase.clone(),
            stage_hint: phase.clone(),
            depends_on_seq: Vec::new(),
            rationale: if req.explain {
                Some(format!(
                    "{}模式：A3 加权配额 {}{}，A4 顺序落位 {}",
                    pattern_label(pattern),
                    fmt_qty(*cost),
                    compiled.unit,
                    placement.date.format("%Y-%m-%d")
                ))
            } else {
                None
            },
            confidence: None,
        };
        if placement.reason == PlacementReason::OverflowTail {
            warnings.push(PlanWarning {
                code: "W-OVERFLOW-TAIL".to_string(),
                date: Some(placement.date.format("%Y-%m-%d").to_string()),
                message: format!("配额耗尽，任务 {} 已挂视界内最后可用日", placement.task_id),
            });
        }
        *tasks_per_date.entry(placement.date).or_insert(0) += 1;
        learn_meta.push(LearnMeta {
            date: placement.date,
            qty: *cost,
            n,
            cum,
            phase,
            scope,
        });
        tagged.push(((placement.date, 0, n, 0), item));
    }

    // 复习注入（间隔重复：学习日 + interval 天）
    let review_cfg = req
        .structure
        .review
        .as_ref()
        .filter(|r| r.enabled.unwrap_or(false) && !r.intervals.is_empty());
    if review_cfg.is_some() {
        applied_rules.push(AppliedRule {
            id: "REVIEW-INJECT".to_string(),
            dimension: "structure".to_string(),
            note: format!(
                "间隔重复注入（复习量系数 {}）",
                fmt_qty(review_cfg.and_then(|r| r.review_ratio).unwrap_or(DEFAULT_REVIEW_RATIO))
            ),
        });
    }
    let mut review_per_date: HashMap<NaiveDate, u32> = HashMap::new();
    if let Some(review) = review_cfg {
        let ratio = review.review_ratio.unwrap_or(DEFAULT_REVIEW_RATIO);
        for lm in &learn_meta {
            for &interval in &review.intervals {
                let rdate = lm.date + Duration::days(interval as i64);
                let mut reject: Option<String> = None;
                if rdate > compiled.end {
                    reject = Some("超出规划视界".to_string());
                } else {
                    match compiled.days.iter().find(|d| d.date == rdate) {
                        None => reject = Some("不在规划视界内".to_string()),
                        Some(d) if !d.available || d.is_rest => {
                            reject = Some("落点为不可用日（黑名单/非允许星期）".to_string())
                        }
                        Some(_) => {}
                    }
                }
                if reject.is_none() {
                    if let Some(m) = review.max_review_per_day {
                        if review_per_date.get(&rdate).copied().unwrap_or(0) >= m {
                            reject = Some("超出单日复习条数上限".to_string());
                        }
                    }
                }
                if reject.is_none() {
                    if let Some(m) = compiled.max_daily_tasks {
                        if tasks_per_date.get(&rdate).copied().unwrap_or(0) as f64 >= m - QTY_EPS
                        {
                            reject = Some("超出单日任务条数上限".to_string());
                        }
                    }
                }
                match reject {
                    Some(reason) => warnings.push(PlanWarning {
                        code: "W-REVIEW-DROPPED".to_string(),
                        date: Some(rdate.format("%Y-%m-%d").to_string()),
                        message: format!(
                            "复习任务（学习日 {} + {} 天）{}，已丢弃",
                            lm.date.format("%Y-%m-%d"),
                            interval,
                            reason
                        ),
                    }),
                    None => {
                        // 受理：占用当日名额
                        if review.max_review_per_day.is_some() {
                            *review_per_date.entry(rdate).or_insert(0) += 1;
                        }
                        if compiled.max_daily_tasks.is_some() {
                            *tasks_per_date.entry(rdate).or_insert(0) += 1;
                        }
                        let rqty = lm.qty * ratio;
                        let ctx = TemplateCtx {
                            goal: goal_name,
                            phase: lm.phase.as_deref(),
                            n: lm.n,
                            scope: lm.scope.clone(),
                            cum: fmt_qty(lm.cum),
                            total: fmt_qty(total),
                            unit: &compiled.unit,
                            hours: String::new(),
                            date: rdate.format("%Y-%m-%d").to_string(),
                        };
                        let item = PlanItem {
                            seq: 0,
                            name: render_template(review_tpl, &ctx),
                            description: desc_tpl.map(|t| render_template(t, &ctx)),
                            plan_date: rdate,
                            plan_qty: rqty,
                            unit: compiled.unit.clone(),
                            estimated_hours: None,
                            task_type: TaskType::Review,
                            phase: lm.phase.clone(),
                            stage_hint: lm.phase.clone(),
                            depends_on_seq: Vec::new(),
                            rationale: if req.explain {
                                Some(format!(
                                    "间隔重复：学习日 {} + {} 天，复习量 {}{}",
                                    lm.date.format("%Y-%m-%d"),
                                    interval,
                                    fmt_qty(rqty),
                                    compiled.unit
                                ))
                            } else {
                                None
                            },
                            confidence: None,
                        };
                        tagged.push(((rdate, 1, lm.n, interval), item));
                    }
                }
            }
        }
    }

    // 排序（INV-2：日期非递减；同日 learn 先于 review）+ 序号
    tagged.sort_by(|a, b| a.0.cmp(&b.0));
    let mut items: Vec<PlanItem> = tagged.into_iter().map(|(_, it)| it).collect();
    for (i, it) in items.iter_mut().enumerate() {
        it.seq = (i + 1) as u32;
    }

    // ---------- ⑭ 不变式校验（防御性复核，违反记 warning） ----------
    let day_map: HashMap<NaiveDate, (bool, bool)> = compiled
        .days
        .iter()
        .map(|d| (d.date, (d.available, d.is_rest)))
        .collect();
    for it in &items {
        match day_map.get(&it.plan_date) {
            None => warnings.push(PlanWarning {
                code: "W-INV3".to_string(),
                date: Some(it.plan_date.format("%Y-%m-%d").to_string()),
                message: "计划日期不在规划视界内（INV-3）".to_string(),
            }),
            Some((avail, rest)) if !*avail || *rest => warnings.push(PlanWarning {
                code: "W-INV3".to_string(),
                date: Some(it.plan_date.format("%Y-%m-%d").to_string()),
                message: "计划日期落入黑名单或非允许星期（INV-3/INV-4）".to_string(),
            }),
            _ => {}
        }
        let weekday = it.plan_date.weekday().num_days_from_sunday() as u8;
        if !compiled.allowed_weekdays.contains(&weekday) {
            warnings.push(PlanWarning {
                code: "W-INV4".to_string(),
                date: Some(it.plan_date.format("%Y-%m-%d").to_string()),
                message: "计划日期的星期不在允许集合（INV-4）".to_string(),
            });
        }
    }
    if let Some(m) = compiled.max_daily_qty {
        for it in &items {
            if it.task_type == TaskType::Learn && it.plan_qty > m + QTY_EPS {
                warnings.push(PlanWarning {
                    code: "W-OVERFLOW-TAIL".to_string(),
                    date: Some(it.plan_date.format("%Y-%m-%d").to_string()),
                    message: format!(
                        "任务量 {} 超过单日上限 {}（容量不足的防御性兜底，INV-5）",
                        fmt_qty(it.plan_qty),
                        fmt_qty(m)
                    ),
                });
            }
        }
    }
    if min_chunk > QTY_EPS {
        for it in &items {
            if it.task_type == TaskType::Learn
                && it.plan_qty > QTY_EPS
                && it.plan_qty < min_chunk - QTY_EPS
            {
                warnings.push(PlanWarning {
                    code: "W-CHUNK-UNDERSIZED".to_string(),
                    date: Some(it.plan_date.format("%Y-%m-%d").to_string()),
                    message: format!(
                        "任务量 {} 低于最小粒度 {}（尾差未再切分，INV-6）",
                        fmt_qty(it.plan_qty),
                        fmt_qty(min_chunk)
                    ),
                });
            }
        }
    }

    // ---------- ⑮⑯ 组装（守恒显式校验 + plan_hash） ----------
    assemble_plan(
        &req.goal_id,
        req.api_version,
        items,
        compiled.total_qty,
        pattern,
        unified.objective,
        ResolvedHorizon {
            start: compiled.start,
            end: compiled.end,
            buffer_days: compiled.buffer_days,
        },
        compiled.unit.clone(),
        feasibility,
        PlanDiagnostics {
            applied_rules,
            habit_profile: None,
            warnings,
        },
        true,
    )
}

/// legacy 旧语义直通（等价旧 smart_split 的均匀方案）
fn solve_legacy(
    req: &SmartSplitRequest,
    goal: &Goal,
    snapshot: &PlanSnapshot,
    compiled: &CompiledSpec,
) -> AppResult<SplitPlan> {
    let legacy = req
        .legacy
        .as_ref()
        .ok_or_else(|| AppError::Internal("legacy 路径缺少兼容层配置".into()))?;
    let specs = legacy_item_specs(legacy, goal, snapshot.today)?;

    // A2 报告（沿用编译口径；不可行时附 remedies，不阻断产出）
    let mut feasibility = check_feasibility(compiled, snapshot, compiled.total_qty);
    if feasibility.status == FeasibilityStatus::Infeasible {
        feasibility.remedies = build_remedies(compiled, snapshot, &feasibility);
    }
    let mut warnings = Vec::new();
    if feasibility.status == FeasibilityStatus::FeasibleWithWarnings {
        warnings.push(PlanWarning {
            code: "W-TIGHT".to_string(),
            date: None,
            message: format!(
                "计划较为紧凑（松弛度 {:.2}），建议留出缓冲",
                feasibility.slack_ratio
            ),
        });
    }

    let naming_tpl = req
        .semantic
        .naming_template
        .as_deref()
        .unwrap_or(DEFAULT_NAMING_TEMPLATE);
    let desc_tpl = req.semantic.description_template.as_deref();
    let goal_name = goal.name.as_str();

    let mut tagged: Vec<((NaiveDate, u8, u32, u32), PlanItem)> = Vec::new();
    let mut cum = 0.0f64;
    for (i, spec) in specs.iter().enumerate() {
        let n = (i + 1) as u32;
        cum += spec.qty;
        let ctx = TemplateCtx {
            goal: goal_name,
            phase: None,
            n,
            scope: format!(
                "{}~{}{}",
                fmt_qty((cum - spec.qty + 1.0).max(1.0)),
                fmt_qty(cum),
                goal.unit
            ),
            cum: fmt_qty(cum),
            total: fmt_qty(compiled.total_qty),
            unit: &goal.unit,
            hours: spec.estimated_hours.map(fmt_qty).unwrap_or_default(),
            date: spec.date.format("%Y-%m-%d").to_string(),
        };
        tagged.push((
            (spec.date, 0, n, 0),
            PlanItem {
                seq: 0,
                name: render_template(naming_tpl, &ctx),
                description: desc_tpl.map(|t| render_template(t, &ctx)),
                plan_date: spec.date,
                plan_qty: spec.qty,
                unit: goal.unit.clone(),
                estimated_hours: spec.estimated_hours,
                task_type: TaskType::Learn,
                phase: None,
                stage_hint: None,
                depends_on_seq: Vec::new(),
                rationale: None,
                confidence: None,
            },
        ));
    }
    tagged.sort_by(|a, b| a.0.cmp(&b.0));
    let mut items: Vec<PlanItem> = tagged.into_iter().map(|(_, it)| it).collect();
    for (i, it) in items.iter_mut().enumerate() {
        it.seq = (i + 1) as u32;
    }
    let (start, end) = match (items.first(), items.last()) {
        (Some(f), Some(l)) => (f.plan_date, l.plan_date),
        _ => (compiled.start, compiled.end),
    };

    assemble_plan(
        &req.goal_id,
        req.api_version,
        items,
        compiled.total_qty,
        SplitPattern::Even,
        Objective::FinishByDeadline,
        ResolvedHorizon {
            start,
            end,
            buffer_days: 0,
        },
        goal.unit.clone(),
        feasibility,
        PlanDiagnostics {
            applied_rules: vec![AppliedRule {
                id: "LEGACY".to_string(),
                dimension: "compat".to_string(),
                note: format!("旧语义直通（strategy={}）", legacy.strategy),
            }],
            habit_profile: None,
            warnings,
        },
        // legacy 无 INV-1 语义（极小总量可能全部舍入为 0），仅标记不 Err
        false,
    )
}

// ============================================================
// 单元测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    /// SHA-256 标准测试向量（FIPS 180-4 / NIST）
    #[test]
    fn test_sha256_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    fn make_item(seq: u32, date: NaiveDate, qty: f64, name: &str) -> PlanItem {
        PlanItem {
            seq,
            name: name.to_string(),
            description: None,
            plan_date: date,
            plan_qty: qty,
            unit: "页".to_string(),
            estimated_hours: None,
            task_type: TaskType::Learn,
            phase: None,
            stage_hint: None,
            depends_on_seq: Vec::new(),
            rationale: None,
            confidence: None,
        }
    }

    /// 7. plan_hash 稳定性：同一输入两次计算相等；输入变化 hash 变化
    #[test]
    fn test_plan_hash_stability() {
        let items = vec![
            make_item(1, d(2026, 10, 10), 10.0, "t1"),
            make_item(2, d(2026, 10, 11), 10.0, "t2"),
        ];
        let h1 = compute_plan_hash(
            "goal-1",
            2,
            SplitPattern::Even,
            20.0,
            d(2026, 10, 10),
            d(2026, 10, 11),
            &items,
        );
        let h2 = compute_plan_hash(
            "goal-1",
            2,
            SplitPattern::Even,
            20.0,
            d(2026, 10, 10),
            d(2026, 10, 11),
            &items,
        );
        assert_eq!(h1, h2, "同一输入两次计算必须相等");
        assert!(h1.starts_with("sha256:"));
        assert_eq!(h1.len(), "sha256:".len() + 64, "格式 sha256:<64 hex>");

        // 输入变化 → hash 变化
        let mut items2 = items.clone();
        items2[0].plan_qty = 11.0;
        let h3 = compute_plan_hash(
            "goal-1",
            2,
            SplitPattern::Even,
            20.0,
            d(2026, 10, 10),
            d(2026, 10, 11),
            &items2,
        );
        assert_ne!(h1, h3, "任务量变化必须改变摘要");

        let h4 = compute_plan_hash(
            "goal-2", // goal_id 变化
            2,
            SplitPattern::Even,
            20.0,
            d(2026, 10, 10),
            d(2026, 10, 11),
            &items,
        );
        assert_ne!(h1, h4);

        let h5 = compute_plan_hash(
            "goal-1",
            2,
            SplitPattern::Phased, // 模式变化
            20.0,
            d(2026, 10, 10),
            d(2026, 10, 11),
            &items,
        );
        assert_ne!(h1, h5);
    }

    /// 8. 语义模板：占位符渲染与未识别占位符原样保留
    #[test]
    fn test_render_template() {
        let ctx = TemplateCtx {
            goal: "背单词",
            phase: Some("基础期"),
            n: 3,
            scope: "第1~10页".to_string(),
            cum: "30".to_string(),
            total: "300".to_string(),
            unit: "页",
            hours: "2.5".to_string(),
            date: "2026-10-12".to_string(),
        };
        // 全白名单占位符
        assert_eq!(
            render_template("{goal} {phase} {n} {scope} {cum} {total} {unit} {hours} {date}", &ctx),
            "背单词 基础期 3 第1~10页 30 300 页 2.5 2026-10-12"
        );
        // 未识别占位符原样保留
        assert_eq!(
            render_template("{goal} 第{n}天 {unknown} {total}{unit}", &ctx),
            "背单词 第3天 {unknown} 300页"
        );
        // 未闭合花括号原样保留
        assert_eq!(render_template("{goal} {oops", &ctx), "背单词 {oops");
        // 空模板
        assert_eq!(render_template("", &ctx), "");
        // phase 为 None 时渲染空串
        let ctx2 = TemplateCtx {
            goal: "背单词",
            phase: None,
            n: 1,
            scope: "1~10页".to_string(),
            cum: "10".to_string(),
            total: "300".to_string(),
            unit: "页",
            hours: String::new(),
            date: "2026-10-10".to_string(),
        };
        assert_eq!(render_template("{goal}[{phase}]{hours}", &ctx2), "背单词[]");
    }

    /// 模式选择：F-4 映射表 + auto 推断规则
    #[test]
    fn test_resolve_pattern_rules() {
        let phases = vec![
            PhaseSpec { name: "一".into(), ratio: 0.5, pace: None, unit_hint: None },
            PhaseSpec { name: "二".into(), ratio: 0.5, pace: None, unit_hint: None },
        ];
        let review = ReviewSpec {
            enabled: Some(true),
            intervals: vec![1, 2, 4],
            review_ratio: Some(0.5),
            max_review_per_day: None,
        };
        // 显式形状直接映射
        assert_eq!(resolve_pattern(SplitShape::Even, Objective::FinishByDeadline, None, None), SplitPattern::Even);
        assert_eq!(resolve_pattern(SplitShape::FrontLoaded, Objective::FinishByDeadline, None, None), SplitPattern::CapacityBound);
        assert_eq!(resolve_pattern(SplitShape::BackLoaded, Objective::FinishByDeadline, None, None), SplitPattern::CapacityBound);
        assert_eq!(resolve_pattern(SplitShape::Phased, Objective::FinishByDeadline, Some(&phases), None), SplitPattern::Phased);
        assert_eq!(resolve_pattern(SplitShape::SpacedRepetition, Objective::FinishByDeadline, None, Some(&review)), SplitPattern::SpacedRepetition);
        // F-4：milestone → PHASED
        assert_eq!(resolve_pattern(SplitShape::Milestone, Objective::FinishByDeadline, Some(&phases), None), SplitPattern::Phased);
        // auto 推断
        assert_eq!(resolve_pattern(SplitShape::Auto, Objective::FinishByDeadline, None, None), SplitPattern::Even);
        assert_eq!(resolve_pattern(SplitShape::Auto, Objective::FinishByDeadline, None, Some(&review)), SplitPattern::SpacedRepetition);
        assert_eq!(resolve_pattern(SplitShape::Auto, Objective::FinishByDeadline, Some(&phases), None), SplitPattern::Phased);
        assert_eq!(resolve_pattern(SplitShape::Auto, Objective::FinishByDeadline, Some(&phases), Some(&review)), SplitPattern::Hybrid);
        assert_eq!(resolve_pattern(SplitShape::Auto, Objective::MaximizeThroughput, None, None), SplitPattern::CapacityBound);
        assert_eq!(resolve_pattern(SplitShape::Auto, Objective::MinimizeDailyLoad, None, None), SplitPattern::CapacityBound);
        assert_eq!(resolve_pattern(SplitShape::Auto, Objective::SteadyPace, None, None), SplitPattern::Even);
    }

    /// 请求适配：legacy 字段优先映射
    #[test]
    fn test_adapt_request_legacy_mapping() {
        let req = SmartSplitRequest {
            goal_id: "goal-1".into(),
            api_version: 2,
            mode: Default::default(),
            intent: Default::default(),
            constraints: Default::default(),
            structure: Default::default(),
            semantic: Default::default(),
            learning: Default::default(),
            legacy: Some(LegacySpec {
                strategy: "by_capacity".into(),
                total_qty: Some(40.0),
                deadline: Some("2026-11-30".into()),
                daily_capacity: Some(2.0),
                start_date: None,
                end_date: None,
                per_day_qty: None,
            }),
            explain: true,
            dry_run: false,
        };
        let unified = adapt_request(&req);
        assert_eq!(unified.goal_id, "goal-1");
        assert_eq!(unified.scope, Scope::Single);
        assert_eq!(unified.api_version, 2);
        assert_eq!(unified.constraints.daily_capacity, Some(2.0));
        assert_eq!(unified.total_qty_override, Some(40.0));
        assert_eq!(
            unified.horizon.as_ref().and_then(|h| h.end_date.as_deref()),
            Some("2026-11-30")
        );
        // 非 legacy：intent 层透传
        let req2 = SmartSplitRequest {
            legacy: None,
            intent: crate::services::planner::types::IntentSpec {
                total_qty_override: Some(99.0),
                ..Default::default()
            },
            ..req.clone()
        };
        let unified2 = adapt_request(&req2);
        assert_eq!(unified2.total_qty_override, Some(99.0));
        assert_eq!(unified2.constraints.daily_capacity, None);
        assert!(unified2.horizon.is_none());
    }

    fn make_goal(deadline: Option<&str>, total_qty: f64) -> Goal {
        Goal {
            id: "goal-1".into(),
            name: "背单词".into(),
            description: None,
            parent_id: None,
            path: "/goal-1".into(),
            deadline: deadline.map(|s| s.to_string()),
            total_qty,
            unit: "词".into(),
            sort_order: 0,
            created_at: "2026-10-09 10:00:00".into(),
            daily_capacity: None,
        }
    }

    /// legacy by_deadline：floor 均分 + 余数分前几天（等价旧算法）
    #[test]
    fn test_legacy_by_deadline_equivalence() {
        let today = d(2026, 10, 9);
        let goal = make_goal(Some("2026-10-16"), 13.0); // 7 天
        let legacy = LegacySpec {
            strategy: "by_deadline".into(),
            total_qty: None,
            deadline: None,
            daily_capacity: None,
            start_date: None,
            end_date: None,
            per_day_qty: None,
        };
        let specs = legacy_item_specs(&legacy, &goal, today).unwrap();
        // base=1, rem=6 → 前 6 天 2，最后 1 天 1
        let qtys: Vec<f64> = specs.iter().map(|s| s.qty).collect();
        assert_eq!(qtys, vec![2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 1.0]);
        assert_eq!(specs[0].date, today + Duration::days(1));
        assert_eq!(specs[6].date, today + Duration::days(7));
        assert!(specs.iter().all(|s| s.estimated_hours.is_none()));
        // 总量覆盖
        let legacy2 = LegacySpec { total_qty: Some(7.0), ..legacy };
        let specs2 = legacy_item_specs(&legacy2, &goal, today).unwrap();
        assert_eq!(specs2.iter().map(|s| s.qty).sum::<f64>(), 7.0);
    }

    /// legacy by_capacity：ceil(total/cap) 个任务 + 末任务余量 + estimated_hours
    #[test]
    fn test_legacy_by_capacity_equivalence() {
        let today = d(2026, 10, 9);
        let goal = make_goal(Some("2026-10-30"), 5.0);
        let legacy = LegacySpec {
            strategy: "by_capacity".into(),
            total_qty: None,
            deadline: None,
            daily_capacity: Some(2.0),
            start_date: None,
            end_date: None,
            per_day_qty: None,
        };
        let specs = legacy_item_specs(&legacy, &goal, today).unwrap();
        let qtys: Vec<f64> = specs.iter().map(|s| s.qty).collect();
        assert_eq!(qtys, vec![2.0, 2.0, 1.0]);
        assert!(specs.iter().all(|s| s.estimated_hours == Some(2.0)));
        // 天数不足 → 与旧引擎同款错误
        let goal2 = make_goal(Some("2026-10-10"), 5.0); // 仅 1 天
        assert!(legacy_item_specs(&legacy, &goal2, today).is_err());
        // 未设置容量 → 错误
        let legacy3 = LegacySpec { daily_capacity: None, ..legacy };
        assert!(legacy_item_specs(&legacy3, &goal, today).is_err());
    }

    /// legacy by_date_range：per_day_qty / 均分两分支
    #[test]
    fn test_legacy_by_date_range_equivalence() {
        let today = d(2026, 10, 9);
        let goal = make_goal(None, 10.0);
        // 均分分支：5 天 10 → 每天 2
        let legacy = LegacySpec {
            strategy: "by_date_range".into(),
            total_qty: None,
            deadline: None,
            daily_capacity: None,
            start_date: Some("2026-10-10".into()),
            end_date: Some("2026-10-14".into()),
            per_day_qty: None,
        };
        let specs = legacy_item_specs(&legacy, &goal, today).unwrap();
        assert_eq!(specs.len(), 5);
        assert!(specs.iter().all(|s| s.qty == 2.0));
        assert_eq!(specs[0].date, d(2026, 10, 10));
        assert_eq!(specs[4].date, d(2026, 10, 14));
        // per_day_qty 分支：ceil(10/4)=3 → [4, 4, 2]，estimated_hours=Some(4)
        let legacy2 = LegacySpec { per_day_qty: Some(4.0), ..legacy.clone() };
        let specs2 = legacy_item_specs(&legacy2, &goal, today).unwrap();
        let qtys: Vec<f64> = specs2.iter().map(|s| s.qty).collect();
        assert_eq!(qtys, vec![4.0, 4.0, 2.0]);
        assert!(specs2.iter().all(|s| s.estimated_hours == Some(4.0)));
        // 任务数超出范围天数 → 错误
        let legacy3 = LegacySpec { per_day_qty: Some(1.0), ..legacy.clone() };
        assert!(legacy_item_specs(&legacy3, &goal, today).is_err());
        // 缺起始日期 → 错误
        let legacy4 = LegacySpec { start_date: None, ..legacy.clone() };
        assert!(legacy_item_specs(&legacy4, &goal, today).is_err());
        // 未知策略 → 错误
        let legacy5 = LegacySpec { strategy: "by_x".into(), ..legacy };
        assert!(legacy_item_specs(&legacy5, &goal, today).is_err());
    }

    /// 阶段切分：Σ段长 == 日数、工作量占比 == ratio
    #[test]
    fn test_phase_segments() {
        let phases = vec![
            PhaseSpec { name: "基础".into(), ratio: 0.5, pace: Some(1.0), unit_hint: None },
            PhaseSpec { name: "强化".into(), ratio: 0.5, pace: Some(2.0), unit_hint: None },
        ];
        let segs = phase_segments(&phases, 6).unwrap();
        let total_len: usize = segs.iter().map(|(_, _, len, _)| *len).sum();
        assert_eq!(total_len, 6, "段长之和必须等于日数");
        // 段长 ∝ ratio/pace → [4, 2]；工作量 = 段长×pace → [4, 4] → 0.5:0.5
        let work: Vec<f64> = segs.iter().map(|(_, _, len, pace)| *len as f64 * pace).collect();
        assert_eq!(work, vec![4.0, 4.0]);
        assert_eq!(segs[0].1, 0);
        assert_eq!(segs[1].1, 4);
        // 无效：单阶段 / 日数不足
        assert!(phase_segments(&phases[..1], 6).is_none());
        assert!(phase_segments(&phases, 1).is_none());
    }

    /// 草案合并：min_chunk 触发向前合并且守恒
    #[test]
    fn test_merge_drafts() {
        let mk = |(date, qty): (NaiveDate, f64)| DayQuota { date, qty };
        let quotas: Vec<DayQuota> = [
            (d(2026, 10, 10), 1.0),
            (d(2026, 10, 11), 1.0),
            (d(2026, 10, 12), 1.0),
        ]
        .iter()
        .map(|&q| mk(q))
        .collect();
        // min_chunk=2：[1,1] 合并 → (0, 2) + (2, 1)
        let drafts = merge_drafts(&quotas, 2.0, None);
        assert_eq!(drafts, vec![(0, 2.0), (2, 1.0)]);
        // 守恒
        let sum: f64 = drafts.iter().map(|(_, q)| *q).sum();
        assert_eq!(sum, 3.0);
        // min_chunk=0：每天一个草案
        let drafts = merge_drafts(&quotas, 0.0, None);
        assert_eq!(drafts.len(), 3);
        // max_qty 阻断合并：min=2 但 max=1.5 → 不合并
        let drafts = merge_drafts(&quotas, 2.0, Some(1.5));
        assert_eq!(drafts.len(), 3);
        assert_eq!(drafts.iter().map(|(_, q)| *q).sum::<f64>(), 3.0);
    }
}
