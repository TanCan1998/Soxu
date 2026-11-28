//! 受控 PTY 宿主（M6-A）：打开 / 输入 / 尺寸 / 关闭，有界分片输出 + 单调序号。
//!
//! 红线：输出按 `PTY_OUTPUT_CHUNK_MAX_BYTES` 切片并携带从 0 起严格递增的 `sequence`，
//! 流结束时补一个 `end_of_stream` 终结片；会话有上限、空闲可回收、关闭幂等。
//! 能力位 `controlled_pty` 未声明时整族命令拒绝，且不下发任何事件。
//!
//! 生命周期闭环（资源治理阶段 2）：
//!
//! * **容量原子化**：固定 [`PTY_MAX_SESSIONS`] 个许可在派生之前原子预留，许可由会话
//!   通过 [`session::CapacityLease`] 持有；检查与占用不再是两步，超限请求绝不派生子进程。
//! * **终态即释放**：会话进入 `Exited` / `Closed` / `Failed` 时经终态回调立即归还许可
//!   并从活动表摘除，`live_sessions()` 只统计真正活动的会话。
//! * **有界墓碑**：摘除后保留最后快照（最多 [`PTY_TOMBSTONE_MAX`] 条、保留
//!   [`PTY_TOMBSTONE_TTL`]），重复 `close` 与事后 `snapshot` 仍能拿到明确终态。
//! * **周期回收**：宿主构造时若处于 tokio 运行时内，则启动维护任务
//!   （每 [`PTY_REAP_INTERVAL`] 回收空闲会话并清理过期墓碑），可由环境变量
//!   [`PTY_REAP_ENV`] 置 `off` 关闭。
//! * **优雅关停**：[`PtyHost::shutdown`] 关闭全部活动会话并有界等待输出泵收敛，
//!   未收敛的会话登记为「回收未确认」并写日志。

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
use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::{Duration, Instant};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

pub use backend::{
    PortablePtyBackend, UnavailablePtyBackend, discover_or_unavailable, terminate_process_tree,
};
pub use session::{CapacityLease, PtyPump, PtySession, TerminalHook};

/// 同时存活的 PTY 会话上限。
pub const PTY_MAX_SESSIONS: usize = 8;
/// 会话空闲回收阈值。
pub const PTY_IDLE_TIMEOUT: Duration = Duration::from_secs(900);
/// 关闭时的有界退出等待窗口。
pub const PTY_CLOSE_GRACE: Duration = Duration::from_secs(2);
/// 终态会话墓碑条数上限（供重复关闭与事后快照返回最后状态）。
pub const PTY_TOMBSTONE_MAX: usize = 64;
/// 墓碑保留时长。
pub const PTY_TOMBSTONE_TTL: Duration = Duration::from_secs(300);
/// 周期维护间隔。
pub const PTY_REAP_INTERVAL: Duration = Duration::from_secs(60);
/// 关停时等待输出泵线程收敛的窗口。
pub const PTY_PUMP_JOIN_GRACE: Duration = Duration::from_secs(5);
/// 关闭周期维护任务的环境变量（`off` / `0` / `false` / `no` / `disabled`）。
pub const PTY_REAP_ENV: &str = "SOXU_PTY_REAP";

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
    /// 子进程标识（供关停核对进程树是否收敛；未知时返回 `None`）。
    fn pid(&self) -> Option<u32> {
        None
    }
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

/// 周期维护结果。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PtyMaintenanceReport {
    /// 本次因空闲回收的会话数。
    pub reaped_idle: usize,
    /// 本次清理的过期墓碑数。
    pub expired_tombstones: usize,
}

/// 会话表口径（活动会话与终态墓碑）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PtyTableStats {
    /// 活动会话数（真正占用容量）。
    pub live: usize,
    /// 仍然保留的终态墓碑数。
    pub tombstones: usize,
}

/// 关停结论。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PtyShutdownReport {
    /// 关停时仍活动的会话数。
    pub requested: usize,
    /// 输出泵在窗口内收敛的会话数。
    pub confirmed: usize,
    /// 窗口内未收敛的会话标识：必须记录为「回收未确认」，不得静默。
    pub unconfirmed: Vec<String>,
}

