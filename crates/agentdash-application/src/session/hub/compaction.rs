//! Hub 的 compaction 事件元数据富化。
//!
//! `context_compacted` 事件允许 connector 只写 `messages_compacted` 计数而不带
//! `compacted_until_ref`——hub 在持久化前回补 MessageRef，让 Inspector/前端能够
//! 精确定位历史边界。相关逻辑只与 `persist_notification` 协同，不涉及 connector。

use agent_client_protocol::{SessionNotification, SessionUpdate};
use agentdash_acp_meta::{AgentDashMetaV1, merge_agentdash_meta, parse_agentdash_meta};
use agentdash_agent_types::{AgentMessage, MessageRef};
use std::io;

use super::super::continuation::build_projected_transcript_from_events;
use super::SessionHub;

impl SessionHub {
    pub(super) async fn maybe_enrich_compaction_notification(
        &self,
        session_id: &str,
        mut notification: SessionNotification,
    ) -> io::Result<SessionNotification> {
        let SessionUpdate::SessionInfoUpdate(info) = &mut notification.update else {
            return Ok(notification);
        };
        let Some(meta) = info.meta.as_ref() else {
            return Ok(notification);
        };
        let Some(parsed) = parse_agentdash_meta(meta) else {
            return Ok(notification);
        };
        let Some(mut event) = parsed.event.clone() else {
            return Ok(notification);
        };
        if event.r#type != "context_compacted" {
            return Ok(notification);
        }

        let Some(data) = event
            .data
            .as_mut()
            .and_then(serde_json::Value::as_object_mut)
        else {
            return Ok(notification);
        };
        let has_boundary_ref = data
            .get("compacted_until_ref")
            .and_then(|value| serde_json::from_value::<MessageRef>(value.clone()).ok())
            .is_some();
        if has_boundary_ref {
            return Ok(notification);
        }

        let messages_compacted = data
            .get("messages_compacted")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or_default();
        let Some(compacted_until_ref) = self
            .derive_compaction_boundary_ref(session_id, messages_compacted)
            .await?
        else {
            return Ok(notification);
        };

        data.insert(
            "compacted_until_ref".to_string(),
            serde_json::to_value(compacted_until_ref).unwrap_or(serde_json::Value::Null),
        );
        info.meta = merge_agentdash_meta(
            info.meta.clone(),
            &AgentDashMetaV1::new()
                .source(parsed.source)
                .trace(parsed.trace)
                .event(Some(event)),
        );
        Ok(notification)
    }

    async fn derive_compaction_boundary_ref(
        &self,
        session_id: &str,
        messages_compacted: u32,
    ) -> io::Result<Option<MessageRef>> {
        if messages_compacted == 0 {
            return Ok(None);
        }

        let events = self.persistence.list_all_events(session_id).await?;
        Ok(derive_compaction_boundary_ref_from_events(
            &events,
            messages_compacted,
        ))
    }
}

pub(super) fn derive_compaction_boundary_ref_from_events(
    events: &[super::super::persistence::PersistedSessionEvent],
    messages_compacted: u32,
) -> Option<MessageRef> {
    if messages_compacted == 0 {
        return None;
    }

    let transcript = build_projected_transcript_from_events(events);
    let first_entry = transcript.entries.first()?;
    let (start_index, previously_compacted, previous_boundary_ref) = match &first_entry.message {
        AgentMessage::CompactionSummary {
            messages_compacted,
            compacted_until_ref,
            ..
        } => (
            1_usize,
            usize::try_from(*messages_compacted).ok()?,
            compacted_until_ref.clone(),
        ),
        _ => (0, 0, None),
    };

    let total_compacted = usize::try_from(messages_compacted).ok()?;
    if total_compacted < previously_compacted {
        return None;
    }
    if total_compacted == previously_compacted {
        return previous_boundary_ref;
    }

    let cut = start_index.checked_add(total_compacted - previously_compacted)?;
    transcript
        .entries
        .get(cut.checked_sub(1)?)
        .map(|entry| entry.message_ref.clone())
}
