//! 5 类结构化工具卡：折叠/展开、限流截断与「构建一次」快照（纯逻辑）。
//!
//! 设计基准：`pages/03-timeline-tools-and-checkpoints.md` §2（5 类卡片）与 §6（限流表）。
//! 参考 Padu `apps/desktop/src/app/transcript_view.rs` 的
//! `CHANGED_FILES_PREVIEW_LIMIT / CHANGED_FILES_EXPANDED_LIMIT / ACTIVITY_PREVIEW_LIMIT /
//! ACTIVITY_DIFF_MAX_HEIGHT` 结构，但常量与语义全部取自本仓设计页：
//!
//! | 限额 | 值 | 出处 |
//! | :--- | :--- | :--- |
//! | 工具卡折叠态单行 | `34px` | 03 §2 表头 |
//! | 内联 Diff 预览 | 最多 `400` 行 | 03 §2 第 2 行 / §6 第 3 行 |
//! | 只读活动组预览 | `4` 条 | 03 §6 第 1 行 |
//! | 本轮变更文件列表 | 折叠 `3` / 展开 `12` | 03 §6 第 2 行 |
//! | 失败终端默认展开 | 末尾 `25` 行 | 03 §2 第 3 行 |
//!
//! **逐帧禁则**：展开的 Diff 快照只在点击时构建一次
//! （[`EditCard::diff_preview`] 用 `Rc` 缓存），虚拟列表逐帧渲染不得重复解析工具输出。

use std::rc::Rc;

use crate::theme::tokens::color::dark;
use crate::theme::tokens::color::light;
use crate::timeline::model::{RowId, RowTone, TimelineRow, TimelineRowKind};
use crate::timeline::text;

/// 工具卡折叠态单行高度（px）。
pub const CARD_FOLDED_HEIGHT_PX: f32 = 34.0;

/// 内联 Diff 直接渲染的最大行数（超出部分折叠为「展开剩余 N 行」）。
pub const DIFF_PREVIEW_MAX_LINES: usize = 400;

/// 只读活动组折叠态预览条数。
pub const ACTIVITY_PREVIEW_LIMIT: usize = 4;

/// 本轮变更文件列表折叠态条数。
pub const CHANGED_FILES_PREVIEW_LIMIT: usize = 3;

/// 本轮变更文件列表展开态上限。
pub const CHANGED_FILES_EXPANDED_LIMIT: usize = 12;

/// 终端命令失败时默认展开的末尾行数。
pub const TERMINAL_FAILURE_TAIL_LINES: usize = 25;

/// 5 类结构化工具卡。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ToolKind {
    /// 只读探索聚合组（`Read` / `Search` / `Glob`）。
    ReadGroup,
    /// 文件编辑与新建（`Edit` / `Write` / `Patch`）。
    FileEdit,
    /// 终端命令执行（`Bash` / `PowerShell`）。
    Terminal,
    /// MCP 工具调用（`aoci_*` / `codegraph_*`）。
    Mcp,
    /// 子代理委派（`Subagent` / `Handoff`）。
    Subagent,
}

impl ToolKind {
    /// 全部类别（顺序即设计表格顺序）。
    pub const ALL: [Self; 5] = [
        Self::ReadGroup,
        Self::FileEdit,
        Self::Terminal,
        Self::Mcp,
        Self::Subagent,
    ];

