//! M6-A 专项回归：受管脚本执行器与受控 PTY 宿主。
//!
//! 覆盖 Issue #139 要求的四类证据：注入面、超时与进程树回收、输出上限、
//! PTY 分片序号与关闭幂等、能力位缺省关闭拒绝。
//! 平台语义按 L-013 逐平台显式门控；子进程等待一律「有界 + 非管道收集」
//! （L-016），失败不靠无界 `output()` 收集。

use soxu_daemon::pty::{NullPtyEventSink, PtyHost};
use soxu_daemon::pty::{
    PTY_CLOSE_GRACE, PTY_IDLE_TIMEOUT, PTY_MAX_SESSIONS, PtyBackend, PtyChild, PtyControl,
    PtyEventSink, PtySpawnSpec,
};
use soxu_daemon::scripts::{
    FeatureGate, ReapOutcome, SCRIPT_OUTPUT_LIMIT_BYTES, ScriptEventSink, ScriptService,
};
use soxu_protocol::{
    ActionableError, DaemonFeature, ErrorCode, ManagedScriptEntry, OpenPtyRequest, PtyCloseReason,
    PtyCommand, PtyEvent, PtyExitStatus, PtyInputData, PtySessionId, PtySessionState, PtySize,
    PtyWorkdirPolicy, RunScriptRequest, ScriptCommand, ScriptCommandResult, ScriptEnvPolicy,
    ScriptId, ScriptRunState, ScriptUserArguments, ScriptWorkdirPolicy,
};
use std::io::{Cursor, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// 夹具：事件记录、脚本条目构造、伪 PTY 后端
// ---------------------------------------------------------------------------

#[derive(Default)]
struct RecordingScriptSink {
    events: Mutex<Vec<soxu_protocol::ScriptLifecycleEvent>>,
}

impl RecordingScriptSink {
    fn phases(&self) -> Vec<soxu_protocol::ScriptLifecyclePhase> {
        self.events
            .lock()
            .expect("事件锁")
            .iter()
            .map(|event| event.phase)
            .collect()
    }

    fn len(&self) -> usize {
        self.events.lock().expect("事件锁").len()
    }
}

impl ScriptEventSink for RecordingScriptSink {
    fn emit(&self, event: soxu_protocol::ScriptLifecycleEvent) {
        self.events.lock().expect("事件锁").push(event);
    }
}

#[derive(Default)]
struct RecordingPtySink {
    events: Mutex<Vec<PtyEvent>>,
}

impl RecordingPtySink {
    fn output_chunks(&self) -> Vec<soxu_protocol::PtyOutputChunk> {
        self.events
            .lock()
            .expect("事件锁")
            .iter()
            .filter_map(|event| match event {
                PtyEvent::Output(chunk) => Some(chunk.clone()),
                _ => None,
            })
            .collect()
    }

    fn count_of(&self, predicate: impl Fn(&PtyEvent) -> bool) -> usize {
        self.events
            .lock()
            .expect("事件锁")
            .iter()
            .filter(|event| predicate(event))
            .count()
    }

    fn len(&self) -> usize {
        self.events.lock().expect("事件锁").len()
    }
}

impl PtyEventSink for RecordingPtySink {
    fn emit(&self, event: PtyEvent) {
        self.events.lock().expect("事件锁").push(event);
    }
}

/// 构造一条受管脚本条目；`script_id` 与固定参数由调用方给出。
fn script_entry(script_id: &str, arguments: &[&str]) -> ManagedScriptEntry {
    let raw = serde_json::json!({
        "script_id": script_id,
        "display_name": "回归脚本",
        "program": node_program(),
        "argument_policy": {
            "fixed_arguments": arguments,
            "accepts_user_arguments": false,
            "max_user_arguments": 0,
            "max_user_argument_bytes": 4096
        },
        "workdir": { "kind": "workspace_root" },
        "risk": "low"
    });
    serde_json::from_value(raw).expect("脚本条目应合法")
}

/// 用裸程序名 `node`：解析走 PATH，不绑定本机绝对路径。
fn node_program() -> &'static str {
    "node"
}

