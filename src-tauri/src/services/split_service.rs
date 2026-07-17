use chrono::{Datelike, NaiveDate};
use std::collections::HashMap;
use uuid::Uuid;

use crate::db::models::{
    Goal, ReplanPreview, ReplanPreviewItem, RepeatSplitInput, SmartSplitInput, Task,
};
use crate::error::{AppError, AppResult};
use crate::services::dependency_service;

/// 自动拆解目标为每日任务
///
/// PRD §4.2 模块二：
/// - 按截止日期剩余天数（不含今天，且 ≥ 1）将总量平均分配到每天
/// - 余数分配到前几天
/// - 每个任务记录"计划数量"和"单位"
/// - 任务从明天开始，到截止日为止
pub fn split_goal_into_tasks(goal: &Goal, today: NaiveDate) -> AppResult<Vec<Task>> {
    let deadline_str = goal.deadline.as_ref().ok_or_else(|| {
        AppError::Param("目标未设置截止日期，无法拆解".into())
    })?;

    let deadline = NaiveDate::parse_from_str(deadline_str, "%Y-%m-%d")
        .map_err(|e| AppError::Param(format!("截止日期格式错误: {}", e)))?;

    let remaining_days = (deadline - today).num_days();
    if remaining_days < 1 {
        return Err(AppError::Business(format!(
            "截止日期剩余天数不足（{}天），至少需要留出1天",
            remaining_days
        )));
    }

    let total = goal.total_qty;
    if total <= 0.0 {
        return Err(AppError::Business("目标总量必须大于0才能自动拆解".into()));
    }

    // 平均分配：base = floor(total / days)，余数分到前几天
    let base = (total / remaining_days as f64).floor();
    let remainder = (total - base * remaining_days as f64).round() as i64;

    let now = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();
    let mut tasks = Vec::new();
    let mut day_index = 0;

    for i in 0..remaining_days {
        let plan_qty = if i < remainder { base + 1.0 } else { base };

        // 跳过 plan_qty = 0 的任务（总量小于天数时）
        if plan_qty <= 0.0 {
            continue;
        }

        day_index += 1;
        let task_date = today + chrono::Duration::days(i + 1); // 从明天开始
        let task_id = Uuid::new_v4().to_string();
        let path = format!("/{}/{}", goal.id, task_id);

        tasks.push(Task {
            id: task_id,
            goal_id: goal.id.clone(),
            stage_id: None,
            parent_id: Some(goal.id.clone()),
            path,
            name: format!("{} - 第{}天", goal.name, day_index),
            plan_date: Some(task_date.format("%Y-%m-%d").to_string()),
            overdue_date: None,
            plan_qty,
            actual_qty: 0.0,
            unit: goal.unit.clone(),
            status: "pending".to_string(),
            is_manual: 0,
            source: "auto".to_string(),
            sort_order: i,
            created_at: now.clone(),
            estimated_hours: None,
        });
    }

    Ok(tasks)
}

