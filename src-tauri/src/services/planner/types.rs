//! 目标树智能规划引擎（v2）——统一契约类型层
//!
//! 定义三类契约的完整类型：
//! 1. 源 A 入口 [`SmartSplitRequest`]：面向首次拆解，六层配置
//!    （intent / constraints / structure / semantic / learning / legacy）；
//! 2. 内核统一入参 [`UnifiedPlanRequest`]：源 A / 源 B（ReplanSolveInput）
//!    经适配层归并后的唯一内核入参；
//! 3. 统一输出 [`SplitPlan`]：方案级元信息 + 可行性报告 + 诊断 + 任务序列 + 汇总。
//!
//! 本模块为纯类型层：不访问数据库、不读时钟；校验失败一律返回
//! `AppError::Param`（对应错误码 E-PARAM），目标存在性检查（E-NOTFOUND）
//! 由数据库装配层（snapshot.rs）负责。
//!
//! 契约来源：docs/目标树智能自动拆解重设计方案.md §2.4~2.5、
//! docs/目标树智能规划与重新规划整合设计方案.md §2.6 / §3.3~3.5。

use chrono::{NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

use crate::error::{AppError, AppResult};
use crate::util::parse_date_or_param;

/// 引擎版本（写入 `split_plans.engine_version` 与 `SplitPlan.engine_version`）
pub const ENGINE_VERSION: &str = "2.0.0";

/// 当前 API 版本（`api_version = 2` 走新引擎）
pub const CURRENT_API_VERSION: u32 = 2;

/// 大纲最大条目数（NFR-K5：输入长度受控）
pub const MAX_OUTLINE_ITEMS: usize = 500;

/// 命名/描述模板最大长度（字符，NFR-K5）
pub const MAX_TEMPLATE_CHARS: usize = 200;

/// 复习间隔最大个数
pub const MAX_REVIEW_INTERVALS: usize = 20;

/// 阶段数下限（`phases.length ≥ 2`）
pub const MIN_PHASES: usize = 2;

/// 阶段数上限（`phases.length ≤ 6`）
pub const MAX_PHASES: usize = 6;

/// 阶段占比求和容差（`Σ ratio ≈ 1.0 ± 1e-3`）
pub const PHASE_RATIO_TOLERANCE: f64 = 1e-3;

// ============================================================
// 枚举（serde 命名与 JSON 规格一一对应，统一 snake_case）
// ============================================================

/// 规划模式：用户参与决策的程度
///
/// - `auto`：全自动，直接产出并应用
/// - `assisted`：返回候选方案供用户选择
/// - `preview`：只算不落库（等价 dry_run）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PlanMode {
    /// 全自动
    #[default]
    Auto,
    /// 返回候选方案供选
    Assisted,
    /// 只算不落库
    Preview,
}

/// 规划目标函数（intent.objective，5 个枚举值）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Objective {
    /// 在截止日前完成（默认）
    #[default]
    FinishByDeadline,
    /// 保持稳定节奏，不追求提前
    SteadyPace,
    /// 记忆留存最大化（触发间隔重复）
    MaximizeRetention,
    /// 最小化单日负担（尽量摊平拉长）
    MinimizeDailyLoad,
    /// 吞吐最大化（尽早完成，前重后轻）
    MaximizeThroughput,
}

/// 规划范围（scope）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    /// 单目标（源 A 恒为该值，除非 allow_sub_goals）
    #[default]
    Single,
    /// 目标子树
    Subtree,
    /// 跨目标协同（goal_ids 必填）
    Coordinate,
}

/// 拆解形状（structure.shape，输入侧；输出侧映射为 `SplitPattern`）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SplitShape {
    /// 自动判定
    #[default]
    Auto,
    /// 均匀分配
    Even,
    /// 前重后轻
    FrontLoaded,
    /// 后重前轻
    BackLoaded,
    /// 阶段化
    Phased,
    /// 间隔重复
    SpacedRepetition,
    /// 里程碑倒推
    Milestone,
}

/// 任务类型（items[].task_type）：描述任务性质，与排期决策依据（reason）正交
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TaskType {
    /// 首次学习
    #[default]
    Learn,
    /// 复习巩固
    Review,
    /// 练习
    Practice,
    /// 里程碑
    Milestone,
}

/// 可行性三态（feasibility.status）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FeasibilityStatus {
    /// 可行
    #[default]
    Feasible,
    /// 可行但有警告
    FeasibleWithWarnings,
    /// 不可行（需返回补救方案）
    Infeasible,
}

/// 实际采用的拆解模式（resolved.pattern，输出侧，6 枚举值）
///
/// 由输入 `SplitShape` 单向映射而来；`milestone` 输入映射为 `Phased` + 里程碑标记。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SplitPattern {
    /// 均匀
    #[default]
    Even,
    /// 容量约束型
    CapacityBound,
    /// 阶段化
    Phased,
    /// 间隔重复
    SpacedRepetition,
    /// 里程碑倒推
    MilestoneBackward,
    /// 混合
    Hybrid,
}

/// 补救方案类型（remedies[].type）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RemedyType {
    /// 提高每日容量
    #[default]
    IncreaseCapacity,
    /// 延长截止日期
    ExtendDeadline,
    /// 缩减范围
    ReduceScope,
    /// 开放周末
    AllowWeekends,
    /// 放宽约束
    RelaxConstraint,
}

