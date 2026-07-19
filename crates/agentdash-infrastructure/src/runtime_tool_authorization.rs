use std::sync::Arc;

use agentdash_agent_runtime::{
    RuntimeTaskExecutionGrant, RuntimeTaskGrantedOperation, RuntimeToolAppliedSurfaceEvidence,
    RuntimeToolAuthorizationGrant, RuntimeToolAuthorizationPort, RuntimeToolAuthorizationRequest,
    RuntimeToolBrokerError, RuntimeToolProductTarget, RuntimeToolResourceGrant,
    RuntimeVfsExecutionGrant, RuntimeVfsGrantedOperation, RuntimeVfsMountGrant,
    RuntimeVfsPathGrant,
};
use agentdash_agent_runtime_contract::RuntimeThreadId;
use agentdash_application_agentrun::agent_run::{
    AgentRunAppliedResourceSurfaceQueryError, AgentRunAppliedResourceSurfaceQueryPort,
    AgentRunAppliedResourceSurfaceSnapshot, AgentRunProductRuntimeBinding, AppliedTaskOperation,
    AppliedTaskScope, AppliedVfsOperation, AppliedVfsPathScope,
};
use async_trait::async_trait;

/// Runtime tool authorization only consumes an immutable Product activation record.
///
/// The PostgreSQL repository and its transaction semantics are intentionally supplied by the
/// Product projection composition owner. Keeping this DTO beside the adapter prevents the Runtime
/// tool surface from assuming a migration or concrete persistence layout.
#[derive(Clone, Debug)]
pub struct CommittedRuntimeToolProductBinding {
    pub binding: AgentRunProductRuntimeBinding,
    pub binding_digest: String,
    pub applied_resource_snapshot_revision: Option<u64>,
    pub applied_resource_binding_generation: Option<u64>,
}

pub struct ProductRuntimeToolAuthorizer {
    bindings: Arc<dyn RuntimeToolProductBindingQueryPort>,
    surfaces: Arc<dyn AgentRunAppliedResourceSurfaceQueryPort>,
}

#[async_trait]
pub trait RuntimeToolProductBindingQueryPort: Send + Sync {
    async fn binding_and_digest(
        &self,
        runtime_thread_id: &RuntimeThreadId,
    ) -> Result<Option<CommittedRuntimeToolProductBinding>, String>;
}

impl ProductRuntimeToolAuthorizer {
    pub fn new(
        bindings: Arc<dyn RuntimeToolProductBindingQueryPort>,
        surfaces: Arc<dyn AgentRunAppliedResourceSurfaceQueryPort>,
    ) -> Self {
        Self { bindings, surfaces }
    }
}

#[async_trait]
impl RuntimeToolAuthorizationPort for ProductRuntimeToolAuthorizer {
    async fn authorize(
        &self,
        request: RuntimeToolAuthorizationRequest,
    ) -> Result<RuntimeToolAuthorizationGrant, RuntimeToolBrokerError> {
        let binding = self
            .bindings
            .binding_and_digest(&request.context.runtime_thread_id)
            .await
            .map_err(|message| denied("product_binding_query_failed", message))?
            .ok_or_else(|| {
                denied(
                    "missing_product_binding",
                    "runtime thread has no committed Product target binding",
                )
            })?;
        let binding_generation = binding.applied_resource_binding_generation.ok_or_else(|| {
            denied(
                "applied_resource_surface_not_activated",
                "Product resource surface is not pinned to a Host binding generation",
            )
        })?;
        if binding_generation != request.context.binding_generation.0 {
            return Err(denied(
                "stale_product_surface_generation",
                "Product resource snapshot is pinned to a different Host binding generation",
            ));
        }
        let snapshot_revision = binding.applied_resource_snapshot_revision.ok_or_else(|| {
            denied(
                "applied_resource_surface_not_activated",
                "Product resource surface is not pinned before activation",
            )
        })?;
        ensure_current_surface(&binding.binding, &request)?;
        let snapshot = self
            .surfaces
            .applied_resource_surface(&binding.binding.target, Some(snapshot_revision))
            .await
            .map_err(map_surface_query_error)?;
        if snapshot.surface.product_binding_digest != binding.binding_digest {
            return Err(denied(
                "stale_product_binding",
                "Product resource surface does not attest the committed Product binding digest",
            ));
        }
        authorize_snapshot(snapshot, request)
    }
}

