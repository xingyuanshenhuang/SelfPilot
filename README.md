<div align="center">

# SelfPilot

**离线个人任务规划桌面应用 · 把宏大目标自动拆解为可执行的每日任务**

[![Version](https://img.shields.io/badge/version-0.1.0-blue.svg)](./package.json)
[![Tauri](https://img.shields.io/badge/Tauri-2.x-orange.svg)](https://tauri.app/)
[![Vue](https://img.shields.io/badge/Vue-3.5+-42b883.svg)](https://vuejs.org/)
[![Rust](https://img.shields.io/badge/Rust-1.80+-dea584.svg)](https://www.rust-lang.org/)
[![TypeScript](https://img.shields.io/badge/TypeScript-5.6+-3178c6.svg)](https://www.typescriptlang.org/)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-lightgrey.svg)](#安装指南)
[![License](https://img.shields.io/badge/license-TBD-red.svg)](#许可证)

面向自学者、学生及考证人群 · 离线优先 · 数据本地存储 · 无需登录

</div>

---

## 目录

- [项目简介](#项目简介)
- [核心功能](#核心功能)
- [技术栈](#技术栈)
- [项目结构](#项目结构)
- [快速开始](#快速开始)
- [安装指南](#安装指南)
- [使用方法](#使用方法)
- [数据存储与备份](#数据存储与备份)
- [项目状态](#项目状态)
- [常见问题](#常见问题)
- [贡献说明](#贡献说明)
- [许可证](#许可证)

---

## 项目简介

**SelfPilot** 解决了传统 Todo 工具在「长期目标管理」上的不足：用户常常只能记录零散任务，却难以把一个宏大目标（如 *30 天背完 3000 个单词*、*3 个月通过 PMP*）自动拆解为可执行的每日任务，并实时掌握整体进度。

### 核心价值

| 特性 | 说明 |
|------|------|
| **三级结构** | 目标 → 阶段 → 任务，兼顾宏观规划与微观执行 |
| **自动拆解** | 按截止日期把目标总量平均分配到每天，余数自动前置 |
| **进度汇总** | 子任务完成情况实时向上反馈到阶段与目标 |
| **离线优先** | 数据完全存储在本地 SQLite，无需登录、无需网络 |

---

## 核心功能

应用围绕 **6 个固定模块**展开，对应左侧导航栏：

### 目标总览
展示所有目标完成百分比（环形进度）、今日待办、逾期任务，支持快速 **完成 / 跳过 / 补完成**。

### 目标树
以层级卡片形式管理目标、阶段与任务；支持创建目标、自动拆解、添加阶段、重新规划。

### 日历视图
月 / 周 / 日三视图展示每日任务分布，支持点击查看详情并批量完成 / 跳过。

### 数据统计
- 柱状图：各目标完成度
- 折线图：近 7 / 30 天完成趋势
- 日历热力图：90 / 180 / 365 天切换
- 完成预测：基于过去 7 天平均速度给出建议

### 鼓励语库
预设 + 自定义鼓励语，按连续打卡天数智能抽取等级（normal / advanced / highlight / celebration），全部目标完成时触发庆祝语。

### 设置
浅色 / 深色主题切换，JSON 数据导出 / 导入，支持 **跳过 / 覆盖 / 重命名** 三种 ID 冲突处理。

---

## 技术栈

| 层级 | 技术 | 版本 | 说明 |
|------|------|------|------|
| 桌面框架 | [Tauri](https://tauri.app/) | 2.x | 系统 WebView 渲染，包体小、启动快 |
| 后端语言 | [Rust](https://www.rust-lang.org/) | 1.80+ | 内存安全，SQLite 查询与汇总逻辑均在 Rust 侧 |
| 前端框架 | [Vue](https://vuejs.org/) | 3.5+ | Composition API + `<script setup>` |
| 前端语言 | [TypeScript](https://www.typescriptlang.org/) | 5.6+ | 与 Rust 模型对齐，减少 IPC 类型错位 |
| 构建工具 | [Vite](https://vitejs.dev/) | 5.x | Tauri 官方默认，HMR 极速 |
| UI 组件库 | [Naive UI](https://www.naiveui.com/) | 2.x | Vue 3 原生，支持深色模式与主题定制 |
| CSS 引擎 | [UnoCSS](https://unocss.dev/) | 0.63 | 原子化按需生成 |
| 图标库 | [@iconify/vue](https://iconify.design/) | 4.x | 按需加载，图标映射零成本 |
| 状态管理 | [Pinia](https://pinia.vuejs.org/) + persistedstate | 2.x / 3.x | 主题持久化到 localStorage |
| 日期处理 | [date-fns](https://date-fns.org/) | 3.x | 函数式、按需引入 |
| 图表库 | [ECharts](https://echarts.apache.org/)（vue-echarts） | 5.5+ | 柱状图、折线图、热力图单库覆盖 |
| 数据库 | SQLite + [sqlx](https://github.com/launchbadge/sqlx) | 3.x / 0.8 | 编译时 SQL 检查，异步原生 |
| 数据序列化 | [serde](https://serde.rs/) + serde_json | 1.x | 导入导出 JSON 备份 |

---

## 项目结构

```
SelfPilot/
├── src/                          # 前端源码（Vue 3 + TypeScript）
│   ├── api/                      # Tauri Command 调用封装
│   ├── stores/                   # Pinia 状态管理
│   ├── views/                    # 6 个页面级视图
│   ├── components/               # 公共组件（TaskItem、ProgressRing）
│   ├── types/                    # 全局 TypeScript 类型
│   ├── constants/                # 预设鼓励语等常量
│   ├── App.vue                   # 根组件（左右布局 + 鼓励语弹窗）
│   └── main.ts                   # 应用入口
├── src-tauri/                    # Tauri / Rust 后端
│   ├── src/
│   │   ├── commands/             # Tauri Commands（IPC 接口）
│   │   ├── db/                   # 数据模型与连接池
│   │   ├── services/             # 业务逻辑（自动拆解、进度汇总）
│   │   ├── error.rs              # 全局错误类型
│   │   └── lib.rs                # 模块聚合与迁移初始化
│   └── migrations/               # sqlx 迁移脚本
├── package.json
├── Cargo.toml
├── vite.config.mts
└── uno.config.ts
```

### 数据模型

采用 **UUID + parent_id + path** 的三级树形结构：

- `Goal`（目标，一级节点）
- `Stage`（阶段，二级节点，可选）
- `Task`（任务，三级节点，实际执行单元）

任务状态：`pending`（未完成）、`partial`（部分完成）、`done`（已完成）、`skipped`（已跳过）。

---

## 快速开始

只需三步即可在本地运行：

```bash
# 1. 克隆仓库
git clone https://github.com/<your-username>/SelfPilot.git
cd SelfPilot

# 2. 安装前端依赖
npm install

# 3. 启动 Tauri 开发模式（同时编译前端与 Rust 后端）
npm run tauri:dev
```

> 首次运行需安装 Rust 工具链，详见 [安装指南](#安装指南)。

---

## 安装指南

### 环境要求

| 工具 | 最低版本 | 说明 |
|------|----------|------|
| [Node.js](https://nodejs.org/) | 18+ | 前端构建与依赖管理 |
| [Rust](https://www.rust-lang.org/tools/install) | 1.80+ | 编译 Tauri 后端 |
| [Tauri 2 Prerequisites](https://tauri.app/start/prerequisites/) | — | 各平台系统依赖（WebView 等） |

**平台专属依赖：**

- **Windows**：[Microsoft C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) + WebView2 Runtime
- **macOS**：Xcode Command Line Tools（`xcode-select --install`）
- **Linux**：参考 [Tauri Linux 依赖说明](https://tauri.app/start/prerequisites/)

### 详细安装步骤

1. **克隆仓库**

   ```bash
   git clone https://github.com/<your-username>/SelfPilot.git
   cd SelfPilot
   ```

2. **安装前端依赖**

   ```bash
   npm install
   ```

3. **拉取 Rust 依赖**（首次运行由 Cargo 自动完成）

   ```bash
   cd src-tauri && cargo fetch && cd ..
   ```

### 开发模式

```bash
npm run tauri:dev    # Tauri 开发模式（前端 HMR + Rust 后端）
npm run dev          # 仅前端开发服务器（纯 UI 调试，无 IPC）
```

### 构建生产包

```bash
npm run tauri:build
```

构建产物位于 `src-tauri/target/release/bundle/`，按平台输出：

- Windows：`.msi` / `.exe`
- macOS：`.dmg` / `.app`
- Linux：`.deb` / `.AppImage`

### 可用脚本

| 命令 | 说明 |
|------|------|
| `npm run dev` | 仅启动 Vite 前端开发服务器 |
| `npm run build` | 类型检查 (`vue-tsc`) + 生产构建前端 |
| `npm run preview` | 预览前端构建产物 |
| `npm run tauri:dev` | 启动 Tauri 开发模式（前端 + Rust） |
| `npm run tauri:build` | 构建生产级桌面安装包 |

---

## 使用方法

### 1. 创建目标

进入「目标树」→ 点击「创建目标」→ 填写目标名称、总量、单位、开始日期与截止日期，应用会自动按剩余天数拆解为每日任务。

### 2. 日常执行

- **目标总览**：查看今日待办，可一键 **完成 / 跳过 / 补完成**
- **日历视图**：在月 / 周 / 日视图中查看任务分布，日视图支持多选后批量完成 / 跳过

### 3. 进度追踪

- **数据统计**：通过柱状图、折线图、热力图多维度查看完成情况
- **完成预测**：基于过去 7 天平均速度给出 `ahead` / `on_track` / `need_speed` / `completed` 等状态与建议

### 4. 任务生命周期

| 操作 | 说明 |
|------|------|
| 完成 | 支持部分完成（`actual_qty < plan_qty` → `partial`） |
| 跳过 | 标记为 `skipped`，不计入完成，不影响后续计划 |
| 补完成 | 仅更新历史任务的实际完成量，**绝不重新分配未来任务** |
| 重新规划 | 过滤已跳过任务，保留手动调整项，支持 Before / After 预览 |

### 5. 鼓励机制

- 完成当日首个任务后弹出鼓励语弹窗
- 连续打卡规则：每天至少完成 1 个任务计 1 天；无任务不中断也不计入；有任务未完成则中断
- 全部目标完成时触发庆祝语

### 6. 主题与数据

- 在「设置」中切换 **浅色 / 深色** 主题
- 支持 JSON 全量导出 / 导入，导入时可选 `skip` / `overwrite` / `rename` 三种 ID 冲突策略

---

## 数据存储与备份

- 本地 SQLite 数据库存储于应用数据目录（`selfpilot.db`），无需联网
- 数据库版本通过 sqlx 迁移脚本管理：`001_init` → `002_encouragement_settings` → `003_encouragement_level`
- 支持 JSON 全量备份与恢复，便于跨设备迁移

---

## 项目状态

整体完成度约 **85% ~ 90%**，已具备内测可用条件。

| 迭代 | 计划内容 | 状态 |
|------|----------|------|
| Sprint 1 | 创建目标、自动拆解、今日待办、完成 / 跳过 | ✅ 完成 |
| Sprint 2 | 补完成、重新规划、阶段管理 | ⚠️ ~90%（目标树手动添加任务 UI 待补） |
| Sprint 3 | 日历视图（月/周/日）、基础统计 | ✅ 完成 |
| Sprint 4 | 鼓励语库、设置、数据导出导入 | ✅ ~95% |
| Sprint 5 | 日历热力图、鼓励语个性化规则 | ✅ 完成 |
| Sprint 6 | 完成时间预测 | ✅ 完成 |

### 后续规划

- **P0**：目标树内手动添加任务入口、任务拖拽归属
- **P1**：鼓励语编辑入口、目标编辑、周期性手动任务
- **P2**：SQLite 原生备份、云端同步、虚拟滚动优化、国际化

---

## 常见问题

<details>
<summary><b>Windows 下 <code>cargo check</code> / <code>tauri build</code> 失败（rc.exe 报错）</b></summary>

Tauri 构建脚本调用 Windows Resource Compiler 时可能出现环境级 panic（`Os { code: 0 }`），属于工具链 / 环境兼容问题，非代码逻辑错误。

**解决方法：**
1. 安装完整的 [Microsoft C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)（勾选「使用 C++ 的桌面开发」工作负载）
2. 确保 Windows SDK 已安装
3. 在「x64 Native Tools Command Prompt for VS」中重新执行构建
</details>

<details>
<summary><b><code>npm run build</code> 提示找不到依赖</b></summary>

项目尚未安装依赖，请先执行：

```bash
npm install
```
</details>

<details>
<summary><b>修改 Rust 代码后未生效</b></summary>

Tauri 开发模式下，Rust 代码改动会自动重新编译；若未生效，请手动重启 `npm run tauri:dev`，或在终端按 <kbd>R</kbd> 触发重新编译。
</details>

<details>
<summary><b>数据库迁移失败</b></summary>

sqlx 迁移脚本位于 `src-tauri/migrations/`。如遇迁移冲突，可删除应用数据目录下的 `selfpilot.db` 重新初始化（**会清空数据，请先导出 JSON 备份**）。
</details>

---

## 贡献说明

欢迎通过 Issue 与 Pull Request 参与本项目！

### 贡献流程

1. **Fork** 本仓库并克隆到本地
2. 基于最新 `main` 分支创建特性分支：
   ```bash
   git checkout -b feat/your-feature
   ```
3. 遵循现有代码风格进行开发：
   - 前端使用 Vue 3 `<script setup>` + TypeScript
   - 后端使用 Rust，业务逻辑置于 `services/`，IPC 接口置于 `commands/`
4. 提交前请确保通过以下校验：
   ```bash
   npm run build          # 前端类型检查 + 构建
   cd src-tauri && cargo check && cd ..   # Rust 编译检查
   ```
5. 提交信息遵循 [Conventional Commits](https://www.conventionalcommits.org/) 规范：
   - `feat: 新增目标树手动添加任务入口`
   - `fix: 修复日历视图逾期标记计算错误`
   - `docs: 更新安装指南`
6. 发起 Pull Request，并在描述中说明改动内容与关联 Issue

### 代码规范

- 提交前请移除调试代码与无意义注释
- 新增功能请同步更新相关类型定义与文档
- 不建议引入与现有技术栈重复的第三方依赖

---

## 许可证

本项目当前**未指定开源许可证**，默认所有权利归作者所有，仅供学习与内部参考使用。

如需将其开源，建议添加 [LICENSE](./LICENSE) 文件（推荐 [MIT](https://opensource.org/licenses/MIT) 或 [Apache-2.0](https://www.apache.org/licenses/LICENSE-2.0)），并在本节更新声明：

```text
MIT License

Copyright (c) 2026 SelfPilot Contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files...
```

---

## 致谢

本项目基于以下优秀开源技术构建：

- [Tauri](https://tauri.app/) · 轻量级跨平台桌面框架
- [Vue.js](https://vuejs.org/) · 渐进式 JavaScript 框架
- [Naive UI](https://www.naiveui.com/) · Vue 3 组件库
- [ECharts](https://echarts.apache.org/) · 数据可视化图表库
- [sqlx](https://github.com/launchbadge/sqlx) · Rust 异步 SQL 工具包

---

<div align="center">

**SelfPilot** · 让每一个长期目标都拥有清晰的执行路径

[报告问题](https://github.com/<your-username>/SelfPilot/issues) · [发起讨论](https://github.com/<your-username>/SelfPilot/discussions)

</div>
