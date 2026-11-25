//! 将本机只读回合接入既有会话与事件流。

use super::{DaemonState, internal_err, lock_err, not_found_err};
use chrono::Utc;
use soxu_drivers::{
    DriverError, DriverEvent, NativeCliKind, NativeDriver, Outcome, RunRequest, RunTarget,
    RunningTurn, SessionMode, TurnControl,
};
use soxu_protocol::{
    ActionableError, ActiveTurnBehavior, DaemonEvent, ErrorCode, LostControlNotice,
    PermissionLevel, PermissionOptionMeta, PermissionTone, QueuedMessageItem, RpcCommand,
    RpcRequest, RpcResponse, RpcResultData, SessionSummary, SessionTarget, TurnLifecycleState,
    TurnMessageRole, TurnMessageSnapshot, WorkspaceBucket, WorkspaceIsolation,
};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use uuid::Uuid;

const MAX_PROMPT_BYTES: usize = 128 * 1024;
const MAX_REPLY_BYTES: usize = 4 * 1024 * 1024;
const TURN_TIMEOUT: Duration = Duration::from_secs(300);

pub(super) struct NativeTurns {
    quick_chat_root: PathBuf,
    drivers: HashMap<NativeCliKind, Result<NativeDriver, DriverError>>,
    active: Mutex<ActiveTurns>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ReadOnlyAgent {
    Codex,
    Pi,
    Claude,
    Qwen,
}

impl ReadOnlyAgent {
    fn for_session(session: &SessionSummary) -> Result<Self, ActionableError> {
        if !session.archived && session.permission == PermissionLevel::ReadOnly {
            match session.agent_id.as_str() {
                "codex" => return Ok(Self::Codex),
                "pi" => return Ok(Self::Pi),
                "claude" => return Ok(Self::Claude),
                "qwen" => return Ok(Self::Qwen),
                _ => {}
            }
        }
        Err(turn_error(
            ErrorCode::DriverUnavailable,
            "当前只接通未归档的 Codex、Pi、Claude Code 和 Qwen Code 只读会话",
            "请选择已配置的代理与只读权限，或新建只读快速对话",
        ))
    }

    fn name(self) -> &'static str {
        match self {
            Self::Codex => "Codex",
            Self::Pi => "Pi",
            Self::Claude => "Claude Code",
            Self::Qwen => "Qwen Code",
        }
    }

    fn kind(self) -> NativeCliKind {
        match self {
            Self::Codex => NativeCliKind::Codex,
            Self::Pi => NativeCliKind::Pi,
            Self::Claude => NativeCliKind::ClaudeCode,
            Self::Qwen => NativeCliKind::QwenCode,
        }
    }

    fn driver(self, runtime: &NativeTurns) -> Result<&NativeDriver, ActionableError> {
        runtime
            .drivers
            .get(&self.kind())
            .unwrap_or(&Err(DriverError::BinaryNotFound))
            .as_ref()
            .map_err(|error| self.driver_error(error))
    }

    fn driver_error(self, error: &DriverError) -> ActionableError {
        let advice = match self {
            Self::Codex => "请检查本机 Codex，或通过 SOXU_CODEX_EXE 指定原生可执行文件",
            Self::Pi => "请通过 SOXU_PI_EXE 指定可信可执行文件，使用 Node 时同时设置 SOXU_PI_ENTRY",
            Self::Claude => "请通过 SOXU_CLAUDE_EXE 指定可信 Claude Code 可执行文件",
            Self::Qwen => "请通过 SOXU_QWEN_NODE 和 SOXU_QWEN_ENTRY 指定可信 Node 和 Qwen JS 入口",
        };
        turn_error(ErrorCode::DriverUnavailable, error.to_string(), advice)
    }
}

type ActiveTurns = BTreeMap<String, ActiveTurn>;

struct ActiveTurn {
    target: SessionTarget,
    control: TurnControl,
    stopping: bool,
}

impl DaemonState {
    pub(crate) fn has_local_native_turns(&self) -> bool {
        self.native_turns.is_some() && !self.require_auth_for_all
    }

