-- ============================================================================
-- SelfPilot 模拟数据库（首任务弹 / 里程碑功能测试）
-- ----------------------------------------------------------------------------
-- 用途：为「首任务鼓励语弹框」与「里程碑（最长连续 / 全部完成庆祝）」功能
--       提供一份可直接导入的结构完整、数据真实的模拟数据库。
--
-- 结构与生产环境一致：schema = migrations 001~017 应用后的最终形态。
--   · 项目表        → goals（含子目标=项目内的里程碑节点，parent_id 树结构）
--   · 任务表        → tasks（status: pending/partial/done/skipped）
--   · 里程碑表      → 生产无独立里程碑表，里程碑由后端计算：
--                     ① 全部目标完成（stats 百分比=100% → celebration）
--                     ② 接近/追平/超越历史最长连续（streak_service）
--   · 用户信息表    → 生产无独立 users 表（单用户桌面应用），用户偏好存 settings(key,value)
--   · 关联表        → task_dependencies / encouragement_show_log / encouragement_favorites
--
-- 日期说明：全部使用 date('now','localtime',±N) 相对今日生成，导入任何一天
--           运行都能命中「今日任务 / 连续天数」逻辑，无需手工改日期。
--
-- 使用方式（三选一）：
--   1) 独立建库：  python -c "import sqlite3;c=sqlite3.connect('mock_selfpilot.db');c.executescript(open('mock_selfpilot.sql',encoding='utf-8').read());c.commit();c.close()"
--   2) 接入应用（推荐）：先启动应用一次生成已迁移的真实库
--      （%APPDATA%\com.selfpilot.desktop\selfpilot.db），然后用 sqlite3/Python
--      对该库【只执行第 2 节起的数据部分】（不要执行第 1 节建表），重启应用即可测试。
--   3) 直接替换：用第 1 节 + 第 2 节生成独立 .db 后，配合应用内「设置-备份/恢复」
--      导入（恢复前应用会自动备份现库）。注意：若以独立 .db 直接覆盖应用库，
--      需保证该库含正确的 _sqlx_migrations 记录，否则启动迁移会因列已存在而失败。
--
-- 场景开关（第 3 节）：全局 streak 单一日志态，不同里程碑阶段用几行 UPDATE 切换。
-- ============================================================================

PRAGMA foreign_keys = ON;

-- ============================================================================
-- 第 1 节：建表（与生产最终 schema 一致）
-- ============================================================================

DROP TABLE IF EXISTS encouragement_favorites;
DROP TABLE IF EXISTS encouragement_show_log;
DROP TABLE IF EXISTS task_dependencies;
DROP TABLE IF EXISTS tasks;
DROP TABLE IF EXISTS stages;
DROP TABLE IF EXISTS goals;
DROP TABLE IF EXISTS encouragements;
DROP TABLE IF EXISTS settings;

-- 目标表（项目；一级节点 parent_id IS NULL，子目标=项目内里程碑节点）
CREATE TABLE goals (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    parent_id TEXT,
    path TEXT NOT NULL DEFAULT '',
    deadline TEXT,
    total_qty REAL NOT NULL DEFAULT 0,
    unit TEXT NOT NULL DEFAULT '',
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    daily_capacity REAL
);

-- 阶段表（migration 004 后已废弃，应用不再引用；保留表结构以与生产一致）
CREATE TABLE stages (
    id TEXT PRIMARY KEY,
    goal_id TEXT NOT NULL,
    name TEXT NOT NULL,
    parent_id TEXT,
    path TEXT NOT NULL,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    FOREIGN KEY (goal_id) REFERENCES goals(id) ON DELETE CASCADE
);

