use std::sync::Arc;

use agentdash_application_agentrun::agent_run::frame::AgentFrameBuilder;
use agentdash_application_agentrun::agent_run::{
    AgentFrameSurfaceExt, AgentRunProductRuntimeBinding, AgentRunProductRuntimeBindingStore,
    AgentRunProductRuntimeSurfaceRebindPort, AgentRunProductRuntimeSurfaceRebindRequest,
    ProductAgentFrameRef, ProductAgentSurfaceFacts,
};
use agentdash_application_ports::agent_frame_materialization::{
    AgentFrameWriteRole, AgentRunFrameSurfaceCommandOutcome, AgentRunFrameSurfaceError,
    AgentRunRuntimeSurfaceUpdatePort, RuntimeSurfaceChange, RuntimeSurfaceUpdateRequest,
};
use agentdash_domain::canvas::{Canvas, CanvasDataBinding};
use agentdash_domain::workflow::AgentFrame;
use agentdash_workspace_module::canvas::{
    CanvasMountAccess, append_canvas_mount, upsert_canvas_runtime_data_binding,
};
use async_trait::async_trait;

use crate::repository_set::RepositorySet;

pub struct ProductAgentRunRuntimeSurfaceUpdateService {
    repos: RepositorySet,
    bindings: Arc<dyn AgentRunProductRuntimeBindingStore>,
    surface_rebind: Arc<dyn AgentRunProductRuntimeSurfaceRebindPort>,
}

impl ProductAgentRunRuntimeSurfaceUpdateService {
    pub fn new(
        repos: RepositorySet,
        bindings: Arc<dyn AgentRunProductRuntimeBindingStore>,
        surface_rebind: Arc<dyn AgentRunProductRuntimeSurfaceRebindPort>,
    ) -> Self {
        Self {
            repos,
            bindings,
            surface_rebind,
        }
    }

    async fn update(
        &self,
        request: RuntimeSurfaceUpdateRequest,
    ) -> Result<AgentRunFrameSurfaceCommandOutcome, AgentRunFrameSurfaceError> {
        let binding = self
            .bindings
            .load_product_binding(&request.target)
            .await
            .map_err(surface_rejected)?
            .ok_or_else(|| surface_rejected("AgentRun has no Product Runtime binding"))?;
        if binding.target != request.target
            || binding.runtime_thread_id != request.runtime_thread_id
        {
            return Err(surface_rejected(
                "Runtime surface request does not match the durable Product binding",
            ));
        }
        let launch_frame = self
            .repos
            .agent_frame_repo
            .get(binding.launch_frame.frame_id)
            .await
            .map_err(surface_rejected)?
            .ok_or_else(|| surface_rejected("Product launch AgentFrame is missing"))?;
        let current = self
            .repos
            .agent_frame_repo
            .get_latest(request.target.agent_id)
            .await
            .map_err(surface_rejected)?
            .ok_or_else(|| surface_rejected("AgentRun has no current AgentFrame"))?;
        if launch_frame.agent_id != request.target.agent_id
            || current.agent_id != request.target.agent_id
            || current.revision < launch_frame.revision
        {
            return Err(surface_rejected(
                "current AgentFrame does not descend from the Product launch frame",
            ));
        }

        let candidate = self.apply_change(&request, &current).await?;
        let (frame, wrote_frame_revision) = if frame_surface_equal(&current, &candidate) {
            (current, false)
        } else {
            self.repos
                .agent_frame_repo
                .create(&candidate)
                .await
                .map_err(surface_rejected)?;
            (candidate, true)
        };
        let frame_ref = ProductAgentFrameRef {
            frame_id: frame.id,
            agent_id: frame.agent_id,
            revision: u64::try_from(frame.revision)
                .map_err(|_| surface_rejected("AgentFrame revision is negative"))?,
        };
        let operation_key = surface_operation_key(&request, &frame_ref);
        if binding.launch_frame != frame_ref {
            let evidence = self
                .surface_rebind
                .prepare_runtime_surface_rebind(AgentRunProductRuntimeSurfaceRebindRequest {
                    target: request.target.clone(),
                    runtime_thread_id: request.runtime_thread_id.clone(),
                    idempotency_key: format!("{operation_key}:agent"),
                    frame: frame_ref.clone(),
                    execution_profile: binding.execution_profile.clone(),
                    surface_facts: ProductAgentSurfaceFacts::from_frame(&frame),
                })
                .await
                .map_err(surface_rejected)?;
            if evidence.target != request.target
                || evidence.runtime_thread_id != request.runtime_thread_id
                || evidence.frame != frame_ref
            {
                return Err(surface_rejected(
                    "surface apply returned different Product coordinates",
                ));
            }
            let converged_binding = AgentRunProductRuntimeBinding {
                launch_frame: frame_ref.clone(),
                ..binding.clone()
            };
            self.bindings
                .replace_product_binding(
                    &binding.calculated_digest().map_err(surface_rejected)?,
                    &converged_binding,
                )
                .await
                .map_err(surface_rejected)?;
        }

        let mut outcome =
            AgentRunFrameSurfaceCommandOutcome::new(AgentFrameWriteRole::RuntimeSurfaceUpdate);
        outcome.frame_id = Some(frame.id);
        outcome.agent_id = Some(frame.agent_id);
        outcome.runtime_thread_id = Some(request.runtime_thread_id.to_string());
        outcome.wrote_frame_revision = wrote_frame_revision;
        outcome.adopted_active_runtime = true;
        outcome.diagnostics.push(format!(
            "Product AgentFrame surface converged from {:?} change",
            request.surface_kind()
        ));
        Ok(outcome)
    }

