//! 调度域跨模块回归：门控、幂等、周期、重试、停用/删除收敛与持久化。

use chrono::{DateTime, Duration, Utc};
use soxu_daemon::{
    FeatureGate, FinishOutcome, ManualClock, ScheduleEventSink, ScheduleService, ScheduleStore,
};
use soxu_protocol::{
    ActionableError, CreateScheduleRequest, CronExpression, DaemonFeature, DeleteScheduleRequest,
    ErrorCode, IdempotencyKey, ListSchedulesRequest, PauseScheduleRequest, ScheduleCadence,
    ScheduleCommand, ScheduleCommandResult, ScheduleEvent, ScheduleId, ScheduleName,
    ScheduleRetryPolicy, ScheduleRunOutcome, ScheduleRunResult, ScheduleState, ScheduleTrigger,
    ScheduleValidityWindow, TriggerScheduleRequest,
};
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

const START: i64 = 1_800_000_000;

#[derive(Default)]
struct Recorder(Mutex<Vec<ScheduleEvent>>);

impl ScheduleEventSink for Recorder {
    fn emit(&self, event: ScheduleEvent) {
        self.0.lock().expect("event recorder").push(event);
    }
}

impl Recorder {
    fn take(&self) -> Vec<ScheduleEvent> {
        std::mem::take(&mut *self.0.lock().expect("event recorder"))
    }

    fn snapshot(&self) -> Vec<ScheduleEvent> {
        self.0.lock().expect("event recorder").clone()
    }
}

fn at(seconds: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(seconds, 0).expect("valid timestamp")
}

fn key(raw: &str) -> IdempotencyKey {
    IdempotencyKey::try_from(raw.to_owned()).expect("valid idempotency key")
}

fn service(
    gate: FeatureGate,
    recorder: &Arc<Recorder>,
    clock: &Arc<ManualClock>,
) -> ScheduleService {
    ScheduleService::new(gate, recorder.clone()).with_clock(clock.clone())
}

fn interval_request(every_seconds: u32, key_text: &str) -> CreateScheduleRequest {
    CreateScheduleRequest {
        workspace_id: "wks_test".to_owned(),
        name: ScheduleName::try_from("自动巡检".to_owned()).expect("name"),
        cadence: ScheduleCadence::Interval { every_seconds },
        trigger: ScheduleTrigger {
            agent_id: "agent-test".to_owned(),
            model_id: None,
            permission: None,
            text: "检查当前状态".to_owned(),
        },
        validity: ScheduleValidityWindow::default(),
        retry: ScheduleRetryPolicy::default(),
        idempotency_key: key(key_text),
        paused: false,
    }
}

fn cron_request(expression: &str, key_text: &str) -> CreateScheduleRequest {
    let mut request = interval_request(60, key_text);
    request.cadence = ScheduleCadence::Cron {
        expression: CronExpression::try_from(expression.to_owned()).expect("cron expression"),
        timezone: None,
    };
    request
}

fn create(
    service: &ScheduleService,
    request: CreateScheduleRequest,
) -> soxu_protocol::ScheduleSummary {
    match service
        .handle(ScheduleCommand::Create(request))
        .expect("create")
    {
        ScheduleCommandResult::Created(summary) => summary,
        other => panic!("expected Created, got {other:?}"),
    }
}

fn trigger_of(events: &[ScheduleEvent]) -> Vec<soxu_protocol::ScheduleTriggeredEvent> {
    events
        .iter()
        .filter_map(|event| match event {
            ScheduleEvent::Triggered(trigger) => Some(trigger.clone()),
            _ => None,
        })
        .collect()
}

fn run_result(
    schedule_id: &ScheduleId,
    run_id: &str,
    outcome: ScheduleRunOutcome,
    attempts: u8,
    finished_at: DateTime<Utc>,
) -> ScheduleRunResult {
    ScheduleRunResult {
        schedule_id: schedule_id.clone(),
        run_id: run_id.to_owned(),
        outcome,
        attempts,
        session_id: None,
        error: None,
        finished_at,
    }
}

#[test]
fn default_gate_rejects_schedule_and_heartbeat_without_events() {
    let recorder = Arc::new(Recorder::default());
    let clock = Arc::new(ManualClock::new(at(START)));
    let service = service(FeatureGate::disabled(), &recorder, &clock);

    let error = service
        .handle(ScheduleCommand::Create(interval_request(
            60,
            "closed-schedule",
        )))
        .expect_err("schedule must be disabled by default");
    assert_eq!(error.code, ErrorCode::Unsupported);

    let mut heartbeat = interval_request(60, "closed-heartbeat");
    heartbeat.cadence = ScheduleCadence::Heartbeat {
        every_seconds: 30,
        only_when_idle: true,
    };
    let error = service
        .handle(ScheduleCommand::Create(heartbeat))
        .expect_err("heartbeat must be disabled by default");
    assert_eq!(error.code, ErrorCode::Unsupported);
    assert!(service.tick().expect("closed tick").is_quiet());
    assert!(
        recorder.snapshot().is_empty(),
        "缺省关闭时一条事件也不能下发"
    );
}

