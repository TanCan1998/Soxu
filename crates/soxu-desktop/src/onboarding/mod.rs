//! 首次启动三步引导向导（`pages/06-modals-onboarding-and-automation.md` §一）。
//!
//! **按需触发、非默认常驻**：只有调用方传入的标志位判定「该弹」时才出现；
//! 主界面与左栏底部不保留任何常驻引导入口。本模块不自行探测环境、不读配置文件、
//! 不落盘——`has_completed_onboarding` 的持久化由 daemon 承担，这里只维护
//! 纯内存状态机与「完成/跳过之后不再自动弹出」的抑制语义。
//!
//! 三步（§一）：
//! 1. 本机生态零破坏自检（Magpie 状态卡 + 原生 CLI 绿灯 + 只读承诺横幅）；
//! 2. 绑定首个 Git 项目与 Worktree 隔离（三选一 + 共享编译缓存提示）；
//! 3. 手机遥控与快捷键速览（配对二维码预览 + 5 张快捷键卡）。
//!
//! 几何（§一 + §5 边界表）：宽 `560px`（移动端 `92vw` 自适应）、`p(20)`、
//! `rounded(18)`、底色 `#171717`（`surface.card`）、描边 `1px #383838`
//! （`border.strong`），顶部 3 段进度条，右上角常驻 `[跳过引导]`。
//!
//! 键位：打开时聚焦主按钮（[`OnboardingStep::primary_action`]），`Esc` 等同跳过；
//! 即使用户没有网络或没装 Magpie，Step 1 也必须能走完（§5 边界表要求）。

pub mod view;

use crate::theme::gpui_colors::Palette;
use crate::tokens::{radius, spacing};

/// 引导步骤数（`顶部 3 段进度条`）。
pub const STEP_COUNT: usize = 3;

/// 桌面端卡片宽度（§一 `560px`）。
pub const CARD_WIDTH_PX: f32 = 560.0;

/// 移动端卡片宽度占比（§一 `92vw`）。
pub const MOBILE_WIDTH_VW: f32 = 92.0;

/// 卡片内边距（§5 边界表 `p(20)`）。
pub const CARD_PADDING_PX: f32 = spacing::SPACE_5_PX;

/// 卡片圆角（§5 边界表 `rounded(18)`）。
pub const CARD_RADIUS_PX: f32 = radius::RADIUS_LG_PX;

/// 顶部进度条段数（与步骤数一致）。
pub const PROGRESS_SEGMENTS: usize = STEP_COUNT;

/// 只读承诺横幅文案（§一步骤 1 字面值）。
pub const PROMISE_BANNER: &str =
    "Soxu 只读读取已有配置与历史，绝不篡改 ~/.claude / ~/.codex / ~/.pi / ~/.config/magpie";

/// Step 2 的共享编译缓存提示（§一步骤 2 字面值）。
pub const SHARED_TARGET_HINT: &str =
    "已启用共享编译缓存：CARGO_TARGET_DIR 跨 Worktree 共享，免重复编全量 Rust";

/// Step 2 三选一按钮（§一步骤 2）。
pub const PROJECT_CHOICES: [&str; 3] =
    ["选择本地目录", "克隆 GitHub 仓库", "新建并初始化 Git 仓库"];

/// Step 3 配对二维码地址前缀（§一步骤 3 与 §3.1）。
pub const PAIRING_URL_PREFIX: &str = "http://<ip>:7698/#offer=";

/// Step 3 五张快捷键卡（§一步骤 3：5 个最常用快捷键）。
pub const SHORTCUT_CARDS: [(&str, &str); 5] = [
    ("⌘K", "命令面板"),
    ("⌘B", "折叠左栏"),
    ("⌘J", "显隐右栏"),
    ("⌘⏎", "发送"),
    ("Esc", "清空/关闭"),
];

/// 原生 CLI 清单（§一步骤 1 的「7 大原生 CLI」，顺序与 drivers 目录一致）。
pub const NATIVE_CLIS: [&str; 7] = [
    "claude", "codex", "pi", "opencode", "agy", "copilot", "qwen",
];

