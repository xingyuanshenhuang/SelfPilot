-- 为任务添加精确逾期日期记录，支持按日期聚合查看逾期任务
ALTER TABLE tasks ADD COLUMN overdue_date TEXT;

CREATE INDEX IF NOT EXISTS idx_tasks_overdue_date ON tasks(overdue_date);
