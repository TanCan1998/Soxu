# 运行时资源采集（只读、可重复、口径固定）。
#
# 设计约束：
# 1. 只读纪律：SQLite 一律以 `-readonly` 打开并先执行 `PRAGMA query_only=1`；
#    绝不执行写入语句、`wal_checkpoint`、`VACUUM`、`ANALYZE`。连接被锁时带 `.timeout 2000`，
#    超时后只标记 `status = "busy"`，继续采集文件长度与其余库。
# 2. 脚本只报告测量值，不做判定、不做平滑、不把差值换算成收益；结论留给使用方。
# 3. 字节口径：所有字节字段以 `Bytes` 结尾。`pageBytes = pageSize * pageCount`（该库已分配的
#    页空间），`freelistBytes = freelistCount * pageSize`（其中空闲可回收部分，是 `pageBytes`
#    的子集，不是额外占用）。`-wal`/`-shm` 长度与主库长度分开报告，不合并进 `lengthBytes`。
# 4. 退化不抛异常：PID 不存在 / 目录不存在 / sqlite3 缺失 / 库被锁 都以结构化状态与
#    `warnings` 表达。退出码契约：0 = 至少采到一个有效目标；3 = 请求的目标全部退化；
#    自检失败 = 1；参数非法由 PowerShell 参数校验拦截。脚本末尾显式 `exit`，
#    避免 sqlite3 的残留 `$LASTEXITCODE` 污染调用方。
# 5. 隐私：不读取凭据，不硬编码任何本机用户目录；提示信息中的绝对路径统一收敛为 `<path>`。
#    脚本会回显 `-DataDir` 指定的被观测目录（这是测量对象），贴出报告前请自行判断是否脱敏。

param(
  [int[]]$ProcessId = @(),
  [string]$DataDir = "",
  [switch]$Json,
  [ValidateRange(1, 600)]
  [int]$SampleCount = 1,
  [ValidateRange(0, 600000)]
  [int]$IntervalMilliseconds = 0,
  [ValidateRange(0, 8)]
  [int]$MaxDepth = 3,
  [switch]$SelfTest
)

$ErrorActionPreference = "Stop"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$OutputEncoding = [System.Text.Encoding]::UTF8

# 报告契约版本：字段只做加法演进，既有字段语义不变。
$MetricsSchemaVersion = 1
# 只认这些数据面文件；`-wal`/`-shm` 必须与同名主库成对出现才属于同一库。
$DataFilePattern = '(?i)\.(db|sqlite|sqlite3)(-wal|-shm)?$'
# SQLite `auto_vacuum` 整数值到名称的稳定映射，报告里两者都给出，避免读者二次查表。
$AutoVacuumModes = @{ 0 = "none"; 1 = "full"; 2 = "incremental" }
# 单次 sqlite3 调用的硬上限：`.timeout` 只覆盖锁等待，这里防止异常挂起。
$SqliteHardTimeoutMilliseconds = 30000

function Format-IsoTimestamp([datetimeoffset]$Value) {
  return $Value.ToString("yyyy-MM-ddTHH:mm:ss.fffzzz", [System.Globalization.CultureInfo]::InvariantCulture)
}

# 提示信息脱敏：绝对路径、UNC 一律收敛为占位符，避免报告与日志带出本机目录。
function ConvertTo-SafeMessage([string]$Text) {
  if ([string]::IsNullOrWhiteSpace($Text)) { return "" }
  $value = $Text
  $value = [regex]::Replace($value, '(?i)\b[a-z]:[\\/][^\s"'']*', '<path>')
  $value = [regex]::Replace($value, '(?i)\\\\[^\s"'']+', '<unc>')
  $value = [regex]::Replace($value, '(?i)[a-z][a-z0-9+.\-]*://\S+', '<url>')
  $value = [regex]::Replace($value, '\s+', ' ').Trim()
  if ($value.Length -gt 240) { $value = $value.Substring(0, 240) + "…" }
  return $value
}

function New-Warnings {
  # 逗号包裹：PowerShell 会展开空集合为 $null，单元素逗号保证调用方拿到可写列表。
  return , ([System.Collections.Generic.List[object]]::new())
}

# 同一 (code, target) 只登记一次：多次采样时不会把同一条退化刷成 N 条噪音。
function Add-Warning($Warnings, [string]$Code, [string]$Message, [string]$Target = "") {
  if ($null -eq $Warnings) { return }
  foreach ($existing in $Warnings) {
    if ($existing.code -eq $Code -and $existing.target -eq $Target) { return }
  }
  $Warnings.Add([ordered]@{
      code    = $Code
      target  = $Target
      message = (ConvertTo-SafeMessage $Message)
    })
}

function Format-Value($Value) {
  if ($null -eq $Value) { return "-" }
  if ($Value -is [string] -and $Value -eq "") { return "-" }
  return [string]$Value
}

function Resolve-SqliteCommand {
  $command = Get-Command -Name "sqlite3" -CommandType Application -ErrorAction SilentlyContinue |
    Select-Object -First 1
  if ($null -eq $command) { return $null }
  return [string]$command.Source
}

