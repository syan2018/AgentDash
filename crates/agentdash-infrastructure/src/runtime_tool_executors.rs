use std::{
    collections::BTreeSet,
    sync::{Arc, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

use agentdash_agent_runtime::{
    RuntimeTaskExecutionScope, RuntimeTaskGrantedOperation, RuntimeToolDefinition,
    RuntimeToolEffect, RuntimeToolExecutor, RuntimeToolInvocation, RuntimeToolPermission,
    RuntimeToolResourceGrant, RuntimeVfsGrantedOperation, RuntimeVfsPathGrant,
};
use agentdash_agent_runtime_contract::{RuntimeItemId, RuntimeTurnId, SurfaceRevision};
use agentdash_agent_service_api::{AgentToolName, AgentToolResult};
use agentdash_application_agentrun::runtime_task_tools::{
    RuntimeTaskToolKind, RuntimeTaskToolOutcome, RuntimeTaskToolRequest, RuntimeTaskToolScope,
    RuntimeTaskToolService,
};
use agentdash_application_ports::product_runtime_tool::{
    ProductRuntimeToolContext, ProductRuntimeToolKind, ProductRuntimeToolOutcome,
    ProductRuntimeToolRequest, ProductRuntimeToolService, ProductRuntimeToolTarget,
};
use agentdash_application_vfs::{
    AppliedVfsRuntimeToolService, AppliedVfsToolKind, AppliedVfsToolMount, AppliedVfsToolOperation,
    AppliedVfsToolOutcome, AppliedVfsToolOwner, AppliedVfsToolPathScope, AppliedVfsToolRequest,
    AppliedVfsToolSurface,
};
use agentdash_contracts::workspace_module::WorkspaceModulePresentation;
use agentdash_workspace_module::workspace_module::presentation_protocol::{
    WorkspaceModulePresentationActor, WorkspaceModulePresentationActorKind,
    WorkspaceModulePresentationCause, WorkspaceModulePresentationCommand,
    WorkspaceModulePresentationCommandPort, WorkspaceModulePresentationEffectId,
};
use async_trait::async_trait;
use uuid::Uuid;

pub fn final_runtime_tool_catalog(
    vfs: Arc<AppliedVfsRuntimeToolService>,
    task: Arc<dyn RuntimeTaskToolService>,
    workspace_module_present: Arc<dyn RuntimeToolExecutor>,
) -> Vec<Arc<dyn RuntimeToolExecutor>> {
    vec![
        Arc::new(MountsListRuntimeTool::new(vfs.clone())),
        Arc::new(FsReadRuntimeTool::new(vfs.clone())),
        Arc::new(FsGlobRuntimeTool::new(vfs.clone())),
        Arc::new(FsGrepRuntimeTool::new(vfs.clone())),
        Arc::new(FsApplyPatchRuntimeTool::new(vfs.clone())),
        Arc::new(ShellExecRuntimeTool::new(vfs)),
        Arc::new(RuntimeTaskReadTool::new(task.clone())),
        Arc::new(RuntimeTaskWriteTool::new(task)),
        workspace_module_present,
    ]
}

pub fn product_runtime_tool_catalog(
    services: impl IntoIterator<Item = Arc<dyn ProductRuntimeToolService>>,
) -> Vec<Arc<dyn RuntimeToolExecutor>> {
    services
        .into_iter()
        .map(|service| {
            Arc::new(ProductCommandRuntimeTool::new(service)) as Arc<dyn RuntimeToolExecutor>
        })
        .collect()
}

pub struct DeferredProductRuntimeToolService {
    kind: ProductRuntimeToolKind,
    parameters_schema: serde_json::Value,
    service: OnceLock<Arc<dyn ProductRuntimeToolService>>,
}

impl DeferredProductRuntimeToolService {
    pub fn new(kind: ProductRuntimeToolKind, parameters_schema: serde_json::Value) -> Self {
        Self {
            kind,
            parameters_schema,
            service: OnceLock::new(),
        }
    }

    pub fn install(&self, service: Arc<dyn ProductRuntimeToolService>) -> Result<(), String> {
        if service.kind() != self.kind {
            return Err(format!(
                "Product runtime tool binding kind mismatch: expected {:?}, received {:?}",
                self.kind,
                service.kind()
            ));
        }
        self.service
            .set(service)
            .map_err(|_| format!("Product runtime tool {:?} is already installed", self.kind))
    }

    pub fn is_installed(&self) -> bool {
        self.service.get().is_some()
    }
}

#[async_trait]
impl ProductRuntimeToolService for DeferredProductRuntimeToolService {
    fn kind(&self) -> ProductRuntimeToolKind {
        self.kind
    }

    fn parameters_schema(&self) -> serde_json::Value {
        self.parameters_schema.clone()
    }

    async fn execute(&self, request: ProductRuntimeToolRequest) -> ProductRuntimeToolOutcome {
        let Some(service) = self.service.get().cloned() else {
            return ProductRuntimeToolOutcome::Failed {
                code: "product_runtime_tool_not_installed".to_owned(),
                message: format!(
                    "Product runtime tool {:?} has not completed application composition",
                    self.kind
                ),
            };
        };
        service.execute(request).await
    }
}

pub struct ProductCommandRuntimeTool {
    service: Arc<dyn ProductRuntimeToolService>,
}

impl ProductCommandRuntimeTool {
    pub fn new(service: Arc<dyn ProductRuntimeToolService>) -> Self {
        Self { service }
    }
}

#[async_trait]
impl RuntimeToolExecutor for ProductCommandRuntimeTool {
    fn definition(&self) -> RuntimeToolDefinition {
        let (name, description, permission, effect) = product_tool_definition(self.service.kind());
        RuntimeToolDefinition {
            name: AgentToolName::new(name).expect("static Product runtime tool name"),
            description: description.to_owned(),
            parameters_schema: self.service.parameters_schema(),
            permission,
            effect,
        }
    }

    async fn execute(&self, invocation: RuntimeToolInvocation) -> AgentToolResult {
        if !matches!(
            invocation.grant.resources,
            RuntimeToolResourceGrant::Product
        ) {
            return rejected(
                "runtime_product_grant_required",
                "Product tool requires a typed Product execution grant",
            );
        }
        let project_id = match parse_uuid("project_id", &invocation.grant.target.project_id) {
            Ok(value) => value,
            Err(result) => return result,
        };
        let run_id = match parse_uuid("run_id", &invocation.grant.target.run_id) {
            Ok(value) => value,
            Err(result) => return result,
        };
        let agent_id = match parse_uuid("agent_id", &invocation.grant.target.agent_id) {
            Ok(value) => value,
            Err(result) => return result,
        };
        let request = ProductRuntimeToolRequest {
            context: ProductRuntimeToolContext {
                runtime_thread_id: invocation.context.runtime_thread_id,
                target: ProductRuntimeToolTarget {
                    project_id,
                    run_id,
                    agent_id,
                },
                turn_id: invocation.context.turn_id.to_string(),
                item_id: invocation.context.item_id.map(|value| value.to_string()),
                effect_id: invocation.context.effect_id.to_string(),
                invocation_id: invocation.context.callback_idempotency_key,
                deadline_at_ms: invocation.context.deadline_at_ms,
            },
            arguments: invocation.arguments,
        };
        match self.service.execute(request).await {
            ProductRuntimeToolOutcome::Completed { output } => {
                AgentToolResult::Completed { output }
            }
            ProductRuntimeToolOutcome::Rejected { code, message } => {
                AgentToolResult::Rejected { code, message }
            }
            ProductRuntimeToolOutcome::Failed { code, message } => {
                AgentToolResult::Failed { code, message }
            }
        }
    }
}

fn product_tool_definition(
    kind: ProductRuntimeToolKind,
) -> (
    &'static str,
    &'static str,
    RuntimeToolPermission,
    RuntimeToolEffect,
) {
    match kind {
        ProductRuntimeToolKind::Wait => (
            "wait",
            "Wait for bounded Product activities without cancelling their background execution.",
            RuntimeToolPermission::ProductRead,
            RuntimeToolEffect::ReadOnly,
        ),
        ProductRuntimeToolKind::CompleteLifecycleNode => (
            "complete_lifecycle_node",
            "Submit the current lifecycle node terminal outcome to Product orchestration.",
            RuntimeToolPermission::ProductWrite,
            RuntimeToolEffect::ProductMutation,
        ),
        ProductRuntimeToolKind::CompanionRequest => (
            "companion_request",
            "Request durable Companion collaboration through the Product command path.",
            RuntimeToolPermission::ProductWrite,
            RuntimeToolEffect::ProductMutation,
        ),
        ProductRuntimeToolKind::CompanionRespond => (
            "companion_respond",
            "Respond to a durable Companion collaboration request.",
            RuntimeToolPermission::ProductWrite,
            RuntimeToolEffect::ProductMutation,
        ),
        ProductRuntimeToolKind::WorkspaceModuleList => (
            "workspace_module_list",
            "List Workspace Modules visible through the applied Product runtime surface.",
            RuntimeToolPermission::ProductRead,
            RuntimeToolEffect::ReadOnly,
        ),
        ProductRuntimeToolKind::WorkspaceModuleDescribe => (
            "workspace_module_describe",
            "Describe one Workspace Module and its visible operations.",
            RuntimeToolPermission::ProductRead,
            RuntimeToolEffect::ReadOnly,
        ),
        ProductRuntimeToolKind::WorkspaceModuleOperate => (
            "workspace_module_operate",
            "Apply a Workspace Module operation through the canonical Product runtime surface.",
            RuntimeToolPermission::ProductWrite,
            RuntimeToolEffect::ProductMutation,
        ),
        ProductRuntimeToolKind::WorkspaceModuleInvoke => (
            "workspace_module_invoke",
            "Invoke a Workspace Module operation through its declared Product dispatch.",
            RuntimeToolPermission::ProductWrite,
            RuntimeToolEffect::ProductMutation,
        ),
    }
}

macro_rules! vfs_executor {
    ($name:ident, $tool_name:literal, $kind:expr, $description:literal, $permission:expr, $effect:expr) => {
        pub struct $name {
            service: Arc<AppliedVfsRuntimeToolService>,
        }

        impl $name {
            pub fn new(service: Arc<AppliedVfsRuntimeToolService>) -> Self {
                Self { service }
            }
        }

        #[async_trait]
        impl RuntimeToolExecutor for $name {
            fn definition(&self) -> RuntimeToolDefinition {
                RuntimeToolDefinition {
                    name: AgentToolName::new($tool_name).expect("static runtime tool name"),
                    description: $description.to_owned(),
                    parameters_schema: AppliedVfsRuntimeToolService::parameters_schema($kind),
                    permission: $permission,
                    effect: $effect,
                }
            }

            async fn execute(&self, invocation: RuntimeToolInvocation) -> AgentToolResult {
                execute_vfs(self.service.as_ref(), $kind, invocation).await
            }
        }
    };
}

vfs_executor!(
    MountsListRuntimeTool,
    "mounts_list",
    AppliedVfsToolKind::MountsList,
    "List VFS mounts granted by the applied AgentRun resource surface.",
    RuntimeToolPermission::VfsRead,
    RuntimeToolEffect::ReadOnly
);
vfs_executor!(
    FsReadRuntimeTool,
    "fs_read",
    AppliedVfsToolKind::Read,
    "Read a file through the applied AgentRun VFS surface.",
    RuntimeToolPermission::VfsRead,
    RuntimeToolEffect::ReadOnly
);
vfs_executor!(
    FsGlobRuntimeTool,
    "fs_glob",
    AppliedVfsToolKind::Glob,
    "List files matching a glob through the applied AgentRun VFS surface.",
    RuntimeToolPermission::VfsRead,
    RuntimeToolEffect::ReadOnly
);
vfs_executor!(
    FsGrepRuntimeTool,
    "fs_grep",
    AppliedVfsToolKind::Grep,
    "Search file contents through the applied AgentRun VFS surface.",
    RuntimeToolPermission::VfsRead,
    RuntimeToolEffect::ReadOnly
);
vfs_executor!(
    FsApplyPatchRuntimeTool,
    "fs_apply_patch",
    AppliedVfsToolKind::ApplyPatch,
    "Apply a patch through the applied AgentRun VFS surface.",
    RuntimeToolPermission::VfsWrite,
    RuntimeToolEffect::VfsMutation
);
vfs_executor!(
    ShellExecRuntimeTool,
    "shell_exec",
    AppliedVfsToolKind::ShellExec,
    "Execute or continue a shell command through the applied AgentRun VFS surface.",
    RuntimeToolPermission::ProcessExecute,
    RuntimeToolEffect::LocalProcess
);

pub struct RuntimeTaskReadTool {
    service: Arc<dyn RuntimeTaskToolService>,
}

impl RuntimeTaskReadTool {
    pub fn new(service: Arc<dyn RuntimeTaskToolService>) -> Self {
        Self { service }
    }
}

pub struct RuntimeTaskWriteTool {
    service: Arc<dyn RuntimeTaskToolService>,
}

impl RuntimeTaskWriteTool {
    pub fn new(service: Arc<dyn RuntimeTaskToolService>) -> Self {
        Self { service }
    }
}

#[async_trait]
impl RuntimeToolExecutor for RuntimeTaskReadTool {
    fn definition(&self) -> RuntimeToolDefinition {
        RuntimeToolDefinition {
            name: AgentToolName::new("task_read").expect("static runtime tool name"),
            description: "Read the granted Product Task scope.".to_owned(),
            parameters_schema: self.service.parameters_schema(RuntimeTaskToolKind::Read),
            permission: RuntimeToolPermission::ProductRead,
            effect: RuntimeToolEffect::ReadOnly,
        }
    }

    async fn execute(&self, invocation: RuntimeToolInvocation) -> AgentToolResult {
        execute_task(self.service.as_ref(), RuntimeTaskToolKind::Read, invocation).await
    }
}

#[async_trait]
impl RuntimeToolExecutor for RuntimeTaskWriteTool {
    fn definition(&self) -> RuntimeToolDefinition {
        RuntimeToolDefinition {
            name: AgentToolName::new("task_write").expect("static runtime tool name"),
            description: "Mutate the granted Product Task scope.".to_owned(),
            parameters_schema: self.service.parameters_schema(RuntimeTaskToolKind::Write),
            permission: RuntimeToolPermission::ProductWrite,
            effect: RuntimeToolEffect::ProductMutation,
        }
    }

    async fn execute(&self, invocation: RuntimeToolInvocation) -> AgentToolResult {
        execute_task(
            self.service.as_ref(),
            RuntimeTaskToolKind::Write,
            invocation,
        )
        .await
    }
}

pub struct WorkspaceModulePresentRuntimeTool {
    product_bindings: Arc<crate::PostgresAgentRunProductRuntimeBindingRepository>,
    presentations: Arc<dyn WorkspaceModulePresentationCommandPort>,
}

impl WorkspaceModulePresentRuntimeTool {
    pub fn new(
        product_bindings: Arc<crate::PostgresAgentRunProductRuntimeBindingRepository>,
        presentations: Arc<dyn WorkspaceModulePresentationCommandPort>,
    ) -> Self {
        Self {
            product_bindings,
            presentations,
        }
    }
}

#[async_trait]
impl RuntimeToolExecutor for WorkspaceModulePresentRuntimeTool {
    fn definition(&self) -> RuntimeToolDefinition {
        RuntimeToolDefinition {
            name: AgentToolName::new("workspace_module_present").expect("static runtime tool name"),
            description:
                "Present a typed Workspace Module view through the Product-owned projection."
                    .to_owned(),
            parameters_schema: serde_json::json!({
                "type": "object",
                "additionalProperties": false,
                "required": [
                    "module_id",
                    "view_key",
                    "renderer_kind",
                    "presentation_uri",
                    "title"
                ],
                "properties": {
                    "module_id": {"type": "string", "minLength": 1},
                    "view_key": {"type": "string", "minLength": 1},
                    "renderer_kind": {"type": "string", "minLength": 1},
                    "presentation_uri": {"type": "string", "minLength": 1},
                    "title": {"type": "string"},
                    "payload": {},
                    "diagnostics": {}
                }
            }),
            permission: RuntimeToolPermission::ProductWrite,
            effect: RuntimeToolEffect::ProductMutation,
        }
    }

    async fn execute(&self, invocation: RuntimeToolInvocation) -> AgentToolResult {
        if !matches!(
            invocation.grant.resources,
            RuntimeToolResourceGrant::Product
        ) {
            return rejected(
                "runtime_product_grant_required",
                "Workspace Module presentation requires a typed Product execution grant",
            );
        }
        let presentation: WorkspaceModulePresentation =
            match serde_json::from_value(invocation.arguments) {
                Ok(value) => value,
                Err(error) => {
                    return rejected(
                        "invalid_workspace_module_presentation",
                        format!("Workspace Module presentation arguments are invalid: {error}"),
                    );
                }
            };
        let Some(item_id) = invocation.context.item_id.as_ref() else {
            return rejected(
                "missing_runtime_tool_item",
                "Workspace Module presentation requires the Complete Agent tool item identity",
            );
        };
        let binding = match self
            .product_bindings
            .load_product_binding_by_runtime_thread(&invocation.context.runtime_thread_id)
            .await
        {
            Ok(Some(binding)) => binding,
            Ok(None) => {
                return rejected(
                    "missing_product_binding",
                    "Runtime thread has no Product target binding",
                );
            }
            Err(error) => {
                return AgentToolResult::Failed {
                    code: "product_binding_query_failed".to_owned(),
                    message: error,
                };
            }
        };
        if binding.target.run_id.to_string() != invocation.grant.target.run_id
            || binding.target.agent_id.to_string() != invocation.grant.target.agent_id
            || binding.runtime_thread_id != invocation.context.runtime_thread_id
        {
            return rejected(
                "stale_product_binding",
                "Runtime callback coordinates differ from the Product target binding",
            );
        }
        let effect_id =
            match WorkspaceModulePresentationEffectId::new(invocation.context.effect_id.as_str()) {
                Ok(value) => value,
                Err(error) => {
                    return rejected("invalid_workspace_presentation_effect", error.to_string());
                }
            };
        let runtime_turn_id = match RuntimeTurnId::new(invocation.context.turn_id.as_str()) {
            Ok(value) => value,
            Err(error) => return rejected("invalid_runtime_turn", error.to_string()),
        };
        let runtime_item_id = match RuntimeItemId::new(item_id.as_str()) {
            Ok(value) => value,
            Err(error) => return rejected("invalid_runtime_item", error.to_string()),
        };
        let command = WorkspaceModulePresentationCommand {
            effect_id,
            target: binding.target,
            actor: WorkspaceModulePresentationActor {
                kind: WorkspaceModulePresentationActorKind::AgentTool,
                actor_id: invocation.context.service_instance_id.to_string(),
            },
            cause: WorkspaceModulePresentationCause {
                runtime_thread_id: binding.runtime_thread_id,
                runtime_operation_id: None,
                runtime_turn_id,
                runtime_item_id,
            },
            source_binding: binding.source_binding,
            surface_revision: SurfaceRevision(invocation.context.applied_surface_revision.0),
            presentation,
            committed_at_ms: now_ms(),
        };
        match self.presentations.present(command).await {
            Ok(change) => AgentToolResult::Completed {
                output: serde_json::json!({
                    "intent_id": change.intent.intent_id,
                    "change_sequence": change.sequence.0,
                    "presentation": change.intent.presentation,
                }),
            },
            Err(error) => AgentToolResult::Failed {
                code: "workspace_module_presentation_commit_failed".to_owned(),
                message: error.to_string(),
            },
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

async fn execute_vfs(
    service: &AppliedVfsRuntimeToolService,
    kind: AppliedVfsToolKind,
    invocation: RuntimeToolInvocation,
) -> AgentToolResult {
    let RuntimeToolResourceGrant::Vfs(grant) = invocation.grant.resources else {
        return rejected(
            "runtime_vfs_grant_required",
            "VFS tool requires a typed VFS execution grant",
        );
    };
    let run_id = match parse_uuid("run_id", &invocation.grant.target.run_id) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let agent_id = match parse_uuid("agent_id", &invocation.grant.target.agent_id) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let surface = AppliedVfsToolSurface {
        default_mount_id: grant.default_mount_id,
        mounts: grant
            .mounts
            .into_iter()
            .map(|mount| AppliedVfsToolMount {
                id: mount.id,
                provider: mount.provider,
                backend_id: mount.backend_id,
                root_ref: mount.root_ref,
                display_name: mount.display_name,
                metadata: mount.metadata,
                operations: mount
                    .operations
                    .into_iter()
                    .map(|operation| match operation {
                        RuntimeVfsGrantedOperation::Read => AppliedVfsToolOperation::Read,
                        RuntimeVfsGrantedOperation::List => AppliedVfsToolOperation::List,
                        RuntimeVfsGrantedOperation::Search => AppliedVfsToolOperation::Search,
                        RuntimeVfsGrantedOperation::Write => AppliedVfsToolOperation::Write,
                        RuntimeVfsGrantedOperation::Execute => AppliedVfsToolOperation::Execute,
                    })
                    .collect::<BTreeSet<_>>(),
                path_scopes: mount
                    .path_scopes
                    .into_iter()
                    .map(|scope| match scope {
                        RuntimeVfsPathGrant::All => AppliedVfsToolPathScope::All,
                        RuntimeVfsPathGrant::Exact(path) => AppliedVfsToolPathScope::Exact(path),
                        RuntimeVfsPathGrant::Prefix(path) => AppliedVfsToolPathScope::Prefix(path),
                    })
                    .collect(),
            })
            .collect(),
    };
    match service
        .execute(AppliedVfsToolRequest {
            kind,
            arguments: invocation.arguments,
            surface,
            owner: AppliedVfsToolOwner {
                run_id,
                agent_id,
                runtime_thread_id: invocation.context.runtime_thread_id.to_string(),
                invocation_id: invocation.context.callback_idempotency_key,
            },
        })
        .await
    {
        AppliedVfsToolOutcome::Completed { output } => AgentToolResult::Completed { output },
        AppliedVfsToolOutcome::Rejected { code, message } => {
            AgentToolResult::Rejected { code, message }
        }
        AppliedVfsToolOutcome::Failed { code, message } => {
            AgentToolResult::Failed { code, message }
        }
    }
}

async fn execute_task(
    service: &dyn RuntimeTaskToolService,
    kind: RuntimeTaskToolKind,
    invocation: RuntimeToolInvocation,
) -> AgentToolResult {
    let RuntimeToolResourceGrant::Task(grant) = invocation.grant.resources else {
        return rejected(
            "runtime_task_grant_required",
            "Task tool requires a typed Task execution grant",
        );
    };
    let required = match kind {
        RuntimeTaskToolKind::Read => RuntimeTaskGrantedOperation::Read,
        RuntimeTaskToolKind::Write => RuntimeTaskGrantedOperation::Write,
    };
    if !grant.operations.contains(&required) {
        return rejected(
            "runtime_task_operation_denied",
            "Task execution grant does not allow the requested operation",
        );
    }
    let run_id = match parse_uuid("run_id", &invocation.grant.target.run_id) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let agent_id = match parse_uuid("agent_id", &invocation.grant.target.agent_id) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let target_project_id = match parse_uuid("project_id", &invocation.grant.target.project_id) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let scope = match grant.scope {
        RuntimeTaskExecutionScope::Project { project_id } => RuntimeTaskToolScope::Project {
            project_id: match parse_uuid("project_id", &project_id) {
                Ok(value) => value,
                Err(result) => return result,
            },
        },
        RuntimeTaskExecutionScope::Task {
            project_id,
            task_id,
        } => RuntimeTaskToolScope::Task {
            project_id: match parse_uuid("project_id", &project_id) {
                Ok(value) => value,
                Err(result) => return result,
            },
            task_id: match parse_uuid("task_id", &task_id) {
                Ok(value) => value,
                Err(result) => return result,
            },
        },
    };
    let scoped_project_id = match &scope {
        RuntimeTaskToolScope::Project { project_id }
        | RuntimeTaskToolScope::Task { project_id, .. } => *project_id,
    };
    if scoped_project_id != target_project_id {
        return rejected(
            "runtime_task_scope_mismatch",
            "Task grant scope does not belong to the authorized Product target",
        );
    }
    match service
        .execute(RuntimeTaskToolRequest {
            kind,
            scope,
            run_id,
            agent_id,
            arguments: invocation.arguments,
        })
        .await
    {
        RuntimeTaskToolOutcome::Completed { output } => AgentToolResult::Completed { output },
        RuntimeTaskToolOutcome::Rejected { code, message } => {
            AgentToolResult::Rejected { code, message }
        }
        RuntimeTaskToolOutcome::Failed { code, message } => {
            AgentToolResult::Failed { code, message }
        }
    }
}

fn parse_uuid(field: &str, value: &str) -> Result<Uuid, AgentToolResult> {
    Uuid::parse_str(value).map_err(|error| {
        rejected(
            "invalid_runtime_tool_target",
            format!("{field} is not a valid UUID: {error}"),
        )
    })
}

fn rejected(code: impl Into<String>, message: impl Into<String>) -> AgentToolResult {
    AgentToolResult::Rejected {
        code: code.into(),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use agentdash_agent_runtime::{
        RuntimeToolAppliedSurfaceEvidence, RuntimeToolAuthorizationGrant, RuntimeToolProductTarget,
        RuntimeToolProvenanceEvidence, RuntimeToolResolvedContext,
    };
    use agentdash_agent_runtime_contract::RuntimeThreadId;
    use agentdash_agent_service_api::{
        AgentBindingGeneration, AgentEffectIdentity, AgentItemId, AgentProfileDigest,
        AgentServiceInstanceId, AgentSourceCoordinate, AgentSurfaceDigest, AgentSurfaceRevision,
        AgentTurnId,
    };
    use serde_json::Value;

    use super::*;
    use agentdash_application_vfs::tools::{
        ShellTerminalOutputSnapshot, ShellTerminalRegistration, ShellTerminalRegistry,
    };
    use agentdash_application_vfs::{MountProviderRegistry, VfsService};

    struct NoopTerminalRegistry;

    impl ShellTerminalRegistry for NoopTerminalRegistry {
        fn register_shell_terminal(&self, _: ShellTerminalRegistration) {}

        fn resolve_shell_terminal(&self, _: &str) -> Option<ShellTerminalRegistration> {
            None
        }

        fn record_shell_terminal_output_snapshot(&self, _: ShellTerminalOutputSnapshot<'_>) {}

        fn remove_shell_terminal(&self, _: &str) {}
    }

    struct NoopTaskService;

    #[async_trait]
    impl RuntimeTaskToolService for NoopTaskService {
        fn parameters_schema(&self, kind: RuntimeTaskToolKind) -> Value {
            match kind {
                RuntimeTaskToolKind::Read => {
                    serde_json::json!({"type": "object", "owner": "task_read"})
                }
                RuntimeTaskToolKind::Write => {
                    serde_json::json!({"type": "object", "owner": "task_write"})
                }
            }
        }

        async fn execute(&self, _: RuntimeTaskToolRequest) -> RuntimeTaskToolOutcome {
            RuntimeTaskToolOutcome::Completed {
                output: Value::Null,
            }
        }
    }

    struct NoopWorkspaceModulePresentTool;

    #[async_trait]
    impl RuntimeToolExecutor for NoopWorkspaceModulePresentTool {
        fn definition(&self) -> RuntimeToolDefinition {
            RuntimeToolDefinition {
                name: AgentToolName::new("workspace_module_present").expect("tool"),
                description: "fixture".to_owned(),
                parameters_schema: serde_json::json!({
                    "type": "object",
                    "owner": "workspace_module_present"
                }),
                permission: RuntimeToolPermission::ProductWrite,
                effect: RuntimeToolEffect::ProductMutation,
            }
        }

        async fn execute(&self, _: RuntimeToolInvocation) -> AgentToolResult {
            AgentToolResult::Completed {
                output: Value::Null,
            }
        }
    }

    struct NoopProductToolService {
        kind: ProductRuntimeToolKind,
    }

    #[async_trait]
    impl ProductRuntimeToolService for NoopProductToolService {
        fn kind(&self) -> ProductRuntimeToolKind {
            self.kind
        }

        fn parameters_schema(&self) -> Value {
            serde_json::json!({
                "type": "object",
                "owner": format!("{:?}", self.kind),
            })
        }

        async fn execute(&self, request: ProductRuntimeToolRequest) -> ProductRuntimeToolOutcome {
            ProductRuntimeToolOutcome::Completed {
                output: serde_json::json!({
                    "runtime_thread_id": request.context.runtime_thread_id,
                    "project_id": request.context.target.project_id,
                    "run_id": request.context.target.run_id,
                    "agent_id": request.context.target.agent_id,
                    "turn_id": request.context.turn_id,
                    "item_id": request.context.item_id,
                    "effect_id": request.context.effect_id,
                    "invocation_id": request.context.invocation_id,
                    "arguments": request.arguments,
                }),
            }
        }
    }

    #[test]
    fn final_runtime_catalog_defines_all_nine_platform_tools() {
        let vfs = Arc::new(AppliedVfsRuntimeToolService::new(
            Arc::new(VfsService::new(Arc::new(MountProviderRegistry::new()))),
            Arc::new(NoopTerminalRegistry),
        ));
        let task: Arc<dyn RuntimeTaskToolService> = Arc::new(NoopTaskService);
        let executors =
            final_runtime_tool_catalog(vfs, task, Arc::new(NoopWorkspaceModulePresentTool));
        assert_eq!(
            executors
                .iter()
                .map(|executor| executor.definition().name.to_string())
                .collect::<Vec<_>>(),
            vec![
                "mounts_list",
                "fs_read",
                "fs_glob",
                "fs_grep",
                "fs_apply_patch",
                "shell_exec",
                "task_read",
                "task_write",
                "workspace_module_present",
            ]
        );
    }

    #[test]
    fn final_runtime_catalog_uses_all_nine_owner_parameter_schemas_exactly() {
        let vfs = Arc::new(AppliedVfsRuntimeToolService::new(
            Arc::new(VfsService::new(Arc::new(MountProviderRegistry::new()))),
            Arc::new(NoopTerminalRegistry),
        ));
        let task: Arc<dyn RuntimeTaskToolService> = Arc::new(NoopTaskService);
        let expected = vec![
            AppliedVfsRuntimeToolService::parameters_schema(AppliedVfsToolKind::MountsList),
            AppliedVfsRuntimeToolService::parameters_schema(AppliedVfsToolKind::Read),
            AppliedVfsRuntimeToolService::parameters_schema(AppliedVfsToolKind::Glob),
            AppliedVfsRuntimeToolService::parameters_schema(AppliedVfsToolKind::Grep),
            AppliedVfsRuntimeToolService::parameters_schema(AppliedVfsToolKind::ApplyPatch),
            AppliedVfsRuntimeToolService::parameters_schema(AppliedVfsToolKind::ShellExec),
            task.parameters_schema(RuntimeTaskToolKind::Read),
            task.parameters_schema(RuntimeTaskToolKind::Write),
            NoopWorkspaceModulePresentTool
                .definition()
                .parameters_schema,
        ];

        let actual =
            final_runtime_tool_catalog(vfs, task, Arc::new(NoopWorkspaceModulePresentTool))
                .into_iter()
                .map(|executor| executor.definition().parameters_schema)
                .collect::<Vec<_>>();

        assert_eq!(actual, expected);
        assert_eq!(actual[1]["properties"]["path"]["type"], "string");
        assert!(schema_contains_enum_value(
            &actual[5]["properties"]["operation"],
            "start"
        ));
    }

    #[test]
    fn product_runtime_catalog_exposes_all_product_tool_families() {
        let services = [
            ProductRuntimeToolKind::Wait,
            ProductRuntimeToolKind::CompleteLifecycleNode,
            ProductRuntimeToolKind::CompanionRequest,
            ProductRuntimeToolKind::CompanionRespond,
            ProductRuntimeToolKind::WorkspaceModuleList,
            ProductRuntimeToolKind::WorkspaceModuleDescribe,
            ProductRuntimeToolKind::WorkspaceModuleOperate,
            ProductRuntimeToolKind::WorkspaceModuleInvoke,
        ]
        .into_iter()
        .map(|kind| {
            Arc::new(NoopProductToolService { kind }) as Arc<dyn ProductRuntimeToolService>
        });
        let definitions = product_runtime_tool_catalog(services)
            .into_iter()
            .map(|executor| executor.definition())
            .collect::<Vec<_>>();

        assert_eq!(
            definitions
                .iter()
                .map(|definition| definition.name.to_string())
                .collect::<Vec<_>>(),
            vec![
                "wait",
                "complete_lifecycle_node",
                "companion_request",
                "companion_respond",
                "workspace_module_list",
                "workspace_module_describe",
                "workspace_module_operate",
                "workspace_module_invoke",
            ]
        );
        assert_eq!(
            definitions[0].permission,
            RuntimeToolPermission::ProductRead
        );
        assert_eq!(definitions[0].effect, RuntimeToolEffect::ReadOnly);
        assert!(definitions[1..4].iter().all(|definition| {
            definition.permission == RuntimeToolPermission::ProductWrite
                && definition.effect == RuntimeToolEffect::ProductMutation
        }));
        assert!(definitions[4..6].iter().all(|definition| {
            definition.permission == RuntimeToolPermission::ProductRead
                && definition.effect == RuntimeToolEffect::ReadOnly
        }));
        assert!(definitions[6..].iter().all(|definition| {
            definition.permission == RuntimeToolPermission::ProductWrite
                && definition.effect == RuntimeToolEffect::ProductMutation
        }));
    }

    #[tokio::test]
    async fn product_runtime_executor_forwards_authorized_callback_coordinates() {
        let project_id = Uuid::new_v4();
        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let executor = ProductCommandRuntimeTool::new(Arc::new(NoopProductToolService {
            kind: ProductRuntimeToolKind::Wait,
        }));
        let result = executor
            .execute(RuntimeToolInvocation {
                context: product_runtime_context(),
                tool: AgentToolName::new("wait").expect("tool"),
                arguments: serde_json::json!({"activity_refs": ["gate"]}),
                grant: product_grant(project_id, run_id, agent_id),
            })
            .await;

        let AgentToolResult::Completed { output } = result else {
            panic!("Product command must complete");
        };
        assert_eq!(output["runtime_thread_id"], "runtime-thread");
        assert_eq!(output["project_id"], project_id.to_string());
        assert_eq!(output["run_id"], run_id.to_string());
        assert_eq!(output["agent_id"], agent_id.to_string());
        assert_eq!(output["turn_id"], "turn");
        assert_eq!(output["item_id"], "item");
        assert_eq!(output["effect_id"], "effect");
        assert_eq!(output["invocation_id"], "callback");
        assert_eq!(output["arguments"]["activity_refs"][0], "gate");
    }

    fn schema_contains_enum_value(schema: &Value, expected: &str) -> bool {
        match schema {
            Value::Array(values) => values
                .iter()
                .any(|value| schema_contains_enum_value(value, expected)),
            Value::Object(object) => {
                object.get("enum").is_some_and(|values| {
                    values
                        .as_array()
                        .is_some_and(|values| values.iter().any(|value| value == expected))
                }) || object
                    .values()
                    .any(|value| schema_contains_enum_value(value, expected))
            }
            _ => false,
        }
    }

    fn product_runtime_context() -> RuntimeToolResolvedContext {
        RuntimeToolResolvedContext {
            runtime_thread_id: RuntimeThreadId::new("runtime-thread").expect("thread"),
            binding_generation: AgentBindingGeneration(1),
            source: AgentSourceCoordinate::new("source").expect("source"),
            service_instance_id: AgentServiceInstanceId::new("service").expect("service"),
            profile_digest: AgentProfileDigest::new("profile").expect("profile"),
            bound_surface_revision: AgentSurfaceRevision(1),
            bound_surface_digest: AgentSurfaceDigest::new("bound").expect("bound"),
            applied_surface_revision: AgentSurfaceRevision(1),
            applied_surface_digest: AgentSurfaceDigest::new("applied").expect("applied"),
            turn_id: AgentTurnId::new("turn").expect("turn"),
            item_id: Some(AgentItemId::new("item").expect("item")),
            effect_id: AgentEffectIdentity::new("effect").expect("effect"),
            callback_idempotency_key: "callback".to_owned(),
            deadline_at_ms: u64::MAX,
        }
    }

    fn product_grant(
        project_id: Uuid,
        run_id: Uuid,
        agent_id: Uuid,
    ) -> RuntimeToolAuthorizationGrant {
        let provenance = RuntimeToolProvenanceEvidence {
            source_kind: "test".to_owned(),
            source_id: "surface".to_owned(),
            source_revision: 1,
            projection_revision: 1,
            captured_at_ms: 1,
        };
        RuntimeToolAuthorizationGrant {
            permission: RuntimeToolPermission::ProductRead,
            effect: RuntimeToolEffect::ReadOnly,
            target: RuntimeToolProductTarget {
                project_id: project_id.to_string(),
                run_id: run_id.to_string(),
                agent_id: agent_id.to_string(),
            },
            applied_surface: RuntimeToolAppliedSurfaceEvidence {
                snapshot_revision: 1,
                agent_surface_revision: 1,
                agent_surface_digest: "surface".to_owned(),
                vfs_revision: 1,
                vfs_digest: "vfs".to_owned(),
                vfs_provenance: provenance.clone(),
                task_revision: 1,
                task_digest: "task".to_owned(),
                task_provenance: provenance,
                product_binding_digest: "binding".to_owned(),
                host_binding_generation: 1,
            },
            resources: RuntimeToolResourceGrant::Product,
        }
    }
}
