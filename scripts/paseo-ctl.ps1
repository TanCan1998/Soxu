# Soxu 跨桌面端（AGY / Codex）统一 Paseo CLI 包装器
# 注意：不要添加 [CmdletBinding()] 或 param(...)，以便原样透传 --json / --isolation / --help 等原生参数。

$ErrorActionPreference = "Stop"
$rawArgs = @($args)

function Resolve-PaseoCli {
  if (-not [string]::IsNullOrWhiteSpace($env:PASEO_CLI) -and (Test-Path -LiteralPath $env:PASEO_CLI -PathType Leaf)) {
    return $env:PASEO_CLI
  }

  $cmd = Get-Command paseo -ErrorAction SilentlyContinue
  if ($cmd -and -not [string]::IsNullOrWhiteSpace($cmd.Source)) {
    return $cmd.Source
  }

  $candidates = @(
    "D:/Programs/Scoop/apps/paseo/current/resources/bin/paseo.cmd",
    (Join-Path $HOME "scoop/apps/paseo/current/resources/bin/paseo.cmd")
  )
  if (-not [string]::IsNullOrWhiteSpace($env:LOCALAPPDATA)) {
    $candidates += (Join-Path $env:LOCALAPPDATA "Programs/Paseo/resources/bin/paseo.cmd")
  }
  $candidates += "C:/Program Files/Paseo/resources/bin/paseo.cmd"

  foreach ($candidate in $candidates) {
    if (-not [string]::IsNullOrWhiteSpace($candidate) -and (Test-Path -LiteralPath $candidate -PathType Leaf)) {
      return $candidate
    }
  }
  return $null
}

$paseoCli = Resolve-PaseoCli
if (-not $paseoCli) {
  [Console]::Error.WriteLine("[paseo-ctl] 未找到可用的 Paseo CLI。请确认已通过 Scoop 安装 Paseo，或设置环境变量 `$env:PASEO_CLI 指向 paseo.cmd。")
  exit 127
}

if ($rawArgs.Count -eq 0) {
  $rawArgs = @("status")
}

$subcmd = [string]$rawArgs[0]
$daemonCommands = @("project", "workspace", "ls", "run", "send", "wait", "logs", "stop", "rm", "script", "host")
$hasBypassFlag = $false
foreach ($arg in $rawArgs) {
  $textArg = [string]$arg
  if ($textArg -in @("--host", "--server", "-s", "--help", "-h", "help", "-v", "--version") -or $textArg.StartsWith("--host=") -or $textArg.StartsWith("--server=")) {
    $hasBypassFlag = $true
    break
  }
}

if (($subcmd -in $daemonCommands) -and (-not $hasBypassFlag)) {
  $statusRaw = (& $paseoCli status --json 2>$null | Out-String)
  $daemonRunning = $false
  if (-not [string]::IsNullOrWhiteSpace($statusRaw)) {
    try {
      $statusObj = $statusRaw | ConvertFrom-Json -ErrorAction Stop
      if ($statusObj.localDaemon -eq "running") {
        $daemonRunning = $true
      }
    } catch {
      $daemonRunning = $false
    }
  }

  if (-not $daemonRunning) {
    [Console]::Error.WriteLine("[paseo-ctl] 检测到本地 Paseo 守护进程未运行，正在后台启动...")
    Start-Process -FilePath $paseoCli -ArgumentList @("start", "--quiet") -WindowStyle Hidden | Out-Null
    $ready = $false
    for ($i = 0; $i -lt 30; $i++) {
      Start-Sleep -Milliseconds 400
      $probeRaw = (& $paseoCli status --json 2>$null | Out-String)
      if (-not [string]::IsNullOrWhiteSpace($probeRaw)) {
        try {
          $probeObj = $probeRaw | ConvertFrom-Json -ErrorAction Stop
          if ($probeObj.localDaemon -eq "running") {
            $ready = $true
            break
          }
        } catch {}
      }
    }
    if (-not $ready) {
      [Console]::Error.WriteLine("[paseo-ctl] 警告：等待本地 Paseo 守护进程就绪超时，尝试直接执行命令。")
    }
  }
}

if ($paseoCli -match '[\\/]resources[\\/]bin[\\/]paseo\.cmd$') {
  $resourcesDir = Split-Path -Parent (Split-Path -Parent $paseoCli)
  $appDir = Split-Path -Parent $resourcesDir
  $appExe = Join-Path $appDir "Paseo.exe"
  $runnerJs = Join-Path $resourcesDir "app.asar.unpacked/dist/daemon/node-entrypoint-runner.js"
  $cliJs = Join-Path $resourcesDir "app.asar/node_modules/@getpaseo/cli/dist/index.js"
  if ((Test-Path -LiteralPath $appExe -PathType Leaf) -and (Test-Path -LiteralPath $runnerJs -PathType Leaf)) {
    $env:ELECTRON_RUN_AS_NODE = "1"
    $env:PASEO_NODE_ENV = "production"
    $env:PASEO_CLI = $paseoCli
    & $appExe --disable-warning=DEP0040 $runnerJs node-script $cliJs @rawArgs
    exit $LASTEXITCODE
  }
}

& $paseoCli @rawArgs
exit $LASTEXITCODE
