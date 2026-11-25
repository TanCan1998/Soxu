//! `soxu-daemon` HTTP / WebSocket 端到端集成测试（真实 TCP 监听 + SQLite 持久恢复）。

use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use soxu_daemon::{DaemonState, build_router};
use soxu_protocol::{
    ActiveTurnBehavior, ApprovalItemSnapshot, ApprovalResolutionKind, AskQuestionOption,
    ClaimPairingRequest, ClientHello, ClientKind, DaemonEvent, DraftSnapshot, ErrorCode,
    EventEnvelope, PairingCredentialInput, PairingTransportMode, PendingUserInputAnswer,
    PendingUserInputAnswerItem, PendingUserInputQuestion, PendingUserInputRequest, PermissionLevel,
    RpcCommand, RpcRequest, RpcResponse, RpcResultData, ServerHello, SessionTarget, SubagentStatus,
    SubagentSummary, ThinkingEffort,
};
use std::sync::Arc;
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
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral test port");
    let addr = listener.local_addr().expect("read local addr");
    let endpoint = format!("http://{addr}");
    let state =
        Arc::new(DaemonState::open(Some(dir.path()), &endpoint, false).expect("open daemon state"));
    let router = build_router(Arc::clone(&state));
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    (addr, state, handle)
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
