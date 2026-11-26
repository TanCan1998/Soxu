$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
Push-Location $repoRoot
try {
  Write-Host "==> mise current" -ForegroundColor Cyan
  mise current
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

  Write-Host "==> 工具链版本核对" -ForegroundColor Cyan
  rustc --version
  cargo --version
  cargo fmt --version
  cargo clippy --version
  node -v
  pnpm -v

  foreach ($tool in @("gitleaks", "gh", "git", "codegraph", "pi")) {
    $cmd = Get-Command $tool -ErrorAction SilentlyContinue
    if ($cmd) {
      Write-Host ("  [ok] {0}: {1}" -f $tool, $cmd.Name)
    } else {
      Write-Host ("  [missing] {0}" -f $tool) -ForegroundColor Yellow
    }
  }

  $aociCmd = Get-Command aoci -ErrorAction SilentlyContinue
  if ($aociCmd) {
    Write-Host ("  [ok] aoci: {0}" -f $aociCmd.Name)
  } elseif (Test-Path -LiteralPath "D:/Programs/AOCI/aoci.exe" -PathType Leaf) {
    Write-Host "  [ok] aoci: D:/Programs/AOCI/aoci.exe"
  } else {
    Write-Host "  [missing] aoci" -ForegroundColor Yellow
  }

  Write-Host ("  CARGO_TARGET_DIR = {0}" -f $env:CARGO_TARGET_DIR)
  Write-Host ("  PNPM_CONFIG_MANAGE_PACKAGE_MANAGER_VERSIONS = {0}" -f $env:PNPM_CONFIG_MANAGE_PACKAGE_MANAGER_VERSIONS)
} finally {
  Pop-Location
}
