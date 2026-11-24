param(
  [ValidateSet("PreCommit", "CommitMsg", "CiCheck")]
  [string]$Mode = "CiCheck",
  [string]$CommitMsgFile = ""
)

$ErrorActionPreference = "Stop"

# 允许的 Git Emoji + Conventional Commits 正则（兼容 Unicode FE0F 变体选择符）
$CommitPattern = '^(?:🎉|✨|🐛|♻\uFE0F?|⚡\uFE0F?|💄|🎨|📝|✅|🔧|👷|🔒\uFE0F?) (?:init|feat|fix|refactor|perf|ui|style|docs|test|chore|ci|security)(?:\([a-z0-9_/-]+\))?!?: .{4,72}$'
$DisallowedVague = ':\s*(?:wip|update|fix|test|temp|changes|commit)\s*$'

function Test-CommitSubject([string]$Subject, [string]$ContextLabel) {
  if ([string]::IsNullOrWhiteSpace($Subject)) {
    Write-Error "[$ContextLabel] 提交标题不能为空。"
    exit 1
  }
  if ($Subject -match '^Merge\s+' -or $Subject -match '^Revert\s+') {
    return
  }
  if ($Subject -notmatch $CommitPattern -or $Subject -imatch $DisallowedVague) {
    Write-Host "❌ [$ContextLabel] 提交信息不符合 Soxu Git Emoji 规范：$Subject" -ForegroundColor Red
    Write-Host "正确格式：<emoji> <type>(<scope>): <中文简明描述>（描述 4~72 字，禁止 wip/update 等含糊词）" -ForegroundColor Yellow
    Write-Host "示例：" -ForegroundColor Cyan
    Write-Host "  🎉 init: 初始化 Soxu 全栈工作区与工程治理基座"
    Write-Host "  ✨ feat(protocol): 新增增量事件游标与断线追平协议帧"
    Write-Host "  🐛 fix(magpie): 修复未安装 Magpie 时的原生配置目录探测逻辑"
    Write-Host "  💄 ui(desktop): 更新 GPUI 工作台深色主题对比度令牌"
    Write-Host "允许的类型映射：🎉 init | ✨ feat | 🐛 fix | ♻️ refactor | ⚡ perf | 💄 ui | 🎨 style | 📝 docs | ✅ test | 🔧 chore | 👷 ci | 🔒️ security"
    exit 1
  }
}

function Invoke-PrivacyAndFileHygieneCheck([bool]$StagedOnly) {
  $files = if ($StagedOnly) {
    git diff --cached --name-only --diff-filter=ACMR
  } else {
    git ls-files
  }

  $forbiddenFileRegex = '(?i)(^|[\\/])(\.env(\..+)?|redact\.key|local-credential|daemon-keypair\.json|caller-keys\.json|logins\.json|push-tokens\.json|id_rsa|id_ed25519|.*\.(pem|key|p12|pfx))$'
  $personalPathRegex = '(?i)([a-z]:[\\/]+users[\\/]+(?!public\b|default\b|runneradmin\b|example\b|<)[a-z0-9._-]{2,}[\\/]+|(?:/Users/|/home/)(?!runner\b|ubuntu\b|root\b|paseo\b|example\b|user\b|<)[a-z0-9._-]{2,}/)'

  foreach ($f in $files) {
    if ([string]::IsNullOrWhiteSpace($f)) { continue }
    if ($f -eq ".env.example") { continue }

    if ($f -match $forbiddenFileRegex) {
      Write-Error "❌ [隐私/安全拦截] 检测到禁止提交的敏感凭据或密钥文件：$f"
      exit 1
    }

    if (Test-Path -LiteralPath $f -PathType Leaf) {
      $item = Get-Item -LiteralPath $f
      if ($item.Length -gt 5MB) {
        Write-Error "❌ [大文件拦截] 文件超过 5MB 上限，请检查是否误提交构建产物或二进制包：$f ($([math]::Round($item.Length / 1MB, 2)) MB)"
        exit 1
      }

      if ($f -notin @(".gitleaks.toml", "scripts/verify-commits-and-privacy.ps1") -and $item.Extension -notin @(".png", ".ico", ".woff2", ".pdf", ".zip", ".7z")) {
        $raw = Get-Content -LiteralPath $f -Raw -ErrorAction SilentlyContinue
        if ($null -ne $raw -and $raw -match $personalPathRegex) {
          Write-Error "❌ [隐私泄露拦截] 文件 '$f' 中检测到硬编码个人用户目录路径（匹配项：$($Matches[0])），请改用相对路径、环境变量或标准占位符！"
          exit 1
        }
      }
    }
  }

  $gitleaksCmd = Get-Command gitleaks -ErrorAction SilentlyContinue
  if ($gitleaksCmd) {
    if ($StagedOnly) {
      & $gitleaksCmd.Source git --staged --redact --config .gitleaks.toml
    } else {
      & $gitleaksCmd.Source git --redact --config .gitleaks.toml
    }
    if ($LASTEXITCODE -ne 0) {
      Write-Error "❌ [Gitleaks 拦截] 检测到潜在密钥或个人隐私泄露，提交已中止！"
      exit 1
    }
  } elseif ($StagedOnly) {
    Write-Warning "本机未在 PATH 找到 gitleaks，已完成内置路径与敏感文件名检查；强烈建议通过 Scoop 安装 gitleaks。"
  }
}

switch ($Mode) {
  "PreCommit" {
    Invoke-PrivacyAndFileHygieneCheck -StagedOnly $true
    Write-Host "✓ [pre-commit] 隐私防泄露与文件卫生检查通过" -ForegroundColor Green
  }
  "CommitMsg" {
    if (-not (Test-Path -LiteralPath $CommitMsgFile)) {
      Write-Error "未找到提交信息文件：$CommitMsgFile"
      exit 1
    }
    $firstLine = (Get-Content -LiteralPath $CommitMsgFile -Encoding UTF8 | Where-Object { $_ -notmatch '^\s*#' } | Select-Object -First 1)
    Test-CommitSubject -Subject "$firstLine".Trim() -ContextLabel "commit-msg"
    Write-Host "✓ [commit-msg] Git Emoji 提交规范校验通过" -ForegroundColor Green
  }
  "CiCheck" {
    Invoke-PrivacyAndFileHygieneCheck -StagedOnly $false
    if (-not [string]::IsNullOrWhiteSpace($env:PR_TITLE)) {
      Test-CommitSubject -Subject $env:PR_TITLE.Trim() -ContextLabel "PR Title"
    }
    $commits = git log -n 20 --pretty=format:"%s" 2>$null
    foreach ($subject in $commits) {
      if (-not [string]::IsNullOrWhiteSpace($subject)) {
        Test-CommitSubject -Subject $subject.Trim() -ContextLabel "Git History"
      }
    }
    Write-Host "✓ [CI] 提交历史 Git Emoji 规范与全仓隐私检查通过" -ForegroundColor Green
  }
}
