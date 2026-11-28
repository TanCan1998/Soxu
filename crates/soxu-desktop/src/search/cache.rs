//! `90ms` 防抖窗口与 `24` 条 LRU 查询缓存（06 页 §二搜索输入区）。
//!
//! 两个约束都落成纯逻辑，时间由调用方以毫秒整数注入（[`DebouncedSearch::on_input`]
//! 的 `now_ms`），因此单测不需要睡觉、不需要 GPUI 定时器，也不会碰任何 I/O。
//!
//! 语义：
//! - **防抖**：每次按键都把窗口重置为 `now + 90ms`，只有 [`DebouncedSearch::poll`]
//!   观察到「当前时刻已到截止时刻」才真正重算；连续敲键期间零重算。
//! - **LRU**：缓存键是原始查询串（未 trim，保留用户输入形态），容量 24；命中即
//!   前移，超容淘汰最久未用的一条，对应设计稿「24 条 LRU 缓存」。
//! - **打开即算**：面板刚打开（空查询）走 [`DebouncedSearch::results_now`]，
//!   不进防抖窗口，避免首帧空列表闪烁（对照 Paseo command-center 的
//!   「打开即定位首行」）。

use crate::search::model::SearchCatalog;
use crate::search::rank::{SearchResultSet, build_results};

/// 防抖窗口（06 页 §二：`90ms`）。
pub const DEBOUNCE_MS: u64 = 90;

/// 查询缓存容量（06 页 §二：`24` 条 LRU）。
pub const QUERY_CACHE_CAPACITY: usize = 24;

/// 一定量查询缓存（LRU）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryCache {
    capacity: usize,
    /// 最近使用在前。
    entries: Vec<(String, SearchResultSet)>,
    hits: u64,
    misses: u64,
}

impl QueryCache {
    /// 指定容量构造。
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            entries: Vec::new(),
            hits: 0,
            misses: 0,
        }
    }

    /// 当前条目数。
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 容量。
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// 命中次数。
    pub fn hits(&self) -> u64 {
        self.hits
    }

    /// 未命中次数。
    pub fn misses(&self) -> u64 {
        self.misses
    }

    /// 只读判定是否在缓存内（不改变 LRU 顺序、不计命中）。
    pub fn contains(&self, query: &str) -> bool {
        self.entries.iter().any(|(key, _)| key == query)
    }

    /// 取缓存并把该键前移（真 LRU 语义）。
    pub fn get(&mut self, query: &str) -> Option<&SearchResultSet> {
        let Some(position) = self.entries.iter().position(|(key, _)| key == query) else {
            self.misses += 1;
            return None;
        };
        self.hits += 1;
        if position != 0 {
            let entry = self.entries.remove(position);
            self.entries.insert(0, entry);
        }
        self.entries.first().map(|(_, results)| results)
    }

    /// 写入缓存：同键替换并前移，超容淘汰队尾。
    pub fn insert(&mut self, query: impl Into<String>, results: SearchResultSet) {
        let query = query.into();
        self.entries.retain(|(key, _)| key != &query);
        self.entries.insert(0, (query, results));
        self.entries.truncate(self.capacity);
    }

    /// 清空（关闭面板时保留缓存，仅在数据源换代时由调用方显式清理）。
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

impl Default for QueryCache {
    fn default() -> Self {
        Self::new(QUERY_CACHE_CAPACITY)
    }
}

/// 一次输入的即时裁决。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueryOutcome {
    /// 命中缓存：直接给结果，零重算。
    Ready(SearchResultSet),
    /// 已进入防抖窗口，等 [`DebouncedSearch::poll`]。
    Pending,
}

/// 待重算的查询。
#[derive(Clone, Debug, PartialEq, Eq)]
struct Pending {
    query: String,
    deadline_ms: u64,
}

/// 防抖调度器 + LRU 缓存的组合体（面板持有它即可）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DebouncedSearch {
    cache: QueryCache,
    pending: Option<Pending>,
    window_ms: u64,
}

impl DebouncedSearch {
    /// 使用 06 页默认 `90ms` / `24` 条规格构造。
    pub fn new() -> Self {
        Self::with_spec(DEBOUNCE_MS, QUERY_CACHE_CAPACITY)
    }

    /// 自定义规格构造（测试窗口边界的对照组）。
    pub fn with_spec(window_ms: u64, capacity: usize) -> Self {
        Self {
            cache: QueryCache::new(capacity),
            pending: None,
            window_ms,
        }
    }

    /// 防抖窗口（毫秒）。
    pub fn window_ms(&self) -> u64 {
        self.window_ms
    }

    /// 缓存只读视图。
    pub fn cache(&self) -> &QueryCache {
        &self.cache
    }

    /// 是否还有待重算的查询。
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// 待重算的查询串。
    pub fn pending_query(&self) -> Option<&str> {
        self.pending.as_ref().map(|p| p.query.as_str())
    }

    /// 距截止时刻还剩多少毫秒（已到期为 `0`）。
    pub fn remaining_ms(&self, now_ms: u64) -> u64 {
        self.pending
            .as_ref()
            .map_or(0, |p| p.deadline_ms.saturating_sub(now_ms))
    }

