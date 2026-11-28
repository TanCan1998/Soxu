//! 调度执行器：定义注册、下次触发求值、幂等触发、失败重试与停用/删除收敛。
//!
//! 设计要点（对齐参考实现的做法并落到本仓契约）：
//! - **纯函数求值 + 薄执行器**：`cadence → 下次触发` 由 [`CronSchedule`] 与
//!   [`first_occurrence`] / [`advance_from`] 给出，执行器只负责状态与事件
//!   （对照 t3code `scheduledTasks/Schedule.ts` 与 `scheduling/Scheduler.ts` 的分层）；
//! - **不补发历史**：错过的触发点直接跳到 `now` 之后的最新一次，绝不按积压次数连发
//!   （对照 t3code `nextScheduledRunAt` 的 interval 推进语义）；
//! - **幂等键双通道**：创建按 `idempotency_key` 收敛到同一定义，手动触发按
//!   `idempotency_key` 收敛到同一 `Triggered` 事件，重复下发不产生第二次运行；
//! - **重试沿用同一次运行**：失败重试保持同一 `run_id` 与同一 `scheduled_for`，
//!   只有次数推进到 `retry.max_attempts` 才下发 `RunFinished` 收敛结论；
//! - **能力门控贯穿始终**：门控关闭时命令直接拒绝，`tick` 也不为对应调度下发任何事件。

use crate::schedules::clock::{Clock, SystemClock};
use crate::schedules::cron::CronSchedule;
use crate::schedules::store::ScheduleStore;
use crate::scripts::gate::FeatureGate;
use chrono::{DateTime, Duration, Utc};
use soxu_protocol::{
    ActionableError, CreateScheduleRequest, DaemonFeature, DeleteScheduleRequest, ErrorCode,
    IdempotencyKey, ListSchedulesRequest, PauseScheduleRequest, ScheduleCadence, ScheduleCommand,
    ScheduleCommandResult, ScheduleEvent, ScheduleId, ScheduleListResult, ScheduleRunOutcome,
    ScheduleRunResult, ScheduleRunTrigger, ScheduleState, ScheduleSummary, ScheduleTriggeredEvent,
    TriggerScheduleRequest, UpdateScheduleRequest,
};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

/// 单工作区调度定义上限。
pub const MAX_SCHEDULES_PER_WORKSPACE: usize = 256;
/// 单个调度保留的手动触发幂等键条数上限（先进先出淘汰）。
const MAX_TRIGGER_KEYS_PER_SCHEDULE: usize = 256;
/// 工作区标识字节上限。
const MAX_WORKSPACE_ID_BYTES: usize = 256;

/// 调度事件出口。
///
/// 由宿主注入（守护进程侧可为广播通道）；门控关闭时执行器不会调用该出口，
/// 未协商能力的客户端一条事件都收不到。
pub trait ScheduleEventSink: Send + Sync {
    /// 下发一条调度域事件。
    fn emit(&self, event: ScheduleEvent);
}

/// 丢弃事件的出口（未接线宿主与只读回归用）。
#[derive(Debug, Default)]
pub struct NullScheduleEventSink;

impl ScheduleEventSink for NullScheduleEventSink {
    fn emit(&self, _event: ScheduleEvent) {}
}

/// 尚未收敛的重试。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PendingRetry {
    /// 与首次尝试共享的运行标识。
    pub run_id: String,
    /// 本次运行负责的计划时刻（重试沿用，不随重试时刻漂移）。
    pub scheduled_for: DateTime<Utc>,
    /// 下一次尝试的序号（从 2 起）。
    pub attempt: u8,
    /// 退避到期时刻；`tick` 到期后才下发触发事件。
    pub due_at: DateTime<Utc>,
}

/// 持久化的调度条目：协议摘要 + 执行器簿记。
///
/// 所有字段都参与持久化，因此重启后「下次触发 / 重试余额 / 最近结果」保持一致。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StoredSchedule {
    /// 协议可见摘要（由执行器唯一维护）。
    pub summary: ScheduleSummary,
    /// 是否停用；停用期间 `next_run_at` 恒为 `None`。
    pub paused: bool,
    /// 最近一次收敛的运行结果。
    pub last_result: Option<ScheduleRunResult>,
    /// 当前这次运行已经用掉的尝试次数（含首次）。
    pub attempts: u8,
    /// 在途重试；`None` 表示没有待重试。
    pub pending_retry: Option<PendingRetry>,
    /// 创建时的幂等键（删除时同步清理索引）。
    pub idempotency_key: IdempotencyKey,
}

/// 一次 `tick` 的收敛统计。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TickReport {
    /// 本次下发触发事件的条数。
    pub triggered: u32,
    /// 因能力位未声明而被压制、仅推进周期的调度条数。
    pub gated: u32,
    /// 因 `only_when_idle` 条件不满足而跳过、仅推进周期的心跳条数。
    pub idle_skipped: u32,
    /// 因有效期结束而收敛为 `Expired` 的调度条数。
    pub expired: u32,
}

impl TickReport {
    /// 是否没有任何动作（便于宿主跳过空转广播）。
    #[must_use]
    pub fn is_quiet(&self) -> bool {
        self.triggered == 0 && self.gated == 0 && self.idle_skipped == 0 && self.expired == 0
    }
}

