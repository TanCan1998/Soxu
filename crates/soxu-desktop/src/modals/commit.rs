//! 结构化 Git 提交弹窗（`pages/06-modals-onboarding-and-automation.md` §3.2）。
//!
//! 规范要点：
//! - 内边距 `24px`（`p-6`）、区块垂直间距 `16–20px` → 本模块只描述**内容**结构，
//!   卡片骨架与 `p(20)` 由 [`crate::modals::render_modal_card`] 统一提供；
//! - 复选框待提交文件变动列表，单文件可取消勾选并带内联行统计 `+28 -4`；
//! - **拆分式首行构建条**：提交类型（emoji+type）→ 范围（预设或自定义）→ 简述，
//!   并实时合成首行预览 `<emoji> <type>(<scope>): <简述>`；
//! - **提交前隐私与规范门禁栏**：Emoji 格式合规、无个人路径泄露、密钥扫描通过，
//!   任一失败即禁用「立即提交」。
//!
//! 阶段边界（Issue #133）：纯状态与校验模型，**不执行任何 Git 写入**；
//! 真实提交经 `soxu-protocol` 的 `CommitPlanPreview` 在 M6 由 daemon 承担。

/// 简述最小字符数（`AGENTS.md` 提交规范：中文简明描述 `4~72` 字）。
pub const SUMMARY_MIN_CHARS: usize = 4;

/// 简述最大字符数（同上）。
pub const SUMMARY_MAX_CHARS: usize = 72;

/// 「无范围」选项文案（首行省略括号段）。
pub const SCOPE_NONE_LABEL: &str = "无范围";

/// 提交范围预设（§3.2 组合选择器预设；自定义范围另行输入）。
pub const SCOPE_PRESETS: [&str; 8] = [
    "drivers", "core", "desktop", "mobile", "protocol", "git", "design", "ci",
];

/// 个人目录路径标记（命中即判定「可能泄露个人路径」）。
///
/// 仅用于**检测**，不包含任何真实用户名；大小写不敏感比较。
const PRIVATE_PATH_MARKERS: [&str; 5] =
    ["c:/users/", "c:\\users\\", "/users/", "/home/", "\\users\\"];

/// 密钥特征片段（轻量词法占位；正式门禁由 `.gitleaks` 在 pre-commit 执行）。
const SECRET_MARKERS: [&str; 7] = [
    "sk-",
    "ghp_",
    "github_pat_",
    "-----begin",
    "api_key",
    "secret=",
    "password=",
];

/// 提交类型（emoji + type，与 `AGENTS.md` 标准映射表一致）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum CommitKind {
    /// 新增功能。
    #[default]
    Feat,
    /// 修复缺陷。
    Fix,
    /// UI 布局、交互或设计令牌调整。
    Ui,
    /// 代码重构（外部行为不变）。
    Refactor,
    /// 文档与运维记录更新。
    Docs,
    /// 构建脚本、依赖升级、`.gitignore` 维护。
    Chore,
    /// 性能优化。
    Perf,
    /// 新增或完善测试。
    Test,
}

impl CommitKind {
    /// 全部类型，按弹窗快选面板顺序。
    pub const ALL: [Self; 8] = [
        Self::Feat,
        Self::Fix,
        Self::Ui,
        Self::Refactor,
        Self::Docs,
        Self::Chore,
        Self::Perf,
        Self::Test,
    ];

    /// 类型对应的标准 emoji。
    pub const fn emoji(self) -> &'static str {
        match self {
            Self::Feat => "✨",
            Self::Fix => "🐛",
            Self::Ui => "💄",
            Self::Refactor => "♻️",
            Self::Docs => "📝",
            Self::Chore => "🔧",
            Self::Perf => "⚡",
            Self::Test => "✅",
        }
    }

    /// 类型对应的 conventional-commit type 段。
    pub const fn type_name(self) -> &'static str {
        match self {
            Self::Feat => "feat",
            Self::Fix => "fix",
            Self::Ui => "ui",
            Self::Refactor => "refactor",
            Self::Docs => "docs",
            Self::Chore => "chore",
            Self::Perf => "perf",
            Self::Test => "test",
        }
    }

    /// 快选按钮文案：`✨ feat`。
    pub fn label(self) -> String {
        format!("{} {}", self.emoji(), self.type_name())
    }
}

/// 待提交文件变动行（可取消勾选 + 内联行统计）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitFile {
    /// 工作区相对路径。
    pub path: String,
    /// 是否勾选为待提交。
    pub staged: bool,
    /// 新增行数。
    pub additions: u32,
    /// 删除行数。
    pub deletions: u32,
}

impl CommitFile {
    /// 默认勾选的文件行。
    pub fn new(path: impl Into<String>, additions: u32, deletions: u32) -> Self {
        Self {
            path: path.into(),
            staged: true,
            additions,
            deletions,
        }
    }

    /// 内联行统计：`+28 -4`。
    pub fn stat_label(&self) -> String {
        format!("+{} -{}", self.additions, self.deletions)
    }
}

