<script setup lang="ts">
import { computed } from "vue";

const props = defineProps<{
  percentage: number; // 0 ~ 1
  size?: number;
  stroke?: number;
}>();

const size = computed(() => props.size ?? 60);
const stroke = computed(() => props.stroke ?? 6);
const radius = computed(() => (size.value - stroke.value) / 2);
const circumference = computed(() => 2 * Math.PI * radius.value);
const offset = computed(
  () => circumference.value * (1 - Math.min(Math.max(props.percentage, 0), 1)),
);
const displayPercent = computed(() => Math.round(props.percentage * 100));
const color = computed(() => {
  if (props.percentage >= 1) return "#67c23a";
  if (props.percentage >= 0.5) return "#409eff";
  if (props.percentage > 0) return "#e6a23c";
  return "#dcdfe6";
});
</script>

<template>
  <div
    class="progress-ring inline-flex items-center justify-center relative"
    :style="{ width: size + 'px', height: size + 'px' }"
  >
    <svg :width="size" :height="size">
      <circle
        :cx="size / 2"
        :cy="size / 2"
        :r="radius"
        fill="none"
        class="progress-ring-track"
        :stroke-width="stroke"
      />
      <circle
        :cx="size / 2"
        :cy="size / 2"
        :r="radius"
        fill="none"
        :stroke="color"
        :stroke-width="stroke"
        stroke-linecap="round"
        :stroke-dasharray="circumference"
        :stroke-dashoffset="offset"
        :transform="`rotate(-90 ${size / 2} ${size / 2})`"
        style="transition: stroke-dashoffset 0.3s ease"
      />
    </svg>
    <span class="absolute text-xs font-semibold text-gray-700 dark:text-gray-300">
      {{ displayPercent }}%
    </span>
  </div>
</template>

<style scoped>
/* 进度环底色轨，默认浅色，深色模式下使用半透明白以贴合暗色表面 */
.progress-ring-track {
  stroke: #e4e7ed;
}

:global(.dark) .progress-ring-track {
  stroke: rgba(255, 255, 255, 0.16);
}
</style>
