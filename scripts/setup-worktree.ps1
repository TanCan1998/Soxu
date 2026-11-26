$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
Push-Location $repoRoot
try {
  & pwsh -NoProfile -File (Join-Path $PSScriptRoot "setup-hooks.ps1")
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

  $miseCmd = Get-Command mise -ErrorAction SilentlyContinue
  if ($miseCmd) {
    & $miseCmd.Source trust $repoRoot
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    Write-Host "✓ 已信任当前工作树 mise.toml" -ForegroundColor Green
  } else {
    Write-Warning "未检测到 mise 命令，跳过 mise trust。"
  }
} finally {
  Pop-Location
}
