//! M5-C 右栏上下文与审阅检视器（`Context Inspector`）。
//!
//! 规范来源（唯一语义来源 `design-system/soxu/**`）：
//! - `pages/01-desktop-workbench.md` §4：右栏四个固定语义页签（`Diff` / `Files` /
//!   `Scripts` / `Terminal`）、`Diff` 工具栏（基准切换 / 单双栏 / `/` 过滤 / 提交入口）、
//!   行号槽与新变更新增删除统计；§6 组件解剖表（页签头 `44px`、`sp(12)` `500`）；
//! - `pages/07-settings-fonts-and-motion.md` §5：`.soxu-auto-scrollbar` 类滚动条与
//!   `0ms` 拖拽期免缓动纪律由布局层承担，本模块只读令牌；
//! - `MASTER.md` §2 表面海拔、§3.2 圆角、§3.3 图标纪律、§4.1 渲染零阻塞。
//!
//! 阶段边界（Issue #133）：
//! - **纯内存视图模型**：不接 daemon I/O、不读文件系统、不起子进程；
//!   工作区遍历与 Diff 拉取在 M6 由后台线程回填本模型的快照；
//! - 工具栏「提交」只**打开模态**（[`RightPanelState::request_commit`] 返回
//!   [`ModalKind::Commit`]），绝不发起任何写入；
//! - 页签只有 `Diff` / `Files` 有内容视图，`Scripts` / `Terminal` 保留语义页签头与
//!   空态占位（M6 接 `paseo.json` 脚本与 PTY）。
//!
//! 子模块：
//! - [`diff`]：归一化 unified diff 解析、行号槽右对齐与 400 行截断；
//! - [`files`]：文件清单快照与词法路径归一化。

pub mod diff;
pub mod files;

use gpui::{Div, Hsla, IntoElement, ParentElement, Styled, div, px};
use soxu_protocol::GitPathChange;

use crate::focus::FocusLayer;
use crate::modals::ModalKind;
use crate::theme::gpui_colors::Palette;
use crate::tokens::{layout, radius, sizing, spacing, typography};

pub use diff::{DiffLine, DiffLineKind, MAX_DIFF_LINES, ParsedDiff, parse_unified_diff};
pub use files::{FileIndex, MAX_LISTED_FILES, normalize_link_path};

/// 右栏页签数量（`pages/01` §4 四合一检视器）。
pub const TAB_COUNT: usize = 4;

/// 页签头高度（`pages/01` §6：右栏 Context 页签头 `h = 44px`）。
pub const TAB_HEADER_HEIGHT_PX: f32 = sizing::TOUCH_HIT_MIN_PX;

/// 双栏视图最小宽度：低于该宽度强制回落单栏（`pages/01` §4「宽 < 480px 强制 Unified」）。
pub const SPLIT_MIN_WIDTH_PX: f32 = 480.0;

/// 空态卡高度（`pages/06` §5 命令面板空态卡 `160px` 的同一收敛值）。
pub const EMPTY_STATE_HEIGHT_PX: f32 = 160.0;

/// 右栏语义页签（`pages/01` §4 固定顺序）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum RightPanelTab {
    /// 实时工作树差异审阅。
    #[default]
    Diff,
    /// 零阻塞工作区文件树。
    Files,
    /// `paseo.json` / 工作区受管脚本面板。
    Scripts,
    /// 受控多窗格 PTY 终端。
    Terminal,
}

impl RightPanelTab {
    /// 全部页签，按设计固定顺序。
    pub const ALL: [Self; TAB_COUNT] = [Self::Diff, Self::Files, Self::Scripts, Self::Terminal];

    /// 页签文案。
    pub const fn label(self) -> &'static str {
        match self {
            Self::Diff => "变更对比",
            Self::Files => "文件树",
            Self::Scripts => "脚本",
            Self::Terminal => "终端",
        }
    }

    /// 页签对应的焦点层：四页签同属审阅上下文（§5.1 `ReviewDiff / ReviewDiffTree`）。
    pub const fn focus_layer(self) -> FocusLayer {
        FocusLayer::ReviewDiff
    }

    /// 页签序号（`0` 起，供 `Ctrl+PageUp/PageDown` 与测试断言）。
    pub const fn index(self) -> usize {
        match self {
            Self::Diff => 0,
            Self::Files => 1,
            Self::Scripts => 2,
            Self::Terminal => 3,
        }
    }

    /// 循环切换（`delta = ±1`，越界回绕）。
    pub fn cycle(self, delta: i32) -> Self {
        let len = i32::try_from(TAB_COUNT).unwrap_or(4);
        let next = (i32::try_from(self.index()).unwrap_or(0) + delta).rem_euclid(len);
        Self::ALL[usize::try_from(next).unwrap_or(0)]
    }
}

