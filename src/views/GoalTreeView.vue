<script setup lang="ts">
import { onMounted, ref, reactive, h, provide } from "vue";
import {
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
  NDataTable,
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
  UpdateGoalInput,
  RepeatSplitInput,
} from "@/types";
import { format, parseISO, differenceInCalendarDays } from "date-fns";
import GoalTreeNodeItem, {
  goalTreeApiKey,
  type GoalTreeApi,
} from "@/components/GoalTreeNodeItem.vue";

const goalStore = useGoalStore();
const message = useMessage();
const dialog = useDialog();

// ===== 创建/编辑目标（总目标 or 子目标）=====
const showGoalModal = ref(false);
const goalModalMode = ref<"create" | "edit">("create");
const createGoalParentId = ref<string | null>(null); // null=总目标, string=子目标
const editingGoalId = ref<string>("");
const goalForm = reactive({
  name: "",
  deadline: null as number | null,
  total_qty: 0,
  unit: "个",
});

const unitOptions = [
  // 数量类
  { label: "个", value: "个" },
  { label: "次", value: "次" },
  // 无
  { label: "无", value: "" },
  // 时间类
  { label: "分钟", value: "分钟" },
  { label: "小时", value: "小时" },
  // 学习内容类
  { label: "页", value: "页" },
  { label: "章", value: "章" },
  { label: "节", value: "节" },
  { label: "篇", value: "篇" },
  { label: "本", value: "本" },
  { label: "集", value: "集" },
  { label: "讲", value: "讲" },
  // 题目类
  { label: "道", value: "道" },
  { label: "题", value: "题" },
  // 词汇类
  { label: "词", value: "词" },
  { label: "字", value: "字" },
];

// ===== 展开状态 =====
const expandedNodes = ref<Set<string>>(new Set());

