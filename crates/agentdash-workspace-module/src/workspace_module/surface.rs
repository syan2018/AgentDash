use std::collections::BTreeMap;
use std::sync::Arc;

use agentdash_agent_protocol::{
    BackboneEnvelope, BackboneEvent, ControlPlaneProjection, ControlPlaneProjectionChangeReason,
    ControlPlaneProjectionChanged, ControlPlaneWorkspaceModulePresentation, PlatformEvent,
    SourceInfo, TraceInfo,
};
use agentdash_application_ports::agent_frame_materialization::{
    CanvasVisibilityReason, RuntimeSurfaceUpdateRequest,
};
use agentdash_application_ports::agent_run_surface::AgentRunEffectiveCapabilityView;
use agentdash_application_runtime_gateway::{
    ExtensionRuntimeBackendServiceInvokeRequest, ExtensionRuntimeBackendServiceInvokeResult,
    ExtensionRuntimeBackendServiceInvoker, ExtensionRuntimeProtocolConsumer,
    ExtensionRuntimeProtocolInvokeRequest, ExtensionRuntimeProtocolInvokeResult,
    ExtensionRuntimeProtocolInvoker, RuntimeActionKey, RuntimeActor, RuntimeContext,
    RuntimeGateway, RuntimeInvocationError, RuntimeInvocationErrorKind, RuntimeInvocationRequest,
    RuntimeInvocationResult, RuntimeTarget, RuntimeTrace, attach_extension_invocation_workspace,
};
use agentdash_contracts::workspace_module::{
    WorkspaceModuleCanvasHostAction, WorkspaceModuleDescriptor, WorkspaceModuleKind,
    WorkspaceModuleOperation, WorkspaceModuleOperationDispatch, WorkspaceModuleOperationReadiness,
    WorkspaceModuleOperationReadinessKind, WorkspaceModuleOperationVisibility,
    WorkspaceModulePresentation,
};
use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag, diag_error};
use agentdash_domain::canvas::{
    CANVAS_SYSTEM_SKILL_NAME, Canvas, CanvasAccessAction, CanvasAccessProjection,
    CanvasDataBinding, CanvasInteractionSnapshot, CanvasRepository, CanvasRuntimeObservation,
    CanvasRuntimeStateRepository, CanvasScope, canvas_access_projection,
};
use agentdash_domain::project::{
    ProjectAuthorization, ProjectAuthorizationContext, ProjectRepository,
};
use agentdash_domain::shared_library::ProjectExtensionInstallationRepository;
use agentdash_domain::workflow::{
    RuntimeSessionExecutionAnchor, RuntimeSessionExecutionAnchorRepository,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::canvas::{
    CANVAS_BIND_DATA_OPERATION_KEY, CANVAS_RENDERER_KIND, CanvasMutationInput, CanvasRepositorySet,
    CopyCanvasInput, CreatePersonalCanvasInput, canvas_module_id, canvas_presentation_uri,
    canvas_vfs_mount_id, copy_canvas_to_personal, create_personal_canvas,
    load_canvas_by_project_mount_id, normalize_canvas_mount_id, upsert_canvas_data_binding,
    validate_canvas_data_bindings,
};
use crate::workspace_module::runtime_bridge::{
    SharedWorkspaceModuleAgentRunBridgeHandle, SharedWorkspaceModuleRuntimeGatewayHandle,
    WorkspaceModuleRuntimeBridgeError,
};
use crate::workspace_module::{
    ResolvedInvocationBackend, WorkspaceModuleOperationContext,
    WorkspaceModuleRuntimeActionCatalog, WorkspaceModuleRuntimeContext,
    build_canvas_workspace_module, build_workspace_module_presentation,
    resolve_workspace_module_visibility_with_operation_context, validate_input_against_schema,
};

#[derive(Clone, Default)]
pub(crate) struct WorkspaceModuleVisibilitySource {
    agent_run_bridge_handle: Option<SharedWorkspaceModuleAgentRunBridgeHandle>,
    delivery_runtime_session_id: Option<String>,
    current_user: Option<ProjectAuthorizationContext>,
    #[cfg(test)]
    effective_view: Option<AgentRunEffectiveCapabilityView>,
}

impl WorkspaceModuleVisibilitySource {
    pub(crate) fn with_agent_run_delivery(
        mut self,
        agent_run_bridge_handle: SharedWorkspaceModuleAgentRunBridgeHandle,
        delivery_runtime_session_id: impl Into<String>,
    ) -> Self {
        self.agent_run_bridge_handle = Some(agent_run_bridge_handle);
        self.delivery_runtime_session_id = Some(delivery_runtime_session_id.into());
        self
    }

    pub(crate) fn with_current_user(
        mut self,
        current_user: Option<ProjectAuthorizationContext>,
    ) -> Self {
        self.current_user = current_user;
        self
    }

    pub(crate) fn current_user(&self) -> Option<&ProjectAuthorizationContext> {
        self.current_user.as_ref()
    }

    #[cfg(test)]
    pub(crate) fn with_effective_view(mut self, view: AgentRunEffectiveCapabilityView) -> Self {
        self.effective_view = Some(view);
        self
    }

    async fn effective_view(
        &self,
    ) -> Result<AgentRunEffectiveCapabilityView, WorkspaceModuleSurfaceError> {
        #[cfg(test)]
        if let Some(view) = self.effective_view.clone() {
            return Ok(view);
        }

        let (Some(handle), Some(delivery_runtime_session_id)) = (
            self.agent_run_bridge_handle.as_ref(),
            self.delivery_runtime_session_id.as_deref(),
        ) else {
            return Err(WorkspaceModuleSurfaceError::ExecutionFailed(
                "AgentRun effective capability view unavailable for workspace module visibility"
                    .to_string(),
            ));
        };
        let Some(agent_run_bridge) = handle.get().await else {
            return Err(WorkspaceModuleSurfaceError::ExecutionFailed(
                "Workspace module AgentRun bridge 尚未完成初始化".to_string(),
            ));
        };
        agent_run_bridge
            .effective_capability_view_for_agent_run_delivery(delivery_runtime_session_id)
            .await
            .map_err(WorkspaceModuleSurfaceError::ExecutionFailed)
    }
}

#[derive(Clone)]
pub(crate) struct WorkspaceModuleOperationRuntimeSource {
    runtime_gateway: Option<Arc<RuntimeGateway>>,
    runtime_gateway_handle: Option<SharedWorkspaceModuleRuntimeGatewayHandle>,
    delivery_runtime_session_id: Option<String>,
    agent_id: Option<String>,
    protocol_transport_available: bool,
    backend_readiness: WorkspaceModuleOperationReadiness,
    backend_service_readiness: WorkspaceModuleOperationReadiness,
}

impl Default for WorkspaceModuleOperationRuntimeSource {
    fn default() -> Self {
        Self {
            runtime_gateway: None,
            runtime_gateway_handle: None,
            delivery_runtime_session_id: None,
            agent_id: None,
            protocol_transport_available: false,
            backend_readiness: WorkspaceModuleOperationReadiness::unavailable(
                WorkspaceModuleOperationReadinessKind::MissingRuntimeBackendAnchor,
                "runtime backend anchor is not available in this workspace module context",
            ),
            backend_service_readiness: WorkspaceModuleOperationReadiness::unavailable(
                WorkspaceModuleOperationReadinessKind::BackendServiceUnavailable,
                "backendService bridge transport is not available in this workspace module context",
            ),
        }
    }
}

impl WorkspaceModuleOperationRuntimeSource {
    pub(crate) fn with_gateway_handle(
        mut self,
        runtime_gateway_handle: SharedWorkspaceModuleRuntimeGatewayHandle,
        delivery_runtime_session_id: impl Into<String>,
        agent_id: Option<String>,
        protocol_transport_available: bool,
        backend_readiness: WorkspaceModuleOperationReadiness,
        backend_service_readiness: WorkspaceModuleOperationReadiness,
    ) -> Self {
        self.runtime_gateway_handle = Some(runtime_gateway_handle);
        self.delivery_runtime_session_id = Some(delivery_runtime_session_id.into());
        self.agent_id = agent_id;
        self.protocol_transport_available = protocol_transport_available;
        self.backend_readiness = backend_readiness;
        self.backend_service_readiness = backend_service_readiness;
        self
    }

    pub(crate) fn with_gateway(
        mut self,
        runtime_gateway: Arc<RuntimeGateway>,
        delivery_runtime_session_id: impl Into<String>,
        agent_id: Option<String>,
        protocol_transport_available: bool,
        backend_readiness: WorkspaceModuleOperationReadiness,
        backend_service_readiness: WorkspaceModuleOperationReadiness,
    ) -> Self {
        self.runtime_gateway = Some(runtime_gateway);
        self.delivery_runtime_session_id = Some(delivery_runtime_session_id.into());
        self.agent_id = agent_id;
        self.protocol_transport_available = protocol_transport_available;
        self.backend_readiness = backend_readiness;
        self.backend_service_readiness = backend_service_readiness;
        self
    }

    async fn operation_context(&self, project_id: Uuid) -> WorkspaceModuleOperationContext {
        let runtime_actions = self.runtime_action_catalog(project_id).await;
        WorkspaceModuleOperationContext {
            runtime_actions,
            protocol_readiness: self.protocol_readiness(),
            backend_readiness: self.backend_readiness.clone(),
            backend_service_readiness: self.backend_service_readiness.clone(),
        }
    }

    async fn runtime_action_catalog(
        &self,
        project_id: Uuid,
    ) -> WorkspaceModuleRuntimeActionCatalog {
        let Some(delivery_runtime_session_id) = self.delivery_runtime_session_id.as_ref() else {
            return WorkspaceModuleRuntimeActionCatalog::unavailable(
                "delivery runtime session id is unavailable for RuntimeGateway catalog discovery",
            );
        };
        let runtime_gateway = match self.runtime_gateway.as_ref() {
            Some(gateway) => Some(gateway.clone()),
            None => match self.runtime_gateway_handle.as_ref() {
                Some(handle) => handle.get().await,
                None => None,
            },
        };
        let Some(runtime_gateway) = runtime_gateway else {
            return WorkspaceModuleRuntimeActionCatalog::missing_runtime_gateway(
                "RuntimeGateway is not available for workspace module operation catalog discovery",
            );
        };
        match runtime_gateway
            .surface_for_actor(
                RuntimeActor::AgentSession {
                    session_id: delivery_runtime_session_id.clone(),
                    agent_id: self.agent_id.clone(),
                },
                RuntimeContext::Session {
                    session_id: delivery_runtime_session_id.clone(),
                    project_id: Some(project_id),
                    workspace_id: None,
                },
            )
            .await
        {
            Ok(surface) => WorkspaceModuleRuntimeActionCatalog::from_descriptors(surface.actions),
            Err(error) => WorkspaceModuleRuntimeActionCatalog::unavailable(format!(
                "RuntimeGateway actor/context catalog is unavailable: {error}"
            )),
        }
    }

    fn protocol_readiness(&self) -> WorkspaceModuleOperationReadiness {
        if self.protocol_transport_available {
            WorkspaceModuleOperationReadiness::ready()
        } else {
            WorkspaceModuleOperationReadiness::unavailable(
                WorkspaceModuleOperationReadinessKind::MissingProtocolTransport,
                "extension protocol transport is not available in this runtime",
            )
        }
    }
}

pub(crate) struct WorkspaceModuleResolveContext<'a> {
    pub installation_repo: &'a Arc<dyn ProjectExtensionInstallationRepository>,
    pub canvas_repo: &'a Arc<dyn CanvasRepository>,
    pub project_id: Uuid,
    pub visibility_source: &'a WorkspaceModuleVisibilitySource,
    pub operation_runtime_source: &'a WorkspaceModuleOperationRuntimeSource,
}

