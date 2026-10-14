<script setup lang="ts">
/**
 * 重新规划不可行对话框（阶段3B）
 *
 * 展示不可行原因 + 四候选处置（extend_deadline / raise_intensity / trim_scope /
 * accept_risk）的量化后果文案。采纳候选 → emit adopt(kind, payload) 回填 what-if
 * 参数并重新求解；接受风险 → emit accept 直接放行应用。
 */
import {
  NModal,
  NCard,
  NTag,
  NButton,
  NEmpty,
  NSpace,
} from "naive-ui";
import { Icon } from "@iconify/vue";
import type {
  ReplanOption,
  ReplanOptionKind,
  ReplanPlan,
} from "@/types";

const props = defineProps<{
  plan: ReplanPlan;
  goalDeadline: string | null;
}>();

const emit = defineEmits<{
  (e: "adopt", kind: ReplanOptionKind, payload: Record<string, unknown>): void;
  (e: "accept"): void;
}>();

const fea = () => props.plan.feasibility;

/** 候选量化后果文案 */
function optionDetail(o: ReplanOption): string {
  const p = o.payload;
  switch (o.kind) {
    case "extend_deadline":
      return typeof p.days === "number"
        ? `截止日期顺延 ${p.days} 天`
        : typeof p.new_deadline === "string"
          ? `截止日期调整至 ${p.new_deadline}`
          : "顺延截止日期";
    case "raise_intensity":
      return typeof p.intensity === "number"
        ? `每日强度提升至 ×${p.intensity}`
        : "提高每日强度";
    case "trim_scope":
      return typeof p.count === "number"
        ? `裁减 ${p.count} 项任务`
        : "裁减部分任务";
    case "accept_risk":
      return "接受风险照常规划";
  }
  return o.label;
}

function reasonText(): string {
  const r = props.plan.feasibility.reason;
  return r === "capacity_shortfall"
    ? "剩余工作量超过可用容量"
    : r === "dependency_chain_too_long"
      ? "依赖链长度超过可用天数"
      : "当前设定下无法在截止前完成";
}

const iconFor = (kind: ReplanOptionKind): string => {
  switch (kind) {
    case "extend_deadline":
      return "mdi:calendar-arrow-right";
    case "raise_intensity":
      return "mdi:speedometer";
    case "trim_scope":
      return "mdi:content-cut";
    case "accept_risk":
      return "mdi:alert-outline";
  }
};
</script>

<template>
  <NModal :show="true" :mask-closable="true" :close-on-esc="true">
    <NCard
      :bordered="false"
      class="rounded-xl"
      style="width: 460px; max-width: 92vw"
    >
      <template #header>
        <div class="flex items-center gap-2">
          <div
            class="w-9 h-9 rounded-lg bg-red-500/10 text-red-500 flex items-center justify-center"
          >
            <Icon icon="mdi:alert-circle-outline" width="20" />
          </div>
          <span class="text-base font-medium">当前计划不可行</span>
        </div>
      </template>

      <div class="space-y-3">
        <div class="rounded bg-red-50 dark:bg-red-500/15 px-3 py-2 text-sm text-red-600 dark:text-red-400">
          <Icon icon="mdi:information-outline" width="14" class="inline-block mr-1" />
          {{ reasonText() }}（W {{ fea().workload }} / C {{ fea().capacity }}）
        </div>

        <div v-if="fea().options.length === 0" class="py-6">
          <NEmpty description="暂无可处置候选" :size="'small'" />
        </div>

        <div v-else class="space-y-2">
          <div
            v-for="o in fea().options"
            :key="o.kind"
            class="flex items-center justify-between gap-3 rounded-lg border border-gray-100 dark:border-surface-muted px-3 py-2.5"
          >
            <div class="flex items-center gap-2 min-w-0">
              <Icon
                :icon="iconFor(o.kind)"
                width="18"
                class="shrink-0"
                :class="
                  o.kind === 'accept_risk'
                    ? 'text-orange-500'
                    : 'text-brand-500'
                "
              />
              <div class="min-w-0">
                <div class="flex items-center gap-1.5">
                  <span class="text-sm font-medium">{{ o.label }}</span>
                  <NTag v-if="o.risky" size="tiny" type="warning" :bordered="false">
                    含风险
                  </NTag>
                </div>
                <div class="text-xs text-gray-500 dark:text-gray-400">
                  {{ optionDetail(o) }}
                </div>
              </div>
            </div>
            <NButton
              size="small"
              :type="o.kind === 'accept_risk' ? 'warning' : 'primary'"
              ghost
              @click="
                o.kind === 'accept_risk'
                  ? emit('accept')
                  : emit('adopt', o.kind, o.payload)
              "
            >
              {{ o.kind === "accept_risk" ? "接受并应用" : "采纳" }}
            </NButton>
          </div>
        </div>

        <div class="text-xs text-gray-400 dark:text-gray-500 flex items-start gap-1">
          <Icon icon="mdi:lightbulb-outline" width="14" class="mt-0.5 shrink-0" />
          采纳候选会将参数回填到"假设调整"，重新求解后若可行即可应用。
        </div>
      </div>
    </NCard>
  </NModal>
</template>