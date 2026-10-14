-- 重新规划重构（阶段3）· 核心模型
--
-- 依据《目标树重新规划功能重构设计文档》§5.2.1（原设计 021，因 021=split_plans
-- 已占用而顺延）：
-- - goal_planning_policy：目标级规划策略（未设置行时回退全局默认，见 settings）
-- - task_schedule_meta：任务排期元数据（与 tasks 1:1，避免 tasks 表过度膨胀）
-- - goals 扩展：优先级 / 自动化开关 / 风险标记
-- - tasks 扩展：排期锁定（schedule_locked）+ 更新时间（updated_at，撤销可行性判定依赖）
--
-- 一次性迁移，一经提交永不再改（校验和教训）。

-- 1) 目标级规划策略（未设置行时回退全局默认，见 settings）
CREATE TABLE IF NOT EXISTS goal_planning_policy (
    goal_id            TEXT PRIMARY KEY,
    -- 强度系数 0.5 ~ 1.5
    intensity          REAL    NOT NULL DEFAULT 1.0,
    -- 单日硬上限（小时 / 数量），NULL = 不限
    max_daily_hours    REAL,
    max_daily_tasks    REAL,
    -- 是否偏好连续排期（减少碎片日）
    prefer_contiguous  INTEGER NOT NULL DEFAULT 0,
    -- 休息日 JSON 数组，元素 0~6（0=周日）
    rest_days          TEXT    NOT NULL DEFAULT '[]',
    -- 无 deadline 时的滚动窗口天数
    window_cap_days    INTEGER NOT NULL DEFAULT 30,
    -- 单位模式：hours | quantity | auto
    unit_mode          TEXT    NOT NULL DEFAULT 'auto'
        CHECK(unit_mode IN ('hours','quantity','auto')),
    -- 逾期任务数达到该值触发自动规划
    overdue_threshold  REAL    NOT NULL DEFAULT 3,
    -- 单日负载（任务数）达到该值视为过载
    load_threshold     REAL    NOT NULL DEFAULT 7,
    updated_at         TEXT    NOT NULL,
    FOREIGN KEY (goal_id) REFERENCES goals(id) ON DELETE CASCADE
);

-- 2) goals 扩展：优先级、自动化、风险标记
ALTER TABLE goals ADD COLUMN priority    REAL    NOT NULL DEFAULT 1.0;
ALTER TABLE goals ADD COLUMN auto_replan INTEGER NOT NULL DEFAULT 1;
ALTER TABLE goals ADD COLUMN auto_apply  INTEGER NOT NULL DEFAULT 0;
ALTER TABLE goals ADD COLUMN risk_flag   TEXT;

CREATE INDEX IF NOT EXISTS idx_goals_risk_flag ON goals(risk_flag);
CREATE INDEX IF NOT EXISTS idx_goals_auto_replan ON goals(auto_replan);

-- 3) 任务排期元数据（与 tasks 1:1，避免 tasks 表过度膨胀）
CREATE TABLE IF NOT EXISTS task_schedule_meta (
    task_id         TEXT PRIMARY KEY,
    -- 最近一次排期决策理由：locked | dep_lower_bound | overdue_pull |
    --                       load_balance | profile_rhythm | smoothing_retain | manual
    alloc_reason    TEXT,
    last_planned_at TEXT,
    -- 任务被重排迁移的累计次数（用于迁移代价评估）
    move_count      INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE
);

-- 4) tasks 扩展：排期锁定 + 更新时间（撤销可行性判定依赖）
ALTER TABLE tasks ADD COLUMN schedule_locked TEXT;
ALTER TABLE tasks ADD COLUMN updated_at      TEXT;

UPDATE tasks SET updated_at = created_at WHERE updated_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_tasks_schedule_locked ON tasks(schedule_locked);
CREATE INDEX IF NOT EXISTS idx_tasks_goal_status_date ON tasks(goal_id, status, plan_date);
