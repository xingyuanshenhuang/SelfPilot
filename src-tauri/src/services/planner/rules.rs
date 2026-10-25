//! 规则引擎（R-01~R-15）与六维特征提取（对应《整合设计方案》§4.4）
//!
//! 输入为 [`FeatureVector`]（由目标属性、统一入参、编译约束与习惯画像派生），
//! 输出为 [`PatternPlan`]：拆解模式 + 命中规则（`diagnostics.applied_rules`）
//! + 可执行的约束修正（容量基线 / 最小粒度 / 周末系数 / 当日负载权重 /
//! 复习配置 / 依赖链开关）。
//!
//! 规则按固定优先级降序执行（同优先级按编号升序），执行顺序确定，因此同一
//! 特征向量必然产出同一方案（NFR-C2 / AC-06）。规则只做「标注 + 参数修正」，
//! 不直接生成任务；任务排布仍由 A3/A4 求解器完成。

use std::collections::BTreeMap;

use chrono::{Datelike, NaiveDate};

use crate::db::models::Goal;
use crate::services::planner::constraints::CompiledSpec;
use crate::services::planner::snapshot::PlanSnapshot;
use crate::services::planner::types::{
    AppliedRule, LearningSpec, ReviewSpec, SemanticSpec, SplitPattern, SplitShape, StructureSpec,
    UnifiedPlanRequest, MIN_PHASES,
};
use crate::services::replan::learning::{PlanningProfile, WARMUP_SAMPLES};

/// 短视界阈值（R-01：视界 < 7 天视为短期冲刺）
pub const SHORT_HORIZON_DAYS: i64 = 7;
/// R-04 完成率偏离阈值（|σ−1| > 0.15）
pub const SPEED_FACTOR_THRESHOLD: f64 = 0.15;
/// R-05 既有负载占日预算比例阈值
pub const LOAD_THRESHOLD_RATIO: f64 = 0.6;
/// R-05 负载日权重系数
pub const LOAD_WEIGHT_FACTOR: f64 = 0.5;
/// R-08 粒度偏离阈值（相对默认粒度）
pub const PREFERRED_CHUNK_DEVIATION: f64 = 0.3;
/// R-09 模式偏好最小采纳次数
pub const AFFINITY_MIN_COUNT: i64 = 3;
/// R-09 模式偏好最小占比
pub const AFFINITY_MIN_SHARE: f64 = 0.6;
/// R-12 周末产能显著偏低阈值（周末/工作日 < 0.85）
pub const WEEKEND_LOW_RATIO: f64 = 0.85;
/// 默认单次专注时长（小时；max_attention_span 特征缺省值）
pub const DEFAULT_ATTENTION_SPAN_HOURS: f64 = 2.0;
/// R-07 自动注入的默认复习间隔（天）
pub const DEFAULT_REVIEW_INTERVALS: [u32; 5] = [1, 2, 4, 7, 15];

/// 规则元信息（编号 / 维度 / 优先级）
struct RuleDef {
    id: &'static str,
    dimension: &'static str,
    priority: u32,
}

/// 规则表 R-01~R-15（优先级降序执行；编号升序打破同级）
const RULE_TABLE: [RuleDef; 15] = [
    RuleDef {
        id: "R-01",
        dimension: "time",
        priority: 100,
    },
    RuleDef {
        id: "R-02",
        dimension: "capacity",
        priority: 100,
    },
    RuleDef {
        id: "R-11",
        dimension: "capacity",
        priority: 100,
    },
    RuleDef {
        id: "R-13",
        dimension: "safety",
        priority: 100,
    },
    RuleDef {
        id: "R-03",
        dimension: "capacity",
        priority: 90,
    },
    RuleDef {
        id: "R-05",
        dimension: "context",
        priority: 85,
    },
    RuleDef {
        id: "R-04",
        dimension: "capacity",
        priority: 80,
    },
    RuleDef {
        id: "R-07",
        dimension: "cognition",
        priority: 75,
    },
    RuleDef {
        id: "R-06",
        dimension: "structure",
        priority: 70,
    },
    RuleDef {
        id: "R-10",
        dimension: "structure",
        priority: 65,
    },
    RuleDef {
        id: "R-08",
        dimension: "user",
        priority: 60,
    },
    RuleDef {
        id: "R-12",
        dimension: "user",
        priority: 55,
    },
    RuleDef {
        id: "R-09",
        dimension: "user",
        priority: 50,
    },
    RuleDef {
        id: "R-14",
        dimension: "structure",
        priority: 45,
    },
    RuleDef {
        id: "R-15",
        dimension: "intent",
        priority: 40,
    },
];

/// 规则元信息查询（编号来自规则表常量，必然存在）
fn rule_meta(id: &str) -> &'static RuleDef {
    RULE_TABLE
        .iter()
        .find(|r| r.id == id)
        .expect("规则编号必须存在于 RULE_TABLE")
}

/// 记录一条规则命中
fn hit(matched: &mut Vec<AppliedRule>, id: &str, note: String) {
    let meta = rule_meta(id);
    matched.push(AppliedRule {
        id: id.to_string(),
        dimension: meta.dimension.to_string(),
        note,
    });
}

/// 认知型任务类型（§4.4.2 特征 17，与任务输出类型 TaskType 正交）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CognitiveType {
    /// 记忆型（背诵/词汇/公式/条文）
    Memory,
    /// 练习型（刷题/模考/真题）
    Practice,
    /// 项目型（开发/实战/作品）
    Project,
    /// 学习型（阅读/课程/默认）
    Learn,
}

