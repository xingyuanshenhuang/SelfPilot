# SelfPilot 迭代规划 v2

> 基于 PRD v1.0(2026-06-15)与当前代码实现盘点
> 制定日期:2026-06-27
> 当前整体完成度:约 90%,已具备内测可用条件

---

## 一、当前功能完成度盘点

### 1.1 已完成模块

| 模块 | 后端 Command | 前端视图 | 完成度 | 备注 |
|------|-------------|---------|--------|------|
| 目标 CRUD | create/list/get/update/delete_goal | GoalTreeView | 100% | 含子目标嵌套 |
| 目标树查询 | list_goal_tree | GoalTreeView | 100% | 递归构建 + 进度附挂 |
| 视频拆解 | auto_split | GoalTreeView | 100% | 总量摊分 + 余数前置 |
| 重复拆解 | repeat_split | GoalTreeView | 70% | 仅每日重复,缺每周/每月 |
| 重新规划 | replan_preview / replan_goal | GoalTreeView | 100% | Before/After 预览 + 保留 manual |
| 任务 CRUD | create/update/delete_task | GoalTreeView | 100% | 通用更新接口 |
| 任务完成 | complete_task | Dashboard/Calendar/Tree | 100% | 支持部分完成 |
| 任务跳过 | skip_task | Dashboard/Calendar/Tree | 100% | 不计入统计 |
| 补完成 | backfill_task | GoalTreeView | 100% | 仅更新历史,不联动未来 |
| 移动任务 | move_task | — | 0% | 后端就绪,前端无拖拽 UI |
| 今日待办 | list_today_tasks | DashboardView | 100% | — |
| 逾期任务 | list_overdue_tasks | DashboardView | 100% | — |
| 日历视图 | list_tasks_by_date_range | CalendarView | 100% | 月/周/日 + 逾期标记 |
| 进度汇总 | get_goal_progress / get_all_goals_progress | 全局 | 100% | 递归加权汇总 + 批量优化 |
| 完成趋势 | get_completion_trend | StatsView | 100% | 近 7/30 天 |
| 目标统计 | get_goal_completion_stats | StatsView | 100% | 柱状图 |
| 热力图 | get_heatmap | StatsView | 100% | 90/180/365 天 |
| 完成预测 | get_completion_predictions | StatsView | 100% | 基于 7 天均速 |
| 鼓励语 CRUD | list/add/delete_encouragement | EncouragementView | 90% | 缺编辑(update) |
| 鼓励语抽取 | random_encouragement_by_streak | App.vue | 100% | 等级降级 fallback |
| 庆祝鼓励语 | random_celebration_encouragement | App.vue | 100% | 全部目标完成触发 |
| 连续打卡 | get_streak | 全局 | 100% | 无任务日不中断规则 |
| 设置(主题) | get/set_setting | SettingsView | 100% | 浅色/深色 |
| 数据导出导入 | export_data / import_data | SettingsView | 95% | skip/overwrite/rename |

### 1.2 数据模型现状

```
goals (id, parent_id, path, name, deadline, total_qty, unit, sort_order, created_at)
tasks  (id, goal_id, parent_id, path, name, plan_date, plan_qty, actual_qty,
        unit, status, is_manual, source, sort_order, created_at)
encouragements (id, text, category, level, created_at)
settings (key, value)
```

**关键字段语义**:
- `is_manual`:手动修改过 `plan_qty` 的任务,重新规划时保留
- `source`:`auto`(拆解生成) | `manual`(手动创建)
- `status`:`pending` | `partial` | `done` | `skipped`
- `path`:冗余路径,主要用于备份与调试

### 1.3 已识别的技术债

| 编号 | 问题 | 位置 | 风险 |
|------|------|------|------|
| TD-1 | 多步写操作未包事务 | goal.rs delete_goal 循环删除 | 中断留脏数据 |
| TD-2 | 前端写后全量重拉 | goalStore 所有 action | 大数据卡顿 |
| TD-3 | 前端硬编码两层渲染 | GoalTreeView.vue | 无法支持多层嵌套 |
| TD-4 | 单目标进度走 N+1 查询 | progress_service calc_goal_progress | 性能 |
| TD-5 | stages 表/字段残留 | models.rs / migrations | 代码混淆 |

---

## 二、迭代优先级框架