/// 终态墓碑：会话摘表后保留的最后快照与摘除时刻。
#[derive(Debug, Clone)]
struct Tombstone {
    snapshot: PtySessionSnapshot,
    retired_at: Instant,
}

#[derive(Debug, Default)]
struct TableState {
    live: BTreeMap<String, Arc<PtySession>>,
    tombs: BTreeMap<String, Tombstone>,
    tomb_order: VecDeque<String>,
}

impl TableState {
    fn push_tombstone(&mut self, id: String, tomb: Tombstone) {
        if self.tombs.insert(id.clone(), tomb).is_none() {
            self.tomb_order.push_back(id);
        }
        while self.tomb_order.len() > PTY_TOMBSTONE_MAX {
            let Some(oldest) = self.tomb_order.pop_front() else {
                break;
            };
            self.tombs.remove(&oldest);
        }
    }
}

/// 会话表：容量许可 + 活动表 + 有界墓碑。
#[derive(Debug)]
struct PtyTable {
    gate: FeatureGate,
    permits: Arc<Semaphore>,
    state: Mutex<TableState>,
}

impl PtyTable {
    fn new(gate: FeatureGate) -> Arc<Self> {
        Arc::new(Self {
            gate,
            permits: Arc::new(Semaphore::new(PTY_MAX_SESSIONS)),
            state: Mutex::new(TableState::default()),
        })
    }

    fn try_acquire(&self) -> Option<OwnedSemaphorePermit> {
        Arc::clone(&self.permits).try_acquire_owned().ok()
    }

    fn insert(&self, session: Arc<PtySession>) {
        lock(&self.state)
            .live
            .insert(session.id().as_str().to_owned(), session);
    }

    fn live_sessions(&self) -> Vec<Arc<PtySession>> {
        lock(&self.state).live.values().cloned().collect()
    }

    fn live_count(&self) -> usize {
        lock(&self.state).live.len()
    }

    fn locate(&self, pty_id: &PtySessionId) -> Option<Located> {
        let state = lock(&self.state);
        if let Some(session) = state.live.get(pty_id.as_str()) {
            return Some(Located::Live(Arc::clone(session)));
        }
        state
            .tombs
            .get(pty_id.as_str())
            .map(|tomb| Located::Retired(tomb.snapshot.clone()))
    }

    /// 终态回调：摘除表项、登记有界墓碑并立即归还容量许可。
    fn retire(&self, session: &PtySession) {
        let id = session.id().as_str().to_owned();
        let snapshot = session.snapshot();
        {
            let mut state = lock(&self.state);
            state.live.remove(&id);
            state.push_tombstone(
                id,
                Tombstone {
                    snapshot,
                    retired_at: Instant::now(),
                },
            );
        }
        // 摘表与墓碑登记在同一把锁内完成：查表方永远不会看到「既不在活动表、也
        // 没有墓碑」的空窗；随后立即归还许可，容量不再被终态会话占用。
        session.release_capacity();
    }

    fn prune_tombstones(&self, now: Instant, ttl: Duration) -> usize {
        let mut state = lock(&self.state);
        let mut expired = 0;
        // `tomb_order` 是插入序，时刻单调递增：遇到第一个未过期条目即可停止。
        while let Some(front) = state.tomb_order.front().cloned() {
            let is_expired = state
                .tombs
                .get(&front)
                .is_some_and(|tomb| now.duration_since(tomb.retired_at) >= ttl);
            if !is_expired {
                break;
            }
            state.tomb_order.pop_front();
            state.tombs.remove(&front);
            expired += 1;
        }
        expired
    }

    fn stats(&self) -> PtyTableStats {
        let state = lock(&self.state);
        PtyTableStats {
            live: state.live.len(),
            tombstones: state.tombs.len(),
        }
    }
}

/// 会话定位结果。
enum Located {
    /// 仍在活动表中。
    Live(Arc<PtySession>),
    /// 已进入终态并摘表，只剩墓碑快照。
    Retired(PtySessionSnapshot),
}

