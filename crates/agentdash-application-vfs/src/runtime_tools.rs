use std::{collections::BTreeSet, sync::Arc};

use agentdash_platform_spi::{
    CapabilityState, Mount, MountCapability, RuntimeVfsAccessPolicy, RuntimeVfsAccessRule,
    RuntimeVfsAccessSource, RuntimeVfsOperation, RuntimeVfsPathPattern, ToolCluster, Vfs,
};
use serde_json::Value;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    VfsMaterializationService, VfsService,
    inline_persistence::InlineContentOverlay,
    runtime_tool_execution::{VfsToolExecutionError, VfsToolUpdateSink},
    tools::{
        FsApplyPatchExecutionState, FsApplyPatchExecutor, FsGlobExecutor, FsGrepExecutor,
        FsReadExecutionState, FsReadExecutor, MountsListExecutor, SharedRuntimeVfs,
        ShellExecExecutor, ShellTerminalOwner, ShellTerminalRegistry,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppliedVfsToolKind {
    MountsList,
    Read,
    Glob,
    Grep,
    ApplyPatch,
    ShellExec,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AppliedVfsToolOperation {
    Read,
    List,
    Search,
    Write,
    Execute,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppliedVfsToolPathScope {
    All,
    Exact(String),
    Prefix(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct AppliedVfsToolMount {
    pub id: String,
    pub provider: String,
    pub backend_id: String,
    pub root_ref: String,
    pub display_name: String,
    pub metadata: Value,
    pub operations: BTreeSet<AppliedVfsToolOperation>,
    pub path_scopes: Vec<AppliedVfsToolPathScope>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AppliedVfsToolSurface {
    pub default_mount_id: Option<String>,
    pub mounts: Vec<AppliedVfsToolMount>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedVfsToolOwner {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub runtime_thread_id: String,
    pub invocation_id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AppliedVfsToolRequest {
    pub kind: AppliedVfsToolKind,
    pub arguments: Value,
    pub surface: AppliedVfsToolSurface,
    pub owner: AppliedVfsToolOwner,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AppliedVfsToolOutcome {
    Completed { output: Value },
    Rejected { code: String, message: String },
    Failed { code: String, message: String },
}

#[derive(Clone)]
pub struct AppliedVfsRuntimeToolService {
    service: Arc<VfsService>,
    terminal_registry: Arc<dyn ShellTerminalRegistry>,
    materialization: Option<Arc<VfsMaterializationService>>,
    shell_output_registry: Option<Arc<agentdash_relay::ShellOutputRegistry>>,
    overlay: Option<Arc<InlineContentOverlay>>,
    identity: Option<agentdash_platform_spi::platform::auth::AuthIdentity>,
    read_execution_state: FsReadExecutionState,
    patch_execution_state: FsApplyPatchExecutionState,
}

impl AppliedVfsRuntimeToolService {
    pub fn new(
        service: Arc<VfsService>,
        terminal_registry: Arc<dyn ShellTerminalRegistry>,
    ) -> Self {
        Self {
            service,
            terminal_registry,
            materialization: None,
            shell_output_registry: None,
            overlay: None,
            identity: None,
            read_execution_state: FsReadExecutionState::default(),
            patch_execution_state: FsApplyPatchExecutionState::default(),
        }
    }

    pub fn with_materialization(
        mut self,
        materialization: Option<Arc<VfsMaterializationService>>,
    ) -> Self {
        self.materialization = materialization;
        self
    }

    pub fn with_shell_output_registry(
        mut self,
        registry: Option<Arc<agentdash_relay::ShellOutputRegistry>>,
    ) -> Self {
        self.shell_output_registry = registry;
        self
    }

    pub fn with_overlay(mut self, overlay: Option<Arc<InlineContentOverlay>>) -> Self {
        self.overlay = overlay;
        self
    }

    pub fn with_identity(
        mut self,
        identity: Option<agentdash_platform_spi::platform::auth::AuthIdentity>,
    ) -> Self {
        self.identity = identity;
        self
    }

    pub fn parameters_schema(kind: AppliedVfsToolKind) -> Value {
        match kind {
            AppliedVfsToolKind::MountsList => MountsListExecutor::parameters_schema(),
            AppliedVfsToolKind::Read => FsReadExecutor::parameters_schema(),
            AppliedVfsToolKind::Glob => FsGlobExecutor::parameters_schema(),
            AppliedVfsToolKind::Grep => FsGrepExecutor::parameters_schema(),
            AppliedVfsToolKind::ApplyPatch => FsApplyPatchExecutor::parameters_schema(),
            AppliedVfsToolKind::ShellExec => ShellExecExecutor::parameters_schema(),
        }
    }

    pub async fn execute(&self, request: AppliedVfsToolRequest) -> AppliedVfsToolOutcome {
        self.execute_with_controls(request, CancellationToken::new(), None)
            .await
    }

    pub async fn execute_with_controls(
        &self,
        request: AppliedVfsToolRequest,
        cancel: CancellationToken,
        updates: Option<VfsToolUpdateSink>,
    ) -> AppliedVfsToolOutcome {
        let (vfs, policy, capability_state) = match build_invocation_vfs(request.surface) {
            Ok(value) => value,
            Err(message) => {
                return AppliedVfsToolOutcome::Rejected {
                    code: "invalid_applied_vfs_surface".to_owned(),
                    message,
                };
            }
        };
        let shared = SharedRuntimeVfs::new_with_policy(vfs, policy);
        let result = match request.kind {
            AppliedVfsToolKind::MountsList => {
                MountsListExecutor::new(self.service.clone(), shared)
                    .execute(request.arguments, cancel)
                    .await
            }
            AppliedVfsToolKind::Read => {
                FsReadExecutor::new(
                    self.service.clone(),
                    shared,
                    self.overlay.clone(),
                    self.identity.clone(),
                )
                .with_execution_state(
                    self.read_execution_state.clone(),
                    runtime_owner_scope(&request.owner),
                )
                .execute(request.arguments, cancel)
                .await
            }
            AppliedVfsToolKind::Glob => {
                FsGlobExecutor::new(
                    self.service.clone(),
                    shared,
                    self.overlay.clone(),
                    self.identity.clone(),
                )
                .execute(request.arguments, cancel)
                .await
            }
            AppliedVfsToolKind::Grep => {
                FsGrepExecutor::new(
                    self.service.clone(),
                    shared,
                    self.overlay.clone(),
                    self.identity.clone(),
                )
                .execute(request.arguments, cancel)
                .await
            }
            AppliedVfsToolKind::ApplyPatch => {
                FsApplyPatchExecutor::new(
                    self.service.clone(),
                    shared,
                    self.overlay.clone(),
                    self.identity.clone(),
                )
                .with_execution_state(self.patch_execution_state.clone())
                .execute(request.arguments, cancel)
                .await
            }
            AppliedVfsToolKind::ShellExec => {
                let runtime_thread_id = match agentdash_agent_runtime_contract::RuntimeThreadId::new(
                    request.owner.runtime_thread_id.clone(),
                ) {
                    Ok(value) => value,
                    Err(error) => {
                        return AppliedVfsToolOutcome::Rejected {
                            code: "invalid_runtime_tool_owner".to_owned(),
                            message: error.to_string(),
                        };
                    }
                };
                let mut executor = ShellExecExecutor::new(self.service.clone(), shared)
                    .with_terminal_owner(ShellTerminalOwner {
                        run_id: request.owner.run_id,
                        agent_id: request.owner.agent_id,
                        runtime_thread_id,
                    })
                    .with_terminal_registry(self.terminal_registry.clone())
                    .with_materialization_context(
                        self.materialization.clone(),
                        request.owner.run_id.to_string(),
                        Some(request.owner.invocation_id.clone()),
                        self.overlay.clone(),
                        self.identity.clone(),
                    )
                    .with_capability_state(capability_state);
                if let Some(registry) = &self.shell_output_registry {
                    executor = executor.with_shell_output_registry(registry.clone());
                }
                executor
                    .execute(
                        &request.owner.invocation_id,
                        request.arguments,
                        cancel,
                        updates,
                    )
                    .await
            }
        };
        match result {
            Ok(result) if result.is_error => AppliedVfsToolOutcome::Failed {
                code: "vfs_tool_failed".to_owned(),
                message: result
                    .content
                    .iter()
                    .filter_map(|part| match part {
                        crate::runtime_tool_execution::VfsToolContent::Text { text } => {
                            Some(text.as_str())
                        }
                        crate::runtime_tool_execution::VfsToolContent::Image { .. } => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            },
            Ok(result) => AppliedVfsToolOutcome::Completed {
                output: serde_json::to_value(result).unwrap_or_else(|error| {
                    serde_json::json!({"error": format!("failed to encode VFS tool result: {error}")})
                }),
            },
            Err(VfsToolExecutionError::InvalidArguments(message)) => {
                AppliedVfsToolOutcome::Rejected {
                    code: "invalid_vfs_tool_arguments".to_owned(),
                    message,
                }
            }
            Err(VfsToolExecutionError::Cancelled) => AppliedVfsToolOutcome::Rejected {
                code: "vfs_tool_cancelled".to_owned(),
                message: "VFS tool execution was cancelled".to_owned(),
            },
            Err(VfsToolExecutionError::ExecutionFailed(message)) => AppliedVfsToolOutcome::Failed {
                code: "vfs_tool_failed".to_owned(),
                message,
            },
        }
    }
}

fn runtime_owner_scope(owner: &AppliedVfsToolOwner) -> String {
    format!(
        "{}\u{1f}{}\u{1f}{}",
        owner.run_id, owner.agent_id, owner.runtime_thread_id
    )
}

fn build_invocation_vfs(
    surface: AppliedVfsToolSurface,
) -> Result<(Vfs, RuntimeVfsAccessPolicy, CapabilityState), String> {
    if surface.mounts.is_empty() {
        return Err("applied VFS surface must contain at least one authorized mount".to_owned());
    }
    let mut mount_ids = BTreeSet::new();
    let mut mounts = Vec::with_capacity(surface.mounts.len());
    let mut rules = Vec::new();
    let mut clusters = BTreeSet::new();
    for mount in surface.mounts {
        if mount.id.trim().is_empty()
            || mount.provider.trim().is_empty()
            || mount.root_ref.trim().is_empty()
            || !mount_ids.insert(mount.id.clone())
            || mount.operations.is_empty()
            || mount.path_scopes.is_empty()
        {
            return Err("applied VFS mount evidence is incomplete or duplicated".to_owned());
        }
        let capabilities = mount
            .operations
            .iter()
            .copied()
            .map(map_capability)
            .collect::<Vec<_>>();
        let operations = mount
            .operations
            .iter()
            .copied()
            .flat_map(map_policy_operations)
            .collect::<BTreeSet<_>>();
        for operation in &mount.operations {
            clusters.insert(match operation {
                AppliedVfsToolOperation::Read
                | AppliedVfsToolOperation::List
                | AppliedVfsToolOperation::Search => ToolCluster::Read,
                AppliedVfsToolOperation::Write => ToolCluster::Write,
                AppliedVfsToolOperation::Execute => ToolCluster::Execute,
            });
        }
        for scope in mount.path_scopes {
            let pattern = match scope {
                AppliedVfsToolPathScope::All => RuntimeVfsPathPattern::All,
                AppliedVfsToolPathScope::Exact(path) => {
                    ensure_canonical_scope(&path)?;
                    RuntimeVfsPathPattern::Exact(path)
                }
                AppliedVfsToolPathScope::Prefix(path) => {
                    ensure_canonical_scope(&path)?;
                    RuntimeVfsPathPattern::Prefix(path)
                }
            };
            rules.push(RuntimeVfsAccessRule {
                mount_id: mount.id.clone(),
                path_pattern: pattern,
                operations: operations.clone(),
                source: RuntimeVfsAccessSource::ProjectPreset,
            });
        }
        mounts.push(Mount {
            id: mount.id,
            provider: mount.provider,
            backend_id: mount.backend_id,
            root_ref: mount.root_ref,
            capabilities,
            default_write: false,
            display_name: mount.display_name,
            metadata: mount.metadata,
        });
    }
    if surface
        .default_mount_id
        .as_ref()
        .is_some_and(|id| !mount_ids.contains(id))
    {
        return Err("default mount is absent from the authorized invocation surface".to_owned());
    }
    Ok((
        Vfs {
            mounts,
            default_mount_id: surface.default_mount_id,
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        },
        RuntimeVfsAccessPolicy { rules },
        CapabilityState::from_clusters(clusters),
    ))
}

fn ensure_canonical_scope(path: &str) -> Result<(), String> {
    if path.is_empty()
        || path.starts_with('/')
        || path.contains(['\\', '\0'])
        || path
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return Err(format!("VFS path scope `{path}` is not canonical"));
    }
    Ok(())
}

fn map_capability(operation: AppliedVfsToolOperation) -> MountCapability {
    match operation {
        AppliedVfsToolOperation::Read => MountCapability::Read,
        AppliedVfsToolOperation::List => MountCapability::List,
        AppliedVfsToolOperation::Search => MountCapability::Search,
        AppliedVfsToolOperation::Write => MountCapability::Write,
        AppliedVfsToolOperation::Execute => MountCapability::Exec,
    }
}

fn map_policy_operations(
    operation: AppliedVfsToolOperation,
) -> impl IntoIterator<Item = RuntimeVfsOperation> {
    match operation {
        AppliedVfsToolOperation::Read => vec![RuntimeVfsOperation::Read],
        AppliedVfsToolOperation::List => vec![RuntimeVfsOperation::List],
        AppliedVfsToolOperation::Search => vec![RuntimeVfsOperation::Search],
        AppliedVfsToolOperation::Write => {
            vec![RuntimeVfsOperation::Write, RuntimeVfsOperation::ApplyPatch]
        }
        AppliedVfsToolOperation::Execute => vec![RuntimeVfsOperation::Exec],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        MountProviderRegistry,
        tools::{ShellTerminalOutputSnapshot, ShellTerminalRegistration, ShellTerminalRegistry},
    };

    struct NoopTerminalRegistry;

    impl ShellTerminalRegistry for NoopTerminalRegistry {
        fn register_shell_terminal(&self, _: ShellTerminalRegistration) {}

        fn resolve_shell_terminal(&self, _: &str) -> Option<ShellTerminalRegistration> {
            None
        }

        fn record_shell_terminal_output_snapshot(&self, _: ShellTerminalOutputSnapshot<'_>) {}

        fn remove_shell_terminal(&self, _: &str) {}
    }

    #[test]
    fn exact_scope_never_expands_to_descendants_or_prefix_siblings() {
        let (_, policy, _) = build_invocation_vfs(surface(
            "main",
            AppliedVfsToolPathScope::Exact("docs/readme.md".to_owned()),
        ))
        .unwrap();
        assert!(policy.admits("main", "docs/readme.md", RuntimeVfsOperation::Read));
        assert!(!policy.admits("main", "docs/readme.md/child", RuntimeVfsOperation::Read));
        assert!(!policy.admits("main", "docs2/readme.md", RuntimeVfsOperation::Read));
    }

    #[test]
    fn invocation_surfaces_do_not_share_mounts_between_agent_runs() {
        let (first, _, _) =
            build_invocation_vfs(surface("first", AppliedVfsToolPathScope::All)).unwrap();
        let (second, _, _) =
            build_invocation_vfs(surface("second", AppliedVfsToolPathScope::All)).unwrap();
        assert_eq!(first.mounts[0].id, "first");
        assert_eq!(second.mounts[0].id, "second");
        assert!(!second.mounts.iter().any(|mount| mount.id == "first"));
    }

    #[test]
    fn non_canonical_scopes_are_rejected_before_provider_dispatch() {
        for scope in [
            AppliedVfsToolPathScope::Exact("../secret".to_owned()),
            AppliedVfsToolPathScope::Prefix("docs//private".to_owned()),
            AppliedVfsToolPathScope::Prefix("C:\\repo".to_owned()),
        ] {
            assert!(build_invocation_vfs(surface("main", scope)).is_err());
        }
    }

    #[test]
    fn invocation_capabilities_are_derived_from_each_applied_surface() {
        let (_, _, read) =
            build_invocation_vfs(surface("read", AppliedVfsToolPathScope::All)).unwrap();
        assert!(read.has(ToolCluster::Read));
        assert!(!read.has(ToolCluster::Write));
        assert!(!read.has(ToolCluster::Execute));

        let mut write_surface = surface("write", AppliedVfsToolPathScope::All);
        write_surface.mounts[0].operations = BTreeSet::from([
            AppliedVfsToolOperation::Write,
            AppliedVfsToolOperation::Execute,
        ]);
        let (_, _, write) = build_invocation_vfs(write_surface).unwrap();
        assert!(!write.has(ToolCluster::Read));
        assert!(write.has(ToolCluster::Write));
        assert!(write.has(ToolCluster::Execute));
    }

    #[tokio::test]
    async fn direct_runtime_surface_preserves_typed_invalid_argument_errors() {
        let result = service().execute(request(AppliedVfsToolKind::Read)).await;

        assert!(matches!(
            result,
            AppliedVfsToolOutcome::Rejected { code, .. }
                if code == "invalid_vfs_tool_arguments"
        ));
    }

    #[tokio::test]
    async fn direct_runtime_surface_honors_cancellation_without_provider_dispatch() {
        let cancel = CancellationToken::new();
        cancel.cancel();

        let result = service()
            .execute_with_controls(request(AppliedVfsToolKind::MountsList), cancel, None)
            .await;

        assert_eq!(
            result,
            AppliedVfsToolOutcome::Rejected {
                code: "vfs_tool_cancelled".to_owned(),
                message: "VFS tool execution was cancelled".to_owned(),
            }
        );
    }

    fn service() -> AppliedVfsRuntimeToolService {
        AppliedVfsRuntimeToolService::new(
            Arc::new(VfsService::new(Arc::new(MountProviderRegistry::new()))),
            Arc::new(NoopTerminalRegistry),
        )
    }

    fn request(kind: AppliedVfsToolKind) -> AppliedVfsToolRequest {
        AppliedVfsToolRequest {
            kind,
            arguments: Value::Object(Default::default()),
            surface: surface("main", AppliedVfsToolPathScope::All),
            owner: AppliedVfsToolOwner {
                run_id: Uuid::nil(),
                agent_id: Uuid::nil(),
                runtime_thread_id: "runtime-thread-test".to_owned(),
                invocation_id: "invocation-test".to_owned(),
            },
        }
    }

    fn surface(id: &str, scope: AppliedVfsToolPathScope) -> AppliedVfsToolSurface {
        AppliedVfsToolSurface {
            default_mount_id: Some(id.to_owned()),
            mounts: vec![AppliedVfsToolMount {
                id: id.to_owned(),
                provider: "memory".to_owned(),
                backend_id: "backend".to_owned(),
                root_ref: format!("memory://{id}"),
                display_name: id.to_owned(),
                metadata: Value::Null,
                operations: BTreeSet::from([AppliedVfsToolOperation::Read]),
                path_scopes: vec![scope],
            }],
        }
    }
}
