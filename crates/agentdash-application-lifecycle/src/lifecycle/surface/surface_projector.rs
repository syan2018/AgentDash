use std::collections::BTreeSet;

use agentdash_application_ports::agent_run_surface as agent_run_surface_port;
use agentdash_application_ports::lifecycle_surface_projection as lifecycle_surface_port;
use agentdash_application_vfs::mount_skill_asset::refresh_lifecycle_skill_asset_projection;
use agentdash_domain::canvas::CANVAS_SYSTEM_SKILL_NAME;
use agentdash_domain::skill_asset::SkillAssetRepository;
use agentdash_domain::{
    companion::COMPANION_SYSTEM_SKILL_NAME, routine::ROUTINE_MEMORY_SKILL_NAME,
    workspace_module::WORKSPACE_MODULE_SYSTEM_SKILL_NAME,
};
use agentdash_platform_spi::Vfs;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::lifecycle::ActivityActivation;
use crate::lifecycle::build_lifecycle_mount_with_node_scope;

use super::mount::install_agent_run_lifecycle_mount;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunRuntimeAddress {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub frame_id: Uuid,
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

impl From<agent_run_surface_port::AgentRunRuntimeAddress> for AgentRunRuntimeAddress {
    fn from(value: agent_run_surface_port::AgentRunRuntimeAddress) -> Self {
        Self {
            run_id: value.run_id,
            agent_id: value.agent_id,
            frame_id: value.frame_id,
        }
    }
}

impl From<lifecycle_surface_port::MessageStreamTraceKind> for MessageStreamTraceKind {
    fn from(value: lifecycle_surface_port::MessageStreamTraceKind) -> Self {
        match value {
            lifecycle_surface_port::MessageStreamTraceKind::ConnectorRuntimeSession => {
                Self::ConnectorRuntimeSession
            }
            lifecycle_surface_port::MessageStreamTraceKind::RestoredTranscript => {
                Self::RestoredTranscript
            }
        }
    }
}

impl From<MessageStreamTraceKind> for lifecycle_surface_port::MessageStreamTraceKind {
    fn from(value: MessageStreamTraceKind) -> Self {
        match value {
            MessageStreamTraceKind::ConnectorRuntimeSession => Self::ConnectorRuntimeSession,
            MessageStreamTraceKind::RestoredTranscript => Self::RestoredTranscript,
        }
    }
}

impl From<lifecycle_surface_port::MessageStreamProjectionRef> for MessageStreamProjectionRef {
    fn from(value: lifecycle_surface_port::MessageStreamProjectionRef) -> Self {
        Self {
            runtime_session_id: value.runtime_session_id,
            trace_kind: value.trace_kind.into(),
        }
    }
}

impl From<lifecycle_surface_port::OrchestrationNodeEvidenceRef> for OrchestrationNodeEvidenceRef {
    fn from(value: lifecycle_surface_port::OrchestrationNodeEvidenceRef) -> Self {
        Self {
            run_id: value.run_id,
            orchestration_id: value.orchestration_id,
            node_path: value.node_path,
            attempt: value.attempt,
        }
    }
}

impl From<lifecycle_surface_port::OrchestrationNodeProjectionInput>
    for OrchestrationNodeProjectionInput
{
    fn from(value: lifecycle_surface_port::OrchestrationNodeProjectionInput) -> Self {
        Self {
            run_id: value.run_id,
            orchestration_id: value.orchestration_id,
            node_path: value.node_path,
            lifecycle_key: value.lifecycle_key,
            attempt: value.attempt,
            writable_port_keys: value.writable_port_keys,
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

impl BuiltinLifecycleSkill {
    pub fn key(self) -> &'static str {
        match self {
            Self::CanvasSystem => CANVAS_SYSTEM_SKILL_NAME,
            Self::CompanionSystem => COMPANION_SYSTEM_SKILL_NAME,
            Self::WorkspaceModuleSystem => WORKSPACE_MODULE_SYSTEM_SKILL_NAME,
            Self::RoutineMemory => ROUTINE_MEMORY_SKILL_NAME,
        }
    }
}

impl From<lifecycle_surface_port::BuiltinLifecycleSkill> for BuiltinLifecycleSkill {
    fn from(value: lifecycle_surface_port::BuiltinLifecycleSkill) -> Self {
        match value {
            lifecycle_surface_port::BuiltinLifecycleSkill::CanvasSystem => Self::CanvasSystem,
            lifecycle_surface_port::BuiltinLifecycleSkill::CompanionSystem => Self::CompanionSystem,
            lifecycle_surface_port::BuiltinLifecycleSkill::WorkspaceModuleSystem => {
                Self::WorkspaceModuleSystem
            }
            lifecycle_surface_port::BuiltinLifecycleSkill::RoutineMemory => Self::RoutineMemory,
        }
    }
}

impl From<BuiltinLifecycleSkill> for lifecycle_surface_port::BuiltinLifecycleSkill {
    fn from(value: BuiltinLifecycleSkill) -> Self {
        match value {
            BuiltinLifecycleSkill::CanvasSystem => Self::CanvasSystem,
            BuiltinLifecycleSkill::CompanionSystem => Self::CompanionSystem,
            BuiltinLifecycleSkill::WorkspaceModuleSystem => Self::WorkspaceModuleSystem,
            BuiltinLifecycleSkill::RoutineMemory => Self::RoutineMemory,
        }
    }
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

impl From<lifecycle_surface_port::BuiltinLifecycleSkillPolicy> for BuiltinLifecycleSkillPolicy {
    fn from(value: lifecycle_surface_port::BuiltinLifecycleSkillPolicy) -> Self {
        match value {
            lifecycle_surface_port::BuiltinLifecycleSkillPolicy::PreserveProjected => {
                Self::PreserveProjected
            }
            lifecycle_surface_port::BuiltinLifecycleSkillPolicy::Project(skills) => {
                Self::Project(skills.into_iter().map(Into::into).collect())
            }
        }
    }
}

impl From<BuiltinLifecycleSkillPolicy> for lifecycle_surface_port::BuiltinLifecycleSkillPolicy {
    fn from(value: BuiltinLifecycleSkillPolicy) -> Self {
        match value {
            BuiltinLifecycleSkillPolicy::PreserveProjected => Self::PreserveProjected,
            BuiltinLifecycleSkillPolicy::Project(skills) => {
                Self::Project(skills.into_iter().map(Into::into).collect())
            }
        }
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

impl From<lifecycle_surface_port::AgentRunLifecycleSurfaceMode> for AgentRunLifecycleSurfaceMode {
    fn from(value: lifecycle_surface_port::AgentRunLifecycleSurfaceMode) -> Self {
        match value {
            lifecycle_surface_port::AgentRunLifecycleSurfaceMode::WorkspaceReadSurface => {
                Self::WorkspaceReadSurface
            }
            lifecycle_surface_port::AgentRunLifecycleSurfaceMode::LaunchEvidenceSurface => {
                Self::LaunchEvidenceSurface
            }
            lifecycle_surface_port::AgentRunLifecycleSurfaceMode::CompanionChildSurface => {
                Self::CompanionChildSurface
            }
            lifecycle_surface_port::AgentRunLifecycleSurfaceMode::WorkflowNodeExecutionSurface => {
                Self::WorkflowNodeExecutionSurface
            }
        }
    }
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

impl From<lifecycle_surface_port::AgentRunLifecycleSurfaceInput> for AgentRunLifecycleSurfaceInput {
    fn from(value: lifecycle_surface_port::AgentRunLifecycleSurfaceInput) -> Self {
        Self {
            base_vfs: value.base_vfs,
            address: value.address.into(),
            message_stream: value.message_stream.map(Into::into),
            project_id: value.project_id,
            mode: value.mode.into(),
            explicit_skill_asset_keys: value.explicit_skill_asset_keys,
            builtin_skills: value.builtin_skills.into(),
            node_evidence: value.node_evidence.map(Into::into),
            node_projection: value.node_projection.map(Into::into),
        }
    }
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
    pub lifecycle_mount: agentdash_domain::common::Mount,
    pub projections: AgentRunLifecycleProjectionSet,
    pub skill_asset_keys: Vec<String>,
}

impl From<AgentRunLifecycleSurface> for lifecycle_surface_port::AgentRunLifecycleSurface {
    fn from(value: AgentRunLifecycleSurface) -> Self {
        Self {
            vfs: value.vfs,
            lifecycle_mount: value.lifecycle_mount,
            projections: value.projections.into(),
            skill_asset_keys: value.skill_asset_keys,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunLifecycleProjectionSet {
    pub agent_run_identity: bool,
    pub message_stream: Option<MessageStreamProjectionFacts>,
    pub node_evidence: Option<OrchestrationNodeEvidenceFacts>,
    pub orchestration_node: Option<OrchestrationNodeProjectionFacts>,
    pub skill_assets: Vec<String>,
}

impl From<AgentRunLifecycleProjectionSet>
    for lifecycle_surface_port::AgentRunLifecycleProjectionSet
{
    fn from(value: AgentRunLifecycleProjectionSet) -> Self {
        Self {
            agent_run_identity: value.agent_run_identity,
            message_stream: value.message_stream.map(Into::into),
            node_evidence: value.node_evidence.map(Into::into),
            orchestration_node: value.orchestration_node.map(Into::into),
            skill_assets: value.skill_assets,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageStreamProjectionFacts {
    pub runtime_session_id: String,
    pub trace_kind: MessageStreamTraceKind,
}

impl From<MessageStreamProjectionFacts> for lifecycle_surface_port::MessageStreamProjectionFacts {
    fn from(value: MessageStreamProjectionFacts) -> Self {
        Self {
            runtime_session_id: value.runtime_session_id,
            trace_kind: value.trace_kind.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrchestrationNodeEvidenceFacts {
    pub run_id: Uuid,
    pub orchestration_id: Uuid,
    pub node_path: String,
    pub attempt: u32,
}

impl From<OrchestrationNodeEvidenceFacts>
    for lifecycle_surface_port::OrchestrationNodeEvidenceFacts
{
    fn from(value: OrchestrationNodeEvidenceFacts) -> Self {
        Self {
            run_id: value.run_id,
            orchestration_id: value.orchestration_id,
            node_path: value.node_path,
            attempt: value.attempt,
        }
    }
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

impl From<OrchestrationNodeProjectionFacts>
    for lifecycle_surface_port::OrchestrationNodeProjectionFacts
{
    fn from(value: OrchestrationNodeProjectionFacts) -> Self {
        Self {
            run_id: value.run_id,
            orchestration_id: value.orchestration_id,
            node_path: value.node_path,
            lifecycle_key: value.lifecycle_key,
            attempt: value.attempt,
            writable_port_keys: value.writable_port_keys,
        }
    }
}

pub struct AgentRunLifecycleSurfaceProjector {
    repo: Arc<dyn SkillAssetRepository>,
}

impl AgentRunLifecycleSurfaceProjector {
    pub fn from_skill_asset_repo(repo: Arc<dyn SkillAssetRepository>) -> Self {
        Self { repo }
    }

    pub async fn project_workspace_read_surface(
        &self,
        facts: AgentRunLifecycleSessionEvidenceFacts,
    ) -> Result<AgentRunLifecycleSurface, String> {
        self.project(session_evidence_input(
            AgentRunLifecycleSurfaceMode::WorkspaceReadSurface,
            facts,
        ))
        .await
    }

    pub async fn project_launch_evidence_surface(
        &self,
        facts: AgentRunLifecycleSessionEvidenceFacts,
    ) -> Result<AgentRunLifecycleSurface, String> {
        self.project(session_evidence_input(
            AgentRunLifecycleSurfaceMode::LaunchEvidenceSurface,
            facts,
        ))
        .await
    }

    pub async fn project_companion_child_surface(
        &self,
        facts: AgentRunLifecycleSessionEvidenceFacts,
    ) -> Result<AgentRunLifecycleSurface, String> {
        self.project(session_evidence_input(
            AgentRunLifecycleSurfaceMode::CompanionChildSurface,
            facts,
        ))
        .await
    }

    pub async fn project_workflow_node_execution_surface(
        &self,
        facts: AgentRunLifecycleNodeRuntimeFacts,
    ) -> Result<AgentRunLifecycleSurface, String> {
        self.project(node_runtime_input(facts)).await
    }

    pub async fn project_workflow_node_activation(
        &self,
        activation: &mut ActivityActivation,
        facts: AgentRunLifecycleNodeRuntimeFacts,
    ) -> Result<AgentRunLifecycleSurface, String> {
        self.project_activation(activation, node_runtime_input(facts))
            .await
    }

    pub async fn project(
        &self,
        input: AgentRunLifecycleSurfaceInput,
    ) -> Result<AgentRunLifecycleSurface, String> {
        let mut skill_asset_keys = match &input.builtin_skills {
            BuiltinLifecycleSkillPolicy::PreserveProjected => {
                projected_skill_keys_for_project(input.base_vfs.as_ref(), input.project_id)
            }
            BuiltinLifecycleSkillPolicy::Project(skills) => {
                skills.iter().map(|skill| skill.key().to_string()).collect()
            }
        };
        skill_asset_keys.extend(input.explicit_skill_asset_keys.iter().cloned());
        let skill_asset_keys = normalized_skill_asset_keys(skill_asset_keys);
        for key in &skill_asset_keys {
            let asset = self
                .repo
                .get_by_project_and_key(input.project_id, key)
                .await
                .map_err(|error| {
                    format!(
                        "Project {} SkillAsset `{key}` projection 读取失败: {error}",
                        input.project_id
                    )
                })?;
            if asset.is_none() {
                return Err(format!(
                    "Project {} 缺少已 provision 的 SkillAsset `{key}`",
                    input.project_id
                ));
            }
        }
        project_surface_with_effective_skill_keys(input, skill_asset_keys)
    }

    pub async fn project_activation(
        &self,
        activation: &mut ActivityActivation,
        input: AgentRunLifecycleSurfaceInput,
    ) -> Result<AgentRunLifecycleSurface, String> {
        let surface = self.project(input).await?;
        activation.lifecycle_vfs = surface.vfs.clone();
        activation.lifecycle_mount = surface.lifecycle_mount.clone();
        Ok(surface)
    }
}

#[async_trait::async_trait]
impl lifecycle_surface_port::LifecycleSurfaceProjectionPort for AgentRunLifecycleSurfaceProjector {
    async fn project_lifecycle_surface(
        &self,
        input: lifecycle_surface_port::AgentRunLifecycleSurfaceInput,
    ) -> Result<
        lifecycle_surface_port::AgentRunLifecycleSurface,
        lifecycle_surface_port::LifecycleSurfaceProjectionError,
    > {
        self.project(input.into())
            .await
            .map(Into::into)
            .map_err(|message| {
                lifecycle_surface_port::LifecycleSurfaceProjectionError::Projection { message }
            })
    }
}

fn session_evidence_input(
    mode: AgentRunLifecycleSurfaceMode,
    facts: AgentRunLifecycleSessionEvidenceFacts,
) -> AgentRunLifecycleSurfaceInput {
    AgentRunLifecycleSurfaceInput {
        base_vfs: facts.base_vfs,
        address: facts.address,
        message_stream: Some(facts.message_stream),
        project_id: facts.project_id,
        mode,
        explicit_skill_asset_keys: facts.skill_projection.explicit_skill_asset_keys,
        builtin_skills: facts.skill_projection.builtin_skills,
        node_evidence: facts.node_evidence,
        node_projection: None,
    }
}

fn node_runtime_input(facts: AgentRunLifecycleNodeRuntimeFacts) -> AgentRunLifecycleSurfaceInput {
    AgentRunLifecycleSurfaceInput {
        base_vfs: facts.base_vfs,
        address: facts.address,
        message_stream: facts.message_stream,
        project_id: facts.project_id,
        mode: AgentRunLifecycleSurfaceMode::WorkflowNodeExecutionSurface,
        explicit_skill_asset_keys: facts.skill_projection.explicit_skill_asset_keys,
        builtin_skills: facts.skill_projection.builtin_skills,
        node_evidence: Some(facts.node_projection.evidence_ref()),
        node_projection: Some(facts.node_projection),
    }
}

fn project_surface_with_effective_skill_keys(
    mut input: AgentRunLifecycleSurfaceInput,
    skill_asset_keys: Vec<String>,
) -> Result<AgentRunLifecycleSurface, String> {
    let skill_asset_keys = normalized_skill_asset_keys(skill_asset_keys);
    let mut vfs = input.base_vfs.take().unwrap_or_default();
    match input.mode {
        AgentRunLifecycleSurfaceMode::WorkflowNodeExecutionSurface => {
            let Some(node) = input.node_projection.as_ref() else {
                return Err(
                    "Workflow node execution lifecycle surface 缺少 node projection".to_string(),
                );
            };
            let lifecycle_mount = build_lifecycle_mount_with_node_scope(
                node.run_id,
                Some(input.address.agent_id),
                node.orchestration_id,
                &node.node_path,
                &node.lifecycle_key,
                &node.writable_port_keys,
                Some(node.attempt),
            );
            let overlay = Vfs {
                mounts: vec![lifecycle_mount],
                default_mount_id: None,
                source_project_id: None,
                source_story_id: None,
                links: Vec::new(),
            };
            vfs = compose_vfs_with_overlay(&vfs, &overlay);
        }
        _ => {
            let Some(message_stream) = input.message_stream.as_ref() else {
                return Err(
                    "AgentRun lifecycle surface 缺少 message stream 或 node projection".to_string(),
                );
            };
            let node_evidence = input.node_evidence.clone().or_else(|| {
                input
                    .node_projection
                    .as_ref()
                    .map(|node| node.evidence_ref())
            });
            install_agent_run_lifecycle_mount(
                &mut vfs,
                input.address.run_id,
                input.address.agent_id,
                &message_stream.runtime_session_id,
                input.address.frame_id,
                node_evidence
                    .as_ref()
                    .map(|node| (node.orchestration_id, node.node_path.as_str(), node.attempt)),
            );
        }
    }

    refresh_lifecycle_projection_metadata(&mut vfs, input.project_id, &skill_asset_keys);
    let lifecycle_mounts = vfs
        .mounts
        .iter()
        .filter(|mount| mount.id == "lifecycle")
        .cloned()
        .collect::<Vec<_>>();
    if lifecycle_mounts.len() != 1 {
        return Err(format!(
            "AgentRun lifecycle surface 必须产出唯一 lifecycle mount，实际 {}",
            lifecycle_mounts.len()
        ));
    }
    let lifecycle_mount = lifecycle_mounts
        .into_iter()
        .next()
        .expect("checked lifecycle mount");
    Ok(AgentRunLifecycleSurface {
        vfs,
        lifecycle_mount,
        projections: projection_set(&input, &skill_asset_keys),
        skill_asset_keys,
    })
}

fn compose_vfs_with_overlay(base: &Vfs, overlay: &Vfs) -> Vfs {
    let mut merged = base.clone();
    for mount in &overlay.mounts {
        if let Some(existing) = merged
            .mounts
            .iter_mut()
            .find(|candidate| candidate.id == mount.id)
        {
            *existing = mount.clone();
        } else {
            merged.mounts.push(mount.clone());
        }
    }
    if overlay.default_mount_id.is_some() {
        merged.default_mount_id = overlay.default_mount_id.clone();
    }
    merged
}

fn projected_skill_keys_for_project(vfs: Option<&Vfs>, project_id: Uuid) -> Vec<String> {
    let Some(vfs) = vfs else {
        return Vec::new();
    };
    vfs.mounts
        .iter()
        .find(|mount| mount.id == "lifecycle")
        .filter(|mount| {
            mount
                .metadata
                .get("skill_asset_project_id")
                .and_then(serde_json::Value::as_str)
                .and_then(|value| Uuid::parse_str(value).ok())
                == Some(project_id)
        })
        .and_then(|mount| {
            mount
                .metadata
                .get("skill_asset_keys")
                .and_then(serde_json::Value::as_array)
        })
        .map(|items| {
            items
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn normalized_skill_asset_keys(keys: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    keys.into_iter()
        .map(|key| key.trim().to_string())
        .filter(|key| !key.is_empty())
        .filter(|key| seen.insert(key.clone()))
        .collect()
}

fn refresh_lifecycle_projection_metadata(
    vfs: &mut Vfs,
    project_id: Uuid,
    skill_asset_keys: &[String],
) {
    let Some(lifecycle) = vfs.mounts.iter_mut().find(|mount| mount.id == "lifecycle") else {
        return;
    };

    if let serde_json::Value::Object(metadata) = &mut lifecycle.metadata {
        metadata.remove("agent_run_lifecycle_surface");
    }

    refresh_lifecycle_skill_asset_projection(vfs, project_id, skill_asset_keys);
}

fn projection_set(
    input: &AgentRunLifecycleSurfaceInput,
    skill_asset_keys: &[String],
) -> AgentRunLifecycleProjectionSet {
    AgentRunLifecycleProjectionSet {
        agent_run_identity: true,
        message_stream: input.message_stream.as_ref().map(|message_stream| {
            MessageStreamProjectionFacts {
                runtime_session_id: message_stream.runtime_session_id.clone(),
                trace_kind: message_stream.trace_kind,
            }
        }),
        node_evidence: input
            .node_evidence
            .as_ref()
            .map(|node| OrchestrationNodeEvidenceFacts {
                run_id: node.run_id,
                orchestration_id: node.orchestration_id,
                node_path: node.node_path.clone(),
                attempt: node.attempt,
            })
            .or_else(|| {
                input
                    .node_projection
                    .as_ref()
                    .map(|node| OrchestrationNodeEvidenceFacts {
                        run_id: node.run_id,
                        orchestration_id: node.orchestration_id,
                        node_path: node.node_path.clone(),
                        attempt: node.attempt,
                    })
            }),
        orchestration_node: input.node_projection.as_ref().map(|node| {
            OrchestrationNodeProjectionFacts {
                run_id: node.run_id,
                orchestration_id: node.orchestration_id,
                node_path: node.node_path.clone(),
                lifecycle_key: node.lifecycle_key.clone(),
                attempt: node.attempt,
                writable_port_keys: node.writable_port_keys.clone(),
            }
        }),
        skill_assets: skill_asset_keys.to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_application_skill::skill_asset::SkillAssetService;
    use agentdash_application_vfs::append_lifecycle_skill_asset_projection;
    use agentdash_domain::DomainError;
    use agentdash_domain::common::{Mount, MountCapability};
    use agentdash_domain::skill_asset::{SkillAsset, SkillAssetRepository};
    use agentdash_test_support::skill::MemorySkillAssetRepository;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Default)]
    struct RecordingSkillAssetRepository {
        inner: MemorySkillAssetRepository,
        creates: AtomicUsize,
        updates: AtomicUsize,
        deletes: AtomicUsize,
    }

    impl RecordingSkillAssetRepository {
        fn reset_writes(&self) {
            self.creates.store(0, Ordering::SeqCst);
            self.updates.store(0, Ordering::SeqCst);
            self.deletes.store(0, Ordering::SeqCst);
        }

        fn writes(&self) -> usize {
            self.creates.load(Ordering::SeqCst)
                + self.updates.load(Ordering::SeqCst)
                + self.deletes.load(Ordering::SeqCst)
        }
    }

    #[async_trait::async_trait]
    impl SkillAssetRepository for RecordingSkillAssetRepository {
        async fn create(&self, asset: &SkillAsset) -> Result<(), DomainError> {
            self.creates.fetch_add(1, Ordering::SeqCst);
            self.inner.create(asset).await
        }

        async fn get(&self, id: Uuid) -> Result<Option<SkillAsset>, DomainError> {
            self.inner.get(id).await
        }

        async fn get_by_project_and_key(
            &self,
            project_id: Uuid,
            key: &str,
        ) -> Result<Option<SkillAsset>, DomainError> {
            self.inner.get_by_project_and_key(project_id, key).await
        }

        async fn get_by_project_and_builtin_key(
            &self,
            project_id: Uuid,
            builtin_key: &str,
        ) -> Result<Option<SkillAsset>, DomainError> {
            self.inner
                .get_by_project_and_builtin_key(project_id, builtin_key)
                .await
        }

        async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<SkillAsset>, DomainError> {
            self.inner.list_by_project(project_id).await
        }

        async fn update(&self, asset: &SkillAsset) -> Result<(), DomainError> {
            self.updates.fetch_add(1, Ordering::SeqCst);
            self.inner.update(asset).await
        }

        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.deletes.fetch_add(1, Ordering::SeqCst);
            self.inner.delete(id).await
        }
    }

    fn lifecycle_node_vfs(project_id: Uuid) -> Vfs {
        let mut vfs = Vfs {
            mounts: vec![build_lifecycle_mount_with_node_scope(
                Uuid::new_v4(),
                None,
                Uuid::new_v4(),
                "plan",
                "dev",
                &["summary".to_string()],
                Some(1),
            )],
            default_mount_id: None,
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };
        assert!(append_lifecycle_skill_asset_projection(
            &mut vfs,
            project_id,
            &["companion-system".to_string()],
        ));
        vfs
    }

    fn workspace_mount() -> Mount {
        Mount {
            id: "main".to_string(),
            provider: "relay_fs".to_string(),
            backend_id: "backend-test".to_string(),
            root_ref: "workspace://test".to_string(),
            capabilities: vec![MountCapability::Read, MountCapability::List],
            default_write: true,
            display_name: "Workspace".to_string(),
            metadata: serde_json::Value::Null,
        }
    }

    fn projector_input(
        project_id: Uuid,
        base_vfs: Option<Vfs>,
        mode: AgentRunLifecycleSurfaceMode,
    ) -> AgentRunLifecycleSurfaceInput {
        AgentRunLifecycleSurfaceInput {
            base_vfs,
            address: AgentRunRuntimeAddress {
                run_id: Uuid::new_v4(),
                agent_id: Uuid::new_v4(),
                frame_id: Uuid::new_v4(),
            },
            message_stream: Some(MessageStreamProjectionRef {
                runtime_session_id: "session-1".to_string(),
                trace_kind: MessageStreamTraceKind::ConnectorRuntimeSession,
            }),
            project_id,
            mode,
            explicit_skill_asset_keys: Vec::new(),
            builtin_skills: BuiltinLifecycleSkillPolicy::PreserveProjected,
            node_evidence: None,
            node_projection: None,
        }
    }

    #[test]
    fn workspace_preserve_projected_skills_reads_existing_metadata() {
        let project_id = Uuid::new_v4();
        let vfs = lifecycle_node_vfs(project_id);
        assert_eq!(
            projected_skill_keys_for_project(Some(&vfs), project_id),
            vec!["companion-system".to_string()]
        );
    }

    #[tokio::test]
    async fn projected_builtin_keys_are_read_only_and_written_to_metadata() {
        let project_id = Uuid::new_v4();
        let repo = Arc::new(RecordingSkillAssetRepository::default());
        SkillAssetService::new(repo.as_ref())
            .provision_project_builtins(project_id, None)
            .await
            .expect("provision project builtins");
        repo.reset_writes();
        let projector = AgentRunLifecycleSurfaceProjector::from_skill_asset_repo(repo.clone());

        let surface = projector
            .project(AgentRunLifecycleSurfaceInput {
                builtin_skills: BuiltinLifecycleSkillPolicy::project([
                    BuiltinLifecycleSkill::CanvasSystem,
                    BuiltinLifecycleSkill::WorkspaceModuleSystem,
                    BuiltinLifecycleSkill::CompanionSystem,
                ]),
                ..projector_input(
                    project_id,
                    Some(Vfs {
                        mounts: vec![workspace_mount()],
                        default_mount_id: Some("main".to_string()),
                        source_project_id: None,
                        source_story_id: None,
                        links: Vec::new(),
                    }),
                    AgentRunLifecycleSurfaceMode::LaunchEvidenceSurface,
                )
            })
            .await
            .expect("project lifecycle surface");

        let expected_keys = vec![
            "canvas-system".to_string(),
            "workspace-module-system".to_string(),
            "companion-system".to_string(),
        ];
        assert_eq!(surface.skill_asset_keys, expected_keys);
        assert_eq!(
            surface
                .lifecycle_mount
                .metadata
                .get("skill_asset_keys")
                .and_then(serde_json::Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(serde_json::Value::as_str)
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                }),
            Some(surface.skill_asset_keys.clone())
        );
        assert_eq!(repo.writes(), 0, "lifecycle projection must be read-only");
    }

    #[tokio::test]
    async fn projection_rejects_missing_project_skill_asset() {
        let project_id = Uuid::new_v4();
        let repo = Arc::new(MemorySkillAssetRepository::default());
        let projector = AgentRunLifecycleSurfaceProjector::from_skill_asset_repo(repo);

        let error = projector
            .project(AgentRunLifecycleSurfaceInput {
                builtin_skills: BuiltinLifecycleSkillPolicy::project([
                    BuiltinLifecycleSkill::CanvasSystem,
                ]),
                ..projector_input(
                    project_id,
                    Some(Vfs {
                        mounts: vec![workspace_mount()],
                        default_mount_id: Some("main".to_string()),
                        source_project_id: None,
                        source_story_id: None,
                        links: Vec::new(),
                    }),
                    AgentRunLifecycleSurfaceMode::LaunchEvidenceSurface,
                )
            })
            .await
            .expect_err("missing skill asset must reject projection");

        assert!(error.contains(project_id.to_string().as_str()));
        assert!(error.contains("canvas-system"));
        assert!(error.contains("provision"));
    }

    #[tokio::test]
    async fn preserve_projected_keeps_existing_metadata_without_writes() {
        let project_id = Uuid::new_v4();
        let repo = Arc::new(RecordingSkillAssetRepository::default());
        SkillAssetService::new(repo.as_ref())
            .provision_project_builtins(project_id, Some("companion-system"))
            .await
            .expect("provision companion-system");
        repo.reset_writes();
        let projector = AgentRunLifecycleSurfaceProjector::from_skill_asset_repo(repo.clone());

        let surface = projector
            .project(projector_input(
                project_id,
                Some(lifecycle_node_vfs(project_id)),
                AgentRunLifecycleSurfaceMode::LaunchEvidenceSurface,
            ))
            .await
            .expect("project lifecycle surface");

        assert_eq!(surface.skill_asset_keys, vec!["companion-system"]);
        assert_eq!(repo.writes(), 0, "PreserveProjected must be read-only");
    }

    #[test]
    fn companion_child_projection_emits_single_lifecycle_mount() {
        let project_id = Uuid::new_v4();
        let surface = project_surface_with_effective_skill_keys(
            projector_input(
                project_id,
                Some(Vfs {
                    mounts: vec![workspace_mount()],
                    default_mount_id: Some("main".to_string()),
                    source_project_id: None,
                    source_story_id: None,
                    links: Vec::new(),
                }),
                AgentRunLifecycleSurfaceMode::CompanionChildSurface,
            ),
            vec!["companion-system".to_string()],
        )
        .expect("surface");
        assert_eq!(
            surface
                .vfs
                .mounts
                .iter()
                .filter(|mount| mount.id == "lifecycle")
                .count(),
            1
        );
        assert_eq!(surface.skill_asset_keys, vec!["companion-system"]);
        assert_eq!(
            surface
                .lifecycle_mount
                .metadata
                .get("scope")
                .and_then(serde_json::Value::as_str),
            Some("agent_run_session")
        );
    }

    #[test]
    fn launch_projection_replaces_stale_lifecycle_without_parallel_mount() {
        let project_id = Uuid::new_v4();
        let mut base = Vfs {
            mounts: vec![workspace_mount()],
            default_mount_id: Some("main".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };
        base.mounts.push(build_lifecycle_mount_with_node_scope(
            Uuid::new_v4(),
            None,
            Uuid::new_v4(),
            "stale",
            "stale",
            &[],
            Some(1),
        ));
        let surface = project_surface_with_effective_skill_keys(
            projector_input(
                project_id,
                Some(base),
                AgentRunLifecycleSurfaceMode::LaunchEvidenceSurface,
            ),
            vec![
                "writer".to_string(),
                "companion-system".to_string(),
                "writer".to_string(),
            ],
        )
        .expect("surface");
        assert_eq!(
            surface
                .vfs
                .mounts
                .iter()
                .filter(|mount| mount.id == "lifecycle")
                .count(),
            1
        );
        assert_eq!(
            surface.skill_asset_keys,
            vec!["writer".to_string(), "companion-system".to_string()]
        );
    }

    #[test]
    fn launch_projection_with_node_anchor_keeps_session_scope() {
        let project_id = Uuid::new_v4();
        let run_id = Uuid::new_v4();
        let surface = project_surface_with_effective_skill_keys(
            AgentRunLifecycleSurfaceInput {
                base_vfs: Some(Vfs {
                    mounts: vec![workspace_mount()],
                    default_mount_id: Some("main".to_string()),
                    source_project_id: None,
                    source_story_id: None,
                    links: Vec::new(),
                }),
                address: AgentRunRuntimeAddress {
                    run_id,
                    agent_id: Uuid::new_v4(),
                    frame_id: Uuid::new_v4(),
                },
                message_stream: Some(MessageStreamProjectionRef {
                    runtime_session_id: "session-node".to_string(),
                    trace_kind: MessageStreamTraceKind::ConnectorRuntimeSession,
                }),
                project_id,
                mode: AgentRunLifecycleSurfaceMode::LaunchEvidenceSurface,
                explicit_skill_asset_keys: Vec::new(),
                builtin_skills: BuiltinLifecycleSkillPolicy::PreserveProjected,
                node_evidence: Some(OrchestrationNodeEvidenceRef {
                    run_id,
                    orchestration_id: Uuid::new_v4(),
                    node_path: "phase/plan".to_string(),
                    attempt: 2,
                }),
                node_projection: None,
            },
            vec!["companion-system".to_string()],
        )
        .expect("surface");
        let lifecycle = surface.lifecycle_mount;

        assert_eq!(
            lifecycle
                .metadata
                .get("scope")
                .and_then(serde_json::Value::as_str),
            Some("agent_run_session")
        );
        assert_eq!(
            lifecycle
                .metadata
                .get("node_path")
                .and_then(serde_json::Value::as_str),
            Some("phase/plan")
        );
        assert_eq!(
            lifecycle
                .metadata
                .get("skill_asset_keys")
                .and_then(serde_json::Value::as_array)
                .and_then(|items| items.first())
                .and_then(serde_json::Value::as_str),
            Some("companion-system")
        );
        assert!(
            lifecycle.metadata.get("lifecycle_key").is_none(),
            "session evidence mount must not expose writable node runtime metadata"
        );
        assert!(
            lifecycle.metadata.get("writable_port_keys").is_none(),
            "session evidence mount must not expose writable node runtime metadata"
        );
    }

    #[test]
    fn normalized_skill_keys_dedupe_and_trim() {
        assert_eq!(
            normalized_skill_asset_keys([
                " companion-system ".to_string(),
                "companion-system".to_string(),
                "workspace-module-system".to_string(),
            ]),
            vec![
                "companion-system".to_string(),
                "workspace-module-system".to_string()
            ]
        );
    }

    #[test]
    fn projection_refresh_clears_stale_skill_metadata_when_facts_are_empty() {
        let project_id = Uuid::new_v4();
        let surface = project_surface_with_effective_skill_keys(
            projector_input(
                project_id,
                Some(lifecycle_node_vfs(project_id)),
                AgentRunLifecycleSurfaceMode::LaunchEvidenceSurface,
            ),
            Vec::new(),
        )
        .expect("surface");

        assert!(
            surface
                .lifecycle_mount
                .metadata
                .get("skill_asset_project_id")
                .is_none()
        );
        assert!(
            surface
                .lifecycle_mount
                .metadata
                .get("skill_asset_keys")
                .is_none()
        );
    }

    #[test]
    fn node_projection_refreshes_provider_metadata_without_debug_envelope() {
        let project_id = Uuid::new_v4();
        let run_id = Uuid::new_v4();
        let orchestration_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let surface = project_surface_with_effective_skill_keys(
            AgentRunLifecycleSurfaceInput {
                base_vfs: Some(Vfs {
                    mounts: vec![workspace_mount()],
                    default_mount_id: Some("main".to_string()),
                    source_project_id: None,
                    source_story_id: None,
                    links: Vec::new(),
                }),
                address: AgentRunRuntimeAddress {
                    run_id,
                    agent_id,
                    frame_id: Uuid::new_v4(),
                },
                message_stream: Some(MessageStreamProjectionRef {
                    runtime_session_id: "session-1".to_string(),
                    trace_kind: MessageStreamTraceKind::ConnectorRuntimeSession,
                }),
                project_id,
                mode: AgentRunLifecycleSurfaceMode::WorkflowNodeExecutionSurface,
                explicit_skill_asset_keys: vec!["companion-system".to_string()],
                builtin_skills: BuiltinLifecycleSkillPolicy::PreserveProjected,
                node_evidence: None,
                node_projection: Some(OrchestrationNodeProjectionInput {
                    run_id,
                    orchestration_id,
                    node_path: "phase/plan".to_string(),
                    lifecycle_key: "dev".to_string(),
                    attempt: 2,
                    writable_port_keys: vec!["summary".to_string()],
                }),
            },
            vec!["companion-system".to_string()],
        )
        .expect("surface");
        let lifecycle = surface.lifecycle_mount;

        assert!(
            lifecycle
                .metadata
                .get("agent_run_lifecycle_surface")
                .is_none()
        );
        assert_eq!(
            lifecycle
                .metadata
                .get("scope")
                .and_then(serde_json::Value::as_str),
            Some("node_runtime")
        );
        assert_eq!(
            lifecycle
                .metadata
                .get("agent_id")
                .and_then(serde_json::Value::as_str),
            Some(agent_id.to_string().as_str())
        );
        assert_eq!(
            lifecycle
                .metadata
                .get("orchestration_id")
                .and_then(serde_json::Value::as_str),
            Some(orchestration_id.to_string().as_str())
        );
        assert_eq!(
            lifecycle
                .metadata
                .get("node_path")
                .and_then(serde_json::Value::as_str),
            Some("phase/plan")
        );
        assert_eq!(
            lifecycle
                .metadata
                .get("writable_port_keys")
                .and_then(serde_json::Value::as_array)
                .and_then(|items| items.first())
                .and_then(serde_json::Value::as_str),
            Some("summary")
        );
        assert_eq!(
            lifecycle
                .metadata
                .get("skill_asset_keys")
                .and_then(serde_json::Value::as_array)
                .and_then(|items| items.first())
                .and_then(serde_json::Value::as_str),
            Some("companion-system")
        );
    }
}
