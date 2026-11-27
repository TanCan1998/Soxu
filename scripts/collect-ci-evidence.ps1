param(
  [ValidateSet("Stage", "Finalize")]
  [string]$Mode = "Finalize",

  # Stage 模式：逐步追加到同一个阶段文件，避免在工作流里拼 JSON 字符串。
  [string]$StagesPath = "",
  [string]$Name = "",
  [string]$Command = "",
  [int]$ExitCode = 0,
  [double]$DurationMs = -1,
  [string]$LogPath = "",
  [string]$StageStatus = "",
  [string]$CountsJson = "",

  # Finalize 模式：合成最终脱敏证据 JSON。
  [string]$OutputPath = "",
  [ValidateSet("platform", "job")]
  [string]$Kind = "job",
  [string]$Platform = "",
  [string]$JobId = "",
  [string]$RunnerLabel = "",
  [string]$Os = "",
  [string]$Arch = "",
  [string]$RunId = "",
  [string]$RunAttempt = "",
  [string]$Repository = "",
  [string]$EventName = "",
  [string]$BaseSha = "",
  [string]$HeadSha = "",
  [string]$CheckoutSha = "",
  [string]$WorkflowRef = "",
  # 门禁计划原文：其规范化字节的 SHA-256 就是本证据绑定的计划身份。
  [string]$PlanJson = "",
  [string]$PlanDigest = "",
  [string]$RustVersion = "",
  [string]$CargoVersion = "",
  [string]$NodeVersion = "",
  [string]$PnpmVersion = "",
  [string]$StartedAt = "",
  [string]$CompletedAt = "",
  [string]$ResultStatus = "",
  [string]$Reason = ""
)

$ErrorActionPreference = "Stop"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$OutputEncoding = [System.Text.Encoding]::UTF8

# 证据契约版本：加法演进不改变既有字段语义。
$EvidenceSchemaVersion = 1
$EvidenceKindPlatform = "soxu-ci-evidence/platform/v1"
$EvidenceKindJob = "soxu-ci-evidence/job/v1"
$StageStatuses = @("passed", "test_failed", "build_failed", "failed", "skipped", "not_run", "cancelled")

# 脱敏：证据会上传为 artifact，绝不允许携带完整环境变量、真实配置、凭据或会话历史。
function Get-SanitizedText([string]$Text) {
  if ([string]::IsNullOrWhiteSpace($Text)) { return "" }
  $value = $Text
  # 绝对路径（含 POSIX 与盘符）、UNC、URL 一律收敛为占位符。
  $value = [regex]::Replace($value, '(?i)\b[a-z]:[\\/][^\s"'']*', '<path>')
  $value = [regex]::Replace($value, '(?i)\\\\[^\s"'']+', '<unc>')
  $value = [regex]::Replace($value, '(?<![A-Za-z0-9])/(?:[^\s"'':]+/)+[^\s"'':]*', '<path>')
  $value = [regex]::Replace($value, '(?i)[a-z][a-z0-9+.\-]*://\S+', '<url>')
  # 凭据形态：token=…、password: …、Authorization: Bearer … 等。
  $value = [regex]::Replace($value, '(?i)\b(token|password|passwd|secret|api[_-]?key|authorization)\b\s*[:=]\s*\S+', '$1=<redacted>')
  $value = [regex]::Replace($value, '(?i)\bbearer\s+[A-Za-z0-9._\-]+', 'Bearer <redacted>')
  # 宿主用户目录名不得进入 artifact。
  $value = [regex]::Replace($value, '(?i)\busers[\\/][^\\/\s]+', 'users/<user>')
  $value = [regex]::Replace($value, '(?i)\bhome[\\/][^\\/\s]+', 'home/<user>')
  $value = [regex]::Replace($value, '\s+', ' ').Trim()
  if ($value.Length -gt 200) { $value = $value.Substring(0, 200) + '…' }
  return $value
}

# Runner 观测归一化：GitHub 的 RUNNER_OS/RUNNER_ARCH 是原生大写形式
# （Windows/Linux/macOS、X64/ARM64），而计划用平台目录的小写 id。
# 这里只做事实归一，不拿矩阵期望值充当成实际观测；无法归类的值直接失败。
$OsAliases = @{ windows = "windows"; linux = "linux"; macos = "macos"; darwin = "macos" }
$ArchAliases = @{ x64 = "x64"; x86_64 = "x64"; amd64 = "x64"; x86 = "x86"; arm64 = "arm64"; aarch64 = "arm64"; arm = "arm" }

function ConvertTo-CanonicalOs([string]$Value) {
  if ([string]::IsNullOrWhiteSpace($Value)) { throw "最终证据缺少 Runner 操作系统观测（请传入实际 RUNNER_OS）。" }
  $key = $Value.Trim().ToLowerInvariant()
  if (-not $OsAliases.ContainsKey($key)) { throw "无法归类的 Runner 操作系统观测：$Value" }
  return $OsAliases[$key]
}

