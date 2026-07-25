use std::sync::Arc;

use agentdash_application_agentrun::agent_run::{
    AgentFrameSurfaceExt, AgentRunAppliedResourceSurfaceQueryPort,
    AgentRunProductRuntimeBindingRepository,
};
use agentdash_application_operation_gateway::{
    OperationGateway, OperationInvocationCommand, OperationPrincipal, OperationTraceContext,
};
use agentdash_application_ports::product_runtime_tool::{
    ProductRuntimeToolKind, ProductRuntimeToolOutcome, ProductRuntimeToolRequest,
    ProductRuntimeToolService,
};
use agentdash_contracts::workspace_module::{
    WorkspaceModuleDescriptor, WorkspaceModuleOperationRef,
};
use agentdash_domain::interaction::{
    InteractionDefinitionRepository, InteractionDefinitionRevision, InteractionDefinitionStatus,
    InteractionOwner,
};
use agentdash_domain::operation::{OperationOriginRef, OperationRef, OperationScopeRef};
use agentdash_domain::shared_library::ProjectExtensionInstallationRepository;
use agentdash_domain::workflow::AgentFrameRepository;
use agentdash_workspace_module::extension_runtime::extension_runtime_projection_from_installations;
use agentdash_workspace_module::workspace_module::build_workspace_modules;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkspaceModuleDescribeArguments {
    module_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkspaceModuleInvokeArguments {
    operation_ref: WorkspaceModuleOperationRef,
    #[serde(default)]
    input: Value,
}

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
        ProductRuntimeToolKind::WorkspaceModuleInvoke => json!({
            "type": "object",
            "properties": {
                "operation_ref": {
                    "type": "object",
                    "properties": {
                        "namespace": {"type": "string"},
                        "provider_key": {"type": "string"},
                        "operation_key": {"type": "string"},
                        "contract_version": {"type": "integer", "minimum": 1}
                    },
                    "required": [
                        "namespace",
                        "provider_key",
                        "operation_key",
                        "contract_version"
                    ],
                    "additionalProperties": false
                },
                "input": {}
            },
            "required": ["operation_ref"],
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
    pub runtime_bindings: Arc<dyn AgentRunProductRuntimeBindingRepository>,
    pub applied_surfaces: Arc<dyn AgentRunAppliedResourceSurfaceQueryPort>,
    pub frames: Arc<dyn AgentFrameRepository>,
    pub installations: Arc<dyn ProjectExtensionInstallationRepository>,
    pub definitions: Arc<dyn InteractionDefinitionRepository>,
    pub operation_gateway: Arc<OperationGateway>,
}

pub struct ApplicationWorkspaceModuleRuntimeToolService {
    kind: ProductRuntimeToolKind,
    deps: WorkspaceModuleRuntimeToolDeps,
}

impl ApplicationWorkspaceModuleRuntimeToolService {
    pub fn new(kind: ProductRuntimeToolKind, deps: WorkspaceModuleRuntimeToolDeps) -> Self {
        assert!(
            matches!(
                kind,
                ProductRuntimeToolKind::WorkspaceModuleList
                    | ProductRuntimeToolKind::WorkspaceModuleDescribe
                    | ProductRuntimeToolKind::WorkspaceModuleInvoke
            ),
            "Workspace Module Product service only supports list, describe and invoke"
        );
        Self { kind, deps }
    }

    async fn resolve_surface(
        &self,
        request: &ProductRuntimeToolRequest,
    ) -> Result<ResolvedWorkspaceModuleSurface, ProductRuntimeToolOutcome> {
        let target = agentdash_domain::agent_run_target::AgentRunTarget {
            run_id: request.context.target.run_id,
            agent_id: request.context.target.agent_id,
        };
        let binding = self
            .deps
            .runtime_bindings
            .load_product_binding_by_runtime_thread(&request.context.runtime_thread_id)
            .await
            .map_err(|message| failed("workspace_module_binding_query_failed", message))?
            .ok_or_else(|| {
                rejected(
                    "workspace_module_runtime_thread_unbound",
                    "RuntimeThread has no durable Product binding",
                )
            })?;
        if binding.target != target {
            return Err(rejected(
                "workspace_module_product_target_mismatch",
                "RuntimeThread Product binding does not match the authorized tool target",
            ));
        }
        if request.context.target.project_id.is_nil() {
            return Err(rejected(
                "workspace_module_project_missing",
                "authorized Product target has no project identity",
            ));
        }

        let surface = self
            .deps
            .applied_surfaces
            .applied_resource_surface(&target)
            .await
            .map_err(|error| {
                failed(
                    "workspace_module_applied_surface_query_failed",
                    error.to_string(),
                )
            })?;
        surface.validate_for(&target).map_err(|error| {
            rejected(
                "workspace_module_applied_surface_invalid",
                error.to_string(),
            )
        })?;
        if surface.project_id != request.context.target.project_id {
            return Err(rejected(
                "workspace_module_project_mismatch",
                "applied resource surface project does not match the authorized Product target",
            ));
        }

        let frame = self
            .deps
            .frames
            .get(binding.launch_frame.frame_id)
            .await
            .map_err(|error| failed("workspace_module_frame_query_failed", error.to_string()))?
            .ok_or_else(|| {
                failed(
                    "workspace_module_frame_missing",
                    format!(
                        "Product binding AgentFrame {} does not exist",
                        binding.launch_frame.frame_id
                    ),
                )
            })?;
        if frame.agent_id != target.agent_id
            || u64::try_from(frame.revision).ok() != Some(binding.launch_frame.revision)
        {
            return Err(rejected(
                "workspace_module_frame_binding_mismatch",
                "Product binding does not identify the immutable AgentFrame revision",
            ));
        }
        let capability = frame.typed_capability_state().ok_or_else(|| {
            failed(
                "workspace_module_capability_surface_missing",
                "bound AgentFrame has no typed capability surface",
            )
        })?;

        let principal = OperationPrincipal::server_resolved(
            agentdash_domain::operation::OperationPrincipalRef::AgentRunAgent {
                run_id: target.run_id,
                agent_id: target.agent_id,
            },
        );
        let scope = OperationScopeRef::Project {
            project_id: surface.project_id,
        };
        let operation_surface = self
            .deps
            .operation_gateway
            .surface_current(
                &principal,
                &scope,
                &OperationOriginRef::AgentTool,
                CancellationToken::new(),
            )
            .await
            .map_err(|error| {
                failed(
                    "workspace_module_operation_surface_failed",
                    error.to_string(),
                )
            })?;
        let operations = operation_surface
            .catalog
            .descriptors()
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();

        let installations = self
            .deps
            .installations
            .list_enabled_by_project(surface.project_id)
            .await
            .map_err(|error| {
                failed(
                    "workspace_module_installation_query_failed",
                    error.to_string(),
                )
            })?;
        let extensions =
            extension_runtime_projection_from_installations(installations).map_err(|error| {
                failed(
                    "workspace_module_extension_projection_failed",
                    error.to_string(),
                )
            })?;
        let definitions = self.project_definitions(surface.project_id).await?;
        let modules = build_workspace_modules(&extensions, &definitions, &operations)
            .into_iter()
            .filter(|module| {
                capability
                    .workspace_module
                    .allows(&module.summary.module_id)
            })
            .collect();

        Ok(ResolvedWorkspaceModuleSurface {
            modules,
            principal,
            scope,
        })
    }

    async fn project_definitions(
        &self,
        project_id: uuid::Uuid,
    ) -> Result<Vec<InteractionDefinitionRevision>, ProductRuntimeToolOutcome> {
        let definitions = self
            .deps
            .definitions
            .list_canvas_by_project(project_id)
            .await
            .map_err(|error| {
                failed(
                    "workspace_module_interaction_query_failed",
                    error.to_string(),
                )
            })?;
        let mut revisions = Vec::new();
        for definition in definitions {
            if definition.status != InteractionDefinitionStatus::Active
                || !matches!(definition.owner, InteractionOwner::Project(owner) if owner == project_id)
            {
                continue;
            }
            let revision = self
                .deps
                .definitions
                .get_revision(definition.current_revision_id)
                .await
                .map_err(|error| {
                    failed(
                        "workspace_module_interaction_query_failed",
                        error.to_string(),
                    )
                })?
                .ok_or_else(|| {
                    failed(
                        "workspace_module_interaction_revision_missing",
                        "InteractionDefinition current revision is missing",
                    )
                })?;
            revisions.push(revision);
        }
        Ok(revisions)
    }

    async fn execute_list(&self, request: ProductRuntimeToolRequest) -> ProductRuntimeToolOutcome {
        let surface = match self.resolve_surface(&request).await {
            Ok(surface) => surface,
            Err(outcome) => return outcome,
        };
        completed(json!({
            "module_count": surface.modules.len(),
            "modules": surface
                .modules
                .into_iter()
                .map(|module| module.summary)
                .collect::<Vec<_>>(),
        }))
    }

    async fn execute_describe(
        &self,
        request: ProductRuntimeToolRequest,
    ) -> ProductRuntimeToolOutcome {
        let arguments: WorkspaceModuleDescribeArguments =
            match serde_json::from_value(request.arguments.clone()) {
                Ok(arguments) => arguments,
                Err(error) => {
                    return rejected(
                        "workspace_module_invalid_arguments",
                        format!("invalid workspace_module_describe arguments: {error}"),
                    );
                }
            };
        let surface = match self.resolve_surface(&request).await {
            Ok(surface) => surface,
            Err(outcome) => return outcome,
        };
        let Some(module) = surface
            .modules
            .into_iter()
            .find(|module| module.summary.module_id == arguments.module_id)
        else {
            return rejected(
                "workspace_module_not_found",
                format!("workspace module is not visible: {}", arguments.module_id),
            );
        };
        completed(json!({ "module": module }))
    }

    async fn execute_invoke(
        &self,
        request: ProductRuntimeToolRequest,
    ) -> ProductRuntimeToolOutcome {
        let arguments: WorkspaceModuleInvokeArguments =
            match serde_json::from_value(request.arguments.clone()) {
                Ok(arguments) => arguments,
                Err(error) => {
                    return rejected(
                        "workspace_module_invalid_arguments",
                        format!("invalid workspace_module_invoke arguments: {error}"),
                    );
                }
            };
        let surface = match self.resolve_surface(&request).await {
            Ok(surface) => surface,
            Err(outcome) => return outcome,
        };
        let exact = surface
            .modules
            .iter()
            .flat_map(|module| &module.operations)
            .any(|operation| operation.operation_ref == arguments.operation_ref);
        if !exact {
            return rejected(
                "workspace_module_operation_not_visible",
                "OperationRef is not present in the current actor surface",
            );
        }
        let operation_ref = match OperationRef::new(
            arguments.operation_ref.namespace,
            arguments.operation_ref.provider_key,
            arguments.operation_ref.operation_key,
            arguments.operation_ref.contract_version,
        ) {
            Ok(operation_ref) => operation_ref,
            Err(error) => {
                return rejected("workspace_module_invalid_operation_ref", error.to_string());
            }
        };
        match self
            .deps
            .operation_gateway
            .invoke(
                OperationInvocationCommand {
                    operation_ref,
                    input: arguments.input,
                    principal: surface.principal,
                    scope_ref: surface.scope,
                    origin: OperationOriginRef::AgentTool,
                    trace: OperationTraceContext::root(),
                    deadline: chrono::Utc::now() + chrono::Duration::seconds(30),
                    idempotency_key: Some(request.context.invocation_id),
                    attachment_ref: None,
                },
                CancellationToken::new(),
            )
            .await
        {
            Ok(result) => match serde_json::to_value(result) {
                Ok(value) => completed(value),
                Err(error) => failed(
                    "workspace_module_result_serialization_failed",
                    error.to_string(),
                ),
            },
            Err(error) => failed("workspace_module_operation_failed", error.to_string()),
        }
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
            ProductRuntimeToolKind::WorkspaceModuleInvoke => self.execute_invoke(request).await,
            _ => failed(
                "workspace_module_tool_kind_invalid",
                "unsupported Workspace Module Product tool kind",
            ),
        }
    }
}

struct ResolvedWorkspaceModuleSurface {
    modules: Vec<WorkspaceModuleDescriptor>,
    principal: OperationPrincipal,
    scope: OperationScopeRef,
}

fn completed(output: Value) -> ProductRuntimeToolOutcome {
    ProductRuntimeToolOutcome::Completed { output }
}

fn rejected(code: impl Into<String>, message: impl Into<String>) -> ProductRuntimeToolOutcome {
    ProductRuntimeToolOutcome::Rejected {
        code: code.into(),
        message: message.into(),
    }
}

fn failed(code: impl Into<String>, message: impl Into<String>) -> ProductRuntimeToolOutcome {
    ProductRuntimeToolOutcome::Failed {
        code: code.into(),
        message: message.into(),
    }
}
