use std::collections::HashMap;
use tauri::State;
use uuid::Uuid;

use crate::db::models::{
    CreateGoalInput, Goal, GoalTreeNode, ProgressInfo, ReplanPreview, ReplanResult, RepeatSplitInput,
    Task, UpdateGoalInput,
};
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::services::{progress_service, split_service};

/// 创建目标（总目标或子目标）
///
/// parent_id=None → 总目标；parent_id=Some → 子目标
#[tauri::command]
pub async fn create_goal(input: CreateGoalInput, state: State<'_, DbPool>) -> AppResult<Goal> {
    let id = Uuid::new_v4().to_string();
    let now = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();
    let total_qty = input.total_qty.unwrap_or(0.0);
    let unit = input.unit.unwrap_or_default();

    // 构建 path 和 sort_order
    let (path, sort_order) = match &input.parent_id {
        Some(parent_id) => {
            // 子目标：校验父目标存在
            let parent: Goal = sqlx::query_as("SELECT * FROM goals WHERE id = ?")
                .bind(parent_id)
                .fetch_optional(&state.0)
                .await?
                .ok_or_else(|| AppError::NotFound(format!("父目标 {} 不存在", parent_id)))?;
            let count: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM goals WHERE parent_id = ?")
                    .bind(parent_id)
                    .fetch_one(&state.0)
                    .await?;
            (format!("{}/{}", parent.path, id), count)
        }
        None => {
            // 总目标
            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM goals WHERE parent_id IS NULL")
                .fetch_one(&state.0)
                .await?;
            (format!("/{}", id), count)
        }
    };

    sqlx::query(
        "INSERT INTO goals (id, name, parent_id, path, deadline, total_qty, unit, sort_order, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&input.name)
    .bind(&input.parent_id)
    .bind(&path)
    .bind(&input.deadline)
    .bind(total_qty)
    .bind(&unit)
    .bind(sort_order)
    .bind(&now)
    .execute(&state.0)
    .await?;

    let goal: Goal = sqlx::query_as("SELECT * FROM goals WHERE id = ?")
        .bind(&id)
        .fetch_one(&state.0)
        .await?;

    Ok(goal)
}

/// 列出所有目标（扁平）
#[tauri::command]
pub async fn list_goals(state: State<'_, DbPool>) -> AppResult<Vec<Goal>> {
    let goals: Vec<Goal> =
        sqlx::query_as("SELECT * FROM goals ORDER BY sort_order, created_at")
            .fetch_all(&state.0)
            .await?;
    Ok(goals)
}

/// 获取目标树（递归嵌套：总目标 → 子目标 → 子任务）
#[tauri::command]
pub async fn list_goal_tree(state: State<'_, DbPool>) -> AppResult<Vec<GoalTreeNode>> {
    // 1. 查询所有目标
    let all_goals: Vec<Goal> =
        sqlx::query_as("SELECT * FROM goals ORDER BY sort_order, created_at")
            .fetch_all(&state.0)
            .await?;

    // 2. 查询所有任务
    let all_tasks: Vec<Task> =
        sqlx::query_as("SELECT * FROM tasks ORDER BY plan_date, sort_order")
            .fetch_all(&state.0)
            .await?;

    // 3. 按目标分组任务
    let mut tasks_by_goal: HashMap<String, Vec<Task>> = HashMap::new();
    for task in all_tasks {
        tasks_by_goal.entry(task.goal_id.clone()).or_default().push(task);
    }

    // 4. 查询所有目标的进度
    let progresses = progress_service::calc_all_goals_progress(&state.0).await?;
    let progress_map: HashMap<String, ProgressInfo> =
        progresses.into_iter().map(|p| (p.id.clone(), p)).collect();

    // 5. 递归构建树（从根目标开始）
    let roots: Vec<Goal> = all_goals.iter().filter(|g| g.parent_id.is_none()).cloned().collect();
    let all_goals_map: HashMap<String, Goal> =
        all_goals.into_iter().map(|g| (g.id.clone(), g)).collect();

    let tree = build_tree_nodes(&roots, &all_goals_map, &tasks_by_goal, &progress_map);
    Ok(tree)
}

/// 递归构建目标树节点
fn build_tree_nodes(
    goals: &[Goal],
    all_goals: &HashMap<String, Goal>,
    tasks_by_goal: &HashMap<String, Vec<Task>>,
    progress_map: &HashMap<String, ProgressInfo>,
) -> Vec<GoalTreeNode> {
    goals
        .iter()
        .map(|goal| {
            // 找到子目标
            let sub_goals: Vec<Goal> = all_goals
                .values()
                .filter(|g| g.parent_id.as_deref() == Some(&goal.id))
                .cloned()
                .collect();

            let sub_goal_nodes = build_tree_nodes(&sub_goals, all_goals, tasks_by_goal, progress_map);

            let tasks = tasks_by_goal.get(&goal.id).cloned().unwrap_or_default();

            let progress_info = progress_map.get(&goal.id);
            let progress = progress_info.map(|p| p.percentage).unwrap_or(0.0);
            let is_completed = progress_info.map(|p| p.is_completed).unwrap_or(false);

            GoalTreeNode {
                goal: goal.clone(),
                sub_goals: sub_goal_nodes,
                tasks,
                progress,
                is_completed,
            }
        })
        .collect()
}

/// 获取单个目标
#[tauri::command]
pub async fn get_goal(id: String, state: State<'_, DbPool>) -> AppResult<Goal> {
    let goal: Goal = sqlx::query_as("SELECT * FROM goals WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.0)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("目标 {} 不存在", id)))?;
    Ok(goal)
}

/// 更新目标（名称、截止日期、总量、单位）
#[tauri::command]
pub async fn update_goal(input: UpdateGoalInput, state: State<'_, DbPool>) -> AppResult<Goal> {
    let mut updates: Vec<String> = Vec::new();
    if input.name.is_some() {
        updates.push("name = ?".to_string());
    }
    if input.deadline.is_some() {
        updates.push("deadline = ?".to_string());
    }
    if input.total_qty.is_some() {
        updates.push("total_qty = ?".to_string());
    }
    if input.unit.is_some() {
        updates.push("unit = ?".to_string());
    }

    if updates.is_empty() {
        return Err(AppError::Param("未提供任何更新字段".into()));
    }

    let sql = format!("UPDATE goals SET {} WHERE id = ?", updates.join(", "));
    let mut q = sqlx::query(&sql);
    if let Some(name) = &input.name {
        q = q.bind(name);
    }
    if let Some(deadline) = &input.deadline {
        q = q.bind(deadline);
    }
    if let Some(total_qty) = input.total_qty {
        q = q.bind(total_qty);
    }
    if let Some(unit) = &input.unit {
        q = q.bind(unit);
    }
    q = q.bind(&input.id);
    q.execute(&state.0).await?;

    let goal: Goal = sqlx::query_as("SELECT * FROM goals WHERE id = ?")
        .bind(&input.id)
        .fetch_one(&state.0)
        .await?;
    Ok(goal)
}

/// 删除目标（级联删除所有子目标和任务）
#[tauri::command]
pub async fn delete_goal(id: String, state: State<'_, DbPool>) -> AppResult<()> {
    // 递归收集所有后代目标 ID（含自身）
    let mut to_delete: Vec<String> = vec![id.clone()];
    let mut queue = vec![id.clone()];
    while let Some(current) = queue.pop() {
        let children: Vec<String> =
            sqlx::query_scalar("SELECT id FROM goals WHERE parent_id = ?")
                .bind(&current)
                .fetch_all(&state.0)
                .await?;
        for child in children {
            to_delete.push(child.clone());
            queue.push(child);
        }
    }

    // 删除所有关联任务
    for goal_id in &to_delete {
        sqlx::query("DELETE FROM tasks WHERE goal_id = ?")
            .bind(goal_id)
            .execute(&state.0)
            .await?;
    }

    // 删除所有后代目标
    for goal_id in &to_delete {
        sqlx::query("DELETE FROM goals WHERE id = ?")
            .bind(goal_id)
            .execute(&state.0)
            .await?;
    }

    Ok(())
}

/// 自动拆解目标为每日任务（视频/数量拆解类）
///
/// 根据 goal.total_qty 和 goal.deadline 按剩余天数平均分配
#[tauri::command]
pub async fn auto_split(goal_id: String, state: State<'_, DbPool>) -> AppResult<Vec<Task>> {
    let goal: Goal = sqlx::query_as("SELECT * FROM goals WHERE id = ?")
        .bind(&goal_id)
        .fetch_optional(&state.0)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("目标 {} 不存在", goal_id)))?;

    // 检查是否已有自动拆解任务，避免重复
    let existing: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM tasks WHERE goal_id = ? AND source = 'auto'")
            .bind(&goal_id)
            .fetch_one(&state.0)
            .await?;

    if existing > 0 {
        return Err(AppError::Business(
            "该目标已有自动拆解任务，请先删除旧任务再重新拆解".into(),
        ));
    }

    // 执行拆解算法
    let today = chrono::Local::now().date_naive();
    let tasks = split_service::split_goal_into_tasks(&goal, today)?;

    // 批量插入任务
    for task in &tasks {
        sqlx::query(
            "INSERT INTO tasks (id, goal_id, stage_id, parent_id, path, name, plan_date,
             plan_qty, actual_qty, unit, status, is_manual, source, sort_order, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&task.id)
        .bind(&task.goal_id)
        .bind(&task.stage_id)
        .bind(&task.parent_id)
        .bind(&task.path)
        .bind(&task.name)
        .bind(&task.plan_date)
        .bind(task.plan_qty)
        .bind(task.actual_qty)
        .bind(&task.unit)
        .bind(&task.status)
        .bind(task.is_manual)
        .bind(&task.source)
        .bind(task.sort_order)
        .bind(&task.created_at)
        .execute(&state.0)
        .await?;
    }

    Ok(tasks)
}

