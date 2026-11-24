//! Git Worktree 隔离池、对话轮次级 Checkpoint 快照、Rewind 回退与结构化 Diff。

use serde::{Deserialize, Serialize};

/// 与对话轮次绑定的 Git 检查点摘要。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointSummary {
    pub checkpoint_id: String,
    pub turn_index: u32,
    pub commit_oid: String,
    pub changed_files: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructs_checkpoint_summary() {
        let cp = CheckpointSummary {
            checkpoint_id: "cp_01".to_owned(),
            turn_index: 1,
            commit_oid: "01234567".to_owned(),
            changed_files: 3,
        };
        assert_eq!(cp.turn_index, 1);
    }
}
