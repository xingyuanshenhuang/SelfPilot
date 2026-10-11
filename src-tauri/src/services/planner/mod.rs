//! 目标树智能规划引擎（v2）——模块入口与公共导出
//!
//! 批次 A（阶段 1 · 第一批）交付四件套：
//! - [`types`]：统一契约类型层（`SmartSplitRequest` / `UnifiedPlanRequest` / `SplitPlan`）；
//! - [`snapshot`]：A1 快照装配器（目标子树 + 活跃任务 + 依赖边 + 既有负载 + 日槽）；
//! - [`constraints`]：A1 约束编译器（回落链与冲突优先级 → `CompiledSpec`）。
//!
//! 全部为纯新增（零侵入承诺）：不修改 split_service.rs / models.rs / commands/
//! 既有逻辑；后续批次（求解器、适配层、命令注册）在本模块内继续扩展。

// 批次 A 为纯新增，全部类型与函数尚无 crate 内调用方（services 是 lib.rs 的
// 私有模块，适配层/命令注册在批次 B/C 接入）。为使本批次 warning 清零，
// 暂对整个 planner 子树关闭 dead_code / unused_imports 提示；
// 后续批次接入调用方后移除本行。
#![allow(dead_code, unused_imports)]

pub mod allocator;
pub mod constraints;
pub mod dag_scheduler;
pub mod feasibility;
pub mod snapshot;
pub mod types;

// ---------- 引擎层公共导出（供 commands / adapters / 求解器使用） ----------

pub use constraints::{
    compile_constraints, CompiledSpec, DEFAULT_GLOBAL_DAILY_TASK_BUDGET,
    DEFAULT_WINDOW_CAP_DAYS,
};
pub use snapshot::{build_day_slots, DaySlot, HorizonSpan, PlanSnapshot};

pub use allocator::{allocate_weights, apply_hysteresis, DayQuota, MAX_ALLOC_ITER};
pub use dag_scheduler::{schedule_dag, PlacementReason, SchedTask, TaskPlacement};
pub use feasibility::{
    build_remedies, check_feasibility, longest_chain_length, MAX_INTENSITY_HARD,
    TIGHT_SLACK_THRESHOLD,
};

pub use types::{
    AppliedRule, BlackoutRange, ConstraintSpec, ConstraintsLayer, FeasibilityReport,
    FeasibilityStatus, HabitProfile, HardConstraints, HorizonInput, IntentSpec, LegacySpec,
    LearningSpec, Objective, PhaseSpec, PlanDiagnostics, PlanItem, PlanMode, PlanSummary,
    PlanWarning, PlanningSpec, Remedy, RemedyType, ResolvedHorizon, ResolvedPlanMeta, ReviewSpec,
    Scope, SemanticSpec, SmartSplitRequest, SmoothingPolicy, SoftConstraints, SplitPattern,
    SplitPlan, SplitShape, StructureSpec, TaskType, UnifiedPlanRequest, CURRENT_API_VERSION,
    ENGINE_VERSION, MAX_OUTLINE_ITEMS, MAX_PHASES, MAX_REVIEW_INTERVALS, MAX_TEMPLATE_CHARS,
    MIN_PHASES, PHASE_RATIO_TOLERANCE,
};
