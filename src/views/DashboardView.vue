<script setup lang="ts">
import {
  onMounted,
  onBeforeUnmount,
  ref,
  computed,
  nextTick,
  watch,
} from "vue";
import {
  NCard,
  NEmpty,
  NButton,
  NProgress,
  NTag,
  NSpace,
  useMessage,
} from "naive-ui";
import { Icon } from "@iconify/vue";
import { useTaskStore } from "@/stores/taskStore";
import { useGoalStore } from "@/stores/goalStore";
import { useEncouragementStore } from "@/stores/encouragementStore";
import TaskItem from "@/components/TaskItem.vue";
import SetbackModal from "@/components/SetbackModal.vue";
import type { TodayTask, SetbackSituation } from "@/types";
import ProgressRing from "@/components/ProgressRing.vue";
import { format } from "date-fns";
import { getSetbackSituation } from "@/api/encouragement";

const taskStore = useTaskStore();
const goalStore = useGoalStore();
const encStore = useEncouragementStore();
const message = useMessage();

const today = computed(() => format(new Date(), "yyyy-MM-dd"));
const encouragement = ref("");
const selectedOverdueDate = ref<string | null>(null);

// P1-2：挫折场景检测
const setbackSituation = ref<SetbackSituation | null>(null);
const showSetbackModal = ref(false);

/** 只展示根目标（总目标），子目标的进度已汇总到父目标 */
const rootGoals = computed(() =>
  goalStore.goals.filter(
    (g) => g.parent_id === null || g.parent_id === undefined,
  ),
);

/** 按逾期日期聚合逾期任务 */
const overdueGroups = computed(() => {
  const map: Record<string, TodayTask[]> = {};
  for (const task of taskStore.overdueTasks) {
    const date = task.overdue_date || task.plan_date;
    if (!date) continue;
    if (!map[date]) map[date] = [];
    map[date].push(task);
  }
  return Object.entries(map).sort(([a], [b]) => a.localeCompare(b));
});

const overdueDates = computed(() => overdueGroups.value.map(([date]) => date));

const activeOverdueDate = computed(() => {
  if (!selectedOverdueDate.value) return null;
  if (overdueDates.value.includes(selectedOverdueDate.value)) {
    return selectedOverdueDate.value;
  }
  return null;
});

const filteredOverdueGroups = computed(() => {
  if (!activeOverdueDate.value) return overdueGroups.value;
  return overdueGroups.value.filter(
    ([date]) => date === activeOverdueDate.value,
  );
});

const selectedOverdueIndex = computed(() => {
  if (!activeOverdueDate.value) return -1;
  return overdueDates.value.indexOf(activeOverdueDate.value);
});

const canPrevOverdueDate = computed(() => selectedOverdueIndex.value > 0);
const canNextOverdueDate = computed(
  () =>
    selectedOverdueIndex.value >= 0 &&
    selectedOverdueIndex.value < overdueDates.value.length - 1,
);

function selectOverdueDate(date: string | null) {
  selectedOverdueDate.value = date;
}

function prevOverdueDate() {
  const idx = selectedOverdueIndex.value;
  if (idx > 0) {
    selectedOverdueDate.value = overdueDates.value[idx - 1];
  }
}

function nextOverdueDate() {
  const idx = selectedOverdueIndex.value;
  if (idx >= 0 && idx < overdueDates.value.length - 1) {
    selectedOverdueDate.value = overdueDates.value[idx + 1];
  }
}

// ===== 逾期日期筛选栏折叠 =====

const filterElRef = ref<HTMLElement | null>(null);
const filterExpanded = ref(false);
const filterOverflow = ref(false);
const firstRowHeight = ref(0);

let filterObserver: ResizeObserver | null = null;

/** 折叠时仍将已选日期置顶，保证已选筛选条件清晰可见 */
const displayDates = computed(() => {
  if (!activeOverdueDate.value) return overdueDates.value;
  return [
    activeOverdueDate.value,
    ...overdueDates.value.filter((d) => d !== activeOverdueDate.value),
  ];
});

function detectFilterOverflow() {
  const el = filterElRef.value;
  if (!el) return;
  const items = Array.from(el.children) as HTMLElement[];
  if (items.length === 0) {
    filterOverflow.value = false;
    return;
  }
  // flex 换行后，末项 offsetTop 必然大于首项（第二行及以后则判定溢出）
  const firstTop = items[0].offsetTop;
  const lastTop = items[items.length - 1].offsetTop;
  filterOverflow.value = lastTop > firstTop;
  if (!filterOverflow.value) {
    filterExpanded.value = false;
  }
  firstRowHeight.value = items[0].offsetHeight;
}