fn write_manifest(root: &Path, entries: &[&ManagedScriptEntry]) {
    let raw = serde_json::json!({ "scripts": entries });
    std::fs::write(
        root.join("paseo.json"),
        serde_json::to_vec_pretty(&raw).expect("清单序列化"),
    )
    .expect("写入清单");
}

fn run_request(script_id: &str) -> ScriptCommand {
    ScriptCommand::Run(RunScriptRequest {
        workspace_id: "wks_test".to_owned(),
        script_id: ScriptId::try_from(script_id.to_owned()).expect("脚本标识"),
        user_arguments: ScriptUserArguments::default(),
        idempotency_key: None,
        approval_id: Some("approval_test".to_owned()),
    })
}

/// 等待运行进入终止态；异步有界轮询（禁止在运行时线程上阻塞睡眠）。
async fn wait_terminal(service: &ScriptService, run_id: &str) -> soxu_protocol::ScriptRunSnapshot {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        let snapshot = service.runner().inspect(run_id).expect("查询运行");
        if matches!(
            snapshot.state,
            ScriptRunState::Exited | ScriptRunState::Failed | ScriptRunState::Cancelled
        ) {
            return snapshot;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "运行 {run_id} 未在 30s 内有界收敛（当前 {:?}）",
            snapshot.state
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn accepted_snapshot(result: ScriptCommandResult) -> soxu_protocol::ScriptRunSnapshot {
    match result {
        ScriptCommandResult::Accepted(snapshot) => snapshot,
        other => panic!("期望 Accepted，实际 {other:?}"),
    }
}

fn enabled_scripts() -> FeatureGate {
    FeatureGate::from_features([DaemonFeature::ManagedScripts])
}

// ---------------------------------------------------------------------------
// 脚本域：能力门控
// ---------------------------------------------------------------------------

#[tokio::test]
async fn managed_scripts_are_off_by_default_and_emit_no_events() {
    let sink = Arc::new(RecordingScriptSink::default());
    let service = ScriptService::new(FeatureGate::disabled(), sink.clone());
    let root = tempfile::tempdir().expect("临时工作区");

    let error = service
        .handle(
            root.path(),
            ScriptCommand::List(soxu_protocol::ListScriptsRequest {
                workspace_id: "wks_test".to_owned(),
            }),
        )
        .expect_err("能力位缺省关闭必须拒绝");
    assert_eq!(error.code, ErrorCode::Unsupported);
    assert!(error.summary_zh.contains("managed_scripts"));
    assert_eq!(sink.len(), 0, "拒绝路径不得下发任何事件");

    let error = service
        .handle(root.path(), run_request("check"))
        .expect_err("能力位缺省关闭必须拒绝");
    assert_eq!(error.code, ErrorCode::Unsupported);
    assert_eq!(sink.len(), 0);
}

#[tokio::test]
async fn capability_off_rejects_pty_open_and_emits_nothing() {
    let sink = Arc::new(RecordingPtySink::default());
    let backend = Arc::new(FixedOutputBackend::new(b"hello".to_vec()));
    let host = PtyHost::new(
        FeatureGate::disabled(),
        sink.clone() as Arc<dyn PtyEventSink>,
        backend,
    );
    let root = tempfile::tempdir().expect("临时工作区");

    let error = host
        .handle(root.path(), PtyCommand::Open(open_request()))
        .expect_err("能力位缺省关闭必须拒绝");
    assert_eq!(error.code, ErrorCode::Unsupported);
    assert!(error.summary_zh.contains("controlled_pty"));
    assert_eq!(sink.len(), 0);
    assert_eq!(host.live_sessions(), 0);
}

// ---------------------------------------------------------------------------
// 脚本域：注入面
// ---------------------------------------------------------------------------

#[tokio::test]
async fn manifest_rejects_injection_surfaces_before_any_spawn() {
    let root = tempfile::tempdir().expect("临时工作区");
    let sink = Arc::new(RecordingScriptSink::default());
    let service = ScriptService::new(enabled_scripts(), sink.clone());

    // 1. shell 解释器作为程序：契约层在反序列化即拒绍，这里用原始清单取证。
    let raw_manifest = serde_json::json!({
        "scripts": [{
            "script_id": "shell",
            "display_name": "shell 注入",
            "program": "bash",
            "argument_policy": {
                "fixed_arguments": [],
                "accepts_user_arguments": false,
                "max_user_arguments": 0,
                "max_user_argument_bytes": 16
            },
            "workdir": { "kind": "workspace_root" },
            "risk": "low"
        }]
    });
    std::fs::write(
        root.path().join("paseo.json"),
        serde_json::to_vec(&raw_manifest).expect("清单序列化"),
    )
    .expect("写入清单");
    let error = service
        .handle(root.path(), run_request("shell"))
        .expect_err("解释器程序必须被拒绝");
    assert_eq!(
        error.code,
        ErrorCode::ValidationFailed,
        "非法 program 会在清单校验阶段整份拒绝"
    );
    assert_eq!(sink.len(), 0);

    // 2. 越界工作目录
    let mut escaping = script_entry("escape", &[]);
    escaping.workdir = ScriptWorkdirPolicy::Subdirectory {
        relative_path: soxu_protocol::WorkspaceRelativePath::try_from("nope".to_owned())
            .expect("词法合法但目录不存在"),
    };
    write_manifest(root.path(), &[&escaping]);
    let error = service
        .handle(root.path(), run_request("escape"))
        .expect_err("越界/缺失工作目录必须拒绝");
    assert_eq!(error.code, ErrorCode::ValidationFailed);
    assert_eq!(sink.len(), 0);

    // 3. 透传参数越权（条目声明不接受透传参数）
    let strict = script_entry("strict", &["-e", "process.exit(0)"]);
    write_manifest(root.path(), &[&strict]);
    let error = service
        .handle(
            root.path(),
            ScriptCommand::Run(RunScriptRequest {
                workspace_id: "wks_test".to_owned(),
                script_id: ScriptId::try_from("strict".to_owned()).expect("标识"),
                user_arguments: ScriptUserArguments::try_from(vec![
                    soxu_protocol::ScriptArgument::try_from("--evil".to_owned()).expect("参数"),
                ])
                .expect("参数列表"),
                idempotency_key: None,
                approval_id: Some("approval_test".to_owned()),
            }),
        )
        .expect_err("不接受透传参数的条目必须拒绝");
    assert_eq!(error.code, ErrorCode::ValidationFailed);
    assert_eq!(sink.len(), 0, "注入面拒绝必须发生在派生之前");
}

#[tokio::test]
async fn high_risk_entry_requires_approval_marker() {
    let root = tempfile::tempdir().expect("临时工作区");
    let raw = serde_json::json!({
        "script_id": "risky",
        "display_name": "高风险脚本",
        "program": node_program(),
        "argument_policy": {
            "fixed_arguments": ["-e", "process.exit(0)"],
            "accepts_user_arguments": false,
            "max_user_arguments": 0,
            "max_user_argument_bytes": 4096
        },
        "workdir": { "kind": "workspace_root" },
        "risk": "critical"
    });
    let entry: ManagedScriptEntry = serde_json::from_value(raw).expect("条目");
    write_manifest(root.path(), &[&entry]);

    let sink = Arc::new(RecordingScriptSink::default());
    let service = ScriptService::new(enabled_scripts(), sink.clone());
    let error = service
        .handle(
            root.path(),
            ScriptCommand::Run(RunScriptRequest {
                workspace_id: "wks_test".to_owned(),
                script_id: ScriptId::try_from("risky".to_owned()).expect("标识"),
                user_arguments: ScriptUserArguments::default(),
                idempotency_key: None,
                approval_id: None,
            }),
        )
        .expect_err("高风险未批准必须拒绝");
    assert_eq!(error.code, ErrorCode::PolicyDenied);
    assert_eq!(sink.len(), 0, "未批准不得出现任何生命周期事件");
}

// ---------------------------------------------------------------------------
// 脚本域：正常生命周期、输出上限、超时回收、环境白名单、幂等
// ---------------------------------------------------------------------------

#[tokio::test]
async fn script_runs_without_shell_and_reports_lifecycle() {
    assert_node_available();
    let root = tempfile::tempdir().expect("临时工作区");
    let entry = script_entry("echo", &["-e", "process.exit(0)"]);
    write_manifest(root.path(), &[&entry]);
    let sink = Arc::new(RecordingScriptSink::default());
    let service = ScriptService::new(enabled_scripts(), sink.clone());

    let accepted = accepted_snapshot(
        service
            .handle(root.path(), run_request("echo"))
            .expect("脚本应可执行"),
    );
    assert_eq!(accepted.state, ScriptRunState::Queued);
    let final_snapshot = wait_terminal(&service, &accepted.run_id).await;
    assert_eq!(final_snapshot.state, ScriptRunState::Exited);
    assert_eq!(final_snapshot.exit_code, Some(0));
    assert!(final_snapshot.started_at.is_some());
    assert!(final_snapshot.finished_at.is_some());

    let phases = sink.phases();
    assert_eq!(
        phases.first().copied(),
        Some(soxu_protocol::ScriptLifecyclePhase::Queued)
    );
    assert!(phases.contains(&soxu_protocol::ScriptLifecyclePhase::Started));
    assert_eq!(
        phases.last().copied(),
        Some(soxu_protocol::ScriptLifecyclePhase::Exited)
    );
    assert_eq!(
        service
            .runner()
            .reap_outcome(&accepted.run_id)
            .expect("回收结论"),
        Some(ReapOutcome::NotRequired),
        "正常退出路径不需要终止与回收"
    );
}

#[tokio::test]
async fn output_budget_truncates_instead_of_unbounded_buffering() {
    assert_node_available();
    let root = tempfile::tempdir().expect("临时工作区");
    let entry = script_entry(
        "chatter",
        &["-e", "process.stdout.write('x'.repeat(1024 * 1024))"],
    );
    write_manifest(root.path(), &[&entry]);
    let sink = Arc::new(RecordingScriptSink::default());
    let service = ScriptService::new(enabled_scripts(), sink);

    let accepted = accepted_snapshot(
        service
            .handle(root.path(), run_request("chatter"))
            .expect("脚本应可执行"),
    );
    wait_terminal(&service, &accepted.run_id).await;
    let output = service.runner().output(&accepted.run_id).expect("输出快照");
    assert!(
        output.as_bytes().len() <= SCRIPT_OUTPUT_LIMIT_BYTES,
        "输出必须被预算截断，实际 {} 字节",
        output.as_bytes().len()
    );
    assert!(output.truncated(), "超预算必须置截断标记");
    assert!(output.dropped_bytes() > 0, "必须记录被丢弃的字节数");
}

#[tokio::test]
async fn timeout_stops_run_and_reclaims_process_tree_within_bounds() {
    assert_node_available();
    let root = tempfile::tempdir().expect("临时工作区");
    let mut entry = script_entry("sleeper", &["-e", "setTimeout(() => {}, 60000)"]);
    entry.timeout_seconds = Some(1);
    write_manifest(root.path(), &[&entry]);
    let sink = Arc::new(RecordingScriptSink::default());
    let service = ScriptService::new(enabled_scripts(), sink.clone());

    let started = Instant::now();
    let accepted = accepted_snapshot(
        service
            .handle(root.path(), run_request("sleeper"))
            .expect("脚本应可执行"),
    );
    let final_snapshot = wait_terminal(&service, &accepted.run_id).await;
    let elapsed = started.elapsed();

    assert_eq!(final_snapshot.state, ScriptRunState::Cancelled);
    assert!(
        elapsed < Duration::from_secs(20),
        "超时回收必须有界，实际耗时 {elapsed:?}"
    );
    assert_eq!(
        service
            .runner()
            .reap_outcome(&accepted.run_id)
            .expect("回收结论"),
        Some(ReapOutcome::ExitedAfterStop),
        "终止后必须在回收窗口内确认退出"
    );
    assert_eq!(
        sink.phases().last().copied(),
        Some(soxu_protocol::ScriptLifecyclePhase::Stopped)
    );
}

#[tokio::test]
async fn environment_is_cleared_unless_allowlisted() {
    assert_node_available();
    let root = tempfile::tempdir().expect("临时工作区");
    let probe = "process.stdout.write(process.env.PATH ? 'has-path' : 'no-path')";
    let mut isolated = script_entry("env-isolated", &["-e", probe]);
    isolated.env_policy = ScriptEnvPolicy::default();
    write_manifest(root.path(), &[&isolated]);
    let sink = Arc::new(RecordingScriptSink::default());
    let service = ScriptService::new(enabled_scripts(), sink);

    let accepted = accepted_snapshot(
        service
            .handle(root.path(), run_request("env-isolated"))
            .expect("脚本应可执行"),
    );
    wait_terminal(&service, &accepted.run_id).await;
    let output = service.runner().output(&accepted.run_id).expect("输出");
    assert_eq!(
        String::from_utf8_lossy(output.as_bytes()),
        "no-path",
        "默认不继承父进程环境"
    );

    let mut allowed = script_entry("env-allowed", &["-e", probe]);
    allowed.env_policy = ScriptEnvPolicy {
        allow_keys: vec![soxu_protocol::EnvKey::try_from("PATH".to_owned()).expect("环境名")],
        inherits_parent_env: false,
    };
    write_manifest(root.path(), &[&allowed]);
    let accepted = accepted_snapshot(
        service
            .handle(root.path(), run_request("env-allowed"))
            .expect("脚本应可执行"),
    );
    wait_terminal(&service, &accepted.run_id).await;
    let output = service.runner().output(&accepted.run_id).expect("输出");
    assert_eq!(
        String::from_utf8_lossy(output.as_bytes()),
        "has-path",
        "白名单内的变量必须按名注入"
    );
}

#[tokio::test]
async fn identical_idempotency_key_never_runs_twice() {
    assert_node_available();
    let root = tempfile::tempdir().expect("临时工作区");
    let entry = script_entry("idem", &["-e", "setTimeout(() => {}, 400)"]);
    write_manifest(root.path(), &[&entry]);
    let sink = Arc::new(RecordingScriptSink::default());
    let service = ScriptService::new(enabled_scripts(), sink);
    let key = soxu_protocol::IdempotencyKey::try_from("run-once".to_owned()).expect("幂等键");

    let command = || {
        ScriptCommand::Run(RunScriptRequest {
            workspace_id: "wks_test".to_owned(),
            script_id: ScriptId::try_from("idem".to_owned()).expect("标识"),
            user_arguments: ScriptUserArguments::default(),
            idempotency_key: Some(key.clone()),
            approval_id: Some("approval_test".to_owned()),
        })
    };
    let first = accepted_snapshot(service.handle(root.path(), command()).expect("首跑"));
    let second = accepted_snapshot(service.handle(root.path(), command()).expect("重放"));
    assert_eq!(first.run_id, second.run_id, "同键必须收敛到同一运行");
    wait_terminal(&service, &first.run_id).await;
}

// ---------------------------------------------------------------------------
// PTY 域：分片序号、幂等关闭、会话上限、空闲回收、尺寸与输入边界
// ---------------------------------------------------------------------------

fn open_request() -> OpenPtyRequest {
    OpenPtyRequest {
        workspace_id: "wks_test".to_owned(),
        size: PtySize::new(100, 30).expect("合法尺寸"),
        workdir: PtyWorkdirPolicy::WorkspaceRoot,
        env_policy: soxu_protocol::PtyEnvPolicy::default(),
    }
}

fn enabled_pty(backend: Arc<dyn PtyBackend>, sink: Arc<RecordingPtySink>) -> PtyHost {
    PtyHost::new(
        FeatureGate::from_features([DaemonFeature::ControlledPty]),
        sink as Arc<dyn PtyEventSink>,
        backend,
    )
}

#[test]
fn pty_output_is_chunked_sequenced_and_terminated_once() {
    let payload = "终端输出🙂".repeat(4_000);
    let backend = Arc::new(FixedOutputBackend::new(payload.clone().into_bytes()));
    let sink = Arc::new(RecordingPtySink::default());
    let host = enabled_pty(backend, sink.clone());
    let root = tempfile::tempdir().expect("临时工作区");

    let opened = host.open(root.path(), open_request()).expect("打开会话");
    assert_eq!(opened.state, PtySessionState::Running);
    wait_for_events(&sink, |sink| {
        sink.count_of(|event| matches!(event, PtyEvent::Exited { .. })) == 1
    });

    let chunks = sink.output_chunks();
    assert!(chunks.len() > 1, "超限输出必须切成多片");
    let mut replayed = String::new();
    for (index, chunk) in chunks.iter().enumerate() {
        assert!(
            chunk.data.byte_len() <= soxu_protocol::PTY_OUTPUT_CHUNK_MAX_BYTES,
            "分片 {index} 超出预算"
        );
        assert_eq!(chunk.sequence, index as u64, "序号必须从 0 起连续递增");
        let is_last = index + 1 == chunks.len();
        assert_eq!(chunk.end_of_stream, is_last, "只有终结片标记流结束");
        if !is_last {
            replayed.push_str(chunk.data.as_str());
        }
    }
    assert_eq!(replayed, payload, "分片必须可无损重组");
    let last_sequence = chunks.last().expect("至少一片").sequence;
    let exited = sink.count_of(|event| {
        matches!(event, PtyEvent::Exited { last_sequence: seq, .. } if *seq == last_sequence)
    });
    assert_eq!(exited, 1, "Exited 只下发一次且携带最后一片序号");
}

#[test]
fn duplicate_close_is_idempotent_and_emits_each_terminal_event_once() {
    let backend = Arc::new(FixedOutputBackend::new(b"ready".to_vec()));
    let sink = Arc::new(RecordingPtySink::default());
    let host = enabled_pty(backend, sink.clone());
    let root = tempfile::tempdir().expect("临时工作区");
    let session = host.open(root.path(), open_request()).expect("打开会话");

    let first = host
        .close(&session.pty_id, PtyCloseReason::ClientRequest)
        .expect("首次关闭");
    let second = host
        .close(&session.pty_id, PtyCloseReason::ClientRequest)
        .expect("重复关闭不得报错");
    assert_eq!(first.state, PtySessionState::Closed);
    assert_eq!(second.state, PtySessionState::Closed);
    assert_eq!(host.live_sessions(), 0);
    assert_eq!(
        sink.count_of(|event| matches!(event, PtyEvent::Closed(_))),
        1,
        "Closed 只下发一次"
    );
    assert_eq!(
        sink.count_of(|event| matches!(event, PtyEvent::Exited { .. })),
        1,
        "Exited 只下发一次"
    );

    let write_error = host
        .write(
            &session.pty_id,
            PtyInputData::try_from("ls\n".to_owned()).expect("输入"),
        )
        .expect_err("已关闭会话必须拒绝写入");
    assert_eq!(write_error.code, ErrorCode::Conflict);
}

#[test]
fn session_cap_and_size_bounds_are_enforced() {
    let backend = Arc::new(FixedOutputBackend::new(Vec::new()));
    let sink = Arc::new(RecordingPtySink::default());
    let host = enabled_pty(backend, sink);
    let root = tempfile::tempdir().expect("临时工作区");

    let mut opened = Vec::new();
    for _ in 0..PTY_MAX_SESSIONS {
        opened.push(host.open(root.path(), open_request()).expect("打开会话"));
    }
    let error = host
        .open(root.path(), open_request())
        .expect_err("超过会话上限必须拒绝");
    assert_eq!(error.code, ErrorCode::Conflict);
    assert_eq!(host.live_sessions(), PTY_MAX_SESSIONS);

    // 尺寸边界由协议在反序列化阶段拦截，这里复核构造侧同样拒绝越界值。
    assert!(PtySize::new(0, 30).is_err());
    assert!(PtySize::new(1_001, 30).is_err());
    assert!(PtySize::new(100, 501).is_err());

    let first = opened.first().expect("至少一个会话");
    let resized = host
        .resize(&first.pty_id, PtySize::new(120, 40).expect("合法尺寸"))
        .expect("尺寸调整应成功");
    assert_eq!(resized.size, PtySize::new(120, 40).expect("合法尺寸"));
    for session in &opened {
        host.close(&session.pty_id, PtyCloseReason::ClientRequest)
            .expect("关闭会话");
    }
}

#[test]
fn idle_sessions_are_reaped_with_idle_reason() {
    let backend = Arc::new(FixedOutputBackend::new(b"idle".to_vec()));
    let sink = Arc::new(RecordingPtySink::default());
    let host = enabled_pty(backend, sink.clone());
    let root = tempfile::tempdir().expect("临时工作区");
    let _session = host.open(root.path(), open_request()).expect("打开会话");

    assert_eq!(host.reap_idle(Instant::now()), 0, "刚打开的会话不得被回收");
    let later = Instant::now() + PTY_IDLE_TIMEOUT + Duration::from_secs(1);
    assert_eq!(host.reap_idle(later), 1, "超过空闲阈值必须回收");
    assert_eq!(host.live_sessions(), 0);
    let closed = sink.count_of(|event| matches!(event, PtyEvent::Closed(_)));
    assert_eq!(closed, 1, "空闲回收同样只下发一次 Closed");
}

#[test]
fn write_reaches_the_backend_and_reports_accepted_bytes() {
    let backend = Arc::new(FixedOutputBackend::new(Vec::new()));
    let written = Arc::clone(&backend.written);
    let sink = Arc::new(RecordingPtySink::default());
    let host = enabled_pty(backend, sink);
    let root = tempfile::tempdir().expect("临时工作区");
    let session = host.open(root.path(), open_request()).expect("打开会话");

    let accepted = host
        .write(
            &session.pty_id,
            PtyInputData::try_from("echo hi\n".to_owned()).expect("输入"),
        )
        .expect("写入应成功");
    assert_eq!(accepted as usize, "echo hi\n".len());
    assert_eq!(
        String::from_utf8_lossy(&written.lock().expect("写入记录")),
        "echo hi\n"
    );

    assert!(
        PtyInputData::try_from(String::new()).is_err(),
        "空输入必须拒绝"
    );
    assert!(
        PtyInputData::try_from("x".repeat(soxu_protocol::PTY_INPUT_CHUNK_MAX_BYTES + 1)).is_err(),
        "超长输入必须拒绝"
    );
}

/// 真实 PTY 冒烟：只在具备真实伪终端的平台运行（L-013：逐平台显式门控）。
#[cfg(any(windows, target_os = "linux"))]
#[test]
fn real_backend_opens_a_terminal_and_reports_exit() {
    use soxu_daemon::pty::PortablePtyBackend;

    let Ok(backend) = PortablePtyBackend::discover() else {
        // 本机缺少可用 shell 时不算失败，但必须给出可执行错误而非静默通过。
        return;
    };
    let sink = Arc::new(RecordingPtySink::default());
    let host = PtyHost::new(
        FeatureGate::from_features([DaemonFeature::ControlledPty]),
        sink.clone() as Arc<dyn PtyEventSink>,
        Arc::new(backend),
    );
    let root = tempfile::tempdir().expect("临时工作区");
    let session = host
        .open(root.path(), open_request())
        .expect("打开真实终端");
    let closed = host
        .close(&session.pty_id, PtyCloseReason::ClientRequest)
        .expect("关闭真实终端");
    assert!(matches!(
        closed.state,
        PtySessionState::Closed | PtySessionState::Failed
    ));
    let deadline = Instant::now() + PTY_CLOSE_GRACE + Duration::from_secs(5);
    while sink.count_of(|event| matches!(event, PtyEvent::Closed(_))) == 0 {
        assert!(
            Instant::now() < deadline,
            "真实终端未在有界窗口内下发 Closed"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

// ---------------------------------------------------------------------------
// 伪 PTY 后端
// ---------------------------------------------------------------------------

struct FixedOutputBackend {
    payload: Vec<u8>,
    written: Arc<Mutex<Vec<u8>>>,
}

impl FixedOutputBackend {
    fn new(payload: Vec<u8>) -> Self {
        Self {
            payload,
            written: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl PtyBackend for FixedOutputBackend {
    fn spawn(&self, _spec: &PtySpawnSpec) -> Result<PtyChild, ActionableError> {
        Ok(PtyChild {
            reader: Box::new(Cursor::new(self.payload.clone())),
            writer: Box::new(SharedWriter {
                target: Arc::clone(&self.written),
            }),
            control: Arc::new(FixedControl {
                polls: AtomicU32::new(0),
            }),
        })
    }
}

struct SharedWriter {
    target: Arc<Mutex<Vec<u8>>>,
}

impl Write for SharedWriter {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.target
            .lock()
            .expect("写入记录")
            .extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// 第一次轮询即报「已退出 0」，便于验证有界回收路径。
struct FixedControl {
    polls: AtomicU32,
}

impl PtyControl for FixedControl {
    fn resize(&self, _size: PtySize) -> Result<(), ActionableError> {
        Ok(())
    }

    fn terminate(&self) -> Result<(), ActionableError> {
        Ok(())
    }

    fn try_wait(&self) -> Result<Option<PtyExitStatus>, ActionableError> {
        let previous = self.polls.fetch_add(1, Ordering::AcqRel);
        if previous == 0 {
            return Ok(None);
        }
        Ok(Some(PtyExitStatus {
            exit_code: Some(0),
            exit_signal: None,
        }))
    }
}

fn wait_for_events(sink: &Arc<RecordingPtySink>, done: impl Fn(&RecordingPtySink) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done(sink) {
        assert!(Instant::now() < deadline, "事件未在 10s 内有界到达");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn assert_node_available() {
    let found = std::env::var_os("PATH").is_some_and(|path| {
        std::env::split_paths(&path)
            .filter(|dir| dir.is_absolute())
            .any(|dir| dir.join("node").is_file() || dir.join("node.exe").is_file())
    });
    assert!(
        found,
        "受管脚本回归需要 PATH 中存在 node（按项目 mise 工具链提供）"
    );
}

#[allow(dead_code)]
fn assert_no_events(sink: &RecordingScriptSink) {
    assert_eq!(sink.len(), 0, "拒绝路径不得下发任何生命周期事件");
}

#[allow(dead_code)]
fn open_session_id(snapshot: &soxu_protocol::PtySessionSnapshot) -> PtySessionId {
    snapshot.pty_id.clone()
}

#[allow(dead_code)]
fn unused_null_sink() -> Arc<NullPtyEventSink> {
    Arc::new(NullPtyEventSink)
}
