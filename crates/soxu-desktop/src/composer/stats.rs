//! 三色上下文圆环与 11 项会话遥测统计的呈现层（纯逻辑 + 几何计算）。
//!
//! 规范来源：`pages/02-composer-and-handoff.md` §2.1 与 §3.7——
//! - **三色圆环**：按已用比例取主题三档语义色（绿 `< 65%`、琥珀 `65% – 88%`、
//!   绯红 `>= 88%`），精度保留一位小数；12 点钟起点顺时针圆弧；
//! - **11 项遥测统计**：轮次/步数、累计 Token、API 参考费用、平均/峰值/最近速度、
//!   缓存命中、缓存观察（8 行主表）+ 模型用时、工具用时、首 Token 延迟（3 行「更多统计」）；
//! - **空态**：无任何遥测时不渲染假数据，给出明确空态文案；
//! - **遥测文本严格禁止使用中圆点分隔符**（`·`），本模块所有格式化输出均受单测约束。
//!
//! 数据契约直接复用 [`soxu_protocol::SessionTelemetryStats`] 与
//! [`soxu_protocol::ContextUsageBand`]，本模块只做格式化、聚合与圆弧几何。

use soxu_protocol::{ContextUsageBand, SessionTelemetryStats};

/// 圆环基准半径（px，`16×16` 视框内的描边半径）。
pub const RING_RADIUS_PX: f32 = 6.5;

/// 圆环描边宽度（px）。
pub const RING_STROKE_PX: f32 = 2.2;

/// 圆环百分比的一位小数呈现（`86.4%`；满量程显示 `100%`）。
pub fn format_usage_percent(pct: f64) -> String {
    let clamped = pct.clamp(0.0, 100.0);
    if clamped >= 99.95 {
        "100%".to_string()
    } else {
        format!("{clamped:.1}%")
    }
}

/// 上下文占用圆环的呈现快照。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UsageRing {
    /// 已用比例（`0.0..=100.0`）。
    pub percent: f64,
    /// 三段语义色档位（绿 / 琥珀 / 绯红）。
    pub band: ContextUsageBand,
    /// 是否满量程（渲染为整圈而非圆弧）。
    pub is_full: bool,
}

impl UsageRing {
    /// 由遥测快照推导圆环（限量为 0 时占比按 0 处理，避免除零）。
    pub fn from_stats(stats: &SessionTelemetryStats) -> Self {
        let percent = stats.context_usage_pct();
        Self {
            percent,
            band: ContextUsageBand::from_percentage(percent),
            is_full: percent >= 99.5,
        }
    }

    /// 一位小数的百分比文案。
    pub fn label(&self) -> String {
        format_usage_percent(self.percent)
    }

    /// 圆环是否处于空态（无任何上下文占用）。
    pub fn is_empty(&self) -> bool {
        self.percent <= 0.0
    }
}

/// 圆弧两个端点相对圆心的偏移（px，x 向右、y 向下），12 点钟为起点顺时针。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RingArc {
    /// 起点偏移（`pct = 0` 时恒为 `(0, -r)`）。
    pub start: (f32, f32),
    /// 终点偏移。
    pub end: (f32, f32),
    /// 是否走大弧（`pct > 50`）。
    pub large_arc: bool,
}

/// 计算 12 点钟起点顺时针圆弧的端点几何。
///
/// 起点钳制在 `2%`、终点钳制在 `99.5%`：极小比例仍可见起笔、接近满量程时不吞掉缺口
/// （与移动端 `describeProgressArc` 同口径，但本实现保证可单测）。
pub fn ring_arc(percent: f64, radius: f32) -> RingArc {
    let clamped = percent.clamp(2.0, 99.5);
    let start_angle = -std::f64::consts::FRAC_PI_2;
    let end_angle = start_angle + (clamped / 100.0) * std::f64::consts::TAU;

    let offset = |angle: f64| {
        (
            (radius as f64 * angle.cos()) as f32,
            (radius as f64 * angle.sin()) as f32,
        )
    };

    RingArc {
        start: offset(start_angle),
        end: offset(end_angle),
        large_arc: clamped > 50.0,
    }
}

/// 遥测行的分组（主表 8 行 / 可折叠「更多统计」3 行）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TelemetryGroup {
    /// 主表（默认展开）。
    Primary,
    /// 「更多统计」折叠区。
    More,
}

/// 一行遥测统计。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TelemetryRow {
    /// 中文标签。
    pub label: &'static str,
    /// 已格式化的数值。
    pub value: String,
    /// 分组。
    pub group: TelemetryGroup,
}

