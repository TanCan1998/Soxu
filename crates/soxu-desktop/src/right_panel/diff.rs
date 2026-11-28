//! 统一 Diff 解析与行号槽（`pages/01-desktop-workbench.md` §4 `Diff` 页签）。
//!
//! 规范要点：
//! - **行号槽右对齐**：新旧双侧行号以等宽数字右对齐，空号位保留等宽空格，
//!   避免增删行让代码列左右跳动（§4「实时工作树差异审阅」+ §6 组件解剖表）；
//! - **400 行截断**：单文件渲染行上限 400，超出即截断并在末尾标注，
//!   防止超长 Diff 拖垮渲染帧（§4 渲染零阻塞铁律）；
//! - 只接受**归一化 unified diff** 文本：文件头（`diff --git` / `index` / `---` / `+++` /
//!   `\ No newline`）不进入行列表，由所属文件行承担路径语义。
//!
//! 结构借鉴 Padu `apps/mobile/src/lib/diff-presentation.ts`（`MAX_DIFF_LINES = 400`
//! 与文件头剔除、尾随空上下文行回收）与 `apps/desktop/src/review_diff.rs`
//! （`old_line`/`new_line` 双侧行号模型），按本仓设计令牌与纯内存边界重写。

use crate::tokens::sizing;

/// 单文件 Diff 渲染行上限（400 行截断语义）。
pub const MAX_DIFF_LINES: usize = 400;

/// 行号槽单侧数字位宽（右对齐基准，支持至 9999 行）。
pub const GUTTER_DIGITS: usize = 4;

/// Diff 单行语义类别。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffLineKind {
    /// 区段头 `@@ -a,b +c,d @@`。
    Hunk,
    /// 新增行（`+` 前缀）。
    Addition,
    /// 删除行（`-` 前缀）。
    Removal,
    /// 上下文行（空格或无前缀）。
    Context,
}

impl DiffLineKind {
    /// 行首标记字形（纯字符，非 Emoji；对齐 `pages/01` §4 单栏视图）。
    pub const fn marker(self) -> &'static str {
        match self {
            Self::Hunk => "@@",
            Self::Addition => "+",
            Self::Removal => "-",
            Self::Context => " ",
        }
    }

    /// 该类别是否计入变更统计。
    pub const fn is_change(self) -> bool {
        matches!(self, Self::Addition | Self::Removal)
    }
}

/// 一行 Diff：类别、双侧行号（缺失侧为 `None`）与行文本（不含前缀标记）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffLine {
    /// 行类别。
    pub kind: DiffLineKind,
    /// 旧文件行号；新增行与区段头为 `None`。
    pub old_line: Option<u32>,
    /// 新文件行号；删除行与区段头为 `None`。
    pub new_line: Option<u32>,
    /// 去前缀后的行文本。
    pub text: String,
}

impl DiffLine {
    /// 旧侧行号槽文案（右对齐，缺失时等宽空格）。
    pub fn old_gutter(&self) -> String {
        format_gutter(self.old_line)
    }

    /// 新侧行号槽文案（右对齐，缺失时等宽空格）。
    pub fn new_gutter(&self) -> String {
        format_gutter(self.new_line)
    }
}

/// 解析结果：行列表 + 是否因 400 行上限被截断。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParsedDiff {
    /// 待渲染行（已剔除文件头）。
    pub lines: Vec<DiffLine>,
    /// 是否触发 400 行截断。
    pub truncated: bool,
}

impl ParsedDiff {
    /// 是否没有任何可渲染行（对应空态）。
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// 新增行数（仅统计已渲染范围）。
    pub fn additions(&self) -> u32 {
        self.count_kind(DiffLineKind::Addition)
    }

    /// 删除行数（仅统计已渲染范围）。
    pub fn deletions(&self) -> u32 {
        self.count_kind(DiffLineKind::Removal)
    }

    /// 行统计摘要：`+28 -4`。
    pub fn stat_label(&self) -> String {
        format!("+{} -{}", self.additions(), self.deletions())
    }

