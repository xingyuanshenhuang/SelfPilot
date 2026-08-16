/**
 * 任务批量操作逻辑封装
 *
 * 提供任务选择、批量完成/跳过等功能
 */

import { ref } from "vue";
import { format } from "date-fns";
import type { CalendarTask } from "@/types";

export function useTaskBatch() {
  // ===== 状态 =====

  const selectedTaskIds = ref<Set<string>>(new Set());

  // ===== 选择操作 =====
  // 注意：直接调用 Set.prototype.add/delete/clear 不会可靠触发 ref 响应式更新。
  // 这里通过创建新 Set 实例并重新赋值，确保视图能正确响应选择状态变化。

  function toggleSelect(taskId: string, checked: boolean) {
    const newSet = new Set(selectedTaskIds.value);
    if (checked) {
      newSet.add(taskId);
    } else {
      newSet.delete(taskId);
    }
    selectedTaskIds.value = newSet;
  }

  function selectAllVisible(tasks: CalendarTask[]) {
    const newSet = new Set(selectedTaskIds.value);
    for (const t of tasks) {
      if (t.status !== "done" && t.status !== "skipped" && !t.is_blocked) {
        newSet.add(t.id);
      }
    }
    selectedTaskIds.value = newSet;
  }

  function selectAllVisibleWeek(
    weekGrid: Date[],
    tasksByDate: Record<string, CalendarTask[]>,
  ) {
    const newSet = new Set(selectedTaskIds.value);
    for (const day of weekGrid) {
      const key = format(day, "yyyy-MM-dd");
      const tasks = tasksByDate[key] || [];
      for (const t of tasks) {
        if (t.status !== "done" && t.status !== "skipped" && !t.is_blocked) {
          newSet.add(t.id);
        }
      }
    }
    selectedTaskIds.value = newSet;
  }

  function clearSelection() {
    selectedTaskIds.value = new Set();
  }

  // ===== 批量操作 =====

  async function batchComplete(
    tasks: CalendarTask[],
    completeFn: (taskId: string, actualQty: number) => Promise<void>,
    onSuccess?: () => void,
  ) {
    const ids = Array.from(selectedTaskIds.value);
    let ok = 0;
    const affectedGoalIds = new Set<string>();

    for (const id of ids) {
      const task = tasks.find((t) => t.id === id);
      if (!task) continue;

      try {
        await completeFn(id, task.plan_qty);
        affectedGoalIds.add(task.goal_id);
        ok++;
      } catch (e) {
        console.error(`任务 ${id} 完成失败:`, e);
      }
    }

    if (ok > 0) {
      console.log(`已批量完成 ${ok} 个任务`);
      clearSelection();
      onSuccess?.();
    }

    return { ok, affectedGoalIds };
  }

  async function batchSkip(
    tasks: CalendarTask[],
    skipFn: (taskId: string) => Promise<void>,
    onSuccess?: () => void,
  ) {
    const ids = Array.from(selectedTaskIds.value);
    let ok = 0;
    const affectedGoalIds = new Set<string>();

    for (const id of ids) {
      const task = tasks.find((t) => t.id === id);
      if (!task) continue;

      try {
        await skipFn(id);
        affectedGoalIds.add(task.goal_id);
        ok++;
      } catch (e) {
        console.error(`任务 ${id} 跳过失败:`, e);
      }
    }

    if (ok > 0) {
      console.log(`已批量跳过 ${ok} 个任务`);
      clearSelection();
      onSuccess?.();
    }

    return { ok, affectedGoalIds };
  }

  // ===== 返回 =====

  return {
    // 状态
    selectedTaskIds,

    // 选择操作
    toggleSelect,
    selectAllVisible,
    selectAllVisibleWeek,
    clearSelection,

    // 批量操作
    batchComplete,
    batchSkip,
  };
}