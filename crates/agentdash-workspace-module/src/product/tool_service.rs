use std::sync::Arc;

use agentdash_application::execution_authority::{
    ExecutionAuthority, ExecutionAuthorityRequest, ExecutionAuthorityResolver,
};
use agentdash_application_operation_gateway::{
    OperationGateway, OperationPrincipal, OperationResultValue, OperationSurfaceDiagnostic,
};
use agentdash_application_ports::product_runtime_tool::{
    ProductRuntimeToolKind, ProductRuntimeToolOutcome, ProductRuntimeToolRequest,
    ProductRuntimeToolService,
};
use agentdash_contracts::workspace_module::WorkspaceModuleDescriptor;
use agentdash_domain::operation::{OperationOriginRef, OperationRef, OperationScopeRef};
use agentdash_domain::workflow::LifecycleAgentRepository;
use async_trait::async_trait;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use super::{
    WorkspaceModuleActor, WorkspaceModuleProviderContext, WorkspaceModuleProviderRegistry,
};

pub fn workspace_module_runtime_tool_schema(kind: ProductRuntimeToolKind) -> Value {
    match kind {
        ProductRuntimeToolKind::WorkspaceModuleList => json!({
            "type": "object",
            "properties": {},
            "required": [],
            "additionalProperties": false
        }),
        ProductRuntimeToolKind::WorkspaceModuleDescribe => json!({
            "type": "object",
            "properties": {
                "module_id": {
                    "type": "string",
                    "description": "Stable module id returned by workspace_module_list."
                }
            },
            "required": ["module_id"],
            "additionalProperties": false
        }),
        ProductRuntimeToolKind::WorkspaceModuleOperate => json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "description": "Provider-qualified Workspace Module operation route."
                },
                "input": {"type": "object"}
            },
            "required": ["operation"],
            "additionalProperties": false
        }),
        ProductRuntimeToolKind::WorkspaceModuleInvoke => json!({
            "type": "object",
            "properties": {
                "module_id": {"type": "string"},
                "operation_key": {"type": "string"},
                "input": {}
            },
            "required": ["module_id", "operation_key"],
            "additionalProperties": false
        }),
        ProductRuntimeToolKind::WorkspaceModulePresent => json!({
            "type": "object",
            "properties": {
                "module_id": {
                    "type": "string",
                    "description": "Stable module id returned by workspace_module_list."
                },
                "view_key": {
                    "type": "string",
                    "default": "preview",
                    "description": "Visible view key returned by workspace_module_describe."
                },
                "payload": {}
            },
            "required": ["module_id"],
            "additionalProperties": false
        }),
        _ => json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        }),
    }
}

#[derive(Clone)]
pub struct WorkspaceModuleRuntimeToolDeps {
    pub execution_authorities: Arc<dyn ExecutionAuthorityResolver>,
    pub lifecycle_agents: Arc<dyn LifecycleAgentRepository>,
    pub providers: WorkspaceModuleProviderRegistry,
    pub operation_gateway: Arc<OperationGateway>,
}

pub struct ApplicationWorkspaceModuleRuntimeToolService {
    pub(crate) kind: ProductRuntimeToolKind,
    pub(crate) deps: WorkspaceModuleRuntimeToolDeps,
}

impl ApplicationWorkspaceModuleRuntimeToolService {
    pub fn new(kind: ProductRuntimeToolKind, deps: WorkspaceModuleRuntimeToolDeps) -> Self {
        assert!(
            matches!(
                kind,
                ProductRuntimeToolKind::WorkspaceModuleList
                    | ProductRuntimeToolKind::WorkspaceModuleDescribe
                    | ProductRuntimeToolKind::WorkspaceModuleOperate
                    | ProductRuntimeToolKind::WorkspaceModuleInvoke
                    | ProductRuntimeToolKind::WorkspaceModulePresent
            ),
            "Workspace Module Product service only supports list, describe, operate, invoke and present"
        );
        Self { kind, deps }
    }

