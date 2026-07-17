mod capability_key;
mod companion_agent;
mod mcp_server;
mod memory;
mod skill;
mod tool_path;
mod tool_schema;
mod vfs;

use agentdash_agent_protocol::ContextFrameSection;

use super::surface_state::{NormalizedContextSurfaceDelta, NormalizedContextSurfaceState};

#[derive(Debug, Clone, PartialEq)]
pub(super) struct ProjectedSurfaceDimension {
    pub section: ContextFrameSection,
    pub rendered_text: String,
}

pub(super) fn project_all(
    delta: &NormalizedContextSurfaceDelta,
    previous: &NormalizedContextSurfaceState,
    target: &NormalizedContextSurfaceState,
    phase_node: Option<&str>,
) -> Vec<ProjectedSurfaceDimension> {
    [
        capability_key::project(delta, target, phase_node),
        tool_path::project(delta, phase_node),
        mcp_server::project(delta, phase_node),
        companion_agent::project(delta, target, phase_node),
        vfs::project(delta, phase_node),
        memory::project(delta, previous, target, phase_node),
        skill::project(delta, target, phase_node),
        tool_schema::project(delta, target, phase_node),
    ]
    .into_iter()
    .flatten()
    .collect()
}

pub(super) fn surface_update_heading(title: &str, phase_node: Option<&str>) -> String {
    match phase_node {
        Some(phase_node) => format!("## {title} — Step Transition: {phase_node}"),
        None => format!("## {title} — Runtime Surface Update"),
    }
}
