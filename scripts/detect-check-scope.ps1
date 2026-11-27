param(
  [string]$BaseRef = "",
  [string]$HeadRef = "HEAD",
  [string[]]$ChangedFiles = @(),
  [string]$ChangedFilesJson = "",
  [string]$ChangedFilesPath = "",
  [switch]$IncludeWorkingTree,
  [switch]$Full,
  # CI-C 明确入口：只有显式请求才追加 Intel 平台实例，普通 -Full 不再推导 Intel。
  [switch]$IncludeIntel,
  [ValidateSet("auto", "local", "pull_request", "push", "manual", "release")]
  [string]$Event = "auto",
  # 仓库根覆盖：默认本脚本所在仓库，仅供夹具测试指到临时工作区。
  [string]$RepoRoot = "",
  [switch]$Json
)

$ErrorActionPreference = "Stop"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$OutputEncoding = [System.Text.Encoding]::UTF8

$repoRoot = if ([string]::IsNullOrWhiteSpace($RepoRoot)) {
  (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
} else {
  (Resolve-Path -LiteralPath $RepoRoot).Path
}
$repoRootCanonical = $repoRoot.Replace('\', '/')
# 路径分隔符与大小写规则随宿主走：Windows 接受 native/slash 且大小写不敏感，
# POSIX 保留字面反斜杠作为文件名的一部分且路径大小写敏感。
$isWindowsHost = [System.IO.Path]::DirectorySeparatorChar -eq '\'
$pathComparison = if ($isWindowsHost) { [StringComparison]::OrdinalIgnoreCase } else { [StringComparison]::Ordinal }

# 计划版本号：结构化检测计划的对外契约版本，加法演进不改变既有字段语义。
$PlanSchemaVersion = 1
# Rust 基线平台固定为 Windows；Linux 与 ARM64 macOS 由实际消费者闭包追加。
$RustBaselinePlatform = "windows"
# 这些 Crate 的变更会影响跨宿主行为，需额外覆盖 Linux 与 ARM64 macOS。
$CrossHostConsumerPackages = @(
  "soxu-cli", "soxu-daemon", "soxu-desktop", "soxu-drivers", "soxu-git", "soxu-magpie"
)

# 平台目录是计划与证据共用的稳定身份表。计划只选择平台 id，runner 标签与 CPU 架构
# 一律由本表派生，因此计划无法自造 runner 或冒充架构。键序即矩阵实例的确定性顺序。
$PlatformCatalog = [ordered]@{
  "windows"     = [ordered]@{ id = "windows"; os = "windows"; arch = "x64"; runner = "windows-2025" }
  "linux"       = [ordered]@{ id = "linux"; os = "linux"; arch = "x64"; runner = "ubuntu-24.04" }
  "macos-arm64" = [ordered]@{ id = "macos-arm64"; os = "macos"; arch = "arm64"; runner = "macos-15" }
  "macos-intel" = [ordered]@{ id = "macos-intel"; os = "macos"; arch = "x64"; runner = "macos-15-intel" }
}
$PlatformOrder = @($PlatformCatalog.Keys)

# 架构敏感路径：目前为空。repository_create 的 /proc/self/fd、Windows reparse 加固与
# Darwin 写入拒绝都是 OS 契约，由常规 Windows/Linux/macOS ARM64 三平台矩阵覆盖，不得
# 当成 CPU 架构敏感规则。只有出现可定位、可复现的 CPU 架构差异证据时才在本表登记。
$ArchSensitivePathPrefixes = @()
# 基础设施文件变更直接触发全量门禁。
$InfraFullPaths = @(
  "mise.toml",
  ".github/workflows/ci.yml",
  "scripts/check-all.ps1",
  "scripts/detect-check-scope.ps1",
  "scripts/test-check-scope.ps1",
  "scripts/test-ci-gate.ps1",
  "scripts/verify-ci-gate.ps1",
  "scripts/verify-commits-and-privacy.ps1"
)
# 非代码/治理/认知文档与基础配置只需运行治理与隐私门禁。
$DocsOrGovernancePaths = @(
  "AGENTS.md", "CLAUDE.md", "README.md",
  "aoci.txt", "aoci.meta.txt", "aoci.code.txt", "aoci.database.txt",
  ".editorconfig", ".gitattributes", ".gitignore", ".gitleaks.toml",
  ".github/workflows/release.yml",
  "paseo.json", "scripts/mise-doctor.ps1", "scripts/setup-hooks.ps1", "scripts/setup-worktree.ps1"
)

$reasons = [System.Collections.Generic.List[string]]::new()
$fallbackReasons = [System.Collections.Generic.List[string]]::new()

function Add-Reason([string]$Value) {
  if (-not $reasons.Contains($Value)) { $reasons.Add($Value) }
}

function Add-FallbackReason([string]$Value) {
  if (-not $fallbackReasons.Contains($Value)) { $fallbackReasons.Add($Value) }
}

function Normalize-RepoPath([string]$PathValue) {
  if ([string]::IsNullOrWhiteSpace($PathValue)) { return "" }
  $normalized = $PathValue.Trim().Replace('\', '/')
  while ($normalized.StartsWith('./')) {
    $normalized = $normalized.Substring(2)
  }
  return $normalized
}

function Test-StructuredRepoPath([string]$PathValue) {
  # 版本化结构化输入的路径必须是仓库相对规范路径。字面反斜杠、逗号、空白与非 ASCII
  # 都是合法的：只拒绝越界、盘符、空段和相对段这类无法安全归因的写法。
  if ($PathValue.IndexOf([char]0) -ge 0) { return $false }
  if ($PathValue.StartsWith("/")) { return $false }
  if ($PathValue -match '^[A-Za-z]:') { return $false }
  if ($PathValue.EndsWith("/")) { return $false }
  foreach ($segment in $PathValue.Split('/')) {
    if ($segment -eq "" -or $segment -eq "." -or $segment -eq "..") { return $false }
  }
  return $true
}

function Get-SanitizedProcessDetail([string]$Text, [int]$ExitCode) {
  # 只保留可机读的类别与最小诊断：绝对目录、UNC、URL 与凭据形态一律不进入工作流计划。
  $line = @(($Text -split "`n") | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }) | Select-Object -First 1
  if ([string]::IsNullOrWhiteSpace($line)) { return "exit $ExitCode" }
  $detail = $line.Trim()
  $detail = [regex]::Replace($detail, '(?i)\b[a-z]:[\\/][^\s"'']*', '<path>')
  $detail = [regex]::Replace($detail, '(?i)\\\\[^\s"'']+', '<unc>')
  $detail = [regex]::Replace($detail, '(?i)(?<![a-z0-9])/(?:[^\s"'':]+/)+[^\s"'':]*', '<path>')
  $detail = [regex]::Replace($detail, '(?i)[a-z][a-z0-9+.\-]*://\S+', '<url>')
  $detail = [regex]::Replace($detail, '(?i)\b(token|password|passwd|secret|api[_-]?key|authorization)\b\s*[:=]\s*\S+', '$1=<redacted>')
  $detail = [regex]::Replace($detail, '\s+', ' ')
  if ($detail.Length -gt 120) { $detail = $detail.Substring(0, 120) + '…' }
  return "exit ${ExitCode}: $detail"
}

# 结构化进程捕获：保留原始字节与尾部 NUL，避免 PowerShell 管道按行解码破坏记录边界。
function Invoke-CapturedProcess {
  param(
    [Parameter(Mandatory = $true)][string]$FileName,
    [string[]]$Arguments = @(),
    [string]$WorkingDirectory = $repoRoot
  )
  $psi = [System.Diagnostics.ProcessStartInfo]::new()
  $psi.FileName = $FileName
  $psi.UseShellExecute = $false
  $psi.RedirectStandardOutput = $true
  $psi.RedirectStandardError = $true
  $psi.CreateNoWindow = $true
  $psi.WorkingDirectory = $WorkingDirectory
  foreach ($arg in $Arguments) { $psi.ArgumentList.Add($arg) }

  $proc = $null
  try {
    $proc = [System.Diagnostics.Process]::Start($psi)
  } catch {
    return [pscustomobject]@{
      Started  = $false
      ExitCode = -1
      Bytes    = [byte[]]@()
      Error    = $_.Exception.Message
    }
  }

  $buffer = [System.IO.MemoryStream]::new()
  try {
    $stderrTask = $proc.StandardError.ReadToEndAsync()
    $proc.StandardOutput.BaseStream.CopyTo($buffer)
    $stderr = $stderrTask.GetAwaiter().GetResult()
    $proc.WaitForExit()
    return [pscustomobject]@{
      Started  = $true
      ExitCode = $proc.ExitCode
      Bytes    = $buffer.ToArray()
      Error    = [string]$stderr
    }
  } finally {
    $buffer.Dispose()
    $proc.Dispose()
  }
}

function Invoke-Git([string[]]$Arguments) {
  $gitArgs = @("-C", $repoRoot, "-c", "core.quotepath=false") + $Arguments
  return Invoke-CapturedProcess -FileName "git" -Arguments $gitArgs
}

function ConvertFrom-Utf8Bytes([byte[]]$Bytes) {
  if ($null -eq $Bytes -or $Bytes.Length -eq 0) { return "" }
  return [System.Text.UTF8Encoding]::new($false, $false).GetString($Bytes)
}

function Split-NulRecords([byte[]]$Bytes) {
  $text = ConvertFrom-Utf8Bytes $Bytes
  if ($text.Length -eq 0) { return @() }
  $records = [System.Collections.Generic.List[string]]::new()
  foreach ($part in $text.Split([char]0)) { $records.Add($part) }
  # NUL 分隔输出总以结尾 NUL 收束，会产生一个末尾空记录；真实路径不可能为空。
  while ($records.Count -gt 0 -and $records[$records.Count - 1] -eq "") {
    $records.RemoveAt($records.Count - 1)
  }
  return @($records)
}

function ConvertFrom-NameStatusRecords([string[]]$Records) {
  # git diff --name-status -z -M 的记录形态：<STATUS>\0<path>\0，
  # 其中重命名/复制为 <R|C><score>\0<old>\0<new>\0，删除为 <D>\0<old>\0。
  $files = [System.Collections.Generic.SortedSet[string]]::new([StringComparer]::Ordinal)
  $index = 0
  while ($index -lt $Records.Count) {
    $status = $Records[$index]
    $index++
    if ($status -notmatch '^[A-Z][0-9]*$') {
      return [pscustomobject]@{ Ok = $false; Files = @(); Reason = "unexpected_status_token:$status" }
    }
    $code = $status.Substring(0, 1)
    if ($code -eq "R" -or $code -eq "C") {
      if ($index + 1 -ge $Records.Count) {
        return [pscustomobject]@{ Ok = $false; Files = @(); Reason = "truncated_rename_record:$status" }
      }
      $oldPath = $Records[$index]
      $newPath = $Records[$index + 1]
      $index += 2
      if ([string]::IsNullOrEmpty($oldPath) -or [string]::IsNullOrEmpty($newPath)) {
        return [pscustomobject]@{ Ok = $false; Files = @(); Reason = "empty_rename_record:$status" }
      }
      [void]$files.Add($oldPath)
      [void]$files.Add($newPath)
      continue
    }
    if ($index -ge $Records.Count) {
      return [pscustomobject]@{ Ok = $false; Files = @(); Reason = "truncated_record:$status" }
    }
    $path = $Records[$index]
    $index++
    if ([string]::IsNullOrEmpty($path)) {
      return [pscustomobject]@{ Ok = $false; Files = @(); Reason = "empty_path_record:$status" }
    }
    [void]$files.Add($path)
  }
  return [pscustomobject]@{ Ok = $true; Files = @($files); Reason = "" }
}

function Get-CommitSha([string]$Ref) {
  if ([string]::IsNullOrWhiteSpace($Ref)) { return $null }
  $result = Invoke-Git @("rev-parse", "--verify", "--quiet", "$Ref^{commit}")
  if (-not $result.Started -or $result.ExitCode -ne 0) { return $null }
  $sha = (ConvertFrom-Utf8Bytes $result.Bytes).Trim()
  if ($sha -notmatch '^[0-9a-f]{40,64}$') { return $null }
  return $sha
}

function Get-MergeBase([string]$Base, [string]$Head) {
  $result = Invoke-Git @("merge-base", $Base, $Head)
  if (-not $result.Started -or $result.ExitCode -ne 0) { return $null }
  $sha = (ConvertFrom-Utf8Bytes $result.Bytes).Trim()
  if ($sha -notmatch '^[0-9a-f]{40,64}$') { return $null }
  return $sha
}

function Resolve-AutoBaseRef([string]$RequestedBase, [string]$TargetHead) {
  if (-not [string]::IsNullOrWhiteSpace($RequestedBase)) {
    if ($RequestedBase -match '^0+$') {
      return [pscustomobject]@{ Ref = $null; Reason = "base_ref_all_zeros" }
    }
    if ($null -eq (Get-CommitSha $RequestedBase)) {
      return [pscustomobject]@{ Ref = $null; Reason = "base_ref_unresolvable:$RequestedBase" }
    }
    return [pscustomobject]@{ Ref = $RequestedBase; Reason = "explicit_base" }
  }

  foreach ($candidate in @("origin/main", "main")) {
    if ($null -ne (Get-CommitSha $candidate)) {
      return [pscustomobject]@{ Ref = $candidate; Reason = "auto_base:$candidate" }
    }
  }

  if ($null -ne (Get-CommitSha "$TargetHead~1")) {
    return [pscustomobject]@{ Ref = "$TargetHead~1"; Reason = "auto_base:head_parent" }
  }

  return [pscustomobject]@{ Ref = $null; Reason = "no_base_ref_available" }
}

function Get-DiffPaths([string]$From, [string]$To) {
  $result = Invoke-Git @("diff", "--name-status", "-z", "-M", $From, $To)
  if (-not $result.Started -or $result.ExitCode -ne 0) {
    return [pscustomobject]@{ Ok = $false; Files = @(); Reason = "git_diff_failed:$(Get-SanitizedProcessDetail $result.Error $result.ExitCode)" }
  }
  return ConvertFrom-NameStatusRecords @(Split-NulRecords $result.Bytes)
}

function Get-WorktreePaths {
  $files = [System.Collections.Generic.SortedSet[string]]::new([StringComparer]::Ordinal)
  foreach ($variant in @(
    @("diff", "--name-status", "-z", "-M"),
    @("diff", "--cached", "--name-status", "-z", "-M")
  )) {
    $result = Invoke-Git $variant
    if (-not $result.Started -or $result.ExitCode -ne 0) {
      return [pscustomobject]@{ Ok = $false; Files = @(); Reason = "git_diff_failed:$(Get-SanitizedProcessDetail $result.Error $result.ExitCode)" }
    }
    $parsed = ConvertFrom-NameStatusRecords @(Split-NulRecords $result.Bytes)
    if (-not $parsed.Ok) { return $parsed }
    foreach ($item in $parsed.Files) { [void]$files.Add($item) }
  }

  # 未跟踪文件用 ls-files -z，同样按 NUL 记录消费，避免空白/逗号被拆分。
  $untracked = Invoke-Git @("ls-files", "-z", "--others", "--exclude-standard")
  if (-not $untracked.Started -or $untracked.ExitCode -ne 0) {
    return [pscustomobject]@{ Ok = $false; Files = @(); Reason = "git_ls_files_failed:$(Get-SanitizedProcessDetail $untracked.Error $untracked.ExitCode)" }
  }
  foreach ($item in @(Split-NulRecords $untracked.Bytes)) {
    if (-not [string]::IsNullOrEmpty($item)) { [void]$files.Add($item) }
  }

  return [pscustomobject]@{ Ok = $true; Files = @($files); Reason = "" }
}

function Get-RepoRelativeSubPath([string]$PathValue) {
  if ([string]::IsNullOrWhiteSpace($PathValue)) { return $null }
  # 绝对路径按宿主规则归一化：Windows 接受 native 或 slash 分隔，
  # POSIX 不把字面反斜杠当分隔符；拒绝不属于两种语义的混合写法。
  $absolute = [System.IO.Path]::GetFullPath($PathValue)
  # Windows 的 GetFullPath 会统一换成 native 分隔符，所以归一到 '/' 必须在它之后做；
  # POSIX 不替换，保留字面反斜杠作为文件名内容。
  if ($isWindowsHost) { $absolute = $absolute.Replace('\', '/') }
  if ($absolute -eq $repoRootCanonical) { return "" }
  $prefix = $repoRootCanonical + "/"
  if (-not $absolute.StartsWith($prefix, $pathComparison)) { return $null }
  return $absolute.Substring($prefix.Length)
}

function Get-CargoWorkspaceGraph {
  # 依赖图来自 Cargo 自己的 JSON（workspace_members + packages），
  # 不解析 TOML 字段，天然覆盖 workspace 继承、成员通配、重命名别名与 target-specific。
  $manifestPath = Join-Path $repoRoot "Cargo.toml"
  if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
    return [pscustomobject]@{ Ok = $false; Reason = "cargo_manifest_absent"; DirToPkg = @{}; ReverseDeps = @{}; Packages = @() }
  }

  $result = Invoke-CapturedProcess -FileName "cargo" -Arguments @(
    "metadata", "--no-deps", "--format-version", "1", "--locked", "--manifest-path", $manifestPath
  )
  if (-not $result.Started) {
    return [pscustomobject]@{ Ok = $false; Reason = "cargo_not_invokable"; DirToPkg = @{}; ReverseDeps = @{}; Packages = @() }
  }
  if ($result.ExitCode -ne 0) {
    return [pscustomobject]@{ Ok = $false; Reason = "cargo_metadata_failed:$(Get-SanitizedProcessDetail $result.Error $result.ExitCode)"; DirToPkg = @{}; ReverseDeps = @{}; Packages = @() }
  }

  try {
    $metadata = (ConvertFrom-Utf8Bytes $result.Bytes) | ConvertFrom-Json -Depth 64 -ErrorAction Stop
  } catch {
    return [pscustomobject]@{ Ok = $false; Reason = "cargo_metadata_invalid_json"; DirToPkg = @{}; ReverseDeps = @{}; Packages = @() }
  }

  $memberIds = [System.Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
  foreach ($id in @($metadata.workspace_members)) {
    if ($id -is [string] -and -not [string]::IsNullOrWhiteSpace($id)) { [void]$memberIds.Add($id) }
  }
  if ($memberIds.Count -eq 0) {
    return [pscustomobject]@{ Ok = $false; Reason = "cargo_members_empty"; DirToPkg = @{}; ReverseDeps = @{}; Packages = @() }
  }

  $pkgById = @{}
  foreach ($pkg in @($metadata.packages)) {
    if ($null -eq $pkg -or $pkg.id -isnot [string]) { continue }
    $pkgById[[string]$pkg.id] = $pkg
  }

  $members = [System.Collections.Generic.List[object]]::new()
  foreach ($id in $memberIds) {
    if (-not $pkgById.ContainsKey($id)) {
      # 成员身份在 packages 中找不到同身份记录：无法可靠建图，保守回退。
      return [pscustomobject]@{ Ok = $false; Reason = "cargo_member_identity_missing:$id"; DirToPkg = @{}; ReverseDeps = @{}; Packages = @() }
    }
    $members.Add($pkgById[$id])
  }

  $dirToPkg = @{}
  foreach ($pkg in $members) {
    if ($pkg.manifest_path -isnot [string] -or [string]::IsNullOrWhiteSpace($pkg.name)) {
      return [pscustomobject]@{ Ok = $false; Reason = "cargo_member_shape_invalid"; DirToPkg = @{}; ReverseDeps = @{}; Packages = @() }
    }
    $relManifest = Get-RepoRelativeSubPath $pkg.manifest_path
    if ($null -eq $relManifest) {
      return [pscustomobject]@{ Ok = $false; Reason = "cargo_manifest_outside_repo:$($pkg.name)"; DirToPkg = @{}; ReverseDeps = @{}; Packages = @() }
    }
    $dir = if ($relManifest.Contains('/')) { $relManifest.Substring(0, $relManifest.LastIndexOf('/')) } else { "" }
    if ($dirToPkg.ContainsKey($dir)) {
      return [pscustomobject]@{ Ok = $false; Reason = "cargo_member_dir_collision:$dir"; DirToPkg = @{}; ReverseDeps = @{}; Packages = @() }
    }
    $dirToPkg[$dir] = [string]$pkg.name
  }

  $reverseDeps = @{}
  foreach ($pkg in $members) {
    $consumer = [string]$pkg.name
    foreach ($dep in @($pkg.dependencies)) {
      # 依赖类型（normal/dev/build）、目标平台与 optional 一律不裁剪：
      # 任何会影响成员的依赖边都必须进入影响范围，宿主平台不等于全部影响范围。
      if ($null -eq $dep -or $dep.path -isnot [string] -or [string]::IsNullOrWhiteSpace($dep.path)) { continue }
      $depDir = Get-RepoRelativeSubPath $dep.path
      if ($null -eq $depDir) { continue }
      if (-not $dirToPkg.ContainsKey($depDir)) { continue }
      # 用目标成员的真实包名，忽略 Cargo.toml 里的重命名别名。
      $depName = $dirToPkg[$depDir]
      if (-not $reverseDeps.ContainsKey($depName)) {
        $reverseDeps[$depName] = [System.Collections.Generic.SortedSet[string]]::new([StringComparer]::Ordinal)
      }
      [void]$reverseDeps[$depName].Add($consumer)
    }
  }

  return [pscustomobject]@{
    Ok          = $true
    Reason      = ""
    DirToPkg    = $dirToPkg
    ReverseDeps = $reverseDeps
    Packages    = @($dirToPkg.Values | Sort-Object)
  }
}

function Find-MemberDirForPath($DirToPkg, [string]$Path) {
  # 成员目录从 Cargo 的 manifest_path 得出，因此嵌套通配成员（如 crates/group/sub）
  # 会按最长前缀命中自己的真实成员目录，而不是只看 crates/<第一级>。
  $best = $null
  foreach ($dir in $DirToPkg.Keys) {
    if ([string]::IsNullOrEmpty($dir)) { continue }
    $prefix = $dir + "/"
    if ($Path.Length -le $prefix.Length) { continue }
    if (-not $Path.StartsWith($prefix, [StringComparison]::Ordinal)) { continue }
    if ($null -eq $best -or $dir.Length -gt $best.Length) { $best = $dir }
  }
  return $best
}

function Expand-ReverseDependencies([string[]]$SeedPackages, $ReverseDeps) {
  $visited = [System.Collections.Generic.SortedSet[string]]::new([StringComparer]::Ordinal)
  $queue = [System.Collections.Generic.Queue[string]]::new()
  foreach ($pkg in $SeedPackages) {
    if ($visited.Add($pkg)) { $queue.Enqueue($pkg) }
  }
  while ($queue.Count -gt 0) {
    $current = $queue.Dequeue()
    if (-not $ReverseDeps.ContainsKey($current)) { continue }
    foreach ($dependent in $ReverseDeps[$current]) {
      if ($visited.Add($dependent)) { $queue.Enqueue($dependent) }
    }
  }
  return @($visited)
}

function Read-StructuredChangedFiles {
  # 非阻塞契约：任何结构性问题都不抛异常，而是返回稳定类别，由调用方保守回退全量。
  # 检查器自己接收到的非法计划仍由 verify-ci-gate 失败关闭，两者职责不同。
  $payload = $null
  try {
    if (-not [string]::IsNullOrWhiteSpace($ChangedFilesPath)) {
      if (-not (Test-Path -LiteralPath $ChangedFilesPath -PathType Leaf)) {
        return [pscustomobject]@{ Ok = $false; Category = "structured_payload_unreadable"; Files = @() }
      }
      $payload = Get-Content -LiteralPath $ChangedFilesPath -Raw -Encoding utf8
    } else {
      $payload = $ChangedFilesJson
    }
  } catch {
    return [pscustomobject]@{ Ok = $false; Category = "structured_payload_unreadable"; Files = @() }
  }
  if ([string]::IsNullOrWhiteSpace($payload)) {
    return [pscustomobject]@{ Ok = $false; Category = "structured_payload_empty"; Files = @() }
  }

  try {
    $parsed = $payload | ConvertFrom-Json -Depth 20 -ErrorAction Stop
  } catch {
    return [pscustomobject]@{ Ok = $false; Category = "structured_payload_invalid_json"; Files = @() }
  }
  if ($null -eq $parsed -or $parsed -isnot [pscustomobject]) {
    return [pscustomobject]@{ Ok = $false; Category = "structured_payload_not_object"; Files = @() }
  }

  $versionProp = $parsed.PSObject.Properties["schemaVersion"]
  if ($null -eq $versionProp -or $null -eq $versionProp.Value) {
    return [pscustomobject]@{ Ok = $false; Category = "structured_schema_version_missing"; Files = @() }
  }
  if ($versionProp.Value -isnot [int] -and $versionProp.Value -isnot [long]) {
    return [pscustomobject]@{ Ok = $false; Category = "structured_schema_version_invalid"; Files = @() }
  }
  if ([int]$versionProp.Value -ne $PlanSchemaVersion) {
    return [pscustomobject]@{ Ok = $false; Category = "structured_schema_version_unsupported"; Files = @() }
  }

  $filesProp = $parsed.PSObject.Properties["changedFiles"]
  if ($null -eq $filesProp -or $null -eq $filesProp.Value) {
    return [pscustomobject]@{ Ok = $false; Category = "structured_changed_files_missing"; Files = @() }
  }
  # 必须真的是 JSON 数组：单个标量不能被 @() 包装后当成一条路径。
  if ($filesProp.Value -isnot [System.Array]) {
    return [pscustomobject]@{ Ok = $false; Category = "structured_changed_files_not_array"; Files = @() }
  }

  $files = [System.Collections.Generic.SortedSet[string]]::new([StringComparer]::Ordinal)
  foreach ($item in @($filesProp.Value)) {
    if ($item -isnot [string]) {
      return [pscustomobject]@{ Ok = $false; Category = "structured_changed_files_entry_not_string"; Files = @() }
    }
    if ([string]::IsNullOrWhiteSpace($item)) {
      return [pscustomobject]@{ Ok = $false; Category = "structured_changed_files_entry_empty"; Files = @() }
    }
    if (-not (Test-StructuredRepoPath $item)) {
      return [pscustomobject]@{ Ok = $false; Category = "structured_path_out_of_repo"; Files = @() }
    }
    [void]$files.Add($item)
  }
  return [pscustomobject]@{ Ok = $true; Category = ""; Files = @($files) }
}

# ---------------------------------------------------------------------------
# 一、确定变更路径与判定模式
# ---------------------------------------------------------------------------

$mode = "auto"
$fullScope = [bool]$Full
# Intel 只由显式入口（full-with-intel）或正式发版入口请求，不由普通 -Full 推导。
$intelRequested = [bool]$IncludeIntel
$releaseEntryRequested = $false
$resolvedBaseRef = ""
$headRefValue = $HeadRef
$mergeBaseSha = ""
# 解析后的真实 SHA 与 ref 分开保存：CI-C 要用它把计划、checkout 与实际证据绑定。
$resolvedBaseSha = ""
$resolvedHeadSha = ""
$checkoutSha = [string](Get-CommitSha "HEAD")
$detectedFiles = [System.Collections.Generic.SortedSet[string]]::new([StringComparer]::Ordinal)

if ($fullScope) {
  $mode = "full"
  Add-Reason "explicit_full"
}

if (-not $fullScope) {
  $structuredInputPresent = (-not [string]::IsNullOrWhiteSpace($ChangedFilesJson)) -or (-not [string]::IsNullOrWhiteSpace($ChangedFilesPath))

  if ($structuredInputPresent) {
    $structured = Read-StructuredChangedFiles
    if ($structured.Ok) {
      $mode = "explicit"
      foreach ($item in $structured.Files) { [void]$detectedFiles.Add($item) }
      Add-Reason "structured_changed_files:$($detectedFiles.Count)"
    } else {
      # 无效结构化输入不中止检查：保守回退全量，并留下可机器分组的稳定原因。
      $fullScope = $true
      $mode = "full"
      Add-FallbackReason "structured_input_invalid:$($structured.Category)"
      Add-Reason "fallback_full:structured_input_invalid"
    }
  } elseif ($ChangedFiles.Count -gt 0) {
    # 旧 CLI 契约继续按逗号拆分，并把既有 Windows 路径格式归一化为仓库相对规范路径。
    $mode = "explicit"
    foreach ($raw in $ChangedFiles) {
      foreach ($piece in ($raw -split ',')) {
        $norm = Normalize-RepoPath $piece
        if (-not [string]::IsNullOrWhiteSpace($norm)) { [void]$detectedFiles.Add($norm) }
      }
    }
    Add-Reason "explicit_changed_files:$($detectedFiles.Count)"
  } else {
    $effectiveEvent = $Event
    if ($effectiveEvent -eq "auto") {
      $effectiveEvent = if ($IncludeWorkingTree) { "local" } else { "range" }
    }

    switch ($effectiveEvent) {
      "manual" {
        # 手动全量只提升工作区范围，不隐式追加 Intel；Intel 须显式 -IncludeIntel。
        $mode = "full"
        $fullScope = $true
        Add-Reason "manual_full"
      }
      "release" {
        # 正式发版入口（tag 发版）：全量范围 + Intel 原生验证。
        $mode = "full"
        $fullScope = $true
        $releaseEntryRequested = $true
        Add-Reason "release_entry_full"
      }
      "push" {
        $mode = "push"
        # push 事件比较 before/after 的完整树，即使历史非线性也不改用 merge base。
        if ([string]::IsNullOrWhiteSpace($BaseRef)) {
          $fullScope = $true
          Add-FallbackReason "base_ref_missing"
        } elseif ($BaseRef -match '^0+$') {
          $fullScope = $true
          Add-FallbackReason "base_ref_all_zeros"
        } elseif ([string]::IsNullOrWhiteSpace($HeadRef) -or $HeadRef -match '^0+$') {
          $fullScope = $true
          Add-FallbackReason "head_ref_all_zeros"
        } else {
          $baseSha = Get-CommitSha $BaseRef
          $headSha = Get-CommitSha $HeadRef
          if ($null -eq $baseSha) {
            $fullScope = $true
            Add-FallbackReason "base_ref_unresolvable:$BaseRef"
          } elseif ($null -eq $headSha) {
            $fullScope = $true
            Add-FallbackReason "head_ref_unresolvable:$HeadRef"
          } else {
            $resolvedBaseRef = $BaseRef
            $headRefValue = $HeadRef
            $resolvedBaseSha = $baseSha
            $resolvedHeadSha = $headSha
            Add-Reason "push_range:$baseSha..$headSha"
            $diff = Get-DiffPaths $baseSha $headSha
            if (-not $diff.Ok) {
              $fullScope = $true
              Add-FallbackReason $diff.Reason
            } else {
              foreach ($item in $diff.Files) { [void]$detectedFiles.Add((Normalize-RepoPath $item)) }
            }
          }
        }
        if ($fullScope) { Add-Reason "fallback_full:push" }
      }
      "pull_request" {
        $mode = "pull_request"
        # PR 必须以明确的 base/head 求 merge base，不能拿 base 分支头直接相比。
        if ([string]::IsNullOrWhiteSpace($BaseRef)) {
          $fullScope = $true
          Add-FallbackReason "base_ref_missing"
        } else {
          $baseSha = Get-CommitSha $BaseRef
          $headSha = Get-CommitSha $HeadRef
          if ($null -eq $baseSha) {
            $fullScope = $true
            Add-FallbackReason "base_ref_unresolvable:$BaseRef"
          } elseif ($null -eq $headSha) {
            $fullScope = $true
            Add-FallbackReason "head_ref_unresolvable:$HeadRef"
          } else {
            $resolvedBaseRef = $BaseRef
            $headRefValue = $HeadRef
            $resolvedBaseSha = $baseSha
            $resolvedHeadSha = $headSha
            $mergeBaseSha = Get-MergeBase $baseSha $headSha
            if ([string]::IsNullOrWhiteSpace($mergeBaseSha)) {
              $fullScope = $true
              Add-FallbackReason "merge_base_unresolvable:$baseSha..$headSha"
            } else {
              Add-Reason "pull_request_merge_base:$mergeBaseSha"
              $diff = Get-DiffPaths $mergeBaseSha $headSha
              if (-not $diff.Ok) {
                $fullScope = $true
                Add-FallbackReason $diff.Reason
              } else {
                foreach ($item in $diff.Files) { [void]$detectedFiles.Add((Normalize-RepoPath $item)) }
              }
            }
          }
        }
        if ($fullScope) { Add-Reason "fallback_full:pull_request" }
      }
      default {
        # local / range：基准可为显式 ref 或自动探测 origin/main -> main -> HEAD~1。
        $mode = if ($effectiveEvent -eq "local") { "local" } else { "range" }
        $baseInfo = Resolve-AutoBaseRef $BaseRef $HeadRef
        $includeWorktree = [bool]$IncludeWorkingTree
        if ($null -eq $baseInfo.Ref) {
          if ($includeWorktree) {
            Add-Reason "worktree_only:$($baseInfo.Reason)"
            Add-FallbackReason $baseInfo.Reason
          } else {
            $fullScope = $true
            Add-FallbackReason $baseInfo.Reason
            Add-Reason "fallback_full:$($baseInfo.Reason)"
          }
        } else {
          $resolvedBaseRef = $baseInfo.Ref
          Add-Reason $baseInfo.Reason
        }

        if (-not $fullScope) {
          if (-not [string]::IsNullOrWhiteSpace($resolvedBaseRef)) {
            $baseSha = Get-CommitSha $resolvedBaseRef
            $headSha = Get-CommitSha $HeadRef
            $resolvedBaseSha = [string]$baseSha
            $resolvedHeadSha = [string]$headSha
            $mergeSource = if ($null -ne $baseSha -and $null -ne $headSha) { Get-MergeBase $baseSha $headSha } else { $null }
            if ([string]::IsNullOrWhiteSpace($mergeSource)) {
              $fullScope = $true
              Add-FallbackReason "merge_base_unresolvable:$resolvedBaseRef..$HeadRef"
              Add-Reason "fallback_full:merge_base_unresolvable"
            } else {
              $mergeBaseSha = $mergeSource
              $diff = Get-DiffPaths $mergeBaseSha $HeadRef
              if (-not $diff.Ok) {
                $fullScope = $true
                Add-FallbackReason $diff.Reason
                Add-Reason "fallback_full:git_diff_failed"
              } else {
                foreach ($item in $diff.Files) { [void]$detectedFiles.Add((Normalize-RepoPath $item)) }
              }
            }
          }

          if (-not $fullScope -and $includeWorktree) {
            $worktree = Get-WorktreePaths
            if (-not $worktree.Ok) {
              $fullScope = $true
              Add-FallbackReason $worktree.Reason
              Add-Reason "fallback_full:git_diff_failed"
            } else {
              foreach ($item in $worktree.Files) { [void]$detectedFiles.Add((Normalize-RepoPath $item)) }
            }
          }
        }
      }
    }
  }
}

$changedFiles = @($detectedFiles)
if ([string]::IsNullOrEmpty($resolvedHeadSha)) { $resolvedHeadSha = $checkoutSha }

# ---------------------------------------------------------------------------
# 二、按路径归因，必要时按需读取 Cargo 元数据
# ---------------------------------------------------------------------------

$runRust = $false
$runMobileWeb = $false
$runPaseoOrchestration = $false
$runCheckScopeSelfTest = $false
$rustScope = "none"
$seedPackages = [System.Collections.Generic.SortedSet[string]]::new([StringComparer]::Ordinal)

$rustCandidate = $false
if (-not $fullScope) {
  foreach ($path in $changedFiles) {
    if ($path -in $InfraFullPaths -or $path -like "rust-toolchain*") {
      $fullScope = $true
      Add-Reason "infra_full:$path"
      break
    }
    if ($path -eq "Cargo.toml" -or $path -eq "Cargo.lock" -or $path -like ".cargo/*") {
      $rustCandidate = $true
      continue
    }
    # crates/ 下的任何深度都可能是工作区成员（含嵌套通配成员）。
    if ($path -like "crates/*") { $rustCandidate = $true }
  }
}

$graph = $null
if ($rustCandidate -or $fullScope) {
  # 只有确实需要 Rust 影响范围时才启动 cargo；纯文档/AOCI/PWA 路径不读元数据。
  $graph = Get-CargoWorkspaceGraph
  if (-not $graph.Ok) {
    Add-FallbackReason $graph.Reason
    if (-not $fullScope) {
      $fullScope = $true
      Add-Reason "fallback_full:cargo_metadata"
    }
  }
}

if (-not $fullScope) {
  foreach ($path in $changedFiles) {
    if (
      $path -eq "Cargo.toml" -or
      $path -eq "Cargo.lock" -or
      $path -like ".cargo/*"
    ) {
      $runRust = $true
      $rustScope = "workspace"
      Add-Reason "rust_workspace:$path"
      continue
    }

    $memberDir = if ($null -ne $graph -and $graph.Ok) { Find-MemberDirForPath $graph.DirToPkg $path } else { $null }
    if ($null -ne $memberDir) {
      $runRust = $true
      [void]$seedPackages.Add($graph.DirToPkg[$memberDir])
      if ($rustScope -eq "none") { $rustScope = "crates" }
      Add-Reason "crate:$($graph.DirToPkg[$memberDir])"
      continue
    }

    if ($path -like "crates/*") {
      # crates/ 下但无法匹配到当前图里的成员：可能是已删除或已移出的历史 Crate，
      # 无法可靠归因时保守回退全量，并保留可机器分组的原因。
      $runRust = $true
      $rustScope = "workspace"
      $fullScope = $true
      $unmatchedDir = ($path.Split('/') | Select-Object -First 2) -join '/'
      Add-FallbackReason "unknown_crate_dir:$unmatchedDir"
      Add-Reason "fallback_full:unknown_crate_dir"
      continue
    }

    if (
      $path -like "apps/mobile-web/*" -or
      $path -like "design-system/*" -or
      $path -eq "scripts/sync-design-tokens.mjs" -or
      $path -eq "scripts/verify-design-system.mjs"
    ) {
      $runMobileWeb = $true
      Add-Reason "mobile_web:$path"
      continue
    }

    if (
      $path -like "scripts/paseo-*.ps1" -or
      $path -eq "scripts/test-paseo-orchestration.ps1" -or
      $path -like ".agents/skills/paseo-orchestrator/*"
    ) {
      $runPaseoOrchestration = $true
      Add-Reason "paseo_orchestration:$path"
      continue
    }

    if (
      $path -in $DocsOrGovernancePaths -or
      $path -like "docs/*" -or
      $path -like ".aoci/*" -or
      $path -like ".githooks/*" -or
      $path -like ".github/ISSUE_TEMPLATE/*" -or
      $path -eq ".github/PULL_REQUEST_TEMPLATE.md" -or
      $path -like ".agents/local/*"
    ) {
      Add-Reason "docs_or_governance:$path"
      continue
    }

    # 无法归因的路径安全回退全量检查，避免遗漏
    $fullScope = $true
    Add-FallbackReason "unattributed_path:$path"
    Add-Reason "fallback_full:unknown_path"
    break
  }
}

if ($fullScope) {
  $runRust = $true
  $rustScope = "workspace"
  $runMobileWeb = $true
  $runPaseoOrchestration = $true
  $runCheckScopeSelfTest = $true
  Add-Reason "full_scope"
} elseif ($runRust -and $changedFiles.Count -gt 0) {
  # 门禁脚本自身变更必须跑作用域自测，即使没有回退全量。
  foreach ($path in $changedFiles) {
    if (@("scripts/detect-check-scope.ps1", "scripts/test-check-scope.ps1", "scripts/check-all.ps1", ".github/workflows/ci.yml") -contains $path) {
      $runCheckScopeSelfTest = $true
      Add-Reason "check_scope_self_test:$path"
    }
  }
}

# ---------------------------------------------------------------------------
# 三、反向闭包、平台、必跑 job 与测试配置
# ---------------------------------------------------------------------------

$rustPackages = @()
if ($runRust) {
  if ($rustScope -eq "workspace") {
    $rustPackages = if ($null -ne $graph -and $graph.Ok) { @($graph.Packages) } else { @() }
  } elseif ($seedPackages.Count -gt 0 -and $null -ne $graph -and $graph.Ok) {
    $rustPackages = Expand-ReverseDependencies @($seedPackages) $graph.ReverseDeps
    if ($graph.Packages.Count -gt 0 -and $rustPackages.Count -ge $graph.Packages.Count) {
      $rustScope = "workspace"
    }
  } else {
    $rustScope = "workspace"
    $rustPackages = if ($null -ne $graph -and $graph.Ok) { @($graph.Packages) } else { @() }
  }
}

$affectedPackages = @($rustPackages)
$rustPackageArgs = if ($rustScope -eq "crates" -and $rustPackages.Count -gt 0) {
  ($rustPackages | ForEach-Object { "-p $_" }) -join " "
} else {
  ""
}

$requiredPlatforms = [System.Collections.Generic.SortedSet[string]]::new([StringComparer]::Ordinal)
if ($runRust) {
  [void]$requiredPlatforms.Add($RustBaselinePlatform)
  $needsCrossHost = $rustScope -eq "workspace"
  if (-not $needsCrossHost) {
    foreach ($pkg in $affectedPackages) {
      if ($CrossHostConsumerPackages -contains $pkg) { $needsCrossHost = $true; break }
    }
  }
  if ($needsCrossHost) {
    [void]$requiredPlatforms.Add("linux")
    [void]$requiredPlatforms.Add("macos-arm64")
    Add-Reason "cross_host_rust_scope"
  }
}
if ($runMobileWeb) {
  # PWA 构建在 Linux runner 上执行。
  [void]$requiredPlatforms.Add("linux")
}
$releaseTouched = $changedFiles -contains ".github/workflows/release.yml"

# Intel 平台只由显式入口或架构敏感变化请求；不能从“改了 release.yml”这类文件修改、
# 也不能从通用 -Full 推导出当前应运行 Intel。Intel 是 Rust 矩阵实例，rust=false 时无意义。
if ($runRust) {
  if (-not $intelRequested) {
    if ($releaseEntryRequested) {
      $intelRequested = $true
      Add-Reason "intel_platform:release_entry"
    } elseif ($ArchSensitivePathPrefixes.Count -gt 0) {
      foreach ($path in $changedFiles) {
        $sensitive = @($ArchSensitivePathPrefixes | Where-Object {
            $path -eq $_ -or $path.StartsWith($_, [StringComparison]::Ordinal)
          })
        if ($sensitive.Count -gt 0) {
          $intelRequested = $true
          Add-Reason "intel_platform:arch_sensitive:$path"
          break
        }
      }
    }
  } elseif ($intelRequested) {
    Add-Reason "intel_platform:explicit_request"
  }
  if ($intelRequested) { [void]$requiredPlatforms.Add("macos-intel") }
} elseif ($intelRequested) {
  # 显式请求但未触及 Rust：不产生无意义的 Intel 实例，只留可追溯原因。
  $intelRequested = $false
  Add-Reason "intel_not_applicable:rust_disabled"
}

# Rust 矩阵实例集：只有 rust=true 才有矩阵，且严格按平台目录键序输出，
# 因此 requiredPlatforms 里由 PWA-only 引入的 linux 不会凭空开启 Rust 矩阵。
$rustPlatforms = [System.Collections.Generic.List[string]]::new()
if ($runRust) {
  foreach ($platformId in $PlatformOrder) {
    if ($requiredPlatforms.Contains($platformId)) { $rustPlatforms.Add($platformId) }
  }
}

$requiredJobs = [System.Collections.Generic.List[string]]::new()
$requiredJobs.Add("detect-changes")
$requiredJobs.Add("governance-and-privacy")
# 纯 Rust 格式检查在 Ubuntu 上只跑一次，不随平台矩阵复制。
if ($runRust) { $requiredJobs.Add("rust-format") }
if ($runRust) { $requiredJobs.Add("rust-check") }
if ($runMobileWeb) { $requiredJobs.Add("mobile-web-check") }
$requiredJobs.Add("ci-gate")

$testProfiles = [System.Collections.Generic.List[string]]::new()
$testProfiles.Add("governance-and-privacy")
if ($runRust) {
  $testProfiles.Add("cargo-fmt")
  $testProfiles.Add("cargo-clippy")
  $testProfiles.Add("cargo-test")
}
if ($runMobileWeb) {
  $testProfiles.Add("design-tokens")
  $testProfiles.Add("design-system")
  $testProfiles.Add("mobile-web-build")
}
if ($runPaseoOrchestration) { $testProfiles.Add("paseo-orchestration") }
if ($runCheckScopeSelfTest) { $testProfiles.Add("check-scope-self-test") }
if ($releaseTouched) { $testProfiles.Add("release-contract") }
if ($fullScope) { $testProfiles.Add("ci-gate-plan-verify") }

$result = [ordered]@{
  # 结构化计划契约：schemaVersion 决定路径保真强度，mode 决定基准语义。
  schemaVersion        = $PlanSchemaVersion
  mode                 = $mode
  baseRef              = $resolvedBaseRef
  headRef              = $headRefValue
  mergeBase            = $mergeBaseSha
  baseSha              = $resolvedBaseSha
  headSha              = $resolvedHeadSha
  checkoutSha          = $checkoutSha
  changedFiles         = @($changedFiles)
  seedPackages         = @($seedPackages)
  affectedPackages     = @($affectedPackages)
  requiredPlatforms    = @($requiredPlatforms)
  # Rust 矩阵实例集：CI Gate 逐个平台要求“各有且仅有一份已通过证据”。
  platforms            = @($rustPlatforms)
  # 矩阵展开表：id/runner/os/arch 全部由平台目录派生，工作流只消费不自造。
  # rust=false 时保留基线单实例，避免 GitHub 因空矩阵拒绝展开作业。
  platformMatrix       = @(
    if ($rustPlatforms.Count -gt 0) {
      foreach ($platformId in $rustPlatforms) { $PlatformCatalog[$platformId] }
    } else {
      $PlatformCatalog["windows"]
    }
  )
  requiredJobs         = @($requiredJobs)
  testProfiles         = @($testProfiles)
  fallbackReasons      = @($fallbackReasons)

  # 既有调用契约字段：语义不变，保证 check-all、CI Gate 与旧消费者继续可用。
  full                 = $fullScope
  governance           = $true
  rust                 = $runRust
  rustScope            = $rustScope
  rustPackages         = @($rustPackages)
  rustPackageArgs      = $rustPackageArgs
  mobileWeb            = $runMobileWeb
  paseoOrchestration   = $runPaseoOrchestration
  checkScopeSelfTest   = $runCheckScopeSelfTest
  reasons              = @($reasons)
}

if ($Json) {
  $result | ConvertTo-Json -Depth 8
} else {
  [pscustomobject]$result
}
exit 0
