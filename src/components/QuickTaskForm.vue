<script setup lang="ts">
/**
 * QuickTaskForm.vue - 快速创建任务表单公共组件（R-05b）
 *
 * 收敛日视图内联快速添加栏与月视图创建任务弹窗中重复的表单：
 * 任务名称 + 目标 + 数量（可选）。
 *
 * - mode="inline"：水平排列的内联条（日视图底部），自带无目标空态提示
 * - mode="modal"：纵向排列的表单 + 底部 取消/提交（月视图弹窗）
 * 组件内部维护输入状态，提交后通过 emit 上报数据。
 */
import { ref, computed, nextTick, useId } from "vue";
import {
  NInput,
  NSelect,
  NInputNumber,
  NButton,
  NSpace,
  useMessage,
} from "naive-ui";
import { Icon } from "@iconify/vue";

/** 提交载荷 */
export interface QuickTaskFormInput {
  name: string;
  goalId: string | null;
  planQty: number | null;
}

interface QuickTaskFormProps {
  /** 表单模式：inline 内联条 / modal 纵向表单 */
  mode?: "inline" | "modal";
  /** 目标选项 */
  goalOptions: { label: string; value: string }[];
  /** 提交按钮文案（默认"添加"） */
  submitLabel?: string;
  /** 是否正在提交 */
  loading?: boolean;
  /** 提交成功后是否重置输入并聚焦（内联模式默认开启） */
  resetOnSubmit?: boolean;
}

const props = withDefaults(defineProps<QuickTaskFormProps>(), {
  mode: "inline",
  submitLabel: "添加",
  loading: false,
  resetOnSubmit: true,
});

interface QuickTaskFormEmits {
  (e: "submit", input: QuickTaskFormInput): void;
  (e: "cancel"): void;
}

const emit = defineEmits<QuickTaskFormEmits>();

const message = useMessage();

// 唯一 id，避免同页多个实例的 label/input 关联冲突
const FIELD_NAME = `qtf-name-${useId()}`;
const FIELD_GOAL = `qtf-goal-${useId()}`;
const FIELD_QTY = `qtf-qty-${useId()}`;

const name = ref("");
const goalId = ref<string | null>(null);
const planQty = ref<number | null>(null);

const canSubmit = computed(
  () => name.value.trim().length > 0 && !!goalId.value,
);

function handleSubmit() {
  if (!name.value.trim()) {
    message.warning("请输入任务名称");
    return;
  }
  if (!goalId.value) {
    message.warning("请选择目标");
    return;
  }

  emit("submit", {
    name: name.value,
    goalId: goalId.value,
    planQty: planQty.value,
  });

  if (props.resetOnSubmit) {
    name.value = "";
    planQty.value = null;
    nextTick(() => {
      document.getElementById(FIELD_NAME)?.focus();
    });
  }
}
</script>

<template>
  <!-- 无可用目标空态（仅内联模式） -->
  <div
    v-if="goalOptions.length === 0 && mode === 'inline'"
    class="w-full text-center py-3 text-sm text-gray-400"
  >
    <Icon icon="mdi:information-outline" width="16" class="mr-1" />
    当前没有可用目标，请先创建目标
  </div>

  <!-- inline：水平排列 -->
  <div
    v-else-if="mode === 'inline'"
    class="flex flex-wrap items-end gap-2 w-full"
  >
    <div class="flex-1 min-w-[160px]">
      <label :for="FIELD_NAME" class="block text-xs text-gray-500 mb-1">
        任务名称
      </label>
      <NInput
        :id="FIELD_NAME"
        v-model:value="name"
        placeholder="输入任务名，回车创建"
        size="small"
        :disabled="loading"
        @keydown.enter="handleSubmit"
      />
    </div>

    <div class="w-[140px]">
      <label :for="FIELD_GOAL" class="block text-xs text-gray-500 mb-1">
        选择目标
      </label>
      <NSelect
        :id="FIELD_GOAL"
        v-model:value="goalId"
        :options="goalOptions"
        placeholder="目标"
        size="small"
        :disabled="loading"
        :max-tag-count="1"
      />
    </div>

    <div class="w-[100px]">
      <label :for="FIELD_QTY" class="block text-xs text-gray-500 mb-1">
        数量（可选）
      </label>
      <NInputNumber
        :id="FIELD_QTY"
        v-model:value="planQty"
        placeholder="1"
        size="small"
        :min="1"
        :disabled="loading"
      />
    </div>

    <NButton
      type="primary"
      size="small"
      :disabled="!canSubmit"
      :loading="loading"
      aria-label="创建任务"
      @click="handleSubmit"
    >
      <template #icon>
        <Icon icon="mdi:plus" width="16" />
      </template>
      {{ submitLabel }}
    </NButton>
  </div>

  <!-- modal：纵向排列 + 底部操作 -->
  <div v-else class="space-y-4">
    <div>
      <label :for="FIELD_NAME" class="block text-xs text-gray-500 mb-1">
        任务名称
      </label>
      <NInput
        :id="FIELD_NAME"
        v-model:value="name"
        placeholder="输入任务名"
        :disabled="loading"
        @keydown.enter="handleSubmit"
      />
    </div>

    <div>
      <label :for="FIELD_GOAL" class="block text-xs text-gray-500 mb-1">
        选择目标
      </label>
      <NSelect
        :id="FIELD_GOAL"
        v-model:value="goalId"
        :options="goalOptions"
        placeholder="选择目标"
        :disabled="loading"
        :max-tag-count="1"
      />
    </div>

    <div>
      <label :for="FIELD_QTY" class="block text-xs text-gray-500 mb-1">
        数量（可选）
      </label>
      <NInputNumber
        :id="FIELD_QTY"
        v-model:value="planQty"
        placeholder="1"
        :min="1"
        :disabled="loading"
        style="width: 100%"
      />
    </div>

    <div class="flex justify-end gap-2">
      <NButton :disabled="loading" @click="emit('cancel')">取消</NButton>
      <NButton
        type="primary"
        :disabled="!canSubmit"
        :loading="loading"
        @click="handleSubmit"
      >
        {{ submitLabel }}
      </NButton>
    </div>
  </div>
</template>