/// 重复拆解（纯文字类任务：每天重复 or 单次）
///
/// - end_date=None 或等于 start_date → 单次任务
/// - end_date > start_date → 每天生成一个重复任务
#[tauri::command]
pub async fn repeat_split(input: RepeatSplitInput, state: State<'_, DbPool>) -> AppResult<Vec<Task>> {
    let goal: Goal = sqlx::query_as("SELECT * FROM goals WHERE id = ?")
        .bind(&input.goal_id)
        .fetch_optional(&state.0)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("目标 {} 不存在", input.goal_id)))?;

    let today = chrono::Local::now().date_naive();
    let tasks = split_service::split_repeat_tasks(&goal, &input, today)?;

    for task in &tasks {
        sqlx::query(
            "INSERT INTO tasks (id, goal_id, stage_id, parent_id, path, name, plan_date,
             plan_qty, actual_qty, unit, status, is_manual, source, sort_order, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&task.id)
        .bind(&task.goal_id)
        .bind(&task.stage_id)
        .bind(&task.parent_id)
        .bind(&task.path)
        .bind(&task.name)
        .bind(&task.plan_date)
        .bind(task.plan_qty)
        .bind(task.actual_qty)
        .bind(&task.unit)
        .bind(&task.status)
        .bind(task.is_manual)
        .bind(&task.source)
        .bind(task.sort_order)
        .bind(&task.created_at)
        .execute(&state.0)
        .await?;
    }

    Ok(tasks)
}

