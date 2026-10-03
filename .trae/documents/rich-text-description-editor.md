# 目标/任务描述升级为富文本编辑器（Tiptap 3）

## Context

上一轮已为"目标树"的目标和任务新增 `description` 字段（迁移 019，goals/tasks 各加 `description TEXT`），目前用 `<NInput type="textarea">` 编辑、`{{ }}` 纯文本展示。用户要求将其升级为支持加粗/斜体/下划线/字号/颜色等格式的富文本编辑，且**用户已确认**：① 覆盖全部三处描述输入（目标创建/编辑弹窗、任务创建/编辑弹窗、任务描述查看/编辑弹窗）；② 选型 **Tiptap 3**（最新 3.31.4，无头编辑器，用 Naive UI 自建工具栏）。

核心决策：
- **存储**：仍用现有 `description TEXT` 列存**净化后的 HTML**（无需新迁移、无需改 TS 类型）。
- **安全**：HTML 属用户输入 + 备份导入是注入向量 → 在 **Rust 后端统一用 ammonia 白名单净化**（写库前），前端展示直接 `v-html`（数据已安全）。严格 CSP（`script-src 'self'` 等）无需任何改动：Tiptap/ProseMirror 不用 worker/blob，`style-src 'unsafe-inline'` 已覆盖内联 style 属性。
- **兼容**：旧纯文本描述（含换行）在展示容器保留 `whitespace-pre-line`，回车换行不塌陷。

已核实的 Tiptap v3 包结构（curl 官方 dist 声明）：
- `@tiptap/extension-text-style@3.31.4` 导出 `TextStyleKit`（一键注册 TextStyle + Color + FontSize + FontFamily + LineHeight + BackgroundColor），无需自写字号扩展。
- `@tiptap/extensions@3.31.4` 导出 `Placeholder`。
- `@tiptap/starter-kit@3.31.4` 已内置 underline / strike / link / heading / 列表 / `@tiptap/pm`。

## Step 1 — 前端依赖

```powershell
npm i @tiptap/vue-3@^3.31.4 @tiptap/starter-kit@^3.31.4 @tiptap/extension-text-style@^3.31.4 @tiptap/extensions@^3.31.4
```

## Step 2 — 后端净化与校验（Rust）