/// Diff 基准（`pages/01` §4 顶部工具栏基准切换）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum DiffBase {
    /// 未提交改动。
    #[default]
    Uncommitted,
    /// 对比某基准引用（默认 `origin/main`）。
    BaseRef(String),
    /// 对比指定检查点（如 `#04`）。
    Checkpoint(String),
}

impl DiffBase {
    /// 基准标签文案。
    pub fn label(&self) -> String {
        match self {
            Self::Uncommitted => "未提交改动".to_string(),
            Self::BaseRef(reference) => format!("对比 {reference}"),
            Self::Checkpoint(id) => format!("对比检查点 {id}"),
        }
    }
}

/// Diff 视图形态。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DiffViewMode {
    /// 单栏统一视图。
    #[default]
    Unified,
    /// 双栏并排视图。
    Split,
}

impl DiffViewMode {
    /// 视图文案。
    pub const fn label(self) -> &'static str {
        match self {
            Self::Unified => "单栏",
            Self::Split => "双栏",
        }
    }

    /// 翻转视图形态。
    pub const fn toggled(self) -> Self {
        match self {
            Self::Unified => Self::Split,
            Self::Split => Self::Unified,
        }
    }

    /// 按右栏实时宽度收敛：`< 480px` 强制单栏（设计硬约束）。
    pub fn effective(self, panel_width: f32) -> Self {
        if panel_width < SPLIT_MIN_WIDTH_PX {
            Self::Unified
        } else {
            self
        }
    }
}

/// Diff 数据加载状态（空态 / 错误态的唯一来源）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum DiffLoadState {
    /// 拉取中。
    #[default]
    Loading,
    /// 已就绪（可能没有变更）。
    Ready,
    /// 拉取失败（携带脱敏后的错误摘要）。
    Failed(String),
}

/// 单个变更文件的视图条目（复用协议 `GitPathChange` 契约 + 行统计）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffFile {
    /// 协议层路径变更描述（新增/删除/重命名/修改）。
    pub change: GitPathChange,
    /// 新增行数。
    pub additions: u32,
    /// 删除行数。
    pub deletions: u32,
}

impl DiffFile {
    /// 由协议路径变更与行统计构造。
    pub fn new(change: GitPathChange, additions: u32, deletions: u32) -> Self {
        Self {
            change,
            additions,
            deletions,
        }
    }

    /// 工作区相对路径。
    pub fn path(&self) -> &str {
        &self.change.path
    }

    /// 行统计徽标文案：`+28 -4`。
    pub fn stat_label(&self) -> String {
        format!("+{} -{}", self.additions, self.deletions)
    }
}

/// 空态/错误态卡片内容。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RightPanelEmpty {
    /// 主文案。
    pub title: String,
    /// 说明文案。
    pub description: String,
    /// 是否为错误态（错误态用绯红描边高亮）。
    pub is_error: bool,
}

/// Diff 页签完整状态。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffState {
    /// 当前基准。
    pub base: DiffBase,
    /// 当前视图形态（未按宽度收敛的原始选择）。
    pub mode: DiffViewMode,
    /// 加载状态。
    pub load: DiffLoadState,
    /// 变更文件清单（升序，由后台线程回填）。
    pub files: Vec<DiffFile>,
    /// 当前选中文件下标。
    pub selected: Option<usize>,
    /// `/` 路径过滤词。
    pub filter: String,
    /// 当前选中文件的解析结果。
    pub parsed: ParsedDiff,
}

impl Default for DiffState {
    fn default() -> Self {
        Self {
            base: DiffBase::default(),
            mode: DiffViewMode::default(),
            load: DiffLoadState::Ready,
            files: Vec::new(),
            selected: None,
            filter: String::new(),
            parsed: ParsedDiff::default(),
        }
    }
}