/// 按每日可用时长拆解（P1-3 时间预算模型）
///
/// 与 `split_goal_into_tasks` 不同，此函数按 `goal.daily_capacity` 决定每个任务的计划量，
/// 而非按天数均摊总量。任务数 = ceil(total_qty / daily_capacity)，
/// 每个任务 plan_qty = daily_capacity（最后一个任务可能不足）。
///
/// 任务从明天开始，逐日生成，每天一个任务，直到总量分配完毕。
/// 若剩余天数不足（截止日期前无法排完），返回错误。
pub fn split_by_daily_capacity(goal: &Goal, today: NaiveDate) -> AppResult<Vec<Task>> {
    let deadline_str = goal.deadline.as_ref().ok_or_else(|| {
        AppError::Param("目标未设置截止日期，无法拆解".into())
    })?;
    let deadline = NaiveDate::parse_from_str(deadline_str, "%Y-%m-%d")
        .map_err(|e| AppError::Param(format!("截止日期格式错误: {}", e)))?;

    let remaining_days = (deadline - today).num_days();
    if remaining_days < 1 {
        return Err(AppError::Business(format!(
            "截止日期剩余天数不足（{}天），至少需要留出1天",
            remaining_days
        )));
    }

    let total = goal.total_qty;
    if total <= 0.0 {
        return Err(AppError::Business("目标总量必须大于0才能拆解".into()));
    }

    let capacity = goal.daily_capacity.ok_or_else(|| {
        AppError::Param("未设置每日可用时长（daily_capacity），无法按时间预算拆解".into())
    })?;
    if capacity <= 0.0 {
        return Err(AppError::Business("每日可用时长必须大于0".into()));
    }

    // 任务数 = ceil(total / capacity)
    let num_tasks = (total / capacity).ceil() as i64;
    if num_tasks == 0 {
        return Err(AppError::Business("计算任务数为0，无法拆解".into()));
    }

    // 检查剩余天数是否足够
    if num_tasks > remaining_days {
        return Err(AppError::Business(format!(
            "按每日{}{}拆解需要{}天，但截止日期前仅剩{}天，请调整每日可用时长或截止日期",
            capacity, goal.unit, num_tasks, remaining_days
        )));
    }

    let now = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();
    let mut tasks = Vec::new();
    let mut allocated = 0.0;

    for i in 0..num_tasks {
        // 最后一个任务可能不足 capacity
        let remaining = total - allocated;
        let plan_qty = remaining.min(capacity);
        allocated += plan_qty;

        let task_date = today + chrono::Duration::days(i + 1); // 从明天开始
        let task_id = Uuid::new_v4().to_string();
        let path = format!("/{}/{}", goal.id, task_id);

        tasks.push(Task {
            id: task_id,
            goal_id: goal.id.clone(),
            stage_id: None,
            parent_id: Some(goal.id.clone()),
            path,
            name: format!("{} - 第{}天", goal.name, i + 1),
            plan_date: Some(task_date.format("%Y-%m-%d").to_string()),
            overdue_date: None,
            plan_qty,
            actual_qty: 0.0,
            unit: goal.unit.clone(),
            status: "pending".to_string(),
            is_manual: 0,
            source: "auto".to_string(),
            sort_order: i,
            created_at: now.clone(),
            estimated_hours: Some(capacity),
        });
    }

    Ok(tasks)
}

/// 按自定义日期范围拆解（整合拆解的第三种策略）
///
/// 用户指定起始日期和结束日期，可选每日数量：
/// - 指定 `per_day_qty`：任务数 = ceil(total_qty / per_day_qty)，从 start_date 起每天一个任务，
///   每个任务 plan_qty = per_day_qty（最后一个可能不足）。若任务数超出日期范围天数则报错。
/// - 未指定 `per_day_qty`：按日期范围天数均分总量（余数分前几天），每天一个任务。
///
/// 与 `split_goal_into_tasks` / `split_by_daily_capacity` 的区别：
/// - 起止日期由用户显式指定，不依赖 goal.deadline 和"今天"
/// - 适用于"我想在这段时间内完成"的灵活场景
pub fn split_by_date_range(
    goal: &Goal,
    start_date: NaiveDate,
    end_date: NaiveDate,
    per_day_qty: Option<f64>,
    today: NaiveDate,
) -> AppResult<Vec<Task>> {
    if end_date < start_date {
        return Err(AppError::Param("结束日期不能早于起始日期".into()));
    }

    let total = goal.total_qty;
    if total <= 0.0 {
        return Err(AppError::Business("目标总量必须大于0才能拆解".into()));
    }

    // 日期范围内的可用天数（含起止）
    let span_days = (end_date - start_date).num_days() + 1;
    if span_days < 1 {
        return Err(AppError::Business(format!(
            "日期范围内仅有{}天，至少需要1天",
            span_days
        )));
    }

    let now = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();
    let mut tasks = Vec::new();

    match per_day_qty {
        Some(qty) if qty > 0.0 => {
            // 按每日数量：任务数 = ceil(total / qty)
            let num_tasks = (total / qty).ceil() as i64;
            if num_tasks == 0 {
                return Err(AppError::Business("计算任务数为0，无法拆解".into()));
            }
            if num_tasks > span_days {
                return Err(AppError::Business(format!(
                    "按每日{}{}拆解需要{}个任务，但日期范围内仅有{}天，请调整每日数量或日期范围",
                    qty, goal.unit, num_tasks, span_days
                )));
            }
            let mut allocated = 0.0;
            for i in 0..num_tasks {
                let remaining = total - allocated;
                let plan_qty = remaining.min(qty);
                allocated += plan_qty;
                let task_date = start_date + chrono::Duration::days(i);
                let task_id = Uuid::new_v4().to_string();
                let path = format!("/{}/{}", goal.id, task_id);
                tasks.push(Task {
                    id: task_id,
                    goal_id: goal.id.clone(),
                    stage_id: None,
                    parent_id: Some(goal.id.clone()),
                    path,
                    name: format!("{} - 第{}天", goal.name, i + 1),
                    plan_date: Some(task_date.format("%Y-%m-%d").to_string()),
                    overdue_date: None,
                    plan_qty,
                    actual_qty: 0.0,
                    unit: goal.unit.clone(),
                    status: "pending".to_string(),
                    is_manual: 0,
                    source: "auto".to_string(),
                    sort_order: i,
                    created_at: now.clone(),
                    estimated_hours: Some(qty),
                });
            }
        }
        _ => {
            // 未指定每日数量：按天数均分（余数分前几天），复用 split_goal_into_tasks 的分配逻辑
            let base = (total / span_days as f64).floor();
            let remainder = (total - base * span_days as f64).round() as i64;
            let mut day_index = 0;
            for i in 0..span_days {
                let plan_qty = if i < remainder { base + 1.0 } else { base };
                if plan_qty <= 0.0 {
                    continue;
                }
                let task_date = start_date + chrono::Duration::days(i);
                let task_id = Uuid::new_v4().to_string();
                let path = format!("/{}/{}", goal.id, task_id);
                day_index += 1;
                tasks.push(Task {
                    id: task_id,
                    goal_id: goal.id.clone(),
                    stage_id: None,
                    parent_id: Some(goal.id.clone()),
                    path,
                    name: format!("{} - 第{}天", goal.name, day_index),
                    plan_date: Some(task_date.format("%Y-%m-%d").to_string()),
                    overdue_date: None,
                    plan_qty,
                    actual_qty: 0.0,
                    unit: goal.unit.clone(),
                    status: "pending".to_string(),
                    is_manual: 0,
                    source: "auto".to_string(),
                    sort_order: i,
                    created_at: now.clone(),
                    estimated_hours: None,
                });
            }
        }
    }

    // 保留 today 参数用于未来扩展（如校验起始日期不早于今天）
    let _ = today;
    Ok(tasks)
}

