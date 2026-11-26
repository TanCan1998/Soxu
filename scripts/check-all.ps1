param(
  [ValidateSet("Auto", "Full")]
  [string]$Scope = "Auto",
  [switch]$Full,
  [string]$BaseRef = ""
)

$ErrorActionPreference = "Stop"

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
Write-Host ("==> 门禁范围：full={0} | files={1} | rust={2} | mobileWeb={3} | paseo={4}" -f $plan.full, $plan.changedFiles.Count, $rustSummary, $plan.mobileWeb, $plan.paseoOrchestration) -ForegroundColor DarkCyan

Invoke-Step "[1/7] 校验 Git Emoji 提交历史与隐私安全..." {
  pwsh -NoProfile -File scripts/verify-commits-and-privacy.ps1 -Mode CiCheck
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
  if ($plan.checkScopeSelfTest) {
    pwsh -NoProfile -File scripts/test-check-scope.ps1
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

  $pkgArgs = @($plan.rustPackageArgs -split '\s+' | Where-Object { $_ })

  if ($plan.rustScope -eq "workspace") {
    Invoke-Step "[5/7] 运行全工作区 Clippy 静态检查..." {
      cargo clippy --workspace --all-targets -- -D warnings
    }
    Invoke-Step "[6/7] 运行全工作区 Rust 测试..." {
      cargo test --workspace
    }
  } else {
    Invoke-Step "[5/7] 运行增量 Clippy 检查 ($($plan.rustPackages -join ', '))..." {
      cargo clippy @pkgArgs --all-targets -- -D warnings
    }
    Invoke-Step "[6/7] 运行增量 Rust 测试 ($($plan.rustPackages -join ', '))..." {
      cargo test @pkgArgs
    }
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
