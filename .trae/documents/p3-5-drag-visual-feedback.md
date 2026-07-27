# P3-5 拖拽排序视觉反馈增强

## Context
鼓励语库拖拽排序功能（P3-5）当前的视觉反馈过于微弱：放置指示线仅 2px border、无背景高亮、拖拽完成无确认动画、手柄 hover 无变色。用户无法清晰确认移动操作的目标位置和执行结果。需要增强视觉反馈系统，与项目中 GoalTree/TaskList 的拖拽惯例对齐。

## 修改文件
- `d:\桌面\SelfPilot\src\views\EncouragementView.vue` — 唯一修改文件

## 实施步骤

### 1. 新增状态变量
在 `savingOrder` ref 后新增：
```ts
const droppedItemId = ref<string | null>(null);
```

### 2. 改造 getDragClass 函数
从返回 UnoCSS 工具类改为返回语义化 CSS 类名：
- 拖拽源 → `enc-dragging`
- 悬停目标 → `enc-drag-over` + `enc-drop-before` / `enc-drop-after`
- 放置确认 → `enc-dropped`

### 3. 修改 handleDrop 函数
成功排序后触发确认闪烁：
```ts
droppedItemId.value = sourceId;
setTimeout(() => { droppedItemId.value = null; }, 800);
```

### 4. 修改 handleDragEnd 函数
清除 `droppedItemId` 防止残留。

### 5. 模板修改
- 行 class 绑定：`'border-blue-100 bg-blue-50/50'` → `enc-item-preset`，green 同理
- 拖拽手柄：加 `enc-drag-handle` 类 + `title="拖拽排序"`

### 6. 新增 `<style scoped>` 区域
| 效果 | CSS 类 | 关键属性 |
|------|--------|---------|
| 插入指示线 | `.enc-drop-before/after` | `box-shadow: 0 ±3px 0 0 #599dff` (brand-400) |
| 悬停背景高亮 | `.enc-drag-over` | `background: rgba(238,246,255,0.7)` + `border-color: #bcd9ff` |
| 拖拽源 | `.enc-dragging` | `opacity: 0.35` + `scale(0.97)` |
| 放置确认 | `.enc-dropped` | `@keyframes enc-drop-confirm` 0.8s，brand-100 高亮 + brand-400 光圈 |
| 手柄 hover | `.enc-drag-handle:hover` | `color: #599dff` + `scale(1.2)` |

## 验证
- `npx vue-tsc --noEmit` 类型检查通过
- `npm run build` 构建通过
