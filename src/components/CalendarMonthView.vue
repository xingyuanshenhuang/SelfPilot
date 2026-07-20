<script setup lang="ts">
import { ref, computed, nextTick, type ComponentPublicInstance } from "vue";
import { NCard, NSpin, NPopover, NTag, NButton } from "naive-ui";
import { Icon } from "@iconify/vue";
import { format, isSameMonth, isToday, isSameDay } from "date-fns";
import { zhCN } from "date-fns/locale";
import type { CalendarTask, DailyLoad } from "@/types";
import { STATUS_META } from "@/types";

// ===== Props & Emits =====

interface MonthViewProps {
  /** 当前显示的月份 */
  currentDate: Date;
  /** 月视图网格日期 */
  monthGrid: Date[];
  /** 按日期分组的任务 */
  tasksByDate: Record<string, CalendarTask[]>;
  /** 每日负载数据 */
  dailyLoadMap: Record<string, DailyLoad>;
  /** 当前焦点日期（roving tabindex） */
  focusedDay: Date;
  /** 加载状态 */
  loading: boolean;
}

const props = defineProps<MonthViewProps>();

interface MonthViewEmits {
  /** 点击日期单元格 */
  (e: "select-day", day: Date): void;
  /** 双击创建任务 */
  (e: "create-task", day: Date, triggerElement: HTMLElement | null): void;
  /** 更新焦点日期 */
  (e: "update:focusedDay", day: Date): void;
  /** 拖拽任务到新日期 */
  (e: "move-task", taskId: string, newDate: string, oldDate: string): void;
}

const emit = defineEmits<MonthViewEmits>();

// ===== P2-3：拖拽改期状态 =====

const draggedTask = ref<CalendarTask | null>(null);
const dragOverDay = ref<Date | null>(null);

// ===== 负载参数 =====

const LOAD_THRESHOLD_MEDIUM = 4;
const LOAD_THRESHOLD_HIGH = 7;
const LOAD_THRESHOLD_EXTREME = 11;
const LOAD_MAX_CAPACITY = 12;
const RING_RADIUS = 10;
const RING_CIRCUMFERENCE = 2 * Math.PI * RING_RADIUS;

const LOAD_COLORS: Record<"low" | "medium" | "high" | "extreme", string> = {
  low: "#22c55e",
  medium: "#f59e0b",
  high: "#ef4444",
  extreme: "#9333ea",
};

const weekDays = ["一", "二", "三", "四", "五", "六", "日"];

// ===== 月视图特有逻辑 =====

/** 月视图单元格 DOM 引用（用于 roving tabindex 焦点管理） */
const monthCellRefs = ref<Map<string, HTMLElement>>(new Map());

function setMonthCellRef(
  el: Element | ComponentPublicInstance | null,
  day: Date,
) {
  const key = format(day, "yyyy-MM-dd");
  const map = monthCellRefs.value as Map<string, HTMLElement>;
  if (el && el instanceof HTMLElement) {
    map.set(key, el);
  } else {
    map.delete(key);
  }
}

/** 月视图单元格是否为当前焦点格（roving tabindex） */
function isMonthCellFocusable(day: Date): boolean {
  const focusedInGrid = props.monthGrid.some((d) =>
    isSameDay(d, props.focusedDay),
  );
  if (focusedInGrid) return isSameDay(day, props.focusedDay);
  return isToday(day);
}

/** 月视图日期网格键盘导航
 * - 仅处理 Enter/Space 阻止 button 默认 click（不进入日视图）
 * - 方向键/Home/End 由父组件全局处理
 */
function onMonthCellKeydown(e: KeyboardEvent) {
  if (e.key === "Enter" || e.key === " ") {
    e.preventDefault();
  }
}

// ===== 任务统计 =====

