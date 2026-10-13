//! 智能拆解（v2 引擎）命令门面
//!
//! 阶段 2 接线：在既有纯计算规划引擎（`split_planner::solve`，零 DB 写入）
//! 之上提供预览与非落库应用两个 Tauri 命令门面：
//! - `smart_split_preview`：只算不落库（强制 dry_run=true），返回方案；
//! - `smart_split_apply`：求解后在同一事务内写入 `split_plans` 历史行，
//!   并将 `items` 落为 `tasks` + `task_dependencies`。
//!
//! 落库决策与旧 `goal::smart_split` 保持一致：目标已存在 `source='auto'`
//! 任务时返回业务错误，要求先删除旧任务再重新拆解（见 split_service 现状）。

use std::collections::HashMap;

use tauri::State;
use validator::Validate;

use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::services::planner::types::{
    FeasibilityStatus, SmartSplitRequest, SplitPlan,
};
use crate::services::split_planner;
use crate::util::{new_uuid, now_local_ts};

/// 只计算、不落库的预览命令。
///
/// 将请求复制并强制 `dry_run=true` 后调用引擎（引擎本就不写库，此处显式
/// 声明语义），返回完整方案供前端展示，不产生任何持久化副作用。
#[tauri::command]
pub async fn smart_split_preview(
    req: SmartSplitRequest,
    state: State<'_, DbPool>,
) -> AppResult<SplitPlan> {
    // S-05：入参校验（字段级 + 跨字段规则；solve 内部还会防御性复验）
    req.validate()?;

    let mut preview_req = req;
    preview_req.dry_run = true;
    split_planner::solve(&state.0, &preview_req).await
}

