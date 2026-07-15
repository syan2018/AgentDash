use agentdash_agent_protocol::{ContextFrameSection, RuntimeCompanionAgentEntry};

use super::ProjectedSurfaceDimension;
use crate::context_projection::surface_state::{
    NormalizedContextSurfaceDelta, NormalizedContextSurfaceState,
};

pub(super) fn project(
    delta: &NormalizedContextSurfaceDelta,
    target: &NormalizedContextSurfaceState,
    phase_node: &str,
) -> Option<ProjectedSurfaceDimension> {
    let collaboration_enabled = target.capability_keys.contains("collaboration");
    let collaboration_changed = delta
        .capability_keys
        .added
        .iter()
        .chain(delta.capability_keys.removed.iter())
        .any(|capability| capability == "collaboration");
    let roster_changed = !delta.companion_agents.is_empty();
    if !roster_changed && !(collaboration_enabled && collaboration_changed) {
        return None;
    }

    let lookup = |key: &str| {
        target
            .companion_agents
            .get(key)
            .cloned()
            .unwrap_or_else(|| fallback_agent(key))
    };
    let added = delta
        .companion_agents
        .added
        .iter()
        .map(|key| lookup(key))
        .collect::<Vec<_>>();
    let changed = delta
        .companion_agents
        .changed
        .iter()
        .map(|key| lookup(key))
        .collect::<Vec<_>>();
    let effective = ordered_effective_agents(target);

    let mut lines = vec![format!(
        "## Companion Agent Roster Delta — Step Transition: {phase_node}"
    )];
    append_agent_lines(&mut lines, "Added Companion Agents", &added, "已加入");
    append_key_lines(
        &mut lines,
        "Removed Companion Agents",
        &delta.companion_agents.removed,
        "已移除",
    );
    append_agent_lines(&mut lines, "Changed Companion Agents", &changed, "已变更");
    lines.push("### Effective Companion Agents".to_string());
    if effective.is_empty() {
        lines.push("- （无）".to_string());
    } else {
        lines.push(
            "- 调用提示：使用 `companion_request` 且 `target: \"sub\"` 时，必须把下列精确 `agent_key` 填入 `payload.agent_key`；不要使用 executor 或 display_name。"
                .to_string(),
        );
        lines.extend(
            effective
                .iter()
                .map(|agent| format_agent_line(agent, "可调用")),
        );
    }
    Some(ProjectedSurfaceDimension {
        section: ContextFrameSection::CompanionAgentRosterDelta {
            added_agents: added,
            removed_agent_keys: delta.companion_agents.removed.clone(),
            changed_agents: changed,
            effective_agents: effective,
        },
        rendered_text: lines.join("\n"),
    })
}

fn ordered_effective_agents(
    target: &NormalizedContextSurfaceState,
) -> Vec<RuntimeCompanionAgentEntry> {
    let mut rendered = target
        .companion_agent_order
        .iter()
        .filter_map(|key| target.companion_agents.get(key))
        .cloned()
        .collect::<Vec<_>>();
    for (key, agent) in &target.companion_agents {
        if !target.companion_agent_order.contains(key) {
            rendered.push(agent.clone());
        }
    }
    rendered
}

fn fallback_agent(key: &str) -> RuntimeCompanionAgentEntry {
    RuntimeCompanionAgentEntry {
        agent_key: key.to_string(),
        executor: String::new(),
        display_name: String::new(),
        context_usage_kind: Some("agents".to_string()),
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
    lines.extend(values.iter().map(|agent| format_agent_line(agent, suffix)));
}

fn append_key_lines(lines: &mut Vec<String>, title: &str, values: &[String], suffix: &str) {
    if values.is_empty() {
        return;
    }
    lines.push(format!("### {title}"));
    lines.extend(
        values
            .iter()
            .map(|key| format!("- agent_key: `{key}` — {suffix}")),
    );
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