function setupFilterObserver() {
  const el = filterElRef.value;
  if (el) {
    if (!filterObserver) {
      filterObserver = new ResizeObserver(() => detectFilterOverflow());
      filterObserver.observe(el);
    }
  } else if (filterObserver) {
    filterObserver.disconnect();
    filterObserver = null;
  }
}

function toggleFilterExpanded() {
  filterExpanded.value = !filterExpanded.value;
}

watch(
  overdueDates,
  async () => {
    await nextTick();
    setupFilterObserver();
    detectFilterOverflow();
  },
  { immediate: true },
);

onBeforeUnmount(() => {
  if (filterObserver) {
    filterObserver.disconnect();
    filterObserver = null;
  }
});

onMounted(async () => {
  await Promise.all([
    taskStore.fetchAll(),
    goalStore.fetchGoals(),
    goalStore.fetchProgresses(),
    encStore.fetchSettings(), // P1-4：确保 settings 已加载
  ]);

  // P2-2：判断是否当日首次打开应用
  const today = new Date().toISOString().split("T")[0];
  const lastOpenDate = localStorage.getItem("selfpilot_last_open_date");
  const isFirstOpen = lastOpenDate !== today;

  // P0-1 + P2-2：banner 文案，首次打开用 app_first_open，否则用 dashboard_banner
  if (isFirstOpen) {
    const enc = await encStore.random("app_first_open");
    encouragement.value = enc?.text ?? "";
    localStorage.setItem("selfpilot_last_open_date", today);
  } else {
    const enc = await encStore.random("dashboard_banner");
    encouragement.value = enc?.text ?? "";
  }

  // P1-2：挫折场景检测（仅在鼓励语开关开启时）
  // 每日仅提示一次：当天已显示过逾期/挫折提示，则切回本视图时不再重复弹出
  if (encStore.settings.enabled) {
    try {
      const setbackToday = format(new Date(), "yyyy-MM-dd");
      const setbackShownDate = localStorage.getItem(
        "selfpilot_setback_shown_date",
      );
      if (setbackShownDate !== setbackToday) {
        const setback = await getSetbackSituation();
        if (setback.has_streak_break || setback.has_progress_lag) {
          setbackSituation.value = setback;
          showSetbackModal.value = true;
          localStorage.setItem("selfpilot_setback_shown_date", setbackToday);
        }
      }
    } catch (e) {
      console.warn("挫折场景检测失败:", e);
    }
  }
});

