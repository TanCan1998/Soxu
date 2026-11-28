//! M6 传输契约回归：`RpcCommand` 脚本 / 调度 / PTY 全集变体、结果与事件经既有信封下发、
//! 未知变体容错，以及能力位缺省关闭的类型可表达性。
//!
//! 本文件只针对传输层：载荷一律复用 `scripts` / `schedules` / `pty` 域类型，
//! 因此这里同时校验「与域类型一致的字段校验」——非法载荷在反序列化阶段即被拒绝，
//! 不会等到守护进程 handler 才暴露。

use chrono::{TimeZone, Utc};
use serde_json::{Value, json};
use soxu_protocol::{
    ActionableError, ClosePtyRequest, CreateScheduleRequest, CronExpression, DaemonEvent,
    DaemonFeature, DaemonFeatureSet, DeleteScheduleRequest, ErrorCode, EventEnvelope,
    IdempotencyKey, InspectScriptRequest, ListSchedulesRequest, ListScriptsRequest, OpenPtyRequest,
    PROTOCOL_VERSION, PauseScheduleRequest, PtyCloseReason, PtyCommand, PtyCommandResult,
    PtyEnvPolicy, PtyEvent, PtyInputData, PtyOutputChunk, PtySessionId, PtySize, PtyWorkdirPolicy,
    ResizePtyRequest, RpcCommand, RpcDomainCommand, RpcRequest, RpcResponse, RpcResultData,
    RunScriptRequest, ScheduleCadence, ScheduleCommand, ScheduleCommandResult, ScheduleEvent,
    ScheduleId, ScheduleName, ScheduleRetryPolicy, ScheduleRunOutcome, ScheduleRunResult,
    ScheduleRunTrigger, ScheduleTrigger, ScheduleTriggeredEvent, ScheduleValidityWindow,
    ScriptArgument, ScriptCommand, ScriptCommandResult, ScriptEnvPolicy, ScriptId, ScriptProgram,
    ScriptRunSnapshot, ScriptRunState, ScriptUserArguments, StopScriptRequest,
    TriggerScheduleRequest, UpdateScheduleRequest, WorkspaceRelativePath, WritePtyRequest,
    WritePtyResponse,
};

fn ts(day: u32, hour: u32) -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, day, hour, 0, 0)
        .single()
        .expect("时间戳应可构造")
}

fn idem(value: &str) -> IdempotencyKey {
    IdempotencyKey::try_from(value.to_owned()).expect("幂等键应合法")
}

fn script_id(value: &str) -> ScriptId {
    ScriptId::try_from(value.to_owned()).expect("脚本标识应合法")
}

fn schedule_id(value: &str) -> ScheduleId {
    ScheduleId::try_from(value.to_owned()).expect("调度标识应合法")
}

fn pty_id(value: &str) -> PtySessionId {
    PtySessionId::try_from(value.to_owned()).expect("PTY 标识应合法")
}

fn list_scripts() -> ListScriptsRequest {
    ListScriptsRequest {
        workspace_id: "wks_01".to_owned(),
    }
}

fn run_script() -> RunScriptRequest {
    RunScriptRequest {
        workspace_id: "wks_01".to_owned(),
        script_id: script_id("check-all"),
        user_arguments: ScriptUserArguments::try_from(vec![
            ScriptArgument::try_from("--fast".to_owned()).expect("透传参数"),
        ])
        .expect("透传参数列表"),
        idempotency_key: Some(idem("run-check-all-1")),
        approval_id: Some("apr_01".to_owned()),
    }
}

fn stop_script() -> StopScriptRequest {
    StopScriptRequest {
        run_id: "run_01".to_owned(),
        force: true,
    }
}

fn inspect_script() -> InspectScriptRequest {
    InspectScriptRequest {
        run_id: "run_01".to_owned(),
    }
}

fn list_schedules() -> ListSchedulesRequest {
    ListSchedulesRequest {
        workspace_id: "wks_01".to_owned(),
    }
}