// ============================================================
// 源 A 入口：SmartSplitRequest（六层配置）
// ============================================================

/// 智能拆解请求（源 A 入口，`smart_split_v2` 命令入参）
///
/// 设计原则（§2.4.1）：全字段可选（未提供由系统从目标属性/习惯画像推导）、
/// 向后兼容（`legacy` 子对象承载旧 `SmartSplitInput` 全部 7 个字段）、
/// 显式优于隐式（影响结果的参数在输出 diagnostics 中回显）。
///
/// 校验分两层：字段级规则由 `Validate` derive 承担（长度等），
/// 跨字段与格式规则由 [`SmartSplitRequest::validate_request`] 承担；
/// 调用方应先执行 `request.validate()?` 再调用 `validate_request()?`。
#[derive(Debug, Clone, Deserialize, Serialize, Validate)]
pub struct SmartSplitRequest {
    /// 目标 ID（必填；存在性检查由装配层负责，违反时 `AppError::NotFound`）
    #[validate(length(min = 1, message = "goal_id 不能为空"))]
    pub goal_id: String,
    /// API 版本（必填，必须 = 2 走新引擎）
    pub api_version: u32,
    /// 规划模式（默认 auto）
    #[serde(default)]
    pub mode: PlanMode,
    /// 1. 意图层：目标函数、时间视界与总量覆盖
    #[serde(default)]
    pub intent: IntentSpec,
    /// 2. 约束层：硬约束（违反即失败）与软约束（允许打折）
    #[serde(default)]
    pub constraints: ConstraintsLayer,
    /// 3. 结构层：拆解形状、阶段与间隔重复配置
    #[serde(default)]
    pub structure: StructureSpec,
    /// 4. 语义层：内容大纲与命名模板
    #[serde(default)]
    pub semantic: SemanticSpec,
    /// 5. 学习层：习惯画像修正开关与信任度
    #[serde(default)]
    pub learning: LearningSpec,
    /// 6. 兼容层：提供时完全等价于旧 smart_split 语义（可选）
    #[serde(default)]
    pub legacy: Option<LegacySpec>,
    /// 是否生成 rationale / diagnostics（默认 true）
    #[serde(default = "default_true")]
    pub explain: bool,
    /// true 时只算不落库（默认 false）
    #[serde(default)]
    pub dry_run: bool,
}

/// 布尔缺省值 = true 的辅助函数（serde default）
fn default_true() -> bool {
    true
}

/// 意图层（intent）：目标函数、时间视界、缓冲与总量/单位覆盖
#[derive(Debug, Clone, Default, Deserialize, Serialize, Validate)]
pub struct IntentSpec {
    /// 目标函数（默认 finish_by_deadline）
    #[serde(default)]
    pub objective: Objective,
    /// 时间视界（start 默认=明天；end 默认=goal.deadline）
    #[serde(default)]
    pub horizon: Option<HorizonInput>,
    /// 末尾预留缓冲天数（默认 0；约束 0 ≤ v < 规划总天数）
    #[serde(default)]
    pub buffer_days: Option<u32>,
    /// 覆盖 goal.total_qty（默认用目标自身总量）
    #[serde(default)]
    pub total_qty_override: Option<f64>,
    /// 覆盖 goal.unit
    #[serde(default)]
    pub unit_override: Option<String>,
}

/// 时间视界输入（intent.horizon，字符串形态，编译期解析为日期）
#[derive(Debug, Clone, Default, Deserialize, Serialize, Validate)]
pub struct HorizonInput {
    /// 起始日期 yyyy-MM-dd（可选，默认明天）
    #[serde(default)]
    pub start_date: Option<String>,
    /// 结束日期 yyyy-MM-dd（可选，默认 goal.deadline）
    #[serde(default)]
    pub end_date: Option<String>,
}

/// 约束层（constraints）：hard + soft
#[derive(Debug, Clone, Default, Deserialize, Serialize, Validate)]
pub struct ConstraintsLayer {
    /// 硬约束：违反即失败（或触发修复循环）
    #[serde(default)]
    pub hard: HardConstraints,
    /// 软约束：尽量满足，允许打折
    #[serde(default)]
    pub soft: SoftConstraints,
}

/// 硬约束（constraints.hard）
#[derive(Debug, Clone, Default, Deserialize, Serialize, Validate)]
pub struct HardConstraints {
    /// 完全不可用日（yyyy-MM-dd 列表）
    #[serde(default)]
    pub blackout_dates: Vec<String>,
    /// 不可用日区间（含端点）
    #[serde(default)]
    pub blackout_ranges: Vec<BlackoutRange>,
    /// 允许的星期（0=周日；None=全周可用；提供时去重后非空）
    #[serde(default)]
    pub allowed_weekdays: Option<Vec<u8>>,
    /// 单日计划量上限（> 0）
    #[serde(default)]
    pub max_daily_qty: Option<f64>,
    /// 单任务最小粒度（> 0 且 ≤ max_daily_qty）
    #[serde(default)]
    pub min_chunk_qty: Option<f64>,
    /// 单日时长上限（小时）
    #[serde(default)]
    pub max_daily_hours: Option<f64>,
    /// 单日任务条数上限（统一侧对应 max_daily_tasks）
    #[serde(default)]
    pub max_tasks_per_day: Option<u32>,
    /// 跨目标单日总任务数上限（默认 6）
    #[serde(default)]
    pub global_daily_task_budget: Option<u32>,
}

