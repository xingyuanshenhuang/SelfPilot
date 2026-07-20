<script setup lang="ts">
import { ref, computed, nextTick } from "vue";
import {
  NCard,
  NSpace,
  NButton,
  NRadioGroup,
  NRadioButton,
  NSelect,
  NSwitch,
  NTag,
  NIcon,
} from "naive-ui";
import { Icon } from "@iconify/vue";
import type { TaskStatus } from "@/types";

type ViewMode = "month" | "week" | "day";

// ===== Props & Emits =====

interface ToolbarProps {
  /** 视图模式 */
  viewMode: ViewMode;
  /** 标题文本 */
  headerLabel: string;
  /** 周期标签 */
  periodLabel: string;
  /** 周期统计 */
  periodStats: {
    total: number;
    done: number;
    pending: number;
    overdue: number;
    skipped: number;
    completionRate: number;
  };
  /** 目标选项 */
  goalOptions: { label: string; value: string }[];
  /** 筛选激活条件数 */
  filterActiveCount: number;
  /** 是否有筛选条件 */
  filterHasCondition: boolean;
  /** 加载状态 */
  loading: boolean;
}

const props = defineProps<ToolbarProps>();

interface ToolbarEmits {
  (e: "prev"): void;
  (e: "next"): void;
  (e: "go-today"): void;
  (e: "update:viewMode", mode: ViewMode): void;
  (e: "reset-filter"): void;
  (e: "toggle-filter-collapsed"): void;
}

const emit = defineEmits<ToolbarEmits>();

// ===== 筛选状态 =====

const filterGoalIds = ref<string[]>([]);
const filterStatuses = ref<TaskStatus[]>([]);
const filterOverdueOnly = ref(false);
const filterCollapsed = ref(true);
const filterContentRef = ref<HTMLElement | null>(null);

const statusOptions: { label: string; value: TaskStatus }[] = [
  { label: "未完成", value: "pending" },
  { label: "部分完成", value: "partial" },
  { label: "已完成", value: "done" },
  { label: "已跳过", value: "skipped" },
];

// ===== 事件处理 =====

function handlePrev() {
  emit("prev");
}

function handleNext() {
  emit("next");
}

function handleGoToday() {
  emit("go-today");
}

function handleViewModeChange(value: ViewMode) {
  emit("update:viewMode", value);
}

function handleResetFilter() {
  filterGoalIds.value = [];
  filterStatuses.value = [];
  filterOverdueOnly.value = false;
  emit("reset-filter");
}

function handleToggleFilter() {
  filterCollapsed.value = !filterCollapsed.value;
  emit("toggle-filter-collapsed");

  // 展开后聚焦到第一个可交互元素
  if (!filterCollapsed.value) {
    nextTick(() => {
      filterContentRef.value
        ?.querySelector<HTMLElement>(
          "input, button, [role='button'], [role='combobox'], [tabindex='0']"
        )
        ?.focus();
    });
  }
}

// ===== 暴露筛选状态给父组件 =====

defineExpose({
  filterGoalIds,
  filterStatuses,
  filterOverdueOnly,
});
</script>

