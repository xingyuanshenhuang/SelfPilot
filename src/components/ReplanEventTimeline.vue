<script setup lang="ts">
/**
 * 重新规划历史时间线（阶段3B）
 *
 * 展示某目标的 replan_events（created_at DESC），可展开事件差异明细，支持撤销
 * 最近一次已应用事件。撤销三条件（最新 applied + 其后无任务变更 + 目标仍存在）
 * 由后端校验为准，前端仅按事件顺序对最新 applied 放行。
 */
import { ref, watch, computed, onMounted } from "vue";
import {
  NModal,
  NCard,
  NButton,
  NTag,
  NEmpty,
  NSpin,
  NTooltip,
  NDivider,
  useMessage,
} from "naive-ui";
import { Icon } from "@iconify/vue";
import { format, parseISO, formatDistanceToNow } from "date-fns";
import { zhCN } from "date-fns/locale";
import { useReplanStore } from "@/stores/replanStore";
import { toErrorMessage } from "@/api/client";
import type {
  Goal,
  ReplanEvent,
  ReplanEventDiff,
  ReplanEventDetail,
  ReplanChangeType,
} from "@/types";

const props = defineProps<{
  goal: Goal | null;
  show: boolean;
}>();

const emit = defineEmits<{
  (e: "update:show", value: boolean): void;
  (e: "success"): void;
}>();

const message = useMessage();
const store = useReplanStore();

const events = ref<ReplanEvent[]>([]);
const loading = ref(false);
const expandedId = ref<string | null>(null);
const detail = ref<ReplanEventDetail | null>(null);
const detailLoading = ref(false);
const reverting = ref<string | null>(null);

function relativeTime(ts: string | null): string {
  if (!ts) return "—";
  try {
    return formatDistanceToNow(parseISO(ts), { addSuffix: true, locale: zhCN });
  } catch {
    return ts;
  }
}
function fmt(ts: string | null): string {
  if (!ts) return "—";
  try {
    return format(parseISO(ts), "yyyy-MM-dd HH:mm");
  } catch {
    return ts;
  }
}

/** 触发徽标 */
function triggerTag(ev: ReplanEvent): {
  text: string;
  type: "info" | "warning" | "success" | "error" | "default";
} {
  const t = ev.trigger_type ?? "";
  if (!t) return { text: "手动", type: "default" };
  const map: Record<
    string,
    { text: string; type: "info" | "warning" | "success" | "error" | "default" }
  > = {
    deadline_elapsed: { text: "临近截止", type: "warning" },
    overloaded: { text: "负载过载", type: "error" },
    overdue: { text: "逾期回收", type: "error" },
    manual: { text: "手动", type: "info" },
  };
  return map[t] ?? { text: t, type: "info" };
}

const statusTag = (s: string | null) => {
  switch (s) {
    case "applied":
      return { text: "已应用", type: "success" as const };
    case "previewed":
      return { text: "仅预览", type: "default" as const };
    case "reverted":
      return { text: "已撤销", type: "default" as const };
    default:
      return { text: s ?? "未知", type: "default" as const };
  }
};

/** 最新一个可撤销的已应用事件 id */
const revertableId = computed(() => events.value.find((e) => e.status === "applied")?.id ?? null);

async function load() {
  if (!props.goal) return;
  loading.value = true;
  try {
    events.value = await store.fetchEvents(50, 0);
  } catch (e) {
    message.error(toErrorMessage(e));
  } finally {
    loading.value = false;
  }
}

watch(
  () => props.show,
  async (v) => {
    if (v) {
      detail.value = null;
      expandedId.value = null;
      await load();
    }
  },
);
onMounted(() => {
  if (props.show) load();
});

async function toggleExpand(id: string) {
  if (!expandedId.value) {
    expandedId.value = id;
    detailLoading.value = true;
    try {
      detail.value = await store.fetchEventDetail(id);
    } catch (e) {
      message.error(toErrorMessage(e));
    } finally {
      detailLoading.value = false;
    }
  } else {
    expandedId.value = null;
    detail.value = null;
  }
}

const allDiffs = computed(() => detail.value?.diffs ?? []);
const diffChangeIcon = (t: ReplanChangeType | null): string => {
  switch (t) {
    case "date_moved": return "mdi:calendar-move";
    case "qty_changed": return "mdi:tune-variant";
    case "both": return "mdi:swap-horizontal-bold";
    case "retained": return "mdi:check";
    case "locked": return "mdi:lock";
    case "new": return "mdi:plus-circle-outline";
    default: return "mdi:dots-horizontal";
  }
};
const diffChangeText = (t: ReplanChangeType | null): string =>
  t ?? "—";

