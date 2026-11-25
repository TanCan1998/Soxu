//! 项目与工作区注意力状态桶聚合与排序规则。

use soxu_protocol::{QuickChatSummary, WorkspaceBucket, WorkspaceSummary};

/// 按注意力优先级聚合多个状态桶（`needs_input > failed > running > attention > done`）。
#[must_use]
pub fn aggregate_bucket(buckets: impl IntoIterator<Item = WorkspaceBucket>) -> WorkspaceBucket {
    buckets
        .into_iter()
        .min_by_key(|bucket| bucket.priority_rank())
        .unwrap_or(WorkspaceBucket::Done)
}

/// 聚合指定项目下未归档工作树的最高优先级状态桶。
#[must_use]
pub fn project_aggregated_bucket(
    workspaces: &[WorkspaceSummary],
    project_id: &str,
) -> WorkspaceBucket {
    aggregate_bucket(
        workspaces
            .iter()
            .filter(|wks| wks.project_id == project_id && !wks.archived)
            .map(|wks| wks.bucket),
    )
}

/// 按「未归档优先 -> 注意力优先级 -> 最近更新时间降序 -> ID 升序」排序工作区列表。
pub fn sort_workspaces_by_attention(workspaces: &mut [WorkspaceSummary]) {
    workspaces.sort_by(|a, b| {
        a.archived
            .cmp(&b.archived)
            .then_with(|| a.bucket.priority_rank().cmp(&b.bucket.priority_rank()))
            .then_with(|| b.updated_at.cmp(&a.updated_at))
            .then_with(|| a.workspace_id.cmp(&b.workspace_id))
    });
}

/// 按「未归档优先 -> 注意力优先级 -> 最近更新时间降序 -> ID 升序」排序无项目快速对话列表。
pub fn sort_quick_chats_by_attention(chats: &mut [QuickChatSummary]) {
    chats.sort_by(|a, b| {
        a.archived
            .cmp(&b.archived)
            .then_with(|| a.bucket.priority_rank().cmp(&b.bucket.priority_rank()))
            .then_with(|| b.updated_at.cmp(&a.updated_at))
            .then_with(|| a.chat_id.cmp(&b.chat_id))
    });
}
