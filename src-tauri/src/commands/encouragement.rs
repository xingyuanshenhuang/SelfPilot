use chrono::Timelike;
use tauri::State;
use uuid::Uuid;

use crate::db::models::{
    AddEncouragementInput, Encouragement, EncouragementFeedback, LaggingGoal, SetbackSituation,
    StreakInfo, UpdateEncouragementInput, UserFavorite,
};
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use sqlx::QueryBuilder;

/// 鼓励语展示历史去重窗口：最近 N 条不重复
const DEDUP_WINDOW: i64 = 5;

/// 校验等级合法性，返回归一化后的等级字符串
fn validate_level(level: &Option<String>) -> AppResult<String> {
    match level.as_deref().unwrap_or("normal") {
        "normal" | "advanced" | "highlight" | "celebration" | "setback" => {
            Ok(level.as_deref().unwrap_or("normal").to_string())
        }
        _ => Err(AppError::Param(
            "等级无效，应为 normal/advanced/highlight/celebration/setback".into(),
        )),
    }
}

// ============================================================
// P0-3 辅助函数：统一降级链与去重抽取
// ============================================================

/// 获取当前时段（P2-3）
/// - morning: 6-12 点
/// - afternoon: 12-18 点
/// - evening: 18-24 点
/// - night: 0-6 点
fn get_current_time_period() -> &'static str {
    let hour = chrono::Local::now().hour();
    match hour {
        6..=11 => "morning",
        12..=17 => "afternoon",
        18..=23 => "evening",
        _ => "night",
    }
}