function getTasksOfDay(day: Date): CalendarTask[] {
  const key = format(day, "yyyy-MM-dd");
  return props.tasksByDate[key] || [];
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

/** 预计算所有日期的统计 */
const dayStatsMap = computed(() => {
  const map: Record<string, ReturnType<typeof getDayStats>> = {};
  for (const key of Object.keys(props.tasksByDate)) {
    const list = props.tasksByDate[key];
    const total = list.length;
    const done = list.filter((t) => t.status === "done").length;
    const partial = list.filter((t) => t.status === "partial").length;
    const overdue = list.filter((t) => t.is_overdue).length;
    map[key] = { total, done, partial, overdue };
  }
  return map;
});

const EMPTY_STATS = { total: 0, done: 0, partial: 0, overdue: 0 };

function getDayStatsCached(day: Date) {
  const key = format(day, "yyyy-MM-dd");
  return dayStatsMap.value[key] || EMPTY_STATS;
}

// ===== 负载计算 =====

function getLoadLevel(
  day: Date,
): "none" | "low" | "medium" | "high" | "extreme" {
  const key = format(day, "yyyy-MM-dd");
  const load = props.dailyLoadMap[key];
  if (!load || load.total_tasks === 0) return "none";
  if (load.total_tasks >= LOAD_THRESHOLD_EXTREME) return "extreme";
  if (load.total_tasks >= LOAD_THRESHOLD_HIGH) return "high";
  if (load.total_tasks >= LOAD_THRESHOLD_MEDIUM) return "medium";
  return "low";
}

function getLoadOfDay(day: Date): string {
  const key = format(day, "yyyy-MM-dd");
  const load = props.dailyLoadMap[key];
  if (!load || load.total_tasks === 0) return "无任务";
  const detail = load.by_goal
    .map((g) => `${g.goal_name}×${g.task_count}`)
    .join(", ");
  return `${load.total_tasks} 个任务（${detail}）`;
}

function getLoadColor(day: Date): string {
  const level = getLoadLevel(day);
  if (level === "none") return "#d1d5db";
  return LOAD_COLORS[level];
}

function getLoadCount(day: Date): number {
  const key = format(day, "yyyy-MM-dd");
  return props.dailyLoadMap[key]?.total_tasks ?? 0;
}

function getLoadPercentage(day: Date): number {
  const count = getLoadCount(day);
  return Math.min(100, Math.round((count / LOAD_MAX_CAPACITY) * 100));
}

function getRingDashArray(day: Date): string {
  const pct = getLoadPercentage(day);
  const filled = (pct / 100) * RING_CIRCUMFERENCE;
  return `${filled} ${RING_CIRCUMFERENCE}`;
}

// ===== ARIA 标签 =====

function getDayAriaLabel(day: Date): string {
  const dateStr = format(day, "yyyy 年 M 月 d 日 EEEE", { locale: zhCN });
  const stats = getDayStatsCached(day);
  if (stats.total === 0) return `${dateStr}，无任务`;
  const parts = [
    `${dateStr}，共 ${stats.total} 个任务`,
    `已完成 ${stats.done}`,
  ];
  if (stats.overdue > 0) parts.push(`${stats.overdue} 个逾期`);
  return parts.join("，");
}

// ===== 事件处理 =====

function handleClick(day: Date) {
  emit("select-day", day);
}

function handleDblclick(day: Date, event: MouseEvent) {
  const triggerElement = event.currentTarget as HTMLElement | null;
  emit("create-task", day, triggerElement);
}

// ===== P2-3：拖拽改期 =====

/** 拖拽开始：记录任务，设置视觉反馈 */
function handleDragStart(e: DragEvent, task: CalendarTask) {
  draggedTask.value = task;

  if (e.dataTransfer) {
    e.dataTransfer.effectAllowed = "move";
    e.dataTransfer.setData("text/plain", task.id);

    // 创建自定义拖拽预览
    const dragImage = document.createElement("div");
    dragImage.textContent = task.name;
    dragImage.className = "drag-preview";
    dragImage.style.cssText = `
      position: absolute;
      top: -1000px;
      background: white;
      padding: 4px 8px;
      border-radius: 4px;
      font-size: 12px;
      box-shadow: 0 2px 8px rgba(0,0,0,0.15);
      white-space: nowrap;
    `;
    document.body.appendChild(dragImage);
    e.dataTransfer.setDragImage(dragImage, 0, 0);
    setTimeout(() => document.body.removeChild(dragImage), 0);
  }

  // 源元素视觉反馈
  const target = e.target as HTMLElement;
  target.classList.add("opacity-40");
}

/** 拖拽结束：清理状态 */
function handleDragEnd(e: DragEvent) {
  draggedTask.value = null;
  dragOverDay.value = null;

  // 移除源元素视觉反馈
  const target = e.target as HTMLElement;
  target.classList.remove("opacity-40");
}

/** 拖拽悬停在日期单元格上 */
function handleDragOver(e: DragEvent, day: Date) {
  e.preventDefault();

  if (e.dataTransfer) {
    e.dataTransfer.dropEffect = "move";
  }

  dragOverDay.value = day;
}

/** 拖拽离开日期单元格 */
function handleDragLeave(day: Date) {
  if (dragOverDay.value && isSameDay(day, dragOverDay.value)) {
    dragOverDay.value = null;
  }
}

/** 放置任务到新日期 */
function handleDrop(e: DragEvent, targetDay: Date) {
  e.preventDefault();
  dragOverDay.value = null;

  if (!draggedTask.value) return;

  const oldDate = draggedTask.value.plan_date || "";
  const newDate = format(targetDay, "yyyy-MM-dd");

  // 检查是否移动到不同日期
  if (oldDate === newDate) {
    return; // 同一天，不触发移动
  }

  // 触发移动事件，由父组件处理API调用和消息提示
  emit("move-task", draggedTask.value.id, newDate, oldDate);

  // 清理状态
  draggedTask.value = null;
}

/** 判断日期单元格是否为当前拖拽目标 */
function isDragTarget(day: Date): boolean {
  return dragOverDay.value ? isSameDay(day, dragOverDay.value) : false;
}

// ===== 暴露给父组件的接口 =====

defineExpose({
  monthCellRefs,
  isMonthCellFocusable,
  getDayStatsCached,
});
</script>

<template>
  <NCard :bordered="false">
    <NSpin :show="loading">
      <!-- 星期标题行 -->
      <div
        class="grid grid-cols-7 gap-1 text-center text-xs text-gray-500 mb-1"
        role="presentation"
      >
        <div
          v-for="d in weekDays"
          :key="d"
          class="flex items-center justify-center py-1"
        >
          {{ d }}
        </div>
      </div>

      <!-- 日期网格 -->
      <div
        class="grid grid-cols-7 gap-1"
        role="grid"
        aria-label="月视图日期网格"
      >
        <NPopover
          v-for="day in monthGrid"
          :key="day.toISOString()"
          trigger="hover"
          :delay="300"
          placement="bottom"
          :width="280"
          :disabled="getTasksOfDay(day).length === 0"
        >
          <template #trigger>
            <div
              :ref="(el) => setMonthCellRef(el, day)"
              class="calendar-cell relative flex flex-col items-center justify-center min-h-[88px] p-1.5 rounded border cursor-pointer transition-all duration-200"
              role="button"
              :tabindex="isMonthCellFocusable(day) ? 0 : -1"
              :aria-label="getDayAriaLabel(day)"
              :aria-current="isToday(day) ? 'date' : undefined"
              :class="{
                'bg-gray-50': !isSameMonth(day, currentDate) && !isToday(day),
                'bg-brand-100 border-brand-500 border-2 shadow-md ring-2 ring-brand-300 ring-offset-1':
                  isToday(day),
                'ring-2 ring-brand-400 ring-offset-1 bg-blue-50':
                  isMonthCellFocusable(day) && !isToday(day),
                'hover:bg-blue-50': !isToday(day) && !isMonthCellFocusable(day),
                'hover:bg-brand-200': isToday(day),
                'bg-green-50':
                  isSameMonth(day, currentDate) &&
                  !isToday(day) &&
                  !isMonthCellFocusable(day) &&
                  getLoadLevel(day) === 'low',
                'bg-yellow-50':
                  isSameMonth(day, currentDate) &&
                  !isToday(day) &&
                  !isMonthCellFocusable(day) &&
                  getLoadLevel(day) === 'medium',
                'bg-red-50':
                  isSameMonth(day, currentDate) &&
                  !isToday(day) &&
                  !isMonthCellFocusable(day) &&
                  getLoadLevel(day) === 'high',
                'bg-purple-50':
                  isSameMonth(day, currentDate) &&
                  !isToday(day) &&
                  !isMonthCellFocusable(day) &&
                  getLoadLevel(day) === 'extreme',
                'ring-2 ring-brand-500 bg-brand-50': isDragTarget(day),
              }"
              @click="handleClick(day)"
              @dblclick.stop="handleDblclick(day, $event)"
              @keydown="onMonthCellKeydown($event)"
              @dragover.prevent="handleDragOver($event, day)"
              @dragleave="handleDragLeave(day)"
              @drop="handleDrop($event, day)"
            >
              <!-- 日期数字 -->
              <div
                class="text-center text-sm font-semibold"
                :class="{
                  'text-brand-600 font-bold text-base': isToday(day),
                  'text-gray-400':
                    !isSameMonth(day, currentDate) && !isToday(day),
                }"
              >
                {{ format(day, "d") }}
              </div>

              <!-- 负载圆环指示器 -->
              <div
                v-if="getLoadLevel(day) !== 'none'"
                class="load-ring absolute top-1 right-1 w-[26px] h-[26px]"
                :title="`当日负载: ${getLoadOfDay(day)}`"
                :aria-label="`当日负载等级: ${getLoadLevel(day)}, ${getLoadOfDay(day)}`"
                role="img"
              >
                <svg
                  width="26"
                  height="26"
                  viewBox="0 0 26 26"
                  style="transform: rotate(-90deg); transform-origin: center"
                  aria-hidden="true"
                >
                  <circle
                    cx="13"
                    cy="13"
                    r="10"
                    fill="none"
                    stroke="#f3f4f6"
                    stroke-width="3"
                  />
                  <circle
                    cx="13"
                    cy="13"
                    r="10"
                    fill="none"
                    :stroke="getLoadColor(day)"
                    stroke-width="3"
                    stroke-linecap="round"
                    :stroke-dasharray="getRingDashArray(day)"
                  />
                </svg>
                <span
                  class="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 text-[10px] font-bold leading-none tabular-nums"
                  :style="{ color: getLoadColor(day) }"
                  >{{ getLoadCount(day) }}</span
                >
              </div>

              <!-- 任务统计标签 -->
              <div
                v-if="getDayStatsCached(day).total > 0"
                class="mt-1 space-y-0.5"
              >
                <div class="flex items-center gap-1 text-[10px]">
                  <NTag size="tiny" :bordered="false" type="success" round
                    >{{ getDayStatsCached(day).done }}/{{
                      getDayStatsCached(day).total
                    }}</NTag
                  >
                  <NTag
                    v-if="getDayStatsCached(day).overdue > 0"
                    size="tiny"
                    :bordered="false"
                    type="error"
                    round
                    >{{ getDayStatsCached(day).overdue }}逾期</NTag
                  >
                </div>

                <!-- 任务圆点 -->
                <div class="flex flex-wrap gap-0.5" aria-hidden="true">
                  <div
                    v-for="t in getTasksOfDay(day).slice(0, 4)"
                    :key="t.id"
                    class="w-1.5 h-1.5 rounded-full cursor-move transition-opacity"
                    :class="{
                      'opacity-50': t.status === 'skipped',
                    }"
                    :style="{ backgroundColor: STATUS_META[t.status].color }"
                    :title="t.name"
                    draggable="true"
                    :aria-label="`拖拽任务：${t.name}`"
                    @dragstart="handleDragStart($event, t)"
                    @dragend="handleDragEnd($event)"
                  />
                  <span
                    v-if="getTasksOfDay(day).length > 4"
                    class="text-[9px] text-gray-400"
                    >+{{ getTasksOfDay(day).length - 4 }}</span
                  >
                </div>
              </div>
            </div>
          </template>

          <!-- hover 预览弹层 -->
          <div class="space-y-1" role="list" aria-label="当日任务预览">
            <div
              class="text-xs text-gray-500 mb-1 flex items-center gap-2 pb-1 border-b border-gray-100"
            >
              <Icon icon="mdi:speedometer" width="12" />
              负载：{{ getLoadOfDay(day) }}
            </div>
            <div
              v-for="t in getTasksOfDay(day).slice(0, 8)"
              :key="t.id"
              class="flex items-center gap-2 text-xs py-0.5"
              role="listitem"
            >
              <Icon
                :icon="STATUS_META[t.status].icon"
                :color="STATUS_META[t.status].color"
                width="14"
              />
              <span
                class="flex-1 truncate"
                :class="{
                  'line-through text-gray-400':
                    t.status === 'done' || t.status === 'skipped',
                }"
                >{{ t.name }}</span
              >
              <span class="text-[10px] text-gray-400 shrink-0">{{
                t.goal_name
              }}</span>
            </div>
            <NButton
              v-if="getTasksOfDay(day).length > 8"
              size="tiny"
              block
              @click="handleClick(day)"
            >
              查看全部 ({{ getTasksOfDay(day).length }})
            </NButton>
          </div>
        </NPopover>
      </div>
    </NSpin>
  </NCard>
</template>

<style scoped>
.calendar-cell:hover {
  transform: translateY(-1px);
}
.calendar-cell:focus-visible {
  outline: 2px solid #3478f6;
  outline-offset: 2px;
}
</style>
