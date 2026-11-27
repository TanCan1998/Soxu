param(
  [string]$Verifier = "",
  [switch]$RequireBash
)

$ErrorActionPreference = "Stop"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$OutputEncoding = [System.Text.Encoding]::UTF8

# CI Gate 与发布契约回归矩阵：
#   1. Verify：必跑/计划内跳过/取消/失败、多实例证据、证据集合异常、计划缺失或非法
#   2. 计划契约：删 job、换 job、翻转 required、篡改 depends_on/触发标志/scope 一律拒绝
#   3. Plan：检测器输出与 scope 自身矛盾（rust/rustScope/full/changedFiles）必须失败关闭
#   4. 突变敏感度：移除契约校验后上述绕过用例重新通过，证明确实是该校验在防守
#   5. 工作流静态契约（真实文件 + 负向变体）
#   6. 发布契约动态验证：抽取 release.yml 内联 Bash 做 bash -n，并用离线 gh 桩
#      与临时 dist 夹具真正执行镜像校验路径，坏例必须在任何 create/edit/upload 之前失败
# 所有夹具只写入系统临时目录，结束即清理；成功必须显式 exit 0。
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$verifierPath = if ([string]::IsNullOrWhiteSpace($Verifier)) {
  Join-Path $PSScriptRoot "verify-ci-gate.ps1"
} else {
  $Verifier
}
if (-not (Test-Path -LiteralPath $verifierPath -PathType Leaf)) {
  Write-Host "::error::未找到门禁校验脚本：$verifierPath" -ForegroundColor Red
  exit 1
}

$ciWorkflow = Join-Path $repoRoot ".github/workflows/ci.yml"
$releaseWorkflow = Join-Path $repoRoot ".github/workflows/release.yml"
$releaseAssetNames = @(
  "soxu-linux-amd64.tar.gz"
  "soxu-windows-amd64.zip"
  "soxu-darwin-arm64.tar.gz"
  "soxu-darwin-amd64.tar.gz"
)
$releaseAssetManifest = ($releaseAssetNames -join " ")

$tempRoot = Join-Path ([IO.Path]::GetTempPath()) ("soxu-ci-gate-" + [Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null

$script:checks = 0
$script:failures = [System.Collections.Generic.List[string]]::new()

function Assert-Check([bool]$Condition, [string]$Name) {
  $script:checks++
  if (-not $Condition) { $script:failures.Add($Name) }
}

function Get-PosixPath([string]$PathValue) {
  return ($PathValue -replace '\\', '/')
}

function Invoke-Verifier([string[]]$Arguments) {
  $output = (& pwsh -NoProfile -File $verifierPath @Arguments 2>&1 | Out-String)
  return [pscustomobject]@{ Code = $LASTEXITCODE; Output = $output }
}

function Invoke-Script([string]$ScriptPath, [string[]]$Arguments) {
  $output = (& pwsh -NoProfile -File $ScriptPath @Arguments 2>&1 | Out-String)
  return [pscustomobject]@{ Code = $LASTEXITCODE; Output = $output }
}

function Save-Fixture([string]$Name, [string]$Content) {
  $path = Join-Path $tempRoot ($Name + ".json")
  Set-Content -LiteralPath $path -Value $Content -Encoding utf8 -NoNewline
  return $path
}

function Invoke-GateVerify {
  param(
    [string]$PlanJson,
    [string]$EvidenceJson,
    [string]$ScriptPath = "",
    [string]$JobsJson = ""
  )
  $target = if ([string]::IsNullOrWhiteSpace($ScriptPath)) { $verifierPath } else { $ScriptPath }
  $arguments = @("-Mode", "Verify")
  if ($null -ne $PlanJson) {
    $arguments += @("-PlanPath", (Save-Fixture ("plan-" + [Guid]::NewGuid().ToString("N")) $PlanJson))
  }
  if ($null -ne $EvidenceJson) {
    $arguments += @("-EvidencePath", (Save-Fixture ("evidence-" + [Guid]::NewGuid().ToString("N")) $EvidenceJson))
  }
  if (-not [string]::IsNullOrWhiteSpace($JobsJson)) {
    $arguments += @("-JobsPath", (Save-Fixture ("jobs-verify-" + [Guid]::NewGuid().ToString("N")) $JobsJson))
  }
  return Invoke-Script $target $arguments
}

function Invoke-PlanMode([string]$ScopeJson) {
  $scopePath = Save-Fixture ("scope-" + [Guid]::NewGuid().ToString("N")) $ScopeJson
  return Invoke-Verifier @("-Mode", "Plan", "-ScopePath", $scopePath)
}

function Invoke-WorkflowContract([string]$WorkflowPath) {
  return Invoke-Verifier @("-Mode", "Contract", "-WorkflowPath", $WorkflowPath)
}

function Invoke-ReleaseContract([string]$WorkflowPath) {
  return Invoke-Verifier @("-Mode", "ReleaseContract", "-ReleaseWorkflowPath", $WorkflowPath)
}

function Assert-Pass([string]$Name, $Result) {
  Assert-Check ($Result.Code -eq 0) "$Name → 期望通过，实际 exit $($Result.Code)：$($Result.Output.Trim())"
}

function Assert-Fail([string]$Name, $Result, [string]$Marker, [int]$ExpectedCode = 1) {
  $codeOk = $Result.Code -eq $ExpectedCode
  Assert-Check $codeOk "$Name → 期望 exit $ExpectedCode，实际 exit $($Result.Code)：$($Result.Output.Trim())"
  if ($codeOk -and -not [string]::IsNullOrEmpty($Marker)) {
    Assert-Check ($Result.Output.Contains($Marker)) "$Name → 缺少失败标记 $Marker：$($Result.Output.Trim())"
  }
}

function New-WorkflowVariant([string]$SourceRelPath, [string]$Name, [scriptblock]$Mutate) {
  $sourcePath = Join-Path $repoRoot $SourceRelPath
  $text = (Get-Content -LiteralPath $sourcePath -Raw -Encoding utf8).Replace("`r`n", "`n")
  $mutated = & $Mutate $text
  if ($mutated -eq $text) { throw "变体 $Name 未改变源文件内容，夹具失效。" }
  $dest = Join-Path $tempRoot ($Name + ".yml")
  Set-Content -LiteralPath $dest -Value $mutated -Encoding utf8 -NoNewline
  return $dest
}

function Get-RunBlocks([string]$WorkflowPath) {
  # 复用 verify-ci-gate.ps1 的 ExtractRuns 模式：工作流内联脚本提取只有一份实现
  $result = Invoke-Verifier @("-Mode", "ExtractRuns", "-WorkflowPath", $WorkflowPath)
  if ($result.Code -ne 0) { throw "提取工作流内联脚本失败：$($result.Output.Trim())" }
  return @(($result.Output.Trim() | ConvertFrom-Json) | ForEach-Object { $_ })
}

function Get-StubPatternList([string]$StubLog) {
  $line = @($StubLog -split "`n" | Where-Object { $_ -like 'PATTERNS:*' }) | Select-Object -First 1
  if (-not $line) { return $null }
  return (($line.Substring('PATTERNS:'.Length)).Trim())
}

function Find-BashExecutable {
  $candidates = [System.Collections.Generic.List[string]]::new()
  $gitCommand = Get-Command git -ErrorAction SilentlyContinue
  if ($gitCommand -and $gitCommand.Source) {
    $gitDir = Split-Path (Split-Path $gitCommand.Source -Parent) -Parent
    $candidates.Add((Join-Path $gitDir "bin/bash.exe"))
    $candidates.Add((Join-Path $gitDir "usr/bin/bash.exe"))
  }
  $candidates.Add("C:\Program Files\Git\bin\bash.exe")
  $candidates.Add("C:\Program Files\Git\usr\bin\bash.exe")
  foreach ($candidate in $candidates) {
    if (Test-Path -LiteralPath $candidate -PathType Leaf) { return $candidate }
  }
  $bashCommand = Get-Command bash -ErrorAction SilentlyContinue
  if ($bashCommand -and $bashCommand.Source -and $bashCommand.Source -notmatch 'WindowsApps') {
    return $bashCommand.Source
  }
  return $null
}

$script:bashPath = Find-BashExecutable

function Invoke-Bash([string[]]$Arguments, [string]$WorkingDirectory = "") {
  if ([string]::IsNullOrEmpty($WorkingDirectory)) {
    $output = (& $script:bashPath --noprofile --norc @Arguments 2>&1 | Out-String)
    return [pscustomobject]@{ Code = $LASTEXITCODE; Output = $output }
  }
  Push-Location $WorkingDirectory
  try {
    $output = (& $script:bashPath --noprofile --norc @Arguments 2>&1 | Out-String)
    $code = $LASTEXITCODE
  } finally {
    Pop-Location
  }
  return [pscustomobject]@{ Code = $code; Output = $output }
}

# 计划夹具构造器：默认产出与 Plan 模式一致的合法计划，可选篡改单个字段以构造绕过用例
function New-PlanJson {
  param(
    [bool]$GovernanceRequired = $true,
    [bool]$RustRequired = $true,
    [bool]$MobileRequired = $false,
    [bool]$Rust = $true,
    [string]$RustScope = "crates",
    [bool]$MobileWeb = $false,
    [bool]$Paseo = $false,
    [bool]$ScopeSelfTest = $false,
    [bool]$Full = $false,
    [object[]]$ChangedFiles = @("crates/soxu-git/src/probe.rs"),
    # 平台矩阵实例集：默认与检测器的三平台常规覆盖一致。
    [object[]]$Platforms = @("windows", "linux", "macos-arm64"),
    [bool]$SelfTest = $false,
    [bool]$ReleaseContract = $false,
    [object[]]$RustInstances = $null,
    [switch]$OmitRustInstances,
    [string]$DependsOnRust = "rust",
    [string]$Version = "ci-gate-plan/v1",
    [int]$ScopeSchemaVersion = 0,
    [string]$PathContractOverride = "",
    [switch]$OmitPathContract,
    [switch]$OmitSelfTest,
    [switch]$OmitScope,
    [switch]$OmitGate,
    [switch]$OmitChangedFiles,
    # scope.platforms 可与 instances 分开篡改，用于构造“计划删平台”绕过用例。
    [object[]]$ScopePlatforms = $null,
    [switch]$OmitPlatforms
  )
  $effectiveScopePlatforms = if ($null -ne $ScopePlatforms) { @($ScopePlatforms) } elseif ($Rust) { @($Platforms) } else { @() }
  $plan = [ordered]@{ version = $Version; detector = "detect-changes" }
  # 路径契约必须与 scope 推导一致：版本化 scope 原样保留，旧 scope 归一化反斜杠。
  $derivedContract = if ($ScopeSchemaVersion -ge 1) { "repo-relative/v1" } else { "repo-relative/legacy" }
  $contract = if ([string]::IsNullOrWhiteSpace($PathContractOverride)) { $derivedContract } else { $PathContractOverride }
  if (-not $OmitPathContract) { $plan["pathContract"] = $contract }
  if (-not $OmitGate) { $plan["gate"] = "ci-gate" }

  $rustJob = [ordered]@{ id = "rust-check"; required = $RustRequired }
  if ($DependsOnRust -ne "omit") { $rustJob["depends_on"] = $DependsOnRust }
  $effectivePlatforms = if (-not $Rust) { @() } else { @($Platforms) }
  if ($null -ne $RustInstances) {
    $rustJob["instances"] = @($RustInstances)
  } elseif ($RustRequired -and -not $OmitRustInstances -and $effectivePlatforms.Count -gt 0) {
    $rustJob["instances"] = @($effectivePlatforms)
  }

  $plan["jobs"] = @(
    [ordered]@{ id = "governance-and-privacy"; required = $GovernanceRequired }
    [ordered]@{ id = "rust-format"; required = $RustRequired; depends_on = "rust" }
    $rustJob
    [ordered]@{ id = "mobile-web-check"; required = $MobileRequired; depends_on = "mobile_web" }
  )
  if (-not $OmitSelfTest) { $plan["selfTest"] = $SelfTest }
  $plan["releaseContract"] = $ReleaseContract
  if (-not $OmitScope) {
    $scope = [ordered]@{
      full               = $Full
      rust               = $Rust
      rustScope          = $RustScope
      mobileWeb          = $MobileWeb
      paseoOrchestration = $Paseo
      checkScopeSelfTest = $ScopeSelfTest
    }
    if (-not $OmitPlatforms) { $scope["platforms"] = @($effectiveScopePlatforms) }
    if ($ScopeSchemaVersion -ge 1) { $scope["schemaVersion"] = $ScopeSchemaVersion }
    if (-not $OmitChangedFiles) {
      $scopeFiles = if ($ScopeSchemaVersion -ge 1) {
        @($ChangedFiles)
      } else {
        @($ChangedFiles | ForEach-Object { ([string]$_).Replace('\', '/') })
      }
      $scope["changedFiles"] = @($scopeFiles)
    }
    $plan["scope"] = $scope
  }
  return ($plan | ConvertTo-Json -Depth 10)
}

function New-ScopeJson {
  param(
    [bool]$Full = $false,
    [bool]$Rust = $true,
    [string]$RustScope = "crates",
    [bool]$MobileWeb = $false,
    [bool]$Paseo = $false,
    [bool]$ScopeSelfTest = $false,
    [object[]]$ChangedFiles = @("crates/soxu-git/src/probe.rs"),
    # 平台矩阵实例集：与检测器一致，rust=false 时为空。
    [object[]]$Platforms = $null,
    [int]$ScopeSchemaVersion = 0,
    [switch]$OmitChangedFiles
  )
  $scopePlatforms = if ($null -ne $Platforms) { @($Platforms) } elseif ($Rust) { @("windows", "linux", "macos-arm64") } else { @() }
  $scope = [ordered]@{
    full               = $Full
    rust               = $Rust
    rustScope          = $RustScope
    mobileWeb          = $MobileWeb
    paseoOrchestration = $Paseo
    checkScopeSelfTest = $ScopeSelfTest
    platforms          = @($scopePlatforms)
  }
  if ($ScopeSchemaVersion -ge 1) { $scope["schemaVersion"] = $ScopeSchemaVersion }
  if (-not $OmitChangedFiles) { $scope["changedFiles"] = @($ChangedFiles) }
  $scope["governance"] = $true
  return ($scope | ConvertTo-Json -Depth 5)
}

function New-DistFixture {
  param(
    [string]$Name,
    [string[]]$Assets = $null,
    [string[]]$SumMembers = $null,
    [string[]]$ExtraFiles = @(),
    [switch]$Nested,
    [switch]$OmitSums,
    [switch]$DuplicateMember,
    [switch]$WrongHash,
    [switch]$InvalidDigest
  )
  if ($null -eq $Assets) { $Assets = $releaseAssetNames }
  $dir = Join-Path $tempRoot ("fixture-" + $Name)
  New-Item -ItemType Directory -Path $dir -Force | Out-Null
  foreach ($asset in $Assets) {
    Set-Content -LiteralPath (Join-Path $dir $asset) -Value "soxu offline payload for $asset" -Encoding utf8 -NoNewline
  }
  foreach ($extra in $ExtraFiles) {
    Set-Content -LiteralPath (Join-Path $dir $extra) -Value "unexpected extra payload" -Encoding utf8 -NoNewline
  }
  if ($Nested) {
    $nestedDir = Join-Path $dir "nested"
    New-Item -ItemType Directory -Path $nestedDir -Force | Out-Null
    Set-Content -LiteralPath (Join-Path $nestedDir "stray.txt") -Value "stray" -Encoding utf8 -NoNewline
  }
  if (-not $OmitSums) {
    $members = if ($null -eq $SumMembers) { $Assets } else { $SumMembers }
    $sumLines = [System.Collections.Generic.List[string]]::new()
    foreach ($member in $members) {
      $memberPath = Join-Path $dir $member
      if (-not (Test-Path -LiteralPath $memberPath -PathType Leaf)) { continue }
      $hash = (Get-FileHash -LiteralPath $memberPath -Algorithm SHA256).Hash.ToLowerInvariant()
      if ($WrongHash -and $sumLines.Count -eq 0) { $hash = ("0" * 64) }
      if ($InvalidDigest -and $sumLines.Count -eq 0) {
        # 非法摘要：不是 64 位十六进制。非严格 sha256sum -c 只告警且 exit 0，--strict 才失败。
        $sumLines.Add("invalid  $member")
      } else {
        $sumLines.Add("$hash  $member")
        if ($DuplicateMember -and $sumLines.Count -eq 1) { $sumLines.Add("$hash  $member") }
      }
    }
    Set-Content -LiteralPath (Join-Path $dir "SHA256SUMS") -Value (($sumLines -join "`n") + "`n") -Encoding utf8 -NoNewline
  }
  return $dir
}

function New-BashHarness {
  param(
    [string]$MirrorBody,
    [string]$FixtureDir,
    [string]$StubLog,
    [string[]]$StubExtraFiles = @(),
    [switch]$StubNested
  )
  # 单引号 here-string：内部保留字面 $ 与反引号，避免 PowerShell 提前展开 Bash 变量
  $prelude = @'
# --- 离线 gh 桩：只记录调用并按固定应答返回，绝不触网，不使用真实 secrets ---
# 下载行为模拟 --pattern：只把显式选中的资产放进 dist，其余远端内容不会进入。
gh() {
  printf '%s
' "$*" >> "$STUB_LOG"
  case "$1" in
    release)
      case "$2" in
        download)
          shift 2
          patterns=""
          while [ "$#" -gt 0 ]; do
            case "$1" in
              --pattern|-p)
                patterns="$patterns $2"
                shift 2
                ;;
              --pattern=*)
                patterns="$patterns ${1#--pattern=}"
                shift
                ;;
              *)
                shift
                ;;
            esac
          done
          printf 'PATTERNS:%s\n' "$(printf '%s\n' $patterns | sed '/^$/d' | sort | tr '\n' ' ')" >> "$STUB_LOG"
          for pattern in $patterns; do
            if [ -e "$FIXTURE_DIR/$pattern" ]; then cp -R "$FIXTURE_DIR/$pattern" dist/; fi
          done
          for extra in ${STUB_EXTRA_FILES:-}; do
            if [ -e "$FIXTURE_DIR/$extra" ]; then cp -R "$FIXTURE_DIR/$extra" dist/; fi
          done
          if [ "${STUB_NESTED:-0}" = "1" ] && [ -d "$FIXTURE_DIR/nested" ]; then cp -R "$FIXTURE_DIR/nested" dist/; fi
          ;;
        view)
          case "$*" in
            *"--json body"*) printf 'stub upstream release notes\n' ;;
            *"--json assets"*) printf '%s\n' $EXPECTED_RELEASE_ASSETS SHA256SUMS ;;
            *) return 1 ;;
          esac
          ;;
        list)
          printf '%s\n' $STUB_TAGS
          ;;
        create|edit|upload)
          printf 'STUB %s\n' "$2"
          ;;
        *)
          return 0
          ;;
      esac
      ;;
    api)
      printf 'v1.1.0\n'
      ;;
    *)
      return 0
      ;;
  esac
}
'@
  $nestedFlag = if ($StubNested) { "1" } else { "0" }
  $headerLines = @(
    "export REF='v1.1.0'"
    "export RELEASES_REPO='stub/upstream'"
    "export GITHUB_REPOSITORY='stub/mirror'"
    "export EXPECTED_RELEASE_ASSETS='$releaseAssetManifest'"
    "export GH_TOKEN='stub-token'"
    "export MIRROR_TOKEN='stub-token'"
    "export FIXTURE_DIR='$(Get-PosixPath $FixtureDir)'"
    "export STUB_LOG='$(Get-PosixPath $StubLog)'"
    "export STUB_TAGS='v1.1.0 v0.9.0'"
    "export STUB_EXTRA_FILES='$($StubExtraFiles -join ' ')'"
    "export STUB_NESTED='$nestedFlag'"
  )
  return (($headerLines -join "`n") + "`n" + $prelude + "`n" + $MirrorBody + "`n")
}