    /// 由守护进程宿主注入可信的 Codex 启动器和快速对话根目录。
    pub fn with_native_turns(
        self,
        quick_chat_root: PathBuf,
        codex: Result<NativeDriver, DriverError>,
    ) -> anyhow::Result<Self> {
        self.with_native_drivers(quick_chat_root, codex, Err(DriverError::BinaryNotFound))
    }

    /// 由守护进程宿主注入可信的只读启动器和快速对话根目录。
    pub fn with_native_drivers(
        self,
        quick_chat_root: PathBuf,
        codex: Result<NativeDriver, DriverError>,
        pi: Result<NativeDriver, DriverError>,
    ) -> anyhow::Result<Self> {
        self.with_native_driver_set(
            quick_chat_root,
            [(NativeCliKind::Codex, codex), (NativeCliKind::Pi, pi)],
        )
    }

    /// 注册宿主配置的只读原生驱动，启动时恢复中断会话。
    pub fn with_native_driver_set(
        mut self,
        quick_chat_root: PathBuf,
        configured: impl IntoIterator<Item = (NativeCliKind, Result<NativeDriver, DriverError>)>,
    ) -> anyhow::Result<Self> {
        let drivers: HashMap<_, _> = configured.into_iter().collect();
        if !self.require_auth_for_all {
            let orchestrator = self
                .orchestrator
                .get_mut()
                .map_err(|_| lock_err("orchestrator"))?;
            let mut agents = orchestrator.agents().to_vec();
            for kind in [
                NativeCliKind::Pi,
                NativeCliKind::ClaudeCode,
                NativeCliKind::QwenCode,
            ] {
                let (Some(driver), Some(agent)) = (
                    drivers.get(&kind),
                    agents
                        .iter_mut()
                        .find(|agent| agent.agent_id == kind.binary_name()),
                ) else {
                    continue;
                };
                agent.permissions.push(PermissionOptionMeta {
                    id: PermissionLevel::ReadOnly,
                    label: "只读对话".to_owned(),
                    description: "仅投递文本，禁用工具、扩展和 MCP".to_owned(),
                    tone: PermissionTone::Neutral,
                });
                agent.supports_live_steer = false;
                agent.installed = driver.is_ok();
            }
            orchestrator.set_agent_catalog(agents);
        }
        for mut session in self.store.interrupted_native_sessions()? {
            session.lifecycle = TurnLifecycleState::Idle;
            session.bucket = WorkspaceBucket::Failed;
            session.updated_at = Utc::now();
            self.store.upsert_session(&session)?;
            self.emit(DaemonEvent::SessionUpdated(session.clone()))?;
            self.update_target_bucket(&session.target, None, session.bucket)?;
            self.publish_native_failure(
                &session.session_id,
                turn_error(
                    ErrorCode::DriverUnavailable,
                    "守护进程重启前的原生回合已中断",
                    "请核对已保存的对话并重新发送",
                ),
            );
        }
        self.native_turns = Some(NativeTurns {
            quick_chat_root,
            drivers,
            active: Mutex::new(BTreeMap::new()),
        });
        Ok(self)
    }

    pub(crate) fn handle_transport_rpc(self: &Arc<Self>, req: RpcRequest) -> RpcResponse {
        let Some(runtime) = &self.native_turns else {
            return self.handle_rpc(req);
        };
        let RpcRequest {
            request_id,
            command,
        } = req;
        let result = self.dispatch_native(runtime, command);
        match result {
            Ok(data) => RpcResponse::ok(request_id, data),
            Err(error) => RpcResponse::err(request_id, error),
        }
    }

