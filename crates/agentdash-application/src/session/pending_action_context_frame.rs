use agentdash_spi::hooks::{
    ContextFrame, ContextFrameSection, HookPendingAction, HookPendingActionStatus,
    HookSessionRuntimeSnapshot, RuntimeContextFragmentEntry, RuntimeEventSource,
    RuntimeHookInjectionEntry,
};

use super::context_frame::{self, ContextFramePayload};
use super::hook_messages as msg;

#[derive(Debug, Clone)]
struct PendingActionFrame {
    action: HookPendingAction,
    owners_md: Option<String>,
    runtime_revision: u64,
}

impl PendingActionFrame {
    fn from_parts(
        snapshot: &agentdash_spi::hooks::SessionHookSnapshot,
        action: &HookPendingAction,
        runtime: &HookSessionRuntimeSnapshot,
    ) -> Option<Self> {
        if action.summary.trim().is_empty() && action.injections.is_empty() {
            return None;
        }
        let owners_md = (!snapshot.owners.is_empty()).then(|| {
            snapshot
                .owners
                .iter()
                .map(|owner| {
                    format!(
                        "- {}: {} {}",
                        owner.owner_type,
                        owner.label.as_deref().unwrap_or("??"),
                        owner.owner_id
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        });
        Some(Self {
            action: action.clone(),
            owners_md,
            runtime_revision: runtime.revision,
        })
    }
}

impl ContextFramePayload for PendingActionFrame {
    fn id(&self, created_at_ms: i64) -> String {
        format!("pending-action-{}-{created_at_ms}", self.action.id)
    }

    fn kind(&self) -> &'static str {
        "pending_action"
    }

    fn source(&self) -> RuntimeEventSource {
        self.action.source.clone()
    }

    fn delivery_status(&self) -> String {
        "queued_for_transform_context".to_string()
    }

    fn sections(&self) -> Vec<ContextFrameSection> {
        let instruction = msg::pending_action_instruction(self.action.action_type.as_str());
        let mut instructions = Vec::new();
        if !instruction.trim().is_empty() {
            instructions.push(instruction.to_string());
        }
        if let Some(owners_md) = &self.owners_md {
            instructions.push(format!("归属对象：\n{owners_md}"));
        }
        vec![ContextFrameSection::PendingAction {
            title: self.action.title.clone(),
            summary: self.action.summary.clone(),
            action_id: self.action.id.clone(),
            action_type: self.action.action_type.clone(),
            status: pending_action_status_label(self.action.status).to_string(),
            revision: self.runtime_revision,
            turn_id: self.action.turn_id.clone(),
            instructions,
            injections: self
                .action
                .injections
                .iter()
                .map(|injection| RuntimeHookInjectionEntry {
                    slot: injection.slot.clone(),
                    source: injection.source.clone(),
                    content: injection.content.clone(),
                })
                .collect(),
        }]
    }

    fn rendered_text(&self) -> String {
        let mut sections = vec![msg::pending_action_header(
            &self.action.title,
            &self.action.action_type,
            pending_action_status_label(self.action.status),
            self.runtime_revision,
        )];
        sections.push(msg::pending_action_id_line(&self.action.id));
        if !self.action.summary.trim().is_empty() {
            sections.push(self.action.summary.trim().to_string());
        }
        if let Some(turn_id) = self.action.turn_id.as_deref() {
            sections.push(msg::pending_action_turn_line(turn_id));
        }
        sections
            .push(msg::pending_action_instruction(self.action.action_type.as_str()).to_string());
        if let Some(owners_md) = &self.owners_md {
            sections.push(msg::owners_section(owners_md));
        }
        if !self.action.injections.is_empty() {
            let fragments = self
                .action
                .injections
                .iter()
                .map(|injection| RuntimeContextFragmentEntry {
                    slot: injection.slot.clone(),
                    label: injection.slot.clone(),
                    source: injection.source.clone(),
                    content: injection.content.clone(),
                })
                .collect::<Vec<_>>();
            sections.push(format_injection_fragments(&fragments));
        }
        sections.push(msg::PENDING_ACTION_FOOTER.to_string());
        sections.join("\n\n")
    }
}

pub(crate) fn build_pending_action_context_frame(
    snapshot: &agentdash_spi::hooks::SessionHookSnapshot,
    action: &HookPendingAction,
    runtime: &HookSessionRuntimeSnapshot,
) -> Option<ContextFrame> {
    let frame = PendingActionFrame::from_parts(snapshot, action, runtime)?;
    Some(context_frame::build_context_frame(&frame))
}

fn pending_action_status_label(status: HookPendingActionStatus) -> &'static str {
    match status {
        HookPendingActionStatus::Pending => "pending",
        HookPendingActionStatus::Resolved => "resolved",
    }
}

fn format_injection_fragments(fragments: &[RuntimeContextFragmentEntry]) -> String {
    let lines = fragments
        .iter()
        .map(|fragment| {
            if fragment.content.trim().is_empty() {
                format!("- [{}] {}", fragment.slot, fragment.source)
            } else {
                format!(
                    "- [{}] {}: {}",
                    fragment.slot,
                    fragment.source,
                    fragment.content.trim()
                )
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    if lines.is_empty() {
        "关联注入片段：\n- （无）".to_string()
    } else {
        format!("关联注入片段：\n{lines}")
    }
}
