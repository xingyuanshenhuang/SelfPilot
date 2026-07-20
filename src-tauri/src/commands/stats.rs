use tauri::State;

use crate::db::models::{
    CelebrationAchievement, CompletionPrediction, DailyLoad, DailyTrend, GoalCompletionStat,
    GoalLoad, HeatmapCell,
};
use crate::db::DbPool;
use crate::error::AppResult;

/// 获取近 N 天每日完成趋势
///
/// PRD §4.2 模块六 & 分阶段计划 Sprint 3：
/// - 以 plan_date 为统计维度（按日计划分组）
/// - completed_qty：数量型按 actual_qty 统计（截断到 plan_qty），布尔型 done 计 1
/// - completed_count：status='done' 的任务条数
/// - 已跳过任务不计入
#[tauri::command]
pub async fn get_completion_trend(
    days: i64,
    state: State<'_, DbPool>,
) -> AppResult<Vec<DailyTrend>> {
    // 计算起始日期 = today - (days - 1)
    let today = chrono::Local::now().date_naive();
    let start_date = today - chrono::Duration::days(days - 1);
    let start_str = start_date.format("%Y-%m-%d").to_string();
    let end_str = today.format("%Y-%m-%d").to_string();

    // 一次查询拿到所有相关任务，按日期分组在应用层聚合
    // 字段：plan_date, plan_qty, actual_qty, status
    let rows: Vec<(Option<String>, f64, f64, String)> = sqlx::query_as(
        "SELECT plan_date, plan_qty, actual_qty, status FROM tasks
         WHERE plan_date IS NOT NULL
           AND plan_date >= ? AND plan_date <= ?
           AND status != 'skipped'",
    )
    .bind(&start_str)
    .bind(&end_str)
    .fetch_all(&state.0)
    .await?;

    // 用 BTreeMap 保证日期有序
    use std::collections::BTreeMap;
    let mut daily_map: BTreeMap<String, (f64, i64)> = BTreeMap::new();

    for (plan_date, plan_qty, actual_qty, status) in rows {
        if let Some(date) = plan_date {
            let entry = daily_map.entry(date).or_insert((0.0_f64, 0_i64));
            // 完成数量：取 min(actual_qty, plan_qty) 的正值部分
            let qty_done = if actual_qty > 0.0 {
                actual_qty.min(plan_qty)
            } else {
                0.0
            };
            entry.0 += qty_done;
            if status == "done" {
                entry.1 += 1;
            }
        }
    }

    // 填充无任务的日期（趋势线连续）
    let mut result: Vec<DailyTrend> = Vec::with_capacity(days as usize);
    for i in 0..days {
        let date = (start_date + chrono::Duration::days(i)).format("%Y-%m-%d").to_string();
        let (completed_qty, completed_count) = daily_map.remove(&date).unwrap_or((0.0, 0));
        result.push(DailyTrend {
            date,
            completed_qty,
            completed_count,
        });
    }

    Ok(result)
}

/// 获取所有目标的完成统计（柱状图）
///
/// PRD §4.2 模块六 & 分阶段计划 Sprint 3：
/// - 各目标完成百分比、任务总数、已完成任务数
/// - 已跳过任务不计入
#[tauri::command]
pub async fn get_goal_completion_stats(
    state: State<'_, DbPool>,
) -> AppResult<Vec<GoalCompletionStat>> {
    let stats: Vec<GoalCompletionStat> = sqlx::query_as(
        "SELECT g.id, g.name,
                CAST(COALESCE(SUM(CASE WHEN t.status != 'skipped' THEN t.plan_qty ELSE 0 END), 0) AS REAL) as total_plan,
                CAST(COALESCE(SUM(t.actual_qty), 0) AS REAL) as total_actual,
                CASE
                    WHEN COALESCE(SUM(CASE WHEN t.status != 'skipped' THEN t.plan_qty ELSE 0 END), 0) = 0 THEN 0.0
                    ELSE MIN(1.0, COALESCE(SUM(t.actual_qty), 0) * 1.0
                        / COALESCE(SUM(CASE WHEN t.status != 'skipped' THEN t.plan_qty ELSE 0 END), 0))
                END as percentage,
                COUNT(CASE WHEN t.status != 'skipped' THEN 1 END) as task_count,
                COUNT(CASE WHEN t.status = 'done' THEN 1 END) as done_count
         FROM goals g
         LEFT JOIN tasks t ON t.goal_id = g.id
         WHERE g.parent_id IS NULL
         GROUP BY g.id, g.name
         ORDER BY g.created_at",
    )
    .fetch_all(&state.0)
    .await?;

    Ok(stats)
}

