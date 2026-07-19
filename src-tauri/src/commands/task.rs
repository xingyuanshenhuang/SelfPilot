use tauri::State;
use uuid::Uuid;

use crate::db::models::{
    CalendarTask, CompleteTaskInput, CreateTaskInput, DeleteTaskResult, DeleteTasksBatchResult,
    Goal, MoveTaskInput, SetTaskDependencyInput, Task, TaskDependency, TodayTask, UpdateTaskInput,
};
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::services::dependency_service;

/// 手动创建任务
#[tauri::command]
pub async fn create_task(input: CreateTaskInput, state: State<'_, DbPool>) -> AppResult<Task> {
    let id = Uuid::new_v4().to_string();
    let now = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();
    let plan_qty = input.plan_qty.unwrap_or(1.0);
    let unit = input.unit.unwrap_or_default();
    let path = format!("/{}/{}", input.goal_id, id);

    sqlx::query(
        "INSERT INTO tasks (id, goal_id, stage_id, parent_id, path, name, plan_date,
         plan_qty, actual_qty, unit, status, is_manual, source, sort_order, created_at, estimated_hours)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&input.goal_id)
    .bind(&input.stage_id)
    .bind(&input.goal_id)
    .bind(&path)
    .bind(&input.name)
    .bind(&input.plan_date)
    .bind(plan_qty)
    .bind(0.0)
    .bind(&unit)
    .bind("pending")
    .bind(1)
    .bind("manual")
    .bind(0)
    .bind(&now)
    .bind(None::<f64>)
    .execute(&state.0)
    .await?;

    let task: Task = sqlx::query_as("SELECT * FROM tasks WHERE id = ?")
        .bind(&id)
        .fetch_one(&state.0)
        .await?;

    Ok(task)
}

/// 完成任务（支持部分完成）
///
/// PRD §4.2 模块二：
/// - actual_qty >= plan_qty → done
/// - 0 < actual_qty < plan_qty → partial
/// - 禁止对已完成任务重复标记
#[tauri::command]
pub async fn complete_task(
    input: CompleteTaskInput,
    state: State<'_, DbPool>,
) -> AppResult<Task> {
    if input.actual_qty < 0.0 {
        return Err(AppError::Param("实际完成量不能为负".into()));
    }

    let task: Task = sqlx::query_as("SELECT * FROM tasks WHERE id = ?")
        .bind(&input.task_id)
        .fetch_optional(&state.0)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("任务 {} 不存在", input.task_id)))?;

    // 禁止对已完成任务再次标记完成
    if task.status == "done" {
        return Err(AppError::Business("任务已完成，不能重复标记".into()));
    }

    // P1-1：存在未完成的前置依赖时禁止完成
    if dependency_service::has_unfinished_dependencies(&state.0, &input.task_id).await? {
        return Err(AppError::Business("前置任务未完成，暂不能标记完成".into()));
    }

    // 校验不超过计划数量
    if input.actual_qty > task.plan_qty {
        return Err(AppError::Business(format!(
            "实际完成量({})不能超过计划数量({})",
            input.actual_qty, task.plan_qty
        )));
    }

    // 计算新状态
    let new_status = if input.actual_qty >= task.plan_qty {
        "done"
    } else if input.actual_qty > 0.0 {
        "partial"
    } else {
        "pending"
    };

    sqlx::query("UPDATE tasks SET actual_qty = ?, status = ? WHERE id = ?")
        .bind(input.actual_qty)
        .bind(new_status)
        .bind(&input.task_id)
        .execute(&state.0)
        .await?;

    let updated: Task = sqlx::query_as("SELECT * FROM tasks WHERE id = ?")
        .bind(&input.task_id)
        .fetch_one(&state.0)
        .await?;

    Ok(updated)
}

