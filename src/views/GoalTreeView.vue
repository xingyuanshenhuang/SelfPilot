<script setup lang="ts">
import { onMounted, ref, reactive, h } from "vue";
import {
  NCard,
  NButton,
  NSpace,
  NInput,
  NDatePicker,
  NSelect,
  NInputNumber,
  NForm,
  NFormItem,
  NModal,
  NEmpty,
  NTag,
  NPopconfirm,
  NDataTable,
  NProgress,
  NDropdown,
  NCheckbox,
  useMessage,
  useDialog,
} from "naive-ui";
import type { DataTableColumns, DropdownOption } from "naive-ui";
import { Icon } from "@iconify/vue";
import { useGoalStore } from "@/stores/goalStore";
import * as taskApi from "@/api/task";
import * as goalApi from "@/api/goal";
import type {
  Goal,
  Task,
  ReplanPreview,
  CreateTaskInput,
  UpdateTaskInput,
  CreateGoalInput,
  RepeatSplitInput,
} from "@/types";
import { STATUS_META } from "@/types";
import { format, parseISO, differenceInCalendarDays } from "date-fns";

const goalStore = useGoalStore();
const message = useMessage();
const dialog = useDialog();

// ===== 创建目标（总目标 or 子目标）=====
const showCreateGoal = ref(false);
const createGoalParentId = ref<string | null>(null); // null=总目标, string=子目标
const goalForm = reactive({
  name: "",
  deadline: null as number | null,
  total_qty: 0,
  unit: "个",
});

const unitOptions = [
  { label: "个", value: "个" },
  { label: "页", value: "页" },
  { label: "小时", value: "小时" },
  { label: "道", value: "道" },
  { label: "无", value: "" },
];

// ===== 展开状态 =====
const expandedNodes = ref<Set<string>>(new Set());

// ===== 重复拆解弹窗 =====
const showRepeatSplit = ref(false);
const repeatForm = reactive({
  goal_id: "",
  goal_name: "",
  name: "",
  start_date: Date.now() as number | null,
  end_date: null as number | null,
  is_repeat: false,
  plan_qty: 1,
  unit: "",
});

// ===== 任务创建/编辑弹窗 =====
const showTaskModal = ref(false);
const taskModalMode = ref<"create" | "edit">("create");
const taskForm = reactive({
  task_id: "",
  goal_id: "",
  name: "",
  plan_date: null as number | null,
  plan_qty: 1,
  unit: "个",
});

// ===== 补完成弹窗 =====
const showBackfillModal = ref(false);
const backfillForm = reactive({
  task_id: "",
  task_name: "",
  goal_id: "",
  plan_qty: 1,
  actual_qty: 0,
  unit: "个",
});

// ===== 重新规划弹窗 =====
const showReplanModal = ref(false);
const replanPreview = ref<ReplanPreview | null>(null);
const replanGoalId = ref("");

onMounted(async () => {
  await goalStore.fetchGoalTree();
  await goalStore.fetchProgresses();
});

// ===== 目标创建 =====
function openCreateGoalModal(parentId: string | null) {
  createGoalParentId.value = parentId;
  goalForm.name = "";
  goalForm.deadline = null;
  goalForm.total_qty = 0;
  goalForm.unit = "个";
  showCreateGoal.value = true;
}

async function handleCreateGoal() {
  if (!goalForm.name.trim()) {
    message.warning("请输入目标名称");
    return;
  }
  const deadline = goalForm.deadline
    ? format(new Date(goalForm.deadline), "yyyy-MM-dd")
    : null;
  try {
    const input: CreateGoalInput = {
      name: goalForm.name,
      parent_id: createGoalParentId.value,
      deadline,
      total_qty: goalForm.total_qty,
      unit: goalForm.unit,
    };
    await goalStore.createGoal(input);
    message.success(
      createGoalParentId.value ? "子目标创建成功" : "总目标创建成功",
    );
    showCreateGoal.value = false;
    await goalStore.fetchGoalTree();
    await goalStore.fetchProgresses();
    if (createGoalParentId.value) {
      expandedNodes.value.add(createGoalParentId.value);
    }
  } catch (e) {
    message.error(String(e));
  }
}

