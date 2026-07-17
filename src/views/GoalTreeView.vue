<script setup lang="ts">
import { onMounted, ref, reactive, computed, h, provide } from "vue";
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
  NCheckboxGroup,
  NRadioGroup,
  NRadio,
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
  GoalTreeNode,
  Task,
  ReplanPreview,
  CreateTaskInput,
  UpdateTaskInput,
  CreateGoalInput,
  UpdateGoalInput,
  RepeatSplitInput,
  SmartSplitInput,
  SplitStrategy,
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
  /** P1-3：每日可用时长（按时间预算拆解时使用，null=未设置） */
  daily_capacity: null as number | null,
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
  // 重复任务频率：daily | weekly | monthly
  frequency: "daily" as "daily" | "weekly" | "monthly",
  // 周几（0=周日, 1-6=周一至周六），仅 weekly 有效
  weekdays: [] as number[],
  // 每月几号（1-31），仅 monthly 有效
  month_days: [] as number[],
  // P1-1：前置任务 ID 列表（同目标下其他任务，编辑模式可设置）
  dependency_ids: [] as string[],
});
/** 编辑任务时记录其原始依赖，保存时对比计算增删 */
const originalDependencyIds = ref<string[]>([]);

// 重复任务频率选项
const frequencyOptions = [
  { label: "每天", value: "daily" },
  { label: "每周", value: "weekly" },
  { label: "每月", value: "monthly" },
];

// 周几选项（0=周日, 1-6=周一至周六）
const weekdayOptions = [
  { label: "日", value: 0 },
  { label: "一", value: 1 },
  { label: "二", value: 2 },
  { label: "三", value: 3 },
  { label: "四", value: 4 },
  { label: "五", value: 5 },
  { label: "六", value: 6 },
];

// 每月几号选项（1-31）
const monthDayOptions = Array.from({ length: 31 }, (_, i) => ({
  label: `${i + 1}`,
  value: i + 1,
}));

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
  goalForm.daily_capacity = null;
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
  goalForm.daily_capacity = goal.daily_capacity;
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
        daily_capacity: goalForm.daily_capacity,
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
        daily_capacity: goalForm.daily_capacity,
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

// ===== 目标模板（P1-2 非线性阶段拆解配套）=====

interface GoalTemplateStage {
  name: string;
  unit: string;
  qty_placeholder: number;
}

interface GoalTemplate {
  id: string;
  name: string;
  description: string;
  icon: string;
  stages: GoalTemplateStage[];
}

const goalTemplates: GoalTemplate[] = [
  {
    id: "study_video_practice",
    name: "学习：视频 + 练习",
    description: "前阶段看视频，后阶段做题练习，各自独立节奏",
    icon: "mdi:school-outline",
    stages: [
      { name: "视频学习", unit: "小时", qty_placeholder: 40 },
      { name: "练习题", unit: "道", qty_placeholder: 100 },
    ],
  },
  {
    id: "reading_notes",
    name: "阅读：通读 + 笔记",
    description: "前阶段通读全书，后阶段整理笔记",
    icon: "mdi:book-open-page-variant-outline",
    stages: [
      { name: "通读", unit: "页", qty_placeholder: 300 },
      { name: "笔记", unit: "篇", qty_placeholder: 10 },
    ],
  },
  {
    id: "exam_prep",
    name: "备考：学习 + 练习 + 模考",
    description: "三阶段备考：知识学习、刷题巩固、模拟考试",
    icon: "mdi:file-document-edit-outline",
    stages: [
      { name: "知识学习", unit: "章", qty_placeholder: 8 },
      { name: "练习", unit: "道", qty_placeholder: 200 },
      { name: "模考", unit: "套", qty_placeholder: 5 },
    ],
  },
  {
    id: "vocab_review",
    name: "背词：新词 + 复习",
    description: "前阶段背新词，后阶段滚动复习",
    icon: "mdi:translate",
    stages: [
      { name: "新词", unit: "词", qty_placeholder: 1000 },
      { name: "复习", unit: "次", qty_placeholder: 20 },
    ],
  },
];

const showTemplateModal = ref(false);
const selectedTemplateId = ref<string>("");
const templateForm = reactive({
  parent_name: "",
  parent_deadline: null as number | null,
  stages: [] as Array<{
    name: string;
    unit: string;
    total_qty: number;
    deadline: null | number;
  }>,
});