#[test]
fn heartbeat_requires_its_own_capability_and_is_hidden_without_it() {
    let recorder = Arc::new(Recorder::default());
    let clock = Arc::new(ManualClock::new(at(START)));
    let service = service(
        FeatureGate::from_features([DaemonFeature::Schedules]),
        &recorder,
        &clock,
    );
    let mut heartbeat = interval_request(60, "heartbeat-capability");
    heartbeat.cadence = ScheduleCadence::Heartbeat {
        every_seconds: 30,
        only_when_idle: false,
    };
    let error = service
        .handle(ScheduleCommand::Create(heartbeat))
        .expect_err("schedules must not imply heartbeats");
    assert_eq!(error.code, ErrorCode::Unsupported);
    assert!(recorder.snapshot().is_empty());
}

#[test]
fn idle_only_heartbeat_waits_for_idle_and_skips_busy_occurrences() {
    let recorder = Arc::new(Recorder::default());
    let clock = Arc::new(ManualClock::new(at(START)));
    let service = service(
        FeatureGate::from_features([DaemonFeature::Heartbeats]),
        &recorder,
        &clock,
    );
    let mut request = interval_request(60, "idle-heartbeat");
    request.cadence = ScheduleCadence::Heartbeat {
        every_seconds: 30,
        only_when_idle: true,
    };
    let summary = create(&service, request);
    recorder.take();

    clock.set(at(START + 30));
    let busy = service.tick_with_idle(false).expect("busy tick");
    assert_eq!(busy.idle_skipped, 1);
    assert!(recorder.take().is_empty(), "忙碌时不下发心跳事件");
    assert_eq!(
        service
            .summary(&summary.schedule_id)
            .and_then(|item| item.next_run_at),
        Some(at(START + 60)),
        "跳过后按周期前进，不能积压重放"
    );

    clock.set(at(START + 60));
    let idle = service.tick_with_idle(true).expect("idle tick");
    assert_eq!(idle.triggered, 1);
    assert!(
        trigger_of(&recorder.take())
            .iter()
            .any(|trigger| trigger.trigger == soxu_protocol::ScheduleRunTrigger::Heartbeat)
    );
}

#[test]
fn create_and_manual_trigger_are_idempotent() {
    let recorder = Arc::new(Recorder::default());
    let clock = Arc::new(ManualClock::new(at(START)));
    let service = service(
        FeatureGate::from_features([DaemonFeature::Schedules]),
        &recorder,
        &clock,
    );
    let first = create(&service, interval_request(60, "create-once"));
    let second = create(&service, interval_request(300, "create-once"));
    assert_eq!(first.schedule_id, second.schedule_id);
    assert_eq!(first.cadence, second.cadence, "同键重复创建不得改写原定义");

    let request = TriggerScheduleRequest {
        schedule_id: first.schedule_id.clone(),
        idempotency_key: key("manual-once"),
    };
    let first_trigger = match service
        .handle(ScheduleCommand::TriggerNow(request.clone()))
        .expect("manual trigger")
    {
        ScheduleCommandResult::TriggerAccepted(trigger) => trigger,
        other => panic!("expected trigger, got {other:?}"),
    };
    let duplicate = match service
        .handle(ScheduleCommand::TriggerNow(request))
        .expect("duplicate manual trigger")
    {
        ScheduleCommandResult::TriggerAccepted(trigger) => trigger,
        other => panic!("expected trigger, got {other:?}"),
    };
    assert_eq!(first_trigger, duplicate);
    assert_eq!(trigger_of(&recorder.snapshot()).len(), 1);
}

