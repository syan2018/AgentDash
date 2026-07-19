use std::{collections::BTreeSet, sync::Arc};

use agentdash_application_ports::lifecycle_surface_projection as port;
use agentdash_application_vfs::mount_skill_asset::refresh_lifecycle_skill_asset_projection;
use agentdash_domain::{
    canvas::CANVAS_SYSTEM_SKILL_NAME, companion::COMPANION_SYSTEM_SKILL_NAME,
    routine::ROUTINE_MEMORY_SKILL_NAME, skill_asset::SkillAssetRepository,
    workspace_module::WORKSPACE_MODULE_SYSTEM_SKILL_NAME,
};
use agentdash_platform_spi::Vfs;

use crate::lifecycle::vfs_mount::{
    build_agent_run_lifecycle_mount, build_lifecycle_mount_with_node_scope,
};

/// Converts immutable Product frame coordinates into a Lifecycle VFS projection.
///
/// Conversation content is resolved later through Product binding -> Managed Runtime canonical
/// history. This component only projects the mount and provisioned SkillAsset references.
pub struct AgentRunLifecycleSurfaceProjector {
    skill_assets: Arc<dyn SkillAssetRepository>,
}

impl AgentRunLifecycleSurfaceProjector {
    pub fn from_skill_asset_repo(skill_assets: Arc<dyn SkillAssetRepository>) -> Self {
        Self { skill_assets }
    }

    async fn project(
        &self,
        mut input: port::AgentRunLifecycleSurfaceInput,
    ) -> Result<port::AgentRunLifecycleSurface, port::LifecycleSurfaceProjectionError> {
        let skill_asset_keys = self.resolve_skill_assets(&input).await?;
        let lifecycle_mount = lifecycle_mount(&input)?;
        let mut vfs = input.base_vfs.take().unwrap_or_default();
        vfs.mounts
            .retain(|candidate| candidate.id != port::LIFECYCLE_MOUNT_ID);
        vfs.mounts.push(lifecycle_mount);
        normalize_default_mount(&mut vfs);
        refresh_lifecycle_skill_asset_projection(&mut vfs, input.project_id, &skill_asset_keys);
        let lifecycle_mount = vfs
            .mounts
            .iter()
            .find(|mount| mount.id == port::LIFECYCLE_MOUNT_ID)
            .cloned()
            .expect("projector installed Lifecycle mount");
        Ok(port::AgentRunLifecycleSurface {
            vfs,
            lifecycle_mount,
            projections: projection_set(&input, &skill_asset_keys),
            skill_asset_keys,
        })
    }

    async fn resolve_skill_assets(
        &self,
        input: &port::AgentRunLifecycleSurfaceInput,
    ) -> Result<Vec<String>, port::LifecycleSurfaceProjectionError> {
        let mut keys = match &input.builtin_skills {
            port::BuiltinLifecycleSkillPolicy::PreserveProjected => {
                projected_skill_keys(input.base_vfs.as_ref(), input.project_id)
            }
            port::BuiltinLifecycleSkillPolicy::Project(skills) => skills
                .iter()
                .map(|skill| builtin_skill_key(*skill))
                .collect(),
        };
        keys.extend(input.explicit_skill_asset_keys.iter().cloned());
        let keys = normalized_keys(keys);
        for key in &keys {
            let asset = self
                .skill_assets
                .get_by_project_and_key(input.project_id, key)
                .await
                .map_err(|error| port::LifecycleSurfaceProjectionError::Repository {
                    operation: "skill_asset.get_by_project_and_key",
                    message: error.to_string(),
                })?;
            if asset.is_none() {
                return Err(projection_error(format!(
                    "Project {} 缺少已 provision 的 SkillAsset `{key}`",
                    input.project_id
                )));
            }
        }
        Ok(keys)
    }
}

#[async_trait::async_trait]
impl port::LifecycleSurfaceProjectionPort for AgentRunLifecycleSurfaceProjector {
    async fn project_lifecycle_surface(
        &self,
        input: port::AgentRunLifecycleSurfaceInput,
    ) -> Result<port::AgentRunLifecycleSurface, port::LifecycleSurfaceProjectionError> {
        self.project(input).await
    }
}

