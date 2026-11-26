$ErrorActionPreference = "Stop"

function Invoke-Step([string]$Title, [scriptblock]$Block) {
  Write-Host "==> $Title" -ForegroundColor Cyan
  & $Block
  if ($LASTEXITCODE -ne 0) {
    Write-Error "❌ 步骤失败 (exit $LASTEXITCODE): $Title"
    exit $LASTEXITCODE
  }
}

Invoke-Step "[1/7] 校验 Git Emoji 提交历史与隐私安全..." {
  pwsh -NoProfile -File scripts/verify-commits-and-privacy.ps1 -Mode CiCheck
}

Invoke-Step "[2/7] 校验双端设计令牌同源无漂移 (tokens.json -> CSS)..." {
  node scripts/sync-design-tokens.mjs --check
}

Invoke-Step "[3/7] 校验设计规范完整性、原型语法与零 Emoji UI 纪律..." {
  node scripts/verify-design-system.mjs
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
  pwsh -NoProfile -File scripts/test-paseo-orchestration.ps1
}

Invoke-Step "[4/7] 检查 Rust 代码格式 (cargo fmt)..." {
  cargo fmt --all -- --check
}

Invoke-Step "[5/7] 运行 Clippy 静态检查..." {
  cargo clippy --workspace --all-targets -- -D warnings
}

Invoke-Step "[6/7] 运行 Rust 全工作区测试..." {
  cargo test --workspace
}

if (Test-Path "apps/mobile-web/package.json") {
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
}

Write-Host "✓ 全部门禁检查通过！" -ForegroundColor Green
