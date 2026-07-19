import { invoke } from "@tauri-apps/api/core";
import type {
  Task,
  CreateTaskInput,
  CompleteTaskInput,
  TodayTask,
  MoveTaskInput,
  CalendarTask,
  UpdateTaskInput,
  SetTaskDependencyInput,
  TaskDependency,
  DeleteTaskResult,
  DeleteTasksBatchResult,
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

/** 删除任务（P2-3：返回被删任务所属 goal_id，供局部更新进度） */
export async function deleteTask(taskId: string): Promise<DeleteTaskResult> {
  return invoke("delete_task", { taskId });
}

/** 批量删除任务（单事务，避免 N 次 IPC 调用撑爆通道）
 *
 * P2-3：返回受影响的 goal_id 列表，供局部更新进度。
 */
export async function deleteTasksBatch(
  taskIds: string[],
): Promise<DeleteTasksBatchResult> {
  return invoke("delete_tasks_batch", { taskIds });
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

// ===== P1-1 任务依赖关系 =====

/** 设置任务依赖（task_id 依赖 depends_on_id），自动防循环 */
export async function setTaskDependency(
  input: SetTaskDependencyInput,
): Promise<void> {
  return invoke("set_task_dependency", { input });
}

/** 列出某任务的直接前置依赖任务 */
export async function listTaskDependencies(taskId: string): Promise<Task[]> {
  return invoke("list_task_dependencies", { taskId });
}

/** 列出依赖某任务的后继任务（哪些任务依赖此任务） */
export async function listTaskDependents(taskId: string): Promise<Task[]> {
  return invoke("list_task_dependents", { taskId });
}

/** 移除任务依赖 */
export async function removeTaskDependency(
  taskId: string,
  dependsOnId: string,
): Promise<void> {
  return invoke("remove_task_dependency", { taskId, dependsOnId });
}

/** 校验添加依赖是否会形成循环（返回 true 表示安全无环） */
export async function validateDependencyChain(
  taskId: string,
  dependsOnId: string,
): Promise<boolean> {
  return invoke("validate_dependency_chain", { taskId, dependsOnId });
}

/** 列出某任务的依赖记录（含 id、created_at） */
export async function listTaskDependencyRecords(
  taskId: string,
): Promise<TaskDependency[]> {
  return invoke("list_task_dependency_records", { taskId });
}
