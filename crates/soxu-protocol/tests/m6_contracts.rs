//! M6 前置契约回归：受管脚本、Schedules & Heartbeats 与受控 PTY。
//!
//! 覆盖：serde 往返、未知字段容错、非法参数拒绝、输出分片边界与序号、
//! 能力位缺省关闭，以及"新增枚举增量、不改主版本"的兼容约束。

use chrono::{TimeZone, Utc};
use serde_json::json;
use soxu_protocol::{
    CreateScheduleRequest, CronExpression, DaemonEvent, DaemonFeature, DaemonFeatureSet,
    EventEnvelope, IdempotencyKey, ManagedScriptEntry, OpenPtyRequest, PROTOCOL_VERSION, PtyEvent,
    PtyInputData, PtyOutputChunk, PtyOutputData, PtySessionId, PtySize, RpcCommand, RpcRequest,
    RpcResponse, RpcResultData, RunScriptRequest, ScheduleCadence, ScheduleCommand,
    ScheduleCommandResult, ScheduleEvent, ScheduleId, ScheduleListResult, ScheduleName,
    ScheduleRetryPolicy, ScheduleRunOutcome, ScheduleRunResult, ScheduleRunTrigger, ScheduleState,
    ScheduleSummary, ScheduleTimezone, ScheduleTrigger, ScheduleTriggeredEvent,
    ScheduleValidityWindow, ScriptArgument, ScriptCommand, ScriptCommandResult, ScriptEnvPolicy,
    ScriptId, ScriptLifecycleEvent, ScriptLifecyclePhase, ScriptManifestListing, ScriptProgram,
    ScriptRiskLevel, ScriptRunSnapshot, ScriptRunState, ScriptUserArguments,
};

fn ts(day: u32, hour: u32) -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, day, hour, 0, 0)
        .single()
        .expect("时间戳应可构造")
}

fn idempotency(value: &str) -> IdempotencyKey {
    IdempotencyKey::try_from(value.to_owned()).expect("幂等键应合法")
}

fn schedule_id(value: &str) -> ScheduleId {
    ScheduleId::try_from(value.to_owned()).expect("调度标识应合法")
}

fn pty_id(value: &str) -> PtySessionId {
    PtySessionId::try_from(value.to_owned()).expect("PTY 标识应合法")
}

fn build_script_entry() -> ManagedScriptEntry {
    serde_json::from_value::<ManagedScriptEntry>(json!({
        "script_id": "check-all",
        "display_name": "全量门禁",
        "description": "运行工作区门禁",
        "program": "pwsh",
        "argument_policy": {
            "fixed_arguments": ["-NoProfile"],
            "accepts_user_arguments": true,
            "max_user_arguments": 2,
            "max_user_argument_bytes": 256
        },
        "workdir": { "kind": "workspace_root" },
        "env_policy": { "allow_keys": ["PATHEXT"], "inherits_parent_env": false },
        "risk": "high",
        "service": false,
        "timeout_seconds": 900
    }))
    .expect_err("pwsh 属于 shell 解释器，必须拒绝");
    let entry: ManagedScriptEntry = serde_json::from_value(json!({
        "script_id": "check-all",
        "display_name": "全量门禁",
        "program": "cargo",
        "argument_policy": {
            "fixed_arguments": ["run", "--bin"],
            "accepts_user_arguments": true,
            "max_user_arguments": 2,
            "max_user_argument_bytes": 256
        },
        "workdir": { "kind": "subdirectory", "relative_path": "scripts" },
        "env_policy": { "allow_keys": ["PATHEXT"], "inherits_parent_env": false },
        "risk": "high",
        "service": false,
        "timeout_seconds": 900
    }))
    .expect("合法脚本声明应解析成功");
    entry
}

