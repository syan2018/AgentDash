use std::{collections::BTreeSet, sync::Arc};

use agentdash_agent_runtime::{
    RuntimeTaskExecutionGrant, RuntimeTaskExecutionScope, RuntimeTaskGrantedOperation,
    RuntimeToolAppliedSurfaceEvidence, RuntimeToolAuthorizationGrant,
    RuntimeToolAuthorizationPolicy, RuntimeToolAuthorizationPort, RuntimeToolAuthorizationRequest,
    RuntimeToolBrokerError, RuntimeToolProductTarget, RuntimeToolProvenanceEvidence,
    RuntimeToolResourceGrant, RuntimeVfsExecutionGrant, RuntimeVfsGrantedOperation,
    RuntimeVfsMountGrant, RuntimeVfsPathGrant,
};
use agentdash_agent_runtime_contract::RuntimeThreadId;
use agentdash_application_agentrun::agent_run::{
    AgentRunAppliedResourceSurface, AppliedTaskOperation, AppliedTaskScope, AppliedVfsOperation,
    AppliedVfsPathScope,
};
use async_trait::async_trait;
use serde_json::Value;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct RuntimeToolExecutionAuthority {
    pub runtime_thread_id: RuntimeThreadId,
    pub capabilities: BTreeSet<String>,
    pub surface: AgentRunAppliedResourceSurface,
}

pub struct ProductRuntimeToolAuthorizer {
    authorities: Arc<dyn RuntimeToolExecutionAuthorityPort>,
}

#[async_trait]
pub trait RuntimeToolExecutionAuthorityPort: Send + Sync {
    async fn execution_authority(
        &self,
        runtime_thread_id: &RuntimeThreadId,
    ) -> Result<RuntimeToolExecutionAuthority, String>;
}

impl ProductRuntimeToolAuthorizer {
    pub fn new(authorities: Arc<dyn RuntimeToolExecutionAuthorityPort>) -> Self {
        Self { authorities }
    }
}

#[async_trait]
impl RuntimeToolAuthorizationPort for ProductRuntimeToolAuthorizer {
    async fn authorize(
        &self,
        request: RuntimeToolAuthorizationRequest,
    ) -> Result<RuntimeToolAuthorizationGrant, RuntimeToolBrokerError> {
        let authority = self
            .authorities
            .execution_authority(&request.context.runtime_thread_id)
            .await
            .map_err(|message| denied("execution_authority_unavailable", message))?;
        if authority.runtime_thread_id != request.context.runtime_thread_id {
            return Err(denied(
                "stale_execution_authority",
                "execution authority belongs to another runtime thread",
            ));
        }
        authorize_surface(authority.surface, &authority.capabilities, request)
    }
}

fn authorize_surface(
    surface: agentdash_application_agentrun::agent_run::AgentRunAppliedResourceSurface,
    capabilities: &BTreeSet<String>,
    request: RuntimeToolAuthorizationRequest,
) -> Result<RuntimeToolAuthorizationGrant, RuntimeToolBrokerError> {
    if surface.agent_surface_revision != request.context.applied_surface_revision.0 {
        return Err(denied(
            "stale_product_surface",
            "Product authority does not attest the callback applied surface revision",
        ));
    }
    if !capabilities.contains(&request.definition.provenance.capability_key) {
        return Err(denied(
            "missing_runtime_tool_capability",
            format!(
                "execution authority does not grant capability `{}`",
                request.definition.provenance.capability_key
            ),
        ));
    }
    let resources = match request.definition.authorization_policy {
        RuntimeToolAuthorizationPolicy::Product => RuntimeToolResourceGrant::Product,
        RuntimeToolAuthorizationPolicy::VfsMountCatalog => {
            vfs_grant(&surface, AppliedVfsOperation::List, &[], true)?
        }
        RuntimeToolAuthorizationPolicy::VfsRead => vfs_grant(
            &surface,
            AppliedVfsOperation::Read,
            &[path_argument(&surface, &request.arguments, "path", false)?],
            false,
        )?,
        RuntimeToolAuthorizationPolicy::VfsGlob => vfs_grant(
            &surface,
            AppliedVfsOperation::List,
            &[path_argument(&surface, &request.arguments, "path", true)?],
            false,
        )?,
        RuntimeToolAuthorizationPolicy::VfsGrep => vfs_grant(
            &surface,
            AppliedVfsOperation::Search,
            &[path_argument(&surface, &request.arguments, "path", true)?],
            false,
        )?,
        RuntimeToolAuthorizationPolicy::VfsApplyPatch => vfs_grant(
            &surface,
            AppliedVfsOperation::Write,
            &patch_paths(&request.arguments)?,
            false,
        )?,
        RuntimeToolAuthorizationPolicy::VfsShell => shell_vfs_grant(&surface, &request.arguments)?,
        RuntimeToolAuthorizationPolicy::TaskRead => {
            task_grant(&surface, AppliedTaskOperation::Read, &request.arguments)?
        }
        RuntimeToolAuthorizationPolicy::TaskWrite => {
            task_grant(&surface, AppliedTaskOperation::Write, &request.arguments)?
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
            agent_surface_revision: surface.agent_surface_revision,
            agent_surface_digest: surface.agent_surface_digest.clone(),
            vfs_digest: surface.vfs_digest.clone(),
            vfs_provenance: map_provenance(&surface.provenance),
            task_digest: surface.task_surface_digest.clone(),
            product_binding_digest: surface.product_binding_digest.clone(),
            host_binding_generation: request
                .context
                .host_binding_generation
                .map(|generation| generation.0),
        },
        resources,
    })
}

