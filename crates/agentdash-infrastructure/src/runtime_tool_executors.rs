use std::{collections::BTreeSet, sync::Arc};

use agentdash_agent_runtime::{
    RuntimeTaskExecutionScope, RuntimeTaskGrantedOperation, RuntimeToolDefinition,
    RuntimeToolEffect, RuntimeToolExecutor, RuntimeToolInvocation, RuntimeToolPermission,
    RuntimeToolResourceGrant, RuntimeVfsGrantedOperation, RuntimeVfsPathGrant,
};
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
use async_trait::async_trait;
use serde_json::{Value, json};
use uuid::Uuid;

pub fn final_runtime_tool_catalog(
    vfs: Arc<AppliedVfsRuntimeToolService>,
    task: Arc<dyn RuntimeTaskToolService>,
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
    ]
}

macro_rules! vfs_executor {
    ($name:ident, $tool_name:literal, $kind:expr, $description:literal, $permission:expr, $effect:expr, $schema:expr) => {
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
                    parameters_schema: $schema,
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
    RuntimeToolEffect::ReadOnly,
    object_schema(&[], &[])
);
vfs_executor!(
    FsReadRuntimeTool,
    "fs_read",
    AppliedVfsToolKind::Read,
    "Read a file through the applied AgentRun VFS surface.",
    RuntimeToolPermission::VfsRead,
    RuntimeToolEffect::ReadOnly,
    object_schema(&["path"], &["path", "offset", "limit"])
);
vfs_executor!(
    FsGlobRuntimeTool,
    "fs_glob",
    AppliedVfsToolKind::Glob,
    "List files matching a glob through the applied AgentRun VFS surface.",
    RuntimeToolPermission::VfsRead,
    RuntimeToolEffect::ReadOnly,
    object_schema(&["pattern"], &["pattern", "path"])
);
vfs_executor!(
    FsGrepRuntimeTool,
    "fs_grep",
    AppliedVfsToolKind::Grep,
    "Search file contents through the applied AgentRun VFS surface.",
    RuntimeToolPermission::VfsRead,
    RuntimeToolEffect::ReadOnly,
    object_schema(
        &["pattern"],
        &[
            "pattern",
            "path",
            "glob",
            "type",
            "output_mode",
            "-i",
            "-n",
            "multiline",
            "-B",
            "-A",
            "-C",
            "context",
            "head_limit",
            "offset",
        ],
    )
);
vfs_executor!(
    FsApplyPatchRuntimeTool,
    "fs_apply_patch",
    AppliedVfsToolKind::ApplyPatch,
    "Apply a patch through the applied AgentRun VFS surface.",
    RuntimeToolPermission::VfsWrite,
    RuntimeToolEffect::VfsMutation,
    object_schema(&["patch"], &["patch"])
);
vfs_executor!(
    ShellExecRuntimeTool,
    "shell_exec",
    AppliedVfsToolKind::ShellExec,
    "Execute or continue a shell command through the applied AgentRun VFS surface.",
    RuntimeToolPermission::ProcessExecute,
    RuntimeToolEffect::LocalProcess,
    object_schema(
        &[],
        &[
            "operation",
            "cwd",
            "command",
            "timeout_secs",
            "terminal_id",
            "after_seq",
            "wait_ms",
            "max_bytes",
            "max_output_bytes",
            "data",
            "close_stdin",
            "tty",
            "cols",
            "rows",
        ],
    )
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
            parameters_schema: object_schema(
                &[],
                &[
                    "mode",
                    "format",
                    "run_id",
                    "task_id",
                    "story_id",
                    "include_archived",
                    "owner_agent_id",
                    "assigned_agent_id",
                    "statuses",
                ],
            ),
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
            parameters_schema: object_schema(
                &[],
                &[
                    "mode",
                    "run_id",
                    "operations",
                    "snapshot",
                    "drop_missing",
                    "return_mode",
                ],
            ),
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

fn object_schema(required: &[&str], properties: &[&str]) -> Value {
    let properties = properties
        .iter()
        .map(|name| ((*name).to_owned(), json!({})))
        .collect::<serde_json::Map<_, _>>();
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false
    })
}

#[cfg(test)]
mod tests {
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
        async fn execute(&self, _: RuntimeTaskToolRequest) -> RuntimeTaskToolOutcome {
            RuntimeTaskToolOutcome::Completed {
                output: Value::Null,
            }
        }
    }

    #[test]
    fn final_runtime_catalog_defines_all_eight_platform_tools() {
        let vfs = Arc::new(AppliedVfsRuntimeToolService::new(
            Arc::new(VfsService::new(Arc::new(MountProviderRegistry::new()))),
            Arc::new(NoopTerminalRegistry),
        ));
        let task: Arc<dyn RuntimeTaskToolService> = Arc::new(NoopTaskService);
        let executors = final_runtime_tool_catalog(vfs, task);
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
            ]
        );
    }
}
