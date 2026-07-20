use tauri::State;

use crate::db::models::{
    EncouragementSettings, SetSettingInput, Setting, UpdateEncouragementSettingsInput,
};
use crate::db::DbPool;
use crate::error::{AppError, AppResult};

/// 获取所有设置项
#[tauri::command]
pub async fn get_all_settings(state: State<'_, DbPool>) -> AppResult<Vec<Setting>> {
    let list: Vec<Setting> = sqlx::query_as("SELECT * FROM settings ORDER BY key")
        .fetch_all(&state.0)
        .await?;
    Ok(list)
}

/// 获取单个设置项
#[tauri::command]
pub async fn get_setting(
    key: String,
    state: State<'_, DbPool>,
) -> AppResult<Option<String>> {
    let value: Option<String> =
        sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
            .bind(&key)
            .fetch_optional(&state.0)
            .await?;
    Ok(value)
}

/// 设置某个值（upsert）
#[tauri::command]
pub async fn set_setting(input: SetSettingInput, state: State<'_, DbPool>) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO settings (key, value) VALUES (?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(&input.key)
    .bind(&input.value)
    .execute(&state.0)
    .await?;
    Ok(())
}

// ============================================================
// P1-4: 鼓励语偏好设置命令
// ============================================================

/// 获取鼓励语偏好设置
#[tauri::command]
pub async fn get_encouragement_settings(
    state: State<'_, DbPool>,
) -> AppResult<EncouragementSettings> {
    // 批量读取 5 个设置项
    let keys = [
        "encouragement_enabled",
        "encouragement_frequency",
        "encouragement_style",
        "encouragement_celebration_animation",
        "encouragement_emoji_enabled",
    ];

    let mut values: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for key in keys {
        let value: Option<String> =
            sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
                .bind(key)
                .fetch_optional(&state.0)
                .await?;
        if let Some(v) = value {
            values.insert(key.to_string(), v);
        }
    }

    // 解析并返回默认值
    Ok(EncouragementSettings {
        enabled: values
            .get("encouragement_enabled")
            .and_then(|v| v.parse::<bool>().ok())
            .unwrap_or(true),
        frequency: values
            .get("encouragement_frequency")
            .cloned()
            .unwrap_or_else(|| "normal".to_string()),
        style: values
            .get("encouragement_style")
            .cloned()
            .unwrap_or_else(|| "warm".to_string()),
        celebration_animation: values
            .get("encouragement_celebration_animation")
            .and_then(|v| v.parse::<bool>().ok())
            .unwrap_or(true),
        emoji_enabled: values
            .get("encouragement_emoji_enabled")
            .and_then(|v| v.parse::<bool>().ok())
            .unwrap_or(true),
    })
}

/// 更新鼓励语偏好设置
#[tauri::command]
pub async fn update_encouragement_settings(
    input: UpdateEncouragementSettingsInput,
    state: State<'_, DbPool>,
) -> AppResult<()> {
    // 校验 frequency 合法性
    if let Some(ref freq) = input.frequency {
        if !["aggressive", "normal", "sparse"].contains(&freq.as_str()) {
            return Err(AppError::Param(
                "frequency 必须为 aggressive/normal/sparse".into(),
            ));
        }
    }

    // 校验 style 合法性
    if let Some(ref style) = input.style {
        if !["warm", "professional", "minimal"].contains(&style.as_str()) {
            return Err(AppError::Param(
                "style 必须为 warm/professional/minimal".into(),
            ));
        }
    }

    // 批量更新（upsert）
    let updates = [
        ("encouragement_enabled", input.enabled.map(|v| v.to_string())),
        ("encouragement_frequency", input.frequency),
        ("encouragement_style", input.style),
        (
            "encouragement_celebration_animation",
            input.celebration_animation.map(|v| v.to_string()),
        ),
        (
            "encouragement_emoji_enabled",
            input.emoji_enabled.map(|v| v.to_string()),
        ),
    ];

    for (key, value) in updates {
        if let Some(v) = value {
            sqlx::query(
                "INSERT INTO settings (key, value) VALUES (?, ?)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            )
            .bind(key)
            .bind(&v)
            .execute(&state.0)
            .await?;
        }
    }

    Ok(())
}