/// 遥测面板的呈现状态。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TelemetryView {
    /// 空态：没有任何可展示的遥测。
    Empty,
    /// 正常态：11 行（8 主表 + 3 更多）。
    Rows(Vec<TelemetryRow>),
}

impl TelemetryView {
    /// 行数（空态为 0）。
    pub fn len(&self) -> usize {
        match self {
            Self::Empty => 0,
            Self::Rows(rows) => rows.len(),
        }
    }

    /// 是否为空态。
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// 空态文案（无遥测时不渲染假数据）。
pub const EMPTY_TELEMETRY_LABEL: &str = "暂无遥测数据";

/// 是否整份遥测均为零值（空态判定）。
pub fn is_empty_stats(stats: &SessionTelemetryStats) -> bool {
    stats.turns == 0
        && stats.steps == 0
        && stats.used_context_tokens == 0
        && stats.total_tokens == 0
        && stats.api_cost_usd == 0.0
        && stats.avg_speed_tps == 0.0
        && stats.peak_speed_tps == 0.0
        && stats.recent_speed_tps == 0.0
        && stats.cache_hit_pct == 0.0
        && stats.cache_note.is_empty()
        && stats.model_duration_sec == 0.0
        && stats.tool_duration_sec == 0.0
        && stats.ttft_sec == 0.0
}

/// Token 数人类可读格式：`1.2M` / `31.5k` / `940`。
pub fn format_tokens(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}k", tokens as f64 / 1_000.0)
    } else {
        format!("{tokens}")
    }
}

/// 参考费用格式：`$31.49`。
pub fn format_cost(usd: f64) -> String {
    format!("${usd:.2}")
}

/// 速率格式：`94.4 tok/s`。
pub fn format_speed(tps: f64) -> String {
    format!("{tps:.1} tok/s")
}

/// 百分比格式：`91.2%`。
pub fn format_percent(pct: f64) -> String {
    format!("{pct:.1}%")
}

/// 时长格式：`1分34秒` / `34秒` / `0秒`。
pub fn format_duration(sec: f64) -> String {
    let total = sec.max(0.0).round() as u64;
    let minutes = total / 60;
    let seconds = total % 60;
    if minutes > 0 {
        format!("{minutes}分{seconds}秒")
    } else {
        format!("{seconds}秒")
    }
}

/// 聚合 11 行遥测统计；整份为零值时返回 [`TelemetryView::Empty`]。
pub fn telemetry_view(stats: &SessionTelemetryStats) -> TelemetryView {
    if is_empty_stats(stats) {
        return TelemetryView::Empty;
    }

    let mut rows = Vec::with_capacity(11);
    rows.push(TelemetryRow {
        label: "轮次 / 步数",
        value: format!("{} / {}", stats.turns, stats.steps),
        group: TelemetryGroup::Primary,
    });
    rows.push(TelemetryRow {
        label: "累计 Token",
        value: format_tokens(stats.total_tokens),
        group: TelemetryGroup::Primary,
    });
    rows.push(TelemetryRow {
        label: "API 参考费用",
        value: format_cost(stats.api_cost_usd),
        group: TelemetryGroup::Primary,
    });
    rows.push(TelemetryRow {
        label: "平均速度",
        value: format_speed(stats.avg_speed_tps),
        group: TelemetryGroup::Primary,
    });
    rows.push(TelemetryRow {
        label: "峰值速度",
        value: format_speed(stats.peak_speed_tps),
        group: TelemetryGroup::Primary,
    });
    rows.push(TelemetryRow {
        label: "最近速度",
        value: format_speed(stats.recent_speed_tps),
        group: TelemetryGroup::Primary,
    });
    rows.push(TelemetryRow {
        label: "缓存命中",
        value: format_percent(stats.cache_hit_pct),
        group: TelemetryGroup::Primary,
    });
    rows.push(TelemetryRow {
        label: "缓存观察",
        value: stats.cache_note.clone(),
        group: TelemetryGroup::Primary,
    });
    rows.push(TelemetryRow {
        label: "模型用时",
        value: format_duration(stats.model_duration_sec),
        group: TelemetryGroup::More,
    });
    rows.push(TelemetryRow {
        label: "工具用时",
        value: format_duration(stats.tool_duration_sec),
        group: TelemetryGroup::More,
    });
    rows.push(TelemetryRow {
        label: "首 Token 延迟",
        value: format_duration(stats.ttft_sec),
        group: TelemetryGroup::More,
    });

    TelemetryView::Rows(rows)
}

