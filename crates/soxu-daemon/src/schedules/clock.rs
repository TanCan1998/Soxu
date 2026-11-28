//! 可注入时钟：调度求值只依赖 [`Clock`]，测试不依赖真实等待。
//!
//! 生产环境用 [`SystemClock`]（系统 UTC 时钟）；测试用 [`ManualClock`] 显式推进，
//! 因此「下次触发 / 重试退避 / 有效期过期」全部可确定性复现。

use chrono::{DateTime, Duration, Utc};
use std::sync::{Mutex, MutexGuard};

/// 调度域时间源。
///
/// 实现必须是线程安全的：执行器在 `tick` 与命令处理中共享同一实例。
pub trait Clock: Send + Sync {
    /// 当前 UTC 时刻。
    fn now(&self) -> DateTime<Utc>;
}

/// 系统时钟。
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// 手动时钟：初始时刻由调用方给定，只通过 [`ManualClock::advance`] /
/// [`ManualClock::set`] 前进，永不自行走动。
#[derive(Debug)]
pub struct ManualClock {
    now: Mutex<DateTime<Utc>>,
}

impl ManualClock {
    /// 以上述时刻为起点。
    #[must_use]
    pub fn new(start: DateTime<Utc>) -> Self {
        Self {
            now: Mutex::new(start),
        }
    }

    /// 前进给定时长，返回新时刻。
    pub fn advance(&self, by: Duration) -> DateTime<Utc> {
        let mut guard = lock(&self.now);
        *guard += by;
        *guard
    }

    /// 直接设定时刻（用于「跨过有效期终点」这类边界构造）。
    pub fn set(&self, at: DateTime<Utc>) {
        *lock(&self.now) = at;
    }
}

impl Clock for ManualClock {
    fn now(&self) -> DateTime<Utc> {
        *lock(&self.now)
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(seconds: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(seconds, 0).expect("合法时间戳")
    }

    #[test]
    fn manual_clock_only_moves_when_told() {
        let clock = ManualClock::new(at(1_800_000_000));
        assert_eq!(clock.now(), at(1_800_000_000));
        assert_eq!(clock.now(), at(1_800_000_000), "手动时钟不得自行走动");

        assert_eq!(clock.advance(Duration::seconds(90)), at(1_800_000_090));
        assert_eq!(clock.now(), at(1_800_000_090));

        clock.set(at(1_900_000_000));
        assert_eq!(clock.now(), at(1_900_000_000));
    }

    #[test]
    fn system_clock_is_monotonic_enough_for_scheduling() {
        let clock = SystemClock;
        let first = clock.now();
        let second = clock.now();
        assert!(second >= first, "系统时钟两次读取不得回退");
    }
}