/// 跳过任务
///
/// PRD §4.2 模块二：标记为"已跳过"，不计入完成，不影响后续计划
#[tauri::command]
pub async fn skip_task(task_id: String, state: State<'_, DbPool>) -> AppResult<Task> {
    let _task: Task = sqlx::query_as("SELECT * FROM tasks WHERE id = ?")
        .bind(&task_id)
        .fetch_optional(&state.0)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("任务 {} 不存在", task_id)))?;

    sqlx::query("UPDATE tasks SET status = 'skipped' WHERE id = ?")
        .bind(&task_id)
        .execute(&state.0)
        .await?;

    let updated: Task = sqlx::query_as("SELECT * FROM tasks WHERE id = ?")
        .bind(&task_id)
        .fetch_one(&state.0)
        .await?;

    Ok(updated)
}

/// 列出今日待办任务（今日计划任务，不含跳过）
#[tauri::command]
pub async fn list_today_tasks(state: State<'_, DbPool>) -> AppResult<Vec<TodayTask>> {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();

    let tasks: Vec<TodayTask> = sqlx::query_as(
        &format!(
            "SELECT t.id, t.goal_id, g.name as goal_name, t.stage_id, t.name,
                    t.plan_date, t.overdue_date, t.plan_qty, t.actual_qty, t.unit, t.status, t.source,
                    {} as is_blocked,
                    {} as blocked_by_names
             FROM tasks t
             JOIN goals g ON t.goal_id = g.id
             WHERE t.plan_date = ? AND t.status != 'skipped'
             ORDER BY t.sort_order",
            dependency_service::BLOCKED_EXISTS_SQL,
            dependency_service::BLOCKED_BY_NAMES_SQL
        ),
    )
    .bind(&today)
    .fetch_all(&state.0)
    .await?;

    Ok(tasks)
}

/// 列出逾期未完成任务（截止日期早于今日且未完成）
///
/// 数据校验与写入：
/// - 逾期任务的 overdue_date 以 plan_date 为准
/// - 首次查询到逾期且 overdue_date 为空时，自动补写入库
/// - 若已存储的 overdue_date 与 plan_date 不一致，自动修正
#[tauri::command]
pub async fn list_overdue_tasks(state: State<'_, DbPool>) -> AppResult<Vec<TodayTask>> {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();

    let mut tasks: Vec<TodayTask> = sqlx::query_as(
        &format!(
            "SELECT t.id, t.goal_id, g.name as goal_name, t.stage_id, t.name,
                    t.plan_date, t.overdue_date, t.plan_qty, t.actual_qty, t.unit, t.status, t.source,
                    {} as is_blocked,
                    {} as blocked_by_names
             FROM tasks t
             JOIN goals g ON t.goal_id = g.id
             WHERE t.plan_date < ? AND t.status IN ('pending', 'partial')
             ORDER BY COALESCE(t.overdue_date, t.plan_date), t.plan_date",
            dependency_service::BLOCKED_EXISTS_SQL,
            dependency_service::BLOCKED_BY_NAMES_SQL
        ),
    )
    .bind(&today)
    .fetch_all(&state.0)
    .await?;

    for task in &mut tasks {
        if let Some(plan_date) = &task.plan_date {
            let expected = Some(plan_date.clone());
            if task.overdue_date != expected {
                sqlx::query("UPDATE tasks SET overdue_date = ? WHERE id = ?")
                    .bind(plan_date)
                    .bind(&task.id)
                    .execute(&state.0)
                    .await?;
                task.overdue_date = expected;
            }
        }
    }

    Ok(tasks)
}

/// 列出某目标下所有任务
#[tauri::command]
pub async fn list_tasks_by_goal(
    goal_id: String,
    state: State<'_, DbPool>,
) -> AppResult<Vec<Task>> {
    let tasks: Vec<Task> = sqlx::query_as(
        "SELECT * FROM tasks WHERE goal_id = ? ORDER BY plan_date, sort_order",
    )
    .bind(&goal_id)
    .fetch_all(&state.0)
    .await?;

    Ok(tasks)
}