function handleSetbackClose() {
  showSetbackModal.value = false;
}

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
      class="!bg-gradient-to-r from-brand-50 to-blue-50 dark:from-brand-500/20 dark:to-blue-500/20"
    >
      <div class="flex items-center gap-3">
        <Icon icon="mdi:star-four-points" width="24" class="text-brand-500" />
        <span class="text-base font-medium">{{ encouragement }}</span>
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

    <!-- 逾期任务 -->
    <NCard v-if="taskStore.overdueTasks.length > 0" :bordered="false">
      <template #header>
        <div class="flex items-center gap-2 text-red-500">
          <Icon icon="mdi:alert-circle" width="20" />
          <span>逾期任务</span>
        </div>
      </template>
      <template #header-extra>
        <NSpace align="center">
          <NButton
            quaternary
            circle
            size="small"
            :disabled="!canPrevOverdueDate"
            @click="prevOverdueDate"
          >
            <template #icon><Icon icon="mdi:chevron-left" /></template>
          </NButton>
          <NTag type="error" size="small" round>
            {{ activeOverdueDate ?? "全部" }}
          </NTag>
          <NButton
            quaternary
            circle
            size="small"
            :disabled="!canNextOverdueDate"
            @click="nextOverdueDate"
          >
            <template #icon><Icon icon="mdi:chevron-right" /></template>
          </NButton>
          <NTag type="error" size="small" round>{{
            taskStore.overdueTasks.length
          }}</NTag>
        </NSpace>
      </template>

      <!-- 日期筛选 -->
      <div class="mb-3">
        <div
          class="overdue-dates"
          :class="{ 'overdue-dates--collapsed': filterOverflow && !filterExpanded }"
          :style="
            filterOverflow && !filterExpanded
              ? { maxHeight: firstRowHeight + 'px' }
              : undefined
          "
          role="group"
          aria-label="逾期日期筛选"
        >
          <div ref="filterElRef" class="overdue-dates-tags">
            <NTag
              :type="activeOverdueDate === null ? 'error' : 'default'"
              size="small"
              round
              class="cursor-pointer"
              @click="selectOverdueDate(null)"
            >
              全部
            </NTag>
            <NTag
              v-for="date in displayDates"
              :key="date"
              :type="activeOverdueDate === date ? 'error' : 'default'"
              size="small"
              round
              class="cursor-pointer"
              @click="selectOverdueDate(date)"
            >
              {{ date }}
            </NTag>
          </div>
          <button
            v-if="filterOverflow"
            type="button"
            class="overdue-filter-toggle"
            :aria-expanded="filterExpanded"
            :aria-label="filterExpanded ? '收起日期筛选' : '展开全部逾期日期'"
            @click="toggleFilterExpanded"
          >
            <Icon
              :icon="filterExpanded ? 'mdi:chevron-up' : 'mdi:chevron-down'"
              width="16"
            />
            {{ filterExpanded ? "收起" : "展开全部" }}
          </button>
        </div>
      </div>

      <!-- 按日期分组展示 -->
      <div
        v-for="[date, tasks] in filteredOverdueGroups"
        :key="date"
        class="space-y-1 mb-4 last:mb-0"
      >
        <div class="flex items-center justify-between py-1">
          <div class="flex items-center gap-2">
            <Icon icon="mdi:calendar-alert" width="18" class="text-red-500" />
            <span class="text-base font-medium">{{ date }} 逾期的任务</span>
          </div>
          <NTag type="error" size="small" round>{{ tasks.length }}</NTag>
        </div>
        <TaskItem
          v-for="task in tasks"
          :key="task.id"
          :task="task"
          overdue
          @completed="refresh"
        />
      </div>
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
          class="flex items-center gap-3 p-3 rounded-lg border border-gray-100 dark:border-surface-borderMuted hover:shadow-sm transition"
        >
          <ProgressRing
            :percentage="goalStore.getProgress(goal.id)?.percentage ?? 0"
          />
          <div class="flex-1 min-w-0">
            <div class="font-medium text-sm truncate">{{ goal.name }}</div>
            <div class="text-xs text-gray-500 dark:text-gray-400 mt-1">
              {{ goal.deadline ? `截止：${goal.deadline}` : "无限期" }}
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

    <!-- P1-2：挫折安抚弹窗 -->
    <SetbackModal
      v-model:show="showSetbackModal"
      :setback="setbackSituation"
      @close="handleSetbackClose"
    />
  </div>
</template>

<style scoped>
/* 逾期日期筛选栏容器：裁剪超出部分，右侧为折叠按钮预留空间 */
.overdue-dates {
  position: relative;
  overflow: hidden;
  transition: max-height 0.25s ease;
}

/* 日期标签行：flex 换行，右侧预留折叠按钮位避免遮挡 */
.overdue-dates-tags {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
  padding-right: 88px;
}

/* 展开/收起按钮：嵌入内容区，折叠时固定在第一行末尾 */
.overdue-filter-toggle {
  position: absolute;
  display: inline-flex;
  align-items: center;
  gap: 2px;
  height: 20px;
  padding: 0 4px;
  border: 0;
  border-radius: 4px;
  background: transparent;
  cursor: pointer;
  color: #3478f6;
  font-size: 12px;
  line-height: 1;
  transition: background-color 0.2s ease, color 0.2s ease;
}

.overdue-filter-toggle:hover {
  background-color: rgba(52, 120, 246, 0.08);
  color: #1f5fd8;
}

.overdue-filter-toggle:focus-visible {
  outline: 2px solid #3478f6;
  outline-offset: 2px;
}

/* 折叠态：按钮对齐第一行末尾 */
.overdue-dates--collapsed .overdue-filter-toggle {
  top: 2px;
  right: 2px;
}

/* 展开态：按钮移至内容区右下角，作为折叠入口 */
.overdue-dates:not(.overdue-dates--collapsed) .overdue-filter-toggle {
  bottom: 2px;
  right: 2px;
}
</style>
