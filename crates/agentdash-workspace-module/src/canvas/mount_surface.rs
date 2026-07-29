use std::sync::Arc;

use agentdash_application_ports::agent_frame_materialization::{
    AgentRunRuntimeSurfaceUpdatePort, RuntimeSurfaceChange, RuntimeSurfaceUpdateRequest,
};
use agentdash_domain::agent_run_target::AgentRunTarget;
use agentdash_domain::common::{Mount, MountCapability};
use agentdash_domain::interaction::{
    InteractionDefinitionRepository, InteractionDefinitionRevision, InteractionDefinitionStatus,
    InteractionOwner,
};
use agentdash_domain::workflow::{LifecycleAgentRepository, MountDirective};
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
    surface_updates: Arc<dyn AgentRunRuntimeSurfaceUpdatePort>,
}

impl ProductCanvasMountMaterializer {
    pub fn new(
        definitions: Arc<dyn InteractionDefinitionRepository>,
        agents: Arc<dyn LifecycleAgentRepository>,
        surface_updates: Arc<dyn AgentRunRuntimeSurfaceUpdatePort>,
    ) -> Self {
        Self {
            definitions,
            agents,
            surface_updates,
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
        let module_ref = format!("canvas:{}", revision.definition_id);
        let mount = canvas_authoring_mount(&revision, &user_id);
        let outcome = self
            .surface_updates
            .execute_runtime_surface_update(RuntimeSurfaceUpdateRequest {
                target: request.target.clone(),
                idempotency_key: request.idempotency_key,
                created_by_kind: match request.intent {
                    CanvasMountIntentKind::Create => "canvas_mount_create",
                    CanvasMountIntentKind::Attach => "canvas_mount_attach",
                }
                .to_owned(),
                created_by_id: Some(request.definition_id.to_string()),
                changes: vec![
                    RuntimeSurfaceChange::AllowWorkspaceModule {
                        module_ref: module_ref.clone(),
                    },
                    RuntimeSurfaceChange::ApplyVfsDirectives {
                        directives: vec![MountDirective::AddMount { mount }],
                    },
                ],
            })
            .await
            .map_err(|error| CanvasMountMaterializationError::Failed(error.to_string()))?;
        Ok(CanvasMountConvergence {
            frame_id: outcome.frame_id.ok_or_else(|| {
                CanvasMountMaterializationError::Failed(
                    "runtime surface update 未返回 AgentFrame identity".to_owned(),
                )
            })?,
            frame_revision: outcome.frame_revision.ok_or_else(|| {
                CanvasMountMaterializationError::Failed(
                    "runtime surface update 未返回 AgentFrame revision".to_owned(),
                )
            })?,
            wrote_frame_revision: outcome.wrote_frame_revision,
            applied_generation: outcome.applied_generation,
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