/// 智能拆解调度：根据策略将输入参数转换为 Goal 覆盖并调用对应算法
///
/// 这是整合后的统一入口，三种策略分别复用原有算法：
/// - `by_deadline` → `split_goal_into_tasks`（用覆盖后的 total_qty/deadline）
/// - `by_capacity` → `split_by_daily_capacity`（用覆盖后的 total_qty/deadline/daily_capacity）
/// - `by_date_range` → `split_by_date_range`（用 start_date/end_date/per_day_qty）
///
/// 不会修改 goal 本身，仅构造临时覆盖的 Goal 副本供算法使用。
pub fn smart_split(input: &SmartSplitInput, goal: &Goal, today: NaiveDate) -> AppResult<Vec<Task>> {
    // 构造覆盖后的 Goal 副本（不写回数据库）
    let mut effective_goal = goal.clone();
    if let Some(qty) = input.total_qty {
        effective_goal.total_qty = qty;
    }
    if let Some(deadline) = &input.deadline {
        effective_goal.deadline = Some(deadline.clone());
    }
    if let Some(cap) = input.daily_capacity {
        effective_goal.daily_capacity = Some(cap);
    }

    match input.strategy.as_str() {
        "by_deadline" => {
            if effective_goal.total_qty <= 0.0 {
                return Err(AppError::Business("目标总量必须大于0才能拆解".into()));
            }
            split_goal_into_tasks(&effective_goal, today)
        }
        "by_capacity" => {
            if effective_goal.total_qty <= 0.0 {
                return Err(AppError::Business("目标总量必须大于0才能拆解".into()));
            }
            split_by_daily_capacity(&effective_goal, today)
        }
        "by_date_range" => {
            let start_str = input.start_date.as_deref().ok_or_else(|| {
                AppError::Param("自定义日期范围策略必须指定起始日期".into())
            })?;
            let end_str = input.end_date.as_deref().ok_or_else(|| {
                AppError::Param("自定义日期范围策略必须指定结束日期".into())
            })?;
            let start = NaiveDate::parse_from_str(start_str, "%Y-%m-%d")
                .map_err(|e| AppError::Param(format!("起始日期格式错误: {}", e)))?;
            let end = NaiveDate::parse_from_str(end_str, "%Y-%m-%d")
                .map_err(|e| AppError::Param(format!("结束日期格式错误: {}", e)))?;
            split_by_date_range(
                &effective_goal,
                start,
                end,
                input.per_day_qty,
                today,
            )
        }
        other => Err(AppError::Param(format!(
            "未知的拆解策略: {}（支持: by_deadline | by_capacity | by_date_range）",
            other
        ))),
    }
}

