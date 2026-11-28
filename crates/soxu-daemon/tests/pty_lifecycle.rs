//! 受控 PTY 生命周期闭环回归（Issue #161 / 资源治理阶段 2）。
//!
//! 覆盖四类证据：
//! 1. 容量原子化：并发 `open` ×16 只有 `PTY_MAX_SESSIONS` 个成功，且失败请求**不派生进程**；
//! 2. 终态即释放：自然退出的会话立即归还容量，活动表为空、墓碑有界；
//! 3. 幂等与可诊断：重复 `close` 幂等，已回收会话的 `snapshot` 返回明确终态；
//! 4. 关停与周期维护：`shutdown` 关闭全部会话并有界等待输出泵收敛，维护任务回收空闲
//!    会话与过期墓碑。
//!
//! 进程树证据边界：Windows 用例用 `taskkill /T` 之外的独立进程快照断言子孙进程已消失；
//! Unix 侧的组信号收敛由 CI（Linux）覆盖，本文件只断言会话与直接子进程。

use soxu_daemon::pty::{
    PTY_IDLE_TIMEOUT, PTY_MAX_SESSIONS, PTY_PUMP_JOIN_GRACE, PTY_TOMBSTONE_MAX, PTY_TOMBSTONE_TTL,
    PtyBackend, PtyChild, PtyControl, PtyEventSink, PtyHost, PtyMaintenanceReport, PtySpawnSpec,
};
use soxu_daemon::scripts::FeatureGate;
use soxu_protocol::{
    ActionableError, DaemonFeature, ErrorCode, OpenPtyRequest, PtyCloseReason, PtyEnvPolicy,
    PtyEvent, PtyExitStatus, PtyInputData, PtySessionId, PtySessionState, PtySize,
    PtyWorkdirPolicy,
};
use std::io::{Cursor, Read};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// 夹具
// ---------------------------------------------------------------------------

#[derive(Default)]
struct RecordingSink {
    events: Mutex<Vec<PtyEvent>>,
}

impl RecordingSink {
    fn text(&self) -> String {
        self.events
            .lock()
            .expect("事件锁")
            .iter()
            .filter_map(|event| match event {
                PtyEvent::Output(chunk) => Some(chunk.data.as_str().to_owned()),
                _ => None,
            })
            .collect()
    }

    fn state_events(&self, predicate: impl Fn(&PtyEvent) -> bool) -> usize {
        self.events
            .lock()
            .expect("事件锁")
            .iter()
            .filter(|event| predicate(event))
            .count()
    }
}

impl PtyEventSink for RecordingSink {
    fn emit(&self, event: PtyEvent) {
        self.events.lock().expect("事件锁").push(event);
    }
}

fn host_with(backend: Arc<dyn PtyBackend>, sink: Arc<RecordingSink>) -> PtyHost {
    PtyHost::new(
        FeatureGate::from_features([DaemonFeature::ControlledPty]),
        sink as Arc<dyn PtyEventSink>,
        backend,
    )
}

fn enabled_host(backend: Arc<dyn PtyBackend>) -> PtyHost {
    host_with(backend, Arc::new(RecordingSink::default()))
}

fn open_request() -> OpenPtyRequest {
    OpenPtyRequest {
        workspace_id: "wks_test".to_owned(),
        size: PtySize::new(100, 30).expect("合法尺寸"),
        workdir: PtyWorkdirPolicy::WorkspaceRoot,
        env_policy: PtyEnvPolicy::default(),
    }
}