<template>
  <!-- 顶部导航栏 -->
  <NCard :bordered="false" size="small">
    <div class="flex items-center justify-between flex-wrap gap-2">
      <NSpace align="center">
        <NButton
          quaternary
          circle
          :disabled="loading"
          aria-label="上一个周期"
          @click="handlePrev"
        >
          <template #icon>
            <Icon icon="mdi:chevron-left" />
          </template>
        </NButton>

        <span
          class="text-lg font-semibold min-w-[180px] text-center"
          aria-live="polite"
        >
          {{ headerLabel }}
        </span>

        <NButton
          quaternary
          circle
          :disabled="loading"
          aria-label="下一个周期"
          @click="handleNext"
        >
          <template #icon>
            <Icon icon="mdi:chevron-right" />
          </template>
        </NButton>

        <NButton size="small" :disabled="loading" @click="handleGoToday">
          今天
        </NButton>
      </NSpace>

      <NRadioGroup
        :value="viewMode"
        size="small"
        aria-label="视图模式切换"
        @update:value="handleViewModeChange"
      >
        <NRadioButton value="month">月</NRadioButton>
        <NRadioButton value="week">周</NRadioButton>
        <NRadioButton value="day">日</NRadioButton>
      </NRadioGroup>
    </div>
  </NCard>

  <!-- 周期统计概览 -->
  <NCard :bordered="false" size="small">
    <div
      class="flex items-center gap-6 flex-wrap"
      role="status"
      aria-live="polite"
      :aria-label="`${periodLabel}统计：完成率 ${periodStats.completionRate}%，已完成 ${periodStats.done}，待完成 ${periodStats.pending}，逾期 ${periodStats.overdue}`"
    >
      <div class="flex items-baseline gap-2">
        <span class="text-xs text-gray-500">{{ periodLabel }}</span>
        <span class="text-xl font-bold text-brand-600">
          {{ periodStats.completionRate }}%
        </span>
        <span class="text-xs text-gray-400">完成率</span>
      </div>

      <div class="flex items-center gap-1.5">
        <Icon icon="mdi:check-circle" class="text-green-500" width="16" />
        <span class="text-sm font-semibold">{{ periodStats.done }}</span>
        <span class="text-xs text-gray-500">已完成</span>
      </div>

      <div class="flex items-center gap-1.5">
        <Icon icon="mdi:clock-outline" class="text-gray-400" width="16" />
        <span class="text-sm font-semibold">{{ periodStats.pending }}</span>
        <span class="text-xs text-gray-500">待完成</span>
      </div>

      <div v-if="periodStats.overdue > 0" class="flex items-center gap-1.5">
        <Icon icon="mdi:alert-circle" class="text-red-500" width="16" />
        <span class="text-sm font-semibold text-red-500">
          {{ periodStats.overdue }}
        </span>
        <span class="text-xs text-gray-500">逾期</span>
      </div>

      <div v-if="periodStats.skipped > 0" class="flex items-center gap-1.5">
        <Icon icon="mdi:skip-next-circle-outline" class="text-gray-400" width="16" />
        <span class="text-sm font-semibold text-gray-400">
          {{ periodStats.skipped }}
        </span>
        <span class="text-xs text-gray-500">已跳过</span>
      </div>
    </div>
  </NCard>

  <!-- 可折叠筛选栏 -->
  <NCard :bordered="false" size="small" class="filter-card">
    <!-- 标题行 -->
    <div class="flex items-center gap-2">
      <button
        type="button"
        class="filter-toggle-btn"
        :aria-expanded="!filterCollapsed"
        aria-controls="calendar-filter-content"
        @click="handleToggleFilter"
      >
        <Icon
          icon="mdi:chevron-right"
          width="18"
          class="filter-toggle-icon"
          :class="{ 'rotate-90': !filterCollapsed }"
          aria-hidden="true"
        />
        <Icon icon="mdi:filter-variant" class="text-gray-400" width="16" aria-hidden="true" />
        <span class="text-sm font-medium text-gray-700">筛选</span>

        <!-- 激活条件数徽标 -->
        <span
          v-if="filterCollapsed && filterActiveCount > 0"
          class="filter-badge"
          role="status"
          aria-label="已应用筛选条件数"
        >
          {{ filterActiveCount }}
        </span>

        <!-- 折叠态提示 -->
        <span
          v-if="filterCollapsed"
          class="text-xs text-gray-400 ml-auto"
          aria-hidden="true"
        >
          点击展开
        </span>
      </button>
    </div>

    <!-- 展开内容区 -->
    <Transition name="filter-expand">
      <div
        v-show="!filterCollapsed"
        id="calendar-filter-content"
        ref="filterContentRef"
        class="filter-content"
        role="region"
        aria-label="筛选选项"
      >
        <div class="flex items-center gap-3 flex-wrap pt-3 mt-1 border-t border-gray-100">
          <div class="filter-field">
            <label class="filter-label" for="filter-goal-select">目标</label>
            <NSelect
              v-model:value="filterGoalIds"
              multiple
              :options="goalOptions"
              placeholder="选择目标"
              size="small"
              clearable
              max-tag-count="responsive"
              input-id="filter-goal-select"
              style="min-width: 180px"
            />
          </div>

          <div class="filter-field">
            <label class="filter-label" for="filter-status-select">状态</label>
            <NSelect
              v-model:value="filterStatuses"
              multiple
              :options="statusOptions"
              placeholder="选择状态"
              size="small"
              clearable
              max-tag-count="responsive"
              input-id="filter-status-select"
              style="min-width: 160px"
            />
          </div>

          <div class="filter-field">
            <label class="filter-label" for="filter-overdue-switch">仅逾期</label>
            <NSwitch
              v-model:value="filterOverdueOnly"
              size="small"
              input-id="filter-overdue-switch"
            />
          </div>

          <NButton v-if="filterHasCondition" size="small" quaternary @click="handleResetFilter">
            <template #icon>
              <Icon icon="mdi:close" />
            </template>
            重置
          </NButton>
        </div>
      </div>
    </Transition>
  </NCard>
</template>

<style scoped>
/* 筛选栏按钮 */
.filter-toggle-btn {
  display: flex;
  align-items: center;
  gap: 0.375rem;
  width: 100%;
  padding: 4px 6px;
  border: 0;
  background: transparent;
  border-radius: 4px;
  cursor: pointer;
  color: inherit;
  font: inherit;
  text-align: left;
  transition: background-color 0.2s ease;
}

.filter-toggle-btn:hover {
  background-color: rgba(52, 120, 246, 0.06);
}

.filter-toggle-btn:focus-visible {
  outline: 2px solid #3478f6;
  outline-offset: 2px;
}

.filter-toggle-icon {
  transition: transform 0.3s ease;
  color: #6b7280;
  flex-shrink: 0;
}

.filter-toggle-icon.rotate-90 {
  transform: rotate(90deg);
}

/* 激活条件数徽标 */
.filter-badge {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 18px;
  height: 18px;
  padding: 0 4px;
  border-radius: 9px;
  background-color: #3478f6;
  color: #fff;
  font-size: 11px;
  font-weight: 600;
  line-height: 1;
  margin-left: 2px;
}

/* 展开内容区 */
.filter-content {
  overflow: hidden;
}

/* 筛选字段布局 */
.filter-field {
  display: flex;
  align-items: center;
  gap: 0.375rem;
}

.filter-label {
  font-size: 12px;
  color: #6b7280;
  white-space: nowrap;
}

/* 过渡动画 */
.filter-expand-enter-active,
.filter-expand-leave-active {
  transition: opacity 0.3s ease, transform 0.3s ease;
  max-height: 300px;
  overflow: hidden;
}

.filter-expand-enter-from,
.filter-expand-leave-to {
  opacity: 0;
  transform: translateY(-4px);
  max-height: 0;
}

/* 响应式 */
@media (max-width: 640px) {
  .filter-toggle-btn {
    flex-wrap: wrap;
    gap: 0.25rem;
  }

  .filter-field {
    width: 100%;
  }

  .filter-field :deep(.n-base-selection) {
    flex: 1;
    min-width: 0 !important;
  }

  .filter-field .filter-label {
    width: 40px;
    flex-shrink: 0;
  }
}
</style>