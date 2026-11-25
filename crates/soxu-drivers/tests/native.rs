use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use soxu_drivers::{
    Control, ControlSupport, DriverError, DriverEvent, NativeCliKind, NativeDriver, Outcome,
    Persistence, RunRequest, RunTarget, RunningTurn, SessionMode, UsageScope, discover_history,
};

const PROMPT: &str = "-n \"引号\" & | < > ^ ; %PATH% !test! $(false) `literal`\n第二行\\";
const PROVIDER_PROMPT: &str = "Repeat the fixture marker SOXU_ALPHA.";
const SESSION: &str = "aabbccdd-1234-4234-8234-aabbccddeeff";

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "Soxu fixture {} {timestamp} {}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).expect("create isolated fixture");
        Self(path)
    }

    fn dir(&self, name: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::create_dir_all(&path).expect("create child fixture");
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let root = std::env::temp_dir().canonicalize();
        if let (Ok(root), Ok(path)) = (root, self.0.canonicalize())
            && path.parent() == Some(root.as_path())
            && path.file_name() == self.0.file_name()
        {
            let _ = fs::remove_dir_all(path);
        }
    }
}

fn node() -> Option<PathBuf> {
    std::env::var_os("SOXU_TEST_NODE")
        .map(PathBuf::from)
        .or_else(|| {
            let path = std::env::var_os("PATH")?;
            let name = if cfg!(windows) { "node.exe" } else { "node" };
            std::env::split_paths(&path)
                .filter(|dir| dir.is_absolute())
                .map(|dir| dir.join(name))
                .find(|file| file.is_file())
        })
}

fn fixture(
    scratch: &Scratch,
    kind: NativeCliKind,
    label: &str,
    mode: &str,
    cwd: &Path,
) -> NativeDriver {
    let script = scratch.0.join("native stream fixture.cjs");
    fs::write(&script, include_bytes!("fixtures/stream.cjs")).expect("install test fixture");
    NativeDriver::with_program(
        kind,
        node().expect("Node is required for native process tests; set SOXU_TEST_NODE"),
        [script.into_os_string()],
    )
    .expect("native entry")
    .with_env("SOXU_FIXTURE_KIND", Some(label.into()))
    .with_env("SOXU_FIXTURE_MODE", Some(mode.into()))
    .with_env("SOXU_FIXTURE_PROMPT", Some(PROMPT.into()))
    .with_env("SOXU_FIXTURE_CWD", Some(cwd.as_os_str().to_owned()))
}

async fn event(turn: &mut RunningTurn) -> Option<DriverEvent> {
    tokio::time::timeout(Duration::from_secs(12), turn.next_event())
        .await
        .expect("event deadline")
}

async fn collect(mut turn: RunningTurn) -> Vec<DriverEvent> {
    let mut events = Vec::new();
    while let Some(next) = event(&mut turn).await {
        events.push(next);
    }
    events
}

#[tokio::test]
async fn codex_streams_both_targets_and_preserves_unknown_telemetry() {
    let scratch = Scratch::new();
    let quick = scratch.dir("quick chat");
    let local = scratch.dir("local project");
    for target in [RunTarget::QuickChat(quick), RunTarget::Directory(local)] {
        let driver = fixture(
            &scratch,
            NativeCliKind::Codex,
            "codex",
            "ok",
            target.directory(),
        );
        let mut request = RunRequest::new(target, PROMPT);
        request.persistence = Persistence::Ephemeral;
        let events = collect(driver.start(request).expect("spawn Codex fixture")).await;
        assert!(events.contains(&DriverEvent::Text(PROMPT.into())));
        assert!(events.contains(&DriverEvent::MalformedOutput));
        assert_eq!(
            events.last(),
            Some(&DriverEvent::Finished(Outcome::Succeeded))
        );
        let result = events
            .iter()
            .find_map(|event| match event {
                DriverEvent::Usage {
                    scope: UsageScope::Turn,
                    telemetry,
                } => Some(telemetry),
                _ => None,
            })
            .expect("native usage");
        assert_eq!(result.total_tokens, Some(13));
        assert_eq!(result.cache_hit_pct, Some(20.0));
        assert_eq!(result.used_context_tokens, None);
        assert_eq!(result.api_cost_usd, None);
    }
}

