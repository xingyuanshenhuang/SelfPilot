/**
 * 重新规划（阶段3）—— 后端契约 cmd
 *
 * - replan_solve：只读求解预览（永不落库，what-if 天然成立），键名 req
 * - replan_apply：提交应用（内部重新求解校验 plan_hash，防 stale），键名 req
 * - replan_revert：撤销最近一次已应用的规划，命名参数 event_id
 * - replan_list_events：事件时间线，命名参数 goal_id / limit / offset
 * - replan_event_detail：事件 + 差异明细，命名参数 event_id
 *
 * 注意：invoke args 键名必须与 Rust 命令参数名一致（replan_solve/apply 用 req，
 * 其余按命名参数）。
 */
import { invokeCommand } from "./client";
import type {
  ReplanApplyInput,
  ReplanApplyResult,
  ReplanEvent,
  ReplanEventDetail,
  ReplanPlan,
  ReplanSolveInput,
} from "@/types";

/** 重新规划 · 预览（只读求解，永不落库） */
export async function replanSolve(req: ReplanSolveInput): Promise<ReplanPlan> {
  return invokeCommand<ReplanPlan>("replan_solve", { req });
}

/** 重新规划 · 提交应用 */
export async function replanApply(
  req: ReplanApplyInput,
): Promise<ReplanApplyResult> {
  return invokeCommand<ReplanApplyResult>("replan_apply", { req });
}

/** 重新规划 · 撤销最近一次已应用的规划 */
export async function replanRevert(
  event_id: string,
): Promise<ReplanApplyResult> {
  return invokeCommand<ReplanApplyResult>("replan_revert", { event_id });
}

/** 重新规划 · 事件时间线（created_at DESC，分页） */
export async function replanListEvents(
  goal_id: string,
  limit = 50,
  offset = 0,
): Promise<ReplanEvent[]> {
  return invokeCommand<ReplanEvent[]>("replan_list_events", {
    goal_id,
    limit,
    offset,
  });
}

/** 重新规划 · 事件详情（含差异明细） */
export async function replanEventDetail(
  event_id: string,
): Promise<ReplanEventDetail> {
  return invokeCommand<ReplanEventDetail>("replan_event_detail", { event_id });
}