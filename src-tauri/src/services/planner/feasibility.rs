//! 目标树智能规划引擎（v2）——A2 可行性判定器
//!
//! 实现重设计方案 §3.3（A2 `check_feasibility`：容量缺口 / 依赖链关键路径
//! 双条件前置拦截，杜绝静默产出荒谬计划 G-05）与拆解重设计方案 §2.5.2
//! （`build_remedies`：不可行时生成 ≥3 个按偏离成本升序的补救方案）。
//!
//! 与伪代码的对应关系：
//! - 行 1~2：`W > C` → `Infeasible`（CapacityShortfall）；
//! - 行 26~31：关键路径 `L_crit > H_avail` → `Infeasible`
//!   （DependencyChainTooLong，仅算链长不做分配的 A4 预演）；
//! - 行 33~35：裕度评估（`slack_ratio` 三档，紧凑档映射
//!   `FeasibleWithWarnings`，对应 W-TIGHT 的 0.15 阈值）；
//! - 互斥域（§3.3 关键点）：`min_intensity > max_intensity_hard(1.5)` 时
//!   排除提高强度类方案（`IncreaseCapacity`），只保留
//!   `ExtendDeadline` / `ReduceScope` / `RelaxConstraint`（接受风险）。
//!
//! 容量口径：`effective_capacity_total = daily_capacity × Σ 有效日 weight`
//! （`daily_capacity` 缺省时 base = 1.0，即每权重单位承载 1 份）。
//!
//! 本模块为纯函数：不访问数据库、不读时钟。

use std::collections::{HashMap, HashSet};

use chrono::{Datelike, NaiveDate, Weekday};
use serde_json::json;

use crate::db::models::Task;
use crate::services::planner::constraints::CompiledSpec;
use crate::services::planner::snapshot::PlanSnapshot;
use crate::services::planner::types::{
    FeasibilityReport, FeasibilityStatus, Remedy, RemedyType,
};

/// 强度系数硬上限（`max_intensity_hard`，互斥域判定阈值）
pub const MAX_INTENSITY_HARD: f64 = 1.5;

/// 紧凑警告阈值（W-TIGHT：0 ≤ slack_ratio < 0.15 → FeasibleWithWarnings）
pub const TIGHT_SLACK_THRESHOLD: f64 = 0.15;

/// 依赖链关键路径长度（含端点节点数）
///
/// 仅统计 `tasks` 集合内部的依赖边；返回 `None` 表示存在依赖环
/// （防御：`set_task_dependency` 已防环，此处兜底判定为不可行）。
/// 实现：DFS + 备忘录，复杂度 O(|V| + |E|)。
pub fn longest_chain_length(
    tasks: &[Task],
    deps: &HashMap<String, Vec<String>>,
) -> Option<usize> {
    let id_set: HashSet<&str> = tasks.iter().map(|t| t.id.as_str()).collect();

    #[derive(Clone, Copy, PartialEq)]
    enum Mark {
        InProgress,
        Done,
    }

    let mut marks: HashMap<&str, Mark> = HashMap::with_capacity(tasks.len());
    let mut memo: HashMap<&str, usize> = HashMap::with_capacity(tasks.len());

    // 返回以 task 为终点的最长链节点数；None = 检测到环
    fn dfs<'a>(
        id: &'a str,
        id_set: &'a HashSet<&'a str>,
        deps: &'a HashMap<String, Vec<String>>,
        marks: &mut HashMap<&'a str, Mark>,
        memo: &mut HashMap<&'a str, usize>,
    ) -> Option<usize> {
        if let Some(&len) = memo.get(id) {
            return Some(len);
        }
        if marks.get(id) == Some(&Mark::InProgress) {
            return None; // 回边：依赖环
        }
        marks.insert(id, Mark::InProgress);
        let mut best: usize = 1; // 自身节点
        if let Some(preds) = deps.get(id) {
            for p in preds {
                if id_set.contains(p.as_str()) {
                    let len = dfs(p, id_set, deps, marks, memo)?;
                    best = best.max(len + 1);
                }
            }
        }
        marks.insert(id, Mark::Done);
        memo.insert(id, best);
        Some(best)
    }

    let mut best: usize = 0;
    for t in tasks {
        let len = dfs(t.id.as_str(), &id_set, deps, &mut marks, &mut memo)?;
        best = best.max(len);
    }
    Some(best)
}

