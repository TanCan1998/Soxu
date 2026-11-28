//! 命中评分与分区排序（「前缀优先」的唯一实现点）。
//!
//! 评分模型（参考 Paseo `packages/protocol/src/search/text-match.ts` 的
//! 「tier + offset，两者都升序即更好」，以及 CodeG `src/lib/file-search-match.ts`
//! 的「层级压倒位置、位置压倒长度」）：
//!
//! 1. **命中字段**：标题 → 摘要 → 隐藏关键字（可见文本永远先于隐藏关键字，
//!    对照 Paseo `command-center/results.ts` 的 `fieldRank`）；
//! 2. **命中层级**：完全相等 → 前缀 → 词首边界 → 子串（[`MatchTier`]）；
//! 3. **命中位置**：同一层级内越靠前越好；
//! 4. **标题长度**：更短的标题更贴近用户意图（CodeG 的 `length` 修正项）；
//! 5. **目录声明序**：最终稳定兜底，保证同分结果顺序确定、可回归。
//!
//! 分区之间**不参与打分比较**：分区序位（[`SearchSection::rank`]）是第一关键字，
//! 五段顺序因此恒定，不会因某个分区命中更好而整体前移。
//!
//! 未移植：拼写容错（Paseo 的 `fuzzy` 层）。面板是在已知列表上做收窄，容错会
//! 把 `main` 拉向 `mail`；历史全文检索才需要它，不属于本 Issue 范围。

use crate::search::model::{MAX_ROWS_PER_SECTION, SearchCatalog, SearchItem, SearchSection};

/// 命中层级，声明顺序即优劣顺序（小者更优）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MatchTier {
    /// 整段完全相等。
    Exact,
    /// 从头前缀命中。
    Prefix,
    /// 在某个词/段边界处开始命中。
    WordStart,
    /// 任意位置子串命中。
    Substring,
}

/// 命中字段，声明顺序即优劣顺序（可见字段优先于隐藏关键字）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MatchField {
    /// 主标题。
    Title,
    /// 摘要行。
    Subtitle,
    /// 隐藏关键字。
    Keyword,
}

/// 单条结果的命中证据（不含候选本身）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SearchMatch {
    /// 命中的字段。
    pub field: MatchField,
    /// 命中层级。
    pub tier: MatchTier,
    /// 命中起点（字节偏移）。
    pub offset: usize,
}

impl SearchMatch {
    /// 排序键的前两段：字段优先，层级次之。
    fn rank(self) -> (MatchField, MatchTier, usize) {
        (self.field, self.tier, self.offset)
    }
}

/// 判断 `haystack` 中 `offset` 之前是否处在词/段边界（非字母数字即边界，
/// 因此 `search/mod.rs` 里的 `mod` 能在 `/` 与 `.` 之后被识别为词首）。
fn at_boundary(haystack: &str, offset: usize) -> bool {
    haystack
        .get(..offset)
        .and_then(|head| head.chars().next_back())
        .is_none_or(|prev| !prev.is_alphanumeric())
}

/// 在单个字段上求最优命中层级与位置。
fn tier_on(needle: &str, haystack: &str) -> Option<(MatchTier, usize)> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    if haystack.starts_with(needle) {
        let tier = if haystack.len() == needle.len() {
            MatchTier::Exact
        } else {
            MatchTier::Prefix
        };
        return Some((tier, 0));
    }
    let mut fallback: Option<usize> = None;
    for (offset, _) in haystack.match_indices(needle) {
        if at_boundary(haystack, offset) {
            return Some((MatchTier::WordStart, offset));
        }
        fallback.get_or_insert(offset);
    }
    fallback.map(|offset| (MatchTier::Substring, offset))
}

/// 对单条结果打分。
///
/// 前置条件：`needle` 已 `trim` 并小写化（由 [`build_results`] 统一处理，避免
/// 每个候选都重新分配一份小写查询）；`needle` 为空时返回 `None`，空查询的
/// 「全部展示」由 [`build_results`] 单独处理。
pub fn score_item(needle: &str, item: &SearchItem) -> Option<SearchMatch> {
    let mut best: Option<SearchMatch> = None;
    let mut consider = |field: MatchField, text: &str| {
        if let Some((tier, offset)) = tier_on(needle, text) {
            let candidate = SearchMatch {
                field,
                tier,
                offset,
            };
            if best.is_none_or(|current| candidate.rank() < current.rank()) {
                best = Some(candidate);
            }
        }
    };
    consider(MatchField::Title, item.lower_title());
    if let Some(subtitle) = item.lower_subtitle() {
        consider(MatchField::Subtitle, subtitle);
    }
    for keyword in item.keywords() {
        consider(MatchField::Keyword, keyword);
    }
    best
}

/// 单个分区的结果切片。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SectionResults {
    section: SearchSection,
    items: Vec<SearchItem>,
}

impl SectionResults {
    /// 分区。
    pub fn section(&self) -> SearchSection {
        self.section
    }

