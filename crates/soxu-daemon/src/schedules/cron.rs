//! 标准 5 段式 cron 求值（分 时 日 月 周），纯函数、固定 UTC。
//!
//! 协议层（`soxu-protocol::schedules`）只做结构性校验并明确把**语义求值**留给
//! 服务端；本模块是唯一求值点，避免两套 cron 语义分叉。
//!
//! 语义口径（与 Paseo `packages/protocol/src/schedule/cron-expression.ts` 的解析
//! 口径对齐，按本仓契约补齐周字段 `0/7` 等价）：
//! - 字段取值：分 `0-59`、时 `0-23`、日 `1-31`、月 `1-12`、周 `0-7`（`0` 与 `7` 均为周日）；
//! - 步长只能接在 `*` 或 `范围` 之后（`5/2` 这种「单值带步长」属误配置，拒绝）；
//! - 范围步进从下界起算（`10-20/5` 命中 `10`、`15`、`20`）；
//! - 日与周**同时受限**时取并集（Vixie 口径）；任一为 `*` 前缀时取交集；
//! - 求值只在 UTC 进行：`ScheduleTimezone` 只接受 `UTC`/省略，其他 IANA 时区在
//!   创建/更新时 fail-closed 拒绝（本仓 `Cargo.lock` 冻结，未引入时区数据库依赖）。

use chrono::{DateTime, Datelike, Days, NaiveDate, NaiveDateTime, TimeZone, Timelike, Utc};
use soxu_protocol::{ActionableError, CronExpression, ErrorCode};

/// 单次求值最多向后扫描的天数（8 年：覆盖闰年 2 月 29 日与全部月末组合）。
pub const DEFAULT_MAX_SCAN_DAYS: u32 = 366 * 8;

/// 解析完成的 cron 表达式。
///
/// 各字段保存去重升序的取值表，避免求值时反复解析字符串。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CronSchedule {
    minutes: Vec<u32>,
    hours: Vec<u32>,
    days_of_month: Vec<u32>,
    months: Vec<u32>,
    days_of_week: Vec<u32>,
    day_of_month_restricted: bool,
    day_of_week_restricted: bool,
}

impl CronSchedule {
    /// 解析并做语义校验；误配置一律返回可执行错误，不静默降级。
    pub fn parse(expression: &CronExpression) -> Result<Self, ActionableError> {
        let fields: Vec<&str> = expression.as_str().split_whitespace().collect();
        if fields.len() != 5 {
            return Err(invalid("cron 表达式必须是标准 5 段式（分 时 日 月 周）"));
        }
        let (minutes, _) = parse_field(fields[0], 0, 59, "分")?;
        let (hours, _) = parse_field(fields[1], 0, 23, "时")?;
        let (days_of_month, day_of_month_restricted) = parse_field(fields[2], 1, 31, "日")?;
        let (months, _) = parse_field(fields[3], 1, 12, "月")?;
        let (raw_days_of_week, day_of_week_restricted) = parse_field(fields[4], 0, 7, "周")?;
        // 周字段 `7` 与 `0` 等价（周日），归一化后去重，避免出现重复候选。
        let mut days_of_week: Vec<u32> = raw_days_of_week
            .into_iter()
            .map(|day| if day == 7 { 0 } else { day })
            .collect();
        days_of_week.sort_unstable();
        days_of_week.dedup();
        Ok(Self {
            minutes,
            hours,
            days_of_month,
            months,
            days_of_week,
            day_of_month_restricted,
            day_of_week_restricted,
        })
    }