/// 求解并落库命令。
///
/// 逻辑：
/// 1. 校验 + 求解（复用阶段 1 纯计算引擎）；
/// 2. 事务内：
///    - 目标已存在 auto 任务 → 返回业务错误（与旧 smart_split 决策一致）；
///    - 同一目标旧生效方案 `is_active` 置 0（版本化，FR-15）；
///    - 插入一条 `split_plans` 历史行（含 plan_hash / feasibility / 摘要）；
///    - 将 `items` 落为 `tasks`，`depends_on_seq` 映射为 `task_dependencies`；
/// 3. 提交后原样返回方案（不破坏序列化契约）。
///
/// 错误信息统一经 `AppError` 脱敏（内部错误序列化时由 error.rs 挡为笼统提示）。
#[tauri::command]
pub async fn smart_split_apply(
    req: SmartSplitRequest,
    state: State<'_, DbPool>,
) -> AppResult<SplitPlan> {
    // S-05：入参校验
    req.validate()?;

    // 求解（只读装配快照），得到完整可序列化方案
    let plan = split_planner::solve(&state.0, &req).await?;

    let mut tx = state.0.begin().await?;

    // 决策对齐 split_service 现状：目标已有自动拆解任务时拒绝重复拆解
    // （与 goal::auto_split / split_by_capacity / smart_split 行为一致）
    let existing: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM tasks WHERE goal_id = ? AND source = 'auto'")
            .bind(&req.goal_id)
            .fetch_one(&mut *tx)
            .await?;
    if existing > 0 {
        return Err(AppError::Business(
            "该目标已有自动拆解任务，请先删除旧任务再重新拆解".into(),
        ));
    }

    // 版本化（FR-15）：同一目标先占用的旧生效方案置为非 active
    sqlx::query("UPDATE split_plans SET is_active = 0 WHERE goal_id = ? AND is_active = 1")
        .bind(&req.goal_id)
        .execute(&mut *tx)
        .await?;

    // feasibility / pattern 以列的类型对齐写入（TEXT）
    let feasibility = match plan.feasibility.status {
        FeasibilityStatus::Feasible => "feasible",
        FeasibilityStatus::FeasibleWithWarnings => "feasible_with_warnings",
        FeasibilityStatus::Infeasible => "infeasible",
    };
    let pattern = serde_json::to_string(&plan.resolved.pattern)
        .map(|s| s.trim_matches('"').to_string())
        .unwrap_or_default();
    let request_json = serde_json::to_string(&req)
        .map_err(|_| AppError::Internal("请求序列化失败".into()))?;
    let summary_json = serde_json::to_string(&plan.summary)
        .map_err(|_| AppError::Internal("方案摘要序列化失败".into()))?;
    let now = now_local_ts();

    // 插入方案历史行（is_active = 1）
    sqlx::query(
        "INSERT INTO split_plans
         (id, goal_id, engine_version, pattern, plan_hash, request_json, summary_json,
          feasibility, slack_ratio, task_count, total_allocated_qty, is_active, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?)",
    )
    .bind(plan.plan_id.to_string())
    .bind(&req.goal_id)
    .bind(&plan.engine_version)
    .bind(pattern)
    .bind(plan.plan_hash.clone())
    .bind(request_json)
    .bind(summary_json)
    .bind(feasibility)
    .bind(plan.feasibility.slack_ratio)
    .bind(plan.items.len() as i64)
    .bind(plan.summary.total_allocated_qty)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    // 阶段（phase/stage_hint）落库：按阶段分组为一棵 stage 二级节点，
    // 同一阶段任务共享 stage_id（避免信息丢失，对齐 stages 表结构）。
    let mut seq_to_stage: HashMap<String, String> = HashMap::new();
    for item in &plan.items {
        let stage_key = item.phase.as_deref().or_else(|| item.stage_hint.as_deref());
        let stage_key = match stage_key {
            Some(k) if !k.trim().is_empty() => k.trim().to_string(),
            _ => continue,
        };
        if seq_to_stage.contains_key(&stage_key) {
            continue;
        }
        let stage_id = new_uuid();
        sqlx::query(
            "INSERT INTO stages
             (id, goal_id, name, parent_id, path, sort_order, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&stage_id)
        .bind(&req.goal_id)
        .bind(&stage_key)
        .bind(&req.goal_id)
        .bind(format!("/{}/{}", req.goal_id, stage_id))
        .bind(seq_to_stage.len() as i64)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
        seq_to_stage.insert(stage_key, stage_id);
    }

    // 落任务并建立 序号 -> 新 task id 映射（依赖回溯用）
    let mut seq_to_id: HashMap<u32, String> = HashMap::with_capacity(plan.items.len());
    for item in &plan.items {
        let task_id = new_uuid();
        let plan_date = item.plan_date.format("%Y-%m-%d").to_string();
        let path = format!("/{}/{}", req.goal_id, task_id);
        let stage_id: Option<String> = item
            .phase
            .as_deref()
            .or_else(|| item.stage_hint.as_deref())
            .filter(|s| !s.trim().is_empty())
            .and_then(|k| seq_to_stage.get(k.trim()).cloned());

        sqlx::query(
            "INSERT INTO tasks
             (id, goal_id, stage_id, parent_id, path, name, plan_date, plan_qty,
              actual_qty, unit, status, is_manual, source, sort_order, created_at,
              estimated_hours, description)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, 0, ?, 'pending', 0, 'auto', ?, ?, ?, ?)",
        )
        .bind(&task_id)
        .bind(&req.goal_id)
        .bind(&stage_id)
        .bind(&req.goal_id)
        .bind(&path)
        .bind(&item.name)
        .bind(&plan_date)
        .bind(item.plan_qty)
        .bind(&item.unit)
        .bind(item.seq as i64)
        .bind(&now)
        .bind(item.estimated_hours)
        .bind(&item.description)
        .execute(&mut *tx)
        .await?;

        seq_to_id.insert(item.seq, task_id);
    }

    // 依赖：seq 序号 → 新 task id，写入 task_dependencies（去重语义与 task.rs 一致）
    for item in &plan.items {
        let task_id = match seq_to_id.get(&item.seq) {
            Some(id) => id.clone(),
            None => continue,
        };
        for dep_seq in &item.depends_on_seq {
            let dep_id = match seq_to_id.get(dep_seq) {
                Some(id) => id,
                None => continue,
            };
            sqlx::query(
                "INSERT OR IGNORE INTO task_dependencies (id, task_id, depends_on_id, created_at)
                 VALUES (?, ?, ?, ?)",
            )
            .bind(new_uuid())
            .bind(&task_id)
            .bind(dep_id)
            .bind(&now)
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await?;

    Ok(plan)
}