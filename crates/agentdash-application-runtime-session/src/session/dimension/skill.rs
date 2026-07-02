//! Skill 维度 — 追踪 skill 的增删与变更。

use agentdash_spi::SkillClusterMeta;
use agentdash_spi::context::capability::SkillEntry;
use agentdash_spi::context_usage_kind;
use agentdash_spi::hooks::{ContextFrameSection, RuntimeSkillEntry};

use super::DimensionDelta;
use agentdash_spi::CapabilityStateDelta;

#[derive(Debug, Clone)]
pub(crate) struct SkillDimensionDelta {
    pub added: Vec<RuntimeSkillEntry>,
    pub removed: Vec<RuntimeSkillEntry>,
    pub changed: Vec<RuntimeSkillEntry>,
    pub cluster_meta: Vec<SkillClusterMeta>,
}

impl SkillDimensionDelta {
    pub fn from_state_delta(
        state_delta: Option<&CapabilityStateDelta>,
        skill_entries: &[SkillEntry],
        cluster_meta: &[SkillClusterMeta],
    ) -> Option<Box<dyn DimensionDelta>> {
        let state_delta = state_delta?;
        if state_delta.skills.is_empty() {
            return None;
        }
        let lookup = |name: &str| -> RuntimeSkillEntry {
            if let Some(entry) = skill_entries
                .iter()
                .find(|e| e.capability_key_or_name() == name)
            {
                RuntimeSkillEntry {
                    name: entry.name.clone(),
                    capability_key: entry.capability_key.clone(),
                    provider_key: entry.provider_key.clone(),
                    local_name: entry.local_name.clone(),
                    display_name: entry.display_name.clone(),
                    description: entry.description.clone(),
                    file_path: entry.file_path.clone(),
                    base_dir: entry.base_dir.clone(),
                    exposure: entry.exposure,
                    disable_model_invocation: entry.disable_model_invocation,
                    context_usage_kind: Some(context_usage_kind::SKILLS.to_string()),
                }
            } else {
                RuntimeSkillEntry {
                    name: name.to_string(),
                    capability_key: name.to_string(),
                    provider_key: String::new(),
                    local_name: name.to_string(),
                    display_name: None,
                    description: String::new(),
                    file_path: String::new(),
                    base_dir: None,
                    exposure: agentdash_spi::SkillContextExposure::DefaultExposed,
                    disable_model_invocation: false,
                    context_usage_kind: Some(context_usage_kind::SKILLS.to_string()),
                }
            }
        };
        let delta = Self {
            added: state_delta.skills.added.iter().map(|n| lookup(n)).collect(),
            removed: state_delta
                .skills
                .removed
                .iter()
                .map(|n| lookup(n))
                .collect(),
            changed: state_delta
                .skills
                .changed
                .iter()
                .map(|n| lookup(n))
                .collect(),
            cluster_meta: cluster_meta.to_vec(),
        };
        Some(Box::new(delta))
    }
}

impl DimensionDelta for SkillDimensionDelta {
    fn has_changes(&self) -> bool {
        !self.added.is_empty() || !self.removed.is_empty() || !self.changed.is_empty()
    }

    fn to_section(&self) -> ContextFrameSection {
        ContextFrameSection::SkillDelta {
            added_skills: self.added.clone(),
            removed_skills: self.removed.clone(),
            changed_skills: self.changed.clone(),
        }
    }

    fn render_text(&self, phase_node: Option<&str>) -> String {
        let mut lines = vec![match phase_node {
            Some(node) => format!("## Skill Delta — Step Transition: {node}"),
            None => "## Skill Delta".to_string(),
        }];
        render_grouped_skill_lines(
            &mut lines,
            "Added Skills",
            &self.added,
            "已加入",
            &self.cluster_meta,
        );
        render_grouped_skill_lines(
            &mut lines,
            "Removed Skills",
            &self.removed,
            "已移除",
            &self.cluster_meta,
        );
        render_grouped_skill_lines(
            &mut lines,
            "Changed Skills",
            &self.changed,
            "定义已变更",
            &self.cluster_meta,
        );
        lines.join("\n")
    }
}

fn render_grouped_skill_lines(
    lines: &mut Vec<String>,
    title: &str,
    values: &[RuntimeSkillEntry],
    suffix: &str,
    cluster_meta: &[SkillClusterMeta],
) {
    if values.is_empty() {
        return;
    }

    lines.push(format!("### {title}"));

    if cluster_meta.is_empty() {
        for skill in values {
            lines.push(format_skill_line(skill, suffix));
        }
        return;
    }

    // Group skills by provider_key, preserving cluster registration order.
    let mut rendered_providers: Vec<&str> = Vec::new();
    for cluster in cluster_meta {
        let group: Vec<&RuntimeSkillEntry> = values
            .iter()
            .filter(|s| s.provider_key == cluster.provider_key)
            .collect();
        if group.is_empty() {
            continue;
        }
        rendered_providers.push(&cluster.provider_key);
        lines.push(format!("#### {}", cluster.display_name));
        if let Some(summary) = &cluster.model_summary {
            lines.push(format!("> {summary}"));
        }
        for skill in &group {
            lines.push(format_skill_line(skill, suffix));
        }
    }

    // Render any skills whose provider_key didn't match a known cluster (fallback).
    let ungrouped: Vec<&RuntimeSkillEntry> = values
        .iter()
        .filter(|s| !rendered_providers.contains(&s.provider_key.as_str()))
        .collect();
    if !ungrouped.is_empty() {
        for skill in &ungrouped {
            lines.push(format_skill_line(skill, suffix));
        }
    }
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