fn build_schedule_summary() -> ScheduleSummary {
    ScheduleSummary {
        schedule_id: schedule_id("sch_nightly"),
        workspace_id: "wks_01".to_owned(),
        name: ScheduleName::try_from("夜间巡检".to_owned()).expect("调度名称应合法"),
        cadence: ScheduleCadence::Cron {
            expression: CronExpression::try_from("0 3 * * *".to_owned()).expect("cron 应合法"),
            timezone: Some(
                ScheduleTimezone::try_from("Asia/Shanghai".to_owned()).expect("时区应合法"),
            ),
        },
        trigger: ScheduleTrigger {
            agent_id: "pi".to_owned(),
            model_id: Some("magpie/pi-1.5".to_owned()),
            permission: None,
            text: "巡检夜间 CI 失败并汇总".to_owned(),
        },
        validity: ScheduleValidityWindow {
            starts_at: Some(ts(28, 0)),
            expires_at: None,
        },
        retry: ScheduleRetryPolicy {
            max_attempts: 3,
            backoff_seconds: 120,
        },
        state: ScheduleState::Active,
        next_run_at: Some(ts(29, 3)),
        last_run_at: None,
        created_at: ts(28, 0),
        updated_at: ts(28, 0),
    }
}

#[test]
fn managed_script_contract_roundtrips_and_tolerates_unknown_fields() {
    let entry = build_script_entry();
    let raw = serde_json::to_value(&entry).expect("序列化脚本声明");
    assert_eq!(raw["script_id"], "check-all");
    assert_eq!(raw["argument_policy"]["accepts_user_arguments"], true);
    assert_eq!(raw["workdir"]["kind"], "subdirectory");

    // 未知字段必须被容忍：后续版本新增的字段不得让旧客户端解析失败。
    let mut tolerant = raw.clone();
    tolerant["future_field"] = json!({ "nested": [1, 2, 3] });
    tolerant["argument_policy"]["future_policy"] = json!("ignored");
    let decoded: ManagedScriptEntry = serde_json::from_value(tolerant).expect("未知字段应被忽略");
    assert_eq!(decoded, entry);

    let run = ScriptRunSnapshot {
        run_id: "run_01".to_owned(),
        workspace_id: "wks_01".to_owned(),
        script_id: ScriptId::try_from("check-all".to_owned()).expect("脚本标识"),
        state: ScriptRunState::Running,
        exit_code: None,
        started_at: Some(ts(28, 10)),
        finished_at: None,
        idempotency_key: Some(idempotency("check-all-2026-11-28")),
    };

    let response = RpcResponse::ok(
        "req_script",
        RpcResultData::ManagedScript(ScriptCommandResult::Listed(ScriptManifestListing {
            workspace_id: "wks_01".to_owned(),
            manifest_file: "paseo.json".to_owned(),
            scripts: vec![entry],
        })),
    );
    let encoded = serde_json::to_string(&response).expect("序列化脚本响应");
    let decoded_response: RpcResponse = serde_json::from_str(&encoded).expect("反序列化脚本响应");
    assert_eq!(decoded_response, response);

    let envelope = EventEnvelope::new(
        7,
        DaemonEvent::ScriptLifecycleChanged(ScriptLifecycleEvent {
            phase: ScriptLifecyclePhase::Started,
            run,
            occurred_at: ts(28, 10),
        }),
    );
    let encoded_event = serde_json::to_value(&envelope).expect("序列化脚本事件");
    assert_eq!(
        encoded_event["payload"]["event"],
        "script_lifecycle_changed"
    );
    assert_eq!(encoded_event["payload"]["data"]["phase"], "started");
    let decoded_event: EventEnvelope<DaemonEvent> =
        serde_json::from_value(encoded_event).expect("反序列化脚本事件");
    assert_eq!(decoded_event, envelope);

    let command = ScriptCommand::Run(RunScriptRequest {
        workspace_id: "wks_01".to_owned(),
        script_id: ScriptId::try_from("check-all".to_owned()).expect("脚本标识"),
        user_arguments: ScriptUserArguments::try_from(vec![
            ScriptArgument::try_from("--fast".to_owned()).expect("参数"),
        ])
        .expect("透传参数"),
        idempotency_key: Some(idempotency("run-1")),
        approval_id: Some("apr_01".to_owned()),
    });
    let raw_command = serde_json::to_string(&command).expect("序列化脚本命令");
    assert!(raw_command.contains("\"method\":\"run\""));
    let decoded_command: ScriptCommand =
        serde_json::from_str(&raw_command).expect("反序列化脚本命令");
    assert_eq!(decoded_command, command);
}