function ConvertTo-CanonicalArch([string]$Value) {
  if ([string]::IsNullOrWhiteSpace($Value)) { throw "最终证据缺少 Runner CPU 架构观测（请传入实际 RUNNER_ARCH）。" }
  $key = $Value.Trim().ToLowerInvariant()
  if (-not $ArchAliases.ContainsKey($key)) { throw "无法归类的 Runner CPU 架构观测：$Value" }
  return $ArchAliases[$key]
}

function Read-Stages {
  if ([string]::IsNullOrWhiteSpace($StagesPath)) { return @() }
  if (-not (Test-Path -LiteralPath $StagesPath -PathType Leaf)) { return @() }
  $raw = Get-Content -LiteralPath $StagesPath -Raw -Encoding utf8
  if ([string]::IsNullOrWhiteSpace($raw)) { return @() }
  try {
    $parsed = $raw | ConvertFrom-Json -Depth 12 -ErrorAction Stop
  } catch {
    throw "阶段文件不是合法 JSON：$StagesPath"
  }
  return @($parsed)
}

function Write-Stages([object[]]$Stages) {
  $directory = Split-Path -Parent $StagesPath
  if (-not [string]::IsNullOrEmpty($directory)) {
    New-Item -ItemType Directory -Path $directory -Force | Out-Null
  }
  $json = ConvertTo-Json -InputObject @($Stages) -Depth 12 -Compress
  Set-Content -LiteralPath $StagesPath -Value $json -Encoding utf8 -NoNewline
}

# 从 cargo 测试/构建日志里提取真实计数与失败分型。
# 无法解析计数时保持 countsKnown=false，绝不用 0 冒充“0 项通过”。
function Get-LogObservation([string]$Path) {
  $observation = [pscustomobject]@{
    CountsKnown = $false
    Counts      = $null
    BuildFailed = $false
    TestsFailed = $false
  }
  if ([string]::IsNullOrWhiteSpace($Path) -or -not (Test-Path -LiteralPath $Path -PathType Leaf)) {
    return $observation
  }
  $text = Get-Content -LiteralPath $Path -Raw -Encoding utf8
  if ([string]::IsNullOrEmpty($text)) { return $observation }

  # 编译中断：Rust 报错前缀或“could not compile”出现在输出里。
  if ($text -match 'error\[E[0-9]+\]' -or $text -match 'could not compile') {
    $observation.BuildFailed = $true
  }

  $passed = 0
  $failed = 0
  $ignored = 0
  $measured = 0
  $filtered = 0
  $seen = $false
  foreach ($match in [regex]::Matches($text, 'test result:\s*(?<verdict>[A-Za-z]+)\.\s*(?<p>\d+)\s+passed;\s*(?<f>\d+)\s+failed;\s*(?<i>\d+)\s+ignored;\s*(?<m>\d+)\s+measured;\s*(?<x>\d+)\s+filtered out')) {
    $seen = $true
    $passed += [int]$match.Groups['p'].Value
    $failed += [int]$match.Groups['f'].Value
    $ignored += [int]$match.Groups['i'].Value
    $measured += [int]$match.Groups['m'].Value
    $filtered += [int]$match.Groups['x'].Value
    if ($match.Groups['verdict'].Value -eq 'FAILED') { $observation.TestsFailed = $true }
  }
  if ($seen) {
    $observation.CountsKnown = $true
    $observation.Counts = [ordered]@{
      passed   = $passed
      failed   = $failed
      ignored  = $ignored
      measured = $measured
      filtered = $filtered
    }
    if ($failed -gt 0) { $observation.TestsFailed = $true }
  }
  return $observation
}

function Resolve-StageStatus([string]$Requested, [int]$Code, $Observation) {
  if (-not [string]::IsNullOrWhiteSpace($Requested)) {
    if ($StageStatuses -notcontains $Requested) {
      throw "非法阶段状态：$Requested（允许 $($StageStatuses -join '/')）。"
    }
    return $Requested
  }
  # 编译失败优先于测试失败：编译没通过时“测试失败”是噪声结论。
  if ($null -ne $Observation -and $Observation.BuildFailed) { return "build_failed" }
  if ($Code -eq 0) { return "passed" }
  if ($null -ne $Observation -and $Observation.TestsFailed) { return "test_failed" }
  return "failed"
}

function Invoke-StageMode {
  if ([string]::IsNullOrWhiteSpace($StagesPath)) { throw "Stage 模式必须提供 -StagesPath。" }
  if ([string]::IsNullOrWhiteSpace($Name)) { throw "Stage 模式必须提供 -Name。" }

  $observation = Get-LogObservation $LogPath
  $status = Resolve-StageStatus $StageStatus $ExitCode $observation

  $counts = $null
  $countsKnown = $false
  if (-not [string]::IsNullOrWhiteSpace($CountsJson)) {
    try {
      $counts = $CountsJson | ConvertFrom-Json -Depth 6 -ErrorAction Stop
      $countsKnown = $true
    } catch {
      throw "CountsJson 不是合法 JSON。"
    }
  } elseif ($null -ne $observation -and $observation.CountsKnown) {
    $counts = $observation.Counts
    $countsKnown = $true
  }

  $record = [ordered]@{
    name          = Get-SanitizedText $Name
    command       = Get-SanitizedText $Command
    status        = $status
    exitCode      = [int]$ExitCode
    durationMs    = if ($DurationMs -ge 0) { [int]$DurationMs } else { $null }
    countsKnown   = [bool]$countsKnown
    counts        = $counts
  }

  $stages = [System.Collections.Generic.List[object]]::new()
  # Read-Stages 的返回值经管道会展开，单条记录会变成标量；这里逐项归入列表，
  # 避免第二、第三次调用时对 PSObject 做数组加法（PSObject 没有 op_Addition）。
  foreach ($existing in @(Read-Stages)) {
    if ($null -ne $existing) { $stages.Add($existing) }
  }
  $stages.Add([pscustomobject]$record)
  Write-Stages $stages.ToArray()
  Write-Host "证据阶段已记录：$($record.name) status=$status countsKnown=$countsKnown"
  return 0
}

