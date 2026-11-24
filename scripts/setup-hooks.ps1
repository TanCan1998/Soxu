$ErrorActionPreference = "Stop"
git config core.hooksPath .githooks
Write-Host "✓ 已启用仓库级 .githooks（主工作区与所有 Git Worktree 自动继承生效）" -ForegroundColor Green
