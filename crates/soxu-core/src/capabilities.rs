//! 7 大原生 Agent CLI 能力协商矩阵与 `lost-controls` 自动回退。

use soxu_protocol::{
    ActiveTurnBehavior, AgentCapabilityDescriptor, LostControlNotice, ModelDescriptor,
    PermissionLevel, PermissionOptionMeta, PermissionTone, ThinkingEffort,
};

/// 返回思考深度等级的中文展示名称。
#[must_use]
pub const fn thinking_label_zh(effort: ThinkingEffort) -> &'static str {
    match effort {
        ThinkingEffort::Low => "低",
        ThinkingEffort::Medium => "中",
        ThinkingEffort::High => "高",
        ThinkingEffort::Xhigh => "极高",
        ThinkingEffort::Max => "最高",
    }
}

/// 返回执行权限档位的中文展示名称（随代理不同提供合理默认中文标签）。
#[must_use]
pub const fn permission_label_zh(level: PermissionLevel) -> &'static str {
    match level {
        PermissionLevel::Plan => "规划确认",
        PermissionLevel::ReadOnly => "只读分析",
        PermissionLevel::Ask => "询问执行",
        PermissionLevel::AutoEdit => "自动编辑",
        PermissionLevel::FullAccess => "完全授权",
        PermissionLevel::Turbo => "自主运行",
    }
}

/// 返回内建默认模型路由目录（与前端设计原型保持 1:1 一致）。
#[must_use]
pub fn default_model_catalog() -> Vec<ModelDescriptor> {
    vec![
        ModelDescriptor {
            model_id: "group/gpt-6.2-astra".to_owned(),
            display_name: "gpt-6.2-astra".to_owned(),
            tier: "复杂架构".to_owned(),
            note: "协同路由".to_owned(),
            context_window_limit: 256_000,
            thinking_options: vec![
                ThinkingEffort::Medium,
                ThinkingEffort::High,
                ThinkingEffort::Xhigh,
            ],
            native_direct_only_agent: None,
        },
        ModelDescriptor {
            model_id: "group/claude-opus-5.5".to_owned(),
            display_name: "claude-opus-5.5".to_owned(),
            tier: "复杂架构".to_owned(),
            note: "协同路由".to_owned(),
            context_window_limit: 200_000,
            thinking_options: vec![
                ThinkingEffort::Medium,
                ThinkingEffort::High,
                ThinkingEffort::Max,
            ],
            native_direct_only_agent: None,
        },
        ModelDescriptor {
            model_id: "gemini-4-carbon".to_owned(),
            display_name: "gemini-4-carbon".to_owned(),
            tier: "复杂架构".to_owned(),
            note: "原生直连".to_owned(),
            context_window_limit: 1_000_000,
            thinking_options: vec![ThinkingEffort::High, ThinkingEffort::Max],
            native_direct_only_agent: Some("agy".to_owned()),
        },
        ModelDescriptor {
            model_id: "group/gpt-6.2-sol".to_owned(),
            display_name: "gpt-6.2-sol".to_owned(),
            tier: "主力开发".to_owned(),
            note: "协同路由".to_owned(),
            context_window_limit: 200_000,
            thinking_options: vec![
                ThinkingEffort::Low,
                ThinkingEffort::Medium,
                ThinkingEffort::High,
            ],
            native_direct_only_agent: None,
        },
        ModelDescriptor {
            model_id: "group/claude-sonnet-5.5".to_owned(),
            display_name: "claude-sonnet-5.5".to_owned(),
            tier: "主力开发".to_owned(),
            note: "协同路由".to_owned(),
            context_window_limit: 200_000,
            thinking_options: vec![
                ThinkingEffort::Low,
                ThinkingEffort::Medium,
                ThinkingEffort::High,
            ],
            native_direct_only_agent: None,
        },
        ModelDescriptor {
            model_id: "auto/deepseek-v4.1-flash-1017".to_owned(),
            display_name: "deepseek-v4.1-flash-1017".to_owned(),
            tier: "轻量巡检".to_owned(),
            note: "协同路由".to_owned(),
            context_window_limit: 128_000,
            thinking_options: vec![ThinkingEffort::Low, ThinkingEffort::Medium],
            native_direct_only_agent: None,
        },
        ModelDescriptor {
            model_id: "auto/gemini-4-hydrogen".to_owned(),
            display_name: "gemini-4-hydrogen".to_owned(),
            tier: "轻量巡检".to_owned(),
            note: "协同路由".to_owned(),
            context_window_limit: 256_000,
            thinking_options: vec![ThinkingEffort::Low, ThinkingEffort::Medium],
            native_direct_only_agent: None,
        },
    ]
}

