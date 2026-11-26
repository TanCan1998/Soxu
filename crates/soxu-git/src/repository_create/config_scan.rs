//! 仓库配置与属性来源的只读扫描。
//!
//! 本模块不解释完整 Git 语义，只拒绝会让受控发布引入外部对象、外部程序、外部过滤器
//! 或未获授权配置来源的段落与键。扫描按行进行，不做任何 Git 调用，因此结果与 Git 版本无关。

use super::RepositoryCreateError;

/// 出现即拒绝的配置片段（小写子串），作为段落解析的兜底。
const REJECTED_FRAGMENTS: [&str; 6] = [
    "promisor",
    "partialclone",
    "alternates",
    "fsmonitor",
    "sshcommand",
    "insteadof",
];

/// 出现即拒绝的配置段落：外部配置来源、过滤器、子模块与 LFS。
const REJECTED_SECTIONS: [&str; 6] = [
    "include",
    "includeif",
    "filter",
    "submodule",
    "lfs",
    "alias",
];

/// 非空即拒绝的键：外部配置来源、外部程序、未获授权的对象路径与工作树重定向。
const REJECTED_KEYS: [&str; 16] = [
    "hookspath",
    "askpass",
    "sshcommand",
    "fsmonitor",
    "editor",
    "pager",
    "worktree",
    "alternates",
    "external",
    "textconv",
    "driver",
    "uploadpack",
    "receivepack",
    "worktreeconfig",
    "partialclone",
    "preciousobjects",
];

/// 扫描配置字节，拒绝外部配置来源、外部对象来源、辅助进程与过滤器驱动段。
pub(super) fn reject_unsafe(bytes: &[u8], what: &'static str) -> Result<(), RepositoryCreateError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| RepositoryCreateError::UnsupportedSource("配置不是合法 UTF-8"))?;
    let folded = text.to_ascii_lowercase();
    for fragment in REJECTED_FRAGMENTS {
        if folded.contains(fragment) {
            return Err(RepositoryCreateError::UnsupportedSource(what));
        }
    }
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(rest) = line.strip_prefix('[') {
            let Some(end) = rest.find(']') else {
                return Err(RepositoryCreateError::UnsupportedSource(what));
            };
            let header = &rest[..end];
            let name = header
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .to_ascii_lowercase();
            if REJECTED_SECTIONS.contains(&name.as_str()) {
                return Err(RepositoryCreateError::UnsupportedSource(what));
            }
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        if REJECTED_KEYS.contains(&key.as_str()) && !value.trim().is_empty() {
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