/// 获取日历热力图数据
///
/// PRD §4.2 模块六 & 分阶段计划 Sprint 5：
/// 按日期格子颜色深浅展示每日"完成任务量 / 当日应有任务总量"的比例
/// - 数量型按 actual_qty 截断到 plan_qty 统计完成量
/// - 布尔型 done 计 1
/// - 已跳过任务不计入
/// - 填充无任务日期（completion_rate = 0）保证热力图连续
#[tauri::command]
pub async fn get_heatmap(
    start_date: String,
    end_date: String,
    state: State<'_, DbPool>,
) -> AppResult<Vec<HeatmapCell>> {
    // 查询范围内的所有非跳过任务
    let rows: Vec<(String, f64, f64, String)> = sqlx::query_as(
        "SELECT plan_date, plan_qty, actual_qty, status FROM tasks
         WHERE plan_date IS NOT NULL
           AND plan_date >= ? AND plan_date <= ?
           AND status != 'skipped'",
    )
    .bind(&start_date)
    .bind(&end_date)
    .fetch_all(&state.0)
    .await?;

    use std::collections::BTreeMap;
    let mut daily_map: BTreeMap<String, (f64, f64, i64, i64)> = BTreeMap::new();

    for (plan_date, plan_qty, actual_qty, status) in rows {
        let entry = daily_map
            .entry(plan_date)
            .or_insert((0.0_f64, 0.0_f64, 0_i64, 0_i64));
        entry.0 += plan_qty; // plan_qty
        // 完成量：取 min(actual_qty, plan_qty) 的正值
        let qty_done = if actual_qty > 0.0 {
            actual_qty.min(plan_qty)
        } else {
            0.0
        };
        entry.1 += qty_done; // completed_qty
        entry.2 += 1; // task_count
        if status == "done" {
            entry.3 += 1; // done_count
        }
    }

    // 填充日期范围内的每一天（含无任务日）
    let start = chrono::NaiveDate::parse_from_str(&start_date, "%Y-%m-%d")
        .map_err(|e| crate::error::AppError::Param(format!("起始日期格式错误: {}", e)))?;
    let end = chrono::NaiveDate::parse_from_str(&end_date, "%Y-%m-%d")
        .map_err(|e| crate::error::AppError::Param(format!("结束日期格式错误: {}", e)))?;

    let mut result: Vec<HeatmapCell> = Vec::new();
    let mut cursor = start;
    while cursor <= end {
        let date_str = cursor.format("%Y-%m-%d").to_string();
        let (plan_qty, completed_qty, task_count, done_count) =
            daily_map.remove(&date_str).unwrap_or((0.0, 0.0, 0, 0));
        let completion_rate = if plan_qty > 0.0 {
            (completed_qty / plan_qty).min(1.0)
        } else {
            0.0
        };
        result.push(HeatmapCell {
            date: date_str,
            plan_qty,
            completed_qty,
            task_count,
            done_count,
            completion_rate,
        });
        cursor = cursor + chrono::Duration::days(1);
    }

    Ok(result)
}

