//! 调度中心纯内存状态机与视图占位（M6-C）。
//!
//! 桌面层不持有数据库、磁盘、时钟或 RPC 客户端；外层宿主把协议摘要喂给
//! [`ScheduleCenterState::replace_schedules`]，用户操作生成纯意图，再由 RPC 层提交。
//! 这里约束列表/新建/停用/删除/运行结果呈现状态，避免视图组件自行维护多份派生状态。
//!
//! 触控/鼠标热区至少 44px（消费 `tokens::sizing::TOUCH_HIT_MIN_PX`），焦点通过
//! 既有 `FocusStack` 的 `ReviewDiff` 检视上下文复用；打开时单一压入，关闭时回到
//! 打开前上下文，模态锁定时拒绝打开并保留原焦点。

pub mod view;

use soxu_protocol::{
    ScheduleId, ScheduleRunOutcome, ScheduleRunResult, ScheduleState, ScheduleSummary,
};
use std::collections::BTreeMap;

use crate::focus::{FocusDecision, FocusLayer, FocusStack};
use crate::tokens::sizing;

/// 所有调度中心触控/鼠标热区的最小边长（44px）。
pub const SCHEDULE_HIT_TARGET_MIN_PX: f32 = sizing::TOUCH_HIT_MIN_PX;

// 编译期锁定热区下限：设计令牌回退到 44px 以下时直接编译失败。
const _: () = assert!(SCHEDULE_HIT_TARGET_MIN_PX >= 44.0);

/// 占位列表显示的最大定义数（滚动由后续真实 GPUI 列表承载）。
pub const SCHEDULE_CENTER_MAX_VISIBLE: usize = 8;

/// 新建表单草稿：尚未提交 RPC。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScheduleDraft {
    pub name: String,
    pub cadence: String,
    pub agent_id: String,
    pub prompt: String,
}

/// 用户对某条调度提出的意图；RPC 由宿主执行并回写新摘要。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScheduleIntent {
    Pause {
        schedule_id: ScheduleId,
        paused: bool,
    },
    Delete {
        schedule_id: ScheduleId,
    },
    TriggerNow {
        schedule_id: ScheduleId,
    },
}

/// 调度中心的纯内存状态机。
#[derive(Debug, Clone, Default)]
pub struct ScheduleCenterState {
    schedules: BTreeMap<ScheduleId, ScheduleSummary>,
    selected: Option<ScheduleId>,
    draft: Option<ScheduleDraft>,
    recent_results: BTreeMap<ScheduleId, ScheduleRunResult>,
    last_error: Option<String>,
    open: bool,
    focus: FocusStack,
    focus_before_open: Option<FocusLayer>,
}

impl ScheduleCenterState {
    /// 以外部快照完整替换列表，删去已经不存在的选择和运行结果。
    pub fn replace_schedules(&mut self, schedules: impl IntoIterator<Item = ScheduleSummary>) {
        self.schedules = schedules
            .into_iter()
            .map(|summary| (summary.schedule_id.clone(), summary))
            .collect();
        if self
            .selected
            .as_ref()
            .is_some_and(|id| !self.schedules.contains_key(id))
        {
            self.selected = None;
        }
        self.recent_results
            .retain(|id, _| self.schedules.contains_key(id));
    }