fn task_grant(
    surface: &agentdash_application_agentrun::agent_run::AgentRunAppliedResourceSurface,
    required: AppliedTaskOperation,
    arguments: &Value,
) -> Result<RuntimeToolResourceGrant, RuntimeToolBrokerError> {
    let scope = requested_task_scope(surface, required, arguments)?;
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
        scope: match grant.scope {
            AppliedTaskScope::Project { project_id } => RuntimeTaskExecutionScope::Project {
                project_id: project_id.to_string(),
            },
            AppliedTaskScope::Task {
                project_id,
                task_id,
            } => RuntimeTaskExecutionScope::Task {
                project_id: project_id.to_string(),
                task_id: task_id.to_string(),
            },
        },
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct RequestedVfsPath {
    mount_id: String,
    relative_path: String,
}

fn path_argument(
    surface: &agentdash_application_agentrun::agent_run::AgentRunAppliedResourceSurface,
    arguments: &Value,
    field: &str,
    optional_root: bool,
) -> Result<RequestedVfsPath, RuntimeToolBrokerError> {
    match arguments.get(field) {
        Some(Value::String(path)) => parse_requested_vfs_path(surface, path, optional_root),
        None if optional_root => {
            let default_mount = surface.default_mount_id.as_deref().ok_or_else(|| {
                denied(
                    "ambiguous_vfs_mount",
                    format!("{field} is required when no default VFS mount is applied"),
                )
            })?;
            Ok(RequestedVfsPath {
                mount_id: default_mount.to_owned(),
                relative_path: String::new(),
            })
        }
        _ => Err(denied(
            "invalid_vfs_tool_arguments",
            format!("{field} must be a canonical VFS path string"),
        )),
    }
}

fn parse_requested_vfs_path(
    surface: &agentdash_application_agentrun::agent_run::AgentRunAppliedResourceSurface,
    raw: &str,
    allow_root: bool,
) -> Result<RequestedVfsPath, RuntimeToolBrokerError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(denied("invalid_vfs_path", "VFS path must not be empty"));
    }
    let (mount_id, path) = if let Some((mount, path)) = raw.split_once("://") {
        (mount, path)
    } else {
        (
            surface.default_mount_id.as_deref().ok_or_else(|| {
                denied(
                    "ambiguous_vfs_mount",
                    "unqualified VFS path requires an applied default mount",
                )
            })?,
            raw,
        )
    };
    if mount_id.is_empty()
        || mount_id.contains(['/', '\\', '\0'])
        || !surface
            .vfs_mounts
            .iter()
            .any(|mount| mount.mount_id == mount_id)
    {
        return Err(denied(
            "invalid_vfs_mount",
            "VFS path references an absent or invalid applied mount",
        ));
    }
    let relative_path = if path == "." && allow_root {
        String::new()
    } else {
        ensure_canonical_relative_path(path, allow_root)?;
        path.to_owned()
    };
    Ok(RequestedVfsPath {
        mount_id: mount_id.to_owned(),
        relative_path,
    })
}

fn ensure_canonical_relative_path(
    path: &str,
    allow_root: bool,
) -> Result<(), RuntimeToolBrokerError> {
    if (path.is_empty() && !allow_root)
        || path.starts_with('/')
        || path.contains(['\\', '\0'])
        || (!path.is_empty()
            && path
                .split('/')
                .any(|segment| segment.is_empty() || segment == "." || segment == ".."))
    {
        return Err(denied(
            "invalid_vfs_path",
            "VFS path must be canonical, relative, and free of traversal segments",
        ));
    }
    Ok(())
}

fn patch_paths(arguments: &Value) -> Result<Vec<RequestedVfsPath>, RuntimeToolBrokerError> {
    let patch = arguments
        .get("patch")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            denied(
                "invalid_vfs_tool_arguments",
                "fs_apply_patch.patch must be a string",
            )
        })?;
    let entries = agentdash_application_vfs::parse_patch_text(patch)
        .map_err(|error| denied("invalid_vfs_tool_arguments", error.to_string()))?;
    let mut paths = Vec::new();
    for entry in entries {
        let primary = entry.path().to_string_lossy();
        paths.push(parse_explicit_patch_path(&primary)?);
        if let agentdash_application_vfs::PatchEntry::UpdateFile {
            move_path: Some(path),
            ..
        } = entry
        {
            paths.push(parse_explicit_patch_path(&path.to_string_lossy())?);
        }
    }
    if paths.is_empty() {
        return Err(denied(
            "invalid_vfs_tool_arguments",
            "fs_apply_patch must contain at least one file entry",
        ));
    }
    Ok(paths)
}

fn parse_explicit_patch_path(raw: &str) -> Result<RequestedVfsPath, RuntimeToolBrokerError> {
    let (mount_id, relative_path) = raw.split_once("://").ok_or_else(|| {
        denied(
            "invalid_vfs_path",
            "patch paths must use mount_id://relative/path",
        )
    })?;
    if mount_id.is_empty() || mount_id.contains(['/', '\\', '\0']) {
        return Err(denied("invalid_vfs_mount", "patch mount id is invalid"));
    }
    ensure_canonical_relative_path(relative_path, false)?;
    Ok(RequestedVfsPath {
        mount_id: mount_id.to_owned(),
        relative_path: relative_path.to_owned(),
    })
}

fn vfs_grant(
    surface: &agentdash_application_agentrun::agent_run::AgentRunAppliedResourceSurface,
    required: AppliedVfsOperation,
    requested_paths: &[RequestedVfsPath],
    list_all: bool,
) -> Result<RuntimeToolResourceGrant, RuntimeToolBrokerError> {
    let mut mounts = Vec::new();
    for mount in &surface.vfs_mounts {
        let matching_grants = surface
            .vfs_grants
            .iter()
            .filter(|grant| {
                grant.mount_id == mount.mount_id && grant.operations.contains(&required)
            })
            .collect::<Vec<_>>();
        if matching_grants.is_empty() {
            continue;
        }
        let paths = requested_paths
            .iter()
            .filter(|path| path.mount_id == mount.mount_id)
            .collect::<Vec<_>>();
        if !list_all
            && (paths.is_empty()
                || paths.iter().any(|path| {
                    !matching_grants.iter().any(|grant| {
                        grant
                            .path_scopes
                            .iter()
                            .any(|scope| scope_allows(scope, &path.relative_path))
                    })
                }))
        {
            continue;
        }
        let described_grants = if list_all {
            surface
                .vfs_grants
                .iter()
                .filter(|grant| grant.mount_id == mount.mount_id)
                .collect::<Vec<_>>()
        } else {
            matching_grants.clone()
        };
        let mut path_scopes = Vec::new();
        for scope in described_grants
            .iter()
            .flat_map(|grant| grant.path_scopes.iter())
            .cloned()
            .map(map_path_scope)
        {
            if !path_scopes.contains(&scope) {
                path_scopes.push(scope);
            }
        }
        mounts.push(RuntimeVfsMountGrant {
            id: mount.mount_id.clone(),
            provider: mount.provider.clone(),
            backend_id: mount.backend_id.clone(),
            root_ref: mount.root_ref.clone(),
            display_name: mount.display_name.clone(),
            metadata: mount.metadata.clone(),
            operations: if list_all {
                described_grants
                    .iter()
                    .flat_map(|grant| grant.operations.iter().copied())
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .map(map_vfs_operation)
                    .collect()
            } else {
                vec![map_vfs_operation(required)]
            },
            path_scopes,
        });
    }
    if mounts.is_empty()
        || (!list_all
            && requested_paths
                .iter()
                .any(|path| !mounts.iter().any(|mount| mount.id == path.mount_id)))
    {
        return Err(denied(
            "missing_vfs_grant",
            format!("applied Product surface does not grant {required:?} on every requested path"),
        ));
    }
    let default_mount_id = surface
        .default_mount_id
        .clone()
        .filter(|id| mounts.iter().any(|mount| &mount.id == id));
    Ok(RuntimeToolResourceGrant::Vfs(RuntimeVfsExecutionGrant {
        default_mount_id,
        mounts,
    }))
}

