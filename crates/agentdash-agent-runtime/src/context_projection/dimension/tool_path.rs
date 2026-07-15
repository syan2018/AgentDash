use agentdash_agent_protocol::ContextFrameSection;

use super::ProjectedSurfaceDimension;
use crate::context_projection::surface_state::NormalizedContextSurfaceDelta;

pub(super) fn project(
    delta: &NormalizedContextSurfaceDelta,
    phase_node: &str,
) -> Option<ProjectedSurfaceDimension> {
    let blocked = &delta.excluded_tool_paths.added;
    let unblocked = &delta.excluded_tool_paths.removed;
    let whitelisted = &delta.included_tool_paths.added;
    let removed_whitelist = &delta.included_tool_paths.removed;
    if blocked.is_empty()
        && unblocked.is_empty()
        && whitelisted.is_empty()
        && removed_whitelist.is_empty()
    {
        return None;
    }
    let mut lines = vec![format!(
        "## Tool Path Changes — Step Transition: {phase_node}"
    )];
    append_path_lines(&mut lines, "Blocked tool paths", blocked, "不再暴露");
    append_path_lines(&mut lines, "Unblocked tool paths", unblocked, "重新暴露");
    append_path_lines(
        &mut lines,
        "Whitelisted tool paths",
        whitelisted,
        "进入白名单",
    );
    append_path_lines(
        &mut lines,
        "Removed whitelist paths",
        removed_whitelist,
        "移出白名单",
    );
    Some(ProjectedSurfaceDimension {
        section: ContextFrameSection::ToolPathDelta {
            blocked_tool_paths: blocked.clone(),
            unblocked_tool_paths: unblocked.clone(),
            whitelisted_tool_paths: whitelisted.clone(),
            removed_whitelist_paths: removed_whitelist.clone(),
        },
        rendered_text: lines.join("\n"),
    })
}

fn append_path_lines(lines: &mut Vec<String>, title: &str, values: &[String], suffix: &str) {
    if values.is_empty() {
        return;
    }
    lines.push(format!("- {title}:"));
    lines.extend(
        values
            .iter()
            .map(|value| format!("  - `{value}` — {suffix}")),
    );
}