    /// 类别图标（`22×22px` 圆角图标方块内的矢量字符，03 §7 第 1 条）。
    pub const fn icon(self) -> &'static str {
        match self {
            Self::ReadGroup => "🔍",
            Self::FileEdit => "✏️",
            Self::Terminal => "❯",
            Self::Mcp => "🔌",
            Self::Subagent => "🤖",
        }
    }

    /// 类别标题（紧凑类型标题，避免「输入/输出」冗余文本）。
    pub const fn title(self) -> &'static str {
        match self {
            Self::ReadGroup => "只读探索",
            Self::FileEdit => "文件编辑",
            Self::Terminal => "终端命令",
            Self::Mcp => "MCP 工具",
            Self::Subagent => "子代理委派",
        }
    }

    /// 是否属于「连续可自动合并」的轻量只读类别。
    pub const fn is_read_only(self) -> bool {
        matches!(self, Self::ReadGroup)
    }

    /// 图标方块底色令牌（read 灰 / edit 琥珀 / bash 主题强调 / mcp 紫罗兰）。
    pub const fn dark_accent_token(self) -> u32 {
        match self {
            Self::ReadGroup => dark::SURFACE_CONTROL,
            Self::FileEdit => dark::SEMANTIC_WARNING,
            Self::Terminal => dark::SEMANTIC_ACCENT,
            Self::Mcp => dark::SEMANTIC_HANDOFF,
            Self::Subagent => dark::SEMANTIC_RUNNING,
        }
    }

    /// 浅色模式对应的图标方块底色令牌。
    pub const fn light_accent_token(self) -> u32 {
        match self {
            Self::ReadGroup => light::SURFACE_CONTROL,
            Self::FileEdit => light::SEMANTIC_WARNING,
            Self::Terminal => light::SEMANTIC_ACCENT,
            Self::Mcp => light::SEMANTIC_HANDOFF,
            Self::Subagent => light::SEMANTIC_RUNNING,
        }
    }
}

/// 把一段连续的工具类别合并为活动组（连续同类只读自动折叠成 1 行）。
///
/// 规范（03 §2）：连续发生的同类轻量只读工具自动折叠成「活动组」，
/// 写操作与命令执行独立成卡。返回值按出现顺序给出 `(类别, 归并数量)`。
pub fn merge_activity_groups(kinds: &[ToolKind]) -> Vec<(ToolKind, usize)> {
    let mut groups: Vec<(ToolKind, usize)> = Vec::new();
    for kind in kinds {
        match groups.last_mut() {
            Some((last, count)) if last == kind => *count += 1,
            _ => groups.push((*kind, 1)),
        }
    }
    groups
}

/// 内联 Diff 预览的截断状态。
///
/// 构建一次后只做 `toggle`，不重新解析差异（03 §2 第 2 行末句）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiffPreview {
    /// 差异总行数。
    pub total_lines: usize,
    /// 是否已展开到全部行。
    pub expanded: bool,
}

impl DiffPreview {
    /// 由差异总行数构建（默认折叠到 400 行上限）。
    pub fn new(total_lines: usize) -> Self {
        Self {
            total_lines,
            expanded: false,
        }
    }

    /// 当前实际渲染的行数。
    pub fn visible_lines(&self) -> usize {
        if self.expanded || self.total_lines <= DIFF_PREVIEW_MAX_LINES {
            self.total_lines
        } else {
            DIFF_PREVIEW_MAX_LINES
        }
    }

    /// 被折叠隐藏的行数（折叠态下实际不渲染的行）。
    pub fn hidden_lines(&self) -> usize {
        self.total_lines - self.visible_lines()
    }

    /// 收起后将隐藏的行数（展开态下用于「隐藏剩余 N 行」提示）。
    pub fn collapsible_lines(&self) -> usize {
        self.total_lines.saturating_sub(DIFF_PREVIEW_MAX_LINES)
    }

    /// 是否存在可展开的隐藏行。
    pub fn can_expand(&self) -> bool {
        self.hidden_lines() > 0 && !self.expanded
    }

    /// 是否存在可收起的隐藏行。
    pub fn can_collapse(&self) -> bool {
        self.expanded && self.total_lines > DIFF_PREVIEW_MAX_LINES
    }

    /// 切换展开/收起；无隐藏行时状态不变。
    pub fn toggle(&mut self) {
        if self.can_expand() {
            self.expanded = true;
        } else if self.can_collapse() {
            self.expanded = false;
        }
    }

