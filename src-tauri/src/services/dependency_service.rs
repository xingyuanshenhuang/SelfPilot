use std::collections::{HashMap, HashSet};

use sqlx::SqlitePool;

use crate::db::models::Task;
use crate::error::AppResult;

/// 依赖图子查询片段：返回某任务是否存在未完成的前置依赖
///
/// 用于在 list_today_tasks / list_tasks_by_date_range 中通过子查询填充 is_blocked。
/// 依赖未完成 = 存在 depends_on_id 对应任务 status NOT IN ('done', 'skipped')。
/// 根据项目约束，skipped 视为不存在，不阻塞后续任务。
pub const BLOCKED_EXISTS_SQL: &str = "EXISTS(\
    SELECT 1 FROM task_dependencies d \
    JOIN tasks dep ON d.depends_on_id = dep.id \
    WHERE d.task_id = t.id AND dep.status NOT IN ('done', 'skipped')\
)";

/// 子查询片段：返回阻塞当前任务的前置任务名称列表（顿号分隔），无阻塞时为 NULL
pub const BLOCKED_BY_NAMES_SQL: &str = "(\
    SELECT GROUP_CONCAT(dep.name, '、') \
    FROM task_dependencies d \
    JOIN tasks dep ON d.depends_on_id = dep.id \
    WHERE d.task_id = t.id AND dep.status NOT IN ('done', 'skipped')\
)";

/// 检测添加 `task_id depends_on depends_on_id` 是否会形成循环依赖。
///
/// 原理：添加该边后，若从 `depends_on_id` 沿 depends_on 链能到达 `task_id`，
/// 说明 depends_on_id 已经（直接或间接）依赖 task_id，再添加此边会成环。
///
/// 自依赖（task_id == depends_on_id）也视为环。
pub async fn detect_cycle(
    pool: &SqlitePool,
    task_id: &str,
    depends_on_id: &str,
) -> AppResult<bool> {
    if task_id == depends_on_id {
        return Ok(true);
    }

    // DFS：从 depends_on_id 出发，沿 depends_on 边（即查 X depends_on Y 中 X=cur 的 Y）
    // 看能否到达 task_id
    let mut visited: HashSet<String> = HashSet::new();
    let mut stack: Vec<String> = vec![depends_on_id.to_string()];

    while let Some(cur) = stack.pop() {
        if !visited.insert(cur.clone()) {
            continue;
        }
        if cur == task_id {
            return Ok(true);
        }
        // 查询 cur 的所有 depends_on_id
        let deps: Vec<String> =
            sqlx::query_scalar("SELECT depends_on_id FROM task_dependencies WHERE task_id = ?")
                .bind(&cur)
                .fetch_all(pool)
                .await?;
        for d in deps {
            if !visited.contains(&d) {
                stack.push(d);
            }
        }
    }

    Ok(false)
}

/// 判断某任务是否存在未完成的前置依赖
pub async fn has_unfinished_dependencies(pool: &SqlitePool, task_id: &str) -> AppResult<bool> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM task_dependencies d
         JOIN tasks dep ON d.depends_on_id = dep.id
         WHERE d.task_id = ? AND dep.status NOT IN ('done', 'skipped')",
    )
    .bind(task_id)
    .fetch_one(pool)
    .await?;
    Ok(count > 0)
}

/// 列出某任务的直接前置依赖任务（task_id 依赖哪些任务）
pub async fn list_dependencies(pool: &SqlitePool, task_id: &str) -> AppResult<Vec<Task>> {
    let tasks: Vec<Task> = sqlx::query_as(
        "SELECT t.* FROM tasks t
         JOIN task_dependencies d ON d.depends_on_id = t.id
         WHERE d.task_id = ?
         ORDER BY t.plan_date, t.sort_order",
    )
    .bind(task_id)
    .fetch_all(pool)
    .await?;
    Ok(tasks)
}

/// 列出依赖某任务的后继任务（哪些任务依赖 task_id）
pub async fn list_dependents(pool: &SqlitePool, task_id: &str) -> AppResult<Vec<Task>> {
    let tasks: Vec<Task> = sqlx::query_as(
        "SELECT t.* FROM tasks t
         JOIN task_dependencies d ON d.task_id = t.id
         WHERE d.depends_on_id = ?
         ORDER BY t.plan_date, t.sort_order",
    )
    .bind(task_id)
    .fetch_all(pool)
    .await?;
    Ok(tasks)
}

/// 加载某目标下所有任务的依赖关系（task_id → 其 depends_on_id 列表）
///
/// 仅返回 task_id 与 depends_on_id 均属于该目标的依赖，用于重新规划时的拓扑排序。
pub async fn load_goal_dependency_map(
    pool: &SqlitePool,
    goal_id: &str,
) -> AppResult<HashMap<String, Vec<String>>> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT d.task_id, d.depends_on_id
         FROM task_dependencies d
         JOIN tasks t1 ON t1.id = d.task_id
         JOIN tasks t2 ON t2.id = d.depends_on_id
         WHERE t1.goal_id = ? AND t2.goal_id = ?",
    )
    .bind(goal_id)
    .bind(goal_id)
    .fetch_all(pool)
    .await?;

    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    for (task_id, depends_on_id) in rows {
        map.entry(task_id).or_default().push(depends_on_id);
    }
    Ok(map)
}

