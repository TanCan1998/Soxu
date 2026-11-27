# soxu-daemon

Soxu 核心守护进程：持久化状态机、WebSocket 游标事件流、移动配对鉴权与移动端 PWA 静态资产伺服。

## 运行

```powershell
cargo run -p soxu-daemon
```

- 默认监听 `127.0.0.1:7698`（`SOXU_LISTEN` 覆写）。
- 数据目录默认 `~/.soxu`（`SOXU_DATA_DIR` 覆写）；目录级文件锁防止双实例。
- 非回环监听地址自动要求全部请求携带设备 Bearer 令牌（`require_auth_for_all`）。

## 移动端 PWA 伺服（rust-embed）

守护进程把 `apps/mobile-web/dist` 的构建产物内嵌进二进制：

- 构建前先在 `apps/mobile-web` 下执行 `pnpm run build`；
- `dist` 缺失时 `#[allow_missing = true]` 让守护进程照常编译，访问 `/` 返回 500 与明确的中文构建指引，不 panic；
- 路由语义：`/` 与无扩展路径回退 SPA `index.html`；带扩展名的路径必须真实命中嵌入资产，未知资产返回 404；`/api/*` 与 `/ws` 未注册路径返回协议错误 JSON 404，不被 SPA 回退吞掉；
- 缓存策略：`assets/*`（内容哈希文件名）`immutable` 长缓存，入口与 manifest `must-revalidate`；
- 安全边界：静态资产不含任何守护进程凭据；配对令牌仅经 `/api/rpc` 的 `ClaimPairing` 下发，快照响应不回显令牌。

## 移动端配对

- 桌面端 `CreatePairingOffer` 生成 `#offer=` 一次性扫码链接与 6 位码（5 分钟有效，认领即作废防重放）；
- 移动端凭 `PairingCredentialInput`（扫码令牌或 6 位码）认领后获得持久 Bearer 令牌（仅 SHA-256 哈希落盘）；
- WS 支持经 `Authorization: Bearer` 头或 `?token=` query 携带令牌（移动端浏览器无法自定义 WS header 的降级通道）；
- 设备支持吊销（`RevokePairedDevice`）与空闲自动锁定（`remote_idle_lock_min`）。

## 测试

```powershell
cargo test -p soxu-daemon
```

覆盖 HTTP/WS 端到端、`afterSeq` 增量追平、配对全生命周期、原生驱动只读会话与 PWA 伺服 404/200/鉴权回归。