/// Magpie 双态探测结果（§一步骤 1 状态卡）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MagpieStatus {
    /// 已安装并接管：翠绿高亮。
    Managed,
    /// 未安装，直接驱动原生 CLI：中性提示。
    StandaloneNative,
}

impl MagpieStatus {
    /// 状态卡文案。
    pub const fn label(self) -> &'static str {
        match self {
            Self::Managed => "Magpie 已接管：模型网关与 MCP/Skill 路由生效",
            Self::StandaloneNative => "未安装 Magpie：直接驱动本机原生 CLI",
        }
    }

    /// 状态卡语义色标识（视图层映射到令牌）。
    pub const fn tone(self) -> &'static str {
        match self {
            Self::Managed => "accent",
            Self::StandaloneNative => "neutral",
        }
    }
}

/// Step 1 所需的环境快照：由调用方（`soxu-magpie` / `soxu-drivers` 装配期）探测后传入。
///
/// 本模块不做任何探测：即便两个列表都为空，Step 1 依然可以完整走完（§5 边界表）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EcosystemSnapshot {
    /// Magpie 是否已接管；`None` 表示尚未探测（按未接管展示）。
    pub magpie_managed: Option<bool>,
    /// 已安装可用的原生 CLI（顺序不限，渲染时按 [`NATIVE_CLIS`] 过滤排序）。
    pub available_clis: Vec<&'static str>,
}

impl EcosystemSnapshot {
    /// Magpie 状态（未探测按 [`MagpieStatus::StandaloneNative`] 展示）。
    pub fn magpie(&self) -> MagpieStatus {
        match self.magpie_managed {
            Some(true) => MagpieStatus::Managed,
            _ => MagpieStatus::StandaloneNative,
        }
    }

    /// 按 [`NATIVE_CLIS`] 顺序列出可用 CLI（未装的自动隐藏）。
    pub fn clis_in_order(&self) -> Vec<&'static str> {
        NATIVE_CLIS
            .into_iter()
            .filter(|cli| self.available_clis.contains(cli))
            .collect()
    }
}

/// 三步引导的步骤。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum OnboardingStep {
    /// 步骤 1：本机生态零破坏自检。
    Ecosystem,
    /// 步骤 2：绑定首个 Git 项目与 Worktree 隔离。
    Project,
    /// 步骤 3：手机遥控与快捷键速览。
    Pairing,
}

impl OnboardingStep {
    /// 三步顺序（顶部进度条与「下一步」推进方向）。
    pub const ALL: [Self; STEP_COUNT] = [Self::Ecosystem, Self::Project, Self::Pairing];

    /// 从 0 起的序号。
    pub const fn index(self) -> usize {
        match self {
            Self::Ecosystem => 0,
            Self::Project => 1,
            Self::Pairing => 2,
        }
    }

    /// 从 1 起的步骤号（「Step 1 / 3」文案）。
    pub const fn ordinal(self) -> usize {
        self.index() + 1
    }

    /// 步骤标题。
    pub const fn title(self) -> &'static str {
        match self {
            Self::Ecosystem => "本机生态零破坏自检",
            Self::Project => "绑定首个 Git 项目与 Worktree 隔离",
            Self::Pairing => "手机遥控与快捷键速览",
        }
    }

    /// 步骤导读。
    pub const fn subtitle(self) -> &'static str {
        match self {
            Self::Ecosystem => "自动展示本机 Magpie 与原生 CLI 探测结果，只读不写",
            Self::Project => "选择本地目录、克隆仓库或初始化新仓库，并行开发走独立 Worktree",
            Self::Pairing => "扫码用手机遥控；五个最常用快捷键先熟悉一遍",
        }
    }

    /// 主按钮文案（打开时聚焦它）。
    pub const fn primary_action(self) -> &'static str {
        match self {
            Self::Ecosystem | Self::Project => "下一步",
            Self::Pairing => "开始使用 Soxu",
        }
    }

    /// 下一步（末步返回 `None`）。
    pub const fn next(self) -> Option<Self> {
        match self {
            Self::Ecosystem => Some(Self::Project),
            Self::Project => Some(Self::Pairing),
            Self::Pairing => None,
        }
    }

    /// 上一步（首步返回 `None`）。
    pub const fn previous(self) -> Option<Self> {
        match self {
            Self::Ecosystem => None,
            Self::Project => Some(Self::Ecosystem),
            Self::Pairing => Some(Self::Project),
        }
    }

    /// 是否为末步（末步主按钮即「完成」）。
    pub const fn is_last(self) -> bool {
        self.next().is_none()
    }
}

