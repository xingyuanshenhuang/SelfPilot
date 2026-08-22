-- Migration 015: 鼓励语库幂等补录（R-09）
-- 背景：009_encouragement_p1.sql 采用 INSERT ... WHERE NOT EXISTS 模式，
--       仅在迁移首次执行时插入 52 条预设鼓励语。若用户后续手动删除了某条，
--       该条不会随迁移重演而恢复。本迁移用多值 INSERT OR IGNORE 幂等补录，
--       仅插入缺失项（按固定 id 判定），不重复、不覆盖用户已有数据。
--
-- 补录范围：009 中新增的 52 条预设文案
--   normal 7 + advanced 14 + highlight 15 + celebration 6 + setback 10
-- 表结构兼容：列 (id, text, category, level, created_at)，其余列走默认值
--   （context_tags='{}'、hidden=0、weight=1.0、sort_order=0）

-- ============================================================
-- normal 等级（7 条）
-- ============================================================
INSERT OR IGNORE INTO encouragements (id, text, category, level, created_at) VALUES
('preset-p1-n01', '又完成一项，稳扎稳打。', 'preset', 'normal', '2026-07-20T00:00:00'),
('preset-p1-n02', '这一步，很实在。', 'preset', 'normal', '2026-07-20T00:00:00'),
('preset-p1-n03', '做到了，就值得记录。', 'preset', 'normal', '2026-07-20T00:00:00'),
('preset-p1-n04', '完成本身就是一种积累。', 'preset', 'normal', '2026-07-20T00:00:00'),
('preset-p1-n05', '不急不躁，一步步来。', 'preset', 'normal', '2026-07-20T00:00:00'),
('preset-p1-n06', '你正在按自己的节奏前进。', 'preset', 'normal', '2026-07-20T00:00:00'),
('preset-p1-n07', '每个完成的任务，都是进步的证据。', 'preset', 'normal', '2026-07-20T00:00:00');

-- ============================================================
-- advanced 等级（14 条）
-- ============================================================
INSERT OR IGNORE INTO encouragements (id, text, category, level, created_at) VALUES
('preset-p1-a01', '三天不间断，习惯已成型。', 'preset', 'advanced', '2026-07-20T00:00:00'),
('preset-p1-a02', '三天打卡，你已经超过多数人的坚持。', 'preset', 'advanced', '2026-07-20T00:00:00'),
('preset-p1-a03', '连续三天，自律正在成为本能。', 'preset', 'advanced', '2026-07-20T00:00:00'),
('preset-p1-a04', '三天的坚持，胜过三天的空想。', 'preset', 'advanced', '2026-07-20T00:00:00'),
('preset-p1-a05', '你用行动证明了：能坚持。', 'preset', 'advanced', '2026-07-20T00:00:00'),
('preset-p1-a06', '三天累计，不是运气，是选择。', 'preset', 'advanced', '2026-07-20T00:00:00'),
('preset-p1-a07', '你已经连续三天完成计划。', 'preset', 'advanced', '2026-07-20T00:00:00'),
('preset-p1-a08', '三天的小目标，累积成大改变。', 'preset', 'advanced', '2026-07-20T00:00:00'),
('preset-p1-a09', '三天坚持下来，已经值得给自己点个赞。', 'preset', 'advanced', '2026-07-20T00:00:00'),
('preset-p1-a10', '连续完成三天，节奏找到了。', 'preset', 'advanced', '2026-07-20T00:00:00'),
('preset-p1-a11', '三天连胜，你的执行力在说话。', 'preset', 'advanced', '2026-07-20T00:00:00'),
('preset-p1-a12', '连续三天，你已经建立起一种秩序。', 'preset', 'advanced', '2026-07-20T00:00:00'),
('preset-p1-a13', '三天不空，习惯正在发芽。', 'preset', 'advanced', '2026-07-20T00:00:00'),
('preset-p1-a14', '三天打卡完成，执行力在线。', 'preset', 'advanced', '2026-07-20T00:00:00');

