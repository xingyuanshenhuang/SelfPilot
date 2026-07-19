use tauri::State;
use uuid::Uuid;

use crate::db::models::{AddEncouragementInput, Encouragement, StreakInfo, UpdateEncouragementInput};
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use sqlx::QueryBuilder;

/// 鼓励语展示历史去重窗口：最近 N 条不重复
const DEDUP_WINDOW: i64 = 5;

/// 校验等级合法性，返回归一化后的等级字符串
fn validate_level(level: &Option<String>) -> AppResult<String> {
    match level.as_deref().unwrap_or("normal") {
        "normal" | "advanced" | "highlight" | "celebration" => {
            Ok(level.as_deref().unwrap_or("normal").to_string())
        }
        _ => Err(AppError::Param(
            "等级无效，应为 normal/advanced/highlight/celebration".into(),
        )),
    }
}

// ============================================================
// P0-3 辅助函数：统一降级链与去重抽取
// ============================================================

/// 从指定等级桶中随机抽取一条，排除指定 ids
async fn pick_by_level(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    level: &str,
    exclude_ids: &[String],
) -> AppResult<Option<Encouragement>> {
    let mut builder =
        QueryBuilder::<sqlx::Sqlite>::new("SELECT * FROM encouragements WHERE level = ");
    builder.push_bind(level);
    if !exclude_ids.is_empty() {
        builder.push(" AND id NOT IN (");
        let mut sep = builder.separated(", ");
        for id in exclude_ids {
            sep.push_bind(id);
        }
        builder.push(")");
    }
    builder.push(" ORDER BY RANDOM() LIMIT 1");
    let item: Option<Encouragement> = builder
        .build_query_as::<Encouragement>()
        .fetch_optional(&mut **tx)
        .await?;
    Ok(item)
}

/// 从全库随机抽取一条，排除指定 ids（无等级过滤）
async fn pick_any(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    exclude_ids: &[String],
) -> AppResult<Option<Encouragement>> {
    let mut builder = QueryBuilder::<sqlx::Sqlite>::new("SELECT * FROM encouragements");
    if !exclude_ids.is_empty() {
        builder.push(" WHERE id NOT IN (");
        let mut sep = builder.separated(", ");
        for id in exclude_ids {
            sep.push_bind(id);
        }
        builder.push(")");
    }
    builder.push(" ORDER BY RANDOM() LIMIT 1");
    let item: Option<Encouragement> = builder
        .build_query_as::<Encouragement>()
        .fetch_optional(&mut **tx)
        .await?;
    Ok(item)
}

/// 按 levels 顺序依次尝试抽取，首个非空即返回；全部为空返回 None
async fn random_with_fallback(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    levels: &[&str],
    exclude_ids: &[String],
) -> AppResult<Option<Encouragement>> {
    for level in levels {
        if let Some(item) = pick_by_level(tx, level, exclude_ids).await? {
            return Ok(Some(item));
        }
    }
    Ok(None)
}

/// 查询最近 DEDUP_WINDOW 条展示记录的 encouragement_id（去重排除列表）
async fn recent_shown_ids(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
) -> AppResult<Vec<String>> {
    let ids: Vec<String> =
        sqlx::query_scalar("SELECT encouragement_id FROM encouragement_show_log ORDER BY shown_at DESC LIMIT ?")
            .bind(DEDUP_WINDOW)
            .fetch_all(&mut **tx)
            .await?;
    Ok(ids)
}

