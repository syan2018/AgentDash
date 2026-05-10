use agentdash_spi::context::injection::FragmentScope;
use agentdash_spi::hooks::{
    ContextFrame, ContextFrameSection, RuntimeContextFragmentEntry, RuntimeEventSource,
};
use agentdash_spi::{ContextFragment, DiscoveredGuideline, MISSION_CONTEXT_SLOTS};

use crate::session::context_frame::{self, ContextFramePayload};

#[derive(Debug, Clone)]
struct MissionContextFrame {
    phase_tag: String,
    apply_mode_override: Option<String>,
    fragments: Vec<RuntimeContextFragmentEntry>,
}

impl MissionContextFrame {
    fn from_parts(
        phase_tag: Option<&str>,
        runtime_fragments: &[ContextFragment],
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

        let phase_tag = phase_tag.unwrap_or("bootstrap").to_string();
        fragments.extend(mission_runtime_fragments(runtime_fragments));

        (!fragments.is_empty()).then_some(Self {
            phase_tag,
            apply_mode_override: None,
            fragments,
        })
    }
}

impl ContextFramePayload for MissionContextFrame {
    fn id(&self, created_at_ms: i64) -> String {
        format!("mission-context-{}-{created_at_ms}", self.phase_tag)
    }

    fn kind(&self) -> &'static str {
        "mission_context"
    }

    fn source(&self) -> RuntimeEventSource {
        RuntimeEventSource::RuntimeContextUpdate
    }

    fn phase_node(&self) -> Option<String> {
        Some(self.phase_tag.clone())
    }

    fn apply_mode(&self) -> Option<String> {
        self.apply_mode_override.clone()
    }

    fn delivery_status(&self) -> String {
        "queued_for_transform_context".to_string()
    }

    fn sections(&self) -> Vec<ContextFrameSection> {
        vec![ContextFrameSection::MissionContext {
            title: "Mission Context".to_string(),
            summary: format!(
                "当前任务上下文已注入，本 frame 汇聚 {} 个上下文片段。",
                self.fragments.len()
            ),
            fragments: self.fragments.clone(),
        }]
    }

    fn rendered_text(&self) -> String {
        render_mission_context_text(&self.fragments)
    }
}

pub(crate) fn build_mission_context_frame(
    phase_tag: Option<&str>,
    runtime_fragments: &[ContextFragment],
    user_preferences: &[String],
    discovered_guidelines: &[DiscoveredGuideline],
) -> Option<ContextFrame> {
    let metadata = MissionContextFrame::from_parts(
        phase_tag,
        runtime_fragments,
        user_preferences,
        discovered_guidelines,
    )?;
    Some(context_frame::build_context_frame(&metadata))
}

/// Runtime transition 路径的 mission context frame 构建入口。
///
/// 与 `build_mission_context_frame` 共享同一个 `MissionContextFrame` payload 和渲染逻辑，
/// 但不需要 user_preferences / discovered_guidelines（它们在 bootstrap 时已注入过）。
/// 此函数接收已桥接为 `ContextFragment` 的数据，保证整个链路走 Fragment → Frame 单一路径。
pub(crate) fn build_runtime_mission_context_frame(
    phase_tag: &str,
    apply_mode: Option<&str>,
    runtime_fragments: &[ContextFragment],
) -> Option<ContextFrame> {
    let fragments = mission_runtime_fragments(runtime_fragments);
    if fragments.is_empty() {
        return None;
    }
    let metadata = MissionContextFrame {
        phase_tag: phase_tag.to_string(),
        apply_mode_override: apply_mode.map(ToString::to_string),
        fragments,
    };
    Some(context_frame::build_context_frame(&metadata))
}

fn mission_runtime_fragments(fragments: &[ContextFragment]) -> Vec<RuntimeContextFragmentEntry> {
    let mut fragments = fragments
        .iter()
        .filter(|fragment| fragment.scope.contains(FragmentScope::RuntimeAgent))
        .filter(|fragment| MISSION_CONTEXT_SLOTS.contains(&fragment.slot.as_str()))
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

fn render_mission_context_text(fragments: &[RuntimeContextFragmentEntry]) -> String {
    let mut lines = vec![
        "## Mission Context".to_string(),
        "以下上下文片段已在本轮对话开始前注入，用于约束任务目标、工作流要求与项目规则。"
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
    use agentdash_spi::{ContextFragment, MergeStrategy};

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
    fn mission_context_frame_filters_runtime_slots() {
        let runtime_fragments = vec![
            fragment("task", "## Task\n处理上下文可视化"),
            fragment("vfs", "不应进入 mission context frame"),
            fragment("tools", "不应进入 mission context frame"),
        ];

        let frame = MissionContextFrame::from_parts(
            Some("task_start"),
            &runtime_fragments,
            &["中文交流".to_string()],
            &[],
        )
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
        assert!(frame.rendered_text().contains("Mission Context"));
    }
}