fn perm_meta(
    id: PermissionLevel,
    label: &str,
    description: &str,
    tone: PermissionTone,
) -> PermissionOptionMeta {
    PermissionOptionMeta {
        id,
        label: label.to_owned(),
        description: description.to_owned(),
        tone,
    }
}

/// 返回 7 大原生 CLI 的默认能力协商矩阵（初始 `installed` 为 `false`，由 M2 探测更新）。
#[must_use]
pub fn default_agent_catalog() -> Vec<AgentCapabilityDescriptor> {
    vec![
        AgentCapabilityDescriptor {
            agent_id: "agy".to_owned(),
            display_name: "Agy".to_owned(),
            binary_name: "agy".to_owned(),
            installed: false,
            version: None,
            supported_models: vec![
                "gemini-4-carbon".to_owned(),
                "group/claude-opus-5.5".to_owned(),
                "group/claude-sonnet-5.5".to_owned(),
                "group/gpt-6.2-astra".to_owned(),
                "auto/gemini-4-hydrogen".to_owned(),
            ],
            permissions: vec![
                perm_meta(
                    PermissionLevel::Plan,
                    "规划确认",
                    "先产出完整执行方案再人工拍板",
                    PermissionTone::Neutral,
                ),
                perm_meta(
                    PermissionLevel::Ask,
                    "询问执行",
                    "写文件与执行命令均逐项确认",
                    PermissionTone::Accent,
                ),
                perm_meta(
                    PermissionLevel::AutoEdit,
                    "自动编辑",
                    "工作区内编辑放行，终端命令需确认",
                    PermissionTone::Warning,
                ),
                perm_meta(
                    PermissionLevel::Turbo,
                    "自主运行",
                    "隔离工作树内自动推进，仅高危拦截",
                    PermissionTone::Danger,
                ),
            ],
            supports_thinking: true,
            supports_live_steer: false,
        },
        AgentCapabilityDescriptor {
            agent_id: "codex".to_owned(),
            display_name: "Codex".to_owned(),
            binary_name: "codex".to_owned(),
            installed: false,
            version: None,
            supported_models: vec![
                "group/gpt-6.2-astra".to_owned(),
                "group/gpt-6.2-sol".to_owned(),
                "group/claude-sonnet-5.5".to_owned(),
                "auto/deepseek-v4.1-flash-1017".to_owned(),
            ],
            permissions: vec![
                perm_meta(
                    PermissionLevel::ReadOnly,
                    "只读分析",
                    "禁止写入任何工作区文件或执行改动命令",
                    PermissionTone::Neutral,
                ),
                perm_meta(
                    PermissionLevel::Ask,
                    "询问确认",
                    "文件写入或命令执行前请求人工确认",
                    PermissionTone::Accent,
                ),
                perm_meta(
                    PermissionLevel::AutoEdit,
                    "自动写入",
                    "工作区内修改自动写入，危险命令仍确认",
                    PermissionTone::Warning,
                ),
                perm_meta(
                    PermissionLevel::FullAccess,
                    "完全授权",
                    "当前工作树全权自主运行（受红线保护）",
                    PermissionTone::Danger,
                ),
            ],
            supports_thinking: true,
            supports_live_steer: true,
        },
        AgentCapabilityDescriptor {
            agent_id: "claude".to_owned(),
            display_name: "Claude".to_owned(),
            binary_name: "claude".to_owned(),
            installed: false,
            version: None,
            supported_models: vec![
                "group/claude-opus-5.5".to_owned(),
                "group/claude-sonnet-5.5".to_owned(),
                "group/gpt-6.2-sol".to_owned(),
                "auto/deepseek-v4.1-flash-1017".to_owned(),
            ],
            permissions: vec![
                perm_meta(
                    PermissionLevel::Plan,
                    "只读规划",
                    "只读取和制定方案，不执行磁盘写入",
                    PermissionTone::Neutral,
                ),
                perm_meta(
                    PermissionLevel::Ask,
                    "每次询问",
                    "严格遵从交互式写入与执行确认",
                    PermissionTone::Accent,
                ),
                perm_meta(
                    PermissionLevel::AutoEdit,
                    "自动接受编辑",
                    "代码修改自动应用，命令执行需审批",
                    PermissionTone::Warning,
                ),
                perm_meta(
                    PermissionLevel::FullAccess,
                    "跳过常规确认",
                    "隔离环境自主推进（高危命令仍拦截）",
                    PermissionTone::Danger,
                ),
            ],
            supports_thinking: true,
            supports_live_steer: true,
        },
        AgentCapabilityDescriptor {
            agent_id: "pi".to_owned(),
            display_name: "Pi".to_owned(),
            binary_name: "pi".to_owned(),
            installed: false,
            version: None,
            supported_models: vec![
                "group/gpt-6.2-sol".to_owned(),
                "group/claude-sonnet-5.5".to_owned(),
                "auto/deepseek-v4.1-flash-1017".to_owned(),
                "auto/gemini-4-hydrogen".to_owned(),
            ],
            permissions: vec![
                perm_meta(
                    PermissionLevel::Ask,
                    "每次询问",
                    "执行关键写操作前请求人工授权",
                    PermissionTone::Accent,
                ),
                perm_meta(
                    PermissionLevel::AutoEdit,
                    "自动编辑",
                    "自动写入代码变更并审批外围命令",
                    PermissionTone::Warning,
                ),
                perm_meta(
                    PermissionLevel::FullAccess,
                    "工作树全权",
                    "多步并行代理全自动编排运行",
                    PermissionTone::Danger,
                ),
            ],
            supports_thinking: true,
            supports_live_steer: true,
        },
        AgentCapabilityDescriptor {
            agent_id: "opencode".to_owned(),
            display_name: "OpenCode".to_owned(),
            binary_name: "opencode".to_owned(),
            installed: false,
            version: None,
            supported_models: vec![
                "group/gpt-6.2-sol".to_owned(),
                "group/claude-sonnet-5.5".to_owned(),
                "auto/deepseek-v4.1-flash-1017".to_owned(),
            ],
            permissions: vec![
                perm_meta(
                    PermissionLevel::Plan,
                    "仅规划分析",
                    "仅可查阅代码与输出结构化规划",
                    PermissionTone::Neutral,
                ),
                perm_meta(
                    PermissionLevel::Ask,
                    "每次询问",
                    "修改前需确认",
                    PermissionTone::Accent,
                ),
                perm_meta(
                    PermissionLevel::AutoEdit,
                    "自动编辑",
                    "仅自动应用局部代码补丁",
                    PermissionTone::Warning,
                ),
            ],
            supports_thinking: true,
            supports_live_steer: false,
        },
        AgentCapabilityDescriptor {
            agent_id: "copilot".to_owned(),
            display_name: "Copilot".to_owned(),
            binary_name: "copilot".to_owned(),
            installed: false,
            version: None,
            supported_models: vec![
                "group/gpt-6.2-sol".to_owned(),
                "group/claude-sonnet-5.5".to_owned(),
            ],
            permissions: vec![
                perm_meta(
                    PermissionLevel::Ask,
                    "每次询问",
                    "默认询问确认模式",
                    PermissionTone::Accent,
                ),
                perm_meta(
                    PermissionLevel::AutoEdit,
                    "自动编辑",
                    "工作区局部写入免确认",
                    PermissionTone::Warning,
                ),
            ],
            supports_thinking: false,
            supports_live_steer: false,
        },
        AgentCapabilityDescriptor {
            agent_id: "qwen".to_owned(),
            display_name: "Qwen".to_owned(),
            binary_name: "qwen".to_owned(),
            installed: false,
            version: None,
            supported_models: vec![
                "auto/deepseek-v4.1-flash-1017".to_owned(),
                "group/gpt-6.2-sol".to_owned(),
            ],
            permissions: vec![
                perm_meta(
                    PermissionLevel::ReadOnly,
                    "只读分析",
                    "只读检索模式",
                    PermissionTone::Neutral,
                ),
                perm_meta(
                    PermissionLevel::Ask,
                    "每次询问",
                    "交互式授权",
                    PermissionTone::Accent,
                ),
            ],
            supports_thinking: false,
            supports_live_steer: false,
        },
    ]
}