# 结构化进程调用：stdout/stderr 分开读取，保留退出码，避免 PowerShell 把 stderr
# 混进值解析，也避免 sqlite3 的错误文本被当成数据。
function Invoke-CapturedTool {
  param(
    [Parameter(Mandatory = $true)][string]$FilePath,
    [string[]]$Arguments = @()
  )
  $psi = [System.Diagnostics.ProcessStartInfo]::new()
  $psi.FileName = $FilePath
  $psi.UseShellExecute = $false
  $psi.RedirectStandardOutput = $true
  $psi.RedirectStandardError = $true
  $psi.CreateNoWindow = $true
  foreach ($argument in $Arguments) { $psi.ArgumentList.Add($argument) }

  $process = $null
  try {
    $process = [System.Diagnostics.Process]::Start($psi)
  } catch {
    return [pscustomobject]@{ started = $false; exitCode = -1; stdout = ""; stderr = (ConvertTo-SafeMessage $_.Exception.Message) }
  }
  try {
    $stdoutTask = $process.StandardOutput.ReadToEndAsync()
    $stderrTask = $process.StandardError.ReadToEndAsync()
    if (-not $process.WaitForExit($SqliteHardTimeoutMilliseconds)) {
      try { $process.Kill($true) } catch { }
      return [pscustomobject]@{ started = $true; exitCode = -2; stdout = ""; stderr = "工具调用超时" }
    }
    $stdout = $stdoutTask.GetAwaiter().GetResult()
    $stderr = $stderrTask.GetAwaiter().GetResult()
    return [pscustomobject]@{ started = $true; exitCode = $process.ExitCode; stdout = $stdout; stderr = $stderr }
  } finally {
    $process.Dispose()
  }
}

function Get-SqliteVersion([string]$SqliteExe) {
  if ([string]::IsNullOrWhiteSpace($SqliteExe)) { return $null }
  $result = Invoke-CapturedTool -FilePath $SqliteExe -Arguments @("--version")
  if ($result.exitCode -ne 0 -or [string]::IsNullOrWhiteSpace($result.stdout)) { return $null }
  $first = @(($result.stdout -split "\s+") | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }) | Select-Object -First 1
  if ([string]::IsNullOrWhiteSpace($first)) { return $null }
  return $first
}

# 只读会话：`-readonly` + `PRAGMA query_only=1` 双保险；`.timeout 2000` 只等待锁，不写库。
function Invoke-SqliteReadOnlyQuery {
  param(
    [Parameter(Mandatory = $true)][string]$SqliteExe,
    [Parameter(Mandatory = $true)][string]$DatabasePath,
    [Parameter(Mandatory = $true)][string]$Sql
  )
  $arguments = @(
    "-readonly",
    "-cmd", ".timeout 2000",
    "-cmd", ".bail on",
    $DatabasePath,
    $Sql
  )
  return Invoke-CapturedTool -FilePath $SqliteExe -Arguments $arguments
}

# 把 `key=value` 行解析成哈希表；非匹配行（含提示噪音）直接忽略。
function ConvertFrom-KeyValueLines([string]$Text) {
  $map = @{}
  foreach ($line in ($Text -split "`n")) {
    $trimmed = $line.Trim().TrimEnd("`r")
    if ([string]::IsNullOrWhiteSpace($trimmed)) { continue }
    $match = [regex]::Match($trimmed, '^(?<key>[a-z_]+)=(?<value>.*)$')
    if (-not $match.Success) { continue }
    $map[$match.Groups["key"].Value] = $match.Groups["value"].Value
  }
  return $map
}

function ConvertTo-LongOrNull([string]$Value) {
  if ([string]::IsNullOrWhiteSpace($Value)) { return $null }
  $parsed = 0L
  if ([long]::TryParse($Value.Trim(), [ref]$parsed)) { return [long]$parsed }
  return $null
}

# 逐个指标单独取用：某项在当前平台不支持时只退化该项，不连坐整条进程记录。
function Get-ProcessValue {
  param(
    [scriptblock]$Getter,
    $Warnings,
    [string]$Target
  )
  try {
    return & $Getter
  } catch {
    Add-Warning $Warnings "process_metric_unavailable" "该进程指标在当前平台或当前状态下不可读。" $Target
    return $null
  }
}

function Get-ProcessMetrics {
  param(
    [int]$Id,
    $Warnings
  )
  $entry = [ordered]@{
    processId              = $Id
    status                 = "exited"
    processName            = $null
    workingSetBytes        = $null
    peakWorkingSetBytes    = $null
    privateMemorySizeBytes = $null
    handleCount            = $null
    threadCount            = $null
    startTime              = $null
    message                = $null
  }

  $process = $null
  try {
    $process = [System.Diagnostics.Process]::GetProcessById($Id)
  } catch {
    $process = $null
  }

  if ($null -eq $process) {
    $entry.message = "进程不存在或已退出，未获取任何数值。"
    Add-Warning $Warnings "process_not_found" "PID 不存在或已退出：按 exited 记录。" "pid:$Id"
    return $entry
  }

  try {
    $exited = $true
    try { $exited = [bool]$process.HasExited } catch { $exited = $false }
    if ($exited) {
      $entry.message = "进程已退出，未取到数值。"
      Add-Warning $Warnings "process_exited" "PID 存在但已退出：按 exited 记录。" "pid:$Id"
      return $entry
    }

    $entry.status = "running"
    $entry.processName = Get-ProcessValue { $process.ProcessName } $Warnings "pid:$Id/processName"
    $entry.workingSetBytes = Get-ProcessValue { [long]$process.WorkingSet64 } $Warnings "pid:$Id/workingSetBytes"
    $entry.peakWorkingSetBytes = Get-ProcessValue { [long]$process.PeakWorkingSet64 } $Warnings "pid:$Id/peakWorkingSetBytes"
    $entry.privateMemorySizeBytes = Get-ProcessValue { [long]$process.PrivateMemorySize64 } $Warnings "pid:$Id/privateMemorySizeBytes"
    $entry.handleCount = Get-ProcessValue { [int]$process.HandleCount } $Warnings "pid:$Id/handleCount"
    $entry.threadCount = Get-ProcessValue { [int]$process.Threads.Count } $Warnings "pid:$Id/threadCount"
    $entry.startTime = Get-ProcessValue { Format-IsoTimestamp ([datetimeoffset]::new($process.StartTime)) } $Warnings "pid:$Id/startTime"
    return $entry
  } catch {
    $entry.status = "error"
    $entry.message = (ConvertTo-SafeMessage $_.Exception.Message)
    Add-Warning $Warnings "process_read_failed" "读取进程指标失败：$($_.Exception.Message)" "pid:$Id"
    return $entry
  } finally {
    try { $process.Dispose() } catch { }
  }
}

