//! Companion 子域的 notification 构造 helpers。
//!
//! `build_companion_human_response_notification` 构造的是 companion "用户回应"
//! 事件通知，不属于 continuation transcript 重建。用户回应先 resolve durable
//! `LifecycleGate`，再由 companion gate delivery adapter 写入 runtime event stream，
//! 让 Context Inspector 能看到人类回应。

use agentdash_agent_protocol::{
    BackboneEnvelope, BackboneEvent, PlatformEvent, SourceInfo, TraceInfo,
};

/// 构造 companion "人类回应" 事件通知。
///
/// 调用方：`companion::gate_control` 的 runtime delivery adapter。
/// 被 registered 的 companion tool 等待 gate resolve 时，HTTP 层先写 gate truth，
/// 再由 delivery adapter 产出 `BackboneEnvelope` 供 Inspector 可视化。
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
