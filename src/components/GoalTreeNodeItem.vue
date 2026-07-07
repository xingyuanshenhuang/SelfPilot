<script lang="ts">
import type { Ref } from "vue";
import type { DropdownOption } from "naive-ui";
import type { Goal, Task } from "@/types";

/**
 * 递归目标树节点共享的 API。
 * 由 GoalTreeView 通过 provide("goalTreeApi") 注入，避免逐层 prop 透传。
 */
export interface GoalTreeApi {
  expandedNodes: Ref<Set<string>>;
  toggleNode: (id: string) => void;
  getDaysLeft: (deadline: string | null) => string;
  openCreateGoalModal: (parentId: string | null) => void;
  openCreateTaskModal: (goalId: string) => void;
  openEditGoalModal: (goal: Goal) => void;
  handleAutoSplit: (goal: Goal) => void;
  handleReplanPreview: (goal: Goal) => void;
  handleDeleteGoal: (goal: Goal) => void;
  buildTaskActions: (task: Task) => DropdownOption[];
  handleTaskAction: (key: string, task: Task) => void;

  /** 拖拽任务：移动到指定目标，beforeTaskId 指定插入位置 */
  handleMoveTask: (
    task: Task,
    targetGoalId: string,
    beforeTaskId?: string | null,
  ) => Promise<void>;
  /** 拖拽目标：移动到新父目标下，beforeGoalId 指定插入位置 */
  handleMoveGoal: (
    goalId: string,
    newParentId: string | null,
    beforeGoalId?: string | null,
  ) => Promise<void>;

  /** 当前正在拖拽的任务 ID */
  draggingTaskId: Ref<string | null>;
  /** 当前正在拖拽的目标对象 */
  draggingGoal: Ref<Goal | null>;

  /** 拖拽悬停的目标 goal ID */
  dragOverGoalId: Ref<string | null>;
  /** 目标放置模式：before=插入上方，inside=放入内部，after=插入下方 */
  dropPosition: Ref<"before" | "inside" | "after">;
  /** 拖拽悬停的任务 ID（用于任务间排序） */
  dragOverTaskId: Ref<string | null>;
  /** 任务放置模式：before=插入上方，after=插入下方 */
  taskDropPosition: Ref<"before" | "after">;
  /** 查找目标的下一个同级目标 ID（用于目标拖拽插入下方） */
  findNextSiblingId: (goalId: string) => string | null;
}

/** provide/inject 键 */
export const goalTreeApiKey = "goalTreeApi";

/** 拖拽任务时通过 dataTransfer 传递的 MIME 类型 */
export const TASK_DRAG_MIME = "application/x-selfpilot-task-id";
/** 拖拽目标时通过 dataTransfer 传递的 MIME 类型 */
export const GOAL_DRAG_MIME = "application/x-selfpilot-goal-id";
</script>

<script setup lang="ts">
import { computed, inject } from "vue";
import {
  NCard,
  NButton,
  NTag,
  NPopconfirm,
  NProgress,
  NDropdown,
} from "naive-ui";
import { Icon } from "@iconify/vue";
import { STATUS_META } from "@/types";
import type { GoalTreeNode } from "@/types";

const props = withDefaults(
  defineProps<{
    node: GoalTreeNode;
    /** 层级深度，0=总目标 */
    level?: number;
  }>(),
  { level: 0 },
);

const api = inject<GoalTreeApi>(goalTreeApiKey)!;
const expandedNodes = api.expandedNodes;
const toggleNode = api.toggleNode;
const getDaysLeft = api.getDaysLeft;

const isRoot = computed(() => props.level === 0);
const isExpanded = computed(() => expandedNodes.value.has(props.node.goal.id));
const hasChildren = computed(() => props.node.sub_goals.length > 0);
const goalIcon = computed(() =>
  props.node.is_completed
    ? "mdi:check-circle"
    : isRoot.value
      ? "mdi:target"
      : "mdi:target-variant",
);
const isOverdue = computed(() =>
  getDaysLeft(props.node.goal.deadline).includes("逾期"),
);