    pub(crate) async fn resolve_surface(
        &self,
        request: &ProductRuntimeToolRequest,
    ) -> Result<ResolvedWorkspaceModuleSurface, ProductRuntimeToolOutcome> {
        let target = agentdash_domain::agent_run_target::AgentRunTarget {
            run_id: request.context.target.run_id,
            agent_id: request.context.target.agent_id,
        };
        let agent_run_surface = self
            .deps
            .execution_authorities
            .resolve(ExecutionAuthorityRequest::for_target_and_runtime_thread(
                target.clone(),
                request.context.runtime_thread_id.clone(),
            ))
            .await
            .map_err(|error| failed(error.code(), error.to_string()))?;
        if request.context.target.project_id.is_nil() {
            return Err(rejected(
                "workspace_module_project_missing",
                "authorized Product target has no project identity",
            ));
        }

        if agent_run_surface.project_id() != request.context.target.project_id {
            return Err(rejected(
                "workspace_module_project_mismatch",
                "applied resource surface project does not match the authorized Product target",
            ));
        }
        let lifecycle_agent = self
            .deps
            .lifecycle_agents
            .get(request.context.target.agent_id)
            .await
            .map_err(|error| failed("workspace_module_agent_query_failed", error.to_string()))?
            .ok_or_else(|| {
                rejected(
                    "workspace_module_agent_not_found",
                    "authorized Product Lifecycle Agent does not exist",
                )
            })?;
        if lifecycle_agent.run_id != request.context.target.run_id
            || lifecycle_agent.project_id != request.context.target.project_id
        {
            return Err(rejected(
                "workspace_module_agent_target_mismatch",
                "Product Lifecycle Agent does not match the authorized target",
            ));
        }

        let principal = OperationPrincipal::server_resolved(
            agentdash_domain::operation::OperationPrincipalRef::AgentRunAgent {
                run_id: target.run_id,
                agent_id: target.agent_id,
            },
        );
        let scope = OperationScopeRef::Project {
            project_id: agent_run_surface.project_id(),
        };
        let mut provider_context = WorkspaceModuleProviderContext {
            project_id: request.context.target.project_id,
            actor: WorkspaceModuleActor::AgentRunAgent {
                user_id: lifecycle_agent.created_by_user_id,
                target,
            },
            invocation_id: request.context.invocation_id.clone(),
            visibility: agent_run_surface
                .capability_state()
                .workspace_module
                .clone(),
            operations: Vec::new(),
        };
        let mut operation_authority = agent_run_surface.operation_authority_grant();
        operation_authority.capabilities.extend(
            self.deps
                .providers
                .operation_capabilities(&provider_context)
                .await?,
        );
        let operation_surface = self
            .deps
            .operation_gateway
            .surface_authorized(
                &principal,
                &scope,
                &OperationOriginRef::AgentTool,
                operation_authority,
                CancellationToken::new(),
            )
            .await
            .map_err(|error| {
                failed(
                    "workspace_module_operation_surface_failed",
                    error.to_string(),
                )
            })?;
        reject_required_provider_failures(&agent_run_surface, &operation_surface.diagnostics)?;
        provider_context.operations = operation_surface
            .catalog
            .descriptors()
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        let modules = self.deps.providers.modules(&provider_context).await?;

        Ok(ResolvedWorkspaceModuleSurface {
            modules,
            principal,
            scope,
            diagnostics: operation_surface.diagnostics,
            provider_context,
        })
    }
}

#[async_trait]
impl ProductRuntimeToolService for ApplicationWorkspaceModuleRuntimeToolService {
    fn kind(&self) -> ProductRuntimeToolKind {
        self.kind
    }

    fn parameters_schema(&self) -> Value {
        workspace_module_runtime_tool_schema(self.kind)
    }

    async fn execute(&self, request: ProductRuntimeToolRequest) -> ProductRuntimeToolOutcome {
        match self.kind {
            ProductRuntimeToolKind::WorkspaceModuleList => self.execute_list(request).await,
            ProductRuntimeToolKind::WorkspaceModuleDescribe => self.execute_describe(request).await,
            ProductRuntimeToolKind::WorkspaceModuleOperate => self.execute_operate(request).await,
            ProductRuntimeToolKind::WorkspaceModuleInvoke => self.execute_invoke(request).await,
            ProductRuntimeToolKind::WorkspaceModulePresent => self.execute_present(request).await,
            _ => failed(
                "workspace_module_tool_kind_invalid",
                "unsupported Workspace Module Product tool kind",
            ),
        }
    }
}

