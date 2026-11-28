//! 受管脚本执行器（M6-A）。
//!
//! 红线：只按 `program + 参数数组` 直接 spawn，**永不经 shell**；
//! 工作目录、环境白名单、透传参数、超时与输出预算全部在派生之前复核，
//! 退出与回收走「有界等待 + 进程树终止 + 回收窗口」三段，
//! 高风险脚本必须携带审批标记，未批准一律不派生任何子进程。

use crate::scripts::gate::FeatureGate;
use crate::scripts::manifest::{child_facing_path, resolve_workdir};
use chrono::Utc;
use soxu_protocol::{
    ActionableError, DaemonFeature, ErrorCode, IdempotencyKey, ManagedScriptEntry,
    ScriptLifecycleEvent, ScriptLifecyclePhase, ScriptProgram, ScriptRunSnapshot, ScriptRunState,
    ScriptUserArguments,
};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tokio::sync::mpsc;

/// 单次运行累计输出预算（stdout + stderr 合计），超出后丢弃并置截断标记。
pub const SCRIPT_OUTPUT_LIMIT_BYTES: usize = 256 * 1024;
/// 同时运行的脚本上限。
pub const SCRIPT_MAX_CONCURRENT_RUNS: usize = 8;
/// 内存中保留的最近运行记录条数上限。
pub const SCRIPT_MAX_RETAINED_RUNS: usize = 64;
/// 终止后的回收窗口：窗口内未确认退出即判定回收未确认，绝不无限期等待。
pub const SCRIPT_REAP_WINDOW: Duration = Duration::from_secs(5);
/// 子进程状态轮询间隔（有界等待的最小粒度）。
const WAIT_POLL_INTERVAL: Duration = Duration::from_millis(10);

/// 运行生命周期事件出口。
///
/// 由宿主注入（守护进程侧可为广播通道）；门控关闭时执行器不会调用该出口，
/// 因此未协商能力的客户端一条事件都收不到。
pub trait ScriptEventSink: Send + Sync {
    /// 下发一条生命周期事件。
    fn emit(&self, event: ScriptLifecycleEvent);
}

/// 丢弃事件的出口（未接线宿主与只读回归用）。
#[derive(Debug, Default)]
pub struct NullScriptEventSink;

impl ScriptEventSink for NullScriptEventSink {
    fn emit(&self, _event: ScriptLifecycleEvent) {}
}

/// 终止后的回收结论（L-016 有界五态的可判别证据）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReapOutcome {
    /// 正常退出路径，不需要终止与回收。
    NotRequired,
    /// 主动终止后在有界窗口内确认退出。
    ExitedAfterStop,
    /// 回收窗口内未确认退出：不宣称已回收干净。
    ReapUnconfirmed,
    /// 派生即失败，从未产生可回收的子进程。
    SpawnFailed,
}

/// 单次运行的输出快照（执行器内部接缝，协议快照不含输出正文）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunOutput {
    bytes: Vec<u8>,
    truncated: bool,
    dropped_bytes: u64,
}

impl RunOutput {
    /// 已捕获的输出字节。
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// 是否因超预算被截断。
    #[must_use]
    pub fn truncated(&self) -> bool {
        self.truncated
    }

    /// 被丢弃的字节数。
    #[must_use]
    pub fn dropped_bytes(&self) -> u64 {
        self.dropped_bytes
    }
}

#[derive(Debug)]
struct OutputBudget {
    limit: usize,
    used: AtomicUsize,
    output: Mutex<RunOutput>,
}

impl OutputBudget {
    fn new(limit: usize) -> Self {
        Self {
            limit,
            used: AtomicUsize::new(0),
            output: Mutex::new(RunOutput::default()),
        }
    }

