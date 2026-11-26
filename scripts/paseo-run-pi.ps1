param(
  [Parameter(Mandatory)][string]$WorkspaceId,
  [Parameter(Mandatory)][string]$WorkspacePath,
  [Parameter(Mandatory)][string]$Prompt,
  [string]$Model = "magpie/group/gpt-6.2-sol",
  [string]$Thinking = "high",
  [string]$Title,
  [string]$TargetDir,
  [string[]]$ExtraEnv = @(),
  [string]$ControlScript = (Join-Path $PSScriptRoot "paseo-ctl.ps1")
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path -LiteralPath $WorkspacePath -PathType Container)) {
  throw "目标工作区目录不存在：$WorkspacePath"
}

$resolvedWorkspace = (Resolve-Path -LiteralPath $WorkspacePath).Path
$miseCmd = Get-Command mise -ErrorAction Stop

# 1. 确保目标工作树已信任 mise.toml，避免 mise exec / shim 拒绝加载或回退全局配置
& $miseCmd.Source trust $resolvedWorkspace | Out-Null
if ($LASTEXITCODE -ne 0) {
  throw "对目标工作区执行 mise trust 失败：$resolvedWorkspace"
}

# 2. 按项目 mise.toml 导出干净环境（屏蔽无关全局工具）
$env:MISE_DISABLE_TOOLS = "python,java,maven"
$miseEnvJson = (& $miseCmd.Source -C $resolvedWorkspace env --json 2>&1)
if ($LASTEXITCODE -ne 0) {
  throw "读取目标工作区 mise env --json 失败：$($miseEnvJson | Out-String)"
}
$miseEnv = ($miseEnvJson | Out-String) | ConvertFrom-Json -ErrorAction Stop

$envPairs = [System.Collections.Generic.List[string]]::new()
$envPairs.Add("MISE_DISABLE_TOOLS=python,java,maven")
$envPairs.Add("PNPM_CONFIG_MANAGE_PACKAGE_MANAGER_VERSIONS=false")

$allowedKeys = @(
  "CARGO_HOME",
  "RUSTUP_HOME",
  "RUSTUP_TOOLCHAIN",
  "RUSTUP_DIST_SERVER",
  "RUSTUP_UPDATE_ROOT",
  "CARGO_TARGET_DIR",
  "PNPM_HOME",
  "GOPROXY",
  "Path"
)

foreach ($key in $allowedKeys) {
  $prop = $miseEnv.PSObject.Properties[$key]
  if ($prop -and -not [string]::IsNullOrWhiteSpace([string]$prop.Value)) {
    $value = [string]$prop.Value
    if ($key -eq "CARGO_TARGET_DIR" -and -not [string]::IsNullOrWhiteSpace($TargetDir)) {
      $value = $TargetDir
    }
    $envPairs.Add("$key=$value")
  }
}

if (-not [string]::IsNullOrWhiteSpace($TargetDir) -and -not ($miseEnv.PSObject.Properties["CARGO_TARGET_DIR"])) {
  $envPairs.Add("CARGO_TARGET_DIR=$TargetDir")
}

foreach ($item in $ExtraEnv) {
  if (-not [string]::IsNullOrWhiteSpace($item)) {
    $envPairs.Add($item)
  }
}

$runArgs = @(
  "run",
  "--no-wait",
  "--json",
  "--workspace", $WorkspaceId,
  "--provider", "pi",
  "--model", $Model,
  "--thinking", $Thinking
)

if (-not [string]::IsNullOrWhiteSpace($Title)) {
  $runArgs += @("--title", $Title)
}

foreach ($pair in $envPairs) {
  $runArgs += @("--env", $pair)
}

$runArgs += $Prompt

& pwsh -NoProfile -File $ControlScript @runArgs
exit $LASTEXITCODE
