//! 权限审批卡与结构化提问卡：风险分级、默认焦点、禁用态与跨设备幂等消解（纯逻辑）。
//!
//! 设计基准：`pages/03-timeline-tools-and-checkpoints.md` §3（权限审批）、§5（提问卡）、
//! §6 第 4/5 行（常规 vs 高危红线对比矩阵）。
//!
//! 不可违反的三条硬规则：
//! 1. **禁止左侧加宽色条**：审批卡与提问卡一律四边等宽 `1px` 细描边（本模块只产出
//!    描边令牌，不产出「左侧色带」任何几何）；
//! 2. **高危红线自动禁用「本会话允许」**，且默认键盘焦点停在「拒绝并指导修改」上，
//!    确认键不直接批准；
//! 3. **跨设备幂等**：桌面端与手机 PWA 共享同一 `approval_id` 状态，先到的一票生效，
//!    后续决议一律幂等忽略（防止两端重复点击冲突）。

use crate::theme::tokens::color::dark;
use crate::theme::tokens::color::light;
use crate::theme::tokens::layout;
use crate::timeline::model::{RowId, RowTone, TimelineRow, TimelineRowKind};
use crate::timeline::text;

/// 审批卡详情区最大高度（px）：`sizing.permissionDetailMaxHeight = 96`。
pub const DETAIL_MAX_HEIGHT_PX: f32 = crate::tokens::sizing::PERMISSION_DETAIL_MAX_HEIGHT_PX;

/// 触控下限（px）：44px 热区纪律。
pub const TOUCH_TARGET_MIN_PX: f32 = layout::TOUCH_TARGET_MIN_PX;

/// 审批风险等级。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApprovalRisk {
    /// 常规写外围 / 网络 / 普通命令审批（琥珀等宽描边）。
    Routine,
    /// 命中高危破坏性红线（绯红等宽描边 + 禁用「本会话允许」）。
    DestructiveRedline,
}

impl ApprovalRisk {
    /// 卡片描边令牌（深色模式）：四边等宽 `1px`。
    pub fn dark_border_token(self) -> u32 {
        match self {
            Self::Routine => dark::SEMANTIC_WARNING_BORDER,
            Self::DestructiveRedline => dark::SEMANTIC_DANGER_BORDER,
        }
    }

    /// 卡片描边令牌（浅色模式）。
    pub fn light_border_token(self) -> u32 {
        match self {
            Self::Routine => light::SEMANTIC_WARNING_BORDER,
            Self::DestructiveRedline => light::SEMANTIC_DANGER_BORDER,
        }
    }

    /// 是否为高危红线。
    pub fn is_redline(self) -> bool {
        matches!(self, Self::DestructiveRedline)
    }

    /// 风险横幅文案（`None` 表示常规审批无横幅）。
    pub fn banner(self) -> Option<&'static str> {
        self.is_redline().then_some(text::REDLINE_BANNER)
    }
}

/// 审批动作。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApprovalAction {
    /// 拒绝并指导修改（展开单行输入条）。
    RejectWithGuidance,
    /// 本会话允许同类只读/测试（高危红线禁用）。
    AllowSession,
    /// 允许一次（纯白主 CTA）。
    AllowOnce,
}

impl ApprovalAction {
    /// 全部动作（顺序即呈现顺序）。
    pub const ALL: [Self; 3] = [
        Self::RejectWithGuidance,
        Self::AllowSession,
        Self::AllowOnce,
    ];

    /// 按钮文案。
    pub const fn label(self) -> &'static str {
        match self {
            Self::RejectWithGuidance => text::APPROVAL_REJECT,
            Self::AllowSession => text::APPROVAL_ALLOW_SESSION,
            Self::AllowOnce => text::APPROVAL_ALLOW_ONCE,
        }
    }

    /// 是否为纯白主 CTA。
    pub const fn is_primary(self) -> bool {
        matches!(self, Self::AllowOnce)
    }
}

/// 决议来源（区分本机点击与手机遥控端点击）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApprovalSource {
    /// 桌面端本机。
    Local,
    /// 已配对手机遥控端（携带设备名）。
    Remote(String),
}