fn authorize_snapshot(
    snapshot: AgentRunAppliedResourceSurfaceSnapshot,
    request: RuntimeToolAuthorizationRequest,
) -> Result<RuntimeToolAuthorizationGrant, RuntimeToolBrokerError> {
    let surface = &snapshot.surface;
    if surface.agent_surface_revision != request.context.applied_surface_revision.0
        || surface.agent_surface_digest != request.context.applied_surface_digest.as_str()
    {
        return Err(denied(
            "stale_product_surface",
            "Product snapshot does not attest the callback applied surface revision and digest",
        ));
    }
    let resources = match request.definition.name.as_str() {
        "mounts_list" => {
            let mut mounts = Vec::new();
            for grant in &surface.vfs_grants {
                if !grant.operations.contains(&AppliedVfsOperation::List) {
                    continue;
                }
                let mount = surface
                    .vfs_mounts
                    .iter()
                    .find(|mount| mount.mount_id == grant.mount_id)
                    .ok_or_else(|| {
                        denied(
                            "corrupt_vfs_grant",
                            "VFS grant references an absent applied mount",
                        )
                    })?;
                mounts.push(RuntimeVfsMountGrant {
                    id: mount.mount_id.clone(),
                    provider: mount.provider.clone(),
                    backend_id: mount.backend_id.clone(),
                    root_ref: mount.root_ref.clone(),
                    display_name: mount.display_name.clone(),
                    metadata: serde_json::Value::Null,
                    operations: grant
                        .operations
                        .iter()
                        .copied()
                        .map(map_vfs_operation)
                        .collect(),
                    path_scopes: grant
                        .path_scopes
                        .iter()
                        .cloned()
                        .map(map_path_scope)
                        .collect(),
                });
            }
            if mounts.is_empty() {
                return Err(denied(
                    "missing_vfs_list_grant",
                    "applied Product surface grants no VFS mount List operation",
                ));
            }
            let default_mount_id = surface
                .default_mount_id
                .clone()
                .filter(|id| mounts.iter().any(|mount| &mount.id == id));
            RuntimeToolResourceGrant::Vfs(RuntimeVfsExecutionGrant {
                default_mount_id,
                mounts,
            })
        }
        "task_read" => task_grant(surface, AppliedTaskOperation::Read)?,
        "task_write" => task_grant(surface, AppliedTaskOperation::Write)?,
        _ => {
            return Err(denied(
                "unsupported_runtime_tool_policy",
                "no Product authorization policy is registered for this runtime tool",
            ));
        }
    };
    Ok(RuntimeToolAuthorizationGrant {
        permission: request.definition.permission,
        effect: request.definition.effect,
        target: RuntimeToolProductTarget {
            project_id: surface.project_id.to_string(),
            run_id: surface.target.run_id.to_string(),
            agent_id: surface.target.agent_id.to_string(),
        },
        applied_surface: RuntimeToolAppliedSurfaceEvidence {
            snapshot_revision: snapshot.snapshot_revision,
            revision: surface.agent_surface_revision,
            digest: surface.agent_surface_digest.clone(),
            projection_revision: surface.provenance.projection_revision,
            provenance_source: format!(
                "{}:{}",
                surface.provenance.source_kind, surface.provenance.source_id
            ),
            provenance_revision: surface.provenance.source_revision,
        },
        resources,
    })
}

fn task_grant(
    surface: &agentdash_application_agentrun::agent_run::AgentRunAppliedResourceSurface,
    required: AppliedTaskOperation,
) -> Result<RuntimeToolResourceGrant, RuntimeToolBrokerError> {
    let scope = AppliedTaskScope::Project {
        project_id: surface.project_id,
    };
    let grant = surface
        .task_grants
        .iter()
        .find(|grant| grant.scope == scope && grant.operations.contains(&required))
        .ok_or_else(|| {
            denied(
                "missing_task_grant",
                format!("applied Product surface does not grant Task {required:?}"),
            )
        })?;
    Ok(RuntimeToolResourceGrant::Task(RuntimeTaskExecutionGrant {
        plan_revision: surface.task_surface_revision,
        plan_digest: surface.task_surface_digest.clone(),
        operations: grant
            .operations
            .iter()
            .copied()
            .map(|operation| match operation {
                AppliedTaskOperation::Read => RuntimeTaskGrantedOperation::Read,
                AppliedTaskOperation::Write => RuntimeTaskGrantedOperation::Write,
            })
            .collect(),
    }))
}

