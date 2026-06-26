-- SelfPilot 目标树重构：统一 Goal 为树结构，Stage 转为子目标
-- 总目标(parent_id=NULL) → 子目标(parent_id=总目标id) → 子任务
-- 完成规则：子目标完成=其下所有子任务完成；总目标完成=所有子目标完成+所有直属子任务完成

-- 1. 为 goals 表添加树结构字段
ALTER TABLE goals ADD COLUMN parent_id TEXT;
ALTER TABLE goals ADD COLUMN path TEXT NOT NULL DEFAULT '';
ALTER TABLE goals ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0;

-- 2. 为现有 goals 设置 path（根目标）
UPDATE goals SET path = '/' || id WHERE parent_id IS NULL;

-- 3. 将 stages 转换为子目标（插入 goals 表，parent_id 指向原 goal_id）
INSERT OR IGNORE INTO goals (id, name, parent_id, path, deadline, total_qty, unit, sort_order, created_at)
SELECT
    s.id,
    s.name,
    s.goal_id,
    '/' || s.goal_id || '/' || s.id,
    NULL,
    0,
    '',
    s.sort_order,
    s.created_at
FROM stages s;

-- 4. 将原来挂在 stage 下的 task 的 goal_id 改为指向子目标（原 stage_id）
UPDATE tasks
SET goal_id = stage_id
WHERE stage_id IS NOT NULL;

-- 5. 重置所有 task 的 path 为简单格式
UPDATE tasks SET path = '/' || goal_id || '/' || id;

-- 6. stages 表不再使用（保留以备回滚，应用代码不再引用）
