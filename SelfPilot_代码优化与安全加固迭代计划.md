# SelfPilot 代码优化与安全加固迭代计划

> **版本**: v1.0
> **制定日期**: 2026-08-08
> **基于代码状态**: 全量代码重复性分析 + 全量安全漏洞审计
> **目标**: 消除冗余代码、修复安全漏洞，显著减少项目体积并提升可维护性与安全性

***

## 目录

- [一、工作目标与范围](#一工作目标与范围)
- [二、现状分析摘要](#二现状分析摘要)
- [三、详细实施步骤](#三详细实施步骤)
- [四、各阶段时间节点](#四各阶段时间节点)
- [五、负责人分配](#五负责人分配)
- [六、资源需求](#六资源需求)
- [七、风险评估与应对策略](#七风险评估与应对策略)
- [八、质量验收标准](#八质量验收标准)
- [九、预期成果](#九预期成果)
- [附录 A：代码重复性分析明细](#附录-a代码重复性分析明细)
- [附录 B：安全漏洞清单](#附录-b安全漏洞清单)

***

## 一、工作目标与范围

### 1.1 工作目标

| 维度 | 目标 | 量化指标 |
|------|------|----------|
| **代码瘦身** | 消除重复代码、提取公共模块 | 前端减少 ~2600 行（20-25%），后端减少 ~700 行（15-18%） |
| **安全加固** | 修复全部高危/中危漏洞，覆盖低危项 | 高危 0 漏洞，中危≤1（可接受残余），低危纳入跟踪 |
| **可维护性** | 统一模式、收敛样板代码 | 新增 4 个共享模块、2 个公共组件、1 个工具 crate 模块 |
| **功能保真** | 优化与加固过程不改变现有功能 | 回归测试用例 100% 通过 |

### 1.2 范围界定

**纳入范围**：
- 前端：`src/` 下全部 `.vue` 与 `.ts` 文件（views、components、stores、api、composables、types）
- 后端：`src-tauri/src/` 下全部 `.rs` 文件（commands、services、db、error、lib、main）
- 配置：`tauri.conf.json`、`capabilities/default.json`、`package.json`、`Cargo.toml`
- 迁移：`src-tauri/migrations/` 下 SQL 文件（仅涉及新增迁移，不修改已执行迁移）

**不纳入范围**：
- 功能性新增需求（属于 `SelfPilot_功能迭代计划.md` 范畴）
- UI/UX 视觉重构（属于 `日历视图迭代评估.md` 范畴）
- 已执行过的数据库迁移文件内容修改（通过新增迁移幂等补录）

***

## 二、现状分析摘要

### 2.1 代码重复性分析结果

| 重复模式 | 出现次数 | 涉及文件数 | 预估减少行数 |
|---------|---------|-----------|------------|
| **前端：CalendarView 新旧版本并存** | 1 整文件 | 6 | ~2100 |
| **前端：API invoke 样板** | 72 | 7 | ~80-100 |
| **前端：message.error 错误处理** | 35 | 7 | ~70 |
| **前端：局部更新三连调用** | 13+19 | 4 | ~50 |
| **前端：getTaskAriaLabel 重复** | 3 份 | 3 | ~40 |
| **前端：批量操作工具栏 UI** | 4 | 4 | ~60 |
| **前端：快速创建任务表单** | 3 | 2 | ~80 |
| **前端：重复类型/常量定义** | ~10 | 6 | ~30 |
| **后端：streak 计算逻辑重复** | 3 份 | 2 | ~250 |
| **后端：tasks 表 16 列 INSERT** | 7 | 3 | ~150 |
| **后端：fetch_one_or_not_found 样板** | 24 | 3 | ~50 |
| **后端：4 个 split 函数结构重复** | 4 | 1 | ~100 |
| **后端：日期范围填充循环** | 3 | 1 | ~40 |
| **后端：时间戳/UUID 生成样板** | 24 | 5 | ~24 |
| **后端：SQL 迁移幂等 INSERT 模式** | 62 | 1 | ~190 |
| **后端：stages 表死代码** | 5 结构体 | 1 | ~60 |
| **合计** | — | — | **~3300** |

### 2.2 安全漏洞检测结果

| 风险等级 | 数量 | 关键项 |
|---------|------|--------|
| **高危** | 3 | CSP 完全关闭、备份功能路径遍历、SQLite 外键未启用 |
| **中危** | 4 | SQL 拼接隐患、文件写入路径无范围限制、错误信息泄露、入参缺乏校验 |
| **低危** | 6 | panic 致崩溃、动态 SQL 模式、数据库文件权限、依赖审计缺失、无用依赖、持久化插件版本旧 |

> 完整漏洞清单见附录 B。

***

## 三、详细实施步骤

### 阶段一：P0 安全紧急修复（高危漏洞）

#### 任务 S-01：配置内容安全策略（CSP）

- **对应漏洞**：SEC-H-02（CSP 完全关闭）
- **操作**：
  1. 修改 `src-tauri/tauri.conf.json` 的 `security.csp` 字段，由 `null` 改为严格策略：`"default-src 'self'; img-src 'self' data: blob:; style-src 'self' 'unsafe-inline'; script-src 'self'; connect-src 'self' ipc: http://ipc.localhost"`
  2. 在 `index.html` 的 `<head>` 补充 CSP meta 标签作为双重保障
  3. 全量回归测试，确认 Naive UI 样式、UnoCSS、ECharts 图表渲染正常
- **验收**：应用正常启动，浏览器开发者工具无 CSP 违规报告，ECharts/Naive UI 渲染正常
- **影响文件**：`src-tauri/tauri.conf.json`、`index.html`

#### 任务 S-02：备份功能路径遍历修复

- **对应漏洞**：SEC-H-01（路径遍历/任意文件读写）、SEC-M-02（文件写入无范围限制）、SEC-M-03（restore 无类型校验）
- **操作**：
  1. 在 `src-tauri/src/commands/backup.rs` 新增路径校验函数 `validate_path_scope(path, must_be_sqlite)`：调用 `std::fs::canonicalize` 规范化路径；校验路径是否在允许目录白名单内；`restore_database` 增加扩展名校验（`.db`/`.sqlite`）+ SQLite 魔术字校验（前 16 字节 `SQLite format 3\0`）；`backup_database` 的 `VACUUM INTO` 目标若已存在则拒绝覆盖
  2. 前端 `src/api/backup.ts` 增加路径基本校验（拒绝空字符串、含空字节、含 `..` 的路径）
  3. 所有备份命令强制要求路径来自 `@tauri-apps/plugin-dialog` 原生对话框
- **验收**：尝试传入越界路径时后端返回 `Param` 错误；restore 非 SQLite 文件时被拒绝
- **影响文件**：`src-tauri/src/commands/backup.rs`、`src/api/backup.ts`

#### 任务 S-03：启用 SQLite 外键约束

- **对应漏洞**：SEC-H-03（外键未启用致数据完整性问题）
- **操作**：
  1. 修改 `src-tauri/src/lib.rs` 连接池初始化，使用 `SqlitePoolOptions::after_connect` 对每个连接执行 `PRAGMA foreign_keys = ON`
  2. 全量回归测试，验证级联删除（删 goal 时 task/encouragement_show_log 联动清理）正常
  3. 逐步移除 `task.rs`、`encouragement.rs`、`goal.rs` 中手动级联 DELETE 的冗余代码（移至阶段二执行，避免一次性改动过大）
- **验收**：删除目标后关联任务、日志自动清理；无孤儿记录
- **影响文件**：`src-tauri/src/lib.rs`，后续涉及 `commands/goal.rs`、`commands/task.rs`

---

### 阶段二：P1 代码瘦身高收益项

#### 任务 R-01：完成日历视图重构（最大收益项）

- **对应重复**：DUP-F-01（CalendarView 新旧并存，~2100 行）
- **操作**：
  1. 核对 `CalendarViewNew.vue` 与旧版 `CalendarView.vue` 功能差异清单：补齐 `handleCreateTask`（当前第 212-215 行为 TODO）；补齐批量操作事件（当前第 284-287 行为空函数）；验证拖拽改期、负载圆环、筛选栏、快速创建表单功能完整
  2. 修改 `src/App.vue` 第 40-42 行导入，指向 `CalendarViewNew.vue`
  3. 删除旧版 `src/views/CalendarView.vue`
  4. 将 `CalendarViewNew.vue` 重命名为 `CalendarView.vue`
  5. 全量回归日历模块（月/周/日视图切换、导航、批量操作、拖拽、创建任务）
- **验收**：日历三视图功能与旧版完全一致；旧版文件已删除
- **影响文件**：`src/views/CalendarView.vue`（删除）、`src/views/CalendarViewNew.vue`（重命名）、`src/App.vue`

#### 任务 R-02：统一 streak 计算逻辑

- **对应重复**：DUP-B-01（streak 计算重复 3 份，~250 行）+ 隐性 bug
- **操作**：
  1. 新建 `src-tauri/src/services/streak_service.rs`，导出 `calc_streak(pool) -> AppResult<StreakInfo>`
  2. 迁移 `encouragement.rs` 第 607-761 行的完整算法（含 longest_streak 遍历）
  3. 让 `get_streak`、`get_streak_inner`（`encouragement.rs:896-983`）、`get_celebration_achievement`（`stats.rs:535-598`）全部调用 `calc_streak`
  4. 删除三处重复实现
  5. **修复隐性 bug**：`get_streak_inner` 中 longest_streak 被错误简化为 `= current_streak`，统一后与 `get_streak` 保持一致
- **验收**：streak 数值与优化前 `get_streak` 一致；longest_streak 正确反映历史最长
- **影响文件**：新增 `src-tauri/src/services/streak_service.rs`；修改 `commands/encouragement.rs`、`commands/stats.rs`、`services/mod.rs`

#### 任务 R-03：提取后端公共数据库操作

- **对应重复**：DUP-B-02（INSERT 样板）、DUP-B-03（NotFound 样板）、DUP-B-04（split 函数结构）
- **操作**：
  1. 新建 `src-tauri/src/db/helpers.rs`，提取：`fetch_one_or_found<T>(pool, id, label)`（替代 24 处 NotFound 样板）；`exists_by_id(pool, table, id)`（替代 5 处存在性检查）；`insert_task_row(tx, task)`（替代 7 处 16 列 INSERT 样板）；`insert_tasks_batch(tx, &Vec<Task>)`（替代 4 处 split 批量插入）；`setting_upsert(pool, key, value)`（替代 3 处 settings upsert）
  2. 逐个文件替换调用点，保持行为不变
  3. 全量回归测试
- **验收**：CRUD 操作结果一致；`cargo build` 无警告
- **影响文件**：新增 `src-tauri/src/db/helpers.rs`；修改 `commands/goal.rs`、`commands/task.rs`、`commands/encouragement.rs`、`commands/backup.rs`、`commands/settings.rs`、`db/mod.rs`

---

### 阶段三：P2 共享模块与公共组件

#### 任务 R-04：提取前端工具模块

- **对应重复**：DUP-F-05（ariaLabel）、DUP-F-06（getDayStats）、DUP-F-07（阻塞提示）、DUP-F-11（重复类型/常量）
- **操作**：
  1. 新建 `src/utils/task.ts`，提取：`getTaskAriaLabel(task)`、`getDayAriaLabel(day, stats)`、`getDayStats(tasks)`/`getDayStatsCached`、`getBlockedTooltip(task)`（统一"前置任务未完成"文案，替代 20 处）
  2. 新建 `src/utils/calendar.ts`，提取负载常量与函数：`LOAD_THRESHOLD_*`/`LOAD_COLORS`/`LOAD_MAX_CAPACITY`/`RING_RADIUS`；`getLoadLevel`/`getLoadColor`/`getRingDashArray`；`statusOptions`/`weekDays` 常量
  3. 在 `src/types/index.ts` 统一 `TagType`、`ViewMode` 定义，删除各文件局部定义
  4. 替换全部调用点
- **验收**：aria 标签、负载圆环、完成率计算结果一致
- **影响文件**：新增 `src/utils/task.ts`、`src/utils/calendar.ts`；修改 `src/types/index.ts` 及多个组件/视图

#### 任务 R-05：提取公共组件

- **对应重复**：DUP-F-08（批量工具栏）、DUP-F-09（快速创建表单）
- **操作**：
  1. 新建 `src/components/BatchToolbar.vue` —— 封装全选/清空/批量完成/批量跳过 + 已选计数，替代 4 处重复模板
  2. 新建 `src/components/QuickTaskForm.vue` —— 封装任务名+目标+数量表单，支持 `inline`/`modal` 两种模式，替代 3 处重复表单
  3. 在 `CalendarWeekView.vue`、`CalendarDayView.vue`、`CalendarMonthView.vue` 中替换为公共组件
- **验收**：批量操作、快速创建功能与原实现一致
- **影响文件**：新增 2 个组件；修改 3 个日历视图子组件

#### 任务 R-06：封装 API 工厂与错误处理

- **对应重复**：DUP-F-02（invoke 样板）、DUP-F-03（错误处理）、DUP-F-04（局部更新三连）
- **操作**：
  1. 新建 `src/api/client.ts`，封装类型安全 invoke 工厂 `defineApi<TInput, TOutput>(cmd)`
  2. 将 7 个 API 文件的 72 个样板函数改为声明式定义
  3. 新建 `src/composables/useAsyncAction.ts`，统一 try/catch + message.error + loading 状态，替代 35 处重复
  4. 在 `goalStore` 中封装 `applyTaskUpdate(updated, { reload? })`，替代 13+19 处三连调用
- **验收**：API 调用结果一致；错误提示行为一致
- **影响文件**：新增 `src/api/client.ts`、`src/composables/useAsyncAction.ts`；修改 7 个 API 文件、`stores/goalStore.ts`、多个视图

---

### 阶段四：P2 安全加固（中危漏洞）

#### 任务 S-04：错误信息脱敏

- **对应漏洞**：SEC-M-04（错误信息泄露）
- **操作**：
  1. 修改 `src-tauri/src/error.rs` 的 `Serialize` 实现，区分面向用户错误（`Param`/`Business`/`NotFound` 返回原消息）与内部错误（`Db`/`Migrate`/`Internal` 返回笼统"内部错误"）
  2. 详细错误信息通过 `tracing` 日志记录
  3. 在 `lib.rs` 初始化 `tracing` subscriber，日志写入应用数据目录
- **验收**：前端不再接收 SQL 语句/表名/文件路径等内部信息
- **影响文件**：`src-tauri/src/error.rs`、`src-tauri/src/lib.rs`、`Cargo.toml`（新增 tracing 依赖）

#### 任务 S-05：入参校验加固

- **对应漏洞**：SEC-M-05（入参缺乏校验）、SEC-M-06（导入数据无校验）
- **操作**：
  1. 引入 `validator` crate，为 `CreateGoalInput`、`CreateTaskInput`、`AddEncouragementInput`、`SetSettingInput` 等添加校验：字符串长度上限（name ≤ 200，text ≤ 100）；数值非负有限（plan_qty、total_qty、estimated_qty）；日期格式校验（`yyyy-MM-dd`）；批量操作数组长度上限（≤ 500）
  2. `import_data` 增加：JSON 大小限制（≤ 50MB）；settings key 白名单过滤；status/source/category/level 枚举值合法性校验
- **验收**：超长字符串、负数、非法日期、超大 JSON 被拒绝
- **影响文件**：`src-tauri/src/commands/` 多个文件、`src-tauri/src/db/models.rs`、`Cargo.toml`

#### 任务 S-06：替换 panic 为错误返回

- **对应漏洞**：SEC-L-01（panic 致崩溃）
- **操作**：
  1. 修改 `encouragement.rs` 第 157 行 `panic!("candidates should not be empty")` 为返回 `AppError::Business`
  2. 全量检索其他 `panic!`/`unwrap()`/`expect()` 在命令函数中的使用，替换为 `?` 或 `AppError`
- **验收**：空候选列表时返回错误提示而非崩溃
- **影响文件**：`src-tauri/src/commands/encouragement.rs`

---

### 阶段五：P3 后端清理与依赖治理

#### 任务 R-07：提取工具函数与清理死代码

- **对应重复**：DUP-B-05（时间戳/UUID 样板）、DUP-B-08（stages 死代码）
- **操作**：
  1. 新建 `src-tauri/src/util.rs`，提取：`now_local_ts()`、`now_utc_rfc3339()`、`new_uuid()`、`parse_date_or_param(s, label)`
  2. **统一时间格式**：全项目统一使用 `Local` 或 `Utc::to_rfc3339()`，消除三种格式并存
  3. 删除 `models.rs` 第 143-198 行的 stages 死代码（5 个 `#[allow(dead_code)]` 结构体）
  4. 移除 `backup.rs` 中 `ExportData.stages` 兼容字段（需确认前端不依赖）
  5. 替换 24 处时间戳/UUID 调用点
- **验收**：`cargo build` 无 dead_code 警告；时间格式统一
- **影响文件**：新增 `src-tauri/src/util.rs`；修改 `commands/` 多个文件、`db/models.rs`、`commands/backup.rs`

#### 任务 R-08：合并 stats 日期范围填充循环

- **对应重复**：DUP-B-07（日期填充循环重复 3 处）
- **操作**：
  1. 在 `stats.rs` 内提取私有函数 `fill_date_range(start, end, map) -> impl Iterator`
  2. 替换 `get_completion_trend`、`get_heatmap`、`get_daily_load` 三处内联循环
- **验收**：统计结果一致
- **影响文件**：`src-tauri/src/commands/stats.rs`

#### 任务 S-07：依赖治理

- **对应漏洞**：SEC-L-04（无用依赖）、SEC-L-06（依赖审计缺失）、SEC-L-05（持久化插件版本旧）
- **操作**：
  1. 从 `package.json` 移除未使用的 `@tauri-apps/plugin-fs`
  2. 升级 `pinia-plugin-persistedstate` 至 `^4.x`，回归测试
  3. 升级 `thiserror` 至 `^2.x`
  4. 新增 `cargo-audit` 到 CI 流程，执行 `cargo audit` 基线扫描
  5. 执行 `npm audit` 并修复高危项
- **验收**：`npm audit`/`cargo audit` 无高危；构建正常
- **影响文件**：`package.json`、`src-tauri/Cargo.toml`、CI 配置

---

### 阶段六：P3 迁移文件优化与命令注册重构

#### 任务 R-09：迁移文件幂等补录

- **对应重复**：DUP-B-09（009 文件 62 次幂等 INSERT 模式）
- **操作**：
  1. **不修改**已执行的 `009_encouragement_p1.sql`
  2. 新建 `015_encouragement_consolidate.sql`，用多值 `INSERT OR IGNORE INTO ... VALUES (...),(...)` 幂等补录 52 条鼓励语（仅插入缺失项）
  3. 验证补录后数据库鼓励语数量与预期一致
- **验收**：新迁移执行成功；鼓励语数据完整无重复
- **影响文件**：新增 `src-tauri/migrations/015_encouragement_consolidate.sql`

#### 任务 R-10：命令注册分模块

- **对应重复**：DUP-B-10（lib.rs 89 个命令手动注册）
- **操作**：
  1. 在每个 `commands/X.rs` 模块暴露 `pub fn register(builder: Builder)` 方法
  2. `lib.rs` 调用各模块 `register`，减轻单文件 90 行列表
- **验收**：应用启动正常；所有命令可调用
- **影响文件**：`src-tauri/src/lib.rs`、`src-tauri/src/commands/` 各模块

***

## 四、各阶段时间节点

> 时间节点按工作日估算，可并行项已标注。

| 阶段 | 任务编号 | 任务名称 | 预计耗时 | 依赖关系 | 里程碑 |
|------|---------|---------|---------|---------|--------|
| **阶段一** | S-01 | 配置 CSP | 0.5 天 | 无 | M1：高危漏洞清零 |
| | S-02 | 路径遍历修复 | 1 天 | 无 | |
| | S-03 | 启用外键约束 | 1 天 | 无 | |
| **阶段二** | R-01 | 日历视图重构 | 2 天 | 无 | M2：代码瘦身 50% |
| | R-02 | 统一 streak 计算 | 1 天 | 无（可与 R-01 并行） | |
| | R-03 | 提取后端公共操作 | 1.5 天 | 无（可与 R-01/R-02 并行） | |
| **阶段三** | R-04 | 提取前端工具模块 | 1 天 | R-01 完成 | M3：共享模块就位 |
| | R-05 | 提取公共组件 | 1 天 | R-01 完成（可与 R-04 并行） | |
| | R-06 | 封装 API 与错误处理 | 1 天 | 无（可与 R-04/R-05 并行） | |
| **阶段四** | S-04 | 错误信息脱敏 | 0.5 天 | 无 | M4：中危漏洞清零 |
| | S-05 | 入参校验加固 | 1.5 天 | 无 | |
| | S-06 | 替换 panic | 0.5 天 | 无 | |
| **阶段五** | R-07 | 工具函数与死代码清理 | 1 天 | R-03 完成 | M5：后端清理完成 |
| | R-08 | 合并 stats 循环 | 0.5 天 | R-03 完成 | |
| | S-07 | 依赖治理 | 0.5 天 | 无 | |
| **阶段六** | R-09 | 迁移文件补录 | 0.5 天 | S-03 完成 | M6：全部完成 |
| | R-10 | 命令注册重构 | 0.5 天 | R-03 完成 | |

**总预计耗时**：约 12-14 工作日（含并行优化），串行约 18 工作日。

***

## 五、负责人分配

| 角色 | 职责 | 负责任务 |
|------|------|---------|
| **安全工程师** | 高危/中危漏洞修复、CSP 配置、路径校验、错误脱敏 | S-01、S-02、S-03、S-04、S-05、S-06 |
| **后端工程师** | Rust 代码重构、公共模块提取、迁移补录 | R-02、R-03、R-07、R-08、R-09、R-10、S-06 |
| **前端工程师** | Vue 代码重构、公共组件提取、API 封装 | R-01、R-04、R-05、R-06 |
| **QA 工程师** | 回归测试、验收测试、体积对比 | 全部任务的验收 |
| **项目负责人** | 进度跟踪、风险协调、资源调度 | 全程 |

> 若团队规模较小，前端/后端可由同一人按阶段顺序执行，安全修复优先。

***

## 六、资源需求

### 6.1 人力资源

| 角色 | 人数 | 投入占比 |
|------|------|---------|
| 安全工程师 | 1 | 50% |
| 后端工程师 | 1 | 80% |
| 前端工程师 | 1 | 80% |
| QA 工程师 | 1 | 30% |

### 6.2 工具与环境

| 工具 | 用途 |
|------|------|
| Rust toolchain（stable） | 后端编译与测试 |
| Node.js 18+ / pnpm | 前端构建与测试 |
| `cargo-audit` | Rust 依赖漏洞扫描 |
| `npm audit` | 前端依赖漏洞扫描 |
| vitest | 前端单元测试 |
| Git | 版本管理与分支策略 |

### 6.3 测试数据

- 现有数据库快照（含真实学习计划数据，用于回归验证）
- 边界测试数据集（超长字符串、负数、非法日期、超大 JSON、空候选列表）

***

## 七、风险评估与应对策略

| 风险编号 | 风险描述 | 风险等级 | 概率 | 应对策略 |
|---------|---------|---------|------|---------|
| RSK-01 | 日历重构遗漏旧版功能，导致用户感知的功能丢失 | 高 | 中 | 重构前逐项核对功能清单；重构后全量回归；保留旧版 git 历史可回退 |
| RSK-02 | 启用外键约束后，存量数据中已有孤儿记录导致迁移失败 | 高 | 中 | 启用前先执行数据清洗脚本检查孤儿记录；提供回滚迁移 |
| RSK-03 | CSP 配置过严导致 Naive UI/ECharts 样式或脚本被拦截 | 中 | 中 | 先在 dev 环境验证；保留 `style-src 'unsafe-inline'`；准备逐步收紧策略 |
| RSK-04 | streak 统一后数值变化影响鼓励语触发逻辑 | 中 | 低 | 对比优化前后 streak 值；重点测试 1/3/7 天阈值触发 |
| RSK-05 | `restore_database` 增加 SQLite 魔术字校验后，合法备份被误拒 | 中 | 低 | 校验逻辑容错（仅检查前 16 字节）；提供跳过校验的应急选项 |
| RSK-06 | 删除 stages 死代码后前端仍引用导致运行时错误 | 中 | 低 | 删除前全量检索前端 `stages` 引用；保留一个迭代周期观察 |
| RSK-07 | 依赖升级（pinia-plugin-persistedstate v4）引入 breaking change | 中 | 中 | 升级前查阅 CHANGELOG；先在分支测试；仅持久化 theme 字段影响面小 |
| RSK-08 | 入参校验过严导致历史数据导入失败 | 中 | 中 | 校验仅对新写入生效；导入功能单独做数据清洗兼容层 |
| RSK-09 | 重构改动面大，回归测试不充分 | 高 | 中 | 每个任务独立提交+测试；建立测试用例清单；关键路径人工验证 |
| RSK-10 | 时间格式统一导致历史数据时间字段解析失败 | 中 | 低 | 统一前盘点所有时间格式使用点；保持解析层兼容多种格式 |

***

## 八、质量验收标准

### 8.1 功能验收

| 验收项 | 标准 | 验证方法 |
|--------|------|---------|
| 日历视图 | 月/周/日三视图功能与优化前完全一致 | 人工回归 + 截图对比 |
| 任务 CRUD | 创建/完成/跳过/补完成/编辑/移动正常 | 功能测试用例 |
| 目标树 | 创建/拆解/重新规划/编辑/删除正常 | 功能测试用例 |
| 鼓励语 | 触发/收藏/反馈/排序/批量操作正常 | 功能测试用例 |
| 统计图表 | 柱状图/折线图/热力图/预测数据正确 | 数据对比 |
| 备份恢复 | 导出/导入 JSON、备份/恢复 DB 正常 | 端到端测试 |
| streak 计算 | 连续天数与最长连续天数正确 | 对比优化前后数值 |

### 8.2 安全验收

| 验收项 | 标准 | 验证方法 |
|--------|------|---------|
| CSP | 浏览器控制台无 CSP 违规 | devtools 检查 |
| 路径遍历 | 越界路径被拒绝 | 构造恶意路径测试 |
| restore 校验 | 非 SQLite 文件被拒绝 | 传入文本文件测试 |
| 外键约束 | 删除目标后关联记录自动清理 | 数据库检查 |
| 错误脱敏 | 前端不接收 SQL/路径等内部信息 | 故意触发错误检查响应 |
| 入参校验 | 超长/负数/非法日期被拒绝 | 边界值测试 |
| panic 清零 | 命令函数无 panic/unwrap | `cargo clippy` 检查 |

### 8.3 代码质量验收

| 验收项 | 标准 | 验证方法 |
|--------|------|---------|
| 体积减少 | 前端减少 ≥2500 行，后端减少 ≥650 行 | `git diff --stat` 对比 |
| 编译无警告 | `cargo build` + `vue-tsc --noEmit` 无警告 | 构建检查 |
| 依赖审计 | `npm audit` + `cargo audit` 无高危 | 审计工具 |
| 重复代码 | 主要重复模式消除 ≥80% | 重复性扫描复查 |
| dead code | 无 `#[allow(dead_code)]` 残留 | `cargo build` 检查 |

### 8.4 体积对比数据记录

> 优化完成后需填写本表，作为验收依据。

| 指标 | 优化前 | 优化后 | 减少量 | 减少比例 |
|------|--------|--------|--------|---------|
| 前端代码总行数 | _待填_ | _待填_ | — | — |
| 后端代码总行数 | _待填_ | _待填_ | — | — |
| `CalendarView.vue` 行数 | 2431 | 0（删除） | 2431 | 100% |
| `encouragement.rs` 行数 | _待填_ | _待填_ | — | — |
| `package.json` 依赖数 | 28 | _待填_ | — | — |
| 高危漏洞数 | 3 | 0 | 3 | 100% |
| 中危漏洞数 | 4 | _待填_ | — | — |

***

## 九、预期成果

### 9.1 可交付物清单

1. **代码**：完成全部 P0-P3 任务的代码提交（按任务粒度分 commit）
2. **文档**：本迭代计划文档（含体积对比数据回填）
3. **测试报告**：回归测试通过报告 + 安全验收报告
4. **新增模块**：
   - `src/utils/task.ts`、`src/utils/calendar.ts`
   - `src/components/BatchToolbar.vue`、`src/components/QuickTaskForm.vue`
   - `src/api/client.ts`、`src/composables/useAsyncAction.ts`
   - `src-tauri/src/services/streak_service.rs`
   - `src-tauri/src/db/helpers.rs`
   - `src-tauri/src/util.rs`
   - `src-tauri/migrations/015_encouragement_consolidate.sql`
5. **配置变更**：CSP 配置、依赖版本更新、CI 审计集成

### 9.2 量化收益

| 维度 | 收益 |
|------|------|
| 代码体积 | 减少 ~3300 行（前端 ~2600 + 后端 ~700） |
| 安全性 | 高危漏洞 3→0，中危漏洞 4→0-1 |
| 可维护性 | 样板代码减少 80%+，公共模块复用率提升 |
| Bug 修复 | 修复 streak 计算隐性 bug 1 处 |
| 技术债务 | 清理死代码、统一时间格式、收敛错误处理模式 |

***

## 附录 A：代码重复性分析明细

### A.1 前端重复项

| 编号 | 重复模式 | 出现次数 | 涉及文件 | 预估减少行数 |
|------|---------|---------|---------|------------|
| DUP-F-01 | CalendarView 新旧版本并存 | 1 整文件 | CalendarView.vue 等 6 个 | ~2100 |
| DUP-F-02 | API invoke 样板 | 72 | api/ 7 个文件 | ~80-100 |
| DUP-F-03 | message.error 错误处理 | 35 | 7 个视图/组件 | ~70 |
| DUP-F-04 | 局部更新三连调用 | 13+19 | 4 个文件 | ~50 |
| DUP-F-05 | getTaskAriaLabel 重复 | 3 份 | 3 个日历组件 | ~40 |
| DUP-F-06 | getDayStats 重复 | 3 份(29引用) | 3 个文件 | ~30 |
| DUP-F-07 | 阻塞提示文案 | 20 | 4 个文件 | ~30 |
| DUP-F-08 | 批量操作工具栏 UI | 4 | 4 处 | ~60 |
| DUP-F-09 | 快速创建任务表单 | 3 | 2 个文件 | ~80 |
| DUP-F-10 | Store fetch 模式 | 3 | 2 个 store | ~20 |
| DUP-F-11 | 重复类型/常量 | ~10 | 6 个文件 | ~30 |

### A.2 后端重复项

| 编号 | 重复模式 | 出现次数 | 涉及文件 | 预估减少行数 |
|------|---------|---------|---------|------------|
| DUP-B-01 | streak 计算逻辑重复 | 3 份 | encouragement.rs、stats.rs | ~250 |
| DUP-B-02 | tasks 表 16 列 INSERT | 7 | goal.rs、backup.rs、task.rs | ~150 |
| DUP-B-03 | fetch_one_or_not_found 样板 | 24 | goal.rs、task.rs、encouragement.rs | ~50 |
| DUP-B-04 | 4 个 split 函数结构重复 | 4 | goal.rs | ~100 |
| DUP-B-05 | 时间戳/UUID 生成样板 | 24 | 5 个文件 | ~24 |
| DUP-B-06 | settings upsert 重复 | 3 | settings.rs、backup.rs | ~20 |
| DUP-B-07 | 日期范围填充循环 | 3 | stats.rs | ~40 |
| DUP-B-08 | stages 表死代码 | 5 结构体 | models.rs | ~60 |
| DUP-B-09 | SQL 迁移幂等 INSERT 模式 | 62 | 009_encouragement_p1.sql | ~190 |
| DUP-B-10 | 命令注册手动列表 | 89 | lib.rs | — (结构性) |

***

## 附录 B：安全漏洞清单

| 编号 | 风险等级 | 漏洞名称 | 位置 | 对应任务 |
|------|---------|---------|------|---------|
| SEC-H-01 | 高危 | 备份功能路径遍历/任意文件读写 | backup.rs:53-72, 389-447 | S-02 |
| SEC-H-02 | 高危 | CSP 内容安全策略完全关闭 | tauri.conf.json:26-28 | S-01 |
| SEC-H-03 | 高危 | SQLite 外键约束未启用 | lib.rs:22-36 | S-03 |
| SEC-M-01 | 中危 | backup_database SQL 拼接隐患 | backup.rs:399-403 | S-02 |
| SEC-M-02 | 中危 | 文件写入路径无范围限制 | backup.rs:53-72, 389-448 | S-02 |
| SEC-M-03 | 中危 | restore_database 无类型校验 | backup.rs:412-447 | S-02 |
| SEC-M-04 | 中危 | 错误信息泄露内部实现 | error.rs:5-33 | S-04 |
| SEC-M-05 | 中危 | 命令入参缺乏校验 | commands/ 多个文件 | S-05 |
| SEC-M-06 | 中危 | 导入数据无内容校验 | backup.rs:78-380 | S-05 |
| SEC-L-01 | 低危 | weighted_random_pick panic | encouragement.rs:157 | S-06 |
| SEC-L-02 | 低危 | 动态拼接 SQL 模式脆弱 | goal.rs:170-209, task.rs:446-497 | 跟踪 |
| SEC-L-03 | 低危 | VACUUM INTO 字符串插值 | backup.rs:399-401 | S-02 |
| SEC-L-04 | 低危 | 数据库文件权限未设置 | lib.rs:19 | 跟踪 |
| SEC-L-05 | 低危 | 依赖版本宽泛无审计 | Cargo.toml | S-07 |
| SEC-L-06 | 低危 | 无用 plugin-fs 依赖 | package.json:17 | S-07 |
| SEC-L-07 | 低危 | 持久化插件版本偏旧 | package.json:24 | S-07 |
| SEC-L-08 | 低危 | 资源密集型命令无速率限制 | backup 相关命令 | 跟踪 |
| SEC-L-09 | 低危 | dialog content 字符串拼接 | GoalTreeView.vue 等 | 跟踪 |

> **备注**：前端未发现 XSS（无 v-html/innerHTML/eval）、CSRF/SSRF（无网络请求）、命令注入（无 shell 插件）。后端未发现命令注入（无 std::process::Command）。SQL 注入在绝大多数查询中已通过参数化绑定规避，仅 VACUUM INTO 因 SQLite 限制使用转义拼接（已做单引号转义，风险可控）。

***

> **文档结束** | 本计划随实施进度持续更新，体积对比数据在验收阶段回填。
