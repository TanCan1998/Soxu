//! M6 端到端接线回归：`RpcCommand` 的脚本 / 调度 / PTY 变体经守护进程
//! 能力门控与三域服务（`ScriptService` / `ScheduleService` / `PtyHost`）落地。
//!
//! 覆盖 Issue #150 要求的四类证据：
//! 1. 能力位缺省关闭时 14 个变体一律 `Unsupported` 三要素且零事件；
//! 2. 能力声明后 14 个变体全部真实路由（不再有静默空实现）；
//! 3. 脚本生命周期、PTY 分片、调度触发/结果经既有信封广播且 `seq` 严格单调；
//! 4. 事件只经订阅通道下发，订阅前已发生的事件不回放给新订阅者。
//!
//! 夹具红线（L-017）：脚本子进程桩一律用**当前测试二进制自执行**，PTY 用内存假后端，
//! 不依赖宿主预装的 node / python / 真实 shell 等第三方工具。

use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use soxu_daemon::{
    DaemonState, FeatureGate, FinishOutcome, PtyBackend, PtyChild, PtyControl, PtySpawnSpec,
    build_router,
};
use soxu_protocol::{
    ActionableError, ClientHello, ClientKind, CreateScheduleRequest, DaemonEvent, DaemonFeature,
    DeleteScheduleRequest, ErrorCode, EventEnvelope, IdempotencyKey, InspectScriptRequest,
    ListSchedulesRequest, ListScriptsRequest, ManagedScriptEntry, OpenPtyRequest, PROTOCOL_VERSION,
    PTY_OUTPUT_CHUNK_MAX_BYTES, PauseScheduleRequest, ProjectSummary, PtyCloseReason,
    PtyCommandResult, PtyEvent, PtyExitStatus, PtyInputData, PtyOutputChunk, PtySessionId,
    PtySessionState, PtySize, PtyWorkdirPolicy, ResizePtyRequest, RpcCommand, RpcRequest,
    RpcResponse, RpcResultData, RunScriptRequest, ScheduleCadence, ScheduleCommandResult,
    ScheduleEvent, ScheduleId, ScheduleName, ScheduleRetryPolicy, ScheduleRunOutcome,
    ScheduleRunResult, ScheduleTrigger, ScheduleValidityWindow, ScriptCommandResult, ScriptId,
    ScriptLifecycleEvent, ScriptLifecyclePhase, ScriptRunState, ScriptUserArguments,
    StopScriptRequest, TriggerScheduleRequest, UpdateScheduleRequest, WorkspaceBucket,
    WorkspaceIsolation, WorkspaceSummary, WritePtyRequest,
};
use std::io::Cursor;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio_tungstenite::tungstenite::Message as WsMsg;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

const WORKSPACE_ID: &str = "wks_m6_wiring";
const EVENT_TIMEOUT: Duration = Duration::from_secs(60);

type WsClient = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

// ---------------------------------------------------------------------------
// 夹具：守护进程、工作区登记、受管脚本清单、假 PTY 后端
// ---------------------------------------------------------------------------

fn open_state(data_dir: Option<&TempDir>) -> Arc<DaemonState> {
    Arc::new(
        DaemonState::open(data_dir.map(TempDir::path), "http://127.0.0.1:0", false)
            .expect("打开测试守护进程"),
    )
}

/// 装配带指定能力位与假 PTY 后端的守护进程。
fn open_state_with(
    data_dir: Option<&TempDir>,
    features: impl IntoIterator<Item = DaemonFeature>,
    output: Vec<u8>,
) -> Arc<DaemonState> {
    let state = DaemonState::open(data_dir.map(TempDir::path), "http://127.0.0.1:0", false)
        .expect("打开测试守护进程")
        .with_feature_gate(FeatureGate::from_features(features))
        .expect("注入 M6 能力门控")
        .with_pty_backend(Arc::new(FixedOutputBackend::new(output)));
    Arc::new(state)
}

fn register_workspace(state: &DaemonState, root: &Path) -> String {
    let display = root.to_string_lossy().into_owned();
    state
        .upsert_project(ProjectSummary {
            project_id: "prj_m6_wiring".to_owned(),
            name: "M6 接线夹具项目".to_owned(),
            display_path: "~/project-fixture".to_owned(),
            root_path: display.clone(),
            is_git_repo: false,
            default_branch: None,
            aggregated_bucket: WorkspaceBucket::Done,
            workspace_count: 1,
        })
        .expect("登记夹具项目");
    state
        .upsert_workspace(WorkspaceSummary {
            workspace_id: WORKSPACE_ID.to_owned(),
            project_id: "prj_m6_wiring".to_owned(),
            project_name: "M6 接线夹具项目".to_owned(),
            display_path: display,
            branch: "main".to_owned(),
            isolation: WorkspaceIsolation::Local,
            agent_id: "codex".to_owned(),
            model_id: "codex-default".to_owned(),
            bucket: WorkspaceBucket::Done,
            summary: String::new(),
            diff_stat: String::new(),
            label: None,
            archived: false,
            needs_recovery: false,
            recovery_reason: None,
            updated_at: Utc::now(),
        })
        .expect("登记夹具工作区");
    WORKSPACE_ID.to_owned()
}

