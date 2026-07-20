<script setup lang="ts">
/**
 * CalendarView.vue - 容器组件版本
 *
 * 重构说明：
 * - 使用子组件：CalendarMonthView、CalendarWeekView、CalendarDayView、CalendarToolbar
 * - 使用composables：useCalendarNavigation、useCalendarData、useTaskBatch
 * - 保留：全局键盘监听、月视图创建弹窗、任务API调用
 * - 删除：已迁移到子组件的模板和逻辑
 */

import { ref, computed, onMounted, onBeforeUnmount, nextTick } from "vue";
import { useMessage, useDialog } from "naive-ui";
import {
  format,
  isToday,
  eachDayOfInterval,
  startOfWeek,
  endOfWeek,
} from "date-fns";
import { zhCN } from "date-fns/locale";
import type { CalendarTask } from "@/types";

// 子组件
import CalendarMonthView from "@/components/CalendarMonthView.vue";
import CalendarWeekView from "@/components/CalendarWeekView.vue";
import CalendarDayView from "@/components/CalendarDayView.vue";
import CalendarToolbar from "@/components/CalendarToolbar.vue";

// Composables
import { useCalendarNavigation } from "@/composables/useCalendarNavigation";
import { useCalendarData } from "@/composables/useCalendarData";
import { useTaskBatch } from "@/composables/useTaskBatch";

// API & Store
import * as taskApi from "@/api/task";
import { useGoalStore } from "@/stores/goalStore";

// ===== 使用 Composables =====

const {
  viewMode,
  currentDate,
  selectedDate,
  prevViewMode,
  prev,
  next,
  goToday,
  selectDay,
  syncFocusedDay,
  headerLabel,
  periodLabel,
  switchView,
} = useCalendarNavigation();

const { tasks, dailyLoadMap, loading, loadData, getDateRange } =
  useCalendarData(viewMode, currentDate);

const { selectedTaskIds, toggleSelect, selectAllVisible, clearSelection } =
  useTaskBatch();

const goalStore = useGoalStore();
const message = useMessage();
const dialog = useDialog();

// ===== 月视图焦点管理 =====

const focusedDay = ref(new Date());

function syncFocusedDayState() {
  focusedDay.value = new Date(currentDate.value);
}

// ===== 计算属性 =====

const goalOptions = computed(() =>
  goalStore.goals.map((g) => ({ label: g.name, value: g.id })),
);

const monthGrid = computed(() => {
  const { start, end } = getDateRange();
  return eachDayOfInterval({ start, end });
});

const weekGrid = computed(() => {
  const start = startOfWeek(currentDate.value, { weekStartsOn: 1 });
  const end = endOfWeek(currentDate.value, { weekStartsOn: 1 });
  return eachDayOfInterval({ start, end });
});

const tasksByDate = computed(() => {
  const map: Record<string, CalendarTask[]> = {};
  for (const t of tasks.value) {
    if (!t.plan_date) continue;
    if (!map[t.plan_date]) map[t.plan_date] = [];
    map[t.plan_date].push(t);
  }
  return map;
});

const selectedDayTasks = computed(() => {
  const key = format(selectedDate.value, "yyyy-MM-dd");
  return tasksByDate.value[key] || [];
});

const periodStats = computed(() => {
  const total = tasks.value.length;
  const done = tasks.value.filter((t) => t.status === "done").length;
  const pending = tasks.value.filter((t) => t.status === "pending").length;
  const overdue = tasks.value.filter((t) => t.is_overdue).length;
  const skipped = tasks.value.filter((t) => t.status === "skipped").length;
  const completionRate = total > 0 ? Math.round((done / total) * 100) : 0;
  return { total, done, pending, overdue, skipped, completionRate };
});

// ===== 任务操作 =====

async function quickComplete(task: CalendarTask) {
  try {
    const updated = await taskApi.completeTask({
      task_id: task.id,
      actual_qty: task.plan_qty,
    });
    goalStore.updateTaskLocally(updated);
    await goalStore.refreshProgressForGoalChain(updated.goal_id);
    await loadData();
    message.success("已完成");
  } catch (e) {
    message.error(String(e));
  }
}

function quickSkip(task: CalendarTask) {
  dialog.warning({
    title: "跳过任务",
    content: `确定跳过任务"${task.name}"？`,
    positiveText: "跳过",
    negativeText: "取消",
    onPositiveClick: async () => {
      try {
        const updated = await taskApi.skipTask(task.id);
        goalStore.updateTaskLocally(updated);
        await goalStore.refreshProgressForGoalChain(updated.goal_id);
        await loadData();
        message.info("已跳过");
      } catch (e) {
        message.error(String(e));
      }
    },
  });
}

