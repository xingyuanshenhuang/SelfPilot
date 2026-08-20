use serde::{Deserialize, Serialize};
use validator::{Validate, ValidationError};

// ============================================================
// S-05 (SEC-M-05/SEC-M-06)：入参校验常量与辅助函数
// ============================================================

/// 批量操作数组长度上限
pub const MAX_BATCH_SIZE: usize = 500;

/// 导入数据 JSON 大小上限（50MB）
pub const MAX_IMPORT_JSON_BYTES: usize = 50 * 1024 * 1024;

/// 校验名称：去除首尾空白后非空，且不超过 200 字符（中文友好）
fn validate_name(s: &str) -> Result<(), ValidationError> {
    if s.trim().is_empty() {
        Err(ValidationError::new("名称不能为空白"))
    } else if s.chars().count() > 200 {
        Err(ValidationError::new("名称长度不能超过 200 字"))
    } else {
        Ok(())
    }
}

/// 校验日期格式 yyyy-MM-dd（严格，不允许空串）
fn validate_date_format(s: &str) -> Result<(), ValidationError> {
    if chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok() {
        Ok(())
    } else {
        Err(ValidationError::new("日期格式必须为 yyyy-MM-dd"))
    }
}

/// 校验日期格式 yyyy-MM-dd（宽松：空串表示"清除日期"，用于更新场景）
fn validate_date_optional_clear(s: &str) -> Result<(), ValidationError> {
    if s.is_empty() {
        Ok(())
    } else {
        validate_date_format(s)
    }
}

/// 校验数量：非负有限数（拒绝 NaN / Infinity / 负数）
///
/// 注：validator 0.18 对数值字段按值传递（f64: Copy），故此处不取引用
fn validate_qty(v: f64) -> Result<(), ValidationError> {
    if v.is_finite() && v >= 0.0 {
        Ok(())
    } else {
        Err(ValidationError::new("数量必须为非负有限数"))
    }
}

/// 校验鼓励语文本：非空白且不超过 100 字
fn validate_encouragement_text(s: &str) -> Result<(), ValidationError> {
    if s.trim().is_empty() {
        Err(ValidationError::new("鼓励语内容不能为空"))
    } else if s.chars().count() > 100 {
        Err(ValidationError::new("鼓励语长度不能超过 100 字"))
    } else {
        Ok(())
    }
}

/// 校验用户可设置的鼓励语等级（预设专用 longest_streak 不开放给用户输入）
fn validate_user_level(s: &str) -> Result<(), ValidationError> {
    if ["normal", "advanced", "highlight", "celebration", "setback"].contains(&s) {
        Ok(())
    } else {
        Err(ValidationError::new(
            "等级必须为 normal/advanced/highlight/celebration/setback",
        ))
    }
}

/// 校验重复拆解频率
fn validate_frequency(s: &str) -> Result<(), ValidationError> {
    if ["daily", "weekly", "monthly"].contains(&s) {
        Ok(())
    } else {
        Err(ValidationError::new("频率必须为 daily/weekly/monthly"))
    }
}

/// 校验智能拆解策略
fn validate_split_strategy(s: &str) -> Result<(), ValidationError> {
    if ["by_deadline", "by_capacity", "by_date_range"].contains(&s) {
        Ok(())
    } else {
        Err(ValidationError::new(
            "拆解策略必须为 by_deadline/by_capacity/by_date_range",
        ))
    }
}

/// 校验周几列表（0=周日, 1-6=周一至周六）
fn validate_weekdays(v: &Vec<u8>) -> Result<(), ValidationError> {
    if v.len() > 7 || v.iter().any(|&d| d > 6) {
        Err(ValidationError::new("weekdays 取值需在 0~6 且最多 7 个"))
    } else {
        Ok(())
    }
}

/// 校验每月几号列表（1-31）
fn validate_month_days(v: &Vec<u8>) -> Result<(), ValidationError> {
    if v.len() > 31 || v.iter().any(|&d| !(1..=31).contains(&d)) {
        Err(ValidationError::new("month_days 取值需在 1~31"))
    } else {
        Ok(())
    }
}

/// 校验导入 JSON 大小（按字节数，上限 50MB）
fn validate_import_size(s: &str) -> Result<(), ValidationError> {
    if s.len() > MAX_IMPORT_JSON_BYTES {
        Err(ValidationError::new("导入数据超过 50MB 上限"))
    } else {
        Ok(())
    }
}

