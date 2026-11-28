//! 时间线模块专属中文文案（M5-A）。
//!
//! **边界说明**：`crate::i18n`（`i18n.rs`）在本任务书中列为冻结文件，无法新增 key，
//! 因此时间线所需文案收敛为本模块的静态常量与纯格式化函数。语义唯一来源仍是
//! `design-system/soxu/pages/03-timeline-tools-and-checkpoints.md`；
//! `i18n.rs` 解冻后把这些 key 迁入全局文案表即可，调用点无需改动。
//!
//! 本模块只做字符串拼装（纯函数），不持有任何状态。

/// 时长格式：毫秒 → 一位小数秒（`400` → `0.4s`）。
pub fn duration(ms: u64) -> String {
    format!("{:.1}s", ms as f64 / 1000.0)
}

/// 只读探索聚合组折叠态摘要。
///
/// 设计基准：`已检索并读取 6 个文件 (aoci.code.txt, lib.rs...) · 0.4s`（03 §2 第 1 行）。
pub fn read_group_summary(file_count: usize, preview: &[String], duration_ms: u64) -> String {
    let head = format!("已检索并读取 {file_count} 个文件");
    if preview.is_empty() {
        format!("{head} · {}", duration(duration_ms))
    } else {
        format!(
            "{head} ({}) · {}",
            preview.join(", "),
            duration(duration_ms)
        )
    }
}

/// 文件编辑卡折叠态摘要：`修改 crates/soxu-magpie/src/lib.rs (+24 -5)`。
pub fn file_edit_summary(path: &str, additions: u32, deletions: u32) -> String {
    format!("修改 {path} (+{additions} -{deletions})")
}

/// 终端命令卡折叠态摘要：`cargo test --workspace · ✓ 退出码 0 · 2.1s`。
///
/// 退出码缺失（仍在运行）时用 `⟳ 运行中` 代替退出码段。
pub fn terminal_summary(command: &str, exit_code: Option<i32>, duration_ms: u64) -> String {
    match exit_code {
        Some(code) => format!(
            "{command} · {} 退出码 {code} · {}",
            if code == 0 { "✓" } else { "✗" },
            duration(duration_ms)
        ),
        None => format!("{command} · ⟳ 运行中 · {}", duration(duration_ms)),
    }
}

/// MCP 工具卡折叠态摘要：`aoci · aoci_maintain [Direct 工作区] · ✓ 0.3s`。
pub fn mcp_summary(server: &str, tool: &str, route: &str, duration_ms: u64) -> String {
    format!("{server} · {tool} [{route}] · ✓ {}", duration(duration_ms))
}

/// 子代理委派卡折叠态摘要：`委派 pi (deepseek-v4.1-flash) · 巡检文档链接 · ⟳ 运行中`。
pub fn subagent_summary(agent: &str, model: &str, intent: &str, running: bool) -> String {
    let tail = if running {
        "⟳ 运行中"
    } else {
        "✓ 已完成"
    };
    format!("委派 {agent} ({model}) · {intent} · {tail}")
}

/// Diff 截断提示：`展开剩余 128 行隐藏差异`（03 §6 第 3 行）。
pub fn diff_expand_label(hidden_lines: usize) -> String {
    format!("展开剩余 {hidden_lines} 行隐藏差异")
}

/// Diff 收起提示：`隐藏剩余 128 行`（03 §6 第 3 行降级交互）。
pub fn diff_collapse_label(hidden_lines: usize) -> String {
    format!("隐藏剩余 {hidden_lines} 行")
}

/// 只读活动组超限提示：`展开其余 N 条探索动作`。
pub fn activity_more_label(hidden_count: usize) -> String {
    format!("展开其余 {hidden_count} 条探索动作")
}

/// 本轮变更文件列表超限提示：`在右侧 Diff 栏查看全部 N 个文件`。
pub fn changed_files_more_label(total: usize) -> String {
    format!("在右侧 Diff 栏查看全部 {total} 个文件")
}