#[test]
fn interval_tick_uses_injected_clock_and_skips_missed_intervals() {
    let recorder = Arc::new(Recorder::default());
    let clock = Arc::new(ManualClock::new(at(START)));
    let service = service(
        FeatureGate::from_features([DaemonFeature::Schedules]),
        &recorder,
        &clock,
    );
    let summary = create(&service, interval_request(60, "interval-tick"));
    recorder.take();
    assert_eq!(summary.next_run_at, Some(at(START + 60)));

    clock.set(at(START + 3 * 60 + 5));
    let report = service.tick().expect("due tick");
    assert_eq!(report.triggered, 1);
    let events = recorder.take();
    let triggers = trigger_of(&events);
    let [trigger] = triggers.as_slice() else {
        panic!("exactly one trigger expected: {events:?}");
    };
    assert_eq!(trigger.scheduled_for, at(START + 60));
    assert_eq!(trigger.attempt, 1);
    assert_eq!(
        service
            .summary(&summary.schedule_id)
            .and_then(|item| item.next_run_at),
        Some(at(START + 4 * 60)),
        "错过的周期不积压连发，跳到 now 之后的周期网格"
    );
    assert!(
        service
            .tick()
            .expect("same instant repeated tick")
            .is_quiet()
    );
    assert!(recorder.take().is_empty());
}

#[test]
fn cron_uses_fixed_utc_and_rejects_non_utc_timezone() {
    let recorder = Arc::new(Recorder::default());
    let clock = Arc::new(ManualClock::new(at(START)));
    let service = service(
        FeatureGate::from_features([DaemonFeature::Schedules]),
        &recorder,
        &clock,
    );
    let summary = create(&service, cron_request("0 3 * * *", "cron-utc"));
    assert!(summary.next_run_at.is_some());

    let mut request = cron_request("0 3 * * *", "cron-local");
    request.cadence = ScheduleCadence::Cron {
        expression: CronExpression::try_from("0 3 * * *".to_owned()).expect("expression"),
        timezone: Some(
            soxu_protocol::ScheduleTimezone::try_from("America/Los_Angeles".to_owned())
                .expect("timezone contract shape"),
        ),
    };
    let error = service
        .handle(ScheduleCommand::Create(request))
        .expect_err("fixed UTC contract rejects other zones");
    assert_eq!(error.code, ErrorCode::Unsupported);
    assert!(
        recorder
            .take()
            .iter()
            .all(|event| !matches!(event, ScheduleEvent::Triggered(_)))
    );
}

#[test]
fn retry_uses_same_run_id_and_scheduled_for_until_attempt_limit() {
    let recorder = Arc::new(Recorder::default());
    let clock = Arc::new(ManualClock::new(at(START)));
    let service = service(
        FeatureGate::from_features([DaemonFeature::Schedules]),
        &recorder,
        &clock,
    );
    let mut request = interval_request(60, "retry-policy");
    request.retry = ScheduleRetryPolicy {
        max_attempts: 3,
        backoff_seconds: 10,
    };
    let summary = create(&service, request);
    recorder.take();

    clock.set(at(START + 60));
    service.tick().expect("first tick");
    let first = trigger_of(&recorder.take()).pop().expect("first trigger");
    let retry = service
        .finish_run(run_result(
            &summary.schedule_id,
            &first.run_id,
            ScheduleRunOutcome::Failed,
            1,
            at(START + 61),
        ))
        .expect("arm retry");
    assert_eq!(
        retry,
        FinishOutcome::RetryArmed {
            due_at: at(START + 71),
            attempt: 2
        }
    );
    assert!(
        recorder
            .take()
            .iter()
            .all(|event| !matches!(event, ScheduleEvent::RunFinished(_))),
        "可重试失败先登记重试，不提前收敛 RunFinished"
    );

    clock.set(at(START + 70));
    assert!(service.tick().expect("before retry").is_quiet());
    clock.set(at(START + 71));
    service.tick().expect("retry tick");
    let second = trigger_of(&recorder.take()).pop().expect("retry trigger");
    assert_eq!(second.run_id, first.run_id);
    assert_eq!(second.scheduled_for, first.scheduled_for);
    assert_eq!(second.attempt, 2);

    let retry = service
        .finish_run(run_result(
            &summary.schedule_id,
            &second.run_id,
            ScheduleRunOutcome::TimedOut,
            2,
            at(START + 72),
        ))
        .expect("arm last retry");
    assert_eq!(
        retry,
        FinishOutcome::RetryArmed {
            due_at: at(START + 82),
            attempt: 3
        }
    );
    recorder.take();
    clock.set(at(START + 82));
    service.tick().expect("last attempt tick");
    let third = trigger_of(&recorder.take()).pop().expect("last trigger");
    assert_eq!(third.run_id, first.run_id);
    assert_eq!(third.attempt, 3);
    assert_eq!(
        service.finish_run(run_result(
            &summary.schedule_id,
            &third.run_id,
            ScheduleRunOutcome::Failed,
            3,
            at(START + 83),
        )),
        Ok(FinishOutcome::Converged),
        "达到上限后必须收敛"
    );
    assert!(
        recorder.take().iter().any(
            |event| matches!(event, ScheduleEvent::RunFinished(result) if result.attempts == 3)
        )
    );
}