pub(crate) struct WorkspaceModuleSurface {
    pub modules: Vec<WorkspaceModuleDescriptor>,
}

#[derive(Debug, Error)]
pub(crate) enum WorkspaceModuleSurfaceError {
    #[error("{0}")]
    InvalidArguments(String),
    #[error("{0}")]
    ExecutionFailed(String),
}

pub(crate) enum WorkspaceModuleAgentSurfaceCommand<'a> {
    Operate(WorkspaceModuleOperateCommand<'a>),
    Invoke(WorkspaceModuleInvokeCommand<'a>),
    Present(WorkspaceModulePresentCommand<'a>),
}

pub(crate) struct WorkspaceModuleOperateCommand<'a> {
    pub project_repo: &'a Arc<dyn ProjectRepository>,
    pub canvas_repo: &'a Arc<dyn CanvasRepository>,
    pub project_id: Uuid,
    pub runtime_context: &'a WorkspaceModuleRuntimeContext,
    pub operation: String,
    pub input: serde_json::Value,
}

pub(crate) struct WorkspaceModuleInvokeCommand<'a> {
    pub installation_repo: &'a Arc<dyn ProjectExtensionInstallationRepository>,
    pub canvas_repo: &'a Arc<dyn CanvasRepository>,
    pub canvas_runtime_state_repo: &'a Arc<dyn CanvasRuntimeStateRepository>,
    pub execution_anchor_repo: &'a Arc<dyn RuntimeSessionExecutionAnchorRepository>,
    pub project_id: Uuid,
    pub gateway: &'a Arc<RuntimeGateway>,
    pub protocol_invoker: &'a Arc<ExtensionRuntimeProtocolInvoker>,
    pub backend_service_invoker: Option<&'a ExtensionRuntimeBackendServiceInvoker>,
    pub visibility_source: &'a WorkspaceModuleVisibilitySource,
    pub operation_runtime_source: &'a WorkspaceModuleOperationRuntimeSource,
    pub runtime_context: &'a WorkspaceModuleRuntimeContext,
    pub module_id: String,
    pub operation_key: String,
    pub input: serde_json::Value,
}

pub(crate) struct WorkspaceModulePresentCommand<'a> {
    pub installation_repo: &'a Arc<dyn ProjectExtensionInstallationRepository>,
    pub canvas_repo: &'a Arc<dyn CanvasRepository>,
    pub execution_anchor_repo: &'a Arc<dyn RuntimeSessionExecutionAnchorRepository>,
    pub project_id: Uuid,
    pub turn_id: &'a str,
    pub visibility_source: &'a WorkspaceModuleVisibilitySource,
    pub operation_runtime_source: &'a WorkspaceModuleOperationRuntimeSource,
    pub runtime_context: &'a WorkspaceModuleRuntimeContext,
    pub module_id: String,
    pub view_key: String,
    pub payload: Option<serde_json::Value>,
}

pub(crate) enum WorkspaceModuleOperationOutcome {
    CanvasOperated {
        operation: String,
        module_id: String,
        descriptor: WorkspaceModuleDescriptor,
        canvas: WorkspaceModuleCanvasOperationResult,
    },
    RuntimeActionInvoked {
        result: RuntimeInvocationResult,
        provenance: serde_json::Value,
    },
    ProtocolMethodInvoked {
        result: ExtensionRuntimeProtocolInvokeResult,
        provenance: serde_json::Value,
    },
    BackendServiceInvoked {
        result: ExtensionRuntimeBackendServiceInvokeResult,
        provenance: serde_json::Value,
    },
    CanvasBindingApplied {
        result: WorkspaceModuleCanvasBindingResult,
        provenance: serde_json::Value,
    },
    CanvasRuntimeObservationRead {
        canvas_mount_id: String,
        run_id: Uuid,
        agent_id: Uuid,
        observation: Option<CanvasRuntimeObservation>,
    },
    CanvasInteractionSnapshotRead {
        canvas_mount_id: String,
        run_id: Uuid,
        agent_id: Uuid,
        snapshot: Option<CanvasInteractionSnapshot>,
    },
    Presented {
        presentation: WorkspaceModulePresentation,
    },
    Diagnostic(WorkspaceModuleCommandDiagnostic),
}