fn create_schedule() -> CreateScheduleRequest {
    CreateScheduleRequest {
        workspace_id: "wks_01".to_owned(),
        name: ScheduleName::try_from("每夜巡检".to_owned()).expect("调度名称"),
        cadence: ScheduleCadence::Cron {
            expression: CronExpression::try_from("0 3 * * *".to_owned()).expect("cron 表达式"),
            timezone: None,
        },
        trigger: ScheduleTrigger {
            agent_id: "pi".to_owned(),
            model_id: None,
            permission: None,
            text: "汇总夜间 CI 失败".to_owned(),
        },
        validity: ScheduleValidityWindow::default(),
        retry: ScheduleRetryPolicy::default(),
        idempotency_key: idem("create-nightly"),
        paused: false,
    }
}

fn update_schedule() -> UpdateScheduleRequest {
    UpdateScheduleRequest {
        schedule_id: schedule_id("sch_nightly"),
        name: Some(ScheduleName::try_from("每夜巡检（新）".to_owned()).expect("调度名称")),
        cadence: Some(ScheduleCadence::Interval { every_seconds: 300 }),
        trigger: None,
        validity: None,
        retry: None,
    }
}

fn disable_schedule() -> PauseScheduleRequest {
    PauseScheduleRequest {
        schedule_id: schedule_id("sch_nightly"),
        paused: true,
    }
}

fn delete_schedule() -> DeleteScheduleRequest {
    DeleteScheduleRequest {
        schedule_id: schedule_id("sch_nightly"),
    }
}

fn trigger_schedule() -> TriggerScheduleRequest {
    TriggerScheduleRequest {
        schedule_id: schedule_id("sch_nightly"),
        idempotency_key: idem("trigger-nightly-1"),
    }
}

fn open_pty() -> OpenPtyRequest {
    OpenPtyRequest {
        workspace_id: "wks_01".to_owned(),
        size: PtySize::new(120, 40).expect("合法尺寸"),
        workdir: PtyWorkdirPolicy::Subdirectory {
            relative_path: WorkspaceRelativePath::try_from("crates/soxu-daemon".to_owned())
                .expect("工作区相对路径"),
        },
        env_policy: PtyEnvPolicy::default(),
    }
}

fn write_pty() -> WritePtyRequest {
    WritePtyRequest {
        pty_id: pty_id("pty_01"),
        data: PtyInputData::try_from("cargo test\r".to_owned()).expect("PTY 输入"),
    }
}

fn resize_pty() -> ResizePtyRequest {
    ResizePtyRequest {
        pty_id: pty_id("pty_01"),
        size: PtySize::new(80, 24).expect("合法尺寸"),
    }
}

fn close_pty() -> ClosePtyRequest {
    ClosePtyRequest {
        pty_id: pty_id("pty_01"),
        reason: PtyCloseReason::ClientRequest,
    }
}

/// M6 传输层全集：每个变体都必须 serde 往返一致，并 1:1 折叠成对应域命令。
fn m6_command_cases() -> Vec<(RpcCommand, &'static str, RpcDomainCommand)> {
    use ScriptCommand::{Inspect, List, Run, Stop};

    vec![
        (
            RpcCommand::ListScripts(list_scripts()),
            "list_scripts",
            RpcDomainCommand::Script(List(list_scripts())),
        ),
        (
            RpcCommand::RunScript(run_script()),
            "run_script",
            RpcDomainCommand::Script(Run(run_script())),
        ),
        (
            RpcCommand::StopScript(stop_script()),
            "stop_script",
            RpcDomainCommand::Script(Stop(stop_script())),
        ),
        (
            RpcCommand::InspectScript(inspect_script()),
            "inspect_script",
            RpcDomainCommand::Script(Inspect(inspect_script())),
        ),
        (
            RpcCommand::ListSchedules(list_schedules()),
            "list_schedules",
            RpcDomainCommand::Schedule(ScheduleCommand::List(list_schedules())),
        ),
        (
            RpcCommand::CreateSchedule(create_schedule()),
            "create_schedule",
            RpcDomainCommand::Schedule(ScheduleCommand::Create(create_schedule())),
        ),
        (
            RpcCommand::UpdateSchedule(update_schedule()),
            "update_schedule",
            RpcDomainCommand::Schedule(ScheduleCommand::Update(update_schedule())),
        ),
        (
            RpcCommand::DisableSchedule(disable_schedule()),
            "disable_schedule",
            RpcDomainCommand::Schedule(ScheduleCommand::Pause(disable_schedule())),
        ),
        (
            RpcCommand::DeleteSchedule(delete_schedule()),
            "delete_schedule",
            RpcDomainCommand::Schedule(ScheduleCommand::Delete(delete_schedule())),
        ),
        (
            RpcCommand::TriggerSchedule(trigger_schedule()),
            "trigger_schedule",
            RpcDomainCommand::Schedule(ScheduleCommand::TriggerNow(trigger_schedule())),
        ),
        (
            RpcCommand::OpenPty(open_pty()),
            "open_pty",
            RpcDomainCommand::Pty(PtyCommand::Open(open_pty())),
        ),
        (
            RpcCommand::WritePty(write_pty()),
            "write_pty",
            RpcDomainCommand::Pty(PtyCommand::Write(write_pty())),
        ),
        (
            RpcCommand::ResizePty(resize_pty()),
            "resize_pty",
            RpcDomainCommand::Pty(PtyCommand::Resize(resize_pty())),
        ),
        (
            RpcCommand::ClosePty(close_pty()),
            "close_pty",
            RpcDomainCommand::Pty(PtyCommand::Close(close_pty())),
        ),
    ]
}

