use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use tokio::sync::RwLock;
use uuid::Uuid;

use agentdash_agent_protocol::UserInputBlock;
use agentdash_spi::AgentConfig;

/// 单条待发送消息
#[derive(Debug, Clone)]
pub struct PendingMessage {
    pub id: String,
    pub input: Vec<UserInputBlock>,
    pub executor_config: Option<AgentConfig>,
    pub created_at: DateTime<Utc>,
}

/// Pending 队列暂停原因
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueuePauseReason {
    TurnFailed,
    TurnInterrupted,
}

/// 单个 session 的 pending 消息队列
#[derive(Debug, Clone, Default)]
struct SessionPendingQueue {
    messages: Vec<PendingMessage>,
    paused: Option<QueuePauseReason>,
}

/// Pending 消息轻量视图（不含完整 input）
#[derive(Debug, Clone)]
pub struct PendingMessagePreview {
    pub id: String,
    pub preview: String,
    pub has_images: bool,
    pub created_at: DateTime<Utc>,
}

fn build_preview(msg: &PendingMessage) -> PendingMessagePreview {
    let preview = msg
        .input
        .iter()
        .find_map(|block| match block {
            UserInputBlock::Text { text, .. } => {
                let trimmed = text.trim();
                if trimmed.len() > 80 {
                    Some(format!("{}…", &trimmed[..77]))
                } else {
                    Some(trimmed.to_string())
                }
            }
            _ => None,
        })
        .unwrap_or_default();
    let has_images = msg
        .input
        .iter()
        .any(|block| matches!(block, UserInputBlock::Image { .. }));
    PendingMessagePreview {
        id: msg.id.clone(),
        preview,
        has_images,
        created_at: msg.created_at,
    }
}

/// 进程内 pending 消息队列管理
#[derive(Clone)]
pub struct PendingQueueService {
    queues: Arc<RwLock<HashMap<String, SessionPendingQueue>>>,
}