function Get-ProcessSample {
  param(
    [int[]]$Ids = @(),
    $Warnings,
    [int]$Index = 1
  )
  $list = [System.Collections.Generic.List[object]]::new()
  foreach ($id in $Ids) {
    $list.Add((Get-ProcessMetrics -Id $id -Warnings $Warnings))
  }
  return [ordered]@{
    index       = $Index
    collectedAt = (Format-IsoTimestamp ([datetimeoffset]::Now))
    processes   = @($list)
  }
}

# 单个库的只读元数据。任何失败都收敛为 status + message，不向上抛。
function Read-SqliteDatabaseMetrics {
  param(
    [string]$SqliteExe,
    [string]$DatabasePath,
    [string]$RelativePath,
    $LengthBytes,
    $WalBytes,
    $ShmBytes,
    $Warnings
  )
  $entry = [ordered]@{
    relativePath  = $RelativePath
    status        = "ok"
    lengthBytes   = $LengthBytes
    walBytes      = $WalBytes
    shmBytes      = $ShmBytes
    pageSize      = $null
    pageCount     = $null
    pageBytes     = $null
    freelistCount = $null
    freelistBytes = $null
    journalMode   = $null
    autoVacuum    = $null
    autoVacuumMode = $null
    eventLog      = $null
    message       = $null
  }

  # 一条语句取全部结构事实；`pragma_*` 表函数把 PRAGMA 结果变成可并行的单行查询。
  $probeSql = @'
PRAGMA query_only = 1;
SELECT 'page_size=' || (SELECT * FROM pragma_page_size());
SELECT 'page_count=' || (SELECT * FROM pragma_page_count());
SELECT 'freelist_count=' || (SELECT * FROM pragma_freelist_count());
SELECT 'journal_mode=' || (SELECT * FROM pragma_journal_mode());
SELECT 'auto_vacuum=' || (SELECT * FROM pragma_auto_vacuum());
SELECT 'event_log=' || (SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'event_log');
'@

  $probe = Invoke-SqliteReadOnlyQuery -SqliteExe $SqliteExe -DatabasePath $DatabasePath -Sql $probeSql
  if ($probe.exitCode -ne 0) {
    $detail = if (-not [string]::IsNullOrWhiteSpace($probe.stderr)) { $probe.stderr } else { $probe.stdout }
    $firstLine = @(($detail -split "`n") | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }) | Select-Object -First 1
    $entry.message = (ConvertTo-SafeMessage $firstLine)
    if ($entry.message -match '(?i)locked|busy') {
      $entry.status = "busy"
      Add-Warning $Warnings "sqlite_busy" "只读会话在规定等待时间内仍被锁，仅保留文件长度。" $RelativePath
    } else {
      $entry.status = "error"
      Add-Warning $Warnings "sqlite_read_failed" "只读元数据查询失败：$($entry.message)" $RelativePath
    }
    return $entry
  }

  $map = ConvertFrom-KeyValueLines $probe.stdout
  $entry.pageSize = ConvertTo-LongOrNull $map["page_size"]
  $entry.pageCount = ConvertTo-LongOrNull $map["page_count"]
  $entry.freelistCount = ConvertTo-LongOrNull $map["freelist_count"]
  $entry.journalMode = if ($map.ContainsKey("journal_mode")) { $map["journal_mode"] } else { $null }
  $entry.autoVacuum = ConvertTo-LongOrNull $map["auto_vacuum"]
  if ($null -ne $entry.autoVacuum) {
    $entry.autoVacuumMode = if ($AutoVacuumModes.ContainsKey([int]$entry.autoVacuum)) {
      $AutoVacuumModes[[int]$entry.autoVacuum]
    } else {
      "unknown"
    }
  }
  if ($null -ne $entry.pageSize -and $null -ne $entry.pageCount) {
    $entry.pageBytes = [long]($entry.pageSize * $entry.pageCount)
  }
  if ($null -ne $entry.pageSize -and $null -ne $entry.freelistCount) {
    $entry.freelistBytes = [long]($entry.pageSize * $entry.freelistCount)
  }

  $hasEventLog = (ConvertTo-LongOrNull $map["event_log"]) -eq 1
  if (-not $hasEventLog) {
    # 缺表是正常形态（未启用事件日志的库），不是失败。
    $entry.eventLog = "absent"
    return $entry
  }

  $eventSql = @'
