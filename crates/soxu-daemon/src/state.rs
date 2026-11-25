//! 守护进程核心运行态、RPC 命令调度器与单调游标广播总线。

use crate::pairing::PairingManager;
use crate::store::SqliteStore;
use anyhow::Result;
use chrono::Utc;
use soxu_core::{CoreOrchestrator, DraftSaveOutcome, ReconcileRequest};
use soxu_magpie::{EnvironmentMode, discover_environment};
use soxu_protocol::{
    ActionableError, ActiveTurnBehavior, ApprovalAuditRecord, ApprovalItemSnapshot, DaemonEvent,
    ErrorCode, EventEnvelope, HandoffAnchorCard, HealthSnapshot, PROTOCOL_VERSION,
    PairedDeviceSummary, PendingUserInputRequest, ProjectSummary, QueuedMessageItem,
    QuickChatSummary, RpcCommand, RpcRequest, RpcResponse, RpcResultData, SessionSummary,
    SessionTarget, StateSnapshot, SubagentSummary, TurnLifecycleState, TurnMessageRole,
    TurnMessageSnapshot, WorkspaceBucket, WorkspaceSummary,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tokio::sync::broadcast;
use uuid::Uuid;

mod native;

/// 默认本机 Magpie 配置目录探测（优先 `USERPROFILE`，未设置时取 `HOME`）。
#[must_use]
pub fn default_magpie_config_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(|home| PathBuf::from(home).join(".config").join("magpie"))
}

/// 检测本机是否启用 Magpie 协同模式。
#[must_use]
pub fn detect_local_magpie() -> bool {
    discover_environment().mode == EnvironmentMode::MagpieManaged
}

/// 守护进程共享运行时核心。
pub struct DaemonState {
    store: SqliteStore,
    orchestrator: Mutex<CoreOrchestrator>,
    pairing: Mutex<PairingManager>,
    subagents: Mutex<BTreeMap<String, SubagentSummary>>,
    pending_inputs: Mutex<BTreeMap<String, PendingUserInputRequest>>,
    events_tx: broadcast::Sender<EventEnvelope<DaemonEvent>>,
    event_order: Mutex<()>,
    native_turns: Option<native::NativeTurns>,
    daemon_fingerprint: String,
    magpie_detected: bool,
    require_auth_for_all: bool,
}

impl DaemonState {
    /// 打开或初始化持久化守护进程状态；传 `None` 时启用纯内存测试库。
    pub fn open(
        data_dir: Option<&Path>,
        endpoint_base_url: &str,
        require_auth_for_all: bool,
    ) -> Result<Self> {
        let store = match data_dir {
            Some(dir) => SqliteStore::open(&dir.join("soxu-daemon.db"))?,
            None => SqliteStore::open_in_memory()?,
        };
        let daemon_fingerprint = store.get_or_create_daemon_fingerprint()?;
        let pairing = PairingManager::new(daemon_fingerprint.clone(), endpoint_base_url.to_owned());
        let (events_tx, _) = broadcast::channel(512);
        let magpie_detected = detect_local_magpie();

        Ok(Self {
            store,
            orchestrator: Mutex::new(CoreOrchestrator::new()),
            pairing: Mutex::new(pairing),
            subagents: Mutex::new(BTreeMap::new()),
            pending_inputs: Mutex::new(BTreeMap::new()),
            events_tx,
            event_order: Mutex::new(()),
            native_turns: None,
            daemon_fingerprint,
            magpie_detected,
            require_auth_for_all,
        })
    }

    #[must_use]
    pub fn daemon_fingerprint(&self) -> &str {
        &self.daemon_fingerprint
    }

    #[must_use]
    pub fn magpie_detected(&self) -> bool {
        self.magpie_detected
    }

    #[must_use]
    pub fn require_auth_for_all(&self) -> bool {
        self.require_auth_for_all
    }

    #[must_use]
    pub fn health(&self) -> HealthSnapshot {
        HealthSnapshot {
            status: "ok".to_owned(),
            daemon_version: env!("CARGO_PKG_VERSION").to_owned(),
            protocol_version: PROTOCOL_VERSION,
            magpie_detected: self.magpie_detected,
        }
    }

    #[must_use]
    pub fn subscribe_events(&self) -> broadcast::Receiver<EventEnvelope<DaemonEvent>> {
        self.events_tx.subscribe()
    }

    pub fn latest_seq(&self) -> Result<u64, ActionableError> {
        self.store.latest_seq().map_err(internal_err)
    }