/// 获取所有目标的完成预测
///
/// PRD §4.2 模块六 & 分阶段计划 Sprint 6：
/// 基于过去7天平均完成速度，预测每个目标能否按期完成
/// - 计算过去7天每日完成量的平均值（avg_daily_speed）
/// - remaining_qty = total_qty - completed_qty
/// - predicted_days = remaining_qty / avg_daily_speed
/// - 与 deadline 比较给出状态和建议
#[tauri::command]
pub async fn get_completion_predictions(
    state: State<'_, DbPool>,
) -> AppResult<Vec<CompletionPrediction>> {
    let today = chrono::Local::now().date_naive();
    let seven_days_ago = today - chrono::Duration::days(6); // 含今天共7天
    let start_str = seven_days_ago.format("%Y-%m-%d").to_string();
    let end_str = today.format("%Y-%m-%d").to_string();

    // 1. 查询所有根目标的基础信息
    let goals: Vec<(String, String, Option<String>, f64)> = sqlx::query_as(
        "SELECT id, name, deadline, total_qty FROM goals WHERE parent_id IS NULL ORDER BY created_at",
    )
    .fetch_all(&state.0)
    .await?;

    // 2. 查询每个目标的已完成量
    let goal_completed: Vec<(String, f64)> = sqlx::query_as(
        "SELECT goal_id, CAST(COALESCE(SUM(actual_qty), 0) AS REAL) FROM tasks
         WHERE status != 'skipped' GROUP BY goal_id",
    )
    .fetch_all(&state.0)
    .await?;

    use std::collections::HashMap;
    let mut completed_map: HashMap<String, f64> = HashMap::new();
    for (gid, qty) in goal_completed {
        completed_map.insert(gid, qty);
    }

    // 3. 查询过去7天每个目标的每日完成量（用于计算均速）
    let daily_rows: Vec<(String, String, f64)> = sqlx::query_as(
        "SELECT goal_id, plan_date, CAST(COALESCE(SUM(actual_qty), 0) AS REAL) FROM tasks
         WHERE plan_date IS NOT NULL
           AND plan_date >= ? AND plan_date <= ?
           AND status != 'skipped'
         GROUP BY goal_id, plan_date",
    )
    .bind(&start_str)
    .bind(&end_str)
    .fetch_all(&state.0)
    .await?;

    // goal_id -> 总完成量（过去7天）
    let mut speed_map: HashMap<String, f64> = HashMap::new();
    for (gid, _, qty) in daily_rows {
        *speed_map.entry(gid).or_insert(0.0) += qty;
    }

    // 4. 构建预测结果
    let mut predictions: Vec<CompletionPrediction> = Vec::new();
    for (goal_id, goal_name, deadline_str, total_qty) in goals {
        let completed_qty = *completed_map.get(&goal_id).unwrap_or(&0.0);
        let remaining_qty = (total_qty - completed_qty).max(0.0);

        // 过去7天总完成量 / 7 = 日均速度
        let weekly_total = *speed_map.get(&goal_id).unwrap_or(&0.0);
        let avg_daily_speed = weekly_total / 7.0;

        // 已全部完成
        if remaining_qty <= 0.0 || total_qty <= 0.0 {
            let days_to_deadline = deadline_str
                .as_ref()
                .and_then(|d| {
                    chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d")
                        .ok()
                        .map(|dl| (dl - today).num_days())
                });
            predictions.push(CompletionPrediction {
                goal_id,
                goal_name,
                deadline: deadline_str,
                total_qty,
                completed_qty,
                remaining_qty: 0.0,
                avg_daily_speed,
                predicted_days: Some(0),
                predicted_date: Some(today.format("%Y-%m-%d").to_string()),
                days_to_deadline,
                status: "completed".to_string(),
                suggestion: "目标已全部完成！".to_string(),
            });
            continue;
        }

        // 无截止日期
        let deadline = deadline_str
            .as_ref()
            .and_then(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok());

        if deadline.is_none() {
            let predicted_days = if avg_daily_speed > 0.0 {
                Some((remaining_qty / avg_daily_speed).ceil() as i64)
            } else {
                None
            };
            let predicted_date = predicted_days.map(|d| {
                (today + chrono::Duration::days(d)).format("%Y-%m-%d").to_string()
            });
            predictions.push(CompletionPrediction {
                goal_id,
                goal_name,
                deadline: deadline_str,
                total_qty,
                completed_qty,
                remaining_qty,
                avg_daily_speed,
                predicted_days,
                predicted_date,
                days_to_deadline: None,
                status: "no_deadline".to_string(),
                suggestion: if avg_daily_speed > 0.0 {
                    format!(
                        "按当前速度预计还需 {} 天完成",
                        (remaining_qty / avg_daily_speed).ceil() as i64
                    )
                } else {
                    "近7天无完成记录，建议开始打卡".to_string()
                },
            });
            continue;
        }

        let Some(dl) = deadline else {
            continue;
        };
        let days_to_deadline = (dl - today).num_days();

        // 无历史数据
        if avg_daily_speed <= 0.0 {
            let status = if days_to_deadline < 0 {
                "need_speed"
            } else {
                "no_data"
            };
            let suggestion = if days_to_deadline < 0 {
                format!("已逾期 {} 天，且近7天无完成记录，请立即开始", -days_to_deadline)
            } else if days_to_deadline == 0 {
                "今天截止，但近7天无完成记录".to_string()
            } else {
                format!(
                    "剩余 {} 天，近7天无完成记录，建议立即开始打卡",
                    days_to_deadline
                )
            };
            predictions.push(CompletionPrediction {
                goal_id,
                goal_name,
                deadline: deadline_str,
                total_qty,
                completed_qty,
                remaining_qty,
                avg_daily_speed,
                predicted_days: None,
                predicted_date: None,
                days_to_deadline: Some(days_to_deadline),
                status: status.to_string(),
                suggestion,
            });
            continue;
        }

        // 有数据：计算预测天数
        let predicted_days = (remaining_qty / avg_daily_speed).ceil() as i64;
        let predicted_date =
            (today + chrono::Duration::days(predicted_days)).format("%Y-%m-%d").to_string();

        // 判断状态
        let (status, suggestion) = if days_to_deadline < 0 {
            (
                "need_speed",
                format!(
                    "已逾期 {} 天，按当前速度还需 {} 天完成，建议大幅提速",
                    -days_to_deadline,
                    predicted_days
                ),
            )
        } else if predicted_days < days_to_deadline {
            (
                "ahead",
                format!(
                    "可提前 {} 天完成（预计 {} 完成）",
                    days_to_deadline - predicted_days,
                    predicted_date
                ),
            )
        } else if predicted_days <= days_to_deadline {
            (
                "on_track",
                format!("按期完成（预计 {} 完成）", predicted_date),
            )
        } else {
            (
                "need_speed",
                format!(
                    "需提高速度：按当前速度需 {} 天，但仅剩 {} 天，建议日完成量提升至 {:.1}",
                    predicted_days,
                    days_to_deadline,
                    remaining_qty / days_to_deadline as f64
                ),
            )
        };

        predictions.push(CompletionPrediction {
            goal_id,
            goal_name,
            deadline: deadline_str,
            total_qty,
            completed_qty,
            remaining_qty,
            avg_daily_speed,
            predicted_days: Some(predicted_days),
            predicted_date: Some(predicted_date),
            days_to_deadline: Some(days_to_deadline),
            status: status.to_string(),
            suggestion,
        });
    }

    Ok(predictions)
}

