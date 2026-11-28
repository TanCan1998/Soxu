//! 五分区检索的数据模型（`pages/06-modals-onboarding-and-automation.md` §二）。
//!
//! 设计要点：
//! - **五分区固定顺序**：`Suggested → Sessions → Files → History → Commands`，
//!   由 [`SearchSection::ALL`] 与 [`SearchSection::rank`] 双保险固化，排序时**先比分区**
//!   再比命中质量，分区之间绝不因打分交叉换位；
//! - **零 I/O**：目录（[`SearchCatalog`]）是调用方传入的纯内存快照，本模块不读文件
//!   系统、不查 SQLite、不起子进程，也不直接调用 CodeGraph/daemon；
//! - **匹配字段预小写**：`title` / `subtitle` / `keywords` 在构造时即落一份小写副本，
//!   每次按键只做 `&str` 比较与 `index_of`，不在渲染线程分配新字符串
//!   （对照 CodeG `file-search-match.ts` 让候选自带 `lowerName`/`lowerPath` 的做法）。
//!
//! 参考边界：Paseo `packages/app/src/command-center/{results,sections}.ts` 的
//! 「分区 band + 分区 rank 先于分数」总序，以及「可见字段优先于隐藏关键字」的
//! `fieldRank` 语义；CodeG `src/lib/file-search-match.ts` 的「层级（tier）压倒位置、
//! 位置压倒长度」评分。二者均为结构对照，未移植其宿主框架代码。

/// 分区数量（06 页 §二固定五段）。
pub const SECTION_COUNT: usize = 5;

/// 单分区最多渲染行数（06 页 §二「最多 8 行」）。
pub const MAX_ROWS_PER_SECTION: usize = 8;

/// 命令面板的五个固定分区。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SearchSection {
    /// 推荐动作（`Suggested`）。
    Suggested,
    /// 活跃工作区与会话（`Sessions`）。
    Sessions,
    /// 工作区文件与代码符号（`Files`）。
    Files,
    /// 原生 CLI 历史（`History`）。
    History,
    /// 设置与命令（`Commands`）。
    Commands,
}

impl SearchSection {
    /// 全部分区，顺序即 06 页 §二规定的展示顺序（禁止重排）。
    pub const ALL: [Self; SECTION_COUNT] = [
        Self::Suggested,
        Self::Sessions,
        Self::Files,
        Self::History,
        Self::Commands,
    ];

    /// 分区固定序位（`0..5`），排序第一关键字。
    pub const fn rank(self) -> usize {
        match self {
            Self::Suggested => 0,
            Self::Sessions => 1,
            Self::Files => 2,
            Self::History => 3,
            Self::Commands => 4,
        }
    }

    /// 分区头文案（设计稿 `26px` 大写标签，英文标识遵循 06 页括注）。
    pub const fn header(self) -> &'static str {
        match self {
            Self::Suggested => "SUGGESTED",
            Self::Sessions => "SESSIONS",
            Self::Files => "FILES",
            Self::History => "HISTORY",
            Self::Commands => "COMMANDS",
        }
    }

    /// 分区中文名（无障碍朗读与设置页引用）。
    pub const fn label(self) -> &'static str {
        match self {
            Self::Suggested => "推荐动作",
            Self::Sessions => "活跃工作区与会话",
            Self::Files => "工作区文件与代码符号",
            Self::History => "原生 CLI 历史",
            Self::Commands => "设置与命令",
        }
    }

    /// Lucide 图标名（kebab-case，与 Web 端同名同义）。
    pub const fn icon(self) -> &'static str {
        match self {
            Self::Suggested => "sparkles",
            Self::Sessions => "messages-square",
            Self::Files => "file-code",
            Self::History => "history",
            Self::Commands => "command",
        }
    }
}

/// 命中后要执行的语义动作。
///
/// 本阶段不持有任何回调闭包：动作是纯数据，由壳层键位分发在 `↵ 执行` 时消费，
/// 从而让排序、选中与执行三段都能在无 GPU、无 I/O 的单元测试里被断言。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SearchAction {
    /// 跳到某个会话/工作区。
    OpenSession {
        /// 会话标识。
        session_id: String,
    },
    /// 打开工作区文件。
    OpenFile {
        /// 工作区内相对路径。
        path: String,
    },
    /// 把代码符号插入输入舱（符号补全入口的落地形态）。
    InsertSymbol {
        /// 符号名。
        name: String,
        /// 符号所在文件路径。
        path: String,
    },
    /// 执行命令。
    RunCommand {
        /// 命令标识（如 `settings.open`）。
        command: String,
    },
}

