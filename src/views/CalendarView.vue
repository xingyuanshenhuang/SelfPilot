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

import { ref, computed, onMounted, onBeforeUnmount, nextTick, watch } from "vue";
import {
  useMessage,
  useDialog,
  NModal,
  NCard,
  NSpace,
  NButton,
  NInput,
  NSelect,
  NInputNumber,
} from "naive-ui";
import {
  format,
  isToday,
  eachDayOfInterval,
  startOfWeek,
  endOfWeek,
  addDays,
  subDays,
  addWeeks,
  subWeeks,
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

// ===== 视图切换时清空选择状态 =====
// 避免跨视图残留选中状态导致显示混乱
watch(viewMode, () => {
  clearSelection();
});

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
  const key = format(currentDate.value, "yyyy-MM-dd");
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
  // 目标加载与数据加载独立 catch，避免任一失败阻塞组件挂载
  try {
    await goalStore.fetchGoals();
  } catch (e) {
    console.error("加载目标失败:", e);
  }
  try {
    await loadData();
  } catch (e) {
    console.error("初始加载日历数据失败:", e);
  }
});

onBeforeUnmount(() => {
  window.removeEventListener("keydown", onGlobalKeydown, true);
});

// ===== 全局键盘导航 =====

function onGlobalKeydown(e: KeyboardEvent) {
  // 上下文感知：输入框内不拦截快捷键
  const activeEl = document.activeElement as HTMLElement | null;
  if (activeEl) {
    const tag = activeEl.tagName;
    if (
      tag === "INPUT" ||
      tag === "TEXTAREA" ||
      activeEl.isContentEditable
    ) {
      return;
    }
    // Naive UI 组件（NSelect/NInputNumber 等）内部使用 role 属性
    const role = activeEl.getAttribute("role");
    if (role === "combobox" || role === "textbox" || role === "spinbutton") {
      return;
    }
    // 检查是否在 Naive UI 可编辑组件的容器内
    if (
      activeEl.closest(
        '[role="combobox"], [role="textbox"], [role="spinbutton"], .n-input, .n-base-selection, .n-input-number',
      )
    ) {
      return;
    }
  }

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
  currentDate.value = new Date(day);
  switchView("day");
}

// ===== 处理来自子组件的日期切换事件 =====

function handleChangeDate(newDate: Date) {
  // 用于处理日视图的日期切换
  currentDate.value = newDate;
  // 切换到日视图确保视图正确显示
  if (viewMode.value !== "day") {
    switchView("day");
  }
}

function handleChangeWeek(newStartDate: Date) {
  // 用于处理周视图的日期切换
  currentDate.value = newStartDate;
  // 切换到周视图确保视图正确显示
  if (viewMode.value !== "week") {
    switchView("week");
  }
}

// ===== 月视图创建任务弹窗 =====

const showCreateTaskModal = ref(false);
const createTaskDate = ref<Date>(new Date());
const createTaskName = ref("");
const createTaskGoalId = ref<string | null>(null);
const createTaskPlanQty = ref<number | null>(null);

function handleCreateTask(day: Date, _triggerElement: HTMLElement | null) {
  createTaskDate.value = day;
  createTaskName.value = "";
  createTaskGoalId.value = null;
  createTaskPlanQty.value = null;
  showCreateTaskModal.value = true;
}

async function submitCreateTask() {
  if (!createTaskName.value.trim()) {
    message.warning("请输入任务名称");
    return;
  }
  if (!createTaskGoalId.value) {
    message.warning("请选择目标");
    return;
  }
  try {
    const created = await taskApi.createTask({
      name: createTaskName.value,
      goal_id: createTaskGoalId.value,
      plan_qty: createTaskPlanQty.value ?? 1,
      plan_date: format(createTaskDate.value, "yyyy-MM-dd"),
    });
    goalStore.updateTaskLocally(created);
    await loadData();
    showCreateTaskModal.value = false;
    message.success("任务已创建");
  } catch (e) {
    message.error(String(e));
  }
}

// ===== 日视图快速创建任务 =====

async function handleCreateTaskInDay(input: {
  name: string;
  goalId: string | null;
  planQty: number | null;
}) {
  if (!input.goalId) {
    message.warning("请选择目标");
    return;
  }
  try {
    const created = await taskApi.createTask({
      name: input.name,
      goal_id: input.goalId,
      plan_qty: input.planQty ?? 1,
      plan_date: format(currentDate.value, "yyyy-MM-dd"),
    });
    goalStore.updateTaskLocally(created);
    await loadData();
    message.success("任务已创建");
  } catch (e) {
    message.error(String(e));
  }
}

// ===== 批量操作 =====