    /// 严格晚于 `from` 的下一次触发时刻；8 年内无命中返回 `None`。
    ///
    /// 求值按「天 → 时 → 分」逐层推进，不做逐分钟扫描，闰日与月末组合也在
    /// 有界步数内收敛。
    #[must_use]
    pub fn next_after(&self, from: DateTime<Utc>) -> Option<DateTime<Utc>> {
        let start = from.date_naive();
        for offset in 0..=DEFAULT_MAX_SCAN_DAYS {
            let date = start.checked_add_days(Days::new(u64::from(offset)))?;
            if !self.matches_date(date) {
                continue;
            }
            let first_hour = if offset == 0 { from.hour() } else { 0 };
            for &hour in self.hours.iter().filter(|&&hour| hour >= first_hour) {
                for &minute in &self.minutes {
                    // 候选时刻的秒位恒为 0，因此「同一小时且分钟不大于下界分钟」
                    // 一律不满足严格晚于下界，必须跳过（含恰好落在触发点的情形）。
                    if offset == 0 && hour == from.hour() && minute <= from.minute() {
                        continue;
                    }
                    let naive: NaiveDateTime = date.and_hms_opt(hour, minute, 0)?;
                    return Some(Utc.from_utc_datetime(&naive));
                }
            }
        }
        None
    }

    /// 给定日期是否命中（月 + 日/周规则）。
    fn matches_date(&self, date: NaiveDate) -> bool {
        if !self.months.contains(&date.month()) {
            return false;
        }
        let day_matches = self.days_of_month.contains(&date.day());
        let weekday = date.weekday().num_days_from_sunday();
        let weekday_matches = self.days_of_week.contains(&weekday);
        match (self.day_of_month_restricted, self.day_of_week_restricted) {
            // 两者都受限：Vixie 口径取并集。
            (true, true) => day_matches || weekday_matches,
            // 至少一侧是 `*` 前缀：取交集（未被限制的一侧恒为真）。
            _ => day_matches && weekday_matches,
        }
    }
}

/// 解析单个字段，返回（去重升序取值, 是否受限）。
///
/// 「受限」沿用 Paseo 口径：字段以 `*` 开头即视为不受限（含 `*/n`），
/// 从而 `0 0 1 * *` 这类写法仍按交集求值。
fn parse_field(
    raw: &str,
    min: u32,
    max: u32,
    label: &'static str,
) -> Result<(Vec<u32>, bool), ActionableError> {
    if raw.is_empty() {
        return Err(invalid_field(label, "字段为空"));
    }
    let restricted = !raw.starts_with('*');
    let mut values = Vec::new();
    for item in raw.split(',') {
        if item.is_empty() {
            return Err(invalid_field(label, "列表存在空项"));
        }
        let (base, step) = match item.split_once('/') {
            Some((base, step)) => {
                let step = parse_step(step, label)?;
                (base, Some(step))
            }
            None => (item, None),
        };
        let (low, high) = parse_range(base, min, max, label)?;
        if base != "*" && base.parse::<u32>().is_ok() && step.is_some() {
            return Err(invalid_field(
                label,
                "步长只能接在 `*` 或范围之后，单值带步长属误配置",
            ));
        }
        let stride = step.unwrap_or(1);
        let mut value = low;
        while value <= high {
            values.push(value);
            value = match value.checked_add(stride) {
                Some(next) => next,
                None => break,
            };
        }
    }
    if values.is_empty() {
        return Err(invalid_field(label, "字段没有任何有效取值"));
    }
    values.sort_unstable();
    values.dedup();
    Ok((values, restricted))
}

/// 解析范围/通配符，返回闭区间上下界。
fn parse_range(
    base: &str,
    min: u32,
    max: u32,
    label: &'static str,
) -> Result<(u32, u32), ActionableError> {
    if base == "*" {
        return Ok((min, max));
    }
    match base.split_once('-') {
        Some((low, high)) => {
            let low = parse_number(low, label)?;
            let high = parse_number(high, label)?;
            if low > high {
                return Err(invalid_field(label, "范围下界不得大于上界"));
            }
            if low < min || high > max {
                return Err(invalid_field(label, "取值超出字段允许区间"));
            }
            Ok((low, high))
        }
        None => {
            let value = parse_number(base, label)?;
            if value < min || value > max {
                return Err(invalid_field(label, "取值超出字段允许区间"));
            }
            Ok((value, value))
        }
    }
}

