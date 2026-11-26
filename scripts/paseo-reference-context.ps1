param(
  [string]$RepositoryRoot = (Split-Path -Parent $PSScriptRoot),
  [string]$ReferenceRoot,
  [string[]]$Names = @('paseo', 'codeg', 'padu'),
  [string]$AociExecutable,
  [string]$CodeGraphExecutable,
  [switch]$RequireReady
)

$ErrorActionPreference = 'Stop'
if (-not $ReferenceRoot) {
  $ReferenceRoot = Join-Path $RepositoryRoot '.refs'
}

function Find-Executable([string]$ExplicitPath, [string]$Name, [string]$Fallback) {
  if ($ExplicitPath) {
    return (Get-Command $ExplicitPath -ErrorAction Stop).Source
  }
  $command = Get-Command $Name -ErrorAction SilentlyContinue
  if ($command) { return $command.Source }
  if ($Fallback -and (Test-Path -LiteralPath $Fallback -PathType Leaf)) { return $Fallback }
  return $null
}

function Invoke-Query([string]$Executable, [string[]]$Arguments) {
  if (-not $Executable) { return @{ ok = $false; data = $null; reason = '工具不可用' } }
  try {
    $output = & $Executable @Arguments 2>&1
    if ($LASTEXITCODE -ne 0) {
      return @{ ok = $false; data = $null; reason = "查询失败，退出码 $LASTEXITCODE" }
    }
    $data = ($output | Out-String) | ConvertFrom-Json -ErrorAction Stop
    return @{ ok = $true; data = $data; reason = $null }
  } catch {
    return @{ ok = $false; data = $null; reason = '查询未返回可解析的 JSON' }
  }
}

function Read-Asset([string]$Root, [string]$RelativePath) {
  if ([IO.Path]::IsPathRooted($RelativePath)) { throw '参考索引声明必须使用仓库相对路径。' }
  $path = [IO.Path]::GetFullPath((Join-Path $Root $RelativePath))
  $relative = [IO.Path]::GetRelativePath($Root, $path)
  if ($relative -eq '..' -or $relative.StartsWith("..$([IO.Path]::DirectorySeparatorChar)")) {
    throw '参考索引声明越出参考仓库。'
  }
  if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { return $null }
  $ancestor = $path
  while ($ancestor -ne $Root) {
    $item = Get-Item -Force -LiteralPath $ancestor
    if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 -or $item.LinkType) {
      throw '参考索引资产不能经链接跳出其声明路径。'
    }
    $ancestor = Split-Path -Parent $ancestor
  }
  $resolved = (Resolve-Path -LiteralPath $path).Path
  $file = Get-Item -LiteralPath $resolved
  return [ordered]@{ path = $resolved; bytes = $file.Length; sha256 = (Get-FileHash -LiteralPath $resolved -Algorithm SHA256).Hash.ToLowerInvariant() }
}

