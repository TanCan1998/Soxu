param(
  [Parameter(Mandatory)][string]$AgentId,
  [Parameter(Mandatory)][string]$PromptFile,
  [ValidateSet('steer', 'interrupt')][string]$Behavior = 'steer',
  [ValidateRange(1, 60)][int]$Attempts = 5,
  [ValidateRange(0, 10000)][int]$DelayMilliseconds = 500,
  [switch]$VerifyOnly,
  [string]$ControlScript = (Join-Path $PSScriptRoot 'paseo-ctl.ps1')
)

$ErrorActionPreference = 'Stop'
$promptPath = (Resolve-Path -LiteralPath $PromptFile).Path
$expected = (Get-Content -Raw -Encoding utf8 -LiteralPath $promptPath).Replace("`r`n", "`n").TrimEnd([char]10)
$marker = $expected.Split([char]10)[0]
if ([string]::IsNullOrWhiteSpace($marker) -or -not $expected.Contains("`n")) {
  throw '任务书需要唯一首行标记及多行正文。'
}

function Invoke-Control([string[]]$Arguments) {
  $output = & pwsh -NoProfile -File $ControlScript @Arguments
  if ($LASTEXITCODE -ne 0) { throw "Paseo 命令失败，退出码 $LASTEXITCODE。" }
  return ($output | Out-String) | ConvertFrom-Json -ErrorAction Stop
}

if (-not $VerifyOnly) {
  $sent = Invoke-Control @('send', $AgentId, '--prompt-file', $promptPath, '--behavior', $Behavior, '--no-wait', '--json')
  if ($sent.agentId -ne $AgentId -or $sent.status -ne 'sent') { throw 'Paseo 未确认发送到指定代理。' }
}

for ($attempt = 1; $attempt -le $Attempts; $attempt++) {
  $timeline = Invoke-Control @('logs', $AgentId, '--json')
  if ($timeline.agentId -ne $AgentId) { throw 'Paseo 日志的代理身份不匹配。' }
  $delivery = $timeline.items | Where-Object {
    $_.type -eq 'user_message' -and $_.text -and $_.text.Replace("`r`n", "`n").StartsWith("$marker`n", [StringComparison]::Ordinal)
  } | Select-Object -Last 1
  if ($delivery) {
    $actual = $delivery.text.Replace("`r`n", "`n").TrimEnd([char]10)
    if (-not [string]::Equals($expected, $actual, [StringComparison]::Ordinal)) { throw 'Paseo 任务书被截断或正文不一致；未确认开工。' }
    if (-not $delivery.messageId) { throw '送达日志缺少消息 ID；未确认开工。' }
    $bytes = [Text.Encoding]::UTF8.GetBytes($expected)
    [ordered]@{
      agentId = $timeline.agentId; messageId = $delivery.messageId; marker = $marker
      exactMatch = $true; characters = $expected.Length
      bodySha256 = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)).ToLowerInvariant()
      startConfirmed = $false
    } | ConvertTo-Json
    exit 0
  }
  if ($attempt -lt $Attempts) { Start-Sleep -Milliseconds $DelayMilliseconds }
}
throw '有限等待后仍无任务书全文送达证据；不重发、不创建代理、不确认开工。'
