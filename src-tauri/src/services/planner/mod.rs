//! 目标树智能规划引擎（v2）——模块入口与公共导出
//!
//! 批次 A（阶段 1 · 第一批）交付四件套：
//! - [`types`]：统一契约类型层（`SmartSplitRequest` / `UnifiedPlanRequest` / `SplitPlan`）；
//! - [`snapshot`]：A1 快照装配器（目标子树 + 活跃任务 + 依赖边 + 既有负载 + 日槽）；
//! - [`constraints`]：A1 约束编译器（回落链与冲突优先级 → `CompiledSpec`）；
//! - 其余子模块（allocator / dag_scheduler / feasibility）为求解器内部件。
//!
//! 消费方（阶段 2 起）：`split_planner::solve` 通过子模块路径直接引用
//! 各子模块符号，故不再在此维护顶层重导出。

pub mod allocator;
pub mod constraints;
pub mod dag_scheduler;
pub mod feasibility;
pub mod snapshot;
pub mod types;
