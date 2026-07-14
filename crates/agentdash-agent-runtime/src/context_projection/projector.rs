use agentdash_agent_protocol::{ContextDeliveryMetadata, ContextFrame};
use sha2::{Digest, Sha256};

use super::{ContextFrameFacts, ContextProjectionIdentity, RuntimeSurfacePresentationPlan};

#[derive(Debug, Clone, Copy, Default)]
pub struct ContextProjector;

impl ContextProjector {
    #[must_use]
    pub fn project_auto_resume(
        identity: &ContextProjectionIdentity,
        reason: &str,
        prompt: &str,
    ) -> ContextFrame {
        Self::project(
            identity,
            [ContextFrameFacts {
                kind: agentdash_agent_protocol::ContextFrameKind::AutoResume,
                source: agentdash_agent_protocol::ContextFrameSource::CompanionResult,
                phase_node: None,
                apply_mode: None,
                delivery_status:
                    agentdash_agent_protocol::ContextDeliveryStatus::QueuedForTransformContext,
                delivery_channel: agentdash_agent_protocol::ContextDeliveryChannel::TurnStart,
                message_role: agentdash_agent_protocol::ContextMessageRole::User,
                rendered_text: prompt.to_string(),
                sections: vec![
                    agentdash_agent_protocol::ContextFrameSection::AutoResume {
                        title: "Resume".to_string(),
                        summary: prompt.to_string(),
                        reason: reason.to_string(),
                        prompt: prompt.to_string(),
                    },
                    agentdash_agent_protocol::ContextFrameSection::SystemNotice {
                        title: "Notice".to_string(),
                        summary: "resumed".to_string(),
                        body: None,
                    },
                ],
            }],
        )
        .bootstrap_frames
        .remove(0)
    }

    #[must_use]
    pub fn project_pending_action(
        identity: &ContextProjectionIdentity,
        title: &str,
        summary: &str,
        action_id: &str,
        action_type: &str,
        revision: u64,
        turn_id: Option<String>,
        injections: Vec<agentdash_agent_protocol::RuntimeHookInjectionEntry>,
    ) -> ContextFrame {
        let instruction = pending_action_instruction(action_type);
        let status = "pending";
        let mut rendered_sections = vec![format!(
            "[待处理 Hook 事项]\n{title}（type={action_type}，status={status}，revision={revision}）"
        )];
        rendered_sections.push(format!("事项 id: {action_id}"));
        if !summary.trim().is_empty() {
            rendered_sections.push(summary.trim().to_string());
        }
        if let Some(turn_id) = turn_id.as_deref() {
            rendered_sections.push(format!("关联 turn: {turn_id}"));
        }
        rendered_sections.push(instruction.to_string());
        if !injections.is_empty() {
            let lines = injections
                .iter()
                .map(|injection| {
                    if injection.content.trim().is_empty() {
                        format!("- [{}] {}", injection.slot, injection.source)
                    } else {
                        format!(
                            "- [{}] {}: {}",
                            injection.slot,
                            injection.source,
                            injection.content.trim()
                        )
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            rendered_sections.push(format!("关联注入片段：\n{lines}"));
        }
        rendered_sections.push(
            "以上事项来自 Hook Runtime 的待处理回流，优先级高于普通自然对话推进。处理时尽量避免重复总结，聚焦完成剩余动作。"
                .to_string(),
        );
        Self::project(
            identity,
            [ContextFrameFacts {
                kind: agentdash_agent_protocol::ContextFrameKind::PendingAction,
                source: agentdash_agent_protocol::ContextFrameSource::CompanionResult,
                phase_node: None,
                apply_mode: None,
                delivery_status:
                    agentdash_agent_protocol::ContextDeliveryStatus::QueuedForTransformContext,
                delivery_channel: agentdash_agent_protocol::ContextDeliveryChannel::TurnStart,
                message_role: agentdash_agent_protocol::ContextMessageRole::User,
                rendered_text: rendered_sections.join("\n\n"),
                sections: vec![
                    agentdash_agent_protocol::ContextFrameSection::PendingAction {
                        title: title.to_string(),
                        summary: summary.to_string(),
                        action_id: action_id.to_string(),
                        action_type: action_type.to_string(),
                        status: status.to_string(),
                        revision,
                        turn_id,
                        instructions: vec![instruction.to_string()],
                        injections,
                    },
                ],
            }],
        )
        .bootstrap_frames
        .remove(0)
    }

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
            transition_phase_node: None,
            bootstrap_frames: frames,
            adoption_frames: Vec::new(),
        }
    }
}

fn pending_action_instruction(action_type: &str) -> &'static str {
    match action_type {
        "blocking_review" => {
            "当前事项是阻塞式 review。不要复述前文；直接处理剩余动作，并在完成后明确结案。"
        }
        "follow_up_required" => {
            "当前事项要求继续跟进。不要停在总结；请直接落实后续动作，并在完成后明确结案。"
        }
        _ => "请直接处理这项 Hook 待办，并在完成后明确结案。",
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
