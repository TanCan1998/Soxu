//! M6-A 专项回归：受管脚本执行器与受控 PTY 宿主。
//!
//! 覆盖 Issue #139 要求的四类证据：注入面、超时与进程树回收、输出上限、
//! PTY 分片序号与关闭幂等、能力位缺省关闭拒绝。
//! 平台语义按 L-013 逐平台显式门控；子进程等待一律「有界 + 非管道收集」
//! （L-016），失败不靠无界 `output()` 收集。

use soxu_daemon::pty::PtyHost;
use soxu_daemon::pty::{
    PTY_IDLE_TIMEOUT, PTY_MAX_SESSIONS, PtyBackend, PtyChild, PtyControl, PtyEventSink,
    PtySpawnSpec,
};
use soxu_daemon::scripts::{
    FeatureGate, ReapOutcome, RunScriptOptions, SCRIPT_MAX_CONCURRENT_RUNS,
    SCRIPT_OUTPUT_LIMIT_BYTES, ScriptEventSink, ScriptService,
};
use soxu_protocol::{
    ActionableError, DaemonFeature, ErrorCode, ManagedScriptEntry, OpenPtyRequest, PtyCloseReason,
    PtyCommand, PtyEvent, PtyExitStatus, PtyInputData, PtySessionState, PtySize, PtyWorkdirPolicy,
    RunScriptRequest, ScriptCommand, ScriptCommandResult, ScriptEnvPolicy, ScriptId,
    ScriptRunState, ScriptUserArguments, ScriptWorkdirPolicy,
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

/// 构造一条受管脚本条目：程序固定为**受控子进程桩**（当前测试二进制自执行），
/// 固定参数负责选中具体桩测试。
///
/// 只依赖 Rust 工具链产物，不依赖宿主预装的 node/python 或任何第三方 CLI：
/// CI Runner 上的 Node 会因运行时自检（`Assertion failed: ncrypto::CSPRNG`）直接崩溃，
/// 属 L-017 同类「测试依赖开发机预装工具」陷阱。
fn script_entry(script_id: &str, helper: &str) -> ManagedScriptEntry {
    let raw = serde_json::json!({
        "script_id": script_id,
        "display_name": "回归脚本",
        "program": stub_program(),
        "argument_policy": {
            "fixed_arguments": stub_arguments(helper),
            "accepts_user_arguments": false,
            "max_user_arguments": 0,
            "max_user_argument_bytes": 4096
        },
        "workdir": { "kind": "workspace_root" },
        "risk": "low"
    });
    serde_json::from_value(raw).expect("脚本条目应合法")
}

/// 选中桩测试的固定参数：libtest 只运行那一个被忽略的桩用例。
fn stub_arguments(helper: &str) -> Vec<String> {
    vec![
        "--exact".to_owned(),
        helper.to_owned(),
        "--ignored".to_owned(),
        "--nocapture".to_owned(),
    ]
}

/// 当前测试二进制路径（作为受控子进程桩）。
///
/// 协议要求 `program` 不得含空白与 shell 元字符；若测试二进制路径本身不满足
/// （例如工作树或用户名含空格），就复制一份到无空白的临时目录再执行。
fn stub_program() -> String {
    let current = std::env::current_exe().expect("当前测试二进制路径");
    if program_is_acceptable(&current) {
        return current.to_string_lossy().into_owned();
    }
    let fallback_dir = std::env::temp_dir().join("soxu-m6a-stub");
    std::fs::create_dir_all(&fallback_dir).expect("创建桩目录");
    let target = fallback_dir.join(if cfg!(windows) {
        "soxu-m6a-stub.exe"
    } else {
        "soxu-m6a-stub"
    });
    std::fs::copy(&current, &target).expect("复制桩二进制");
    assert!(
        program_is_acceptable(&target),
        "受控桩路径仍不满足协议对 program 的约束：{}",
        target.display()
    );
    target.to_string_lossy().into_owned()
}

fn program_is_acceptable(path: &Path) -> bool {
    path.is_file()
        && soxu_protocol::ScriptProgram::try_from(path.to_string_lossy().into_owned()).is_ok()
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
            Some(root.path()),
            ScriptCommand::List(soxu_protocol::ListScriptsRequest {
                workspace_id: "wks_test".to_owned(),
            }),
        )
        .expect_err("能力位缺省关闭必须拒绝");
    assert_eq!(error.code, ErrorCode::Unsupported);
    assert!(error.summary_zh.contains("managed_scripts"));
    assert_eq!(sink.len(), 0, "拒绝路径不得下发任何事件");

    let error = service
        .handle(Some(root.path()), run_request("check"))
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
        .handle(Some(root.path()), PtyCommand::Open(open_request()))
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
        .handle(Some(root.path()), run_request("shell"))
        .expect_err("解释器程序必须被拒绝");
    assert_eq!(
        error.code,
        ErrorCode::ValidationFailed,
        "非法 program 会在清单校验阶段整份拒绝"
    );
    assert_eq!(sink.len(), 0);

    // 2. 越界工作目录
    let mut escaping = script_entry("escape", "stub_child_exits_cleanly");
    escaping.workdir = ScriptWorkdirPolicy::Subdirectory {
        relative_path: soxu_protocol::WorkspaceRelativePath::try_from("nope".to_owned())
            .expect("词法合法但目录不存在"),
    };
    write_manifest(root.path(), &[&escaping]);
    let error = service
        .handle(Some(root.path()), run_request("escape"))
        .expect_err("越界/缺失工作目录必须拒绝");
    assert_eq!(error.code, ErrorCode::ValidationFailed);
    assert_eq!(sink.len(), 0);

    // 3. 透传参数越权（条目声明不接受透传参数）
    let strict = script_entry("strict", "stub_child_exits_cleanly");
    write_manifest(root.path(), &[&strict]);
    let error = service
        .handle(
            Some(root.path()),
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
        "program": stub_program(),
        "argument_policy": {
            "fixed_arguments": stub_arguments("stub_child_exits_cleanly"),
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
            Some(root.path()),
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
    let root = tempfile::tempdir().expect("临时工作区");
    let entry = script_entry("echo", "stub_child_exits_cleanly");
    write_manifest(root.path(), &[&entry]);
    let sink = Arc::new(RecordingScriptSink::default());
    let service = ScriptService::new(enabled_scripts(), sink.clone());

    let accepted = accepted_snapshot(
        service
            .handle(Some(root.path()), run_request("echo"))
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
    let root = tempfile::tempdir().expect("临时工作区");
    let entry = script_entry("chatter", "stub_child_floods_stdout");
    write_manifest(root.path(), &[&entry]);
    let sink = Arc::new(RecordingScriptSink::default());
    let service = ScriptService::new(enabled_scripts(), sink);

    let accepted = accepted_snapshot(
        service
            .handle(Some(root.path()), run_request("chatter"))
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
    let root = tempfile::tempdir().expect("临时工作区");
    let mut entry = script_entry("sleeper", "stub_child_idles");
    entry.timeout_seconds = Some(1);
    write_manifest(root.path(), &[&entry]);
    let sink = Arc::new(RecordingScriptSink::default());
    let service = ScriptService::new(enabled_scripts(), sink.clone());

    let started = Instant::now();
    let accepted = accepted_snapshot(
        service
            .handle(Some(root.path()), run_request("sleeper"))
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
    let root = tempfile::tempdir().expect("临时工作区");
    let parent_has_path = std::env::var_os("PATH").is_some();
    let mut isolated = script_entry("env-isolated", "stub_child_reports_environment");
    isolated.env_policy = ScriptEnvPolicy::default();
    write_manifest(root.path(), &[&isolated]);
    let sink = Arc::new(RecordingScriptSink::default());
    let service = ScriptService::new(enabled_scripts(), sink);

    let accepted = accepted_snapshot(
        service
            .handle(Some(root.path()), run_request("env-isolated"))
            .expect("脚本应可执行"),
    );
    wait_terminal(&service, &accepted.run_id).await;
    let output = service.runner().output(&accepted.run_id).expect("输出");
    let cleared = String::from_utf8_lossy(output.as_bytes()).into_owned();
    let cleared_keys = stub_env_keys(&cleared);
    assert!(
        !stub_env_path(&cleared),
        "未放行的 PATH 不得泄漏给子进程，桩实际看到：{cleared}"
    );
    // 绝对数量不做断言：平台运行时可能在进程初始化时自行注入自有变量
    // （例如 macOS 的 `__CF_USER_TEXT_ENCODING`），那不属于父环境继承。
    // 真正的安全断言是「父进程的可识别变量一个都不出现」。
    for sensitive in ["HOME", "USER", "SHELL", "TMPDIR", "CARGO_HOME"] {
        assert!(
            !cleared_keys.iter().any(|key| key == sensitive),
            "默认清空环境后不得继承 {sensitive}：{cleared_keys:?}"
        );
    }

    let mut allowed = script_entry("env-allowed", "stub_child_reports_environment");
    allowed.env_policy = ScriptEnvPolicy {
        allow_keys: vec![soxu_protocol::EnvKey::try_from("PATH".to_owned()).expect("环境名")],
        inherits_parent_env: false,
    };
    write_manifest(root.path(), &[&allowed]);
    let accepted = accepted_snapshot(
        service
            .handle(Some(root.path()), run_request("env-allowed"))
            .expect("脚本应可执行"),
    );
    wait_terminal(&service, &accepted.run_id).await;
    let output = service.runner().output(&accepted.run_id).expect("输出");
    let allowed = String::from_utf8_lossy(output.as_bytes()).into_owned();
    let allowed_keys = stub_env_keys(&allowed);
    assert_eq!(
        stub_env_path(&allowed),
        parent_has_path,
        "白名单内的变量必须按名注入，桩实际看到：{allowed}"
    );
    assert!(
        !allowed_keys.iter().any(|key| key == "HOME"),
        "白名单之外的变量仍不得注入：{allowed_keys:?}"
    );
    // 与清空环境的那次相比，只能多出被放行的那一个变量；平台自有注入两边都计，故差值稳定。
    let expected_delta = usize::from(parent_has_path);
    assert_eq!(
        stub_env_count(&allowed),
        stub_env_count(&cleared) + expected_delta,
        "白名单只应多注入被放行的变量：cleared={cleared_keys:?} allowed={allowed_keys:?}"
    );
}

fn stub_env_count(text: &str) -> usize {
    stub_env_value(text, "SOXU-STUB-ENV-COUNT=")
        .and_then(|raw| raw.parse::<usize>().ok())
        .unwrap_or_else(|| panic!("桩未报告环境变量数量：{text}"))
}

fn stub_env_keys(text: &str) -> Vec<String> {
    stub_env_value(text, "SOXU-STUB-ENV-KEYS=")
        .map(|raw| {
            raw.split(',')
                .map(|key| key.trim().to_ascii_uppercase())
                .filter(|key| !key.is_empty())
                .collect()
        })
        .unwrap_or_else(|| panic!("桩未报告环境变量键名：{text}"))
}

fn stub_env_path(text: &str) -> bool {
    match stub_env_value(text, "SOXU-STUB-PATH=") {
        Some("present") => true,
        Some("absent") => false,
        other => panic!("桩未报告 PATH 存在性（{other:?}）：{text}"),
    }
}

fn stub_env_value<'a>(text: &'a str, marker: &str) -> Option<&'a str> {
    text.lines()
        .find_map(|line| line.trim().strip_prefix(marker))
}

#[tokio::test]
async fn identical_idempotency_key_never_runs_twice() {
    let root = tempfile::tempdir().expect("临时工作区");
    let entry = script_entry("idem", "stub_child_sleeps_briefly");
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
    let first = accepted_snapshot(service.handle(Some(root.path()), command()).expect("首跑"));
    let second = accepted_snapshot(service.handle(Some(root.path()), command()).expect("重放"));
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
    // 用「会话保持运行」的后端：派生后立即 EOF 的会话会立即归还容量，
    // 那样就测不到并发上限，只能测到一个时序凑巧。
    let backend = Arc::new(PipeHoldBackend::default());
    let sink = Arc::new(RecordingPtySink::default());
    let host = enabled_pty(backend.clone(), sink);
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
    assert_eq!(
        backend.spawned(),
        PTY_MAX_SESSIONS,
        "被拒绝的请求不得派生 shell 进程"
    );

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
    let deadline = Instant::now() + soxu_daemon::pty::PTY_CLOSE_GRACE + Duration::from_secs(5);
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

/// 保持会话运行的后端：读端是真实管道，写端由会话持有；关闭会话释放写端后读到 EOF。
#[derive(Default)]
struct PipeHoldBackend {
    spawned: AtomicU32,
}

impl PipeHoldBackend {
    fn spawned(&self) -> usize {
        usize::try_from(self.spawned.load(Ordering::Acquire)).unwrap_or(0)
    }
}

impl PtyBackend for PipeHoldBackend {
    fn spawn(&self, _spec: &PtySpawnSpec) -> Result<PtyChild, ActionableError> {
        self.spawned.fetch_add(1, Ordering::AcqRel);
        let (reader, writer) = std::io::pipe().map_err(|err| {
            ActionableError::new(
                ErrorCode::InternalError,
                "无法创建测试管道",
                "该请求未打开会话",
                "请重试",
            )
            .with_diagnostic(err.to_string())
        })?;
        Ok(PtyChild {
            reader: Box::new(reader),
            writer: Box::new(writer),
            control: Arc::new(AlwaysExitedControl),
        })
    }
}

/// 立即报告「已退出 0」的控制面：让关闭路径无需等待退出窗口。
struct AlwaysExitedControl;

impl PtyControl for AlwaysExitedControl {
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

fn wait_for_events(sink: &Arc<RecordingPtySink>, done: impl Fn(&RecordingPtySink) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done(sink) {
        assert!(Instant::now() < deadline, "事件未在 10s 内有界到达");
        std::thread::sleep(Duration::from_millis(10));
    }
}

// ---------------------------------------------------------------------------
// 脚本域：并发上限与关停收敛（资源治理阶段 2）
// ---------------------------------------------------------------------------

/// 并发 `run` ×16 不得越过上限，且被拒绝的请求不派生子进程。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_runs_never_exceed_capacity() {
    let root = tempfile::tempdir().expect("临时工作区");
    let entry = script_entry("idler", "stub_child_idles");
    write_manifest(root.path(), &[&entry]);
    let sink = Arc::new(RecordingScriptSink::default());
    let service = Arc::new(ScriptService::new(enabled_scripts(), sink));
    let barrier = Arc::new(std::sync::Barrier::new(16));

    let mut tasks = Vec::new();
    for _ in 0..16 {
        let service = Arc::clone(&service);
        let entry = entry.clone();
        let root = root.path().to_path_buf();
        let barrier = Arc::clone(&barrier);
        tasks.push(tokio::task::spawn_blocking(move || {
            barrier.wait();
            service.runner().run(
                "wks_test",
                &root,
                &entry,
                RunScriptOptions {
                    user_arguments: ScriptUserArguments::default(),
                    idempotency_key: None,
                    approval_id: Some("approval_test".to_owned()),
                },
            )
        }));
    }

    let mut accepted = Vec::new();
    let mut rejected = 0;
    for task in tasks {
        match task.await.expect("并发任务不得 panic") {
            Ok(snapshot) => accepted.push(snapshot.run_id),
            Err(error) => {
                assert_eq!(
                    error.code,
                    ErrorCode::Conflict,
                    "超限必须返回可诊断上限错误"
                );
                rejected += 1;
            }
        }
    }
    assert_eq!(accepted.len(), SCRIPT_MAX_CONCURRENT_RUNS);
    assert_eq!(rejected, 16 - SCRIPT_MAX_CONCURRENT_RUNS);

    // 长驻运行必须能被关停有界收敛，否则测试进程退出时会遗留子进程。
    let report = service.runner().shutdown(Duration::from_secs(10));
    assert_eq!(report.requested, SCRIPT_MAX_CONCURRENT_RUNS);
    assert!(
        report.unconfirmed.is_empty(),
        "受控子进程桩必须在窗口内收敛：{:?}",
        report.unconfirmed
    );
    for run_id in &accepted {
        let snapshot = service.runner().inspect(run_id).expect("查询运行");
        assert_eq!(snapshot.state, ScriptRunState::Cancelled);
    }
}

/// daemon 关停时，运行中的脚本必须被取消并在有界窗口内确认回收。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shutdown_cancels_running_scripts_and_reports_convergence() {
    let root = tempfile::tempdir().expect("临时工作区");
    let entry = script_entry("idler", "stub_child_idles");
    write_manifest(root.path(), &[&entry]);
    let sink = Arc::new(RecordingScriptSink::default());
    let service = Arc::new(ScriptService::new(enabled_scripts(), sink.clone()));

    let accepted = accepted_snapshot(
        service
            .handle(Some(root.path()), run_request("idler"))
            .expect("脚本应可执行"),
    );
    wait_state(&service, &accepted.run_id, ScriptRunState::Running).await;

    let report = service.runner().shutdown(Duration::from_secs(10));
    assert_eq!(report.requested, 1);
    assert_eq!(report.confirmed, 1);
    assert!(report.unconfirmed.is_empty(), "脚本必须被有界取消");

    let snapshot = service
        .runner()
        .inspect(&accepted.run_id)
        .expect("查询运行");
    assert_eq!(snapshot.state, ScriptRunState::Cancelled);
    assert_eq!(
        service
            .runner()
            .reap_outcome(&accepted.run_id)
            .expect("回收结论"),
        Some(ReapOutcome::ExitedAfterStop),
        "关停后必须确认进程树已终止"
    );
    assert_eq!(
        sink.phases().last().copied(),
        Some(soxu_protocol::ScriptLifecyclePhase::Stopped)
    );
}

/// 执行器释放（daemon 退出）时必须取消在飞运行并有界回收。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dropping_runner_cancels_running_runs() {
    let root = tempfile::tempdir().expect("临时工作区");
    let entry = script_entry("idler", "stub_child_idles");
    write_manifest(root.path(), &[&entry]);
    let sink = Arc::new(RecordingScriptSink::default());
    let service = Arc::new(ScriptService::new(enabled_scripts(), sink.clone()));
    let accepted = accepted_snapshot(
        service
            .handle(Some(root.path()), run_request("idler"))
            .expect("脚本应可执行"),
    );
    wait_state(&service, &accepted.run_id, ScriptRunState::Running).await;

    // 执行器 Drop 会在有界窗口内取消运行并写入停止终态（事件出口在释放前必须可观察）。
    drop(service);
    assert_eq!(
        sink.phases().last().copied(),
        Some(soxu_protocol::ScriptLifecyclePhase::Stopped),
        "执行器释放必须为在飞运行写入 Stopped 终态"
    );
}

/// 有界等待运行进入指定状态（禁止在运行时线程上阻塞睡眠）。
async fn wait_state(
    service: &ScriptService,
    run_id: &str,
    expected: ScriptRunState,
) -> soxu_protocol::ScriptRunSnapshot {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        let snapshot = service.runner().inspect(run_id).expect("查询运行");
        if snapshot.state == expected {
            return snapshot;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "运行 {run_id} 未在 30s 内进入 {expected:?}（当前 {:?}）",
            snapshot.state
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

// ---------------------------------------------------------------------------
// 受控子进程桩（由父测试经 current_exe 重新执行，不依赖宿主任何第三方工具）
// ---------------------------------------------------------------------------
/// 受控子进程桩：正常退出。
#[test]
#[ignore = "受控子进程桩：仅由父测试经当前二进制重新执行"]
fn stub_child_exits_cleanly() {}

/// 受控子进程桩：刷出远超输出预算的字节，用于验证截断而不是无界缓冲。
#[test]
#[ignore = "受控子进程桩：仅由父测试经当前二进制重新执行"]
fn stub_child_floods_stdout() {
    const TOTAL_BYTES: usize = 1024 * 1024;
    let chunk = [b'x'; 8192];
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let mut written = 0;
    while written < TOTAL_BYTES {
        out.write_all(&chunk).expect("桩输出");
        written += chunk.len();
    }
    out.flush().expect("桩刷新");
}

/// 受控子进程桩：长时间挂起，用于验证超时后的进程树回收。
#[test]
#[ignore = "受控子进程桩：仅由父测试经当前二进制重新执行"]
fn stub_child_idles() {
    std::thread::sleep(Duration::from_secs(60));
}

/// 受控子进程桩：只报告自身可见的环境变量数量、键名与 PATH 存在性。
#[test]
#[ignore = "受控子进程桩：仅由父测试经当前二进制重新执行"]
fn stub_child_reports_environment() {
    let mut keys: Vec<String> = std::env::vars_os()
        .map(|(key, _)| key.to_string_lossy().into_owned())
        .collect();
    keys.sort();
    let path = if std::env::var_os("PATH").is_some() {
        "present"
    } else {
        "absent"
    };
    println!("SOXU-STUB-ENV-COUNT={}", keys.len());
    println!("SOXU-STUB-ENV-KEYS={}", keys.join(","));
    println!("SOXU-STUB-PATH={path}");
}

/// 受控子进程桩：短暂存活，用于验证幂等键不重复派生。
#[test]
#[ignore = "受控子进程桩：仅由父测试经当前二进制重新执行"]
fn stub_child_sleeps_briefly() {
    std::thread::sleep(Duration::from_millis(400));
}
