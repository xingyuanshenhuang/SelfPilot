<script setup lang="ts">
import {
  ref,
  computed,
  onMounted,
  onBeforeUnmount,
  watch,
  nextTick,
} from "vue";
import type { ComponentPublicInstance } from "vue";
import {
  NCard,
  NButton,
  NSpace,
  NRadioGroup,
  NRadioButton,
  NTag,
  NEmpty,
  NCheckbox,
  NTooltip,
  NSpin,
  NPopover,
  NProgress,
  NSelect,
  NSwitch,
  useMessage,
  useDialog,
} from "naive-ui";
import { Icon } from "@iconify/vue";
import {
  format,
  addMonths,
  subMonths,
  addWeeks,
  subWeeks,
  addDays,
  subDays,
  startOfMonth,
  endOfMonth,
  startOfWeek,
  endOfWeek,
  eachDayOfInterval,
  isSameDay,
  isSameMonth,
  isToday,
} from "date-fns";
import { zhCN } from "date-fns/locale";
import * as taskApi from "@/api/task";
import * as statsApi from "@/api/stats";
import { useGoalStore } from "@/stores/goalStore";
import type { CalendarTask, TaskStatus, DailyLoad } from "@/types";
import { STATUS_META } from "@/types";

type ViewMode = "month" | "week" | "day";

const goalStore = useGoalStore();
const message = useMessage();
const dialog = useDialog();

const viewMode = ref<ViewMode>("month");
const currentDate = ref(new Date());
const selectedDate = ref(new Date());
const tasks = ref<CalendarTask[]>([]);
const loading = ref(false);

// P2-5：每日负载数据（按日期索引）
const dailyLoadMap = ref<Record<string, DailyLoad>>({});
// 负载阈值：≥4 中等，≥7 过载，≥11 极高
const LOAD_THRESHOLD_MEDIUM = 4;
const LOAD_THRESHOLD_HIGH = 7;
const LOAD_THRESHOLD_EXTREME = 11;

// P2-5+：圆环仪表盘参数（方案 D）
// 满负载参考值（12 个任务 = 100%），圆环半径 r=10，stroke-width=3
const LOAD_MAX_CAPACITY = 12;
const RING_RADIUS = 10;
const RING_CIRCUMFERENCE = 2 * Math.PI * RING_RADIUS; // ≈62.83

// 负载等级配色（4 级，与 HTML demo 一致）
const LOAD_COLORS: Record<"low" | "medium" | "high" | "extreme", string> = {
  low: "#22c55e", // green-500
  medium: "#f59e0b", // amber-500
  high: "#ef4444", // red-500
  extreme: "#9333ea", // purple-600
};

// P2-2：记录上一个视图模式，用于 Esc 返回
const prevViewMode = ref<ViewMode | null>(null);

// P2-2 R2：月视图 roving tabindex - 当前焦点日期
const focusedDay = ref<Date>(new Date());

/** 同步 focusedDay 到 currentDate（视图切换/导航时调用） */
function syncFocusedDay() {
  focusedDay.value = new Date(currentDate.value);
}

// 批量操作选中
const selectedTaskIds = ref<Set<string>>(new Set());

// P2-1：筛选条件
const filterGoalIds = ref<string[]>([]);
const filterStatuses = ref<TaskStatus[]>([]);
const filterOverdueOnly = ref(false);

// 可折叠筛选栏状态（localStorage 持久化）
const FILTER_COLLAPSE_KEY = "selfpilot:calendar:filterCollapsed";
const filterCollapsed = ref<boolean>(
  (() => {
    try {
      return localStorage.getItem(FILTER_COLLAPSE_KEY) !== "false";
    } catch {
      return true;
    }
  })(),
);

function toggleFilterCollapsed() {
  filterCollapsed.value = !filterCollapsed.value;
  try {
    localStorage.setItem(FILTER_COLLAPSE_KEY, String(filterCollapsed.value));
  } catch {
    // ignore storage errors (private mode etc.)
  }
  // 展开后聚焦到第一个可交互元素，便于键盘用户继续操作
  if (!filterCollapsed.value) {
    nextTick(() => {
      filterContentRef.value
        ?.querySelector<HTMLElement>(
          "input, button, [role='button'], [role='combobox'], [tabindex='0']",
        )
        ?.focus();
    });
  }
}

// 展开内容区 DOM 引用（用于聚焦管理）
const filterContentRef = ref<HTMLElement | null>(null);

// 筛选栏激活条件数（用于折叠态徽标提示）
const filterActiveCount = computed(
  () =>
    filterGoalIds.value.length +
    filterStatuses.value.length +
    (filterOverdueOnly.value ? 1 : 0),
);

const filterHasCondition = computed(
  () =>
    filterGoalIds.value.length > 0 ||
    filterStatuses.value.length > 0 ||
    filterOverdueOnly.value,
);

function resetFilter() {
  filterGoalIds.value = [];
  filterStatuses.value = [];
  filterOverdueOnly.value = false;
}

/** P2-1：目标下拉选项（来自 goalStore.goals） */
const goalOptions = computed(() =>
  goalStore.goals.map((g) => ({ label: g.name, value: g.id })),
);

const statusOptions: { label: string; value: TaskStatus }[] = [
  { label: "未完成", value: "pending" },
  { label: "部分完成", value: "partial" },
  { label: "已完成", value: "done" },
  { label: "已跳过", value: "skipped" },
];

const weekDays = ["一", "二", "三", "四", "五", "六", "日"];

onMounted(async () => {
  // P2-2：注册全局键盘导航监听器
  window.addEventListener("keydown", onGlobalKeydown, true);
  await goalStore.fetchGoals();
  await loadData();
});

onBeforeUnmount(() => {
  // P2-2：移除全局键盘导航监听器，避免内存泄漏
  window.removeEventListener("keydown", onGlobalKeydown, true);
});

watch([viewMode, currentDate], () => {
  loadData();
});

async function loadData() {
  loading.value = true;
  try {
    const { start, end } = getDateRange();
    const startStr = format(start, "yyyy-MM-dd");
    const endStr = format(end, "yyyy-MM-dd");
    // P2-5：并行查询任务列表和每日负载
    const [taskList, loadList] = await Promise.all([
      taskApi.listTasksByDateRange(startStr, endStr),
      statsApi.getDailyLoad(startStr, endStr),
    ]);
    tasks.value = taskList;
    dailyLoadMap.value = Object.fromEntries(loadList.map((l) => [l.date, l]));
  } catch (e) {
    message.error(String(e));
  } finally {
    loading.value = false;
  }
}

/** 根据视图模式获取查询日期范围（多查一周以填充首尾） */
function getDateRange(): { start: Date; end: Date } {
  if (viewMode.value === "month") {
    const start = startOfWeek(startOfMonth(currentDate.value), {
      weekStartsOn: 1,
    });
    const end = endOfWeek(endOfMonth(currentDate.value), { weekStartsOn: 1 });
    return { start, end };
  }
  if (viewMode.value === "week") {
    const start = startOfWeek(currentDate.value, { weekStartsOn: 1 });
    const end = endOfWeek(currentDate.value, { weekStartsOn: 1 });
    return { start, end };
  }
  // day 模式：单日查询
  return { start: currentDate.value, end: currentDate.value };
}

/** 月视图网格日期 */
const monthGrid = computed(() => {
  const { start, end } = getDateRange();
  return eachDayOfInterval({ start, end });
});

/** 周视图日期 */
const weekGrid = computed(() => {
  const start = startOfWeek(currentDate.value, { weekStartsOn: 1 });
  const end = endOfWeek(currentDate.value, { weekStartsOn: 1 });
  return eachDayOfInterval({ start, end });
});