#[test]
fn m6_commands_roundtrip_and_fold_into_domain_commands() {
    let cases = m6_command_cases();
    assert_eq!(cases.len(), 14, "脚本 4 + 调度 6 + PTY 4 必须全覆盖");

    for (command, method, domain) in cases {
        let encoded = serde_json::to_value(&command).expect("序列化 M6 命令");
        assert_eq!(encoded["method"], method, "{method} 的线缆方法名不得漂移");
        assert!(
            encoded.get("params").is_some(),
            "{method} 必须携带 params 载荷"
        );

        let roundtrip: RpcCommand = serde_json::from_value(encoded).expect("反序列化 M6 命令");
        assert_eq!(roundtrip, command, "{method} 必须 serde 往返一致");

        assert_eq!(
            command.into_domain_command(),
            Some(domain),
            "{method} 必须 1:1 折叠为对应域命令"
        );
    }

    // 非 M6 命令不参与域命令折叠，避免守护进程接线时误吞既有分支。
    assert_eq!(RpcCommand::GetStateSnapshot.into_domain_command(), None);
    assert_eq!(
        RpcCommand::StopTurn {
            session_id: "sess_01".to_owned(),
        }
        .into_domain_command(),
        None
    );
}

#[test]
fn m6_commands_are_additive_on_the_wire_without_version_bump() {
    assert_eq!(PROTOCOL_VERSION, 1, "增量变体不得提升主协议版本");

    // 既有命令线缆形状保持不变（单元变体不带 params）。
    assert_eq!(
        serde_json::to_string(&RpcCommand::GetStateSnapshot).expect("序列化既有命令"),
        r#"{"method":"get_state_snapshot"}"#
    );

    let request = RpcRequest {
        request_id: "req_scripts".to_owned(),
        command: RpcCommand::ListScripts(list_scripts()),
    };
    let raw = serde_json::to_value(&request).expect("序列化 M6 请求信封");
    assert_eq!(
        raw,
        json!({
            "request_id": "req_scripts",
            "command": { "method": "list_scripts", "params": { "workspace_id": "wks_01" } }
        }),
        "信封沿用 method/params 相邻标签，不新增外层字段"
    );
    let decoded: RpcRequest = serde_json::from_value(raw).expect("反序列化 M6 请求信封");
    assert_eq!(decoded, request);
}

/// 解码未知变体必须返回 `Err` 而非 panic：serde 失败由调用方转成 `Unsupported` 错误响应。
fn assert_rejected_without_panic<T>(
    label: &str,
    decode: impl FnOnce() -> Result<T, serde_json::Error>,
) {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(decode)) {
        Ok(Err(_)) => {}
        Ok(Ok(_)) => panic!("{label} 必须被拒绝，而不是静默接受未知变体"),
        Err(_) => panic!("{label} 解码不得 panic"),
    }
}

