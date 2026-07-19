# CalendarView 视觉优化 — 收尾计划

## Context

原始方案 [calendar-visual-overhaul.md](file:///d:/桌面/SelfPilot/.trae/documents/calendar-visual-overhaul.md) 已被用户批准并执行到约 50%。本计划只覆盖**未完成**的部分，沿用原方案的设计系统决策（颜色/间距/字号规则），不引入新决策。

## 当前实施状态核查（基于 [CalendarView.vue](file:///d:/桌面/SelfPilot/src/views/CalendarView.vue) 1723 行实际代码）

### ✅ 已完成（与原方案一致，不再改动）

1. **顶部工具栏（L743-786）**
   - `class="toolbar-card"` 已加
   - headerLabel：`text-xl font-bold tracking-tight ... text-gray-800`
   - NRadioGroup 外包 `<div class="view-mode-segment">`

2. **周期统计栏（L789-841）**
   - `class="stats-card"` 已加
   - 主指标块：`text-2xl font-bold tabular-nums text-brand-600` + `pr-4 border-r border-gray-100`
   - 副指标：`.stat-tile` 水平排布

3. **筛选栏（L844-967）**
   - `class="filter-card"` 已加

4. **月视图（L970-1149）**
   - 周名行：`text-[11px] font-medium uppercase tracking-wider text-gray-400 pb-2 border-b border-gray-100`
   - 网格 gap：`gap-1.5`
   - Cell：`calendar-cell min-h-[96px] p-2 rounded-lg border border-gray-100 hover:shadow-sm hover:border-gray-200`
   - 今日：`bg-brand-50/60 border-brand-200` + 数字下小蓝点
   - 聚焦：`ring-2 ring-brand-400/60 ring-offset-1`（已去掉 `bg-blue-50`）
   - 负载色阶：`bg-green-50/40` / `bg-amber-50/50` / `bg-red-50/50`
   - 负载指示条：`absolute top-0 left-0 right-0 h-1 rounded-t-lg` + `bg-green-400/amber-400/red-400`
   - 统计标签：自定义 span（`text-[10px] font-medium ... rounded-full`）
   - 任务点：`w-2 h-2 rounded-full ring-1 ring-white/80`
   - "+N"：`text-[10px] text-gray-500 font-medium`

### ❌ 待完成（本计划范围）

5. **周视图（L1151-1351）** — 完全未改
6. **日视图（L1353-1543）** — 完全未改
7. **`<style scoped>`（L1564-1722）** — 缺少原方案要求的 ~50 行 CSS
8. **构建验证** — 未执行

## 改造细节

### 步骤 1：周视图列容器 + 今日列 + 选中列（L1207-1218）

**当前**：
```vue
<div
  v-for="day in weekGrid"
  :key="day.toISOString()"
  class="min-h-[280px] p-2 rounded border flex flex-col transition-all duration-200"
  role="gridcell"
  :aria-label="getDayAriaLabel(day)"
  :class="{
    'border-brand-400 border-2 bg-brand-50/50 shadow-sm': isToday(day),
    'bg-blue-50': isSameDay(day, selectedDate) && !isToday(day),
  }"
>
```

**改为**：
```vue
<div
  v-for="day in weekGrid"
  :key="day.toISOString()"
  class="week-col min-h-[300px] p-2.5 rounded-lg border border-gray-100 bg-white flex flex-col transition-all duration-200 hover:border-gray-200 hover:shadow-sm"
  role="gridcell"
  :aria-label="getDayAriaLabel(day)"
  :class="{
    'week-col-today bg-brand-50/40 shadow-sm': isToday(day),
    'bg-brand-50/20': isSameDay(day, selectedDate) && !isToday(day),
  }"
>
```

要点：
- `min-h-[280px] p-2 rounded border` → `min-h-[300px] p-2.5 rounded-lg border border-gray-100 bg-white`
- 加 `hover:border-gray-200 hover:shadow-sm`
- 今日列：去掉 `border-brand-400 border-2 bg-brand-50/50 shadow-sm`，改为 `.week-col-today bg-brand-50/40 shadow-sm`（顶部渐变条由 CSS `::before` 实现）
- 选中非今日：`bg-blue-50` → `bg-brand-50/20`（与品牌色统一）

### 步骤 2：周视图列头拆为两行（L1219-1233）

**当前**：
```vue
<div
  class="flex items-center justify-center gap-1.5 text-center text-sm font-medium pb-1.5 border-b"
  :class="{ 'text-brand-600 font-bold': isToday(day) }"
>
  <span>{{ format(day, "E d", { locale: zhCN }) }}</span>
  <NTag
    v-if="getDayStatsCached(day).overdue > 0"
    size="tiny"
    type="error"
    :bordered="false"
    round
    >{{ getDayStatsCached(day).overdue }}逾期</NTag
  >
</div>
```

**改为**：
```vue
<div
  class="flex flex-col items-center gap-0.5 text-center pb-2 border-b border-gray-100"
  :class="{ 'border-brand-100': isToday(day) }"
>
  <span
    class="text-[11px] uppercase tracking-wider leading-none"
    :class="isToday(day) ? 'text-brand-500' : 'text-gray-400'"
    >{{ format(day, "EEEEEE", { locale: zhCN }) }}</span
  >
  <span
    class="text-base font-semibold tabular-nums leading-none mt-0.5"
    :class="isToday(day) ? 'text-brand-600' : 'text-gray-700'"
    >{{ format(day, "d") }}</span
  >
  <NTag
    v-if="getDayStatsCached(day).overdue > 0"
    size="tiny"
    type="error"
    :bordered="false"
    round
    class="mt-1"
    >{{ getDayStatsCached(day).overdue }}逾期</NTag
  >
</div>
```

要点：
- 单行 `text-sm font-medium` → 两行（周几 + 日期）
- 周几：`text-[11px] uppercase tracking-wider`，今日 `brand-500`，否则 `gray-400`
- 日期：`text-base font-semibold tabular-nums`，今日 `brand-600`，否则 `gray-700`
- 使用 `format(day, "EEEEEE", { locale: zhCN })` 得到本地化短周几（"一"/"二"/.../"日"），与月视图 weekDays 数组一致
- 逾期 NTag 保留，加 `mt-1` 间距

### 步骤 3：周视图任务行卡片化 + 状态色（L1248-1281）

**当前**：
```vue
<div
  class="text-xs p-1 rounded flex items-center gap-1 cursor-default"
  role="listitem"
  :aria-label="getTaskAriaLabel(t)"
  :class="{
    'bg-red-50': t.is_overdue,
    'bg-green-50': t.status === 'done',
    'opacity-60': t.status === 'skipped',
  }"
>
```

**改为**：
```vue
<div
  class="text-xs px-2 py-1.5 rounded-md border flex items-center gap-1.5 cursor-default transition-colors"
  role="listitem"
  :aria-label="getTaskAriaLabel(t)"
  :class="{
    'bg-red-50/60 border-red-100': t.is_overdue,
    'bg-green-50/40 border-green-100': t.status === 'done' && !t.is_overdue,
    'bg-white border-gray-100 hover:bg-gray-50':
      t.status !== 'done' && t.status !== 'skipped' && !t.is_overdue,
    'opacity-60 bg-gray-50/40 border-gray-100': t.status === 'skipped',
  }"
>
```

要点：
- `p-1 rounded` → `px-2 py-1.5 rounded-md border`
- 加 `transition-colors`
- 状态色：逾期 `bg-red-50/60 border-red-100`、完成（且非逾期）`bg-green-50/40 border-green-100`、跳过 `opacity-60 bg-gray-50/40 border-gray-100`、其他 `bg-white border-gray-100 hover:bg-gray-50`
- gap-1 → gap-1.5（更舒展）

### 步骤 4：周视图空状态图标化（L1319-1324）

**当前**：
```vue
<div
  v-if="getTasksOfDay(day).length === 0"
  class="flex items-center justify-center text-[10px] text-gray-300 pt-2"
>
  无
</div>
```

**改为**：
```vue
<div
  v-if="getTasksOfDay(day).length === 0"
  class="flex flex-col items-center justify-center py-6 text-gray-300 gap-1 flex-1"
  aria-label="无任务"
>
  <Icon icon="mdi:calendar-blank-outline" width="20" class="opacity-40" aria-hidden="true" />
  <span class="text-[10px]">无任务</span>
</div>
```

要点：
- 单字"无" → 图标 + 文案
- 加 `flex-1` 让空状态在列内居中
- 保留 `aria-label="无任务"` 供屏幕阅读器

### 步骤 5：周视图列底进度条圆角化（L1327-1347）

**当前**：
```vue
<NProgress
  type="line"
  :percentage="getDayCompletionRate(day)"
  :show-indicator="false"
  size="small"
  :color="
    getDayCompletionRate(day) === 100 ? '#67c23a' : '#3478f6'
  "
  aria-hidden="true"
/>
```

**改为**：在 NProgress 上加 `class="week-progress"`，并把百分比文字改为 `tabular-nums font-medium`，100% 时 `text-green-600`：

```vue
<div
  v-if="getDayStatsCached(day).total > 0"
  class="mt-1 pt-1.5 border-t border-gray-100"
>
  <div
    class="flex items-center justify-between text-[10px] mb-0.5"
  >
    <span class="text-gray-400">完成率</span>
    <span
      class="tabular-nums font-medium"
      :class="
        getDayCompletionRate(day) === 100 ? 'text-green-600' : 'text-gray-500'
      "
      >{{ getDayCompletionRate(day) }}%</span
    >
  </div>
  <NProgress
    type="line"
    class="week-progress"
    :percentage="getDayCompletionRate(day)"
    :show-indicator="false"
    size="small"
    :color="
      getDayCompletionRate(day) === 100 ? '#67c23a' : '#3478f6'
    "
    aria-hidden="true"
  />
</div>
```

### 步骤 6：日视图头部撕页式（L1362-1378）

**当前**：
```vue
<NCard :bordered="false">
  <template #header>
    <div class="flex items-center gap-2">
      <Icon
        icon="mdi:calendar-today"
        width="20"
        class="text-brand-500"
        aria-hidden="true"
      />
      <span>{{
        format(selectedDate, "yyyy-MM-dd EEEE", { locale: zhCN })
      }}</span>
      <NTag v-if="isToday(selectedDate)" type="info" size="small" round
        >今天</NTag
      >
    </div>
  </template>
```

**改为**：
```vue
<NCard :bordered="false">
  <template #header>
    <div class="flex items-center gap-3">
      <div
        class="flex flex-col items-center justify-center w-12 h-12 rounded-lg bg-brand-50 border border-brand-100"
        aria-hidden="true"
      >
        <span class="text-[10px] text-brand-500 font-medium leading-none">{{
          format(selectedDate, "MMM", { locale: zhCN })
        }}</span>
        <span
          class="text-xl font-bold text-brand-700 leading-none mt-0.5 tabular-nums"
          >{{ format(selectedDate, "d") }}</span
        >
      </div>
      <div class="flex flex-col">
        <span class="text-base font-semibold text-gray-800">{{
          format(selectedDate, "yyyy 年 M 月 d 日", { locale: zhCN })
        }}</span>
        <span class="text-xs text-gray-500">{{
          format(selectedDate, "EEEE", { locale: zhCN })
        }}</span>
      </div>
      <NTag
        v-if="isToday(selectedDate)"
        type="info"
        size="small"
        round
        class="ml-1"
        >今天</NTag
      >
    </div>
  </template>
```

要点：
- 用日历撕页块（月缩写 + 日数字）替代 `mdi:calendar-today` 图标
- 撕页块：`w-12 h-12 rounded-lg bg-brand-50 border border-brand-100`
- 主标题：`text-base font-semibold text-gray-800`
- 副标题（周几）：`text-xs text-gray-500`
- 保留"今天" NTag

### 步骤 7：日视图任务行卡片化 + 左侧色条（L1418-1427）

**当前**：
```vue
<div
  v-for="t in selectedDayTasks"
  :key="t.id"
  class="flex items-center gap-2 px-3 py-2 rounded hover:bg-gray-50"
  role="listitem"
  :aria-label="getTaskAriaLabel(t)"
  :class="{
    'bg-red-50': t.is_overdue,
  }"
>
```

**改为**：
```vue
<div
  v-for="t in selectedDayTasks"
  :key="t.id"
  class="task-row-card relative flex items-center gap-2 pl-3 pr-2 py-2.5 rounded-md bg-white border border-gray-100 hover:border-gray-200 hover:shadow-sm transition-all"
  role="listitem"
  :aria-label="getTaskAriaLabel(t)"
>
  <span
    class="status-bar"
    :style="{ backgroundColor: STATUS_META[t.status].color }"
    aria-hidden="true"
  />
```

要点：
- 加 `task-row-card` class（CSS 控制左侧色条定位 + hover 微移）
- 加 `relative` 让 `.status-bar` 绝对定位生效
- 改为白底卡片：`bg-white border border-gray-100 hover:border-gray-200 hover:shadow-sm`
- 在容器内首位插入 `<span class="status-bar">` 显示状态色
- 容器列表间距：`space-y-1` → `space-y-1.5`（在 L1414 修改）
- 移除原 `bg-red-50` 逾期整行染色，改由左侧色条 + 任务名后 NTag 表达逾期（保留 L1478-1484 的"逾期"NTag）
- 跳过/完成的视觉表达通过 `.status-bar` 颜色 + 现有 line-through 实现

### 步骤 8：日视图空状态增强（L1540）

**当前**：
```vue
<NEmpty v-else description="当日无任务" />
```

**改为**：
```vue
<div v-else class="py-12">
  <NEmpty description="当日无任务">
    <template #icon>
      <Icon
        icon="mdi:calendar-check-outline"
        width="48"
        class="text-gray-300"
      />
    </template>
    <template #extra>
      <span class="text-xs text-gray-400"
        >点击左侧日期或使用 ← → 切换</span
      >
    </template>
  </NEmpty>
</div>
```

### 步骤 9：新增 `<style scoped>` CSS 规则

在现有 `<style scoped>` 块末尾（L1722 `}` 之后、`</style>` 之前）追加：

```css
/* ===== 视觉优化（沿用 calendar-visual-overhaul.md 设计系统） ===== */

/* 工具栏：粘性吸顶 + 渐变背景 + 毛玻璃 */
.toolbar-card {
  position: sticky;
  top: 0;
  z-index: 20;
}
.toolbar-card :deep(.n-card__content) {
  background: linear-gradient(
    180deg,
    rgba(238, 246, 255, 0.6) 0%,
    rgba(255, 255, 255, 0.85) 100%
  );
  backdrop-filter: saturate(180%) blur(8px);
  -webkit-backdrop-filter: saturate(180%) blur(8px);
  border-bottom: 1px solid rgba(52, 120, 246, 0.08);
}

/* 统计卡片：紧凑内边距 + stat-tile 水平排布 */
.stats-card :deep(.n-card__content) {
  padding-top: 12px;
  padding-bottom: 12px;
}
.stat-tile {
  display: flex;
  align-items: center;
  gap: 0.375rem;
  padding: 0 0.25rem;
}

/* 筛选栏：灰底形成视觉断点（与 stats-card 区分） */
.filter-card :deep(.n-card__content) {
  background-color: rgba(249, 250, 251, 0.5);
  border-radius: 8px;
}

/* 视图模式分段控件背景 */
.view-mode-segment {
  padding: 2px;
  background-color: rgba(249, 250, 251, 0.8);
  border-radius: 8px;
}

/* 月格：hover 微浮起（focus-visible 已在原样式中） */
.calendar-cell {
  position: relative;
  overflow: hidden;
}
.calendar-cell:hover {
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.04);
  z-index: 1;
}

/* 周视图今日列：顶部 3px 渐变条 */
.week-col-today {
  position: relative;
  border-color: rgba(52, 120, 246, 0.2) !important;
}
.week-col-today::before {
  content: "";
  position: absolute;
  top: -1px;
  left: -1px;
  right: -1px;
  height: 3px;
  background: linear-gradient(90deg, #3478f6 0%, #599dff 100%);
  border-radius: 8px 8px 0 0;
  z-index: 1;
}

/* 周视图进度条：rail/fill 圆角化 */
.week-progress :deep(.n-progress-graph-line-rail) {
  border-radius: 9999px;
  height: 6px !important;
}
.week-progress :deep(.n-progress-graph-line-fill) {
  border-radius: 9999px;
}

/* 日视图任务行：左侧状态色条 + hover 微浮起 */
.task-row-card {
  position: relative;
  transition:
    border-color 0.2s,
    box-shadow 0.2s,
    transform 0.15s;
}
.task-row-card:hover {
  transform: translateY(-0.5px);
}
.task-row-card .status-bar {
  position: absolute;
  left: 4px;
  top: 8px;
  bottom: 8px;
  width: 3px;
  border-radius: 9999px;
}

/* 响应式：窄屏周视图水平滚动而非压扁 */
@media (max-width: 1024px) {
  .week-grid {
    grid-template-columns: repeat(7, minmax(140px, 1fr));
    overflow-x: auto;
  }
}
@media (max-width: 768px) {
  .calendar-cell {
    min-height: 72px !important;
    padding: 0.25rem !important;
  }
}
```

要点：
- `.toolbar-card` 粘性 + 渐变 + backdrop-blur（sticky 在 NScrollbar 内可能失效，先按方案尝试；如运行时验证失效再回退 `position: relative`）
- `.stats-card :deep(.n-card__content)` 12px 上下内边距
- `.stat-tile` 水平 flex 布局（template 已使用但 CSS 缺失）
- `.filter-card :deep(.n-card__content)` 灰底 + 圆角（与现有 padding-bottom 规则合并：选择器相同，CSS 自然叠加）
- `.view-mode-segment` 分段控件背景
- `.calendar-cell` 补充 `position: relative; overflow: hidden`（hover 阴影原样式中已有）
- `.week-col-today` + `::before` 顶部渐变条
- `.week-progress` rail/fill 圆角
- `.task-row-card` + `.status-bar` 左侧色条
- 响应式：1024px 周视图水平滚动、768px 月格缩小

### 步骤 10：构建验证

1. **类型检查**：`npx vue-tsc --noEmit`（在 [package.json](file:///d:/桌面/SelfPilot/package.json) 目录下执行）
2. **构建**：`npm run build`
3. 两者均需通过，无新 warning/error

## 涉及文件

仅修改 1 个文件：[CalendarView.vue](file:///d:/桌面/SelfPilot/src/views/CalendarView.vue)
- template：周视图（L1207-1347）、日视图（L1362-1540）类名/结构调整
- `<style scoped>`：末尾追加 ~85 行 CSS

**不修改**：script 逻辑、ARIA 属性、naive-ui 组件类型、UnoCSS 配置、其他 view 文件、原已完成的月视图/工具栏/统计栏/筛选栏 template

## 风险与约束

- **不改 script 逻辑**：仅改 template class、新增 `<style scoped>`
- **保留 ARIA**：所有 `role`/`aria-label`/`aria-current`/`aria-live` 不删，装饰元素加 `aria-hidden="true"`
- **保留 naive-ui 组件**：NCard/NButton/NTag/NPopover/NProgress/NEmpty 等不改组件类型，只调 class
- **NCard bg 注入**：通过 `:deep(.n-card__content)` 而非直接给 NCard 加 bg class
- **sticky 风险**：[App.vue](file:///d:/桌面/SelfPilot/src/App.vue) 的 NScrollbar 内 sticky 可能失效；如运行时验证失效，回退 `.toolbar-card { position: relative; }` 并保留渐变背景
- **暗色模式**：第一版优先 light；dark 如出现"灰糊"问题单独记录，不在本次范围
- **linter 干扰**：Edit 前先 Read 当前状态，old_string 包含 linter 可能注入的属性（如 `data-month-cell`）

## 实施顺序（建议使用 TodoWrite 跟踪）

1. 周视图列容器 + 今日列 + 选中列（步骤 1）
2. 周视图列头两行（步骤 2）
3. 周视图任务行卡片化（步骤 3）
4. 周视图空状态图标化（步骤 4）
5. 周视图列底进度条（步骤 5）
6. 日视图头部撕页式（步骤 6）
7. 日视图任务行 + 左侧色条（步骤 7，含 `space-y-1.5` 调整）
8. 日视图空状态增强（步骤 8）
9. 新增 `<style scoped>` CSS 规则（步骤 9）
10. 构建验证（步骤 10）

## 验证步骤

1. **构建验证**：
   - `npx vue-tsc --noEmit` — 类型检查通过
   - `npm run build` — 构建成功

2. **运行时验证**（启动应用后逐项确认）：
   - 周视图：列头两行（周几+日期），今日列顶部 3px 渐变条，任务行卡片化带边框，空状态有图标，列底进度条圆角
   - 日视图：撕页式日期块（月缩写+日数字），任务行左侧状态色条，空状态有图标和提示文案
   - 顶部工具栏：粘性吸顶（如失效回退），渐变背景，视图切换分段控件背景
   - 统计栏：完成率大数字突出，stat-tile 水平排布
   - 筛选栏：灰底与上方视觉断点明显
   - 月视图：保持已优化状态不变（回归验证）
   - P2-5 负载色阶：green/amber/red 三色清晰
   - P1-1 阻塞任务：锁图标和透明度正常
   - P1-2 批量操作：checkbox 和按钮正常
   - 键盘导航：方向键/Enter/Esc/Ctrl+A 全部正常

3. **响应式验证**：
   - 窗口缩放到 1024px 以下：周视图水平滚动而非压扁
   - 窗口缩放到 768px 以下：月格缩小但可读

4. **暗色模式验证**：
   - 切换暗色主题，记录半透明色问题（不在