impl PendingQueueService {
    pub fn new() -> Self {
        Self {
            queues: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 排队一条消息
    pub async fn enqueue(
        &self,
        runtime_session_id: &str,
        input: Vec<UserInputBlock>,
        executor_config: Option<AgentConfig>,
    ) -> PendingMessagePreview {
        let msg = PendingMessage {
            id: Uuid::new_v4().to_string(),
            input,
            executor_config,
            created_at: Utc::now(),
        };
        let preview = build_preview(&msg);
        let mut queues = self.queues.write().await;
        queues
            .entry(runtime_session_id.to_string())
            .or_default()
            .messages
            .push(msg);
        preview
    }

    /// 列出某 session 的全部 pending 消息预览
    pub async fn list(&self, runtime_session_id: &str) -> Vec<PendingMessagePreview> {
        let queues = self.queues.read().await;
        queues
            .get(runtime_session_id)
            .map(|q| q.messages.iter().map(build_preview).collect())
            .unwrap_or_default()
    }

    /// 删除一条 pending 消息
    pub async fn delete(&self, runtime_session_id: &str, message_id: &str) -> bool {
        let mut queues = self.queues.write().await;
        if let Some(queue) = queues.get_mut(runtime_session_id) {
            let before = queue.messages.len();
            queue.messages.retain(|m| m.id != message_id);
            queue.messages.len() < before
        } else {
            false
        }
    }

    /// 取出队首消息用于派发（返回 None = 队列为空或暂停中）
    pub async fn dequeue_front(&self, runtime_session_id: &str) -> Option<PendingMessage> {
        let mut queues = self.queues.write().await;
        if let Some(queue) = queues.get_mut(runtime_session_id) {
            if queue.paused.is_some() {
                return None;
            }
            if queue.messages.is_empty() {
                return None;
            }
            Some(queue.messages.remove(0))
        } else {
            None
        }
    }

    /// 将消息放回队首，用于自动派发失败后的无损恢复。
    pub async fn requeue_front(&self, runtime_session_id: &str, message: PendingMessage) {
        let mut queues = self.queues.write().await;
        queues
            .entry(runtime_session_id.to_string())
            .or_default()
            .messages
            .insert(0, message);
    }

    /// 取出指定消息用于 promote-to-steer
    pub async fn take(&self, runtime_session_id: &str, message_id: &str) -> Option<PendingMessage> {
        let mut queues = self.queues.write().await;
        if let Some(queue) = queues.get_mut(runtime_session_id) {
            if let Some(idx) = queue.messages.iter().position(|m| m.id == message_id) {
                Some(queue.messages.remove(idx))
            } else {
                None
            }
        } else {
            None
        }
    }

    /// 标记队列暂停（turn_failed / turn_interrupted）
    pub async fn pause(&self, runtime_session_id: &str, reason: QueuePauseReason) {
        let mut queues = self.queues.write().await;
        queues
            .entry(runtime_session_id.to_string())
            .or_default()
            .paused = Some(reason);
    }

    /// 恢复队列（用户手动重试时）
    pub async fn resume(&self, runtime_session_id: &str) {
        let mut queues = self.queues.write().await;
        if let Some(queue) = queues.get_mut(runtime_session_id) {
            queue.paused = None;
        }
    }

    /// 队列是否处于暂停状态
    pub async fn is_paused(&self, runtime_session_id: &str) -> Option<QueuePauseReason> {
        let queues = self.queues.read().await;
        queues.get(runtime_session_id).and_then(|q| q.paused)
    }

    /// 获取队列长度
    pub async fn len(&self, runtime_session_id: &str) -> usize {
        let queues = self.queues.read().await;
        queues
            .get(runtime_session_id)
            .map(|q| q.messages.len())
            .unwrap_or(0)
    }

    /// 清空 session 的全部 pending 消息
    pub async fn clear(&self, runtime_session_id: &str) {
        let mut queues = self.queues.write().await;
        queues.remove(runtime_session_id);
    }
}

impl Default for PendingQueueService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_agent_protocol::text_user_input_blocks;

    #[tokio::test]
    async fn enqueue_and_list() {
        let svc = PendingQueueService::new();
        svc.enqueue("s1", text_user_input_blocks("hello"), None)
            .await;
        svc.enqueue("s1", text_user_input_blocks("world"), None)
            .await;
        let list = svc.list("s1").await;
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].preview, "hello");
        assert_eq!(list[1].preview, "world");
    }

    #[tokio::test]
    async fn dequeue_front_takes_first() {
        let svc = PendingQueueService::new();
        svc.enqueue("s1", text_user_input_blocks("first"), None)
            .await;
        svc.enqueue("s1", text_user_input_blocks("second"), None)
            .await;
        let msg = svc.dequeue_front("s1").await.unwrap();
        assert!(
            msg.input
                .iter()
                .any(|b| matches!(b, UserInputBlock::Text { text, .. } if text == "first"))
        );
        assert_eq!(svc.len("s1").await, 1);
    }

    #[tokio::test]
    async fn requeue_front_restores_message_order() {
        let svc = PendingQueueService::new();
        svc.enqueue("s1", text_user_input_blocks("first"), None)
            .await;
        svc.enqueue("s1", text_user_input_blocks("second"), None)
            .await;
        let first = svc.dequeue_front("s1").await.unwrap();

        svc.requeue_front("s1", first).await;

        let restored = svc.dequeue_front("s1").await.unwrap();
        assert!(
            restored
                .input
                .iter()
                .any(|b| matches!(b, UserInputBlock::Text { text, .. } if text == "first"))
        );
    }

    #[tokio::test]
    async fn pause_blocks_dequeue() {
        let svc = PendingQueueService::new();
        svc.enqueue("s1", text_user_input_blocks("msg"), None).await;
        svc.pause("s1", QueuePauseReason::TurnFailed).await;
        assert!(svc.dequeue_front("s1").await.is_none());
        svc.resume("s1").await;
        assert!(svc.dequeue_front("s1").await.is_some());
    }

    #[tokio::test]
    async fn delete_message() {
        let svc = PendingQueueService::new();
        let preview = svc.enqueue("s1", text_user_input_blocks("del"), None).await;
        assert!(svc.delete("s1", &preview.id).await);
        assert_eq!(svc.len("s1").await, 0);
    }

    #[tokio::test]
    async fn take_removes_by_id() {
        let svc = PendingQueueService::new();
        let p1 = svc.enqueue("s1", text_user_input_blocks("a"), None).await;
        let _p2 = svc.enqueue("s1", text_user_input_blocks("b"), None).await;
        let taken = svc.take("s1", &p1.id).await.unwrap();
        assert_eq!(taken.id, p1.id);
        assert_eq!(svc.len("s1").await, 1);
    }
}