#[test]
fn pause_clears_due_work_and_resume_recomputes_from_current_time() {
    let recorder = Arc::new(Recorder::default());
    let clock = Arc::new(ManualClock::new(at(START)));
    let service = service(
        FeatureGate::from_features([DaemonFeature::Schedules]),
        &recorder,
        &clock,
    );
    let summary = create(&service, interval_request(60, "pause-resume"));
    recorder.take();
    service
        .handle(ScheduleCommand::Pause(PauseScheduleRequest {
            schedule_id: summary.schedule_id.clone(),
            paused: true,
        }))
        .expect("pause");
    recorder.take();
    clock.set(at(START + 600));
    assert!(service.tick().expect("paused tick").is_quiet());
    assert!(recorder.take().is_empty());
    assert_eq!(
        service
            .summary(&summary.schedule_id)
            .expect("summary")
            .next_run_at,
        None
    );

    service
        .handle(ScheduleCommand::Pause(PauseScheduleRequest {
            schedule_id: summary.schedule_id.clone(),
            paused: false,
        }))
        .expect("resume");
    let resumed = service
        .summary(&summary.schedule_id)
        .expect("resumed summary");
    assert_eq!(resumed.state, ScheduleState::Active);
    assert_eq!(resumed.next_run_at, Some(at(START + 660)));
}

#[test]
fn delete_clears_definition_and_duplicate_keys_and_never_reemits() {
    let recorder = Arc::new(Recorder::default());
    let clock = Arc::new(ManualClock::new(at(START)));
    let service = service(
        FeatureGate::from_features([DaemonFeature::Schedules]),
        &recorder,
        &clock,
    );
    let summary = create(&service, interval_request(60, "delete-definition"));
    recorder.take();
    service
        .handle(ScheduleCommand::Delete(DeleteScheduleRequest {
            schedule_id: summary.schedule_id.clone(),
        }))
        .expect("delete");
    let removed = recorder.take();
    assert!(removed.iter().any(|event| matches!(event, ScheduleEvent::Removed { schedule_id } if schedule_id == &summary.schedule_id)));
    assert!(service.is_empty());
    let recreated = create(&service, interval_request(60, "delete-definition"));
    assert_ne!(
        recreated.schedule_id, summary.schedule_id,
        "删除时同步清理创建幂等索引"
    );
}

#[test]
fn list_only_exposes_declared_capability_families() {
    let recorder = Arc::new(Recorder::default());
    let clock = Arc::new(ManualClock::new(at(START)));
    let service = service(
        FeatureGate::from_features([DaemonFeature::Schedules]),
        &recorder,
        &clock,
    );
    let schedule = create(&service, interval_request(60, "visible-interval"));
    let listed = service
        .handle(ScheduleCommand::List(ListSchedulesRequest {
            workspace_id: "wks_test".to_owned(),
        }))
        .expect("list");
    let ScheduleCommandResult::Listed(listed) = listed else {
        panic!("expected list result");
    };
    assert_eq!(listed.schedules.len(), 1);
    assert_eq!(listed.schedules[0].schedule_id, schedule.schedule_id);
}

#[test]
fn persisted_schedule_survives_service_recreation() {
    let recorder = Arc::new(Recorder::default());
    let clock = Arc::new(ManualClock::new(at(START)));
    let temp = TempDir::new().expect("temp dir");
    let path = temp.path().join("schedules.json");
    let gate = FeatureGate::from_features([DaemonFeature::Schedules]);
    let first = ScheduleService::new(gate.clone(), recorder.clone())
        .with_clock(clock.clone())
        .with_store(path.clone())
        .expect("load empty");
    let created = create(&first, interval_request(60, "persisted-definition"));
    recorder.take();

    let second = ScheduleService::new(gate, recorder.clone())
        .with_clock(clock)
        .with_store(path.clone())
        .expect("load persisted");
    assert_eq!(second.len(), 1);
    assert_eq!(second.summary(&created.schedule_id), Some(created.clone()));
    assert!(
        ScheduleStore::new(path)
            .load()
            .expect("valid store")
            .contains_key(&created.schedule_id)
    );
}