/// 抽取成功后写入展示日志
async fn log_show(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    encouragement_id: &str,
    trigger_source: &str,
) -> AppResult<()> {
    let log_id = Uuid::new_v4().to_string();
    let now = chrono::Local::now()
        .format("%Y-%m-%dT%H:%M:%S%.3f")
        .to_string();
    sqlx::query(
        "INSERT INTO encouragement_show_log (id, encouragement_id, shown_at, trigger_source) VALUES (?, ?, ?, ?)",
    )
    .bind(&log_id)
    .bind(encouragement_id)
    .bind(&now)
    .bind(trigger_source)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

// ============================================================
// 命令实现
// ============================================================

/// 列出所有鼓励语
#[tauri::command]
pub async fn list_encouragements(state: State<'_, DbPool>) -> AppResult<Vec<Encouragement>> {
    let list: Vec<Encouragement> =
        sqlx::query_as("SELECT * FROM encouragements ORDER BY created_at")
            .fetch_all(&state.0)
            .await?;
    Ok(list)
}

/// 添加自定义鼓励语
#[tauri::command]
pub async fn add_encouragement(
    input: AddEncouragementInput,
    state: State<'_, DbPool>,
) -> AppResult<Encouragement> {
    if input.text.trim().is_empty() {
        return Err(AppError::Param("鼓励语内容不能为空".into()));
    }

    let level = validate_level(&input.level)?;

    let id = Uuid::new_v4().to_string();
    let now = chrono::Local::now()
        .format("%Y-%m-%dT%H:%M:%S")
        .to_string();

    sqlx::query(
        "INSERT INTO encouragements (id, text, category, level, created_at) VALUES (?, ?, 'custom', ?, ?)",
    )
    .bind(&id)
    .bind(&input.text)
    .bind(&level)
    .bind(&now)
    .execute(&state.0)
    .await?;

    let item: Encouragement = sqlx::query_as("SELECT * FROM encouragements WHERE id = ?")
        .bind(&id)
        .fetch_one(&state.0)
        .await?;

    Ok(item)
}

/// 更新自定义鼓励语（P0-5：补齐编辑功能）
///
/// - 仅自定义文案可修改文本与等级，预设文案拒绝修改
/// - text 非空且字符数 2~100（用 chars().count() 计算中文长度）
/// - level 合法性校验
/// - 使用事务保证读取-校验-更新原子性
#[tauri::command]
pub async fn update_encouragement(
    input: UpdateEncouragementInput,
    state: State<'_, DbPool>,
) -> AppResult<Encouragement> {
    let mut tx = state.0.begin().await?;

    // 1. 查记录
    let item: Encouragement =
        sqlx::query_as("SELECT * FROM encouragements WHERE id = ?")
            .bind(&input.id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("鼓励语 {} 不存在", input.id)))?;

    // 2. 校验非预设
    if item.category == "preset" {
        return Err(AppError::Business("预设鼓励语不允许修改".into()));
    }

    // 3. 校验 text（用 chars().count() 计算字符数，中文友好）
    let new_text = match &input.text {
        Some(t) => {
            let trimmed = t.trim();
            let char_count = trimmed.chars().count();
            if char_count < 2 {
                return Err(AppError::Param("鼓励语至少 2 个字".into()));
            }
            if char_count > 100 {
                return Err(AppError::Param("鼓励语不超过 100 字".into()));
            }
            if trimmed.is_empty() {
                return Err(AppError::Param("鼓励语内容不能为空".into()));
            }
            t.clone()
        }
        None => item.text.clone(),
    };

    // 4. 校验 level
    let new_level = match &input.level {
        Some(l) => validate_level(&Some(l.clone()))?,
        None => item.level.clone(),
    };

    // 5. 更新
    sqlx::query("UPDATE encouragements SET text = ?, level = ? WHERE id = ?")
        .bind(&new_text)
        .bind(&new_level)
        .bind(&input.id)
        .execute(&mut *tx)
        .await?;

    // 6. 读取最新记录
    let updated: Encouragement = sqlx::query_as("SELECT * FROM encouragements WHERE id = ?")
        .bind(&input.id)
        .fetch_one(&mut *tx)
        .await?;

    tx.commit().await?;

    Ok(updated)
}

