use std::collections::BTreeSet;

use agentdash_domain::{
    canvas::CANVAS_SYSTEM_SKILL_NAME, companion::COMPANION_SYSTEM_SKILL_NAME,
    routine::ROUTINE_MEMORY_SKILL_NAME, workspace_module::WORKSPACE_MODULE_SYSTEM_SKILL_NAME,
};
use agentdash_spi::Vfs;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::lifecycle::ActivityActivation;
use crate::repository_set::RepositorySet;
use crate::session::capability_state::compose_vfs_with_overlay_and_directives;
use crate::skill_asset::SkillAssetService;
use crate::vfs::build_lifecycle_mount_with_node_scope;
use crate::vfs::mount_skill_asset::refresh_lifecycle_skill_asset_projection;

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
    pub lifecycle_mount: agentdash_domain::common::Mount,
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

pub struct AgentRunLifecycleSurfaceProjector<'a> {
    repos: &'a RepositorySet,
}

impl<'a> AgentRunLifecycleSurfaceProjector<'a> {
    pub fn new(repos: &'a RepositorySet) -> Self {
        Self { repos }
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
            BuiltinLifecycleSkillPolicy::EnsureAndProject(_) => Vec::new(),
        };
        skill_asset_keys.extend(input.explicit_skill_asset_keys.iter().cloned());

        if let BuiltinLifecycleSkillPolicy::EnsureAndProject(skills) = &input.builtin_skills {
            let service = SkillAssetService::new(self.repos.skill_asset_repo.as_ref());
            for skill in skills {
                service
                    .bootstrap_builtins(input.project_id, Some(skill.key()))
                    .await
                    .map_err(|error| error.to_string())?;
                skill_asset_keys.push(skill.key().to_string());
            }
        }
        project_surface_with_effective_skill_keys(
            input,
            normalized_skill_asset_keys(skill_asset_keys),
        )
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
            vfs = compose_vfs_with_overlay_and_directives(Some(&vfs), &overlay, &[]);
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
            let anchor = agent_run_session_anchor_from_projector_input(
                &input.address,
                message_stream,
                node_evidence.as_ref(),
            );
            install_agent_run_lifecycle_mount(&mut vfs, &anchor);
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

fn agent_run_session_anchor_from_projector_input(
    address: &AgentRunRuntimeAddress,
    message_stream: &MessageStreamProjectionRef,
    node_evidence: Option<&OrchestrationNodeEvidenceRef>,
) -> agentdash_domain::workflow::RuntimeSessionExecutionAnchor {
    match node_evidence {
        Some(node) => {
            agentdash_domain::workflow::RuntimeSessionExecutionAnchor::new_orchestration_dispatch(
                message_stream.runtime_session_id.clone(),
                address.run_id,
                address.frame_id,
                address.agent_id,
                node.orchestration_id,
                node.node_path.clone(),
                node.attempt,
            )
        }
        None => agentdash_domain::workflow::RuntimeSessionExecutionAnchor::new_dispatch(
            message_stream.runtime_session_id.clone(),
            address.run_id,
            address.frame_id,
            address.agent_id,
        ),
    }
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
    use crate::vfs::append_lifecycle_skill_asset_projection;
    use agentdash_domain::common::{Mount, MountCapability};

    fn lifecycle_node_vfs(project_id: Uuid) -> Vfs {
        let mut vfs = Vfs {
            mounts: vec![build_lifecycle_mount_with_node_scope(
                Uuid::new_v4(),
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
