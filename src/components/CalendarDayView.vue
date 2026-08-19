<script setup lang="ts">
import { ref, onMounted, onUnmounted } from "vue";
import {
  NCard,
  NSpin,
  NSpace,
  NButton,
  NCheckbox,
  NTag,
  NTooltip,
  NEmpty,
} from "naive-ui";
import { Icon } from "@iconify/vue";
import { format, isToday, addDays, subDays } from "date-fns";
import { zhCN } from "date-fns/locale";
import type { CalendarTask } from "@/types";
import { STATUS_META } from "@/types";
import { getTaskAriaLabel, getBlockedTooltip } from "@/utils/task";
import BatchToolbar from "@/components/BatchToolbar.vue";
import QuickTaskForm from "@/components/QuickTaskForm.vue";

// ===== Props & Emits =====

interface DayViewProps {
  /** 选中日期 */
  selectedDate: Date;
  /** 当日任务列表 */
  tasks: CalendarTask[];
  /** 选中的任务ID集合 */
  selectedTaskIds: Set<string>;
  /** 目标选项 */
  goalOptions: { label: string; value: string }[];
  /** 加载状态 */
  loading: boolean;
}

const props = defineProps<DayViewProps>();

interface DayViewEmits {
  /** 切换任务选择 */
  (e: "toggle-select", taskId: string, checked: boolean): void;
  /** 全选当日 */
  (e: "select-all"): void;
  /** 清空选择 */
  (e: "clear-selection"): void;
  /** 批量完成 */
  (e: "batch-complete"): void;
  /** 批量跳过 */
  (e: "batch-skip"): void;
  /** 完成单个任务 */
  (e: "complete-task", task: CalendarTask): void;
  /** 跳过单个任务 */
  (e: "skip-task", task: CalendarTask): void;
  /** 快速创建任务 */
  (e: "create-task", input: {
    name: string;
    goalId: string | null;
    planQty: number | null;
  }): void;
  /** 切换日期 */
  (e: "change-date", newDate: Date): void;
}

const emit = defineEmits<DayViewEmits>();

// ===== 动画状态 =====

const animationClass = ref("");

// ===== 键盘导航 =====

let keyboardHandler: ((e: KeyboardEvent) => void) | null = null;