/// 解析步长：正整数字面量，禁止前导零（`07` 属误配置）。
fn parse_step(raw: &str, label: &'static str) -> Result<u32, ActionableError> {
    let step = parse_number(raw, label)?;
    if step == 0 {
        return Err(invalid_field(label, "步长必须大于 0"));
    }
    if raw.len() > 1 && raw.starts_with('0') {
        return Err(invalid_field(label, "步长禁止前导零"));
    }
    Ok(step)
}

fn parse_number(raw: &str, label: &'static str) -> Result<u32, ActionableError> {
    raw.parse::<u32>()
        .map_err(|_| invalid_field(label, "取值必须是十进制数字"))
}

fn invalid_field(label: &'static str, reason: &str) -> ActionableError {
    ActionableError::new(
        ErrorCode::ValidationFailed,
        format!("cron 表达式的「{label}」字段非法：{reason}"),
        "该调度定义未被创建或更新，调度器不会触发任何运行",
        "请修正 cron 表达式后重试；步长只能接在 `*` 或范围之后",
    )
}

fn invalid(reason: &str) -> ActionableError {
    ActionableError::new(
        ErrorCode::ValidationFailed,
        reason.to_owned(),
        "该调度定义未被创建或更新，调度器不会触发任何运行",
        "请使用标准 5 段式 cron 表达式（分 时 日 月 周）后重试",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cron(raw: &str) -> CronSchedule {
        let expression = CronExpression::try_from(raw.to_owned()).expect("协议层应接受该表达式");
        CronSchedule::parse(&expression).expect("语义层应接受该表达式")
    }

    fn at(raw: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(raw)
            .expect("合法 RFC3339")
            .with_timezone(&Utc)
    }

    #[test]
    fn every_minute_advances_by_one_minute() {
        let schedule = cron("* * * * *");
        let from = at("2026-11-28T10:20:30Z");
        assert_eq!(schedule.next_after(from), Some(at("2026-11-28T10:21:00Z")));
    }

    #[test]
    fn next_is_strictly_after_the_floor_instant() {
        let schedule = cron("30 12 * * *");
        // 恰好在触发点上时，必须返回次日同一时刻，而不是自身。
        assert_eq!(
            schedule.next_after(at("2026-11-28T12:30:00Z")),
            Some(at("2026-11-29T12:30:00Z"))
        );
        assert_eq!(
            schedule.next_after(at("2026-11-28T12:29:59Z")),
            Some(at("2026-11-28T12:30:00Z"))
        );
    }

    #[test]
    fn steps_start_from_range_lower_bound() {
        let schedule = cron("10-20/5 * * * *");
        let from = at("2026-11-28T00:00:00Z");
        assert_eq!(schedule.next_after(from), Some(at("2026-11-28T00:10:00Z")));
        let from = at("2026-11-28T00:10:00Z");
        assert_eq!(schedule.next_after(from), Some(at("2026-11-28T00:15:00Z")));
        let from = at("2026-11-28T00:20:00Z");
        assert_eq!(
            schedule.next_after(from),
            Some(at("2026-11-28T01:10:00Z")),
            "范围步进止于上界；下一小时重新从 10 分开始"
        );
    }

    #[test]
    fn single_value_with_step_is_rejected() {
        let expression =
            CronExpression::try_from("5/2 * * * *".to_owned()).expect("协议层只校验结构");
        let error = CronSchedule::parse(&expression).expect_err("单值带步长必须拒绝");
        assert_eq!(error.code, ErrorCode::ValidationFailed);
        assert!(error.summary_zh.contains("单值带步长"));
    }

    #[test]
    fn out_of_range_and_reversed_ranges_are_rejected() {
        for raw in [
            "60 * * * *",
            "* 24 * * *",
            "0 0 0 * *",
            "0 0 * 13 *",
            "0 0 * * 8",
        ] {
            let expression = CronExpression::try_from(raw.to_owned()).expect("协议层只校验结构");
            assert!(
                CronSchedule::parse(&expression).is_err(),
                "{raw} 必须被语义层拒绝"
            );
        }
        let expression =
            CronExpression::try_from("20-10 * * * *".to_owned()).expect("协议层只校验结构");
        assert!(
            CronSchedule::parse(&expression).is_err(),
            "倒序范围必须拒绝"
        );
    }

    #[test]
    fn leading_zero_step_is_rejected() {
        let expression =
            CronExpression::try_from("*/05 * * * *".to_owned()).expect("协议层只校验结构");
        assert!(CronSchedule::parse(&expression).is_err());
    }

    #[test]
    fn weekday_zero_and_seven_both_mean_sunday() {
        let zero = cron("0 3 * * 0");
        let seven = cron("0 3 * * 7");
        let from = at("2026-11-28T04:00:00Z"); // 周六
        assert_eq!(zero.next_after(from), Some(at("2026-11-29T03:00:00Z")));
        assert_eq!(seven.next_after(from), Some(at("2026-11-29T03:00:00Z")));
        assert_eq!(zero, seven, "`0` 与 `7` 归一化后应完全相同");
    }

    #[test]
    fn leap_day_converges_across_four_years() {
        let schedule = cron("0 0 29 2 *");
        assert_eq!(
            schedule.next_after(at("2026-11-28T00:00:00Z")),
            Some(at("2028-02-29T00:00:00Z")),
            "非闰年必须跨到下一个闰年"
        );
        assert_eq!(
            schedule.next_after(at("2028-02-29T00:00:00Z")),
            Some(at("2032-02-29T00:00:00Z"))
        );
    }

    #[test]
    fn month_end_skips_short_months() {
        let schedule = cron("0 0 31 * *");
        assert_eq!(
            schedule.next_after(at("2026-11-01T00:00:00Z")),
            Some(at("2026-12-31T00:00:00Z")),
            "11 月只有 30 天，必须跳到 12 月 31 日"
        );
        assert_eq!(
            schedule.next_after(at("2026-12-31T00:00:00Z")),
            Some(at("2027-01-31T00:00:00Z"))
        );
    }

    #[test]
    fn impossible_date_returns_none() {
        let schedule = cron("0 0 31 2 *");
        assert_eq!(schedule.next_after(at("2026-01-01T00:00:00Z")), None);
    }

    #[test]
    fn restricted_day_and_weekday_are_unioned() {
        // 13 日或周五都触发（Vixie 并集口径）。
        let schedule = cron("0 0 13 * 5");
        let from = at("2026-11-01T00:00:00Z");
        let first = schedule.next_after(from).expect("应命中");
        assert_eq!(first, at("2026-11-06T00:00:00Z"), "11 月 6 日是周五");
        let second = schedule.next_after(first).expect("应命中");
        assert_eq!(
            second,
            at("2026-11-13T00:00:00Z"),
            "13 日与周五各自命中且去重"
        );
    }

    #[test]
    fn wildcard_day_with_restricted_weekday_stays_intersection() {
        let schedule = cron("0 0 * * 1");
        let from = at("2026-11-28T00:00:00Z"); // 周六
        assert_eq!(
            schedule.next_after(from),
            Some(at("2026-11-30T00:00:00Z")),
            "日字段为 `*` 时只按周字段过滤"
        );
    }

    #[test]
    fn hourly_step_scan_is_bounded() {
        let schedule = cron("0 */6 * * *");
        let mut cursor = at("2026-11-28T00:00:00Z");
        let mut seen = Vec::new();
        for _ in 0..4 {
            cursor = schedule.next_after(cursor).expect("应命中");
            seen.push(cursor);
        }
        assert_eq!(
            seen,
            vec![
                at("2026-11-28T06:00:00Z"),
                at("2026-11-28T12:00:00Z"),
                at("2026-11-28T18:00:00Z"),
                at("2026-11-29T00:00:00Z"),
            ]
        );
    }

    #[test]
    fn scan_is_fast_for_sparse_expressions() {
        let schedule = cron("0 0 29 2 *");
        let started = std::time::Instant::now();
        let next = schedule.next_after(at("2026-01-01T00:00:00Z"));
        assert!(next.is_some());
        assert!(
            started.elapsed() < std::time::Duration::from_secs(1),
            "稀疏表达式必须在有界步数内收敛，实耗 {:?}",
            started.elapsed()
        );
    }
}
