-- Migration 010: 鼓励语情境扩展（P2阶段）
-- 新增字段：
--   - context_tags: JSON 存储多维度情境标签
--   - hidden: 预设文案隐藏标记

-- 新增 context_tags 字段（JSON 存储标签）
ALTER TABLE encouragements ADD COLUMN context_tags TEXT DEFAULT '{}';

-- 新增 hidden 字段（预设文案隐藏标记）
ALTER TABLE encouragements ADD COLUMN hidden INTEGER DEFAULT 0;

-- 创建索引加速按标签查询
CREATE INDEX IF NOT EXISTS idx_encouragements_hidden ON encouragements(hidden);