/// 校验导入冲突模式
fn validate_conflict_mode(s: &str) -> Result<(), ValidationError> {
    if ["skip", "overwrite", "rename"].contains(&s) {
        Ok(())
    } else {
        Err(ValidationError::new(
            "冲突模式必须为 skip/overwrite/rename",
        ))
    }
}

/// 任务状态枚举（已废弃，保留用于未来类型安全重构）
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    Partial,
    Done,
    Skipped,
}

impl TaskStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskStatus::Pending => "pending",
            TaskStatus::Partial => "partial",
            TaskStatus::Done => "done",
            TaskStatus::Skipped => "skipped",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(TaskStatus::Pending),
            "partial" => Some(TaskStatus::Partial),
            "done" => Some(TaskStatus::Done),
            "skipped" => Some(TaskStatus::Skipped),
            _ => None,
        }
    }
}

/// 目标（树节点：parent_id=NULL 为总目标，否则为子目标）
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Goal {
    pub id: String,
    pub name: String,
    /// 父目标 ID（NULL=总目标，否则=子目标）
    pub parent_id: Option<String>,
    pub path: String,
    pub deadline: Option<String>,
    pub total_qty: f64,
    pub unit: String,
    pub sort_order: i64,
    pub created_at: String,
    /// P1-3：每日可用时长（按时间预算拆解时使用，None=未设置）
    pub daily_capacity: Option<f64>,
}

/// 创建目标的输入参数
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct CreateGoalInput {
    #[validate(custom(function = "validate_name"))]
    pub name: String,
    /// 父目标 ID（None=总目标，Some=子目标）
    pub parent_id: Option<String>,
    #[validate(custom(function = "validate_date_format"))]
    pub deadline: Option<String>,
    #[validate(custom(function = "validate_qty"))]
    pub total_qty: Option<f64>,
    pub unit: Option<String>,
    /// P1-3：每日可用时长（按时间预算拆解时使用）
    #[serde(default)]
    #[validate(custom(function = "validate_qty"))]
    pub daily_capacity: Option<f64>,
}

/// 更新目标的输入参数
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct UpdateGoalInput {
    pub id: String,
    #[validate(custom(function = "validate_name"))]
    pub name: Option<String>,
    #[validate(custom(function = "validate_date_format"))]
    pub deadline: Option<String>,
    #[validate(custom(function = "validate_qty"))]
    pub total_qty: Option<f64>,
    pub unit: Option<String>,
    /// P1-3：每日可用时长（按时间预算拆解时使用）
    #[serde(default)]
    #[validate(custom(function = "validate_qty"))]
    pub daily_capacity: Option<f64>,
}

/// 重复拆解输入（纯文字类任务：按频率重复 or 单次）
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct RepeatSplitInput {
    pub goal_id: String,
    #[validate(custom(function = "validate_name"))]
    pub name: String,
    /// 起始日期 yyyy-MM-dd
    #[validate(custom(function = "validate_date_format"))]
    pub start_date: String,
    /// 结束日期 yyyy-MM-dd（None 或等于 start_date → 单次任务）
    #[validate(custom(function = "validate_date_format"))]
    pub end_date: Option<String>,
    #[validate(custom(function = "validate_qty"))]
    pub plan_qty: Option<f64>,
    pub unit: Option<String>,
    /// 频率：daily | weekly | monthly（None 或 daily → 每天重复）
    #[serde(default)]
    #[validate(custom(function = "validate_frequency"))]
    pub frequency: Option<String>,
    /// 周几（0=周日, 1-6=周一至周六），仅 weekly 有效
    #[serde(default)]
    #[validate(custom(function = "validate_weekdays"))]
    pub weekdays: Option<Vec<u8>>,
    /// 每月几号（1-31），仅 monthly 有效
    #[serde(default)]
    #[validate(custom(function = "validate_month_days"))]
    pub month_days: Option<Vec<u8>>,
}