    fn dispatch_native(
        self: &Arc<Self>,
        runtime: &NativeTurns,
        command: RpcCommand,
    ) -> Result<RpcResultData, ActionableError> {
        let mut active = runtime
            .active
            .lock()
            .map_err(|_| lock_err("native turns"))?;
        match command {
            RpcCommand::SubmitTurn {
                session_id,
                text,
                chips,
                behavior,
                handoff,
                handoff_scope,
            } => {
                if self.require_auth_for_all {
                    return Err(turn_error(
                        ErrorCode::Unauthorized,
                        "原生对话当前仅在本机回环监听时启用",
                        "请在本机使用回环地址启动守护进程",
                    ));
                }
                if !chips.is_empty() || handoff.is_some() || handoff_scope.is_some() {
                    return Err(turn_error(
                        ErrorCode::ValidationFailed,
                        "只读原生对话暂未接入附件与移交",
                        "请提交纯文本消息",
                    ));
                }
                validate_prompt(&text)?;
                let session = self
                    .store
                    .get_session(&session_id)
                    .map_err(internal_err)?
                    .ok_or_else(|| not_found_err("会话"))?;
                let agent = ReadOnlyAgent::for_session(&session)?;
                if let Some(running) = active.get(&session_id)
                    && (running.stopping || behavior == ActiveTurnBehavior::Interrupt)
                {
                    return Err(turn_error(
                        ErrorCode::Conflict,
                        "当前回合需要先停止并等待退出",
                        "请使用 StopTurn，待会话空闲后重新发送",
                    ));
                }
                let running = active.contains_key(&session_id);
                let queued = if running {
                    true
                } else {
                    self.start_queued_native(runtime, &mut active, session.clone())?
                };
                if queued {
                    let queue_id = format!("q_{}", Uuid::new_v4().simple());
                    let queued = self
                        .orchestrator
                        .lock()
                        .map_err(|_| lock_err("orchestrator"))?
                        .enqueue_turn(QueuedMessageItem {
                            queue_id: queue_id.clone(),
                            session_id: session_id.clone(),
                            text,
                            chips,
                            behavior: ActiveTurnBehavior::Queue,
                            created_at: Utc::now(),
                        });
                    self.publish_native(DaemonEvent::QueueUpdated {
                        session_id: session_id.clone(),
                        queued,
                    });
                    if behavior == ActiveTurnBehavior::Steer {
                        self.publish_native(DaemonEvent::CapabilitiesLostControls {
                            session_id: session_id.clone(),
                            notices: vec![LostControlNotice {
                                field: "behavior".to_owned(),
                                previous_value: "steer".to_owned(),
                                fallback_value: "queue".to_owned(),
                                reason_zh: format!(
                                    "Daemon 当前未为 {} 启用中途纠偏，消息已排队",
                                    agent.name()
                                ),
                            }],
                        });
                    }
                    return Ok(submitted(session_id, Some(queue_id)));
                }
                self.start_native(runtime, &mut active, session, text)?;
                Ok(submitted(session_id, None))
            }
            RpcCommand::StopTurn { session_id } => {
                self.store
                    .get_session(&session_id)
                    .map_err(internal_err)?
                    .ok_or_else(|| not_found_err("会话"))?;
                if let Some(running) = active.get_mut(&session_id) {
                    running.stopping = true;
                    running.control.cancel();
                }
                Ok(RpcResultData::Ack)
            }
            RpcCommand::ArchiveTarget {
                ref target,
                archived: true,
            } if active.values().any(|turn| &turn.target == target) => Err(turn_error(
                ErrorCode::Conflict,
                "运行中的对话不能归档",
                "请等待回合结束或先停止回合",
            )),
            RpcCommand::PromoteQueuedTurn { .. } => Err(turn_error(
                ErrorCode::Conflict,
                "只读原生对话暂未接入排队提级",
                "请等待当前回合结束或撤回排队消息",
            )),
            RpcCommand::SwitchSessionCapabilities { ref session_id, .. }
                if active.contains_key(session_id) =>
            {
                Err(turn_error(
                    ErrorCode::Conflict,
                    "原生回合运行期间不能切换会话能力",
                    "请等待当前回合结束后再切换",
                ))
            }
            other => self.dispatch_command(other),
        }
    }

