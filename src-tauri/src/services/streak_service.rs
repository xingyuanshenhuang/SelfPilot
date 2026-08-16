//! 连续天数（streak）计算服务
//!
//! 统一原先分散在 commands/encouragement.rs、commands/stats.rs 中的三份重复实现，
//! 并修复 get_streak_inner / get_celebration_achievement 中 longest_streak
//! 被错误简化为 current_streak 的隐性 bug。
//!
//! 算法说明（从今天往前推）：
//! 1. 若今日有任务但未完成任何 → current_streak = 0
//! 2. 若今日无任务 → 从昨日开始往前统计
//! 3. 若今日已完成 → 从今日开始往前统计
//! 4. 遇到"有任务但未完成"的日期 → 中断
//! 5. 遇到"无任务"的日期 → 跳过（不中断）
//!
//! longest_streak：遍历所有有任务的日期，找历史最长连续段（允许中间有无任务日）。

use std::collections::HashMap;

use chrono::{Local, NaiveDate, Duration};

use crate::db::models::StreakInfo;
use crate::error::AppResult;

/// 日期完成状态： (has_task, completed)
type DayStatus = (bool, bool);

/// 查询所有有任务日期的完成状态
///
/// 返回 HashMap<日期, (是否有任务, 是否至少完成一个)>
async fn load_day_map(
    pool: &sqlx::SqlitePool,
) -> AppResult<HashMap<NaiveDate, DayStatus>> {
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

    let mut day_map: HashMap<NaiveDate, DayStatus> = HashMap::new();
    for (date_str, task_count, done_count) in rows {
        if let Ok(d) = NaiveDate::parse_from_str(&date_str, "%Y-%m-%d") {
            let has_task = task_count > 0;
            let completed = done_count > 0;
            day_map.insert(d, (has_task, completed));
        }
    }
    Ok(day_map)
}

/// 计算当前连续天数（从今天往前推）
///
/// 返回 (current_streak, completed_today)
fn calc_current_streak(
    today: NaiveDate,
    day_map: &HashMap<NaiveDate, DayStatus>,
) -> (i64, bool) {
    let mut current_streak: i64 = 0;
    // cursor 初始化为昨日（所有分支都会从昨日开始往前推）
    let mut cursor = today - Duration::days(1);

    let today_entry = day_map.get(&today);
    let completed_today = today_entry.map(|(_, c)| *c).unwrap_or(false);

    match today_entry {
        None => {
            // 今日无任务，从昨日开始（cursor 已是昨日）
        }
        Some((true, false)) => {
            // 今日有任务但未完成 → 中断
            current_streak = 0;
        }
        Some((true, true)) => {
            // 今日已完成
            current_streak = 1;
        }
        _ => {}
    }

    // 今日有任务但未完成时，跳过循环
    let today_unfinished = matches!(today_entry, Some((true, false)));
    if !today_unfinished {
        loop {
            let entry = day_map.get(&cursor);
            match entry {
                None => {
                    // 无任务日，跳过（不中断）
                    cursor = cursor - Duration::days(1);
                }
                Some((true, true)) => {
                    // 有任务且完成 → 连续+1
                    current_streak += 1;
                    cursor = cursor - Duration::days(1);
                }
                Some((true, false)) => {
                    // 有任务但未完成 → 中断
                    break;
                }
                _ => {
                    cursor = cursor - Duration::days(1);
                }
            }

            // 防止无限循环（最多回溯 10 年）
            if (today - cursor).num_days() > 3650 {
                break;
            }
        }
    }

    (current_streak, completed_today)
}

/// 计算历史最长连续天数
///
/// 遍历所有有任务的日期，按日期升序逐个判断：
/// - 已完成且与前一个已完成日期之间无"有任务但未完成"的日期 → 连续+1
/// - 否则 → 从当前日期重新计数为 1
fn calc_longest_streak(day_map: &HashMap<NaiveDate, DayStatus>) -> i64 {
    let mut longest_streak: i64 = 0;
    let mut temp_streak: i64 = 0;
    let mut last_date: Option<NaiveDate> = None;

    let mut sorted_dates: Vec<NaiveDate> = day_map.keys().copied().collect();
    sorted_dates.sort();

    for d in &sorted_dates {
        let (has_task, completed) = day_map[d];
        if !has_task {
            continue;
        }
        if completed {
            // 检查与上一个已完成日期的连续性（允许中间有无任务日）
            let should_continue = match last_date {
                None => true,
                Some(last) => {
                    // 从 last 到 d 之间，所有有任务的日期都应已完成
                    let mut check = last + Duration::days(1);
                    let mut ok = true;
                    while check < *d {
                        if let Some((ht, comp)) = day_map.get(&check) {
                            if *ht && !*comp {
                                ok = false;
                                break;
                            }
                        }
                        check = check + Duration::days(1);
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

    longest_streak
}

/// 计算 P2-4 里程碑成就
fn calc_milestone(longest_streak: i64) -> String {
    if longest_streak >= 30 {
        "master".to_string()
    } else if longest_streak >= 14 {
        "expert".to_string()
    } else {
        "none".to_string()
    }
}

/// 统一入口：计算连续天数信息
///
/// 替代原先分散在 encouragement.rs / stats.rs 中的三份重复实现。
/// 注意：此处 longest_streak 调用完整的 calc_longest_streak 算法，
/// 而非简化为 current_streak（修复原 get_streak_inner 的隐性 bug）。
pub async fn calc_streak(pool: &sqlx::SqlitePool) -> AppResult<StreakInfo> {
    let today = Local::now().date_naive();
    let day_map = load_day_map(pool).await?;

    let (current_streak, completed_today) = calc_current_streak(today, &day_map);
    let mut longest_streak = calc_longest_streak(&day_map);

    // 确保 longest_streak 至少等于 current_streak
    if current_streak > longest_streak {
        longest_streak = current_streak;
    }

    let milestone = calc_milestone(longest_streak);

    Ok(StreakInfo {
        current_streak,
        longest_streak,
        completed_today,
        milestone,
    })
}