/// 调用方传入的触发标志位（本模块零探测、零 I/O）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OnboardingSignals {
    /// 本次是否为全新环境首次启动。
    pub first_run: bool,
    /// `has_completed_onboarding`（引导已完成）。
    pub has_completed: bool,
    /// 是否已经绑定至少一个工作区。
    pub has_workspace: bool,
}

impl OnboardingSignals {
    /// 构造标志位。
    pub const fn new(first_run: bool, has_completed: bool, has_workspace: bool) -> Self {
        Self {
            first_run,
            has_completed,
            has_workspace,
        }
    }

    /// 是否应当自动弹出：已完成永不弹；否则首次运行或尚无工作区时按需弹出。
    pub const fn should_present(&self) -> bool {
        !self.has_completed && (self.first_run || !self.has_workspace)
    }

    /// 全新环境（首次运行且未完成）。
    pub const fn brand_new() -> Self {
        Self::new(true, false, false)
    }
}

/// 引导结束方式。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OnboardingOutcome {
    /// 走完三步并点击「开始使用 Soxu」。
    Completed,
    /// 任意步骤点了「跳过引导」（或按了 `Esc`）。
    Skipped,
}

/// 三步引导状态机（纯内存）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OnboardingState {
    step: OnboardingStep,
    visible: bool,
    finished: Option<OnboardingOutcome>,
    /// `has_completed_onboarding` 的内存镜像（真实持久化由 daemon 落盘）。
    completed_flag: bool,
    /// 用户已显式关闭过（完成或跳过），此后不再自动弹出。
    dismissed: bool,
}

impl OnboardingState {
    /// 关闭态初始状态（用户从未触发过引导）。
    pub fn new() -> Self {
        Self {
            step: OnboardingStep::Ecosystem,
            visible: false,
            finished: None,
            completed_flag: false,
            dismissed: false,
        }
    }

    /// 按调用方标志位决定是否自动弹出。
    pub fn for_signals(signals: &OnboardingSignals) -> Self {
        let mut state = Self::new();
        state.completed_flag = signals.has_completed;
        state.dismissed = signals.has_completed;
        state.visible = signals.should_present();
        state
    }

    /// 当前步骤。
    pub fn step(&self) -> OnboardingStep {
        self.step
    }

    /// 是否正在展示。
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// 结束方式（未结束为 `None`）。
    pub fn finished(&self) -> Option<OnboardingOutcome> {
        self.finished
    }

    /// `has_completed_onboarding` 的内存镜像（完成才置真，跳过不置）。
    pub fn completed_flag(&self) -> bool {
        self.completed_flag
    }

    /// 顶部进度条已点亮的段数（当前步骤号，1..=3）。
    pub fn progress_filled(&self) -> usize {
        self.step.ordinal()
    }

    /// 进度条总段数。
    pub fn progress_total(&self) -> usize {
        PROGRESS_SEGMENTS
    }

    /// 「Step 2 / 3」编号文案。
    pub fn step_counter(&self) -> String {
        format!("{} / {}", self.step.ordinal(), STEP_COUNT)
    }

    /// 打开 / 重新打开（设置页「重新触发引导」也走这里）。
    pub fn show(&mut self) {
        self.step = OnboardingStep::Ecosystem;
        self.visible = true;
        self.finished = None;
    }

    /// 是否还会自动弹出：用户显式完成或跳过之后一律不再自动弹。
    pub fn should_auto_present(&self, signals: &OnboardingSignals) -> bool {
        !self.dismissed && !signals.has_completed && signals.should_present()
    }

