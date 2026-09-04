use chrono::Timelike;
use serde::{Deserialize, Serialize};
use tauri::State;
use crate::util::{new_uuid, now_local_ts};

use crate::db::models::{
    AddEncouragementInput, Encouragement, EncouragementTriggerSource, LaggingGoal,
    SetbackSituation, StreakInfo, UpdateEncouragementInput,
};
use crate::db::DbPool;
use crate::db::helpers;
use crate::error::{AppError, AppResult};
use sqlx::QueryBuilder;
use validator::Validate;

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

/// 查询指定条件下的候选文案（可选 style / 时段过滤）
async fn fetch_candidates(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    level: &str,
    style: Option<&str>,
    period: Option<&str>,
    exclude_ids: &[String],
) -> AppResult<Vec<CandidateWithStats>> {
    // 注意：QueryBuilder 不会解析 SQL 字符串中的 `?` 占位符，
    // push_bind 会在 SQL 末尾追加占位符，因此初始 SQL 中不能写字面 `?`，
    // 否则会生成 `= ??`、`LIKE ??` 这类非法 SQL 导致 "near \"?\": syntax error"。
    let mut builder = QueryBuilder::<sqlx::Sqlite>::new(
        r#"
        SELECT
            e.*,
            COUNT(DISTINCT l.id) as show_count,
            CASE WHEN f.id IS NOT NULL THEN 1 ELSE 0 END as is_favorite
        FROM encouragements e
        LEFT JOIN encouragement_show_log l ON e.id = l.encouragement_id
        LEFT JOIN encouragement_favorites f ON e.id = f.encouragement_id
        WHERE e.hidden = 0
        "#,
    );
    builder.push(" AND e.level = ");
    builder.push_bind(level);
    if let Some(s) = style {
        builder.push(" AND e.style = ");
        builder.push_bind(s);
    }
    if let Some(p) = period {
        builder.push(" AND e.context_tags LIKE ");
        builder.push_bind(format!("%\"time\":\"{}\"", p));
    }
    if !exclude_ids.is_empty() {
        builder.push(" AND e.id NOT IN (");
        let mut sep = builder.separated(", ");
        for id in exclude_ids {
            sep.push_bind(id);
        }
        builder.push(")");
    }
    builder.push(" GROUP BY e.id");

    let candidates: Vec<CandidateWithStats> = builder
        .build_query_as::<CandidateWithStats>()
        .fetch_all(&mut **tx)
        .await?;
    Ok(candidates)
}

/// 判断候选是否需要按风格（style）过滤并抽取
///
/// 尝试顺序：① 风格+时段 → ② 风格 → ③ 无风格（回退 warm 等既有文案）
/// F4：支持"文案风格"设置，确保不同风格取到不同口吻的文案
async fn pick_by_level(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    level: &str,
    exclude_ids: &[String],
    style: Option<&str>,
) -> AppResult<Option<Encouragement>> {
    // P2-3：获取当前时段
    let time_period = get_current_time_period();

    // P3-1：查询最近5条展示记录（用于惩罚）
    let recent_shown: Vec<String> = sqlx::query_scalar(
        "SELECT encouragement_id FROM encouragement_show_log ORDER BY shown_at DESC LIMIT 5"
    )
    .fetch_all(&mut **tx)
    .await?;

    let combos: Vec<(Option<&str>, Option<&str>)> = match style {
        Some(s) => {
            vec![(Some(s), Some(time_period)), (Some(s), None), (None, None)]
        }
        None => vec![(None, Some(time_period)), (None, None)],
    };

    for (style_f, period_f) in combos {
        let candidates = fetch_candidates(tx, level, style_f, period_f, exclude_ids).await?;
        if !candidates.is_empty() {
            return Ok(Some(weighted_random_pick(candidates, &recent_shown)?));
        }
    }

    Ok(None)
}

/// 读取用户当前文案风格设置，默认 "warm"
async fn current_style(pool: &sqlx::SqlitePool) -> AppResult<String> {
    let style: Option<String> =
        sqlx::query_scalar("SELECT value FROM settings WHERE key = 'encouragement_style'")
            .fetch_optional(pool)
            .await?;
    Ok(style.unwrap_or_else(|| "warm".to_string()))
}

