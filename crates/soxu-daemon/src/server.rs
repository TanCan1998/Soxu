//! Axum HTTP 与 WebSocket 双向 RPC / `afterSeq` 增量追平服务端。

use crate::state::DaemonState;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use soxu_protocol::{
    ActionableError, ClientHello, DaemonEvent, ErrorCode, EventEnvelope, HealthSnapshot,
    PROTOCOL_VERSION, RpcCommand, RpcRequest, RpcResponse, RpcResultData, ServerHello,
};
use std::sync::Arc;
use tokio::sync::broadcast::error::RecvError;

#[derive(Debug, Deserialize)]
pub struct EventsQuery {
    pub after_seq: Option<u64>,
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct WsQuery {
    pub token: Option<String>,
}

/// 构建完整守护进程 HTTP + WebSocket 路由表。
pub fn build_router(state: Arc<DaemonState>) -> Router {
    Router::new()
        .route("/api/health", get(health_handler))
        .route("/api/snapshot", get(snapshot_handler))
        .route("/api/events", get(events_handler))
        .route("/api/rpc", post(rpc_handler))
        .route("/ws", get(ws_handler))
        .with_state(state)
}

fn extract_bearer(headers: &HeaderMap, query_token: Option<&str>) -> Option<String> {
    if let Some(auth) = headers.get(header::AUTHORIZATION)
        && let Ok(val) = auth.to_str()
        && let Some(stripped) = val
            .strip_prefix("Bearer ")
            .or_else(|| val.strip_prefix("bearer "))
    {
        return Some(stripped.trim().to_owned());
    }
    query_token.map(str::trim).map(ToOwned::to_owned)
}

fn authorize_request(
    state: &DaemonState,
    headers: &HeaderMap,
    query_token: Option<&str>,
    allow_unauthenticated_local: bool,
) -> Result<(), (StatusCode, Json<ActionableError>)> {
    if let Some(token) = extract_bearer(headers, query_token) {
        state
            .authenticate_bearer(&token)
            .map(|_| ())
            .map_err(|err| (StatusCode::UNAUTHORIZED, Json(err)))
    } else if state.require_auth_for_all() || !allow_unauthenticated_local {
        Err((
            StatusCode::UNAUTHORIZED,
            Json(ActionableError::new(
                ErrorCode::Unauthorized,
                "缺少设备访问令牌",
                "当前接口禁止匿名访问",
                "请先完成扫码或 6 位配对码认证并携带 Bearer 令牌",
            )),
        ))
    } else {
        Ok(())
    }
}

async fn health_handler(State(state): State<Arc<DaemonState>>) -> Json<HealthSnapshot> {
    Json(state.health())
}

async fn snapshot_handler(State(state): State<Arc<DaemonState>>, headers: HeaderMap) -> Response {
    if let Err((status, err)) = authorize_request(&state, &headers, None, true) {
        return (status, err).into_response();
    }
    match state.snapshot() {
        Ok(snap) => (StatusCode::OK, Json(snap)).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, Json(err)).into_response(),
    }
}

async fn events_handler(
    State(state): State<Arc<DaemonState>>,
    headers: HeaderMap,
    Query(query): Query<EventsQuery>,
) -> Response {
    if let Err((status, err)) = authorize_request(&state, &headers, None, true) {
        return (status, err).into_response();
    }
    let after_seq = query.after_seq.unwrap_or(0);
    let limit = query.limit.unwrap_or(500);
    match state.events_after(after_seq, limit) {
        Ok((events, latest_seq)) => (
            StatusCode::OK,
            Json(RpcResultData::Events { events, latest_seq }),
        )
            .into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, Json(err)).into_response(),
    }
}

async fn rpc_handler(
    State(state): State<Arc<DaemonState>>,
    headers: HeaderMap,
    Json(req): Json<RpcRequest>,
) -> Response {
    let is_claim = matches!(req.command, RpcCommand::ClaimPairing(_));
    if let Err((status, err)) = authorize_request(&state, &headers, None, true)
        && !is_claim
    {
        return (status, Json(RpcResponse::err(req.request_id, err.0))).into_response();
    }
    let resp = state.handle_rpc(req);
    (StatusCode::OK, Json(resp)).into_response()
}

async fn ws_handler(
    State(state): State<Arc<DaemonState>>,
    headers: HeaderMap,
    Query(query): Query<WsQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    let bearer_opt = extract_bearer(&headers, query.token.as_deref());
    let authed_device = if let Some(ref token) = bearer_opt {
        match state.authenticate_bearer(token) {
            Ok(dev) => Some(dev.device_id),
            Err(err) => return (StatusCode::UNAUTHORIZED, Json(err)).into_response(),
        }
    } else if state.require_auth_for_all() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ActionableError::new(
                ErrorCode::Unauthorized,
                "WebSocket 连接缺少设备认证令牌",
                "已阻止未配对连接",
                "请先扫码或输入配对码获取令牌",
            )),
        )
            .into_response();
    } else {
        None
    };

    ws.on_upgrade(move |socket| handle_ws_socket(state, socket, authed_device))
}

