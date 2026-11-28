//! M5-A 时间线：虚拟对话流、5 类工具卡/审批卡/提问卡、检查点锚点与轮次导航轨。
//!
//! 设计权威：`design-system/soxu/pages/03-timeline-tools-and-checkpoints.md` 与
//! `pages/01-desktop-workbench.md`（§7.1 轮次导航轨）。数据以内存视图模型/快照为输入，
//! **不接 daemon I/O**：整个模块的渲染路径只读内存，零文件系统/SQLite/Git/子进程调用。
//!
//! 分层（纯逻辑先行，视图层只画状态）：
//!
//! | 子模块 | 职责 | 关键不变量 |
//! | :--- | :--- | :--- |
//! | [`cadence`] | 流式合批节拍 | 120ms 量化、`<= 8.3Hz`、每种增量同批折叠 |
//! | [`window`] | 虚拟窗口数学 | 行高估算 + 上下占位 + 滚动锚定 |
//! | [`model`] | 视图模型快照 | `Rc::ptr_eq` `O(1)` 比较，高度只算一次 |
//! | [`cards`] | 5 类工具卡 | 400 行 Diff 截断、Diff 快照只构建一次 |
//! | [`gate`] | 审批卡 + 提问卡 | 高危禁用「本会话允许」+ 默认焦点在拒绝 + 跨设备幂等 |
//! | [`checkpoint`] | 检查点锚点 + `LiveSteps` | 回退三选一不静默覆写、`LiveSteps` 默认折叠 |
//! | [`rail`] | 20px 轮次导航轨 | 刻度样式优先级、命中边界、跳转钳制 |
//! | [`focus_scope`] | 卡片内焦点 | 复用冻结的 `FocusLayer`，Tab 不逃逸、Esc 自消费 |
//! | [`view`] | GPUI 渲染 | 只读快照、虚拟列表、点击跳转 |
//! | [`text`] | 时间线文案 | 静态常量 + 纯格式化（`i18n.rs` 冻结期的收敛点） |
//!
//! 参考边界：Padu（`fbe97a3`）仅借鉴「虚拟列表 + 导航轨 + 尾行重测」结构，
//! 不移植 `bezel-gpui` 依赖、不用 `#[allow(dead_code)]` 压制，几何常量全部来自
//! 本仓 `tokens.json` 编译产物。

pub mod cadence;
pub mod cards;
pub mod checkpoint;
pub mod focus_scope;
pub mod gate;
pub mod model;
pub mod rail;
pub mod text;
pub mod view;
pub mod window;

pub use view::TimelineView;

use crate::theme::tokens::sizing;

/// 主对话阅读列最大宽度（px）：`layout.transcriptMaxWidth` = 760。
pub const READING_MEASURE_PX: f32 = crate::tokens::layout::TRANSCRIPT_MAX_WIDTH_PX;

/// 中栏上下必须让出的壳层高度（Titlebar 48 + Statusbar 30）。
pub const CHROME_HEIGHT_PX: f32 = sizing::TITLEBAR_HEIGHT_PX + sizing::STATUSBAR_HEIGHT_PX;

#[cfg(test)]
mod tests {
    use super::*;

    /// 关键几何常量与令牌零漂移（防止视图层手写数字）。
    #[test]
    fn geometry_constants_match_tokens() {
        assert_eq!(READING_MEASURE_PX, 760.0);
        assert_eq!(CHROME_HEIGHT_PX, 78.0);
        assert_eq!(sizing::TURN_NAV_RAIL_WIDTH_PX, rail::RAIL_WIDTH_PX);
        assert_eq!(
            sizing::ACTIVITY_DIFF_MAX_HEIGHT_PX,
            cards::DIFF_PREVIEW_MAX_LINES as f32
        );
        assert_eq!(sizing::PERMISSION_DETAIL_MAX_HEIGHT_PX, 96.0);
    }

    /// 合批窗口与令牌频率上限自洽：`1000 / 120 = 8.33`，不超过 8.3Hz 声明值。
    #[test]
    fn commit_window_matches_max_hz_token() {
        let hz = 1000.0 / cadence::STREAM_COMMIT_WINDOW_MS as f32;
        assert!(hz >= cadence::MAX_COMMIT_HZ);
        assert!(hz < cadence::MAX_COMMIT_HZ + 0.1);
    }
}