impl ApprovalSource {
    /// 审计记录里的设备描述。
    pub fn device_label(&self) -> &str {
        match self {
            Self::Local => "桌面端",
            Self::Remote(device) => device,
        }
    }
}

/// 一次决议的结果。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResolveOutcome {
    /// 本次决议被采纳。
    Applied,
    /// 已被其它端决议过：幂等忽略，不改变状态。
    AlreadyResolved,
    /// 该动作在当前风险等级下被禁用（高危红线的「本会话允许」）。
    Disabled,
    /// 拒绝必须附带指导意见才能生效。
    GuidanceRequired,
}

/// 已生效的决议。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolution {
    /// 采纳的动作。
    pub action: ApprovalAction,
    /// 决议来源。
    pub source: ApprovalSource,
}

/// 权限审批卡状态。
#[derive(Clone, Debug)]
pub struct PermissionGate {
    /// 审批 ID（桌面端与所有配对端共享同一标识）。
    pub approval_id: String,
    /// 风险等级。
    pub risk: ApprovalRisk,
    /// 工具标签（如 `Bash 命令执行`）。
    pub tool_label: String,
    /// 目标工作区分支。
    pub branch: String,
    /// 待执行命令/操作原文。
    pub command: String,
    /// 风险解释行（命中规则与影响范围）。
    pub warning: String,
    /// 已生效决议（`None` 表示待决）。
    resolved: Option<Resolution>,
    /// 「拒绝并指导」输入区是否展开。
    guidance_open: bool,
    /// 输入的指导意见。
    guidance: String,
}

impl PermissionGate {
    /// 构造待决审批卡。
    pub fn new(
        approval_id: impl Into<String>,
        risk: ApprovalRisk,
        tool_label: impl Into<String>,
        branch: impl Into<String>,
        command: impl Into<String>,
        warning: impl Into<String>,
    ) -> Self {
        Self {
            approval_id: approval_id.into(),
            risk,
            tool_label: tool_label.into(),
            branch: branch.into(),
            command: command.into(),
            warning: warning.into(),
            resolved: None,
            guidance_open: false,
            guidance: String::new(),
        }
    }

    /// 标题：`需要人工授权 · Bash 命令执行`。
    pub fn title(&self) -> String {
        text::approval_title(&self.tool_label)
    }

    /// 工作区徽标：`工作区: feat/m2-cli`。
    pub fn workspace_badge(&self) -> String {
        text::approval_workspace(&self.branch)
    }