    fn push(&self, chunk: &[u8]) {
        let mut used = self.used.load(Ordering::Relaxed);
        loop {
            let free = self.limit.saturating_sub(used);
            let take = free.min(chunk.len());
            match self.used.compare_exchange_weak(
                used,
                used + take,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    let mut output = lock(&self.output);
                    if take > 0 {
                        output.bytes.extend_from_slice(&chunk[..take]);
                    }
                    if take < chunk.len() {
                        output.truncated = true;
                        output.dropped_bytes += u64::try_from(chunk.len() - take).unwrap_or(0);
                    }
                    return;
                }
                Err(actual) => used = actual,
            }
        }
    }

    fn snapshot(&self) -> RunOutput {
        lock(&self.output).clone()
    }
}

/// 一次运行的可查询句柄。
#[derive(Debug)]
pub struct ScriptRun {
    snapshot: Mutex<ScriptRunSnapshot>,
    output: Arc<OutputBudget>,
    stop: mpsc::Sender<bool>,
    reap: Mutex<Option<ReapOutcome>>,
}

impl ScriptRun {
    /// 当前运行快照。
    #[must_use]
    pub fn snapshot(&self) -> ScriptRunSnapshot {
        lock(&self.snapshot).clone()
    }

    /// 输出快照。
    #[must_use]
    pub fn output(&self) -> RunOutput {
        self.output.snapshot()
    }

    /// 回收结论；运行未结束时为 `None`。
    #[must_use]
    pub fn reap_outcome(&self) -> Option<ReapOutcome> {
        *lock(&self.reap)
    }

    fn transition(&self, state: ScriptRunState) -> ScriptRunSnapshot {
        let mut snapshot = lock(&self.snapshot);
        snapshot.state = state;
        if state == ScriptRunState::Running && snapshot.started_at.is_none() {
            snapshot.started_at = Some(Utc::now());
        }
        if snapshot.finished_at.is_none() && is_terminal_state(state) {
            snapshot.finished_at = Some(Utc::now());
        }
        snapshot.clone()
    }

    fn finish(&self, state: ScriptRunState, exit_code: Option<i32>) -> ScriptRunSnapshot {
        let mut snapshot = lock(&self.snapshot);
        snapshot.state = state;
        snapshot.exit_code = exit_code;
        if snapshot.finished_at.is_none() {
            snapshot.finished_at = Some(Utc::now());
        }
        snapshot.clone()
    }

    fn is_terminal(&self) -> bool {
        is_terminal_state(lock(&self.snapshot).state)
    }
}

const fn is_terminal_state(state: ScriptRunState) -> bool {
    matches!(
        state,
        ScriptRunState::Exited | ScriptRunState::Failed | ScriptRunState::Cancelled
    )
}

/// 启动一次脚本运行的入参。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunScriptOptions {
    /// 客户端透传参数（受条目策略约束）。
    pub user_arguments: ScriptUserArguments,
    /// 幂等键：同键重复提交收敛到同一运行，不重复执行副作用。
    pub idempotency_key: Option<IdempotencyKey>,
    /// 高风险条目的审批卡标识。
    pub approval_id: Option<String>,
}

#[derive(Debug, Default)]
struct RunnerState {
    runs: BTreeMap<String, Arc<ScriptRun>>,
    order: Vec<String>,
    idempotent: BTreeMap<String, String>,
}

/// 受管脚本执行器。
pub struct ScriptRunner {
    gate: FeatureGate,
    sink: Arc<dyn ScriptEventSink>,
    state: Arc<Mutex<RunnerState>>,
    active: Arc<AtomicUsize>,
}

