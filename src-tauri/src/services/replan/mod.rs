//! 目标树智能规划（阶段 3）· 重新规划求解与落库
//!
//! 依据《目标树重新规划功能重构设计文档》§3.2~§3.4（A1 简化版 / A2 / A3）：
//! - [`planner::solve`]：只读求解（永不落库；`dry_run` 天然成立），
//!   复用阶段 1 的 `planner::snapshot::load_snapshot` 装配快照；
//! - [`apply::apply`]：提交应用（预览与提交同源：内部重新求解并校验
//!   plan_hash，防 stale；事务内落库 + 事件流水 + diff 留痕）；
//! - [`apply::revert`]：撤销最近一次 applied 事件（三条件校验 + 反向应用 diff）。
//!
//! 设计要点：
//! - 数量守恒 INV-1：参与重排任务的 Σ new_plan_qty 必须等于其剩余量
//!   Σ max(0, plan_qty − actual_qty)（1e-6 容差），违反返回 Err；
//! - 锁定任务（schedule_locked 非空且 respect_locks 生效）与手动任务
//!   （is_manual=1）不参与重新分配，new = old（change_type=locked/retained）；
//! - 不可行时不抛错：仍返回携带 feasibility.options 与 warning 的完整计划。

pub mod apply;
pub mod explain;
pub mod planner;

pub use apply::{apply, revert};
pub use planner::solve;
