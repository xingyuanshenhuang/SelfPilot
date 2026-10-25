//! 多轮修复循环（对应《整合设计方案》§2.5 第 ⑭ 阶段与 INV-1~INV-8）
//!
//! 校验 → 候选修复 → 重算 → 再校验，最多 [`MAX_REPAIR_ROUNDS`] 轮：
//!
//! | 不变式 | 含义 | 修复动作 |
//! | --- | --- | --- |
//! | INV-1 | 学习任务数量守恒 | 把差额补到末尾学习任务（含尾差例外） |
//! | INV-2 | 日期非递减、同日学习先于复习 | 稳定排序后重编 seq |
//! | INV-3 | 不落入黑名单/视界外 | 迁移到最近的可排期日（A4 重排） |
//! | INV-4 | 星期在允许集合 | 同上 |
//! | INV-5 | 单任务量 ≤ 单日上限 | 拆分溢出到后续可排期日（A3 再分配） |
//! | INV-6 | 学习任务量 ≥ 最小粒度 | 合并到相邻学习任务（A3 再分配） |
//! | INV-7 | 单日任务条数 ≤ 上限 | 超出的复习/末尾任务后移到下一天（A4 重排） |
//! | INV-8 | 依赖序号 < 自身序号 | 重建学习任务线性链 / 清理悬空依赖 |
//!
//! 修复按固定优先级执行（日期 → 配额上限 → 最小粒度 → 守恒），每轮后重新
//! 校验；违规数未严格下降或已无修复动作时提前降级，返回最接近的方案并附
//! `W-REPAIR-LIMIT` 警告（E-20）。修复只调整违反项，不整体重算，因此不会
//! 破坏已满足的约束；全过程确定性（同一输入同一输出）。

use std::collections::{HashMap, HashSet};

use chrono::{Datelike, NaiveDate};

use crate::services::planner::constraints::CompiledSpec;
use crate::services::planner::types::{PlanItem, PlanWarning, TaskType};

/// 修复轮次上限（E-20：5 轮后返回最接近解 + W-REPAIR-LIMIT）
pub const MAX_REPAIR_ROUNDS: u32 = 5;
/// 守恒校验容差
const CONSERVATION_EPS: f64 = 1e-6;
/// 数量零保护阈值
const QTY_EPS: f64 = 1e-9;
/// 单次修复输出的逐项警告上限
const MAX_ITEM_WARNINGS: usize = 20;

/// 契约不变式编号
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum InvariantId {
    /// INV-1 数量守恒
    Conservation,
    /// INV-2 日期顺序
    Order,
    /// INV-3 黑名单/视界
    Blackout,
    /// INV-4 允许星期
    Weekday,
    /// INV-5 单日计划量上限
    MaxDailyQty,
    /// INV-6 最小粒度
    MinChunk,
    /// INV-7 单日任务条数上限
    MaxDailyTasks,
    /// INV-8 依赖下界
    Dependency,
}

impl InvariantId {
    /// 契约编号
    pub fn code(self) -> &'static str {
        match self {
            InvariantId::Conservation => "INV-1",
            InvariantId::Order => "INV-2",
            InvariantId::Blackout => "INV-3",
            InvariantId::Weekday => "INV-4",
            InvariantId::MaxDailyQty => "INV-5",
            InvariantId::MinChunk => "INV-6",
            InvariantId::MaxDailyTasks => "INV-7",
            InvariantId::Dependency => "INV-8",
        }
    }

    /// 残留违规对应的警告码（与既有 UI 警告码保持兼容）
    pub fn warning_code(self) -> &'static str {
        match self {
            InvariantId::Conservation => "W-INV1",
            InvariantId::Order => "W-INV2",
            InvariantId::Blackout => "W-INV3",
            InvariantId::Weekday => "W-INV4",
            InvariantId::MaxDailyQty => "W-OVERFLOW-TAIL",
            InvariantId::MinChunk => "W-CHUNK-UNDERSIZED",
            InvariantId::MaxDailyTasks => "W-INV7",
            InvariantId::Dependency => "W-INV8",
        }
    }
}

