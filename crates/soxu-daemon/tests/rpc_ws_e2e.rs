//! `soxu-daemon` HTTP / WebSocket 端到端集成测试（真实 TCP 监听 + SQLite 持久恢复）。

use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use soxu_daemon::{DaemonState, SqliteStore, build_router};
use soxu_drivers::{NativeCliKind, NativeDriver};
use soxu_protocol::{
    ActiveTurnBehavior, ApprovalItemSnapshot, ApprovalResolutionKind, AskQuestionOption,
    ClaimPairingRequest, ClientHello, ClientKind, DaemonEvent, DraftSnapshot, ErrorCode,
    EventEnvelope, PairingCredentialInput, PairingTransportMode, PendingUserInputAnswer,
    PendingUserInputAnswerItem, PendingUserInputQuestion, PendingUserInputRequest, PermissionLevel,
    ProjectSummary, RpcCommand, RpcRequest, RpcResponse, RpcResultData, ServerHello, SessionTarget,
    SubagentStatus, SubagentSummary, ThinkingEffort, TurnLifecycleState, TurnMessageRole,
    WorkspaceBucket, WorkspaceIsolation, WorkspaceSummary,
};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message as WsMsg;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type WsClient = WebSocketStream<MaybeTlsStream<TcpStream>>;

async fn spawn_test_daemon(
    dir: &TempDir,
) -> (
    std::net::SocketAddr,
    Arc<DaemonState>,
    tokio::task::JoinHandle<()>,
) {
    spawn_daemon(dir, None).await
}

async fn spawn_daemon(
    dir: &TempDir,
    driver: Option<NativeDriver>,
) -> (
    std::net::SocketAddr,
    Arc<DaemonState>,
    tokio::task::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral test port");
    let addr = listener.local_addr().expect("read local addr");
    let endpoint = format!("http://{addr}");
    let state = DaemonState::open(Some(dir.path()), &endpoint, false).expect("open daemon state");
    let state = Arc::new(match driver {
        Some(driver) => state
            .with_native_turns(dir.path().join("quick-chats"), Ok(driver))
            .expect("recover previous native turns"),
        None => state,
    });
    let router = build_router(Arc::clone(&state));
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    (addr, state, handle)
}

fn codex_fixture() -> NativeDriver {
    let output = std::process::Command::new("node")
        .args(["-p", "process.execPath"])
        .output()
        .expect("find real Node executable");
    assert!(output.status.success());
    let program = String::from_utf8(output.stdout).expect("Node executable path");
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/codex.cjs");
    NativeDriver::with_program(
        NativeCliKind::Codex,
        PathBuf::from(program.trim()),
        [script.into_os_string()],
    )
    .expect("set Codex fixture")
}

fn submit_native(session_id: &str, text: &str) -> RpcCommand {
    RpcCommand::SubmitTurn {
        session_id: session_id.to_owned(),
        text: text.to_owned(),
        chips: Vec::new(),
        behavior: ActiveTurnBehavior::Queue,
        handoff: None,
        handoff_scope: None,
    }
}

async fn post_rpc(addr: std::net::SocketAddr, command: RpcCommand) -> RpcResponse {
    let request = serde_json::to_string(&RpcRequest {
        request_id: "http-test".to_owned(),
        command,
    })
    .expect("serialize HTTP RPC");
    let (status, body) = http_request(addr, "POST", "/api/rpc", None, Some(&request)).await;
    assert_eq!(status, 200);
    serde_json::from_str(&body).expect("decode HTTP RPC")
}

async fn wait_for_ws_event(
    ws: &mut WsClient,
    mut matches: impl FnMut(&DaemonEvent) -> bool,
) -> u64 {
    tokio::time::timeout(Duration::from_secs(45), async {
        loop {
            let envelope: EventEnvelope<DaemonEvent> =
                serde_json::from_str(&next_text_frame(ws).await).expect("decode native event");
            if matches(&envelope.payload) {
                return envelope.seq;
            }
        }
    })
    .await
    .expect("receive matching native event")
}

