param(
  [string]$BaseRef = "",
  [string]$HeadRef = "HEAD",
  [string[]]$ChangedFiles = @(),
  [switch]$IncludeWorkingTree,
  [switch]$Full,
  [switch]$Json
)

$ErrorActionPreference = "Stop"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$OutputEncoding = [System.Text.Encoding]::UTF8

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path

function Normalize-RepoPath([string]$PathValue) {
  if ([string]::IsNullOrWhiteSpace($PathValue)) { return "" }
  $normalized = $PathValue.Trim().Replace('\', '/')
  while ($normalized.StartsWith('./')) {
    $normalized = $normalized.Substring(2)
  }
  return $normalized
}

function Get-WorkspaceCrateGraph([string]$RootDir) {
  $rootCargo = Join-Path $RootDir "Cargo.toml"
  $dirToPkg = @{}
  $pkgDeps = @{}
  $revDeps = @{}

  if (-not (Test-Path -LiteralPath $rootCargo -PathType Leaf)) {
    return [pscustomobject]@{
      DirToPkg = $dirToPkg
      RevDeps = $revDeps
    }
  }

  $rootText = Get-Content -LiteralPath $rootCargo -Raw -Encoding utf8
  $membersMatch = [regex]::Match($rootText, '(?s)members\s*=\s*\[(.*?)\]')
  $memberDirs = @()
  if ($membersMatch.Success) {
    foreach ($m in [regex]::Matches($membersMatch.Groups[1].Value, '"([^"]+)"')) {
      $memberDirs += (Normalize-RepoPath $m.Groups[1].Value)
    }
  }

  foreach ($memberDir in $memberDirs) {
    $cargoFile = Join-Path $RootDir ($memberDir.Replace('/', [IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar + "Cargo.toml")
    if (-not (Test-Path -LiteralPath $cargoFile -PathType Leaf)) { continue }
    $lines = Get-Content -LiteralPath $cargoFile -Encoding utf8
    $section = ""
    $pkgName = ""
    $deps = [System.Collections.Generic.List[string]]::new()
    foreach ($rawLine in $lines) {
      $line = ($rawLine -split '#', 2)[0].Trim()
      if ([string]::IsNullOrWhiteSpace($line)) { continue }
      if ($line -match '^\[\[?([^\]]+)\]\]?$') {
        $section = $Matches[1].Trim()
        if ($section -match '^(?:dependencies|dev-dependencies|build-dependencies)\.([A-Za-z0-9_-]+)$') {
          $deps.Add($Matches[1].Trim())
        }
        continue
      }
      if ($section -eq "package" -and $line -match '^name\s*=\s*"([^"]+)"') {
        $pkgName = $Matches[1].Trim()
      } elseif ($section -in @("dependencies", "dev-dependencies", "build-dependencies") -and $line -match '^([A-Za-z0-9_-]+)(?:\.[A-Za-z0-9_-]+)*\s*=') {
        $deps.Add($Matches[1].Trim())
      }
    }
    if (-not [string]::IsNullOrWhiteSpace($pkgName)) {
      $dirToPkg[$memberDir] = $pkgName
      $pkgDeps[$pkgName] = $deps
      if (-not $revDeps.ContainsKey($pkgName)) {
        $revDeps[$pkgName] = [System.Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
      }
    }
  }

  foreach ($pkg in $pkgDeps.Keys) {
    foreach ($dep in $pkgDeps[$pkg]) {
      if ($revDeps.ContainsKey($dep)) {
        [void]$revDeps[$dep].Add($pkg)
      }
    }
  }

  return [pscustomobject]@{
    DirToPkg = $dirToPkg
    RevDeps = $revDeps
  }
}

function Expand-ReverseDependencies([string[]]$SeedPackages, $RevDeps) {
  $visited = [System.Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
  $queue = [System.Collections.Generic.Queue[string]]::new()
  foreach ($pkg in $SeedPackages) {
    if ($visited.Add($pkg)) {
      $queue.Enqueue($pkg)
    }
  }
  while ($queue.Count -gt 0) {
    $current = $queue.Dequeue()
    if ($RevDeps.ContainsKey($current)) {
      foreach ($dependent in $RevDeps[$current]) {
        if ($visited.Add($dependent)) {
          $queue.Enqueue($dependent)
        }
      }
    }
  }
  return @($visited | Sort-Object)
}

function Resolve-BaseRef([string]$RootDir, [string]$RequestedBase, [string]$TargetHead) {
  if (-not [string]::IsNullOrWhiteSpace($RequestedBase)) {
    if ($RequestedBase -match '^0+$') {
      return [pscustomobject]@{ Ref = $null; Reason = "base_ref_all_zeros" }
    }
    & git -C $RootDir rev-parse --verify --quiet "$RequestedBase^{commit}" 2>$null | Out-Null
    if ($LASTEXITCODE -eq 0) {
      return [pscustomobject]@{ Ref = $RequestedBase; Reason = "explicit_base" }
    }
    return [pscustomobject]@{ Ref = $null; Reason = "base_ref_unresolvable:$RequestedBase" }
  }

  foreach ($candidate in @("origin/main", "main")) {
    & git -C $RootDir rev-parse --verify --quiet "$candidate^{commit}" 2>$null | Out-Null
    if ($LASTEXITCODE -eq 0) {
      return [pscustomobject]@{ Ref = $candidate; Reason = "auto_base:$candidate" }
    }
  }

  & git -C $RootDir rev-parse --verify --quiet "$TargetHead~1^{commit}" 2>$null | Out-Null
  if ($LASTEXITCODE -eq 0) {
    return [pscustomobject]@{ Ref = "$TargetHead~1"; Reason = "auto_base:head_parent" }
  }

  return [pscustomobject]@{ Ref = $null; Reason = "no_base_ref_available" }
}

function Get-GitChangedFiles([string]$RootDir, [string]$ResolvedBase, [string]$TargetHead, [bool]$IncludeWorktree) {
  $files = [System.Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
  $diffOk = $true

  if (-not [string]::IsNullOrWhiteSpace($ResolvedBase)) {
    $committed = & git -C $RootDir -c core.quotepath=false diff --name-only "$ResolvedBase...$TargetHead" 2>$null
    if ($LASTEXITCODE -ne 0) {
      $committed = & git -C $RootDir -c core.quotepath=false diff --name-only "$ResolvedBase..$TargetHead" 2>$null
      if ($LASTEXITCODE -ne 0) {
        $diffOk = $false
      }
    }
    if ($diffOk) {
      foreach ($item in $committed) {
        $norm = Normalize-RepoPath $item
        if (-not [string]::IsNullOrWhiteSpace($norm)) { [void]$files.Add($norm) }
      }
    }
  }

  if ($IncludeWorktree) {
    foreach ($wtArgs in @(
      @("-c", "core.quotepath=false", "diff", "--name-only"),
      @("-c", "core.quotepath=false", "diff", "--cached", "--name-only"),
      @("-c", "core.quotepath=false", "ls-files", "--others", "--exclude-standard")
    )) {
      $wtFiles = & git -C $RootDir @wtArgs 2>$null
      if ($LASTEXITCODE -ne 0) {
        $diffOk = $false
        break
      }
      foreach ($item in $wtFiles) {
        $norm = Normalize-RepoPath $item
        if (-not [string]::IsNullOrWhiteSpace($norm)) { [void]$files.Add($norm) }
      }
    }
  }

  return [pscustomobject]@{
    Ok = $diffOk
    Files = @($files | Sort-Object)
  }
}

$graph = Get-WorkspaceCrateGraph $repoRoot
$reasons = [System.Collections.Generic.List[string]]::new()
$forceFull = [bool]$Full
$resolvedBaseRef = ""
$detectedFiles = @()

if ($ChangedFiles.Count -gt 0) {
  $detectedFiles = @(
    $ChangedFiles |
      ForEach-Object { $_ -split ',' } |
      ForEach-Object { Normalize-RepoPath $_ } |
      Where-Object { -not [string]::IsNullOrWhiteSpace($_) } |
      Sort-Object -Unique
  )
  $reasons.Add("explicit_changed_files:$($detectedFiles.Count)")
} else {
  $baseInfo = Resolve-BaseRef $repoRoot $BaseRef $HeadRef
  if (-not $baseInfo.Ref) {
    if (-not $IncludeWorkingTree) {
      $forceFull = $true
      $reasons.Add("fallback_full:$($baseInfo.Reason)")
    } else {
      $reasons.Add("worktree_only:$($baseInfo.Reason)")
    }
  } else {
    $resolvedBaseRef = $baseInfo.Ref
    $reasons.Add($baseInfo.Reason)
  }

  if (-not $forceFull) {
    $gitResult = Get-GitChangedFiles $repoRoot $resolvedBaseRef $HeadRef ([bool]$IncludeWorkingTree)
    if (-not $gitResult.Ok) {
      $forceFull = $true
      $reasons.Add("fallback_full:git_diff_failed")
    } else {
      $detectedFiles = $gitResult.Files
    }
  }
}

$runRust = $false
$rustScope = "none"
$seedPackages = [System.Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
$runMobileWeb = $false
$runPaseoOrchestration = $false
$runCheckScopeSelfTest = $false

if (-not $forceFull) {
  foreach ($path in $detectedFiles) {
    if (
      $path -eq "mise.toml" -or
      $path -eq ".github/workflows/ci.yml" -or
      $path -eq "scripts/check-all.ps1" -or
      $path -eq "scripts/detect-check-scope.ps1" -or
      $path -eq "scripts/test-check-scope.ps1" -or
      $path -eq "scripts/verify-commits-and-privacy.ps1" -or
      $path -like "rust-toolchain*"
    ) {
      $forceFull = $true
      $reasons.Add("infra_full:$path")
      break
    }

    if ($path -in @("Cargo.toml", "Cargo.lock") -or $path -like ".cargo/*") {
      $runRust = $true
      $rustScope = "workspace"
      $reasons.Add("rust_workspace:$path")
      continue
    }

    if ($path -match '^(crates/[^/]+)/') {
      $crateDir = $Matches[1]
      $runRust = $true
      if ($graph.DirToPkg.ContainsKey($crateDir)) {
        [void]$seedPackages.Add($graph.DirToPkg[$crateDir])
        if ($rustScope -eq "none") { $rustScope = "crates" }
        $reasons.Add("crate:$($graph.DirToPkg[$crateDir])")
      } else {
        $rustScope = "workspace"
        $reasons.Add("unknown_crate_dir:$crateDir")
      }
      continue
    }

    if (
      $path -like "apps/mobile-web/*" -or
      $path -like "design-system/*" -or
      $path -eq "scripts/sync-design-tokens.mjs" -or
      $path -eq "scripts/verify-design-system.mjs"
    ) {
      $runMobileWeb = $true
      $reasons.Add("mobile_web:$path")
      continue
    }

    if (
      $path -like "scripts/paseo-*.ps1" -or
      $path -eq "scripts/test-paseo-orchestration.ps1" -or
      $path -like ".agents/skills/paseo-orchestrator/*"
    ) {
      $runPaseoOrchestration = $true
      $reasons.Add("paseo_orchestration:$path")
      continue
    }

    # 非代码/治理/认知文档与基础配置只需运行治理与隐私门禁
    if (
      $path -in @(
        "AGENTS.md", "CLAUDE.md", "README.md",
        "aoci.txt", "aoci.meta.txt", "aoci.code.txt", "aoci.database.txt",
        ".editorconfig", ".gitattributes", ".gitignore", ".gitleaks.toml",
        ".github/workflows/release.yml",
        "paseo.json", "scripts/mise-doctor.ps1", "scripts/setup-hooks.ps1", "scripts/setup-worktree.ps1"
      ) -or
      $path -like "docs/*" -or
      $path -like ".aoci/*" -or
      $path -like ".githooks/*" -or
      $path -like ".github/ISSUE_TEMPLATE/*" -or
      $path -eq ".github/PULL_REQUEST_TEMPLATE.md" -or
      $path -like ".agents/local/*"
    ) {
      $reasons.Add("docs_or_governance:$path")
      continue
    }

    # 未知新增路径安全回退全量检查，避免遗漏
    $forceFull = $true
    $reasons.Add("unknown_path_full:$path")
    break
  }
}

if ($forceFull) {
  $runRust = $true
  $rustScope = "workspace"
  $runMobileWeb = $true
  $runPaseoOrchestration = $true
  $runCheckScopeSelfTest = $true
}

$rustPackages = @()
if ($runRust) {
  if ($rustScope -eq "workspace") {
    $rustPackages = @($graph.DirToPkg.Values | Sort-Object -Unique)
  } elseif ($seedPackages.Count -gt 0) {
    $rustPackages = Expand-ReverseDependencies @($seedPackages) $graph.RevDeps
    if ($rustPackages.Count -ge $graph.DirToPkg.Count -and $graph.DirToPkg.Count -gt 0) {
      $rustScope = "workspace"
    }
  } else {
    $rustScope = "workspace"
    $rustPackages = @($graph.DirToPkg.Values | Sort-Object -Unique)
  }
}

$rustPackageArgs = if ($rustScope -eq "crates" -and $rustPackages.Count -gt 0) {
  ($rustPackages | ForEach-Object { "-p $_" }) -join " "
} else {
  ""
}

$result = [ordered]@{
  full = $forceFull
  baseRef = $resolvedBaseRef
  headRef = $HeadRef
  changedFiles = $detectedFiles
  governance = $true
  rust = $runRust
  rustScope = $rustScope
  rustPackages = $rustPackages
  rustPackageArgs = $rustPackageArgs
  mobileWeb = $runMobileWeb
  paseoOrchestration = $runPaseoOrchestration
  checkScopeSelfTest = $runCheckScopeSelfTest
  reasons = @($reasons)
}

if ($Json) {
  $result | ConvertTo-Json -Depth 5
} else {
  $result
}
exit 0
