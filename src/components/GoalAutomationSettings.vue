<script setup lang="ts">
/**
 * 目标级自动化规划设置（阶段 4C）
 *
 * - 允许自动重新规划：系统事件（任务完成/跳过、逾期累积、目标改期等）
 *   经触发网关放行后生成规划建议；手动「重新规划」不受影响
 * - 允许自动应用：事件命中「触发设置」的允许列表时直接落库，
 *   不弹确认（可通过历史时间线撤销）
 */
import { ref, watch } from "vue";
import { NModal, NSpace, NSwitch, NAlert, NButton, useMessage } from "naive-ui";
import { Icon } from "@iconify/vue";
import type { Goal, GoalAutomationPolicy } from "@/types";
import * as goalApi from "@/api/goal";
import { toErrorMessage } from "@/api/client";

const props = defineProps<{
  show: boolean;
  goal: Goal | null;
}>();

const emit = defineEmits<{
  (e: "update:show", value: boolean): void;
  (e: "saved", policy: GoalAutomationPolicy): void;
}>();

const message = useMessage();
const loading = ref(false);
const saving = ref(false);
const autoReplan = ref(true);
const autoApply = ref(false);

async function load() {
  const goal = props.goal;
  if (!goal) return;
  loading.value = true;
  try {
    const policy = await goalApi.getGoalAutomation(goal.id);
    autoReplan.value = policy.auto_replan === 1;
    autoApply.value = policy.auto_apply === 1;
  } catch (e) {
    message.error(toErrorMessage(e));
  } finally {
    loading.value = false;
  }
}

watch(
  () => props.show,
  (v) => {
    if (v) load();
  },
);

async function handleSave() {
  const goal = props.goal;
  if (!goal) return;
  saving.value = true;
  try {
    const policy = await goalApi.updateGoalAutomation({
      goal_id: goal.id,
      auto_replan: autoReplan.value ? 1 : 0,
      auto_apply: autoApply.value ? 1 : 0,
    });
    message.success("自动化规划设置已保存");
    emit("saved", policy);
    emit("update:show", false);
  } catch (e) {
    message.error(toErrorMessage(e));
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <NModal
    :show="show"
    preset="card"
    title="自动化规划设置"
    style="width: 520px"
    @update:show="emit('update:show', $event)"
  >
    <NSpace vertical :size="16">
      <div class="text-sm text-gray-500 dark:text-gray-400">
        目标：<span class="text-gray-800 dark:text-gray-200 font-medium">{{
          goal?.name
        }}</span>
      </div>

      <div class="flex items-start justify-between gap-4">
        <div class="min-w-0">
          <div class="text-sm font-medium">允许自动重新规划</div>
          <div class="text-xs text-gray-400 dark:text-gray-500 mt-0.5">
            任务完成/跳过、逾期累积、目标改期等系统事件会先经触发网关评估，
            通过后生成「规划建议」；关闭后系统事件不再打扰（手动重新规划不受影响）
          </div>
        </div>
        <NSwitch
          v-model:value="autoReplan"
          :disabled="loading || saving"
          class="flex-shrink-0"
        />
      </div>

      <div class="flex items-start justify-between gap-4">
        <div class="min-w-0">
          <div class="text-sm font-medium">允许自动应用（无需确认）</div>
          <div class="text-xs text-gray-400 dark:text-gray-500 mt-0.5">
            事件类型还需命中「触发设置」中的「允许自动应用的事件」，
            才会直接改写任务排期；可在重新规划历史中撤销
          </div>
        </div>
        <NSwitch
          v-model:value="autoApply"
          :disabled="loading || saving"
          class="flex-shrink-0"
        />
      </div>

      <NAlert v-if="autoApply" type="warning" :bordered="false" :show-icon="true">
        开启后，符合条件的系统事件将跳过确认直接调整该目标的任务安排。
      </NAlert>

      <div class="text-xs text-gray-400 dark:text-gray-500 flex items-start gap-1">
        <Icon icon="mdi:information-outline" width="14" class="mt-0.5 shrink-0" />
        <span>
          冷却时长、最小影响面、EWMA 半衰期与自动应用事件列表为全局参数，
          可在目标树右上角「触发设置」中调整。
        </span>
      </div>
    </NSpace>

    <template #footer>
      <NSpace justify="end">
        <NButton @click="emit('update:show', false)">取消</NButton>
        <NButton
          type="primary"
          :loading="saving"
          :disabled="loading || !goal"
          @click="handleSave"
        >
          <template #icon><Icon icon="mdi:content-save-outline" /></template>
          保存
        </NButton>
      </NSpace>
    </template>
  </NModal>
</template>