const selectedTemplate = computed(
  () => goalTemplates.find((t) => t.id === selectedTemplateId.value) ?? null,
);

function openTemplateModal() {
  selectedTemplateId.value = goalTemplates[0].id;
  applyTemplate(goalTemplates[0]);
  templateForm.parent_name = "";
  templateForm.parent_deadline = null;
  showTemplateModal.value = true;
}

function applyTemplate(t: GoalTemplate) {
  templateForm.stages = t.stages.map((s) => ({
    name: s.name,
    unit: s.unit,
    total_qty: s.qty_placeholder,
    deadline: null,
  }));
}

function onTemplateChange(id: string) {
  const t = goalTemplates.find((t) => t.id === id);
  if (t) applyTemplate(t);
}

async function handleSaveTemplate() {
  if (!templateForm.parent_name.trim()) {
    message.warning("请输入总目标名称");
    return;
  }
  for (const s of templateForm.stages) {
    if (!s.name.trim()) {
      message.warning("阶段名称不能为空");
      return;
    }
    if (s.total_qty <= 0) {
      message.warning(`阶段"${s.name}"的总量必须大于 0`);
      return;
    }
  }

  const parentDeadline = templateForm.parent_deadline
    ? format(new Date(templateForm.parent_deadline), "yyyy-MM-dd")
    : null;

  try {
    // 1. 创建总目标
    const parentGoal = await goalStore.createGoal({
      name: templateForm.parent_name,
      parent_id: null,
      deadline: parentDeadline,
      total_qty: 0,
      unit: "",
    });

    // 2. 依次创建各阶段子目标
    for (const s of templateForm.stages) {
      const stageDeadline = s.deadline
        ? format(new Date(s.deadline), "yyyy-MM-dd")
        : parentDeadline;
      await goalStore.createGoal({
        name: s.name,
        parent_id: parentGoal.id,
        deadline: stageDeadline,
        total_qty: s.total_qty,
        unit: s.unit,
      });
    }

    expandedNodes.value.add(parentGoal.id);
    message.success(
      `已创建总目标"${templateForm.parent_name}"及 ${templateForm.stages.length} 个阶段子目标`,
    );
    showTemplateModal.value = false;
    await goalStore.fetchGoalTree();
    await goalStore.fetchProgresses();
  } catch (e) {
    message.error(String(e));
  }
}

// ===== 自动拆解（整合：按截止日期均分 / 按时间预算 / 自定义日期范围）=====

/** 拆解策略选项 */
const splitStrategyOptions = [
  {
    label: "按截止日期均分",
    value: "by_deadline" as SplitStrategy,
    description: "总量 ÷ 截止日期剩余天数，每天一个任务（余数分前几天）",
  },
  {
    label: "按时间预算",
    value: "by_capacity" as SplitStrategy,
    description: "按每日可用时长拆解，任务数 = ⌈总量 ÷ 每日时长⌉",
  },
  {
    label: "自定义日期范围",
    value: "by_date_range" as SplitStrategy,
    description: "指定起止日期，可选每日数量；不填则按天数均分总量",
  },
];

const showSplitModal = ref(false);
const splitForm = reactive({
  goal_id: "",
  goal_name: "",
  strategy: "by_deadline" as SplitStrategy,
  total_qty: 0,
  unit: "个",
  // by_deadline / by_capacity 用
  deadline: null as number | null,
  // by_capacity 用
  daily_capacity: null as number | null,
  // by_date_range 用
  start_date: null as number | null,
  end_date: null as number | null,
  per_day_qty: null as number | null,
});

/** 当前选中策略的描述文案 */
const currentStrategyDesc = computed(
  () =>
    splitStrategyOptions.find((s) => s.value === splitForm.strategy)
      ?.description ?? "",
);

