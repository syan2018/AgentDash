use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::{StateChange, StoryStatus, TaskStatus};

/// 面向客户端的事件流消息
///
/// 通过 NDJSON 推送到前端，每行一个 JSON 对象。
/// 客户端可用 `since_id` 请求增量事件实现 Resume。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum StreamEvent {
    /// 连接建立确认，携带当前最新 event_id
    Connected { last_event_id: i64 },

    /// 状态变更事件
    StateChanged(StateChange),

    /// Story 状态摘要（全量快照，用于首次同步）
    StorySummary {
        story_id: Uuid,
        status: StoryStatus,
        task_count: i64,
        completed_count: i64,
    },

    /// Task 状态摘要
    TaskSummary {
        task_id: Uuid,
        story_id: Uuid,
        status: TaskStatus,
    },

    /// 心跳，维持连接
    Heartbeat { timestamp: i64 },
}