/// 获取日期范围内每日负载（按目标分组）— P2-5 跨目标负载平衡
///
/// 用于：
/// - 日历视图色阶显示（按 total_tasks 分级）
/// - 拆解前过载检查（按 total_tasks 阈值警告）
/// - 已跳过任务不计入负载
#[tauri::command]
pub async fn get_daily_load(
    start_date: String,
    end_date: String,
    state: State<'_, DbPool>,
) -> AppResult<Vec<DailyLoad>> {
    // SQL：JOIN goals 拿名称，按 plan_date + goal_id 分组
    let rows: Vec<(String, String, String, i64, f64)> = sqlx::query_as(
        "SELECT t.plan_date, t.goal_id, g.name,
                COUNT(*) as task_count,
                CAST(COALESCE(SUM(t.plan_qty), 0) AS REAL) as total_qty
         FROM tasks t
         INNER JOIN goals g ON t.goal_id = g.id
         WHERE t.plan_date IS NOT NULL
           AND t.plan_date >= ? AND t.plan_date <= ?
           AND t.status != 'skipped'
         GROUP BY t.plan_date, t.goal_id, g.name
         ORDER BY t.plan_date",
    )
    .bind(&start_date)
    .bind(&end_date)
    .fetch_all(&state.0)
    .await?;

    // 应用层聚合：按日期分组
    use std::collections::BTreeMap;
    let mut daily_map: BTreeMap<String, (i64, f64, Vec<GoalLoad>)> = BTreeMap::new();
    for (date, goal_id, goal_name, task_count, total_qty) in rows {
        let entry = daily_map.entry(date).or_insert((0, 0.0, Vec::new()));
        entry.0 += task_count;
        entry.1 += total_qty;
        entry.2.push(GoalLoad {
            goal_id,
            goal_name,
            task_count,
            total_qty,
        });
    }

    // 填充日期范围内每一天（含无任务日）
    let start = chrono::NaiveDate::parse_from_str(&start_date, "%Y-%m-%d")
        .map_err(|e| crate::error::AppError::Param(format!("起始日期格式错误: {}", e)))?;
    let end = chrono::NaiveDate::parse_from_str(&end_date, "%Y-%m-%d")
        .map_err(|e| crate::error::AppError::Param(format!("结束日期格式错误: {}", e)))?;

    let mut result = Vec::new();
    let mut cursor = start;
    while cursor <= end {
        let date_str = cursor.format("%Y-%m-%d").to_string();
        let (total_tasks, total_qty, by_goal) =
            daily_map.remove(&date_str).unwrap_or((0, 0.0, Vec::new()));
        result.push(DailyLoad {
            date: date_str,
            total_tasks,
            total_qty,
            by_goal,
        });
        cursor += chrono::Duration::days(1);
    }
    Ok(result)
}