fn wait_until(what: &str, mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !predicate() {
        assert!(Instant::now() < deadline, "{what} 未在 10s 内有界到达");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// 保持会话运行的后端：读端阻塞到 `terminate`，用于稳定验证容量与关停语义。
struct HoldOpenBackend {
    spawned: AtomicUsize,
    terminated: Arc<AtomicUsize>,
    gates: Mutex<Vec<Arc<Gate>>>,
}

impl HoldOpenBackend {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            spawned: AtomicUsize::new(0),
            terminated: Arc::new(AtomicUsize::new(0)),
            gates: Mutex::new(Vec::new()),
        })
    }

    fn spawned(&self) -> usize {
        self.spawned.load(Ordering::Acquire)
    }

    fn terminated(&self) -> usize {
        self.terminated.load(Ordering::Acquire)
    }
}

impl PtyBackend for HoldOpenBackend {
    fn spawn(&self, _spec: &PtySpawnSpec) -> Result<PtyChild, ActionableError> {
        self.spawned.fetch_add(1, Ordering::AcqRel);
        // 每个会话一份独立闸门：终止一个会话不得让其它会话被动地看到 EOF。
        let gate = Gate::new();
        self.gates.lock().expect("闸门表").push(Arc::clone(&gate));
        Ok(PtyChild {
            reader: Box::new(GatedReader {
                gate: Arc::clone(&gate),
            }),
            writer: Box::new(std::io::sink()),
            control: Arc::new(HoldControl {
                gate,
                terminated: Arc::clone(&self.terminated),
            }),
        })
    }
}

/// 读端闸门：`true` 表示仍然打开（阻塞读），`terminate` 之后转为 EOF。
struct Gate {
    open: Mutex<bool>,
    changed: Condvar,
}

impl Gate {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            open: Mutex::new(true),
            changed: Condvar::new(),
        })
    }

    fn close(&self) {
        let mut open = self.open.lock().expect("闸门锁");
        *open = false;
        self.changed.notify_all();
    }

    fn is_closed(&self) -> bool {
        !*self.open.lock().expect("闸门锁")
    }
}

struct GatedReader {
    gate: Arc<Gate>,
}

impl Read for GatedReader {
    fn read(&mut self, _buffer: &mut [u8]) -> std::io::Result<usize> {
        let mut open = self.gate.open.lock().expect("闸门锁");
        while *open {
            open = self.gate.changed.wait(open).expect("闸门等待");
        }
        Ok(0)
    }
}

struct HoldControl {
    gate: Arc<Gate>,
    terminated: Arc<AtomicUsize>,
}

impl PtyControl for HoldControl {
    fn resize(&self, _size: PtySize) -> Result<(), ActionableError> {
        Ok(())
    }

    fn terminate(&self) -> Result<(), ActionableError> {
        self.terminated.fetch_add(1, Ordering::AcqRel);
        self.gate.close();
        Ok(())
    }

    fn try_wait(&self) -> Result<Option<PtyExitStatus>, ActionableError> {
        if self.gate.is_closed() {
            return Ok(Some(PtyExitStatus {
                exit_code: Some(0),
                exit_signal: None,
            }));
        }
        Ok(None)
    }
}

/// 派生后立即 EOF 的后端：模拟 shell 自然退出。
struct ImmediateExitBackend;

impl PtyBackend for ImmediateExitBackend {
    fn spawn(&self, _spec: &PtySpawnSpec) -> Result<PtyChild, ActionableError> {
        Ok(PtyChild {
            reader: Box::new(Cursor::new(Vec::new())),
            writer: Box::new(std::io::sink()),
            control: Arc::new(ImmediateControl),
        })
    }
}

struct ImmediateControl;

impl PtyControl for ImmediateControl {
    fn resize(&self, _size: PtySize) -> Result<(), ActionableError> {
        Ok(())
    }

    fn terminate(&self) -> Result<(), ActionableError> {
        Ok(())
    }

    fn try_wait(&self) -> Result<Option<PtyExitStatus>, ActionableError> {
        Ok(Some(PtyExitStatus {
            exit_code: Some(0),
            exit_signal: None,
        }))
    }
}

fn input(text: &str) -> PtyInputData {
    PtyInputData::try_from(text.to_owned()).expect("合法输入")
}

