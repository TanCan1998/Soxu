param(
  [ValidateSet("Plan", "Verify", "Evidence", "Contract", "ReleaseContract", "ExtractRuns", "Summary")]
  [string]$Mode = "Verify",
  [string]$ScopePath = "",
  [string]$ScopeJson = "",
  [string]$PlanPath = "",
  [string]$PlanJson = "",
  [string]$EvidencePath = "",
  [string]$EvidenceJson = "",
  # Evidence 模式：已下载的 artifact 目录、原始 run jobs、事件身份与 attempt 起点。
  [string]$EvidenceDir = "",
  [string]$JobsPath = "",
  [string]$JobsJson = "",
  [string]$RunId = "",
  [string]$RunAttempt = "",
  [string]$CheckoutSha = "",
  [string]$AttemptStartedAt = "",
  [string]$WorkflowPath = "",
  [string]$ReleaseWorkflowPath = "",
  # Summary 模式：只做脱敏的耗时/缓存/跳过汇总，不参与门禁判定。
  [string]$StagesPath = "",
  [string]$SummaryPath = "",
  [string]$PlatformId = "",
  [string]$CacheStatus = "",
  [string]$SkipsJson = "",
  [string]$SummaryTitle = ""
)

$ErrorActionPreference = "Stop"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$OutputEncoding = [System.Text.Encoding]::UTF8

# 本脚本是 CI Gate 的唯一判定入口，四种模式职责互不重叠：
#   Plan            依检测器输出构建带版本号的确定性门禁计划（工作流中的计划真相源）
#   Verify          以“固定 job 契约 + scope 重新推导”对账计划与 needs 实际证据
#   Contract        静态校验 ci.yml 的门禁契约（稳定名称、静态 needs、无 continue-on-error）
#   ReleaseContract 静态校验 release.yml 的四平台资产清单与校验和契约
#   Summary         把单平台阶段耗时/缓存状态/计划内跳过写成脱敏 Step Summary（不参与判定）
#
# 设计红线：计划是数据，不能凭删 job、换 job、翻转 required 或篡改触发标志削弱门禁。
# Verify 会用与 Plan 完全相同的推导函数，从计划自带的 scope 重新算出期望计划再逐项比对。
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path

$PlanVersion = "ci-gate-plan/v1"
$DetectorJobId = "detect-changes"
$GateJobId = "ci-gate"
$ValidStatuses = @("success", "failure", "cancelled", "skipped")
$ValidRustScopes = @("workspace", "crates", "none")

# 平台目录：与 detect-check-scope.ps1 的 $PlatformCatalog 必须逐项一致。
# 计划只携带平台 id，runner 标签与 CPU 架构由本表派生，因此计划不能自造 runner。
# 两个副本的一致性由 test-ci-gate.ps1 的目录对齐用例防守。
$PlatformCatalog = [ordered]@{
  "windows"     = [ordered]@{ id = "windows"; os = "windows"; arch = "x64"; runner = "windows-2025" }
  "linux"       = [ordered]@{ id = "linux"; os = "linux"; arch = "x64"; runner = "ubuntu-24.04" }
  "macos-arm64" = [ordered]@{ id = "macos-arm64"; os = "macos"; arch = "arm64"; runner = "macos-15" }
  "macos-intel" = [ordered]@{ id = "macos-intel"; os = "macos"; arch = "x64"; runner = "macos-15-intel" }
}
$PlatformOrder = @($PlatformCatalog.Keys)
$BaselinePlatform = "windows"
# 平台证据只接受这组终态：部分失败、取消、未知、未跑都不得充当“已通过”。
$PassingEvidenceStatuses = @("passed")
$EvidencePlatformKind = "soxu-ci-evidence/platform/v1"
$EvidenceJobKind = "soxu-ci-evidence/job/v1"
# 矩阵作业的 GitHub 作业显示名模版：逐实例证据只能从 /attempts/<n>/jobs 派生，
# needs.<job>.result 只是聚合字符串，不包含实例身份，不得当作逐实例证据使用。
$MatrixJobDisplayNames = [ordered]@{
  "rust-check" = "Rust ({0})"
}
# 平台证据的必要执行阶段：依赖拉取、Clippy 与测试必须各自真实执行并全绿。
# 缺段、重复段、失败段和未归零退出码一律拒绝，不能用“身份对就算通过”充数。
$RequiredPlatformStages = @("cargo-fetch", "cargo-clippy", "cargo-test")
$PassingStageStatuses = @("passed")
$StageCountKeys = @("passed", "failed", "ignored", "measured", "filtered")

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

# 当前固定 job 契约：governance-and-privacy 无条件必跑，其余由检测器同源输出决定。
# CI-C 新增独立的 rust-format 作业（Ubuntu 上只跑一次），并把 rust-check 改为平台矩阵；
# 矩阵只扩展实例证据，不改变这里的 job 集合身份。
$GatedJobSpecs = @(
  [pscustomobject]@{ Id = "governance-and-privacy"; Output = ""; Unconditional = $true; Instanced = $false }
  [pscustomobject]@{ Id = "rust-format"; Output = "rust"; Unconditional = $false; Instanced = $false }
  [pscustomobject]@{ Id = "rust-check"; Output = "rust"; Unconditional = $false; Instanced = $true }
  [pscustomobject]@{ Id = "mobile-web-check"; Output = "mobile_web"; Unconditional = $false; Instanced = $false }
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
  $raw = Read-RawPayload $PathValue $InlineValue $Label
  try {
    return ($raw | ConvertFrom-Json -Depth 20 -ErrorAction Stop)
  } catch {
    throw "$Label 不是合法 JSON：$($_.Exception.Message)"
  }
}

