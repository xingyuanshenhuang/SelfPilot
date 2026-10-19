//! 智能规划（阶段 4）· 用户习惯画像学习（FR-16/17，整合设计 §4.6）
//!
//! - [`PlanningProfile`]：A 组学习参数（speed_factor/weekday_capacity/preferred_chunk/
//!   date_bias/strategy_affinity/samples/version）+ B 组显式偏好（weekday_hours/
//!   weekend_factor/intensity/rest_days/prefer_contiguous/respect_locks）；
//! - [`update_profile`]：EWMA 更新（§4.6.3，H 默认 20，可由
//!   `settings.planning_ewma_halflife` 覆盖）+ 冷启动门控（§4.6.4，N_warmup=10）；
//! - [`apply_profile`]：容量修正 `base × corr × weekday_capacity[wd]`（§4.6.5）；
//! - [`record_feedback`]：写 `split_feedback` 行 + 读画像 → EWMA → 写回
//!   `settings.planning_profile`（+ 冗余 key planning_profile_version/samples）。
//!
//! 反馈信号 7 类（§4.6.1）：qty_change / date_change / delete / early_finish /
//! late_finish / skip / plan_adopt。

use std::collections::HashMap;

use chrono::{Datelike, NaiveDate};
use serde_json::json;
use sqlx::SqlitePool;

use crate::db::models::FeedbackInput;
use crate::error::{AppError, AppResult};
use crate::util::{new_uuid, now_local_ts};

/// 冷启动门控样本阈值（§4.6.4 N_warmup = 10）
pub const WARMUP_SAMPLES: f64 = 10.0;
/// EWMA 半衰期默认值（§4.6.3 H = 20）
pub const DEFAULT_HALFLIFE: f64 = 20.0;
/// 完成率 clamp 下限（§4.6.5，防异常值污染）
const X_SPEED_MIN: f64 = 0.1;
/// 完成率 clamp 上限
const X_SPEED_MAX: f64 = 3.0;

/// 完整用户规划画像（A 组学习参数 + B 组显式偏好）
#[derive(Debug, Clone)]
pub struct PlanningProfile {
    // ---- A 组 · 学习得到的统计参数（EWMA 自动回流） ----
    /// 实际完成量 / 计划量（σ，全局容量修正，冷启动 1.0）
    pub speed_factor: f64,
    /// 各星期几相对产能（w_d，0=周日 … 6=周六，冷启动全 1.0）
    pub weekday_capacity: [f64; 7],
    /// 用户偏好的单任务粒度（κ）
    pub preferred_chunk: f64,
    /// 平均挪动天数（δ，正=总是往后推）
    pub date_bias: f64,
    /// 各拆解模式的采纳计数（α_p）
    pub strategy_affinity: HashMap<String, i64>,
    /// 累计样本数（门控强度）
    pub samples: i64,
    /// 画像版本（供方案版本化关联）
    pub version: i64,
    // ---- B 组 · 用户显式设置的偏好（手动配置） ----
    /// 各星期可用小时数（缺省 1.0）
    pub weekday_hours: HashMap<u8, f64>,
    /// 周末容量系数（缺省 1.0）
    pub weekend_factor: f64,
    /// 强度偏好系数（缺省 1.0；重排时由目标级策略优先）
    pub intensity: f64,
    /// 休息日星期集合（0=周日）
    pub rest_days: Vec<u8>,
    /// 是否偏好连续排期（平滑偏好）
    pub prefer_contiguous: bool,
    /// 是否尊重锁定
    pub respect_locks: bool,
}

impl Default for PlanningProfile {
    fn default() -> Self {
        PlanningProfile {
            speed_factor: 1.0,
            weekday_capacity: [1.0; 7],
            preferred_chunk: 0.0,
            date_bias: 0.0,
            strategy_affinity: HashMap::new(),
            samples: 0,
            version: 0,
            weekday_hours: HashMap::new(),
            weekend_factor: 1.0,
            intensity: 1.0,
            rest_days: Vec::new(),
            prefer_contiguous: false,
            respect_locks: true,
        }
    }
}

// ============================================================
// EWMA 核心（§4.6.3 ~ §4.6.5，纯函数，单测入口）
// ============================================================

/// EWMA 平滑系数：α = 1 − 0.5^(1/H)
pub fn ewma_alpha(halflife: f64) -> f64 {
    let h = if halflife.is_finite() && halflife > 0.0 {
        halflife
    } else {
        DEFAULT_HALFLIFE
    };
    1.0 - 0.5_f64.powf(1.0 / h)
}