// ===== 展开/折叠 =====
function toggleNode(nodeId: string) {
  if (expandedNodes.value.has(nodeId)) {
    expandedNodes.value.delete(nodeId);
  } else {
    expandedNodes.value.add(nodeId);
  }
}

// ===== 删除目标 =====
async function handleDeleteGoal(goal: Goal) {
  try {
    await goalStore.deleteGoal(goal.id);
    message.success("已删除目标");
    await goalStore.fetchGoalTree();
    await goalStore.fetchProgresses();
  } catch (e) {
    message.error(String(e));
  }
}

// ===== 自动拆解（视频/数量类）=====
async function handleAutoSplit(goal: Goal) {
  try {
    const tasks = await goalStore.autoSplit(goal.id);
    message.success(`已拆解为 ${tasks.length} 个每日任务`);
    await goalStore.fetchGoalTree();
    expandedNodes.value.add(goal.id);
  } catch (e) {
    message.error(String(e));
  }
}

// ===== 重复拆解（纯文字类）=====
function openRepeatSplitModal(goal: Goal) {
  repeatForm.goal_id = goal.id;
  repeatForm.goal_name = goal.name;
  repeatForm.name = "";
  repeatForm.start_date = Date.now();
  repeatForm.end_date = null;
  repeatForm.is_repeat = false;
  repeatForm.plan_qty = 1;
  repeatForm.unit = "";
  showRepeatSplit.value = true;
}

async function handleRepeatSplit() {
  if (!repeatForm.name.trim()) {
    message.warning("请输入任务名称");
    return;
  }
  if (!repeatForm.start_date) {
    message.warning("请选择起始日期");
    return;
  }
  const start_date = format(new Date(repeatForm.start_date), "yyyy-MM-dd");
  const end_date = repeatForm.is_repeat && repeatForm.end_date
    ? format(new Date(repeatForm.end_date), "yyyy-MM-dd")
    : null;

  if (repeatForm.is_repeat && !repeatForm.end_date) {
    message.warning("重复任务请选择结束日期");
    return;
  }

  try {
    const input: RepeatSplitInput = {
      goal_id: repeatForm.goal_id,
      name: repeatForm.name,
      start_date,
      end_date,
      plan_qty: repeatForm.plan_qty,
      unit: repeatForm.unit,
    };
    const tasks = await goalApi.repeatSplit(input);
    message.success(`已生成 ${tasks.length} 个任务`);
    showRepeatSplit.value = false;
    await goalStore.fetchGoalTree();
    await goalStore.fetchProgresses();
    expandedNodes.value.add(repeatForm.goal_id);
  } catch (e) {
    message.error(String(e));
  }
}

// ===== 任务创建 =====
function openCreateTaskModal(goalId: string) {
  taskModalMode.value = "create";
  taskForm.task_id = "";
  taskForm.goal_id = goalId;
  taskForm.name = "";
  taskForm.plan_date = Date.now();
  taskForm.plan_qty = 1;
  taskForm.unit = "个";
  showTaskModal.value = true;
}

async function handleSaveTask() {
  if (!taskForm.name.trim()) {
    message.warning("请输入任务名称");
    return;
  }
  const plan_date = taskForm.plan_date
    ? format(new Date(taskForm.plan_date), "yyyy-MM-dd")
    : null;
  try {
    if (taskModalMode.value === "create") {
      const input: CreateTaskInput = {
        goal_id: taskForm.goal_id,
        name: taskForm.name,
        plan_date,
        plan_qty: taskForm.plan_qty,
        unit: taskForm.unit,
      };
      await taskApi.createTask(input);
      message.success("任务创建成功");
    } else {
      const input: UpdateTaskInput = {
        task_id: taskForm.task_id,
        name: taskForm.name,
        plan_date: plan_date ?? "",
        plan_qty: taskForm.plan_qty,
      };
      await taskApi.updateTask(input);
      message.success("任务已更新");
    }
    showTaskModal.value = false;
    await goalStore.fetchGoalTree();
    await goalStore.fetchProgresses();
  } catch (e) {
    message.error(String(e));
  }
}