    pub fn events_after(
        &self,
        after_seq: u64,
        limit: u32,
    ) -> Result<(Vec<EventEnvelope<DaemonEvent>>, u64), ActionableError> {
        let events = self
            .store
            .query_events_after(after_seq, limit)
            .map_err(internal_err)?;
        let latest = self.store.latest_seq().map_err(internal_err)?;
        Ok((events, latest))
    }

    pub fn snapshot(&self) -> Result<StateSnapshot, ActionableError> {
        let (agents, models) = {
            let orch = self
                .orchestrator
                .lock()
                .map_err(|_| lock_err("orchestrator"))?;
            (orch.agents().to_vec(), orch.models().to_vec())
        };
        let subagents = self
            .subagents
            .lock()
            .map_err(|_| lock_err("subagents"))?
            .values()
            .cloned()
            .collect();
        let pending_inputs = self
            .pending_inputs
            .lock()
            .map_err(|_| lock_err("pending_inputs"))?
            .values()
            .cloned()
            .collect();
        self.store
            .load_snapshot(&agents, &models, subagents, pending_inputs)
            .map_err(internal_err)
    }

    /// 校验远程设备 Bearer 令牌（含未吊销、存在性及空闲锁定超时检查）。
    pub fn authenticate_bearer(
        &self,
        bearer_token: &str,
    ) -> Result<PairedDeviceSummary, ActionableError> {
        let hash = PairingManager::hash_token(bearer_token);
        let device = self
            .store
            .find_device_by_token_hash(&hash)
            .map_err(internal_err)?
            .ok_or_else(|| {
                ActionableError::new(
                    ErrorCode::Unauthorized,
                    "设备访问令牌无效或未配对",
                    "无法访问守护进程受保护资源",
                    "请重新扫描桌面端配对二维码获取有效凭证",
                )
            })?;

        if device.revoked {
            return Err(ActionableError::new(
                ErrorCode::Unauthorized,
                "该远程设备权限已被桌面端吊销",
                "当前连接已拒绝服务",
                "请在桌面端设置中重新配对设备",
            ));
        }

        let prefs = self.store.load_preferences().map_err(internal_err)?;
        let now = Utc::now();
        if PairingManager::is_idle_locked(device.last_seen_at, prefs.remote_idle_lock_min, now) {
            return Err(ActionableError::new(
                ErrorCode::Unauthorized,
                "远程控制会话已因空闲超时自动锁定",
                "出于安全保护已暂停远程操控",
                "请重新配对或刷新授权以解锁会话",
            ));
        }

        let updated = self
            .store
            .touch_device_seen(&device.device_id, now, true)
            .map_err(internal_err)?
            .unwrap_or(device);
        Ok(updated)
    }

    /// 校验 WS 握手 `device_id`（若客户端上报了设备 ID）。
    pub fn verify_device_not_revoked(
        &self,
        device_id: &str,
    ) -> Result<PairedDeviceSummary, ActionableError> {
        let device = self
            .store
            .get_device(device_id)
            .map_err(internal_err)?
            .ok_or_else(|| {
                ActionableError::new(
                    ErrorCode::Unauthorized,
                    "未识别的远程设备身份",
                    "WebSocket 连接已被拒绝",
                    "请重新完成配对握手",
                )
            })?;
        if device.revoked {
            return Err(ActionableError::new(
                ErrorCode::Unauthorized,
                "该远程设备已被吊销",
                "WebSocket 连接已被拒绝",
                "请在桌面端重新配对",
            ));
        }
        Ok(device)
    }

    /// 注册项目或工作区记录（供初始化或外部模块写入）。
    pub fn upsert_project(&self, project: ProjectSummary) -> Result<(), ActionableError> {
        self.store.upsert_project(&project).map_err(internal_err)?;
        self.emit(DaemonEvent::ProjectUpdated(project))?;
        Ok(())
    }

    pub fn upsert_workspace(&self, workspace: WorkspaceSummary) -> Result<(), ActionableError> {
        self.store
            .upsert_workspace(&workspace)
            .map_err(internal_err)?;
        self.emit(DaemonEvent::WorkspaceUpdated(workspace))?;
        Ok(())
    }

    /// 注册待人工确认的权限审批卡（强制执行安全红线评估并持久化）。
    pub fn register_pending_approval(
        &self,
        mut approval: ApprovalItemSnapshot,
    ) -> Result<ApprovalItemSnapshot, ActionableError> {
        let assessment = {
            let orch = self
                .orchestrator
                .lock()
                .map_err(|_| lock_err("orchestrator"))?;
            orch.assess_safety(&approval.tool_kind, &approval.command_or_target)
        };
        if assessment.high_risk {
            approval.high_risk = true;
            approval.allow_session_permitted = false;
            if approval.risk_note.is_empty() {
                approval.risk_note = assessment.risk_note_zh;
            }
        }
        self.store
            .upsert_approval(&approval)
            .map_err(internal_err)?;
        self.update_target_bucket(
            &approval.target,
            Some(&approval.session_id),
            WorkspaceBucket::NeedsInput,
        )?;
        self.emit(DaemonEvent::ApprovalRequested(approval.clone()))?;
        Ok(approval)
    }