function setupKeyboardNavigation() {
  keyboardHandler = (e: KeyboardEvent) => {
    // 只处理左右箭头键
    if (e.key === "ArrowLeft") {
      e.preventDefault();
      const prevDay = subDays(props.selectedDate, 1);
      // 设置动画类
      animationClass.value = "animate-slide-left";
      setTimeout(() => {
        emit("change-date", prevDay);
        animationClass.value = "";
      }, 200);
    } else if (e.key === "ArrowRight") {
      e.preventDefault();
      const nextDay = addDays(props.selectedDate, 1);
      // 设置动画类
      animationClass.value = "animate-slide-right";
      setTimeout(() => {
        emit("change-date", nextDay);
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

// ===== ARIA 标签 =====
// getTaskAriaLabel / getBlockedTooltip 统一见 @/utils/task（R-04a 收敛）

// ===== 事件处理 =====

function handleToggleSelect(taskId: string, checked: boolean) {
  emit("toggle-select", taskId, checked);
}

function handleSelectAll() {
  emit("select-all");
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

function handleCompleteTask(task: CalendarTask) {
  emit("complete-task", task);
}

function handleSkipTask(task: CalendarTask) {
  emit("skip-task", task);
}

// ===== P2-4：快速创建任务 =====
// 表单 UI 与校验统一见 QuickTaskForm.vue（R-05b 收敛），提交事件透传

function handleQuickTaskSubmit(input: {
  name: string;
  goalId: string | null;
  planQty: number | null;
}) {
  emit("create-task", input);
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
  <div class="space-y-3" role="region" aria-label="日视图" tabindex="0" :class="animationClass">
    <NCard :bordered="false">
      <template #header>
        <div class="flex items-center gap-2">
          <Icon
            icon="mdi:calendar-today"
            width="20"
            class="text-brand-500"
            aria-hidden="true"
          />
          <span>{{ format(selectedDate, "yyyy-MM-dd EEEE", { locale: zhCN }) }}</span>
          <NTag v-if="isToday(selectedDate)" type="info" size="small" round>
            今天
          </NTag>
        </div>
      </template>

      <template #header-extra>
        <BatchToolbar
          v-if="tasks.length > 0"
          :selected-count="selectedTaskIds.size"
          select-all-label="全选"
          :show-clear-always="true"
          @select-all="handleSelectAll"
          @clear-selection="handleClearSelection"
          @batch-complete="handleBatchComplete"
          @batch-skip="handleBatchSkip"
        />
      </template>

      <NSpin :show="loading">
        <div
          v-if="tasks.length > 0"
          class="space-y-1"
          role="list"
          aria-label="当日任务列表"
        >
          <div
            v-for="t in tasks"
            :key="t.id"
            class="flex items-center gap-2 px-3 py-2 rounded hover:bg-gray-50"
            role="listitem"
            :aria-label="getTaskAriaLabel(t)"
            :class="{
              'bg-red-50': t.is_overdue,
            }"
          >
            <NCheckbox
              v-if="t.status !== 'done' && t.status !== 'skipped'"
              :checked="selectedTaskIds.has(t.id)"
              :disabled="t.is_blocked"
              :aria-label="`选择任务：${t.name}`"
              @update:checked="(v) => handleToggleSelect(t.id, v)"
            />

            <div
              class="flex-1 flex items-center gap-2 min-w-0"
              :class="{ 'opacity-40': t.is_blocked }"
            >
              <Icon
                :icon="STATUS_META[t.status].icon"
                :color="STATUS_META[t.status].color"
                width="18"
                aria-hidden="true"
              />

              <div class="flex-1 min-w-0">
                <div class="flex items-center gap-2">
                  <NTooltip v-if="t.is_blocked" placement="top">
                    <template #trigger>
                      <Icon
                        icon="mdi:lock-outline"
                        class="text-gray-400 shrink-0"
                        width="14"
                        aria-hidden="true"
                        role="img"
                        :aria-label="getBlockedTooltip(t)"
                      />
                    </template>
                    {{ getBlockedTooltip(t) }}
                  </NTooltip>

                  <span
                    class="text-sm font-medium truncate"
                    :class="{ 'line-through text-gray-400': t.status === 'done' }"
                  >
                    {{ t.name }}
                  </span>

                  <NTag size="tiny" :bordered="false" type="info">
                    {{ t.goal_name }}
                  </NTag>

                  <NTag
                    v-if="t.is_overdue"
                    size="tiny"
                    type="error"
                    :bordered="false"
                  >
                    逾期
                  </NTag>
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
                    :aria-label="`完成任务：${t.name}`"
                    @click="handleCompleteTask(t)"
                  >
                    <template #icon>
                      <Icon icon="mdi:check" width="16" />
                    </template>
                    完成
                  </NButton>
                </template>
                {{ getBlockedTooltip(t) }}
              </NTooltip>

              <NButton
                size="tiny"
                type="default"
                :aria-label="`跳过任务：${t.name}`"
                @click="handleSkipTask(t)"
              >
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
            >
              已完成
            </NTag>
            <NTag v-else size="tiny" type="default" :bordered="false">
              已跳过
            </NTag>
          </div>
        </div>

        <NEmpty v-else description="当日无任务" />
      </NSpin>

      <!-- P2-4：日视图底部快速添加栏（R-05b 公共组件） -->
      <template #footer>
        <div class="pt-3 border-t border-gray-100">
          <QuickTaskForm
            mode="inline"
            :goal-options="goalOptions"
            submit-label="添加"
            @submit="handleQuickTaskSubmit"
          />
        </div>
      </template>
    </NCard>
  </div>
</template>