fn map_vfs_operation(operation: AppliedVfsOperation) -> RuntimeVfsGrantedOperation {
    match operation {
        AppliedVfsOperation::Read => RuntimeVfsGrantedOperation::Read,
        AppliedVfsOperation::List => RuntimeVfsGrantedOperation::List,
        AppliedVfsOperation::Search => RuntimeVfsGrantedOperation::Search,
        AppliedVfsOperation::Write => RuntimeVfsGrantedOperation::Write,
        AppliedVfsOperation::Exec => RuntimeVfsGrantedOperation::Execute,
    }
}

fn map_path_scope(scope: AppliedVfsPathScope) -> RuntimeVfsPathGrant {
    match scope {
        AppliedVfsPathScope::All => RuntimeVfsPathGrant::All,
        AppliedVfsPathScope::Prefix(path) => RuntimeVfsPathGrant::Prefix(path),
        AppliedVfsPathScope::Exact(path) => RuntimeVfsPathGrant::Exact(path),
    }
}

fn map_surface_query_error(
    error: AgentRunAppliedResourceSurfaceQueryError,
) -> RuntimeToolBrokerError {
    match error {
        AgentRunAppliedResourceSurfaceQueryError::SurfaceNotApplied => denied(
            "applied_resource_surface_missing",
            "Product resource surface has not been materialized before activation",
        ),
        AgentRunAppliedResourceSurfaceQueryError::ProjectionStale { .. } => {
            denied("stale_product_surface", error.to_string())
        }
        AgentRunAppliedResourceSurfaceQueryError::TargetMismatch
        | AgentRunAppliedResourceSurfaceQueryError::CorruptEvidence { .. } => {
            denied("invalid_product_surface", error.to_string())
        }
        AgentRunAppliedResourceSurfaceQueryError::Repository { .. } => {
            denied("product_surface_query_failed", error.to_string())
        }
    }
}

fn ensure_current_surface(
    binding: &AgentRunProductRuntimeBinding,
    request: &RuntimeToolAuthorizationRequest,
) -> Result<(), RuntimeToolBrokerError> {
    if binding.runtime_thread_id != request.context.runtime_thread_id {
        return Err(denied(
            "stale_product_binding",
            "Product target binding belongs to a different Runtime thread",
        ));
    }
    if binding.source_binding.source_ref.as_str() != request.context.source.as_str() {
        return Err(denied(
            "stale_product_binding",
            "Product target binding belongs to a different Agent source",
        ));
    }
    if binding.source_binding.applied_surface_revision.0
        != request.context.applied_surface_revision.0
    {
        return Err(denied(
            "stale_product_surface",
            "Product target binding does not attest the callback applied surface revision",
        ));
    }
    Ok(())
}

