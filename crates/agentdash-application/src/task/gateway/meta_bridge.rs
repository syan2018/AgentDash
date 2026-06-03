//! Task 生命周期事件 → backbone notification 桥接。
//!
//! 保留 bridge_task_status_event_to_envelope 供 lifecycle 事件桥接使用。

use serde_json::Value;

use agentdash_agent_protocol::{
    BackboneEnvelope, BackboneEvent, PlatformEvent, SourceInfo, TraceInfo,
};

pub fn bridge_task_status_event_to_envelope(
    session_id: &str,
    turn_id: &str,
    event_type: &str,
    message: &str,
    data: Value,
) -> BackboneEnvelope {
    let source = SourceInfo {
        connector_id: "agentdash-task".to_string(),
        connector_type: "lifecycle".to_string(),
        executor_id: None,
    };

    let value = serde_json::json!({
        "event_type": event_type,
        "message": message,
        "data": data,
    });

    BackboneEnvelope::new(
        BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
            key: "task_lifecycle".to_string(),
            value,
        }),
        session_id,
        source,
    )
    .with_trace(TraceInfo {
        turn_id: Some(turn_id.to_string()),
        entry_index: None,
    })
}
