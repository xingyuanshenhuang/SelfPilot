<script setup lang="ts">
/**
 * 重新规划预览抽屉（阶段3B 主交互）
 *
 * 流程：打开 → preview() 求解 → 摘要卡 / what-if 参数条 / 影响日历 / 差异表
 * → 用户调 what-if 或勾选排除项 → 防抖 400ms 重新求解（plan_hash 保持一致）
 * → 可行时"应用规划"提交（含 UNDO 预告）→ emit success 由父级刷新目标树。
 * 不可行时不抛错，展示 InfeasibilityDialog 四候选。
 */
import {
  computed,
  ref,
  watch,
  onBeforeUnmount,
  h,
} from "vue";
import {
  NDrawer,
  NDrawerContent,
  NButton,
  NTooltip,
  NTag,
  NDataTable,
  NCheckbox,
  NDatePicker,
  NInputNumber,
  NSwitch,
  NSpace,
  NDivider,
  NSpin,
  NAlert,
  useMessage,
} from "naive-ui";
import type { DataTableColumns } from "naive-ui";
import { Icon } from "@iconify/vue";
import { format, parseISO, differenceInCalendarDays } from "date-fns";
import { useReplanStore } from "@/stores/replanStore";
import { toErrorMessage } from "@/api/client";
import type { Goal, TaskPlacement, ReplanChangeType } from "@/types";
import InfeasibilityDialog from "./InfeasibilityDialog.vue";

const props = defineProps<{
  goal: Goal | null;
  show: boolean;
}>();

const emit = defineEmits<{
  (e: "update:show", value: boolean): void;
  (e: "success"): void;
  (e: "open-history"): void;
}>();

const message = useMessage();
const store = useReplanStore();

// ===== 打开/关闭 =====
const drawerWidth = Math.min(1080, window.innerWidth * 0.92);
/** what-if 截止日期（NDatePicker 用时间戳，与 store 的 yyyy-MM-dd 字符串互转） */
const deadlineTs = ref<number | null>(null);
watch(deadlineTs, (v) => {
  store.overrides.deadline = v ? format(new Date(v), "yyyy-MM-dd") : null;
});
watch(
  () => props.show,
  async (v) => {
    if (v && props.goal) {
      deadlineTs.value = null;
      store.open(props.goal.id).catch((e) => message.error(toErrorMessage(e)));
    }
  },
);

// ===== what-if 数字/布尔字段模型（NDatePicker/NInputNumber/NSwitch 用计算属性）=====
const dailyCapacityModel = computed<number | null>({
  get: () => store.overrides.daily_capacity ?? null,
  set: (v) => {
    store.overrides.daily_capacity = v ?? undefined;
  },
});
const intensityModel = computed<number | null>({
  get: () => store.overrides.intensity ?? null,
  set: (v) => {
    store.overrides.intensity = v ?? undefined;
  },
});
const respectLocksModel = computed<boolean>({
  get: () => !!store.overrides.respect_locks,
  set: (v) => {
    store.overrides.respect_locks = v;
  },
});

// ===== what-if 防抖重算 =====
const paramsSig = computed(() => {
  const o = store.overrides;
  return [
    o.deadline ?? "",
    String(o.daily_capacity ?? ""),
    String(o.intensity ?? ""),
    String(o.respect_locks ?? ""),
    store.excludedTaskIds.join(","),
  ].join("|");
});

let timer: ReturnType<typeof setTimeout> | null = null;
watch(paramsSig, () => {
  if (!props.show || !store.goalId) return;
  if (timer) clearTimeout(timer);
  timer = setTimeout(() => {
    store
      .preview()
      .catch((e) => message.error(toErrorMessage(e)));
    timer = null;
  }, 400);
});
onBeforeUnmount(() => {
  if (timer) clearTimeout(timer);
});