#[tokio::test]
async fn native_codex_http_ws_fifo_stop_failure_and_persistence() {
    let dir = TempDir::new().expect("create test data dir");
    let (addr, state, handle) = spawn_daemon(&dir, Some(codex_fixture())).await;
    let created = post_rpc(
        addr,
        RpcCommand::CreateQuickChat {
            title: None,
            agent_id: "codex".to_owned(),
            model_id: None,
            permission: Some(PermissionLevel::ReadOnly),
            thinking: None,
        },
    )
    .await;
    assert!(created.ok);
    let mut handshake =
        tokio_tungstenite::tungstenite::client::IntoClientRequest::into_client_request(format!(
            "ws://{addr}/ws"
        ))
        .expect("construct WS request");
    handshake
        .headers_mut()
        .insert("origin", "https://example.invalid".parse().expect("origin"));
    let denied = tokio_tungstenite::connect_async(handshake)
        .await
        .expect_err("deny foreign origin");
    assert!(matches!(
        denied,
        tokio_tungstenite::tungstenite::Error::Http(response) if response.status() == 403
    ));
    let (mut ws, hello) = connect_ws(
        addr,
        ClientHello {
            client_kind: ClientKind::Desktop,
            client_version: "test".to_owned(),
            protocol_version: soxu_protocol::PROTOCOL_VERSION,
            after_seq: None,
            device_id: None,
        },
    )
    .await;
    let session_id = hello.snapshot.sessions[0].session_id.clone();
    let (first, _) = rpc_via_ws(&mut ws, "native-1", submit_native(&session_id, "hold")).await;
    assert!(first.ok);
    let first_seq = wait_for_ws_event(&mut ws, |event| {
        matches!(event, DaemonEvent::TurnStreamDelta {text_delta,..} if text_delta == "回复 hold")
    })
    .await;
    let queued = post_rpc(addr, submit_native(&session_id, "queued")).await;
    assert!(matches!(
        queued.result,
        Some(RpcResultData::TurnSubmitted {
            queued_id: Some(_),
            ..
        })
    ));
    assert!(
        post_rpc(
            addr,
            RpcCommand::StopTurn {
                session_id: session_id.clone()
            }
        )
        .await
        .ok
    );
    let stop_seq = wait_for_ws_event(&mut ws, |event| {
        matches!(event, DaemonEvent::SessionUpdated(session) if session.lifecycle == TurnLifecycleState::Idle)
    })
    .await;
    assert!(stop_seq > first_seq);
    assert_eq!(
        state
            .list_session_messages(&session_id)
            .expect("stored partial turn")
            .len(),
        2
    );

    let after_stop = post_rpc(addr, submit_native(&session_id, "after-stop")).await;
    assert!(matches!(
        after_stop.result,
        Some(RpcResultData::TurnSubmitted {
            queued_id: Some(_),
            ..
        })
    ));
    let queued_seq = wait_for_ws_event(&mut ws, |event| {
        matches!(event, DaemonEvent::TurnStreamDelta {text_delta,..} if text_delta == "回复 queued")
    }).await;
    let latest_seq = wait_for_ws_event(&mut ws, |event| {
        matches!(event, DaemonEvent::TurnStreamDelta {text_delta,..} if text_delta == "回复 after-stop")
    }).await;
    assert!(stop_seq < queued_seq && queued_seq < latest_seq);
    wait_for_ws_event(&mut ws, |event| {
        matches!(event, DaemonEvent::SessionUpdated(session) if session.lifecycle == TurnLifecycleState::Idle)
    }).await;

    assert!(post_rpc(addr, submit_native(&session_id, "fail")).await.ok);
    wait_for_ws_event(&mut ws, |event| {
        matches!(event, DaemonEvent::ErrorRaised {error,..} if error.code == ErrorCode::DriverUnavailable && error.summary_zh == "Codex 原生回合失败")
    }).await;
    handle.abort();
    let _ = handle.await;
    drop(state);
    let restored = DaemonState::open(Some(dir.path()), &format!("http://{addr}"), false)
        .expect("reopen database");
    let snapshot = restored.snapshot().expect("reopen snapshot");
    let session = &snapshot.sessions[0];
    assert_eq!(session.bucket, WorkspaceBucket::Failed);
    assert_eq!(session.lifecycle, TurnLifecycleState::Idle);
    assert_eq!(
        session.native_session_id.as_deref(),
        Some("aabbccdd-1234-4234-8234-aabbccddeeff")
    );
    let messages = restored
        .list_session_messages(&session_id)
        .expect("reopen turn history");
    assert_eq!(messages.len(), 8);
    let replies: Vec<_> = messages
        .iter()
        .filter(|msg| msg.role == TurnMessageRole::Assistant)
        .map(|msg| msg.text.as_str())
        .collect();
    assert_eq!(replies, ["回复 hold", "回复 queued", "回复 after-stop", ""]);
}