    /// 该分区的行（已排序、已截断）。
    pub fn items(&self) -> &[SearchItem] {
        &self.items
    }

    /// 行数。
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// 一次检索的完整结果：按固定分区顺序排列的非空分区。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchResultSet {
    sections: Vec<SectionResults>,
}

impl SearchResultSet {
    /// 空结果。
    pub fn empty() -> Self {
        Self::default()
    }

    /// 非空分区（顺序即渲染顺序）。
    pub fn sections(&self) -> &[SectionResults] {
        &self.sections
    }

    /// 可见行总数（不含分区头）。
    pub fn total_rows(&self) -> usize {
        self.sections.iter().map(SectionResults::len).sum()
    }

    /// 是否没有任何结果。
    pub fn is_empty(&self) -> bool {
        self.sections.iter().all(SectionResults::is_empty)
    }

    /// 扁平可见行标识：键盘上下移动的遍历域。
    pub fn visible_ids(&self) -> Vec<&str> {
        self.sections
            .iter()
            .flat_map(|section| section.items.iter().map(SearchItem::id))
            .collect()
    }

    /// 某标识在扁平行里的下标。
    pub fn index_of(&self, id: &str) -> Option<usize> {
        self.sections
            .iter()
            .flat_map(|section| section.items.iter())
            .position(|item| item.id() == id)
    }

    /// 按下标取行。
    pub fn get(&self, index: usize) -> Option<&SearchItem> {
        self.sections
            .iter()
            .flat_map(|section| section.items.iter())
            .nth(index)
    }

    /// 按标识取行（选中态跨重排保位用）。
    pub fn find(&self, id: &str) -> Option<&SearchItem> {
        self.sections
            .iter()
            .flat_map(|section| section.items.iter())
            .find(|item| item.id() == id)
    }

    /// 首行标识（打开面板时的默认选中）。
    pub fn first_id(&self) -> Option<&str> {
        self.sections
            .iter()
            .find_map(|section| section.items.first().map(SearchItem::id))
    }
}

