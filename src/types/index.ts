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
  /** P1-3：每日可用时长（按时间预算拆解时使用，null=未设置） */
  daily_capacity: number | null;
}

export interface CreateGoalInput {
  name: string;
  /** 父目标 ID（不传=总目标） */
  parent_id?: string | null;
  deadline?: string | null;
  total_qty?: number;
  unit?: string;
  /** P1-3：每日可用时长（按时间预算拆解时使用） */
  daily_capacity?: number | null;
}

export interface UpdateGoalInput {
  id: string;
  name?: string;
  deadline?: string | null;
  total_qty?: number;
  unit?: string;
  /** P1-3：每日可用时长（按时间预算拆解时使用） */
  daily_capacity?: number | null;
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
  /** 频率：daily | weekly | monthly（不传或 daily → 每天重复） */
  frequency?: string;
  /** 周几（0=周日, 1-6=周一至周六），仅 weekly 有效 */
  weekdays?: number[];
  /** 每月几号（1-31），仅 monthly 有效 */
  month_days?: number[];
}

/** 智能拆解策略 */
export type SplitStrategy = "by_deadline" | "by_capacity" | "by_date_range";

/** 智能拆解输入（整合视频拆解与时间预算拆解的统一入口） */
export interface SmartSplitInput {
  goal_id: string;
  /** 拆解策略 */
  strategy: SplitStrategy;
  /** 总量（可选，默认用 goal.total_qty） */
  total_qty?: number | null;
  /** 截止日期 yyyy-MM-dd（by_deadline / by_capacity 用，默认用 goal.deadline） */
  deadline?: string | null;
  /** 每日可用时长（by_capacity 必填） */
  daily_capacity?: number | null;
  /** 起始日期 yyyy-MM-dd（by_date_range 用，默认明天） */
  start_date?: string | null;
  /** 结束日期 yyyy-MM-dd（by_date_range 用） */
  end_date?: string | null;
  /** 每日数量（by_date_range 可选；不填则按天数均分总量） */
  per_day_qty?: number | null;
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
  /** 精确逾期日期（yyyy-MM-dd），逾期任务才有值 */
  overdue_date: string | null;
  plan_qty: number;
  actual_qty: number;
  unit: string;
  status: TaskStatus;
  is_manual: number;
  source: "auto" | "manual";
  sort_order: number;
  created_at: string;
  /** P1-3：预估时长（小时），按时间预算拆解时自动填充 */
  estimated_hours: number | null;
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
  /** 精确逾期日期（yyyy-MM-dd），逾期任务才有值 */
  overdue_date: string | null;
  plan_qty: number;
  actual_qty: number;
  unit: string;
  status: TaskStatus;
  source: "auto" | "manual";
  /** 是否被依赖阻塞（存在未完成的前置依赖） */
  is_blocked: boolean;
  /** 阻塞本任务的前置任务名称列表（顿号分隔），仅 is_blocked=true 时有值 */
  blocked_by_names: string | null;
}

export type TaskStatus = "pending" | "partial" | "done" | "skipped";

export interface MoveTaskInput {
  task_id: string;
  /** 跨目标移动时指定新目标 ID（拖拽归属） */
  goal_id?: string;
  /** 阶段内移动时指定新阶段 ID */
  stage_id?: string | null;
  /** 插入到此任务之前（同级排序）；为 None 且跨目标时放置到目标直属任务列表最前面 */
  before_task_id?: string | null;
}

