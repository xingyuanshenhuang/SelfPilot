# CalendarView 视觉优化方案

## Context

当前 CalendarView.vue（1555 行）功能完整（P1/P2 全部特性已实现），但视觉层面存在以下问题：

1. **卡片层级缺失** — 所有 NCard 都是 `:bordered="false"`，工具栏/统计/筛选/内容卡看起来完全一样，无视觉层级
2. **月格拥挤** — `min-h-[88px] p-1.5` 太紧，任务点 1.5x1.5（6px）太小看不清
3. **今日高亮过重** — `border-2 + shadow-sm + brand-50` 视觉太抢眼
4. **周视图列单调** — 7 列完全一样，今日列不突出
5. **日视图扁平** — 任务行简单 list，无视觉节奏
6. **统计栏无层次** — 所有数字同等权重，完成率没作为主指标
7. **负载指示（P2-5）不显眼** — 1px 高色条易被忽略
8. **空状态简陋** — 周视图"无"字过小，日视图 NEmpty 无图标
9. **hover 不一致** — 月格 translateY、周格 transition-all、日行 bg-gray-50 各自为政

本方案通过**纯 CSS/类名改造**（不改 script 逻辑、不删 ARIA、不破坏 P2-5/P1-x 功能）让日历达到现代专业标准。

## 设计系统决策

### 颜色规则
- 主操作：`brand-500` #3478f6
- 今日轻提示：`brand-50/60` 底 + `brand-600` 文字 + 数字下小点
- 负载色：低=`green-50/40`，中=`amber-50/50`（替代 yellow-50），高=`red-50/50`
- 状态色：done=`green`，partial=`amber`，overdue=`red`，skipped=`gray`
- 中性表面：`bg-white` / `bg-gray-50/50`

### 间距 & 圆角
- 根容器 `space-y-4` 保留
- 月格：`min-h-[96px] p-2 rounded-lg`（原 88px/p-1.5/rounded）
- 周列：`min-h-[300px] p-2.5 rounded-lg`（原 280px/p-2/rounded）
- 日行：`pl-3 pr-2 py-2.5 rounded-md`（原 px-3 py-2/rounded）

### 字号
- 视图标题：`text-xl font-bold tracking-tight`（原 text-lg font-semibold）
- 统计大数字：`text-2xl font-bold tabular-nums`（原 text-xl）
- 统计标签：`text-[11px] uppercase tracking-wide text-gray-400`（原 text-xs text-gray-500）
- 周列头：拆为两行（周几 `text-[11px] uppercase` + 日期 `text-base font-semibold`）

## 分区块改造

### 1. 顶部工具栏（L711-772）
- NCard 加 `class="toolbar-card"`
- headerLabel 改 `text-xl font-bold tracking-tight text-gray-800`
- NRadioGroup 外包 `<div class="view-mode-segment">` 给"分段控件"感
- 新增 CSS：粘性吸顶 + 渐变背景 + backdrop-blur

### 2. 周期统计栏（L773-817）
- NCard 加 `class="stats-card"`
- 完成率单独提升为左侧"主指标块"，加右边框分隔
- 其余统计改为 `.stat-tile`（图标+数字+标签水平排布）
- 数字加 `tabular-nums` 防抖

### 3. 筛选栏（L819-943）
- NCard 加 `class="filter-card"`
- 灰底 `rgba(249,250,251,0.5)` 与上方 stats-card 形成视觉断点

### 4. 月视图（L945-1114）

**网格**：周名行加 `pb-2 mb-1 border-b border-gray-100`，字号 `text-[11px] uppercase tracking-wider text-gray-400`；gap-1 → gap-1.5

**Cell 容器**：
- `min-h-[96px] p-2 rounded-lg border border-gray-100 relative overflow-hidden`
- 加 `hover:shadow-sm hover:border-gray-200`

**今日高亮**：
- 去掉 `border-brand-400 border-2 shadow-sm`
- 改为 `bg-brand-50/60 border-brand-200`
- 数字下加 `<span class="w-1 h-1 rounded-full bg-brand-500">` 小点

**聚焦格**：去掉 `bg-blue-50`，仅保留 `ring-2 ring-brand-400/60 ring-offset-1`（避免与负载色冲突）

**负载指示**（P2-5 升级）：
- 原 `h-1` 色条改为顶部 4px 色条（`absolute top-0 left-0 right-0 h-1 rounded-t-lg`）
- 保留淡背景 tint（`/40` `/50` 透明度）
- yellow-50 → amber-50

**任务点**：
- `w-1.5 h-1.5` → `w-2 h-2 rounded-full ring-1 ring-white/80`（8px，更易看）