    /// 注册结构化提问请求卡。
    pub fn register_pending_user_input(
        &self,
        req: PendingUserInputRequest,
    ) -> Result<PendingUserInputRequest, ActionableError> {
        self.pending_inputs
            .lock()
            .map_err(|_| lock_err("pending_inputs"))?
            .insert(req.request_id.clone(), req.clone());
        self.emit(DaemonEvent::UserInputRequested(req.clone()))?;
        Ok(req)
    }

    /// 注册子代理委派条目（支持后续 `DetachSubagent` 独立成页签）。
    pub fn register_subagent(
        &self,
        sub: SubagentSummary,
    ) -> Result<SubagentSummary, ActionableError> {
        self.subagents
            .lock()
            .map_err(|_| lock_err("subagents"))?
            .insert(sub.subagent_id.clone(), sub.clone());
        self.emit(DaemonEvent::SubagentUpdated(sub.clone()))?;
        Ok(sub)
    }

    /// 读取指定会话的消息历史（供查询或测试校验持久化）。
    pub fn list_session_messages(
        &self,
        session_id: &str,
    ) -> Result<Vec<TurnMessageSnapshot>, ActionableError> {
        self.store
            .list_session_messages(session_id)
            .map_err(internal_err)
    }

    /// 统一处理所有版本化 RPC 命令。
    pub fn handle_rpc(&self, req: RpcRequest) -> RpcResponse {
        let request_id = req.request_id.clone();
        match self.dispatch_command(req.command) {
            Ok(data) => RpcResponse::ok(request_id, data),
            Err(err) => RpcResponse::err(request_id, err),
        }
    }

    fn emit(&self, event: DaemonEvent) -> Result<EventEnvelope<DaemonEvent>, ActionableError> {
        let _order = self.event_order.lock().map_err(|_| lock_err("events"))?;
        let env = self.store.append_event(&event).map_err(internal_err)?;
        let _ = self.events_tx.send(env.clone());
        Ok(env)
    }