    /// 折叠态/展开态提示文案（`None` 表示无需提示）。
    ///
    /// 折叠态提示「还能展开多少行」，展开态提示「收起后会隐藏多少行」——
    /// 后者必须用 [`DiffPreview::collapsible_lines`]，因为展开后 `hidden_lines()` 为 0。
    pub fn toggle_label(&self) -> Option<String> {
        if self.can_expand() {
            Some(text::diff_expand_label(self.hidden_lines()))
        } else if self.can_collapse() {
            Some(text::diff_collapse_label(self.collapsible_lines()))
        } else {
            None
        }
    }

    /// 渲染容器所需高度上限（03 §6：最大高度 400px，行号槽 52px）。
    pub fn max_height_px(&self) -> f32 {
        crate::tokens::sizing::ACTIVITY_DIFF_MAX_HEIGHT_PX
    }
}

/// 文件编辑卡。
#[derive(Clone, Debug)]
pub struct EditCard {
    /// 目标路径（已剥离工作区前缀）。
    pub path: String,
    /// 新增行数。
    pub additions: u32,
    /// 删除行数。
    pub deletions: u32,
    /// 展开态 Diff 快照缓存：只在首次展开/构建时解析一次。
    preview: Option<Rc<DiffPreview>>,
}

impl EditCard {
    /// 构造编辑卡（尚未构建 Diff 快照）。
    pub fn new(path: impl Into<String>, additions: u32, deletions: u32) -> Self {
        Self {
            path: path.into(),
            additions,
            deletions,
            preview: None,
        }
    }

    /// 折叠态摘要：`修改 crates/soxu-magpie/src/lib.rs (+24 -5)`。
    pub fn summary(&self) -> String {
        text::file_edit_summary(&self.path, self.additions, self.deletions)
    }

    /// 取（必要时构建）Diff 快照；重复调用返回同一 `Rc`，**不重复解析**。
    pub fn diff_preview(&mut self, total_lines: usize) -> Rc<DiffPreview> {
        let total = total_lines.max(self.additions as usize + self.deletions as usize);
        self.preview
            .get_or_insert_with(|| Rc::new(DiffPreview::new(total)))
            .clone()
    }

    /// 是否已构建过快照（供渲染层断言「未在逐帧路径重建」）。
    pub fn is_prepared(&self) -> bool {
        self.preview.is_some()
    }

    /// 转为对话流行（行标题即折叠态摘要，正文行数 = +/- 合计）。
    pub fn row(&self, id: RowId) -> TimelineRow {
        TimelineRow::new(
            id,
            TimelineRowKind::ToolCard,
            self.summary(),
            (self.additions + self.deletions) as usize,
        )
    }
}

/// 只读探索聚合卡。
#[derive(Clone, Debug, Default)]
pub struct ReadGroupCard {
    /// 命中的相对路径（已剥离工作区前缀）。
    pub paths: Vec<String>,
    /// 命中行数合计。
    pub hits: usize,
    /// 耗时（毫秒）。
    pub duration_ms: u64,
    /// 是否展开全部动作。
    pub expanded: bool,
}

impl ReadGroupCard {
    /// 构造只读聚合卡。
    pub fn new(paths: Vec<String>, hits: usize, duration_ms: u64) -> Self {
        Self {
            paths,
            hits,
            duration_ms,
            expanded: false,
        }
    }

    /// 折叠态摘要。
    pub fn summary(&self) -> String {
        let preview = self.preview_paths();
        text::read_group_summary(self.paths.len(), preview, self.duration_ms)
    }

    /// 折叠态预览路径（最多 [`ACTIVITY_PREVIEW_LIMIT`] 条）。
    pub fn preview_paths(&self) -> &[String] {
        let end = self.paths.len().min(ACTIVITY_PREVIEW_LIMIT);
        self.paths.get(..end).unwrap_or(&[])
    }

    /// 折叠态被隐藏的动作条数。
    pub fn hidden_count(&self) -> usize {
        self.paths.len().saturating_sub(ACTIVITY_PREVIEW_LIMIT)
    }

