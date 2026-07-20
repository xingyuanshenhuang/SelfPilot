<script setup lang="ts">
import { computed, watch } from "vue";
import { NModal, NCard, NButton, NProgress } from "naive-ui";
import { Icon } from "@iconify/vue";
import type { CelebrationAchievement, Encouragement } from "@/types";

// canvas-confetti 类型定义（需要安装：npm install canvas-confetti @types/canvas-confetti）
// import confetti from "canvas-confetti";

const props = defineProps<{
  show: boolean;
  achievement: CelebrationAchievement | null;
  encouragement: Encouragement | null;
  animationEnabled: boolean;
}>();

const emit = defineEmits<{
  (e: "update:show", value: boolean): void;
  (e: "close"): void;
}>();

const progressPercent = computed(() => {
  if (!props.achievement) return 0;
  return Math.round(
    (props.achievement.completed_tasks /
      Math.max(props.achievement.total_tasks, 1)) *
      100,
  );
});

// 播放彩带动画（暂不启用，需安装 canvas-confetti）
// function playConfetti() {
//   if (!props.animationEnabled) return;
//   confetti({ particleCount: 100, spread: 70, origin: { x: 0.1, y: 0.9 } });
//   setTimeout(() => {
//     confetti({ particleCount: 100, spread: 70, origin: { x: 0.9, y: 0.9 } });
//   }, 200);
//   setTimeout(() => {
//     confetti({ particleCount: 150, spread: 100, origin: { x: 0.5, y: 0.6 } });
//   }, 400);
// }

// 弹窗显示时触发动画（暂不启用）
// watch(
//   () => props.show,
//   (newVal) => {
//     if (newVal) {
//       setTimeout(playConfetti, 300);
//     }
//   }
// );

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
      style="width: 520px; max-width: 90vw"
      :bordered="false"
      class="rounded-xl overflow-hidden"
      content-style="padding: 0"
    >
      <!-- 顶部发光效果 -->
      <div
        class="h-2 bg-gradient-to-r from-amber-400 via-emerald-400 to-blue-400"
      />

      <div class="p-6">
        <!-- 标题区 -->
        <div class="text-center mb-6">
          <div class="flex justify-center mb-3">
            <div
              class="w-16 h-16 rounded-full bg-gradient-to-br from-amber-100 to-amber-200 flex items-center justify-center"
            >
              <Icon icon="mdi:trophy" width="36" class="text-amber-500" />
            </div>
          </div>
          <h2 class="text-2xl font-bold text-gray-800 mb-2">全部目标完成！</h2>
          <p class="text-gray-500">
            {{ encouragement?.text ?? "这一刻属于坚持的你。" }}
          </p>
        </div>

        <!-- 成就回顾 -->
        <div v-if="achievement" class="grid grid-cols-3 gap-4 mb-6">
          <div class="text-center p-3 rounded-lg bg-gray-50">
            <div class="text-2xl font-bold text-brand-500">
              {{ achievement.days_elapsed }}
            </div>
            <div class="text-xs text-gray-500 mt-1">天耗时</div>
          </div>
          <div class="text-center p-3 rounded-lg bg-gray-50">
            <div class="text-2xl font-bold text-emerald-500">
              {{ achievement.completed_tasks }}
            </div>
            <div class="text-xs text-gray-500 mt-1">任务完成</div>
          </div>
          <div class="text-center p-3 rounded-lg bg-gray-50">
            <div class="text-2xl font-bold text-amber-500">
              {{ achievement.final_longest_streak }}
            </div>
            <div class="text-xs text-gray-500 mt-1">最长连续</div>
          </div>
        </div>

        <!-- 进度条 -->
        <NProgress
          v-if="achievement"
          type="line"
          :percentage="progressPercent"
          :show-indicator="false"
          :height="8"
          class="mb-6"
        />

        <!-- 庆祝文案 -->
        <div class="text-center mb-4">
          <p class="text-sm text-gray-500">你用行动证明了：能坚持。</p>
        </div>

        <!-- 操作按钮 -->
        <div class="flex justify-center">
          <NButton type="primary" size="large" @click="handleClose">
            继续前行
          </NButton>
        </div>
      </div>
    </NCard>
  </NModal>
</template>