/// 按反馈信号 EWMA 更新画像（§4.6.5 update_profile，纯函数）
pub fn update_profile(profile: &mut PlanningProfile, feedback: &FeedbackInput, halflife: f64) {
    let a = ewma_alpha(halflife);

    // 1. 完成率信号：x_speed = actual / planned，clamp[0.1, 3.0]
    if let (Some(planned), Some(actual)) = (feedback.planned_qty, feedback.actual_qty) {
        if planned > 0.0 {
            let x_speed = (actual / planned).clamp(X_SPEED_MIN, X_SPEED_MAX);
            profile.speed_factor = a * x_speed + (1.0 - a) * profile.speed_factor;
            if let Some(pd) = &feedback.plan_date {
                if let Ok(d) = NaiveDate::parse_from_str(pd, "%Y-%m-%d") {
                    let wd = d.weekday().num_days_from_sunday() as usize;
                    profile.weekday_capacity[wd] =
                        a * x_speed + (1.0 - a) * profile.weekday_capacity[wd];
                }
            }
        }
    }

    // 2. 粒度信号：用户最终接受的单任务量
    if let Some(fq) = feedback.final_qty {
        if fq > 0.0 {
            profile.preferred_chunk = a * fq + (1.0 - a) * profile.preferred_chunk;
        }
    }

    // 3. 日期偏移信号
    if let Some(ds) = feedback.date_shift {
        profile.date_bias = a * ds as f64 + (1.0 - a) * profile.date_bias;
    }

    // 4. 模式偏好信号
    if let Some(pat) = &feedback.chosen_pattern {
        *profile.strategy_affinity.entry(pat.clone()).or_insert(0) += 1;
    }

    profile.samples += 1;
    profile.version += 1;
}

/// 容量修正：base × corr × weekday_capacity[wd]（§4.6.5 apply_profile）
///
/// 冷启动门控：samples < N_warmup 时 gate < 1，samples = 0 时直接返回 base。
pub fn apply_profile(
    base_capacity: f64,
    date: NaiveDate,
    profile: &PlanningProfile,
    adapt_strength: f64,
) -> f64 {
    let gate = (profile.samples as f64 / WARMUP_SAMPLES).min(1.0);
    if gate <= 0.0 {
        return base_capacity;
    }
    let adapt = if adapt_strength.is_finite() { adapt_strength } else { 1.0 };
    let corr = 1.0 + gate * adapt * (profile.speed_factor - 1.0);
    let wd = date.weekday().num_days_from_sunday() as usize;
    base_capacity * corr * profile.weekday_capacity[wd]
}

// ============================================================
// 持久化层（读 settings / 写 settings + split_feedback）
// ============================================================

/// 读取 EWMA 半衰期（settings.planning_ewma_halflife，缺省 20）
pub async fn load_halflife(pool: &SqlitePool) -> AppResult<f64> {
    let v: Option<String> =
        sqlx::query_scalar("SELECT value FROM settings WHERE key = 'planning_ewma_halflife'")
            .fetch_optional(pool)
            .await?;
    Ok(v.and_then(|s| s.parse::<f64>().ok())
        .filter(|f| f.is_finite() && *f > 0.0)
        .unwrap_or(DEFAULT_HALFLIFE))
}

