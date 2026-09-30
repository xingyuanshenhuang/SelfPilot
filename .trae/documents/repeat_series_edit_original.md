# 目标树：任意重复任务"编辑原任务"并级联更新

## Context

用户添加带重复规则的任务时，会按频率生成多个实例（如"背单词 - 第1次"）。当前实例间没有系列关联，"编辑"只改单条实例；要修改原任务设置只能删除全部再重建，繁琐耗时。

目标：任意重复任务实例的节点菜单新增「编辑原任务」，弹出与"添加任务"相同的弹窗（预填原任务设置：名称/数量/单位/频率/起止日期/周几/每月几号），保存后级联更新所有实例。

已确认的决策：
1. 编辑范围：名称/数量/单位 + 重复规则（频率/日期范围/周几/每月几号）。
2. 修改重复规则时：保留已完成实例（不动进度记录），删除未完成实例并按新规则重建（跳过与保留实例同日的日期）。
3. 历史重复任务：迁移时自动识别回填（按 `goal_id+created_at` 分组 + 名称含" - 第N次"特征），新老任务都可用。

## 数据模型：迁移 018_repeat_series.sql（新建）

```sql
CREATE TABLE IF NOT EXISTS repeat_series (
    id TEXT PRIMARY KEY,
    goal_id TEXT NOT NULL,
    base_name TEXT NOT NULL,
    plan_qty REAL NOT NULL DEFAULT 1,
    unit TEXT NOT NULL DEFAULT '',
    frequency TEXT NOT NULL DEFAULT 'daily',
    start_date TEXT NOT NULL,
    end_date TEXT,
    weekdays TEXT NOT NULL DEFAULT '[]',   -- JSON 数组
    month_days TEXT NOT NULL DEFAULT '[]', -- JSON 数组
    created_at TEXT NOT NULL,
    FOREIGN KEY (goal_id) REFERENCES goals(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_repeat_series_goal_id ON repeat_series(goal_id);
CREATE INDEX IF NOT EXISTS idx_repeat_series_goal_created ON repeat_series(goal_id, created_at);

ALTER TABLE tasks ADD COLUMN repeat_series_id TEXT;
CREATE INDEX IF NOT EXISTS idx_tasks_repeat_series_id ON tasks(repeat_series_id);

-- 历史回填：识别命名含 " - 第N次" 的自动任务（智能拆解是 " - 第N天"，不受影响）
INSERT INTO repeat_series (id, goal_id, base_name, plan_qty, unit, frequency, start_date, end_date, weekdays, month_days, created_at)
SELECT lower(hex(randomblob(16))), goal_id,
       substr(name, 1, instr(name, ' - 第') - 1),
       min(plan_qty), min(unit), 'daily',
       min(plan_date), max(plan_date), '[]', '[]', created_at
FROM tasks
WHERE source = 'auto' AND instr(name, ' - 第') > 0 AND name LIKE '% - 第%次'
GROUP BY goal_id, created_at, substr(name, 1, instr(name, ' - 第') - 1)
HAVING COUNT(*) > 1;

UPDATE tasks SET repeat_series_id = (
    SELECT s.id FROM repeat_series s
    WHERE s.goal_id = tasks.goal_id AND s.created_at = tasks.created_at
      AND s.base_name = substr(tasks.name, 1, instr(tasks.name, ' - 第') - 1)
)
WHERE source = 'auto' AND instr(name, ' - 第') > 0 AND name LIKE '% - 第%次'
  AND (SELECT COUNT(*) FROM repeat_series s
       WHERE s.goal_id = tasks.goal_id AND s.created_at = tasks.created_at
         AND s.base_name = substr(tasks.name, 1, instr(tasks.name, ' - 第') - 1)) = 1;
```

回填后旧任务的 `repeat_series_id` 即被填充，菜单直接可用；历史频率无法还原，默认 daily、起止取 min/max(plan_date)，用户编辑时可改正。

## 后端改动（src-tauri）

### db/models.rs
- `Task` 增加字段 `pub repeat_series_id: Option<String>`（FromRow 自动兼容 NULL）。
- 新增 `RepeatSeries` 结构体：`weekdays: Vec<u8>`、`month_days: Vec<u8>`（DB 存 JSON 字符串），**手写 `sqlx::FromRow`**（`row.try_get::<String,_>("weekdays")` 后 serde_json 解析，`unwrap_or_default`）。
- 新增 `UpdateRepeatSeriesInput`：`series_id`、`base_name`（复用 validate_name）、`plan_qty`（validate_qty）、`unit`、`frequency`（validate_frequency）、`start_date`/`end_date`（validate_date_format）、`weekdays`/`month_days`（Option，缺省回退系列现值）。
- 新增 `RepeatSeriesUpdateResult { series: RepeatSeries, goal_id: String }`（Serialize/Deserialize）。

### services/split_service.rs
- 从 `split_repeat_tasks` 抽两个可复用 helper（现有逻辑不动、仅抽取）：
  - `pub fn repeat_dates(start: NaiveDate, end: NaiveDate, frequency: &str, weekdays: &[u8], month_days: &[u8], is_single: bool) -> Vec<NaiveDate>`（即现 L553-575 命中判定 + 循环）。
  - `pub fn repeat_name(base: &str, seq: usize, multi: bool) -> String`（multi → `"{base} - 第{seq}次"`，单次 → base）。
- `split_repeat_tasks` 改用 helper，输出不变；`Task { ... }` 字面量全部补 `repeat_series_id: None`（L59/L145/L228/L263 的智能拆解 + L593 重复拆解）。

