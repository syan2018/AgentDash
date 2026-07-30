use std::collections::BTreeSet;
use std::sync::Arc;

use agentdash_agent_runtime_contract::RuntimeThreadId;
use agentdash_application_agentrun::agent_run::{
    AgentFrameSurfaceExt, AgentRunAppliedResourceSurface, AgentRunAppliedResourceSurfaceQueryError,
    AgentRunAppliedResourceSurfaceQueryPort, AgentRunProductRuntimeBinding,
    AgentRunProductRuntimeBindingRepository, AppliedVfsMount, AppliedVfsOperation,
    ProductAgentSurfaceFacts,
};
use agentdash_application_operation_gateway::OperationAuthorityGrant;
use agentdash_domain::agent_run_target::AgentRunTarget;
use agentdash_domain::operation::OperationPrincipalRef;
use agentdash_domain::workflow::{AgentFrame, AgentFrameRepository};
use agentdash_platform_spi::{CapabilityState, Mount, MountCapability, RuntimeMcpServer, Vfs};
use async_trait::async_trait;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionAuthorityRequest {
    target: Option<AgentRunTarget>,
    runtime_thread_id: Option<RuntimeThreadId>,
}

impl ExecutionAuthorityRequest {
    pub fn for_target(target: AgentRunTarget) -> Self {
        Self {
            target: Some(target),
            runtime_thread_id: None,
        }
    }

    pub fn for_runtime_thread(runtime_thread_id: RuntimeThreadId) -> Self {
        Self {
            target: None,
            runtime_thread_id: Some(runtime_thread_id),
        }
    }