    /// 当前可见行数（折叠 4 条 / 展开全部）。
    pub fn visible_rows(&self) -> usize {
        if self.expanded {
            self.paths.len()
        } else {
            self.paths.len().min(ACTIVITY_PREVIEW_LIMIT)
        }
    }

    /// 超限提示文案（`None` 表示无需提示）。
    pub fn more_label(&self) -> Option<String> {
        (!self.expanded && self.hidden_count() > 0)
            .then(|| text::activity_more_label(self.hidden_count()))
    }

    /// 切换展开/收起。
    pub fn toggle(&mut self) {
        self.expanded = !self.expanded;
    }

    /// 转为对话流行（正文行数 = 当前可见动作条数）。
    pub fn row(&self, id: RowId) -> TimelineRow {
        TimelineRow::new(
            id,
            TimelineRowKind::ToolCard,
            self.summary(),
            self.visible_rows(),
        )
        .expanded(self.expanded)
    }
}

/// 终端命令卡。
#[derive(Clone, Debug)]
pub struct TerminalCard {
    /// 伪命令行（结构化 `pseudoLines`）。
    pub command: String,
    /// 退出码；`None` 表示仍在运行。
    pub exit_code: Option<i32>,
    /// 耗时（毫秒）。
    pub duration_ms: u64,
    /// 输出行。
    pub output: Vec<String>,
    /// 是否展开。
    pub expanded: bool,
}

impl TerminalCard {
    /// 构造终端卡；失败（退出码非 0）时**默认展开**并以绯红徽章标记（03 §2 第 3 行）。
    pub fn new(command: impl Into<String>, exit_code: Option<i32>, duration_ms: u64) -> Self {
        let expanded = matches!(exit_code, Some(code) if code != 0);
        Self {
            command: command.into(),
            exit_code,
            duration_ms,
            output: Vec::new(),
            expanded,
        }
    }

    /// 追加一行输出。
    pub fn push_output(&mut self, line: impl Into<String>) {
        self.output.push(line.into());
    }

    /// 是否运行中。
    pub fn is_running(&self) -> bool {
        self.exit_code.is_none()
    }

    /// 是否失败（退出码非 0）。
    pub fn is_failure(&self) -> bool {
        matches!(self.exit_code, Some(code) if code != 0)
    }

    /// 折叠态摘要。
    pub fn summary(&self) -> String {
        text::terminal_summary(&self.command, self.exit_code, self.duration_ms)
    }

    /// 失败时默认展示的末尾错误输出（最多 25 行）。
    pub fn failure_tail(&self) -> &[String] {
        if !self.is_failure() {
            return &[];
        }
        let start = self
            .output
            .len()
            .saturating_sub(TERMINAL_FAILURE_TAIL_LINES);
        self.output.get(start..).unwrap_or(&[])
    }

    /// 折叠/展开切换；失败态可以收起，但重新渲染时必须再次提示失败。
    pub fn toggle(&mut self) {
        self.expanded = !self.expanded;
    }

    /// 转为对话流行：失败 → 绯红，运行中 → 电光蓝，其余中性。
    pub fn row(&self, id: RowId) -> TimelineRow {
        let tone = if self.is_failure() {
            RowTone::Danger
        } else if self.is_running() {
            RowTone::Running
        } else {
            RowTone::Neutral
        };
        TimelineRow::new(
            id,
            TimelineRowKind::ToolCard,
            self.summary(),
            self.output.len(),
        )
        .expanded(self.expanded)
        .with_tone(tone)
    }
}

/// MCP 路由来源（区分全局共享 MCP 与当前 Worktree 专属索引 MCP）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum McpRoute {
    /// Magpie 共享 MCP Router。
    MagpieShared,
    /// 当前 Worktree 的 `direct` MCP。
    DirectWorkspace,
}

impl McpRoute {
    /// 路由来源标签。
    pub const fn label(self) -> &'static str {
        match self {
            Self::MagpieShared => "Magpie Shared",
            Self::DirectWorkspace => "Direct 工作区",
        }
    }
}