impl DiffState {
    /// 按当前宽度收敛后的视图形态。
    pub fn effective_mode(&self, panel_width: f32) -> DiffViewMode {
        self.mode.effective(panel_width)
    }

    /// 单文件是否命中当前 `/` 过滤词（大小写不敏感子串）。
    pub fn matches_filter(&self, file: &DiffFile) -> bool {
        let needle = self.filter.trim().to_lowercase();
        needle.is_empty() || file.path().to_lowercase().contains(&needle)
    }

    /// 过滤后的文件清单。
    pub fn filtered_files(&self) -> Vec<&DiffFile> {
        self.files
            .iter()
            .filter(|file| self.matches_filter(file))
            .collect()
    }

    /// 覆写文件清单：保持后端顺序稳定性，选中项回落到首个文件。
    pub fn set_files(&mut self, files: Vec<DiffFile>) {
        self.files = files;
        self.selected = (!self.files.is_empty()).then_some(0);
        if self.selected.is_none() {
            self.parsed = ParsedDiff::default();
        }
    }

    /// 选中某文件（越界忽略），并清空旧解析结果等待回填。
    pub fn select(&mut self, index: usize) {
        if index < self.files.len() {
            self.selected = Some(index);
            self.parsed = ParsedDiff::default();
        }
    }

    /// 当前选中文件。
    pub fn selected_file(&self) -> Option<&DiffFile> {
        self.selected.and_then(|index| self.files.get(index))
    }

    /// 回填选中文件的补丁文本并解析（400 行截断在此生效）。
    pub fn set_patch(&mut self, patch: &str) {
        self.parsed = parse_unified_diff(patch);
        self.load = DiffLoadState::Ready;
    }

    /// 置为失败态（错误摘要已脱敏，由调用方保证不含个人路径）。
    pub fn fail(&mut self, message: impl Into<String>) {
        self.load = DiffLoadState::Failed(message.into());
        self.parsed = ParsedDiff::default();
    }

    /// 该页签是否需要渲染空态；返回 `None` 表示可直接渲染文件与行列表。
    pub fn empty_state(&self) -> Option<RightPanelEmpty> {
        match &self.load {
            DiffLoadState::Loading => Some(RightPanelEmpty {
                title: "正在读取变更".to_string(),
                description: "工作树差异在后台线程读取，界面保持可交互".to_string(),
                is_error: false,
            }),
            DiffLoadState::Failed(message) => Some(RightPanelEmpty {
                title: "变更不可用".to_string(),
                description: message.clone(),
                is_error: true,
            }),
            DiffLoadState::Ready => {
                if self.files.is_empty() {
                    Some(RightPanelEmpty {
                        title: "暂无变更".to_string(),
                        description: "工作树与所选基准一致".to_string(),
                        is_error: false,
                    })
                } else {
                    None
                }
            }
        }
    }
}

/// 右栏整体状态。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RightPanelState {
    /// 当前页签。
    pub active: RightPanelTab,
    /// Diff 页签状态。
    pub diff: DiffState,
    /// 文件清单快照。
    pub files: FileIndex,
    /// `Files` 页签的 `/` 过滤词。
    pub file_filter: String,
    /// 是否有待消费的「打开提交模态」请求。
    commit_pending: bool,
}

impl RightPanelState {
    /// 空态右栏：`Diff` 页签、无变更、空文件快照。
    pub fn placeholder() -> Self {
        Self::default()
    }

    /// 切换页签。
    pub fn activate(&mut self, tab: RightPanelTab) {
        self.active = tab;
    }

    /// 按 `delta` 循环切换页签。
    pub fn cycle_tab(&mut self, delta: i32) {
        self.active = self.active.cycle(delta);
    }

    /// `Files` 页签过滤后的路径。
    pub fn visible_files(&self) -> Vec<&str> {
        self.files.filter(&self.file_filter)
    }

    /// 点击工具栏「提交」：只产生打开提交模态的请求，**不做任何写入**。
    ///
    /// 返回 `None` 表示当前已有未消费请求（避免重复弹窗）。
    pub fn request_commit(&mut self) -> Option<ModalKind> {
        if self.commit_pending {
            return None;
        }
        self.commit_pending = true;
        Some(ModalKind::Commit)
    }