/// 按依赖关系对任务列表做拓扑排序（被依赖的任务排前）。
///
/// 用于重新规划：被依赖（作为前置）的任务排在前面，以便余数优先分配。
/// - 仅考虑 `tasks` 集合内部的依赖（dep_map 中 depends_on_id 不在集合内的忽略）
/// - 使用 Kahn 算法：入度（在集合内依赖的数量）为 0 的先输出
/// - 同层按 sort_order 稳定排序，保证输出确定
/// - 若存在环（理论不应发生，set_task_dependency 已防环），回退到原 sort_order
pub fn topo_sort_by_dependency(
    tasks: &[Task],
    dep_map: &HashMap<String, Vec<String>>,
) -> Vec<Task> {
    use std::collections::VecDeque;

    let id_set: HashSet<&str> = tasks.iter().map(|t| t.id.as_str()).collect();

    // 入度 = 该任务在集合内依赖的数量
    let mut in_degree: HashMap<&str, usize> = HashMap::new();
    // 邻接表：被依赖任务 → 依赖它的任务列表（depends_on_id → [task_id]）
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();

    for t in tasks {
        in_degree.entry(t.id.as_str()).or_insert(0);
        if let Some(deps) = dep_map.get(&t.id) {
            for dep in deps {
                if id_set.contains(dep.as_str()) {
                    // 边 dep → t.id（dep 在前）
                    adj.entry(dep.as_str()).or_default().push(t.id.as_str());
                    *in_degree.entry(t.id.as_str()).or_insert(0) += 1;
                }
            }
        }
    }

    // 初始队列：入度为 0 的任务，按 sort_order 排序保证稳定
    let mut zero_in: Vec<&Task> = tasks.iter().filter(|t| in_degree[t.id.as_str()] == 0).collect();
    zero_in.sort_by_key(|t| t.sort_order);

    let mut queue: VecDeque<&Task> = zero_in.into_iter().collect();
    let mut result: Vec<Task> = Vec::with_capacity(tasks.len());
    let mut processed: HashSet<&str> = HashSet::new();

    while let Some(t) = queue.pop_front() {
        if !processed.insert(t.id.as_str()) {
            continue;
        }
        result.push(t.clone());

        // 释放依赖 t 的任务
        if let Some(nexts) = adj.get(t.id.as_str()) {
            // 收集新入度为 0 的，按 sort_order 排序
            let mut ready: Vec<&Task> = Vec::new();
            for nid in nexts {
                if let Some(deg) = in_degree.get_mut(*nid) {
                    *deg = deg.saturating_sub(1);
                    if *deg == 0 && !processed.contains(*nid) {
                        if let Some(nt) = tasks.iter().find(|x| x.id.as_str() == *nid) {
                            ready.push(nt);
                        }
                    }
                }
            }
            ready.sort_by_key(|t| t.sort_order);
            for r in ready {
                queue.push_back(r);
            }
        }
    }

    // 环兜底：未被处理的任务按 sort_order 追加到末尾
    if result.len() < tasks.len() {
        let mut remaining: Vec<&Task> = tasks
            .iter()
            .filter(|t| !processed.contains(t.id.as_str()))
            .collect();
        remaining.sort_by_key(|t| t.sort_order);
        for t in remaining {
            result.push(t.clone());
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_task(id: &str, sort_order: i64) -> Task {
        Task {
            id: id.to_string(),
            goal_id: "g1".to_string(),
            stage_id: None,
            parent_id: None,
            path: format!("/g1/{}", id),
            name: id.to_string(),
            plan_date: None,
            overdue_date: None,
            plan_qty: 1.0,
            actual_qty: 0.0,
            unit: "".to_string(),
            status: "pending".to_string(),
            is_manual: 0,
            source: "auto".to_string(),
            sort_order,
            created_at: "2026-07-07T00:00:00".to_string(),
            estimated_hours: None,
        }
    }

    #[test]
    fn test_topo_sort_basic() {
        // C 依赖 A，B 依赖 A → A 应在最前
        let tasks = vec![make_task("A", 2), make_task("B", 1), make_task("C", 0)];
        let mut deps = HashMap::new();
        deps.insert("C".to_string(), vec!["A".to_string()]);
        deps.insert("B".to_string(), vec!["A".to_string()]);

        let sorted = topo_sort_by_dependency(&tasks, &deps);
        assert_eq!(sorted[0].id, "A");
        // B 和 C 谁先看 sort_order（都入度0后按 sort_order）
        assert_eq!(sorted.len(), 3);
    }

    #[test]
    fn test_topo_sort_chain() {
        // A ← B ← C（C 依赖 B，B 依赖 A）→ 顺序 A, B, C
        let tasks = vec![make_task("C", 0), make_task("B", 1), make_task("A", 2)];
        let mut deps = HashMap::new();
        deps.insert("C".to_string(), vec!["B".to_string()]);
        deps.insert("B".to_string(), vec!["A".to_string()]);

        let sorted = topo_sort_by_dependency(&tasks, &deps);
        let ids: Vec<&str> = sorted.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, vec!["A", "B", "C"]);
    }

    #[test]
    fn test_topo_sort_no_deps() {
        let tasks = vec![make_task("A", 2), make_task("B", 0), make_task("C", 1)];
        let deps = HashMap::new();
        let sorted = topo_sort_by_dependency(&tasks, &deps);
        // 无依赖时按 sort_order
        let ids: Vec<&str> = sorted.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, vec!["B", "C", "A"]);
    }

    #[test]
    fn test_topo_sort_cross_goal_dep_ignored() {
        // 依赖指向不在集合内的任务 → 忽略，按 sort_order
        let tasks = vec![make_task("A", 1), make_task("B", 0)];
        let mut deps = HashMap::new();
        // B 依赖 X（X 不在 tasks 中）
        deps.insert("B".to_string(), vec!["X".to_string()]);
        let sorted = topo_sort_by_dependency(&tasks, &deps);
        let ids: Vec<&str> = sorted.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, vec!["B", "A"]);
    }
}