/** 移动目标输入（跨层级归属调整与同级排序） */
export interface MoveGoalInput {
  goal_id: string;
  /** 新父目标 ID（None=总目标） */
  new_parent_id: string | null;
  /** 插入到此目标之前（同级排序）；None=追加到末尾 */
  before_goal_id?: string | null;
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
    color: "#b0b3b8",
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
  /** 是否被依赖阻塞（存在未完成的前置依赖） */
  is_blocked: boolean;
  /** 阻塞本任务的前置任务名称列表（顿号分隔），仅 is_blocked=true 时有值 */
  blocked_by_names: string | null;
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
  | "celebration" // 庆祝（全部完成）
  | "setback"; // 挫折安抚（P1-2：连续中断/进度滞后）

/** 鼓励语 */
export interface Encouragement {
  id: string;
  text: string;
  /** preset 预设 | custom 用户自定义 */
  category: "preset" | "custom";
  /** 鼓励语等级 */
  level: EncouragementLevel;
  created_at: string;
  /** P2-1：情境标签（JSON 格式） */
  context_tags?: string;
  /** P2-5：隐藏标记（仅预设文案可隐藏） */
  hidden?: number;
  /** P3-1：权重（用于加权随机，默认1.0） */
  weight?: number;
  /** P3-5：排序（用于拖拽排序，默认0） */
  sort_order?: number;
}

/** 添加鼓励语输入 */
export interface AddEncouragementInput {
  text: string;
  /** 可选等级，默认 "normal" */
  level?: EncouragementLevel;
}

/** 鼓励语展示触发源（P0-4：展示历史与去重） */
export type EncouragementTriggerSource =
  | "complete_first" // 完成今日首任务（modal）
  | "complete_normal" // 完成非首任务（toast）
  | "complete_celebration" // 全部目标完成（celebration modal）
  | "dashboard_banner" // 进入仪表盘 banner
  | "streak_break" // P1-2：连续中断
  | "progress_lag" // P1-2：进度滞后
  | "goal_created" // P2-2：新建目标
  | "goal_midway" // P2-2：目标进度 50%
  | "task_skipped" // P2-2：跳过任务
  | "streak_recovery"; // P2-2：中断后恢复

/** 更新鼓励语输入（P0-5：补齐编辑功能，仅自定义文案可修改） */
export interface UpdateEncouragementInput {
  id: string;
  text?: string;
  level?: EncouragementLevel;
}

/** 鼓励语偏好设置（P1-4：用户偏好设置） */
export interface EncouragementSettings {
  /** 总开关 */
  enabled: boolean;
  /** 展示频率 */
  frequency: "aggressive" | "normal" | "sparse";
  /** 文案风格 */
  style: "warm" | "professional" | "minimal";
  /** 庆祝动画开关 */
  celebration_animation: boolean;
  /** emoji 显示开关 */
  emoji_enabled: boolean;
}

/** 更新鼓励语偏好设置输入（P1-4：所有字段可选） */
export interface UpdateEncouragementSettingsInput {
  enabled?: boolean;
  frequency?: "aggressive" | "normal" | "sparse";
  style?: "warm" | "professional" | "minimal";
  celebration_animation?: boolean;
  emoji_enabled?: boolean;
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
  /** P2-4：里程碑成就（none/expert/master） */
  milestone: string;
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

/** 滞后目标详情（P1-2：进度滞后检测） */
export interface LaggingGoal {
  id: string;
  name: string;
  deadline: string;
  predicted_end_date: string;
  /** 距截止日期剩余天数（负数=已逾期） */
  days_remaining: number;
}

/** 挫折场景检测结果（P1-2：挫折/安抚场景） */
export interface SetbackSituation {
  /** 是否发生连续中断 */
  has_streak_break: boolean;
  /** 中断前的连续天数 */
  streak_break_prev: number;
  /** 是否存在进度滞后 */
  has_progress_lag: boolean;
  /** 滞后目标列表 */
  lagging_goals: LaggingGoal[];
}

/** 庆祝成就数据（P1-3：celebration 仪式感增强） */
export interface CelebrationAchievement {
  /** 总目标数 */
  total_goals: number;
  /** 总任务数 */
  total_tasks: number;
  /** 已完成任务数 */
  completed_tasks: number;
  /** 本次冲刺耗时（天） */
  days_elapsed: number;
  /** 完成时的连续天数 */
  final_streak: number;
  /** 完成时最长连续天数 */
  final_longest_streak: number;
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

/** 单目标在某日的负载 — P2-5 */
export interface GoalLoad {
  goal_id: string;
  goal_name: string;
  task_count: number;
  total_qty: number;
}

/** 每日负载（按目标分组）— P2-5 跨目标负载平衡 */
export interface DailyLoad {
  /** 日期 yyyy-MM-dd */
  date: string;
  /** 当日任务总数（不含跳过） */
  total_tasks: number;
  /** 当日任务总量（plan_qty 之和，不含跳过） */
  total_qty: number;
  /** 按目标分组的负载明细 */
  by_goal: GoalLoad[];
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

/** 任务依赖关系（P1-1）：task_id 依赖 depends_on_id */
export interface TaskDependency {
  id: string;
  task_id: string;
  depends_on_id: string;
  created_at: string;
}

/** 设置任务依赖输入 */
export interface SetTaskDependencyInput {
  task_id: string;
  /** 前置任务 ID（task_id 依赖此任务） */
  depends_on_id: string;
}

/** 删除任务结果（P2-3：返回被删任务所属 goal_id，供前端局部更新进度） */
export interface DeleteTaskResult {
  task_id: string;
  goal_id: string;
}

/** 批量删除任务结果（P2-3：返回受影响的 goal_id 列表，供前端局部更新进度） */
export interface DeleteTasksBatchResult {
  deleted_count: number;
  /** 受影响的 goal_id 列表（去重） */
  affected_goal_ids: string[];
}

/** 用户收藏（P3-2） */
export interface UserFavorite {
  id: string;
  encouragement_id: string;
  created_at: string;
}

/** 鼓励语反馈（P3-3） */
export interface EncouragementFeedback {
  id: string;
  encouragement_id: string;
  /** like | dislike */
  feedback_type: "like" | "dislike";
  created_at: string;
}
