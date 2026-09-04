<script setup lang="ts">
import { computed, watch } from "vue";
import { NModal, NCard, NButton, NProgress } from "naive-ui";
import { Icon } from "@iconify/vue";
import confetti from "canvas-confetti";
import type { CelebrationAchievement, Encouragement } from "@/types";

// useWorker: false —— 禁用 canvas-confetti 的 blob Worker，
// 避免被严格 CSP（script-src 'self'，worker 回退到 script-src）拦截导致动画失效，
// 改为主线程 Canvas 渲染，同样流畅且无需放宽 CSP。
const confettiCannon = confetti.create(undefined, { useWorker: false });

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

// 播放彩带动画（F6：开启庆祝动画时触发，关闭则仅显示弹框）
function playConfetti() {
  if (!props.animationEnabled) return;
  // 五彩纸屑雨（从两侧底部喷出，再中央绽放）
  confettiCannon({ particleCount: 120, spread: 70, origin: { x: 0.1, y: 0.9 } });
  setTimeout(() => {
    confettiCannon({ particleCount: 120, spread: 70, origin: { x: 0.9, y: 0.9 } });
  }, 150);
  setTimeout(() => {
    confettiCannon({ particleCount: 180, spread: 100, scalar: 1.1, origin: { x: 0.5, y: 0.5 } });
  }, 300);
}

// 弹窗显示时触发动画
watch(
  () => props.show,
  (newVal) => {
    if (newVal) {
      setTimeout(playConfetti, 300);
    }
  },
);

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
              class="w-16 h-16 rounded-full bg-gradient-to-br from-amber-100 to-amber-200 dark:from-amber-500/25 dark:to-amber-500/10 flex items-center justify-center"
            >
              <Icon icon="mdi:trophy" width="36" class="text-amber-500" />
            </div>
          </div>
          <h2 class="text-2xl font-bold text-gray-800 dark:text-gray-100 mb-2">全部目标完成！</h2>
          <p class="text-gray-500 dark:text-gray-400">
            {{ encouragement?.text ?? "这一刻属于坚持的你。" }}
          </p>
        </div>

        <!-- 成就回顾 -->
        <div v-if="achievement" class="grid grid-cols-3 gap-4 mb-6">
          <div class="text-center p-3 rounded-lg bg-gray-50 dark:bg-surface-muted">
            <div class="text-2xl font-bold text-brand-500">
              {{ achievement.days_elapsed }}
            </div>
            <div class="text-xs text-gray-500 dark:text-gray-400 mt-1">天耗时</div>
          </div>
          <div class="text-center p-3 rounded-lg bg-gray-50 dark:bg-surface-muted">
            <div class="text-2xl font-bold text-emerald-500">
              {{ achievement.completed_tasks }}
            </div>
            <div class="text-xs text-gray-500 dark:text-gray-400 mt-1">任务完成</div>
          </div>
          <div class="text-center p-3 rounded-lg bg-gray-50 dark:bg-surface-muted">
            <div class="text-2xl font-bold text-amber-500">
              {{ achievement.final_longest_streak }}
            </div>
            <div class="text-xs text-gray-500 dark:text-gray-400 mt-1">最长连续</div>
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
          <p class="text-sm text-gray-500 dark:text-gray-400">你用行动证明了：能坚持。</p>
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
