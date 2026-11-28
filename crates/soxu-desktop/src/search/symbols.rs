//! 代码符号补全入口（06 页 §二 Files 分区「工作区文件与代码符号」）。
//!
//! 阶段边界（Issue #140 第 3 条）：**数据源是调用方传入的内存符号目录**
//! （[`SymbolCatalog`]），本模块不直接调用 CodeGraph、不请求 daemon、不扫磁盘。
//! 符号目录由壳层在打开面板/输入 `@` 时从既有快照摊平后注入，从而保持
//! 「渲染线程零 I/O」铁律；CodeGraph 只在装配期（非渲染期）离线产出快照。
//!
//! 过滤语义（前缀优先）：
//! 1. 符号名前缀命中（含大小写不敏感）排在最前，同档按名称长度、再按目录声明序；
//! 2. 其次才是符号名其它位置的子串命中，保持目录声明序；
//! 3. 结果最多 [`SYMBOL_SUGGESTION_LIMIT`] 条，与检索每区 8 行同规格。
//!
//! 选中通过回调交付：`select_by_prefix(.., on_select)` 按当前排序依次回调，
//! 视图侧只需在回调里把符号插入输入舱，不必把回调闭包塞进数据结构。

use crate::search::model::{SearchAction, SearchItem, SearchSection};

/// 符号补全最多返回条数（与检索每区 8 行同规格）。
pub const SYMBOL_SUGGESTION_LIMIT: usize = 8;

/// 符号种类（Lucide 图标名与 Web 端同名同义）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SymbolKind {
    /// 函数。
    Function,
    /// 方法。
    Method,
    /// 结构体。
    Struct,
    /// 枚举。
    Enum,
    /// 特征 / 接口。
    Trait,
    /// 模块。
    Module,
    /// 常量。
    Constant,
    /// 宏。
    Macro,
}

impl SymbolKind {
    /// 全部种类。
    pub const ALL: [Self; 8] = [
        Self::Function,
        Self::Method,
        Self::Struct,
        Self::Enum,
        Self::Trait,
        Self::Module,
        Self::Constant,
        Self::Macro,
    ];

    /// Lucide 图标名。
    pub const fn icon(self) -> &'static str {
        match self {
            Self::Function | Self::Method => "square-function",
            Self::Struct => "box",
            Self::Enum => "list",
            Self::Trait => "layout-template",
            Self::Module => "package",
            Self::Constant => "hash",
            Self::Macro => "wand-sparkles",
        }
    }

    /// 符号种类徽标（等宽字，用于 `FILES` 行右侧）。
    pub const fn badge(self) -> &'static str {
        match self {
            Self::Function => "fn",
            Self::Method => "met",
            Self::Struct => "struct",
            Self::Enum => "enum",
            Self::Trait => "trait",
            Self::Module => "mod",
            Self::Constant => "const",
            Self::Macro => "macro",
        }
    }
}

/// 一条符号目录项（纯内存快照）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SymbolEntry {
    name: String,
    kind: SymbolKind,
    path: String,
    lower_name: String,
}

impl SymbolEntry {
    /// 构造一条符号项。
    pub fn new(name: impl Into<String>, kind: SymbolKind, path: impl Into<String>) -> Self {
        let name = name.into();
        let lower_name = name.to_lowercase();
        Self {
            name,
            kind,
            path: path.into(),
            lower_name,
        }
    }

    /// 符号名。
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 符号种类。
    pub fn kind(&self) -> SymbolKind {
        self.kind
    }

    /// 所在文件路径（工作区相对路径）。
    pub fn path(&self) -> &str {
        &self.path
    }

    /// 小写符号名（匹配用）。
    fn lower_name(&self) -> &str {
        &self.lower_name
    }

    /// 落到检索 `FILES` 分区的一条结果（标题=符号名，摘要=`path · kind`）。
    pub fn to_search_item(&self) -> SearchItem {
        SearchItem::new(
            format!("symbol:{}@{}", self.name, self.path),
            SearchSection::Files,
            self.name.clone(),
            SearchAction::InsertSymbol {
                name: self.name.clone(),
                path: self.path.clone(),
            },
        )
        .with_subtitle(format!("{} · {}", self.path, self.kind.badge()))
        .with_keywords(&[self.kind.badge()])
    }
}

/// 前缀命中的分档（小者更优）。
const TIER_PREFIX: u8 = 0;
const TIER_SUBSTRING: u8 = 1;

/// 内存符号目录。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SymbolCatalog {
    entries: Vec<SymbolEntry>,
}

impl SymbolCatalog {
    /// 从既有快照构造（调用方在装配期一次性摊平，渲染期只读）。
    pub fn new(entries: Vec<SymbolEntry>) -> Self {
        Self { entries }
    }

    /// 空目录。
    pub fn empty() -> Self {
        Self::default()
    }

    /// 全部符号项（目录声明序）。
    pub fn entries(&self) -> &[SymbolEntry] {
        &self.entries
    }

    /// 条目数。
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 追加一条符号。
    pub fn push(&mut self, entry: SymbolEntry) {
        self.entries.push(entry);
    }