/// 读取完整画像（settings.planning_profile JSON，缺省时返回冷启动默认值）
pub async fn load_planning_profile(pool: &SqlitePool) -> AppResult<PlanningProfile> {
    let row: Option<String> =
        sqlx::query_scalar("SELECT value FROM settings WHERE key = 'planning_profile'")
            .fetch_optional(pool)
            .await?;
    let mut profile = PlanningProfile::default();
    let Some(raw) = row else {
        return Ok(profile);
    };
    let parsed: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|_| AppError::Internal("规划画像 JSON 解析失败".into()))?;

    // A 组
    if let Some(v) = parsed.get("speed_factor").and_then(|v| v.as_f64()) {
        if v.is_finite() && v > 0.0 {
            profile.speed_factor = v;
        }
    }
    if let Some(arr) = parsed.get("weekday_capacity").and_then(|v| v.as_array()) {
        for (i, v) in arr.iter().enumerate() {
            if i < 7 {
                if let Some(f) = v.as_f64() {
                    if f.is_finite() && f >= 0.0 {
                        profile.weekday_capacity[i] = f;
                    }
                }
            }
        }
    }
    if let Some(v) = parsed.get("preferred_chunk").and_then(|v| v.as_f64()) {
        if v.is_finite() && v >= 0.0 {
            profile.preferred_chunk = v;
        }
    }
    if let Some(v) = parsed.get("date_bias").and_then(|v| v.as_f64()) {
        if v.is_finite() {
            profile.date_bias = v;
        }
    }
    if let Some(obj) = parsed.get("strategy_affinity").and_then(|v| v.as_object()) {
        for (k, v) in obj.iter() {
            if let Some(n) = v.as_i64() {
                profile.strategy_affinity.insert(k.clone(), n);
            }
        }
    }
    if let Some(v) = parsed.get("samples").and_then(|v| v.as_i64()) {
        profile.samples = v;
    }
    if let Some(v) = parsed.get("version").and_then(|v| v.as_i64()) {
        profile.version = v;
    }

    // B 组
    if let Some(weekday_hours) = parsed.get("weekday_hours").and_then(|v| v.as_object()) {
        for (k, v) in weekday_hours.iter() {
            if let (Ok(w), Some(f)) = (k.parse::<u8>(), v.as_f64()) {
                if f.is_finite() && f >= 0.0 {
                    profile.weekday_hours.insert(w, f);
                }
            }
        }
    }
    if let Some(f) = parsed.get("weekend_factor").and_then(|v| v.as_f64()) {
        if f.is_finite() && f >= 0.0 {
            profile.weekend_factor = f;
        }
    }
    if let Some(f) = parsed.get("intensity").and_then(|v| v.as_f64()) {
        if f.is_finite() && f >= 0.0 {
            profile.intensity = f;
        }
    }
    if let Some(days) = parsed.get("rest_days").and_then(|v| v.as_array()) {
        profile.rest_days = days
            .iter()
            .filter_map(|v| v.as_u64())
            .filter(|&w| w <= 6)
            .map(|w| w as u8)
            .collect();
    }
    if let Some(b) = parsed.get("prefer_contiguous").and_then(|v| v.as_bool()) {
        profile.prefer_contiguous = b;
    }
    if let Some(b) = parsed.get("respect_locks").and_then(|v| v.as_bool()) {
        profile.respect_locks = b;
    }
    Ok(profile)
}

/// 画像序列化（settings.planning_profile 存储格式）
fn profile_to_json(profile: &PlanningProfile) -> String {
    let wd: Vec<f64> = profile.weekday_capacity.to_vec();
    let wh: serde_json::Map<String, serde_json::Value> = profile
        .weekday_hours
        .iter()
        .map(|(k, v)| (k.to_string(), json!(v)))
        .collect();
    json!({
        "speed_factor": profile.speed_factor,
        "weekday_capacity": wd,
        "preferred_chunk": profile.preferred_chunk,
        "date_bias": profile.date_bias,
        "strategy_affinity": profile.strategy_affinity,
        "samples": profile.samples,
        "version": profile.version,
        "weekday_hours": wh,
        "weekend_factor": profile.weekend_factor,
        "intensity": profile.intensity,
        "rest_days": profile.rest_days,
        "prefer_contiguous": profile.prefer_contiguous,
        "respect_locks": profile.respect_locks,
    })
    .to_string()
}