// ===== 摘要 =====
const feasible = computed(() => !!store.plan?.feasibility.feasible);
const feasibilityBadge = computed(() => {
  const f = store.plan?.feasibility;
  if (!f) return null;
  if (f.feasible)
    return f.level === "tight"
      ? {
          text: "可行（紧张）",
          cls: "bg-orange-50 dark:bg-orange-500/15 text-orange-600 dark:text-orange-400",
        }
      : {
          text: "可行",
          cls: "bg-green-50 dark:bg-green-500/15 text-green-700 dark:text-green-400",
        };
  return {
    text: "不可行",
    cls: "bg-red-50 dark:bg-red-500/15 text-red-600 dark:text-red-400",
  };
});

const slackText = computed(() => {
  const f = store.plan?.feasibility;
  if (!f) return "—";
  return `${Math.round(f.slack * 100)}%`;
});

const predictedFinish = computed(() => {
  const m = store.plan?.metrics;
  if (!m?.predicted_finish) return "—";
  return m.predicted_finish;
});

// ===== 排除项 =====
function toggleExclude(taskId: string, excluded: boolean) {
  const set = new Set(store.excludedTaskIds);
  if (excluded) set.add(taskId);
  else set.delete(taskId);
  store.excludedTaskIds = [...set];
}

const placements = computed(() => store.plan?.placements ?? []);
const excludedCount = computed(() => store.excludedTaskIds.length);

// ===== 影响日历 =====
const maxLoad = computed(() => {
  let m = 0;
  for (const d of store.plan?.day_load ?? []) {
    m = Math.max(m, d.before_qty, d.after_qty);
  }
  return m;
});
const dayLoad = computed(() => store.plan?.day_load ?? []);

/** before/after qty → 色阶（绿<黄<红，阈值参考 CalendarMonthView 4/7） */
function qtyColor(qty: number): string {
  if (qty <= 4) return "bg-green-500";
  if (qty <= 7) return "bg-yellow-500";
  return "bg-red-500";
}

// ===== 差异表 =====
const todayStr = format(new Date(), "yyyy-MM-dd");
const isOverdue = (date: string | null) =>
  !!date && date < todayStr;

function changeIcon(type: ReplanChangeType): string {
  switch (type) {
    case "date_moved":
      return "mdi:calendar-move";
    case "qty_changed":
      return "mdi:tune-variant";
    case "both":
      return "mdi:swap-horizontal-bold";
    case "retained":
      return "mdi:check";
    case "locked":
      return "mdi:lock";
    case "new":
      return "mdi:plus-circle-outline";
  }
}
function changeCls(type: ReplanChangeType): string {
  switch (type) {
    case "date_moved":
    case "both":
      return "text-primary-500";
    case "qty_changed":
      return "text-warning-600 dark:text-warning-400";
    case "retained":
      return "text-success-500";
    case "locked":
      return "text-gray-400";
    case "new":
      return "text-info";
  }
}
function reasonTag(
  row: TaskPlacement,
): { text: string; type: "info" | "warning" | "success" | "error" | "default" } {
  const s = `${row.reason} ${row.reason_text}`;
  if (row.locked || /lock/i.test(s)) return { text: "锁定", type: "default" };
  if (/依赖/i.test(s) || /dep/i.test(s)) return { text: "依赖下界", type: "info" };
  if (/逾期/i.test(s) || /overdue/i.test(s)) return { text: "逾期回收", type: "error" };
  if (/均衡/i.test(s) || /balance/i.test(s)) return { text: "负载均衡", type: "warning" };
  if (/节奏/i.test(s) || /profile|pattern/i.test(s)) return { text: "节奏画像", type: "info" };
  return { type: "default", text: row.reason_text || row.reason };
}