/// 数量粒度（§4.4.2 特征 11）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QtyGranularity {
    /// 整数分配（个/词/道/页…）
    Integer,
    /// 小数分配（小时/份…）
    Fractional,
}

/// 用户画像特征（R-04/R-08/R-09/R-12 消费）
#[derive(Debug, Clone)]
pub struct ProfileFeatures {
    /// 完成率 σ（实际完成量 / 计划量）
    pub speed_factor: f64,
    /// 各星期相对产能（0=周日 … 6=周六）
    pub weekday_capacity: [f64; 7],
    /// 偏好单任务粒度 κ
    pub preferred_chunk: f64,
    /// 有效修正强度（门控值 × 用户信任度，0~1）
    pub adapt_strength: f64,
    /// 画像门控值 min(samples/WARMUP, 1)
    pub gate: f64,
}

impl ProfileFeatures {
    /// 从持久化画像派生特征；冷启动（samples ≤ 0）返回 None
    pub fn from_profile(profile: &PlanningProfile, adapt_strength: f64) -> Option<Self> {
        if profile.samples <= 0 {
            return None;
        }
        let gate = (profile.samples as f64 / WARMUP_SAMPLES).min(1.0);
        Some(Self {
            speed_factor: profile.speed_factor,
            weekday_capacity: profile.weekday_capacity,
            preferred_chunk: profile.preferred_chunk,
            adapt_strength: gate * adapt_strength.clamp(0.0, 1.0),
            gate,
        })
    }
}

/// 六维特征向量（§4.4.2 全表 + 硬约束辅助特征）
#[derive(Debug, Clone)]
pub struct FeatureVector {
    // ---- ① 时间维 ----
    /// 视界总天数（含缓冲；源文档「剩余天数」口径）
    pub horizon_span: i64,
    /// 有效可用天数（available && weight > 0）
    pub available_days: i64,
    /// 其中工作日天数
    pub workday_days: i64,
    /// 其中周末天数
    pub weekend_days: i64,
    /// 末尾缓冲天数
    pub buffer_days: i64,

    // ---- ② 容量维 ----
    /// 声明每日容量（goal / constraints / 画像回落后的定稿值）
    pub declared_capacity: Option<f64>,
    /// 单日计划量上限
    pub max_daily_qty: Option<f64>,
    /// 单日任务条数上限
    pub max_daily_tasks: Option<f64>,
    /// 单任务最小粒度
    pub min_chunk_qty: Option<f64>,
    /// 拆解总量
    pub total_qty: f64,
    /// 单位
    pub unit: String,
    /// 完成所需日均量（total / available_days）
    pub required_daily_avg: f64,
    /// 数量粒度
    pub qty_granularity: QtyGranularity,

    // ---- ③ 结构维 ----
    /// 是否提供大纲
    pub has_outline: bool,
    /// 大纲条目数
    pub outline_len: usize,
    /// 大纲是否含章节标记（章/篇/单元/模块/部分）
    pub outline_chaptered: bool,
    /// 是否配置了有效阶段（≥ MIN_PHASES）
    pub has_phases: bool,
    /// 显式拆解形状
    pub shape_hint: SplitShape,
    /// 是否可细粒度拆分
    pub is_divisible: bool,

    // ---- ④ 认知维 ----
    /// 认知型任务类别
    pub task_type: CognitiveType,
    /// 是否记忆强度型（记忆类或目标函数 = 留存最大化）
    pub is_memory_intensive: bool,
    /// 最大专注时长（小时；Phase 3 消费：任务时长估算）
    #[allow(dead_code)]
    pub max_attention_span: f64,

    // ---- ⑤ 上下文维 ----
    /// 既有负载：日期 → 子树外目标当日计划量合计
    pub existing_load: BTreeMap<NaiveDate, f64>,
    /// 跨目标单日任务预算
    pub global_budget: u32,
    /// 依赖链提示（大纲有序时的线性链说明）
    pub dependency_hint: Option<String>,
    /// 黑名单展开后的天数
    pub blackout_days: usize,
    /// 允许星期数（7 = 全周）
    pub allowed_weekday_count: usize,

    // ---- ⑥ 用户维 ----
    /// 习惯画像特征（冷启动为 None）
    pub profile: Option<ProfileFeatures>,
    /// 有效修正强度（画像缺失时 0）
    pub adapt_strength: f64,
    /// 模式采纳计数
    pub strategy_affinity: BTreeMap<String, i64>,
    /// 当前周末容量系数
    pub weekend_factor: f64,

    // ---- 输入回传（复习配置与显式形状判定用） ----
    /// 显式复习配置（None = 未指定）
    pub review: Option<ReviewSpec>,
}

impl Default for FeatureVector {
    fn default() -> Self {
        Self {
            horizon_span: 30,
            available_days: 30,
            workday_days: 22,
            weekend_days: 8,
            buffer_days: 0,
            declared_capacity: None,
            max_daily_qty: None,
            max_daily_tasks: None,
            min_chunk_qty: None,
            total_qty: 30.0,
            unit: "页".to_string(),
            required_daily_avg: 1.0,
            qty_granularity: QtyGranularity::Integer,
            has_outline: false,
            outline_len: 0,
            outline_chaptered: false,
            has_phases: false,
            shape_hint: SplitShape::Auto,
            is_divisible: true,
            task_type: CognitiveType::Learn,
            is_memory_intensive: false,
            max_attention_span: DEFAULT_ATTENTION_SPAN_HOURS,
            existing_load: BTreeMap::new(),
            global_budget: 6,
            dependency_hint: None,
            blackout_days: 0,
            allowed_weekday_count: 7,
            profile: None,
            adapt_strength: 0.0,
            strategy_affinity: BTreeMap::new(),
            weekend_factor: 1.0,
            review: None,
        }
    }
}