/// 终端运行中快捷操作：`终止命令`。
pub const TERMINAL_STOP: &str = "终止命令";

/// 审批卡标题：`需要人工授权 · Bash 命令执行`。
pub fn approval_title(tool_label: &str) -> String {
    format!("需要人工授权 · {tool_label}")
}

/// 审批卡工作区徽标：`工作区: feat/m2-cli`。
pub fn approval_workspace(branch: &str) -> String {
    format!("工作区: {branch}")
}

/// 高危红线审批卡风险横幅（绯红描边卡的必需文案，MASTER 铁律：语义色必须配文字）。
pub const REDLINE_BANNER: &str =
    "命中高危破坏性红线：该操作可能删除或覆写不可恢复的数据，请先确认影响范围与回退方式";

/// 审批卡三按钮文案（顺序即呈现顺序，03 §3）。
pub const APPROVAL_REJECT: &str = "拒绝并指导修改";
/// 常规「本会话允许」按钮文案。
pub const APPROVAL_ALLOW_SESSION: &str = "本会话允许同类只读/测试";
/// 主 CTA「允许一次」按钮文案（带 Enter 提示）。
pub const APPROVAL_ALLOW_ONCE: &str = "允许一次 ↵";

/// 拒绝并指导的输入条占位符。
pub const APPROVAL_GUIDANCE_PLACEHOLDER: &str =
    "输入拒绝原因与修改建议，Agent 将在同一轮内自我修正";

/// 跨设备幂等消解后的单行审计记录：`已批准执行 (由 iPhone 遥控端 确认 · 22:41:08)`。
pub fn approval_audit(action_label: &str, device: &str, clock: &str) -> String {
    format!("{action_label} (由 {device} 确认 · {clock})")
}

/// 提问卡进度头：`问题 1 / 2 · 需你确认架构选型`。
pub fn ask_progress(index: usize, total: usize, title: &str) -> String {
    format!("问题 {} / {total} · {title}", index + 1)
}

/// 提问卡推荐项徽标。
pub const ASK_RECOMMENDED: &str = "[推荐]";
/// 提问卡自由输入占位符。
pub const ASK_OTHER_PLACEHOLDER: &str = "或补充其他说明...";
/// 提问卡上一个按钮。
pub const ASK_PREV: &str = "上一题";
/// 提问卡下一个按钮。
pub const ASK_NEXT: &str = "下一题 →";
/// 提问卡最终提交按钮。
pub const ASK_SUBMIT: &str = "确认提交全部回答";

/// 检查点锚点横条：`检查点 #04 · commit 7c4e91a · 变更 4 个文件 (+86 -14)`。
pub fn checkpoint_label(
    ordinal: u32,
    short_commit: &str,
    files: u32,
    additions: u32,
    deletions: u32,
) -> String {
    format!(
        "检查点 #{ordinal:02} · commit {short_commit} · 变更 {files} 个文件 (+{additions} -{deletions})"
    )
}

/// 检查点锚点操作按钮。
pub const CHECKPOINT_VIEW_DIFF: &str = "查看本轮 Diff";
/// 检查点回退按钮。
pub const CHECKPOINT_REWIND: &str = "回退至此轮 ↺";

/// 回退三选一确认框标题（绝不静默覆写工作树，03 §4）。
pub const REWIND_CONFIRM_TITLE: &str = "选择回退方式（不会静默覆写工作树）";

/// `LiveSteps` 默认折叠的单行焦点微条：`实时步骤焦点 · 2/4 · 等待人工确认安全门禁`。
pub fn live_steps_label(done: usize, total: usize, current: &str) -> String {
    if total == 0 {
        return "实时步骤焦点 · 0/0 · 暂无步骤".to_string();
    }
    if current.is_empty() {
        format!("实时步骤焦点 · {done}/{total}")
    } else {
        format!("实时步骤焦点 · {done}/{total} · {current}")
    }
}