const columns: DataTableColumns<TaskPlacement> = [
  {
    title: "操作",
    key: "op",
    width: 56,
    fixed: "left",
    render: (r) =>
      h(NCheckbox, {
        checked: !store.excludedTaskIds.includes(r.task_id),
        "onUpdate:checked": (v: boolean) => toggleExclude(r.task_id, !v),
      }),
  },
  {
    title: "状态",
    key: "change_type",
    width: 44,
    render: (r) =>
      h(NTooltip, { placement: "right" }, {
        trigger: () =>
          h("span", { class: "text-lg leading-none " + changeCls(r.change_type) }, [
            h(Icon, { icon: changeIcon(r.change_type) }),
          ]),
        default: () => r.change_type,
      }),
  },
  {
    title: "原日期",
    key: "old_plan_date",
    width: 96,
    render: (r) => {
      if (!r.old_plan_date) return "—";
      const overdue = isOverdue(r.old_plan_date);
      const label = r.old_plan_date;
      if (!overdue) return label;
      const days = differenceInCalendarDays(
        new Date(),
        parseISO(r.old_plan_date),
      );
      return h("span", { class: "text-red-500" }, [
        label,
        h(
          "sup",
          { class: "ml-0.5 text-red-500" },
          `+${days}`,
        ),
      ]);
    },
  },
  {
    title: "新日期",
    key: "new_plan_date",
    width: 96,
    render: (r) => {
      const changed =
        r.old_plan_date !== r.new_plan_date && !!r.new_plan_date;
      if (!r.new_plan_date) return "—";
      return h(
        "span",
        { class: changed ? "text-primary-500" : "text-gray-400" },
        r.new_plan_date,
      );
    },
  },
  {
    title: "任务",
    key: "name",
    minWidth: 180,
    ellipsis: { tooltip: true },
  },
  {
    title: "原计划",
    key: "old_plan_qty",
    width: 84,
    align: "right",
    render: (r) => `${r.old_plan_qty}${r.unit}`,
  },
  {
    title: "新计划",
    key: "new_plan_qty",
    width: 104,
    align: "right",
    render: (r) => {
      if (r.new_plan_qty === r.old_plan_qty)
        return `${r.new_plan_qty}${r.unit}`;
      const diff = Math.round(r.new_plan_qty - r.old_plan_qty);
      const s = diff > 0 ? `+${diff}` : `${diff}`;
      return h("span", null, [
        `${r.new_plan_qty}${r.unit}`,
        h(
          "sup",
          { class: "ml-1 text-primary-500" },
          `(${s})`,
        ),
      ]);
    },
  },
  {
    title: "理由",
    key: "reason",
    width: 100,
    render: (r) => {
      const t = reasonTag(r);
      return h(NTag, { size: "small", type: t.type, bordered: false }, {
        default: () => t.text,
      });
    },
  },
];

// ===== 警告 =====
const warnings = computed(() => store.plan?.warnings ?? []);
const warningBadge = computed<{ cls: string; text: string } | null>(() => {
  const kind = store.plan?.warnings[0]?.kind ?? "";
  const map: Record<string, { cls: string; text: string }> = {
    overflow_tail: {
      cls: "bg-orange-50 dark:bg-orange-500/15 text-orange-600 dark:text-orange-400",
      text: "部分任务顺延到截止后",
    },
    capacity_tight: {
      cls: "bg-yellow-50 dark:bg-yellow-500/15 text-yellow-600 dark:text-yellow-500",
      text: "容量紧张",
    },
    cycle_detected: {
      cls: "bg-red-50 dark:bg-red-500/15 text-red-600 dark:text-red-400",
      text: "检测到依赖环",
    },
    iteration_capped: {
      cls: "bg-gray-50 dark:bg-surface-muted text-gray-600 dark:text-gray-400",
      text: "迭代达上限",
    },
    low_fairness: {
      cls: "bg-gray-50 dark:bg-surface-muted text-gray-600 dark:text-gray-400",
      text: "公平性偏低",
    },
  };
  return map[kind] ?? null;
});