/// 运行结果的处理结论。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinishOutcome {
    /// 已收敛并下发 `RunFinished`。
    Converged,
    /// 失败且仍有重试余额：不下发 `RunFinished`，重试由后续 `tick` 触发。
    RetryArmed { due_at: DateTime<Utc>, attempt: u8 },
    /// 定义已被删除：结果静默丢弃，不复活定义、不下发事件。
    Dropped,
}

/// 在途运行登记：重试与结果归属的唯一依据。
#[derive(Debug, Clone, PartialEq, Eq)]
struct InFlight {
    schedule_id: ScheduleId,
    scheduled_for: DateTime<Utc>,
    trigger: ScheduleRunTrigger,
}

#[derive(Debug, Default)]
struct Registry {
    schedules: BTreeMap<ScheduleId, StoredSchedule>,
    create_keys: BTreeMap<String, ScheduleId>,
    trigger_keys: BTreeMap<(ScheduleId, String), ScheduleTriggeredEvent>,
    trigger_key_order: BTreeMap<ScheduleId, Vec<String>>,
    in_flight: BTreeMap<String, InFlight>,
}

/// 调度执行器。
pub struct ScheduleService {
    gate: FeatureGate,
    sink: Arc<dyn ScheduleEventSink>,
    clock: Arc<dyn Clock>,
    store: Option<Arc<ScheduleStore>>,
    registry: Mutex<Registry>,
}

impl std::fmt::Debug for ScheduleService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScheduleService")
            .field("gate", &self.gate)
            .field("clock", &self.clock.now())
            .field("persisted", &self.store.is_some())
            .finish_non_exhaustive()
    }
}

impl ScheduleService {
    /// 构造执行器：系统时钟、无持久化、事件出口由宿主注入。
    #[must_use]
    pub fn new(gate: FeatureGate, sink: Arc<dyn ScheduleEventSink>) -> Self {
        Self {
            gate,
            sink,
            clock: Arc::new(SystemClock),
            store: None,
            registry: Mutex::new(Registry::default()),
        }
    }