/// 重新规划预览
#[tauri::command]
pub async fn replan_preview(goal_id: String, state: State<'_, DbPool>) -> AppResult<ReplanPreview> {
    let goal: Goal = sqlx::query_as("SELECT * FROM goals WHERE id = ?")
        .bind(&goal_id)
        .fetch_optional(&state.0)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("目标 {} 不存在", goal_id)))?;

    let unfinished: Vec<Task> = sqlx::query_as(
        "SELECT * FROM tasks WHERE goal_id = ? AND status IN ('pending', 'partial') ORDER BY sort_order",
    )
    .bind(&goal_id)
    .fetch_all(&state.0)
    .await?;

    let today = chrono::Local::now().date_naive();
    let preview = split_service::build_replan_preview(&goal, &unfinished, today)?;
    Ok(preview)
}

/// 执行重新规划
#[tauri::command]
pub async fn replan_goal(goal_id: String, state: State<'_, DbPool>) -> AppResult<ReplanResult> {
    let goal: Goal = sqlx::query_as("SELECT * FROM goals WHERE id = ?")
        .bind(&goal_id)
        .fetch_optional(&state.0)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("目标 {} 不存在", goal_id)))?;

    let unfinished: Vec<Task> = sqlx::query_as(
        "SELECT * FROM tasks WHERE goal_id = ? AND status IN ('pending', 'partial') ORDER BY sort_order",
    )
    .bind(&goal_id)
    .fetch_all(&state.0)
    .await?;

    let today = chrono::Local::now().date_naive();
    let preview = split_service::build_replan_preview(&goal, &unfinished, today)?;

    let mut updated_count = 0usize;
    let mut retained_count = 0usize;

    for item in &preview.items {
        if item.retained {
            retained_count += 1;
            continue;
        }
        sqlx::query("UPDATE tasks SET plan_qty = ? WHERE id = ?")
            .bind(item.new_plan_qty)
            .bind(&item.task_id)
            .execute(&state.0)
            .await?;
        updated_count += 1;
    }

    let updated_tasks: Vec<Task> = sqlx::query_as(
        "SELECT * FROM tasks WHERE goal_id = ? AND status IN ('pending', 'partial') ORDER BY plan_date, sort_order",
    )
    .bind(&goal_id)
    .fetch_all(&state.0)
    .await?;

    Ok(ReplanResult {
        goal_id: goal_id.clone(),
        updated_count,
        retained_count,
        tasks: updated_tasks,
    })
}