/// 单条不变式违规
#[derive(Debug, Clone)]
pub struct Violation {
    /// 违反的不变式
    pub invariant: InvariantId,
    /// 关联任务序号（全局性违规为 0）
    pub seq: u32,
    /// 说明（含具体日期/数值）
    pub message: String,
}

/// 修复循环结果
#[derive(Debug, Clone)]
pub struct RepairOutcome {
    /// 修复后的任务序列（已排序并重编 seq）
    pub items: Vec<PlanItem>,
    /// 实际执行的修复轮数（0 = 初始校验即通过）
    pub rounds: u32,
    /// 是否全部满足
    pub converged: bool,
    /// 收敛时为空；未收敛时为残留违规（供调用方二次处置/测试断言）
    #[allow(dead_code)]
    pub remaining: Vec<Violation>,
    /// 未收敛时输出的警告列表
    pub warnings: Vec<PlanWarning>,
}

/// 任务类型排序权重（同日 learn 先于 review）
fn type_rank(task_type: TaskType) -> u8 {
    match task_type {
        TaskType::Learn => 0,
        TaskType::Review => 1,
        TaskType::Practice => 2,
        TaskType::Milestone => 3,
    }
}

/// 日期是否可排期（available && !is_rest && weight > 0）
fn is_schedulable(spec: &CompiledSpec, date: NaiveDate) -> bool {
    spec.days
        .iter()
        .find(|d| d.date == date)
        .map(|d| d.available && !d.is_rest && d.weight > 0.0)
        .unwrap_or(false)
}

/// 全部可排期日（升序）
fn schedulable_dates(spec: &CompiledSpec) -> Vec<NaiveDate> {
    spec.days
        .iter()
        .filter(|d| d.available && !d.is_rest && d.weight > 0.0)
        .map(|d| d.date)
        .collect()
}

/// 严格晚于 `after` 的最近可排期日
fn next_schedulable(dates: &[NaiveDate], after: NaiveDate) -> Option<NaiveDate> {
    dates.iter().find(|d| **d > after).copied()
}

/// 严格早于 `before` 的最近可排期日
fn prev_schedulable(dates: &[NaiveDate], before: NaiveDate) -> Option<NaiveDate> {
    dates.iter().rev().find(|d| **d < before).copied()
}

/// 规范化：稳定排序（日期 → 同日任务类型 → 原 seq）后重编 seq，并维护依赖链
pub fn normalize(items: &mut Vec<PlanItem>, dependency_chain: bool) {
    items.sort_by(|a, b| {
        (a.plan_date, type_rank(a.task_type), a.seq)
            .cmp(&(b.plan_date, type_rank(b.task_type), b.seq))
    });
    for (i, it) in items.iter_mut().enumerate() {
        it.seq = (i + 1) as u32;
    }
    if dependency_chain {
        let mut prev_learn: Option<u32> = None;
        for it in items.iter_mut() {
            if it.task_type == TaskType::Learn {
                it.depends_on_seq = prev_learn.into_iter().collect();
                prev_learn = Some(it.seq);
            }
        }
    } else {
        let valid: HashSet<u32> = items.iter().map(|it| it.seq).collect();
        for it in items.iter_mut() {
            it.depends_on_seq
                .retain(|d| *d > 0 && *d < it.seq && valid.contains(d));
        }
    }
}