    fn dispatch_command(&self, cmd: RpcCommand) -> Result<RpcResultData, ActionableError> {
        match cmd {
            RpcCommand::GetStateSnapshot => Ok(RpcResultData::StateSnapshot(self.snapshot()?)),
            RpcCommand::CatchUpEvents { after_seq, limit } => {
                let (events, latest_seq) = self.events_after(after_seq, limit.unwrap_or(500))?;
                Ok(RpcResultData::Events { events, latest_seq })
            }
            RpcCommand::CreateQuickChat {
                title,
                agent_id,
                model_id,
                permission,
                thinking,
            } => {
                let reconciled = {
                    let orch = self
                        .orchestrator
                        .lock()
                        .map_err(|_| lock_err("orchestrator"))?;
                    orch.reconcile(ReconcileRequest {
                        agent_id,
                        current_model: model_id,
                        current_perm: permission,
                        current_thinking: thinking,
                        current_behavior: ActiveTurnBehavior::Queue,
                    })
                };
                let now = Utc::now();
                let chat_id = format!("qc_{}", Uuid::new_v4().simple());
                let chat = QuickChatSummary {
                    chat_id: chat_id.clone(),
                    title: title.unwrap_or_else(|| "无项目快速对话".to_owned()),
                    agent_id: reconciled.agent_id.clone(),
                    model_id: reconciled.model.clone(),
                    bucket: WorkspaceBucket::Done,
                    archived: false,
                    updated_at: now,
                };
                self.store.upsert_quick_chat(&chat).map_err(internal_err)?;

                let session = SessionSummary {
                    session_id: format!("sess_{}", Uuid::new_v4().simple()),
                    target: SessionTarget::QuickChat { chat_id },
                    title: chat.title.clone(),
                    agent_id: reconciled.agent_id,
                    model_id: reconciled.model,
                    thinking: reconciled.thinking,
                    permission: reconciled.permission,
                    lifecycle: TurnLifecycleState::Idle,
                    bucket: WorkspaceBucket::Done,
                    archived: false,
                    native_session_id: None,
                    created_at: now,
                    updated_at: now,
                };
                self.store.upsert_session(&session).map_err(internal_err)?;
                self.emit(DaemonEvent::QuickChatUpdated(chat.clone()))?;
                self.emit(DaemonEvent::SessionUpdated(session))?;
                Ok(RpcResultData::QuickChatCreated(chat))
            }
            RpcCommand::CreateSession {
                target,
                title,
                agent_id,
                model_id,
                permission,
                thinking,
            } => {
                match &target {
                    SessionTarget::Workspace { workspace_id } => {
                        if self
                            .store
                            .get_workspace(workspace_id)
                            .map_err(internal_err)?
                            .is_none()
                        {
                            return Err(ActionableError::new(
                                ErrorCode::NotFound,
                                "指定的工作区不存在",
                                "无法创建该工作区下的会话",
                                "请先刷新工作区列表或重新绑定项目目录",
                            ));
                        }
                    }
                    SessionTarget::QuickChat { chat_id } => {
                        if self
                            .store
                            .get_quick_chat(chat_id)
                            .map_err(internal_err)?
                            .is_none()
                        {
                            return Err(ActionableError::new(
                                ErrorCode::NotFound,
                                "指定的快速对话不存在",
                                "无法挂载会话",
                                "请先新建快速对话",
                            ));
                        }
                    }
                }
                let reconciled = {
                    let orch = self
                        .orchestrator
                        .lock()
                        .map_err(|_| lock_err("orchestrator"))?;
                    orch.reconcile(ReconcileRequest {
                        agent_id,
                        current_model: model_id,
                        current_perm: permission,
                        current_thinking: thinking,
                        current_behavior: ActiveTurnBehavior::Queue,
                    })
                };
                let now = Utc::now();
                let session = SessionSummary {
                    session_id: format!("sess_{}", Uuid::new_v4().simple()),
                    target,
                    title: title.unwrap_or_else(|| "新 Agent 会话".to_owned()),
                    agent_id: reconciled.agent_id,
                    model_id: reconciled.model,
                    thinking: reconciled.thinking,
                    permission: reconciled.permission,
                    lifecycle: TurnLifecycleState::Idle,
                    bucket: WorkspaceBucket::Done,
                    archived: false,
                    native_session_id: None,
                    created_at: now,
                    updated_at: now,
                };
                self.store.upsert_session(&session).map_err(internal_err)?;
                self.emit(DaemonEvent::SessionUpdated(session.clone()))?;
                Ok(RpcResultData::SessionCreated(session))
            }
            RpcCommand::ArchiveTarget { target, archived } => {
                match target {
                    SessionTarget::QuickChat { chat_id } => {
                        let mut chat = self
                            .store
                            .get_quick_chat(&chat_id)
                            .map_err(internal_err)?
                            .ok_or_else(|| not_found_err("快速对话"))?;
                        chat.archived = archived;
                        chat.updated_at = Utc::now();
                        self.store.upsert_quick_chat(&chat).map_err(internal_err)?;
                        self.emit(DaemonEvent::QuickChatUpdated(chat))?;
                    }
                    SessionTarget::Workspace { workspace_id } => {
                        let mut wks = self
                            .store
                            .get_workspace(&workspace_id)
                            .map_err(internal_err)?
                            .ok_or_else(|| not_found_err("工作区"))?;
                        wks.archived = archived;
                        wks.updated_at = Utc::now();
                        self.store.upsert_workspace(&wks).map_err(internal_err)?;
                        self.emit(DaemonEvent::WorkspaceUpdated(wks))?;
                    }
                }
                Ok(RpcResultData::Ack)
            }
            RpcCommand::SubmitTurn {
                session_id,
                text,
                chips,
                behavior,
                handoff,
                handoff_scope,
            } => self.handle_submit_turn(session_id, text, chips, behavior, handoff, handoff_scope),
            RpcCommand::PromoteQueuedTurn {
                session_id,
                queue_id,
                target_behavior,
            } => {
                let mut session = self
                    .store
                    .get_session(&session_id)
                    .map_err(internal_err)?
                    .ok_or_else(|| not_found_err("会话"))?;
                let (promotion, remaining) = {
                    let mut orch = self
                        .orchestrator
                        .lock()
                        .map_err(|_| lock_err("orchestrator"))?;
                    let promo =
                        orch.promote_queued_turn(&session_id, &queue_id, target_behavior)?;
                    let rest = orch.queued_turns(&session_id);
                    (promo, rest)
                };

                self.emit(DaemonEvent::QueueUpdated {
                    session_id: session_id.clone(),
                    queued: remaining,
                })?;

                let turn_index = self
                    .store
                    .next_turn_index(&session_id)
                    .map_err(internal_err)?;
                let badge = match promotion.target_behavior {
                    ActiveTurnBehavior::Steer => Some("中途纠偏 · 已注入".to_owned()),
                    ActiveTurnBehavior::Interrupt => Some("打断重发 · 已接管".to_owned()),
                    ActiveTurnBehavior::Queue => None,
                };
                let msg = TurnMessageSnapshot {
                    message_id: format!("msg_{}", Uuid::new_v4().simple()),
                    session_id: session_id.clone(),
                    turn_index,
                    role: TurnMessageRole::User,
                    author: "你".to_owned(),
                    model_id: Some(session.model_id.clone()),
                    thinking: session.thinking,
                    text: promotion.item.text,
                    chips: promotion.item.chips,
                    steer_badge: badge,
                    reasoning_duration_sec: None,
                    reasoning_steps: Vec::new(),
                    tools: Vec::new(),
                    handoff_anchor: None,
                    checkpoint: None,
                    created_at: Utc::now(),
                };
                self.store.upsert_message(&msg).map_err(internal_err)?;
                session.lifecycle = TurnLifecycleState::Running;
                session.bucket = WorkspaceBucket::Running;
                session.updated_at = Utc::now();
                self.store.upsert_session(&session).map_err(internal_err)?;
                self.emit(DaemonEvent::TurnMessageUpserted(msg))?;
                self.emit(DaemonEvent::SessionUpdated(session))?;

                Ok(RpcResultData::TurnSubmitted {
                    session_id,
                    lifecycle: TurnLifecycleState::Running,
                    queued_id: None,
                })
            }
            RpcCommand::CancelQueuedTurn {
                session_id,
                queue_id,
            } => {
                let remaining = {
                    let mut orch = self
                        .orchestrator
                        .lock()
                        .map_err(|_| lock_err("orchestrator"))?;
                    orch.cancel_queued_turn(&session_id, &queue_id)?
                };
                self.emit(DaemonEvent::QueueUpdated {
                    session_id,
                    queued: remaining,
                })?;
                Ok(RpcResultData::Ack)
            }
            RpcCommand::StopTurn { session_id } => {
                let mut session = self
                    .store
                    .get_session(&session_id)
                    .map_err(internal_err)?
                    .ok_or_else(|| not_found_err("会话"))?;
                session.lifecycle = TurnLifecycleState::Idle;
                session.bucket = WorkspaceBucket::Done;
                session.updated_at = Utc::now();
                self.store.upsert_session(&session).map_err(internal_err)?;
                self.update_target_bucket(&session.target, None, WorkspaceBucket::Done)?;
                self.emit(DaemonEvent::SessionUpdated(session))?;
                Ok(RpcResultData::Ack)
            }
            RpcCommand::SwitchSessionCapabilities {
                session_id,
                agent_id,
                model_id,
                permission,
                thinking,
                behavior,
            } => {
                let mut session = self
                    .store
                    .get_session(&session_id)
                    .map_err(internal_err)?
                    .ok_or_else(|| not_found_err("会话"))?;
                let outcome = {
                    let orch = self
                        .orchestrator
                        .lock()
                        .map_err(|_| lock_err("orchestrator"))?;
                    orch.reconcile(ReconcileRequest {
                        agent_id,
                        current_model: model_id.or_else(|| Some(session.model_id.clone())),
                        current_perm: permission.or(Some(session.permission)),
                        current_thinking: thinking.or(session.thinking),
                        current_behavior: behavior,
                    })
                };
                if session.agent_id != outcome.agent_id {
                    session.native_session_id = None;
                }
                session.agent_id = outcome.agent_id;
                session.model_id = outcome.model;
                session.permission = outcome.permission;
                session.thinking = outcome.thinking;
                session.updated_at = Utc::now();
                self.store.upsert_session(&session).map_err(internal_err)?;
                self.emit(DaemonEvent::SessionUpdated(session.clone()))?;
                if !outcome.lost_controls.is_empty() {
                    self.emit(DaemonEvent::CapabilitiesLostControls {
                        session_id,
                        notices: outcome.lost_controls.clone(),
                    })?;
                }
                Ok(RpcResultData::CapabilitiesSwitched {
                    session,
                    lost_controls: outcome.lost_controls,
                })
            }
            RpcCommand::SaveDraft(draft) => {
                let db_res = self
                    .store
                    .save_draft_if_newer(&draft)
                    .map_err(internal_err)?;
                if let DraftSaveOutcome::RejectedStale { current_generation } = db_res {
                    return Err(ActionableError::new(
                        ErrorCode::Conflict,
                        format!(
                            "草稿代次 {} 已过期（当前最新为 {}）",
                            draft.generation, current_generation
                        ),
                        "已拦截过期延迟写，保留最新编辑草稿",
                        "请使用最新草稿状态继续编辑",
                    ));
                }
                {
                    let mut orch = self
                        .orchestrator
                        .lock()
                        .map_err(|_| lock_err("orchestrator"))?;
                    orch.save_draft(draft.clone());
                }
                self.emit(DaemonEvent::DraftSaved(draft))?;
                Ok(RpcResultData::Ack)
            }
            RpcCommand::ResolveApproval {
                approval_id,
                resolution,
                guidance_note,
                resolved_by,
            } => {
                let mut approval = self
                    .store
                    .get_approval(&approval_id)
                    .map_err(internal_err)?
                    .ok_or_else(|| not_found_err("权限审批请求"))?;

                {
                    let orch = self
                        .orchestrator
                        .lock()
                        .map_err(|_| lock_err("orchestrator"))?;
                    orch.validate_approval(&approval, resolution)?;
                }

                let audit = ApprovalAuditRecord {
                    resolution,
                    resolved_by,
                    guidance_note,
                    resolved_at: Utc::now(),
                };
                approval.audit = Some(audit.clone());
                self.store
                    .upsert_approval(&approval)
                    .map_err(internal_err)?;
                self.update_target_bucket(
                    &approval.target,
                    Some(&approval.session_id),
                    WorkspaceBucket::Running,
                )?;
                self.emit(DaemonEvent::ApprovalResolved {
                    approval_id,
                    session_id: approval.session_id,
                    audit: audit.clone(),
                })?;
                Ok(RpcResultData::ApprovalResolved(audit))
            }
            RpcCommand::SubmitUserInput(answer) => {
                let removed = self
                    .pending_inputs
                    .lock()
                    .map_err(|_| lock_err("pending_inputs"))?
                    .remove(&answer.request_id);
                if removed.is_none() {
                    return Err(not_found_err("结构化提问卡"));
                }
                self.emit(DaemonEvent::UserInputSubmitted(answer))?;
                Ok(RpcResultData::Ack)
            }
            RpcCommand::DetachSubagent { subagent_id } => {
                let (parent_session_id, agent_id, model_id, task_title) = {
                    let guard = self.subagents.lock().map_err(|_| lock_err("subagents"))?;
                    let sub = guard
                        .get(&subagent_id)
                        .ok_or_else(|| not_found_err("子代理"))?;
                    if let Some(ref existing_sid) = sub.detached_session_id
                        && let Some(existing_sess) =
                            self.store.get_session(existing_sid).map_err(internal_err)?
                    {
                        return Ok(RpcResultData::SubagentDetached(existing_sess));
                    }
                    (
                        sub.parent_session_id.clone(),
                        sub.agent_id.clone(),
                        sub.model_id.clone(),
                        sub.task_title.clone(),
                    )
                };

                let parent = self
                    .store
                    .get_session(&parent_session_id)
                    .map_err(internal_err)?
                    .ok_or_else(|| not_found_err("父会话"))?;
                let now = Utc::now();
                let detached_session = SessionSummary {
                    session_id: format!("sess_{}", Uuid::new_v4().simple()),
                    target: parent.target,
                    title: format!("子代理: {task_title}"),
                    agent_id,
                    model_id,
                    thinking: parent.thinking,
                    permission: parent.permission,
                    lifecycle: TurnLifecycleState::Running,
                    bucket: WorkspaceBucket::Running,
                    archived: false,
                    native_session_id: None,
                    created_at: now,
                    updated_at: now,
                };
                self.store
                    .upsert_session(&detached_session)
                    .map_err(internal_err)?;

                let updated_sub = {
                    let mut guard = self.subagents.lock().map_err(|_| lock_err("subagents"))?;
                    let sub = guard
                        .get_mut(&subagent_id)
                        .ok_or_else(|| not_found_err("子代理"))?;
                    sub.detached_session_id = Some(detached_session.session_id.clone());
                    sub.clone()
                };

                self.emit(DaemonEvent::SubagentUpdated(updated_sub))?;
                self.emit(DaemonEvent::SessionUpdated(detached_session.clone()))?;
                Ok(RpcResultData::SubagentDetached(detached_session))
            }
            RpcCommand::UpdatePreferences(prefs) => {
                self.store.save_preferences(&prefs).map_err(internal_err)?;
                self.emit(DaemonEvent::PreferencesChanged(prefs.clone()))?;
                Ok(RpcResultData::PreferencesUpdated(prefs))
            }
            RpcCommand::CreatePairingOffer { transport } => {
                let (offer, code) = self
                    .pairing
                    .lock()
                    .map_err(|_| lock_err("pairing"))?
                    .create_offer(transport);
                Ok(RpcResultData::PairingOfferCreated { offer, code })
            }
            RpcCommand::ClaimPairing(claim_req) => {
                let (resp, device, token_sha256) = self
                    .pairing
                    .lock()
                    .map_err(|_| lock_err("pairing"))?
                    .claim(&claim_req)?;
                self.store
                    .save_paired_device(&device, &token_sha256)
                    .map_err(internal_err)?;
                self.emit(DaemonEvent::PairingDeviceUpdated(device))?;
                Ok(RpcResultData::PairingClaimed(resp))
            }
            RpcCommand::RevokePairedDevice { device_id } => {
                let updated = self
                    .store
                    .revoke_device(&device_id)
                    .map_err(internal_err)?
                    .ok_or_else(|| not_found_err("已配对设备"))?;
                self.emit(DaemonEvent::PairingDeviceUpdated(updated))?;
                Ok(RpcResultData::Ack)
            }
        }
    }