/// 计算重新规划的预览
///
/// PRD §4.2 模块四 & 分阶段计划 Sprint 2：
/// - 将目标下所有未完成任务（不含已跳过）按新的剩余天数重新平均分配
/// - 保留 is_manual=true 的任务计划数量不变
/// - 排除已跳过任务
/// - P1-1：按依赖链拓扑排序（被依赖的任务排前），余数优先分配给前置任务
///
/// 参数：
/// - goal: 目标
/// - unfinished_tasks: 未完成任务（调用方已过滤 skipped 和 done）
/// - today: 今天日期
/// - dep_map: 该目标内任务的依赖关系（task_id → 其 depends_on_id 列表）
pub fn build_replan_preview(
    goal: &Goal,
    unfinished_tasks: &[Task],
    today: NaiveDate,
    dep_map: &HashMap<String, Vec<String>>,
) -> AppResult<ReplanPreview> {
    let deadline_str = goal.deadline.as_ref().ok_or_else(|| {
        AppError::Param("目标未设置截止日期，无法重新规划".into())
    })?;
    let deadline = NaiveDate::parse_from_str(deadline_str, "%Y-%m-%d")
        .map_err(|e| AppError::Param(format!("截止日期格式错误: {}", e)))?;

    let remaining_days = (deadline - today).num_days();
    if remaining_days < 1 {
        return Err(AppError::Business(format!(
            "截止日期剩余天数不足（{}天），至少需要留出1天",
            remaining_days
        )));
    }

    // 待重新分配的任务：未完成且非手动修改
    let to_replan: Vec<&Task> = unfinished_tasks
        .iter()
        .filter(|t| t.is_manual == 0)
        .collect();

    // 手动修改的任务保留原计划数量
    let manual: Vec<&Task> = unfinished_tasks
        .iter()
        .filter(|t| t.is_manual == 1)
        .collect();

    // 剩余待分配总量 = 目标总量 - 已完成量 - 手动任务的计划量
    let completed_qty: f64 = 0.0; // 已完成任务不在 unfinished_tasks 中
    let manual_plan_sum: f64 = manual.iter().map(|t| t.plan_qty).sum();
    let remaining_qty = goal.total_qty - completed_qty - manual_plan_sum;

    if remaining_qty < 0.0 {
        return Err(AppError::Business(format!(
            "剩余待分配总量为负({})，可能已完成量超过目标总量，无法重新规划",
            remaining_qty
        )));
    }

    // 待重新分配的任务数量（决定分配到几天）
    let replan_days = to_replan.len() as i64;
    if replan_days == 0 {
        // 没有需要重新分配的任务
        let items = unfinished_tasks
            .iter()
            .map(|t| ReplanPreviewItem {
                task_id: t.id.clone(),
                name: t.name.clone(),
                plan_date: t.plan_date.clone().unwrap_or_default(),
                old_plan_qty: t.plan_qty,
                new_plan_qty: t.plan_qty,
                retained: t.is_manual == 1,
            })
            .collect();
        return Ok(ReplanPreview {
            goal_id: goal.id.clone(),
            goal_name: goal.name.clone(),
            remaining_days,
            remaining_qty,
            daily_base: 0.0,
            remainder: 0,
            items,
        });
    }

    // 平均分配：base = floor(remaining_qty / replan_days)，余数分到前几天
    let daily_base = (remaining_qty / replan_days as f64).floor();
    let remainder = (remaining_qty - daily_base * replan_days as f64).round() as i64;

    // 构建预览项
    let mut items: Vec<ReplanPreviewItem> = Vec::new();

    // 手动任务：保留
    for t in &manual {
        items.push(ReplanPreviewItem {
            task_id: t.id.clone(),
            name: t.name.clone(),
            plan_date: t.plan_date.clone().unwrap_or_default(),
            old_plan_qty: t.plan_qty,
            new_plan_qty: t.plan_qty,
            retained: true,
        });
    }

    // 待重新分配任务：按依赖链拓扑排序（被依赖的任务排前），余数优先分到前置任务
    // to_replan 是 Vec<&Task>，需转为 owned Vec<Task> 供拓扑排序使用
    let to_replan_owned: Vec<Task> = to_replan.iter().map(|t| (*t).clone()).collect();
    let replan_sorted = dependency_service::topo_sort_by_dependency(&to_replan_owned, dep_map);

    for (i, t) in replan_sorted.iter().enumerate() {
        let new_qty = if (i as i64) < remainder {
            daily_base + 1.0
        } else {
            daily_base
        };
        items.push(ReplanPreviewItem {
            task_id: t.id.clone(),
            name: t.name.clone(),
            plan_date: t.plan_date.clone().unwrap_or_default(),
            old_plan_qty: t.plan_qty,
            new_plan_qty: new_qty,
            retained: false,
        });
    }

    // 按 plan_date 排序输出
    items.sort_by(|a, b| a.plan_date.cmp(&b.plan_date));

    Ok(ReplanPreview {
        goal_id: goal.id.clone(),
        goal_name: goal.name.clone(),
        remaining_days,
        remaining_qty,
        daily_base,
        remainder,
        items,
    })
}

