-- 重新规划（阶段3）· 用户规划画像默认值
--
-- 依据《目标树重新规划功能重构设计文档》§5.2.3（原设计 023，因 021=split_plans
-- 已占用而顺延为 024）：
-- 以单行 JSON 形式存储（key = 'planning_profile'），兼容现有 settings 表
-- （key TEXT PRIMARY KEY, value TEXT NOT NULL）；此处仅登记默认值，
-- 实际读写复用 get_settings / set_setting 通道。
--
-- 一次性迁移，一经提交永不再改（校验和教训）。

INSERT OR IGNORE INTO settings (key, value) VALUES
    ('planning_profile',
     '{"weekday_hours":{"1":1.0,"2":1.0,"3":1.0,"4":1.0,"5":1.0,"6":3.0,"0":3.0},' ||
     '"weekend_factor":1.0,"prefer_morning":false}'),
    ('planning_auto_enabled', 'true'),
    ('planning_cooldown_hours', '6'),
    ('planning_min_impact_ratio', '0.10'),
    ('planning_max_intensity_hard', '1.5'),
    ('planning_smoothing',
     '{"qty_change_threshold":0.15,"date_move_threshold_days":1,"max_moves_per_run":0.30}');