/// 校验 INV-1~INV-8
pub fn validate_invariants(items: &[PlanItem], spec: &CompiledSpec) -> Vec<Violation> {
    let mut violations = Vec::new();

    // INV-2：seq 连续 + 日期非递减 + 同日 learn 先于 review
    for (idx, it) in items.iter().enumerate() {
        if it.seq != (idx + 1) as u32 {
            violations.push(Violation {
                invariant: InvariantId::Order,
                seq: it.seq,
                message: format!("任务序号 {} 与位置 {} 不一致（INV-2）", it.seq, idx + 1),
            });
        }
        if idx > 0 {
            let prev = &items[idx - 1];
            if it.plan_date < prev.plan_date {
                violations.push(Violation {
                    invariant: InvariantId::Order,
                    seq: it.seq,
                    message: "计划日期倒退（INV-2）".to_string(),
                });
            } else if it.plan_date == prev.plan_date
                && type_rank(prev.task_type) > type_rank(it.task_type)
            {
                violations.push(Violation {
                    invariant: InvariantId::Order,
                    seq: it.seq,
                    message: "同日学习任务未排在复习之前（INV-2）".to_string(),
                });
            }
        }
    }

    // INV-1：学习任务数量守恒（排除 review）
    let learn_sum: f64 = items
        .iter()
        .filter(|it| it.task_type == TaskType::Learn)
        .map(|it| it.plan_qty)
        .sum();
    if (learn_sum - spec.total_qty).abs() > CONSERVATION_EPS {
        violations.push(Violation {
            invariant: InvariantId::Conservation,
            seq: 0,
            message: format!(
                "学习任务合计 {:.6} ≠ 总量 {:.6}（INV-1）",
                learn_sum, spec.total_qty
            ),
        });
    }

    // INV-7：单日任务条数
    if let Some(max_tasks) = spec.max_daily_tasks {
        let max = max_tasks.floor().max(0.0) as usize;
        let mut counts: HashMap<NaiveDate, usize> = HashMap::new();
        for it in items {
            *counts.entry(it.plan_date).or_insert(0) += 1;
        }
        for (date, count) in counts {
            if count > max {
                violations.push(Violation {
                    invariant: InvariantId::MaxDailyTasks,
                    seq: 0,
                    message: format!(
                        "{} 任务条数 {} 超过上限 {}（INV-7）",
                        date.format("%Y-%m-%d"),
                        count,
                        max
                    ),
                });
            }
        }
    }

    let seq_set: HashSet<u32> = items.iter().map(|it| it.seq).collect();
    for it in items {
        let date_text = it.plan_date.format("%Y-%m-%d").to_string();

        // INV-3：黑名单 / 视界外 / 缓冲日
        if !is_schedulable(spec, it.plan_date) {
            violations.push(Violation {
                invariant: InvariantId::Blackout,
                seq: it.seq,
                message: format!("{} 不在可排期日（黑名单/视界外/缓冲日，INV-3）", date_text),
            });
        } else {
            // INV-4：允许星期（仅在日期本身可排期时检查，避免重复告警）
            let weekday = it.plan_date.weekday().num_days_from_sunday() as u8;
            if !spec.allowed_weekdays.contains(&weekday) {
                violations.push(Violation {
                    invariant: InvariantId::Weekday,
                    seq: it.seq,
                    message: format!("{} 的星期不在允许集合（INV-4）", date_text),
                });
            }
        }

        // INV-5：单任务量不超过单日上限
        if let Some(max_qty) = spec.max_daily_qty {
            if it.plan_qty > max_qty + QTY_EPS {
                violations.push(Violation {
                    invariant: InvariantId::MaxDailyQty,
                    seq: it.seq,
                    message: format!(
                        "{} 任务量 {:.2} 超过单日上限 {:.2}（INV-5）",
                        date_text, it.plan_qty, max_qty
                    ),
                });
            }
        }

        // INV-6：学习任务量不低于最小粒度
        if let Some(min_chunk) = spec.min_chunk_qty {
            if it.task_type == TaskType::Learn
                && it.plan_qty > QTY_EPS
                && it.plan_qty < min_chunk - QTY_EPS
            {
                violations.push(Violation {
                    invariant: InvariantId::MinChunk,
                    seq: it.seq,
                    message: format!(
                        "{} 任务量 {:.2} 低于最小粒度 {:.2}（INV-6）",
                        date_text, it.plan_qty, min_chunk
                    ),
                });
            }
        }

        // INV-8：依赖序号严格小于自身且存在
        for dep in &it.depends_on_seq {
            if *dep == 0 || *dep >= it.seq || !seq_set.contains(dep) {
                violations.push(Violation {
                    invariant: InvariantId::Dependency,
                    seq: it.seq,
                    message: format!("依赖序号 {} 不满足下界（INV-8）", dep),
                });
                break;
            }
        }
    }

    violations
}