    /// 当前可点击的动作（高危红线剔除「本会话允许」，其余保持设计顺序）。
    pub fn actions(&self) -> &'static [ApprovalAction] {
        if self.risk.is_redline() {
            &[
                ApprovalAction::RejectWithGuidance,
                ApprovalAction::AllowOnce,
            ]
        } else {
            &ApprovalAction::ALL
        }
    }

    /// 「本会话允许」是否可用（高危红线恒为 `false`）。
    pub fn session_allow_enabled(&self) -> bool {
        !self.risk.is_redline()
    }

    /// 默认键盘焦点：
    /// - 常规审批 → 「允许一次」（设计：Enter 直接确认主 CTA）；
    /// - 高危红线 → 「拒绝并指导修改」（强制人工审慎确认）。
    pub fn default_focus(&self) -> ApprovalAction {
        if self.risk.is_redline() {
            ApprovalAction::RejectWithGuidance
        } else {
            ApprovalAction::AllowOnce
        }
    }

    /// 当前焦点动作（未决时取默认焦点，已决时锁定到已采纳动作）。
    pub fn focused_action(&self) -> ApprovalAction {
        match &self.resolved {
            Some(resolution) => resolution.action,
            None => self.default_focus(),
        }
    }

    /// Enter / 确认键落到的动作。
    ///
    /// 高危红线**不得**让确认键直接产出批准：此处返回「拒绝并指导修改」，
    /// 与设计「确认键改为绯红警告按钮」一致。
    pub fn enter_action(&self) -> ApprovalAction {
        self.default_focus()
    }

    /// 卡片描边令牌（按明暗模式解析）。
    pub fn border_token(&self, is_dark: bool) -> u32 {
        if is_dark {
            self.risk.dark_border_token()
        } else {
            self.risk.light_border_token()
        }
    }

    /// 风险横幅。
    pub fn banner(&self) -> Option<&'static str> {
        self.risk.banner()
    }

    /// 是否已决议。
    pub fn is_resolved(&self) -> bool {
        self.resolved.is_some()
    }

    /// 已生效决议。
    pub fn resolution(&self) -> Option<&Resolution> {
        self.resolved.as_ref()
    }

    /// 「拒绝并指导」输入区是否展开。
    pub fn guidance_open(&self) -> bool {
        self.guidance_open
    }

    /// 当前指导意见。
    pub fn guidance(&self) -> &str {
        &self.guidance
    }

    /// 写入指导意见。
    pub fn set_guidance(&mut self, guidance: impl Into<String>) {
        self.guidance = guidance.into();
    }

    /// 提交一条决议。
    ///
    /// - 已决议 → [`ResolveOutcome::AlreadyResolved`]（跨设备幂等，先到先得）；
    /// - 高危红线 + 本会话允许 → [`ResolveOutcome::Disabled`]（按钮已禁用，服务端同样拒绝）；
    /// - 拒绝但指导意见为空 → [`ResolveOutcome::GuidanceRequired`] 并展开输入条；
    /// - 其余 → 采纳并冻结状态。
    pub fn resolve(&mut self, action: ApprovalAction, source: ApprovalSource) -> ResolveOutcome {
        if self.is_resolved() {
            return ResolveOutcome::AlreadyResolved;
        }
        if action == ApprovalAction::AllowSession && !self.session_allow_enabled() {
            return ResolveOutcome::Disabled;
        }
        if action == ApprovalAction::RejectWithGuidance && self.guidance.trim().is_empty() {
            self.guidance_open = true;
            return ResolveOutcome::GuidanceRequired;
        }
        self.resolved = Some(Resolution { action, source });
        self.guidance_open = false;
        ResolveOutcome::Applied
    }

    /// 决议后的单行审计记录（卡片收拢为一行，03 §3 第 3 条）。
    pub fn audit_line(&self, clock: &str) -> Option<String> {
        let resolution = self.resolved.as_ref()?;
        let action = match resolution.action {
            ApprovalAction::AllowOnce => "已批准执行",
            ApprovalAction::AllowSession => "已允许本会话同类操作",
            ApprovalAction::RejectWithGuidance => "已拒绝并提交修改指引",
        };
        Some(text::approval_audit(
            action,
            resolution.source.device_label(),
            clock,
        ))
    }

    /// 转为对话流行：高危红线 → 绯红，常规审批 → 琥珀；未决时默认展开。
    pub fn row(&self, id: RowId) -> TimelineRow {
        let tone = if self.risk.is_redline() {
            RowTone::Danger
        } else {
            RowTone::NeedsInput
        };
        TimelineRow::new(id, TimelineRowKind::ApprovalGate, self.title(), 0)
            .with_subtitle(self.command.clone())
            .expanded(!self.is_resolved())
            .with_tone(tone)
    }
}

/// 提问卡单个选项。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AskOption {
    /// 选项文案。
    pub label: String,
    /// 是否为首行推荐项（翠绿微标 `[推荐]`）。
    pub recommended: bool,
}

impl AskOption {
    /// 构造选项。
    pub fn new(label: impl Into<String>, recommended: bool) -> Self {
        Self {
            label: label.into(),
            recommended,
        }
    }
}

/// 提问卡单个问题。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AskQuestion {
    /// 问题标题（进度头右侧）。
    pub title: String,
    /// 提问正文。
    pub prompt: String,
    /// 选项列表。
    pub options: Vec<AskOption>,
    /// 是否多选。
    pub multi: bool,
}

