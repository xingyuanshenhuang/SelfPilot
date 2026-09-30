-- 重复任务系列（原任务）支持：从任意重复任务实例"编辑原任务"并级联更新
--
-- 1. 新增 repeat_series 表：保存重复任务的原任务设置（名称/数量/单位/频率/日期范围/周几/每月几号）
-- 2. tasks 增加 repeat_series_id 列：实例与系列关联
-- 3. 历史回填：识别命名含 " - 第N次" 的自动任务（智能拆解为 " - 第N天"，不受影响），
--    按 goal_id + created_at + 基础名称分组自动补建系列；历史频率无法还原，默认 daily，
--    起止日期取 min/max(plan_date)，用户可在"编辑原任务"时改正

CREATE TABLE IF NOT EXISTS repeat_series (
    id TEXT PRIMARY KEY,
    goal_id TEXT NOT NULL,
    base_name TEXT NOT NULL,
    plan_qty REAL NOT NULL DEFAULT 1,
    unit TEXT NOT NULL DEFAULT '',
    frequency TEXT NOT NULL DEFAULT 'daily',
    start_date TEXT NOT NULL,
    end_date TEXT,
    weekdays TEXT NOT NULL DEFAULT '[]',   -- JSON 数组
    month_days TEXT NOT NULL DEFAULT '[]', -- JSON 数组
    created_at TEXT NOT NULL,
    FOREIGN KEY (goal_id) REFERENCES goals(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_repeat_series_goal_id ON repeat_series(goal_id);
CREATE INDEX IF NOT EXISTS idx_repeat_series_goal_created ON repeat_series(goal_id, created_at);

ALTER TABLE tasks ADD COLUMN repeat_series_id TEXT;
CREATE INDEX IF NOT EXISTS idx_tasks_repeat_series_id ON tasks(repeat_series_id);

-- 历史回填：为符合重复命名模式的自动任务按生成批次补建系列
INSERT INTO repeat_series (id, goal_id, base_name, plan_qty, unit, frequency, start_date, end_date, weekdays, month_days, created_at)
SELECT lower(hex(randomblob(16))), goal_id,
       substr(name, 1, instr(name, ' - 第') - 1),
       min(plan_qty), min(unit), 'daily',
       min(plan_date), max(plan_date), '[]', '[]', created_at
FROM tasks
WHERE source = 'auto' AND instr(name, ' - 第') > 0 AND name LIKE '% - 第%次'
GROUP BY goal_id, created_at, substr(name, 1, instr(name, ' - 第') - 1)
HAVING COUNT(*) > 1;

-- 回填 tasks.repeat_series_id（仅当同批次存在唯一匹配系列，避免歧义）
UPDATE tasks SET repeat_series_id = (
    SELECT s.id FROM repeat_series s
    WHERE s.goal_id = tasks.goal_id AND s.created_at = tasks.created_at
      AND s.base_name = substr(tasks.name, 1, instr(tasks.name, ' - 第') - 1)
)
WHERE source = 'auto' AND instr(name, ' - 第') > 0 AND name LIKE '% - 第%次'
  AND (SELECT COUNT(*) FROM repeat_series s
       WHERE s.goal_id = tasks.goal_id AND s.created_at = tasks.created_at
         AND s.base_name = substr(tasks.name, 1, instr(tasks.name, ' - 第') - 1)) = 1;
