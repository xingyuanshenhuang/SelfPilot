# 问题记录：庆祝弹窗彩带动画被 CSP 拦截（canvas-confetti blob Worker）

> 本文件属于 `issue_records/` 问题记录库。每个问题一个文件，文件命名规范：`YYYY-MM-DD-<简短描述>.md`。

## 元信息

| 字段 | 内容 |
|---|---|
| 问题编号 | ISSUE-20260830-001 |
| 问题标题 | 庆祝弹窗彩带动画被 CSP 拦截（canvas-confetti blob Worker） |
| 状态 | ✅ 已修复（待运行态确认） |
| 优先级 | 中 |
| 影响范围 | 前端庆祝动画功能（CelebrationModal） |
| 报告人 | 用户（测试反馈） |
| 记录时间 | 2026-08-30 |
| 相关文件 | [CelebrationModal.vue](file:///d:/Desktop/SelfPilot/src/components/CelebrationModal.vue)、[tauri.conf.json](file:///d:/Desktop/SelfPilot/src-tauri/tauri.conf.json)、[index.html](file:///d:/Desktop/SelfPilot/index.html) |

---

## 1. 问题描述

### 1.1 发生时间
2026-08-30（本地时区 Asia/Shanghai），在完成庆祝弹窗相关测试时于浏览器/Webview 控制台发现。

### 1.2 环境信息
- **操作系统**：Windows
- **应用框架**：Tauri v2 桌面应用（开发模式，前端地址 `http://localhost:1420`）
- **前端技术栈**：Vue 3 + Vite + TypeScript + Pinia
- **相关依赖**：`canvas-confetti@^1.9.4`
- **安全策略**：严格 CSP（`default-src 'self'; img-src 'self' data: blob:; style-src 'self' 'unsafe-inline'; script-src 'self'; connect-src 'self' ipc: http://ipc.localhost`），配置于 `tauri.conf.json` 与 `index.html` 两处（冗余维护）
- **触发入口**：全部目标完成 → 庆祝弹窗（CelebrationModal）→ `playConfetti()` 播放彩带动画

### 1.3 问题现象
庆祝弹窗正常弹出，但彩带动画不播放；控制台持续输出 CSP 违规报错。

---

## 2. 复现步骤

1. 启动应用（`tauri:dev` / `npm run dev`）。
2. 完成全部目标（使目标进度达到 100%）。
3. 触发庆祝弹窗（`CelebrationModal` 出现）。
4. 打开开发者工具控制台，观察报错。

预期：彩带从两侧及中央喷出，动画流畅播放。
实际：无任何彩带效果，控制台报 CSP 拦截错误。

---

## 3. 错误表现

控制台核心报错（截取关键行）：

```
CelebrationModal.vue:33 Creating a worker from 'blob:http://localhost:1420/<uuid>' violates the
following Content Security Policy directive: "script-src 'self'". Note that 'worker-src' was not
explicitly set, so 'script-src' is used as a fallback. The action has been blocked.
confetti.module.mjs:213 ...
```

次要报错（非功能性）：

```
:1420/favicon.ico:1 Failed to load resource: the server responded with a status of 404 (Not Found)
```

---

## 4. 初步分析

- `canvas-confetti` 默认通过 **blob URL 创建 Web Worker** 来运行动画（离主线程渲染）。
- 项目采用严格 CSP，`script-src 'self'` 且未显式声明 `worker-src`。按 CSP 规范，`worker-src` 未设置时回退到 `script-src`，因此 `blob:` 形式的 Worker 被直接拦截，动画初始化即失败。
- 该问题仅影响动画展示，不影响弹窗本身与后续逻辑（弹窗由 `App.vue` 正常渲染）。
- 修复思路需在"保持严格 CSP 不变"与"让动画可运行"之间取舍，优先使用库自身能力而非放宽安全策略。

---

## 5. 已尝试的解决方案及结果

| 方案 | 说明 | 结果 |
|---|---|---|
| 方案 A：逐调用传 `useWorker: false` | 在每次 `confetti({...})` 调用中追加 `useWorker: false` | ❌ 类型检查失败：`@types/canvas-confetti` 未在每次调用的 `Options` 类型中声明 `useWorker` 字段（仅 `create()` 的 `GlobalOptions` 声明） |
| 方案 B（采用）：`confetti.create(undefined, { useWorker: false })` | 组件内一次性创建禁用 Worker 的 confetti 实例，再于 `playConfetti()` 中调用该实例 | ✅ `npm run build`（含 `vue-tsc` 类型检查 + vite 构建）通过；不产生 blob Worker，无需改动 CSP |
| 方案 C（备选，未采用）：放宽 CSP | 在 `tauri.conf.json` 与 `index.html` 的 CSP 中增加 `worker-src 'self' blob:` | ⏸ 未采用：违反项目"严格 CSP"硬约束，且 `worker-src blob:` 属于放宽面，安全性劣于方案 B |

---

## 6. 当前状态

- **代码修复已完成**：[CelebrationModal.vue](file:///d:/Desktop/SelfPilot/src/components/CelebrationModal.vue#L5-L11) 引入 `const confettiCannon = confetti.create(undefined, { useWorker: false });`，`playConfetti()` 改调用 `confettiCannon(...)`。
- **构建验证通过**：`npm run build`（类型检查 + 生产构建）成功。
- **待运行态确认**：尚未在运行中的应用中人工确认彩带动画播放及控制台无 CSP 报错（需重启 dev 后触发庆祝场景）。
- 注：`favicon.ico 404` 为无害的外观性报错（项目无 `public/` 目录与 favicon 资源），不影响功能。

---

## 7. 后续处理计划

1. **运行态验证**：重启开发服务，完成全部目标触发庆祝弹窗，确认彩带动画正常播放、控制台不再出现 CSP 报错。
2. **（可选）消除 favicon 404**：在 `public/` 放置 favicon 资源并在 `index.html` 引用；若不需要可忽略。
3. **同类排查**：检查其它第三方库是否同样存在 blob Worker / 内联脚本等被严格 CSP 拦截的用法，必要时按"优先使用库能力、最后才考虑放宽 CSP"的原则统一处理。
4. **归档**：运行态验证通过后，将本记录状态更新为"已关闭"，并把修复要点沉淀到项目约定中。