/// 从指定等级桶中随机抽取一条，排除指定 ids（P0-3 + P2-3：时段过滤）
async fn pick_by_level(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    level: &str,
    exclude_ids: &[String],
) -> AppResult<Option<Encouragement>> {
    // P2-3：获取当前时段
    let time_period = get_current_time_period();

    // 优先筛选包含当前时段的文案
    let mut builder =
        QueryBuilder::<sqlx::Sqlite>::new("SELECT * FROM encouragements WHERE level = ");
    builder.push_bind(level);
    builder.push(" AND hidden = 0"); // P2-5：排除隐藏的预设文案
    builder.push(" AND context_tags LIKE ");
    builder.push_bind(format!("%\"time\":\"{}\"", time_period));
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

    // 如果没找到时段匹配的，降级为无标签文案
    if item.is_some() {
        return Ok(item);
    }

    // 降级：筛选无标签或任意文案
    let mut builder =
        QueryBuilder::<sqlx::Sqlite>::new("SELECT * FROM encouragements WHERE level = ");
    builder.push_bind(level);
    builder.push(" AND hidden = 0");
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

/// 从全库随机抽取一条，排除指定 ids（无等级过滤，P2-5：排除隐藏）
async fn pick_any(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    exclude_ids: &[String],
) -> AppResult<Option<Encouragement>> {
    let mut builder = QueryBuilder::<sqlx::Sqlite>::new("SELECT * FROM encouragements WHERE hidden = 0");
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

/// 添加自定义鼓励语（P2-6：去重检测）
#[tauri::command]
pub async fn add_encouragement(
    input: AddEncouragementInput,
    state: State<'_, DbPool>,
) -> AppResult<Encouragement> {
    if input.text.trim().is_empty() {
        return Err(AppError::Param("鼓励语内容不能为空".into()));
    }

    let level = validate_level(&input.level)?;

    // P2-6：去重检测（忽略大小写）
    let text_lower = input.text.trim().to_lowercase();
    let exists: Option<String> = sqlx::query_scalar(
        "SELECT id FROM encouragements WHERE LOWER(text) = ? LIMIT 1",
    )
    .bind(&text_lower)
    .fetch_optional(&state.0)
    .await?;

    if exists.is_some() {
        return Err(AppError::Business("该鼓励语已存在".into()));
    }

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
/// P3-4：根据 longest_streak 调整等级优先级
///   - longest_streak >= 30：expert 级别，优先 celebration/highlight
///   - longest_streak >= 14：专业级，优先 highlight/advanced
///   - 其他：正常推荐
#[tauri::command]
pub async fn random_encouragement_by_streak(
    streak: i64,
    longest_streak: i64,
    trigger_source: String,
    state: State<'_, DbPool>,
) -> AppResult<Option<Encouragement>> {
    let mut tx = state.0.begin().await?;

    let exclude_ids = recent_shown_ids(&mut tx).await?;

    // P3-4：根据 longest_streak 调整等级优先级
    let levels: Vec<&str> = if longest_streak >= 30 {
        // Expert 级别：优先 celebration/highlight
        vec!["celebration", "highlight", "advanced", "normal"]
    } else if longest_streak >= 14 {
        // 专业级：优先 highlight/advanced
        vec!["highlight", "advanced", "normal"]
    } else {
        // 正常推荐：根据当前连续天数
        let target_level = if streak >= 7 {
            "highlight"
        } else if streak >= 3 {
            "advanced"
        } else {
            "normal"
        };

        if target_level == "normal" {
            vec!["normal"]
        } else {
            vec![target_level, "normal"]
        }
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

    // P2-4：计算里程碑成就
    let milestone = if longest_streak >= 30 {
        "master".to_string()
    } else if longest_streak >= 14 {
        "expert".to_string()
    } else {
        "none".to_string()
    };

    Ok(StreakInfo {
        current_streak,
        longest_streak,
        completed_today,
        milestone,
    })
}

// ============================================================
// P1-2：挫折/安抚场景检测
// ============================================================

/// 检测挫折场景
///
/// 检测逻辑：
/// 1. streak_break：昨日连续天数 ≥3，今日有任务但未完成，导致连续归零
/// 2. progress_lag：目标预测完成日期 > 截止日期
///
/// 使用 settings 表存储 last_streak_check_date 和 last_streak_value 实现跨日对比
#[tauri::command]
pub async fn get_setback_situation(
    state: State<'_, DbPool>,
) -> AppResult<SetbackSituation> {
    let today = chrono::Local::now().date_naive();
    let today_str = today.format("%Y-%m-%d").to_string();

    // ============================================================
    // 1. 连续中断检测
    // ============================================================

    // 读取上次检测记录
    let last_check_date: Option<String> =
        sqlx::query_scalar("SELECT value FROM settings WHERE key = 'last_streak_check_date'")
            .fetch_optional(&state.0)
            .await?;

    let last_streak_value: Option<i32> = sqlx::query_scalar(
        "SELECT value FROM settings WHERE key = 'last_streak_value'",
    )
    .fetch_optional(&state.0)
    .await?
    .and_then(|v: String| v.parse::<i32>().ok());

    // 判断是否需要检测（跨日或首次检测）
    let should_check = last_check_date.as_ref() != Some(&today_str);

    // 获取当前 streak
    let streak_info = get_streak_inner(&state.0).await?;

    let (has_streak_break, streak_break_prev) = if should_check {
        // 对比昨日记录与今日
        if let Some(prev_streak) = last_streak_value {
            // 触发条件：昨日连续 ≥3，今日归零，且今日有任务但未完成
            let has_tasks_today = has_tasks_today_inner(&state.0, &today_str).await?;
            let triggered = prev_streak >= 3
                && streak_info.current_streak == 0
                && has_tasks_today
                && !streak_info.completed_today;

            if triggered {
                (true, prev_streak)
            } else {
                (false, 0)
            }
        } else {
            (false, 0)
        }
    } else {
        // 同一天不重复检测
        (false, 0)
    };

    // 更新检测记录（跨日时）
    if should_check {
        sqlx::query(
            "INSERT INTO settings (key, value) VALUES ('last_streak_check_date', ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind(&today_str)
        .execute(&state.0)
        .await?;

        sqlx::query(
            "INSERT INTO settings (key, value) VALUES ('last_streak_value', ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind(streak_info.current_streak.to_string())
        .execute(&state.0)
        .await?;
    }

    // ============================================================
    // 2. 进度滞后检测
    // ============================================================

    // 查询所有活跃目标及其预测完成日期
    let predictions: Vec<(String, String, Option<String>, Option<String>, Option<i64>)> =
        sqlx::query_as(
            "SELECT g.id, g.name, g.deadline, gp.predicted_date, gp.days_to_deadline
             FROM goals g
             LEFT JOIN goal_progress gp ON g.id = gp.goal_id
             WHERE g.status = 'active' AND g.parent_id IS NULL",
        )
        .fetch_all(&state.0)
        .await?;

    let lagging_goals: Vec<LaggingGoal> = predictions
        .iter()
        .filter_map(|(id, name, deadline, predicted, days_remaining)| {
            // 必须有截止日期和预测日期
            let deadline = deadline.as_ref()?;
            let predicted = predicted.as_ref()?;

            // 解析日期
            let dl = chrono::NaiveDate::parse_from_str(deadline, "%Y-%m-%d").ok()?;
            let pred = chrono::NaiveDate::parse_from_str(predicted, "%Y-%m-%d").ok()?;

            // 预测日期 > 截止日期 = 滞后
            if pred > dl {
                Some(LaggingGoal {
                    id: id.clone(),
                    name: name.clone(),
                    deadline: deadline.clone(),
                    predicted_end_date: predicted.clone(),
                    days_remaining: days_remaining.unwrap_or(0) as i32,
                })
            } else {
                None
            }
        })
        .collect();

    Ok(SetbackSituation {
        has_streak_break,
        streak_break_prev,
        has_progress_lag: !lagging_goals.is_empty(),
        lagging_goals,
    })
}

/// 内部函数：获取连续天数（复用现有 get_streak 逻辑，但不依赖 State）
async fn get_streak_inner(pool: &sqlx::SqlitePool) -> AppResult<StreakInfo> {
    let today = chrono::Local::now().date_naive();

    // 查询所有有任务的日期及其完成情况
    let rows: Vec<(String, i64, i64)> = sqlx::query_as(
        "SELECT plan_date,
                COUNT(*) as task_count,
                SUM(CASE WHEN status = 'done' THEN 1 ELSE 0 END) as done_count
         FROM tasks
         WHERE plan_date IS NOT NULL AND status != 'skipped'
         GROUP BY plan_date",
    )
    .fetch_all(pool)
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

    // 计算当前连续天数（与 get_streak 相同的逻辑）
    let mut current_streak: i64 = 0;
    let mut cursor = today - chrono::Duration::days(1);

    let today_entry = day_map.get(&today);
    let completed_today = today_entry.map(|(_, c)| *c).unwrap_or(false);

    match today_entry {
        None => {}
        Some((true, false)) => {
            current_streak = 0;
        }
        Some((true, true)) => {
            current_streak = 1;
        }
        _ => {}
    }

    let today_unfinished = matches!(today_entry, Some((true, false)));
    if !today_unfinished {
        loop {
            let entry = day_map.get(&cursor);
            match entry {
                None => {
                    cursor = cursor - chrono::Duration::days(1);
                }
                Some((true, true)) => {
                    current_streak += 1;
                    cursor = cursor - chrono::Duration::days(1);
                }
                Some((true, false)) => {
                    break;
                }
                _ => {
                    cursor = cursor - chrono::Duration::days(1);
                }
            }

            if (today - cursor).num_days() > 3650 {
                break;
            }
        }
    }

    // 计算 longest_streak（简化：使用 current_streak）
    let longest_streak = current_streak;

    // P2-4：计算里程碑成就
    let milestone = if longest_streak >= 30 {
        "master".to_string()
    } else if longest_streak >= 14 {
        "expert".to_string()
    } else {
        "none".to_string()
    };

    Ok(StreakInfo {
        current_streak,
        longest_streak,
        completed_today,
        milestone,
    })
}

/// 内部函数：获取今日是否有任务
async fn has_tasks_today_inner(pool: &sqlx::SqlitePool, today: &str) -> AppResult<bool> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tasks WHERE plan_date = ? AND status != 'skipped'",
    )
    .bind(today)
    .fetch_one(pool)
    .await?;
    Ok(count > 0)
}

// ============================================================
// P1-5：批量操作
// ============================================================

/// 批量删除鼓励语（仅自定义文案可删除）
///
/// 使用事务保证原子性，显式删除关联的 show_log
#[tauri::command]
pub async fn batch_delete_encouragements(
    ids: Vec<String>,
    state: State<'_, DbPool>,
) -> AppResult<i64> {
    let mut tx = state.0.begin().await?;
    let mut deleted = 0i64;

    for id in &ids {
        // 检查是否为预设文案
        let category: Option<String> =
            sqlx::query_scalar("SELECT category FROM encouragements WHERE id = ?")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?;

        if let Some(cat) = category {
            if cat == "preset" {
                // 预设文案跳过，不删除
                continue;
            }

            // 删除关联的 show_log
            sqlx::query("DELETE FROM encouragement_show_log WHERE encouragement_id = ?")
                .bind(id)
                .execute(&mut *tx)
                .await?;

            // 删除文案
            sqlx::query("DELETE FROM encouragements WHERE id = ?")
                .bind(id)
                .execute(&mut *tx)
                .await?;

            deleted += 1;
        }
    }

    tx.commit().await?;
    Ok(deleted)
}

/// 批量修改鼓励语等级（仅自定义文案可修改）
///
/// 使用事务保证原子性
#[tauri::command]
pub async fn batch_update_encouragement_level(
    ids: Vec<String>,
    level: String,
    state: State<'_, DbPool>,
) -> AppResult<i64> {
    // 校验等级合法性
    let _ = validate_level(&Some(level.clone()));

    let mut tx = state.0.begin().await?;
    let mut updated = 0i64;

    for id in &ids {
        // 检查是否为预设文案
        let category: Option<String> =
            sqlx::query_scalar("SELECT category FROM encouragements WHERE id = ?")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?;

        if let Some(cat) = category {
            if cat == "preset" {
                // 预设文案跳过，不修改
                continue;
            }

            sqlx::query("UPDATE encouragements SET level = ? WHERE id = ?")
                .bind(&level)
                .bind(id)
                .execute(&mut *tx)
                .await?;

            updated += 1;
        }
    }

    tx.commit().await?;
    Ok(updated)
}

// ============================================================
// P2-5：预设文案管理
// ============================================================

/// 隐藏预设鼓励语（仅预设文案可隐藏）
#[tauri::command]
pub async fn hide_preset_encouragement(
    id: String,
    state: State<'_, DbPool>,
) -> AppResult<()> {
    // 检查是否为预设文案
    let category: Option<String> =
        sqlx::query_scalar("SELECT category FROM encouragements WHERE id = ?")
            .bind(&id)
            .fetch_optional(&state.0)
            .await?;

    match category {
        Some(cat) if cat == "preset" => {
            sqlx::query("UPDATE encouragements SET hidden = 1 WHERE id = ?")
                .bind(&id)
                .execute(&state.0)
                .await?;
            Ok(())
        }
        Some(_) => Err(AppError::Business("仅预设文案可隐藏".into())),
        None => Err(AppError::NotFound("鼓励语不存在".into())),
    }
}

/// 重置所有隐藏的预设文案
#[tauri::command]
pub async fn reset_hidden_presets(state: State<'_, DbPool>) -> AppResult<i64> {
    let result = sqlx::query("UPDATE encouragements SET hidden = 0 WHERE category = 'preset'")
        .execute(&state.0)
        .await?;
    Ok(result.rows_affected() as i64)
}

// ============================================================
// P3-2：用户收藏机制
// ============================================================

/// 添加收藏
#[tauri::command]
pub async fn add_favorite(
    encouragement_id: String,
    state: State<'_, DbPool>,
) -> AppResult<UserFavorite> {
    // 检查是否已收藏
    let exists: Option<String> = sqlx::query_scalar(
        "SELECT id FROM user_favorites WHERE encouragement_id = ?",
    )
    .bind(&encouragement_id)
    .fetch_optional(&state.0)
    .await?;

    if exists.is_some() {
        return Err(AppError::Business("已收藏".into()));
    }

    let id = Uuid::new_v4().to_string();
    let now = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();

    sqlx::query("INSERT INTO user_favorites (id, encouragement_id, created_at) VALUES (?, ?, ?)")
        .bind(&id)
        .bind(&encouragement_id)
        .bind(&now)
        .execute(&state.0)
        .await?;

    // 更新权重（收藏文案权重×2）
    sqlx::query("UPDATE encouragements SET weight = weight * 2 WHERE id = ?")
        .bind(&encouragement_id)
        .execute(&state.0)
        .await?;

    let favorite: UserFavorite = sqlx::query_as("SELECT * FROM user_favorites WHERE id = ?")
        .bind(&id)
        .fetch_one(&state.0)
        .await?;

    Ok(favorite)
}

/// 取消收藏
#[tauri::command]
pub async fn remove_favorite(
    encouragement_id: String,
    state: State<'_, DbPool>,
) -> AppResult<()> {
    // 恢复权重
    sqlx::query("UPDATE encouragements SET weight = weight / 2 WHERE id = ?")
        .bind(&encouragement_id)
        .execute(&state.0)
        .await?;

    sqlx::query("DELETE FROM user_favorites WHERE encouragement_id = ?")
        .bind(&encouragement_id)
        .execute(&state.0)
        .await?;

    Ok(())
}

/// 列出收藏
#[tauri::command]
pub async fn list_favorites(state: State<'_, DbPool>) -> AppResult<Vec<Encouragement>> {
    let list: Vec<Encouragement> = sqlx::query_as(
        "SELECT e.* FROM encouragements e
         INNER JOIN user_favorites f ON e.id = f.encouragement_id
         ORDER BY f.created_at DESC",
    )
    .fetch_all(&state.0)
    .await?;
    Ok(list)
}

// ============================================================
// P3-3：展示反馈学习
// ============================================================

/// 记录反馈
#[tauri::command]
pub async fn record_feedback(
    encouragement_id: String,
    feedback_type: String,
    state: State<'_, DbPool>,
) -> AppResult<EncouragementFeedback> {
    if feedback_type != "like" && feedback_type != "dislike" {
        return Err(AppError::Param("feedback_type 必须是 like 或 dislike".into()));
    }

    let id = Uuid::new_v4().to_string();
    let now = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();

    sqlx::query(
        "INSERT INTO encouragement_feedback (id, encouragement_id, feedback_type, created_at) VALUES (?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&encouragement_id)
    .bind(&feedback_type)
    .bind(&now)
    .execute(&state.0)
    .await?;

    // 根据反馈调整权重
    if feedback_type == "like" {
        sqlx::query("UPDATE encouragements SET weight = weight * 1.1 WHERE id = ?")
            .bind(&encouragement_id)
            .execute(&state.0)
            .await?;
    } else {
        sqlx::query("UPDATE encouragements SET weight = weight * 0.9 WHERE id = ?")
            .bind(&encouragement_id)
            .execute(&state.0)
            .await?;
    }

    let feedback: EncouragementFeedback =
        sqlx::query_as("SELECT * FROM encouragement_feedback WHERE id = ?")
            .bind(&id)
            .fetch_one(&state.0)
            .await?;

    Ok(feedback)
}

/// 获取反馈统计
#[tauri::command]
pub async fn get_feedback_stats(
    encouragement_id: String,
    state: State<'_, DbPool>,
) -> AppResult<(i64, i64)> {
    let likes: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM encouragement_feedback WHERE encouragement_id = ? AND feedback_type = 'like'",
    )
    .bind(&encouragement_id)
    .fetch_one(&state.0)
    .await?;

    let dislikes: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM encouragement_feedback WHERE encouragement_id = ? AND feedback_type = 'dislike'",
    )
    .bind(&encouragement_id)
    .fetch_one(&state.0)
    .await?;

    Ok((likes, dislikes))
}

// ============================================================
// P3-5：拖拽排序
// ============================================================

/// 更新鼓励语排序（仅自定义文案可排序）
#[tauri::command]
pub async fn update_encouragement_order(
    id: String,
    sort_order: i32,
    state: State<'_, DbPool>,
) -> AppResult<()> {
    let item: Encouragement = sqlx::query_as("SELECT * FROM encouragements WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.0)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("鼓励语 {} 不存在", id)))?;

    if item.category == "preset" {
        return Err(AppError::Business("预设鼓励语不支持排序".into()));
    }

    sqlx::query("UPDATE encouragements SET sort_order = ? WHERE id = ?")
        .bind(sort_order)
        .bind(&id)
        .execute(&state.0)
        .await?;

    Ok(())
}

/// 批量更新排序（用于拖拽后批量更新）
#[tauri::command]
pub async fn batch_update_encouragement_order(
    orders: Vec<(String, i32)>,
    state: State<'_, DbPool>,
) -> AppResult<()> {
    let mut tx = state.0.begin().await?;

    for (id, sort_order) in orders {
        sqlx::query("UPDATE encouragements SET sort_order = ? WHERE id = ?")
            .bind(sort_order)
            .bind(&id)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;
    Ok(())
}

// ============================================================
// P3-6：独立导入导出
// ============================================================

/// 导出鼓励语（JSON格式）
#[tauri::command]
pub async fn export_encouragements(state: State<'_, DbPool>) -> AppResult<String> {
    let list: Vec<Encouragement> =
        sqlx::query_as("SELECT * FROM encouragements ORDER BY created_at")
            .fetch_all(&state.0)
            .await?;

    let json = serde_json::to_string(&list)?;
    Ok(json)
}

/// 导入鼓励语（JSON格式，去重跳过）
#[tauri::command]
pub async fn import_encouragements(
    json: String,
    state: State<'_, DbPool>,
) -> AppResult<(i64, i64)> {
    let list: Vec<Encouragement> = serde_json::from_str(&json)?;

    let mut imported = 0i64;
    let mut skipped = 0i64;

    for item in list {
        // 去重检测（忽略大小写）
        let exists: Option<String> = sqlx::query_scalar(
            "SELECT id FROM encouragements WHERE LOWER(text) = ? LIMIT 1",
        )
        .bind(item.text.to_lowercase())
        .fetch_optional(&state.0)
        .await?;

        if exists.is_some() {
            skipped += 1;
            continue;
        }

        let id = Uuid::new_v4().to_string();
        let now = chrono::Local::now()
            .format("%Y-%m-%dT%H:%M:%S")
            .to_string();

        sqlx::query(
            "INSERT INTO encouragements (id, text, category, level, created_at, context_tags, hidden, weight, sort_order)
             VALUES (?, ?, 'imported', ?, ?, ?, 0, 1.0, 0)",
        )
        .bind(&id)
        .bind(&item.text)
        .bind(&item.level)
        .bind(&now)
        .bind(&item.context_tags)
        .execute(&state.0)
        .await?;

        imported += 1;
    }

    Ok((imported, skipped))
}