/** 拆解预览：根据当前参数计算预计任务数、每日计划量、跨越天数 */
const splitPreview = computed(() => {
  const total = splitForm.total_qty;
  if (total <= 0) return null;

  if (splitForm.strategy === "by_deadline") {
    if (!splitForm.deadline) return null;
    const days = differenceInCalendarDays(
      new Date(splitForm.deadline),
      new Date(),
    );
    if (days < 1)
      return {
        taskCount: 0,
        perDay: 0,
        spanDays: 0,
        error: "截止日期剩余天数不足",
      };
    const base = Math.floor(total / days);
    const remainder = Math.round(total - base * days);
    return {
      taskCount: days,
      perDay: base,
      spanDays: days,
      detail:
        remainder > 0
          ? `前 ${remainder} 天每天 ${base + 1}，其余每天 ${base}`
          : `每天 ${base}`,
    };
  }

  if (splitForm.strategy === "by_capacity") {
    const cap = splitForm.daily_capacity;
    if (!cap || cap <= 0) return null;
    const num = Math.ceil(total / cap);
    if (!splitForm.deadline)
      return { taskCount: num, perDay: cap, spanDays: num };
    const days = differenceInCalendarDays(
      new Date(splitForm.deadline),
      new Date(),
    );
    if (num > days) {
      return {
        taskCount: num,
        perDay: cap,
        spanDays: num,
        error: `需要 ${num} 天，但截止日期前仅剩 ${days} 天`,
      };
    }
    return { taskCount: num, perDay: cap, spanDays: num };
  }

  // by_date_range
  if (!splitForm.start_date || !splitForm.end_date) return null;
  const span =
    differenceInCalendarDays(
      new Date(splitForm.end_date),
      new Date(splitForm.start_date),
    ) + 1;
  if (span < 1)
    return {
      taskCount: 0,
      perDay: 0,
      spanDays: 0,
      error: "结束日期不能早于起始日期",
    };
  const perDay = splitForm.per_day_qty;
  if (perDay && perDay > 0) {
    const num = Math.ceil(total / perDay);
    if (num > span) {
      return {
        taskCount: num,
        perDay: perDay,
        spanDays: span,
        error: `需要 ${num} 个任务，但日期范围仅 ${span} 天`,
      };
    }
    return { taskCount: num, perDay: perDay, spanDays: num };
  }
  const base = Math.floor(total / span);
  const remainder = Math.round(total - base * span);
  return {
    taskCount: span,
    perDay: base,
    spanDays: span,
    detail:
      remainder > 0
        ? `前 ${remainder} 天每天 ${base + 1}，其余每天 ${base}`
        : `每天 ${base}`,
  };
});

function openSplitModal(goal: Goal) {
  splitForm.goal_id = goal.id;
  splitForm.goal_name = goal.name;
  splitForm.strategy = "by_deadline";
  splitForm.total_qty = goal.total_qty;
  splitForm.unit = goal.unit || "个";
  splitForm.deadline = goal.deadline ? parseISO(goal.deadline).getTime() : null;
  splitForm.daily_capacity = goal.daily_capacity;
  // 默认起始日期=明天，结束日期=截止日期（若有）
  const tomorrow = Date.now() + 86400000;
  splitForm.start_date = tomorrow;
  splitForm.end_date = goal.deadline
    ? parseISO(goal.deadline).getTime()
    : tomorrow + 86400000 * 6;
  splitForm.per_day_qty = null;
  showSplitModal.value = true;
}

