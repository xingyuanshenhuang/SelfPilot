# SelfPilot 迭代规划 v3 · 功能详解

> 基于 PRD v1.0(2026-06-15)与 2026-06-27 代码实际状态盘点
> 当前整体完成度:约 90%,已具备内测可用条件
> 制定日期:2026-06-27

---

## 一、当前功能完成度盘点(基于代码核对)

### 1.1 后端 Command 实际状态(34 个)

| 模块 | Command | 文件 | 状态 | 完成度 |
|------|---------|------|------|--------|
| **目标** | create_goal | [goal.rs#L17](file:///workspace/src-tauri/src/commands/goal.rs) | ✅ | 100% |
| | list_goals | [goal.rs#L74](file:///workspace/src-tauri/src/commands/goal.rs) | ✅ | 100% |
| | list_goal_tree | [goal.rs#L84](file:///workspace/src-tauri/src/commands/goal.rs) | ✅ | 100% |
| | get_goal | [goal.rs#L155](file:///workspace/src-tauri/src/commands/goal.rs) | ✅ | 100% |
| | update_goal | [goal.rs#L166](file:///workspace/src-tauri/src/commands/goal.rs) | ✅ | 100% |
| | delete_goal | [goal.rs#L211](file:///workspace/src-tauri/src/commands/goal.rs) | ⚠️ | 80%(未事务化) |
| | auto_split | [goal.rs#L250](file:///workspace/src-tauri/src/commands/goal.rs) | ⚠️ | 80%(未事务化) |
| | repeat_split | [goal.rs#L308](file:///workspace/src-tauri/src/commands/goal.rs) | ⚠️ | 60%(仅每日) |
| | replan_preview | [goal.rs#L348](file:///workspace/src-tauri/src/commands/goal.rs) | ✅ | 100% |
| | replan_goal | [goal.rs#L369](file:///workspace/src-tauri/src/commands/goal.rs) | ⚠️ | 80%(未事务化) |
| **任务** | create_task | [task.rs#L10](file:///workspace/src-tauri/src/commands/task.rs) | ✅ | 100% |
| | complete_task | [task.rs#L55](file:///workspace/src-tauri/src/commands/task.rs) | ✅ | 100% |
| | skip_task | [task.rs#L110](file:///workspace/src-tauri/src/commands/task.rs) | ✅ | 100% |
| | backfill_task | [task.rs#L194](file:///workspace/src-tauri/src/commands/task.rs) | ✅ | 100% |
| | move_task | [task.rs#L243](file:///workspace/src-tauri/src/commands/task.rs) | ✅ | 100% |
| | update_task_plan_qty | [task.rs#L275](file:///workspace/src-tauri/src/commands/task.rs) | ✅ | 100% |
| | update_task | [task.rs#L305](file:///workspace/src-tauri/src/commands/task.rs) | ✅ | 100% |
| | delete_task | [task.rs#L393](file:///workspace/src-tauri/src/commands/task.rs) | ✅ | 100% |
| | list_today_tasks | [task.rs#L132](file:///workspace/src-tauri/src/commands/task.rs) | ✅ | 100% |
| | list_overdue_tasks | [task.rs#L152](file:///workspace/src-tauri/src/commands/task.rs) | ✅ | 100% |
| | list_tasks_by_goal | [task.rs#L172](file:///workspace/src-tauri/src/commands/task.rs) | ✅ | 100% |
| | list_tasks_by_date_range | [task.rs#L408](file:///workspace/src-tauri/src/commands/task.rs) | ✅ | 100% |
| **进度** | get_goal_progress | progress.rs | ⚠️ | 70%(N+1) |
| | get_all_goals_progress | progress.rs | ✅ | 100%(批量优化) |
| **统计** | get_completion_trend | [stats.rs#L17](file:///workspace/src-tauri/src/commands/stats.rs) | ✅ | 100% |
| | get_goal_completion_stats | [stats.rs#L81](file:///workspace/src-tauri/src/commands/stats.rs) | ✅ | 100% |
| | get_heatmap | [stats.rs#L116](file:///workspace/src-tauri/src/commands/stats.rs) | ✅ | 100% |
| | get_completion_predictions | [stats.rs#L194](file:///workspace/src-tauri/src/commands/stats.rs) | ✅ | 100% |
| **鼓励语** | list_encouragements | [encouragement.rs#L10](file:///workspace/src-tauri/src/commands/encouragement.rs) | ✅ | 100% |
| | add_encouragement | [encouragement.rs#L20](file:///workspace/src-tauri/src/commands/encouragement.rs) | ✅ | 100% |
| | delete_encouragement | [encouragement.rs#L63](file:///workspace/src-tauri/src/commands/encouragement.rs) | ✅ | 100% |
| | random_encouragement_by_streak | [encouragement.rs#L101](file:///workspace/src-tauri/src/commands/encouragement.rs) | ✅ | 100% |
| | random_celebration_encouragement | [encouragement.rs#L144](file:///workspace/src-tauri/src/commands/encouragement.rs) | ✅ | 100% |
| | get_streak | [encouragement.rs#L178](file:///workspace/src-tauri/src/commands/encouragement.rs) | ✅ | 100% |
| **设置** | get_all_settings / get_setting / set_setting | settings.rs | ✅ | 100% |
| **备份** | export_data | [backup.rs#L11](file:///workspace/src-tauri/src/commands/backup.rs) | ✅ | 100% |
| | import_data | [backup.rs#L46](file:///workspace/src-tauri/src/commands/backup.rs) | ⚠️ | 80%(未事务化) |

### 1.2 前端视图状态

| 视图 | 文件 | 状态 | 缺口 |
|------|------|------|------|
| 目标总览 | [DashboardView.vue](file:///workspace/src/views/DashboardView.vue) | ✅ | — |
| 目标树 | [GoalTreeView.vue](file:///workspace/src/views/GoalTreeView.vue) | ⚠️ | 硬编码两层、无拖拽 |
| 日历视图 | [CalendarView.vue](file:///workspace/src/views/CalendarView.vue) | ✅ | — |
| 数据统计 | [StatsView.vue](file:///workspace/src/views/StatsView.vue) | ✅ | — |
| 鼓励语库 | [EncouragementView.vue](file:///workspace/src/views/EncouragementView.vue) | ⚠️ | 缺编辑入口 |
| 设置 | [SettingsView.vue](file:///workspace/src/views/SettingsView.vue) | ✅ | — |

### 1.3 PRD 兑现度对照

| PRD 要求 | 兑现状态 | 说明 |
|----------|---------|------|
| 三级结构 目标→阶段→任务 | ✅ 已重构 | 004 迁移统一为 Goal 树,Stage 转子目标 |
| 自动拆解(总量摊分,余数前置) | ✅ | [split_service.rs#L14](file:///workspace/src-tauri/src/services/split_service.rs) |
| 重复拆解(每天/每周几/每月几号) | ⚠️ 部分 | 仅每日,[split_service.rs#L217](file:///workspace/src-tauri/src/services/split_service.rs) |
| 手动添加任务 | ✅ | create_task |
| 完成/部分完成/跳过/补完成 | ✅ | complete/skip/backfill |
| 重新规划(保留手动项) | ✅ | replan_preview/replan_goal |
| 日历视图(月/周/日) | ✅ | CalendarView |
| 柱状图/折线图/热力图 | ✅ | StatsView |
| 完成预测 | ✅ | get_completion_predictions |
| 鼓励语 4 等级 + 个性化 | ✅ | random_encouragement_by_streak |
| 连续打卡(无任务日不中断) | ✅ | get_streak |
| 主题切换 | ✅ | SettingsView |
| JSON 导出/导入(skip/overwrite/rename) | ✅ | export/import_data |
| 任务拖拽归属 | ❌ | 后端 move_task 就绪,前端缺 UI |

### 1.4 已识别技术债

| 编号 | 问题 | 位置 | 风险 |
|------|------|------|------|
| TD-1 | 多步写未包事务 | goal.rs delete/auto_split/replan | 中断留脏数据 |
| TD-2 | 前端写后全量重拉 | goalStore 所有 action | 大数据卡顿 |
| TD-3 | 前端硬编码两层渲染 | GoalTreeView.vue | 无法支持多层嵌套 |
| TD-4 | 单目标进度 N+1 查询 | progress_service calc_goal_progress | 性能 |
| TD-5 | stages 表/struct 残留 | models.rs / migrations | 代码混淆 |
| TD-6 | import_data 未事务化 | backup.rs | 部分导入留脏数据 |

---

## 二、迭代优先级框架

```
P0  补齐 PRD 缺口 + 数据安全      近期 1-2 Sprint
P1  扩展任务模型,支撑复杂目标     中期 3-5 Sprint
P2  性能体验优化 + 数据层强化      中长期 6+ Sprint
P3  生态扩展,多端与智能           远期
```

---

## 三、P0 — 补齐缺口与数据安全(近期)

### 3.1 周期性任务补全(兑现 PRD 承诺)

**问题**:[split_service.rs#L217](file:///workspace/src-tauri/src/services/split_service.rs) `split_repeat_tasks` 仅支持每日重复,PRD §4.2 承诺的「每周几/每月几号」未实现。

#### 数据模型扩展

```rust
// models.rs: RepeatSplitInput 增加字段
#[derive(Debug, Clone, Deserialize)]
pub struct RepeatSplitInput {
    pub goal_id: String,
    pub name: String,
    pub start_date: String,
    pub end_date: Option<String>,
    pub plan_qty: Option<f64>,
    pub unit: Option<String>,
    // 新增 ↓
    pub frequency: Option<String>,        // "daily"(默认) | "weekly" | "monthly"
    pub weekdays: Option<Vec<u8>>,        // 周几(1-7),weekly 时必填
    pub month_days: Option<Vec<u8>>,      // 每月几号(1-31),monthly 时必填
}
```

#### 拆解逻辑改造

```rust
// split_service.rs: split_repeat_tasks
let frequency = input.frequency.as_deref().unwrap_or("daily");

while cursor <= end {
    let should_generate = match frequency {
        "daily" => true,
        "weekly" => {
            let wd = cursor.weekday().number(); // 1=Mon..7=Sun
            input.weekdays.as_ref().map(|v| v.contains(&wd)).unwrap_or(false)
        }
        "monthly" => {
            let md = cursor.day() as u8;
            input.month_days.as_ref().map(|v| v.contains(&md)).unwrap_or(false)
        }
        _ => true,
    };

    if should_generate {
        // 生成任务(现有逻辑)
        day_index += 1;
        // ...
    }
    cursor += chrono::Duration::days(1);
}
```

#### 前端改造

[GoalTreeView.vue](file:///workspace/src/views/GoalTreeView.vue) `repeatForm` 增加:
- `frequency` 选择器(每天/每周/每月)
- weekly 时显示周一~周日多选
- monthly 时显示 1~31 多选

#### 验收标准

- Given 创建「每周一、三、五」重复任务,When 生成,Then 仅在对应日期生成任务
- Given 「每月 1 号、15 号」重复任务,When 生成,Then 仅在对应日期生成任务
- Given 未传 frequency,When 生成,Then 走 daily 默认逻辑(向后兼容)

### 3.2 事务化写入(数据安全底线)

**问题**:多个写操作循环执行,中途失败留脏数据。

#### 改动点

**3.2.1 [goal.rs](file:///workspace/src-tauri/src/commands/goal.rs) delete_goal**(级联删除)

```rust
pub async fn delete_goal(id: String, state: State<'_, DbPool>) -> AppResult<()> {
    let mut tx = state.0.begin().await?;  // 开启事务

    // BFS 收集后代(在事务内查询)
    let mut to_delete = vec![id.clone()];
    let mut queue = vec![id.clone()];
    while let Some(current) = queue.pop() {
        let children: Vec<String> =
            sqlx::query_scalar("SELECT id FROM goals WHERE parent_id = ?")
                .bind(&current)
                .fetch_all(&mut *tx).await?;
        for child in children {
            to_delete.push(child.clone());
            queue.push(child);
        }
    }

    // 删除任务
    for goal_id in &to_delete {
        sqlx::query("DELETE FROM tasks WHERE goal_id = ?")
            .bind(goal_id).execute(&mut *tx).await?;
    }
    // 删除目标
    for goal_id in &to_delete {
        sqlx::query("DELETE FROM goals WHERE id = ?")
            .bind(goal_id).execute(&mut *tx).await?;
    }

    tx.commit().await?;  // 提交
    Ok(())
}
```

**3.2.2 [goal.rs](file:///workspace/src-tauri/src/commands/goal.rs) auto_split / repeat_split**(批量插入)

```rust
let mut tx = state.0.begin().await?;
for task in &tasks {
    sqlx::query("INSERT INTO tasks ...")
        .bind(...).execute(&mut *tx).await?;
}
tx.commit().await?;
```

**3.2.3 [goal.rs](file:///workspace/src-tauri/src/commands/goal.rs) replan_goal**(循环更新)同理事务化。

**3.2.4 [backup.rs](file:///workspace/src-tauri/src/commands/backup.rs) import_data**(导入流程)整体包事务。

#### 验收标准

- 模拟 delete_goal 中途 panic,数据库不残留部分删除状态
- 模拟 auto_split 插入到一半失败,数据库无新增任务

### 3.3 鼓励语编辑入口(CRUD 完整性)

**问题**:[encouragement.rs](file:///workspace/src-tauri/src/commands/encouragement.rs) 有 add/delete,缺 update。

#### 新增 Command

```rust
#[tauri::command]
pub async fn update_encouragement(
    input: UpdateEncouragementInput,
    state: State<'_, DbPool>,
) -> AppResult<Encouragement> {
    // 校验存在 + 非预设
    // UPDATE text, level WHERE id = ?
}
```

#### 前端改造

EncouragementView 增加编辑按钮 + 复用创建弹窗。

### 3.4 任务拖拽归属 UI(README P0)

**问题**:后端 [task.rs#L243](file:///workspace/src-tauri/src/commands/task.rs) `move_task` 已就绪,前端无拖拽交互。

#### 方案

[GoalTreeView.vue](file:///workspace/src/views/GoalTreeView.vue) 引入 `vuedraggable` 或 `@vueuse/core` 的 `useDraggable`,任务行可拖到子目标卡片上,松手触发 `move_task`。

#### 验收

- 拖拽任务 A 到子目标 B,松手后 A 的 goal_id 更新为 B

---

## 四、P1 — 扩展任务模型(中期)

### 4.1 任务依赖关系(最高价值)

**目标**:支撑学习路径编排(学 A 才能学 B)。

#### 数据模型

```sql
-- 新迁移 005_task_dependency.sql
ALTER TABLE tasks ADD COLUMN depends_on TEXT; -- JSON 数组 ["id1","id2"]

-- 可选:独立关联表(支持多对多 + 元数据)
CREATE TABLE task_dependencies (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL,          -- 后置任务
    depends_on_task_id TEXT NOT NULL, -- 前置任务
    created_at TEXT NOT NULL,
    FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE,
    FOREIGN KEY (depends_on_task_id) REFERENCES tasks(id) ON DELETE CASCADE,
    UNIQUE(task_id, depends_on_task_id)
);
```

#### 业务规则

| 场景 | 行为 |
|------|------|
| 今日待办 | 前置未 done 时,该任务标记「阻塞」不展示为今日应做 |
| 日历视图 | 依赖未满足的任务标灰 |
| 重新规划 | 前置任务 plan_date ≤ 后置任务 plan_date |
| 新增依赖 | DFS 检测有向图无环(DAG),防循环依赖 |

#### 新增 Command

```rust
pub async fn set_task_dependency(task_id, depends_on: Vec<String>, ...)
pub async fn get_blocked_tasks(...) -> AppResult<Vec<Task>>  // 被阻塞的任务
pub async fn check_dependency_satisfied(task_id, ...) -> AppResult<bool>
```

#### 前端

任务详情增加「前置任务」多选器;阻塞任务显示锁图标 + 原因。

#### 验收

- Given B 依赖 A,When A 未完成,Then B 不出现在今日待办
- Given B 依赖 A,When A 完成,Then B 自动解除阻塞
- Given 设置 B→A 又 A→B,When 提交,Then 报错「检测到循环依赖」

### 4.2 非线性阶段拆解

**目标**:各子目标独立拆解规则(前 10 天看视频,后 20 天做题)。

**现状瓶颈**:[split_service.rs#L14](file:///workspace/src-tauri/src/services/split_service.rs) 只对单 goal 拆解。

#### 方案

- 子目标作为独立拆解单元(已有 `handleAutoSplit(subNode.goal)` 调用入口)
- 父目标进度 = Σ 子目标进度(汇总逻辑已支持)
- 增加引导文案:「为不同阶段设置不同总量与截止日」
- 前端子目标卡片增加拆解按钮(已有,需确保子目标有独立 total_qty/deadline)

### 4.3 时间预算模型

**目标**:支持按每日可用小时数拆解,而非仅按天数均匀分配。

#### 数据模型

```sql
ALTER TABLE goals ADD COLUMN daily_available_hours REAL; -- 每日可用时长
ALTER TABLE tasks ADD COLUMN estimated_hours REAL;       -- 预估耗时
```

#### 新增拆解模式

```rust
pub fn split_goal_by_hours(goal, daily_available_hours, today) -> Vec<Task>
```

#### 前端

创建目标时选择「按数量拆解」或「按时间拆解」。

### 4.4 任务优先级

#### 数据模型

```sql
ALTER TABLE tasks ADD COLUMN priority INTEGER DEFAULT 0; -- 0 普通,1 重要,2 紧急
ALTER TABLE tasks ADD COLUMN due_time TEXT;              -- 具体时间点(可选,当日排序)
```

#### 业务影响

- 今日待办排序:`priority DESC, due_time ASC, sort_order`
- 重新规划优先保证高优先级任务日期

### 4.5 里程碑节点

```sql
CREATE TABLE milestones (
    id TEXT PRIMARY KEY,
    goal_id TEXT NOT NULL,
    name TEXT NOT NULL,
    target_date TEXT,
    target_qty REAL,
    description TEXT,
    is_reached INTEGER DEFAULT 0,
    created_at TEXT NOT NULL,
    FOREIGN KEY (goal_id) REFERENCES goals(id) ON DELETE CASCADE
);
```

目标树中里程碑以特殊样式标记,达到 target_qty 自动置 `is_reached=1`。

---

## 五、P2 — 性能体验优化(中长期)

### 5.1 前端渲染优化

#### 5.1.1 虚拟滚动

**问题**:目标树节点 >500 时卡顿,当前 [GoalTreeView.vue](file:///workspace/src/views/GoalTreeView.vue) 全量渲染。

**方案**:引入 `vue-virtual-scroller`,仅渲染可视区域节点。

#### 5.1.2 递归组件支持 N 层

**问题**:前端硬编码两层(总目标 → sub_goals → tasks)。

**方案**:抽 `GoalTreeNode.vue` 递归组件,通过自引用支持无限层级。

```vue
<!-- GoalTreeNode.vue -->
<template>
  <div class="node">
    <!-- 节点头部 -->
    <GoalTreeNode v-for="child in node.sub_goals" :node="child" />
    <TaskRow v-for="task in node.tasks" :task="task" />
  </div>
</template>
```

#### 5.1.3 增量更新替代全量重拉

**问题**:[goalStore.ts](file:///workspace/src/stores/goalStore.ts) 所有写操作后 `fetchGoalTree()` + `fetchProgresses()` 全量重拉。

**方案**:写操作返回受影响节点,Pinia `patch` 局部更新,避免整树重序列化。保留全量重拉作为 fallback。

### 5.2 数据层强化

#### 5.2.1 SQLite 原生备份

**问题**:当前 JSON 导出/导入,大体积慢且易错。

**方案**:新增 `vacuum_backup` Command,使用 `VACUUM INTO 'file.db'` 生成完整 SQLite 备份,恢复时直接替换文件。

```rust
#[tauri::command]
pub async fn backup_database(target_path: String, state: State<'_, DbPool>) -> AppResult<()> {
    sqlx::query(&format!("VACUUM INTO '{}'", target_path))
        .execute(&state.0).await?;
    Ok(())
}
```

#### 5.2.2 单目标进度查询优化

**问题**:[progress_service.rs](file:///workspace/src-tauri/src/services/progress_service.rs) `calc_goal_progress` 走 BFS + 逐 goal 查询(N+1)。

**方案**:复用 `calc_all_goals_progress` 批量逻辑,或改用递归 CTE 一次查询。

#### 5.2.3 跨目标负载平衡

**目标**:多目标并行时避免单日任务爆炸。

**方案**:
- 拆解时查询当日已有任务量,提示「今日已安排 X 个任务,是否继续?」
- 新增「每日负载」视图,展示每日总任务数/总量
- 重新规划时跨目标考虑总负载

### 5.3 数据库清理

#### 5.3.1 清理 stages 残留

[004_goal_tree.sql](file:///workspace/src-tauri/migrations/004_goal_tree.sql) 已将 stages 转为子目标,但 `tasks.stage_id` 字段、`Stage` struct 仍保留。

**方案**:新增迁移 `006_cleanup_stages.sql`:
- SQLite 不支持 DROP COLUMN,需重建表
- 或保留字段但清理 [models.rs](file:///workspace/src-tauri/src/db/models.rs) 中 `Stage` 相关废弃代码

---

## 六、P3 — 生态扩展(远期)

### 6.1 云端同步

[tech-stack-detail.md](file:///workspace/tech-stack-detail.md) 已预留 `sync.rs` / `sync_service.rs`。

#### 核心设计

- **离线优先**:所有操作先写本地 SQLite
- **在线时增量同步**:last-write-wins + 字段级 merge
- **冲突解决**:多设备 ID + 时间戳向量钟
- **数据模型扩展**:
  ```sql
  ALTER TABLE goals ADD COLUMN updated_at TEXT;
  ALTER TABLE goals ADD COLUMN device_id TEXT;
  ALTER TABLE goals ADD COLUMN sync_version INTEGER DEFAULT 0;
  ```

### 6.2 国际化(i18n)

当前所有文案硬编码中文。

**方案**:引入 `vue-i18n`,抽取所有中文字符串到 locale 文件,SettingsView 增加语言切换。

### 6.3 移动端适配

Tauri 2.x 已支持 iOS/Android([src-tauri/icons](file:///workspace/src-tauri/icons) 已含移动端图标)。

**适配点**:
- 触控交互:目标树拖拽改长按
- 响应式布局:左侧导航改底部 Tab
- 移动端 SQLite 性能调优
- App Store / Google Play 打包流程

### 6.4 AI 辅助规划(增值方向)

- **拆解粒度建议**:「你这个目标建议拆 20 天而非 10 天」
- **风险预警**:「按当前速度将逾期 5 天,是否重新规划?」
- **学习路径推荐**:基于任务名称自动推断依赖关系
- **智能鼓励语**:基于用户行为模式生成个性化鼓励语

**方案**:接入 LLM API,本地缓存建议结果,用户确认后应用。

### 6.5 提醒与通知

- Tauri 系统通知 API
- 设置「每日提醒时间」
- 逾期任务强提醒
- 任务到期前 N 分钟提醒

---

## 七、迭代节奏建议

```
Sprint 7-8 (近期)    P0 全部
                     - 周期任务补全(每周/每月)
                     - 事务化写入(delete/auto_split/replan/import)
                     - 鼓励语编辑
                     - 任务拖拽 UI

Sprint 9-11 (中期)   P1 核心
                     - 任务依赖关系(最高价值,打开复杂目标市场)
                     - 非线性阶段拆解
                     - 时间预算模型
                     - 优先级与里程碑

Sprint 12-14 (中长期) P2 性能
                     - 虚拟滚动
                     - 递归组件 N 层
                     - 增量更新
                     - SQLite 原生备份
                     - 单目标进度查询优化

Sprint 15+ (远期)     P3 生态
                     - 云同步(独立大版本)
                     - i18n
                     - 移动端
                     - AI 辅助规划
```

---

## 八、最值得优先的三件事

| 优先级 | 事项 | 价值 | 改动量 |
|--------|------|------|--------|
| 1 | **周期性任务补全** | PRD 已承诺未交付,用户预期差 | 小 |
| 2 | **事务化写入** | 数据安全底线 | 小 |
| 3 | **任务依赖关系** | 决定产品定位(规划系统 vs 待办工具) | 中 |

---

## 九、版本里程碑

| 版本 | 目标 | 包含 |
|------|------|------|
| **v0.2.0** | 内测完善 | P0 全部 + 拖拽 UI |
| **v0.3.0** | 复杂目标支持 | P1 任务依赖 + 非线性拆解 |
| **v0.4.0** | 性能优化 | P2 虚拟滚动 + 增量更新 + 原生备份 |
| **v1.0.0** | 正式发布 | P2 完成 + 文档完善 + 多平台构建验证 |
| **v2.0.0** | 多端同步 | P3 云同步 + 移动端 + i18n |

---

## 十、风险与依赖

| 风险 | 影响 | 缓解 |
|------|------|------|
| 任务依赖引入循环检测复杂度 | 中 | DFS 检测 DAG,单元测试覆盖 |
| 增量更新与全量重拉逻辑分叉 | 中 | 保留全量重拉 fallback,渐进切换 |
| 云同步架构改动大 | 高 | 独立大版本,不影响离线核心 |
| 移动端触控交互重做 | 中 | 响应式优先,复用逻辑层 |
| 数据库迁移兼容旧数据 | 中 | 每个迁移脚本回滚测试 |
| 事务化改动影响现有调用 | 低 | 事务边界仅含写操作,查询不变 |

---

## 十一、附录:数据模型现状

```
goals (id, parent_id, path, name, deadline, total_qty, unit, sort_order, created_at)
tasks  (id, goal_id, stage_id[废弃], parent_id, path, name, plan_date, plan_qty,
        actual_qty, unit, status, is_manual, source, sort_order, created_at)
encouragements (id, text, category, level, created_at)
settings (key, value)
stages  [废弃,仅保留兼容旧备份]
```

**关键字段语义**:
- `is_manual`:手动修改过 plan_qty 的任务,重新规划时保留
- `source`:`auto`(拆解生成) | `manual`(手动创建)
- `status`:`pending` | `partial` | `done` | `skipped`
- `path`:冗余路径,主要用于备份与调试,实际关系靠 parent_id/goal_id

---

*本文档基于 2026-06-27 代码状态制定,随迭代持续更新。*