function openEditTaskModal(task: Task) {
  taskModalMode.value = "edit";
  taskForm.task_id = task.id;
  taskForm.goal_id = task.goal_id;
  taskForm.name = task.name;
  taskForm.plan_date = task.plan_date
    ? parseISO(task.plan_date).getTime()
    : null;
  taskForm.plan_qty = task.plan_qty;
  taskForm.unit = task.unit;
  showTaskModal.value = true;
}

// ===== 任务操作 =====
async function handleCompleteTask(task: Task) {
  try {
    await taskApi.completeTask({
      task_id: task.id,
      actual_qty: task.plan_qty,
    });
    message.success("任务已完成");
    await goalStore.fetchGoalTree();
    await goalStore.fetchProgresses();
  } catch (e) {
    message.error(String(e));
  }
}

async function handleSkipTask(task: Task) {
  try {
    await taskApi.skipTask(task.id);
    message.success("任务已跳过");
    await goalStore.fetchGoalTree();
    await goalStore.fetchProgresses();
  } catch (e) {
    message.error(String(e));
  }
}

function openBackfillModal(task: Task) {
  backfillForm.task_id = task.id;
  backfillForm.task_name = task.name;
  backfillForm.goal_id = task.goal_id;
  backfillForm.plan_qty = task.plan_qty;
  backfillForm.actual_qty = task.actual_qty;
  backfillForm.unit = task.unit;
  showBackfillModal.value = true;
}

async function handleConfirmBackfill() {
  try {
    await taskApi.backfillTask({
      task_id: backfillForm.task_id,
      actual_qty: backfillForm.actual_qty,
    });
    message.success("补完成已保存");
    showBackfillModal.value = false;
    await goalStore.fetchGoalTree();
    await goalStore.fetchProgresses();
  } catch (e) {
    message.error(String(e));
  }
}

async function handleDeleteTask(task: Task) {
  try {
    await taskApi.deleteTask(task.id);
    message.success("任务已删除");
    await goalStore.fetchGoalTree();
    await goalStore.fetchProgresses();
  } catch (e) {
    message.error(String(e));
  }
}

// ===== 重新规划 =====
async function handleReplanPreview(goal: Goal) {
  try {
    replanPreview.value = await goalApi.replanPreview(goal.id);
    replanGoalId.value = goal.id;
    showReplanModal.value = true;
  } catch (e) {
    message.error(String(e));
  }
}

async function handleReplanConfirm() {
  try {
    const result = await goalApi.replanGoal(replanGoalId.value);
    message.success(
      `已重新规划：更新 ${result.updated_count} 个任务，保留 ${result.retained_count} 个手动任务`,
    );
    showReplanModal.value = false;
    replanPreview.value = null;
    await goalStore.fetchGoalTree();
    await goalStore.fetchProgresses();
  } catch (e) {
    message.error(String(e));
  }
}

const replanColumns: DataTableColumns<ReplanPreview["items"][0]> = [
  { title: "日期", key: "plan_date", width: 110 },
  { title: "任务名", key: "name", ellipsis: { tooltip: true } },
  { title: "原计划", key: "old_plan_qty", width: 90 },
  {
    title: "新计划",
    key: "new_plan_qty",
    width: 100,
    render: (row) =>
      row.retained ? `${row.new_plan_qty} (保留)` : `${row.new_plan_qty}`,
  },
];