/// 不可用日区间（constraints.hard.blackout_ranges 元素）
#[derive(Debug, Clone, Default, Deserialize, Serialize, Validate)]
pub struct BlackoutRange {
    /// 起始日 yyyy-MM-dd（≤ to）
    pub from: String,
    /// 结束日 yyyy-MM-dd
    pub to: String,
}

/// 软约束（constraints.soft）
#[derive(Debug, Clone, Default, Deserialize, Serialize, Validate)]
pub struct SoftConstraints {
    /// 周末容量系数（默认 1.0）
    #[serde(default)]
    pub weekend_factor: Option<f64>,
    /// 各阶段节奏系数（前轻后重示例：[0.6, 0.9, 1.2]）
    #[serde(default)]
    pub phase_factors: Option<Vec<f64>>,
    /// 尽量跳过的日期（yyyy-MM-dd 列表，软性，不进黑名单）
    #[serde(default)]
    pub prefer_skip_dates: Vec<String>,
    /// 是否按任务类型交错排布
    #[serde(default)]
    pub interleave: Option<bool>,
}

/// 结构层（structure）：拆解形状、阶段切分与间隔重复
#[derive(Debug, Clone, Default, Deserialize, Serialize, Validate)]
pub struct StructureSpec {
    /// 拆解形状（默认 auto）
    #[serde(default)]
    pub shape: Option<SplitShape>,
    /// 阶段配置（shape=phased 或 auto 判定为阶段化时使用）
    #[serde(default)]
    pub phases: Option<Vec<PhaseSpec>>,
    /// 间隔重复配置（默认不启用）
    #[serde(default)]
    pub review: Option<ReviewSpec>,
    /// 是否允许生成子目标结构（FR-18，默认 false）
    #[serde(default)]
    pub allow_sub_goals: Option<bool>,
}

/// 阶段配置（structure.phases 元素）
#[derive(Debug, Clone, Default, Deserialize, Serialize, Validate)]
pub struct PhaseSpec {
    /// 阶段名称（非空白）
    pub name: String,
    /// 阶段量占比（> 0，全部阶段 Σ ratio ≈ 1.0，容差 1e-3）
    pub ratio: f64,
    /// 阶段节奏系数（缺省 1.0）
    #[serde(default)]
    pub pace: Option<f64>,
    /// 阶段单位提示（如"页"/"题"/"套"）
    #[serde(default)]
    pub unit_hint: Option<String>,
}

/// 间隔重复配置（structure.review）
#[derive(Debug, Clone, Default, Deserialize, Serialize, Validate)]
pub struct ReviewSpec {
    /// 是否启用（默认 false）
    #[serde(default)]
    pub enabled: Option<bool>,
    /// 相对首次学习日的复习间隔（天，正整数、严格递增、≤ 20 个）
    #[serde(default)]
    pub intervals: Vec<u32>,
    /// 复习任务量 = 首次任务量 × 该系数（0 ≤ v ≤ 1）
    #[serde(default)]
    pub review_ratio: Option<f64>,
    /// 单日复习任务条数上限
    #[serde(default)]
    pub max_review_per_day: Option<u32>,
}

/// 语义层（semantic）：内容大纲与命名模板
///
/// 模板占位符仅允许白名单 `{goal}{phase}{n}{scope}{cum}{total}{unit}{hours}{date}`；
/// 未识别的占位符**原样保留**，不报错。
#[derive(Debug, Clone, Default, Deserialize, Serialize, Validate)]
pub struct SemanticSpec {
    /// 内容大纲（驱动语义化命名与依赖；元素非空，≤ 500 项）
    #[serde(default)]
    #[validate(length(max = 500, message = "大纲条目不能超过 500 项"))]
    pub outline: Vec<String>,
    /// 任务命名模板
    #[serde(default)]
    #[validate(length(max = 200, message = "命名模板长度不能超过 200 字符"))]
    pub naming_template: Option<String>,
    /// 任务描述模板
    #[serde(default)]
    #[validate(length(max = 200, message = "描述模板长度不能超过 200 字符"))]
    pub description_template: Option<String>,
    /// 复习任务命名模板（整合新增）
    #[serde(default)]
    #[validate(length(max = 200, message = "复习命名模板长度不能超过 200 字符"))]
    pub review_naming_template: Option<String>,
}

/// 学习层（learning）：习惯画像修正
#[derive(Debug, Clone, Default, Deserialize, Serialize, Validate)]
pub struct LearningSpec {
    /// 是否启用习惯画像修正（默认 true）
    #[serde(default)]
    pub use_habit_profile: Option<bool>,
    /// 对习惯修正的整体信任度（0.0~1.0，默认 1.0；编译时与画像门控值相乘）
    #[serde(default)]
    pub adapt_strength: Option<f64>,
    /// 是否为本次拆解开启反馈采集（默认 true）
    #[serde(default)]
    pub record_feedback: Option<bool>,
}