**统计 NTag** → 改为 span 小标签（`text-[10px] font-medium px-1.5 py-0.5 rounded-full`），避免 NTag 内边距过大

### 5. 周视图（L1116-1316）

**列头**：拆为两行
```
周几（text-[11px] uppercase tracking-wider，今日=brand-500，否则 gray-400）
日期（text-base font-semibold tabular-nums，今日=brand-600，否则 gray-700）
逾期 NTag（保留）
```

**今日列高亮**：
- 去掉 `border-brand-400 border-2`
- 改为 `.week-col-today` class + `::before` 顶部 3px 渐变条（brand-500 → brand-400）
- 加 `bg-brand-50/40 shadow-sm`

**列容器**：`min-h-[300px] p-2.5 rounded-lg border border-gray-100 bg-white hover:border-gray-200 hover:shadow-sm`

**任务行**：
- `text-xs p-1` → `text-xs px-2 py-1.5 rounded-md border transition-colors hover:bg-gray-50`
- 状态色：`bg-red-50/60 border-red-100`（逾期）、`bg-green-50/40 border-green-100`（完成，加 `!is_overdue` 防冲突）、`opacity-60`（跳过）

**空状态**：
- `<div>无</div>` → 图标 + 文案
```vue
<div class="flex flex-col items-center justify-center py-6 text-gray-300 gap-1">
  <Icon icon="mdi:calendar-blank-outline" width="20" class="opacity-40" />
  <span class="text-[10px]">无任务</span>
</div>
```

**列底进度**：
- NProgress 加 class `week-progress`，CSS 让 rail/fill 圆角化
- 百分比文字加 `tabular-nums font-medium`，100% 时 `text-green-600`

### 6. 日视图（L1318-1505）

**头部**：改为"日历撕页"风格
```vue
<div class="flex items-center gap-3">
  <div class="flex flex-col items-center justify-center w-12 h-12 rounded-lg bg-brand-50 border border-brand-100">
    <span class="text-[10px] text-brand-500 font-medium leading-none">{{ format(selectedDate, "MMM", { locale: zhCN }) }}</span>
    <span class="text-xl font-bold text-brand-700 leading-none mt-0.5 tabular-nums">{{ format(selectedDate, "d") }}</span>
  </div>
  <div class="flex flex-col">
    <span class="text-base font-semibold text-gray-800">{{ format(selectedDate, "yyyy 年 M 月 d 日", { locale: zhCN }) }}</span>
    <span class="text-xs text-gray-500">{{ format(selectedDate, "EEEE", { locale: zhCN }) }}</span>
  </div>
  <NTag v-if="isToday(selectedDate)" type="info" size="small" round class="ml-1">今天</NTag>
</div>
```

**任务行**：
- 改为带左侧状态色条的卡片
- `class="task-row-card relative flex items-center gap-2 pl-3 pr-2 py-2.5 rounded-md bg-white border border-gray-100 hover:border-gray-200 hover:shadow-sm transition-all"`
- 内部加 `<span class="status-bar" :style="{ backgroundColor: STATUS_META[t.status].color }" aria-hidden="true" />`
- 容器 `space-y-1` → `space-y-1.5`

**空状态**：
```vue
<div class="py-12">
  <NEmpty description="当日无任务">
    <template #icon><Icon icon="mdi:calendar-check-outline" width="48" class="text-gray-300" /></template>
    <template #extra><span class="text-xs text-gray-400">点击左侧日期或使用 ← → 切换</span></template>
  </NEmpty>
</div>
```

## 新增 `<style scoped>` CSS 规则

在现有 style 块末尾追加：