// ===== 提交 / 撤销 =====
const applying = computed(() => store.applying);
async function handleApply() {
  try {
    const res = await store.commit();
    if (!res) return;
    message.success(
      `已应用重新规划：调整 ${res.moved + res.qty_changed} 项，保留 ${res.retained} 项`,
      { duration: 3000 },
    );
    emit("success");
  } catch (e) {
    message.error(toErrorMessage(e));
  }
}
async function handleRevert() {
  const ev = store.lastApply?.event_id;
  if (!ev) return;
  try {
    await store.revert(ev);
    message.success("已撤销最近的重新规划");
    emit("success");
  } catch (e) {
    message.error(toErrorMessage(e));
  }
}

// ===== 不可行弹窗 =====
const acceptedRisk = ref(false);
const showInfeasible = computed(
  () => !!store.plan && !store.plan.feasibility.feasible && !acceptedRisk.value,
);
/** 应用按钮是否可用：可行 或 已接受风险 */
const canApply = computed(() => feasible.value || acceptedRisk.value);

function onAdopt(
  kind: string,
  payload: Record<string, unknown>,
) {
  if (kind === "extend_deadline") {
    const ds =
      typeof payload.new_deadline === "string"
        ? payload.new_deadline
        : typeof payload.deadline === "string"
          ? payload.deadline
          : null;
    if (ds) {
      store.overrides.deadline = ds;
      deadlineTs.value = parseISO(ds).getTime();
    }
  } else if (kind === "raise_intensity") {
    if (typeof payload.intensity === "number") {
      store.overrides.intensity = Math.round(payload.intensity * 10) / 10;
    }
  }
  // trim_scope：裁剪提示由后端给出 count，前端通过差异表手动排除操作达成
}
function onAcceptRisk() {
  acceptedRisk.value = true;
}
</script>

