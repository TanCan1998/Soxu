param(
  [ValidateSet("Plan", "Verify", "Contract", "ReleaseContract", "ExtractRuns")]
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
#   Verify          以“固定 job 契约 + scope 重新推导”对账计划与 needs 实际证据
#   Contract        静态校验 ci.yml 的门禁契约（稳定名称、静态 needs、无 continue-on-error）
#   ReleaseContract 静态校验 release.yml 的四平台资产清单与校验和契约
#
# 设计红线：计划是数据，不能凭删 job、换 job、翻转 required 或篡改触发标志削弱门禁。
# Verify 会用与 Plan 完全相同的推导函数，从计划自带的 scope 重新算出期望计划再逐项比对。
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path

$PlanVersion = "ci-gate-plan/v1"
$DetectorJobId = "detect-changes"
$GateJobId = "ci-gate"
$ValidStatuses = @("success", "failure", "cancelled", "skipped")
$ValidRustScopes = @("workspace", "crates", "none")

# 计划路径契约。检测器输出的结构化 scope 带 schemaVersion=1，路径一律是仓库相对规范
# 路径且原样传递：POSIX 合法的字面反斜杠不得被改写成分隔符，否则一个路径会变成另一个
# 路径并误触发专项门禁。旧的无版本 scope 继续接受既有 Windows 路径格式并归一化。
$CurrentScopeSchemaVersion = 1
$PathContractVersioned = "repo-relative/v1"
$PathContractLegacy = "repo-relative/legacy"

# 发布仓 TanCan1998/soxu-releases 固定 SHA 33a21eb 的四平台资产精确文件名。
$ReleaseAssetNames = @(
  "soxu-linux-amd64.tar.gz"
  "soxu-windows-amd64.zip"
  "soxu-darwin-arm64.tar.gz"
  "soxu-darwin-amd64.tar.gz"
)
$ReleaseAssetManifest = ($ReleaseAssetNames -join " ")

# 当前 CI-A 固定 job 契约：governance-and-privacy 无条件必跑，其余由检测器同源输出决定。
# CI-C 的三平台动态矩阵只扩展实例证据，不改变这里的 job 集合。
$GatedJobSpecs = @(
  [pscustomobject]@{ Id = "governance-and-privacy"; Output = ""; Unconditional = $true }
  [pscustomobject]@{ Id = "rust-check"; Output = "rust"; Unconditional = $false }
  [pscustomobject]@{ Id = "mobile-web-check"; Output = "mobile_web"; Unconditional = $false }
)

function Get-GatedJobSpec {
  return $GatedJobSpecs
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

function Get-ScopeSchemaVersion($Scope) {
  $prop = $Scope.PSObject.Properties["schemaVersion"]
  if ($null -eq $prop -or $null -eq $prop.Value) { return 0 }
  $value = $prop.Value
  if ($value -isnot [int] -and $value -isnot [long]) {
    throw "变更范围字段 schemaVersion 必须是整数。"
  }
  if ([int]$value -lt 0 -or [int]$value -gt $CurrentScopeSchemaVersion) {
    throw "不支持的变更范围 schemaVersion：$value（当前支持 0..$CurrentScopeSchemaVersion）。"
  }
  return [int]$value
}

function Assert-VersionedRepoPath([string]$PathValue, [string]$Origin) {
  # 只拒绝真正越界或语义不确定的写法；字面反斜杠、逗号、空白与非 ASCII 全部合法。
  if ($PathValue.IndexOf([char]0) -ge 0) { throw "变更范围路径 $Origin 含 NUL 字符。" }
  if ($PathValue.StartsWith("/")) { throw "变更范围路径 $Origin 必须是仓库相对路径，不接受绝对路径。" }
  if ($PathValue -match '^[A-Za-z]:') { throw "变更范围路径 $Origin 必须是仓库相对路径，不接受盘符路径。" }
  if ($PathValue.EndsWith("/")) { throw "变更范围路径 $Origin 不能以 / 结尾。" }
  foreach ($segment in $PathValue.Split('/')) {
    if ($segment -eq "" -or $segment -eq "." -or $segment -eq "..") {
      throw "变更范围路径 $Origin 含空段或相对段：$PathValue"
    }
  }
}

function Get-ChangedFileList($Scope) {
  # changedFiles 是门禁专项触发（selfTest / releaseContract）的唯一输入来源，
  # 缺失、null 或含非字符串条目都必须失败关闭，不能静默退化为“未触及工作流”。
  # 归一化强度由 scope 的路径契约决定：版本化 scope 原样保留，旧 scope 归一化反斜杠。
  $prop = $Scope.PSObject.Properties["changedFiles"]
  if ($null -eq $prop) { throw "变更范围 JSON 缺少字段 changedFiles。" }
  if ($null -eq $prop.Value) { throw "变更范围字段 changedFiles 不能为 null。" }

  $schemaVersion = Get-ScopeSchemaVersion $Scope
  $files = [System.Collections.Generic.List[string]]::new()
  $index = 0
  foreach ($item in @($prop.Value)) {
    $index++
    if ($item -isnot [string]) { throw "变更范围字段 changedFiles 含非字符串条目。" }
    if ([string]::IsNullOrWhiteSpace($item)) { throw "变更范围字段 changedFiles 含空路径。" }
    if ($schemaVersion -ge $CurrentScopeSchemaVersion) {
      Assert-VersionedRepoPath $item "第 $index 项"
      $files.Add($item)
    } else {
      $files.Add($item.Replace('\', '/'))
    }
  }
  return @($files)
}

function Assert-ScopeConsistency($Scope, [bool]$Full, [bool]$Rust, [string]$RustScope, [bool]$MobileWeb, [bool]$Paseo, [bool]$ScopeSelfTest) {
  if (-not $ValidRustScopes.Contains($RustScope)) {
    throw "检测器字段 rustScope 非法：$RustScope（允许 workspace/crates/none）。"
  }
  if ($Rust -and $RustScope -eq "none") {
    throw "rust=true 与 rustScope=none 矛盾，拒绝生成跳过 Rust 的计划。"
  }
  if (-not $Rust -and $RustScope -ne "none") {
    throw "rust=false 时 rustScope 必须为 none，实际：$RustScope。"
  }
  if ($Full) {
    if (-not ($Rust -and $RustScope -eq "workspace")) {
      throw "full=true 与 Rust 跳过冲突：全量回退必须按 workspace 跑 Rust。"
    }
    if (-not ($MobileWeb -and $Paseo -and $ScopeSelfTest)) {
      throw "full=true 与分级跳过冲突：全量回退必须覆盖 PWA、Paseo 编排与作用域自测。"
    }
  }
}

function New-GatePlan($Scope) {
  if ($null -eq $Scope) { throw "变更范围 JSON 为空。" }

  $full = Get-ScopeBoolean $Scope "full"
  $rust = Get-ScopeBoolean $Scope "rust"
  $mobileWeb = Get-ScopeBoolean $Scope "mobileWeb"
  $paseoOrchestration = Get-ScopeBoolean $Scope "paseoOrchestration"
  $checkScopeSelfTest = Get-ScopeBoolean $Scope "checkScopeSelfTest"

  $rustScopeValue = Get-ScopeField $Scope "rustScope"
  if ($rustScopeValue -isnot [string]) { throw "变更范围字段 rustScope 必须是字符串。" }
  $rustScope = [string]$rustScopeValue

  Assert-ScopeConsistency $Scope $full $rust $rustScope $mobileWeb $paseoOrchestration $checkScopeSelfTest

  $schemaVersion = Get-ScopeSchemaVersion $Scope
  $pathContract = if ($schemaVersion -ge $CurrentScopeSchemaVersion) { $PathContractVersioned } else { $PathContractLegacy }
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
    $required = if ($spec.Unconditional) {
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
    # 设问：这个 scope 用的是哪种路径契约。Verify 依此重算，不能靠猜。
    schemaVersion      = [int]$schemaVersion
    full               = $full
    rust               = $rust
    rustScope          = $rustScope
    mobileWeb          = $mobileWeb
    paseoOrchestration = $paseoOrchestration
    checkScopeSelfTest = $checkScopeSelfTest
    # 随计划回写变更文件，使 Verify 能用同一推导函数重算 selfTest / releaseContract。
    changedFiles       = @($changed)
  }

  return [pscustomobject][ordered]@{
    version         = $PlanVersion
    # 显式声明路径契约，供 Verify 与下游消费者区分版本化入口和旧 Windows 入口。
    pathContract    = $pathContract
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

# 计划契约校验：计划结构本身不得削弱门禁，必须与 Plan 的推导结果逐项一致。
function Test-PlanContract($Plan, [System.Collections.Generic.List[string]]$Errors) {
  $versionProp = $Plan.PSObject.Properties["version"]
  if ($null -eq $versionProp -or $versionProp.Value -isnot [string] -or $versionProp.Value -ne $PlanVersion) {
    $Errors.Add("plan_version_invalid")
  }
  $detectorProp = $Plan.PSObject.Properties["detector"]
  if ($null -eq $detectorProp -or $detectorProp.Value -isnot [string] -or $detectorProp.Value -ne $DetectorJobId) {
    $Errors.Add("plan_detector_invalid")
  }
  $gateProp = $Plan.PSObject.Properties["gate"]
  if ($null -eq $gateProp) {
    $Errors.Add("plan_gate_missing")
  } elseif ($gateProp.Value -isnot [string] -or $gateProp.Value -ne $GateJobId) {
    $Errors.Add("plan_gate_invalid")
  }

  foreach ($flag in @("selfTest", "releaseContract")) {
    $flagProp = $Plan.PSObject.Properties[$flag]
    if ($null -eq $flagProp) {
      $Errors.Add("plan_flag_missing:$flag")
    } elseif ($flagProp.Value -isnot [bool]) {
      $Errors.Add("plan_flag_invalid:$flag")
    }
  }

  $scopeProp = $Plan.PSObject.Properties["scope"]
  if ($null -eq $scopeProp -or $null -eq $scopeProp.Value) {
    $Errors.Add("plan_scope_missing")
    return
  }

  # 用与 Plan 相同的推导函数重算期望计划；scope 自身矛盾在这里直接暴露。
  try {
    $expected = New-GatePlan $scopeProp.Value
  } catch {
    $Errors.Add("plan_scope_invalid:$($_.Exception.Message)")
    return
  }

  foreach ($flag in @("selfTest", "releaseContract")) {
    $flagProp = $Plan.PSObject.Properties[$flag]
    if ($null -ne $flagProp -and $flagProp.Value -is [bool] -and [bool]$flagProp.Value -ne [bool]$expected.$flag) {
      $Errors.Add("plan_flag_conflict:$flag")
    }
  }

  # 路径契约必须与 scope 推导一致：旧 scope 不能自封为版本化入口来保留反斜杠，
  # 版本化 scope 也不能被降级为本机 Windows 形式，否则两个入口的路径身份会互相冒用。
  $pathContractProp = $Plan.PSObject.Properties["pathContract"]
  if ($null -eq $pathContractProp -or $pathContractProp.Value -isnot [string] -or [string]::IsNullOrWhiteSpace($pathContractProp.Value)) {
    $Errors.Add("plan_path_contract_missing")
  } elseif ([string]$pathContractProp.Value -ne [string]$expected.pathContract) {
    $Errors.Add("plan_path_contract_conflict")
  }

  $jobsProp = $Plan.PSObject.Properties["jobs"]
  if ($null -eq $jobsProp) { $Errors.Add("plan_jobs_missing"); return }
  $declaredJobs = @($jobsProp.Value)
  if ($declaredJobs.Count -eq 0) { $Errors.Add("plan_jobs_empty"); return }

  $declared = @{}
  foreach ($job in $declaredJobs) {
    if ($null -eq $job -or $job -isnot [pscustomobject]) { $Errors.Add("plan_job_invalid"); continue }
    $idProp = $job.PSObject.Properties["id"]
    if ($null -eq $idProp -or $idProp.Value -isnot [string] -or [string]::IsNullOrWhiteSpace($idProp.Value)) {
      $Errors.Add("plan_job_id_invalid")
      continue
    }
    $id = [string]$idProp.Value
    if ($declared.ContainsKey($id)) { $Errors.Add("plan_job_duplicate:$id"); continue }

    $requiredProp = $job.PSObject.Properties["required"]
    if ($null -eq $requiredProp -or $requiredProp.Value -isnot [bool]) {
      $Errors.Add("plan_job_required_invalid:$id")
      continue
    }

    $dependsOn = $null
    $dependsProp = $job.PSObject.Properties["depends_on"]
    if ($null -ne $dependsProp) {
      if ($dependsProp.Value -isnot [string] -or [string]::IsNullOrWhiteSpace($dependsProp.Value)) {
        $Errors.Add("plan_job_depends_invalid:$id")
        continue
      }
      $dependsOn = [string]$dependsProp.Value
    }

    $instances = [System.Collections.Generic.List[string]]::new()
    $instancesProp = $job.PSObject.Properties["instances"]
    if ($null -ne $instancesProp) {
      if ($null -eq $instancesProp.Value) { $Errors.Add("plan_instances_null:$id"); continue }
      $rawInstances = @($instancesProp.Value)
      if ($rawInstances.Count -eq 0) { $Errors.Add("plan_instances_empty:$id"); continue }
      $seenInstance = @{}
      foreach ($instance in $rawInstances) {
        if ($instance -isnot [string] -or [string]::IsNullOrWhiteSpace($instance)) {
          $Errors.Add("plan_instances_invalid:$id")
          continue
        }
        if ($seenInstance.ContainsKey([string]$instance)) { $Errors.Add("plan_instances_duplicate:$id"); continue }
        $seenInstance[[string]$instance] = $true
        $instances.Add([string]$instance)
      }
    }

    $declared[$id] = [pscustomobject]@{
      Id        = $id
      Required  = [bool]$requiredProp.Value
      DependsOn = $dependsOn
      Instances = @($instances)
    }
  }

  foreach ($spec in Get-GatedJobSpec) {
    if (-not $declared.ContainsKey($spec.Id)) { $Errors.Add("plan_job_missing:$($spec.Id)"); continue }
    $entry = $declared[$spec.Id]
    # 期望的 required 直接从推导出的计划读取，避免与规格重复实现
    $expectedJob = @($expected.jobs | Where-Object { $_.id -eq $spec.Id })[0]
    if ($null -ne $expectedJob -and $entry.Required -ne [bool]$expectedJob.required) {
      $Errors.Add("plan_job_required_conflict:$($spec.Id)")
    }
    if ($spec.Unconditional) {
      if (-not [string]::IsNullOrEmpty($entry.DependsOn)) { $Errors.Add("plan_job_depends_conflict:$($spec.Id)") }
    } else {
      if ($entry.DependsOn -ne $spec.Output) { $Errors.Add("plan_job_depends_conflict:$($spec.Id)") }
    }
  }

  foreach ($id in $declared.Keys) {
    if (-not (@(Get-GatedJobSpec | ForEach-Object { $_.Id }) -contains $id)) {
      $Errors.Add("plan_job_unknown:$id")
    }
  }

  if ($declared.Count -ne @(Get-GatedJobSpec).Count) {
    $Errors.Add("plan_job_set_mismatch")
  }
}

function Invoke-VerifyMode {
  $errors = [System.Collections.Generic.List[string]]::new()
  $plan = Read-JsonPayload $PlanPath $PlanJson "门禁计划 JSON"
  $evidence = Read-JsonPayload $EvidencePath $EvidenceJson "任务证据 JSON"

  if ($null -eq $plan) { $errors.Add("plan_missing"); return $errors }

  Test-PlanContract $plan $errors
  if ($errors.Count -gt 0) { return $errors }

  $planJobs = @($plan.jobs)
  $specMap = @{}
  foreach ($job in $planJobs) { $specMap[[string]$job.id] = $job }

  if ($null -eq $evidence) { $errors.Add("evidence_missing"); return $errors }

  $expectedIds = Get-GateNeedsIds
  $actualIds = @($evidence.PSObject.Properties | ForEach-Object { $_.Name })
  foreach ($id in $expectedIds) {
    if ($actualIds -notcontains $id) { $errors.Add("evidence_missing:$id") }
  }
  foreach ($name in $actualIds) {
    if ($expectedIds -notcontains $name) { $errors.Add("evidence_unknown_job:$name") }
  }

  foreach ($id in $expectedIds) {
    if ($actualIds -notcontains $id) { continue }
    $value = $evidence.$id

    if ($id -eq $DetectorJobId) {
      # 检测器作业：必须显式给出 success，任何其他状态都让门禁失败。
      if ($value -isnot [string]) { $errors.Add("evidence_shape_invalid:$id"); continue }
      if ($value -ne "success") { $errors.Add("detector_not_success:$id=$value") }
      continue
    }

    $job = $specMap[$id]
    if ($null -eq $job) { $errors.Add("evidence_unknown_job:$id"); continue }
    $required = [bool]$job.required
    $instances = @()
    if ($null -ne $job.PSObject.Properties["instances"]) { $instances = @($job.instances) }

    if (-not $required) {
      if ($value -isnot [string]) { $errors.Add("evidence_shape_invalid:$id"); continue }
      if (-not $ValidStatuses.Contains([string]$value)) { $errors.Add("status_invalid:$id=$value"); continue }
      if ($value -eq "success") {
        $errors.Add("plan_evidence_conflict:$id=success 但计划未要求运行")
        continue
      }
      if ($value -ne "skipped") { $errors.Add("optional_not_skipped:$id=$value") }
      continue
    }

    if ($instances.Count -gt 0) {
      if ($value -is [string]) { $errors.Add("multi_instance_evidence_missing:$id"); continue }
      if ($value -isnot [pscustomobject]) { $errors.Add("evidence_shape_invalid:$id"); continue }
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
        if ($null -eq $row -or $row -isnot [pscustomobject]) { $errors.Add("instance_row_invalid:$id"); continue }
        $rowIdProp = $row.PSObject.Properties["id"]
        $rowStatusProp = $row.PSObject.Properties["status"]
        if ($null -eq $rowIdProp -or $rowIdProp.Value -isnot [string] -or [string]::IsNullOrWhiteSpace($rowIdProp.Value)) {
          $errors.Add("instance_id_invalid:$id")
          continue
        }
        $rowId = [string]$rowIdProp.Value
        if ($seen.ContainsKey($rowId)) { $errors.Add("instance_duplicate:$id/$rowId"); continue }
        $seen[$rowId] = $true
        $observed.Add($rowId)
        if ($null -eq $rowStatusProp -or $rowStatusProp.Value -isnot [string]) {
          $errors.Add("instance_status_invalid:$id/$rowId")
          continue
        }
        $rowStatus = [string]$rowStatusProp.Value
        if (-not $ValidStatuses.Contains($rowStatus)) {
          $errors.Add("status_invalid:$id/$rowId=$rowStatus")
          continue
        }
        if ($rowStatus -ne "success") { $errors.Add("required_not_success:$id/$rowId=$rowStatus") }
      }
      foreach ($want in $instances) {
        if (-not $observed.Contains($want)) { $errors.Add("instance_missing:$id/$want") }
      }
      foreach ($got in $observed) {
        if ($instances -notcontains $got) { $errors.Add("instance_unknown:$id/$got") }
      }
      continue
    }

    if ($value -isnot [string]) { $errors.Add("evidence_shape_invalid:$id"); continue }
    if (-not $ValidStatuses.Contains([string]$value)) { $errors.Add("status_invalid:$id=$value"); continue }
    if ($value -ne "success") { $errors.Add("required_not_success:$id=$value") }
  }

  return $errors
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

function Get-WorkflowLines([string]$Text) {
  return @($Text -split "`n" | ForEach-Object { $_.TrimEnd("`r") })
}

function Get-InlineRunBlocks([string]$Text) {
  # 工作流内联 Bash 提取的唯一实现：ReleaseContract 专项与 test-ci-gate 回归均复用此函数。
  # 支持 YAML 块标量的 chomping/缩进标记（|、|-、|+、|2、|-2 等），避免合法写法被静默漏检。
  $lines = Get-WorkflowLines $Text
  $blocks = [System.Collections.Generic.List[object]]::new()
  $currentName = ""
  for ($i = 0; $i -lt $lines.Count; $i++) {
    if ($lines[$i] -match '^\s*- name:\s*(.+?)\s*$') { $currentName = $Matches[1]; continue }
    if ($lines[$i] -notmatch '^(\s*)run:\s*\|[0-9+\-]*\s*$') { continue }
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
    $blocks.Add([pscustomobject]@{ name = $currentName; startLine = $i + 1; body = ($body -join "`n") })
    $i = $j - 1
  }
  return $blocks
}

function Get-UnsupportedRunForms([string]$Text) {
  # 非块标量的 run 形式（行内命令、折叠标量 >、未识别的 chomping 组合）
  # 无法在此处可靠提取与校验，必须显式报告而不是静默漏检。
  $lines = Get-WorkflowLines $Text
  $unsupported = [System.Collections.Generic.List[object]]::new()
  for ($i = 0; $i -lt $lines.Count; $i++) {
    if ($lines[$i] -notmatch '^(\s*)run:\s*(.*)$') { continue }
    if ($lines[$i] -match '^(\s*)run:\s*\|[0-9+\-]*\s*$') { continue }
    $unsupported.Add([pscustomobject]@{ line = $i + 1; header = $lines[$i].Trim() })
  }
  return $unsupported
}

function Invoke-ExtractRunsMode([string]$PathValue) {
  $resolved = Resolve-RepoFile $PathValue ".github/workflows/release.yml"
  $text = Read-WorkflowText $resolved "工作流"
  $blocks = Get-InlineRunBlocks $text
  return ($blocks | ConvertTo-Json -Depth 5 -AsArray)
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

  # 结构化输出的数组序列化形式。管道形式下 ConvertTo-Json 对零元素输出空串、
  # 对单元素输出 JSON 字符串，下游无法区分“空”与“一个值”，必须用 -InputObject 包住数组。
  foreach ($outputName in @("required_platforms", "required_jobs", "test_profiles")) {
    $arrayPattern = $outputName + '=\$\(ConvertTo-Json -InputObject @\(\$scope\.[A-Za-z]+\) -Compress\)'
    if ($detectText -notmatch $arrayPattern) {
      $errors.Add("detect_output_json_not_array:$outputName")
    }
  }

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
  if ($text -notmatch 'sha256sum[^\n]*\s-c\s') { $errors.Add("release_checksum_verify_missing") }
  # 必须使用 --strict：非严格模式下格式非法的清单行只会告警，不会让 sha256sum -c 失败。
  if ($text -notmatch 'sha256sum\s+(?:--strict\s+-c|-c\s+--strict)') { $errors.Add("release_checksum_strict_missing") }
  if ($text -notmatch 'assert_release_assets') { $errors.Add("release_asset_assertion_missing") }
  if ($text -notmatch 'uniq -d') { $errors.Add("release_duplicate_member_check_missing") }
  if ($text -notmatch '\-mindepth 2') { $errors.Add("release_nested_path_check_missing") }
  if ($text -notmatch 'gh release download') { $errors.Add("release_download_missing") }
  if ($text -notmatch '--pattern') { $errors.Add("release_download_pattern_missing") }
  if ($text -notmatch 'sort -V') { $errors.Add("release_monotonic_latest_missing") }
  if ($text -notmatch '--latest=') { $errors.Add("release_latest_flag_missing") }

  # 发布专项自身必须校验实际工作流的内联 Bash 语法（全部分块），
  # 不得依赖 test-ci-gate.ps1 的完整回归分支，也不得在 Bash 缺失时静默通过。
  foreach ($form in @(Get-UnsupportedRunForms $text)) {
    $errors.Add("release_run_form_unsupported:$($form.line):$($form.header)")
  }
  $bashPath = Find-BashExecutable
  if ($null -eq $bashPath) {
    $errors.Add("release_bash_unavailable")
    return $errors
  }
  $runBlocks = @(Get-InlineRunBlocks $text)
  if ($runBlocks.Count -eq 0) { $errors.Add("release_bash_blocks_missing"); return $errors }
  $blockIndex = 0
  foreach ($block in $runBlocks) {
    $blockIndex++
    $scriptPath = Join-Path ([IO.Path]::GetTempPath()) ("soxu-release-bash-" + [Guid]::NewGuid().ToString("N") + ".sh")
    try {
      Set-Content -LiteralPath $scriptPath -Value $block.body -Encoding utf8 -NoNewline
      $syntaxOutput = (& $bashPath --noprofile --norc -n $scriptPath 2>&1 | Out-String)
      if ($LASTEXITCODE -ne 0) {
        $firstLine = @($syntaxOutput -split "`n" | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })[0]
        $errors.Add("release_bash_syntax_invalid:${blockIndex}:$firstLine")
      }
    } finally {
      Remove-Item -LiteralPath $scriptPath -Force -ErrorAction SilentlyContinue
    }
  }

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
    "ExtractRuns" {
      $blocksJson = Invoke-ExtractRunsMode $WorkflowPath
      Write-Output $blocksJson
      exit 0
    }
    default {
      throw "未知模式：$Mode"
    }
  }
} catch {
  Write-Host "::error::CI-GATE-ERROR $($_.Exception.Message)" -ForegroundColor Red
  exit 2
}