impl AskQuestion {
    /// 构造单选题。
    pub fn single(
        title: impl Into<String>,
        prompt: impl Into<String>,
        options: Vec<AskOption>,
    ) -> Self {
        Self {
            title: title.into(),
            prompt: prompt.into(),
            options,
            multi: false,
        }
    }

    /// 构造多选题。
    pub fn multiple(
        title: impl Into<String>,
        prompt: impl Into<String>,
        options: Vec<AskOption>,
    ) -> Self {
        Self {
            title: title.into(),
            prompt: prompt.into(),
            options,
            multi: true,
        }
    }

    /// 该选项的键盘数字快选微标（`1-9`；超过 9 项无快选）。
    pub fn digit_shortcut(index: usize) -> Option<char> {
        match index {
            0..=8 => char::from_digit(index as u32 + 1, 10),
            _ => None,
        }
    }
}

/// 结构化提问卡（`PendingUserInput`）状态。
#[derive(Clone, Debug)]
pub struct AskCard {
    /// 全部问题。
    pub questions: Vec<AskQuestion>,
    /// 当前问题下标。
    current: usize,
    /// 每题已选选项下标（多选可有多个）。
    selections: Vec<Vec<usize>>,
    /// 每题的自由补充输入。
    free_text: Vec<String>,
}

impl AskCard {
    /// 构造提问卡（全部题目初始未答）。
    pub fn new(questions: Vec<AskQuestion>) -> Self {
        let count = questions.len();
        Self {
            questions,
            current: 0,
            selections: vec![Vec::new(); count],
            free_text: vec![String::new(); count],
        }
    }

    /// 当前问题下标。
    pub fn current(&self) -> usize {
        self.current
    }

    /// 问题总数。
    pub fn total(&self) -> usize {
        self.questions.len()
    }

    /// 进度头文案：`问题 1 / 2 · 需你确认架构选型`。
    pub fn progress_label(&self) -> String {
        match self.questions.get(self.current) {
            Some(question) => text::ask_progress(self.current, self.total(), &question.title),
            None => text::ask_progress(0, self.total(), ""),
        }
    }

    /// 当前问题。
    pub fn current_question(&self) -> Option<&AskQuestion> {
        self.questions.get(self.current)
    }

