-- P0 鼓励语库迭代：合并前端硬编码文案 + 展示历史表
-- 对应评估文档 P0-1（统一文案源）与 P0-4（展示历史与去重）

-- ============================================================
-- P0-1: 将 constants/encouragements.ts 的 10 条文案导入 DB
-- 等级全部归为 normal，category=preset
-- 按 text 去重（"今天的努力，是明天的底气。" 已存在于 preset-02，会跳过）
-- 使用 INSERT ... SELECT ... WHERE NOT EXISTS 模式（幂等可重入）
-- ============================================================

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-fe-01', '今天又进步了！', 'preset', 'normal', '2026-07-19T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '今天又进步了！');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-fe-02', '坚持就是胜利，继续加油！', 'preset', 'normal', '2026-07-19T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '坚持就是胜利，继续加油！');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-fe-03', '每一步都算数，你做得很棒！', 'preset', 'normal', '2026-07-19T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '每一步都算数，你做得很棒！');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-fe-04', '学习是给自己最好的礼物。', 'preset', 'normal', '2026-07-19T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '学习是给自己最好的礼物。');

-- 第 5 条 "今天的努力，是明天的底气。" 已存在于 preset-02，去重后跳过
INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-fe-05', '今天的努力，是明天的底气。', 'preset', 'normal', '2026-07-19T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '今天的努力，是明天的底气。');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-fe-06', '小步快跑，日积月累就是大跨越！', 'preset', 'normal', '2026-07-19T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '小步快跑，日积月累就是大跨越！');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-fe-07', '你比昨天的自己更强了。', 'preset', 'normal', '2026-07-19T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '你比昨天的自己更强了。');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-fe-08', '完成一个任务就是一次胜利！', 'preset', 'normal', '2026-07-19T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '完成一个任务就是一次胜利！');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-fe-09', '自律给我自由，继续前行。', 'preset', 'normal', '2026-07-19T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '自律给我自由，继续前行。');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-fe-10', '种一棵树最好的时间是十年前，其次是现在。', 'preset', 'normal', '2026-07-19T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '种一棵树最好的时间是十年前，其次是现在。');

-- ============================================================
-- P0-4: 鼓励语展示历史表（用于最近 N 条去重）
-- trigger_source 枚举值（应用层校验，不用 CHECK 以便未来扩展）:
--   complete_first       完成今日首任务（modal）
--   complete_normal      完成非首任务（toast）
--   complete_celebration 全部目标完成（celebration modal）
--   dashboard_banner     进入仪表盘 banner
-- ============================================================

CREATE TABLE IF NOT EXISTS encouragement_show_log (
    id TEXT PRIMARY KEY,
    encouragement_id TEXT NOT NULL,
    shown_at TEXT NOT NULL,
    trigger_source TEXT NOT NULL,
    FOREIGN KEY (encouragement_id) REFERENCES encouragements(id) ON DELETE CASCADE
);

-- 按 shown_at 倒序查最近 5 条展示记录（去重窗口）
CREATE INDEX IF NOT EXISTS idx_enc_show_log_shown_at ON encouragement_show_log(shown_at DESC);
-- 按 encouragement_id 查展示次数（P3 反馈学习预留）
CREATE INDEX IF NOT EXISTS idx_enc_show_log_enc_id ON encouragement_show_log(encouragement_id);