fn runtime_bridge_error_to_surface_error(
    error: WorkspaceModuleRuntimeBridgeError,
) -> WorkspaceModuleSurfaceError {
    match error {
        WorkspaceModuleRuntimeBridgeError::InvalidArguments(message) => {
            WorkspaceModuleSurfaceError::InvalidArguments(message)
        }
        WorkspaceModuleRuntimeBridgeError::ExecutionFailed(message) => {
            WorkspaceModuleSurfaceError::ExecutionFailed(message)
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct WorkspaceModuleCommandDiagnostic {
    pub code: &'static str,
    pub message: String,
    pub details: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreatePersonalCanvasModuleInput {
    pub canvas_mount_id: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct AttachExistingCanvasModuleInput {
    pub canvas_mount_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CopyCanvasToPersonalModuleInput {
    pub source_mount_id: Option<String>,
    pub canvas_mount_id: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct WorkspaceModuleCanvasOperationResult {
    pub action: String,
    pub canvas_id: String,
    pub canvas_mount_id: String,
    pub vfs_mount_id: String,
    pub module_id: String,
    pub presentation_uri: String,
    pub title: String,
    pub entry_file: String,
    pub skill_name: String,
    pub skill_path: String,
}

#[derive(Debug, Deserialize)]
struct BindCanvasDataParams {
    canvas_mount_id: String,
    alias: String,
    source_uri: String,
    content_type: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct WorkspaceModuleCanvasBindingResult {
    pub canvas_id: String,
    pub canvas_mount_id: String,
    pub vfs_mount_id: String,
    pub bindings: Vec<CanvasDataBinding>,
    pub alias: String,
    pub source_uri: String,
    pub content_type: String,
}

pub(crate) struct WorkspaceModuleAgentSurface;

impl WorkspaceModuleAgentSurface {
    pub(crate) async fn resolve(
        context: WorkspaceModuleResolveContext<'_>,
    ) -> Result<WorkspaceModuleSurface, WorkspaceModuleSurfaceError> {
        let view = context.visibility_source.effective_view().await?;
        let operation_context = context
            .operation_runtime_source
            .operation_context(context.project_id)
            .await;
        let projection = resolve_workspace_module_visibility_with_operation_context(
            context.installation_repo,
            context.canvas_repo,
            context.project_id,
            &view,
            &operation_context,
        )
        .await
        .map_err(WorkspaceModuleSurfaceError::ExecutionFailed)?;
        for diagnostic in &projection.diagnostics {
            diag!(Warn, Subsystem::AgentRun,
                code = %diagnostic.code,
                module_ref = diagnostic.module_ref.as_deref().unwrap_or(""),
                "workspace module visibility diagnostic: {}",
                diagnostic.message
            );
        }
        let modules = reproject_canvas_modules_for_access(
            context.canvas_repo,
            context.project_id,
            projection.modules,
            context.visibility_source.current_user(),
        )
        .await?;
        Ok(WorkspaceModuleSurface {
            modules: filter_agent_visible_operations(modules),
        })
    }

    pub(crate) async fn execute(
        command: WorkspaceModuleAgentSurfaceCommand<'_>,
    ) -> Result<WorkspaceModuleOperationOutcome, WorkspaceModuleSurfaceError> {
        match command {
            WorkspaceModuleAgentSurfaceCommand::Operate(command) => operate(command).await,
            WorkspaceModuleAgentSurfaceCommand::Invoke(command) => invoke(command).await,
            WorkspaceModuleAgentSurfaceCommand::Present(command) => present(command).await,
        }
    }
}

async fn operate(
    command: WorkspaceModuleOperateCommand<'_>,
) -> Result<WorkspaceModuleOperationOutcome, WorkspaceModuleSurfaceError> {
    let operation = command.operation.trim().to_string();
    let Some(current_user) = command.runtime_context.current_user() else {
        return Err(WorkspaceModuleSurfaceError::ExecutionFailed(
            "workspace_module_operate 需要当前 runtime identity".to_string(),
        ));
    };
    let repos = WorkspaceModuleCanvasRepos {
        project_repo: command.project_repo.as_ref(),
        canvas_repo: command.canvas_repo.as_ref(),
    };
    let (canvas, canvas_result) = match operation.as_str() {
        "canvas.create" => {
            let params: CreatePersonalCanvasModuleInput = serde_json::from_value(command.input)
                .map_err(|error| {
                    WorkspaceModuleSurfaceError::InvalidArguments(format!(
                        "invalid canvas.create input: {error}"
                    ))
                })?;
            operate_create_personal_canvas_for_workspace_module(
                &repos,
                command.project_id,
                current_user,
                command.runtime_context,
                params,
            )
            .await?
        }
        "canvas.attach" => {
            let params: AttachExistingCanvasModuleInput = serde_json::from_value(command.input)
                .map_err(|error| {
                    WorkspaceModuleSurfaceError::InvalidArguments(format!(
                        "invalid canvas.attach input: {error}"
                    ))
                })?;
            operate_attach_existing_canvas_for_workspace_module(
                &repos,
                command.project_id,
                current_user,
                command.runtime_context,
                params,
            )
            .await?
        }
        "canvas.copy" => {
            let params: CopyCanvasToPersonalModuleInput = serde_json::from_value(command.input)
                .map_err(|error| {
                    WorkspaceModuleSurfaceError::InvalidArguments(format!(
                        "invalid canvas.copy input: {error}"
                    ))
                })?;
            operate_copy_canvas_to_personal_for_workspace_module(
                &repos,
                command.project_id,
                current_user,
                command.runtime_context,
                params,
            )
            .await?
        }
        _ => {
            return Ok(WorkspaceModuleOperationOutcome::Diagnostic(
                WorkspaceModuleCommandDiagnostic {
                    code: "unsupported_workspace_module_operation",
                    message: format!("workspace_module_operate 暂不支持 operation `{operation}`"),
                    details: serde_json::json!({
                        "operation": operation,
                        "supported_operations": [
                            "canvas.create",
                            "canvas.attach",
                            "canvas.copy"
                        ],
                    }),
                },
            ));
        }
    };
    let access = canvas_access_for_workspace_module(&canvas, current_user);
    let descriptor = build_canvas_workspace_module(&canvas, &access);
    let module_id = descriptor.summary.module_id.clone();
    Ok(WorkspaceModuleOperationOutcome::CanvasOperated {
        operation,
        module_id,
        descriptor,
        canvas: canvas_result,
    })
}

async fn invoke(
    command: WorkspaceModuleInvokeCommand<'_>,
) -> Result<WorkspaceModuleOperationOutcome, WorkspaceModuleSurfaceError> {
    let modules = WorkspaceModuleAgentSurface::resolve(WorkspaceModuleResolveContext {
        installation_repo: command.installation_repo,
        canvas_repo: command.canvas_repo,
        project_id: command.project_id,
        visibility_source: command.visibility_source,
        operation_runtime_source: command.operation_runtime_source,
    })
    .await?
    .modules;

    let module_id = command.module_id.as_str();
    let operation_key = command.operation_key.as_str();
    let (module, operation) = match locate_operation(&modules, module_id, operation_key) {
        Ok(found) => found,
        Err(diagnostic) => {
            if operation_key == CANVAS_BIND_DATA_OPERATION_KEY
                && let Some(module) = modules.iter().find(|module| {
                    module.summary.kind == WorkspaceModuleKind::Canvas
                        && module.summary.module_id == module_id
                })
                && let Err(guard_diagnostic) = load_canvas_for_runtime_binding(
                    command.canvas_repo.as_ref(),
                    command.project_id,
                    command.runtime_context.current_user(),
                    &module.summary.source,
                )
                .await
            {
                return Ok(WorkspaceModuleOperationOutcome::Diagnostic(
                    guard_diagnostic,
                ));
            }
            return Ok(WorkspaceModuleOperationOutcome::Diagnostic(diagnostic));
        }
    };

    if operation.visibility != WorkspaceModuleOperationVisibility::AgentAndPanel {
        return Ok(WorkspaceModuleOperationOutcome::Diagnostic(
            WorkspaceModuleCommandDiagnostic {
                code: "operation_not_agent_visible",
                message: format!(
                    "operation `{operation_key}` on module `{module_id}` is not exposed to Agent"
                ),
                details: serde_json::json!({
                    "module_id": module_id,
                    "operation_key": operation_key,
                    "visibility": operation.visibility,
                }),
            },
        ));
    }

    if !operation.readiness.is_ready() {
        return Ok(WorkspaceModuleOperationOutcome::Diagnostic(
            operation_not_ready_diagnostic(module_id, operation_key, operation),
        ));
    }

    if let Some(schema) = operation.input_schema.as_ref()
        && let Err(reason) = validate_input_against_schema(schema, &command.input)
    {
        return Ok(WorkspaceModuleOperationOutcome::Diagnostic(
            WorkspaceModuleCommandDiagnostic {
                code: "input_schema_mismatch",
                message: format!(
                    "input 不满足 operation `{operation_key}` 的 input_schema：{reason}"
                ),
                details: serde_json::json!({
                    "module_id": module_id,
                    "operation_key": operation_key,
                }),
            },
        ));
    }

    let provenance = serde_json::json!({
        "module_id": module_id,
        "module_kind": module.summary.kind,
        "module_source": module.summary.source,
        "operation_key": operation_key,
        "operation_origin": operation.origin,
        "runtime_backing": module.runtime_backing,
    });

    match &operation.dispatch {
        WorkspaceModuleOperationDispatch::RuntimeAction { action_key } => {
            let backend = match require_backend(command.runtime_context.backend()) {
                Ok(backend) => backend,
                Err(diagnostic) => {
                    return Ok(WorkspaceModuleOperationOutcome::Diagnostic(diagnostic));
                }
            };
            let action_key = RuntimeActionKey::parse(action_key.clone()).map_err(|error| {
                WorkspaceModuleSurfaceError::ExecutionFailed(format!(
                    "operation `{operation_key}` 的 action_key 非法: {error}"
                ))
            })?;
            let mut request = RuntimeInvocationRequest::new(
                action_key,
                command.runtime_context.runtime_actor(),
                command.runtime_context.runtime_context(),
                command.input,
            );
            request.target = Some(RuntimeTarget::Backend {
                backend_id: backend.backend_id.clone(),
            });
            if let Some(workspace) = backend.workspace.clone() {
                attach_extension_invocation_workspace(&mut request, Some(workspace));
            }
            let mut provenance = provenance;
            if let Some(obj) = provenance.as_object_mut() {
                obj.insert("backend".to_string(), serde_json::json!(backend.backend_id));
            }
            let result = command
                .gateway
                .invoke(request)
                .await
                .map_err(runtime_error_to_surface_error)?;
            Ok(WorkspaceModuleOperationOutcome::RuntimeActionInvoked { result, provenance })
        }
        WorkspaceModuleOperationDispatch::ProtocolMethod {
            provider_extension_key,
            provider_extension_id: _,
            protocol_key,
            protocol_version,
            method_name,
        } => {
            let backend = match require_backend(command.runtime_context.backend()) {
                Ok(backend) => backend,
                Err(diagnostic) => {
                    return Ok(WorkspaceModuleOperationOutcome::Diagnostic(diagnostic));
                }
            };
            let trace = RuntimeTrace::new();
            let result = command
                .protocol_invoker
                .invoke(ExtensionRuntimeProtocolInvokeRequest {
                    project_id: command.project_id,
                    session_id: command
                        .runtime_context
                        .delivery_runtime_session_id()
                        .to_string(),
                    backend_id: backend.backend_id.clone(),
                    workspace: backend.workspace.clone(),
                    consumer: ExtensionRuntimeProtocolConsumer::SessionUser,
                    provider_extension_key: Some(provider_extension_key.clone()),
                    protocol_key: protocol_key.clone(),
                    protocol_version: Some(protocol_version.clone()),
                    dependency_alias: None,
                    method: method_name.clone(),
                    input: command.input,
                    trace,
                })
                .await
                .map_err(runtime_error_to_surface_error)?;

            let mut provenance = provenance;
            if let Some(obj) = provenance.as_object_mut() {
                obj.insert("backend".to_string(), serde_json::json!(backend.backend_id));
                obj.insert(
                    "protocol_key".to_string(),
                    serde_json::json!(result.protocol_key),
                );
                obj.insert("method".to_string(), serde_json::json!(result.method));
            }
            Ok(WorkspaceModuleOperationOutcome::ProtocolMethodInvoked { result, provenance })
        }
        WorkspaceModuleOperationDispatch::BackendService { service_key, route } => {
            let backend = match require_backend(command.runtime_context.backend()) {
                Ok(backend) => backend,
                Err(diagnostic) => {
                    return Ok(WorkspaceModuleOperationOutcome::Diagnostic(diagnostic));
                }
            };
            let Some(invoker) = command.backend_service_invoker else {
                return Ok(WorkspaceModuleOperationOutcome::Diagnostic(
                    WorkspaceModuleCommandDiagnostic {
                        code: "backend_service_unavailable",
                        message: "backendService bridge transport is not attached to this runtime"
                            .to_string(),
                        details: serde_json::json!({
                            "module_id": module_id,
                            "operation_key": operation_key,
                            "service_key": service_key,
                            "route": route,
                        }),
                    },
                ));
            };
            let trace = RuntimeTrace::new();
            let body = serde_json::to_string(&command.input).map_err(|error| {
                WorkspaceModuleSurfaceError::InvalidArguments(format!(
                    "backendService operation input 序列化失败: {error}"
                ))
            })?;
            let mut headers = BTreeMap::new();
            headers.insert(
                "content-type".to_string(),
                "application/json; charset=utf-8".to_string(),
            );
            let result = invoker
                .invoke(ExtensionRuntimeBackendServiceInvokeRequest {
                    project_id: command.project_id,
                    session_id: command
                        .runtime_context
                        .delivery_runtime_session_id()
                        .to_string(),
                    backend_id: backend.backend_id.clone(),
                    workspace: backend.workspace.clone(),
                    extension_key: module.summary.source.clone(),
                    service_key: service_key.clone(),
                    route: route.clone(),
                    method: "POST".to_string(),
                    headers,
                    body: Some(body.into_bytes()),
                    trace,
                })
                .await
                .map_err(runtime_error_to_surface_error)?;

            let mut provenance = provenance;
            if let Some(obj) = provenance.as_object_mut() {
                obj.insert("backend".to_string(), serde_json::json!(backend.backend_id));
                obj.insert(
                    "service_key".to_string(),
                    serde_json::json!(&result.metadata.service_key),
                );
                obj.insert(
                    "route".to_string(),
                    serde_json::json!(&result.metadata.route),
                );
                if let Some(response) = &result.response {
                    obj.insert("status".to_string(), serde_json::json!(response.status));
                }
            }
            Ok(WorkspaceModuleOperationOutcome::BackendServiceInvoked { result, provenance })
        }
        WorkspaceModuleOperationDispatch::HostCanvas { canvas_action } => match canvas_action {
            WorkspaceModuleCanvasHostAction::BindData => {
                let editable_canvas = match load_canvas_for_runtime_binding(
                    command.canvas_repo.as_ref(),
                    command.project_id,
                    command.runtime_context.current_user(),
                    &module.summary.source,
                )
                .await
                {
                    Ok(canvas) => canvas,
                    Err(diagnostic) => {
                        return Ok(WorkspaceModuleOperationOutcome::Diagnostic(diagnostic));
                    }
                };
                let mut input = command.input;
                let Some(obj) = input.as_object_mut() else {
                    return Ok(WorkspaceModuleOperationOutcome::Diagnostic(
                        WorkspaceModuleCommandDiagnostic {
                            code: "invalid_canvas_input",
                            message: "canvas.bind_data input 必须是 object".to_string(),
                            details: serde_json::json!({
                                "module_id": module_id,
                                "operation_key": operation_key,
                            }),
                        },
                    ));
                };
                obj.insert(
                    "canvas_mount_id".to_string(),
                    serde_json::Value::String(module.summary.source.clone()),
                );
                let bind_params: BindCanvasDataParams =
                    serde_json::from_value(input).map_err(|error| {
                        WorkspaceModuleSurfaceError::InvalidArguments(format!(
                            "invalid canvas.bind_data input: {error}"
                        ))
                    })?;
                let (canvas, binding, result) = bind_canvas_data_for_loaded_canvas(
                    command.project_id,
                    editable_canvas,
                    bind_params,
                )?;
                command
                    .runtime_context
                    .submit_optional_canvas_surface_update(
                        &canvas,
                        RuntimeSurfaceUpdateRequest::CanvasBindingChanged {
                            canvas_mount_id: canvas.mount_id.clone(),
                            binding,
                        },
                    )
                    .await
                    .map_err(runtime_bridge_error_to_surface_error)?;
                Ok(WorkspaceModuleOperationOutcome::CanvasBindingApplied { result, provenance })
            }
            WorkspaceModuleCanvasHostAction::Inspect => {
                inspect_canvas(
                    command.canvas_runtime_state_repo.as_ref(),
                    command.execution_anchor_repo.as_ref(),
                    command.runtime_context.delivery_runtime_session_id(),
                    &module.summary.source,
                )
                .await
            }
            WorkspaceModuleCanvasHostAction::GetInteractionState => {
                get_canvas_interaction_state(
                    command.canvas_runtime_state_repo.as_ref(),
                    command.execution_anchor_repo.as_ref(),
                    command.runtime_context.delivery_runtime_session_id(),
                    &module.summary.source,
                )
                .await
            }
        },
        WorkspaceModuleOperationDispatch::Builtin { builtin_key } => Ok(
            WorkspaceModuleOperationOutcome::Diagnostic(WorkspaceModuleCommandDiagnostic {
                code: "operation_unimplemented",
                message: format!("builtin operation `{builtin_key}` 暂未实装"),
                details: serde_json::json!({
                    "module_id": module_id,
                    "operation_key": operation_key,
                    "builtin_key": builtin_key,
                }),
            }),
        ),
    }
}

async fn present(
    command: WorkspaceModulePresentCommand<'_>,
) -> Result<WorkspaceModuleOperationOutcome, WorkspaceModuleSurfaceError> {
    let modules = WorkspaceModuleAgentSurface::resolve(WorkspaceModuleResolveContext {
        installation_repo: command.installation_repo,
        canvas_repo: command.canvas_repo,
        project_id: command.project_id,
        visibility_source: command.visibility_source,
        operation_runtime_source: command.operation_runtime_source,
    })
    .await?
    .modules;

    let module_id = command.module_id.as_str();
    let view_key = command.view_key.as_str();
    let Some(module) = modules
        .iter()
        .find(|module| module.summary.module_id == module_id)
    else {
        return Ok(WorkspaceModuleOperationOutcome::Diagnostic(
            WorkspaceModuleCommandDiagnostic {
                code: "module_not_found",
                message: format!("workspace module not found or not visible: {module_id}"),
                details: serde_json::json!({ "module_id": module_id }),
            },
        ));
    };

    let presentation =
        match build_workspace_module_presentation(module, view_key, command.payload, None) {
            Ok(presentation) => presentation,
            Err(error) => {
                let diagnostic = error.diagnostics();
                inject_present_diagnostic(command.runtime_context, command.turn_id, &diagnostic)
                    .await;
                return Ok(WorkspaceModuleOperationOutcome::Diagnostic(
                    WorkspaceModuleCommandDiagnostic {
                        code: "view_not_found",
                        message: error.to_string(),
                        details: diagnostic,
                    },
                ));
            }
        };

    if presentation.renderer_kind == CANVAS_RENDERER_KIND {
        command
            .runtime_context
            .request_existing_canvas_visibility(
                command.canvas_repo.as_ref(),
                &module.summary.source,
            )
            .await
            .map_err(runtime_bridge_error_to_surface_error)?;
    }

    let value = serde_json::to_value(&presentation).map_err(|error| {
        WorkspaceModuleSurfaceError::ExecutionFailed(format!(
            "failed to serialize workspace module presentation: {error}"
        ))
    })?;

    let notification = match build_present_projection_notification(
        command.execution_anchor_repo.as_ref(),
        command.runtime_context.delivery_runtime_session_id(),
        command.turn_id,
        &presentation,
        value.clone(),
    )
    .await
    {
        Ok(notification) => notification,
        Err(diagnostic) => {
            return Ok(WorkspaceModuleOperationOutcome::Diagnostic(diagnostic));
        }
    };
    command
        .runtime_context
        .inject_agent_run_notification(notification)
        .await
        .map_err(runtime_bridge_error_to_surface_error)?;

    Ok(WorkspaceModuleOperationOutcome::Presented { presentation })
}

async fn reproject_canvas_modules_for_access(
    canvas_repo: &Arc<dyn CanvasRepository>,
    project_id: Uuid,
    modules: Vec<WorkspaceModuleDescriptor>,
    current_user: Option<&ProjectAuthorizationContext>,
) -> Result<Vec<WorkspaceModuleDescriptor>, WorkspaceModuleSurfaceError> {
    let Some(current_user) = current_user else {
        return Ok(modules
            .into_iter()
            .filter(|module| module.summary.kind != WorkspaceModuleKind::Canvas)
            .collect());
    };

    let mut visible = Vec::with_capacity(modules.len());
    for module in modules {
        if module.summary.kind != WorkspaceModuleKind::Canvas {
            visible.push(module);
            continue;
        }

        let canvas = load_canvas_by_project_mount_id_for_tool(
            canvas_repo.as_ref(),
            project_id,
            &module.summary.source,
        )
        .await?;
        let access = canvas_access_for_workspace_module(&canvas, current_user);
        if access.can_view {
            visible.push(build_canvas_workspace_module(&canvas, &access));
        }
    }
    Ok(visible)
}

fn filter_agent_visible_operations(
    modules: Vec<WorkspaceModuleDescriptor>,
) -> Vec<WorkspaceModuleDescriptor> {
    modules
        .into_iter()
        .map(|mut module| {
            module.operations.retain(|operation| {
                operation.visibility == WorkspaceModuleOperationVisibility::AgentAndPanel
            });
            module.summary.operation_summary = module
                .operations
                .iter()
                .map(|operation| operation.operation_key.clone())
                .collect();
            module
        })
        .collect()
}

pub(crate) async fn load_canvas_by_project_mount_id_for_tool(
    canvas_repo: &dyn CanvasRepository,
    project_id: Uuid,
    raw_canvas_mount_id: &str,
) -> Result<Canvas, WorkspaceModuleSurfaceError> {
    let canvas_mount_id = normalize_canvas_mount_id(raw_canvas_mount_id)
        .map_err(|error| WorkspaceModuleSurfaceError::InvalidArguments(error.to_string()))?;
    let canvas = canvas_repo
        .get_by_mount_id(project_id, &canvas_mount_id)
        .await
        .map_err(|error| WorkspaceModuleSurfaceError::ExecutionFailed(error.to_string()))?;
    canvas.ok_or_else(|| {
        WorkspaceModuleSurfaceError::ExecutionFailed(format!("Canvas 不存在: {canvas_mount_id}"))
    })
}

pub(crate) fn canvas_access_for_workspace_module(
    canvas: &Canvas,
    current_user: &ProjectAuthorizationContext,
) -> CanvasAccessProjection {
    canvas_access_projection(
        canvas,
        current_user,
        &workspace_module_project_access(canvas, current_user),
    )
}

fn workspace_module_project_access(
    canvas: &Canvas,
    current_user: &ProjectAuthorizationContext,
) -> ProjectAuthorization {
    ProjectAuthorization {
        role: None,
        via_admin_bypass: current_user.is_admin,
        via_template_visibility: canvas.scope == CanvasScope::Project,
    }
}

struct WorkspaceModuleCanvasRepos<'a> {
    project_repo: &'a dyn ProjectRepository,
    canvas_repo: &'a dyn CanvasRepository,
}

impl CanvasRepositorySet for WorkspaceModuleCanvasRepos<'_> {
    fn project_repo(&self) -> &dyn ProjectRepository {
        self.project_repo
    }

    fn canvas_repo(&self) -> &dyn CanvasRepository {
        self.canvas_repo
    }
}

async fn operate_create_personal_canvas_for_workspace_module(
    repos: &dyn CanvasRepositorySet,
    project_id: Uuid,
    current_user: &ProjectAuthorizationContext,
    runtime_context: &WorkspaceModuleRuntimeContext,
    params: CreatePersonalCanvasModuleInput,
) -> Result<(Canvas, WorkspaceModuleCanvasOperationResult), WorkspaceModuleSurfaceError> {
    let title = params
        .title
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            WorkspaceModuleSurfaceError::InvalidArguments(
                "title is required for canvas.create".to_string(),
            )
        })?;
    let canvas = create_personal_canvas(
        repos,
        current_user,
        CreatePersonalCanvasInput {
            project_id,
            title: title.to_string(),
            description: params.description,
            mount_id: params.canvas_mount_id,
            mutation: CanvasMutationInput::default(),
        },
    )
    .await
    .map_err(|error| WorkspaceModuleSurfaceError::ExecutionFailed(error.to_string()))?
    .canvas;
    expose_canvas_for_workspace_module(
        runtime_context,
        canvas,
        "created",
        CanvasVisibilityReason::Created,
    )
    .await
}

async fn operate_attach_existing_canvas_for_workspace_module(
    repos: &dyn CanvasRepositorySet,
    project_id: Uuid,
    current_user: &ProjectAuthorizationContext,
    runtime_context: &WorkspaceModuleRuntimeContext,
    params: AttachExistingCanvasModuleInput,
) -> Result<(Canvas, WorkspaceModuleCanvasOperationResult), WorkspaceModuleSurfaceError> {
    let canvas_mount_id = required_canvas_mount_id(
        params.canvas_mount_id.as_deref(),
        "canvas.attach input.canvas_mount_id",
    )?;
    let canvas = load_canvas_by_project_mount_id(repos, project_id, &canvas_mount_id)
        .await
        .map_err(|error| WorkspaceModuleSurfaceError::ExecutionFailed(error.to_string()))?;
    ensure_canvas_visible_to_current_user(&canvas, current_user)?;
    expose_canvas_for_workspace_module(
        runtime_context,
        canvas,
        "attached",
        CanvasVisibilityReason::Presented,
    )
    .await
}

async fn operate_copy_canvas_to_personal_for_workspace_module(
    repos: &dyn CanvasRepositorySet,
    project_id: Uuid,
    current_user: &ProjectAuthorizationContext,
    runtime_context: &WorkspaceModuleRuntimeContext,
    params: CopyCanvasToPersonalModuleInput,
) -> Result<(Canvas, WorkspaceModuleCanvasOperationResult), WorkspaceModuleSurfaceError> {
    let source_mount_id = required_canvas_mount_id(
        params.source_mount_id.as_deref(),
        "canvas.copy input.source_mount_id",
    )?;
    let source = load_canvas_by_project_mount_id(repos, project_id, &source_mount_id)
        .await
        .map_err(|error| WorkspaceModuleSurfaceError::ExecutionFailed(error.to_string()))?;
    let copy = copy_canvas_to_personal(
        repos,
        current_user,
        source.id,
        CopyCanvasInput {
            mount_id: params.canvas_mount_id,
            title: params.title,
            description: params.description,
        },
    )
    .await
    .map_err(|error| WorkspaceModuleSurfaceError::ExecutionFailed(error.to_string()))?
    .canvas;
    expose_canvas_for_workspace_module(
        runtime_context,
        copy,
        "copied",
        CanvasVisibilityReason::Created,
    )
    .await
}

async fn expose_canvas_for_workspace_module(
    runtime_context: &WorkspaceModuleRuntimeContext,
    canvas: Canvas,
    action: &str,
    reason: CanvasVisibilityReason,
) -> Result<(Canvas, WorkspaceModuleCanvasOperationResult), WorkspaceModuleSurfaceError> {
    runtime_context
        .submit_canvas_surface_update(
            &canvas,
            RuntimeSurfaceUpdateRequest::CanvasVisibilityRequested {
                canvas_mount_id: canvas.mount_id.clone(),
                reason,
            },
        )
        .await
        .map_err(runtime_bridge_error_to_surface_error)?;

    let result = WorkspaceModuleCanvasOperationResult {
        action: action.to_string(),
        canvas_id: canvas.id.to_string(),
        canvas_mount_id: canvas.mount_id.clone(),
        vfs_mount_id: canvas_vfs_mount_id(&canvas.mount_id),
        module_id: canvas_module_id(&canvas.mount_id),
        presentation_uri: canvas_presentation_uri(&canvas.mount_id),
        title: canvas.title.clone(),
        entry_file: canvas.entry_file.clone(),
        skill_name: CANVAS_SYSTEM_SKILL_NAME.to_string(),
        skill_path: format!("lifecycle://skills/{CANVAS_SYSTEM_SKILL_NAME}/SKILL.md"),
    };
    Ok((canvas, result))
}

fn required_canvas_mount_id(
    value: Option<&str>,
    field_name: &str,
) -> Result<String, WorkspaceModuleSurfaceError> {
    let raw = value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            WorkspaceModuleSurfaceError::InvalidArguments(format!("{field_name} is required"))
        })?;
    normalize_canvas_mount_id(raw)
        .map_err(|error| WorkspaceModuleSurfaceError::InvalidArguments(error.to_string()))
}

fn ensure_canvas_visible_to_current_user(
    canvas: &Canvas,
    current_user: &ProjectAuthorizationContext,
) -> Result<(), WorkspaceModuleSurfaceError> {
    let access = canvas_access_for_workspace_module(canvas, current_user);
    if access.allows(CanvasAccessAction::View) {
        Ok(())
    } else {
        Err(WorkspaceModuleSurfaceError::ExecutionFailed(format!(
            "当前用户无权查看 Canvas {}",
            canvas.id
        )))
    }
}

fn locate_operation<'a>(
    modules: &'a [WorkspaceModuleDescriptor],
    module_id: &str,
    operation_key: &str,
) -> Result<
    (&'a WorkspaceModuleDescriptor, &'a WorkspaceModuleOperation),
    WorkspaceModuleCommandDiagnostic,
> {
    let Some(module) = modules
        .iter()
        .find(|module| module.summary.module_id == module_id)
    else {
        return Err(WorkspaceModuleCommandDiagnostic {
            code: "module_not_found",
            message: format!("workspace module not found or not visible: {module_id}"),
            details: serde_json::json!({ "module_id": module_id }),
        });
    };
    let Some(operation) = module
        .operations
        .iter()
        .find(|operation| operation.operation_key == operation_key)
    else {
        return Err(WorkspaceModuleCommandDiagnostic {
            code: "operation_not_found",
            message: format!("unknown operation `{operation_key}` for module `{module_id}`"),
            details: serde_json::json!({
                "module_id": module_id,
                "operation_key": operation_key,
                "available_operations": module
                    .operations
                    .iter()
                    .map(|op| op.operation_key.clone())
                    .collect::<Vec<_>>(),
            }),
        });
    };
    Ok((module, operation))
}

fn readiness_error_code(readiness: &WorkspaceModuleOperationReadiness) -> &'static str {
    match readiness.kind {
        WorkspaceModuleOperationReadinessKind::Ready => "operation_ready",
        WorkspaceModuleOperationReadinessKind::MissingRuntimeGateway => "missing_runtime_gateway",
        WorkspaceModuleOperationReadinessKind::MissingProtocolTransport => {
            "missing_protocol_transport"
        }
        WorkspaceModuleOperationReadinessKind::MissingRuntimeBackendAnchor => {
            "missing_runtime_backend_anchor"
        }
        WorkspaceModuleOperationReadinessKind::BackendUnavailable => "backend_unavailable",
        WorkspaceModuleOperationReadinessKind::RuntimeActionUnavailable => {
            "runtime_action_unavailable"
        }
        WorkspaceModuleOperationReadinessKind::BackendServiceUnavailable => {
            "backend_service_unavailable"
        }
    }
}

