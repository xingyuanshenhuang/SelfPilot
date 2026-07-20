<script setup lang="ts">
import { computed } from "vue";
import { NModal, NCard, NButton, NTag } from "naive-ui";
import { Icon } from "@iconify/vue";
import type { SetbackSituation } from "@/types";

const props = defineProps<{
  show: boolean;
  setback: SetbackSituation | null;
}>();

const emit = defineEmits<{
  (e: "update:show", value: boolean): void;
  (e: "close"): void;
}>();

const hasStreakBreak = computed(() => props.setback?.has_streak_break ?? false);
const hasProgressLag = computed(() => props.setback?.has_progress_lag ?? false);
const streakPrev = computed(() => props.setback?.streak_break_prev ?? 0);
const laggingGoals = computed(() => props.setback?.lagging_goals ?? []);

function handleClose() {
  emit("update:show", false);
  emit("close");
}
</script>

<template>
  <NModal
    :show="show"
    :mask-closable="true"
    :close-on-esc="true"
    @update:show="emit('update:show', $event)"
  >
    <NCard
      style="width: 480px; max-width: 90vw"
      :bordered="false"
      class="rounded-xl"
    >
      <template #header>
        <div class="flex items-center gap-2 text-amber-600">
          <Icon icon="mdi:heart-outline" width="24" />
          <span class="text-lg font-medium">嘿，给自己一点时间</span>
        </div>
      </template>

      <div class="space-y-4">
        <!-- 连续中断提示 -->
        <div
          v-if="hasStreakBreak"
          class="p-3 rounded-lg bg-amber-50 border border-amber-100"
        >
          <div class="flex items-center gap-2 mb-2">
            <Icon
              icon="mdi:calendar-remove"
              class="text-amber-500"
              width="18"
            />
            <span class="font-medium text-amber-700">连续记录中断了</span>
          </div>
          <p class="text-sm text-amber-600">
            之前保持了
            <strong>{{ streakPrev }}</strong> 天连续，今天没有完成任务。
            没关系，中断只是暂停，不是放弃。
          </p>
        </div>

        <!-- 进度滞后提示 -->
        <div
          v-if="hasProgressLag"
          class="p-3 rounded-lg bg-orange-50 border border-orange-100"
        >
          <div class="flex items-center gap-2 mb-2">
            <Icon
              icon="mdi:clock-alert-outline"
              class="text-orange-500"
              width="18"
            />
            <span class="font-medium text-orange-700"
              >部分目标进度需要关注</span
            >
          </div>
          <div class="space-y-2">
            <div
              v-for="goal in laggingGoals"
              :key="goal.id"
              class="flex items-center justify-between text-sm"
            >
              <span class="text-orange-600">{{ goal.name }}</span>
              <NTag
                size="small"
                :type="goal.days_remaining < 0 ? 'error' : 'warning'"
              >
                {{
                  goal.days_remaining < 0
                    ? `已逾期 ${-goal.days_remaining} 天`
                    : `剩余 ${goal.days_remaining} 天`
                }}
              </NTag>
            </div>
          </div>
          <p class="text-sm text-orange-600 mt-2">
            可以尝试重新规划，或者调整目标节奏。
          </p>
        </div>

        <!-- 安抚文案 -->
        <p class="text-gray-600 text-sm">
          遇到波折很正常，关键是能不能重启。明天又是新的一天。
        </p>
      </div>

      <template #footer>
        <div class="flex justify-end">
          <NButton type="primary" @click="handleClose"> 我知道了 </NButton>
        </div>
      </template>
    </NCard>
  </NModal>
</template>