// ===== 任务创建/编辑弹窗 =====
const showTaskModal = ref(false);
const taskModalMode = ref<"create" | "edit">("create");
const taskForm = reactive({
  task_id: "",
  goal_id: "",
  name: "",
  plan_date: null as number | null,
  is_repeat: false,
  end_date: null as number | null,
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

// ===== 目标创建/编辑 =====
function openCreateGoalModal(parentId: string | null) {
  goalModalMode.value = "create";
  createGoalParentId.value = parentId;
  editingGoalId.value = "";
  goalForm.name = "";
  goalForm.deadline = null;
  goalForm.total_qty = 0;
  goalForm.unit = "个";
  showGoalModal.value = true;
}

function openEditGoalModal(goal: Goal) {
  goalModalMode.value = "edit";
  editingGoalId.value = goal.id;
  createGoalParentId.value = goal.parent_id;
  goalForm.name = goal.name;
  goalForm.deadline = goal.deadline ? parseISO(goal.deadline).getTime() : null;
  goalForm.total_qty = goal.total_qty;
  goalForm.unit = goal.unit || "个";
  showGoalModal.value = true;
}

async function handleSaveGoal() {
  if (!goalForm.name.trim()) {
    message.warning("请输入目标名称");
    return;
  }
  const deadline = goalForm.deadline
    ? format(new Date(goalForm.deadline), "yyyy-MM-dd")
    : null;
  try {
    if (goalModalMode.value === "create") {
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
      if (createGoalParentId.value) {
        expandedNodes.value.add(createGoalParentId.value);
      }
    } else {
      const input: UpdateGoalInput = {
        id: editingGoalId.value,
        name: goalForm.name,
        deadline,
        total_qty: goalForm.total_qty,
        unit: goalForm.unit,
      };
      await goalApi.updateGoal(input);
      message.success("目标已更新");
    }
    showGoalModal.value = false;
    await goalStore.fetchGoalTree();
    await goalStore.fetchProgresses();
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

// ===== 任务创建 =====
function openCreateTaskModal(goalId: string) {
  taskModalMode.value = "create";
  taskForm.task_id = "";
  taskForm.goal_id = goalId;
  taskForm.name = "";
  taskForm.plan_date = Date.now();
  taskForm.is_repeat = false;
  taskForm.end_date = null;
  taskForm.plan_qty = 1;
  taskForm.unit = "个";
  showTaskModal.value = true;
}

async function handleSaveTask() {
  if (!taskForm.name.trim()) {
    message.warning("请输入任务名称");
    return;
  }
  try {
    if (taskModalMode.value === "create") {
      if (taskForm.is_repeat) {
        // 重复任务：按日期范围每天生成一个任务
        if (!taskForm.plan_date) {
          message.warning("请选择起始日期");
          return;
        }
        if (!taskForm.end_date) {
          message.warning("重复任务请选择结束日期");
          return;
        }
        const start_date = format(new Date(taskForm.plan_date), "yyyy-MM-dd");
        const end_date = format(new Date(taskForm.end_date), "yyyy-MM-dd");
        const input: RepeatSplitInput = {
          goal_id: taskForm.goal_id,
          name: taskForm.name,
          start_date,
          end_date,
          plan_qty: taskForm.plan_qty,
          unit: taskForm.unit,
        };
        const tasks = await goalApi.repeatSplit(input);
        message.success(`已生成 ${tasks.length} 个任务`);
        expandedNodes.value.add(taskForm.goal_id);
      } else {
        // 单次任务
        const plan_date = taskForm.plan_date
          ? format(new Date(taskForm.plan_date), "yyyy-MM-dd")
          : null;
        const input: CreateTaskInput = {
          goal_id: taskForm.goal_id,
          name: taskForm.name,
          plan_date,
          plan_qty: taskForm.plan_qty,
          unit: taskForm.unit,
        };
        await taskApi.createTask(input);
        message.success("任务创建成功");
      }
    } else {
      const plan_date = taskForm.plan_date
        ? format(new Date(taskForm.plan_date), "yyyy-MM-dd")
        : null;
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

/** 在已加载的目标树中查找某目标下的全部任务 */
function findTasksByGoalId(goalId: string): Task[] {
  for (const rootNode of goalStore.goalTree) {
    if (rootNode.goal.id === goalId) return rootNode.tasks;
    for (const subNode of rootNode.sub_goals) {
      if (subNode.goal.id === goalId) return subNode.tasks;
    }
  }
  return [];
}

/**
 * 获取与指定任务同批生成的关联任务。
 * 自动拆解/重复任务由后端一次性生成，共享 source='auto' 与相同的 created_at。
 * 返回长度 > 1 表示该任务属于一个批量（自动拆解或重复任务批次）。
 */
function getBatchTasks(task: Task): Task[] {
  if (task.source !== "auto") return [task];
  return findTasksByGoalId(task.goal_id).filter(
    (t) => t.source === "auto" && t.created_at === task.created_at,
  );
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

/** 批量删除同批生成的关联任务 */
async function handleDeleteBatch(tasks: Task[]) {
  try {
    await Promise.all(tasks.map((t) => taskApi.deleteTask(t.id)));
    message.success(`已删除 ${tasks.length} 个关联任务`);
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
    case "delete": {
      const batch = getBatchTasks(task);
      if (batch.length > 1) {
        // 自动拆解/重复任务批次：可选择仅删当前或删除全部关联
        dialog.warning({
          title: "删除任务",
          content: `该任务由自动拆解/重复任务生成，共 ${batch.length} 个关联任务。"删除全部关联"将一并删除这 ${batch.length} 个任务。操作不可撤销。`,
          positiveText: "仅删除当前",
          negativeText: `删除全部关联 (${batch.length})`,
          onPositiveClick: () => handleDeleteTask(task),
          onNegativeClick: () => handleDeleteBatch(batch),
        });
      } else {
        dialog.warning({
          title: "删除任务",
          content: `确定删除任务"${task.name}"？此操作不可撤销。`,
          positiveText: "删除",
          negativeText: "取消",
          onPositiveClick: () => handleDeleteTask(task),
        });
      }
      break;
    }
  }
}

// ===== 拖拽归属：共享状态 + 跨目标移动 =====
const draggingTaskId = ref<string | null>(null);
const dragOverGoalId = ref<string | null>(null);

async function handleMoveTask(task: Task, targetGoalId: string) {
  try {
    await taskApi.moveTask({
      task_id: task.id,
      goal_id: targetGoalId,
    });
    message.success(`已将任务"${task.name}"移动到新目标`);
    await goalStore.fetchGoalTree();
    await goalStore.fetchProgresses();
    // 自动展开目标节点，便于用户立刻看到移动结果
    expandedNodes.value.add(targetGoalId);
  } catch (e) {
    message.error(String(e));
  }
}

// ===== 向递归子组件注入树 API（避免逐层 prop 透传）=====
const treeApi: GoalTreeApi = {
  expandedNodes,
  toggleNode,
  getDaysLeft,
  openCreateGoalModal,
  openCreateTaskModal,
  openEditGoalModal,
  handleAutoSplit,
  handleReplanPreview,
  handleDeleteGoal,
  buildTaskActions,
  handleTaskAction,
  handleMoveTask,
  draggingTaskId,
  dragOverGoalId,
};
provide(goalTreeApiKey, treeApi);
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
      支持任意层级嵌套子目标；任务行可拖拽到其他目标下调整归属
    </div>

    <!-- 目标树 -->
    <div v-if="goalStore.goalTree.length > 0" class="space-y-2">
      <GoalTreeNodeItem
        v-for="rootNode in goalStore.goalTree"
        :key="rootNode.goal.id"
        :node="rootNode"
        :level="0"
      />
    </div>

    <NEmpty v-else description="还没有目标，点击右上角创建第一个总目标吧" />

    <!-- 创建/编辑目标弹窗 -->
    <NModal
      v-model:show="showGoalModal"
      preset="card"
      :title="
        goalModalMode === 'edit'
          ? '编辑目标'
          : createGoalParentId
            ? '创建子目标'
            : '创建总目标'
      "
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
          <NButton @click="showGoalModal = false">取消</NButton>
          <NButton type="primary" @click="handleSaveGoal">
            {{ goalModalMode === "edit" ? "保存" : "创建" }}
          </NButton>
        </NSpace>
      </template>
    </NModal>

    <!-- 创建/编辑任务弹窗（含重复任务） -->
    <NModal
      v-model:show="showTaskModal"
      preset="card"
      :title="taskModalMode === 'create' ? '添加任务' : '编辑任务'"
      style="width: 500px"
    >
      <div class="space-y-3">
        <NFormItem label="任务名称" :show-feedback="false" required>
          <NInput
            v-model:value="taskForm.name"
            placeholder="如：完成 Vue 练习题"
          />
        </NFormItem>
        <!-- 重复开关（仅创建模式显示） -->
        <div v-if="taskModalMode === 'create'" class="flex items-center gap-2">
          <NCheckbox v-model:checked="taskForm.is_repeat"> 每天重复 </NCheckbox>
          <span class="text-xs text-gray-400">
            {{
              taskForm.is_repeat
                ? "在日期范围内每天生成一个任务"
                : "只生成一个单次任务"
            }}
          </span>
        </div>
        <NSpace>
          <NFormItem
            :label="taskForm.is_repeat ? '起始日期' : '计划日期'"
            :show-feedback="false"
          >
            <NDatePicker
              v-model:value="taskForm.plan_date"
              type="date"
              clearable
            />
          </NFormItem>
          <NFormItem
            v-if="taskForm.is_repeat && taskModalMode === 'create'"
            label="结束日期"
            :show-feedback="false"
          >
            <NDatePicker
              v-model:value="taskForm.end_date"
              type="date"
              clearable
            />
          </NFormItem>
        </NSpace>
        <NSpace>
          <NFormItem label="计划数量" :show-feedback="false">
            <NInputNumber v-model:value="taskForm.plan_qty" :min="0" />
          </NFormItem>
          <NFormItem label="单位" :show-feedback="false">
            <NSelect
              v-model:value="taskForm.unit"
              :options="unitOptions"
              style="width: 100px"
              :disabled="taskModalMode === 'edit'"
            />
          </NFormItem>
        </NSpace>
        <div
          v-if="taskModalMode === 'create'"
          class="text-xs text-blue-500 bg-blue-50 px-3 py-2 rounded"
        >
          <Icon icon="mdi:information" class="inline-block mr-1" />
          纯文字类任务（如练习题）可勾选"每天重复"在日期范围内每天生成；视频类任务请使用"视频拆解"
        </div>
      </div>
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
          <NButton type="primary" @click="handleConfirmBackfill"
            >确认补完成</NButton
          >
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
          <NButton type="warning" @click="handleReplanConfirm"
            >确认重新规划</NButton
          >
        </NSpace>
      </template>
    </NModal>
  </div>
</template>
