//! 高危破坏性命令拦截、本机原生 CLI/Magpie 全局配置保护与审批幂等校验。

use soxu_protocol::{ActionableError, ApprovalItemSnapshot, ApprovalResolutionKind, ErrorCode};

/// 安全红线评估结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedlineAssessment {
    pub high_risk: bool,
    /// 命中任意高危红线时必须强制为 `false`，禁止「本会话允许」。
    pub allow_session_permitted: bool,
    pub matched_rules: Vec<String>,
    pub risk_note_zh: String,
}

fn contains_token(tokens: &[&str], target: &str) -> bool {
    tokens.contains(&target)
}

/// 对拟执行的命令或目标路径执行确定性高危红线评估。
#[must_use]
pub fn assess_command_safety(tool_kind: &str, command_or_target: &str) -> RedlineAssessment {
    let normalized = command_or_target.to_ascii_lowercase().replace('\\', "/");
    let tokens: Vec<&str> = normalized.split_whitespace().collect();
    let mut matched_rules = Vec::new();

    let has_rm = contains_token(&tokens, "rm")
        && (contains_token(&tokens, "-rf")
            || contains_token(&tokens, "-fr")
            || (contains_token(&tokens, "-r") && contains_token(&tokens, "-f"))
            || (normalized.contains("--recursive") && normalized.contains("--force")));
    let has_ps_remove = normalized.contains("remove-item") && normalized.contains("-recurse");
    let has_win_rmdir = contains_token(&tokens, "rmdir") && contains_token(&tokens, "/s");
    let has_win_del = contains_token(&tokens, "del") && contains_token(&tokens, "/s");
    let has_git_clean = normalized.contains("git clean")
        && tokens
            .iter()
            .any(|t| t.starts_with('-') && t.contains('f') && t.contains('d'));

    if has_rm || has_ps_remove || has_win_rmdir || has_win_del || has_git_clean {
        matched_rules.push("destructive-recursive-delete".to_owned());
    }

    let has_git_force_push = normalized.contains("git push")
        && (contains_token(&tokens, "--force") || contains_token(&tokens, "-f"));
    let has_git_reset_hard = normalized.contains("git reset") && contains_token(&tokens, "--hard");
    let has_repo_delete = normalized.contains("gh repo delete");

    if has_git_force_push || has_git_reset_hard || has_repo_delete {
        matched_rules.push("destructive-git-remote-or-history".to_owned());
    }

    if normalized.contains("format-volume")
        || normalized.contains("set-executionpolicy")
        || normalized.contains("mkfs.")
        || normalized.contains("dd if=")
    {
        matched_rules.push("system-configuration-or-disk-mutation".to_owned());
    }

    const PROTECTED_CONFIG_MARKERS: [&str; 9] = [
        "~/.claude",
        "/.claude/",
        "~/.codex",
        "/.codex/",
        "~/.pi",
        "/.pi/",
        "~/.config/opencode",
        "/.config/opencode/",
        "~/.config/magpie",
    ];
    const PROTECTED_SECRET_FILES: [&str; 5] = [
        "/.config/magpie/",
        "applied.json",
        "redact.key",
        "local-credential",
        "daemon-keypair.json",
    ];

    if PROTECTED_CONFIG_MARKERS
        .iter()
        .any(|marker| normalized.contains(marker))
        || PROTECTED_SECRET_FILES
            .iter()
            .any(|marker| normalized.contains(marker))
    {
        matched_rules.push("global-cli-or-magpie-config-protection".to_owned());
    }

    let high_risk = !matched_rules.is_empty();
    let risk_note_zh = if high_risk {
        format!(
            "命中高危红线（{} / {}）：已强制禁用「本会话允许」，请审慎核对影响范围或选择拒绝并指导改用临时隔离目录。",
            tool_kind,
            matched_rules.join(", ")
        )
    } else {
        "常规工作区操作：未命中全局配置保护或破坏性红线，可按需选择允许一次或本会话放行。"
            .to_owned()
    };

    RedlineAssessment {
        high_risk,
        allow_session_permitted: !high_risk,
        matched_rules,
        risk_note_zh,
    }
}

/// 校验客户端（桌面端或移动 PWA）提交的审批决议是否合法且满足幂等与红线约束。
pub fn validate_approval_resolution(
    approval: &ApprovalItemSnapshot,
    resolution: ApprovalResolutionKind,
) -> Result<(), ActionableError> {
    if let Some(existing) = &approval.audit {
        return Err(ActionableError::new(
            ErrorCode::Conflict,
            "该权限请求已被其他终端处理",
            "审批状态保持最新已决记录，无需重复操作",
            format!("请刷新查看来自 {} 的处理结果", existing.resolved_by),
        ));
    }

    if matches!(resolution, ApprovalResolutionKind::AllowSession)
        && (approval.high_risk || !approval.allow_session_permitted)
    {
        return Err(ActionableError::new(
            ErrorCode::SafetyRedlineBlocked,
            "命中高危红线：禁止本会话自动放行",
            "当前审批请求仍处于待人工裁决状态",
            "请明确选择「允许一次」或「拒绝并指导修改」",
        ));
    }

    Ok(())
}