#[test]
fn script_rejects_shell_wrappers_and_injection_arguments() {
    for shell in [
        "pwsh",
        "pwsh.exe",
        "C:\\Program Files\\PowerShell\\7\\pwsh.exe",
        "bash",
        "/bin/sh",
    ] {
        assert!(
            ScriptProgram::try_from(shell.to_owned()).is_err(),
            "{shell} 必须作为 shell 解释器被拒绝"
        );
    }
    assert!(ScriptProgram::try_from("cargo".to_owned()).is_ok());
    assert!(ScriptProgram::try_from("./scripts/dev-daemon.ps1".to_owned()).is_ok());
    assert!(
        ScriptProgram::try_from("cargo; rm -rf /".to_owned()).is_err(),
        "程序字段不得携带 shell 元字符"
    );
    assert!(ScriptProgram::try_from("cargo run".to_owned()).is_err());

    assert!(
        ScriptArgument::try_from("a\nb".to_owned()).is_err(),
        "参数禁止换行"
    );
    assert!(ScriptArgument::try_from("a\0b".to_owned()).is_err());
    assert!(
        ScriptArgument::try_from("a; rm -rf /".to_owned()).is_ok(),
        "含元字符的参数作为单个不透明实参合法，绝不拼接"
    );
    assert!(ScriptArgument::try_from("x".repeat(4097)).is_err());

    for bad_id in ["Check-All", "-lead", "", "含中文"] {
        assert!(
            ScriptId::try_from(bad_id.to_owned()).is_err(),
            "{bad_id} 应被拒绝"
        );
    }

    let bad_policy = json!({
        "fixed_arguments": [],
        "accepts_user_arguments": false,
        "max_user_arguments": 2,
        "max_user_argument_bytes": 256
    });
    assert!(
        serde_json::from_value::<soxu_protocol::ScriptArgumentPolicy>(bad_policy).is_err(),
        "不接受透传参数时上限必须为 0"
    );

    let zero_bytes = json!({
        "fixed_arguments": [],
        "accepts_user_arguments": true,
        "max_user_arguments": 1,
        "max_user_argument_bytes": 0
    });
    assert!(
        serde_json::from_value::<soxu_protocol::ScriptArgumentPolicy>(zero_bytes).is_err(),
        "单条参数字节上限不得为 0"
    );

    let duplicate_env = json!({ "allow_keys": ["PATH", "PATH"] });
    assert!(
        serde_json::from_value::<ScriptEnvPolicy>(duplicate_env).is_err(),
        "环境白名单禁止重复键"
    );
    let invalid_env = json!({ "allow_keys": ["PATH=1"] });
    assert!(serde_json::from_value::<ScriptEnvPolicy>(invalid_env).is_err());
}

#[test]
fn script_rejects_escaping_workdir_and_inconsistent_service_timeout() {
    let escape = json!({
        "script_id": "escape",
        "display_name": "越级",
        "program": "cargo",
        "argument_policy": {
            "fixed_arguments": [],
            "accepts_user_arguments": false,
            "max_user_arguments": 0,
            "max_user_argument_bytes": 64
        },
        "workdir": { "kind": "subdirectory", "relative_path": "../outside" },
        "risk": "low"
    });
    assert!(
        serde_json::from_value::<ManagedScriptEntry>(escape).is_err(),
        "工作目录不得逃出工作区"
    );

    let service_timeout = json!({
        "script_id": "daemon-dev",
        "display_name": "开发守护进程",
        "program": "cargo",
        "argument_policy": {
            "fixed_arguments": ["run"],
            "accepts_user_arguments": false,
            "max_user_arguments": 0,
            "max_user_argument_bytes": 64
        },
        "workdir": { "kind": "workspace_root" },
        "risk": "medium",
        "service": true,
        "timeout_seconds": 60
    });
    assert!(
        serde_json::from_value::<ManagedScriptEntry>(service_timeout).is_err(),
        "常驻服务型脚本不得声明一次性超时"
    );

    let mut blank_name = serde_json::to_value(build_script_entry()).expect("序列化脚本声明");
    blank_name["display_name"] = json!("");
    assert!(serde_json::from_value::<ManagedScriptEntry>(blank_name).is_err());

    let mut zero_timeout = serde_json::to_value(build_script_entry()).expect("序列化脚本声明");
    zero_timeout["timeout_seconds"] = json!(0);
    assert!(serde_json::from_value::<ManagedScriptEntry>(zero_timeout).is_err());

    let risk_low = ScriptRiskLevel::Low;
    assert!(!risk_low.requires_approval());
    assert!(ScriptRiskLevel::High.requires_approval());
    assert!(ScriptRiskLevel::Critical.requires_approval());
}