function Invoke-ReleaseMirrorCase {
  param(
    [string]$Name,
    [string]$FixtureDir,
    [string]$MirrorBody,
    [string[]]$StubExtraFiles = @(),
    [switch]$StubNested
  )
  $workDir = Join-Path $tempRoot ("mirror-" + $Name)
  New-Item -ItemType Directory -Path $workDir -Force | Out-Null
  $stubLog = Join-Path $workDir "stub.log"
  Set-Content -LiteralPath $stubLog -Value "" -Encoding utf8 -NoNewline
  $harnessPath = Join-Path $workDir "harness.sh"
  $harness = New-BashHarness -MirrorBody $MirrorBody -FixtureDir $FixtureDir -StubLog $stubLog -StubExtraFiles $StubExtraFiles -StubNested:$StubNested
  Set-Content -LiteralPath $harnessPath -Value $harness -Encoding utf8 -NoNewline

  $result = Invoke-Bash @((Get-PosixPath $harnessPath)) $workDir
  $log = if (Test-Path -LiteralPath $stubLog) { Get-Content -LiteralPath $stubLog -Raw } else { "" }
  $distDir = Join-Path $workDir "dist"
  $distFiles = if (Test-Path -LiteralPath $distDir) {
    @(Get-ChildItem -LiteralPath $distDir -Force | Select-Object -ExpandProperty Name)
  } else {
    @()
  }
  return [pscustomobject]@{
    Code      = $result.Code
    Output    = $result.Output
    StubLog   = $log
    DistFiles = $distFiles
  }
}