    /// 消费挂起请求（模态宿主打开后调用）。
    pub fn consume_commit_request(&mut self) -> bool {
        std::mem::take(&mut self.commit_pending)
    }

    /// 是否有未消费的提交请求。
    pub fn commit_requested(&self) -> bool {
        self.commit_pending
    }
}

/// 携带 alpha 的颜色副本（柔和行底色，不新增令牌）。
fn with_alpha(color: Hsla, alpha: f32) -> Hsla {
    Hsla { a: alpha, ..color }
}

/// 空态/错误态卡片：`160px` 定高、居中双行文案。
fn empty_card(empty: &RightPanelEmpty, palette: &Palette) -> Div {
    let mut card = div()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(spacing::SPACE_1_5_PX))
        .h(px(EMPTY_STATE_HEIGHT_PX))
        .w_full()
        .px(px(spacing::SPACE_4_PX))
        .bg(palette.surface_card)
        .rounded(px(radius::RADIUS_MD_PX));
    if empty.is_error {
        card = card.border_1().border_color(palette.semantic_danger);
    }
    card.child(
        div()
            .text_size(px(typography::TEXT_CAPTION_PX))
            .text_color(palette.text_secondary)
            .child(empty.title.clone()),
    )
    .child(
        div()
            .text_size(px(typography::TEXT_MICRO_PX))
            .text_color(palette.text_tertiary)
            .child(empty.description.clone()),
    )
}

/// 页签头（`44px`，选中项浮起一档表面海拔）。
fn tab_header(state: &RightPanelState, palette: &Palette) -> Div {
    let mut header = div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(spacing::SPACE_2_PX))
        .h(px(TAB_HEADER_HEIGHT_PX))
        .px(px(spacing::SPACE_3_PX))
        .bg(palette.surface_sidebar)
        .border_b_1()
        .border_color(palette.border_hairline);
    for tab in RightPanelTab::ALL {
        let selected = state.active == tab;
        let mut button = div()
            .flex()
            .items_center()
            .h(px(spacing::SPACE_6_PX))
            .px(px(spacing::SPACE_2_PX))
            .rounded(px(radius::RADIUS_XS_PX))
            .text_size(px(typography::TEXT_CAPTION_PX))
            .child(tab.label());
        button = if selected {
            button
                .bg(palette.surface_raised)
                .text_color(palette.text_primary)
                .border_1()
                .border_color(palette.border_subtle)
        } else {
            button.text_color(palette.text_tertiary)
        };
        header = header.child(button);
    }
    header
}

/// Diff 工具栏：基准、视图形态、`/` 过滤与提交入口（只开模态）。
fn diff_toolbar(state: &RightPanelState, palette: &Palette) -> Div {
    let mut toolbar = div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(spacing::SPACE_2_PX))
        .h(px(sizing::TABBAR_HEIGHT_PX))
        .px(px(spacing::SPACE_3_PX))
        .bg(palette.surface_sidebar)
        .border_b_1()
        .border_color(palette.border_hairline)
        .text_size(px(typography::TEXT_MICRO_PX));

    toolbar = toolbar.child(
        div()
            .px(px(spacing::SPACE_1_5_PX))
            .py(px(spacing::SPACE_HALF_PX))
            .rounded(px(radius::RADIUS_XS_PX))
            .bg(palette.surface_raised)
            .border_1()
            .border_color(palette.border_subtle)
            .text_color(palette.text_secondary)
            .child(state.diff.base.label()),
    );
    toolbar = toolbar.child(
        div()
            .px(px(spacing::SPACE_1_5_PX))
            .py(px(spacing::SPACE_HALF_PX))
            .rounded(px(radius::RADIUS_XS_PX))
            .text_color(palette.text_tertiary)
            .child(state.diff.mode.label()),
    );

    // `/` 过滤：有词时显示过滤词，无词时显示快捷键提示。
    let filter_label = if state.diff.filter.trim().is_empty() {
        "/ 过滤文件".to_string()
    } else {
        format!("/ {}", state.diff.filter)
    };
    toolbar = toolbar.child(
        div()
            .flex_1()
            .min_w_0()
            .text_color(if state.diff.filter.trim().is_empty() {
                palette.text_ghost
            } else {
                palette.text_secondary
            })
            .truncate()
            .child(filter_label),
    );

    // 提交入口：仅打开模态（点击回调由 M6 接线到 `ModalHost`）。
    toolbar.child(
        div()
            .flex()
            .items_center()
            .justify_center()
            .h(px(spacing::SPACE_5_PX))
            .px(px(spacing::SPACE_2_PX))
            .rounded(px(radius::RADIUS_XS_PX))
            .bg(palette.action_inverse)
            .text_color(palette.action_on_inverse)
            .child("提交"),
    )
}

