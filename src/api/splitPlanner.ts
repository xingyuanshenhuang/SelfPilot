/**
 * 智能拆解 (Beta) —— 后端契约 cmd: smart_split_preview / smart_split_apply
 *
 * preview 纯预览不落库；apply 求解并落库为任务。
 *
 * 注意：invoke args 键名必须与 Rust 命令参数名一致（smart_split_* 用 req；
 * replan_* 用 input）。
 */
import { invokeCommand } from "./client";
import type { SmartSplitRequest, SplitPlan } from "@/types";

/** 智能拆解预览（不落库） */
export async function smartSplitPreview(
  input: SmartSplitRequest,
): Promise<SplitPlan> {
  return invokeCommand<SplitPlan>("smart_split_preview", { req: input });
}

/** 智能拆解并落库为任务 */
export async function smartSplitApply(
  input: SmartSplitRequest,
): Promise<SplitPlan> {
  return invokeCommand<SplitPlan>("smart_split_apply", { req: input });
}