impl FeatureVector {
    /// 硬约束条数（R-13 安全门条件）
    fn hard_constraint_count(&self) -> u32 {
        let mut n = 0;
        if self.max_daily_qty.is_some() {
            n += 1;
        }
        if self.max_daily_tasks.is_some() {
            n += 1;
        }
        if self.min_chunk_qty.is_some() {
            n += 1;
        }
        if self.blackout_days > 0 {
            n += 1;
        }
        if self.allowed_weekday_count < 7 {
            n += 1;
        }
        n
    }
}

/// 规则引擎输出：模式 + 命中规则 + 可执行修正
#[derive(Debug, Clone)]
pub struct PatternPlan {
    /// 最终拆解模式
    pub pattern: SplitPattern,
    /// 命中规则（按优先级降序，供 diagnostics.applied_rules）
    pub matched: Vec<AppliedRule>,
    /// 生效的复习配置（None = 不注入复习）
    pub review: Option<ReviewSpec>,
    /// 复习配置是否由规则自动注入（R-07）
    pub review_auto: bool,
    /// 容量基线系数（R-04；1.0 = 不修正）
    pub capacity_factor: f64,
    /// 最小粒度默认值（R-08；None = 不覆盖）
    pub min_chunk_qty: Option<f64>,
    /// 周末容量系数覆盖值（R-12；None = 不覆盖）
    pub weekend_factor: Option<f64>,
    /// 当日权重系数（R-05：负载过高日 × 0.5）
    pub load_weights: BTreeMap<NaiveDate, f64>,
    /// 是否生成学习任务线性依赖链（R-10）
    pub dependency_chain: bool,
    /// 是否建议启用修复循环（R-13 安全门）
    pub repair_enabled: bool,
    /// 是否短视界（R-01）
    pub short_horizon: bool,
}

impl Default for PatternPlan {
    fn default() -> Self {
        Self {
            pattern: SplitPattern::Even,
            matched: Vec::new(),
            review: None,
            review_auto: false,
            capacity_factor: 1.0,
            min_chunk_qty: None,
            weekend_factor: None,
            load_weights: BTreeMap::new(),
            dependency_chain: false,
            repair_enabled: false,
            short_horizon: false,
        }
    }
}

impl PatternPlan {
    /// 把规则修正应用到编译约束的副本上（不修改入参）
    ///
    /// 修正顺序：容量基线（R-04）→ 最小粒度（R-08）→ 周末系数（R-12）→
    /// 当日负载权重（R-05）→ 重算有效天数与总容量。
    pub fn apply_to_spec(&self, compiled: &CompiledSpec) -> CompiledSpec {
        let mut spec = compiled.clone();

        if (self.capacity_factor - 1.0).abs() > 1e-9 {
            let base = spec.daily_capacity.unwrap_or(1.0);
            spec.daily_capacity = Some((base * self.capacity_factor).max(1e-9));
        }

        if spec.min_chunk_qty.is_none() {
            if let Some(chunk) = self.min_chunk_qty {
                spec.min_chunk_qty = Some(chunk);
            }
        }

        if let Some(factor) = self.weekend_factor {
            if (factor - spec.weekend_factor).abs() > 1e-9 {
                for day in &mut spec.days {
                    let weekday = day.date.weekday().num_days_from_sunday();
                    if (weekday == 0 || weekday == 6)
                        && day.available
                        && !day.is_rest
                        && day.weight > 0.0
                    {
                        day.weight = factor.max(0.0);
                    }
                }
                spec.weekend_factor = factor;
            }
        }

        for (date, factor) in &self.load_weights {
            if let Some(day) = spec.days.iter_mut().find(|d| d.date == *date) {
                if day.available && !day.is_rest {
                    day.weight = (day.weight * factor).max(0.0);
                }
            }
        }

        spec.effective_days = spec
            .days
            .iter()
            .filter(|d| d.available && d.weight > 0.0)
            .count() as u32;
        spec.total_capacity = spec
            .days
            .iter()
            .filter(|d| d.available && d.weight > 0.0)
            .map(|d| d.weight)
            .sum();
        spec
    }
}

/// 是否记忆型关键词（§4.4.2 关键词表）
fn is_memory_keyword(text: &str) -> bool {
    ["背", "记", "单词", "词汇", "公式", "条文", "口诀"]
        .iter()
        .any(|k| text.contains(k))
}

/// 是否练习型关键词
fn is_practice_keyword(text: &str) -> bool {
    ["刷", "练", "题", "真题", "模考"]
        .iter()
        .any(|k| text.contains(k))
}

/// 是否项目型关键词
fn is_project_keyword(text: &str) -> bool {
    ["项目", "实战", "开发", "搭建", "作品"]
        .iter()
        .any(|k| text.contains(k))
}

/// 认知型任务识别（顺序：目标函数 → 记忆 → 练习 → 项目 → 学习/默认）
fn detect_cognitive_type(
    objective: crate::services::planner::types::Objective,
    text: &str,
) -> CognitiveType {
    use crate::services::planner::types::Objective;
    if objective == Objective::MaximizeRetention {
        return CognitiveType::Memory;
    }
    let lowered = text.to_lowercase();
    if is_memory_keyword(&lowered) {
        return CognitiveType::Memory;
    }
    if is_practice_keyword(&lowered) {
        return CognitiveType::Practice;
    }
    if is_project_keyword(&lowered) {
        return CognitiveType::Project;
    }
    // 看/读/学/课程/视频/教材 与「个/词/道/页」等计数单位均回落到 Learn
    CognitiveType::Learn
}