function Assert-ReleaseRejection([string]$Name, $Result, [string]$Marker) {
  Assert-Check ($Result.Code -ne 0) "$Name → 期望非零退出，实际 exit 0（输出：$($Result.Output.Trim())）"
  if ($Result.Code -ne 0) {
    Assert-Check ($Result.Output.Contains($Marker)) "$Name → 缺少拒绝原因 $Marker（输出：$($Result.Output.Trim())）"
  }
  Assert-Check (-not ($Result.StubLog -match '(?m)^release (create|edit|upload)')) `
    "$Name → 校验失败前已触发 create/edit/upload（桩日志：$($Result.StubLog.Trim())）"
}

# ---------------------------------------------------------------------------
# CI-C 平台证据对账夹具：证据是脱敏 JSON，阶段、runner 观测与 attempt 绑定均需可造。
# ---------------------------------------------------------------------------
$script:testRunId = "45899688832"
$script:testAttempt = "2"
$script:testSha = ("a" * 40) -join ""
# 主控在真实 run 的 /attempts/2 端点核对到的起点（复用 job 仍停在早于它的时间）。
$script:testAttemptStart = "2026-11-26T20:00:00Z"
$script:testStageStarted = "2026-11-26T20:01:00Z"
$script:testStageCompleted = "2026-11-26T20:05:00Z"
$script:testStaleCompleted = "2026-11-26T19:50:00Z"
$script:testStaleStarted = "2026-11-26T19:45:00Z"
$script:evidenceRunnerCatalog = @{
  "windows"     = @{ label = "windows-2025"; os = "windows"; arch = "x64" }
  "linux"       = @{ label = "ubuntu-24.04"; os = "linux"; arch = "x64" }
  "macos-arm64" = @{ label = "macos-15"; os = "macos"; arch = "arm64" }
  "macos-intel" = @{ label = "macos-15-intel"; os = "macos"; arch = "x64" }
}

function Get-TextSha256([string]$Text) {
  $bytes = [System.Text.Encoding]::UTF8.GetBytes($Text)
  $sha = [System.Security.Cryptography.SHA256]::Create()
  try {
    return ([System.BitConverter]::ToString($sha.ComputeHash($bytes)) -replace '-', '').ToLowerInvariant()
  } finally {
    $sha.Dispose()
  }
}

function New-StageRecord {
  param(
    [string]$Name,
    [string]$Status = "passed",
    [int]$ExitCode = 0,
    $CountsKnown = $false,
    $Counts = $null,
    [switch]$OmitCountsFlag
  )
  $record = [ordered]@{
    name = $Name; command = "cargo $($Name -replace '^cargo-', '')"; status = $Status
    exitCode = $ExitCode; durationMs = 1200
  }
  if (-not $OmitCountsFlag) { $record["countsKnown"] = $CountsKnown }
  $record["counts"] = $Counts
  return $record
}

function New-PlatformStages {
  param(
    [string]$Status = "passed",
    [int]$ExitCode = 0,
    [bool]$TestCountsKnown = $true,
    [int]$Passed = 156
  )
  $testCounts = if ($TestCountsKnown) {
    [ordered]@{ passed = $Passed; failed = 0; ignored = 5; measured = 0; filtered = 0 }
  } else {
    $null
  }
  return @(
    (New-StageRecord -Name "cargo-fetch" -Status $Status -ExitCode $ExitCode)
    (New-StageRecord -Name "cargo-clippy" -Status $Status -ExitCode $ExitCode)
    (New-StageRecord -Name "cargo-test" -Status $Status -ExitCode $ExitCode -CountsKnown $TestCountsKnown -Counts $testCounts)
  )
}

function New-PlatformEvidenceDoc {
  param(
    [string]$Platform = "windows",
    [string]$RunId = $script:testRunId,
    [string]$RunAttempt = $script:testAttempt,
    [string]$CheckoutSha = $script:testSha,
    [string]$PlanDigest = "",
    [string]$RunnerLabel = "",
    [string]$Os = "",
    [string]$Arch = "",
    [string]$ResultStatus = "passed",
    [string]$CompletedAt = $script:testStageCompleted,
    [string]$StartedAt = $script:testStageStarted,
    $Stages = $null
  )
  $catalog = $script:evidenceRunnerCatalog[$Platform]
  $labelValue = if ([string]::IsNullOrEmpty($RunnerLabel)) { $catalog.label } else { $RunnerLabel }
  $osValue = if ([string]::IsNullOrEmpty($Os)) { $catalog.os } else { $Os }
  $archValue = if ([string]::IsNullOrEmpty($Arch)) { $catalog.arch } else { $Arch }
  [object[]]$stageRows = $null
  if ($null -ne $Stages) { $stageRows = @($Stages) } else { $stageRows = @(New-PlatformStages) }
  return [ordered]@{
    schemaVersion = 1
    kind          = "soxu-ci-evidence/platform/v1"
    run           = [ordered]@{ id = $RunId; attempt = $RunAttempt; repository = "TanCan1998/Soxu"; event = "pull_request" }
    plan          = [ordered]@{ digest = $PlanDigest }
    identity      = [ordered]@{ baseSha = ("b" * 40) -join ""; headSha = ("c" * 40) -join ""; checkoutSha = $CheckoutSha; workflowRef = "stub/mirror/.github/workflows/ci.yml@refs/heads/main" }
    runner        = [ordered]@{ label = $labelValue; os = $osValue; arch = $archValue }
    platform      = $Platform
    job           = ""
    toolchain     = [ordered]@{ rust = "rustc 1.99.1"; cargo = "cargo 1.99.1"; node = ""; pnpm = "" }
    timing        = [ordered]@{ startedAt = $StartedAt; completedAt = $CompletedAt }
    stages        = $stageRows
    result        = [ordered]@{ status = $ResultStatus; reason = "" }
  }
}

function New-MatrixJobRow {
  param(
    [string]$Platform,
    [string]$Conclusion = "success",
    [string]$StartedAt = $script:testStageStarted,
    [string]$CompletedAt = $script:testStageCompleted
  )
  # 注意：jobs 列表会把沿用（复用）的 job 也标 run_attempt=2，所以只能靠原始时间分辨。
  return [ordered]@{
    name = "Rust ($Platform)"; conclusion = $Conclusion; status = "completed"
    started_at = $StartedAt; completed_at = $CompletedAt; run_attempt = 2
  }
}

function New-JobsJson([object[]]$Rows) {
  return (@{ total_count = $Rows.Count; jobs = @($Rows) } | ConvertTo-Json -Depth 8)
}

function Invoke-GateEvidence {
  param(
    [string]$PlanJson,
    [object[]]$Documents = @(),
    [string]$JobsJson = $null,
    [string]$RunId = $script:testRunId,
    [string]$RunAttempt = $script:testAttempt,
    [string]$CheckoutSha = $script:testSha,
    [string]$AttemptStartedAt = $script:testAttemptStart,
    [string]$VerifierOverride = ""
  )
  $dir = Join-Path $tempRoot ("evidence-" + [Guid]::NewGuid().ToString("N"))
  New-Item -ItemType Directory -Path $dir -Force | Out-Null
  $index = 0
  foreach ($document in $Documents) {
    $index++
    $sub = Join-Path $dir ("artifact-" + $index)
    New-Item -ItemType Directory -Path $sub -Force | Out-Null
    Set-Content -LiteralPath (Join-Path $sub "evidence.json") -Value ($document | ConvertTo-Json -Depth 12) -Encoding utf8 -NoNewline
  }
  $arguments = @(
    "-Mode", "Evidence", "-EvidenceDir", $dir,
    "-PlanPath", (Save-Fixture ("plan-evidence-" + [Guid]::NewGuid().ToString("N")) $PlanJson),
    "-RunId", $RunId, "-RunAttempt", $RunAttempt, "-CheckoutSha", $CheckoutSha
  )
  if (-not [string]::IsNullOrWhiteSpace($AttemptStartedAt)) { $arguments += @("-AttemptStartedAt", $AttemptStartedAt) }
  if ($null -ne $JobsJson) { $arguments += @("-JobsPath", (Save-Fixture ("jobs-" + [Guid]::NewGuid().ToString("N")) $JobsJson)) }
  $target = if ([string]::IsNullOrWhiteSpace($VerifierOverride)) { $verifierPath } else { $VerifierOverride }
  return Invoke-Script $target $arguments
}

# 证据夹具：作业键集合必须与固定 job 契约一致（含 CI-C 新增的 rust-format）。
$evidenceBaseline = @'
{
  "detect-changes": "success",
  "governance-and-privacy": "success",
  "rust-format": "success",
  "rust-check": { "instances": [
    { "id": "windows", "status": "success" },
    { "id": "linux", "status": "success" },
    { "id": "macos-arm64", "status": "success" }
  ] },
  "mobile-web-check": "skipped"
}
'@

$evidenceRustSkipped = @'
{
  "detect-changes": "success",
  "governance-and-privacy": "success",
  "rust-format": "skipped",
  "rust-check": "skipped",
  "mobile-web-check": "skipped"
}
'@

$evidenceGovSkipped = @'
{
  "detect-changes": "success",
  "governance-and-privacy": "skipped",
  "rust-format": "success",
  "rust-check": { "instances": [
    { "id": "windows", "status": "success" },
    { "id": "linux", "status": "success" },
    { "id": "macos-arm64", "status": "success" }
  ] },
  "mobile-web-check": "skipped"
}
'@

$evidenceMultiPartial = @'
{
  "detect-changes": "success",
  "governance-and-privacy": "success",
  "rust-format": "success",
  "rust-check": { "instances": [
    { "id": "windows", "status": "failure" },
    { "id": "linux", "status": "success" },
    { "id": "macos-arm64", "status": "success" }
  ] },
  "mobile-web-check": "skipped"
}
'@

$evidenceMultiMissing = @'
{
  "detect-changes": "success",
  "governance-and-privacy": "success",
  "rust-format": "success",
  "rust-check": { "instances": [
    { "id": "windows", "status": "success" },
    { "id": "linux", "status": "success" }
  ] },
  "mobile-web-check": "skipped"
}
'@

$evidenceMultiDuplicate = @'
{
  "detect-changes": "success",
  "governance-and-privacy": "success",
  "rust-format": "success",
  "rust-check": { "instances": [
    { "id": "windows", "status": "success" },
    { "id": "windows", "status": "success" },
    { "id": "linux", "status": "success" },
    { "id": "macos-arm64", "status": "success" }
  ] },
  "mobile-web-check": "skipped"
}
'@

$evidenceMultiStatusInvalid = @'
{
  "detect-changes": "success",
  "governance-and-privacy": "success",
  "rust-format": "success",
  "rust-check": { "instances": [
    { "id": "windows", "status": 200 },
    { "id": "linux", "status": "success" },
    { "id": "macos-arm64", "status": "success" }
  ] },
  "mobile-web-check": "skipped"
}
'@

# 矩阵作业被折叠为单一字符串：不再区分平台，必须拒绝。
$evidenceMultiFolded = @'
{
  "detect-changes": "success",
  "governance-and-privacy": "success",
  "rust-format": "success",
  "rust-check": "success",
  "mobile-web-check": "skipped"
}
'@

# 聚合字符串形式的 needs 结果：不含实例身份，必须拒绝当作逐实例证据。
$evidenceAggregateSuccess = @'
{
  "detect-changes": "success",
  "governance-and-privacy": "success",
  "rust-format": "success",
  "rust-check": "success",
  "mobile-web-check": "skipped"
}
'@

$evidenceAggregateSkipped = @'
{
  "detect-changes": "success",
  "governance-and-privacy": "success",
  "rust-format": "skipped",
  "rust-check": "skipped",
  "mobile-web-check": "skipped"
}
'@

$planBaseline = New-PlanJson
$planRustSkipped = New-PlanJson -Rust $false -RustScope "none" -RustRequired $false -ChangedFiles @("docs/ops/PROGRESS.md")
$planMobileRequired = New-PlanJson -Rust $false -RustScope "none" -RustRequired $false -MobileWeb $true -MobileRequired $true -ChangedFiles @("apps/mobile-web/src/main.tsx")
# 两/三平台矩阵：证据实例集必须与计划完全一致，少一个平台就是缺项。
$planMultiInstance = New-PlanJson

try {
  # ============ 一、证据与状态对账 ============
  Assert-Pass "通过：必跑 success + 计划内 skipped" (Invoke-GateVerify -PlanJson $planBaseline -EvidenceJson $evidenceBaseline)
  Assert-Pass "通过：rust 与 mobile 均按计划跳过" (Invoke-GateVerify -PlanJson $planRustSkipped -EvidenceJson $evidenceRustSkipped)

  $evidenceDetectFailed = @'
{
  "detect-changes": "failure",
  "governance-and-privacy": "skipped",
  "rust-format": "skipped",
  "rust-check": "skipped",
  "mobile-web-check": "skipped"
}
'@
  Assert-Fail "失败：检测器失败" (Invoke-GateVerify -PlanJson $planBaseline -EvidenceJson $evidenceDetectFailed) 'detector_not_success:detect-changes=failure'

  $evidenceCancelled = @'
{
  "detect-changes": "success",
  "governance-and-privacy": "success",
  "rust-format": "success",
  "rust-check": { "instances": [
    { "id": "windows", "status": "cancelled" },
    { "id": "linux", "status": "success" },
    { "id": "macos-arm64", "status": "success" }
  ] },
  "mobile-web-check": "skipped"
}
'@
  Assert-Fail "失败：必跑实例被取消" (Invoke-GateVerify -PlanJson $planBaseline -EvidenceJson $evidenceCancelled) 'required_not_success:rust-check/windows=cancelled'

  $evidenceMobileFailed = @'
{
  "detect-changes": "success",
  "governance-and-privacy": "success",
  "rust-format": "skipped",
  "rust-check": "skipped",
  "mobile-web-check": "failure"
}
'@
  Assert-Fail "失败：mobile 计划必跑但 failure" (Invoke-GateVerify -PlanJson $planMobileRequired -EvidenceJson $evidenceMobileFailed) 'required_not_success:mobile-web-check=failure'

  $evidenceInvalidStatus = @'
{
  "detect-changes": "success",
  "governance-and-privacy": "success",
  "rust-format": "success",
  "rust-check": { "instances": [
    { "id": "windows", "status": "ok" },
    { "id": "linux", "status": "success" },
    { "id": "macos-arm64", "status": "success" }
  ] },
  "mobile-web-check": "skipped"
}
'@
  Assert-Fail "失败：非法状态串" (Invoke-GateVerify -PlanJson $planBaseline -EvidenceJson $evidenceInvalidStatus) 'status_invalid:rust-check/windows=ok'

  $evidenceMissingJob = @'
{
  "detect-changes": "success",
  "governance-and-privacy": "success",
  "rust-format": "success",
  "mobile-web-check": "skipped"
}
'@
  Assert-Fail "失败：证据缺少作业" (Invoke-GateVerify -PlanJson $planBaseline -EvidenceJson $evidenceMissingJob) 'evidence_missing:rust-check'

  $evidenceUnknownJob = @'
{
  "detect-changes": "success",
  "governance-and-privacy": "success",
  "rust-format": "success",
  "rust-check": "success",
  "mobile-web-check": "skipped",
  "lint-check": "success"
}
'@
  Assert-Fail "失败：证据含未知作业" (Invoke-GateVerify -PlanJson $planBaseline -EvidenceJson $evidenceUnknownJob) 'evidence_unknown_job:lint-check'

  $evidenceMobileSuccessConflict = @'
{
  "detect-changes": "success",
  "governance-and-privacy": "success",
  "rust-format": "success",
  "rust-check": { "instances": [
    { "id": "windows", "status": "success" },
    { "id": "linux", "status": "success" },
    { "id": "macos-arm64", "status": "success" }
  ] },
  "mobile-web-check": "success"
}
'@
  Assert-Fail "失败：计划未要求却运行成功（计划与证据冲突）" (Invoke-GateVerify -PlanJson $planBaseline -EvidenceJson $evidenceMobileSuccessConflict) 'plan_evidence_conflict:mobile-web-check'

  # 多实例证据：缺项、部分失败、重复证据、非字符串状态、折叠为单值都必须拒绝
  Assert-Fail "失败：多实例部分失败" (Invoke-GateVerify -PlanJson $planMultiInstance -EvidenceJson $evidenceMultiPartial) 'required_not_success:rust-check/windows=failure'
  Assert-Fail "失败：多实例证据缺项" (Invoke-GateVerify -PlanJson $planMultiInstance -EvidenceJson $evidenceMultiMissing) 'instance_missing:rust-check/macos-arm64'
  Assert-Fail "失败：多实例重复证据" (Invoke-GateVerify -PlanJson $planMultiInstance -EvidenceJson $evidenceMultiDuplicate) 'instance_duplicate:rust-check/windows'
  Assert-Fail "失败：多实例被折叠为单一状态" (Invoke-GateVerify -PlanJson $planMultiInstance -EvidenceJson $evidenceMultiFolded) 'matrix_jobs_unavailable:rust-check'
  Assert-Fail "失败：实例状态非字符串" (Invoke-GateVerify -PlanJson $planMultiInstance -EvidenceJson $evidenceMultiStatusInvalid) 'instance_status_invalid:rust-check/windows'

  # 计划实例集不得被篡改：删平台、不声明实例、scope 与 instances 不同源都必须拒绝。
  $planInstancesDropped = New-PlanJson -RustInstances @("windows", "linux")
  Assert-Fail "失败：计划删除平台实例" (Invoke-GateVerify -PlanJson $planInstancesDropped -EvidenceJson $evidenceMultiMissing) 'plan_instances_conflict:rust-check'
  Assert-Fail "失败：计划未声明矩阵实例" (Invoke-GateVerify -PlanJson (New-PlanJson -OmitRustInstances) -EvidenceJson $evidenceBaseline) 'plan_instances_missing:rust-check'
  Assert-Fail "失败：scope 删除平台" (Invoke-GateVerify -PlanJson (New-PlanJson -ScopePlatforms @("windows", "linux")) -EvidenceJson $evidenceMultiPartial) 'plan_instances_conflict:rust-check'

  # ============ 二、计划契约：不得靠改写计划削弱门禁 ============
  # 绕过 1：把无条件必跑的治理任务改成非必跑
  $planGovernanceDowngraded = New-PlanJson -GovernanceRequired $false
  Assert-Fail "绕过 1：治理任务 required 被翻转" (Invoke-GateVerify -PlanJson $planGovernanceDowngraded -EvidenceJson $evidenceGovSkipped) 'plan_job_required_conflict:governance-and-privacy'

  # 绕过 2：整体替换 jobs 集合为虚构作业
  $planInventedJobs = @'
{
  "version": "ci-gate-plan/v1",
  "detector": "detect-changes",
  "gate": "ci-gate",
  "jobs": [ { "id": "invented-job", "required": false } ],
  "selfTest": false,
  "releaseContract": false,
  "scope": {
    "full": false,
    "rust": false,
    "rustScope": "none",
    "mobileWeb": false,
    "paseoOrchestration": false,
    "checkScopeSelfTest": false,
    "platforms": [],
    "changedFiles": ["docs/ops/PROGRESS.md"]
  }
}
'@
  $evidenceInventedJobs = @'
{
  "detect-changes": "success",
  "invented-job": "skipped"
}
'@
  Assert-Fail "绕过 2：jobs 被替换为虚构作业" (Invoke-GateVerify -PlanJson $planInventedJobs -EvidenceJson $evidenceInventedJobs) 'plan_job_unknown:invented-job'

  # 绕过 3：条件作业的 required 与 scope 不一致
  $planMobileRequiredButScopeFalse = New-PlanJson -MobileRequired $true -MobileWeb $false
  Assert-Fail "绕过 3：mobile required 与 scope 矛盾" (Invoke-GateVerify -PlanJson $planMobileRequiredButScopeFalse -EvidenceJson $evidenceBaseline) 'plan_job_required_conflict:mobile-web-check'

  # 绕过 4：篡改 depends_on
  $planDependsTampered = New-PlanJson -DependsOnRust "mobile_web"
  Assert-Fail "绕过 4：depends_on 被篡改" (Invoke-GateVerify -PlanJson $planDependsTampered -EvidenceJson $evidenceBaseline) 'plan_job_depends_conflict:rust-check'

  # 绕过 5：触发标志与 changedFiles 冲突（触及 ci.yml 却声明 selfTest=false）
  $planSelfTestSuppressed = New-PlanJson -ChangedFiles @(".github/workflows/ci.yml")
  Assert-Fail "绕过 5：selfTest 触发标志被压制" (Invoke-GateVerify -PlanJson $planSelfTestSuppressed -EvidenceJson $evidenceBaseline) 'plan_flag_conflict:selfTest'

  # 绕过 6：scope 自身矛盾（rust=false 但 rustScope=workspace）
  $planScopeContradiction = New-PlanJson -Rust $false -RustRequired $false -RustScope "workspace"
  Assert-Fail "绕过 6：计划 scope 内部矛盾" (Invoke-GateVerify -PlanJson $planScopeContradiction -EvidenceJson $evidenceRustSkipped) 'plan_scope_invalid'

  # 绕过 7：当前版必需字段缺失
  Assert-Fail "失败：计划缺少 gate 字段" (Invoke-GateVerify -PlanJson (New-PlanJson -OmitGate) -EvidenceJson $evidenceBaseline) 'plan_gate_missing'
  Assert-Fail "失败：计划缺少 scope" (Invoke-GateVerify -PlanJson (New-PlanJson -OmitScope) -EvidenceJson $evidenceBaseline) 'plan_scope_missing'
  Assert-Fail "失败：计划缺少 selfTest" (Invoke-GateVerify -PlanJson (New-PlanJson -OmitSelfTest) -EvidenceJson $evidenceBaseline) 'plan_flag_missing:selfTest'
  Assert-Fail "失败：scope 缺少 changedFiles" (Invoke-GateVerify -PlanJson (New-PlanJson -OmitChangedFiles) -EvidenceJson $evidenceBaseline) 'plan_scope_invalid'

  # 绕过 8：畸形实例声明不得被强制字符串化后接受
  $planInstancesWithEmpty = @'
{
  "version": "ci-gate-plan/v1",
  "detector": "detect-changes",
  "gate": "ci-gate",
  "jobs": [
    { "id": "governance-and-privacy", "required": true },
    { "id": "rust-check", "required": true, "depends_on": "rust", "instances": ["ubuntu-latest", ""] },
    { "id": "mobile-web-check", "required": false, "depends_on": "mobile_web" }
  ],
  "selfTest": false,
  "releaseContract": false,
  "scope": { "full": false, "rust": true, "rustScope": "crates", "mobileWeb": false, "paseoOrchestration": false, "checkScopeSelfTest": false, "platforms": ["windows", "linux", "macos-arm64"], "changedFiles": ["crates/soxu-git/src/probe.rs"] }
}
'@
  Assert-Fail "失败：实例声明含空串" (Invoke-GateVerify -PlanJson $planInstancesWithEmpty -EvidenceJson $evidenceMultiMissing) 'plan_instances_invalid:rust-check'

  $planInstancesWithNumber = @'
{
  "version": "ci-gate-plan/v1",
  "detector": "detect-changes",
  "gate": "ci-gate",
  "jobs": [
    { "id": "governance-and-privacy", "required": true },
    { "id": "rust-check", "required": true, "depends_on": "rust", "instances": [1] },
    { "id": "mobile-web-check", "required": false, "depends_on": "mobile_web" }
  ],
  "selfTest": false,
  "releaseContract": false,
  "scope": { "full": false, "rust": true, "rustScope": "crates", "mobileWeb": false, "paseoOrchestration": false, "checkScopeSelfTest": false, "platforms": ["windows", "linux", "macos-arm64"], "changedFiles": ["crates/soxu-git/src/probe.rs"] }
}
'@
  Assert-Fail "失败：实例声明为数字" (Invoke-GateVerify -PlanJson $planInstancesWithNumber -EvidenceJson $evidenceMultiMissing) 'plan_instances_invalid:rust-check'

  $planInstancesNull = @'
{
  "version": "ci-gate-plan/v1",
  "detector": "detect-changes",
  "gate": "ci-gate",
  "jobs": [
    { "id": "governance-and-privacy", "required": true },
    { "id": "rust-check", "required": true, "depends_on": "rust", "instances": null },
    { "id": "mobile-web-check", "required": false, "depends_on": "mobile_web" }
  ],
  "selfTest": false,
  "releaseContract": false,
  "scope": { "full": false, "rust": true, "rustScope": "crates", "mobileWeb": false, "paseoOrchestration": false, "checkScopeSelfTest": false, "platforms": ["windows", "linux", "macos-arm64"], "changedFiles": ["crates/soxu-git/src/probe.rs"] }
}
'@
  Assert-Fail "失败：实例声明为 null" (Invoke-GateVerify -PlanJson $planInstancesNull -EvidenceJson $evidenceMultiMissing) 'plan_instances_null:rust-check'

  # 其余计划结构负例
  Assert-Fail "失败：计划缺失" (Invoke-GateVerify -PlanJson $null -EvidenceJson $evidenceBaseline) 'CI-GATE-ERROR' 2
  Assert-Fail "失败：计划不是合法 JSON" (Invoke-GateVerify -PlanJson '{ "version": "ci-gate-plan/v1", ' -EvidenceJson $evidenceBaseline) 'CI-GATE-ERROR' 2

  $planEmptyJobs = @'
{
  "version": "ci-gate-plan/v1",
  "detector": "detect-changes",
  "gate": "ci-gate",
  "jobs": [],
  "selfTest": false,
  "releaseContract": false,
  "scope": { "full": false, "rust": false, "rustScope": "none", "mobileWeb": false, "paseoOrchestration": false, "checkScopeSelfTest": false, "platforms": [], "changedFiles": ["docs/ops/PROGRESS.md"] }
}
'@
  Assert-Fail "失败：计划作业为空" (Invoke-GateVerify -PlanJson $planEmptyJobs -EvidenceJson $evidenceBaseline) 'plan_jobs_empty'

  $planDuplicateJob = @'
{
  "version": "ci-gate-plan/v1",
  "detector": "detect-changes",
  "gate": "ci-gate",
  "jobs": [
    { "id": "governance-and-privacy", "required": true },
    { "id": "rust-check", "required": true, "depends_on": "rust" },
    { "id": "rust-check", "required": false, "depends_on": "rust" },
    { "id": "mobile-web-check", "required": false, "depends_on": "mobile_web" }
  ],
  "selfTest": false,
  "releaseContract": false,
  "scope": { "full": false, "rust": true, "rustScope": "crates", "mobileWeb": false, "paseoOrchestration": false, "checkScopeSelfTest": false, "platforms": ["windows", "linux", "macos-arm64"], "changedFiles": ["crates/soxu-git/src/probe.rs"] }
}
'@
  Assert-Fail "失败：计划重复作业" (Invoke-GateVerify -PlanJson $planDuplicateJob -EvidenceJson $evidenceBaseline) 'plan_job_duplicate:rust-check'

  Assert-Fail "失败：计划版本非法" (Invoke-GateVerify -PlanJson (New-PlanJson -Version "ci-gate-plan/v2") -EvidenceJson $evidenceBaseline) 'plan_version_invalid'

  $planRequiredNotBoolean = @'
{
  "version": "ci-gate-plan/v1",
  "detector": "detect-changes",
  "gate": "ci-gate",
  "jobs": [
    { "id": "governance-and-privacy", "required": "true" }
  ],
  "selfTest": false,
  "releaseContract": false,
  "scope": { "full": false, "rust": false, "rustScope": "none", "mobileWeb": false, "paseoOrchestration": false, "checkScopeSelfTest": false, "platforms": [], "changedFiles": ["docs/ops/PROGRESS.md"] }
}
'@
  Assert-Fail "失败：计划必跑标记不是布尔" (Invoke-GateVerify -PlanJson $planRequiredNotBoolean -EvidenceJson $evidenceBaseline) 'plan_job_required_invalid:governance-and-privacy'

  # ============ 三、Plan 模式的检测器输出一致性 ============
  $planResult = Invoke-PlanMode (New-ScopeJson -Rust $true -RustScope "crates")
  Assert-Pass "通过：Plan 模式构建计划" $planResult
  $planObject = $null
  if ($planResult.Code -eq 0) {
    try { $planObject = ($planResult.Output.Trim() | ConvertFrom-Json -Depth 10) } catch { $planObject = $null }
  }
  Assert-Check ($null -ne $planObject) "Plan 模式输出必须是可解析的单行 JSON：$($planResult.Output.Trim())"
  if ($null -ne $planObject) {
    Assert-Check ($planObject.version -eq "ci-gate-plan/v1") "Plan 模式版本应为 ci-gate-plan/v1"
    $rustJob = @($planObject.jobs | Where-Object { $_.id -eq "rust-check" })[0]
    $mobileJob = @($planObject.jobs | Where-Object { $_.id -eq "mobile-web-check" })[0]
    $govJob = @($planObject.jobs | Where-Object { $_.id -eq "governance-and-privacy" })[0]
    Assert-Check ($null -ne $rustJob -and $rustJob.required -eq $true) "rust 命中时 rust-check 必须必跑"
    Assert-Check ($null -ne $mobileJob -and $mobileJob.required -eq $false) "mobileWeb 未命中时 mobile-web-check 不得必跑"
    Assert-Check ($null -ne $govJob -and $govJob.required -eq $true) "治理隐私任务必须无条件必跑"
    Assert-Check ($planObject.selfTest -eq $false) "未触及门禁脚本时 selfTest 应为 false"
    Assert-Check ($planObject.releaseContract -eq $false) "未触及 release.yml 时 releaseContract 应为 false"
    Assert-Check ($null -ne $planObject.scope.changedFiles) "计划 scope 必须回写 changedFiles 以便 Verify 重算"
    Assert-Pass "通过：Plan 产物可直接供 Verify 消费" (Invoke-GateVerify -PlanJson ($planObject | ConvertTo-Json -Depth 10 -Compress) -EvidenceJson $evidenceBaseline)
  }

  $releasePlan = Invoke-PlanMode (New-ScopeJson -Rust $false -RustScope "none" -ChangedFiles @(".github/workflows/release.yml"))
  Assert-Pass "通过：触及 release.yml 时构建计划" $releasePlan
  if ($releasePlan.Code -eq 0) {
    $releasePlanObject = ($releasePlan.Output.Trim() | ConvertFrom-Json -Depth 10)
    Assert-Check ($releasePlanObject.releaseContract -eq $true) "触及 release.yml 时 releaseContract 必须为 true"
    Assert-Check ($releasePlanObject.selfTest -eq $false) "仅触及 release.yml 时 selfTest 应为 false"
  }

  $ciPlan = Invoke-PlanMode (New-ScopeJson -Rust $false -RustScope "none" -ChangedFiles @(".github/workflows/ci.yml", "scripts/verify-ci-gate.ps1"))
  Assert-Pass "通过：触及门禁脚本时构建计划" $ciPlan
  if ($ciPlan.Code -eq 0) {
    $ciPlanObject = ($ciPlan.Output.Trim() | ConvertFrom-Json -Depth 10)
    Assert-Check ($ciPlanObject.selfTest -eq $true) "触及 ci.yml 或门禁脚本时 selfTest 必须为 true"
  }

  # 检测器输出自身矛盾必须失败关闭，不能生成跳过 Rust/PWA 的计划
  Assert-Fail "失败：rust=false 但 rustScope=workspace" (Invoke-PlanMode (New-ScopeJson -Rust $false -RustScope "workspace")) 'CI-GATE-ERROR' 2
  Assert-Fail "失败：rust=true 但 rustScope=none" (Invoke-PlanMode (New-ScopeJson -Rust $true -RustScope "none")) 'CI-GATE-ERROR' 2
  Assert-Fail "失败：rustScope 非法取值" (Invoke-PlanMode (New-ScopeJson -Rust $true -RustScope "everything")) 'CI-GATE-ERROR' 2
  Assert-Fail "失败：full=true 却跳过 Rust" (Invoke-PlanMode (New-ScopeJson -Full $true -Rust $false -RustScope "none" -MobileWeb $true -Paseo $true -ScopeSelfTest $true)) 'CI-GATE-ERROR' 2
  Assert-Fail "失败：full=true 却跳过 PWA/Paseo" (Invoke-PlanMode (New-ScopeJson -Full $true -Rust $true -RustScope "workspace" -MobileWeb $false -Paseo $false -ScopeSelfTest $false)) 'CI-GATE-ERROR' 2
  Assert-Fail "失败：changedFiles 缺失" (Invoke-PlanMode (New-ScopeJson -OmitChangedFiles)) 'CI-GATE-ERROR' 2

  $scopeInvalidType = @'
{
  "full": false,
  "rust": "true",
  "rustScope": "crates",
  "mobileWeb": false,
  "paseoOrchestration": false,
  "checkScopeSelfTest": false,
  "changedFiles": ["crates/soxu-git/src/probe.rs"]
}
'@
  Assert-Fail "失败：检测器字段类型非法" (Invoke-PlanMode $scopeInvalidType) 'CI-GATE-ERROR' 2

  $scopeNullChangedFile = @'
{
  "full": false,
  "rust": true,
  "rustScope": "crates",
  "mobileWeb": false,
  "paseoOrchestration": false,
  "checkScopeSelfTest": false,
  "changedFiles": ["crates/soxu-git/src/probe.rs", null]
}
'@
  Assert-Fail "失败：changedFiles 含 null" (Invoke-PlanMode $scopeNullChangedFile) 'CI-GATE-ERROR' 2

  $scopeEmptyChangedFile = @'
{
  "full": false,
  "rust": true,
  "rustScope": "crates",
  "mobileWeb": false,
  "paseoOrchestration": false,
  "checkScopeSelfTest": false,
  "changedFiles": [""]
}
'@
  Assert-Fail "失败：changedFiles 含空路径" (Invoke-PlanMode $scopeEmptyChangedFile) 'CI-GATE-ERROR' 2

  # ============ 三·补、路径契约保真：旧 Windows 入口 vs POSIX 字面入口 ============
  # 旧无版本 scope 继续接受既有 Windows 路径格式，归一化为仓库相对规范路径。
  $legacyPathScope = New-ScopeJson -Rust $false -RustScope "none" -ChangedFiles @('docs\ops\LESSONS.md')
  $legacyPathPlan = Invoke-PlanMode $legacyPathScope
  Assert-Pass "通过：旧无版本 scope 的 Windows 路径格式" $legacyPathPlan
  if ($legacyPathPlan.Code -eq 0) {
    $legacyPathObject = ($legacyPathPlan.Output.Trim() | ConvertFrom-Json -Depth 10)
    Assert-Check ($legacyPathObject.pathContract -eq 'repo-relative/legacy') "旧 scope 计划必须声明 legacy 路径契约"
    Assert-Check (@($legacyPathObject.scope.changedFiles).Count -eq 1) "旧 scope 路径不得被拆成多个条目"
    Assert-Check ($legacyPathObject.scope.changedFiles[0] -eq 'docs/ops/LESSONS.md') "旧 Windows 路径归一化为仓库相对规范路径"
    Assert-Check ($legacyPathObject.releaseContract -eq $false) "旧 scope 的普通文档路径不触发发布专项"
  }

  # 版本化 scope：POSIX 合法的字面反斜杠必须原样贯穿 Plan 与 Verify。
  $literalScope = New-ScopeJson -Rust $false -RustScope "none" -ChangedFiles @('docs/literal\backslash.md') -ScopeSchemaVersion 1
  $literalPlan = Invoke-PlanMode $literalScope
  Assert-Pass "通过：版本化 scope 的 POSIX 字面反斜杠路径" $literalPlan
  if ($literalPlan.Code -eq 0) {
    $literalObject = ($literalPlan.Output.Trim() | ConvertFrom-Json -Depth 10)
    Assert-Check ($literalObject.pathContract -eq 'repo-relative/v1') "版本化 scope 计划必须声明 v1 路径契约"
    Assert-Check ($literalObject.scope.changedFiles[0] -eq 'docs/literal\backslash.md') "字面反斜杠在计划中原样保留，不被改写成分隔符"
    Assert-Check ($literalObject.selfTest -eq $false) "字面反斜杠路径不得误触发门禁专项"
    Assert-Check ($literalObject.releaseContract -eq $false) "字面反斜杠路径不得误触发发布专项"
    Assert-Pass "通过：版本化计划与证据重算一致（Verify 同源推导）" (Invoke-GateVerify -PlanJson ($literalObject | ConvertTo-Json -Depth 10) -EvidenceJson $evidenceRustSkipped)
  }

  # 路径身份碰撞：同一串字面反斜杠输入，在两个入口下必须得到不同结论。
  # 旧无版本 scope 把它当作 Windows 分隔符写法，归一化后就是仓库里真实存在的门禁脚本；
  # 版本化 scope 认为它是一个字面文件名，不得冒用门禁专项。
  $collisionGate = Invoke-PlanMode (New-ScopeJson -Rust $false -RustScope "none" -ChangedFiles @('scripts\verify-ci-gate.ps1'))
  if ($collisionGate.Code -eq 0) {
    $collisionGateObject = ($collisionGate.Output.Trim() | ConvertFrom-Json -Depth 10)
    Assert-Check ($collisionGateObject.selfTest -eq $true) "旧 scope 的 Windows 路径写法确实指向真实门禁脚本"
  }
  $collisionGateVersioned = Invoke-PlanMode (New-ScopeJson -Rust $false -RustScope "none" -ChangedFiles @('scripts\verify-ci-gate.ps1') -ScopeSchemaVersion 1)
  if ($collisionGateVersioned.Code -eq 0) {
    $collisionGateVersionedObject = ($collisionGateVersioned.Output.Trim() | ConvertFrom-Json -Depth 10)
    Assert-Check ($collisionGateVersionedObject.scope.changedFiles[0] -eq 'scripts\verify-ci-gate.ps1') "版本化 scope 保留字面反斜杠路径原样"
    Assert-Check ($collisionGateVersionedObject.selfTest -eq $false) "版本化 scope 的字面反斜杠路径不得冒用门禁专项"
  }

  $collisionRelease = Invoke-PlanMode (New-ScopeJson -Rust $false -RustScope "none" -ChangedFiles @('.github\workflows\release.yml'))
  if ($collisionRelease.Code -eq 0) {
    $collisionReleaseObject = ($collisionRelease.Output.Trim() | ConvertFrom-Json -Depth 10)
    Assert-Check ($collisionReleaseObject.releaseContract -eq $true) "旧 scope 的 Windows 路径写法确实指向真实发布工作流"
  }
  $collisionReleaseVersioned = Invoke-PlanMode (New-ScopeJson -Rust $false -RustScope "none" -ChangedFiles @('.github\workflows\release.yml') -ScopeSchemaVersion 1)
  if ($collisionReleaseVersioned.Code -eq 0) {
    $collisionReleaseVersionedObject = ($collisionReleaseVersioned.Output.Trim() | ConvertFrom-Json -Depth 10)
    Assert-Check ($collisionReleaseVersionedObject.releaseContract -eq $false) "版本化 scope 的字面反斜杠路径不得冒用发布专项"
  }

  # 真实 release.yml 仍必须触发发布专项，只有它才算发布入口。
  $realRelease = Invoke-PlanMode (New-ScopeJson -Rust $false -RustScope "none" -ChangedFiles @('.github/workflows/release.yml'))
  if ($realRelease.Code -eq 0) {
    $realReleaseObject = ($realRelease.Output.Trim() | ConvertFrom-Json -Depth 10)
    Assert-Check ($realReleaseObject.releaseContract -eq $true) "真实的 .github/workflows/release.yml 修改仍触发发布专项"
  }

  # 路径契约不得被自封或降级：scope 与计划必须同源。
  $planContractForged = New-PlanJson -Rust $false -RustScope "none" -ChangedFiles @('docs/ops/PROGRESS.md') -PathContractOverride 'repo-relative/v1'
  Assert-Fail "失败：旧 scope 自封 v1 路径契约" (Invoke-GateVerify -PlanJson $planContractForged -EvidenceJson $evidenceRustSkipped) 'plan_path_contract_conflict'
  $planContractMissing = New-PlanJson -Rust $false -RustScope "none" -ChangedFiles @('docs/ops/PROGRESS.md') -OmitPathContract
  Assert-Fail "失败：计划缺少路径契约" (Invoke-GateVerify -PlanJson $planContractMissing -EvidenceJson $evidenceRustSkipped) 'plan_path_contract_missing'

  $scopeVersionedAbsolute = @'
{
  "schemaVersion": 1,
  "full": false,
  "rust": false,
  "rustScope": "none",
  "mobileWeb": false,
  "paseoOrchestration": false,
  "checkScopeSelfTest": false,
  "changedFiles": ["/etc/passwd"]
}
'@
  Assert-Fail "失败：版本化 scope 接受绝对路径" (Invoke-PlanMode $scopeVersionedAbsolute) 'CI-GATE-ERROR' 2

  # ============ 三·二、CI-C 平台证据对账（Evidence 模式） ============
  $evPlan = (New-PlanJson).Trim()
  $evDigest = Get-TextSha256 $evPlan
  $evPlatforms = @("windows", "linux", "macos-arm64")

  $goodDocs = @(foreach ($p in $evPlatforms) { New-PlatformEvidenceDoc -Platform $p -PlanDigest $evDigest })
  $goodJobs = New-JobsJson @(foreach ($p in $evPlatforms) { New-MatrixJobRow $p })

  Assert-Pass "证据通过：三平台均有 fetch/clippy/test 通过阶段且矩阵 job 绿" `
    (Invoke-GateEvidence -PlanJson $evPlan -Documents $goodDocs -JobsJson $goodJobs)

  # 计数未知是合法表态：不伪造 0，也不因此失败。
  $unknownCountsDocs = @(foreach ($p in $evPlatforms) {
      New-PlatformEvidenceDoc -Platform $p -PlanDigest $evDigest -Stages (New-PlatformStages -TestCountsKnown $false)
    })
  Assert-Pass "证据通过：测试计数未知（countsKnown=false）不伪造 0" `
    (Invoke-GateEvidence -PlanJson $evPlan -Documents $unknownCountsDocs -JobsJson $goodJobs)

  # 主控发现：仅有身份、runner 和 result.status=passed、完全没有 stages 的 JSON 不得通过。
  # 注意：空数组作为未声明参数传入会被绑成 $null，所以直接就地赋空数组。
  $emptyStagesDocs = @(foreach ($p in $evPlatforms) {
      $doc = New-PlatformEvidenceDoc -Platform $p -PlanDigest $evDigest
      $doc["stages"] = @()
      $doc
    })
  Assert-Fail "证据失败：空 stages 且 jobs 全绿" `
    (Invoke-GateEvidence -PlanJson $evPlan -Documents $emptyStagesDocs -JobsJson $goodJobs) 'evidence_stages_empty:windows'

  $missingStageDocs = @(foreach ($p in $evPlatforms) {
      New-PlatformEvidenceDoc -Platform $p -PlanDigest $evDigest -Stages @(
        (New-StageRecord -Name 'cargo-fetch')
        (New-StageRecord -Name 'cargo-clippy')
      )
    })
  Assert-Fail "证据失败：缺少 test 阶段" `
    (Invoke-GateEvidence -PlanJson $evPlan -Documents $missingStageDocs -JobsJson $goodJobs) 'evidence_stage_missing:windows/cargo-test'

  $duplicateStageDocs = @(foreach ($p in $evPlatforms) {
      New-PlatformEvidenceDoc -Platform $p -PlanDigest $evDigest -Stages (@(New-PlatformStages) + @(New-StageRecord -Name 'cargo-clippy'))
    })
  Assert-Fail "证据失败：阶段重复上报" `
    (Invoke-GateEvidence -PlanJson $evPlan -Documents $duplicateStageDocs -JobsJson $goodJobs) 'evidence_stage_duplicate:windows/cargo-clippy'

  $failedStageDocs = @(foreach ($p in $evPlatforms) {
      New-PlatformEvidenceDoc -Platform $p -PlanDigest $evDigest -ResultStatus 'failed' -Stages @(
        (New-StageRecord -Name 'cargo-fetch')
        (New-StageRecord -Name 'cargo-clippy')
        (New-StageRecord -Name 'cargo-test' -Status 'test_failed' -ExitCode 101 -CountsKnown $true -Counts ([ordered]@{ passed = 150; failed = 6; ignored = 5; measured = 0; filtered = 0 }))
      )
    })
  $failedStageResult = Invoke-GateEvidence -PlanJson $evPlan -Documents $failedStageDocs -JobsJson (New-JobsJson @(foreach ($p in $evPlatforms) { New-MatrixJobRow $p -Conclusion 'failure' }))
  Assert-Fail "证据失败：测试阶段失败" $failedStageResult 'evidence_stage_not_passed:windows/cargo-test=test_failed'
  Assert-Fail "证据失败：失败阶段退出码非零" $failedStageResult 'evidence_stage_exit_code:windows/cargo-test=101'

  $noFlagDocs = @(foreach ($p in $evPlatforms) {
      New-PlatformEvidenceDoc -Platform $p -PlanDigest $evDigest -Stages @(
        (New-StageRecord -Name 'cargo-fetch')
        (New-StageRecord -Name 'cargo-clippy')
        (New-StageRecord -Name 'cargo-test' -OmitCountsFlag)
      )
    })
  Assert-Fail "证据失败：缺少计数表态（countsKnown）" `
    (Invoke-GateEvidence -PlanJson $evPlan -Documents $noFlagDocs -JobsJson $goodJobs) 'evidence_stage_counts_flag_invalid:windows/cargo-test'

  $claimKnownNoCountsDocs = @(foreach ($p in $evPlatforms) {
      New-PlatformEvidenceDoc -Platform $p -PlanDigest $evDigest -Stages @(
        (New-StageRecord -Name 'cargo-fetch')
        (New-StageRecord -Name 'cargo-clippy')
        (New-StageRecord -Name 'cargo-test' -CountsKnown $true -Counts $null)
      )
    })
  Assert-Fail "证据失败：声明已统计但无计数" `
    (Invoke-GateEvidence -PlanJson $evPlan -Documents $claimKnownNoCountsDocs -JobsJson $goodJobs) 'evidence_stage_counts_missing:windows/cargo-test'

  $unknownButCountedDocs = @(foreach ($p in $evPlatforms) {
      New-PlatformEvidenceDoc -Platform $p -PlanDigest $evDigest -Stages @(
        (New-StageRecord -Name 'cargo-fetch')
        (New-StageRecord -Name 'cargo-clippy')
        (New-StageRecord -Name 'cargo-test' -CountsKnown $false -Counts ([ordered]@{ passed = 0; failed = 0; ignored = 0; measured = 0; filtered = 0 }))
      )
    })
  Assert-Fail "证据失败：计数未知却填了 0" `
    (Invoke-GateEvidence -PlanJson $evPlan -Documents $unknownButCountedDocs -JobsJson $goodJobs) 'evidence_stage_counts_conflict:windows/cargo-test'

  # passed 阶段却带失败计数是矛盾证据：计数已统计且 failed>0，就不可能通过。
  # 计数未知（countsKnown=false）仍是合法表态，不在此列。
  $contradictionCountDocs = @(foreach ($p in $evPlatforms) {
      New-PlatformEvidenceDoc -Platform $p -PlanDigest $evDigest -Stages @(
        (New-StageRecord -Name 'cargo-fetch')
        (New-StageRecord -Name 'cargo-clippy')
        (New-StageRecord -Name 'cargo-test' -Status 'passed' -ExitCode 0 -CountsKnown $true -Counts ([ordered]@{ passed = 150; failed = 6; ignored = 5; measured = 0; filtered = 0 }))
      )
    })
  Assert-Fail "证据失败：passed 阶段却带失败计数" `
    (Invoke-GateEvidence -PlanJson $evPlan -Documents $contradictionCountDocs -JobsJson $goodJobs) 'evidence_stage_counts_contradiction:windows/cargo-test'

  # 平台集合：缺项、重复、未知平台
  $missingPlatform = Invoke-GateEvidence -PlanJson $evPlan -Documents @($goodDocs[0], $goodDocs[1]) -JobsJson $goodJobs
  Assert-Fail "证据失败：缺少必要平台" $missingPlatform 'evidence_platform_missing:macos-arm64'
  Assert-Check ($missingPlatform.Output.Contains('CI-C-EVIDENCE-GUIDANCE')) "证据缺少平台时必须提示全量重跑（实际输出：$($missingPlatform.Output.Trim()))"

  Assert-Fail "证据失败：同一平台多份证据" `
    (Invoke-GateEvidence -PlanJson $evPlan -Documents ($goodDocs + @($goodDocs[0])) -JobsJson $goodJobs) 'evidence_platform_duplicate:windows'

  Assert-Fail "证据失败：未知平台证据" `
    (Invoke-GateEvidence -PlanJson $evPlan -Documents ($goodDocs + @((New-PlatformEvidenceDoc -Platform 'freebsd' -PlanDigest $evDigest))) -JobsJson $goodJobs) 'evidence_platform_unknown:freebsd'

  # 事件身份：sha / attempt / 计划摘要
  Assert-Fail "证据失败：checkout SHA 不一致" `
    (Invoke-GateEvidence -PlanJson $evPlan -Documents @(New-PlatformEvidenceDoc -Platform 'windows' -PlanDigest $evDigest -CheckoutSha (("d" * 40) -join ""), $goodDocs[1], $goodDocs[2]) -JobsJson $goodJobs) 'evidence_sha_mismatch'

  Assert-Fail "证据失败：attempt 不一致" `
    (Invoke-GateEvidence -PlanJson $evPlan -Documents @(New-PlatformEvidenceDoc -Platform 'windows' -PlanDigest $evDigest -RunAttempt '1', $goodDocs[1], $goodDocs[2]) -JobsJson $goodJobs) 'evidence_attempt_mismatch'

  Assert-Fail "证据失败：计划摘要不一致" `
    (Invoke-GateEvidence -PlanJson $evPlan -Documents @(New-PlatformEvidenceDoc -Platform 'windows' -PlanDigest ("0" * 64), $goodDocs[1], $goodDocs[2]) -JobsJson $goodJobs) 'evidence_plan_mismatch'

  # Runner 观测：label / OS / arch 都必须与平台目录一致，不得用交叉编译或 Rosetta 冒充
  Assert-Fail "证据失败：runner 标签与平台不符" `
    (Invoke-GateEvidence -PlanJson $evPlan -Documents @(New-PlatformEvidenceDoc -Platform 'windows' -PlanDigest $evDigest -RunnerLabel 'ubuntu-24.04', $goodDocs[1], $goodDocs[2]) -JobsJson $goodJobs) 'evidence_runner_mismatch:windows'

  Assert-Fail "证据失败：OS 观测与平台不符" `
    (Invoke-GateEvidence -PlanJson $evPlan -Documents @(New-PlatformEvidenceDoc -Platform 'windows' -PlanDigest $evDigest -Os 'linux', $goodDocs[1], $goodDocs[2]) -JobsJson $goodJobs) 'evidence_runner_os_mismatch:windows'

  Assert-Fail "证据失败：ARM64 被当成 Intel" `
    (Invoke-GateEvidence -PlanJson $evPlan -Documents @(New-PlatformEvidenceDoc -Platform 'macos-arm64' -PlanDigest $evDigest -Arch 'x64', $goodDocs[0], $goodDocs[1]) -JobsJson $goodJobs) 'evidence_runner_arch_mismatch:macos-arm64'

  # 结果状态：部分失败、取消、未跑都不得充当通过
  Assert-Fail "证据失败：平台结果非 passed" `
    (Invoke-GateEvidence -PlanJson $evPlan -Documents @(New-PlatformEvidenceDoc -Platform 'windows' -PlanDigest $evDigest -ResultStatus 'partial', $goodDocs[1], $goodDocs[2]) -JobsJson $goodJobs) 'evidence_not_passed:windows=partial'

  # 部分重跑：复用前 attempt 的证据必须失败关闭，并提示全量重跑
  $reusedDocs = @(foreach ($p in $evPlatforms) {
      New-PlatformEvidenceDoc -Platform $p -PlanDigest $evDigest -CompletedAt $script:testStaleCompleted -StartedAt $script:testStaleStarted
    })
  $reusedResult = Invoke-GateEvidence -PlanJson $evPlan -Documents $reusedDocs -JobsJson $goodJobs
  Assert-Fail "证据失败：沿用前 attempt 的已完成时间" $reusedResult 'evidence_from_previous_attempt'
  Assert-Check ($reusedResult.Output.Contains('CI-C-EVIDENCE-GUIDANCE')) "复用旧证据时必须提示全量重跑（实际输出：$($reusedResult.Output.Trim()))"

  Assert-Fail "证据失败：缺 attempt 起点无法区分新旧执行" `
    (Invoke-GateEvidence -PlanJson $evPlan -Documents $goodDocs -JobsJson $goodJobs -AttemptStartedAt '') 'attempt_started_at_missing'

  Assert-Fail "证据失败：证据目录为空" `
    (Invoke-GateEvidence -PlanJson $evPlan -Documents @() -JobsJson $goodJobs) 'evidence_dir_empty'

  # 原始矩阵 job 汇总：缺项、重复、计划外、非成功都必须拒绝
  Assert-Fail "证据失败：矩阵 job 缺平台实例" `
    (Invoke-GateEvidence -PlanJson $evPlan -Documents $goodDocs -JobsJson (New-JobsJson @((New-MatrixJobRow 'windows'), (New-MatrixJobRow 'linux')))) 'matrix_job_missing:macos-arm64'

  Assert-Fail "证据失败：矩阵 job 非成功" `
    (Invoke-GateEvidence -PlanJson $evPlan -Documents $goodDocs -JobsJson (New-JobsJson @((New-MatrixJobRow 'windows' -Conclusion 'failure'), (New-MatrixJobRow 'linux'), (New-MatrixJobRow 'macos-arm64')))) 'matrix_job_not_success:windows=failure'

  Assert-Fail "证据失败：出现计划外平台实例" `
    (Invoke-GateEvidence -PlanJson $evPlan -Documents $goodDocs -JobsJson (New-JobsJson @((New-MatrixJobRow 'windows'), (New-MatrixJobRow 'linux'), (New-MatrixJobRow 'macos-arm64'), (New-MatrixJobRow 'macos-intel')))) 'matrix_job_unplanned:macos-intel'

  # 关键防伪：jobs 列表会把沿用项也标成当前 attempt，只能靠开始时间分辨。
  Assert-Fail "证据失败：矩阵 job 开始时间早于本次 attempt" `
    (Invoke-GateEvidence -PlanJson $evPlan -Documents $goodDocs -JobsJson (New-JobsJson @((New-MatrixJobRow 'windows' -StartedAt '2026-11-26T19:40:02Z'), (New-MatrixJobRow 'linux'), (New-MatrixJobRow 'macos-arm64')))) 'matrix_job_from_previous_attempt:windows'

  # 无需要平台证据时（rust 未触发）不得因空证据目录失败
  Assert-Pass "证据通过：rust 未触发时不要求平台证据" `
    (Invoke-GateEvidence -PlanJson ((New-PlanJson -Rust $false -RustScope "none" -RustRequired $false -ChangedFiles @("docs/ops/PROGRESS.md")).Trim()) -Documents @() -JobsJson (New-JobsJson @()))

  # 突变敏感度：移除阶段完整性校验后空 stages 必须重新通过
  $verifierTextForStages = Get-Content -LiteralPath $verifierPath -Raw -Encoding utf8
  $stageMarker = '    Test-PlatformStages $doc $platformId $errors'
  Assert-Check ($verifierTextForStages.Contains($stageMarker)) "突变测试锚点缺失：Evidence 模式应调用阶段校验"
  if ($verifierTextForStages.Contains($stageMarker)) {
    $stageMutantPath = Join-Path $tempRoot "verify-ci-gate-stage-mutant.ps1"
    Set-Content -LiteralPath $stageMutantPath -Value $verifierTextForStages.Replace($stageMarker, '    # 突变：临时移除阶段完整性校验') -Encoding utf8 -NoNewline
    Assert-Pass "突变敏感度：移除阶段校验后空 stages 证据重新通过" `
      (Invoke-GateEvidence -PlanJson $evPlan -Documents $emptyStagesDocs -JobsJson $goodJobs -VerifierOverride $stageMutantPath)
  }

  # ============ 三·三、采集器真实往返：Stage→Stage→Stage→Finalize ============
  # 每一步都起独立进程，与工作流实际调用形态一致：上一轮写入必须能被下一轮读回。
  $collectorPath = Join-Path $repoRoot "scripts/collect-ci-evidence.ps1"
  Assert-Check (Test-Path -LiteralPath $collectorPath -PathType Leaf) "采集器脚本必须存在"

  $collectDir = Join-Path $tempRoot ("collector-" + [Guid]::NewGuid().ToString("N"))
  New-Item -ItemType Directory -Path $collectDir -Force | Out-Null
  $collectStages = Join-Path $collectDir "stages.json"
  $fetchLog = Join-Path $collectDir "fetch.log"
  $clippyLog = Join-Path $collectDir "clippy.log"
  $testLog = Join-Path $collectDir "test.log"
  Set-Content -LiteralPath $fetchLog -Value "Updating crates.io index" -NoNewline -Encoding utf8
  Set-Content -LiteralPath $clippyLog -Value "Finished dev profile target(s)" -NoNewline -Encoding utf8
  # 顶层多套件形状：每个 Cargo 套件边界内恰好一条 test result，且 running N 与
  # passed+failed+ignored 一致（libtest 的 N 是筛选后数量，filtered 不参与总数）。
  $testLogBody = @(
    '   Running unittests src\lib.rs (target\debug\deps\soxu-fixture.exe)'
    ''
    'running 161 tests'
    'test result: ok. 156 passed; 0 failed; 5 ignored; 0 measured; 2 filtered out; finished in 1.20s'
    '   Running tests\extra.rs (target\debug\deps\extra-fixture.exe)'
    ''
    'running 12 tests'
    'test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.30s'
  ) -join "`n"
  Set-Content -LiteralPath $testLog -Value $testLogBody -NoNewline -Encoding utf8

  $stageSpecs = @(
    @{ Name = "cargo-fetch"; Log = $fetchLog; Command = "cargo fetch --locked" }
    @{ Name = "cargo-clippy"; Log = $clippyLog; Command = "cargo clippy --workspace --all-targets --locked -- -D warnings" }
    @{ Name = "cargo-test"; Log = $testLog; Command = "cargo test --workspace --locked" }
  )
  $stageIndex = 0
  foreach ($spec in $stageSpecs) {
    $stageIndex++
    $stageResult = Invoke-Script $collectorPath @(
      "-Mode", "Stage", "-StagesPath", $collectStages, "-Name", $spec.Name,
      "-Command", $spec.Command, "-ExitCode", "0", "-DurationMs", "1500", "-LogPath", $spec.Log
    )
    Assert-Check ($stageResult.Code -eq 0) "采集器第 $stageIndex 阶段 $($spec.Name) 必须成功（实际 exit $($stageResult.Code)：$($stageResult.Output.Trim())）"
  }

  $recordedStages = @()
  if (Test-Path -LiteralPath $collectStages) {
    $recordedStages = @((Get-Content -LiteralPath $collectStages -Raw -Encoding utf8) | ConvertFrom-Json -Depth 12)
  }
  Assert-Check ($recordedStages.Count -eq 3) "采集器必须累计三个阶段而不是互相覆盖（实际 $($recordedStages.Count)）"
  $recordedNames = @($recordedStages | ForEach-Object { $_.name }) -join ','
  Assert-Check ($recordedNames -eq "cargo-fetch,cargo-clippy,cargo-test") "阶段名称与顺序必须保持调用顺序（实际 $recordedNames）"

  $testStage = @($recordedStages | Where-Object { $_.name -eq "cargo-test" })[0]
  Assert-Check ($null -ne $testStage -and $testStage.countsKnown -eq $true) "cargo-test 必须解析出真实计数而不是标成未知"
  if ($null -ne $testStage) {
    Assert-Check (
      $testStage.counts.passed -eq 168 -and $testStage.counts.failed -eq 0 -and
      $testStage.counts.ignored -eq 5 -and $testStage.counts.filtered -eq 2
    ) "cargo-test 计数必须汇总全部 test result 行（实际 $($testStage.counts | ConvertTo-Json -Compress)）"
  }
  $fetchStage = @($recordedStages | Where-Object { $_.name -eq "cargo-fetch" })[0]
  Assert-Check ($null -ne $fetchStage -and $fetchStage.countsKnown -eq $false -and $null -eq $fetchStage.counts) "无测试计数的阶段必须明确标为未知而不是 0"

  $collectorSha = $script:testSha
  $finalizeArgs = @(
    "-Mode", "Finalize", "-StagesPath", $collectStages, "-OutputPath", (Join-Path $collectDir "evidence.json"),
    "-Kind", "platform", "-Platform", "windows",
    "-RunnerLabel", "windows-2025", "-Os", "Windows", "-Arch", "X64",
    "-RunId", $script:testRunId, "-RunAttempt", $script:testAttempt,
    "-Repository", "TanCan1998/Soxu", "-EventName", "pull_request",
    "-BaseSha", (("b" * 40) -join ""), "-HeadSha", (("c" * 40) -join ""),
    "-CheckoutSha", $collectorSha,
    "-WorkflowRef", "stub/mirror/.github/workflows/ci.yml@refs/heads/main",
    "-PlanJson", $evPlan, "-ResultStatus", "passed",
    "-StartedAt", $script:testStageStarted, "-CompletedAt", $script:testStageCompleted
  )
  $finalizeResult = Invoke-Script $collectorPath $finalizeArgs
  Assert-Check ($finalizeResult.Code -eq 0) "采集器 Finalize 必须成功（实际 exit $($finalizeResult.Code)：$($finalizeResult.Output.Trim())）"

  $realEvidencePath = Join-Path $collectDir "evidence.json"
  $realEvidence = $null
  if (Test-Path -LiteralPath $realEvidencePath) {
    $realEvidence = (Get-Content -LiteralPath $realEvidencePath -Raw -Encoding utf8) | ConvertFrom-Json -Depth 20
  }
  Assert-Check ($null -ne $realEvidence) "采集器必须写出可解析的证据 JSON"
  if ($null -ne $realEvidence) {
    Assert-Check (@($realEvidence.stages).Count -eq 3) "最终证据必须携带三个阶段（实际 $(@($realEvidence.stages).Count)）"
    Assert-Check ($realEvidence.runner.os -eq "windows" -and $realEvidence.runner.arch -eq "x64") "Runner 观测必须从原生形态（Windows/X64）归一为平台目录取值"
    Assert-Check ($realEvidence.plan.digest -eq $evDigest) "采集器写出的计划摘要必须与 Gate 重算一致"
    Assert-Check ($realEvidence.result.status -eq "passed") "三阶段全绿时结果必须为 passed"
  }

  # Gate 接收：采集器真实产物直接参加对账。
  Assert-Pass "证据通过：采集器真实产物被 Gate 接收" `
    (Invoke-GateEvidence -PlanJson $evPlan -Documents @($realEvidence, $goodDocs[1], $goodDocs[2]) -JobsJson $goodJobs)

  # Gate 拒绝：从真实产物里移除一个阶段。
  if ($null -ne $realEvidence) {
    $stageTrimmed = (Get-Content -LiteralPath $realEvidencePath -Raw -Encoding utf8) | ConvertFrom-Json -Depth 20
    $stageTrimmed.stages = @(@($stageTrimmed.stages) | Where-Object { $_.name -ne "cargo-test" })
    Assert-Fail "证据失败：采集器产物缺 test 阶段" `
      (Invoke-GateEvidence -PlanJson $evPlan -Documents @($stageTrimmed, $goodDocs[1], $goodDocs[2]) -JobsJson $goodJobs) 'evidence_stage_missing:windows/cargo-test'
  }

  # 采集器自身必须把编译中断识别为 build_failed，且 Gate 拒绝该产物。
  $failDir = Join-Path $tempRoot ("collector-fail-" + [Guid]::NewGuid().ToString("N"))
  New-Item -ItemType Directory -Path $failDir -Force | Out-Null
  $failStages = Join-Path $failDir "stages.json"
  $failLog = Join-Path $failDir "clippy.log"
  # 日志夹具里不使用反引号包裹 crate 名：反引号会转义掉收尾引号导致字符串跨界。
  $failLogBody = @(
    "error[E0432]: unresolved import"
    "error: could not compile soxu-core"
  ) -join "`n"
  Set-Content -LiteralPath $failLog -Value $failLogBody -NoNewline -Encoding utf8
  foreach ($spec in @(
      @{ Name = "cargo-fetch"; Log = $fetchLog; Code = "0" }
      @{ Name = "cargo-clippy"; Log = $failLog; Code = "101" }
    )) {
    $recordResult = Invoke-Script $collectorPath @(
      "-Mode", "Stage", "-StagesPath", $failStages, "-Name", $spec.Name,
      "-Command", "cargo $($spec.Name -replace '^cargo-', '')", "-ExitCode", $spec.Code,
      "-DurationMs", "900", "-LogPath", $spec.Log
    )
    Assert-Check ($recordResult.Code -eq 0) "失败阶段的记录动作本身必须成功（$($spec.Name) 实际 exit $($recordResult.Code)）"
  }
  $failedStagesJson = @((Get-Content -LiteralPath $failStages -Raw -Encoding utf8) | ConvertFrom-Json -Depth 12)
  $clippyFailStage = @($failedStagesJson | Where-Object { $_.name -eq "cargo-clippy" })[0]
  Assert-Check ($null -ne $clippyFailStage -and $clippyFailStage.status -eq "build_failed") "编译中断必须识别为 build_failed 而不是普通测试失败"

  $failedEvidencePath = Join-Path $failDir "evidence.json"
  $failedFinalize = Invoke-Script $collectorPath @(
    "-Mode", "Finalize", "-StagesPath", $failStages, "-OutputPath", $failedEvidencePath,
    "-Kind", "platform", "-Platform", "windows",
    "-RunnerLabel", "windows-2025", "-Os", "Windows", "-Arch", "X64",
    "-RunId", $script:testRunId, "-RunAttempt", $script:testAttempt,
    "-Repository", "TanCan1998/Soxu", "-EventName", "pull_request",
    "-BaseSha", (("b" * 40) -join ""), "-HeadSha", (("c" * 40) -join ""),
    "-CheckoutSha", $collectorSha,
    "-WorkflowRef", "stub/mirror/.github/workflows/ci.yml@refs/heads/main",
    "-PlanJson", $evPlan,
    "-StartedAt", $script:testStageStarted, "-CompletedAt", $script:testStageCompleted
  )
  Assert-Check ($failedFinalize.Code -eq 0) "失败证据的 Finalize 也必须能写出报告（实际 exit $($failedFinalize.Code)）"
  if (Test-Path -LiteralPath $failedEvidencePath) {
    $failedEvidence = (Get-Content -LiteralPath $failedEvidencePath -Raw -Encoding utf8) | ConvertFrom-Json -Depth 20
    Assert-Check ($failedEvidence.result.status -eq "failed") "存在失败阶段时结语必须为 failed"
    Assert-Fail "证据失败：采集器记录的编译中断被 Gate 拒绝" `
      (Invoke-GateEvidence -PlanJson $evPlan -Documents @($failedEvidence, $goodDocs[1], $goodDocs[2]) -JobsJson (New-JobsJson @((New-MatrixJobRow "windows" -Conclusion "failure"), (New-MatrixJobRow "linux"), (New-MatrixJobRow "macos-arm64")))) 'evidence_stage_not_passed:windows/cargo-clippy=build_failed'
  }

  # ============ 三·三·二、日志形状：嵌套派生进程不充顶层计数 ============
  # 裁决 A：可可靠归属（每个套件边界内恰好一条 test result）才计数；
  # 嵌套/交错/边界外一律 countsKnown=false + counts=null + 稳定脱敏原因，不猜数字。
  function Invoke-LogShapeStage([string]$Tag, [string]$LogBody, [int]$ExitCode = 0, [string]$CollectorOverride = "") {
    $shapeDir = Join-Path $tempRoot ("shape-" + $Tag + "-" + [Guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Path $shapeDir -Force | Out-Null
    $logPath = Join-Path $shapeDir 'run.log'
    Set-Content -LiteralPath $logPath -Value $LogBody -Encoding utf8 -NoNewline
    $stagesPath = Join-Path $shapeDir 'stages.json'
    $target = if ([string]::IsNullOrWhiteSpace($CollectorOverride)) { $collectorPath } else { $CollectorOverride }
    $result = Invoke-Script $target @(
      '-Mode', 'Stage', '-StagesPath', $stagesPath, '-Name', 'cargo-test',
      '-Command', 'cargo test --workspace --locked', '-ExitCode', "$ExitCode", '-LogPath', $logPath
    )
    $record = $null
    if (Test-Path -LiteralPath $stagesPath) {
      $record = @((Get-Content -LiteralPath $stagesPath -Raw -Encoding utf8) | ConvertFrom-Json -Depth 12)[0]
    }
    return [pscustomobject]@{ Code = $result.Code; Output = $result.Output; Record = $record; Dir = $shapeDir; LogPath = $logPath }
  }

  # 嵌套派生进程形状：复刻 worktree_lifecycle 夹具，顶层 138 + 9 条子进程汇总。
  $suiteSpecs = @(
    @{ Path = 'unittests src/lib.rs (target/debug/deps/git-lib)'; Count = 27 }
    @{ Path = 'tests/checkpoint.rs (target/debug/deps/checkpoint-1)'; Count = 8 }
    @{ Path = 'tests/diff.rs (target/debug/deps/diff-2)'; Count = 13 }
    @{ Path = 'tests/fixed_diff.rs (target/debug/deps/fixed-diff-3)'; Count = 7 }
    @{ Path = 'tests/probe.rs (target/debug/deps/probe-4)'; Count = 9 }
    @{ Path = 'tests/project.rs (target/debug/deps/project-5)'; Count = 5 }
    @{ Path = 'tests/repository_create.rs (target/debug/deps/repo-6)'; Count = 28 }
    @{ Path = 'tests/status.rs (target/debug/deps/status-7)'; Count = 7 }
    @{ Path = 'tests/worktree_checkout.rs (target/debug/deps/wc-8)'; Count = 6 }
    @{ Path = 'tests/worktree_lifecycle.rs (target/debug/deps/wl-9)'; Count = 17; Nested = 9 }
    @{ Path = 'tests/worktrees.rs (target/debug/deps/wt-10)'; Count = 11 }
  )
  $nestedLogLines = [System.Collections.Generic.List[string]]::new()
  foreach ($spec in $suiteSpecs) {
    $nestedLogLines.Add("   Running $($spec.Path)")
    $nestedLogLines.Add('')
    $nestedLogLines.Add("running $($spec.Count) tests")
    if ($spec.ContainsKey('Nested')) {
      for ($n = 0; $n -lt $spec.Nested; $n++) {
        $nestedLogLines.Add('test fixture_worker ... ok')
        $nestedLogLines.Add('running 1 test')
        $nestedLogLines.Add('test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 16 filtered out; finished in 0.02s')
      }
    }
    $nestedLogLines.Add("test result: ok. $($spec.Count) passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.00s")
  }
  $nestedLogLines.Add('   Doc-tests soxu_git')
  $nestedLogLines.Add('')
  $nestedLogLines.Add('running 0 tests')
  $nestedLogLines.Add('test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s')
  $nestedLogBody = ($nestedLogLines -join "`n")

  $shapeNested = Invoke-LogShapeStage 'nested' $nestedLogBody
  Assert-Check ($shapeNested.Code -eq 0) "嵌套形状 Stage 必须能记录（实际 exit $($shapeNested.Code)）"
  Assert-Check ($null -ne $shapeNested.Record) "嵌套形状必须产出阶段记录"
  if ($null -ne $shapeNested.Record) {
    Assert-Check ($shapeNested.Record.countsKnown -eq $false) "嵌套派生进程汇总不得充顶层计数（实际 countsKnown=$($shapeNested.Record.countsKnown)）"
    Assert-Check ($null -eq $shapeNested.Record.counts) "计数未知时 counts 必须为 null 而不是数字"
    Assert-Check ($shapeNested.Record.countsReason -eq 'nested_test_result_lines') "未知原因必须稳定可审计（实际 $($shapeNested.Record.countsReason)）"
    Assert-Check ($shapeNested.Record.testResultLines -eq 21) "应识别全部 21 条 test result 行（实际 $($shapeNested.Record.testResultLines)）"
    Assert-Check ($shapeNested.Record.suiteCount -eq 12) "应识别 12 个 Cargo 套件边界（实际 $($shapeNested.Record.suiteCount)）"
    Assert-Check ($shapeNested.Record.nestedResultLines -eq 9) "应识别 9 条派生进程汇总（实际 $($shapeNested.Record.nestedResultLines)）"
  }

  # 回归敏感度：恢复旧“全文求和”实现必须给出 147，证明该用例确实在防守。
  $collectorText = Get-Content -LiteralPath $collectorPath -Raw -Encoding utf8
  $functionMatch = [regex]::Match($collectorText, '(?s)function Get-LogObservation\(\[string\]\$Path\) \{.*?\r?\n\}\r?\n')
  Assert-Check ($functionMatch.Success) "突变测试锚点缺失：未找到 Get-LogObservation 完整函数体"
  if ($functionMatch.Success) {
    $legacyFunction = @'
function Get-LogObservation([string]$Path) {
  $observation = [pscustomobject]@{ CountsKnown = $false; Counts = $null; CountsReason = "legacy"; TestResultLines = 0; SuiteCount = 0; NestedResultLines = 0; BuildFailed = $false; TestsFailed = $false }
  if ([string]::IsNullOrWhiteSpace($Path) -or -not (Test-Path -LiteralPath $Path -PathType Leaf)) { return $observation }
  $text = Get-Content -LiteralPath $Path -Raw -Encoding utf8
  if ($text -match 'error\[E[0-9]+\]' -or $text -match 'could not compile') { $observation.BuildFailed = $true }
  $p = 0; $f = 0; $i = 0; $m = 0; $x = 0; $seen = $false
  foreach ($match in [regex]::Matches($text, 'test result:\s*(?<verdict>[A-Za-z]+)\.\s*(?<p>\d+)\s+passed;\s*(?<f>\d+)\s+failed;\s*(?<i>\d+)\s+ignored;\s*(?<m>\d+)\s+measured;\s*(?<x>\d+)\s+filtered out')) {
    $seen = $true
    $p += [int]$match.Groups['p'].Value; $f += [int]$match.Groups['f'].Value
    $i += [int]$match.Groups['i'].Value; $m += [int]$match.Groups['m'].Value; $x += [int]$match.Groups['x'].Value
    if ($match.Groups['verdict'].Value -eq 'FAILED') { $observation.TestsFailed = $true }
  }
  if ($seen) {
    $observation.CountsKnown = $true
    $observation.Counts = [ordered]@{ passed = $p; failed = $f; ignored = $i; measured = $m; filtered = $x }
    $observation.TestResultLines = ([regex]::Matches($text, 'test result:')).Count
  }
  if ($f -gt 0) { $observation.TestsFailed = $true }
  return $observation
}
'@
    $legacyCollector = Join-Path $tempRoot 'collect-ci-evidence-legacy.ps1'
    $legacyText = $collectorText.Substring(0, $functionMatch.Index) + $legacyFunction + $collectorText.Substring($functionMatch.Index + $functionMatch.Length)
    Set-Content -LiteralPath $legacyCollector -Value $legacyText -Encoding utf8 -NoNewline

    $shapeLegacy = Invoke-LogShapeStage 'nested-legacy' $nestedLogBody 0 $legacyCollector
    Assert-Check ($null -ne $shapeLegacy.Record) "旧实现突变体必须能产出阶段记录"
    if ($null -ne $shapeLegacy.Record) {
      Assert-Check ($shapeLegacy.Record.countsKnown -eq $true) "旧实现确实会把嵌套行当计数（否则该回归无意义）"
      Assert-Check ($shapeLegacy.Record.counts.passed -eq 147) "旧实现对同一日志给出 147（138+9），证明差异可稳定复现（实际 $($shapeLegacy.Record.counts.passed)）"
    }
  }

  # 合法顶层筛选：单套件只有一条 result 时 filtered>0 仍必须正确计数（N 含 ignored）。
  $shapeFiltered = Invoke-LogShapeStage 'single-filtered' @'
running 5 tests
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 3 filtered out; finished in 0.10s
'@
  if ($null -ne $shapeFiltered.Record) {
    Assert-Check ($shapeFiltered.Record.countsKnown -eq $true) "单套件单条 result 必须可计数"
    Assert-Check ($shapeFiltered.Record.counts.passed -eq 5 -and $shapeFiltered.Record.counts.filtered -eq 3) "合法顶层 filtered 必须原样计入（实际 $($shapeFiltered.Record.counts | ConvertTo-Json -Compress)）"
    Assert-Check ($shapeFiltered.Record.countsReason -eq 'single_suite') "无边界但仅一条 result 应标记 single_suite"
  }

  # 无足够边界的多行形状：宁可未知。
  $shapeAmbiguous = Invoke-LogShapeStage 'unattributed' @'
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.1s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.1s
'@
  if ($null -ne $shapeAmbiguous.Record) {
    Assert-Check ($shapeAmbiguous.Record.countsKnown -eq $false -and $null -eq $shapeAmbiguous.Record.counts) "无套件边界的多行不得猜总数"
    Assert-Check ($shapeAmbiguous.Record.countsReason -eq 'unattributed_test_result_lines') "无边界多行原因必须稳定（实际 $($shapeAmbiguous.Record.countsReason)）"
  }

  # 套件边界之前的 result 行无法归属。
  $shapeOutside = Invoke-LogShapeStage 'outside' @'
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.1s
   Running tests/a.rs (target/debug/deps/a-d)

running 2 tests
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.1s
'@
  if ($null -ne $shapeOutside.Record) {
    Assert-Check ($shapeOutside.Record.countsKnown -eq $false) "边界外 result 行不得参与累加"
    Assert-Check ($shapeOutside.Record.countsReason -eq 'result_outside_suite_boundary') "边界外原因必须稳定（实际 $($shapeOutside.Record.countsReason)）"
  }

  # 两个真实反例：套件已开始但没有汇总（后续套件），以及顶层未结束但已见子进程汇总。
  # 两者都属“不完整”，整阶段未知，不得取已完整那一段的数字。
  $shapeIncompleteSecond = Invoke-LogShapeStage 'incomplete-second' @'
   Running tests/first.rs (target/debug/deps/first-a)

running 2 tests
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s
   Running tests/second.rs (target/debug/deps/second-b)

running 3 tests
'@
  if ($null -ne $shapeIncompleteSecond.Record) {
    Assert-Check ($shapeIncompleteSecond.Record.countsKnown -eq $false) "存在未结束的后续套件时不得报部分计数"
    Assert-Check ($null -eq $shapeIncompleteSecond.Record.counts) "不完整日志的 counts 必须为 null"
    Assert-Check ($shapeIncompleteSecond.Record.countsReason -eq 'incomplete_suite_summary') "不完整原因必须稳定（实际 $($shapeIncompleteSecond.Record.countsReason)）"
  }

  $shapeVisibleNesting = Invoke-LogShapeStage 'visible-nesting' @'
   Running tests/top.rs (target/debug/deps/top-c)

running 3 tests
running 1 test
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 16 filtered out; finished in 0.02s
'@
  if ($null -ne $shapeVisibleNesting.Record) {
    Assert-Check ($shapeVisibleNesting.Record.countsKnown -eq $false) "可见子进程汇总且顶层未结束时不得取子进程数字"
    Assert-Check ($null -eq $shapeVisibleNesting.Record.counts) "该形状的 counts 必须为 null"
    Assert-Check ($shapeVisibleNesting.Record.countsReason -eq 'incomplete_suite_summary') "可见嵌套不完整原因必须稳定（实际 $($shapeVisibleNesting.Record.countsReason)）"
  }

  # 不完整 + FAILED：计数未知但失败分型不能丢。
  $shapeIncompleteFailed = Invoke-LogShapeStage 'incomplete-failed' @'
   Running tests/top.rs (target/debug/deps/top-c)

running 3 tests
running 1 test
test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
'@ 101
  if ($null -ne $shapeIncompleteFailed.Record) {
    Assert-Check ($shapeIncompleteFailed.Record.countsKnown -eq $false -and $null -eq $shapeIncompleteFailed.Record.counts) "不完整时计数必须未知"
    Assert-Check ($shapeIncompleteFailed.Record.status -eq 'test_failed') "未知计数不得掩盖 FAILED 分型（实际 $($shapeIncompleteFailed.Record.status)）"
  }

  # 补审边界：第二套件只打印了 Cargo Running 边界、连 running N tests 都未就打印就中断。
  # 有边界但无汇总一律不完整，不能因为另一段完整就报已知计数。
  $shapeBoundaryOnly = Invoke-LogShapeStage 'boundary-only' @'
   Running tests/first.rs (target/debug/deps/first-a)

running 2 tests
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s
   Running tests/second.rs (target/debug/deps/second-b)
'@
  if ($null -ne $shapeBoundaryOnly.Record) {
    Assert-Check ($shapeBoundaryOnly.Record.countsKnown -eq $false) "仅有 Running 边界的空段也属不完整"
    Assert-Check ($null -eq $shapeBoundaryOnly.Record.counts) "有边界但无汇总时 counts 必须为 null"
    Assert-Check ($shapeBoundaryOnly.Record.countsReason -eq 'incomplete_suite_summary') "缺汇总原因必须稳定（实际 $($shapeBoundaryOnly.Record.countsReason)）"
  }

  # 开始与汇总行顺序错乱（先汇总后 running）同样不可归属。
  $shapeOutOfOrder = Invoke-LogShapeStage 'result-before-start' @'
   Running tests/a.rs (target/debug/deps/a)

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.1s
running 2 tests
'@
  if ($null -ne $shapeOutOfOrder.Record) {
    Assert-Check ($shapeOutOfOrder.Record.countsKnown -eq $false) "汇总行早于开始行时不得计数"
    Assert-Check ($shapeOutOfOrder.Record.countsReason -eq 'incomplete_suite_summary') "顺序错乱原因必须稳定（实际 $($shapeOutOfOrder.Record.countsReason)）"
  }

  # 未知计数不得掩盖真实失败：嵌套 + 退出码 101 + FAILED 必须仍判 test_failed。
  $shapeNestedFailed = Invoke-LogShapeStage 'nested-failed' @'
   Running tests\wl.rs (target\debug\deps\wl-c.exe)

running 3 tests
running 1 test
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 16 filtered out; finished in 0.02s
test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.4s
'@ 101
  if ($null -ne $shapeNestedFailed.Record) {
    Assert-Check ($shapeNestedFailed.Record.status -eq 'test_failed') "计数未知不得掩盖测试失败（实际 $($shapeNestedFailed.Record.status)）"
    Assert-Check ($shapeNestedFailed.Record.countsKnown -eq $false -and $null -eq $shapeNestedFailed.Record.counts) "失败时计数仍应保持未知而非伪造"
  }

  # 编译中断：计数未知 + build_failed，而不是测试失败。
  $shapeBuild = Invoke-LogShapeStage 'build' @'
   Compiling soxu-git v0.1.0

error[E0432]: unresolved import
error: could not compile soxu-git (lib) due to 2 previous errors
'@ 101
  if ($null -ne $shapeBuild.Record) {
    Assert-Check ($shapeBuild.Record.status -eq 'build_failed') "编译中断必须识别为 build_failed（实际 $($shapeBuild.Record.status)）"
    Assert-Check ($shapeBuild.Record.countsKnown -eq $false -and $null -eq $shapeBuild.Record.counts) "编译中断不得产出计数"
  }

  # 真实 Stage → Finalize → Evidence 往返：嵌套日志产出的未知计数阶段必须被契约接受，
  # 且不得被当成数字使用（未知是合法显式表态，但仍无法冒充顶层汇总）。
  if ($null -ne $shapeNested.Record) {
    $nestedRunDir = $shapeNested.Dir
    $nestedFetchLog = Join-Path $nestedRunDir 'fetch.log'
    $nestedClippyLog = Join-Path $nestedRunDir 'clippy.log'
    Set-Content -LiteralPath $nestedFetchLog -Value 'Updating crates.io index' -Encoding utf8 -NoNewline
    Set-Content -LiteralPath $nestedClippyLog -Value 'Finished dev profile' -Encoding utf8 -NoNewline
    $nestedStagesPath = Join-Path $nestedRunDir 'stages-rt.json'
    foreach ($stageSpec in @(
        @{ Name = 'cargo-fetch'; Log = $nestedFetchLog },
        @{ Name = 'cargo-clippy'; Log = $nestedClippyLog }
      )) {
      $null = Invoke-Script $collectorPath @('-Mode', 'Stage', '-StagesPath', $nestedStagesPath, '-Name', $stageSpec.Name, '-ExitCode', '0', '-LogPath', $stageSpec.Log)
    }
    # 嵌套测试阶段（复用同一日志，仍不可归属）
    $null = Invoke-Script $collectorPath @('-Mode', 'Stage', '-StagesPath', $nestedStagesPath, '-Name', 'cargo-test', '-ExitCode', '0', '-LogPath', $shapeNested.LogPath)

    $nestedEvidencePath = Join-Path $nestedRunDir 'evidence.json'
    $nestedFinalize = Invoke-Script $collectorPath @(
      '-Mode', 'Finalize', '-StagesPath', $nestedStagesPath, '-OutputPath', $nestedEvidencePath,
      '-Kind', 'platform', '-Platform', 'windows',
      '-RunnerLabel', 'windows-2025', '-Os', 'Windows', '-Arch', 'X64',
      '-RunId', $script:testRunId, '-RunAttempt', $script:testAttempt,
      '-Repository', 'TanCan1998/Soxu', '-EventName', 'pull_request',
      '-BaseSha', (("b" * 40) -join ""), '-HeadSha', (("c" * 40) -join ""),
      '-CheckoutSha', $script:testSha,
      '-WorkflowRef', 'stub/mirror/.github/workflows/ci.yml@refs/heads/main',
      '-PlanJson', $evPlan, '-ResultStatus', 'passed',
      '-StartedAt', $script:testStageStarted, '-CompletedAt', $script:testStageCompleted
    )
    Assert-Check ($nestedFinalize.Code -eq 0) "嵌套日志的 Finalize 必须成功（实际 exit $($nestedFinalize.Code)：$($nestedFinalize.Output.Trim())）"
    if (Test-Path -LiteralPath $nestedEvidencePath) {
      $nestedDoc = (Get-Content -LiteralPath $nestedEvidencePath -Raw -Encoding utf8) | ConvertFrom-Json -Depth 20
      $nestedTestStage = @($nestedDoc.stages | Where-Object { $_.name -eq 'cargo-test' })[0]
      Assert-Check ($null -ne $nestedTestStage -and $nestedTestStage.countsKnown -eq $false -and $null -eq $nestedTestStage.counts) "往返后的证据必须保留“计数未知”事实"
      Assert-Check ($null -ne $nestedTestStage -and [string]$nestedTestStage.countsReason -eq 'nested_test_result_lines') "证据必须保留脱敏原因"
      Assert-Pass "证据通过：嵌套日志产生的未知计数阶段被 Evidence 接受" `
        (Invoke-GateEvidence -PlanJson $evPlan -Documents @($nestedDoc, $goodDocs[1], $goodDocs[2]) -JobsJson $goodJobs)
    }
  }

  # 形式合法但数字不一致：running 3 tests 却只报 1 passed，必须未知（不采信）。
  $shapeCountMismatch = Invoke-LogShapeStage 'count-mismatch' @'
   Running tests/top.rs (target/debug/deps/top-c)

running 3 tests
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 16 filtered out; finished in 0.02s
'@
  if ($null -ne $shapeCountMismatch.Record) {
    Assert-Check ($shapeCountMismatch.Record.countsKnown -eq $false) "开始行数字与汇总不一致时不得计数"
    Assert-Check ($null -eq $shapeCountMismatch.Record.counts) "数字不一致时 counts 必须为 null"
    Assert-Check ($shapeCountMismatch.Record.countsReason -eq 'result_count_mismatch') "数字不一致原因必须稳定（实际 $($shapeCountMismatch.Record.countsReason)）"
  }

  # 无边界但只有一个开始与一个汇总时，同样校验开始行数字。
  $shapeCountMismatchNoBoundary = Invoke-LogShapeStage 'count-mismatch-noboundary' @'
running 3 tests
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 16 filtered out; finished in 0.02s
'@
  if ($null -ne $shapeCountMismatchNoBoundary.Record) {
    Assert-Check ($shapeCountMismatchNoBoundary.Record.countsKnown -eq $false) "无边界时数字不一致也不得计数"
    Assert-Check ($shapeCountMismatchNoBoundary.Record.countsReason -eq 'result_count_mismatch') "无边界数字不一致原因必须稳定（实际 $($shapeCountMismatchNoBoundary.Record.countsReason)）"
  }

  # 普通文字中嵌入的 test result 不是正式汇总（汇总行必须行首独立）。
  $shapeEmbedded = Invoke-LogShapeStage 'embedded-summary' @'
   Running tests/top.rs (target/debug/deps/top-c)

running 1 test
thread printed: test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 16 filtered out; finished in 0.02s
'@
  if ($null -ne $shapeEmbedded.Record) {
    Assert-Check ($shapeEmbedded.Record.countsKnown -eq $false) "行内嵌入的 test result 不得当作汇总"
    Assert-Check ($shapeEmbedded.Record.testResultLines -eq 0) "行内嵌入不得计入 test result 行数（实际 $($shapeEmbedded.Record.testResultLines)）"
    Assert-Check ($shapeEmbedded.Record.countsReason -eq 'incomplete_suite_summary') "嵌入后等价于无汇总，原因必须稳定（实际 $($shapeEmbedded.Record.countsReason)）"
  }

  # 合法正例：running 2 tests + 1 passed + 1 ignored（16 filtered 不参与总数）。
  $shapeFilteredIgnored = Invoke-LogShapeStage 'filtered-ignored' @'
   Running tests/top.rs (target/debug/deps/top-c)

running 2 tests
test result: ok. 1 passed; 0 failed; 1 ignored; 0 measured; 16 filtered out; finished in 0.02s
'@
  if ($null -ne $shapeFilteredIgnored.Record) {
    Assert-Check ($shapeFilteredIgnored.Record.countsKnown -eq $true) "合法 filtered+ignored 形状必须可计数"
    Assert-Check (
      $shapeFilteredIgnored.Record.counts.passed -eq 1 -and
      $shapeFilteredIgnored.Record.counts.ignored -eq 1 -and
      $shapeFilteredIgnored.Record.counts.filtered -eq 16 -and
      $shapeFilteredIgnored.Record.counts.failed -eq 0
    ) "合法形状计数必须原样保留（实际 $($shapeFilteredIgnored.Record.counts | ConvertTo-Json -Compress)）"
  }

  # 完整失败 + N 一致：已知计数 + test_failed 分型。
  $shapeCompleteFailed = Invoke-LogShapeStage 'complete-failed' @'
   Running tests/a.rs (target/debug/deps/a)

running 2 tests
test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.1s
'@ 101
  if ($null -ne $shapeCompleteFailed.Record) {
    Assert-Check ($shapeCompleteFailed.Record.countsKnown -eq $true -and $shapeCompleteFailed.Record.counts.failed -eq 1) "完整失败形状必须可计数"
    Assert-Check ($shapeCompleteFailed.Record.status -eq 'test_failed') "完整失败仍须判 test_failed"
  }

  # 零测试套件（Doc-tests 空）必须可计数为 0。
  $shapeZero = Invoke-LogShapeStage 'zero-tests' @'
   Running unittests src/lib.rs (target/debug/deps/a)

running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
'@
  if ($null -ne $shapeZero.Record) {
    Assert-Check ($shapeZero.Record.countsKnown -eq $true -and $shapeZero.Record.counts.passed -eq 0) "零测试套件必须可计数为 0"
  }

  # ============ 三·四、矩阵作业的 Verify 逐实例来源 ============
  # needs.<job>.result 只是聚合字符串，不含实例身份；矩阵作业必须从原始 jobs 派生。
  $mvPlan = (New-PlanJson).Trim()
  $mvEvidence = $evidenceBaseline
  $mvJobsGood = New-JobsJson @(foreach ($p in @('windows', 'linux', 'macos-arm64')) { New-MatrixJobRow $p })

  Assert-Pass "矩阵 Verify 通过：逐实例来自原始 jobs 且全绿" `
    (Invoke-GateVerify -PlanJson $mvPlan -EvidenceJson $mvEvidence -JobsJson $mvJobsGood)

  # 核心防回归：只有聚合 success 字符串、没原始 jobs 时必须拒绝。
  Assert-Fail "矩阵 Verify 失败：聚合成功字符串不得代替逐实例证据" `
    (Invoke-GateVerify -PlanJson $mvPlan -EvidenceJson $evidenceAggregateSuccess) 'matrix_jobs_unavailable:rust-check'

  Assert-Fail "矩阵 Verify 失败：原始 jobs 缺少平台实例" `
    (Invoke-GateVerify -PlanJson $mvPlan -EvidenceJson $mvEvidence -JobsJson (New-JobsJson @((New-MatrixJobRow 'windows'), (New-MatrixJobRow 'linux')))) 'matrix_instance_missing:rust-check/macos-arm64'

  Assert-Fail "矩阵 Verify 失败：平台实例失败" `
    (Invoke-GateVerify -PlanJson $mvPlan -EvidenceJson $evidenceMultiPartial -JobsJson (New-JobsJson @((New-MatrixJobRow 'windows' -Conclusion 'failure'), (New-MatrixJobRow 'linux'), (New-MatrixJobRow 'macos-arm64')))) 'required_not_success:rust-check/windows=failure'

  Assert-Fail "矩阵 Verify 失败：needs 报成功但实例不全绿" `
    (Invoke-GateVerify -PlanJson $mvPlan -EvidenceJson $evidenceAggregateSuccess -JobsJson (New-JobsJson @((New-MatrixJobRow 'windows' -Conclusion 'failure'), (New-MatrixJobRow 'linux'), (New-MatrixJobRow 'macos-arm64')))) 'matrix_aggregate_conflict:rust-check=success'

  Assert-Fail "矩阵 Verify 失败：needs 报跳过但实例全绿" `
    (Invoke-GateVerify -PlanJson $mvPlan -EvidenceJson $evidenceAggregateSkipped -JobsJson $mvJobsGood) 'matrix_aggregate_conflict:rust-check=skipped'

  Assert-Fail "矩阵 Verify 失败：同名实例行重复" `
    (Invoke-GateVerify -PlanJson $mvPlan -EvidenceJson $mvEvidence -JobsJson (New-JobsJson @((New-MatrixJobRow 'windows'), (New-MatrixJobRow 'windows'), (New-MatrixJobRow 'linux'), (New-MatrixJobRow 'macos-arm64')))) 'matrix_instance_duplicate:rust-check/windows'

  Assert-Fail "矩阵 Verify 失败：出现计划外平台行" `
    (Invoke-GateVerify -PlanJson $mvPlan -EvidenceJson $mvEvidence -JobsJson (New-JobsJson @((New-MatrixJobRow 'windows'), (New-MatrixJobRow 'linux'), (New-MatrixJobRow 'macos-arm64'), (New-MatrixJobRow 'macos-intel')))) 'matrix_instance_unknown:rust-check'

  Assert-Fail "矩阵 Verify 失败：证据 instances 与原始 jobs 不符" `
    (Invoke-GateVerify -PlanJson $mvPlan -EvidenceJson $evidenceMultiPartial -JobsJson $mvJobsGood) 'matrix_evidence_conflict:rust-check/windows'

  # rust 未触发时不应要求原始 jobs。
  Assert-Pass "矩阵 Verify 通过：rust 未触发时不要求原始 jobs" `
    (Invoke-GateVerify -PlanJson ((New-PlanJson -Rust $false -RustScope "none" -RustRequired $false -ChangedFiles @("docs/ops/PROGRESS.md")).Trim()) -EvidenceJson $evidenceRustSkipped)

  # ============ 三·五、真实 workflow run 块按 Actions pwsh 包装往返 ============
  # 只靠字符串契约盖不住参数绑定：本轮远端真实失败就是内部脚本调用的数组 splatting。
  # 这里从真实 ci.yml 提取 run 块，按 Actions 的 pwsh 包装真实执行：
  # 先 $ErrorActionPreference='Stop'，再 dot-source，最后按 LASTEXITCODE 决定退出码。
  $rtTemp = Join-Path $tempRoot ("runblocks-" + [Guid]::NewGuid().ToString("N"))
  $rtEvidence = Join-Path $rtTemp 'ci-evidence'
  New-Item -ItemType Directory -Path $rtEvidence -Force | Out-Null
  foreach ($p in $evPlatforms) {
    $sub = Join-Path $rtEvidence ("ci-evidence-1-rust-" + $p)
    New-Item -ItemType Directory -Path $sub -Force | Out-Null
    Set-Content -LiteralPath (Join-Path $sub 'evidence.json') -Value ((New-PlatformEvidenceDoc -Platform $p -PlanDigest $evDigest) | ConvertTo-Json -Depth 12) -Encoding utf8 -NoNewline
  }
  Set-Content -LiteralPath (Join-Path $rtTemp 'ci-jobs.json') -Value (New-JobsJson @(foreach ($p in $evPlatforms) { New-MatrixJobRow $p })) -Encoding utf8 -NoNewline
  Set-Content -LiteralPath (Join-Path $rtTemp 'ci-attempt-start.txt') -Value $script:testAttemptStart -Encoding utf8 -NoNewline
  Set-Content -LiteralPath (Join-Path $rtTemp 'ci-scope.json') -Value (New-ScopeJson -Rust $true -RustScope "workspace") -Encoding utf8 -NoNewline
  Set-Content -LiteralPath (Join-Path $rtTemp 'github-output.txt') -Value "" -Encoding utf8 -NoNewline
  Set-Content -LiteralPath (Join-Path $rtTemp 'github-env.txt') -Value "" -Encoding utf8 -NoNewline

  # 按 step 的明确 name 挑选并断言精确集合：不用命令词黑名单，否则新增接线会被静默漏测。
  # 接线 step 分两类：可整体执行往返的，与含构建命令只能做结构断言的 Stage step。
  $expectedExecutableBlocks = @(
    'Build deterministic CI gate plan'
    'Verify CI gate plan against actual job evidence'
    'Verify per-platform evidence against plan and run identity'
    'Finalize sanitized governance evidence'
    'Finalize sanitized format evidence'
    'Finalize sanitized platform evidence'
    'Finalize sanitized mobile web evidence'
  )
  # Stage step 的真实名字对应唯一采集阶段；漏一个就是少一个平台的执行证据。
  $expectedStageBlocks = [ordered]@{
    'Check formatting'                          = 'cargo-fmt'
    'Fetch dependencies (shared build directory)' = 'cargo-fetch'
    'Run Clippy'                                = 'cargo-clippy'
    'Run scoped unit & integration tests'       = 'cargo-test'
  }
  $expectedAllBlocks = @($expectedExecutableBlocks) + @($expectedStageBlocks.Keys)

  $allBlocks = @(Get-RunBlocks $ciWorkflow)
  $blockGrouped = @{}
  foreach ($candidate in $allBlocks) {
    if (-not $blockGrouped.ContainsKey($candidate.name)) { $blockGrouped[$candidate.name] = [System.Collections.Generic.List[object]]::new() }
    $blockGrouped[$candidate.name].Add($candidate)
  }

  $missingBlocks = @($expectedAllBlocks | Where-Object { -not $blockGrouped.ContainsKey($_) })
  Assert-Check ($missingBlocks.Count -eq 0) "缺少预期的脚本接线 step：$($missingBlocks -join ', ')"

  # 反向覆盖：真实工作流里调脚本的 step 必须全部列入，否则新增接线会漏测。
  $scriptStepBlocks = @($allBlocks | Where-Object { $_.body -match 'scripts/(verify-ci-gate|collect-ci-evidence)\.ps1' })
  $unlistedBlocks = @($scriptStepBlocks | Where-Object { $expectedAllBlocks -notcontains $_.name })
  Assert-Check ($unlistedBlocks.Count -eq 0) "存在未纳入回归的脚本接线 step：$((@($unlistedBlocks | ForEach-Object { $_.name })) -join ', ')"
  Assert-Check ($scriptStepBlocks.Count -eq $expectedAllBlocks.Count) "调脚本的接线 step 总数必须精确为 $($expectedAllBlocks.Count)（实际 $($scriptStepBlocks.Count)）"

  # Stage step 不执行 cargo，但必须断言其采集调用形态与阶段名完全正确。
  foreach ($stageStepName in $expectedStageBlocks.Keys) {
    $stageBlock = $blockGrouped[$stageStepName]
    Assert-Check ($null -ne $stageBlock) "缺少 Stage step「$stageStepName」"
    if ($null -eq $stageBlock) { continue }
    $stageBody = $stageBlock.body
    Assert-Check ($stageBody -match "-Mode Stage") "Stage step「$stageStepName」必须显式传 -Mode Stage"
    Assert-Check ($stageBody -match "-StagesPath") "Stage step「$stageStepName」必须传 -StagesPath"
    $expectedStageName = [string]$expectedStageBlocks[$stageStepName]
    Assert-Check ($stageBody -match [regex]::Escape("-Name $expectedStageName")) "Stage step「$stageStepName」必须记录阶段名 $expectedStageName"
    Assert-Check ($stageBody -match "-ExitCode") "Stage step「$stageStepName」必须传递真实退出码"
  }

  $rtBlocks = @($expectedExecutableBlocks | ForEach-Object { $blockGrouped[$_] } | Where-Object { $null -ne $_ })
  Assert-Check ($rtBlocks.Count -eq $expectedExecutableBlocks.Count) "可执行往返必须精确覆盖 $($expectedExecutableBlocks.Count) 个接线 step（实际 $($rtBlocks.Count)）"
  foreach ($expectedBlock in $rtBlocks) {
    Assert-Check ($expectedBlock.body -notmatch '(?m)^\s*(&\s*)?(cargo|pnpm|gitleaks)\b') "往返 step「$($expectedBlock.name)」不得包含构建工具执行行"
  }

  $rtEnv = @{
    GATE_PLAN              = $evPlan
    DETECT_RESULT          = 'success'
    GOV_RESULT             = 'success'
    FMT_RESULT             = 'success'
    RUST_RESULT            = 'success'
    WEB_RESULT             = 'skipped'
    EVIDENCE_JOB_STATUS    = 'success'
    EVIDENCE_RUN_ID        = $script:testRunId
    EVIDENCE_RUN_ATTEMPT   = $script:testAttempt
    EVIDENCE_REPOSITORY    = 'TanCan1998/Soxu'
    EVIDENCE_EVENT         = 'pull_request'
    EVIDENCE_WORKFLOW_REF  = 'stub/mirror/.github/workflows/ci.yml@refs/heads/main'
    EVIDENCE_BASE_SHA      = (("b" * 40) -join "")
    EVIDENCE_HEAD_SHA      = (("c" * 40) -join "")
    EVIDENCE_CHECKOUT_SHA  = $script:testSha
    EVIDENCE_PLATFORM      = 'windows'
    EVIDENCE_RUNNER_LABEL  = 'windows-2025'
    EVIDENCE_RUNNER_OS     = 'Windows'
    EVIDENCE_RUNNER_ARCH   = 'X64'
    RUNNER_TEMP            = $rtTemp
    GITHUB_OUTPUT          = (Join-Path $rtTemp 'github-output.txt')
    GITHUB_ENV             = (Join-Path $rtTemp 'github-env.txt')
  }

  # 模拟 Actions 的 pwsh 包装：Stop + dot-source + LASTEXITCODE 退出。
  function Invoke-ActionPwshStep([string]$StepPath) {
    $escaped = $StepPath.Replace("'", "''")
    $wrapperPath = Join-Path $rtTemp ("wrapper-" + [Guid]::NewGuid().ToString("N") + ".ps1")
    $wrapperText = "`$ErrorActionPreference = 'Stop'`n. '$escaped'`nif (Test-Path -LiteralPath variable:\LASTEXITCODE) { exit `$LASTEXITCODE }`n"
    Set-Content -LiteralPath $wrapperPath -Value $wrapperText -Encoding utf8 -NoNewline
    Push-Location $repoRoot
    try { $out = (& pwsh -NoProfile -File $wrapperPath 2>&1 | Out-String); $code = $LASTEXITCODE }
    finally { Pop-Location }
    return [pscustomobject]@{ Code = $code; Output = $out }
  }

  $rtPrevious = @{}
  foreach ($key in $rtEnv.Keys) {
    $rtPrevious[$key] = [Environment]::GetEnvironmentVariable($key)
    [Environment]::SetEnvironmentVariable($key, [string]$rtEnv[$key])
  }
  try {
    foreach ($block in $rtBlocks) {
      # 消费型 step 读共享输入根；Finalize 必须写进自己的 RUNNER_TEMP，
      # 共用同一根会互相覆盖，也无法证明每个作业只产出自己的那份证据。
      $isConsumer = $block.name -like 'Build deterministic*' -or $block.name -like 'Verify *'
      if ($isConsumer) {
        $blockTemp = $rtTemp
      } else {
        $blockTemp = Join-Path $rtTemp ("run-" + [Guid]::NewGuid().ToString("N"))
        New-Item -ItemType Directory -Path (Join-Path $blockTemp 'ci-evidence') -Force | Out-Null
        Set-Content -LiteralPath (Join-Path $blockTemp 'ci-start.txt') -Value $script:testStageStarted -Encoding utf8 -NoNewline
      }

      $previousTemp = [Environment]::GetEnvironmentVariable('RUNNER_TEMP')
      [Environment]::SetEnvironmentVariable('RUNNER_TEMP', $blockTemp)
      try {
        $stepPath = Join-Path $rtTemp ("step-" + [Guid]::NewGuid().ToString("N") + ".ps1")
        Set-Content -LiteralPath $stepPath -Value $block.body -Encoding utf8 -NoNewline
        $rtResult = Invoke-ActionPwshStep $stepPath
      } finally {
        [Environment]::SetEnvironmentVariable('RUNNER_TEMP', $previousTemp)
      }
      Assert-Check ($rtResult.Code -eq 0) "真实 run 块「$($block.name)」在 Actions pwsh 包装下必须 exit 0（实际 exit $($rtResult.Code)：$($rtResult.Output.Trim())）"
      Assert-Check ($rtResult.Output -notmatch 'Cannot validate argument on parameter') "真实 run 块「$($block.name)」不得出现参数绑定错误（实际：$($rtResult.Output.Trim())）"

      if (-not $isConsumer) {
        $produced = Join-Path $blockTemp 'ci-evidence/evidence.json'
        Assert-Check (Test-Path -LiteralPath $produced -PathType Leaf) "「$($block.name)」必须产出自己的证据文件"
        if (Test-Path -LiteralPath $produced -PathType Leaf) {
          $producedDoc = (Get-Content -LiteralPath $produced -Raw -Encoding utf8) | ConvertFrom-Json -Depth 20
          Assert-Check ([int]$producedDoc.schemaVersion -eq 1) "「$($block.name)」产出的证据必须声明 schemaVersion=1"
          Assert-Check ([string]$producedDoc.run.id -eq $script:testRunId -and [string]$producedDoc.run.attempt -eq $script:testAttempt) "「$($block.name)」证据必须绑定本次 Run/Attempt"
          Assert-Check ([string]$producedDoc.identity.checkoutSha -eq $script:testSha) "「$($block.name)」证据必须绑定 checkoutSha"
          if ($block.name -eq 'Finalize sanitized platform evidence') {
            Assert-Check ([string]$producedDoc.kind -eq 'soxu-ci-evidence/platform/v1' -and [string]$producedDoc.platform -eq 'windows') "平台证据必须使用 platform 类型与真实平台 id"
          } else {
            Assert-Check ([string]$producedDoc.kind -eq 'soxu-ci-evidence/job/v1') "作业证据必须使用 job 类型（$($block.name)）"
          }
        }
      }
    }

    # 突变回归：哈希表 splatting 退回字符串数组 splatting。主判据是真实退出码非零，
    # 错误文字只用于辅助定位，不能拿来代替退出码判定。
    $verifyBlock = @($rtBlocks | Where-Object { $_.name -eq 'Verify CI gate plan against actual job evidence' })[0]
    Assert-Check ($null -ne $verifyBlock) "必须存在调用 verify-ci-gate.ps1 的 Verify step"
    if ($null -ne $verifyBlock) {
      Assert-Check ($verifyBlock.body -match '@[A-Za-z_]\w*(Params|Parameters)') "Verify step 必须用哈希表 splatting 调用内部脚本"
      $brokenBody = @'
$args = @(
  '-Mode', 'Evidence',
  '-PlanJson', $env:GATE_PLAN,
  '-EvidenceDir', (Join-Path $env:RUNNER_TEMP 'ci-evidence')
)
./scripts/verify-ci-gate.ps1 @args
'@
      $brokenPath = Join-Path $rtTemp 'step-broken-splat.ps1'
      Set-Content -LiteralPath $brokenPath -Value $brokenBody -Encoding utf8 -NoNewline
      $brokenResult = Invoke-ActionPwshStep $brokenPath
      Assert-Check ($brokenResult.Code -ne 0) "字符串数组 splatting 在 Stop 包装下必须真实非零退出（实际 exit $($brokenResult.Code)：$($brokenResult.Output.Trim())）"
      Assert-Check ($brokenResult.Output -match "Cannot validate argument on parameter 'Mode'") "失败应来自 Mode 参数绑定（辅助定位）：$($brokenResult.Output.Trim())"
    }
  } finally {
    foreach ($key in $rtPrevious.Keys) {
      [Environment]::SetEnvironmentVariable($key, $rtPrevious[$key])
    }
  }

  # ============ 四、突变敏感度：移除契约校验后绕过用例必须重新通过 ============
  $verifierText = Get-Content -LiteralPath $verifierPath -Raw -Encoding utf8
  $contractMarker = '  Test-PlanContract $plan $errors'
  $hasMarker = $verifierText.Contains($contractMarker)
  Assert-Check $hasMarker "突变测试锚点缺失：verify-ci-gate.ps1 应调用计划契约校验"
  if ($hasMarker) {
    $mutantPath = Join-Path $tempRoot "verify-ci-gate-mutant.ps1"
    $mutantText = $verifierText.Replace($contractMarker, '  # 突变：临时移除计划契约校验')
    Set-Content -LiteralPath $mutantPath -Value $mutantText -Encoding utf8 -NoNewline
    Assert-Pass "突变敏感度：移除契约校验后治理降级绕过重新通过" `
      (Invoke-GateVerify -ScriptPath $mutantPath -PlanJson $planGovernanceDowngraded -EvidenceJson $evidenceGovSkipped)
    Assert-Pass "突变敏感度：移除契约校验后 scope 矛盾计划重新通过" `
      (Invoke-GateVerify -ScriptPath $mutantPath -PlanJson $planScopeContradiction -EvidenceJson $evidenceRustSkipped)
  }

  # ============ 五、工作流静态契约 ============
  Assert-Pass "通过：真实 ci.yml 门禁契约" (Invoke-WorkflowContract $ciWorkflow)

  # 结构化 outputs 的数组序列化：0/1/多项都必须保持 JSON 数组（与 ci.yml 同一惯用法）
  $zeroArrayJson = ConvertTo-Json -InputObject @() -Compress
  $singleArrayJson = ConvertTo-Json -InputObject @('linux') -Compress
  $multiArrayJson = ConvertTo-Json -InputObject @('linux', 'windows') -Compress
  Assert-Check ($zeroArrayJson -eq '[]') "0 元素输出必须序列化为空数组而不是空字符串（实际：$zeroArrayJson）"
  Assert-Check ($singleArrayJson -eq '["linux"]') "1 元素输出必须序列化为数组而不是 JSON 字符串（实际：$singleArrayJson）"
  Assert-Check ($multiArrayJson -eq '["linux","windows"]') "多元素输出序列化为确定性数组（实际：$multiArrayJson）"

  $variantJsonOutput = New-WorkflowVariant ".github/workflows/ci.yml" "ci-json-output" {
    param($text)
    $text.Replace('"required_platforms=$(ConvertTo-Json -InputObject @($scope.requiredPlatforms) -Compress)"', '"required_platforms=$((@($scope.requiredPlatforms) | ConvertTo-Json -Compress))"')
  }
  Assert-Fail "失败：结构化输出退回管道序列化" (Invoke-WorkflowContract $variantJsonOutput) 'detect_output_json_not_array:required_platforms'

  $variantContinueOnError = New-WorkflowVariant ".github/workflows/ci.yml" "ci-continue-on-error" {
    param($text)
    $text.Replace("    runs-on: ubuntu-latest`n    timeout-minutes: 5", "    runs-on: ubuntu-latest`n    timeout-minutes: 5`n    continue-on-error: true")
  }
  Assert-Fail "失败：门禁作业使用 continue-on-error" (Invoke-WorkflowContract $variantContinueOnError) 'workflow_uses_continue_on_error'

  $variantGateName = New-WorkflowVariant ".github/workflows/ci.yml" "ci-gate-name" {
    param($text)
    $text.Replace("  ci-gate:`n    name: CI Gate", "  ci-gate:`n    name: CI Gate (scoped)")
  }
  Assert-Fail "失败：门禁作业名称被改写" (Invoke-WorkflowContract $variantGateName) 'gate_name_unstable'

  $variantNeeds = New-WorkflowVariant ".github/workflows/ci.yml" "ci-gate-needs" {
    param($text)
    $text.Replace("    needs: [detect-changes, governance-and-privacy, rust-format, rust-check, mobile-web-check]", "    needs: [detect-changes, governance-and-privacy, rust-format, rust-check]")
  }
  Assert-Fail "失败：门禁 needs 缺项" (Invoke-WorkflowContract $variantNeeds) 'gate_needs'

  $variantUngatedRust = New-WorkflowVariant ".github/workflows/ci.yml" "ci-rust-ungated" {
    param($text)
    $text.Replace("    if: needs.detect-changes.outputs.rust == 'true'`n", "")
  }
  Assert-Fail "失败：rust 作业失去计划门控" (Invoke-WorkflowContract $variantUngatedRust) 'rust_job_ungated'

  Assert-Pass "通过：真实 release.yml 发布契约" (Invoke-ReleaseContract $releaseWorkflow)

  $variantNoChecksum = New-WorkflowVariant ".github/workflows/release.yml" "release-no-checksum" {
    param($text)
    $text.Replace("sha256sum --strict -c SHA256SUMS", "sha256sum SHA256SUMS")
  }
  Assert-Fail "失败：发布流水线丢失校验和回算" (Invoke-ReleaseContract $variantNoChecksum) 'release_checksum_verify_missing'

  $variantNonStrict = New-WorkflowVariant ".github/workflows/release.yml" "release-non-strict" {
    param($text)
    $text.Replace("sha256sum --strict -c SHA256SUMS", "sha256sum -c SHA256SUMS")
  }
  Assert-Fail "失败：发布清单未使用严格校验模式" (Invoke-ReleaseContract $variantNonStrict) 'release_checksum_strict_missing'

  $variantCountOnly = New-WorkflowVariant ".github/workflows/release.yml" "release-count-only" {
    param($text)
    $text.Replace("expected_manifest=`$(printf '%s\n' `$EXPECTED_RELEASE_ASSETS SHA256SUMS | sort | tr '\n' ' ')", "n=`$(gh release view `"`$REF`" -R `"`$RELEASES_REPO`" --json assets -q '.assets | length')`n          expected_manifest=`$(printf '%s\n' `$EXPECTED_RELEASE_ASSETS SHA256SUMS | sort | tr '\n' ' ')")
  }
  Assert-Fail "失败：发布流水线退回数量放行" (Invoke-ReleaseContract $variantCountOnly) 'release_count_only_gate'

  $variantMissingAsset = New-WorkflowVariant ".github/workflows/release.yml" "release-missing-asset" {
    param($text)
    $text.Replace("EXPECTED_RELEASE_ASSETS: soxu-linux-amd64.tar.gz soxu-windows-amd64.zip soxu-darwin-arm64.tar.gz soxu-darwin-amd64.tar.gz", "EXPECTED_RELEASE_ASSETS: soxu-linux-amd64.tar.gz soxu-windows-amd64.zip soxu-darwin-arm64.tar.gz")
  }
  Assert-Fail "失败：发布资产清单缺平台" (Invoke-ReleaseContract $variantMissingAsset) 'release_asset_manifest_missing'

  $variantNoDupCheck = New-WorkflowVariant ".github/workflows/release.yml" "release-no-dup-check" {
    param($text)
    $text.Replace("uniq -d", "uniq")
  }
  Assert-Fail "失败：发布清单丢失重复成员检测" (Invoke-ReleaseContract $variantNoDupCheck) 'release_duplicate_member_check_missing'

  $variantNoPattern = New-WorkflowVariant ".github/workflows/release.yml" "release-no-pattern" {
    param($text)
    $text.Replace('download_patterns+=(--pattern "$asset")', 'download_patterns+=("$asset")')
  }
  Assert-Fail "失败：发布下载未固定 --pattern" (Invoke-ReleaseContract $variantNoPattern) 'release_download_pattern_missing'

  # 静态标记齐全但内联 Bash 语法非法：真实发布专项入口必须非零，证明专项不靠纯文本匹配
  $variantBashBroken = New-WorkflowVariant ".github/workflows/release.yml" "release-bash-broken" {
    param($text)
    $anchor = '          set -euo pipefail' + "`n" + '          expected_manifest='
    $broken = '          set -euo pipefail' + "`n" + '          if [ -n "$MISSING"' + "`n" + '          expected_manifest='
    $text.Replace($anchor, $broken)
  }
  $brokenText = Get-Content -LiteralPath $variantBashBroken -Raw -Encoding utf8
  Assert-Check ($brokenText.Contains("sha256sum --strict -c SHA256SUMS")) "语法非法变体必须保留静态契约标记，否则负例不成立"
  Assert-Check ($brokenText.Contains($releaseAssetManifest)) "语法非法变体必须保留四平台资产标记"
  Assert-Fail "失败：静态标记齐全但 Bash 语法非法" (Invoke-ReleaseContract $variantBashBroken) 'release_bash_syntax_invalid'

  # run 结构失败闭合：|、|-、|+ 必须同样被提取；未支持的 run 形式必须显式拒绝
  $mirrorStepHeader = "      - name: Mirror release assets & enforce monotonic Latest tag`n        run: |"
  $variantChompBroken = New-WorkflowVariant ".github/workflows/release.yml" "release-chomp-broken" {
    param($text)
    $out = $text.Replace($mirrorStepHeader, "      - name: Mirror release assets & enforce monotonic Latest tag`n        run: |-")
    $anchor = '          set -euo pipefail' + "`n" + '          expected_manifest='
    $broken = '          set -euo pipefail' + "`n" + '          if [ -n "$MISSING"' + "`n" + '          expected_manifest='
    $out.Replace($anchor, $broken)
  }
  Assert-Check (((Get-Content -LiteralPath $variantChompBroken -Raw -Encoding utf8)).Contains("run: |-")) "chomping 变体必须真实写成 |-"
  $realBlockCount = @(Get-RunBlocks $releaseWorkflow).Count
  $chompBlockCount = @(Get-RunBlocks $variantChompBroken).Count
  Assert-Check ($chompBlockCount -eq $realBlockCount) "|- 块标量必须与 | 同样被提取（真实 $realBlockCount 段，chomping 变体 $chompBlockCount 段）"
  Assert-Fail "失败：|- 块标量内的非法 Bash 仍被检出" (Invoke-ReleaseContract $variantChompBroken) 'release_bash_syntax_invalid'

  $variantFoldedRun = New-WorkflowVariant ".github/workflows/release.yml" "release-folded-run" {
    param($text)
    $text.Replace($mirrorStepHeader, "      - name: Mirror release assets & enforce monotonic Latest tag`n        run: >")
  }
  Assert-Fail "失败：折叠标量 run 形式必须显式拒绝" (Invoke-ReleaseContract $variantFoldedRun) 'release_run_form_unsupported'

  # ============ 六、发布契约动态验证（bash -n + 离线桩执行） ============
  if ($null -eq $script:bashPath) {
    if ($RequireBash) {
      Assert-Check $false "发布契约动态验证要求 bash 可用，但当前环境未找到可用的 bash。"
    } else {
      Write-Host "::warning::未找到可用的 bash，跳过发布契约动态验证（CI 中必须通过）。" -ForegroundColor Yellow
    }
  } else {
    Write-Host "发布契约动态验证使用 bash：$script:bashPath" -ForegroundColor DarkCyan
    $runBlocks = Get-RunBlocks $releaseWorkflow
    Assert-Check ($runBlocks.Count -ge 4) "release.yml 应包含至少 4 段内联 Bash 脚本，实际 $($runBlocks.Count)"

    $blockIndex = 0
    foreach ($block in $runBlocks) {
      $blockIndex++
      $blockPath = Join-Path $tempRoot ("release-run-" + $blockIndex + ".sh")
      Set-Content -LiteralPath $blockPath -Value $block.body -Encoding utf8 -NoNewline
      $syntax = Invoke-Bash @("-n", (Get-PosixPath $blockPath))
      Assert-Check ($syntax.Code -eq 0) "release.yml 第 $blockIndex 段内联 Bash（约第 $($block.startLine) 行）语法检查失败：$($syntax.Output.Trim())"
    }

    $mirrorBlocks = @($runBlocks | Where-Object { $_.name -eq "Mirror release assets & enforce monotonic Latest tag" })
    Assert-Check ($mirrorBlocks.Count -eq 1) "应当且仅当存在一个镜像发布步骤，实际 $($mirrorBlocks.Count)"
    $mirrorBody = if ($mirrorBlocks.Count -gt 0) { $mirrorBlocks[0].body } else { "" }
    Assert-Check ($mirrorBody.Contains("assert_release_assets dist")) "镜像脚本必须调用资产清单校验函数"
    Assert-Check ($mirrorBody.Contains("--pattern")) "镜像脚本必须用 --pattern 固定下载范围"

    $expectedPatterns = ((@($releaseAssetNames) + "SHA256SUMS") | Sort-Object) -join " "

    # 正例：资产齐备、散列可回算、下载范围精确到五个名字
    $goodFixture = New-DistFixture -Name "good"
    $goodResult = Invoke-ReleaseMirrorCase -Name "good" -FixtureDir $goodFixture -MirrorBody $mirrorBody
    Assert-Check ($goodResult.Code -eq 0) "正例：齐备且校验通过的资产应放行，实际 exit $($goodResult.Code)：$($goodResult.Output.Trim())"
    Assert-Check ($goodResult.Output.Contains("资产清单与 SHA-256 校验全部通过")) "正例：应输出校验通过结论：$($goodResult.Output.Trim())"
    Assert-Check ($goodResult.StubLog -match '(?m)^release (create|upload)') "正例：校验通过后应继续发布动作（桩日志：$($goodResult.StubLog.Trim())）"
    Assert-Check ($goodResult.DistFiles.Count -eq 5) "正例：镜像目录应恰含 4 平台资产与 SHA256SUMS，实际 $($goodResult.DistFiles -join ',')"
    Assert-Check ((Get-StubPatternList $goodResult.StubLog) -eq $expectedPatterns) "正例：下载必须固定为五个精确名字，实际 [$(Get-StubPatternList $goodResult.StubLog)]"

    # 远端存在额外资产：下载只看五个精确名字，额外内容不得被选入
    $upstreamExtraFixture = New-DistFixture -Name "upstream-extra" -ExtraFiles @("soxu-freebsd-amd64.tar.gz")
    $upstreamExtraResult = Invoke-ReleaseMirrorCase -Name "upstream-extra" -FixtureDir $upstreamExtraFixture -MirrorBody $mirrorBody
    Assert-Check ($upstreamExtraResult.Code -eq 0) "额外远端资产：不得影响镜像，实际 exit $($upstreamExtraResult.Code)：$($upstreamExtraResult.Output.Trim())"
    Assert-Check ($upstreamExtraResult.DistFiles.Count -eq 5) "额外远端资产：下载结果必须仍为五个允许资产，实际 $($upstreamExtraResult.DistFiles -join ',')"
    Assert-Check ($upstreamExtraResult.DistFiles -notcontains "soxu-freebsd-amd64.tar.gz") "额外远端资产：不得被选入下载"
    Assert-Check ((Get-StubPatternList $upstreamExtraResult.StubLog) -eq $expectedPatterns) "额外远端资产：下载范围仍须为五个精确名字"

    $missingFixture = New-DistFixture -Name "missing" -Assets @($releaseAssetNames[0..2]) -SumMembers @($releaseAssetNames[0..2])
    Assert-ReleaseRejection "负例：资产缺项" (Invoke-ReleaseMirrorCase -Name "missing" -FixtureDir $missingFixture -MirrorBody $mirrorBody) '资产集合不符'

    # 强制注入一个未被选中的额外文件，验证下载后的复核仍会拒绝泄漏
    $leakedFixture = New-DistFixture -Name "leaked-extra" -ExtraFiles @("soxu-freebsd-amd64.tar.gz")
    Assert-ReleaseRejection "负例：下载结果泄漏额外文件" `
      (Invoke-ReleaseMirrorCase -Name "leaked-extra" -FixtureDir $leakedFixture -MirrorBody $mirrorBody -StubExtraFiles @("soxu-freebsd-amd64.tar.gz")) '资产集合不符'

    $duplicateFixture = New-DistFixture -Name "duplicate" -DuplicateMember
    Assert-ReleaseRejection "负例：SHA256SUMS 成员重复" (Invoke-ReleaseMirrorCase -Name "duplicate" -FixtureDir $duplicateFixture -MirrorBody $mirrorBody) 'SHA256SUMS 存在重复成员'

    $nestedFixture = New-DistFixture -Name "nested" -Nested
    Assert-ReleaseRejection "负例：资产目录存在嵌套路径" `
      (Invoke-ReleaseMirrorCase -Name "nested" -FixtureDir $nestedFixture -MirrorBody $mirrorBody -StubNested) '存在嵌套路径'

    $wrongHashFixture = New-DistFixture -Name "wrong-hash" -WrongHash
    Assert-ReleaseRejection "负例：内容哈希与清单不符" (Invoke-ReleaseMirrorCase -Name "wrong-hash" -FixtureDir $wrongHashFixture -MirrorBody $mirrorBody) 'FAILED'

    # 清单行摘要格式非法：非严格 sha256sum -c 只告警且 exit 0，--strict 才失败
    $invalidDigestFixture = New-DistFixture -Name "invalid-digest" -InvalidDigest
    Assert-ReleaseRejection "负例：清单行摘要格式非法" `
      (Invoke-ReleaseMirrorCase -Name "invalid-digest" -FixtureDir $invalidDigestFixture -MirrorBody $mirrorBody) 'SHA256SUMS 严格校验失败'

    # 严格模式敏感度：把同一镜像脚本降为非严格后，同一非法清单必须重新通过
    $nonStrictBlocks = @(Get-RunBlocks $variantNonStrict | Where-Object { $_.name -eq "Mirror release assets & enforce monotonic Latest tag" })
    Assert-Check ($nonStrictBlocks.Count -eq 1) "非严格变体应仍能提取镜像步骤，实际 $($nonStrictBlocks.Count)"
    if ($nonStrictBlocks.Count -eq 1) {
      $nonStrictResult = Invoke-ReleaseMirrorCase -Name "invalid-digest-non-strict" -FixtureDir $invalidDigestFixture -MirrorBody $nonStrictBlocks[0].body
      Assert-Check ($nonStrictResult.Code -eq 0) "严格模式敏感度：去掉 --strict 后同一非法清单必须重新通过（证明防守来自 --strict），实际 exit $($nonStrictResult.Code)：$($nonStrictResult.Output.Trim())"
    }

    $noSumsFixture = New-DistFixture -Name "no-sums" -OmitSums
    Assert-ReleaseRejection "负例：缺少 SHA256SUMS" (Invoke-ReleaseMirrorCase -Name "no-sums" -FixtureDir $noSumsFixture -MirrorBody $mirrorBody) '缺少 SHA256SUMS'
  }
}
finally {
  if (Test-Path -LiteralPath $tempRoot) {
    Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
  }
}

if ($script:failures.Count -eq 0) {
  Write-Host "✓ CI Gate 回归矩阵全部通过（$($script:checks) 项断言）" -ForegroundColor Green
  exit 0
}

foreach ($failure in $script:failures) {
  Write-Host "::error::CI-GATE-TEST-FAIL $failure" -ForegroundColor Red
}
Write-Host "✗ CI Gate 回归矩阵失败：$($script:failures.Count) / $($script:checks) 项断言未通过" -ForegroundColor Red
exit 1
