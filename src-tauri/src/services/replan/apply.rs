//! 重新规划 · 提交应用与撤销（replan_apply / replan_revert）
//!
//! 依据《目标树重新规划功能重构设计文档》§3.3（A3 落库）：
//! - `apply`：预览与提交同源（apply 内部重新求解并校验 plan_hash，
//!   FR-11/AC-15 PlanStale 防 stale）；事务内逐任务现值复核 + 锁定保护后
//!   落库，并写 replan_events / replan_event_diffs 事件流水；
//! - `revert`：三条件校验（事件 applied / 该目标最新 applied / 目标仍存在）
//!   后反向应用 diff：仅当任务当前值 == diff 的 new 值才回写 old，
//!   不匹配跳过，防止覆盖后续手动改动。
//!
//! 变更判定（INV-6 幂等）：new_plan_qty ≠ old_plan_qty 或
//! new_plan_date ≠ old_plan_date 才 UPDATE；updated_at 使用本地时间
//! `yyyy-MM-dd HH:mm:ss`（撤销可行性判定依赖）。

use std::collections::BTreeSet;

use sqlx::{QueryBuilder, Sqlite, SqlitePool};

use crate::db::models::{
    ReplanApplyInput, ReplanApplyResult, ReplanEventDiff, ReplanOverrides, ReplanSolveInput,
    Task, TaskPlacement,
};
use crate::error::{AppError, AppResult};
use crate::services::replan::planner::solve;
use crate::util::{new_uuid, now_local_ts, now_updated_at_ts};

