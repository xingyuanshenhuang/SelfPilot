<script setup lang="ts">
import { computed } from "vue";
import {
  NCard,
  NSpin,
  NSpace,
  NButton,
  NCheckbox,
  NPopover,
  NTag,
  NProgress,
} from "naive-ui";
import { Icon } from "@iconify/vue";
import { format, isToday, isSameDay } from "date-fns";
import { zhCN } from "date-fns/locale";
import type { CalendarTask } from "@/types";
import { STATUS_META } from "@/types";

// ===== Props & Emits =====

interface WeekViewProps {
  /** 周视图日期网格 */
  weekGrid: Date[];
  /** 按日期分组的任务 */
  tasksByDate: Record<string, CalendarTask[]>;
  /** 选中的任务ID集合 */
  selectedTaskIds: Set<string>;
  /** 加载状态 */
  loading: boolean;
}

const props = defineProps<WeekViewProps>();

interface WeekViewEmits {
  /** 切换任务选择 */
  (e: "toggle-select", taskId: string, checked: boolean): void;
  /** 全选本周任务 */
  (e: "select-all-week"): void;
  /** 清空选择 */
  (e: "clear-selection"): void;
  /** 批量完成 */
  (e: "batch-complete"): void;
  /** 批量跳过 */
  (e: "batch-skip"): void;
}

const emit = defineEmits<WeekViewEmits>();

// ===== 任务统计 =====

function getTasksOfDay(day: Date): CalendarTask[] {
  const key = format(day, "yyyy-MM-dd");
  return props.tasksByDate[key] || [];
}

function getDayStats(day: Date) {
  const list = getTasksOfDay(day);
  const total = list.length;
  const done = list.filter((t) => t.status === "done").length;
  const partial = list.filter((t) => t.status === "partial").length;
  const overdue = list.filter((t) => t.is_overdue).length;
  return { total, done, partial, overdue };
}

/** 周视图是否存在可操作任务（用于显示批量工具栏） */
const weekHasTasks = computed(() =>
  props.weekGrid.some((day) => getTasksOfDay(day).length > 0),
);

/** 列底部完成率 */
function getDayCompletionRate(day: Date): number {
  const stats = getDayStats(day);
  const effective = stats.total;
  if (effective === 0) return 0;
  return Math.round((stats.done / effective) * 100);
}

// ===== ARIA 标签 =====

function getDayAriaLabel(day: Date): string {
  const dateStr = format(day, "yyyy 年 M 月 d 日 EEEE", { locale: zhCN });
  const stats = getDayStats(day);
  if (stats.total === 0) return `${dateStr}，无任务`;
  const parts = [
    `${dateStr}，共 ${stats.total} 个任务`,
    `已完成 ${stats.done}`,
  ];
  if (stats.overdue > 0) parts.push(`${stats.overdue} 个逾期`);
  return parts.join("，");
}

function getTaskAriaLabel(t: CalendarTask): string {
  const parts = [
    t.name,
    `状态：${STATUS_META[t.status].label}`,
    `目标：${t.goal_name}`,
    `进度：${t.actual_qty}/${t.plan_qty}${t.unit}`,
  ];
  if (t.is_overdue) parts.push("已逾期");
  if (t.is_blocked) {
    parts.push(
      t.blocked_by_names
        ? `前置未完成：${t.blocked_by_names}`
        : "前置任务未完成",
    );
  }
  return parts.join("，");
}

// ===== 事件处理 =====

function handleToggleSelect(taskId: string, checked: boolean) {
  emit("toggle-select", taskId, checked);
}

function handleSelectAllWeek() {
  emit("select-all-week");
}

function handleClearSelection() {
  emit("clear-selection");
}

function handleBatchComplete() {
  emit("batch-complete");
}

function handleBatchSkip() {
  emit("batch-skip");
}
</script>

