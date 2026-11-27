param(
  [string]$Path = "",
  [string]$Ref = "",
  [string]$RepoRoot = "",
  [switch]$Json,
  [string]$GitHubOutput = "",
  [switch]$AssertInstalled,
  [string[]]$AssertTools = @("rust", "node", "pnpm")
)

# CI-D：mise.toml 是工具链版本的唯一真相源。
# 本脚本只做一件事：从受信任来源的 mise.toml 严格提取精确的 rust / node / pnpm 版本，
# 供 GitHub Actions 与发布仓消费。任何不确定形态都非零退出，绝不回退内置默认值：
# 一旦允许回退，"锁定的版本"就变成脚本里的常量，mise.toml 与 CI 会静默漂移。
#
# 严格性契约（每条都有 test-read-mise-versions.ps1 的负例防守）：
#   - [tools] 段必须存在，且恰好声明 rust / node / pnpm 三项；
#   - 版本必须是精确 semver 的 major.minor.patch，范围、浮动、复合、标签一律拒绝；
#   - 同一工具重复声明（内联表与 [tools.<name>] 子表并存）视为重复声明；
#   - 未知工具与未知字段一律拒绝，扩展必须显式改本脚本而不是被静默忽略；
#   - 输出只含版本、字段摘要与 ref 名，绝不写入本机绝对路径、环境变量或令牌。

$ErrorActionPreference = "Stop"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$OutputEncoding = [System.Text.Encoding]::UTF8

$RequiredTools = @("rust", "node", "pnpm")
$VersionPattern = '^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$'
$ComponentsPattern = '^[A-Za-z0-9_-]+(,[A-Za-z0-9_-]+)*$'
$InstalledMap = [ordered]@{ rust = "rustc"; node = "node"; pnpm = "pnpm" }

$violations = [System.Collections.Generic.List[string]]::new()

function Add-Violation([string]$Code, [string]$Detail = "") {
  if ([string]::IsNullOrEmpty($Detail)) {
    $violations.Add($Code)
  } else {
    $violations.Add(("{0}:{1}" -f $Code, $Detail))
  }
}

function Write-ViolationsAndExit {
  foreach ($violation in $violations) {
    [Console]::Error.WriteLine(("::error::mise-version-contract:{0}" -f $violation))
  }
  exit 1
}