// ---------------------------------------------------------------------------
// 1. 容量原子化
// ---------------------------------------------------------------------------

#[test]
fn concurrent_open_is_bounded_by_atomic_capacity() {
    let backend = HoldOpenBackend::new();
    let host = Arc::new(enabled_host(backend.clone()));
    let root = tempfile::tempdir().expect("临时工作区");
    let root = Arc::new(root.path().to_path_buf());
    let barrier = Arc::new(std::sync::Barrier::new(16));

    let mut threads = Vec::new();
    for _ in 0..16 {
        let host = Arc::clone(&host);
        let root = Arc::clone(&root);
        let barrier = Arc::clone(&barrier);
        threads.push(std::thread::spawn(move || {
            barrier.wait();
            host.open(&root, open_request())
        }));
    }
    let results: Vec<_> = threads
        .into_iter()
        .map(|thread| thread.join().expect("并发线程不得 panic"))
        .collect();

    let accepted = results.iter().filter(|result| result.is_ok()).count();
    let rejected = results
        .iter()
        .filter(|result| {
            result
                .as_ref()
                .is_err_and(|error| error.code == ErrorCode::Conflict)
        })
        .count();
    assert_eq!(
        accepted, PTY_MAX_SESSIONS,
        "并发打开最多成功 {PTY_MAX_SESSIONS} 个"
    );
    assert_eq!(
        rejected,
        16 - PTY_MAX_SESSIONS,
        "超限请求必须返回可诊断上限错误"
    );
    assert_eq!(
        host.live_sessions(),
        PTY_MAX_SESSIONS,
        "活动会话数必须与成功数一致"
    );
    assert_eq!(
        backend.spawned(),
        accepted,
        "被拒绝的请求绝不能派生 shell 进程"
    );

    let report = host.shutdown(PTY_PUMP_JOIN_GRACE);
    assert_eq!(report.requested, PTY_MAX_SESSIONS);
    assert_eq!(report.confirmed, PTY_MAX_SESSIONS);
    assert!(report.unconfirmed.is_empty(), "受控后端必须全部收敛");
    assert_eq!(host.live_sessions(), 0);
}

// ---------------------------------------------------------------------------
// 2. 终态即释放 + 表项有界
// ---------------------------------------------------------------------------

#[test]
fn natural_exit_releases_capacity_and_only_leaves_bounded_tombstones() {
    let host = enabled_host(Arc::new(ImmediateExitBackend));
    let root = tempfile::tempdir().expect("临时工作区");
    let mut last_id = None;
    for round in 0..20u32 {
        let session = host
            .open(root.path(), open_request())
            .unwrap_or_else(|error| panic!("第 {round} 次打开必须成功（容量应已释放）：{error:?}"));
        wait_until("自然退出后活动表清空", || {
            host.live_sessions() == 0
        });
        last_id = Some(session.pty_id);
    }
    let stats = host.table_stats();
    assert_eq!(stats.live, 0, "自然退出的会话不得继续占用活动表");
    assert!(
        stats.tombstones <= PTY_TOMBSTONE_MAX,
        "墓碑必须有界，实际 {} 条",
        stats.tombstones
    );

    let retired = last_id.expect("至少打开过一次");
    let snapshot = host.snapshot(&retired).expect("已回收会话应返回墓碑快照");
    assert_eq!(
        snapshot.state,
        PtySessionState::Exited,
        "自然退出必须保留明确终态"
    );
}

#[test]
fn tombstones_are_bounded_and_expire_via_maintenance() {
    let host = enabled_host(Arc::new(ImmediateExitBackend));
    let root = tempfile::tempdir().expect("临时工作区");
    for _ in 0..(PTY_TOMBSTONE_MAX + 20) {
        host.open(root.path(), open_request()).expect("打开会话");
        wait_until("活动表清空", || host.live_sessions() == 0);
    }
    assert_eq!(
        host.table_stats().tombstones,
        PTY_TOMBSTONE_MAX,
        "墓碑条数必须被上限截断"
    );

    let report = host.maintain(Instant::now() + PTY_TOMBSTONE_TTL + Duration::from_secs(1));
    assert_eq!(report.expired_tombstones, PTY_TOMBSTONE_MAX);
    assert_eq!(host.table_stats().tombstones, 0, "过期墓碑必须被清理");
}