/// 兼容层（legacy）：承载旧 `SmartSplitInput` 除 goal_id 外的全部 7 个字段，
/// 提供时完全等价于旧 smart_split 语义。
#[derive(Debug, Clone, Default, Deserialize, Serialize, Validate)]
pub struct LegacySpec {
    /// 拆解策略：by_deadline | by_capacity | by_date_range
    pub strategy: String,
    /// 总量覆盖（默认用 goal.total_qty）
    #[serde(default)]
    pub total_qty: Option<f64>,
    /// 截止日期 yyyy-MM-dd（by_deadline / by_capacity 用）
    #[serde(default)]
    pub deadline: Option<String>,
    /// 每日可用时长（by_capacity 必填）
    #[serde(default)]
    pub daily_capacity: Option<f64>,
    /// 起始日期 yyyy-MM-dd（by_date_range 必填）
    #[serde(default)]
    pub start_date: Option<String>,
    /// 结束日期 yyyy-MM-dd（by_date_range 必填）
    #[serde(default)]
    pub end_date: Option<String>,
    /// 每日数量（by_date_range 可选；不填则按天数均分总量）
    #[serde(default)]
    pub per_day_qty: Option<f64>,
}

impl SmartSplitRequest {
    /// 手写跨字段校验：覆盖规格 §2.4.3 输入校验规则表的全部条目
    ///
    /// 规则与违反行为：
    /// - `api_version` 必须 = 2 → `AppError::Param("不支持的 api_version")`
    /// - `horizon.*` / `blackout_*` 必须 yyyy-MM-dd 且 from ≤ to → `AppError::Param`
    /// - `buffer_days` 满足 0 ≤ v < 规划总天数（起止均提供时精确校验）→ `AppError::Param`
    /// - `allowed_weekdays` 元素 ∈ 0..=6 且非空 → `AppError::Param`
    /// - `max_daily_qty` / `min_chunk_qty` > 0 且 min ≤ max → `AppError::Param`
    /// - `phases[]` 2~6 个、ratio 全 > 0、Σ ratio ≈ 1.0（容差 1e-3）→ `AppError::Param`
    /// - `review.intervals` 正整数、严格递增、≤ 20 个；`review_ratio` ∈ [0,1] → `AppError::Param`
    /// - `outline` 元素非空、≤ 500 项；模板 ≤ 200 字符 → `AppError::Param`
    /// - 模板未识别占位符原样保留（非错误，无需校验）
    /// - `learning.adapt_strength` ∈ [0,1] → `AppError::Param`
    /// - `legacy` 等价复刻旧 `SmartSplitInput` 校验语义 → `AppError::Param`
    ///
    /// `goal_id` 存在性（E-NOTFOUND）与模板占位符白名单不在本层职责内。
    pub fn validate_request(&self) -> AppResult<()> {
        // ---------- 顶层 ----------
        if self.goal_id.trim().is_empty() {
            return Err(AppError::Param("goal_id 不能为空".into()));
        }
        if self.api_version != CURRENT_API_VERSION {
            return Err(AppError::Param("不支持的 api_version".into()));
        }

        // ---------- 意图层 ----------
        let mut start: Option<NaiveDate> = None;
        let mut end: Option<NaiveDate> = None;
        if let Some(horizon) = &self.intent.horizon {
            if let Some(s) = &horizon.start_date {
                start = Some(parse_date_or_param(s, "时间范围起始")?);
            }
            if let Some(e) = &horizon.end_date {
                end = Some(parse_date_or_param(e, "时间范围结束")?);
            }
            if let (Some(s), Some(e)) = (start, end) {
                if e < s {
                    return Err(AppError::Param(
                        "时间范围结束日期不能早于开始日期".into(),
                    ));
                }
            }
        }
        if let Some(buffer) = self.intent.buffer_days {
            // 起止均提供时可精确校验 0 ≤ v < 总天数；否则 u32 天然满足非负
            if let (Some(s), Some(e)) = (start, end) {
                let span = (e - s).num_days() + 1;
                if buffer as i64 >= span {
                    return Err(AppError::Param(format!(
                        "buffer_days({})必须小于规划总天数({})",
                        buffer, span
                    )));
                }
            }
        }
        if let Some(qty) = self.intent.total_qty_override {
            if !qty.is_finite() || qty <= 0.0 {
                return Err(AppError::Param("total_qty_override 必须为正数".into()));
            }
        }
        if let Some(unit) = &self.intent.unit_override {
            if unit.trim().is_empty() {
                return Err(AppError::Param("unit_override 不能为空白".into()));
            }
        }

        // ---------- 约束层 ----------
        let hard = &self.constraints.hard;
        for d in &hard.blackout_dates {
            parse_date_or_param(d, "不可用日")?;
        }
        for range in &hard.blackout_ranges {
            let from = parse_date_or_param(&range.from, "不可用日区间起始")?;
            let to = parse_date_or_param(&range.to, "不可用日区间结束")?;
            if from > to {
                return Err(AppError::Param(
                    "blackout_ranges 的 from 不能晚于 to".into(),
                ));
            }
        }
        if let Some(weekdays) = &hard.allowed_weekdays {
            if weekdays.is_empty() {
                return Err(AppError::Param("allowed_weekdays 去重后不能为空".into()));
            }
            if weekdays.iter().any(|&w| w > 6) {
                return Err(AppError::Param(
                    "allowed_weekdays 取值必须在 0~6（0=周日）".into(),
                ));
            }
        }
        if let Some(max_qty) = hard.max_daily_qty {
            if !max_qty.is_finite() || max_qty <= 0.0 {
                return Err(AppError::Param("max_daily_qty 必须大于 0".into()));
            }
        }
        if let Some(min_chunk) = hard.min_chunk_qty {
            if !min_chunk.is_finite() || min_chunk <= 0.0 {
                return Err(AppError::Param("min_chunk_qty 必须大于 0".into()));
            }
        }
        if let (Some(max_qty), Some(min_chunk)) = (hard.max_daily_qty, hard.min_chunk_qty) {
            if min_chunk > max_qty {
                return Err(AppError::Param("min_chunk_qty 不能大于 max_daily_qty".into()));
            }
        }
        if let Some(max_hours) = hard.max_daily_hours {
            if !max_hours.is_finite() || max_hours <= 0.0 {
                return Err(AppError::Param("max_daily_hours 必须大于 0".into()));
            }
        }
        if let Some(max_tasks) = hard.max_tasks_per_day {
            if max_tasks == 0 {
                return Err(AppError::Param("max_tasks_per_day 必须至少为 1".into()));
            }
        }
        if let Some(budget) = hard.global_daily_task_budget {
            if budget == 0 {
                return Err(AppError::Param(
                    "global_daily_task_budget 必须至少为 1".into(),
                ));
            }
        }
        let soft = &self.constraints.soft;
        if let Some(factor) = soft.weekend_factor {
            if !factor.is_finite() || factor < 0.0 {
                return Err(AppError::Param(
                    "weekend_factor 必须为非负有限数".into(),
                ));
            }
        }
        for d in &soft.prefer_skip_dates {
            parse_date_or_param(d, "期望跳过日")?;
        }

        // ---------- 结构层 ----------
        if let Some(phases) = &self.structure.phases {
            if phases.len() < MIN_PHASES || phases.len() > MAX_PHASES {
                return Err(AppError::Param(format!(
                    "阶段数必须在 {}~{} 之间",
                    MIN_PHASES, MAX_PHASES
                )));
            }
            let mut ratio_sum = 0.0;
            for phase in phases {
                if phase.name.trim().is_empty() {
                    return Err(AppError::Param("阶段 name 不能为空白".into()));
                }
                if !phase.ratio.is_finite() || phase.ratio <= 0.0 {
                    return Err(AppError::Param("phases[].ratio 必须全部大于 0".into()));
                }
                ratio_sum += phase.ratio;
            }
            if (ratio_sum - 1.0).abs() > PHASE_RATIO_TOLERANCE {
                return Err(AppError::Param(format!(
                    "phases[].ratio 之和必须约为 1.0（当前 {}）",
                    ratio_sum
                )));
            }
        }
        if let Some(review) = &self.structure.review {
            if review.intervals.len() > MAX_REVIEW_INTERVALS {
                return Err(AppError::Param(format!(
                    "review.intervals 不能超过 {} 个",
                    MAX_REVIEW_INTERVALS
                )));
            }
            if review.intervals.iter().any(|&i| i == 0) {
                return Err(AppError::Param("review.intervals 必须为正整数".into()));
            }
            for w in review.intervals.windows(2) {
                if w[0] >= w[1] {
                    return Err(AppError::Param("review.intervals 必须严格递增".into()));
                }
            }
            if review.enabled == Some(true) && review.intervals.is_empty() {
                return Err(AppError::Param(
                    "启用间隔重复时 review.intervals 不能为空".into(),
                ));
            }
            if let Some(ratio) = review.review_ratio {
                if !ratio.is_finite() || !(0.0..=1.0).contains(&ratio) {
                    return Err(AppError::Param(
                        "review.review_ratio 必须在 0~1 之间".into(),
                    ));
                }
            }
        }

        // ---------- 语义层 ----------
        if self.semantic.outline.len() > MAX_OUTLINE_ITEMS {
            return Err(AppError::Param(format!(
                "大纲不能超过 {} 项",
                MAX_OUTLINE_ITEMS
            )));
        }
        if self.semantic.outline.iter().any(|s| s.trim().is_empty()) {
            return Err(AppError::Param("outline 元素不能为空白".into()));
        }
        for (label, template) in [
            ("naming_template", &self.semantic.naming_template),
            ("description_template", &self.semantic.description_template),
            (
                "review_naming_template",
                &self.semantic.review_naming_template,
            ),
        ] {
            if let Some(t) = template {
                if t.chars().count() > MAX_TEMPLATE_CHARS {
                    return Err(AppError::Param(format!(
                        "{} 长度不能超过 {} 字符",
                        label, MAX_TEMPLATE_CHARS
                    )));
                }
            }
        }

        // ---------- 学习层 ----------
        if let Some(strength) = self.learning.adapt_strength {
            if !strength.is_finite() || !(0.0..=1.0).contains(&strength) {
                return Err(AppError::Param(
                    "adapt_strength 必须在 0.0~1.0 之间".into(),
                ));
            }
        }

        // ---------- 兼容层（等价复刻旧 SmartSplitInput 校验语义） ----------
        if let Some(legacy) = &self.legacy {
            if !["by_deadline", "by_capacity", "by_date_range"]
                .contains(&legacy.strategy.as_str())
            {
                return Err(AppError::Param(
                    "拆解策略必须为 by_deadline/by_capacity/by_date_range".into(),
                ));
            }
            for (label, date) in [
                ("截止", &legacy.deadline),
                ("起始", &legacy.start_date),
                ("结束", &legacy.end_date),
            ] {
                if let Some(d) = date {
                    parse_date_or_param(d, label)?;
                }
            }
            for (label, qty) in [
                ("total_qty", legacy.total_qty),
                ("daily_capacity", legacy.daily_capacity),
                ("per_day_qty", legacy.per_day_qty),
            ] {
                if let Some(v) = qty {
                    if !v.is_finite() || v < 0.0 {
                        return Err(AppError::Param(format!(
                            "legacy.{} 必须为非负有限数",
                            label
                        )));
                    }
                }
            }
            if legacy.strategy == "by_date_range"
                && (legacy.start_date.is_none() || legacy.end_date.is_none())
            {
                return Err(AppError::Param(
                    "自定义日期范围策略必须指定起始与结束日期".into(),
                ));
            }
        }

        Ok(())
    }
}