/// 单轮修复（按固定优先级），返回是否产生了任何修改
fn repair_once(items: &mut Vec<PlanItem>, violations: &[Violation], spec: &CompiledSpec) -> bool {
    let dates = schedulable_dates(spec);
    let mut changed = false;

    // 1) 日期违规（INV-3/4）→ A4 重排到最近可排期日
    let bad_seqs: HashSet<u32> = violations
        .iter()
        .filter(|v| matches!(v.invariant, InvariantId::Blackout | InvariantId::Weekday))
        .map(|v| v.seq)
        .collect();
    if !bad_seqs.is_empty() && !dates.is_empty() {
        for it in items.iter_mut() {
            if !bad_seqs.contains(&it.seq) {
                continue;
            }
            let target = next_schedulable(&dates, it.plan_date)
                .or_else(|| prev_schedulable(&dates, it.plan_date));
            if let Some(target) = target {
                if target != it.plan_date {
                    it.plan_date = target;
                    changed = true;
                }
            }
        }
    }

    // 2) 单任务超额（INV-5）→ A3 再分配：拆分溢出到后续可排期日
    if violations
        .iter()
        .any(|v| v.invariant == InvariantId::MaxDailyQty)
    {
        changed |= repair_overflow(items, spec, &dates);
    }

    // 3) 单日任务条数（INV-7）→ A4 重排：优先后移复习/末尾任务
    if violations
        .iter()
        .any(|v| v.invariant == InvariantId::MaxDailyTasks)
    {
        changed |= repair_daily_task_count(items, spec, &dates);
    }

    // 4) 最小粒度（INV-6）→ A3 再分配：合并到相邻学习任务
    if violations.iter().any(|v| v.invariant == InvariantId::MinChunk) {
        changed |= repair_undersized(items, spec);
    }

    // 5) 守恒（INV-1）→ 补差到末尾学习任务（尾差例外）
    if violations
        .iter()
        .any(|v| v.invariant == InvariantId::Conservation)
    {
        changed |= repair_conservation(items, spec);
    }

    changed
}

/// INV-5 修复：把超额部分拆分到后续可排期日；复习任务直接截断（不参与守恒）
fn repair_overflow(items: &mut Vec<PlanItem>, spec: &CompiledSpec, dates: &[NaiveDate]) -> bool {
    let Some(max_qty) = spec.max_daily_qty else {
        return false;
    };
    if max_qty <= QTY_EPS {
        return false;
    }
    let mut produced: Vec<PlanItem> = Vec::new();
    let mut changed = false;

    for it in items.iter_mut() {
        if it.plan_qty <= max_qty + QTY_EPS {
            continue;
        }
        if it.task_type == TaskType::Review {
            it.plan_qty = max_qty;
            changed = true;
            continue;
        }
        let overflow = it.plan_qty - max_qty;
        it.plan_qty = max_qty;
        let template = it.clone();
        let mut remaining = overflow;
        let mut cursor = it.plan_date;
        let mut piece_no = 1usize;
        while remaining > QTY_EPS {
            let Some(target) = next_schedulable(dates, cursor) else {
                break;
            };
            let chunk = remaining.min(max_qty);
            let mut piece = template.clone();
            piece.plan_date = target;
            piece.plan_qty = chunk;
            piece.seq = 0;
            piece.depends_on_seq.clear();
            piece.name = format!("{}（续{}）", template.name, piece_no);
            produced.push(piece);
            remaining -= chunk;
            cursor = target;
            piece_no += 1;
        }
        if remaining > QTY_EPS {
            // 后续无可用日：保留尾差（由下一轮或 W-OVERFLOW-TAIL 兜底）
            it.plan_qty += remaining;
        }
        changed = true;
    }
    items.extend(produced);
    changed
}

