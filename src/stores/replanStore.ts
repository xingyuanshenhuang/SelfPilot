/**
 * 重新规划（阶段3B）Pinia store
 *
 * 三态：preview（只读求解）→ what-if 仿真 → commit（apply）→ （可选）revert。
 * what-if 参数（deadline / daily_capacity / intensity / respect_locks / 排除项）
 * 变化时置 stale 标记，由消费方防抖触发 preview() 重算，保证 plan_hash 与提交一致。
 */
import { defineStore } from "pinia";
import { ref, reactive } from "vue";
import * as replanApi from "@/api/replan";
import type {
  ReplanApplyResult,
  ReplanEvent,
  ReplanEventDetail,
  ReplanOverrides,
  ReplanPlan,
  ReplanScope,
} from "@/types";

export const useReplanStore = defineStore("replan", () => {
  // ===== 当前求解状态 =====
  const goalId = ref<string>("");
  const scope = ref<ReplanScope>("single");
  const plan = ref<ReplanPlan | null>(null);
  const loading = ref(false);
  const applying = ref(false);
  /** what-if 参数或排除项自上次预览后是否变更（需防抖重算） */
  const stale = ref(false);
  /** 最近一次提交结果（供撤销预告 / UNDO） */
  const lastApply = ref<ReplanApplyResult | null>(null);

  // ===== what-if 覆盖参数（None=使用生效策略值）=====
  const overrides = reactive<ReplanOverrides>({
    deadline: null,
    daily_capacity: undefined,
    intensity: undefined,
    respect_locks: undefined,
  });
  /** 逐任务排除本次调整（用户取消勾选的项） */
  const excludedTaskIds = ref<string[]>([]);

  function markStale() {
    stale.value = true;
  }

  /** 打开某目标：重置并触发首次预览 */
  async function open(goalIdArg: string, scopeArg: ReplanScope = "single") {
    goalId.value = goalIdArg;
    scope.value = scopeArg;
    plan.value = null;
    lastApply.value = null;
    overrides.deadline = null;
    overrides.daily_capacity = undefined;
    overrides.intensity = undefined;
    overrides.respect_locks = undefined;
    excludedTaskIds.value = [];
    stale.value = false;
    await preview();
  }

  /** 生成当前 what-if 的求解覆盖参数（含排除项） */
  function buildOverrides(): ReplanOverrides {
    const o: ReplanOverrides = {
      deadline: overrides.deadline ?? null,
    };
    if (overrides.daily_capacity !== undefined) {
      o.daily_capacity = overrides.daily_capacity;
    }
    if (overrides.intensity !== undefined) {
      o.intensity = overrides.intensity;
    }
    if (overrides.respect_locks !== undefined) {
      o.respect_locks = overrides.respect_locks;
    }
    if (excludedTaskIds.value.length > 0) {
      o.excluded_task_ids = [...excludedTaskIds.value];
    }
    return o;
  }

  /** 预览（只读求解，永不落库） */
  async function preview() {
    if (!goalId.value) return null;
    loading.value = true;
    try {
      plan.value = await replanApi.replanSolve({
        goal_id: goalId.value,
        scope: scope.value,
        overrides: buildOverrides(),
      });
      stale.value = false;
      return plan.value;
    } finally {
      loading.value = false;
    }
  }

  /** 提交应用（plan_hash 与 overrides 必须与最近一次预览一致） */
  async function commit(): Promise<ReplanApplyResult | null> {
    if (!goalId.value || !plan.value) return null;
    applying.value = true;
    try {
      const o = buildOverrides();
      const result = await replanApi.replanApply({
        goal_id: goalId.value,
        plan_hash: plan.value.plan_hash,
        scope: scope.value,
        overrides: o,
        excluded_task_ids:
          excludedTaskIds.value.length > 0
            ? [...excludedTaskIds.value]
            : undefined,
      });
      lastApply.value = result;
      plan.value = null;
      stale.value = false;
      return result;
    } finally {
      applying.value = false;
    }
  }

  /** 撤销最近一次已应用的规划 */
  async function revert(eventId: string): Promise<ReplanApplyResult | null> {
    applying.value = true;
    try {
      const result = await replanApi.replanRevert(eventId);
      lastApply.value = null;
      return result;
    } finally {
      applying.value = false;
    }
  }

  /** 事件时间线 */
  async function fetchEvents(limit = 50, offset = 0): Promise<ReplanEvent[]> {
    if (!goalId.value) return [];
    return replanApi.replanListEvents(goalId.value, limit, offset);
  }

  /** 事件详情（含差异明细） */
  async function fetchEventDetail(
    eventId: string,
  ): Promise<ReplanEventDetail> {
    return replanApi.replanEventDetail(eventId);
  }

  return {
    goalId,
    scope,
    plan,
    loading,
    applying,
    stale,
    lastApply,
    overrides,
    excludedTaskIds,
    markStale,
    open,
    preview,
    commit,
    revert,
    fetchEvents,
    fetchEventDetail,
  };
});