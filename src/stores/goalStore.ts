import { defineStore } from "pinia";
import { ref } from "vue";
import * as goalApi from "@/api/goal";
import * as progressApi from "@/api/progress";
import type {
  Goal,
  CreateGoalInput,
  ProgressInfo,
  GoalTreeNode,
  SmartSplitInput,
  Task,
} from "@/types";

export const useGoalStore = defineStore("goal", () => {
  const goals = ref<Goal[]>([]);
  const goalTree = ref<GoalTreeNode[]>([]);
  const progresses = ref<ProgressInfo[]>([]);
  const loading = ref(false);

  async function fetchGoals() {
    loading.value = true;
    try {
      goals.value = await goalApi.listGoals();
    } finally {
      loading.value = false;
    }
  }

  async function fetchGoalTree() {
    loading.value = true;
    try {
      goalTree.value = await goalApi.listGoalTree();
    } finally {
      loading.value = false;
    }
  }

  async function fetchProgresses() {
    progresses.value = await progressApi.getAllGoalsProgress();
  }

  async function createGoal(input: CreateGoalInput) {
    const goal = await goalApi.createGoal(input);
    return goal;
  }

  async function deleteGoal(id: string) {
    await goalApi.deleteGoal(id);
  }

  async function autoSplit(goalId: string) {
    const tasks = await goalApi.autoSplit(goalId);
    await fetchProgresses();
    return tasks;
  }

  async function splitByCapacity(goalId: string) {
    const tasks = await goalApi.splitByCapacity(goalId);
    await fetchProgresses();
    return tasks;
  }

  async function smartSplit(input: SmartSplitInput): Promise<Task[]> {
    const tasks = await goalApi.smartSplit(input);
    await fetchProgresses();
    return tasks;
  }

  async function repeatSplit(
    goalId: string,
    name: string,
    startDate: string,
    endDate?: string | null,
  ) {
    const tasks = await goalApi.repeatSplit({
      goal_id: goalId,
      name,
      start_date: startDate,
      end_date: endDate ?? null,
    });
    await fetchProgresses();
    return tasks;
  }

  function getProgress(goalId: string): ProgressInfo | undefined {
    return progresses.value.find((p) => p.id === goalId);
  }

  // ===== P2-3：局部 mutation（替代全量重拉）=====
  //
  // 以下方法在 goalTree / progresses 上原地修改，不替换整个引用，
  // 避免 1000+ 任务的全量重拉和 reactive 代理重建。
  // 高频写操作（完成/跳过/删除/编辑）改用这些方法，配合 refreshProgressForGoalChain
  // 仅重算受影响的祖先链进度。

  function findGoalNode(
    goalId: string,
    nodes: GoalTreeNode[] = goalTree.value,
  ): GoalTreeNode | null {
    for (const node of nodes) {
      if (node.goal.id === goalId) return node;
      const found = findGoalNode(goalId, node.sub_goals);
      if (found) return found;
    }
    return null;
  }

  function updateTaskLocally(task: Task): boolean {
    function search(nodes: GoalTreeNode[]): boolean {
      for (const node of nodes) {
        const idx = node.tasks.findIndex((t) => t.id === task.id);
        if (idx >= 0) {
          node.tasks[idx] = task;
          return true;
        }
        if (search(node.sub_goals)) return true;
      }
      return false;
    }
    return search(goalTree.value);
  }

  function removeTaskLocally(taskId: string): string | null {
    function search(nodes: GoalTreeNode[]): string | null {
      for (const node of nodes) {
        const idx = node.tasks.findIndex((t) => t.id === taskId);
        if (idx >= 0) {
          const goalId = node.tasks[idx].goal_id;
          node.tasks.splice(idx, 1);
          return goalId;
        }
        const found = search(node.sub_goals);
        if (found !== null) return found;
      }
      return null;
    }
    return search(goalTree.value);
  }

  function updateGoalLocally(goal: Goal): boolean {
    const node = findGoalNode(goal.id);
    if (!node) return false;
    node.goal = goal;
    return true;
  }

  function patchProgresses(newProgresses: ProgressInfo[]): void {
    for (const np of newProgresses) {
      const idx = progresses.value.findIndex((p) => p.id === np.id);
      if (idx >= 0) {
        progresses.value[idx] = np;
      } else {
        progresses.value.push(np);
      }
      const node = findGoalNode(np.id);
      if (node) {
        node.progress = np.percentage;
        node.is_completed = np.is_completed;
      }
    }
  }

  async function refreshProgressForGoalChain(goalId: string): Promise<void> {
    try {
      const ancestors = await progressApi.getGoalAncestorsProgress(goalId);
      patchProgresses(ancestors);
    } catch (e) {
      console.warn(
        "[goalStore] refreshProgressForGoalChain failed, will be corrected on next full fetch:",
        e,
      );
    }
  }

  return {
    goals,
    goalTree,
    progresses,
    loading,
    fetchGoals,
    fetchGoalTree,
    fetchProgresses,
    createGoal,
    deleteGoal,
    autoSplit,
    splitByCapacity,
    smartSplit,
    repeatSplit,
    getProgress,
    updateTaskLocally,
    removeTaskLocally,
    updateGoalLocally,
    patchProgresses,
    refreshProgressForGoalChain,
  };
});
