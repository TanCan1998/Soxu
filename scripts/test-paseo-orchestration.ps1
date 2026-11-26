$ErrorActionPreference = 'Stop'
$fixture = Join-Path ([IO.Path]::GetTempPath()) ("soxu-paseo-test-" + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $fixture | Out-Null
$referenceScript = Join-Path $PSScriptRoot 'paseo-reference-context.ps1'
$dispatchScript = Join-Path $PSScriptRoot 'paseo-dispatch.ps1'
$queryStub = Join-Path $fixture 'query.ps1'
$controlStub = Join-Path $fixture 'control.ps1'
$queryLog = Join-Path $fixture 'query.log'
$controlLog = Join-Path $fixture 'control.log'
$prompt = Join-Path $fixture 'prompt.txt'
$agent = 'fixture-agent'
$passed = 0

function Assert-Result([bool]$Condition, [string]$Title) {
  if (-not $Condition) { throw "夹具失败：$Title；证据保留于 $fixture" }
  $script:passed++
  Write-Host "通过：$Title"
}

@'
$arguments = @($args)
Add-Content -LiteralPath (Join-Path $PSScriptRoot 'query.log') -Value ($arguments -join '|')
if ($arguments[0] -eq '--repo') {
  @{root=$arguments[1]; baseline_exists=$true; index_entries=3} | ConvertTo-Json
} else {
  @{initialized=$true; projectPath=$arguments[2]; nodeCount=7; edgeCount=8; rebuildInProgress=$false; worktreeMismatch=$null; index=@{state='complete'; reindexRecommended=$false; filesNeedingReindex=0; filesWithParseErrors=0}; pendingChanges=@{added=0;modified=0;removed=0}} | ConvertTo-Json -Depth 4
}
exit 0
'@ | Set-Content -LiteralPath $queryStub -Encoding utf8

@'
$arguments = @($args)
Add-Content -LiteralPath (Join-Path $PSScriptRoot 'control.log') -Value $arguments[0]
$mode = Get-Content -Raw -LiteralPath (Join-Path $PSScriptRoot 'mode.txt')
if ($mode.Trim() -eq 'error') { exit 7 }
if ($arguments[0] -eq 'send') {
  @{agentId=$arguments[1];status='sent'} | ConvertTo-Json
} else {
  $text = (Get-Content -Raw -LiteralPath (Join-Path $PSScriptRoot 'prompt.txt')).Replace("`r`n", "`n").Replace("`n", "`r`n") + "`r`n"
  if ($mode.Trim() -eq 'truncated') { $text = $text.Substring(0,$text.Length-6) }
  $items = @(@{type='user_message';text=$text;messageId='fixture-message'})
  if ($mode.Trim() -eq 'missing') { $items = @() }
  $agent = $arguments[1]
  if ($mode.Trim() -eq 'wrong-agent') { $agent = 'other-agent' }
  @{agentId=$agent;items=$items} | ConvertTo-Json -Depth 4
}
exit 0
'@ | Set-Content -LiteralPath $controlStub -Encoding utf8

$reference = Join-Path $fixture 'reference'
New-Item -ItemType Directory -Path $reference | Out-Null
& git init -q $reference
if ($LASTEXITCODE -ne 0) { throw 'Git 夹具初始化失败。' }
'fixture' | Set-Content -LiteralPath (Join-Path $reference 'source.txt')
& git -C $reference add source.txt
if ($LASTEXITCODE -ne 0) { throw 'Git 夹具暂存失败。' }
& git -C $reference -c core.hooksPath= -c user.name=Fixture -c user.email=fixture@example.invalid commit -qm fixture
if ($LASTEXITCODE -ne 0) { throw 'Git 夹具提交失败。' }
New-Item -ItemType Directory -Path (Join-Path $reference '.codegraph') | Out-Null
'fixture-db' | Set-Content -LiteralPath (Join-Path $reference '.codegraph/codegraph.db')
"#AOCI-ROOT-MANIFEST: 1`n#Volume: id=code kind=code path=aoci.code.txt state=enabled" | Set-Content -LiteralPath (Join-Path $reference 'aoci.txt')
'fixture-entry' | Set-Content -LiteralPath (Join-Path $reference 'aoci.code.txt')
$before = Get-FileHash -LiteralPath (Join-Path $reference 'aoci.txt'), (Join-Path $reference 'aoci.code.txt'), (Join-Path $reference '.codegraph/codegraph.db')
$output = & pwsh -NoProfile -File $referenceScript -ReferenceRoot $fixture -Names reference -AociExecutable $queryStub -CodeGraphExecutable $queryStub -RequireReady
Assert-Result ($LASTEXITCODE -eq 0) '完整参考资产可查询'
$context = ($output | Out-String) | ConvertFrom-Json
Assert-Result ($context.references[0].aoci.assets.Count -eq 2 -and $context.references[0].codegraph.exploreArguments[2] -eq $reference) 'Volumes 与图查询均绑定参考根'
$after = Get-FileHash -LiteralPath (Join-Path $reference 'aoci.txt'), (Join-Path $reference 'aoci.code.txt'), (Join-Path $reference '.codegraph/codegraph.db')
Assert-Result (($before.Hash -join ',') -eq ($after.Hash -join ',')) '盘点未改既有索引字节'
$queries = Get-Content -LiteralPath $queryLog
Assert-Result ($queries.Count -eq 2 -and $queries[0].StartsWith('--repo|') -and $queries[1].StartsWith('status|--json|')) '仅运行状态查询，不初始化或重建'
$output = & pwsh -NoProfile -File $referenceScript -ReferenceRoot $fixture -Names reference,missing -AociExecutable $queryStub -CodeGraphExecutable $queryStub -RequireReady
Assert-Result ($LASTEXITCODE -eq 1 -and (($output | Out-String) | ConvertFrom-Json).references.Count -eq 2) '缺失参考返回完整清单且阻止 ready'
Add-Content -LiteralPath (Join-Path $reference 'aoci.txt') -Value '#Volume: id=unsafe kind=code path=../outside.txt state=enabled'
$output = & pwsh -NoProfile -File $referenceScript -ReferenceRoot $fixture -Names reference -AociExecutable $queryStub -CodeGraphExecutable $queryStub -RequireReady
Assert-Result ($LASTEXITCODE -eq 1 -and -not (($output | Out-String) | ConvertFrom-Json).references[0].ready) '越界 Volume 声明拒绝复用'

"Soxu fixture / unique`n参考已有 AOCI 与 CodeGraph。`n第二行正文" | Set-Content -LiteralPath $prompt -Encoding utf8
'good' | Set-Content -LiteralPath (Join-Path $fixture 'mode.txt')
$output = & pwsh -NoProfile -File $dispatchScript -AgentId $agent -PromptFile $prompt -ControlScript $controlStub -Attempts 1
Assert-Result ($LASTEXITCODE -eq 0 -and (($output | Out-String) | ConvertFrom-Json).exactMatch) '发送并全文核验，只归一换行'
Assert-Result ((Get-Content -LiteralPath $controlLog) -join ',' -eq 'send,logs') '不自动开工或重复发送'
$output = & pwsh -NoProfile -File $dispatchScript -AgentId $agent -PromptFile $prompt -ControlScript $controlStub -VerifyOnly -Attempts 1
Assert-Result ($LASTEXITCODE -eq 0 -and ((Get-Content -LiteralPath $controlLog) -join ',') -eq 'send,logs,logs') '仅核验模式不发送'
foreach ($mode in @('truncated','missing','wrong-agent','error')) {
  $mode | Set-Content -LiteralPath (Join-Path $fixture 'mode.txt')
  & pwsh -NoProfile -File $dispatchScript -AgentId $agent -PromptFile $prompt -ControlScript $controlStub -VerifyOnly -Attempts 1 2>$null | Out-Null
  Assert-Result ($LASTEXITCODE -ne 0) "拒绝 $mode 送达证据"
}
Write-Host "$passed 项回归通过；临时夹具保留于 $fixture"
exit 0