function Read-RawPayload([string]$PathValue, [string]$InlineValue, [string]$Label) {
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
  return $raw.Trim()
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

function Assert-ScopePlatforms($Scope, [bool]$Rust) {
  # platforms 是 CI-C 的必需字段：Rust 矩阵实例集必须随 scope 声明，
  # 否则 Verify 无法以同一推导函数重算“哪些平台份证据是必须的”。
  $prop = $Scope.PSObject.Properties["platforms"]
  if ($null -eq $prop) { throw "变更范围 JSON 缺少字段 platforms（CI-C 要求显式声明 Rust 矩阵实例）。" }
  if ($null -eq $prop.Value) { throw "变更范围字段 platforms 不能为 null。" }
  if ($prop.Value -isnot [System.Array]) { throw "变更范围字段 platforms 必须是数组。" }

  $seen = @{}
  foreach ($item in @($prop.Value)) {
    if ($item -isnot [string] -or [string]::IsNullOrWhiteSpace($item)) {
      throw "变更范围字段 platforms 含非字符串或空值。"
    }
    if (-not $PlatformCatalog.Contains([string]$item)) {
      throw "未知平台 id：$item（允许 $($PlatformOrder -join '/')）。"
    }
    if ($seen.ContainsKey([string]$item)) { throw "变更范围字段 platforms 含重复平台：$item。" }
    $seen[[string]$item] = $true
  }

  if (-not $Rust) {
    if ($seen.Count -gt 0) {
      throw "rust=false 时 platforms 必须为空（PWA-only 的 linux 不得开开 Rust 矩阵），实际：$($seen.Keys -join ',')。"
    }
    return @()
  }
  if ($seen.Count -eq 0) { throw "rust=true 时 platforms 不能为空，否则 Rust 矩阵无可核对实例。" }
  if (-not $seen.ContainsKey($BaselinePlatform)) {
    throw "rust=true 时 platforms 必须包含基线平台 $BaselinePlatform。"
  }

  # 按平台目录键序重排，使矩阵实例顺序与证据顺序都是确定的。
  $ordered = [System.Collections.Generic.List[string]]::new()
  foreach ($platformId in $PlatformOrder) {
    if ($seen.ContainsKey($platformId)) { $ordered.Add($platformId) }
  }
  return @($ordered)
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
  $platforms = Assert-ScopePlatforms $Scope $rust

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
    # 只有平台矩阵作业带 instances，且仅在它真的必跑时声明。
    if ($spec.Instanced -and $required) { $job["instances"] = @($platforms) }
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
    # 随计划回写平台集，使 Verify 能用同一推导函数重算矩阵实例。
    platforms          = @($platforms)
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

    # 矩阵实例集必须与 scope 重算结果完全一致：删平台、加平台、改顺序、伪造实例都拒绝。
    $expectedInstances = @()
    if ($null -ne $expectedJob) {
      $instProp = $expectedJob.PSObject.Properties["instances"]
      if ($null -ne $instProp -and $null -ne $instProp.Value) { $expectedInstances = @($instProp.Value) }
    }
    if ($expectedInstances.Count -gt 0) {
      if ($entry.Instances.Count -eq 0) {
        $Errors.Add("plan_instances_missing:$($spec.Id)")
      } elseif (($entry.Instances -join ',') -ne ($expectedInstances -join ',')) {
        $Errors.Add("plan_instances_conflict:$($spec.Id)")
      }
    } elseif ($entry.Instances.Count -gt 0) {
      $Errors.Add("plan_instances_conflict:$($spec.Id)")
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

# 逐实例证据派生：只从原始 jobs 列表读取，缺失、重复、未知与非法结论均拒绝。
# 返回 $null 表示无法派生，调用方必须失败关闭，不得回退到聚合字符串。
function Get-MatrixInstanceEvidence($JobsDocument, [string]$JobId, [string[]]$ExpectedInstances, $Errors) {
  if (-not $MatrixJobDisplayNames.Contains($JobId)) {
    $Errors.Add("matrix_job_display_name_unmapped:$JobId")
    return $null
  }
  $template = [string]$MatrixJobDisplayNames[$JobId]
  $rows = @()
  $jobsProp = $JobsDocument.PSObject.Properties["jobs"]
  if ($null -ne $jobsProp -and $null -ne $jobsProp.Value) { $rows = @($jobsProp.Value) }

  $observed = [ordered]@{}
  $seen = @{}
  foreach ($row in $rows) {
    $name = [string]$row.name
    $matched = $null
    foreach ($candidate in $ExpectedInstances) {
      if ($name -eq ($template -f $candidate)) { $matched = $candidate; break }
    }
    if ($null -eq $matched) { continue }
    if ($seen.ContainsKey($matched)) {
      $Errors.Add("matrix_instance_duplicate:${JobId}/${matched}")
      continue
    }
    $seen[$matched] = $true
    $conclusion = [string]$row.conclusion
    if (-not $ValidStatuses.Contains($conclusion)) {
      $Errors.Add("matrix_instance_status_invalid:${JobId}/${matched}=$conclusion")
      continue
    }
    $observed[$matched] = $conclusion
  }

  foreach ($expected in $ExpectedInstances) {
    if (-not $seen.ContainsKey($expected)) { $Errors.Add("matrix_instance_missing:${JobId}/${expected}") }
  }

  # 计划外的同名矩阵行也拒绝：否则多出的平台实例可以旕混过对账。
  $expectedNames = @($ExpectedInstances | ForEach-Object { $template -f $_ })
  foreach ($row in $rows) {
    $name = [string]$row.name
    if ($expectedNames -contains $name) { continue }
    $prefix = ($template -f "@")
    $splitAt = $prefix.IndexOf("@")
    $head = $prefix.Substring(0, $splitAt)
    $tail = $prefix.Substring($splitAt + 1)
    if ($name.StartsWith($head, [StringComparison]::Ordinal) -and $name.EndsWith($tail, [StringComparison]::Ordinal)) {
      $Errors.Add("matrix_instance_unknown:${JobId}/$name")
    }
  }

  $instances = @()
  foreach ($key in $observed.Keys) { $instances += [pscustomobject]@{ id = $key; status = [string]$observed[$key] } }
  return [pscustomobject]@{ instances = @($instances) }
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
      # 逐实例证据优先来自原始 jobs 列表。needs 聚合字符串不包含实例身份，
      # 它只能与其他来源交叉校验，绝不能自己充当逐实例证据。
      $hasJobs = -not [string]::IsNullOrWhiteSpace($JobsPath) -or -not [string]::IsNullOrWhiteSpace($JobsJson)
      if (-not $hasJobs) {
        if ($value -is [string]) {
          # 聚合字符串 + 无原始 jobs：无法证明逐平台结果，必须失败关闭。
          $errors.Add("matrix_jobs_unavailable:$id")
          continue
        }
        # 证据自带 instances（仅单测夹具会走这条）：仍走下方逐实例校验。
      } else {
        $jobsDocument = $null
        try {
          $jobsRaw = Read-RawPayload $JobsPath $JobsJson "run jobs JSON"
          $jobsDocument = $jobsRaw | ConvertFrom-Json -Depth 20 -ErrorAction Stop
        } catch {
          $errors.Add("matrix_jobs_unreadable:$id")
          continue
        }
        if ($null -eq $jobsDocument) { $errors.Add("matrix_jobs_unreadable:$id"); continue }
        $derived = Get-MatrixInstanceEvidence $jobsDocument $id $instances $errors
        if ($null -eq $derived) { continue }

        # 聚合状态与逐实例结论必须一致：needs 报成功但实例不全绿，同样是矛盾。
        $aggregate = if ($value -is [string]) { [string]$value } else { "" }
        $derivedAllSuccess = (@($derived.instances | Where-Object { $_.status -ne "success" }).Count -eq 0)
        if ($aggregate -eq "success" -and -not $derivedAllSuccess) {
          $errors.Add("matrix_aggregate_conflict:${id}=success")
        }
        if ($aggregate -ne "" -and $aggregate -ne "success" -and $derivedAllSuccess) {
          $errors.Add("matrix_aggregate_conflict:${id}=$aggregate")
        }

        # 证据自带的 instances 与原始 jobs 不一致时，以 jobs 为准并显式报冲突。
        if ($value -isnot [string]) {
          $evidenceRowsProp = $value.PSObject.Properties["instances"]
          if ($null -ne $evidenceRowsProp -and $null -ne $evidenceRowsProp.Value) {
            $evidenceMap = @{}
            foreach ($row in @($evidenceRowsProp.Value)) {
              if ($null -ne $row -and $null -ne $row.PSObject.Properties["id"]) {
                $evidenceMap[[string]$row.id] = [string]$row.status
              }
            }
            foreach ($row in @($derived.instances)) {
              if ($evidenceMap.ContainsKey([string]$row.id) -and $evidenceMap[[string]$row.id] -ne [string]$row.status) {
                $errors.Add("matrix_evidence_conflict:${id}/$($row.id)")
              }
            }
          }
        }

        $value = $derived
      }
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

function Get-EvidenceField($Document, [string]$Path) {
  $node = $Document
  foreach ($segment in $Path.Split('.')) {
    if ($null -eq $node) { return $null }
    $prop = $node.PSObject.Properties[$segment]
    if ($null -eq $prop) { return $null }
    $node = $prop.Value
  }
  return $node
}

function ConvertTo-UtcTimestamp([string]$Value) {
  if ([string]::IsNullOrWhiteSpace($Value)) { return $null }
  $parsed = [datetimeoffset]::MinValue
  $style = [System.Globalization.DateTimeStyles]::AssumeUniversal -bor [System.Globalization.DateTimeStyles]::AdjustToUniversal
  if ([datetimeoffset]::TryParse($Value, [System.Globalization.CultureInfo]::InvariantCulture, $style, [ref]$parsed)) {
    return $parsed.ToUniversalTime()
  }
  return $null
}

function Get-Sha256Hex([string]$Text) {
  $bytes = [System.Text.Encoding]::UTF8.GetBytes($Text)
  $sha = [System.Security.Cryptography.SHA256]::Create()
  try {
    return ([System.BitConverter]::ToString($sha.ComputeHash($bytes)) -replace '-', '').ToLowerInvariant()
  } finally {
    $sha.Dispose()
  }
}

# CI-C 平台阶段完整性：必要阶段必须有真实执行记录，且阶段状态、退出码、计数表态均真实。
# 计数缺失只能显式记为 countsKnown=false + counts=null，不得用 0 冒充“已统计”结果。
function Test-PlatformStages($Document, [string]$PlatformId, $Errors) {
  $stagesProp = $Document.PSObject.Properties["stages"]
  if ($null -eq $stagesProp) { $Errors.Add("evidence_stages_missing:$PlatformId"); return }
  if ($null -eq $stagesProp.Value) { $Errors.Add("evidence_stages_null:$PlatformId"); return }
  if ($stagesProp.Value -isnot [System.Array]) { $Errors.Add("evidence_stages_not_array:$PlatformId"); return }

  $rows = @($stagesProp.Value)
  if ($rows.Count -eq 0) { $Errors.Add("evidence_stages_empty:$PlatformId"); return }

  $seen = @{}
  $seenOrder = [System.Collections.Generic.List[string]]::new()
  foreach ($row in $rows) {
    if ($null -eq $row -or $row -isnot [pscustomobject]) { $Errors.Add("evidence_stage_invalid:$PlatformId"); continue }
    $nameProp = $row.PSObject.Properties["name"]
    if ($null -eq $nameProp -or $nameProp.Value -isnot [string] -or [string]::IsNullOrWhiteSpace([string]$nameProp.Value)) {
      $Errors.Add("evidence_stage_name_invalid:$PlatformId")
      continue
    }
    $name = [string]$nameProp.Value
    if ($seen.ContainsKey($name)) { $Errors.Add("evidence_stage_duplicate:${PlatformId}/${name}"); continue }
    $seen[$name] = $row
    $seenOrder.Add($name)
  }

  foreach ($required in $RequiredPlatformStages) {
    if (-not $seen.ContainsKey($required)) { $Errors.Add("evidence_stage_missing:${PlatformId}/${required}") }
  }
  foreach ($name in $seenOrder) {
    if ($RequiredPlatformStages -notcontains $name) { $Errors.Add("evidence_stage_unexpected:${PlatformId}/${name}") }
  }

  foreach ($name in $seenOrder) {
    $row = $seen[$name]
    $status = [string](Get-EvidenceField $row 'status')
    if ($PassingStageStatuses -notcontains $status) {
      $Errors.Add("evidence_stage_not_passed:${PlatformId}/${name}=$status")
    }
    $exitProp = $row.PSObject.Properties["exitCode"]
    if ($null -eq $exitProp -or ($exitProp.Value -isnot [int] -and $exitProp.Value -isnot [long])) {
      $Errors.Add("evidence_stage_exit_code_invalid:${PlatformId}/${name}")
    } elseif ([int]$exitProp.Value -ne 0) {
      $Errors.Add("evidence_stage_exit_code:${PlatformId}/${name}=$($exitProp.Value)")
    }

    $knownProp = $row.PSObject.Properties["countsKnown"]
    if ($null -eq $knownProp -or $knownProp.Value -isnot [bool]) {
      $Errors.Add("evidence_stage_counts_flag_invalid:${PlatformId}/${name}")
      continue
    }
    $countsProp = $row.PSObject.Properties["counts"]
    if (-not [bool]$knownProp.Value) {
      # 未知计数是合法表态，但必须与 counts 都为 null 一致，不能用 0 冒充。
      if ($null -ne $countsProp -and $null -ne $countsProp.Value) {
        $Errors.Add("evidence_stage_counts_conflict:${PlatformId}/${name}")
      }
      continue
    }
    if ($null -eq $countsProp -or $null -eq $countsProp.Value -or $countsProp.Value -isnot [pscustomobject]) {
      $Errors.Add("evidence_stage_counts_missing:${PlatformId}/${name}")
      continue
    }
    foreach ($key in $StageCountKeys) {
      $keyProp = $countsProp.Value.PSObject.Properties[$key]
      $isInt = $null -ne $keyProp -and ($keyProp.Value -is [int] -or $keyProp.Value -is [long])
      if (-not $isInt -or [int]$keyProp.Value -lt 0) {
        $Errors.Add("evidence_stage_counts_invalid:${PlatformId}/${name}")
        break
      }
    }

    # 阶段声称 passed 但计数里 failed>0 是矛盾证据，一律拒绝。
    # 计数未知（countsKnown=false）仍是合法表态，不在此列。
    $failedProp = $countsProp.Value.PSObject.Properties["failed"]
    if ($status -eq "passed" -and $null -ne $failedProp -and [int]$failedProp.Value -gt 0) {
      $Errors.Add("evidence_stage_counts_contradiction:${PlatformId}/${name}")
    }
  }
}

# CI-C 证据对账：同一 Run、Attempt、SHA 与计划下，预期平台必须各有且仅有一份已通过证据。
# 拒绝缺项、重复、未知平台、差异 SHA、部分失败、取消，以及沿用前一次 attempt 的复用证据。
function Invoke-EvidenceMode {
  $errors = [System.Collections.Generic.List[string]]::new()

  $planRaw = Read-RawPayload $PlanPath $PlanJson "门禁计划 JSON"
  $plan = $null
  try { $plan = $planRaw | ConvertFrom-Json -Depth 20 -ErrorAction Stop } catch { $errors.Add("plan_invalid_json"); return $errors }
  if ($null -eq $plan) { $errors.Add("plan_missing"); return $errors }

  Test-PlanContract $plan $errors
  if ($errors.Count -gt 0) { return $errors }

  if ([string]::IsNullOrWhiteSpace($EvidenceDir)) { $errors.Add("evidence_dir_missing"); return $errors }
  if (-not (Test-Path -LiteralPath $EvidenceDir -PathType Container)) { $errors.Add("evidence_dir_unreadable"); return $errors }

  $rustJob = @($plan.jobs | Where-Object { $_.id -eq "rust-check" })[0]
  $expectedPlatforms = @()
  if ($null -ne $rustJob) {
    $instProp = $rustJob.PSObject.Properties["instances"]
    if ($null -ne $instProp -and $null -ne $instProp.Value) { $expectedPlatforms = @($instProp.Value) }
  }
  $planDigest = Get-Sha256Hex $planRaw

  if ([string]::IsNullOrWhiteSpace($RunId)) { $errors.Add("run_identity_missing") }
  if ([string]::IsNullOrWhiteSpace($RunAttempt)) { $errors.Add("attempt_identity_missing") }
  if ([string]::IsNullOrWhiteSpace($CheckoutSha)) { $errors.Add("checkout_sha_missing") }

  $attemptAt = ConvertTo-UtcTimestamp $AttemptStartedAt
  if ([string]::IsNullOrWhiteSpace($AttemptStartedAt)) {
    # 缺 attempt 起点就无法区分“本次新执行”与“沿用前一次 attempt”，必须失败关闭。
    $errors.Add("attempt_started_at_missing")
  } elseif ($null -eq $attemptAt) {
    $errors.Add("attempt_started_at_invalid")
  }

  $observed = @{}
  $duplicated = @{}
  $files = @(Get-ChildItem -LiteralPath $EvidenceDir -Recurse -File -Filter '*.json' | Sort-Object FullName)
  if ($files.Count -eq 0) {
    # 本次 attempt 一个证据都没有：仅在真的需要平台证据时才是错误。
    if ($expectedPlatforms.Count -gt 0) { $errors.Add("evidence_dir_empty") }
  }

  foreach ($file in $files) {
    $label = $file.Name
    $raw = Get-Content -LiteralPath $file.FullName -Raw -Encoding utf8
    if ([string]::IsNullOrWhiteSpace($raw)) { $errors.Add("evidence_unreadable:$label"); continue }
    try { $doc = $raw | ConvertFrom-Json -Depth 20 -ErrorAction Stop } catch { $errors.Add("evidence_unreadable:$label"); continue }

    $schemaVersion = Get-EvidenceField $doc 'schemaVersion'
    if ($null -eq $schemaVersion -or $schemaVersion -isnot [int] -and $schemaVersion -isnot [long]) {
      $errors.Add("evidence_schema_invalid:$label")
      continue
    }
    if ([int]$schemaVersion -ne 1) { $errors.Add("evidence_schema_invalid:$label"); continue }

    $kind = [string](Get-EvidenceField $doc 'kind')
    if ($kind -ne $EvidencePlatformKind -and $kind -ne $EvidenceJobKind) {
      $errors.Add("evidence_kind_invalid:$label")
      continue
    }

    # 事件身份绑定：Run、Attempt、SHA 与计划四项均必须与本次招待一致。
    if ([string](Get-EvidenceField $doc 'run.id') -ne $RunId) { $errors.Add("evidence_run_mismatch:$label") }
    if ([string](Get-EvidenceField $doc 'run.attempt') -ne $RunAttempt) { $errors.Add("evidence_attempt_mismatch:$label") }
    if ([string](Get-EvidenceField $doc 'identity.checkoutSha') -ne $CheckoutSha) { $errors.Add("evidence_sha_mismatch:$label") }
    if ([string](Get-EvidenceField $doc 'plan.digest') -ne $planDigest) { $errors.Add("evidence_plan_mismatch:$label") }

    # 部分重跑：gh run rerun --failed 会沿用前一次 attempt 的成功 job 与 artifact，
    # 那些结果不属于本次 attempt。时间戳早于本次 attempt 起点即失败关闭，不默默放宽绑定。
    if ($null -ne $attemptAt) {
      $completedRaw = [string](Get-EvidenceField $doc 'timing.completedAt')
      if ([string]::IsNullOrWhiteSpace($completedRaw)) {
        $errors.Add("evidence_timing_missing:$label")
      } else {
        $completedAt = ConvertTo-UtcTimestamp $completedRaw
        if ($null -eq $completedAt) {
          $errors.Add("evidence_timing_invalid:$label")
        } elseif ($completedAt -lt $attemptAt) {
          $errors.Add("evidence_from_previous_attempt:$label")
        }
      }
    }

    if ($kind -ne $EvidencePlatformKind) { continue }

    $platformId = [string](Get-EvidenceField $doc 'platform')
    if ([string]::IsNullOrWhiteSpace($platformId)) { $errors.Add("evidence_platform_missing_field:$label"); continue }
    if (-not $PlatformCatalog.Contains($platformId)) { $errors.Add("evidence_platform_unknown:$platformId"); continue }
    if ($expectedPlatforms -notcontains $platformId) { $errors.Add("evidence_platform_unexpected:$platformId"); continue }
    if ($observed.ContainsKey($platformId)) {
      $duplicated[$platformId] = $true
      $errors.Add("evidence_platform_duplicate:$platformId")
      continue
    }
    $observed[$platformId] = $doc

    # Runner 与架构只认平台目录：证据不得用 Ubuntu 冒充 macOS 或把交叉编译当原生。
    $expectedRunner = [string]$PlatformCatalog[$platformId].runner
    $expectedOs = [string]$PlatformCatalog[$platformId].os
    $expectedArch = [string]$PlatformCatalog[$platformId].arch
    if ([string](Get-EvidenceField $doc 'runner.label') -ne $expectedRunner) { $errors.Add("evidence_runner_mismatch:$platformId") }
    if ([string](Get-EvidenceField $doc 'runner.os') -ne $expectedOs) { $errors.Add("evidence_runner_os_mismatch:$platformId") }
    if ([string](Get-EvidenceField $doc 'runner.arch') -ne $expectedArch) { $errors.Add("evidence_runner_arch_mismatch:$platformId") }

    $resultStatus = [string](Get-EvidenceField $doc 'result.status')
    if ($PassingEvidenceStatuses -notcontains $resultStatus) {
      $errors.Add("evidence_not_passed:$platformId=$resultStatus")
    }

    # 只有身份对还不够：必须真有 fetch/clippy/test 三个通过阶段。
    Test-PlatformStages $doc $platformId $errors
  }

  if ($expectedPlatforms.Count -gt 0) {
    foreach ($platformId in $expectedPlatforms) {
      if (-not $observed.ContainsKey($platformId) -and -not $duplicated.ContainsKey($platformId)) {
        $errors.Add("evidence_platform_missing:$platformId")
      }
    }
  } elseif ($observed.Count -gt 0) {
    $errors.Add("evidence_platform_unexpected:$((@($observed.Keys)) -join ',')")
  }

  # 原始矩阵 job 汇总状态必须与实际证据一致：不接受“job 绿但无证据”或反向失配。
  if (-not [string]::IsNullOrWhiteSpace($JobsPath) -or -not [string]::IsNullOrWhiteSpace($JobsJson)) {
    $jobsRaw = Read-RawPayload $JobsPath $JobsJson "run jobs JSON"
    $jobsDoc = $null
    try { $jobsDoc = $jobsRaw | ConvertFrom-Json -Depth 20 -ErrorAction Stop } catch { $errors.Add("matrix_jobs_invalid_json"); $jobsDoc = $null }
    if ($null -ne $jobsDoc) {
      $rows = @()
      $jobsProp = $jobsDoc.PSObject.Properties["jobs"]
      if ($null -ne $jobsProp -and $null -ne $jobsProp.Value) { $rows = @($jobsProp.Value) }
      if ($rows.Count -eq 0 -and $expectedPlatforms.Count -gt 0) { $errors.Add("matrix_jobs_empty") }

      $matrixRows = @($rows | Where-Object { [string]$_.name -match '^Rust \((.+)\)$' })
      $seenRowPlatforms = @{}
      foreach ($row in $matrixRows) {
        $null = ([string]$row.name) -match '^Rust \((.+)\)$'
        $rowPlatform = $Matches[1]
        # GitHub Jobs API 在矩阵 job 被计划跳过且从未生成实例时，会把 job name 保留为
        # 未展开的 `${{ matrix.platform.id }}`。此时没有任何可归属的平台实例；当计划
        # 本来就不要求 Rust 矩阵时跳过该占位符，否则仍按未知名称失败闭合。
        if ($rowPlatform -eq '${{ matrix.platform.id }}' -and $expectedPlatforms.Count -eq 0) { continue }
        if (-not $PlatformCatalog.Contains($rowPlatform)) { $errors.Add("matrix_job_unknown:$rowPlatform"); continue }
        if ($expectedPlatforms -notcontains $rowPlatform) { $errors.Add("matrix_job_unplanned:$rowPlatform"); continue }
        if ($seenRowPlatforms.ContainsKey($rowPlatform)) { $errors.Add("matrix_job_duplicate:$rowPlatform"); continue }
        $seenRowPlatforms[$rowPlatform] = $true
        $conclusion = [string]$row.conclusion
        if ($conclusion -ne "success") { $errors.Add("matrix_job_not_success:$rowPlatform=$conclusion") }
        # jobs 列表会把沿用（复用）的成功 job 也标上当前 attempt，所以只能看原始时间：
        # 开始时间早于本次 attempt 起点即证明它是上一 attempt 的执行结果。
        if ($null -ne $attemptAt) {
          $rowStartedRaw = [string](Get-EvidenceField $row 'started_at')
          if ([string]::IsNullOrWhiteSpace($rowStartedRaw)) {
            $errors.Add("matrix_job_timing_missing:$rowPlatform")
          } else {
            $rowStarted = ConvertTo-UtcTimestamp $rowStartedRaw
            if ($null -eq $rowStarted) {
              $errors.Add("matrix_job_timing_invalid:$rowPlatform")
            } elseif ($rowStarted -lt $attemptAt) {
              $errors.Add("matrix_job_from_previous_attempt:$rowPlatform")
            }
          }
        }
        if ($observed.ContainsKey($rowPlatform)) {
          $evidenceStatus = [string](Get-EvidenceField $observed[$rowPlatform] 'result.status')
          if ($conclusion -eq "success" -and $evidenceStatus -ne "passed") {
            $errors.Add("matrix_job_evidence_conflict:$rowPlatform")
          }
          if ($conclusion -ne "success" -and $evidenceStatus -eq "passed") {
            $errors.Add("matrix_job_evidence_conflict:$rowPlatform")
          }
        }
      }
      foreach ($platformId in $expectedPlatforms) {
        if (-not $seenRowPlatforms.ContainsKey($platformId)) { $errors.Add("matrix_job_missing:$platformId") }
      }
    }
  }

  # 合法的部分重跑策略必须显式说明：不拓宽绑定，而是要求全量重跑。
  # 既包括“证据时间戳早于本次 attempt”的复用，也包括“按名根本没有本次证据”。
  $reuseMarkers = @(
    @($errors | Where-Object { $_ -like 'evidence_from_previous_attempt:*' }).Count -gt 0
    @($errors | Where-Object { $_ -like 'matrix_job_from_previous_attempt:*' }).Count -gt 0
    @($errors | Where-Object { $_ -like 'evidence_platform_missing:*' }).Count -gt 0
    @($errors | Where-Object { $_ -eq 'evidence_dir_empty' }).Count -gt 0
  )
  if (@($reuseMarkers | Where-Object { $_ }).Count -gt 0) {
    Write-Host "::notice::CI-C-EVIDENCE-GUIDANCE 本次 attempt 缺少必要平台证据，或证据来自上一 attempt 的复用 job。请使用 gh run rerun <run-id> 全量重跑（不要用 --failed），让所有平台实例在本 attempt 重新执行。"
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

  foreach ($jobId in @($GateJobId, $DetectorJobId, "governance-and-privacy", "rust-format", "rust-check", "mobile-web-check")) {
    if (-not $jobs.Contains($jobId)) { $errors.Add("workflow_job_missing:$jobId") }
  }
  if ($errors.Count -gt 0) { return $errors }

  # 门禁作业：稳定名称、静态 needs、always() 汇总、调用本脚本校验。
  $gateText = ($jobs[$GateJobId] -join "`n")
  if ($gateText -notmatch '(?m)^    name: CI Gate\s*$') { $errors.Add("gate_name_unstable") }
  if ($gateText -notmatch '(?m)^    if: always\(\)\s*$') { $errors.Add("gate_not_always") }
  if ($gateText -notmatch 'scripts/verify-ci-gate\.ps1') { $errors.Add("gate_verifier_missing") }
  if ($gateText -notmatch '(?m)-Mode Verify|Mode\s*=\s*\x27Verify\x27') { $errors.Add("gate_verify_mode_missing") }

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
  foreach ($outputName in @("required_platforms", "required_jobs", "test_profiles", "platform_matrix")) {
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

  # CI-C：Rust 矩阵契约。每个平台只生成一个实例，依赖/Clippy/测试共用一个构建目录，
  # 且矩阵只消费检测器派生的 platform_matrix，不得在工作流内硬编码平台清单。
  if ($rustText -notmatch '(?m)^    strategy:\s*$') { $errors.Add("rust_matrix_strategy_missing") }
  if ($rustText -notmatch '(?m)^\s+matrix:\s*$') { $errors.Add("rust_matrix_missing") }
  if ($rustText -notmatch 'platform:\s*\$\{\{\s*fromJSON\(needs\.detect-changes\.outputs\.platform_matrix\)\s*\}\}') {
    $errors.Add("rust_matrix_not_from_plan")
  }
  if ($rustText -notmatch '(?m)^    name: Rust \(\$\{\{ matrix\.platform\.id \}\}\)\s*$') { $errors.Add("rust_matrix_name_unstable") }
  if ($rustText -notmatch '(?m)^    runs-on: \$\{\{ matrix\.platform\.runner \}\}\s*$') { $errors.Add("rust_matrix_runner_not_from_plan") }
  if ($rustText -notmatch 'ci-evidence-\$\{\{ github\.run_attempt \}\}-rust-\$\{\{ matrix\.platform\.id \}\}') { $errors.Add("rust_matrix_evidence_artifact_missing") }
  # Runner 的 OS 与 CPU 架构必须取自实际观测（RUNNER_OS/RUNNER_ARCH），
  # 不得把矩阵期待值原样写成观测，否则证据只能证明计划写了什么。
  if ($rustText -notmatch '\$\{\{ runner\.os \}\}') { $errors.Add("rust_matrix_runner_os_not_observed") }
  if ($rustText -notmatch '\$\{\{ runner\.arch \}\}') { $errors.Add("rust_matrix_runner_arch_not_observed") }
  if ($rustText -match 'matrix\.platform\.os|matrix\.platform\.arch') { $errors.Add("rust_matrix_runner_observation_forged") }
  # 必要阶段必须在矩阵作业里真实执行，证据才有东西可对。
  foreach ($stageName in @('cargo-fetch', 'cargo-clippy', 'cargo-test')) {
    if ($rustText -notmatch [regex]::Escape("-Name $stageName")) { $errors.Add("rust_matrix_stage_missing:$stageName") }
  }
  if ($rustText -notmatch 'retention-days:\s*14') { $errors.Add("rust_matrix_evidence_retention_missing") }
  # 格式检查必须只跑一次：矩阵作业里出现 cargo fmt 就是按平台重复执行。
  if ($rustText -match 'cargo fmt') { $errors.Add("rust_matrix_duplicates_format_check") }

  # CI-C：纯 Rust 格式检查独立在 Ubuntu 上执行一次，本身不得再开矩阵。
  $formatText = ($jobs["rust-format"] -join "`n")
  if ($formatText -notmatch '(?m)^    name: Rust Format Check\s*$') { $errors.Add("rust_format_name_unstable") }
  if ($formatText -notmatch '(?m)^    runs-on: ubuntu-24\.04\s*$') { $errors.Add("rust_format_runner_not_ubuntu") }
  if ($formatText -notmatch 'cargo fmt --all -- --check') { $errors.Add("rust_format_command_missing") }
  if ($formatText -notmatch "needs\.detect-changes\.outputs\.rust == 'true'") { $errors.Add("rust_format_job_ungated") }
  if ($formatText -match '(?m)^\s+matrix:') { $errors.Add("rust_format_matrix_forbidden") }

  # ---- CI-D：工具链版本唯一真相源 / 缓存键边界 / 并发分组 ----
  # 硬编码版本是 mise.toml 与 CI 静默漂移的根因，直接拒绝而不是“建议”。
  if ($text -match '(?m)^\s+toolchain:\s*\d') { $errors.Add("toolchain_version_hardcoded") }
  if ($text -match "(?m)^\s+node-version:\s*'?\d") { $errors.Add("node_version_hardcoded") }
  if ($text -match '(?m)^\s+version:\s*\d+\s*$') { $errors.Add("pnpm_version_hardcoded") }
  if ($text -notmatch 'scripts/read-mise-versions\.ps1') { $errors.Add("toolchain_source_script_missing") }
  # 版本必须在检测器作业里读一次并写成 GHA 输出；作业内其它地方只能消费输出，不能重读。
  if ($detectText -notmatch 'read-mise-versions\.ps1') { $errors.Add("toolchain_source_script_missing") }
  if ($detectText -notmatch 'read-mise-versions\.ps1[^\n]*-GitHubOutput') { $errors.Add("toolchain_source_not_published") }
  if ($detectText -notmatch '(?m)^\s+id:\s*toolchain\s*$') { $errors.Add("toolchain_step_id_missing") }
  # 版本必须经 detect-changes 的步骤输出传递：跨 job 只能靠 outputs，不能重读文件或写常量。
  foreach ($toolOutput in @("rust_version", "node_version", "pnpm_version", "mise_toml_sha256")) {
    if ($text -notmatch ("outputs\." + $toolOutput)) { $errors.Add("toolchain_output_not_published:$toolOutput") }
  }
  # 格式检查与矩阵作业各接一次 rust 版本（共 ≥2），少一处就是漏接。
  $rustVersionConsumer = 'toolchain:\s*\$\{\{\s*needs\.detect-changes\.outputs\.rust_version\s*\}\}'
  if (@([regex]::Matches($text, $rustVersionConsumer)).Count -lt 2) {
    $errors.Add("toolchain_output_not_consumed:rust_version")
  }
  if ($text -notmatch 'node-version:\s*\$\{\{\s*needs\.detect-changes\.outputs\.node_version\s*\}\}') {
    $errors.Add("toolchain_output_not_consumed:node_version")
  }
  if ($text -notmatch 'version:\s*\$\{\{\s*needs\.detect-changes\.outputs\.pnpm_version\s*\}\}') {
    $errors.Add("toolchain_output_not_consumed:pnpm_version")
  }
  # 只把版本号写进 action 参数不足以证明真的用上了这个版本，装完必须实测比对。
  if (@([regex]::Matches($text, 'read-mise-versions\.ps1[^\n]*-AssertInstalled')).Count -lt 2) {
    $errors.Add("toolchain_drift_guard_missing")
  }

  # 缓存键必须显式绑定 OS、架构、工具链、锁文件、profile 与有影响的 feature：
  # 少任一维度都会让不同环境或不同依赖图互相复用缓存，证据随之失真。
  if ($rustText -notmatch 'Swatinem/rust-cache') { $errors.Add("cache_action_missing") }
  if ($rustText -notmatch '(?m)^\s+id:\s*rust-cache\s*$') { $errors.Add("cache_step_id_missing") }
  $cacheKeyLine = (@($rustText -split "`n" | Where-Object { $_ -match '^\s+key:\s' }) | Select-Object -First 1)
  if ([string]::IsNullOrEmpty($cacheKeyLine)) {
    $errors.Add("cache_key_missing")
  } else {
    foreach ($dimension in @("matrix.platform.id", "runner.os", "runner.arch", "outputs.rust_version", "profile-", "features-", "Cargo.lock")) {
      if (-not $cacheKeyLine.Contains($dimension)) { $errors.Add("cache_key_dimension_missing:$dimension") }
    }
  }

  # 并发分组：组名必须按事件分开（手动触发不占用 push/PR 的槽位），
  # 只有 PR 才取消旧 run（main 保留在途并排队），且 queue 不得与 cancel-in-progress: true 共存。
  $concurrencyLines = [System.Collections.Generic.List[string]]::new()
  $inConcurrency = $false
  foreach ($line in (Get-WorkflowLines $text)) {
    if ($line -match '^concurrency:\s*$') { $inConcurrency = $true; continue }
    if ($inConcurrency) {
      if ($line -match '^\S') { break }
      $concurrencyLines.Add($line)
    }
  }
  if ($concurrencyLines.Count -eq 0) {
    $errors.Add("concurrency_block_missing")
  } else {
    $concurrencyText = ($concurrencyLines -join "`n")
    if ($concurrencyText -notmatch 'group:\s*ci-\$\{\{\s*github\.event_name\s*\}\}-\$\{\{\s*github\.ref\s*\}\}') {
      $errors.Add("concurrency_group_not_event_scoped")
    }
    if ($concurrencyText -notmatch 'cancel-in-progress:\s*\$\{\{\s*github\.event_name\s*==\s*.pull_request.\s*\}\}') {
      $errors.Add("concurrency_pr_cancel_missing")
    }
    $queueMatch = [regex]::Match($concurrencyText, '(?m)^\s+queue:\s*(\S+)\s*$')
    if ($queueMatch.Success) {
      $queueValue = $queueMatch.Groups[1].Value
      if ($queueValue -notin @("single", "max")) { $errors.Add("concurrency_queue_invalid:$queueValue") }
      if ($queueValue -eq "max" -and $concurrencyText -match '(?m)^\s+cancel-in-progress:\s*true\s*$') {
        $errors.Add("concurrency_queue_max_with_cancel_true")
      }
      # GitHub 在运行时求值 concurrency：只要 cancel-in-progress 在该 run 上解析为 true，
      # queue: max 就会让整个 workflow 判为非法而零作业失败（实测 run 46023242089）。
      # 因此本仓这种必须同时服务 PR（取消）与 main（排队）的单一 concurrency 块禁止出现 queue: max。
      if ($queueValue -eq "max" -and $concurrencyText -match '(?m)^\s+cancel-in-progress:\s*\$\{\{') {
        $errors.Add("concurrency_queue_max_with_dynamic_cancel")
      }
    }
  }

  # Step Summary 必须由校验脚本生成，才能保证脱敏逻辑只有一份实现。
  foreach ($summaryStepName in @("Write sanitized platform summary", "Write sanitized gate summary")) {
    if ($text -notmatch [regex]::Escape($summaryStepName)) { $errors.Add("summary_step_missing:$summaryStepName") }
  }
  if ($text -notmatch '(?m)-Mode Summary') { $errors.Add("summary_mode_not_used") }

  # CI Gate 必须在同一 Run/Attempt/SHA 上对账平台证据，并具备读取原始 job 的权限。
  if ($gateText -notmatch 'download-artifact') { $errors.Add("gate_evidence_download_missing") }
  # 产物名与下载 pattern 均必须带 attempt：否则重跑会写入同名产物，
  # 或把上一 attempt 的旧产物当成本次证据重下载。
  if ($gateText -notmatch 'pattern:\s*ci-evidence-\$\{\{ github\.run_attempt \}\}-\*') { $errors.Add("gate_evidence_pattern_not_attempt_scoped") }
  if ($gateText -notmatch "'-Mode',\s*'Evidence'|Mode\s*=\s*'Evidence'") { $errors.Add("gate_evidence_mode_missing") }
  if ($gateText -notmatch 'actions:\s*read') { $errors.Add("gate_actions_read_missing") }
  # 精确 attempt 端点的起点必须参与对账，不能用 runs 端点或 jobs 列表的 attempt 字段代替。
  if ($gateText -notmatch 'attempts/\$\{RUN_ATTEMPT\}') { $errors.Add("gate_attempt_endpoint_missing") }
  if ($gateText -notmatch 'run_started_at') { $errors.Add("gate_attempt_start_missing") }
  if ($gateText -notmatch '-AttemptStartedAt|AttemptStartedAt' ) { $errors.Add("gate_attempt_start_unused") }
  # 矩阵作业的逐实例结论只能来自原始 jobs：Verify 不得只传聚合 needs 结果。
  if ($gateText -notmatch '-JobsPath|JobsPath\s*=') { $errors.Add("gate_verify_jobs_missing") }
  if ($gateText -notmatch 'ci-jobs\.json') { $errors.Add("gate_verify_jobs_file_missing") }

  # 内部脚本调用必须用命名参数或哈希表 splatting：`$args` 是 PowerShell 自动变量，
  # `./scripts/x.ps1 @args` 会把 '-Mode' 当成位置参数值传给 Mode。
  foreach ($splatMatch in [regex]::Matches($text, '(?m)^\s*\./scripts/[^\s]+\.ps1\s+(@\S+)')) {
    $splat = $splatMatch.Groups[1].Value
    if ($splat -notmatch '^@[A-Za-z_]\w*(Params|Parameters)$') {
      $errors.Add("workflow_script_positional_splat:$splat")
    }
  }
  # 自动变量 $args 不得用于 splatting（无论内部脚本还是 native 命令）：
  # 它既能被赋值又被当成自动变量，行为随调用形式变化，本轮远端故障就出在这里。
  if ($text -match '(?m)@args\b') { $errors.Add("workflow_automation_variable_splat") }
  if ($text -match '(?m)\$args\s*=\s*@\(') { $errors.Add("workflow_automation_variable_assigned") }

  # 逐实例证据必须先取回原始 jobs 才能对账，顺序颠倒就是接线错误。
  $fetchIndex = $text.IndexOf("name: Fetch attempt start and raw run jobs", [StringComparison]::Ordinal)
  $verifyIndex = $text.IndexOf("name: Verify CI gate plan against actual job evidence", [StringComparison]::Ordinal)
  if ($fetchIndex -lt 0) {
    $errors.Add("gate_jobs_step_missing")
  } elseif ($verifyIndex -lt 0) {
    $errors.Add("gate_verify_step_missing")
  } elseif ($fetchIndex -gt $verifyIndex) {
    $errors.Add("gate_jobs_before_verify")
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

function Format-Seconds($DurationMs) {
  if ($null -eq $DurationMs) { return "未知" }
  $value = 0.0
  if (-not [double]::TryParse([string]$DurationMs, [ref]$value)) { return "未知" }
  if ($value -lt 0) { return "未知" }
  return ([math]::Round($value / 1000.0, 1)).ToString("0.0", [System.Globalization.CultureInfo]::InvariantCulture)
}

# 所有进入 Step Summary 的文本必须先过脱敏：绝对路径、用户目录与令牌一律不出现。
function ConvertTo-SummarySafeText([string]$Value) {
  if ([string]::IsNullOrEmpty($Value)) { return "" }
  $sanitized = [string]$Value
  foreach ($prefix in @($env:RUNNER_TEMP, $env:GITHUB_WORKSPACE, $env:GITHUB_JOB, $env:HOME, $env:USERPROFILE)) {
    if (-not [string]::IsNullOrWhiteSpace($prefix) -and $prefix.Length -gt 3) {
      $sanitized = $sanitized.Replace($prefix, "[workspace]")
    }
  }
  $sanitized = [regex]::Replace($sanitized, '[A-Za-z]:\\[^\s\|)]*', '[redacted-path]')
  $sanitized = [regex]::Replace($sanitized, '(^|[\s(])/(?:home|Users|root|tmp|mnt)/[^\s\|)]*', '$1[redacted-path]')
  $sanitized = [regex]::Replace($sanitized, '(?i)\b(?:gh[pousr]_[A-Za-z0-9]{16,}|github_pat_[A-Za-z0-9_]{16,})\b', '[redacted-token]')
  $sanitized = $sanitized.Replace("|", "/")
  return $sanitized
}

function Read-SummaryStages([string]$SourcePath) {
  if ([string]::IsNullOrWhiteSpace($SourcePath)) { return @() }
  if (-not (Test-Path -LiteralPath $SourcePath -PathType Leaf)) { return @() }
  try {
    $raw = Get-Content -LiteralPath $SourcePath -Raw -Encoding utf8
    if ([string]::IsNullOrWhiteSpace($raw)) { return @() }
    return @($raw | ConvertFrom-Json -Depth 12 -ErrorAction Stop)
  } catch {
    return @()
  }
}

function Read-SummarySkips([string]$SkipJson) {
  if ([string]::IsNullOrWhiteSpace($SkipJson)) { return @() }
  $result = [System.Collections.Generic.List[object]]::new()
  try {
    $parsed = $SkipJson | ConvertFrom-Json -Depth 6 -ErrorAction Stop
  } catch {
    return @()
  }
  if ($null -eq $parsed) { return @() }
  if ($parsed -is [System.Management.Automation.PSCustomObject]) {
    foreach ($property in $parsed.PSObject.Properties) {
      $result.Add([pscustomobject]@{ Job = $property.Name; Reason = [string]$property.Value })
    }
    return @($result)
  }
  foreach ($item in @($parsed)) {
    $jobName = if ($item.PSObject.Properties["job"]) { [string]$item.job } else { [string]$item.name }
    $reason = if ($item.PSObject.Properties["reason"]) { [string]$item.reason } else { "计划内跳过" }
    $result.Add([pscustomobject]@{ Job = $jobName; Reason = $reason })
  }
  return @($result)
}

# Summary 只是汇报，不参与门禁判定：任何异常都只告警，不能让上报步骤拖坠整个 run。
function Invoke-SummaryMode {
  $title = if ([string]::IsNullOrWhiteSpace($SummaryTitle)) { "CI 耗时与缓存摘要" } else { ConvertTo-SummarySafeText $SummaryTitle }
  $platform = if ([string]::IsNullOrWhiteSpace($PlatformId)) { "全部平台" } else { ConvertTo-SummarySafeText $PlatformId }
  $cacheValue = if ([string]::IsNullOrWhiteSpace($CacheStatus)) { "unknown" } else { (ConvertTo-SummarySafeText $CacheStatus).ToLowerInvariant() }
  if ($cacheValue -notin @("hit", "miss", "true", "false", "unknown")) { $cacheValue = "unknown" }
  $cacheLabel = if ($cacheValue -in @("hit", "true")) { "hit" } elseif ($cacheValue -in @("miss", "false")) { "miss" } else { "unknown" }

  $markdown = [System.Collections.Generic.List[string]]::new()
  $markdown.Add("### $title（$platform）")
  $markdown.Add("")

  $stages = @(Read-SummaryStages $StagesPath)
  if ($stages.Count -eq 0) {
    $markdown.Add("- 阶段数据：未产出（本次未执行可采集的阶段或采集步骤被跳过）")
  } else {
    $markdown.Add("| 阶段 | 状态 | 耗时(s) |")
    $markdown.Add("| --- | --- | --- |")
    $totalMs = 0.0
    $totalKnown = $true
    foreach ($stage in $stages) {
      $stageName = ConvertTo-SummarySafeText ([string]$stage.name)
      $stageStatus = ConvertTo-SummarySafeText ([string]$stage.status)
      $seconds = Format-Seconds $stage.durationMs
      if ($seconds -eq "未知") { $totalKnown = $false } else { $totalMs += [double]::Parse($seconds, [System.Globalization.CultureInfo]::InvariantCulture) * 1000.0 }
      $markdown.Add("| $stageName | $stageStatus | $seconds |")
    }
    # 必须写“阶段累计”而不是“墙钟”：排队、action 安装与 checkout 不在其中。
    if ($totalKnown) {
      $markdown.Add("")
      $markdown.Add("- 阶段累计：$([math]::Round($totalMs / 1000.0, 1).ToString('0.0', [System.Globalization.CultureInfo]::InvariantCulture)) s（不含排队、checkout 与 action 安装）")
    }
  }

  $markdown.Add("- 依赖缓存：$cacheLabel")

  $skips = @(Read-SummarySkips $SkipsJson)
  if ($skips.Count -eq 0) {
    $markdown.Add("- 跳过：0 项")
  } else {
    $markdown.Add("- 跳过：$($skips.Count) 项")
    foreach ($skip in $skips) {
      $markdown.Add("  - $(ConvertTo-SummarySafeText $skip.Job)：$(ConvertTo-SummarySafeText $skip.Reason)")
    }
  }

  $text = ($markdown -join "`n") + "`n"
  $target = if ([string]::IsNullOrWhiteSpace($SummaryPath)) { $env:GITHUB_STEP_SUMMARY } else { $SummaryPath }
  if ([string]::IsNullOrWhiteSpace($target)) {
    Write-Output $text
  } else {
    $parent = Split-Path -Parent $target
    if (-not [string]::IsNullOrWhiteSpace($parent) -and -not (Test-Path -LiteralPath $parent -PathType Container)) {
      New-Item -ItemType Directory -Path $parent -Force | Out-Null
    }
    Add-Content -LiteralPath $target -Value $text -Encoding utf8
  }
  Write-Host ("已写入脱敏摘要：$platform（阶段 $($stages.Count) 段，跳过 $($skips.Count) 项，缓存 $cacheLabel）")
  return 0
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
    "Evidence" {
      $errors = Invoke-EvidenceMode
      exit (Write-CheckResult $errors "Evidence")
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
    "Summary" {
      try {
        exit (Invoke-SummaryMode)
      } catch {
        Write-Host "::warning::CI-GATE-SUMMARY $($_.Exception.Message)" -ForegroundColor Yellow
        exit 0
      }
    }
    default {
      throw "未知模式：$Mode"
    }
  }
} catch {
  Write-Host "::error::CI-GATE-ERROR $($_.Exception.Message)" -ForegroundColor Red
  exit 2
}