PRAGMA query_only = 1;
SELECT 'row_count=' || COUNT(*) FROM event_log;
SELECT 'max_seq=' || COALESCE((SELECT MAX(seq) FROM event_log), 0);
'@
  $eventProbe = Invoke-SqliteReadOnlyQuery -SqliteExe $SqliteExe -DatabasePath $DatabasePath -Sql $eventSql
  if ($eventProbe.exitCode -ne 0) {
    $detail = if (-not [string]::IsNullOrWhiteSpace($eventProbe.stderr)) { $eventProbe.stderr } else { $eventProbe.stdout }
    $firstLine = @(($detail -split "`n") | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }) | Select-Object -First 1
    $entry.eventLog = [ordered]@{ status = "error"; rowCount = $null; maxSeq = $null }
    Add-Warning $Warnings "event_log_read_failed" "event_log 存在但读取失败：$(ConvertTo-SafeMessage $firstLine)" $RelativePath
    return $entry
  }

  $eventMap = ConvertFrom-KeyValueLines $eventProbe.stdout
  $entry.eventLog = [ordered]@{
    status   = "present"
    rowCount = (ConvertTo-LongOrNull $eventMap["row_count"])
    maxSeq   = (ConvertTo-LongOrNull $eventMap["max_seq"])
  }
  return $entry
}

