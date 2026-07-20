-- Migration 011: 鼓励语个性化机制（P3阶段）
-- 新增字段/表：
--   - weight: 权重（用于加权随机）
--   - sort_order: 排序（用于拖拽排序）
--   - user_favorites: 用户收藏表
--   - encouragement_feedback: 反馈表

-- 新增 weight 字段（默认权重1.0）
ALTER TABLE encouragements ADD COLUMN weight REAL DEFAULT 1.0;

-- 新增 sort_order 字段（默认0表示按创建时间排序）
ALTER TABLE encouragements ADD COLUMN sort_order INTEGER DEFAULT 0;

-- 用户收藏表
CREATE TABLE IF NOT EXISTS user_favorites (
    id TEXT PRIMARY KEY,
    encouragement_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (encouragement_id) REFERENCES encouragements(id) ON DELETE CASCADE
);

-- 创建索引加速查询
CREATE INDEX IF NOT EXISTS idx_user_favorites_encouragement_id ON user_favorites(encouragement_id);

-- 鼓励语反馈表
CREATE TABLE IF NOT EXISTS encouragement_feedback (
    id TEXT PRIMARY KEY,
    encouragement_id TEXT NOT NULL,
    feedback_type TEXT NOT NULL CHECK (feedback_type IN ('like', 'dislike')),
    created_at TEXT NOT NULL,
    FOREIGN KEY (encouragement_id) REFERENCES encouragements(id) ON DELETE CASCADE
);

-- 创建索引
CREATE INDEX IF NOT EXISTS idx_encouragement_feedback_encouragement_id ON encouragement_feedback(encouragement_id);
CREATE INDEX IF NOT EXISTS idx_encouragement_feedback_type ON encouragement_feedback(feedback_type);