/// 一条可检索结果。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchItem {
    /// 稳定标识：选中态跨重排按它保持，而不是按行下标（对照 Paseo
    /// `command-center/interaction.ts` 用 `activeResultId` 保位的做法）。
    id: String,
    /// 所属分区。
    section: SearchSection,
    /// 主标题。
    title: String,
    /// 可选摘要行（有摘要的增长行高，06 页 §二 `54px`）。
    subtitle: Option<String>,
    /// 隐藏关键字：参与匹配但不渲染（`设置` 类动作暴露「快捷键」等别名）。
    keywords: Vec<String>,
    /// 命中后的语义动作。
    action: SearchAction,
    /// `title` 的小写副本（构造期一次，渲染期零分配）。
    lower_title: String,
    /// `subtitle` 的小写副本。
    lower_subtitle: Option<String>,
}

impl SearchItem {
    /// 构造一条结果（仅标题）。
    pub fn new(
        id: impl Into<String>,
        section: SearchSection,
        title: impl Into<String>,
        action: SearchAction,
    ) -> Self {
        let title = title.into();
        let lower_title = title.to_lowercase();
        Self {
            id: id.into(),
            section,
            title,
            subtitle: None,
            keywords: Vec::new(),
            action,
            lower_title,
            lower_subtitle: None,
        }
    }

    /// 追加摘要行。
    pub fn with_subtitle(mut self, subtitle: impl Into<String>) -> Self {
        let subtitle = subtitle.into();
        self.lower_subtitle = Some(subtitle.to_lowercase());
        self.subtitle = Some(subtitle);
        self
    }

    /// 追加隐藏关键字（不渲染，参与匹配）。
    pub fn with_keywords(mut self, keywords: &[&str]) -> Self {
        self.keywords = keywords.iter().map(|k| k.to_lowercase()).collect();
        self
    }

    /// 稳定标识。
    pub fn id(&self) -> &str {
        &self.id
    }

    /// 所属分区。
    pub fn section(&self) -> SearchSection {
        self.section
    }

    /// 主标题。
    pub fn title(&self) -> &str {
        &self.title
    }

    /// 摘要行。
    pub fn subtitle(&self) -> Option<&str> {
        self.subtitle.as_deref()
    }

    /// 语义动作。
    pub fn action(&self) -> &SearchAction {
        &self.action
    }

    /// 小写标题（匹配用，调用方不得直接展示）。
    pub(crate) fn lower_title(&self) -> &str {
        &self.lower_title
    }

    /// 小写摘要（匹配用）。
    pub(crate) fn lower_subtitle(&self) -> Option<&str> {
        self.lower_subtitle.as_deref()
    }

    /// 是否带摘要行（决定行高 `54px` 或 `40px`）。
    pub fn has_subtitle(&self) -> bool {
        self.subtitle.is_some()
    }

    /// 隐藏关键字（已经小写，匹配用）。
    pub(crate) fn keywords(&self) -> &[String] {
        &self.keywords
    }
}

/// 纯内存检索目录：调用方把既有快照（会话列表、文件树、CLI 历史、命令注册表）
/// 摊平成 [`SearchItem`] 后交进来，之后每次按键都只在这一份快照上做纯计算。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchCatalog {
    items: Vec<SearchItem>,
}

impl SearchCatalog {
    /// 从既有结果集构造目录。
    pub fn new(items: Vec<SearchItem>) -> Self {
        Self { items }
    }

    /// 空目录。
    pub fn empty() -> Self {
        Self::default()
    }

    /// 追加一条结果。
    pub fn push(&mut self, item: SearchItem) {
        self.items.push(item);
    }

    /// 全部结果（目录声明顺序，用于空查询的稳定展示）。
    pub fn items(&self) -> &[SearchItem] {
        &self.items
    }

    /// 目录规模。
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// 目录是否为空。
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// 某分区的全部结果（保持目录声明顺序）。
    pub fn in_section(&self, section: SearchSection) -> impl Iterator<Item = &SearchItem> {
        self.items.iter().filter(move |i| i.section == section)
    }