fn bind_local_fixture(dir: &TempDir, state: &DaemonState) -> SessionTarget {
    let project_dir = dir.path().join("project-fixture");
    std::fs::create_dir(&project_dir).expect("create existing project dir");
    state
        .upsert_project(ProjectSummary {
            project_id: "project-fixture".to_owned(),
            name: "测试项目".to_owned(),
            display_path: "~/project-fixture".to_owned(),
            root_path: project_dir.to_string_lossy().into_owned(),
            is_git_repo: false,
            default_branch: None,
            aggregated_bucket: WorkspaceBucket::Done,
            workspace_count: 1,
        })
        .expect("bind test project");
    state
        .upsert_workspace(WorkspaceSummary {
            workspace_id: "workspace-fixture".to_owned(),
            project_id: "project-fixture".to_owned(),
            project_name: "测试项目".to_owned(),
            display_path: "~/project-fixture".to_owned(),
            branch: String::new(),
            isolation: WorkspaceIsolation::Local,
            agent_id: "codex".to_owned(),
            model_id: String::new(),
            bucket: WorkspaceBucket::Done,
            summary: String::new(),
            diff_stat: String::new(),
            label: None,
            archived: false,
            needs_recovery: false,
            recovery_reason: None,
            updated_at: Utc::now(),
        })
        .expect("bind local workspace");
    SessionTarget::Workspace {
        workspace_id: "workspace-fixture".to_owned(),
    }
}

#[tokio::test]
async fn native_codex_uses_bound_directory_and_requires_read_only() {
    let dir = TempDir::new().expect("create test data dir");
    let (addr, state, handle) = spawn_daemon(&dir, Some(codex_fixture())).await;
    let target = bind_local_fixture(&dir, &state);
    let create = |permission| RpcCommand::CreateSession {
        target: target.clone(),
        title: None,
        agent_id: "codex".to_owned(),
        model_id: None,
        permission: Some(permission),
        thinking: None,
    };
    let RpcResultData::SessionCreated(writable) = post_rpc(addr, create(PermissionLevel::Ask))
        .await
        .result
        .expect("create default session")
    else {
        panic!("expected session");
    };
    let denied = post_rpc(addr, submit_native(&writable.session_id, "directory")).await;
    assert_eq!(
        denied.error.expect("permission error").code,
        ErrorCode::DriverUnavailable
    );
    assert!(
        state
            .list_session_messages(&writable.session_id)
            .expect("load denied session history")
            .is_empty()
    );

    let RpcResultData::SessionCreated(session) = post_rpc(addr, create(PermissionLevel::ReadOnly))
        .await
        .result
        .expect("create read-only session")
    else {
        panic!("expected session");
    };
    let (mut ws, _) = connect_ws(
        addr,
        ClientHello {
            client_kind: ClientKind::Desktop,
            client_version: "test".to_owned(),
            protocol_version: soxu_protocol::PROTOCOL_VERSION,
            after_seq: None,
            device_id: None,
        },
    )
    .await;
    assert!(
        post_rpc(addr, submit_native(&session.session_id, "directory"))
            .await
            .ok
    );
    wait_for_ws_event(&mut ws, |event| {
        matches!(event, DaemonEvent::SessionUpdated(updated) if updated.lifecycle == TurnLifecycleState::Idle && updated.bucket == WorkspaceBucket::Done)
    }).await;
    let messages = state
        .list_session_messages(&session.session_id)
        .expect("load bound directory history");
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[1].text, "回复 directory");
    handle.abort();
    let _ = handle.await;
    let mut interrupted = state
        .snapshot()
        .expect("read session before restart")
        .sessions
        .into_iter()
        .find(|item| item.session_id == session.session_id)
        .expect("completed session");
    interrupted.bucket = WorkspaceBucket::Running;
    interrupted.lifecycle = TurnLifecycleState::Running;
    SqliteStore::open(&dir.path().join("soxu-daemon.db"))
        .expect("open fixture database")
        .upsert_session(&interrupted)
        .expect("simulate interrupted session");
    drop(state);
    let (_, reopened, reopened_handle) = spawn_daemon(&dir, Some(codex_fixture())).await;
    let restored = reopened.snapshot().expect("snapshot after interruption");
    let recovered = restored
        .sessions
        .iter()
        .find(|item| item.session_id == session.session_id)
        .expect("interrupted session");
    assert_eq!(recovered.lifecycle, TurnLifecycleState::Idle);
    assert_eq!(recovered.bucket, WorkspaceBucket::Failed);
    assert_eq!(recovered.native_session_id, interrupted.native_session_id);
    assert_eq!(restored.workspaces[0].bucket, WorkspaceBucket::Failed);
    reopened_handle.abort();
}