fn operation_not_ready_diagnostic(
    module_id: &str,
    operation_key: &str,
    operation: &WorkspaceModuleOperation,
) -> WorkspaceModuleCommandDiagnostic {
    let code = readiness_error_code(&operation.readiness);
    WorkspaceModuleCommandDiagnostic {
        code,
        message: format!(
            "operation `{operation_key}` on module `{module_id}` is not ready: {}",
            operation.readiness.reason.as_deref().unwrap_or(code)
        ),
        details: serde_json::json!({
            "module_id": module_id,
            "operation_key": operation_key,
            "readiness": operation.readiness,
        }),
    }
}

fn require_backend(
    backend: Option<&ResolvedInvocationBackend>,
) -> Result<&ResolvedInvocationBackend, WorkspaceModuleCommandDiagnostic> {
    backend.ok_or_else(|| WorkspaceModuleCommandDiagnostic {
        code: "backend_unavailable",
        message: "当前 AgentRun delivery 无可用 backend target（既无 remote backend execution，vfs 也无 default mount backend），无法执行该 operation".to_string(),
        details: serde_json::json!({}),
    })
}

fn runtime_error_to_surface_error(error: RuntimeInvocationError) -> WorkspaceModuleSurfaceError {
    match error.kind() {
        RuntimeInvocationErrorKind::InvalidRequest => {
            WorkspaceModuleSurfaceError::InvalidArguments(error.to_string())
        }
        RuntimeInvocationErrorKind::CapabilityDenied
        | RuntimeInvocationErrorKind::Conflict
        | RuntimeInvocationErrorKind::ProviderUnavailable
        | RuntimeInvocationErrorKind::ProviderFailed
        | RuntimeInvocationErrorKind::Timeout => {
            WorkspaceModuleSurfaceError::ExecutionFailed(error.to_string())
        }
    }
}

