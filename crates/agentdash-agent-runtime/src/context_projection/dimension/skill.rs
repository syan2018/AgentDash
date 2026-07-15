use agentdash_agent_protocol::{ContextFrameSection, RuntimeSkillEntry};

use super::ProjectedSurfaceDimension;
use crate::context_projection::surface_state::{
    NormalizedContextSurfaceDelta, NormalizedContextSurfaceState, NormalizedSkillCluster,
};

pub(super) fn project(
    delta: &NormalizedContextSurfaceDelta,
    target: &NormalizedContextSurfaceState,
    phase_node: &str,
) -> Option<ProjectedSurfaceDimension> {
    if delta.skills.is_empty() {
        return None;
    }
    let lookup = |key: &str| {
        target
            .skills
            .get(key)
            .cloned()
            .unwrap_or_else(|| fallback_entry(key))
    };
    let added = delta
        .skills
        .added
        .iter()
        .map(|key| lookup(key))
        .collect::<Vec<_>>();
    let removed = delta
        .skills
        .removed
        .iter()
        .map(|key| lookup(key))
        .collect::<Vec<_>>();
    let changed = delta
        .skills
        .changed
        .iter()
        .map(|key| lookup(key))
        .collect::<Vec<_>>();

    let mut lines = vec![format!("## Skill Delta — Step Transition: {phase_node}")];
    render_grouped_skill_lines(
        &mut lines,
        "Added Skills",
        &added,
        "已加入",
        &target.skill_clusters,
    );
    render_grouped_skill_lines(
        &mut lines,
        "Removed Skills",
        &removed,
        "已移除",
        &target.skill_clusters,
    );
    render_grouped_skill_lines(
        &mut lines,
        "Changed Skills",
        &changed,
        "定义已变更",
        &target.skill_clusters,
    );
    Some(ProjectedSurfaceDimension {
        section: ContextFrameSection::SkillDelta {
            added_skills: added,
            removed_skills: removed,
            changed_skills: changed,
        },
        rendered_text: lines.join("\n"),
    })
}

fn fallback_entry(key: &str) -> RuntimeSkillEntry {
    RuntimeSkillEntry {
        name: key.to_string(),
        capability_key: key.to_string(),
        provider_key: String::new(),
        local_name: key.to_string(),
        display_name: None,
        description: String::new(),
        file_path: String::new(),
        base_dir: None,
        exposure: agentdash_agent_protocol::SkillContextExposure::DefaultExposed,
        disable_model_invocation: false,
        context_usage_kind: Some("skills".to_string()),
    }
}

fn render_grouped_skill_lines(
    lines: &mut Vec<String>,
    title: &str,
    values: &[RuntimeSkillEntry],
    suffix: &str,
    clusters: &[NormalizedSkillCluster],
) {
    if values.is_empty() {
        return;
    }
    lines.push(format!("### {title}"));
    if clusters.is_empty() {
        lines.extend(values.iter().map(|skill| format_skill_line(skill, suffix)));
        return;
    }
    let mut rendered_providers = Vec::new();
    for cluster in clusters {
        let group = values
            .iter()
            .filter(|skill| skill.provider_key == cluster.provider_key)
            .collect::<Vec<_>>();
        if group.is_empty() {
            continue;
        }
        rendered_providers.push(cluster.provider_key.as_str());
        lines.push(format!("#### {}", cluster.display_name));
        if let Some(summary) = &cluster.model_summary {
            lines.push(format!("> {summary}"));
        }
        lines.extend(group.iter().map(|skill| format_skill_line(skill, suffix)));
    }
    lines.extend(
        values
            .iter()
            .filter(|skill| !rendered_providers.contains(&skill.provider_key.as_str()))
            .map(|skill| format_skill_line(skill, suffix)),
    );
}

fn format_skill_line(skill: &RuntimeSkillEntry, suffix: &str) -> String {
    if skill.description.is_empty() {
        format!("- `{}` — {suffix}", skill.name)
    } else {
        format!(
            "- `{}`: {} (path: `{}`) — {suffix}",
            skill.name, skill.description, skill.file_path
        )
    }
}
