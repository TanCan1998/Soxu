//! 受控 PTY 宿主（M6-A）：打开 / 输入 / 尺寸 / 关闭，有界分片输出 + 单调序号。
//!
//! 红线：输出按 `PTY_OUTPUT_CHUNK_MAX_BYTES` 切片并携带从 0 起严格递增的 `sequence`，
//! 流结束时补一个 `end_of_stream` 终结片；会话有上限、空闲可回收、关闭幂等。
//! 能力位 `controlled_pty` 未声明时整族命令拒绝，且不下发任何事件。

pub mod backend;
pub mod session;

use crate::scripts::FeatureGate;
use crate::scripts::manifest::resolve_workdir;
use soxu_protocol::{
    ActionableError, DaemonFeature, ErrorCode, OpenPtyRequest, PtyCloseReason, PtyCommand,
    PtyCommandResult, PtyEnvPolicy, PtyEvent, PtyExitStatus, PtyInputData, PtySessionId,
    PtySessionSnapshot, PtySessionState, PtySize, PtyWorkdirPolicy, ScriptWorkdirPolicy,
    WorkspaceRelativePath,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

pub use backend::PortablePtyBackend;
pub use session::PtySession;

/// 同时存活的 PTY 会话上限。
pub const PTY_MAX_SESSIONS: usize = 8;
/// 会话空闲回收阈值。
pub const PTY_IDLE_TIMEOUT: Duration = Duration::from_secs(900);
/// 关闭时的有界退出等待窗口。
pub const PTY_CLOSE_GRACE: Duration = Duration::from_secs(2);

/// PTY 事件出口（门控关闭时宿主不会调用）。
pub trait PtyEventSink: Send + Sync {
    /// 下发一条 PTY 事件。
    fn emit(&self, event: PtyEvent);
}

/// 丢弃事件的出口。
#[derive(Debug, Default)]
pub struct NullPtyEventSink;

impl PtyEventSink for NullPtyEventSink {
    fn emit(&self, _event: PtyEvent) {}
}

/// 派生一次 PTY 所需的受控参数。
#[derive(Debug, Clone)]
pub struct PtySpawnSpec {
    /// 工作区根（仅用于诊断与后续校验，不作为起点目录）。
    pub workspace_root: PathBuf,
    /// 已复核落在工作区内的起点目录。
    pub workdir: PathBuf,
    /// 初始栅格尺寸。
    pub size: PtySize,
    /// 白名单环境变量（名 → 值，值只来自受控来源）。
    pub environment: Vec<(String, String)>,
}

/// PTY 子进程控制面。
pub trait PtyControl: Send + Sync {
    /// 调整栅格尺寸。
    fn resize(&self, size: PtySize) -> Result<(), ActionableError>;
    /// 终止子进程（幂等）。
    fn terminate(&self) -> Result<(), ActionableError>;
    /// 非阻塞查询退出状态。
    fn try_wait(&self) -> Result<Option<PtyExitStatus>, ActionableError>;
}

/// 一次 PTY 派生结果：读端、写端与控制面。
pub struct PtyChild {
    /// 主端读句柄（子进程输出）。
    pub reader: Box<dyn std::io::Read + Send>,
    /// 主端写句柄（子进程输入）。
    pub writer: Box<dyn std::io::Write + Send>,
    /// 控制面。
    pub control: Arc<dyn PtyControl>,
}

/// PTY 后端：真实实现见 [`PortablePtyBackend`]，测试可注入内存实现。
pub trait PtyBackend: Send + Sync {
    /// 按受控参数派生一个 PTY 会话。
    fn spawn(&self, spec: &PtySpawnSpec) -> Result<PtyChild, ActionableError>;
}

/// 受控 PTY 宿主。
pub struct PtyHost {
    gate: FeatureGate,
    sink: Arc<dyn PtyEventSink>,
    backend: Arc<dyn PtyBackend>,
    sessions: Mutex<BTreeMap<String, Arc<PtySession>>>,
    next_id: AtomicU64,
}

impl std::fmt::Debug for PtyHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PtyHost")
            .field("gate", &self.gate)
            .field("live", &self.live_sessions())
            .finish_non_exhaustive()
    }
}