/// INV-7 修复：把超限日期的任务后移到未满的可排期日
fn repair_daily_task_count(
    items: &mut [PlanItem],
    spec: &CompiledSpec,
    dates: &[NaiveDate],
) -> bool {
    let Some(max_tasks) = spec.max_daily_tasks else {
        return false;
    };
    let max = max_tasks.floor().max(0.0) as usize;
    let mut counts: std::collections::BTreeMap<NaiveDate, usize> = std::collections::BTreeMap::new();
    for it in items.iter() {
        *counts.entry(it.plan_date).or_insert(0) += 1;
    }
    let mut changed = false;
    for date in dates {
        let count = counts.get(date).copied().unwrap_or(0);
        if count <= max {
            continue;
        }
        let mut indices: Vec<usize> = items
            .iter()
            .enumerate()
            .filter(|(_, it)| it.plan_date == *date)
            .map(|(i, _)| i)
            .collect();
        // 优先移动复习任务，其次移动同日末尾任务
        indices.sort_by_key(|&i| {
            (
                if items[i].task_type == TaskType::Review {
                    0usize
                } else {
                    1usize
                },
                std::cmp::Reverse(i),
            )
        });
        for idx in indices {
            let current = counts.get(date).copied().unwrap_or(0);
            if current <= max {
                break;
            }
            let target = dates.iter().find(|d| {
                **d > *date && counts.get(d).copied().unwrap_or(0) < max
            });
            let Some(target) = target else { break };
            items[idx].plan_date = *target;
            counts.insert(*date, current - 1);
            *counts.entry(*target).or_insert(0) += 1;
            changed = true;
        }
    }
    changed
}

/// INV-6 修复：把低于最小粒度的学习任务合并到相邻学习任务（受单日上限约束）
fn repair_undersized(items: &mut Vec<PlanItem>, spec: &CompiledSpec) -> bool {
    let Some(min_chunk) = spec.min_chunk_qty else {
        return false;
    };
    let mut removed: HashSet<usize> = HashSet::new();
    let mut changed = false;

    for i in 0..items.len() {
        if removed.contains(&i) {
            continue;
        }
        if items[i].task_type != TaskType::Learn
            || items[i].plan_qty <= QTY_EPS
            || items[i].plan_qty >= min_chunk - QTY_EPS
        {
            continue;
        }
        let mut candidates: Vec<usize> = Vec::new();
        // 同日的相邻学习任务优先
        candidates.extend((0..i).rev().filter(|&j| {
            !removed.contains(&j)
                && items[j].task_type == TaskType::Learn
                && items[j].plan_date == items[i].plan_date
        }));
        candidates.extend(((i + 1)..items.len()).filter(|&j| {
            !removed.contains(&j)
                && items[j].task_type == TaskType::Learn
                && items[j].plan_date == items[i].plan_date
        }));
        // 再尝试相邻日期的学习任务
        candidates.extend((0..i).rev().filter(|&j| {
            !removed.contains(&j)
                && items[j].task_type == TaskType::Learn
                && items[j].plan_date != items[i].plan_date
        }));
        candidates.extend(((i + 1)..items.len()).filter(|&j| {
            !removed.contains(&j)
                && items[j].task_type == TaskType::Learn
                && items[j].plan_date != items[i].plan_date
        }));

        for j in candidates {
            let fits = spec
                .max_daily_qty
                .map(|max| items[j].plan_qty + items[i].plan_qty <= max + QTY_EPS)
                .unwrap_or(true);
            if fits {
                items[j].plan_qty += items[i].plan_qty;
                removed.insert(i);
                changed = true;
                break;
            }
        }
    }

    if !removed.is_empty() {
        let mut idx = 0usize;
        items.retain(|_| {
            let keep = !removed.contains(&idx);
            idx += 1;
            keep
        });
    }
    changed
}

