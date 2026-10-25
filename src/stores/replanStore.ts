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
  TriggerReport,
} from "@/types";

export const useReplanStore = defineStore("replan", () => {
  // ===== 当前求解状态 =====
  const goalId = ref<string>("");
  const scope = ref<ReplanScope>("single");
  /** 协同目标（scope=coordinate 时透传，须与预览一致保证 apply hash 相同） */
  const coordinateGoalIds = ref<string[]>([]);
  const plan = ref<ReplanPlan | null>(null);
  const loading = ref(false);
  const applying = ref(false);
  /** what-if 参数或排除项自上次预览后是否变更（需防抖重算） */
  const stale = ref(false);
  /** 最近一次提交结果（供撤销预告 / UNDO） */
  const lastApply = ref<ReplanApplyResult | null>(null);

  // ===== 触发网关（阶段 4）=====
  /** 最近一次批量触发检查结果（建议条数据源） */
  const pendingTriggers = ref<TriggerReport | null>(null);
  const checkingTriggers = ref(false);

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
  async function open(
    goalIdArg: string,
    scopeArg: ReplanScope = "single",
    goalIdsArg: string[] = [],
  ) {
    goalId.value = goalIdArg;
    scope.value = scopeArg;
    coordinateGoalIds.value = [...goalIdsArg];
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

  /**
   * 设置协同目标（抽屉内多选）：
   * 有选择 → scope=coordinate，goal_ids = [当前目标, ...选中目标]（去重）；
   * 清空 → 回到 subtree（含子目标）。
   */
  function setCoordinate(ids: string[]) {
    const uniq = [...new Set(ids.filter((id) => id !== goalId.value))];
    coordinateGoalIds.value = uniq;
    scope.value = uniq.length > 0 ? "coordinate" : "subtree";
    stale.value = true;
  }

  /** 触发网关批量检查（建议条数据源） */
  async function checkTriggers(): Promise<TriggerReport> {
    checkingTriggers.value = true;
    try {
      pendingTriggers.value = await replanApi.replanCheckTriggers();
      return pendingTriggers.value;
    } finally {
      checkingTriggers.value = false;
    }
  }

  /** 忽略某条建议（仅本地移除，不抑制后端状态） */
  function dismissTrigger(goalIdArg: string) {
    if (!pendingTriggers.value) return;
    const goals = pendingTriggers.value.goals.filter(
      (g) => g.goal_id !== goalIdArg,
    );
    pendingTriggers.value = {
      goals,
      pending_count: goals.filter((g) => g.decision === "proceed").length,
    };
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
        goal_ids:
          scope.value === "coordinate" && coordinateGoalIds.value.length > 0
            ? [...coordinateGoalIds.value]
            : undefined,
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
        goal_ids:
          scope.value === "coordinate" && coordinateGoalIds.value.length > 0
            ? [...coordinateGoalIds.value]
            : undefined,
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
    coordinateGoalIds,
    plan,
    loading,
    applying,
    stale,
    lastApply,
    pendingTriggers,
    checkingTriggers,
    overrides,
    excludedTaskIds,
    markStale,
    open,
    setCoordinate,
    checkTriggers,
    dismissTrigger,
    preview,
    commit,
    revert,
    fetchEvents,
    fetchEventDetail,
  };
});