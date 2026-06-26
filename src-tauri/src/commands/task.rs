use tauri::State;
use uuid::Uuid;

use crate::db::models::{CalendarTask, CompleteTaskInput, CreateTaskInput, MoveTaskInput, Task, TodayTask, UpdateTaskInput};
use crate::db::DbPool;
use crate::error::{AppError, AppResult};

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
         plan_qty, actual_qty, unit, status, is_manual, source, sort_order, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
        "SELECT t.id, t.goal_id, g.name as goal_name, t.stage_id, t.name,
                t.plan_date, t.plan_qty, t.actual_qty, t.unit, t.status, t.source
         FROM tasks t
         JOIN goals g ON t.goal_id = g.id
         WHERE t.plan_date = ? AND t.status != 'skipped'
         ORDER BY t.sort_order",
    )
    .bind(&today)
    .fetch_all(&state.0)
    .await?;

    Ok(tasks)
}

/// 列出逾期未完成任务（截止日期早于今日且未完成）
#[tauri::command]
pub async fn list_overdue_tasks(state: State<'_, DbPool>) -> AppResult<Vec<TodayTask>> {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();

    let tasks: Vec<TodayTask> = sqlx::query_as(
        "SELECT t.id, t.goal_id, g.name as goal_name, t.stage_id, t.name,
                t.plan_date, t.plan_qty, t.actual_qty, t.unit, t.status, t.source
         FROM tasks t
         JOIN goals g ON t.goal_id = g.id
         WHERE t.plan_date < ? AND t.status IN ('pending', 'partial')
         ORDER BY t.plan_date",
    )
    .bind(&today)
    .fetch_all(&state.0)
    .await?;

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

/// 移动任务到指定阶段（或移出阶段）
#[tauri::command]
pub async fn move_task(input: MoveTaskInput, state: State<'_, DbPool>) -> AppResult<Task> {
    let task: Task = sqlx::query_as("SELECT * FROM tasks WHERE id = ?")
        .bind(&input.task_id)
        .fetch_optional(&state.0)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("任务 {} 不存在", input.task_id)))?;

    // 更新 stage_id 和 path
    let new_path = match &input.stage_id {
        Some(stage_id) => format!("/{}/{}/{}", task.goal_id, stage_id, input.task_id),
        None => format!("/{}/{}", task.goal_id, input.task_id),
    };

    sqlx::query("UPDATE tasks SET stage_id = ?, path = ? WHERE id = ?")
        .bind(&input.stage_id)
        .bind(&new_path)
        .bind(&input.task_id)
        .execute(&state.0)
        .await?;

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
        // 允许空字符串表示清除日期
        updates.push("plan_date = ?".to_string());
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
#[tauri::command]
pub async fn delete_task(task_id: String, state: State<'_, DbPool>) -> AppResult<()> {
    sqlx::query("DELETE FROM tasks WHERE id = ?")
        .bind(&task_id)
        .execute(&state.0)
        .await?;
    Ok(())
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
        "SELECT t.id, t.goal_id, g.name as goal_name, t.stage_id, t.name,
                t.plan_date, t.plan_qty, t.actual_qty, t.unit, t.status, t.source,
                0 as is_overdue
         FROM tasks t
         JOIN goals g ON t.goal_id = g.id
         WHERE t.plan_date IS NOT NULL
           AND t.plan_date >= ? AND t.plan_date <= ?
         ORDER BY t.plan_date, t.sort_order",
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
