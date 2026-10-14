-- 重新规划（阶段3）· 事件流水与差异留痕（可解释 + 可回滚）
--
-- 依据《目标树重新规划功能重构设计文档》§5.2.2（原设计 022，因 021=split_plans
-- 已占用而顺延为 023）：
-- - replan_events：每次求解/应用/撤销/抑制留一条事件记录（触发原因、输入输出摘要）
-- - replan_event_diffs：一行 = 一个任务的字段级变更（撤销时反向应用即可回滚）
-- - replan_trigger_state：冷却与幂等判定依据（阶段4 触发网关消费）
--
-- 一次性迁移，一经提交永不再改（校验和教训）。

CREATE TABLE IF NOT EXISTS replan_events (
    id              TEXT PRIMARY KEY,
    goal_id         TEXT NOT NULL,
    -- 触发类型：manual | task_completed | task_skipped | overdue_accumulated |
    --           goal_deadline_changed | goal_total_changed | progress_lag |
    --           load_overload | periodic_review
    trigger_type    TEXT NOT NULL,
    -- 触发来源：user | system | scheduler
    trigger_source  TEXT NOT NULL DEFAULT 'user',
    -- 严重度：info | warning | critical
    severity        TEXT NOT NULL DEFAULT 'info',
    -- 状态：previewed | applied | reverted | suppressed | failed
    status          TEXT NOT NULL DEFAULT 'previewed',
    -- 计划内容摘要（提交校验）与快照摘要（幂等去重）
    plan_hash       TEXT,
    snapshot_hash   TEXT,
    -- 变更前/后摘要 JSON
    before_summary  TEXT,
    after_summary   TEXT,
    -- 指标 JSON：{workload, capacity, slack, jain_index, moved, updated, feasibility}
    metrics         TEXT,
    error_message   TEXT,
    created_at      TEXT NOT NULL,
    applied_at      TEXT,
    FOREIGN KEY (goal_id) REFERENCES goals(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_replan_events_goal_created
    ON replan_events(goal_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_replan_events_status
    ON replan_events(status);

-- 差异明细：一行 = 一个任务的字段级变更
CREATE TABLE IF NOT EXISTS replan_event_diffs (
    id            TEXT PRIMARY KEY,
    event_id      TEXT NOT NULL,
    task_id       TEXT NOT NULL,
    -- 变更类型：date_moved | qty_changed | both | retained | locked | split | new
    change_type   TEXT NOT NULL,
    old_plan_date TEXT,
    new_plan_date TEXT,
    old_plan_qty  REAL,
    new_plan_qty  REAL,
    old_unit      TEXT,
    new_unit      TEXT,
    -- 决策理由：locked | dep_lower_bound | overdue_pull | load_balance |
    --           profile_rhythm | smoothing_retain | capacity_trim
    reason        TEXT,
    FOREIGN KEY (event_id) REFERENCES replan_events(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_replan_diffs_event ON replan_event_diffs(event_id);
CREATE INDEX IF NOT EXISTS idx_replan_diffs_task  ON replan_event_diffs(task_id);

-- 触发状态：冷却与幂等判定依据
CREATE TABLE IF NOT EXISTS replan_trigger_state (
    goal_id              TEXT PRIMARY KEY,
    last_snapshot_hash   TEXT,
    last_auto_replan_at  TEXT,
    suppressed_count     INTEGER NOT NULL DEFAULT 0,
    updated_at           TEXT NOT NULL,
    FOREIGN KEY (goal_id) REFERENCES goals(id) ON DELETE CASCADE
);