/// 单行 Diff：右侧行号槽（右对齐） + 增删删标记 + 等宽行文本。
fn diff_row(line: &DiffLine, palette: &Palette) -> Div {
    let (row_bg, text_color, gutter_color) = match line.kind {
        DiffLineKind::Addition => (
            with_alpha(palette.semantic_success, 0.10),
            palette.text_primary,
            palette.semantic_success,
        ),
        DiffLineKind::Removal => (
            with_alpha(palette.semantic_danger, 0.10),
            palette.text_primary,
            palette.semantic_danger,
        ),
        DiffLineKind::Hunk => (
            palette.surface_card,
            palette.text_tertiary,
            palette.text_tertiary,
        ),
        DiffLineKind::Context => (
            palette.surface_canvas,
            palette.text_secondary,
            palette.text_ghost,
        ),
    };

    div()
        .flex()
        .flex_row()
        .items_center()
        .w_full()
        .min_w_0()
        .bg(row_bg)
        .child(
            // 行号槽：定宽等宽数字，右对齐保证代码列不跳动。
            div()
                .flex()
                .flex_row()
                .flex_none()
                .w(px(diff::gutter_side_width_px() * 2.0))
                .justify_end()
                .gap(px(spacing::SPACE_1_PX))
                .px(px(spacing::SPACE_1_PX))
                .border_r_1()
                .border_color(palette.border_hairline)
                .text_size(px(typography::TEXT_CODE_PX))
                .text_color(gutter_color)
                .child(line.old_gutter())
                .child(line.new_gutter()),
        )
        .child(
            div()
                .flex_none()
                .w(px(spacing::SPACE_3_PX))
                .text_size(px(typography::TEXT_CODE_PX))
                .text_color(gutter_color)
                .child(line.kind.marker()),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .whitespace_nowrap()
                .text_size(px(typography::TEXT_CODE_PX))
                .text_color(text_color)
                .child(line.text.clone()),
        )
}

/// 文件清单行：路径 + 行统计。
fn diff_file_row(file: &DiffFile, selected: bool, palette: &Palette) -> Div {
    let mut row = div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(spacing::SPACE_2_PX))
        .h(px(sizing::SIDEBAR_ROW_HEIGHT_PX))
        .px(px(spacing::SPACE_3_PX))
        .rounded(px(radius::RADIUS_SM_PX))
        .text_size(px(typography::TEXT_CAPTION_PX));
    row = if selected {
        row.bg(palette.interaction_selected)
            .text_color(palette.text_primary)
    } else {
        row.text_color(palette.text_secondary)
    };
    row.child(
        div()
            .flex_1()
            .min_w_0()
            .truncate()
            .child(file.path().to_string()),
    )
    .child(
        div()
            .flex_none()
            .text_color(palette.text_tertiary)
            .child(file.stat_label()),
    )
}

/// Diff 页签内容区：空态 / 错误态 / 文件清单 + 行列表 + 截断提示。
fn diff_body(state: &RightPanelState, palette: &Palette) -> Div {
    let mut body = div().flex().flex_col().flex_1().min_h_0().w_full();

    if let Some(empty) = state.diff.empty_state() {
        return body
            .p(px(spacing::SPACE_3_PX))
            .child(empty_card(&empty, palette));
    }

    // 过滤后无命中：过滤无结果态（与「暂无变更」文案区分）。
    if state.diff.filtered_files().is_empty() {
        let empty = RightPanelEmpty {
            title: "没有匹配的文件".to_string(),
            description: "调整 / 过滤词，或清空后查看全部变更".to_string(),
            is_error: false,
        };
        return body
            .p(px(spacing::SPACE_3_PX))
            .child(empty_card(&empty, palette));
    }

    let mut list = div().flex().flex_col().flex_none().w_full();
    let selected_index = state.diff.selected;
    for (index, file) in state.diff.files.iter().enumerate() {
        if !state.diff.matches_filter(file) {
            continue;
        }
        list = list.child(diff_file_row(file, selected_index == Some(index), palette));
    }
    body = body.child(list);

    // 选中文件的补丁行。
    let mut patch = div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .w_full()
        .bg(palette.surface_canvas)
        .font_family(crate::font::FontConfig::default().effective_code_family());
    for line in &state.diff.parsed.lines {
        patch = patch.child(diff_row(line, palette));
    }
    if let Some(notice) = state.diff.parsed.truncation_notice() {
        patch = patch.child(
            div()
                .px(px(spacing::SPACE_3_PX))
                .py(px(spacing::SPACE_2_PX))
                .text_size(px(typography::TEXT_MICRO_PX))
                .text_color(palette.text_tertiary)
                .child(notice),
        );
    }
    body.child(patch)
}