```
P0  补齐 PRD 承诺缺口 + 数据安全底线  (近期 1-2 Sprint)
P1  扩展任务模型,支撑复杂目标        (中期 3-5 Sprint)
P2  性能体验优化 + 数据层强化         (中长期 6+ Sprint)
P3  生态扩展,走向多端与智能          (远期)
```

---

## 三、P0 — 补齐缺口与数据安全(近期)

### 3.1 周期性任务补全

**目标**:兑现 PRD §4.2「每周几 / 每月几号」承诺。

**现状**:[split_service.rs](file:///workspace/src-tauri/src/services/split_service.rs) `split_repeat_tasks` 仅支持 `daily` 重复。

**改动方案**:

1. 扩展数据模型与输入
```rust
// RepeatSplitInput 新增字段
pub frequency: String,      // "daily" | "weekly" | "monthly"
pub weekdays: Option<Vec<i8>>,  // 周几(1-7),weekly 时必填
pub month_days: Option<Vec<i8>>, // 每月几号(1-31),monthly 时必填
```

2. 拆解逻辑按 frequency 枚举日期
```rust
match frequency.as_str() {
    "daily" => { /* 现有逻辑 */ }
    "weekly" => {
        while cursor <= end {
            if weekdays.contains(&cursor.weekday().number()) {
                // 生成任务
            }
            cursor += Duration::days(1);
        }
    }
    "monthly" => {
        while cursor <= end {
            if month_days.contains(&cursor.day() as i8) {
                // 生成任务
            }
            cursor += Duration::days(1);
        }
    }
}
```

3. 前端 `repeatForm` 增加频率选择器,weekly 显示周几多选,monthly 显示日期多选。

**验收**:
- Given 用户创建「每周一、三、五」重复任务,When 生成,Then 仅在对应日期生成任务
- Given 「每月 1 号、15 号」重复任务,When 生成,Then 仅在对应日期生成任务

### 3.2 事务化写入

**目标**:消除脏数据风险。

**改动点**:
- [goal.rs](file:///workspace/src-tauri/src/commands/goal.rs) `delete_goal`:BFS 收集 + 循环删除 → 包入 `pool.begin()` / `commit()`
- [goal.rs](file:///workspace/src-tauri/src/commands/goal.rs) `auto_split` / `repeat_split`:批量插入 → 事务
- [goal.rs](file:///workspace/src-tauri/src/commands/goal.rs) `replan_goal`:循环更新 → 事务
- [backup.rs](file:///workspace/src-tauri/src/commands/backup.rs) `import_data`:导入流程 → 事务(已有,需复核)

**实现模板**:
```rust
let mut tx = state.0.begin().await?;
// 多步写操作
tx.commit().await?;
```

**验收**:模拟中途失败的写操作,数据库不残留部分数据。

### 3.3 鼓励语编辑入口

**目标**:补齐 CRUD 完整性。

**现状**:[encouragement.rs](file:///workspace/src-tauri/src/commands/encouragement.rs) 有 add/delete,缺 update。

**改动**:
1. 新增 `update_encouragement` Command
2. 前端 EncouragementView 增加编辑按钮与弹窗

### 3.4 目标树手动添加任务 UI 补全

**目标**:让任意节点可添加任务(README P0 列出)。

**现状**:前端已有「任务」按钮,但子目标下的任务创建流程需复核 `goal_id` 传递正确性。

---

## 四、P1 — 扩展任务模型(中期)

### 4.1 任务依赖关系

**目标**:支撑学习路径编排(学 A 才能学 B)。

**数据模型新增**:
```sql
ALTER TABLE tasks ADD COLUMN depends_on TEXT; -- JSON 数组:["task_id_1","task_id_2"]
```

**业务规则**:
- 今日待办过滤:前置任务未 `done` 时,该任务标记为「阻塞」不展示为今日应做
- 日历视图:依赖未满足的任务标灰显示
- 重新规划:依赖链决定排序,前置任务日期 ≤ 后置任务日期
- 防循环依赖:新增依赖时检测有向图无环(DAG)

**新增 Command**:
```rust
pub async fn set_task_dependency(task_id: String, depends_on: Vec<String>, ...)
pub async fn check_dependency_satisfied(task_id: String, ...) -> AppResult<bool>
```

**前端**:
- 任务详情增加「前置任务」选择器
- 阻塞任务在列表中显示锁图标 + 阻塞原因

**验收**:
- Given B 依赖 A,When A 未完成,Then B 不出现在今日待办
- Given B 依赖 A,When A 完成,Then B 解除阻塞

### 4.2 非线性阶段拆解

**目标**:各子目标独立拆解规则。

**现状**:[split_service.rs](file:///workspace/src-tauri/src/services/split_service.rs) `split_goal_into_tasks` 仅对单 goal,子目标继承不了独立节奏。

**方案**:子目标作为独立拆解单元,父目标进度 = Σ 子目标进度(当前汇总逻辑已支持,无需改后端)。

**改动**:
- 前端子目标头部按钮已有 `handleAutoSplit(subNode.goal)`,需确保子目标有独立 `total_qty`/`deadline`
- 增加引导文案:「为不同阶段设置不同总量与截止日」

### 4.3 时间预算模型

**目标**:支持按每日可用小时数拆解,而非仅按天数均匀分配。

**数据模型**:
```sql
ALTER TABLE tasks ADD COLUMN estimated_hours REAL; -- 预估耗时
```

**新增拆解模式**:
```rust
// 按时间预算拆解
pub fn split_goal_by_hours(goal, daily_available_hours, today) -> Vec<Task>
```

**前端**:创建目标时选择「按数量拆解」或「按时间拆解」。

### 4.4 任务优先级与排序

**目标**:多任务并行时明确执行顺序。

**数据模型**:
```sql
ALTER TABLE tasks ADD COLUMN priority INTEGER DEFAULT 0; -- 0 普通,1 重要,2 紧急
ALTER TABLE tasks ADD COLUMN due_time TEXT; -- 具体时间点(可选,用于当日排序)
```

**业务影响**:
- 今日待办按 `priority DESC, due_time ASC` 排序
- 重新规划时优先保证高优先级任务的日期

### 4.5 里程碑节点

**目标**:标记关键检查点。

**方案**:新增 `milestones` 表,或在 goals 增加 `is_milestone` 标记。

```sql
CREATE TABLE milestones (
    id TEXT PRIMARY KEY,
    goal_id TEXT NOT NULL,
    name TEXT NOT NULL,
    target_date TEXT,
    target_qty REAL,
    description TEXT,
    FOREIGN KEY (goal_id) REFERENCES goals(id) ON DELETE CASCADE
);
```

---

## 五、P2 — 性能体验优化(中长期)

### 5.1 前端渲染优化

**5.1.1 虚拟滚动**

目标树节点 >500 时卡顿,当前 [GoalTreeView.vue](file:///workspace/src/views/GoalTreeView.vue) 全量渲染。

**方案**:引入虚拟滚动组件(如 `vue-virtual-scroller`),仅渲染可视区域节点。

**5.1.2 递归组件支持 N 层**

当前前端硬编码两层(总目标 → sub_goals → tasks)。

**方案**:抽 `GoalTreeNode.vue` 递归组件,通过 `<GoalTreeNode>` 自引用支持无限层级。

**5.1.3 增量更新替代全量重拉**

[goalStore.ts](file:///workspace/src/stores/goalStore.ts) 当前所有写操作后 `fetchGoalTree()` + `fetchProgresses()` 全量重拉。

**方案**:写操作返回受影响节点,Pinia `patch` 局部更新,避免整树重序列化。

### 5.2 数据层强化

**5.2.1 SQLite 原生备份**

当前 JSON 导出/导入,大体积慢且易错。

**方案**:新增 `vacuum_backup` Command,使用 `VACUUM INTO 'file.db'` 生成完整 SQLite 备份,恢复时直接替换文件。

**5.2.2 单目标进度查询优化**

[progress_service.rs](file:///workspace/src-tauri/src/services/progress_service.rs) `calc_goal_progress` 走 BFS + 逐 goal 查询(N+1)。

**方案**:复用 `calc_all_goals_progress` 批量逻辑,或改用递归 CTE 一次查询。

**5.2.3 移动任务拖拽 UI**

后端 [task.rs](file:///workspace/src-tauri/src/commands/task.rs) `move_task` 已就绪,前端缺拖拽交互。

**方案**:在 GoalTreeView 引入拖拽库(如 `vuedraggable`),子目标间拖拽任务触发 `move_task`。

**5.2.4 跨目标负载平衡**

**目标**:多目标并行时避免单日任务爆炸。

**方案**:
- 拆解时查询当日已有任务量,提示「今日已安排 X 个任务,是否继续?」
- 新增「每日负载」视图,展示每日总任务数 / 总量

### 5.3 数据库清理

**5.3.1 清理 stages 残留**

[004_goal_tree.sql](file:///workspace/src-tauri/migrations/004_goal_tree.sql) 已将 stages 转为子目标,但 `tasks.stage_id` 字段、`Stage` struct 仍保留。

**方案**:新增迁移 `005_cleanup_stages.sql` 移除字段,清理 [models.rs](file:///workspace/src-tauri/src/db/models.rs) 中 `Stage` 相关废弃代码。

---

## 六、P3 — 生态扩展(远期)

### 6.1 云端同步

[tech-stack-detail.md](file:///workspace/tech-stack-detail.md) 已预留 `sync.rs` / `sync_service.rs`。

**核心设计**:
- 离线优先:所有操作先写本地 SQLite
- 在线时增量同步:last-write-wins + 字段级 merge
- 冲突解决:多设备 ID 管理 + 时间戳向量钟
- 数据模型增加 `updated_at`、`device_id`、`sync_version` 字段

### 6.2 国际化(i18n)

当前所有文案硬编码中文。

**方案**:
- 引入 `vue-i18n`
- 抽取所有中文字符串到 locale 文件
- SettingsView 增加语言切换

### 6.3 移动端适配

Tauri 2.x 已支持 iOS/Android([src-tauri/icons](file:///workspace/src-tauri/icons) 已含移动端图标)。

**适配点**:
- 触控交互:目标树拖拽改长按
- 响应式布局:左侧导航改底部 Tab
- 移动端 SQLite 性能调优
- App Store / Google Play 打包流程

### 6.4 AI 辅助规划(增值方向)

**场景**:
- 拆解粒度建议:「你这个目标建议拆 20 天而非 10 天」
- 风险预警:「按当前速度将逾期 5 天,是否重新规划?」
- 学习路径推荐:基于任务名称自动推断依赖关系
- 智能鼓励语:基于用户行为模式生成个性化鼓励语

**方案**:接入 LLM API,本地缓存建议结果,用户确认后应用。

### 6.5 提醒与通知

**目标**:任务到期提醒。

**方案**:
- Tauri 系统通知 API
- 设置「每日提醒时间」
- 逾期任务强提醒

---

## 七、迭代节奏建议

```
Sprint 7-8 (近期)   : P0 全部
                      - 周期任务补全
                      - 事务化写入
                      - 鼓励语编辑
                      - 目标树手动任务 UI 复核

Sprint 9-11 (中期)  : P1 核心
                      - 任务依赖关系(最高价值)
                      - 非线性阶段拆解
                      - 时间预算模型
                      - 优先级排序

Sprint 12-14 (中长期): P2 性能
                      - 虚拟滚动
                      - 递归组件 N 层
                      - 增量更新
                      - SQLite 原生备份
                      - 拖拽 UI

Sprint 15+ (远期)    : P3 生态
                      - 云同步(架构改动大,建议独立大版本)
                      - i18n
                      - 移动端
                      - AI 辅助
```

---

## 八、最值得优先的三件事

1. **周期性任务补全** — PRD 已承诺但未交付,用户预期差,改动小
2. **事务化写入** — 数据安全底线,改动小收益大
3. **任务依赖关系** — 打开「复杂目标」市场的钥匙,决定产品定位

---

## 九、风险与依赖

| 风险 | 影响 | 缓解 |
|------|------|------|
| 任务依赖引入循环检测复杂度 | 中 | 用 DFS 检测 DAG,单元测试覆盖 |
| 增量更新与全量重拉逻辑分叉 | 中 | 保留全量重拉作为 fallback,渐进切换 |
| 云同步架构改动大 | 高 | 独立大版本,不影响离线核心 |
| 移动端触控交互重做 | 中 | 响应式优先,复用逻辑层 |
| 数据库迁移兼容旧数据 | 中 | 每个迁移脚本回滚测试 |

---

## 十、版本里程碑规划

| 版本 | 目标 | 包含 |
|------|------|------|
| v0.2.0 | 内测完善 | P0 全部 + 移动任务拖拽 UI |
| v0.3.0 | 复杂目标支持 | P1 任务依赖 + 非线性拆解 |
| v0.4.0 | 性能优化 | P2 虚拟滚动 + 增量更新 + 原生备份 |
| v1.0.0 | 正式发布 | P2 完成 + 文档完善 + 多平台构建验证 |
| v2.0.0 | 多端同步 | P3 云同步 + 移动端 + i18n |

---

*本文档基于 2026-06-27 代码状态制定,随迭代持续更新。*