// ===== 辅助函数 =====
function getDaysLeft(deadline: string | null): string {
  if (!deadline) return "未设置截止日期";
  const days = differenceInCalendarDays(parseISO(deadline), new Date());
  if (days < 0) return `已逾期 ${-days} 天`;
  if (days === 0) return "今天截止";
  return `剩余 ${days} 天`;
}

/** 任务行操作菜单 */
function buildTaskActions(task: Task): DropdownOption[] {
  const actions: DropdownOption[] = [];
  if (task.status !== "done" && task.status !== "skipped") {
    actions.push({
      label: "标记完成",
      key: "complete",
      icon: () => h(Icon, { icon: "mdi:check-circle-outline" }),
    });
    actions.push({
      label: "跳过",
      key: "skip",
      icon: () => h(Icon, { icon: "mdi:skip-next-outline" }),
    });
  }
  actions.push({
    label: "补完成",
    key: "backfill",
    icon: () => h(Icon, { icon: "mdi:history" }),
  });
  actions.push({
    label: "编辑",
    key: "edit",
    icon: () => h(Icon, { icon: "mdi:pencil-outline" }),
  });
  actions.push({ type: "divider", key: "d1" });
  actions.push({
    label: "删除",
    key: "delete",
    icon: () => h(Icon, { icon: "mdi:delete-outline" }),
  });
  return actions;
}

function handleTaskAction(key: string, task: Task) {
  switch (key) {
    case "complete":
      handleCompleteTask(task);
      break;
    case "skip":
      handleSkipTask(task);
      break;
    case "backfill":
      openBackfillModal(task);
      break;
    case "edit":
      openEditTaskModal(task);
      break;
    case "delete":
      dialog.warning({
        title: "删除任务",
        content: `确定删除任务"${task.name}"？此操作不可撤销。`,
        positiveText: "删除",
        negativeText: "取消",
        onPositiveClick: () => handleDeleteTask(task),
      });
      break;
  }
}
</script>

