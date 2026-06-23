//! Skill 维度 — 追踪 skill 的增删与变更。

use agentdash_spi::context::capability::SkillEntry;
use agentdash_spi::context_usage_kind;
use agentdash_spi::hooks::{ContextFrameSection, RuntimeSkillEntry};

use super::DimensionDelta;
use crate::agent_run::runtime_capability::CapabilityStateDelta;

#[derive(Debug, Clone)]
pub(crate) struct SkillDimensionDelta {
    pub added: Vec<RuntimeSkillEntry>,
    pub removed: Vec<RuntimeSkillEntry>,
    pub changed: Vec<RuntimeSkillEntry>,
}

impl SkillDimensionDelta {
    pub fn from_state_delta(
        state_delta: Option<&CapabilityStateDelta>,
        skill_entries: &[SkillEntry],
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
        append_skill_lines(&mut lines, "Added Skills", &self.added, "已加入");
        append_skill_lines(&mut lines, "Removed Skills", &self.removed, "已移除");
        append_skill_lines(&mut lines, "Changed Skills", &self.changed, "定义已变更");
        lines.join("\n")
    }
}

fn append_skill_lines(
    lines: &mut Vec<String>,
    title: &str,
    values: &[RuntimeSkillEntry],
    suffix: &str,
) {
    if values.is_empty() {
        return;
    }
    lines.push(format!("### {title}"));
    for skill in values {
        if skill.description.is_empty() {
            lines.push(format!("- `{}` — {suffix}", skill.name));
        } else {
            lines.push(format!(
                "- `{}`: {} (path: `{}`) — {suffix}",
                skill.name, skill.description, skill.file_path
            ));
        }
    }
}
