//! Companion 子域的 notification 构造 helpers。
//!
//! `build_companion_human_response_notification` 是旧 runtime event helper。
//! 当前 AgentRun-facing human response 投递以 durable mailbox message 为事实源；
//! human request 仍通过 `build_companion_event_notification` 暴露为 UI-facing 事件。

use agentdash_agent_protocol::{
    BackboneEnvelope, BackboneEvent, PlatformEvent, SourceInfo, TraceInfo,
};

/// 构造 companion "人类回应" 事件通知。
///
/// 该 helper 仅保留给仍需要 runtime meta event 的旧调用点；新的 human response
/// continuation 应进入 AgentRun mailbox。
pub fn build_companion_human_response_notification(
    session_id: &str,
    turn_id: Option<&str>,
    request_id: &str,
    payload: &serde_json::Value,
    request_type: Option<&str>,
    resumed_waiting_tool: bool,
) -> BackboneEnvelope {
    let summary = payload
        .get("summary")
        .or_else(|| payload.get("note"))
        .or_else(|| payload.get("choice"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let status = payload
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("responded");
    let response_type = payload.get("type").and_then(|v| v.as_str());

    let value = serde_json::json!({
        "event_type": "companion_human_response",
        "severity": "info",
        "message": format!("[用户回应] status={status} {summary}"),
        "request_id": request_id,
        "status": status,
        "summary": summary,
        "payload": payload,
        "request_type": request_type,
        "response_type": response_type,
        "resumed_waiting_tool": resumed_waiting_tool,
    });

    let source = SourceInfo {
        connector_id: "agentdash-companion".to_string(),
        connector_type: "human_respond".to_string(),
        executor_id: None,
    };

    BackboneEnvelope::new(
        BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
            key: "companion_human_response".to_string(),
            value,
        }),
        session_id,
        source,
    )
    .with_trace(TraceInfo {
        turn_id: turn_id.map(ToString::to_string),
        entry_index: None,
    })
}

pub fn build_companion_event_notification(
    session_id: &str,
    turn_id: &str,
    event_type: &str,
    message: String,
    data: serde_json::Value,
) -> BackboneEnvelope {
    let source = SourceInfo {
        connector_id: "agentdash-companion".to_string(),
        connector_type: "runtime_tool".to_string(),
        executor_id: None,
    };

    BackboneEnvelope::new(
        BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
            key: event_type.to_string(),
            value: serde_json::json!({
                "message": message,
                "data": data,
            }),
        }),
        session_id,
        source,
    )
    .with_trace(TraceInfo {
        turn_id: Some(turn_id.to_string()),
        entry_index: None,
    })
}