#[tokio::test]
#[ignore = "需本机 Codex EXE 与可用的隔离 CODEX_HOME"]
async fn native_codex_executable_smoke() {
    let program = PathBuf::from(std::env::var_os("SOXU_CODEX_EXE").expect("real Codex executable"));
    let codex_home = std::env::var_os("SOXU_TEST_CODEX_HOME").expect("isolated Codex home");
    let driver = NativeDriver::with_program(NativeCliKind::Codex, program, [])
        .expect("configure real Codex")
        .with_env("CODEX_HOME", Some(codex_home));
    let dir = TempDir::new().expect("create isolated daemon state");
    let (addr, state, handle) = spawn_daemon(&dir, Some(driver)).await;
    let target = bind_local_fixture(&dir, &state);
    for command in [
        RpcCommand::CreateQuickChat {
            title: None,
            agent_id: "codex".to_owned(),
            model_id: None,
            permission: Some(PermissionLevel::ReadOnly),
            thinking: None,
        },
        RpcCommand::CreateSession {
            target,
            title: None,
            agent_id: "codex".to_owned(),
            model_id: None,
            permission: Some(PermissionLevel::ReadOnly),
            thinking: None,
        },
    ] {
        assert!(post_rpc(addr, command).await.ok);
    }
    let (mut ws, hello) = connect_ws(
        addr,
        ClientHello {
            client_kind: ClientKind::Desktop,
            client_version: "test".to_owned(),
            protocol_version: soxu_protocol::PROTOCOL_VERSION,
            after_seq: None,
            device_id: None,
        },
    )
    .await;
    assert_eq!(hello.snapshot.sessions.len(), 2);
    for session in hello.snapshot.sessions {
        let session_id = session.session_id;
        let prompt = "Reply only SOXU_NATIVE_CODEX_OK. Do not use tools.";
        assert!(post_rpc(addr, submit_native(&session_id, prompt)).await.ok);
        wait_for_ws_event(&mut ws, |event| {
            matches!(event, DaemonEvent::SessionUpdated(updated) if updated.session_id == session_id && updated.lifecycle == TurnLifecycleState::Idle)
        }).await;
        let snapshot = state.snapshot().expect("load completed real session");
        let completed = snapshot
            .sessions
            .iter()
            .find(|item| item.session_id == session_id)
            .expect("find completed real session");
        assert_eq!(
            completed.bucket,
            WorkspaceBucket::Done,
            "{:?}",
            completed.target
        );
        assert!(completed.native_session_id.is_some());
        let messages = state
            .list_session_messages(&session_id)
            .expect("load real messages");
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[1].text.trim(), "SOXU_NATIVE_CODEX_OK");
    }
    handle.abort();
}