    fn native_target(
        &self,
        runtime: &NativeTurns,
        target: &SessionTarget,
    ) -> Result<RunTarget, ActionableError> {
        match target {
            SessionTarget::QuickChat { chat_id } => {
                let chat = self
                    .store
                    .get_quick_chat(chat_id)
                    .map_err(internal_err)?
                    .ok_or_else(|| not_found_err("快速对话"))?;
                if chat.archived
                    || chat_id
                        .strip_prefix("qc_")
                        .and_then(|id| Uuid::parse_str(id).ok())
                        .is_none()
                {
                    return Err(invalid_directory());
                }
                std::fs::create_dir_all(&runtime.quick_chat_root)
                    .map_err(|_| invalid_directory())?;
                let root = runtime
                    .quick_chat_root
                    .canonicalize()
                    .map_err(|_| invalid_directory())?;
                let dir = root.join(chat_id);
                std::fs::create_dir_all(&dir).map_err(|_| invalid_directory())?;
                if dir.canonicalize().map_err(|_| invalid_directory())? != dir {
                    return Err(invalid_directory());
                }
                Ok(RunTarget::QuickChat(dir))
            }
            SessionTarget::Workspace { workspace_id } => {
                let workspace = self
                    .store
                    .get_workspace(workspace_id)
                    .map_err(internal_err)?
                    .ok_or_else(|| not_found_err("工作区"))?;
                if workspace.archived
                    || workspace.needs_recovery
                    || workspace.isolation != WorkspaceIsolation::Local
                {
                    return Err(invalid_directory());
                }
                let project = self
                    .store
                    .get_project(&workspace.project_id)
                    .map_err(internal_err)?
                    .ok_or_else(|| not_found_err("项目"))?;
                let dir = PathBuf::from(project.root_path);
                if !dir.is_absolute() || !dir.is_dir() {
                    return Err(invalid_directory());
                }
                Ok(RunTarget::Directory(dir))
            }
        }
    }

    fn start_native(
        self: &Arc<Self>,
        runtime: &NativeTurns,
        active: &mut ActiveTurns,
        mut session: SessionSummary,
        text: String,
    ) -> Result<(), ActionableError> {
        let agent = ReadOnlyAgent::for_session(&session)?;
        let driver = agent.driver(runtime)?;
        let target = self.native_target(runtime, &session.target)?;
        let mut request = RunRequest::new(target, text.clone());
        if let Some(id) = &session.native_session_id {
            request.session = SessionMode::Resume(id.into());
        }
        let turn_index = self
            .store
            .next_turn_index(&session.session_id)
            .map_err(internal_err)?;
        let turn = driver
            .start(request)
            .map_err(|error| agent.driver_error(&error))?;

        session.lifecycle = TurnLifecycleState::Running;
        session.bucket = WorkspaceBucket::Running;
        session.updated_at = Utc::now();
        let user = turn_message(&session, turn_index, TurnMessageRole::User, "你", text);
        let assistant = turn_message(
            &session,
            turn_index,
            TurnMessageRole::Assistant,
            agent.name(),
            String::new(),
        );
        self.store
            .begin_native_turn(&session, &user, &assistant)
            .map_err(internal_err)?;
        active.insert(
            session.session_id.clone(),
            ActiveTurn {
                target: session.target.clone(),
                control: turn.control(),
                stopping: false,
            },
        );
        self.publish_native(DaemonEvent::TurnMessageUpserted(user));
        self.publish_native(DaemonEvent::TurnMessageUpserted(assistant.clone()));
        self.publish_native(DaemonEvent::SessionUpdated(session.clone()));
        self.record_target_bucket(&session.target, WorkspaceBucket::Running);
        self.publish_native(DaemonEvent::CapabilitiesLostControls {
            session_id: session.session_id,
            notices: vec![LostControlNotice {
                field: "model".to_owned(),
                previous_value: session.model_id,
                fallback_value: format!("{} 本机默认模型", agent.name()),
                reason_zh: "本轮使用本机配置中的默认模型和思考设置".to_owned(),
            }],
        });

        let state = Arc::clone(self);
        tokio::spawn(async move { state.drive_native(agent, turn, assistant).await });
        Ok(())
    }