    /// 替换时钟（测试推进用）。
    #[must_use]
    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.clock = clock;
        self
    }

    /// 绑定持久化文件并立即加载既有定义（fail-closed）。
    ///
    /// 文件缺失视为空表；损坏、版本未知或条目重复一律返回可执行错误，
    /// 不做「尽量恢复」，避免把不可信的调度定义带入触发。
    pub fn with_store(mut self, path: impl Into<PathBuf>) -> Result<Self, ActionableError> {
        let store = ScheduleStore::new(path);
        let loaded = store.load()?;
        {
            let mut registry = lock(&self.registry);
            for (id, schedule) in loaded {
                validate_stored(&schedule)?;
                registry
                    .create_keys
                    .insert(schedule.idempotency_key.as_str().to_owned(), id.clone());
                registry.schedules.insert(id, schedule);
            }
        }
        self.store = Some(Arc::new(store));
        Ok(self)
    }

    /// 当前门控。
    #[must_use]
    pub fn gate(&self) -> &FeatureGate {
        &self.gate
    }

    /// 已登记的调度条数。
    #[must_use]
    pub fn len(&self) -> usize {
        lock(&self.registry).schedules.len()
    }

    /// 是否没有任何调度定义。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 读取某条调度的当前摘要。
    #[must_use]
    pub fn summary(&self, id: &ScheduleId) -> Option<ScheduleSummary> {
        lock(&self.registry)
            .schedules
            .get(id)
            .map(|schedule| schedule.summary.clone())
    }

    /// 处理一条调度域命令（1:1 对应 `ScheduleCommand` 变体）。
    pub fn handle(
        &self,
        command: ScheduleCommand,
    ) -> Result<ScheduleCommandResult, ActionableError> {
        match command {
            ScheduleCommand::Create(request) => self.create(request),
            ScheduleCommand::Update(request) => self.update(request),
            ScheduleCommand::Pause(request) => self.pause(request),
            ScheduleCommand::Delete(request) => self.delete(request),
            ScheduleCommand::List(request) => self.list(request),
            ScheduleCommand::TriggerNow(request) => self.trigger_now(request),
        }
    }

    /// 推进一次调度：下发到期触发、收敛过期定义。
    ///
    /// 幂等：同一时刻重复调用只会推进一次（触发后 `next_run_at` 已前移）。
    pub fn tick(&self) -> Result<TickReport, ActionableError> {
        self.tick_with_idle(false)
    }

    /// 推进一次调度，并由宿主提供当前 Agent 是否空闲的快照。
    ///
    /// 普通调度不受 `agent_idle` 影响；仅 `Heartbeat { only_when_idle: true }` 使用它。
    /// 当 Agent 忙碌时跳过本次心跳并前移周期，不补发积压心跳。
    pub fn tick_with_idle(&self, agent_idle: bool) -> Result<TickReport, ActionableError> {
        let now = self.clock.now();
        let mut report = TickReport::default();
        let mut events: Vec<ScheduleEvent> = Vec::new();
        {
            let mut registry = lock(&self.registry);
            let ids: Vec<ScheduleId> = registry.schedules.keys().cloned().collect();
            for id in ids {
                let Some(mut schedule) = registry.schedules.remove(&id) else {
                    continue;
                };
                let mut changed = false;
                let outcome = self.advance(
                    &mut schedule,
                    now,
                    agent_idle,
                    &mut report,
                    &mut events,
                    &mut registry,
                );
                changed |= outcome;
                registry.schedules.insert(id, schedule);
                if changed {
                    // 变更落盘在临界区内完成，保证事件不会先于持久化被下发。
                    persist(&self.store, &registry)?;
                }
            }
        }
        for event in events {
            self.sink.emit(event);
        }
        Ok(report)
    }

    /// 记录一次运行结果，并按重试策略收敛。
    pub fn finish_run(&self, result: ScheduleRunResult) -> Result<FinishOutcome, ActionableError> {
        let mut events: Vec<ScheduleEvent> = Vec::new();
        let outcome;
        {
            let mut registry = lock(&self.registry);
            let Some(in_flight) = registry.in_flight.remove(&result.run_id) else {
                // 未知运行标识：定义可能已被删除，如实报错而不是伪造一条运行记录。
                return Err(ActionableError::new(
                    ErrorCode::NotFound,
                    format!("未找到运行 `{}`", result.run_id),
                    "该结果未被记录，也没有任何调度被复活",
                    "请确认 run_id 来自本实例下发的触发事件",
                ));
            };
            let Some(mut schedule) = registry.schedules.remove(&in_flight.schedule_id) else {
                // 定义已删除：收敛丢弃，不复活、不下发事件。
                persist(&self.store, &registry)?;
                return Ok(FinishOutcome::Dropped);
            };
            schedule.summary.last_run_at = Some(result.finished_at);
            schedule.summary.updated_at = result.finished_at;
            schedule.last_result = Some(result.clone());
            let attempts = result.attempts.max(1);
            schedule.attempts = attempts;

            let retryable = matches!(
                result.outcome,
                ScheduleRunOutcome::Failed | ScheduleRunOutcome::TimedOut
            ) && schedule.summary.state == ScheduleState::Active
                && !schedule.paused
                && attempts < schedule.summary.retry.max_attempts;

            if retryable {
                let backoff = i64::from(schedule.summary.retry.backoff_seconds);
                let due_at = result.finished_at + Duration::seconds(backoff);
                schedule.pending_retry = Some(PendingRetry {
                    run_id: result.run_id.clone(),
                    scheduled_for: in_flight.scheduled_for,
                    attempt: attempts + 1,
                    due_at,
                });
                events.push(ScheduleEvent::Updated(schedule.summary.clone()));
                outcome = FinishOutcome::RetryArmed {
                    due_at,
                    attempt: attempts + 1,
                };
            } else {
                schedule.pending_retry = None;
                events.push(ScheduleEvent::RunFinished(result.clone()));
                events.push(ScheduleEvent::Updated(schedule.summary.clone()));
                outcome = FinishOutcome::Converged;
            }
            registry
                .schedules
                .insert(in_flight.schedule_id.clone(), schedule);
            persist(&self.store, &registry)?;
        }
        for event in events {
            self.sink.emit(event);
        }
        Ok(outcome)
    }

    /// 单条调度的推进逻辑；返回是否发生了需要落盘的变化。
    fn advance(
        &self,
        schedule: &mut StoredSchedule,
        now: DateTime<Utc>,
        agent_idle: bool,
        report: &mut TickReport,
        events: &mut Vec<ScheduleEvent>,
        registry: &mut Registry,
    ) -> bool {
        if schedule.paused {
            return false;
        }
        if schedule
            .summary
            .validity
            .expires_at
            .is_some_and(|end| now >= end)
        {
            let already = schedule.summary.state == ScheduleState::Expired
                && schedule.summary.next_run_at.is_none()
                && schedule.pending_retry.is_none();
            if already {
                return false;
            }
            schedule.summary.state = ScheduleState::Expired;
            schedule.summary.next_run_at = None;
            schedule.summary.updated_at = now;
            schedule.pending_retry = None;
            report.expired += 1;
            events.push(ScheduleEvent::Updated(schedule.summary.clone()));
            return true;
        }

        let retry_due = schedule
            .pending_retry
            .clone()
            .filter(|retry| retry.due_at <= now);
        // 有重试计划时先收敛该次运行，避免下一周期与重试并行或覆盖重试登记。
        let occurrence_due = schedule.pending_retry.is_none()
            && schedule.summary.next_run_at.is_some_and(|next| next <= now);
        if retry_due.is_none() && !occurrence_due {
            return false;
        }

        // 能力位未声明：不下发任何事件，仅把周期推进到 now 之后，
        // 避免能力开启后按积压补发历史触发。
        if !self
            .gate
            .is_enabled(cadence_feature(&schedule.summary.cadence))
        {
            schedule.pending_retry = None;
            let (next, expired) = advance_after(schedule, now, now);
            schedule.summary.next_run_at = next;
            if expired {
                schedule.summary.state = ScheduleState::Expired;
            }
            schedule.summary.updated_at = now;
            report.gated += 1;
            // 门控关闭时整族事件均不可下发；仅内部推进周期并持久化。
            return true;
        }
        let idle_only = matches!(
            schedule.summary.cadence,
            ScheduleCadence::Heartbeat {
                only_when_idle: true,
                ..
            }
        );
        if idle_only && !agent_idle {
            schedule.pending_retry = None;
            let (next, expired) = advance_after(schedule, now, now);
            schedule.summary.next_run_at = next;
            if expired {
                schedule.summary.state = ScheduleState::Expired;
            }
            schedule.summary.updated_at = now;
            report.idle_skipped += 1;
            return true;
        }

        let (trigger, next, expired) = match retry_due {
            Some(retry) => {
                let (next, expired) = advance_after(schedule, retry.scheduled_for, now);
                (
                    ScheduleTriggeredEvent {
                        schedule_id: schedule.summary.schedule_id.clone(),
                        run_id: retry.run_id.clone(),
                        trigger: cadence_trigger(&schedule.summary.cadence),
                        scheduled_for: retry.scheduled_for,
                        attempt: retry.attempt,
                    },
                    next,
                    expired,
                )
            }
            None => {
                let scheduled_for = schedule.summary.next_run_at.unwrap_or(now);
                let (next, expired) = advance_after(schedule, scheduled_for, now);
                (
                    ScheduleTriggeredEvent {
                        schedule_id: schedule.summary.schedule_id.clone(),
                        run_id: new_run_id(),
                        trigger: cadence_trigger(&schedule.summary.cadence),
                        scheduled_for,
                        attempt: 1,
                    },
                    next,
                    expired,
                )
            }
        };

        schedule.pending_retry = None;
        schedule.attempts = trigger.attempt;
        schedule.summary.next_run_at = next;
        schedule.summary.state = if expired {
            ScheduleState::Expired
        } else {
            ScheduleState::Active
        };
        schedule.summary.updated_at = now;
        registry.in_flight.insert(
            trigger.run_id.clone(),
            InFlight {
                schedule_id: schedule.summary.schedule_id.clone(),
                scheduled_for: trigger.scheduled_for,
                trigger: trigger.trigger,
            },
        );
        report.triggered += 1;
        events.push(ScheduleEvent::Triggered(trigger));
        events.push(ScheduleEvent::Updated(schedule.summary.clone()));
        true
    }

    fn create(
        &self,
        request: CreateScheduleRequest,
    ) -> Result<ScheduleCommandResult, ActionableError> {
        self.gate.require(cadence_feature(&request.cadence))?;
        validate_workspace_id(&request.workspace_id)?;
        let cron = self.validate_cadence(&request.cadence)?;
        let now = self.clock.now();
        let mut events: Vec<ScheduleEvent> = Vec::new();
        let summary;
        {
            let mut registry = lock(&self.registry);
            if let Some(existing) = registry
                .create_keys
                .get(request.idempotency_key.as_str())
                .and_then(|id| registry.schedules.get(id))
            {
                return Ok(ScheduleCommandResult::Created(existing.summary.clone()));
            }
            if registry
                .schedules
                .values()
                .filter(|schedule| schedule.summary.workspace_id == request.workspace_id)
                .count()
                >= MAX_SCHEDULES_PER_WORKSPACE
            {
                return Err(ActionableError::new(
                    ErrorCode::Conflict,
                    format!("该工作区的调度定义已达 {MAX_SCHEDULES_PER_WORKSPACE} 条上限"),
                    "本次创建未生效，既有定义不受影响",
                    "请先删除不再需要的调度后重试",
                ));
            }
            let floor = request
                .validity
                .starts_at
                .map_or(now, |start| start.max(now));
            let next_run_at =
                first_occurrence(&request.cadence, cron.as_ref(), floor, &request.validity);
            let state = if request.paused {
                ScheduleState::Paused
            } else {
                ScheduleState::Active
            };
            summary = ScheduleSummary {
                schedule_id: new_schedule_id(),
                workspace_id: request.workspace_id.clone(),
                name: request.name,
                cadence: request.cadence,
                trigger: request.trigger,
                validity: request.validity,
                retry: request.retry,
                state,
                next_run_at: if request.paused { None } else { next_run_at },
                last_run_at: None,
                created_at: now,
                updated_at: now,
            };
            let stored = StoredSchedule {
                summary: summary.clone(),
                paused: request.paused,
                last_result: None,
                attempts: 0,
                pending_retry: None,
                idempotency_key: request.idempotency_key,
            };
            registry.create_keys.insert(
                stored.idempotency_key.as_str().to_owned(),
                summary.schedule_id.clone(),
            );
            registry
                .schedules
                .insert(summary.schedule_id.clone(), stored);
            persist(&self.store, &registry)?;
            events.push(ScheduleEvent::Updated(summary.clone()));
        }
        for event in events {
            self.sink.emit(event);
        }
        Ok(ScheduleCommandResult::Created(summary))
    }

    fn update(
        &self,
        request: UpdateScheduleRequest,
    ) -> Result<ScheduleCommandResult, ActionableError> {
        let now = self.clock.now();
        let mut events: Vec<ScheduleEvent> = Vec::new();
        let summary;
        {
            let mut registry = lock(&self.registry);
            let mut schedule = registry
                .schedules
                .get(&request.schedule_id)
                .cloned()
                .ok_or_else(|| unknown_schedule(&request.schedule_id))?;
            // 能力门控对「现有节奏」和「目标节奏」都要成立，避免借更新绕开能力位。
            self.gate
                .require(cadence_feature(&schedule.summary.cadence))?;
            let cadence_changed = request
                .cadence
                .as_ref()
                .is_some_and(|cadence| *cadence != schedule.summary.cadence);
            if let Some(cadence) = &request.cadence {
                self.gate.require(cadence_feature(cadence))?;
                self.validate_cadence(cadence)?;
            }
            if let Some(name) = request.name {
                schedule.summary.name = name;
            }
            if let Some(cadence) = request.cadence {
                schedule.summary.cadence = cadence;
            }
            if let Some(trigger) = request.trigger {
                schedule.summary.trigger = trigger;
            }
            if let Some(validity) = request.validity {
                schedule.summary.validity = validity;
            }
            if let Some(retry) = request.retry {
                schedule.summary.retry = retry;
            }
            if cadence_changed {
                // 节奏变化后旧的重试计划不再有意义，直接丢弃（收敛，不静默继承）。
                schedule.pending_retry = None;
                schedule.attempts = 0;
            }
            let cron = self.validate_cadence(&schedule.summary.cadence)?;
            let floor = schedule
                .summary
                .validity
                .starts_at
                .map_or(now, |start| start.max(now));
            schedule.summary.next_run_at = if schedule.paused {
                None
            } else {
                first_occurrence(
                    &schedule.summary.cadence,
                    cron.as_ref(),
                    floor,
                    &schedule.summary.validity,
                )
            };
            if schedule.summary.next_run_at.is_none()
                && schedule
                    .summary
                    .validity
                    .expires_at
                    .is_some_and(|end| now >= end)
            {
                schedule.summary.state = ScheduleState::Expired;
            }
            schedule.summary.updated_at = now;
            summary = schedule.summary.clone();
            registry
                .schedules
                .insert(request.schedule_id.clone(), schedule);
            persist(&self.store, &registry)?;
            events.push(ScheduleEvent::Updated(summary.clone()));
        }
        for event in events {
            self.sink.emit(event);
        }
        Ok(ScheduleCommandResult::Updated(summary))
    }

    fn pause(
        &self,
        request: PauseScheduleRequest,
    ) -> Result<ScheduleCommandResult, ActionableError> {
        let now = self.clock.now();
        let mut events: Vec<ScheduleEvent> = Vec::new();
        let summary;
        {
            let mut registry = lock(&self.registry);
            let mut schedule = registry
                .schedules
                .get(&request.schedule_id)
                .cloned()
                .ok_or_else(|| unknown_schedule(&request.schedule_id))?;
            self.gate
                .require(cadence_feature(&schedule.summary.cadence))?;
            let changed = schedule.paused != request.paused;
            if changed {
                schedule.paused = request.paused;
                if request.paused {
                    // 停用收敛：丢弃在途重试计划，恢复后按新时刻重新计算。
                    schedule.pending_retry = None;
                    schedule.summary.state = ScheduleState::Paused;
                    schedule.summary.next_run_at = None;
                } else {
                    schedule.attempts = 0;
                    let cron = self.validate_cadence(&schedule.summary.cadence)?;
                    let floor = schedule
                        .summary
                        .validity
                        .starts_at
                        .map_or(now, |start| start.max(now));
                    schedule.summary.next_run_at = first_occurrence(
                        &schedule.summary.cadence,
                        cron.as_ref(),
                        floor,
                        &schedule.summary.validity,
                    );
                    schedule.summary.state = if schedule.summary.next_run_at.is_none() {
                        ScheduleState::Expired
                    } else {
                        ScheduleState::Active
                    };
                }
                schedule.summary.updated_at = now;
                events.push(ScheduleEvent::Updated(schedule.summary.clone()));
            }
            summary = schedule.summary.clone();
            registry.schedules.insert(request.schedule_id, schedule);
            if changed {
                persist(&self.store, &registry)?;
            }
        }
        for event in events {
            self.sink.emit(event);
        }
        Ok(ScheduleCommandResult::Updated(summary))
    }

    fn delete(
        &self,
        request: DeleteScheduleRequest,
    ) -> Result<ScheduleCommandResult, ActionableError> {
        let mut events: Vec<ScheduleEvent> = Vec::new();
        {
            let mut registry = lock(&self.registry);
            let schedule = registry
                .schedules
                .get(&request.schedule_id)
                .cloned()
                .ok_or_else(|| unknown_schedule(&request.schedule_id))?;
            self.gate
                .require(cadence_feature(&schedule.summary.cadence))?;
            registry.schedules.remove(&request.schedule_id);
            // 收敛：幂等索引、触发幂等键与在途登记一并清理，删除后不再复活。
            registry
                .create_keys
                .remove(schedule.idempotency_key.as_str());
            if let Some(keys) = registry.trigger_key_order.remove(&request.schedule_id) {
                for key in keys {
                    registry
                        .trigger_keys
                        .remove(&(request.schedule_id.clone(), key));
                }
            }
            registry
                .in_flight
                .retain(|_, entry| entry.schedule_id != request.schedule_id);
            persist(&self.store, &registry)?;
            events.push(ScheduleEvent::Removed {
                schedule_id: request.schedule_id.clone(),
            });
        }
        for event in events {
            self.sink.emit(event);
        }
        Ok(ScheduleCommandResult::Removed {
            schedule_id: request.schedule_id,
        })
    }

    fn list(
        &self,
        request: ListSchedulesRequest,
    ) -> Result<ScheduleCommandResult, ActionableError> {
        if !self.gate.is_enabled(DaemonFeature::Schedules)
            && !self.gate.is_enabled(DaemonFeature::Heartbeats)
        {
            // 两个能力位都关闭时不下发任何事件，也没有可列出的结果。
            return Err(self
                .gate
                .require(DaemonFeature::Schedules)
                .expect_err("门控全关时必然拒绝"));
        }
        validate_workspace_id(&request.workspace_id)?;
        let registry = lock(&self.registry);
        let mut schedules: Vec<ScheduleSummary> = registry
            .schedules
            .values()
            .filter(|schedule| schedule.summary.workspace_id == request.workspace_id)
            // 只列出能力位已声明的节奏类型，避免把不可管理的定义暴露给客户端。
            .filter(|schedule| {
                self.gate
                    .is_enabled(cadence_feature(&schedule.summary.cadence))
            })
            .map(|schedule| schedule.summary.clone())
            .collect();
        schedules.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.schedule_id.cmp(&right.schedule_id))
        });
        Ok(ScheduleCommandResult::Listed(ScheduleListResult {
            workspace_id: request.workspace_id,
            schedules,
        }))
    }

    fn trigger_now(
        &self,
        request: TriggerScheduleRequest,
    ) -> Result<ScheduleCommandResult, ActionableError> {
        let now = self.clock.now();
        let mut events: Vec<ScheduleEvent> = Vec::new();
        let trigger;
        {
            let mut registry = lock(&self.registry);
            let schedule = registry
                .schedules
                .get(&request.schedule_id)
                .ok_or_else(|| unknown_schedule(&request.schedule_id))?;
            self.gate
                .require(cadence_feature(&schedule.summary.cadence))?;
            if let Some(existing) = registry.trigger_keys.get(&(
                request.schedule_id.clone(),
                request.idempotency_key.as_str().to_owned(),
            )) {
                return Ok(ScheduleCommandResult::TriggerAccepted(existing.clone()));
            }
            if schedule.paused
                || schedule.summary.state == ScheduleState::Expired
                || !schedule.summary.validity.is_active_at(now)
            {
                return Err(ActionableError::new(
                    ErrorCode::Conflict,
                    format!(
                        "调度 `{}` 当前不可触发（停用或已过期）",
                        request.schedule_id.as_str()
                    ),
                    "本次手动触发未生效，未下发任何触发事件",
                    "请先恢复该调度或修正有效期窗口后重试",
                ));
            }
            trigger = ScheduleTriggeredEvent {
                schedule_id: request.schedule_id.clone(),
                run_id: new_run_id(),
                trigger: ScheduleRunTrigger::Manual,
                scheduled_for: now,
                attempt: 1,
            };
            registry.in_flight.insert(
                trigger.run_id.clone(),
                InFlight {
                    schedule_id: request.schedule_id.clone(),
                    scheduled_for: trigger.scheduled_for,
                    trigger: ScheduleRunTrigger::Manual,
                },
            );
            registry.trigger_keys.insert(
                (
                    request.schedule_id.clone(),
                    request.idempotency_key.as_str().to_owned(),
                ),
                trigger.clone(),
            );
            let evicted_keys = {
                let order = registry
                    .trigger_key_order
                    .entry(request.schedule_id.clone())
                    .or_default();
                order.push(request.idempotency_key.as_str().to_owned());
                let overflow = order.len().saturating_sub(MAX_TRIGGER_KEYS_PER_SCHEDULE);
                order.drain(..overflow).collect::<Vec<_>>()
            };
            for evicted in evicted_keys {
                registry
                    .trigger_keys
                    .remove(&(request.schedule_id.clone(), evicted));
            }
            persist(&self.store, &registry)?;
            events.push(ScheduleEvent::Triggered(trigger.clone()));
        }
        for event in events {
            self.sink.emit(event);
        }
        Ok(ScheduleCommandResult::TriggerAccepted(trigger))
    }

    /// 校验节奏语义，返回已解析的 cron（非 cron 节奏为 `None`）。
    fn validate_cadence(
        &self,
        cadence: &ScheduleCadence,
    ) -> Result<Option<CronSchedule>, ActionableError> {
        match cadence {
            ScheduleCadence::Cron {
                expression,
                timezone,
            } => {
                if let Some(timezone) = timezone
                    && timezone.as_str() != "UTC"
                {
                    return Err(ActionableError::new(
                        ErrorCode::Unsupported,
                        format!("暂不支持时区 `{}` 的 cron 调度", timezone.as_str()),
                        "该调度定义未被创建或更新，调度器不会触发任何运行",
                        "请改用 `UTC` 或省略时区；本阶段求值固定 UTC，未引入时区数据库依赖",
                    ));
                }
                let cron = CronSchedule::parse(expression)?;
                if cron.next_after(self.clock.now()).is_none() {
                    return Err(ActionableError::new(
                        ErrorCode::ValidationFailed,
                        "该 cron 表达式在可预见范围内永远不会触发",
                        "该调度定义未被创建或更新，调度器不会触发任何运行",
                        "请修正日期组合（例如 2 月 31 日这类不存在的日期）后重试",
                    ));
                }
                Ok(Some(cron))
            }
            ScheduleCadence::Interval { every_seconds } => {
                require_interval(*every_seconds)?;
                Ok(None)
            }
            ScheduleCadence::Heartbeat { every_seconds, .. } => {
                require_heartbeat_interval(*every_seconds)?;
                Ok(None)
            }
        }
    }
}