/** P2-1：按筛选条件过滤后的任务列表 */
const filteredTasks = computed(() => {
  let list = tasks.value;
  if (filterGoalIds.value.length > 0) {
    const idSet = new Set(filterGoalIds.value);
    list = list.filter((t) => idSet.has(t.goal_id));
  }
  if (filterStatuses.value.length > 0) {
    const statusSet = new Set(filterStatuses.value);
    list = list.filter((t) => statusSet.has(t.status));
  }
  if (filterOverdueOnly.value) {
    list = list.filter((t) => t.is_overdue);
  }
  return list;
});

/** 按日期分组任务 */
const tasksByDate = computed(() => {
  const map: Record<string, CalendarTask[]> = {};
  for (const t of filteredTasks.value) {
    if (!t.plan_date) continue;
    if (!map[t.plan_date]) map[t.plan_date] = [];
    map[t.plan_date].push(t);
  }
  return map;
});

function getTasksOfDay(day: Date): CalendarTask[] {
  const key = format(day, "yyyy-MM-dd");
  return tasksByDate.value[key] || [];
}

/** 当日完成统计 */
function getDayStats(day: Date) {
  const list = getTasksOfDay(day);
  const total = list.length;
  const done = list.filter((t) => t.status === "done").length;
  const partial = list.filter((t) => t.status === "partial").length;
  const overdue = list.filter((t) => t.is_overdue).length;
  return { total, done, partial, overdue };
}

/** P1-4：预计算所有日期的统计，避免模板中重复调用 */
const dayStatsMap = computed(() => {
  const map: Record<string, ReturnType<typeof getDayStats>> = {};
  for (const key of Object.keys(tasksByDate.value)) {
    const list = tasksByDate.value[key];
    const total = list.length;
    const done = list.filter((t) => t.status === "done").length;
    const partial = list.filter((t) => t.status === "partial").length;
    const overdue = list.filter((t) => t.is_overdue).length;
    map[key] = { total, done, partial, overdue };
  }
  return map;
});

const EMPTY_STATS = { total: 0, done: 0, partial: 0, overdue: 0 };

function getDayStatsCached(day: Date) {
  const key = format(day, "yyyy-MM-dd");
  return dayStatsMap.value[key] || EMPTY_STATS;
}

// P2-5：负载等级判断（4 级：none/low/medium/high/extreme）
function getLoadLevel(
  day: Date,
): "none" | "low" | "medium" | "high" | "extreme" {
  const key = format(day, "yyyy-MM-dd");
  const load = dailyLoadMap.value[key];
  if (!load || load.total_tasks === 0) return "none";
  if (load.total_tasks >= LOAD_THRESHOLD_EXTREME) return "extreme";
  if (load.total_tasks >= LOAD_THRESHOLD_HIGH) return "high";
  if (load.total_tasks >= LOAD_THRESHOLD_MEDIUM) return "medium";
  return "low";
}

/** P2-5：获取某日的负载描述文本（用于悬浮提示） */
function getLoadOfDay(day: Date): string {
  const key = format(day, "yyyy-MM-dd");
  const load = dailyLoadMap.value[key];
  if (!load || load.total_tasks === 0) return "无任务";
  const detail = load.by_goal
    .map((g) => `${g.goal_name}×${g.task_count}`)
    .join(", ");
  return `${load.total_tasks} 个任务（${detail}）`;
}

/** P2-5+：圆环仪表盘辅助函数（方案 D） */

/** 获取当日负载等级对应的颜色（无任务返回轨道灰） */
function getLoadColor(day: Date): string {
  const level = getLoadLevel(day);
  if (level === "none") return "#d1d5db"; // gray-300（轨道色）
  return LOAD_COLORS[level];
}

/** 获取当日任务总数（用于圆环中心数字） */
function getLoadCount(day: Date): number {
  const key = format(day, "yyyy-MM-dd");
  return dailyLoadMap.value[key]?.total_tasks ?? 0;
}

/** 获取当日负载百分比（0-100，上限 100） */
function getLoadPercentage(day: Date): number {
  const count = getLoadCount(day);
  return Math.min(100, Math.round((count / LOAD_MAX_CAPACITY) * 100));
}

/** 获取圆环 stroke-dasharray（已填充长度 + 剩余空隙） */
function getRingDashArray(day: Date): string {
  const pct = getLoadPercentage(day);
  const filled = (pct / 100) * RING_CIRCUMFERENCE;
  return `${filled} ${RING_CIRCUMFERENCE}`;
}

// 日期导航
function prev() {
  if (viewMode.value === "month")
    currentDate.value = subMonths(currentDate.value, 1);
  else if (viewMode.value === "week")
    currentDate.value = subWeeks(currentDate.value, 1);
  else currentDate.value = subDays(currentDate.value, 1);
  syncFocusedDay();
}
function next() {
  if (viewMode.value === "month")
    currentDate.value = addMonths(currentDate.value, 1);
  else if (viewMode.value === "week")
    currentDate.value = addWeeks(currentDate.value, 1);
  else currentDate.value = addDays(currentDate.value, 1);
  syncFocusedDay();
}
function goToday() {
  currentDate.value = new Date();
  selectedDate.value = new Date();
  syncFocusedDay();
}

function selectDay(day: Date) {
  selectedDate.value = day;
  focusedDay.value = new Date(day);
  if (viewMode.value === "month") {
    // 月视图点击切换到 day 模式查看详情
    prevViewMode.value = "month";
    currentDate.value = day;
    viewMode.value = "day";
    // P2-2 修复：切到日视图后聚焦日视图容器，避免焦点丢失到 body
    // 让方向键/Esc/Ctrl+A 等全局快捷键能正常响应
    nextTick(() => {
      // 优先聚焦第一个可交互元素（更自然），fallback 到容器
      const dayView = document.querySelector(
        '[aria-label="日视图"]',
      ) as HTMLElement | null;
      const firstInteractive = dayView?.querySelector<HTMLElement>(
        "input, button, [role='checkbox']",
      );
      (firstInteractive ?? dayView)?.focus();
    });
  }
}

const headerLabel = computed(() => {
  if (viewMode.value === "month")
    return format(currentDate.value, "yyyy 年 M 月");
  if (viewMode.value === "week") {
    const start = startOfWeek(currentDate.value, { weekStartsOn: 1 });
    const end = endOfWeek(currentDate.value, { weekStartsOn: 1 });
    return `${format(start, "MM-dd")} ~ ${format(end, "MM-dd")}`;
  }
  return format(currentDate.value, "yyyy 年 M 月 d 日 EEEE", { locale: zhCN });
});

// 选中日期的任务列表
const selectedDayTasks = computed(() => getTasksOfDay(selectedDate.value));

/** P1-3：周期统计概览（按视图模式聚合当前周期任务） */
const periodLabel = computed(() => {
  if (viewMode.value === "month") return "本月";
  if (viewMode.value === "week") return "本周";
  return "当日";
});