impl std::fmt::Debug for ScriptRunner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScriptRunner")
            .field("gate", &self.gate)
            .field("active", &self.active.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

impl ScriptRunner {
    /// 构造执行器；门控缺省全关。
    #[must_use]
    pub fn new(gate: FeatureGate, sink: Arc<dyn ScriptEventSink>) -> Self {
        Self {
            gate,
            sink,
            state: Arc::new(Mutex::new(RunnerState::default())),
            active: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// 当前门控。
    #[must_use]
    pub fn gate(&self) -> &FeatureGate {
        &self.gate
    }

    /// 启动一次受管脚本运行。
    ///
    /// 所有校验都在派生之前完成；任一项不通过就返回可执行错误且不产生子进程。
    ///
    /// `workspace_id` 由守护进程按已登记的 `workspace_id` 原样注入：运行快照与
    /// 生命周期事件都沿用它，客户端才能把事件回挂到具体工作区，而不是靠路径
    /// 末段猜一个标识。
    pub fn run(
        &self,
        workspace_id: &str,
        workspace_root: &Path,
        entry: &ManagedScriptEntry,
        options: RunScriptOptions,
    ) -> Result<ScriptRunSnapshot, ActionableError> {
        self.gate.require(DaemonFeature::ManagedScripts)?;
        require_approval(entry, options.approval_id.as_deref())?;
        let arguments = build_arguments(entry, &options.user_arguments)?;
        let workdir = resolve_workdir(workspace_root, &entry.workdir)?;
        let program = resolve_program(&entry.program)?;
        let (clear_env, environment) = build_environment(entry);

        if tokio::runtime::Handle::try_current().is_err() {
            return Err(ActionableError::new(
                ErrorCode::InternalError,
                "脚本执行器需要异步运行时",
                "该脚本未执行",
                "请在守护进程运行时内调用执行器",
            ));
        }

        if let Some(key) = options.idempotency_key.as_ref() {
            let guard = lock(&self.state);
            if let Some(run_id) = guard.idempotent.get(key.as_str())
                && let Some(existing) = guard.runs.get(run_id)
            {
                return Ok(existing.snapshot());
            }
        }

        if self.active.load(Ordering::Acquire) >= SCRIPT_MAX_CONCURRENT_RUNS {
            return Err(ActionableError::new(
                ErrorCode::Conflict,
                "并行的受管脚本已达上限",
                "该脚本未执行，已有运行不受影响",
                "请等待其他脚本结束或停止不需要的运行后重试",
            ));
        }

        let run_id = format!("run_{}", uuid::Uuid::now_v7().simple());
        let snapshot = ScriptRunSnapshot {
            run_id: run_id.clone(),
            workspace_id: workspace_id.to_owned(),
            script_id: entry.script_id.clone(),
            state: ScriptRunState::Queued,
            exit_code: None,
            started_at: None,
            finished_at: None,
            idempotency_key: options.idempotency_key.clone(),
        };
        let (stop, stop_rx) = mpsc::channel::<bool>(1);
        let run = Arc::new(ScriptRun {
            snapshot: Mutex::new(snapshot.clone()),
            output: Arc::new(OutputBudget::new(SCRIPT_OUTPUT_LIMIT_BYTES)),
            stop,
            reap: Mutex::new(None),
        });

        {
            let mut guard = lock(&self.state);
            guard.runs.insert(run_id.clone(), Arc::clone(&run));
            guard.order.push(run_id.clone());
            if let Some(key) = options.idempotency_key.as_ref() {
                guard
                    .idempotent
                    .insert(key.as_str().to_owned(), run_id.clone());
            }
        }
        self.active.fetch_add(1, Ordering::AcqRel);
        emit_lifecycle(
            &self.sink,
            ScriptLifecyclePhase::Queued,
            &ScriptRunSnapshot {
                state: ScriptRunState::Queued,
                ..snapshot.clone()
            },
        );

        let spec = SpawnSpec {
            program,
            arguments,
            workdir,
            clear_env,
            environment,
        };
        let task_run = Arc::clone(&run);
        let task_sink = Arc::clone(&self.sink);
        let task_state = Arc::clone(&self.state);
        let task_active = Arc::clone(&self.active);
        let task_entry = entry.clone();
        tokio::spawn(async move {
            execute(
                task_state,
                task_active,
                task_run,
                task_sink,
                task_entry,
                spec,
                stop_rx,
            )
            .await;
        });

        Ok(snapshot)
    }

    /// 查询运行快照。
    pub fn inspect(&self, run_id: &str) -> Result<ScriptRunSnapshot, ActionableError> {
        self.gate.require(DaemonFeature::ManagedScripts)?;
        let guard = lock(&self.state);
        guard
            .runs
            .get(run_id)
            .map(|run| run.snapshot())
            .ok_or_else(|| unknown_run(run_id))
    }

    /// 查询运行输出。
    pub fn output(&self, run_id: &str) -> Result<RunOutput, ActionableError> {
        self.gate.require(DaemonFeature::ManagedScripts)?;
        let guard = lock(&self.state);
        guard
            .runs
            .get(run_id)
            .map(|run| run.output())
            .ok_or_else(|| unknown_run(run_id))
    }

    /// 查询回收结论。
    pub fn reap_outcome(&self, run_id: &str) -> Result<Option<ReapOutcome>, ActionableError> {
        let guard = lock(&self.state);
        guard
            .runs
            .get(run_id)
            .map(|run| run.reap_outcome())
            .ok_or_else(|| unknown_run(run_id))
    }

    /// 请求停止：`force` 为真时直接强制终止进程树，否则先温和终止再按回收窗口升级。
    pub fn stop(&self, run_id: &str, force: bool) -> Result<ScriptRunSnapshot, ActionableError> {
        self.gate.require(DaemonFeature::ManagedScripts)?;
        let guard = lock(&self.state);
        let run = guard.runs.get(run_id).ok_or_else(|| unknown_run(run_id))?;
        let snapshot = run.snapshot();
        if is_terminal_state(snapshot.state) {
            return Ok(snapshot);
        }
        let stopping = run.transition(ScriptRunState::Stopping);
        let _ = run.stop.try_send(force);
        Ok(stopping)
    }

    /// 已登记运行条数（含已结束但仍在保留窗口内的运行）。
    #[must_use]
    pub fn retained_runs(&self) -> usize {
        lock(&self.state).runs.len()
    }
}

struct SpawnSpec {
    program: PathBuf,
    arguments: Vec<OsString>,
    workdir: PathBuf,
    clear_env: bool,
    environment: Vec<(OsString, Option<OsString>)>,
}

enum Trigger {
    Exited(std::io::Result<ExitStatus>),
    Stop { force: bool },
    Timeout,
}

async fn execute(
    state: Arc<Mutex<RunnerState>>,
    active: Arc<AtomicUsize>,
    run: Arc<ScriptRun>,
    sink: Arc<dyn ScriptEventSink>,
    entry: ManagedScriptEntry,
    spec: SpawnSpec,
    mut stop_rx: mpsc::Receiver<bool>,
) {
    let mut command = Command::new(&spec.program);
    command
        .args(&spec.arguments)
        .current_dir(child_facing_path(&spec.workdir))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if spec.clear_env {
        command.env_clear();
    }
    for (key, value) in &spec.environment {
        match value {
            Some(value) => command.env(key, value),
            None => command.env_remove(key),
        };
    }
    let reaper = ProcessTreeReaper::configure(&mut command);

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(err) => {
            *lock(&run.reap) = Some(ReapOutcome::SpawnFailed);
            let snapshot = run.finish(ScriptRunState::Failed, None);
            emit_lifecycle(&sink, ScriptLifecyclePhase::Failed, &snapshot);
            active.fetch_sub(1, Ordering::AcqRel);
            release_terminal(&state);
            tracing::warn!(
                script_id = entry.script_id.as_str(),
                kind = ?err.kind(),
                "受管脚本派生失败"
            );
            return;
        }
    };

    let pid = child.id();
    let started = run.transition(ScriptRunState::Running);
    emit_lifecycle(&sink, ScriptLifecyclePhase::Started, &started);

    let budget = Arc::clone(&run.output);
    if let Some(stdout) = child.stdout.take() {
        let budget = Arc::clone(&budget);
        tokio::spawn(async move { drain(stdout, budget).await });
    }
    if let Some(stderr) = child.stderr.take() {
        let budget = Arc::clone(&budget);
        tokio::spawn(async move { drain(stderr, budget).await });
    }

    let timeout = entry
        .timeout_seconds
        .map(|seconds| Duration::from_secs(u64::from(seconds)));
    let deadline = timeout.map(|limit| tokio::time::Instant::now() + limit);

    // 用 try_wait 轮询而不是长期持有 `child.wait()` future：
    // 后者会把 `child` 的独占借用一直抳住，导致终止阶段无法再取 `&mut child`。
    let trigger = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Trigger::Exited(Ok(status)),
            Ok(None) => {}
            Err(err) => break Trigger::Exited(Err(err)),
        }
        if deadline.is_some_and(|deadline| tokio::time::Instant::now() >= deadline) {
            break Trigger::Timeout;
        }
        tokio::select! {
            () = tokio::time::sleep(WAIT_POLL_INTERVAL) => {}
            request = stop_rx.recv() => break Trigger::Stop { force: request.unwrap_or(true) },
        }
    };

    let mut reap = ReapOutcome::NotRequired;
    let mut waited: Option<ExitStatus> = None;
    let mut stopped = false;
    match trigger {
        Trigger::Exited(Ok(status)) => waited = Some(status),
        Trigger::Exited(Err(_)) => reap = ReapOutcome::ReapUnconfirmed,
        Trigger::Stop { force } => {
            stopped = true;
            run.transition(ScriptRunState::Stopping);
            waited = terminate_and_reap(
                &mut child,
                pid,
                reaper.as_ref(),
                force,
                &mut reap,
                &mut stop_rx,
            )
            .await;
        }
        Trigger::Timeout => {
            stopped = true;
            run.transition(ScriptRunState::Stopping);
            waited = terminate_and_reap(
                &mut child,
                pid,
                reaper.as_ref(),
                true,
                &mut reap,
                &mut stop_rx,
            )
            .await;
        }
    }

    *lock(&run.reap) = Some(reap);
    let final_state = if stopped {
        ScriptRunState::Cancelled
    } else if waited.is_some() {
        ScriptRunState::Exited
    } else {
        ScriptRunState::Failed
    };
    let exit_code = waited.and_then(|status| status.code());
    let snapshot = run.finish(final_state, exit_code);
    let phase = match final_state {
        ScriptRunState::Exited => ScriptLifecyclePhase::Exited,
        ScriptRunState::Cancelled => ScriptLifecyclePhase::Stopped,
        _ => ScriptLifecyclePhase::Failed,
    };
    emit_lifecycle(&sink, phase, &snapshot);
    active.fetch_sub(1, Ordering::AcqRel);
    release_terminal(&state);
}