// ============================================================
// P1-3：庆祝成就数据
// ============================================================

/// 获取庆祝成就数据
///
/// 全部目标完成时调用，返回成就回顾信息
#[tauri::command]
pub async fn get_celebration_achievement(
    state: State<'_, DbPool>,
) -> AppResult<CelebrationAchievement> {
    let today = chrono::Local::now().date_naive();

    // 1. 统计根目标数
    let goal_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM goals WHERE parent_id IS NULL",
    )
    .fetch_one(&state.0)
    .await?;

    // 2. 统计任务总数和已完成数
    let (total_tasks, completed_tasks): (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*), SUM(CASE WHEN status = 'done' THEN 1 ELSE 0 END) 
         FROM tasks WHERE status != 'skipped'",
    )
    .fetch_one(&state.0)
    .await?;

    // 3. 计算耗时（从首个任务创建到今天）
    let first_task_date: Option<String> = sqlx::query_scalar(
        "SELECT MIN(created_at) FROM tasks",
    )
    .fetch_optional(&state.0)
    .await?;

    let days_elapsed = if let Some(first_date) = first_task_date {
        // 解析 ISO 8601 格式：2026-07-19T15:30:00
        let first = chrono::NaiveDateTime::parse_from_str(&first_date, "%Y-%m-%dT%H:%M:%S")
            .map(|dt| dt.date())
            .ok();
        if let Some(first) = first {
            (today - first).num_days() as i32
        } else {
            0
        }
    } else {
        0
    };

    // 4. 获取连续天数（简化：查询 settings 或重新计算）
    // 使用已有的 get_streak 命令逻辑，这里直接查询
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
    let mut cursor = today - chrono::Duration::days(1);
    let today_entry = day_map.get(&today);
    let _completed_today = today_entry.map(|(_, c)| *c).unwrap_or(false);

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

    let final_longest_streak = current_streak; // 简化：使用当前连续

    Ok(CelebrationAchievement {
        total_goals: goal_count as i32,
        total_tasks: total_tasks as i32,
        completed_tasks: completed_tasks as i32,
        days_elapsed,
        final_streak: current_streak as i32,
        final_longest_streak: final_longest_streak as i32,
    })
}
