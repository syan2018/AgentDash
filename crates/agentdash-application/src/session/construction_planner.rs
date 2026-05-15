use std::path::PathBuf;

use super::construction::{
    SessionConstructionLaunchInput, SessionConstructionPlan, SessionConstructionTraceEntry,
};
use super::ownership::ResolvedSessionOwner;

pub(super) struct SessionConstructionPlanner;

pub(super) struct SessionConstructionPlannerInput {
    pub session_id: String,
    pub owner: Option<ResolvedSessionOwner>,
    pub source: super::construction::SourceContractPlan,
    pub working_dir_input: Option<String>,
    pub working_directory: PathBuf,
    pub executor_config: agentdash_domain::common::AgentConfig,
    pub vfs: Option<agentdash_spi::Vfs>,
    pub context_bundle: Option<agentdash_spi::SessionContextBundle>,
    pub identity: Option<agentdash_spi::AuthIdentity>,
    pub mcp_servers: Vec<agentdash_spi::SessionMcpServer>,
    pub capability_state: agentdash_spi::CapabilityState,
    pub session_capabilities: agentdash_spi::SessionBaselineCapabilities,
    pub prompt_lifecycle: super::types::SessionPromptLifecycle,
    pub capability_source: super::launch::LaunchCapabilitySource,
    pub vfs_source: super::launch::LaunchVfsSource,
}

impl SessionConstructionPlanner {
    pub fn plan_launch(input: SessionConstructionPlannerInput) -> Option<SessionConstructionPlan> {
        let owner = input.owner?;
        Some(SessionConstructionPlan::from_launch(
            SessionConstructionLaunchInput {
                session_id: input.session_id,
                owner,
                source: input.source,
                workspace_id: None,
                working_dir_input: input.working_dir_input,
                working_directory: input.working_directory,
                executor_config: input.executor_config,
                vfs: input.vfs,
                runtime_surface: None,
                context_bundle: input.context_bundle,
                context_snapshot: None,
                identity: input.identity,
                mcp_servers: input.mcp_servers,
                capability_state: input.capability_state,
                session_capabilities: Some(input.session_capabilities),
                trace_entries: vec![
                    SessionConstructionTraceEntry {
                        stage: "launch_lifecycle",
                        source: format!("{:?}", input.prompt_lifecycle),
                    },
                    SessionConstructionTraceEntry {
                        stage: "capability_source",
                        source: format!("{:?}", input.capability_source),
                    },
                    SessionConstructionTraceEntry {
                        stage: "vfs_source",
                        source: format!("{:?}", input.vfs_source),
                    },
                ],
            },
        ))
    }
}
