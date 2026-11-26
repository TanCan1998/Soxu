# 开发者构建与多 Agent 并行贡献指南

## 1. 环境准备

项目使用 `mise.toml` 统一锁定工具链（Rust `1.99.1` 含 `rustfmt`/`clippy`、Node `26.13.1`、`pnpm 12.21.0`），通过 `disable_tools = ["python", "java", "maven"]` 隔离宿主全局无关或缺失工具，并通过 Scoop 使用 `gitleaks` 执行提交前隐私与密钥扫描。

```powershell
# 1. 一键激活仓库级 Git Hooks 并信任当前工作树的 mise.toml（启用 CARGO_TARGET_DIR 与工具链隔离）
pwsh -NoProfile -File scripts/setup-worktree.ps1

# 2. 核对项目 mise 锁定环境与工具链版本
mise run doctor

# 3. 安装移动端前端依赖并执行全工作区一键检查（自动在 mise 锁定环境内执行）
mise run mobile-install
mise run check-all
```

---

## 2. 多 Agent 并行与 Paseo Worktree 工作流

1. **契约先行**：若需求涉及 `Cargo.toml` 公共依赖、`crates/soxu-protocol`、`crates/soxu-core` 公共接口或 `design-system/soxu/MASTER.md`，先在单流分支完成契约 PR 合入 `main`。
2. **正交扇出（上限 2~3 并行）**：每个具体实现 Issue 在独立的 Paseo Git Worktree（`feat/<issue>-<slug>`）内开发，严格限制在各自 Issue 声明的 `Owned Paths` 内修改。
3. **粒度控制**：单个 Issue 聚焦单一职责，确保单次会话 10~25 分钟内可完成编码、单测与本地 `check-all.ps1` 验证。
4. **提交、PR 与合入回收规范**：
   - 提交标题必须符合 `<emoji> <type>(<scope>): <中文简明描述>`（详见根目录 [`AGENTS.md`](../../AGENTS.md)）；
   - 严禁使用 `--no-verify` 绕过隐私与规范钩子；
   - 新增任何缓存、临时产物或新工具配置时，必须在同一 PR 同步更新 `.gitignore` 与 `docs/ops/PROGRESS.md`；
   - PR 经 CI 全绿后统一使用 `gh pr merge <num> --squash --delete-branch` 合入 `main`，并立即归档清理对应的 Paseo Worktree 与本地已合入分支，不保留冗余分支历史。