/// 补完成（历史任务）
///
/// PRD §4.2 模块四 & 分阶段计划 Sprint 2：
/// - 对已跳过/逾期的历史任务补录实际完成量
/// - 只更新 actual_qty 和状态，不触发任何重新分配算法
/// - 允许对 skipped 状态的任务补完成（区别于 complete_task 禁止 done 重复标记）
/// - 补完成后不联动未来任务
#[tauri::command]
pub async fn backfill_task(
    input: CompleteTaskInput,
    state: State<'_, DbPool>,
) -> AppResult<Task> {
    if input.actual_qty < 0.0 {
        return Err(AppError::Param("实际完成量不能为负".into()));
    }

    let task: Task = sqlx::query_as("SELECT * FROM tasks WHERE id = ?")
        .bind(&input.task_id)
        .fetch_optional(&state.0)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("任务 {} 不存在", input.task_id)))?;

    // 已完成且补完成量不超过原完成量时禁止重复标记
    if task.status == "done" && input.actual_qty <= task.actual_qty {
        return Err(AppError::Business("任务已完成，不能重复补录".into()));
    }

    // P1-1：存在未完成的前置依赖时禁止补完成
    if dependency_service::has_unfinished_dependencies(&state.0, &input.task_id).await? {
        return Err(AppError::Business("前置任务未完成，暂不能补完成".into()));
    }

    // 补完成允许超过原计划数量（用户可能补录超额完成）
    let new_actual = input.actual_qty;

    // 计算新状态（补完成可以把 skipped/pending 变为 partial/done）
    let new_status = if new_actual >= task.plan_qty && task.plan_qty > 0.0 {
        "done"
    } else if new_actual > 0.0 {
        "partial"
    } else {
        "pending"
    };

    // 只更新 actual_qty 和 status，绝不触碰 plan_qty 或其他任务的计划数量
    sqlx::query("UPDATE tasks SET actual_qty = ?, status = ? WHERE id = ?")
        .bind(new_actual)
        .bind(new_status)
        .bind(&input.task_id)
        .execute(&state.0)
        .await?;

    let updated: Task = sqlx::query_as("SELECT * FROM tasks WHERE id = ?")
        .bind(&input.task_id)
        .fetch_one(&state.0)
        .await?;

    Ok(updated)
}

