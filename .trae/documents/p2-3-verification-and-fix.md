# P2-3 增量更新验证与修复计划

## 摘要

P2-3（增量更新替代全量重拉）核心改造已完成，但验证过程中发现 **6处** 写操作仍使用 `goalStore.fetchProgresses()` 全量重拉，需修复后再进行完整验证。

## 当前状态分析

### 已完成转换（使用局部 mutation）

| 文件             | 操作                           | 状态 |
| ---------------- | ------------------------------ | ---- |
| GoalTreeView.vue | 完成任务 `handleCompleteTask`  | ✅   |
| GoalTreeView.vue | 跳过任务 `handleSkipTask`      | ✅   |
| GoalTreeView.vue | 补完成 `handleConfirmBackfill` | ✅   |
| GoalTreeView.vue | 删除任务 `handleDeleteTask`    | ✅   |
| GoalTreeView.vue | 批量删除 `handleDeleteBatch`   | ✅   |
| GoalTreeView.vue | 编辑任务 `handleSaveTask`      | ✅   |
| GoalTreeView.vue | 编辑目标 `handleSaveGoal`      | ✅   |
| CalendarView.vue | 批量完成 `batchComplete()`     | ✅   |

### 未转换（仍使用全量重拉）— 需修复

| 文件             | 操作              | 行号     | 当前模式                      |
| ---------------- | ----------------- | -------- | ----------------------------- |
| CalendarView.vue | `batchSkip()`     | L237-258 | `goalStore.fetchProgresses()` |
| CalendarView.vue | `quickComplete()` | L260-269 | `goalStore.fetchProgresses()` |
| CalendarView.vue | `quickSkip()`     | L271-288 | `goalStore.fetchProgresses()` |
| TaskItem.vue     | `doComplete()`    | L81-93   | `goalStore.fetchProgresses()` |
| TaskItem.vue     | `doBackfill()`    | L96-109  | `goalStore.fetchProgresses()` |
| TaskItem.vue     | `handleSkip()`    | L116-132 | `goalStore.fetchProgresses()` |

### 设计决策：哪些操作保留全量重拉

以下操作因结构变化（新增/删除节点、重排序）保留全量重拉，这是正确的：

- 创建任务/目标 → `fetchGoalTree()` + `fetchProgresses()`
- 删除目标 → `fetchGoalTree()` + `fetchProgresses()`
- 自动拆解 / 重复拆解 / 重新规划 → `fetchGoalTree()` + `fetchProgresses()`
- 移动任务/目标 → `fetchGoalTree()` + `fetchProgresses()`

## 修复方案

### Step 1: 修复 CalendarView.vue 3处写操作

**1a. `batchSkip()`** — 参照已修复的 `batchComplete()` 模式：

```ts
async function batchSkip() {
  const ids = Array.from(selectedTaskIds.value);
  if (ids.length === 0) {
    message.warning("请先选择任务");
    return;
  }
  let ok = 0;
  const affectedGoalIds = new Set<string>();
  for (const id of ids) {
    try {
      // P2-3：保存返回值，局部更新任务
      const updated = await taskApi.skipTask(id);
      goalStore.updateTaskLocally(updated);
      affectedGoalIds.add(updated.goal_id);
      ok++;
    } catch (e) {
      message.error(`任务 ${id} 跳过失败: ${String(e)}`);
    }
  }
  if (ok > 0) {
    message.success(`已批量跳过 ${ok} 个任务`);
    // P2-3：局部刷新受影响目标的祖先链进度
    for (const gid of affectedGoalIds) {
      await goalStore.refreshProgressForGoalChain(gid);
    }
    await loadData();
    clearSelection();
  }
}
```

**1b. `quickComplete()`**：

```ts
async function quickComplete(task: CalendarTask) {
  try {
    // P2-3：保存返回值，局部更新任务
    const updated = await taskApi.completeTask({
      task_id: task.id,
      actual_qty: task.plan_qty,
    });
    goalStore.updateTaskLocally(updated);
    await goalStore.refreshProgressForGoalChain(updated.goal_id);
    await loadData();
    message.success("已完成");
  } catch (e) {
    message.error(String(e));
  }
}
```

**1c. `quickSkip()`**：

```ts
function quickSkip(task: CalendarTask) {
  dialog.warning({
    title: "跳过任务",
    content: `确定跳过任务"${task.name}"？`,
    positiveText: "跳过",
    negativeText: "取消",
    onPositiveClick: async () => {
      try {
        // P2-3：保存返回值，局部更新任务
        const updated = await taskApi.skipTask(task.id);
        goalStore.updateTaskLocally(updated);
        await goalStore.refreshProgressForGoalChain(updated.goal_id);
        await loadData();
        message.info("已跳过");
      } catch (e) {
        message.error(String(e));
      }
    },
  });
}
```

### Step 2: 修复 TaskItem.vue 3处写操作

