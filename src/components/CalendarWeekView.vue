<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import {
  NCard,
  NSpin,
  NCheckbox,
  NPopover,
  NTag,
  NProgress,
} from "naive-ui";
import { Icon } from "@iconify/vue";
import { format, isToday, addWeeks, subWeeks } from "date-fns";
import { zhCN } from "date-fns/locale";
import type { CalendarTask } from "@/types";
import { STATUS_META } from "@/types";
import {
  getTaskAriaLabel,
  getDayAriaLabel,
  getDayStats as calcDayStats,
  getBlockedTooltip,
} from "@/utils/task";
import BatchToolbar from "@/components/BatchToolbar.vue";

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
  /** 切换周 */
  (e: "change-week", newStartDate: Date): void;
}

const emit = defineEmits<WeekViewEmits>();

// ===== 动画状态 =====

const animationClass = ref("");

// ===== 键盘导航 =====

let keyboardHandler: ((e: KeyboardEvent) => void) | null = null;

function setupKeyboardNavigation() {
  keyboardHandler = (e: KeyboardEvent) => {
    // 只处理左右箭头键
    if (e.key === "ArrowLeft") {
      e.preventDefault();
      // 获取当前周的第一天（周一）
      const firstDay = props.weekGrid[0];
      const prevWeek = subWeeks(firstDay, 1);
      // 设置动画类
      animationClass.value = "animate-slide-left";
      setTimeout(() => {
        emit("change-week", prevWeek);
        animationClass.value = "";
      }, 200);
    } else if (e.key === "ArrowRight") {
      e.preventDefault();
      // 获取当前周的第一天（周一）
      const firstDay = props.weekGrid[0];
      const nextWeek = addWeeks(firstDay, 1);
      // 设置动画类
      animationClass.value = "animate-slide-right";
      setTimeout(() => {
        emit("change-week", nextWeek);
        animationClass.value = "";
      }, 200);
    }
  };

  // 添加键盘事件监听器
  window.addEventListener("keydown", keyboardHandler);
}

function cleanupKeyboardNavigation() {
  if (keyboardHandler) {
    window.removeEventListener("keydown", keyboardHandler);
    keyboardHandler = null;
  }
}

// ===== 任务统计（computed 缓存，避免模板中重复计算） =====

interface DayStat {
  total: number;
  done: number;
  partial: number;
  overdue: number;
  tasks: CalendarTask[];
  completionRate: number;
}

const dayStatsMap = computed(() => {
  const map: Record<string, DayStat> = {};
  for (const day of props.weekGrid) {
    const key = format(day, "yyyy-MM-dd");
    const tasks = props.tasksByDate[key] || [];
    const { total, done, partial, overdue } = calcDayStats(tasks);
    const completionRate = total > 0 ? Math.round((done / total) * 100) : 0;
    map[key] = { total, done, partial, overdue, tasks, completionRate };
  }
  return map;
});

function getTasksOfDay(day: Date): CalendarTask[] {
  const key = format(day, "yyyy-MM-dd");
  return dayStatsMap.value[key]?.tasks || [];
}

function getDayStats(day: Date): Omit<DayStat, "tasks" | "completionRate"> {
  const key = format(day, "yyyy-MM-dd");
  const stat = dayStatsMap.value[key];
  if (!stat) return { total: 0, done: 0, partial: 0, overdue: 0 };
  return { total: stat.total, done: stat.done, partial: stat.partial, overdue: stat.overdue };
}

/** 周视图是否存在可操作任务（用于显示批量工具栏） */
const weekHasTasks = computed(() =>
  props.weekGrid.some((day) => getTasksOfDay(day).length > 0),
);

/** 列底部完成率 */
function getDayCompletionRate(day: Date): number {
  const key = format(day, "yyyy-MM-dd");
  return dayStatsMap.value[key]?.completionRate ?? 0;
}

// ===== ARIA 标签 =====
// getDayAriaLabel / getTaskAriaLabel / getBlockedTooltip 统一见 @/utils/task（R-04a 收敛）

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

// ===== 生命周期 =====

onMounted(() => {
  setupKeyboardNavigation();
});

onUnmounted(() => {
  cleanupKeyboardNavigation();
});
</script>

<template>
  <NCard :bordered="false" role="region" aria-label="周视图" tabindex="0" :class="animationClass">
    <!-- 批量操作工具栏（R-05a 公共组件） -->
    <BatchToolbar
      v-if="weekHasTasks"
      :selected-count="selectedTaskIds.size"
      select-all-label="全选本周"
      bordered
      @select-all="handleSelectAllWeek"
      @clear-selection="handleClearSelection"
      @batch-complete="handleBatchComplete"
      @batch-skip="handleBatchSkip"
    >
      <template #leading>
        <span class="text-xs text-gray-500">点击任务前框选以批量操作</span>
      </template>
    </BatchToolbar>

    <NSpin :show="loading">
      <div class="overflow-x-auto">
        <div
          class="grid grid-cols-7 gap-2 min-w-[700px]"
          role="grid"
          aria-label="周视图日期网格"
        >
          <div
            v-for="day in weekGrid"
            :key="day.toISOString()"
            class="min-h-[280px] p-2 rounded border flex flex-col transition-all duration-200"
            role="gridcell"
            :aria-label="getDayAriaLabel(day, getDayStats(day))"
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
                  <span>{{ getBlockedTooltip(t) }}</span>
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
      </div>
    </NSpin>
  </NCard>
</template>