<template>
  <NCard :bordered="false" role="region" aria-label="周视图" tabindex="-1">
    <!-- 批量操作工具栏 -->
    <div
      v-if="weekHasTasks"
      class="flex items-center justify-between mb-3 pb-2 border-b border-gray-100"
    >
      <span class="text-xs text-gray-500">点击任务前框选以批量操作</span>
      <NSpace :size="4">
        <NButton size="small" @click="handleSelectAllWeek">
          全选本周
        </NButton>
        <NButton
          v-if="selectedTaskIds.size > 0"
          size="small"
          @click="handleClearSelection"
        >
          清空
        </NButton>
        <span
          v-if="selectedTaskIds.size > 0"
          class="text-xs text-gray-500 self-center"
          role="status"
          aria-live="polite"
        >
          已选 {{ selectedTaskIds.size }} 项
        </span>
        <NButton
          size="small"
          type="primary"
          :disabled="selectedTaskIds.size === 0"
          @click="handleBatchComplete"
        >
          <template #icon>
            <Icon icon="mdi:playlist-check" />
          </template>
          批量完成
        </NButton>
        <NButton
          size="small"
          type="warning"
          :disabled="selectedTaskIds.size === 0"
          @click="handleBatchSkip"
        >
          <template #icon>
            <Icon icon="mdi:skip-next" />
          </template>
          批量跳过
        </NButton>
      </NSpace>
    </div>

    <NSpin :show="loading">
      <div
        class="grid grid-cols-7 gap-2"
        role="grid"
        aria-label="周视图日期网格"
      >
        <div
          v-for="day in weekGrid"
          :key="day.toISOString()"
          class="min-h-[280px] p-2 rounded border flex flex-col transition-all duration-200"
          role="gridcell"
          :aria-label="getDayAriaLabel(day)"
          :class="{
            'border-brand-500 border-2 bg-brand-100/70 shadow-md ring-1 ring-brand-300':
              isToday(day),
          }"
        >
          <!-- 列头：日期 + 逾期标记 -->
          <div
            class="flex items-center justify-center gap-1.5 text-center text-sm font-medium pb-1.5 border-b"
            :class="{ 'text-brand-600 font-bold': isToday(day) }"
          >
            <span>{{ format(day, "E d", { locale: zhCN }) }}</span>
            <NTag
              v-if="getDayStats(day).overdue > 0"
              size="tiny"
              type="error"
              :bordered="false"
              round
            >
              {{ getDayStats(day).overdue }}逾期
            </NTag>
          </div>

          <!-- 任务列表 -->
          <div
            class="mt-1 space-y-1 flex-1 overflow-auto max-h-[220px]"
            role="list"
            :aria-label="`${format(day, 'M月d日', { locale: zhCN })}任务列表`"
          >
            <NPopover
              v-for="t in getTasksOfDay(day)"
              :key="t.id"
              trigger="hover"
              :delay="300"
              placement="right"
              :width="240"
            >
              <template #trigger>
                <div
                  class="text-xs p-1 rounded flex items-center gap-1 cursor-default"
                  role="listitem"
                  :aria-label="getTaskAriaLabel(t)"
                  :class="{
                    'bg-red-50': t.is_overdue,
                    'bg-green-50': t.status === 'done',
                    'opacity-60': t.status === 'skipped',
                  }"
                >
                  <NCheckbox
                    v-if="t.status !== 'done' && t.status !== 'skipped'"
                    :checked="selectedTaskIds.has(t.id)"
                    :disabled="t.is_blocked"
                    :aria-label="`选择任务：${t.name}`"
                    @update:checked="(v) => handleToggleSelect(t.id, v)"
                    @click.stop
                  />
                  <Icon
                    :icon="STATUS_META[t.status].icon"
                    :color="STATUS_META[t.status].color"
                    width="12"
                    aria-hidden="true"
                  />
                  <span
                    class="flex-1 truncate"
                    :class="{
                      'line-through':
                        t.status === 'done' || t.status === 'skipped',
                    }"
                  >
                    {{ t.name }}
                  </span>
                </div>
              </template>

              <!-- 任务详情弹层 -->
              <div class="space-y-1 text-xs">
                <div class="font-medium text-sm">{{ t.name }}</div>
                <div class="flex items-center gap-2 text-gray-500">
                  <Icon
                    :icon="STATUS_META[t.status].icon"
                    :color="STATUS_META[t.status].color"
                    width="14"
                  />
                  <span>{{ STATUS_META[t.status].label }}</span>
                  <NTag size="tiny" :bordered="false" type="info">
                    {{ t.goal_name }}
                  </NTag>
                </div>
                <div class="text-gray-500">
                  进度：{{ t.actual_qty }}/{{ t.plan_qty }}{{ t.unit }}
                </div>
                <div
                  v-if="t.is_overdue"
                  class="text-red-500 flex items-center gap-1"
                >
                  <Icon icon="mdi:alert-circle" width="14" />已逾期
                </div>
                <div
                  v-if="t.is_blocked"
                  class="text-gray-500 flex items-center gap-1"
                >
                  <Icon icon="mdi:lock-outline" width="14" />
                  <span>{{
                    t.blocked_by_names
                      ? `前置未完成：${t.blocked_by_names}`
                      : "前置任务未完成"
                  }}</span>
                </div>
              </div>
            </NPopover>

            <div
              v-if="getTasksOfDay(day).length === 0"
              class="flex items-center justify-center text-[10px] text-gray-300 pt-2"
            >
              无
            </div>
          </div>

          <!-- 列底部完成率进度条 -->
          <div
            v-if="getDayStats(day).total > 0"
            class="mt-1 pt-1 border-t border-gray-100"
          >
            <div
              class="flex items-center justify-between text-[10px] text-gray-500 mb-0.5"
            >
              <span>完成率</span>
              <span>{{ getDayCompletionRate(day) }}%</span>
            </div>
            <NProgress
              type="line"
              :percentage="getDayCompletionRate(day)"
              :show-indicator="false"
              size="small"
              :color="
                getDayCompletionRate(day) === 100 ? '#67c23a' : '#3478f6'
              "
              aria-hidden="true"
            />
          </div>
        </div>
      </div>
    </NSpin>
  </NCard>
</template>