/// 移动任务（支持跨目标归属调整、阶段移动、同级排序）
///
/// - `goal_id=Some` → 跨目标移动（拖拽归属）：更新 goal_id、parent_id、path，清空 stage_id
/// - `goal_id=None & stage_id=Some` → 阶段内移动：仅更新 stage_id、path
/// - `before_task_id=Some` → 插入到该任务之前（同级排序）
/// - `before_task_id=None & goal_id=Some` → 放置到目标直属任务列表最前面
/// - 校验：目标 goal 必须存在；新 goal_id 不能与当前相同（除非同时指定 before_task_id 排序）
#[tauri::command]
pub async fn move_task(input: MoveTaskInput, state: State<'_, DbPool>) -> AppResult<Task> {
    let task: Task = sqlx::query_as("SELECT * FROM tasks WHERE id = ?")
        .bind(&input.task_id)
        .fetch_optional(&state.0)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("任务 {} 不存在", input.task_id)))?;

    let cross_goal = input.goal_id.is_some();
    let new_goal_id = match &input.goal_id {
        Some(gid) => {
            // 校验目标存在
            let _goal: Goal = sqlx::query_as("SELECT * FROM goals WHERE id = ?")
                .bind(gid)
                .fetch_optional(&state.0)
                .await?
                .ok_or_else(|| AppError::NotFound(format!("目标 {} 不存在", gid)))?;
            gid.clone()
        }
        None => task.goal_id.clone(),
    };

    // 计算新 stage_id：跨目标移动时清空（旧 stage 属于旧 goal），否则按入参
    let new_stage_id: Option<String> = if cross_goal {
        None
    } else {
        input.stage_id.clone()
    };

    // 计算新 path：始终基于新 goal_id 重建
    let new_path = match &new_stage_id {
        Some(stage_id) => format!("/{}/{}/{}", new_goal_id, stage_id, input.task_id),
        None => format!("/{}/{}", new_goal_id, input.task_id),
    };

    // 计算 sort_order
    let new_sort_order: i64 = if let Some(before_id) = &input.before_task_id {
        // 插入到 before_task 之前：取 before 的 sort_order，将 >= 该值的其他任务后移
        let before: Task = sqlx::query_as("SELECT * FROM tasks WHERE id = ?")
            .bind(before_id)
            .fetch_optional(&state.0)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("前置任务 {} 不存在", before_id)))?;
        if before.goal_id != new_goal_id {
            return Err(AppError::Param(
                "前置任务不属于目标目标，无法排序".into(),
            ));
        }
        let target_sort = before.sort_order;
        sqlx::query(
            "UPDATE tasks SET sort_order = sort_order + 1
             WHERE goal_id = ? AND sort_order >= ? AND id != ?",
        )
        .bind(&new_goal_id)
        .bind(target_sort)
        .bind(&input.task_id)
        .execute(&state.0)
        .await?;
        target_sort
    } else {
        // 无前置：追加到末尾（跨目标或同目标均适用）
        let max_sort: Option<i64> =
            sqlx::query_scalar("SELECT MAX(sort_order) FROM tasks WHERE goal_id = ? AND id != ?")
                .bind(&new_goal_id)
                .bind(&input.task_id)
                .fetch_one(&state.0)
                .await?;
        max_sort.unwrap_or(-1) + 1
    };

    if cross_goal {
        // 跨目标移动：更新 goal_id、parent_id（指向新 goal）、stage_id、path、sort_order
        sqlx::query(
            "UPDATE tasks SET goal_id = ?, parent_id = ?, stage_id = ?, path = ?, sort_order = ? WHERE id = ?",
        )
        .bind(&new_goal_id)
        .bind(&new_goal_id)
        .bind(None::<String>)
        .bind(&new_path)
        .bind(new_sort_order)
        .bind(&input.task_id)
        .execute(&state.0)
        .await?;
    } else if input.before_task_id.is_some() {
        // 同目标排序：仅更新 sort_order（及可能的 stage_id/path）
        sqlx::query("UPDATE tasks SET stage_id = ?, path = ?, sort_order = ? WHERE id = ?")
            .bind(&new_stage_id)
            .bind(&new_path)
            .bind(new_sort_order)
            .bind(&input.task_id)
            .execute(&state.0)
            .await?;
    } else {
        // 同目标追加到末尾：更新 sort_order（及可能的 stage_id/path）
        sqlx::query("UPDATE tasks SET stage_id = ?, path = ?, sort_order = ? WHERE id = ?")
            .bind(&new_stage_id)
            .bind(&new_path)
            .bind(new_sort_order)
            .bind(&input.task_id)
            .execute(&state.0)
            .await?;
    }

    let updated: Task = sqlx::query_as("SELECT * FROM tasks WHERE id = ?")
        .bind(&input.task_id)
        .fetch_one(&state.0)
        .await?;

    Ok(updated)
}

/// 更新任务的计划数量（手动调整单日计划）
///
/// 设置 is_manual = 1，重新规划时会保留手动修改项
#[tauri::command]
pub async fn update_task_plan_qty(
    task_id: String,
    plan_qty: f64,
    state: State<'_, DbPool>,
) -> AppResult<Task> {
    if plan_qty < 0.0 {
        return Err(AppError::Param("计划数量不能为负".into()));
    }

    sqlx::query("UPDATE tasks SET plan_qty = ?, is_manual = 1 WHERE id = ?")
        .bind(plan_qty)
        .bind(&task_id)
        .execute(&state.0)
        .await?;

    let updated: Task = sqlx::query_as("SELECT * FROM tasks WHERE id = ?")
        .bind(&task_id)
        .fetch_one(&state.0)
        .await?;

    Ok(updated)
}