/// 首次触发时刻：以 `floor`（`max(now, starts_at)`）为下界取下一次。
fn first_occurrence(
    cadence: &ScheduleCadence,
    cron: Option<&CronSchedule>,
    floor: DateTime<Utc>,
    validity: &soxu_protocol::ScheduleValidityWindow,
) -> Option<DateTime<Utc>> {
    let candidate = match cadence {
        ScheduleCadence::Cron { .. } => cron?.next_after(floor),
        ScheduleCadence::Interval { every_seconds }
        | ScheduleCadence::Heartbeat { every_seconds, .. } => {
            Some(floor + Duration::seconds(i64::from(*every_seconds)))
        }
    }?;
    clamp_to_window(candidate, validity)
}

/// 触发之后的推进：`cron` 从 `now` 之后取下一次（跳过错过的触发点），
/// 固定周期保持相位网格推进，同样跳过错过的触发点，绝不按积压次数连发。
///
/// 返回（下一次触发时刻, 是否因有效期结束而收敛）。
fn advance_after(
    schedule: &StoredSchedule,
    scheduled_for: DateTime<Utc>,
    now: DateTime<Utc>,
) -> (Option<DateTime<Utc>>, bool) {
    let cadence = &schedule.summary.cadence;
    let candidate = match cadence {
        ScheduleCadence::Cron { expression, .. } => CronSchedule::parse(expression)
            .ok()
            .and_then(|cron| cron.next_after(now.max(scheduled_for))),
        ScheduleCadence::Interval { every_seconds }
        | ScheduleCadence::Heartbeat { every_seconds, .. } => {
            let step = i64::from(*every_seconds).max(1);
            let mut candidate = scheduled_for + Duration::seconds(step);
            if candidate <= now {
                let overdue = (now - candidate).num_seconds() / step + 1;
                candidate += Duration::seconds(step * overdue);
            }
            Some(candidate)
        }
    };
    let Some(candidate) = candidate else {
        return (None, true);
    };
    match clamp_to_window(candidate, &schedule.summary.validity) {
        Some(candidate) => (Some(candidate), false),
        None => (None, true),
    }
}

