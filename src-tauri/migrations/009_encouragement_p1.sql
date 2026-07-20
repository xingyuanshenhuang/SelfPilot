-- P1 鼓励语库迭代：文案扩充至 80 条 + 用户偏好设置默认值
-- 对应评估文档 P1-1（文案库扩充）与 P1-4（用户偏好设置）

-- ============================================================
-- 扩展 level CHECK 约束以包含 'setback'（挫折场景）
-- SQLite 不支持 ALTER CHECK，需重建表
-- ============================================================

-- 临时禁用外键约束（encouragement_show_log 引用 encouragements）
PRAGMA foreign_keys = OFF;

-- 1. 创建新表（扩展 CHECK 约束）
CREATE TABLE IF NOT EXISTS encouragements_new (
    id TEXT PRIMARY KEY,
    text TEXT NOT NULL,
    category TEXT NOT NULL,
    level TEXT NOT NULL DEFAULT 'normal'
        CHECK(level IN ('normal','advanced','highlight','celebration','setback')),
    created_at TEXT NOT NULL
);

-- 2. 复制数据
INSERT INTO encouragements_new (id, text, category, level, created_at)
SELECT id, text, category, level, created_at FROM encouragements;

-- 3. 删除旧表
DROP TABLE encouragements;

-- 4. 重命名新表
ALTER TABLE encouragements_new RENAME TO encouragements;

-- 恢复外键约束
PRAGMA foreign_keys = ON;

-- ============================================================
-- P1-1: 文案库扩充（幂等插入）
-- 使用 INSERT ... SELECT ... WHERE NOT EXISTS 模式
-- 总计新增 52 条：normal 7 + advanced 14 + highlight 15 + celebration 6 + setback 10
-- ============================================================

-- normal 等级新增 7 条（连续 1 天完成 / 非首任务 toast）
-- 风格：平实鼓励，避免鸡汤化，10-30 字

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-n01', '又完成一项，稳扎稳打。', 'preset', 'normal', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '又完成一项，稳扎稳打。');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-n02', '这一步，很实在。', 'preset', 'normal', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '这一步，很实在。');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-n03', '做到了，就值得记录。', 'preset', 'normal', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '做到了，就值得记录。');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-n04', '完成本身就是一种积累。', 'preset', 'normal', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '完成本身就是一种积累。');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-n05', '不急不躁，一步步来。', 'preset', 'normal', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '不急不躁，一步步来。');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-n06', '你正在按自己的节奏前进。', 'preset', 'normal', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '你正在按自己的节奏前进。');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-n07', '每个完成的任务，都是进步的证据。', 'preset', 'normal', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '每个完成的任务，都是进步的证据。');

-- advanced 等级新增 14 条（连续 3 天完成）
-- 风格：肯定坚持，强调习惯养成

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-a01', '三天不间断，习惯已成型。', 'preset', 'advanced', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '三天不间断，习惯已成型。');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-a02', '三天打卡，你已经超过多数人的坚持。', 'preset', 'advanced', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '三天打卡，你已经超过多数人的坚持。');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-a03', '连续三天，自律正在成为本能。', 'preset', 'advanced', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '连续三天，自律正在成为本能。');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-a04', '三天的坚持，胜过三天的空想。', 'preset', 'advanced', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '三天的坚持，胜过三天的空想。');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-a05', '你用行动证明了：能坚持。', 'preset', 'advanced', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '你用行动证明了：能坚持。');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-a06', '三天累计，不是运气，是选择。', 'preset', 'advanced', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '三天累计，不是运气，是选择。');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-a07', '你已经连续三天完成计划。', 'preset', 'advanced', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '你已经连续三天完成计划。');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-a08', '三天的小目标，累积成大改变。', 'preset', 'advanced', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '三天的小目标，累积成大改变。');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-a09', '三天坚持下来，已经值得给自己点个赞。', 'preset', 'advanced', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '三天坚持下来，已经值得给自己点个赞。');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-a10', '连续完成三天，节奏找到了。', 'preset', 'advanced', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '连续完成三天，节奏找到了。');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-a11', '三天连胜，你的执行力在说话。', 'preset', 'advanced', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '三天连胜，你的执行力在说话。');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-a12', '连续三天，你已经建立起一种秩序。', 'preset', 'advanced', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '连续三天，你已经建立起一种秩序。');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-a13', '三天不空，习惯正在发芽。', 'preset', 'advanced', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '三天不空，习惯正在发芽。');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-a14', '三天打卡完成，执行力在线。', 'preset', 'advanced', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '三天打卡完成，执行力在线。');