#[allow(clippy::too_many_arguments)]
async fn terminate_and_reap(
    child: &mut tokio::process::Child,
    pid: Option<u32>,
    reaper: Option<&ProcessTreeReaper>,
    force: bool,
    reap: &mut ReapOutcome,
    stop_rx: &mut mpsc::Receiver<bool>,
) -> Option<ExitStatus> {
    // 强制终止途中若客户端再次要求强制，就地升级为硬终止。
    let escalate = stop_rx.try_recv().unwrap_or(false);
    let terminated = match (reaper, pid) {
        (Some(reaper), Some(pid)) => reaper.terminate(pid, force || escalate).await,
        _ => child.start_kill().is_ok(),
    };
    if !terminated {
        let _ = child.start_kill();
    }
    // 第一段：有界等待确认退出。
    if let Some(status) = poll_exit(child, SCRIPT_REAP_WINDOW).await {
        *reap = ReapOutcome::ExitedAfterStop;
        return Some(status);
    }
    // 回收窗口内未确认退出：补一次硬终止，仍有界等待，绝不无限期阻塞。
    let _ = child.start_kill();
    match poll_exit(child, SCRIPT_REAP_WINDOW).await {
        Some(status) => {
            *reap = ReapOutcome::ExitedAfterStop;
            Some(status)
        }
        None => {
            *reap = ReapOutcome::ReapUnconfirmed;
            None
        }
    }
}