// ============================================================
// 内核统一入参：UnifiedPlanRequest（源 A / 源 B 归并后）
// ============================================================

/// 统一内部入参（内核唯一入参，由适配层从双契约归并而来）
///
/// 字段树与字段级适配规则见整合设计方案 §2.6.1：源 A 的六层配置与源 B 的
/// scope / overrides / plan_hash / dry_run 语义均可映射到本结构；
/// 编译期回落链与冲突优先级由 `constraints::compile_constraints` 执行。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UnifiedPlanRequest {
    /// 规划根目标（A.goal_id | B.goal_id）
    pub goal_id: String,
    /// 规划范围（A 恒为 Single；B 可为 subtree/coordinate）
    pub scope: Scope,
    /// 跨目标协同时的目标列表（scope=coordinate 时必填、长度 ≥ 2 且互异）
    pub goal_ids: Option<Vec<String>>,
    /// API 版本（A 侧传入；B 侧默认 2）
    pub api_version: u32,
    /// 规划模式（B 的 dry_run=true 由适配层映射为 Preview）
    pub mode: PlanMode,
    /// 时间视界（A.intent.horizon | B.overrides.deadline；回落 goal.deadline → 滚动窗口）
    pub horizon: Option<HorizonInput>,
    /// 目标函数（B 无，默认 FinishByDeadline）
    pub objective: Objective,
    /// 末尾缓冲天数（A.intent.buffer_days；B 无，默认 0）
    pub buffer_days: Option<u32>,
    /// 总量覆盖（A.intent.total_qty_override；B 无，回落 goal.total_qty / W）
    pub total_qty_override: Option<f64>,
    /// 单位覆盖（A.intent.unit_override；B 无，回落 goal.unit）
    pub unit_override: Option<String>,
    /// 统一约束规格（A.constraints.hard/soft | B.overrides | goal_planning_policy 归并）
    pub constraints: ConstraintSpec,
    /// 结构层（A.structure；B 无，由规则引擎推断）
    pub structure: StructureSpec,
    /// 语义层（A.semantic；B 无，回落内置默认模板）
    pub semantic: SemanticSpec,
    /// 学习层（A.learning；B 无）
    pub learning: LearningSpec,
    /// 重排专用：任务排除、期望摘要与试算开关（B.overrides / B 顶层）
    pub planning: PlanningSpec,
    /// 兼容层（A.legacy；B 无）
    pub legacy: Option<LegacySpec>,
}