/// MCP 工具卡。
#[derive(Clone, Debug)]
pub struct McpCard {
    /// MCP server 名。
    pub server: String,
    /// 工具名。
    pub tool: String,
    /// 路由来源。
    pub route: McpRoute,
    /// 耗时（毫秒）。
    pub duration_ms: u64,
    /// 结构化键值表（已解析 `structuredContent`，屏蔽冗长内部字段）。
    pub fields: Vec<(String, String)>,
}

impl McpCard {
    /// 构造 MCP 卡。
    pub fn new(
        server: impl Into<String>,
        tool: impl Into<String>,
        route: McpRoute,
        duration_ms: u64,
    ) -> Self {
        Self {
            server: server.into(),
            tool: tool.into(),
            route,
            duration_ms,
            fields: Vec::new(),
        }
    }

    /// 折叠态摘要。
    pub fn summary(&self) -> String {
        text::mcp_summary(
            &self.server,
            &self.tool,
            self.route.label(),
            self.duration_ms,
        )
    }

    /// 转为对话流行（副标题即 MCP 路由来源标签）。
    pub fn row(&self, id: RowId) -> TimelineRow {
        TimelineRow::new(
            id,
            TimelineRowKind::ToolCard,
            self.summary(),
            self.fields.len(),
        )
        .with_subtitle(self.route.label())
    }
}

/// 子代理委派卡。
#[derive(Clone, Debug)]
pub struct SubagentCard {
    /// 子代理名（`pi` / `codex` / `claude` …）。
    pub agent: String,
    /// 模型标识。
    pub model: String,
    /// 一句话意图。
    pub intent: String,
    /// 是否仍在运行。
    pub running: bool,
    /// 是否有待审批请求（卡片边框升级为琥珀呼吸态）。
    pub pending_approval: bool,
    /// 最近 3 条工具活动摘要。
    pub recent_activity: Vec<String>,
}

impl SubagentCard {
    /// 构造子代理卡。
    pub fn new(
        agent: impl Into<String>,
        model: impl Into<String>,
        intent: impl Into<String>,
    ) -> Self {
        Self {
            agent: agent.into(),
            model: model.into(),
            intent: intent.into(),
            running: true,
            pending_approval: false,
            recent_activity: Vec::new(),
        }
    }

    /// 折叠态摘要。
    pub fn summary(&self) -> String {
        text::subagent_summary(&self.agent, &self.model, &self.intent, self.running)
    }

    /// 卡片描边令牌：待审批时升级为琥珀描边，否则常态 `border.subtle`。
    pub fn dark_border_token(&self) -> u32 {
        if self.pending_approval {
            dark::SEMANTIC_WARNING
        } else {
            dark::BORDER_SUBTLE
        }
    }

    /// 浅色模式对应的描边令牌。
    pub fn light_border_token(&self) -> u32 {
        if self.pending_approval {
            light::SEMANTIC_WARNING
        } else {
            light::BORDER_SUBTLE
        }
    }

    /// 最近 3 条活动（按时间正序）。
    pub fn recent_activity(&self) -> &[String] {
        let start = self.recent_activity.len().saturating_sub(3);
        self.recent_activity.get(start..).unwrap_or(&[])
    }

    /// 转为对话流行：待审批 → 琥珀，运行中 → 电光蓝，其余中性。
    pub fn row(&self, id: RowId) -> TimelineRow {
        let tone = if self.pending_approval {
            RowTone::NeedsInput
        } else if self.running {
            RowTone::Running
        } else {
            RowTone::Neutral
        };
        TimelineRow::new(
            id,
            TimelineRowKind::ToolCard,
            self.summary(),
            self.recent_activity.len(),
        )
        .expanded(true)
        .with_tone(tone)
    }
}

/// 「本轮变更文件列表」的可见性收敛（折叠 3 个 / 展开上限 12 个）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChangedFilesVisibility {
    /// 文件总数。
    pub total: usize,
    /// 是否展开。
    pub expanded: bool,
}