/// 页脚主读数：`$31.49` 与 `94.4 tok/s`（无外框、无中圆点分隔符）。
pub struct FooterReadout {
    /// 参考费用（`None` 表示极窄屏省略该项）。
    pub cost: Option<String>,
    /// 最近速率读数。
    pub speed: String,
}

impl FooterReadout {
    /// 由遥测快照推导页脚读数；窄屏（`compact = true`）省略费用。
    pub fn from_stats(stats: &SessionTelemetryStats, compact: bool) -> Self {
        Self {
            cost: (!compact).then(|| format_cost(stats.api_cost_usd)),
            speed: format_speed(stats.recent_speed_tps),
        }
    }

    /// 拼接为单行文本（以空格分隔，禁止中圆点）。
    pub fn text(&self) -> String {
        match &self.cost {
            Some(cost) => format!("{cost} {}", self.speed),
            None => self.speed.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_stats() -> SessionTelemetryStats {
        SessionTelemetryStats {
            turns: 7,
            steps: 23,
            used_context_tokens: 172_800,
            context_window_limit: 200_000,
            total_tokens: 1_234_567,
            api_cost_usd: 31.49,
            avg_speed_tps: 88.2,
            peak_speed_tps: 121.7,
            recent_speed_tps: 94.4,
            cache_hit_pct: 91.2,
            cache_note: "命中稳定".to_string(),
            model_duration_sec: 94.4,
            tool_duration_sec: 12.0,
            ttft_sec: 1.8,
        }
    }

    /// 三色分档边界：65% 与 88% 是档位切换点（`>=` 归入高档）。
    #[test]
    fn usage_band_boundaries() {
        let mut stats = SessionTelemetryStats {
            used_context_tokens: 0,
            context_window_limit: 1000,
            ..SessionTelemetryStats::default()
        };

        for (used, expected) in [
            (0_u64, ContextUsageBand::SafeGreen),
            (649, ContextUsageBand::SafeGreen),
            (650, ContextUsageBand::WarningAmber),
            (879, ContextUsageBand::WarningAmber),
            (880, ContextUsageBand::DangerRed),
            (1000, ContextUsageBand::DangerRed),
        ] {
            stats.used_context_tokens = used;
            assert_eq!(
                UsageRing::from_stats(&stats).band,
                expected,
                "used={used}/1000 档位不符"
            );
        }
    }

    /// 比例格式化保留一位小数，满量程显示 100%。
    #[test]
    fn usage_percent_formatting() {
        assert_eq!(format_usage_percent(86.4), "86.4%");
        assert_eq!(format_usage_percent(0.0), "0.0%");
        assert_eq!(format_usage_percent(99.96), "100%");
        // 越界输入被钳制。
        assert_eq!(format_usage_percent(150.0), "100%");
        assert_eq!(format_usage_percent(-3.0), "0.0%");
    }

    /// 限量为 0 时不除零，圆环为空态。
    #[test]
    fn zero_limit_is_not_a_division_by_zero() {
        let stats = SessionTelemetryStats::default();
        let ring = UsageRing::from_stats(&stats);
        assert_eq!(ring.percent, 0.0);
        assert_eq!(ring.band, ContextUsageBand::SafeGreen);
        assert!(ring.is_empty());
        assert!(!ring.is_full);
    }

    /// 12 点钟起点顺时针圆弧几何：25% / 50% / 75% 三个锚点。
    #[test]
    fn ring_arc_anchors_are_clockwise_from_twelve() {
        let r = 10.0_f32;

        let quarter = ring_arc(25.0, r);
        // 起点固定在 12 点钟（x=0, y=-r）。
        assert!((quarter.start.0).abs() < 0.001);
        assert!((quarter.start.1 + r).abs() < 0.001);
        // 25% 落在 3 点钟（x=+r, y≈0）。
        assert!((quarter.end.0 - r).abs() < 0.001);
        assert!(quarter.end.1.abs() < 0.001);
        assert!(!quarter.large_arc);

        let half = ring_arc(50.0, r);
        assert!(half.end.0.abs() < 0.001);
        assert!((half.end.1 - r).abs() < 0.001);
        assert!(!half.large_arc);

        let three_quarters = ring_arc(75.0, r);
        assert!((three_quarters.end.0 + r).abs() < 0.001);
        assert!(three_quarters.end.1.abs() < 0.001);
        assert!(three_quarters.large_arc);
    }

    /// 极小/接近满量程时端点被钳制，弧线始终可见。
    #[test]
    fn ring_arc_clamps_extremes() {
        let tiny = ring_arc(0.0, 10.0);
        assert!(!tiny.large_arc);
        // 钳制到 2%，终点略偏离起点。
        assert!((tiny.end.0 - tiny.start.0).abs() > 0.5);

        let almost_full = ring_arc(100.0, 10.0);
        assert!(almost_full.large_arc);
        // 终点与起点之间仍留缺口（不闭合整圈，整圈由 `is_full` 分支单独处理）。
        let gap = ((almost_full.end.0 - almost_full.start.0).powi(2)
            + (almost_full.end.1 - almost_full.start.1).powi(2))
        .sqrt();
        assert!(gap > 0.1);
    }

    /// 11 项统计：8 行主表 + 3 行「更多统计」，标签与顺序符合 §3.7。
    #[test]
    fn telemetry_view_has_eleven_rows() {
        let view = telemetry_view(&sample_stats());
        let rows = match &view {
            TelemetryView::Rows(rows) => rows.clone(),
            TelemetryView::Empty => panic!("样本统计不应为空态"),
        };
        assert_eq!(rows.len(), 11);
        assert_eq!(view.len(), 11);

        let labels: Vec<&str> = rows.iter().map(|row| row.label).collect();
        assert_eq!(
            labels,
            vec![
                "轮次 / 步数",
                "累计 Token",
                "API 参考费用",
                "平均速度",
                "峰值速度",
                "最近速度",
                "缓存命中",
                "缓存观察",
                "模型用时",
                "工具用时",
                "首 Token 延迟",
            ]
        );

        let primary = rows
            .iter()
            .filter(|row| row.group == TelemetryGroup::Primary)
            .count();
        let more = rows
            .iter()
            .filter(|row| row.group == TelemetryGroup::More)
            .count();
        assert_eq!((primary, more), (8, 3));
    }

    /// 数值格式化：Token 缩写、费用两位小数、速率一位小数、时长分秒。
    #[test]
    fn value_formatting() {
        assert_eq!(format_tokens(940), "940");
        assert_eq!(format_tokens(31_500), "31.5k");
        assert_eq!(format_tokens(1_234_567), "1.2M");
        assert_eq!(format_cost(31.49), "$31.49");
        assert_eq!(format_speed(94.4), "94.4 tok/s");
        assert_eq!(format_percent(91.2), "91.2%");
        assert_eq!(format_duration(94.4), "1分34秒");
        assert_eq!(format_duration(34.0), "34秒");
        assert_eq!(format_duration(0.0), "0秒");
        // 负数（异常遥测）不成负数时长。
        assert_eq!(format_duration(-5.0), "0秒");
    }

    /// 空态：整份为零值时不出假数据，给出明确空态判定。
    #[test]
    fn empty_stats_render_empty_state() {
        let stats = SessionTelemetryStats::default();
        assert!(is_empty_stats(&stats));
        let view = telemetry_view(&stats);
        assert_eq!(view, TelemetryView::Empty);
        assert!(view.is_empty());
        assert_eq!(EMPTY_TELEMETRY_LABEL, "暂无遥测数据");

        // 只要有一项非零就不再是空态。
        let almost = SessionTelemetryStats {
            turns: 1,
            ..SessionTelemetryStats::default()
        };
        assert!(!is_empty_stats(&almost));
        assert_eq!(telemetry_view(&almost).len(), 11);
    }

    /// 遥测文本严格禁止中圆点分隔符（§3.7 硬约束）。
    #[test]
    fn telemetry_text_never_uses_middle_dot() {
        let stats = sample_stats();
        let view = telemetry_view(&stats);
        let rows = match view {
            TelemetryView::Rows(rows) => rows,
            TelemetryView::Empty => panic!("样本统计不应为空态"),
        };
        for row in rows {
            assert!(!row.label.contains('·'), "标签含中圆点：{}", row.label);
            assert!(!row.value.contains('·'), "数值含中圆点：{}", row.value);
        }

        let readout = FooterReadout::from_stats(&stats, false);
        assert_eq!(readout.text(), "$31.49 94.4 tok/s");
        assert!(!readout.text().contains('·'));
    }

    /// 窄屏页脚省略费用读数，只留速率。
    #[test]
    fn compact_footer_drops_cost() {
        let stats = sample_stats();
        let compact = FooterReadout::from_stats(&stats, true);
        assert!(compact.cost.is_none());
        assert_eq!(compact.text(), "94.4 tok/s");
    }
}