/// 统一约束规格（UnifiedPlanRequest.constraints）
///
/// 由适配层把 A 的 hard/soft 约束与 B 的 overrides
/// （daily_capacity / intensity / respect_locks）及目标级策略归并为单一结构；
/// 编译期由 `compile_constraints` 按回落链与冲突优先级定稿。
#[derive(Debug, Clone, Default, Deserialize, Serialize, Validate)]
pub struct ConstraintSpec {
    /// 每日容量（B.overrides 优先；回落 goal.daily_capacity → 画像）
    #[serde(default)]
    pub daily_capacity: Option<f64>,
    /// 强度系数（B.overrides.intensity，∈ [0.5, 1.5]）
    #[serde(default)]
    pub intensity: Option<f64>,
    /// 是否尊重 schedule_locked 锁定任务（B.overrides.respect_locks）
    #[serde(default)]
    pub respect_locks: Option<bool>,
    /// 单日计划量上限（A 与策略取更严 min）
    #[serde(default)]
    pub max_daily_qty: Option<f64>,
    /// 单日时长上限（小时，取更严 min）
    #[serde(default)]
    pub max_daily_hours: Option<f64>,
    /// 单日任务条数上限（取更严 min）
    #[serde(default)]
    pub max_daily_tasks: Option<f64>,
    /// 单任务最小粒度（取更大 max）
    #[serde(default)]
    pub min_chunk_qty: Option<f64>,
    /// 跨目标单日任务预算（默认 6）
    #[serde(default)]
    pub global_daily_task_budget: Option<u32>,
    /// 不可用日（请求侧，与 day_availability 表做并集）
    #[serde(default)]
    pub blackout_dates: Vec<String>,
    /// 不可用日区间（含端点）
    #[serde(default)]
    pub blackout_ranges: Vec<BlackoutRange>,
    /// 允许的星期（0=周日；与策略休息日补集做交集；None=全周）
    #[serde(default)]
    pub allowed_weekdays: Option<Vec<u8>>,
    /// 周末容量系数（A 请求 > 画像 > 默认 1.0）
    #[serde(default)]
    pub weekend_factor: Option<f64>,
    /// 各阶段节奏系数
    #[serde(default)]
    pub phase_factors: Option<Vec<f64>>,
    /// 尽量跳过的日期
    #[serde(default)]
    pub prefer_skip_dates: Vec<String>,
    /// 是否按任务类型交错排布
    #[serde(default)]
    pub interleave: Option<bool>,
}

