use agentdash_spi::context::bundle::SessionContextBundle;
use agentdash_spi::context::injection::FragmentScope;
use agentdash_spi::hooks::{
    ContextFrame, ContextFrameSection, HookTurnStartNotice, RuntimeContextFragmentEntry,
    RuntimeEventSource, SharedHookSessionRuntime,
};
use agentdash_spi::{ContextFragment, DiscoveredGuideline};

use crate::session::context_frame::{self, ContextFramePayload};

const BOOTSTRAP_CONTEXT_SLOTS: &[&str] = &[
    "task",
    "story",
    "project",
    "workspace",
    "initial_context",
    "persona",
    "required_context",
    "workflow",
    "workflow_context",
    "story_context",
    "declared_source",
    "static_fragment",
    "requirements",
    "constraints",
    "constraint",
    "codebase",
    "references",
    "project_guidelines",
    "instruction",
    "instruction_append",
    "companion_agents",
];

#[derive(Debug, Clone)]
struct BootstrapContextFrame {
    phase_tag: String,
    fragments: Vec<RuntimeContextFragmentEntry>,
}

impl BootstrapContextFrame {
    fn from_parts(
        bundle: Option<&SessionContextBundle>,
        user_preferences: &[String],
        discovered_guidelines: &[DiscoveredGuideline],
    ) -> Option<Self> {
        let mut fragments = Vec::new();

        if !user_preferences.is_empty() {
            fragments.push(RuntimeContextFragmentEntry {
                slot: "user_preferences".to_string(),
                label: "User Preferences".to_string(),
                source: "settings:user_preferences".to_string(),
                content: user_preferences
                    .iter()
                    .map(|preference| format!("- {preference}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
            });
        }

        for guideline in discovered_guidelines {
            if guideline.content.trim().is_empty() {
                continue;
            }
            fragments.push(RuntimeContextFragmentEntry {
                slot: "project_guidelines".to_string(),
                label: guideline.path.clone(),
                source: "workspace:guideline".to_string(),
                content: format!("### {}\n\n{}", guideline.path, guideline.content),
            });
        }

        let phase_tag = bundle
            .map(|bundle| bundle.phase_tag.clone())
            .unwrap_or_else(|| "bootstrap".to_string());
        if let Some(bundle) = bundle {
            fragments.extend(bundle_runtime_fragments(bundle));
        }

        (!fragments.is_empty()).then_some(Self {
            phase_tag,
            fragments,
        })
    }
}

impl ContextFramePayload for BootstrapContextFrame {
    fn id(&self, created_at_ms: i64) -> String {
        format!("bootstrap-context-{}-{created_at_ms}", self.phase_tag)
    }

    fn kind(&self) -> &'static str {
        "bootstrap_context"
    }

    fn source(&self) -> RuntimeEventSource {
        RuntimeEventSource::RuntimeContextUpdate
    }

    fn phase_node(&self) -> Option<String> {
        Some(self.phase_tag.clone())
    }

    fn delivery_status(&self) -> String {
        "queued_for_transform_context".to_string()
    }

    fn sections(&self) -> Vec<ContextFrameSection> {
        vec![ContextFrameSection::BootstrapContext {
            title: "Bootstrap Context".to_string(),
            summary: format!(
                "Session 启动上下文已注入，本 frame 汇聚 {} 个上下文片段。",
                self.fragments.len()
            ),
            fragments: self.fragments.clone(),
        }]
    }

    fn rendered_text(&self) -> String {
        render_bootstrap_context_text(&self.fragments)
    }
}

pub(crate) fn enqueue_bootstrap_context_frame(
    hook_session: Option<&SharedHookSessionRuntime>,
    bundle: Option<&SessionContextBundle>,
    user_preferences: &[String],
    discovered_guidelines: &[DiscoveredGuideline],
) -> Option<ContextFrame> {
    let hook_session = hook_session?;
    let metadata =
        BootstrapContextFrame::from_parts(bundle, user_preferences, discovered_guidelines)?;
    let frame = context_frame::build_context_frame(&metadata);
    hook_session.enqueue_turn_start_notice(HookTurnStartNotice {
        id: frame.id.clone(),
        created_at_ms: frame.created_at_ms,
        source: RuntimeEventSource::RuntimeContextUpdate,
        content: frame.rendered_text.clone(),
        context_frame: Some(frame.clone()),
    });
    Some(frame)
}

fn bundle_runtime_fragments(bundle: &SessionContextBundle) -> Vec<RuntimeContextFragmentEntry> {
    let mut fragments = bundle
        .filter_for(FragmentScope::RuntimeAgent)
        .filter(|fragment| BOOTSTRAP_CONTEXT_SLOTS.contains(&fragment.slot.as_str()))
        .filter(|fragment| !fragment.content.trim().is_empty())
        .collect::<Vec<_>>();
    fragments.sort_by_key(|fragment| fragment.order);
    fragments.into_iter().map(fragment_entry).collect()
}

fn fragment_entry(fragment: &ContextFragment) -> RuntimeContextFragmentEntry {
    RuntimeContextFragmentEntry {
        slot: fragment.slot.clone(),
        label: fragment.label.clone(),
        source: fragment.source.clone(),
        content: fragment.content.clone(),
    }
}

fn render_bootstrap_context_text(fragments: &[RuntimeContextFragmentEntry]) -> String {
    let mut lines = vec![
        "## Bootstrap Context".to_string(),
        "以下上下文片段已在本轮对话开始前注入，用于建立任务、流程、项目规则与用户偏好。"
            .to_string(),
    ];
    for fragment in fragments {
        let label = if fragment.label.trim().is_empty() {
            fragment.slot.as_str()
        } else {
            fragment.label.as_str()
        };
        lines.push(format!(
            "### {} (`{}`)\nsource: `{}`\n\n{}",
            label,
            fragment.slot,
            fragment.source,
            fragment.content.trim()
        ));
    }
    lines.join("\n\n")
}

#[cfg(test)]
mod tests {
    use agentdash_spi::{ContextFragment, MergeStrategy, SessionContextBundle};

    use super::*;

    fn fragment(slot: &str, content: &str) -> ContextFragment {
        ContextFragment {
            slot: slot.to_string(),
            label: slot.to_string(),
            order: 10,
            strategy: MergeStrategy::Append,
            scope: ContextFragment::default_scope(),
            source: "test".to_string(),
            content: content.to_string(),
        }
    }

    #[test]
    fn bootstrap_frame_filters_runtime_surface_slots() {
        let mut bundle = SessionContextBundle::new(uuid::Uuid::new_v4(), "task_start");
        bundle.merge([
            fragment("task", "## Task\n处理上下文可视化"),
            fragment("vfs", "不应进入 bootstrap frame"),
            fragment("tools", "不应进入 bootstrap frame"),
        ]);

        let frame =
            BootstrapContextFrame::from_parts(Some(&bundle), &["中文交流".to_string()], &[])
                .expect("frame metadata");

        assert_eq!(frame.fragments.len(), 2);
        assert!(frame.fragments.iter().any(|entry| entry.slot == "task"));
        assert!(
            frame
                .fragments
                .iter()
                .any(|entry| entry.slot == "user_preferences")
        );
        assert!(!frame.fragments.iter().any(|entry| entry.slot == "vfs"));
        assert!(frame.rendered_text().contains("Bootstrap Context"));
    }
}
