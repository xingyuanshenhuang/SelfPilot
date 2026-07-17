use std::collections::HashMap;
use tauri::State;
use uuid::Uuid;

use crate::db::models::{
    CreateGoalInput, Goal, GoalTreeNode, MoveGoalInput, ProgressInfo, ReplanPreview, ReplanResult,
    RepeatSplitInput, SmartSplitInput, Task, UpdateGoalInput,
};
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::services::{dependency_service, progress_service, split_service};

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
        "INSERT INTO goals (id, name, parent_id, path, deadline, total_qty, unit, sort_order, created_at, daily_capacity)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
    .bind(input.daily_capacity)
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
    // 注意：必须直接在已排序的 Vec 上 filter，不能收集进 HashMap
    // （Rust HashMap 不保证迭代顺序，会导致子目标顺序随机抖动）
    let roots: Vec<Goal> = all_goals.iter().filter(|g| g.parent_id.is_none()).cloned().collect();

    let tree = build_tree_nodes(&roots, &all_goals, &tasks_by_goal, &progress_map);
    Ok(tree)
}

/// 递归构建目标树节点
///
/// `all_goals` 必须保留 SQL `ORDER BY sort_order, created_at` 的排序，
/// 在其上 filter 出子目标可保证子目标顺序稳定。
fn build_tree_nodes(
    goals: &[Goal],
    all_goals: &[Goal],
    tasks_by_goal: &HashMap<String, Vec<Task>>,
    progress_map: &HashMap<String, ProgressInfo>,
) -> Vec<GoalTreeNode> {
    goals
        .iter()
        .map(|goal| {
            // 找到子目标（保留 all_goals 的排序）
            let sub_goals: Vec<Goal> = all_goals
                .iter()
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
    if input.daily_capacity.is_some() {
        updates.push("daily_capacity = ?".to_string());
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
    if let Some(cap) = input.daily_capacity {
        q = q.bind(cap);
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
    let mut tx = state.0.begin().await?;

    // 递归收集所有后代目标 ID（含自身）
    let mut to_delete: Vec<String> = vec![id.clone()];
    let mut queue = vec![id.clone()];
    while let Some(current) = queue.pop() {
        let children: Vec<String> =
            sqlx::query_scalar("SELECT id FROM goals WHERE parent_id = ?")
                .bind(&current)
                .fetch_all(&mut *tx)
                .await?;
        for child in children {
            to_delete.push(child.clone());
            queue.push(child);
        }
    }

    // 清理依赖关系：任一端属于将被删除任务的依赖记录都删除
    // SQLite 默认未启用外键级联，需显式清理 task_dependencies
    for goal_id in &to_delete {
        sqlx::query(
            "DELETE FROM task_dependencies
             WHERE task_id IN (SELECT id FROM tasks WHERE goal_id = ?)
                OR depends_on_id IN (SELECT id FROM tasks WHERE goal_id = ?)",
        )
        .bind(goal_id)
        .bind(goal_id)
        .execute(&mut *tx)
        .await?;
    }

    // 删除所有关联任务
    for goal_id in &to_delete {
        sqlx::query("DELETE FROM tasks WHERE goal_id = ?")
            .bind(goal_id)
            .execute(&mut *tx)
            .await?;
    }

    // 删除所有后代目标
    for goal_id in &to_delete {
        sqlx::query("DELETE FROM goals WHERE id = ?")
            .bind(goal_id)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;
    Ok(())
}

/// 自动拆解目标为每日任务（视频/数量拆解类）
///
/// 根据 goal.total_qty 和 goal.deadline 按剩余天数平均分配
#[tauri::command]
pub async fn auto_split(goal_id: String, state: State<'_, DbPool>) -> AppResult<Vec<Task>> {
    let mut tx = state.0.begin().await?;

    let goal: Goal = sqlx::query_as("SELECT * FROM goals WHERE id = ?")
        .bind(&goal_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("目标 {} 不存在", goal_id)))?;

    // 检查是否已有自动拆解任务，避免重复
    let existing: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM tasks WHERE goal_id = ? AND source = 'auto'")
            .bind(&goal_id)
            .fetch_one(&mut *tx)
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
             plan_qty, actual_qty, unit, status, is_manual, source, sort_order, created_at, estimated_hours)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
        .bind(task.estimated_hours)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(tasks)
}

/// 按时间预算拆解目标为每日任务（P1-3 时间预算模型）
///
/// 与 `auto_split` 不同，此命令按 `goal.daily_capacity`（每日可用时长）决定每个任务的计划量，
/// 而非按剩余天数平均分配总量。
/// - 任务数 = ceil(total_qty / daily_capacity)
/// - 每个任务 plan_qty = daily_capacity（最后一个可能不足）
/// - 每个任务 estimated_hours = daily_capacity
/// - 任务从明天开始，逐日生成，每天一个任务
#[tauri::command]
pub async fn split_by_capacity(goal_id: String, state: State<'_, DbPool>) -> AppResult<Vec<Task>> {
    let mut tx = state.0.begin().await?;

    let goal: Goal = sqlx::query_as("SELECT * FROM goals WHERE id = ?")
        .bind(&goal_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("目标 {} 不存在", goal_id)))?;

    // 检查是否已有自动拆解任务，避免重复
    let existing: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM tasks WHERE goal_id = ? AND source = 'auto'")
            .bind(&goal_id)
            .fetch_one(&mut *tx)
            .await?;

    if existing > 0 {
        return Err(AppError::Business(
            "该目标已有自动拆解任务，请先删除旧任务再重新拆解".into(),
        ));
    }

    // 执行按时间预算拆解算法
    let today = chrono::Local::now().date_naive();
    let tasks = split_service::split_by_daily_capacity(&goal, today)?;

    // 批量插入任务
    for task in &tasks {
        sqlx::query(
            "INSERT INTO tasks (id, goal_id, stage_id, parent_id, path, name, plan_date,
             plan_qty, actual_qty, unit, status, is_manual, source, sort_order, created_at, estimated_hours)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
        .bind(task.estimated_hours)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(tasks)
}

/// 重复拆解（纯文字类任务：每天重复 or 单次）
///
/// - end_date=None 或等于 start_date → 单次任务
/// - end_date > start_date → 每天生成一个重复任务
#[tauri::command]
pub async fn repeat_split(input: RepeatSplitInput, state: State<'_, DbPool>) -> AppResult<Vec<Task>> {
    let mut tx = state.0.begin().await?;

    let goal: Goal = sqlx::query_as("SELECT * FROM goals WHERE id = ?")
        .bind(&input.goal_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("目标 {} 不存在", input.goal_id)))?;

    let today = chrono::Local::now().date_naive();
    let tasks = split_service::split_repeat_tasks(&goal, &input, today)?;

    for task in &tasks {
        sqlx::query(
            "INSERT INTO tasks (id, goal_id, stage_id, parent_id, path, name, plan_date,
             plan_qty, actual_qty, unit, status, is_manual, source, sort_order, created_at, estimated_hours)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
        .bind(task.estimated_hours)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(tasks)
}

/// 智能拆解（整合入口：按截止日期均分 / 按时间预算 / 自定义日期范围）
///
/// 三种策略统一入口，参数可临时覆盖目标属性（不修改目标本身）。
/// 详见 `split_service::smart_split`。
#[tauri::command]
pub async fn smart_split(input: SmartSplitInput, state: State<'_, DbPool>) -> AppResult<Vec<Task>> {
    let mut tx = state.0.begin().await?;

    let goal: Goal = sqlx::query_as("SELECT * FROM goals WHERE id = ?")
        .bind(&input.goal_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("目标 {} 不存在", input.goal_id)))?;

    // 检查是否已有自动拆解任务，避免重复
    let existing: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM tasks WHERE goal_id = ? AND source = 'auto'")
            .bind(&input.goal_id)
            .fetch_one(&mut *tx)
            .await?;

    if existing > 0 {
        return Err(AppError::Business(
            "该目标已有自动拆解任务，请先删除旧任务再重新拆解".into(),
        ));
    }

    // 执行智能拆解（根据策略分发到对应算法）
    let today = chrono::Local::now().date_naive();
    let tasks = split_service::smart_split(&input, &goal, today)?;

    // 批量插入任务
    for task in &tasks {
        sqlx::query(
            "INSERT INTO tasks (id, goal_id, stage_id, parent_id, path, name, plan_date,
             plan_qty, actual_qty, unit, status, is_manual, source, sort_order, created_at, estimated_hours)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
        .bind(task.estimated_hours)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
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
    let dep_map = dependency_service::load_goal_dependency_map(&state.0, &goal_id).await?;
    let preview = split_service::build_replan_preview(&goal, &unfinished, today, &dep_map)?;
    Ok(preview)
}

/// 执行重新规划
#[tauri::command]
pub async fn replan_goal(goal_id: String, state: State<'_, DbPool>) -> AppResult<ReplanResult> {
    // 在事务前查询依赖映射（基于已提交数据，本地单用户场景无竞态）
    let dep_map = dependency_service::load_goal_dependency_map(&state.0, &goal_id).await?;

    let mut tx = state.0.begin().await?;

    let goal: Goal = sqlx::query_as("SELECT * FROM goals WHERE id = ?")
        .bind(&goal_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("目标 {} 不存在", goal_id)))?;

    let unfinished: Vec<Task> = sqlx::query_as(
        "SELECT * FROM tasks WHERE goal_id = ? AND status IN ('pending', 'partial') ORDER BY sort_order",
    )
    .bind(&goal_id)
    .fetch_all(&mut *tx)
    .await?;

    let today = chrono::Local::now().date_naive();
    let preview = split_service::build_replan_preview(&goal, &unfinished, today, &dep_map)?;

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
            .execute(&mut *tx)
            .await?;
        updated_count += 1;
    }

    let updated_tasks: Vec<Task> = sqlx::query_as(
        "SELECT * FROM tasks WHERE goal_id = ? AND status IN ('pending', 'partial') ORDER BY plan_date, sort_order",
    )
    .bind(&goal_id)
    .fetch_all(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(ReplanResult {
        goal_id: goal_id.clone(),
        updated_count,
        retained_count,
        tasks: updated_tasks,
    })
}

/// 移动目标（跨层级归属调整与同级排序）
///
/// - 更新 parent_id、path，并递归更新所有后代目标的 path
/// - 环检测：新父目标不能是自身或自身后代
/// - sort_order：若指定 before_goal_id 则插入其前，否则追加到新父下末尾
#[tauri::command]
pub async fn move_goal(input: MoveGoalInput, state: State<'_, DbPool>) -> AppResult<Goal> {
    let goal: Goal = sqlx::query_as("SELECT * FROM goals WHERE id = ?")
        .bind(&input.goal_id)
        .fetch_optional(&state.0)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("目标 {} 不存在", input.goal_id)))?;

    // 不允许移动到自身
    if let Some(ref pid) = input.new_parent_id {
        if pid == &goal.id {
            return Err(AppError::Business("不能将目标移动到自身下".into()));
        }
        // 环检测：新父目标不能是自身后代
        let mut queue = vec![goal.id.clone()];
        while let Some(current) = queue.pop() {
            let children: Vec<String> =
                sqlx::query_scalar("SELECT id FROM goals WHERE parent_id = ?")
                    .bind(&current)
                    .fetch_all(&state.0)
                    .await?;
            if children.iter().any(|c| c == pid) {
                return Err(AppError::Business(
                    "不能将目标移动到其子目标下（会形成循环）".into(),
                ));
            }
            queue.extend(children);
        }
        // 校验新父目标存在
        let _parent: Goal = sqlx::query_as("SELECT * FROM goals WHERE id = ?")
            .bind(pid)
            .fetch_optional(&state.0)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("父目标 {} 不存在", pid)))?;
    }

    // 计算新 path
    let new_path = match &input.new_parent_id {
        Some(pid) => {
            let parent: Goal = sqlx::query_as("SELECT * FROM goals WHERE id = ?")
                .bind(pid)
                .fetch_one(&state.0)
                .await?;
            format!("{}/{}", parent.path, goal.id)
        }
        None => format!("/{}", goal.id),
    };

    // 计算 sort_order
    let new_sort_order: i64 = if let Some(before_id) = &input.before_goal_id {
        let before: Goal = sqlx::query_as("SELECT * FROM goals WHERE id = ?")
            .bind(before_id)
            .fetch_optional(&state.0)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("前置目标 {} 不存在", before_id)))?;
        // 校验：前置目标必须与新目标同级（同一 parent_id）
        if before.parent_id != input.new_parent_id {
            return Err(AppError::Param(
                "前置目标与新父目标不同级，无法排序".into(),
            ));
        }
        let target_sort = before.sort_order;
        // 将同级中 >= target_sort 的目标后移（排除被移动目标自身）
        if input.new_parent_id.is_some() {
            sqlx::query(
                "UPDATE goals SET sort_order = sort_order + 1
                 WHERE parent_id = ? AND sort_order >= ? AND id != ?",
            )
            .bind(&input.new_parent_id)
            .bind(target_sort)
            .bind(&goal.id)
            .execute(&state.0)
            .await?;
        } else {
            sqlx::query(
                "UPDATE goals SET sort_order = sort_order + 1
                 WHERE parent_id IS NULL AND sort_order >= ? AND id != ?",
            )
            .bind(target_sort)
            .bind(&goal.id)
            .execute(&state.0)
            .await?;
        }
        target_sort
    } else {
        // 追加到末尾
        let count: i64 = match &input.new_parent_id {
            Some(pid) => {
                sqlx::query_scalar("SELECT COUNT(*) FROM goals WHERE parent_id = ? AND id != ?")
                    .bind(pid)
                    .bind(&goal.id)
                    .fetch_one(&state.0)
                    .await?
            }
            None => {
                sqlx::query_scalar("SELECT COUNT(*) FROM goals WHERE parent_id IS NULL AND id != ?")
                    .bind(&goal.id)
                    .fetch_one(&state.0)
                    .await?
            }
        };
        count
    };

    // 更新被移动目标的 parent_id、path、sort_order
    sqlx::query("UPDATE goals SET parent_id = ?, path = ?, sort_order = ? WHERE id = ?")
        .bind(&input.new_parent_id)
        .bind(&new_path)
        .bind(new_sort_order)
        .bind(&goal.id)
        .execute(&state.0)
        .await?;

    // 递归更新所有后代目标的 path（parent_id 不变，仅 path 前缀变化）
    update_descendant_paths(&goal.id, &new_path, &state.0).await?;

    let updated: Goal = sqlx::query_as("SELECT * FROM goals WHERE id = ?")
        .bind(&goal.id)
        .fetch_one(&state.0)
        .await?;
    Ok(updated)
}

/// 递归更新后代目标的 path
///
/// 给定父目标 id 和新 path，查询其直接子目标，
/// 将每个子目标的 path 更新为 `{parent_path}/{child_id}`，再递归处理子目标的子目标。
async fn update_descendant_paths(
    parent_id: &str,
    parent_path: &str,
    pool: &sqlx::SqlitePool,
) -> AppResult<()> {
    let children: Vec<Goal> = sqlx::query_as("SELECT * FROM goals WHERE parent_id = ?")
        .bind(parent_id)
        .fetch_all(pool)
        .await?;

    for child in children {
        let child_path = format!("{}/{}", parent_path, child.id);
        sqlx::query("UPDATE goals SET path = ? WHERE id = ?")
            .bind(&child_path)
            .bind(&child.id)
            .execute(pool)
            .await?;
        // 递归更新子目标的子目标
        Box::pin(update_descendant_paths(&child.id, &child_path, pool)).await?;
    }

    Ok(())
}
