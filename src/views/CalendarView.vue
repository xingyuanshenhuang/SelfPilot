<script setup lang="ts">
import { ref, computed, onMounted, watch } from "vue";
import {
  NCard,
  NButton,
  NSpace,
  NRadioGroup,
  NRadioButton,
  NTag,
  NEmpty,
  NCheckbox,
  NTooltip,
  useMessage,
  useDialog,
} from "naive-ui";
import { Icon } from "@iconify/vue";
import {
  format,
  addMonths,
  subMonths,
  addWeeks,
  subWeeks,
  addDays,
  subDays,
  startOfMonth,
  endOfMonth,
  startOfWeek,
  endOfWeek,
  eachDayOfInterval,
  isSameDay,
  isSameMonth,
  isToday,
} from "date-fns";
import { zhCN } from "date-fns/locale";
import * as taskApi from "@/api/task";
import { useGoalStore } from "@/stores/goalStore";
import type { CalendarTask } from "@/types";
import { STATUS_META } from "@/types";

type ViewMode = "month" | "week" | "day";

const goalStore = useGoalStore();
const message = useMessage();
const dialog = useDialog();

const viewMode = ref<ViewMode>("month");
const currentDate = ref(new Date());
const selectedDate = ref(new Date());
const tasks = ref<CalendarTask[]>([]);
const loading = ref(false);

// 批量操作选中
const selectedTaskIds = ref<Set<string>>(new Set());

const weekDays = ["一", "二", "三", "四", "五", "六", "日"];

onMounted(async () => {
  await goalStore.fetchGoals();
  await loadData();
});

watch([viewMode, currentDate], () => {
  loadData();
});

async function loadData() {
  loading.value = true;
  try {
    const { start, end } = getDateRange();
    const startStr = format(start, "yyyy-MM-dd");
    const endStr = format(end, "yyyy-MM-dd");
    tasks.value = await taskApi.listTasksByDateRange(startStr, endStr);
  } catch (e) {
    message.error(String(e));
  } finally {
    loading.value = false;
  }
}

/** 根据视图模式获取查询日期范围（多查一周以填充首尾） */
function getDateRange(): { start: Date; end: Date } {
  if (viewMode.value === "month") {
    const start = startOfWeek(startOfMonth(currentDate.value), {
      weekStartsOn: 1,
    });
    const end = endOfWeek(endOfMonth(currentDate.value), { weekStartsOn: 1 });
    return { start, end };
  }
  if (viewMode.value === "week") {
    const start = startOfWeek(currentDate.value, { weekStartsOn: 1 });
    const end = endOfWeek(currentDate.value, { weekStartsOn: 1 });
    return { start, end };
  }
  // day 模式：单日查询
  return { start: currentDate.value, end: currentDate.value };
}

/** 月视图网格日期 */
const monthGrid = computed(() => {
  const { start, end } = getDateRange();
  return eachDayOfInterval({ start, end });
});

/** 周视图日期 */
const weekGrid = computed(() => {
  const start = startOfWeek(currentDate.value, { weekStartsOn: 1 });
  const end = endOfWeek(currentDate.value, { weekStartsOn: 1 });
  return eachDayOfInterval({ start, end });
});

/** 按日期分组任务 */
const tasksByDate = computed(() => {
  const map: Record<string, CalendarTask[]> = {};
  for (const t of tasks.value) {
    if (!t.plan_date) continue;
    if (!map[t.plan_date]) map[t.plan_date] = [];
    map[t.plan_date].push(t);
  }
  return map;
});

function getTasksOfDay(day: Date): CalendarTask[] {
  const key = format(day, "yyyy-MM-dd");
  return tasksByDate.value[key] || [];
}

/** 当日完成统计 */
function getDayStats(day: Date) {
  const list = getTasksOfDay(day);
  const total = list.length;
  const done = list.filter((t) => t.status === "done").length;
  const partial = list.filter((t) => t.status === "partial").length;
  const overdue = list.filter((t) => t.is_overdue).length;
  return { total, done, partial, overdue };
}

