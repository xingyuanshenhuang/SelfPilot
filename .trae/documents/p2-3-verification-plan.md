# P2-3 增量更新验证计划

## Context

P2-3（增量更新替代全量重拉）已完成核心改造，7 个高频写操作改用局部 mutation。但需验证：
1. **局部更新生效** — 确认改用局部 mutation 的操作确实不再全量重拉
2. **功能正确性** — 操作后前端状态与后端一致（任务状态、进度、依赖）
3. **CalendarView 遗漏修复** — CalendarView 中 4 个写操作未改造

## 发现的问题

### CalendarView 未改造（需修复）

`CalendarView.vue` 中 4 个操作仍使用 `goalStore.fetchProgresses()` + `loadData()` 全量重拉：
- `batchComplete()` (L208-223)
- `batchSkip()` (L226-246)
- `quickComplete()` (L249-257)
- `quickSkip()` (L260-277)

这些操作调用 `taskApi.completeTask/skipTask` 但**丢弃了返回值**，需改为保存返回的 `updated: Task`，调用 `goalStore.updateTaskLocally(updated)` + `goalStore.refreshProgressForGoalChain(updated.goal_id)` 替代 `goalStore.fetchProgresses()`。仍需 `loadData()` 刷新日历视图数据。

## 实施步骤

### Step 1: 修复 CalendarView.vue 写操作（4 处）

- `batchComplete`: 循环中收集 updated tasks + affected goal_ids，循环后批量 refreshProgressForGoalChain
- `batchSkip`: 同上模式
- `quickComplete`: 保存返回值，局部更新 + 局部进度 + loadData
- `quickSkip`: 同上模式

### Step 2: 添加临时验证探针

在 `goalStore.ts` 中添加 `[P2-3-VERIFY]` console.log，区分「局部更新」和「全量重拉」，便于在 DevTools Console 中观察。

### Step 3: 手动测试清单

| 操作 | 预期日志 | 不应出现 |
|---|---|---|
| 完成任务 | `updateTaskLocally` + `refreshProgressForGoalChain` | `fetchGoalTree` / `fetchProgresses` |
| 跳过任务 | 同上 | 同上 |
| 补完成 | 同上 | 同上 |
| 删除任务 | `removeTaskLocally` + `refreshProgressForGoalChain` | 同上 |
| 编辑任务 | `updateTaskLocally` + `refreshProgressForGoalChain` | 同上 |
| 编辑目标 | `updateGoalLocally` + `refreshProgressForGoalChain` | 同上 |
| 创建任务 | `fetchGoalTree` + `fetchProgresses`（全量，正确） | — |
| 创建目标 | `fetchGoalTree` + `fetchProgresses`（全量，正确） | — |
| 自动拆解 | 全量（正确） | — |
| 删除目标 | 全量（正确） | — |
| 移动任务 | 全量（正确） | — |

同时验证 UI：完成→✓进度更新，跳过→⊘进度不增，删除→消失进度更新，编辑→立即反映，嵌套目标祖先进度都更新。

### Step 4: 状态漂移校验

完成操作后，对比全量重拉与局部状态是否一致。

### Step 5: 清理探针

验证完成后，删除所有 `[P2-3-VERIFY]` console.log 语句。

## 涉及文件

- `src/views/CalendarView.vue` — 修复 4 个写操作
- `src/stores/goalStore.ts` — 添加临时探针（Step 2），最终清理（Step 5）