    /// 零 I/O 的占位目录：仅含静态「设置与命令」与「推荐动作」两类入口。
    ///
    /// 真实会话/文件/历史目录由壳层在打开面板时注入；本函数让壳层在不接任何
    /// I/O 的前提下也能挂载出符合 06 页分区的面板骨架。
    pub fn seeded() -> Self {
        let mut catalog = Self::empty();
        for (id, title, subtitle) in [
            ("cmd.new-session", "新建会话", "在当前工作区开一个干净会话"),
            (
                "cmd.open-project",
                "打开或克隆项目",
                "三合一：打开目录 / 克隆 / 初始化",
            ),
            ("cmd.settings", "打开设置", "7 分区设置弹窗"),
            ("cmd.toggle-theme", "切换明暗主题", "跟随系统或强制明暗"),
            (
                "cmd.worktree-isolate",
                "新建隔离 Worktree",
                "并行开发互不干扰",
            ),
            ("cmd.check-all", "运行全仓门禁", "scripts/check-all.ps1"),
        ] {
            catalog.push(
                SearchItem::new(
                    id,
                    SearchSection::Commands,
                    title,
                    SearchAction::RunCommand {
                        command: id.to_string(),
                    },
                )
                .with_subtitle(subtitle)
                .with_keywords(&["设置", "命令", "command"]),
            );
        }
        for (id, title, subtitle) in [
            ("sug.review-diff", "审阅当前变更", "右栏 Diff 与行号槽"),
            ("sug.pair-mobile", "手机扫码配对", "局域网直连或加密中继"),
            (
                "sug.schedules",
                "打开自动化调度中心",
                "Schedules 与 Heartbeats",
            ),
        ] {
            catalog.push(
                SearchItem::new(
                    id,
                    SearchSection::Suggested,
                    title,
                    SearchAction::RunCommand {
                        command: id.to_string(),
                    },
                )
                .with_subtitle(subtitle),
            );
        }
        catalog
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 五分区顺序与 06 页 §二字面一致，且序位单调。
    #[test]
    fn sections_keep_the_fixed_spec_order() {
        assert_eq!(SearchSection::ALL.len(), SECTION_COUNT);
        let headers: Vec<&str> = SearchSection::ALL.iter().map(|s| s.header()).collect();
        assert_eq!(
            headers,
            vec!["SUGGESTED", "SESSIONS", "FILES", "HISTORY", "COMMANDS"]
        );
        for (index, section) in SearchSection::ALL.iter().enumerate() {
            assert_eq!(section.rank(), index, "分区序位必须等于其在 ALL 中的下标");
            assert!(!section.label().is_empty());
            assert!(!section.icon().is_empty());
        }
    }

    /// 构造期即落小写副本：中文与大小写混排都不影响匹配输入。
    #[test]
    fn items_precompute_lowercase_fields() {
        let item = SearchItem::new(
            "f1",
            SearchSection::Files,
            "CommandPalette.rs",
            SearchAction::OpenFile {
                path: "crates/soxu-desktop/src/search/mod.rs".to_string(),
            },
        )
        .with_subtitle("Search / 面板")
        .with_keywords(&["Palette", "搜索"]);

        assert_eq!(item.lower_title(), "commandpalette.rs");
        assert_eq!(item.lower_subtitle(), Some("search / 面板"));
        assert_eq!(item.keywords(), ["palette".to_string(), "搜索".to_string()]);
        assert!(item.has_subtitle());
        assert_eq!(item.subtitle(), Some("Search / 面板"));
        assert_eq!(item.section(), SearchSection::Files);
        assert_eq!(item.id(), "f1");
    }

    /// 占位目录只覆盖推荐动作与命令两区，且全部是零 I/O 静态项。
    #[test]
    fn seeded_catalog_is_pure_memory_and_grouped() {
        let catalog = SearchCatalog::seeded();
        assert!(!catalog.is_empty());
        assert_eq!(catalog.len(), catalog.items().len());
        assert!(catalog.in_section(SearchSection::Sessions).next().is_none());
        assert!(catalog.in_section(SearchSection::Files).next().is_none());
        assert_eq!(catalog.in_section(SearchSection::Suggested).count(), 3);
        assert_eq!(catalog.in_section(SearchSection::Commands).count(), 6);
        assert!(
            catalog
                .in_section(SearchSection::Commands)
                .all(|i| i.has_subtitle())
        );
        assert!(SearchCatalog::empty().is_empty());
    }
}
