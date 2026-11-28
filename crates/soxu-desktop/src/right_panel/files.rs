//! `Files` 页签文件清单与词法路径归一化（`pages/01-desktop-workbench.md` §4 `Files`）。
//!
//! 本阶段是**纯内存快照模型**：目录遍历仍在后台线程（M6 接线），渲染帧只读已排序的
//! 路径快照，`render` 内零文件系统调用（§4「零阻塞工作区文件树」+ §4.1 渲染零阻塞）。
//!
//! 路径归一化严守**纯词法**纪律（借鉴 Padu `apps/desktop/src/app/right_panel/links.rs`
//! 的路由思路，按本仓令牌与边界重写）：不探测文件系统、不解析符号链接，只做字符串级
//! 归一化与越界拒绝，避免把远程/未挂载工作区的路径误解释成本机路径。

/// 文件清单一次性渲染上限（超出由后续虚拟列表接管，不在本阶段深挖）。
pub const MAX_LISTED_FILES: usize = 2000;

/// 工作区文件路径快照（升序、去重、统一正斜杠）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileIndex {
    paths: Vec<String>,
}

impl FileIndex {
    /// 由原始路径列表构建：词法归一化 → 去重 → 升序。
    pub fn new<I, S>(paths: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut normalized: Vec<String> = paths
            .into_iter()
            .filter_map(|raw| normalize_link_path(raw.as_ref()))
            .collect();
        normalized.sort_unstable();
        normalized.dedup();
        Self { paths: normalized }
    }

    /// 快照中的全部路径（已归一化排序）。
    pub fn paths(&self) -> &[String] {
        &self.paths
    }

    /// 路径总数。
    pub fn len(&self) -> usize {
        self.paths.len()
    }

    /// 是否为空快照（对应 `Files` 空态）。
    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }

    /// `/` 过滤：大小写不敏感的子串匹配，保持快照升序。
    ///
    /// 空查询返回全量；结果上限 [`MAX_LISTED_FILES`]。
    pub fn filter(&self, query: &str) -> Vec<&str> {
        let needle = query.trim().to_lowercase();
        let matched = self
            .paths
            .iter()
            .filter(|path| needle.is_empty() || path.to_lowercase().contains(needle.as_str()));
        matched.take(MAX_LISTED_FILES).map(String::as_str).collect()
    }

    /// 过滤结果是否为空（非空快照 + 无命中 → 过滤无结果态）。
    pub fn is_filtered_empty(&self, query: &str) -> bool {
        !self.is_empty() && self.filter(query).is_empty()
    }
}

/// 去掉 `:行[:列]` 位置后缀：`src/lib.rs:42` → `src/lib.rs`。
///
/// 只在后缀整体为十进制数字时剥离，避免误伤形如 `a:1b` 的合法文件名。
pub fn strip_location_suffix(raw: &str) -> &str {
    let Some((head, tail)) = raw.rsplit_once(':') else {
        return raw;
    };
    // 先尝试 `:line:col`，再退化到 `:line`。
    if let Some((head2, tail2)) = head.rsplit_once(':')
        && is_decimal(tail2)
        && is_decimal(tail)
    {
        return head2;
    }
    if is_decimal(tail) { head } else { raw }
}

/// 是否全部为 ASCII 十进制数字（且非空）。
fn is_decimal(raw: &str) -> bool {
    !raw.is_empty() && raw.bytes().all(|byte| byte.is_ascii_digit())
}

/// 词法归一化工作区相对路径；越界逃逸或非法输入返回 `None`。
///
/// 处理顺序：
/// 1. 去空白与 `file://` 前缀；
/// 2. 剥离 `:行[:列]` 位置后缀；
/// 3. 反斜杠统一为正斜杠；
/// 4. 处理 `.` 段，`..` 段仅允许在仍有父段可回退时生效；
///    **任何试图越过工作区根的 `..` 一律拒绝**（返回 `None`）。
pub fn normalize_link_path(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let without_scheme = trimmed.strip_prefix("file://").unwrap_or(trimmed);
    let without_location = strip_location_suffix(without_scheme);
    let unified = without_location.replace('\\', "/");

    let mut segments: Vec<&str> = Vec::new();
    for segment in unified.split('/') {
        match segment {
            "" | "." => continue,
            ".." => {
                // 没有可回退的父段 → 越过工作区根，拒绝。
                segments.pop()?;
            }
            other => segments.push(other),
        }
    }
    if segments.is_empty() {
        return None;
    }
    Some(segments.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 位置后缀剥离：单行号、行:列、以及不该剥离的普通文件名。
    #[test]
    fn strips_location_suffix_only_when_numeric() {
        assert_eq!(
            strip_location_suffix("crates/soxu-core/src/lib.rs:42"),
            "crates/soxu-core/src/lib.rs"
        );
        assert_eq!(
            strip_location_suffix("crates/soxu-core/src/lib.rs:42:7"),
            "crates/soxu-core/src/lib.rs"
        );
        assert_eq!(strip_location_suffix("notes:a:1b"), "notes:a:1b");
        assert_eq!(strip_location_suffix("README.md"), "README.md");
        assert_eq!(strip_location_suffix("weird:"), "weird:");
    }

    /// 归一化：反斜杠、`file://`、`.` 段都被收敛。
    #[test]
    fn normalizes_lexically() {
        assert_eq!(
            normalize_link_path("  crates\\soxu-desktop\\src\\shell.rs:12  ").as_deref(),
            Some("crates/soxu-desktop/src/shell.rs")
        );
        assert_eq!(
            normalize_link_path("file:///repo/./src/./lib.rs").as_deref(),
            Some("repo/src/lib.rs")
        );
    }

    /// 越界逃逸与空输入一律拒绝（纯词法，不探测文件系统）。
    #[test]
    fn rejects_escapes_and_empty_inputs() {
        assert_eq!(normalize_link_path("../../etc/passwd"), None);
        assert_eq!(normalize_link_path("src/../../escape.rs"), None);
        assert_eq!(normalize_link_path(""), None);
        assert_eq!(normalize_link_path("   "), None);
        assert_eq!(normalize_link_path("./"), None);

        // 根内回退是合法归一化。
        assert_eq!(
            normalize_link_path("src/../lib.rs").as_deref(),
            Some("lib.rs")
        );
    }

    /// 快照去重升序，过滤保持大小写不敏感子串语义。
    #[test]
    fn index_dedups_sorts_and_filters() {
        let index = FileIndex::new(["src/b.rs", "src/a.rs", "src/a.rs", "docs/guide.md"]);
        assert_eq!(index.len(), 3);
        assert_eq!(
            index.paths(),
            &[
                "docs/guide.md".to_string(),
                "src/a.rs".to_string(),
                "src/b.rs".to_string()
            ]
        );

        assert_eq!(index.filter("SRC/"), vec!["src/a.rs", "src/b.rs"]);
        assert_eq!(
            index.filter(""),
            vec!["docs/guide.md", "src/a.rs", "src/b.rs"]
        );
        assert!(index.filter("zzz").is_empty());
        assert!(index.is_filtered_empty("zzz"));
        assert!(!index.is_filtered_empty(""));
    }

    /// 空快照是空态而非「过滤无结果」态（两者渲染文案不同）。
    #[test]
    fn empty_index_is_not_filtered_empty() {
        let index = FileIndex::new(Vec::<String>::new());
        assert!(index.is_empty());
        assert!(!index.is_filtered_empty("anything"));
    }
}