// 日期导航
function prev() {
  if (viewMode.value === "month")
    currentDate.value = subMonths(currentDate.value, 1);
  else if (viewMode.value === "week")
    currentDate.value = subWeeks(currentDate.value, 1);
  else currentDate.value = subDays(currentDate.value, 1);
}
function next() {
  if (viewMode.value === "month")
    currentDate.value = addMonths(currentDate.value, 1);
  else if (viewMode.value === "week")
    currentDate.value = addWeeks(currentDate.value, 1);
  else currentDate.value = addDays(currentDate.value, 1);
}
function goToday() {
  currentDate.value = new Date();
  selectedDate.value = new Date();
}

function selectDay(day: Date) {
  selectedDate.value = day;
  if (viewMode.value === "month") {
    // 月视图点击切换到 day 模式查看详情
    currentDate.value = day;
    viewMode.value = "day";
  }
}

const headerLabel = computed(() => {
  if (viewMode.value === "month")
    return format(currentDate.value, "yyyy 年 M 月");
  if (viewMode.value === "week") {
    const start = startOfWeek(currentDate.value, { weekStartsOn: 1 });
    const end = endOfWeek(currentDate.value, { weekStartsOn: 1 });
    return `${format(start, "MM-dd")} ~ ${format(end, "MM-dd")}`;
  }
  return format(currentDate.value, "yyyy 年 M 月 d 日 EEEE", { locale: zhCN });
});

// 选中日期的任务列表
const selectedDayTasks = computed(() => getTasksOfDay(selectedDate.value));

// 批量选择
function toggleSelect(taskId: string, checked: boolean) {
  if (checked) selectedTaskIds.value.add(taskId);
  else selectedTaskIds.value.delete(taskId);
}

function selectAllVisible() {
  for (const t of selectedDayTasks.value) {
    if (t.status !== "done" && t.status !== "skipped" && !t.is_blocked) {
      selectedTaskIds.value.add(t.id);
    }
  }
}
function clearSelection() {
  selectedTaskIds.value.clear();
}

async function batchComplete() {
  const ids = Array.from(selectedTaskIds.value);
  // 防御性过滤阻塞任务，避免后端校验报错
  const validIds = ids.filter(
    (id) => !tasks.value.find((t) => t.id === id)?.is_blocked,
  );
  if (validIds.length === 0) {
    message.warning("没有可完成的任务（前置依赖未就绪）");
    return;
  }
  let ok = 0;
  for (const id of validIds) {
    try {
      const task = tasks.value.find((t) => t.id === id);
      if (!task) continue;
      await taskApi.completeTask({ task_id: id, actual_qty: task.plan_qty });
      ok++;
    } catch (e) {
      message.error(`任务 ${id} 完成失败: ${String(e)}`);
    }
  }
  if (ok > 0) {
    message.success(`已批量完成 ${ok} 个任务`);
    await goalStore.fetchProgresses();
    await loadData();
    clearSelection();
  }
}

async function batchSkip() {
  const ids = Array.from(selectedTaskIds.value);
  if (ids.length === 0) {
    message.warning("请先选择任务");
    return;
  }
  let ok = 0;
  for (const id of ids) {
    try {
      await taskApi.skipTask(id);
      ok++;
    } catch (e) {
      message.error(`任务 ${id} 跳过失败: ${String(e)}`);
    }
  }
  if (ok > 0) {
    message.success(`已批量跳过 ${ok} 个任务`);
    await goalStore.fetchProgresses();
    await loadData();
    clearSelection();
  }
}

async function quickComplete(task: CalendarTask) {
  try {
    await taskApi.completeTask({ task_id: task.id, actual_qty: task.plan_qty });
    await goalStore.fetchProgresses();
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
        await taskApi.skipTask(task.id);
        await goalStore.fetchProgresses();
        await loadData();
        message.info("已跳过");
      } catch (e) {
        message.error(String(e));
      }
    },
  });
}
</script>

