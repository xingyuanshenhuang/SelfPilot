-- Migration 014: 鼓励语自定义排序（P3-5）
-- 新增 sort_order 字段，支持用户拖拽排序

ALTER TABLE encouragements ADD COLUMN sort_order INTEGER DEFAULT 0;

-- 按现有 created_at 顺序初始化 sort_order
UPDATE encouragements SET sort_order = (
    SELECT COUNT(*) FROM encouragements e2
    WHERE e2.created_at <= encouragements.created_at
);

-- 创建索引加速排序查询
CREATE INDEX IF NOT EXISTS idx_encouragements_sort_order ON encouragements(sort_order);