#[test]
fn unknown_command_result_and_event_variants_do_not_panic() {
    assert_rejected_without_panic("未知 RPC 命令", || {
        serde_json::from_str::<RpcCommand>(r#"{"method":"future_command","params":{"x":1}}"#)
    });
    assert_rejected_without_panic("未知 RPC 请求信封", || {
        serde_json::from_str::<RpcRequest>(r#"{"request_id":"r1","command":"not-an-object"}"#)
    });
    assert_rejected_without_panic("未知结果变体", || {
        serde_json::from_str::<RpcResultData>(r#"{"kind":"future_result","data":{}}"#)
    });
    assert_rejected_without_panic("未知事件变体", || {
        serde_json::from_str::<DaemonEvent>(r#"{"event":"future_event","data":{"x":1}}"#)
    });
    assert_rejected_without_panic("事件信封内未知事件", || {
        serde_json::from_str::<EventEnvelope<DaemonEvent>>(
            r#"{"protocol_version":1,"seq":9,"timestamp":"2026-11-28T00:00:00Z","payload":{"event":"future_event","data":{}}}"#,
        )
    });

    // 反向兼容的宽松面必须保留：已知变体仍容忍未知字段。
    let tolerant: DaemonEvent = serde_json::from_value(json!({
        "event": "schedule_activity",
        "data": {
            "kind": "removed",
            "data": { "schedule_id": "sch_1" },
            "future_field": [1, 2, 3]
        }
    }))
    .expect("已知变体携带未知字段不得解析失败");
    assert!(matches!(
        tolerant,
        DaemonEvent::ScheduleActivity(ScheduleEvent::Removed { .. })
    ));
}

#[test]
fn m6_command_payloads_reuse_domain_validation() {
    // 非法载荷必须在反序列化阶段被拒绝：传输层不重复实现也不放宽域校验。
    let invalid: Vec<(&str, Value)> = vec![
        (
            "open_pty 零列尺寸",
            json!({
                "method": "open_pty",
                "params": {
                    "workspace_id": "wks_01",
                    "size": { "columns": 0, "rows": 24 },
                    "workdir": { "kind": "workspace_root" }
                }
            }),
        ),
        (
            "open_pty 越级工作目录",
            json!({
                "method": "open_pty",
                "params": {
                    "workspace_id": "wks_01",
                    "size": { "columns": 80, "rows": 24 },
                    "workdir": { "kind": "subdirectory", "relative_path": "../outside" }
                }
            }),
        ),
        (
            "write_pty 空输入",
            json!({ "method": "write_pty", "params": { "pty_id": "pty_01", "data": "" } }),
        ),
        (
            "write_pty 超长输入",
            json!({
                "method": "write_pty",
                "params": {
                    "pty_id": "pty_01",
                    "data": "x".repeat(soxu_protocol::PTY_INPUT_CHUNK_MAX_BYTES + 1)
                }
            }),
        ),
        (
            "resize_pty 超限行列",
            json!({
                "method": "resize_pty",
                "params": { "pty_id": "pty_01", "size": { "columns": 1001, "rows": 24 } }
            }),
        ),
        (
            "close_pty 未知关闭原因",
            json!({
                "method": "close_pty",
                "params": { "pty_id": "pty_01", "reason": "future_reason" }
            }),
        ),
        (
            "close_pty 非法会话标识",
            json!({
                "method": "close_pty",
                "params": { "pty_id": "pty 01/../x", "reason": "client_request" }
            }),
        ),
        (
            "run_script 非法脚本标识",
            json!({
                "method": "run_script",
                "params": { "workspace_id": "wks_01", "script_id": "Check-All" }
            }),
        ),
        (
            "run_script 透传参数超量",
            json!({
                "method": "run_script",
                "params": {
                    "workspace_id": "wks_01",
                    "script_id": "check-all",
                    "user_arguments": (0..17).map(|i| format!("--arg{i}")).collect::<Vec<_>>()
                }
            }),
        ),
        (
            "stop_script 缺少 force",
            json!({ "method": "stop_script", "params": { "run_id": "run_01" } }),
        ),
        (
            "create_schedule 非法 cron",
            json!({
                "method": "create_schedule",
                "params": {
                    "workspace_id": "wks_01",
                    "name": "夜间巡检",
                    "cadence": { "kind": "cron", "expression": "0 0 * *" },
                    "trigger": { "agent_id": "pi", "text": "巡检" },
                    "idempotency_key": "create-1"
                }
            }),
        ),
        (
            "create_schedule 空触发正文",
            json!({
                "method": "create_schedule",
                "params": {
                    "workspace_id": "wks_01",
                    "name": "夜间巡检",
                    "cadence": { "kind": "interval", "every_seconds": 300 },
                    "trigger": { "agent_id": "pi", "text": "" },
                    "idempotency_key": "create-2"
                }
            }),
        ),
        (
            "update_schedule 非法调度标识",
            json!({ "method": "update_schedule", "params": { "schedule_id": "sch/../x" } }),
        ),
        (
            "delete_schedule 缺少标识",
            json!({ "method": "delete_schedule", "params": {} }),
        ),
        (
            "trigger_schedule 缺少幂等键",
            json!({ "method": "trigger_schedule", "params": { "schedule_id": "sch_1" } }),
        ),
        (
            "list_scripts 类型不符",
            json!({ "method": "list_scripts", "params": { "workspace_id": 42 } }),
        ),
    ];

    for (label, payload) in invalid {
        let decoded: Result<RpcCommand, _> = serde_json::from_value(payload);
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| decoded))
            .unwrap_or_else(|_| panic!("{label} 解码不得 panic"));
        assert!(outcome.is_err(), "{label} 必须在反序列化阶段被拒绝");
    }

    // 域类型本身的构造器同样拒绝越界值（与线缆层一致）。
    assert!(PtySize::new(0, 24).is_err());
    assert!(ScriptProgram::try_from("pwsh".to_owned()).is_err());
    assert!(CronExpression::try_from("0 0 * * MON".to_owned()).is_err());
    assert!(!ScriptEnvPolicy::default().inherits_parent_env);
}

/// 结果变体与其门控能力位的对应关系：能力位未声明即不得下发该结果。
fn gating_feature(result: &RpcResultData) -> DaemonFeature {
    match result {
        RpcResultData::ManagedScript(_) => DaemonFeature::ManagedScripts,
        RpcResultData::Schedule(_) => DaemonFeature::Schedules,
        RpcResultData::ControlledPty(_) => DaemonFeature::ControlledPty,
        other => panic!("{other:?} 不属于 M6 三组域结果"),
    }
}

#[test]
fn capability_disabled_semantics_are_type_expressible() {
    let run = ScriptRunSnapshot {
        run_id: "run_01".to_owned(),
        workspace_id: "wks_01".to_owned(),
        script_id: script_id("check-all"),
        state: ScriptRunState::Queued,
        exit_code: None,
        started_at: None,
        finished_at: None,
        idempotency_key: None,
    };
    let results = vec![
        RpcResultData::ManagedScript(ScriptCommandResult::Snapshot(run)),
        RpcResultData::Schedule(ScheduleCommandResult::Removed {
            schedule_id: schedule_id("sch_nightly"),
        }),
        RpcResultData::ControlledPty(PtyCommandResult::Written(WritePtyResponse {
            pty_id: pty_id("pty_01"),
            accepted_bytes: 4,
        })),
    ];

    // 缺省全关：任何 M6 结果都不得下发到未协商能力的客户端。
    let disabled = DaemonFeatureSet::disabled();
    for result in &results {
        let feature = gating_feature(result);
        assert!(
            !disabled.is_enabled(feature),
            "{feature:?} 缺省必须关闭，因而禁止下发 {result:?}"
        );
    }

    // 只声明 PTY 时，其余两域仍关闭——能力位逐项独立，不做整组放行。
    let only_pty = DaemonFeatureSet::from_features([DaemonFeature::ControlledPty]);
    assert!(only_pty.is_enabled(gating_feature(&results[2])));
    assert!(!only_pty.is_enabled(gating_feature(&results[0])));
    assert!(!only_pty.is_enabled(gating_feature(&results[1])));

    // 能力关闭时守护进程的线缆表达：ok=false + Unsupported 三要素，且不带 result。
    let gated = RpcResponse::err(
        "req_pty",
        ActionableError::new(
            ErrorCode::Unsupported,
            "客户端未协商 controlled_pty 能力",
            "本次 PTY 开启请求未执行，未创建任何终端会话",
            "请在握手声明的能力位中加入 controlled_pty 后重试",
        ),
    );
    let raw = serde_json::to_value(&gated).expect("序列化能力关闭响应");
    assert_eq!(raw["ok"], false);
    assert_eq!(raw["error"]["code"], "unsupported");
    assert!(raw["result"].is_null(), "失败响应不得携带结果载荷");
    assert!(
        !raw["error"]["impact_scope_zh"]
            .as_str()
            .unwrap_or_default()
            .is_empty()
    );
    let roundtrip: RpcResponse = serde_json::from_value(raw).expect("反序列化能力关闭响应");
    assert_eq!(roundtrip, gated);
}

#[test]
fn m6_events_share_the_existing_envelope_and_catch_up_batch() {
    let run = ScriptRunSnapshot {
        run_id: "run_01".to_owned(),
        workspace_id: "wks_01".to_owned(),
        script_id: script_id("check-all"),
        state: ScriptRunState::Exited,
        exit_code: Some(0),
        started_at: Some(ts(28, 10)),
        finished_at: Some(ts(28, 11)),
        idempotency_key: Some(idem("run-check-all-1")),
    };
    let script_event = EventEnvelope::new(
        31,
        DaemonEvent::ScriptLifecycleChanged(soxu_protocol::ScriptLifecycleEvent {
            phase: soxu_protocol::ScriptLifecyclePhase::Exited,
            run,
            occurred_at: ts(28, 11),
        }),
    );

    let chunks = PtyOutputChunk::split_output(&pty_id("pty_01"), 0, &"编译完成 ✅".repeat(4_000));
    let pty_event = EventEnvelope::new(
        32,
        DaemonEvent::PtyActivity(PtyEvent::Output(chunks[0].clone())),
    );

    let triggered = ScheduleTriggeredEvent {
        schedule_id: schedule_id("sch_nightly"),
        run_id: "run_9".to_owned(),
        trigger: ScheduleRunTrigger::Cron,
        scheduled_for: ts(29, 3),
        attempt: 1,
    };
    let schedule_trigger = EventEnvelope::new(
        33,
        DaemonEvent::ScheduleActivity(ScheduleEvent::Triggered(triggered)),
    );
    let finished = ScheduleRunResult {
        schedule_id: schedule_id("sch_nightly"),
        run_id: "run_9".to_owned(),
        outcome: ScheduleRunOutcome::Succeeded,
        attempts: 1,
        session_id: Some("sess_9".to_owned()),
        error: None,
        finished_at: ts(29, 4),
    };
    let schedule_result = EventEnvelope::new(
        34,
        DaemonEvent::ScheduleActivity(ScheduleEvent::RunFinished(finished)),
    );

    // 一次断线追平批量里同时承载三类事件：证明它们共用同一信封与同一 seq 通道。
    let batch = RpcResultData::Events {
        events: vec![script_event, pty_event, schedule_trigger, schedule_result],
        latest_seq: 34,
    };
    let raw = serde_json::to_value(&batch).expect("序列化追平批量");
    let events = raw["data"]["events"].as_array().expect("events 必须是数组");
    assert_eq!(events.len(), 4);
    let tags: Vec<&str> = events
        .iter()
        .map(|event| event["payload"]["event"].as_str().expect("事件标签"))
        .collect();
    assert_eq!(
        tags,
        vec![
            "script_lifecycle_changed",
            "pty_activity",
            "schedule_activity",
            "schedule_activity"
        ]
    );
    let seqs: Vec<u64> = events
        .iter()
        .map(|event| event["seq"].as_u64().expect("seq"))
        .collect();
    assert_eq!(seqs, vec![31, 32, 33, 34], "同一批内 seq 必须严格递增");
    for event in events {
        assert_eq!(event["protocol_version"], PROTOCOL_VERSION);
    }

    let roundtrip: RpcResultData = serde_json::from_value(raw).expect("反序列化追平批量");
    assert_eq!(roundtrip, batch);
    assert!(chunks[0].data.byte_len() <= soxu_protocol::PTY_OUTPUT_CHUNK_MAX_BYTES);
}