/// 更新任务（通用：名称、计划日期、计划数量）
///
/// PRD §4.2 模块二 & 分阶段计划 Sprint 2：
/// - 支持修改任务名称、计划日期、计划数量
/// - 修改 plan_qty 时自动标记 is_manual = 1，重新规划时保留
/// - 修改 plan_qty 时若任务已完成(actual_qty > plan_qty)则截断 actual_qty
#[tauri::command]
pub async fn update_task(input: UpdateTaskInput, state: State<'_, DbPool>) -> AppResult<Task> {
    let task: Task = sqlx::query_as("SELECT * FROM tasks WHERE id = ?")
        .bind(&input.task_id)
        .fetch_optional(&state.0)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("任务 {} 不存在", input.task_id)))?;

    let mut updates: Vec<String> = Vec::new();
    let mut mark_manual = false;

    if let Some(name) = &input.name {
        if name.trim().is_empty() {
            return Err(AppError::Param("任务名称不能为空".into()));
        }
        updates.push("name = ?".to_string());
    }

    if let Some(plan_date) = &input.plan_date {
        // 允许空字符串表示清除日期；计划日期变更后，逾期日期需重新计算
        updates.push("plan_date = ?".to_string());
        updates.push("overdue_date = NULL".to_string());
        let _ = plan_date; // 绑定在下方动态处理
    }

    if let Some(plan_qty) = input.plan_qty {
        if plan_qty < 0.0 {
            return Err(AppError::Param("计划数量不能为负".into()));
        }
        updates.push("plan_qty = ?".to_string());
        mark_manual = true;
    }

    if mark_manual {
        updates.push("is_manual = 1".to_string());
    }

    if updates.is_empty() {
        return Err(AppError::Param("未提供任何更新字段".into()));
    }

    let sql = format!("UPDATE tasks SET {} WHERE id = ?", updates.join(", "));

    // 动态绑定参数
    let mut q = sqlx::query(&sql);
    if let Some(name) = &input.name {
        q = q.bind(name);
    }
    if let Some(plan_date) = &input.plan_date {
        if plan_date.is_empty() {
            q = q.bind::<Option<String>>(None);
        } else {
            q = q.bind(plan_date);
        }
    }
    if let Some(plan_qty) = input.plan_qty {
        q = q.bind(plan_qty);
    }
    q = q.bind(&input.task_id);
    q.execute(&state.0).await?;

    // 若 plan_qty 变小且小于 actual_qty，截断 actual_qty 并重算状态
    if let Some(plan_qty) = input.plan_qty {
        if task.actual_qty > plan_qty {
            let new_status = if plan_qty > 0.0 && task.actual_qty >= plan_qty {
                "done"
            } else if task.actual_qty > 0.0 {
                "partial"
            } else {
                "pending"
            };
            sqlx::query("UPDATE tasks SET actual_qty = ?, status = ? WHERE id = ?")
                .bind(plan_qty)
                .bind(new_status)
                .bind(&input.task_id)
                .execute(&state.0)
                .await?;
        }
    }

    let updated: Task = sqlx::query_as("SELECT * FROM tasks WHERE id = ?")
        .bind(&input.task_id)
        .fetch_one(&state.0)
        .await?;

    Ok(updated)
}

/// 删除任务
///
/// 同时清理该任务相关的依赖关系（作为前置或后继）。
/// SQLite 默认未启用外键级联，需显式删除 task_dependencies。
///
/// P2-3：返回被删任务所属 goal_id，供前端局部更新进度（无需全量重拉）。
#[tauri::command]
pub async fn delete_task(task_id: String, state: State<'_, DbPool>) -> AppResult<DeleteTaskResult> {
    let mut tx = state.0.begin().await?;

    // 删除前查询 goal_id（供前端局部更新进度）
    let goal_id: String = sqlx::query_scalar("SELECT goal_id FROM tasks WHERE id = ?")
        .bind(&task_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("任务 {} 不存在", task_id)))?;

    // 清理依赖：删除该任务作为 task_id 或 depends_on_id 的所有记录
    sqlx::query("DELETE FROM task_dependencies WHERE task_id = ? OR depends_on_id = ?")
        .bind(&task_id)
        .bind(&task_id)
        .execute(&mut *tx)
        .await?;

    sqlx::query("DELETE FROM tasks WHERE id = ?")
        .bind(&task_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(DeleteTaskResult { task_id, goal_id })
}