const periodStats = computed(() => {
  let periodTasks: CalendarTask[] = [];
  if (viewMode.value === "month") {
    const startStr = format(startOfMonth(currentDate.value), "yyyy-MM-dd");
    const endStr = format(endOfMonth(currentDate.value), "yyyy-MM-dd");
    periodTasks = filteredTasks.value.filter(
      (t) => t.plan_date && t.plan_date >= startStr && t.plan_date <= endStr,
    );
  } else if (viewMode.value === "week") {
    const startStr = format(
      startOfWeek(currentDate.value, { weekStartsOn: 1 }),
      "yyyy-MM-dd",
    );
    const endStr = format(
      endOfWeek(currentDate.value, { weekStartsOn: 1 }),
      "yyyy-MM-dd",
    );
    periodTasks = filteredTasks.value.filter(
      (t) => t.plan_date && t.plan_date >= startStr && t.plan_date <= endStr,
    );
  } else {
    const key = format(currentDate.value, "yyyy-MM-dd");
    periodTasks = filteredTasks.value.filter((t) => t.plan_date === key);
  }

  const total = periodTasks.length;
  const done = periodTasks.filter((t) => t.status === "done").length;
  const skipped = periodTasks.filter((t) => t.status === "skipped").length;
  const pending = periodTasks.filter(
    (t) => t.status !== "done" && t.status !== "skipped",
  ).length;
  const overdue = periodTasks.filter((t) => t.is_overdue).length;
  // 完成率：已完成 / (总数 - 已跳过)，跳过不计入待办
  const effective = total - skipped;
  const completionRate =
    effective > 0 ? Math.round((done / effective) * 100) : 0;
  return { total, done, pending, skipped, overdue, completionRate };
});

/** P1-2：周视图每列完成率 */
function getDayCompletionRate(day: Date): number {
  const stats = getDayStatsCached(day);
  const effective = stats.total;
  if (effective === 0) return 0;
  return Math.round((stats.done / effective) * 100);
}

/** P1-2：周视图是否存在可操作任务（用于显示批量工具栏） */
const weekHasTasks = computed(() =>
  weekGrid.value.some((day) => getTasksOfDay(day).length > 0),
);

// 批量选择
function toggleSelect(taskId: string, checked: boolean) {
  if (checked) selectedTaskIds.value.add(taskId);
  else selectedTaskIds.value.delete(taskId);
}

function selectAllVisible() {
  for (const t of selectedDayTasks.value) {
    if (t.status !== "done" && t.status !== "skipped" && !t.is_blocked) {
      selectedTaskIds.value.add(t.id);
    }
  }
}
/** P1-2：周视图全选当前周可操作任务 */
function selectAllVisibleWeek() {
  for (const day of weekGrid.value) {
    for (const t of getTasksOfDay(day)) {
      if (t.status !== "done" && t.status !== "skipped" && !t.is_blocked) {
        selectedTaskIds.value.add(t.id);
      }
    }
  }
}
function clearSelection() {
  selectedTaskIds.value.clear();
}

async function batchComplete() {
  const ids = Array.from(selectedTaskIds.value);
  // 防御性过滤阻塞任务，避免后端校验报错
  const validIds = ids.filter(
    (id) => !tasks.value.find((t) => t.id === id)?.is_blocked,
  );
  if (validIds.length === 0) {
    message.warning("没有可完成的任务（前置依赖未就绪）");
    return;
  }
  let ok = 0;
  const affectedGoalIds = new Set<string>();
  for (const id of validIds) {
    try {
      const task = tasks.value.find((t) => t.id === id);
      if (!task) continue;
      // P2-3：保存返回值，局部更新任务
      const updated = await taskApi.completeTask({
        task_id: id,
        actual_qty: task.plan_qty,
      });
      goalStore.updateTaskLocally(updated);
      affectedGoalIds.add(updated.goal_id);
      ok++;
    } catch (e) {
      message.error(`任务 ${id} 完成失败: ${String(e)}`);
    }
  }
  if (ok > 0) {
    message.success(`已批量完成 ${ok} 个任务`);
    // P2-3：局部刷新受影响目标的祖先链进度
    for (const gid of affectedGoalIds) {
      await goalStore.refreshProgressForGoalChain(gid);
    }
    await loadData();
    clearSelection();
  }
}

async function batchSkip() {
  const ids = Array.from(selectedTaskIds.value);
  if (ids.length === 0) {
    message.warning("请先选择任务");
    return;
  }
  let ok = 0;
  const affectedGoalIds = new Set<string>();
  for (const id of ids) {
    try {
      // P2-3：保存返回值，局部更新任务
      const updated = await taskApi.skipTask(id);
      goalStore.updateTaskLocally(updated);
      affectedGoalIds.add(updated.goal_id);
      ok++;
    } catch (e) {
      message.error(`任务 ${id} 跳过失败: ${String(e)}`);
    }
  }
  if (ok > 0) {
    message.success(`已批量跳过 ${ok} 个任务`);
    // P2-3：局部刷新受影响目标的祖先链进度
    for (const gid of affectedGoalIds) {
      await goalStore.refreshProgressForGoalChain(gid);
    }
    await loadData();
    clearSelection();
  }
}

async function quickComplete(task: CalendarTask) {
  try {
    // P2-3：保存返回值，局部更新任务
    const updated = await taskApi.completeTask({
      task_id: task.id,
      actual_qty: task.plan_qty,
    });
    goalStore.updateTaskLocally(updated);
    await goalStore.refreshProgressForGoalChain(updated.goal_id);
    await loadData();
    message.success("已完成");
  } catch (e) {
    message.error(String(e));
  }
}

function quickSkip(task: CalendarTask) {
  dialog.warning({
    title: "跳过任务",
    content: `确定跳过任务"${task.name}"？`,
    positiveText: "跳过",
    negativeText: "取消",
    onPositiveClick: async () => {
      try {
        // P2-3：保存返回值，局部更新任务
        const updated = await taskApi.skipTask(task.id);
        goalStore.updateTaskLocally(updated);
        await goalStore.refreshProgressForGoalChain(updated.goal_id);
        await loadData();
        message.info("已跳过");
      } catch (e) {
        message.error(String(e));
      }
    },
  });
}

/** P2-2 R2：月视图日期网格键盘导航
 * - 仅处理 Enter/Space 阻止 button 默认 click（不进入日视图）
 * - 方向键/Home/End 由全局 onGlobalKeydown 统一处理（避免 capture/bubble 事件传播问题）
 */
function onMonthCellKeydown(e: KeyboardEvent) {
  // Enter/Space：阻止默认 click 行为，不进入日视图
  // 用户应使用数字键 3 切换到日视图，或鼠标点击日期单元格
  if (e.key === "Enter" || e.key === " ") {
    e.preventDefault();
  }
}

/** P2-2 R1：月视图单元格 DOM 引用（用于 roving tabindex 焦点管理） */
const monthCellRefs = ref<Map<string, HTMLElement>>(new Map());

function setMonthCellRef(
  el: Element | ComponentPublicInstance | null,
  day: Date,
) {
  const key = format(day, "yyyy-MM-dd");
  const map = monthCellRefs.value as Map<string, HTMLElement>;
  if (el && el instanceof HTMLElement) {
    map.set(key, el);
  } else {
    map.delete(key);
  }
}

/** P2-2 R2：月视图单元格是否为当前焦点格（roving tabindex） */
function isMonthCellFocusable(day: Date): boolean {
  const focusedInGrid = monthGrid.value.some((d) =>
    isSameDay(d, focusedDay.value),
  );
  if (focusedInGrid) return isSameDay(day, focusedDay.value);
  return isToday(day);
}

/**
 * 判断当前键盘事件是否发生在可输入/可编辑元素中，
 * 若是则跳过全局快捷键处理（避免在输入框中按方向键/A 键触发视图切换）
 */
function isEventInEditableTarget(e: KeyboardEvent): boolean {
  const target = e.target as HTMLElement | null;
  if (!target) return false;
  const tag = target.tagName;
  if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") {
    return true;
  }
  // Naive UI 的 NSelect/NInput 等使用 contenteditable 或 role=combobox
  if (target.isContentEditable) return true;
  if (target.getAttribute("role") === "combobox") return true;
  // 在 NSelect 弹层内
  const inDropdown = target.closest(".n-base-select-option, .n-checkbox");
  if (inDropdown) return true;
  return false;
}

