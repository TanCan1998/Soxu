//! M6-C 调度域：cron 语义求值、可注入时钟、定义持久化、幂等触发与重试收敛。
//!
//! 本模块消费已合入并冻结的 `soxu-protocol::schedules` 契约，不新增线缆形态；
//! 传输层与命令转发属 `soxu-protocol` 冻结范围，宿主接线后直接调用
//! [`ScheduleService::handle`] 即可。
//!
//! 红线：
//! 1. **能力位缺省关闭**：`Schedules` / `Heartbeats` 未声明时整族命令拒绝，
//!    `tick` 也不为这类调度下发任何事件（见 [`executor`]）；
//! 2. **时间来自可注入时钟**：生产用 [`SystemClock`]，测试用 [`ManualClock`]
//!    推进，禁用 `sleep` 等真实等待；
//! 3. **幂等与重试有硬上限**：创建与手动触发按 `idempotency_key` 收敛，
//!    失败重试沿用同一 `run_id` 与 `scheduled_for`，次数受 `retry.max_attempts` 约束；
//! 4. **停用/删除收敛**：停用丢弃在途重试，删除同时清理幂等索引与在途登记，
//!    删除后到达的运行结果只丢弃，绝不复活定义、也不下发事件。

pub mod clock;
pub mod cron;
pub mod executor;
pub mod store;

pub use clock::{Clock, ManualClock, SystemClock};
pub use cron::{CronSchedule, DEFAULT_MAX_SCAN_DAYS};
pub use executor::{
    FinishOutcome, MAX_SCHEDULES_PER_WORKSPACE, NullScheduleEventSink, ScheduleEventSink,
    ScheduleService, StoredSchedule, TickReport,
};
pub use store::{SCHEDULE_STORE_MAX_BYTES, ScheduleStore};