<template>
  <div class="space-y-4">
    <!-- 顶部工具栏 -->
    <NCard :bordered="false" size="small">
      <div class="flex items-center justify-between flex-wrap gap-2">
        <NSpace align="center">
          <NButton quaternary circle @click="prev">
            <template #icon><Icon icon="mdi:chevron-left" /></template>
          </NButton>
          <span class="text-lg font-semibold min-w-[180px] text-center">
            {{ headerLabel }}
          </span>
          <NButton quaternary circle @click="next">
            <template #icon><Icon icon="mdi:chevron-right" /></template>
          </NButton>
          <NButton size="small" @click="goToday">今天</NButton>
        </NSpace>
        <NRadioGroup v-model:value="viewMode" size="small">
          <NRadioButton value="month">月</NRadioButton>
          <NRadioButton value="week">周</NRadioButton>
          <NRadioButton value="day">日</NRadioButton>
        </NRadioGroup>
      </div>
    </NCard>

    <!-- 月视图 -->
    <NCard v-if="viewMode === 'month'" :bordered="false">
      <div
        class="grid grid-cols-7 gap-1 text-center text-xs text-gray-500 mb-1"
      >
        <div
          v-for="d in weekDays"
          :key="d"
          class="flex items-center justify-center py-1"
        >
          {{ d }}
        </div>
      </div>
      <div class="grid grid-cols-7 gap-1">
        <div
          v-for="day in monthGrid"
          :key="day.toISOString()"
          class="calendar-cell flex flex-col items-center justify-center min-h-[88px] p-1.5 rounded border cursor-pointer transition-colors"
          :class="{
            'bg-gray-50': !isSameMonth(day, currentDate),
            'border-brand-500 border-2': isToday(day),
            'hover:bg-blue-50': true,
          }"
          @click="selectDay(day)"
        >
          <div
            class="text-center text-sm font-semibold"
            :class="{
              'text-brand-600 font-bold': isToday(day),
              'text-gray-400': !isSameMonth(day, currentDate),
            }"
          >
            {{ format(day, "d") }}
          </div>
          <div v-if="getDayStats(day).total > 0" class="mt-1 space-y-0.5">
            <div class="flex items-center gap-1 text-[10px]">
              <NTag size="tiny" :bordered="false" type="success" round
                >{{ getDayStats(day).done }}/{{ getDayStats(day).total }}</NTag
              >
              <NTag
                v-if="getDayStats(day).overdue > 0"
                size="tiny"
                :bordered="false"
                type="error"
                round
                >{{ getDayStats(day).overdue }}逾期</NTag
              >
            </div>
            <!-- 任务点 -->
            <div class="flex flex-wrap gap-0.5">
              <div
                v-for="t in getTasksOfDay(day).slice(0, 4)"
                :key="t.id"
                class="w-1.5 h-1.5 rounded-full"
                :style="{ backgroundColor: STATUS_META[t.status].color }"
                :title="t.name"
              />
            </div>
          </div>
        </div>
      </div>
    </NCard>

    <!-- 周视图 -->
    <NCard v-else-if="viewMode === 'week'" :bordered="false">
      <div class="grid grid-cols-7 gap-2">
        <div
          v-for="day in weekGrid"
          :key="day.toISOString()"
          class="min-h-[280px] p-2 rounded border"
          :class="{
            'border-brand-500 border-2': isToday(day),
            'bg-blue-50': isSameDay(day, selectedDate),
          }"
        >
          <div
            class="flex items-center justify-center text-center text-sm font-medium pb-1 border-b"
            :class="{ 'text-brand-600': isToday(day) }"
          >
            {{ format(day, "E d", { locale: zhCN }) }}
          </div>
          <div class="mt-1 space-y-1">
            <div
              v-for="t in getTasksOfDay(day)"
              :key="t.id"
              class="text-xs p-1 rounded flex items-center gap-1"
              :class="{
                'bg-red-50': t.is_overdue,
                'bg-green-50': t.status === 'done',
              }"
            >
              <Icon
                :icon="STATUS_META[t.status].icon"
                :color="STATUS_META[t.status].color"
                width="12"
              />
              <span class="flex-1 truncate">{{ t.name }}</span>
            </div>
            <div
              v-if="getTasksOfDay(day).length === 0"
              class="flex items-center justify-center text-[10px] text-gray-300 pt-2"
            >
              无
            </div>
          </div>
        </div>
      </div>
    </NCard>

    <!-- 日视图 -->
    <div v-else class="space-y-3">
      <NCard :bordered="false">
        <template #header>
          <div class="flex items-center gap-2">
            <Icon icon="mdi:calendar-today" width="20" class="text-brand-500" />
            <span>{{
              format(selectedDate, "yyyy-MM-dd EEEE", { locale: zhCN })
            }}</span>
            <NTag v-if="isToday(selectedDate)" type="info" size="small" round
              >今天</NTag
            >
          </div>
        </template>
        <template #header-extra>
          <NSpace v-if="selectedDayTasks.length > 0" :size="4">
            <NButton size="small" @click="selectAllVisible">全选</NButton>
            <NButton size="small" @click="clearSelection">清空</NButton>
            <NButton size="small" type="primary" @click="batchComplete">
              <template #icon><Icon icon="mdi:playlist-check" /></template>
              批量完成
            </NButton>
            <NButton size="small" type="warning" @click="batchSkip">
              <template #icon><Icon icon="mdi:skip-next" /></template>
              批量跳过
            </NButton>
          </NSpace>
        </template>

        <div v-if="selectedDayTasks.length > 0" class="space-y-1">
          <div
            v-for="t in selectedDayTasks"
            :key="t.id"
            class="flex items-center gap-2 px-3 py-2 rounded hover:bg-gray-50"
            :class="{
              'bg-red-50': t.is_overdue,
            }"
          >
            <NCheckbox
              v-if="t.status !== 'done' && t.status !== 'skipped'"
              :checked="selectedTaskIds.has(t.id)"
              :disabled="t.is_blocked"
              @update:checked="(v) => toggleSelect(t.id, v)"
            />
            <div
              class="flex-1 flex items-center gap-2 min-w-0"
              :class="{ 'opacity-40': t.is_blocked }"
            >
              <Icon
                :icon="STATUS_META[t.status].icon"
                :color="STATUS_META[t.status].color"
                width="18"
              />
              <div class="flex-1 min-w-0">
                <div class="flex items-center gap-2">
                  <NTooltip v-if="t.is_blocked" placement="top">
                    <template #trigger>
                      <Icon
                        icon="mdi:lock-outline"
                        class="text-gray-400 shrink-0"
                        width="14"
                      />
                    </template>
                    {{
                      t.blocked_by_names
                        ? `前置任务未完成：${t.blocked_by_names}`
                        : "前置任务未完成，暂不可标记完成"
                    }}
                  </NTooltip>
                  <span
                    class="text-sm font-medium truncate"
                    :class="{
                      'line-through text-gray-400': t.status === 'done',
                    }"
                    >{{ t.name }}</span
                  >
                  <NTag size="tiny" :bordered="false" type="info">{{
                    t.goal_name
                  }}</NTag>
                  <NTag
                    v-if="t.is_overdue"
                    size="tiny"
                    type="error"
                    :bordered="false"
                    >逾期</NTag
                  >
                </div>
                <div class="text-xs text-gray-500 mt-0.5">
                  {{ t.actual_qty }}/{{ t.plan_qty }}{{ t.unit }}
                </div>
              </div>
            </div>
            <NSpace
              v-if="t.status !== 'done' && t.status !== 'skipped'"
              :size="4"
            >
              <NTooltip :disabled="!t.is_blocked" placement="top">
                <template #trigger>
                  <NButton
                    size="tiny"
                    type="primary"
                    :disabled="t.is_blocked"
                    @click="quickComplete(t)"
                  >
                    <template #icon>
                      <Icon icon="mdi:check" width="16" />
                    </template>
                    完成
                  </NButton>
                </template>
                {{
                  t.blocked_by_names
                    ? `前置任务未完成：${t.blocked_by_names}`
                    : "前置任务未完成"
                }}
              </NTooltip>
              <NButton size="tiny" type="default" @click="quickSkip(t)">
                <template #icon>
                  <Icon icon="mdi:skip-next" width="16" />
                </template>
                跳过
              </NButton>
            </NSpace>
            <NTag
              v-else-if="t.status === 'done'"
              size="tiny"
              type="success"
              :bordered="false"
              >已完成</NTag
            >
            <NTag v-else size="tiny" type="default" :bordered="false"
              >已跳过</NTag
            >
          </div>
        </div>
        <NEmpty v-else description="当日无任务" />
      </NCard>
    </div>
  </div>
</template>

<style scoped>
.calendar-cell:hover {
  transform: translateY(-1px);
}
</style>