function Get-RepoRoot {
  if (-not [string]::IsNullOrWhiteSpace($RepoRoot)) {
    return (Resolve-Path -LiteralPath $RepoRoot).Path
  }
  return (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
}

# 只剥离引号外的 # 注释；引号内与转义一律原样保留，避免把 "1.99.1#x" 这种坏值洗成合法值。
function Remove-TomlComment([string]$Line) {
  $builder = [System.Text.StringBuilder]::new()
  $inBasic = $false
  $inLiteral = $false
  for ($i = 0; $i -lt $Line.Length; $i++) {
    $ch = $Line[$i]
    if ($inBasic) {
      if ($ch -eq '\') {
        [void]$builder.Append($ch)
        if ($i + 1 -lt $Line.Length) {
          $i++
          [void]$builder.Append($Line[$i])
        }
        continue
      }
      if ($ch -eq '"') { $inBasic = $false }
      [void]$builder.Append($ch)
      continue
    }
    if ($inLiteral) {
      if ($ch -eq "'") { $inLiteral = $false }
      [void]$builder.Append($ch)
      continue
    }
    if ($ch -eq '"') { $inBasic = $true; [void]$builder.Append($ch); continue }
    if ($ch -eq "'") { $inLiteral = $true; [void]$builder.Append($ch); continue }
    if ($ch -eq '#') { break }
    [void]$builder.Append($ch)
  }
  return $builder.ToString()
}

# 只在顶层逗号处切分内联表，避免 components = "rustfmt,clippy" 被切开。
function Split-TopLevelComma([string]$Value) {
  $parts = [System.Collections.Generic.List[string]]::new()
  $current = [System.Text.StringBuilder]::new()
  $inBasic = $false
  $inLiteral = $false
  for ($i = 0; $i -lt $Value.Length; $i++) {
    $ch = $Value[$i]
    if ($inBasic) {
      if ($ch -eq '"') { $inBasic = $false }
      [void]$current.Append($ch)
      continue
    }
    if ($inLiteral) {
      if ($ch -eq "'") { $inLiteral = $false }
      [void]$current.Append($ch)
      continue
    }
    if ($ch -eq '"') { $inBasic = $true; [void]$current.Append($ch); continue }
    if ($ch -eq "'") { $inLiteral = $true; [void]$current.Append($ch); continue }
    if ($ch -eq ',') {
      $parts.Add($current.ToString())
      [void]$current.Clear()
      continue
    }
    [void]$current.Append($ch)
  }
  $parts.Add($current.ToString())
  return $parts
}

function Read-TomlString([string]$Value) {
  $trimmed = $Value.Trim()
  if ($trimmed.Length -lt 2) { return $null }
  $first = $trimmed[0]
  $last = $trimmed[$trimmed.Length - 1]
  if ($first -eq '"' -and $last -eq '"') {
    $inner = $trimmed.Substring(1, $trimmed.Length - 2)
  } elseif ($first -eq "'" -and $last -eq "'") {
    $inner = $trimmed.Substring(1, $trimmed.Length - 2)
  } else {
    return $null
  }
  if ($inner.Contains('"') -or $inner.Contains("'") -or $inner.Contains('\')) { return $null }
  return $inner
}

function Test-ExactVersion([string]$Name, [string]$Version) {
  if ($Version -match '[,\s]') {
    Add-Violation "version_composite" $Name
    return
  }
  if ($Version -match $VersionPattern) { return }
  Add-Violation "version_not_exact" ("{0}={1}" -f $Name, $Version)
}

# 返回 @{ version = "..."; components = "..." }；不可判定时返回 $null 并记录违规。
function Parse-ToolDeclaration([string]$Name, [string]$RawValue, [int]$LineNumber) {
  $value = $RawValue.Trim()
  if ([string]::IsNullOrEmpty($value)) {
    Add-Violation "malformed_value" ("{0}@line{1}" -f $Name, $LineNumber)
    return $null
  }
  if ($value.StartsWith("{")) {
    if (-not $value.EndsWith("}")) {
      Add-Violation "malformed_inline_table" ("{0}@line{1}" -f $Name, $LineNumber)
      return $null
    }
    $inner = $value.Substring(1, $value.Length - 2)
    $result = @{ version = $null; components = $null }
    foreach ($part in (Split-TopLevelComma $inner)) {
      $pair = $part.Trim()
      if ([string]::IsNullOrEmpty($pair)) { continue }
      $eq = $pair.IndexOf('=')
      if ($eq -lt 1) {
        Add-Violation "malformed_inline_table" ("{0}@line{1}" -f $Name, $LineNumber)
        return $null
      }
      $key = $pair.Substring(0, $eq).Trim()
      $rawSub = $pair.Substring($eq + 1).Trim()
      if ($key -notin @("version", "components")) {
        Add-Violation "unexpected_field" ("{0}.{1}" -f $Name, $key)
        return $null
      }
      $sub = Read-TomlString $rawSub
      if ($null -eq $sub) {
        Add-Violation "malformed_value" ("{0}.{1}@line{2}" -f $Name, $key, $LineNumber)
        return $null
      }
      if ($null -ne $result[$key]) {
        Add-Violation "duplicate_field" ("{0}.{1}" -f $Name, $key)
        return $null
      }
      $result[$key] = $sub
    }
    if ([string]::IsNullOrEmpty($result["version"])) {
      Add-Violation "missing_field" ("{0}.version" -f $Name)
      return $null
    }
    if ($null -ne $result["components"] -and $result["components"] -notmatch $ComponentsPattern) {
      Add-Violation "malformed_components" $Name
      return $null
    }
    return @{ version = $result["version"]; components = $result["components"] }
  }

  $single = Read-TomlString $value
  if ($null -eq $single) {
    # 未闭合引号、裸数字、数组等一切非字符串形态都是不确定来源。
    Add-Violation "malformed_value" ("{0}@line{1}" -f $Name, $LineNumber)
    return $null
  }
  return @{ version = $single; components = $null }
}

function Get-DeclaredDocument {
  $root = Get-RepoRoot
  $sourceRef = ""
  $text = $null

  if (-not [string]::IsNullOrWhiteSpace($Ref)) {
    # ref 只允许安全字符且不得以 '-' 开头，避免被 git 当成选项解析。
    if ($Ref -notmatch '^[A-Za-z0-9][A-Za-z0-9._/@^~{}-]*$') {
      Add-Violation "source_ref_invalid" $Ref
      Write-ViolationsAndExit
    }
    $raw = & git -C $root show ("{0}:mise.toml" -f $Ref) 2>&1
    if ($LASTEXITCODE -ne 0) {
      Add-Violation "source_ref_unresolved" $Ref
      Write-ViolationsAndExit
    }
    $text = (@($raw) -join "`n")
    $sourceRef = $Ref
  } else {
    $target = if ([string]::IsNullOrWhiteSpace($Path)) { Join-Path $root "mise.toml" } else { $Path }
    if (-not (Test-Path -LiteralPath $target -PathType Leaf)) {
      Add-Violation "source_not_found" "mise.toml"
      Write-ViolationsAndExit
    }
    $text = Get-Content -LiteralPath $target -Raw -Encoding utf8
  }

  if ($null -eq $text) {
    Add-Violation "source_unreadable" "mise.toml"
    Write-ViolationsAndExit
  }

  # 跨平台换行：CRLF 与 LF 都接受；孤立 CR 是损坏文件，必须显式拒绝而不是让正则去兜。
  $normalized = $text.Replace("`r`n", "`n")
  if ($normalized.Contains("`r")) { Add-Violation "malformed_line_ending" "" }

  $sha256 = [System.Security.Cryptography.SHA256]::Create()
  try {
    $bytes = [System.Text.Encoding]::UTF8.GetBytes($normalized)
    $digest = ($sha256.ComputeHash($bytes) | ForEach-Object { $_.ToString("x2") }) -join ""
  } finally {
    $sha256.Dispose()
  }

  $tools = [ordered]@{}
  $toolTables = [ordered]@{}
  $section = ""
  $sawToolsSection = $false
  $lines = $normalized.Split("`n")

  for ($index = 0; $index -lt $lines.Count; $index++) {
    $lineNumber = $index + 1
    $line = (Remove-TomlComment $lines[$index]).Trim()
    if ([string]::IsNullOrEmpty($line)) { continue }

    if ($line -match '^\[\[(.*)\]\]$') {
      Add-Violation "malformed_array_table" ("line{0}" -f $lineNumber)
      continue
    }
    if ($line.StartsWith("[")) {
      if (-not $line.EndsWith("]")) {
        Add-Violation "malformed_table_header" ("line{0}" -f $lineNumber)
        continue
      }
      $name = $line.Substring(1, $line.Length - 2).Trim()
      if ($name -notmatch '^[A-Za-z0-9_.-]+$') {
        Add-Violation "malformed_table_header" ("line{0}" -f $lineNumber)
        continue
      }
      $section = $name
      if ($name -eq "tools") {
        $sawToolsSection = $true
      } elseif ($name.StartsWith("tools.")) {
        $sub = $name.Substring("tools.".Length)
        if ($sub -notin $RequiredTools) {
          Add-Violation "unexpected_tool" $sub
        } elseif ($toolTables.Contains($sub)) {
          Add-Violation "duplicate_tool" $sub
        } else {
          $toolTables[$sub] = @{ version = $null; components = $null }
        }
      }
      continue
    }

    $eq = $line.IndexOf('=')
    if ($eq -lt 1) {
      Add-Violation "malformed_line" ("line{0}" -f $lineNumber)
      continue
    }
    $key = $line.Substring(0, $eq).Trim()
    $rawValue = $line.Substring($eq + 1)
    if ($key -notmatch '^[A-Za-z0-9_.-]+$') {
      Add-Violation "malformed_key" ("line{0}" -f $lineNumber)
      continue
    }

    if ($section -eq "tools") {
      if ($key -notin $RequiredTools) {
        Add-Violation "unexpected_tool" $key
        continue
      }
      if ($tools.Contains($key)) {
        Add-Violation "duplicate_tool" $key
        continue
      }
      $parsed = Parse-ToolDeclaration $key $rawValue $lineNumber
      if ($null -ne $parsed) { $tools[$key] = $parsed }
      continue
    }

    if ($section.StartsWith("tools.")) {
      $sub = $section.Substring("tools.".Length)
      if ($sub -notin $RequiredTools) { continue }
      if (-not $toolTables.Contains($sub)) {
        Add-Violation "duplicate_tool" $sub
        continue
      }
      if ($key -notin @("version", "components")) {
        Add-Violation "unexpected_field" ("{0}.{1}" -f $sub, $key)
        continue
      }
      if ($null -ne $toolTables[$sub][$key]) {
        Add-Violation "duplicate_field" ("{0}.{1}" -f $sub, $key)
        continue
      }
      $value = Read-TomlString $rawValue
      if ($null -eq $value) {
        Add-Violation "malformed_value" ("{0}.{1}@line{2}" -f $sub, $key, $lineNumber)
        continue
      }
      $toolTables[$sub][$key] = $value
      continue
    }

    # 其他段落里出现同名工具键属于跨段重复声明，会让"唯一真相源"出现两个候选。
    if ($key -in $RequiredTools) {
      Add-Violation "duplicate_tool" ("{0}@{1}" -f $key, $section)
    }
  }

  if (-not $sawToolsSection) { Add-Violation "tools_section_missing" "" }

  foreach ($name in $RequiredTools) {
    if ($toolTables.Contains($name)) {
      $table = $toolTables[$name]
      if ([string]::IsNullOrEmpty($table["version"])) {
        Add-Violation "missing_field" ("{0}.version" -f $name)
        continue
      }
      if ($null -ne $table["components"] -and $table["components"] -notmatch $ComponentsPattern) {
        Add-Violation "malformed_components" $name
        continue
      }
      if ($tools.Contains($name)) {
        Add-Violation "duplicate_tool" $name
        continue
      }
      $tools[$name] = @{ version = $table["version"]; components = $table["components"] }
    }
    if (-not $tools.Contains($name)) {
      Add-Violation "missing_tool" $name
      continue
    }
    Test-ExactVersion $name ([string]$tools[$name].version)
  }

  if ($violations.Count -gt 0) { Write-ViolationsAndExit }

  return [ordered]@{
    schemaVersion  = 1
    source         = "mise.toml"
    sourceRef      = $sourceRef
    miseTomlSha256 = $digest
    rust           = [ordered]@{ version = $tools["rust"].version; components = $tools["rust"].components }
    node           = [ordered]@{ version = $tools["node"].version }
    pnpm           = [ordered]@{ version = $tools["pnpm"].version }
  }
}

function Get-InstalledVersion([string]$Tool) {
  $executable = $InstalledMap[$Tool]
  $command = Get-Command $executable -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
  if ($null -eq $command) { return $null }
  $raw = @(& $command.Source "--version" 2>&1)
  if ($LASTEXITCODE -ne 0) { return $null }
  $line = (@($raw | Where-Object { -not [string]::IsNullOrWhiteSpace([string]$_) }) | Select-Object -First 1)
  if ($null -eq $line) { return $null }
  $token = (([string]$line).Trim() -split '\s+')[0]
  if ($token -match '^v(\d.*)$') { return $Matches[1] }
  if ($Tool -eq "rustc" -or $Tool -eq "rust") {
    $fields = (([string]$line).Trim() -split '\s+')
    if ($fields.Count -ge 2) { return $fields[1] }
  }
  return $token
}

$document = Get-DeclaredDocument

if ($AssertInstalled) {
  # 支持 -AssertTools rust,node,pnpm（命令行 -File 传参不会拆成数组）与真实数组两种形式。
  $toolList = @($AssertTools | ForEach-Object { ([string]$_).Split(',') } | ForEach-Object { $_.Trim() } | Where-Object { -not [string]::IsNullOrEmpty($_) })
  $observed = [ordered]@{}
  foreach ($tool in $toolList) {
    if ($tool -notin $RequiredTools) {
      Add-Violation "assert_tool_unknown" $tool
      continue
    }
    $actual = Get-InstalledVersion $tool
    if ($null -eq $actual -or [string]::IsNullOrWhiteSpace($actual)) {
      Add-Violation "installed_tool_missing" $tool
      continue
    }
    $observed[$tool] = $actual
    $declared = [string]$document[$tool].version
    if ($actual -ne $declared) {
      Add-Violation "installed_tool_mismatch" ("{0}:declared={1},installed={2}" -f $tool, $declared, $actual)
    }
  }
  $document["installed"] = $observed
  if ($violations.Count -gt 0) { Write-ViolationsAndExit }
}

# 注意：不要命名成 $json —— PowerShell 变量不区分大小写，会直接撞上 -Json 开关参数。
$jsonText = ($document | ConvertTo-Json -Depth 6 -Compress)

if (-not [string]::IsNullOrWhiteSpace($GitHubOutput)) {
  $lines = @(
    ("rust_version={0}" -f $document.rust.version)
    ("node_version={0}" -f $document.node.version)
    ("pnpm_version={0}" -f $document.pnpm.version)
    ("rust_components={0}" -f $document.rust.components)
    ("mise_toml_sha256={0}" -f $document.miseTomlSha256)
    ("toolchain_source={0}" -f $(if ([string]::IsNullOrEmpty($document.sourceRef)) { "worktree" } else { $document.sourceRef }))
  )
  $existing = ""
  if (Test-Path -LiteralPath $GitHubOutput -PathType Leaf) {
    $existing = Get-Content -LiteralPath $GitHubOutput -Raw -Encoding utf8
    if ($null -eq $existing) { $existing = "" }
    if ($existing.Length -gt 0 -and -not $existing.EndsWith("`n")) { $existing += "`n" }
  }
  Set-Content -LiteralPath $GitHubOutput -Value ($existing + ($lines -join "`n") + "`n") -Encoding utf8 -NoNewline
}

Write-Output $jsonText
exit 0