fn lifecycle_mount(
    input: &port::AgentRunLifecycleSurfaceInput,
) -> Result<agentdash_domain::common::Mount, port::LifecycleSurfaceProjectionError> {
    if input.mode == port::AgentRunLifecycleSurfaceMode::WorkflowNodeExecutionSurface {
        let node = input
            .node_projection
            .as_ref()
            .ok_or_else(|| projection_error("Workflow node surface 缺少 node projection"))?;
        return Ok(build_lifecycle_mount_with_node_scope(
            node.run_id,
            Some(input.address.agent_id),
            node.orchestration_id,
            &node.node_path,
            &node.lifecycle_key,
            &node.writable_port_keys,
            Some(node.attempt),
        ));
    }

    let stream = input
        .message_stream
        .as_ref()
        .ok_or_else(|| projection_error("AgentRun lifecycle surface 缺少 Runtime thread"))?;
    let direct_node = input.node_evidence.as_ref();
    let projected_node = input.node_projection.as_ref();
    Ok(build_agent_run_lifecycle_mount(
        input.address.run_id,
        input.address.agent_id,
        &stream.runtime_thread_id,
        input.address.frame_id,
        direct_node
            .map(|node| node.orchestration_id)
            .or_else(|| projected_node.map(|node| node.orchestration_id)),
        direct_node
            .map(|node| node.node_path.as_str())
            .or_else(|| projected_node.map(|node| node.node_path.as_str())),
        direct_node
            .map(|node| node.attempt)
            .or_else(|| projected_node.map(|node| node.attempt)),
    ))
}

fn builtin_skill_key(skill: port::BuiltinLifecycleSkill) -> String {
    match skill {
        port::BuiltinLifecycleSkill::CanvasSystem => CANVAS_SYSTEM_SKILL_NAME,
        port::BuiltinLifecycleSkill::CompanionSystem => COMPANION_SYSTEM_SKILL_NAME,
        port::BuiltinLifecycleSkill::WorkspaceModuleSystem => WORKSPACE_MODULE_SYSTEM_SKILL_NAME,
        port::BuiltinLifecycleSkill::RoutineMemory => ROUTINE_MEMORY_SKILL_NAME,
    }
    .to_string()
}

fn projected_skill_keys(vfs: Option<&Vfs>, project_id: uuid::Uuid) -> Vec<String> {
    vfs.and_then(|vfs| {
        vfs.mounts
            .iter()
            .find(|mount| mount.id == port::LIFECYCLE_MOUNT_ID)
    })
    .filter(|mount| {
        mount
            .metadata
            .get("skill_asset_project_id")
            .and_then(serde_json::Value::as_str)
            .and_then(|value| uuid::Uuid::parse_str(value).ok())
            == Some(project_id)
    })
    .and_then(|mount| {
        mount
            .metadata
            .get("skill_asset_keys")
            .and_then(serde_json::Value::as_array)
    })
    .map(|values| {
        values
            .iter()
            .filter_map(serde_json::Value::as_str)
            .map(ToOwned::to_owned)
            .collect()
    })
    .unwrap_or_default()
}

fn normalized_keys(keys: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    keys.into_iter()
        .map(|key| key.trim().to_string())
        .filter(|key| !key.is_empty())
        .filter(|key| seen.insert(key.clone()))
        .collect()
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

fn projection_set(
    input: &port::AgentRunLifecycleSurfaceInput,
    skill_asset_keys: &[String],
) -> port::AgentRunLifecycleProjectionSet {
    let node_evidence = input
        .node_evidence
        .as_ref()
        .map(|node| port::OrchestrationNodeEvidenceFacts {
            run_id: node.run_id,
            orchestration_id: node.orchestration_id,
            node_path: node.node_path.clone(),
            attempt: node.attempt,
        })
        .or_else(|| {
            input
                .node_projection
                .as_ref()
                .map(|node| port::OrchestrationNodeEvidenceFacts {
                    run_id: node.run_id,
                    orchestration_id: node.orchestration_id,
                    node_path: node.node_path.clone(),
                    attempt: node.attempt,
                })
        });
    port::AgentRunLifecycleProjectionSet {
        agent_run_identity: true,
        message_stream: input.message_stream.as_ref().map(|stream| {
            port::MessageStreamProjectionFacts {
                runtime_thread_id: stream.runtime_thread_id.clone(),
                trace_kind: stream.trace_kind,
            }
        }),
        node_evidence,
        orchestration_node: input.node_projection.as_ref().map(|node| {
            port::OrchestrationNodeProjectionFacts {
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

fn projection_error(message: impl Into<String>) -> port::LifecycleSurfaceProjectionError {
    port::LifecycleSurfaceProjectionError::Projection {
        message: message.into(),
    }
}