/// 有界轮询子进程退出：窗口内退出返回状态，否则返回 `None`（不宣称已回收）。
async fn poll_exit(child: &mut tokio::process::Child, window: Duration) -> Option<ExitStatus> {
    let deadline = tokio::time::Instant::now() + window;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Ok(None) => {}
            Err(_) => return None,
        }
        if tokio::time::Instant::now() >= deadline {
            return None;
        }
        tokio::time::sleep(WAIT_POLL_INTERVAL).await;
    }
}

async fn drain<R: AsyncRead + Unpin>(mut reader: R, budget: Arc<OutputBudget>) {
    let mut buffer = [0u8; 8192];
    loop {
        match reader.read(&mut buffer).await {
            Ok(0) => return,
            Ok(read) => budget.push(&buffer[..read]),
            Err(_) => return,
        }
    }
}

/// 进程树终止器。
///
/// 与 `soxu-drivers` 的原生 CLI 回收保持同一策略：Unix 让子进程自成进程组后
/// 以系统 `kill` 终止整个组；Windows 用 `taskkill /T` 连带子孙进程。
/// 两条路径都带**有界等待**，绝不无限期阻塞在回收上。
struct ProcessTreeReaper {
    program: PathBuf,
}

impl ProcessTreeReaper {
    fn configure(command: &mut Command) -> Option<Self> {
        #[cfg(unix)]
        {
            command.process_group(0);
            ["/bin/kill", "/usr/bin/kill"]
                .into_iter()
                .map(PathBuf::from)
                .find(|path| path.is_file())
                .map(|program| Self { program })
        }
        #[cfg(windows)]
        {
            let _ = command;
            std::env::var_os("SystemRoot")
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .map(|path| path.join("System32").join("taskkill.exe"))
                .filter(|path| path.is_file())
                .map(|program| Self { program })
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = command;
            None
        }
    }