impl ChangedFilesVisibility {
    /// 由文件总数构建。
    pub fn new(total: usize) -> Self {
        Self {
            total,
            expanded: false,
        }
    }

    /// 当前可见条数。
    pub fn visible(&self) -> usize {
        if self.expanded {
            self.total.min(CHANGED_FILES_EXPANDED_LIMIT)
        } else {
            self.total.min(CHANGED_FILES_PREVIEW_LIMIT)
        }
    }

    /// 展开后仍被隐藏的条数（超过 12 个时的剩余）。
    pub fn hidden_after_expand(&self) -> usize {
        self.total.saturating_sub(CHANGED_FILES_EXPANDED_LIMIT)
    }

    /// 是否需要「在右侧 Diff 栏查看全部 N 个文件」降级入口。
    pub fn needs_footer(&self) -> bool {
        self.total
            > if self.expanded {
                CHANGED_FILES_EXPANDED_LIMIT
            } else {
                CHANGED_FILES_PREVIEW_LIMIT
            }
    }

    /// 降级入口文案。
    pub fn footer_label(&self) -> Option<String> {
        self.needs_footer()
            .then(|| text::changed_files_more_label(self.total))
    }

    /// 切换展开/收起。
    pub fn toggle(&mut self) {
        self.expanded = !self.expanded;
    }
}

