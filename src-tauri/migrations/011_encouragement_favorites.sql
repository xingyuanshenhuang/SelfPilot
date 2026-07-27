-- Migration 011: 鼓励语收藏机制（P3-1 加权随机抽取）
-- 新增表：
--   - encouragement_favorites: 用户收藏的鼓励语

-- 创建收藏表
CREATE TABLE IF NOT EXISTS encouragement_favorites (
    id TEXT PRIMARY KEY,
    encouragement_id TEXT NOT NULL,
    favorited_at TEXT NOT NULL,
    FOREIGN KEY (encouragement_id) REFERENCES encouragements(id) ON DELETE CASCADE
);

-- 创建索引加速收藏查询
CREATE INDEX IF NOT EXISTS idx_encouragement_favorites_encouragement_id ON encouragement_favorites(encouragement_id);
CREATE INDEX IF NOT EXISTS idx_encouragement_favorites_favorited_at ON encouragement_favorites(favorited_at);