function toggle() {
  toggleNode(props.node.goal.id);
}

function onTaskSelect(key: string, task: Task) {
  api.handleTaskAction(key, task);
}

// ===== 共享拖拽状态 =====
const draggingTaskId = api.draggingTaskId;
const draggingGoal = api.draggingGoal;
const dragOverGoalId = api.dragOverGoalId;
const dropPosition = api.dropPosition;
const dragOverTaskId = api.dragOverTaskId;
const taskDropPosition = api.taskDropPosition;

// ===== 路径工具：判断祖先/后代关系（用于环检测） =====

/** target 是否是 draggedGoal 的后代（含子目标） */
function isDescendantOf(target: Goal, dragged: Goal): boolean {
  if (target.id === dragged.id) return false;
  return target.path.startsWith(dragged.path + "/");
}

/** target 是否是 draggedGoal 的祖先 */
function isAncestorOf(target: Goal, dragged: Goal): boolean {
  if (target.id === dragged.id) return false;
  return dragged.path.startsWith(target.path + "/");
}

// ===== 任务拖拽 =====

/** 任务行：开始拖拽 */
function onTaskDragStart(e: DragEvent, task: Task) {
  if (!e.dataTransfer) return;
  e.dataTransfer.setData(TASK_DRAG_MIME, task.id);
  e.dataTransfer.setData("text/plain", task.id);
  e.dataTransfer.effectAllowed = "move";
  draggingTaskId.value = task.id;
}

/** 任务行：结束拖拽 */
function onTaskDragEnd() {
  draggingTaskId.value = null;
  dragOverTaskId.value = null;
  dragOverGoalId.value = null;
}

/** 任务行：拖拽悬停（用于同级排序） */
function onTaskRowDragOver(e: DragEvent, task: Task) {
  if (!draggingTaskId.value) return;
  e.preventDefault();
  e.stopPropagation(); // 阻止冒泡到目标卡片
  if (e.dataTransfer) e.dataTransfer.dropEffect = "move";
  dragOverTaskId.value = task.id;
  // 上半部 = before，下半部 = after
  const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
  taskDropPosition.value =
    e.clientY - rect.top < rect.height / 2 ? "before" : "after";
}

/** 任务行：拖拽离开 */
function onTaskRowDragLeave(e: DragEvent, task: Task) {
  if (dragOverTaskId.value !== task.id) return;
  const rt = e.relatedTarget as Node | null;
  const current = e.currentTarget as HTMLElement;
  if (current && rt && current.contains(rt)) return;
  if (dragOverTaskId.value === task.id) {
    dragOverTaskId.value = null;
  }
}

/** 任务行：释放（同级排序） */
async function onTaskRowDrop(e: DragEvent, task: Task, index: number) {
  if (!draggingTaskId.value) return;
  e.preventDefault();
  e.stopPropagation();
  const taskId = draggingTaskId.value;
  const targetGoalId = props.node.goal.id;
  clearTaskDrag();

  const draggedTask = findTaskInTree(taskId);
  if (!draggedTask || draggedTask.id === task.id) return;

  const pos = taskDropPosition.value;
  if (pos === "before") {
    // 插入到当前任务之前
    await api.handleMoveTask(draggedTask, targetGoalId, task.id);
  } else {
    // 插入到当前任务之后：找下一个同级任务，插入其前；若为最后一个则追加末尾
    const nextTask = props.node.tasks[index + 1];
    if (nextTask && nextTask.id !== draggedTask.id) {
      await api.handleMoveTask(draggedTask, targetGoalId, nextTask.id);
    } else {
      // 追加到末尾
      await api.handleMoveTask(draggedTask, targetGoalId, null);
    }
  }
}

