//! 上下文附件芯片、`+` 更多操作菜单与 `/` 斜杠命令纯内存过滤（无 I/O）。
//!
//! 规范来源：
//! - `pages/02-composer-and-handoff.md` §5.1 上下文芯片像素解剖（文件/行区间、图片缩略、
//!   可移除 `×`）；
//! - §3.6 左侧 `+` 按钮：上下文引用（源码片段、目录、`Git Diff`、最近终端输出、
//!   图片/本地文件上传）与快捷技能指令（`/plan`、`/review`、`check-all`）；
//! - §4.2 `/` 技能补全：聚合已预热缓存后**击键过滤纯内存 0 I/O 完成（耗时 < 1ms）**，
//!   弹层最高展示 8 行。
//!
//! 芯片契约类型复用 [`soxu_protocol::ComposerChip`]；本模块只补构建器、去重集合与过滤。

use soxu_protocol::{ComposerChip, ComposerChipKind};

/// 补全弹层单次最多展示的行数（`pages/02` §4：`最高展示 8 行`）。
pub const AUTOCOMPLETE_MAX_ROWS: usize = 8;

/// 上下文芯片集合：保持插入顺序，(kind, 引用) 相同的芯片不重复挂载。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChipSet {
    chips: Vec<ComposerChip>,
}

impl ChipSet {
    /// 空集合。
    pub fn new() -> Self {
        Self { chips: Vec::new() }
    }

    /// 挂载一枚芯片；同类型同引用已存在时返回 `false` 且不重复插入。
    pub fn push(&mut self, chip: ComposerChip) -> bool {
        if self
            .chips
            .iter()
            .any(|existing| same_source(existing, &chip))
        {
            return false;
        }
        self.chips.push(chip);
        true
    }

    /// 按 id 移除芯片，返回是否真的移除了。
    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.chips.len();
        self.chips.retain(|chip| chip.id != id);
        self.chips.len() != before
    }

    /// 已挂载芯片（按插入顺序）。
    pub fn iter(&self) -> impl Iterator<Item = &ComposerChip> {
        self.chips.iter()
    }

    /// 芯片数量。
    pub fn len(&self) -> usize {
        self.chips.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.chips.is_empty()
    }

    /// 清空全部芯片。
    pub fn clear(&mut self) {
        self.chips.clear();
    }
}

/// 两枚芯片是否指向同一来源（同类型 + 同路径引用，或同类型 + 同标签）。
fn same_source(left: &ComposerChip, right: &ComposerChip) -> bool {
    if left.kind != right.kind {
        return false;
    }
    match (&left.path_or_ref, &right.path_or_ref) {
        (Some(a), Some(b)) => a == b,
        (None, None) => left.label == right.label,
        _ => false,
    }
}

/// 构建「代码文件 / 行区间」芯片：`crates/soxu-drivers/src/lib.rs:18-46`。
pub fn code_chip(id: &str, path: &str, start_line: u32, end_line: u32) -> ComposerChip {
    ComposerChip {
        id: id.to_string(),
        kind: ComposerChipKind::File,
        label: format!("{path}:{start_line}-{end_line}"),
        path_or_ref: Some(path.to_string()),
        target_agent: None,
        target_model: None,
        target_thinking: None,
    }
}

/// 构建「目录」芯片。
pub fn folder_chip(id: &str, path: &str) -> ComposerChip {
    ComposerChip {
        id: id.to_string(),
        kind: ComposerChipKind::Folder,
        label: path.to_string(),
        path_or_ref: Some(path.to_string()),
        target_agent: None,
        target_model: None,
        target_thinking: None,
    }
}

/// 构建「图片 / 截屏附件」芯片：`shot.png 12.4 KB`。
pub fn image_chip(id: &str, file_name: &str, bytes: u64) -> ComposerChip {
    ComposerChip {
        id: id.to_string(),
        kind: ComposerChipKind::Image,
        label: format!("{file_name} {}", format_bytes(bytes)),
        path_or_ref: Some(file_name.to_string()),
        target_agent: None,
        target_model: None,
        target_thinking: None,
    }
}

/// 构建「终端输出」芯片。
pub fn terminal_chip(id: &str, command: &str) -> ComposerChip {
    ComposerChip {
        id: id.to_string(),
        kind: ComposerChipKind::Terminal,
        label: command.to_string(),
        path_or_ref: Some(command.to_string()),
        target_agent: None,
        target_model: None,
        target_thinking: None,
    }
}

/// 字节数的人类可读表示（`512 B` / `12.4 KB` / `3.1 MB`）。
pub fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    let value = bytes as f64;
    if value >= MB {
        format!("{:.1} MB", value / MB)
    } else if value >= KB {
        format!("{:.1} KB", value / KB)
    } else {
        format!("{bytes} B")
    }
}

/// 斜杠命令的类别（决定是否需要提示「执行」还是「插入技能」）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlashKind {
    /// 内置快捷动作（直接执行）。
    Action,
    /// 技能 / 子代理入口（插入并执行）。
    Skill,
}