<template>
  <NDrawer
    :show="show"
    :width="drawerWidth"
    placement="right"
    @update:show="emit('update:show', $event)"
  >
    <NDrawerContent
      :title="undefined"
      closable
      :native-scrollbar="false"
      body-content-style="padding:0"
    >
      <div class="h-full flex flex-col">
        <!-- 头部 -->
        <div
          class="px-5 py-4 border-b border-gray-100 dark:border-surface-muted flex items-center justify-between shrink-0"
        >
          <div class="flex items-center gap-2 min-w-0">
            <div
              class="w-9 h-9 rounded-lg bg-brand-500/10 text-brand-500 flex items-center justify-center"
            >
              <Icon icon="mdi:refresh-circle" width="22" />
            </div>
            <div class="min-w-0">
              <div class="flex items-center gap-2">
                <span class="text-base font-medium truncate">重新规划</span>
                <NTag
                  v-if="store.plan?.trigger"
                  size="tiny"
                  type="info"
                  :bordered="false"
                >
                  {{ store.plan.trigger }}
                </NTag>
              </div>
              <div class="text-xs text-gray-500 dark:text-gray-400 truncate">
                {{ goal?.name }}
              </div>
            </div>
          </div>
          <NButton size="small" quaternary @click="emit('open-history')">
            <template #icon><Icon icon="mdi:history" /></template>
            规划历史
          </NButton>
        </div>

        <!-- 主体滚动区 -->
        <div class="flex-1 overflow-y-auto px-5 py-4 space-y-4">
          <div v-if="store.loading" class="flex justify-center py-16">
            <NSpin size="large" />
          </div>

          <template v-else-if="store.plan">
            <!-- 摘要卡 -->
            <div
              class="rounded-xl border border-gray-100 dark:border-surface-muted p-4"
            >
              <div class="flex items-start justify-between gap-4">
                <div class="grid grid-cols-3 sm:grid-cols-4 gap-4 flex-1">
                  <div>
                    <div class="text-xs text-gray-400">剩余工作量 W</div>
                    <div class="text-lg font-semibold">
                      {{ store.plan.feasibility.workload }}
                    </div>
                  </div>
                  <div>
                    <div class="text-xs text-gray-400">可用容量 C</div>
                    <div class="text-lg font-semibold">
                      {{ store.plan.feasibility.capacity }}
                    </div>
                  </div>
                  <div>
                    <div class="text-xs text-gray-400">裕度</div>
                    <div class="text-lg font-semibold">{{ slackText }}</div>
                  </div>
                  <div>
                    <div class="text-xs text-gray-400">预计完成</div>
                    <div class="text-lg font-semibold">
                      {{ predictedFinish }}
                    </div>
                  </div>
                </div>
                <NTag
                  v-if="feasibilityBadge"
                  :bordered="false"
                  class="!px-2.5 !py-1 shrink-0"
                  :class="feasibilityBadge.cls"
                >
                  {{ feasibilityBadge.text }}
                </NTag>
              </div>
              <NDivider class="!my-3" />
              <div class="flex flex-wrap gap-x-5 gap-y-1 text-xs text-gray-500 dark:text-gray-400">
                <span>调整日期 {{ store.plan.metrics.moved }}</span>
                <span>调整数量 {{ store.plan.metrics.qty_changed }}</span>
                <span>保留 {{ store.plan.metrics.retained }}</span>
                <span>锁定 {{ store.plan.metrics.locked }}</span>
                <span>逾期回收 {{ store.plan.metrics.overdue_recovered }}</span>
                <span v-if="store.plan.metrics.jain_index !== null">
                  公平指数 {{ store.plan.metrics.jain_index.toFixed(2) }}
                </span>
              </div>
            </div>

            <!-- 告警 -->
            <div
              v-if="warningBadge || warnings.length > 0"
              class="space-y-2"
            >
              <NAlert
                v-if="warningBadge"
                type="warning"
                :title="warningBadge.text"
                :show-icon="true"
                :bordered="false"
              />
              <div
                v-for="(w, i) in warnings"
                :key="i"
                class="text-xs text-gray-500 dark:text-gray-400"
              >
                {{ w.message }}
              </div>
            </div>

            <!-- what-if 参数条 -->
            <div
              class="rounded-xl border border-gray-100 dark:border-surface-muted p-4 space-y-3"
            >
              <div class="text-sm font-medium flex items-center gap-1">
                <Icon
                  icon="mdi:tune-variant"
                  class="text-brand-500"
                  width="16"
                />
                假设调整（What-if）
              </div>
              <div class="flex flex-wrap gap-x-6 gap-y-3 items-center">
                <div class="flex items-center gap-2">
                  <span class="text-xs text-gray-500 w-14">截止日期</span>
                  <NDatePicker
                    v-model:value="deadlineTs"
                    type="date"
                    clearable
                    size="small"
                    :placeholder="goal?.deadline ?? '不调整'"
                    style="width: 150px"
                    :is-date-disabled="
                      (ts: number) => ts < new Date().getTime() - 86400000
                    "
                  />
                </div>
                <div class="flex items-center gap-2">
                  <span class="text-xs text-gray-500 w-14">每日容量</span>
                  <NInputNumber
                    v-model:value="dailyCapacityModel"
                    :min="0"
                    :step="0.5"
                    size="small"
                    clearable
                    placeholder="默认"
                    style="width: 110px"
                  />
                </div>
                <div class="flex items-center gap-2">
                  <span class="text-xs text-gray-500 w-14">强度倍率</span>
                  <NInputNumber
                    v-model:value="intensityModel"
                    :min="0"
                    :step="0.1"
                    size="small"
                    clearable
                    placeholder="默认"
                    style="width: 110px"
                  />
                </div>
                <div class="flex items-center gap-2">
                  <span class="text-xs text-gray-500 w-14">尊重锁定</span>
                  <NSwitch
                    v-model:value="respectLocksModel"
                    size="small"
                  />
                </div>
              </div>
              <div class="text-xs text-gray-400 dark:text-gray-500 flex items-center gap-1">
                <Icon icon="mdi:information-outline" width="14" />
                调整后自动重算（已排除 {{ excludedCount }} 项）。仅影响本次预览，不写回数据库。
              </div>
            </div>

            <!-- 影响日历 -->
            <div
              v-if="dayLoad.length"
              class="rounded-xl border border-gray-100 dark:border-surface-muted p-4"
            >
              <div class="text-sm font-medium mb-2 flex items-center gap-2">
                <Icon
                  icon="mdi:calendar-range"
                  class="text-brand-500"
                  width="16"
                />
                影响日历（上=规划前，下=规划后）
                <span v-if="maxLoad > 0" class="text-xs text-gray-400 ml-auto">
                  色阶：绿 ≤4｜黄 5-7｜红 &gt;7
                </span>
              </div>
              <div class="flex flex-wrap gap-1">
                <NTooltip
                  v-for="d in dayLoad"
                  :key="d.date"
                  placement="top"
                >
                  <template #trigger>
                    <div
                      class="w-9 h-9 rounded relative overflow-hidden bg-gray-50 dark:bg-surface-muted"
                    >
                      <!-- 上三角：规划前 -->
                      <div
                        class="absolute inset-0"
                        :class="qtyColor(d.before_qty)"
                        style="clip-path: polygon(0 0, 100% 0, 0 100%)"
                      />
                      <!-- 下三角：规划后 -->
                      <div
                        class="absolute inset-0"
                        :class="qtyColor(d.after_qty)"
                        style="clip-path: polygon(100% 0, 100% 100%, 0 100%)"
                      />
                      <span
                        class="absolute inset-0 flex items-center justify-center text-[9px] text-white font-medium"
                        style="text-shadow: 0 1px 2px rgba(0,0,0,.6)"
                      >
                        {{ d.date.slice(5) }}
                      </span>
                    </div>
                  </template>
                  规划前 {{ d.before_qty }} → 规划后 {{ d.after_qty }}（{{ d.date }}）
                </NTooltip>
              </div>
            </div>

            <!-- 差异表 -->
            <div
              class="rounded-xl border border-gray-100 dark:border-surface-muted p-4"
            >
              <div class="text-sm font-medium mb-2">变更明细</div>
              <NDataTable
                :columns="columns"
                :data="placements"
                :max-height="340"
                :scroll-x="820"
                size="small"
                :bordered="false"
              />
            </div>
          </template>
        </div>

        <!-- 底部操作 -->
        <div
          class="shrink-0 px-5 py-3 border-t border-gray-100 dark:border-surface-muted flex items-center justify-between gap-3"
        >
          <div class="flex items-center gap-2 text-xs text-gray-500">
            <template v-if="store.lastApply">
              <NTag size="small" type="success" :bordered="false">
                已应用（event {{ store.lastApply.event_id.slice(0, 8) }}）
              </NTag>
              <NButton
                size="tiny"
                type="warning"
                ghost
                :loading="applying"
                @click="handleRevert"
              >
                <template #icon><Icon icon="mdi:undo-variant" /></template>
                撤销此次
              </NButton>
            </template>
            <template v-else>
              <NButton
                size="tiny"
                quaternary
                @click="emit('open-history')"
              >
                规划历史
              </NButton>
            </template>
          </div>
          <NSpace justify="end">
            <NButton @click="emit('update:show', false)">关闭</NButton>
            <NButton
              type="warning"
              :disabled="!canApply"
              :loading="applying"
              @click="handleApply"
            >
              <template #icon><Icon icon="mdi:play" /></template>
              应用规划
            </NButton>
          </NSpace>
        </div>
      </div>
    </NDrawerContent>
  </NDrawer>

  <InfeasibilityDialog
    v-if="store.plan && showInfeasible"
    :plan="store.plan"
    :goal-deadline="goal?.deadline ?? null"
    @adopt="onAdopt"
    @accept="onAcceptRisk"
  />
</template>