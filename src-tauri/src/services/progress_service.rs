use std::collections::{HashMap, HashSet};

use sqlx::SqlitePool;

use crate::db::models::{Goal, ProgressInfo};
use crate::error::{AppError, AppResult};

/// 计算单个目标的进度（递归包含所有后代目标的任务）
pub async fn calc_goal_progress(pool: &SqlitePool, goal_id: &str) -> AppResult<ProgressInfo> {
    let goal_name: String = sqlx::query_scalar("SELECT name FROM goals WHERE id = ?")
        .bind(goal_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("目标 {} 不存在", goal_id)))?;

    // 递归收集所有后代目标 ID
    let descendant_ids = collect_descendant_goal_ids(pool, goal_id).await?;

    // 查询所有后代目标（含自身）的任务
    let mut total_plan = 0.0_f64;
    let mut total_actual = 0.0_f64;

    for gid in &descendant_ids {
        let rows: Vec<(f64, f64)> = sqlx::query_as(
            "SELECT plan_qty, actual_qty FROM tasks
             WHERE goal_id = ? AND status != 'skipped'",
        )
        .bind(gid)
        .fetch_all(pool)
        .await?;

        for (pq, aq) in rows {
            total_plan += pq;
            total_actual += aq;
        }
    }

    let percentage = if total_plan > 0.0 {
        (total_actual / total_plan).min(1.0)
    } else {
        0.0
    };

    // 计算完成状态
    let is_completed = check_goal_completed(pool, goal_id).await?;

    Ok(ProgressInfo {
        id: goal_id.to_string(),
        name: goal_name,
        total_plan,
        total_actual,
        percentage,
        is_completed,
    })
}

/// 计算所有目标的进度（批量优化：一次查询，内存计算）
pub async fn calc_all_goals_progress(pool: &SqlitePool) -> AppResult<Vec<ProgressInfo>> {
    // 1. 查询所有目标
    let all_goals: Vec<Goal> =
        sqlx::query_as("SELECT * FROM goals ORDER BY sort_order, created_at")
            .fetch_all(pool)
            .await?;

    // 2. 查询所有任务的聚合数据（按 goal_id 分组）
    let task_rows: Vec<(String, f64, f64, String)> = sqlx::query_as(
        "SELECT goal_id, plan_qty, actual_qty, status FROM tasks WHERE status != 'skipped'",
    )
    .fetch_all(pool)
    .await?;

    // 3. 查询所有任务的状态（用于完成判断，含 skipped）
    let task_status_rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT goal_id, status FROM tasks",
    )
    .fetch_all(pool)
    .await?;

    // 构建 goal_id → 任务状态列表
    let mut tasks_by_goal: HashMap<String, Vec<String>> = HashMap::new();
    for (gid, status) in task_status_rows {
        tasks_by_goal.entry(gid).or_default().push(status);
    }

    // 4. 构建 parent → children 映射
    let mut children_map: HashMap<String, Vec<String>> = HashMap::new();
    for g in &all_goals {
        if let Some(pid) = &g.parent_id {
            children_map.entry(pid.clone()).or_default().push(g.id.clone());
        }
    }

    // 5. 构建 goal_id → (total_plan, total_actual) 聚合
    let mut plan_by_goal: HashMap<String, (f64, f64)> = HashMap::new();
    for (gid, pq, aq, _status) in task_rows {
        let entry = plan_by_goal.entry(gid).or_insert((0.0, 0.0));
        entry.0 += pq;
        entry.1 += aq;
    }

    // 6. 递归计算每个目标的进度（汇总后代）
    let mut progress_cache: HashMap<String, (f64, f64)> = HashMap::new(); // (plan, actual)

    fn calc_recursive(
        goal_id: &str,
        children_map: &HashMap<String, Vec<String>>,
        plan_by_goal: &HashMap<String, (f64, f64)>,
        cache: &mut HashMap<String, (f64, f64)>,
    ) -> (f64, f64) {
        if let Some(cached) = cache.get(goal_id) {
            return *cached;
        }

        let own = plan_by_goal.get(goal_id).copied().unwrap_or((0.0, 0.0));
        let mut total_plan = own.0;
        let mut total_actual = own.1;

        if let Some(children) = children_map.get(goal_id) {
            for child_id in children {
                let (cp, ca) = calc_recursive(child_id, children_map, plan_by_goal, cache);
                total_plan += cp;
                total_actual += ca;
            }
        }

        cache.insert(goal_id.to_string(), (total_plan, total_actual));
        (total_plan, total_actual)
    }

    // 7. 递归计算完成状态
    fn check_completed_recursive(
        goal_id: &str,
        children_map: &HashMap<String, Vec<String>>,
        tasks_by_goal: &HashMap<String, Vec<String>>,
        cache: &mut HashMap<String, bool>,
    ) -> bool {
        if let Some(cached) = cache.get(goal_id) {
            return *cached;
        }

        // 检查直属任务（非 skipped 的必须全 done）
        let own_tasks = tasks_by_goal.get(goal_id);
        let has_own_tasks = own_tasks.map(|t| t.iter().any(|s| s != "skipped")).unwrap_or(false);
        let own_tasks_done = own_tasks
            .map(|t| t.iter().all(|s| s == "done" || s == "skipped"))
            .unwrap_or(true); // 无任务视为全完成（空集真）

        // 检查子目标
        let children = children_map.get(goal_id);
        let has_children = children.map(|c| !c.is_empty()).unwrap_or(false);
        let children_done = children
            .map(|cs| {
                cs.iter()
                    .all(|cid| check_completed_recursive(cid, children_map, tasks_by_goal, cache))
            })
            .unwrap_or(true); // 无子目标视为全完成

        // 必须有内容且全完成
        let result = (has_own_tasks || has_children) && own_tasks_done && children_done;
        cache.insert(goal_id.to_string(), result);
        result
    }

    let mut completion_cache: HashMap<String, bool> = HashMap::new();

    // 8. 构建 ProgressInfo 列表
    let mut result = Vec::with_capacity(all_goals.len());
    for g in &all_goals {
        let (total_plan, total_actual) =
            calc_recursive(&g.id, &children_map, &plan_by_goal, &mut progress_cache);
        let percentage = if total_plan > 0.0 {
            (total_actual / total_plan).min(1.0)
        } else {
            0.0
        };
        let is_completed = check_completed_recursive(
            &g.id,
            &children_map,
            &tasks_by_goal,
            &mut completion_cache,
        );

        result.push(ProgressInfo {
            id: g.id.clone(),
            name: g.name.clone(),
            total_plan,
            total_actual,
            percentage,
            is_completed,
        });
    }

    Ok(result)
}