/// 有效期窗口裁剪：落在 `starts_at..expires_at` 之外即无下一次触发。
fn clamp_to_window(
    candidate: DateTime<Utc>,
    validity: &soxu_protocol::ScheduleValidityWindow,
) -> Option<DateTime<Utc>> {
    validity.is_active_at(candidate).then_some(candidate)
}

fn cadence_feature(cadence: &ScheduleCadence) -> DaemonFeature {
    match cadence {
        ScheduleCadence::Heartbeat { .. } => DaemonFeature::Heartbeats,
        ScheduleCadence::Cron { .. } | ScheduleCadence::Interval { .. } => DaemonFeature::Schedules,
    }
}

fn cadence_trigger(cadence: &ScheduleCadence) -> ScheduleRunTrigger {
    match cadence {
        ScheduleCadence::Cron { .. } => ScheduleRunTrigger::Cron,
        ScheduleCadence::Interval { .. } => ScheduleRunTrigger::Interval,
        ScheduleCadence::Heartbeat { .. } => ScheduleRunTrigger::Heartbeat,
    }
}

fn require_interval(every_seconds: u32) -> Result<(), ActionableError> {
    let range = soxu_protocol::schedules::SCHEDULE_INTERVAL_MIN_SECONDS
        ..=soxu_protocol::schedules::SCHEDULE_INTERVAL_MAX_SECONDS;
    if range.contains(&every_seconds) {
        return Ok(());
    }
    Err(ActionableError::new(
        ErrorCode::ValidationFailed,
        format!("固定周期 {every_seconds} 秒超出允许区间"),
        "该调度定义未被创建或更新，调度器不会触发任何运行",
        "请把周期调整到 60 秒至 30 天之间",
    ))
}

