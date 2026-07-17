use tauri::State;

use crate::db::models::{
    Encouragement, ExportData, Goal, ImportInput, ImportResult, Setting, Task,
};
use crate::db::DbPool;
use crate::error::{AppError, AppResult};

/// 导出全部数据为 JSON 字符串
#[tauri::command]
pub async fn export_data(state: State<'_, DbPool>) -> AppResult<String> {
    let goals: Vec<Goal> =
        sqlx::query_as("SELECT * FROM goals ORDER BY sort_order, created_at")
            .fetch_all(&state.0)
            .await?;
    let tasks: Vec<Task> =
        sqlx::query_as("SELECT * FROM tasks ORDER BY plan_date, sort_order")
            .fetch_all(&state.0)
            .await?;
    let encouragements: Vec<Encouragement> =
        sqlx::query_as("SELECT * FROM encouragements ORDER BY created_at")
            .fetch_all(&state.0)
            .await?;
    let settings: Vec<Setting> = sqlx::query_as("SELECT * FROM settings ORDER BY key")
        .fetch_all(&state.0)
        .await?;

    let task_dependencies: Vec<crate::db::models::TaskDependency> =
        sqlx::query_as("SELECT * FROM task_dependencies ORDER BY created_at")
            .fetch_all(&state.0)
            .await?;

    let data = ExportData {
        version: "2.1".to_string(),
        exported_at: chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string(),
        goals,
        stages: vec![], // 已废弃，保留字段兼容旧备份
        tasks,
        task_dependencies,
        encouragements,
        settings,
    };

    serde_json::to_string_pretty(&data)
        .map_err(|e| AppError::Internal(format!("序列化失败: {}", e)))
}

