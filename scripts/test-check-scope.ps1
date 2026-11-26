$ErrorActionPreference = "Stop"

$detectScript = Join-Path $PSScriptRoot "detect-check-scope.ps1"
$passed = 0

function Assert-Condition([bool]$Condition, [string]$Title) {
  if (-not $Condition) {
    throw "❌ 作用域检测回归失败：$Title"
  }
  $script:passed++
  Write-Host "通过：$Title" -ForegroundColor Green
}

function Invoke-Detect([string[]]$Paths, [switch]$Full) {
  $params = @{
    ChangedFiles = $Paths
  }
  if ($Full) { $params.Full = $true }
  return (& $detectScript @params)
}

# 1. 纯文档与 AOCI 资产变更仅跑治理与隐私门禁
$docs = Invoke-Detect @("AGENTS.md", "docs/ops/LESSONS.md", "aoci.code.txt", ".aoci/baseline.json")
Assert-Condition ($docs.governance -and (-not $docs.rust) -and $docs.rustScope -eq "none" -and (-not $docs.mobileWeb) -and (-not $docs.paseoOrchestration)) "纯文档与 AOCI 变更跳过 Rust 与 PWA 全仓检查"

# 2. 单改 soxu-git 应展开反向依赖闭包至 soxu-daemon，且不触发 PWA
$git = Invoke-Detect @("crates/soxu-git/src/diff.rs")
Assert-Condition ($git.rust -and $git.rustScope -eq "crates" -and ($git.rustPackages -contains "soxu-git") -and ($git.rustPackages -contains "soxu-daemon") -and (-not $git.mobileWeb) -and ($git.rustPackageArgs -eq "-p soxu-daemon -p soxu-git")) "soxu-git 变更自动覆盖反向依赖 soxu-daemon 且跳过 PWA"

# 3. 仅改顶层二进制/守护进程 soxu-daemon 不回扫底层 soxu-git
$daemon = Invoke-Detect @("crates/soxu-daemon/src/main.rs")
Assert-Condition ($daemon.rust -and $daemon.rustScope -eq "crates" -and ($daemon.rustPackages -contains "soxu-daemon") -and (-not ($daemon.rustPackages -contains "soxu-git")) -and (-not $daemon.mobileWeb)) "顶层 soxu-daemon 变更不回跑上游独立 crates"

# 4. 修改协议基础层 soxu-protocol 传递覆盖全部工作区 crates；修改中间层 soxu-magpie 仅展开其消费方
$proto = Invoke-Detect @("crates/soxu-protocol/src/lib.rs")
Assert-Condition ($proto.rust -and $proto.rustScope -eq "workspace" -and $proto.rustPackages.Count -eq 8 -and ($proto.rustPackages -contains "soxu-git")) "soxu-protocol 变更传递展开并自动升级为整工作区 8 crates"

$magpie = Invoke-Detect @("crates/soxu-magpie/src/lib.rs")
Assert-Condition ($magpie.rust -and $magpie.rustScope -eq "crates" -and ($magpie.rustPackages -contains "soxu-magpie") -and ($magpie.rustPackages -contains "soxu") -and ($magpie.rustPackages -contains "soxu-daemon") -and (-not ($magpie.rustPackages -contains "soxu-git")) -and (-not ($magpie.rustPackages -contains "soxu-core"))) "soxu-magpie 变更精确展开至 soxu 与 soxu-daemon 而不波及 git/core"

# 5. 前端与设计令牌变更仅触发 mobileWeb，不触发 Rust
$web = Invoke-Detect @("design-system/soxu/tokens.json", "apps/mobile-web/src/App.tsx")
Assert-Condition ((-not $web.rust) -and $web.mobileWeb -and (-not $web.paseoOrchestration)) "PWA 与设计令牌变更跳过 Rust 构建测试"

# 6. Paseo 编排脚本与技能变更仅触发 paseoOrchestration 自测
$paseo = Invoke-Detect @("scripts/paseo-dispatch.ps1", ".agents/skills/paseo-orchestrator/SKILL.md")
Assert-Condition ((-not $paseo.rust) -and (-not $paseo.mobileWeb) -and $paseo.paseoOrchestration) "Paseo 编排变更精准命中 paseo-orchestration 自测"

# 7. 根 Cargo.toml 触发 workspace Rust；工作流/门禁脚本或 -Full 触发全量
$cargoRoot = Invoke-Detect @("Cargo.toml")
Assert-Condition ($cargoRoot.rust -and $cargoRoot.rustScope -eq "workspace" -and (-not $cargoRoot.mobileWeb)) "根 Cargo.toml 变更升级为整工作区 Rust 检查"

$ciSelf = Invoke-Detect @(".github/workflows/ci.yml")
Assert-Condition ($ciSelf.full -and $ciSelf.rust -and $ciSelf.rustScope -eq "workspace" -and $ciSelf.mobileWeb -and $ciSelf.paseoOrchestration -and $ciSelf.checkScopeSelfTest) "修改 CI 工作流自身回退全量检查"

$explicitFull = Invoke-Detect @("README.md") -Full
Assert-Condition ($explicitFull.full -and $explicitFull.rust -and $explicitFull.rustScope -eq "workspace" -and $explicitFull.mobileWeb) "-Full 参数强制触发全量检查"

# 8. 外部 pwsh -File CLI 逗号传入多文件也应完整识别
$cliRaw = (& pwsh -NoProfile -File $detectScript -ChangedFiles "AGENTS.md,docs/ops/LESSONS.md" -Json | Out-String)
if ($LASTEXITCODE -ne 0) { throw "外部 CLI 调用 detect-check-scope.ps1 失败" }
$cliObj = $cliRaw | ConvertFrom-Json -ErrorAction Stop
Assert-Condition ($cliObj.changedFiles.Count -eq 2 -and (-not $cliObj.rust) -and (-not $cliObj.mobileWeb)) "pwsh -File 逗号传参兼容多个变更路径"

# 9. 发版编排与编码标准变更仅跑治理隐私门禁，不拖累 Rust 与 PWA
$releaseDocs = Invoke-Detect @(".github/workflows/release.yml", "docs/ops/CODE_STANDARDS.md")
Assert-Condition ($releaseDocs.governance -and (-not $releaseDocs.rust) -and (-not $releaseDocs.mobileWeb)) "release.yml 与 CODE_STANDARDS.md 变更跳过无关 Rust 与 PWA 构建"

Write-Host "✓ 共 $passed 项作用域检测回归全部通过！" -ForegroundColor Green
exit 0
