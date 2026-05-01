//! Task 生命周期事件 → Session notification / overview 桥接。
//!
//! 职责：把 Task 侧的状态事件封装成 `BackboneEnvelope`（`PlatformEvent::SessionMetaUpdate`），
//! 以及从 `SessionHub` 拉取最小 SessionOverview 供 Task service 使用。

use serde_json::Value;

use agentdash_protocol::{BackboneEnvelope, BackboneEvent, PlatformEvent, SourceInfo, TraceInfo};

use crate::task::execution::TaskExecutionError;

use super::errors::map_internal_error;

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

pub async fn get_session_overview(
    session_hub: &crate::session::SessionHub,
    session_id: &str,
) -> Result<Option<crate::task::execution::SessionOverview>, TaskExecutionError> {
    let meta = session_hub
        .get_session_meta(session_id)
        .await
        .map_err(map_internal_error)?;
    Ok(meta.map(|value| crate::task::execution::SessionOverview {
        title: value.title,
        updated_at: value.updated_at,
    }))
}
