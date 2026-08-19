<script setup lang="ts">
/**
 * BatchToolbar.vue - 批量操作工具栏公共组件（R-05a）
 *
 * 收敛周视图 / 日视图（及历史重复实现）中的批量操作 UI：
 * 全选 / 清空 / 已选计数 / 批量完成 / 批量跳过。
 * 组件为纯展示组件，不感知任务状态，仅通过 props 接收已选数量、
 * 通过 events 上报用户动作。
 */
import { NSpace, NButton } from "naive-ui";
import { Icon } from "@iconify/vue";

interface BatchToolbarProps {
  /** 已选任务数量 */
  selectedCount: number;
  /** 全选按钮文案（默认"全选"） */
  selectAllLabel?: string;
  /** 是否使用带边框分隔的工具栏条布局（周视图） */
  bordered?: boolean;
  /** 始终显示"清空"按钮（即使未选中任何项，日视图） */
  showClearAlways?: boolean;
}

const props = withDefaults(defineProps<BatchToolbarProps>(), {
  selectAllLabel: "全选",
  bordered: false,
  showClearAlways: false,
});

interface BatchToolbarEmits {
  (e: "select-all"): void;
  (e: "clear-selection"): void;
  (e: "batch-complete"): void;
  (e: "batch-skip"): void;
}

const emit = defineEmits<BatchToolbarEmits>();
</script>

<template>
  <div
    class="flex items-center justify-between gap-2"
    :class="bordered ? 'mb-3 pb-2 border-b border-gray-100' : ''"
  >
    <!-- 左侧内容（如操作提示），默认留空 -->
    <slot name="leading" />

    <NSpace :size="4">
      <NButton size="small" @click="emit('select-all')">
        {{ selectAllLabel }}
      </NButton>
      <NButton
        v-if="showClearAlways || selectedCount > 0"
        size="small"
        @click="emit('clear-selection')"
      >
        清空
      </NButton>
      <span
        v-if="selectedCount > 0"
        class="text-xs text-gray-500 self-center"
        role="status"
        aria-live="polite"
      >
        已选 {{ selectedCount }} 项
      </span>
      <NButton
        size="small"
        type="primary"
        :disabled="selectedCount === 0"
        @click="emit('batch-complete')"
      >
        <template #icon>
          <Icon icon="mdi:playlist-check" />
        </template>
        批量完成
      </NButton>
      <NButton
        size="small"
        type="warning"
        :disabled="selectedCount === 0"
        @click="emit('batch-skip')"
      >
        <template #icon>
          <Icon icon="mdi:skip-next" />
        </template>
        批量跳过
      </NButton>
    </NSpace>
  </div>
</template>