impl PtyHost {
    /// 构造宿主；门控缺省全关。
    #[must_use]
    pub fn new(
        gate: FeatureGate,
        sink: Arc<dyn PtyEventSink>,
        backend: Arc<dyn PtyBackend>,
    ) -> Self {
        Self {
            gate,
            sink,
            backend,
            sessions: Mutex::new(BTreeMap::new()),
            next_id: AtomicU64::new(1),
        }
    }

    /// 当前门控。
    #[must_use]
    pub fn gate(&self) -> &FeatureGate {
        &self.gate
    }

    /// 存活会话数（未关闭）。
    #[must_use]
    pub fn live_sessions(&self) -> usize {
        lock(&self.sessions)
            .values()
            .filter(|session| !session.is_closed())
            .count()
    }

    /// 处理一条 PTY 域命令。
    pub fn handle(
        &self,
        workspace_root: &Path,
        command: PtyCommand,
    ) -> Result<PtyCommandResult, ActionableError> {
        self.gate.require(DaemonFeature::ControlledPty)?;
        match command {
            PtyCommand::Open(request) => {
                Ok(PtyCommandResult::Opened(soxu_protocol::OpenPtyResponse {
                    session: self.open(workspace_root, request)?,
                }))
            }
            PtyCommand::Write(request) => {
                let session = self.session(&request.pty_id)?;
                let accepted = session.write(&request.data)?;
                Ok(PtyCommandResult::Written(soxu_protocol::WritePtyResponse {
                    pty_id: request.pty_id,
                    accepted_bytes: accepted,
                }))
            }
            PtyCommand::Resize(request) => Ok(PtyCommandResult::Resized(
                soxu_protocol::ResizePtyResponse {
                    session: self.resize(&request.pty_id, request.size)?,
                },
            )),
            PtyCommand::Close(request) => {
                Ok(PtyCommandResult::Closed(soxu_protocol::ClosePtyResponse {
                    session: self.close(&request.pty_id, request.reason)?,
                }))
            }
        }
    }

    /// 打开一个 PTY 会话。
    pub fn open(
        &self,
        workspace_root: &Path,
        request: OpenPtyRequest,
    ) -> Result<PtySessionSnapshot, ActionableError> {
        self.gate.require(DaemonFeature::ControlledPty)?;
        if self.live_sessions() >= PTY_MAX_SESSIONS {
            return Err(ActionableError::new(
                ErrorCode::Conflict,
                format!("存活的终端会话已达上限（{PTY_MAX_SESSIONS} 个）"),
                "该请求未打开新会话，既有会话不受影响",
                "请关闭不需要的终端会话后重试",
            ));
        }
        let workdir = resolve_pty_workdir(workspace_root, &request.workdir)?;
        let environment = build_environment(&request.env_policy);
        let spec = PtySpawnSpec {
            workspace_root: workspace_root.to_path_buf(),
            workdir,
            size: request.size,
            environment,
        };
        let child = self.backend.spawn(&spec)?;
        let id = PtySessionId::try_from(format!(
            "pty_{:08}",
            self.next_id.fetch_add(1, Ordering::AcqRel)
        ))
        .map_err(|err| {
            ActionableError::new(
                ErrorCode::InternalError,
                "无法签发终端会话标识",
                "该请求未打开会话",
                "请重试；若持续失败请上报缺陷",
            )
            .with_diagnostic(err.to_owned())
        })?;
        let session = PtySession::start(
            id,
            request.workspace_id,
            request.size,
            child,
            Arc::clone(&self.sink),
        );
        let snapshot = session.snapshot();
        lock(&self.sessions).insert(session.id().as_str().to_owned(), Arc::clone(&session));
        Ok(snapshot)
    }

