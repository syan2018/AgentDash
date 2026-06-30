use std::path::PathBuf;

use agentdash_domain::workflow::{ActivityDefinition, AgentProcedure, LifecycleRun, WorkflowGraph};
use agentdash_spi::{AgentConfig, RuntimeMcpServer};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutineLaunchSource {
    pub routine_id: Uuid,
    pub execution_id: Uuid,
    pub trigger_source: String,
    pub entity_key: Option<String>,
}

#[derive(Clone)]
pub struct CompanionLaunchWorkflowSource {
    pub run: LifecycleRun,
    pub orchestration_id: Uuid,
    pub node_path: String,
    pub attempt: u32,
    pub lifecycle: WorkflowGraph,
    pub activity: ActivityDefinition,
    pub workflow: Option<AgentProcedure>,
}

#[derive(Clone)]
pub struct CompanionLaunchSource {
    pub parent_session_id: String,
    pub selected_project_agent_id: Option<Uuid>,
    pub selected_agent_key: Option<String>,
    pub slice_mode: agentdash_spi::CompanionSliceMode,
    pub companion_executor_config: AgentConfig,
    pub dispatch_prompt: String,
    pub workflow: Option<CompanionLaunchWorkflowSource>,
}

#[derive(Clone)]
pub struct LocalRelayLaunchPayload {
    pub mcp_servers: Vec<RuntimeMcpServer>,
    pub workspace_root: PathBuf,
}

#[derive(Clone)]
pub enum LaunchModifier {
    Companion(Box<CompanionLaunchSource>),
    Routine(RoutineLaunchSource),
    LocalRelay(LocalRelayLaunchPayload),
    HookAutoResume,
}