function clearTaskDrag() {
  draggingTaskId.value = null;
  dragOverTaskId.value = null;
  dragOverGoalId.value = null;
}

// ===== 目标拖拽 =====

/** 目标头部：开始拖拽 */
function onGoalDragStart(e: DragEvent, goal: Goal) {
  // 阻止从按钮区域启动拖拽
  const target = e.target as HTMLElement;
  if (target.closest(".n-button") || target.closest("button")) {
    e.preventDefault();
    return;
  }
  if (!e.dataTransfer) return;
  e.dataTransfer.setData(GOAL_DRAG_MIME, goal.id);
  e.dataTransfer.setData("text/plain", goal.id);
  e.dataTransfer.effectAllowed = "move";
  draggingGoal.value = goal;
}

/** 目标头部：结束拖拽 */
function onGoalDragEnd() {
  draggingGoal.value = null;
  dragOverGoalId.value = null;
}

// ===== 目标卡片：统一放置区（任务 + 目标） =====

/** 目标卡片：拖拽悬停 */
function onCardDragOver(e: DragEvent) {
  // 非本应用拖拽（如外部文件）：不阻止默认行为，显示禁止光标
  if (!draggingTaskId.value && !draggingGoal.value) return;

  // 阻止冒泡到父级放置区，确保只有最内层目标节点响应
  e.stopPropagation();

  // 任务拖拽悬停
  if (draggingTaskId.value) {
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = "move";
    // 自动展开目标
    if (!expandedNodes.value.has(props.node.goal.id)) {
      expandedNodes.value.add(props.node.goal.id);
    }
    dragOverGoalId.value = props.node.goal.id;
    dropPosition.value = "inside";
    return;
  }

  // 目标拖拽悬停
  if (draggingGoal.value) {
    const dragged = draggingGoal.value;
    // 不能拖到自身或后代上
    if (
      dragged.id === props.node.goal.id ||
      isDescendantOf(props.node.goal, dragged)
    ) {
      if (e.dataTransfer) e.dataTransfer.dropEffect = "none";
      return;
    }
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = "move";
    dragOverGoalId.value = props.node.goal.id;
    // 三区检测：上 25%=before（插入上方），中 50%=inside（放入内部），下 25%=after（插入下方）
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const y = e.clientY - rect.top;
    if (y < rect.height * 0.25) {
      dropPosition.value = "before";
    } else if (y > rect.height * 0.75) {
      dropPosition.value = "after";
    } else {
      dropPosition.value = "inside";
    }
  }
}

/** 目标卡片：拖拽离开 */
function onCardDragLeave(e: DragEvent) {
  if (dragOverGoalId.value !== props.node.goal.id) return;
  const rt = e.relatedTarget as Node | null;
  const current = e.currentTarget as HTMLElement;
  if (current && rt && current.contains(rt)) return;
  if (dragOverGoalId.value === props.node.goal.id) {
    dragOverGoalId.value = null;
  }
}

