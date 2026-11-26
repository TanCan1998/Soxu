param(
  [string]$Verifier = "",
  [switch]$RequireBash
)

$ErrorActionPreference = "Stop"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$OutputEncoding = [System.Text.Encoding]::UTF8

# CI Gate 与发布契约回归矩阵：
#   1. Plan 模式从检测器输出构建确定性计划
#   2. Verify 模式逐项对账计划与 needs 证据（多实例、缺失、冲突、非法输入）
#   3. Contract / ReleaseContract 静态契约校验（真实工作流 + 负向变体）
#   4. 发布契约动态验证：抽取 release.yml 内联 Bash 做 bash -n，并用离线 gh 桩
#      与临时 dist 夹具真正执行镜像校验路径，断言坏例在任何 create/edit/upload 之前失败
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

function Save-Fixture([string]$Name, [string]$Content) {
  $path = Join-Path $tempRoot ($Name + ".json")
  Set-Content -LiteralPath $path -Value $Content -Encoding utf8 -NoNewline
  return $path
}

function Invoke-GateVerify([string]$PlanJson, [string]$EvidenceJson) {
  $arguments = @("-Mode", "Verify")
  if ($null -ne $PlanJson) {
    $arguments += @("-PlanPath", (Save-Fixture ("plan-" + [Guid]::NewGuid().ToString("N")) $PlanJson))
  }
  if ($null -ne $EvidenceJson) {
    $arguments += @("-EvidencePath", (Save-Fixture ("evidence-" + [Guid]::NewGuid().ToString("N")) $EvidenceJson))
  }
  return Invoke-Verifier $arguments
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

function Get-WorkflowLines([string]$Text) {
  return @($Text -split "`n" | ForEach-Object { $_.TrimEnd("`r") })
}

function Get-InlineRunBlocks([string]$Text) {
  $lines = Get-WorkflowLines $Text
  $blocks = [System.Collections.Generic.List[object]]::new()
  for ($i = 0; $i -lt $lines.Count; $i++) {
    if ($lines[$i] -notmatch '^(\s*)run:\s*\|\s*$') { continue }
    $runIndent = $Matches[1].Length
    $body = [System.Collections.Generic.List[string]]::new()
    $bodyIndent = -1
    for ($j = $i + 1; $j -lt $lines.Count; $j++) {
      $line = $lines[$j]
      if ([string]::IsNullOrWhiteSpace($line)) { $body.Add(""); continue }
      $indent = $line.Length - $line.TrimStart().Length
      if ($indent -le $runIndent) { break }
      if ($bodyIndent -lt 0) { $bodyIndent = $indent }
      $body.Add($line.Substring([Math]::Min($bodyIndent, $line.Length)))
    }
    while ($body.Count -gt 0 -and [string]::IsNullOrWhiteSpace($body[$body.Count - 1])) {
      $body.RemoveAt($body.Count - 1)
    }
    $blocks.Add([pscustomobject]@{ StartLine = $i + 1; Body = ($body -join "`n") })
    $i = $j - 1
  }
  return $blocks
}

function Get-RunBlockByStepName([string]$Text, [string]$StepName) {
  $lines = Get-WorkflowLines $Text
  $nameIndex = -1
  for ($i = 0; $i -lt $lines.Count; $i++) {
    if ($lines[$i].Trim() -eq "- name: $StepName") { $nameIndex = $i; break }
  }
  if ($nameIndex -lt 0) { throw "未找到步骤：$StepName" }
  for ($i = $nameIndex + 1; $i -lt $lines.Count; $i++) {
    if ($lines[$i] -match '^(\s*)run:\s*\|\s*$') {
      $runIndent = $Matches[1].Length
      $body = [System.Collections.Generic.List[string]]::new()
      $bodyIndent = -1
      for ($j = $i + 1; $j -lt $lines.Count; $j++) {
        $line = $lines[$j]
        if ([string]::IsNullOrWhiteSpace($line)) { $body.Add(""); continue }
        $indent = $line.Length - $line.TrimStart().Length
        if ($indent -le $runIndent) { break }
        if ($bodyIndent -lt 0) { $bodyIndent = $indent }
        $body.Add($line.Substring([Math]::Min($bodyIndent, $line.Length)))
      }
      while ($body.Count -gt 0 -and [string]::IsNullOrWhiteSpace($body[$body.Count - 1])) {
        $body.RemoveAt($body.Count - 1)
      }
      return ($body -join "`n")
    }
    if ($lines[$i] -match '^    - ') { break }
  }
  throw "步骤 $StepName 未找到内联 run 脚本。"
}

function Find-BashExecutable {
  $candidates = [System.Collections.Generic.List[string]]::new()
  $gitCommand = Get-Command git -ErrorAction SilentlyContinue
  if ($gitCommand -and $gitCommand.Source) {
    # <Git>/cmd/git.exe 或 <Git>/bin/git.exe -> <Git>
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

function New-DistFixture {
  param(
    [string]$Name,
    [string[]]$Assets = $null,
    [string[]]$SumMembers = $null,
    [string[]]$ExtraFiles = @(),
    [switch]$Nested,
    [switch]$OmitSums,
    [switch]$DuplicateMember,
    [switch]$WrongHash
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
      $sumLines.Add("$hash  $member")
      if ($DuplicateMember -and $sumLines.Count -eq 1) { $sumLines.Add("$hash  $member") }
    }
    Set-Content -LiteralPath (Join-Path $dir "SHA256SUMS") -Value (($sumLines -join "`n") + "`n") -Encoding utf8 -NoNewline
  }
  return $dir
}

function New-BashHarness {
  param(
    [string]$MirrorBody,
    [string]$FixtureDir,
    [string]$StubLog
  )
  # 单引号 here-string：内部保留字面 $ 与反引号，避免 PowerShell 提前展开 Bash 变量
  $prelude = @'
# --- 离线 gh 桩：只记录调用并按固定应答返回，绝不触网，不使用真实 secrets ---
gh() {
  printf '%s\n' "$*" >> "$STUB_LOG"
  case "$1" in
    release)
      case "$2" in
        download)
          cp -R "$FIXTURE_DIR"/. dist/
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
  )
  return (($headerLines -join "`n") + "`n" + $prelude + "`n" + $MirrorBody + "`n")
}

