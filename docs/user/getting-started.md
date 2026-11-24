# 快速上手指南

## 1. 运行前提

Soxu 无需单独注册云端账号，直接复用你本机已安装并完成登录的任意一款或多款 AI 编码 CLI：

- Claude Code (`claude`)
- Codex CLI (`codex`)
- Pi (`pi`)
- OpenCode (`opencode`)
- Antigravity CLI (`agy`)
- GitHub Copilot CLI (`copilot`)
- Qwen Code (`qwen`)

---

## 2. Magpie 自动协同说明

- **如果你已安装 Magpie**：Soxu 启动后会自动识别你的 Magpie 模型网关路由组、Skills 路由（`magpie-skills`）与 MCP Hub（`shared` / `direct`），无需重复配置模型密钥或技能路径。
- **如果你未安装 Magpie**：Soxu 会自动切换为独立原生模式，直接扫描已装 CLI 的默认配置与本地历史会话，所有桌面编排与移动端遥控功能均可完整使用。

---

## 3. 常用命令（开发预览期）

```powershell
# 诊断本机 CLI 与 Magpie 发现状态
cargo run -p soxu-cli -- doctor

# 启动本地守护进程（默认监听 127.0.0.1:7698）
cargo run -p soxu-daemon

# 启动 GPUI 桌面端
cargo run -p soxu-desktop
```