/// 重排专用参数（UnifiedPlanRequest.planning，源 B 侧）
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct PlanningSpec {
    /// 本次规划需排除的任务 ID（B.overrides.excluded_task_ids）
    #[serde(default)]
    pub excluded_task_ids: Vec<String>,
    /// 期望的方案内容摘要（格式 sha256:<64 hex>；None 表示不校验）
    #[serde(default)]
    pub expected_plan_hash: Option<String>,
    /// 试算开关（true 时不落库；与 mode=preview 任一为真即生效）
    #[serde(default)]
    pub dry_run: bool,
}

// ============================================================
// 平滑与迟滞参数
// ============================================================

/// 平滑与迟滞策略（SmoothingPolicy，防抖动 SC-10）
///
/// 默认值来自整合设计方案 §4.7.5 迟滞规则表。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SmoothingPolicy {
    /// 数量变化迟滞阈值（相对变化；变化小于 15% 不调整该任务）
    pub qty_change_threshold: f64,
    /// 日期迁移迟滞阈值（天；只挪动 1 天且非逾期则不挪）
    pub date_move_threshold_days: i64,
    /// 同一目标两次自动重排的最小间隔（小时）
    pub cooldown_hours: u32,
    /// 单次重排最多迁移的任务比例（0~1，超出按迁移代价升序截断）
    pub max_moves_per_run: f64,
    /// 触发全量重排的总量变化比例（超过则忽略迟滞；整合新增）
    pub proportional_change_threshold: f64,
}

impl Default for SmoothingPolicy {
    fn default() -> Self {
        Self {
            qty_change_threshold: 0.15,
            date_move_threshold_days: 1,
            cooldown_hours: 6,
            max_moves_per_run: 0.30,
            proportional_change_threshold: 0.30,
        }
    }
}

// ============================================================
// 统一输出：SplitPlan 及嵌套结构
// ============================================================

/// 智能拆解方案（统一输出契约，`smart_split_v2` / `replan_solve` 共用）
///
/// 不变式（INV-1~INV-8，§2.5.3）由求解器在每次返回前校验；
/// `infeasible` 时本结构仍完整返回并携带 `feasibility.remedies`（≥ 3 个），
/// 而非仅抛错（错误码 E-INFEASIBLE 的降级行为）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplitPlan {
    /// 本次拆解方案 UUID（FR-15；与 split_plans.id 同值）
    pub plan_id: Uuid,
    pub goal_id: String,
    /// API 版本
    pub api_version: u32,
    /// 生成时间（无时区本地时间，格式 yyyy-MM-ddTHH:mm:ss）
    pub generated_at: NaiveDateTime,
    /// 引擎版本（如 "2.0.0"）
    pub engine_version: String,
    /// 方案内容摘要（格式 sha256:<64 hex>；提交一致性校验用，源 B 口径）
    #[serde(default)]
    pub plan_hash: Option<String>,
    /// 方案级元信息（实际采用的口径）
    pub resolved: ResolvedPlanMeta,
    /// 可行性报告（FR-07）
    pub feasibility: FeasibilityReport,
    /// 诊断信息（FR-19）
    pub diagnostics: PlanDiagnostics,
    /// 任务序列
    pub items: Vec<PlanItem>,
    /// 汇总与守恒校验
    pub summary: PlanSummary,
}

/// 方案级元信息（resolved）：实际采用的拆解模式与口径
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedPlanMeta {
    /// 实际采用的拆解模式（输入 shape → 输出 pattern 单向映射）
    pub pattern: SplitPattern,
    /// 实际采用的目标函数
    pub objective: Objective,
    /// 解析后的时间视界
    pub horizon: ResolvedHorizon,
    /// 实际拆解总量
    pub total_qty: f64,
    /// 实际单位
    pub unit: String,
}

/// 解析后的时间视界（resolved.horizon）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedHorizon {
    /// 起始日
    pub start: NaiveDate,
    /// 结束日（含末尾缓冲）
    pub end: NaiveDate,
    /// 末尾预留缓冲天数
    pub buffer_days: u32,
}