async fn load_canvas_for_runtime_binding(
    canvas_repo: &dyn CanvasRepository,
    project_id: Uuid,
    current_user: Option<&ProjectAuthorizationContext>,
    canvas_mount_id: &str,
) -> Result<Canvas, WorkspaceModuleCommandDiagnostic> {
    let Some(current_user) = current_user else {
        return Err(WorkspaceModuleCommandDiagnostic {
            code: "runtime_identity_required",
            message: "canvas.bind_data 需要当前 runtime identity".to_string(),
            details: serde_json::json!({
                "canvas_mount_id": canvas_mount_id,
                "required_action": "runtime_binding",
            }),
        });
    };
    let canvas =
        match load_canvas_by_project_mount_id_for_tool(canvas_repo, project_id, canvas_mount_id)
            .await
        {
            Ok(canvas) => canvas,
            Err(error) => {
                return Err(WorkspaceModuleCommandDiagnostic {
                    code: "canvas_not_found",
                    message: error.to_string(),
                    details: serde_json::json!({
                        "canvas_mount_id": canvas_mount_id,
                    }),
                });
            }
        };
    let access = canvas_access_for_workspace_module(&canvas, current_user);
    if access.can_view {
        Ok(canvas)
    } else {
        Err(WorkspaceModuleCommandDiagnostic {
            code: "canvas_not_viewable",
            message: format!(
                "当前用户无权查看 Canvas `{}`，无法挂接运行期数据",
                canvas.mount_id
            ),
            details: serde_json::json!({
                "canvas_id": canvas.id,
                "canvas_mount_id": canvas.mount_id,
                "scope": canvas.scope,
                "required_action": "runtime_binding",
            }),
        })
    }
}

fn bind_canvas_data_for_loaded_canvas(
    project_id: Uuid,
    canvas: Canvas,
    params: BindCanvasDataParams,
) -> Result<
    (
        Canvas,
        CanvasDataBinding,
        WorkspaceModuleCanvasBindingResult,
    ),
    WorkspaceModuleSurfaceError,