async function handleRevert(id: string) {
  if (reverting.value) return;
  reverting.value = id;
  try {
    await store.revert(id);
    message.success("已撤销此次重新规划，任务已回滚");
    emit("success");
    emit("update:show", false);
  } catch (e) {
    message.error(toErrorMessage(e));
  } finally {
    reverting.value = null;
  }
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
      :bordered="false"
      class="rounded-xl"
      style="max-width: 640px; width: 92vw"
      content-style="max-height:72vh; overflow-y:auto"
    >
      <template #header>
        <div class="flex items-center gap-2">
          <div
            class="w-9 h-9 rounded-lg bg-brand-500/10 text-brand-500 flex items-center justify-center"
          >
            <Icon icon="mdi:history" width="20" />
          </div>
          <div>
            <div class="text-base font-medium">重新规划历史</div>
            <div class="text-xs text-gray-500 dark:text-gray-400">
              {{ goal?.name }}
            </div>
          </div>
        </div>
      </template>

      <div v-if="loading" class="flex justify-center py-12">
        <NSpin />
      </div>

      <div v-else-if="events.length === 0" class="py-10">
        <NEmpty description="该目标暂无重新规划历史" />
      </div>

      <div v-else class="space-y-2">
        <div
          v-for="(ev, ei) in events"
          :key="ev.id ?? ei"
          class="rounded-lg border border-gray-100 dark:border-surface-muted overflow-hidden"
        >
          <button
            class="w-full flex items-center justify-between gap-2 px-3 py-2.5 text-left hover:bg-gray-50 dark:hover:bg-surface-muted transition-colors"
            @click="toggleExpand(ev.id!)"
          >
            <div class="flex items-center gap-2 min-w-0">
              <NTag size="tiny" :bordered="false" :type="triggerTag(ev).type">
                {{ triggerTag(ev).text }}
              </NTag>
              <NTag size="tiny" :bordered="false" :type="statusTag(ev.status).type">
                {{ statusTag(ev.status).text }}
              </NTag>
              <span class="text-sm text-gray-700 dark:text-gray-200 truncate">
                {{ relativeTime(ev.created_at) }}
              </span>
            </div>
            <div class="flex items-center gap-2 shrink-0">
              <NTooltip
                v-if="revertableId === ev.id"
                placement="top"
                content="撤销最近一次已应用的规划（最新的已应用事件，其后无任务变更）"
              >
                <NButton
                  size="tiny"
                  type="warning"
                  ghost
                  :loading="reverting === ev.id"
                  @click.stop="handleRevert(ev.id!)"
                >
                  <template #icon><Icon icon="mdi:undo-variant" /></template>
                  撤销
                </NButton>
              </NTooltip>
              <Icon
                icon="mdi:chevron-down"
                class="text-gray-400 transition-transform"
                :class="{ 'rotate-180': expandedId === ev.id }"
              />
            </div>
          </button>

          <div v-if="expandedId === ev.id" class="border-t border-gray-100 dark:border-surface-muted px-3 py-3 space-y-2 bg-gray-50/60 dark:bg-surface-muted/40">
            <div class="text-xs text-gray-500 dark:text-gray-400 space-y-1">
              <div><span class="text-gray-400">创建</span> {{ fmt(ev.created_at) }}</div>
              <div v-if="ev.applied_at">
                <span class="text-gray-400">应用</span> {{ fmt(ev.applied_at) }}
              </div>
              <div v-if="ev.after_summary" class="flex items-start gap-1">
                <Icon icon="mdi:information-outline" width="13" class="mt-0.5 shrink-0" />
                {{ ev.after_summary }}
              </div>
            </div>

            <NDivider class="!my-1.5" />

            <div v-if="detailLoading" class="flex justify-center py-6">
              <NSpin size="small" />
            </div>
            <div v-else-if="allDiffs.length === 0" class="text-xs text-gray-400 py-3">
              无差异明细
            </div>
            <div v-else class="space-y-1">
              <div
                v-for="(d, di) in allDiffs"
                :key="d.id ?? di"
                class="flex items-center gap-2 text-xs"
              >
                <Icon
                  :icon="diffChangeIcon(d.change_type)"
                  width="14"
                  class="shrink-0 text-gray-400"
                />
                <span class="flex-1 truncate text-gray-700 dark:text-gray-200">
                  {{ d.task_id }}
                </span>
                <span class="text-gray-400">{{ diffChangeText(d.change_type) }}</span>
                <span class="shrink-0">
                  {{ d.old_plan_date ?? "—" }} <Icon icon="mdi:arrow-right" width="13" class="inline-block" /> {{ d.new_plan_date ?? "—" }}
                  <template v-if="d.old_plan_qty !== null || d.new_plan_qty !== null">
                    （{{ d.old_plan_qty ?? "—" }}→{{ d.new_plan_qty ?? "—" }}）
                  </template>
                </span>
              </div>
            </div>
          </div>
        </div>
      </div>
    </NCard>
  </NModal>
</template>