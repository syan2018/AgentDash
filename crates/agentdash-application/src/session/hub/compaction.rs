//! Hub 的 compaction 事件元数据富化。
//!
//! `context_compacted` 事件允许 connector 只写 `messages_compacted` 计数而不带
//! `compacted_until_ref`——hub 在持久化前回补 MessageRef，让 Inspector/前端能够
//! 精确定位历史边界。相关逻辑只与 `persist_notification` 协同，不涉及 connector。

use agentdash_agent_types::{AgentMessage, MessageRef};
use agentdash_protocol::{BackboneEnvelope, BackboneEvent};
use std::io;

use super::super::continuation::build_projected_transcript_from_events;
use super::SessionHub;

impl SessionHub {
    /// 对 context_compacted 事件回补 compacted_until_ref。
    ///
    /// 直接从 BackboneEvent 提取 messages_compacted 计数，不绕道 ACP compat。
    pub(super) async fn maybe_enrich_compaction_notification(
        &self,
        session_id: &str,
        envelope: BackboneEnvelope,
    ) -> io::Result<BackboneEnvelope> {
        let messages_compacted = match &envelope.event {
            BackboneEvent::Platform(agentdash_protocol::PlatformEvent::SessionMetaUpdate {
                key,
                value,
            }) if key == "context_compacted" => value
                .get("messages_compacted")
                .and_then(serde_json::Value::as_u64)
                .and_then(|v| u32::try_from(v).ok()),
            _ => None,
        };

        let Some(messages_compacted) = messages_compacted else {
            return Ok(envelope);
        };

        let Some(compacted_until_ref) = self
            .derive_compaction_boundary_ref(session_id, messages_compacted)
            .await?
        else {
            return Ok(envelope);
        };

        let mut enriched = envelope;
        if let BackboneEvent::Platform(agentdash_protocol::PlatformEvent::SessionMetaUpdate {
            value,
            ..
        }) = &mut enriched.event
        {
            if let Some(obj) = value.as_object_mut() {
                if let Ok(ref_value) = serde_json::to_value(&compacted_until_ref) {
                    obj.insert("compacted_until_ref".to_string(), ref_value);
                }
            }
        }
        Ok(enriched)
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