/// 批量删除任务（单事务，避免 N 次 IPC 调用撑爆通道）
///
/// 同时清理相关依赖关系。用于"删除全部关联项"等场景。
///
/// P2-3：返回受影响的 goal_id 列表（去重），供前端局部更新进度。
#[tauri::command]
pub async fn delete_tasks_batch(
    task_ids: Vec<String>,
    state: State<'_, DbPool>,
) -> AppResult<DeleteTasksBatchResult> {
    if task_ids.is_empty() {
        return Ok(DeleteTasksBatchResult {
            deleted_count: 0,
            affected_goal_ids: vec![],
        });
    }
    let mut tx = state.0.begin().await?;

    // 删除前查询所有受影响的 goal_id（去重）
    let mut affected_goal_ids: Vec<String> = Vec::new();
    for id in &task_ids {
        let goal_id: Option<String> =
            sqlx::query_scalar("SELECT goal_id FROM tasks WHERE id = ?")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?;
        if let Some(gid) = goal_id {
            if !affected_goal_ids.contains(&gid) {
                affected_goal_ids.push(gid);
            }
        }
    }

    // 批量清理依赖
    for id in &task_ids {
        sqlx::query("DELETE FROM task_dependencies WHERE task_id = ? OR depends_on_id = ?")
            .bind(id)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }

    // 批量删除任务
    let mut deleted = 0i64;
    for id in &task_ids {
        let r = sqlx::query("DELETE FROM tasks WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        deleted += r.rows_affected() as i64;
    }

    tx.commit().await?;
    Ok(DeleteTasksBatchResult {
        deleted_count: deleted,
        affected_goal_ids,
    })
}

/// 按日期范围查询任务（日历视图用）
///
/// PRD §4.2 模块五 & 分阶段计划 Sprint 3：
/// - 返回日期范围内的所有任务，附带目标名称
/// - 标记 is_overdue：plan_date < today 且 status IN ('pending','partial')
/// - 按 plan_date, sort_order 排序
#[tauri::command]
pub async fn list_tasks_by_date_range(
    start_date: String,
    end_date: String,
    state: State<'_, DbPool>,
) -> AppResult<Vec<CalendarTask>> {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();

    let mut tasks: Vec<CalendarTask> = sqlx::query_as(
        &format!(
            "SELECT t.id, t.goal_id, g.name as goal_name, t.stage_id, t.name,
                    t.plan_date, t.plan_qty, t.actual_qty, t.unit, t.status, t.source,
                    0 as is_overdue,
                    {} as is_blocked,
                    {} as blocked_by_names
             FROM tasks t
             JOIN goals g ON t.goal_id = g.id
             WHERE t.plan_date IS NOT NULL
               AND t.plan_date >= ? AND t.plan_date <= ?
             ORDER BY t.plan_date, t.sort_order",
            dependency_service::BLOCKED_EXISTS_SQL,
            dependency_service::BLOCKED_BY_NAMES_SQL
        ),
    )
    .bind(&start_date)
    .bind(&end_date)
    .fetch_all(&state.0)
    .await?;

    // 在应用层计算 is_overdue（SQLite 不便直接返回布尔）
    for task in &mut tasks {
        if let Some(plan_date) = &task.plan_date {
            if plan_date.as_str() < today.as_str()
                && (task.status == "pending" || task.status == "partial")
            {
                task.is_overdue = true;
            }
        }
    }

    Ok(tasks)
}

// ===== P1-1 任务依赖关系 =====

/// 设置任务依赖（task_id 依赖 depends_on_id）
///
/// 业务规则：
/// - 校验两个任务均存在
/// - 禁止自依赖
/// - 通过 DFS 检测防止循环依赖
/// - 重复依赖静默忽略（INSERT OR IGNORE）
#[tauri::command]
pub async fn set_task_dependency(
    input: SetTaskDependencyInput,
    state: State<'_, DbPool>,
) -> AppResult<()> {
    if input.task_id == input.depends_on_id {
        return Err(AppError::Param("任务不能依赖自身".into()));
    }

    // 校验两个任务均存在
    let _task: Task = sqlx::query_as("SELECT * FROM tasks WHERE id = ?")
        .bind(&input.task_id)
        .fetch_optional(&state.0)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("任务 {} 不存在", input.task_id)))?;
    let _dep: Task = sqlx::query_as("SELECT * FROM tasks WHERE id = ?")
        .bind(&input.depends_on_id)
        .fetch_optional(&state.0)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("前置任务 {} 不存在", input.depends_on_id)))?;

    // 环检测：添加该依赖是否会形成循环
    if dependency_service::detect_cycle(&state.0, &input.task_id, &input.depends_on_id).await? {
        return Err(AppError::Business(
            "添加该依赖会形成循环依赖，已拒绝".into(),
        ));
    }

    let id = Uuid::new_v4().to_string();
    let now = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();
    // 重复依赖静默忽略
    sqlx::query(
        "INSERT OR IGNORE INTO task_dependencies (id, task_id, depends_on_id, created_at)
         VALUES (?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&input.task_id)
    .bind(&input.depends_on_id)
    .bind(&now)
    .execute(&state.0)
    .await?;

    Ok(())
}