/// 智能拆解输入（整合视频拆解与时间预算拆解的统一入口）
///
/// 三种策略：
/// - `by_deadline`：按截止日期均分（总量 ÷ 剩余天数），保留原 auto_split 能力
/// - `by_capacity`：按时间预算（每日可用时长决定任务量），保留原 split_by_capacity 能力
/// - `by_date_range`：自定义日期范围（指定起止日期，可选每日数量）
///
/// 所有参数可选字段均用于临时覆盖目标自身属性，不修改目标本身。
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct SmartSplitInput {
    pub goal_id: String,
    /// 拆解策略：by_deadline | by_capacity | by_date_range
    #[validate(custom(function = "validate_split_strategy"))]
    pub strategy: String,
    /// 总量（可选，默认用 goal.total_qty）
    #[serde(default)]
    #[validate(custom(function = "validate_qty"))]
    pub total_qty: Option<f64>,
    /// 截止日期 yyyy-MM-dd（by_deadline / by_capacity 用，默认用 goal.deadline）
    #[serde(default)]
    #[validate(custom(function = "validate_date_format"))]
    pub deadline: Option<String>,
    /// 每日可用时长（by_capacity 必填）
    #[serde(default)]
    #[validate(custom(function = "validate_qty"))]
    pub daily_capacity: Option<f64>,
    /// 起始日期 yyyy-MM-dd（by_date_range 用，默认明天）
    #[serde(default)]
    #[validate(custom(function = "validate_date_format"))]
    pub start_date: Option<String>,
    /// 结束日期 yyyy-MM-dd（by_date_range 用）
    #[serde(default)]
    #[validate(custom(function = "validate_date_format"))]
    pub end_date: Option<String>,
    /// 每日数量（by_date_range 可选；不填则按天数均分总量）
    #[serde(default)]
    #[validate(custom(function = "validate_qty"))]
    pub per_day_qty: Option<f64>,
}

/// 目标树节点（含子目标和子任务）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalTreeNode {
    pub goal: Goal,
    pub sub_goals: Vec<GoalTreeNode>,
    pub tasks: Vec<Task>,
    pub progress: f64,
    pub is_completed: bool,
}

/// 阶段（已废弃，仅用于旧版数据备份兼容）
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Stage {
    pub id: String,
    pub goal_id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub path: String,
    pub sort_order: i64,
    pub created_at: String,
}

/// 创建阶段的输入参数（已废弃）
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct CreateStageInput {
    pub goal_id: String,
    pub name: String,
    pub parent_id: Option<String>,
}

/// 更新阶段的输入参数（已废弃）
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateStageInput {
    pub id: String,
    pub name: Option<String>,
    pub sort_order: Option<i64>,
}

/// 删除阶段时子任务的处理方式（已废弃）
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct DeleteStageInput {
    pub id: String,
    /// "detach" 子任务转为独立任务；"cascade" 级联删除子任务
    pub mode: String,
}

/// 阶段带进度信息（已废弃）
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct StageWithProgress {
    pub id: String,
    pub goal_id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub path: String,
    pub sort_order: i64,
    pub created_at: String,
    pub total_plan: f64,
    pub total_actual: f64,
    pub percentage: f64,
    pub task_count: i64,
}

/// 任务（三级节点，实际执行单元）
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Task {
    pub id: String,
    pub goal_id: String,
    pub stage_id: Option<String>,
    pub parent_id: Option<String>,
    pub path: String,
    pub name: String,
    pub plan_date: Option<String>,
    /// 精确逾期日期（yyyy-MM-dd），逾期任务首次被检测时写入
    pub overdue_date: Option<String>,
    pub plan_qty: f64,
    pub actual_qty: f64,
    pub unit: String,
    pub status: String,
    pub is_manual: i64,
    pub source: String,
    pub sort_order: i64,
    pub created_at: String,
    /// P1-3：预估时长（小时），按时间预算拆解时自动填充
    pub estimated_hours: Option<f64>,
}

/// 创建任务的输入参数
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct CreateTaskInput {
    pub goal_id: String,
    pub stage_id: Option<String>,
    #[validate(custom(function = "validate_name"))]
    pub name: String,
    #[validate(custom(function = "validate_date_format"))]
    pub plan_date: Option<String>,
    #[validate(custom(function = "validate_qty"))]
    pub plan_qty: Option<f64>,
    pub unit: Option<String>,
}

/// 完成任务的输入参数（支持部分完成）
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct CompleteTaskInput {
    pub task_id: String,
    #[validate(custom(function = "validate_qty"))]
    pub actual_qty: f64,
}