async fn send_json<T: serde::Serialize>(socket: &mut WebSocket, val: &T) -> bool {
    let Ok(raw) = serde_json::to_string(val) else {
        return false;
    };
    socket.send(Message::Text(raw.into())).await.is_ok()
}

async fn handle_ws_socket(
    state: Arc<DaemonState>,
    mut socket: WebSocket,
    authed_device_id: Option<String>,
) {
    // 1. 等待 ClientHello 握手
    let Some(Ok(Message::Text(hello_raw))) = socket.recv().await else {
        return;
    };

    let Ok(hello) = serde_json::from_str::<ClientHello>(hello_raw.as_str()) else {
        let err = ActionableError::new(
            ErrorCode::ValidationFailed,
            "无效的 WebSocket 握手帧",
            "连接即将关闭",
            "请发送符合 ClientHello 结构的 JSON 握手包",
        );
        let _ = send_json(&mut socket, &RpcResponse::err("hello", err)).await;
        return;
    };

    if hello.protocol_version != PROTOCOL_VERSION {
        let err = ActionableError::new(
            ErrorCode::ProtocolMismatch,
            format!(
                "客户端协议版本 v{} 与守护进程 v{} 不兼容",
                hello.protocol_version, PROTOCOL_VERSION
            ),
            "实时同步连接已拒绝",
            "请升级客户端或守护进程至相同协议版本",
        );
        let _ = send_json(&mut socket, &RpcResponse::err("hello", err)).await;
        return;
    }

    if let Some(dev_id) = hello.device_id.as_ref().or(authed_device_id.as_ref())
        && let Err(err) = state.verify_device_not_revoked(dev_id)
    {
        let _ = send_json(&mut socket, &RpcResponse::err("hello", err)).await;
        return;
    }

    // 2. 先订阅广播通道，避免快照与追平窗口内漏事件
    let mut rx = state.subscribe_events();
    let Ok(snapshot) = state.snapshot() else {
        return;
    };
    let Ok(latest_seq) = state.latest_seq() else {
        return;
    };

    let server_hello = ServerHello {
        daemon_version: env!("CARGO_PKG_VERSION").to_owned(),
        daemon_fingerprint: state.daemon_fingerprint().to_owned(),
        protocol_version: PROTOCOL_VERSION,
        magpie_detected: state.magpie_detected(),
        latest_seq,
        snapshot,
    };
    if !send_json(&mut socket, &server_hello).await {
        return;
    }

    // 3. 按 after_seq 增量追平并下发 CatchUpCompleted 摘要事件
    let mut last_sent_seq = latest_seq;
    if let Some(after_seq) = hello.after_seq {
        last_sent_seq = after_seq;
        if let Ok((missed, _)) = state.events_after(after_seq, 1000) {
            let count = u64::try_from(missed.len()).unwrap_or(0);
            for env in missed {
                last_sent_seq = env.seq;
                if !send_json(&mut socket, &env).await {
                    return;
                }
            }
            let catchup_evt = EventEnvelope::new(
                last_sent_seq,
                DaemonEvent::CatchUpCompleted {
                    from_seq: after_seq,
                    to_seq: last_sent_seq,
                    count,
                },
            );
            if !send_json(&mut socket, &catchup_evt).await {
                return;
            }
        }
    }

    // 4. 全双工：上行 RPC 请求 / 下行增量事件
    loop {
        tokio::select! {
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Text(raw))) => {
                        match serde_json::from_str::<RpcRequest>(raw.as_str()) {
                            Ok(req) => {
                                let resp = state.handle_rpc(req);
                                if !send_json(&mut socket, &resp).await {
                                    break;
                                }
                            }
                            Err(parse_err) => {
                                let err = ActionableError::new(
                                    ErrorCode::ValidationFailed,
                                    "无法解析 RPC 请求",
                                    "该指令未被执行",
                                    "请检查 JSON 请求格式与字段类型",
                                )
                                .with_diagnostic(parse_err.to_string());
                                if !send_json(&mut socket, &RpcResponse::err("", err)).await {
                                    break;
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Ping(payload))) => {
                        if socket.send(Message::Pong(payload)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(_)) => break,
                    _ => {}
                }
            }
            broadcast_msg = rx.recv() => {
                match broadcast_msg {
                    Ok(env) => {
                        if env.seq > last_sent_seq {
                            last_sent_seq = env.seq;
                            if !send_json(&mut socket, &env).await {
                                break;
                            }
                        }
                    }
                    Err(RecvError::Lagged(_)) => {
                        if let Ok((missed, _)) = state.events_after(last_sent_seq, 500) {
                            for env in missed {
                                if env.seq > last_sent_seq {
                                    last_sent_seq = env.seq;
                                    if !send_json(&mut socket, &env).await {
                                        return;
                                    }
                                }
                            }
                        }
                    }
                    Err(RecvError::Closed) => break,
                }
            }
        }
    }
}