/// 列出某任务的直接前置依赖任务
#[tauri::command]
pub async fn list_task_dependencies(
    task_id: String,
    state: State<'_, DbPool>,
) -> AppResult<Vec<Task>> {
    let tasks = dependency_service::list_dependencies(&state.0, &task_id).await?;
    Ok(tasks)
}

/// 列出依赖某任务的后继任务（哪些任务依赖 task_id）
#[tauri::command]
pub async fn list_task_dependents(
    task_id: String,
    state: State<'_, DbPool>,
) -> AppResult<Vec<Task>> {
    let tasks = dependency_service::list_dependents(&state.0, &task_id).await?;
    Ok(tasks)
}

/// 移除任务依赖
#[tauri::command]
pub async fn remove_task_dependency(
    task_id: String,
    depends_on_id: String,
    state: State<'_, DbPool>,
) -> AppResult<()> {
    sqlx::query("DELETE FROM task_dependencies WHERE task_id = ? AND depends_on_id = ?")
        .bind(&task_id)
        .bind(&depends_on_id)
        .execute(&state.0)
        .await?;
    Ok(())
}

/// 校验依赖链无环（添加 task_id 依赖 depends_on_id 是否会形成循环）
///
/// 返回 true 表示安全（无环），false 表示会形成循环。
#[tauri::command]
pub async fn validate_dependency_chain(
    task_id: String,
    depends_on_id: String,
    state: State<'_, DbPool>,
) -> AppResult<bool> {
    let has_cycle =
        dependency_service::detect_cycle(&state.0, &task_id, &depends_on_id).await?;
    Ok(!has_cycle)
}

/// 列出某任务的所有依赖记录（含 id、created_at）
#[tauri::command]
pub async fn list_task_dependency_records(
    task_id: String,
    state: State<'_, DbPool>,
) -> AppResult<Vec<TaskDependency>> {
    let records: Vec<TaskDependency> = sqlx::query_as(
        "SELECT * FROM task_dependencies WHERE task_id = ? ORDER BY created_at",
    )
    .bind(&task_id)
    .fetch_all(&state.0)
    .await?;
    Ok(records)
}