/**
 * P2-2：判断是否有"交互式"弹层打开（需要独占键盘操作的弹层）
 * - NSelect 下拉、NDialog、NDrawer：返回 true（阻止全局快捷键）
 * - hover 预览 popover（日期/任务悬浮提示）：返回 false（不阻止方向键）
 * 通过检查弹层内是否含可交互控件区分
 */
function isInteractiveOverlayOpen(): boolean {
  // NDialog / NDrawer：直接判断容器存在
  if (document.querySelector(".n-modal-container, .n-drawer")) return true;

  // NSelect 下拉菜单（动态挂载到 body）
  const selectMenus = document.querySelectorAll<HTMLElement>(
    ".n-base-select-menu",
  );
  for (const menu of selectMenus) {
    if (menu.offsetParent !== null) return true;
  }

  // NPopover：含可交互控件 或 含快捷键说明（键盘帮助 popover）时视为交互式
  // hover 预览 popover 仅含文本/链接，不阻止方向键
  const popovers = document.querySelectorAll<HTMLElement>(".n-popover");
  for (const p of popovers) {
    if (p.offsetParent === null) continue;
    // 含 input/select/textarea
    const hasInput = p.querySelector(
      "input, select, textarea, .n-base-selection",
    );
    if (hasInput) return true;
    // 键盘帮助 popover（含 kbd 元素）
    if (p.querySelector("kbd")) return true;
  }

  return false;
}

/**
 * P2-2：全局键盘导航处理（挂载在 window 上，避免容器 tabindex=-1 导致的失效问题）
 * - 仅在日历视图内可交互元素聚焦时生效
 * - 弹层打开时仅响应 Esc（由弹层自身处理）
 * - 月视图单元格聚焦时，方向键交给单元格自身处理（网格内移动），不触发月份切换
 */
function onGlobalKeydown(e: KeyboardEvent) {
  // 交互式弹层打开时跳过全局处理（让 Naive UI 自身的 Esc 关闭弹层优先）
  if (isInteractiveOverlayOpen()) return;

  // 在可编辑元素中不触发视图级快捷键
  if (isEventInEditableTarget(e)) return;

  // 当前焦点是否在月视图单元格内？方向键由全局统一处理网格内移动
  const active = document.activeElement;
  const inMonthCell = !!active?.closest(".calendar-cell");
  if (
    inMonthCell &&
    ["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "Home", "End"].includes(
      e.key,
    )
  ) {
    e.preventDefault();
    const grid = monthGrid.value;
    // 从 focusedDay 查找当前索引（而非依赖传入的 day 参数）
    const idx = grid.findIndex((d) => isSameDay(d, focusedDay.value));
    if (idx < 0) return;
    let nextIdx = idx;
    switch (e.key) {
      case "ArrowLeft":
        nextIdx = Math.max(0, idx - 1);
        break;
      case "ArrowRight":
        nextIdx = Math.min(grid.length - 1, idx + 1);
        break;
      case "ArrowUp":
        nextIdx = Math.max(0, idx - 7);
        break;
      case "ArrowDown":
        nextIdx = Math.min(grid.length - 1, idx + 7);
        break;
      case "Home":
        // 当周第一天（周一）
        nextIdx = idx - (((idx % 7) + 7) % 7);
        nextIdx = Math.max(0, nextIdx);
        break;
      case "End":
        // 当周最后一天（周日）
        nextIdx = idx + (6 - (((idx % 7) + 7) % 7));
        nextIdx = Math.min(grid.length - 1, nextIdx);
        break;
    }
    if (nextIdx === idx) return;
    focusedDay.value = new Date(grid[nextIdx]);
    // nextTick 后聚焦新单元格（等待 tabindex 响应式更新）
    nextTick(() => {
      const cell = monthCellRefs.value.get(format(grid[nextIdx], "yyyy-MM-dd"));
      cell?.focus();
    });
    return;
  }

  // 当前焦点是否在按钮/链接等可交互元素上？方向键不触发视图切换，避免误操作
  const onInteractive =
    active?.closest("button, a, [role='checkbox'], [role='radio']") != null;
  if (onInteractive && ["ArrowLeft", "ArrowRight"].includes(e.key)) {
    return;
  }

  // 数字键 1/2/3：切换视图模式（1=月，2=周，3=日），所有视图通用
  if (e.key === "1" || e.key === "2" || e.key === "3") {
    const targetMode: ViewMode =
      e.key === "1" ? "month" : e.key === "2" ? "week" : "day";
    if (viewMode.value !== targetMode) {
      e.preventDefault();
      // 记录 prevViewMode 便于 Esc 返回（仅月→日/周 时记录）
      if (viewMode.value === "month" && targetMode !== "month") {
        prevViewMode.value = "month";
      } else {
        prevViewMode.value = null;
      }
      viewMode.value = targetMode;
    }
    return;
  }

  // Ctrl/Cmd + A：日视图全选当日任务
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "a") {
    if (viewMode.value === "day") {
      e.preventDefault();
      selectAllVisible();
    }
    return;
  }

  // Esc：日/周视图返回上一视图
  if (e.key === "Escape") {
    if (prevViewMode.value) {
      e.preventDefault();
      viewMode.value = prevViewMode.value;
      prevViewMode.value = null;
    }
    return;
  }

  // 左右方向键：切换日期（周/日视图）
  if (e.key === "ArrowLeft") {
    e.preventDefault();
    prev();
    return;
  }
  if (e.key === "ArrowRight") {
    e.preventDefault();
    next();
    return;
  }
}

/** P2-2 R3：日期单元格 aria-label（供屏幕阅读器朗读） */
function getDayAriaLabel(day: Date): string {
  const dateStr = format(day, "yyyy 年 M 月 d 日 EEEE", { locale: zhCN });
  const stats = getDayStatsCached(day);
  if (stats.total === 0) return `${dateStr}，无任务`;
  const parts = [
    `${dateStr}，共 ${stats.total} 个任务`,
    `已完成 ${stats.done}`,
  ];
  if (stats.overdue > 0) parts.push(`${stats.overdue} 个逾期`);
  return parts.join("，");
}

/** P2-2 R4：任务 aria-label（供屏幕阅读器朗读周视图任务行） */
function getTaskAriaLabel(t: CalendarTask): string {
  const parts = [
    t.name,
    `状态：${STATUS_META[t.status].label}`,
    `目标：${t.goal_name}`,
    `进度：${t.actual_qty}/${t.plan_qty}${t.unit}`,
  ];
  if (t.is_overdue) parts.push("已逾期");
  if (t.is_blocked) {
    parts.push(
      t.blocked_by_names
        ? `前置未完成：${t.blocked_by_names}`
        : "前置任务未完成",
    );
  }
  return parts.join("，");
}
</script>