pub(crate) struct ResolvedWorkspaceModuleSurface {
    pub(crate) modules: Vec<WorkspaceModuleDescriptor>,
    pub(crate) principal: OperationPrincipal,
    pub(crate) scope: OperationScopeRef,
    pub(crate) diagnostics: Vec<OperationSurfaceDiagnostic>,
    pub(crate) provider_context: WorkspaceModuleProviderContext,
}

pub(crate) fn completed(output: Value) -> ProductRuntimeToolOutcome {
    ProductRuntimeToolOutcome::Completed { output }
}

pub(crate) fn compact_module_descriptor(module: WorkspaceModuleDescriptor) -> Value {
    let builtin = module.summary.module_id.starts_with("builtin:");
    json!({
        "summary": module.summary,
        "ui_entries": module.ui_entries,
        "operations": module.operations.into_iter().map(|operation| {
            let input_contract = if builtin {
                json!({
                    "same_as_agent_tool": operation.operation_key,
                    "note": "Builtin Operation input is identical to the corresponding Agent tool parameters; use the Agent tool schema already visible in this session."
                })
            } else {
                json!({ "input_schema": operation.input_schema })
            };
            let mut value = json!({
                "operation_ref": operation.operation_ref,
                "operation_key": operation.operation_key,
                "description": operation.description,
                "effect": operation.effect,
                "replay_policy": operation.replay_policy,
                "readiness": operation.readiness,
            });
            if let Some(map) = value.as_object_mut()
                && let Some(input_map) = input_contract.as_object()
            {
                for (key, value) in input_map {
                    map.insert(key.clone(), value.clone());
                }
            }
            value
        }).collect::<Vec<_>>(),
    })
}

pub(crate) fn compact_operation_result(
    result: agentdash_application_operation_gateway::OperationExecutionResult,
) -> Value {
    let value = match result.value {
        OperationResultValue::Inline { value } => value,
        OperationResultValue::Ref { result_ref } => json!({ "result_ref": result_ref }),
    };
    json!({
        "value": value,
        "operation": operation_ref_label(&result.operation_ref),
        "output_bytes": result.output_bytes,
    })
}

pub(crate) fn compact_diagnostics(diagnostics: Vec<OperationSurfaceDiagnostic>) -> Value {
    if diagnostics.is_empty() {
        json!([])
    } else {
        json!(diagnostics)
    }
}

fn operation_ref_label(operation_ref: &OperationRef) -> String {
    format!(
        "{}:{}:{}:v{}",
        operation_ref.provider.namespace,
        operation_ref.provider.provider_key,
        operation_ref.operation_key,
        operation_ref.contract_version
    )
}

pub(crate) fn rejected(
    code: impl Into<String>,
    message: impl Into<String>,
) -> ProductRuntimeToolOutcome {
    ProductRuntimeToolOutcome::Rejected {
        code: code.into(),
        message: message.into(),
    }
}

pub(crate) fn failed(
    code: impl Into<String>,
    message: impl Into<String>,
) -> ProductRuntimeToolOutcome {
    ProductRuntimeToolOutcome::Failed {
        code: code.into(),
        message: message.into(),
    }
}

fn reject_required_provider_failures(
    surface: &ExecutionAuthority,
    diagnostics: &[OperationSurfaceDiagnostic],
) -> Result<(), ProductRuntimeToolOutcome> {
    let capabilities = surface.operation_authority_grant().capabilities;
    let native_operations_required = ["file_read", "file_write", "shell_execute", "task"]
        .iter()
        .any(|capability| capabilities.contains(*capability));
    if native_operations_required
        && let Some(diagnostic) = diagnostics
            .iter()
            .find(|diagnostic| diagnostic.provider == "platform_tool")
    {
        return Err(failed(
            "workspace_module_platform_operation_surface_unavailable",
            format!("{}: {}", diagnostic.code, diagnostic.message),
        ));
    }
    Ok(())
}