fn denied(code: impl Into<String>, message: impl Into<String>) -> RuntimeToolBrokerError {
    RuntimeToolBrokerError::AuthorizationDenied {
        code: code.into(),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use agentdash_agent_runtime::{
        RuntimeToolDefinition, RuntimeToolEffect, RuntimeToolPermission, RuntimeToolResolvedContext,
    };
    use agentdash_agent_runtime_contract::{
        ManagedRuntimeSourceBindingEvidence, RuntimeProjectionRevision, RuntimeSourceRef,
        SurfaceRevision,
    };
    use agentdash_agent_service_api::{
        AgentBindingGeneration, AgentProfileDigest, AgentServiceInstanceId, AgentSourceCoordinate,
        AgentSurfaceDigest, AgentSurfaceRevision, AgentToolName,
    };
    use agentdash_application_agentrun::agent_run::{
        AgentRunAppliedResourceSurface, AgentRunAppliedResourceSurfaceProvenance, AppliedTaskGrant,
        AppliedVfsGrant, AppliedVfsMount,
    };
    use agentdash_domain::agent_run_target::AgentRunTarget;
    use uuid::Uuid;

    use super::*;

    struct BindingFixture {
        value: Option<CommittedRuntimeToolProductBinding>,
    }

    #[async_trait]
    impl RuntimeToolProductBindingQueryPort for BindingFixture {
        async fn binding_and_digest(
            &self,
            _runtime_thread_id: &RuntimeThreadId,
        ) -> Result<Option<CommittedRuntimeToolProductBinding>, String> {
            Ok(self.value.clone())
        }
    }

    struct SurfaceFixture {
        value: Result<
            AgentRunAppliedResourceSurfaceSnapshot,
            AgentRunAppliedResourceSurfaceQueryError,
        >,
    }

    #[async_trait]
    impl AgentRunAppliedResourceSurfaceQueryPort for SurfaceFixture {
        async fn applied_resource_surface(
            &self,
            _target: &AgentRunTarget,
            expected_snapshot_revision: Option<u64>,
        ) -> Result<AgentRunAppliedResourceSurfaceSnapshot, AgentRunAppliedResourceSurfaceQueryError>
        {
            let snapshot = self.value.clone()?;
            if let Some(expected) = expected_snapshot_revision
                && snapshot.snapshot_revision != expected
            {
                return Err(AgentRunAppliedResourceSurfaceQueryError::ProjectionStale {
                    expected_revision: expected,
                    actual_revision: snapshot.snapshot_revision,
                });
            }
            Ok(snapshot)
        }
    }

    #[tokio::test]
    async fn missing_applied_surface_is_typed_deny() {
        let binding = binding();
        let authorizer = ProductRuntimeToolAuthorizer::new(
            Arc::new(BindingFixture {
                value: Some(binding),
            }),
            Arc::new(SurfaceFixture {
                value: Err(AgentRunAppliedResourceSurfaceQueryError::SurfaceNotApplied),
            }),
        );
        let error = authorizer
            .authorize(request("mounts_list"))
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            RuntimeToolBrokerError::AuthorizationDenied { code, .. }
                if code == "applied_resource_surface_missing"
        ));
    }

    #[tokio::test]
    async fn stale_surface_digest_is_typed_deny() {
        let binding = binding();
        let mut snapshot = snapshot(
            binding.binding.target.clone(),
            binding.binding_digest.clone(),
        );
        snapshot.surface.agent_surface_digest = "different".to_owned();
        let authorizer = ProductRuntimeToolAuthorizer::new(
            Arc::new(BindingFixture {
                value: Some(binding),
            }),
            Arc::new(SurfaceFixture {
                value: Ok(snapshot),
            }),
        );
        let error = authorizer
            .authorize(request("mounts_list"))
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            RuntimeToolBrokerError::AuthorizationDenied { code, .. }
                if code == "stale_product_surface"
        ));
    }

    #[tokio::test]
    async fn task_read_grant_does_not_authorize_task_write() {
        let binding = binding();
        let snapshot = snapshot(
            binding.binding.target.clone(),
            binding.binding_digest.clone(),
        );
        let authorizer = ProductRuntimeToolAuthorizer::new(
            Arc::new(BindingFixture {
                value: Some(binding),
            }),
            Arc::new(SurfaceFixture {
                value: Ok(snapshot),
            }),
        );
        let error = authorizer
            .authorize(request("task_write"))
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            RuntimeToolBrokerError::AuthorizationDenied { code, .. }
                if code == "missing_task_grant"
        ));
    }

    #[tokio::test]
    async fn old_activation_cannot_observe_newer_grant_snapshot() {
        let binding = binding();
        let mut newer = snapshot(
            binding.binding.target.clone(),
            binding.binding_digest.clone(),
        );
        newer.snapshot_revision = 2;
        newer.surface.vfs_grants[0]
            .operations
            .insert(AppliedVfsOperation::Write);
        let authorizer = ProductRuntimeToolAuthorizer::new(
            Arc::new(BindingFixture {
                value: Some(binding),
            }),
            Arc::new(SurfaceFixture { value: Ok(newer) }),
        );
        let error = authorizer
            .authorize(request("mounts_list"))
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            RuntimeToolBrokerError::AuthorizationDenied { code, .. }
                if code == "stale_product_surface"
        ));
    }

    fn binding() -> CommittedRuntimeToolProductBinding {
        let target = AgentRunTarget {
            run_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
        };
        CommittedRuntimeToolProductBinding {
            binding: AgentRunProductRuntimeBinding {
                target,
                runtime_thread_id: RuntimeThreadId::new("thread-test").unwrap(),
                source_binding: ManagedRuntimeSourceBindingEvidence {
                    source_ref: RuntimeSourceRef::new("source-test").unwrap(),
                    committed_at_revision: RuntimeProjectionRevision(1),
                    applied_surface_revision: SurfaceRevision(1),
                    activated_at_revision: Some(RuntimeProjectionRevision(1)),
                },
            },
            binding_digest: "binding-test".to_owned(),
            applied_resource_snapshot_revision: Some(1),
            applied_resource_binding_generation: Some(1),
        }
    }

    fn snapshot(
        target: AgentRunTarget,
        product_binding_digest: String,
    ) -> AgentRunAppliedResourceSurfaceSnapshot {
        let project_id = Uuid::new_v4();
        let provenance = AgentRunAppliedResourceSurfaceProvenance {
            source_kind: "product".to_owned(),
            source_id: "surface-test".to_owned(),
            source_revision: 1,
            projection_revision: 1,
            captured_at_ms: 1,
        };
        AgentRunAppliedResourceSurfaceSnapshot {
            snapshot_revision: 1,
            surface: AgentRunAppliedResourceSurface {
                target,
                project_id,
                workspace_id: None,
                vfs_mounts: vec![AppliedVfsMount {
                    mount_id: "main".to_owned(),
                    provider: "memory".to_owned(),
                    backend_id: "backend".to_owned(),
                    root_ref: "memory://main".to_owned(),
                    capabilities: BTreeSet::from([
                        AppliedVfsOperation::Read,
                        AppliedVfsOperation::List,
                    ]),
                    default_write: false,
                    display_name: "Main".to_owned(),
                }],
                default_mount_id: Some("main".to_owned()),
                vfs_grants: vec![AppliedVfsGrant {
                    mount_id: "main".to_owned(),
                    operations: BTreeSet::from([
                        AppliedVfsOperation::Read,
                        AppliedVfsOperation::List,
                    ]),
                    path_scopes: vec![AppliedVfsPathScope::All],
                }],
                agent_surface_revision: 1,
                agent_surface_digest: "applied-test".to_owned(),
                vfs_digest: "vfs-test".to_owned(),
                task_grants: vec![AppliedTaskGrant {
                    scope: AppliedTaskScope::Project { project_id },
                    operations: BTreeSet::from([AppliedTaskOperation::Read]),
                }],
                task_surface_revision: 1,
                task_surface_digest: "task-test".to_owned(),
                task_provenance: provenance.clone(),
                product_binding_digest,
                provenance,
            },
        }
    }

    fn request(tool: &str) -> RuntimeToolAuthorizationRequest {
        let (permission, effect) = match tool {
            "task_write" => (
                RuntimeToolPermission::ProductWrite,
                RuntimeToolEffect::ProductMutation,
            ),
            "task_read" => (
                RuntimeToolPermission::ProductRead,
                RuntimeToolEffect::ReadOnly,
            ),
            _ => (RuntimeToolPermission::VfsRead, RuntimeToolEffect::ReadOnly),
        };
        RuntimeToolAuthorizationRequest {
            context: RuntimeToolResolvedContext {
                runtime_thread_id: RuntimeThreadId::new("thread-test").unwrap(),
                binding_generation: AgentBindingGeneration(1),
                source: AgentSourceCoordinate::new("source-test").unwrap(),
                service_instance_id: AgentServiceInstanceId::new("service-test").unwrap(),
                profile_digest: AgentProfileDigest::new("profile-test").unwrap(),
                bound_surface_revision: AgentSurfaceRevision(1),
                bound_surface_digest: AgentSurfaceDigest::new("bound-test").unwrap(),
                applied_surface_revision: AgentSurfaceRevision(1),
                applied_surface_digest: AgentSurfaceDigest::new("applied-test").unwrap(),
            },
            definition: RuntimeToolDefinition {
                name: AgentToolName::new(tool).unwrap(),
                description: tool.to_owned(),
                parameters_schema: serde_json::json!({}),
                permission,
                effect,
            },
            arguments: serde_json::json!({}),
        }
    }
}