async fn http_request(
    addr: std::net::SocketAddr,
    method: &str,
    path: &str,
    bearer: Option<&str>,
    body: Option<&str>,
) -> (u16, String) {
    let mut stream = TcpStream::connect(addr).await.expect("connect http");
    let payload = body.unwrap_or("");
    let auth_header = bearer
        .map(|t| format!("Authorization: Bearer {t}\r\n"))
        .unwrap_or_default();
    let content_headers = if body.is_some() {
        format!(
            "Content-Type: application/json\r\nContent-Length: {}\r\n",
            payload.len()
        )
    } else {
        String::new()
    };
    let req = format!(
        "{method} {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n{auth_header}{content_headers}\r\n{payload}"
    );
    stream
        .write_all(req.as_bytes())
        .await
        .expect("write http request");
    let mut raw = Vec::new();
    stream
        .read_to_end(&mut raw)
        .await
        .expect("read http response");
    let text = String::from_utf8(raw).expect("utf8 http response");
    let (head, body_part) = text.split_once("\r\n\r\n").expect("split http head/body");
    let status_line = head.lines().next().expect("status line");
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .expect("status code token")
        .parse()
        .expect("parse status code");
    (status, body_part.to_owned())
}

async fn connect_ws(addr: std::net::SocketAddr, hello: ClientHello) -> (WsClient, ServerHello) {
    let url = format!("ws://{addr}/ws");
    let (mut ws, _) = tokio_tungstenite::connect_async(url)
        .await
        .expect("connect websocket");
    let hello_json = serde_json::to_string(&hello).expect("serialize ClientHello");
    ws.send(WsMsg::Text(hello_json.into()))
        .await
        .expect("send ClientHello");
    let frame = next_text_frame(&mut ws).await;
    let server_hello: ServerHello = serde_json::from_str(&frame).expect("parse ServerHello");
    (ws, server_hello)
}

async fn next_text_frame(ws: &mut WsClient) -> String {
    loop {
        let msg = ws
            .next()
            .await
            .expect("ws stream open")
            .expect("ws frame ok");
        match msg {
            WsMsg::Text(txt) => return txt.to_string(),
            WsMsg::Ping(data) => {
                ws.send(WsMsg::Pong(data)).await.expect("send pong");
            }
            WsMsg::Pong(_) => {}
            other => panic!("unexpected ws message: {other:?}"),
        }
    }
}

async fn rpc_via_ws(
    ws: &mut WsClient,
    request_id: &str,
    command: RpcCommand,
) -> (RpcResponse, Vec<EventEnvelope<DaemonEvent>>) {
    let req = RpcRequest {
        request_id: request_id.to_owned(),
        command,
    };
    let raw = serde_json::to_string(&req).expect("serialize RpcRequest");
    ws.send(WsMsg::Text(raw.into()))
        .await
        .expect("send RpcRequest");

    let mut events = Vec::new();
    loop {
        let text = next_text_frame(ws).await;
        if let Ok(resp) = serde_json::from_str::<RpcResponse>(&text)
            && resp.request_id == request_id
        {
            return (resp, events);
        }
        let env: EventEnvelope<DaemonEvent> =
            serde_json::from_str(&text).expect("expected EventEnvelope or RpcResponse");
        events.push(env);
    }
}

