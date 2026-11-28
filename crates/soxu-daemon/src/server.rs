//! Axum HTTP 与 WebSocket 双向 RPC / `afterSeq` 增量追平服务端。

use crate::state::{CATCH_UP_PAGE_SIZE, DaemonState};
use crate::web_assets::build_web_assets_router;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode, header, uri::Authority};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use soxu_protocol::{
    ActionableError, ClientHello, DaemonEvent, ErrorCode, EventEnvelope, HealthSnapshot,
    PROTOCOL_VERSION, RpcCommand, RpcRequest, RpcResponse, RpcResultData, ServerHello,
};
use std::{net::IpAddr, sync::Arc};
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
    build_web_assets_router().merge(
        Router::new()
            .route("/api/health", get(health_handler))
            .route("/api/snapshot", get(snapshot_handler))
            .route("/api/events", get(events_handler))
            .route("/api/rpc", post(rpc_handler))
            .route("/ws", get(ws_handler))
            .with_state(state),
    )
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

fn local_origin_allowed(state: &DaemonState, headers: &HeaderMap) -> bool {
    if !state.has_local_native_turns() {
        return true;
    }
    let Some(host) = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<Authority>().ok())
    else {
        return false;
    };
    let name = host.host().trim_matches(['[', ']']);
    let loopback = name.eq_ignore_ascii_case("localhost")
        || name.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback());
    loopback
        && headers
            .get(header::ORIGIN)
            .is_none_or(|origin| origin.as_bytes() == format!("http://{host}").as_bytes())
}

fn origin_error() -> ActionableError {
    ActionableError::new(
        ErrorCode::Unauthorized,
        "请求来源与本机回环地址不匹配",
        "已拒绝此连接",
        "请使用本机 Soxu 客户端或从同一回环地址连接",
    )
}

async fn health_handler(State(state): State<Arc<DaemonState>>) -> Json<HealthSnapshot> {
    Json(state.health())
}

async fn snapshot_handler(State(state): State<Arc<DaemonState>>, headers: HeaderMap) -> Response {
    if !local_origin_allowed(&state, &headers) {
        return (StatusCode::FORBIDDEN, Json(origin_error())).into_response();
    }
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
    if !local_origin_allowed(&state, &headers) {
        return (StatusCode::FORBIDDEN, Json(origin_error())).into_response();
    }
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
    if !local_origin_allowed(&state, &headers) {
        return (
            StatusCode::FORBIDDEN,
            Json(RpcResponse::err(req.request_id, origin_error())),
        )
            .into_response();
    }
    let is_claim = matches!(req.command, RpcCommand::ClaimPairing(_));
    if let Err((status, err)) = authorize_request(&state, &headers, None, true)
        && !is_claim
    {
        return (status, Json(RpcResponse::err(req.request_id, err.0))).into_response();
    }
    let resp = state.handle_transport_rpc(req);
    (StatusCode::OK, Json(resp)).into_response()
}

async fn ws_handler(
    State(state): State<Arc<DaemonState>>,
    headers: HeaderMap,
    Query(query): Query<WsQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    if !local_origin_allowed(&state, &headers) {
        return (StatusCode::FORBIDDEN, Json(origin_error())).into_response();
    }
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

/// 单轮追平页预算：一轮最多取这么多页，轮与轮之间让出执行权。
///
/// 预算用尽并不是截断：`catch_up_to_head` 从最后已发送序号续读，
/// 直到覆盖固定头部 `H` 才返回完成。
const CATCH_UP_PAGES_PER_ROUND: u32 = 8;

/// 追平结果：只有覆盖固定头部 `H` 才算完成。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CatchUpOutcome {
    Completed { to_seq: u64, count: u64 },
    Aborted,
}

fn catch_up_failure(
    stage: &str,
    after_seq: u64,
    head_seq: u64,
    cause: Option<&ActionableError>,
) -> ActionableError {
    let error = ActionableError::new(
        ErrorCode::CatchUpFailed,
        format!("追平事件读取失败（阶段 {stage}，after_seq={after_seq}，head_seq={head_seq}）"),
        "本次追平已中止，客户端将收不到 CatchUpCompleted，事件流存在缺口",
        "请重新连接；守护进程会重新握手取快照并重新分页追平",
    );
    match cause {
        Some(cause) => error.with_diagnostic(format!(
            "{}；{}",
            cause.summary_zh,
            cause.diagnostic_detail.as_deref().unwrap_or("无底层细节")
        )),
        None => error,
    }
}

async fn send_catch_up_error(socket: &mut WebSocket, seq: u64, error: ActionableError) -> bool {
    let envelope = EventEnvelope::new(
        seq,
        DaemonEvent::ErrorRaised {
            session_id: None,
            error,
        },
    );
    send_json(socket, &envelope).await
}