#[test]
fn schedules_roundtrip_and_reject_invalid_parameters() {
    let summary = build_schedule_summary();
    let raw = serde_json::to_value(&summary).expect("序列化调度摘要");
    assert_eq!(raw["cadence"]["kind"], "cron");
    assert_eq!(raw["cadence"]["expression"], "0 3 * * *");
    assert_eq!(raw["retry"]["max_attempts"], 3);

    let mut tolerant = raw.clone();
    tolerant["future_schedule_field"] = json!(true);
    let decoded: ScheduleSummary = serde_json::from_value(tolerant).expect("未知字段应被忽略");
    assert_eq!(decoded, summary);

    let create = CreateScheduleRequest {
        workspace_id: "wks_01".to_owned(),
        name: ScheduleName::try_from("每夜巡检".to_owned()).expect("调度名称"),
        cadence: ScheduleCadence::Heartbeat {
            every_seconds: 300,
            only_when_idle: true,
        },
        trigger: summary.trigger.clone(),
        validity: ScheduleValidityWindow::default(),
        retry: ScheduleRetryPolicy::default(),
        idempotency_key: idempotency("create-nightly"),
        paused: false,
    };
    let encoded = serde_json::to_string(&create).expect("序列化创建请求");
    let decoded_create: CreateScheduleRequest =
        serde_json::from_str(&encoded).expect("反序列化创建请求");
    assert_eq!(decoded_create, create);
    assert!(create.validity.is_active_at(ts(28, 12)));
    assert!(
        !ScheduleValidityWindow {
            starts_at: Some(ts(29, 0)),
            expires_at: Some(ts(30, 0)),
        }
        .is_active_at(ts(28, 12)),
        "未到起点的调度必须视为未生效"
    );

    // 非法参数一律在反序列化阶段拒绝。
    assert!(CronExpression::try_from("*/5 * * * *".to_owned()).is_ok());
    assert!(
        CronExpression::try_from("0 0 * *".to_owned()).is_err(),
        "必须是 5 段"
    );
    assert!(CronExpression::try_from("0 0 * * * *".to_owned()).is_err());
    assert!(CronExpression::try_from("0 0 * * MON".to_owned()).is_err());
    assert!(CronExpression::try_from(String::new()).is_err());

    assert!(
        serde_json::from_value::<ScheduleCadence>(
            json!({ "kind": "interval", "every_seconds": 30 })
        )
        .is_err()
    );
    assert!(
        serde_json::from_value::<ScheduleCadence>(
            json!({ "kind": "interval", "every_seconds": 60 })
        )
        .is_ok()
    );
    assert!(
        serde_json::from_value::<ScheduleCadence>(
            json!({ "kind": "heartbeat", "every_seconds": 10, "only_when_idle": true })
        )
        .is_err()
    );
    assert!(
        serde_json::from_value::<ScheduleCadence>(
            json!({ "kind": "heartbeat", "every_seconds": 30 })
        )
        .is_ok()
    );

    assert!(ScheduleTimezone::try_from("Asia/Shanghai".to_owned()).is_ok());
    assert!(ScheduleTimezone::try_from("UTC".to_owned()).is_ok());
    assert!(ScheduleTimezone::try_from("Local".to_owned()).is_err());
    assert!(ScheduleTimezone::try_from("Asia Shanghai".to_owned()).is_err());

    for (attempts, backoff) in [(0u8, 0u32), (11, 0), (3, 3601)] {
        let bad = json!({ "max_attempts": attempts, "backoff_seconds": backoff });
        assert!(
            serde_json::from_value::<ScheduleRetryPolicy>(bad).is_err(),
            "attempts={attempts} backoff={backoff} 应被拒绝"
        );
    }

    let inverted = json!({
        "starts_at": ts(30, 0).to_rfc3339(),
        "expires_at": ts(29, 0).to_rfc3339()
    });
    assert!(serde_json::from_value::<ScheduleValidityWindow>(inverted).is_err());
    let equal_bounds = json!({
        "starts_at": ts(29, 0).to_rfc3339(),
        "expires_at": ts(29, 0).to_rfc3339()
    });
    assert!(serde_json::from_value::<ScheduleValidityWindow>(equal_bounds).is_err());

    let blank_agent = json!({ "agent_id": "  ", "text": "巡检" });
    assert!(serde_json::from_value::<ScheduleTrigger>(blank_agent).is_err());
    let empty_text = json!({ "agent_id": "pi", "text": "" });
    assert!(serde_json::from_value::<ScheduleTrigger>(empty_text).is_err());
}