/// 更新任务的输入参数（通用更新，所有字段可选）
///
/// PRD §4.2 模块二 & 分阶段计划 Sprint 2：
/// - 支持修改任务名称、计划日期、计划数量
/// - 修改 plan_qty 时自动标记 is_manual = 1（重新规划时保留）
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct UpdateTaskInput {
    pub task_id: String,
    #[validate(custom(function = "validate_name"))]
    pub name: Option<String>,
    /// 空串表示清除日期（宽松校验放行）
    #[validate(custom(function = "validate_date_optional_clear"))]
    pub plan_date: Option<String>,
    #[validate(custom(function = "validate_qty"))]
    pub plan_qty: Option<f64>,
}

/// 进度信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressInfo {
    pub id: String,
    pub name: String,
    pub total_plan: f64,
    pub total_actual: f64,
    /// 完成百分比 0.0 ~ 1.0
    pub percentage: f64,
    /// 是否完成（子目标全完成 + 直属子任务全完成）
    pub is_completed: bool,
}

/// 今日待办任务（带目标名称）
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TodayTask {
    pub id: String,
    pub goal_id: String,
    pub goal_name: String,
    pub stage_id: Option<String>,
    pub name: String,
    pub plan_date: Option<String>,
    /// 精确逾期日期（yyyy-MM-dd），仅逾期任务有值
    pub overdue_date: Option<String>,
    pub plan_qty: f64,
    pub actual_qty: f64,
    pub unit: String,
    pub status: String,
    pub source: String,
    /// 是否被依赖阻塞（存在未完成的前置依赖），由查询层通过子查询填充
    pub is_blocked: bool,
    /// 阻塞本任务的前置任务名称列表（顿号分隔），仅 is_blocked=true 时有值
    pub blocked_by_names: Option<String>,
}

/// 重新规划预览项：展示某任务变更前后的计划数量
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplanPreviewItem {
    pub task_id: String,
    pub name: String,
    pub plan_date: String,
    pub old_plan_qty: f64,
    pub new_plan_qty: f64,
    /// 是否被保留（手动修改的任务）
    pub retained: bool,
}

/// 重新规划预览结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplanPreview {
    pub goal_id: String,
    pub goal_name: String,
    pub remaining_days: i64,
    pub remaining_qty: f64,
    /// 每日新计划数量（基础值）
    pub daily_base: f64,
    /// 余数（分到前几天）
    pub remainder: i64,
    pub items: Vec<ReplanPreviewItem>,
}

/// 重新规划执行结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplanResult {
    pub goal_id: String,
    pub updated_count: usize,
    pub retained_count: usize,
    pub tasks: Vec<Task>,
}

/// 移动任务（支持跨目标归属调整、阶段移动、同级排序）
///
/// - `goal_id=Some` → 跨目标移动（拖拽归属）：更新 goal_id、parent_id、path，清空 stage_id
/// - `goal_id=None & stage_id=Some` → 阶段内移动：仅更新 stage_id、path
/// - `before_task_id=Some` → 插入到该任务之前（同级排序）；为 None 且跨目标时放置到目标直属任务最前面
#[derive(Debug, Clone, Deserialize)]
pub struct MoveTaskInput {
    pub task_id: String,
    pub goal_id: Option<String>,
    pub stage_id: Option<String>,
    /// 插入到此任务之前（用于同级排序）；None 表示放置到目标直属任务列表最前面
    #[serde(default)]
    pub before_task_id: Option<String>,
}

/// 移动目标（支持跨层级归属调整与同级排序）
///
/// - `new_parent_id`：新父目标 ID；None 表示提升为总目标
/// - `before_goal_id`：插入到此目标之前（用于同级排序）；None 表示追加到末尾
#[derive(Debug, Clone, Deserialize)]
pub struct MoveGoalInput {
    pub goal_id: String,
    /// 新父目标 ID（None=总目标）
    pub new_parent_id: Option<String>,
    /// 插入到此目标之前（同级排序）；None=追加到末尾
    #[serde(default)]
    pub before_goal_id: Option<String>,
}

/// 日历视图任务（带目标名称和逾期标记）
///
/// PRD §4.2 模块五 & 分阶段计划 Sprint 3：
/// 用于日历视图按日期范围查询，附带目标名称与逾期状态
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CalendarTask {
    pub id: String,
    pub goal_id: String,
    pub goal_name: String,
    pub stage_id: Option<String>,
    pub name: String,
    pub plan_date: Option<String>,
    pub plan_qty: f64,
    pub actual_qty: f64,
    pub unit: String,
    pub status: String,
    pub source: String,
    /// 是否逾期（plan_date < today 且未完成），由命令层填充
    pub is_overdue: bool,
    /// 是否被依赖阻塞（存在未完成的前置依赖），由查询层通过子查询填充
    pub is_blocked: bool,
    /// 阻塞本任务的前置任务名称列表（顿号分隔），仅 is_blocked=true 时有值
    pub blocked_by_names: Option<String>,
}