/// INV-1 修复：把差额补到/扣自末尾学习任务
fn repair_conservation(items: &mut [PlanItem], spec: &CompiledSpec) -> bool {
    let learn_sum: f64 = items
        .iter()
        .filter(|it| it.task_type == TaskType::Learn)
        .map(|it| it.plan_qty)
        .sum();
    let diff = spec.total_qty - learn_sum;
    if diff.abs() <= CONSERVATION_EPS {
        return false;
    }

    if diff > 0.0 {
        let mut remaining = diff;
        for it in items.iter_mut().rev() {
            if it.task_type != TaskType::Learn {
                continue;
            }
            let spare = spec
                .max_daily_qty
                .map(|max| (max - it.plan_qty).max(0.0))
                .unwrap_or(remaining);
            let add = spare.min(remaining);
            if add > QTY_EPS {
                it.plan_qty += add;
                remaining -= add;
            }
            if remaining <= QTY_EPS {
                break;
            }
        }
        if remaining > QTY_EPS {
            // 尾差例外：挂到最后一个学习任务（对应 W-OVERFLOW-TAIL）
            if let Some(last) = items
                .iter_mut()
                .rev()
                .find(|it| it.task_type == TaskType::Learn)
            {
                last.plan_qty += remaining;
            } else {
                return false;
            }
        }
    } else {
        let mut remaining = -diff;
        for it in items.iter_mut().rev() {
            if it.task_type != TaskType::Learn {
                continue;
            }
            let take = it.plan_qty.min(remaining);
            it.plan_qty -= take;
            remaining -= take;
            if remaining <= QTY_EPS {
                break;
            }
        }
    }
    true
}

/// 残留违规 → 警告
fn violation_warning(v: &Violation, items: &[PlanItem]) -> PlanWarning {
    let date = items
        .iter()
        .find(|it| it.seq == v.seq)
        .map(|it| it.plan_date.format("%Y-%m-%d").to_string());
    PlanWarning {
        code: v.invariant.warning_code().to_string(),
        date,
        message: v.message.clone(),
    }
}