-- 任务表（三级节点，实际执行单元）
CREATE TABLE tasks (
    id TEXT PRIMARY KEY,
    goal_id TEXT NOT NULL,
    stage_id TEXT,
    parent_id TEXT,
    path TEXT NOT NULL,
    name TEXT NOT NULL,
    plan_date TEXT,
    plan_qty REAL NOT NULL DEFAULT 1,
    actual_qty REAL NOT NULL DEFAULT 0,
    unit TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK(status IN ('pending','partial','done','skipped')),
    is_manual INTEGER NOT NULL DEFAULT 0,
    source TEXT NOT NULL DEFAULT 'manual'
        CHECK(source IN ('auto','manual')),
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    overdue_date TEXT,
    estimated_hours REAL,
    FOREIGN KEY (goal_id) REFERENCES goals(id) ON DELETE CASCADE
);

CREATE INDEX idx_tasks_plan_date ON tasks(plan_date);
CREATE INDEX idx_tasks_goal_id ON tasks(goal_id);
CREATE INDEX idx_tasks_status ON tasks(status);
CREATE INDEX idx_tasks_overdue_date ON tasks(overdue_date);

-- 任务依赖关系
CREATE TABLE task_dependencies (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL,
    depends_on_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE,
    FOREIGN KEY (depends_on_id) REFERENCES tasks(id) ON DELETE CASCADE,
    UNIQUE(task_id, depends_on_id)
);

-- 鼓励语库
CREATE TABLE encouragements (
    id TEXT PRIMARY KEY,
    text TEXT NOT NULL,
    category TEXT NOT NULL DEFAULT 'custom'
        CHECK(category IN ('preset','custom')),
    level TEXT NOT NULL DEFAULT 'normal'
        CHECK(level IN ('normal','advanced','highlight','celebration','setback','longest_streak')),
    created_at TEXT NOT NULL,
    context_tags TEXT DEFAULT '{}',
    hidden INTEGER DEFAULT 0,
    weight REAL DEFAULT 1.0,
    sort_order INTEGER DEFAULT 0,
    style TEXT NOT NULL DEFAULT 'warm'
);

-- 鼓励语展示历史（最近 5 条去重窗口）
CREATE TABLE encouragement_show_log (
    id TEXT PRIMARY KEY,
    encouragement_id TEXT NOT NULL,
    shown_at TEXT NOT NULL,
    trigger_source TEXT NOT NULL,
    closed_at TEXT,
    view_duration INTEGER DEFAULT 0,
    FOREIGN KEY (encouragement_id) REFERENCES encouragements(id) ON DELETE CASCADE
);

-- 鼓励语收藏
CREATE TABLE encouragement_favorites (
    id TEXT PRIMARY KEY,
    encouragement_id TEXT NOT NULL,
    favorited_at TEXT NOT NULL,
    FOREIGN KEY (encouragement_id) REFERENCES encouragements(id) ON DELETE CASCADE
);

-- 设置项（key-value）
CREATE TABLE settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- ============================================================================
-- 第 2 节：测试数据
-- 场景总览：
--   S1 新用户首次登录      → 目标 C「晨读习惯」：今日刚建、仅今日待办、无历史
--   S2 多里程碑项目        → 目标 A「百日健身」：子目标 A1已完成/A2进行中/A3未开始
--   S3 不同优先级任务      → 用 sort_order 编码（生产无独立 priority 字段）
--   S4 已完成/未完成/部分/跳过 → 四种 status 全覆盖
--   S5 时间跨越多个阶段    → 过去(已完成/逾期)/今日(混合)/未来(计划)
--   S6 首任务弹（默认态）  → 今日全部 pending；完成第 1 个 → current=3，远离里程碑 → 弹首任务
--   S7 里程碑              → 见第 3 节开关，可切到接近/追平/超越历史最长连续
-- ============================================================================

-- ----------------------------------------------------------------------------
-- 2.1 settings（用户偏好；key 与应用 settings.rs 完全一致）
-- ----------------------------------------------------------------------------
INSERT INTO settings (key, value) VALUES
    ('theme', 'light'),
    ('encouragement_enabled', 'true'),
    ('encouragement_frequency', 'normal'),          -- normal=首任务+里程碑（默认测试档）
    ('encouragement_style', 'warm'),
    ('encouragement_celebration_animation', 'true'),
    ('encouragement_emoji_enabled', 'true');

