<script setup lang="ts">
import { ref, nextTick } from "vue";
import {
  NCard,
  NSpin,
  NSpace,
  NButton,
  NCheckbox,
  NTag,
  NTooltip,
  NEmpty,
  NInput,
  NSelect,
  NInputNumber,
  useMessage,
  useDialog,
} from "naive-ui";
import { Icon } from "@iconify/vue";
import { format, isToday } from "date-fns";
import { zhCN } from "date-fns/locale";
import type { CalendarTask } from "@/types";
import { STATUS_META } from "@/types";

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
}

const emit = defineEmits<DayViewEmits>();

const message = useMessage();
const dialog = useDialog();

// ===== P2-4：快速添加栏状态 =====

const quickTaskName = ref("");
const quickTaskGoalId = ref<string | null>(null);
const quickTaskPlanQty = ref<number | null>(null);
const quickTaskCreating = ref(false);

// ===== ARIA 标签 =====

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

function handleCreateTask() {
  if (!quickTaskName.value.trim()) {
    message.warning("请输入任务名称");
    return;
  }
  if (!quickTaskGoalId.value) {
    message.warning("请选择目标");
    return;
  }

  emit("create-task", {
    name: quickTaskName.value,
    goalId: quickTaskGoalId.value,
    planQty: quickTaskPlanQty.value,
  });

  // 清空输入并聚焦
  quickTaskName.value = "";
  quickTaskPlanQty.value = null;
  nextTick(() => {
    document.getElementById("quick-task-name")?.focus();
  });
}
</script>

<template>
  <div class="space-y-3" role="region" aria-label="日视图" tabindex="-1">
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
        <NSpace v-if="tasks.length > 0" :size="4">
          <NButton size="small" @click="handleSelectAll">全选</NButton>
          <NButton size="small" @click="handleClearSelection">清空</NButton>
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
                        :aria-label="
                          t.blocked_by_names
                            ? `前置任务未完成：${t.blocked_by_names}`
                            : '前置任务未完成，暂不可标记完成'
                        "
                      />
                    </template>
                    {{
                      t.blocked_by_names
                        ? `前置任务未完成：${t.blocked_by_names}`
                        : '前置任务未完成，暂不可标记完成'
                    }}
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
                {{
                  t.blocked_by_names
                    ? `前置任务未完成：${t.blocked_by_names}`
                    : '前置任务未完成'
                }}
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

      <!-- P2-4：日视图底部快速添加栏 -->
      <template #footer>
        <div class="flex flex-wrap items-end gap-2 pt-3 border-t border-gray-100">
          <!-- 无目标提示 -->
          <div
            v-if="goalOptions.length === 0"
            class="w-full text-center py-3 text-sm text-gray-400"
          >
            <Icon icon="mdi:information-outline" width="16" class="mr-1" />
            当前没有可用目标，请先创建目标
          </div>

          <div v-else class="flex flex-wrap items-end gap-2 w-full">
            <div class="flex-1 min-w-[160px]">
              <label for="quick-task-name" class="block text-xs text-gray-500 mb-1">
                任务名称
              </label>
              <NInput
                id="quick-task-name"
                v-model:value="quickTaskName"
                placeholder="输入任务名，回车创建"
                size="small"
                :disabled="quickTaskCreating"
                @keydown.enter="handleCreateTask"
              />
            </div>

            <div class="w-[140px]">
              <label for="quick-task-goal" class="block text-xs text-gray-500 mb-1">
                选择目标
              </label>
              <NSelect
                id="quick-task-goal"
                v-model:value="quickTaskGoalId"
                :options="goalOptions"
                placeholder="目标"
                size="small"
                :disabled="quickTaskCreating"
                :max-tag-count="1"
              />
            </div>

            <div class="w-[100px]">
              <label for="quick-task-qty" class="block text-xs text-gray-500 mb-1">
                数量（可选）
              </label>
              <NInputNumber
                id="quick-task-qty"
                v-model:value="quickTaskPlanQty"
                placeholder="1"
                size="small"
                :min="1"
                :disabled="quickTaskCreating"
              />
            </div>

            <NButton
              type="primary"
              size="small"
              :disabled="!quickTaskName.trim() || !quickTaskGoalId"
              :loading="quickTaskCreating"
              aria-label="创建任务"
              @click="handleCreateTask"
            >
              <template #icon>
                <Icon icon="mdi:plus" width="16" />
              </template>
              添加
            </NButton>
          </div>
        </div>
      </template>
    </NCard>
  </div>
</template>