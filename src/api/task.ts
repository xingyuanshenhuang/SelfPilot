import { invoke } from "@tauri-apps/api/core";
import type {
  Task,
  CreateTaskInput,
  CompleteTaskInput,
  TodayTask,
  MoveTaskInput,
  CalendarTask,
  UpdateTaskInput,
} from "@/types";

export async function createTask(input: CreateTaskInput): Promise<Task> {
  return invoke("create_task", { input });
}

export async function completeTask(input: CompleteTaskInput): Promise<Task> {
  return invoke("complete_task", { input });
}

export async function skipTask(taskId: string): Promise<Task> {
  return invoke("skip_task", { taskId });
}

/** 补完成（历史任务，不触发重新规划） */
export async function backfillTask(input: CompleteTaskInput): Promise<Task> {
  return invoke("backfill_task", { input });
}

/** 移动任务到阶段 */
export async function moveTask(input: MoveTaskInput): Promise<Task> {
  return invoke("move_task", { input });
}

/** 更新任务计划数量（手动调整，标记 is_manual） */
export async function updateTaskPlanQty(
  taskId: string,
  planQty: number,
): Promise<Task> {
  return invoke("update_task_plan_qty", { taskId, planQty });
}

/** 通用更新任务（名称、计划日期、计划数量） */
export async function updateTask(input: UpdateTaskInput): Promise<Task> {
  return invoke("update_task", { input });
}

/** 删除任务 */
export async function deleteTask(taskId: string): Promise<void> {
  return invoke("delete_task", { taskId });
}

export async function listTodayTasks(): Promise<TodayTask[]> {
  return invoke("list_today_tasks");
}

export async function listOverdueTasks(): Promise<TodayTask[]> {
  return invoke("list_overdue_tasks");
}

export async function listTasksByGoal(goalId: string): Promise<Task[]> {
  return invoke("list_tasks_by_goal", { goalId });
}

/** 按日期范围查询任务（日历视图） */
export async function listTasksByDateRange(
  startDate: string,
  endDate: string,
): Promise<CalendarTask[]> {
  return invoke("list_tasks_by_date_range", { startDate, endDate });
}