-- ----------------------------------------------------------------------------
-- 2.2 goals（项目 / 子目标=里程碑节点）
-- ----------------------------------------------------------------------------
INSERT INTO goals (id, name, parent_id, path, deadline, total_qty, unit, sort_order, created_at, daily_capacity) VALUES
    -- 目标 A：多里程碑项目（3 个子目标处于不同阶段）
    ('mock-goal-a',  '百日健身计划',        NULL, '/mock-goal-a',            date('now','localtime','+90 day'), 100, '天', 1, date('now','localtime','-20 day') || 'T08:00:00', 1.0),
    ('mock-goal-a1', '第一阶段·基础体能',    'mock-goal-a', '/mock-goal-a/mock-goal-a1', date('now','localtime','-7 day'),  30, '天', 1, date('now','localtime','-20 day') || 'T08:00:00', NULL),
    ('mock-goal-a2', '第二阶段·增肌塑形',    'mock-goal-a', '/mock-goal-a/mock-goal-a2', date('now','localtime','+30 day'), 30, '天', 2, date('now','localtime','-10 day') || 'T08:00:00', 1.0),
    ('mock-goal-a3', '第三阶段·耐力突破',    'mock-goal-a', '/mock-goal-a/mock-goal-a3', date('now','localtime','+80 day'), 40, '天', 3, date('now','localtime','-2 day')  || 'T08:00:00', NULL),
    -- 目标 B：部分完成项目
    ('mock-goal-b',  '英语学习',            NULL, '/mock-goal-b',            date('now','localtime','+60 day'), 100, '词', 2, date('now','localtime','-30 day') || 'T08:00:00', 1.0),
    ('mock-goal-b1', '词汇复习',            'mock-goal-b', '/mock-goal-b/mock-goal-b1', date('now','localtime','-15 day'), 50, '词', 1, date('now','localtime','-30 day') || 'T08:00:00', NULL),
    ('mock-goal-b2', '新课程跟读',          'mock-goal-b', '/mock-goal-b/mock-goal-b2', date('now','localtime','+30 day'), 50, '词', 2, date('now','localtime','-8 day')  || 'T08:00:00', 0.5),
    -- 目标 C：新用户首次登录场景（今日新建、仅今日待办、无历史）
    ('mock-goal-c',  '晨读习惯',            NULL, '/mock-goal-c',            date('now','localtime','+30 day'), 30, '天', 3, date('now','localtime') || 'T07:30:00', 1.0);

