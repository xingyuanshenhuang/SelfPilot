//! 目标树智能规划引擎（v2）——A4 DAG 依赖压力传递调度器
//!
//! 实现重设计方案 §3.5（A4 `schedule_dag`）：把依赖从「排序提示」升级为
//! 「日期硬约束」，修复 G-06 死锁风险。
//!
//! 与伪代码的对应关系：
//! - 行 1~3：Kahn 拓扑排序（同层按 `sort_order` 稳定排序，语义与
//!   `dependency_service::topo_sort_by_dependency` 一致，但对自定义调度
//!   任务重实现，且检测到环时返回 `Err(CycleDetected)` 而非回退）；
//! - 行 5~9：`est` 前向传播——后继任务的最早可开始日 = 前置完成日 + 1；
//! - 行 13~17：锁定任务预占位——原日期落位并扣减当日配额（FR-10 /
//!   SC-08：重排只动未锁定部分）；
//! - 行 19~31：拓扑序消费配额——从 `est[t]` 起找首个有配额的日；
//!   当日配额不足时剩余量顺延到后续日（`split_remainder`，稀疏日场景），
//!   任务仍记在首个落位日（单日完成不变式：`plan_date` 单值模型）；
//! - 行 23~24：尾部溢出——配额耗尽时挂视界末日并标记 `OverflowTail`
//!   （不可行应由 A2 拦截，此处为防御性兜底，调用方转为 W-OVERFLOW-TAIL）；
//! - 行 33~34：`apply_overdue_pull`——`plan_date < today` 的**未锁定**任务
//!   下界强制为 `today + 1`，真正改写日期（修复 G-04）；锁定任务受
//!   INV-4 保护（∀t∈locked: placement(t) = original(t)），即使逾期也不前移。
//!
//! 本模块为纯函数：不访问数据库、不读时钟（`today` 由调用方注入）。

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use chrono::{Duration, NaiveDate};

use crate::error::{AppError, AppResult};
use crate::services::planner::allocator::DayQuota;

/// 配额被视为耗尽的阈值（浮点噪声保护）
const QTY_EPS: f64 = 1e-9;

/// 调度输入任务（源 A 任务草案 / 源 B 既有任务的统一投影）
#[derive(Debug, Clone)]
pub struct SchedTask {
    /// 任务 ID（源 B 为既有任务 id；源 A 为草案临时 id）
    pub id: String,
    /// 任务量（消耗的配额）
    pub cost: f64,
    /// 既有计划日期（锁定任务预占位与逾期检测用；无则为 None）
    pub plan_date: Option<NaiveDate>,
    /// 稳定排序键（与 `tasks.sort_order` 一致，同层拓扑输出按其升序）
    pub sort_order: i64,
}

/// 排期决策依据（F-5：与 `task_type` 正交）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlacementReason {
    /// 锁定任务原位保留（schedule_locked / 预占位）
    Locked,
    /// est 依赖下界 + 拓扑序配额消费（常规路径）
    DepLowerBound,
    /// 尾部溢出（配额耗尽挂视界末日，对应 W-OVERFLOW-TAIL）
    OverflowTail,
    /// 逾期前移（plan_date < today 被拉到 today + 1，对应 G-04）
    OverduePull,
}

/// 任务落位（A4 输出，与任务一一对应——单日完成不变式）
#[derive(Debug, Clone, PartialEq)]
pub struct TaskPlacement {
    /// 任务 ID
    pub task_id: String,
    /// 落位日期（`plan_date` 单值）
    pub date: NaiveDate,
    /// 任务量（完整 cost；跨日扣减配额时仍记首个落位日）
    pub qty: f64,
    /// 是否锁定任务（预占位，位置不动）
    pub locked: bool,
    /// 排期决策依据
    pub reason: PlacementReason,
    /// 是否被逾期前移改写过日期
    pub overdue_pulled: bool,
}

