<script setup lang="ts">
/**
 * 智能拆解 (Beta) 对话框
 *
 * 流程：配置相关目标/约束 → 生成预览(smart_split_preview) → 渲染可行性/明细/摘要
 * → 可行时"确认拆解"落库(smart_split_apply) → emit success，由父级刷新目标树。
 */
import { ref, reactive, computed, watch } from "vue";
import {
  NModal,
  NCard,
  NButton,
  NFormItem,
  NInputNumber,
  NSelect,
  NDatePicker,
  NTag,
  NAlert,
  NDataTable,
  NSpin,
  NSpace,
  NDivider,
  useMessage,
} from "naive-ui";
import type { DataTableColumns, SelectOption } from "naive-ui";
import { format } from "date-fns";
import { Icon } from "@iconify/vue";
import { smartSplitPreview, smartSplitApply } from "@/api/splitPlanner";
import { toErrorMessage } from "@/api/client";
import type {
  Goal,
  PlanMode,
  SplitShape,
  SplitPlan,
  PlanItem,
  SmartSplitRequest,
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

// ===== 表单状态 =====
const form = reactive<{
  dateRange: [number, number] | null;
  total_qty: number | null;
  max_daily_qty: number | null;
  allowed_weekdays: number[];
  mode: PlanMode;
  shape: SplitShape;
  buffer_days: number;
}>({
  dateRange: null,
  total_qty: null,
  max_daily_qty: null,
  allowed_weekdays: [],
  mode: "auto",
  shape: "auto",
  buffer_days: 0,
});

const modeOptions: SelectOption[] = [
  { label: "自动决策", value: "auto" },
  { label: "返回候选", value: "assisted" },
];
const shapeOptions: SelectOption[] = [
  { label: "自动判定", value: "auto" },
  { label: "均匀分配", value: "even" },
  { label: "前重后轻", value: "front_loaded" },
  { label: "后重前轻", value: "back_loaded" },
  { label: "阶段化", value: "phased" },
  { label: "间隔重复", value: "spaced_repetition" },
  { label: "里程碑倒推", value: "milestone" },
];
const weekdayOptions: SelectOption[] = [
  { label: "周一", value: 1 },
  { label: "周二", value: 2 },
  { label: "周三", value: 3 },
  { label: "周四", value: 4 },
  { label: "周五", value: 5 },
  { label: "周六", value: 6 },
  { label: "周日", value: 0 },
];

// ===== 预览状态 =====
const previewing = ref(false);
const previewError = ref("");
const plan = ref<SplitPlan | null>(null);
const applying = ref(false);

function reset() {
  previewError.value = "";
  plan.value = null;
  previewing.value = false;
  applying.value = false;
}

/** 打开弹窗时按目标预填表单 */
watch(
  () => props.show,
  (v) => {
    if (!v || !props.goal) return;
    reset();
    const start = Date.now() + 86400000;
    const end = props.goal.deadline
      ? new Date(props.goal.deadline).getTime()
      : start + 86400000 * 6;
    form.dateRange = [start, end];
    form.total_qty = props.goal.total_qty > 0 ? props.goal.total_qty : null;
    form.max_daily_qty = null;
    form.allowed_weekdays = [];
    form.buffer_days = 0;
  },
);

// ===== 可行性 =====
const feasible = computed(
  () =>
    plan.value?.feasibility.status === "feasible" ||
    plan.value?.feasibility.status === "feasible_with_warnings",
);

const feasibleBar = computed(() => {
  const f = plan.value?.feasibility;
  if (!f) return null;
  if (f.status === "feasible")
    return {
      text: "可行",
      cls: "bg-green-50 dark:bg-green-500/15 text-green-700 dark:text-green-400",
    };
  if (f.status === "feasible_with_warnings")
    return {
      text: "可行（有警告）",
      cls: "bg-orange-50 dark:bg-orange-500/15 text-orange-600 dark:text-orange-400",
    };
  return {
    text: "不可行",
    cls: "bg-red-50 dark:bg-red-500/15 text-red-600 dark:text-red-400",
  };
});

const items = computed(() => plan.value?.items ?? []);

const columns: DataTableColumns<PlanItem> = [
  { title: "日期", key: "plan_date", width: 100 },
  { title: "名称", key: "name", minWidth: 150, ellipsis: { tooltip: true } },
  {
    title: "数量",
    key: "plan_qty",
    width: 90,
    render: (r) => `${r.plan_qty} ${r.unit}`,
  },
  { title: "类型", key: "task_type", width: 76 },
  {
    title: "依赖",
    key: "depends_on_seq",
    width: 70,
    render: (r) => (r.depends_on_seq.length ? r.depends_on_seq.join(",") : "—"),
  },
];

// ===== 构造请求 =====
function buildRequest(): SmartSplitRequest {
  const g = props.goal!;
  const req: SmartSplitRequest = {
    goal_id: g.id,
    api_version: 2,
    mode: form.mode,
    structure: { shape: form.shape },
  };
  if (form.dateRange) {
    req.intent = {
      horizon: {
        start_date: format(new Date(form.dateRange[0]), "yyyy-MM-dd"),
        end_date: format(new Date(form.dateRange[1]), "yyyy-MM-dd"),
      },
    };
  }
  if (form.total_qty && form.total_qty > 0) {
    req.intent = { ...(req.intent ?? {}), total_qty_override: form.total_qty };
  }
  if (form.buffer_days > 0) {
    req.intent = { ...(req.intent ?? {}), buffer_days: form.buffer_days };
  }
  if (form.max_daily_qty && form.max_daily_qty > 0) {
    req.constraints = {
      hard: { max_daily_qty: form.max_daily_qty },
    };
  }
  if (form.allowed_weekdays.length > 0) {
    req.constraints = {
      ...(req.constraints ?? {}),
      hard: { ...(req.constraints?.hard ?? {}), allowed_weekdays: form.allowed_weekdays },
    };
  }
  return req;
}

// ===== 生成预览 =====
async function handlePreview() {
  if (!props.goal) return;
  previewError.value = "";
  plan.value = null;
  previewing.value = true;
  try {
    const result = await smartSplitPreview(buildRequest());
    plan.value = result;
  } catch (e) {
    previewError.value = toErrorMessage(e);
  } finally {
    previewing.value = false;
  }
}

// ===== 应用补救：把 remedy.delta 合并回表单并重算 =====
function applyRemedy(delta: Record<string, unknown>) {
  for (const [key, val] of Object.entries(delta)) {
    if (key === "buffer_days" && typeof val === "number") {
      form.buffer_days = val;
    } else if (key === "total_qty_override" && typeof val === "number") {
      form.total_qty = val;
    } else if (key === "max_daily_qty" && typeof val === "number") {
      form.max_daily_qty = val;
    } else if (
      (key === "start_date" || key === "end_date") &&
      typeof val === "string" &&
      form.dateRange
    ) {
      const ts = new Date(val).getTime();
      const cur = [...form.dateRange] as [number, number];
      if (key === "start_date") cur[0] = ts;
      else cur[1] = ts;
      form.dateRange = cur;
    }
  }
  message.success("已应用补救参数，请重新生成预览");
}

// ===== 确认拆解（落库）=====
async function handleApply() {
  if (!props.goal || !plan.value) return;
  applying.value = true;
  try {
    await smartSplitApply(buildRequest());
    message.success(`已生成 ${plan.value.summary.task_count} 个任务`);
    emit("update:show", false);
    emit("success");
  } catch (e) {
    message.error(toErrorMessage(e));
  } finally {
    applying.value = false;
  }
}

function handleClose() {
  emit("update:show", false);
}
</script>

<template>
  <NModal
    :show="show"
    :mask-closable="true"
    :close-on-esc="true"
    style="width: 680px; max-width: 94vw"
    @update:show="emit('update:show', $event)"
  >
    <NCard
      :bordered="false"
      class="rounded-xl"
      style="max-height: 86vh"
      content-style="overflow-y:auto"
    >
      <template #header>
        <div class="flex items-center gap-2">
          <Icon icon="mdi:creation" class="text-brand-500" width="20" />
          <span class="text-base font-medium">智能拆解</span>
          <NTag size="tiny" type="info" :bordered="false">Beta</NTag>
        </div>
      </template>

      <div class="space-y-4">
        <!-- 目标信息（只读） -->
        <div
          v-if="goal"
          class="rounded px-3 py-2 bg-gray-50 dark:bg-surface-muted text-sm text-gray-600 dark:text-gray-300 space-y-0.5"
        >
          <div class="font-medium text-gray-800 dark:text-gray-100">
            {{ goal.name }}
          </div>
          <div class="text-xs text-gray-500 dark:text-gray-400">
            总量：{{ goal.total_qty }}{{ goal.unit || "" }}
            <template v-if="goal.deadline"> ｜截止：{{ goal.deadline }}</template>
          </div>
        </div>

        <!-- 配置表单 -->
        <div class="grid grid-cols-2 gap-x-4 gap-y-2">
          <NFormItem label="时间范围" :show-feedback="false" class="col-span-2">
            <NDatePicker
              v-model:value="form.dateRange"
              type="daterange"
              clearable
              style="width: 100%"
            />
          </NFormItem>
          <NFormItem label="总量覆盖" :show-feedback="false">
            <NInputNumber v-model:value="form.total_qty" :min="0" style="width: 100%" />
          </NFormItem>
          <NFormItem label="缓冲天数" :show-feedback="false">
            <NInputNumber v-model:value="form.buffer_days" :min="0" style="width: 100%" />
          </NFormItem>
          <NFormItem label="每日容量上限" :show-feedback="false">
            <NInputNumber
              v-model:value="form.max_daily_qty"
              :min="0"
              style="width: 100%"
            />
          </NFormItem>
          <NFormItem label="模式" :show-feedback="false">
            <NSelect v-model:value="form.mode" :options="modeOptions" style="width: 100%" />
          </NFormItem>
          <NFormItem label="拆解形状" :show-feedback="false">
            <NSelect v-model:value="form.shape" :options="shapeOptions" style="width: 100%" />
          </NFormItem>
          <NFormItem label="允许星期" :show-feedback="false" class="col-span-2">
            <NSelect
              v-model:value="form.allowed_weekdays"
              :options="weekdayOptions"
              multiple
              clearable
              placeholder="不选=不限制"
              style="width: 100%"
            />
          </NFormItem>
        </div>

        <!-- 操作 -->
        <NSpace justify="end">
          <NButton @click="handleClose">取消</NButton>
          <NButton
            type="primary"
            :loading="previewing"
            :disabled="!goal"
            @click="handlePreview"
          >
            <template #icon><Icon icon="mdi:creation" /></template>
            生成预览
          </NButton>
        </NSpace>

        <NDivider style="margin: 4px 0" />

        <!-- 预览错误 -->
        <NAlert v-if="previewError" type="error" :show-icon="true">
          {{ previewError }}
        </NAlert>

        <!-- 加载态 -->
        <div v-if="previewing" class="flex justify-center py-8">
          <NSpin size="large" />
        </div>

        <!-- 预览结果 -->
        <template v-if="!previewing && plan">
          <!-- 可行性条 -->
          <div
            v-if="feasibleBar"
            class="flex flex-wrap items-center gap-x-4 gap-y-1 px-3 py-2 rounded"
            :class="feasibleBar.cls"
          >
            <span class="flex items-center gap-1 font-medium">
              <Icon
                :icon="feasible ? 'mdi:check-decagram-outline' : 'mdi:alert-decagram-outline'"
                width="16"
              />
              {{ feasibleBar.text }}
            </span>
            <span class="text-xs">
              {{ plan.feasibility.effective_days }} 天 ｜ 峰值
              {{ plan.feasibility.peak_day_load }} ｜ 平均
              {{ plan.feasibility.avg_day_load.toFixed(1) }}
            </span>
            <span
              class="text-xs"
              :class="
                plan.feasibility.slack_ratio >= 0
                  ? 'text-green-600 dark:text-green-400'
                  : 'text-red-600 dark:text-red-400'
              "
            >
              松弛比
              {{ plan.feasibility.slack_ratio >= 0 ? "+" : "" }}{{ plan.feasibility.slack_ratio.toFixed(2) }}
            </span>
          </div>

          <!-- 补救建议（infeasible / partial） -->
          <div
            v-if="plan.feasibility.remedies.length"
            class="rounded px-3 py-2 bg-gray-50 dark:bg-surface-muted"
          >
            <div class="text-xs text-gray-500 dark:text-gray-400 mb-1.5 font-medium">
              补救建议（点击应用参数并重算）：
            </div>
            <div class="space-y-1">
              <NButton
                v-for="(r, i) in plan.feasibility.remedies"
                :key="i"
                size="small"
                quaternary
                type="info"
                @click="applyRemedy(r.delta)"
                class="!h-auto !justify-start whitespace-normal text-left"
              >
                <span class="text-left">
                  <strong>{{ r.label }}</strong>
                  <span class="opacity-70">（{{ r.cost }}）{{ r.tradeoff }}</span>
                </span>
              </NButton>
            </div>
          </div>

          <!-- 摘要 -->
          <div
            class="flex flex-wrap items-center gap-x-4 gap-y-1 px-1 text-sm text-gray-600 dark:text-gray-300"
          >
            <span>任务数 <strong>{{ plan.summary.task_count }}</strong></span>
            <span>总量 <strong>{{ plan.summary.total_allocated_qty }}</strong></span>
            <NTag
              :type="plan.summary.conservation_ok ? 'success' : 'warning'"
              size="small"
              :bordered="false"
            >
              {{ plan.summary.conservation_ok ? "总量守恒 ✓" : "总量守恒 ✗" }}
            </NTag>
          </div>

          <!-- 明细表 -->
          <NDataTable
            :columns="columns"
            :data="items"
            :max-height="320"
            :scroll-x="520"
            size="small"
            :bordered="false"
            :row-key="(row: PlanItem) => row.seq"
          />

          <!-- 确认拆解 -->
          <div v-if="feasible" class="flex justify-end">
            <NButton type="primary" :loading="applying" @click="handleApply">
              <template #icon><Icon icon="mdi:check" /></template>
              确认拆解（落库）
            </NButton>
          </div>
          <div
            v-else-if="plan.feasibility.status === 'infeasible'"
            class="text-right text-xs text-gray-400"
          >
            当前约束下不可行，请调整参数或应用补救后重算。
          </div>
        </template>
      </div>
    </NCard>
  </NModal>
</template>