<template>
  <div class="space-y-4 calendar-root">
    <!-- 顶部工具栏 -->
    <NCard :bordered="false" size="small">
      <div class="flex items-center justify-between flex-wrap gap-2">
        <NSpace align="center">
          <NButton
            quaternary
            circle
            :disabled="loading"
            aria-label="上一个周期"
            @click="prev"
          >
            <template #icon><Icon icon="mdi:chevron-left" /></template>
          </NButton>
          <span
            class="text-lg font-semibold min-w-[180px] text-center"
            aria-live="polite"
          >
            {{ headerLabel }}
          </span>
          <NButton
            quaternary
            circle
            :disabled="loading"
            aria-label="下一个周期"
            @click="next"
          >
            <template #icon><Icon icon="mdi:chevron-right" /></template>
          </NButton>
          <NButton size="small" :disabled="loading" @click="goToday"
            >今天</NButton
          >
        </NSpace>
        <NRadioGroup
          v-model:value="viewMode"
          size="small"
          aria-label="视图模式切换"
        >
          <NRadioButton value="month">月</NRadioButton>
          <NRadioButton value="week">周</NRadioButton>
          <NRadioButton value="day">日</NRadioButton>
        </NRadioGroup>
      </div>
    </NCard>

    <!-- P1-3：周期统计概览 -->
    <NCard :bordered="false" size="small">
      <div
        class="flex items-center gap-6 flex-wrap"
        role="status"
        aria-live="polite"
        :aria-label="`${periodLabel}统计：完成率 ${periodStats.completionRate}%，已完成 ${periodStats.done}，待完成 ${periodStats.pending}，逾期 ${periodStats.overdue}`"
      >
        <div class="flex items-baseline gap-2">
          <span class="text-xs text-gray-500">{{ periodLabel }}</span>
          <span class="text-xl font-bold text-brand-600"
            >{{ periodStats.completionRate }}%</span
          >
          <span class="text-xs text-gray-400">完成率</span>
        </div>
        <div class="flex items-center gap-1.5">
          <Icon icon="mdi:check-circle" class="text-green-500" width="16" />
          <span class="text-sm font-semibold">{{ periodStats.done }}</span>
          <span class="text-xs text-gray-500">已完成</span>
        </div>
        <div class="flex items-center gap-1.5">
          <Icon icon="mdi:clock-outline" class="text-gray-400" width="16" />
          <span class="text-sm font-semibold">{{ periodStats.pending }}</span>
          <span class="text-xs text-gray-500">待完成</span>
        </div>
        <div v-if="periodStats.overdue > 0" class="flex items-center gap-1.5">
          <Icon icon="mdi:alert-circle" class="text-red-500" width="16" />
          <span class="text-sm font-semibold text-red-500">{{
            periodStats.overdue
          }}</span>
          <span class="text-xs text-gray-500">逾期</span>
        </div>
        <div v-if="periodStats.skipped > 0" class="flex items-center gap-1.5">
          <Icon
            icon="mdi:skip-next-circle-outline"
            class="text-gray-400"
            width="16"
          />
          <span class="text-sm font-semibold text-gray-400">{{
            periodStats.skipped
          }}</span>
          <span class="text-xs text-gray-500">已跳过</span>
        </div>
      </div>
    </NCard>

    <!-- P2-1：可折叠筛选栏 -->
    <NCard :bordered="false" size="small" class="filter-card">
      <!-- 标题行（始终显示）：折叠按钮 + 标题 + 激活徽标 + 焦点统计 -->
      <div class="flex items-center gap-2">
        <button
          type="button"
          class="filter-toggle-btn"
          :aria-expanded="!filterCollapsed"
          aria-controls="calendar-filter-content"
          @click="toggleFilterCollapsed"
        >
          <Icon
            icon="mdi:chevron-right"
            width="18"
            class="filter-toggle-icon"
            :class="{ 'rotate-90': !filterCollapsed }"
            aria-hidden="true"
          />
          <Icon
            icon="mdi:filter-variant"
            class="text-gray-400"
            width="16"
            aria-hidden="true"
          />
          <span class="text-sm font-medium text-gray-700">筛选</span>
          <!-- 激活条件数徽标（折叠态有条件时显示） -->
          <span
            v-if="filterCollapsed && filterActiveCount > 0"
            class="filter-badge"
            role="status"
            aria-label="已应用筛选条件数"
          >
            {{ filterActiveCount }}
          </span>
          <!-- 折叠态：显示筛选后数量提示 -->
          <span
            v-if="filterCollapsed && filterHasCondition"
            class="text-xs text-gray-400 ml-1"
          >
            筛选后 {{ filteredTasks.length }} / {{ tasks.length }} 项
          </span>
          <span
            v-if="filterCollapsed"
            class="text-xs text-gray-400 ml-auto"
            aria-hidden="true"
          >
            点击展开
          </span>
        </button>
      </div>

      <!-- 展开内容区（过渡动画） -->
      <Transition name="filter-expand">
        <div
          v-show="!filterCollapsed"
          id="calendar-filter-content"
          ref="filterContentRef"
          class="filter-content"
          role="region"
          aria-label="筛选选项"
        >
          <div
            class="flex items-center gap-3 flex-wrap pt-3 mt-1 border-t border-gray-100"
          >
            <div class="filter-field">
              <label class="filter-label" for="filter-goal-select">目标</label>
              <NSelect
                v-model:value="filterGoalIds"
                multiple
                :options="goalOptions"
                placeholder="选择目标"
                size="small"
                clearable
                max-tag-count="responsive"
                input-id="filter-goal-select"
                style="min-width: 180px"
              />
            </div>
            <div class="filter-field">
              <label class="filter-label" for="filter-status-select"
                >状态</label
              >
              <NSelect
                v-model:value="filterStatuses"
                multiple
                :options="statusOptions"
                placeholder="选择状态"
                size="small"
                clearable
                max-tag-count="responsive"
                input-id="filter-status-select"
                style="min-width: 160px"
              />
            </div>
            <div class="filter-field">
              <label class="filter-label" for="filter-overdue-switch"
                >仅逾期</label
              >
              <NSwitch
                v-model:value="filterOverdueOnly"
                size="small"
                input-id="filter-overdue-switch"
              />
            </div>
            <NButton
              v-if="filterHasCondition"
              size="small"
              quaternary
              @click="resetFilter"
            >
              <template #icon><Icon icon="mdi:close" /></template>
              重置
            </NButton>
            <span
              v-if="filterHasCondition"
              class="text-xs text-gray-400 ml-auto self-center"
              role="status"
              aria-live="polite"
            >
              筛选后 {{ filteredTasks.length }} / {{ tasks.length }} 项
            </span>
          </div>
        </div>
      </Transition>
    </NCard>

    <!-- 月视图 -->
    <NCard v-if="viewMode === 'month'" :bordered="false">
      <NSpin :show="loading">
        <div
          class="grid grid-cols-7 gap-1 text-center text-xs text-gray-500 mb-1"
          role="presentation"
        >
          <div
            v-for="d in weekDays"
            :key="d"
            class="flex items-center justify-center py-1"
          >
            {{ d }}
          </div>
        </div>
        <div
          class="grid grid-cols-7 gap-1"
          role="grid"
          aria-label="月视图日期网格"
        >
          <NPopover
            v-for="day in monthGrid"
            :key="day.toISOString()"
            trigger="hover"
            :delay="300"
            placement="bottom"
            :width="280"
            :disabled="getTasksOfDay(day).length === 0"
          >
            <template #trigger>
              <div
                :ref="(el) => setMonthCellRef(el, day)"
                class="calendar-cell relative flex flex-col items-center justify-center min-h-[88px] p-1.5 rounded border cursor-pointer transition-all duration-200"
                role="button"
                :tabindex="isMonthCellFocusable(day) ? 0 : -1"
                :aria-label="getDayAriaLabel(day)"
                :aria-current="isToday(day) ? 'date' : undefined"
                :class="{
                  'bg-gray-50': !isSameMonth(day, currentDate) && !isToday(day),
                  'bg-brand-100 border-brand-500 border-2 shadow-md ring-2 ring-brand-300 ring-offset-1':
                    isToday(day),
                  'ring-2 ring-brand-400 ring-offset-1 bg-blue-50':
                    isMonthCellFocusable(day) && !isToday(day),
                  'hover:bg-blue-50':
                    !isToday(day) && !isMonthCellFocusable(day),
                  'hover:bg-brand-200': isToday(day),
                  // P2-5：负载色阶（仅当月且非今天、非聚焦时生效）
                  'bg-green-50':
                    isSameMonth(day, currentDate) &&
                    !isToday(day) &&
                    !isMonthCellFocusable(day) &&
                    getLoadLevel(day) === 'low',
                  'bg-yellow-50':
                    isSameMonth(day, currentDate) &&
                    !isToday(day) &&
                    !isMonthCellFocusable(day) &&
                    getLoadLevel(day) === 'medium',
                  'bg-red-50':
                    isSameMonth(day, currentDate) &&
                    !isToday(day) &&
                    !isMonthCellFocusable(day) &&
                    getLoadLevel(day) === 'high',
                  'bg-purple-50':
                    isSameMonth(day, currentDate) &&
                    !isToday(day) &&
                    !isMonthCellFocusable(day) &&
                    getLoadLevel(day) === 'extreme',
                }"
                @click="selectDay(day)"
                @keydown="onMonthCellKeydown($event)"
              >
                <div
                  class="text-center text-sm font-semibold"
                  :class="{
                    'text-brand-600 font-bold text-base': isToday(day),
                    'text-gray-400':
                      !isSameMonth(day, currentDate) && !isToday(day),
                  }"
                >
                  {{ format(day, "d") }}
                </div>
                <!-- P2-5+：圆环仪表盘负载指示器（方案 D） -->
                <div
                  v-if="getLoadLevel(day) !== 'none'"
                  class="load-ring absolute top-1 right-1 w-[26px] h-[26px]"
                  :title="`当日负载: ${getLoadOfDay(day)}`"
                  :aria-label="`当日负载等级: ${getLoadLevel(day)}, ${getLoadOfDay(day)}`"
                  role="img"
                >
                  <svg
                    width="26"
                    height="26"
                    viewBox="0 0 26 26"
                    style="transform: rotate(-90deg); transform-origin: center"
                    aria-hidden="true"
                  >
                    <!-- 轨道 -->
                    <circle
                      cx="13"
                      cy="13"
                      r="10"
                      fill="none"
                      stroke="#f3f4f6"
                      stroke-width="3"
                    />
                    <!-- 填充弧 -->
                    <circle
                      cx="13"
                      cy="13"
                      r="10"
                      fill="none"
                      :stroke="getLoadColor(day)"
                      stroke-width="3"
                      stroke-linecap="round"
                      :stroke-dasharray="getRingDashArray(day)"
                    />
                  </svg>
                  <span
                    class="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 text-[10px] font-bold leading-none tabular-nums"
                    :style="{ color: getLoadColor(day) }"
                    >{{ getLoadCount(day) }}</span
                  >
                </div>
                <div
                  v-if="getDayStatsCached(day).total > 0"
                  class="mt-1 space-y-0.5"
                >
                  <div class="flex items-center gap-1 text-[10px]">
                    <NTag size="tiny" :bordered="false" type="success" round
                      >{{ getDayStatsCached(day).done }}/{{
                        getDayStatsCached(day).total
                      }}</NTag
                    >
                    <NTag
                      v-if="getDayStatsCached(day).overdue > 0"
                      size="tiny"
                      :bordered="false"
                      type="error"
                      round
                      >{{ getDayStatsCached(day).overdue }}逾期</NTag
                    >
                  </div>
                  <!-- 任务点（视觉）+ sr-only 文本（屏幕阅读器） -->
                  <div class="flex flex-wrap gap-0.5" aria-hidden="true">
                    <div
                      v-for="t in getTasksOfDay(day).slice(0, 4)"
                      :key="t.id"
                      class="w-1.5 h-1.5 rounded-full"
                      :class="{ 'opacity-50': t.status === 'skipped' }"
                      :style="{ backgroundColor: STATUS_META[t.status].color }"
                      :title="t.name"
                    />
                    <span
                      v-if="getTasksOfDay(day).length > 4"
                      class="text-[9px] text-gray-400"
                      >+{{ getTasksOfDay(day).length - 4 }}</span
                    >
                  </div>
                </div>
              </div>
            </template>
            <!-- hover/click 预览弹层 -->
            <div class="space-y-1" role="list" aria-label="当日任务预览">
              <!-- P2-5：负载概要 -->
              <div
                class="text-xs text-gray-500 mb-1 flex items-center gap-2 pb-1 border-b border-gray-100"
              >
                <Icon icon="mdi:speedometer" width="12" />
                负载：{{ getLoadOfDay(day) }}
              </div>
              <div
                v-for="t in getTasksOfDay(day).slice(0, 8)"
                :key="t.id"
                class="flex items-center gap-2 text-xs py-0.5"
                role="listitem"
              >
                <Icon
                  :icon="STATUS_META[t.status].icon"
                  :color="STATUS_META[t.status].color"
                  width="14"
                />
                <span
                  class="flex-1 truncate"
                  :class="{
                    'line-through text-gray-400':
                      t.status === 'done' || t.status === 'skipped',
                  }"
                  >{{ t.name }}</span
                >
                <span class="text-[10px] text-gray-400 shrink-0">{{
                  t.goal_name
                }}</span>
              </div>
              <NButton
                v-if="getTasksOfDay(day).length > 8"
                size="tiny"
                block
                @click="selectDay(day)"
              >
                查看全部 ({{ getTasksOfDay(day).length }})
              </NButton>
            </div>
          </NPopover>
        </div>
      </NSpin>
    </NCard>

    <!-- 周视图 -->
    <NCard
      v-else-if="viewMode === 'week'"
      :bordered="false"
      role="region"
      aria-label="周视图"
      tabindex="-1"
    >
      <!-- P1-2：周视图批量操作工具栏 -->
      <div
        v-if="weekHasTasks"
        class="flex items-center justify-between mb-3 pb-2 border-b border-gray-100"
      >
        <span class="text-xs text-gray-500">点击任务前框选以批量操作</span>
        <NSpace :size="4">
          <NButton size="small" @click="selectAllVisibleWeek">全选本周</NButton>
          <NButton
            v-if="selectedTaskIds.size > 0"
            size="small"
            @click="clearSelection"
            >清空</NButton
          >
          <span
            v-if="selectedTaskIds.size > 0"
            class="text-xs text-gray-500 self-center"
            role="status"
            aria-live="polite"
            >已选 {{ selectedTaskIds.size }} 项</span
          >
          <NButton
            size="small"
            type="primary"
            :disabled="selectedTaskIds.size === 0"
            @click="batchComplete"
          >
            <template #icon><Icon icon="mdi:playlist-check" /></template>
            批量完成
          </NButton>
          <NButton
            size="small"
            type="warning"
            :disabled="selectedTaskIds.size === 0"
            @click="batchSkip"
          >
            <template #icon><Icon icon="mdi:skip-next" /></template>
            批量跳过
          </NButton>
        </NSpace>
      </div>
      <NSpin :show="loading">
        <div
          class="grid grid-cols-7 gap-2"
          role="grid"
          aria-label="周视图日期网格"
        >
          <div
            v-for="day in weekGrid"
            :key="day.toISOString()"
            class="min-h-[280px] p-2 rounded border flex flex-col transition-all duration-200"
            role="gridcell"
            :aria-label="getDayAriaLabel(day)"
            :class="{
              'border-brand-500 border-2 bg-brand-100/70 shadow-md ring-1 ring-brand-300':
                isToday(day),
              'bg-blue-50': isSameDay(day, selectedDate) && !isToday(day),
            }"
          >
            <!-- 列头：日期 + 逾期标记 -->
            <div
              class="flex items-center justify-center gap-1.5 text-center text-sm font-medium pb-1.5 border-b"
              :class="{ 'text-brand-600 font-bold': isToday(day) }"
            >
              <span>{{ format(day, "E d", { locale: zhCN }) }}</span>
              <NTag
                v-if="getDayStatsCached(day).overdue > 0"
                size="tiny"
                type="error"
                :bordered="false"
                round
                >{{ getDayStatsCached(day).overdue }}逾期</NTag
              >
            </div>
            <!-- 任务列表 -->
            <div
              class="mt-1 space-y-1 flex-1 overflow-auto max-h-[220px]"
              role="list"
              :aria-label="`${format(day, 'M月d日', { locale: zhCN })}任务列表`"
            >
              <NPopover
                v-for="t in getTasksOfDay(day)"
                :key="t.id"
                trigger="hover"
                :delay="300"
                placement="right"
                :width="240"
              >
                <template #trigger>
                  <div
                    class="text-xs p-1 rounded flex items-center gap-1 cursor-default"
                    role="listitem"
                    :aria-label="getTaskAriaLabel(t)"
                    :class="{
                      'bg-red-50': t.is_overdue,
                      'bg-green-50': t.status === 'done',
                      'opacity-60': t.status === 'skipped',
                    }"
                  >
                    <NCheckbox
                      v-if="t.status !== 'done' && t.status !== 'skipped'"
                      :checked="selectedTaskIds.has(t.id)"
                      :disabled="t.is_blocked"
                      :aria-label="`选择任务：${t.name}`"
                      @update:checked="(v) => toggleSelect(t.id, v)"
                      @click.stop
                    />
                    <Icon
                      :icon="STATUS_META[t.status].icon"
                      :color="STATUS_META[t.status].color"
                      width="12"
                      aria-hidden="true"
                    />
                    <span
                      class="flex-1 truncate"
                      :class="{
                        'line-through':
                          t.status === 'done' || t.status === 'skipped',
                      }"
                      >{{ t.name }}</span
                    >
                  </div>
                </template>
                <!-- 任务详情弹层（hover 触发，键盘用户通过 aria-label 获取等价信息） -->
                <div class="space-y-1 text-xs">
                  <div class="font-medium text-sm">{{ t.name }}</div>
                  <div class="flex items-center gap-2 text-gray-500">
                    <Icon
                      :icon="STATUS_META[t.status].icon"
                      :color="STATUS_META[t.status].color"
                      width="14"
                    />
                    <span>{{ STATUS_META[t.status].label }}</span>
                    <NTag size="tiny" :bordered="false" type="info">{{
                      t.goal_name
                    }}</NTag>
                  </div>
                  <div class="text-gray-500">
                    进度：{{ t.actual_qty }}/{{ t.plan_qty }}{{ t.unit }}
                  </div>
                  <div
                    v-if="t.is_overdue"
                    class="text-red-500 flex items-center gap-1"
                  >
                    <Icon icon="mdi:alert-circle" width="14" />已逾期
                  </div>
                  <div
                    v-if="t.is_blocked"
                    class="text-gray-500 flex items-center gap-1"
                  >
                    <Icon icon="mdi:lock-outline" width="14" />
                    <span>{{
                      t.blocked_by_names
                        ? `前置未完成：${t.blocked_by_names}`
                        : "前置任务未完成"
                    }}</span>
                  </div>
                </div>
              </NPopover>
              <div
                v-if="getTasksOfDay(day).length === 0"
                class="flex items-center justify-center text-[10px] text-gray-300 pt-2"
              >
                无
              </div>
            </div>
            <!-- P1-2：列底部完成率进度条 -->
            <div
              v-if="getDayStatsCached(day).total > 0"
              class="mt-1 pt-1 border-t border-gray-100"
            >
              <div
                class="flex items-center justify-between text-[10px] text-gray-500 mb-0.5"
              >
                <span>完成率</span>
                <span>{{ getDayCompletionRate(day) }}%</span>
              </div>
              <NProgress
                type="line"
                :percentage="getDayCompletionRate(day)"
                :show-indicator="false"
                size="small"
                :color="
                  getDayCompletionRate(day) === 100 ? '#67c23a' : '#3478f6'
                "
                aria-hidden="true"
              />
            </div>
          </div>
        </div>
      </NSpin>
    </NCard>

    <!-- 日视图 -->
    <div
      v-else
      class="space-y-3"
      role="region"
      aria-label="日视图"
      tabindex="-1"
    >
      <NCard :bordered="false">
        <template #header>
          <div class="flex items-center gap-2">
            <Icon
              icon="mdi:calendar-today"
              width="20"
              class="text-brand-500"
              aria-hidden="true"
            />
            <span>{{
              format(selectedDate, "yyyy-MM-dd EEEE", { locale: zhCN })
            }}</span>
            <NTag v-if="isToday(selectedDate)" type="info" size="small" round
              >今天</NTag
            >
          </div>
        </template>
        <template #header-extra>
          <NSpace v-if="selectedDayTasks.length > 0" :size="4">
            <NButton size="small" @click="selectAllVisible">全选</NButton>
            <NButton size="small" @click="clearSelection">清空</NButton>
            <span
              v-if="selectedTaskIds.size > 0"
              class="text-xs text-gray-500 self-center"
              role="status"
              aria-live="polite"
              >已选 {{ selectedTaskIds.size }} 项</span
            >
            <NButton
              size="small"
              type="primary"
              :disabled="selectedTaskIds.size === 0"
              @click="batchComplete"
            >
              <template #icon><Icon icon="mdi:playlist-check" /></template>
              批量完成
            </NButton>
            <NButton
              size="small"
              type="warning"
              :disabled="selectedTaskIds.size === 0"
              @click="batchSkip"
            >
              <template #icon><Icon icon="mdi:skip-next" /></template>
              批量跳过
            </NButton>
          </NSpace>
        </template>

        <NSpin :show="loading">
          <div
            v-if="selectedDayTasks.length > 0"
            class="space-y-1"
            role="list"
            aria-label="当日任务列表"
          >
            <div
              v-for="t in selectedDayTasks"
              :key="t.id"
              class="flex items-center gap-2 px-3 py-2 rounded hover:bg-gray-50"
              role="listitem"
              :aria-label="getTaskAriaLabel(t)"
              :class="{
                'bg-red-50': t.is_overdue,
              }"
            >
              <NCheckbox
                v-if="t.status !== 'done' && t.status !== 'skipped'"
                :checked="selectedTaskIds.has(t.id)"
                :disabled="t.is_blocked"
                :aria-label="`选择任务：${t.name}`"
                @update:checked="(v) => toggleSelect(t.id, v)"
              />
              <div
                class="flex-1 flex items-center gap-2 min-w-0"
                :class="{ 'opacity-40': t.is_blocked }"
              >
                <Icon
                  :icon="STATUS_META[t.status].icon"
                  :color="STATUS_META[t.status].color"
                  width="18"
                  aria-hidden="true"
                />
                <div class="flex-1 min-w-0">
                  <div class="flex items-center gap-2">
                    <NTooltip v-if="t.is_blocked" placement="top">
                      <template #trigger>
                        <Icon
                          icon="mdi:lock-outline"
                          class="text-gray-400 shrink-0"
                          width="14"
                          aria-hidden="true"
                          role="img"
                          :aria-label="
                            t.blocked_by_names
                              ? `前置任务未完成：${t.blocked_by_names}`
                              : '前置任务未完成，暂不可标记完成'
                          "
                        />
                      </template>
                      {{
                        t.blocked_by_names
                          ? `前置任务未完成：${t.blocked_by_names}`
                          : "前置任务未完成，暂不可标记完成"
                      }}
                    </NTooltip>
                    <span
                      class="text-sm font-medium truncate"
                      :class="{
                        'line-through text-gray-400': t.status === 'done',
                      }"
                      >{{ t.name }}</span
                    >
                    <NTag size="tiny" :bordered="false" type="info">{{
                      t.goal_name
                    }}</NTag>
                    <NTag
                      v-if="t.is_overdue"
                      size="tiny"
                      type="error"
                      :bordered="false"
                      >逾期</NTag
                    >
                  </div>
                  <div class="text-xs text-gray-500 mt-0.5">
                    {{ t.actual_qty }}/{{ t.plan_qty }}{{ t.unit }}
                  </div>
                </div>
              </div>
              <NSpace
                v-if="t.status !== 'done' && t.status !== 'skipped'"
                :size="4"
              >
                <NTooltip :disabled="!t.is_blocked" placement="top">
                  <template #trigger>
                    <NButton
                      size="tiny"
                      type="primary"
                      :disabled="t.is_blocked"
                      :aria-label="`完成任务：${t.name}`"
                      @click="quickComplete(t)"
                    >
                      <template #icon>
                        <Icon icon="mdi:check" width="16" />
                      </template>
                      完成
                    </NButton>
                  </template>
                  {{
                    t.blocked_by_names
                      ? `前置任务未完成：${t.blocked_by_names}`
                      : "前置任务未完成"
                  }}
                </NTooltip>
                <NButton
                  size="tiny"
                  type="default"
                  :aria-label="`跳过任务：${t.name}`"
                  @click="quickSkip(t)"
                >
                  <template #icon>
                    <Icon icon="mdi:skip-next" width="16" />
                  </template>
                  跳过
                </NButton>
              </NSpace>
              <NTag
                v-else-if="t.status === 'done'"
                size="tiny"
                type="success"
                :bordered="false"
                >已完成</NTag
              >
              <NTag v-else size="tiny" type="default" :bordered="false"
                >已跳过</NTag
              >
            </div>
          </div>
          <NEmpty v-else description="当日无任务" />
        </NSpin>
      </NCard>
    </div>

    <!-- 浮动键盘快捷键帮助按钮（右下角） -->
    <NPopover trigger="click" placement="top-end" :width="240">
      <template #trigger>
        <button
          type="button"
          class="keyboard-help-fab"
          aria-label="键盘快捷键说明"
        >
          <Icon icon="mdi:keyboard-outline" width="20" aria-hidden="true" />
        </button>
      </template>
      <div class="text-xs space-y-1">
        <div class="font-medium text-sm pb-1 border-b border-gray-100 mb-1">
          键盘快捷键
        </div>
        <div class="flex items-center justify-between">
          <span>切换视图</span>
          <span>
            <kbd class="kbd">1</kbd>
            <kbd class="kbd">2</kbd>
            <kbd class="kbd">3</kbd>
          </span>
        </div>
        <div class="flex items-center justify-between">
          <span class="text-gray-500 text-[10px] pl-2">月 / 周 / 日</span>
          <span></span>
        </div>
        <div class="flex items-center justify-between">
          <span>切换日期</span>
          <span>
            <kbd class="kbd">←</kbd>
            <kbd class="kbd">→</kbd>
          </span>
        </div>
        <div class="flex items-center justify-between">
          <span>月视图移动焦点</span>
          <span>
            <kbd class="kbd">↑</kbd>
            <kbd class="kbd">↓</kbd>
          </span>
        </div>
        <div class="flex items-center justify-between">
          <span>返回上一视图</span>
          <kbd class="kbd">Esc</kbd>
        </div>
        <div class="flex items-center justify-between">
          <span>全选当日任务</span>
          <span><kbd class="kbd">Ctrl</kbd> + <kbd class="kbd">A</kbd></span>
        </div>
      </div>
    </NPopover>
  </div>