    async fn terminate(&self, pid: u32, force: bool) -> bool {
        let mut command = Command::new(&self.program);
        #[cfg(unix)]
        {
            let signal = if force { "KILL" } else { "TERM" };
            command.args(["-s", signal, "--", &format!("-{pid}")]);
        }
        #[cfg(windows)]
        {
            command.args(["/PID", &pid.to_string(), "/T"]);
            if force {
                command.arg("/F");
            }
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = (pid, force);
            return false;
        }
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        tokio::time::timeout(Duration::from_secs(5), command.status())
            .await
            .is_ok_and(|result| result.is_ok_and(|status| status.success()))
    }
}

fn require_approval(
    entry: &ManagedScriptEntry,
    approval_id: Option<&str>,
) -> Result<(), ActionableError> {
    if !entry.risk.requires_approval() {
        return Ok(());
    }
    if approval_id.is_some_and(|id| !id.trim().is_empty()) {
        return Ok(());
    }
    Err(ActionableError::new(
        ErrorCode::PolicyDenied,
        format!("高风险脚本 `{}` 缺少逐次审批标记", entry.script_id.as_str()),
        "该脚本未执行，未派生子进程，也没有修改任何文件",
        "请先在审批卡上逐次批准，并在启动请求中携带 approval_id",
    ))
}

fn build_arguments(
    entry: &ManagedScriptEntry,
    user: &ScriptUserArguments,
) -> Result<Vec<OsString>, ActionableError> {
    let policy = &entry.argument_policy;
    if !policy.accepts_user_arguments && !user.as_slice().is_empty() {
        return Err(ActionableError::new(
            ErrorCode::ValidationFailed,
            "该脚本不接受透传参数",
            "该脚本未执行",
            "请移除请求中的 user_arguments 后重试",
        ));
    }
    if user.as_slice().len() > usize::from(policy.max_user_arguments) {
        return Err(ActionableError::new(
            ErrorCode::ValidationFailed,
            format!("透传参数条数超出上限（{} 条）", policy.max_user_arguments),
            "该脚本未执行",
            "请减少透传参数条数后重试",
        ));
    }
    let limit = usize::from(policy.max_user_argument_bytes);
    for argument in user.as_slice() {
        if argument.as_str().len() > limit {
            return Err(ActionableError::new(
                ErrorCode::ValidationFailed,
                format!("单条透传参数超过 {limit} 字节上限"),
                "该脚本未执行",
                "请缩短该参数或改用固定参数声明",
            ));
        }
    }
    let mut arguments: Vec<OsString> = policy
        .fixed_arguments
        .as_slice()
        .iter()
        .map(|argument| OsString::from(argument.as_str()))
        .collect();
    arguments.extend(
        user.as_slice()
            .iter()
            .map(|argument| OsString::from(argument.as_str())),
    );
    Ok(arguments)
}

