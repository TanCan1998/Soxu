param(
  [ValidateSet("Auto", "Full")]
  [string]$Scope = "Auto",
  [switch]$Full,
  [string]$BaseRef = ""
)

$ErrorActionPreference = "Stop"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$OutputEncoding = [System.Text.Encoding]::UTF8

function Invoke-Step([string]$Title, [scriptblock]$Block) {
  Write-Host "==> $Title" -ForegroundColor Cyan
  & $Block
  if ($LASTEXITCODE -ne 0) {
    Write-Error "❌ 步骤失败 (exit $LASTEXITCODE): $Title"
    exit $LASTEXITCODE
  }
}

$detectArgs = @("-NoProfile", "-File", (Join-Path $PSScriptRoot "detect-check-scope.ps1"), "-IncludeWorkingTree", "-Json")
if ($Full -or $Scope -eq "Full") {
  $detectArgs += "-Full"
}
if (-not [string]::IsNullOrWhiteSpace($BaseRef)) {
  $detectArgs += @("-BaseRef", $BaseRef)
}

$scopeRaw = (& pwsh @detectArgs | Out-String)
if ($LASTEXITCODE -ne 0) {
  Write-Error "❌ 变更范围分析失败，退出码 $LASTEXITCODE"
  exit $LASTEXITCODE
}
$plan = $scopeRaw | ConvertFrom-Json -ErrorAction Stop

$rustSummary = if ($plan.rust) {
  if ($plan.rustScope -eq "workspace") { "workspace" } else { "$($plan.rustPackages -join ',')" }
} else {
  "跳过"
}
$planMode = if ($plan.PSObject.Properties["mode"]) { $plan.mode } else { "legacy" }
$planPlatforms = if ($plan.PSObject.Properties["requiredPlatforms"]) { @($plan.requiredPlatforms) -join ',' } else { "" }
Write-Host ("==> 门禁范围：full={0} | mode={1} | files={2} | rust={3} | mobileWeb={4} | paseo={5} | platforms={6}" -f $plan.full, $planMode, $plan.changedFiles.Count, $rustSummary, $plan.mobileWeb, $plan.paseoOrchestration, $planPlatforms) -ForegroundColor DarkCyan
# 变更路径只用于追溯；无法归因或回退时把原因全部打出来，避免“跳过了但没人知道为什么”。
if ($plan.PSObject.Properties["fallbackReasons"] -and @($plan.fallbackReasons).Count -gt 0) {
  Write-Host ("==> 保守回退原因：{0}" -f (@($plan.fallbackReasons) -join ' ; ')) -ForegroundColor Yellow
}

Invoke-Step "[1/7] 校验 Git Emoji 提交历史与隐私安全..." {
  pwsh -NoProfile -File scripts/verify-commits-and-privacy.ps1 -Mode CiCheck
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
  if ($plan.checkScopeSelfTest) {
    pwsh -NoProfile -File scripts/test-check-scope.ps1
  }
  # CI-D：工具链版本的唯一真相源是 mise.toml。读取脚本或工作流变更时必须跑它的回归矩阵，
  # 否则 CI 可能悄悄退回已安装版本或硬编码常量，而本地门禁看不出来。
  $toolchainPlan = if ($plan.PSObject.Properties["changedFiles"]) { @($plan.changedFiles) } else { @() }
  $toolchainTouched = @($toolchainPlan | Where-Object {
      $_ -eq "mise.toml" -or
      $_ -eq "scripts/read-mise-versions.ps1" -or
      $_ -eq "scripts/test-read-mise-versions.ps1" -or
      $_ -eq ".github/workflows/ci.yml"
    }).Count -gt 0
  if ($plan.full -or $toolchainTouched) {
    pwsh -NoProfile -File scripts/test-read-mise-versions.ps1
  }
}

if ($plan.mobileWeb) {
  Invoke-Step "[2/7] 校验双端设计令牌同源无漂移 (tokens.json -> CSS)..." {
    node scripts/sync-design-tokens.mjs --check
  }
} else {
  Write-Host "==> [2/7] 跳过设计令牌同源校验（未触及前端或设计系统）" -ForegroundColor DarkGray
}

if ($plan.mobileWeb -or $plan.paseoOrchestration) {
  Invoke-Step "[3/7] 校验设计规范完整性 / Paseo 编排回归..." {
    if ($plan.mobileWeb) {
      node scripts/verify-design-system.mjs
      if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    }
    if ($plan.paseoOrchestration) {
      pwsh -NoProfile -File scripts/test-paseo-orchestration.ps1
    }
  }
} else {
  Write-Host "==> [3/7] 跳过设计规范与 Paseo 编排回归（均未触及）" -ForegroundColor DarkGray
}

if ($plan.rust) {
  Invoke-Step "[4/7] 检查 Rust 代码格式 (cargo fmt)..." {
    cargo fmt --all -- --check
  }

  # 包列表以结构化参数数组传递，不再把字符串再拆回令牌；统一加 --locked 保锁文件纪律。
  $pkgArgs = [System.Collections.Generic.List[string]]::new()
  foreach ($pkg in @($plan.rustPackages)) {
    if ([string]::IsNullOrWhiteSpace($pkg)) { continue }
    $pkgArgs.Add("-p")
    $pkgArgs.Add([string]$pkg)
  }

  if ($plan.rustScope -eq "workspace") {
    Invoke-Step "[5/7] 运行全工作区 Clippy 静态检查..." {
      cargo clippy --workspace --all-targets --locked -- -D warnings
    }
    Invoke-Step "[6/7] 运行全工作区 Rust 测试..." {
      cargo test --workspace --locked
    }
  } elseif ($pkgArgs.Count -gt 0) {
    Invoke-Step "[5/7] 运行增量 Clippy 检查 ($($plan.rustPackages -join ', '))..." {
      cargo clippy @pkgArgs --all-targets --locked -- -D warnings
    }
    Invoke-Step "[6/7] 运行增量 Rust 测试 ($($plan.rustPackages -join ', '))..." {
      cargo test @pkgArgs --locked
    }
  } else {
    Write-Error "❌ 检测器给出 crates 范围但包列表为空，拒绝误跳过 Clippy 与测试"
    exit 1
  }
} else {
  Write-Host "==> [4/7]-[6/7] 跳过 Rust 格式、Clippy 与测试（未触及 Cargo/Crate 源码）" -ForegroundColor DarkGray
}

if ($plan.mobileWeb -and (Test-Path "apps/mobile-web/package.json")) {
  Push-Location "apps/mobile-web"
  try {
    Invoke-Step "[7/7] 检查并构建移动端 Web/PWA 与单文件桌面预览..." {
      pnpm run build
      if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
      pnpm run build:preview
    }
  } finally {
    Pop-Location
  }
} else {
  Write-Host "==> [7/7] 跳过移动端 Web/PWA 构建（未触及前端或设计系统）" -ForegroundColor DarkGray
}

Write-Host "✓ 所选范围门禁检查全部通过！" -ForegroundColor Green