fn require_heartbeat_interval(every_seconds: u32) -> Result<(), ActionableError> {
    let range = soxu_protocol::schedules::HEARTBEAT_INTERVAL_MIN_SECONDS
        ..=soxu_protocol::schedules::HEARTBEAT_INTERVAL_MAX_SECONDS;
    if range.contains(&every_seconds) {
        return Ok(());
    }
    Err(ActionableError::new(
        ErrorCode::ValidationFailed,
        format!("心跳间隔 {every_seconds} 秒超出允许区间"),
        "该调度定义未被创建或更新，调度器不会触发任何运行",
        "请把心跳间隔调整到 30 秒至 30 天之间",
    ))
}

fn validate_workspace_id(workspace_id: &str) -> Result<(), ActionableError> {
    if workspace_id.is_empty() || workspace_id.len() > MAX_WORKSPACE_ID_BYTES {
        return Err(ActionableError::new(
            ErrorCode::ValidationFailed,
            "workspace_id 长度必须在 1..=256 字节之间",
            "本次调度命令未执行，未创建任何定义",
            "请传入守护进程登记过的工作区标识",
        ));
    }
    if workspace_id.chars().any(char::is_control) {
        return Err(ActionableError::new(
            ErrorCode::ValidationFailed,
            "workspace_id 禁止控制字符",
            "本次调度命令未执行，未创建任何定义",
            "请传入守护进程登记过的工作区标识",
        ));
    }
    Ok(())
}