async function handleSmartSplit() {
  if (splitForm.total_qty <= 0) {
    message.warning("总量必须大于 0");
    return;
  }

  // 构造入参（仅传有意义的字段，null/undefined 不传以使用目标默认值）
  const input: SmartSplitInput = {
    goal_id: splitForm.goal_id,
    strategy: splitForm.strategy,
    total_qty: splitForm.total_qty,
  };

  if (splitForm.strategy === "by_deadline") {
    if (!splitForm.deadline) {
      message.warning("请选择截止日期");
      return;
    }
    input.deadline = format(new Date(splitForm.deadline), "yyyy-MM-dd");
  } else if (splitForm.strategy === "by_capacity") {
    if (!splitForm.daily_capacity || splitForm.daily_capacity <= 0) {
      message.warning("请输入每日可用时长");
      return;
    }
    if (!splitForm.deadline) {
      message.warning("请选择截止日期");
      return;
    }
    input.daily_capacity = splitForm.daily_capacity;
    input.deadline = format(new Date(splitForm.deadline), "yyyy-MM-dd");
  } else if (splitForm.strategy === "by_date_range") {
    if (!splitForm.start_date || !splitForm.end_date) {
      message.warning("请选择起止日期");
      return;
    }
    input.start_date = format(new Date(splitForm.start_date), "yyyy-MM-dd");
    input.end_date = format(new Date(splitForm.end_date), "yyyy-MM-dd");
    if (splitForm.per_day_qty && splitForm.per_day_qty > 0) {
      input.per_day_qty = splitForm.per_day_qty;
    }
  }

  try {
    const tasks = await goalStore.smartSplit(input);
    message.success(`已拆解为 ${tasks.length} 个每日任务`);
    showSplitModal.value = false;
    await goalStore.fetchGoalTree();
    expandedNodes.value.add(splitForm.goal_id);
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
  taskForm.frequency = "daily";
  taskForm.weekdays = [];
  taskForm.month_days = [];
  taskForm.dependency_ids = [];
  originalDependencyIds.value = [];
  showTaskModal.value = true;
}

/** 前置任务可选项：同目标下其他任务（排除当前编辑任务自身） */
const dependencyOptions = computed(() => {
  const tasks = findTasksByGoalId(taskForm.goal_id);
  return tasks
    .filter((t) => t.id !== taskForm.task_id)
    .map((t) => ({
      label: t.name + (t.plan_date ? ` (${t.plan_date})` : ""),
      value: t.id,
    }));
});

async function handleSaveTask() {
  if (!taskForm.name.trim()) {
    message.warning("请输入任务名称");
    return;
  }
  try {
    if (taskModalMode.value === "create") {
      if (taskForm.is_repeat) {
        // 重复任务：按频率在日期范围内生成任务
        if (!taskForm.plan_date) {
          message.warning("请选择起始日期");
          return;
        }
        if (!taskForm.end_date) {
          message.warning("重复任务请选择结束日期");
          return;
        }
        if (taskForm.frequency === "weekly" && taskForm.weekdays.length === 0) {
          message.warning("请至少选择一个周几");
          return;
        }
        if (
          taskForm.frequency === "monthly" &&
          taskForm.month_days.length === 0
        ) {
          message.warning("请至少选择一个日期");
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
          frequency: taskForm.frequency,
          weekdays:
            taskForm.frequency === "weekly" ? taskForm.weekdays : undefined,
          month_days:
            taskForm.frequency === "monthly" ? taskForm.month_days : undefined,
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
        const created = await taskApi.createTask(input);
        message.success("任务创建成功");
        // P1-1：创建时若选择了前置依赖，立即同步
        if (taskForm.dependency_ids.length > 0) {
          taskForm.task_id = created.id;
          originalDependencyIds.value = [];
          const depOk = await syncDependencies();
          if (!depOk) {
            // 任务已创建但依赖未同步成功，保持弹窗打开供用户重试
            return;
          }
        }
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
      // 同步前置依赖（对比编辑前后的 dependency_ids）
      const depOk = await syncDependencies();
      if (!depOk) {
        // 依赖同步失败时保持弹窗打开，让用户有机会重试或取消
        return;
      }
      message.success("任务已更新");
    }
    showTaskModal.value = false;
    await goalStore.fetchGoalTree();
    await goalStore.fetchProgresses();
  } catch (e) {
    message.error(String(e));
  }
}

/** 同步任务前置依赖：新增勾选的、移除取消的（后端已防循环、防重复）
 * @returns true 表示全部成功，false 表示存在失败
 */
async function syncDependencies(): Promise<boolean> {
  const taskId = taskForm.task_id;
  const oldSet = new Set(originalDependencyIds.value);
  const newSet = new Set(taskForm.dependency_ids);
  const toAdd = taskForm.dependency_ids.filter((id) => !oldSet.has(id));
  const toRemove = originalDependencyIds.value.filter((id) => !newSet.has(id));
  const ops: Promise<unknown>[] = [];
  for (const depId of toAdd) {
    ops.push(
      taskApi.setTaskDependency({ task_id: taskId, depends_on_id: depId }),
    );
  }
  for (const depId of toRemove) {
    ops.push(taskApi.removeTaskDependency(taskId, depId));
  }
  if (ops.length === 0) {
    return true;
  }
  const results = await Promise.allSettled(ops);
  const failed = results.filter((r) => r.status === "rejected");
  if (failed.length > 0) {
    message.warning(
      `部分依赖更新失败（${failed.length} 个），可能存在循环依赖`,
    );
    return false;
  }
  return true;
}

async function openEditTaskModal(task: Task) {
  taskModalMode.value = "edit";
  taskForm.task_id = task.id;
  taskForm.goal_id = task.goal_id;
  taskForm.name = task.name;
  taskForm.plan_date = task.plan_date
    ? parseISO(task.plan_date).getTime()
    : null;
  taskForm.plan_qty = task.plan_qty;
  taskForm.unit = task.unit;
  taskForm.dependency_ids = [];
  originalDependencyIds.value = [];
  showTaskModal.value = true;

  // 加载现有前置依赖
  try {
    const deps = await taskApi.listTaskDependencies(task.id);
    const ids = deps.map((d) => d.id);
    taskForm.dependency_ids = ids;
    originalDependencyIds.value = [...ids];
  } catch (e) {
    message.error(String(e));
  }
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

/** 在已加载的目标树中递归查找某目标下的全部任务 */
function findTasksByGoalId(goalId: string): Task[] {
  function search(nodes: GoalTreeNode[]): Task[] | null {
    for (const node of nodes) {
      if (node.goal.id === goalId) return node.tasks;
      const found = search(node.sub_goals);
      if (found !== null) return found;
    }
    return null;
  }
  return search(goalStore.goalTree) ?? [];
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

// ===== 拖拽：共享状态 + 跨目标移动 =====
const draggingTaskId = ref<string | null>(null);
const draggingGoal = ref<Goal | null>(null);
const dragOverGoalId = ref<string | null>(null);
const dropPosition = ref<"before" | "inside" | "after">("inside");
const dragOverTaskId = ref<string | null>(null);
const taskDropPosition = ref<"before" | "after">("before");

async function handleMoveTask(
  task: Task,
  targetGoalId: string,
  beforeTaskId?: string | null,
) {
  try {
    await taskApi.moveTask({
      task_id: task.id,
      goal_id: targetGoalId,
      before_task_id: beforeTaskId ?? null,
    });
    message.success(`已将任务"${task.name}"移动到新位置`);
    await goalStore.fetchGoalTree();
    await goalStore.fetchProgresses();
    expandedNodes.value.add(targetGoalId);
  } catch (e) {
    message.error(String(e));
  }
}

async function handleMoveGoal(
  goalId: string,
  newParentId: string | null,
  beforeGoalId?: string | null,
) {
  try {
    await goalApi.moveGoal({
      goal_id: goalId,
      new_parent_id: newParentId,
      before_goal_id: beforeGoalId ?? null,
    });
    message.success("目标已移动到新位置");
    await goalStore.fetchGoalTree();
    await goalStore.fetchProgresses();
    // 展开新父目标（若有）以便用户立刻看到结果
    if (newParentId) {
      expandedNodes.value.add(newParentId);
    }
  } catch (e) {
    message.error(String(e));
  }
}

/** 查找目标的下一个同级目标 ID（用于目标拖拽插入下方） */
function findNextSiblingId(goalId: string): string | null {
  function search(nodes: GoalTreeNode[]): string | null {
    for (let i = 0; i < nodes.length; i++) {
      if (nodes[i].goal.id === goalId) {
        return nodes[i + 1]?.goal.id ?? null;
      }
      const found = search(nodes[i].sub_goals);
      if (found) return found;
    }
    return null;
  }
  return search(goalStore.goalTree);
}

/** 在整棵目标树中查找任务（跨目标拖拽时定位被拖任务对象） */
function findTaskInTree(taskId: string): Task | null {
  function search(nodes: GoalTreeNode[]): Task | null {
    for (const node of nodes) {
      const t = node.tasks.find((x) => x.id === taskId);
      if (t) return t;
      const found = search(node.sub_goals);
      if (found) return found;
    }
    return null;
  }
  return search(goalStore.goalTree);
}

/** 清空任务拖拽共享状态 */
function clearTaskDrag() {
  draggingTaskId.value = null;
  dragOverTaskId.value = null;
  dragOverGoalId.value = null;
}

// ===== 向递归子组件注入树 API（避免逐层 prop 透传）=====
const treeApi: GoalTreeApi = {
  expandedNodes,
  toggleNode,
  getDaysLeft,
  openCreateGoalModal,
  openCreateTaskModal,
  openEditGoalModal,
  openSplitModal,
  handleReplanPreview,
  handleDeleteGoal,
  buildTaskActions,
  handleTaskAction,
  handleMoveTask,
  handleMoveGoal,
  draggingTaskId,
  draggingGoal,
  dragOverGoalId,
  dropPosition,
  dragOverTaskId,
  taskDropPosition,
  findNextSiblingId,
  findTaskInTree,
  clearTaskDrag,
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
      <NSpace>
        <NButton @click="openTemplateModal">
          <template #icon><Icon icon="mdi:clipboard-list-outline" /></template>
          使用模板
        </NButton>
        <NButton type="primary" @click="openCreateGoalModal(null)">
          <template #icon><Icon icon="mdi:plus" /></template>
          创建总目标
        </NButton>
      </NSpace>
    </div>

    <!-- 提示 -->
    <div
      class="text-xs text-gray-400 flex items-start gap-1 bg-gray-50 px-3 py-1.5 rounded"
    >
      <Icon icon="mdi:information-outline" width="14" class="mt-0.5 shrink-0" />
      <span>
        支持任意层级嵌套子目标；每个子目标可设置独立的总量、单位和截止日期，实现分阶段拆解（如先看视频后做题）；任务行可拖拽到其他目标下调整归属。点击「使用模板」快速创建分阶段目标结构，点击「自动拆解」按截止日期/时间预算/日期范围生成每日任务。
      </span>
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
        <NFormItem label="每日可用时长（时间预算拆解默认值）">
          <NInputNumber
            v-model:value="goalForm.daily_capacity"
            :min="0"
            :step="0.5"
            placeholder="如：2（每天2小时）"
            clearable
            style="width: 100%"
          />
          <template #feedback>
            <span class="text-gray-400 text-xs">
              设置后，使用"自动拆解"的"按时间预算"策略时将预填此值
            </span>
          </template>
        </NFormItem>
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

    <!-- 目标模板弹窗（P1-2 非线性阶段拆解） -->
    <NModal
      v-model:show="showTemplateModal"
      preset="card"
      title="使用模板创建分阶段目标"
      style="width: 560px"
    >
      <div class="space-y-4">
        <!-- 模板选择 -->
        <NFormItem label="选择模板" :show-feedback="false">
          <NSelect
            :value="selectedTemplateId"
            :options="
              goalTemplates.map((t) => ({
                label: t.name,
                value: t.id,
              }))
            "
            @update:value="onTemplateChange"
          />
        </NFormItem>
        <div
          v-if="selectedTemplate"
          class="text-xs text-gray-500 flex items-start gap-1 bg-blue-50 px-3 py-2 rounded"
        >
          <Icon
            :icon="selectedTemplate.icon"
            width="16"
            class="mt-0.5 shrink-0 text-blue-500"
          />
          <span>{{ selectedTemplate.description }}</span>
        </div>

        <!-- 总目标信息 -->
        <div class="border-t pt-3 space-y-2">
          <div class="text-sm font-medium text-gray-600">总目标</div>
          <NFormItem label="目标名称" :show-feedback="false" required>
            <NInput
              v-model:value="templateForm.parent_name"
              placeholder="如：Vue 框架学习"
            />
          </NFormItem>
          <NFormItem label="截止日期" :show-feedback="false">
            <NDatePicker
              v-model:value="templateForm.parent_deadline"
              type="date"
              clearable
              :is-date-disabled="(ts: number) => ts < Date.now() - 86400000"
            />
          </NFormItem>
        </div>

        <!-- 各阶段子目标 -->
        <div
          v-for="(stage, idx) in templateForm.stages"
          :key="idx"
          class="border-t pt-3 space-y-2"
        >
          <div
            class="text-sm font-medium text-gray-600 flex items-center gap-1"
          >
            <Icon icon="mdi:flag-outline" width="16" class="text-blue-500" />
            阶段 {{ idx + 1 }}
          </div>
          <NFormItem label="阶段名称" :show-feedback="false" required>
            <NInput v-model:value="stage.name" placeholder="如：视频学习" />
          </NFormItem>
          <NSpace>
            <NFormItem label="总量" :show-feedback="false">
              <NInputNumber v-model:value="stage.total_qty" :min="1" />
            </NFormItem>
            <NFormItem label="单位" :show-feedback="false">
              <NSelect
                v-model:value="stage.unit"
                :options="unitOptions"
                style="width: 100px"
              />
            </NFormItem>
          </NSpace>
          <NFormItem label="截止日期（可选）" :show-feedback="false">
            <NDatePicker
              v-model:value="stage.deadline"
              type="date"
              clearable
              :is-date-disabled="(ts: number) => ts < Date.now() - 86400000"
            />
          </NFormItem>
        </div>

        <!-- 使用说明 -->
        <div
          class="text-xs text-gray-400 flex items-start gap-1 bg-gray-50 px-3 py-2 rounded"
        >
          <Icon
            icon="mdi:lightbulb-outline"
            width="14"
            class="mt-0.5 shrink-0"
          />
          <span>
            创建后，对每个阶段子目标分别点击「自动拆解」即可按各自节奏生成每日任务。各阶段进度独立汇总到总目标。
          </span>
        </div>
      </div>
      <template #footer>
        <NSpace justify="end">
          <NButton @click="showTemplateModal = false">取消</NButton>
          <NButton type="primary" @click="handleSaveTemplate">创建</NButton>
        </NSpace>
      </template>
    </NModal>

    <!-- 自动拆解配置弹窗（整合：按截止日期均分 / 按时间预算 / 自定义日期范围） -->
    <NModal
      v-model:show="showSplitModal"
      preset="card"
      title="自动拆解"
      style="width: 520px"
    >
      <div class="space-y-4">
        <div class="text-sm text-gray-600">
          目标：<strong>{{ splitForm.goal_name }}</strong>
        </div>

        <!-- 拆解策略选择 -->
        <NFormItem label="拆解方式" :show-feedback="false">
          <NRadioGroup v-model:value="splitForm.strategy">
            <NSpace vertical>
              <NRadio
                v-for="opt in splitStrategyOptions"
                :key="opt.value"
                :value="opt.value"
              >
                {{ opt.label }}
              </NRadio>
            </NSpace>
          </NRadioGroup>
        </NFormItem>
        <div
          class="text-xs text-blue-600 bg-blue-50 px-3 py-2 rounded flex items-start gap-1"
        >
          <Icon
            icon="mdi:information-outline"
            width="14"
            class="mt-0.5 shrink-0"
          />
          <span>{{ currentStrategyDesc }}</span>
        </div>

        <!-- 通用参数：总量 + 单位 -->
        <div class="border-t pt-3">
          <NSpace>
            <NFormItem label="总量" :show-feedback="false">
              <NInputNumber v-model:value="splitForm.total_qty" :min="0" />
            </NFormItem>
            <NFormItem label="单位" :show-feedback="false">
              <NSelect
                v-model:value="splitForm.unit"
                :options="unitOptions"
                style="width: 100px"
              />
            </NFormItem>
          </NSpace>
        </div>

        <!-- 策略参数：按截止日期均分 -->
        <div v-if="splitForm.strategy === 'by_deadline'" class="border-t pt-3">
          <NFormItem label="截止日期" :show-feedback="false">
            <NDatePicker
              v-model:value="splitForm.deadline"
              type="date"
              clearable
              :is-date-disabled="(ts: number) => ts < Date.now() - 86400000"
              style="width: 100%"
            />
          </NFormItem>
        </div>

        <!-- 策略参数：按时间预算 -->
        <div
          v-else-if="splitForm.strategy === 'by_capacity'"
          class="border-t pt-3 space-y-3"
        >
          <NFormItem label="每日可用时长" :show-feedback="false">
            <NInputNumber
              v-model:value="splitForm.daily_capacity"
              :min="0"
              :step="0.5"
              placeholder="如：2（每天2小时）"
              clearable
              style="width: 100%"
            />
          </NFormItem>
          <NFormItem label="截止日期" :show-feedback="false">
            <NDatePicker
              v-model:value="splitForm.deadline"
              type="date"
              clearable
              :is-date-disabled="(ts: number) => ts < Date.now() - 86400000"
              style="width: 100%"
            />
          </NFormItem>
        </div>

        <!-- 策略参数：自定义日期范围 -->
        <div
          v-else-if="splitForm.strategy === 'by_date_range'"
          class="border-t pt-3 space-y-3"
        >
          <NSpace>
            <NFormItem label="起始日期" :show-feedback="false">
              <NDatePicker
                v-model:value="splitForm.start_date"
                type="date"
                clearable
                style="width: 100%"
              />
            </NFormItem>
            <NFormItem label="结束日期" :show-feedback="false">
              <NDatePicker
                v-model:value="splitForm.end_date"
                type="date"
                clearable
                style="width: 100%"
              />
            </NFormItem>
          </NSpace>
          <NFormItem label="每日数量（可选）" :show-feedback="false">
            <NInputNumber
              v-model:value="splitForm.per_day_qty"
              :min="0"
              placeholder="不填则按天数均分总量"
              clearable
              style="width: 100%"
            />
          </NFormItem>
        </div>

        <!-- 预览 -->
        <div
          v-if="splitPreview"
          class="border-t pt-3 text-sm bg-gray-50 px-3 py-2 rounded space-y-1"
          :class="splitPreview.error ? 'text-red-500' : 'text-gray-600'"
        >
          <div v-if="!splitPreview.error">
            <Icon icon="mdi:chart-bar" class="inline-block mr-1" />
            预计 <strong>{{ splitPreview.taskCount }}</strong> 个任务， 每日计划
            <strong>{{ splitPreview.perDay }}</strong
            >{{ splitForm.unit }}， 跨越
            <strong>{{ splitPreview.spanDays }}</strong> 天
          </div>
          <div v-if="splitPreview.detail" class="text-xs text-gray-500 pl-5">
            {{ splitPreview.detail }}
          </div>
          <div v-if="splitPreview.error" class="flex items-start gap-1">
            <Icon
              icon="mdi:alert-circle-outline"
              width="16"
              class="mt-0.5 shrink-0"
            />
            <span>{{ splitPreview.error }}</span>
          </div>
        </div>
      </div>
      <template #footer>
        <NSpace justify="end">
          <NButton @click="showSplitModal = false">取消</NButton>
          <NButton
            type="primary"
            :disabled="!splitPreview || !!splitPreview.error"
            @click="handleSmartSplit"
          >
            开始拆解
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
          <NCheckbox v-model:checked="taskForm.is_repeat"> 重复任务 </NCheckbox>
          <span class="text-xs text-gray-400">
            {{
              taskForm.is_repeat
                ? "在日期范围内按频率生成多个任务"
                : "只生成一个单次任务"
            }}
          </span>
        </div>
        <!-- 频率选择（仅创建模式 + 重复任务时显示） -->
        <div v-if="taskModalMode === 'create' && taskForm.is_repeat">
          <NFormItem label="频率" :show-feedback="false">
            <NRadioGroup v-model:value="taskForm.frequency">
              <NRadio
                v-for="opt in frequencyOptions"
                :key="opt.value"
                :value="opt.value"
              >
                {{ opt.label }}
              </NRadio>
            </NRadioGroup>
          </NFormItem>
          <!-- 周几多选（仅 weekly） -->
          <NFormItem
            v-if="taskForm.frequency === 'weekly'"
            label="周几"
            :show-feedback="false"
          >
            <NCheckboxGroup v-model:value="taskForm.weekdays">
              <NSpace>
                <NCheckbox
                  v-for="opt in weekdayOptions"
                  :key="opt.value"
                  :value="opt.value"
                >
                  {{ opt.label }}
                </NCheckbox>
              </NSpace>
            </NCheckboxGroup>
          </NFormItem>
          <!-- 每月几号多选（仅 monthly） -->
          <NFormItem
            v-if="taskForm.frequency === 'monthly'"
            label="日期"
            :show-feedback="false"
          >
            <NSelect
              v-model:value="taskForm.month_days"
              :options="monthDayOptions"
              multiple
              placeholder="选择每月几号"
              style="width: 100%"
            />
          </NFormItem>
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
        <!-- 前置任务依赖（编辑模式或单次创建模式可设置；重复任务创建时不显示） -->
        <NFormItem
          v-if="
            taskModalMode === 'edit' ||
            (taskModalMode === 'create' && !taskForm.is_repeat)
          "
          label="前置任务"
          :show-feedback="false"
        >
          <NSelect
            v-model:value="taskForm.dependency_ids"
            :options="dependencyOptions"
            multiple
            filterable
            placeholder="选择前置任务（完成后本任务才可执行）"
            style="width: 100%"
          />
        </NFormItem>
        <div
          v-if="taskModalMode === 'edit' && taskForm.dependency_ids.length > 0"
          class="text-xs text-gray-500 bg-gray-50 px-3 py-2 rounded"
        >
          <Icon icon="mdi:link-variant" class="inline-block mr-1" />
          本任务将在所有前置任务完成后解锁
        </div>
        <div
          v-if="taskModalMode === 'create'"
          class="text-xs text-blue-500 bg-blue-50 px-3 py-2 rounded"
        >
          <Icon icon="mdi:information" class="inline-block mr-1" />
          纯文字类任务（如练习题）可勾选"重复任务"按每天/每周/每月生成；视频/数量类任务请使用"自动拆解"
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