/// 每日完成趋势项
///
/// PRD §4.2 模块六：近7天/30天完成任务数量趋势
/// - 数量型任务按完成数量统计（actual_qty 截断到 plan_qty）
/// - 布尔型任务计 1（status=done）
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DailyTrend {
    /// 日期 yyyy-MM-dd
    pub date: String,
    /// 当日完成任务数（数量型按完成数量计，布尔型计1，已跳过不计）
    pub completed_qty: f64,
    /// 当日完成任务条数（status=done 的任务数）
    pub completed_count: i64,
}

/// 目标完成统计（用于柱状图）
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct GoalCompletionStat {
    pub id: String,
    pub name: String,
    pub total_plan: f64,
    pub total_actual: f64,
    /// 完成百分比 0.0 ~ 1.0
    pub percentage: f64,
    /// 任务总数（不含跳过）
    pub task_count: i64,
    /// 已完成任务数
    pub done_count: i64,
}

/// 鼓励语
///
/// PRD §4.2 模块七 & 分阶段计划 Sprint 4 / Sprint 5：
/// - category: "preset" 预设 / "custom" 用户自定义
/// - level: 鼓励语等级（Sprint 5 个性化规则）
///   - "normal" 普通（1天）
///   - "advanced" 进阶（3天）
///   - "highlight" 高亮（7天）
///   - "celebration" 庆祝（全部完成）
///   - "setback" 挫折安抚（P1-2）
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Encouragement {
    pub id: String,
    pub text: String,
    /// preset | custom
    pub category: String,
    /// normal | advanced | highlight | celebration | setback | longest_streak
    pub level: String,
    pub created_at: String,
    /// P2-1：情境标签（JSON 格式）
    pub context_tags: Option<String>,
    /// P2-5：隐藏标记（仅预设文案可隐藏）
    pub hidden: Option<i32>,
    /// P3-5：自定义排序顺序
    pub sort_order: Option<i64>,
}

/// 添加鼓励语输入
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct AddEncouragementInput {
    #[validate(custom(function = "validate_encouragement_text"))]
    pub text: String,
    /// 可选等级，默认 "normal"
    #[validate(custom(function = "validate_user_level"))]
    pub level: Option<String>,
}

/// P3-2：鼓励语收藏
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct EncouragementFavorite {
    pub id: String,
    pub encouragement_id: String,
    pub favorited_at: String,
}

/// P3-3：鼓励语展示日志（含用户行为）
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct EncouragementShowLog {
    pub id: String,
    pub encouragement_id: String,
    pub shown_at: String,
    pub trigger_source: String,
    /// 关闭时间（可选）
    pub closed_at: Option<String>,
    /// 观看时长（秒）
    pub view_duration: Option<i64>,
}

/// 更新鼓励语输入（P0-5：补齐编辑功能）
///
/// 仅自定义文案可修改；预设文案拒绝修改。
/// text 与 level 均为可选，未传字段保持原值。
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateEncouragementInput {
    pub id: String,
    /// 可选新文本，传入时需满足 2~100 字符
    pub text: Option<String>,
    /// 可选新等级：normal | advanced | highlight | celebration | setback
    pub level: Option<String>,
}

/// 更新鼓励语情境标签输入（P2-1）
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateEncouragementContextInput {
    pub id: String,
    /// 情境标签 JSON 字符串
    pub context_tags: String,
}

/** 设置项（key-value） */
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Setting {
    pub key: String,
    pub value: String,
}

/// 设置项输入
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct SetSettingInput {
    #[validate(length(min = 1, max = 100, message = "key 长度需在 1~100 个字符之间"))]
    pub key: String,
    #[validate(length(max = 10000, message = "value 长度不能超过 10000 个字符"))]
    pub value: String,
}