/// 受控 PTY 宿主。
pub struct PtyHost {
    gate: FeatureGate,
    sink: Arc<dyn PtyEventSink>,
    backend: Arc<dyn PtyBackend>,
    table: Arc<PtyTable>,
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
        let table = PtyTable::new(gate.clone());
        spawn_maintenance(&table);
        Self {
            gate,
            sink,
            backend,
            table,
            next_id: AtomicU64::new(1),
        }
    }

    /// 当前门控。
    #[must_use]
    pub fn gate(&self) -> &FeatureGate {
        &self.gate
    }

    /// 存活会话数（真正活动的会话；终态会话已摘表）。
    #[must_use]
    pub fn live_sessions(&self) -> usize {
        self.table.live_count()
    }

    /// 会话表口径（活动数 + 墓碑数）。
    #[must_use]
    pub fn table_stats(&self) -> PtyTableStats {
        self.table.stats()
    }

    /// 处理一条 PTY 域命令。
    ///
    /// 只有 `Open` 需要工作区根（用于复核起点目录）；`Write` / `Resize` / `Close`
    /// 以 `pty_id` 定位会话，传 `None` 即可——关闭一个已经失去工作区的终端会话
    /// 不能被路径复核挡住。
    pub fn handle(
        &self,
        workspace_root: Option<&Path>,
        command: PtyCommand,
    ) -> Result<PtyCommandResult, ActionableError> {
        self.gate.require(DaemonFeature::ControlledPty)?;
        match command {
            PtyCommand::Open(request) => {
                let workspace_root = workspace_root.ok_or_else(|| {
                    ActionableError::new(
                        ErrorCode::ValidationFailed,
                        "打开终端会话必须绑定工作区根目录",
                        "该请求未打开会话，未派生任何进程",
                        "请由守护进程按 workspace_id 解析受控目录后重试",
                    )
                })?;
                Ok(PtyCommandResult::Opened(soxu_protocol::OpenPtyResponse {
                    session: self.open(workspace_root, request)?,
                }))
            }
            PtyCommand::Write(request) => {
                let session = self.live_session(&request.pty_id)?;
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
    ///
    /// 校验全部前置完成之后才原子预留容量、然后才派生进程：并发请求越过上限时
    /// 拿到的是可诊断的上限错误，而不是多派生的 shell。
    pub fn open(
        &self,
        workspace_root: &Path,
        request: OpenPtyRequest,
    ) -> Result<PtySessionSnapshot, ActionableError> {
        self.gate.require(DaemonFeature::ControlledPty)?;
        let workdir = resolve_pty_workdir(workspace_root, &request.workdir)?;
        let environment = build_environment(&request.env_policy);
        let spec = PtySpawnSpec {
            workspace_root: workspace_root.to_path_buf(),
            workdir,
            size: request.size,
            environment,
        };
        let Some(permit) = self.table.try_acquire() else {
            return Err(capacity_error());
        };
        // 派生失败时许可随作用域释放，容量不会被失败请求长期占用。
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
        let on_terminal: TerminalHook = {
            let weak: Weak<PtyTable> = Arc::downgrade(&self.table);
            Arc::new(move |session: &PtySession| match weak.upgrade() {
                Some(table) => table.retire(session),
                // 宿主已释放时仍要归还许可，避免容量随宿主生命周期泄漏。
                None => session.release_capacity(),
            })
        };
        let (session, pump) = PtySession::prepare(
            id,
            request.workspace_id,
            request.size,
            child,
            Arc::clone(&self.sink),
            on_terminal,
            CapacityLease::new(permit),
        );
        let snapshot = session.snapshot();
        self.table.insert(Arc::clone(&session));
        session.start_pump(pump);
        Ok(snapshot)
    }

    /// 写入输入。
    pub fn write(&self, pty_id: &PtySessionId, data: PtyInputData) -> Result<u32, ActionableError> {
        self.gate.require(DaemonFeature::ControlledPty)?;
        self.live_session(pty_id)?.write(&data)
    }

    /// 调整尺寸。
    pub fn resize(
        &self,
        pty_id: &PtySessionId,
        size: PtySize,
    ) -> Result<PtySessionSnapshot, ActionableError> {
        self.gate.require(DaemonFeature::ControlledPty)?;
        self.live_session(pty_id)?.resize(size)
    }

    /// 关闭会话；同一会话重复关闭返回当前（或墓碑）快照而不报错。
    pub fn close(
        &self,
        pty_id: &PtySessionId,
        reason: PtyCloseReason,
    ) -> Result<PtySessionSnapshot, ActionableError> {
        self.gate.require(DaemonFeature::ControlledPty)?;
        match self.table.locate(pty_id) {
            Some(Located::Live(session)) => session.close(reason),
            Some(Located::Retired(snapshot)) => Ok(snapshot),
            None => Err(unknown_session(pty_id)),
        }
    }

    /// 查询会话快照；已回收的会话返回明确终态而不是内部错误。
    pub fn snapshot(&self, pty_id: &PtySessionId) -> Result<PtySessionSnapshot, ActionableError> {
        self.gate.require(DaemonFeature::ControlledPty)?;
        match self.table.locate(pty_id) {
            Some(Located::Live(session)) => Ok(session.snapshot()),
            Some(Located::Retired(snapshot)) => Ok(snapshot),
            None => Err(unknown_session(pty_id)),
        }
    }

    /// 回收空闲会话，返回本次回收数量。
    pub fn reap_idle(&self, now: Instant) -> usize {
        if !self.gate.is_enabled(DaemonFeature::ControlledPty) {
            return 0;
        }
        let candidates: Vec<Arc<PtySession>> = self
            .table
            .live_sessions()
            .into_iter()
            .filter(|session| session.is_idle(now, PTY_IDLE_TIMEOUT))
            .collect();
        let mut reaped = 0;
        for session in candidates {
            if session.close(PtyCloseReason::IdleTimeout).is_ok() {
                reaped += 1;
            }
        }
        reaped
    }

    /// 周期维护：回收空闲会话 + 清理过期墓碑。
    pub fn maintain(&self, now: Instant) -> PtyMaintenanceReport {
        PtyMaintenanceReport {
            reaped_idle: self.reap_idle(now),
            expired_tombstones: self.table.prune_tombstones(now, PTY_TOMBSTONE_TTL),
        }
    }

    /// 关停宿主：关闭全部活动会话，有界等待输出泵收敛并给出可判别的回收结论。
    ///
    /// 未在窗口内收敛的会话会被登记进 [`PtyShutdownReport::unconfirmed`] 并写警告日志，
    /// 绝不把「没法确认」当成「已经回收干净」。
    pub fn shutdown(&self, grace: Duration) -> PtyShutdownReport {
        let sessions = self.table.live_sessions();
        let requested = sessions.len();
        // 并发关闭：任何一个 shell 的终止等待不得串行放大整个关停时长。
        std::thread::scope(|scope| {
            for session in &sessions {
                scope.spawn(move || {
                    if let Err(error) = session.close(PtyCloseReason::ServerShutdown) {
                        tracing::warn!(
                            pty_id = session.id().as_str(),
                            %error,
                            "终端会话关停时关闭失败"
                        );
                    }
                });
            }
        });
        let deadline = Instant::now() + grace;
        let mut unconfirmed = Vec::new();
        for session in &sessions {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if !session.wait_pump(remaining) {
                tracing::warn!(
                    pty_id = session.id().as_str(),
                    already_terminal = session.is_terminal(),
                    "终端会话关停后输出泵回收未确认"
                );
                unconfirmed.push(session.id().as_str().to_owned());
            }
        }
        // 兜底：关闭失败或终态回调缺失的会话仍占着容量，必须显式摘表并登记墓碑。
        for session in &sessions {
            if self
                .table
                .locate(session.id())
                .is_some_and(|located| matches!(located, Located::Live(_)))
            {
                self.table.retire(session);
            }
        }
        let report = PtyShutdownReport {
            requested,
            confirmed: requested.saturating_sub(unconfirmed.len()),
            unconfirmed,
        };
        if report.requested > 0 {
            tracing::info!(
                requested = report.requested,
                confirmed = report.confirmed,
                unconfirmed = report.unconfirmed.len(),
                "PTY 宿主关停完成"
            );
        }
        report
    }

    fn live_session(&self, pty_id: &PtySessionId) -> Result<Arc<PtySession>, ActionableError> {
        match self.table.locate(pty_id) {
            Some(Located::Live(session)) => Ok(session),
            Some(Located::Retired(snapshot)) => Err(finished_session(pty_id, &snapshot)),
            None => Err(unknown_session(pty_id)),
        }
    }
}

impl Drop for PtyHost {
    /// 宿主释放时兜底关闭仍活动的会话，避免把 shell 进程遗留到守护进程生命周期之外。
    fn drop(&mut self) {
        let sessions = self.table.live_sessions();
        if sessions.is_empty() {
            return;
        }
        tracing::debug!(count = sessions.len(), "PTY 宿主释放：兜底关闭活动会话");
        for session in sessions {
            let _ = session.close(PtyCloseReason::ServerShutdown);
            let _ = session.wait_pump(PTY_CLOSE_GRACE);
        }
    }
}

/// 周期维护任务：只在 tokio 运行时内且未显式关闭时启动。
///
/// 任务持有会话表的弱引用：宿主释放后任务自行退出，不会把宿主（及其会话）续命。
fn spawn_maintenance(table: &Arc<PtyTable>) {
    if !pty_reap_enabled() {
        tracing::debug!(env = PTY_REAP_ENV, "终端周期维护已被环境变量关闭");
        return;
    }
    if !table.gate.is_enabled(DaemonFeature::ControlledPty) {
        return;
    }
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        // 无异步运行时（例如纯同步测试）时不启动维护任务，调用方可显式 `maintain`。
        return;
    };
    let weak = Arc::downgrade(table);
    handle.spawn(async move {
        let mut ticker = tokio::time::interval(PTY_REAP_INTERVAL);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        // 第一次 tick 立即返回：跳过，避免启动瞬间做一次无意义扫描。
        ticker.tick().await;
        loop {
            ticker.tick().await;
            let Some(table) = weak.upgrade() else {
                break;
            };
            let report = PtyMaintenanceReport {
                reaped_idle: reap_idle_sessions(&table, Instant::now()),
                expired_tombstones: table.prune_tombstones(Instant::now(), PTY_TOMBSTONE_TTL),
            };
            if report.reaped_idle > 0 || report.expired_tombstones > 0 {
                tracing::debug!(
                    reaped_idle = report.reaped_idle,
                    expired_tombstones = report.expired_tombstones,
                    "终端周期维护完成"
                );
            }
            drop(table);
        }
    });
}

fn reap_idle_sessions(table: &Arc<PtyTable>, now: Instant) -> usize {
    let candidates: Vec<Arc<PtySession>> = table
        .live_sessions()
        .into_iter()
        .filter(|session| session.is_idle(now, PTY_IDLE_TIMEOUT))
        .collect();
    let mut reaped = 0;
    for session in candidates {
        if session.close(PtyCloseReason::IdleTimeout).is_ok() {
            reaped += 1;
        }
    }
    reaped
}

/// 周期维护开关：`SOXU_PTY_REAP` 为 `off` / `0` / `false` / `no` / `disabled` 时关闭，缺省开启。
fn pty_reap_enabled() -> bool {
    match std::env::var(PTY_REAP_ENV) {
        Ok(raw) => !matches!(
            raw.trim().to_ascii_lowercase().as_str(),
            "off" | "0" | "false" | "no" | "disabled"
        ),
        Err(_) => true,
    }
}

fn capacity_error() -> ActionableError {
    ActionableError::new(
        ErrorCode::Conflict,
        format!("存活的终端会话已达上限（{PTY_MAX_SESSIONS} 个）"),
        "该请求未打开新会话，未派生任何进程，既有会话不受影响",
        "请关闭不需要的终端会话后重试",
    )
}

fn unknown_session(pty_id: &PtySessionId) -> ActionableError {
    ActionableError::new(
        ErrorCode::NotFound,
        format!("未找到终端会话：{}", pty_id.as_str()),
        "该请求未作用于任何会话",
        "请重新打开终端会话后再输入或关闭",
    )
}

fn finished_session(pty_id: &PtySessionId, snapshot: &PtySessionSnapshot) -> ActionableError {
    ActionableError::new(
        ErrorCode::Conflict,
        format!(
            "终端会话已结束（{}）：{}",
            pty_id.as_str(),
            describe_state(snapshot.state)
        ),
        "本次输入未写入任何进程；会话最后快照仍可查询",
        "请重新打开终端会话后重试",
    )
}

const fn describe_state(state: PtySessionState) -> &'static str {
    match state {
        PtySessionState::Starting => "启动中",
        PtySessionState::Running => "运行中",
        PtySessionState::Exited => "已退出",
        PtySessionState::Closed => "已关闭",
        PtySessionState::Failed => "已失败",
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