/// 提交前门禁结果（§3.2 门禁栏三态 + 范围合法性）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CommitGate {
    /// 首行格式合规（类型 + 范围 + 简述长度与单行约束）。
    pub first_line_ok: bool,
    /// 范围合法（空或仅 `[a-z0-9_/-]`）。
    pub scope_ok: bool,
    /// 无个人路径泄露。
    pub no_private_path: bool,
    /// 密钥扫描通过。
    pub secret_scan_ok: bool,
}

impl CommitGate {
    /// 三项门禁是否全部通过（禁用「立即提交」的判据之一）。
    pub const fn all_ok(&self) -> bool {
        self.first_line_ok && self.scope_ok && self.no_private_path && self.secret_scan_ok
    }
}

/// 提交草稿：类型、范围、简述、正文与文件勾选状态。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CommitDraft {
    /// 提交类型。
    pub kind: CommitKind,
    /// 提交范围（空字符串表示「无范围」）。
    pub scope: String,
    /// 简述（首行描述段）。
    pub summary: String,
    /// 多行正文。
    pub body: String,
    /// 待提交文件列表。
    pub files: Vec<CommitFile>,
}

impl CommitDraft {
    /// 空草稿（无文件、无范围、`✨ feat`）。
    pub fn new() -> Self {
        Self::default()
    }

    /// 首行合成预览：`<emoji> <type>(<scope>): <简述>`。
    ///
    /// 无范围时省略括号段（与仓库提交规范一致）。
    pub fn first_line(&self) -> String {
        let scope = self.scope.trim();
        if scope.is_empty() {
            format!(
                "{} {}: {}",
                self.kind.emoji(),
                self.kind.type_name(),
                self.summary.trim()
            )
        } else {
            format!(
                "{} {}({}): {}",
                self.kind.emoji(),
                self.kind.type_name(),
                scope,
                self.summary.trim()
            )
        }
    }

    /// 选中某个范围预设；传入 [`SCOPE_NONE_LABEL`] 即清空范围。
    pub fn set_scope_preset(&mut self, preset: &str) {
        self.scope = if preset == SCOPE_NONE_LABEL {
            String::new()
        } else {
            preset.to_string()
        };
    }

    /// 自定义范围输入（由门禁校验合法性）。
    pub fn set_scope(&mut self, scope: impl Into<String>) {
        self.scope = scope.into();
    }

    /// 翻转某文件的勾选状态（越界忽略）。
    pub fn toggle_file(&mut self, index: usize) -> bool {
        match self.files.get_mut(index) {
            Some(file) => {
                file.staged = !file.staged;
                true
            }
            None => false,
        }
    }

    /// 已勾选文件（提交范围）。
    pub fn staged_files(&self) -> Vec<&CommitFile> {
        self.files.iter().filter(|file| file.staged).collect()
    }

    /// 勾选文件数。
    pub fn staged_count(&self) -> usize {
        self.files.iter().filter(|file| file.staged).count()
    }

    /// 当前门禁结果。
    pub fn gate(&self) -> CommitGate {
        let scope_ok = is_valid_scope(&self.scope);
        let summary_chars = self.summary.trim().chars().count();
        let first_line_ok = !self.summary.trim().is_empty()
            && !self.summary.contains('\n')
            && (SUMMARY_MIN_CHARS..=SUMMARY_MAX_CHARS).contains(&summary_chars);
        let text = format!("{}\n{}", self.summary, self.body).to_lowercase();
        CommitGate {
            first_line_ok,
            scope_ok,
            no_private_path: !contains_any(&text, &PRIVATE_PATH_MARKERS),
            secret_scan_ok: !contains_any(&text, &SECRET_MARKERS),
        }
    }

    /// 「立即提交」是否可用：门禁全过且至少勾选一个文件。
    pub fn can_submit(&self) -> bool {
        self.gate().all_ok() && self.staged_count() > 0
    }
}

/// 范围合法性：空（无范围）或仅 `[a-z0-9_/-]` 且不以 `-` / `/` 开头结尾
/// （与提交门禁正则同源，仅额外拒绝无意义的纯符号范围段）。
pub fn is_valid_scope(scope: &str) -> bool {
    let trimmed = scope.trim();
    if trimmed.is_empty() {
        return true;
    }
    let charset_ok = trimmed
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '_' | '/' | '-'));
    let edges_ok = trimmed
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_alphanumeric())
        && trimmed
            .chars()
            .last()
            .is_some_and(|ch| ch.is_ascii_alphanumeric());
    charset_ok && edges_ok
}