/// 鼓励语展示触发源（P0-4：展示历史与去重）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EncouragementTriggerSource {
    /// 完成今日首任务（modal）
    CompleteFirst,
    /// 完成非首任务（toast）
    CompleteNormal,
    /// 全部目标完成（celebration modal）
    CompleteCelebration,
    /// 进入仪表盘 banner
    DashboardBanner,
    /// P1-2：连续中断
    StreakBreak,
    /// P1-2：进度滞后
    ProgressLag,
    /// P2-2：新建目标
    GoalCreated,
    /// P2-2：目标进度 50%
    GoalMidway,
    /// P2-2：跳过任务
    TaskSkipped,
    /// P2-2：中断后恢复
    StreakRecovery,
}

impl AsRef<str> for EncouragementTriggerSource {
    fn as_ref(&self) -> &str {
        match self {
            Self::CompleteFirst => "complete_first",
            Self::CompleteNormal => "complete_normal",
            Self::CompleteCelebration => "complete_celebration",
            Self::DashboardBanner => "dashboard_banner",
            Self::StreakBreak => "streak_break",
            Self::ProgressLag => "progress_lag",
            Self::GoalCreated => "goal_created",
            Self::GoalMidway => "goal_midway",
            Self::TaskSkipped => "task_skipped",
            Self::StreakRecovery => "streak_recovery",
        }
    }
}

/// 鼓励语偏好设置（P1-4：用户偏好设置）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncouragementSettings {
    /// 总开关
    pub enabled: bool,
    /// 展示频率：aggressive（每次完成）/ normal（首任务+里程碑）/ sparse（仅里程碑）
    pub frequency: String,
    /// 文案风格：warm（温暖鼓励）/ professional（专业理性）/ minimal（极简克制）
    pub style: String,
    /// 庆祝动画开关（全部目标完成时显示 celebration 动画）
    pub celebration_animation: bool,
    /// emoji 显示开关（文案中是否显示 emoji）
    pub emoji_enabled: bool,
}

/// 更新鼓励语偏好设置输入（P1-4：所有字段可选）
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateEncouragementSettingsInput {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub frequency: Option<String>,
    #[serde(default)]
    pub style: Option<String>,
    #[serde(default)]
    pub celebration_animation: Option<bool>,
    #[serde(default)]
    pub emoji_enabled: Option<bool>,
}

/// 连续完成天数统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreakInfo {
    /// 当前连续天数
    pub current_streak: i64,
    /// 历史最长连续天数
    pub longest_streak: i64,
    /// 今日是否已完成至少一个任务
    pub completed_today: bool,
    /// P2-4：里程碑成就（none/expert/master）
    pub milestone: String,
}

/// 滞后目标详情（P1-2：进度滞后检测）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaggingGoal {
    pub id: String,
    pub name: String,
    pub deadline: String,
    pub predicted_end_date: String,
    /// 距截止日期剩余天数（负数=已逾期）
    pub days_remaining: i32,
}

/// 挫折场景检测结果（P1-2：挫折/安抚场景）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetbackSituation {
    /// 是否发生连续中断
    pub has_streak_break: bool,
    /// 中断前的连续天数（仅 has_streak_break=true 时有效）
    pub streak_break_prev: i32,
    /// 是否存在进度滞后
    pub has_progress_lag: bool,
    /// 滞后目标列表
    pub lagging_goals: Vec<LaggingGoal>,
}

/// 庆祝成就数据（P1-3：celebration 仪式感增强）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CelebrationAchievement {
    /// 总目标数
    pub total_goals: i32,
    /// 总任务数
    pub total_tasks: i32,
    /// 已完成任务数
    pub completed_tasks: i32,
    /// 本次冲刺耗时（天，从首个任务创建到全部完成）
    pub days_elapsed: i32,
    /// 完成时的连续天数
    pub final_streak: i32,
    /// 完成时最长连续天数
    pub final_longest_streak: i32,
}

/// 导出数据（完整备份）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportData {
    pub version: String,
    pub exported_at: String,
    pub goals: Vec<Goal>,
    pub stages: Vec<Stage>,
    pub tasks: Vec<Task>,
    /// P1-1：任务依赖关系（兼容旧备份：缺失时默认为空数组）
    #[serde(default)]
    pub task_dependencies: Vec<TaskDependency>,
    pub encouragements: Vec<Encouragement>,
    pub settings: Vec<Setting>,
}

/// 导入数据输入
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct ImportInput {
    /// JSON 字符串
    #[validate(custom(function = "validate_import_size"))]
    pub data: String,
    /// 冲突处理模式：skip | overwrite | rename
    #[validate(custom(function = "validate_conflict_mode"))]
    pub conflict_mode: String,
}

