use std::sync::Arc;

use agentdash_application_agentrun::agent_run::frame::{AgentFrameBuilder, FrameSurfaceDraft};
use agentdash_application_agentrun::agent_run::{
    AgentFrameSurfaceExt, AgentRunProductRuntimeBindingStore,
    AgentRunProductRuntimeSurfaceRebindPort, AgentRunProductRuntimeSurfaceRebindRequest,
    ProductAgentFrameRef, ProductAgentSurfaceFacts,
};
use agentdash_domain::agent_run_target::AgentRunTarget;
use agentdash_domain::common::{Mount, MountCapability};
use agentdash_domain::interaction::{
    InteractionDefinitionRepository, InteractionDefinitionRevision, InteractionDefinitionStatus,
    InteractionOwner,
};
use agentdash_domain::workflow::{AgentFrameRepository, LifecycleAgentRepository};
use agentdash_platform_spi::WorkspaceModuleVisibilityMode;
use async_trait::async_trait;
use uuid::Uuid;

const CANVAS_FS_PROVIDER: &str = "canvas_fs";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanvasMountIntentKind {
    Create,
    Attach,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanvasMountMaterializationRequest {
    pub target: AgentRunTarget,
    pub definition_id: Uuid,
    pub definition_revision_id: Uuid,
    pub intent: CanvasMountIntentKind,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CanvasMountConvergence {
    pub frame_id: Uuid,
    pub frame_revision: u64,
    pub wrote_frame_revision: bool,
    pub applied_generation: Option<u64>,
    pub module_ref: String,
    pub authoring_mount_id: String,
}

#[derive(Debug, thiserror::Error)]
pub enum CanvasMountMaterializationError {
    #[error("Canvas mount materialization request is invalid: {0}")]
    Invalid(String),
    #[error("Canvas mount authority was rejected: {0}")]
    Rejected(String),
    #[error("Canvas mount surface convergence failed: {0}")]
    Failed(String),
}

#[async_trait]
pub trait CanvasMountMaterializationPort: Send + Sync {
    async fn materialize(
        &self,
        request: CanvasMountMaterializationRequest,
    ) -> Result<CanvasMountConvergence, CanvasMountMaterializationError>;
}

#[derive(Clone)]
pub struct ProductCanvasMountMaterializer {
    definitions: Arc<dyn InteractionDefinitionRepository>,
    agents: Arc<dyn LifecycleAgentRepository>,
    frames: Arc<dyn AgentFrameRepository>,
    bindings: Arc<dyn AgentRunProductRuntimeBindingStore>,
    rebind: Arc<dyn AgentRunProductRuntimeSurfaceRebindPort>,
}

impl ProductCanvasMountMaterializer {
    pub fn new(
        definitions: Arc<dyn InteractionDefinitionRepository>,
        agents: Arc<dyn LifecycleAgentRepository>,
        frames: Arc<dyn AgentFrameRepository>,
        bindings: Arc<dyn AgentRunProductRuntimeBindingStore>,
        rebind: Arc<dyn AgentRunProductRuntimeSurfaceRebindPort>,
    ) -> Self {
        Self {
            definitions,
            agents,
            frames,
            bindings,
            rebind,
        }
    }

    async fn resolve_revision(
        &self,
        request: &CanvasMountMaterializationRequest,
    ) -> Result<(InteractionDefinitionRevision, String), CanvasMountMaterializationError> {
        if request.target.run_id.is_nil()
            || request.target.agent_id.is_nil()
            || request.definition_id.is_nil()
            || request.definition_revision_id.is_nil()
            || request.idempotency_key.trim().is_empty()
        {
            return Err(CanvasMountMaterializationError::Invalid(
                "target、definition、revision 与 idempotency key 必须有效".to_owned(),
            ));
        }
        let agent = self
            .agents
            .get(request.target.agent_id)
            .await
            .map_err(|error| CanvasMountMaterializationError::Failed(error.to_string()))?
            .ok_or_else(|| {
                CanvasMountMaterializationError::Rejected(
                    "Product Lifecycle Agent 不存在".to_owned(),
                )
            })?;
        if agent.run_id != request.target.run_id {
            return Err(CanvasMountMaterializationError::Rejected(
                "Product Lifecycle Agent 不属于目标 AgentRun".to_owned(),
            ));
        }
        let definition = self
            .definitions
            .get(request.definition_id)
            .await
            .map_err(|error| CanvasMountMaterializationError::Failed(error.to_string()))?
            .ok_or_else(|| {
                CanvasMountMaterializationError::Rejected(
                    "Canvas InteractionDefinition 不存在".to_owned(),
                )
            })?;
        if definition.project_id != agent.project_id
            || definition.current_revision_id != request.definition_revision_id
            || definition.status != InteractionDefinitionStatus::Active
        {
            return Err(CanvasMountMaterializationError::Rejected(
                "Canvas definition 不是当前 Product project 的 active revision".to_owned(),
            ));
        }
        let revision = self
            .definitions
            .get_revision(request.definition_revision_id)
            .await
            .map_err(|error| CanvasMountMaterializationError::Failed(error.to_string()))?
            .ok_or_else(|| {
                CanvasMountMaterializationError::Rejected(
                    "Canvas InteractionDefinition revision 不存在".to_owned(),
                )
            })?;
        if revision.definition_id != definition.id
            || revision.project_id != agent.project_id
            || !match &revision.owner {
                InteractionOwner::Project(project_id) => *project_id == agent.project_id,
                InteractionOwner::User(user_id) => user_id == &agent.created_by_user_id,
            }
        {
            return Err(CanvasMountMaterializationError::Rejected(
                "当前 Agent 所属用户无权物化该 Canvas".to_owned(),
            ));
        }
        Ok((revision, agent.created_by_user_id))
    }
}

#[async_trait]
impl CanvasMountMaterializationPort for ProductCanvasMountMaterializer {
    async fn materialize(
        &self,
        request: CanvasMountMaterializationRequest,
    ) -> Result<CanvasMountConvergence, CanvasMountMaterializationError> {
        let (revision, user_id) = self.resolve_revision(&request).await?;
        let binding = self
            .bindings
            .load_product_binding(&request.target)
            .await
            .map_err(CanvasMountMaterializationError::Failed)?
            .ok_or_else(|| {
                CanvasMountMaterializationError::Rejected(
                    "AgentRun Product runtime binding 不存在".to_owned(),
                )
            })?;
        let current = self
            .frames
            .get(binding.launch_frame.frame_id)
            .await
            .map_err(|error| CanvasMountMaterializationError::Failed(error.to_string()))?
            .ok_or_else(|| {
                CanvasMountMaterializationError::Failed(
                    "Product binding 指向的 AgentFrame 不存在".to_owned(),
                )
            })?;
        let module_ref = format!("canvas:{}", revision.definition_id);
        let mount = canvas_authoring_mount(&revision, &user_id);
        let mut capability = current.typed_capability_state().ok_or_else(|| {
            CanvasMountMaterializationError::Failed(
                "AgentFrame 缺少 typed capability surface".to_owned(),
            )
        })?;
        let mut vfs = current.typed_vfs().ok_or_else(|| {
            CanvasMountMaterializationError::Failed("AgentFrame 缺少 typed VFS surface".to_owned())
        })?;
        let module_visible = capability.workspace_module.allows(&module_ref);
        let mount_visible = vfs.mounts.iter().any(|candidate| {
            candidate.id == mount.id
                && candidate.provider == mount.provider
                && candidate.backend_id == mount.backend_id
                && candidate.root_ref == mount.root_ref
        });
        if module_visible && mount_visible {
            return Ok(CanvasMountConvergence {
                frame_id: current.id,
                frame_revision: u64::try_from(current.revision).unwrap_or_default(),
                wrote_frame_revision: false,
                applied_generation: None,
                module_ref,
                authoring_mount_id: revision.authoring_mount_id,
            });
        }
        if capability.workspace_module.mode == WorkspaceModuleVisibilityMode::Allowlist
            && !module_visible
        {
            capability
                .workspace_module
                .allowed_module_ids
                .push(module_ref.clone());
            capability.workspace_module.allowed_module_ids.sort();
            capability.workspace_module.allowed_module_ids.dedup();
        }
        if !mount_visible {
            vfs.mounts.push(mount);
            vfs.mounts.sort_by(|left, right| left.id.cmp(&right.id));
        }

        let mut draft = FrameSurfaceDraft::from_frame(&current);
        draft.capability_state = Some(capability);
        draft.vfs = Some(vfs);
        let mut builder = AgentFrameBuilder::new(request.target.agent_id)
            .with_surface_draft(&draft)
            .with_created_by(
                match request.intent {
                    CanvasMountIntentKind::Create => "canvas_mount_create",
                    CanvasMountIntentKind::Attach => "canvas_mount_attach",
                },
                Some(request.definition_id.to_string()),
            );
        if let Some(hook_plan) = current.hook_plan.clone() {
            builder = builder.with_hook_plan_raw(hook_plan);
        }
        let next = builder
            .build_uncommitted(self.frames.as_ref())
            .await
            .map_err(|error| CanvasMountMaterializationError::Failed(error.to_string()))?;
        self.frames
            .create(&next)
            .await
            .map_err(|error| CanvasMountMaterializationError::Failed(error.to_string()))?;
        let next_ref = ProductAgentFrameRef {
            frame_id: next.id,
            agent_id: next.agent_id,
            revision: u64::try_from(next.revision).map_err(|_| {
                CanvasMountMaterializationError::Failed(
                    "AgentFrame revision 无法投影为 Product revision".to_owned(),
                )
            })?,
        };
        let evidence = self
            .rebind
            .prepare_runtime_surface_rebind(AgentRunProductRuntimeSurfaceRebindRequest {
                target: request.target.clone(),
                runtime_thread_id: binding.runtime_thread_id.clone(),
                idempotency_key: request.idempotency_key,
                frame: next_ref.clone(),
                execution_profile: binding.execution_profile.clone(),
                surface_facts: ProductAgentSurfaceFacts::from_frame(&next),
            })
            .await
            .map_err(|error| CanvasMountMaterializationError::Failed(error.to_string()))?;
        let previous_digest = binding
            .calculated_digest()
            .map_err(CanvasMountMaterializationError::Failed)?;
        let mut next_binding = binding;
        next_binding.launch_frame = next_ref;
        self.bindings
            .replace_product_binding(&previous_digest, &next_binding)
            .await
            .map_err(CanvasMountMaterializationError::Failed)?;
        Ok(CanvasMountConvergence {
            frame_id: next.id,
            frame_revision: u64::try_from(next.revision).unwrap_or_default(),
            wrote_frame_revision: true,
            applied_generation: Some(evidence.prepared_generation),
            module_ref,
            authoring_mount_id: revision.authoring_mount_id,
        })
    }
}

fn canvas_authoring_mount(revision: &InteractionDefinitionRevision, user_id: &str) -> Mount {
    let editable = matches!(&revision.owner, InteractionOwner::User(owner) if owner == user_id);
    let mut capabilities = vec![
        MountCapability::Read,
        MountCapability::List,
        MountCapability::Search,
    ];
    if editable {
        capabilities.push(MountCapability::Write);
    }
    Mount {
        id: revision.authoring_mount_id.clone(),
        provider: CANVAS_FS_PROVIDER.to_owned(),
        backend_id: revision.definition_id.to_string(),
        root_ref: format!("canvas-root://{}", revision.definition_id),
        capabilities,
        default_write: editable,
        display_name: revision.title.clone(),
        metadata: serde_json::json!({
            "definition_id": revision.definition_id,
            "definition_revision_id": revision.revision_id,
            "source_bundle_digest": revision.source_bundle.digest,
            "authoring_mount_id": revision.authoring_mount_id,
        }),
    }
}