/// 当前测试二进制路径（作为受控子进程桩）。
///
/// 协议要求 `program` 不得含空白与 shell 元字符；工作树或用户名含空格时，
/// 先把测试二进制复制到无空白的临时目录再执行。
fn stub_program() -> String {
    let current = std::env::current_exe().expect("当前测试二进制路径");
    if program_is_acceptable(&current) {
        return current.to_string_lossy().into_owned();
    }
    let fallback_dir = std::env::temp_dir().join("soxu-m6-wiring-stub");
    std::fs::create_dir_all(&fallback_dir).expect("创建桩目录");
    let target = fallback_dir.join(if cfg!(windows) {
        "soxu-m6-wiring-stub.exe"
    } else {
        "soxu-m6-wiring-stub"
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

/// 构造一条受管脚本条目：程序固定为当前测试二进制，固定参数选中具体桩用例。
fn script_entry(helper: &str) -> ManagedScriptEntry {
    let raw = serde_json::json!({
        "script_id": "wiring-echo",
        "display_name": "接线回归脚本",
        "program": stub_program(),
        "argument_policy": {
            "fixed_arguments": ["--exact", helper, "--ignored", "--nocapture"],
            "accepts_user_arguments": false,
            "max_user_arguments": 0,
            "max_user_argument_bytes": 4096
        },
        "workdir": { "kind": "workspace_root" },
        "risk": "low"
    });
    serde_json::from_value(raw).expect("脚本条目应合法")
}

fn write_manifest(root: &Path, entries: &[&ManagedScriptEntry]) {
    let raw = serde_json::json!({ "scripts": entries });
    std::fs::write(
        root.join("paseo.json"),
        serde_json::to_vec_pretty(&raw).expect("清单序列化"),
    )
    .expect("写入清单");
}

/// 内存假 PTY 后端：派生即吐固定输出并立即 EOF，不依赖真实 shell（L-017）。
struct FixedOutputBackend {
    output: Vec<u8>,
}

impl FixedOutputBackend {
    fn new(output: Vec<u8>) -> Self {
        Self { output }
    }
}

impl PtyBackend for FixedOutputBackend {
    fn spawn(&self, _spec: &PtySpawnSpec) -> Result<PtyChild, ActionableError> {
        Ok(PtyChild {
            reader: Box::new(Cursor::new(self.output.clone())),
            writer: Box::new(std::io::sink()),
            control: Arc::new(FixedControl),
        })
    }
}

struct FixedControl;

impl PtyControl for FixedControl {
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

fn run_script_request() -> RunScriptRequest {
    RunScriptRequest {
        workspace_id: WORKSPACE_ID.to_owned(),
        script_id: ScriptId::try_from("wiring-echo".to_owned()).expect("脚本标识"),
        user_arguments: ScriptUserArguments::default(),
        idempotency_key: None,
        approval_id: None,
    }
}

fn open_pty_request() -> OpenPtyRequest {
    OpenPtyRequest {
        workspace_id: WORKSPACE_ID.to_owned(),
        size: PtySize::new(80, 24).expect("合法尺寸"),
        workdir: PtyWorkdirPolicy::WorkspaceRoot,
        env_policy: Default::default(),
    }
}

fn interval_schedule_request(every_seconds: u32, key: &str) -> CreateScheduleRequest {
    CreateScheduleRequest {
        workspace_id: WORKSPACE_ID.to_owned(),
        name: ScheduleName::try_from("接线巡检".to_owned()).expect("调度名称"),
        cadence: ScheduleCadence::Interval { every_seconds },
        trigger: ScheduleTrigger {
            agent_id: "codex".to_owned(),
            model_id: None,
            permission: None,
            text: "检查当前状态".to_owned(),
        },
        validity: ScheduleValidityWindow::default(),
        retry: ScheduleRetryPolicy::default(),
        idempotency_key: IdempotencyKey::try_from(key.to_owned()).expect("幂等键"),
        paused: false,
    }
}

fn pty_id() -> PtySessionId {
    PtySessionId::try_from("pty_00000001".to_owned()).expect("会话标识")
}

fn schedule_id() -> ScheduleId {
    ScheduleId::try_from("sch_probe".to_owned()).expect("调度标识")
}

fn schedule_run_result(schedule_id: &ScheduleId, run_id: &str) -> ScheduleRunResult {
    ScheduleRunResult {
        schedule_id: schedule_id.clone(),
        run_id: run_id.to_owned(),
        outcome: ScheduleRunOutcome::Succeeded,
        attempts: 1,
        session_id: None,
        error: None,
        finished_at: Utc::now(),
    }
}

/// 14 个 M6 变体的完整样本；工作区与运行标识刻意都不存在，
/// 用来证明能力门控发生在工作区解析之前而不是之后。
fn m6_commands() -> Vec<RpcCommand> {
    vec![
        RpcCommand::ListScripts(ListScriptsRequest {
            workspace_id: WORKSPACE_ID.to_owned(),
        }),
        RpcCommand::RunScript(run_script_request()),
        RpcCommand::StopScript(StopScriptRequest {
            run_id: "run_missing".to_owned(),
            force: true,
        }),
        RpcCommand::InspectScript(InspectScriptRequest {
            run_id: "run_missing".to_owned(),
        }),
        RpcCommand::ListSchedules(ListSchedulesRequest {
            workspace_id: WORKSPACE_ID.to_owned(),
        }),
        RpcCommand::CreateSchedule(interval_schedule_request(3_600, "m6-create")),
        RpcCommand::UpdateSchedule(UpdateScheduleRequest {
            schedule_id: schedule_id(),
            name: None,
            cadence: None,
            trigger: None,
            validity: None,
            retry: None,
        }),
        RpcCommand::DisableSchedule(PauseScheduleRequest {
            schedule_id: schedule_id(),
            paused: true,
        }),
        RpcCommand::DeleteSchedule(DeleteScheduleRequest {
            schedule_id: schedule_id(),
        }),
        RpcCommand::TriggerSchedule(TriggerScheduleRequest {
            schedule_id: schedule_id(),
            idempotency_key: IdempotencyKey::try_from("m6-trigger".to_owned()).expect("幂等键"),
        }),
        RpcCommand::OpenPty(open_pty_request()),
        RpcCommand::WritePty(WritePtyRequest {
            pty_id: pty_id(),
            data: PtyInputData::try_from("echo hi".to_owned()).expect("输入"),
        }),
        RpcCommand::ResizePty(ResizePtyRequest {
            pty_id: pty_id(),
            size: PtySize::new(100, 30).expect("合法尺寸"),
        }),
        RpcCommand::ClosePty(soxu_protocol::ClosePtyRequest {
            pty_id: pty_id(),
            reason: PtyCloseReason::ClientRequest,
        }),
    ]
}

/// 每个变体在门控关闭时必须命中的能力令牌。
fn expected_token(command: &RpcCommand) -> &'static str {
    match command {
        RpcCommand::ListScripts(_)
        | RpcCommand::RunScript(_)
        | RpcCommand::StopScript(_)
        | RpcCommand::InspectScript(_) => "managed_scripts",
        RpcCommand::ListSchedules(_)
        | RpcCommand::CreateSchedule(_)
        | RpcCommand::UpdateSchedule(_)
        | RpcCommand::DisableSchedule(_)
        | RpcCommand::DeleteSchedule(_)
        | RpcCommand::TriggerSchedule(_) => "schedules",
        _ => "controlled_pty",
    }
}

fn rpc(state: &Arc<DaemonState>, request_id: &str, command: RpcCommand) -> RpcResponse {
    // `handle_rpc` 与 WS / HTTP 处理器最终走的 `handle_transport_rpc` 在本骨骼
    // （未注入原生驱动）下等价；真实传输路径另由 WebSocket 用例覆盖。
    state.handle_rpc(RpcRequest {
        request_id: request_id.to_owned(),
        command,
    })
}

fn expect_ok(response: RpcResponse) -> RpcResultData {
    assert!(
        response.ok,
        "期望成功，实际错误：{:?}",
        response.error.map(|error| error.summary_zh)
    );
    response.result.expect("成功响应必须带结果")
}

fn error_of(response: RpcResponse) -> ActionableError {
    assert!(!response.ok, "期望失败，实际成功：{:?}", response.result);
    response.error.expect("失败响应必须带三要素错误")
}

// ---------------------------------------------------------------------------
// 事件读取：订阅通道
// ---------------------------------------------------------------------------

async fn next_event(
    rx: &mut broadcast::Receiver<EventEnvelope<DaemonEvent>>,
    mut matches: impl FnMut(&DaemonEvent) -> bool,
) -> EventEnvelope<DaemonEvent> {
    tokio::time::timeout(EVENT_TIMEOUT, async {
        loop {
            match rx.recv().await {
                Ok(envelope) => {
                    if matches(&envelope.payload) {
                        return envelope;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => panic!("事件广播通道已关闭"),
            }
        }
    })
    .await
    .expect("在限期内收到预期事件")
}

fn lifecycle_of(payload: &DaemonEvent) -> Option<ScriptLifecycleEvent> {
    match payload {
        DaemonEvent::ScriptLifecycleChanged(event) => Some(event.clone()),
        _ => None,
    }
}

fn pty_event_of(payload: &DaemonEvent) -> Option<PtyEvent> {
    match payload {
        DaemonEvent::PtyActivity(event) => Some(event.clone()),
        _ => None,
    }
}

fn schedule_event_of(payload: &DaemonEvent) -> Option<ScheduleEvent> {
    match payload {
        DaemonEvent::ScheduleActivity(event) => Some(event.clone()),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// 1. 能力位缺省关闭：14 个变体一律拒绝且零事件
// ---------------------------------------------------------------------------

#[tokio::test]
async fn capabilities_off_reject_all_m6_variants_without_any_event() {
    let state = open_state(None);
    let baseline = state.latest_seq().expect("基线事件序号");

    for (index, command) in m6_commands().into_iter().enumerate() {
        let token = expected_token(&command);
        let error = error_of(rpc(&state, &format!("off-{index}"), command));
        assert_eq!(error.code, ErrorCode::Unsupported, "{token} 关闭时必须拒绝");
        assert!(
            error.summary_zh.contains(token),
            "错误摘要必须点名 {token}：{}",
            error.summary_zh
        );
        assert!(
            !error.impact_scope_zh.is_empty() && !error.remediation_zh.is_empty(),
            "错误必须保持三要素形态"
        );
        assert_eq!(
            error.diagnostic_detail.as_deref(),
            Some("declared=[]"),
            "诊断只暴露能力令牌，不泄漏其他信息"
        );
    }

    let (events, latest) = state.events_after(baseline, 100).expect("查询事件");
    assert!(events.is_empty(), "拒绝路径不得下发任何事件：{events:?}");
    assert_eq!(latest, baseline, "事件序号不得因拒绝而推进");
}

// ---------------------------------------------------------------------------
// 2. 只声明一部分能力：其余域仍缺省关闭
// ---------------------------------------------------------------------------

#[tokio::test]
async fn partial_capability_declaration_leaves_other_domains_unsupported() {
    let data = TempDir::new().expect("数据目录");
    let root = TempDir::new().expect("工作区目录");
    let state = open_state_with(Some(&data), [DaemonFeature::ManagedScripts], b"ok".to_vec());
    let workspace_id = register_workspace(&state, root.path());

    // managed_scripts 已声明：清单查询必须放行（空清单也是合法结果）。
    let listed = expect_ok(rpc(
        &state,
        "partial-list-scripts",
        RpcCommand::ListScripts(ListScriptsRequest {
            workspace_id: workspace_id.clone(),
        }),
    ));
    match listed {
        RpcResultData::ManagedScript(ScriptCommandResult::Listed(listing)) => {
            assert!(listing.scripts.is_empty(), "未声明任何脚本时清单应为空");
            assert_eq!(listing.manifest_file, "paseo.json");
        }
        other => panic!("期望脚本清单，实际 {other:?}"),
    }

    // 调度与终端未声明：整族仍然拒绝。
    for (index, command) in [
        RpcCommand::ListSchedules(ListSchedulesRequest {
            workspace_id: workspace_id.clone(),
        }),
        RpcCommand::OpenPty(open_pty_request()),
        RpcCommand::WritePty(WritePtyRequest {
            pty_id: pty_id(),
            data: PtyInputData::try_from("x".to_owned()).expect("输入"),
        }),
    ]
    .into_iter()
    .enumerate()
    {
        let token = expected_token(&command);
        let error = error_of(rpc(&state, &format!("partial-{index}"), command));
        assert_eq!(error.code, ErrorCode::Unsupported);
        assert!(error.summary_zh.contains(token));
    }
}

// ---------------------------------------------------------------------------
// 3. 能力声明后 14 个变体全部真实路由（无静默空实现）
// ---------------------------------------------------------------------------

#[tokio::test]
async fn every_m6_variant_is_routed_when_capabilities_are_declared() {
    let data = TempDir::new().expect("数据目录");
    let root = TempDir::new().expect("工作区目录");
    let state = open_state_with(Some(&data), DaemonFeature::ALL, b"hello".to_vec());
    let workspace_id = register_workspace(&state, root.path());
    let entry = script_entry("stub_child_exits_cleanly");
    write_manifest(root.path(), &[&entry]);

    let listed = expect_ok(rpc(
        &state,
        "route-list-scripts",
        RpcCommand::ListScripts(ListScriptsRequest {
            workspace_id: workspace_id.clone(),
        }),
    ));
    match listed {
        RpcResultData::ManagedScript(ScriptCommandResult::Listed(listing)) => {
            assert_eq!(listing.scripts.len(), 1, "清单应读到一条受管脚本");
            assert_eq!(listing.scripts[0].script_id.as_str(), "wiring-echo");
        }
        other => panic!("期望脚本清单，实际 {other:?}"),
    }

    // 未知脚本 / 未知运行：路由到位后应是 NotFound（而不是 Unsupported）。
    let error = error_of(rpc(
        &state,
        "route-run-missing",
        RpcCommand::RunScript(RunScriptRequest {
            script_id: ScriptId::try_from("absent".to_owned()).expect("脚本标识"),
            ..run_script_request()
        }),
    ));
    assert_eq!(error.code, ErrorCode::NotFound, "未知脚本必须由脚本域裁决");
    let error = error_of(rpc(
        &state,
        "route-inspect-missing",
        RpcCommand::InspectScript(InspectScriptRequest {
            run_id: "run_missing".to_owned(),
        }),
    ));
    assert_eq!(error.code, ErrorCode::NotFound);
    let error = error_of(rpc(
        &state,
        "route-stop-missing",
        RpcCommand::StopScript(StopScriptRequest {
            run_id: "run_missing".to_owned(),
            force: true,
        }),
    ));
    assert_eq!(error.code, ErrorCode::NotFound);

    let created = expect_ok(rpc(
        &state,
        "route-create-schedule",
        RpcCommand::CreateSchedule(interval_schedule_request(3_600, "route-create")),
    ));
    let schedule = match created {
        RpcResultData::Schedule(ScheduleCommandResult::Created(summary)) => summary,
        other => panic!("期望调度已创建，实际 {other:?}"),
    };

    expect_ok(rpc(
        &state,
        "route-list-schedules",
        RpcCommand::ListSchedules(ListSchedulesRequest {
            workspace_id: workspace_id.clone(),
        }),
    ));
    expect_ok(rpc(
        &state,
        "route-update-schedule",
        RpcCommand::UpdateSchedule(UpdateScheduleRequest {
            schedule_id: schedule.schedule_id.clone(),
            name: Some(ScheduleName::try_from("接线巡检（已更新）".to_owned()).expect("名称")),
            cadence: None,
            trigger: None,
            validity: None,
            retry: None,
        }),
    ));
    expect_ok(rpc(
        &state,
        "route-disable-schedule",
        RpcCommand::DisableSchedule(PauseScheduleRequest {
            schedule_id: schedule.schedule_id.clone(),
            paused: true,
        }),
    ));
    // 停用状态下触发必须由调度域裁决（Conflict），证明请求真的进了域服务。
    let error = error_of(rpc(
        &state,
        "route-trigger-paused",
        RpcCommand::TriggerSchedule(TriggerScheduleRequest {
            schedule_id: schedule.schedule_id.clone(),
            idempotency_key: IdempotencyKey::try_from("route-trigger".to_owned()).expect("幂等键"),
        }),
    ));
    assert_eq!(error.code, ErrorCode::Conflict, "停用的调度不得被触发");
    expect_ok(rpc(
        &state,
        "route-enable-schedule",
        RpcCommand::DisableSchedule(PauseScheduleRequest {
            schedule_id: schedule.schedule_id.clone(),
            paused: false,
        }),
    ));
    let triggered = expect_ok(rpc(
        &state,
        "route-trigger-schedule",
        RpcCommand::TriggerSchedule(TriggerScheduleRequest {
            schedule_id: schedule.schedule_id.clone(),
            idempotency_key: IdempotencyKey::try_from("route-trigger".to_owned()).expect("幂等键"),
        }),
    ));
    match triggered {
        RpcResultData::Schedule(ScheduleCommandResult::TriggerAccepted(triggered)) => {
            assert_eq!(triggered.schedule_id, schedule.schedule_id);
        }
        other => panic!("期望触发已受理，实际 {other:?}"),
    }
    expect_ok(rpc(
        &state,
        "route-delete-schedule",
        RpcCommand::DeleteSchedule(DeleteScheduleRequest {
            schedule_id: schedule.schedule_id.clone(),
        }),
    ));

    // PTY 四条：打开 / 写入 / 调整 / 关闭全部落到受控终端域。
    let opened = expect_ok(rpc(
        &state,
        "route-open-pty",
        RpcCommand::OpenPty(open_pty_request()),
    ));
    let session = match opened {
        RpcResultData::ControlledPty(PtyCommandResult::Opened(response)) => response.session,
        other => panic!("期望会话已打开，实际 {other:?}"),
    };
    assert_eq!(session.workspace_id, workspace_id);
    let written = expect_ok(rpc(
        &state,
        "route-write-pty",
        RpcCommand::WritePty(WritePtyRequest {
            pty_id: session.pty_id.clone(),
            data: PtyInputData::try_from("echo hi".to_owned()).expect("输入"),
        }),
    ));
    match written {
        RpcResultData::ControlledPty(PtyCommandResult::Written(response)) => {
            assert_eq!(response.accepted_bytes, 7, "写入字节数必须如实回执");
        }
        other => panic!("期望写入回执，实际 {other:?}"),
    }
    expect_ok(rpc(
        &state,
        "route-resize-pty",
        RpcCommand::ResizePty(ResizePtyRequest {
            pty_id: session.pty_id.clone(),
            size: PtySize::new(100, 30).expect("合法尺寸"),
        }),
    ));
    let closed = expect_ok(rpc(
        &state,
        "route-close-pty",
        RpcCommand::ClosePty(soxu_protocol::ClosePtyRequest {
            pty_id: session.pty_id.clone(),
            reason: PtyCloseReason::ClientRequest,
        }),
    ));
    match closed {
        RpcResultData::ControlledPty(PtyCommandResult::Closed(response)) => {
            assert_eq!(response.session.pty_id, session.pty_id);
            assert!(soxu_daemon::pty::is_terminal_state(response.session.state));
        }
        other => panic!("期望关闭回执，实际 {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 4. 受管脚本：RPC 运行 + 生命周期事件 + 单调 seq
// ---------------------------------------------------------------------------

#[tokio::test]
async fn managed_script_run_over_rpc_broadcasts_lifecycle_with_monotonic_seq() {
    let data = TempDir::new().expect("数据目录");
    let root = TempDir::new().expect("工作区目录");
    let state = open_state_with(Some(&data), [DaemonFeature::ManagedScripts], Vec::new());
    let workspace_id = register_workspace(&state, root.path());
    let entry = script_entry("stub_child_exits_cleanly");
    write_manifest(root.path(), &[&entry]);

    let mut rx = state.subscribe_events();
    let accepted = expect_ok(rpc(
        &state,
        "script-run",
        RpcCommand::RunScript(run_script_request()),
    ));
    let accepted = match accepted {
        RpcResultData::ManagedScript(ScriptCommandResult::Accepted(snapshot)) => snapshot,
        other => panic!("期望运行已受理，实际 {other:?}"),
    };
    assert_eq!(
        accepted.workspace_id, workspace_id,
        "运行快照必须沿用守护进程登记的工作区标识"
    );
    let run_id = accepted.run_id.clone();

    let queued = next_event(&mut rx, |payload| {
        lifecycle_of(payload).is_some_and(|event| {
            event.run.run_id == run_id && event.phase == ScriptLifecyclePhase::Queued
        })
    })
    .await;
    let started = next_event(&mut rx, |payload| {
        lifecycle_of(payload).is_some_and(|event| {
            event.run.run_id == run_id && event.phase == ScriptLifecyclePhase::Started
        })
    })
    .await;
    let finished = next_event(&mut rx, |payload| {
        lifecycle_of(payload).is_some_and(|event| {
            event.run.run_id == run_id
                && matches!(
                    event.phase,
                    ScriptLifecyclePhase::Exited | ScriptLifecyclePhase::Failed
                )
        })
    })
    .await;

    assert!(
        queued.seq < started.seq && started.seq < finished.seq,
        "生命周期事件 seq 必须严格单调：{} / {} / {}",
        queued.seq,
        started.seq,
        finished.seq
    );
    assert_eq!(
        lifecycle_of(&finished.payload)
            .expect("生命周期事件")
            .run
            .state,
        ScriptRunState::Exited,
        "受控桩应干净退出"
    );

    let inspected = expect_ok(rpc(
        &state,
        "script-inspect",
        RpcCommand::InspectScript(InspectScriptRequest {
            run_id: run_id.clone(),
        }),
    ));
    match inspected {
        RpcResultData::ManagedScript(ScriptCommandResult::Snapshot(snapshot)) => {
            assert_eq!(snapshot.state, ScriptRunState::Exited);
            assert_eq!(snapshot.exit_code, Some(0));
            assert_eq!(snapshot.workspace_id, workspace_id);
        }
        other => panic!("期望运行快照，实际 {other:?}"),
    }

    // 已收敛的运行再次 Stop 是幂等回执，不产生新事件。
    let before_stop = state.latest_seq().expect("事件序号");
    let stopped = expect_ok(rpc(
        &state,
        "script-stop",
        RpcCommand::StopScript(StopScriptRequest {
            run_id: run_id.clone(),
            force: false,
        }),
    ));
    match stopped {
        RpcResultData::ManagedScript(ScriptCommandResult::Snapshot(snapshot)) => {
            assert_eq!(snapshot.state, ScriptRunState::Exited);
        }
        other => panic!("期望幂等停止快照，实际 {other:?}"),
    }
    assert_eq!(
        state.latest_seq().expect("事件序号"),
        before_stop,
        "幂等停止不得重复下发生命周期事件"
    );

    // 事件只经订阅通道下发：订阅前发生的事件不会回放给新订阅者。
    let mut late = state.subscribe_events();
    assert!(late.try_recv().is_err(), "新订阅者不得收到订阅前发生的事件");
}

// ---------------------------------------------------------------------------
// 5. 受控 PTY：RPC 分片输出序号单调 + 关闭事件
// ---------------------------------------------------------------------------

#[tokio::test]
async fn controlled_pty_rpc_streams_sequenced_output_chunks() {
    let data = TempDir::new().expect("数据目录");
    let root = TempDir::new().expect("工作区目录");
    // 输出刻意跨越多个 16 KiB 分片边界。
    let expected: String = "终端输出🙂".repeat(3_000);
    assert!(expected.len() > PTY_OUTPUT_CHUNK_MAX_BYTES * 2);
    let state = open_state_with(
        Some(&data),
        [DaemonFeature::ControlledPty],
        expected.clone().into_bytes(),
    );
    let workspace_id = register_workspace(&state, root.path());

    let mut rx = state.subscribe_events();
    let opened = expect_ok(rpc(
        &state,
        "pty-open",
        RpcCommand::OpenPty(open_pty_request()),
    ));
    let session = match opened {
        RpcResultData::ControlledPty(PtyCommandResult::Opened(response)) => response.session,
        other => panic!("期望会话已打开，实际 {other:?}"),
    };
    assert_eq!(session.workspace_id, workspace_id);
    assert_eq!(session.size, PtySize::new(80, 24).expect("尺寸"));
    assert_ne!(
        session.state,
        PtySessionState::Failed,
        "内存假后端不得把会话判为失败"
    );

    let mut chunks: Vec<PtyOutputChunk> = Vec::new();
    loop {
        let envelope = next_event(&mut rx, |payload| {
            pty_event_of(payload).is_some_and(|event| matches!(event, PtyEvent::Output(_)))
        })
        .await;
        let Some(PtyEvent::Output(chunk)) = pty_event_of(&envelope.payload) else {
            unreachable!("已按 Output 过滤");
        };
        assert_eq!(chunk.pty_id, session.pty_id, "分片必须归属同一会话");
        assert_eq!(
            chunk.sequence,
            u64::try_from(chunks.len()).expect("分片序号"),
            "分片序号必须从 0 起严格递增"
        );
        let is_last = chunk.end_of_stream;
        chunks.push(chunk);
        if is_last {
            break;
        }
    }
    assert!(chunks.len() >= 3, "跨分片输出必须被切成多片");
    for chunk in &chunks {
        assert!(
            chunk.data.byte_len() <= PTY_OUTPUT_CHUNK_MAX_BYTES,
            "分片不得超出字节预算"
        );
    }
    let replayed: String = chunks
        .iter()
        .map(|chunk| chunk.data.as_str().to_owned())
        .collect();
    assert_eq!(replayed, expected, "分片必须可无损重组");

    let exited = next_event(&mut rx, |payload| {
        pty_event_of(payload).is_some_and(|event| matches!(event, PtyEvent::Exited { .. }))
    })
    .await;
    match pty_event_of(&exited.payload) {
        Some(PtyEvent::Exited {
            pty_id,
            last_sequence,
            ..
        }) => {
            assert_eq!(pty_id, session.pty_id);
            assert_eq!(
                last_sequence,
                u64::try_from(chunks.len() - 1).expect("末片序号"),
                "退出事件必须回报末片序号，便于客户端校验漏帧"
            );
        }
        other => panic!("期望退出事件，实际 {other:?}"),
    }

    let written = expect_ok(rpc(
        &state,
        "pty-write",
        RpcCommand::WritePty(WritePtyRequest {
            pty_id: session.pty_id.clone(),
            data: PtyInputData::try_from("echo hi".to_owned()).expect("输入"),
        }),
    ));
    match written {
        RpcResultData::ControlledPty(PtyCommandResult::Written(response)) => {
            assert_eq!(response.accepted_bytes, 7);
        }
        other => panic!("期望写入回执，实际 {other:?}"),
    }

    let resized = expect_ok(rpc(
        &state,
        "pty-resize",
        RpcCommand::ResizePty(ResizePtyRequest {
            pty_id: session.pty_id.clone(),
            size: PtySize::new(120, 40).expect("合法尺寸"),
        }),
    ));
    match resized {
        RpcResultData::ControlledPty(PtyCommandResult::Resized(response)) => {
            assert_eq!(response.session.size, PtySize::new(120, 40).expect("尺寸"));
        }
        other => panic!("期望尺寸回执，实际 {other:?}"),
    }

    let closed = expect_ok(rpc(
        &state,
        "pty-close",
        RpcCommand::ClosePty(soxu_protocol::ClosePtyRequest {
            pty_id: session.pty_id.clone(),
            reason: PtyCloseReason::ClientRequest,
        }),
    ));
    match closed {
        RpcResultData::ControlledPty(PtyCommandResult::Closed(response)) => {
            assert_eq!(response.session.state, PtySessionState::Closed);
        }
        other => panic!("期望关闭回执，实际 {other:?}"),
    }
    let closed_event = next_event(&mut rx, |payload| {
        pty_event_of(payload).is_some_and(|event| matches!(event, PtyEvent::Closed(_)))
    })
    .await;
    match pty_event_of(&closed_event.payload) {
        Some(PtyEvent::Closed(snapshot)) => assert_eq!(snapshot.pty_id, session.pty_id),
        other => panic!("期望关闭事件，实际 {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 6. 调度：RPC 触发与运行结果都经既有信封广播
// ---------------------------------------------------------------------------

#[tokio::test]
async fn schedule_trigger_and_result_are_broadcast_over_rpc() {
    let data = TempDir::new().expect("数据目录");
    let state = open_state_with(Some(&data), [DaemonFeature::Schedules], Vec::new());

    let created = expect_ok(rpc(
        &state,
        "schedule-create",
        RpcCommand::CreateSchedule(interval_schedule_request(3_600, "wire-create")),
    ));
    let schedule = match created {
        RpcResultData::Schedule(ScheduleCommandResult::Created(summary)) => summary,
        other => panic!("期望调度已创建，实际 {other:?}"),
    };

    let mut rx = state.subscribe_events();
    let triggered = expect_ok(rpc(
        &state,
        "schedule-trigger",
        RpcCommand::TriggerSchedule(TriggerScheduleRequest {
            schedule_id: schedule.schedule_id.clone(),
            idempotency_key: IdempotencyKey::try_from("wire-trigger".to_owned()).expect("幂等键"),
        }),
    ));
    let trigger = match triggered {
        RpcResultData::Schedule(ScheduleCommandResult::TriggerAccepted(trigger)) => trigger,
        other => panic!("期望触发已受理，实际 {other:?}"),
    };
    assert_eq!(trigger.schedule_id, schedule.schedule_id);

    let triggered_event = next_event(&mut rx, |payload| {
        schedule_event_of(payload).is_some_and(|event| matches!(event, ScheduleEvent::Triggered(_)))
    })
    .await;
    match schedule_event_of(&triggered_event.payload) {
        Some(ScheduleEvent::Triggered(event)) => {
            assert_eq!(event.run_id, trigger.run_id);
            assert_eq!(event.attempt, 1);
        }
        other => panic!("期望触发事件，实际 {other:?}"),
    }

    let outcome = state
        .finish_schedule_run(schedule_run_result(&schedule.schedule_id, &trigger.run_id))
        .expect("收敛运行结果");
    assert_eq!(outcome, FinishOutcome::Converged, "成功结果应一次收敛");

    let finished_event = next_event(&mut rx, |payload| {
        schedule_event_of(payload)
            .is_some_and(|event| matches!(event, ScheduleEvent::RunFinished(_)))
    })
    .await;
    assert!(
        finished_event.seq > triggered_event.seq,
        "结果事件 seq 必须晚于触发事件"
    );
    match schedule_event_of(&finished_event.payload) {
        Some(ScheduleEvent::RunFinished(result)) => {
            assert_eq!(result.run_id, trigger.run_id);
            assert_eq!(result.outcome, ScheduleRunOutcome::Succeeded);
        }
        other => panic!("期望结果事件，实际 {other:?}"),
    }

    // 宿主驱动的推进口：1 小时周期在本次调用里没有到期项。
    let report = state.tick_schedules(false).expect("推进一次调度");
    assert!(report.is_quiet(), "未到期的调度不得凭空触发：{report:?}");
}

// ---------------------------------------------------------------------------
// 7. WebSocket 订阅者收到 M6 信封；后连客户端不回放历史事件
// ---------------------------------------------------------------------------

async fn spawn_router(state: Arc<DaemonState>) -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("绑定临时端口");
    let addr = listener.local_addr().expect("读取本地地址");
    let router = build_router(Arc::clone(&state));
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    (addr, handle)
}

async fn ws_connect(addr: SocketAddr) -> WsClient {
    let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws"))
        .await
        .expect("建立 WebSocket 连接");
    let hello = ClientHello {
        client_kind: ClientKind::MobilePwa,
        client_version: "0.1.0".to_owned(),
        protocol_version: PROTOCOL_VERSION,
        after_seq: None,
        device_id: None,
    };
    ws.send(WsMsg::Text(
        serde_json::to_string(&hello).expect("序列化握手").into(),
    ))
    .await
    .expect("发送握手帧");
    let frame = tokio::time::timeout(Duration::from_secs(30), ws.next())
        .await
        .expect("等待握手响应")
        .expect("连接仍可读")
        .expect("握手帧可解码");
    let _: soxu_protocol::ServerHello =
        serde_json::from_str(frame.to_text().expect("握手指令必须是文本帧"))
            .expect("解析守护进程握手");
    ws
}

async fn ws_next_text(ws: &mut WsClient) -> String {
    let frame = tokio::time::timeout(EVENT_TIMEOUT, ws.next())
        .await
        .expect("在限期内收到下行帧")
        .expect("连接仍可读")
        .expect("下行帧可解码");
    frame.to_text().expect("下行帧必须是文本").to_owned()
}

#[tokio::test]
async fn websocket_subscriber_receives_m6_events_while_late_client_gets_none() {
    let data = TempDir::new().expect("数据目录");
    let root = TempDir::new().expect("工作区目录");
    let state = open_state_with(
        Some(&data),
        [DaemonFeature::ControlledPty],
        b"ws-output".to_vec(),
    );
    register_workspace(&state, root.path());
    let (_addr, server) = spawn_router(Arc::clone(&state)).await;
    let mut subscriber = ws_connect(_addr).await;

    // 直接经真实 WebSocket 传输层发一条 M6 RPC，证明接线在传输层落地。
    let request = RpcRequest {
        request_id: "ws-open-pty".to_owned(),
        command: RpcCommand::OpenPty(open_pty_request()),
    };
    subscriber
        .send(WsMsg::Text(
            serde_json::to_string(&request).expect("序列化 RPC").into(),
        ))
        .await
        .expect("发送 RPC 帧");

    let mut observed = Vec::new();
    let mut response_seen = false;
    loop {
        let raw = ws_next_text(&mut subscriber).await;
        let value: serde_json::Value = serde_json::from_str(&raw).expect("下行帧必须是 JSON");
        if value.get("request_id").and_then(serde_json::Value::as_str) == Some("ws-open-pty") {
            let response: RpcResponse = serde_json::from_str(&raw).expect("解析 RPC 响应");
            assert!(response.ok, "WS 上的 M6 RPC 必须成功：{:?}", response.error);
            assert!(matches!(
                response.result,
                Some(RpcResultData::ControlledPty(PtyCommandResult::Opened(_)))
            ));
            response_seen = true;
            continue;
        }
        let envelope: EventEnvelope<DaemonEvent> =
            serde_json::from_str(&raw).expect("下行事件必须是事件信封");
        if let Some(PtyEvent::Output(chunk)) = pty_event_of(&envelope.payload) {
            if let Some(previous) = observed.last() {
                assert!(
                    envelope.seq > *previous,
                    "下行事件 seq 必须严格单调：{} 之后收到 {}",
                    previous,
                    envelope.seq
                );
            }
            let is_last = chunk.end_of_stream;
            observed.push(envelope.seq);
            if is_last && response_seen {
                break;
            }
        }
    }
    assert!(response_seen, "必须收到 M6 RPC 响应帧");
    assert!(!observed.is_empty(), "订阅客户端必须收到 PTY 输出分片");

    // 事件都发生在订阅之后：此时再连的客户端不会被回放历史 M6 事件。
    let mut late = ws_connect(_addr).await;
    match tokio::time::timeout(Duration::from_millis(500), late.next()).await {
        Err(_) => {}
        Ok(Some(Ok(WsMsg::Text(raw)))) => {
            let envelope: EventEnvelope<DaemonEvent> =
                serde_json::from_str(raw.as_str()).expect("下行事件必须是事件信封");
            assert!(
                pty_event_of(&envelope.payload).is_none(),
                "未携带 after_seq 的新连接不得收到历史 M6 事件"
            );
        }
        Ok(other) => panic!("后连客户端不应收到 M6 事件，实际 {other:?}"),
    }

    server.abort();
}

// ---------------------------------------------------------------------------
// 受控子进程桩：仅由父测试经当前二进制重新执行（L-017）
// ---------------------------------------------------------------------------

#[test]
#[ignore = "受控子进程桩：仅由父测试经当前二进制重新执行"]
fn stub_child_exits_cleanly() {}
