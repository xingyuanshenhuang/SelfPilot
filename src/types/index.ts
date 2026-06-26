/// 全局 TypeScript 类型定义（与 Rust 侧 models.rs 对应）

export interface Goal {
  id: string;
  name: string;
  /** 父目标 ID（null=总目标，否则=子目标） */
  parent_id: string | null;
  path: string;
  deadline: string | null;
  total_qty: number;
  unit: string;
  sort_order: number;
  created_at: string;
}

export interface CreateGoalInput {
  name: string;
  /** 父目标 ID（不传=总目标） */
  parent_id?: string | null;
  deadline?: string | null;
  total_qty?: number;
  unit?: string;
}

export interface UpdateGoalInput {
  id: string;
  name?: string;
  deadline?: string | null;
  total_qty?: number;
  unit?: string;
}

/** 重复拆解输入（纯文字类任务） */
export interface RepeatSplitInput {
  goal_id: string;
  name: string;
  /** 起始日期 yyyy-MM-dd */
  start_date: string;
  /** 结束日期 yyyy-MM-dd（不传或等于 start_date → 单次任务） */
  end_date?: string | null;
  plan_qty?: number;
  unit?: string;
}

/** 目标树节点 */
export interface GoalTreeNode {
  goal: Goal;
  sub_goals: GoalTreeNode[];
  tasks: Task[];
  progress: number;
  is_completed: boolean;
}

export interface Task {
  id: string;
  goal_id: string;
  stage_id: string | null;
  parent_id: string | null;
  path: string;
  name: string;
  plan_date: string | null;
  plan_qty: number;
  actual_qty: number;
  unit: string;
  status: TaskStatus;
  is_manual: number;
  source: "auto" | "manual";
  sort_order: number;
  created_at: string;
}

export interface CreateTaskInput {
  goal_id: string;
  stage_id?: string | null;
  name: string;
  plan_date?: string | null;
  plan_qty?: number;
  unit?: string;
}

export interface CompleteTaskInput {
  task_id: string;
  actual_qty: number;
}

/** 更新任务输入（通用：名称、计划日期、计划数量，均可选） */
export interface UpdateTaskInput {
  task_id: string;
  name?: string;
  plan_date?: string;
  plan_qty?: number;
}

export interface ProgressInfo {
  id: string;
  name: string;
  total_plan: number;
  total_actual: number;
  /** 完成百分比 0.0 ~ 1.0 */
  percentage: number;
  /** 是否完成（子目标全完成 + 直属子任务全完成） */
  is_completed: boolean;
}

export interface TodayTask {
  id: string;
  goal_id: string;
  goal_name: string;
  stage_id: string | null;
  name: string;
  plan_date: string | null;
  plan_qty: number;
  actual_qty: number;
  unit: string;
  status: TaskStatus;
  source: "auto" | "manual";
}

export type TaskStatus = "pending" | "partial" | "done" | "skipped";

export interface MoveTaskInput {
  task_id: string;
  stage_id: string | null;
}

/** 重新规划预览项 */
export interface ReplanPreviewItem {
  task_id: string;
  name: string;
  plan_date: string;
  old_plan_qty: number;
  new_plan_qty: number;
  /** 是否被保留（手动修改的任务） */
  retained: boolean;
}

/** 重新规划预览结果 */
export interface ReplanPreview {
  goal_id: string;
  goal_name: string;
  remaining_days: number;
  remaining_qty: number;
  daily_base: number;
  remainder: number;
  items: ReplanPreviewItem[];
}

/** 重新规划执行结果 */
export interface ReplanResult {
  goal_id: string;
  updated_count: number;
  retained_count: number;
  tasks: Task[];
}

/** 任务状态对应的显示信息 */
export const STATUS_META: Record<
  TaskStatus,
  { icon: string; label: string; color: string }
> = {
  pending: {
    icon: "mdi:checkbox-blank-outline",
    label: "未完成",
    color: "#909399",
  },
  partial: {
    icon: "mdi:checkbox-intermediate",
    label: "部分完成",
    color: "#e6a23c",
  },
  done: { icon: "mdi:check-circle", label: "已完成", color: "#67c23a" },
  skipped: {
    icon: "mdi:skip-next-circle-outline",
    label: "已跳过",
    color: "#909399",
  },
};