/// P3-1：候选文案（包含统计信息）
#[derive(sqlx::FromRow)]
struct CandidateWithStats {
    id: String,
    text: String,
    category: String,
    level: String,
    created_at: String,
    context_tags: Option<String>,
    hidden: Option<i32>,
    sort_order: Option<i64>,
    style: String,
    show_count: i64,
    is_favorite: i64,
}

/// P3-1：加权随机抽取（从候选列表中按权重随机选择）
///
/// S-06 (SEC-L-01)：空候选列表返回 `AppError::Business` 而非 panic，
/// 避免命令执行途中应用崩溃。
fn weighted_random_pick(
    candidates: Vec<CandidateWithStats>,
    recent_shown_ids: &[String],
) -> AppResult<Encouragement> {
    use rand::Rng;

    if candidates.is_empty() {
        return Err(AppError::Business("候选鼓励语列表为空".into()));
    }

    // 计算每条文案的权重
    let base_weight = 1.0;
    let mut weighted: Vec<(f64, &CandidateWithStats)> = Vec::new();

    for candidate in &candidates {
        let mut weight = base_weight;

        // 新文案加成：7天内权重 ×3
        let created_at = chrono::DateTime::parse_from_rfc3339(&candidate.created_at);
        if let Ok(created) = created_at {
            let now = chrono::Utc::now();
            let days = (now - created.with_timezone(&chrono::Utc)).num_days();
            if days <= 7 {
                weight *= 3.0;
            }
        }

        // 低频文案加成：展示次数 <3 权重 ×2
        if candidate.show_count < 3 {
            weight *= 2.0;
        }

        // 收藏文案加成：权重 ×2
        if candidate.is_favorite > 0 {
            weight *= 2.0;
        }

        // 最近展示惩罚：权重 ×0.1
        if recent_shown_ids.contains(&candidate.id) {
            weight *= 0.1;
        }

        weighted.push((weight, candidate));
    }

    // 加权随机抽取
    let total_weight: f64 = weighted.iter().map(|(w, _)| w).sum();
    let mut rng = rand::thread_rng();
    let mut random_val = rng.gen_range(0.0..total_weight);

    for (weight, candidate) in weighted {
        random_val -= weight;
        if random_val <= 0.0 {
            return Ok(Encouragement {
                id: candidate.id.clone(),
                text: candidate.text.clone(),
                category: candidate.category.clone(),
                level: candidate.level.clone(),
                created_at: candidate.created_at.clone(),
                context_tags: candidate.context_tags.clone(),
                hidden: candidate.hidden,
                sort_order: candidate.sort_order,
                style: candidate.style.clone(),
            });
        }
    }

    // 不应该到达这里（浮点累减误差兜底），返回最后一个
    // S-06：函数入口已排除空列表，此处仍用 ok_or 兜底，避免 unwrap 引发崩溃
    let last = candidates
        .last()
        .ok_or_else(|| AppError::Business("候选鼓励语列表为空".into()))?;
    Ok(Encouragement {
        id: last.id.clone(),
        text: last.text.clone(),
        category: last.category.clone(),
        level: last.level.clone(),
        created_at: last.created_at.clone(),
        context_tags: last.context_tags.clone(),
        hidden: last.hidden,
        sort_order: last.sort_order,
        style: last.style.clone(),
    })
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
    style: Option<&str>,
) -> AppResult<Option<Encouragement>> {
    for level in levels {
        if let Some(item) = pick_by_level(tx, level, exclude_ids, style).await? {
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
    let log_id = new_uuid();
    let now = now_local_ts();
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
        sqlx::query_as("SELECT * FROM encouragements ORDER BY sort_order, created_at")
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
    // S-05 (SEC-M-05)：入参校验（文本长度 ≤ 100、等级枚举）
    input.validate()?;

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

    let id = new_uuid();
    let now = now_local_ts();

    // P3-5：计算当前最大 sort_order + 1，确保新增项排到最后
    let max_sort_order: Option<i64> = sqlx::query_scalar(
        "SELECT MAX(sort_order) FROM encouragements",
    )
    .fetch_one(&state.0)
    .await?;
    let sort_order = max_sort_order.unwrap_or(0) + 1;

    sqlx::query(
        "INSERT INTO encouragements (id, text, category, level, created_at, sort_order) VALUES (?, ?, 'custom', ?, ?, ?)",
    )
    .bind(&id)
    .bind(&input.text)
    .bind(&level)
    .bind(&now)
    .bind(sort_order)
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
            .ok_or_else(|| helpers::not_found("鼓励语", &input.id))?;

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
        .ok_or_else(|| helpers::not_found("鼓励语", &id))?;

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

    // F4：读取当前文案风格，用于按风格过滤
    let style = current_style(&state.0).await?;

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

    let item = random_with_fallback(&mut tx, &levels, &exclude_ids, Some(&style)).await?;

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

    // F4：读取当前文案风格，用于按风格过滤
    let style = current_style(&state.0).await?;

    // 逐级降级：celebration → highlight → advanced → normal
    let levels = vec!["celebration", "highlight", "advanced", "normal"];
    let item = random_with_fallback(&mut tx, &levels, &exclude_ids, Some(&style)).await?;

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
    // R-02：统一调用 streak_service::calc_streak，消除重复实现
    crate::services::streak_service::calc_streak(&state.0).await
}

// ============================================================
// P1-2：挫折/安抚场景检测
// ============================================================

/// 检测挫折场景
///
/// 检测逻辑：
/// 1. streak_break：昨日连续天数 ≥3，今日有任务但未完成，导致连续归零
/// 2. progress_lag：目标截止日期已过（逾期）或预测完成日期晚于截止日期（已完成目标除外）
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

    // 查询所有根目标（进度滞后检测关注根目标；goals 表无 status 列，
    // 活跃即根目标 parent_id IS NULL，与 stats 口径保持一致）
    use std::collections::HashSet;
    let active_roots: HashSet<String> = sqlx::query_scalar(
        "SELECT id FROM goals WHERE parent_id IS NULL",
    )
    .fetch_all(&state.0)
    .await?
    .into_iter()
    .collect();

    // 已完成目标不再纳入滞后/逾期提示（无论其原定截止时间是否已过）。
    // 使用 progress_service 的递归完成判定，避免预测逻辑对含子目标的根目标漏判。
    let completed_ids: HashSet<String> =
        crate::services::progress_service::calc_all_goals_progress(&state.0)
            .await?
            .into_iter()
            .filter(|p| p.is_completed)
            .map(|p| p.id)
            .collect();

    // 复用运行时完成预测，替代原 JOIN 不存在的 goal_progress 表
    // （goal_progress 表从未被迁移创建，原实现每次调用报 "no such table: goal_progress"
    //   且进度滞后检测静默失效；现基于 stats 完成预测在运行时计算 predicted_date）
    let predictions = crate::commands::stats::calc_completion_predictions(&state.0).await?;

    let lagging_goals: Vec<LaggingGoal> = predictions
        .iter()
        .filter(|p| {
            active_roots.contains(&p.goal_id) && !completed_ids.contains(&p.goal_id)
        })
        .filter_map(|p| {
            // 必须有截止日期
            let deadline = p.deadline.as_ref()?;

            // 解析日期
            let dl = chrono::NaiveDate::parse_from_str(deadline, "%Y-%m-%d").ok()?;
            let predicted = p
                .predicted_date
                .as_ref()
                .and_then(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok());

            // 滞后 = 截止日期已过（逾期）或预测完成日期晚于截止日期。
            // 逾期判断直接基于截止日期，避免依赖预测日期（无近期进度数据时
            // predicted_date 为空，会导致逾期目标被漏报）。
            let is_overdue = dl < today;
            let is_late = predicted.map(|pred| pred > dl).unwrap_or(false);

            if is_overdue || is_late {
                Some(LaggingGoal {
                    id: p.goal_id.clone(),
                    name: p.goal_name.clone(),
                    deadline: deadline.clone(),
                    predicted_end_date: predicted
                        .map(|d| d.format("%Y-%m-%d").to_string())
                        .unwrap_or_else(|| deadline.clone()),
                    days_remaining: p.days_to_deadline.unwrap_or(0) as i32,
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

/// 内部函数：获取连续天数
///
/// R-02：原先这里是重复实现且 longest_streak 被错误简化为 current_streak，
/// 现统一委托给 streak_service::calc_streak，修复了 longest_streak 隐性 bug。
async fn get_streak_inner(pool: &sqlx::SqlitePool) -> AppResult<StreakInfo> {
    crate::services::streak_service::calc_streak(pool).await
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
    // S-05 (SEC-M-05)：批量操作数组长度上限
    if ids.len() > crate::db::models::MAX_BATCH_SIZE {
        return Err(AppError::Param(format!(
            "批量操作一次最多 {} 条",
            crate::db::models::MAX_BATCH_SIZE
        )));
    }

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
    // S-05 (SEC-M-05)：批量操作数组长度上限
    if ids.len() > crate::db::models::MAX_BATCH_SIZE {
        return Err(AppError::Param(format!(
            "批量操作一次最多 {} 条",
            crate::db::models::MAX_BATCH_SIZE
        )));
    }

    // 校验等级合法性
    // S-05：原先 `let _ = validate_level(...)` 丢弃了校验结果，非法等级可入库，改为错误传播
    validate_level(&Some(level.clone()))?;

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

/// 切换收藏状态（已收藏则取消，未收藏则添加）
#[tauri::command]
pub async fn toggle_favorite(id: String, state: State<'_, DbPool>) -> AppResult<bool> {
    // 检查是否已收藏
    let existing: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM encouragement_favorites WHERE encouragement_id = ?",
    )
    .bind(&id)
    .fetch_optional(&state.0)
    .await?;

    if let Some(_) = existing {
        // 已收藏，取消收藏
        sqlx::query("DELETE FROM encouragement_favorites WHERE encouragement_id = ?")
            .bind(&id)
            .execute(&state.0)
            .await?;
        Ok(false)
    } else {
        // 未收藏，添加收藏
        let favorite_id = new_uuid();
        let now = now_local_ts();
        sqlx::query(
            "INSERT INTO encouragement_favorites (id, encouragement_id, favorited_at) VALUES (?, ?, ?)",
        )
        .bind(&favorite_id)
        .bind(&id)
        .bind(&now)
        .execute(&state.0)
        .await?;
        Ok(true)
    }
}

/// 获取收藏的鼓励语列表
#[tauri::command]
pub async fn get_favorites(state: State<'_, DbPool>) -> AppResult<Vec<Encouragement>> {
    let favorites = sqlx::query_as::<_, Encouragement>(
        r#"
        SELECT e.* FROM encouragements e
        INNER JOIN encouragement_favorites f ON e.id = f.encouragement_id
        ORDER BY f.favorited_at DESC
        "#,
    )
    .fetch_all(&state.0)
    .await?;
    Ok(favorites)
}

/// 检查鼓励语是否已收藏
#[tauri::command]
pub async fn is_favorite(id: String, state: State<'_, DbPool>) -> AppResult<bool> {
    let existing: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM encouragement_favorites WHERE encouragement_id = ?",
    )
    .bind(&id)
    .fetch_optional(&state.0)
    .await?;
    Ok(existing.is_some())
}

// ============================================================
// P3-3：展示反馈学习
// ============================================================

/// 记录用户关闭鼓励语弹窗的行为
#[tauri::command]
pub async fn log_encouragement_close(
    id: String,
    view_duration: i64,
    state: State<'_, DbPool>,
) -> AppResult<()> {
    let closed_at = now_local_ts();

    // 更新最新的展示记录
    sqlx::query(
        r#"
        UPDATE encouragement_show_log
        SET closed_at = ?, view_duration = ?
        WHERE id = (
            SELECT id FROM encouragement_show_log
            WHERE encouragement_id = ?
            ORDER BY shown_at DESC
            LIMIT 1
        )
        "#,
    )
    .bind(&closed_at)
    .bind(view_duration)
    .bind(&id)
    .execute(&state.0)
    .await?;

    Ok(())
}

/// 获取鼓励语展示统计
#[tauri::command]
pub async fn get_encouragement_stats(
    id: String,
    state: State<'_, DbPool>,
) -> AppResult<EncouragementStats> {
    // 总展示次数
    let total_shows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM encouragement_show_log WHERE encouragement_id = ?",
    )
    .bind(&id)
    .fetch_one(&state.0)
    .await?;

    // 平均观看时长（秒）
    let avg_duration: Option<f64> = sqlx::query_scalar(
        "SELECT AVG(view_duration) FROM encouragement_show_log WHERE encouragement_id = ? AND view_duration IS NOT NULL",
    )
    .bind(&id)
    .fetch_optional(&state.0)
    .await?;

    // 最近一次展示时间
    let last_shown: Option<String> = sqlx::query_scalar(
        "SELECT shown_at FROM encouragement_show_log WHERE encouragement_id = ? ORDER BY shown_at DESC LIMIT 1",
    )
    .bind(&id)
    .fetch_optional(&state.0)
    .await?;

    Ok(EncouragementStats {
        total_shows,
        avg_duration: avg_duration.unwrap_or(0.0),
        last_shown,
    })
}

/// 鼓励语展示统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncouragementStats {
    /// 总展示次数
    pub total_shows: i64,
    /// 平均观看时长（秒）
    pub avg_duration: f64,
    /// 最近一次展示时间
    pub last_shown: Option<String>,
}

// ============================================================
// P3-4：longest_streak 信号利用
// ============================================================

/// 检测是否接近/超越历史最长连续天数
#[tauri::command]
pub async fn check_longest_streak_milestone(
    state: State<'_, DbPool>,
) -> AppResult<Option<StreakMilestone>> {
    let streak_info = get_streak(state).await?;

    // 接近历史记录：current_streak >= longest_streak - 2 且 < longest_streak
    // 超越历史记录：current_streak >= longest_streak
    let current = streak_info.current_streak;
    let longest = streak_info.longest_streak;

    if current >= longest && longest > 0 {
        // 超越或追平历史记录
        Ok(Some(StreakMilestone {
            milestone_type: if current > longest {
                "超越历史".to_string()
            } else {
                "追平历史".to_string()
            },
            current_streak: current,
            longest_streak: longest,
        }))
    } else if current >= longest - 2 && longest > 0 {
        // 接近历史记录（距离 2 天内）
        Ok(Some(StreakMilestone {
            milestone_type: "接近历史".to_string(),
            current_streak: current,
            longest_streak: longest,
        }))
    } else {
        Ok(None)
    }
}

/// 抽取 longest_streak 触发的鼓励语
#[tauri::command]
pub async fn random_longest_streak_encouragement(
    trigger_source: EncouragementTriggerSource,
    state: State<'_, DbPool>,
) -> AppResult<Option<Encouragement>> {
    let mut tx = state.0.begin().await?;

    // 查询最近5条展示记录（去重）
    let recent_shown: Vec<String> = sqlx::query_scalar(
        "SELECT encouragement_id FROM encouragement_show_log ORDER BY shown_at DESC LIMIT 5"
    )
    .fetch_all(&mut *tx)
    .await?;

    // F4：读取当前文案风格，用于按风格过滤
    let style = current_style(&state.0).await?;

    // 从 longest_streak 等级抽取（新增的标签）
    let enc = pick_by_level(&mut tx, "longest_streak", &recent_shown, Some(&style)).await?;

    // 记录展示日志
    if let Some(ref e) = enc {
        let log_id = new_uuid();
        let now = now_local_ts();
        sqlx::query(
            "INSERT INTO encouragement_show_log (id, encouragement_id, shown_at, trigger_source) VALUES (?, ?, ?, ?)",
        )
        .bind(&log_id)
        .bind(&e.id)
        .bind(&now)
        .bind(trigger_source.as_ref())
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(enc)
}

/// 连续天数里程碑信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreakMilestone {
    /// 里程碑类型：接近历史/追平历史/超越历史
    pub milestone_type: String,
    /// 当前连续天数
    pub current_streak: i64,
    /// 历史最长连续天数
    pub longest_streak: i64,
}

// ============================================================
// P3-5：拖拽排序与自定义顺序
// ============================================================

/// 更新鼓励语排序顺序（批量）
#[tauri::command]
pub async fn update_encouragement_order(
    items: Vec<EncouragementOrderItem>,
    state: State<'_, DbPool>,
) -> AppResult<()> {
    let mut tx = state.0.begin().await?;

    for item in &items {
        sqlx::query("UPDATE encouragements SET sort_order = ? WHERE id = ?")
            .bind(item.sort_order)
            .bind(&item.id)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;
    Ok(())
}

/// 排序项输入
#[derive(Debug, Clone, Deserialize)]
pub struct EncouragementOrderItem {
    pub id: String,
    pub sort_order: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::collections::HashSet;
    use std::path::PathBuf;
    use std::time::Duration;

    /// 回归测试：四个 mock 库上验证 fetch_candidates 生成的 SQL 有效且能取到文案。
    ///
    /// 背景：此前 QueryBuilder 初始 SQL 中混有字面 `?`，生成 `level = ??`、`LIKE ??`
    /// 导致 "near \"?\": syntax error"，首任务/里程碑/庆祝分支全部取不到文案。
    const MOCK_DBS: &[&str] = &[
        "mock_selfpilot.db",
        "mock_selfpilot_approaching.db",
        "mock_selfpilot_milestone.db",
        "mock_selfpilot_celebration.db",
    ];

    async fn open_mock(name: &str) -> sqlx::SqlitePool {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("testdata")
            .join(name);
        let opts = SqliteConnectOptions::new()
            .filename(path)
            .read_only(true)
            .busy_timeout(Duration::from_secs(5));
        SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(opts)
            .await
            .expect("mock 库打开失败")
    }

    /// 在独立事务中执行 pick_by_level，结束后回滚释放连接。
    async fn pick_in_tx(
        pool: &sqlx::SqlitePool,
        level: &str,
        exclude_ids: &[String],
        style: Option<&str>,
    ) -> Result<Option<Encouragement>, AppError> {
        let mut tx = pool.begin().await?;
        let r = pick_by_level(&mut tx, level, exclude_ids, style).await;
        tx.rollback().await.ok();
        r
    }

    #[tokio::test]
    async fn pick_by_level_retrieves_on_all_mock_dbs() {
        for name in MOCK_DBS {
            let pool = open_mock(name).await;

            // 里程碑分支：longest_streak 等级
            let enc = pick_in_tx(&pool, "longest_streak", &[], Some("warm"))
                .await
                .unwrap_or_else(|e| panic!("{} longest_streak 查询失败: {e}", name));
            assert!(enc.is_some(), "{} 未取到 longest_streak 文案", name);

            // 首任务分支：normal 等级
            let enc = pick_in_tx(&pool, "normal", &[], Some("warm"))
                .await
                .unwrap_or_else(|e| panic!("{} normal 查询失败: {e}", name));
            assert!(enc.is_some(), "{} 未取到 normal 文案", name);

            // 庆祝分支：celebration 等级（含降级链入口）
            let enc = pick_in_tx(&pool, "celebration", &[], Some("warm"))
                .await
                .unwrap_or_else(|e| panic!("{} celebration 查询失败: {e}", name));
            assert!(enc.is_some(), "{} 未取到 celebration 文案", name);

            // 模拟 exclude_ids 非空（走 NOT IN 分支）
            let ids = vec!["nonexistent-id".to_string()];
            let enc = pick_in_tx(&pool, "normal", &ids, Some("warm"))
                .await
                .unwrap_or_else(|e| panic!("{} normal(NOT IN) 查询失败: {e}", name));
            assert!(enc.is_some(), "{} 未取到 normal(NOT IN) 文案", name);
        }
    }

    // ============================================================
    // 已完成目标不再进入滞后/逾期提示（get_setback_situation 筛选逻辑回归）
    // ============================================================

    /// 构造临时场景库（应用全部迁移），插入四类场景目标后返回连接池
    async fn setup_scenario_db() -> sqlx::SqlitePool {
        let dir = std::env::temp_dir().join("selfpilot_test_setback");
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("scenario.db");
        let _ = std::fs::remove_file(&db_path);

        let opts = SqliteConnectOptions::new()
            .filename(&db_path)
            .create_if_missing(true)
            .busy_timeout(Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    async fn insert_goal(
        pool: &sqlx::SqlitePool,
        id: &str,
        name: &str,
        deadline: Option<&str>,
        total_qty: f64,
    ) {
        let path = format!("/{}", id);
        sqlx::query(
            "INSERT INTO goals (id, name, parent_id, path, deadline, total_qty, unit, sort_order, created_at)
             VALUES (?, ?, NULL, ?, ?, ?, '', 0, '2026-01-01T00:00:00')",
        )
        .bind(id)
        .bind(name)
        .bind(&path)
        .bind(deadline)
        .bind(total_qty)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn insert_task(
        pool: &sqlx::SqlitePool,
        id: &str,
        goal_id: &str,
        status: &str,
        plan_qty: f64,
        actual_qty: f64,
    ) {
        let path = format!("/{}/{}", goal_id, id);
        sqlx::query(
            "INSERT INTO tasks (id, goal_id, stage_id, parent_id, path, name, plan_date, plan_qty, actual_qty, unit, status, is_manual, source, sort_order, created_at)
             VALUES (?, ?, NULL, NULL, ?, ?, NULL, ?, ?, '', ?, 0, 'manual', 0, '2026-01-01T00:00:00')",
        )
        .bind(id)
        .bind(goal_id)
        .bind(&path)
        .bind(id)
        .bind(plan_qty)
        .bind(actual_qty)
        .bind(status)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn completed_goals_excluded_from_overdue_lagging() {
        let pool = setup_scenario_db().await;

        // A 按时完成：截止在未来，任务全部完成
        insert_goal(&pool, "g-a", "按时完成", Some("2999-01-01"), 10.0).await;
        insert_task(&pool, "t-a1", "g-a", "done", 5.0, 5.0).await;
        insert_task(&pool, "t-a2", "g-a", "done", 5.0, 5.0).await;

        // B 超期后完成：截止已过，任务全部完成（核心修复场景）
        insert_goal(&pool, "g-b", "超期后完成", Some("2020-01-01"), 10.0).await;
        insert_task(&pool, "t-b1", "g-b", "done", 10.0, 10.0).await;

        // C 逾期未完成：截止已过，任务未完成（回归：逾期检测不应失效）
        insert_goal(&pool, "g-c", "逾期未完成", Some("2020-01-01"), 10.0).await;
        insert_task(&pool, "t-c1", "g-c", "pending", 10.0, 0.0).await;

        // D 完成后再次编辑：原按时完成，之后把截止改到过去，仍应保持已完成且不提示
        insert_goal(&pool, "g-d", "完成后编辑", Some("2020-01-01"), 10.0).await;
        insert_task(&pool, "t-d1", "g-d", "done", 10.0, 10.0).await;

        // 已完成集合来源与 get_setback_situation 完全一致
        let completed_ids: HashSet<String> =
            crate::services::progress_service::calc_all_goals_progress(&pool)
                .await
                .unwrap()
                .into_iter()
                .filter(|p| p.is_completed)
                .map(|p| p.id)
                .collect();

        assert!(completed_ids.contains("g-a"), "A 按时完成应判为已完成");
        assert!(completed_ids.contains("g-b"), "B 超期后完成应判为已完成");
        assert!(!completed_ids.contains("g-c"), "C 逾期未完成不应判为已完成");
        assert!(completed_ids.contains("g-d"), "D 完成后再次编辑仍应为已完成");

        // 复现 get_setback_situation 的滞后筛选（与真实实现同一段逻辑）
        let today = chrono::Local::now().date_naive();
        let predictions =
            crate::commands::stats::calc_completion_predictions(&pool).await.unwrap();
        let lagging: Vec<String> = predictions
            .iter()
            .filter(|p| !completed_ids.contains(&p.goal_id))
            .filter_map(|p| {
                let dl = chrono::NaiveDate::parse_from_str(p.deadline.as_ref()?, "%Y-%m-%d").ok()?;
                let predicted = p
                    .predicted_date
                    .as_ref()
                    .and_then(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok());
                let is_overdue = dl < today;
                let is_late = predicted.map(|pred| pred > dl).unwrap_or(false);
                if is_overdue || is_late {
                    Some(p.goal_id.clone())
                } else {
                    None
                }
            })
            .collect();

        assert!(
            !lagging.contains(&"g-a".to_string()),
            "A 按时完成不应进入逾期/滞后提示"
        );
        assert!(
            !lagging.contains(&"g-b".to_string()),
            "B 超期后完成不应进入逾期/滞后提示（核心修复）"
        );
        assert!(
            lagging.contains(&"g-c".to_string()),
            "C 逾期未完成仍应进入逾期/滞后提示（回归）"
        );
        assert!(
            !lagging.contains(&"g-d".to_string()),
            "D 完成后再次编辑不应进入逾期/滞后提示"
        );
    }
}