/// A2 可行性判定（伪代码 §3.3）
///
/// - `compiled`：A1 约束编译产物（可用日 / 有效容量 / 缓冲）；
/// - `snapshot`：规划快照（活跃任务与依赖，用于关键路径预演）；
/// - `required`：剩余工作量 W（拆解场景 = total_qty；重排场景 =
///   `snapshot.remaining_qty()`，调用方决定口径）。
///
/// 判定顺序：容量缺口 → 依赖环 / 关键路径 → 裕度三档。
/// `remedies` 由调用方在不可行时经 [`build_remedies`] 填充。
pub fn check_feasibility(
    compiled: &CompiledSpec,
    snapshot: &PlanSnapshot,
    required: f64,
) -> FeasibilityReport {
    let effective_days = compiled.effective_days;
    // 容量基数：daily_capacity 缺省时每权重单位承载 1 份
    let capacity_base = compiled
        .daily_capacity
        .filter(|c| c.is_finite() && *c > 0.0)
        .unwrap_or(1.0);
    let effective_capacity_total = capacity_base * compiled.total_capacity;

    // 单日峰值 / 平均负载（理想化加权分配口径：峰值落在最大权重日）
    let avail_weights: Vec<f64> = compiled
        .days
        .iter()
        .filter(|d| d.available && !d.is_rest)
        .map(|d| d.weight.max(0.0))
        .collect();
    let max_weight = avail_weights.iter().cloned().fold(0.0, f64::max);
    let weight_sum: f64 = avail_weights.iter().sum();
    let avg_day_load = if effective_days > 0 {
        required / effective_days as f64
    } else {
        0.0
    };
    let peak_day_load = if weight_sum > 0.0 {
        required * max_weight / weight_sum
    } else {
        0.0
    };

    // 关键路径预演（A4 只算长度）
    let chain_len = longest_chain_length(&snapshot.active_tasks, &snapshot.deps);

    // 状态判定
    let mut status = FeasibilityStatus::Feasible;
    if !required.is_finite() || required < 0.0 {
        status = FeasibilityStatus::Infeasible;
    } else if required > effective_capacity_total + 1e-9 {
        // 行 2：容量缺口（CapacityShortfall）
        status = FeasibilityStatus::Infeasible;
    } else if chain_len.is_none() {
        // 依赖环（防御，理论不可达）
        status = FeasibilityStatus::Infeasible;
    } else if let Some(l_crit) = chain_len {
        // 行 28：关键路径超过可用天数（DependencyChainTooLong）
        if l_crit > effective_days as usize {
            status = FeasibilityStatus::Infeasible;
        }
    }

    // 行 34：松弛度 (capacity - required) / required（types.rs 契约口径）
    let slack_ratio = if required > 1e-9 {
        (effective_capacity_total - required) / required
    } else {
        1.0 // 无剩余工作量：充分松弛
    };

    // 紧凑档 → 可行但有警告（W-TIGHT：0 ≤ slack < 0.15）
    if status == FeasibilityStatus::Feasible && slack_ratio < TIGHT_SLACK_THRESHOLD {
        status = FeasibilityStatus::FeasibleWithWarnings;
    }

    FeasibilityReport {
        status,
        effective_days,
        effective_capacity_total,
        required_capacity: required,
        slack_ratio,
        peak_day_load,
        avg_day_load,
        remedies: Vec::new(),
    }
}

