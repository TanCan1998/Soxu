//! 守护进程核心运行态、RPC 命令调度器与单调游标广播总线。

use crate::pairing::PairingManager;
use crate::pty::{PtyBackend, PtyEventSink, PtyHost, discover_or_unavailable};
use crate::schedules::{FinishOutcome, ScheduleEventSink, ScheduleService, TickReport};
use crate::scripts::{FeatureGate, ScriptEventSink, ScriptService};
use crate::store::SqliteStore;
use anyhow::Result;
use chrono::Utc;
use soxu_core::{CoreOrchestrator, DraftSaveOutcome, ReconcileRequest};
use soxu_magpie::{EnvironmentMode, discover_environment};
use soxu_protocol::{
    ActionableError, ActiveTurnBehavior, ApprovalAuditRecord, ApprovalItemSnapshot, DaemonEvent,
    DaemonFeature, ErrorCode, EventEnvelope, HandoffAnchorCard, HealthSnapshot, PROTOCOL_VERSION,
    PairedDeviceSummary, PendingUserInputRequest, ProjectSummary, PtyCommand, PtyEvent,
    QueuedMessageItem, QuickChatSummary, RpcCommand, RpcDomainCommand, RpcRequest, RpcResponse,
    RpcResultData, ScheduleEvent, ScheduleRunResult, ScriptCommand, ScriptLifecycleEvent,
    SessionSummary, SessionTarget, StateSnapshot, SubagentSummary, TurnLifecycleState,
    TurnMessageRole, TurnMessageSnapshot, WorkspaceBucket, WorkspaceIsolation, WorkspaceSummary,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;
use uuid::Uuid;

mod native;
mod worktrees;

pub use worktrees::{WorktreeAdmission, WorktreeAdmissionRequest};

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

/// 宿主能力位环境变量：逗号分隔的 [`DaemonFeature`] 令牌；未设置即三组 M6 能力全关。
///
/// 线缆级 `ClientHello` 能力字段属 `soxu-protocol` 冻结范围，本阶段由宿主配置注入；
/// 将来握手携带能力集合时只需把它灌入同一个 [`FeatureGate`]，判定逻辑不变。
pub const DAEMON_FEATURES_ENV: &str = "SOXU_DAEMON_FEATURES";

/// 调度定义持久化文件名（与 `soxu-daemon.db` 同放在宿主数据目录）。
pub const SCHEDULES_STORE_FILE: &str = "schedules.json";

/// 域事件出口：与 [`DaemonState`] 共用同一条「落盘 + 广播」管线。
///
/// 域服务会在自己的后台线程（脚本执行器、PTY 排流线程）里回调事件出口，
/// 因此这里只能持有可克隆的 `Arc` 句柄，绝不能反向持有 `DaemonState`
/// （否则形成 Arc 自引用环，守护进程实例永不释放）。
#[derive(Clone)]
struct DomainEventBus {
    store: Arc<SqliteStore>,
    order: Arc<Mutex<()>>,
    tx: broadcast::Sender<EventEnvelope<DaemonEvent>>,
}

impl DomainEventBus {
    /// 与 [`DaemonState::emit`] 完全同一条路径：同一把顺序锁、同一 `seq` 序列。
    fn emit(&self, event: DaemonEvent) -> Result<EventEnvelope<DaemonEvent>, ActionableError> {
        let _order = self.order.lock().map_err(|_| lock_err("events"))?;
        let envelope = self.store.append_event(&event).map_err(internal_err)?;
        let _ = self.tx.send(envelope.clone());
        Ok(envelope)
    }
}

/// 三域服务事件 → 全局事件信封的适配器。
///
/// 域服务的事件出口是不可失败的同步 `emit`，因此这里只能吐下落盘错误并留下
/// 脱敏告警：域操作本身已经成功，不能因为事件日志写失败而反向失败一次已派生的运行。
#[derive(Clone)]
struct DomainEventSink {
    bus: DomainEventBus,
}

impl DomainEventSink {
    fn publish(&self, event: DaemonEvent) {
        if let Err(error) = self.bus.emit(event) {
            tracing::warn!(
                code = ?error.code,
                summary = %error.summary_zh,
                "M6 域事件未能写入事件日志或广播通道"
            );
        }
    }
}

impl ScriptEventSink for DomainEventSink {
    fn emit(&self, event: ScriptLifecycleEvent) {
        self.publish(DaemonEvent::ScriptLifecycleChanged(event));
    }
}

impl ScheduleEventSink for DomainEventSink {
    fn emit(&self, event: ScheduleEvent) {
        self.publish(DaemonEvent::ScheduleActivity(event));
    }
}

impl PtyEventSink for DomainEventSink {
    fn emit(&self, event: PtyEvent) {
        self.publish(DaemonEvent::PtyActivity(event));
    }
}

/// M6 三域服务：脚本、调度、受控终端。
struct DomainServices {
    scripts: ScriptService,
    schedules: ScheduleService,
    pty: PtyHost,
}

impl DomainServices {
    /// 按同一门控与同一事件总线装配三域服务。
    fn build(
        gate: &FeatureGate,
        bus: &DomainEventBus,
        data_dir: Option<&Path>,
        pty_backend: Arc<dyn PtyBackend>,
    ) -> anyhow::Result<Self> {
        let sink = Arc::new(DomainEventSink { bus: bus.clone() });
        let schedules = ScheduleService::new(gate.clone(), sink.clone());
        // 有宿主数据目录时绑定持久化定义；加载失败即 fail-closed，不带入不可信定义。
        let schedules = match data_dir {
            Some(dir) => schedules.with_store(dir.join(SCHEDULES_STORE_FILE))?,
            None => schedules,
        };
        Ok(Self {
            scripts: ScriptService::new(gate.clone(), sink.clone()),
            schedules,
            pty: PtyHost::new(gate.clone(), sink, pty_backend),
        })
    }
}

/// 读取宿主能力位配置；未设置时全关（M6 契约红线：能力位缺省关闭）。
fn host_feature_gate() -> FeatureGate {
    let raw = std::env::var(DAEMON_FEATURES_ENV).unwrap_or_default();
    let gate = FeatureGate::from_csv(&raw);
    let unknown = gate.declared().unknown_tokens();
    if !unknown.is_empty() {
        tracing::warn!(
            env = DAEMON_FEATURES_ENV,
            tokens = ?unknown,
            "忽略未识别的 M6 能力令牌"
        );
    }
    gate
}

/// 守护进程共享运行时核心。
pub struct DaemonState {
    store: Arc<SqliteStore>,
    orchestrator: Mutex<CoreOrchestrator>,
    pairing: Mutex<PairingManager>,
    subagents: Mutex<BTreeMap<String, SubagentSummary>>,
    pending_inputs: Mutex<BTreeMap<String, PendingUserInputRequest>>,
    event_bus: DomainEventBus,
    native_turns: Option<native::NativeTurns>,
    worktree_admission_lock: tokio::sync::Mutex<()>,
    worktree_ready: Mutex<BTreeMap<String, soxu_magpie::WorktreeDirectory>>,
    daemon_fingerprint: String,
    magpie_detected: bool,
    require_auth_for_all: bool,
    feature_gate: FeatureGate,
    domain: DomainServices,
    pty_backend: Arc<dyn PtyBackend>,
    data_dir: Option<PathBuf>,
}

impl DaemonState {
    /// 打开或初始化持久化守护进程状态；传 `None` 时启用纯内存测试库。
    pub fn open(
        data_dir: Option<&Path>,
        endpoint_base_url: &str,
        require_auth_for_all: bool,
    ) -> Result<Self> {
        Self::open_with_environment(
            data_dir,
            endpoint_base_url,
            require_auth_for_all,
            discover_environment().mode,
        )
    }

    /// 使用宿主已经探测的环境模式打开状态。
    pub fn open_with_environment(
        data_dir: Option<&Path>,
        endpoint_base_url: &str,
        require_auth_for_all: bool,
        environment_mode: EnvironmentMode,
    ) -> Result<Self> {
        let store = match data_dir {
            Some(dir) => SqliteStore::open(&dir.join("soxu-daemon.db"))?,
            None => SqliteStore::open_in_memory()?,
        };
        let daemon_fingerprint = store.get_or_create_daemon_fingerprint()?;
        let pairing = PairingManager::new(daemon_fingerprint.clone(), endpoint_base_url.to_owned());
        let (events_tx, _) = broadcast::channel(512);
        let store = Arc::new(store);
        let event_bus = DomainEventBus {
            store: Arc::clone(&store),
            order: Arc::new(Mutex::new(())),
            tx: events_tx,
        };
        let magpie_detected = environment_mode == EnvironmentMode::MagpieManaged;
        // M6：能力位来自宿主配置（缺省全关），三域服务在装配阶段一次性完成接线。
        let feature_gate = host_feature_gate();
        let (pty_backend, pty_error) = discover_or_unavailable();
        if let Some(error) = pty_error {
            tracing::warn!(summary = %error.summary_zh, "受控终端本次不可用（不影响守护进程启动）");
        }
        let domain = DomainServices::build(
            &feature_gate,
            &event_bus,
            data_dir,
            Arc::clone(&pty_backend),
        )?;

        Ok(Self {
            store,
            orchestrator: Mutex::new(CoreOrchestrator::new()),
            pairing: Mutex::new(pairing),
            subagents: Mutex::new(BTreeMap::new()),
            pending_inputs: Mutex::new(BTreeMap::new()),
            event_bus,
            native_turns: None,
            worktree_admission_lock: tokio::sync::Mutex::new(()),
            worktree_ready: Mutex::new(BTreeMap::new()),
            daemon_fingerprint,
            magpie_detected,
            require_auth_for_all,
            feature_gate,
            domain,
            pty_backend,
            data_dir: data_dir.map(Path::to_path_buf),
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
        self.event_bus.tx.subscribe()
    }

    /// 当前 M6 能力门控（宿主配置注入，缺省全关）。
    #[must_use]
    pub fn feature_gate(&self) -> &FeatureGate {
        &self.feature_gate
    }

    /// 以宿主配置重设 M6 能力门控，并按新门控重建三域服务。
    ///
    /// 必须在启动装配阶段（任何域命令之前）调用：重建会丢弃域服务内存中已有的
    /// 运行登记与会话表；已落盘的调度定义会被重新加载。
    pub fn with_feature_gate(mut self, gate: FeatureGate) -> anyhow::Result<Self> {
        self.domain = DomainServices::build(
            &gate,
            &self.event_bus,
            self.data_dir.as_deref(),
            Arc::clone(&self.pty_backend),
        )?;
        self.feature_gate = gate;
        Ok(self)
    }

    /// 注入宿主提供的 PTY 后端（生产默认按 `SOXU_PTY_SHELL` 发现）。
    ///
    /// 同样只应在启动装配阶段调用，避免丢掉已经打开的终端会话。
    pub fn with_pty_backend(mut self, backend: Arc<dyn PtyBackend>) -> Self {
        let sink = Arc::new(DomainEventSink {
            bus: self.event_bus.clone(),
        });
        self.domain.pty = PtyHost::new(self.feature_gate.clone(), sink, Arc::clone(&backend));
        self.pty_backend = backend;
        self
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
        let mut snapshot = self
            .store
            .load_snapshot(&agents, &models, subagents, pending_inputs)
            .map_err(internal_err)?;
        for workspace in &mut snapshot.workspaces {
            if matches!(workspace.isolation, WorkspaceIsolation::Worktree { .. })
                && !workspace.needs_recovery
                && !self.worktree_directory_ready(&workspace.workspace_id)?
            {
                workspace.needs_recovery = true;
                workspace.recovery_reason = Some("Worktree 需要重新通过宿主接入复核".to_owned());
            }
        }
        Ok(snapshot)
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
        if workspace.workspace_id.starts_with("worktree:")
            || matches!(workspace.isolation, WorkspaceIsolation::Worktree { .. })
            || self
                .store
                .get_workspace(&workspace.workspace_id)
                .map_err(internal_err)?
                .is_some_and(|existing| {
                    matches!(existing.isolation, WorkspaceIsolation::Worktree { .. })
                })
        {
            return Err(ActionableError::new(
                ErrorCode::SafetyRedlineBlocked,
                "Worktree 须经受控接入登记",
                "工作区尚未通过 Git 检出复核",
                "请使用宿主 Worktree 接入入口",
            ));
        }
        self.persist_workspace(workspace)
    }

    fn persist_workspace(&self, workspace: WorkspaceSummary) -> Result<(), ActionableError> {
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
        self.event_bus.emit(event)
    }

    fn dispatch_command(&self, cmd: RpcCommand) -> Result<RpcResultData, ActionableError> {
        match cmd {
            RpcCommand::ListCheckpoints(_)
            | RpcCommand::CompareCheckpoints(_)
            | RpcCommand::PreviewRewind(_)
            | RpcCommand::ExecuteRewind(_)
            | RpcCommand::PreviewStructuredCommit(_)
            | RpcCommand::ExecuteStructuredCommit(_)
            | RpcCommand::GetGitOperationStatus(_) => Err(ActionableError::new(
                ErrorCode::Unsupported,
                "底层 Git 引擎尚未接入当前协议动作",
                "当前请求未执行，工作树、暂存区和分支均未修改",
                "请在守护进程接入相应 Git 能力后重试",
            )),
            // M6 域命令：能力门控 → 工作区解析 → 1:1 转发三域服务（#150 接线）。
            // 折叠关系由 `RpcCommand::into_domain_command` 唯一给出，本层不解释任何字段。
            RpcCommand::ListScripts(_)
            | RpcCommand::RunScript(_)
            | RpcCommand::StopScript(_)
            | RpcCommand::InspectScript(_)
            | RpcCommand::ListSchedules(_)
            | RpcCommand::CreateSchedule(_)
            | RpcCommand::UpdateSchedule(_)
            | RpcCommand::DisableSchedule(_)
            | RpcCommand::DeleteSchedule(_)
            | RpcCommand::TriggerSchedule(_)
            | RpcCommand::OpenPty(_)
            | RpcCommand::WritePty(_)
            | RpcCommand::ResizePty(_)
            | RpcCommand::ClosePty(_) => match cmd.into_domain_command() {
                Some(domain) => self.dispatch_domain_command(domain),
                None => Err(ActionableError::new(
                    ErrorCode::InternalError,
                    "RPC 域命令折叠失败",
                    "该请求未执行，未派发任何子进程、终端会话或调度任务",
                    "请升级守护进程：这是协议映射缺陷",
                )),
            },
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
                        agent_id: crate::antigravity::canonical_agent_id(agent_id),
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
                        self.require_workspace_for_session(workspace_id)?;
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
                        agent_id: crate::antigravity::canonical_agent_id(agent_id),
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
                        agent_id: crate::antigravity::canonical_agent_id(agent_id),
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

    /// 分派一条折叠后的 M6 域命令。
    ///
    /// 顺序红线：先做能力位门控（未声明即回 `Unsupported` 三要素、零事件），
    /// 再按 `workspace_id` 解析并复核受控工作区根，最后才 1:1 转发域服务。
    ///
    /// `StopScript` / `InspectScript` / `WritePty` / `ResizePty` / `ClosePty` 以域内
    /// 标识定位，**不**解析工作区根：停掉一个已经失去工作区的失控进程树或关闭一个
    /// 失效终端会话，不能被路径复核挡住。
    fn dispatch_domain_command(
        &self,
        command: RpcDomainCommand,
    ) -> Result<RpcResultData, ActionableError> {
        match command {
            RpcDomainCommand::Script(command) => match command {
                ScriptCommand::List(request) => {
                    self.feature_gate.require(DaemonFeature::ManagedScripts)?;
                    let root = self.resolve_workspace_root(&request.workspace_id)?;
                    Ok(RpcResultData::ManagedScript(
                        self.domain
                            .scripts
                            .handle(Some(&root), ScriptCommand::List(request))?,
                    ))
                }
                ScriptCommand::Run(request) => {
                    self.feature_gate.require(DaemonFeature::ManagedScripts)?;
                    let root = self.resolve_workspace_root(&request.workspace_id)?;
                    Ok(RpcResultData::ManagedScript(
                        self.domain
                            .scripts
                            .handle(Some(&root), ScriptCommand::Run(request))?,
                    ))
                }
                ScriptCommand::Stop(request) => Ok(RpcResultData::ManagedScript(
                    self.domain
                        .scripts
                        .handle(None, ScriptCommand::Stop(request))?,
                )),
                ScriptCommand::Inspect(request) => Ok(RpcResultData::ManagedScript(
                    self.domain
                        .scripts
                        .handle(None, ScriptCommand::Inspect(request))?,
                )),
            },
            // 调度域自带按节奏展开的能力门控（`Schedules` / `Heartbeats`），
            // 且全部命令不依赖工作区根；这里只先做一次总门控——否则未知
            // `schedule_id` 会先命中 `NotFound`，让关闭的能力位漏成“定义不存在”。
            RpcDomainCommand::Schedule(command) => {
                self.require_schedule_feature()?;
                Ok(RpcResultData::Schedule(
                    self.domain.schedules.handle(command)?,
                ))
            }
            RpcDomainCommand::Pty(command) => match command {
                PtyCommand::Open(request) => {
                    self.feature_gate.require(DaemonFeature::ControlledPty)?;
                    let root = self.resolve_workspace_root(&request.workspace_id)?;
                    Ok(RpcResultData::ControlledPty(
                        self.domain
                            .pty
                            .handle(Some(&root), PtyCommand::Open(request))?,
                    ))
                }
                PtyCommand::Write(request) => Ok(RpcResultData::ControlledPty(
                    self.domain.pty.handle(None, PtyCommand::Write(request))?,
                )),
                PtyCommand::Resize(request) => Ok(RpcResultData::ControlledPty(
                    self.domain.pty.handle(None, PtyCommand::Resize(request))?,
                )),
                PtyCommand::Close(request) => Ok(RpcResultData::ControlledPty(
                    self.domain.pty.handle(None, PtyCommand::Close(request))?,
                )),
            },
        }
    }

    /// 调度域总门控：`Schedules` 与 `Heartbeats` 都未声明时整族命令拒绝。
    ///
    /// 单个节奏类型的能力位由域服务按 `cadence` 再判一次（例如只声明
    /// `heartbeats` 时 cron / interval 调度仍被拒绕）。
    fn require_schedule_feature(&self) -> Result<(), ActionableError> {
        if self.feature_gate.is_enabled(DaemonFeature::Schedules)
            || self.feature_gate.is_enabled(DaemonFeature::Heartbeats)
        {
            return Ok(());
        }
        // 两个能力位都关闭：复用同一门控错误（含 `declared=[...]` 诊断），零事件下发。
        self.feature_gate.require(DaemonFeature::Schedules)
    }

    /// 按 `workspace_id` 解析已登记的受控工作区根目录。
    ///
    /// 与原生对话同一套授权语义：归档、待恢复、Worktree 未通过宿主复核一律拒绝；
    /// `Local` 工作区取项目登记的真实根路径，绝不使用脱敏展示路径。
    fn resolve_workspace_root(&self, workspace_id: &str) -> Result<PathBuf, ActionableError> {
        let workspace = self
            .store
            .get_workspace(workspace_id)
            .map_err(internal_err)?
            .ok_or_else(|| {
                ActionableError::new(
                    ErrorCode::NotFound,
                    "指定的工作区不存在",
                    "该请求未执行，未读取清单也未派生任何进程",
                    "请刷新工作区列表或重新绑定项目目录后重试",
                )
            })?;
        if workspace.archived || workspace.needs_recovery {
            return Err(ActionableError::new(
                ErrorCode::WorkspaceDamaged,
                "该工作区已归档或需要恢复",
                "该请求未执行，未读取清单也未派生任何进程",
                "请恢复工作区后重试",
            ));
        }
        match &workspace.isolation {
            WorkspaceIsolation::Local => {
                let project = self
                    .store
                    .get_project(&workspace.project_id)
                    .map_err(internal_err)?
                    .ok_or_else(|| not_found_err("项目"))?;
                let root = PathBuf::from(&project.root_path);
                if !root.is_absolute() || !root.is_dir() {
                    return Err(ActionableError::new(
                        ErrorCode::WorkspaceDamaged,
                        "该工作区的项目根目录当前不可用",
                        "该请求未执行，未读取清单也未派生任何进程",
                        "请核对项目绑定目录是否存在且有权限后重试",
                    ));
                }
                Ok(root)
            }
            WorkspaceIsolation::Worktree { .. } => {
                self.resolve_ready_worktree_dir(&workspace.workspace_id, &workspace.display_path)
            }
        }
    }

    /// 推进一次调度（宿主按自身节拍驱动，或测试推进 `ManualClock`）。
    ///
    /// 心跳节奏由 `agent_idle` 决定是否跳过；触发事件经同一信封下发。
    pub fn tick_schedules(&self, agent_idle: bool) -> Result<TickReport, ActionableError> {
        self.domain.schedules.tick_with_idle(agent_idle)
    }

    /// 收敛一次调度运行结果（宿主执行完触发后回调）。
    ///
    /// 只在本次实例下发过触发事件时收敛；未知 `run_id` 返回可执行错误，
    /// 绝不伪造运行记录，也不复活已删除的定义。
    pub fn finish_schedule_run(
        &self,
        result: ScheduleRunResult,
    ) -> Result<FinishOutcome, ActionableError> {
        self.domain.schedules.finish_run(result)
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

#[cfg(test)]
mod git_operation_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn git_operation_rpc_returns_actionable_unsupported() {
        let state = DaemonState::open(None, "http://127.0.0.1:0", false).expect("打开测试守护进程");
        let approval = json!({
            "operation_id": "op-1",
            "workspace_id": "ws-1",
            "session_id": "session-1",
            "approved_actions": [],
            "selected_paths": ["src/lib.rs"],
            "plan_digest": "plan-1",
            "idempotency_key": "attempt-1",
            "expires_at": "2026-11-26T12:00:00Z"
        });
        let endpoint = json!({"kind": "checkpoint", "checkpoint_id": "cp-1"});
        let commands = [
            (
                "list_checkpoints",
                json!({"workspace_id": "ws-1", "session_id": "session-1"}),
            ),
            (
                "compare_checkpoints",
                json!({
                    "workspace_id": "ws-1", "session_id": "session-1", "from": endpoint, "to": endpoint
                }),
            ),
            (
                "preview_rewind",
                json!({"workspace_id": "ws-1", "session_id": "session-1", "actions": []}),
            ),
            ("execute_rewind", approval.clone()),
            (
                "preview_structured_commit",
                json!({
                    "workspace_id": "ws-1", "selected_paths": ["src/lib.rs"], "message": "更新示例"
                }),
            ),
            ("execute_structured_commit", approval),
            (
                "get_git_operation_status",
                json!({
                    "workspace_id": "ws-1", "kind": "rewind",
                    "lookup": {"kind": "operation_id", "operation_id": "op-1"}
                }),
            ),
        ];

        for (method, params) in commands {
            let command = serde_json::from_value(json!({"method": method, "params": params}))
                .expect("解析 Git 操作命令");
            let response = state.handle_rpc(RpcRequest {
                request_id: method.into(),
                command,
            });
            assert_eq!(response.request_id, method);
            assert!(!response.ok);
            assert!(response.result.is_none());
            let error = response.error.expect("返回三要素错误");
            assert_eq!(error.code, ErrorCode::Unsupported);
            assert!(error.summary_zh.contains("尚未接入"));
            assert!(!error.impact_scope_zh.is_empty());
            assert!(!error.remediation_zh.is_empty());
        }
    }
}
