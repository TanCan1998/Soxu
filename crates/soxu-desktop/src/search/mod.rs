//! 窗口级 `Cmd/Ctrl + K` 全局命令面板（`pages/06-modals-onboarding-and-automation.md` §二）。
//!
//! 五分区检索的**纯内存视图模型**：`Suggested → Sessions → Files → History → Commands`
//! 固定顺序收窄，`90ms` 防抖 + `24` 条 LRU，前缀优先排序，每区最多 8 行，全程零 I/O
//! （不读文件系统 / SQLite / Git，不起子进程，不直连 daemon 或 CodeGraph）。
//!
//! 子模块：
//! - [`model`]：分区、结果项与纯内存目录；
//! - [`rank`]：命中层级（完全相等 → 前缀 → 词首 → 子串）与分区内排序、8 行截断；
//! - [`cache`]：`90ms` 防抖窗口与 `24` 条 LRU；
//! - [`symbols`]：内存符号目录、前缀补全与选中回调；
//! - [`keymap`]：`⌘K` 打开、双段 `Esc`、上下键与分区跳转；
//! - [`view`]：`640 × 440` 面板与标题栏触发条的薄渲染。
//!
//! 几何全部取自设计令牌（[`sizing::CMD_PALETTE_WIDTH_PX`] 等），仅
//! `16px` 圆角与列表行高无对应令牌，按 06 页 §二字面值在本模块单点声明，
//! 避免同一数字散落多处。
//!
//! 参考结构（未移植宿主代码）：
//! - Paseo `packages/app/src/command-center/{sections,list-metrics,interaction}.ts`：
//!   分区 band+rank 总序、行高单一来源、选中态按 ID 跨重排保持；
//! - CodeG `src/lib/file-search-match.ts`：层级 → 位置 → 长度三段评分；
//! - t3code `apps/mobile/src/features/keyboard/commandPaletteItems.ts`：
//!   `nextPaletteIndex` 取模回绕与按组限流。

pub mod cache;
pub mod keymap;
pub mod model;
pub mod rank;
pub mod symbols;
pub mod view;

use crate::focus::{FocusDecision, FocusLayer, FocusStack};
use crate::search::cache::{DebouncedSearch, QueryCache, QueryOutcome};
use crate::search::rank::SearchResultSet;
use crate::theme::gpui_colors::Palette;
use crate::tokens::sizing;

pub use cache::{DEBOUNCE_MS, QUERY_CACHE_CAPACITY};
pub use keymap::{
    FOOTER_HINTS, KeyContext, PaletteKey, TRIGGER_PLACEHOLDER, open_chord_label, resolve,
};
pub use model::{
    MAX_ROWS_PER_SECTION, SECTION_COUNT, SearchAction, SearchCatalog, SearchItem, SearchSection,
};
pub use rank::{MatchField, MatchTier, SearchMatch, SectionResults, score_item};
pub use symbols::{SYMBOL_SUGGESTION_LIMIT, SymbolCatalog, SymbolEntry, SymbolKind};

/// 面板宽度（06 页 §二 `640px`，令牌 `sizing.cmdPaletteWidth`）。
pub const PALETTE_WIDTH_PX: f32 = sizing::CMD_PALETTE_WIDTH_PX;

/// 面板最大高度（06 页 §二 `440px`，令牌 `sizing.cmdPaletteMaxHeight`）。
pub const PALETTE_MAX_HEIGHT_PX: f32 = sizing::CMD_PALETTE_MAX_HEIGHT_PX;

/// 搜索输入区高度（06 页 §二 `52px`，令牌 `sizing.cmdPaletteInputHeight`）。
pub const PALETTE_INPUT_HEIGHT_PX: f32 = sizing::CMD_PALETTE_INPUT_HEIGHT_PX;

/// 顶距视口（06 页 §二 `12vh`，按视口百分比表达，渲染时折算像素）。
pub const PALETTE_TOP_INSET_VH: f32 = 12.0;

/// 面板圆角（06 页 §二 `16px`；令牌圆角刻度无 16，按设计字面值单点声明）。
pub const PALETTE_RADIUS_PX: f32 = 16.0;

/// 分区头高度（06 页 §二 `26px`）。
pub const SECTION_HEADER_HEIGHT_PX: f32 = 26.0;