    /// 前进一步；已在末步返回 `false`。
    pub fn advance(&mut self) -> bool {
        if !self.visible {
            return false;
        }
        let Some(next) = self.step.next() else {
            return false;
        };
        self.step = next;
        true
    }

    /// 后退一步；已在首步返回 `false`。
    pub fn retreat(&mut self) -> bool {
        if !self.visible {
            return false;
        }
        let Some(previous) = self.step.previous() else {
            return false;
        };
        self.step = previous;
        true
    }

    /// 跳过引导（任意步骤可用）：关闭并抑制后续自动弹出。
    pub fn skip(&mut self) -> bool {
        if !self.visible {
            return false;
        }
        self.visible = false;
        self.finished = Some(OnboardingOutcome::Skipped);
        self.dismissed = true;
        true
    }

    /// 走完最后一步：写入完成标志、关闭并抑制后续自动弹出。
    ///
    /// 非末步调用返回 `false`（主按钮此时是「下一步」，不构成完成）。
    pub fn complete(&mut self) -> bool {
        if !self.visible || !self.step.is_last() {
            return false;
        }
        self.visible = false;
        self.finished = Some(OnboardingOutcome::Completed);
        self.completed_flag = true;
        self.dismissed = true;
        true
    }

    /// `Esc` 等同跳过（§5 边界表：允许 `Esc` 跳过）。
    pub fn handle_escape(&mut self) -> bool {
        self.skip()
    }

    /// 卡片渲染宽度：桌面固定 `560px`，窄视口按 `92vw` 收缩。
    pub fn card_width_px(viewport_width: f32) -> f32 {
        (viewport_width * MOBILE_WIDTH_VW / 100.0).min(CARD_WIDTH_PX)
    }

    /// 便捷渲染入口（壳层/测试一致）。
    pub fn preview(
        signals: &OnboardingSignals,
        ecosystem: &EcosystemSnapshot,
        palette: &Palette,
    ) -> gpui::Div {
        view::render_card(&Self::for_signals(signals), ecosystem, palette)
    }
}