#[test]
fn schedule_commands_and_events_roundtrip_through_rpc_and_events() {
    let summary = build_schedule_summary();
    let command = ScheduleCommand::List(soxu_protocol::ListSchedulesRequest {
        workspace_id: "wks_01".to_owned(),
    });
    let encoded = serde_json::to_string(&command).expect("序列化调度命令");
    assert_eq!(
        encoded,
        "{\"method\":\"list\",\"params\":{\"workspace_id\":\"wks_01\"}}"
    );
    let decoded: ScheduleCommand = serde_json::from_str(&encoded).expect("反序列化调度命令");
    assert_eq!(decoded, command);

    let response = RpcResponse::ok(
        "req_schedule",
        RpcResultData::Schedule(ScheduleCommandResult::Listed(ScheduleListResult {
            workspace_id: "wks_01".to_owned(),
            schedules: vec![summary.clone()],
        })),
    );
    let roundtrip: RpcResponse =
        serde_json::from_str(&serde_json::to_string(&response).expect("序列化调度响应"))
            .expect("反序列化调度响应");
    assert_eq!(roundtrip, response);

    let triggered = ScheduleTriggeredEvent {
        schedule_id: schedule_id("sch_nightly"),
        run_id: "run_9".to_owned(),
        trigger: ScheduleRunTrigger::Cron,
        scheduled_for: ts(29, 3),
        attempt: 2,
    };
    let finished = ScheduleRunResult {
        schedule_id: schedule_id("sch_nightly"),
        run_id: "run_9".to_owned(),
        outcome: ScheduleRunOutcome::Failed,
        attempts: 2,
        session_id: Some("sess_9".to_owned()),
        error: Some(soxu_protocol::ActionableError::new(
            soxu_protocol::ErrorCode::DriverUnavailable,
            "调度触发失败",
            "仅影响本次调度运行",
            "请检查 agent 是否安装",
        )),
        finished_at: ts(29, 4),
    };

    let envelope = EventEnvelope::new(
        11,
        DaemonEvent::ScheduleActivity(ScheduleEvent::Triggered(triggered.clone())),
    );
    let raw = serde_json::to_value(&envelope).expect("序列化调度事件");
    assert_eq!(raw["payload"]["event"], "schedule_activity");
    assert_eq!(raw["payload"]["data"]["kind"], "triggered");
    let decoded_event: EventEnvelope<DaemonEvent> =
        serde_json::from_value(raw).expect("反序列化调度事件");
    assert_eq!(decoded_event, envelope);

    let finished_envelope = EventEnvelope::new(
        12,
        DaemonEvent::ScheduleActivity(ScheduleEvent::RunFinished(finished.clone())),
    );
    let raw_finished = serde_json::to_string(&finished_envelope).expect("序列化运行结果");
    let decoded_finished: EventEnvelope<DaemonEvent> =
        serde_json::from_str(&raw_finished).expect("反序列化运行结果");
    assert_eq!(decoded_finished, finished_envelope);

    let removed: DaemonEvent = serde_json::from_value(json!({
        "event": "schedule_activity",
        "data": { "kind": "removed", "data": { "schedule_id": "sch_nightly", "future": 1 } }
    }))
    .expect("未知字段与已知变体应共存");
    assert!(matches!(
        removed,
        DaemonEvent::ScheduleActivity(ScheduleEvent::Removed { .. })
    ));
}