/// 数量粒度识别
fn detect_granularity(unit: &str) -> QtyGranularity {
    match unit.trim() {
        "个" | "词" | "道" | "题" | "页" | "本" | "套" | "章" | "篇" | "次" | "遍" | "份" | "张" => {
            QtyGranularity::Integer
        }
        _ => QtyGranularity::Fractional,
    }
}

/// 单位是否支持细粒度拆分
fn is_divisible_unit(unit: &str) -> bool {
    !matches!(unit.trim(), "本" | "套" | "门" | "次" | "遍")
}

/// 大纲是否含章节标记
fn outline_chaptered(outline: &[String]) -> bool {
    outline.iter().any(|entry| {
        ["章", "篇", "单元", "模块", "部分"]
            .iter()
            .any(|k| entry.contains(k))
    })
}

/// 提取特征向量
///
/// `adapt_strength` 取自统一入参 learning 层（默认 1.0），画像缺失时置 0。
pub fn extract_features(
    goal: &Goal,
    unified: &UnifiedPlanRequest,
    snapshot: &PlanSnapshot,
    compiled: &CompiledSpec,
    profile: Option<&PlanningProfile>,
) -> FeatureVector {
    let structure: &StructureSpec = &unified.structure;
    let semantic: &SemanticSpec = &unified.semantic;
    let learning: &LearningSpec = &unified.learning;

    let horizon_span = (compiled.end - compiled.start).num_days() + 1;
    let available_days = compiled.effective_days as i64;
    let workday_days = compiled
        .days
        .iter()
        .filter(|d| d.available && d.weight > 0.0)
        .filter(|d| {
            let w = d.date.weekday().num_days_from_sunday();
            w != 0 && w != 6
        })
        .count() as i64;
    let weekend_days = compiled
        .days
        .iter()
        .filter(|d| d.available && d.weight > 0.0)
        .filter(|d| {
            let w = d.date.weekday().num_days_from_sunday();
            w == 0 || w == 6
        })
        .count() as i64;

    let text = format!(
        "{} {}",
        goal.name,
        goal.description.as_deref().unwrap_or_default()
    );
    let task_type = detect_cognitive_type(unified.objective, &text);
    let memory_intensive = task_type == CognitiveType::Memory
        || unified.objective == crate::services::planner::types::Objective::MaximizeRetention;

    let profile_features = profile.and_then(|p| {
        ProfileFeatures::from_profile(p, learning.adapt_strength.unwrap_or(1.0))
    });
    let adapt_strength = profile_features
        .as_ref()
        .map(|p| p.adapt_strength)
        .unwrap_or(0.0);

    let outline = &semantic.outline;
    let review = structure.review.clone();

    FeatureVector {
        horizon_span,
        available_days,
        workday_days,
        weekend_days,
        buffer_days: compiled.buffer_days as i64,
        declared_capacity: compiled.daily_capacity,
        max_daily_qty: compiled.max_daily_qty,
        max_daily_tasks: compiled.max_daily_tasks,
        min_chunk_qty: compiled.min_chunk_qty,
        total_qty: compiled.total_qty,
        unit: compiled.unit.clone(),
        required_daily_avg: if available_days > 0 {
            compiled.total_qty / available_days as f64
        } else {
            0.0
        },
        qty_granularity: detect_granularity(&compiled.unit),
        has_outline: !outline.is_empty(),
        outline_len: outline.len(),
        outline_chaptered: outline_chaptered(outline),
        has_phases: structure
            .phases
            .as_ref()
            .map(|p| p.len() >= MIN_PHASES)
            .unwrap_or(false),
        shape_hint: structure.shape.unwrap_or_default(),
        is_divisible: is_divisible_unit(&compiled.unit),
        task_type,
        is_memory_intensive: memory_intensive,
        max_attention_span: DEFAULT_ATTENTION_SPAN_HOURS,
        existing_load: snapshot.external_load.iter().map(|(k, v)| (*k, *v)).collect(),
        global_budget: compiled.global_daily_task_budget,
        dependency_hint: if outline.len() >= 2 {
            Some(format!("大纲 {} 条有序，生成线性依赖链", outline.len()))
        } else {
            None
        },
        blackout_days: compiled.blackout.len(),
        allowed_weekday_count: compiled.allowed_weekdays.len(),
        profile: profile_features,
        adapt_strength,
        strategy_affinity: profile
            .map(|p| {
                p.strategy_affinity
                    .iter()
                    .map(|(k, v)| (k.clone(), *v))
                    .collect()
            })
            .unwrap_or_default(),
        weekend_factor: compiled.weekend_factor,
        review,
    }
}

/// 显式 shape → F-4 模式映射（与 [`super::super::split_planner::resolve_pattern`] 一致）
fn map_shape(shape: SplitShape) -> SplitPattern {
    match shape {
        SplitShape::Even => SplitPattern::Even,
        SplitShape::FrontLoaded | SplitShape::BackLoaded => SplitPattern::CapacityBound,
        SplitShape::Phased | SplitShape::Milestone => SplitPattern::Phased,
        SplitShape::SpacedRepetition => SplitPattern::SpacedRepetition,
        SplitShape::Auto => SplitPattern::Even,
    }
}