function Get-PlanDigest([string]$Json, [string]$Explicit) {
  if (-not [string]::IsNullOrWhiteSpace($Explicit)) { return $Explicit }
  if ([string]::IsNullOrWhiteSpace($Json)) { throw "Finalize 模式必须提供 -PlanJson 或 -PlanDigest。" }
  $bytes = [System.Text.Encoding]::UTF8.GetBytes($Json.Trim())
  $sha = [System.Security.Cryptography.SHA256]::Create()
  try {
    return ([System.BitConverter]::ToString($sha.ComputeHash($bytes)) -replace '-', '').ToLowerInvariant()
  } finally {
    $sha.Dispose()
  }
}

function Invoke-FinalizeMode {
  if ([string]::IsNullOrWhiteSpace($OutputPath)) { throw "Finalize 模式必须提供 -OutputPath。" }

  $stages = @(Read-Stages)
  $failedStatuses = @("test_failed", "build_failed", "failed")
  $stagesFailed = @($stages | Where-Object { $failedStatuses -contains [string]$_.status }).Count -gt 0
  $stagesRan = @($stages | Where-Object { [string]$_.status -notin @("skipped", "not_run") }).Count -gt 0

  $resultStatus = if (-not [string]::IsNullOrWhiteSpace($ResultStatus)) {
    $ResultStatus
  } elseif ($stagesFailed) {
    "failed"
  } elseif (-not $stagesRan) {
    "not_run"
  } else {
    "passed"
  }

  $evidenceKind = if ($Kind -eq "platform") { $EvidenceKindPlatform } else { $EvidenceKindJob }
  if ($Kind -eq "platform" -and [string]::IsNullOrWhiteSpace($Platform)) {
    throw "platform 证据必须提供 -Platform。"
  }
  if ($Kind -eq "job" -and [string]::IsNullOrWhiteSpace($JobId)) {
    throw "job 证据必须提供 -JobId。"
  }

  $evidence = [ordered]@{
    schemaVersion = $EvidenceSchemaVersion
    kind          = $evidenceKind
    run           = [ordered]@{
      id         = [string]$RunId
      attempt    = [string]$RunAttempt
      repository = [string]$Repository
      event      = [string]$EventName
    }
    plan          = [ordered]@{
      digest = Get-PlanDigest $PlanJson $PlanDigest
    }
    identity      = [ordered]@{
      baseSha     = [string]$BaseSha
      headSha     = [string]$HeadSha
      checkoutSha = [string]$CheckoutSha
      workflowRef = [string]$WorkflowRef
    }
    runner        = [ordered]@{
      # label 是矩阵预期绑定；os/arch 必须来自实际 RUNNER_OS/RUNNER_ARCH 观测。
      label = Get-SanitizedText $RunnerLabel
      os    = ConvertTo-CanonicalOs $Os
      arch  = ConvertTo-CanonicalArch $Arch
    }
    platform      = [string]$Platform
    job           = [string]$JobId
    toolchain     = [ordered]@{
      rust  = Get-SanitizedText $RustVersion
      cargo = Get-SanitizedText $CargoVersion
      node  = Get-SanitizedText $NodeVersion
      pnpm  = Get-SanitizedText $PnpmVersion
    }
    timing        = [ordered]@{
      startedAt   = [string]$StartedAt
      completedAt = [string]$CompletedAt
    }
    stages        = @($stages)
    result        = [ordered]@{
      status = $resultStatus
      reason = Get-SanitizedText $Reason
    }
  }

  $directory = Split-Path -Parent $OutputPath
  if (-not [string]::IsNullOrEmpty($directory)) {
    New-Item -ItemType Directory -Path $directory -Force | Out-Null
  }
  $json = ConvertTo-Json -InputObject $evidence -Depth 12
  Set-Content -LiteralPath $OutputPath -Value $json -Encoding utf8 -NoNewline
  Write-Host "脱敏证据已写入：$(Split-Path -Leaf $OutputPath)（stages=$($stages.Count) result=$resultStatus）"
  return 0
}

switch ($Mode) {
  "Stage" { exit (Invoke-StageMode) }
  "Finalize" { exit (Invoke-FinalizeMode) }
  default { throw "未知模式：$Mode" }
}