-- ----------------------------------------------------------------------------
-- 2.3 tasks（任务；覆盖四种状态 / 逾期 / 未来 / 不同 sort_order 优先级）
-- ----------------------------------------------------------------------------
INSERT INTO tasks (id, goal_id, stage_id, parent_id, path, name, plan_date, plan_qty, actual_qty, unit, status, is_manual, source, sort_order, created_at, overdue_date, estimated_hours) VALUES
    -- ===== 目标 A1（已完成里程碑，历史 4 连完成 + 恢复期休息作 streak 断点） =====
    ('mock-task-a1-1', 'mock-goal-a1', NULL, NULL, '/mock-goal-a1/mock-task-a1-1', '深蹲训练',   date('now','localtime','-10 day'), 1, 1, '次', 'done',    1, 'manual', 1, date('now','localtime','-20 day') || 'T08:00:00', NULL, 0.5),
    ('mock-task-a1-2', 'mock-goal-a1', NULL, NULL, '/mock-goal-a1/mock-task-a1-2', '卧推训练',   date('now','localtime','-9 day'),  1, 1, '次', 'done',    1, 'manual', 2, date('now','localtime','-20 day') || 'T08:00:00', NULL, 0.5),
    ('mock-task-a1-3', 'mock-goal-a1', NULL, NULL, '/mock-goal-a1/mock-task-a1-3', '引体向上',   date('now','localtime','-8 day'),  1, 1, '次', 'done',    1, 'manual', 3, date('now','localtime','-20 day') || 'T08:00:00', NULL, 0.3),
    ('mock-task-a1-4', 'mock-goal-a1', NULL, NULL, '/mock-goal-a1/mock-task-a1-4', '核心训练',   date('now','localtime','-7 day'),  1, 1, '次', 'done',    1, 'manual', 4, date('now','localtime','-20 day') || 'T08:00:00', NULL, 0.4),
    ('mock-task-a1-5', 'mock-goal-a1', NULL, NULL, '/mock-goal-a1/mock-task-a1-5', '恢复期休息', date('now','localtime','-6 day'),  1, 0, '次', 'pending', 1, 'manual', 5, date('now','localtime','-15 day') || 'T08:00:00', date('now','localtime','-6 day'), 0),
    ('mock-task-a1-6', 'mock-goal-a1', NULL, NULL, '/mock-goal-a1/mock-task-a1-6', '拉伸放松',   date('now','localtime','-11 day'), 1, 0, '次', 'skipped', 1, 'manual', 6, date('now','localtime','-20 day') || 'T08:00:00', NULL, 0.2),

    -- ===== 目标 A2（进行中里程碑：过去完成 2 天 + 今日待办 2 个 + 未来 3 个） =====
    ('mock-task-a2-1', 'mock-goal-a2', NULL, NULL, '/mock-goal-a2/mock-task-a2-1', '哑铃训练',     date('now','localtime','-2 day'), 1, 1, '次', 'done',    1, 'manual', 1, date('now','localtime','-10 day') || 'T08:00:00', NULL, 0.5),
    ('mock-task-a2-2', 'mock-goal-a2', NULL, NULL, '/mock-goal-a2/mock-task-a2-2', '有氧操',       date('now','localtime','-1 day'), 1, 1, '次', 'done',    1, 'manual', 2, date('now','localtime','-10 day') || 'T08:00:00', NULL, 0.4),
    ('mock-task-a2-5', 'mock-goal-a2', NULL, NULL, '/mock-goal-a2/mock-task-a2-5', '力量恢复训练', date('now','localtime','-3 day'), 1, 0, '次', 'pending', 1, 'manual', 5, date('now','localtime','-10 day') || 'T08:00:00', date('now','localtime','-3 day'), 0.5), -- 逾期断点
    ('mock-task-a2-3', 'mock-goal-a2', NULL, NULL, '/mock-goal-a2/mock-task-a2-3', '力量训练',     date('now','localtime'),           1, 0, '次', 'pending', 1, 'manual', 3, date('now','localtime','-10 day') || 'T08:00:00', NULL, 0.6),
    ('mock-task-a2-4', 'mock-goal-a2', NULL, NULL, '/mock-goal-a2/mock-task-a2-4', '拉伸放松',     date('now','localtime'),           1, 0, '次', 'pending', 1, 'manual', 4, date('now','localtime','-10 day') || 'T08:00:00', NULL, 0.2),
    ('mock-task-a2-6', 'mock-goal-a2', NULL, NULL, '/mock-goal-a2/mock-task-a2-6', '增肌训练',     date('now','localtime','+2 day'),  1, 0, '次', 'pending', 1, 'manual', 6, date('now','localtime','-10 day') || 'T08:00:00', NULL, 0.7), -- 依赖 a2-3
    ('mock-task-a2-7', 'mock-goal-a2', NULL, NULL, '/mock-goal-a2/mock-task-a2-7', '高强度间歇',   date('now','localtime','+4 day'),  1, 0, '次', 'pending', 1, 'manual', 7, date('now','localtime','-10 day') || 'T08:00:00', NULL, 0.5),
    ('mock-task-a2-8', 'mock-goal-a2', NULL, NULL, '/mock-goal-a2/mock-task-a2-8', '热身跳绳',     date('now','localtime','+1 day'),  1, 0, '次', 'pending', 1, 'manual', 99, date('now','localtime','-10 day') || 'T08:00:00', NULL, 0.3), -- 低优先级(sort_order=99)

    -- ===== 目标 A3（未开始里程碑：全部未来计划） =====
    ('mock-task-a3-1', 'mock-goal-a3', NULL, NULL, '/mock-goal-a3/mock-task-a3-1', '长跑训练', date('now','localtime','+7 day'),  1, 0, '次', 'pending', 1, 'manual', 1, date('now','localtime','-2 day') || 'T08:00:00', NULL, 1.0),
    ('mock-task-a3-2', 'mock-goal-a3', NULL, NULL, '/mock-goal-a3/mock-task-a3-2', '登山训练', date('now','localtime','+9 day'),  1, 0, '次', 'pending', 1, 'manual', 2, date('now','localtime','-2 day') || 'T08:00:00', NULL, 1.5),
    ('mock-task-a3-3', 'mock-goal-a3', NULL, NULL, '/mock-goal-a3/mock-task-a3-3', '体能测试', date('now','localtime','+14 day'), 1, 0, '次', 'pending', 1, 'manual', 3, date('now','localtime','-2 day') || 'T08:00:00', NULL, 0.5),

    -- ===== 目标 A 直属任务（顶层项目直接任务，完成度统计口径） =====
    ('mock-task-a-direct-1', 'mock-goal-a', NULL, NULL, '/mock-goal-a/mock-task-a-direct-1', '每周体测', date('now','localtime','+3 day'), 1, 0, '次', 'pending', 1, 'manual', 1, date('now','localtime','-20 day') || 'T08:00:00', NULL, 0.5),

    -- ===== 目标 B1（已完成里程碑：历史 6 连完成 + 1 个逾期断点任务） =====
    ('mock-task-b1-1', 'mock-goal-b1', NULL, NULL, '/mock-goal-b1/mock-task-b1-1', '词汇复习第1组', date('now','localtime','-20 day'), 5, 5, '词', 'done',    1, 'manual', 1, date('now','localtime','-30 day') || 'T08:00:00', NULL, 0.4),
    ('mock-task-b1-2', 'mock-goal-b1', NULL, NULL, '/mock-goal-b1/mock-task-b1-2', '词汇复习第2组', date('now','localtime','-19 day'), 5, 5, '词', 'done',    1, 'manual', 2, date('now','localtime','-30 day') || 'T08:00:00', NULL, 0.4),
    ('mock-task-b1-3', 'mock-goal-b1', NULL, NULL, '/mock-goal-b1/mock-task-b1-3', '词汇复习第3组', date('now','localtime','-18 day'), 5, 5, '词', 'done',    1, 'manual', 3, date('now','localtime','-30 day') || 'T08:00:00', NULL, 0.4),
    ('mock-task-b1-4', 'mock-goal-b1', NULL, NULL, '/mock-goal-b1/mock-task-b1-4', '词汇复习第4组', date('now','localtime','-17 day'), 5, 5, '词', 'done',    1, 'manual', 4, date('now','localtime','-30 day') || 'T08:00:00', NULL, 0.4),
    ('mock-task-b1-5', 'mock-goal-b1', NULL, NULL, '/mock-goal-b1/mock-task-b1-5', '词汇复习第5组', date('now','localtime','-16 day'), 5, 5, '词', 'done',    1, 'manual', 5, date('now','localtime','-30 day') || 'T08:00:00', NULL, 0.4),
    ('mock-task-b1-6', 'mock-goal-b1', NULL, NULL, '/mock-goal-b1/mock-task-b1-6', '词汇复习第6组', date('now','localtime','-15 day'), 5, 5, '词', 'done',    1, 'manual', 6, date('now','localtime','-30 day') || 'T08:00:00', NULL, 0.4),
    ('mock-task-b1-7', 'mock-goal-b1', NULL, NULL, '/mock-goal-b1/mock-task-b1-7', '听力训练',     date('now','localtime','-12 day'), 2, 0, '次', 'pending', 1, 'manual', 7, date('now','localtime','-30 day') || 'T08:00:00', date('now','localtime','-12 day'), 0.5), -- 逾期断点

    -- ===== 目标 B 直属任务（顶层项目直接任务，完成度统计口径，保证庆祝可触发） =====
    ('mock-task-b-direct-1', 'mock-goal-b', NULL, NULL, '/mock-goal-b/mock-task-b-direct-1', '每周听力测试', date('now','localtime','+5 day'), 1, 0, '次', 'pending', 1, 'manual', 1, date('now','localtime','-30 day') || 'T08:00:00', NULL, 0.4),

    -- ===== 目标 B2（部分完成：done/partial/pending 混合） =====
    ('mock-task-b2-1', 'mock-goal-b2', NULL, NULL, '/mock-goal-b2/mock-task-b2-1', '新课程跟读1', date('now','localtime','-5 day'), 3, 3, '词', 'done',    1, 'manual', 1, date('now','localtime','-8 day') || 'T08:00:00', NULL, 0.3),
    ('mock-task-b2-2', 'mock-goal-b2', NULL, NULL, '/mock-goal-b2/mock-task-b2-2', '新课程跟读2', date('now','localtime','-4 day'), 3, 3, '词', 'done',    1, 'manual', 2, date('now','localtime','-8 day') || 'T08:00:00', NULL, 0.3),
    ('mock-task-b2-3', 'mock-goal-b2', NULL, NULL, '/mock-goal-b2/mock-task-b2-3', '新课程跟读3', date('now','localtime','-1 day'), 3, 1, '词', 'partial', 1, 'manual', 3, date('now','localtime','-8 day') || 'T08:00:00', NULL, 0.3),
    ('mock-task-b2-4', 'mock-goal-b2', NULL, NULL, '/mock-goal-b2/mock-task-b2-4', '新课程跟读4', date('now','localtime'),           3, 0, '词', 'pending', 1, 'manual', 4, date('now','localtime','-8 day') || 'T08:00:00', NULL, 0.3),
    ('mock-task-b2-5', 'mock-goal-b2', NULL, NULL, '/mock-goal-b2/mock-task-b2-5', '新课程跟读5', date('now','localtime','+1 day'),  3, 0, '词', 'pending', 1, 'manual', 5, date('now','localtime','-8 day') || 'T08:00:00', NULL, 0.3),

    -- ===== 目标 C（新用户首次登录场景：今日新建 + 仅今日 3 个待办，无历史） =====
    ('mock-task-c-1', 'mock-goal-c', NULL, NULL, '/mock-goal-c/mock-task-c-1', '晨读 30 分钟', date('now','localtime'), 1, 0, '次', 'pending', 1, 'manual', 1, date('now','localtime') || 'T07:30:00', NULL, 0.5),
    ('mock-task-c-2', 'mock-goal-c', NULL, NULL, '/mock-goal-c/mock-task-c-2', '晨写 200 字',  date('now','localtime'), 1, 0, '次', 'pending', 1, 'manual', 2, date('now','localtime') || 'T07:30:00', NULL, 0.3), -- 依赖 c-1
    ('mock-task-c-3', 'mock-goal-c', NULL, NULL, '/mock-goal-c/mock-task-c-3', '晨练 15 分钟', date('now','localtime'), 1, 0, '次', 'pending', 1, 'manual', 3, date('now','localtime') || 'T07:30:00', NULL, 0.3); -- 依赖 c-2