/// 切换代理或模型时的能力协商输入请求。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconcileRequest {
    pub agent_id: String,
    pub current_model: Option<String>,
    pub current_perm: Option<PermissionLevel>,
    pub current_thinking: Option<ThinkingEffort>,
    pub current_behavior: ActiveTurnBehavior,
}

/// 能力协商与失效控制项自动回退结果（`lost-controls` Recovery）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconcileOutcome {
    pub agent_id: String,
    pub model: String,
    pub permission: PermissionLevel,
    pub thinking: Option<ThinkingEffort>,
    pub behavior: ActiveTurnBehavior,
    pub lost_controls: Vec<LostControlNotice>,
}

/// 按目标代理与模型目录协商有效控制项，并对失效参数生成可解释回退提示。
#[must_use]
pub fn reconcile_capabilities(
    agents: &[AgentCapabilityDescriptor],
    models: &[ModelDescriptor],
    req: ReconcileRequest,
) -> ReconcileOutcome {
    let profile = agents
        .iter()
        .find(|a| a.agent_id == req.agent_id)
        .or_else(|| agents.iter().find(|a| a.agent_id == "codex"))
        .or_else(|| agents.first());

    let Some(profile) = profile else {
        return ReconcileOutcome {
            agent_id: req.agent_id,
            model: req
                .current_model
                .unwrap_or_else(|| "group/gpt-6.2-sol".to_owned()),
            permission: req.current_perm.unwrap_or(PermissionLevel::Ask),
            thinking: req.current_thinking,
            behavior: req.current_behavior,
            lost_controls: Vec::new(),
        };
    };

    let mut lost_controls = Vec::new();
    let default_model = profile
        .supported_models
        .first()
        .or_else(|| models.first().map(|m| &m.model_id))
        .cloned()
        .unwrap_or_else(|| "group/gpt-6.2-sol".to_owned());

    let resolved_model = match req.current_model {
        Some(ref candidate) if profile.supported_models.iter().any(|m| m == candidate) => {
            candidate.clone()
        }
        Some(prev) => {
            lost_controls.push(LostControlNotice {
                field: "model".to_owned(),
                previous_value: prev,
                fallback_value: default_model.clone(),
                reason_zh: format!(
                    "已将模型切换为 {} 支持的 {}",
                    profile.display_name, default_model
                ),
            });
            default_model
        }
        None => default_model,
    };

    let fallback_perm = profile
        .permissions
        .iter()
        .find(|p| p.id == PermissionLevel::Ask)
        .or_else(|| profile.permissions.first())
        .map_or((PermissionLevel::Ask, "询问执行"), |p| {
            (p.id, p.label.as_str())
        });

    let resolved_perm = match req.current_perm {
        Some(candidate) if profile.permissions.iter().any(|p| p.id == candidate) => candidate,
        Some(prev) => {
            lost_controls.push(LostControlNotice {
                field: "permission".to_owned(),
                previous_value: permission_label_zh(prev).to_owned(),
                fallback_value: fallback_perm.1.to_owned(),
                reason_zh: format!("权限已自适应调整为「{}」", fallback_perm.1),
            });
            fallback_perm.0
        }
        None => fallback_perm.0,
    };

    let model_meta = models.iter().find(|m| m.model_id == resolved_model);
    let resolved_thinking = if profile.supports_thinking {
        let supported_options = model_meta
            .map(|m| m.thinking_options.as_slice())
            .unwrap_or(&[]);
        if supported_options.is_empty() {
            req.current_thinking
        } else {
            let default_effort = if supported_options.contains(&ThinkingEffort::Medium) {
                ThinkingEffort::Medium
            } else {
                supported_options[0]
            };
            match req.current_thinking {
                Some(candidate) if supported_options.contains(&candidate) => Some(candidate),
                Some(prev) => {
                    lost_controls.push(LostControlNotice {
                        field: "thinking".to_owned(),
                        previous_value: thinking_label_zh(prev).to_owned(),
                        fallback_value: thinking_label_zh(default_effort).to_owned(),
                        reason_zh: format!(
                            "已因切换至 {} 自动调整不支持的思考档位 ({} -> {})",
                            resolved_model,
                            thinking_label_zh(prev),
                            thinking_label_zh(default_effort)
                        ),
                    });
                    Some(default_effort)
                }
                None => Some(default_effort),
            }
        }
    } else {
        if let Some(prev) = req.current_thinking {
            lost_controls.push(LostControlNotice {
                field: "thinking".to_owned(),
                previous_value: thinking_label_zh(prev).to_owned(),
                fallback_value: "不支持".to_owned(),
                reason_zh: format!(
                    "{} 暂不支持调节思考强度，已隐藏思考档位",
                    profile.display_name
                ),
            });
        }
        None
    };

    let resolved_behavior =
        if !profile.supports_live_steer && req.current_behavior == ActiveTurnBehavior::Steer {
            lost_controls.push(LostControlNotice {
                field: "behavior".to_owned(),
                previous_value: "steer".to_owned(),
                fallback_value: "queue".to_owned(),
                reason_zh: format!(
                    "{} 暂不支持运行中热纠偏，投递已调整为排队",
                    profile.display_name
                ),
            });
            ActiveTurnBehavior::Queue
        } else {
            req.current_behavior
        };

    ReconcileOutcome {
        agent_id: profile.agent_id.clone(),
        model: resolved_model,
        permission: resolved_perm,
        thinking: resolved_thinking,
        behavior: resolved_behavior,
        lost_controls,
    }
}