/// 执行多轮修复循环
pub fn run_repair_loop(
    mut items: Vec<PlanItem>,
    spec: &CompiledSpec,
    dependency_chain: bool,
) -> RepairOutcome {
    normalize(&mut items, dependency_chain);
    let mut remaining = validate_invariants(&items, spec);
    let mut rounds = 0u32;

    while !remaining.is_empty() && rounds < MAX_REPAIR_ROUNDS {
        let before = remaining.len();
        let changed = repair_once(&mut items, &remaining, spec);
        normalize(&mut items, dependency_chain);
        let after = validate_invariants(&items, spec);
        rounds += 1;
        if after.is_empty() {
            remaining.clear();
            break;
        }
        if !changed || after.len() >= before {
            // 无进展或违规数未严格下降：提前降级，保留最接近解
            remaining = after;
            break;
        }
        remaining = after;
    }

    let converged = remaining.is_empty();
    let mut warnings = Vec::new();
    if !converged {
        for v in remaining.iter().take(MAX_ITEM_WARNINGS) {
            warnings.push(violation_warning(v, &items));
        }
        let codes: std::collections::BTreeSet<&str> = remaining
            .iter()
            .map(|v| v.invariant.code())
            .collect();
        warnings.push(PlanWarning {
            code: "W-REPAIR-LIMIT".to_string(),
            date: None,
            message: format!(
                "修复循环 {} 轮后仍有 {} 项约束未满足（{}），已返回最接近的方案",
                rounds,
                remaining.len(),
                codes.into_iter().collect::<Vec<_>>().join("/")
            ),
        });
    }

    RepairOutcome {
        items,
        rounds,
        converged,
        remaining,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::planner::constraints::CompiledSpec;
    use crate::services::planner::snapshot::DaySlot;
    use crate::services::planner::types::SmoothingPolicy;

    fn start() -> NaiveDate {
        // 2026-10-26（周一）
        NaiveDate::from_ymd_opt(2026, 10, 26).unwrap()
    }

    fn make_spec(
        day_count: i64,
        max_qty: Option<f64>,
        min_chunk: Option<f64>,
        max_tasks: Option<f64>,
    ) -> CompiledSpec {
        let start = start();
        let days: Vec<DaySlot> = (0..day_count)
            .map(|i| DaySlot {
                date: start + chrono::Duration::days(i),
                available: true,
                weight: 1.0,
                is_rest: false,
            })
            .collect();
        CompiledSpec {
            start,
            end: start + chrono::Duration::days(day_count - 1),
            buffer_days: 0,
            total_qty: 10.0,
            unit: "页".to_string(),
            daily_capacity: None,
            max_daily_qty: max_qty,
            max_daily_hours: None,
            max_daily_tasks: max_tasks,
            min_chunk_qty: min_chunk,
            global_daily_task_budget: 6,
            blackout: Default::default(),
            allowed_weekdays: (0..7).collect(),
            weekend_factor: 1.0,
            adapt_strength: 1.0,
            smoothing: SmoothingPolicy::default(),
            days,
            effective_days: day_count as u32,
            total_capacity: day_count as f64,
        }
    }

    fn make_item(seq: u32, date: NaiveDate, qty: f64, task_type: TaskType) -> PlanItem {
        PlanItem {
            seq,
            name: format!("任务{}", seq),
            description: None,
            plan_date: date,
            plan_qty: qty,
            unit: "页".to_string(),
            estimated_hours: None,
            task_type,
            phase: None,
            stage_hint: None,
            depends_on_seq: Vec::new(),
            rationale: None,
            confidence: None,
        }
    }

    fn day(offset: i64) -> NaiveDate {
        start() + chrono::Duration::days(offset)
    }

    #[test]
    fn test_conservation_repaired() {
        let spec = make_spec(3, None, None, None);
        let items = vec![
            make_item(1, day(0), 4.0, TaskType::Learn),
            make_item(2, day(1), 4.0, TaskType::Learn),
        ];
        let outcome = run_repair_loop(items, &spec, false);
        assert!(outcome.converged, "{:?}", outcome.remaining);
        let sum: f64 = outcome.items.iter().map(|it| it.plan_qty).sum();
        assert!((sum - 10.0).abs() < CONSERVATION_EPS);
    }

    #[test]
    fn test_blackout_relocated_to_next_schedulable() {
        let mut spec = make_spec(4, None, None, None);
        spec.blackout.insert(day(0));
        if let Some(slot) = spec.days.iter_mut().find(|d| d.date == day(0)) {
            slot.available = false;
            slot.is_rest = true;
            slot.weight = 0.0;
        }
        spec.effective_days = 3;
        spec.total_capacity = 3.0;
        let items = vec![
            make_item(1, day(0), 5.0, TaskType::Learn),
            make_item(2, day(1), 5.0, TaskType::Learn),
        ];
        let outcome = run_repair_loop(items, &spec, false);
        assert!(outcome.converged, "{:?}", outcome.remaining);
        assert_eq!(outcome.items[1].plan_date, day(1));
        assert_eq!(outcome.items[0].plan_date, day(1));
    }

    #[test]
    fn test_overflow_split_across_days() {
        let spec = make_spec(5, Some(10.0), None, None);
        let items = vec![
            make_item(1, day(0), 25.0, TaskType::Learn),
            make_item(2, day(1), 0.0, TaskType::Learn),
        ];
        // 总量 10 与任务合计不一致，守恒会补差；这里只验证 INV-5 被修复
        let outcome = run_repair_loop(items, &spec, false);
        assert!(outcome
            .items
            .iter()
            .all(|it| it.plan_qty <= 10.0 + QTY_EPS));
        assert!(!outcome.remaining
            .iter()
            .any(|v| v.invariant == InvariantId::MaxDailyQty));
    }

    #[test]
    fn test_min_chunk_merged() {
        let spec = make_spec(3, Some(10.0), Some(3.0), None);
        let items = vec![
            make_item(1, day(0), 3.0, TaskType::Learn),
            make_item(2, day(1), 1.0, TaskType::Learn),
            make_item(3, day(2), 6.0, TaskType::Learn),
        ];
        let outcome = run_repair_loop(items, &spec, false);
        assert!(outcome.converged, "{:?}", outcome.remaining);
        assert!(outcome
            .items
            .iter()
            .all(|it| it.plan_qty >= 3.0 - QTY_EPS));
        assert_eq!(outcome.items.len(), 2);
    }

    #[test]
    fn test_daily_task_cap_moves_to_next_day() {
        let spec = make_spec(3, None, None, Some(1.0));
        let items = vec![
            make_item(1, day(0), 4.0, TaskType::Learn),
            make_item(2, day(0), 3.0, TaskType::Learn),
            make_item(3, day(1), 3.0, TaskType::Learn),
        ];
        let outcome = run_repair_loop(items, &spec, false);
        assert!(outcome.converged, "{:?}", outcome.remaining);
        let d0 = outcome
            .items
            .iter()
            .filter(|it| it.plan_date == day(0))
            .count();
        assert_eq!(d0, 1);
    }

    #[test]
    fn test_dependency_chain_regenerated() {
        let spec = make_spec(3, None, None, None);
        let mut a = make_item(1, day(0), 4.0, TaskType::Learn);
        let mut b = make_item(2, day(1), 3.0, TaskType::Learn);
        let mut c = make_item(3, day(2), 3.0, TaskType::Learn);
        a.depends_on_seq = vec![3];
        b.depends_on_seq = vec![1];
        c.depends_on_seq = vec![2];
        let outcome = run_repair_loop(vec![a, b, c], &spec, true);
        assert!(outcome.converged, "{:?}", outcome.remaining);
        assert_eq!(outcome.items[0].depends_on_seq, Vec::<u32>::new());
        assert_eq!(outcome.items[1].depends_on_seq, vec![1]);
        assert_eq!(outcome.items[2].depends_on_seq, vec![2]);
    }

    #[test]
    fn test_repair_limit_no_panic_and_warns() {
        // min_chunk=10 与 max_daily_qty=5 冲突：无法合并也无法拆分到满足
        let spec = make_spec(2, Some(5.0), Some(10.0), None);
        let items = vec![
            make_item(1, day(0), 5.0, TaskType::Learn),
            make_item(2, day(1), 5.0, TaskType::Learn),
        ];
        let outcome = run_repair_loop(items, &spec, false);
        assert!(!outcome.converged);
        assert!(outcome.rounds <= MAX_REPAIR_ROUNDS);
        assert!(outcome
            .warnings
            .iter()
            .any(|w| w.code == "W-REPAIR-LIMIT"));
        assert!(!outcome.remaining.is_empty());
    }

    #[test]
    fn test_order_normalized() {
        let spec = make_spec(3, None, None, None);
        let items = vec![
            make_item(2, day(1), 5.0, TaskType::Learn),
            make_item(1, day(0), 5.0, TaskType::Learn),
        ];
        let outcome = run_repair_loop(items, &spec, false);
        assert!(outcome.converged, "{:?}", outcome.remaining);
        assert_eq!(outcome.items[0].plan_date, day(0));
        assert_eq!(outcome.items[0].seq, 1);
        assert_eq!(outcome.items[1].seq, 2);
    }
}
