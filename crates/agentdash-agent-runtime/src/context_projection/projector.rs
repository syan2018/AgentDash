use agentdash_agent_protocol::{ContextDeliveryMetadata, ContextFrame};
use sha2::{Digest, Sha256};

use super::{ContextFrameFacts, ContextProjectionIdentity, RuntimeSurfacePresentationPlan};

#[derive(Debug, Clone, Copy, Default)]
pub struct ContextProjector;

impl ContextProjector {
    /// Projects facts without consulting a clock, repository, connector, or global queue.
    #[must_use]
    pub fn project(
        identity: &ContextProjectionIdentity,
        facts: impl IntoIterator<Item = ContextFrameFacts>,
    ) -> RuntimeSurfacePresentationPlan {
        let frames = facts
            .into_iter()
            .enumerate()
            .map(|(ordinal, facts)| ContextFrame {
                id: format!("context-frame-{}-{ordinal}", identity.operation_id),
                kind: facts.kind,
                source: facts.source,
                phase_node: facts.phase_node,
                apply_mode: facts.apply_mode,
                delivery_status: facts.delivery_status,
                delivery_channel: facts.delivery_channel,
                message_role: facts.message_role,
                delivery_metadata: ContextDeliveryMetadata::for_frame(
                    facts.kind,
                    facts.delivery_channel,
                    facts.message_role,
                ),
                rendered_text: facts.rendered_text,
                sections: facts.sections,
                created_at_ms: identity.recorded_at_ms,
            })
            .collect::<Vec<_>>();
        let encoded = serde_json::to_vec(&frames).expect("ContextFrame is serializable");
        let digest = format!("sha256:{:x}", Sha256::digest(encoded));
        RuntimeSurfacePresentationPlan {
            digest,
            source_frame_id: identity.source_frame_id.clone(),
            source_frame_revision: identity.source_frame_revision,
            bootstrap_frames: frames,
            adoption_frames: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use agentdash_agent_protocol::{
        ContextDeliveryChannel, ContextDeliveryStatus, ContextFrameKind, ContextFrameSection,
        ContextFrameSource, ContextMessageRole,
    };

    use super::*;

    #[test]
    fn projection_is_replayable_and_keeps_main_payload_shape() {
        let identity = ContextProjectionIdentity {
            operation_id: "operation-1".to_string(),
            source_frame_id: "frame-1".to_string(),
            source_frame_revision: 3,
            recorded_at_ms: 1_720_000_000_000,
        };
        let facts = ContextFrameFacts {
            kind: ContextFrameKind::Identity,
            source: ContextFrameSource::RuntimeContextUpdate,
            phase_node: None,
            apply_mode: None,
            delivery_status: ContextDeliveryStatus::Accepted,
            delivery_channel: ContextDeliveryChannel::ConnectorContext,
            message_role: ContextMessageRole::System,
            rendered_text: "system prompt".to_string(),
            sections: vec![ContextFrameSection::Identity {
                title: "Identity".to_string(),
                summary: "system prompt".to_string(),
                fragments: Vec::new(),
            }],
        };
        let first = ContextProjector::project(&identity, [facts.clone()]);
        let replay = ContextProjector::project(&identity, [facts]);
        assert_eq!(first, replay);
        assert_eq!(
            serde_json::to_value(&first.bootstrap_frames[0]).unwrap(),
            serde_json::json!({
                "id": "context-frame-operation-1-0",
                "kind": "identity",
                "source": "runtime_context_update",
                "delivery_status": "accepted",
                "delivery_channel": "connector_context",
                "message_role": "system",
                "delivery_metadata": {
                    "delivery_phase": "stable_system",
                    "delivery_order": 10,
                    "cache_policy": "static",
                    "model_channel": "system",
                    "agent_consumption": { "target": "", "mode": "consume", "reason": "default_identity_delivery" },
                    "frontend_label": "Identity",
                    "connector_profile": { "profile_id": "" }
                },
                "rendered_text": "system prompt",
                "sections": [{ "kind": "identity", "title": "Identity", "summary": "system prompt", "fragments": [] }],
                "created_at_ms": 1_720_000_000_000_i64
            })
        );
    }
}
