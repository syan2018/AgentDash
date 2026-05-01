//! Hub 的 compaction 事件元数据富化。
//!
//! `context_compacted` 事件允许 connector 只写 `messages_compacted` 计数而不带
//! `compacted_until_ref`——hub 在持久化前回补 MessageRef，让 Inspector/前端能够
//! 精确定位历史边界。相关逻辑只与 `persist_notification` 协同，不涉及 connector。

use agentdash_protocol::{BackboneEnvelope, BackboneEvent};
use agentdash_agent_types::{AgentMessage, MessageRef};
use std::io;

use super::super::continuation::build_projected_transcript_from_events;
use super::SessionHub;

impl SessionHub {
    /// 对 context_compacted 事件回补 compacted_until_ref。
    ///
    /// 如果 envelope 是 `BackboneEvent::ContextCompacted` 且缺少 boundary ref，
    /// 从已持久化的 transcript 推导补全。
    /// 对于通过 compat 路径产出的 SessionInfoUpdate 形式的 compaction 事件，
    /// 同样尝试补全。
    pub(super) async fn maybe_enrich_compaction_notification(
        &self,
        session_id: &str,
        envelope: BackboneEnvelope,
    ) -> io::Result<BackboneEnvelope> {
        let messages_compacted = match &envelope.event {
            BackboneEvent::ContextCompacted(_compacted) => {
                // codex ContextCompactedNotification 只包含 thread_id/turn_id，
                // messages_compacted 需要从 compat 路径获取
                self.maybe_enrich_compaction_via_compat(session_id, &envelope)
                    .await
            }
            BackboneEvent::Platform(agentdash_protocol::PlatformEvent::SessionMetaUpdate {
                key,
                value,
            }) if key == "context_compacted" || key == "turn_lifecycle" => {
                // compat path: 从 ACP SessionInfoUpdate meta 的 event.data 里抽
                value
                    .get("messages_compacted")
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|v| u32::try_from(v).ok())
            }
            _ => {
                // 也检查 compat 转换后的 ACP 路径
                self.maybe_enrich_compaction_via_compat(session_id, &envelope)
                    .await
            }
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

        // 在 envelope 中注入 boundary ref（通过 trace 或其他方式）
        // 对于 ContextCompacted 事件，无法直接修改 codex 类型字段，
        // 所以我们保持 envelope 不变 —— boundary ref 已通过 transcript 投影可导出。
        // 下游 build_projected_transcript_from_events 会使用 derive_compaction_boundary_ref_from_events。
        let _ = compacted_until_ref;
        Ok(envelope)
    }

    async fn maybe_enrich_compaction_via_compat(
        &self,
        _session_id: &str,
        envelope: &BackboneEnvelope,
    ) -> Option<u32> {
        let notification = agentdash_protocol::envelope_to_session_notification(envelope)?;
        let agent_client_protocol::SessionUpdate::SessionInfoUpdate(info) = &notification.update
        else {
            return None;
        };
        let meta = info.meta.as_ref()?;
        let parsed = agentdash_acp_meta::parse_agentdash_meta(meta)?;
        let event = parsed.event?;
        if event.r#type != "context_compacted" {
            return None;
        }
        event
            .data?
            .get("messages_compacted")
            .and_then(serde_json::Value::as_u64)
            .and_then(|v| u32::try_from(v).ok())
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