#[tokio::test]
async fn other_stdout_formats_and_argv_pass_through_directly() {
    let scratch = Scratch::new();
    let cwd = scratch.dir("project with spaces & metadata");
    for (kind, label) in [
        (NativeCliKind::ClaudeCode, "claude"),
        (NativeCliKind::QwenCode, "qwen"),
        (NativeCliKind::OpenCode, "opencode"),
        (NativeCliKind::Antigravity, "agy"),
        (NativeCliKind::Copilot, "copilot"),
    ] {
        let driver = fixture(&scratch, kind, label, "ok", &cwd);
        let events = collect(
            driver
                .start(RunRequest::new(RunTarget::Directory(cwd.clone()), PROMPT))
                .expect("spawn"),
        )
        .await;
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, DriverEvent::Text(_)))
                .count(),
            1,
            "{kind:?}"
        );
        assert!(
            events.contains(&DriverEvent::Text(PROMPT.into())),
            "{kind:?}"
        );
        assert_eq!(
            events.last(),
            Some(&DriverEvent::Finished(Outcome::Succeeded)),
            "{kind:?}"
        );
    }
}

#[tokio::test]
async fn codex_resume_and_fork_identifiers_are_separate_arguments() {
    let scratch = Scratch::new();
    let cwd = scratch.dir("historical workspace");
    for (session, flag) in [
        (SessionMode::Resume(SESSION.into()), "resume"),
        (SessionMode::Fork(SESSION.into()), "fork"),
    ] {
        let driver = fixture(&scratch, NativeCliKind::Codex, "codex", "ok", &cwd)
            .with_env("SOXU_FIXTURE_SESSION", Some(flag.into()));
        let mut request = RunRequest::new(RunTarget::Directory(cwd.clone()), PROMPT);
        request.session = session;
        let events = collect(driver.start(request).expect("spawn")).await;
        assert_eq!(
            events.last(),
            Some(&DriverEvent::Finished(Outcome::Succeeded))
        );
    }
}

#[tokio::test]
async fn codex_handles_failure_truncation_and_oversized_records() {
    let scratch = Scratch::new();
    let cwd = scratch.dir("fault fixture");
    for (mode, outcome) in [
        ("fail", Outcome::ProcessFailed { code: Some(7) }),
        ("truncated", Outcome::Incomplete),
        ("overflow", Outcome::Succeeded),
    ] {
        let driver = fixture(&scratch, NativeCliKind::Codex, "codex", mode, &cwd);
        let events = collect(
            driver
                .start(RunRequest::new(RunTarget::QuickChat(cwd.clone()), PROMPT))
                .expect("spawn"),
        )
        .await;
        assert_eq!(events.last(), Some(&DriverEvent::Finished(outcome)));
        if mode == "fail" {
            assert!(events.contains(&DriverEvent::NativeError));
        } else {
            assert!(events.contains(&DriverEvent::MalformedOutput));
        }
        assert!(!format!("{events:?}").contains("private diagnostic"));
    }
}

#[tokio::test]
async fn codex_interrupt_is_process_control_and_queue_is_deferred() {
    let scratch = Scratch::new();
    let cwd = scratch.dir("hold");
    let driver = fixture(&scratch, NativeCliKind::Codex, "codex", "hold", &cwd);
    let mut turn = driver
        .start(RunRequest::new(RunTarget::QuickChat(cwd), PROMPT))
        .expect("spawn");
    while event(&mut turn).await.expect("start") != DriverEvent::TurnStarted {}
    let control = turn.control();
    assert_eq!(
        control
            .send(1, Control::Queue(PROMPT.into()))
            .expect("state"),
        ControlSupport::DeferredToCaller
    );
    assert_eq!(
        control
            .send(2, Control::Steer(PROMPT.into()))
            .expect("state"),
        ControlSupport::Unavailable
    );
    assert_eq!(
        control.send(3, Control::Interrupt).expect("state"),
        ControlSupport::Process
    );
    assert_eq!(
        collect(turn).await.last(),
        Some(&DriverEvent::Finished(Outcome::Cancelled))
    );
}

#[tokio::test]
async fn pi_acknowledges_native_controls_including_abort_after_settled() {
    let scratch = Scratch::new();
    let cwd = scratch.dir("pi project");
    let driver = fixture(&scratch, NativeCliKind::Pi, "pi", "ok", &cwd);
    let file = cwd.join("prior session with spaces & metadata.jsonl");
    let mut request = RunRequest::new(RunTarget::Directory(cwd), PROMPT);
    request.session = SessionMode::Fork(file.into_os_string());
    let mut turn = driver.start(request).expect("spawn");
    while event(&mut turn).await.expect("start") != DriverEvent::TurnStarted {}
    let control = turn.control();
    for (id, command) in [
        (1, Control::Queue(PROMPT.into())),
        (2, Control::Steer(PROMPT.into())),
        (3, Control::Interrupt),
    ] {
        assert_eq!(
            control.send(id, command).expect("native control"),
            ControlSupport::Native
        );
    }
    let events = collect(turn).await;
    for id in 1..=3 {
        assert!(events.contains(&DriverEvent::ControlAck { id, accepted: true }));
    }
    assert!(events.iter().any(|event| matches!(
        event,
        DriverEvent::Usage { scope: UsageScope::Context, telemetry }
            if telemetry.used_context_tokens == Some(123)
    )));
    assert_eq!(
        events.last(),
        Some(&DriverEvent::Finished(Outcome::Cancelled))
    );
}