    /// 截断提示文案（未截断时为 `None`）。
    pub fn truncation_notice(&self) -> Option<String> {
        self.truncated
            .then(|| format!("差异过长，仅显示前 {MAX_DIFF_LINES} 行"))
    }

    fn count_kind(&self, kind: DiffLineKind) -> u32 {
        let count = self.lines.iter().filter(|line| line.kind == kind).count();
        u32::try_from(count).unwrap_or(u32::MAX)
    }
}

/// 行号槽右对齐文案：有行号时右对齐定宽数字，无行号时同宽空格。
pub fn format_gutter(number: Option<u32>) -> String {
    match number {
        Some(value) => format!("{value:>width$}", width = GUTTER_DIGITS),
        None => " ".repeat(GUTTER_DIGITS),
    }
}

/// 解析区段头，返回 `(old_start, new_start)`；非法区段头返回 `None`。
pub fn parse_hunk_header(header: &str) -> Option<(u32, u32)> {
    let body = header.strip_prefix("@@")?;
    let body = body.split("@@").next()?;
    let mut old_start = None;
    let mut new_start = None;
    for token in body.split_whitespace() {
        if let Some(rest) = token.strip_prefix('-') {
            old_start = parse_hunk_start(rest);
        } else if let Some(rest) = token.strip_prefix('+') {
            new_start = parse_hunk_start(rest);
        }
    }
    Some((old_start?, new_start?))
}

/// 区段头的起止范围里只取起始行号（`12,7` → `12`）。
fn parse_hunk_start(token: &str) -> Option<u32> {
    token.split(',').next()?.parse::<u32>().ok()
}

/// 文件头/元信息行判定：这些行不进入行列表（路径与统计已由文件行承担）。
fn is_file_header_line(raw: &str) -> bool {
    raw.starts_with("diff --git")
        || raw.starts_with("index ")
        || raw.starts_with("--- ")
        || raw.starts_with("+++ ")
        || raw == "---"
        || raw == "+++"
        || raw.starts_with("\\ No newline")
}

/// 解析归一化 unified diff 文本。
///
/// 语义（与设计 §4 一致）：
/// 1. 文件头行跳过；区段头重置双侧行号游标；
/// 2. `+` 计新增（新侧行号递增），`-` 计删除（旧侧行号递增），其余计上下文（双增）；
/// 3. 累计到 [`MAX_DIFF_LINES`] 即截断并置 `truncated`，不再消费后续行；
/// 4. 结尾的空上下文行回收（统一 Diff 末尾常见多余空行）。
pub fn parse_unified_diff(text: &str) -> ParsedDiff {
    let mut parsed = ParsedDiff::default();
    let mut old_cursor = 1_u32;
    let mut new_cursor = 1_u32;

    for raw in text.split('\n') {
        if parsed.lines.len() >= MAX_DIFF_LINES {
            parsed.truncated = true;
            break;
        }
        if is_file_header_line(raw) {
            continue;
        }
        if raw.starts_with("@@") {
            if let Some((old_start, new_start)) = parse_hunk_header(raw) {
                old_cursor = old_start;
                new_cursor = new_start;
            }
            parsed.lines.push(DiffLine {
                kind: DiffLineKind::Hunk,
                old_line: None,
                new_line: None,
                text: raw.trim().to_string(),
            });
            continue;
        }
        if let Some(rest) = raw.strip_prefix('+') {
            parsed.lines.push(DiffLine {
                kind: DiffLineKind::Addition,
                old_line: None,
                new_line: Some(new_cursor),
                text: rest.to_string(),
            });
            new_cursor = new_cursor.saturating_add(1);
            continue;
        }
        if let Some(rest) = raw.strip_prefix('-') {
            parsed.lines.push(DiffLine {
                kind: DiffLineKind::Removal,
                old_line: Some(old_cursor),
                new_line: None,
                text: rest.to_string(),
            });
            old_cursor = old_cursor.saturating_add(1);
            continue;
        }
        parsed.lines.push(DiffLine {
            kind: DiffLineKind::Context,
            old_line: Some(old_cursor),
            new_line: Some(new_cursor),
            text: raw.strip_prefix(' ').unwrap_or(raw).to_string(),
        });
        old_cursor = old_cursor.saturating_add(1);
        new_cursor = new_cursor.saturating_add(1);
    }

    trim_trailing_blank_context(&mut parsed.lines);
    parsed
}