/// 删除鼓励语（预设鼓励语不允许删除）
///
/// P0-4：因 foreign_keys pragma 未启用，需显式删除 encouragement_show_log 关联记录
#[tauri::command]
pub async fn delete_encouragement(id: String, state: State<'_, DbPool>) -> AppResult<()> {
    let item: Encouragement = sqlx::query_as("SELECT * FROM encouragements WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.0)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("鼓励语 {} 不存在", id)))?;

    if item.category == "preset" {
        return Err(AppError::Business("预设鼓励语不允许删除".into()));
    }

    // P0-4：显式删除展示历史（FK 级联未启用）
    sqlx::query("DELETE FROM encouragement_show_log WHERE encouragement_id = ?")
        .bind(&id)
        .execute(&state.0)
        .await?;

    sqlx::query("DELETE FROM encouragements WHERE id = ?")
        .bind(&id)
        .execute(&state.0)
        .await?;

    Ok(())
}

/// 随机抽取一句鼓励语（全等级，含展示去重）
///
/// P0-3 + P0-4：
/// - 事务保证 SELECT + INSERT 原子性，防止并发竞态
/// - 排除最近 DEDUP_WINDOW 条展示记录
/// - 抽取成功后写入展示日志
#[tauri::command]
pub async fn random_encouragement(
    trigger_source: String,
    state: State<'_, DbPool>,
) -> AppResult<Option<Encouragement>> {
    let mut tx = state.0.begin().await?;

    let exclude_ids = recent_shown_ids(&mut tx).await?;

    // 全库随机（无等级过滤），优先排除最近展示过的
    let item = pick_any(&mut tx, &exclude_ids).await?;

    match item {
        Some(item) => {
            log_show(&mut tx, &item.id, &trigger_source).await?;
            tx.commit().await?;
            Ok(Some(item))
        }
        None => {
            tx.rollback().await?;
            Ok(None)
        }
    }
}

/// 根据当前连续天数智能选择鼓励语等级
///
/// PRD §4.2 模块七 & 分阶段计划 Sprint 5：鼓励语个性化规则
/// - 连续 1 天 → normal 普通
/// - 连续 3 天 → advanced 进阶
/// - 连续 7 天 → highlight 高亮
///
/// P0-3：统一降级链为 `目标等级 → normal → 全库`
/// P0-4：排除最近 DEDUP_WINDOW 条展示记录，事务保证原子性
#[tauri::command]
pub async fn random_encouragement_by_streak(
    streak: i64,
    trigger_source: String,
    state: State<'_, DbPool>,
) -> AppResult<Option<Encouragement>> {
    let mut tx = state.0.begin().await?;

    let exclude_ids = recent_shown_ids(&mut tx).await?;

    // 确定目标等级
    let target_level = if streak >= 7 {
        "highlight"
    } else if streak >= 3 {
        "advanced"
    } else {
        "normal"
    };

    // 降级链：目标等级 → normal → （全库兜底）
    let levels: Vec<&str> = if target_level == "normal" {
        vec!["normal"]
    } else {
        vec![target_level, "normal"]
    };

    let item = random_with_fallback(&mut tx, &levels, &exclude_ids).await?;

    // 全库兜底
    let item = match item {
        Some(_) => item,
        None => pick_any(&mut tx, &exclude_ids).await?,
    };

    match item {
        Some(item) => {
            log_show(&mut tx, &item.id, &trigger_source).await?;
            tx.commit().await?;
            Ok(Some(item))
        }
        None => {
            tx.rollback().await?;
            Ok(None)
        }
    }
}

/// 抽取庆祝鼓励语（全部目标完成时使用）
///
/// P0-3：统一降级链为 `celebration → highlight → advanced → normal → 全库`
/// P0-4：排除最近 DEDUP_WINDOW 条展示记录，事务保证原子性
#[tauri::command]
pub async fn random_celebration_encouragement(
    trigger_source: String,
    state: State<'_, DbPool>,
) -> AppResult<Option<Encouragement>> {
    let mut tx = state.0.begin().await?;

    let exclude_ids = recent_shown_ids(&mut tx).await?;

    // 逐级降级：celebration → highlight → advanced → normal
    let levels = vec!["celebration", "highlight", "advanced", "normal"];
    let item = random_with_fallback(&mut tx, &levels, &exclude_ids).await?;

    // 全库兜底
    let item = match item {
        Some(_) => item,
        None => pick_any(&mut tx, &exclude_ids).await?,
    };

    match item {
        Some(item) => {
            log_show(&mut tx, &item.id, &trigger_source).await?;
            tx.commit().await?;
            Ok(Some(item))
        }
        None => {
            tx.rollback().await?;
            Ok(None)
        }
    }
}

/// 获取连续完成天数统计
///
/// PRD §4.2 模块七 & 分阶段计划开发注意事项：
/// - "每天至少完成一个任务"才计入连续
/// - 当天无任务则"不中断也不计入"
/// - 当天有任务但未完成则中断
///
/// 实现逻辑（从今天往前推）：
/// 1. 若今日有任务但未完成任何 → current_streak = 0
/// 2. 若今日无任务 → 从昨日开始往前统计
/// 3. 若今日已完成 → 从今日开始往前统计
/// 4. 遇到"有任务但未完成"的日期 → 中断
/// 5. 遇到"无任务"的日期 → 跳过（不中断）
#[tauri::command]
pub async fn get_streak(state: State<'_, DbPool>) -> AppResult<StreakInfo> {
    let today = chrono::Local::now().date_naive();

    // 查询所有有任务的日期及其完成情况
    // day_has_task: 当天是否有任务
    // day_completed: 当天是否至少完成一个任务
    let rows: Vec<(String, i64, i64)> = sqlx::query_as(
        "SELECT plan_date,
                COUNT(*) as task_count,
                SUM(CASE WHEN status = 'done' THEN 1 ELSE 0 END) as done_count
         FROM tasks
         WHERE plan_date IS NOT NULL AND status != 'skipped'
         GROUP BY plan_date",
    )
    .fetch_all(&state.0)
    .await?;

    use std::collections::HashMap;
    let mut day_map: HashMap<chrono::NaiveDate, (bool, bool)> = HashMap::new();
    for (date_str, task_count, done_count) in rows {
        if let Ok(d) = chrono::NaiveDate::parse_from_str(&date_str, "%Y-%m-%d") {
            let has_task = task_count > 0;
            let completed = done_count > 0;
            day_map.insert(d, (has_task, completed));
        }
    }

    // 计算当前连续天数
    let mut current_streak: i64 = 0;
    // cursor 初始化为昨日（所有分支都会从昨日开始往前推）
    let mut cursor = today - chrono::Duration::days(1);

    // 今日特殊处理：若今日无任务，从昨日开始；若今日有任务但未完成，中断
    let today_entry = day_map.get(&today);
    let completed_today = today_entry.map(|(_, c)| *c).unwrap_or(false);

    match today_entry {
        None => {
            // 今日无任务，从昨日开始（cursor 已是昨日）
        }
        Some((true, false)) => {
            // 今日有任务但未完成 → 中断
            current_streak = 0;
            // cursor 已是昨日，但 today_unfinished 会跳过循环
        }
        Some((true, true)) => {
            // 今日已完成
            current_streak = 1;
        }
        _ => {}
    }

    // 如果今日有任务但未完成，current_streak 已为 0，跳过循环
    let today_unfinished = matches!(today_entry, Some((true, false)));
    if !today_unfinished {
        // 往前推算连续天数
        loop {
            let entry = day_map.get(&cursor);
            match entry {
                None => {
                    // 无任务日，跳过（不中断）
                    cursor = cursor - chrono::Duration::days(1);
                }
                Some((true, true)) => {
                    // 有任务且完成 → 连续+1
                    current_streak += 1;
                    cursor = cursor - chrono::Duration::days(1);
                }
                Some((true, false)) => {
                    // 有任务但未完成 → 中断
                    break;
                }
                _ => {
                    cursor = cursor - chrono::Duration::days(1);
                }
            }

            // 防止无限循环（最多回溯 10 年）
            if (today - cursor).num_days() > 3650 {
                break;
            }
        }
    }

    // 计算 longest_streak：遍历所有有任务的日期
    let mut longest_streak: i64 = 0;
    let mut temp_streak: i64 = 0;
    let mut last_date: Option<chrono::NaiveDate> = None;

    let mut sorted_dates: Vec<chrono::NaiveDate> = day_map.keys().copied().collect();
    sorted_dates.sort();

    for d in &sorted_dates {
        let (has_task, completed) = day_map[d];
        if !has_task {
            continue;
        }
        if completed {
            // 检查与上一个日期的连续性（允许中间有无任务日）
            let should_continue = match last_date {
                None => true,
                Some(last) => {
                    // 从 last 到 d 之间，所有有任务的日期都应已完成
                    // 简化处理：只要日期递增且中间没有"有任务但未完成"的日期
                    let mut check = last + chrono::Duration::days(1);
                    let mut ok = true;
                    while check < *d {
                        if let Some((ht, comp)) = day_map.get(&check) {
                            if *ht && !*comp {
                                ok = false;
                                break;
                            }
                        }
                        check = check + chrono::Duration::days(1);
                    }
                    ok
                }
            };
            if should_continue {
                temp_streak += 1;
            } else {
                temp_streak = 1;
            }
            last_date = Some(*d);
            if temp_streak > longest_streak {
                longest_streak = temp_streak;
            }
        } else {
            // 有任务但未完成 → 中断
            temp_streak = 0;
            last_date = Some(*d);
        }
    }

    // 确保 longest_streak 至少等于 current_streak
    if current_streak > longest_streak {
        longest_streak = current_streak;
    }

    Ok(StreakInfo {
        current_streak,
        longest_streak,
        completed_today,
    })
}
