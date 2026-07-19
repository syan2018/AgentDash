use std::{
    collections::BTreeSet,
    sync::Arc,
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
}