/** 目标卡片：释放 */
async function onCardDrop(e: DragEvent) {
  // 非本应用拖拽：不处理
  if (!draggingTaskId.value && !draggingGoal.value) return;

  e.preventDefault();
  e.stopPropagation();

  // ===== 任务释放到目标卡片 =====
  if (draggingTaskId.value) {
    const taskId = draggingTaskId.value;
    const targetGoalId = props.node.goal.id;
    const firstTask = props.node.tasks[0]; // 放置到最前面
    clearTaskDrag();
    const task = findTaskInTree(taskId);
    if (!task || task.goal_id === targetGoalId) return;
    // 1b: 放置到该目标直属任务列表最前面
    await api.handleMoveTask(task, targetGoalId, firstTask?.id ?? null);
    return;
  }

  // ===== 目标释放到目标卡片 =====
  if (draggingGoal.value) {
    const dragged = draggingGoal.value;
    const target = props.node.goal;
    clearGoalDrag();
    if (dragged.id === target.id) return;
    if (isDescendantOf(target, dragged)) return; // 环检测

    const pos = dropPosition.value;
    if (pos === "before") {
      // 同级上方 → 插入到该目标之前（成为其同级）
      await api.handleMoveGoal(dragged.id, target.parent_id, target.id);
    } else if (pos === "after") {
      // 同级下方 → 插入到该目标之后（成为其同级）
      const nextSiblingId = api.findNextSiblingId(target.id);
      if (nextSiblingId && nextSiblingId !== dragged.id) {
        // 有下一个同级：插入到其前
        await api.handleMoveGoal(dragged.id, target.parent_id, nextSiblingId);
      } else {
        // 无下一个同级或下一个是自身：追加到末尾
        await api.handleMoveGoal(dragged.id, target.parent_id, null);
      }
    } else {
      // inside
      if (isAncestorOf(target, dragged)) {
        // 拖到上级目标上 → 变为与该上级同级
        await api.handleMoveGoal(dragged.id, target.parent_id, null);
      } else {
        // 拖到同级目标上 → 变为该目标的子目标
        await api.handleMoveGoal(dragged.id, target.id, null);
      }
    }
  }
}

function clearGoalDrag() {
  draggingGoal.value = null;
  dragOverGoalId.value = null;
}

// ===== 视觉反馈计算 =====

const isTaskDragOverInside = computed(
  () =>
    draggingTaskId.value &&
    dragOverGoalId.value === props.node.goal.id &&
    !dragOverTaskId.value,
);

const isGoalDragOverBefore = computed(
  () =>
    draggingGoal.value &&
    dragOverGoalId.value === props.node.goal.id &&
    dropPosition.value === "before",
);

const isGoalDragOverAfter = computed(
  () =>
    draggingGoal.value &&
    dragOverGoalId.value === props.node.goal.id &&
    dropPosition.value === "after",
);

const isGoalDragOverInside = computed(
  () =>
    draggingGoal.value &&
    dragOverGoalId.value === props.node.goal.id &&
    dropPosition.value === "inside",
);

function isTaskDropBefore(task: Task): boolean {
  return (
    draggingTaskId.value !== null &&
    dragOverTaskId.value === task.id &&
    taskDropPosition.value === "before"
  );
}

function isTaskDropAfter(task: Task): boolean {
  return (
    draggingTaskId.value !== null &&
    dragOverTaskId.value === task.id &&
    taskDropPosition.value === "after"
  );
}

const isDraggingThisGoal = computed(
  () => draggingGoal.value?.id === props.node.goal.id,
);

// ===== 树内查找 =====

function findTaskInTree(taskId: string): Task | null {
  for (const t of props.node.tasks) {
    if (t.id === taskId) return t;
  }
  for (const child of props.node.sub_goals) {
    const found = findTaskInNode(child, taskId);
    if (found) return found;
  }
  return null;
}

function findTaskInNode(node: GoalTreeNode, taskId: string): Task | null {
  for (const t of node.tasks) {
    if (t.id === taskId) return t;
  }
  for (const child of node.sub_goals) {
    const found = findTaskInNode(child, taskId);
    if (found) return found;
  }
  return null;
}
</script>

