<script setup lang="ts">
import { onMounted, ref, computed } from "vue";
import { NCard, NEmpty, NButton, NProgress, NTag, useMessage } from "naive-ui";
import { Icon } from "@iconify/vue";
import { useTaskStore } from "@/stores/taskStore";
import { useGoalStore } from "@/stores/goalStore";
import TaskItem from "@/components/TaskItem.vue";
import ProgressRing from "@/components/ProgressRing.vue";
import { randomEncouragement } from "@/constants/encouragements";
import { format } from "date-fns";

const taskStore = useTaskStore();
const goalStore = useGoalStore();
const message = useMessage();

const today = computed(() => format(new Date(), "yyyy-MM-dd"));
const encouragement = ref("");

/** 只展示根目标（总目标），子目标的进度已汇总到父目标 */
const rootGoals = computed(() =>
  goalStore.goals.filter(
    (g) => g.parent_id === null || g.parent_id === undefined,
  ),
);

onMounted(async () => {
  await Promise.all([
    taskStore.fetchAll(),
    goalStore.fetchGoals(),
    goalStore.fetchProgresses(),
  ]);
  encouragement.value = randomEncouragement();
});

async function refresh() {
  await Promise.all([taskStore.fetchAll(), goalStore.fetchProgresses()]);
}
</script>

<template>
  <div class="space-y-4">
    <!-- 鼓励语 -->
    <NCard
      v-if="encouragement"
      :bordered="false"
      class="!bg-gradient-to-r from-brand-50 to-blue-50"
    >
      <div class="flex items-center gap-3">
        <Icon icon="mdi:star-four-points" width="24" class="text-brand-500" />
        <span class="text-base font-medium">{{ encouragement }}</span>
      </div>
    </NCard>

    <!-- 逾期任务 -->
    <NCard v-if="taskStore.overdueTasks.length > 0" :bordered="false">
      <template #header>
        <div class="flex items-center gap-2 text-red-500">
          <Icon icon="mdi:alert-circle" width="20" />
          <span>逾期任务</span>
        </div>
      </template>
      <template #header-extra>
        <NTag type="error" size="small" round>{{
          taskStore.overdueTasks.length
        }}</NTag>
      </template>
      <div class="space-y-1">
        <TaskItem
          v-for="task in taskStore.overdueTasks"
          :key="task.id"
          :task="task"
          overdue
          @completed="refresh"
        />
      </div>
    </NCard>

    <!-- 今日待办 -->
    <NCard :bordered="false">
      <template #header>
        <div class="flex items-center gap-2">
          <Icon icon="mdi:calendar-today" width="20" class="text-brand-500" />
          <span>今日待办 ({{ today }})</span>
        </div>
      </template>
      <template #header-extra>
        <NButton size="small" quaternary @click="refresh">
          <template #icon><Icon icon="mdi:refresh" /></template>
          刷新
        </NButton>
      </template>
      <div v-if="taskStore.todayTasks.length > 0" class="space-y-1">
        <TaskItem
          v-for="task in taskStore.todayTasks"
          :key="task.id"
          :task="task"
          @completed="refresh"
        />
      </div>
      <NEmpty v-else description="今日暂无待办任务" />
    </NCard>

    <!-- 目标进度总览 -->
    <NCard :bordered="false">
      <template #header>
        <div class="flex items-center gap-2">
          <Icon icon="mdi:chart-donut" width="20" class="text-brand-500" />
          <span>目标进度总览</span>
        </div>
      </template>
      <div
        v-if="rootGoals.length > 0"
        class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3"
      >
        <div
          v-for="goal in rootGoals"
          :key="goal.id"
          class="flex items-center gap-3 p-3 rounded-lg border border-gray-100 hover:shadow-sm transition"
        >
          <ProgressRing
            :percentage="goalStore.getProgress(goal.id)?.percentage ?? 0"
          />
          <div class="flex-1 min-w-0">
            <div class="font-medium text-sm truncate">{{ goal.name }}</div>
            <div class="text-xs text-gray-500 mt-1">
              截止：{{ goal.deadline || "未设置" }}
            </div>
            <NProgress
              type="line"
              :percentage="
                Math.round(
                  (goalStore.getProgress(goal.id)?.percentage ?? 0) * 100,
                )
              "
              :show-indicator="false"
              :height="4"
              class="mt-1"
            />
          </div>
        </div>
      </div>
      <NEmpty v-else description="还没有目标，请到左侧「目标树」创建" />
    </NCard>
  </div>
</template>