/// 可行性报告（FR-07）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeasibilityReport {
    /// 可行性三态
    pub status: FeasibilityStatus,
    /// 扣黑名单/非允许星期/缓冲后的可用天数（available 且非 is_rest 的日数）
    pub effective_days: u32,
    /// 有效容量合计 Σ C_eff(d)
    pub effective_capacity_total: f64,
    /// 剩余工作量 W（= Σ max(0, plan_qty − actual_qty)，active 排除 done/skipped）
    pub required_capacity: f64,
    /// 松弛度 (capacity − required) / required
    pub slack_ratio: f64,
    /// 单日峰值负载
    pub peak_day_load: f64,
    /// 单日平均负载
    pub avg_day_load: f64,
    /// 补救方案（infeasible 时非空且 ≥ 3 个，按 cost 升序）
    #[serde(default)]
    pub remedies: Vec<Remedy>,
}

/// 补救方案（infeasible 时的处置建议）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Remedy {
    /// 方案类型（JSON 键为 type）
    #[serde(rename = "type")]
    pub remedy_type: RemedyType,
    /// 用户可读的方案标签
    pub label: String,
    /// 应用该方案所需的参数变化（任意键值对）
    pub delta: serde_json::Value,
    /// 应用后的松弛度
    pub resulting_slack_ratio: f64,
    /// 与原意图的偏离成本（0~1，越小越推荐）
    pub cost: f64,
    /// 取舍说明
    pub tradeoff: String,
}

/// 诊断信息（FR-19）：规则命中、画像生效与警告
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlanDiagnostics {
    /// 命中的规则链（R-xx）
    #[serde(default)]
    pub applied_rules: Vec<AppliedRule>,
    /// 生效的习惯画像（未启用时为 None）
    #[serde(default)]
    pub habit_profile: Option<HabitProfile>,
    /// 计划警告（W-xx）
    #[serde(default)]
    pub warnings: Vec<PlanWarning>,
}

/// 命中规则（diagnostics.applied_rules 元素）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppliedRule {
    /// 规则编号（如 R-03）
    pub id: String,
    /// 规则维度（capacity / structure / user …）
    pub dimension: String,
    /// 规则说明
    pub note: String,
}

/// 生效的习惯画像快照（diagnostics.habit_profile）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HabitProfile {
    /// 画像版本号
    pub version: u32,
    /// 反馈样本数
    pub samples: u32,
    /// 实际生效的信任度（门控 × 用户设定）
    pub adapt_strength: f64,
    /// 速度系数
    pub speed_factor: f64,
    /// 各星期容量（0=周日，长度 7）
    pub weekday_capacity: Vec<f64>,
    /// 偏好单任务粒度
    pub preferred_chunk: f64,
}

/// 计划警告（diagnostics.warnings 元素）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanWarning {
    /// 警告码（W-LOAD / W-MERGED / W-REVIEW-DROPPED …，§2.6）
    pub code: String,
    /// 关联日期 yyyy-MM-dd（无具体日期时为 None）
    #[serde(default)]
    pub date: Option<String>,
    /// 警告文案
    pub message: String,
}

/// 任务序列项（items[] 元素）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanItem {
    /// 批次内序号（从 1 起）
    pub seq: u32,
    /// 任务名称（语义化命名）
    pub name: String,
    /// 任务描述（模板渲染结果；未设置模板时为 None）
    #[serde(default)]
    pub description: Option<String>,
    /// 计划日期
    pub plan_date: NaiveDate,
    /// 计划数量
    pub plan_qty: f64,
    /// 单位
    pub unit: String,
    /// 预估时长（小时）
    #[serde(default)]
    pub estimated_hours: Option<f64>,
    /// 任务性质（learn / review / practice / milestone）
    pub task_type: TaskType,
    /// 所属阶段名（非阶段化时为 None）
    #[serde(default)]
    pub phase: Option<String>,
    /// 阶段提示（供落库时创建 stage）
    #[serde(default)]
    pub stage_hint: Option<String>,
    /// 同批次内前置任务序号（FR-12；无环且 dep < seq）
    #[serde(default)]
    pub depends_on_seq: Vec<u32>,
    /// 分配理由链（explain=false 时为 None）
    #[serde(default)]
    pub rationale: Option<String>,
    /// 方案对该任务估算的置信度（0~1）
    #[serde(default)]
    pub confidence: Option<f64>,
}

/// 方案汇总与守恒校验（summary）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanSummary {
    /// 任务总数
    pub task_count: u32,
    /// 学习任务数
    pub learn_task_count: u32,
    /// 复习任务数
    pub review_task_count: u32,
    /// 练习任务数
    pub practice_task_count: u32,
    /// 学习类任务 plan_qty 之和（不变式：必须 == resolved.total_qty）
    pub total_allocated_qty: f64,
    /// 复习量之和（≈ Σ 学习量 × review_ratio，不参与守恒）
    pub review_qty_total: f64,
    /// 守恒校验结果（Σ plan_qty（排除 review）== total_qty）
    pub conservation_ok: bool,
    /// 首末任务计划日期（无任务时为 None）
    #[serde(default)]
    pub date_span: Option<(NaiveDate, NaiveDate)>,
    /// 汇总解释文案
    #[serde(default)]
    pub explanation: Option<String>,
}
