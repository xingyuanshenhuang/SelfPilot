/// 全局 TypeScript 类型定义（与 Rust 侧 models.rs 对应）

export interface Goal {
  id: string;
  name: string;
  /** 目标描述：详细内容、要求或达成标准（null=未设置） */
  description: string | null;
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
  /** 目标优先级权重（A5 跨目标公平分摊，默认 1.0） */
  priority: number;
  /** 是否允许系统事件自动触发重排（1=允许，0=关闭） */
  auto_replan: number;
  /** 是否允许无需确认直接应用（1=开启，0=关闭） */
  auto_apply: number;
  /** 风险标记：'at_risk' = 用户选择"接受风险"（null=无） */
  risk_flag: string | null;
}

/** 目标级自动化规划开关（goals.auto_replan / auto_apply） */
export interface GoalAutomationPolicy {
  goal_id: string;
  auto_replan: number;
  auto_apply: number;
}

export interface UpdateGoalAutomationInput {
  goal_id: string;
  /** 不传=不修改 */
  auto_replan?: number | null;
  /** 不传=不修改 */
  auto_apply?: number | null;
}

export interface CreateGoalInput {
  name: string;
  /** 目标描述：详细内容、要求或达成标准 */
  description?: string | null;
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
  /** 目标描述：不传=不修改，传空串=清除 */
  description?: string | null;
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
  /** 任务描述（可选，应用于所有生成的实例） */
  description?: string | null;
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
  /** 任务描述：详细内容或要求（null=未设置） */
  description: string | null;
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
  /** 所属重复任务系列 id（"添加任务-重复任务"生成的实例；普通任务为 null） */
  repeat_series_id: string | null;
}

/** 重复任务系列（原任务）：保存"添加任务-重复任务"的原任务设置 */
export interface RepeatSeries {
  id: string;
  goal_id: string;
  /** 基础名称（不含" - 第N次"后缀） */
  base_name: string;
  /** 系列描述（应用于所有实例） */
  description: string | null;
  plan_qty: number;
  unit: string;
  /** 频率：daily | weekly | monthly */
  frequency: "daily" | "weekly" | "monthly";
  /** 起始日期 yyyy-MM-dd */
  start_date: string;
  /** 结束日期 yyyy-MM-dd */
  end_date: string | null;
  /** 周几（0=周日, 1-6=周一至周六） */
  weekdays: number[];
  /** 每月几号（1-31） */
  month_days: number[];
  created_at: string;
}

/** 更新重复任务系列输入（编辑原任务） */
export interface UpdateRepeatSeriesInput {
  series_id: string;
  base_name: string;
  plan_qty?: number;
  unit?: string;
  frequency?: string;
  start_date?: string;
  end_date?: string | null;
  weekdays?: number[];
  month_days?: number[];
  /** 任务描述：不传=保持现值，传空串=清除 */
  description?: string | null;
}

/** 更新重复任务系列结果 */
export interface RepeatSeriesUpdateResult {
  series: RepeatSeries;
  goal_id: string;
}