/// 从 `after_seq` 分页追平到固定头部 `head_seq`，逐页读取、逐页发送。
///
/// 语义对照 t3code `OrchestrationEventStore.catchUpApplicationEvents`：分页以固定的
/// `throughSequence` 为上界，到达上界即终止；每次只驻留一页事件，因此内存开销只与
/// 单页大小相关，而与积压总量无关。与本仓差异：页读取失败或出现空洞时发出可诊断
/// 错误并中止，而不是像参考实现那样由上层重取快照。
async fn catch_up_to_head(
    state: &DaemonState,
    socket: &mut WebSocket,
    after_seq: u64,
    head_seq: u64,
) -> CatchUpOutcome {
    let mut last_sent_seq = after_seq;
    let mut sent: u64 = 0;
    loop {
        if last_sent_seq >= head_seq {
            return CatchUpOutcome::Completed {
                to_seq: head_seq,
                count: sent,
            };
        }
        let mut pages_this_round = 0;
        while pages_this_round < CATCH_UP_PAGES_PER_ROUND && last_sent_seq < head_seq {
            pages_this_round += 1;
            let page = match state.events_page(last_sent_seq, CATCH_UP_PAGE_SIZE) {
                Ok(page) => page,
                Err(cause) => {
                    let error =
                        catch_up_failure("page_query", last_sent_seq, head_seq, Some(&cause));
                    let _ = send_catch_up_error(socket, last_sent_seq, error).await;
                    return CatchUpOutcome::Aborted;
                }
            };
            let mut consumed = 0usize;
            for env in page {
                if env.seq > head_seq {
                    break;
                }
                last_sent_seq = env.seq;
                sent += 1;
                consumed += 1;
                if !send_json(socket, &env).await {
                    return CatchUpOutcome::Aborted;
                }
            }
            if consumed == 0 {
                // 头部之前出现空洞（日志被裁切或已不连续）：绝不静默当作追平完成，
                // 也绝不拿同一游标反复重读而空转。
                let error = catch_up_failure("page_gap", last_sent_seq, head_seq, None);
                let _ = send_catch_up_error(socket, last_sent_seq, error).await;
                return CatchUpOutcome::Aborted;
            }
        }
        // 单轮页预算用尽仍未覆盖头部：让出执行权后从最后已发送序号续读。
        tokio::task::yield_now().await;
    }
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
    // 快照与头部在同一次握手内固定：先读快照、后取头部，保证 `seq <= H` 的写入
    // 要么已在快照内、要么由 `[after_seq, H]` 的追平重放覆盖。
    let (snapshot, head_seq) = match state.snapshot_with_head() {
        Ok(pair) => pair,
        Err(cause) => {
            let mut error = ActionableError::new(
                ErrorCode::CatchUpFailed,
                "握手快照读取失败",
                "实时同步连接即将关闭",
                "请稍后重新连接；若持续失败请检查守护进程数据目录",
            );
            if let Some(detail) = cause.diagnostic_detail {
                error = error.with_diagnostic(detail);
            } else {
                error = error.with_diagnostic(cause.summary_zh);
            }
            let _ = send_json(&mut socket, &RpcResponse::err("hello", error)).await;
            return;
        }
    };

    let server_hello = ServerHello {
        daemon_version: env!("CARGO_PKG_VERSION").to_owned(),
        daemon_fingerprint: state.daemon_fingerprint().to_owned(),
        protocol_version: PROTOCOL_VERSION,
        magpie_detected: state.magpie_detected(),
        snapshot_seq: Some(head_seq),
        latest_seq: head_seq,
        snapshot,
    };
    if !send_json(&mut socket, &server_hello).await {
        return;
    }

    // 3. 按 after_seq 分页追平到固定头部 H，只有覆盖 H 才下发 CatchUpCompleted
    let mut last_sent_seq = head_seq;
    if let Some(after_seq) = hello.after_seq {
        // 游标超前（事件库被重置或换了库）时按 t3code `afterSequence > highWater` 的处理
        // 重取快照语义：从头重放全部事件，而不是让客户端停在无效游标上。
        let replay_from = if after_seq > head_seq { 0 } else { after_seq };
        match catch_up_to_head(&state, &mut socket, replay_from, head_seq).await {
            CatchUpOutcome::Completed { to_seq, count } => {
                last_sent_seq = to_seq;
                let catchup_evt = EventEnvelope::new(
                    to_seq,
                    DaemonEvent::CatchUpCompleted {
                        from_seq: replay_from,
                        to_seq,
                        count,
                    },
                );
                if !send_json(&mut socket, &catchup_evt).await {
                    return;
                }
            }
            CatchUpOutcome::Aborted => return,
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
                                let resp = state.handle_transport_rpc(req);
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
                    Err(RecvError::Lagged(skipped)) => {
                        // 广播环落后（`Lagged(n)`：接收器已被移到环内最老项）：
                        // 复用同一分页函数从最后已发送序号追到当前头部，再恢复广播流转。
                        let head_seq = match state.latest_seq() {
                            Ok(head_seq) => head_seq,
                            Err(cause) => {
                                let error = catch_up_failure(
                                    "lagged_head_query",
                                    last_sent_seq,
                                    last_sent_seq,
                                    Some(&cause),
                                );
                                let _ = send_catch_up_error(&mut socket, last_sent_seq, error).await;
                                break;
                            }
                        };
                        if head_seq > last_sent_seq {
                            let from_seq = last_sent_seq;
                            match catch_up_to_head(&state, &mut socket, from_seq, head_seq).await {
                                CatchUpOutcome::Completed { to_seq, count } => {
                                    last_sent_seq = to_seq;
                                    let catchup_evt = EventEnvelope::new(
                                        to_seq,
                                        DaemonEvent::CatchUpCompleted {
                                            from_seq,
                                            to_seq,
                                            count,
                                        },
                                    );
                                    if !send_json(&mut socket, &catchup_evt).await {
                                        break;
                                    }
                                }
                                CatchUpOutcome::Aborted => break,
                            }
                            tracing::debug!(skipped, "广播环落后已按分页追平到当前头部");
                        }
                    }
                    Err(RecvError::Closed) => break,
                }
            }
        }
    }
}