#[tokio::test]
async fn full_m1_ws_rpc_redline_pairing_catchup_and_persistence_e2e() {
    let dir = TempDir::new().expect("create temp dir");
    let (addr, state, handle) = spawn_test_daemon(&dir).await;

    // 1. WS 握手与创建 QuickChat
    let (mut ws, hello) = connect_ws(
        addr,
        ClientHello {
            client_kind: ClientKind::Desktop,
            client_version: "0.1.0".to_owned(),
            protocol_version: soxu_protocol::PROTOCOL_VERSION,
            after_seq: None,
            device_id: None,
        },
    )
    .await;
    assert_eq!(hello.protocol_version, soxu_protocol::PROTOCOL_VERSION);

    let (resp, _) = rpc_via_ws(
        &mut ws,
        "req-qc",
        RpcCommand::CreateQuickChat {
            title: Some("架构与协议验证".to_owned()),
            agent_id: "agy".to_owned(),
            model_id: Some("gemini-4-carbon".to_owned()),
            permission: Some(PermissionLevel::Turbo),
            thinking: Some(ThinkingEffort::Max),
        },
    )
    .await;
    assert!(resp.ok);
    let qc = match resp.result.expect("quick chat result") {
        RpcResultData::QuickChatCreated(q) => q,
        other => panic!("unexpected result: {other:?}"),
    };

    let (snap_resp, _) = rpc_via_ws(&mut ws, "req-snap", RpcCommand::GetStateSnapshot).await;
    let snap = match snap_resp.result.expect("snapshot") {
        RpcResultData::StateSnapshot(s) => s,
        other => panic!("unexpected: {other:?}"),
    };
    let sess = snap
        .sessions
        .iter()
        .find(|s| match &s.target {
            SessionTarget::QuickChat { chat_id } => chat_id == &qc.chat_id,
            SessionTarget::Workspace { .. } => false,
        })
        .expect("session created with quick chat")
        .clone();

    // 1b. 首轮提交进入 Running，次轮 Queue 入队并提级为 Steer
    let (t1_resp, _) = rpc_via_ws(
        &mut ws,
        "req-t1",
        RpcCommand::SubmitTurn {
            session_id: sess.session_id.clone(),
            text: "梳理多端 RPC 契约".to_owned(),
            chips: Vec::new(),
            behavior: ActiveTurnBehavior::Queue,
            handoff: None,
            handoff_scope: None,
        },
    )
    .await;
    assert!(t1_resp.ok);

    let (t2_resp, _) = rpc_via_ws(
        &mut ws,
        "req-t2",
        RpcCommand::SubmitTurn {
            session_id: sess.session_id.clone(),
            text: "中途加入游标追平单测".to_owned(),
            chips: Vec::new(),
            behavior: ActiveTurnBehavior::Queue,
            handoff: None,
            handoff_scope: None,
        },
    )
    .await;
    let queued_id = match t2_resp.result.expect("turn2 result") {
        RpcResultData::TurnSubmitted { queued_id, .. } => queued_id.expect("queued id"),
        other => panic!("unexpected: {other:?}"),
    };

    let (promo_resp, _) = rpc_via_ws(
        &mut ws,
        "req-promo",
        RpcCommand::PromoteQueuedTurn {
            session_id: sess.session_id.clone(),
            queue_id: queued_id,
            target_behavior: ActiveTurnBehavior::Steer,
        },
    )
    .await;
    assert!(promo_resp.ok);

    // 2. 切换代理至 copilot 验证 lost-controls 回退通知
    let (sw_resp, _) = rpc_via_ws(
        &mut ws,
        "req-switch",
        RpcCommand::SwitchSessionCapabilities {
            session_id: sess.session_id.clone(),
            agent_id: "copilot".to_owned(),
            model_id: None,
            permission: None,
            thinking: None,
            behavior: ActiveTurnBehavior::Steer,
        },
    )
    .await;
    match sw_resp.result.expect("switch result") {
        RpcResultData::CapabilitiesSwitched { lost_controls, .. } => {
            assert_eq!(lost_controls.len(), 4);
        }
        other => panic!("unexpected: {other:?}"),
    }

    // 3. 高危 rm -rf 审批红线拦截 AllowSession，ApproveOnce 后再次审批报 Conflict
    let approval = state
        .register_pending_approval(ApprovalItemSnapshot {
            approval_id: "apr-e2e-1".to_owned(),
            session_id: sess.session_id.clone(),
            target: sess.target.clone(),
            branch_label: "快速对话".to_owned(),
            agent_id: "codex".to_owned(),
            tool_kind: "Bash".to_owned(),
            command_or_target: "rm -rf ./tmp-demo".to_owned(),
            risk_note: String::new(),
            high_risk: false,
            allow_session_permitted: true,
            diff_preview: Vec::new(),
            audit: None,
            created_at: Utc::now(),
        })
        .expect("register approval");
    assert!(approval.high_risk);
    assert!(!approval.allow_session_permitted);

    let (blocked_resp, _) = rpc_via_ws(
        &mut ws,
        "req-apr-block",
        RpcCommand::ResolveApproval {
            approval_id: approval.approval_id.clone(),
            resolution: ApprovalResolutionKind::AllowSession,
            guidance_note: None,
            resolved_by: "desktop".to_owned(),
        },
    )
    .await;
    assert!(!blocked_resp.ok);
    assert_eq!(
        blocked_resp.error.expect("redline err").code,
        ErrorCode::SafetyRedlineBlocked
    );

    let (ok_apr, _) = rpc_via_ws(
        &mut ws,
        "req-apr-ok",
        RpcCommand::ResolveApproval {
            approval_id: approval.approval_id.clone(),
            resolution: ApprovalResolutionKind::ApproveOnce,
            guidance_note: None,
            resolved_by: "mobile".to_owned(),
        },
    )
    .await;
    assert!(ok_apr.ok);

    let (dup_apr, _) = rpc_via_ws(
        &mut ws,
        "req-apr-dup",
        RpcCommand::ResolveApproval {
            approval_id: approval.approval_id.clone(),
            resolution: ApprovalResolutionKind::ApproveOnce,
            guidance_note: None,
            resolved_by: "desktop".to_owned(),
        },
    )
    .await;
    assert!(!dup_apr.ok);
    assert_eq!(
        dup_apr.error.expect("conflict err").code,
        ErrorCode::Conflict
    );

    // 3b. 结构化提问卡与子代理 Detach
    let ask_req = state
        .register_pending_user_input(PendingUserInputRequest {
            request_id: "ask-e2e".to_owned(),
            session_id: sess.session_id.clone(),
            title: "确认策略".to_owned(),
            questions: vec![PendingUserInputQuestion {
                question_id: "q1".to_owned(),
                prompt: "选择降级策略".to_owned(),
                multi_select: false,
                options: vec![AskQuestionOption {
                    option_id: "opt1".to_owned(),
                    label: "柔和中断".to_owned(),
                    description: None,
                    recommended: true,
                }],
            }],
            created_at: Utc::now(),
        })
        .expect("register ask");
    let (ask_resp, _) = rpc_via_ws(
        &mut ws,
        "req-ask",
        RpcCommand::SubmitUserInput(PendingUserInputAnswer {
            request_id: ask_req.request_id,
            answers: vec![PendingUserInputAnswerItem {
                question_id: "q1".to_owned(),
                selected_option_ids: vec!["opt1".to_owned()],
                custom_note: None,
            }],
            submitted_at: Utc::now(),
        }),
    )
    .await;
    assert!(ask_resp.ok);

    state
        .register_subagent(SubagentSummary {
            subagent_id: "sub-1".to_owned(),
            parent_session_id: sess.session_id.clone(),
            agent_id: "pi".to_owned(),
            model_id: "auto/deepseek-v4.1-flash-1017".to_owned(),
            task_title: "单测巡检".to_owned(),
            status: SubagentStatus::Running,
            elapsed_sec: 4,
            recent_activities: vec!["cargo test".to_owned()],
            detached_session_id: None,
        })
        .expect("register subagent");
    let (det_resp, _) = rpc_via_ws(
        &mut ws,
        "req-detach",
        RpcCommand::DetachSubagent {
            subagent_id: "sub-1".to_owned(),
        },
    )
    .await;
    assert!(det_resp.ok);

    // 4. Draft generation 防旧写覆盖
    let (d2_resp, _) = rpc_via_ws(
        &mut ws,
        "req-d2",
        RpcCommand::SaveDraft(DraftSnapshot {
            target: sess.target.clone(),
            session_id: sess.session_id.clone(),
            text: "新版草稿".to_owned(),
            chips: Vec::new(),
            generation: 2,
            updated_at: Utc::now(),
        }),
    )
    .await;
    assert!(d2_resp.ok);

    let (d1_resp, _) = rpc_via_ws(
        &mut ws,
        "req-d1",
        RpcCommand::SaveDraft(DraftSnapshot {
            target: sess.target.clone(),
            session_id: sess.session_id.clone(),
            text: "过期草稿".to_owned(),
            chips: Vec::new(),
            generation: 1,
            updated_at: Utc::now(),
        }),
    )
    .await;
    assert!(!d1_resp.ok);
    assert_eq!(
        d1_resp.error.expect("stale draft").code,
        ErrorCode::Conflict
    );

    let seq_before_pairing = state.latest_seq().expect("seq before pairing");

    // 5. 配对生成、6 位码认领、重放拒绝、Bearer 访问与吊销失效
    let (pair_resp, _) = rpc_via_ws(
        &mut ws,
        "req-pair",
        RpcCommand::CreatePairingOffer {
            transport: PairingTransportMode::LanDirect,
        },
    )
    .await;
    let code_snap = match pair_resp.result.expect("pairing offer") {
        RpcResultData::PairingOfferCreated { code, .. } => code,
        other => panic!("unexpected: {other:?}"),
    };
    assert!(code_snap.offer_url.contains("#offer="));

    let claim_body = serde_json::to_string(&RpcRequest {
        request_id: "http-claim".to_owned(),
        command: RpcCommand::ClaimPairing(ClaimPairingRequest {
            device_name: "测试手机".to_owned(),
            platform: "iOS".to_owned(),
            credential: PairingCredentialInput::SixDigitCode(code_snap.six_digit_code.clone()),
        }),
    })
    .expect("serialize claim");

    let (st_claim, body_claim) =
        http_request(addr, "POST", "/api/rpc", None, Some(&claim_body)).await;
    assert_eq!(st_claim, 200);
    let claim_rpc: RpcResponse = serde_json::from_str(&body_claim).expect("parse claim resp");
    let claimed = match claim_rpc.result.expect("claimed") {
        RpcResultData::PairingClaimed(c) => c,
        other => panic!("unexpected: {other:?}"),
    };

    // 同一配对码禁止重放
    let (_, replay_body) = http_request(addr, "POST", "/api/rpc", None, Some(&claim_body)).await;
    let replay_rpc: RpcResponse = serde_json::from_str(&replay_body).expect("replay resp");
    assert!(!replay_rpc.ok);
    assert_eq!(
        replay_rpc.error.expect("replay err").code,
        ErrorCode::PairingExpired
    );

    // 持 Bearer 令牌拉取快照成功
    let (st_auth_ok, _) = http_request(
        addr,
        "GET",
        "/api/snapshot",
        Some(&claimed.bearer_token),
        None,
    )
    .await;
    assert_eq!(st_auth_ok, 200);

    // 吊销设备后 Bearer 令牌立即 401
    let (rev_resp, _) = rpc_via_ws(
        &mut ws,
        "req-revoke",
        RpcCommand::RevokePairedDevice {
            device_id: claimed.device_id.clone(),
        },
    )
    .await;
    assert!(rev_resp.ok);

    let (st_revoked, _) = http_request(
        addr,
        "GET",
        "/api/snapshot",
        Some(&claimed.bearer_token),
        None,
    )
    .await;
    assert_eq!(st_revoked, 401);

    // 6. 断线重连携带 after_seq 增量追平并以 CatchUpCompleted 收尾
    let (mut ws_catchup, _) = connect_ws(
        addr,
        ClientHello {
            client_kind: ClientKind::MobilePwa,
            client_version: "0.1.0".to_owned(),
            protocol_version: soxu_protocol::PROTOCOL_VERSION,
            after_seq: Some(seq_before_pairing),
            device_id: None,
        },
    )
    .await;

    let mut replayed = Vec::new();
    loop {
        let raw = next_text_frame(&mut ws_catchup).await;
        let env: EventEnvelope<DaemonEvent> =
            serde_json::from_str(&raw).expect("catchup event envelope");
        if matches!(env.payload, DaemonEvent::CatchUpCompleted { .. }) {
            assert!(
                replayed.len() >= 2,
                "expected >=2 replayed events, got {}",
                replayed.len()
            );
            break;
        }
        assert!(env.seq > seq_before_pairing);
        replayed.push(env);
    }

    let expected_latest_seq = state.latest_seq().expect("latest seq");
    let expected_fp = state.daemon_fingerprint().to_owned();
    drop(ws);
    drop(ws_catchup);
    handle.abort();
    drop(state);

    // 7. 使用同一 TempDir 重建 DaemonState，验证 SQLite 100% 持久恢复
    let reopened =
        DaemonState::open(Some(dir.path()), "http://127.0.0.1:7698", false).expect("reopen state");
    assert_eq!(reopened.daemon_fingerprint(), expected_fp);
    assert_eq!(
        reopened.latest_seq().expect("reopened seq"),
        expected_latest_seq
    );
    let restored_snap = reopened.snapshot().expect("reopened snapshot");
    assert_eq!(restored_snap.quick_chats.len(), 1);
    assert_eq!(restored_snap.sessions.len(), 2);
    let msgs = reopened
        .list_session_messages(&sess.session_id)
        .expect("reopened messages");
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[1].steer_badge.as_deref(), Some("中途纠偏 · 已注入"));
}