/// 常规行高（06 页 §二 `40px`）。
pub const ROW_HEIGHT_PX: f32 = 40.0;

/// 带摘要行高（06 页 §二 `54px`）。
pub const ROW_WITH_SUBTITLE_HEIGHT_PX: f32 = 54.0;

/// 文件结果行高（06 页 §二 `48px`）。
pub const FILE_ROW_HEIGHT_PX: f32 = 48.0;

/// 底部键位提示栏高度（06 页 §二 `36px`）。
pub const FOOTER_HEIGHT_PX: f32 = 36.0;

/// 搜索无结果空态卡高度（06 页 §五边界表 `160px`）。
pub const EMPTY_STATE_HEIGHT_PX: f32 = 160.0;

/// 标题栏搜索触发条高度（`pages/01` §1.1 中央搜索框 `32px`）。
pub const TRIGGER_HEIGHT_PX: f32 = 32.0;

/// 单行渲染高度：文件结果行 `48px` 优先，其次带摘要 `54px`，其余 `40px`。
pub fn row_height(item: &SearchItem) -> f32 {
    if item.section() == SearchSection::Files {
        FILE_ROW_HEIGHT_PX
    } else if item.has_subtitle() {
        ROW_WITH_SUBTITLE_HEIGHT_PX
    } else {
        ROW_HEIGHT_PX
    }
}

/// 面板总高度（分区头 + 行 + 输入区 + 键位栏，不超过 `440px`）。
pub fn palette_height(results: &SearchResultSet) -> f32 {
    let headers = results.sections().len() as f32 * SECTION_HEADER_HEIGHT_PX;
    let rows: f32 = results
        .sections()
        .iter()
        .flat_map(|section| section.items())
        .map(row_height)
        .sum();
    let body = if results.is_empty() {
        EMPTY_STATE_HEIGHT_PX
    } else {
        headers + rows
    };
    (PALETTE_INPUT_HEIGHT_PX + body + FOOTER_HEIGHT_PX).min(PALETTE_MAX_HEIGHT_PX)
}

/// 键位裁决结果。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyOutcome {
    /// 按键与面板无关（面板未打开或该键无绑定）。
    Ignored,
    /// 面板被打开。
    Opened,
    /// 双段 `Esc` 第一段：只清空搜索框，焦点仍在面板内。
    Cleared,
    /// 双段 `Esc` 第二段：关闭面板并还原打开前的焦点层。
    Closed,
    /// `↑↓` 移动选中（不重算结果）。
    Moved,
    /// `⇥ / ⇧⇥` 跳到相邻分区首行。
    SectionJumped,
    /// `↵` 执行选中行（携带待执行动作，面板随之关闭）。
    Confirmed(SearchAction),
}

/// 命令面板状态机（纯内存，可在无 GPU 环境下完整测试）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaletteState {
    open: bool,
    query: String,
    results: SearchResultSet,
    /// 选中行标识：跨重排、跨结果刷新按 ID 保位（不是行下标）。
    active: Option<String>,
    search: DebouncedSearch,
    focus: FocusStack,
    /// 打开前的焦点层（关闭后还原）。
    captured: Option<FocusLayer>,
}

impl PaletteState {
    /// 关闭态、空查询、空缓存的初始状态。
    pub fn new() -> Self {
        Self {
            open: false,
            query: String::new(),
            results: SearchResultSet::empty(),
            active: None,
            search: DebouncedSearch::new(),
            focus: FocusStack::new(),
            captured: None,
        }
    }

    /// 面板是否打开。
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// 当前查询串。
    pub fn query(&self) -> &str {
        &self.query
    }

    /// 搜索框是否有文字（决定 `Esc` 走第一段还是第二段）。
    pub fn has_query(&self) -> bool {
        !self.query.is_empty()
    }

    /// 当前结果集。
    pub fn results(&self) -> &SearchResultSet {
        &self.results
    }

    /// 选中行标识。
    pub fn active_id(&self) -> Option<&str> {
        self.active.as_deref()
    }

    /// 选中行。
    pub fn active_item(&self) -> Option<&SearchItem> {
        self.active.as_deref().and_then(|id| self.results.find(id))
    }

    /// 选中行所在分区。
    pub fn active_section(&self) -> Option<SearchSection> {
        self.active_item().map(SearchItem::section)
    }

