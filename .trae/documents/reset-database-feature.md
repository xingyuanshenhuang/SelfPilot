# 设置视图新增「重置数据库」功能

## Context（背景）

用户需要"一键将数据库恢复至初始默认状态"，替代繁琐的手动清理流程。当前应用为本地单用户桌面应用（Tauri 2 + Vue 3 + Naive UI + SQLite），无登录体系。

已确认的两个关键决策：
1. **重置范围**：全部恢复默认 —— 清空目标/任务/阶段/自定义鼓励语等业务数据，恢复预设鼓励语，同时删除 `settings` 表（主题回到浅色、图标模式回到本地、鼓励语偏好回默认）。
2. **权限控制 + 操作日志**：确认弹窗中需手动输入「重置」二字才可点击确认（防误触/防非授权操作）；后端用 `tracing` 记录操作时间与结果（操作人员为本地用户，日志文件在 data 目录 logs/）。

方案采用**事务内原地清空**（不重启、不换文件）：单事务内按外键顺序 DELETE + 恢复预设鼓励语 + 重置自增序列，失败自动回滚；可选的重置前备份复用 `restore_database` 已有的 `selfpilot.db.before_restore` 安全网模式。成功后前端刷新各 store + 清理 localStorage 持久化缓存。

## 后端改动（Rust）

### 1. `src-tauri/src/db/models.rs` — 新增输入模型

```rust
/// 重置数据库入参
#[derive(serde::Deserialize, validator::Validate)]
pub struct ResetDatabaseInput {
    /// 是否在重置前自动备份当前数据库（推荐开启）
    pub backup: bool,
}
```

（遵循现有 Input 结构体放 models.rs 的惯例；bool 无需校验规则。）

### 2. `src-tauri/src/commands/backup.rs` — 新增 `reset_database` 命令

放在 `restore_database`（约 L576-605）之后，复用其 `resolve_app_dir` + `before_restore` 备份模式：

```rust
/// 重置数据库：单事务内清空业务数据，恢复预设鼓励语与默认设置
///
/// 步骤：
/// 1. （可选）重置前自动备份当前 db → selfpilot.db.before_restore（复用恢复安全网模式）
/// 2. 单事务按外键顺序 DELETE 用户数据；失败自动回滚（原子性）
/// 3. 恢复预设鼓励语（保留 category='preset'，清 hidden=0，删除 category='custom'）
/// 4. 删除 settings（前端读取侧自带默认值回退 → 主题/图标/鼓励语偏好回默认）
/// 5. 重置 sqlite_sequence（若存在该表）
/// 6. tracing::info 记录操作日志（操作人员=本地用户、时间、结果）
#[tauri::command]
pub async fn reset_database(
    input: ResetDatabaseInput,
    app: AppHandle,
    state: State<'_, DbPool>,
) -> AppResult<()>
```

事务内 SQL（严格按此顺序，外键安全）：

```sql
DELETE FROM task_dependencies;      -- 先清依赖
DELETE FROM tasks;
DELETE FROM stages;
DELETE FROM repeat_series;          -- FK→goals，先删
DELETE FROM goals;                  -- 级联兜底
DELETE FROM encouragement_favorites;
DELETE FROM encouragement_show_log; -- FK→encouragements，先删
DELETE FROM encouragements WHERE category = 'custom';  -- 预设保留
UPDATE encouragements SET hidden = 0 WHERE category = 'preset';  -- 恢复默认可见
DELETE FROM settings;               -- 恢复默认设置
-- sqlite_sequence（存在才删，需先查 sqlite_master）
```

事务处理遵循项目惯例：`state.0.begin()` → 各语句 `execute(&mut *tx)` → `tx.commit()`；错误经 `AppError` 返回（内部错误脱敏，详情进 tracing 日志，见 `error.rs`）。

可选备份：`if input.backup && db_path.exists()` 则 `std::fs::copy(db_path, app_dir.join("selfpilot.db.before_restore"))`，失败直接返回错误（不触碰数据）。

### 3. `src-tauri/src/commands/mod.rs` — 注册命令