    /// 记录一次按键：缓存命中立即返回，否则重置 90ms 窗口。
    pub fn on_input(&mut self, query: impl Into<String>, now_ms: u64) -> QueryOutcome {
        let query = query.into();
        if self.cache.contains(&query) {
            self.pending = None;
            if let Some(results) = self.cache.get(&query) {
                return QueryOutcome::Ready(results.clone());
            }
        }
        self.pending = Some(Pending {
            query,
            deadline_ms: now_ms.saturating_add(self.window_ms),
        });
        QueryOutcome::Pending
    }

    /// 到点则重算一次并写入缓存（未到点返回 `None`）。
    pub fn poll(&mut self, now_ms: u64, catalog: &SearchCatalog) -> Option<SearchResultSet> {
        let pending = self.pending.as_ref()?;
        if now_ms < pending.deadline_ms {
            return None;
        }
        let pending = self.pending.take()?;
        Some(self.compute(&pending.query, catalog))
    }

    /// 立即求值（`↵` 执行、打开面板、或放弃防抖时使用），结果照常进缓存。
    pub fn results_now(&mut self, query: &str, catalog: &SearchCatalog) -> SearchResultSet {
        self.pending = None;
        self.compute(query, catalog)
    }

    /// 命中缓存则复用，否则重算并写入。
    fn compute(&mut self, query: &str, catalog: &SearchCatalog) -> SearchResultSet {
        if let Some(results) = self.cache.get(query) {
            return results.clone();
        }
        let results = build_results(query, catalog);
        self.cache.insert(query.to_string(), results.clone());
        results
    }
}

impl Default for DebouncedSearch {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> SearchCatalog {
        let mut catalog = SearchCatalog::seeded();
        catalog.push(crate::search::model::SearchItem::new(
            "f1",
            crate::search::model::SearchSection::Files,
            "search/mod.rs",
            crate::search::model::SearchAction::OpenFile {
                path: "crates/soxu-desktop/src/search/mod.rs".to_string(),
            },
        ));
        catalog
    }

    /// 90ms 窗口：窗口内每次按键都顺延，到点前不重算、到点才重算一次。
    #[test]
    fn debounce_window_slides_and_fires_once() {
        let catalog = catalog();
        let mut search = DebouncedSearch::new();
        assert_eq!(search.window_ms(), DEBOUNCE_MS);

        assert_eq!(search.on_input("s", 0), QueryOutcome::Pending);
        assert_eq!(search.on_input("se", 40), QueryOutcome::Pending);
        assert_eq!(search.on_input("sea", 80), QueryOutcome::Pending);
        assert_eq!(search.pending_query(), Some("sea"));
        assert_eq!(search.remaining_ms(80), 90);

        assert_eq!(search.poll(169, &catalog), None, "窗口未到不得重算");
        let results = search.poll(170, &catalog);
        assert!(results.is_some(), "80 + 90 到点必须重算");
        assert!(!search.is_pending());
        assert_eq!(search.poll(400, &catalog), None, "重算后无待办");

        // 覆盖率：中间态查询从未被计算，只有最后一次进缓存。
        assert_eq!(search.cache().len(), 1);
        assert!(search.cache().contains("sea"));
        assert!(!search.cache().contains("s"));
    }

    /// 缓存命中即时返回，且不再进入防抖窗口。
    #[test]
    fn cache_hit_skips_the_debounce_window() {
        let catalog = catalog();
        let mut search = DebouncedSearch::new();
        assert_eq!(search.on_input("setting", 0), QueryOutcome::Pending);
        assert!(search.poll(DEBOUNCE_MS, &catalog).is_some());
        assert_eq!(search.cache().hits(), 0);

        let outcome = search.on_input("setting", 500);
        assert!(matches!(outcome, QueryOutcome::Ready(_)));
        assert!(!search.is_pending());
        assert_eq!(search.cache().hits(), 1);
    }

    /// 24 条 LRU：第 25 条淘汰最久未用的，命中会把该条前移。
    #[test]
    fn lru_evicts_the_least_recently_used_entry() {
        let mut cache = QueryCache::new(QUERY_CACHE_CAPACITY);
        for index in 0..QUERY_CACHE_CAPACITY {
            cache.insert(format!("q{index}"), SearchResultSet::empty());
        }
        assert_eq!(cache.len(), QUERY_CACHE_CAPACITY);
        assert!(cache.contains("q0"), "最早的条目此时仍应存在");

        // 命中 q0 → 前移到队首。
        assert!(cache.get("q0").is_some());
        assert_eq!(cache.hits(), 1);

        cache.insert("q24".to_string(), SearchResultSet::empty());
        assert_eq!(cache.len(), QUERY_CACHE_CAPACITY);
        assert!(cache.contains("q0"), "刚被命中的条目不得被淘汰");
        assert!(cache.contains("q24"));
        assert!(!cache.contains("q1"), "最久未用的 q1 应被淘汰");

        // 未命中计入 misses。
        assert!(cache.get("q1").is_none());
        assert_eq!(cache.misses(), 1);
    }

    /// 立即求值绕过防抖但照常写缓存，重复调用不再重算。
    #[test]
    fn immediate_evaluation_bypasses_the_window_and_caches() {
        let catalog = catalog();
        let mut search = DebouncedSearch::new();
        let first = search.results_now("", &catalog);
        assert!(!first.is_empty());
        assert!(!search.is_pending());
        let again = search.results_now("", &catalog);
        assert_eq!(first, again);
        assert_eq!(search.cache().len(), 1);
        assert_eq!(search.cache().hits(), 1);
    }
}