// ===== 生命周期 =====

onMounted(async () => {
  window.addEventListener("keydown", onGlobalKeydown, true);
  await goalStore.fetchGoals();
  await loadData();
});

onBeforeUnmount(() => {
  window.removeEventListener("keydown", onGlobalKeydown, true);
});

// ===== 全局键盘导航 =====

function onGlobalKeydown(e: KeyboardEvent) {
  // 数字键 1/2/3：切换视图
  if (e.key === "1" || e.key === "2" || e.key === "3") {
    const targetMode = e.key === "1" ? "month" : e.key === "2" ? "week" : "day";
    if (viewMode.value !== targetMode) {
      e.preventDefault();
      switchView(targetMode);
    }
    return;
  }

  // Esc：返回上一视图
  if (e.key === "Escape") {
    if (prevViewMode.value) {
      e.preventDefault();
      switchView(prevViewMode.value);
    }
    return;
  }

  // 其他快捷键...
}

// ===== 事件处理函数 =====

function handlePrev() {
  prev();
  syncFocusedDayState();
}

function handleNext() {
  next();
  syncFocusedDayState();
}

function handleGoToday() {
  goToday();
  syncFocusedDayState();
}

function handleSelectDay(day: Date) {
  selectDay(day);
  switchView("day");
}

function handleCreateTask(day: Date, triggerElement: HTMLElement | null) {
  // TODO: 实现月视图创建任务弹窗
  console.log("创建任务:", day, triggerElement);
}

// ===== P2-3：拖拽改期事件处理 =====

async function handleMoveTask(
  taskId: string,
  newDate: string,
  oldDate: string,
) {
  try {
    // 调用API更新任务计划日期
    const updated = await taskApi.updateTask({
      task_id: taskId,
      plan_date: newDate,
    });

    message.success(`任务已从 ${oldDate} 移至 ${newDate}`);

    // 刷新数据
    await loadData();
    await goalStore.refreshProgressForGoalChain(updated.goal_id);
  } catch (err) {
    console.error("拖拽改期失败:", err);
    message.error("改期失败，请重试");
  }
}
</script>

<template>
  <div class="space-y-4 calendar-root">
    <!-- 工具栏 -->
    <CalendarToolbar
      :view-mode="viewMode"
      :header-label="headerLabel"
      :period-label="periodLabel"
      :period-stats="periodStats"
      :goal-options="goalOptions"
      :filter-active-count="0"
      :filter-has-condition="false"
      :loading="loading"
      @prev="handlePrev"
      @next="handleNext"
      @go-today="handleGoToday"
      @update:view-mode="switchView"
    />

    <!-- 月视图 -->
    <CalendarMonthView
      v-if="viewMode === 'month'"
      :current-date="currentDate"
      :month-grid="monthGrid"
      :tasks-by-date="tasksByDate"
      :daily-load-map="dailyLoadMap"
      :focused-day="focusedDay"
      :loading="loading"
      @select-day="handleSelectDay"
      @create-task="handleCreateTask"
      @move-task="handleMoveTask"
      @update:focused-day="focusedDay = $event"
    />

    <!-- 周视图 -->
    <CalendarWeekView
      v-else-if="viewMode === 'week'"
      :week-grid="weekGrid"
      :tasks-by-date="tasksByDate"
      :selected-task-ids="selectedTaskIds"
      :loading="loading"
      @toggle-select="toggleSelect"
      @select-all-week="() => {}"
      @clear-selection="clearSelection"
      @batch-complete="() => {}"
      @batch-skip="() => {}"
    />

    <!-- 日视图 -->
    <CalendarDayView
      v-else
      :selected-date="selectedDate"
      :tasks="selectedDayTasks"
      :selected-task-ids="selectedTaskIds"
      :goal-options="goalOptions"
      :loading="loading"
      @toggle-select="toggleSelect"
      @select-all="() => selectAllVisible(selectedDayTasks)"
      @clear-selection="clearSelection"
      @batch-complete="() => {}"
      @batch-skip="() => {}"
      @complete-task="quickComplete"
      @skip-task="quickSkip"
      @create-task="() => {}"
    />
  </div>
</template>

<style scoped>
.calendar-root {
  outline: none;
}
</style>