### commands/goal.rs
- `repeat_split`：生成任务后，若 `input.end_date` 存在且 > start（用户选择了重复范围），创建 repeat_series 行（uuid、`now_local_ts()`），为每个任务置 `repeat_series_id`；内联 INSERT 扩为 17 列（加 repeat_series_id + bind）。单次任务不建系列，行为不变。
- 新增 `get_repeat_series(series_id: String, state) -> AppResult<Option<RepeatSeries>>`：按 id 查 repeat_series 返回；任务无系列/系列不存在时返回 None（前端隐藏入口）。
- 新增 `update_repeat_series(input, state) -> AppResult<RepeatSeriesUpdateResult>`，单事务：
  1. `input.validate()`，取系列行（not_found）。
  2. 用 `repeat_dates` 算新发生日期序列；`repeat_name` 生成新名。
  3. 取该系列全部实例；**保留** `status != 'pending' || actual_qty > 0 || is_manual == 1` 的实例，按 `sort_order+1` 重新命名、更新 plan_qty/unit（名称随新 base_name）。
  4. 删除其余（未完成）实例；先清依赖 `DELETE FROM task_dependencies WHERE task_id IN (...) OR depends_on_id IN (...)`（仿 delete_task，SQLite 无外键级联）。
  5. 按新日期序列重建：跳过与保留实例 plan_date 冲突的日期；新实例 `sort_order` 从 max(保留实例 sort_order)+1 起排，`source="auto"`、`repeat_series_id`、`path="/{goal}/{id}"`、`created_at=now_local_ts()`、`estimated_hours=None`。
  6. `UPDATE repeat_series` 写回新设置；commit；返回 `{ series, goal_id }`。

### commands/mod.rs
- `build_handler()` 注册 `goal::get_repeat_series`、`goal::update_repeat_series`。

> 不动 `helpers.rs::INSERT_TASK_SQL`（16 列），避免波及 task.rs/backup.rs 7 处调用点；新增系列只在 repeat_split 内联 SQL 写入。

## 前端改动（src）

### types/index.ts
- `Task` 增加 `repeat_series_id: string | null`。
- 新增 `RepeatSeries`（含 `weekdays: number[]`、`month_days: number[]`）、`UpdateRepeatSeriesInput`、`RepeatSeriesUpdateResult`。

### api/goal.ts
- `getRepeatSeries(seriesId)` → `invokeCommand("get_repeat_series", { seriesId })`。
- `updateRepeatSeries(input)` → `invokeCommand("update_repeat_series", { input })`。

### views/GoalTreeView.vue
- `taskModalMode` 类型扩为 `"create" | "edit" | "edit_repeat"`。
- 新增 `openEditRepeatModal(task)`：`task.repeat_series_id` 为空则 return；`getRepeatSeries` 预填表单（`task_id=series.id`、base_name、plan_qty、unit、frequency、weekdays、month_days、`plan_date=parseISO(start_date)`、`end_date=parseISO(end_date)`、`is_repeat=true`），模式设 `edit_repeat`，打开弹窗。
- `buildTaskActions`：`task.source === "auto" && task.repeat_series_id` 时，在"编辑"前插入「编辑原任务」（key: `edit_repeat`）。
- `handleTaskAction`：新增 `case "edit_repeat"`。
- `handleSaveTask`：新增 `else if (taskModalMode === "edit_repeat")` 分支——校验 end_date/weekdays/month_days（镜像 create 重复分支的校验），组装 `UpdateRepeatSeriesInput` 调 `updateRepeatSeries`，成功后关弹窗 + `goalStore.fetchGoalTree()` + `fetchProgresses()`（系列实例 id 集合变化，无法局部更新，走 create 模式的全量刷新）。
- 模板：弹窗标题三态（edit_repeat → "编辑原任务"）；频率/周几/每月几号/结束日期区块显隐条件由 `taskModalMode === 'create' && taskForm.is_repeat` 改为 `taskForm.is_repeat && taskModalMode !== 'edit'`；"重复任务"开关与底部提示保持仅 create；unit 下拉 `:disabled` 保持 `taskModalMode === 'edit'`（edit_repeat 可改）；底部按钮文字保持不变（非 create 显示"保存"）。

## 边界与风险
- 回填的系列频率为 daily、周几/每月几号为空，用户编辑时按实际改正；`instr(name, ' - 第')` 对 base_name 本身含" - "的任务也能正确切分（取的是最后一个" - 第"前）。
- 历史旧实例若经单条"编辑"改过 plan_qty（is_manual=1），重建时会被保留，避免误删用户手动改过的记录。
- 删除实例必须显式清理 task_dependencies（双向）。
- restore 数据不含 repeat_series 表 → 恢复后任务可能带悬空 series id，`get_repeat_series` 返回 None，入口自动隐藏（可接受的降级，不在本次范围）。

## 验证
1. 后端：`cargo check`（src-tauri 目录，target-dir 被 .cargo/config.toml 重定向）。
2. 前端 + 端到端（仅单实例运行 tauri dev，避免日志冲突）：
   - 新建目标 → 添加每日重复任务（起止 7 天）→ 生成 7 个"第N次"实例；补完成其中 1 个。
   - 节点菜单出现「编辑原任务」→ 弹窗预填正确（名称无" - 第N次"后缀、频率/日期/周几正确）。
   - 改名称 + 频率为每周 → 保存 → 已完成实例保留并改名、未完成实例按新规则重建、无重复日期；树与进度正确刷新。
   - 仅改名称保存 → 所有实例名称级联更新。
   - 用旧数据启动：迁移回填后，历史重复任务显示「编辑原任务」且可正常编辑。
3. 回归：单条实例「编辑」仍只改自身；新建普通任务/智能拆解（" - 第N天"）不受影响；「删除」批次逻辑不变。