/// `Files` 页签内容区：路径清单、空快照态与过滤无结果态。
fn files_body(state: &RightPanelState, palette: &Palette) -> Div {
    let body = div().flex().flex_col().flex_1().min_h_0().w_full();
    if state.files.is_empty() {
        let empty = RightPanelEmpty {
            title: "尚未载入文件树".to_string(),
            description: "工作区遍历在后台线程执行，完成后回填只读快照".to_string(),
            is_error: false,
        };
        return body
            .p(px(spacing::SPACE_3_PX))
            .child(empty_card(&empty, palette));
    }
    if state.files.is_filtered_empty(&state.file_filter) {
        let empty = RightPanelEmpty {
            title: "没有匹配的文件".to_string(),
            description: "调整过滤词后重试".to_string(),
            is_error: false,
        };
        return body
            .p(px(spacing::SPACE_3_PX))
            .child(empty_card(&empty, palette));
    }
    let mut list = div().flex().flex_col().w_full();
    for path in state.visible_files() {
        list = list.child(
            div()
                .flex()
                .items_center()
                .h(px(sizing::SIDEBAR_ROW_HEIGHT_PX))
                .px(px(spacing::SPACE_3_PX))
                .rounded(px(radius::RADIUS_SM_PX))
                .text_size(px(typography::TEXT_CAPTION_PX))
                .text_color(palette.text_secondary)
                .truncate()
                .child(path.to_string()),
        );
    }
    body.child(list)
}

/// 未接线页签（`Scripts` / `Terminal`）的语义占位空态。
fn pending_body(tab: RightPanelTab, palette: &Palette) -> Div {
    let (title, description) = match tab {
        RightPanelTab::Scripts => ("脚本面板待接线", "M6 从 paseo.json 读取受管脚本与端口徽章"),
        RightPanelTab::Terminal => ("终端面板待接线", "M6 接入受控多窗格 PTY 与断线恢复"),
        _ => ("即将到来", "该页签在后续里程碑接线"),
    };
    let empty = RightPanelEmpty {
        title: title.to_string(),
        description: description.to_string(),
        is_error: false,
    };
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .w_full()
        .p(px(spacing::SPACE_3_PX))
        .child(empty_card(&empty, palette))
}

/// 渲染右栏（页签头 + 工具栏 + 内容区）。
pub fn render(state: &RightPanelState, palette: &Palette) -> Div {
    let mut panel = div()
        .flex()
        .flex_col()
        .flex_none()
        .w(px(layout::INSPECTOR_DEFAULT_PX))
        .h_full()
        .min_h_0()
        .bg(palette.surface_sidebar)
        .border_l_1()
        .border_color(palette.border_hairline)
        .child(tab_header(state, palette));

    if state.active == RightPanelTab::Diff {
        panel = panel.child(diff_toolbar(state, palette));
    }
    let content = match state.active {
        RightPanelTab::Diff => diff_body(state, palette),
        RightPanelTab::Files => files_body(state, palette),
        other => pending_body(other, palette),
    };
    panel.child(content)
}