fn shell_vfs_grant(
    surface: &agentdash_application_agentrun::agent_run::AgentRunAppliedResourceSurface,
    arguments: &Value,
) -> Result<RuntimeToolResourceGrant, RuntimeToolBrokerError> {
    let operation = arguments
        .get("operation")
        .and_then(Value::as_str)
        .unwrap_or("start");
    if operation != "start" {
        return vfs_grant(surface, AppliedVfsOperation::Exec, &[], true);
    }
    match arguments.get("cwd").and_then(Value::as_str).map(str::trim) {
        Some(cwd) if !cwd.is_empty() && cwd != "platform://" => {
            let exec_grant = vfs_grant(
                surface,
                AppliedVfsOperation::Exec,
                &[parse_requested_vfs_path(surface, cwd, true)?],
                false,
            )?;
            let command = arguments
                .get("command")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    denied(
                        "invalid_vfs_tool_arguments",
                        "shell_exec.command must be a string",
                    )
                })?;
            let mount_ids = surface
                .vfs_mounts
                .iter()
                .map(|mount| mount.mount_id.clone())
                .collect::<Vec<_>>();
            let mut read_paths = Vec::new();
            for candidate in agentdash_application_vfs::rewrite::find_shell_mount_uri_candidates(
                command, &mount_ids,
            ) {
                let requested = parse_requested_vfs_path(surface, &candidate.value, true)?;
                if !read_paths.contains(&requested) {
                    read_paths.push(requested);
                }
            }
            let mut grants = vec![exec_grant];
            if !read_paths.is_empty() {
                grants.push(vfs_grant(
                    surface,
                    AppliedVfsOperation::Read,
                    &read_paths,
                    false,
                )?);
            }
            merge_vfs_grants(surface, grants)
        }
        _ => {
            let mut grants = Vec::new();
            for operation in [
                AppliedVfsOperation::Read,
                AppliedVfsOperation::List,
                AppliedVfsOperation::Write,
                AppliedVfsOperation::Exec,
            ] {
                if let Ok(grant) = vfs_grant(surface, operation, &[], true) {
                    grants.push(grant);
                }
            }
            merge_vfs_grants(surface, grants)
        }
    }
}

fn merge_vfs_grants(
    surface: &agentdash_application_agentrun::agent_run::AgentRunAppliedResourceSurface,
    grants: Vec<RuntimeToolResourceGrant>,
) -> Result<RuntimeToolResourceGrant, RuntimeToolBrokerError> {
    let mut mounts: Vec<RuntimeVfsMountGrant> = Vec::new();
    for grant in grants {
        let RuntimeToolResourceGrant::Vfs(grant) = grant else {
            return Err(denied(
                "invalid_vfs_grant",
                "shell authorization produced a non-VFS grant",
            ));
        };
        for mount in grant.mounts {
            if let Some(existing) = mounts.iter_mut().find(|existing| existing.id == mount.id) {
                for operation in mount.operations {
                    if !existing.operations.contains(&operation) {
                        existing.operations.push(operation);
                    }
                }
                for scope in mount.path_scopes {
                    if !existing.path_scopes.contains(&scope) {
                        existing.path_scopes.push(scope);
                    }
                }
            } else {
                mounts.push(mount);
            }
        }
    }
    if mounts.is_empty() {
        return Err(denied(
            "missing_vfs_grant",
            "shell has no applied VFS operations",
        ));
    }
    let default_mount_id = surface
        .default_mount_id
        .clone()
        .filter(|id| mounts.iter().any(|mount| &mount.id == id));
    Ok(RuntimeToolResourceGrant::Vfs(RuntimeVfsExecutionGrant {
        default_mount_id,
        mounts,
    }))
}

fn scope_allows(scope: &AppliedVfsPathScope, relative_path: &str) -> bool {
    match scope {
        AppliedVfsPathScope::All => true,
        AppliedVfsPathScope::Exact(path) => relative_path == path,
        AppliedVfsPathScope::Prefix(prefix) => {
            relative_path == prefix
                || relative_path
                    .strip_prefix(prefix)
                    .is_some_and(|suffix| suffix.starts_with('/'))
        }
    }
}

fn requested_task_scope(
    surface: &agentdash_application_agentrun::agent_run::AgentRunAppliedResourceSurface,
    required: AppliedTaskOperation,
    arguments: &Value,
) -> Result<AppliedTaskScope, RuntimeToolBrokerError> {
    if surface.task_grants.iter().any(|grant| {
        grant.scope
            == AppliedTaskScope::Project {
                project_id: surface.project_id,
            }
            && grant.operations.contains(&required)
    }) {
        return Ok(AppliedTaskScope::Project {
            project_id: surface.project_id,
        });
    }
    let mut ids = BTreeSet::new();
    collect_task_ids(arguments, &mut ids)?;
    let task_scopes = surface
        .task_grants
        .iter()
        .filter_map(|grant| match grant.scope {
            AppliedTaskScope::Task {
                project_id,
                task_id,
            } if grant.operations.contains(&required) => Some((project_id, task_id)),
            AppliedTaskScope::Project { .. } | AppliedTaskScope::Task { .. } => None,
        })
        .collect::<Vec<_>>();
    if task_scopes.is_empty() {
        return Err(denied(
            "missing_task_grant",
            format!("applied Product surface does not grant Task {required:?}"),
        ));
    }
    let task_id = match ids.len() {
        0 if task_scopes.len() == 1 => task_scopes[0].1,
        1 => *ids.first().expect("single task id"),
        _ => {
            return Err(denied(
                "task_scope_violation",
                "Task-scoped execution must resolve to exactly one granted Task",
            ));
        }
    };
    Ok(AppliedTaskScope::Task {
        project_id: surface.project_id,
        task_id,
    })
}

