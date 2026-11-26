//! 仓库配置与属性来源的只读扫描。
//!
//! 本模块不解释完整 Git 语义，只拒绝会让受控发布引入外部对象、外部程序或外部过滤器的来源。

use super::RepositoryCreateError;

/// 出现即拒绝的配置片段（小写子串）。
const REJECTED_FRAGMENTS: [&str; 6] = [
    "promisor",
    "partialclone",
    "alternates",
    "fsmonitor",
    "sshcommand",
    "insteadof",
];

/// 必须为空值才放行的配置键。
const EMPTY_ONLY_KEYS: [&str; 3] = ["hookspath", "templatedir", "askpass"];

/// 扫描配置字节，拒绝外部对象来源、辅助进程与过滤器驱动段。
pub(super) fn reject_unsafe(bytes: &[u8], what: &'static str) -> Result<(), RepositoryCreateError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| RepositoryCreateError::UnsupportedSource("配置不是合法 UTF-8"))?;
    let folded = text.to_ascii_lowercase();
    for fragment in REJECTED_FRAGMENTS {
        if folded.contains(fragment) {
            return Err(RepositoryCreateError::UnsupportedSource(what));
        }
    }
    if folded.contains("[filter") {
        return Err(RepositoryCreateError::UnsupportedSource(what));
    }
    for line in folded.lines() {
        let line = line.trim();
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if EMPTY_ONLY_KEYS.contains(&key) && !value.trim().is_empty() {
            return Err(RepositoryCreateError::UnsupportedSource(what));
        }
    }
    Ok(())
}

/// 提交树中的属性文件一旦声明过滤器或编码转换即拒绝发布。
pub(super) fn reject_filter_attributes(bytes: &[u8]) -> Result<(), RepositoryCreateError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| RepositoryCreateError::UnsafeContent("属性文件不是合法 UTF-8"))?;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let folded = line.to_ascii_lowercase();
        if folded.contains("filter=")
            || folded.contains("textconv")
            || folded.contains("working-tree-encoding")
        {
            return Err(RepositoryCreateError::UnsafeContent(
                "提交属性声明了外部过滤器",
            ));
        }
    }
    Ok(())
}