<template>
  <div class="space-y-4">
    <div class="flex items-center justify-between">
      <h2 class="text-lg font-semibold flex items-center gap-2">
        <Icon icon="mdi:file-tree-outline" width="22" class="text-brand-500" />
        目标树
      </h2>
      <NButton type="primary" @click="openCreateGoalModal(null)">
        <template #icon><Icon icon="mdi:plus" /></template>
        创建总目标
      </NButton>
    </div>

    <!-- 提示 -->
    <div
      class="text-xs text-gray-400 flex items-center gap-1 bg-gray-50 px-3 py-1.5 rounded"
    >
      <Icon icon="mdi:information-outline" width="14" />
      总目标下可添加子目标和子任务；完成所有子目标+子任务即完成总目标
    </div>

    <!-- 目标树 -->
    <div v-if="goalStore.goalTree.length > 0" class="space-y-2">
      <NCard
        v-for="rootNode in goalStore.goalTree"
        :key="rootNode.goal.id"
        size="small"
      >
        <!-- 总目标头部 -->
        <div
          class="flex items-center gap-3 cursor-pointer"
          @click="toggleNode(rootNode.goal.id)"
        >
          <Icon
            :icon="
              expandedNodes.has(rootNode.goal.id)
                ? 'mdi:chevron-down'
                : 'mdi:chevron-right'
            "
            width="20"
            class="text-gray-400"
          />
          <Icon
            :icon="rootNode.is_completed ? 'mdi:check-circle' : 'mdi:target'"
            width="20"
            :class="rootNode.is_completed ? 'text-green-500' : 'text-brand-500'"
          />
          <div class="flex-1">
            <div
              class="font-medium"
              :class="{ 'line-through text-gray-400': rootNode.is_completed }"
            >
              {{ rootNode.goal.name }}
            </div>
            <div class="text-xs text-gray-500 flex items-center gap-3 mt-0.5">
              <span
                :class="{
                  'text-red-500': getDaysLeft(rootNode.goal.deadline).includes('逾期'),
                }"
              >
                {{ getDaysLeft(rootNode.goal.deadline) }}
              </span>
              <span v-if="rootNode.goal.total_qty > 0">
                总量：{{ rootNode.goal.total_qty }}{{ rootNode.goal.unit }}
              </span>
              <span>
                进度：{{ Math.round(rootNode.progress * 100) }}%
              </span>
              <NTag v-if="rootNode.is_completed" size="tiny" type="success" :bordered="false">
                已完成
              </NTag>
            </div>
          </div>
          <NSpace :size="4" @click.stop>
            <NButton
              size="tiny"
              quaternary
              type="info"
              @click="openCreateGoalModal(rootNode.goal.id)"
            >
              <template #icon><Icon icon="mdi:folder-plus-outline" /></template>
              子目标
            </NButton>
            <NButton
              size="tiny"
              type="primary"
              ghost
              :disabled="!rootNode.goal.deadline || rootNode.goal.total_qty <= 0"
              @click="handleAutoSplit(rootNode.goal)"
            >
              <template #icon><Icon icon="mdi:auto-fix" /></template>
              视频拆解
            </NButton>
            <NButton
              size="tiny"
              type="success"
              ghost
              @click="openRepeatSplitModal(rootNode.goal)"
            >
              <template #icon><Icon icon="mdi:repeat" /></template>
              重复任务
            </NButton>
            <NButton
              size="tiny"
              type="warning"
              ghost
              :disabled="!rootNode.goal.deadline"
              @click="handleReplanPreview(rootNode.goal)"
            >
              <template #icon><Icon icon="mdi:refresh-circle" /></template>
              重新规划
            </NButton>
            <NButton
              size="tiny"
              quaternary
              type="info"
              @click="openCreateTaskModal(rootNode.goal.id)"
            >
              <template #icon><Icon icon="mdi:plus-box-outline" /></template>
              任务
            </NButton>
            <NPopconfirm @positive-click="handleDeleteGoal(rootNode.goal)">
              <template #trigger>
                <NButton size="tiny" quaternary type="error">
                  <Icon icon="mdi:delete" />
                </NButton>
              </template>
              确定删除总目标"{{ rootNode.goal.name }}"？所有子目标和任务将一并删除。
            </NPopconfirm>
          </NSpace>
        </div>

        <!-- 总目标展开内容 -->
        <div v-if="expandedNodes.has(rootNode.goal.id)" class="mt-3 ml-8 space-y-2">
          <!-- 子目标列表 -->
          <NCard
            v-for="subNode in rootNode.sub_goals"
            :key="subNode.goal.id"
            size="small"
            :bordered="true"
          >
            <!-- 子目标头部 -->
            <div
              class="flex items-center gap-2 cursor-pointer"
              @click="toggleNode(subNode.goal.id)"
            >
              <Icon
                :icon="
                  expandedNodes.has(subNode.goal.id)
                    ? 'mdi:chevron-down'
                    : 'mdi:chevron-right'
                "
                width="16"
                class="text-gray-400"
              />
              <Icon
                :icon="subNode.is_completed ? 'mdi:check-circle' : 'mdi:target-variant'"
                width="16"
                :class="subNode.is_completed ? 'text-green-500' : 'text-blue-500'"
              />
              <span
                class="flex-1 text-sm font-medium"
                :class="{ 'line-through text-gray-400': subNode.is_completed }"
              >
                {{ subNode.goal.name }}
              </span>
              <NProgress
                type="line"
                :percentage="Math.round(subNode.progress * 100)"
                :show-indicator="false"
                :height="4"
                style="width: 80px"
              />
              <span class="text-xs text-gray-500">
                {{ Math.round(subNode.progress * 100) }}%
              </span>
              <NTag v-if="subNode.is_completed" size="tiny" type="success" :bordered="false">
                完成
              </NTag>
              <NSpace :size="2" @click.stop>
                <NButton
                  size="tiny"
                  quaternary
                  type="primary"
                  ghost
                  :disabled="!subNode.goal.deadline || subNode.goal.total_qty <= 0"
                  @click="handleAutoSplit(subNode.goal)"
                >
                  <Icon icon="mdi:auto-fix" width="14" />
                </NButton>
                <NButton
                  size="tiny"
                  quaternary
                  type="success"
                  @click="openRepeatSplitModal(subNode.goal)"
                >
                  <Icon icon="mdi:repeat" width="14" />
                </NButton>
                <NButton
                  size="tiny"
                  quaternary
                  type="info"
                  @click="openCreateTaskModal(subNode.goal.id)"
                >
                  <Icon icon="mdi:plus" width="14" />
                </NButton>
                <NPopconfirm @positive-click="handleDeleteGoal(subNode.goal)">
                  <template #trigger>
                    <NButton size="tiny" quaternary type="error">
                      <Icon icon="mdi:close" width="14" />
                    </NButton>
                  </template>
                  确定删除子目标"{{ subNode.goal.name }}"？
                </NPopconfirm>
              </NSpace>
            </div>

            <!-- 子目标下的任务 -->
            <div v-if="expandedNodes.has(subNode.goal.id)" class="ml-6 mt-1 space-y-0.5">
              <div
                v-for="task in subNode.tasks"
                :key="task.id"
                class="flex items-center gap-2 px-3 py-1 rounded hover:bg-gray-50 text-sm"
              >
                <Icon
                  :icon="STATUS_META[task.status].icon"
                  :color="STATUS_META[task.status].color"
                  width="14"
                />
                <span
                  class="flex-1 truncate"
                  :class="{ 'line-through text-gray-400': task.status === 'done' }"
                >
                  {{ task.name }}
                </span>
                <span class="text-xs text-gray-500">{{ task.plan_date }}</span>
                <span class="text-xs text-gray-500">
                  {{ task.actual_qty }}/{{ task.plan_qty }}{{ task.unit }}
                </span>
                <NTag v-if="task.source === 'manual'" size="tiny" :bordered="false" type="warning">
                  手动
                </NTag>
                <NDropdown
                  trigger="click"
                  :options="buildTaskActions(task)"
                  @select="(key: string) => handleTaskAction(key, task)"
                >
                  <NButton size="tiny" quaternary>
                    <Icon icon="mdi:dots-horizontal" width="16" />
                  </NButton>
                </NDropdown>
              </div>
              <div v-if="subNode.tasks.length === 0" class="text-xs text-gray-400 py-1 px-3">
                暂无任务
              </div>
            </div>
          </NCard>

          <!-- 总目标直属任务 -->
          <div v-if="rootNode.tasks.length > 0" class="space-y-0.5">
            <div
              v-if="rootNode.sub_goals.length > 0"
              class="text-xs text-gray-400 px-3 py-1"
            >
              直属任务
            </div>
            <div
              v-for="task in rootNode.tasks"
              :key="task.id"
              class="flex items-center gap-2 px-3 py-1.5 rounded hover:bg-gray-50 text-sm"
            >
              <Icon
                :icon="STATUS_META[task.status].icon"
                :color="STATUS_META[task.status].color"
                width="16"
              />
              <span
                class="flex-1 truncate"
                :class="{ 'line-through text-gray-400': task.status === 'done' }"
              >
                {{ task.name }}
              </span>
              <span class="text-xs text-gray-500">{{ task.plan_date }}</span>
              <NTag
                size="tiny"
                :bordered="false"
                :type="task.source === 'auto' ? 'info' : 'warning'"
              >
                {{ task.source === "auto" ? "自动" : "手动" }}
              </NTag>
              <span class="text-xs text-gray-500">
                {{ task.actual_qty }}/{{ task.plan_qty }}{{ task.unit }}
              </span>
              <NDropdown
                trigger="click"
                :options="buildTaskActions(task)"
                @select="(key: string) => handleTaskAction(key, task)"
              >
                <NButton size="tiny" quaternary>
                  <Icon icon="mdi:dots-horizontal" width="16" />
                </NButton>
              </NDropdown>
            </div>
          </div>

          <!-- 空状态 -->
          <div
            v-if="rootNode.sub_goals.length === 0 && rootNode.tasks.length === 0"
            class="text-sm text-gray-400 py-2"
          >
            暂无子目标和任务，点击上方按钮开始添加
          </div>
        </div>
      </NCard>
    </div>

    <NEmpty v-else description="还没有目标，点击右上角创建第一个总目标吧" />

    <!-- 创建目标弹窗 -->
    <NModal
      v-model:show="showCreateGoal"
      preset="card"
      :title="createGoalParentId ? '创建子目标' : '创建总目标'"
      style="width: 480px"
    >
      <NForm label-placement="top">
        <NFormItem label="目标名称" required>
          <NInput
            v-model:value="goalForm.name"
            placeholder="如：Vue 框架学习"
          />
        </NFormItem>
        <NFormItem label="截止日期">
          <NDatePicker
            v-model:value="goalForm.deadline"
            type="date"
            clearable
            :is-date-disabled="(ts: number) => ts < Date.now() - 86400000"
          />
        </NFormItem>
        <NSpace>
          <NFormItem label="总量（视频拆解用）">
            <NInputNumber v-model:value="goalForm.total_qty" :min="0" />
          </NFormItem>
          <NFormItem label="单位">
            <NSelect
              v-model:value="goalForm.unit"
              :options="unitOptions"
              style="width: 100px"
            />
          </NFormItem>
        </NSpace>
      </NForm>
      <template #footer>
        <NSpace justify="end">
          <NButton @click="showCreateGoal = false">取消</NButton>
          <NButton type="primary" @click="handleCreateGoal">创建</NButton>
        </NSpace>
      </template>
    </NModal>

    <!-- 重复拆解弹窗 -->
    <NModal
      v-model:show="showRepeatSplit"
      preset="card"
      title="添加重复任务"
      style="width: 500px"
    >
      <div class="space-y-3">
        <div class="text-sm text-gray-600">
          目标：<strong>{{ repeatForm.goal_name }}</strong>
        </div>
        <NFormItem label="任务名称" :show-feedback="false">
          <NInput
            v-model:value="repeatForm.name"
            placeholder="如：完成 Vue 练习题"
          />
        </NFormItem>
        <div class="flex items-center gap-2">
          <NCheckbox v-model:checked="repeatForm.is_repeat">
            每天重复
          </NCheckbox>
          <span class="text-xs text-gray-400">
            {{ repeatForm.is_repeat ? '在日期范围内每天生成一个任务' : '只生成一个单次任务' }}
          </span>
        </div>
        <NSpace>
          <NFormItem :label="repeatForm.is_repeat ? '起始日期' : '完成日期'" :show-feedback="false">
            <NDatePicker
              v-model:value="repeatForm.start_date"
              type="date"
            />
          </NFormItem>
          <NFormItem v-if="repeatForm.is_repeat" label="结束日期" :show-feedback="false">
            <NDatePicker
              v-model:value="repeatForm.end_date"
              type="date"
              clearable
            />
          </NFormItem>
        </NSpace>
        <NSpace>
          <NFormItem label="计划数量" :show-feedback="false">
            <NInputNumber v-model:value="repeatForm.plan_qty" :min="1" />
          </NFormItem>
          <NFormItem label="单位" :show-feedback="false">
            <NSelect
              v-model:value="repeatForm.unit"
              :options="unitOptions"
              style="width: 100px"
            />
          </NFormItem>
        </NSpace>
        <div class="text-xs text-blue-500 bg-blue-50 px-3 py-2 rounded">
          <Icon icon="mdi:information" class="inline-block mr-1" />
          纯文字类任务（如练习题）可设置每天重复；视频类任务请使用"视频拆解"
        </div>
      </div>
      <template #footer>
        <NSpace justify="end">
          <NButton @click="showRepeatSplit = false">取消</NButton>
          <NButton type="primary" @click="handleRepeatSplit">生成任务</NButton>
        </NSpace>
      </template>
    </NModal>

    <!-- 创建/编辑任务弹窗 -->
    <NModal
      v-model:show="showTaskModal"
      preset="card"
      :title="taskModalMode === 'create' ? '添加任务' : '编辑任务'"
      style="width: 480px"
    >
      <NForm label-placement="top">
        <NFormItem label="任务名称" required>
          <NInput
            v-model:value="taskForm.name"
            placeholder="如：背诵第1单元单词"
          />
        </NFormItem>
        <NFormItem label="计划日期">
          <NDatePicker
            v-model:value="taskForm.plan_date"
            type="date"
            clearable
          />
        </NFormItem>
        <NSpace>
          <NFormItem label="计划数量">
            <NInputNumber v-model:value="taskForm.plan_qty" :min="0" />
          </NFormItem>
          <NFormItem label="单位">
            <NSelect
              v-model:value="taskForm.unit"
              :options="unitOptions"
              style="width: 100px"
              :disabled="taskModalMode === 'edit'"
            />
          </NFormItem>
        </NSpace>
      </NForm>
      <template #footer>
        <NSpace justify="end">
          <NButton @click="showTaskModal = false">取消</NButton>
          <NButton type="primary" @click="handleSaveTask">
            {{ taskModalMode === "create" ? "创建" : "保存" }}
          </NButton>
        </NSpace>
      </template>
    </NModal>

    <!-- 补完成弹窗 -->
    <NModal
      v-model:show="showBackfillModal"
      preset="card"
      title="补完成"
      style="width: 420px"
    >
      <div class="space-y-3">
        <div class="text-sm text-gray-600">
          任务：<strong>{{ backfillForm.task_name }}</strong>
        </div>
        <div class="text-xs text-gray-400">
          计划数量：{{ backfillForm.plan_qty }}{{ backfillForm.unit }}
        </div>
        <NFormItem label="实际完成量" :show-feedback="false">
          <NInputNumber
            v-model:value="backfillForm.actual_qty"
            :min="0"
            :step="1"
            style="width: 100%"
          />
        </NFormItem>
      </div>
      <template #footer>
        <NSpace justify="end">
          <NButton @click="showBackfillModal = false">取消</NButton>
          <NButton type="primary" @click="handleConfirmBackfill">确认补完成</NButton>
        </NSpace>
      </template>
    </NModal>

    <!-- 重新规划预览弹窗 -->
    <NModal
      v-model:show="showReplanModal"
      preset="card"
      title="重新规划预览"
      style="width: 720px"
    >
      <div v-if="replanPreview" class="space-y-3">
        <div class="text-sm text-gray-600 space-y-1">
          <div>
            目标：<strong>{{ replanPreview.goal_name }}</strong>
          </div>
          <div>
            剩余天数：<strong>{{ replanPreview.remaining_days }} 天</strong> ｜
            剩余总量：<strong>{{ replanPreview.remaining_qty }}</strong>
          </div>
          <div>
            每日基础量：{{ replanPreview.daily_base }} ｜ 余数分前
            {{ replanPreview.remainder }} 天
          </div>
        </div>
        <NDataTable
          :columns="replanColumns"
          :data="replanPreview.items"
          :max-height="320"
          size="small"
          :bordered="false"
        />
      </div>
      <template #footer>
        <NSpace justify="end">
          <NButton @click="showReplanModal = false">取消</NButton>
          <NButton type="warning" @click="handleReplanConfirm">确认重新规划</NButton>
        </NSpace>
      </template>
    </NModal>
  </div>
</template>
