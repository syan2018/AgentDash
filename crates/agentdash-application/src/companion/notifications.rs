//! Companion 子域的 notification 构造 helpers。
//!
//! PR 7d：`build_companion_human_response_notification` 从
//! `session/continuation.rs` 挪出来。原位置是因为早期把所有"历史事件
//! 转 notification"逻辑都堆进去了，但这个函数与"continuation transcript
//! 重建"毫无关系——它构造的是 companion "用户回应" 事件通知（由
//! `hub::respond_companion_request` 持久化进事件流，让 Context Inspector
//! 能看到人类回应）。归位到 `companion/`，让调用链"companion 工具请求 →
//! companion 工具回应"全部落在一个子域。

use agent_client_protocol::{SessionId, SessionInfoUpdate, SessionNotification, SessionUpdate};
use agentdash_acp_meta::{
    AgentDashEventV1, AgentDashMetaV1, AgentDashSourceV1, AgentDashTraceV1, merge_agentdash_meta,
};

/// 构造 companion "人类回应" 事件通知。
///
/// 调用方：`SessionHub::respond_companion_request`（hub/facade.rs）。
/// 被 registered 的 companion tool 在 pending 状态等待时，HTTP 层触发此
/// 函数产出 `SessionInfoUpdate` notification，进事件流供 Inspector 可视化。
pub fn build_companion_human_response_notification(
    session_id: &str,
    turn_id: Option<&str>,
    request_id: &str,
    payload: &serde_json::Value,
    request_type: Option<&str>,
    resumed_waiting_tool: bool,
) -> SessionNotification {
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

    let mut trace = AgentDashTraceV1::new();
    trace.turn_id = turn_id.map(ToString::to_string);

    let mut event = AgentDashEventV1::new("companion_human_response");
    event.severity = Some("info".to_string());
    event.message = Some(format!("[用户回应] status={status} {summary}"));
    event.data = Some(serde_json::json!({
        "request_id": request_id,
        "status": status,
        "summary": summary,
        "payload": payload,
        "request_type": request_type,
        "response_type": response_type,
        "resumed_waiting_tool": resumed_waiting_tool,
    }));

    let source = AgentDashSourceV1::new("agentdash-companion", "human_respond");
    let agentdash = AgentDashMetaV1::new()
        .source(Some(source))
        .trace(Some(trace))
        .event(Some(event));

    SessionNotification::new(
        SessionId::new(session_id.to_string()),
        SessionUpdate::SessionInfoUpdate(SessionInfoUpdate::new().meta(
            merge_agentdash_meta(None, &agentdash).expect("构造 companion response meta 不应失败"),
        )),
    )
}