/// Kahn 拓扑排序（仅统计集合内依赖；同层按 `(sort_order, 输入序)` 稳定排序）
///
/// 检测到环时返回 `Err`（伪代码行 2~3 的 CycleDetected 防御）。
fn kahn_sort(tasks: &[SchedTask], deps: &HashMap<String, Vec<String>>) -> AppResult<Vec<SchedTask>> {
    let id_set: HashSet<&str> = tasks.iter().map(|t| t.id.as_str()).collect();

    // 入度 = 集合内前置数量；邻接表：前置 → 后继
    let mut in_degree: HashMap<&str, usize> = HashMap::with_capacity(tasks.len());
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    for t in tasks {
        in_degree.entry(t.id.as_str()).or_insert(0);
        if let Some(preds) = deps.get(&t.id) {
            for p in preds {
                if id_set.contains(p.as_str()) {
                    adj.entry(p.as_str()).or_default().push(t.id.as_str());
                    *in_degree.entry(t.id.as_str()).or_insert(0) += 1;
                }
            }
        }
    }

    // 输入序做次级稳定键（sort_order 相同任务保持输入顺序）
    let input_index: HashMap<&str, usize> = tasks
        .iter()
        .enumerate()
        .map(|(i, t)| (t.id.as_str(), i))
        .collect();

    let mut zero_in: Vec<&SchedTask> = tasks
        .iter()
        .filter(|t| in_degree[t.id.as_str()] == 0)
        .collect();
    zero_in.sort_by_key(|t| (t.sort_order, input_index[t.id.as_str()]));

    let mut queue: VecDeque<&SchedTask> = zero_in.into_iter().collect();
    // id → 任务引用索引（O(1) 查找，避免释放后继时的线性扫描）
    let task_by_id: HashMap<&str, &SchedTask> =
        tasks.iter().map(|t| (t.id.as_str(), t)).collect();
    let mut result: Vec<SchedTask> = Vec::with_capacity(tasks.len());
    let mut processed: HashSet<&str> = HashSet::with_capacity(tasks.len());

    while let Some(t) = queue.pop_front() {
        if !processed.insert(t.id.as_str()) {
            continue;
        }
        let id = t.id.as_str();
        result.push(t.clone());
        if let Some(nexts) = adj.get(id) {
            // 释放后继：新入度为 0 的按 (sort_order, 输入序) 排序后入队
            let mut ready: Vec<&str> = Vec::new();
            for nid in nexts {
                if let Some(deg) = in_degree.get_mut(*nid) {
                    *deg -= 1;
                    if *deg == 0 {
                        ready.push(nid);
                    }
                }
            }
            let mut ready_tasks: Vec<&SchedTask> = ready
                .into_iter()
                .filter_map(|nid| task_by_id.get(nid).copied())
                .collect();
            ready_tasks.sort_by_key(|t| (t.sort_order, input_index[t.id.as_str()]));
            queue.extend(ready_tasks);
        }
    }

    if result.len() < tasks.len() {
        return Err(AppError::Business(
            "任务依赖存在环，无法完成拓扑排序（CycleDetected）".into(),
        ));
    }
    Ok(result)
}