/// 导入数据
///
/// conflict_mode: "skip" | "overwrite" | "rename"
#[tauri::command]
pub async fn import_data(
    input: ImportInput,
    state: State<'_, DbPool>,
) -> AppResult<ImportResult> {
    let mut tx = state.0.begin().await?;

    let data: ExportData = serde_json::from_str(&input.data)
        .map_err(|e| AppError::Param(format!("JSON 解析失败: {}", e)))?;

    let mode = input.conflict_mode.as_str();
    if !["skip", "overwrite", "rename"].contains(&mode) {
        return Err(AppError::Param(format!(
            "未知冲突模式: {}，应为 skip/overwrite/rename",
            mode
        )));
    }

    let mut result = ImportResult {
        goals_imported: 0,
        goals_skipped: 0,
        stages_imported: 0,
        stages_skipped: 0,
        tasks_imported: 0,
        tasks_skipped: 0,
        dependencies_imported: 0,
        dependencies_skipped: 0,
        encouragements_imported: 0,
        settings_imported: 0,
    };

    use std::collections::HashMap;
    let mut goal_id_map: HashMap<String, String> = HashMap::new();
    let mut task_id_map: HashMap<String, String> = HashMap::new();

    // 导入 goals
    for g in data.goals {
        let exists: bool = sqlx::query_scalar::<_, i64>("SELECT 1 FROM goals WHERE id = ?")
            .bind(&g.id)
            .fetch_optional(&mut *tx)
            .await?
            .is_some();

        let (id, action) = match (exists, mode) {
            (false, _) => (g.id.clone(), "import"),
            (true, "skip") => (g.id.clone(), "skip"),
            (true, "overwrite") => (g.id.clone(), "overwrite"),
            (true, "rename") => {
                let new_id = uuid::Uuid::new_v4().to_string();
                goal_id_map.insert(g.id.clone(), new_id.clone());
                (new_id, "rename")
            }
            _ => (g.id.clone(), "skip"),
        };

        match action {
            "skip" => {
                result.goals_skipped += 1;
                if mode == "rename" {
                    goal_id_map.insert(g.id.clone(), g.id.clone());
                }
            }
            "overwrite" => {
                sqlx::query(
                    "INSERT INTO goals (id, name, parent_id, path, deadline, total_qty, unit, sort_order, created_at)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                     ON CONFLICT(id) DO UPDATE SET
                     name=excluded.name, parent_id=excluded.parent_id, deadline=excluded.deadline,
                     total_qty=excluded.total_qty, unit=excluded.unit, sort_order=excluded.sort_order",
                )
                .bind(&id)
                .bind(&g.name)
                .bind(&g.parent_id)
                .bind(&g.path)
                .bind(&g.deadline)
                .bind(g.total_qty)
                .bind(&g.unit)
                .bind(g.sort_order)
                .bind(&g.created_at)
            .execute(&mut *tx)
            .await?;
        result.goals_imported += 1;
        }
        _ => {
        sqlx::query(
            "INSERT INTO goals (id, name, parent_id, path, deadline, total_qty, unit, sort_order, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&g.name)
        .bind(&g.parent_id)
        .bind(&g.path)
        .bind(&g.deadline)
        .bind(g.total_qty)
        .bind(&g.unit)
        .bind(g.sort_order)
        .bind(&g.created_at)
        .execute(&mut *tx)
        .await?;
        result.goals_imported += 1;
        if mode == "rename" && id != g.id {
            goal_id_map.insert(g.id.clone(), id.clone());
        }
        }
    }
    }

    // 导入 tasks（stages 已废弃，跳过）
    for t in data.tasks {
        let mapped_goal_id = if mode == "rename" {
            goal_id_map.get(&t.goal_id).cloned().unwrap_or(t.goal_id.clone())
        } else {
            t.goal_id.clone()
        };

        let exists: bool = sqlx::query_scalar::<_, i64>("SELECT 1 FROM tasks WHERE id = ?")
            .bind(&t.id)
            .fetch_optional(&mut *tx)
            .await?
            .is_some();

        let (id, action) = match (exists, mode) {
            (false, _) => (t.id.clone(), "import"),
            (true, "skip") => (t.id.clone(), "skip"),
            (true, "overwrite") => (t.id.clone(), "overwrite"),
            (true, "rename") => (uuid::Uuid::new_v4().to_string(), "rename"),
            _ => (t.id.clone(), "skip"),
        };

        // 记录任务 ID 映射：依赖关系中的原始 task_id / depends_on_id 需要映射到导入后的 ID
        task_id_map.insert(t.id.clone(), id.clone());

        match action {
            "skip" => {
                result.tasks_skipped += 1;
            }
            "overwrite" => {
                sqlx::query(
                    "INSERT INTO tasks (id, goal_id, stage_id, parent_id, path, name, plan_date,
                     plan_qty, actual_qty, unit, status, is_manual, source, sort_order, created_at)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                     ON CONFLICT(id) DO UPDATE SET
                     name=excluded.name, plan_date=excluded.plan_date, plan_qty=excluded.plan_qty,
                     actual_qty=excluded.actual_qty, status=excluded.status",
                )
                .bind(&id)
                .bind(&mapped_goal_id)
                .bind(&t.stage_id)
                .bind(&t.parent_id)
                .bind(&t.path)
                .bind(&t.name)
                .bind(&t.plan_date)
                .bind(t.plan_qty)
                .bind(t.actual_qty)
                .bind(&t.unit)
                .bind(&t.status)
                .bind(t.is_manual)
                .bind(&t.source)
                .bind(t.sort_order)
                .bind(&t.created_at)
            .execute(&mut *tx)
            .await?;
        result.tasks_imported += 1;
        }
        _ => {
        sqlx::query(
            "INSERT INTO tasks (id, goal_id, stage_id, parent_id, path, name, plan_date,
             plan_qty, actual_qty, unit, status, is_manual, source, sort_order, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&mapped_goal_id)
        .bind(&t.stage_id)
        .bind(&t.parent_id)
        .bind(&t.path)
        .bind(&t.name)
        .bind(&t.plan_date)
        .bind(t.plan_qty)
        .bind(t.actual_qty)
        .bind(&t.unit)
        .bind(&t.status)
        .bind(t.is_manual)
        .bind(&t.source)
        .bind(t.sort_order)
        .bind(&t.created_at)
        .execute(&mut *tx)
        .await?;
        result.tasks_imported += 1;
        }
    }
    }

    // 导入 task_dependencies（P1-1）
    // 前置条件：tasks 已经导入完成，task_id_map 已建立
    for d in data.task_dependencies {
        let mapped_task_id = task_id_map.get(&d.task_id).cloned().unwrap_or(d.task_id.clone());
        let mapped_dep_id = task_id_map
            .get(&d.depends_on_id)
            .cloned()
            .unwrap_or(d.depends_on_id.clone());

        // 若任一端任务被跳过（不存在于当前库），则跳过该依赖，避免外键错误
        let task_exists: bool =
            sqlx::query_scalar::<_, i64>("SELECT 1 FROM tasks WHERE id = ?")
                .bind(&mapped_task_id)
                .fetch_optional(&mut *tx)
                .await?
                .is_some();
        let dep_exists: bool =
            sqlx::query_scalar::<_, i64>("SELECT 1 FROM tasks WHERE id = ?")
                .bind(&mapped_dep_id)
                .fetch_optional(&mut *tx)
                .await?
                .is_some();

        if !task_exists || !dep_exists {
            result.dependencies_skipped += 1;
            continue;
        }

        let exists: bool =
            sqlx::query_scalar::<_, i64>("SELECT 1 FROM task_dependencies WHERE task_id = ? AND depends_on_id = ?")
                .bind(&mapped_task_id)
                .bind(&mapped_dep_id)
                .fetch_optional(&mut *tx)
                .await?
                .is_some();

        if exists && mode == "skip" {
            result.dependencies_skipped += 1;
            continue;
        }

        let id = if exists && mode == "rename" {
            uuid::Uuid::new_v4().to_string()
        } else {
            d.id.clone()
        };

        sqlx::query(
            "INSERT INTO task_dependencies (id, task_id, depends_on_id, created_at)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(task_id, depends_on_id) DO UPDATE SET created_at=excluded.created_at",
        )
        .bind(&id)
        .bind(&mapped_task_id)
        .bind(&mapped_dep_id)
        .bind(&d.created_at)
        .execute(&mut *tx)
        .await?;
        result.dependencies_imported += 1;
    }

    // 导入 encouragements（自定义鼓励语，预设不导入）
    for e in data.encouragements {
        if e.category == "preset" {
            continue;
        }
        let exists: bool = sqlx::query_scalar::<_, i64>("SELECT 1 FROM encouragements WHERE id = ?")
            .bind(&e.id)
            .fetch_optional(&mut *tx)
            .await?
            .is_some();

        let id = if exists && mode == "rename" {
            uuid::Uuid::new_v4().to_string()
        } else if exists && mode == "skip" {
            continue;
        } else {
            e.id.clone()
        };

        sqlx::query(
            "INSERT INTO encouragements (id, text, category, level, created_at)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET text=excluded.text",
        )
        .bind(&id)
        .bind(&e.text)
        .bind(&e.category)
        .bind(&e.level)
        .bind(&e.created_at)
        .execute(&mut *tx)
        .await?;
        result.encouragements_imported += 1;
    }

    // 导入 settings（upsert）
    for s in data.settings {
        sqlx::query(
            "INSERT INTO settings (key, value) VALUES (?, ?)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        )
        .bind(&s.key)
        .bind(&s.value)
        .execute(&mut *tx)
        .await?;
        result.settings_imported += 1;
    }

    tx.commit().await?;

    Ok(result)
}
