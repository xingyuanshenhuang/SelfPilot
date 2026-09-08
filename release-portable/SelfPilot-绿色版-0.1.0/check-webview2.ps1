<#
  检测系统是否已安装 Microsoft Edge WebView2 Runtime

  用法:
    双击运行
    或: powershell -ExecutionPolicy Bypass -File .\check-webview2.ps1

  退出码:
    0 = 已安装
    1 = 未安装
#>
$ErrorActionPreference = "Stop"

# WebView2 Runtime 的注册表客户端 GUID（分别对应 WOW6432Node 与原生路径）
$keys = @(
  "HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}",
  "HKLM:\SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"
)

$installed = $false
foreach ($key in $keys) {
  if (Test-Path $key) {
    $pv = (Get-ItemProperty -Path $key -ErrorAction SilentlyContinue).pv
    if ($pv) {
      Write-Host "已检测到 WebView2 Runtime，版本: $pv"
      $installed = $true
      break
    }
  }
}

if (-not $installed) {
  Write-Host "未检测到 WebView2 Runtime。" -ForegroundColor Yellow
  Write-Host "SelfPilot（绿色版）依赖该组件才能运行。"
  Write-Host "请从以下页面下载安装 Evergreen 版（建议选择管理员下载模式或离线安装包）："
  Write-Host "  https://developer.microsoft.com/microsoft-edge/webview2/"
  Write-Host ""
  exit 1
}

exit 0