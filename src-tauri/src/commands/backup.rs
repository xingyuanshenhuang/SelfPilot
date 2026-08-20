use std::io::Read;
use tauri::{AppHandle, Manager, State};
use validator::Validate;

use crate::db::models::{
    Encouragement, ExportData, Goal, ImportInput, ImportResult, Setting, Task,
};
use crate::db::DbPool;
use crate::error::{AppError, AppResult};

/// S-05 (SEC-M-06)：导入数据允许的 settings key 白名单
///
/// 仅放行应用自身使用的设置项，防止导入的备份写入任意 settings 行
/// （settings 会被 streak 检测等后端逻辑读取）。
const IMPORT_ALLOWED_SETTING_KEYS: &[&str] = &[
    // 主题（settingStore）
    "theme",
    // 鼓励语偏好（P1-4）
    "encouragement_enabled",
    "encouragement_frequency",
    "encouragement_style",
    "encouragement_celebration_animation",
    "encouragement_emoji_enabled",
    // streak 检测游标（P1-2）
    "last_streak_check_date",
    "last_streak_value",
];

/// S-05 (SEC-M-06)：校验导入数据的枚举字段合法性
///
/// 枚举值与数据库 CHECK 约束对齐，拒绝被篡改/损坏的备份，
/// 防止非法 status/source/category/level 绕过约束写入。
fn validate_import_payload(data: &ExportData) -> AppResult<()> {
    for t in &data.tasks {
        if !["pending", "partial", "done", "skipped"].contains(&t.status.as_str()) {
            return Err(AppError::Param(format!(
                "任务 {} 的 status 非法: {}",
                t.id, t.status
            )));
        }
        if !["auto", "manual"].contains(&t.source.as_str()) {
            return Err(AppError::Param(format!(
                "任务 {} 的 source 非法: {}",
                t.id, t.source
            )));
        }
    }
    for e in &data.encouragements {
        if !["preset", "custom"].contains(&e.category.as_str()) {
            return Err(AppError::Param(format!(
                "鼓励语 {} 的 category 非法: {}",
                e.id, e.category
            )));
        }
        if ![
            "normal",
            "advanced",
            "highlight",
            "celebration",
            "setback",
            "longest_streak",
        ]
        .contains(&e.level.as_str())
        {
            return Err(AppError::Param(format!(
                "鼓励语 {} 的 level 非法: {}",
                e.id, e.level
            )));
        }
    }
    Ok(())
}

/// S-02 (SEC-H-01/SEC-M-02/SEC-M-03): 校验文件路径安全性
///
/// 防止路径遍历攻击：
/// - 拒绝空路径
/// - 拒绝包含空字节的路径（防止 null byte 注入）
/// - 拒绝包含 `..` 的路径组件（防止目录遍历）
/// - 当 `must_be_sqlite` 为 true 时，额外校验：
///   - 扩展名为 .db 或 .sqlite
///   - 文件存在且前 16 字节为 SQLite 魔术字 `SQLite format 3\0`
fn validate_path_scope(path: &str, must_be_sqlite: bool) -> AppResult<()> {
    // 拒绝空路径
    if path.trim().is_empty() {
        return Err(AppError::Param("路径不能为空".to_string()));
    }

    // 拒绝包含空字节的路径
    if path.contains('\0') {
        return Err(AppError::Param(
            "路径包含非法字符（空字节）".to_string(),
        ));
    }

    let p = std::path::Path::new(path);

    // 拒绝包含 .. 的路径组件
    for component in p.components() {
        if component == std::path::Component::ParentDir {
            return Err(AppError::Param(
                "路径不能包含父目录引用 (..)".to_string(),
            ));
        }
    }

    if must_be_sqlite {
        // 校验扩展名（使用 Path::extension 而非 Path::ends_with，
        // 因为 Path::ends_with 检查的是完整路径组件，不是字符串后缀）
        let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext != "db" && ext != "sqlite" {
            return Err(AppError::Param(
                "文件必须以 .db 或 .sqlite 结尾".to_string(),
            ));
        }

        // 校验文件存在
        if !p.exists() {
            return Err(AppError::Param(format!("文件不存在: {}", path)));
        }

        // 校验 SQLite 魔术字（前 16 字节为 "SQLite format 3\0"）
        let mut file = std::fs::File::open(p)
            .map_err(|e| AppError::Internal(format!("打开文件失败: {}", e)))?;
        let mut header = [0u8; 16];
        file.read_exact(&mut header)
            .map_err(|e| AppError::Internal(format!("读取文件头失败: {}", e)))?;

        const SQLITE_MAGIC: &[u8; 16] = b"SQLite format 3\0";
        if &header != SQLITE_MAGIC {
            return Err(AppError::Param(
                "文件不是有效的 SQLite 数据库".to_string(),
            ));
        }
    }

    Ok(())
}

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