#[test]
fn pty_bounds_are_enforced_on_open_write_and_output() {
    assert!(PtySize::new(0, 24).is_err());
    assert!(PtySize::new(1_001, 24).is_err());
    assert!(PtySize::new(80, 501).is_err());
    assert!(serde_json::from_value::<PtySize>(json!({ "columns": 80, "rows": 24 })).is_ok());

    assert!(PtyInputData::try_from(String::new()).is_err());
    assert!(
        PtyInputData::try_from("x".repeat(soxu_protocol::PTY_INPUT_CHUNK_MAX_BYTES + 1)).is_err()
    );
    assert!(PtyInputData::try_from("x".repeat(soxu_protocol::PTY_INPUT_CHUNK_MAX_BYTES)).is_ok());
    assert!(
        PtyOutputData::try_from("x".repeat(soxu_protocol::PTY_OUTPUT_CHUNK_MAX_BYTES + 1)).is_err()
    );

    let open = OpenPtyRequest {
        workspace_id: "wks_01".to_owned(),
        size: PtySize::new(120, 40).expect("合法尺寸"),
        workdir: soxu_protocol::PtyWorkdirPolicy::Subdirectory {
            relative_path: soxu_protocol::WorkspaceRelativePath::try_from(
                "crates/soxu-daemon".to_owned(),
            )
            .expect("相对路径"),
        },
        env_policy: soxu_protocol::PtyEnvPolicy::default(),
    };
    let encoded = serde_json::to_string(&open).expect("序列化打开请求");
    let decoded: OpenPtyRequest = serde_json::from_str(&encoded).expect("反序列化打开请求");
    assert_eq!(decoded, open);

    let duplicate_env = json!({ "allow_keys": ["TERM", "TERM"] });
    assert!(serde_json::from_value::<soxu_protocol::PtyEnvPolicy>(duplicate_env).is_err());
    let escaping = json!({ "kind": "subdirectory", "relative_path": "../.." });
    assert!(serde_json::from_value::<soxu_protocol::PtyWorkdirPolicy>(escaping).is_err());

    let mut tolerant = serde_json::to_value(&open).expect("序列化打开请求");
    tolerant["future_pty_field"] = json!({ "x": 1 });
    let decoded_tolerant: OpenPtyRequest =
        serde_json::from_value(tolerant).expect("未知字段应被忽略");
    assert_eq!(decoded_tolerant, open);
}

#[test]
fn pty_output_chunks_are_bounded_and_sequenced() {
    let session = pty_id("pty_01");
    let output = "编译完成 ✅".repeat(6_000);
    let chunks = PtyOutputChunk::split_output(&session, 41, &output);
    assert!(chunks.len() > 1);
    let mut reassembled = String::new();
    for (index, chunk) in chunks.iter().enumerate() {
        assert!(chunk.data.byte_len() <= soxu_protocol::PTY_OUTPUT_CHUNK_MAX_BYTES);
        assert_eq!(chunk.sequence, 41 + index as u64);
        assert_eq!(chunk.end_of_stream, index + 1 == chunks.len());
        reassembled.push_str(chunk.data.as_str());
    }
    assert_eq!(reassembled, output);

    let last = chunks.last().expect("至少一个分片");
    let envelope = EventEnvelope::new(5, DaemonEvent::PtyActivity(PtyEvent::Output(last.clone())));
    let raw = serde_json::to_value(&envelope).expect("序列化 PTY 输出事件");
    assert_eq!(raw["payload"]["event"], "pty_activity");
    assert_eq!(raw["payload"]["data"]["kind"], "output");
    assert_eq!(raw["payload"]["data"]["data"]["sequence"], last.sequence);
    let decoded: EventEnvelope<DaemonEvent> =
        serde_json::from_value(raw).expect("反序列化 PTY 输出事件");
    assert_eq!(decoded, envelope);

    let oversized = json!({
        "pty_id": "pty_01",
        "sequence": 1,
        "data": "x".repeat(soxu_protocol::PTY_OUTPUT_CHUNK_MAX_BYTES + 1),
        "end_of_stream": false
    });
    assert!(
        serde_json::from_value::<PtyOutputChunk>(oversized).is_err(),
        "无界输出分片必须被拒绝"
    );

    let exited = DaemonEvent::PtyActivity(PtyEvent::Exited {
        pty_id: session,
        exit: soxu_protocol::PtyExitStatus {
            exit_code: Some(0),
            exit_signal: None,
        },
        last_sequence: last.sequence,
    });
    let raw_exited = serde_json::to_string(&exited).expect("序列化 PTY 退出事件");
    let decoded_exited: DaemonEvent =
        serde_json::from_str(&raw_exited).expect("反序列化 PTY 退出事件");
    assert_eq!(decoded_exited, exited);
}