    async fn drive_native(
        self: Arc<Self>,
        agent: ReadOnlyAgent,
        mut turn: RunningTurn,
        mut assistant: TurnMessageSnapshot,
    ) {
        let result = tokio::time::timeout(TURN_TIMEOUT, async {
            let mut pending_id = None;
            while let Some(event) = turn.next_event().await {
                match event {
                    DriverEvent::Session { id } if Uuid::parse_str(&id).is_ok() => match agent {
                        ReadOnlyAgent::Codex => self
                            .record_native_session_id(&assistant.session_id, id)
                            .map_err(|_| "原生会话标识无法落盘".to_owned())?,
                        ReadOnlyAgent::Pi | ReadOnlyAgent::Claude | ReadOnlyAgent::Qwen => {
                            pending_id = Some(id)
                        }
                    },
                    DriverEvent::Text(text_delta) => {
                        if assistant.text.len().saturating_add(text_delta.len()) > MAX_REPLY_BYTES {
                            return Err("原生回复超过长度上限".to_owned());
                        }
                        assistant.text.push_str(&text_delta);
                        if self.store.upsert_message(&assistant).is_err() {
                            return Err("原生回复无法落盘".to_owned());
                        }
                        self.publish_native(DaemonEvent::TurnStreamDelta {
                            session_id: assistant.session_id.clone(),
                            message_id: assistant.message_id.clone(),
                            text_delta,
                            reasoning_delta: None,
                        });
                    }
                    DriverEvent::Finished(Outcome::Succeeded) => {
                        if agent != ReadOnlyAgent::Codex {
                            let id = pending_id.ok_or_else(|| {
                                format!("{} 未返回有效原生会话标识", agent.name())
                            })?;
                            self.record_native_session_id(&assistant.session_id, id)
                                .map_err(|_| "原生会话标识无法落盘".to_owned())?;
                        }
                        return Ok(());
                    }
                    DriverEvent::Finished(Outcome::Cancelled) => return Ok(()),
                    DriverEvent::Finished(_) => {
                        return Err(format!("{} 原生回合失败", agent.name()));
                    }
                    _ => {}
                }
            }
            Err(format!("{} 事件流提前结束", agent.name()))
        })
        .await
        .unwrap_or_else(|_| Err(format!("{} 原生回合超时", agent.name())));

        if result.is_err() {
            turn.control().cancel();
            let _ = tokio::time::timeout(Duration::from_secs(5), async {
                while let Some(event) = turn.next_event().await {
                    if matches!(event, DriverEvent::Finished(_)) {
                        break;
                    }
                }
            })
            .await;
        }
        drop(turn);
        self.finish_native(&assistant.session_id, result);
    }

    fn record_native_session_id(
        &self,
        session_id: &str,
        id: String,
    ) -> Result<(), ActionableError> {
        let runtime = self
            .native_turns
            .as_ref()
            .ok_or_else(|| lock_err("native turns"))?;
        let _active = runtime
            .active
            .lock()
            .map_err(|_| lock_err("native turns"))?;
        let mut session = self
            .store
            .get_session(session_id)
            .map_err(internal_err)?
            .ok_or_else(|| not_found_err("会话"))?;
        if session.native_session_id.as_ref() != Some(&id) {
            session.native_session_id = Some(id);
            self.store.upsert_session(&session).map_err(internal_err)?;
            self.emit(DaemonEvent::SessionUpdated(session))?;
        }
        Ok(())
    }