</template>

<style scoped>
.calendar-cell:hover {
  transform: translateY(-1px);
}
.calendar-cell:focus-visible {
  outline: 2px solid #3478f6;
  outline-offset: 2px;
}
.calendar-root {
  outline: none;
}

/* ===== 可折叠筛选栏 ===== */
.filter-card :deep(.n-card__content) {
  padding-bottom: 6px;
}

/* ===== 浮动键盘快捷键帮助按钮 ===== */
.keyboard-help-fab {
  position: fixed;
  right: 20px;
  bottom: 20px;
  z-index: 100;
  width: 40px;
  height: 40px;
  border-radius: 50%;
  border: 1px solid rgba(0, 0, 0, 0.08);
  background-color: #fff;
  color: #6b7280;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.12);
  transition: all 0.2s ease;
}
.keyboard-help-fab:hover {
  background-color: #3478f6;
  color: #fff;
  transform: translateY(-1px);
  box-shadow: 0 4px 12px rgba(52, 120, 246, 0.3);
}
.keyboard-help-fab:focus-visible {
  outline: 2px solid #3478f6;
  outline-offset: 2px;
}

/* kbd 按键标签 */
.kbd {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 20px;
  height: 18px;
  padding: 0 4px;
  margin-left: 2px;
  border-radius: 3px;
  border: 1px solid #d1d5db;
  border-bottom-width: 2px;
  background-color: #f9fafb;
  color: #374151;
  font-family: ui-monospace, monospace;
  font-size: 11px;
  line-height: 1;
}