-- ----------------------------------------------------------------------------
-- 2.4 task_dependencies（依赖链，测试前置未完成时阻塞完成）
--   c-1 晨读 → c-2 晨写 → c-3 晨练（链式）；a2-3 力量训练 → a2-6 增肌训练
-- ----------------------------------------------------------------------------
INSERT INTO task_dependencies (id, task_id, depends_on_id, created_at) VALUES
    ('mock-dep-1', 'mock-task-c-2', 'mock-task-c-1', date('now','localtime') || 'T07:30:00'),
    ('mock-dep-2', 'mock-task-c-3', 'mock-task-c-2', date('now','localtime') || 'T07:30:00'),
    ('mock-dep-3', 'mock-task-a2-6', 'mock-task-a2-3', date('now','localtime','-10 day') || 'T08:00:00');

-- ----------------------------------------------------------------------------
-- 2.5 encouragements（精简但覆盖全部 level + 三种 style，里程碑文案齐备）
--    完整 78+ 预设文案由应用迁移自动灌入，此处仅需覆盖测试所需等级/风格。
-- ----------------------------------------------------------------------------
INSERT INTO encouragements (id, text, category, level, created_at, context_tags, hidden, weight, sort_order, style) VALUES
    ('mock-enc-n1', '今天又前进了一步。',           'preset', 'normal',          date('now','localtime','-1 day') || 'T08:00:00', '{}', 0, 1.0, 1, 'warm'),
    ('mock-enc-n2', '每个完成的任务，都是进步的证据。','preset','normal',         date('now','localtime','-1 day') || 'T08:00:00', '{}', 0, 1.0, 2, 'professional'),
    ('mock-enc-n3', '做到了，就值得记录。',         'preset', 'normal',          date('now','localtime','-1 day') || 'T08:00:00', '{}', 0, 1.0, 3, 'minimal'),
    ('mock-enc-a1', '三天不间断，习惯已成型。',     'preset', 'advanced',        date('now','localtime','-1 day') || 'T08:00:00', '{}', 0, 1.0, 4, 'warm'),
    ('mock-enc-h1', '一周完成！习惯已成自然。',     'preset', 'highlight',       date('now','localtime','-1 day') || 'T08:00:00', '{}', 0, 1.0, 5, 'warm'),
    ('mock-enc-c1', '全部完成！这一刻属于坚持的你。', 'preset','celebration',     date('now','localtime','-1 day') || 'T08:00:00', '{}', 0, 1.0, 6, 'warm'),
    ('mock-enc-s1', '中断了？没关系，明天重新开始。', 'preset','setback',         date('now','localtime','-1 day') || 'T08:00:00', '{}', 0, 1.0, 7, 'warm'),
    -- 最长连续里程碑文案（里程碑弹框实际取用；context_tags 对应 milestone_type）
    ('mock-enc-l1', '距离历史记录只差一步，你曾经坚持过，这次也能！', 'preset', 'longest_streak', date('now','localtime','-1 day') || 'T08:00:00', '{"milestone":"approaching"}', 0, 1.0, 8, 'warm'),
    ('mock-enc-l2', '追平了自己的历史记录！你已经是自己最好的对手了。', 'preset', 'longest_streak', date('now','localtime','-1 day') || 'T08:00:00', '{"milestone":"equal"}', 0, 1.0, 9, 'warm'),
    ('mock-enc-l3', '超越历史记录！每次突破都是在重新定义自己的极限。', 'preset', 'longest_streak', date('now','localtime','-1 day') || 'T08:00:00', '{"milestone":"breakthrough"}', 0, 1.0, 10, 'warm'),
    ('mock-enc-l4', '大幅超越历史记录！你正在书写一个全新的自己。', 'preset', 'longest_streak', date('now','localtime','-1 day') || 'T08:00:00', '{"milestone":"major"}', 0, 1.0, 11, 'warm'),
    ('mock-enc-l5', '历史已经被你甩在身后，新的记录在等着你书写。', 'preset', 'longest_streak', date('now','localtime','-1 day') || 'T08:00:00', '{"milestone":"continue"}', 0, 1.0, 12, 'warm'),
    ('mock-enc-custom1', '自定义：今天也要加油鸭！', 'custom', 'normal',        date('now','localtime','-1 day') || 'T08:00:00', '{}', 0, 1.0, 13, 'warm');