/// 提交应用：事务外重新求解校验 plan_hash，事务内现值复核后落库。
pub async fn apply(pool: &SqlitePool, req: &ReplanApplyInput) -> AppResult<ReplanApplyResult> {
    // ---------- 1. 预览与提交同源（FR-11/AC-15：PlanStale 防 stale） ----------
    // 复用预览时的 scope 与 overrides（含 excluded_task_ids），保证求解输入
    // 与预览一致，否则 plan_hash 必然不匹配（前端透传预览上下文）。
    let mut merged_overrides = req.overrides.clone();
    if let (Some(base), Some(extra)) = (&mut merged_overrides, &req.excluded_task_ids) {
        let mut ids = base.excluded_task_ids.clone().unwrap_or_default();
        for id in extra {
            if !ids.contains(id) {
                ids.push(id.clone());
            }
        }
        base.excluded_task_ids = Some(ids);
    } else if req.excluded_task_ids.is_some() {
        merged_overrides = Some(ReplanOverrides {
            deadline: None,
            daily_capacity: None,
            intensity: None,
            respect_locks: None,
            excluded_task_ids: req.excluded_task_ids.clone(),
        });
    }
    let solve_req = ReplanSolveInput {
        goal_id: req.goal_id.clone(),
        scope: req.scope.clone(),
        // coordinate 必须透传 goal_ids，否则重解目标集/plan_hash 必然不一致
        goal_ids: req.goal_ids.clone(),
        dry_run: true,
        overrides: merged_overrides,
        expected_plan_hash: None,
    };
    let plan = solve(pool, &solve_req).await?;
    if plan.plan_hash != req.plan_hash {
        return Err(AppError::Business("计划已过期，请重新预览".into()));
    }

    // 受影响目标（去重）：主目标 + 协同目标
    let mut affected: BTreeSet<String> = req
        .goal_ids
        .clone()
        .unwrap_or_default()
        .into_iter()
        .collect();
    affected.insert(req.goal_id.clone());

    let mut tx = pool.begin().await?;
    let now = now_local_ts();
    let updated_ts = now_updated_at_ts();

    // ---------- 2. 事务内逐任务现值复核 + 锁定保护 ----------
    // 需落库的变更行（仅当 qty/date 实际发生变化，INV-6 幂等）
    let mut changed: Vec<&TaskPlacement> = Vec::new();
    for p in &plan.placements {
        // 锁定保护：schedule_locked 非空 → 不改写日期/数量（跳过）
        let locked: Option<String> =
            sqlx::query_scalar("SELECT schedule_locked FROM tasks WHERE id = ?")
                .bind(&p.task_id)
                .fetch_optional(&mut *tx)
                .await?;
        if locked.is_some() {
            continue;
        }
        // 现值复核：当前 plan_date/plan_qty 必须等于预览时旧值，不一致跳过
        let cur: Option<(Option<String>, Option<f64>)> = sqlx::query_as(
            "SELECT plan_date, plan_qty FROM tasks WHERE id = ?",
        )
        .bind(&p.task_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some((cur_date, cur_qty)) = cur else {
            continue; // 任务已删除
        };
        if cur_date != p.old_plan_date
            || (cur_qty.unwrap_or(0.0) - p.old_plan_qty).abs() > 1e-9
        {
            continue; // 预览后被手动改动，跳过该任务
        }
        let date_changed = p.new_plan_date != p.old_plan_date;
        let qty_changed = (p.new_plan_qty - p.old_plan_qty).abs() > 1e-9;
        if date_changed || qty_changed {
            changed.push(p);
        }
    }

    // ---------- 3. 落库：tasks UPDATE + task_schedule_meta upsert ----------
    for p in &changed {
        let date_changed = p.new_plan_date != p.old_plan_date;
        sqlx::query("UPDATE tasks SET plan_qty = ?, plan_date = ?, updated_at = ? WHERE id = ?")
            .bind(p.new_plan_qty)
            .bind(&p.new_plan_date)
            .bind(&updated_ts)
            .bind(&p.task_id)
            .execute(&mut *tx)
            .await?;
        // 排期元数据：理由 + 最近规划时间 + 日期变更时 move_count +1
        let move_incr = if date_changed { 1 } else { 0 };
        sqlx::query(
            "INSERT INTO task_schedule_meta (task_id, alloc_reason, last_planned_at, move_count)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(task_id) DO UPDATE SET
                alloc_reason = excluded.alloc_reason,
                last_planned_at = excluded.last_planned_at,
                move_count = MAX(task_schedule_meta.move_count + excluded.move_count, 0)",
        )
        .bind(&p.task_id)
        .bind(&p.reason)
        .bind(&now)
        .bind(move_incr)
        .execute(&mut *tx)
        .await?;
    }

    // ---------- 4. 事件流水（replan_events + replan_event_diffs） ----------
    // 事件 id 一律新生成：solve 不写 previewed 事件，复用调用方传入的 event_id
    // 会与既有 replan_events 主键冲突。
    let event_id = new_uuid();
    let summary = serde_json::json!({
        "workload": plan.feasibility.workload,
        "capacity": plan.feasibility.capacity,
        "moved": plan.metrics.moved,
        "qty_changed": plan.metrics.qty_changed,
        "retained": plan.metrics.retained,
        "locked": plan.metrics.locked,
    });
    let metrics_json = serde_json::json!({
        "workload": plan.feasibility.workload,
        "capacity": plan.feasibility.capacity,
        "slack": plan.feasibility.slack,
        "jain_index": plan.metrics.jain_index,
        "moved": plan.metrics.moved,
        "updated": plan.metrics.qty_changed,
        "feasibility": plan.feasibility.feasible,
    })
    .to_string();

    sqlx::query(
        "INSERT INTO replan_events
         (id, goal_id, trigger_type, trigger_source, severity, status, plan_hash,
          snapshot_hash, before_summary, after_summary, metrics, error_message,
          created_at, applied_at)
         VALUES (?, ?, 'manual', 'user', 'info', 'applied', ?, NULL, ?, ?, ?, NULL, ?, ?)",
    )
    .bind(&event_id)
    .bind(&req.goal_id)
    .bind(&plan.plan_hash)
    .bind(summary.to_string())
    .bind(summary.to_string())
    .bind(&metrics_json)
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    for p in &changed {
        sqlx::query(
            "INSERT INTO replan_event_diffs
             (id, event_id, task_id, change_type, old_plan_date, new_plan_date,
              old_plan_qty, new_plan_qty, old_unit, new_unit, reason)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(new_uuid())
        .bind(&event_id)
        .bind(&p.task_id)
        .bind(&p.change_type)
        .bind(&p.old_plan_date)
        .bind(&p.new_plan_date)
        .bind(p.old_plan_qty)
        .bind(p.new_plan_qty)
        .bind(&p.unit)
        .bind(&p.unit)
        .bind(&p.reason)
        .execute(&mut *tx)
        .await?;
    }

    // ---------- 4.5 风险标记（SC-12 / AC-35） ----------
    // 接受风险 → 目标标记 at_risk；随后应用可行计划 → 自动清除过期风险标记
    for gid in &affected {
        if req.accept_risk {
            sqlx::query("UPDATE goals SET risk_flag = 'at_risk' WHERE id = ?")
                .bind(gid)
                .execute(&mut *tx)
                .await?;
        } else if plan.feasibility.feasible {
            sqlx::query("UPDATE goals SET risk_flag = NULL WHERE id = ? AND risk_flag = 'at_risk'")
                .bind(gid)
                .execute(&mut *tx)
                .await?;
        }
    }

    // ---------- 4.6 刷新触发冷却 ----------
    // 应用（手动或自动）即视为一次重排：写入 last_auto_replan_at，
    // 避免 check_triggers 在应用后立即又生成一轮建议（抖动/事件噪声）。
    for gid in &affected {
        sqlx::query(
            "INSERT INTO replan_trigger_state
             (goal_id, last_snapshot_hash, last_auto_replan_at, suppressed_count, updated_at)
             VALUES (?, NULL, ?, 0, ?)
             ON CONFLICT(goal_id) DO UPDATE SET
               last_auto_replan_at = excluded.last_auto_replan_at,
               updated_at          = excluded.updated_at",
        )
        .bind(gid)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
    }

    // ---------- 5. 回查受影响任务 ----------
    let tasks: Vec<Task> = if changed.is_empty() {
        Vec::new()
    } else {
        let ids: Vec<&str> = changed.iter().map(|p| p.task_id.as_str()).collect();
        let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new("SELECT * FROM tasks WHERE id IN (");
        let mut sep = qb.separated(", ");
        for id in ids {
            sep.push_bind(id);
        }
        sep.push_unseparated(")");
        qb.build_query_as().fetch_all(&mut *tx).await?
    };

    tx.commit().await?;

    // ---------- 6. 统计（按实际落库行） ----------
    let mut moved = 0usize;
    let mut qty_changed_total = 0usize;
    let mut retained = 0usize;
    for p in &changed {
        match p.change_type.as_str() {
            "date_moved" | "both" => moved += 1,
            "qty_changed" => qty_changed_total += 1,
            "retained" => retained += 1,
            _ => {}
        }
        if p.change_type == "both" {
            qty_changed_total += 1;
        }
    }

    Ok(ReplanApplyResult {
        event_id,
        goal_id: req.goal_id.clone(),
        moved,
        qty_changed: qty_changed_total,
        retained,
        affected_goal_ids: affected.into_iter().collect(),
        tasks,
    })
}

/// 撤销最近一次已应用的规划（三条件校验 + 反向应用 diff）。
pub async fn revert(pool: &SqlitePool, event_id: String) -> AppResult<ReplanApplyResult> {
    // ---------- 1. 三条件校验 ----------
    let event: Option<(String, Option<String>)> =
        sqlx::query_as("SELECT goal_id, status FROM replan_events WHERE id = ?")
            .bind(&event_id)
            .fetch_optional(pool)
            .await?;
    let Some((goal_id, status)) = event else {
        return Err(AppError::NotFound("重新规划事件不存在".into()));
    };
    // ① 事件处于 applied 状态
    if status.as_deref() != Some("applied") {
        return Err(AppError::Business("该规划事件不处于已应用状态，无法撤销".into()));
    }
    // ③ 目标仍存在
    let goal_exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM goals WHERE id = ?")
        .bind(&goal_id)
        .fetch_optional(pool)
        .await?;
    if goal_exists.is_none() {
        return Err(AppError::NotFound("目标不存在".into()));
    }
    // ② 是该目标最近一次 applied 事件
    let latest: Option<String> = sqlx::query_scalar(
        "SELECT id FROM replan_events WHERE goal_id = ? AND status = 'applied'
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(&goal_id)
    .fetch_optional(pool)
    .await?;
    if latest.as_deref() != Some(event_id.as_str()) {
        return Err(AppError::Business("只能撤销该目标最近一次已应用的规划".into()));
    }

    // ---------- 2. 事务内反向应用 diff ----------
    let mut tx = pool.begin().await?;
    let updated_ts = now_updated_at_ts();
    let diffs: Vec<ReplanEventDiff> = sqlx::query_as(
        "SELECT id, event_id, task_id, change_type, old_plan_date, new_plan_date,
                old_plan_qty, new_plan_qty, old_unit, new_unit, reason
         FROM replan_event_diffs WHERE event_id = ?",
    )
    .bind(&event_id)
    .fetch_all(&mut *tx)
    .await?;

    let mut reverted_count = 0usize;
    let mut skipped = 0usize;
    for d in &diffs {
        let Some(task_id) = &d.task_id else { continue };
        // retained/locked 类未改数据，无需回滚
        if matches!(d.change_type.as_deref(), Some("retained") | Some("locked")) {
            continue;
        }
        // 仅当当前值 == diff 的 new 值才回写 old（防止覆盖后续手动改动）
        let cur: Option<(Option<String>, Option<f64>)> =
            sqlx::query_as("SELECT plan_date, plan_qty FROM tasks WHERE id = ?")
                .bind(task_id)
                .fetch_optional(&mut *tx)
                .await?;
        let Some((cur_date, cur_qty)) = cur else {
            skipped += 1;
            continue; // 任务已删除
        };
        let new_date_matches = d.new_plan_date == cur_date;
        let new_qty_matches = match (d.new_plan_qty, cur_qty) {
            (Some(n), Some(c)) => (n - c).abs() < 1e-9,
            (None, None) => true,
            _ => false,
        };
        if !new_date_matches || !new_qty_matches {
            skipped += 1;
            continue; // 已被后续改动，跳过并累计
        }
        sqlx::query("UPDATE tasks SET plan_date = ?, plan_qty = ?, updated_at = ? WHERE id = ?")
            .bind(&d.old_plan_date)
            .bind(d.old_plan_qty)
            .bind(&updated_ts)
            .bind(task_id)
            .execute(&mut *tx)
            .await?;
        // move_count 回退（不降为负）
        if d.new_plan_date != d.old_plan_date {
            sqlx::query(
                "INSERT INTO task_schedule_meta (task_id, alloc_reason, last_planned_at, move_count)
                 VALUES (?, NULL, NULL, 0)
                 ON CONFLICT(task_id) DO UPDATE SET
                    move_count = MAX(task_schedule_meta.move_count - 1, 0)",
            )
            .bind(task_id)
            .execute(&mut *tx)
            .await?;
        }
        reverted_count += 1;
    }

    sqlx::query("UPDATE replan_events SET status = 'reverted' WHERE id = ?")
        .bind(&event_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    Ok(ReplanApplyResult {
        event_id,
        goal_id: goal_id.clone(),
        moved: reverted_count,
        qty_changed: 0,
        retained: skipped,
        affected_goal_ids: vec![goal_id],
        tasks: Vec::new(),
    })
}