// ---------------------------------------------------------------------------
// 3. 幂等关闭与可诊断快照
// ---------------------------------------------------------------------------

#[test]
fn close_is_idempotent_and_retired_snapshot_is_terminal() {
    let backend = HoldOpenBackend::new();
    let sink = Arc::new(RecordingSink::default());
    let host = host_with(backend.clone(), Arc::clone(&sink));
    let root = tempfile::tempdir().expect("临时工作区");
    let session = host.open(root.path(), open_request()).expect("打开会话");

    let first = host
        .close(&session.pty_id, PtyCloseReason::ClientRequest)
        .expect("首次关闭");
    let second = host
        .close(&session.pty_id, PtyCloseReason::ClientRequest)
        .expect("重复关闭必须幂等");
    assert_eq!(first.state, PtySessionState::Closed);
    assert_eq!(second.state, PtySessionState::Closed);
    assert_eq!(host.live_sessions(), 0);
    assert_eq!(backend.terminated(), 1, "重复关闭不得重复终止进程");
    assert_eq!(host.table_stats().tombstones, 1);

    let snapshot = host
        .snapshot(&session.pty_id)
        .expect("已回收会话的快照必须返回明确终态而不是内部错误");
    assert_eq!(snapshot.state, PtySessionState::Closed);
    assert_eq!(snapshot.pty_id, session.pty_id);

    let error = host
        .write(&session.pty_id, input("ls\n"))
        .expect_err("已结束会话必须拒绝写入");
    assert_eq!(error.code, ErrorCode::Conflict);
    assert!(
        error.summary_zh.contains("已结束"),
        "错误摘要必须说明会话已结束：{}",
        error.summary_zh
    );

    let unknown = PtySessionId::try_from("pty_99999999".to_owned()).expect("会话标识");
    assert_eq!(
        host.snapshot(&unknown).expect_err("未知会话必须报错").code,
        ErrorCode::NotFound
    );
    let _ = &sink;
}

// ---------------------------------------------------------------------------
// 4. 周期维护
// ---------------------------------------------------------------------------

#[test]
fn maintenance_reaps_idle_sessions_into_tombstones() {
    let host = enabled_host(HoldOpenBackend::new());
    let root = tempfile::tempdir().expect("临时工作区");
    let session = host.open(root.path(), open_request()).expect("打开会话");

    assert_eq!(
        host.maintain(Instant::now()),
        PtyMaintenanceReport::default(),
        "刚打开的会话不得被回收"
    );
    // 空闲回收与墓碑过期共用同一时间基准，这里只验证回收：用一个不越过墓碑 TTL 的
    // 时刻（空闲阈值 900s > TTL 300s，所以直接驱动 `reap_idle` 看快照）。
    let reaped = host.reap_idle(Instant::now() + PTY_IDLE_TIMEOUT + Duration::from_secs(1));
    assert_eq!(reaped, 1);
    assert_eq!(host.live_sessions(), 0);
    assert_eq!(
        host.snapshot(&session.pty_id).expect("墓碑快照").state,
        PtySessionState::Closed
    );
    // 超过墓碑 TTL 的维护必须把墓碑一并清掉，不留下无界表项。
    let report = host.maintain(Instant::now() + PTY_TOMBSTONE_TTL + Duration::from_secs(1));
    assert_eq!(report.expired_tombstones, 1);
    assert_eq!(host.table_stats().tombstones, 0);
}

// ---------------------------------------------------------------------------
// 5. 关停收敛
// ---------------------------------------------------------------------------