/// 生成补救方案（不可行时 ≥3 个，按 cost 升序）
///
/// 方案族与量化后果（对应拆解重设计方案 §2.5.2 与整合方案 §4.13）：
/// 1. `ExtendDeadline`：需要的最小延期天数 = ceil(缺口 / 日均可用容量)；
/// 2. `IncreaseCapacity`（RaiseIntensity）：保持视界不变所需容量倍率，
///    受互斥域约束（倍率 > 1.5 时排除）；
/// 3. `ReduceScope`（TrimScope）：视界不变时可承载的最大工作量；
/// 4. `RelaxConstraint`（AcceptRisk）：接受风险维持原参数；
/// 5. `AllowWeekends`：周末系数 < 1 且存在可用周末日时开放周末。
///
/// 依赖链过长场景（容量充足）：`ExtendDeadline` 按链长差延期，
/// `IncreaseCapacity` 无意义自动排除。
pub fn build_remedies(
    compiled: &CompiledSpec,
    snapshot: &PlanSnapshot,
    report: &FeasibilityReport,
) -> Vec<Remedy> {
    let w = report.required_capacity;
    let c = report.effective_capacity_total;
    let deficit = (w - c).max(0.0);
    let avg_daily = if report.effective_days > 0 {
        c / report.effective_days as f64
    } else {
        0.0
    };
    let chain_len = longest_chain_length(&snapshot.active_tasks, &snapshot.deps);
    let chain_over = chain_len.filter(|l| *l > report.effective_days as usize);

    let mut remedies: Vec<Remedy> = Vec::new();

    // ---------- 1. ExtendDeadline ----------
    let extend_days: i64 = if let Some(l) = chain_over {
        // 依赖链过长：延期天数 = 链长 - 可用天数（至少 1 天）
        ((l as i64) - report.effective_days as i64).max(1)
    } else if avg_daily > 0.0 {
        // 容量缺口：ceil(deficit / 日均可用)
        (deficit / avg_daily).ceil() as i64
    } else {
        1
    }
    .max(1);
    let new_end: NaiveDate = compiled.end + chrono::Duration::days(extend_days);
    let new_capacity = c + extend_days as f64 * avg_daily;
    let extend_slack = if w > 1e-9 {
        (new_capacity - w) / w
    } else {
        1.0
    };
    remedies.push(Remedy {
        remedy_type: RemedyType::ExtendDeadline,
        label: format!("延长截止日期 {} 天（至 {}）", extend_days, new_end.format("%Y-%m-%d")),
        delta: json!({
            "extend_days": extend_days,
            "new_deadline": new_end.format("%Y-%m-%d").to_string(),
        }),
        resulting_slack_ratio: extend_slack,
        cost: 0.20,
        tradeoff: format!(
            "保持每日节奏不变，完成时间推迟 {} 天；整体视界拉长后松弛度约 {:.2}",
            extend_days, extend_slack
        ),
    });

    // ---------- 2. IncreaseCapacity（RaiseIntensity，互斥域） ----------
    // min_intensity = intensity × W / C（当前 intensity 基准折算为 1.0 时即 W/C）
    let min_intensity = if c > 1e-9 { w / c } else { f64::INFINITY };
    let intensity_excluded = chain_over.is_some() || min_intensity > MAX_INTENSITY_HARD;
    if !intensity_excluded {
        // 保持视界不变所需的每日容量（C' = W → slack = 0）
        let weight_sum = compiled.total_capacity.max(1e-9);
        let new_daily_capacity = w / weight_sum;
        remedies.push(Remedy {
            remedy_type: RemedyType::IncreaseCapacity,
            label: format!("提高每日容量至 {:.1}", new_daily_capacity),
            delta: json!({
                "daily_capacity": new_daily_capacity,
                "min_intensity": min_intensity,
            }),
            resulting_slack_ratio: 0.0,
            cost: 0.35,
            tradeoff: format!(
                "每日负担变为原来的 {:.1} 倍（强度系数 {}），视界不变",
                min_intensity, min_intensity
            ),
        });
    }

    // ---------- 2b. AllowWeekends（周末系数 < 1 时可用） ----------
    if compiled.weekend_factor < 1.0 - 1e-9 {
        // 按星期判定周末日（与 build_day_slots 的 is_weekend 同源：周六/周日），
        // 不依赖 weight 数值相等——Phase 3/4 画像注入后 weight≠weekend_factor 仍成立
        let weekend_count = compiled
            .days
            .iter()
            .filter(|d| {
                d.available
                    && !d.is_rest
                    && matches!(d.date.weekday(), Weekday::Sat | Weekday::Sun)
            })
            .count();
        if weekend_count > 0 {
            let capacity_base = compiled
                .daily_capacity
                .filter(|c| c.is_finite() && *c > 0.0)
                .unwrap_or(1.0);
            // 周末系数提到 1.0 的增量容量：可用周末日数 × (1 - factor) × 容量基数
            //（factor=0 时增益最大，公式仍成立且无 1/factor 发散）
            let gain = capacity_base * weekend_count as f64 * (1.0 - compiled.weekend_factor);
            let new_capacity = c + gain;
            let slack = if w > 1e-9 {
                (new_capacity - w) / w
            } else {
                1.0
            };
            remedies.push(Remedy {
                remedy_type: RemedyType::AllowWeekends,
                label: format!(
                    "开放周末（周末容量系数 {} → 1.0）",
                    compiled.weekend_factor
                ),
                delta: json!({ "weekend_factor": 1.0 }),
                resulting_slack_ratio: slack,
                cost: 0.28,
                tradeoff: format!(
                    "周末日恢复全量承载，总容量约增加 {:.1}（+{:.0}%）",
                    gain,
                    if c > 1e-9 { gain / c * 100.0 } else { 0.0 }
                ),
            });
        }
    }

    // ---------- 3. ReduceScope（TrimScope） ----------
    let max_workload = c;
    remedies.push(Remedy {
        remedy_type: RemedyType::ReduceScope,
        label: format!("缩减范围：总量减至 {:.1} 以内", max_workload),
        delta: json!({
            "max_workload": max_workload,
            "suggested_cut": deficit,
        }),
        resulting_slack_ratio: 0.0,
        cost: 0.55,
        tradeoff: format!(
            "需要裁掉 {:.1} 的工作量（约 {:.0}%），视界与节奏保持不变",
            deficit,
            if w > 1e-9 { deficit / w * 100.0 } else { 0.0 }
        ),
    });

    // ---------- 4. RelaxConstraint（AcceptRisk，兜底凑满 ≥3） ----------
    remedies.push(Remedy {
        remedy_type: RemedyType::RelaxConstraint,
        label: "接受风险：维持当前参数强行排布".to_string(),
        delta: json!({ "accept_risk": true }),
        resulting_slack_ratio: report.slack_ratio,
        cost: 0.85,
        tradeoff: "计划将超载（部分日期超出容量），执行时可能大面积逾期".to_string(),
    });

    // 按 cost 升序（稳定排序保证同成本时方案顺序确定）
    remedies.sort_by(|a, b| a.cost.partial_cmp(&b.cost).unwrap_or(std::cmp::Ordering::Equal));
    remedies
}

