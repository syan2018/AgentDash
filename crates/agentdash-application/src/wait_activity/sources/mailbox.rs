use agentdash_domain::agent_run_mailbox::AgentRunMailboxMessage;
use serde_json::json;

use super::bound_string;
use crate::wait_activity::types::{ResolvedWaitScope, WAIT_PREVIEW_CHARS, WaitActivityItem};

pub(crate) fn mailbox_belongs_to_scope(
    message: &AgentRunMailboxMessage,
    scope: &ResolvedWaitScope,
) -> bool {
    scope.run_id.is_none_or(|run_id| message.run_id == run_id)
        && scope
            .agent_id
            .is_none_or(|agent_id| message.agent_id == agent_id)
}

pub(crate) fn mailbox_item_from_message(message: &AgentRunMailboxMessage) -> WaitActivityItem {
    WaitActivityItem {
        activity_ref: message.id.to_string(),
        kind: "mailbox".to_string(),
        status: message.status.as_str().to_string(),
        source_ref: message.source.source_ref.clone(),
        correlation_ref: message.source.correlation_ref.clone(),
        preview: Some(bound_string(&message.preview, WAIT_PREVIEW_CHARS)),
        diagnostic: None,
        result_refs: json!({
            "mailbox_message_id": message.id.to_string(),
            "run_id": message.run_id.to_string(),
            "agent_id": message.agent_id.to_string(),
            "source_namespace": message.source.namespace,
            "source_kind": message.source.kind,
            "source_ref": message.source.source_ref,
            "correlation_ref": message.source.correlation_ref,
            "source_dedup_key": message.source_dedup_key,
        }),
        exec: None,
        cursor: Some(message.updated_at.timestamp_millis().to_string()),
        next: Some(json!({
            "source": "mailbox",
            "message_id": message.id.to_string(),
        })),
        updated_at_ms: message.updated_at.timestamp_millis(),
    }
}

pub(crate) fn mailbox_message_is_wait_relevant(message: &AgentRunMailboxMessage) -> bool {
    if matches!(
        message.source.namespace.as_str(),
        "companion" | "exec" | "workflow" | "wait" | "routine"
    ) {
        return true;
    }
    matches!(
        message.source.kind.as_str(),
        "wake" | "response" | "result" | "completion" | "parent_resume" | "hook_auto_resume"
    )
}