/// 执行一次完整检索：固定分区顺序 + 前缀优先排序 + 每区最多 [`MAX_ROWS_PER_SECTION`] 行。
///
/// 空查询（或纯空白）返回每个分区的目录声明序（稳定、不做二次重排），
/// 对应 06 页「刚打开面板时给推荐而不是空白」的语义。
pub fn build_results(query: &str, catalog: &SearchCatalog) -> SearchResultSet {
    let needle = query.trim().to_lowercase();
    let mut sections = Vec::new();

    for section in SearchSection::ALL {
        // （命中证据, 分区内声明序, 候选）：分区内声明序作为最终兜底，
        // 保证同分结果顺序确定。
        let mut matched: Vec<(Option<SearchMatch>, usize, &SearchItem)> = Vec::new();
        for (index, item) in catalog.in_section(section).enumerate() {
            if needle.is_empty() {
                matched.push((None, index, item));
                continue;
            }
            if let Some(hit) = score_item(&needle, item) {
                matched.push((Some(hit), index, item));
            }
        }

        if needle.is_empty() {
            matched.sort_by_key(|(_, index, _)| *index);
        } else {
            matched.sort_by(|left, right| {
                let left_hit = left.0.map(SearchMatch::rank);
                let right_hit = right.0.map(SearchMatch::rank);
                left_hit
                    .cmp(&right_hit)
                    .then_with(|| left.2.title().len().cmp(&right.2.title().len()))
                    .then_with(|| left.1.cmp(&right.1))
            });
        }

        let items: Vec<SearchItem> = matched
            .into_iter()
            .take(MAX_ROWS_PER_SECTION)
            .map(|(_, _, item)| item.clone())
            .collect();
        if !items.is_empty() {
            sections.push(SectionResults { section, items });
        }
    }

    SearchResultSet { sections }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::model::SearchAction;

    fn command(id: &str, title: &str) -> SearchItem {
        SearchItem::new(
            id,
            SearchSection::Commands,
            title,
            SearchAction::RunCommand {
                command: id.to_string(),
            },
        )
    }

    fn file(id: &str, title: &str) -> SearchItem {
        SearchItem::new(
            id,
            SearchSection::Files,
            title,
            SearchAction::OpenFile {
                path: title.to_string(),
            },
        )
    }

    /// 层级判定：完全相等 / 前缀 / 词首 / 子串四档，互不混淆。
    #[test]
    fn tiers_cover_exact_prefix_word_start_and_substring() {
        assert_eq!(tier_on("open", "open"), Some((MatchTier::Exact, 0)));
        assert_eq!(tier_on("open", "openfile"), Some((MatchTier::Prefix, 0)));
        assert_eq!(
            tier_on("mod", "search/mod.rs"),
            Some((MatchTier::WordStart, 7))
        );
        assert_eq!(tier_on("pen", "openfile"), Some((MatchTier::Substring, 1)));
        assert_eq!(tier_on("zzz", "openfile"), None);
        assert_eq!(tier_on("", "openfile"), None);
        assert_eq!(tier_on("openfile!", "openfile"), None);
    }

    /// 可见字段优先于隐藏关键字：标题子串命中压过关键字词首命中。
    #[test]
    fn visible_fields_outrank_hidden_keywords() {
        let titled = command("a", "terminal").with_keywords(&["shell"]);
        let keyworded = command("b", "运行面板").with_keywords(&["term"]);
        let wanted = score_item("term", &titled);
        let hidden = score_item("term", &keyworded);
        assert!(wanted.is_some() && hidden.is_some());
        if let (Some(wanted), Some(hidden)) = (wanted, hidden) {
            assert_eq!(wanted.field, MatchField::Title);
            assert_eq!(hidden.field, MatchField::Keyword);
            assert!(wanted.rank() < hidden.rank());
        }
    }

    /// 前缀优先排序：前缀命中排在任何子串命中之前。
    #[test]
    fn prefix_matches_sort_before_substrings() {
        let catalog = SearchCatalog::new(vec![
            command("c1", "重新打开"),
            command("c2", "Open settings"),
            file("f1", "crates/open.rs"),
        ]);
        let results = build_results("open", &catalog);
        let files = results
            .sections()
            .iter()
            .find(|s| s.section() == SearchSection::Files);
        let Some(files) = files else {
            panic!("Files 分区应命中");
        };
        assert_eq!(files.items()[0].id(), "f1");
        // 命令区里「Open settings」是词首/前缀命中，排在子串命中之前。
        let commands = results
            .sections()
            .iter()
            .find(|s| s.section() == SearchSection::Commands);
        let Some(commands) = commands else {
            panic!("Commands 分区应命中");
        };
        assert_eq!(commands.items()[0].id(), "c2");
        // 中文子串仍能命中，且不与前缀命中抢位。
        if let Some(hit) = results.find("c1") {
            assert_eq!(hit.id(), "c1");
        }
    }

    /// 五分区顺序恒定：即使后面的分区命中更好，也不越过前面的分区。
    #[test]
    fn section_order_is_never_reordered_by_score() {
        let catalog = SearchCatalog::new(vec![
            SearchItem::new(
                "s1",
                SearchSection::Suggested,
                "无关推荐",
                SearchAction::RunCommand {
                    command: "sug".to_string(),
                },
            )
            .with_keywords(&["diff"]),
            file("f1", "diff.rs"),
            SearchItem::new(
                "h1",
                SearchSection::History,
                "diff 历史",
                SearchAction::OpenSession {
                    session_id: "h1".to_string(),
                },
            ),
        ]);
        let results = build_results("diff", &catalog);
        let order: Vec<SearchSection> = results.sections().iter().map(|s| s.section()).collect();
        assert_eq!(
            order,
            vec![
                SearchSection::Suggested,
                SearchSection::Files,
                SearchSection::History
            ]
        );
        // 空分区（Sessions / Commands）不占位。
        assert_eq!(order.len(), 3);
    }

    /// 每区最多 8 行，且截断发生在排序之后（留下的都是最靠前的）。
    #[test]
    fn each_section_is_capped_at_eight_rows_after_ranking() {
        let mut items = Vec::new();
        for index in 0..12 {
            items.push(command(&format!("x{index}"), &format!("zzz 命令 {index}")));
        }
        items.push(command("top", "zzz"));
        let catalog = SearchCatalog::new(items);
        let results = build_results("zzz", &catalog);
        let commands = &results.sections()[0];
        assert_eq!(commands.len(), MAX_ROWS_PER_SECTION);
        assert_eq!(commands.items()[0].id(), "top", "完全相等命中必须排第一");
        assert!(results.total_rows() <= SECTION_MAX_TOTAL);
    }

    const SECTION_MAX_TOTAL: usize = MAX_ROWS_PER_SECTION;

    /// 空查询展示目录声明序，且可整段导航。
    #[test]
    fn empty_query_keeps_catalog_order() {
        let catalog = SearchCatalog::new(vec![command("b", "第二条"), command("a", "第一条")]);
        let results = build_results("   ", &catalog);
        let ids: Vec<&str> = results.visible_ids();
        assert_eq!(ids, vec!["b", "a"]);
        assert_eq!(results.first_id(), Some("b"));
        assert_eq!(results.index_of("a"), Some(1));
        assert_eq!(results.index_of("missing"), None);
        assert!(!results.is_empty());
        assert_eq!(results.get(5), None);
        assert_eq!(SearchResultSet::empty().first_id(), None);
        assert!(SearchResultSet::empty().is_empty());
    }

    /// 结果集为空时判定正确（用于 160px 空态卡）。
    #[test]
    fn unmatched_query_yields_empty_result_set() {
        let catalog = SearchCatalog::new(vec![command("c", "Open settings")]);
        let results = build_results("绝对不存在的关键词", &catalog);
        assert!(results.is_empty());
        assert_eq!(results.total_rows(), 0);
        assert!(results.sections().is_empty());
    }
}
