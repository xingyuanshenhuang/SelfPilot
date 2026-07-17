-- P1-1 任务依赖关系
-- 记录任务之间的前置依赖：task_id 依赖 depends_on_id（depends_on_id 完成后 task_id 才可执行）
-- - UNIQUE(task_id, depends_on_id) 防止重复依赖
-- - ON DELETE CASCADE：任务删除时自动清理其依赖关系
-- - 防循环依赖由应用层 DFS 检测（set_task_dependency 命令）
CREATE TABLE IF NOT EXISTS task_dependencies (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL,
    depends_on_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE,
    FOREIGN KEY (depends_on_id) REFERENCES tasks(id) ON DELETE CASCADE,
    UNIQUE(task_id, depends_on_id)
);

CREATE INDEX IF NOT EXISTS idx_task_dep_task ON task_dependencies(task_id);
CREATE INDEX IF NOT EXISTS idx_task_dep_on ON task_dependencies(depends_on_id);