$aoci = Find-Executable $AociExecutable 'aoci' 'D:/Programs/AOCI/aoci.exe'
$codegraph = Find-Executable $CodeGraphExecutable 'codegraph' $null
$references = @()
foreach ($name in ($Names -split ',')) {
  if ($name -notmatch '^[a-zA-Z0-9][a-zA-Z0-9_-]*$') { throw '参考项目名称必须是单层便携目录名。' }
  $issues = [Collections.Generic.List[string]]::new()
  $root = [IO.Path]::GetFullPath((Join-Path $ReferenceRoot $name))
  $head = $null
  $assets = @()
  $aociStatus = @{ ok = $false; data = $null }
  $graphStatus = @{ ok = $false; data = $null }
  $graphPath = Join-Path $root '.codegraph'
  if (-not (Test-Path -LiteralPath $root -PathType Container)) {
    $issues.Add('参考仓库不存在或不可访问')
  } else {
    $root = (Resolve-Path -LiteralPath $root).Path
    $gitRoot = & git -C $root rev-parse --show-toplevel 2>$null
    if ($LASTEXITCODE -ne 0 -or -not $gitRoot -or (Resolve-Path -LiteralPath $gitRoot).Path -ne $root) {
      $issues.Add('参考路径不是独立 Git 仓库根，不能复用宿主仓库身份')
    } else {
      $head = & git -C $root rev-parse HEAD 2>$null
      if ($LASTEXITCODE -ne 0) { $head = $null; $issues.Add('参考 HEAD 不可用') }
      $indexPath = 'aoci.txt'
      $configPath = Join-Path $root '.aoci/config.json'
      try {
        if (Test-Path -LiteralPath $configPath -PathType Leaf) {
          $config = Get-Content -Raw -LiteralPath $configPath | ConvertFrom-Json
          if ($config.index_path) { $indexPath = [string]$config.index_path }
        }
        $asset = Read-Asset $root $indexPath
        if ($asset) {
          $assets += $asset
          foreach ($line in Get-Content -LiteralPath $asset.path) {
            if ($line -notmatch '\bstate=disabled\b' -and $line -match '^#Volume:.*\bpath=(\S+)') {
              $volume = Read-Asset $root $Matches[1]
              if ($volume) { $assets += $volume } else { $issues.Add('Root 声明的参考 Volume 缺失') }
            }
          }
        } else { $issues.Add('既有 AOCI 索引缺失') }
      } catch { $issues.Add('AOCI 资产或配置不可解析，禁止猜测索引布局') }
      if ($assets.Count -gt 0) {
        $aociStatus = Invoke-Query $aoci @('--repo', $root, 'status', '--json')
        if (-not $aociStatus.ok) { $issues.Add("AOCI：$($aociStatus.reason)") }
        elseif ($aociStatus.data.root -ne $root) { $issues.Add('AOCI 返回的仓库根不匹配') }
        elseif (-not $aociStatus.data.baseline_exists -or $aociStatus.data.index_entries -le 0) { $issues.Add('AOCI 缺少既有 Baseline 或有效条目') }
      }
      if (Test-Path -LiteralPath (Join-Path $graphPath 'codegraph.db') -PathType Leaf) {
        $graphStatus = Invoke-Query $codegraph @('status', '--json', $root)
        if (-not $graphStatus.ok) { $issues.Add("CodeGraph：$($graphStatus.reason)") }
        elseif (-not $graphStatus.data.initialized -or $graphStatus.data.projectPath -ne $root) { $issues.Add('CodeGraph 未初始化或绑定了其他仓库') }
        elseif ($graphStatus.data.worktreeMismatch -or $graphStatus.data.rebuildInProgress -or $graphStatus.data.index.state -ne 'complete' -or $graphStatus.data.index.reindexRecommended -or $graphStatus.data.index.filesNeedingReindex -gt 0 -or $graphStatus.data.index.filesWithParseErrors -gt 0 -or ($graphStatus.data.pendingChanges.added + $graphStatus.data.pendingChanges.modified + $graphStatus.data.pendingChanges.removed) -gt 0) {
          $issues.Add('CodeGraph 存在漂移、解析缺口或未完成状态，查询需注明限制')
        }
      } else { $issues.Add('既有 CodeGraph 数据库缺失') }
    }
  }
  $references += [ordered]@{
    name = $name; root = $root; head = $head; ready = ($issues.Count -eq 0); issues = @($issues.ToArray())
    aoci = [ordered]@{
      executable = $aoci; assets = $assets; statusChecked = $aociStatus.ok
      entryCount = $aociStatus.data.index_entries; freshnessVerified = $false
      mcpArguments = @('--repo', $root, 'mcp')
    }
    codegraph = [ordered]@{
      executable = $codegraph; indexPath = $graphPath; statusChecked = $graphStatus.ok
      nodeCount = $graphStatus.data.nodeCount; edgeCount = $graphStatus.data.edgeCount
      exploreArguments = @('explore', '--path', $root); nodeArguments = @('node', '--path', $root)
    }
    usage = '复用此参考根的既有索引；AOCI 按独立 stdio MCP 实时合同读取，CodeGraph 显式 --path 查询。不得复制、初始化、重建、同步或维护参考索引。'
    fallback = '报告缺失、漂移或访问失败，再在此参考根用 rg 和源码调查；不声称完整认知校验或图查询成功。Soxu 工作树治理另列。'
  }
}
[ordered]@{ version = 'paseo-reference-context/v1'; observedAt = [DateTimeOffset]::Now.ToString('o'); readinessMeaning = '仅表示入口和既有资产可查询且图状态无已知缺口，不证明 AOCI 新鲜度或模型认知已校验。'; references = $references } | ConvertTo-Json -Depth 10
if ($RequireReady -and @($references | Where-Object { -not $_.ready }).Count -gt 0) { exit 1 }