/// 宿主释放（daemon 退出）时必须兜底关闭仍活动的会话。
#[test]
fn dropping_host_closes_live_sessions() {
    let backend = HoldOpenBackend::new();
    let root = tempfile::tempdir().expect("临时工作区");
    {
        let host = enabled_host(backend.clone());
        let _session = host.open(root.path(), open_request()).expect("打开会话");
    }
    assert_eq!(backend.terminated(), 1, "宿主释放必须终止仍活动的 shell");
}

#[test]
fn shutdown_closes_every_session_and_reports_convergence() {
    let backend = HoldOpenBackend::new();
    let sink = Arc::new(RecordingSink::default());
    let host = host_with(backend.clone(), Arc::clone(&sink));
    let root = tempfile::tempdir().expect("临时工作区");
    let mut opened = Vec::new();
    for _ in 0..3 {
        opened.push(
            host.open(root.path(), open_request())
                .expect("打开会话")
                .pty_id,
        );
    }

    let report = host.shutdown(PTY_PUMP_JOIN_GRACE);
    assert_eq!(report.requested, 3);
    assert_eq!(report.confirmed, 3);
    assert!(report.unconfirmed.is_empty(), "受控后端必须全部收敛");
    assert_eq!(host.live_sessions(), 0);
    assert_eq!(backend.terminated(), 3, "每个会话都必须终止一次");

    for pty_id in opened {
        let snapshot = host.snapshot(&pty_id).expect("关停后仍可查快照");
        assert_eq!(snapshot.state, PtySessionState::Closed);
    }
    assert_eq!(
        sink.state_events(|event| matches!(event, PtyEvent::Closed(_))),
        3,
        "关停必须为每个会话下发一次 Closed"
    );
}

// ---------------------------------------------------------------------------
// 6. 真实后端与进程树证据（仅 Windows）
// ---------------------------------------------------------------------------

/// Windows：daemon 关停后，PTY 直接子进程（受控 shell）不得存活。
///
/// 证据边界：本用例断言「PTY 会话直接派生的 shell 进程」在关停窗口内消失；由 shell
/// 再派生的孙进程树由 `process_tree_termination_reaps_grandchild` 用同一套
/// `taskkill /T` 终止器独立证明。Unix 的组信号收敛由 Linux CI 覆盖。
#[cfg(windows)]
#[test]
fn windows_shutdown_leaves_no_pty_child_process() {
    use soxu_daemon::pty::PortablePtyBackend;

    let Ok(backend) = PortablePtyBackend::discover() else {
        eprintln!("跳过：本机没有可用的受控 shell");
        return;
    };
    let sink = Arc::new(RecordingSink::default());
    let host = host_with(Arc::new(backend), Arc::clone(&sink));
    let root = tempfile::tempdir().expect("临时工作区");
    let session = host
        .open(root.path(), open_request())
        .expect("打开真实终端");

    // 交互式 PowerShell 会先向终端发 DSR（`ESC[6n`）并等位置回报：给一次回报再敲命令，
    // 否则输入一直堆在 PSReadLine 之前，命令不会被执行。
    let _ = host.write(&session.pty_id, input("\u{1b}[1;1R"));
    std::thread::sleep(Duration::from_millis(800));
    host.write(&session.pty_id, input("Write-Output \"SOXU-SHELL=$PID\"\r"))
        .expect("写入自报 pid 命令");

    let shell_pid = wait_child_pid(&sink, "SOXU-SHELL=");
    assert!(
        process_alive(shell_pid),
        "终端 shell 应当存活（pid {shell_pid}）"
    );

    let report = host.shutdown(PTY_PUMP_JOIN_GRACE);
    assert_eq!(report.requested, 1);
    assert!(report.unconfirmed.is_empty(), "真实终端必须在窗口内收敛");

    wait_process_gone(shell_pid, "关停后终端 shell 仍存活");
}

