# P2-1 虚拟滚动优化 — 任务列表虚拟化

## Context

**问题**：[GoalTreeNodeItem.vue](file://d:\桌面\SelfPilot\src\components\GoalTreeNodeItem.vue) 当前对每个目标的直属任务用 `v-for` 全量渲染。一个目标经过"自动拆解"可生成 100+ 每日任务，多个目标全展开后 DOM 节点数爆炸，滚动和拖拽出现明显卡顿。

**目标**：1000 任务节点下连续滚动保持 ≥55fps（验收标准 60fps），同时不破坏现有拖拽体验（任务跨目标移动 + 同级排序 + 目标 3-zone 拖拽）。

**方案选定**：**仅对任务列表虚拟化**（保留目标树递归结构不变）。理由：目标节点由用户手动创建，通常几十个；真正爆炸的是每日任务列表。此方案对拖拽逻辑影响最小、风险最低、精准命中实际瓶颈。

**关键技术验证**（已读 vueuc 源码确认）：

* naive-ui `<NVirtualList>` 由 `NaiveUiResolver` 自动导入，可直接使用

* slot 签名 `{ item, index }`，`index` 是**原始数组索引**（非可视索引）—— `onTaskRowDrop(e, task, idx)` 无需改造

* `item-size` 必填且为 Number（固定行高），`key-field` 改为 `"id"`

* 容器靠 `max-height: inherit` 继承父级高度，父级必须显式 `max-height`

* slot 模板必须是单根 `<div>`

* `tauri.conf.json` 已设 `dragDropEnabled: false`，HTML5 drag/drop 正常

***

## 关键设计决策

### 1. 阈值切换

* 阈值 **50**（任务数 > 50 启用虚拟列表，≤ 50 走原 v-for）

* 用 `computed` + `v-if/v-else` 切换，不用动态组件

* 两种模式共用 `rowClass(task)` 函数和行内 DOM 结构，保证视觉一致

### 2. NVirtualList 配置

| 配置                | 取值                               |
| ----------------- | -------------------------------- |
| `:items`          | `tasks`（Task 对象本身即可）             |
| `:item-size`      | `36`                             |
| `key-field`       | `"id"`                           |
| `:item-resizable` | `false`                          |
| 容器                | 外包 `<div class="max-h-[400px]">` |

**行高对齐**：两种模式都给任务行加 `h-9`（36px），保证 `item-size=36` 与真实行高 1:1，避免 FinweckTree 漂移。当前行高约 34px，加 `h-9` 视觉无感知差异。

### 3. 拖拽逻辑零改造（关键结论）

NVirtualList 渲染的是真实 DOM，slot 模板上的 `draggable="true"` + `@dragstart/@dragover/@drop` 行为与 v-for 完全一致：

* **同级排序**：`onTaskRowDrop(e, task, index)` 的 `index` 是原始数组索引，`tasks[index + 1]` 取下一个任务仍然正确

* **拖到列表空白区**：drop 不在任何 task-row 上，冒泡到外层 `onCardDrop`，进入 inside 分支放最前（与原行为一致）

* **跨目标拖拽**：拖到对方 task-row 或目标头部，逻辑路径全部覆盖

* **视觉反馈类**（`task-drop-before/after`、`dragging-opacity`）：作用在真实 DOM 上，正常显示

### 4. 组件结构：抽出 `TaskList.vue` 子组件

**理由**：GoalTreeNodeItem.vue 已 686 行；行模板在两种模式下必须 100% 一致，子组件可单一定义；拖拽状态已在 `GoalTreeApi` provide，子组件 inject 即可。

### 5. 耦合点修正（关键点）

[onCardDrop](file://d:\桌面\SelfPilot\src\components\GoalTreeNodeItem.vue#L297-L350) 保留在父组件，但其内部调用了 `findTaskInTree(taskId)` 和 `clearTaskDrag()`（这两个原本计划迁移到 TaskList）。如果迁移，父组件会断链。

**解决方案**：把这两个函数**提升到** **`GoalTreeApi`** **provide 层**（在 [GoalTreeView.vue](file://d:\桌面\SelfPilot\src\views\GoalTreeView.vue) 中实现）：

* `api.findTaskInTree(taskId)` — 依赖完整 `goalStore.goalTree`，放在拥有完整树的 view 层最自然

* `api.clearTaskDrag()` — 仅清 3 个 ref，提升后父组件和 TaskList 共用一份

这样 GoalTreeNodeItem.vue 中任务拖拽相关代码可彻底删除，TaskList.vue 通过 inject 调用 `api.findTaskInTree` / `api.clearTaskDrag`。

***

## 文件改动清单

### 1. 新增：`src/components/TaskList.vue`

**职责**：渲染目标直属任务列表，按阈值切换 v-for / NVirtualList

* **Props**：`tasks: Task[]`、`goalId: string`

* **Slot**：`label`（"直属任务"标签）

* **Inject**：`goalTreeApi`

* **迁移入**（从 GoalTreeNodeItem.vue）：

  * `onTaskDragStart` / `onTaskDragEnd` / `onTaskRowDragOver` / `onTaskRowDragLeave` / `onTaskRowDrop`

  * `isTaskDropBefore` / `isTaskDropAfter`

* **调用 api**：`api.draggingTaskId` / `api.dragOverTaskId` / `api.taskDropPosition` / `api.findTaskInTree` / `api.clearTaskDrag` / `api.handleMoveTask` / `api.handleTaskAction` / `api.buildTaskActions`

* **新增**：`useVirtual = computed(() => props.tasks.length > 50)`、`rowClass(task)` 函数、NVirtualList 配置

* **行模板**：两种模式各一份相同的 DOM（Icon + 名称 + plan\_date + NTag + qty + NDropdown），用 `rowClass` 统一类名

* **CSS**：迁移 `.task-drop-before` / `.task-drop-after` / `.dragging-opacity` / `[draggable="true"]` 样式到此组件 scoped style

### 2. 修改：`src/components/GoalTreeNodeItem.vue`

* **删除**任务拖拽相关代码：

  * 行 114–120（共享拖拽状态局部别名 — 任务相关部分）

  * 行 136–210（`onTaskDragStart/End/RowDragOver/RowDragLeave/RowDrop` + `clearTaskDrag`）

  * 行 387–401（`isTaskDropBefore/isTaskDropAfter`）

  * 行 407–429（`findTaskInTree/findTaskInNode` — 提升到 api）

* **保留**：

  * `onCardDragOver/Leave/Drop`（目标卡片放置区，处理任务 inside + 目标 3-zone）

  * `onGoalDragStart/End`、`clearGoalDrag`、目标拖拽视觉反馈 computed

  * `onCardDrop` 内改为调用 `api.findTaskInTree` 和 `api.clearTaskDrag`

* **修改 template 第 617–674 行**：把"直属任务"块替换为：

  ```vue
  <TaskList v-if="node.tasks.length > 0" :tasks="node.tasks" :goal-id="node.goal.id">
    <template #label>
      <div v-if="hasChildren" class="text-xs text-gray-400 px-3 py-1">直属任务</div>
    </template>
  </TaskList>
  ```

* **修改** **`<style scoped>`**：删除任务行相关样式（`.task-drop-before/after`、`.dragging-opacity`、`[draggable="true"]`），保留目标相关样式（`.drop-before-goal/after-goal/inside-goal/inside-task`）

### 3. 修改：`src/views/GoalTreeView.vue`

* **扩展** **`GoalTreeApi`** **接口**（接口定义在 GoalTreeNodeItem.vue 的 `<script lang="ts">` 块）：新增 `findTaskInTree: (taskId: string) => Task | null` 和 `clearTaskDrag: () => void`

* **在 provide 对象中实现并暴露**这两个函数：

  * `findTaskInTree` 遍历 `goalStore.goalTree` 递归查找

  * `clearTaskDrag` 清空 `draggingTaskId/dragOverTaskId/dragOverGoalId` 三个 ref

* 其余不动（`expandedNodes`、`handleMoveTask/Goal`、`draggingTaskId` 等 ref 全部保留）

### 4. 自动生成：`components.d.ts`

* `unplugin-vue-components` 自动检测 TaskList.vue 并添加类型声明，无需手动改

***

## 实现步骤

1. **扩展 GoalTreeApi 接口**：在 GoalTreeNodeItem.vue 的 `<script lang="ts">` 块新增 `findTaskInTree` 和 `clearTaskDrag` 签名
2. **在 GoalTreeView\.vue 实现**：把现有 `findTaskInTree` 逻辑（递归 goalStore.goalTree）和 `clearTaskDrag` 加到 provide 对象
3. **新建 TaskList.vue**：

   * inject api、定义 props、迁移任务拖拽函数（`props.node.goal.id` → `props.goalId`，`props.node.tasks` → `props.tasks`）

   * 实现行模板（两份相同 DOM，分别用于 v-for 和 NVirtualList slot）

   * 实现 `rowClass`、`useVirtual`、NVirtualList 配置

   * 迁移任务拖拽 CSS 到 scoped style
4. **改造 GoalTreeNodeItem.vue**：

   * 删除任务拖拽相关函数和 computed

   * `onCardDrop` 内 `findTaskInTree`/`clearTaskDrag` 改为 `api.findTaskInTree`/`api.clearTaskDrag`

   * template 任务列表块替换为 `<TaskList>`

   * 删除任务相关 CSS
5. **类型检查 + 构建**：`npm run build`（vue-tsc + vite build）通过
6. **手动回归**：见验证步骤阶段 1

***

## 风险与回滚

| 风险                                | 缓解                                                                     |
| --------------------------------- | ---------------------------------------------------------------------- |
| 行高 `h-9=36px` 与实际不符导致滚动错位         | 兜底改 `:item-resizable="true"`，性能略降但正确性保证                                |
| `max-height: inherit` 在 flex 容器失效 | 外层用普通 `<div style="max-height: 400px; overflow: hidden">` 强制约束，不用 flex |
| 拖到虚拟列表边缘不自动滚动                     | 本期不做（任务排序通常是同屏操作），文档说明                                                 |
| 行模板复制两份后修改不同步                     | 用 `rowClass` 函数统一类名；行内 DOM 复制后加注释提醒同步                                  |
| 模式切换（49↔51）DOM 重建抖动               | 行模板一致，视觉无差异；仅在数据变化时切换，可接受                                              |

**回滚**：单 commit 完成，`git revert` 即可恢复。TaskList.vue 是新增文件，删除即可；GoalTreeNodeItem.vue 改动是删除 + 局部替换，revert 后自动恢复。不涉及数据迁移、后端、类型定义变更。

***

## 验证步骤

### 阶段 1：功能回归（开发期）

1. **小列表体验**：创建 30 个任务的目标，验证：

   * 拖拽排序（before/after 视觉反馈正确）

   * 跨目标移动（拖到目标头部 → 移入对方首位；拖到对方 task-row → 排序）

   * NDropdown 操作菜单弹出位置正确

   * 折叠/展开后任务列表正常
2. **模式切换**：给目标逐步加任务到 51，验证 v-for → NVirtualList 无视觉跳变；删到 49 切回 v-for

### 阶段 2：虚拟模式拖拽

1. 造 100 个任务，验证：

   * 同屏内拖拽排序正常

   * 拖到列表底部空白区，任务移到首位（`onCardDrop` inside 行为）

   * 拖到另一个目标头部，移入对方首位

   * 拖到另一个目标虚拟列表内 task-row，排序正确（**重点验证** **`index`** **参数**）

   * 跨虚拟列表拖拽（父目标虚拟列表 → 子目标虚拟列表）

### 阶段 3：性能验收

1. **造 1000 任务测试数据**：通过 Tauri SQL 直接插入，或临时在 goalStore 加 `__mock1000Tasks` 函数（提交前删除）
2. **Chrome DevTools Performance tab**：录制滚动 5 秒，FPS meter ≥ 55，主线程长任务（>50ms）< 5 个
3. **Vue DevTools Components 面板**：确认 NVirtualList 的 `viewportItems` 长度 ≈ 11–13（不随任务总数增长）
4. **DOM 节点数**：Elements 面板搜索 `.task-row`，应稳定在 \~15 个（不随任务总数增长）

### 阶段 4：边界

1. 空任务列表：显示"暂无子目标和任务"
2. 嵌套场景：父目标 200 任务 + 子目标 200 任务，两个独立虚拟列表分别滚动
3. 重复任务批量生成：用"自动拆解"生成 100 任务，列表从 v-for 切换到 NVirtualList 无异常

### 阶段 5：构建

1. `npm run build` 通过（vue-tsc 类型检查 + vite build）
2. `cargo check`（src-tauri）不涉及，确认无后端改动

