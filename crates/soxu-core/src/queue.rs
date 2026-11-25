//! 活跃回合待发队列管理与一键提级流转（`Queue -> Steer / Interrupt`）。

use soxu_protocol::{ActionableError, ActiveTurnBehavior, ErrorCode, QueuedMessageItem};
use std::collections::BTreeMap;

/// 将排队消息提级为纠偏或打断时返回的原子消费负载。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuePromotion {
    pub item: QueuedMessageItem,
    pub target_behavior: ActiveTurnBehavior,
}

/// 按会话维护的活跃回合待发队列管理器。
#[derive(Debug, Default, Clone)]
pub struct TurnQueueManager {
    by_session: BTreeMap<String, Vec<QueuedMessageItem>>,
}

impl TurnQueueManager {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 将新指令加入目标会话待发队列并返回最新队列快照。
    pub fn enqueue(&mut self, item: QueuedMessageItem) -> Vec<QueuedMessageItem> {
        let queue = self.by_session.entry(item.session_id.clone()).or_default();
        queue.push(item);
        queue.clone()
    }

    /// 撤回指定待发排队条目，防止找不到目标时静默忽略。
    pub fn cancel(
        &mut self,
        session_id: &str,
        queue_id: &str,
    ) -> Result<Vec<QueuedMessageItem>, ActionableError> {
        let Some(queue) = self.by_session.get_mut(session_id) else {
            return Err(ActionableError::new(
                ErrorCode::NotFound,
                "未找到可撤回的排队消息",
                "仅影响当前排队撤回操作",
                "请刷新会话状态确认该消息是否已被自动发出",
            ));
        };

        let Some(pos) = queue.iter().position(|item| item.queue_id == queue_id) else {
            return Err(ActionableError::new(
                ErrorCode::NotFound,
                "指定排队消息已不存在",
                "仅影响当前排队撤回操作",
                "该消息可能已随回合结束发出或已被其他终端取消",
            ));
        };

        queue.remove(pos);
        Ok(queue.clone())
    }

    /// 将指定排队消息原子提级为 `Steer`（纠偏）或 `Interrupt`（打断），并立即移出队列防重复投递。
    pub fn promote(
        &mut self,
        session_id: &str,
        queue_id: &str,
        target_behavior: ActiveTurnBehavior,
    ) -> Result<QueuePromotion, ActionableError> {
        if target_behavior == ActiveTurnBehavior::Queue {
            return Err(ActionableError::new(
                ErrorCode::ValidationFailed,
                "无效的排队提级目标",
                "当前消息仍保留在待发队列中",
                "请选择提级为中途纠偏（Steer）或立即打断（Interrupt）",
            ));
        }

        let Some(queue) = self.by_session.get_mut(session_id) else {
            return Err(ActionableError::new(
                ErrorCode::NotFound,
                "待提级的排队消息不存在",
                "未能注入当前活跃回合",
                "请检查该消息是否已消费或重新输入指令",
            ));
        };

        let Some(pos) = queue.iter().position(|item| item.queue_id == queue_id) else {
            return Err(ActionableError::new(
                ErrorCode::NotFound,
                "待提级的排队消息不存在",
                "未能注入当前活跃回合",
                "请检查该消息是否已消费或重新输入指令",
            ));
        };

        let mut item = queue.remove(pos);
        item.behavior = target_behavior;
        Ok(QueuePromotion {
            item,
            target_behavior,
        })
    }

    /// 回合自然结束后按 FIFO 取出下一条待发指令。
    pub fn pop_next(&mut self, session_id: &str) -> Option<QueuedMessageItem> {
        let queue = self.by_session.get_mut(session_id)?;
        if queue.is_empty() {
            return None;
        }
        Some(queue.remove(0))
    }

    #[must_use]
    pub fn snapshot(&self, session_id: &str) -> Vec<QueuedMessageItem> {
        self.by_session.get(session_id).cloned().unwrap_or_default()
    }
}