    async fn apply_change(
        &self,
        request: &RuntimeSurfaceUpdateRequest,
        current: &AgentFrame,
    ) -> Result<AgentFrame, AgentRunFrameSurfaceError> {
        let mut candidate = AgentFrameBuilder::new(current.agent_id)
            .with_created_by(
                "runtime_surface_update",
                Some(request.runtime_thread_id.to_string()),
            )
            .build_uncommitted(self.repos.agent_frame_repo.as_ref())
            .await
            .map_err(surface_rejected)?;
        match &request.change {
            RuntimeSurfaceChange::CanvasBindingChanged {
                canvas_mount_id, ..
            }
            | RuntimeSurfaceChange::CanvasVisibilityRequested {
                canvas_mount_id, ..
            } => {
                let canvas = self
                    .repos
                    .canvas_repo
                    .get_by_mount_id(self.project_id(request).await?, canvas_mount_id.as_str())
                    .await
                    .map_err(surface_rejected)?
                    .ok_or_else(|| {
                        surface_rejected(format!("Canvas `{canvas_mount_id}` does not exist"))
                    })?;
                apply_canvas_change(
                    current,
                    &mut candidate,
                    &canvas,
                    match &request.change {
                        RuntimeSurfaceChange::CanvasBindingChanged { binding, .. } => {
                            Some(binding.clone())
                        }
                        RuntimeSurfaceChange::CanvasVisibilityRequested { .. } => None,
                        _ => unreachable!("Canvas change was matched above"),
                    },
                )?;
            }
            RuntimeSurfaceChange::WorkspaceModuleVisibilityChanged { .. } => {}
            RuntimeSurfaceChange::ProjectVfsMountChanged { mount_id } => {
                let vfs = current.typed_vfs().ok_or_else(|| {
                    surface_rejected("current AgentFrame has no typed VFS surface")
                })?;
                if !vfs.mounts.iter().any(|mount| mount.id == *mount_id) {
                    return Err(surface_rejected(format!(
                        "current AgentFrame does not contain changed VFS mount `{mount_id}`"
                    )));
                }
            }
            RuntimeSurfaceChange::McpPresetChanged { preset_key } => {
                if !current
                    .typed_mcp_servers()
                    .iter()
                    .any(|server| server.name == *preset_key)
                {
                    return Err(surface_rejected(format!(
                        "current AgentFrame does not contain changed MCP preset `{preset_key}`"
                    )));
                }
            }
            RuntimeSurfaceChange::SkillInventoryChanged { .. }
            | RuntimeSurfaceChange::AgentProcedureContractChanged { .. } => {}
        }
        Ok(candidate)
    }

    async fn project_id(
        &self,
        request: &RuntimeSurfaceUpdateRequest,
    ) -> Result<uuid::Uuid, AgentRunFrameSurfaceError> {
        let run = self
            .repos
            .lifecycle_run_repo
            .get_by_id(request.target.run_id)
            .await
            .map_err(surface_rejected)?
            .ok_or_else(|| surface_rejected("LifecycleRun does not exist"))?;
        let agent = self
            .repos
            .lifecycle_agent_repo
            .get(request.target.agent_id)
            .await
            .map_err(surface_rejected)?
            .ok_or_else(|| surface_rejected("LifecycleAgent does not exist"))?;
        if agent.run_id != run.id || agent.project_id != run.project_id {
            return Err(surface_rejected(
                "Lifecycle AgentRun facts do not match the Runtime surface target",
            ));
        }
        Ok(run.project_id)
    }
}

fn surface_operation_key(
    request: &RuntimeSurfaceUpdateRequest,
    frame: &ProductAgentFrameRef,
) -> String {
    format!(
        "surface-update:v1:{}:{}:{}:{}",
        request.target.run_id, request.target.agent_id, frame.frame_id, frame.revision
    )
}

#[async_trait]
impl AgentRunRuntimeSurfaceUpdatePort for ProductAgentRunRuntimeSurfaceUpdateService {
    async fn execute_runtime_surface_update(
        &self,
        request: RuntimeSurfaceUpdateRequest,
    ) -> Result<AgentRunFrameSurfaceCommandOutcome, AgentRunFrameSurfaceError> {
        self.update(request).await
    }
}

