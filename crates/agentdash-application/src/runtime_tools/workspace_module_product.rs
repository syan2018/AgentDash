use std::sync::Arc;

use crate::execution_authority::{
    ExecutionAuthority, ExecutionAuthorityRequest, ExecutionAuthorityResolver,
};
use crate::extension_runtime::extension_runtime_projection_from_installations;
use crate::workspace_module::{
    build_interaction_runtime_module, build_workspace_module_presentation, build_workspace_modules,
    project_workspace_module_visibility,
};
use agentdash_application_operation_gateway::{
    OperationGateway, OperationInvocationCommand, OperationPrincipal, OperationResultValue,
    OperationSurfaceDiagnostic, OperationTraceContext,
};
use agentdash_application_ports::product_runtime_tool::{
    ProductRuntimeToolKind, ProductRuntimeToolOutcome, ProductRuntimeToolRequest,
    ProductRuntimeToolService,
};
use agentdash_contracts::workspace_module::{
    WorkspaceModuleDescriptor, WorkspaceModuleKind, WorkspaceModuleOperationRef,
};
use agentdash_domain::interaction::{
    AttachmentCapabilityProjection, AttachmentSubject, InteractionAttachment,
    InteractionAttachmentRole, InteractionDefinitionRepository, InteractionDefinitionRevision,
    InteractionDefinitionStatus, InteractionError, InteractionInstance,
    InteractionInstanceRepository, InteractionInstanceStatus, InteractionOwner,
    InteractionRetention,
};
use agentdash_domain::operation::{OperationOriginRef, OperationRef, OperationScopeRef};
use agentdash_domain::shared_library::ProjectExtensionInstallationRepository;
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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkspaceModulePresentArguments {
    module_id: String,
    #[serde(default = "default_view_key")]
    view_key: String,
    #[serde(default)]
    payload: Option<Value>,
}

