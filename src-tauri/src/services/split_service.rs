use chrono::{Datelike, NaiveDate};
use uuid::Uuid;

use crate::db::models::{Goal, ReplanPreview, ReplanPreviewItem, RepeatSplitInput, Task};
use crate::error::{AppError, AppResult};

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
        });
    }

    Ok(tasks)
}

/// 计算重新规划的预览
///
/// PRD §4.2 模块四 & 分阶段计划 Sprint 2：
/// - 将目标下所有未完成任务（不含已跳过）按新的剩余天数重新平均分配
/// - 保留 is_manual=true 的任务计划数量不变
/// - 排除已跳过任务
///
/// 参数：
/// - goal: 目标
/// - unfinished_tasks: 未完成任务（调用方已过滤 skipped 和 done）
/// - manual_tasks: 手动修改过的任务（is_manual=1），这些将被保留
/// - today: 今天日期
pub fn build_replan_preview(
    goal: &Goal,
    unfinished_tasks: &[Task],
    today: NaiveDate,
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

    // 待重新分配任务：按 sort_order 排序后分配新数量
    let mut replan_sorted = to_replan.clone();
    replan_sorted.sort_by_key(|t| t.sort_order);

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
}