fn build_environment(entry: &ManagedScriptEntry) -> (bool, Vec<(OsString, Option<OsString>)>) {
    let clear = !entry.env_policy.inherits_parent_env;
    let mut environment = Vec::new();
    let mut seen = BTreeSet::new();
    for key in &entry.env_policy.allow_keys {
        if !seen.insert(key.as_str().to_owned()) {
            continue;
        }
        if let Some(value) = std::env::var_os(key.as_str()) {
            environment.push((OsString::from(key.as_str()), Some(value)));
        }
    }
    (clear, environment)
}

/// 解析脚本程序：带路径分隔符必须是存在的绝对文件，裸名按 `PATH` 查找。
fn resolve_program(program: &ScriptProgram) -> Result<PathBuf, ActionableError> {
    let raw = program.as_str();
    if raw.contains('/') || raw.contains('\\') {
        let path = PathBuf::from(raw);
        if !path.is_absolute() {
            return Err(ActionableError::new(
                ErrorCode::ValidationFailed,
                "脚本程序路径必须是绝对路径",
                "该脚本未执行",
                "请在 paseo.json 中改用绝对程序路径或裸程序名",
            ));
        }
        if !path.is_file() {
            return Err(ActionableError::new(
                ErrorCode::ValidationFailed,
                format!("脚本程序不存在：{raw}"),
                "该脚本未执行",
                "请确认程序路径存在且是可执行文件",
            ));
        }
        return Ok(path);
    }
    let Some(path) = std::env::var_os("PATH") else {
        return Err(program_missing(raw));
    };
    for directory in std::env::split_paths(&path).filter(|dir| dir.is_absolute()) {
        let candidate = directory.join(raw);
        if candidate.is_file() {
            return Ok(candidate);
        }
        if cfg!(windows) {
            let executable = candidate.with_extension("exe");
            if executable.is_file() {
                return Ok(executable);
            }
        }
    }
    Err(program_missing(raw))
}

fn program_missing(raw: &str) -> ActionableError {
    ActionableError::new(
        ErrorCode::ValidationFailed,
        format!("未在 PATH 中找到脚本程序：{raw}"),
        "该脚本未执行",
        "请安装该程序或改用绝对路径声明，并确认 PATH 对守护进程可见",
    )
}

/// 工作区标识：沿用守护进程对工作区目录的稳定命名口径（目录名）。
fn unknown_run(run_id: &str) -> ActionableError {
    ActionableError::new(
        ErrorCode::NotFound,
        format!("未找到脚本运行：{run_id}"),
        "该查询未返回任何运行数据",
        "请确认 run_id 属于当前守护进程实例，并重新查询清单后启动",
    )
}

fn emit_lifecycle(
    sink: &Arc<dyn ScriptEventSink>,
    phase: ScriptLifecyclePhase,
    snapshot: &ScriptRunSnapshot,
) {
    sink.emit(ScriptLifecycleEvent {
        phase,
        run: snapshot.clone(),
        occurred_at: Utc::now(),
    });
}

fn release_terminal(state: &Arc<Mutex<RunnerState>>) {
    let mut guard = lock(state);
    while guard.runs.len() > SCRIPT_MAX_RETAINED_RUNS {
        let position = {
            let view = &*guard;
            view.order
                .iter()
                .position(|id| view.runs.get(id).is_some_and(|run| run.is_terminal()))
        };
        let Some(position) = position else {
            return;
        };
        let run_id = guard.order.remove(position);
        guard.runs.remove(&run_id);
        guard.idempotent.retain(|_, value| *value != run_id);
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