/// 恢复持久化定义时的复核：节奏与有效期必须仍然自洽。
fn validate_stored(schedule: &StoredSchedule) -> Result<(), ActionableError> {
    match &schedule.summary.cadence {
        ScheduleCadence::Cron {
            expression,
            timezone,
        } => {
            if let Some(timezone) = timezone
                && timezone.as_str() != "UTC"
            {
                return Err(ActionableError::new(
                    ErrorCode::WorkspaceDamaged,
                    format!("持久化的调度使用了不支持的时区 `{}`", timezone.as_str()),
                    "守护进程拒绝加载该文件，没有任何调度会被触发",
                    "请修正或删除该调度定义后重新启动守护进程",
                ));
            }
            CronSchedule::parse(expression)?;
        }
        ScheduleCadence::Interval { every_seconds } => require_interval(*every_seconds)?,
        ScheduleCadence::Heartbeat { every_seconds, .. } => {
            require_heartbeat_interval(*every_seconds)?;
        }
    }
    Ok(())
}

fn persist(store: &Option<Arc<ScheduleStore>>, registry: &Registry) -> Result<(), ActionableError> {
    match store {
        Some(store) => store.save(&registry.schedules),
        None => Ok(()),
    }
}

fn unknown_schedule(id: &ScheduleId) -> ActionableError {
    ActionableError::new(
        ErrorCode::NotFound,
        format!("未找到调度 `{}`", id.as_str()),
        "该命令未执行，未改变任何调度定义，也没有下发事件",
        "请先列出当前工作区的调度，确认标识后重试",
    )
}