#[test]
fn validity_expiry_converges_to_expired_without_trigger() {
    let recorder = Arc::new(Recorder::default());
    let clock = Arc::new(ManualClock::new(at(START)));
    let service = service(
        FeatureGate::from_features([DaemonFeature::Schedules]),
        &recorder,
        &clock,
    );
    let mut request = interval_request(60, "expires");
    request.validity.expires_at = Some(at(START + 30));
    let summary = create(&service, request);
    recorder.take();
    clock.advance(Duration::seconds(30));
    let report = service.tick().expect("expiry tick");
    assert_eq!(report.expired, 1);
    assert_eq!(
        service
            .summary(&summary.schedule_id)
            .expect("summary")
            .state,
        ScheduleState::Expired
    );
    assert!(trigger_of(&recorder.take()).is_empty());
}

#[test]
fn disabled_schedule_ability_does_not_send_events_from_restored_entries() {
    let recorder = Arc::new(Recorder::default());
    let clock = Arc::new(ManualClock::new(at(START)));
    let temp = TempDir::new().expect("temp dir");
    let path = temp.path().join("schedules.json");
    let enabled = ScheduleService::new(
        FeatureGate::from_features([DaemonFeature::Schedules]),
        recorder.clone(),
    )
    .with_clock(clock.clone())
    .with_store(path.clone())
    .expect("load empty");
    create(&enabled, interval_request(60, "restore-gated"));
    recorder.take();

    let disabled = ScheduleService::new(FeatureGate::disabled(), recorder.clone())
        .with_clock(clock.clone())
        .with_store(path)
        .expect("restore definition independently of gate");
    clock.set(at(START + 60));
    let report = disabled.tick().expect("gated tick");
    assert_eq!(report.gated, 1);
    assert!(recorder.take().is_empty(), "恢复后的定义仍受能力门控保护");
}

#[test]
fn paused_or_expired_manual_trigger_is_rejected_without_trigger_event() {
    let recorder = Arc::new(Recorder::default());
    let clock = Arc::new(ManualClock::new(at(START)));
    let service = service(
        FeatureGate::from_features([DaemonFeature::Schedules]),
        &recorder,
        &clock,
    );
    let summary = create(&service, interval_request(60, "manual-pause"));
    recorder.take();
    service
        .handle(ScheduleCommand::Pause(PauseScheduleRequest {
            schedule_id: summary.schedule_id.clone(),
            paused: true,
        }))
        .expect("pause");
    recorder.take();
    let error = service
        .handle(ScheduleCommand::TriggerNow(TriggerScheduleRequest {
            schedule_id: summary.schedule_id,
            idempotency_key: key("manual-paused"),
        }))
        .expect_err("paused schedule cannot trigger");
    assert_eq!(error.code, ErrorCode::Conflict);
    assert!(trigger_of(&recorder.take()).is_empty());
}

#[test]
fn malformed_heartbeat_interval_is_rejected_before_creation() {
    let recorder = Arc::new(Recorder::default());
    let clock = Arc::new(ManualClock::new(at(START)));
    let service = service(
        FeatureGate::from_features([DaemonFeature::Heartbeats]),
        &recorder,
        &clock,
    );
    let mut request = interval_request(60, "bad-heartbeat");
    request.cadence = ScheduleCadence::Heartbeat {
        every_seconds: 29,
        only_when_idle: false,
    };
    let error: ActionableError = service
        .handle(ScheduleCommand::Create(request))
        .expect_err("heartbeat lower bound is 30s");
    assert_eq!(error.code, ErrorCode::ValidationFailed);
    assert!(service.is_empty());
    assert!(recorder.snapshot().is_empty());
}

#[test]
fn create_request_is_protocol_shaped_and_uses_service_generated_identity() {
    let recorder = Arc::new(Recorder::default());
    let clock = Arc::new(ManualClock::new(at(START)));
    let service = service(
        FeatureGate::from_features([DaemonFeature::Schedules]),
        &recorder,
        &clock,
    );
    let summary = create(&service, interval_request(60, "server-id"));
    assert!(summary.schedule_id.as_str().starts_with("sch_"));
    assert_ne!(
        summary.schedule_id,
        ScheduleId::try_from("client".to_owned()).expect("id")
    );
}

#[test]
fn api_errors_remain_actionable_and_non_mutating() {
    let recorder = Arc::new(Recorder::default());
    let clock = Arc::new(ManualClock::new(at(START)));
    let service = service(
        FeatureGate::from_features([DaemonFeature::Schedules]),
        &recorder,
        &clock,
    );
    let missing = ScheduleId::try_from("missing".to_owned()).expect("id");
    let error = service
        .handle(ScheduleCommand::Delete(DeleteScheduleRequest {
            schedule_id: missing,
        }))
        .expect_err("missing delete");
    assert_eq!(error.code, ErrorCode::NotFound);
    assert!(service.is_empty());
    assert!(recorder.snapshot().is_empty());
}