// ============================================================
// 单元测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::Goal;
    use crate::services::planner::snapshot::DaySlot;
    use crate::services::planner::types::SmoothingPolicy;
    use std::collections::BTreeSet;

    /// 构造 Goal（models.rs 字段的最小填充）
    fn make_goal() -> Goal {
        Goal {
            id: "goal-1".into(),
            name: "背单词".into(),
            description: None,
            parent_id: None,
            path: "/goal-1".into(),
            deadline: Some("2026-11-30".into()),
            total_qty: 300.0,
            unit: "词".into(),
            sort_order: 0,
            created_at: "2026-10-09 10:00:00".into(),
            daily_capacity: None,
        }
    }

    /// 构造 CompiledSpec（仅填充 A2 消费的字段）
    fn make_compiled(days: Vec<DaySlot>, daily_capacity: Option<f64>) -> CompiledSpec {
        let effective_days = days.iter().filter(|d| d.available).count() as u32;
        let total_capacity: f64 = days.iter().filter(|d| d.available).map(|d| d.weight).sum();
        CompiledSpec {
            start: NaiveDate::from_ymd_opt(2026, 10, 10).unwrap(),
            end: NaiveDate::from_ymd_opt(2026, 10, 19).unwrap(),
            buffer_days: 0,
            total_qty: 300.0,
            unit: "词".into(),
            daily_capacity,
            max_daily_qty: None,
            max_daily_hours: None,
            max_daily_tasks: None,
            min_chunk_qty: None,
            global_daily_task_budget: 6,
            blackout: BTreeSet::new(),
            allowed_weekdays: (0..=6).collect(),
            weekend_factor: 1.0,
            adapt_strength: 1.0,
            smoothing: SmoothingPolicy::default(),
            days,
            effective_days,
            total_capacity,
        }
    }

    fn make_snapshot(tasks: Vec<Task>, deps: HashMap<String, Vec<String>>) -> PlanSnapshot {
        PlanSnapshot {
            today: NaiveDate::from_ymd_opt(2026, 10, 9).unwrap(),
            root_goal: make_goal(),
            goals: vec![make_goal()],
            days: vec![],
            active_tasks: tasks,
            deps,
            external_load: HashMap::new(),
        }
    }

    fn make_task(id: &str) -> Task {
        Task {
            id: id.into(),
            goal_id: "goal-1".into(),
            description: None,
            stage_id: None,
            parent_id: None,
            path: format!("/goal-1/{}", id),
            name: format!("task-{}", id),
            plan_date: None,
            overdue_date: None,
            plan_qty: 10.0,
            actual_qty: 0.0,
            unit: "词".into(),
            status: "pending".into(),
            is_manual: 0,
            source: "auto".into(),
            sort_order: 0,
            created_at: "2026-10-09 10:00:00".into(),
            estimated_hours: None,
            repeat_series_id: None,
        }
    }

    fn slots(start: NaiveDate, n_days: usize, weight: f64) -> Vec<DaySlot> {
        (0..n_days)
            .map(|i| DaySlot {
                date: start + chrono::Duration::days(i as i64),
                available: true,
                weight,
                is_rest: false,
            })
            .collect()
    }

    /// 容量充足 → Feasible
    #[test]
    fn test_feasible() {
        let start = NaiveDate::from_ymd_opt(2026, 10, 10).unwrap();
        let compiled = make_compiled(slots(start, 10, 1.0), None);
        let snapshot = make_snapshot(vec![], HashMap::new());
        // W = 5 ≤ C = 10
        let report = check_feasibility(&compiled, &snapshot, 5.0);
        assert_eq!(report.status, FeasibilityStatus::Feasible);
        assert_eq!(report.effective_days, 10);
        assert!((report.effective_capacity_total - 10.0).abs() < 1e-9);
        assert!((report.slack_ratio - 1.0).abs() < 1e-9);
        assert!((report.avg_day_load - 0.5).abs() < 1e-9);
    }

    /// W > C → Infeasible，slack 为负
    #[test]
    fn test_capacity_shortfall_infeasible() {
        let start = NaiveDate::from_ymd_opt(2026, 10, 10).unwrap();
        let compiled = make_compiled(slots(start, 5, 1.0), None);
        let snapshot = make_snapshot(vec![], HashMap::new());
        let report = check_feasibility(&compiled, &snapshot, 40.0);
        assert_eq!(report.status, FeasibilityStatus::Infeasible);
        assert!((report.slack_ratio + 0.875).abs() < 1e-9); // (5-40)/40 = -0.875
    }

    /// 紧凑档（0 ≤ slack < 0.15）→ FeasibleWithWarnings
    #[test]
    fn test_tight_feasible_with_warnings() {
        let start = NaiveDate::from_ymd_opt(2026, 10, 10).unwrap();
        let compiled = make_compiled(slots(start, 10, 1.0), None);
        let snapshot = make_snapshot(vec![], HashMap::new());
        // slack = (10-9.5)/9.5 ≈ 0.0526 < 0.15
        let report = check_feasibility(&compiled, &snapshot, 9.5);
        assert_eq!(report.status, FeasibilityStatus::FeasibleWithWarnings);
    }

    /// daily_capacity 存在时容量按容量基数放大
    #[test]
    fn test_capacity_base_scaling() {
        let start = NaiveDate::from_ymd_opt(2026, 10, 10).unwrap();
        let compiled = make_compiled(slots(start, 10, 1.0), Some(2.5));
        let snapshot = make_snapshot(vec![], HashMap::new());
        let report = check_feasibility(&compiled, &snapshot, 20.0);
        // C = 2.5 × 10 = 25 ≥ 20 → 可行
        assert_eq!(report.status, FeasibilityStatus::Feasible);
        assert!((report.effective_capacity_total - 25.0).abs() < 1e-9);
    }

    /// 依赖链超过可用天数 → Infeasible
    #[test]
    fn test_dependency_chain_too_long() {
        let start = NaiveDate::from_ymd_opt(2026, 10, 10).unwrap();
        // 4 天可用
        let compiled = make_compiled(slots(start, 4, 1.0), None);
        // A → B → C → D → E 链长 5 > 4
        let deps: HashMap<String, Vec<String>> = [
            ("b".to_string(), vec!["a".to_string()]),
            ("c".to_string(), vec!["b".to_string()]),
            ("d".to_string(), vec!["c".to_string()]),
            ("e".to_string(), vec!["d".to_string()]),
        ]
        .into_iter()
        .collect();
        // W = 5×10 = 50 ≤ C？容量 4×1=4 < 50 —— 为隔离链长因素，用小任务量
        let mut small_tasks: Vec<Task> = ["a", "b", "c", "d", "e"]
            .iter()
            .map(|i| make_task(i))
            .collect();
        for t in small_tasks.iter_mut() {
            t.plan_qty = 0.5; // W = 2.5 ≤ 4
        }
        let snapshot = make_snapshot(small_tasks, deps);
        let report = check_feasibility(&compiled, &snapshot, 2.5);
        assert_eq!(report.status, FeasibilityStatus::Infeasible);
    }

    /// 依赖环 → Infeasible（防御）
    #[test]
    fn test_dependency_cycle_infeasible() {
        let start = NaiveDate::from_ymd_opt(2026, 10, 10).unwrap();
        let compiled = make_compiled(slots(start, 10, 1.0), None);
        let tasks: Vec<Task> = ["a", "b"].iter().map(|i| make_task(i)).collect();
        let deps: HashMap<String, Vec<String>> = [
            ("a".to_string(), vec!["b".to_string()]),
            ("b".to_string(), vec!["a".to_string()]),
        ]
        .into_iter()
        .collect();
        let snapshot = make_snapshot(tasks, deps);
        let report = check_feasibility(&compiled, &snapshot, 1.0);
        assert_eq!(report.status, FeasibilityStatus::Infeasible);
    }

    /// 补救方案：≥3 个、按 cost 升序、互斥域生效
    #[test]
    fn test_remedies_sorted_and_mutex_domain() {
        let start = NaiveDate::from_ymd_opt(2026, 10, 10).unwrap();
        let snapshot = make_snapshot(vec![], HashMap::new());

        // 场景 1：轻度缺口（W/C = 1.2 ≤ 1.5）→ 含 IncreaseCapacity
        let compiled = make_compiled(slots(start, 10, 1.0), None);
        let report = check_feasibility(&compiled, &snapshot, 12.0);
        assert_eq!(report.status, FeasibilityStatus::Infeasible);
        let remedies = build_remedies(&compiled, &snapshot, &report);
        assert!(remedies.len() >= 3, "补救方案必须 ≥3 个");
        // cost 升序
        for w in remedies.windows(2) {
            assert!(w[0].cost <= w[1].cost + 1e-12);
        }
        assert!(
            remedies.iter().any(|r| r.remedy_type == RemedyType::IncreaseCapacity),
            "轻度缺口应提供提高容量方案"
        );

        // 场景 2：重度缺口（W/C = 3.0 > 1.5）→ 互斥域排除 IncreaseCapacity
        let compiled = make_compiled(slots(start, 10, 1.0), None);
        let report = check_feasibility(&compiled, &snapshot, 30.0);
        let remedies = build_remedies(&compiled, &snapshot, &report);
        assert!(remedies.len() >= 3);
        assert!(
            !remedies.iter().any(|r| r.remedy_type == RemedyType::IncreaseCapacity),
            "互斥域触发（min_intensity=3.0 > 1.5）时必须排除提高强度方案"
        );
        // 必含 ExtendDeadline 与 ReduceScope
        assert!(remedies.iter().any(|r| r.remedy_type == RemedyType::ExtendDeadline));
        assert!(remedies.iter().any(|r| r.remedy_type == RemedyType::ReduceScope));
    }

    /// AllowWeekends 补救：按星期识别周末日；factor=0 时方案不被静默丢弃
    #[test]
    fn test_allow_weekends_remedy() {
        let start = NaiveDate::from_ymd_opt(2026, 10, 10).unwrap(); // 周六
        // 7 天（周六~周五）：周末 2 天（10-10 周六、10-11 周日）
        let mut compiled = make_compiled(slots(start, 7, 1.0), None);
        compiled.weekend_factor = 0.5;
        let snapshot = make_snapshot(vec![], HashMap::new());
        let report = check_feasibility(&compiled, &snapshot, 8.0);
        assert_eq!(report.status, FeasibilityStatus::Infeasible);
        let remedies = build_remedies(&compiled, &snapshot, &report);
        let aw = remedies
            .iter()
            .find(|r| r.remedy_type == RemedyType::AllowWeekends)
            .expect("周末系数 < 1 时应提供开放周末方案");
        // 方案 delta 指示周末系数提至 1.0；增量容量 = 2 个周末日 × (1 - 0.5) × 1 = 1.0，
        // 开放后总容量 7 + 1 = 8 ≥ W = 8 → resulting_slack_ratio ≥ 0
        assert_eq!(aw.delta.get("weekend_factor"), Some(&serde_json::json!(1.0)));
        assert!(aw.resulting_slack_ratio >= -1e-9);

        // factor = 0：增益最大场景，方案必须存在（旧实现会静默丢失）
        let mut compiled0 = make_compiled(slots(start, 7, 1.0), None);
        compiled0.weekend_factor = 0.0;
        let report0 = check_feasibility(&compiled0, &snapshot, 8.0);
        assert_eq!(report0.status, FeasibilityStatus::Infeasible);
        let remedies0 = build_remedies(&compiled0, &snapshot, &report0);
        assert!(
            remedies0
                .iter()
                .any(|r| r.remedy_type == RemedyType::AllowWeekends),
            "weekend_factor=0 时开放周末方案不应被静默丢弃"
        );
        // gain = 2 × (1 - 0) × 1 = 2.0，无 NaN
        let aw0 = remedies0
            .iter()
            .find(|r| r.remedy_type == RemedyType::AllowWeekends)
            .unwrap();
        assert!(aw0.resulting_slack_ratio.is_finite());
    }

    /// longest_chain_length：链长与环检测
    #[test]
    fn test_longest_chain() {
        let tasks: Vec<Task> = ["a", "b", "c", "d"].iter().map(|i| make_task(i)).collect();
        // a→b→c，d 独立：最长链 3
        let deps: HashMap<String, Vec<String>> = [
            ("b".to_string(), vec!["a".to_string()]),
            ("c".to_string(), vec!["b".to_string()]),
        ]
        .into_iter()
        .collect();
        assert_eq!(longest_chain_length(&tasks, &deps), Some(3));
        // 集合外依赖不计入
        let deps_external: HashMap<String, Vec<String>> =
            [("b".to_string(), vec!["zzz".to_string()])]
                .into_iter()
                .collect();
        assert_eq!(longest_chain_length(&tasks, &deps_external), Some(1));
        // 空任务集
        assert_eq!(longest_chain_length(&[], &HashMap::new()), Some(0));
    }
}