    fn finish_native(self: &Arc<Self>, session_id: &str, result: Result<(), String>) {
        let Some(runtime) = &self.native_turns else {
            return;
        };
        let Ok(mut active) = runtime.active.lock() else {
            return;
        };
        let Some(finished) = active.remove(session_id) else {
            return;
        };
        let Ok(Some(mut session)) = self.store.get_session(session_id) else {
            tracing::warn!("无法读取原生回合所属会话");
            return;
        };
        session.lifecycle = TurnLifecycleState::Idle;
        session.bucket = if result.is_ok() || finished.stopping {
            WorkspaceBucket::Done
        } else {
            WorkspaceBucket::Failed
        };
        session.updated_at = Utc::now();
        if self.store.upsert_session(&session).is_err() {
            tracing::warn!("无法落盘原生回合终态");
            return;
        }
        self.publish_native(DaemonEvent::SessionUpdated(session.clone()));
        let bucket = if active.values().any(|turn| turn.target == session.target) {
            WorkspaceBucket::Running
        } else {
            session.bucket
        };
        self.record_target_bucket(&session.target, bucket);
        if finished.stopping {
            return;
        }
        if let Err(summary) = result {
            self.publish_native_failure(
                session_id,
                turn_error(
                    ErrorCode::DriverUnavailable,
                    summary,
                    "请检查当前代理的本机登录与只读模式后重试",
                ),
            );
            return;
        }
        if let Err(error) = self.start_queued_native(runtime, &mut active, session) {
            self.publish_native_failure(session_id, error);
        }
    }

    fn start_queued_native(
        self: &Arc<Self>,
        runtime: &NativeTurns,
        active: &mut ActiveTurns,
        session: SessionSummary,
    ) -> Result<bool, ActionableError> {
        let session_id = session.session_id.clone();
        let mut orchestrator = self
            .orchestrator
            .lock()
            .map_err(|_| lock_err("orchestrator"))?;
        let Some(item) = orchestrator.queued_turns(&session_id).into_iter().next() else {
            return Ok(false);
        };
        self.start_native(runtime, active, session, item.text)?;
        let queued = orchestrator.cancel_queued_turn(&session_id, &item.queue_id)?;
        self.publish_native(DaemonEvent::QueueUpdated { session_id, queued });
        Ok(true)
    }

    fn publish_native_failure(&self, session_id: &str, error: ActionableError) {
        self.publish_native(DaemonEvent::ErrorRaised {
            session_id: Some(session_id.to_owned()),
            error,
        });
    }

    fn record_target_bucket(&self, target: &SessionTarget, bucket: WorkspaceBucket) {
        if self.update_target_bucket(target, None, bucket).is_err() {
            tracing::warn!("无法更新原生回合所属目标的状态");
        }
    }

    fn publish_native(&self, event: DaemonEvent) {
        if self.emit(event).is_err() {
            tracing::warn!("无法记录原生回合事件");
        }
    }
}

fn turn_message(
    session: &SessionSummary,
    turn_index: u32,
    role: TurnMessageRole,
    author: &str,
    text: String,
) -> TurnMessageSnapshot {
    TurnMessageSnapshot {
        message_id: format!("msg_{}", Uuid::new_v4().simple()),
        session_id: session.session_id.clone(),
        turn_index,
        role,
        author: author.to_owned(),
        model_id: None,
        thinking: None,
        text,
        chips: Vec::new(),
        steer_badge: None,
        reasoning_duration_sec: None,
        reasoning_steps: Vec::new(),
        tools: Vec::new(),
        handoff_anchor: None,
        checkpoint: None,
        created_at: Utc::now(),
    }
}

fn validate_prompt(text: &str) -> Result<(), ActionableError> {
    if text.trim().is_empty() || text.contains('\0') || text.len() > MAX_PROMPT_BYTES {
        return Err(turn_error(
            ErrorCode::ValidationFailed,
            "原生消息内容或长度无效",
            "请输入不超过 128 KiB 的非空文本",
        ));
    }
    Ok(())
}

fn submitted(session_id: String, queued_id: Option<String>) -> RpcResultData {
    RpcResultData::TurnSubmitted {
        session_id,
        lifecycle: TurnLifecycleState::Running,
        queued_id,
    }
}

fn invalid_directory() -> ActionableError {
    turn_error(
        ErrorCode::WorkspaceDamaged,
        "只读原生对话无法使用该工作目录",
        "请恢复快速对话目录或选择一个有效的本地绑定目录",
    )
}

fn turn_error(code: ErrorCode, summary: impl Into<String>, advice: &str) -> ActionableError {
    ActionableError::new(code, summary, "仅影响当前原生对话", advice)
}