/// 画像中的模式偏好名 → 模式枚举（大小写与分隔符不敏感）
fn affinity_pattern(name: &str) -> Option<SplitPattern> {
    let key = name.to_lowercase();
    if key.contains("hybrid") {
        Some(SplitPattern::Hybrid)
    } else if key.contains("phased") {
        Some(SplitPattern::Phased)
    } else if key.contains("spaced") {
        Some(SplitPattern::SpacedRepetition)
    } else if key.contains("milestone") {
        Some(SplitPattern::MilestoneBackward)
    } else if key.contains("capacity") {
        Some(SplitPattern::CapacityBound)
    } else if key.contains("even") {
        Some(SplitPattern::Even)
    } else {
        None
    }
}

/// 规则引擎主入口
///
/// 按 RULE_TABLE 的优先级降序执行 R-01~R-15；显式 shape 在规则评估后覆盖
/// 模式（F-4 单向映射），但规则产出的容量/粒度/权重修正仍然生效。
pub fn evaluate_rules(features: &FeatureVector) -> PatternPlan {
    let mut plan = PatternPlan::default();
    let mut short_horizon = false;
    let mut capacity_bound = false;
    let mut phased = false;
    let mut spaced = false;
    let mut milestone = false;
    let mut affinity: Option<SplitPattern> = None;

    let mut order: Vec<&RuleDef> = RULE_TABLE.iter().collect();
    order.sort_by(|a, b| b.priority.cmp(&a.priority).then_with(|| a.id.cmp(b.id)));

    for rule in order {
        match rule.id {
            // R-01 短视界：禁用间隔重复 + 模式降级
            "R-01" => {
                if features.horizon_span < SHORT_HORIZON_DAYS {
                    short_horizon = true;
                    spaced = false;
                    hit(
                        &mut plan.matched,
                        "R-01",
                        format!(
                            "时间视界仅 {} 天（< {}），禁用间隔重复并降级为均匀/容量模式",
                            features.horizon_span, SHORT_HORIZON_DAYS
                        ),
                    );
                }
            }
            // R-02 黑名单展开
            "R-02" => {
                if features.blackout_days > 0 {
                    hit(
                        &mut plan.matched,
                        "R-02",
                        format!(
                            "黑名单展开 {} 天，当日不可排期（已从可用日剔除）",
                            features.blackout_days
                        ),
                    );
                }
            }
            // R-11 容量不足 → 触发可行性补救
            "R-11" => {
                let max_capacity = features
                    .max_daily_qty
                    .or(features.declared_capacity)
                    .unwrap_or(1.0)
                    .max(1e-9);
                let needed_days = (features.total_qty / max_capacity).ceil() as i64;
                if features.available_days < needed_days {
                    hit(
                        &mut plan.matched,
                        "R-11",
                        format!(
                            "可用日 {} 天 < 单日上限所需 {} 天，触发可行性补救流程",
                            features.available_days, needed_days
                        ),
                    );
                }
            }
            // R-13 安全门：硬约束校验与修复循环
            "R-13" => {
                let count = features.hard_constraint_count();
                if count > 0 {
                    plan.repair_enabled = true;
                    hit(
                        &mut plan.matched,
                        "R-13",
                        format!(
                            "存在 {} 项硬约束，违反时进入修复循环（≤{} 轮）",
                            count, super::repair::MAX_REPAIR_ROUNDS
                        ),
                    );
                }
            }
            // R-03 声明容量 → 容量约束模式
            "R-03" => {
                if features.declared_capacity.is_some() {
                    capacity_bound = true;
                    hit(
                        &mut plan.matched,
                        "R-03",
                        format!(
                            "声明每日容量 {}，启用容量约束模式",
                            features.declared_capacity.map(|v| v.to_string()).unwrap_or_default()
                        ),
                    );
                }
            }
            // R-05 既有负载避让
            "R-05" => {
                let threshold = LOAD_THRESHOLD_RATIO * features.global_budget as f64;
                if threshold > 0.0 {
                    let loaded: Vec<NaiveDate> = features
                        .existing_load
                        .iter()
                        .filter(|(_, qty)| **qty > threshold)
                        .map(|(date, _)| *date)
                        .collect();
                    if !loaded.is_empty() {
                        for date in &loaded {
                            plan.load_weights.insert(*date, LOAD_WEIGHT_FACTOR);
                        }
                        hit(
                            &mut plan.matched,
                            "R-05",
                            format!(
                                "{} 个日期既有负载超过日预算 {} 的 60%，当日权重 ×0.5",
                                loaded.len(),
                                features.global_budget
                            ),
                        );
                    }
                }
            }
            // R-04 完成率修正容量基线
            "R-04" => {
                if let Some(profile) = &features.profile {
                    if (profile.speed_factor - 1.0).abs() > SPEED_FACTOR_THRESHOLD
                        && profile.adapt_strength > 0.0
                    {
                        let factor = 1.0 + profile.adapt_strength * (profile.speed_factor - 1.0);
                        plan.capacity_factor = factor.max(1e-9);
                        hit(
                            &mut plan.matched,
                            "R-04",
                            format!(
                                "历史完成率 {:.2}（偏离 > 15%），容量基线 ×{:.2}",
                                profile.speed_factor, plan.capacity_factor
                            ),
                        );
                    }
                }
            }
            // R-07 记忆型 → 间隔重复
            "R-07" => {
                if features.is_memory_intensive {
                    spaced = true;
                    hit(
                        &mut plan.matched,
                        "R-07",
                        "记忆型目标，启用间隔重复并注入复习".to_string(),
                    );
                }
            }
            // R-06 阶段化（显式阶段或可切分大纲）
            "R-06" => {
                if features.has_phases || features.outline_chaptered {
                    phased = true;
                    hit(
                        &mut plan.matched,
                        "R-06",
                        if features.has_phases {
                            "配置了有效阶段，启用阶段化切分".to_string()
                        } else {
                            "大纲含章节标记，按阶段化切分".to_string()
                        },
                    );
                }
            }
            // R-10 有序大纲 → 依赖线性链
            "R-10" => {
                if features.has_outline && features.outline_len >= 2 {
                    plan.dependency_chain = true;
                    hit(
                        &mut plan.matched,
                        "R-10",
                        features
                            .dependency_hint
                            .clone()
                            .unwrap_or_else(|| "大纲有序，生成学习任务线性依赖链".to_string()),
                    );
                }
            }
            // R-08 偏好粒度 → 最小粒度默认值
            "R-08" => {
                if let Some(profile) = &features.profile {
                    let default_chunk = features.required_daily_avg;
                    if features.is_divisible
                        && profile.preferred_chunk > 0.0
                        && default_chunk > 0.0
                        && ((profile.preferred_chunk - default_chunk) / default_chunk).abs()
                            > PREFERRED_CHUNK_DEVIATION
                    {
                        let chunk = match features.qty_granularity {
                            QtyGranularity::Integer => profile.preferred_chunk.round().max(1.0),
                            QtyGranularity::Fractional => profile.preferred_chunk,
                        };
                        plan.min_chunk_qty = Some(chunk);
                        hit(
                            &mut plan.matched,
                            "R-08",
                            format!(
                                "历史偏好单任务粒度 {}{}（默认 {:.1}），作为最小粒度默认值",
                                chunk, features.unit, default_chunk
                            ),
                        );
                    }
                }
            }
            // R-12 周末产能偏低
            "R-12" => {
                if let Some(profile) = &features.profile {
                    if profile.gate >= 1.0
                        && features.workday_days > 0
                        && features.weekend_days > 0
                    {
                        let workday_avg = (1..=5)
                            .map(|w| profile.weekday_capacity[w])
                            .sum::<f64>()
                            / 5.0;
                        let weekend_avg =
                            (profile.weekday_capacity[0] + profile.weekday_capacity[6]) / 2.0;
                        if workday_avg > 0.0 {
                            let ratio = weekend_avg / workday_avg;
                            if ratio < WEEKEND_LOW_RATIO {
                                let target = features.weekend_factor.min(ratio.max(0.0));
                                if target < features.weekend_factor - 1e-9 {
                                    plan.weekend_factor = Some(target);
                                    hit(
                                        &mut plan.matched,
                                        "R-12",
                                        format!(
                                            "周末产能为工作日的 {:.0}%，周末容量系数 {} → {:.2}",
                                            ratio * 100.0,
                                            features.weekend_factor,
                                            target
                                        ),
                                    );
                                }
                            }
                        }
                    }
                }
            }
            // R-09 模式偏好（优先级 +10，仅用于平级裁决）
            "R-09" => {
                let total: i64 = features.strategy_affinity.values().sum();
                if total > 0 {
                    if let Some((name, count)) = features
                        .strategy_affinity
                        .iter()
                        .max_by_key(|(_, c)| **c)
                    {
                        if *count >= AFFINITY_MIN_COUNT
                            && (*count as f64 / total as f64) >= AFFINITY_MIN_SHARE
                        {
                            if let Some(p) = affinity_pattern(name) {
                                affinity = Some(p);
                                hit(
                                    &mut plan.matched,
                                    "R-09",
                                    format!(
                                        "历史模式偏好 {}（{}/{}），该模式优先级 +10",
                                        name, count, total
                                    ),
                                );
                            }
                        }
                    }
                }
            }
            // R-14 项目型 → 里程碑倒推
            "R-14" => {
                if features.task_type == CognitiveType::Project {
                    milestone = true;
                    hit(
                        &mut plan.matched,
                        "R-14",
                        "项目型目标，启用里程碑倒推".to_string(),
                    );
                }
            }
            // R-15 缓冲日
            "R-15" => {
                if features.buffer_days > 0 {
                    hit(
                        &mut plan.matched,
                        "R-15",
                        format!(
                            "末尾预留 {} 天缓冲，已从可用日尾部剔除",
                            features.buffer_days
                        ),
                    );
                }
            }
            _ => {}
        }
    }

    // 模式合成（§4.4.5：阶段+复习 / 阶段+容量 → HYBRID；R-01 优先降级）
    plan.pattern = if short_horizon {
        if capacity_bound {
            SplitPattern::CapacityBound
        } else {
            SplitPattern::Even
        }
    } else if phased && spaced {
        SplitPattern::Hybrid
    } else if phased && capacity_bound {
        SplitPattern::Hybrid
    } else if phased {
        SplitPattern::Phased
    } else if spaced {
        SplitPattern::SpacedRepetition
    } else if capacity_bound {
        SplitPattern::CapacityBound
    } else if milestone {
        SplitPattern::MilestoneBackward
    } else if let Some(p) = affinity {
        p
    } else {
        SplitPattern::Even
    };
    plan.short_horizon = short_horizon;

    // 显式 shape 覆盖模式（规则修正仍生效）
    if features.shape_hint != SplitShape::Auto {
        plan.pattern = map_shape(features.shape_hint);
    }

    // 复习配置解析：显式优先；R-07 记忆型自动注入默认间隔；R-01 禁用
    plan.review = resolve_review(features, short_horizon, &mut plan.review_auto);

    plan
}

