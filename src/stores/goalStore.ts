import { defineStore } from "pinia";
import { ref } from "vue";
import * as goalApi from "@/api/goal";
import * as progressApi from "@/api/progress";
import type {
  Goal,
  CreateGoalInput,
  ProgressInfo,
  GoalTreeNode,
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

  async function repeatSplit(goalId: string, name: string, startDate: string, endDate?: string | null) {
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
    repeatSplit,
    getProgress,
  };
});