/// 壳层挂载入口：以空态快照渲染（M6 改为持有真实状态）。
///
/// 纯渲染、零 I/O：每帧构造一个空态快照，渲染路径与真实状态完全一致，
/// 便于 M6 直接替换为持久的 `RightPanelState`。
pub fn render_default(palette: &Palette) -> impl IntoElement {
    render(&RightPanelState::placeholder(), palette)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::focus::{FocusDecision, FocusStack};
    use crate::modals::ModalHost;
    use soxu_protocol::GitPathChangeKind;

    fn change(kind: GitPathChangeKind, path: &str) -> GitPathChange {
        GitPathChange {
            kind,
            path: path.to_string(),
            previous_path: None,
        }
    }

    /// 四页签固定顺序、文案与审阅焦点层。
    #[test]
    fn tabs_follow_design_order() {
        assert_eq!(RightPanelTab::ALL.len(), TAB_COUNT);
        assert_eq!(RightPanelTab::ALL[0], RightPanelTab::Diff);
        assert_eq!(RightPanelTab::ALL[1], RightPanelTab::Files);
        assert_eq!(RightPanelTab::ALL[2], RightPanelTab::Scripts);
        assert_eq!(RightPanelTab::ALL[3], RightPanelTab::Terminal);
        assert_eq!(RightPanelTab::Diff.label(), "变更对比");
        assert_eq!(RightPanelTab::Files.focus_layer(), FocusLayer::ReviewDiff);
        // 页签头高度取 44px 无障碍热区令牌。
        assert_eq!(TAB_HEADER_HEIGHT_PX, layout::TOUCH_TARGET_MIN_PX);
    }

    /// 页签循环切换回绕。
    #[test]
    fn tab_cycling_wraps() {
        assert_eq!(RightPanelTab::Diff.cycle(1), RightPanelTab::Files);
        assert_eq!(RightPanelTab::Terminal.cycle(1), RightPanelTab::Diff);
        assert_eq!(RightPanelTab::Diff.cycle(-1), RightPanelTab::Terminal);

        let mut state = RightPanelState::placeholder();
        assert_eq!(state.active, RightPanelTab::Diff);
        state.cycle_tab(1);
        assert_eq!(state.active, RightPanelTab::Files);
        state.activate(RightPanelTab::Terminal);
        assert_eq!(state.active, RightPanelTab::Terminal);
    }

    /// 视图形态：`< 480px` 强制单栏。
    #[test]
    fn split_forces_unified_below_480() {
        assert_eq!(DiffViewMode::Split.effective(479.0), DiffViewMode::Unified);
        assert_eq!(DiffViewMode::Split.effective(480.0), DiffViewMode::Split);
        assert_eq!(
            DiffViewMode::Unified.effective(1200.0),
            DiffViewMode::Unified
        );
        assert_eq!(DiffViewMode::Split.toggled(), DiffViewMode::Unified);
        assert_eq!(SPLIT_MIN_WIDTH_PX, 480.0);

        let diff = DiffState {
            mode: DiffViewMode::Split,
            ..DiffState::default()
        };
        assert_eq!(diff.effective_mode(380.0), DiffViewMode::Unified);
        assert_eq!(diff.effective_mode(620.0), DiffViewMode::Split);
    }

    /// 基准切换文案与默认值。
    #[test]
    fn diff_base_labels() {
        assert_eq!(DiffBase::default(), DiffBase::Uncommitted);
        assert_eq!(DiffBase::Uncommitted.label(), "未提交改动");
        assert_eq!(
            DiffBase::BaseRef("origin/main".to_string()).label(),
            "对比 origin/main"
        );
        assert_eq!(
            DiffBase::Checkpoint("#04".to_string()).label(),
            "对比检查点 #04"
        );
    }

    /// 三种空/错态各自给出独立文案，错误态标记为绯红。
    #[test]
    fn empty_and_error_states_are_distinct() {
        let mut diff = DiffState {
            load: DiffLoadState::Loading,
            ..DiffState::default()
        };
        let loading = diff.empty_state().expect("加载中应有空态");
        assert!(!loading.is_error);

        diff.fail("索引不存在");
        let failed = diff.empty_state().expect("失败应有错误态");
        assert!(failed.is_error);
        assert_eq!(failed.description, "索引不存在");
        assert!(diff.parsed.is_empty());

        diff.load = DiffLoadState::Ready;
        let empty = diff.empty_state().expect("无变更应有空态");
        assert_eq!(empty.title, "暂无变更");
        assert!(!empty.is_error);

        diff.set_files(vec![DiffFile::new(
            change(GitPathChangeKind::Modified, "src/lib.rs"),
            3,
            1,
        )]);
        assert!(diff.empty_state().is_none(), "有文件时不应再渲染空态");
    }

    /// 行号槽与 400 行截断在右栏模型上端到端生效。
    #[test]
    fn patch_is_truncated_at_four_hundred_lines() {
        let mut state = RightPanelState::placeholder();
        state.diff.set_files(vec![DiffFile::new(
            change(GitPathChangeKind::Modified, "crates/soxu-core/src/lib.rs"),
            5,
            2,
        )]);
        let body = (0..MAX_DIFF_LINES + 30)
            .map(|index| format!("+line {index}"))
            .collect::<Vec<_>>()
            .join("\n");
        state.diff.set_patch(&body);

        assert_eq!(state.diff.parsed.lines.len(), MAX_DIFF_LINES);
        assert!(state.diff.parsed.truncated);
        assert_eq!(
            state.diff.parsed.truncation_notice().as_deref(),
            Some("差异过长，仅显示前 400 行")
        );
        // 首行新侧行号从 1 起，右对齐定宽。
        let first = state.diff.parsed.lines.first().expect("应有首行");
        assert_eq!(first.new_line, Some(1));
        assert_eq!(first.new_gutter(), "   1");
        assert_eq!(first.old_gutter(), "    ");
    }

    /// 选中与过滤：越界忽略，过滤大小写不敏感。
    #[test]
    fn selection_and_filtering() {
        let mut diff = DiffState::default();
        diff.set_files(vec![
            DiffFile::new(
                change(GitPathChangeKind::Modified, "crates/soxu-core/src/lib.rs"),
                5,
                2,
            ),
            DiffFile::new(
                change(GitPathChangeKind::Added, "docs/ops/PROGRESS.md"),
                4,
                0,
            ),
        ]);
        assert_eq!(diff.selected, Some(0));
        diff.select(1);
        assert_eq!(
            diff.selected_file().map(DiffFile::path),
            Some("docs/ops/PROGRESS.md")
        );
        diff.select(9);
        assert_eq!(diff.selected, Some(1), "越界选择应被忽略");

        diff.filter = "SOXU-CORE".to_string();
        assert_eq!(diff.filtered_files().len(), 1);
        diff.filter = String::new();
        assert_eq!(diff.filtered_files().len(), 2);

        // 行统计徽标沿用 `+28 -4` 契约。
        assert_eq!(diff.files[0].stat_label(), "+5 -2");
    }

    /// 提交入口只打开模态：产生一次 `ModalKind::Commit` 请求并在模态宿主中锁定焦点。
    #[test]
    fn commit_entry_only_opens_modal() {
        let mut state = RightPanelState::placeholder();
        assert!(!state.commit_requested());

        let kind = state.request_commit().expect("首次点击应打开模态");
        assert_eq!(kind, ModalKind::Commit);
        assert!(state.commit_requested(), "请求保持挂起直到被消费");
        assert!(
            state.request_commit().is_none(),
            "已有挂起请求时不得重复弹窗"
        );

        // 模态宿主：打开后模态锁定生效，下层无法夺焦。
        let mut host = ModalHost::new();
        let mut stack = FocusStack::new();
        stack.push(FocusLayer::ReviewDiff);
        assert_eq!(stack.push(FocusLayer::ModalDialog), FocusDecision::Allowed);
        assert_eq!(
            stack.push(FocusLayer::ComposerInput),
            FocusDecision::RejectedModalLock
        );

        assert!(host.open_modal(kind));
        assert_eq!(host.kind(), Some(ModalKind::Commit));
        assert!(state.consume_commit_request());
        assert!(!state.commit_requested());
    }

    /// 文件清单快照的过滤与空态区分。
    #[test]
    fn files_snapshot_filtering() {
        let mut state = RightPanelState::placeholder();
        assert!(state.files.is_empty());
        state.files = FileIndex::new(["src/main.rs", "docs/a.md"]);
        state.file_filter = "SRC".to_string();
        assert_eq!(state.visible_files(), vec!["src/main.rs"]);
        state.file_filter = "zzz".to_string();
        assert!(state.visible_files().is_empty());
        assert!(state.files.is_filtered_empty("zzz"));
    }
}
