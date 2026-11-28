# 运行时资源基线（2026-11-28 首次实测）

> 工具：`scripts/collect-runtime-metrics.ps1`（PR #162，阶段 0 交付物）。
> 目的：为 `docs/dev/resource-lifecycle-and-retention.md` 的阶段 1–4 提供同一口径的起点数值。
> 纪律：只记录测量值，不做收益换算；预估与实测分开标注；不可外推的组合明确写「未实测」。

## 1. 环境与口径

| 项 | 值 |
| :--- | :--- |
| 主机 | Windows，31.4 GB 物理内存 |
| 二进制 | `target/debug/soxu-daemon.exe`（debug 构建，`main` = `2f3b7e3`） |
| 启动参数 | `SOXU_DATA_DIR=<非系统盘的临时目录>`、`SOXU_LISTEN=127.0.0.1:7798`、`SOXU_DAEMON_FEATURES=controlled_pty` |
| 采样 | `-SampleCount 3 -IntervalMilliseconds 1000`，取最后一次 |
| 进程口径 | `WorkingSet64` / `PeakWorkingSet64` / `PrivateMemorySize64` / 句柄数 / 线程数 |
| 数据口径 | 文件长度 + `page_size` / `page_count` / `freelist_count` + `event_log` 行数与 `MAX(seq)` |

debug 构建的内存与体积**普遍高于** release，下列数值不能直接当作发布版占用；但同口径的前后对比仍然有效。

## 2. 空载 daemon

| 指标 | 实测 |
| :--- | ---: |
| 工作集 `workingSetBytes` | 10.1 MB |
| 峰值工作集 | 10.1 MB |
| 私有字节 `privateMemorySizeBytes` | 2.7 MB |
| 句柄数 | 94 |
| 线程数 | 20 |
| 主库 `soxu-daemon.db` | 4,096 B（WAL 中还有 144,232 B） |
| `page_count` | 23 |

结论：空载常驻开销很低，此前 30–60 MB 的预估偏保守（且口径不同）。

## 3. 事件日志增长（合成事件，直接写库）

合成方式：在 daemon 停止后，用 `sqlite3` 直接向 `event_log` 插入行，随后 `PRAGMA wal_checkpoint(TRUNCATE)` 合并 WAL，再采集。**这不是通过 RPC 产生的真实事件**，只用于测量库体积与页分布。

| 场景 | 行数 | 主库文件 | `page_count` | 空闲页 |
| :--- | ---: | ---: | ---: | ---: |
| 空载基线 | 0 | 4 KB | 23 | 0 |
| 1,000 条小事件（约 200 B/条） | 1,000 | 438 KB | 107 | 0 |
| 再追加 9,000 条 16 KB 负载（模拟 PTY 分片） | 10,000 | **148 MB** | 36,127 | 0 |
| 在上一步库上 `DELETE FROM event_log` | 0 | **仍 148 MB** | 36,127 | **36,104 页（147.9 MB）** |

结论：

- `event_log` 每 16 KB PTY 分片一行，1 万片就是约 148 MB，且当前没有任何删除路径（阶段 3 的动机）。
- `DELETE` 只把页放进 freelist，文件不缩小；没有 `auto_vacuum` 与 `VACUUM` 时磁盘不会归还。`latest_seq = MAX(seq)` 在删空后还会回退到 0，而自增序号继续变大——阶段 3 必须引入独立的 `H`/`P` 双序号。

## 4. 未实测项（如实标注）

| 场景 | 状态 | 原因 |
| :--- | :--- | :--- |
| 1 PTY / 8 PTY 的 daemon RSS 与子进程树 | **未实测** | `soxu-cli` 无 PTY 子命令，需桌面端或 RPC 客户端驱动 |
| 脚本执行的并发与峰值内存 | **未实测** | 同上，需 RPC 触发 `run_script` |
| 桌面 GPUI 空闲/长会话 | **未实测** | 需构建并运行 `soxu-desktop`（GPUI 依赖树重） |
| PWA 页签内存 | **未实测** | 需浏览器长会话录制 |
| release 构建的常驻 | **未实测** | 本次为 debug 二进制 |

上述五项沿用 `docs/dev/resource-lifecycle-and-retention.md` §2 的预估值，直到有新的实测覆盖。

## 5. 复现命令

```powershell
$env:SOXU_DATA_DIR = '<非系统盘的临时目录>\run-<时间戳>'
$env:SOXU_LISTEN = '127.0.0.1:7798'
$env:SOXU_DAEMON_FEATURES = 'controlled_pty'
$p = Start-Process -FilePath '.\target\debug\soxu-daemon.exe' -PassThru -WindowStyle Hidden

pwsh -NoProfile -File scripts/collect-runtime-metrics.ps1 `
  -ProcessId $p.Id -DataDir $env:SOXU_DATA_DIR `
  -SampleCount 3 -IntervalMilliseconds 1000 -Json

Stop-Process -Id $p.Id
```

合成事件与删库测量见本文件 §3 的表格；脚本本身不执行任何写入。

## 6. 与方案文档的关系

- 阶段 0 完成：工具与首批基线已落地。
- 阶段 1（追平分页）合入后需要**重跑 §2**，确认逐页实现没有抬高常驻。
- 阶段 2（PTY 生命周期）合入后必须补齐 §4 的 PTY 场景，否则不能声称「容量与关停已修复」。
- 阶段 3（事件留存）应以 §3 的 148 MB / 36,104 空闲页为对照，给出删行 + checkpoint + `incremental_vacuum` 的实测差值。