/// 导航轨悬停气泡的轮次标题：`第 4 轮`。
pub fn rail_turn_label(index: usize) -> String {
    format!("第 {} 轮", index + 1)
}

/// 中栏空态文案（与 `crate::i18n` 的 `shell.panel.canvas_empty` 同义）。
///
/// 帧内不构造 [`crate::i18n::I18n`]（每次都会建两张 HashMap），因此在此以静态常量
/// 提供同一文案；`i18n.rs` 解冻后改用 `I18n::t("shell.panel.canvas_empty")` 即可。
pub const CANVAS_EMPTY: &str = "要做点什么？";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_formats_one_decimal_second() {
        assert_eq!(duration(0), "0.0s");
        assert_eq!(duration(400), "0.4s");
        assert_eq!(duration(2100), "2.1s");
    }

    #[test]
    fn read_group_summary_matches_design_wording() {
        let preview = vec!["aoci.code.txt".to_string(), "lib.rs".to_string()];
        assert_eq!(
            read_group_summary(6, &preview, 400),
            "已检索并读取 6 个文件 (aoci.code.txt, lib.rs) · 0.4s"
        );
        // 无预览路径时不渲染空括号。
        assert_eq!(
            read_group_summary(0, &[], 120),
            "已检索并读取 0 个文件 · 0.1s"
        );
    }

    #[test]
    fn terminal_summary_covers_three_states() {
        assert_eq!(
            terminal_summary("cargo test --workspace", Some(0), 2100),
            "cargo test --workspace · ✓ 退出码 0 · 2.1s"
        );
        assert_eq!(
            terminal_summary("cargo clippy", Some(101), 900),
            "cargo clippy · ✗ 退出码 101 · 0.9s"
        );
        assert_eq!(
            terminal_summary("pnpm build", None, 300),
            "pnpm build · ⟳ 运行中 · 0.3s"
        );
    }

    #[test]
    fn truncation_labels_use_design_wording() {
        assert_eq!(diff_expand_label(128), "展开剩余 128 行隐藏差异");
        assert_eq!(diff_collapse_label(128), "隐藏剩余 128 行");
        assert_eq!(activity_more_label(2), "展开其余 2 条探索动作");
        assert_eq!(
            changed_files_more_label(12),
            "在右侧 Diff 栏查看全部 12 个文件"
        );
    }

    #[test]
    fn checkpoint_label_zero_pads_ordinal() {
        assert_eq!(
            checkpoint_label(4, "7c4e91a", 4, 86, 14),
            "检查点 #04 · commit 7c4e91a · 变更 4 个文件 (+86 -14)"
        );
    }

    #[test]
    fn live_steps_label_handles_empty_and_blocked() {
        assert_eq!(live_steps_label(0, 0, ""), "实时步骤焦点 · 0/0 · 暂无步骤");
        assert_eq!(live_steps_label(2, 4, ""), "实时步骤焦点 · 2/4");
        assert_eq!(
            live_steps_label(2, 4, "等待人工确认安全门禁"),
            "实时步骤焦点 · 2/4 · 等待人工确认安全门禁"
        );
    }

    #[test]
    fn approval_and_ask_labels_are_stable() {
        assert_eq!(
            approval_title("Bash 命令执行"),
            "需要人工授权 · Bash 命令执行"
        );
        assert_eq!(approval_workspace("feat/m2-cli"), "工作区: feat/m2-cli");
        assert_eq!(
            approval_audit("已批准执行", "iPhone 遥控端", "22:41:08"),
            "已批准执行 (由 iPhone 遥控端 确认 · 22:41:08)"
        );
        assert_eq!(
            ask_progress(0, 2, "需你确认架构选型"),
            "问题 1 / 2 · 需你确认架构选型"
        );
        assert_eq!(rail_turn_label(3), "第 4 轮");
    }
}