function Invoke-ReleaseMirrorCase {
  param([string]$Name, [string]$FixtureDir, [string]$MirrorBody)
  $workDir = Join-Path $tempRoot ("mirror-" + $Name)
  New-Item -ItemType Directory -Path $workDir -Force | Out-Null
  $stubLog = Join-Path $workDir "stub.log"
  Set-Content -LiteralPath $stubLog -Value "" -Encoding utf8 -NoNewline
  $harnessPath = Join-Path $workDir "harness.sh"
  $harness = New-BashHarness -MirrorBody $MirrorBody -FixtureDir $FixtureDir -StubLog $stubLog
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

# 门禁作业与证据基线夹具
$planGovernanceRequired = @'
{
  "version": "ci-gate-plan/v1",
  "detector": "detect-changes",
  "gate": "ci-gate",
  "jobs": [
    { "id": "governance-and-privacy", "required": true },
    { "id": "rust-check", "required": true, "depends_on": "rust" },
    { "id": "mobile-web-check", "required": false, "depends_on": "mobile_web" }
  ],
  "selfTest": false,
  "releaseContract": false,
  "scope": { "full": false, "rust": true, "rustScope": "crates", "mobileWeb": false, "paseoOrchestration": false, "checkScopeSelfTest": false }
}
'@

$planRustSkipped = @'
{
  "version": "ci-gate-plan/v1",
  "detector": "detect-changes",
  "gate": "ci-gate",
  "jobs": [
    { "id": "governance-and-privacy", "required": true },
    { "id": "rust-check", "required": false, "depends_on": "rust" },
    { "id": "mobile-web-check", "required": false, "depends_on": "mobile_web" }
  ],
  "selfTest": false,
  "releaseContract": false
}
'@

$planMobileRequired = @'
{
  "version": "ci-gate-plan/v1",
  "detector": "detect-changes",
  "gate": "ci-gate",
  "jobs": [
    { "id": "governance-and-privacy", "required": true },
    { "id": "rust-check", "required": false, "depends_on": "rust" },
    { "id": "mobile-web-check", "required": true, "depends_on": "mobile_web" }
  ],
  "selfTest": false,
  "releaseContract": false
}
'@

$planMultiInstance = @'
{
  "version": "ci-gate-plan/v1",
  "detector": "detect-changes",
  "gate": "ci-gate",
  "jobs": [
    { "id": "governance-and-privacy", "required": true },
    { "id": "rust-check", "required": true, "depends_on": "rust", "instances": ["ubuntu-latest", "windows-latest"] },
    { "id": "mobile-web-check", "required": false, "depends_on": "mobile_web" }
  ],
  "selfTest": false,
  "releaseContract": false
}
'@

$evidenceBaseline = @'
{
  "detect-changes": "success",
  "governance-and-privacy": "success",
  "rust-check": "success",
  "mobile-web-check": "skipped"
}
'@

try {
  # ============ 一、门禁计划与证据对账 ============
  Assert-Pass "通过：必跑 success + 计划内 skipped" (Invoke-GateVerify $planGovernanceRequired $evidenceBaseline)

  $evidenceRustSkipped = @'
{
  "detect-changes": "success",
  "governance-and-privacy": "success",
  "rust-check": "skipped",
  "mobile-web-check": "skipped"
}
'@
  Assert-Pass "通过：rust 与 mobile 均按计划跳过" (Invoke-GateVerify $planRustSkipped $evidenceRustSkipped)

  $evidenceGovSkipped = @'
{
  "detect-changes": "success",
  "governance-and-privacy": "skipped",
  "rust-check": "success",
  "mobile-web-check": "skipped"
}
'@
  Assert-Fail "失败：必跑治理任务异常 skipped" (Invoke-GateVerify $planGovernanceRequired $evidenceGovSkipped) 'required_not_success:governance-and-privacy=skipped'

  $evidenceDetectFailed = @'
{
  "detect-changes": "failure",
  "governance-and-privacy": "skipped",
  "rust-check": "skipped",
  "mobile-web-check": "skipped"
}
'@
  Assert-Fail "失败：检测器失败" (Invoke-GateVerify $planGovernanceRequired $evidenceDetectFailed) 'detector_not_success:detect-changes=failure'

  $evidenceCancelled = @'
{
  "detect-changes": "success",
  "governance-and-privacy": "success",
  "rust-check": "cancelled",
  "mobile-web-check": "skipped"
}
'@
  Assert-Fail "失败：必跑任务取消" (Invoke-GateVerify $planGovernanceRequired $evidenceCancelled) 'required_not_success:rust-check=cancelled'

  $evidenceMobileFailed = @'
{
  "detect-changes": "success",
  "governance-and-privacy": "success",
  "rust-check": "skipped",
  "mobile-web-check": "failure"
}
'@
  Assert-Fail "失败：mobile 计划必跑但 failure" (Invoke-GateVerify $planMobileRequired $evidenceMobileFailed) 'required_not_success:mobile-web-check=failure'

  $evidenceInvalidStatus = @'
{
  "detect-changes": "success",
  "governance-and-privacy": "success",
  "rust-check": "ok",
  "mobile-web-check": "skipped"
}
'@
  Assert-Fail "失败：非法状态串" (Invoke-GateVerify $planGovernanceRequired $evidenceInvalidStatus) 'status_invalid:rust-check=ok'

  # 多实例证据：缺项、部分失败、重复证据都必须拒绝
  $evidenceMultiPartial = @'
{
  "detect-changes": "success",
  "governance-and-privacy": "success",
  "rust-check": { "instances": [
    { "id": "ubuntu-latest", "status": "success" },
    { "id": "windows-latest", "status": "failure" }
  ] },
  "mobile-web-check": "skipped"
}
'@
  Assert-Fail "失败：多实例部分失败" (Invoke-GateVerify $planMultiInstance $evidenceMultiPartial) 'required_not_success:rust-check/windows-latest=failure'

  $evidenceMultiMissing = @'
{
  "detect-changes": "success",
  "governance-and-privacy": "success",
  "rust-check": { "instances": [
    { "id": "ubuntu-latest", "status": "success" }
  ] },
  "mobile-web-check": "skipped"
}
'@
  Assert-Fail "失败：多实例证据缺项" (Invoke-GateVerify $planMultiInstance $evidenceMultiMissing) 'instance_missing:rust-check/windows-latest'

  $evidenceMultiDuplicate = @'
{
  "detect-changes": "success",
  "governance-and-privacy": "success",
  "rust-check": { "instances": [
    { "id": "ubuntu-latest", "status": "success" },
    { "id": "ubuntu-latest", "status": "success" }
  ] },
  "mobile-web-check": "skipped"
}
'@
  Assert-Fail "失败：多实例重复证据" (Invoke-GateVerify $planMultiInstance $evidenceMultiDuplicate) 'instance_duplicate:rust-check/ubuntu-latest'

  Assert-Fail "失败：多实例被折叠为单一状态" (Invoke-GateVerify $planMultiInstance $evidenceBaseline) 'multi_instance_evidence_missing:rust-check'
  Assert-Fail "失败：单实例计划收到多实例证据" (Invoke-GateVerify $planGovernanceRequired $evidenceMultiPartial) 'evidence_shape_invalid:rust-check'

  $evidenceMissingJob = @'
{
  "detect-changes": "success",
  "governance-and-privacy": "success",
  "mobile-web-check": "skipped"
}
'@
  Assert-Fail "失败：证据缺少作业" (Invoke-GateVerify $planGovernanceRequired $evidenceMissingJob) 'evidence_missing:rust-check'

  $evidenceUnknownJob = @'
{
  "detect-changes": "success",
  "governance-and-privacy": "success",
  "rust-check": "success",
  "mobile-web-check": "skipped",
  "lint-check": "success"
}
'@
  Assert-Fail "失败：证据含未知作业" (Invoke-GateVerify $planGovernanceRequired $evidenceUnknownJob) 'evidence_unknown_job:lint-check'

  $evidenceMobileSuccessConflict = @'
{
  "detect-changes": "success",
  "governance-and-privacy": "success",
  "rust-check": "success",
  "mobile-web-check": "success"
}
'@
  Assert-Fail "失败：计划未要求却运行成功（计划与证据冲突）" (Invoke-GateVerify $planGovernanceRequired $evidenceMobileSuccessConflict) 'plan_evidence_conflict:mobile-web-check'

  Assert-Fail "失败：计划缺失" (Invoke-GateVerify $null $evidenceBaseline) 'CI-GATE-ERROR' 2
  Assert-Fail "失败：计划不是合法 JSON" (Invoke-GateVerify '{ "version": "ci-gate-plan/v1", ' $evidenceBaseline) 'CI-GATE-ERROR' 2

  $planEmptyJobs = @'
{
  "version": "ci-gate-plan/v1",
  "detector": "detect-changes",
  "gate": "ci-gate",
  "jobs": [],
  "selfTest": false,
  "releaseContract": false
}
'@
  Assert-Fail "失败：计划作业为空" (Invoke-GateVerify $planEmptyJobs $evidenceBaseline) 'plan_jobs_empty'

  $planDuplicateJob = @'
{
  "version": "ci-gate-plan/v1",
  "detector": "detect-changes",
  "gate": "ci-gate",
  "jobs": [
    { "id": "governance-and-privacy", "required": true },
    { "id": "rust-check", "required": true },
    { "id": "rust-check", "required": false },
    { "id": "mobile-web-check", "required": false }
  ],
  "selfTest": false,
  "releaseContract": false
}
'@
  Assert-Fail "失败：计划重复作业" (Invoke-GateVerify $planDuplicateJob $evidenceBaseline) 'plan_job_duplicate:rust-check'

  $planBadVersion = @'
{
  "version": "ci-gate-plan/v2",
  "detector": "detect-changes",
  "gate": "ci-gate",
  "jobs": [
    { "id": "governance-and-privacy", "required": true },
    { "id": "rust-check", "required": true },
    { "id": "mobile-web-check", "required": false }
  ],
  "selfTest": false,
  "releaseContract": false
}
'@
  Assert-Fail "失败：计划版本非法" (Invoke-GateVerify $planBadVersion $evidenceBaseline) 'plan_version_invalid'

  $planRequiredNotBoolean = @'
{
  "version": "ci-gate-plan/v1",
  "detector": "detect-changes",
  "gate": "ci-gate",
  "jobs": [
    { "id": "governance-and-privacy", "required": "true" }
  ],
  "selfTest": false,
  "releaseContract": false
}
'@
  Assert-Fail "失败：计划必跑标记不是布尔" (Invoke-GateVerify $planRequiredNotBoolean $evidenceBaseline) 'plan_job_required_invalid:governance-and-privacy'

  # ============ 二、Plan 模式与检测器输出契约 ============
  $scopeRustOnly = @'
{
  "full": false,
  "baseRef": "origin/main",
  "changedFiles": ["crates/soxu-git/src/probe.rs"],
  "governance": true,
  "rust": true,
  "rustScope": "crates",
  "rustPackages": ["soxu-git"],
  "rustPackageArgs": "-p soxu-git",
  "mobileWeb": false,
  "paseoOrchestration": false,
  "checkScopeSelfTest": false,
  "reasons": ["crate:soxu-git"]
}
'@
  $planResult = Invoke-PlanMode $scopeRustOnly
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
    Assert-Check ($null -ne $govJob -and $govJob.required -eq $true) "治理隐私任务必须必跑"
    Assert-Check ($planObject.selfTest -eq $false) "未触及门禁脚本时 selfTest 应为 false"
    Assert-Check ($planObject.releaseContract -eq $false) "未触及 release.yml 时 releaseContract 应为 false"
    Assert-Pass "通过：Plan 产物可直接供 Verify 消费" (Invoke-GateVerify ($planObject | ConvertTo-Json -Depth 10 -Compress) $evidenceBaseline)
  }

  $scopeReleaseTouched = @'
{
  "full": false,
  "baseRef": "origin/main",
  "changedFiles": [".github/workflows/release.yml"],
  "governance": true,
  "rust": false,
  "rustScope": "none",
  "rustPackages": [],
  "rustPackageArgs": "",
  "mobileWeb": false,
  "paseoOrchestration": false,
  "checkScopeSelfTest": false,
  "reasons": ["docs_or_governance:.github/workflows/release.yml"]
}
'@
  $releasePlan = Invoke-PlanMode $scopeReleaseTouched
  Assert-Pass "通过：触及 release.yml 时构建计划" $releasePlan
  if ($releasePlan.Code -eq 0) {
    $releasePlanObject = ($releasePlan.Output.Trim() | ConvertFrom-Json -Depth 10)
    Assert-Check ($releasePlanObject.releaseContract -eq $true) "触及 release.yml 时 releaseContract 必须为 true"
    Assert-Check ($releasePlanObject.selfTest -eq $false) "仅触及 release.yml 时 selfTest 应为 false"
  }

  $scopeCiTouched = @'
{
  "full": false,
  "baseRef": "origin/main",
  "changedFiles": [".github/workflows/ci.yml", "scripts/verify-ci-gate.ps1"],
  "governance": true,
  "rust": false,
  "rustScope": "none",
  "rustPackages": [],
  "rustPackageArgs": "",
  "mobileWeb": false,
  "paseoOrchestration": false,
  "checkScopeSelfTest": false,
  "reasons": ["infra_full:.github/workflows/ci.yml"]
}
'@
  $ciPlan = Invoke-PlanMode $scopeCiTouched
  Assert-Pass "通过：触及门禁脚本时构建计划" $ciPlan
  if ($ciPlan.Code -eq 0) {
    $ciPlanObject = ($ciPlan.Output.Trim() | ConvertFrom-Json -Depth 10)
    Assert-Check ($ciPlanObject.selfTest -eq $true) "触及 ci.yml 或门禁脚本时 selfTest 必须为 true"
  }

  $scopeInvalidType = @'
{
  "full": false,
  "rust": "true",
  "rustScope": "crates",
  "mobileWeb": false,
  "paseoOrchestration": false,
  "checkScopeSelfTest": false,
  "changedFiles": []
}
'@
  Assert-Fail "失败：检测器输出字段类型非法" (Invoke-PlanMode $scopeInvalidType) 'CI-GATE-ERROR' 2

  $scopeInvalidRustScope = @'
{
  "full": false,
  "rust": true,
  "rustScope": "everything",
  "mobileWeb": false,
  "paseoOrchestration": false,
  "checkScopeSelfTest": false,
  "changedFiles": ["crates/soxu-git/src/probe.rs"]
}
'@
  Assert-Fail "失败：检测器 rustScope 非法" (Invoke-PlanMode $scopeInvalidRustScope) 'CI-GATE-ERROR' 2

  # ============ 三、工作流静态契约 ============
  Assert-Pass "通过：真实 ci.yml 门禁契约" (Invoke-WorkflowContract $ciWorkflow)

  $variantContinueOnError = New-WorkflowVariant ".github/workflows/ci.yml" "ci-continue-on-error" {
    param($text)
    $text.Replace("    timeout-minutes: 5`n    steps:`n      - uses: actions/checkout@v7", "    timeout-minutes: 5`n    continue-on-error: true`n    steps:`n      - uses: actions/checkout@v7")
  }
  Assert-Fail "失败：门禁作业使用 continue-on-error" (Invoke-WorkflowContract $variantContinueOnError) 'workflow_uses_continue_on_error'

  $variantGateName = New-WorkflowVariant ".github/workflows/ci.yml" "ci-gate-name" {
    param($text)
    $text.Replace("  ci-gate:`n    name: CI Gate", "  ci-gate:`n    name: CI Gate (scoped)")
  }
  Assert-Fail "失败：门禁作业名称被改写" (Invoke-WorkflowContract $variantGateName) 'gate_name_unstable'

  $variantNeeds = New-WorkflowVariant ".github/workflows/ci.yml" "ci-gate-needs" {
    param($text)
    $text.Replace("    needs: [detect-changes, governance-and-privacy, rust-check, mobile-web-check]", "    needs: [detect-changes, governance-and-privacy, rust-check]")
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
    $text.Replace("sha256sum -c SHA256SUMS", "sha256sum SHA256SUMS")
  }
  Assert-Fail "失败：发布流水线丢失校验和回算" (Invoke-ReleaseContract $variantNoChecksum) 'release_checksum_verify_missing'

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

  # ============ 四、发布契约动态验证（bash -n + 离线桩执行） ============
  if ($null -eq $script:bashPath) {
    if ($RequireBash) {
      Assert-Check $false "发布契约动态验证要求 bash 可用，但当前环境未找到可用的 bash。"
    } else {
      Write-Host "::warning::未找到可用的 bash，跳过发布契约动态验证（CI 中必须通过）。" -ForegroundColor Yellow
    }
  } else {
    Write-Host "发布契约动态验证使用 bash：$script:bashPath" -ForegroundColor DarkCyan
    $releaseText = (Get-Content -LiteralPath $releaseWorkflow -Raw -Encoding utf8).Replace("`r`n", "`n")
    $runBlocks = Get-InlineRunBlocks $releaseText
    Assert-Check ($runBlocks.Count -ge 4) "release.yml 应包含至少 4 段内联 Bash 脚本，实际 $($runBlocks.Count)"

    $blockIndex = 0
    foreach ($block in $runBlocks) {
      $blockIndex++
      $blockPath = Join-Path $tempRoot ("release-run-" + $blockIndex + ".sh")
      Set-Content -LiteralPath $blockPath -Value $block.Body -Encoding utf8 -NoNewline
      $syntax = Invoke-Bash @("-n", (Get-PosixPath $blockPath))
      Assert-Check ($syntax.Code -eq 0) "release.yml 第 $blockIndex 段内联 Bash（约第 $($block.StartLine) 行）语法检查失败：$($syntax.Output.Trim())"
    }

    $mirrorBody = Get-RunBlockByStepName $releaseText "Mirror release assets & enforce monotonic Latest tag"
    Assert-Check ($mirrorBody.Contains("assert_release_assets dist")) "镜像脚本必须调用资产清单校验函数"

    # 正例：四平台资产 + SHA256SUMS 齐备，校验和可回算，允许继续发布
    $goodFixture = New-DistFixture -Name "good"
    $goodResult = Invoke-ReleaseMirrorCase -Name "good" -FixtureDir $goodFixture -MirrorBody $mirrorBody
    Assert-Check ($goodResult.Code -eq 0) "正例：齐备且校验通过的资产应放行，实际 exit $($goodResult.Code)：$($goodResult.Output.Trim())"
    Assert-Check ($goodResult.Output.Contains("资产清单与 SHA-256 校验全部通过")) "正例：应输出校验通过结论：$($goodResult.Output.Trim())"
    Assert-Check ($goodResult.StubLog -match '(?m)^release (create|upload)') "正例：校验通过后应继续发布动作（桩日志：$($goodResult.StubLog.Trim())）"
    Assert-Check ($goodResult.DistFiles.Count -eq 5) "正例：镜像目录应恰含 4 平台资产与 SHA256SUMS，实际 $($goodResult.DistFiles -join ',')"

    # 负例 1：缺项
    $missingFixture = New-DistFixture -Name "missing" -Assets @($releaseAssetNames[0..2]) -SumMembers @($releaseAssetNames[0..2])
    Assert-ReleaseRejection "负例：资产缺项" (Invoke-ReleaseMirrorCase -Name "missing" -FixtureDir $missingFixture -MirrorBody $mirrorBody) '资产集合不符'

    # 负例 2：额外文件
    $extraFixture = New-DistFixture -Name "extra" -ExtraFiles @("soxu-freebsd-amd64.tar.gz")
    Assert-ReleaseRejection "负例：资产存在额外文件" (Invoke-ReleaseMirrorCase -Name "extra" -FixtureDir $extraFixture -MirrorBody $mirrorBody) '资产集合不符'

    # 负例 3：清单成员重复
    $duplicateFixture = New-DistFixture -Name "duplicate" -DuplicateMember
    Assert-ReleaseRejection "负例：SHA256SUMS 成员重复" (Invoke-ReleaseMirrorCase -Name "duplicate" -FixtureDir $duplicateFixture -MirrorBody $mirrorBody) 'SHA256SUMS 存在重复成员'

    # 负例 4：路径异常（嵌套目录）
    $nestedFixture = New-DistFixture -Name "nested" -Nested
    Assert-ReleaseRejection "负例：资产目录存在嵌套路径" (Invoke-ReleaseMirrorCase -Name "nested" -FixtureDir $nestedFixture -MirrorBody $mirrorBody) '存在嵌套路径'

    # 负例 5：内容哈希不匹配
    $wrongHashFixture = New-DistFixture -Name "wrong-hash" -WrongHash
    Assert-ReleaseRejection "负例：内容哈希与清单不符" (Invoke-ReleaseMirrorCase -Name "wrong-hash" -FixtureDir $wrongHashFixture -MirrorBody $mirrorBody) 'FAILED'

    # 负例 6：缺少 SHA256SUMS
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