/// 记录一条反馈：写 split_feedback 行 + 读画像 → EWMA → 写回 settings。
pub async fn record_feedback(pool: &SqlitePool, input: &FeedbackInput) -> AppResult<()> {
    let mut tx = pool.begin().await?;

    // 1. 写 split_feedback（画像学习数据源）
    sqlx::query(
        "INSERT INTO split_feedback
         (id, goal_id, task_id, signal_type, plan_date, planned_qty, actual_qty,
          final_qty, date_shift, chosen_pattern, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(new_uuid())
    .bind(&input.goal_id)
    .bind(&input.task_id)
    .bind(&input.signal_type)
    .bind(&input.plan_date)
    .bind(input.planned_qty)
    .bind(input.actual_qty)
    .bind(input.final_qty)
    .bind(input.date_shift)
    .bind(&input.chosen_pattern)
    .bind(now_local_ts())
    .execute(&mut *tx)
    .await?;

    // 2. 读画像 → EWMA → 写回（含冗余 key）
    let halflife = load_halflife(pool).await?;
    let mut profile = load_planning_profile(pool).await?;
    update_profile(&mut profile, input, halflife);
    let json = profile_to_json(&profile);
    for (key, value) in [
        ("planning_profile".to_string(), json),
        (
            "planning_profile_version".to_string(),
            profile.version.to_string(),
        ),
        ("planning_profile_samples".to_string(), profile.samples.to_string()),
    ] {
        sqlx::query(
            "INSERT INTO settings (key, value) VALUES (?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind(key)
        .bind(&value)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
}

// ============================================================
// 单元测试（纯函数层：EWMA 数值 / 冷启动门控）
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn feedback(planned: f64, actual: f64, plan_date: &str) -> FeedbackInput {
        FeedbackInput {
            goal_id: "g1".into(),
            task_id: "t1".into(),
            signal_type: "early_finish".into(),
            plan_date: Some(plan_date.into()),
            planned_qty: Some(planned),
            actual_qty: Some(actual),
            final_qty: None,
            date_shift: None,
            chosen_pattern: None,
        }
    }

    #[test]
    fn test_ewma_alpha_default() {
        // H=20 → α = 1 − 0.5^(1/20) ≈ 0.03406
        let a = ewma_alpha(DEFAULT_HALFLIFE);
        let expected = 1.0 - 0.5_f64.powf(1.0 / 20.0);
        assert!((a - expected).abs() < 1e-12);
        assert!((a - 0.03406).abs() < 1e-4);
    }

    #[test]
    fn test_ewma_alpha_fast_adapt() {
        // H=2（快适应档）→ α ≈ 0.2929
        let a = ewma_alpha(2.0);
        assert!((a - 0.2929).abs() < 1e-3);
    }

    #[test]
    fn test_update_profile_speed_factor() {
        // 单样本：actual/planned = 2.0 → speed_factor 从 1.0 向 2.0 靠近一步
        let mut p = PlanningProfile::default();
        let f = feedback(10.0, 20.0, "2026-10-19");
        update_profile(&mut p, &f, DEFAULT_HALFLIFE);
        let a = ewma_alpha(DEFAULT_HALFLIFE);
        let expected = a * 2.0 + (1.0 - a) * 1.0;
        assert!((p.speed_factor - expected).abs() < 1e-12);
        assert_eq!(p.samples, 1);
        assert_eq!(p.version, 1);
        // 2026-10-19 为周一 → 下标 1（num_days_from_sunday: 0=周日…6=周六）
        assert!((p.weekday_capacity[1] - expected).abs() < 1e-12);
    }

    #[test]
    fn test_update_profile_clamp_speed() {
        // 完成率 5.0 → x_speed clamp 3.0；EWMA 从默认 1.0 向 3.0 靠近一步（α≈0.034）
        let mut p = PlanningProfile::default();
        let f = feedback(1.0, 5.0, "2026-10-19");
        update_profile(&mut p, &f, DEFAULT_HALFLIFE);
        let a = ewma_alpha(DEFAULT_HALFLIFE);
        let expected = a * 3.0 + (1.0 - a) * 1.0;
        assert!((p.speed_factor - expected).abs() < 1e-12);
        assert!(p.speed_factor <= 3.0 + 1e-12);
        assert!(p.speed_factor > 1.0, "单步应向 3.0 靠近");
        // 完成率 0.01 → x_speed clamp 0.1；向 0.1 靠近一步
        let mut p2 = PlanningProfile::default();
        let f2 = feedback(100.0, 1.0, "2026-10-19");
        update_profile(&mut p2, &f2, DEFAULT_HALFLIFE);
        let expected2 = a * 0.1 + (1.0 - a) * 1.0;
        assert!((p2.speed_factor - expected2).abs() < 1e-12);
        assert!(p2.speed_factor >= 0.1 - 1e-12);
        assert!(p2.speed_factor < 1.0, "单步应向 0.1 靠近");
    }

    #[test]
    fn test_apply_profile_cold_start() {
        // samples=0 → 直接返回 base（画像静默）
        let p = PlanningProfile::default();
        let d = NaiveDate::from_ymd_opt(2026, 10, 19).unwrap();
        assert_eq!(apply_profile(2.0, d, &p, 1.0), 2.0);
    }

    #[test]
    fn test_apply_profile_gate_and_correction() {
        // samples=5 → gate=0.5；speed_factor=1.5、weekday_capacity[1]=2.0
        let mut p = PlanningProfile::default();
        p.samples = 5;
        p.speed_factor = 1.5;
        p.weekday_capacity[1] = 2.0;
        let d = NaiveDate::from_ymd_opt(2026, 10, 19).unwrap(); // 周一 → 下标 1
        let base = 2.0;
        // corr = 1 + 0.5*1*(1.5-1) = 1.25；结果 = 2.0 * 1.25 * 2.0 = 5.0
        let out = apply_profile(base, d, &p, 1.0);
        assert!((out - 5.0).abs() < 1e-9);
    }
}
