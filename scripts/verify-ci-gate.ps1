param(
  [ValidateSet("Plan", "Verify", "Contract", "ReleaseContract")]
  [string]$Mode = "Verify",
  [string]$ScopePath = "",
  [string]$ScopeJson = "",
  [string]$PlanPath = "",
  [string]$PlanJson = "",
  [string]$EvidencePath = "",
  [string]$EvidenceJson = "",
  [string]$WorkflowPath = "",
  [string]$ReleaseWorkflowPath = ""
)

$ErrorActionPreference = "Stop"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$OutputEncoding = [System.Text.Encoding]::UTF8

# 本脚本是 CI Gate 的唯一判定入口，四种模式职责互不重叠：
#   Plan            依检测器输出构建带版本号的确定性门禁计划（工作流中的计划真相源）
#   Verify          逐项对账“计划”与“needs 实际证据”，仅 success 放行必跑任务
#   Contract        静态校验 ci.yml 的门禁契约（稳定名称、静态 needs、无 continue-on-error）
#   ReleaseContract 静态校验 release.yml 的四平台资产清单与校验和契约
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path

$PlanVersion = "ci-gate-plan/v1"
$DetectorJobId = "detect-changes"
$GateJobId = "ci-gate"
$ValidStatuses = @("success", "failure", "cancelled", "skipped")

# 发布仓 TanCan1998/soxu-releases 固定 SHA 33a21eb 的四平台资产精确文件名。
$ReleaseAssetNames = @(
  "soxu-linux-amd64.tar.gz",
  "soxu-windows-amd64.zip",
  "soxu-darwin-arm64.tar.gz",
  "soxu-darwin-amd64.tar.gz"
)
$ReleaseAssetManifest = ($ReleaseAssetNames -join " ")

function Get-GatedJobSpec {
  # 空 Output 代表无条件必跑；其余代表由检测器对应输出决定是否运行。
  return @(
    [pscustomobject]@{ Id = "governance-and-privacy"; Output = "" }
    [pscustomobject]@{ Id = "rust-check"; Output = "rust" }
    [pscustomobject]@{ Id = "mobile-web-check"; Output = "mobile_web" }
  )
}

function Get-GateNeedsIds {
  $ids = [System.Collections.Generic.List[string]]::new()
  $ids.Add($DetectorJobId)
  foreach ($spec in Get-GatedJobSpec) { $ids.Add($spec.Id) }
  return @($ids)
}

function Resolve-RepoFile([string]$PathValue, [string]$FallbackRelPath) {
  $candidate = if ([string]::IsNullOrWhiteSpace($PathValue)) { $FallbackRelPath } else { $PathValue }
  if ([IO.Path]::IsPathRooted($candidate)) { return $candidate }
  return (Join-Path $repoRoot $candidate)
}

function Read-JsonPayload([string]$PathValue, [string]$InlineValue, [string]$Label) {
  if (-not [string]::IsNullOrWhiteSpace($PathValue)) {
    if (-not (Test-Path -LiteralPath $PathValue -PathType Leaf)) {
      throw "$Label 文件不存在：$PathValue"
    }
    $raw = Get-Content -LiteralPath $PathValue -Raw -Encoding utf8
  } elseif (-not [string]::IsNullOrWhiteSpace($InlineValue)) {
    $raw = $InlineValue
  } else {
    throw "缺少 $Label 输入（请提供文件路径或内联 JSON）。"
  }
  if ([string]::IsNullOrWhiteSpace($raw)) { throw "$Label 内容为空。" }
  try {
    return ($raw | ConvertFrom-Json -Depth 20 -ErrorAction Stop)
  } catch {
    throw "$Label 不是合法 JSON：$($_.Exception.Message)"
  }
}

function Get-ScopeField($Scope, [string]$Name) {
  $prop = $Scope.PSObject.Properties[$Name]
  if ($null -eq $prop) { throw "变更范围 JSON 缺少字段 $Name。" }
  return $prop.Value
}

function Get-ScopeBoolean($Scope, [string]$Name) {
  $value = Get-ScopeField $Scope $Name
  if ($value -isnot [bool]) { throw "变更范围字段 $Name 必须是布尔值。" }
  return [bool]$value
}