在 `generate_handler!` 列表 backup 区块追加：`backup::reset_database,`

## 前端改动（Vue）

### 1. `src/api/backup.ts` — 新增 API 封装

```ts
/** 重置数据库（恢复到初始默认状态，P 需求） */
export async function resetDatabase(input: { backup: boolean }): Promise<void> {
  return invokeCommand("reset_database", { input });
}
```

（复用现有 `invokeCommand` 通道，与 `restoreDatabase` 同级。）

### 2. `src/views/SettingsView.vue` — 新增「重置数据库」危险卡片 + 确认弹窗 + 状态反馈

**a) 新卡片**（插在「一键备份」卡片 L502 之后，「数据导入导出」之前，属于数据库管理区域）：
- NCard 头部：红色图标 `mdi:database-refresh-outline` + 「重置数据库」 + `NTag type="error"` 「危险」
- 说明文字：警告将清空全部目标/任务/自定义鼓励语并恢复默认设置
- `NButton type="error" ghost :loading="resetting"` 「重置数据库」按钮

**b) 确认弹窗**（复用文件内已有的 NModal preset="card" 模式，仿导入确认弹窗）：
- 红色警告文案：明确列出将被清空的内容与后果
- `NCheckbox`：「重置前自动备份当前数据（推荐）」默认勾选（对应 `input.backup`）
- `NInput` 类型确认：「输入「重置」以确认操作」，`v-model` 值不等于「重置」时禁用确认按钮（权限控制落地）
- 底部：取消 / `NButton type="error" :loading="resetting" :disabled="typed !== '重置'"` 确认重置

**c) 确认逻辑 `handleResetDatabase()`**：
- `resetting = true` → `await backupApi.resetDatabase({ backup: backupChecked.value })`
- 成功：刷新全部 store（缓存清理）：
  - `goalStore.fetchGoals()` + `fetchGoalTree()` + `fetchProgresses()`
  - `taskStore.fetchAll()`
  - `encStore.fetchSettings()`
  - 清理持久化缓存：`localStorage.removeItem("selfpilot-settings")` 后 `settingStore.loadTheme()` + `loadIconMode()`（否则主题会停留在旧值，违反"恢复默认"）
- `message.success("数据库已重置为初始默认状态")`
- 失败：`message.error(String(e))`（后端已脱敏）
- `finally: resetting = false`

**d) 状态反馈**：确认按钮 loading 动画 + 成功/失败 message，满足需求第 4 点。

## 需求要点对应

| 需求 | 落地 |
| --- | --- |
| 1 按钮位置与 UI | 一键备份卡片下方新增危险卡片，风格与现有 NCard/NButton 一致（含 dark 模式变体） |
| 2 二次确认 | NModal 弹窗 + 红色风险提示 |
| 3 备份/默认数据/缓存清理 | 可选 before_restore 备份；恢复预设鼓励语（保留 preset 清 custom + hidden 归零）；前端清 localStorage + 刷新全部 store |
| 4 状态反馈 | 按钮 loading + 成功/失败 message |
| 5 权限控制 | 输入「重置」文字校验后才可确认（本地单用户应用无登录体系） |
| 6 操作日志 | `tracing::info!` 记录时间与结果（operator=本地用户） |
| 7 原子性/回滚 | 单事务，任一步失败自动 rollback；备份失败先行中止 |

## 验证

1. `cargo check`（src-tauri）与 `vue-tsc --noEmit` 通过。
2. 手动验证：新建若干目标/任务/自定义鼓励语，打开设置 → 重置数据库 → 弹窗显示 → 取消勾选备份 / 勾选备份两种路径 → 未输入「重置」时确认按钮禁用 → 输入后确认 → 按钮 loading → 成功提示；目标树/今日视图数据已清空，预设鼓励语仍在、自定义消失，主题回到浅色。
3. 失败路径：断开数据目录写权限（或临时制造错误）→ 确认后提示失败、原数据完好（事务回滚）。
4. 检查 data 目录 logs/selfpilot.log 出现 reset 操作记录；勾选备份时生成 selfpilot.db.before_restore。
