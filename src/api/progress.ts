import { invoke } from "@tauri-apps/api/core";
import type { ProgressInfo } from "@/types";

export async function getGoalProgress(goalId: string): Promise<ProgressInfo> {
  return invoke("get_goal_progress", { goalId });
}

export async function getAllGoalsProgress(): Promise<ProgressInfo[]> {
  return invoke("get_all_goals_progress");
}

/** 获取目标及其所有祖先的进度（P2-3：局部更新专用）
 *
 * 返回顺序：[自身, 父目标, 祖父目标, ...]
 * 用于写操作后只重算受影响的祖先链，而非全量重算所有目标进度。
 */
export async function getGoalAncestorsProgress(
  goalId: string,
): Promise<ProgressInfo[]> {
  return invoke("get_goal_ancestors_progress", { goalId });
}