    fn handle_submit_turn(
        &self,
        session_id: String,
        text: String,
        chips: Vec<soxu_protocol::ComposerChip>,
        behavior: ActiveTurnBehavior,
        handoff: Option<soxu_protocol::HandoffSelection>,
        handoff_scope: Option<soxu_protocol::HandoffScopeOptions>,
    ) -> Result<RpcResultData, ActionableError> {
        if text.trim().is_empty() && chips.is_empty() {
            return Err(ActionableError::new(
                ErrorCode::ValidationFailed,
                "指令内容与附件不可同时为空",
                "未向当前会话发送任何输入",
                "请输入指令或附加上下文芯片后再发送",
            ));
        }

        let mut session = self
            .store
            .get_session(&session_id)
            .map_err(internal_err)?
            .ok_or_else(|| not_found_err("会话"))?;

        if session.lifecycle == TurnLifecycleState::Running && behavior == ActiveTurnBehavior::Queue
        {
            let queue_id = format!("q_{}", Uuid::new_v4().simple());
            let item = QueuedMessageItem {
                queue_id: queue_id.clone(),
                session_id: session_id.clone(),
                text,
                chips,
                behavior,
                created_at: Utc::now(),
            };
            let queued = {
                let mut orch = self
                    .orchestrator
                    .lock()
                    .map_err(|_| lock_err("orchestrator"))?;
                orch.enqueue_turn(item)
            };
            self.emit(DaemonEvent::QueueUpdated {
                session_id: session_id.clone(),
                queued,
            })?;
            return Ok(RpcResultData::TurnSubmitted {
                session_id,
                lifecycle: TurnLifecycleState::Running,
                queued_id: Some(queue_id),
            });
        }

        let now = Utc::now();
        let turn_index = self
            .store
            .next_turn_index(&session_id)
            .map_err(internal_err)?;
        let steer_badge = if session.lifecycle == TurnLifecycleState::Running {
            match behavior {
                ActiveTurnBehavior::Steer => Some("中途纠偏 · 已注入".to_owned()),
                ActiveTurnBehavior::Interrupt => Some("打断重发 · 新回合".to_owned()),
                ActiveTurnBehavior::Queue => None,
            }
        } else {
            None
        };

        let mut handoff_anchor_src = None;
        if let Some(target_sel) = handoff {
            let scope = handoff_scope.unwrap_or_default();
            let target_session = SessionSummary {
                session_id: format!("sess_{}", Uuid::new_v4().simple()),
                target: session.target.clone(),
                title: format!("{} 移交承接", target_sel.target_agent),
                agent_id: target_sel.target_agent.clone(),
                model_id: target_sel.target_model.clone(),
                thinking: target_sel.target_thinking,
                permission: session.permission,
                lifecycle: TurnLifecycleState::Running,
                bucket: WorkspaceBucket::Running,
                archived: false,
                native_session_id: None,
                created_at: now,
                updated_at: now,
            };
            let anchor = HandoffAnchorCard {
                handoff_id: format!("hof_{}", Uuid::new_v4().simple()),
                source_session_id: session.session_id.clone(),
                source_agent: session.agent_id.clone(),
                source_model: session.model_id.clone(),
                target_session_id: target_session.session_id.clone(),
                target_agent: target_sel.target_agent,
                target_model: target_sel.target_model,
                target_thinking: target_sel.target_thinking,
                summary_preview: format!(
                    "附带最近 {} 轮上下文{}",
                    scope.summary_turns,
                    if scope.include_uncommitted_diff_paths {
                        "与当前未提交变更"
                    } else {
                        ""
                    }
                ),
                attached_files_count: u32::try_from(chips.len()).unwrap_or(0),
                created_at: now,
            };
            self.store
                .upsert_session(&target_session)
                .map_err(internal_err)?;
            let target_msg = TurnMessageSnapshot {
                message_id: format!("msg_{}", Uuid::new_v4().simple()),
                session_id: target_session.session_id.clone(),
                turn_index: 1,
                role: TurnMessageRole::System,
                author: "Soxu 移交锚点".to_owned(),
                model_id: Some(target_session.model_id.clone()),
                thinking: target_session.thinking,
                text: text.clone(),
                chips: chips.clone(),
                steer_badge: None,
                reasoning_duration_sec: None,
                reasoning_steps: Vec::new(),
                tools: Vec::new(),
                handoff_anchor: Some(anchor.clone()),
                checkpoint: None,
                created_at: now,
            };
            self.store
                .upsert_message(&target_msg)
                .map_err(internal_err)?;
            self.emit(DaemonEvent::SessionUpdated(target_session))?;
            self.emit(DaemonEvent::TurnMessageUpserted(target_msg))?;
            handoff_anchor_src = Some(anchor);
        }

        let user_msg = TurnMessageSnapshot {
            message_id: format!("msg_{}", Uuid::new_v4().simple()),
            session_id: session_id.clone(),
            turn_index,
            role: TurnMessageRole::User,
            author: "你".to_owned(),
            model_id: Some(session.model_id.clone()),
            thinking: session.thinking,
            text,
            chips,
            steer_badge,
            reasoning_duration_sec: None,
            reasoning_steps: Vec::new(),
            tools: Vec::new(),
            handoff_anchor: handoff_anchor_src,
            checkpoint: None,
            created_at: now,
        };
        self.store.upsert_message(&user_msg).map_err(internal_err)?;

        session.lifecycle = TurnLifecycleState::Running;
        session.bucket = WorkspaceBucket::Running;
        session.updated_at = now;
        self.store.upsert_session(&session).map_err(internal_err)?;
        self.update_target_bucket(&session.target, None, WorkspaceBucket::Running)?;

        self.emit(DaemonEvent::TurnMessageUpserted(user_msg))?;
        self.emit(DaemonEvent::SessionUpdated(session))?;

        Ok(RpcResultData::TurnSubmitted {
            session_id,
            lifecycle: TurnLifecycleState::Running,
            queued_id: None,
        })
    }