/** 日历视图任务（带目标名称和逾期标记） */
export interface CalendarTask {
  id: string;
  goal_id: string;
  goal_name: string;
  stage_id: string | null;
  name: string;
  plan_date: string | null;
  plan_qty: number;
  actual_qty: number;
  unit: string;
  status: TaskStatus;
  source: "auto" | "manual";
  /** 是否逾期（plan_date < today 且未完成） */
  is_overdue: boolean;
}

/** 每日完成趋势项 */
export interface DailyTrend {
  /** 日期 yyyy-MM-dd */
  date: string;
  /** 当日完成数量（数量型按 actual_qty 截断到 plan_qty，布尔型 done 计 1） */
  completed_qty: number;
  /** 当日 done 任务条数 */
  completed_count: number;
}

/** 目标完成统计（柱状图） */
export interface GoalCompletionStat {
  id: string;
  name: string;
  total_plan: number;
  total_actual: number;
  /** 完成百分比 0.0 ~ 1.0 */
  percentage: number;
  /** 任务总数（不含跳过） */
  task_count: number;
  /** 已完成任务数 */
  done_count: number;
}

/** 鼓励语等级 */
export type EncouragementLevel =
  | "normal" // 普通（1天）
  | "advanced" // 进阶（3天）
  | "highlight" // 高亮（7天）
  | "celebration"; // 庆祝（全部完成）

/** 鼓励语 */
export interface Encouragement {
  id: string;
  text: string;
  /** preset 预设 | custom 用户自定义 */
  category: "preset" | "custom";
  /** 鼓励语等级 */
  level: EncouragementLevel;
  created_at: string;
}

/** 添加鼓励语输入 */
export interface AddEncouragementInput {
  text: string;
  /** 可选等级，默认 "normal" */
  level?: EncouragementLevel;
}

/** 设置项（key-value） */
export interface Setting {
  key: string;
  value: string;
}

/** 设置项输入 */
export interface SetSettingInput {
  key: string;
  value: string;
}

/** 连续完成天数统计 */
export interface StreakInfo {
  /** 当前连续天数 */
  current_streak: number;
  /** 历史最长连续天数 */
  longest_streak: number;
  /** 今日是否已完成至少一个任务 */
  completed_today: boolean;
}

/** 导出数据（完整备份） */
export interface ExportData {
  version: string;
  exported_at: string;
  goals: Goal[];
  /** 已废弃，保留用于旧版备份兼容 */
  stages: unknown[];
  tasks: Task[];
  encouragements: Encouragement[];
  settings: Setting[];
}

/** 导入冲突处理模式 */
export type ImportConflictMode = "skip" | "overwrite" | "rename";

/** 导入数据输入 */
export interface ImportInput {
  data: string;
  conflict_mode: ImportConflictMode;
}

/** 导入数据结果 */
export interface ImportResult {
  goals_imported: number;
  goals_skipped: number;
  stages_imported: number;
  stages_skipped: number;
  tasks_imported: number;
  tasks_skipped: number;
  encouragements_imported: number;
  settings_imported: number;
}

/** 热力图单元格 */
export interface HeatmapCell {
  /** 日期 yyyy-MM-dd */
  date: string;
  /** 当日计划任务总量（不含跳过） */
  plan_qty: number;
  /** 当日完成量 */
  completed_qty: number;
  /** 当日任务总数（不含跳过） */
  task_count: number;
  /** 当日已完成任务数 */
  done_count: number;
  /** 完成率 0.0 ~ 1.0 */
  completion_rate: number;
}

/** 完成预测状态 */
export type PredictionStatus =
  | "on_track" // 按期完成
  | "ahead" // 可提前完成
  | "need_speed" // 需提高速度
  | "no_deadline" // 未设置截止日期
  | "no_data" // 无历史完成数据
  | "completed"; // 已全部完成

/** 完成预测 */
export interface CompletionPrediction {
  goal_id: string;
  goal_name: string;
  /** 截止日期（可能为空） */
  deadline: string | null;
  /** 目标总量 */
  total_qty: number;
  /** 已完成量 */
  completed_qty: number;
  /** 剩余量 */
  remaining_qty: number;
  /** 过去7天平均每日完成量 */
  avg_daily_speed: number;
  /** 预测还需天数（无数据时为 null） */
  predicted_days: number | null;
  /** 预测完成日期（无数据时为 null） */
  predicted_date: string | null;
  /** 距截止日期剩余天数（负数表示已逾期） */
  days_to_deadline: number | null;
  /** 预测状态 */
  status: PredictionStatus;
  /** 建议文案 */
  suggestion: string;
}