TaskItem.vue 在 DashboardView 中使用，操作涉及两个 store：

- `taskStore`：管理今日/逾期任务列表 + 鼓励语逻辑
- `goalStore`：管理目标进度

**关键发现**：`taskStore.completeTask()` 内含鼓励语弹窗逻辑（首次完成触发鼓励），不能替换为 `taskApi.completeTask()` 直接调用！`taskStore.skipTask()` 不返回 Task 对象，需修改。

**修复策略**：

- `doComplete()`：保留 `taskStore.completeTask()`（保存鼓励语逻辑），用其返回值做局部进度更新
- `doBackfill()`：已直接调用 `taskApi.backfillTask()`，只需保存返回值
- `handleSkip()`：修改 `taskStore.skipTask()` 使其返回 `Task`，用返回值做局部更新

> 注意：TaskItem 不在 goalTree 视图中，`updateTaskLocally()` 在 goalTree 中查找并替换该任务 — 如果用户在 Dashboard 完成任务后切换到 GoalTree 视图，goalTree 中的状态已经是局部的最新值。如果 goalTree 中找不到该任务（极端情况），`updateTaskLocally()` 返回 false，不影响正确性。

**2a. `doComplete()`** — 保留 taskStore 调用，仅替换 fetchProgresses：

```ts
async function doComplete(qty: number) {
  try {
    // P2-3：taskStore.completeTask 保留鼓励语逻辑，用返回值做局部更新
    const updated = await taskStore.completeTask({
      task_id: props.task.id,
      actual_qty: qty,
    });
    goalStore.updateTaskLocally(updated);
    await goalStore.refreshProgressForGoalChain(updated.goal_id);
    message.success(randomEncouragement());
    emit("completed");
  } catch (e) {
    message.error(String(e));
  }
}
```

**2b. `doBackfill()`** — 已直接调用 taskApi，保存返回值：

```ts
async function doBackfill() {
  try {
    // P2-3：保存返回值，局部更新任务
    const updated = await taskApi.backfillTask({
      task_id: props.task.id,
      actual_qty: backfillQty.value,
    });
    goalStore.updateTaskLocally(updated);
    await goalStore.refreshProgressForGoalChain(updated.goal_id);
    message.success("已补完成");
    showBackfillModal.value = false;
    emit("completed");
  } catch (e) {
    message.error(String(e));
  }
}
```

**2c. `handleSkip()`** — 需先修改 taskStore.skipTask 返回 Task：
先修改 `src/stores/taskStore.ts`：

```ts
async function skipTask(taskId: string): Promise<Task> {
  const updated = await taskApi.skipTask(taskId);
  await fetchAll();
  return updated;
}
```

然后修改 TaskItem.vue：

```ts
function handleSkip() {
  dialog.warning({
    title: "跳过任务",
    content: `确定跳过任务"${props.task.name}"？`,
    positiveText: "跳过",
    negativeText: "取消",
    onPositiveClick: async () => {
      try {
        // P2-3：taskStore.skipTask 现在返回 Task，用返回值做局部更新
        const updated = await taskStore.skipTask(props.task.id);
        goalStore.updateTaskLocally(updated);
        await goalStore.refreshProgressForGoalChain(updated.goal_id);
        message.info("已跳过");
      } catch (e) {
        message.error(String(e));
      }
    },
  });
}
```

**2d. taskStore.ts 需添加 Task 类型 import**：

```ts
import type {
  TodayTask,
  CompleteTaskInput,
  Encouragement,
  GoalCompletionStat,
  Task, // 新增
} from "@/types";
```

### Step 3: 添加验证探针

在 `goalStore.ts` 中添加 `[P2-3-VERIFY]` console.log，用于在 DevTools Console 中区分局部更新和全量重拉：

```ts
async function fetchGoalTree() {
  console.log("[P2-3-VERIFY] fetchGoalTree — 全量重拉目标树");
  // ...existing code
}

async function fetchProgresses() {
  console.log("[P2-3-VERIFY] fetchProgresses — 全量重拉进度");
  // ...existing code
}

function updateTaskLocally(task: Task): boolean {
  console.log("[P2-3-VERIFY] updateTaskLocally — 局部更新任务", task.id);
  // ...existing code
}

function removeTaskLocally(taskId: string): string | null {
  console.log("[P2-3-VERIFY] removeTaskLocally — 局部移除任务", taskId);
  // ...existing code
}

function updateGoalLocally(goal: Goal): boolean {
  console.log("[P2-3-VERIFY] updateGoalLocally — 局部更新目标", goal.id);
  // ...existing code
}

async function refreshProgressForGoalChain(goalId: string): Promise<void> {
  console.log(
    "[P2-3-VERIFY] refreshProgressForGoalChain — 局部刷新祖先链进度",
    goalId,
  );
  // ...existing code
}
```

### Step 4: 构建验证