    /// 当前题的已选下标。
    pub fn selection(&self) -> &[usize] {
        self.selections
            .get(self.current)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// 当前题的自由补充输入。
    pub fn free_text(&self) -> &str {
        self.free_text
            .get(self.current)
            .map(String::as_str)
            .unwrap_or("")
    }

    /// 写入当前题的自由补充输入。
    pub fn set_free_text(&mut self, value: impl Into<String>) {
        if let Some(slot) = self.free_text.get_mut(self.current) {
            *slot = value.into();
        }
    }

    /// 选择当前题的第 `index` 个选项。
    ///
    /// 单选替换原选择；多选切换；越界忽略。
    pub fn select(&mut self, index: usize) -> bool {
        let Some(question) = self.questions.get(self.current) else {
            return false;
        };
        if index >= question.options.len() {
            return false;
        }
        let multi = question.multi;
        let Some(slot) = self.selections.get_mut(self.current) else {
            return false;
        };
        if multi {
            match slot.iter().position(|selected| *selected == index) {
                Some(pos) => {
                    slot.remove(pos);
                }
                None => slot.push(index),
            }
        } else {
            slot.clear();
            slot.push(index);
        }
        true
    }

    /// 数字快选：`'1'..='9'` 映射到选项下标。
    pub fn select_digit(&mut self, digit: char) -> bool {
        match digit.to_digit(10) {
            Some(value) if (1..=9).contains(&value) => self.select(value as usize - 1),
            _ => false,
        }
    }

    /// 切到下一题；已是末题返回 `false`。
    pub fn next_question(&mut self) -> bool {
        if self.current + 1 >= self.questions.len() {
            return false;
        }
        self.current += 1;
        true
    }

    /// 切到上一题；已是首题返回 `false`。
    pub fn prev_question(&mut self) -> bool {
        if self.current == 0 {
            return false;
        }
        self.current -= 1;
        true
    }

    /// 某题是否已作答（选中至少一项，或填写了自由补充）。
    pub fn is_answered(&self, index: usize) -> bool {
        let has_choice = self
            .selections
            .get(index)
            .map(|slot| !slot.is_empty())
            .unwrap_or(false);
        let has_text = self
            .free_text
            .get(index)
            .map(|text| !text.trim().is_empty())
            .unwrap_or(false);
        has_choice || has_text
    }

    /// 是否全部题目已作答（决定「确认提交全部回答」是否可用）。
    pub fn can_submit(&self) -> bool {
        (0..self.questions.len()).all(|index| self.is_answered(index))
    }

    /// 汇总全部回答：`(问题下标, 选中文本, 自由补充)`。
    pub fn answers(&self) -> Vec<(usize, Vec<String>, String)> {
        self.questions
            .iter()
            .enumerate()
            .map(|(index, question)| {
                let picked = self
                    .selections
                    .get(index)
                    .map(|slot| {
                        slot.iter()
                            .filter_map(|option_index| {
                                question.options.get(*option_index).map(|o| o.label.clone())
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let free = self.free_text.get(index).cloned().unwrap_or_default();
                (index, picked, free)
            })
            .collect()
    }

    /// 触控/鼠标下限高度（44px 热区纪律）。
    pub fn option_min_height_px() -> f32 {
        TOUCH_TARGET_MIN_PX
    }

    /// 转为对话流行（标题即进度头，正文行数 = 当前题选项数）。
    pub fn row(&self, id: RowId) -> TimelineRow {
        let options = self
            .current_question()
            .map(|question| question.options.len())
            .unwrap_or(0);
        TimelineRow::new(
            id,
            TimelineRowKind::AskQuestion,
            self.progress_label(),
            options,
        )
        .expanded(true)
        .with_tone(RowTone::NeedsInput)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn routine() -> PermissionGate {
        PermissionGate::new(
            "approval-1",
            ApprovalRisk::Routine,
            "Bash 命令执行",
            "feat/m2-cli",
            "cargo clippy -p soxu-desktop",
            "命中普通命令审批",
        )
    }

    fn redline() -> PermissionGate {
        PermissionGate::new(
            "approval-2",
            ApprovalRisk::DestructiveRedline,
            "Bash 命令执行",
            "feat/m2-cli",
            "git clean -fdX temp-fixtures/",
            "命中文件清理规则：将删除受忽略的临时测试产物",
        )
    }

    #[test]
    fn routine_card_enables_all_three_actions() {
        let gate = routine();
        assert_eq!(gate.actions().len(), 3);
        assert!(gate.session_allow_enabled());
        assert_eq!(gate.default_focus(), ApprovalAction::AllowOnce);
        assert_eq!(gate.enter_action(), ApprovalAction::AllowOnce);
        assert_eq!(gate.banner(), None);
        assert_eq!(gate.border_token(true), dark::SEMANTIC_WARNING_BORDER);
        assert_eq!(gate.border_token(false), light::SEMANTIC_WARNING_BORDER);
        assert_eq!(gate.title(), "需要人工授权 · Bash 命令执行");
        assert_eq!(gate.workspace_badge(), "工作区: feat/m2-cli");
        assert_eq!(DETAIL_MAX_HEIGHT_PX, 96.0);
        assert_eq!(TOUCH_TARGET_MIN_PX, 44.0);
    }

    #[test]
    fn redline_disables_session_allow_and_defaults_to_reject() {
        let mut gate = redline();
        // 高危红线：只暴露「拒绝并指导」与「允许一次」。
        assert_eq!(
            gate.actions(),
            &[
                ApprovalAction::RejectWithGuidance,
                ApprovalAction::AllowOnce
            ]
        );
        assert!(!gate.session_allow_enabled());
        // 默认焦点与确认键都不得落在批准上。
        assert_eq!(gate.default_focus(), ApprovalAction::RejectWithGuidance);
        assert_eq!(gate.enter_action(), ApprovalAction::RejectWithGuidance);
        assert_eq!(gate.focused_action(), ApprovalAction::RejectWithGuidance);
        // 服务端同样拒绝被禁用的动作，且状态不变。
        assert_eq!(
            gate.resolve(ApprovalAction::AllowSession, ApprovalSource::Local),
            ResolveOutcome::Disabled
        );
        assert!(!gate.is_resolved());
        // 绯红等宽描边 + 风险横幅。
        assert_eq!(gate.border_token(true), dark::SEMANTIC_DANGER_BORDER);
        assert_eq!(gate.border_token(false), light::SEMANTIC_DANGER_BORDER);
        assert_eq!(gate.banner(), Some(text::REDLINE_BANNER));
        assert!(ApprovalRisk::Routine.banner().is_none());
        assert_eq!(ApprovalAction::AllowOnce.label(), "允许一次 ↵");
        assert!(ApprovalAction::AllowOnce.is_primary());
        assert!(!ApprovalAction::AllowSession.is_primary());
    }

    #[test]
    fn reject_requires_guidance_before_taking_effect() {
        let mut gate = routine();
        assert_eq!(
            gate.resolve(
                ApprovalAction::RejectWithGuidance,
                ApprovalSource::Remote("iPhone 遥控端".to_string())
            ),
            ResolveOutcome::GuidanceRequired
        );
        assert!(gate.guidance_open());
        assert!(!gate.is_resolved());

        gate.set_guidance("改用 tempfile::TempDir，不要清理真实目录");
        assert_eq!(
            gate.resolve(
                ApprovalAction::RejectWithGuidance,
                ApprovalSource::Remote("iPhone 遥控端".to_string())
            ),
            ResolveOutcome::Applied
        );
        assert!(!gate.guidance_open());
        assert_eq!(
            gate.audit_line("22:41:08").as_deref(),
            Some("已拒绝并提交修改指引 (由 iPhone 遥控端 确认 · 22:41:08)")
        );
    }

    #[test]
    fn cross_device_resolution_is_first_wins_and_idempotent() {
        let mut gate = routine();
        assert_eq!(
            gate.resolve(
                ApprovalAction::AllowOnce,
                ApprovalSource::Remote("iPhone 遥控端".to_string())
            ),
            ResolveOutcome::Applied
        );
        // 桌面端随后点击：幂等忽略，不覆盖先到决议。
        assert_eq!(
            gate.resolve(ApprovalAction::AllowOnce, ApprovalSource::Local),
            ResolveOutcome::AlreadyResolved
        );
        assert_eq!(
            gate.resolve(
                ApprovalAction::RejectWithGuidance,
                ApprovalSource::Remote("Android 遥控端".to_string())
            ),
            ResolveOutcome::AlreadyResolved
        );
        let resolution = gate.resolution().cloned();
        assert_eq!(
            resolution,
            Some(Resolution {
                action: ApprovalAction::AllowOnce,
                source: ApprovalSource::Remote("iPhone 遥控端".to_string()),
            })
        );
        assert_eq!(
            gate.audit_line("22:41:08").as_deref(),
            Some("已批准执行 (由 iPhone 遥控端 确认 · 22:41:08)")
        );
        // 已决议后焦点锁定在已采纳动作上。
        assert_eq!(gate.focused_action(), ApprovalAction::AllowOnce);
        // 未决议时没有审计行。
        assert_eq!(routine().audit_line("00:00:00"), None);
    }

    #[test]
    fn session_allow_on_routine_card_is_accepted() {
        let mut gate = routine();
        assert_eq!(
            gate.resolve(ApprovalAction::AllowSession, ApprovalSource::Local),
            ResolveOutcome::Applied
        );
        assert_eq!(
            gate.audit_line("09:00:00").as_deref(),
            Some("已允许本会话同类操作 (由 桌面端 确认 · 09:00:00)")
        );
    }

    fn two_options() -> Vec<AskOption> {
        vec![
            AskOption::new("A：独立 timeline 子目录", true),
            AskOption::new("B：并入 shell.rs", false),
            AskOption::new("C：新建 crate", false),
        ]
    }

    #[test]
    fn ask_card_digit_shortcuts_and_single_select() {
        let mut card = AskCard::new(vec![AskQuestion::single(
            "需你确认架构选型",
            "时间线视图放在哪里？",
            two_options(),
        )]);
        assert_eq!(card.progress_label(), "问题 1 / 1 · 需你确认架构选型");
        // 数字快选 1-9。
        assert!(card.select_digit('1'));
        assert_eq!(card.selection(), &[0]);
        assert!(card.select_digit('3'));
        assert_eq!(card.selection(), &[2], "单选必须替换旧选择");
        // 0 与越界忽略。
        assert!(!card.select_digit('0'));
        assert!(!card.select_digit('x'));
        assert!(!card.select(9));
        assert_eq!(card.selection(), &[2]);
        assert_eq!(AskQuestion::digit_shortcut(0), Some('1'));
        assert_eq!(AskQuestion::digit_shortcut(8), Some('9'));
        assert_eq!(AskQuestion::digit_shortcut(9), None);
        assert!(card.can_submit());
    }

    #[test]
    fn ask_card_multi_select_and_navigation_bounds() {
        let mut card = AskCard::new(vec![
            AskQuestion::multiple("可选多项", "选出所有适用项", two_options()),
            AskQuestion::single("第二题", "继续", two_options()),
        ]);
        // 多选切换。
        assert!(card.select(0));
        assert!(card.select(2));
        assert_eq!(card.selection(), &[0, 2]);
        assert!(card.select(0));
        assert_eq!(card.selection(), &[2]);

        // 未答完不得提交。
        assert!(!card.can_submit());
        // 上一题边界。
        assert!(!card.prev_question());
        assert!(card.next_question());
        assert_eq!(card.current(), 1);
        assert_eq!(card.progress_label(), "问题 2 / 2 · 第二题");
        // 末题边界。
        assert!(!card.next_question());

        // 自由补充也算作答。
        card.set_free_text("补充：需要与 M5-B 对齐");
        assert!(card.can_submit());

        let answers = card.answers();
        assert_eq!(answers.len(), 2);
        assert_eq!(answers[0].1, vec!["C：新建 crate".to_string()]);
        assert_eq!(answers[1].2, "补充：需要与 M5-B 对齐");
        assert_eq!(AskCard::option_min_height_px(), 44.0);
    }

    #[test]
    fn gates_project_into_timeline_rows() {
        let routine_row = routine().row(1);
        assert_eq!(routine_row.kind, TimelineRowKind::ApprovalGate);
        assert_eq!(routine_row.tone, RowTone::NeedsInput);
        assert!(routine_row.expanded, "未决审批卡默认展开");
        assert_eq!(
            routine_row.subtitle.as_deref(),
            Some("cargo clippy -p soxu-desktop")
        );
        // 高危红线 → 绯红行。
        assert_eq!(redline().row(2).tone, RowTone::Danger);
        // 已决议 → 收拢成单行（不再默认展开）。
        let mut resolved = routine();
        resolved.resolve(ApprovalAction::AllowOnce, ApprovalSource::Local);
        assert!(!resolved.row(3).expanded);

        // 提问卡：琥珀 + 展开 + 正文行数 = 选项数。
        let card = AskCard::new(vec![AskQuestion::single(
            "需你确认架构选型",
            "放哪里？",
            two_options(),
        )]);
        let row = card.row(4);
        assert_eq!(row.kind, TimelineRowKind::AskQuestion);
        assert_eq!(row.tone, RowTone::NeedsInput);
        assert_eq!(row.body_lines, 3);
        assert!(row.expanded);
    }

    #[test]
    fn empty_ask_card_is_safe() {
        let mut card = AskCard::new(Vec::new());
        assert_eq!(card.total(), 0);
        assert_eq!(card.current_question(), None);
        assert!(!card.next_question());
        assert!(!card.prev_question());
        assert!(!card.select(0));
        assert!(!card.select_digit('1'));
        assert!(card.selection().is_empty());
        assert!(card.free_text().is_empty());
        assert!(card.can_submit());
        assert!(card.answers().is_empty());
        let _ = card.progress_label();
    }
}
