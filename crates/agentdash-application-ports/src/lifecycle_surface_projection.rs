use std::collections::BTreeSet;

use agentdash_domain::common::{Mount, MountCapability};
use agentdash_domain::workflow::{
    ActivityDefinition, ActivityExecutorSpec, AgentActivityExecutorSpec, AgentProcedure,
    AgentProcedureContract, AgentReusePolicy, BashExecExecutorSpec, ExecutorSpec,
    FunctionActivityExecutorSpec, LifecycleNodeType, LifecycleRun, MountDirective,
    OrchestrationInstance, OrchestrationSourceRef, PlanNode, RuntimeNodeState,
    RuntimeSessionPolicy,
};
use agentdash_platform_spi::Vfs;
use agentdash_platform_spi::{CapabilityState, RuntimeMcpServer};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::agent_run_surface::AgentRunRuntimeAddress;

pub const LIFECYCLE_MOUNT_ID: &str = "lifecycle";
pub const PROVIDER_LIFECYCLE_VFS: &str = "lifecycle_vfs";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleProjectionIdentity {
    pub graph_id: Option<Uuid>,
    pub key: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct ActiveWorkflowProjection {
    pub run: LifecycleRun,
    pub orchestration_id: Uuid,
    pub node_path: String,
    pub lifecycle_graph_id: Option<Uuid>,
    pub lifecycle_key: String,
    pub lifecycle_name: String,
    pub active_activity: ActivityDefinition,
    pub active_attempt: RuntimeNodeState,
    pub active_node_type: LifecycleNodeType,
    pub active_procedure_key: Option<String>,
    pub snapshot_contract: Option<AgentProcedureContract>,
    pub primary_workflow: Option<AgentProcedure>,
}

impl ActiveWorkflowProjection {
    pub fn active_contract(&self) -> Option<&AgentProcedureContract> {
        self.snapshot_contract.as_ref().or_else(|| {
            self.primary_workflow
                .as_ref()
                .map(|workflow| &workflow.contract)
        })
    }

    pub fn advance_label(&self) -> &'static str {
        if self.active_contract().is_some() {
            "auto"
        } else {
            "manual"
        }
    }
}