<template>
  <div
    :class="[
      level > 0 ? 'ml-3 pl-3 border-l border-gray-200' : '',
      isDraggingThisGoal ? 'dragging-opacity' : '',
      isTaskDragOverInside ? 'drop-inside-task' : '',
      isGoalDragOverBefore ? 'drop-before-goal' : '',
      isGoalDragOverAfter ? 'drop-after-goal' : '',
      isGoalDragOverInside ? 'drop-inside-goal' : '',
    ]"
    @dragover="onCardDragOver"
    @dragenter.prevent
    @dragleave="onCardDragLeave"
    @drop="onCardDrop"
  >
    <NCard size="small" :class="level > 0 ? 'mt-1.5' : ''">
      <!-- 节点头部（可拖拽目标） -->
      <div
        class="flex items-center cursor-pointer justify-between"
        :class="isRoot ? 'gap-3' : 'gap-2'"
        draggable="true"
        @dragstart="onGoalDragStart($event, node.goal)"
        @dragend="onGoalDragEnd"
        @click="toggle"
      >
        <Icon
          :icon="isExpanded ? 'mdi:chevron-down' : 'mdi:chevron-right'"
          :width="isRoot ? 20 : 16"
          class="text-gray-400 flex-shrink-0"
        />
        <Icon
          :icon="goalIcon"
          :width="isRoot ? 20 : 16"
          :class="
            node.is_completed
              ? 'text-green-500'
              : isRoot
                ? 'text-brand-500'
                : 'text-blue-500'
          "
          class="flex-shrink-0"
        />
        <div class="flex-1 min-w-0">
          <div
            class="truncate"
            :class="[
              isRoot ? 'font-medium' : 'text-sm font-medium',
              { 'line-through text-gray-400': node.is_completed },
            ]"
          >
            {{ node.goal.name }}
          </div>
          <div
            class="text-xs text-gray-500 flex items-center gap-3 mt-0.5 flex-wrap"
          >
            <span :class="{ 'text-red-500': isOverdue }">
              {{ getDaysLeft(node.goal.deadline) }}
            </span>
            <span v-if="node.goal.total_qty > 0">
              总量：{{ node.goal.total_qty }}{{ node.goal.unit }}
            </span>
            <span>进度：{{ Math.round(node.progress * 100) }}%</span>
            <NProgress
              v-if="!isRoot"
              type="line"
              :percentage="Math.round(node.progress * 100)"
              :show-indicator="false"
              :height="4"
              style="width: 80px"
            />
            <NTag
              v-if="node.is_completed"
              size="tiny"
              type="success"
              :bordered="false"
            >
              {{ isRoot ? "已完成" : "完成" }}
            </NTag>
          </div>
        </div>
        <div
          class="flex items-center justify-end flex-shrink-0 flex-wrap gap-1"
          @click.stop
        >
          <!-- 添加子目标（所有层级均可嵌套） -->
          <NButton
            size="tiny"
            quaternary
            type="info"
            @click="api.openCreateGoalModal(node.goal.id)"
          >
            <template #icon><Icon icon="mdi:folder-plus-outline" /></template>
            子目标
          </NButton>
          <!-- 视频拆解 -->
          <NButton
            size="tiny"
            type="primary"
            ghost
            :disabled="!node.goal.deadline || node.goal.total_qty <= 0"
            @click="api.handleAutoSplit(node.goal)"
          >
            <template #icon><Icon icon="mdi:auto-fix" /></template>
            视频拆解
          </NButton>
          <!-- 重新规划 -->
          <NButton
            size="tiny"
            type="warning"
            ghost
            :disabled="!node.goal.deadline"
            @click="api.handleReplanPreview(node.goal)"
          >
            <template #icon><Icon icon="mdi:refresh-circle" /></template>
            重新规划
          </NButton>
          <!-- 任务 -->
          <NButton
            size="tiny"
            type="success"
            ghost
            @click="api.openCreateTaskModal(node.goal.id)"
          >
            <template #icon><Icon icon="mdi:plus-box-outline" /></template>
            任务
          </NButton>
          <!-- 编辑 -->
          <NButton
            size="tiny"
            quaternary
            @click="api.openEditGoalModal(node.goal)"
          >
            <Icon icon="mdi:pencil-outline" />
          </NButton>
          <!-- 删除（级联删除所有后代） -->
          <NPopconfirm @positive-click="api.handleDeleteGoal(node.goal)">
            <template #trigger>
              <NButton size="tiny" quaternary type="error">
                <Icon icon="mdi:delete" />
              </NButton>
            </template>
            确定删除目标"{{ node.goal.name }}"？所有子目标和任务将一并删除。
          </NPopconfirm>
        </div>
      </div>

      <!-- 展开内容：递归子目标 + 直属任务 -->
      <div v-if="isExpanded" class="mt-2 space-y-2">
        <!-- 递归子目标（支持任意层级嵌套） -->
        <GoalTreeNodeItem
          v-for="child in node.sub_goals"
          :key="child.goal.id"
          :node="child"
          :level="level + 1"
        />

        <!-- 直属任务 -->
        <div v-if="node.tasks.length > 0" class="space-y-0.5">
          <div v-if="hasChildren" class="text-xs text-gray-400 px-3 py-1">
            直属任务
          </div>
          <div
            v-for="(task, idx) in node.tasks"
            :key="task.id"
            draggable="true"
            :class="[
              'flex items-center gap-2 px-3 py-1.5 rounded text-sm cursor-grab',
              'hover:bg-gray-50',
              'task-row',
              draggingTaskId === task.id ? 'dragging-opacity' : '',
              isTaskDropBefore(task) ? 'task-drop-before' : '',
              isTaskDropAfter(task) ? 'task-drop-after' : '',
            ]"
            :title="`拖拽以移动或排序（当前归属：${node.goal.name}）`"
            @dragstart="onTaskDragStart($event, task)"
            @dragend="onTaskDragEnd"
            @dragover="onTaskRowDragOver($event, task)"
            @dragleave="onTaskRowDragLeave($event, task)"
            @drop="onTaskRowDrop($event, task, idx)"
            @click.stop
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
              :options="api.buildTaskActions(task)"
              @select="(key: string) => onTaskSelect(key, task)"
            >
              <NButton size="tiny" quaternary>
                <Icon icon="mdi:dots-horizontal" width="16" />
              </NButton>
            </NDropdown>
          </div>
        </div>

        <!-- 空状态 -->
        <div
          v-if="node.sub_goals.length === 0 && node.tasks.length === 0"
          class="text-sm text-gray-400 py-2"
        >
          暂无子目标和任务，点击上方按钮开始添加
        </div>
      </div>
    </NCard>
  </div>