/// 一条斜杠命令。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlashCommand {
    /// 命令名（含前导 `/`，如 `/plan`）。
    pub name: &'static str,
    /// 中文短标签。
    pub label: &'static str,
    /// 中文说明。
    pub description: &'static str,
    /// 类别。
    pub kind: SlashKind,
}

/// 内置斜杠命令目录（§4.2 明列项 + `check-all` 门禁命令）。
pub const SLASH_COMMANDS: [SlashCommand; 6] = [
    SlashCommand {
        name: "/plan",
        label: "规划",
        description: "先产出完整执行方案，暂不改动代码",
        kind: SlashKind::Action,
    },
    SlashCommand {
        name: "/review",
        label: "审阅",
        description: "让代理审阅当前工作树改动并给出结论",
        kind: SlashKind::Action,
    },
    SlashCommand {
        name: "/check-all",
        label: "全仓门禁",
        description: "运行仓库统一门禁检查并汇总结果",
        kind: SlashKind::Action,
    },
    SlashCommand {
        name: "/compact",
        label: "压缩上下文",
        description: "压缩冗余对话上下文，释放窗口占用",
        kind: SlashKind::Action,
    },
    SlashCommand {
        name: "/checkpoint",
        label: "检查点",
        description: "为当前工作树创建可回滚检查点",
        kind: SlashKind::Action,
    },
    SlashCommand {
        name: "/handoff",
        label: "移交",
        description: "把上下文移交给另一个代理继续推进",
        kind: SlashKind::Skill,
    },
];

/// 纯内存过滤斜杠命令：0 I/O、无分配表结构、最多返回 [`AUTOCOMPLETE_MAX_ROWS`] 行。
///
/// 匹配规则（大小写不敏感，允许携带前导 `/`）：
/// 1. 命令名前缀命中优先（排名 0）；
/// 2. 命令名包含命中（排名 1）；
/// 3. 中文标签 / 说明包含命中（排名 2）。
pub fn filter_slash_commands(query: &str) -> Vec<&'static SlashCommand> {
    let needle = query.trim().trim_start_matches('/').to_lowercase();

    let mut ranked: Vec<(u8, usize, &'static SlashCommand)> = Vec::new();
    for (index, command) in SLASH_COMMANDS.iter().enumerate() {
        let name = command.name.trim_start_matches('/').to_lowercase();
        let rank = if needle.is_empty() || name.starts_with(&needle) {
            Some(0_u8)
        } else if name.contains(&needle) {
            Some(1)
        } else if command.label.contains(&needle)
            || command.description.to_lowercase().contains(&needle)
        {
            Some(2)
        } else {
            None
        };
        if let Some(rank) = rank {
            ranked.push((rank, index, command));
        }
    }

    // 稳定排序：先按命中排名，再按目录声明顺序。
    ranked.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    ranked
        .into_iter()
        .take(AUTOCOMPLETE_MAX_ROWS)
        .map(|(_, _, command)| command)
        .collect()
}

/// `+` 更多操作菜单的分区。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlusMenuSection {
    /// 上下文引用。
    Context,
    /// 快捷技能指令。
    Shortcut,
}

/// `+` 菜单项。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlusMenuItem {
    /// 稳定键（供点击回调与测试断言）。
    pub key: &'static str,
    /// 中文标签。
    pub label: &'static str,
    /// 所属分区。
    pub section: PlusMenuSection,
}

/// `+` 菜单项清单（§3.6：5 项上下文引用 + 3 项快捷指令）。
pub const PLUS_MENU_ITEMS: [PlusMenuItem; 8] = [
    PlusMenuItem {
        key: "context.snippet",
        label: "源码片段",
        section: PlusMenuSection::Context,
    },
    PlusMenuItem {
        key: "context.folder",
        label: "目录",
        section: PlusMenuSection::Context,
    },
    PlusMenuItem {
        key: "context.diff",
        label: "Git Diff",
        section: PlusMenuSection::Context,
    },
    PlusMenuItem {
        key: "context.terminal",
        label: "最近终端输出",
        section: PlusMenuSection::Context,
    },
    PlusMenuItem {
        key: "context.image",
        label: "图片或本地文件",
        section: PlusMenuSection::Context,
    },
    PlusMenuItem {
        key: "shortcut.plan",
        label: "/plan",
        section: PlusMenuSection::Shortcut,
    },
    PlusMenuItem {
        key: "shortcut.review",
        label: "/review",
        section: PlusMenuSection::Shortcut,
    },
    PlusMenuItem {
        key: "shortcut.check-all",
        label: "/check-all",
        section: PlusMenuSection::Shortcut,
    },
];

