use agent_client_protocol::Meta;

const AGENTDASH_NS: &str = "agentdash";

/// 从 ACP Meta 中提取 turn_id（不做匹配，纯提取）
pub fn extract_turn_id_from_meta(meta: Option<&Meta>) -> Option<String> {
    let agentdash = meta?.get(AGENTDASH_NS)?;
    agentdash
        .get("trace")?
        .get("turnId")
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string)
}

/// 判断通知事件是否属于指定 turn
pub fn turn_matches(meta: Option<&Meta>, expected_turn_id: &str) -> bool {
    extract_turn_id_from_meta(meta).as_deref() == Some(expected_turn_id)
}

/// 从通知 meta 中提取匹配的 turn 事件类型和可选消息
pub fn parse_turn_event(
    meta: Option<&Meta>,
    expected_turn_id: &str,
) -> Option<(String, Option<String>)> {
    let agentdash = meta?.get(AGENTDASH_NS)?;
    let turn_id = agentdash
        .get("trace")?
        .get("turnId")
        .and_then(serde_json::Value::as_str)?;
    if turn_id != expected_turn_id {
        return None;
    }
    let event = agentdash.get("event")?;
    let event_type = event.get("type").and_then(serde_json::Value::as_str)?;
    let message = event
        .get("message")
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string);
    Some((event_type.to_string(), message))
}

/// 构造 Task 生命周期事件的 BackboneEnvelope。
pub fn build_task_lifecycle_envelope(
    session_id: &str,
    turn_id: &str,
    event_type: &str,
    message: &str,
    data: serde_json::Value,
) -> agentdash_protocol::BackboneEnvelope {
    use agentdash_protocol::{BackboneEvent, PlatformEvent, SourceInfo};

    let source = SourceInfo {
        connector_id: "agentdash-task-execution".to_string(),
        connector_type: "api_task_route".to_string(),
        executor_id: None,
    };
    let value = serde_json::json!({
        "event_type": event_type,
        "severity": "info",
        "message": message,
        "data": data,
    });
    agentdash_protocol::BackboneEnvelope::new(
        BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
            key: event_type.to_string(),
            value,
        }),
        session_id,
        source,
    )
    .with_turn_id(turn_id)
}