fn default_view_key() -> String {
    "preview".to_owned()
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
    pub installations: Arc<dyn ProjectExtensionInstallationRepository>,
    pub definitions: Arc<dyn InteractionDefinitionRepository>,
    pub instances: Arc<dyn InteractionInstanceRepository>,
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
                    | ProductRuntimeToolKind::WorkspaceModulePresent
            ),
            "Workspace Module Product service only supports list, describe, invoke and present"
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

        let principal = OperationPrincipal::server_resolved(
            agentdash_domain::operation::OperationPrincipalRef::AgentRunAgent {
                run_id: target.run_id,
                agent_id: target.agent_id,
            },
        );
        let scope = OperationScopeRef::Project {
            project_id: agent_run_surface.project_id(),
        };
        let installations = self
            .deps
            .installations
            .list_enabled_by_project(agent_run_surface.project_id())
            .await
            .map_err(|error| {
                failed(
                    "workspace_module_installation_query_failed",
                    error.to_string(),
                )
            })?;
        let mut operation_authority = agent_run_surface.operation_authority_grant();
        for installation in &installations {
            if agent_run_surface
                .capability_state()
                .workspace_module
                .allows(&format!("ext:{}", installation.extension_key))
            {
                operation_authority
                    .capabilities
                    .insert(format!("extension:{}", installation.extension_key));
            }
        }
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
        let operations = operation_surface
            .catalog
            .descriptors()
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();

        let extensions =
            extension_runtime_projection_from_installations(installations).map_err(|error| {
                failed(
                    "workspace_module_extension_projection_failed",
                    error.to_string(),
                )
            })?;
        let definitions = self
            .project_definitions(agent_run_surface.project_id())
            .await?;
        let mut modules = build_workspace_modules(&extensions, &definitions, &operations);
        let runtime_modules = self
            .project_interaction_runtime_modules(
                &target,
                request.context.target.project_id,
                &operations,
            )
            .await?;
        let runtime_module_sources = runtime_modules
            .iter()
            .filter_map(|module| {
                module.agent_state_projection.as_ref().map(|projection| {
                    (
                        module.summary.module_id.clone(),
                        format!("canvas:{}", projection.definition_id),
                    )
                })
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        modules.extend(runtime_modules);
        let modules = project_workspace_module_visibility(
            modules,
            &agent_run_surface.capability_state().workspace_module,
            &runtime_module_sources,
        );

        Ok(ResolvedWorkspaceModuleSurface {
            modules,
            definitions,
            principal,
            scope,
            diagnostics: operation_surface.diagnostics,
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

    async fn project_interaction_runtime_modules(
        &self,
        target: &agentdash_domain::agent_run_target::AgentRunTarget,
        project_id: uuid::Uuid,
        operations: &[agentdash_application_operation_gateway::OperationDescriptor],
    ) -> Result<Vec<WorkspaceModuleDescriptor>, ProductRuntimeToolOutcome> {
        let owner = InteractionOwner::Project(project_id);
        let instances = self
            .deps
            .instances
            .list_by_owner(&owner)
            .await
            .map_err(|error| failed("workspace_module_instance_query_failed", error.to_string()))?;
        let subject = AttachmentSubject::AgentRun {
            run_id: target.run_id,
            agent_id: target.agent_id,
        };
        let mut modules = Vec::new();
        for instance in instances
            .into_iter()
            .filter(|instance| instance.status == InteractionInstanceStatus::Open)
        {
            let attached = self
                .deps
                .instances
                .list_attachments(instance.id)
                .await
                .map_err(|error| {
                    failed(
                        "workspace_module_attachment_query_failed",
                        error.to_string(),
                    )
                })?
                .into_iter()
                .any(|attachment| {
                    attachment.detached_at.is_none() && attachment.subject == subject
                });
            if !attached {
                continue;
            }
            let revision = self
                .deps
                .definitions
                .get_revision(instance.definition_revision_id)
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
                        "Interaction instance pinned definition revision is missing",
                    )
                })?;
            if revision.definition_id != instance.definition_id || revision.project_id != project_id
            {
                return Err(rejected(
                    "workspace_module_interaction_identity_mismatch",
                    "Interaction instance does not belong to the authorized Product surface",
                ));
            }
            modules.push(
                build_interaction_runtime_module(&instance, &revision, operations).map_err(
                    |error| {
                        failed(
                            "workspace_module_agent_projection_failed",
                            error.to_string(),
                        )
                    },
                )?,
            );
        }
        Ok(modules)
    }

    async fn execute_list(&self, request: ProductRuntimeToolRequest) -> ProductRuntimeToolOutcome {
        let surface = match self.resolve_surface(&request).await {
            Ok(surface) => surface,
            Err(outcome) => return outcome,
        };
        completed(json!({
            "module_count": surface.modules.len(),
            "surface_readiness": if surface.diagnostics.is_empty() { "ready" } else { "degraded" },
            "surface_diagnostics": compact_diagnostics(surface.diagnostics),
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
        completed(json!({
            "module": compact_module_descriptor(module),
            "surface_readiness": if surface.diagnostics.is_empty() { "ready" } else { "degraded" },
            "surface_diagnostics": compact_diagnostics(surface.diagnostics),
        }))
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
            Ok(result) => completed(compact_operation_result(result)),
            Err(error) => failed("workspace_module_operation_failed", error.to_string()),
        }
    }

    async fn execute_present(
        &self,
        request: ProductRuntimeToolRequest,
    ) -> ProductRuntimeToolOutcome {
        let arguments: WorkspaceModulePresentArguments =
            match serde_json::from_value(request.arguments.clone()) {
                Ok(arguments) => arguments,
                Err(error) => {
                    return rejected(
                        "workspace_module_invalid_arguments",
                        format!("invalid workspace_module_present arguments: {error}"),
                    );
                }
            };
        let surface = match self.resolve_surface(&request).await {
            Ok(surface) => surface,
            Err(outcome) => return outcome,
        };
        let Some(module) = surface
            .modules
            .iter()
            .find(|module| module.summary.module_id == arguments.module_id)
        else {
            return rejected(
                "workspace_module_not_found",
                format!("workspace module is not visible: {}", arguments.module_id),
            );
        };

        let mut diagnostics = None;
        let mut interaction_instance_id = None;
        if let Some(definition_id) = arguments
            .module_id
            .strip_prefix("canvas:")
            .and_then(|value| uuid::Uuid::parse_str(value).ok())
        {
            let Some(definition) = surface
                .definitions
                .iter()
                .find(|revision| revision.definition_id == definition_id)
            else {
                return rejected(
                    "workspace_module_definition_not_visible",
                    "Canvas definition revision is not visible in the current actor surface",
                );
            };
            let target = match self.attach_canvas_presentation(&request, definition).await {
                Ok(target) => target,
                Err(outcome) => return outcome,
            };
            interaction_instance_id = Some(target.instance_id);
            diagnostics = Some(json!({
                "definition_uri": format!("canvas://{definition_id}"),
                "instance_id": target.instance_id,
                "attachment_id": target.attachment_id
            }));
        }
        let mut presentation = match build_workspace_module_presentation(
            module,
            arguments.view_key.trim(),
            arguments.payload,
            diagnostics,
        ) {
            Ok(presentation) => presentation,
            Err(error) => {
                return rejected("workspace_module_presentation_invalid", error.to_string());
            }
        };
        if let Some(instance_id) = interaction_instance_id {
            presentation.presentation_uri = format!("interaction://{instance_id}");
        }
        completed(json!({
            "content": [{
                "type": "text",
                "text": format!(
                    "Workspace Module `{}` presentation requested",
                    presentation.title
                )
            }],
            "is_error": false,
            "details": {
                "workspace_module_presentation": presentation
            }
        }))
    }

    async fn attach_canvas_presentation(
        &self,
        request: &ProductRuntimeToolRequest,
        revision: &InteractionDefinitionRevision,
    ) -> Result<WorkspaceModulePresentationTarget, ProductRuntimeToolOutcome> {
        let definition = self
            .deps
            .definitions
            .get(revision.definition_id)
            .await
            .map_err(|error| {
                failed(
                    "workspace_module_definition_query_failed",
                    error.to_string(),
                )
            })?
            .ok_or_else(|| {
                rejected(
                    "workspace_module_definition_not_found",
                    "Canvas InteractionDefinition does not exist",
                )
            })?;
        if definition.current_revision_id != revision.revision_id
            || definition.project_id != request.context.target.project_id
        {
            return Err(rejected(
                "workspace_module_definition_revision_mismatch",
                "Canvas definition revision does not belong to the authorized Product surface",
            ));
        }
        let owner = InteractionOwner::Project(request.context.target.project_id);
        let instance = match self
            .deps
            .instances
            .list_by_owner(&owner)
            .await
            .map_err(|error| failed("workspace_module_instance_query_failed", error.to_string()))?
            .into_iter()
            .find(|instance| {
                instance.status == InteractionInstanceStatus::Open
                    && instance.definition_revision_id == revision.revision_id
            }) {
            Some(instance) => instance,
            None => {
                let instance = InteractionInstance::new_v1(
                    owner,
                    revision.definition_id,
                    revision.revision_id,
                    revision.initial_state.clone(),
                    InteractionRetention { retain_until: None },
                )
                .map_err(|error| failed("workspace_module_instance_invalid", error.to_string()))?;
                self.deps
                    .instances
                    .create(&instance)
                    .await
                    .map_err(|error| {
                        failed("workspace_module_instance_create_failed", error.to_string())
                    })?;
                instance
            }
        };
        let subject = AttachmentSubject::AgentRun {
            run_id: request.context.target.run_id,
            agent_id: request.context.target.agent_id,
        };
        let existing = self
            .deps
            .instances
            .list_attachments(instance.id)
            .await
            .map_err(|error| {
                failed(
                    "workspace_module_attachment_query_failed",
                    error.to_string(),
                )
            })?
            .into_iter()
            .find(|attachment| attachment.detached_at.is_none() && attachment.subject == subject);
        let attachment_id = if let Some(existing) = existing {
            existing.id
        } else {
            let attachment = InteractionAttachment {
                id: uuid::Uuid::new_v4(),
                instance_id: instance.id,
                subject,
                role: InteractionAttachmentRole::Renderer,
                capabilities: AttachmentCapabilityProjection::for_role(
                    InteractionAttachmentRole::Renderer,
                ),
                created_at: chrono::Utc::now(),
                detached_at: None,
            };
            attachment.validate().map_err(|error| {
                failed("workspace_module_attachment_invalid", error.to_string())
            })?;
            self.deps
                .instances
                .attach(&attachment)
                .await
                .map_err(|error| match error {
                    InteractionError::PersistenceConflict { .. } => rejected(
                        "workspace_module_attachment_conflict",
                        "AgentRun already has an active attachment for this Interaction",
                    ),
                    _ => failed("workspace_module_attachment_failed", error.to_string()),
                })?;
            attachment.id
        };
        Ok(WorkspaceModulePresentationTarget {
            instance_id: instance.id,
            attachment_id,
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
            ProductRuntimeToolKind::WorkspaceModuleInvoke => self.execute_invoke(request).await,
            ProductRuntimeToolKind::WorkspaceModulePresent => self.execute_present(request).await,
            _ => failed(
                "workspace_module_tool_kind_invalid",
                "unsupported Workspace Module Product tool kind",
            ),
        }
    }
}

struct ResolvedWorkspaceModuleSurface {
    modules: Vec<WorkspaceModuleDescriptor>,
    definitions: Vec<InteractionDefinitionRevision>,
    principal: OperationPrincipal,
    scope: OperationScopeRef,
    diagnostics: Vec<OperationSurfaceDiagnostic>,
}

struct WorkspaceModulePresentationTarget {
    instance_id: uuid::Uuid,
    attachment_id: uuid::Uuid,
}

fn completed(output: Value) -> ProductRuntimeToolOutcome {
    ProductRuntimeToolOutcome::Completed { output }
}

fn compact_module_descriptor(module: WorkspaceModuleDescriptor) -> Value {
    let builtin = module.summary.kind == WorkspaceModuleKind::Builtin;
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
            if let Some(map) = value.as_object_mut() {
                if let Some(input_map) = input_contract.as_object() {
                    for (key, value) in input_map {
                        map.insert(key.clone(), value.clone());
                    }
                }
            }
            value
        }).collect::<Vec<_>>(),
    })
}

fn compact_operation_result(
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

fn compact_diagnostics(diagnostics: Vec<OperationSurfaceDiagnostic>) -> Value {
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