    /// 按前缀（其次子串）过滤，返回最多 `limit` 条。
    ///
    /// 空前缀返回目录声明序的前 `limit` 条（刚输入 `@` 时的默认建议）。
    pub fn prefix_matches(&self, prefix: &str, limit: usize) -> Vec<&SymbolEntry> {
        let needle = prefix.trim().to_lowercase();
        if needle.is_empty() {
            return self.entries.iter().take(limit).collect();
        }

        let mut ranked: Vec<(u8, usize, usize, &SymbolEntry)> = Vec::new();
        for (index, entry) in self.entries.iter().enumerate() {
            let tier = if entry.lower_name().starts_with(&needle) {
                TIER_PREFIX
            } else if entry.lower_name().contains(&needle) {
                TIER_SUBSTRING
            } else {
                continue;
            };
            // 越短的名称越贴近用户想打的符号；长度相同再回落到声明序。
            ranked.push((tier, entry.name().len(), index, entry));
        }
        ranked.sort_by_key(|(tier, len, index, _)| (*tier, *len, *index));
        ranked
            .into_iter()
            .take(limit)
            .map(|(_, _, _, entry)| entry)
            .collect()
    }

    /// 按前缀取第一条（`↵` 直接补全的场景）。
    pub fn first_prefix_match(&self, prefix: &str) -> Option<&SymbolEntry> {
        self.prefix_matches(prefix, 1).into_iter().next()
    }

    /// 按前缀过滤并按排序依次回调选中项，返回回调次数。
    ///
    /// 视图侧把「插入输入舱」写进回调即可；不返回值持有，避免把闭包塞进状态。
    pub fn select_by_prefix(
        &self,
        prefix: &str,
        limit: usize,
        mut on_select: impl FnMut(&SymbolEntry),
    ) -> usize {
        let matches = self.prefix_matches(prefix, limit);
        for entry in &matches {
            on_select(entry);
        }
        matches.len()
    }

    /// 摊平成检索目录（`FILES` 分区），供命令面板五分区检索复用同一份符号快照。
    pub fn to_search_catalog(&self) -> crate::search::model::SearchCatalog {
        crate::search::model::SearchCatalog::new(
            self.entries
                .iter()
                .map(SymbolEntry::to_search_item)
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> SymbolCatalog {
        SymbolCatalog::new(vec![
            SymbolEntry::new("build_results", SymbolKind::Function, "src/search/rank.rs"),
            SymbolEntry::new("SearchCatalog", SymbolKind::Struct, "src/search/model.rs"),
            SymbolEntry::new("search_palette", SymbolKind::Function, "src/search/mod.rs"),
            SymbolEntry::new("results_now", SymbolKind::Method, "src/search/cache.rs"),
            SymbolEntry::new("SearchResultSet", SymbolKind::Struct, "src/search/rank.rs"),
        ])
    }

    /// 前缀命中排第一档，子串命中次之；同档更短名称更靠前。
    #[test]
    fn prefix_matches_rank_before_substring_hits() {
        let catalog = catalog();
        let hits: Vec<&str> = catalog
            .prefix_matches("search", SYMBOL_SUGGESTION_LIMIT)
            .into_iter()
            .map(SymbolEntry::name)
            .collect();
        // 「SearchCatalog」「SearchResultSet」「search_palette」是前缀命中，按长度排序；
        // 不含其它位置的子串命中。
        assert_eq!(hits.len(), 3);
        assert_eq!(hits[0], "SearchCatalog");
        assert_eq!(hits[1], "search_palette");
        assert_eq!(hits[2], "SearchResultSet");

        // 前缀命中在前（tier 0），子串命中在后（tier 1），同档按名称长度。
        let substring: Vec<&str> = catalog
            .prefix_matches("results", SYMBOL_SUGGESTION_LIMIT)
            .into_iter()
            .map(SymbolEntry::name)
            .collect();
        assert_eq!(
            substring,
            vec!["results_now", "build_results", "SearchResultSet"]
        );
    }

    /// 空前缀给默认建议；无命中给空集；上限生效。
    #[test]
    fn empty_prefix_and_limits_are_bounded() {
        let catalog = catalog();
        assert_eq!(catalog.prefix_matches("", 2).len(), 2);
        assert!(catalog.prefix_matches("绝对不存在", 8).is_empty());
        assert_eq!(
            catalog.first_prefix_match("search").map(SymbolEntry::name),
            Some("SearchCatalog")
        );
        assert_eq!(catalog.first_prefix_match("zzz"), None);
        assert!(SymbolCatalog::empty().is_empty());
        assert_eq!(SymbolCatalog::empty().len(), 0);
    }

    /// 选中回调按排序依次交付，且不改动目录。
    #[test]
    fn select_by_prefix_invokes_the_callback_in_rank_order() {
        let catalog = catalog();
        let mut selected: Vec<(String, SymbolKind, String)> = Vec::new();
        let count = catalog.select_by_prefix("build", 4, |entry| {
            selected.push((
                entry.name().to_string(),
                entry.kind(),
                entry.path().to_string(),
            ));
        });
        assert_eq!(count, 1);
        assert_eq!(
            selected,
            vec![(
                "build_results".to_string(),
                SymbolKind::Function,
                "src/search/rank.rs".to_string()
            )]
        );
        assert_eq!(catalog.len(), 5, "回调不得改动目录");
    }

    /// 符号可摊平成 FILES 分区结果：标题为符号名，摘要带路径与种类徽标。
    #[test]
    fn symbols_project_into_the_files_section() {
        let catalog = catalog();
        let search = catalog.to_search_catalog();
        assert_eq!(search.len(), 5);
        let first = &search.items()[0];
        assert_eq!(first.section(), SearchSection::Files);
        assert_eq!(first.title(), "build_results");
        assert_eq!(
            first.subtitle(),
            Some("src/search/rank.rs · fn"),
            "摘要必须同时给出路径与种类徽标"
        );
        assert!(matches!(
            first.action(),
            SearchAction::InsertSymbol { name, path }
                if name == "build_results" && path == "src/search/rank.rs"
        ));
    }
}
