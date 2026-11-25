//! 会话草稿防抖与单调代次号（`generation`）并发控制。

use soxu_protocol::DraftSnapshot;
use std::collections::BTreeMap;

/// 草稿落盘裁决结果：仅接受严格更新的代次号，拦截过期延迟写覆盖。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DraftSaveOutcome {
    Accepted(DraftSnapshot),
    RejectedStale { current_generation: u64 },
}

/// 内存草稿缓存管理器（按 `session_id` 索引）。
#[derive(Debug, Default, Clone)]
pub struct DraftStore {
    by_session: BTreeMap<String, DraftSnapshot>,
}

impl DraftStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 按单调 `generation` 尝试保存草稿；若旧代次延迟到达则拒绝覆盖并返回当前最新代次。
    pub fn save(&mut self, draft: DraftSnapshot) -> DraftSaveOutcome {
        if let Some(existing) = self.by_session.get(&draft.session_id)
            && draft.generation <= existing.generation
        {
            return DraftSaveOutcome::RejectedStale {
                current_generation: existing.generation,
            };
        }

        self.by_session
            .insert(draft.session_id.clone(), draft.clone());
        DraftSaveOutcome::Accepted(draft)
    }

    #[must_use]
    pub fn get(&self, session_id: &str) -> Option<&DraftSnapshot> {
        self.by_session.get(session_id)
    }

    pub fn clear(&mut self, session_id: &str) -> Option<DraftSnapshot> {
        self.by_session.remove(session_id)
    }

    #[must_use]
    pub fn all(&self) -> Vec<DraftSnapshot> {
        self.by_session.values().cloned().collect()
    }
}