/// A4 DAG 依赖压力传递调度（伪代码 §3.5）
///
/// - `tasks`：待落位任务（含 cost 与既有 plan_date）；
/// - `deps`：task_id → 前置任务 id 列表（集合外前置自动忽略）；
/// - `quotas`：A3 输出的每日配额（按日期升序）；
/// - `today`：今天（注入，不读时钟）；
/// - `locked`：锁定任务 id 集合（预占位：原日期落位、扣配额、位置不动）。
///
/// 返回与任务一一对应的落位序列（单日完成不变式）；尾部溢出经
/// `reason == OverflowTail` 标记，由调用方转为 W-OVERFLOW-TAIL 警告。
pub fn schedule_dag(
    tasks: &[SchedTask],
    deps: &HashMap<String, Vec<String>>,
    quotas: &[DayQuota],
    today: NaiveDate,
    locked: &HashSet<String>,
) -> AppResult<Vec<TaskPlacement>> {
    if quotas.is_empty() {
        return Err(AppError::Business("每日配额为空，无法调度".into()));
    }
    let tomorrow = today + Duration::days(1);

    // 配额索引（日期升序；BTreeMap 保证遍历序确定）
    let mut quota_map: BTreeMap<NaiveDate, f64> = BTreeMap::new();
    for q in quotas {
        let entry = quota_map.entry(q.date).or_insert(0.0);
        *entry += q.qty;
    }
    let dates: Vec<NaiveDate> = quota_map.keys().copied().collect();

    // 行 1~3：拓扑排序（环 → Err）
    let order = kahn_sort(tasks, deps)?;

    let mut placements: Vec<TaskPlacement> = Vec::with_capacity(tasks.len());
    // finish：完成日（est 前向传播用）
    let mut finish: HashMap<&str, NaiveDate> = HashMap::with_capacity(tasks.len());

    // 行 13~17：锁定任务先占位（消耗对应日配额，形成 reserved）
    for t in tasks.iter().filter(|t| locked.contains(&t.id)) {
        let d = t.plan_date.ok_or_else(|| {
            AppError::Business(format!("锁定任务 {} 缺少计划日期，无法预占位", t.id))
        })?;
        placements.push(TaskPlacement {
            task_id: t.id.clone(),
            date: d,
            qty: t.cost,
            locked: true,
            reason: PlacementReason::Locked,
            overdue_pulled: false,
        });
        if let Some(q) = quota_map.get_mut(&d) {
            *q = (*q - t.cost).max(0.0);
        }
        finish.insert(t.id.as_str(), d);
    }

    // 行 19~31：拓扑序分配
    for t in &order {
        if locked.contains(&t.id) {
            continue; // 锁定任务已占位
        }
        // 行 7~9：est 前向传播——所有集合内前置完成日的次日取 max
        let mut lower = tomorrow;
        if let Some(preds) = deps.get(&t.id) {
            for p in preds {
                if let Some(&fd) = finish.get(p.as_str()) {
                    let next = fd + Duration::days(1);
                    if next > lower {
                        lower = next;
                    }
                }
            }
        }

        // 行 21：从 est 起首个有配额的日（日期升序）
        let mut chosen: Option<NaiveDate> = None;
        for &d in &dates {
            if d >= lower && quota_map.get(&d).copied().unwrap_or(0.0) > QTY_EPS {
                chosen = Some(d);
                break;
            }
        }
        // 行 23~24：尾部溢出——挂视界末日（防御性兜底，A2 应已拦截）
        let (first_day, mut reason) = match chosen {
            Some(d) => (d, PlacementReason::DepLowerBound),
            None => (
                *dates
                    .last()
                    .ok_or_else(|| AppError::Business("配额日期序列为空".into()))?,
                PlacementReason::OverflowTail,
            ),
        };

        // 行 25~28：split_remainder——当日配额不足时剩余量顺延后续日
        //（任务仍记首个落位日，保持 plan_date 单值模型）
        let mut remaining = t.cost;
        let mut idx = dates.partition_point(|&d| d < first_day);
        while remaining > QTY_EPS && idx < dates.len() {
            let d = dates[idx];
            if let Some(q) = quota_map.get_mut(&d) {
                let take = (*q).min(remaining);
                if take > QTY_EPS {
                    *q -= take;
                    remaining -= take;
                }
            }
            idx += 1;
        }
        if remaining > QTY_EPS {
            // 配额整体耗尽：尾部溢出，任务挂账首日并标记
            reason = PlacementReason::OverflowTail;
        }

        // 行 29~31：落位（单日完成：finish = 首个落位日）
        placements.push(TaskPlacement {
            task_id: t.id.clone(),
            date: first_day,
            qty: t.cost,
            locked: false,
            reason,
            overdue_pulled: false,
        });
        finish.insert(t.id.as_str(), first_day);
    }

    // 行 33~34：apply_overdue_pull——plan_date < today 的**未锁定**任务拉到
    // today + 1。锁定任务受 INV-4 保护（SC-08/FR-10：重排只动未锁定部分），
    // 即使原日期已逾期也保持不动（配额记账以预占位为准，不产生超售）。
    for p in placements.iter_mut() {
        if !p.locked && p.date < today {
            p.date = tomorrow;
            p.overdue_pulled = true;
            p.reason = PlacementReason::OverduePull;
        }
    }

    Ok(placements)
}

