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
  /** 拖拽归属：将任务移动到指定目标下 */
  handleMoveTask: (task: Task, targetGoalId: string) => Promise<void>;
  /** 当前正在拖拽的任务 ID（共享响应式状态，用于全树视觉反馈） */
  draggingTaskId: Ref<string | null>;
  /** 当前拖拽悬停的目标 goal ID（用于高亮放置区域） */
  dragOverGoalId: Ref<string | null>;
}

/** provide/inject 键 */
export const goalTreeApiKey = "goalTreeApi";

/** 拖拽任务时通过 dataTransfer 传递的 MIME 类型 */
export const TASK_DRAG_MIME = "application/x-selfpilot-task-id";
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

// ===== 拖拽归属 =====
const draggingTaskId = api.draggingTaskId;
const dragOverGoalId = api.dragOverGoalId;

const isDragOver = computed(
  () =>
    dragOverGoalId.value === props.node.goal.id &&
    draggingTaskId.value !== null,
);

/** 任务行：开始拖拽 */
function onTaskDragStart(e: DragEvent, task: Task) {
  if (!e.dataTransfer) return;
  e.dataTransfer.setData(TASK_DRAG_MIME, task.id);
  e.dataTransfer.setData("text/plain", task.id);
  e.dataTransfer.effectAllowed = "move";
  draggingTaskId.value = task.id;
}

/** 任务行：结束拖拽（无论是否成功放置） */
function onTaskDragEnd() {
  draggingTaskId.value = null;
  dragOverGoalId.value = null;
}

/** 目标卡片：进入放置区 */
function onGoalDragOver(e: DragEvent) {
  if (!draggingTaskId.value) return;
  // 仅允许 move 操作
  if (e.dataTransfer) e.dataTransfer.dropEffect = "move";
  e.preventDefault();
  dragOverGoalId.value = props.node.goal.id;
}

/** 目标卡片：离开放置区 */
function onGoalDragLeave(e: DragEvent) {
  // relatedTarget 仍在本节点内时不清除
  if (dragOverGoalId.value !== props.node.goal.id) return;
  const rt = e.relatedTarget as Node | null;
  const current = (e.currentTarget as HTMLElement)?.parentElement;
  if (current && rt && current.contains(rt)) return;
  if (dragOverGoalId.value === props.node.goal.id) {
    dragOverGoalId.value = null;
  }
}

/** 目标卡片：释放 */
async function onGoalDrop(e: DragEvent) {
  e.preventDefault();
  const taskId = draggingTaskId.value;
  const targetGoalId = props.node.goal.id;
  draggingTaskId.value = null;
  dragOverGoalId.value = null;
  if (!taskId) return;

  // 找到被拖拽的任务对象
  const task = findTaskInTree(taskId);
  if (!task) return;
  if (task.goal_id === targetGoalId) return; // 同目标忽略
  await api.handleMoveTask(task, targetGoalId);
}

/** 在整棵目标树中查找指定 ID 的任务（递归遍历本节点子树） */
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
  <div :class="level > 0 ? 'ml-3 pl-3 border-l border-gray-200' : ''">
    <NCard
      size="small"
      :class="[level > 0 ? 'mt-1.5' : '', isDragOver ? 'drag-over' : '']"
      @dragover="onGoalDragOver"
      @dragleave="onGoalDragLeave"
      @drop="onGoalDrop"
    >
      <!-- 节点头部 -->
      <div
        class="flex items-center cursor-pointer justify-between"
        :class="isRoot ? 'gap-3' : 'gap-2'"
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
            v-for="task in node.tasks"
            :key="task.id"
            draggable="true"
            :class="[
              'flex items-center gap-2 px-3 py-1.5 rounded text-sm cursor-grab',
              'hover:bg-gray-50',
              draggingTaskId === task.id ? 'dragging opacity-50' : '',
            ]"
            :title="`拖拽以移动到其他目标（当前归属：${node.goal.name}）`"
            @dragstart="onTaskDragStart($event, task)"
            @dragend="onTaskDragEnd"
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
/* 拖拽悬停时的目标卡片高亮 */
:deep(.drag-over) {
  outline: 2px dashed #18a058;
  outline-offset: -2px;
  background-color: rgba(24, 160, 88, 0.04);
  transition:
    outline 0.1s ease,
    background-color 0.1s ease;
}

/* 被拖拽的任务行：半透明 + 占位虚线 */
:deep(.dragging) {
  opacity: 0.5;
  background: repeating-linear-gradient(
    45deg,
    transparent,
    transparent 4px,
    rgba(24, 160, 88, 0.1) 4px,
    rgba(24, 160, 88, 0.1) 8px
  );
}

/* 任务行拖拽手柄视觉 */
[draggable="true"] {
  user-select: none;
  -webkit-user-drag: element;
}

[draggable="true"]:active {
  cursor: grabbing;
}
</style>