/// 卡片折叠态高度（px）：统一下限 34px，保证单行不塌陷。
pub fn folded_height_px(kind: ToolKind) -> f32 {
    match kind {
        ToolKind::ReadGroup
        | ToolKind::FileEdit
        | ToolKind::Terminal
        | ToolKind::Mcp
        | ToolKind::Subagent => CARD_FOLDED_HEIGHT_PX,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(count: usize) -> Vec<String> {
        (0..count)
            .map(|i| format!("crates/soxu-core/src/f{i}.rs"))
            .collect()
    }

    #[test]
    fn five_kinds_have_icon_and_title() {
        assert_eq!(ToolKind::ALL.len(), 5);
        for kind in ToolKind::ALL {
            assert!(!kind.icon().is_empty());
            assert!(!kind.title().is_empty());
            assert_eq!(folded_height_px(kind), 34.0);
        }
        assert!(ToolKind::ReadGroup.is_read_only());
        assert!(!ToolKind::FileEdit.is_read_only());
        assert!(!ToolKind::Terminal.is_read_only());
    }

    #[test]
    fn diff_preview_truncates_at_400_lines() {
        // 恰好 400 行：全部渲染，无展开入口。
        let exact = DiffPreview::new(DIFF_PREVIEW_MAX_LINES);
        assert_eq!(exact.visible_lines(), 400);
        assert_eq!(exact.hidden_lines(), 0);
        assert!(!exact.can_expand());
        assert_eq!(exact.toggle_label(), None);
        assert!(!exact.expanded);
        assert_eq!(exact.max_height_px(), 400.0);

        // 528 行：折叠渲染 400 行，隐藏 128 行。
        let mut long = DiffPreview::new(528);
        assert_eq!(long.visible_lines(), 400);
        assert_eq!(long.hidden_lines(), 128);
        assert!(long.can_expand());
        assert_eq!(
            long.toggle_label().as_deref(),
            Some("展开剩余 128 行隐藏差异")
        );
        long.toggle();
        assert!(long.expanded);
        assert_eq!(long.visible_lines(), 528);
        assert_eq!(long.hidden_lines(), 0);
        assert_eq!(long.toggle_label().as_deref(), Some("隐藏剩余 128 行"));
        long.toggle();
        assert!(!long.expanded);
        assert_eq!(long.visible_lines(), 400);

        // 空差异不产生提示，也不进入展开态。
        let mut empty = DiffPreview::new(0);
        empty.toggle();
        assert!(!empty.expanded);
        assert_eq!(empty.toggle_label(), None);
    }

    #[test]
    fn edit_card_prepares_diff_snapshot_exactly_once() {
        let mut card = EditCard::new("crates/soxu-magpie/src/lib.rs", 24, 5);
        assert!(!card.is_prepared());
        assert_eq!(
            card.summary(),
            "修改 crates/soxu-magpie/src/lib.rs (+24 -5)"
        );

        let first = card.diff_preview(0);
        assert!(card.is_prepared());
        // 未显式给出行数时，用 +/- 合计作为差异规模。
        assert_eq!(first.total_lines, 29);

        // 逐帧重复取用必须返回同一快照（禁止重复解析）。
        let second = card.diff_preview(9_999);
        assert!(Rc::ptr_eq(&first, &second));
        assert_eq!(second.total_lines, 29, "已构建的快照不得被后续调用改写");

        // 显式大差异按真实行数构建。
        let mut big = EditCard::new("big.rs", 300, 228);
        assert_eq!(big.diff_preview(528).total_lines, 528);
    }

    #[test]
    fn read_group_previews_four_and_expands() {
        let mut group = ReadGroupCard::new(paths(6), 42, 400);
        assert_eq!(
            group.summary(),
            "已检索并读取 6 个文件 (crates/soxu-core/src/f0.rs, crates/soxu-core/src/f1.rs, crates/soxu-core/src/f2.rs, crates/soxu-core/src/f3.rs) · 0.4s"
        );
        assert_eq!(group.preview_paths().len(), ACTIVITY_PREVIEW_LIMIT);
        assert_eq!(group.visible_rows(), 4);
        assert_eq!(group.hidden_count(), 2);
        assert_eq!(group.more_label().as_deref(), Some("展开其余 2 条探索动作"));

        group.toggle();
        assert_eq!(group.visible_rows(), 6);
        assert_eq!(group.more_label(), None);

        // 少于 4 条时不提示。
        let small = ReadGroupCard::new(paths(2), 3, 12);
        assert_eq!(small.more_label(), None);
        assert_eq!(small.visible_rows(), 2);
    }

    #[test]
    fn terminal_failure_auto_expands_tail() {
        let mut failing = TerminalCard::new("cargo clippy", Some(101), 900);
        assert!(failing.is_failure());
        assert!(failing.expanded, "失败命令必须默认展开（03 §2 第 3 行）");
        for i in 0..40 {
            failing.push_output(format!("error[{i}]"));
        }
        let tail = failing.failure_tail();
        assert_eq!(tail.len(), TERMINAL_FAILURE_TAIL_LINES);
        assert_eq!(tail.first().map(String::as_str), Some("error[15]"));
        assert_eq!(tail.last().map(String::as_str), Some("error[39]"));

        // 成功命令默认折叠，且没有失败尾部。
        let success = TerminalCard::new("cargo test", Some(0), 2100);
        assert!(!success.expanded);
        assert!(!success.is_failure());
        assert!(success.failure_tail().is_empty());
        assert_eq!(success.summary(), "cargo test · ✓ 退出码 0 · 2.1s");

        // 运行中：不展开、无退出码。
        let running = TerminalCard::new("pnpm build", None, 300);
        assert!(running.is_running());
        assert!(!running.expanded);
        assert_eq!(running.summary(), "pnpm build · ⟳ 运行中 · 0.3s");
    }

    #[test]
    fn mcp_and_subagent_cards_fold_to_one_line() {
        let mcp = McpCard::new("aoci", "aoci_maintain", McpRoute::DirectWorkspace, 300);
        assert_eq!(
            mcp.summary(),
            "aoci · aoci_maintain [Direct 工作区] · ✓ 0.3s"
        );
        assert_eq!(McpRoute::MagpieShared.label(), "Magpie Shared");

        let mut sub = SubagentCard::new("pi", "deepseek-v4.1-flash", "巡检文档链接");
        assert_eq!(
            sub.summary(),
            "委派 pi (deepseek-v4.1-flash) · 巡检文档链接 · ⟳ 运行中"
        );
        assert_eq!(sub.dark_border_token(), dark::BORDER_SUBTLE);
        // 待审批 → 琥珀呼吸态描边。
        sub.pending_approval = true;
        assert_eq!(sub.dark_border_token(), dark::SEMANTIC_WARNING);
        assert_eq!(sub.light_border_token(), light::SEMANTIC_WARNING);

        // 最近 3 条活动。
        for i in 0..5 {
            sub.recent_activity.push(format!("act{i}"));
        }
        let recent = sub.recent_activity();
        assert_eq!(recent.len(), 3);
        assert_eq!(recent.first().map(String::as_str), Some("act2"));
        assert_eq!(recent.last().map(String::as_str), Some("act4"));
    }

    #[test]
    fn changed_files_preview_and_expand_limits() {
        let mut small = ChangedFilesVisibility::new(2);
        assert_eq!(small.visible(), 2);
        assert!(!small.needs_footer());
        assert_eq!(small.footer_label(), None);

        let mut mid = ChangedFilesVisibility::new(8);
        assert_eq!(mid.visible(), 3);
        assert!(mid.needs_footer());
        assert_eq!(
            mid.footer_label().as_deref(),
            Some("在右侧 Diff 栏查看全部 8 个文件")
        );
        mid.toggle();
        assert_eq!(mid.visible(), 8);
        assert!(!mid.needs_footer());
        mid.toggle();
        assert_eq!(mid.visible(), 3);

        let mut huge = ChangedFilesVisibility::new(40);
        huge.expanded = true;
        assert_eq!(huge.visible(), 12);
        assert_eq!(huge.hidden_after_expand(), 28);
        assert!(huge.needs_footer());
        small.toggle();
        assert_eq!(small.visible(), 2);
    }

    #[test]
    fn cards_project_into_timeline_rows() {
        // 只读组：折叠态，正文行数 = 可见动作数（上限 4）。
        let group = ReadGroupCard::new(paths(6), 5, 400);
        let row = group.row(1);
        assert_eq!(row.kind, TimelineRowKind::ToolCard);
        assert_eq!(row.body_lines, ACTIVITY_PREVIEW_LIMIT);
        assert_eq!(row.tone, RowTone::Neutral);
        assert!(!row.expanded);

        // 失败终端：默认展开 + 绯红。
        let mut terminal = TerminalCard::new("cargo test", Some(1), 10);
        terminal.push_output("boom");
        let row = terminal.row(2);
        assert!(row.expanded);
        assert_eq!(row.tone, RowTone::Danger);
        assert_eq!(row.body_lines, 1);

        // 运行中终端 → 电光蓝。
        assert_eq!(
            TerminalCard::new("pnpm build", None, 1).row(3).tone,
            RowTone::Running
        );

        // 编辑卡正文行数 = +/- 合计。
        assert_eq!(EditCard::new("a.rs", 24, 5).row(4).body_lines, 29);

        // MCP 卡把路由来源放进副标题。
        let mcp = McpCard::new("aoci", "aoci_maintain", McpRoute::DirectWorkspace, 300);
        assert_eq!(mcp.row(5).subtitle.as_deref(), Some("Direct 工作区"));

        // 子代理待审批 → 琥珀呼吸态。
        let mut subagent = SubagentCard::new("pi", "deepseek-v4.1-flash", "巡检文档链接");
        subagent.pending_approval = true;
        assert_eq!(subagent.row(6).tone, RowTone::NeedsInput);
    }

    #[test]
    fn activity_groups_merge_only_contiguous_reads() {
        use ToolKind::*;
        let merged = merge_activity_groups(&[ReadGroup, ReadGroup, FileEdit, ReadGroup, Terminal]);
        assert_eq!(
            merged,
            vec![(ReadGroup, 2), (FileEdit, 1), (ReadGroup, 1), (Terminal, 1)]
        );
        assert!(merge_activity_groups(&[]).is_empty());
        assert_eq!(
            merge_activity_groups(&[Terminal, Terminal, Terminal]),
            vec![(Terminal, 3)]
        );
    }
}
