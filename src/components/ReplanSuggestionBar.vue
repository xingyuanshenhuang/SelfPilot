<script setup lang="ts">
/**
 * 规划建议条（阶段 4）
 *
 * 数据源：replan_check_triggers 返回的 TriggerReport。
 * 仅展示 decision=proceed 的目标（AutoApply 已由后端直接应用，Suppressed 被网关抑制），
 * 点击「立即规划」打开该目标的重新规划抽屉（携带触发原因徽标）。
 */
import { computed } from "vue";
import { NButton, NTag, NTooltip, NSpin } from "naive-ui";
import { Icon } from "@iconify/vue";
import type { TriggerGoalResult } from "@/types";

const props = defineProps<{
  goals: TriggerGoalResult[];
  loading?: boolean;
}>();

const emit = defineEmits<{
  (e: "plan", goalId: string): void;
  (e: "dismiss", goalId: string): void;
  (e: "refresh"): void;
}>();

/** 触发类型 → 中文标签 + 图标 */
const triggerMeta: Record<
  string,
  { label: string; icon: string }
> = {
  task_completed: { label: "任务完成", icon: "mdi:check-circle-outline" },
  task_skipped: { label: "任务跳过", icon: "mdi:skip-next-outline" },
  overdue_accumulated: { label: "逾期累积", icon: "mdi:clock-alert-outline" },
  goal_deadline_changed: { label: "截止变动", icon: "mdi:calendar-edit" },
  goal_total_changed: { label: "总量变动", icon: "mdi:counter" },
  progress_lag: { label: "进度滞后", icon: "mdi:trending-down" },
  load_overload: { label: "负载过载", icon: "mdi:gauge" },
  periodic_review: { label: "周期复评", icon: "mdi:calendar-sync-outline" },
  manual_request: { label: "手动请求", icon: "mdi:gesture-tap" },
};

function meta(type: string) {
  return triggerMeta[type] ?? { label: type, icon: "mdi:bell-outline" };
}

/** 严重度 → NTag type */
function severityType(sev: string): "info" | "warning" | "error" {
  if (sev === "critical") return "error";
  if (sev === "info") return "info";
  return "warning";
}

const items = computed(() =>
  props.goals.filter((g) => g.decision === "proceed"),
);
</script>

<template>
  <div
    v-if="loading || items.length > 0"
    class="rounded-lg border border-amber-200 dark:border-amber-500/30 bg-amber-50/70 dark:bg-amber-500/10 px-3 py-2.5"
  >
    <div class="flex items-center gap-2 mb-1.5">
      <Icon
        icon="mdi:lightbulb-on-outline"
        width="16"
        class="text-amber-500 shrink-0"
      />
      <span class="text-sm font-medium text-amber-700 dark:text-amber-300">
        规划提醒
      </span>
      <span class="text-xs text-amber-600/80 dark:text-amber-400/80">
        {{ items.length }} 个目标有新变化
      </span>
      <NSpin v-if="loading" :size="12" class="ml-1" />
      <NButton
        size="tiny"
        quaternary
        class="ml-auto"
        :loading="loading"
        @click="emit('refresh')"
      >
        <template #icon><Icon icon="mdi:refresh" /></template>
        刷新
      </NButton>
    </div>

    <div class="space-y-1">
      <div
        v-for="g in items"
        :key="g.goal_id"
        class="flex items-center gap-2 text-xs bg-white/70 dark:bg-black/10 rounded px-2 py-1.5"
      >
        <Icon
          :icon="meta(g.trigger_type).icon"
          width="14"
          class="text-amber-500 shrink-0"
        />
        <span
          class="font-medium text-gray-700 dark:text-gray-200 truncate max-w-[180px]"
        >
          {{ g.goal_name }}
        </span>
        <NTag size="tiny" :bordered="false" :type="severityType(g.severity)">
          {{ meta(g.trigger_type).label }}
        </NTag>
        <NTooltip placement="top" style="max-width: 360px">
          <template #trigger>
            <span class="text-gray-500 dark:text-gray-400 truncate flex-1 min-w-0">
              {{ g.reason || g.suppress_reason || "计划需要调整" }}
            </span>
          </template>
          {{ g.reason || g.suppress_reason || "计划需要调整" }}
        </NTooltip>
        <span
          v-if="g.impact_ratio !== null"
          class="text-gray-400 shrink-0"
        >
          影响 {{ Math.round(g.impact_ratio * 100) }}%
        </span>
        <NButton
          size="tiny"
          type="warning"
          ghost
          class="shrink-0"
          @click="emit('plan', g.goal_id)"
        >
          立即规划
        </NButton>
        <NButton
          size="tiny"
          quaternary
          class="shrink-0"
          @click="emit('dismiss', g.goal_id)"
        >
          忽略
        </NButton>
      </div>
    </div>
  </div>
</template>
