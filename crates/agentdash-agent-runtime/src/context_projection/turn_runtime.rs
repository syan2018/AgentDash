use agentdash_agent_protocol::{
    ContextDeliveryChannel, ContextDeliveryStatus, ContextFrame, ContextFrameKind,
    ContextFrameSection, ContextFrameSource, ContextMessageRole, RuntimeHookInjectionEntry,
};

use super::{ContextFrameFacts, ContextProjectionIdentity, ContextProjector};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingActionPresentationFacts {
    pub source: ContextFrameSource,
    pub title: String,
    pub summary: String,
    pub action_id: String,
    pub action_type: String,
    pub status: String,
    pub runtime_revision: u64,
    pub turn_id: Option<String>,
    pub owners: Vec<String>,
    pub injections: Vec<RuntimeHookInjectionEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemDeliveryPresentationFacts {
    pub source: ContextFrameSource,
    pub session_id: String,
    pub turn_id: String,
    pub delivery_kind: String,
    pub source_kind: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemNoticePresentationFacts {
    pub id: String,
    pub source: ContextFrameSource,
    pub content: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookSemanticPresentationFacts {
    SystemNotice {
        title: String,
        summary: String,
        body: Option<String>,
    },
    AssignmentInjection {
        title: String,
        summary: String,
        injections: Vec<RuntimeHookInjectionEntry>,
    },
}

pub fn project_hook_presentation(
    identity: &ContextProjectionIdentity,
    source: ContextFrameSource,
    facts: HookSemanticPresentationFacts,
) -> Result<ContextFrame, String> {
    Ok(single_frame(
        identity,
        compile_hook_presentation_facts(source, facts)?,
    ))
}

pub fn compile_hook_presentation_facts(
    source: ContextFrameSource,
    facts: HookSemanticPresentationFacts,
) -> Result<ContextFrameFacts, String> {
    let facts = match facts {
        HookSemanticPresentationFacts::SystemNotice {
            title,
            summary,
            body,
        } => {
            let rendered_text = body
                .as_deref()
                .filter(|body| !body.trim().is_empty())
                .unwrap_or(summary.as_str())
                .trim()
                .to_string();
            if rendered_text.is_empty() {
                return Err("Hook system notice presentation must contain summary or body".into());
            }
            ContextFrameFacts {
                kind: ContextFrameKind::SystemNotice,
                source,
                phase_node: None,
                apply_mode: None,
                delivery_status: ContextDeliveryStatus::QueuedForTransformContext,
                delivery_channel: ContextDeliveryChannel::TurnStart,
                message_role: ContextMessageRole::User,
                rendered_text,
                sections: vec![ContextFrameSection::SystemNotice {
                    title,
                    summary,
                    body,
                }],
            }
        }
        HookSemanticPresentationFacts::AssignmentInjection {
            title,
            summary,
            injections,
        } => {
            let fragments = injections
                .into_iter()
                .filter(|injection| !injection.content.trim().is_empty())
                .map(
                    |injection| agentdash_agent_protocol::RuntimeContextFragmentEntry {
                        label: injection.slot.clone(),
                        slot: injection.slot,
                        source: injection.source,
                        content: injection.content,
                        context_usage_kind: None,
                    },
                )
                .collect::<Vec<_>>();
            if fragments.is_empty() {
                return Err("Hook assignment presentation requires a non-empty injection".into());
            }
            let rendered_text = std::iter::once("# Assignment Context".to_string())
                .chain(fragments.iter().map(|fragment| {
                    format!(
                        "## {} (`{}`)\nsource: `{}`\n\n{}",
                        fragment.label,
                        fragment.slot,
                        fragment.source,
                        fragment.content.trim()
                    )
                }))
                .collect::<Vec<_>>()
                .join("\n\n");
            ContextFrameFacts {
                kind: ContextFrameKind::AssignmentContext,
                source,
                phase_node: Some("hook_injection".to_string()),
                apply_mode: None,
                delivery_status: ContextDeliveryStatus::QueuedForTransformContext,
                delivery_channel: ContextDeliveryChannel::TurnStart,
                message_role: ContextMessageRole::User,
                rendered_text,
                sections: vec![ContextFrameSection::AssignmentContext {
                    title,
                    summary,
                    fragments,
                }],
            }
        }
    };
    Ok(facts)
}

#[must_use]
pub fn project_pending_action(
    identity: &ContextProjectionIdentity,
    facts: &PendingActionPresentationFacts,
) -> Option<ContextFrame> {
    if facts.summary.trim().is_empty() && facts.injections.is_empty() {
        return None;
    }
    let instruction = pending_action_instruction(&facts.action_type);
    let owners = (!facts.owners.is_empty()).then(|| facts.owners.join("\n"));
    let mut instructions = vec![instruction.to_string()];
    if let Some(owners) = owners.as_deref() {
        instructions.push(format!("归属对象：\n{owners}"));
    }
    let mut rendered = vec![format!(
        "[待处理 Hook 事项]\n{}（type={}，status={}，revision={}）",
        facts.title, facts.action_type, facts.status, facts.runtime_revision
    )];
    rendered.push(format!("事项 id: {}", facts.action_id));
    if !facts.summary.trim().is_empty() {
        rendered.push(facts.summary.trim().to_string());
    }
    if let Some(turn_id) = facts.turn_id.as_deref() {
        rendered.push(format!("关联 turn: {turn_id}"));
    }
    rendered.push(instruction.to_string());
    if let Some(owners) = owners {
        rendered.push(format!("归属对象：\n{owners}"));
    }
    if !facts.injections.is_empty() {
        rendered.push(format!(
            "关联注入片段：\n{}",
            facts
                .injections
                .iter()
                .map(|entry| if entry.content.trim().is_empty() {
                    format!("- [{}] {}", entry.slot, entry.source)
                } else {
                    format!(
                        "- [{}] {}: {}",
                        entry.slot,
                        entry.source,
                        entry.content.trim()
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    rendered.push("以上事项来自 Hook Runtime 的待处理回流，优先级高于普通自然对话推进。处理时尽量避免重复总结，聚焦完成剩余动作。".to_string());

    let frame = single_frame(
        identity,
        ContextFrameFacts {
            kind: ContextFrameKind::PendingAction,
            source: facts.source,
            phase_node: None,
            apply_mode: None,
            delivery_status: ContextDeliveryStatus::QueuedForTransformContext,
            delivery_channel: ContextDeliveryChannel::TurnStart,
            message_role: ContextMessageRole::User,
            rendered_text: rendered.join("\n\n"),
            sections: vec![ContextFrameSection::PendingAction {
                title: facts.title.clone(),
                summary: facts.summary.clone(),
                action_id: facts.action_id.clone(),
                action_type: facts.action_type.clone(),
                status: facts.status.clone(),
                revision: facts.runtime_revision,
                turn_id: facts.turn_id.clone(),
                instructions,
                injections: facts.injections.clone(),
            }],
        },
    );
    Some(frame)
}

#[must_use]
pub fn project_system_delivery(
    identity: &ContextProjectionIdentity,
    facts: &SystemDeliveryPresentationFacts,
) -> Option<ContextFrame> {
    let content = facts.content.trim();
    if content.is_empty() {
        return None;
    }
    let summary = bounded_summary(content);
    let rendered_text = format!(
        "## AgentDash System Delivery\n\n- kind: {}\n- source: {}\n- status: delivered\n- turn_id: {}\n\n{}",
        facts.delivery_kind, facts.source_kind, facts.turn_id, summary
    );
    let mut frame = single_frame(
        identity,
        ContextFrameFacts {
            kind: ContextFrameKind::SystemDelivery,
            source: facts.source,
            phase_node: None,
            apply_mode: None,
            delivery_status: ContextDeliveryStatus::Accepted,
            delivery_channel: ContextDeliveryChannel::ConnectorContext,
            message_role: ContextMessageRole::System,
            rendered_text: rendered_text.clone(),
            sections: vec![ContextFrameSection::SystemNotice {
                title: "AgentDash System Delivery".to_string(),
                summary: format!(
                    "{} from {} for session {}.",
                    facts.delivery_kind, facts.source_kind, facts.session_id
                ),
                body: Some(rendered_text),
            }],
        },
    );
    frame.id = format!("{}:system-delivery-context", facts.turn_id);
    Some(frame)
}

#[must_use]
pub fn project_system_notice(facts: &SystemNoticePresentationFacts) -> Option<ContextFrame> {
    let content = facts.content.trim();
    if content.is_empty() {
        return None;
    }
    let identity = ContextProjectionIdentity {
        operation_id: facts.id.clone(),
        source_frame_id: facts.id.clone(),
        source_frame_revision: 0,
        recorded_at_ms: facts.created_at_ms,
    };
    let mut frame = single_frame(
        &identity,
        ContextFrameFacts {
            kind: ContextFrameKind::SystemNotice,
            source: facts.source,
            phase_node: None,
            apply_mode: None,
            delivery_status: ContextDeliveryStatus::QueuedForTransformContext,
            delivery_channel: ContextDeliveryChannel::TurnStart,
            message_role: ContextMessageRole::User,
            rendered_text: content.to_string(),
            sections: vec![ContextFrameSection::SystemNotice {
                title: "TurnStart Notice".to_string(),
                summary: "TurnStart notice 已桥接为 ContextFrame。".to_string(),
                body: Some(content.to_string()),
            }],
        },
    );
    frame.id.clone_from(&facts.id);
    Some(frame)
}

fn single_frame(identity: &ContextProjectionIdentity, facts: ContextFrameFacts) -> ContextFrame {
    ContextProjector::project(identity, [facts])
        .bootstrap_frames
        .remove(0)
}

fn bounded_summary(text: &str) -> String {
    const MAX_CHARS: usize = 2_000;
    if text.chars().count() <= MAX_CHARS {
        return text.to_string();
    }
    let mut summary = text.chars().take(MAX_CHARS).collect::<String>();
    summary.push_str("...");
    summary
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
    use super::*;

    fn identity() -> ContextProjectionIdentity {
        ContextProjectionIdentity {
            operation_id: "turn-1".to_string(),
            source_frame_id: "frame-1".to_string(),
            source_frame_revision: 7,
            recorded_at_ms: 10,
        }
    }

    #[test]
    fn system_delivery_matches_main_family_and_delivery_semantics() {
        let frame = project_system_delivery(
            &identity(),
            &SystemDeliveryPresentationFacts {
                source: ContextFrameSource::RuntimeContextUpdate,
                session_id: "session-1".to_string(),
                turn_id: "turn-1".to_string(),
                delivery_kind: "hook_auto_resume".to_string(),
                source_kind: "hook_auto_resume".to_string(),
                content: "continue".to_string(),
            },
        )
        .expect("system delivery");
        assert_eq!(frame.id, "turn-1:system-delivery-context");
        assert_eq!(frame.kind, ContextFrameKind::SystemDelivery);
        assert_eq!(frame.delivery_status, ContextDeliveryStatus::Accepted);
        assert_eq!(
            frame.delivery_channel,
            ContextDeliveryChannel::ConnectorContext
        );
        assert_eq!(frame.message_role, ContextMessageRole::System);
        assert!(frame.rendered_text.contains("kind: hook_auto_resume"));
        assert!(matches!(
            frame.sections.as_slice(),
            [ContextFrameSection::SystemNotice { body: Some(body), .. }]
                if body == &frame.rendered_text
        ));
    }

    #[test]
    fn notice_and_pending_apply_main_empty_rules_and_real_revision() {
        assert!(
            project_system_notice(&SystemNoticePresentationFacts {
                id: "notice-1".to_string(),
                source: ContextFrameSource::RuntimeContextUpdate,
                content: "  ".to_string(),
                created_at_ms: 10,
            })
            .is_none()
        );
        assert!(
            project_pending_action(
                &identity(),
                &PendingActionPresentationFacts {
                    source: ContextFrameSource::CompanionResult,
                    title: "Review".to_string(),
                    summary: String::new(),
                    action_id: "action-1".to_string(),
                    action_type: "blocking_review".to_string(),
                    status: "pending".to_string(),
                    runtime_revision: 42,
                    turn_id: None,
                    owners: Vec::new(),
                    injections: Vec::new(),
                }
            )
            .is_none()
        );
        let frame = project_pending_action(
            &identity(),
            &PendingActionPresentationFacts {
                source: ContextFrameSource::CompanionResult,
                title: "Review".to_string(),
                summary: "result".to_string(),
                action_id: "action-1".to_string(),
                action_type: "blocking_review".to_string(),
                status: "pending".to_string(),
                runtime_revision: 42,
                turn_id: Some("turn-0".to_string()),
                owners: vec!["- scope: project project: p1".to_string()],
                injections: Vec::new(),
            },
        )
        .expect("pending action");
        assert_eq!(frame.id, "pending-action-action-1-10");
        assert!(matches!(
            frame.sections.as_slice(),
            [ContextFrameSection::PendingAction { revision: 42, instructions, .. }]
                if instructions.iter().any(|line| line.contains("project: p1"))
        ));
    }
}