/// 文本是否包含任一标记（大小写不敏感，调用方已小写化）。
fn contains_any(lowercased: &str, markers: &[&str]) -> bool {
    markers.iter().any(|marker| lowercased.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 首行合成：有范围带括号，无范围省略括号段。
    #[test]
    fn first_line_composition() {
        let mut draft = CommitDraft::new();
        draft.kind = CommitKind::Feat;
        draft.summary = "落地右栏审阅与设置模态".to_string();
        assert_eq!(draft.first_line(), "✨ feat: 落地右栏审阅与设置模态");

        draft.set_scope_preset("desktop");
        assert_eq!(
            draft.first_line(),
            "✨ feat(desktop): 落地右栏审阅与设置模态"
        );

        draft.set_scope_preset(SCOPE_NONE_LABEL);
        assert!(draft.scope.is_empty());
        draft.kind = CommitKind::Fix;
        assert_eq!(draft.first_line(), "🐛 fix: 落地右栏审阅与设置模态");
    }

    /// 类型映射表与 `AGENTS.md` 标准一致。
    #[test]
    fn commit_kind_catalog_matches_repo_convention() {
        assert_eq!(CommitKind::ALL.len(), 8);
        assert_eq!(CommitKind::Feat.label(), "✨ feat");
        assert_eq!(CommitKind::Fix.label(), "🐛 fix");
        assert_eq!(CommitKind::Ui.label(), "💄 ui");
        assert_eq!(CommitKind::Refactor.label(), "♻️ refactor");
        assert_eq!(CommitKind::Docs.label(), "📝 docs");
        assert_eq!(CommitKind::Chore.label(), "🔧 chore");
        assert_eq!(CommitKind::Perf.label(), "⚡ perf");
        assert_eq!(CommitKind::Test.label(), "✅ test");
    }

    /// 范围校验：预设合法，自定义须小写字母/数字与 `_/-`。
    #[test]
    fn scope_validation() {
        for preset in SCOPE_PRESETS {
            assert!(is_valid_scope(preset), "预设 {preset} 应合法");
        }
        assert!(is_valid_scope(""));
        assert!(is_valid_scope("  "));
        assert!(is_valid_scope("m5-c/right_panel"));
        assert!(!is_valid_scope("Desktop"));
        assert!(!is_valid_scope("--evil"));
        assert!(!is_valid_scope("desktop)"));

        let mut draft = CommitDraft::new();
        draft.set_scope("Desktop");
        assert!(!draft.gate().scope_ok);
        draft.set_scope("desktop");
        assert!(draft.gate().scope_ok);
    }

    /// 简述长度边界：4~72 字符（按字符而非字节计数，兼容中文）。
    #[test]
    fn summary_length_boundaries() {
        let mut draft = CommitDraft::new();
        draft.summary = "落地".to_string();
        assert_eq!(draft.summary.chars().count(), 2);
        assert!(!draft.gate().first_line_ok);

        draft.summary = "落地右栏".to_string();
        assert_eq!(draft.summary.chars().count(), 4);
        assert!(draft.gate().first_line_ok);

        draft.summary = "测".repeat(SUMMARY_MAX_CHARS);
        assert!(draft.gate().first_line_ok);
        draft.summary = "测".repeat(SUMMARY_MAX_CHARS + 1);
        assert!(!draft.gate().first_line_ok);

        // 多行简述不合规（首行必须是单行）。
        draft.summary = "落地右栏\n第二行".to_string();
        assert!(!draft.gate().first_line_ok);

        draft.summary = "   ".to_string();
        assert!(!draft.gate().first_line_ok);
    }

    /// 隐私与密钥门禁：命中即置灰，正文同样参与扫描。
    #[test]
    fn privacy_and_secret_gates() {
        let mut draft = CommitDraft::new();
        draft.summary = "落地右栏审阅面板".to_string();
        assert!(draft.gate().no_private_path);
        assert!(draft.gate().secret_scan_ok);

        draft.body = "来源 C:/Users/example/project".to_string();
        assert!(!draft.gate().no_private_path);

        draft.body = "token=ghp_placeholder".to_string();
        // `ghp_` 与隐私无关，但触发密钥门禁。
        assert!(draft.gate().no_private_path);
        assert!(!draft.gate().secret_scan_ok);

        draft.body = "正常正文，Refs #133".to_string();
        assert!(draft.gate().all_ok());
    }

    /// 勾选状态与提交可用性：门禁全过但零勾选仍不可提交。
    #[test]
    fn submit_requires_staged_files_and_green_gate() {
        let mut draft = CommitDraft::new();
        draft.kind = CommitKind::Feat;
        draft.set_scope_preset("desktop");
        draft.summary = "落地右栏审阅与核心模态".to_string();
        assert_eq!(draft.staged_count(), 0);
        assert!(!draft.can_submit(), "零勾选文件不得提交");

        draft.files = vec![
            CommitFile::new("crates/soxu-desktop/src/right_panel/mod.rs", 28, 4),
            CommitFile::new("crates/soxu-desktop/src/shell.rs", 2, 2),
        ];
        assert_eq!(draft.staged_count(), 2);
        assert!(draft.can_submit());
        assert_eq!(draft.staged_files()[0].stat_label(), "+28 -4");

        assert!(draft.toggle_file(0));
        assert!(!draft.files[0].staged);
        assert_eq!(draft.staged_count(), 1);
        assert!(!draft.toggle_file(9), "越界勾选应被忽略");

        // 全部取消勾选后再次禁用提交。
        assert!(draft.toggle_file(1));
        assert!(!draft.can_submit());
    }
}