impl Default for OnboardingState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 触发条件：已完成永不弹；首次运行或无工作区按需弹。
    #[test]
    fn trigger_conditions_follow_the_spec() {
        assert!(OnboardingSignals::brand_new().should_present());
        assert!(OnboardingSignals::new(true, false, true).should_present());
        assert!(OnboardingSignals::new(false, false, false).should_present());
        assert!(!OnboardingSignals::new(false, true, false).should_present());
        assert!(!OnboardingSignals::new(true, true, true).should_present());

        // 已完成的环境即使无工作区也不自动弹。
        let state = OnboardingState::for_signals(&OnboardingSignals::brand_new());
        assert!(state.is_visible());
        assert_eq!(state.step(), OnboardingStep::Ecosystem);
        assert_eq!(state.progress_filled(), 1);

        let done = OnboardingState::for_signals(&OnboardingSignals::new(true, true, false));
        assert!(!done.is_visible());
        assert!(!done.should_auto_present(&OnboardingSignals::new(true, true, false)));
    }

    /// 三步推进：next/back 边界正确，末步才允许完成。
    #[test]
    fn state_machine_walks_three_steps_once() {
        let mut state = OnboardingState::for_signals(&OnboardingSignals::brand_new());
        assert_eq!(state.progress_total(), PROGRESS_SEGMENTS);
        assert_eq!(state.step_counter(), "1 / 3");
        assert_eq!(state.step().primary_action(), "下一步");
        assert!(!state.complete(), "首步不得直接完成");

        assert!(state.advance());
        assert_eq!(state.step(), OnboardingStep::Project);
        assert_eq!(state.progress_filled(), 2);
        assert!(state.advance());
        assert_eq!(state.step(), OnboardingStep::Pairing);
        assert_eq!(state.step_counter(), "3 / 3");
        assert_eq!(state.step().primary_action(), "开始使用 Soxu");
        assert!(!state.advance(), "末步没有下一步");

        assert!(state.complete());
        assert!(!state.is_visible());
        assert_eq!(state.finished(), Some(OnboardingOutcome::Completed));
        assert!(state.completed_flag());
        assert!(!state.complete(), "已结束不得重复完成");

        // 完成后不再自动弹出。
        assert!(!state.should_auto_present(&OnboardingSignals::brand_new()));

        // 设置页可重新触发。
        state.show();
        assert!(state.is_visible());
        assert_eq!(state.step(), OnboardingStep::Ecosystem);

        // back 边界。
        assert!(!state.retreat(), "首步没有上一步");
    }

    /// 任意步骤可跳过，跳过同样抑制后续自动弹出，但不写完成标志。
    #[test]
    fn skipping_from_any_step_suppresses_auto_present() {
        let mut state = OnboardingState::for_signals(&OnboardingSignals::brand_new());
        assert!(state.advance());
        assert_eq!(state.step(), OnboardingStep::Project);
        assert!(state.skip());
        assert!(!state.is_visible());
        assert_eq!(state.finished(), Some(OnboardingOutcome::Skipped));
        assert!(!state.completed_flag(), "跳过不等于完成");
        assert!(!state.should_auto_present(&OnboardingSignals::brand_new()));
        assert!(!state.skip(), "已结束时跳过无效");
        assert!(!state.advance() && !state.retreat(), "已结束时导航无效");
    }

    /// `Esc` 等同跳过。
    #[test]
    fn escape_skips_the_wizard() {
        let mut state = OnboardingState::for_signals(&OnboardingSignals::brand_new());
        assert!(state.handle_escape());
        assert_eq!(state.finished(), Some(OnboardingOutcome::Skipped));
        assert!(!state.handle_escape());
    }

    /// 几何：560 宽 / p20 / r18，窄视口按 92vw 收缩且不溢出。
    #[test]
    fn geometry_matches_the_spec_table() {
        assert_eq!(CARD_WIDTH_PX, 560.0);
        assert_eq!(CARD_PADDING_PX, 20.0);
        assert_eq!(CARD_RADIUS_PX, 18.0);
        assert_eq!(MOBILE_WIDTH_VW, 92.0);
        assert_eq!(OnboardingState::card_width_px(1280.0), 560.0);
        assert!((OnboardingState::card_width_px(400.0) - 368.0).abs() < f32::EPSILON);
        assert_eq!(STEP_COUNT, 3);
        assert_eq!(PROGRESS_SEGMENTS, 3);
    }

    /// 步骤文案与内容常量齐备：即便 CLIP 全部缺失，Step 1 也能完整展示。
    #[test]
    fn step_content_is_complete_even_without_any_probe() {
        for step in OnboardingStep::ALL {
            assert!(!step.title().is_empty());
            assert!(!step.subtitle().is_empty());
            assert!(!step.primary_action().is_empty());
        }
        assert_eq!(NATIVE_CLIS.len(), 7);
        assert_eq!(PROJECT_CHOICES.len(), 3);
        assert_eq!(SHORTCUT_CARDS.len(), 5);
        assert!(PROMISE_BANNER.contains("绝不篡改"));
        assert!(PAIRING_URL_PREFIX.ends_with("#offer="));
        assert!(SHARED_TARGET_HINT.contains("CARGO_TARGET_DIR"));

        let empty = EcosystemSnapshot::default();
        assert_eq!(empty.magpie(), MagpieStatus::StandaloneNative);
        assert!(empty.clis_in_order().is_empty());

        let snapshot = EcosystemSnapshot {
            magpie_managed: Some(true),
            available_clis: vec!["pi", "claude", "not-a-cli"],
        };
        assert_eq!(snapshot.magpie(), MagpieStatus::Managed);
        assert_eq!(snapshot.clis_in_order(), vec!["claude", "pi"]);
        assert_eq!(MagpieStatus::Managed.tone(), "accent");
        assert_eq!(MagpieStatus::StandaloneNative.tone(), "neutral");
    }
}