#[tokio::test]
#[ignore = "需要本机 Codex 和 loopback 连接"]
async fn real_codex_quick_chat_directory_resume_and_fork() {
    use std::process::Stdio;
    use tokio::io::{AsyncBufReadExt, BufReader};

    let scratch = Scratch::new();
    let home = scratch.dir("isolated codex home");
    let quick = scratch.dir("quick chat");
    let local = scratch.dir("local project");
    fs::write(local.join("fixture.txt"), "local project fixture").expect("local file");
    let script = scratch.0.join("loopback provider.cjs");
    let requests = scratch.0.join("provider request stages.log");
    fs::write(&requests, "").expect("initialize provider stages");
    fs::write(&script, include_bytes!("fixtures/provider.cjs")).expect("provider fixture");
    let mut provider = tokio::process::Command::new(node().expect("Node"))
        .arg(script)
        .env("SOXU_FIXTURE_PROMPT", PROVIDER_PROMPT)
        .env("SOXU_FIXTURE_REQUEST_LOG", &requests)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("start loopback provider");
    let mut output = BufReader::new(provider.stdout.take().expect("provider stdout")).lines();
    let port: u16 = tokio::time::timeout(Duration::from_secs(8), output.next_line())
        .await
        .expect("provider deadline")
        .expect("port output")
        .expect("port")
        .parse()
        .expect("loopback port");
    fs::write(
        home.join("config.toml"),
        format!(
            r#"model = "soxu-fixture"
model_provider = "fixture"

[model_providers.fixture]
name = "Soxu loopback fixture"
base_url = "http://127.0.0.1:{port}/v1"
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0
"#
        ),
    )
    .expect("write isolated config");
    let driver = match std::env::var_os("SOXU_TEST_CODEX") {
        Some(path) => NativeDriver::with_program(NativeCliKind::Codex, path.into(), []),
        None => NativeDriver::discover(NativeCliKind::Codex),
    }
    .expect("installed Codex")
    .with_env("CODEX_HOME", Some(home.clone().into_os_string()))
    .with_env("HTTP_PROXY", None)
    .with_env("HTTPS_PROXY", None)
    .with_env("ALL_PROXY", None)
    .with_env("NO_PROXY", Some("127.0.0.1,localhost".into()))
    .with_env("CODEX_CLI_DISABLE_UPDATE_CHECK", Some("1".into()));
    let version = tokio::time::timeout(
        Duration::from_secs(20),
        tokio::process::Command::new(driver.program())
            .arg("--version")
            .current_dir(&quick)
            .env("CODEX_HOME", &home)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .output(),
    )
    .await
    .expect("Codex version deadline")
    .expect("Codex executable");
    assert!(
        version.status.success(),
        "Codex executable={:?}",
        driver.program()
    );
    let version = String::from_utf8_lossy(&version.stdout);
    assert!(version.starts_with("codex-cli "));
    eprintln!(
        "Codex executable={:?}, version={}, isolated_home={}, provider=ready",
        driver.program(),
        version.trim(),
        home.join("config.toml").is_file()
    );

    let mut request = RunRequest::new(RunTarget::QuickChat(quick), PROVIDER_PROMPT);
    request.persistence = Persistence::Ephemeral;
    assert_codex_turn(
        collect_codex_loopback(
            driver.start(request).expect("quick chat"),
            "QuickChat",
            &requests,
        )
        .await,
        "SOXU_NEW",
    );
    let sessions = home.join("sessions");
    assert!(
        discover_history(NativeCliKind::Codex, &sessions)
            .expect("quick history")
            .entries
            .is_empty()
    );

    let target = RunTarget::Directory(local);
    let events = collect_codex_loopback(
        driver
            .start(RunRequest::new(target.clone(), PROVIDER_PROMPT))
            .expect("local chat"),
        "Directory",
        &requests,
    )
    .await;
    let original = assert_codex_turn(events, "SOXU_NEW");
    let history = discover_history(NativeCliKind::Codex, &sessions).expect("local history");
    let entry = history
        .entries
        .iter()
        .find(|entry| entry.id == original)
        .expect("native rollout");
    let mut request = RunRequest::new(target.clone(), format!("resume {PROVIDER_PROMPT}"));
    request.session = entry.resume();
    let events =
        collect_codex_loopback(driver.start(request).expect("resume"), "Resume", &requests).await;
    assert_eq!(assert_codex_turn(events, "SOXU_RESUMED"), original);

    let mut request = RunRequest::new(target, format!("fork {PROVIDER_PROMPT}"));
    request.session = entry.fork().expect("fork mode");
    let events =
        collect_codex_loopback(driver.start(request).expect("fork"), "Fork", &requests).await;
    assert_ne!(assert_codex_turn(events, "SOXU_FORKED"), original);
    assert_eq!(
        discover_history(NativeCliKind::Codex, &sessions)
            .expect("forked history")
            .entries
            .len(),
        2
    );
    let request_log = fs::read_to_string(&requests).expect("request stages");
    assert_eq!(
        request_log
            .lines()
            .filter(|line| *line == "responses_post")
            .count(),
        4,
        "{request_log}"
    );
    assert_eq!(
        request_log
            .lines()
            .filter(|line| *line == "SOXU_NEW")
            .count(),
        2,
        "{request_log}"
    );
    assert!(request_log.contains("SOXU_RESUMED\n") && request_log.contains("SOXU_FORKED\n"));
    eprintln!("loopback provider accepted four /responses requests and verified parent history");
    provider.kill().await.expect("reap loopback fixture");
}