-- ============================================================
-- highlight 等级（15 条）
-- ============================================================
INSERT OR IGNORE INTO encouragements (id, text, category, level, created_at) VALUES
('preset-p1-h01', '一周完成！习惯已成自然。', 'preset', 'highlight', '2026-07-20T00:00:00'),
('preset-p1-h02', '七天坚持，你已经超越多数人的毅力。', 'preset', 'highlight', '2026-07-20T00:00:00'),
('preset-p1-h03', '连续一周打卡，自律已是你的标签。', 'preset', 'highlight', '2026-07-20T00:00:00'),
('preset-p1-h04', '七天不间断，积累的力量可见了。', 'preset', 'highlight', '2026-07-20T00:00:00'),
('preset-p1-h05', '一周的坚持，让你比上周更强。', 'preset', 'highlight', '2026-07-20T00:00:00'),
('preset-p1-h06', '七日打卡完成，你已经证明了自己。', 'preset', 'highlight', '2026-07-20T00:00:00'),
('preset-p1-h07', '连续七天，时间开始在你这边。', 'preset', 'highlight', '2026-07-20T00:00:00'),
('preset-p1-h08', '一周的执行，胜过无数个计划。', 'preset', 'highlight', '2026-07-20T00:00:00'),
('preset-p1-h09', '七天连胜，习惯已扎根。', 'preset', 'highlight', '2026-07-20T00:00:00'),
('preset-p1-h10', '一周打卡达成，你可以继续保持。', 'preset', 'highlight', '2026-07-20T00:00:00'),
('preset-p1-h11', '七天的坚持，你对目标的掌控更强了。', 'preset', 'highlight', '2026-07-20T00:00:00'),
('preset-p1-h12', '一周的积累，让你更接近目标。', 'preset', 'highlight', '2026-07-20T00:00:00'),
('preset-p1-h13', '连续七天完成，执行力已经稳定。', 'preset', 'highlight', '2026-07-20T00:00:00'),
('preset-p1-h14', '一周不间断，自律已成习惯。', 'preset', 'highlight', '2026-07-20T00:00:00'),
('preset-p1-h15', '七天打卡成功，你用行动验证了可能。', 'preset', 'highlight', '2026-07-20T00:00:00');

-- ============================================================
-- celebration 等级（6 条）
-- ============================================================
INSERT OR IGNORE INTO encouragements (id, text, category, level, created_at) VALUES
('preset-p1-c01', '全部完成！这一刻属于坚持的你。', 'preset', 'celebration', '2026-07-20T00:00:00'),
('preset-p1-c02', '目标达成！你的执行力值得被记住。', 'preset', 'celebration', '2026-07-20T00:00:00'),
('preset-p1-c03', '全部目标完成，你已经证明了自己能做到。', 'preset', 'celebration', '2026-07-20T00:00:00'),
('preset-p1-c04', '征程结束，但你的自律不会停。', 'preset', 'celebration', '2026-07-20T00:00:00'),
('preset-p1-c05', '所有目标达成，这是你选择的结果。', 'preset', 'celebration', '2026-07-20T00:00:00'),
('preset-p1-c06', '圆满完成！这一刻是对过往坚持的最好回报。', 'preset', 'celebration', '2026-07-20T00:00:00');

-- ============================================================
-- setback 等级（10 条）
-- ============================================================
INSERT OR IGNORE INTO encouragements (id, text, category, level, created_at) VALUES
('preset-p1-s01', '中断了？没关系，明天重新开始。', 'preset', 'setback', '2026-07-20T00:00:00'),
('preset-p1-s02', '进度落后不是终点，调整计划继续走。', 'preset', 'setback', '2026-07-20T00:00:00'),
('preset-p1-s03', '遇到波折很正常，关键是能不能重启。', 'preset', 'setback', '2026-07-20T00:00:00'),
('preset-p1-s04', '今天没完成，明天还有机会补上。', 'preset', 'setback', '2026-07-20T00:00:00'),
('preset-p1-s05', '进度慢了，考虑重新规划一下节奏。', 'preset', 'setback', '2026-07-20T00:00:00'),
('preset-p1-s06', '连续被打断，不代表不能重新开始。', 'preset', 'setback', '2026-07-20T00:00:00'),
('preset-p1-s07', '进度有差距，但还有时间追回来。', 'preset', 'setback', '2026-07-20T00:00:00'),
('preset-p1-s08', '中断只是暂停，不是放弃。', 'preset', 'setback', '2026-07-20T00:00:00'),
('preset-p1-s09', '进度落后，试试调整目标或增加投入。', 'preset', 'setback', '2026-07-20T00:00:00'),
('preset-p1-s10', '今天没跟上，明天继续，别放弃。', 'preset', 'setback', '2026-07-20T00:00:00');