> {
    if canvas.project_id != project_id {
        return Err(WorkspaceModuleSurfaceError::ExecutionFailed(
            "当前 session 无权操作其它 Project 的 Canvas".to_string(),
        ));
    }
    let requested_mount_id = normalize_canvas_mount_id(&params.canvas_mount_id)
        .map_err(|error| WorkspaceModuleSurfaceError::InvalidArguments(error.to_string()))?;
    if requested_mount_id != canvas.mount_id {
        return Err(WorkspaceModuleSurfaceError::InvalidArguments(format!(
            "canvas.bind_data target `{requested_mount_id}` does not match Canvas `{}`",
            canvas.mount_id
        )));
    }

    let binding =
        CanvasDataBinding::with_content_type(params.alias, params.source_uri, params.content_type);
    let alias = binding.alias.clone();
    let source_uri = binding.source_uri.clone();
    let content_type = binding.content_type.clone();
    let mut runtime_bindings = Vec::new();
    upsert_canvas_data_binding(&mut runtime_bindings, binding.clone())
        .map_err(|error| WorkspaceModuleSurfaceError::ExecutionFailed(error.to_string()))?;
    validate_canvas_data_bindings(&canvas, &runtime_bindings)
        .map_err(|error| WorkspaceModuleSurfaceError::ExecutionFailed(error.to_string()))?;

    let result = WorkspaceModuleCanvasBindingResult {
        canvas_id: canvas.id.to_string(),
        canvas_mount_id: canvas.mount_id.clone(),
        vfs_mount_id: canvas_vfs_mount_id(&canvas.mount_id),
        bindings: runtime_bindings,
        alias,
        source_uri,
        content_type,
    };
    Ok((canvas, binding, result))
}

async fn current_anchor(
    execution_anchor_repo: &dyn RuntimeSessionExecutionAnchorRepository,
    delivery_runtime_session_id: &str,
) -> Result<RuntimeSessionExecutionAnchor, WorkspaceModuleCommandDiagnostic> {
    match execution_anchor_repo
        .find_by_session(delivery_runtime_session_id)
        .await
    {
        Ok(Some(anchor)) => Ok(anchor),
        Ok(None) => Err(WorkspaceModuleCommandDiagnostic {
            code: "runtime_anchor_not_found",
            message:
                "当前 AgentRun delivery runtime 无 execution anchor，无法解析 Canvas 诊断状态归属"
                    .to_string(),
            details: serde_json::json!({
                "delivery_runtime_session_id": delivery_runtime_session_id,
            }),
        }),
        Err(error) => Err(WorkspaceModuleCommandDiagnostic {
            code: "runtime_anchor_query_failed",
            message: format!("查询 runtime execution anchor 失败: {error}"),
            details: serde_json::json!({
                "delivery_runtime_session_id": delivery_runtime_session_id,
            }),
        }),
    }
}

async fn inspect_canvas(
    canvas_runtime_state_repo: &dyn CanvasRuntimeStateRepository,
    execution_anchor_repo: &dyn RuntimeSessionExecutionAnchorRepository,
    delivery_runtime_session_id: &str,
    canvas_mount_id: &str,
) -> Result<WorkspaceModuleOperationOutcome, WorkspaceModuleSurfaceError> {
    let anchor = match current_anchor(execution_anchor_repo, delivery_runtime_session_id).await {
        Ok(anchor) => anchor,
        Err(diagnostic) => return Ok(WorkspaceModuleOperationOutcome::Diagnostic(diagnostic)),
    };
    let observation = canvas_runtime_state_repo
        .latest_runtime_observation(anchor.run_id, anchor.agent_id, canvas_mount_id)
        .await
        .map_err(|error| WorkspaceModuleSurfaceError::ExecutionFailed(error.to_string()))?;
    Ok(
        WorkspaceModuleOperationOutcome::CanvasRuntimeObservationRead {
            canvas_mount_id: canvas_mount_id.to_string(),
            run_id: anchor.run_id,
            agent_id: anchor.agent_id,
            observation,
        },
    )
}

async fn get_canvas_interaction_state(
    canvas_runtime_state_repo: &dyn CanvasRuntimeStateRepository,
    execution_anchor_repo: &dyn RuntimeSessionExecutionAnchorRepository,
    delivery_runtime_session_id: &str,
    canvas_mount_id: &str,
) -> Result<WorkspaceModuleOperationOutcome, WorkspaceModuleSurfaceError> {
    let anchor = match current_anchor(execution_anchor_repo, delivery_runtime_session_id).await {
        Ok(anchor) => anchor,
        Err(diagnostic) => return Ok(WorkspaceModuleOperationOutcome::Diagnostic(diagnostic)),
    };
    let snapshot = canvas_runtime_state_repo
        .latest_interaction_snapshot(anchor.run_id, anchor.agent_id, canvas_mount_id)
        .await
        .map_err(|error| WorkspaceModuleSurfaceError::ExecutionFailed(error.to_string()))?;
    Ok(
        WorkspaceModuleOperationOutcome::CanvasInteractionSnapshotRead {
            canvas_mount_id: canvas_mount_id.to_string(),
            run_id: anchor.run_id,
            agent_id: anchor.agent_id,
            snapshot,
        },
    )
}

async fn inject_present_diagnostic(
    runtime_context: &WorkspaceModuleRuntimeContext,
    turn_id: &str,
    value: &serde_json::Value,
) {
    let notification = build_present_notification(
        runtime_context.delivery_runtime_session_id(),
        turn_id,
        "workspace_module_present_failed",
        value.clone(),
    );
    if let Err(error) = runtime_context
        .inject_agent_run_notification(notification)
        .await
    {
        let diagnostic_context =
            DiagnosticErrorContext::new("workspace_module.surface", "inject_present_diagnostic");
        diag_error!(Warn, Subsystem::AgentRun,
            context = &diagnostic_context,
            error = &error,
            session_id = %runtime_context.delivery_runtime_session_id(),
            turn_id = %turn_id,
            event_kind = "workspace_module_present_failed",
            "workspace_module_present diagnostic notification injection failed"
        );
    }
}

fn build_present_notification(
    session_id: &str,
    turn_id: &str,
    key: &str,
    value: serde_json::Value,
) -> BackboneEnvelope {
    let source = SourceInfo {
        connector_id: "agentdash-workspace-module".to_string(),
        connector_type: "runtime_tool".to_string(),
        executor_id: None,
    };
    BackboneEnvelope::new(
        BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
            key: key.to_string(),
            value,
        }),
        session_id,
        source,
    )
    .with_trace(TraceInfo {
        turn_id: Some(turn_id.to_string()),
        entry_index: None,
    })
}