function Get-DataMetrics {
  param(
    [string]$Dir,
    [int]$MaxDepth = 3,
    $Warnings
  )

  $result = [ordered]@{
    requested   = -not [string]::IsNullOrWhiteSpace($Dir)
    dir         = $Dir
    resolvedDir = $null
    exists      = $false
    maxDepth    = $MaxDepth
    sqlite      = "unavailable"
    sqliteVersion = $null
    totals      = [ordered]@{ fileCount = 0; dbBytes = [long]0; walBytes = [long]0; shmBytes = [long]0 }
    files       = @()
    databases   = @()
  }

  $sqliteExe = Resolve-SqliteCommand
  if ($null -ne $sqliteExe) {
    $result.sqlite = "available"
    $result.sqliteVersion = Get-SqliteVersion $sqliteExe
  } elseif ($result.requested) {
    Add-Warning $Warnings "sqlite_unavailable" "未找到 sqlite3，仅采集数据面文件长度；脚本不安装任何依赖。" "sqlite3"
  }

  if (-not $result.requested) { return $result }

  $resolvedDir = $null
  try {
    $resolvedDir = (Resolve-Path -LiteralPath $Dir -ErrorAction Stop).ProviderPath
  } catch {
    $resolvedDir = $null
  }
  if ($null -eq $resolvedDir -or -not (Test-Path -LiteralPath $resolvedDir -PathType Container)) {
    Add-Warning $Warnings "data_dir_missing" "数据目录不存在或不是目录，未采集任何文件。" "data.dir"
    return $result
  }
  $result.resolvedDir = $resolvedDir
  $result.exists = $true

  $items = @()
  try {
    $items = @(Get-ChildItem -LiteralPath $resolvedDir -File -Recurse -Depth $MaxDepth -Force -ErrorAction Stop)
  } catch {
    Add-Warning $Warnings "data_dir_unreadable" "目录枚举中断，已采集部分可能不完整：$($_.Exception.Message)" "data.dir"
  }

  $files = [System.Collections.Generic.List[object]]::new()
  $sidecars = @{}
  foreach ($item in $items) {
    if ($null -eq $item) { continue }
    if (($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) { continue }
    if ($item.Name -notmatch $DataFilePattern) { continue }

    $relativePath = [System.IO.Path]::GetRelativePath($resolvedDir, $item.FullName).Replace("\", "/")
    $lowerName = $item.Name.ToLowerInvariant()
    $kind = if ($lowerName.EndsWith("-wal")) { "wal" } elseif ($lowerName.EndsWith("-shm")) { "shm" } else { "db" }

    $lengthBytes = $null
    try { $lengthBytes = [long]$item.Length } catch { $lengthBytes = $null }
    $lastWriteTime = $null
    try { $lastWriteTime = Format-IsoTimestamp ([datetimeoffset]$item.LastWriteTime) } catch { $lastWriteTime = $null }

    $files.Add([ordered]@{
        relativePath  = $relativePath
        kind          = $kind
        lengthBytes   = $lengthBytes
        lastWriteTime = $lastWriteTime
      })

    $result.totals.fileCount += 1
    $safeLength = if ($null -eq $lengthBytes) { 0 } else { [long]$lengthBytes }
    switch ($kind) {
      "db" { $result.totals.dbBytes += $safeLength }
      "wal" {
        $result.totals.walBytes += $safeLength
        $sidecars[$relativePath.Substring(0, $relativePath.Length - 4)] = $relativePath
      }
      "shm" {
        $result.totals.shmBytes += $safeLength
        $sidecars[$relativePath.Substring(0, $relativePath.Length - 4)] = $relativePath
      }
    }
  }

  $fileList = @($files | Sort-Object { $_.relativePath })
  $result.files = $fileList
  $dbFiles = @($fileList | Where-Object { $_.kind -eq "db" })
  $dbRelativePaths = @($dbFiles | ForEach-Object { $_.relativePath })

  # `-wal`/`-shm` 找不到同名主库：只登记文件长度并提示，不当成数据库。
  foreach ($sidecarBase in @($sidecars.Keys)) {
    if ($dbRelativePaths -contains $sidecarBase) { continue }
    Add-Warning $Warnings "orphan_sidecar" "发现没有同名主库的 -wal/-shm 侧文件，仅登记长度。" $sidecarBase
  }

  $databases = [System.Collections.Generic.List[object]]::new()
  foreach ($dbFile in $dbFiles) {
    $baseRelativePath = $dbFile.relativePath
    $walBytes = $null
    $shmBytes = $null
    foreach ($file in $fileList) {
      if ($file.kind -eq "wal" -and $file.relativePath -eq "$baseRelativePath-wal") { $walBytes = $file.lengthBytes }
      if ($file.kind -eq "shm" -and $file.relativePath -eq "$baseRelativePath-shm") { $shmBytes = $file.lengthBytes }
    }

    if ($null -eq $sqliteExe) {
      # 无 sqlite3 时保留库条目骨架：长度可用，元数据显式声明不可用。
      $databases.Add([ordered]@{
          relativePath   = $baseRelativePath
          status         = "unavailable"
          lengthBytes    = $dbFile.lengthBytes
          walBytes       = $walBytes
          shmBytes       = $shmBytes
          pageSize       = $null
          pageCount      = $null
          pageBytes      = $null
          freelistCount  = $null
          freelistBytes  = $null
          journalMode    = $null
          autoVacuum     = $null
          autoVacuumMode = $null
          eventLog       = $null
          message        = "sqlite3 不可用，仅采集文件长度。"
        })
      continue
    }

    $absolutePath = Join-Path $resolvedDir ($baseRelativePath.Replace("/", [System.IO.Path]::DirectorySeparatorChar))
    $databases.Add((Read-SqliteDatabaseMetrics -SqliteExe $sqliteExe -DatabasePath $absolutePath `
          -RelativePath $baseRelativePath -LengthBytes $dbFile.lengthBytes `
          -WalBytes $walBytes -ShmBytes $shmBytes -Warnings $Warnings))
  }
  $result.databases = @($databases)
  return $result
}

function New-RuntimeMetricsReport {
  param(
    [int[]]$Ids = @(),
    [string]$Dir = "",
    [int]$Samples = 1,
    [int]$Interval = 0,
    [int]$Depth = 3
  )
  $warnings = New-Warnings
  $sampleList = [System.Collections.Generic.List[object]]::new()
  for ($index = 1; $index -le $Samples; $index++) {
    $sampleList.Add((Get-ProcessSample -Ids $Ids -Warnings $warnings -Index $index))
    if ($index -lt $Samples -and $Interval -gt 0) { Start-Sleep -Milliseconds $Interval }
  }
  $data = Get-DataMetrics -Dir $Dir -MaxDepth $Depth -Warnings $warnings

  $lastSample = if ($sampleList.Count -gt 0) { $sampleList[$sampleList.Count - 1] } else { $null }
  # 先落到变量：直接写 `processes = if (...) { @(...) }` 会被管道展开成单个对象。
  $processSnapshot = @(if ($null -ne $lastSample) { $lastSample.processes } else { @() })
  return [ordered]@{
    schemaVersion        = $MetricsSchemaVersion
    collectedAt          = (Format-IsoTimestamp ([datetimeoffset]::Now))
    host                 = [ordered]@{
      os             = [System.Runtime.InteropServices.RuntimeInformation]::OSDescription
      platform       = if ($IsWindows) { "windows" } elseif ($IsMacOS) { "macos" } else { "linux" }
      osArchitecture = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString().ToLowerInvariant()
      psVersion      = $PSVersionTable.PSVersion.ToString()
      psEdition      = [string]$PSVersionTable.PSEdition
    }
    sampleCount          = $Samples
    intervalMilliseconds = $Interval
    samples              = @($sampleList)
    # `processes` 恒等于最后一次采样，便于单次读取；观察波动请读 `samples`。
    processes            = $processSnapshot
    data                 = $data
    warnings             = @($warnings)
  }
}

function Format-MetricsReport($Report) {
  $lines = [System.Collections.Generic.List[string]]::new()
  $lines.Add("运行时资源指标（只读采集；脚本不做判定、不做平滑）")
  $lines.Add(("采集时间：{0} | 采样 {1} 次 / 间隔 {2} ms" -f $Report.collectedAt, $Report.sampleCount, $Report.intervalMilliseconds))
  $lines.Add(("主机：{0} | PowerShell {1} ({2}) | {3}" -f `
        (Format-Value $Report.host.os), (Format-Value $Report.host.psVersion), `
        (Format-Value $Report.host.psEdition), (Format-Value $Report.host.osArchitecture)))
  $lines.Add("")

  $lines.Add("[进程]")
  if (@($Report.samples).Count -eq 0 -or @($Report.samples[0].processes).Count -eq 0) {
    $lines.Add("  （未指定 -ProcessId）")
  } else {
    $lines.Add(("  {0} {1,8} {2,-10} {3,-16} {4,10} {5,14} {6,14} {7,6} {8,6}  {9}" -f `
          "样本", "PID", "状态", "进程名", "工作集(B)", "峰值工作集(B)", "私有提交(B)", "句柄", "线程", "启动时间"))
    foreach ($sample in @($Report.samples)) {
      foreach ($process in @($sample.processes)) {
        $lines.Add(("  {0} {1,8} {2,-10} {3,-16} {4,10} {5,14} {6,14} {7,6} {8,6}  {9}" -f `
              (Format-Value $sample.index), (Format-Value $process.processId), (Format-Value $process.status), `
              (Format-Value $process.processName), (Format-Value $process.workingSetBytes), `
              (Format-Value $process.peakWorkingSetBytes), (Format-Value $process.privateMemorySizeBytes), `
              (Format-Value $process.handleCount), (Format-Value $process.threadCount), (Format-Value $process.startTime)))
      }
    }
  }
  $lines.Add("")

  $data = $Report.data
  $lines.Add("[数据目录]")
  if (-not $data.requested) {
    $lines.Add("  （未指定 -DataDir）")
  } elseif (-not $data.exists) {
    $lines.Add(("  {0} -> 不存在或不可访问" -f (Format-Value $data.dir)))
  } else {
    $lines.Add(("  {0}" -f (Format-Value $data.resolvedDir)))
    $lines.Add(("  sqlite3：{0} {1} | 文件 {2} 个（db={3} B, wal={4} B, shm={5} B）" -f `
          (Format-Value $data.sqlite), (Format-Value $data.sqliteVersion), `
          (Format-Value $data.totals.fileCount), (Format-Value $data.totals.dbBytes), `
          (Format-Value $data.totals.walBytes), (Format-Value $data.totals.shmBytes)))
    $lines.Add("")
    $lines.Add("  文件（kind | lengthBytes | relativePath）：")
    if (@($data.files).Count -eq 0) {
      $lines.Add("    （无匹配文件）")
    } else {
      foreach ($file in @($data.files)) {
        $lines.Add(("    {0,-4} {1,14}  {2}" -f $file.kind, (Format-Value $file.lengthBytes), $file.relativePath))
      }
    }
    $lines.Add("")
    $lines.Add("  数据库（pageBytes = pageSize*pageCount；freelistBytes 是其中可回收部分）：")
    if (@($data.databases).Count -eq 0) {
      $lines.Add("    （无 *.db / *.sqlite / *.sqlite3）")
    } else {
      foreach ($database in @($data.databases)) {
        $eventLogText = if ($database.eventLog -is [string]) {
          $database.eventLog
        } elseif ($null -eq $database.eventLog) {
          "-"
        } else {
          "rowCount=$(Format-Value $database.eventLog.rowCount) maxSeq=$(Format-Value $database.eventLog.maxSeq) status=$(Format-Value $database.eventLog.status)"
        }
        $lines.Add(("    {0}" -f $database.relativePath))
        $lines.Add(("      status={0} lengthBytes={1} walBytes={2} shmBytes={3}" -f `
              (Format-Value $database.status), (Format-Value $database.lengthBytes), `
              (Format-Value $database.walBytes), (Format-Value $database.shmBytes)))
        $lines.Add(("      pageSize={0} pageCount={1} pageBytes={2} freelistCount={3} freelistBytes={4}" -f `
              (Format-Value $database.pageSize), (Format-Value $database.pageCount), (Format-Value $database.pageBytes), `
              (Format-Value $database.freelistCount), (Format-Value $database.freelistBytes)))
        $lines.Add(("      journalMode={0} autoVacuum={1} ({2}) eventLog={3}" -f `
              (Format-Value $database.journalMode), (Format-Value $database.autoVacuum), `
              (Format-Value $database.autoVacuumMode), $eventLogText))
        if (-not [string]::IsNullOrWhiteSpace([string]$database.message)) {
          $lines.Add(("      message={0}" -f $database.message))
        }
      }
    }
  }
  $lines.Add("")

  $lines.Add(("[警告] {0} 条" -f @($Report.warnings).Count))
  foreach ($warning in @($Report.warnings)) {
    $lines.Add(("  {0} | {1} | {2}" -f (Format-Value $warning.code), (Format-Value $warning.target), (Format-Value $warning.message)))
  }
  return ($lines -join [Environment]::NewLine)
}

function Add-SelfTestCheck {
  param(
    $Checks,
    [string]$Name,
    [bool]$Ok,
    [string]$Detail = ""
  )
  $Checks.Add([ordered]@{ name = $Name; ok = $Ok; detail = (ConvertTo-SafeMessage $Detail) })
}

# 自检只写系统临时目录，并在 finally 中清理；断言覆盖空目录、仅 -wal 目录、
# 含 event_log 的小库、缺 event_log 分支、进程侧、JSON 往返与相对路径纪律。
function Invoke-SelfTest {
  $checks = [System.Collections.Generic.List[object]]::new()
  $tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("soxu-runtime-metrics-selftest-" + [guid]::NewGuid().ToString("N"))
  $sqliteExe = Resolve-SqliteCommand
  Write-Host "==> 自检：sqlite3 = $(if ($null -eq $sqliteExe) { 'unavailable' } else { Get-SqliteVersion $sqliteExe })" -ForegroundColor Cyan

  try {
    $emptyDir = Join-Path $tempRoot "empty"
    $walOnlyDir = Join-Path $tempRoot "wal-only"
    $eventsDir = Join-Path $tempRoot "with-events"
    $plainDir = Join-Path $tempRoot "without-events"
    foreach ($directory in @($emptyDir, $walOnlyDir, $eventsDir, $plainDir)) {
      New-Item -ItemType Directory -Path $directory -Force | Out-Null
    }
    # 仅 -wal：没有同名主库，用于验证侧文件枚举与 orphan 提示。
    [System.IO.File]::WriteAllText((Join-Path $walOnlyDir "app.db-wal"), ("WAL-FIXTURE-" * 8))

    $sqliteFixturesOk = $true
    if ($null -ne $sqliteExe) {
      $eventsSql = "CREATE TABLE event_log (seq INTEGER PRIMARY KEY AUTOINCREMENT, protocol_version INTEGER NOT NULL, timestamp TEXT NOT NULL, payload_json TEXT NOT NULL);" +
      "INSERT INTO event_log (protocol_version, timestamp, payload_json) VALUES (1, '2026-01-01T00:00:00Z', '{}'), (1, '2026-01-02T00:00:00Z', '{}');"
      $createEvents = Invoke-CapturedTool -FilePath $sqliteExe -Arguments @((Join-Path $eventsDir "app.db"), $eventsSql)
      $plainSql = "CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL); INSERT INTO meta (key, value) VALUES ('schema_version', '1');"
      $createPlain = Invoke-CapturedTool -FilePath $sqliteExe -Arguments @((Join-Path $plainDir "app.db"), $plainSql)
      if ($createEvents.exitCode -ne 0 -or $createPlain.exitCode -ne 0) {
        $sqliteFixturesOk = $false
        Add-Warning (New-Warnings) "selftest_fixture_failed" "无法创建 SQLite 自检夹具。" "selftest"
      }
    } else {
      # 无 sqlite3：只验证文件枚举路径，元数据相关断言分派到 degraded 分支。
      [System.IO.File]::WriteAllText((Join-Path $plainDir "app.db"), "not-a-sqlite-database")
    }

    $warnings = New-Warnings
    $emptyMetrics = Get-DataMetrics -Dir $emptyDir -MaxDepth 3 -Warnings $warnings
    $walMetrics = Get-DataMetrics -Dir $walOnlyDir -MaxDepth 3 -Warnings $warnings
    $eventsMetrics = Get-DataMetrics -Dir $eventsDir -MaxDepth 3 -Warnings $warnings
    $plainMetrics = Get-DataMetrics -Dir $plainDir -MaxDepth 3 -Warnings $warnings

    Add-SelfTestCheck $checks "empty-dir" `
      ($emptyMetrics.exists -and @($emptyMetrics.files).Count -eq 0 -and @($emptyMetrics.databases).Count -eq 0) `
      ("files={0} databases={1}" -f @($emptyMetrics.files).Count, @($emptyMetrics.databases).Count)

    $walFilePath = (Join-Path $walOnlyDir "app.db-wal").Replace($tempRoot, "<temp>")
    Add-SelfTestCheck $checks "wal-only-enumerated" `
      (@($walMetrics.files).Count -eq 1 -and @($walMetrics.files)[0].kind -eq "wal" -and @($walMetrics.databases).Count -eq 0) `
      ("files={0} kind={1} databases={2} path={3}" -f @($walMetrics.files).Count, `
        $(if (@($walMetrics.files).Count -gt 0) { @($walMetrics.files)[0].kind } else { "-" }), `
        @($walMetrics.databases).Count, $walFilePath)
    $orphanWarning = @($warnings | Where-Object { $_.code -eq "orphan_sidecar" })
    Add-SelfTestCheck $checks "wal-only-orphan-warning" ($orphanWarning.Count -ge 1) `
      ("orphan_sidecar={0}" -f $orphanWarning.Count)

    if ($null -ne $sqliteExe -and $sqliteFixturesOk) {
      $eventsDb = @($eventsMetrics.databases)[0]
      $eventLog = $eventsDb.eventLog
      Add-SelfTestCheck $checks "event-log-rowcount" `
        ($eventsDb.status -eq "ok" -and $null -ne $eventLog -and $eventLog -isnot [string] -and $eventLog.rowCount -eq 2) `
        ("status={0} rowCount={1}" -f $eventsDb.status, $(if ($eventLog -is [string]) { $eventLog } else { $eventLog.rowCount }))
      Add-SelfTestCheck $checks "event-log-maxseq" `
        ($null -ne $eventLog -and $eventLog -isnot [string] -and $eventLog.maxSeq -eq 2) `
        ("maxSeq={0}" -f $(if ($eventLog -is [string]) { $eventLog } else { $eventLog.maxSeq }))
      Add-SelfTestCheck $checks "pragma-metadata" `
        ($eventsDb.pageSize -gt 0 -and $eventsDb.pageCount -gt 0 -and $eventsDb.journalMode -ne $null -and $null -ne $eventsDb.autoVacuum -and $null -ne $eventsDb.autoVacuumMode) `
        ("pageSize={0} pageCount={1} journalMode={2} autoVacuum={3}" -f `
          $eventsDb.pageSize, $eventsDb.pageCount, $eventsDb.journalMode, $eventsDb.autoVacuumMode)
      Add-SelfTestCheck $checks "page-bytes-consistent" `
        ($eventsDb.pageBytes -eq ($eventsDb.pageSize * $eventsDb.pageCount) -and `
          $eventsDb.freelistBytes -eq ($eventsDb.pageSize * $eventsDb.freelistCount)) `
        ("pageBytes={0} freelistBytes={1}" -f $eventsDb.pageBytes, $eventsDb.freelistBytes)

      $plainDb = @($plainMetrics.databases)[0]
      Add-SelfTestCheck $checks "event-log-absent" `
        ($plainDb.status -eq "ok" -and $plainDb.eventLog -is [string] -and $plainDb.eventLog -eq "absent") `
        ("status={0} eventLog={1}" -f $plainDb.status, $(if ($plainDb.eventLog -is [string]) { $plainDb.eventLog } else { "object" }))
    } else {
      Add-SelfTestCheck $checks "sqlite-unavailable" ($plainMetrics.sqlite -eq "unavailable" -and @($plainMetrics.files).Count -eq 1) `
        ("sqlite={0} files={1}" -f $plainMetrics.sqlite, @($plainMetrics.files).Count)
    }

    $selfSample = Get-ProcessSample -Ids @($PID) -Warnings (New-Warnings) -Index 1
    $selfEntry = @($selfSample.processes)[0]
    Add-SelfTestCheck $checks "process-current" `
      ($selfEntry.status -eq "running" -and $selfEntry.workingSetBytes -gt 0 -and $selfEntry.threadCount -gt 0) `
      ("status={0} workingSetBytes={1} threads={2}" -f $selfEntry.status, $selfEntry.workingSetBytes, $selfEntry.threadCount)

    $missingWarnings = New-Warnings
    $missingSample = Get-ProcessSample -Ids @(999999) -Warnings $missingWarnings -Index 1
    $missingMetrics = Get-DataMetrics -Dir (Join-Path $tempRoot "does-not-exist") -MaxDepth 3 -Warnings $missingWarnings
    Add-SelfTestCheck $checks "missing-targets-structured" `
      (@($missingSample.processes)[0].status -eq "exited" -and -not $missingMetrics.exists -and @($missingWarnings).Count -ge 2) `
      ("processStatus={0} dirExists={1} warnings={2}" -f @($missingSample.processes)[0].status, $missingMetrics.exists, @($missingWarnings).Count)

    # 隐私纪律：所有文件条目必须是相对路径，不得带盘符、UNC 或前导分隔符。
    $absoluteLeaks = @(@($eventsMetrics.files) + @($walMetrics.files) | Where-Object {
        $_.relativePath -match '^(?:[a-zA-Z]:|/|\\\\)' -or $_.relativePath.Contains($tempRoot)
      })
    Add-SelfTestCheck $checks "relative-paths" ($absoluteLeaks.Count -eq 0) `
      ("offenders={0}" -f $absoluteLeaks.Count)

    $probeReport = New-RuntimeMetricsReport -Ids @($PID) -Dir $eventsDir -Samples 1 -Interval 0 -Depth 3
    $jsonText = $probeReport | ConvertTo-Json -Depth 8
    $parsed = $jsonText | ConvertFrom-Json
    $roundTripOk = $parsed.schemaVersion -eq $MetricsSchemaVersion -and
    @($parsed.processes).Count -eq 1 -and
    @($parsed.samples).Count -eq 1 -and
    $parsed.data.exists -and
    $parsed.PSObject.Properties.Name -contains "warnings"
    Add-SelfTestCheck $checks "json-roundtrip" $roundTripOk `
      ("bytes={0} processes={1} samples={2}" -f $jsonText.Length, @($parsed.processes).Count, @($parsed.samples).Count)
  } finally {
    if (Test-Path -LiteralPath $tempRoot) {
      try { Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction Stop } catch {
        Write-Warning "临时目录清理失败：$(ConvertTo-SafeMessage $_.Exception.Message)"
      }
    }
  }

  $failed = @($checks | Where-Object { -not $_.ok })
  Write-Host ""
  foreach ($check in $checks) {
    $status = if ($check.ok) { "[ok]  " } else { "[fail]" }
    $color = if ($check.ok) { "DarkGray" } else { "Red" }
    Write-Host ("  {0} {1,-30} {2}" -f $status, $check.name, $check.detail) -ForegroundColor $color
  }
  Write-Host ""
  if ($failed.Count -eq 0) {
    Write-Host ("✓ 自检通过：{0}/{1} 项" -f $checks.Count, $checks.Count) -ForegroundColor Green
    return 0
  }
  Write-Host ("❌ 自检失败：{0}/{1} 项" -f $failed.Count, $checks.Count) -ForegroundColor Red
  foreach ($check in $failed) { Write-Host ("   - {0}: {1}" -f $check.name, $check.detail) -ForegroundColor Red }
  return 1
}

if ($SelfTest) {
  exit (Invoke-SelfTest)
}

$report = New-RuntimeMetricsReport -Ids $ProcessId -Dir $DataDir -Samples $SampleCount -Interval $IntervalMilliseconds -Depth $MaxDepth

if ($Json) {
  $report | ConvertTo-Json -Depth 8
} else {
  Write-Output (Format-MetricsReport $report)
}

# 退出码只表达「有没有采到可用测量值」，退化细节一律走 warnings，避免调用方把
# 「进程已退出」误判成脚本故障。
$resolvedProcesses = @(@($report.processes) | Where-Object { $_.status -eq "running" })
$requestedTargets = (@($ProcessId).Count -gt 0) -or $report.data.requested
$dataUsable = $report.data.exists
if ($requestedTargets -and $resolvedProcesses.Count -eq 0 -and -not $dataUsable) {
  exit 3
}
exit 0
