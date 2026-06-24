use agentdash_domain::common::Mount;
use agentdash_spi::Vfs;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::agent_run_surface::AgentRunRuntimeAddress;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageStreamProjectionRef {
    pub runtime_session_id: String,
    pub trace_kind: MessageStreamTraceKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageStreamTraceKind {
    ConnectorRuntimeSession,
    RestoredTranscript,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrchestrationNodeProjectionInput {
    pub run_id: Uuid,
    pub orchestration_id: Uuid,
    pub node_path: String,
    pub lifecycle_key: String,
    pub attempt: u32,
    pub writable_port_keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrchestrationNodeEvidenceRef {
    pub run_id: Uuid,
    pub orchestration_id: Uuid,
    pub node_path: String,
    pub attempt: u32,
}

impl OrchestrationNodeProjectionInput {
    pub fn evidence_ref(&self) -> OrchestrationNodeEvidenceRef {
        OrchestrationNodeEvidenceRef {
            run_id: self.run_id,
            orchestration_id: self.orchestration_id,
            node_path: self.node_path.clone(),
            attempt: self.attempt,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinLifecycleSkill {
    CanvasSystem,
    CompanionSystem,
    WorkspaceModuleSystem,
    RoutineMemory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuiltinLifecycleSkillPolicy {
    PreserveProjected,
    EnsureAndProject(Vec<BuiltinLifecycleSkill>),
}

impl BuiltinLifecycleSkillPolicy {
    pub fn ensure(skills: impl IntoIterator<Item = BuiltinLifecycleSkill>) -> Self {
        Self::EnsureAndProject(skills.into_iter().collect())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunLifecycleSkillProjectionFacts {
    pub explicit_skill_asset_keys: Vec<String>,
    pub builtin_skills: BuiltinLifecycleSkillPolicy,
}

impl AgentRunLifecycleSkillProjectionFacts {
    pub fn preserve_projected() -> Self {
        Self {
            explicit_skill_asset_keys: Vec::new(),
            builtin_skills: BuiltinLifecycleSkillPolicy::PreserveProjected,
        }
    }

    pub fn ensure(
        explicit_skill_asset_keys: Vec<String>,
        skills: impl IntoIterator<Item = BuiltinLifecycleSkill>,
    ) -> Self {
        Self {
            explicit_skill_asset_keys,
            builtin_skills: BuiltinLifecycleSkillPolicy::ensure(skills),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRunLifecycleSurfaceMode {
    WorkspaceReadSurface,
    LaunchEvidenceSurface,
    CompanionChildSurface,
    WorkflowNodeExecutionSurface,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunLifecycleSurfaceInput {
    pub base_vfs: Option<Vfs>,
    pub address: AgentRunRuntimeAddress,
    pub message_stream: Option<MessageStreamProjectionRef>,
    pub project_id: Uuid,
    pub mode: AgentRunLifecycleSurfaceMode,
    pub explicit_skill_asset_keys: Vec<String>,
    pub builtin_skills: BuiltinLifecycleSkillPolicy,
    pub node_evidence: Option<OrchestrationNodeEvidenceRef>,
    pub node_projection: Option<OrchestrationNodeProjectionInput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunLifecycleSessionEvidenceFacts {
    pub base_vfs: Option<Vfs>,
    pub address: AgentRunRuntimeAddress,
    pub message_stream: MessageStreamProjectionRef,
    pub project_id: Uuid,
    pub node_evidence: Option<OrchestrationNodeEvidenceRef>,
    pub skill_projection: AgentRunLifecycleSkillProjectionFacts,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunLifecycleNodeRuntimeFacts {
    pub base_vfs: Option<Vfs>,
    pub address: AgentRunRuntimeAddress,
    pub message_stream: Option<MessageStreamProjectionRef>,
    pub project_id: Uuid,
    pub node_projection: OrchestrationNodeProjectionInput,
    pub skill_projection: AgentRunLifecycleSkillProjectionFacts,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunLifecycleSurface {
    pub vfs: Vfs,
    pub lifecycle_mount: Mount,
    pub projections: AgentRunLifecycleProjectionSet,
    pub skill_asset_keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunLifecycleProjectionSet {
    pub agent_run_identity: bool,
    pub message_stream: Option<MessageStreamProjectionFacts>,
    pub node_evidence: Option<OrchestrationNodeEvidenceFacts>,
    pub orchestration_node: Option<OrchestrationNodeProjectionFacts>,
    pub skill_assets: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageStreamProjectionFacts {
    pub runtime_session_id: String,
    pub trace_kind: MessageStreamTraceKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrchestrationNodeEvidenceFacts {
    pub run_id: Uuid,
    pub orchestration_id: Uuid,
    pub node_path: String,
    pub attempt: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrchestrationNodeProjectionFacts {
    pub run_id: Uuid,
    pub orchestration_id: Uuid,
    pub node_path: String,
    pub lifecycle_key: String,
    pub attempt: u32,
    pub writable_port_keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LifecycleSurfaceProjectionError {
    #[error("lifecycle surface projection failed: {message}")]
    Projection { message: String },
    #[error(
        "lifecycle surface projection repository failed: operation={operation}, message={message}"
    )]
    Repository {
        operation: &'static str,
        message: String,
    },
}

#[async_trait]
pub trait LifecycleSurfaceProjectionPort: Send + Sync {
    async fn project_lifecycle_surface(
        &self,
        input: AgentRunLifecycleSurfaceInput,
    ) -> Result<AgentRunLifecycleSurface, LifecycleSurfaceProjectionError>;
}
