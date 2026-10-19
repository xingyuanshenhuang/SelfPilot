-- 智能规划（阶段4）· 反馈信号留痕（画像学习数据源）
--
-- 依据《目标树智能规划与重新规划整合设计方案》§4.6（习惯学习 FR-16/17）：
-- 每次任务级反馈（数量变更/日期变更/删除/提前/滞后/跳过/方案采纳）写一条记录，
-- 供 planning_profile 画像 EWMA 更新与后续分析复用。
--
-- 一次性迁移，一经提交永不再改（校验和教训）。

CREATE TABLE IF NOT EXISTS split_feedback (
    id             TEXT PRIMARY KEY,
    goal_id        TEXT NOT NULL,
    task_id        TEXT NOT NULL,
    -- 信号类型：qty_change | date_change | delete | early_finish | late_finish |
    --           skip | plan_adopt
    signal_type    TEXT NOT NULL,
    -- 原计划日期（信号发生时的 plan_date，可为空）
    plan_date      TEXT,
    -- 原计划量 / 信号时实际量 / 信号后最终量
    planned_qty    REAL,
    actual_qty     REAL,
    final_qty      REAL,
    -- 日期偏移天数（信号后日期 − 原计划日期；正=顺延，负=提前）
    date_shift     INTEGER,
    -- 采纳的拆解模式（smart_split 的 pattern 字段，可空）
    chosen_pattern TEXT,
    created_at     TEXT NOT NULL,
    FOREIGN KEY (goal_id) REFERENCES goals(id) ON DELETE CASCADE,
    FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE
);

-- 按目标查反馈历史（画像学习聚合）
CREATE INDEX IF NOT EXISTS idx_split_feedback_goal_created
    ON split_feedback(goal_id, created_at DESC);
-- 按信号类型查（触发统计）
CREATE INDEX IF NOT EXISTS idx_split_feedback_signal
    ON split_feedback(signal_type);
