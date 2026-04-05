use agent_client_protocol::Meta;
use agentdash_acp_meta::{
    AgentDashEventV1, AgentDashMetaV1, AgentDashSourceV1, AgentDashTraceV1, merge_agentdash_meta,
    parse_agentdash_meta,
};

/// 从 ACP Meta 中提取 turn_id（不做匹配，纯提取）
pub fn extract_turn_id_from_meta(meta: Option<&Meta>) -> Option<String> {
    parse_agentdash_meta(meta?)
        .and_then(|m| m.trace.and_then(|trace| trace.turn_id))
}

/// 判断通知事件是否属于指定 turn
pub fn turn_matches(meta: Option<&Meta>, expected_turn_id: &str) -> bool {
    let Some(meta) = meta else {
        return false;
    };
    parse_agentdash_meta(meta)
        .and_then(|m| m.trace.and_then(|trace| trace.turn_id))
        .as_deref()
        == Some(expected_turn_id)
}

/// 从通知 meta 中提取匹配的 turn 事件类型和可选消息
pub fn parse_turn_event(
    meta: Option<&Meta>,
    expected_turn_id: &str,
) -> Option<(String, Option<String>)> {
    let parsed = parse_agentdash_meta(meta?)?;
    let trace = parsed.trace?;
    let turn_id = trace.turn_id?;
    if turn_id != expected_turn_id {
        return None;
    }
    let event = parsed.event?;
    Some((event.r#type, event.message))
}

/// 构造 Task 生命周期事件的 ACP Meta，用于桥接到 session 流
pub fn build_task_lifecycle_meta(
    turn_id: &str,
    event_type: &str,
    message: &str,
    data: serde_json::Value,
) -> Meta {
    let mut trace = AgentDashTraceV1::new();
    trace.turn_id = Some(turn_id.to_string());

    let mut event = AgentDashEventV1::new(event_type);
    event.severity = Some("info".to_string());
    event.message = Some(message.to_string());
    event.data = Some(data);

    let source = AgentDashSourceV1::new("agentdash-task-execution", "api_task_route");
    let agentdash = AgentDashMetaV1::new()
        .source(Some(source))
        .trace(Some(trace))
        .event(Some(event));

    merge_agentdash_meta(None, &agentdash).expect("构造 task 生命周期 ACP Meta 不应失败")
}