    /// 是否打开。
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// 当前焦点上下文（空栈时为工作台全局）。
    #[must_use]
    pub fn effective_focus_context(&self) -> &'static str {
        self.focus.effective_context()
    }

    /// 当前选中条目。
    #[must_use]
    pub fn selected(&self) -> Option<&ScheduleSummary> {
        self.selected.as_ref().and_then(|id| self.schedules.get(id))
    }

    /// 条目列表按最近更新时间倒序、标识稳定排序。
    #[must_use]
    pub fn schedules(&self) -> Vec<&ScheduleSummary> {
        let mut schedules: Vec<_> = self.schedules.values().collect();
        schedules.sort_by(|left, right| {
            right
                .updated_at
                .cmp(&left.updated_at)
                .then_with(|| left.schedule_id.cmp(&right.schedule_id))
        });
        schedules
    }

    /// 是否还需要滚动呈现隐藏条目。
    #[must_use]
    pub fn has_overflow(&self) -> bool {
        self.schedules.len() > SCHEDULE_CENTER_MAX_VISIBLE
    }

    /// 最近运行结果。
    #[must_use]
    pub fn recent_result(&self, id: &ScheduleId) -> Option<&ScheduleRunResult> {
        self.recent_results.get(id)
    }

    /// 最近错误摘要（例如 RPC 拒绝原因）。
    #[must_use]
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// 打开调度中心并复用 ReviewDiff 焦点层。模态锁定时返回 false。
    pub fn open(&mut self) -> bool {
        if self.open {
            return true;
        }
        self.focus_before_open = self.focus.active();
        if self.focus.push(FocusLayer::ReviewDiff) != FocusDecision::Allowed {
            self.focus_before_open = None;
            return false;
        }
        self.open = true;
        true
    }

    /// 关闭调度中心并回到打开前焦点。
    pub fn close(&mut self) {
        if !self.open {
            return;
        }
        self.focus.pop_to(FocusLayer::ReviewDiff);
        self.focus.close_active();
        if let Some(previous) = self.focus_before_open.take()
            && self.focus.active() != Some(previous)
        {
            let _ = self.focus.push(previous);
        }
        self.open = false;
    }

    /// 选中定义；未知标识不改变当前选择。
    pub fn select(&mut self, id: &ScheduleId) -> bool {
        if !self.schedules.contains_key(id) {
            return false;
        }
        self.selected = Some(id.clone());
        true
    }

    /// 开始新建草稿。
    pub fn begin_create(&mut self) {
        self.draft = Some(ScheduleDraft::default());
        self.last_error = None;
    }

    /// 草稿只存在内存中。
    #[must_use]
    pub fn draft(&self) -> Option<&ScheduleDraft> {
        self.draft.as_ref()
    }

    /// 更新草稿。
    pub fn update_draft(&mut self, draft: ScheduleDraft) {
        self.draft = Some(draft);
    }

    /// 草稿完整度最低边界，提交后由 RPC 层做协议校验。
    #[must_use]
    pub fn can_submit_draft(&self) -> bool {
        self.draft.as_ref().is_some_and(|draft| {
            !draft.name.trim().is_empty()
                && !draft.cadence.trim().is_empty()
                && !draft.agent_id.trim().is_empty()
                && !draft.prompt.is_empty()
        })
    }

    /// 外层确认 RPC 创建成功后清空草稿。
    pub fn create_succeeded(&mut self, summary: ScheduleSummary) {
        let id = summary.schedule_id.clone();
        self.schedules.insert(id.clone(), summary);
        self.selected = Some(id);
        self.draft = None;
        self.last_error = None;
    }

    /// 由按钮生成的停用/恢复意图。
    pub fn toggle_pause(&self, id: &ScheduleId) -> Option<ScheduleIntent> {
        let schedule = self.schedules.get(id)?;
        Some(ScheduleIntent::Pause {
            schedule_id: id.clone(),
            paused: schedule.state != ScheduleState::Paused,
        })
    }

    /// 由按钮生成的删除意图；本地记录等 RPC 成功后再清理。
    pub fn delete_intent(&self, id: &ScheduleId) -> Option<ScheduleIntent> {
        self.schedules
            .contains_key(id)
            .then(|| ScheduleIntent::Delete {
                schedule_id: id.clone(),
            })
    }

    /// 由按钮生成的立即运行意图。
    pub fn trigger_intent(&self, id: &ScheduleId) -> Option<ScheduleIntent> {
        self.schedules
            .contains_key(id)
            .then(|| ScheduleIntent::TriggerNow {
                schedule_id: id.clone(),
            })
    }

    /// RPC 删除成功后才从本地状态移除。
    pub fn delete_succeeded(&mut self, id: &ScheduleId) {
        self.schedules.remove(id);
        self.recent_results.remove(id);
        if self.selected.as_ref() == Some(id) {
            self.selected = None;
        }
    }

    /// RPC 停用成功后用新摘要覆盖当前状态。
    pub fn pause_succeeded(&mut self, summary: ScheduleSummary) {
        self.schedules.insert(summary.schedule_id.clone(), summary);
        self.last_error = None;
    }

    /// 接收运行结束结果，保存最近结果并保留失败原因。
    pub fn receive_result(&mut self, result: ScheduleRunResult) {
        if self.schedules.contains_key(&result.schedule_id) {
            self.last_error = result.error.as_ref().map(|error| error.summary_zh.clone());
            self.recent_results
                .insert(result.schedule_id.clone(), result);
        }
    }

    /// RPC 或门控错误：保留完整可读摘要供占位视图展示。
    pub fn reject(&mut self, message: impl Into<String>) {
        self.last_error = Some(message.into());
    }

    /// 最近结果是否成功，供视图状态色和文字投影使用。
    #[must_use]
    pub fn last_outcome(&self, id: &ScheduleId) -> Option<ScheduleRunOutcome> {
        self.recent_results.get(id).map(|result| result.outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soxu_protocol::{ActionableError, ErrorCode, ScheduleRunOutcome};

    /// 夹具经 serde 反序列化构造，避免 desktop 直接依赖 `chrono`。
    fn summary(id: &str, updated_at: &str) -> ScheduleSummary {
        let body = format!(
            r#"{{
                "schedule_id": "{id}",
                "workspace_id": "wks_1",
                "name": "调度 {id}",
                "cadence": {{"kind": "interval", "every_seconds": 60}},
                "trigger": {{"agent_id": "agent-1", "text": "巡检"}},
                "validity": {{"starts_at": null, "expires_at": null}},
                "retry": {{"max_attempts": 1, "backoff_seconds": 0}},
                "state": "active",
                "next_run_at": "2030-03-17T00:00:00Z",
                "last_run_at": null,
                "created_at": "2030-03-16T00:00:00Z",
                "updated_at": "{updated_at}"
            }}"#
        );
        serde_json::from_str(&body).expect("合法调度摘要夹具")
    }

    #[test]
    fn open_close_reuses_focus_and_obeys_modal_lock() {
        let mut state = ScheduleCenterState::default();
        assert_eq!(state.effective_focus_context(), "WorkbenchGlobal");
        assert!(state.open());
        assert_eq!(state.effective_focus_context(), "ReviewDiff");
        state.close();
        assert_eq!(state.effective_focus_context(), "WorkbenchGlobal");

        let mut state = ScheduleCenterState::default();
        state.focus.push(FocusLayer::ModalDialog);
        assert!(!state.open(), "模态锁定下不得夺焦");
        assert!(!state.is_open());
        assert_eq!(state.effective_focus_context(), "ModalDialog");
    }

    #[test]
    fn list_is_stable_and_selection_is_pruned_on_snapshot_replace() {
        let mut state = ScheduleCenterState::default();
        state.replace_schedules([
            summary("sch_b", "2030-03-16T00:00:00Z"),
            summary("sch_a", "2030-03-16T00:00:00Z"),
            summary("sch_c", "2030-03-17T00:00:00Z"),
        ]);
        let ids: Vec<_> = state
            .schedules()
            .iter()
            .map(|row| row.schedule_id.as_str())
            .collect();
        assert_eq!(ids, vec!["sch_c", "sch_a", "sch_b"]);
        let selected = ScheduleId::try_from("sch_a".to_owned()).expect("标识");
        assert!(state.select(&selected));
        state.replace_schedules([summary("sch_b", "2030-03-16T00:00:00Z")]);
        assert!(state.selected().is_none(), "外部删除后清除悬空选择");
    }

    #[test]
    fn draft_has_submit_boundary_and_only_commits_on_success() {
        let mut state = ScheduleCenterState::default();
        state.begin_create();
        assert!(!state.can_submit_draft());
        state.update_draft(ScheduleDraft {
            name: "每日检查".to_owned(),
            cadence: "0 3 * * *".to_owned(),
            agent_id: "agent-1".to_owned(),
            prompt: "检查状态".to_owned(),
        });
        assert!(state.can_submit_draft());
        assert!(state.schedules().is_empty(), "草稿不伪装成已提交条目");
        state.create_succeeded(summary("sch_new", "2030-03-18T00:00:00Z"));
        assert!(state.draft().is_none());
        assert_eq!(
            state.selected().map(|row| row.schedule_id.as_str()),
            Some("sch_new")
        );
    }

    #[test]
    fn mutations_are_intents_until_host_reports_success() {
        let mut state = ScheduleCenterState::default();
        let row = summary("sch_a", "2030-03-16T00:00:00Z");
        let id = row.schedule_id.clone();
        state.replace_schedules([row]);
        assert_eq!(
            state.delete_intent(&id),
            Some(ScheduleIntent::Delete {
                schedule_id: id.clone()
            })
        );
        assert!(state.schedules().iter().any(|row| row.schedule_id == id));
        state.delete_succeeded(&id);
        assert!(state.schedules().is_empty());
    }

    #[test]
    fn pause_intent_mirrors_state_and_shows_failure_reason() {
        let mut state = ScheduleCenterState::default();
        let row = summary("sch_a", "2030-03-16T00:00:00Z");
        let id = row.schedule_id.clone();
        state.replace_schedules([row]);
        assert_eq!(
            state.toggle_pause(&id),
            Some(ScheduleIntent::Pause {
                schedule_id: id.clone(),
                paused: true,
            })
        );

        let mut paused = summary("sch_a", "2030-03-16T01:00:00Z");
        paused.state = ScheduleState::Paused;
        paused.next_run_at = None;
        state.pause_succeeded(paused);
        assert_eq!(state.selected(), None);
        assert_eq!(
            state.toggle_pause(&id),
            Some(ScheduleIntent::Pause {
                schedule_id: id.clone(),
                paused: false,
            }),
            "停用后按钮语义反转"
        );

        let body = format!(
            r#"{{
                "schedule_id": "{}",
                "run_id": "run_1",
                "outcome": "failed",
                "attempts": 2,
                "session_id": null,
                "error": null,
                "finished_at": "2030-03-16T02:00:00Z"
            }}"#,
            id.as_str()
        );
        let mut result: soxu_protocol::ScheduleRunResult =
            serde_json::from_str(&body).expect("合法运行结果夹具");
        result.error = Some(ActionableError::new(
            ErrorCode::DriverUnavailable,
            "目标 CLI 不可用",
            "本次运行未触发任何会话",
            "请安装并登录取目标 CLI 后重试",
        ));
        state.receive_result(result);
        assert_eq!(state.last_outcome(&id), Some(ScheduleRunOutcome::Failed));
        assert_eq!(
            state.last_error(),
            Some("目标 CLI 不可用"),
            "失败原因必须可被占位视图展示"
        );
    }

    #[test]
    fn overflow_is_reported_beyond_visible_window() {
        let mut state = ScheduleCenterState::default();
        let rows = (0..SCHEDULE_CENTER_MAX_VISIBLE + 2)
            .map(|index| summary(&format!("sch_{index:02}"), "2030-03-16T00:00:00Z"));
        state.replace_schedules(rows);
        assert!(state.has_overflow());
        assert_eq!(state.schedules().len(), SCHEDULE_CENTER_MAX_VISIBLE + 2);
    }
}