#[test]
fn daemon_features_default_off_and_tolerate_unknown_tokens() {
    let default_set = DaemonFeatureSet::default();
    assert_eq!(default_set, DaemonFeatureSet::disabled());
    for feature in DaemonFeature::ALL {
        assert!(!default_set.is_enabled(feature), "{feature:?} 默认必须关闭");
    }

    let from_empty_object: DaemonFeatureSet =
        serde_json::from_str("{}").expect("空对象应解析为全关");
    assert_eq!(from_empty_object, DaemonFeatureSet::disabled());

    let partial: DaemonFeatureSet =
        serde_json::from_str(r#"{"declared":["schedules","future_capability","controlled_pty"]}"#)
            .expect("未知能力令牌不得导致解析失败");
    assert!(partial.is_enabled(DaemonFeature::Schedules));
    assert!(partial.is_enabled(DaemonFeature::ControlledPty));
    assert!(!partial.is_enabled(DaemonFeature::ManagedScripts));
    assert!(!partial.is_enabled(DaemonFeature::Heartbeats));
    assert_eq!(partial.unknown_tokens(), vec!["future_capability"]);

    let encoded = serde_json::to_string(&partial).expect("序列化能力声明");
    let roundtrip: DaemonFeatureSet = serde_json::from_str(&encoded).expect("反序列化能力声明");
    assert_eq!(roundtrip, partial, "未知令牌必须原样保留");

    let enabled = DaemonFeatureSet::from_features(DaemonFeature::ALL);
    for feature in DaemonFeature::ALL {
        assert!(enabled.is_enabled(feature));
    }
    assert_eq!(enabled.enabled_features(), DaemonFeature::ALL.to_vec());
    assert!(enabled.unknown_tokens().is_empty());

    assert_eq!(
        DaemonFeature::from_token("controlled_pty"),
        Some(DaemonFeature::ControlledPty)
    );
    assert_eq!(DaemonFeature::from_token("not_a_feature"), None);
    assert_eq!(
        DaemonFeature::ALL.map(DaemonFeature::token),
        [
            "managed_scripts",
            "schedules",
            "heartbeats",
            "controlled_pty"
        ]
    );
}

#[test]
fn protocol_extension_is_additive_without_major_version_bump() {
    // 三组契约全部通过增量枚举与新增类型接入，主版本保持不变。
    assert_eq!(PROTOCOL_VERSION, 1);

    let legacy: RpcRequest =
        serde_json::from_str(r#"{"request_id":"r1","command":{"method":"get_state_snapshot"}}"#)
            .expect("既有命令必须保持可解析");
    assert!(matches!(legacy.command, RpcCommand::GetStateSnapshot));

    let script_event: serde_json::Value = serde_json::from_str(
        r#"{"protocol_version":1,"seq":1,"timestamp":"2026-11-28T00:00:00Z","payload":{"event":"script_lifecycle_changed","data":{"phase":"queued","occurred_at":"2026-11-28T00:00:00Z","run":{"run_id":"r","workspace_id":"w","script_id":"s","state":"queued","exit_code":null,"started_at":null,"finished_at":null,"idempotency_key":null}}}}"#,
    )
    .expect("事件信封 JSON 应可解析");
    assert_eq!(script_event["payload"]["event"], "script_lifecycle_changed");

    let script_result =
        RpcResultData::ManagedScript(ScriptCommandResult::Accepted(ScriptRunSnapshot {
            run_id: "run_1".to_owned(),
            workspace_id: "wks_01".to_owned(),
            script_id: ScriptId::try_from("check-all".to_owned()).expect("脚本标识"),
            state: ScriptRunState::Queued,
            exit_code: None,
            started_at: None,
            finished_at: None,
            idempotency_key: None,
        }));
    let encoded = serde_json::to_string(&script_result).expect("序列化脚本结果");
    assert!(encoded.contains("\"kind\":\"managed_script\""));
    let decoded: RpcResultData = serde_json::from_str(&encoded).expect("反序列化脚本结果");
    assert_eq!(decoded, script_result);
}
