<script setup lang="ts">
/**
 * 触发设置（阶段 4）
 *
 * 全局触发网关参数（settings 键）：
 * - planning_cooldown_hours   冷却小时数（ManualRequest 豁免）
 * - planning_min_impact_ratio 最小影响面（低于则抑制）
 * - planning_ewma_halflife    画像 EWMA 半衰期（反馈条数）
 * - planning_auto_apply_events 允许自动应用的事件类型（JSON 数组）
 */
import { reactive, ref, watch } from "vue";
import {
  NModal,
  NForm,
  NFormItem,
  NInputNumber,
  NSelect,
  NButton,
  NSpace,
  NAlert,
  useMessage,
} from "naive-ui";
import { Icon } from "@iconify/vue";
import * as settingsApi from "@/api/settings";
import { toErrorMessage } from "@/api/client";

const props = defineProps<{ show: boolean }>();
const emit = defineEmits<{
  (e: "update:show", value: boolean): void;
}>();

const message = useMessage();
const saving = ref(false);
const loading = ref(false);

const form = reactive({
  cooldown_hours: 6,
  min_impact_ratio: 10,
  ewma_halflife: 20,
  auto_apply_events: ["task_completed"] as string[],
});

/** 事件类型选项（与后端 9 类常量一致） */
const eventOptions = [
  { label: "任务完成", value: "task_completed" },
  { label: "任务跳过", value: "task_skipped" },
  { label: "逾期累积", value: "overdue_accumulated" },
  { label: "目标截止变动", value: "goal_deadline_changed" },
  { label: "目标总量变动", value: "goal_total_changed" },
  { label: "进度滞后", value: "progress_lag" },
  { label: "单日过载", value: "load_overload" },
  { label: "周期复评", value: "periodic_review" },
];

const DEFAULTS = {
  cooldown: "6",
  minImpact: "0.10",
  halflife: "20",
  events: '["task_completed"]',
};

function parseNumber(raw: string | null, fallback: number): number {
  if (raw === null || raw.trim() === "") return fallback;
  const n = Number(raw);
  return Number.isFinite(n) ? n : fallback;
}

async function load() {
  loading.value = true;
  try {
    const [cooldown, minImpact, halflife, events] = await Promise.all([
      settingsApi.getSetting("planning_cooldown_hours"),
      settingsApi.getSetting("planning_min_impact_ratio"),
      settingsApi.getSetting("planning_ewma_halflife"),
      settingsApi.getSetting("planning_auto_apply_events"),
    ]);
    form.cooldown_hours = parseNumber(cooldown, Number(DEFAULTS.cooldown));
    form.min_impact_ratio =
      Math.round(
        parseNumber(minImpact, Number(DEFAULTS.minImpact)) * 1000,
      ) / 10;
    form.ewma_halflife = Math.round(
      parseNumber(halflife, Number(DEFAULTS.halflife)),
    );
    try {
      const list = JSON.parse(events ?? DEFAULTS.events);
      form.auto_apply_events = Array.isArray(list)
        ? list.filter((v): v is string => typeof v === "string")
        : [];
    } catch {
      form.auto_apply_events = ["task_completed"];
    }
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
  if (!Number.isFinite(form.cooldown_hours) || form.cooldown_hours <= 0) {
    message.warning("冷却时长必须大于 0");
    return;
  }
  if (
    !Number.isFinite(form.min_impact_ratio) ||
    form.min_impact_ratio < 0 ||
    form.min_impact_ratio > 100
  ) {
    message.warning("最小影响面需在 0~100% 之间");
    return;
  }
  if (!Number.isFinite(form.ewma_halflife) || form.ewma_halflife < 1) {
    message.warning("EWMA 半衰期至少为 1");
    return;
  }
  saving.value = true;
  try {
    await Promise.all([
      settingsApi.setSetting({
        key: "planning_cooldown_hours",
        value: String(form.cooldown_hours),
      }),
      settingsApi.setSetting({
        key: "planning_min_impact_ratio",
        value: String(form.min_impact_ratio / 100),
      }),
      settingsApi.setSetting({
        key: "planning_ewma_halflife",
        value: String(Math.round(form.ewma_halflife)),
      }),
      settingsApi.setSetting({
        key: "planning_auto_apply_events",
        value: JSON.stringify(form.auto_apply_events),
      }),
    ]);
    message.success("触发设置已保存");
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
    title="自动规划触发设置"
    style="width: 560px"
    @update:show="emit('update:show', $event)"
  >
    <NSpace vertical :size="16">
      <NAlert type="info" :bordered="false" :show-icon="true">
        任务完成/跳过、目标改期与过期累积等事件会先经过触发网关评估；
        被抑制或已自动应用的事件不会打扰你。
      </NAlert>

      <NForm label-placement="top" :disabled="loading">
        <NFormItem label="冷却时长（小时）">
          <NInputNumber
            v-model:value="form.cooldown_hours"
            :min="0.5"
            :step="0.5"
            style="width: 100%"
          />
          <template #feedback>
            <span class="text-xs text-gray-400">
              同一目标两次自动规划的最短间隔；手动请求不受此限制
            </span>
          </template>
        </NFormItem>

        <NFormItem label="最小影响面（%）">
          <NInputNumber
            v-model:value="form.min_impact_ratio"
            :min="0"
            :max="100"
            :step="1"
            style="width: 100%"
          />
          <template #feedback>
            <span class="text-xs text-gray-400">
              受影响任务占比低于该值时直接抑制，避免频繁打扰
            </span>
          </template>
        </NFormItem>

        <NFormItem label="习惯画像半衰期（反馈条数）">
          <NInputNumber
            v-model:value="form.ewma_halflife"
            :min="1"
            :step="5"
            style="width: 100%"
          />
          <template #feedback>
            <span class="text-xs text-gray-400">
              值越小越快适应近期节奏变化，越大越平滑（默认 20）
            </span>
          </template>
        </NFormItem>

        <NFormItem label="允许自动应用的事件">
          <NSelect
            v-model:value="form.auto_apply_events"
            :options="eventOptions"
            multiple
            placeholder="不选择则所有事件仅生成建议"
            style="width: 100%"
          />
          <template #feedback>
            <span class="text-xs text-gray-400">
              仅对开启了「自动应用」的目标生效（可在目标行点击机器人图标设置）
            </span>
          </template>
        </NFormItem>
      </NForm>
    </NSpace>

    <template #footer>
      <NSpace justify="end">
        <NButton @click="emit('update:show', false)">取消</NButton>
        <NButton type="primary" :loading="saving" :disabled="loading" @click="handleSave">
          <template #icon><Icon icon="mdi:content-save-outline" /></template>
          保存
        </NButton>
      </NSpace>
    </template>
  </NModal>
</template>