```css
/* 工具栏：粘性 + 渐变背景 */
.toolbar-card { position: sticky; top: 0; z-index: 20; }
.toolbar-card :deep(.n-card__content) {
  background: linear-gradient(180deg, rgba(238,246,255,0.6) 0%, rgba(255,255,255,0.85) 100%);
  backdrop-filter: saturate(180%) blur(8px);
  -webkit-backdrop-filter: saturate(180%) blur(8px);
  border-bottom: 1px solid rgba(52,120,246,0.08);
}

/* 统计卡片 */
.stats-card :deep(.n-card__content) { padding-top: 12px; padding-bottom: 12px; }
.stat-tile { display: flex; align-items: center; gap: 0.375rem; padding: 0 0.25rem; }

/* 筛选栏：灰底 */
.filter-card :deep(.n-card__content) {
  background-color: rgba(249,250,251,0.5);
  border-radius: 8px;
}

/* 月格：相对定位 + hover 阴影 */
.calendar-cell { position: relative; overflow: hidden; }
.calendar-cell:hover {
  transform: translateY(-1px);
  box-shadow: 0 2px 8px rgba(0,0,0,0.04);
  z-index: 1;
}
.calendar-cell:focus-visible { outline: 2px solid #3478f6; outline-offset: 2px; }

/* 周视图今日列：顶部渐变条 */
.week-col-today { position: relative; }
.week-col-today::before {
  content: ""; position: absolute; top: -1px; left: -1px; right: -1px; height: 3px;
  background: linear-gradient(90deg, #3478f6 0%, #599dff 100%);
  border-radius: 8px 8px 0 0; z-index: 1;
}

/* 日视图任务行：左侧色条 */
.task-row-card { position: relative; transition: border-color 0.2s, box-shadow 0.2s, transform 0.15s; }
.task-row-card:hover { transform: translateY(-0.5px); }
.task-row-card .status-bar {
  position: absolute; left: 4px; top: 8px; bottom: 8px; width: 3px; border-radius: 9999px;
}

/* 周视图进度条圆角 */
.week-progress :deep(.n-progress-graph-line-fill) { border-radius: 9999px; }
.week-progress :deep(.n-progress-graph-line-rail) { border-radius: 9999px; height: 6px !important; }

/* 视图模式分段控件 */
.view-mode-segment { padding: 2px; background-color: rgba(249,250,251,0.8); border-radius: 8px; }

/* 响应式：窄屏周视图水平滚动 */
@media (max-width: 1024px) {
  .week-grid { grid-template-columns: repeat(7, minmax(140px, 1fr)); overflow-x: auto; }
}
@media (max-width: 768px) {
  .calendar-cell { min-height: 72px !important; padding: 0.25rem !important; }
}
```

## 涉及文件

仅修改 1 个文件：`d:\桌面\SelfPilot\src\views\CalendarView.vue`
- template 部分各类名调整
- `<style scoped>` 部分新增 ~50 行 CSS

**不修改**：script 逻辑、ARIA 属性、naive-ui 组件、UnoCSS 配置、其他 view 文件

## 验证步骤

1. **构建验证**：
   - `npx vue-tsc --noEmit` — 类型检查通过
   - `npm run build`（或 `vite build`）— 构建成功

2. **运行时验证**（启动应用后）：
   - 日历月视图：cell 更通透，今日数字下有小蓝点，负载顶部色条清晰可见
   - 日历周视图：列头两行（周几+日期），今日列顶部有渐变条，任务行卡片化
   - 日历日视图：撕页式日期块，任务行左侧色条，空状态有图标
   - 顶部工具栏：粘性吸顶，渐变背景，视图切换分段控件感
   - 统计栏：完成率大数字突出，其他指标水平排布
   - 筛选栏：灰底与上方视觉断点明显
   - 悬浮 popover：负载概要 + 任务列表正常显示
   - P2-5 负载色阶：green/amber/red 三色清晰
   - P1-1 阻塞任务：锁图标和透明度正常
   - P1-2 批量操作：checkbox 和按钮正常
   - 键盘导航：方向键/Enter/Esc/Ctrl+A 全部正常

3. **响应式验证**：
   - 窗口缩放到 800px：周视图水平滚动而非压扁
   - 窗口缩放到 600px：月格缩小但可读

4. **暗色模式验证**：
   - 切换暗色主题，所有半透明色合理（如出现"灰糊"问题记录待修）

## 实施顺序

1. 根容器 + 顶部工具栏（sticky + 渐变）
2. 统计栏（主指标块 + stat-tile）
3. 筛选栏灰底
4. 月视图 cell 容器 + 今日高亮 + 聚焦格
5. 月视图负载指示条 + 任务点 + 统计标签
6. 周视图列头 + 今日列 + 列容器
7. 周视图任务行 + 空状态 + 进度
8. 日视图头部 + 任务行 + 空状态
9. 新增 `<style scoped>` CSS 规则
10. 构建验证

## 风险与约束

- **不改 script 逻辑**：仅改 template class 和 `<style scoped>`
- **保留 ARIA**：所有 `role`/`aria-label`/`aria-current` 不删，装饰元素加 `aria-hidden="true"`
- **保留 naive-ui 组件**：NCard/NButton/NTag/NPopover 等不改组件类型，只调 class
- **NCard bg 注入**：通过 `:deep(.n-card__content)` 而非直接给 NCard 加 bg class
- **sticky 风险**：NScrollbar 内可能失效，如失败回退为 `position: relative` + 始终渐变背景
- **dark 模式**：第一版优先 light，dark 如有问题单独记录