fn collect_task_ids(
    value: &Value,
    ids: &mut std::collections::BTreeSet<Uuid>,
) -> Result<(), RuntimeToolBrokerError> {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                if key == "task_id" {
                    let raw = value.as_str().ok_or_else(|| {
                        denied("invalid_task_arguments", "task_id must be a UUID string")
                    })?;
                    ids.insert(Uuid::parse_str(raw).map_err(|error| {
                        denied(
                            "invalid_task_arguments",
                            format!("invalid task_id: {error}"),
                        )
                    })?);
                } else if key == "task_ids" {
                    let values = value.as_array().ok_or_else(|| {
                        denied("invalid_task_arguments", "task_ids must be a UUID array")
                    })?;
                    for value in values {
                        let raw = value.as_str().ok_or_else(|| {
                            denied(
                                "invalid_task_arguments",
                                "task_ids must contain UUID strings",
                            )
                        })?;
                        ids.insert(Uuid::parse_str(raw).map_err(|error| {
                            denied(
                                "invalid_task_arguments",
                                format!("invalid task_id: {error}"),
                            )
                        })?);
                    }
                } else {
                    collect_task_ids(value, ids)?;
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                collect_task_ids(value, ids)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn map_provenance(
    provenance: &agentdash_application_agentrun::agent_run::AgentRunAppliedResourceSurfaceProvenance,
) -> RuntimeToolProvenanceEvidence {
    RuntimeToolProvenanceEvidence {
        source_kind: provenance.source_kind.clone(),
        source_id: provenance.source_id.clone(),
        source_revision: provenance.source_revision,
        projection_revision: provenance.projection_revision,
        captured_at_ms: provenance.captured_at_ms,
    }
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
        AgentBindingGeneration, AgentSurfaceRevision, AgentToolName,
    };
    use agentdash_application_agentrun::agent_run::{
        AgentRunAppliedResourceSurface, AgentRunAppliedResourceSurfaceProvenance,
        AgentRunAppliedResourceSurfaceQueryError, AgentRunProductRuntimeBinding, AppliedTaskGrant,
        AppliedVfsGrant, AppliedVfsMount,
    };
    use agentdash_domain::agent_run_target::AgentRunTarget;
    use uuid::Uuid;

    use super::*;

    #[derive(Clone)]
    struct CommittedRuntimeToolProductBinding {
        binding: AgentRunProductRuntimeBinding,
        binding_digest: String,
    }

    struct BindingFixture {
        value: Option<CommittedRuntimeToolProductBinding>,
    }

    struct SurfaceFixture {
        value: Result<AgentRunAppliedResourceSurface, AgentRunAppliedResourceSurfaceQueryError>,
    }

    #[derive(Clone)]
    struct AuthorityFixture {
        value: Result<RuntimeToolExecutionAuthority, String>,
    }

    #[async_trait]
    impl RuntimeToolExecutionAuthorityPort for AuthorityFixture {
        async fn execution_authority(
            &self,
            _runtime_thread_id: &RuntimeThreadId,
        ) -> Result<RuntimeToolExecutionAuthority, String> {
            self.value.clone()
        }
    }

    fn test_authorizer(
        binding: Arc<BindingFixture>,
        surface: Arc<SurfaceFixture>,
    ) -> ProductRuntimeToolAuthorizer {
        let value = binding
            .value
            .clone()
            .ok_or_else(|| "runtime thread has no ExecutionAuthority binding".to_string())
            .and_then(|binding| {
                surface
                    .value
                    .clone()
                    .map_err(|error| error.to_string())
                    .and_then(|surface| {
                        if surface.product_binding_digest != binding.binding_digest {
                            return Err(
                                "ExecutionAuthority evidence does not match binding".to_string()
                            );
                        }
                        Ok(RuntimeToolExecutionAuthority {
                            runtime_thread_id: binding.binding.runtime_thread_id,
                            capabilities: all_test_capabilities(),
                            surface,
                        })
                    })
            });
        ProductRuntimeToolAuthorizer::new(Arc::new(AuthorityFixture { value }))
    }

    fn all_test_capabilities() -> BTreeSet<String> {
        BTreeSet::from([
            "collaboration".to_string(),
            "file_read".to_string(),
            "file_write".to_string(),
            "mcp:agentdash-workflow-tools".to_string(),
            "shell_execute".to_string(),
            "task".to_string(),
            "workflow".to_string(),
            "workspace_module".to_string(),
        ])
    }

    #[tokio::test]
    async fn missing_applied_surface_is_typed_deny() {
        let binding = binding();
        let authorizer = test_authorizer(
            Arc::new(BindingFixture {
                value: Some(binding),
            }),
            Arc::new(SurfaceFixture {
                value: Err(AgentRunAppliedResourceSurfaceQueryError::MissingFacts),
            }),
        );
        let error = authorizer
            .authorize(request("mounts_list"))
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            RuntimeToolBrokerError::AuthorizationDenied { code, .. }
                if code == "execution_authority_unavailable"
        ));
    }

    #[tokio::test]
    async fn product_and_host_surface_digests_are_distinct_evidence_namespaces() {
        let binding = binding();
        let snapshot = snapshot(
            binding.binding.target.clone(),
            binding.binding_digest.clone(),
        );
        let authorizer = test_authorizer(
            Arc::new(BindingFixture {
                value: Some(binding),
            }),
            Arc::new(SurfaceFixture {
                value: Ok(snapshot),
            }),
        );
        let grant = authorizer
            .authorize(request("mounts_list"))
            .await
            .expect("distinct Product and Host surface digests");
        assert_eq!(
            grant.applied_surface.agent_surface_digest,
            "product-surface-test"
        );
    }

    #[tokio::test]
    async fn task_read_grant_does_not_authorize_task_write() {
        let binding = binding();
        let mut snapshot = snapshot(
            binding.binding.target.clone(),
            binding.binding_digest.clone(),
        );
        snapshot.task_grants[0].operations = BTreeSet::from([AppliedTaskOperation::Read]);
        let authorizer = test_authorizer(
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
    async fn project_agent_run_task_surface_authorizes_run_scoped_task_write() {
        let binding = binding();
        let snapshot = snapshot(
            binding.binding.target.clone(),
            binding.binding_digest.clone(),
        );
        let authorizer = test_authorizer(
            Arc::new(BindingFixture {
                value: Some(binding),
            }),
            Arc::new(SurfaceFixture {
                value: Ok(snapshot),
            }),
        );

        authorizer
            .authorize(request("task_write"))
            .await
            .expect("Project AgentRun owns the run-scoped Task write boundary");
    }

    #[tokio::test]
    async fn current_product_authority_is_observed_without_snapshot_pin() {
        let binding = binding();
        let mut newer = snapshot(
            binding.binding.target.clone(),
            binding.binding_digest.clone(),
        );
        newer.vfs_grants[0]
            .operations
            .insert(AppliedVfsOperation::Write);
        let authorizer = test_authorizer(
            Arc::new(BindingFixture {
                value: Some(binding),
            }),
            Arc::new(SurfaceFixture { value: Ok(newer) }),
        );
        authorizer
            .authorize(request("mounts_list"))
            .await
            .expect("current Product authority is evaluated directly");
    }

    #[tokio::test]
    async fn active_turn_remains_authorized_by_its_immutable_surface_after_rebind() {
        let mut binding = binding();
        binding.binding.launch_frame.revision = 2;
        let snapshot = snapshot(
            binding.binding.target.clone(),
            binding.binding_digest.clone(),
        );
        let authorizer = test_authorizer(
            Arc::new(BindingFixture {
                value: Some(binding),
            }),
            Arc::new(SurfaceFixture {
                value: Ok(snapshot),
            }),
        );

        authorizer
            .authorize(request("workspace_module_list"))
            .await
            .expect("the callback surface remains the grant authority for its active turn");
    }

    #[tokio::test]
    async fn vfs_catalog_maps_each_tool_to_its_exact_operation() {
        let cases = [
            (
                "mounts_list",
                serde_json::json!({}),
                RuntimeVfsGrantedOperation::List,
            ),
            (
                "fs_read",
                serde_json::json!({"path": "main://docs/readme.md"}),
                RuntimeVfsGrantedOperation::Read,
            ),
            (
                "fs_glob",
                serde_json::json!({"pattern": "**/*.rs", "path": "main://docs"}),
                RuntimeVfsGrantedOperation::List,
            ),
            (
                "fs_grep",
                serde_json::json!({"pattern": "runtime", "path": "main://docs"}),
                RuntimeVfsGrantedOperation::Search,
            ),
            (
                "fs_apply_patch",
                serde_json::json!({"patch": "*** Begin Patch\n*** Update File: main://docs/readme.md\n@@\n-old\n+new\n*** End Patch\n"}),
                RuntimeVfsGrantedOperation::Write,
            ),
            (
                "shell_exec",
                serde_json::json!({"cwd": "main://docs", "command": "pwd"}),
                RuntimeVfsGrantedOperation::Execute,
            ),
        ];
        for (tool, arguments, operation) in cases {
            let binding = binding();
            let snapshot = snapshot(
                binding.binding.target.clone(),
                binding.binding_digest.clone(),
            );
            let authorizer = test_authorizer(
                Arc::new(BindingFixture {
                    value: Some(binding),
                }),
                Arc::new(SurfaceFixture {
                    value: Ok(snapshot),
                }),
            );
            let grant = authorizer
                .authorize(request_with_arguments(tool, arguments))
                .await
                .unwrap_or_else(|error| panic!("{tool} authorization failed: {error}"));
            let RuntimeToolResourceGrant::Vfs(vfs) = grant.resources else {
                panic!("{tool} must produce a VFS grant");
            };
            assert!(
                vfs.mounts
                    .iter()
                    .any(|mount| mount.operations.contains(&operation)),
                "{tool} must carry {operation:?}"
            );
            assert!(
                vfs.mounts
                    .iter()
                    .all(|mount| mount.metadata.get("run_id").is_some()
                        && mount.metadata.get("agent_id").is_some()),
                "{tool} must preserve typed mount identity metadata"
            );
            assert_eq!(grant.applied_surface.vfs_digest, "vfs-test");
            assert_eq!(grant.applied_surface.task_digest, "task-test");
            assert_eq!(grant.applied_surface.product_binding_digest, "binding-test");
            assert_eq!(grant.applied_surface.host_binding_generation, Some(1));
        }
    }

    #[tokio::test]
    async fn shell_mount_exec_grants_read_for_command_vfs_references() {
        let binding = binding();
        let mut snapshot = snapshot(
            binding.binding.target.clone(),
            binding.binding_digest.clone(),
        );
        add_read_only_mount(&mut snapshot, "lifecycle", "lifecycle_vfs");
        let authorizer = test_authorizer(
            Arc::new(BindingFixture {
                value: Some(binding),
            }),
            Arc::new(SurfaceFixture {
                value: Ok(snapshot),
            }),
        );

        let grant = authorizer
            .authorize(request_with_arguments(
                "shell_exec",
                serde_json::json!({
                    "cwd": "main://docs",
                    "command": "python lifecycle://skills/demo/scripts/run.py main://docs/input.txt"
                }),
            ))
            .await
            .expect("shell command VFS references should be authorized");

        let RuntimeToolResourceGrant::Vfs(vfs) = grant.resources else {
            panic!("shell_exec must produce a VFS grant");
        };
        let main = vfs
            .mounts
            .iter()
            .find(|mount| mount.id == "main")
            .expect("main grant");
        assert!(
            main.operations
                .contains(&RuntimeVfsGrantedOperation::Execute)
        );
        assert!(main.operations.contains(&RuntimeVfsGrantedOperation::Read));
        let lifecycle = vfs
            .mounts
            .iter()
            .find(|mount| mount.id == "lifecycle")
            .expect("lifecycle grant");
        assert_eq!(lifecycle.operations, vec![RuntimeVfsGrantedOperation::Read]);
    }

    #[tokio::test]
    async fn shell_mount_exec_does_not_grant_read_for_here_string_data() {
        let binding = binding();
        let mut snapshot = snapshot(
            binding.binding.target.clone(),
            binding.binding_digest.clone(),
        );
        add_read_only_mount(&mut snapshot, "lifecycle", "lifecycle_vfs");
        let authorizer = test_authorizer(
            Arc::new(BindingFixture {
                value: Some(binding),
            }),
            Arc::new(SurfaceFixture {
                value: Ok(snapshot),
            }),
        );

        let grant = authorizer
            .authorize(request_with_arguments(
                "shell_exec",
                serde_json::json!({
                    "cwd": "main://",
                    "command": "$source = @'\nlifecycle://skills/demo\n'@\npython update.py"
                }),
            ))
            .await
            .expect("here-string data must not request a VFS read grant");

        let RuntimeToolResourceGrant::Vfs(vfs) = grant.resources else {
            panic!("shell_exec must produce a VFS grant");
        };
        assert_eq!(vfs.mounts.len(), 1);
        assert_eq!(vfs.mounts[0].id, "main");
        assert_eq!(
            vfs.mounts[0].operations,
            vec![RuntimeVfsGrantedOperation::Execute]
        );
    }

    #[tokio::test]
    async fn mounts_list_reports_the_full_applied_mount_capability_surface() {
        let binding = binding();
        let snapshot = snapshot(
            binding.binding.target.clone(),
            binding.binding_digest.clone(),
        );
        let authorizer = test_authorizer(
            Arc::new(BindingFixture {
                value: Some(binding),
            }),
            Arc::new(SurfaceFixture {
                value: Ok(snapshot),
            }),
        );

        let grant = authorizer
            .authorize(request("mounts_list"))
            .await
            .expect("mount catalog is readable");
        let RuntimeToolResourceGrant::Vfs(vfs) = grant.resources else {
            panic!("mounts_list must produce a VFS catalog grant");
        };
        let main = vfs
            .mounts
            .iter()
            .find(|mount| mount.id == "main")
            .expect("main mount is visible");
        assert_eq!(main.operations.len(), 5);
        for expected in [
            RuntimeVfsGrantedOperation::Read,
            RuntimeVfsGrantedOperation::Write,
            RuntimeVfsGrantedOperation::List,
            RuntimeVfsGrantedOperation::Search,
            RuntimeVfsGrantedOperation::Execute,
        ] {
            assert!(main.operations.contains(&expected), "missing {expected:?}");
        }
    }

    #[tokio::test]
    async fn non_canonical_and_prefix_expanding_paths_are_denied_before_execution() {
        for path in [
            "../secret",
            "main://docs/../secret",
            "main://docs//readme.md",
            "main://docs\\readme.md",
            "/absolute/path",
        ] {
            let binding = binding();
            let snapshot = snapshot(
                binding.binding.target.clone(),
                binding.binding_digest.clone(),
            );
            let authorizer = test_authorizer(
                Arc::new(BindingFixture {
                    value: Some(binding),
                }),
                Arc::new(SurfaceFixture {
                    value: Ok(snapshot),
                }),
            );
            assert!(
                authorizer
                    .authorize(request_with_arguments(
                        "fs_read",
                        serde_json::json!({"path": path}),
                    ))
                    .await
                    .is_err(),
                "{path} must be denied"
            );
        }

        let binding = binding();
        let mut snapshot = snapshot(
            binding.binding.target.clone(),
            binding.binding_digest.clone(),
        );
        snapshot.vfs_grants[0].path_scopes = vec![AppliedVfsPathScope::Prefix("docs".to_owned())];
        let authorizer = test_authorizer(
            Arc::new(BindingFixture {
                value: Some(binding),
            }),
            Arc::new(SurfaceFixture {
                value: Ok(snapshot),
            }),
        );
        let error = authorizer
            .authorize(request_with_arguments(
                "fs_read",
                serde_json::json!({"path": "main://docs2/readme.md"}),
            ))
            .await
            .expect_err("segment prefix expansion must be denied");
        assert!(matches!(
            error,
            RuntimeToolBrokerError::AuthorizationDenied { code, .. }
                if code == "missing_vfs_grant"
        ));
    }

    #[tokio::test]
    async fn operation_specific_grants_on_one_mount_are_authorized_independently() {
        let binding = binding();
        let mut snapshot = snapshot(
            binding.binding.target.clone(),
            binding.binding_digest.clone(),
        );
        snapshot.vfs_grants = vec![
            AppliedVfsGrant {
                mount_id: "main".to_owned(),
                operations: BTreeSet::from([AppliedVfsOperation::Read, AppliedVfsOperation::List]),
                path_scopes: vec![AppliedVfsPathScope::All],
            },
            AppliedVfsGrant {
                mount_id: "main".to_owned(),
                operations: BTreeSet::from([AppliedVfsOperation::Write]),
                path_scopes: vec![AppliedVfsPathScope::Prefix("node/artifacts".to_owned())],
            },
        ];
        let authorizer = test_authorizer(
            Arc::new(BindingFixture {
                value: Some(binding),
            }),
            Arc::new(SurfaceFixture {
                value: Ok(snapshot),
            }),
        );

        authorizer
            .authorize(request_with_arguments(
                "fs_read",
                serde_json::json!({"path": "main://session/events.json"}),
            ))
            .await
            .expect("canonical history remains readable");
        authorizer
            .authorize(request_with_arguments(
                "fs_apply_patch",
                serde_json::json!({
                    "patch": "*** Begin Patch\n*** Add File: main://node/artifacts/result\n+ok\n*** End Patch\n"
                }),
            ))
            .await
            .expect("node artifact write is granted");
        let error = authorizer
            .authorize(request_with_arguments(
                "fs_apply_patch",
                serde_json::json!({
                    "patch": "*** Begin Patch\n*** Add File: main://session/records/forbidden\n+no\n*** End Patch\n"
                }),
            ))
            .await
            .expect_err("write outside the node scope must remain denied");
        assert!(matches!(
            error,
            RuntimeToolBrokerError::AuthorizationDenied { code, .. }
                if code == "missing_vfs_grant"
        ));
    }

    #[tokio::test]
    async fn task_scope_selects_one_task_and_rejects_siblings() {
        let binding = binding();
        let mut snapshot = snapshot(
            binding.binding.target.clone(),
            binding.binding_digest.clone(),
        );
        let project_id = snapshot.project_id;
        let task_id = Uuid::new_v4();
        snapshot.task_grants = vec![AppliedTaskGrant {
            scope: AppliedTaskScope::Task {
                project_id,
                task_id,
            },
            operations: BTreeSet::from([AppliedTaskOperation::Read, AppliedTaskOperation::Write]),
        }];
        let authorizer = test_authorizer(
            Arc::new(BindingFixture {
                value: Some(binding),
            }),
            Arc::new(SurfaceFixture {
                value: Ok(snapshot),
            }),
        );
        let grant = authorizer
            .authorize(request_with_arguments(
                "task_read",
                serde_json::json!({"task_id": task_id}),
            ))
            .await
            .expect("granted Task must authorize");
        assert!(matches!(
            grant.resources,
            RuntimeToolResourceGrant::Task(RuntimeTaskExecutionGrant {
                scope: RuntimeTaskExecutionScope::Task { task_id: ref value, .. },
                ..
            }) if value == &task_id.to_string()
        ));
        let sibling = Uuid::new_v4();
        let error = authorizer
            .authorize(request_with_arguments(
                "task_write",
                serde_json::json!({
                    "operations": [{"op": "set_status", "task_id": sibling, "status": "done"}]
                }),
            ))
            .await
            .expect_err("sibling Task must be denied");
        assert!(matches!(
            error,
            RuntimeToolBrokerError::AuthorizationDenied { code, .. }
                if code == "missing_task_grant"
        ));
    }

    #[tokio::test]
    async fn workspace_presentation_receives_only_the_committed_product_target() {
        let binding = binding();
        let authorizer = test_authorizer(
            Arc::new(BindingFixture {
                value: Some(binding.clone()),
            }),
            Arc::new(SurfaceFixture {
                value: Ok(snapshot(
                    binding.binding.target.clone(),
                    binding.binding_digest,
                )),
            }),
        );

        let grant = authorizer
            .authorize(request("workspace_module_present"))
            .await
            .expect("committed Product target must authorize presentation");

        assert_eq!(grant.resources, RuntimeToolResourceGrant::Product);
        assert_eq!(
            grant.target.run_id,
            binding.binding.target.run_id.to_string()
        );
        assert_eq!(
            grant.target.agent_id,
            binding.binding.target.agent_id.to_string()
        );
    }

    #[tokio::test]
    async fn every_product_runtime_tool_uses_the_product_resource_policy() {
        let binding = binding();
        let authorizer = test_authorizer(
            Arc::new(BindingFixture {
                value: Some(binding.clone()),
            }),
            Arc::new(SurfaceFixture {
                value: Ok(snapshot(
                    binding.binding.target.clone(),
                    binding.binding_digest,
                )),
            }),
        );

        for tool in [
            "workspace_module_list",
            "workspace_module_describe",
            "workspace_module_operate",
            "workspace_module_invoke",
            "workspace_module_present",
            "operation_script",
        ] {
            let grant = authorizer
                .authorize(request(tool))
                .await
                .unwrap_or_else(|error| panic!("{tool} must use Product authorization: {error}"));
            assert_eq!(grant.resources, RuntimeToolResourceGrant::Product);
        }
    }

    #[tokio::test]
    async fn product_policy_still_requires_the_definition_capability() {
        let binding = binding();
        let authorizer = ProductRuntimeToolAuthorizer::new(Arc::new(AuthorityFixture {
            value: Ok(RuntimeToolExecutionAuthority {
                runtime_thread_id: binding.binding.runtime_thread_id.clone(),
                capabilities: BTreeSet::new(),
                surface: snapshot(binding.binding.target, binding.binding_digest),
            }),
        }));

        let error = authorizer
            .authorize(request("operation_script"))
            .await
            .expect_err("Product resource policy must not bypass capability admission");

        assert!(matches!(
            error,
            RuntimeToolBrokerError::AuthorizationDenied { code, .. }
                if code == "missing_runtime_tool_capability"
        ));
    }

    #[tokio::test]
    async fn dynamic_mcp_tool_uses_the_same_committed_product_authority() {
        let binding = binding();
        let authorizer = test_authorizer(
            Arc::new(BindingFixture {
                value: Some(binding.clone()),
            }),
            Arc::new(SurfaceFixture {
                value: Ok(snapshot(
                    binding.binding.target.clone(),
                    binding.binding_digest,
                )),
            }),
        );

        let grant = authorizer
            .authorize(request("mcp_agentdash_workflow_tools_get_lifecycle"))
            .await
            .expect("bound MCP executor must use Product target authority");

        assert_eq!(grant.resources, RuntimeToolResourceGrant::Product);
        assert_eq!(
            grant.target.run_id,
            binding.binding.target.run_id.to_string()
        );
        assert_eq!(
            grant.target.agent_id,
            binding.binding.target.agent_id.to_string()
        );
    }

    fn binding() -> CommittedRuntimeToolProductBinding {
        let target = AgentRunTarget {
            run_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
        };
        let mut execution_profile =
            agentdash_application_agentrun::agent_run::ProductExecutionProfileRef {
                profile_key: "codex".to_owned(),
                profile_revision: 1,
                profile_digest: String::new(),
                configuration: serde_json::json!({"executor": "codex"}),
                credential_scope: None,
            };
        execution_profile.refresh_digest();
        CommittedRuntimeToolProductBinding {
            binding: AgentRunProductRuntimeBinding {
                agent:
                    agentdash_application_agentrun::agent_run::AgentRunCompleteAgentAssociation {
                        service_instance_id:
                            agentdash_agent_runtime_contract::AgentServiceInstanceId::new(
                                "fixture-agent",
                            )
                            .unwrap(),
                        source: agentdash_agent_runtime_contract::AgentSourceCoordinate::new(
                            "fixture-source",
                        )
                        .unwrap(),
                    },
                launch_frame: agentdash_application_agentrun::agent_run::ProductAgentFrameRef {
                    frame_id: Uuid::new_v4(),
                    agent_id: target.agent_id,
                    revision: 1,
                },
                execution_profile_digest: execution_profile.profile_digest.clone(),
                execution_profile,
                target,
                runtime_thread_id: RuntimeThreadId::new("thread-test").unwrap(),
            },
            binding_digest: "binding-test".to_owned(),
        }
    }

    fn snapshot(
        target: AgentRunTarget,
        product_binding_digest: String,
    ) -> AgentRunAppliedResourceSurface {
        let project_id = Uuid::new_v4();
        let provenance = AgentRunAppliedResourceSurfaceProvenance {
            source_kind: "product".to_owned(),
            source_id: "surface-test".to_owned(),
            source_revision: 1,
            projection_revision: 1,
            captured_at_ms: 1,
        };
        let mount_metadata = serde_json::json!({
            "run_id": target.run_id,
            "agent_id": target.agent_id,
        });
        AgentRunAppliedResourceSurface {
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
                    AppliedVfsOperation::Search,
                    AppliedVfsOperation::Write,
                    AppliedVfsOperation::Exec,
                ]),
                default_write: false,
                display_name: "Main".to_owned(),
                metadata: mount_metadata,
            }],
            default_mount_id: Some("main".to_owned()),
            vfs_grants: vec![AppliedVfsGrant {
                mount_id: "main".to_owned(),
                operations: BTreeSet::from([
                    AppliedVfsOperation::Read,
                    AppliedVfsOperation::List,
                    AppliedVfsOperation::Search,
                    AppliedVfsOperation::Write,
                    AppliedVfsOperation::Exec,
                ]),
                path_scopes: vec![AppliedVfsPathScope::All],
            }],
            agent_surface_revision: 1,
            agent_surface_digest: "product-surface-test".to_owned(),
            vfs_digest: "vfs-test".to_owned(),
            task_grants: vec![AppliedTaskGrant {
                scope: AppliedTaskScope::Project { project_id },
                operations: BTreeSet::from([
                    AppliedTaskOperation::Read,
                    AppliedTaskOperation::Write,
                ]),
            }],
            task_surface_digest: "task-test".to_owned(),
            product_binding_digest,
            provenance,
        }
    }

    fn add_read_only_mount(
        surface: &mut AgentRunAppliedResourceSurface,
        mount_id: &str,
        provider: &str,
    ) {
        let mut mount = surface.vfs_mounts[0].clone();
        mount.mount_id = mount_id.to_owned();
        mount.provider = provider.to_owned();
        mount.root_ref = format!("{mount_id}://root");
        mount.capabilities = BTreeSet::from([AppliedVfsOperation::Read]);
        surface.vfs_mounts.push(mount);
        surface.vfs_grants.push(AppliedVfsGrant {
            mount_id: mount_id.to_owned(),
            operations: BTreeSet::from([AppliedVfsOperation::Read]),
            path_scopes: vec![AppliedVfsPathScope::All],
        });
    }

    fn request(tool: &str) -> RuntimeToolAuthorizationRequest {
        request_with_arguments(tool, serde_json::json!({}))
    }

    fn request_with_arguments(
        tool: &str,
        arguments: serde_json::Value,
    ) -> RuntimeToolAuthorizationRequest {
        let (permission, effect) = match tool {
            "task_write"
            | "workspace_module_operate"
            | "workspace_module_invoke"
            | "workspace_module_present"
            | "operation_script" => (
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
                host_binding_generation: Some(AgentBindingGeneration(1)),
                applied_surface_revision: AgentSurfaceRevision(1),
                turn_id: agentdash_agent_runtime_contract::AgentTurnId::new("turn-test").unwrap(),
                item_id: Some(
                    agentdash_agent_runtime_contract::AgentItemId::new("item-test").unwrap(),
                ),
                effect_id: agentdash_agent_runtime_contract::AgentEffectIdentity::new(
                    "effect-test",
                )
                .unwrap(),
                invocation_id: "callback-test".to_owned(),
                deadline_at_ms: u64::MAX,
            },
            definition: RuntimeToolDefinition {
                name: AgentToolName::new(tool).unwrap(),
                description: tool.to_owned(),
                parameters_schema: serde_json::json!({}),
                provenance: agentdash_agent_runtime::RuntimeToolProvenance {
                    capability_key: match tool {
                        "mounts_list" | "fs_read" | "fs_glob" | "fs_grep" => "file_read",
                        "fs_apply_patch" => "file_write",
                        "shell_exec" => "shell_execute",
                        "task_read" | "task_write" => "task",
                        "workspace_module_list"
                        | "workspace_module_describe"
                        | "workspace_module_operate"
                        | "workspace_module_invoke"
                        | "workspace_module_present"
                        | "operation_script" => "workspace_module",
                        "complete_lifecycle_node" => "workflow",
                        "companion_request" | "companion_respond" | "wait" => "collaboration",
                        tool if tool.starts_with("mcp_") => "mcp:agentdash-workflow-tools",
                        _ => "test",
                    }
                    .to_owned(),
                    source: "test".to_owned(),
                    tool_path: format!("test::{tool}"),
                    context_usage_kind: "system_tools".to_owned(),
                },
                protocol_projector: agentdash_agent_protocol::ToolProtocolProjector::Dynamic,
                permission,
                effect,
                authorization_policy: match tool {
                    "mounts_list" => RuntimeToolAuthorizationPolicy::VfsMountCatalog,
                    "fs_read" => RuntimeToolAuthorizationPolicy::VfsRead,
                    "fs_glob" => RuntimeToolAuthorizationPolicy::VfsGlob,
                    "fs_grep" => RuntimeToolAuthorizationPolicy::VfsGrep,
                    "fs_apply_patch" => RuntimeToolAuthorizationPolicy::VfsApplyPatch,
                    "shell_exec" => RuntimeToolAuthorizationPolicy::VfsShell,
                    "task_read" => RuntimeToolAuthorizationPolicy::TaskRead,
                    "task_write" => RuntimeToolAuthorizationPolicy::TaskWrite,
                    _ => RuntimeToolAuthorizationPolicy::Product,
                },
            },
            arguments,
        }
    }
}