pub fn activity_definition_from_plan_node(plan_node: &PlanNode) -> ActivityDefinition {
    let executor = match &plan_node.executor {
        Some(ExecutorSpec::AgentProcedure {
            procedure,
            agent_reuse_policy,
            runtime_session_policy,
        }) => ActivityExecutorSpec::Agent(AgentActivityExecutorSpec {
            procedure_key: procedure
                .procedure_key()
                .unwrap_or("__inline_agent_procedure")
                .to_string(),
            agent_reuse_policy: *agent_reuse_policy,
            runtime_session_policy: *runtime_session_policy,
        }),
        Some(ExecutorSpec::Function { spec }) => ActivityExecutorSpec::Function(spec.clone()),
        Some(ExecutorSpec::Human { spec }) => ActivityExecutorSpec::Human(spec.clone()),
        Some(ExecutorSpec::LocalEffect { .. })
        | Some(ExecutorSpec::ExtensionAction { .. })
        | None => ActivityExecutorSpec::Function(FunctionActivityExecutorSpec::BashExec(
            BashExecExecutorSpec {
                command: "true".to_string(),
                args: Vec::new(),
                working_directory: None,
            },
        )),
    };

    ActivityDefinition {
        key: plan_node.node_path.clone(),
        description: plan_node
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("description"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .or_else(|| plan_node.label.clone())
            .unwrap_or_default(),
        executor,
        input_ports: plan_node.input_ports.clone(),
        output_ports: plan_node.output_ports.clone(),
        completion_policy: plan_node.completion_policy.clone().unwrap_or_default(),
        iteration_policy: plan_node.iteration_policy.clone().unwrap_or_default(),
        join_policy: plan_node.join_policy.unwrap_or_default(),
    }
}

pub fn lifecycle_identity_from_orchestration(
    orchestration: &OrchestrationInstance,
) -> LifecycleProjectionIdentity {
    let metadata_source = orchestration
        .plan_snapshot
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("source"));
    let key = metadata_source
        .and_then(|source| source.get("key"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| lifecycle_key_from_source_ref(&orchestration.source_ref));
    let name = metadata_source
        .and_then(|source| source.get("name"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| key.clone());
    let graph_id = match &orchestration.source_ref {
        OrchestrationSourceRef::WorkflowGraph { graph_id, .. } => Some(*graph_id),
        _ => None,
    };

    LifecycleProjectionIdentity {
        graph_id,
        key,
        name,
    }
}

fn lifecycle_key_from_source_ref(source_ref: &OrchestrationSourceRef) -> String {
    match source_ref {
        OrchestrationSourceRef::WorkflowGraph { graph_id, .. } => {
            format!("workflow_graph:{graph_id}")
        }
        OrchestrationSourceRef::RunScriptArtifact { artifact_id, .. } => {
            format!("run_script:{artifact_id}")
        }
        OrchestrationSourceRef::WorkflowScript { script_id, .. } => {
            format!("workflow_script:{script_id}")
        }
        OrchestrationSourceRef::Inline { source_digest } => {
            format!("inline:{}", digest_suffix(source_digest))
        }
    }
}

fn digest_suffix(digest: &str) -> &str {
    digest
        .strip_prefix("sha256:")
        .unwrap_or(digest)
        .get(..12)
        .unwrap_or(digest)
}

pub fn derive_agent_node_facts(plan_node: &PlanNode) -> (Option<String>, LifecycleNodeType) {
    match &plan_node.executor {
        Some(ExecutorSpec::AgentProcedure {
            procedure,
            agent_reuse_policy,
            runtime_session_policy,
        }) => {
            let node_type = if *agent_reuse_policy == AgentReusePolicy::ContinueCurrentAgent
                && *runtime_session_policy == RuntimeSessionPolicy::DeliverToCurrentTrace
            {
                LifecycleNodeType::PhaseNode
            } else {
                LifecycleNodeType::AgentNode
            };
            (procedure.procedure_key().map(str::to_string), node_type)
        }
        _ => (None, LifecycleNodeType::AgentNode),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeNodeArtifactScope {
    pub run_id: Uuid,
    pub orchestration_id: Uuid,
    pub node_path: String,
    pub attempt: u32,
}

impl RuntimeNodeArtifactScope {
    pub fn port_ref(&self, port_key: impl Into<String>) -> RuntimeNodePortArtifactRef {
        RuntimeNodePortArtifactRef {
            run_id: self.run_id,
            orchestration_id: self.orchestration_id,
            node_path: self.node_path.clone(),
            attempt: self.attempt,
            port_key: port_key.into(),
        }
    }

    pub fn path_prefix(&self) -> String {
        format!(
            "{}/{}/{}/",
            self.orchestration_id,
            encode_node_path_segment(&self.node_path),
            self.attempt
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeNodePortArtifactRef {
    pub run_id: Uuid,
    pub orchestration_id: Uuid,
    pub node_path: String,
    pub attempt: u32,
    pub port_key: String,
}

impl RuntimeNodePortArtifactRef {
    pub fn inline_path(&self) -> String {
        format!(
            "{}/{}/{}/{}",
            self.orchestration_id,
            encode_node_path_segment(&self.node_path),
            self.attempt,
            self.port_key
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleMountSurface {
    pub run_id: Uuid,
    pub orchestration_id: Uuid,
    pub node_path: String,
    pub lifecycle_key: String,
    pub attempt: u32,
    pub writable_port_keys: Vec<String>,
}

pub fn writable_port_keys_for_activity(activity: &ActivityDefinition) -> Vec<String> {
    activity
        .output_ports
        .iter()
        .map(|port| port.key.clone())
        .collect()
}

pub fn writable_port_keys_for_active_workflow(workflow: &ActiveWorkflowProjection) -> Vec<String> {
    writable_port_keys_for_activity(&workflow.active_activity)
}

pub fn lifecycle_mount_surface_for_active_workflow(
    workflow: &ActiveWorkflowProjection,
) -> LifecycleMountSurface {
    LifecycleMountSurface {
        run_id: workflow.run.id,
        orchestration_id: workflow.orchestration_id,
        node_path: workflow.node_path.clone(),
        lifecycle_key: workflow.lifecycle_key.clone(),
        attempt: workflow.active_attempt.attempt,
        writable_port_keys: writable_port_keys_for_active_workflow(workflow),
    }
}

pub fn lifecycle_mount_overlay_for_surface(surface: &LifecycleMountSurface) -> Vfs {
    Vfs {
        mounts: vec![build_lifecycle_mount_with_node_scope(surface)],
        default_mount_id: None,
        source_project_id: None,
        source_story_id: None,
        links: Vec::new(),
    }
}

pub fn project_active_workflow_lifecycle_vfs(
    vfs: Option<Vfs>,
    workflow: Option<&ActiveWorkflowProjection>,
) -> Option<Vfs> {
    let Some(workflow) = workflow else {
        return vfs;
    };

    let mut vfs = vfs.unwrap_or_default();
    let surface = lifecycle_mount_surface_for_active_workflow(workflow);
    let mut overlay = lifecycle_mount_overlay_for_surface(&surface);
    let mount = overlay
        .mounts
        .pop()
        .expect("lifecycle surface overlay must contain one mount");

    if let Some(existing) = vfs
        .mounts
        .iter_mut()
        .find(|candidate| candidate.id == LIFECYCLE_MOUNT_ID)
    {
        *existing = mount;
    } else {
        vfs.mounts.push(mount);
    }
    normalize_default_mount(&mut vfs);
    Some(vfs)
}

fn build_lifecycle_mount_with_node_scope(surface: &LifecycleMountSurface) -> Mount {
    let mut metadata = serde_json::json!({
        "run_id": surface.run_id.to_string(),
        "orchestration_id": surface.orchestration_id.to_string(),
        "node_path": surface.node_path.as_str(),
        "lifecycle_key": surface.lifecycle_key.as_str(),
        "scope": "node_runtime",
        "writable_port_keys": surface.writable_port_keys.as_slice(),
        "directory_hint": "Use artifacts/ for deliverables and records/ for supporting notes."
    });
    metadata["attempt"] = serde_json::json!(surface.attempt);

    Mount {
        id: LIFECYCLE_MOUNT_ID.to_string(),
        provider: PROVIDER_LIFECYCLE_VFS.to_string(),
        backend_id: String::new(),
        root_ref: format!(
            "lifecycle://run/{}/orchestration/{}/node/{}",
            surface.run_id,
            surface.orchestration_id,
            encode_node_path_segment(&surface.node_path)
        ),
        capabilities: vec![
            MountCapability::Read,
            MountCapability::Write,
            MountCapability::List,
            MountCapability::Search,
        ],
        default_write: false,
        display_name: "Lifecycle 执行记录".to_string(),
        metadata,
    }
}

fn normalize_default_mount(vfs: &mut Vfs) {
    if vfs
        .default_mount_id
        .as_ref()
        .is_some_and(|id| vfs.mounts.iter().any(|mount| &mount.id == id))
    {
        return;
    }
    vfs.default_mount_id = vfs
        .mounts
        .iter()
        .find(|mount| mount.default_write)
        .or_else(|| vfs.mounts.first())
        .map(|mount| mount.id.clone());
}

pub fn encode_node_path_segment(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::new();
    for byte in value.as_bytes() {
        let is_safe = byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-');
        if is_safe {
            encoded.push(char::from(*byte));
        } else {
            encoded.push('%');
            encoded.push(char::from(HEX[(byte >> 4) as usize]));
            encoded.push(char::from(HEX[(byte & 0x0F) as usize]));
        }
    }
    encoded
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KickoffPromptFragment {
    pub title_line: String,
    pub output_section: String,
    pub input_section: String,
}

#[derive(Debug, Clone)]
pub struct ActivityActivation {
    pub capability_state: CapabilityState,
    pub mcp_servers: Vec<RuntimeMcpServer>,
    pub capability_keys: BTreeSet<String>,
    pub kickoff_prompt: KickoffPromptFragment,
    pub lifecycle_mount: Mount,
    pub lifecycle_vfs: Vfs,
    pub mount_directives: Vec<MountDirective>,
}

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
    Project(Vec<BuiltinLifecycleSkill>),
}

impl BuiltinLifecycleSkillPolicy {
    pub fn project(skills: impl IntoIterator<Item = BuiltinLifecycleSkill>) -> Self {
        Self::Project(skills.into_iter().collect())
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

    pub fn project(
        explicit_skill_asset_keys: Vec<String>,
        skills: impl IntoIterator<Item = BuiltinLifecycleSkill>,
    ) -> Self {
        Self {
            explicit_skill_asset_keys,
            builtin_skills: BuiltinLifecycleSkillPolicy::project(skills),
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
