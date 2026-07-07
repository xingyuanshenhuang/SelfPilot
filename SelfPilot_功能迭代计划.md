# SelfPilot 功能迭代计划

> **版本**: v1.0
> **制定日期**: 2026-06-27
> **基于代码状态**: README 声明完成度 85%\~90%,Sprint 1-6 已交付
> **目标**: 在内测可用基础上,补齐短板、扩展能力、优化体验、规划生态

***

## 目录

- [一、当前能力盘点](#一当前能力盘点)
- [二、已识别的能力缺口](#二已识别的能力缺口)
- [三、迭代总览路线图](#三迭代总览路线图)
- [四、P0 — 补齐内测短板](#四p0--补齐内测短板)
- [五、P1 — 扩展任务模型](#五p1--扩展任务模型)
- [六、P2 — 性能与体验](#六p2--性能与体验)
- [七、P3 — 生态扩展](#七p3--生态扩展)
- [八、技术约束与设计原则](#八技术约束与设计原则)
- [九、验收标准模板](#九验收标准模板)

***

## 一、当前能力盘点

### 1.1 已交付功能矩阵

| 模块       | 功能点          | 实现位置                                          | 完成度           |
| -------- | ------------ | --------------------------------------------- | ------------- |
| **目标树**  | 创建总目标/子目标    | `commands::goal::create_goal`                 | ✅             |
| <br />   | 视频拆解(总量摊分)   | `split_service::split_goal_into_tasks`        | ✅             |
| <br />   | 重复拆解(每日)     | `split_service::split_repeat_tasks`           | ⚠️ 仅每日重复      |
| <br />   | 重新规划(预览+执行)  | `commands::goal::replan_*`                    | ✅             |
| <br />   | 目标编辑         | `commands::goal::update_goal`                 | ⚠️ UI 不完整     |
| <br />   | 目标树展开/折叠     | `GoalTreeView.vue`                            | ✅             |
| <br />   | 任务拖拽归属       | —                                             | ❌ 缺失          |
| **任务操作** | 完成/部分完成      | `commands::task::complete_task`               | ✅             |
| <br />   | 跳过           | `commands::task::skip_task`                   | ✅             |
| <br />   | 补完成          | `commands::task::backfill_task`               | ✅             |
| <br />   | 任务编辑         | `commands::task::update_task`                 | ✅             |
| <br />   | 移动归属         | `commands::task::move_task`                   | ✅ 命令已有,UI 缺拖拽 |
| **今日待办** | 今日任务+逾期任务    | `commands::task::list_today_tasks`            | ✅             |
| **日历视图** | 月/周/日三视图     | `CalendarView.vue`                            | ✅             |
| <br />   | 批量完成/跳过      | —                                             | ✅             |
| **数据统计** | 柱状图(目标完成度)   | `commands::stats::get_goal_completion_stats`  | ✅             |
| <br />   | 折线图(7/30天趋势) | `commands::stats::get_completion_trend`       | ✅             |
| <br />   | 日历热力图        | `commands::stats::get_heatmap`                | ✅             |
| <br />   | 完成预测         | `commands::stats::get_completion_predictions` | ✅             |
| **鼓励语库** | 预设+自定义       | `commands::encouragement`                     | ✅             |
| <br />   | 等级触发(1/3/7天) | `random_encouragement_by_streak`              | ✅             |
| <br />   | 庆祝语          | `random_celebration_encouragement`            | ✅             |
| **设置**   | 主题切换         | `settingStore`                                | ✅             |
| <br />   | JSON 导出/导入   | `commands::backup`                            | ✅             |

### 1.2 核心数据模型现状

```
goals (id, parent_id, path, deadline, total_qty, unit, sort_order)
  └─ tasks (id, goal_id, stage_id[废弃], parent_id, path,
            name, plan_date, plan_qty, actual_qty, unit,
            status[pending|partial|done|skipped], is_manual,
            source[auto|manual], sort_order)
```

### 1.3 关键算法现状

- **进度汇总**:`progress_service::calc_all_goals_progress` — 3 次查询 + 内存递归,带缓存
- **完成判定**:非 skipped 任务全 done + 所有子目标完成 → 父目标完成
- **拆解**:仅支持「均匀分配 + 余数前置」单一策略

***

## 二、已识别的能力缺口

### 2.1 功能性缺口

| 缺口               | 影响              | 严重度 |
| ---------------- | --------------- | --- |
| 周期任务仅每日,无每周/每月   | PRD 承诺未交付,用户预期差 | P0  |
| 目标树手动添加任务 UI 不完整 | 无法在树内任意节点建任务    | P0  |
| 任务拖拽归属缺失         | 无法灵活重组任务归属      | P0  |
| 任务依赖关系缺失         | 复杂学习路径无法自动排序    | P1  |
| 非线性阶段拆解          | 各阶段无法差异化节奏      | P1  |
| 时间预算模型           | 无法按可用时长拆解       | P1  |
| 跨目标负载平衡          | 多目标并行任务爆炸       | P2  |
| 里程碑节点            | 无关键检查点          | P2  |

### 2.2 非功能性缺口

| 缺口            | 影响                         | 严重度 |
| ------------- | -------------------------- | --- |
| 写操作未事务化       | `delete_goal` 循环删除异常中断留脏数据 | P0  |
| 前端全量重拉        | 数据量大时卡顿                    | P2  |
| 前端层级硬编码两层     | 多层嵌套不显示                    | P2  |
| SQLite 原生备份缺失 | JSON 导出大数据慢                | P2  |

***

## 三、迭代总览路线图

```
P0 (近期 1-2 Sprint)  ── 补短板:周期任务 / 拖拽 / 事务化 / 手动任务 UI
        ↓
P1 (中期 3-5 Sprint)  ── 扩模型:依赖关系 / 非线性阶段 / 时间预算
        ↓
P2 (长期 6+ Sprint)  ── 优体验:虚拟滚动 / 增量更新 / 原生备份 / 负载平衡
        ↓
P3 (远期)            ── 拓生态:云同步 / i18n / 移动端 / AI 辅助
```

### 3.1 优先级定义

| 优先级    | 定义                 | 准入条件        |
| ------ | ------------------ | ----------- |
| **P0** | 阻塞内测发布,补齐 PRD 承诺   | 无前置依赖,改动可控  |
| **P1** | 决定产品定位(待办 vs 规划系统) | P0 完成后启动    |
| **P2** | 体验与性能,非功能性         | 用户反馈或性能瓶颈出现 |
| **P3** | 生态扩展,需架构调整         | P2 稳定后启动    |

***

## 四、P0 — 补齐内测短板 

### P0-1 周期性任务补全(每周/每月)

**背景**:`split_repeat_tasks` 当前只支持每日重复,PRD §4.2 承诺「每周几/每月几号」未实现。

**数据模型变更**:

```sql
-- migrations/005_periodic_task.sql
-- RepeatSplitInput 扩展字段(无需新表,复用 tasks 表)
-- 通过 source='manual' + is_manual=1 标识周期任务实例
```

**接口设计**:

```rust
// models.rs
pub struct RepeatSplitInput {
    pub goal_id: String,
    pub name: String,
    pub start_date: String,
    pub end_date: Option<String>,
    pub plan_qty: Option<f64>,
    pub unit: Option<String>,
    /// 新增:频率 daily | weekly | monthly
    pub frequency: Option<String>,
    /// 新增:周几(0=周日,1-6=周一至周六),仅 weekly 有效
    pub weekdays: Option<Vec<u8>>,
    /// 新增:每月几号(1-31),仅 monthly 有效
    pub month_days: Option<Vec<u8>>,
}
```

**Service 改动**:`split_service::split_repeat_tasks` 按 frequency 枚举日期:

- `daily`:保持现有逻辑
- `weekly`:遍历日期范围,`weekday` 命中 `weekdays` 集合则生成
- `monthly`:遍历日期范围,`day` 命中 `month_days` 集合则生成

**前端改动**:`GoalTreeView.vue` 重复拆解弹窗增加频率选择器 + 周几/月几多选。

**验收标准**:

> **Given** 用户选择 weekly + 勾选 周一、周三、周五,日期范围 2026-07-01 \~ 2026-07-31
> **When** 点击生成
> **Then** 仅在 7 月所有周一/三/五日期生成任务,共约 13 个

***

### P0-2 目标树手动添加任务 UI 补全

**背景**:`create_task` 命令已完整,但目标树内任意节点缺少统一入口。

**改动范围**:仅 `GoalTreeView.vue`。

**实现要点**:

- 子目标头部增加「+任务」按钮(总目标已有)
- 任务列表底部增加「+ 添加任务」行
- 统一调 `openCreateTaskModal(goalId)`,复用现有弹窗

**验收标准**:

> **Given** 用户在任意子目标节点
> **When** 点击「+任务」按钮
> **Then** 弹出创建任务弹窗,goal\_id 预填为该子目标,创建后任务出现在该子目标下

***

### P0-3 任务拖拽归属

**背景**:`move_task` 命令已存在(`commands::task::move_task`),但前端无拖拽交互。

**注意**:`move_task` 当前只改 `stage_id`,需扩展为改 `goal_id`(因 stage 已废弃为子目标)。

**接口调整**:

```rust
// models.rs — MoveTaskInput 扩展
pub struct MoveTaskInput {
    pub task_id: String,
    /// 旧字段保留兼容
    pub stage_id: Option<String>,
    /// 新增:目标 goal_id(子目标或总目标)
    pub goal_id: Option<String>,
}
```

**前端改动**:

- 引入 `vuedraggable` 或基于 `@vueuse/core` 的 `useDraggable`
- 任务行增加 `draggable="true"`,绑定 dragstart/dragover/drop
- drop 时调用 `taskApi.moveTask({ task_id, goal_id: targetGoalId })`
- 乐观更新 + 失败回滚

**验收标准**:

> **Given** 子目标 A 下有任务 T
> **When** 用户将 T 拖拽到子目标 B
> **Then** T 的 goal\_id 更新为 B,目标树刷新后 T 出现在 B 下,进度重新汇总

***

### P0-4 写操作事务化

**背景**:`delete_goal` 的循环删除([goal.rs#L228-L241](file:///workspace/src-tauri/src/commands/goal.rs))未包事务,异常中断留脏数据。`replan_goal`、`import_data` 同样存在。

**改动范围**:`commands/` 下所有多步写操作。

**实现方式**:sqlx 事务封装:

```rust
let mut tx = state.0.begin().await?;
// 所有 sqlx::query 改为 sqlx::query(...).execute(&mut *tx).await?
tx.commit().await?;
```

**需事务化的命令清单**:

- `delete_goal`(级联删除 goals + tasks)
- `auto_split` / `repeat_split`(批量插入 tasks)
- `replan_goal`(批量更新 plan\_qty)
- `import_data`(批量插入 goals + tasks + encouragements + settings)

**验收标准**:

> **Given** 删除含 100 个子任务的总目标
> **When** 删除过程中第 50 个任务时模拟异常
> **Then** 事务回滚,前 49 个已删除的任务恢复,数据一致

***

## 五、P1 — 扩展任务模型

### P1-1 任务依赖关系(核心)

**背景**:复杂学习路径(学 A 才能学 B)无法表达,是「待办工具」与「规划系统」的分水岭。

**数据模型变更**:

```sql
-- migrations/006_task_dependency.sql
CREATE TABLE IF NOT EXISTS task_dependencies (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL,           -- 当前任务
    depends_on_id TEXT NOT NULL,     -- 前置任务
    created_at TEXT NOT NULL,
    FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE,
    FOREIGN KEY (depends_on_id) REFERENCES tasks(id) ON DELETE CASCADE,
    UNIQUE(task_id, depends_on_id)
);
CREATE INDEX idx_task_dep_task ON task_dependencies(task_id);
CREATE INDEX idx_task_dep_on ON task_dependencies(depends_on_id);
```

**新增命令**:

```rust
pub async fn set_task_dependency(task_id: String, depends_on_id: String) -> AppResult<()>
pub async fn list_task_dependencies(task_id: String) -> AppResult<Vec<Task>>
pub async fn remove_task_dependency(task_id: String, depends_on_id: String) -> AppResult<()>

/// 校验依赖图无环(防循环依赖)
pub async fn validate_dependency_chain(task_id: String, depends_on_id: String) -> AppResult<bool>
```

**业务规则**:

- 依赖未完成 → 当前任务在「今日待办」中标灰(不可标记完成)
- 重新规划时,依赖链决定排序(被依赖的任务排前)
- 防循环:设置依赖前用 DFS 检测环

**前端改动**:

- 任务编辑弹窗增加「前置任务」选择器(同目标下任务多选)
- 今日待办/日历视图:依赖未满足的任务显示锁图标
- 目标树:任务行增加依赖箭头指示(可选,P2 美化)

**验收标准**:

> **Given** 任务 B 依赖任务 A,A 未完成
> **When** 用户查看今日待办
> **Then** B 显示锁图标且「完成」按钮禁用;A 完成后 B 自动解锁

***

### P1-2 非线性阶段拆解

**背景**:`split_goal_into_tasks` 仅均匀分配,无法表达「前 10 天看视频每天 2 小时,后 20 天做题每天 10 道」。

**数据模型**:无需新表,利用现有子目标机制。

**实现方式**:子目标各自调用 `auto_split`,各子目标有独立 `total_qty`/`deadline`/`unit`。

**改动范围**:

- 后端:无改动(已支持子目标独立拆解)
- 前端:`GoalTreeView.vue` 子目标按钮已支持,需补文案引导

**配套优化**:增加「目标模板」功能,预设「前视频后练习」等组合模板。

**验收标准**:

> **Given** 总目标「学 Vue」,子目标 A「看视频」(40 小时/10 天)+ 子目标 B「做练习」(100 道/20 天)
> **When** 分别对 A、B 执行视频拆解
> **Then** A 每天生成 4 小时任务(10 天),B 每天生成 5 道任务(20 天),进度各自独立汇总

***

### P1-3 时间预算模型

**背景**:部分用户按「每天可学习时长」而非「总量摊分」规划。

**数据模型变更**:

```sql
-- migrations/007_task_estimation.sql
ALTER TABLE tasks ADD COLUMN estimated_hours REAL;
ALTER TABLE goals ADD COLUMN daily_capacity REAL;  -- 每日可用时长
```

**新增拆解算法**:

```rust
/// 按每日可用时长拆解(替代按天数均匀分配)
pub fn split_by_daily_capacity(
    goal: &Goal,
    today: NaiveDate,
) -> AppResult<Vec<Task>>
```

**逻辑**:`plan_qty = daily_capacity`,任务数 = `ceil(total_qty / daily_capacity)`。

**验收标准**:

> **Given** 目标总量 40 小时,每日可用 2 小时
> **When** 按时间预算拆解
> **Then** 生成 20 个任务,每个 plan\_qty=2,跨越 20 天

***

## 六、P2 — 性能与体验

### P2-1 虚拟滚动优化

**背景**:`GoalTreeView.vue` 全量渲染,节点 >500 时卡顿。

**方案**:

- 引入 `vue-virtual-scroller` 或 `naive-ui` 的 `n-virtual-list`
- 目标树改为递归组件 + 虚拟列表
- 仅渲染可视区域节点

**验收标准**:

> **Given** 目标树含 1000 个任务节点
> **When** 滚动浏览
> **Then** 保持 60fps,无明显卡顿

***

### P2-2 前端层级递归渲染

**背景**:`GoalTreeView.vue` 硬编码两层(总目标 → sub\_goals → tasks),三层以上不显示。

**方案**:抽出 `GoalTreeNode.vue` 递归组件:

```vue
<!-- GoalTreeNode.vue -->
<template>
  <NCard>
    <!-- 节点头部 -->
    <GoalTreeNode
      v-for="sub in node.sub_goals"
      :key="sub.goal.id"
      :node="sub"
    />
    <!-- tasks -->
  </NCard>
</template>
```

**验收标准**:

> **Given** 总目标 → 子目标 → 子子目标 → 任务(三层嵌套)
> **When** 渲染目标树
> **Then** 所有层级正确展示,展开折叠正常

***

### P2-3 增量更新替代全量重拉

**背景**:任何写操作后 `fetchGoalTree()` + `fetchProgresses()` 全量重拉([goalStore.ts](file:///workspace/src/stores/goalStore.ts))。

**方案**:

- Pinia store 增加局部 mutation 方法:`updateTaskLocally`、`removeGoalLocally`
- 写操作返回最新数据 → store patch 局部更新
- 进度按需重算(仅受影响的目标链)

**验收标准**:

> **Given** 完成一个任务
> **When** 触发更新
> **Then** 仅该任务及父目标链进度更新,不重拉整棵树

***

### P2-4 SQLite 原生备份

**背景**:JSON 导出大数据慢,且丢失类型信息。

**方案**:用 SQLite `VACUUM INTO`:

```rust
pub async fn backup_database(target_path: String, state: State<'_, DbPool>) -> AppResult<()> {
    sqlx::query(&format!("VACUUM INTO '{}'", target_path))
        .execute(&state.0)
        .await?;
    Ok(())
}
```

**验收标准**:

> **Given** 数据库 10MB
> **When** 执行原生备份
> **Then** 生成 .db 副本,< 2 秒完成,可恢复全部数据

***

### P2-5 跨目标负载平衡

**背景**:多目标独立拆解,某天任务总量可能爆炸。

**方案**:

- 新增 `commands::stats::get_daily_load(date_range)` 返回每日任务总量
- 拆解时弹窗提示「今日已有 X 任务,是否继续?」
- 日历视图增加每日负载色阶(绿/黄/红)

**验收标准**:

> **Given** 今日已有 5 个任务
> **When** 用户对另一目标拆解生成今日任务
> **Then** 弹出警告「今日负载较高(5个),是否继续?」

***

## 七、P3 — 生态扩展

### P3-1 云端同步(已预留)

**基础**:[tech-stack-detail.md](file:///workspace/tech-stack-detail.md) 已预留 `sync.rs` / `sync_service.rs`。

**核心问题**:

- 冲突解决策略:last-write-wins vs 字段级 merge
- 离线优先 → 在线合并的同步协议(基于 timestamp + vector clock)
- 多设备 ID 管理(需引入用户账号体系)

**阶段规划**:

1. 阶段一:只读云端拉取(查看,不编辑)
2. 阶段二:单向推送(本地 → 云端)
3. 阶段三:双向同步(完整 merge)

***

### P3-2 国际化(i18n)

**方案**:引入 `vue-i18n`,抽取所有硬编码中文文案到 locale 文件。

**工作量**:约 200+ 处文案,前端为主,Rust 侧错误消息也需 i18n(通过 error code 映射)。

***

### P3-3 移动端扩展

**基础**:Tauri 2.x 支持 iOS/Android,`src-tauri/icons` 已含移动端图标资源。

**适配项**:

- 触控交互(目标树拖拽改长按)
- 响应式布局(左侧导航改底部 Tab)
- 移动端 SQLite 性能调优(WAL 模式、连接池调整)

***

### P3-4 AI 辅助规划(增值方向)

**能力设想**:

- 基于历史完成速度,自动建议拆解粒度
- 风险预警:「按当前速度将逾期 5 天,是否重新规划?」
- 学习路径推荐(基于任务名称语义推断依赖关系)
- 智能鼓励语生成(接入 LLM)

**前置条件**:需引入可选的在线 LLM API,违背「离线优先」原则,应作为可选增强。

***

## 八、技术约束与设计原则

### 8.1 不可破坏的核心约束

| 约束                  | 说明                              | 涉及代码                                  |
| ------------------- | ------------------------------- | ------------------------------------- |
| **离线优先**            | 所有核心功能不依赖网络                     | 不引入强在线依赖                              |
| **补完成不联动未来**        | 补完成只更新 actual\_qty,绝不触发重分配      | `task::backfill_task`                 |
| **跳过视为不存在**         | skipped 不计入 plan/actual,不参与完成判定 | `progress_service`                    |
| **is\_manual 尊重人工** | 重新规划保留手动调整                      | `split_service::build_replan_preview` |
| **数据本地化**           | SQLite 存应用数据目录,无外部存储            | `lib.rs`                              |

### 8.2 新增功能设计原则

1. **后端类型对齐**:Rust `models.rs` ↔ TS `types/index.ts` 严格对应
2. **Service 层独立**:业务逻辑在 `services/`,IPC 在 `commands/`,便于复用
3. **迁移向前兼容**:新迁移用 `ALTER TABLE ADD COLUMN`,不破坏旧数据
4. **避免过度工程**:遵循 PRD 范围,不引入未要求的能力

### 8.3 提交规范

- 遵循 [Conventional Commits](https://www.conventionalcommits.org/)
- 前端:`feat: 新增目标树拖拽归属`
- 后端:`feat(task): 新增任务依赖关系命令`
- 迁移:`feat(db): 添加 task_dependencies 表`

***

## 九、验收标准模板

每个功能交付前需通过以下检查:

### 9.1 功能验收(Given-When-Then)

```
Given [前置条件]
When [用户操作]
Then [期望结果]
```

### 9.2 技术验收

- [ ] Rust:`cargo check` 通过
- [ ] 前端:`npm run build`(vue-tsc + vite build)通过
- [ ] 新增命令已在 `lib.rs` 的 `invoke_handler` 注册
- [ ] 新增类型已在 `models.rs` 和 `types/index.ts` 同步定义
- [ ] 数据库迁移脚本可幂等执行
- [ ] 涉及多步写的命令已事务化

### 9.3 回归验收

- [ ] 进度汇总正确(子任务完成 → 父目标进度更新)
- [ ] 重新规划正确(过滤 skipped,保留 manual)
- [ ] 导出/导入数据无丢失
- [ ] 主题切换无样式破坏

***

## 附录:迭代节奏建议

| 阶段           | 内容                       | 建议周期  |
| ------------ | ------------------------ | ----- |
| Sprint 7     | P0-1 周期任务 + P0-2 手动任务 UI | 1-2 周 |
| Sprint 8     | P0-3 拖拽归属 + P0-4 事务化     | 1-2 周 |
| Sprint 9-10  | P1-1 任务依赖关系              | 2-3 周 |
| Sprint 11    | P1-2 非线性阶段 + P1-3 时间预算   | 1-2 周 |
| Sprint 12-13 | P2 性能优化(虚拟滚动 + 增量更新)     | 2-3 周 |
| Sprint 14+   | P2-4 原生备份 + P2-5 负载平衡    | 按需    |
| 远期           | P3 生态扩展                  | 视资源   |

***

*本文档基于 2026-06-27 代码状态制定,随项目演进持续更新。*