/// 回收结尾的空上下文行（Diff 文本尾部常带一条换行产生的空行）。
fn trim_trailing_blank_context(lines: &mut Vec<DiffLine>) {
    while lines
        .last()
        .is_some_and(|line| line.kind == DiffLineKind::Context && line.text.is_empty())
    {
        lines.pop();
    }
}

/// 行号槽单侧像素宽度（供渲染取 `sizing::DIFF_GUTTER_WIDTH_PX` 的一半）。
pub const fn gutter_side_width_px() -> f32 {
    sizing::DIFF_GUTTER_WIDTH_PX / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 归一化 Diff 样例：两处文件头 + 一个区段。
    const SAMPLE: &str = "diff --git a/a.rs b/a.rs\n\
index 111..222 100644\n\
--- a/a.rs\n\
+++ b/a.rs\n\
@@ -10,3 +10,4 @@ fn main() {\n\
 let a = 1;\n\
-let b = 2;\n\
+let b = 3;\n\
+let c = 4;\n\
 }";

    /// 文件头被剔除，`+`/`-`/上下文行分别归类，行文本去掉前缀标记。
    #[test]
    fn parses_normalized_diff_and_drops_file_headers() {
        let parsed = parse_unified_diff(SAMPLE);
        assert!(!parsed.truncated);
        assert_eq!(
            parsed.lines.len(),
            6,
            "区段头 + 1 上下文 + 1 删 + 2 增 + 1 上下文"
        );

        assert_eq!(parsed.lines[0].kind, DiffLineKind::Hunk);
        assert_eq!(parsed.lines[0].text, "@@ -10,3 +10,4 @@ fn main() {");

        assert_eq!(parsed.lines[1].kind, DiffLineKind::Context);
        assert_eq!(parsed.lines[1].text, "let a = 1;");
        assert_eq!(parsed.lines[2].kind, DiffLineKind::Removal);
        assert_eq!(parsed.lines[2].text, "let b = 2;");
        assert_eq!(parsed.lines[3].kind, DiffLineKind::Addition);
        assert_eq!(parsed.lines[3].text, "let b = 3;");
        assert_eq!(parsed.lines[5].text, "}");

        assert_eq!(parsed.additions(), 2);
        assert_eq!(parsed.deletions(), 1);
        assert_eq!(parsed.stat_label(), "+2 -1");
    }

    /// 双侧行号按区段头起点单调推进：删除行只占旧侧，新增行只占新侧。
    #[test]
    fn line_numbers_follow_hunk_cursors() {
        let parsed = parse_unified_diff(SAMPLE);
        // 上下文行同时消耗两侧行号。
        assert_eq!(parsed.lines[1].old_line, Some(10));
        assert_eq!(parsed.lines[1].new_line, Some(10));
        // 删除行占旧侧 11，不占新侧。
        assert_eq!(parsed.lines[2].old_line, Some(11));
        assert_eq!(parsed.lines[2].new_line, None);
        // 两条新增行占新侧 11、12。
        assert_eq!(parsed.lines[3].old_line, None);
        assert_eq!(parsed.lines[3].new_line, Some(11));
        assert_eq!(parsed.lines[4].new_line, Some(12));
        // 尾部上下文行回到两侧一致：旧 12 / 新 13。
        assert_eq!(parsed.lines[5].old_line, Some(12));
        assert_eq!(parsed.lines[5].new_line, Some(13));
    }

    /// 行号槽右对齐：定宽数字，空号位等宽空格。
    #[test]
    fn gutter_is_right_aligned_and_fixed_width() {
        assert_eq!(format_gutter(Some(7)), "   7");
        assert_eq!(format_gutter(Some(42)), "  42");
        assert_eq!(format_gutter(Some(1234)), "1234");
        assert_eq!(format_gutter(None), "    ");
        assert_eq!(format_gutter(Some(7)).chars().count(), GUTTER_DIGITS);
        assert_eq!(format_gutter(None).chars().count(), GUTTER_DIGITS);

        // DiffLine 便捷方法复用同一右对齐契约。
        let line = DiffLine {
            kind: DiffLineKind::Addition,
            old_line: None,
            new_line: Some(9),
            text: "x".to_string(),
        };
        assert_eq!(line.old_gutter(), "    ");
        assert_eq!(line.new_gutter(), "   9");
    }

    /// 400 行截断：超限只保留前 400 行并置截断标记与提示。
    #[test]
    fn truncates_at_four_hundred_lines() {
        let body = (0..MAX_DIFF_LINES + 120)
            .map(|index| format!("+line {index}"))
            .collect::<Vec<_>>()
            .join("\n");
        let parsed = parse_unified_diff(&body);

        assert_eq!(parsed.lines.len(), MAX_DIFF_LINES);
        assert!(parsed.truncated);
        assert_eq!(parsed.additions(), 400);
        assert_eq!(
            parsed.truncation_notice().as_deref(),
            Some("差异过长，仅显示前 400 行")
        );

        // 恰好 400 行不触发截断。
        let exact = (0..MAX_DIFF_LINES)
            .map(|index| format!("+line {index}"))
            .collect::<Vec<_>>()
            .join("\n");
        let parsed_exact = parse_unified_diff(&exact);
        assert_eq!(parsed_exact.lines.len(), MAX_DIFF_LINES);
        assert!(!parsed_exact.truncated);
        assert!(parsed_exact.truncation_notice().is_none());
    }

    /// 空文本 → 空态；只有文件头也视为空态。
    #[test]
    fn empty_and_header_only_diffs_are_empty() {
        assert!(parse_unified_diff("").is_empty());
        assert!(
            parse_unified_diff("diff --git a/x b/x\n--- a/x\n+++ b/x\n").is_empty(),
            "仅文件头不应产生可渲染行"
        );
    }

    /// 尾部空上下文行回收（统一 Diff 末尾常带一条空行）。
    #[test]
    fn trims_trailing_blank_context_lines() {
        let parsed = parse_unified_diff("@@ -1,1 +1,1 @@\n-old\n+new\n\n\n");
        assert_eq!(parsed.lines.len(), 3);
        assert_eq!(
            parsed.lines.last().map(|line| line.text.as_str()),
            Some("new")
        );
    }

    /// 区段头解析容错：缺逗号、非法数字。
    #[test]
    fn hunk_header_parsing() {
        assert_eq!(parse_hunk_header("@@ -10,3 +12,4 @@"), Some((10, 12)));
        assert_eq!(parse_hunk_header("@@ -1 +1 @@"), Some((1, 1)));
        assert_eq!(parse_hunk_header("@@"), None);
        assert_eq!(parse_hunk_header("not a hunk"), None);
    }

    /// 行号槽像素宽度来自设计令牌（单侧半宽，不硬编码）。
    #[test]
    fn gutter_width_comes_from_tokens() {
        assert_eq!(gutter_side_width_px(), sizing::DIFF_GUTTER_WIDTH_PX / 2.0);
        assert_eq!(gutter_side_width_px(), 26.0);
    }

    /// 行类别标记为纯字符且变更判定覆盖新增/删除。
    #[test]
    fn line_kind_markers() {
        assert_eq!(DiffLineKind::Addition.marker(), "+");
        assert_eq!(DiffLineKind::Removal.marker(), "-");
        assert!(DiffLineKind::Addition.is_change());
        assert!(DiffLineKind::Removal.is_change());
        assert!(!DiffLineKind::Context.is_change());
        assert!(!DiffLineKind::Hunk.is_change());
    }
}