    fn update_target_bucket(
        &self,
        target: &SessionTarget,
        session_id: Option<&str>,
        bucket: WorkspaceBucket,
    ) -> Result<(), ActionableError> {
        if let Some(sid) = session_id
            && let Some(mut sess) = self.store.get_session(sid).map_err(internal_err)?
        {
            sess.bucket = bucket;
            sess.updated_at = Utc::now();
            self.store.upsert_session(&sess).map_err(internal_err)?;
            self.emit(DaemonEvent::SessionUpdated(sess))?;
        }

        match target {
            SessionTarget::Workspace { workspace_id } => {
                if let Some(mut wks) = self
                    .store
                    .get_workspace(workspace_id)
                    .map_err(internal_err)?
                {
                    wks.bucket = bucket;
                    wks.updated_at = Utc::now();
                    self.store.upsert_workspace(&wks).map_err(internal_err)?;
                    self.emit(DaemonEvent::WorkspaceUpdated(wks))?;
                }
            }
            SessionTarget::QuickChat { chat_id } => {
                if let Some(mut qc) = self.store.get_quick_chat(chat_id).map_err(internal_err)? {
                    qc.bucket = bucket;
                    qc.updated_at = Utc::now();
                    self.store.upsert_quick_chat(&qc).map_err(internal_err)?;
                    self.emit(DaemonEvent::QuickChatUpdated(qc))?;
                }
            }
        }
        Ok(())
    }
}

fn lock_err(name: &str) -> ActionableError {
    ActionableError::new(
        ErrorCode::InternalError,
        format!("守护进程内部状态锁 ({name}) 异常"),
        "当前请求未能完成",
        "请重启 soxu-daemon 恢复一致状态",
    )
}

fn not_found_err(label: &str) -> ActionableError {
    ActionableError::new(
        ErrorCode::NotFound,
        format!("未找到指定的{label}"),
        "当前操作未生效",
        "请刷新快照后重试",
    )
}

fn internal_err(err: anyhow::Error) -> ActionableError {
    ActionableError::new(
        ErrorCode::InternalError,
        "守护进程持久化或内部调度失败",
        "部分状态可能未落盘",
        "请检查磁盘权限并查看脱敏诊断日志",
    )
    .with_diagnostic(err.to_string())
}
