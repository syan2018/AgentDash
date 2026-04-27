//! Task 生命周期事件 → Session notification / overview 桥接。
//!
//! 职责：把 Task 侧的状态事件封装成 ACP `SessionNotification`（`session_info_update`），
//! 以及从 `SessionHub` 拉取最小 SessionOverview 供 Task service 使用。

use serde_json::Value;

use agent_client_protocol::{SessionId, SessionInfoUpdate, SessionNotification, SessionUpdate};

use crate::task::execution::TaskExecutionError;
use crate::task::meta::build_task_lifecycle_meta;

use super::errors::map_internal_error;

pub fn bridge_task_status_event_to_session_notification(
    session_id: &str,
    turn_id: &str,
    event_type: &str,
    message: &str,
    data: Value,
) -> SessionNotification {
    let meta = build_task_lifecycle_meta(turn_id, event_type, message, data);
    SessionNotification::new(
        SessionId::new(session_id.to_string()),
        SessionUpdate::SessionInfoUpdate(SessionInfoUpdate::new().meta(meta)),
    )
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
