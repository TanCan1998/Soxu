$ErrorActionPreference = "Stop"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

$detectScript = Join-Path $PSScriptRoot "detect-check-scope.ps1"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$passed = 0
$tempRoot = Join-Path ([IO.Path]::GetTempPath()) ("soxu-scope-" + [Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null

function Assert-Condition([bool]$Condition, [string]$Title) {
  if (-not $Condition) {
    throw "❌ 作用域检测回归失败：$Title"
  }
  $script:passed++
  Write-Host "通过：$Title" -ForegroundColor Green
}

function Invoke-Detect {
  param(
    [string[]]$Paths = @(),
    [switch]$Full,
    [string]$ChangedFilesJson = "",
    [string]$ChangedFilesPathFile = "",
    [string]$RepoRootOverride = "",
    [string]$BaseRef = "",
    [string]$HeadRef = "",
    [switch]$IncludeWorkingTree,
    [string]$EventName = ""
  )
  $params = @{}
  if ($Paths.Count -gt 0) { $params.ChangedFiles = $Paths }
  if ($Full) { $params.Full = $true }
  if (-not [string]::IsNullOrEmpty($ChangedFilesJson)) { $params.ChangedFilesJson = $ChangedFilesJson }
  if (-not [string]::IsNullOrEmpty($ChangedFilesPathFile)) { $params.ChangedFilesPath = $ChangedFilesPathFile }
  if (-not [string]::IsNullOrEmpty($RepoRootOverride)) { $params.RepoRoot = $RepoRootOverride }
  if (-not [string]::IsNullOrEmpty($BaseRef)) { $params.BaseRef = $BaseRef }
  if (-not [string]::IsNullOrEmpty($HeadRef)) { $params.HeadRef = $HeadRef }
  if ($IncludeWorkingTree) { $params.IncludeWorkingTree = $true }
  if (-not [string]::IsNullOrEmpty($EventName)) { $params.Event = $EventName }
  return (& $detectScript @params)
}

function Invoke-GitIn([string]$Directory, [string[]]$Arguments) {
  $output = (& git -C $Directory @Arguments 2>&1 | Out-String)
  if ($LASTEXITCODE -ne 0) {
    throw "git -C $Directory $($Arguments -join ' ') 失败：$output"
  }
  return $output
}

function New-GitFixture([string]$Name) {
  $directory = Join-Path $tempRoot $Name
  New-Item -ItemType Directory -Path $directory -Force | Out-Null
  Invoke-GitIn $directory @("init", "-q", "-b", "main") | Out-Null
  Invoke-GitIn $directory @("config", "user.email", "scope-fixture@example.invalid") | Out-Null
  Invoke-GitIn $directory @("config", "user.name", "Scope Fixture") | Out-Null
  Invoke-GitIn $directory @("config", "commit.gpgsign", "false") | Out-Null
  Invoke-GitIn $directory @("config", "core.autocrlf", "false") | Out-Null
  return $directory
}

function Write-FixtureFile([string]$RootDir, [string]$RelativePath, [string]$Content) {
  $full = Join-Path $RootDir ($RelativePath -replace '/', [IO.Path]::DirectorySeparatorChar)
  $parent = Split-Path $full -Parent
  if (-not [string]::IsNullOrEmpty($parent)) { New-Item -ItemType Directory -Path $parent -Force | Out-Null }
  Set-Content -LiteralPath $full -Value $Content -Encoding utf8 -NoNewline
}

function Remove-FixtureFile([string]$RootDir, [string]$RelativePath) {
  $full = Join-Path $RootDir ($RelativePath -replace '/', [IO.Path]::DirectorySeparatorChar)
  Remove-Item -LiteralPath $full -Force
}

function Commit-Fixture([string]$RootDir, [string]$Message) {
  Invoke-GitIn $RootDir @("add", "-A") | Out-Null
  Invoke-GitIn $RootDir @("commit", "-q", "-m", $Message) | Out-Null
  return (Invoke-GitIn $RootDir @("rev-parse", "HEAD")).Trim()
}

function Assert-ConservativeFull($Plan, [string]$Category, [string]$Title) {
  $reasonText = @($Plan.fallbackReasons) -join " ; "
  Assert-Condition (
    $Plan.full -and
    $Plan.mode -eq "full" -and
    $Plan.rust -and
    $Plan.rustScope -eq "workspace" -and
    $reasonText -like "*structured_input_invalid:$Category*"
  ) "$Title（实际原因：$reasonText）"
}

try {
  # -------------------------------------------------------------------------
  # 零、既有契约：分级门禁与真实工作区反向依赖
  # -------------------------------------------------------------------------

  # 1. 纯文档与 AOCI 资产变更仅跑治理与隐私门禁
  $docs = Invoke-Detect @("AGENTS.md", "docs/ops/LESSONS.md", "aoci.code.txt", ".aoci/baseline.json")
  Assert-Condition ($docs.governance -and (-not $docs.rust) -and $docs.rustScope -eq "none" -and (-not $docs.mobileWeb) -and (-not $docs.paseoOrchestration)) "纯文档与 AOCI 变更跳过 Rust 与 PWA 全仓检查"

  # 2. 单改 soxu-git 应展开反向依赖闭包至 soxu-daemon，且不触发 PWA
  $git = Invoke-Detect @("crates/soxu-git/src/diff.rs")
  Assert-Condition ($git.rust -and $git.rustScope -eq "crates" -and ($git.rustPackages -contains "soxu-git") -and ($git.rustPackages -contains "soxu-daemon") -and (-not $git.mobileWeb) -and ($git.rustPackageArgs -eq "-p soxu-daemon -p soxu-git")) "soxu-git 变更自动覆盖反向依赖 soxu-daemon 且跳过 PWA"

  # 3. 仅改顶层二进制/守护进程 soxu-daemon 不回扫底层 soxu-git
  $daemon = Invoke-Detect @("crates/soxu-daemon/src/main.rs")
  Assert-Condition ($daemon.rust -and $daemon.rustScope -eq "crates" -and ($daemon.rustPackages -contains "soxu-daemon") -and (-not ($daemon.rustPackages -contains "soxu-git")) -and (-not $daemon.mobileWeb)) "顶层 soxu-daemon 变更不回跑上游独立 crates"

  # 4. 修改协议基础层 soxu-protocol 传递覆盖全部工作区 crates
  $proto = Invoke-Detect @("crates/soxu-protocol/src/lib.rs")
  Assert-Condition ($proto.rust -and $proto.rustScope -eq "workspace" -and $proto.rustPackages.Count -eq 8 -and ($proto.rustPackages -contains "soxu-git")) "soxu-protocol 变更传递展开并自动升级为整工作区 8 crates"

  $magpie = Invoke-Detect @("crates/soxu-magpie/src/lib.rs")
  Assert-Condition ($magpie.rust -and $magpie.rustScope -eq "crates" -and ($magpie.rustPackages -contains "soxu-magpie") -and ($magpie.rustPackages -contains "soxu-cli") -and ($magpie.rustPackages -contains "soxu-daemon") -and (-not ($magpie.rustPackages -contains "soxu-git")) -and (-not ($magpie.rustPackages -contains "soxu-core"))) "soxu-magpie 变更精确展开至 soxu-cli 与 soxu-daemon 而不波及 git/core"

  # 5. 前端与设计令牌变更仅触发 mobileWeb，不触发 Rust
  $web = Invoke-Detect @("design-system/soxu/tokens.json", "apps/mobile-web/src/App.tsx")
  Assert-Condition ((-not $web.rust) -and $web.mobileWeb -and (-not $web.paseoOrchestration)) "PWA 与设计令牌变更跳过 Rust 构建测试"

  # 6. Paseo 编排脚本与技能变更仅触发 paseoOrchestration 自测
  $paseo = Invoke-Detect @("scripts/paseo-dispatch.ps1", ".agents/skills/paseo-orchestrator/SKILL.md")
  Assert-Condition ((-not $paseo.rust) -and (-not $paseo.mobileWeb) -and $paseo.paseoOrchestration) "Paseo 编排变更精准命中 paseo-orchestration 自测"

  # 7. 根 Cargo.toml 触发 workspace Rust；工作流/门禁脚本或 -Full 触发全量
  $cargoRoot = Invoke-Detect @("Cargo.toml")
  Assert-Condition ($cargoRoot.rust -and $cargoRoot.rustScope -eq "workspace" -and (-not $cargoRoot.mobileWeb)) "根 Cargo.toml 变更升级为整工作区 Rust 检查"

  $explicitFull = Invoke-Detect -Paths @("README.md") -Full
  Assert-Condition ($explicitFull.full -and $explicitFull.rust -and $explicitFull.rustScope -eq "workspace" -and $explicitFull.mobileWeb) "-Full 参数强制触发全量检查"

  # 8. 外部 pwsh -File CLI 逗号传入多文件也应完整识别
  $cliRaw = (& pwsh -NoProfile -File $detectScript -ChangedFiles "AGENTS.md,docs/ops/LESSONS.md" -Json | Out-String)
  if ($LASTEXITCODE -ne 0) { throw "外部 CLI 调用 detect-check-scope.ps1 失败" }
  $cliObj = $cliRaw | ConvertFrom-Json -ErrorAction Stop
  Assert-Condition ($cliObj.changedFiles.Count -eq 2 -and (-not $cliObj.rust) -and (-not $cliObj.mobileWeb)) "pwsh -File 逗号传参兼容多个变更路径"

  # 9. 发版编排与编码标准变更仅跑治理隐私门禁，不拖累 Rust 与 PWA
  $releaseDocs = Invoke-Detect @(".github/workflows/release.yml", "docs/ops/CODE_STANDARDS.md")
  Assert-Condition ($releaseDocs.governance -and (-not $releaseDocs.rust) -and (-not $releaseDocs.mobileWeb)) "release.yml 与 CODE_STANDARDS.md 变更跳过无关 Rust 与 PWA 构建"

  # 10. rustPackages 与 cargo metadata 真实工作区包名一致
  if (Get-Command cargo -ErrorAction SilentlyContinue) {
    $metaRaw = (& cargo metadata --manifest-path (Join-Path $repoRoot "Cargo.toml") --no-deps --format-version 1 --locked 2>$null | Out-String)
    if ($LASTEXITCODE -eq 0 -and -not [string]::IsNullOrWhiteSpace($metaRaw)) {
      $meta = $metaRaw | ConvertFrom-Json -ErrorAction Stop
      $realPkgs = (@($meta.packages | ForEach-Object { $_.name } | Sort-Object -Unique) -join ",")
      $detectedPkgs = (@($cargoRoot.rustPackages | Sort-Object -Unique) -join ",")
      Assert-Condition ($detectedPkgs -eq $realPkgs) "rustPackages 与 cargo metadata 真实工作区包名完全一致 ($realPkgs)"
    }
  }

  # -------------------------------------------------------------------------
  # 一、结构化路径输入：单文件保真与版本校验
  # -------------------------------------------------------------------------

  $singleJson = '{"schemaVersion":1,"changedFiles":["docs/notes,with-comma.md"]}'
  $single = Invoke-Detect -ChangedFilesJson $singleJson
  Assert-Condition ($single.mode -eq "explicit" -and $single.changedFiles.Count -eq 1 -and $single.changedFiles[0] -eq "docs/notes,with-comma.md") "结构化路径中的逗号保持单一路径，不再被拆成两个文件"

  # 旧检测器会把 docs/notes,with-comma.md 拆成两个路径并误升全量；新路径必须留在文档级
  Assert-Condition ((-not $single.full) -and (-not $single.rust) -and (-not $single.mobileWeb)) "含逗号的单文件路径不再被误升为全量检查"

  $literalJson = '{"schemaVersion":1,"changedFiles":["docs/literal\\backslash.md"]}'
  $literal = Invoke-Detect -ChangedFilesJson $literalJson
  Assert-Condition ($literal.changedFiles.Count -eq 1 -and $literal.changedFiles[0] -eq "docs/literal\backslash.md") "POSIX 字面反斜杠路径完整保留，不被改写成分隔符"

  $legacyComma = Invoke-Detect -Paths @("docs\ops\LESSONS.md,AGENTS.md")
  Assert-Condition ($legacyComma.changedFiles.Count -eq 2 -and ($legacyComma.changedFiles -contains "docs/ops/LESSONS.md") -and ($legacyComma.changedFiles -contains "AGENTS.md")) "既有 Windows 反斜杠 CLI 输入仍归一化为仓库相对规范路径"

  $jsonFile = Join-Path $tempRoot "scope-paths.json"
  Set-Content -LiteralPath $jsonFile -Value $literalJson -Encoding utf8 -NoNewline
  $fromFile = Invoke-Detect -ChangedFilesPathFile $jsonFile
  Assert-Condition ($fromFile.changedFiles.Count -eq 1 -and $fromFile.changedFiles[0] -eq "docs/literal\backslash.md") "结构化路径文件输入与内联 JSON 输出一致"

  # 无效结构化输入不得中止检查：保守回退全量并留下稳定可分组原因
  Assert-ConservativeFull (Invoke-Detect -ChangedFilesJson '{"changedFiles":["docs/a.md"]}') "structured_schema_version_missing" "结构化路径缺版本保守回退全量而不是中止"
  Assert-ConservativeFull (Invoke-Detect -ChangedFilesJson '{"schemaVersion":2,"changedFiles":["docs/a.md"]}') "structured_schema_version_unsupported" "不识别的结构化版本保守回退全量"
  Assert-ConservativeFull (Invoke-Detect -ChangedFilesJson '{"schemaVersion":"1","changedFiles":["docs/a.md"]}') "structured_schema_version_invalid" "版本类型非法保守回退全量"
  Assert-ConservativeFull (Invoke-Detect -ChangedFilesJson 'not-a-json-payload') "structured_payload_invalid_json" "非法 JSON 保守回退全量"
  Assert-ConservativeFull (Invoke-Detect -ChangedFilesJson '{"schemaVersion":1,"changedFiles":"docs/a.md"}') "structured_changed_files_not_array" "标量 changedFiles 不被 @() 包装，保守回退全量"
  Assert-ConservativeFull (Invoke-Detect -ChangedFilesJson '{"schemaVersion":1,"changedFiles":["/etc/passwd"]}') "structured_path_out_of_repo" "越界绝对路径保守回退全量"
  Assert-ConservativeFull (Invoke-Detect -ChangedFilesJson '{"schemaVersion":1,"changedFiles":["docs/../secret.md"]}') "structured_path_out_of_repo" "含相对段的越界路径保守回退全量"
  Assert-ConservativeFull (Invoke-Detect -ChangedFilesJson '{"schemaVersion":1,"changedFiles":[42]}') "structured_changed_files_entry_not_string" "非字符串条目保守回退全量"
  Assert-ConservativeFull (Invoke-Detect -ChangedFilesPathFile (Join-Path $tempRoot "missing-scope.json")) "structured_payload_unreadable" "结构化路径文件缺失保守回退全量"

  $emptyArray = Invoke-Detect -ChangedFilesJson '{"schemaVersion":1,"changedFiles":[]}'
  Assert-Condition (
    (-not $emptyArray.full) -and
    (-not $emptyArray.rust) -and
    $emptyArray.mode -eq "explicit" -and
    $emptyArray.changedFiles.Count -eq 0 -and
    $emptyArray.fallbackReasons.Count -eq 0
  ) "合法空数组表示真正无变更，不误升全量"

  # -------------------------------------------------------------------------
  # 二、计划字段：模式、平台、必跑 job、测试配置
  # -------------------------------------------------------------------------

  Assert-Condition ($single.schemaVersion -eq 1) "检测计划声明 schemaVersion=1"

  Assert-Condition (
    $releaseDocs.requiredPlatforms.Count -eq 0 -and
    ($releaseDocs.testProfiles -contains "release-contract") -and
    ($releaseDocs.requiredJobs -notcontains "rust-check") -and
    ($releaseDocs.requiredJobs -notcontains "mobile-web-check")
  ) "单改 release.yml 只在治理 job 跑发布专项，不推导 Intel 平台也不启动 Rust/PWA"

  Assert-Condition ($git.requiredPlatforms -contains "windows" -and $git.requiredPlatforms -contains "linux" -and $git.requiredPlatforms -contains "macos-arm64" -and (-not ($git.requiredPlatforms -contains "macos-intel"))) "soxu-git 消费者闭包要求 Linux 与 ARM64 macOS，但文件修改不推导 Intel"

  Assert-Condition ($cargoRoot.requiredPlatforms -contains "linux" -and $cargoRoot.requiredPlatforms -contains "macos-arm64") "工作区级 Rust 变更要求 Linux 与 ARM64 macOS"

  Assert-Condition ($explicitFull.requiredPlatforms -contains "macos-intel") "手动全量显式请求 Intel macOS"

  Assert-Condition ($web.requiredPlatforms -contains "linux" -and (-not ($web.requiredPlatforms -contains "windows"))) "纯 PWA 变更只要求 Linux runner"

  Assert-Condition ($git.testProfiles -contains "cargo-clippy" -and $git.testProfiles -contains "cargo-test" -and (-not ($git.testProfiles -contains "design-tokens"))) "Rust 变更的测试配置只含 Rust 相关 profile"

  Assert-Condition ($docs.testProfiles.Count -eq 1 -and $docs.testProfiles[0] -eq "governance-and-privacy") "纯文档变更只配置治理与隐私 profile"

  Assert-Condition (@($git.requiredJobs) -join ',' -eq "detect-changes,governance-and-privacy,rust-check,ci-gate") "必跑 job 集合与顺序确定，且 PWA 未触及时不列入"

  # -------------------------------------------------------------------------
  # 三、Git 基准语义：无效/全零基准、本地工作区、merge base、push 完整树
  # -------------------------------------------------------------------------

  $cleanRepo = New-GitFixture "git-clean"
  Write-FixtureFile $cleanRepo "docs/baseline.md" "baseline"
  Commit-Fixture $cleanRepo "init" | Out-Null

  $badBase = Invoke-Detect -RepoRootOverride $cleanRepo -BaseRef "refs/heads/does-not-exist" -IncludeWorkingTree
  Assert-Condition ($badBase.full -eq $false -and $badBase.rustScope -eq "none" -and @($badBase.reasons | Where-Object { $_ -like "worktree_only:base_ref_unresolvable*" }).Count -eq 1) "无效显式基准搭配工作区模式只降级为工作区检测并记录原因"

  $zeroBase = Invoke-Detect -RepoRootOverride $cleanRepo -EventName "push" -BaseRef ("0" * 40) -HeadRef "HEAD"
  Assert-Condition ($zeroBase.full -and @($zeroBase.fallbackReasons | Where-Object { $_ -eq "base_ref_all_zeros" }).Count -eq 1) "全零基准保守回退全量并记录原因"

  $badPushBase = Invoke-Detect -RepoRootOverride $cleanRepo -EventName "push" -BaseRef "deadbeef" -HeadRef "HEAD"
  Assert-Condition ($badPushBase.full -and @($badPushBase.fallbackReasons | Where-Object { $_ -like "base_ref_unresolvable*" }).Count -eq 1) "无法解析的 push 基准保守回退全量"

  # 本地模式：已提交 + 暂存 + 未暂存 + 未跟踪
  $localRepo = New-GitFixture "git-local"
  Write-FixtureFile $localRepo "docs/keep.md" "keep"
  Write-FixtureFile $localRepo "docs/modified.md" "v1"
  Write-FixtureFile $localRepo "docs/deleted.md" "gone"
  Commit-Fixture $localRepo "init" | Out-Null
  Write-FixtureFile $localRepo "docs/modified.md" "v2"
  Write-FixtureFile $localRepo "docs/staged.md" "staged"
  Invoke-GitIn $localRepo @("add", "docs/staged.md") | Out-Null
  Write-FixtureFile $localRepo "docs/untracked.md" "untracked"
  Remove-FixtureFile $localRepo "docs/deleted.md"

  $local = Invoke-Detect -RepoRootOverride $localRepo -IncludeWorkingTree
  Assert-Condition (
    $local.mode -eq "local" -and
    ($local.changedFiles -contains "docs/modified.md") -and
    ($local.changedFiles -contains "docs/staged.md") -and
    ($local.changedFiles -contains "docs/untracked.md") -and
    ($local.changedFiles -contains "docs/deleted.md") -and
    (-not ($local.changedFiles -contains "docs/keep.md"))
  ) "本地模式同时计入暂存、未暂存、删除与未跟踪，不误报未变更文件"

  # push：比较 before/after 完整树，即使历史非线性也不改用 merge base
  $pushRepo = New-GitFixture "git-push"
  Write-FixtureFile $pushRepo "docs/a.md" "a"
  Commit-Fixture $pushRepo "one" | Out-Null
  Write-FixtureFile $pushRepo "docs/b.md" "b"
  $before = Commit-Fixture $pushRepo "two"
  Invoke-GitIn $pushRepo @("checkout", "-q", "-b", "rewrite", "HEAD~1") | Out-Null
  Write-FixtureFile $pushRepo "docs/c.md" "c"
  $after = Commit-Fixture $pushRepo "three"

  $push = Invoke-Detect -RepoRootOverride $pushRepo -EventName "push" -BaseRef $before -HeadRef $after
  Assert-Condition (
    ($push.changedFiles -contains "docs/b.md") -and
    ($push.changedFiles -contains "docs/c.md") -and
    $push.mergeBase -eq ""
  ) "push 用 before/after 完整树比较，非线性历史也保留被回退的路径"

  # PR：以明确 base/head 求 merge base
  $prRepo = New-GitFixture "git-pr"
  Write-FixtureFile $prRepo "docs/base.md" "base"
  $prBase = Commit-Fixture $prRepo "base"
  Invoke-GitIn $prRepo @("checkout", "-q", "-b", "feature") | Out-Null
  Write-FixtureFile $prRepo "docs/feature.md" "feature"
  $prHead = Commit-Fixture $prRepo "feature"
  Invoke-GitIn $prRepo @("checkout", "-q", "main") | Out-Null
  Write-FixtureFile $prRepo "docs/main-only.md" "main"
  Commit-Fixture $prRepo "main moves on" | Out-Null

  $pr = Invoke-Detect -RepoRootOverride $prRepo -EventName "pull_request" -BaseRef "main" -HeadRef $prHead
  Assert-Condition (
    $pr.changedFiles.Count -eq 1 -and
    $pr.changedFiles[0] -eq "docs/feature.md" -and
    $pr.mergeBase -ne ""
  ) "PR 以 merge base 求差，不把基准分支后续提交算进 PR 范围"

  # 删除、重命名、逗号、非 ASCII 与空白路径
  $renameRepo = New-GitFixture "git-rename"
  Write-FixtureFile $renameRepo "old name.md" "payload"
  Write-FixtureFile $renameRepo "docs/gone.md" "gone"
  Commit-Fixture $renameRepo "init" | Out-Null
  Invoke-GitIn $renameRepo @("mv", "old name.md", "docs/new name.md") | Out-Null
  Remove-FixtureFile $renameRepo "docs/gone.md"
  Write-FixtureFile $renameRepo "docs/notes,with-comma.md" "comma"
  Write-FixtureFile $renameRepo "docs/中文 文档.md" "cn"
  $renameHead = Commit-Fixture $renameRepo "rename and delete"

  $renamed = Invoke-Detect -RepoRootOverride $renameRepo -EventName "push" -BaseRef "HEAD~1" -HeadRef $renameHead
  Assert-Condition (
    ($renamed.changedFiles -contains "old name.md") -and
    ($renamed.changedFiles -contains "docs/new name.md") -and
    ($renamed.changedFiles -contains "docs/gone.md") -and
    ($renamed.changedFiles -contains "docs/notes,with-comma.md") -and
    ($renamed.changedFiles -contains "docs/中文 文档.md")
  ) "NUL 记录消费重命名两侧、删除旧路径、逗号、空格与非 ASCII 路径"

  Assert-Condition ($renamed.changedFiles.Count -eq 5) "NUL 记录不产生多余或截断的路径"

  # 跨模块重命名仍保守回退，但两侧路径都要保留
  $crossRepo = New-GitFixture "git-cross-rename"
  Write-FixtureFile $crossRepo "Cargo.toml" @'
[workspace]
resolver = "3"
members = ["crates/new-crate"]
'@
  Write-FixtureFile $crossRepo "crates/old-crate/Cargo.toml" @'
[package]
name = "fx-moved"
version = "0.1.0"
edition = "2024"
'@
  Write-FixtureFile $crossRepo "crates/old-crate/src/lib.rs" "pub fn m() {}"
  Commit-Fixture $crossRepo "init" | Out-Null
  Invoke-GitIn $crossRepo @("mv", "crates/old-crate", "crates/new-crate") | Out-Null
  $crossHead = Commit-Fixture $crossRepo "cross rename"
  & cargo generate-lockfile --offline --manifest-path (Join-Path $crossRepo "Cargo.toml") 2>&1 | Out-Null
  $cross = Invoke-Detect -RepoRootOverride $crossRepo -EventName "push" -BaseRef "HEAD~1" -HeadRef $crossHead
  Assert-Condition (
    $cross.full -and
    ($cross.changedFiles -contains "crates/old-crate/src/lib.rs") -and
    ($cross.changedFiles -contains "crates/new-crate/src/lib.rs") -and
    @($cross.fallbackReasons | Where-Object { $_ -like "unknown_crate_dir:*" }).Count -ge 1
  ) "跨模块重命名保留两侧路径并保守回退全量"

  # 未知路径与手动全量
  $unknown = Invoke-Detect @("weird/dir/file.txt")
  Assert-Condition ($unknown.full -and @($unknown.fallbackReasons | Where-Object { $_ -eq "unattributed_path:weird/dir/file.txt" }).Count -eq 1) "无法归因的路径保守回退全量并留下可分组原因"

  # -------------------------------------------------------------------------
  # 四、Cargo 工作区夹具：嵌套通配成员、别名、继承与依赖类型
  # -------------------------------------------------------------------------

  $ws = Join-Path $tempRoot "cargo-ws"
  New-Item -ItemType Directory -Path $ws -Force | Out-Null
  Write-FixtureFile $ws "Cargo.toml" @'
[workspace]
resolver = "3"
members = ["crates/protocol", "crates/consumer", "crates/group/*"]

[workspace.package]
version = "0.1.0"
edition = "2024"

[workspace.dependencies]
fx-protocol = { path = "crates/protocol" }
'@
  Write-FixtureFile $ws "crates/protocol/Cargo.toml" @'
[package]
name = "fx-protocol"
version.workspace = true
edition.workspace = true
'@
  Write-FixtureFile $ws "crates/protocol/src/lib.rs" "pub fn p() {}"
  Write-FixtureFile $ws "crates/consumer/Cargo.toml" @'
[package]
name = "fx-consumer"
version.workspace = true
edition.workspace = true

[dependencies]
fx-protocol = { workspace = true }
aliased = { path = "../group/a", package = "fx-a" }

[target.'cfg(windows)'.dependencies]
fx-a = { path = "../group/a" }

[dev-dependencies]
fx-a = { path = "../group/a" }

[build-dependencies]
fx-a = { path = "../group/a" }
'@
  Write-FixtureFile $ws "crates/consumer/src/lib.rs" "pub fn c() {}"
  Write-FixtureFile $ws "crates/group/a/Cargo.toml" @'
[package]
name = "fx-a"
version.workspace = true
edition.workspace = true

[dependencies]
fx-protocol = { workspace = true, optional = true }
'@
  Write-FixtureFile $ws "crates/group/a/src/lib.rs" "pub fn a() {}"
  & cargo generate-lockfile --offline --manifest-path (Join-Path $ws "Cargo.toml") 2>&1 | Out-Null
  if ($LASTEXITCODE -ne 0) { throw "夹具工作区生成 Cargo.lock 失败" }

  $nested = Invoke-Detect -RepoRootOverride $ws -Paths @("crates/group/a/src/lib.rs")
  Assert-Condition (
    $nested.rust -and
    $nested.rustScope -eq "crates" -and
    $nested.seedPackages.Count -eq 1 -and
    $nested.seedPackages[0] -eq "fx-a"
  ) "嵌套通配成员的变更路径按真实 manifest_path 命中 fx-a，而不是只看 crates/<第一级>"

  Assert-Condition (
    ($nested.affectedPackages -contains "fx-a") -and
    ($nested.affectedPackages -contains "fx-consumer") -and
    (-not ($nested.affectedPackages -contains "fx-protocol"))
  ) "反向闭包只包含真实消费者，不把被依赖方算进影响范围"

  Assert-Condition (-not ($nested.affectedPackages -contains "aliased")) "重命名别名按目标成员真实包名归因，不产生别名伪包"

  $wsProto = Invoke-Detect -RepoRootOverride $ws -Paths @("crates/protocol/src/lib.rs")
  Assert-Condition ($wsProto.rustScope -eq "workspace" -and $wsProto.affectedPackages.Count -eq 3) "基础层变更传递展开至嵌套成员并升级为整个夹具工作区"

  $removed = Invoke-Detect -RepoRootOverride $ws -Paths @("crates/removed-crate/src/lib.rs")
  Assert-Condition ($removed.full -and @($removed.fallbackReasons | Where-Object { $_ -like "unknown_crate_dir:*" }).Count -eq 1) "已移出当前图的历史 Crate 路径保守回退全量"

  # 元数据损坏：成员清单指向不存在的成员，cargo metadata 失败必须保守回退
  $broken = Join-Path $tempRoot "cargo-broken"
  New-Item -ItemType Directory -Path $broken -Force | Out-Null
  Write-FixtureFile $broken "Cargo.toml" @'
[workspace]
resolver = "3"
members = ["crates/missing"]
'@
  $brokenResult = Invoke-Detect -RepoRootOverride $broken -Paths @("crates/missing/src/lib.rs")
  Assert-Condition (
    $brokenResult.full -and
    @($brokenResult.fallbackReasons | Where-Object { $_ -like "cargo_metadata_failed:*" }).Count -eq 1
  ) "元数据损坏时保守回退全量并记录可机器分组的原因"

  # 纯文档 / AOCI / PWA 不启动 Cargo：夹具目录没有 Cargo.toml，一旦读元数据就会留下 cargo_manifest_absent
  $noCargo = Join-Path $tempRoot "no-cargo"
  New-Item -ItemType Directory -Path $noCargo -Force | Out-Null
  $noCargoDocs = Invoke-Detect -RepoRootOverride $noCargo -Paths @("docs/ops/LESSONS.md", ".aoci/baseline.json")
  Assert-Condition (
    (-not $noCargoDocs.rust) -and
    $noCargoDocs.fallbackReasons.Count -eq 0
  ) "纯文档与 AOCI 路径不启动 cargo metadata"

  $noCargoPwa = Invoke-Detect -RepoRootOverride $noCargo -Paths @("apps/mobile-web/src/App.tsx", "design-system/soxu/tokens.json")
  Assert-Condition (
    $noCargoPwa.mobileWeb -and
    (-not $noCargoPwa.rust) -and
    $noCargoPwa.fallbackReasons.Count -eq 0
  ) "可独立判定的 PWA 路径不启动 cargo metadata"

  # -------------------------------------------------------------------------
  # 五、进程错误脱敏：绝对目录与凭据形态不进入计划
  # -------------------------------------------------------------------------

  $leakyRepo = Join-Path $tempRoot "cargo-leaky"
  New-Item -ItemType Directory -Path $leakyRepo -Force | Out-Null
  Write-FixtureFile $leakyRepo "Cargo.toml" @'
[workspace]
resolver = "3"
members = ["crates/leaky"]
'@
  Write-FixtureFile $leakyRepo "crates/leaky/Cargo.toml" @'
[package]
name = "fx-leaky"
version = "0.1.0"
edition = "2024"
this is not valid toml
'@
  $leaky = Invoke-Detect -RepoRootOverride $leakyRepo -Paths @("crates/leaky/src/lib.rs")
  $leakyReasons = @($leaky.fallbackReasons) -join " ; "
  $hasAbsolutePath = $leakyReasons -match '(?i)[a-z]:[\\/]'
  Assert-Condition (
    ($leakyReasons -like "cargo_metadata_failed:*") -and
    (-not $hasAbsolutePath)
  ) "Cargo 失败原因不携带绝对目录，只保留类别与最小脱敏诊断"

  Assert-Condition ($leakyReasons -notmatch '(?i)bearer|token=') "Cargo 失败原因不携带凭据形态内容"

  Write-Host "✓ 共 $passed 项作用域检测回归全部通过！" -ForegroundColor Green
} finally {
  if (Test-Path -LiteralPath $tempRoot) {
    Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
  }
}
exit 0