1. `Cargo.toml` 加 `ammonia = "4"`（纯 Rust，html5ever 依赖，仅编译期影响）。
2. 新建 `src-tauri/src/sanitize.rs`，在 [lib.rs](file:///d:/Desktop/SelfPilot/src-tauri/src/lib.rs) 顶部加 `mod sanitize;`。核心函数：

   - `sanitize_description_html(html) -> String`：`Builder::default()` 基础上 `.rm_tags(&["img"])`；`generic_attributes(HashSet::from(["style"]))` 放行 style；`tag_attributes("a", {"href","target","rel"})`；`attribute_filter` 仅保留 style 中 `color / background-color / font-size / text-align` 且值不含 `url(` 的声明（防 CSS 注入遮罩），其余属性原样放行；`url_relative(UrlRelative::PassThrough)`。
     - ammonia v4 `AttributeFilter` 签名：`Fn(&str, &str, &str) -> Option<Cow<'_, str>>`，`None` 删属性、`Some` 重写，需 `Send + Sync + 'static`。
   - `normalize_description(desc: Option<String>) -> Option<String>`：净化 → 去标签后空白则为 `None`（替代现有"空串归一化"逻辑）→ `chars().take(20000)` 截断兜底。
3. 调用点（绑定前统一走 `normalize_description`）：
   - [commands/goal.rs](file:///d:/Desktop/SelfPilot/src-tauri/src/commands/goal.rs) `create_goal` / `update_goal`
   - [commands/task.rs](file:///d:/Desktop/SelfPilot/src-tauri/src/commands/task.rs) `create_task` / `update_task`（替换现有 `desc.trim().is_empty()` 归一化）
   - [commands/backup.rs](file:///d:/Desktop/SelfPilot/src-tauri/src/commands/backup.rs) goals/tasks 导入共 4 处 INSERT bind（overwrite + import）——备份导入是 XSS 主入口，必须走同一 helper
4. [db/models.rs](file:///d:/Desktop/SelfPilot/src-tauri/src/db/models.rs) 四个 Input（CreateGoalInput / UpdateGoalInput / CreateTaskInput / UpdateTaskInput）的 `description` 加 `#[validate(length(max = 20000))]`（validator 对 `Option<String>` 仅 Some 时校验）。

## Step 3 — 新建 `src/components/RichTextEditor.vue`

- props：`modelValue: string`（HTML）、`placeholder?: string`、`minHeight?: string`（默认 `96px`）。
- emits：`update:modelValue`。
- `useEditor`：extensions = `[StarterKit, TextStyleKit, Placeholder.configure({ placeholder })]`，`content: props.modelValue`，`editorProps.attributes.class = 'richtext-content'`，`onUpdate: ({ editor }) => emit('update:modelValue', editor.getHTML())`。
- 外部值回填防光标跳动：`watch(() => props.modelValue, v => { if (editor && v !== editor.getHTML()) editor.commands.setContent(v || '', { emitUpdate: false }) })`（v3 setContent 第二参为对象；若签名不符按 v2 布尔形式回退）。
- 工具栏（Naive UI + Iconify mdi 图标，flex-wrap）：
  - 加粗/斜体/下划线/删除线：`toggleBold/toggleItalic/toggleUnderline/toggleStrike`，用 `editor.isActive('bold')` 等高亮。
  - 字号下拉（NSelect：默认/12/14/16/18/24/32px）：`setFontSize('14px') / unsetFontSize()`，高亮 `isActive('textStyle', { fontSize })`。
  - 字体颜色（NColorPicker，预设色板）：`setColor / unsetColor`。
  - 无序/有序列表、清除格式（`clearNodes().unsetAllMarks()`）。
- 编辑区：外层 `border rounded` + `min-height` + `max-height:240px overflow-y:auto`；`.ProseMirror` focus 无 outline、`white-space: pre-wrap`；暗色适配（`dark:bg-surface-muted dark:text-gray-200`）；placeholder 样式 `.tiptap p.is-editor-empty:first-child::before { content: attr(data-placeholder); color: ... }`。

## Step 4 — 接入三处输入 + 保存逻辑（GoalTreeView.vue）

- 替换 [GoalTreeView.vue](file:///d:/Desktop/SelfPilot/src/views/GoalTreeView.vue) 三处 `NInput type="textarea"`（L1395-1401 目标描述、L1761-1776 任务描述、L1947-1975 任务描述弹窗）为 `<RichTextEditor v-model:value="...">`（保持 `v-model` 语义）。
- 新建 `src/utils/richText.ts`：`stripHtml(html): string`（状态机去标签，不引新依赖）、`descToSend(html): string | null`（`stripHtml(...).trim() ? html : null`）。
- 保存逻辑：`handleSaveGoal` / `handleSaveTask` / `handleSaveTaskDescription` 中 `description: goalForm.description.trim() || null` 改为 `description: descToSend(goalForm.description)`。

## Step 5 — 展示改 v-html（数据已后端净化）

- [GoalTreeNodeItem.vue](file:///d:/Desktop/SelfPilot/src/components/GoalTreeNodeItem.vue)：
  - `goalDesc`（L111）改为两个计算：`goalDescHtml = node.goal.description ?? ""`（用于 v-html）、`goalDescText = stripHtml(goalDescHtml).trim()`（用于 v-if 判空与 `descLong`，L115 改为 `goalDescText.length > 50 || includes('\n')`）。
  - 头部 1 行截断 div（L387）、tooltip 内容（L391）、展开区"目标说明" p（L524-529）改 `v-html="goalDescHtml"`，容器保留 `whitespace-pre-line break-words`，头部 div 保留 `truncate`。
- [TaskList.vue](file:///d:/Desktop/SelfPilot/src/components/TaskList.vue)：两处 tooltip 内容 span（L196-198 / L276-281）改 `v-html="task.description"`，保留 `whitespace-pre-line break-words` 与 `max-width: 20rem`；v-if 改为 `task.description && stripHtml(task.description).trim()`。
- 前端 [types/index.ts](file:///d:/Desktop/SelfPilot/src/types/index.ts) 无需改动（description 仍是 string|null）。

## Step 6 — 验证

1. `npx vue-tsc --noEmit`、`cargo check`、`cargo test`（sanitize 单元测试：允许标签/颜色字号保留、`<img>`/`url(`/事件属性被清除、旧纯文本保留换行）。
2. 手工（`npm run tauri:dev`）：
   - 三处弹窗编辑富文本（加粗/字号/颜色/列表）→ 保存 → 重开弹窗回填格式完整；
   - 目标树头部 1 行截断 + hover 全文、展开区"目标说明"、任务行图标 tooltip 均渲染格式；
   - 旧纯文本（含换行）描述展示不塌陷；
   - 导入含 `<img onerror>` 的备份后 description 被净化（打开任务描述弹窗验证）；
   - 暗色模式下编辑器/工具栏观感正常；任务行 36px 行高与虚拟滚动不受影响。

## 风险与回退

- **v3 API 漂移**：若 `setContent(v, { emitUpdate:false })` 签名不符 → 回退 v2 布尔形式或 `editor.commands.setContent(v, false)`；若 TextStyleKit 导出异常 → 改逐个注册 `[TextStyle, Color, FontSize]`。
- **ammonia 细节**：`style` 必须先进 `generic_attributes` 白名单再被 `attribute_filter` 裁剪，否则被整体丢弃；`url(` 值必须拒绝。
- 依赖仅新增 4 个前端包 + 1 个 Rust crate，无 CSP/迁移/类型改动，回退成本低。
