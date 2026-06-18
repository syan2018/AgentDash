//! Companion Agent roster 维度 — 追踪可派发协作 Agent 的增删与变更。

use agentdash_spi::context::capability::CompanionAgentEntry;
use agentdash_spi::context_usage_kind;
use agentdash_spi::hooks::{ContextFrameSection, RuntimeCompanionAgentEntry};

use super::DimensionDelta;
use crate::session::CapabilityStateDelta;

#[derive(Debug, Clone)]
pub(crate) struct CompanionAgentDimensionDelta {
    pub added: Vec<RuntimeCompanionAgentEntry>,
    pub removed: Vec<String>,
    pub changed: Vec<RuntimeCompanionAgentEntry>,
    pub effective: Vec<RuntimeCompanionAgentEntry>,
}

impl CompanionAgentDimensionDelta {
    pub fn from_state_delta(
        state_delta: Option<&CapabilityStateDelta>,
        companion_agents: &[CompanionAgentEntry],
    ) -> Option<Box<dyn DimensionDelta>> {
        let state_delta = state_delta?;
        if state_delta.companion_agents.is_empty() {
            return None;
        }

        let lookup = |agent_key: &str| -> RuntimeCompanionAgentEntry {
            companion_agents
                .iter()
                .find(|agent| agent.name == agent_key)
                .map(runtime_companion_agent_entry)
                .unwrap_or_else(|| RuntimeCompanionAgentEntry {
                    agent_key: agent_key.to_string(),
                    executor: String::new(),
                    display_name: String::new(),
                    context_usage_kind: Some(context_usage_kind::AGENTS.to_string()),
                })
        };

        Some(Box::new(Self {
            added: state_delta
                .companion_agents
                .added
                .iter()
                .map(|key| lookup(key))
                .collect(),
            removed: state_delta.companion_agents.removed.clone(),
            changed: state_delta
                .companion_agents
                .changed
                .iter()
                .map(|key| lookup(key))
                .collect(),
            effective: companion_agents
                .iter()
                .map(runtime_companion_agent_entry)
                .collect(),
        }))
    }
}

impl DimensionDelta for CompanionAgentDimensionDelta {
    fn has_changes(&self) -> bool {
        !self.added.is_empty() || !self.removed.is_empty() || !self.changed.is_empty()
    }

    fn to_section(&self) -> ContextFrameSection {
        ContextFrameSection::CompanionAgentRosterDelta {
            added_agents: self.added.clone(),
            removed_agent_keys: self.removed.clone(),
            changed_agents: self.changed.clone(),
            effective_agents: self.effective.clone(),
        }
    }

    fn render_text(&self, phase_node: Option<&str>) -> String {
        let mut lines = vec![match phase_node {
            Some(node) => format!("## Companion Agent Roster Delta — Step Transition: {node}"),
            None => "## Companion Agent Roster Delta".to_string(),
        }];

        append_agent_lines(&mut lines, "Added Companion Agents", &self.added, "已加入");
        append_key_lines(
            &mut lines,
            "Removed Companion Agents",
            &self.removed,
            "已移除",
        );
        append_agent_lines(
            &mut lines,
            "Changed Companion Agents",
            &self.changed,
            "已变更",
        );

        lines.push("### Effective Companion Agents".to_string());
        if self.effective.is_empty() {
            lines.push("- （无）".to_string());
        } else {
            for agent in &self.effective {
                lines.push(format_agent_line(agent, "可调用"));
            }
        }

        lines.join("\n")
    }
}

fn runtime_companion_agent_entry(agent: &CompanionAgentEntry) -> RuntimeCompanionAgentEntry {
    RuntimeCompanionAgentEntry {
        agent_key: agent.name.clone(),
        executor: agent.executor.clone(),
        display_name: agent.display_name.clone(),
        context_usage_kind: Some(context_usage_kind::AGENTS.to_string()),
    }
}

fn append_agent_lines(
    lines: &mut Vec<String>,
    title: &str,
    values: &[RuntimeCompanionAgentEntry],
    suffix: &str,
) {
    if values.is_empty() {
        return;
    }
    lines.push(format!("### {title}"));
    for agent in values {
        lines.push(format_agent_line(agent, suffix));
    }
}

fn append_key_lines(lines: &mut Vec<String>, title: &str, values: &[String], suffix: &str) {
    if values.is_empty() {
        return;
    }
    lines.push(format!("### {title}"));
    for value in values {
        lines.push(format!("- agent_key: `{value}` — {suffix}"));
    }
}

fn format_agent_line(agent: &RuntimeCompanionAgentEntry, suffix: &str) -> String {
    let display = if agent.display_name.is_empty() {
        String::new()
    } else {
        format!("; display_name: {}", agent.display_name)
    };
    let executor = if agent.executor.is_empty() {
        String::new()
    } else {
        format!("; executor: `{}`", agent.executor)
    };
    format!(
        "- agent_key: `{}`{executor}{display} — {suffix}",
        agent.agent_key
    )
}