async fn collect_codex_loopback(
    mut turn: RunningTurn,
    stage: &str,
    requests: &Path,
) -> Vec<DriverEvent> {
    let mut events = Vec::new();
    let result = tokio::time::timeout(Duration::from_secs(45), async {
        while let Some(event) = turn.next_event().await {
            events.push(event);
        }
    })
    .await;
    if result.is_err() {
        turn.control().cancel();
        let phases = events
            .iter()
            .map(std::mem::discriminant)
            .collect::<Vec<_>>();
        panic!(
            "{stage} deadline; event kinds={phases:?}; provider stages={:?}",
            fs::read_to_string(requests)
        );
    }
    assert_eq!(
        events.last(),
        Some(&DriverEvent::Finished(Outcome::Succeeded)),
        "{stage}; provider stages={:?}",
        fs::read_to_string(requests)
    );
    events
}

fn assert_codex_turn(events: Vec<DriverEvent>, expected: &str) -> String {
    assert_eq!(
        events.last(),
        Some(&DriverEvent::Finished(Outcome::Succeeded)),
        "{events:?}"
    );
    assert!(events.contains(&DriverEvent::TurnStarted));
    assert!(events.contains(&DriverEvent::Text(expected.into())));
    let usage = events
        .iter()
        .find_map(|event| match event {
            DriverEvent::Usage {
                scope: UsageScope::Turn,
                telemetry,
            } => Some(telemetry),
            _ => None,
        })
        .expect("native turn usage");
    assert!(
        usage.input_tokens.is_some() && usage.output_tokens.is_some(),
        "expected={expected}, usage={usage:?}"
    );
    events
        .into_iter()
        .find_map(|event| match event {
            DriverEvent::Session { id } => Some(id),
            _ => None,
        })
        .expect("native session ID")
}

#[tokio::test]
#[ignore = "需要已登录的本机 Codex 和推理网络"]
async fn signed_in_codex_streams_both_targets() {
    let scratch = Scratch::new();
    let quick = scratch.dir("quick chat");
    let local = scratch.dir("local project");
    fs::write(local.join("fixture.txt"), "local project fixture").expect("local file");
    let driver = NativeDriver::discover(NativeCliKind::Codex)
        .expect("installed Codex")
        .with_env("CODEX_CLI_DISABLE_UPDATE_CHECK", Some("1".into()));
    for target in [RunTarget::QuickChat(quick), RunTarget::Directory(local)] {
        let mut request =
            RunRequest::new(target, "Reply with exactly SOXU_SMOKE_OK. Use no tools.");
        request.persistence = Persistence::Ephemeral;
        let turn = driver.start(request).expect("start installed Codex");
        let mut turn = turn;
        let (mut session, mut started, mut text, mut usage) = (false, false, false, false);
        let mut outcome = None;
        tokio::time::timeout(Duration::from_secs(45), async {
            while let Some(event) = turn.next_event().await {
                match event {
                    DriverEvent::Session { .. } => session = true,
                    DriverEvent::TurnStarted => started = true,
                    DriverEvent::Text(answer) => text |= answer.trim() == "SOXU_SMOKE_OK",
                    DriverEvent::Usage {
                        scope: UsageScope::Turn,
                        telemetry,
                    } => {
                        usage |=
                            telemetry.input_tokens.is_some() && telemetry.output_tokens.is_some();
                    }
                    DriverEvent::Finished(result) => outcome = Some(result),
                    _ => {}
                }
            }
        })
        .await
        .expect("signed-in Codex deadline");
        assert_eq!(
            outcome,
            Some(Outcome::Succeeded),
            "session={session}, started={started}, usage={usage}"
        );
        assert!(
            session && started && text && usage,
            "session={session}, started={started}, expected_text={text}, usage={usage}"
        );
    }
}