/* 折叠/展开按钮 */
.filter-toggle-btn {
  display: flex;
  align-items: center;
  gap: 0.375rem;
  width: 100%;
  padding: 4px 6px;
  border: 0;
  background: transparent;
  border-radius: 4px;
  cursor: pointer;
  color: inherit;
  font: inherit;
  text-align: left;
  transition: background-color 0.2s ease;
}
.filter-toggle-btn:hover {
  background-color: rgba(52, 120, 246, 0.06);
}
.filter-toggle-btn:focus-visible {
  outline: 2px solid #3478f6;
  outline-offset: 2px;
}

.filter-toggle-icon {
  transition: transform 0.3s ease;
  color: #6b7280;
  flex-shrink: 0;
}
.filter-toggle-icon.rotate-90 {
  transform: rotate(90deg);
}

/* 激活条件数徽标 */
.filter-badge {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 18px;
  height: 18px;
  padding: 0 4px;
  border-radius: 9px;
  background-color: #3478f6;
  color: #fff;
  font-size: 11px;
  font-weight: 600;
  line-height: 1;
  margin-left: 2px;
}

/* 展开内容区 */
.filter-content {
  overflow: hidden;
}

/* 筛选字段布局 */
.filter-field {
  display: flex;
  align-items: center;
  gap: 0.375rem;
}
.filter-label {
  font-size: 12px;
  color: #6b7280;
  white-space: nowrap;
}

/* 过渡动画：使用 grid-template-rows 技巧实现高度自适应的平滑展开 */
.filter-expand-enter-active,
.filter-expand-leave-active {
  transition:
    opacity 0.3s ease,
    transform 0.3s ease;
  max-height: 300px;
  overflow: hidden;
}
.filter-expand-enter-from,
.filter-expand-leave-to {
  opacity: 0;
  transform: translateY(-4px);
  max-height: 0;
}

/* 响应式：小屏幕换行 + 字段间距收紧 */
@media (max-width: 640px) {
  .filter-toggle-btn {
    flex-wrap: wrap;
    gap: 0.25rem;
  }
  .filter-field {
    width: 100%;
  }
  .filter-field :deep(.n-base-selection) {
    flex: 1;
    min-width: 0 !important;
  }
  .filter-field .filter-label {
    width: 40px;
    flex-shrink: 0;
  }
}
</style>