-- ----------------------------------------------------------------------------
-- 2.6 encouragement_show_log（最近展示历史，测试去重窗口）
-- ----------------------------------------------------------------------------
INSERT INTO encouragement_show_log (id, encouragement_id, shown_at, trigger_source, closed_at, view_duration) VALUES
    ('mock-log-1', 'mock-enc-n1', date('now','localtime') || 'T06:30:00', 'complete_first', NULL, NULL),
    ('mock-log-2', 'mock-enc-a1', date('now','localtime','-1 day') || 'T10:00:00', 'complete_normal', NULL, NULL),
    ('mock-log-3', 'mock-enc-h1', date('now','localtime','-1 day') || 'T12:00:00', 'dashboard_banner', date('now','localtime','-1 day') || 'T12:00:08', 8),
    ('mock-log-4', 'mock-enc-l1', date('now','localtime','-2 day') || 'T09:00:00', 'complete_first', NULL, NULL),
    ('mock-log-5', 'mock-enc-custom1', date('now','localtime','-3 day') || 'T15:00:00', 'complete_normal', date('now','localtime','-3 day') || 'T15:00:05', 5);

-- ----------------------------------------------------------------------------
-- 2.7 encouragement_favorites（收藏，供加权抽取）
-- ----------------------------------------------------------------------------
INSERT INTO encouragement_favorites (id, encouragement_id, favorited_at) VALUES
    ('mock-fav-1', 'mock-enc-a1', date('now','localtime','-2 day') || 'T11:00:00'),
    ('mock-fav-2', 'mock-enc-custom1', date('now','localtime','-1 day') || 'T11:00:00');