    /// 写入输入。
    pub fn write(&self, pty_id: &PtySessionId, data: PtyInputData) -> Result<u32, ActionableError> {
        self.gate.require(DaemonFeature::ControlledPty)?;
        self.session(pty_id)?.write(&data)
    }

    /// 调整尺寸。
    pub fn resize(
        &self,
        pty_id: &PtySessionId,
        size: PtySize,
    ) -> Result<PtySessionSnapshot, ActionableError> {
        self.gate.require(DaemonFeature::ControlledPty)?;
        self.session(pty_id)?.resize(size)
    }

    /// 关闭会话；同一会话重复关闭返回当前快照而不报错。
    pub fn close(
        &self,
        pty_id: &PtySessionId,
        reason: PtyCloseReason,
    ) -> Result<PtySessionSnapshot, ActionableError> {
        self.gate.require(DaemonFeature::ControlledPty)?;
        self.session(pty_id)?.close(reason)
    }

    /// 查询会话快照。
    pub fn snapshot(&self, pty_id: &PtySessionId) -> Result<PtySessionSnapshot, ActionableError> {
        self.gate.require(DaemonFeature::ControlledPty)?;
        Ok(self.session(pty_id)?.snapshot())
    }

    /// 回收空闲会话，返回本次回收数量。
    pub fn reap_idle(&self, now: Instant) -> usize {
        if !self.gate.is_enabled(DaemonFeature::ControlledPty) {
            return 0;
        }
        let candidates: Vec<Arc<PtySession>> = lock(&self.sessions)
            .values()
            .filter(|session| session.is_idle(now, PTY_IDLE_TIMEOUT))
            .cloned()
            .collect();
        let mut reaped = 0;
        for session in candidates {
            if session.close(PtyCloseReason::IdleTimeout).is_ok() {
                reaped += 1;
            }
        }
        reaped
    }

    fn session(&self, pty_id: &PtySessionId) -> Result<Arc<PtySession>, ActionableError> {
        lock(&self.sessions)
            .get(pty_id.as_str())
            .cloned()
            .ok_or_else(|| {
                ActionableError::new(
                    ErrorCode::NotFound,
                    format!("未找到终端会话：{}", pty_id.as_str()),
                    "该请求未作用于任何会话",
                    "请重新打开终端会话后再输入或关闭",
                )
            })
    }
}

fn resolve_pty_workdir(
    workspace_root: &Path,
    policy: &PtyWorkdirPolicy,
) -> Result<PathBuf, ActionableError> {
    let script_policy = match policy {
        PtyWorkdirPolicy::WorkspaceRoot => ScriptWorkdirPolicy::WorkspaceRoot,
        PtyWorkdirPolicy::Subdirectory { relative_path } => ScriptWorkdirPolicy::Subdirectory {
            relative_path: WorkspaceRelativePath::try_from(relative_path.as_str().to_owned())
                .map_err(|err| {
                    ActionableError::new(
                        ErrorCode::ValidationFailed,
                        "终端起点目录不是合法的工作区相对路径",
                        "该请求未打开会话",
                        "请使用工作区内相对路径（禁止绝对路径与 `..`）",
                    )
                    .with_diagnostic(err.to_owned())
                })?,
        },
    };
    resolve_workdir(workspace_root, &script_policy)
}

/// 只把白名单内的变量交给终端，值来自守护进程自身环境，绝不回传原始环境。
fn build_environment(policy: &PtyEnvPolicy) -> Vec<(String, String)> {
    let mut environment = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for key in &policy.allow_keys {
        if !seen.insert(key.as_str().to_owned()) {
            continue;
        }
        if let Ok(value) = std::env::var(key.as_str()) {
            environment.push((key.as_str().to_owned(), value));
        }
    }
    environment
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// 会话状态是否为终止态（供宿主判定回收与幂等关闭）。
#[must_use]
pub fn is_terminal_state(state: PtySessionState) -> bool {
    matches!(
        state,
        PtySessionState::Exited | PtySessionState::Closed | PtySessionState::Failed
    )
}
