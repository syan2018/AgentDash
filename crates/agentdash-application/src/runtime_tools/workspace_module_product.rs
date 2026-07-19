use std::collections::BTreeMap;
use std::sync::Arc;

use agentdash_application_agentrun::agent_run::{
    AgentFrameSurfaceExt, AgentRunAppliedResourceSurface, AgentRunAppliedResourceSurfaceQueryPort,
    AgentRunProductRuntimeBindingRepository,
};
use agentdash_application_extension_gateway::{
    ExtensionGateway, ExtensionInvocationWorkspaceContext,
    ExtensionRuntimeBackendServiceInvokeRequest, ExtensionRuntimeBackendServiceInvoker,
    ExtensionRuntimeChannelConsumer, ExtensionRuntimeChannelInvokeRequest,
    ExtensionRuntimeChannelInvoker, RuntimeActionKey, RuntimeActor, RuntimeContext,
    RuntimeInvocationRequest, RuntimeTarget, RuntimeTrace, attach_extension_invocation_workspace,
};
use agentdash_application_ports::product_runtime_tool::{
    ProductRuntimeToolKind, ProductRuntimeToolOutcome, ProductRuntimeToolRequest,
    ProductRuntimeToolService,
};
use agentdash_contracts::workspace_module::{
    WorkspaceModuleCanvasHostAction, WorkspaceModuleDescriptor, WorkspaceModuleKind,
    WorkspaceModuleOperation, WorkspaceModuleOperationDispatch, WorkspaceModuleOperationVisibility,
};
use agentdash_domain::canvas::{CanvasRepository, CanvasRuntimeStateRepository};
use agentdash_domain::shared_library::ProjectExtensionInstallationRepository;
use agentdash_domain::workflow::AgentFrameRepository;
use agentdash_workspace_module::workspace_module::{
    WorkspaceModuleOperationContext, WorkspaceModuleVisibilityInput,
    WorkspaceModuleVisibilityProjection, project_agent_run_workspace_module_visibility,
    resolve_workspace_module_visibility_with_operation_context, validate_input_against_schema,
};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug, Deserialize)]
struct WorkspaceModuleDescribeArguments {
    module_id: String,
}