/// 递归收集目标的所有后代 ID（含自身）
async fn collect_descendant_goal_ids(
    pool: &SqlitePool,
    goal_id: &str,
) -> AppResult<HashSet<String>> {
    let mut result = HashSet::new();
    result.insert(goal_id.to_string());
    let mut queue = vec![goal_id.to_string()];

    while let Some(current) = queue.pop() {
        let children: Vec<String> =
            sqlx::query_scalar("SELECT id FROM goals WHERE parent_id = ?")
                .bind(&current)
                .fetch_all(pool)
                .await?;
        for child in children {
            if result.insert(child.clone()) {
                queue.push(child);
            }
        }
    }

    Ok(result)
}

/// 检查目标是否完成（递归：子目标全完成 + 直属子任务全完成）
pub async fn check_goal_completed(pool: &SqlitePool, goal_id: &str) -> AppResult<bool> {
    // 检查直属子任务（非 skipped 必须全 done）
    let tasks: Vec<String> = sqlx::query_scalar(
        "SELECT status FROM tasks WHERE goal_id = ? AND status != 'skipped'",
    )
    .bind(goal_id)
    .fetch_all(pool)
    .await?;

    let has_tasks = !tasks.is_empty();
    let tasks_done = tasks.iter().all(|s| s == "done");

    // 检查子目标
    let children: Vec<String> =
        sqlx::query_scalar("SELECT id FROM goals WHERE parent_id = ?")
            .bind(goal_id)
            .fetch_all(pool)
            .await?;

    let has_children = !children.is_empty();
    let mut children_done = true;
    for child_id in &children {
        // 递归 async 调用需 Box::pin 引入间接层，避免无限大小的 future
        if !Box::pin(check_goal_completed(pool, child_id)).await? {
            children_done = false;
            break;
        }
    }

    // 必须有内容（任务或子目标）且全完成
    Ok((has_tasks || has_children) && tasks_done && children_done)
}