/// Windows：进程树终止器（PTY 关闭与脚本关停共用）必须连带回收孙进程。
#[cfg(windows)]
#[test]
fn process_tree_termination_reaps_grandchild() {
    let exe = std::env::current_exe().expect("当前测试二进制");
    let mut parent = std::process::Command::new(exe)
        .args([
            "--exact",
            "stub_parent_spawns_grandchild",
            "--ignored",
            "--nocapture",
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("派生受控父进程");
    let parent_pid = parent.id();
    let (sender, receiver) = std::sync::mpsc::channel();
    if let Some(stdout) = parent.stdout.take() {
        std::thread::spawn(move || {
            use std::io::{BufRead, BufReader};
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Some(rest) = line.strip_prefix("SOXU-GRANDCHILD=")
                    && let Ok(pid) = rest.trim().parse::<u32>()
                {
                    let _ = sender.send(pid);
                    return;
                }
            }
        });
    }
    let grandchild = receiver
        .recv_timeout(Duration::from_secs(30))
        .expect("未拿到孙进程 pid");
    assert!(process_alive(parent_pid), "父进程应当存活");
    assert!(process_alive(grandchild), "孙进程应当存活");

    assert!(
        soxu_daemon::pty::terminate_process_tree(parent_pid, true),
        "taskkill /T 必须报告成功"
    );
    wait_process_gone(grandchild, "进程树终止后孙进程仍存活");
    wait_process_gone(parent_pid, "进程树终止后父进程仍存活");
    let _ = parent.wait();
}

#[cfg(windows)]
fn wait_child_pid(sink: &Arc<RecordingSink>, marker: &str) -> u32 {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(pid) = find_live_pid(&sink.text(), marker) {
            return pid;
        }
        assert!(
            Instant::now() < deadline,
            "未在 20s 内拿到 `{marker}` 的存活 pid；终端尾部：{}",
            tail_of(&sink.text(), 400)
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[cfg(windows)]
fn find_live_pid(text: &str, marker: &str) -> Option<u32> {
    text.split(marker).skip(1).find_map(|rest| {
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        if digits.is_empty() {
            return None;
        }
        digits.parse::<u32>().ok().filter(|pid| process_alive(*pid))
    })
}

#[cfg(windows)]
fn tail_of(text: &str, limit: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    let start = chars.len().saturating_sub(limit);
    chars[start..].iter().collect()
}

#[cfg(windows)]
fn wait_process_gone(pid: u32, what: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while process_alive(pid) {
        assert!(Instant::now() < deadline, "{what}（pid {pid}）");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[cfg(windows)]
fn process_alive(pid: u32) -> bool {
    // 用独立进程快照（tasklist）判定存活，不依赖被测代码的回收结论。
    let output = std::process::Command::new("tasklist.exe")
        .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output();
    match output {
        Ok(output) => String::from_utf8_lossy(&output.stdout).contains(&format!(",\"{pid}\",")),
        Err(error) => panic!("无法通过 tasklist 核对进程 {pid}：{error}"),
    }
}

/// 受控子进程桩：派生长时间运行的孙进程并自报其 pid，用于验证进程树终止。
#[cfg(windows)]
#[test]
#[ignore = "受控子进程桩：仅由父用例经当前二进制重新执行"]
fn stub_parent_spawns_grandchild() {
    use std::io::Write as _;
    let mut child = std::process::Command::new("cmd.exe")
        .args(["/c", "ping", "-n", "300", "127.0.0.1"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .spawn()
        .expect("派生孙进程");
    println!("SOXU-GRANDCHILD={}", child.id());
    let _ = std::io::stdout().flush();
    std::thread::sleep(Duration::from_secs(300));
    // 正常情况下本桩会被父用例的进程树终止器提前杀掉，这里的 wait 只是让句柄
    // 在「未被外部终止」的路径上也能被回收（属稳安，不是正确性依赖）。
    let _ = child.wait();
}
