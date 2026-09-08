<#
  SelfPilot 便携版（绿色版）一键打包脚本

  功能：
    1. 前端构建（npm ci + npm run build）
    2. 后端编译（cargo build --release）
    3. 组装绿色版目录（SelfPilot.exe + portable.flag + 说明 + 环境检测 + version.txt）
    4. 生成 zip 发布包与 SHA-256 校验和

  用法：
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts/portable/build-portable.ps1
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts/portable/build-portable.ps1 -Version "0.1.0" -SkipFrontend

  参数：
    -Version       发布版本号；缺省时从 src-tauri/tauri.conf.json 读取
    -SkipFrontend  跳过 5.1 前端构建（复用已有 dist/），仅编译后端并打包
#>
#Requires -Version 5.1
param(
  [string]$Version = "",
  [switch]$SkipFrontend
)
$ErrorActionPreference = "Stop"

# 用 node 直接调用 npm-cli.js，绕开 npm.ps1/npm.cmd 垫片在嵌套环境下的参数错乱
# （例如 `npm run build:portable` 内再调 `npm ci` 时 npm.ps1 会把 `ci` 误解析为 `pm`）
function Invoke-Npm {
  param([Parameter(ValueFromRemainingArguments = $true)][string[]]$NpmArgs)
  $cli = ""
  $shim = (Get-Command npm -ErrorAction SilentlyContinue).Source
  if ($shim) { $cli = Join-Path (Split-Path -Parent $shim) "node_modules\npm\bin\npm-cli.js" }
  if ($cli -and (Test-Path $cli)) {
    & node $cli @NpmArgs
  } else {
    & npm.cmd @NpmArgs
  }
}

# 仓库根目录 = 脚本目录上两级（scripts/portable -> 仓库根）
$repoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$srcTauri  = Join-Path $repoRoot "src-tauri"
$releaseOut = Join-Path $repoRoot "release-portable"

if (-not $Version) {
  $tauriConf = Get-Content -Raw (Join-Path $srcTauri "tauri.conf.json") | ConvertFrom-Json
  $Version = $tauriConf.version
  if (-not $Version) { $Version = "0.0.0" }
}

# ---------- 定位 release 产物（三级回落） ----------
$targetDir = $env:CARGO_TARGET_DIR
if (-not $targetDir) {
  $cfgPath = Join-Path $srcTauri ".cargo\config.toml"
  if (Test-Path $cfgPath) {
    $cfg = Get-Content $cfgPath
    $m = $cfg | Select-String 'target-dir\s*=\s*"([^"]+)"' | Select-Object -First 1
    if ($m) {
      # TOML 基本字符串中 \\ 为转义后的单反斜杠
      $targetDir = ($m.Matches[0].Groups[1].Value).Replace('\\', '\')
    }
  }
  if (-not $targetDir) { $targetDir = Join-Path $srcTauri "target" }
}
$releaseExe = Join-Path $targetDir "release\self-study-planner.exe"

Write-Host "==> 仓库目录: $repoRoot"
Write-Host "==> 发布版本: $Version"
Write-Host "==> target-dir: $targetDir"

# ---------- 1. 前端构建 ----------
if (-not $SkipFrontend) {
  Write-Host "==> 前端构建（npm ci + npm run build）..."
  Push-Location $repoRoot
  try {
    Invoke-Npm ci
    if ($LASTEXITCODE -ne 0) { throw "npm ci 失败" }
    Invoke-Npm run build
    if ($LASTEXITCODE -ne 0) { throw "npm run build 失败" }
  } finally { Pop-Location }
}

# ---------- 2. 后端编译 ----------
# 必须启用 tauri 的 custom-protocol 特性：它是 dev/prod 的唯一开关，
# 与 `tauri build` 自动传 --features custom-protocol 行为一致。
# 否则会编译成 dev 模式，运行时去加载 devUrl(localhost:1420) 而非内嵌前端资源。
Write-Host "==> 后端编译（cargo build --release，启用 custom-protocol）..."
Push-Location $srcTauri
try {
  & cargo build --release --package self-study-planner --features tauri/custom-protocol
  if ($LASTEXITCODE -ne 0) { throw "cargo build --release 失败" }
} finally { Pop-Location }

if (-not (Test-Path $releaseExe)) {
  throw "构建结束后仍未找到产物: $releaseExe"
}

# ---------- 3. 组装绿色版目录 ----------
$greenName = "SelfPilot-绿色版-$Version"
$greenDir = Join-Path $releaseOut $greenName
if (Test-Path $greenDir) { Remove-Item $greenDir -Recurse -Force }
New-Item -ItemType Directory -Path $greenDir -Force | Out-Null

Write-Host "==> 组装绿色版目录: $greenDir"

Copy-Item $releaseExe (Join-Path $greenDir "SelfPilot.exe") -Force

# 复制运行所需依赖（tauri 若生成 *.resources 目录则一并携带）
Get-ChildItem (Split-Path $releaseExe) -Directory -Filter "*.resources" -ErrorAction SilentlyContinue | ForEach-Object {
  Copy-Item $_.FullName $greenDir -Recurse -Force
}

# portable.flag：便携模式触发器
New-Item -ItemType File -Path (Join-Path $greenDir "portable.flag") -Force | Out-Null

# 说明与环境检测模板
Copy-Item (Join-Path $PSScriptRoot "启动说明.txt") $greenDir -Force
Copy-Item (Join-Path $PSScriptRoot "check-webview2.ps1") $greenDir -Force

# version.txt（UTF-8 BOM）
$commit = ""
try {
  $commit = (& git -C $repoRoot rev-parse --short HEAD 2>$null).Trim()
} catch { }
$buildTime = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
$verContent = "SelfPilot $Version`r`n构建时间: $buildTime`r`n提交: $commit`r`n"
[System.IO.File]::WriteAllText(
  (Join-Path $greenDir "version.txt"),
  $verContent,
  (New-Object System.Text.UTF8Encoding($true))
)

# ---------- 4. 生成 zip 发布包 ----------
$zipPath = Join-Path $releaseOut "$greenName.zip"
if (Test-Path $zipPath) { Remove-Item $zipPath -Force }
Write-Host "==> 压缩发布包: $zipPath"
Compress-Archive -Path $greenDir -DestinationPath $zipPath -CompressionLevel Optimal

# ---------- 5. SHA-256 校验和 ----------
$hash = (Get-FileHash $zipPath -Algorithm SHA256).Hash
$sumPath = Join-Path $releaseOut "校验和.txt"
[System.IO.File]::WriteAllText(
  $sumPath,
  "$hash  $greenName.zip`r`n",
  (New-Object System.Text.UTF8Encoding($true))
)

Write-Host ""
Write-Host "================ 便携版打包完成 ================"
Write-Host "  目录 : $greenDir"
Write-Host "  压缩 : $zipPath"
Write-Host "  SHA256: $hash"
Write-Host "  校验和: $sumPath"