async fn build_present_projection_notification(
    execution_anchor_repo: &dyn RuntimeSessionExecutionAnchorRepository,
    delivery_runtime_session_id: &str,
    turn_id: &str,
    presentation: &WorkspaceModulePresentation,
    payload: serde_json::Value,
) -> Result<BackboneEnvelope, WorkspaceModuleCommandDiagnostic> {
    let anchor = current_anchor(execution_anchor_repo, delivery_runtime_session_id).await?;
    let source = SourceInfo {
        connector_id: "agentdash-workspace-module".to_string(),
        connector_type: "runtime_tool".to_string(),
        executor_id: None,
    };
    Ok(BackboneEnvelope::new(
        BackboneEvent::Platform(PlatformEvent::ControlPlaneProjectionChanged(
            ControlPlaneProjectionChanged {
                projection: ControlPlaneProjection::ResourceSurface,
                reason: ControlPlaneProjectionChangeReason::WorkspaceModulePresented,
                run_id: anchor.run_id.to_string(),
                agent_id: anchor.agent_id.to_string(),
                frame_id: Some(anchor.launch_frame_id.to_string()),
                gate_id: None,
                mailbox_message_id: None,
                delivery_runtime_session_id: Some(delivery_runtime_session_id.to_string()),
                workspace_module_presentation: Some(ControlPlaneWorkspaceModulePresentation {
                    module_id: presentation.module_id.clone(),
                    view_key: presentation.view_key.clone(),
                    renderer_kind: presentation.renderer_kind.clone(),
                    presentation_uri: presentation.presentation_uri.clone(),
                    title: presentation.title.clone(),
                    payload: Some(payload),
                    diagnostics: None,
                }),
            },
        )),
        delivery_runtime_session_id,
        source,
    )
    .with_trace(TraceInfo {
        turn_id: Some(turn_id.to_string()),
        entry_index: None,
    }))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use agentdash_application_ports::agent_run_surface::AgentRunGrantProjection;
    use agentdash_application_ports::extension_runtime::{
        ExtensionProtocolInvokeRequest, ExtensionProtocolInvokeResponse,
        ExtensionRuntimeActionTransportError, ExtensionRuntimeProtocolTransport,
    };
    use agentdash_application_ports::runtime_surface_adoption::AgentFrameRuntimeTarget;
    use agentdash_application_vfs::tools::{RuntimeVfsState, SharedRuntimeVfs};
    use agentdash_contracts::workspace_module::{
        WorkspaceModuleStatus, WorkspaceModuleStatusKind, WorkspaceModuleSummary,
    };
    use agentdash_domain::DomainError;
    use agentdash_domain::canvas::{Canvas, CanvasRepository};
    use agentdash_domain::common::Vfs;
    use agentdash_domain::project::ProjectAuthorizationContext;
    use agentdash_domain::shared_library::{
        ProjectExtensionInstallation, ProjectExtensionInstallationRepository,
    };
    use agentdash_spi::{
        CapabilityState, RuntimeVfsAccessPolicy, ToolCluster, WorkspaceModuleDimension,
    };
    use async_trait::async_trait;

    use super::*;
    use crate::canvas::build_canvas;
    use crate::workspace_module::runtime_bridge::WorkspaceModuleAgentRunBridge;

    #[derive(Default)]
    struct EmptyInstallationRepo;

    #[async_trait]
    impl ProjectExtensionInstallationRepository for EmptyInstallationRepo {
        async fn create(&self, _item: &ProjectExtensionInstallation) -> Result<(), DomainError> {
            Ok(())
        }

        async fn update(&self, _item: &ProjectExtensionInstallation) -> Result<(), DomainError> {
            Ok(())
        }

        async fn get_by_project_and_key(
            &self,
            _project_id: Uuid,
            _extension_key: &str,
        ) -> Result<Option<ProjectExtensionInstallation>, DomainError> {
            Ok(None)
        }

        async fn get_by_project_and_id(
            &self,
            _project_id: Uuid,
            _installation_id: Uuid,
        ) -> Result<Option<ProjectExtensionInstallation>, DomainError> {
            Ok(None)
        }

        async fn list_by_project(
            &self,
            _project_id: Uuid,
        ) -> Result<Vec<ProjectExtensionInstallation>, DomainError> {
            Ok(Vec::new())
        }

        async fn list_enabled_by_project(
            &self,
            _project_id: Uuid,
        ) -> Result<Vec<ProjectExtensionInstallation>, DomainError> {
            Ok(Vec::new())
        }

        async fn delete(
            &self,
            _project_id: Uuid,
            _installation_id: Uuid,
        ) -> Result<bool, DomainError> {
            Ok(false)
        }
    }

    #[derive(Default)]
    struct FixtureCanvasRepo {
        canvases: Mutex<HashMap<(Uuid, String), Canvas>>,
    }

    #[async_trait]
    impl CanvasRepository for FixtureCanvasRepo {
        async fn create(&self, canvas: &Canvas) -> Result<(), DomainError> {
            self.canvases
                .lock()
                .expect("canvas lock")
                .insert((canvas.project_id, canvas.mount_id.clone()), canvas.clone());
            Ok(())
        }

        async fn update(&self, canvas: &Canvas) -> Result<(), DomainError> {
            self.create(canvas).await
        }

        async fn get_by_id(&self, id: Uuid) -> Result<Option<Canvas>, DomainError> {
            Ok(self
                .canvases
                .lock()
                .expect("canvas lock")
                .values()
                .find(|canvas| canvas.id == id)
                .cloned())
        }

        async fn get_by_mount_id(
            &self,
            project_id: Uuid,
            mount_id: &str,
        ) -> Result<Option<Canvas>, DomainError> {
            Ok(self
                .canvases
                .lock()
                .expect("canvas lock")
                .get(&(project_id, mount_id.to_string()))
                .cloned())
        }

        async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<Canvas>, DomainError> {
            Ok(self
                .canvases
                .lock()
                .expect("canvas lock")
                .values()
                .filter(|canvas| canvas.project_id == project_id)
                .cloned()
                .collect())
        }

        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.canvases
                .lock()
                .expect("canvas lock")
                .retain(|_, canvas| canvas.id != id);
            Ok(())
        }
    }

    fn effective_view() -> AgentRunEffectiveCapabilityView {
        let mut state = CapabilityState::from_clusters([ToolCluster::WorkspaceModule]);
        state.workspace_module = WorkspaceModuleDimension::all();
        AgentRunEffectiveCapabilityView {
            target: AgentFrameRuntimeTarget {
                frame_id: Uuid::new_v4(),
                delivery_runtime_session_id: "session-a".to_string(),
            },
            visible_capabilities: state.tool.capabilities.clone(),
            vfs_surface: state.vfs.active.clone().unwrap_or_default(),
            mcp_surface: Vec::new(),
            capability_state: state,
            visible_workspace_module_refs: Vec::new(),
            grant_projection: AgentRunGrantProjection::default(),
        }
    }

    #[derive(Default)]
    struct CapturingAgentRunBridge {
        notifications: Mutex<Vec<BackboneEnvelope>>,
    }

    #[async_trait]
    impl WorkspaceModuleAgentRunBridge for CapturingAgentRunBridge {
        async fn effective_capability_view_for_agent_run_delivery(
            &self,
            _delivery_runtime_session_id: &str,
        ) -> Result<AgentRunEffectiveCapabilityView, String> {
            Ok(effective_view())
        }

        async fn apply_canvas_runtime_surface_update_to_agent_run(
            &self,
            _delivery_runtime_session_id: &str,
            _canvas: &Canvas,
            _current_user: Option<&ProjectAuthorizationContext>,
            _request: RuntimeSurfaceUpdateRequest,
        ) -> Result<RuntimeVfsState, String> {
            let vfs = Vfs::default();
            Ok(RuntimeVfsState::new(
                vfs.clone(),
                RuntimeVfsAccessPolicy::whole_mounts_from_vfs(&vfs),
            ))
        }

        async fn inject_agent_run_notification(
            &self,
            _delivery_runtime_session_id: &str,
            notification: BackboneEnvelope,
        ) -> Result<(), String> {
            self.notifications
                .lock()
                .expect("notification lock")
                .push(notification);
            Ok(())
        }
    }

    #[derive(Default)]
    struct EmptyCanvasRuntimeStateRepo;

    #[async_trait]
    impl CanvasRuntimeStateRepository for EmptyCanvasRuntimeStateRepo {
        async fn upsert_runtime_observation(
            &self,
            observation: CanvasRuntimeObservation,
        ) -> Result<CanvasRuntimeObservation, DomainError> {
            Ok(observation)
        }

        async fn latest_runtime_observation(
            &self,
            _run_id: Uuid,
            _agent_id: Uuid,
            _canvas_mount_id: &str,
        ) -> Result<Option<CanvasRuntimeObservation>, DomainError> {
            Ok(None)
        }

        async fn upsert_interaction_snapshot(
            &self,
            snapshot: CanvasInteractionSnapshot,
        ) -> Result<CanvasInteractionSnapshot, DomainError> {
            Ok(snapshot)
        }

        async fn latest_interaction_snapshot(
            &self,
            _run_id: Uuid,
            _agent_id: Uuid,
            _canvas_mount_id: &str,
        ) -> Result<Option<CanvasInteractionSnapshot>, DomainError> {
            Ok(None)
        }
    }

    struct StaticRuntimeSessionExecutionAnchorRepo {
        anchor: RuntimeSessionExecutionAnchor,
    }

    #[async_trait]
    impl RuntimeSessionExecutionAnchorRepository for StaticRuntimeSessionExecutionAnchorRepo {
        async fn create_once(
            &self,
            _anchor: &RuntimeSessionExecutionAnchor,
        ) -> Result<(), DomainError> {
            Ok(())
        }

        async fn delete_by_session(&self, _runtime_session_id: &str) -> Result<(), DomainError> {
            Ok(())
        }

        async fn find_by_session(
            &self,
            runtime_session_id: &str,
        ) -> Result<Option<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok((self.anchor.runtime_session_id == runtime_session_id).then(|| self.anchor.clone()))
        }

        async fn list_by_run(
            &self,
            run_id: Uuid,
        ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok((self.anchor.run_id == run_id)
                .then(|| self.anchor.clone())
                .into_iter()
                .collect())
        }

        async fn list_by_agent(
            &self,
            agent_id: Uuid,
        ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok((self.anchor.agent_id == agent_id)
                .then(|| self.anchor.clone())
                .into_iter()
                .collect())
        }

        async fn list_by_project_session_ids(
            &self,
            runtime_session_ids: &[String],
        ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(runtime_session_ids
                .iter()
                .any(|runtime_session_id| runtime_session_id == &self.anchor.runtime_session_id)
                .then(|| self.anchor.clone())
                .into_iter()
                .collect())
        }
    }

    struct NoopProtocolTransport;

    #[async_trait]
    impl ExtensionRuntimeProtocolTransport for NoopProtocolTransport {
        async fn invoke_extension_protocol(
            &self,
            _backend_id: &str,
            _payload: ExtensionProtocolInvokeRequest,
        ) -> Result<ExtensionProtocolInvokeResponse, ExtensionRuntimeActionTransportError> {
            Err(ExtensionRuntimeActionTransportError::Failed(
                "noop channel transport".to_string(),
            ))
        }
    }

    fn operation_for_visibility(
        operation_key: &str,
        visibility: WorkspaceModuleOperationVisibility,
    ) -> WorkspaceModuleOperation {
        WorkspaceModuleOperation {
            operation_key: operation_key.to_string(),
            origin: "runtime_action".to_string(),
            description: operation_key.to_string(),
            input_schema: None,
            output_schema: None,
            permission_summary: Vec::new(),
            visibility,
            provenance: serde_json::json!({
                "generated_from": "test",
            }),
            dispatch: WorkspaceModuleOperationDispatch::RuntimeAction {
                action_key: operation_key.to_string(),
            },
            readiness: WorkspaceModuleOperationReadiness::ready(),
        }
    }

    #[test]
    fn agent_surface_filters_panel_only_operations() {
        let modules = vec![WorkspaceModuleDescriptor {
            summary: WorkspaceModuleSummary {
                module_id: "ext:demo".to_string(),
                kind: WorkspaceModuleKind::Extension,
                title: "Demo".to_string(),
                description: "Demo extension".to_string(),
                source: "demo".to_string(),
                ui_summary: None,
                operation_summary: vec![
                    "profile.read".to_string(),
                    "panel.fetch_profile".to_string(),
                ],
                permission_summary: Vec::new(),
                status: WorkspaceModuleStatus {
                    kind: WorkspaceModuleStatusKind::Ready,
                    reason: None,
                },
            },
            ui_entries: Vec::new(),
            operations: vec![
                operation_for_visibility(
                    "profile.read",
                    WorkspaceModuleOperationVisibility::AgentAndPanel,
                ),
                operation_for_visibility(
                    "panel.fetch_profile",
                    WorkspaceModuleOperationVisibility::PanelOnly,
                ),
            ],
            runtime_backing: None,
        }];

        let modules = filter_agent_visible_operations(modules);

        assert_eq!(
            modules[0].summary.operation_summary,
            vec!["profile.read".to_string()]
        );
        assert_eq!(modules[0].operations.len(), 1);
        assert_eq!(modules[0].operations[0].operation_key, "profile.read");
    }

    #[tokio::test]
    async fn resolve_reprojects_canvas_descriptor_with_current_user_access() {
        let project_id = Uuid::new_v4();
        let installation_repo: Arc<dyn ProjectExtensionInstallationRepository> =
            Arc::new(EmptyInstallationRepo);
        let canvas_repo = Arc::new(FixtureCanvasRepo::default());
        let canvas = build_canvas(
            project_id,
            Some("cvs-dashboard-a".to_string()),
            "Dashboard A".to_string(),
            "demo canvas".to_string(),
            Default::default(),
        )
        .expect("canvas");
        canvas_repo.create(&canvas).await.expect("create canvas");
        let canvas_repo: Arc<dyn CanvasRepository> = canvas_repo;
        let current_user =
            ProjectAuthorizationContext::new("user-1".to_string(), Vec::new(), false);
        let visibility_source = WorkspaceModuleVisibilitySource::default()
            .with_current_user(Some(current_user))
            .with_effective_view(effective_view());
        let operation_runtime_source = WorkspaceModuleOperationRuntimeSource::default();

        let surface = WorkspaceModuleAgentSurface::resolve(WorkspaceModuleResolveContext {
            installation_repo: &installation_repo,
            canvas_repo: &canvas_repo,
            project_id,
            visibility_source: &visibility_source,
            operation_runtime_source: &operation_runtime_source,
        })
        .await
        .expect("surface resolve");

        assert_eq!(surface.modules.len(), 1);
        let descriptor = &surface.modules[0];
        assert_eq!(descriptor.summary.module_id, "canvas:cvs-dashboard-a");
        assert!(
            descriptor
                .operations
                .iter()
                .any(|operation| operation.operation_key == "canvas.bind_data"),
            "personal Canvas descriptor should be reprojected with writable runtime operation"
        );
    }

    #[tokio::test]
    async fn invoke_canvas_bind_data_returns_surface_binding_outcome() {
        let project_id = Uuid::new_v4();
        let installation_repo: Arc<dyn ProjectExtensionInstallationRepository> =
            Arc::new(EmptyInstallationRepo);
        let canvas_repo = Arc::new(FixtureCanvasRepo::default());
        let canvas = build_canvas(
            project_id,
            Some("cvs-dashboard-a".to_string()),
            "Dashboard A".to_string(),
            "demo canvas".to_string(),
            Default::default(),
        )
        .expect("canvas");
        canvas_repo.create(&canvas).await.expect("create canvas");
        let canvas_repo: Arc<dyn CanvasRepository> = canvas_repo;
        let canvas_runtime_state_repo: Arc<dyn CanvasRuntimeStateRepository> =
            Arc::new(EmptyCanvasRuntimeStateRepo);
        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let frame_id = Uuid::new_v4();
        let execution_anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository> =
            Arc::new(StaticRuntimeSessionExecutionAnchorRepo {
                anchor: RuntimeSessionExecutionAnchor::new_dispatch(
                    "session-a",
                    run_id,
                    frame_id,
                    agent_id,
                ),
            });
        let current_user =
            ProjectAuthorizationContext::new("user-1".to_string(), Vec::new(), false);
        let visibility_source = WorkspaceModuleVisibilitySource::default()
            .with_current_user(Some(current_user.clone()))
            .with_effective_view(effective_view());
        let operation_runtime_source = WorkspaceModuleOperationRuntimeSource::default();
        let bridge = Arc::new(CapturingAgentRunBridge::default());
        let bridge_handle = SharedWorkspaceModuleAgentRunBridgeHandle::default();
        bridge_handle.set(bridge).await;
        let runtime_context = WorkspaceModuleRuntimeContext::new(project_id, "session-a")
            .with_current_user(Some(current_user.clone()))
            .with_agent_run_bridge(Some(bridge_handle.clone()));
        let gateway = Arc::new(RuntimeGateway::new());
        let protocol_invoker = Arc::new(ExtensionRuntimeProtocolInvoker::new(
            installation_repo.clone(),
            Arc::new(NoopProtocolTransport),
        ));

        let outcome = WorkspaceModuleAgentSurface::execute(
            WorkspaceModuleAgentSurfaceCommand::Invoke(WorkspaceModuleInvokeCommand {
                installation_repo: &installation_repo,
                canvas_repo: &canvas_repo,
                canvas_runtime_state_repo: &canvas_runtime_state_repo,
                execution_anchor_repo: &execution_anchor_repo,
                project_id,
                gateway: &gateway,
                protocol_invoker: &protocol_invoker,
                backend_service_invoker: None,
                visibility_source: &visibility_source,
                operation_runtime_source: &operation_runtime_source,
                runtime_context: &runtime_context,
                module_id: "canvas:cvs-dashboard-a".to_string(),
                operation_key: CANVAS_BIND_DATA_OPERATION_KEY.to_string(),
                input: serde_json::json!({
                    "alias": "stats",
                    "source_uri": "project://data/stats.csv"
                }),
            }),
        )
        .await
        .expect("invoke canvas binding");

        let WorkspaceModuleOperationOutcome::CanvasBindingApplied { result, provenance } = outcome
        else {
            panic!("expected CanvasBindingApplied outcome");
        };
        assert_eq!(result.canvas_mount_id, "cvs-dashboard-a");
        assert_eq!(result.alias, "stats");
        assert_eq!(result.source_uri, "project://data/stats.csv");
        assert_eq!(result.content_type, "text/csv");
        assert_eq!(
            provenance
                .get("operation_origin")
                .and_then(serde_json::Value::as_str),
            Some("host_canvas")
        );
    }

    #[tokio::test]
    async fn present_canvas_returns_presentation_outcome_and_injects_notification() {
        let project_id = Uuid::new_v4();
        let installation_repo: Arc<dyn ProjectExtensionInstallationRepository> =
            Arc::new(EmptyInstallationRepo);
        let canvas_repo = Arc::new(FixtureCanvasRepo::default());
        let canvas = build_canvas(
            project_id,
            Some("cvs-dashboard-a".to_string()),
            "Dashboard A".to_string(),
            "demo canvas".to_string(),
            Default::default(),
        )
        .expect("canvas");
        canvas_repo.create(&canvas).await.expect("create canvas");
        let canvas_repo: Arc<dyn CanvasRepository> = canvas_repo;
        let execution_anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository> =
            Arc::new(StaticRuntimeSessionExecutionAnchorRepo {
                anchor: RuntimeSessionExecutionAnchor::new_dispatch(
                    "session-a",
                    Uuid::new_v4(),
                    Uuid::new_v4(),
                    Uuid::new_v4(),
                ),
            });
        let current_user =
            ProjectAuthorizationContext::new("user-1".to_string(), Vec::new(), false);
        let visibility_source = WorkspaceModuleVisibilitySource::default()
            .with_current_user(Some(current_user))
            .with_effective_view(effective_view());
        let operation_runtime_source = WorkspaceModuleOperationRuntimeSource::default();
        let vfs = Vfs::default();
        let shared_vfs = SharedRuntimeVfs::new_with_policy(
            vfs.clone(),
            RuntimeVfsAccessPolicy::whole_mounts_from_vfs(&vfs),
        );
        let bridge = Arc::new(CapturingAgentRunBridge::default());
        let bridge_handle = SharedWorkspaceModuleAgentRunBridgeHandle::default();
        bridge_handle.set(bridge.clone()).await;
        let runtime_context = WorkspaceModuleRuntimeContext::new(project_id, "session-a")
            .with_vfs(shared_vfs.clone())
            .with_current_user(visibility_source.current_user().cloned())
            .with_agent_run_bridge(Some(bridge_handle.clone()));

        let outcome = WorkspaceModuleAgentSurface::execute(
            WorkspaceModuleAgentSurfaceCommand::Present(WorkspaceModulePresentCommand {
                installation_repo: &installation_repo,
                canvas_repo: &canvas_repo,
                execution_anchor_repo: &execution_anchor_repo,
                project_id,
                turn_id: "turn-a",
                visibility_source: &visibility_source,
                operation_runtime_source: &operation_runtime_source,
                runtime_context: &runtime_context,
                module_id: "canvas:cvs-dashboard-a".to_string(),
                view_key: "preview".to_string(),
                payload: None,
            }),
        )
        .await
        .expect("present command");

        let WorkspaceModuleOperationOutcome::Presented { presentation } = outcome else {
            panic!("expected presented outcome");
        };
        assert_eq!(presentation.module_id, "canvas:cvs-dashboard-a");
        assert_eq!(presentation.view_key, "preview");
        assert_eq!(presentation.presentation_uri, "canvas://cvs-dashboard-a");
        assert_eq!(
            bridge
                .notifications
                .lock()
                .expect("notification lock")
                .len(),
            1,
            "successful presentation is injected by the surface"
        );
    }

    #[tokio::test]
    async fn present_missing_view_is_surface_diagnostic_and_injects_failed_notification() {
        let project_id = Uuid::new_v4();
        let installation_repo: Arc<dyn ProjectExtensionInstallationRepository> =
            Arc::new(EmptyInstallationRepo);
        let canvas_repo = Arc::new(FixtureCanvasRepo::default());
        let canvas = build_canvas(
            project_id,
            Some("cvs-dashboard-a".to_string()),
            "Dashboard A".to_string(),
            "demo canvas".to_string(),
            Default::default(),
        )
        .expect("canvas");
        canvas_repo.create(&canvas).await.expect("create canvas");
        let canvas_repo: Arc<dyn CanvasRepository> = canvas_repo;
        let execution_anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository> =
            Arc::new(StaticRuntimeSessionExecutionAnchorRepo {
                anchor: RuntimeSessionExecutionAnchor::new_dispatch(
                    "session-a",
                    Uuid::new_v4(),
                    Uuid::new_v4(),
                    Uuid::new_v4(),
                ),
            });
        let current_user =
            ProjectAuthorizationContext::new("user-1".to_string(), Vec::new(), false);
        let visibility_source = WorkspaceModuleVisibilitySource::default()
            .with_current_user(Some(current_user))
            .with_effective_view(effective_view());
        let operation_runtime_source = WorkspaceModuleOperationRuntimeSource::default();
        let vfs = Vfs::default();
        let shared_vfs = SharedRuntimeVfs::new_with_policy(
            vfs.clone(),
            RuntimeVfsAccessPolicy::whole_mounts_from_vfs(&vfs),
        );
        let bridge = Arc::new(CapturingAgentRunBridge::default());
        let bridge_handle = SharedWorkspaceModuleAgentRunBridgeHandle::default();
        bridge_handle.set(bridge.clone()).await;
        let runtime_context = WorkspaceModuleRuntimeContext::new(project_id, "session-a")
            .with_vfs(shared_vfs.clone())
            .with_current_user(visibility_source.current_user().cloned())
            .with_agent_run_bridge(Some(bridge_handle.clone()));

        let outcome = WorkspaceModuleAgentSurface::execute(
            WorkspaceModuleAgentSurfaceCommand::Present(WorkspaceModulePresentCommand {
                installation_repo: &installation_repo,
                canvas_repo: &canvas_repo,
                execution_anchor_repo: &execution_anchor_repo,
                project_id,
                turn_id: "turn-a",
                visibility_source: &visibility_source,
                operation_runtime_source: &operation_runtime_source,
                runtime_context: &runtime_context,
                module_id: "canvas:cvs-dashboard-a".to_string(),
                view_key: "missing".to_string(),
                payload: None,
            }),
        )
        .await
        .expect("present command");

        let WorkspaceModuleOperationOutcome::Diagnostic(diagnostic) = outcome else {
            panic!("expected diagnostic outcome");
        };
        assert_eq!(diagnostic.code, "view_not_found");
        assert_eq!(
            bridge
                .notifications
                .lock()
                .expect("notification lock")
                .len(),
            1,
            "failed presentation diagnostics are injected by the surface"
        );
    }
}
