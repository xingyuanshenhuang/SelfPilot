import { defineStore } from "pinia";
import { ref, computed } from "vue";
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

  /** 待显示的鼓励语（App.vue watch 此值弹居中弹框） */
  const pendingEncouragement = ref<Encouragement | null>(null);
  /** 是否为庆祝鼓励语（全部目标完成） */
  const isCelebration = ref(false);
  /** 是否为里程碑鼓励语（接近/超越历史最长连续） */
  const isMilestone = ref(false);

  /** 今日已完成任务数 */
  const todayDoneCount = computed(
    () => todayTasks.value.filter((t) => t.status === "done").length,
  );

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

  // ---------- 鼓励语统一展示入口（F1/F2/F3/F4/F8 收口点） ----------
  // 所有视图（目标总览 / 目标树 / 日历）的完成、补完成一律经由此处，
  // 集中处理：开关(enabled)、频率(frequency)、风格(style)、庆祝、emoji 装饰。
  /** 设置装载状态（懒加载，避免首次调用时 settings 尚未拉取） */
  // 注意：encStore 在函数体内延迟获取，避免与 encouragementStore 循环依赖
  async function maybeShowEncouragement(first: boolean) {
    const { useEncouragementStore } = await import("@/stores/encouragementStore");
    const encStore = useEncouragementStore();
    if (!encStore.loaded) await encStore.fetchSettings();

    // F1：开关关闭 → 一律不显示，也不触发庆祝
    if (!encStore.settings.enabled) return;

    // 里程碑判定：全部目标完成（最高优先级，恒展示庆祝）/ 接近或超越历史最长连续
    let milestone: Awaited<ReturnType<typeof encApi.checkLongestStreakMilestone>> = null;
    try {
      const allComplete = await checkAllGoalsComplete();
      if (allComplete) {
        const enc = await encApi.randomCelebrationEncouragement(
          "complete_celebration",
        );
        if (enc) setPending(enc, true);
        return;
      }
      milestone = await encApi.checkLongestStreakMilestone();
    } catch {
      // 后端不可用时静默失败
    }

    // F3：按频率决定是否展示
    //   aggressive 每次完成 / normal 首任务+里程碑 / sparse 仅里程碑
    const freq = encStore.settings.frequency; // "aggressive" | "normal" | "sparse"
    const show =
      freq === "aggressive" ||
      (freq === "normal" && (first || Boolean(milestone))) ||
      (freq === "sparse" && Boolean(milestone));
    if (!show) return;

    try {
      if (milestone) {
        const enc = await encApi.randomLongestStreakEncouragement(
          "complete_first",
        );
        if (enc) {
          setPending(enc, false, true);
          return;
        }
      }
      if (first) {
        const streak = await encApi.getStreak();
        const enc = await encApi.randomEncouragementByStreak(
          streak.current_streak,
          "complete_first",
        );
        if (enc) {
          setPending(enc, false);
          return;
        }
      }
      const enc = await encApi.randomEncouragement("complete_normal");
      if (enc) setPending(enc, false);
    } catch {
      // 静默失败
    }
  }

  function setPending(
    enc: Encouragement,
    celebration: boolean,
    milestone = false,
  ) {
    pendingEncouragement.value = enc;
    isCelebration.value = celebration;
    isMilestone.value = milestone;
  }

  async function completeTask(input: CompleteTaskInput) {
    const updated = await taskApi.completeTask(input);
    await fetchAll();
    // 完成前是否为今日首个完成
    await maybeShowEncouragement(todayDoneCount.value === 1);
    return updated;
  }

  /** 补完成：只更新历史完成记录，同时触发鼓励语入口保持一致 */
  async function backfillTask(input: CompleteTaskInput) {
    const updated = await taskApi.backfillTask(input);
    await fetchAll();
    await maybeShowEncouragement(false);
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
    isMilestone.value = false;
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
    isMilestone,
    todayDoneCount,
    fetchAll,
    completeTask,
    backfillTask,
    skipTask,
    clearPendingEncouragement,
  };
});