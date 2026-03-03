use serde::{Deserialize, Serialize};

use crate::story::StateChange;

/// 面向客户端的事件流消息
///
/// 通过 NDJSON 推送到前端，每行一个 JSON 对象。
/// 客户端可用 `since_id` 请求增量事件实现 Resume。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum StreamEvent {
    Connected { last_event_id: i64 },
    StateChanged(StateChange),
    Heartbeat { timestamp: i64 },
}
