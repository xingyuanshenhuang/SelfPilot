-- P1-3 时间预算模型
-- goals 增加每日可用时长（按时间预算拆解时使用）
ALTER TABLE goals ADD COLUMN daily_capacity REAL;

-- tasks 增加预估时长（可选，记录每个任务的预估小时数）
ALTER TABLE tasks ADD COLUMN estimated_hours REAL;