-- ============================================================================
-- 第 3 节：里程碑场景切换（默认库为「首任务弹」干净态，需切换时执行对应 UPDATE）
--
-- 默认态说明（全局 streak）：
--   · 历史最长连续 longest = 6（b1 于 -20..-15 天 6 连完成）
--   · 完成今日第 1 个任务后 current = 3（今日 + 昨/前天已完成，-3 天断点）
--   · current(3) < longest(6) - 2 = 4 → 不触发里程碑 → 首任务弹干净可测
--
-- 切换脚本（在应用关闭时对 .db 执行，再启动应用完成今日任务验证）：
--
--   ① 接近历史（current=5 vs longest=6 → 触发「接近历史」里程碑弹框 🚀）：
--        UPDATE tasks SET status='done', actual_qty=plan_qty WHERE id='mock-task-a2-5';
--        UPDATE tasks SET status='pending', actual_qty=0 WHERE id='mock-task-b2-1';
--
--   ② 追平历史（current=6=longest → 触发「追平历史」里程碑弹框 🚀）：
--        UPDATE tasks SET status='done', actual_qty=plan_qty WHERE id='mock-task-a2-5';
--
--   注：因 calc_streak 会做 longest=max(longest,current) 的合并，「超越历史」
--       （current>longest）分支在生产逻辑中实际不可达；补完长链断点只会变成
--       「追平历史」。
--
--   ③ 全部完成庆祝（全部顶层目标直属任务完成 → 完成最后 1 个触发 celebration+彩带）：
--        UPDATE tasks SET status='done', actual_qty=plan_qty WHERE status IN ('pending','partial');
--        UPDATE tasks SET status='done', actual_qty=plan_qty
--            WHERE id IN ('mock-task-a-direct-1','mock-task-b-direct-1');
--        -- 然后仅保留 1 个今日任务为 pending 作为「最后一步」，完成它即触发庆祝：
--        UPDATE tasks SET status='pending', actual_qty=0 WHERE id='mock-task-c-1';
-- ============================================================================
