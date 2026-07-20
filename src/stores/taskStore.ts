import { defineStore } from "pinia";
import { ref } from "vue";
import * as taskApi from "@/api/task";
import * as encApi from "@/api/encouragement";
import * as statsApi from "@/api/stats";
import type {
  TodayTask,
  CompleteTaskInput,
  Encouragement,
  GoalCompletionStat,
  Task,
} from "@/types";

export const useTaskStore = defineStore("task", () => {
  const todayTasks = ref<TodayTask[]>([]);
  const overdueTasks = ref<TodayTask[]>([]);
  const loading = ref(false);

  /** 待显示的鼓励语（完成今日首个任务后触发，App.vue watch 此值弹窗） */
  const pendingEncouragement = ref<Encouragement | null>(null);
  /** 是否为庆祝鼓励语（全部目标完成） */
  const isCelebration = ref(false);

  async function fetchAll() {
    loading.value = true;
    try {
      const [today, overdue] = await Promise.all([
        taskApi.listTodayTasks(),
        taskApi.listOverdueTasks(),
      ]);
      todayTasks.value = today;
      overdueTasks.value = overdue;
    } finally {
      loading.value = false;
    }
  }

  async function completeTask(input: CompleteTaskInput) {
    // 记录完成前的今日已完成数
    const beforeDoneCount = todayTasks.value.filter(
      (t) => t.status === "done",
    ).length;

    const updated = await taskApi.completeTask(input);
    await fetchAll();

    // PRD §4.2 模块七 & Sprint 5：完成当日首个任务后弹出鼓励语
    // beforeDoneCount === 0 表示这是今日第一个完成的任务
    if (beforeDoneCount === 0) {
      try {
        // Sprint 5：检查是否全部目标完成
        const allComplete = await checkAllGoalsComplete();

        if (allComplete) {
          // 全部目标完成 → 庆祝鼓励语
          const enc = await encApi.randomCelebrationEncouragement(
            "complete_celebration",
          );
          if (enc) {
            pendingEncouragement.value = enc;
            isCelebration.value = true;
          }
        } else {
          // 根据连续天数选择等级鼓励语（P3-4：传入 longest_streak）
          const streakInfo = await encApi.getStreak();
          const enc = await encApi.randomEncouragementByStreak(
            streakInfo.current_streak,
            streakInfo.longest_streak,
            "complete_first",
          );
          if (enc) {
            pendingEncouragement.value = enc;
            isCelebration.value = false;
          }
        }
      } catch {
        // 后端不可用时静默失败
      }
    }

    return updated;
  }

  /** 检查是否所有目标都已完成（percentage >= 1.0） */
  async function checkAllGoalsComplete(): Promise<boolean> {
    try {
      const stats: GoalCompletionStat[] =
        await statsApi.getGoalCompletionStats();
      if (stats.length === 0) return false;
      return stats.every((s: GoalCompletionStat) => s.percentage >= 1.0);
    } catch {
      return false;
    }
  }

  /** 清除待显示的鼓励语（App.vue 弹窗关闭后调用） */
  function clearPendingEncouragement() {
    pendingEncouragement.value = null;
    isCelebration.value = false;
  }

  async function skipTask(taskId: string): Promise<Task> {
    const updated = await taskApi.skipTask(taskId);
    await fetchAll();
    return updated;
  }

  return {
    todayTasks,
    overdueTasks,
    loading,
    pendingEncouragement,
    isCelebration,
    fetchAll,
    completeTask,
    skipTask,
    clearPendingEncouragement,
  };
});
