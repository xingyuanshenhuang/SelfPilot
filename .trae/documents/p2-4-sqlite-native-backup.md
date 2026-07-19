# P2-4 SQLite 原生备份 — 实现计划

## Context

当前 SelfPilot 的备份方案是 JSON 导出/导入（`export_data` / `import_data` 命令），存在三个问题：
1. **慢** — 逐表 SELECT → JSON 序列化 → 传输到前端 → Blob 下载，大数据量时链路长
2. **类型信息丢失** — JSON 文本格式，数值精度、NULL 语义可能出问题
3. **不完整** — 新增表/字段若忘记加入 `ExportData` 结构体会丢数据

P2-4 引入 SQLite 原生备份：用 `VACUUM INTO` 生成数据库文件的二进制完整副本，10MB 数据库 < 2 秒完成，100% 保留类型信息。JSON 导出保留作为跨版本迁移手段。

## 现状分析

| 组件 | 现状 |
|------|------|
| 数据库路径 | `app_data_dir/selfpilot.db`（[lib.rs:17](file:///d:/桌面/SelfPilot/src-tauri/src/lib.rs#L17)） |
| 连接池 | `DbPool(SqlitePool)`，max_connections=5（[db/mod.rs](file:///d:/桌面/SelfPilot/src-tauri/src/db/mod.rs)） |
| 现有备份命令 | `export_data` / `import_data`（[backup.rs](file:///d:/桌面/SelfPilot/src-tauri/src/commands/backup.rs)） |
| 备份 UI | [SettingsView.vue](file:///d:/桌面/SelfPilot/src/views/SettingsView.vue) 「数据管理」卡片 |
| 前端插件 | `@tauri-apps/plugin-dialog` ^2.0.1 已安装但未使用 |
| 后端插件 | `Cargo.toml` **缺少** `tauri-plugin-dialog` |
| Capabilities | `default.json` 只有 `core:default` |

## 实现方案

### 核心设计决策

1. **备份**：`VACUUM INTO 'target_path'` — 一条 SQL 生成完整 .db 副本，不影响当前连接
2. **恢复**：关闭连接池 → `std::fs::copy` 覆盖 db 文件 → 提示用户重启应用
   - 恢复前自动备份当前 db 到 `selfpilot.db.before_restore` 作为安全网
   - Tauri State 不可变，无法运行时替换 DbPool，重启是唯一可靠方案
3. **JSON 导出保留**：作为跨版本迁移手段，UI 中明确标注两者用途差异

### Step 1: 添加 Tauri dialog 插件（后端）

**`src-tauri/Cargo.toml`** — 在 `[dependencies]` 添加：
```toml
tauri-plugin-dialog = "2"
```

**`src-tauri/src/lib.rs`** — 在 Builder 链中注册插件：
```rust
tauri::Builder::default()
    .plugin(tauri_plugin_dialog::init())  // 新增
    .setup(|app| { ... })
```

**`src-tauri/capabilities/default.json`** — 添加 dialog 权限：
```json
{
  "permissions": ["core:default", "dialog:default"]
}
```

### Step 2: 后端新增备份/恢复命令

**`src-tauri/src/commands/backup.rs`** — 新增两个命令：

```rust
use tauri::{AppHandle, Manager, State};

/// SQLite 原生备份：使用 VACUUM INTO 生成 .db 完整副本
#[tauri::command]
pub async fn backup_database(
    target_path: String,
    state: State<'_, DbPool>,
) -> AppResult<()> {
    // 验证目标路径以 .db 或 .sqlite 结尾
    if !target_path.ends_with(".db") && !target_path.ends_with(".sqlite") {
        return Err(AppError::Param("备份文件必须以 .db 或 .sqlite 结尾".to_string()));
    }
    // VACUUM INTO 不支持参数绑定，需手动转义单引号
    let escaped = target_path.replace('\'', "''");
    sqlx::query(&format!("VACUUM INTO '{}'", escaped))
        .execute(&state.0)
        .await?;
    Ok(())
}

/// SQLite 原生恢复：关闭连接池 → 覆盖 db 文件 → 提示重启
#[tauri::command]
pub async fn restore_database(
    source_path: String,
    app: AppHandle,
    state: State<'_, DbPool>,
) -> AppResult<()> {
    // 验证源文件存在
    if !std::path::Path::new(&source_path).exists() {
        return Err(AppError::Param(format!("备份文件不存在: {}", source_path)));
    }

    // 获取当前 db 路径
    let app_dir = app.path().app_data_dir()?;
    let db_path = app_dir.join("selfpilot.db");
    let backup_path = app_dir.join("selfpilot.db.before_restore");

    // 恢复前自动备份当前 db（安全网）
    if db_path.exists() {
        std::fs::copy(&db_path, &backup_path)?;
    }

    // 关闭连接池（后续数据库操作将失败，因此这是最后一步）
    state.0.close().await;

    // 覆盖 db 文件
    std::fs::copy(&source_path, &db_path)?;

    Ok(())
}
```

**`src-tauri/src/lib.rs`** — 在 `invoke_handler` 中注册：
```rust
commands::backup::backup_database,
commands::backup::restore_database,
```

### Step 3: 前端 API 封装

**`src/api/backup.ts`** — 新增两个函数：
```ts
import { invoke } from "@tauri-apps/api/core";

/** SQLite 原生备份（生成 .db 完整副本） */
export async function backupDatabase(targetPath: string): Promise<void> {
  return invoke("backup_database", { targetPath });
}

/** SQLite 原生恢复（覆盖当前数据库，需重启应用） */
export async function restoreDatabase(sourcePath: string): Promise<void> {
  return invoke("restore_database", { sourcePath });
}
```

### Step 4: 前端 UI 扩展

**`src/views/SettingsView.vue`** — 在「数据管理」卡片的「导入」区域下方新增「原生备份」区域：

```vue
<!-- 原生备份（P2-4） -->
<div>
  <div class="text-sm font-medium mb-2 flex items-center gap-2">
    <Icon icon="mdi:database-sync-outline" width="16" class="text-purple-500" />
    SQLite 原生备份（推荐）
  </div>
  <div class="text-xs text-gray-500 mb-2">
    使用 SQLite VACUUM INTO 生成完整数据库副本，保留全部类型信息，恢复速度快。适用于同版本快速备份/恢复。
  </div>
  <NSpace>
    <NButton type="primary" ghost :loading="nativeBackingUp" @click="handleNativeBackup">
      <template #icon><Icon icon="mdi:database-export-outline" /></template>
      原生备份
    </NButton>
    <NButton type="warning" ghost :loading="nativeRestoring" @click="handleNativeRestore">
      <template #icon><Icon icon="mdi:database-import-outline" /></template>
      恢复备份
    </NButton>
  </NSpace>
</div>
```

**新增的 script 逻辑**：
```ts
import { save, open } from "@tauri-apps/plugin-dialog";
import * as backupApi from "@/api/backup";

const nativeBackingUp = ref(false);
const nativeRestoring = ref(false);

/** 原生备份：弹出保存对话框 → 调用 backup_database */
async function handleNativeBackup() {
  try {
    const ts = new Date().toISOString().replace(/[:.]/g, "-").slice(0, 19);
    const targetPath = await save({
      title: "选择备份保存位置",
      defaultPath: `selfpilot-backup-${ts}.db`,
      filters: [{ name: "SQLite 数据库", extensions: ["db", "sqlite"] }],
    });
    if (!targetPath) return; // 用户取消

    nativeBackingUp.value = true;
    await backupApi.backupDatabase(targetPath);
    message.success(`已备份到: ${targetPath}`);
  } catch (e) {
    message.error(`备份失败: ${String(e)}`);
  } finally {
    nativeBackingUp.value = false;
  }
}

/** 原生恢复：弹出打开对话框 → 确认对话框 → 调用 restore_database → 提示重启 */
async function handleNativeRestore() {
  try {
    const sourcePath = await open({
      title: "选择备份文件恢复",
      multiple: false,
      filters: [{ name: "SQLite 数据库", extensions: ["db", "sqlite"] }],
    });
    if (!sourcePath || Array.isArray(sourcePath)) return; // 用户取消

    dialog.warning({
      title: "确认恢复备份",
      content: "恢复将覆盖当前所有数据，且应用需要重启。恢复前会自动备份当前数据到 selfpilot.db.before_restore。确定继续？",
      positiveText: "确认恢复",
      negativeText: "取消",
      onPositiveClick: async () => {
        nativeRestoring.value = true;
        try {
          await backupApi.restoreDatabase(sourcePath as string);
          // 恢复成功后连接池已关闭，必须重启应用
          // 未安装 @tauri-apps/plugin-process，提示用户手动重启
          dialog.success({
            title: "恢复成功",
            content: "数据库已恢复，请关闭应用并重新打开以加载恢复的数据。",
            positiveText: "我知道了",
          });
        } catch (e) {
          message.error(`恢复失败: ${String(e)}`);
        } finally {
          nativeRestoring.value = false;
        }
      },
    });
  } catch (e) {
    message.error(String(e));
  }
}
```

### Step 5: 重启策略

`package.json` 中未安装 `@tauri-apps/plugin-process`，为避免引入新依赖，采用**提示用户手动重启**方案。恢复成功后弹出 `dialog.success` 提示「请关闭应用并重新打开」。

> 后续若想实现自动重启，可另装 `@tauri-apps/plugin-process`（前端）+ `tauri-plugin-process`（后端），调用 `relaunch()`。本次不引入。

## 涉及文件

| 文件 | 改动 |
|------|------|
| `src-tauri/Cargo.toml` | 添加 `tauri-plugin-dialog = "2"` |
| `src-tauri/capabilities/default.json` | 添加 `"dialog:default"` 权限 |
| `src-tauri/src/lib.rs` | 注册 dialog 插件 + 注册 2 个新命令 |
| `src-tauri/src/commands/backup.rs` | 新增 `backup_database` 和 `restore_database` 命令 |
| `src/api/backup.ts` | 新增 `backupDatabase` 和 `restoreDatabase` 函数 |
| `src/views/SettingsView.vue` | 新增「SQLite 原生备份」UI 区域 + 交互逻辑 |

## 验证步骤

1. **构建验证**：
   - `cd src-tauri && cargo check` — Rust 编译通过
   - `npx vue-tsc --noEmit` — 前端类型检查通过

2. **功能验证**（启动应用后）：
   - 进入「设置」→「数据管理」→ 看到「SQLite 原生备份」区域
   - 点击「原生备份」→ 弹出保存对话框 → 选择位置 → 生成 .db 文件
   - 验证 .db 文件大小与原数据库相近（应为紧凑版，可能更小）
   - 点击「恢复备份」→ 弹出打开对话框 → 选择刚备份的文件 → 确认对话框 → 恢复成功 → 应用重启
   - 重启后数据完整（目标、任务、依赖、鼓励语、设置都在）

3. **安全网验证**：
   - 恢复后检查 `%APPDATA%\com.selfpilot.app\selfpilot.db.before_restore` 文件存在

4. **错误场景**：
   - 选择非 .db 文件 → 后端返回参数错误
   - 选择不存在的文件 → 后端返回参数错误
   - 备份到只读目录 → 后端返回数据库错误

## 风险与注意事项

1. **VACUUM INTO 路径转义**：不支持参数绑定，需手动转义单引号（`'` → `''`）。Windows 路径的反斜杠不影响 SQLite。
2. **恢复后连接池失效**：`pool.close()` 后任何数据库操作都会 panic，因此恢复命令必须是最后一步操作，前端立即提示重启。
3. **before_restore 安全网**：恢复前自动备份当前 db，若恢复失败用户可手动恢复。但此文件不会自动清理，长期使用可能积累（可接受）。
4. **JSON 导出保留**：UI 中明确标注「原生备份