fn apply_canvas_change(
    current: &AgentFrame,
    candidate: &mut AgentFrame,
    canvas: &Canvas,
    binding: Option<CanvasDataBinding>,
) -> Result<(), AgentRunFrameSurfaceError> {
    let mut vfs = current
        .typed_vfs()
        .ok_or_else(|| surface_rejected("current AgentFrame has no typed VFS surface"))?;
    append_canvas_mount(&mut vfs, canvas, CanvasMountAccess::read_only());
    if let Some(binding) = binding {
        upsert_canvas_runtime_data_binding(&mut vfs, canvas, binding).map_err(surface_rejected)?;
    }
    let capability = if let Some(mut capability) = current.typed_capability_state() {
        capability.vfs.active = Some(vfs.clone());
        Some(serde_json::to_value(capability).map_err(surface_rejected)?)
    } else {
        None
    };
    candidate.attach_immutable_vfs_surface(
        serde_json::to_value(&vfs).map_err(surface_rejected)?,
        capability,
    );
    Ok(())
}

fn frame_surface_equal(left: &AgentFrame, right: &AgentFrame) -> bool {
    left.effective_capability_json == right.effective_capability_json
        && left.context_slice_json == right.context_slice_json
        && left.vfs_surface_json == right.vfs_surface_json
        && left.mcp_surface_json == right.mcp_surface_json
        && left.execution_profile_json == right.execution_profile_json
        && left.hook_plan == right.hook_plan
}

fn surface_rejected(error: impl std::fmt::Display) -> AgentRunFrameSurfaceError {
    AgentRunFrameSurfaceError::RuntimeSurfaceUpdateRejected {
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::common::{Mount, MountCapability, Vfs};
    use agentdash_platform_spi::{CapabilityState, ToolCluster};
    use agentdash_workspace_module::canvas::{
        build_canvas_mount_id, canvas_mount_runtime_data_bindings,
    };
    use uuid::Uuid;

    fn current_frame(agent_id: Uuid, project_id: Uuid) -> AgentFrame {
        let vfs = Vfs {
            mounts: vec![Mount {
                id: "workspace".to_owned(),
                provider: "relay_fs".to_owned(),
                backend_id: "backend-a".to_owned(),
                root_ref: "workspace://root".to_owned(),
                capabilities: vec![MountCapability::Read, MountCapability::List],
                default_write: true,
                display_name: "Workspace".to_owned(),
                metadata: serde_json::Value::Null,
            }],
            default_mount_id: Some("workspace".to_owned()),
            source_project_id: Some(project_id.to_string()),
            source_story_id: None,
            links: Vec::new(),
        };
        let mut capability = CapabilityState::from_clusters([ToolCluster::Read]);
        capability.vfs.active = Some(vfs.clone());
        let mut frame = AgentFrame::new_revision(agent_id, 3, "launch");
        frame.vfs_surface_json = Some(serde_json::to_value(vfs).unwrap());
        frame.effective_capability_json = Some(serde_json::to_value(capability).unwrap());
        frame.apply_surface_projection();
        frame
    }

    #[test]
    fn canvas_binding_updates_the_canonical_frame_surface() {
        let project_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let current = current_frame(agent_id, project_id);
        let mut candidate = current.clone();
        candidate.id = Uuid::new_v4();
        candidate.revision += 1;
        let canvas = Canvas::new(
            project_id,
            "dashboard".to_owned(),
            "Dashboard".to_owned(),
            String::new(),
        );
        let binding =
            CanvasDataBinding::new("metrics".to_owned(), "workspace://metrics.json".to_owned());

        apply_canvas_change(&current, &mut candidate, &canvas, Some(binding.clone())).unwrap();

        let vfs = candidate.typed_vfs().expect("candidate VFS");
        let canvas_mount = vfs
            .mounts
            .iter()
            .find(|mount| mount.id == build_canvas_mount_id(&canvas))
            .expect("Canvas mount");
        assert_eq!(
            canvas_mount_runtime_data_bindings(canvas_mount),
            vec![binding]
        );
        assert_eq!(
            candidate
                .typed_capability_state()
                .and_then(|state| state.vfs.active),
            Some(vfs)
        );
        assert!(
            !frame_surface_equal(&current, &candidate),
            "the persisted frame revision must carry the resource change"
        );
    }

    #[test]
    fn frame_identity_alone_does_not_create_a_surface_revision() {
        let current = current_frame(Uuid::new_v4(), Uuid::new_v4());
        let mut candidate = current.clone();
        candidate.id = Uuid::new_v4();
        candidate.revision += 1;
        candidate.created_by_kind = "runtime_surface_update".to_owned();

        assert!(frame_surface_equal(&current, &candidate));
    }
}
