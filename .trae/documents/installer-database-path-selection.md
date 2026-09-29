# 安装时数据库存储路径选择功能实施计划

## Context

用户要求为 `npm run tauri:build` 产出的 NSIS（exe）与 MSI 两种安装包添加"安装时选择数据库存储路径"功能。已确认的技术边界与决策：

- **NSIS**：Tauri v2 的 `installerHooks` 无法插入向导页面 → 采用完整自定义模板 `nsis.template`，新增真正的"数据库存储路径"向导页（含浏览按钮）。
- **MSI**：Tauri v2 无法替换内置 UI 对话框（ui.wixobj）→ 采用命令行属性 `msiexec /i SelfPilot.msi DATADIR="D:\MyData"`，通过 WiX fragment 写注册表（用户已确认）。
- **持久化机制**：注册表 `HKCU\Software\com.selfpilot.desktop`，字符串值 `DataDir`。卸载不清除（重装/升级保留选择）；文档提供重置命令。
- **运行时**：数据目录唯一汇聚点 [portable.rs](file:///d:/Desktop/SelfPilot/src-tauri/src/portable.rs) 的 `resolve_app_dir()`（[lib.rs L27](file:///d:/Desktop/SelfPilot/src-tauri/src/lib.rs#L27) 初始化 DB/日志、[backup.rs L579](file:///d:/Desktop/SelfPilot/src-tauri/src/commands/backup.rs#L579) 恢复均走它），改此处全局生效，lib.rs/backup.rs 零改动。

已核实的关键事实（Plan agent 验证）：
- @tauri-apps/cli 实际版本 **2.11.4**，官方模板位于 `tauri-apps/tauri` 仓库 `tauri-cli-v2.11.4` tag 下 `crates/tauri-bundler/src/bundle/windows/nsis/installer.nsi`（978 行 handlebars 模板）。
- Tauri v2 WiX 配置字段名为 `componentGroupRefs`（复数数组）+ `fragmentPaths`。
- NSIS `installMode` 未配置时默认 `currentUser`（HKCU 写入无提权问题）；MSI main.wxs 固定 `InstallScope="perMachine"`。
- NSIS 模板页序：Welcome → License → Reinstall → **MUI_PAGE_DIRECTORY** → StartMenu → INSTFILES → Finish，新页插在目录页之后。
- 模板的卸载键 `Software\{厂商}\SelfPilot` 与我们的 `Software\com.selfpilot.desktop` 不同键，卸载/勾选"删除应用数据"不会误删 DataDir。

## 文件修改清单

| 文件 | 操作 | 要点 |
|---|---|---|
| `src-tauri/nsis/installer.nsi` | 新增 | 从官方 tauri-cli-v2.11.4 模板原样复制，仅做 5 处增量修改（见下），文件头加注释列出补丁点（便于 CLI 升级时重放） |
| `src-tauri/tauri.conf.json` | 修改 | nsis 加 `"template": "nsis/installer.nsi"`；wix 加 `"fragmentPaths": ["wix/datadir.wxs"]`、`"componentGroupRefs": ["DataDirComponentGroup"]` |
| `src-tauri/wix/datadir.wxs` | 新增 | MSI 注册表写入 fragment（完整 XML 见下） |
| `src-tauri/nsis/SimpChinese.nsh` | 修改 | 追加 2~3 条 LangString（新页面标题/副标题） |
| `src-tauri/src/portable.rs` | 修改 | 新增注册表读取 + 路径校验，`resolve_app_dir()` 插入第二优先级 |
| `src-tauri/Cargo.toml` | 修改 | 新增 `[target.'cfg(windows)'.dependencies] winreg = "0.55"` |
| `README.md`（构建章节 L208-229） | 修改 | 补 MSI DATADIR 用法、NSIS 页面说明、重置方法 |

## NSIS 模板 5 处增量修改

1. **Var 区**（`Var OldMainBinaryName` 后）：新增 `Var DataDir` / `Var DataDirText`；`!define MANUPRODUCTKEY` 后加 `!define DATADIRKEY "Software\${BUNDLEID}"`。
2. **页面注册**（`!insertmacro MUI_PAGE_DIRECTORY` 之后、Start menu 页之前）：`Page custom PageDataDir PageLeaveDataDir`。
3. **页面函数**（放在 PageLeaveReinstall 之后）：
   - `PageDataDir`：`${If} $PassiveMode = 1 ${OrIf} ${Silent}` → `Abort`（静默/passive 跳过，靠 `/DATADIR=` 参数）；`ReadRegStr $DataDir HKCU "${DATADIRKEY}" "DataDir"`，为空则 `StrCpy $DataDir "$APPDATA\${BUNDLEID}"`（升级安装自动预填旧值）；`!insertmacro MUI_HEADER_TEXT`；`nsDialogs::Create 1018` + `${NSD_CreateDirRequest}`（文本框+浏览按钮；若 browse 未自动关联，则手动 `${NSD_OnClick}` 接 `nsDialogs::SelectFolderDialog`）。
   - `PageLeaveDataDir`：`${NSD_GetText}` 存回 `$DataDir`，去末尾 `\`（保留 `C:\` 根情形）。**不在页面 Leave 写注册表**（用户点"上一步/取消"会残留）。
4. **`.onInit`**（`/UPDATE` GetOptions 块后）：`${GetOptions} $CMDLINE "/DATADIR" $DataDir`（静默安装支持命令行传参）。
5. **Section Install**（`WriteRegStr SHCTX "${MANUPRODUCTKEY}" "" $INSTDIR` 之后）：`${If} $DataDir != ""` → `CreateDirectory "$DataDir"` + `WriteRegStr HKCU "${DATADIRKEY}" "DataDir" "$DataDir"`（仅真正开始安装才落盘；显式 HKCU 与运行时读取对齐）。卸载段不动（保留 DataDir）。

所有 `{{handlebars}}` 变量原样保留，禁止新增模板变量（bundler 渲染严格校验）。

## WiX fragment（src-tauri/wix/datadir.wxs）

```xml
<?xml version="1.0" encoding="utf-8"?>
<Wix xmlns="http://schemas.microsoft.com/wix/2006/wi">
  <Fragment>
    <Property Id="DATADIR" Secure="yes" />
    <ComponentGroup Id="DataDirComponentGroup">
      <Component Id="DataDirRegistry" Directory="INSTALLDIR"
                 Guid="<生成一个 uuid v4>" Permanent="yes">
        <Condition><![CDATA[DATADIR <> ""]]></Condition>
        <RegistryValue Root="HKCU" Key="Software\com.selfpilot.desktop"
                       Name="DataDir" Type="string" Value="[DATADIR]" KeyPath="yes" />
      </Component>
    </ComponentGroup>
  </Fragment>
</Wix>
```

- `INSTALLDIR` 为 main.wxs 已定义目录；`Permanent="yes"` 保证卸载/升级（MajorUpgrade 移除旧产品）不清值。
- 不传 DATADIR 时条件为假，不写不删。

## Rust 侧（portable.rs）

新增私有函数（`#[cfg(windows)]`，非 Windows 编译为返回 None 的空实现）：

```rust
fn registry_data_dir(identifier: &str) -> Option<PathBuf>
```

- `winreg::RegKey::predef(HKEY_CURRENT_USER).open_subkey(format!(r"Software\{identifier}"))` → `get_value::<String, _>("DataDir")`，键不存在/读取失败一律 `None`（属正常情况）。
- `validate_data_dir(&raw)` 校验：非空、绝对路径、无 NUL 与 `<>"|?*` 非法字符、无 `..` 组件、组件名非 Windows 保留名（CON/PRN/AUX/NUL/COM1-9/LPT1-9）。不复用 backup.rs 的 `validate_path_scope`（文件导向私有函数），留注释说明。
- `std::fs::create_dir_all` 失败 → `tracing::warn!` + `None` 回退。
- `resolve_app_dir()` 优先级改为：`portable_data_dir()` → `registry_data_dir(&app.config().identifier)`（identifier 取自 config，不硬编码）→ `app.path().app_data_dir()`。
- 全程 Option/Result，无 panic/unwrap；内部错误细节仅进日志（项目错误脱敏约束）。

## 风险与注意点

- **模板升级维护**：模板绑定 tauri-cli-v2.11.4，升级 CLI 必须重新复制并重放 5 处补丁（文件头注释清单缓解）。
- **HKCU 与提权**：MSI 为 perMachine 安装，注册表值写入运行安装进程（提权）用户的 HKCU hive；单管理员场景成立，README 说明。
- **静默安装**：NSIS 静默跳过页面仅认 `/DATADIR=`；MSI 本就靠属性。
- **范围边界**：WebView2 缓存不随 DataDir（仍 LOCALAPPDATA）；卸载不删除所选数据目录中的数据库文件（避免误删用户数据）。

## 验证步骤

1. `npm run tauri:build` 构建通过（同时验证 NSIS 模板编译与 WiX candle/light fragment 链接）。
2. **exe 安装**：向导目录页后出现新页，默认值 `%APPDATA%\com.selfpilot.desktop`；选 `D:\MyData` 安装 → `reg query HKCU\Software\com.selfpilot.desktop /v DataDir` 有值 → 启动应用，确认 `D:\MyData\selfpilot.db` 与 `D:\MyData\logs\selfpilot.log` 生成。
3. **升级安装**：重跑 exe，页面预填旧值。
4. **取消安装**：填路径后取消 → 注册表值不变。
5. **MSI 属性安装**：`msiexec /i SelfPilot_x64.msi DATADIR="D:\MyData" /l*v msi.log` → 注册表+数据文件验证；不带 DATADIR 安装 → 无注册表值、走默认目录。
6. **重置**：`reg delete HKCU\Software\com.selfpilot.desktop /v DataDir /f` → 应用回默认目录。
7. **便携回归**：exe 同级放 `portable.flag` → 数据落 `exe同级/data`（优先级高于注册表）；卸载重装 → 路径选择保留。