/// 取某分区的菜单项（顺序同 [`PLUS_MENU_ITEMS`]）。
pub fn plus_menu_items(section: PlusMenuSection) -> impl Iterator<Item = &'static PlusMenuItem> {
    PLUS_MENU_ITEMS
        .iter()
        .filter(move |item| item.section == section)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 芯片集合：同类型同引用去重，移除按 id 精确命中。
    #[test]
    fn chip_set_dedupes_and_removes() {
        let mut chips = ChipSet::new();
        assert!(chips.is_empty());

        assert!(chips.push(code_chip("chip-1", "crates/soxu-core/src/lib.rs", 18, 46)));
        // 同路径同类型（不同 id）不重复挂载。
        assert!(!chips.push(code_chip("chip-2", "crates/soxu-core/src/lib.rs", 18, 46)));
        // 同路径不同类型可以共存。
        assert!(chips.push(folder_chip("chip-3", "crates/soxu-core/src/lib.rs")));
        assert_eq!(chips.len(), 2);

        assert!(chips.remove("chip-1"));
        assert!(!chips.remove("chip-1"));
        assert_eq!(chips.len(), 1);
        assert_eq!(
            chips.iter().next().map(|c| c.kind),
            Some(ComposerChipKind::Folder)
        );

        chips.clear();
        assert!(chips.is_empty());
    }

    /// 文件/行区间芯片的标签格式为 `path:start-end`（§5.1）。
    #[test]
    fn code_chip_label_matches_spec() {
        let chip = code_chip("c1", "crates/soxu-drivers/src/lib.rs", 18, 46);
        assert_eq!(chip.label, "crates/soxu-drivers/src/lib.rs:18-46");
        assert_eq!(chip.kind, ComposerChipKind::File);
        assert_eq!(
            chip.path_or_ref.as_deref(),
            Some("crates/soxu-drivers/src/lib.rs")
        );
    }

    /// 图片芯片带人类可读体积；无芯片字段超出协议定义。
    #[test]
    fn image_chip_carries_size() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(12_697), "12.4 KB");
        assert_eq!(format_bytes(3_250_000), "3.1 MB");

        let chip = image_chip("c2", "shot.png", 12_697);
        assert_eq!(chip.kind, ComposerChipKind::Image);
        assert_eq!(chip.label, "shot.png 12.4 KB");
    }

    /// 空查询返回全部命令，且不超过 8 行上限。
    #[test]
    fn empty_query_lists_catalog_within_row_cap() {
        let all = filter_slash_commands("");
        assert_eq!(all.len(), SLASH_COMMANDS.len());
        assert!(all.len() <= AUTOCOMPLETE_MAX_ROWS);
        assert_eq!(all[0].name, "/plan");
    }

    /// 前缀命中优先于子串命中，且允许携带前导 `/`。
    #[test]
    fn prefix_match_ranks_first() {
        let hits = filter_slash_commands("/rev");
        assert_eq!(hits.first().map(|c| c.name), Some("/review"));

        // "plan" 只命中 /plan。
        let plan = filter_slash_commands("plan");
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].name, "/plan");

        // "check" 命中 /check-all 与 /checkpoint，前缀命中 /check-all 排前。
        let check = filter_slash_commands("check");
        assert_eq!(check.first().map(|c| c.name), Some("/check-all"));
        assert!(check.iter().any(|c| c.name == "/checkpoint"));
    }

    /// 中文标签也可命中（`移交` → /handoff）。
    #[test]
    fn chinese_label_matches() {
        let hits = filter_slash_commands("移交");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "/handoff");
        assert_eq!(hits[0].kind, SlashKind::Skill);
    }

    /// 无命中时返回空；重复调用结果稳定（纯函数、0 I/O）。
    #[test]
    fn filter_is_deterministic_and_empty_on_miss() {
        assert!(filter_slash_commands("zzz-not-a-command").is_empty());

        let first: Vec<&str> = filter_slash_commands("e").iter().map(|c| c.name).collect();
        let second: Vec<&str> = filter_slash_commands("e").iter().map(|c| c.name).collect();
        assert_eq!(first, second);
    }

    /// 过滤结果永不超过弹层 8 行上限。
    #[test]
    fn filter_never_exceeds_row_cap() {
        for query in ["", "/", "c", "e", "a", "n", "理", "x"] {
            assert!(
                filter_slash_commands(query).len() <= AUTOCOMPLETE_MAX_ROWS,
                "query={query} 超出 8 行上限"
            );
        }
    }

    /// `+` 菜单包含 §3.6 明列的 5 项上下文引用与 3 项快捷指令。
    #[test]
    fn plus_menu_covers_spec_items() {
        let context: Vec<&str> = plus_menu_items(PlusMenuSection::Context)
            .map(|item| item.label)
            .collect();
        assert_eq!(
            context,
            vec![
                "源码片段",
                "目录",
                "Git Diff",
                "最近终端输出",
                "图片或本地文件"
            ]
        );

        let shortcuts: Vec<&str> = plus_menu_items(PlusMenuSection::Shortcut)
            .map(|item| item.label)
            .collect();
        assert_eq!(shortcuts, vec!["/plan", "/review", "/check-all"]);
        assert_eq!(PLUS_MENU_ITEMS.len(), 8);
    }
}