    /// 查询缓存只读视图（测试与遥测用）。
    pub fn cache(&self) -> &QueryCache {
        self.search.cache()
    }

    /// 是否还有防抖中的待重算查询。
    pub fn is_pending(&self) -> bool {
        self.search.is_pending()
    }

    /// 焦点栈只读视图。
    pub fn focus(&self) -> &FocusStack {
        &self.focus
    }

    /// 显式入栈一个焦点上下文（模拟打开面板前所在层）。
    pub fn push_layer(&mut self, layer: FocusLayer) -> FocusDecision {
        self.focus.push(layer)
    }

    /// 打开面板：捕获当前焦点层、定位首行、立即给出空查询结果（零防抖）。
    ///
    /// 返回 `false` 表示面板已打开，或模态锁定生效（`ModalDialog` 激活时命令面板
    /// 不得夺焦）。
    pub fn open(&mut self, catalog: &SearchCatalog) -> bool {
        if self.open {
            return false;
        }
        let captured = self.focus.active();
        if self.focus.push(FocusLayer::CommandPalette) != FocusDecision::Allowed {
            return false;
        }
        self.captured = captured;
        self.open = true;
        self.query.clear();
        self.results = self.search.results_now("", catalog);
        self.active = self.results.first_id().map(str::to_string);
        true
    }

    /// 关闭面板：清空查询与结果，并把焦点还回打开前所在层。
    pub fn close(&mut self) {
        self.open = false;
        self.query.clear();
        self.results = SearchResultSet::empty();
        self.active = None;
        self.focus.close_active();
        if self.focus.active().is_none()
            && let Some(layer) = self.captured
        {
            self.focus.push(layer);
        }
        self.captured = None;
    }

    /// 双段 `Esc` 第一段：只清空搜索框，恢复空查询结果并保留焦点。
    pub fn clear_query(&mut self, catalog: &SearchCatalog) {
        self.query.clear();
        let results = self.search.results_now("", catalog);
        self.apply_results(results);
    }

    /// 输入一次按键内容：缓存命中即时收窄，否则进入 `90ms` 防抖窗口。
    pub fn set_query(&mut self, query: impl Into<String>, now_ms: u64) -> QueryOutcome {
        self.query = query.into();
        let outcome = self.search.on_input(self.query.clone(), now_ms);
        if let QueryOutcome::Ready(results) = &outcome {
            self.apply_results(results.clone());
        }
        outcome
    }

    /// 推进防抖窗口：到点则重算并应用到结果集，返回是否真的重算。
    pub fn poll(&mut self, now_ms: u64, catalog: &SearchCatalog) -> bool {
        let Some(results) = self.search.poll(now_ms, catalog) else {
            return false;
        };
        self.apply_results(results);
        true
    }

    /// 在当前结果集上移动选中（取模回绕），**不重算、不重新检索**。
    pub fn move_selection(&mut self, delta: i32) -> KeyOutcome {
        let ids: Vec<String> = self
            .results
            .visible_ids()
            .into_iter()
            .map(str::to_string)
            .collect();
        if ids.is_empty() {
            return KeyOutcome::Ignored;
        }
        let count = ids.len() as i32;
        let next = match self
            .active
            .as_ref()
            .and_then(|id| ids.iter().position(|candidate| candidate == id))
        {
            Some(index) => (index as i32 + delta).rem_euclid(count) as usize,
            None if delta < 0 => (count - 1) as usize,
            None => 0,
        };
        self.active = ids.get(next).cloned();
        KeyOutcome::Moved
    }

    /// 按分区跳转：落到相邻非空分区的首行（`direction` 为正即向后）。
    pub fn jump_section(&mut self, direction: i32) -> KeyOutcome {
        let sections = self.results.sections();
        if sections.is_empty() || direction == 0 {
            return KeyOutcome::Ignored;
        }
        let count = sections.len() as i32;
        let current = self
            .active
            .as_deref()
            .and_then(|id| {
                sections
                    .iter()
                    .position(|section| section.items().iter().any(|item| item.id() == id))
            })
            .map_or(0, |index| index as i32);
        let target = (current + direction).rem_euclid(count) as usize;
        self.active = sections
            .get(target)
            .and_then(|section| section.items().first())
            .map(|item| item.id().to_string());
        KeyOutcome::SectionJumped
    }