-- highlight 等级新增 15 条（连续 7 天完成）
-- 风格：肯定里程碑，强调长期积累

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-h01', '一周完成！习惯已成自然。', 'preset', 'highlight', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '一周完成！习惯已成自然。');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-h02', '七天坚持，你已经超越多数人的毅力。', 'preset', 'highlight', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '七天坚持，你已经超越多数人的毅力。');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-h03', '连续一周打卡，自律已是你的标签。', 'preset', 'highlight', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '连续一周打卡，自律已是你的标签。');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-h04', '七天不间断，积累的力量可见了。', 'preset', 'highlight', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '七天不间断，积累的力量可见了。');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-h05', '一周的坚持，让你比上周更强。', 'preset', 'highlight', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '一周的坚持，让你比上周更强。');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-h06', '七日打卡完成，你已经证明了自己。', 'preset', 'highlight', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '七日打卡完成，你已经证明了自己。');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-h07', '连续七天，时间开始在你这边。', 'preset', 'highlight', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '连续七天，时间开始在你这边。');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-h08', '一周的执行，胜过无数个计划。', 'preset', 'highlight', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '一周的执行，胜过无数个计划。');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-h09', '七天连胜，习惯已扎根。', 'preset', 'highlight', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '七天连胜，习惯已扎根。');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-h10', '一周打卡达成，你可以继续保持。', 'preset', 'highlight', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '一周打卡达成，你可以继续保持。');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-h11', '七天的坚持，你对目标的掌控更强了。', 'preset', 'highlight', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '七天的坚持，你对目标的掌控更强了。');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-h12', '一周的积累，让你更接近目标。', 'preset', 'highlight', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '一周的积累，让你更接近目标。');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-h13', '连续七天完成，执行力已经稳定。', 'preset', 'highlight', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '连续七天完成，执行力已经稳定。');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-h14', '一周不间断，自律已成习惯。', 'preset', 'highlight', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '一周不间断，自律已成习惯。');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-h15', '七天打卡成功，你用行动验证了可能。', 'preset', 'highlight', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '七天打卡成功，你用行动验证了可能。');

-- celebration 等级新增 6 条（全部目标完成）
-- 风格：庆祝成就，强调里程碑意义

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-c01', '全部完成！这一刻属于坚持的你。', 'preset', 'celebration', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '全部完成！这一刻属于坚持的你。');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-c02', '目标达成！你的执行力值得被记住。', 'preset', 'celebration', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '目标达成！你的执行力值得被记住。');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-c03', '全部目标完成，你已经证明了自己能做到。', 'preset', 'celebration', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '全部目标完成，你已经证明了自己能做到。');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-c04', '征程结束，但你的自律不会停。', 'preset', 'celebration', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '征程结束，但你的自律不会停。');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-c05', '所有目标达成，这是你选择的结果。', 'preset', 'celebration', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '所有目标达成，这是你选择的结果。');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-c06', '圆满完成！这一刻是对过往坚持的最好回报。', 'preset', 'celebration', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '圆满完成！这一刻是对过往坚持的最好回报。');

-- setback 等级新增 10 条（连续中断 / 目标进度滞后）
-- 风格：理性鼓励，强调重新开始，避免否定性表达

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-s01', '中断了？没关系，明天重新开始。', 'preset', 'setback', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '中断了？没关系，明天重新开始。');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-s02', '进度落后不是终点，调整计划继续走。', 'preset', 'setback', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '进度落后不是终点，调整计划继续走。');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-s03', '遇到波折很正常，关键是能不能重启。', 'preset', 'setback', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '遇到波折很正常，关键是能不能重启。');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-s04', '今天没完成，明天还有机会补上。', 'preset', 'setback', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '今天没完成，明天还有机会补上。');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-s05', '进度慢了，考虑重新规划一下节奏。', 'preset', 'setback', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '进度慢了，考虑重新规划一下节奏。');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-s06', '连续被打断，不代表不能重新开始。', 'preset', 'setback', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '连续被打断，不代表不能重新开始。');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-s07', '进度有差距，但还有时间追回来。', 'preset', 'setback', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '进度有差距，但还有时间追回来。');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-s08', '中断只是暂停，不是放弃。', 'preset', 'setback', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '中断只是暂停，不是放弃。');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-s09', '进度落后，试试调整目标或增加投入。', 'preset', 'setback', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '进度落后，试试调整目标或增加投入。');

INSERT INTO encouragements (id, text, category, level, created_at)
SELECT 'preset-p1-s10', '今天没跟上，明天继续，别放弃。', 'preset', 'setback', '2026-07-20T00:00:00'
WHERE NOT EXISTS (SELECT 1 FROM encouragements WHERE text = '今天没跟上，明天继续，别放弃。');

-- ============================================================
-- P1-4: 用户偏好设置默认值（幂等插入）
-- ============================================================

INSERT OR IGNORE INTO settings (key, value) VALUES ('encouragement_enabled', 'true');
INSERT OR IGNORE INTO settings (key, value) VALUES ('encouragement_frequency', 'normal');
INSERT OR IGNORE INTO settings (key, value) VALUES ('encouragement_style', 'warm');
INSERT OR IGNORE INTO settings (key, value) VALUES ('encouragement_celebration_animation', 'true');
INSERT OR IGNORE INTO settings (key, value) VALUES ('encouragement_emoji_enabled', 'true');