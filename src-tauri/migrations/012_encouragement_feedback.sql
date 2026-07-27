-- Migration 012: 鼓励语展示反馈学习（P3-3）
-- 扩展展示日志表，增加用户行为字段

-- 新增字段：关闭时间、观看时长（秒）
ALTER TABLE encouragement_show_log ADD COLUMN closed_at TEXT;
ALTER TABLE encouragement_show_log ADD COLUMN view_duration INTEGER DEFAULT 0;

-- 创建索引加速反馈查询
CREATE INDEX IF NOT EXISTS idx_encouragement_show_log_encouragement_id_closed_at
ON encouragement_show_log(encouragement_id, closed_at);