// ============================================================
// 单元测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn quota(date: NaiveDate, qty: f64) -> DayQuota {
        DayQuota { date, qty }
    }

    fn task(id: &str, cost: f64, sort_order: i64) -> SchedTask {
        SchedTask {
            id: id.into(),
            cost,
            plan_date: None,
            sort_order,
        }
    }

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    /// 4. est 传播：A→B 依赖时 B 的日期 > A 的日期
    #[test]
    fn test_est_propagation() {
        let today = d(2026, 10, 9);
        // 配额：两天各 5
        let quotas = vec![quota(d(2026, 10, 10), 5.0), quota(d(2026, 10, 11), 5.0)];
        let tasks = vec![task("a", 5.0, 0), task("b", 5.0, 1)];
        let deps: HashMap<String, Vec<String>> =
            [("b".to_string(), vec!["a".to_string()])].into_iter().collect();
        let placements =
            schedule_dag(&tasks, &deps, &quotas, today, &HashSet::new()).unwrap();
        let a = placements.iter().find(|p| p.task_id == "a").unwrap();
        let b = placements.iter().find(|p| p.task_id == "b").unwrap();
        assert_eq!(a.date, d(2026, 10, 10));
        // B 依赖 A：B 的日期 > A 的日期（est = A 完成日 + 1）
        assert!(b.date > a.date, "B({}) 必须晚于 A({})", b.date, a.date);
        assert_eq!(b.date, d(2026, 10, 11));
        assert_eq!(b.reason, PlacementReason::DepLowerBound);
    }

    /// est 传播强化：配额充足时后继也不得与前置同日
    #[test]
    fn test_est_strictly_after_predecessor() {
        let today = d(2026, 10, 9);
        // 单日大配额（10），A、B 各 3——若无依赖会同日
        let quotas = vec![quota(d(2026, 10, 10), 10.0), quota(d(2026, 10, 11), 10.0)];
        let tasks = vec![task("a", 3.0, 0), task("b", 3.0, 1)];
        let deps: HashMap<String, Vec<String>> =
            [("b".to_string(), vec!["a".to_string()])].into_iter().collect();
        let placements =
            schedule_dag(&tasks, &deps, &quotas, today, &HashSet::new()).unwrap();
        let a = placements.iter().find(|p| p.task_id == "a").unwrap();
        let b = placements.iter().find(|p| p.task_id == "b").unwrap();
        assert_eq!(a.date, d(2026, 10, 10));
        assert_eq!(b.date, d(2026, 10, 11), "依赖后继必须在前置完成次日");
    }

    /// 5. 锁定预占位：锁定任务消耗配额且位置不动
    #[test]
    fn test_locked_preallocation() {
        let today = d(2026, 10, 9);
        // 两日各 5；locked-b 锁定在 10-11
        let quotas = vec![quota(d(2026, 10, 10), 5.0), quota(d(2026, 10, 11), 5.0)];
        let mut b = task("b", 3.0, 1);
        b.plan_date = Some(d(2026, 10, 11));
        let tasks = vec![task("a", 4.0, 0), b];
        let locked: HashSet<String> = ["b".to_string()].into_iter().collect();
        let placements = schedule_dag(&tasks, &HashMap::new(), &quotas, today, &locked).unwrap();

        let b_p = placements.iter().find(|p| p.task_id == "b").unwrap();
        assert_eq!(b_p.date, d(2026, 10, 11), "锁定任务位置不动");
        assert!(b_p.locked);
        assert_eq!(b_p.reason, PlacementReason::Locked);

        // a 排在 10-10：第一日配额 5 ≥ 4
        let a_p = placements.iter().find(|p| p.task_id == "a").unwrap();
        assert_eq!(a_p.date, d(2026, 10, 10));

        // 锁定任务真实扣配额：预占后当日余量不足时任务记首日、余量顺延后续日
        let quotas = vec![quota(d(2026, 10, 10), 5.0), quota(d(2026, 10, 11), 5.0)];
        let mut c = task("c", 4.0, 0);
        c.plan_date = Some(d(2026, 10, 10));
        let tasks = vec![c, task("d", 4.0, 1)];
        let locked: HashSet<String> = ["c".to_string()].into_iter().collect();
        let placements = schedule_dag(&tasks, &HashMap::new(), &quotas, today, &locked).unwrap();
        let c_p = placements.iter().find(|p| p.task_id == "c").unwrap();
        let d_p = placements.iter().find(|p| p.task_id == "d").unwrap();
        assert_eq!(c_p.date, d(2026, 10, 10));
        // c 预占 4 后首日仅剩 1：d 落位首个有配额的 10-10（单日完成不变式），
        // 剩余 3 份顺延消耗 10-11 的配额（split_remainder）
        assert_eq!(d_p.date, d(2026, 10, 10));
        assert_eq!(d_p.qty, 4.0);
    }

    /// 6. 逾期锁定任务不被前移（INV-4：∀t∈locked，placement 保持原日期）
    #[test]
    fn test_locked_overdue_not_pulled() {
        let today = d(2026, 10, 9);
        let quotas = vec![quota(d(2026, 10, 10), 5.0), quota(d(2026, 10, 11), 5.0)];
        // 锁定任务 c 停留在已过期的 10-01：锁定保护优先于逾期前移
        let mut c = task("c", 2.0, 0);
        c.plan_date = Some(d(2026, 10, 1));
        let tasks = vec![c];
        let locked: HashSet<String> = ["c".to_string()].into_iter().collect();
        let placements = schedule_dag(&tasks, &HashMap::new(), &quotas, today, &locked).unwrap();
        let c_p = &placements[0];
        assert_eq!(c_p.date, d(2026, 10, 1), "锁定任务必须保持原日期（INV-4）");
        assert!(c_p.locked);
        assert!(!c_p.overdue_pulled);
        assert_eq!(c_p.reason, PlacementReason::Locked);
    }

    /// 尾部溢出：配额耗尽时挂视界末日并标记 OverflowTail
    #[test]
    fn test_overflow_tail() {
        let today = d(2026, 10, 9);
        // 仅 1 日配额 2，任务 a 需要 5
        let quotas = vec![quota(d(2026, 10, 10), 2.0)];
        let tasks = vec![task("a", 5.0, 0)];
        let placements =
            schedule_dag(&tasks, &HashMap::new(), &quotas, today, &HashSet::new()).unwrap();
        let a_p = &placements[0];
        assert_eq!(a_p.date, d(2026, 10, 10), "溢出任务挂视界末日");
        assert_eq!(a_p.reason, PlacementReason::OverflowTail);
        assert_eq!(a_p.qty, 5.0, "任务量保持完整（守恒由配额侧保证）");
    }

    /// 环检测：依赖环返回 Err（防御）
    #[test]
    fn test_cycle_detected() {
        let today = d(2026, 10, 9);
        let quotas = vec![quota(d(2026, 10, 10), 10.0)];
        let tasks = vec![task("a", 1.0, 0), task("b", 1.0, 1)];
        let deps: HashMap<String, Vec<String>> = [
            ("a".to_string(), vec!["b".to_string()]),
            ("b".to_string(), vec!["a".to_string()]),
        ]
        .into_iter()
        .collect();
        let result = schedule_dag(&tasks, &deps, &quotas, today, &HashSet::new());
        assert!(result.is_err());
    }

    /// 拓扑序消费配额：前置任务优先拿到更早日期
    #[test]
    fn test_topological_order_consumes_quota() {
        let today = d(2026, 10, 9);
        // 两日各 3；三个任务 a(1) → b(2)，c 独立
        let quotas = vec![quota(d(2026, 10, 10), 3.0), quota(d(2026, 10, 11), 3.0)];
        let tasks = vec![task("a", 1.0, 0), task("b", 2.0, 1), task("c", 1.0, 2)];
        let deps: HashMap<String, Vec<String>> =
            [("b".to_string(), vec!["a".to_string()])].into_iter().collect();
        let placements =
            schedule_dag(&tasks, &deps, &quotas, today, &HashSet::new()).unwrap();
        let a = placements.iter().find(|p| p.task_id == "a").unwrap();
        let b = placements.iter().find(|p| p.task_id == "b").unwrap();
        let c = placements.iter().find(|p| p.task_id == "c").unwrap();
        // 拓扑序 a → c → b：前置 a 最先拿到最早日期
        assert_eq!(a.date, d(2026, 10, 10));
        assert!(b.date > a.date);
        // c 独立：首日剩余配额 3-1=2 ≥ 1 → 10-10
        assert_eq!(c.date, d(2026, 10, 10));
    }

    /// 空配额返回 Err；空任务返回空落位
    #[test]
    fn test_empty_inputs() {
        let today = d(2026, 10, 9);
        assert!(schedule_dag(&[], &HashMap::new(), &[], today, &HashSet::new()).is_err());
        let quotas = vec![quota(d(2026, 10, 10), 5.0)];
        let placements =
            schedule_dag(&[], &HashMap::new(), &quotas, today, &HashSet::new()).unwrap();
        assert!(placements.is_empty());
    }
}