#[test]
fn unsupported_modes_and_argument_injection_are_rejected_before_spawn() {
    let scratch = Scratch::new();
    let cwd = scratch.dir("project");
    let path = scratch.0.join("unused.exe");
    let driver = NativeDriver::with_program(NativeCliKind::Copilot, path, []).expect("configured");
    let mut request = RunRequest::new(RunTarget::Directory(cwd.clone()), PROMPT);
    request.session = SessionMode::Fork(SESSION.into());
    assert!(matches!(
        driver.start(request),
        Err(DriverError::Unsupported("fork"))
    ));
    let mut request = RunRequest::new(RunTarget::Directory(cwd.clone()), PROMPT);
    request.session = SessionMode::Resume("--allow-all".into());
    assert!(matches!(
        driver.start(request),
        Err(DriverError::InvalidRequest("session id"))
    ));
    let mut request = RunRequest::new(RunTarget::Directory(cwd), PROMPT);
    request.persistence = Persistence::Ephemeral;
    assert!(matches!(
        driver.start(request),
        Err(DriverError::Unsupported("ephemeral"))
    ));
}

#[cfg(windows)]
#[test]
fn script_shims_are_rejected() {
    let scratch = Scratch::new();
    for name in ["pi.cmd", "copilot.ps1", "qwen.bat"] {
        assert!(matches!(
            NativeDriver::with_program(NativeCliKind::Pi, scratch.0.join(name), []),
            Err(DriverError::ScriptShim)
        ));
    }
}

#[test]
fn history_is_read_only_and_uses_verified_native_session_identity() {
    use NativeCliKind::{ClaudeCode, Codex, Copilot, OpenCode, Pi, QwenCode};
    let scratch = Scratch::new();
    let cases = [
        (
            Codex,
            "sessions/2026/11",
            format!("rollout-{SESSION}.jsonl"),
            serde_json::json!({"type":"session_meta","payload":{"id":SESSION}}),
        ),
        (
            Pi,
            "pi workspace",
            "old session.jsonl".into(),
            serde_json::json!({"type":"session","id":SESSION}),
        ),
        (
            ClaudeCode,
            "claude project",
            format!("{SESSION}.jsonl"),
            serde_json::json!({"type":"user","sessionId":SESSION}),
        ),
        (
            QwenCode,
            "qwen project/chats",
            format!("{SESSION}.jsonl"),
            serde_json::json!({"type":"user","uuid":"message","sessionId":SESSION}),
        ),
        (
            Copilot,
            SESSION,
            "events.jsonl".into(),
            serde_json::json!({"type":"session.start","data":{"sessionId":SESSION}}),
        ),
    ];
    for (kind, dir, file, payload) in cases {
        let root = scratch.dir(kind.binary_name());
        let dir = root.join(dir);
        fs::create_dir_all(&dir).expect("session path");
        let path = dir.join(file);
        let history = format!("malformed\n{payload}\n");
        fs::write(&path, &history).expect("session fixture");
        let result = discover_history(kind, &root).expect("read-only history");
        assert_eq!(result.entries.len(), 1, "{kind:?}");
        assert_eq!(result.entries[0].id, SESSION);
        assert_eq!(result.entries[0].fork().is_some(), kind != Copilot);
        let expected = if kind == Pi {
            path.as_os_str().to_owned()
        } else {
            OsString::from(SESSION)
        };
        assert_eq!(result.entries[0].resume(), SessionMode::Resume(expected));
        assert_eq!(fs::read_to_string(&path).expect("read fixture"), history);
    }
    assert_eq!(
        discover_history(OpenCode, &scratch.0)
            .expect_err("unavailable")
            .kind(),
        std::io::ErrorKind::Unsupported
    );
}