</template>

<style scoped>
/* 被拖拽的目标：半透明 */
.dragging-opacity {
  opacity: 0.4;
}

/* 任务拖拽悬停在目标卡片上（放入内部） */
.drop-inside-task {
  outline: 2px dashed #18a058;
  outline-offset: -2px;
  background-color: rgba(24, 160, 88, 0.04);
  transition:
    outline 0.1s ease,
    background-color 0.1s ease;
}

/* 目标拖拽悬停在上方（同级插入） */
.drop-before-goal {
  box-shadow: 0 -3px 0 0 #2080f0;
  transition: box-shadow 0.1s ease;
}

/* 目标拖拽悬停在下方（同级插入） */
.drop-after-goal {
  box-shadow: 0 3px 0 0 #2080f0;
  transition: box-shadow 0.1s ease;
}

/* 目标拖拽悬停在内部（放入为子目标） */
.drop-inside-goal {
  outline: 2px dashed #2080f0;
  outline-offset: -2px;
  background-color: rgba(32, 128, 240, 0.04);
  transition:
    outline 0.1s ease,
    background-color 0.1s ease;
}

/* 任务行拖拽悬停在上方（插入之前） */
.task-drop-before {
  box-shadow: 0 -2px 0 0 #18a058;
  transition: box-shadow 0.1s ease;
}

/* 任务行拖拽悬停在下方（插入之后） */
.task-drop-after {
  box-shadow: 0 2px 0 0 #18a058;
  transition: box-shadow 0.1s ease;
}

/* 任务行拖拽手柄视觉 */
[draggable="true"] {
  user-select: none;
}

[draggable="true"]:active {
  cursor: grabbing;
}
</style>
