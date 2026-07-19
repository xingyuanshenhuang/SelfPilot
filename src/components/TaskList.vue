<script setup lang="ts">
import { computed, inject } from "vue";
import { NVirtualList, NTag, NDropdown, NButton } from "naive-ui";
import { Icon } from "@iconify/vue";
import { STATUS_META, type Task } from "@/types";
import {
  goalTreeApiKey,
  type GoalTreeApi,
  TASK_DRAG_MIME,
} from "@/components/GoalTreeNodeItem.vue";

/**
 * 任务列表虚拟滚动组件。
 * 任务数 > VIRTUAL_THRESHOLD 时启用 naive-ui NVirtualList，
 * 否则保持原 v-for 渲染（拖拽体验最佳）。
 * 两种模式共用同一份行内 DOM 与 rowClass，保证视觉一致。
 */

const VIRTUAL_THRESHOLD = 50;
/** 固定行高，需与 NVirtualList 的 item-size 严格对齐，避免 FinweckTree 漂移 */
const ROW_HEIGHT = 36;

const props = defineProps<{
  tasks: Task[];
  goalId: string;
  goalName: string;
}>();

const api = inject<GoalTreeApi>(goalTreeApiKey)!;

// 共享拖拽状态（来自 provide）
const draggingTaskId = api.draggingTaskId;
const dragOverTaskId = api.dragOverTaskId;
const taskDropPosition = api.taskDropPosition;
const dragOverGoalId = api.dragOverGoalId;

const useVirtual = computed(() => props.tasks.length > VIRTUAL_THRESHOLD);

// ===== 行类名（两种模式共用，保证视觉一致） =====
function rowClass(task: Task): string[] {
  return [
    "flex items-center gap-2 px-3 py-1.5 rounded text-sm cursor-grab h-9",
    "hover:bg-gray-50",
    "task-row",
    draggingTaskId.value === task.id ? "dragging-opacity" : "",
    isTaskDropBefore(task) ? "task-drop-before" : "",
    isTaskDropAfter(task) ? "task-drop-after" : "",
  ];
}

// ===== 任务拖拽（迁移自 GoalTreeNodeItem） =====

function onTaskDragStart(e: DragEvent, task: Task) {
  if (!e.dataTransfer) return;
  e.dataTransfer.setData(TASK_DRAG_MIME, task.id);
  e.dataTransfer.setData("text/plain", task.id);
  e.dataTransfer.effectAllowed = "move";
  draggingTaskId.value = task.id;
}

function onTaskDragEnd() {
  draggingTaskId.value = null;
  dragOverTaskId.value = null;
  dragOverGoalId.value = null;
}

function onTaskRowDragOver(e: DragEvent, task: Task) {
  if (!draggingTaskId.value) return;
  e.preventDefault();
  e.stopPropagation(); // 阻止冒泡到目标卡片
  if (e.dataTransfer) e.dataTransfer.dropEffect = "move";
  dragOverTaskId.value = task.id;
  const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
  taskDropPosition.value =
    e.clientY - rect.top < rect.height / 2 ? "before" : "after";
}

function onTaskRowDragLeave(e: DragEvent, task: Task) {
  if (dragOverTaskId.value !== task.id) return;
  const rt = e.relatedTarget as Node | null;
  const current = e.currentTarget as HTMLElement;
  if (current && rt && current.contains(rt)) return;
  if (dragOverTaskId.value === task.id) {
    dragOverTaskId.value = null;
  }
}

async function onTaskRowDrop(e: DragEvent, task: Task, index: number) {
  if (!draggingTaskId.value) return;
  e.preventDefault();
  e.stopPropagation();
  const taskId = draggingTaskId.value;
  const targetGoalId = props.goalId;
  api.clearTaskDrag();

  const draggedTask = api.findTaskInTree(taskId);
  if (!draggedTask || draggedTask.id === task.id) return;

  const pos = taskDropPosition.value;
  if (pos === "before") {
    await api.handleMoveTask(draggedTask, targetGoalId, task.id);
  } else {
    // 插入到当前任务之后：找下一个同级任务，插入其前；若为最后一个则追加末尾
    const nextTask = props.tasks[index + 1];
    if (nextTask && nextTask.id !== draggedTask.id) {
      await api.handleMoveTask(draggedTask, targetGoalId, nextTask.id);
    } else {
      await api.handleMoveTask(draggedTask, targetGoalId, null);
    }
  }
}

function onTaskSelect(key: string, task: Task) {
  api.handleTaskAction(key, task);
}

// ===== 视觉反馈 =====

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
</script>

<template>
  <div class="space-y-0.5">
    <slot name="label" />

    <!-- 非虚拟模式：保持现有 v-for 体验，拖拽最顺滑 -->
    <template v-if="!useVirtual">
      <div
        v-for="(task, idx) in tasks"
        :key="task.id"
        draggable="true"
        :class="rowClass(task)"
        :title="`拖拽以移动或排序（当前归属：${goalName}）`"
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
    </template>

    <!-- 虚拟模式：NVirtualList 仅渲染可见行（行内 DOM 与上方完全一致） -->
    <!-- 注：NVirtualList 内部 .v-vl 用 height:100% + max-height:inherit，
         外层必须显式高度（不能用 max-height + overflow:hidden，否则 height:100% 解析为 0） -->
    <div v-else style="height: 400px">
      <NVirtualList
        :items="tasks"
        :item-size="ROW_HEIGHT"
        key-field="id"
        :item-resizable="false"
      >
        <template #default="{ item, index }">
          <div
            :key="item.id"
            draggable="true"
            :class="rowClass(item)"
            :title="`拖拽以移动或排序（当前归属：${goalName}）`"
            @dragstart="onTaskDragStart($event, item)"
            @dragend="onTaskDragEnd"
            @dragover="onTaskRowDragOver($event, item)"
            @dragleave="onTaskRowDragLeave($event, item)"
            @drop="onTaskRowDrop($event, item, index)"
            @click.stop
          >
            <Icon
              :icon="STATUS_META[(item as Task).status].icon"
              :color="STATUS_META[(item as Task).status].color"
              width="16"
            />
            <span
              class="flex-1 truncate"
              :class="{ 'line-through text-gray-400': item.status === 'done' }"
            >
              {{ item.name }}
            </span>
            <span class="text-xs text-gray-500">{{ item.plan_date }}</span>
            <NTag
              size="tiny"
              :bordered="false"
              :type="item.source === 'auto' ? 'info' : 'warning'"
            >
              {{ item.source === "auto" ? "自动" : "手动" }}
            </NTag>
            <span class="text-xs text-gray-500">
              {{ item.actual_qty }}/{{ item.plan_qty }}{{ item.unit }}
            </span>
            <NDropdown
              trigger="click"
              :options="api.buildTaskActions(item)"
              @select="(key: string) => onTaskSelect(key, item)"
            >
              <NButton size="tiny" quaternary>
                <Icon icon="mdi:dots-horizontal" width="16" />
              </NButton>
            </NDropdown>
          </div>
        </template>
      </NVirtualList>
    </div>
  </div>
</template>

<style scoped>
/* 被拖拽的任务：半透明 */
.dragging-opacity {
  opacity: 0.4;
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