    pub fn for_target_and_runtime_thread(
        target: AgentRunTarget,
        runtime_thread_id: RuntimeThreadId,
    ) -> Self {
        Self {
            target: Some(target),
            runtime_thread_id: Some(runtime_thread_id),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExecutionAuthority {
    principal: OperationPrincipalRef,
    runtime_thread_id: RuntimeThreadId,
    project_id: uuid::Uuid,
    revision: u64,
    digest: String,
    capability_state: CapabilityState,
    mcp_servers: Vec<RuntimeMcpServer>,
    resources: ExecutionResourceGrants,
    evidence: ExecutionAuthorityEvidence,
}

impl ExecutionAuthority {
    pub fn principal(&self) -> &OperationPrincipalRef {
        &self.principal
    }

    pub fn runtime_thread_id(&self) -> &RuntimeThreadId {
        &self.runtime_thread_id
    }

    pub fn project_id(&self) -> uuid::Uuid {
        self.project_id
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn digest(&self) -> &str {
        &self.digest
    }

    pub fn capability_state(&self) -> &CapabilityState {
        &self.capability_state
    }

    pub fn mcp_servers(&self) -> &[RuntimeMcpServer] {
        &self.mcp_servers
    }

    pub fn resources(&self) -> &ExecutionResourceGrants {
        &self.resources
    }

    pub fn evidence(&self) -> &ExecutionAuthorityEvidence {
        &self.evidence
    }

    fn operation_capabilities(&self) -> BTreeSet<String> {
        self.capability_state.capability_keys()
    }

    pub fn operation_authority_grant(&self) -> OperationAuthorityGrant {
        let mut capabilities = self.operation_capabilities();
        capabilities.insert("operation.invoke".to_string());
        capabilities.insert("agent.operation.invoke".to_string());
        OperationAuthorityGrant {
            authority_revision: self.revision_token(),
            capabilities,
        }
    }

    pub fn revision_token(&self) -> String {
        format!(
            "{}:{}:{}",
            self.revision,
            self.digest,
            self.evidence.binding_digest()
        )
    }

    pub fn agent_run_target(&self) -> Option<AgentRunTarget> {
        match self.principal {
            OperationPrincipalRef::AgentRunAgent { run_id, agent_id } => {
                Some(AgentRunTarget { run_id, agent_id })
            }
            _ => None,
        }
    }

    pub fn applied_resource_projection(&self) -> Option<AgentRunAppliedResourceSurface> {
        let target = self.agent_run_target()?;
        Some(AgentRunAppliedResourceSurface {
            target,
            project_id: self.project_id,
            workspace_id: self.resources.workspace_id,
            vfs_mounts: self.resources.vfs_mounts.clone(),
            default_mount_id: self.resources.default_mount_id.clone(),
            vfs_grants: self.resources.vfs_grants.clone(),
            agent_surface_revision: self.revision,
            agent_surface_digest: self.digest.clone(),
            vfs_digest: self.resources.vfs_digest.clone(),
            task_grants: self.resources.task_grants.clone(),
            task_surface_digest: self.resources.task_digest.clone(),
            product_binding_digest: self.evidence.binding_digest.clone(),
            provenance:
                agentdash_application_agentrun::agent_run::AgentRunAppliedResourceSurfaceProvenance {
                    source_kind: self.evidence.source_kind.clone(),
                    source_id: self.evidence.source_id.clone(),
                    source_revision: self.evidence.source_revision,
                    projection_revision: self.evidence.projection_revision,
                    captured_at_ms: self.evidence.captured_at_ms,
                },
        })
    }
}

#[derive(Debug, Clone)]
pub struct ExecutionResourceGrants {
    workspace_id: Option<uuid::Uuid>,
    vfs_mounts: Vec<AppliedVfsMount>,
    default_mount_id: Option<String>,
    vfs_grants: Vec<agentdash_application_agentrun::agent_run::AppliedVfsGrant>,
    vfs_digest: String,
    task_grants: Vec<agentdash_application_agentrun::agent_run::AppliedTaskGrant>,
    task_digest: String,
}

impl ExecutionResourceGrants {
    pub fn workspace_id(&self) -> Option<uuid::Uuid> {
        self.workspace_id
    }

    pub fn vfs_mounts(&self) -> &[AppliedVfsMount] {
        &self.vfs_mounts
    }

    pub fn default_mount_id(&self) -> Option<&str> {
        self.default_mount_id.as_deref()
    }

    pub fn vfs_digest(&self) -> &str {
        &self.vfs_digest
    }

    pub fn task_digest(&self) -> &str {
        &self.task_digest
    }

    pub fn vfs(&self, project_id: uuid::Uuid) -> Vfs {
        Vfs {
            mounts: self
                .vfs_mounts
                .iter()
                .cloned()
                .map(applied_vfs_mount)
                .collect(),
            default_mount_id: self.default_mount_id.clone(),
            source_project_id: Some(project_id.to_string()),
            source_story_id: None,
            links: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionAuthorityEvidence {
    binding_digest: String,
    source_kind: String,
    source_id: String,
    source_revision: u64,
    projection_revision: u64,
    captured_at_ms: u64,
}

impl ExecutionAuthorityEvidence {
    pub fn binding_digest(&self) -> &str {
        &self.binding_digest
    }

    pub fn source_kind(&self) -> &str {
        &self.source_kind
    }

    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    pub fn source_revision(&self) -> u64 {
        self.source_revision
    }

    pub fn projection_revision(&self) -> u64 {
        self.projection_revision
    }

    pub fn captured_at_ms(&self) -> u64 {
        self.captured_at_ms
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ExecutionAuthorityResolveError {
    #[error("AgentRun Product runtime binding is missing")]
    BindingMissing,
    #[error("AgentRun Product runtime binding query failed: {message}")]
    BindingQuery { message: String },
    #[error("AgentRun Product runtime binding does not match the requested target")]
    BindingTargetMismatch,
    #[error("AgentRun Product runtime binding does not match the requested RuntimeThread")]
    BindingRuntimeThreadMismatch,
    #[error("AgentRun Product runtime binding is invalid: {message}")]
    BindingInvalid { message: String },
    #[error("accepted AgentFrame is missing: {frame_id}")]
    FrameMissing { frame_id: uuid::Uuid },
    #[error("accepted AgentFrame query failed: {message}")]
    FrameQuery { message: String },
    #[error("accepted AgentFrame does not match the Product binding")]
    FrameBindingMismatch,
    #[error("accepted AgentFrame has no typed capability surface")]
    CapabilitySurfaceMissing,
    #[error("AgentRun applied resource surface is unavailable: {message}")]
    AppliedSurface { message: String },
    #[error("AgentRun surface evidence does not match: field={field}")]
    EvidenceMismatch { field: &'static str },
}

impl ExecutionAuthorityResolveError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::BindingMissing => "execution_authority_binding_missing",
            Self::BindingQuery { .. } => "execution_authority_binding_query_failed",
            Self::BindingTargetMismatch => "execution_authority_binding_target_mismatch",
            Self::BindingRuntimeThreadMismatch => "execution_authority_runtime_thread_mismatch",
            Self::BindingInvalid { .. } => "execution_authority_binding_invalid",
            Self::FrameMissing { .. } => "execution_authority_frame_missing",
            Self::FrameQuery { .. } => "execution_authority_frame_query_failed",
            Self::FrameBindingMismatch => "execution_authority_frame_binding_mismatch",
            Self::CapabilitySurfaceMissing => "execution_authority_capability_missing",
            Self::AppliedSurface { .. } => "execution_authority_resources_unavailable",
            Self::EvidenceMismatch { .. } => "execution_authority_evidence_mismatch",
        }
    }
}

#[async_trait]
pub trait ExecutionAuthorityResolver: Send + Sync {
    async fn resolve(
        &self,
        request: ExecutionAuthorityRequest,
    ) -> Result<ExecutionAuthority, ExecutionAuthorityResolveError>;
}

#[derive(Clone)]
pub struct ProductExecutionAuthorityResolver {
    bindings: Arc<dyn AgentRunProductRuntimeBindingRepository>,
    frames: Arc<dyn AgentFrameRepository>,
    applied_resources: Arc<dyn AgentRunAppliedResourceSurfaceQueryPort>,
}

impl ProductExecutionAuthorityResolver {
    pub fn new(
        bindings: Arc<dyn AgentRunProductRuntimeBindingRepository>,
        frames: Arc<dyn AgentFrameRepository>,
        applied_resources: Arc<dyn AgentRunAppliedResourceSurfaceQueryPort>,
    ) -> Self {
        Self {
            bindings,
            frames,
            applied_resources,
        }
    }

    async fn binding(
        &self,
        request: &ExecutionAuthorityRequest,
    ) -> Result<AgentRunProductRuntimeBinding, ExecutionAuthorityResolveError> {
        let binding = if let Some(runtime_thread_id) = request.runtime_thread_id.as_ref() {
            self.bindings
                .load_product_binding_by_runtime_thread(runtime_thread_id)
                .await
        } else {
            self.bindings
                .load_product_binding(
                    request
                        .target
                        .as_ref()
                        .expect("AgentRun authority request has a target or runtime thread"),
                )
                .await
        }
        .map_err(|message| ExecutionAuthorityResolveError::BindingQuery { message })?
        .ok_or(ExecutionAuthorityResolveError::BindingMissing)?;
        if request
            .target
            .as_ref()
            .is_some_and(|target| binding.target != *target)
        {
            return Err(ExecutionAuthorityResolveError::BindingTargetMismatch);
        }
        if request
            .runtime_thread_id
            .as_ref()
            .is_some_and(|thread_id| thread_id != &binding.runtime_thread_id)
        {
            return Err(ExecutionAuthorityResolveError::BindingRuntimeThreadMismatch);
        }
        Ok(binding)
    }

    async fn frame(
        &self,
        binding: &AgentRunProductRuntimeBinding,
    ) -> Result<AgentFrame, ExecutionAuthorityResolveError> {
        let frame = self
            .frames
            .get(binding.launch_frame.frame_id)
            .await
            .map_err(|error| ExecutionAuthorityResolveError::FrameQuery {
                message: error.to_string(),
            })?
            .ok_or(ExecutionAuthorityResolveError::FrameMissing {
                frame_id: binding.launch_frame.frame_id,
            })?;
        if frame.agent_id != binding.target.agent_id
            || frame.id != binding.launch_frame.frame_id
            || u64::try_from(frame.revision).ok() != Some(binding.launch_frame.revision)
        {
            return Err(ExecutionAuthorityResolveError::FrameBindingMismatch);
        }
        Ok(frame)
    }
}

#[async_trait]
impl ExecutionAuthorityResolver for ProductExecutionAuthorityResolver {
    async fn resolve(
        &self,
        request: ExecutionAuthorityRequest,
    ) -> Result<ExecutionAuthority, ExecutionAuthorityResolveError> {
        let binding = self.binding(&request).await?;
        let binding_digest = binding
            .calculated_digest()
            .map_err(|message| ExecutionAuthorityResolveError::BindingInvalid { message })?;
        let frame = self.frame(&binding).await?;
        let capability_state = frame
            .typed_capability_state()
            .ok_or(ExecutionAuthorityResolveError::CapabilitySurfaceMissing)?;
        let mcp_servers = frame.typed_mcp_servers();
        let applied_resources = self
            .applied_resources
            .applied_resource_surface(&binding.target)
            .await
            .map_err(applied_surface_error)?;
        applied_resources
            .validate_for(&binding.target)
            .map_err(applied_surface_error)?;
        let surface_facts = ProductAgentSurfaceFacts::from_frame(&frame);
        for (field, matches) in [
            (
                "product_binding_digest",
                applied_resources.product_binding_digest == binding_digest,
            ),
            (
                "provenance.source_kind",
                applied_resources.provenance.source_kind == "agent_frame",
            ),
            (
                "provenance.source_id",
                applied_resources.provenance.source_id == frame.id.to_string(),
            ),
            (
                "provenance.source_revision",
                applied_resources.provenance.source_revision == binding.launch_frame.revision,
            ),
            (
                "agent_surface_revision",
                applied_resources.agent_surface_revision == surface_facts.surface_revision,
            ),
            (
                "agent_surface_digest",
                applied_resources.agent_surface_digest == surface_facts.surface_digest,
            ),
        ] {
            if !matches {
                return Err(ExecutionAuthorityResolveError::EvidenceMismatch { field });
            }
        }
        if applied_resources.project_id.is_nil() {
            return Err(ExecutionAuthorityResolveError::EvidenceMismatch {
                field: "project_id",
            });
        }

        let resources = ExecutionResourceGrants {
            workspace_id: applied_resources.workspace_id,
            vfs_mounts: applied_resources.vfs_mounts,
            default_mount_id: applied_resources.default_mount_id,
            vfs_grants: applied_resources.vfs_grants,
            vfs_digest: applied_resources.vfs_digest,
            task_grants: applied_resources.task_grants,
            task_digest: applied_resources.task_surface_digest,
        };
        let evidence = ExecutionAuthorityEvidence {
            binding_digest,
            source_kind: applied_resources.provenance.source_kind,
            source_id: applied_resources.provenance.source_id,
            source_revision: applied_resources.provenance.source_revision,
            projection_revision: applied_resources.provenance.projection_revision,
            captured_at_ms: applied_resources.provenance.captured_at_ms,
        };
        Ok(ExecutionAuthority {
            principal: OperationPrincipalRef::AgentRunAgent {
                run_id: binding.target.run_id,
                agent_id: binding.target.agent_id,
            },
            runtime_thread_id: binding.runtime_thread_id,
            project_id: applied_resources.project_id,
            revision: surface_facts.surface_revision,
            digest: surface_facts.surface_digest,
            capability_state,
            mcp_servers,
            resources,
            evidence,
        })
    }
}

fn applied_surface_error(
    error: AgentRunAppliedResourceSurfaceQueryError,
) -> ExecutionAuthorityResolveError {
    ExecutionAuthorityResolveError::AppliedSurface {
        message: error.to_string(),
    }
}

fn applied_vfs_mount(mount: AppliedVfsMount) -> Mount {
    Mount {
        id: mount.mount_id,
        provider: mount.provider,
        backend_id: mount.backend_id,
        root_ref: mount.root_ref,
        capabilities: mount
            .capabilities
            .into_iter()
            .map(|operation| match operation {
                AppliedVfsOperation::Read => MountCapability::Read,
                AppliedVfsOperation::List => MountCapability::List,
                AppliedVfsOperation::Search => MountCapability::Search,
                AppliedVfsOperation::Write => MountCapability::Write,
                AppliedVfsOperation::Exec => MountCapability::Exec,
            })
            .collect(),
        default_write: mount.default_write,
        display_name: mount.display_name,
        metadata: mount.metadata,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use agentdash_agent_runtime_contract::{AgentServiceInstanceId, AgentSourceCoordinate};
    use agentdash_application_agentrun::agent_run::{
        AgentRunAppliedResourceSurfaceProvenance, AgentRunCompleteAgentAssociation,
        ProductAgentFrameRef, ProductExecutionProfileRef,
    };
    use agentdash_domain::DomainError;
    use agentdash_platform_spi::ToolCluster;
    use async_trait::async_trait;

    use super::*;

    struct StaticBindingRepository {
        binding: AgentRunProductRuntimeBinding,
    }

    #[async_trait]
    impl AgentRunProductRuntimeBindingRepository for StaticBindingRepository {
        async fn load_product_binding(
            &self,
            target: &AgentRunTarget,
        ) -> Result<Option<AgentRunProductRuntimeBinding>, String> {
            Ok((self.binding.target == *target).then(|| self.binding.clone()))
        }

        async fn load_product_binding_by_runtime_thread(
            &self,
            runtime_thread_id: &RuntimeThreadId,
        ) -> Result<Option<AgentRunProductRuntimeBinding>, String> {
            Ok(
                (self.binding.runtime_thread_id == *runtime_thread_id)
                    .then(|| self.binding.clone()),
            )
        }
    }

    struct StaticFrameRepository {
        accepted: AgentFrame,
        latest: AgentFrame,
    }

    #[async_trait]
    impl AgentFrameRepository for StaticFrameRepository {
        async fn create(&self, _frame: &AgentFrame) -> Result<(), DomainError> {
            Ok(())
        }

        async fn get(&self, frame_id: uuid::Uuid) -> Result<Option<AgentFrame>, DomainError> {
            Ok((self.accepted.id == frame_id).then(|| self.accepted.clone()))
        }

        async fn get_latest(
            &self,
            _agent_id: uuid::Uuid,
        ) -> Result<Option<AgentFrame>, DomainError> {
            Ok(Some(self.latest.clone()))
        }

        async fn list_by_agent(
            &self,
            _agent_id: uuid::Uuid,
        ) -> Result<Vec<AgentFrame>, DomainError> {
            Ok(vec![self.accepted.clone(), self.latest.clone()])
        }
    }

    struct StaticAppliedSurface {
        surface: AgentRunAppliedResourceSurface,
    }

    #[async_trait]
    impl AgentRunAppliedResourceSurfaceQueryPort for StaticAppliedSurface {
        async fn applied_resource_surface(
            &self,
            _target: &AgentRunTarget,
        ) -> Result<AgentRunAppliedResourceSurface, AgentRunAppliedResourceSurfaceQueryError>
        {
            Ok(self.surface.clone())
        }

        async fn applied_resource_surface_at(
            &self,
            _target: &AgentRunTarget,
            _agent_surface_revision: u64,
        ) -> Result<AgentRunAppliedResourceSurface, AgentRunAppliedResourceSurfaceQueryError>
        {
            Ok(self.surface.clone())
        }
    }

    fn fixture() -> (
        ProductExecutionAuthorityResolver,
        ExecutionAuthorityRequest,
        uuid::Uuid,
    ) {
        let target = AgentRunTarget {
            run_id: uuid::Uuid::new_v4(),
            agent_id: uuid::Uuid::new_v4(),
        };
        let mut accepted = AgentFrame::new_revision(target.agent_id, 1, "accepted");
        accepted.surface.capability_state = Some(
            serde_json::to_value(CapabilityState::from_clusters([
                ToolCluster::Read,
                ToolCluster::Task,
                ToolCluster::WorkspaceModule,
            ]))
            .expect("capability"),
        );
        let mut latest = AgentFrame::new_revision(target.agent_id, 2, "unaccepted");
        latest.surface.capability_state = Some(
            serde_json::to_value(CapabilityState::from_clusters([ToolCluster::Workflow]))
                .expect("capability"),
        );
        let mut execution_profile = ProductExecutionProfileRef {
            profile_key: "fixture".to_string(),
            profile_revision: 1,
            profile_digest: String::new(),
            configuration: serde_json::json!({}),
            credential_scope: None,
        };
        execution_profile.refresh_digest();
        let binding = AgentRunProductRuntimeBinding {
            target: target.clone(),
            runtime_thread_id: RuntimeThreadId::new("runtime-thread").expect("thread"),
            agent: AgentRunCompleteAgentAssociation {
                service_instance_id: AgentServiceInstanceId::new("fixture").expect("service"),
                source: AgentSourceCoordinate::new("source").expect("source"),
            },
            launch_frame: ProductAgentFrameRef {
                frame_id: accepted.id,
                agent_id: accepted.agent_id,
                revision: 1,
            },
            execution_profile_digest: execution_profile.profile_digest.clone(),
            execution_profile,
        };
        let binding_digest = binding.calculated_digest().expect("binding digest");
        let surface_facts = ProductAgentSurfaceFacts::from_frame(&accepted);
        let project_id = uuid::Uuid::new_v4();
        let applied = AgentRunAppliedResourceSurface {
            target: target.clone(),
            project_id,
            workspace_id: None,
            vfs_mounts: Vec::new(),
            default_mount_id: None,
            vfs_grants: Vec::new(),
            agent_surface_revision: surface_facts.surface_revision,
            agent_surface_digest: surface_facts.surface_digest,
            vfs_digest: "vfs-digest".to_string(),
            task_grants: Vec::new(),
            task_surface_digest: "task-digest".to_string(),
            product_binding_digest: binding_digest,
            provenance: AgentRunAppliedResourceSurfaceProvenance {
                source_kind: "agent_frame".to_string(),
                source_id: accepted.id.to_string(),
                source_revision: 1,
                projection_revision: 1,
                captured_at_ms: 1,
            },
        };
        (
            ProductExecutionAuthorityResolver::new(
                Arc::new(StaticBindingRepository { binding }),
                Arc::new(StaticFrameRepository { accepted, latest }),
                Arc::new(StaticAppliedSurface { surface: applied }),
            ),
            ExecutionAuthorityRequest::for_target(target),
            project_id,
        )
    }

    #[tokio::test]
    async fn resolves_only_the_binding_accepted_frame_and_normalizes_capabilities() {
        let (authority, request, project_id) = fixture();

        let resolved = authority.resolve(request).await.expect("surface");

        assert_eq!(resolved.revision(), 1);
        assert_eq!(resolved.project_id(), project_id);
        let operation_authority = resolved.operation_authority_grant();
        assert_eq!(
            operation_authority.authority_revision,
            resolved.revision_token()
        );
        assert_eq!(
            operation_authority.capabilities,
            BTreeSet::from([
                "agent.operation.invoke".to_string(),
                "file_read".to_string(),
                "operation.invoke".to_string(),
                "task".to_string(),
                "workspace_module".to_string(),
            ])
        );
    }

    #[tokio::test]
    async fn runtime_thread_locator_returns_the_authority_value_directly() {
        let (resolver, request, project_id) = fixture();
        let target = request.target.expect("target");

        let resolved = resolver
            .resolve(ExecutionAuthorityRequest::for_runtime_thread(
                RuntimeThreadId::new("runtime-thread").expect("thread"),
            ))
            .await
            .expect("authority");

        assert_eq!(resolved.agent_run_target(), Some(target));
        assert_eq!(resolved.project_id(), project_id);
        assert!(resolved.revision_token().contains(resolved.digest()));
    }

    #[tokio::test]
    async fn rejects_applied_evidence_from_another_binding() {
        let (authority, request, _) = fixture();
        let broken = ProductExecutionAuthorityResolver::new(
            authority.bindings.clone(),
            authority.frames.clone(),
            Arc::new(StaticAppliedSurface {
                surface: AgentRunAppliedResourceSurface {
                    product_binding_digest: "other-binding".to_string(),
                    ..authority
                        .applied_resources
                        .applied_resource_surface(request.target.as_ref().expect("target"))
                        .await
                        .expect("surface")
                },
            }),
        );

        assert_eq!(
            broken.resolve(request).await.expect_err("must reject"),
            ExecutionAuthorityResolveError::EvidenceMismatch {
                field: "product_binding_digest",
            }
        );
    }
}