/// 重复拆解任务（纯文字类：按频率重复 or 单次）
///
/// - end_date=None 或等于 start_date → 生成单个任务
/// - end_date > start_date → 按 frequency 在日期范围内生成任务
///   - daily（默认）：每天生成一个
///   - weekly：命中 weekdays 集合的周几生成
///   - monthly：命中 month_days 集合的几号生成
/// - 任务的 plan_qty 固定（默认 1），unit 可选（默认空）
pub fn split_repeat_tasks(
    goal: &Goal,
    input: &RepeatSplitInput,
    _today: NaiveDate,
) -> AppResult<Vec<Task>> {
    if input.name.trim().is_empty() {
        return Err(AppError::Param("任务名称不能为空".into()));
    }

    let start = NaiveDate::parse_from_str(&input.start_date, "%Y-%m-%d")
        .map_err(|e| AppError::Param(format!("起始日期格式错误: {}", e)))?;

    let plan_qty = input.plan_qty.unwrap_or(1.0);
    let unit = input.unit.clone().unwrap_or_default();
    let now = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();

    // 判断是否重复
    let end = match &input.end_date {
        Some(end_str) => {
            let end_date = NaiveDate::parse_from_str(end_str, "%Y-%m-%d")
                .map_err(|e| AppError::Param(format!("结束日期格式错误: {}", e)))?;
            if end_date < start {
                return Err(AppError::Param("结束日期不能早于起始日期".into()));
            }
            end_date
        }
        None => start, // 无结束日期 → 单次
    };

    // 频率解析与校验
    let frequency = input
        .frequency
        .as_deref()
        .unwrap_or("daily")
        .to_lowercase();
    let is_single = start == end;

    // 单次任务不校验频率参数
    if !is_single {
        match frequency.as_str() {
            "weekly" => {
                if input.weekdays.as_ref().map_or(true, |v| v.is_empty()) {
                    return Err(AppError::Param(
                        "weekly 频率必须指定至少一个周几".into(),
                    ));
                }
            }
            "monthly" => {
                if input.month_days.as_ref().map_or(true, |v| v.is_empty()) {
                    return Err(AppError::Param(
                        "monthly 频率必须指定至少一个日期".into(),
                    ));
                }
            }
            _ => {}
        }
    }

    // 命中判定：给定 cursor 是否应生成任务
    let should_generate = |cursor: NaiveDate| -> bool {
        if is_single {
            return true;
        }
        match frequency.as_str() {
            "weekly" => {
                let wd = cursor.weekday().num_days_from_sunday() as u8;
                input
                    .weekdays
                    .as_ref()
                    .map_or(false, |set| set.contains(&wd))
            }
            "monthly" => {
                let d = cursor.day() as u8;
                input
                    .month_days
                    .as_ref()
                    .map_or(false, |set| set.contains(&d))
            }
            _ => true, // daily
        }
    };

    let mut tasks = Vec::new();
    let mut seq_index = 0; // 实际生成任务的序号
    let mut cursor = start;

    while cursor <= end {
        if should_generate(cursor) {
            seq_index += 1;
            let task_id = Uuid::new_v4().to_string();
            let path = format!("/{}/{}", goal.id, task_id);

            let name = if is_single {
                input.name.clone()
            } else {
                format!("{} - 第{}次", input.name, seq_index)
            };

            tasks.push(Task {
                id: task_id,
                goal_id: goal.id.clone(),
                stage_id: None,
                parent_id: Some(goal.id.clone()),
                path,
                name,
                plan_date: Some(cursor.format("%Y-%m-%d").to_string()),
                overdue_date: None,
                plan_qty,
                actual_qty: 0.0,
                unit: unit.clone(),
                status: "pending".to_string(),
                is_manual: 0,
                source: "auto".to_string(),
                sort_order: (seq_index - 1) as i64,
                created_at: now.clone(),
                estimated_hours: None,
            });
        }

        cursor += chrono::Duration::days(1);
    }

    Ok(tasks)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_goal(id: &str, name: &str, deadline: &str, total_qty: f64, unit: &str) -> Goal {
        Goal {
            id: id.to_string(),
            name: name.to_string(),
            parent_id: None,
            path: format!("/{}", id),
            deadline: Some(deadline.to_string()),
            total_qty,
            unit: unit.to_string(),
            sort_order: 0,
            created_at: "2026-06-24T00:00:00".to_string(),
            daily_capacity: None,
        }
    }

    #[test]
    fn test_split_even() {
        // 10 个任务，5 天，每天 2 个
        let goal = make_goal("g1", "背单词", "2026-06-29", 10.0, "个");
        let today = NaiveDate::from_ymd_opt(2026, 6, 24).unwrap();
        let tasks = split_goal_into_tasks(&goal, today).unwrap();
        assert_eq!(tasks.len(), 5);
        for t in &tasks {
            assert_eq!(t.plan_qty, 2.0);
        }
        // 日期从 6/25 到 6/29
        assert_eq!(tasks[0].plan_date, Some("2026-06-25".to_string()));
        assert_eq!(tasks[4].plan_date, Some("2026-06-29".to_string()));
    }

    #[test]
    fn test_split_with_remainder() {
        // 10 个任务，3 天：4 + 3 + 3
        let goal = make_goal("g2", "看书", "2026-06-27", 10.0, "页");
        let today = NaiveDate::from_ymd_opt(2026, 6, 24).unwrap();
        let tasks = split_goal_into_tasks(&goal, today).unwrap();
        assert_eq!(tasks.len(), 3);
        assert_eq!(tasks[0].plan_qty, 4.0); // 余数 1 分到第1天
        assert_eq!(tasks[1].plan_qty, 3.0);
        assert_eq!(tasks[2].plan_qty, 3.0);
    }

    #[test]
    fn test_split_zero_days() {
        let goal = make_goal("g3", "过期", "2026-06-24", 10.0, "个");
        let today = NaiveDate::from_ymd_opt(2026, 6, 24).unwrap();
        let result = split_goal_into_tasks(&goal, today);
        assert!(result.is_err());
    }

    /// P1-3 验收标准：Given 目标总量 40 小时,每日可用 2 小时
    /// When 按时间预算拆解 Then 生成 20 个任务,每个 plan_qty=2,跨越 20 天
    #[test]
    fn test_split_by_daily_capacity_acceptance() {
        let mut goal = make_goal("g4", "学Vue", "2026-07-15", 40.0, "小时");
        goal.daily_capacity = Some(2.0);
        // today=2026-06-24, deadline=2026-07-15 → 剩余 21 天，足够 20 个任务
        let today = NaiveDate::from_ymd_opt(2026, 6, 24).unwrap();

        let tasks = split_by_daily_capacity(&goal, today).unwrap();
        assert_eq!(tasks.len(), 20, "应生成 20 个任务");
        for (i, t) in tasks.iter().enumerate() {
            if i < 19 {
                assert_eq!(t.plan_qty, 2.0, "前 19 个任务 plan_qty 应为 2");
            } else {
                assert_eq!(t.plan_qty, 2.0, "最后一个任务 plan_qty 也应为 2（40/2 整除）");
            }
            assert_eq!(
                t.estimated_hours,
                Some(2.0),
                "每个任务 estimated_hours 应为 2"
            );
        }
        // 跨越 20 天：从 6/25 到 7/14
        assert_eq!(tasks[0].plan_date, Some("2026-06-25".to_string()));
        assert_eq!(tasks[19].plan_date, Some("2026-07-14".to_string()));
    }

    #[test]
    fn test_split_by_daily_capacity_with_remainder() {
        // 总量 10，每日 3 → ceil(10/3)=4 个任务：3 + 3 + 3 + 1
        let mut goal = make_goal("g5", "读书", "2026-07-10", 10.0, "页");
        goal.daily_capacity = Some(3.0);
        let today = NaiveDate::from_ymd_opt(2026, 6, 24).unwrap();

        let tasks = split_by_daily_capacity(&goal, today).unwrap();
        assert_eq!(tasks.len(), 4);
        assert_eq!(tasks[0].plan_qty, 3.0);
        assert_eq!(tasks[1].plan_qty, 3.0);
        assert_eq!(tasks[2].plan_qty, 3.0);
        assert_eq!(tasks[3].plan_qty, 1.0, "最后一个任务应为余数 1");
        assert_eq!(tasks[3].estimated_hours, Some(3.0));
    }

    #[test]
    fn test_split_by_daily_capacity_no_capacity() {
        let goal = make_goal("g6", "无容量", "2026-07-10", 10.0, "页");
        let today = NaiveDate::from_ymd_opt(2026, 6, 24).unwrap();
        let result = split_by_daily_capacity(&goal, today);
        assert!(result.is_err(), "未设置 daily_capacity 应报错");
    }

    #[test]
    fn test_split_by_daily_capacity_insufficient_days() {
        // 总量 40，每日 2 → 需要 20 天，但剩余仅 5 天
        let mut goal = make_goal("g7", "不够天", "2026-06-29", 40.0, "小时");
        goal.daily_capacity = Some(2.0);
        let today = NaiveDate::from_ymd_opt(2026, 6, 24).unwrap();
        let result = split_by_daily_capacity(&goal, today);
        assert!(result.is_err(), "剩余天数不足应报错");
    }

    // ===== split_by_date_range 测试 =====

    #[test]
    fn test_split_by_date_range_with_per_day_qty() {
        // 日期范围 6/25 ~ 7/4（10天），总量 20，每日 2 → 10 个任务
        let goal = make_goal("g8", "读书", "2026-07-31", 20.0, "页");
        let start = NaiveDate::from_ymd_opt(2026, 6, 25).unwrap();
        let end = NaiveDate::from_ymd_opt(2026, 7, 4).unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 6, 24).unwrap();

        let tasks = split_by_date_range(&goal, start, end, Some(2.0), today).unwrap();
        assert_eq!(tasks.len(), 10);
        for t in &tasks {
            assert_eq!(t.plan_qty, 2.0);
            assert_eq!(t.estimated_hours, Some(2.0));
        }
        assert_eq!(tasks[0].plan_date, Some("2026-06-25".to_string()));
        assert_eq!(tasks[9].plan_date, Some("2026-07-04".to_string()));
    }

    #[test]
    fn test_split_by_date_range_even_split() {
        // 日期范围 6/25 ~ 6/29（5天），总量 10，无每日数量 → 均分每天 2
        let goal = make_goal("g9", "背词", "2026-07-31", 10.0, "词");
        let start = NaiveDate::from_ymd_opt(2026, 6, 25).unwrap();
        let end = NaiveDate::from_ymd_opt(2026, 6, 29).unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 6, 24).unwrap();

        let tasks = split_by_date_range(&goal, start, end, None, today).unwrap();
        assert_eq!(tasks.len(), 5);
        for t in &tasks {
            assert_eq!(t.plan_qty, 2.0);
        }
    }

    #[test]
    fn test_split_by_date_range_with_remainder() {
        // 日期范围 6/25 ~ 6/29（5天），总量 12，无每日数量 → base=2, remainder=2 → 3+3+2+2+2
        let goal = make_goal("g10", "练习", "2026-07-31", 12.0, "道");
        let start = NaiveDate::from_ymd_opt(2026, 6, 25).unwrap();
        let end = NaiveDate::from_ymd_opt(2026, 6, 29).unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 6, 24).unwrap();

        let tasks = split_by_date_range(&goal, start, end, None, today).unwrap();
        assert_eq!(tasks.len(), 5);
        assert_eq!(tasks[0].plan_qty, 3.0);
        assert_eq!(tasks[1].plan_qty, 3.0);
        assert_eq!(tasks[2].plan_qty, 2.0);
        assert_eq!(tasks[3].plan_qty, 2.0);
        assert_eq!(tasks[4].plan_qty, 2.0);
    }

    #[test]
    fn test_split_by_date_range_per_day_exceeds_span() {
        // 日期范围 6/25 ~ 6/27（3天），总量 20，每日 2 → 需要 10 天，超出范围
        let goal = make_goal("g11", "超范围", "2026-07-31", 20.0, "页");
        let start = NaiveDate::from_ymd_opt(2026, 6, 25).unwrap();
        let end = NaiveDate::from_ymd_opt(2026, 6, 27).unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 6, 24).unwrap();
        let result = split_by_date_range(&goal, start, end, Some(2.0), today);
        assert!(result.is_err(), "任务数超出日期范围应报错");
    }

    #[test]
    fn test_split_by_date_range_end_before_start() {
        let goal = make_goal("g12", "倒序", "2026-07-31", 10.0, "页");
        let start = NaiveDate::from_ymd_opt(2026, 6, 29).unwrap();
        let end = NaiveDate::from_ymd_opt(2026, 6, 25).unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 6, 24).unwrap();
        let result = split_by_date_range(&goal, start, end, None, today);
        assert!(result.is_err(), "结束日期早于起始日期应报错");
    }

    // ===== smart_split 调度测试 =====

    #[test]
    fn test_smart_split_by_deadline() {
        let goal = make_goal("g13", "均分", "2026-06-29", 10.0, "个");
        let today = NaiveDate::from_ymd_opt(2026, 6, 24).unwrap();
        let input = SmartSplitInput {
            goal_id: "g13".to_string(),
            strategy: "by_deadline".to_string(),
            total_qty: None,
            deadline: None,
            daily_capacity: None,
            start_date: None,
            end_date: None,
            per_day_qty: None,
        };
        let tasks = smart_split(&input, &goal, today).unwrap();
        // 剩余 5 天，10 个 → 每天 2
        assert_eq!(tasks.len(), 5);
        assert_eq!(tasks[0].plan_qty, 2.0);
    }

    #[test]
    fn test_smart_split_by_capacity_with_override() {
        // 目标未设置 daily_capacity，通过 smart_split 临时覆盖
        let goal = make_goal("g14", "预算", "2026-07-15", 40.0, "小时");
        let today = NaiveDate::from_ymd_opt(2026, 6, 24).unwrap();
        let input = SmartSplitInput {
            goal_id: "g14".to_string(),
            strategy: "by_capacity".to_string(),
            total_qty: None,
            deadline: None,
            daily_capacity: Some(2.0),
            start_date: None,
            end_date: None,
            per_day_qty: None,
        };
        let tasks = smart_split(&input, &goal, today).unwrap();
        assert_eq!(tasks.len(), 20);
        assert_eq!(tasks[0].plan_qty, 2.0);
        assert_eq!(tasks[0].estimated_hours, Some(2.0));
    }

    #[test]
    fn test_smart_split_by_date_range() {
        let goal = make_goal("g15", "范围", "2026-07-31", 20.0, "页");
        let today = NaiveDate::from_ymd_opt(2026, 6, 24).unwrap();
        let input = SmartSplitInput {
            goal_id: "g15".to_string(),
            strategy: "by_date_range".to_string(),
            total_qty: None,
            deadline: None,
            daily_capacity: None,
            start_date: Some("2026-06-25".to_string()),
            end_date: Some("2026-07-04".to_string()),
            per_day_qty: Some(2.0),
        };
        let tasks = smart_split(&input, &goal, today).unwrap();
        assert_eq!(tasks.len(), 10);
        assert_eq!(tasks[0].plan_date, Some("2026-06-25".to_string()));
        assert_eq!(tasks[9].plan_date, Some("2026-07-04".to_string()));
    }

    #[test]
    fn test_smart_split_unknown_strategy() {
        let goal = make_goal("g16", "未知", "2026-07-31", 10.0, "个");
        let today = NaiveDate::from_ymd_opt(2026, 6, 24).unwrap();
        let input = SmartSplitInput {
            goal_id: "g16".to_string(),
            strategy: "unknown".to_string(),
            total_qty: None,
            deadline: None,
            daily_capacity: None,
            start_date: None,
            end_date: None,
            per_day_qty: None,
        };
        let result = smart_split(&input, &goal, today);
        assert!(result.is_err(), "未知策略应报错");
    }
}