/// 导出全部数据到指定路径的 JSON 文件
///
/// 与 export_data 相同，但直接写入用户选择的文件路径，
/// 避免大 JSON 字符串通过 IPC 传输到前端再下载。
#[tauri::command]
pub async fn export_data_to_file(
    target_path: String,
    state: State<'_, DbPool>,
) -> AppResult<()> {
    // S-02: 校验路径安全性（防止路径遍历）
    validate_path_scope(&target_path, false)?;

    // 验证目标路径以 .json 结尾
    if !target_path.ends_with(".json") {
        return Err(AppError::Param(
            "导出文件必须以 .json 结尾".to_string(),
        ));
    }

    // 复用 export_data 的逻辑生成 JSON
    let json_str = export_data(state).await?;

    // 写入文件
    std::fs::write(&target_path, json_str)
        .map_err(|e| AppError::Internal(format!("写入文件失败: {}", e)))?;

    Ok(())
}

/// 导入数据
///
/// conflict_mode: "skip" | "overwrite" | "rename"
#[tauri::command]
pub async fn import_data(
    input: ImportInput,
    state: State<'_, DbPool>,
) -> AppResult<ImportResult> {
    // S-05 (SEC-M-06)：入参校验（JSON ≤ 50MB、冲突模式枚举）
    input.validate()?;

    let mut tx = state.0.begin().await?;

    let data: ExportData = serde_json::from_str(&input.data)
        .map_err(|e| AppError::Param(format!("JSON 解析失败: {}", e)))?;

    // S-05 (SEC-M-06)：导入内容校验（status/source/category/level 枚举合法性）
    validate_import_payload(&data)?;

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
                     plan_qty, actual_qty, unit, status, is_manual, source, sort_order, created_at, estimated_hours)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                     ON CONFLICT(id) DO UPDATE SET
                     name=excluded.name, plan_date=excluded.plan_date, plan_qty=excluded.plan_qty,
                     actual_qty=excluded.actual_qty, status=excluded.status, estimated_hours=excluded.estimated_hours",
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
                .bind(t.estimated_hours)
            .execute(&mut *tx)
            .await?;
        result.tasks_imported += 1;
        }
        _ => {
        sqlx::query(
            "INSERT INTO tasks (id, goal_id, stage_id, parent_id, path, name, plan_date,
             plan_qty, actual_qty, unit, status, is_manual, source, sort_order, created_at, estimated_hours)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
        .bind(t.estimated_hours)
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
        // S-05 (SEC-M-06)：settings key 白名单过滤，白名单外的键直接跳过
        if !IMPORT_ALLOWED_SETTING_KEYS.contains(&s.key.as_str()) {
            continue;
        }
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

/// SQLite 原生备份：使用 VACUUM INTO 生成 .db 完整副本
///
/// 优点（相比 JSON 导出）：
/// - 速度快（10MB < 2s）
/// - 100% 保留类型信息（二进制格式）
/// - 不会漏表/字段（整库快照）
#[tauri::command]
pub async fn backup_database(
    target_path: String,
    state: State<'_, DbPool>,
) -> AppResult<()> {
    // S-02: 校验路径安全性（防止路径遍历）
    validate_path_scope(&target_path, false)?;

    // 验证目标路径以 .db 或 .sqlite 结尾
    if !target_path.ends_with(".db") && !target_path.ends_with(".sqlite") {
        return Err(AppError::Param(
            "备份文件必须以 .db 或 .sqlite 结尾".to_string(),
        ));
    }

    // S-02: 拒绝覆盖已存在的文件（VACUUM INTO 不应静默覆盖）
    if std::path::Path::new(&target_path).exists() {
        return Err(AppError::Param(format!(
            "目标文件已存在，请更换路径或删除后重试: {}",
            target_path
        )));
    }

    // VACUUM INTO 不支持参数绑定，需手动转义单引号
    let escaped = target_path.replace('\'', "''");
    sqlx::query(&format!("VACUUM INTO '{}'", escaped))
        .execute(&state.0)
        .await?;
    Ok(())
}

/// SQLite 原生恢复：关闭连接池 → 覆盖 db 文件 → 提示重启
///
/// 恢复前自动备份当前 db 到 selfpilot.db.before_restore 作为安全网。
/// 恢复后连接池已关闭，前端必须提示用户重启应用。
#[tauri::command]
pub async fn restore_database(
    source_path: String,
    app: AppHandle,
    state: State<'_, DbPool>,
) -> AppResult<()> {
    // S-02: 校验路径安全性 + SQLite 魔术字（防止恢复非数据库文件导致数据损坏）
    validate_path_scope(&source_path, true)?;

    // 获取当前 db 路径
    let app_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Internal(format!("获取应用数据目录失败: {}", e)))?;
    let db_path = app_dir.join("selfpilot.db");
    let backup_path = app_dir.join("selfpilot.db.before_restore");

    // 恢复前自动备份当前 db（安全网）
    if db_path.exists() {
        std::fs::copy(&db_path, &backup_path).map_err(|e| {
            AppError::Internal(format!("恢复前备份当前数据库失败: {}", e))
        })?;
    }

    // 关闭连接池（后续数据库操作将失败，因此这是最后一步）
    state.0.close().await;

    // 覆盖 db 文件
    std::fs::copy(&source_path, &db_path)
        .map_err(|e| AppError::Internal(format!("覆盖数据库文件失败: {}", e)))?;

    Ok(())
}
