use std::collections::BTreeSet;

use agentdash_domain::{
    companion::COMPANION_SYSTEM_SKILL_NAME, routine::ROUTINE_MEMORY_SKILL_NAME,
    workspace_module::WORKSPACE_MODULE_SYSTEM_SKILL_NAME,
};
use agentdash_spi::Vfs;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::lifecycle::ActivityActivation;
use crate::repository_set::RepositorySet;
use crate::session::capability_state::compose_vfs_with_overlay_and_directives;
use crate::skill_asset::SkillAssetService;
use crate::vfs::{append_lifecycle_skill_asset_projection, build_lifecycle_mount_with_node_scope};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinLifecycleSkill {
    CompanionSystem,
    WorkspaceModuleSystem,
    RoutineMemory,
}

impl BuiltinLifecycleSkill {
    pub fn key(self) -> &'static str {
        match self {
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
    pub node_projection: Option<OrchestrationNodeProjectionInput>,
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
    pub orchestration_node: Option<OrchestrationNodeProjectionFacts>,
    pub skill_assets: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageStreamProjectionFacts {
    pub runtime_session_id: String,
    pub trace_kind: MessageStreamTraceKind,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRunLifecycleMountMetadata {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub frame_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_stream: Option<MessageStreamProjectionMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orchestration_node: Option<OrchestrationNodeProjectionMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_asset_project_id: Option<Uuid>,
    #[serde(default)]
    pub skill_asset_keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageStreamProjectionMetadata {
    pub runtime_session_id: String,
    pub trace_kind: MessageStreamTraceKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrchestrationNodeProjectionMetadata {
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

    pub async fn project(
        &self,
        input: AgentRunLifecycleSurfaceInput,
    ) -> Result<AgentRunLifecycleSurface, String> {
        let mut skill_asset_keys =
            projected_skill_keys_for_project(input.base_vfs.as_ref(), input.project_id);
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
            let mut overlay = Vfs {
                mounts: vec![lifecycle_mount],
                default_mount_id: None,
                source_project_id: None,
                source_story_id: None,
                links: Vec::new(),
            };
            annotate_mount_with_projector_metadata(&mut overlay, &input, &skill_asset_keys);
            vfs = compose_vfs_with_overlay_and_directives(Some(&vfs), &overlay, &[]);
        }
        _ => {
            let Some(message_stream) = input.message_stream.as_ref() else {
                return Err(
                    "AgentRun lifecycle surface 缺少 message stream 或 node projection".to_string(),
                );
            };
            let anchor = agent_run_session_anchor_from_projector_input(
                &input.address,
                message_stream,
                input.node_projection.as_ref(),
            );
            install_agent_run_lifecycle_mount(&mut vfs, &anchor);
            annotate_mount_with_projector_metadata(&mut vfs, &input, &skill_asset_keys);
        }
    }

    if !skill_asset_keys.is_empty() {
        append_lifecycle_skill_asset_projection(&mut vfs, input.project_id, &skill_asset_keys);
    }
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
    node_projection: Option<&OrchestrationNodeProjectionInput>,
) -> agentdash_domain::workflow::RuntimeSessionExecutionAnchor {
    match node_projection {
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

fn annotate_mount_with_projector_metadata(
    vfs: &mut Vfs,
    input: &AgentRunLifecycleSurfaceInput,
    skill_asset_keys: &[String],
) {
    let metadata = AgentRunLifecycleMountMetadata {
        run_id: input.address.run_id,
        agent_id: input.address.agent_id,
        frame_id: input.address.frame_id,
        message_stream: input.message_stream.as_ref().map(|message_stream| {
            MessageStreamProjectionMetadata {
                runtime_session_id: message_stream.runtime_session_id.clone(),
                trace_kind: message_stream.trace_kind,
            }
        }),
        orchestration_node: input.node_projection.as_ref().map(|node| {
            OrchestrationNodeProjectionMetadata {
                orchestration_id: node.orchestration_id,
                node_path: node.node_path.clone(),
                lifecycle_key: node.lifecycle_key.clone(),
                attempt: node.attempt,
                writable_port_keys: node.writable_port_keys.clone(),
            }
        }),
        skill_asset_project_id: (!skill_asset_keys.is_empty()).then_some(input.project_id),
        skill_asset_keys: skill_asset_keys.to_vec(),
    };
    let typed_metadata = serde_json::to_value(metadata).unwrap_or(serde_json::Value::Null);

    let Some(lifecycle) = vfs.mounts.iter_mut().find(|mount| mount.id == "lifecycle") else {
        return;
    };
    let mut existing = match std::mem::take(&mut lifecycle.metadata) {
        serde_json::Value::Object(object) => object,
        serde_json::Value::Null => serde_json::Map::new(),
        other => {
            let mut object = serde_json::Map::new();
            object.insert("raw_metadata".to_string(), other);
            object
        }
    };
    existing.insert(
        "agent_run_lifecycle_surface".to_string(),
        typed_metadata.clone(),
    );
    if let serde_json::Value::Object(projector_object) = typed_metadata {
        for (key, value) in projector_object {
            existing.entry(key).or_insert(value);
        }
    }
    lifecycle.metadata = serde_json::Value::Object(existing);
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
                node_projection: Some(OrchestrationNodeProjectionInput {
                    run_id,
                    orchestration_id: Uuid::new_v4(),
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
    fn projector_metadata_roundtrips() {
        let metadata = AgentRunLifecycleMountMetadata {
            run_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            frame_id: Uuid::new_v4(),
            message_stream: Some(MessageStreamProjectionMetadata {
                runtime_session_id: "session".to_string(),
                trace_kind: MessageStreamTraceKind::ConnectorRuntimeSession,
            }),
            orchestration_node: None,
            skill_asset_project_id: None,
            skill_asset_keys: Vec::new(),
        };
        let json = serde_json::to_value(&metadata).expect("json");
        let parsed: AgentRunLifecycleMountMetadata =
            serde_json::from_value(json).expect("metadata");
        assert_eq!(parsed, metadata);
    }

    #[test]
    fn node_projection_input_keeps_node_ownership_out_of_message_stream() {
        let project_id = Uuid::new_v4();
        let run_id = Uuid::new_v4();
        let input = AgentRunLifecycleSurfaceInput {
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
            node_projection: Some(OrchestrationNodeProjectionInput {
                run_id,
                orchestration_id: Uuid::new_v4(),
                node_path: "phase/plan".to_string(),
                lifecycle_key: "dev".to_string(),
                attempt: 2,
                writable_port_keys: vec!["summary".to_string()],
            }),
        };
        let mut vfs = input.base_vfs.clone().unwrap();
        let keys = input.explicit_skill_asset_keys.clone();
        let lifecycle_mount = build_lifecycle_mount_with_node_scope(
            run_id,
            input.node_projection.as_ref().unwrap().orchestration_id,
            "phase/plan",
            "dev",
            &["summary".to_string()],
            Some(2),
        );
        vfs.mounts.push(lifecycle_mount);
        annotate_mount_with_projector_metadata(&mut vfs, &input, &keys);
        let lifecycle = vfs
            .mounts
            .iter()
            .find(|mount| mount.id == "lifecycle")
            .expect("lifecycle");
        assert_eq!(
            lifecycle
                .metadata
                .pointer("/agent_run_lifecycle_surface/orchestration_node/node_path")
                .and_then(serde_json::Value::as_str),
            Some("phase/plan")
        );
        assert_eq!(
            lifecycle
                .metadata
                .pointer("/agent_run_lifecycle_surface/message_stream/runtime_session_id")
                .and_then(serde_json::Value::as_str),
            Some("session-1")
        );
    }
}