export interface CreateTaskInput {
  goal_id: string;
  /** 任务描述：详细内容或要求 */
  description?: string | null;
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
  /** 任务描述：不传=不修改，传空串=清除 */
  description?: string | null;
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

/** Naive UI 标签/按钮类型（统一入口，避免各视图重复定义） */
export type TagType =
  | "default"
  | "success"
  | "error"
  | "warning"
  | "info"
  | "primary";

/** 日历视图模式 */
export type ViewMode = "month" | "week" | "day";

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
  /** P3-5：自定义排序顺序 */
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
  | "streak_recovery" // P2-2：中断后恢复
  | "app_first_open"; // P2-2：每日首次打开应用

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

// ============================================================================
// 智能拆解 (Beta) —— 与 Rust 侧 smart_split_preview / smart_split_apply 契约对应
// 字段名为 camelCase，与 Rust serde 默认命名一致。
// ============================================================================

/** 拆解模式（与 Rust PlanMode snake_case 对齐）：auto=全自动 | assisted=返回候选 | preview=只算不落库 */
export type PlanMode = "auto" | "assisted" | "preview";

/** 目标客观约束（习惯/排产求解所需的事实） */
export interface Objective {
  /** 目标总量（空=使用 goal.total_qty） */
  total_qty?: number;
  /** 单位（空=使用 goal.unit） */
  unit?: string;
  /** 时间范围 */
  horizon?: {
    start_date?: string;
    end_date?: string;
  };
  /** 缓冲天数（追加在 horizon 末尾的冗余天数，默认 0） */
  buffer_days?: number;
}

/** 拆解形状偏好（与 Rust SplitShape snake_case 对齐，7 值） */
export type SplitShape =
  | "auto"
  | "even"
  | "front_loaded"
  | "back_loaded"
  | "phased"
  | "spaced_repetition"
  | "milestone";

/** 拆解模式（计划粒度规则）：by_day=每日一任务 | by_work_chunk=按次 | stepped=阶梯递进 */
export type SplitPattern = "by_day" | "by_work_chunk" | "stepped";

/** 任务类型（与 Rust TaskType snake_case 对齐） */
export type TaskType = "learn" | "review" | "practice" | "milestone";

/** 可行性状态（与 Rust FeasibilityStatus snake_case 对齐） */
export type FeasibilityStatus = "feasible" | "feasible_with_warnings" | "infeasible";

/** 硬约束：黑名单日期/范围、允许星期、每日上限 */
export interface HardConstraints {
  blackout_dates?: string[];
  blackout_ranges?: { start?: string; end?: string }[];
  /** 0=周日, 1-6=周一至周六 */
  allowed_weekdays?: number[];
  max_daily_qty?: number;
  /** 单一 chunk 最小数量 */
  min_chunk_qty?: number;
  /** 每日最大可用时长（小时） */
  max_daily_hours?: number;
  /** 每日最大任务数 */
  max_daily_tasks?: number;
}

/** 软约束：倾向跳过的日期（可被可行性逻辑放宽） */
export interface SoftConstraints {
  /** 倾向跳过的日期（宁可在这些天少排） */
  prefer_skip_dates?: string[];
  /** 优先 weekday 分布在宽松日期 */
  prefer_relaxed_weekdays?: boolean;
  /** 紧迫阶段（靠近截止前 N 天）提高节奏 */
  tighten_near_deadline_days?: number;
}

/** 结构偏好 */
export interface SplitStructure {
  shape?: SplitShape;
  /** 阶段划分（按顺序） */
  phases?: { name: string; weight?: number }[];
  /** 固定复习间隔天数（stepped/复习型） */
  review?: { interval_days?: number };
  /** 计划粒度 */
  interval?: SplitPattern;
}

/** 语义偏好 */
export interface SemanticPrefs {
  /** 大纲（阶段/主题列表），用于命名 */
  outline?: string[];
  /** 命名模板 */
  template?: string;
}

/** 学习/习惯 */
export interface LearningPrefs {
  /** 是否启用产物适应 */
  adapt?: boolean;
  /** 拟人信任等级 */
  trust_level?: number;
}

/** 智能拆解请求 */
export interface SmartSplitRequest {
  goal_id: string;
  /** 契约版本，固定为 2 */
  api_version: number;
  mode?: PlanMode;
  intent?: {
    objective?: Objective;
    horizon?: { start_date?: string; end_date?: string };
    buffer_days?: number;
    total_qty_override?: number;
    unit_override?: string;
  };
  constraints?: {
    hard?: HardConstraints;
    soft?: SoftConstraints;
  };
  structure?: SplitStructure;
  semantic?: SemanticPrefs;
  learning?: LearningPrefs;
  /** 兼容期标志 */
  legacy?: boolean;
  /** 是否返回解释 */
  explain?: boolean;
  /** 试运行（不产生副作用） */
  dry_run?: boolean;
}

/** 可行性补救措施 */
export interface Remedy {
  /** 补救类型（如 extend_horizon / reduce_qty / add_days ...） */
  type: string;
  label: string;
  /** 参数增量（plain object，前端可合并回表单） */
  delta: Record<string, unknown>;
  /** 补救后的 slack_ratio */
  resulting_slack_ratio: number;
  /** 代价描述 */
  cost: string;
  /** 权衡描述 */
  tradeoff: string;
}

/** 计划明细项 */
export interface PlanItem {
  seq: number;
  name: string;
  description?: string;
  /** 计划日期 yyyy-MM-dd */
  plan_date: string;
  plan_qty: number;
  unit: string;
  estimated_hours?: number;
  task_type: TaskType;
  phase?: string;
  stage_hint?: string;
  /** 依赖的前序 seq 列表 */
  depends_on_seq: number[];
  rationale?: string;
  confidence?: number;
}

/** 可行性结果 */
export interface Feasibility {
  status: FeasibilityStatus;
  /** 覆盖到的有效计划天数 */
  effective_days: number;
  effective_capacity_total: number;
  required_capacity: number;
  /** 松弛比（effective - required）/ effective；>=0 表可行 */
  slack_ratio: number;
  peak_day_load: number;
  avg_day_load: number;
  remedies: Remedy[];
}

/** 诊断信息 */
export interface SplitDiagnostics {
  applied_rules: string[];
  habit_profile?: {
    preferred_weekdays?: number[];
    avg_daily_capacity?: number;
  };
  warnings: string[];
}

/** 摘要 */
export interface SplitSummary {
  task_count: number;
  total_allocated_qty: number;
  /** 总量守恒校验 */
  conservation_ok: boolean;
}

/** 计划解析结果 */
export interface SplitResolved {
  pattern: SplitPattern;
  objective: Objective;
  horizon: {
    start: string;
    end: string;
    buffer_days: number;
  };
  total_qty: number;
  unit: string;
}

/** 智能拆解计划结果（preview / apply 共用） */
export interface SplitPlan {
  plan_id: string;
  goal_id: string;
  api_version: number;
  generated_at: string;
  engine_version: string;
  plan_hash?: string;
  resolved: SplitResolved;
  feasibility: Feasibility;
  diagnostics: SplitDiagnostics;
  items: PlanItem[];
  summary: SplitSummary;
}

// ============================================================================
// 重新规划（阶段3B）—— 与 Rust 侧 models.rs §5.4 DTO 契约对应
// 字段名 snake_case，与 Rust serde 默认命名一致；Option 映射为 TS 可空。
// ============================================================================

/** 规划范围：single（单目标）| subtree（含子目标）| coordinate（跨目标协同） */
export type ReplanScope = "single" | "subtree" | "coordinate";

/** 单任务变更类型 */
export type ReplanChangeType =
  | "date_moved"
  | "qty_changed"
  | "both"
  | "retained"
  | "locked"
  | "new";

/** 不可行处置候选 */
export type ReplanOptionKind =
  | "extend_deadline"
  | "raise_intensity"
  | "trim_scope"
  | "accept_risk";

/** 计划告警类型 */
export type ReplanWarningKind =
  | "overflow_tail"
  | "capacity_tight"
  | "cycle_detected"
  | "iteration_capped"
  | "low_fairness";

/** what-if 覆盖参数（全部可选，None=使用生效策略值） */
export interface ReplanOverrides {
  /** 截止日期 yyyy-MM-dd */
  deadline?: string | null;
  /** 每日容量（小时/量纲） */
  daily_capacity?: number;
  /** 强度倍率 */
  intensity?: number;
  /** 是否尊重锁定任务 */
  respect_locks?: boolean;
  /** 逐任务排除本次调整 */
  excluded_task_ids?: string[];
}

/** 重新规划求解（预览/仿真）输入 */
export interface ReplanSolveInput {
  goal_id: string;
  scope?: ReplanScope;
  /** 协同规划时的目标 ID 列表（scope=coordinate 时必填） */
  goal_ids?: string[];
  /** 仅仿真（不落库） */
  dry_run?: boolean;
  /** 本次覆盖策略（what-if 参数，不写回数据库） */
  overrides?: ReplanOverrides;
  /** 提交校验：预览时返回的 plan_hash */
  expected_plan_hash?: string;
}

/** 重新规划提交输入 */
export interface ReplanApplyInput {
  goal_id: string;
  /** 预览时返回的 plan_hash（一致性校验） */
  plan_hash: string;
  scope?: ReplanScope;
  /** 协同规划时的目标 ID 列表（须与预览一致，保证 apply 重解 hash 相同） */
  goal_ids?: string[];
  /** 预览时携带的事件 id（预览阶段已落 previewed 事件时回填） */
  event_id?: string;
  /** 预览时使用的 what-if 覆盖参数（须与预览一致） */
  overrides?: ReplanOverrides;
  /** 逐任务排除（与 overrides.excluded_task_ids 合并） */
  excluded_task_ids?: string[];
  /** 用户选择「接受风险」照常应用（落库后目标标记 at_risk） */
  accept_risk?: boolean;
}

/** 单任务放置结果 */
export interface TaskPlacement {
  task_id: string;
  name: string;
  old_plan_date: string | null;
  new_plan_date: string | null;
  old_plan_qty: number;
  new_plan_qty: number;
  unit: string;
  change_type: ReplanChangeType;
  /** 决策理由枚举 */
  reason: string;
  /** 人类可读理由 */
  reason_text: string;
  locked: boolean;
  /** 依赖下界（若由依赖产生） */
  dep_lower_bound: string | null;
}

/** 不可行处置候选 */
export interface ReplanOption {
  kind: ReplanOptionKind;
  label: string;
  /** 量化后果，如 { days:16, new_deadline:"2026-09-15" } */
  payload: Record<string, unknown>;
  /** 是否含风险（超强度上限等） */
  risky: boolean;
}

/** 可行性报告 */
export interface FeasibilityReport {
  feasible: boolean;
  workload: number;
  capacity: number;
  /** 裕度 (capacity - workload) / capacity */
  slack: number;
  level: "tight" | "normal" | "relaxed";
  /** 不可行原因：capacity_shortfall | dependency_chain_too_long */
  reason: string | null;
  options: ReplanOption[];
}

/** 规划后单日负载 */
export interface DayLoadAfter {
  date: string;
  before_tasks: number;
  before_qty: number;
  after_tasks: number;
  after_qty: number;
}

/** 计划指标 */
export interface ReplanMetrics {
  moved: number;
  qty_changed: number;
  retained: number;
  locked: number;
  overdue_recovered: number;
  /** 跨目标协同时的 Jain 公平指数 */
  jain_index: number | null;
  /** 预计完成日期 */
  predicted_finish: string | null;
  /** 求解耗时（毫秒） */
  elapsed_ms: number;
}

/** 计划告警 */
export interface PlanWarning {
  kind: ReplanWarningKind;
  message: string;
  task_ids: string[];
}

/** 重新规划完整计划（预览与提交共用） */
export interface ReplanPlan {
  goal_id: string;
  scope: ReplanScope;
  feasibility: FeasibilityReport;
  placements: TaskPlacement[];
  /** 日期 → 规划后负载（含外部目标） */
  day_load: DayLoadAfter[];
  warnings: PlanWarning[];
  metrics: ReplanMetrics;
  /** 内容摘要，提交时校验 */
  plan_hash: string;
  /** 本次触发原因（用于 UI 徽标） */
  trigger: string | null;
}

/** 提交结果 */
export interface ReplanApplyResult {
  event_id: string;
  goal_id: string;
  moved: number;
  qty_changed: number;
  retained: number;
  /** 受影响目标 ID（供前端局部刷新进度链） */
  affected_goal_ids: string[];
  tasks: Task[];
}

/** 重规划事件（时间线分页项） */
export interface ReplanEvent {
  id: string | null;
  goal_id: string | null;
  trigger_type: string | null;
  trigger_source: string | null;
  severity: string | null;
  status: string | null;
  plan_hash: string | null;
  snapshot_hash: string | null;
  before_summary: string | null;
  after_summary: string | null;
  metrics: string | null;
  error_message: string | null;
  created_at: string | null;
  applied_at: string | null;
}

/** 事件差异 */
export interface ReplanEventDiff {
  id: string | null;
  event_id: string | null;
  task_id: string | null;
  change_type: ReplanChangeType | null;
  old_plan_date: string | null;
  new_plan_date: string | null;
  old_plan_qty: number | null;
  new_plan_qty: number | null;
  old_unit: string | null;
  new_unit: string | null;
  reason: string | null;
}

/** 事件详情（时间线展开） */
export interface ReplanEventDetail {
  event: ReplanEvent;
  diffs: ReplanEventDiff[];
}

// ============================================================================
// 触发网关与画像学习（阶段 4）
// ============================================================================

/** 触发事件类型（9 类） */
export type ReplanTriggerType =
  | "task_completed"
  | "task_skipped"
  | "overdue_accumulated"
  | "goal_deadline_changed"
  | "goal_total_changed"
  | "progress_lag"
  | "load_overload"
  | "periodic_review"
  | "manual_request";

/** 单个目标的触发评估结果 */
export interface TriggerGoalResult {
  goal_id: string;
  goal_name: string;
  /** proceed | auto_apply | suppressed */
  decision: "proceed" | "auto_apply" | "suppressed";
  trigger_type: string;
  /** info | warning | critical */
  severity: string;
  reason: string | null;
  suppress_reason: string | null;
  impact_ratio: number | null;
  event_id: string | null;
}

/** 批量触发检查报告 */
export interface TriggerReport {
  goals: TriggerGoalResult[];
  pending_count: number;
}

/** 画像学习反馈信号（7 类，与后端 validate_feedback_signal 对齐） */
export type FeedbackSignalType =
  | "qty_change"
  | "date_change"
  | "delete"
  | "early_finish"
  | "late_finish"
  | "skip"
  | "plan_adopt";

/** 反馈输入（写入 split_feedback 并更新画像） */
export interface FeedbackInput {
  goal_id: string;
  task_id: string;
  signal_type: FeedbackSignalType;
  plan_date?: string | null;
  planned_qty?: number | null;
  actual_qty?: number | null;
  final_qty?: number | null;
  date_shift?: number | null;
  chosen_pattern?: string | null;
}
