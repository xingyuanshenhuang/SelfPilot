-- 目标树智能规划引擎（v2）：拆解方案版本化（FR-15）
--
-- split_plans 记录每次拆解/重新规划的完整方案（完整请求 + 完整摘要）：
-- - 同一目标重复规划时，旧行 is_active 置 0，新行 is_active = 1
-- - id = plan_id（UUID 实体主键）；plan_hash 为方案内容摘要（格式 sha256:<64 hex>），
--   用于「预览-落库」一致性校验（PlanStale 防护），二者不等价、均保留
-- - request_json / summary_json 保留审计与问题复现所需的完整原文
-- - feasibility / slack_ratio / task_count / total_allocated_qty 为关键指标冗余列，便于查询
--
-- 关联关系（后续迁移建立，此处仅预留语义）：
-- - split_feedback.plan_id 弱关联本表（反馈采集，024 迁移）
-- - replan_events.metrics 记录 plan_id、plan_hash 与本表关联（022 迁移）

CREATE TABLE IF NOT EXISTS split_plans (
    id                  TEXT PRIMARY KEY,
    goal_id             TEXT NOT NULL,
    engine_version      TEXT NOT NULL DEFAULT '',
    pattern             TEXT NOT NULL DEFAULT '',
    plan_hash           TEXT NOT NULL DEFAULT '',
    request_json        TEXT NOT NULL DEFAULT '{}',
    summary_json        TEXT NOT NULL DEFAULT '{}',
    feasibility         TEXT NOT NULL DEFAULT 'feasible',
    slack_ratio          REAL NOT NULL DEFAULT 0,
    task_count           INTEGER NOT NULL DEFAULT 0,
    total_allocated_qty REAL NOT NULL DEFAULT 0,
    is_active           INTEGER NOT NULL DEFAULT 0,
    created_at          TEXT NOT NULL DEFAULT '',
    FOREIGN KEY (goal_id) REFERENCES goals(id) ON DELETE CASCADE
);

-- 按目标查方案历史 / 当前生效方案（WHERE goal_id = ? [AND is_active = 1]）
CREATE INDEX IF NOT EXISTS idx_split_plans_goal_active
    ON split_plans(goal_id, is_active);

-- 按内容摘要定位方案（提交一致性校验、replan_events 关联查询）
CREATE INDEX IF NOT EXISTS idx_split_plans_plan_hash
    ON split_plans(plan_hash);
