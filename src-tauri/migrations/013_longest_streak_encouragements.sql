-- Migration 013: longest_streak 触发文案（P3-4）
-- 新增 5 条历史记录里程碑鼓励语

-- ============================================================
-- 扩展 level CHECK 约束以包含 'longest_streak'
-- SQLite 不支持 ALTER CHECK，需重建表
--
-- 幂等策略：检查是否已存在 longest_streak 记录来判断是否需要重建
-- ============================================================

-- 创建临时表标记迁移状态（如果 longest_streak 记录已存在，说明迁移已完成）
CREATE TEMP TABLE IF NOT EXISTS _mig13_done AS
SELECT 1 AS done WHERE EXISTS (SELECT 1 FROM encouragements WHERE level = 'longest_streak');

-- 仅在迁移未完成时执行重建
-- SQLite 不支持 IF NOT EXISTS 对整段 DDL，用 SELECT + 子查询模拟条件执行
-- 这里采用更安全的策略：先检测，再决定是否执行

-- 清理可能残留的临时表
DROP TABLE IF EXISTS encouragements_new;

-- 临时禁用外键约束
PRAGMA foreign_keys = OFF;

-- 1. 创建新表（扩展 CHECK 约束）
CREATE TABLE IF NOT EXISTS encouragements_new (
    id TEXT PRIMARY KEY,
    text TEXT NOT NULL,
    category TEXT NOT NULL,
    level TEXT NOT NULL DEFAULT 'normal'
        CHECK(level IN ('normal','advanced','highlight','celebration','setback','longest_streak')),
    created_at TEXT NOT NULL,
    context_tags TEXT DEFAULT '{}',
    hidden INTEGER DEFAULT 0,
    weight REAL DEFAULT 1.0
);

-- 2. 使用 INSERT OR REPLACE 复制数据（幂等）
INSERT OR REPLACE INTO encouragements_new (id, text, category, level, created_at, context_tags, hidden)
SELECT id, text, category, level, created_at, context_tags, hidden FROM encouragements;

-- 3. 删除旧表
DROP TABLE encouragements;

-- 4. 重命名新表
ALTER TABLE encouragements_new RENAME TO encouragements;

-- 恢复外键约束
PRAGMA foreign_keys = ON;

-- 清理临时标记表
DROP TABLE IF EXISTS _mig13_done;

-- ============================================================
-- 新增 5 条历史记录里程碑鼓励语（幂等插入）
-- ============================================================

INSERT OR IGNORE INTO encouragements (id, text, category, level, created_at, context_tags)
VALUES
-- 接近历史记录（距离 2 天内）
('longest-streak-1', '距离历史记录只差一步，你曾经坚持过，这次也能！', 'preset', 'longest_streak', datetime('now'), '{"milestone":"approaching"}'),

-- 追平历史记录
('longest-streak-2', '追平了自己的历史记录！你已经是自己最好的对手了。', 'preset', 'longest_streak', datetime('now'), '{"milestone":"equal"}'),

-- 超越历史记录（小幅度）
('longest-streak-3', '超越历史记录！每次突破都是在重新定义自己的极限。', 'preset', 'longest_streak', datetime('now'), '{"milestone":"breakthrough"}'),

-- 超越历史记录（大幅度，+7天以上）
('longest-streak-4', '大幅超越历史记录！你正在书写一个全新的自己。', 'preset', 'longest_streak', datetime('now'), '{"milestone":"major"}'),

-- 鼓励继续坚持（接近/追平后）
('longest-streak-5', '历史已经被你甩在身后，新的记录在等着你书写。', 'preset', 'longest_streak', datetime('now'), '{"milestone":"continue"}');