function Get-ChangedFileList($Scope) {
  $prop = $Scope.PSObject.Properties["changedFiles"]
  if ($null -eq $prop -or $null -eq $prop.Value) { return @() }
  return @($prop.Value | ForEach-Object { ([string]$_).Replace('\', '/') })
}

function New-GatePlan($Scope) {
  if ($null -eq $Scope) { throw "变更范围 JSON 为空。" }

  $full = Get-ScopeBoolean $Scope "full"
  $rust = Get-ScopeBoolean $Scope "rust"
  $mobileWeb = Get-ScopeBoolean $Scope "mobileWeb"
  $paseoOrchestration = Get-ScopeBoolean $Scope "paseoOrchestration"
  $checkScopeSelfTest = Get-ScopeBoolean $Scope "checkScopeSelfTest"

  $rustScopeValue = Get-ScopeField $Scope "rustScope"
  $rustScope = [string]$rustScopeValue
  if ($rust -and $rustScope -notin @("workspace", "crates", "none")) {
    throw "检测器字段 rustScope 非法：$rustScope"
  }

  $changed = Get-ChangedFileList $Scope
  # 门禁自身与其依赖脚本变更时必须跑回归矩阵；发布工作流变更时必须跑发布契约校验。
  $gateTouchPaths = @(
    ".github/workflows/ci.yml",
    "scripts/verify-ci-gate.ps1",
    "scripts/test-ci-gate.ps1",
    "scripts/detect-check-scope.ps1",
    "scripts/check-all.ps1"
  )
  $selfTest = [bool]$full -or (@($changed | Where-Object { $_ -in $gateTouchPaths }).Count -gt 0)
  $releaseContract = [bool]$full -or (@($changed | Where-Object { $_ -eq ".github/workflows/release.yml" }).Count -gt 0)

  $jobs = [System.Collections.Generic.List[object]]::new()
  foreach ($spec in Get-GatedJobSpec) {
    $required = if ([string]::IsNullOrEmpty($spec.Output)) {
      $true
    } elseif ($spec.Output -eq "rust") {
      $rust
    } else {
      $mobileWeb
    }
    $job = [ordered]@{ id = $spec.Id; required = [bool]$required }
    if (-not [string]::IsNullOrEmpty($spec.Output)) { $job["depends_on"] = $spec.Output }
    $jobs.Add([pscustomobject]$job)
  }

  $scopeSummary = [pscustomobject][ordered]@{
    full                = $full
    rust                = $rust
    rustScope           = $rustScope
    mobileWeb           = $mobileWeb
    paseoOrchestration  = $paseoOrchestration
    checkScopeSelfTest  = $checkScopeSelfTest
  }

  return [pscustomobject][ordered]@{
    version         = $PlanVersion
    detector        = $DetectorJobId
    gate            = $GateJobId
    jobs            = @($jobs)
    selfTest        = [bool]$selfTest
    releaseContract = [bool]$releaseContract
    scope           = $scopeSummary
  }
}

function Invoke-PlanMode {
  $scope = Read-JsonPayload $ScopePath $ScopeJson "变更范围 JSON"
  $plan = New-GatePlan $scope
  # 单行压缩输出，便于直接写入 GITHUB_OUTPUT 与跨作业传递。
  return ($plan | ConvertTo-Json -Depth 10 -Compress)
}

function Get-EvidenceStatusValue($Evidence, [string]$JobId) {
  return $Evidence.$JobId
}

function Invoke-VerifyMode {
  $errors = [System.Collections.Generic.List[string]]::new()
  $plan = Read-JsonPayload $PlanPath $PlanJson "门禁计划 JSON"
  $evidence = Read-JsonPayload $EvidencePath $EvidenceJson "任务证据 JSON"

  if ($null -eq $plan) { $errors.Add("plan_missing"); return $errors }

  $versionProp = $plan.PSObject.Properties["version"]
  if ($null -eq $versionProp -or [string]$versionProp.Value -ne $PlanVersion) {
    $errors.Add("plan_version_invalid")
  }
  $detectorProp = $plan.PSObject.Properties["detector"]
  if ($null -eq $detectorProp -or [string]$detectorProp.Value -ne $DetectorJobId) {
    $errors.Add("plan_detector_invalid")
  }
  $gateProp = $plan.PSObject.Properties["gate"]
  if ($null -ne $gateProp -and [string]$gateProp.Value -ne $GateJobId) {
    $errors.Add("plan_gate_invalid")
  }
  foreach ($flag in @("selfTest", "releaseContract")) {
    $flagProp = $plan.PSObject.Properties[$flag]
    if ($null -ne $flagProp -and $flagProp.Value -isnot [bool]) {
      $errors.Add("plan_flag_invalid:$flag")
    }
  }

  $jobsProp = $plan.PSObject.Properties["jobs"]
  if ($null -eq $jobsProp) { $errors.Add("plan_jobs_missing"); return $errors }
  $planJobs = @($jobsProp.Value)
  if ($planJobs.Count -eq 0) { $errors.Add("plan_jobs_empty"); return $errors }

  $expectedIds = [System.Collections.Generic.List[string]]::new()
  $expectedIds.Add($DetectorJobId)
  $specMap = @{}
  foreach ($job in $planJobs) {
    if ($null -eq $job) { $errors.Add("plan_job_null"); continue }
    $idProp = $job.PSObject.Properties["id"]
    $id = if ($null -ne $idProp) { [string]$idProp.Value } else { "" }
    if ([string]::IsNullOrWhiteSpace($id)) { $errors.Add("plan_job_id_missing"); continue }
    if ($expectedIds -contains $id) { $errors.Add("plan_job_duplicate:$id"); continue }

    $requiredProp = $job.PSObject.Properties["required"]
    if ($null -eq $requiredProp -or $requiredProp.Value -isnot [bool]) {
      $errors.Add("plan_job_required_invalid:$id")
      continue
    }

    $instances = @()
    $instancesProp = $job.PSObject.Properties["instances"]
    if ($null -ne $instancesProp -and $null -ne $instancesProp.Value) {
      $instances = @($instancesProp.Value | ForEach-Object { [string]$_ })
      if ($instances.Count -eq 0) { $errors.Add("plan_instances_empty:$id") }
      if (@($instances | Sort-Object -Unique).Count -ne $instances.Count) {
        $errors.Add("plan_instances_duplicate:$id")
      }
    }

    $expectedIds.Add($id)
    $specMap[$id] = [pscustomobject]@{
      Id        = $id
      Required  = [bool]$requiredProp.Value
      Instances = $instances
    }
  }

  if ($null -eq $evidence) { $errors.Add("evidence_missing"); return $errors }

  $actualIds = @($evidence.PSObject.Properties | ForEach-Object { $_.Name })
  foreach ($id in $expectedIds) {
    if ($actualIds -notcontains $id) { $errors.Add("evidence_missing:$id") }
  }
  foreach ($name in $actualIds) {
    if ($expectedIds -notcontains $name) { $errors.Add("evidence_unknown_job:$name") }
  }

  foreach ($id in $expectedIds) {
    if ($actualIds -notcontains $id) { continue }
    $value = Get-EvidenceStatusValue $evidence $id
    $spec = $specMap[$id]

    if ($null -eq $spec) {
      # 检测器作业：必须显式给出 success，任何其他状态都让门禁失败。
      if ($value -isnot [string]) { $errors.Add("evidence_shape_invalid:$id"); continue }
      if ($value -ne "success") { $errors.Add("detector_not_success:$id=$value") }
      continue
    }

    if (-not $spec.Required) {
      if ($value -isnot [string]) { $errors.Add("evidence_shape_invalid:$id"); continue }
      if ($value -eq "success") {
        $errors.Add("plan_evidence_conflict:$id=success 但计划未要求运行")
        continue
      }
      if ($value -ne "skipped") { $errors.Add("optional_not_skipped:$id=$value") }
      continue
    }

    if ($spec.Instances.Count -gt 0) {
      if ($value -is [string]) { $errors.Add("multi_instance_evidence_missing:$id"); continue }
      $rowsProp = $value.PSObject.Properties["instances"]
      if ($null -eq $rowsProp -or $null -eq $rowsProp.Value) {
        $errors.Add("multi_instance_evidence_missing:$id")
        continue
      }
      $rows = @($rowsProp.Value)
      if ($rows.Count -eq 0) { $errors.Add("multi_instance_evidence_missing:$id"); continue }
      $seen = @{}
      $observed = [System.Collections.Generic.List[string]]::new()
      foreach ($row in $rows) {
        if ($null -eq $row) { $errors.Add("instance_row_invalid:$id"); continue }
        $rowIdProp = $row.PSObject.Properties["id"]
        $rowStatusProp = $row.PSObject.Properties["status"]
        $rowId = if ($null -ne $rowIdProp) { [string]$rowIdProp.Value } else { "" }
        if ([string]::IsNullOrWhiteSpace($rowId)) { $errors.Add("instance_id_missing:$id"); continue }
        if ($seen.ContainsKey($rowId)) { $errors.Add("instance_duplicate:$id/$rowId"); continue }
        $seen[$rowId] = $true
        $observed.Add($rowId)
        $rowStatus = if ($null -ne $rowStatusProp) { [string]$rowStatusProp.Value } else { "" }
        if ($rowStatus -notin $ValidStatuses) {
          $errors.Add("status_invalid:$id/$rowId=$rowStatus")
          continue
        }
        if ($rowStatus -ne "success") { $errors.Add("required_not_success:$id/$rowId=$rowStatus") }
      }
      foreach ($want in $spec.Instances) {
        if (-not $observed.Contains($want)) { $errors.Add("instance_missing:$id/$want") }
      }
      foreach ($got in $observed) {
        if ($spec.Instances -notcontains $got) { $errors.Add("instance_unknown:$id/$got") }
      }
      continue
    }

    if ($value -isnot [string]) { $errors.Add("evidence_shape_invalid:$id"); continue }
    if ($value -notin $ValidStatuses) { $errors.Add("status_invalid:$id=$value"); continue }
    if ($value -ne "success") { $errors.Add("required_not_success:$id=$value") }
  }

  return $errors
}

function Read-WorkflowText([string]$ResolvedPath, [string]$Label) {
  if (-not (Test-Path -LiteralPath $ResolvedPath -PathType Leaf)) {
    throw "$Label 不存在：$ResolvedPath"
  }
  return (Get-Content -LiteralPath $ResolvedPath -Raw -Encoding utf8)
}

function Split-WorkflowJobs([string]$Text) {
  $lines = @($Text -split "`n" | ForEach-Object { $_.TrimEnd("`r") })
  $jobsStart = -1
  for ($i = 0; $i -lt $lines.Count; $i++) {
    if ($lines[$i] -match '^jobs:\s*$') { $jobsStart = $i; break }
  }
  if ($jobsStart -lt 0) { throw "工作流缺少 jobs: 段。" }

  $jobs = [ordered]@{}
  $current = $null
  for ($i = $jobsStart + 1; $i -lt $lines.Count; $i++) {
    $line = $lines[$i]
    if ($line -match '^  ([A-Za-z0-9_-]+):\s*$') {
      $current = $Matches[1]
      $jobs[$current] = [System.Collections.Generic.List[string]]::new()
      continue
    }
    if ($null -ne $current) { $jobs[$current].Add($line) }
  }
  return $jobs
}

function Get-JobNeeds([System.Collections.Generic.List[string]]$BlockLines) {
  for ($i = 0; $i -lt $BlockLines.Count; $i++) {
    $line = $BlockLines[$i]
    if ($line -match '^    needs:\s*\[(.*)\]\s*$') {
      return @($Matches[1] -split ',' | ForEach-Object { $_.Trim() } | Where-Object { $_ })
    }
    if ($line -match '^    needs:\s*$') {
      $ids = [System.Collections.Generic.List[string]]::new()
      for ($j = $i + 1; $j -lt $BlockLines.Count; $j++) {
        if ($BlockLines[$j] -match '^      -\s*([A-Za-z0-9_-]+)\s*$') {
          $ids.Add($Matches[1])
          continue
        }
        break
      }
      return @($ids)
    }
  }
  return @()
}

function Assert-WorkflowShape($Jobs, $Errors) {
  if ($Jobs.Count -eq 0) { $Errors.Add("workflow_jobs_empty"); return }
  foreach ($jobId in $Jobs.Keys) {
    $text = ($Jobs[$jobId] -join "`n")
    if ($text -notmatch '(?m)^    runs-on:') { $Errors.Add("job_runs_on_missing:$jobId") }
    if ($text -notmatch '(?m)^    steps:') { $Errors.Add("job_steps_missing:$jobId") }
  }
}

function Invoke-ContractMode([string]$PathValue) {
  $errors = [System.Collections.Generic.List[string]]::new()
  $resolved = Resolve-RepoFile $PathValue ".github/workflows/ci.yml"
  $text = Read-WorkflowText $resolved "CI 工作流"

  if ($text -match "\t") { $errors.Add("workflow_contains_tab") }
  if ($text -match 'continue-on-error') { $errors.Add("workflow_uses_continue_on_error") }

  $jobs = Split-WorkflowJobs $text
  Assert-WorkflowShape $jobs $errors

  foreach ($jobId in @($GateJobId, $DetectorJobId, "governance-and-privacy", "rust-check", "mobile-web-check")) {
    if (-not $jobs.Contains($jobId)) { $errors.Add("workflow_job_missing:$jobId") }
  }
  if ($errors.Count -gt 0) { return $errors }

  # 门禁作业：稳定名称、静态 needs、always() 汇总、调用本脚本校验。
  $gateText = ($jobs[$GateJobId] -join "`n")
  if ($gateText -notmatch '(?m)^    name: CI Gate\s*$') { $errors.Add("gate_name_unstable") }
  if ($gateText -notmatch '(?m)^    if: always\(\)\s*$') { $errors.Add("gate_not_always") }
  if ($gateText -notmatch 'scripts/verify-ci-gate\.ps1') { $errors.Add("gate_verifier_missing") }
  if ($gateText -notmatch '(?m)-Mode Verify') { $errors.Add("gate_verify_mode_missing") }

  $needsList = @(Get-JobNeeds $jobs[$GateJobId])
  $expectedNeeds = @(Get-GateNeedsIds)
  if ($needsList.Count -ne $expectedNeeds.Count) {
    $errors.Add("gate_needs_mismatch:期望 $($expectedNeeds -join ',') 实际 $($needsList -join ',')")
  } else {
    foreach ($id in $expectedNeeds) {
      if ($needsList -notcontains $id) { $errors.Add("gate_needs_missing:$id") }
    }
    foreach ($id in $needsList) {
      if ($expectedNeeds -notcontains $id) { $errors.Add("gate_needs_unknown:$id") }
    }
  }

  # 检测器作业：产出计划与两个触发标志，并调用 Plan 模式构建计划。
  $detectText = ($jobs[$DetectorJobId] -join "`n")
  foreach ($outputName in @("gate_plan", "gate_self_test", "gate_release_contract")) {
    if ($detectText -notmatch "outputs\.$outputName") { $errors.Add("detect_output_missing:$outputName") }
  }
  if ($detectText -notmatch 'scripts/detect-check-scope\.ps1') { $errors.Add("detect_scope_script_missing") }
  if ($detectText -notmatch '(?m)-Mode Plan') { $errors.Add("detect_plan_mode_missing") }

  # 治理作业：按计划触发门禁回归矩阵与发布契约校验。
  $govText = ($jobs["governance-and-privacy"] -join "`n")
  if ($govText -notmatch 'scripts/test-ci-gate\.ps1') { $errors.Add("governance_selftest_missing") }
  if ($govText -notmatch "needs\.detect-changes\.outputs\.gate_self_test == 'true'") {
    $errors.Add("governance_selftest_unguarded")
  }
  if ($govText -notmatch '(?m)-Mode ReleaseContract') { $errors.Add("governance_release_contract_missing") }
  if ($govText -notmatch "needs\.detect-changes\.outputs\.gate_release_contract == 'true'") {
    $errors.Add("governance_release_contract_unguarded")
  }

  # 条件作业：必须由检测器同源输出驱动，计划内跳过才可能真实 skipped。
  $rustText = ($jobs["rust-check"] -join "`n")
  if ($rustText -notmatch "needs\.detect-changes\.outputs\.rust == 'true'") {
    $errors.Add("rust_job_ungated")
  }
  $webText = ($jobs["mobile-web-check"] -join "`n")
  if ($webText -notmatch "needs\.detect-changes\.outputs\.mobile_web == 'true'") {
    $errors.Add("mobile_web_job_ungated")
  }

  return $errors
}

function Invoke-ReleaseContractMode([string]$PathValue) {
  $errors = [System.Collections.Generic.List[string]]::new()
  $resolved = Resolve-RepoFile $PathValue ".github/workflows/release.yml"
  $text = Read-WorkflowText $resolved "发版工作流"

  if ($text -match "\t") { $errors.Add("workflow_contains_tab") }

  $jobs = Split-WorkflowJobs $text
  Assert-WorkflowShape $jobs $errors

  # 四平台资产必须逐名声明，且以清单集合而非数量放行。
  $assetPattern = ($ReleaseAssetNames | ForEach-Object { [regex]::Escape($_) }) -join ' '
  $manifestPattern = "EXPECTED_RELEASE_ASSETS\s*[:=]\s*`"?$assetPattern`"?"
  if ($text -notmatch $manifestPattern) { $errors.Add("release_asset_manifest_missing") }
  if (@($ReleaseAssetNames | Where-Object { -not $text.Contains($_) }).Count -gt 0) {
    $errors.Add("release_asset_name_missing")
  }
  if ($text -match '\.assets\s*\|\s*length') { $errors.Add("release_count_only_gate") }
  if ($text -match '\$EXPECTED_ASSETS\b') { $errors.Add("release_count_only_gate") }
  if ($text -match 'EXPECTED_ASSETS:') { $errors.Add("release_count_only_gate") }

  # 校验和回算、集合声明与结构防护必须同时存在。
  if ($text -notmatch 'sha256sum -c SHA256SUMS') { $errors.Add("release_checksum_verify_missing") }
  if ($text -notmatch 'assert_release_assets') { $errors.Add("release_asset_assertion_missing") }
  if ($text -notmatch 'uniq -d') { $errors.Add("release_duplicate_member_check_missing") }
  if ($text -notmatch '\-mindepth 2') { $errors.Add("release_nested_path_check_missing") }
  if ($text -notmatch 'gh release download') { $errors.Add("release_download_missing") }
  if ($text -notmatch 'sort -V') { $errors.Add("release_monotonic_latest_missing") }
  if ($text -notmatch '--latest=') { $errors.Add("release_latest_flag_missing") }

  return $errors
}

function Write-CheckResult([System.Collections.Generic.List[string]]$Errors, [string]$ModeName) {
  if ($Errors.Count -eq 0) {
    Write-Host "✓ CI 门禁校验通过（模式：$ModeName）" -ForegroundColor Green
    return 0
  }
  foreach ($item in $Errors) {
    Write-Host "::error::CI-GATE-FAIL $item" -ForegroundColor Red
  }
  Write-Host "✗ CI 门禁校验失败（模式：$ModeName，共 $($Errors.Count) 项）" -ForegroundColor Red
  return 1
}

try {
  switch ($Mode) {
    "Plan" {
      $planText = Invoke-PlanMode
      Write-Output $planText
      exit 0
    }
    "Verify" {
      $errors = Invoke-VerifyMode
      exit (Write-CheckResult $errors "Verify")
    }
    "Contract" {
      $errors = Invoke-ContractMode $WorkflowPath
      exit (Write-CheckResult $errors "Contract")
    }
    "ReleaseContract" {
      $errors = Invoke-ReleaseContractMode $ReleaseWorkflowPath
      exit (Write-CheckResult $errors "ReleaseContract")
    }
    default {
      throw "未知模式：$Mode"
    }
  }
} catch {
  Write-Host "::error::CI-GATE-ERROR $($_.Exception.Message)" -ForegroundColor Red
  exit 2
}
