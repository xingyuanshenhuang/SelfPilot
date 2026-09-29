# NSIS 安装向导：数据库路径交互式选择 + 架构感知默认建议

## Context

数据库路径选择功能目前仅存在于 NSIS 安装向导页 [PageDataDir](file:///d:/Desktop/SelfPilot/src-tauri/nsis/installer.nsi#L406-L450)：使用 `${NSD_CreateDirRequest}`（编辑框+内置浏览按钮），默认建议固定为 `$APPDATA\${BUNDLEID}`。用户需求：

1. **交互式选择 + 路径预览**：点击浏览按钮后弹出标准文件夹树对话框（安装程序常见样式，SHBrowseForFolder）直观浏览文件系统；可手动输入；界面实时显示当前选中的路径。
2. **架构感知默认建议**：自动识别 32/64/ARM64 位操作系统，默认建议路径 = 应用安装目录（`$INSTDIR`，安装器已按架构解析为正确的 Program Files / Program Files (x86) / 用户目录），与运行时默认（exe 同级）完全一致。

已确认决策：仅改安装向导（MSI 保持 DATADIR 参数、无 UI）；默认建议 = 安装目录；标准浏览对话框。

关键事实：`$INSTDIR` 在 `.onInit` 中已按 `${RunningX64}` 等解析完成（[installer.nsi L556-L585](file:///d:/Desktop/SelfPilot/src-tauri/nsis/installer.nsi#L556-L585)），页面出现时即为架构正确路径；x64.nsh 已 include（模板 L23），提供 `${RunningX64}` / `${RunningARM64}`。

## 修改文件

### 1. src-tauri/nsis/installer.nsi（仅扩展现有 SelfPilot-DATADIR 补丁区）

**Var 区**（`Var DataDirHwnd` 后）追加：
```nsi
Var DataDirBrowseBtn   ; 浏览按钮
Var DataDirHintHwnd    ; 顶部提示标签（运行时按架构填充）
Var DataDirPreviewHwnd ; 底部预览标签（实时显示选中路径）
Var DataDirArch        ; 检测到的架构文本
```

**`PageDataDir` 重写**（替换原 nsDialogs 部分）：
1. 跳过逻辑、`/DATADIR=` 与注册表读取逻辑保持不变。
2. **默认建议改为安装目录**：末级回退由 `StrCpy $DataDir "$APPDATA\${BUNDLEID}"` 改为 `StrCpy $DataDir "$INSTDIR"`。
3. **架构检测**（显式识别，用于提示文案）：
   ```nsi
   ${If} ${RunningARM64}
     StrCpy $DataDirArch "ARM64"
   ${ElseIf} ${RunningX64}
     StrCpy $DataDirArch "64"
   ${Else}
     StrCpy $DataDirArch "32"
   ${EndIf}
   ```
4. **布局**（nsDialogs::Create 1018 后）：
   - 顶部提示标签 `$DataDirHintHwnd`（0 0 100% 24u），初始文本动态拼接：`"$(dataDirPageHint)（检测到 $DataDirArch 位系统）"`——用 `${NSD_SetText}` 拼接（保持 LangString 静态部分，架构动态部分运行时拼接）。
   - 编辑框 `${NSD_CreateText} 0 26u -40u 13u "$DataDir"` → `$DataDirText`（可手动输入）。
   - 浏览按钮 `${NSD_CreateBrowseButton} -35u 26u 35u 13u "$(dataDirBrowse)"` → `$DataDirBrowseBtn`，`${NSD_OnClick} $DataDirBrowseBtn PageDataDirBrowse`。
   - 底部预览标签 `$DataDirPreviewHwnd`（0 50u 100% 20u），初始调用更新函数填充。
   - `${NSD_OnChange} $DataDirText PageDataDirTextChanged`（键入时实时更新预览）。
   - `${NSD_SetFocus} $DataDirText` 后 `nsDialogs::Show`。

**新增函数**（PageDataDir 之后）：
```nsi
Function PageDataDirBrowse        ; 浏览按钮：打开标准文件夹树对话框
  ${NSD_GetText} $DataDirText $DataDir
  nsDialogs::SelectFolderDialog "$(dataDirBrowseTitle)" "$DataDir"
  Pop $0
  ${If} $0 != "error"
    StrCpy $DataDir $0
    ${NSD_SetText} $DataDirText $DataDir
    Call PageDataDirUpdatePreview
  ${EndIf}
FunctionEnd

Function PageDataDirTextChanged   ; 编辑框键入实时刷新预览
  Call PageDataDirUpdatePreview
FunctionEnd

Function PageDataDirUpdatePreview ; 归一化路径并刷新预览标签
  ${NSD_GetText} $DataDirText $DataDir
  ; 去末尾反斜杠（保留盘根 C:\），复用 PageLeaveDataDir 中的长度判断逻辑
  StrLen $0 $DataDir
  ${If} $0 > 3
    StrCpy $1 $DataDir 1 -1
    ${If} $1 == "\"
      StrCpy $DataDir $DataDir -1
    ${EndIf}
  ${EndIf}
  ${NSD_SetText} $DataDirPreviewHwnd "$(dataDirPreviewPrefix)$DataDir\selfpilot.db"
FunctionEnd
```
`PageLeaveDataDir` 保持不变（取值→去尾反斜杠）。

说明：由 `${NSD_CreateDirRequest}`（依赖内置浏览行为）改为显式"编辑框+浏览按钮"，保证点击浏览按钮必定弹出标准文件夹树对话框，并在选择后回填编辑框+刷新预览，行为确定、可验证。预览标签实时显示 `数据库文件将保存到：<路径>\selfpilot.db`。

### 2. src-tauri/nsis/SimpChinese.nsh（追加 LangString）

```nsi
LangString dataDirBrowse ${LANG_SIMPCHINESE} "浏览..."
LangString dataDirBrowseTitle ${LANG_SIMPCHINESE} "选择数据库存储文件夹"
LangString dataDirPreviewPrefix ${LANG_SIMPCHINESE} "数据库文件将保存到："
```
（现有 `dataDirPageTitle/dataDirPageSubtitle/dataDirPageHint` 保留；`dataDirPageHint` 文案更新为"请选择数据库存储位置，默认使用应用安装目录"。）

### 3. 文档（README L221-234）

"安装时自定义数据库存储路径"章节补充：安装向导页默认建议 = 应用安装目录（自动按 32/64 位系统识别），浏览按钮可打开标准文件夹对话框实时预览路径。

## 不做的事

- 不改运行时 [portable.rs](file:///d:/Desktop/SelfPilot/src-tauri/src/portable.rs)（默认=exe 同级已实现，与安装器建议一致）。
- 不加应用内设置界面（用户已选仅安装向导）。
- 不改 MSI（无 UI，DATADIR 参数维持现状）。

## 风险与注意

- **NSIS 宏可用性**：`${NSD_CreateBrowseButton}`、`nsDialogs::SelectFolderDialog` 为 nsDialogs 标准插件命令；`${RunningARM64}` 需 NSIS ≥ 3.0.5（Tauri 内置 NSIS 3.x，满足）。编译期（makensis）即可校验，构建失败立即可见。
- **动态文案**：`$DataDirArch` 拼接仅用于提示标签，LangString 静态部分不变，保持简体中文编码（UTF-8 BOM 文件已满足）。
- **实时预览**：`NSD_OnChange` 随键入触发；浏览选择后由 Browse 函数手动刷新，保证"选中后立即更新"。

## 验证

1. `npm run tauri:build`：makensis 编译通过 → 生成 `SelfPilot_0.1.0_x64-setup.exe`（无报错即证明宏/函数语法正确）。
2. 运行 exe 安装向导到"数据库存储位置"页，人工验证：
   - 默认建议为安装目录（64 位系统提示含"64"）；
   - 点"浏览..."弹出文件夹树对话框，选择后编辑框与预览标签即时更新；
   - 手动输入任意路径，预览标签随键入实时变化；
   - 保持默认安装 → 注册表 `HKCU\Software\com.selfpilot.desktop\DataDir` 写入安装目录；启动应用数据库落 exe 同级。
3. 升级安装回归：页面预填旧注册表值；静默安装 `/S /DATADIR=` 仍生效（跳过页面逻辑未变）。