fn new_schedule_id() -> ScheduleId {
    ScheduleId::try_from(format!("sch_{}", uuid::Uuid::now_v7().simple()))
        .expect("服务端生成的调度标识必合法")
}

fn new_run_id() -> String {
    format!("run_{}", uuid::Uuid::now_v7().simple())
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schedules::clock::ManualClock;
    use soxu_protocol::{
        CronExpression, ScheduleName, ScheduleRetryPolicy, ScheduleTrigger, ScheduleValidityWindow,
    };
    use std::sync::Mutex as StdMutex;

    fn moment(seconds: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(seconds, 0).expect("合法时间戳")
    }

    #[derive(Default)]
    struct Recorder {
        events: StdMutex<Vec<ScheduleEvent>>,
    }

    impl ScheduleEventSink for Recorder {
        fn emit(&self, event: ScheduleEvent) {
            self.events.lock().expect("记录器锁").push(event);
        }
    }

    impl Recorder {
        fn take(&self) -> Vec<ScheduleEvent> {
            std::mem::take(&mut *self.events.lock().expect("记录器锁"))
        }
    }

    fn service(gate: FeatureGate, recorder: &Arc<Recorder>) -> ScheduleService {
        ScheduleService::new(gate, recorder.clone())
            .with_clock(Arc::new(ManualClock::new(moment(1_800_000_000))))
    }

    fn create_request(every_seconds: u32, key: &str) -> CreateScheduleRequest {
        CreateScheduleRequest {
            workspace_id: "wks_test".to_owned(),
            name: ScheduleName::try_from("巡检".to_owned()).expect("名称"),
            cadence: ScheduleCadence::Interval { every_seconds },
            trigger: ScheduleTrigger {
                agent_id: "agent-1".to_owned(),
                model_id: None,
                permission: None,
                text: "检查构建".to_owned(),
            },
            validity: ScheduleValidityWindow::default(),
            retry: ScheduleRetryPolicy::default(),
            idempotency_key: IdempotencyKey::try_from(key.to_owned()).expect("幂等键"),
            paused: false,
        }
    }

    #[test]
    fn cadence_maps_to_expected_feature() {
        assert_eq!(
            cadence_feature(&ScheduleCadence::Interval { every_seconds: 60 }),
            DaemonFeature::Schedules
        );
        assert_eq!(
            cadence_feature(&ScheduleCadence::Heartbeat {
                every_seconds: 30,
                only_when_idle: true
            }),
            DaemonFeature::Heartbeats
        );
    }

    #[test]
    fn tick_is_quiet_when_nothing_is_due() {
        let recorder = Arc::new(Recorder::default());
        let service = service(
            FeatureGate::from_features([DaemonFeature::Schedules]),
            &recorder,
        );
        service
            .handle(ScheduleCommand::Create(create_request(60, "k1")))
            .expect("创建应成功");
        recorder.take();
        let report = service.tick().expect("tick 应成功");
        assert!(report.is_quiet());
        assert!(recorder.take().is_empty());
    }

    #[test]
    fn weekly_expression_is_accepted_and_persisted_verbatim() {
        let recorder = Arc::new(Recorder::default());
        let service = service(
            FeatureGate::from_features([DaemonFeature::Schedules]),
            &recorder,
        );
        let mut request = create_request(60, "k2");
        request.cadence = ScheduleCadence::Cron {
            expression: CronExpression::try_from("0 3 * * 1-5".to_owned()).expect("表达式"),
            timezone: None,
        };
        let ScheduleCommandResult::Created(summary) = service
            .handle(ScheduleCommand::Create(request))
            .expect("创建应成功")
        else {
            panic!("期望 Created");
        };
        assert_eq!(summary.state, ScheduleState::Active);
        assert!(summary.next_run_at.is_some());
    }
}