    /// 点选某行（鼠标悬停/点击与键盘共用同一选中语义）。
    pub fn select(&mut self, id: &str) -> bool {
        if self.results.find(id).is_none() {
            return false;
        }
        self.active = Some(id.to_string());
        true
    }

    /// 执行选中行：取走动作并关闭面板。
    pub fn confirm(&mut self) -> KeyOutcome {
        let Some(item) = self.active_item() else {
            return KeyOutcome::Ignored;
        };
        let action = item.action().clone();
        self.close();
        KeyOutcome::Confirmed(action)
    }

    /// 应用一份新结果集：选中态按 ID 保位，仅在原选中行消失时回落到首行。
    fn apply_results(&mut self, results: SearchResultSet) {
        let keep = self
            .active
            .as_deref()
            .filter(|id| results.find(id).is_some())
            .map(str::to_string);
        self.active = keep.or_else(|| results.first_id().map(str::to_string));
        self.results = results;
    }

    /// 用一份既有目录直接渲染（壳层与测试的便捷入口）。
    pub fn preview(catalog: &SearchCatalog, palette: &Palette) -> gpui::Div {
        let mut state = Self::new();
        state.open(catalog);
        view::render_palette(&state, palette)
    }
}

impl Default for PaletteState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::cache::QueryOutcome;

    fn catalog() -> SearchCatalog {
        let mut catalog = SearchCatalog::seeded();
        catalog.push(SearchItem::new(
            "sess-1",
            SearchSection::Sessions,
            "feat/140 搜索面板",
            SearchAction::OpenSession {
                session_id: "sess-1".to_string(),
            },
        ));
        catalog.push(SearchItem::new(
            "file-1",
            SearchSection::Files,
            "search/mod.rs",
            SearchAction::OpenFile {
                path: "crates/soxu-desktop/src/search/mod.rs".to_string(),
            },
        ));
        catalog
    }

    fn opened() -> (PaletteState, SearchCatalog) {
        let catalog = catalog();
        let mut state = PaletteState::new();
        state.push_layer(FocusLayer::WorkbenchGlobal);
        assert!(state.open(&catalog));
        (state, catalog)
    }

    /// 06 页 §二几何：宽 640 / 最高 440 / 输入区 52 / 分区头 26 / 行高 40·54·48 / 键位栏 36。
    #[test]
    fn palette_geometry_matches_spec() {
        assert_eq!(PALETTE_WIDTH_PX, 640.0);
        assert_eq!(PALETTE_MAX_HEIGHT_PX, 440.0);
        assert_eq!(PALETTE_INPUT_HEIGHT_PX, 52.0);
        assert_eq!(PALETTE_TOP_INSET_VH, 12.0);
        assert_eq!(PALETTE_RADIUS_PX, 16.0);
        assert_eq!(SECTION_HEADER_HEIGHT_PX, 26.0);
        assert_eq!(FOOTER_HEIGHT_PX, 36.0);
        assert_eq!(EMPTY_STATE_HEIGHT_PX, 160.0);

        let plain = SearchItem::new(
            "p",
            SearchSection::Suggested,
            "推荐",
            SearchAction::RunCommand {
                command: "p".to_string(),
            },
        );
        assert_eq!(row_height(&plain), ROW_HEIGHT_PX);
        let with_subtitle = plain.clone().with_subtitle("摘要");
        assert_eq!(row_height(&with_subtitle), ROW_WITH_SUBTITLE_HEIGHT_PX);
        let file = SearchItem::new(
            "f",
            SearchSection::Files,
            "a.rs",
            SearchAction::OpenFile {
                path: "a.rs".to_string(),
            },
        )
        .with_subtitle("src · fn");
        assert_eq!(
            row_height(&file),
            FILE_ROW_HEIGHT_PX,
            "文件结果行 48px 优先于带摘要 54px"
        );

        let empty = SearchResultSet::empty();
        assert_eq!(
            palette_height(&empty),
            PALETTE_INPUT_HEIGHT_PX + EMPTY_STATE_HEIGHT_PX + FOOTER_HEIGHT_PX
        );
    }

    /// 打开即定位首行并进入 CommandPalette 焦点层；关闭还原到打开前所在层。
    #[test]
    fn open_locates_first_row_and_close_restores_focus() {
        let (mut state, _catalog) = opened();
        assert!(state.is_open());
        assert_eq!(state.focus().effective_context(), "CommandPalette");
        assert_eq!(state.active_section(), Some(SearchSection::Suggested));
        assert_eq!(state.active_id(), state.results().first_id());
        assert!(!state.has_query());

        state.close();
        assert!(!state.is_open());
        assert_eq!(state.focus().effective_context(), "WorkbenchGlobal");
        assert!(state.results().is_empty());
        assert_eq!(state.active_id(), None);
    }

    /// 双段 `Esc`：第一次只清空搜索框并保留焦点，第二次才关闭并还原焦点。
    #[test]
    fn double_escape_clears_then_closes() {
        let (mut state, catalog) = opened();
        let ctx = KeyContext::new(0, &catalog);
        assert!(matches!(
            state.set_query("设置", 0),
            QueryOutcome::Pending | QueryOutcome::Ready(_)
        ));
        state.poll(DEBOUNCE_MS, &catalog);
        assert!(state.has_query());

        // 第一段：清空，面板仍然打开、焦点仍在面板。
        assert_eq!(
            resolve(&mut state, PaletteKey::Escape, &ctx),
            KeyOutcome::Cleared
        );
        assert!(!state.has_query());
        assert!(state.is_open());
        assert_eq!(state.focus().effective_context(), "CommandPalette");
        assert!(!state.results().is_empty(), "清空后回到空查询的默认结果");

        // 第二段：关闭并还原焦点。
        assert_eq!(
            resolve(&mut state, PaletteKey::Escape, &ctx),
            KeyOutcome::Closed
        );
        assert!(!state.is_open());
        assert_eq!(state.focus().effective_context(), "WorkbenchGlobal");

        // 已关闭时 Esc 不再被面板消费。
        assert_eq!(
            resolve(&mut state, PaletteKey::Escape, &ctx),
            KeyOutcome::Ignored
        );
    }

    /// `⌘K` 打开幂等；模态锁定时拒绝夺焦。
    #[test]
    fn toggle_opens_once_and_respects_modal_lock() {
        let catalog = catalog();
        let ctx = KeyContext::new(0, &catalog);
        let mut state = PaletteState::new();
        state.push_layer(FocusLayer::WorkbenchGlobal);
        assert_eq!(
            resolve(&mut state, PaletteKey::Toggle, &ctx),
            KeyOutcome::Opened
        );
        assert_eq!(state.focus().effective_context(), "CommandPalette");
        assert_eq!(
            resolve(&mut state, PaletteKey::Toggle, &ctx),
            KeyOutcome::Ignored
        );

        let mut locked = PaletteState::new();
        locked.push_layer(FocusLayer::ModalDialog);
        assert!(!locked.open(&catalog), "模态锁定时命令面板不得打开");
        assert_eq!(locked.focus().effective_context(), "ModalDialog");
    }

    /// 上下键只在既有结果集上移动选中：不重算、不查缓存、不进入防抖。
    #[test]
    fn arrow_keys_move_selection_without_recomputing() {
        let (mut state, _catalog) = opened();
        let before = state.results().clone();
        let cache_len = state.cache().len();
        let hits = state.cache().hits();
        let misses = state.cache().misses();

        assert_eq!(state.move_selection(1), KeyOutcome::Moved);
        let first = before.visible_ids()[0].to_string();
        let second = before.visible_ids()[1].to_string();
        assert_eq!(state.active_id(), Some(second.as_str()));
        assert_eq!(state.move_selection(-1), KeyOutcome::Moved);
        assert_eq!(state.active_id(), Some(first.as_str()));

        assert_eq!(state.results(), &before, "移动选中不得改变结果集");
        assert_eq!(state.cache().len(), cache_len, "移动选中不得触发新查询");
        assert_eq!(state.cache().hits(), hits);
        assert_eq!(state.cache().misses(), misses);
        assert!(!state.is_pending());
        assert_eq!(state.query(), "", "移动选中不得动查询串");

        // 取模回绕：首行继续向上 → 末行。
        let last = before.visible_ids()[before.total_rows() - 1].to_string();
        assert!(state.select(&first));
        assert_eq!(state.move_selection(-1), KeyOutcome::Moved);
        assert_eq!(state.active_id(), Some(last.as_str()));

        // 空结果集时上下键被忽略。
        let mut empty = PaletteState::new();
        assert_eq!(empty.move_selection(1), KeyOutcome::Ignored);
    }

    /// `⇥ / ⇧⇥` 按分区跳转并回绕，同样不重算结果。
    #[test]
    fn section_jump_lands_on_adjacent_section_head() {
        let (mut state, _catalog) = opened();
        let before = state.results().clone();
        assert_eq!(state.active_section(), Some(SearchSection::Suggested));

        assert_eq!(state.jump_section(1), KeyOutcome::SectionJumped);
        assert_eq!(state.active_section(), Some(SearchSection::Sessions));
        assert_eq!(state.active_id(), Some("sess-1"));

        assert_eq!(state.jump_section(1), KeyOutcome::SectionJumped);
        assert_eq!(state.active_section(), Some(SearchSection::Files));
        assert_eq!(state.active_id(), Some("file-1"));

        // Commands 区尾行 → 回绕到 Suggested 首行。
        assert_eq!(state.jump_section(1), KeyOutcome::SectionJumped);
        assert_eq!(state.active_section(), Some(SearchSection::Commands));
        assert_eq!(state.jump_section(1), KeyOutcome::SectionJumped);
        assert_eq!(state.active_section(), Some(SearchSection::Suggested));

        // 反向回绕。
        assert_eq!(state.jump_section(-1), KeyOutcome::SectionJumped);
        assert_eq!(state.active_section(), Some(SearchSection::Commands));

        assert_eq!(state.results(), &before, "分区跳转不得重算结果");
        assert_eq!(state.jump_section(0), KeyOutcome::Ignored);
    }

    /// `↵` 执行选中行动作并关闭面板。
    #[test]
    fn confirm_returns_action_and_closes() {
        let (mut state, _catalog) = opened();
        assert_eq!(state.active_id(), state.results().first_id());
        let outcome = state.confirm();
        match outcome {
            KeyOutcome::Confirmed(action) => {
                assert!(matches!(action, SearchAction::RunCommand { .. }))
            }
            other => panic!("期望执行动作，实际 {other:?}"),
        }
        assert!(!state.is_open());
        assert_eq!(state.focus().effective_context(), "WorkbenchGlobal");
        assert_eq!(state.confirm(), KeyOutcome::Ignored, "已关闭时 ↵ 不再执行");
    }

    /// 防抖与缓存贯通：连续输入只在 90ms 到点后重算一次，二次相同查询直接命中。
    #[test]
    fn typing_is_debounced_and_cached() {
        let (mut state, catalog) = opened();
        let base_cache = state.cache().len();
        assert_eq!(state.set_query("设", 0), QueryOutcome::Pending);
        assert_eq!(state.set_query("设置", 40), QueryOutcome::Pending);
        assert!(!state.poll(100, &catalog), "40 + 90 未到，不得重算");
        assert!(state.poll(130, &catalog), "到点必须重算一次");
        assert_eq!(state.cache().len(), base_cache + 1);

        assert!(
            state
                .results()
                .sections()
                .iter()
                .any(|section| { section.section() == SearchSection::Commands })
        );

        // 选中态跨查询刷新按 ID 保位。
        let active = state.active_id().map(str::to_string);
        assert!(active.is_some());
        assert!(matches!(
            state.set_query("设置", 500),
            QueryOutcome::Ready(_)
        ));
        assert_eq!(state.active_id().map(str::to_string), active);
    }

    /// 缩小查询后再放宽，选中行消失时回落到新结果首行，而不是留空。
    #[test]
    fn selection_falls_back_to_first_row_when_it_disappears() {
        let (mut state, catalog) = opened();
        state.set_query("命令", 0);
        state.poll(DEBOUNCE_MS, &catalog);
        let narrowed = state.results().first_id().map(str::to_string);
        assert!(narrowed.is_some());
        state.apply_results(SearchResultSet::empty());
        assert_eq!(state.active_id(), None);
        state.clear_query(&catalog);
        assert_eq!(state.active_id(), state.results().first_id());
    }

    /// 预览入口用占位目录即可渲染（不依赖任何 I/O 或 GPU 上下文）。
    #[test]
    fn preview_entry_point_uses_pure_catalog() {
        let catalog = SearchCatalog::seeded();
        let state = PaletteState::new();
        assert!(!state.is_open());
        assert!(state.active_item().is_none());
        assert!(state.cache().is_empty());
        assert_eq!(catalog.len(), 9);
    }
}