```bash
# 前端类型检查
npx vue-tsc --noEmit

# Rust 构建
cd src-tauri && cargo build
```

### Step 5: 运行时验证

启动应用后，打开 DevTools Console（Tauri 开发模式下 F12 可用），执行以下测试清单：

| 操作            | 预期 Console 日志                                   | 不应出现的日志                      |
| --------------- | --------------------------------------------------- | ----------------------------------- |
| 目标树-完成任务 | `updateTaskLocally` + `refreshProgressForGoalChain` | `fetchGoalTree` / `fetchProgresses` |
| 目标树-跳过任务 | `updateTaskLocally` + `refreshProgressForGoalChain` | `fetchGoalTree` / `fetchProgresses` |
| 目标树-补完成   | `updateTaskLocally` + `refreshProgressForGoalChain` | `fetchGoalTree` / `fetchProgresses` |
| 目标树-删除任务 | `removeTaskLocally` + `refreshProgressForGoalChain` | `fetchGoalTree` / `fetchProgresses` |
| 目标树-编辑任务 | `updateTaskLocally` + `refreshProgressForGoalChain` | `fetchGoalTree` / `fetchProgresses` |
| 目标树-编辑目标 | `updateGoalLocally` + `refreshProgressForGoalChain` | `fetchGoalTree` / `fetchProgresses` |
| 仪表盘-完成任务 | `updateTaskLocally` + `refreshProgressForGoalChain` | `fetchProgresses`                   |
| 仪表盘-跳过任务 | `updateTaskLocally` + `refreshProgressForGoalChain` | `fetchProgresses`                   |
| 仪表盘-补完成   | `updateTaskLocally` + `refreshProgressForGoalChain` | `fetchProgresses`                   |
| 日历-批量完成   | `updateTaskLocally` + `refreshProgressForGoalChain` | `fetchProgresses`                   |
| 日历-批量跳过   | `updateTaskLocally` + `refreshProgressForGoalChain` | `fetchProgresses`                   |
| 日历-快速完成   | `updateTaskLocally` + `refreshProgressForGoalChain` | `fetchProgresses`                   |
| 日历-快速跳过   | `updateTaskLocally` + `refreshProgressForGoalChain` | `fetchProgresses`                   |
| 创建任务/目标   | `fetchGoalTree` + `fetchProgresses`（全量，正确）   | —                                   |
| 删除目标        | `fetchGoalTree` + `fetchProgresses`（全量，正确）   | —                                   |
| 自动拆解        | `fetchGoalTree` + `fetchProgresses`（全量，正确）   | —                                   |

**UI 正确性验证**：

- 完成 → ✓ 图标变绿，进度百分比增加
- 跳过 → ⊘ 图标，进度不增
- 删除 → 任务从列表消失，进度更新
- 编辑 → 修改立即反映
- 嵌套目标：子目标完成后，父目标祖先进度都更新
- Dashboard 和 Calendar 视图中操作后，切换到 GoalTree 视图，数据一致

### Step 6: 状态漂移校验

在完成若干操作后，手动触发一次全量刷新（刷新按钮或切换页面），对比前后状态是否一致。如果一致，说明局部更新没有引入状态漂移。

### Step 7: 清理探针

验证完成后，删除 `goalStore.ts` 中所有 `[P2-3-VERIFY]` console.log 语句。

## 涉及文件

| 文件                          | 修改内容                                         |
| ----------------------------- | ------------------------------------------------ |
| `src/views/CalendarView.vue`  | 修复 batchSkip/quickComplete/quickSkip 3处写操作 |
| `src/components/TaskItem.vue` | 修复 doComplete/doBackfill/handleSkip 3处写操作  |
| `src/stores/taskStore.ts`     | 修改 skipTask 返回 Task 类型，添加 Task import   |
| `src/stores/goalStore.ts`     | 添加临时探针（Step 3），最终清理（Step 7）       |

## 风险与假设

1. **taskStore.skipTask() 返回值变更**：原来返回 `Promise<void>`，改为 `Promise<Task>`。需确认没有其他调用方依赖 void 返回类型（已确认：GoalTreeView 不使用 taskStore.skipTask，CalendarView 也不使用）。
2. **updateTaskLocally 在 Dashboard 场景**：Dashboard 中完成的任务，`updateTaskLocally` 会在 goalTree 中查找更新 — 如果 goalTree 尚未加载（首次进入 Dashboard），查找会返回 false，不影响正确性，下次进入 GoalTree 时会全量加载。
3. **loadData() 保留**：CalendarView 中 `loadData()` 仍需调用以刷新日历视图的任务数据（这是日历视图自身的数据源，与 goalStore 的全局状态无关）。
4. **taskStore.completeTask() 保留**：TaskItem 的 doComplete 继续使用 taskStore.completeTask() 而非 taskApi.completeTask()，因为 taskStore 内含鼓励语弹窗逻辑，替换会丢失该功能。
