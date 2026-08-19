import { invokeCommand } from "./client";
import type {
  Goal,
  CreateGoalInput,
  UpdateGoalInput,
  Task,
  ReplanPreview,
  ReplanResult,
  RepeatSplitInput,
  GoalTreeNode,
  MoveGoalInput,
  SmartSplitInput,
} from "@/types";

export async function createGoal(input: CreateGoalInput): Promise<Goal> {
  return invokeCommand("create_goal", { input });
}

export async function listGoals(): Promise<Goal[]> {
  return invokeCommand("list_goals");
}

export async function listGoalTree(): Promise<GoalTreeNode[]> {
  return invokeCommand("list_goal_tree");
}

export async function getGoal(id: string): Promise<Goal> {
  return invokeCommand("get_goal", { id });
}

export async function updateGoal(input: UpdateGoalInput): Promise<Goal> {
  return invokeCommand("update_goal", { input });
}

export async function deleteGoal(id: string): Promise<void> {
  return invokeCommand("delete_goal", { id });
}

export async function autoSplit(goalId: string): Promise<Task[]> {
  return invokeCommand("auto_split", { goalId });
}

/**
 * 按时间预算拆解（P1-3 时间预算模型）
 *
 * 与 autoSplit 不同，此函数按 goal.daily_capacity（每日可用时长）决定每个任务的计划量，
 * 而非按剩余天数平均分配总量。
 * - 任务数 = ceil(total_qty / daily_capacity)
 * - 每个任务 plan_qty = daily_capacity（最后一个可能不足）
 * - 每个任务 estimated_hours = daily_capacity
 */
export async function splitByCapacity(goalId: string): Promise<Task[]> {
  return invokeCommand("split_by_capacity", { goalId });
}

/**
 * 智能拆解（整合入口：按截止日期均分 / 按时间预算 / 自定义日期范围）
 *
 * 三种策略统一入口，参数可临时覆盖目标属性（不修改目标本身）：
 * - by_deadline：总量 ÷ 剩余天数均分（原视频拆解）
 * - by_capacity：按每日可用时长决定任务量（原时间预算）
 * - by_date_range：自定义起止日期，可选每日数量
 */
export async function smartSplit(input: SmartSplitInput): Promise<Task[]> {
  return invokeCommand("smart_split", { input });
}

/** 重复拆解（纯文字类任务：每天重复 or 单次） */
export async function repeatSplit(input: RepeatSplitInput): Promise<Task[]> {
  return invokeCommand("repeat_split", { input });
}

/** 重新规划预览 */
export async function replanPreview(goalId: string): Promise<ReplanPreview> {
  return invokeCommand("replan_preview", { goalId });
}

/** 执行重新规划 */
export async function replanGoal(goalId: string): Promise<ReplanResult> {
  return invokeCommand("replan_goal", { goalId });
}

/** 移动目标（跨层级归属调整与同级排序） */
export async function moveGoal(input: MoveGoalInput): Promise<Goal> {
  return invokeCommand("move_goal", { input });
}
