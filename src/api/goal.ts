import { invoke } from "@tauri-apps/api/core";
import type {
  Goal,
  CreateGoalInput,
  UpdateGoalInput,
  Task,
  ReplanPreview,
  ReplanResult,
  RepeatSplitInput,
  GoalTreeNode,
} from "@/types";

export async function createGoal(input: CreateGoalInput): Promise<Goal> {
  return invoke("create_goal", { input });
}

export async function listGoals(): Promise<Goal[]> {
  return invoke("list_goals");
}

export async function listGoalTree(): Promise<GoalTreeNode[]> {
  return invoke("list_goal_tree");
}

export async function getGoal(id: string): Promise<Goal> {
  return invoke("get_goal", { id });
}

export async function updateGoal(input: UpdateGoalInput): Promise<Goal> {
  return invoke("update_goal", { input });
}

export async function deleteGoal(id: string): Promise<void> {
  return invoke("delete_goal", { id });
}

export async function autoSplit(goalId: string): Promise<Task[]> {
  return invoke("auto_split", { goalId });
}

/** 重复拆解（纯文字类任务：每天重复 or 单次） */
export async function repeatSplit(input: RepeatSplitInput): Promise<Task[]> {
  return invoke("repeat_split", { input });
}

/** 重新规划预览 */
export async function replanPreview(goalId: string): Promise<ReplanPreview> {
  return invoke("replan_preview", { goalId });
}

/** 执行重新规划 */
export async function replanGoal(goalId: string): Promise<ReplanResult> {
  return invoke("replan_goal", { goalId });
}