#[derive(Debug, Deserialize)]
struct WorkspaceModuleInvokeArguments {
    module_id: String,
    operation_key: String,
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
                "module_id": {
                    "type": "string",
                    "description": "Stable module id returned by workspace_module_list."
                },
                "operation_key": {
                    "type": "string",
                    "description": "Agent-visible operation key returned by workspace_module_describe."
                },
                "input": {
                    "type": "object",
                    "description": "Operation input satisfying the operation input_schema."
                }
            },
            "required": ["module_id", "operation_key"],
            "additionalProperties": false
        }),
        ProductRuntimeToolKind::WorkspaceModuleOperate => json!({
            "type": "object",
            "properties": {
                "operation": {"type": "string"},
                "input": {"type": "object"}
            },
            "required": ["operation"],
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
    pub canvases: Arc<dyn CanvasRepository>,
    pub canvas_runtime_state: Arc<dyn CanvasRuntimeStateRepository>,
    pub extension_gateway: Arc<ExtensionGateway>,
    pub channel_invoker: Arc<ExtensionRuntimeChannelInvoker>,
    pub backend_service_invoker: Arc<ExtensionRuntimeBackendServiceInvoker>,
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
            "Workspace Module Product service only supports list, describe and invoke until the canonical surface-update command is attached"
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

        let snapshot = self
            .deps
            .applied_surfaces
            .applied_resource_surface(&target, None)
            .await
            .map_err(|error| {
                failed(
                    "workspace_module_applied_surface_query_failed",
                    error.to_string(),
                )
            })?;
        snapshot.validate_for(&target).map_err(|error| {
            rejected(
                "workspace_module_applied_surface_invalid",
                error.to_string(),
            )
        })?;
        if snapshot.surface.project_id != request.context.target.project_id {
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
        let vfs = frame.typed_vfs().ok_or_else(|| {
            failed(
                "workspace_module_vfs_surface_missing",
                "bound AgentFrame has no typed VFS surface",
            )
        })?;

        let operation_context = WorkspaceModuleOperationContext::ready(
            self.deps.extension_gateway.action_descriptors(),
        );
        let projection = resolve_workspace_module_visibility_with_operation_context(
            &self.deps.installations,
            &self.deps.canvases,
            snapshot.surface.project_id,
            WorkspaceModuleVisibilityInput {
                base_visibility: &capability.workspace_module,
                runtime_vfs: &vfs,
            },
            &operation_context,
        )
        .await
        .map_err(|message| failed("workspace_module_projection_failed", message))?;
        let projection = restrict_to_agent_surface(projection, &vfs);
        Ok(ResolvedWorkspaceModuleSurface {
            modules: projection.modules,
            diagnostics: projection
                .diagnostics
                .into_iter()
                .map(|diagnostic| {
                    json!({
                        "code": diagnostic.code,
                        "message": diagnostic.message,
                        "module_ref": diagnostic.module_ref,
                    })
                })
                .collect(),
            applied_surface: snapshot.surface,
        })
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
            "diagnostics": surface.diagnostics,
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
        let module_id = arguments.module_id.trim();
        if module_id.is_empty() {
            return rejected(
                "workspace_module_invalid_arguments",
                "module_id must not be empty",
            );
        }
        let surface = match self.resolve_surface(&request).await {
            Ok(surface) => surface,
            Err(outcome) => return outcome,
        };
        let Some(module) = surface
            .modules
            .into_iter()
            .find(|module| module.summary.module_id == module_id)
        else {
            return rejected(
                "workspace_module_not_found",
                format!("workspace module is not visible: {module_id}"),
            );
        };
        completed(json!({
            "module": module,
            "diagnostics": surface.diagnostics,
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
        let module_id = arguments.module_id.trim();
        let operation_key = arguments.operation_key.trim();
        if module_id.is_empty() || operation_key.is_empty() {
            return rejected(
                "workspace_module_invalid_arguments",
                "module_id and operation_key must not be empty",
            );
        }
        let surface = match self.resolve_surface(&request).await {
            Ok(surface) => surface,
            Err(outcome) => return outcome,
        };
        let Some(module) = surface
            .modules
            .iter()
            .find(|module| module.summary.module_id == module_id)
        else {
            return rejected(
                "workspace_module_not_found",
                format!("workspace module is not visible: {module_id}"),
            );
        };
        let Some(operation) = module
            .operations
            .iter()
            .find(|operation| operation.operation_key == operation_key)
        else {
            return rejected(
                "workspace_module_operation_not_found",
                format!("operation `{operation_key}` is not exposed by module `{module_id}`"),
            );
        };
        if operation.visibility != WorkspaceModuleOperationVisibility::AgentAndPanel {
            return rejected(
                "workspace_module_operation_not_agent_visible",
                format!("operation `{operation_key}` is not exposed to Agent runtimes"),
            );
        }
        if !operation.readiness.is_ready() {
            return rejected(
                "workspace_module_operation_not_ready",
                operation
                    .readiness
                    .reason
                    .clone()
                    .unwrap_or_else(|| format!("operation `{operation_key}` is not ready")),
            );
        }
        if let Some(schema) = operation.input_schema.as_ref()
            && let Err(message) = validate_input_against_schema(schema, &arguments.input)
        {
            return rejected("workspace_module_input_schema_mismatch", message);
        }

        let provenance = json!({
            "module_id": module.summary.module_id,
            "module_kind": module.summary.kind,
            "module_source": module.summary.source,
            "operation_key": operation.operation_key,
            "operation_origin": operation.origin,
            "runtime_backing": module.runtime_backing,
        });
        self.dispatch_operation(
            &request,
            &surface.applied_surface,
            module,
            operation,
            arguments.input,
            provenance,
        )
        .await
    }

    async fn dispatch_operation(
        &self,
        request: &ProductRuntimeToolRequest,
        applied_surface: &AgentRunAppliedResourceSurface,
        module: &WorkspaceModuleDescriptor,
        operation: &WorkspaceModuleOperation,
        input: Value,
        provenance: Value,
    ) -> ProductRuntimeToolOutcome {
        match &operation.dispatch {
            WorkspaceModuleOperationDispatch::RuntimeAction { action_key } => {
                let (backend_id, workspace) = match resolve_invocation_backend(applied_surface) {
                    Ok(value) => value,
                    Err(outcome) => return outcome,
                };
                let action_key = match RuntimeActionKey::parse(action_key.clone()) {
                    Ok(value) => value,
                    Err(error) => {
                        return failed("workspace_module_action_key_invalid", error.to_string());
                    }
                };
                let runtime_thread_id = request.context.runtime_thread_id.to_string();
                let mut invocation = RuntimeInvocationRequest::new(
                    action_key,
                    RuntimeActor::AgentRuntimeThread {
                        runtime_thread_id: runtime_thread_id.clone(),
                        agent_id: Some(request.context.target.agent_id.to_string()),
                    },
                    RuntimeContext::RuntimeThread {
                        runtime_thread_id,
                        project_id: Some(applied_surface.project_id),
                        workspace_id: applied_surface.workspace_id,
                    },
                    input,
                );
                invocation.target = Some(RuntimeTarget::Backend {
                    backend_id: backend_id.clone(),
                });
                attach_extension_invocation_workspace(&mut invocation, workspace);
                match self.deps.extension_gateway.invoke(invocation).await {
                    Ok(result) => completed(json!({
                        "dispatch": "runtime_action",
                        "backend_id": backend_id,
                        "result": result,
                        "provenance": provenance,
                    })),
                    Err(error) => {
                        failed("workspace_module_runtime_action_failed", error.to_string())
                    }
                }
            }
            WorkspaceModuleOperationDispatch::ProtocolChannel {
                channel_key,
                method_name,
            } => {
                let (backend_id, workspace) = match resolve_invocation_backend(applied_surface) {
                    Ok(value) => value,
                    Err(outcome) => return outcome,
                };
                let invocation = ExtensionRuntimeChannelInvokeRequest {
                    project_id: applied_surface.project_id,
                    runtime_thread_id: request.context.runtime_thread_id.to_string(),
                    backend_id: backend_id.clone(),
                    workspace,
                    consumer: ExtensionRuntimeChannelConsumer::RuntimeThreadUser,
                    channel_key: channel_key.clone(),
                    dependency_alias: None,
                    method: method_name.clone(),
                    input,
                    trace: RuntimeTrace::new(),
                };
                match self.deps.channel_invoker.invoke(invocation).await {
                    Ok(result) => completed(json!({
                        "dispatch": "protocol_channel",
                        "backend_id": backend_id,
                        "channel_key": result.channel_key,
                        "method": result.method,
                        "trace": result.trace,
                        "output": result.output,
                        "provenance": provenance,
                    })),
                    Err(error) => failed(
                        "workspace_module_protocol_channel_failed",
                        error.to_string(),
                    ),
                }
            }
            WorkspaceModuleOperationDispatch::BackendService { service_key, route } => {
                let (backend_id, workspace) = match resolve_invocation_backend(applied_surface) {
                    Ok(value) => value,
                    Err(outcome) => return outcome,
                };
                let body = match serde_json::to_vec(&input) {
                    Ok(body) => body,
                    Err(error) => {
                        return rejected(
                            "workspace_module_backend_input_invalid",
                            error.to_string(),
                        );
                    }
                };
                let invocation = ExtensionRuntimeBackendServiceInvokeRequest {
                    project_id: applied_surface.project_id,
                    runtime_thread_id: request.context.runtime_thread_id.to_string(),
                    backend_id: backend_id.clone(),
                    workspace,
                    extension_key: module.summary.source.clone(),
                    service_key: service_key.clone(),
                    route: route.clone(),
                    method: "POST".to_string(),
                    headers: BTreeMap::from([(
                        "content-type".to_string(),
                        "application/json; charset=utf-8".to_string(),
                    )]),
                    body: Some(body),
                    trace: RuntimeTrace::new(),
                };
                match self.deps.backend_service_invoker.invoke(invocation).await {
                    Ok(result) => completed(json!({
                        "dispatch": "backend_service",
                        "backend_id": backend_id,
                        "trace": result.trace,
                        "output": result.output,
                        "provenance": provenance,
                    })),
                    Err(error) => {
                        failed("workspace_module_backend_service_failed", error.to_string())
                    }
                }
            }
            WorkspaceModuleOperationDispatch::HostCanvas { canvas_action } => {
                self.dispatch_host_canvas(request, module, canvas_action.clone(), input, provenance)
                    .await
            }
            WorkspaceModuleOperationDispatch::Builtin { builtin_key } => rejected(
                "workspace_module_builtin_unimplemented",
                format!("builtin operation `{builtin_key}` is not implemented"),
            ),
        }
    }

    async fn dispatch_host_canvas(
        &self,
        request: &ProductRuntimeToolRequest,
        module: &WorkspaceModuleDescriptor,
        action: WorkspaceModuleCanvasHostAction,
        _input: Value,
        provenance: Value,
    ) -> ProductRuntimeToolOutcome {
        if module.summary.kind != WorkspaceModuleKind::Canvas {
            return rejected(
                "workspace_module_canvas_dispatch_mismatch",
                "Host Canvas dispatch requires a Canvas workspace module",
            );
        }
        match action {
            WorkspaceModuleCanvasHostAction::Inspect => {
                match self
                    .deps
                    .canvas_runtime_state
                    .latest_runtime_observation(
                        request.context.target.run_id,
                        request.context.target.agent_id,
                        &module.summary.source,
                    )
                    .await
                {
                    Ok(observation) => completed(json!({
                        "canvas_mount_id": module.summary.source,
                        "observation": observation,
                        "provenance": provenance,
                    })),
                    Err(error) => {
                        failed("workspace_module_canvas_inspect_failed", error.to_string())
                    }
                }
            }
            WorkspaceModuleCanvasHostAction::GetInteractionState => {
                match self
                    .deps
                    .canvas_runtime_state
                    .latest_interaction_snapshot(
                        request.context.target.run_id,
                        request.context.target.agent_id,
                        &module.summary.source,
                    )
                    .await
                {
                    Ok(snapshot) => completed(json!({
                        "canvas_mount_id": module.summary.source,
                        "snapshot": snapshot,
                        "provenance": provenance,
                    })),
                    Err(error) => failed(
                        "workspace_module_canvas_interaction_query_failed",
                        error.to_string(),
                    ),
                }
            }
            WorkspaceModuleCanvasHostAction::BindData => rejected(
                "workspace_module_canvas_surface_update_unavailable",
                "Canvas binding requires the canonical AgentFrame runtime-surface update command",
            ),
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
                "Workspace Module Product service was composed with an unsupported tool kind",
            ),
        }
    }
}

struct ResolvedWorkspaceModuleSurface {
    modules: Vec<WorkspaceModuleDescriptor>,
    diagnostics: Vec<Value>,
    applied_surface: AgentRunAppliedResourceSurface,
}

fn restrict_to_agent_surface(
    projection: WorkspaceModuleVisibilityProjection,
    vfs: &agentdash_platform_spi::Vfs,
) -> WorkspaceModuleVisibilityProjection {
    let mut projection = project_agent_run_workspace_module_visibility(
        projection.modules,
        WorkspaceModuleVisibilityInput {
            base_visibility: &projection.base_visibility,
            runtime_vfs: vfs,
        },
    );
    projection.modules.iter_mut().for_each(|module| {
        module.operations.retain(|operation| {
            operation.visibility == WorkspaceModuleOperationVisibility::AgentAndPanel
        });
        module.summary.operation_summary = module
            .operations
            .iter()
            .map(|operation| operation.operation_key.clone())
            .collect();
    });
    projection
}

fn resolve_invocation_backend(
    surface: &AgentRunAppliedResourceSurface,
) -> Result<(String, Option<ExtensionInvocationWorkspaceContext>), ProductRuntimeToolOutcome> {
    let mount = surface
        .default_mount_id
        .as_deref()
        .and_then(|mount_id| {
            surface
                .vfs_mounts
                .iter()
                .find(|mount| mount.mount_id == mount_id)
        })
        .or_else(|| {
            surface
                .vfs_mounts
                .iter()
                .find(|mount| !mount.backend_id.trim().is_empty())
        })
        .ok_or_else(|| {
            rejected(
                "workspace_module_runtime_backend_missing",
                "applied resource surface has no runtime backend mount",
            )
        })?;
    if mount.backend_id.trim().is_empty() {
        return Err(rejected(
            "workspace_module_runtime_backend_missing",
            "selected applied VFS mount has no backend identity",
        ));
    }
    let workspace = (!mount.root_ref.trim().is_empty()).then(|| {
        ExtensionInvocationWorkspaceContext::new(mount.mount_id.clone(), mount.root_ref.clone())
    });
    Ok((mount.backend_id.clone(), workspace))
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
