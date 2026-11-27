param(
  [string]$Reader = ""
)

# CI-D：read-mise-versions.ps1 的负向回归矩阵。
# 版本唯一真相源只有"严格"才有意义：任何可以回退、可以忽略、可以部分解析的路径，
# 都会让 CI 静默使用与 mise.toml 不一致的工具链。每个负例都在断言"必须非零退出"。
# 夹具只写系统临时目录，结束即清理；末尾显式 exit 0，避免残留子进程退出码污染 CI shell。

$ErrorActionPreference = "Stop"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$OutputEncoding = [System.Text.Encoding]::UTF8

$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
$readerPath = if ([string]::IsNullOrWhiteSpace($Reader)) {
  Join-Path $PSScriptRoot "read-mise-versions.ps1"
} else {
  $Reader
}
if (-not (Test-Path -LiteralPath $readerPath -PathType Leaf)) {
  Write-Host "::error::未找到版本读取脚本：$readerPath" -ForegroundColor Red
  exit 1
}

$tempRoot = Join-Path ([IO.Path]::GetTempPath()) ("soxu-mise-versions-" + [Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null

$script:checks = 0
$script:failures = [System.Collections.Generic.List[string]]::new()

function Assert-Check([bool]$Condition, [string]$Name) {
  $script:checks++
  if (-not $Condition) { $script:failures.Add($Name) }
}

function Invoke-Reader([string[]]$Arguments) {
  $output = (& pwsh -NoProfile -File $readerPath @Arguments 2>&1 | Out-String)
  return [pscustomobject]@{ Code = $LASTEXITCODE; Output = $output }
}

function Save-Fixture([string]$Name, [string]$Content, [switch]$Crlf) {
  $path = Join-Path $tempRoot ($Name + ".toml")
  $payload = if ($Crlf) { $Content.Replace("`n", "`r`n") } else { $Content }
  Set-Content -LiteralPath $path -Value $payload -Encoding utf8 -NoNewline
  return $path
}

function Assert-Pass([string]$Name, $Result) {
  Assert-Check ($Result.Code -eq 0) "$Name → 期望 exit 0，实际 exit $($Result.Code)：$($Result.Output.Trim())"
}

function Assert-Fail([string]$Name, $Result, [string]$Marker) {
  Assert-Check ($Result.Code -eq 1) "$Name → 期望 exit 1，实际 exit $($Result.Code)：$($Result.Output.Trim())"
  if ($Result.Code -eq 1 -and -not [string]::IsNullOrEmpty($Marker)) {
    Assert-Check ($Result.Output.Contains($Marker)) "$Name → 缺少失败标记 $Marker：$($Result.Output.Trim())"
  }
}

function New-Body([string]$Rust = 'rust = { version = "1.99.1", components = "rustfmt,clippy" }', [string]$Node = 'node = "26.13.1"', [string]$Pnpm = 'pnpm = "12.21.0"', [string]$Extra = "") {
  $lines = @("[settings]", 'disable_tools = ["java", "maven"]', "", "[tools]", $Rust, $Node, $Pnpm)
  if (-not [string]::IsNullOrEmpty($Extra)) { $lines += $Extra }
  return ($lines -join "`n") + "`n"
}

try {
  # ---------- 一、成功路径 ----------
  $realPath = Join-Path $repoRoot "mise.toml"
  $realResult = Invoke-Reader @("-Path", $realPath)
  Assert-Pass "成功：真实 mise.toml 可解析" $realResult
  $realDoc = $null
  if ($realResult.Code -eq 0) {
    $realDoc = ($realResult.Output.Trim() | ConvertFrom-Json)
    Assert-Check ($realDoc.schemaVersion -eq 1) "成功：输出必须带 schemaVersion=1"
    Assert-Check ($realDoc.rust.version -match '^\d+\.\d+\.\d+$') "成功：rust 必须是精确版本（实际 $($realDoc.rust.version)）"
    Assert-Check ($realDoc.node.version -match '^\d+\.\d+\.\d+$') "成功：node 必须是精确版本（实际 $($realDoc.node.version)）"
    Assert-Check ($realDoc.pnpm.version -match '^\d+\.\d+\.\d+$') "成功：pnpm 必须是精确版本（实际 $($realDoc.pnpm.version)）"
    Assert-Check ($realDoc.rust.components -eq 'rustfmt,clippy') "成功：rust components 必须原样带出（实际 $($realDoc.rust.components)）"
    Assert-Check ([string]::IsNullOrEmpty($realDoc.sourceRef)) "成功：worktree 来源不得伪造 ref"

    # 独立第二读者交叉校验：脚本输出的版本必须真的出现在 mise.toml 文本里，
    # 否则脚本可能读到了别的来源（缓存、内置默认、环境变量）。
    $realText = (Get-Content -LiteralPath $realPath -Raw -Encoding utf8).Replace("`r`n", "`n")
    Assert-Check ($realText.Contains('"' + $realDoc.rust.version + '"')) "成功：rust 版本必须与 mise.toml 文本一致"
    Assert-Check ($realText.Contains('"' + $realDoc.node.version + '"')) "成功：node 版本必须与 mise.toml 文本一致"
    Assert-Check ($realText.Contains('"' + $realDoc.pnpm.version + '"')) "成功：pnpm 版本必须与 mise.toml 文本一致"

    $expectedSha = (([System.BitConverter]::ToString([System.Security.Cryptography.SHA256]::HashData([System.Text.Encoding]::UTF8.GetBytes($realText)))).Replace("-", "")).ToLowerInvariant()
    Assert-Check ($realDoc.miseTomlSha256 -eq $expectedSha) "成功：mise 摘要必须与源文件字节一致（实际 $($realDoc.miseTomlSha256) 期望 $expectedSha）"

    # 脱敏：绝不出现本机绝对路径
    Assert-Check (-not ($realResult.Output -match '[A-Za-z]:\\')) "成功：输出不得包含 Windows 绝对路径"
    Assert-Check (-not ($realResult.Output -match '/(home|Users)/')) "成功：输出不得包含用户目录路径"
  }

  $crlfFixture = Save-Fixture "crlf" (New-Body) -Crlf
  $lfTwinFixture = Save-Fixture "lf-twin" (New-Body)
  $crlfResult = Invoke-Reader @("-Path", $crlfFixture)
  Assert-Pass "成功：CRLF 换行可解析" $crlfResult
  if ($crlfResult.Code -eq 0) {
    $crlfDoc = ($crlfResult.Output.Trim() | ConvertFrom-Json)
    Assert-Check ($crlfDoc.rust.version -eq '1.99.1' -and $crlfDoc.node.version -eq '26.13.1' -and $crlfDoc.pnpm.version -eq '12.21.0') "成功：CRLF 夹具必须解析出与 LF 相同版本"
    $lfTwinResult = Invoke-Reader @("-Path", $lfTwinFixture)
    Assert-Pass "成功：LF 孪生夹具可解析" $lfTwinResult
    if ($lfTwinResult.Code -eq 0) {
      $lfTwinDoc = ($lfTwinResult.Output.Trim() | ConvertFrom-Json)
      Assert-Check ($crlfDoc.miseTomlSha256 -eq $lfTwinDoc.miseTomlSha256) "成功：同一内容的 CRLF 与 LF 必须得到相同摘要（换行归一化）"
    }
  }

  $tableFixture = Save-Fixture "rust-subtable" ((@(
      "[tools]"
      'node = "26.13.1"'
      'pnpm = "12.21.0"'
      ""
      "[tools.rust]"
      'version = "1.99.1"'
      'components = "rustfmt,clippy"'
    ) -join "`n") + "`n")
  $tableResult = Invoke-Reader @("-Path", $tableFixture)
  Assert-Pass "成功：[tools.rust] 子表形式可解析" $tableResult
  if ($tableResult.Code -eq 0) {
    $tableDoc = ($tableResult.Output.Trim() | ConvertFrom-Json)
    Assert-Check ($tableDoc.rust.version -eq '1.99.1') "成功：子表形式的 rust 版本必须正确"
  }

  # ---------- 二、失败路径：缺项 / 重复 / 浮动 / 复合 / 畸形 / 额外字段 ----------
  Assert-Fail "失败：缺少 node 声明" (Invoke-Reader @("-Path", (Save-Fixture "missing-node" (New-Body -Node "")))) 'missing_tool:node'

  Assert-Fail "失败：内联表重复声明 rust" (Invoke-Reader @("-Path", (Save-Fixture "dup-inline" (New-Body -Extra 'rust = "1.98.0"')))) 'duplicate_tool:rust'

  Assert-Fail "失败：内联表与子表并存的重复声明" `
    (Invoke-Reader @("-Path", (Save-Fixture "dup-cross" ((New-Body) + "`n[tools.rust]`nversion = `"1.98.0`"`n")))) 'duplicate_tool:rust'

  Assert-Fail "失败：范围版本 (>=)" (Invoke-Reader @("-Path", (Save-Fixture "range" (New-Body -Node 'node = ">=26.0.0"')))) 'version_not_exact'
  Assert-Fail "失败：浮动版本 (26)" (Invoke-Reader @("-Path", (Save-Fixture "major-only" (New-Body -Node 'node = "26"')))) 'version_not_exact'
  Assert-Fail "失败：浮动版本 (26.x)" (Invoke-Reader @("-Path", (Save-Fixture "floor" (New-Body -Node 'node = "26.x"')))) 'version_not_exact'
  Assert-Fail "失败：标签版本 (latest)" (Invoke-Reader @("-Path", (Save-Fixture "latest" (New-Body -Pnpm 'pnpm = "latest"')))) 'version_not_exact'
  Assert-Fail "失败：复合版本" (Invoke-Reader @("-Path", (Save-Fixture "composite" (New-Body -Rust 'rust = "1.99.1,1.98.0"')))) 'version_composite'

  Assert-Fail "失败：未闭合字符串" (Invoke-Reader @("-Path", (Save-Fixture "unclosed" (New-Body -Node 'node = "26.13.1')))) 'malformed_value'
  Assert-Fail "失败：未闭合内联表" (Invoke-Reader @("-Path", (Save-Fixture "unclosed-table" (New-Body -Rust 'rust = { version = "1.99.1"')))) 'malformed_inline_table'
  Assert-Fail "失败：裸数字版本" (Invoke-Reader @("-Path", (Save-Fixture "bare-number" (New-Body -Node 'node = 26')))) 'malformed_value'
  $arrayTableFixture = Save-Fixture "array-table" ("[[tools]]`n" + (New-Body))
  Assert-Fail "失败：数组表头" (Invoke-Reader @("-Path", $arrayTableFixture)) 'malformed_array_table'
  Assert-Fail "失败：无法解析的行" (Invoke-Reader @("-Path", (Save-Fixture "garbage" ((New-Body) + "`nnot a toml line`n")))) 'malformed_line'

  Assert-Fail "失败：声明了未约定工具" (Invoke-Reader @("-Path", (Save-Fixture "extra-tool" (New-Body -Extra 'python = "3.13.17"')))) 'unexpected_tool:python'
  Assert-Fail "失败：内联表含未知字段" `
    (Invoke-Reader @("-Path", (Save-Fixture "extra-field" (New-Body -Rust 'rust = { version = "1.99.1", profile = "minimal" }')))) 'unexpected_field:rust.profile'
  Assert-Fail "失败：缺少 [tools] 段" `
    (Invoke-Reader @("-Path", (Save-Fixture "no-tools" (@("[settings]", 'disable_tools = ["java"]', "", "[env]", 'CARGO_TARGET_DIR = "x"') -join "`n")))) 'tools_section_missing'

  # ---------- 三、GitHub Actions 输出契约 ----------
  $outputPath = Join-Path $tempRoot "github-output.txt"
  Set-Content -LiteralPath $outputPath -Value "" -Encoding utf8 -NoNewline
  $outputResult = Invoke-Reader @("-Path", $crlfFixture, "-GitHubOutput", $outputPath)
  Assert-Pass "成功：写出 GITHUB_OUTPUT" $outputResult
  if ($outputResult.Code -eq 0) {
    $written = (Get-Content -LiteralPath $outputPath -Raw -Encoding utf8)
    foreach ($expectedKey in @("rust_version=1.99.1", "node_version=26.13.1", "pnpm_version=12.21.0", "rust_components=rustfmt,clippy", "toolchain_source=")) {
      Assert-Check ($written.Contains($expectedKey)) "GITHUB_OUTPUT 缺少 $expectedKey"
    }
    Assert-Check ($written.Contains("mise_toml_sha256=")) "GITHUB_OUTPUT 缺少 mise_toml_sha256"
    Assert-Check ($written.Contains("toolchain_source=worktree")) "GITHUB_OUTPUT 的 worktree 来源必须显式写 worktree 而不是路径"
    Assert-Check (-not ($written -match '[A-Za-z]:\\')) "GITHUB_OUTPUT 不得写入绝对路径"
    Assert-Check (-not ($written -match '/(home|Users)/')) "GITHUB_OUTPUT 不得写入用户目录"
  }

  # ---------- 四、受信任 ref 读取 ----------
  $refResult = Invoke-Reader @("-Ref", "HEAD", "-Json")
  Assert-Pass "成功：按 HEAD ref 读取 mise.toml" $refResult
  if ($refResult.Code -eq 0) {
    $refDoc = ($refResult.Output.Trim() | ConvertFrom-Json)
    Assert-Check ($refDoc.sourceRef -eq 'HEAD') "成功：ref 来源必须回填到输出"
    if ($null -ne $realDoc) {
      Assert-Check ($refDoc.rust.version -eq $realDoc.rust.version) "成功：ref 读取与工作树读取必须一致"
    }
  }
  Assert-Fail "失败：不存在的 ref" (Invoke-Reader @("-Ref", "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef")) 'source_ref_unresolved'
  Assert-Fail "失败：危险 ref 形态（以 - 开头）" (Invoke-Reader @("-Ref", "--help")) 'source_ref_invalid'
  Assert-Fail "失败：源文件不存在" (Invoke-Reader @("-Path", (Join-Path $tempRoot "does-not-exist.toml"))) 'source_not_found'

  # ---------- 五、已安装版本一致性 ----------
  $mismatchFixture = Save-Fixture "assert-mismatch" (@("[tools]", 'rust = "9.9.9"', 'node = "9.9.9"', 'pnpm = "9.9.9"') -join "`n")
  Assert-Fail "失败：已安装版本与声明不符" (Invoke-Reader @("-Path", $mismatchFixture, "-AssertInstalled")) 'installed_tool_mismatch:rust'

  $assertResult = Invoke-Reader @("-Path", $realPath, "-AssertInstalled", "-Json")
  $installedMatchesDeclared = ($assertResult.Code -eq 0)
  if ($installedMatchesDeclared) {
    $assertDoc = ($assertResult.Output.Trim() | ConvertFrom-Json)
    Assert-Check ($null -ne $assertDoc.installed) "成功：一致性校验必须回填实测版本"
    Assert-Check ($assertDoc.installed.rust -eq $assertDoc.rust.version) "成功：实测 rust 必须等于声明版本"
    Assert-Check ($assertDoc.installed.node -eq $assertDoc.node.version) "成功：实测 node 必须等于声明版本"
    Assert-Check ($assertDoc.installed.pnpm -eq $assertDoc.pnpm.version) "成功：实测 pnpm 必须等于声明版本"
  } else {
    # 本机未按 mise 锁定环境安装时，必须如实非零退出（这正是防漂移的意义），
    # 但不能把它当成"测试失败"：只要报告的是版本不符而不是解析错误即可。
    Assert-Check ($assertResult.Output.Contains('installed_tool_mismatch') -or $assertResult.Output.Contains('installed_tool_missing')) `
      "环境未对齐时一致性校验必须报 installed_tool_* 而不是其他错误：$($assertResult.Output.Trim())"
  }

  Assert-Fail "失败：一致性校验不接受未知工具参数" (Invoke-Reader @("-Path", $realPath, "-AssertInstalled", "-AssertTools", "go")) 'assert_tool_unknown:go'

  # ---------- 六、严格性反证：去掉精确性校验的突变体必须放行浮动版本 ----------
  $readerText = Get-Content -LiteralPath $readerPath -Raw -Encoding utf8
  $marker = '  if ($Version -match $VersionPattern) { return }'
  Assert-Check ($readerText.Contains($marker)) "突变测试锚点缺失：read-mise-versions.ps1 应有精确版本校验"
  if ($readerText.Contains($marker)) {
    $mutantPath = Join-Path $tempRoot "reader-mutant.ps1"
    Set-Content -LiteralPath $mutantPath -Value $readerText.Replace($marker, '  return') -Encoding utf8 -NoNewline
    $mutantResult = & pwsh -NoProfile -File $mutantPath -Path (Save-Fixture "range-mutant" (New-Body -Node 'node = ">=26.0.0"')) 2>&1 | Out-String
    $mutantCode = $LASTEXITCODE
    Assert-Check ($mutantCode -eq 0) "突变敏感度：移除精确性校验后浮动版本必须重新通过（实际 exit $mutantCode）"
    Assert-Check ($mutantCode -eq 0 -and $mutantResult.Contains('>=26.0.0')) "突变敏感度：突变体必须真的把浮动版本当作声明值带出（实际：$($mutantResult.Trim())）"
  }
}
finally {
  if (Test-Path -LiteralPath $tempRoot) {
    Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
  }
}

if ($script:failures.Count -eq 0) {
  Write-Host "✓ mise 版本读取回归矩阵全部通过（$($script:checks) 项断言）" -ForegroundColor Green
  exit 0
}

foreach ($failure in $script:failures) {
  Write-Host "::error::MISE-VERSIONS-TEST-FAIL $failure" -ForegroundColor Red
}
Write-Host "✗ mise 版本读取回归矩阵失败：$($script:failures.Count) / $($script:checks) 项断言未通过" -ForegroundColor Red
exit 1