async function handleBatchComplete() {
  const ids = Array.from(selectedTaskIds.value);
  const tasksToComplete = tasks.value.filter(
    (t) =>
      ids.includes(t.id) &&
      t.status !== "done" &&
      t.status !== "skipped" &&
      !t.is_blocked,
  );
  if (tasksToComplete.length === 0) {
    message.warning("请选择可操作的任务");
    return;
  }
  let ok = 0;
  for (const t of tasksToComplete) {
    try {
      const updated = await taskApi.completeTask({
        task_id: t.id,
        actual_qty: t.plan_qty,
      });
      goalStore.updateTaskLocally(updated);
      await goalStore.refreshProgressForGoalChain(updated.goal_id);
      ok++;
    } catch (e) {
      console.error(`任务 ${t.name} 完成失败:`, e);
    }
  }
  await loadData();
  clearSelection();
  if (ok > 0) message.success(`已完成 ${ok} 个任务`);
}

function handleBatchSkip() {
  const ids = Array.from(selectedTaskIds.value);
  const tasksToSkip = tasks.value.filter(
    (t) => ids.includes(t.id) && t.status !== "done" && t.status !== "skipped",
  );
  if (tasksToSkip.length === 0) {
    message.warning("请选择可操作的任务");
    return;
  }
  dialog.warning({
    title: "批量跳过任务",
    content: `确定跳过选中的 ${tasksToSkip.length} 个任务？`,
    positiveText: "跳过",
    negativeText: "取消",
    onPositiveClick: async () => {
      let ok = 0;
      for (const t of tasksToSkip) {
        try {
          const updated = await taskApi.skipTask(t.id);
          goalStore.updateTaskLocally(updated);
          await goalStore.refreshProgressForGoalChain(updated.goal_id);
          ok++;
        } catch (e) {
          console.error(`任务 ${t.name} 跳过失败:`, e);
        }
      }
      await loadData();
      clearSelection();
      if (ok > 0) message.info(`已跳过 ${ok} 个任务`);
    },
  });
}

function handleSelectAllWeek() {
  const weekTasks: CalendarTask[] = [];
  for (const day of weekGrid.value) {
    const key = format(day, "yyyy-MM-dd");
    const dayTasks = tasksByDate.value[key] || [];
    weekTasks.push(...dayTasks);
  }
  selectAllVisible(weekTasks);
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
      @select-all-week="handleSelectAllWeek"
      @clear-selection="clearSelection"
      @batch-complete="handleBatchComplete"
      @batch-skip="handleBatchSkip"
      @change-week="handleChangeWeek"
    />

    <!-- 日视图 -->
    <CalendarDayView
      v-else
      :selected-date="currentDate"
      :tasks="selectedDayTasks"
      :selected-task-ids="selectedTaskIds"
      :goal-options="goalOptions"
      :loading="loading"
      @toggle-select="toggleSelect"
      @select-all="() => selectAllVisible(selectedDayTasks)"
      @clear-selection="clearSelection"
      @batch-complete="handleBatchComplete"
      @batch-skip="handleBatchSkip"
      @complete-task="quickComplete"
      @skip-task="quickSkip"
      @create-task="handleCreateTaskInDay"
      @change-date="handleChangeDate"
    />

    <!-- 月视图创建任务弹窗 -->
    <NModal
      v-model:show="showCreateTaskModal"
      preset="card"
      :title="`创建任务 — ${format(createTaskDate, 'yyyy-MM-dd EEEE', { locale: zhCN })}`"
      style="width: 420px"
      :mask-closable="true"
    >
      <NSpace vertical :size="16">
        <div>
          <label class="block text-xs text-gray-500 mb-1">任务名称</label>
          <NInput
            v-model:value="createTaskName"
            placeholder="输入任务名"
            @keydown.enter="submitCreateTask"
          />
        </div>
        <div>
          <label class="block text-xs text-gray-500 mb-1">选择目标</label>
          <NSelect
            v-model:value="createTaskGoalId"
            :options="goalOptions"
            placeholder="选择目标"
            :max-tag-count="1"
          />
        </div>
        <div>
          <label class="block text-xs text-gray-500 mb-1">数量（可选）</label>
          <NInputNumber
            v-model:value="createTaskPlanQty"
            placeholder="1"
            :min="1"
            style="width: 100%"
          />
        </div>
      </NSpace>

      <template #footer>
        <NSpace justify="end">
          <NButton @click="showCreateTaskModal = false">取消</NButton>
          <NButton
            type="primary"
            :disabled="!createTaskName.trim() || !createTaskGoalId"
            @click="submitCreateTask"
          >
            创建
          </NButton>
        </NSpace>
      </template>
    </NModal>
  </div>
</template>

<style scoped>
.calendar-root {
  outline: none;
}
</style>