/// 导入数据结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportResult {
    pub goals_imported: usize,
    pub goals_skipped: usize,
    pub stages_imported: usize,
    pub stages_skipped: usize,
    pub tasks_imported: usize,
    pub tasks_skipped: usize,
    pub dependencies_imported: usize,
    pub dependencies_skipped: usize,
    pub encouragements_imported: usize,
    pub settings_imported: usize,
}

/// 热力图单元格
///
/// PRD §4.2 模块六 & 分阶段计划 Sprint 5：
/// 按日期格子颜色深浅展示每日"完成任务量 / 当日应有任务总量"的比例
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct HeatmapCell {
    /// 日期 yyyy-MM-dd
    pub date: String,
    /// 当日计划任务总量（不含跳过）
    pub plan_qty: f64,
    /// 当日完成量（数量型按 actual_qty 截断到 plan_qty，布尔型 done 计 1）
    pub completed_qty: f64,
    /// 当日任务总数（不含跳过）
    pub task_count: i64,
    /// 当日已完成任务数（status=done）
    pub done_count: i64,
    /// 完成率 0.0 ~ 1.0（completed_qty / plan_qty，plan_qty=0 时为 0）
    pub completion_rate: f64,
}

/// 每日负载（按目标分组）— P2-5 跨目标负载平衡
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyLoad {
    /// 日期 yyyy-MM-dd
    pub date: String,
    /// 当日任务总数（不含跳过）
    pub total_tasks: i64,
    /// 当日任务总量（plan_qty 之和，不含跳过）
    pub total_qty: f64,
    /// 按目标分组的负载明细
    pub by_goal: Vec<GoalLoad>,
}

/// 单目标在某日的负载 — P2-5
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalLoad {
    pub goal_id: String,
    pub goal_name: String,
    pub task_count: i64,
    pub total_qty: f64,
}

/// 完成预测状态
///
/// PRD §4.2 模块六 & 分阶段计划 Sprint 6：
/// - on_track：按期完成（预测日期 <= deadline）
/// - ahead：可提前完成（预测日期 < deadline - 1天）
/// - need_speed：需提高速度（预测日期 > deadline）
/// - no_deadline：未设置截止日期
/// - no_data：无历史完成数据，无法预测
/// - completed：已全部完成
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionPrediction {
    pub goal_id: String,
    pub goal_name: String,
    /// 截止日期（可能为空）
    pub deadline: Option<String>,
    /// 目标总量
    pub total_qty: f64,
    /// 已完成量
    pub completed_qty: f64,
    /// 剩余量
    pub remaining_qty: f64,
    /// 过去7天平均每日完成量
    pub avg_daily_speed: f64,
    /// 预测还需天数（remaining_qty / avg_daily_speed，无数据时为 null）
    pub predicted_days: Option<i64>,
    /// 预测完成日期（today + predicted_days，无数据时为 null）
    pub predicted_date: Option<String>,
    /// 距截止日期剩余天数（负数表示已逾期）
    pub days_to_deadline: Option<i64>,
    /// 预测状态：on_track | ahead | need_speed | no_deadline | no_data | completed
    pub status: String,
    /// 建议文案
    pub suggestion: String,
}

/// 任务依赖关系（P1-1）
///
/// 表示 task_id 依赖 depends_on_id：depends_on_id 完成后 task_id 才可执行。
/// 防循环依赖由 `set_task_dependency` 命令通过 DFS 检测。
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TaskDependency {
    pub id: String,
    pub task_id: String,
    pub depends_on_id: String,
    pub created_at: String,
}

/// 删除任务结果（P2-3：返回被删任务所属 goal_id，供前端局部更新进度）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteTaskResult {
    pub task_id: String,
    pub goal_id: String,
}

/// 批量删除任务结果（P2-3：返回受影响的 goal_id 列表，供前端局部更新进度）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteTasksBatchResult {
    pub deleted_count: i64,
    /// 受影响的 goal_id 列表（去重）
    pub affected_goal_ids: Vec<String>,
}

/// 设置任务依赖输入
#[derive(Debug, Clone, Deserialize)]
pub struct SetTaskDependencyInput {
    pub task_id: String,
    /// 前置任务 ID（task_id 依赖此任务）
    pub depends_on_id: String,
}
