//! 重新规划 · 理由链（人类可读中文文案）
//!
//! 每个 `TaskPlacement.reason` 产出对应的人读文案，供预览抽屉展示；
//! 纯函数、不访问数据库、不读时钟。

/// 生成人类可读的理由文案
///
/// - `reason`：决策理由枚举（locked / dep_lower_bound / overdue_pull /
///   load_balance / profile_rhythm / smoothing_retain / manual / capacity_trim）
/// - `task_name`：任务名（用于文案定位）
/// - `old` / `new`：变更前/后计划日期（yyyy-MM-dd，无则为 None）
pub fn reason_text(reason: &str, task_name: &str, old: Option<&str>, new: Option<&str>) -> String {
    let name = if task_name.trim().is_empty() {
        "该任务".to_string()
    } else {
        task_name.trim().to_string()
    };
    match reason {
        "locked" => format!("{}：排期已锁定，保持原计划不变", name),
        "manual" => format!("{}：手动排期，保留用户自定义", name),
        "smoothing_retain" => format!("{}：防抖保留，变化幅度低于阈值", name),
        "overdue_pull" => {
            let old_d = old.unwrap_or("过去");
            let new_d = new.unwrap_or("明日");
            format!("{}：逾期回收，由 {} 前移至 {}", name, old_d, new_d)
        }
        "dep_lower_bound" => format!(
            "{}：依赖约束，不得早于前置任务完成次日{}",
            name,
            new.map(|d| format!("（最早 {}", d) + "）").unwrap_or_default()
        ),
        "load_balance" => format!("{}：负载均衡，分散单日压力", name),
        "profile_rhythm" => format!("{}：习惯节奏，贴合用户工作日历", name),
        "capacity_trim" => format!("{}：容量裁剪，超出每日上限后顺延", name),
        other => format!("{}：{}", name, other),
    }
}

/// 变更类型 → 差异表标签（与 replan_event_diffs.change_type 同枚举）
#[allow(dead_code)] // 前端时间线 UI（阶段 3B）消费
pub fn change_type_label(change_type: &str) -> String {
    match change_type {
        "date_moved" => "日期调整".to_string(),
        "qty_changed" => "数量调整".to_string(),
        "both" => "日期与数量调整".to_string(),
        "retained" => "保留".to_string(),
        "locked" => "锁定".to_string(),
        other => other.to_string(),
    }
}