/// 复习配置解析
fn resolve_review(
    features: &FeatureVector,
    short_horizon: bool,
    review_auto: &mut bool,
) -> Option<ReviewSpec> {
    if short_horizon {
        return None;
    }
    if let Some(explicit) = &features.review {
        match explicit.enabled {
            Some(false) => return None,
            Some(true) if !explicit.intervals.is_empty() => return Some(explicit.clone()),
            Some(true) => {}
            None => {
                // enabled 缺省视为未启用；但有间隔配置时沿用旧口径（enabled 未显式 true 也不启用）
                return None;
            }
        }
    }
    if features.is_memory_intensive {
        *review_auto = true;
        Some(ReviewSpec {
            enabled: Some(true),
            intervals: DEFAULT_REVIEW_INTERVALS.to_vec(),
            review_ratio: None,
            max_review_per_day: None,
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cognitive_type_detection() {
        use crate::services::planner::types::Objective;
        assert_eq!(
            detect_cognitive_type(Objective::FinishByDeadline, "背单词 3000"),
            CognitiveType::Memory
        );
        assert_eq!(
            detect_cognitive_type(Objective::FinishByDeadline, "刷数学真题"),
            CognitiveType::Practice
        );
        assert_eq!(
            detect_cognitive_type(Objective::FinishByDeadline, "开发个人项目"),
            CognitiveType::Project
        );
        assert_eq!(
            detect_cognitive_type(Objective::FinishByDeadline, "看视频课程"),
            CognitiveType::Learn
        );
        assert_eq!(
            detect_cognitive_type(Objective::MaximizeRetention, "复习"),
            CognitiveType::Memory
        );
        assert_eq!(
            detect_cognitive_type(Objective::FinishByDeadline, "随便"),
            CognitiveType::Learn
        );
    }

    #[test]
    fn test_r01_short_horizon_downgrades_to_even() {
        let mut f = FeatureVector {
            horizon_span: 5,
            available_days: 5,
            has_phases: true,
            is_memory_intensive: true,
            ..Default::default()
        };
        f.review = Some(ReviewSpec {
            enabled: Some(true),
            intervals: vec![1, 2],
            review_ratio: None,
            max_review_per_day: None,
        });
        let plan = evaluate_rules(&f);
        assert_eq!(plan.pattern, SplitPattern::Even);
        assert!(plan.short_horizon);
        assert!(plan.review.is_none());
        assert!(plan.matched.iter().any(|r| r.id == "R-01"));
    }

    #[test]
    fn test_r01_short_horizon_with_capacity_is_capacity_bound() {
        let f = FeatureVector {
            horizon_span: 5,
            declared_capacity: Some(2.0),
            ..Default::default()
        };
        assert_eq!(evaluate_rules(&f).pattern, SplitPattern::CapacityBound);
    }

    #[test]
    fn test_r03_capacity_and_r07_review_auto_injection() {
        let f = FeatureVector {
            declared_capacity: Some(2.0),
            is_memory_intensive: true,
            task_type: CognitiveType::Memory,
            ..Default::default()
        };
        let plan = evaluate_rules(&f);
        assert_eq!(plan.pattern, SplitPattern::SpacedRepetition);
        let review = plan.review.expect("记忆型应自动注入复习");
        assert_eq!(review.intervals, DEFAULT_REVIEW_INTERVALS.to_vec());
        assert!(plan.review_auto);
        assert!(plan.matched.iter().any(|r| r.id == "R-03"));
        assert!(plan.matched.iter().any(|r| r.id == "R-07"));
    }

    #[test]
    fn test_r06_r07_hybrid_and_r10_dependency_chain() {
        let f = FeatureVector {
            has_phases: true,
            is_memory_intensive: true,
            task_type: CognitiveType::Memory,
            has_outline: true,
            outline_len: 3,
            ..Default::default()
        };
        let plan = evaluate_rules(&f);
        assert_eq!(plan.pattern, SplitPattern::Hybrid);
        assert!(plan.dependency_chain);
    }

    #[test]
    fn test_r11_capacity_shortage_hits() {
        let f = FeatureVector {
            available_days: 5,
            total_qty: 100.0,
            max_daily_qty: Some(10.0),
            ..Default::default()
        };
        let plan = evaluate_rules(&f);
        assert!(plan.matched.iter().any(|r| r.id == "R-11"));
    }

    #[test]
    fn test_r13_repair_gate_with_hard_constraints() {
        let f = FeatureVector {
            max_daily_qty: Some(10.0),
            min_chunk_qty: Some(2.0),
            ..Default::default()
        };
        let plan = evaluate_rules(&f);
        assert!(plan.repair_enabled);
        assert!(plan.matched.iter().any(|r| r.id == "R-13"));
    }

    #[test]
    fn test_r14_project_milestone() {
        let f = FeatureVector {
            task_type: CognitiveType::Project,
            ..Default::default()
        };
        assert_eq!(
            evaluate_rules(&f).pattern,
            SplitPattern::MilestoneBackward
        );
    }

    #[test]
    fn test_r15_buffer_reported() {
        let f = FeatureVector {
            buffer_days: 3,
            ..Default::default()
        };
        let plan = evaluate_rules(&f);
        assert!(plan.matched.iter().any(|r| r.id == "R-15"));
    }

    #[test]
    fn test_r02_blackout_reported() {
        let f = FeatureVector {
            blackout_days: 2,
            ..Default::default()
        };
        let plan = evaluate_rules(&f);
        assert!(plan.matched.iter().any(|r| r.id == "R-02"));
    }

    #[test]
    fn test_r04_r08_r12_profile_corrections() {
        let f = FeatureVector {
            profile: Some(ProfileFeatures {
                speed_factor: 0.5,
                weekday_capacity: [0.5, 1.0, 1.0, 1.0, 1.0, 1.0, 0.5],
                preferred_chunk: 10.0,
                adapt_strength: 1.0,
                gate: 1.0,
            }),
            adapt_strength: 1.0,
            required_daily_avg: 2.0,
            weekend_factor: 1.0,
            ..Default::default()
        };
        let plan = evaluate_rules(&f);
        assert!((plan.capacity_factor - 0.5).abs() < 1e-9);
        assert_eq!(plan.min_chunk_qty, Some(10.0));
        assert_eq!(plan.weekend_factor, Some(0.5));
        for id in ["R-04", "R-08", "R-12"] {
            assert!(plan.matched.iter().any(|r| r.id == id), "{} 应命中", id);
        }
    }

    #[test]
    fn test_r05_load_days_weighted() {
        let date = NaiveDate::from_ymd_opt(2026, 10, 26).unwrap();
        let mut load = BTreeMap::new();
        load.insert(date, 5.0);
        let f = FeatureVector {
            existing_load: load,
            global_budget: 6,
            ..Default::default()
        };
        let plan = evaluate_rules(&f);
        assert_eq!(plan.load_weights.get(&date), Some(&LOAD_WEIGHT_FACTOR));
    }

    #[test]
    fn test_r09_affinity_tie_break() {
        let mut affinity = BTreeMap::new();
        affinity.insert("CAPACITY_BOUND".to_string(), 5);
        affinity.insert("EVEN".to_string(), 1);
        let f = FeatureVector {
            strategy_affinity: affinity,
            ..Default::default()
        };
        assert_eq!(
            evaluate_rules(&f).pattern,
            SplitPattern::CapacityBound
        );
    }

    #[test]
    fn test_rule_order_is_deterministic_by_priority() {
        let mut load = BTreeMap::new();
        load.insert(NaiveDate::from_ymd_opt(2026, 10, 26).unwrap(), 9.0);
        let f = FeatureVector {
            horizon_span: 5,
            available_days: 5,
            declared_capacity: Some(2.0),
            max_daily_qty: Some(1.0),
            total_qty: 20.0,
            blackout_days: 1,
            existing_load: load,
            buffer_days: 2,
            task_type: CognitiveType::Project,
            ..Default::default()
        };
        let plan = evaluate_rules(&f);
        let ids: Vec<&str> = plan.matched.iter().map(|r| r.id.as_str()).collect();
        let mut expected = ids.clone();
        expected.sort_by_key(|id| {
            let meta = rule_meta(id);
            (std::cmp::Reverse(meta.priority), *id)
        });
        assert_eq!(ids, expected);
        assert_eq!(plan.pattern, SplitPattern::CapacityBound);
    }

    #[test]
    fn test_explicit_shape_overrides_pattern_but_keeps_corrections() {
        let f = FeatureVector {
            shape_hint: SplitShape::Even,
            declared_capacity: Some(2.0),
            is_memory_intensive: true,
            ..Default::default()
        };
        let plan = evaluate_rules(&f);
        assert_eq!(plan.pattern, SplitPattern::Even);
        assert!(plan.review.is_some());
    }

    #[test]
    fn test_apply_to_spec_updates_capacity_and_weekend() {
        use crate::services::planner::constraints::CompiledSpec;
        use crate::services::planner::snapshot::DaySlot;
        use crate::services::planner::types::SmoothingPolicy;

        let start = NaiveDate::from_ymd_opt(2026, 10, 26).unwrap();
        let days: Vec<DaySlot> = (0..7)
            .map(|i| {
                let date = start + chrono::Duration::days(i);
                let wd = date.weekday().num_days_from_sunday();
                DaySlot {
                    date,
                    available: true,
                    weight: if wd == 0 || wd == 6 { 1.0 } else { 1.0 },
                    is_rest: false,
                }
            })
            .collect();
        let compiled = CompiledSpec {
            start,
            end: start + chrono::Duration::days(6),
            buffer_days: 0,
            total_qty: 14.0,
            unit: "页".to_string(),
            daily_capacity: Some(2.0),
            max_daily_qty: None,
            max_daily_hours: None,
            max_daily_tasks: None,
            min_chunk_qty: None,
            global_daily_task_budget: 6,
            blackout: Default::default(),
            allowed_weekdays: (0..7).collect(),
            weekend_factor: 1.0,
            adapt_strength: 1.0,
            smoothing: SmoothingPolicy::default(),
            days,
            effective_days: 7,
            total_capacity: 7.0,
        };
        let mut plan = PatternPlan::default();
        plan.capacity_factor = 1.5;
        plan.min_chunk_qty = Some(3.0);
        plan.weekend_factor = Some(0.5);
        plan.load_weights.insert(
            start + chrono::Duration::days(1),
            LOAD_WEIGHT_FACTOR,
        );
        let adjusted = plan.apply_to_spec(&compiled);
        assert_eq!(adjusted.daily_capacity, Some(3.0));
        assert_eq!(adjusted.min_chunk_qty, Some(3.0));
        // 周一负载 ×0.5 + 周六/周日 ×0.5 → 0.5 + 4×1.0 + 2×0.5 = 5.5
        assert!((adjusted.total_capacity - 5.5).abs() < 1e-9);
        let monday = adjusted
            .days
            .iter()
            .find(|d| d.date == start + chrono::Duration::days(1))
            .unwrap();
        assert!((monday.weight - 0.5).abs() < 1e-9);
    }
}
