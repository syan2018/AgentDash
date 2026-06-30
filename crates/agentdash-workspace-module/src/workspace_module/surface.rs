use std::sync::Arc;

use agentdash_application_ports::agent_frame_materialization::{
    CanvasVisibilityReason, RuntimeSurfaceUpdateRequest,
};
use agentdash_application_ports::agent_run_surface::AgentRunEffectiveCapabilityView;
use agentdash_application_runtime_gateway::{RuntimeActor, RuntimeContext, RuntimeGateway};
use agentdash_application_vfs::tools::SharedRuntimeVfs;
use agentdash_contracts::workspace_module::{
    WorkspaceModuleDescriptor, WorkspaceModuleKind, WorkspaceModuleOperationReadiness,
    WorkspaceModuleOperationReadinessKind,
};
use agentdash_diagnostics::{Subsystem, diag};
use agentdash_domain::canvas::{
    CANVAS_SYSTEM_SKILL_NAME, Canvas, CanvasAccessAction, CanvasAccessProjection, CanvasRepository,
    CanvasScope, canvas_access_projection,
};
use agentdash_domain::project::{
    ProjectAuthorization, ProjectAuthorizationContext, ProjectRepository,
};
use agentdash_domain::shared_library::ProjectExtensionInstallationRepository;
use agentdash_spi::{AgentToolError, ContentPart};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::canvas::{
    CanvasMutationInput, CanvasRepositorySet, CopyCanvasInput, CreatePersonalCanvasInput,
    canvas_module_id, canvas_presentation_uri, canvas_vfs_mount_id, copy_canvas_to_personal,
    create_personal_canvas, load_canvas_by_project_mount_id, normalize_canvas_mount_id,
};
use crate::workspace_module::runtime_bridge::{
    SharedWorkspaceModuleAgentRunBridgeHandle, SharedWorkspaceModuleRuntimeGatewayHandle,
};
use crate::workspace_module::{
    WorkspaceModuleOperationContext, WorkspaceModuleRuntimeActionCatalog,
    build_canvas_workspace_module, resolve_workspace_module_visibility_with_operation_context,
    submit_canvas_runtime_surface_update,
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

    async fn effective_view(&self) -> Result<AgentRunEffectiveCapabilityView, AgentToolError> {
        #[cfg(test)]
        if let Some(view) = self.effective_view.clone() {
            return Ok(view);
        }

        let (Some(handle), Some(delivery_runtime_session_id)) = (
            self.agent_run_bridge_handle.as_ref(),
            self.delivery_runtime_session_id.as_deref(),
        ) else {
            return Err(AgentToolError::ExecutionFailed(
                "AgentRun effective capability view unavailable for workspace module visibility"
                    .to_string(),
            ));
        };
        let Some(agent_run_bridge) = handle.get().await else {
            return Err(AgentToolError::ExecutionFailed(
                "Workspace module AgentRun bridge 尚未完成初始化".to_string(),
            ));
        };
        agent_run_bridge
            .effective_capability_view_for_agent_run_delivery(delivery_runtime_session_id)
            .await
            .map_err(AgentToolError::ExecutionFailed)
    }
}

#[derive(Clone)]
pub(crate) struct WorkspaceModuleOperationRuntimeSource {
    runtime_gateway: Option<Arc<RuntimeGateway>>,
    runtime_gateway_handle: Option<SharedWorkspaceModuleRuntimeGatewayHandle>,
    delivery_runtime_session_id: Option<String>,
    agent_id: Option<String>,
    channel_transport_available: bool,
    backend_readiness: WorkspaceModuleOperationReadiness,
}

impl Default for WorkspaceModuleOperationRuntimeSource {
    fn default() -> Self {
        Self {
            runtime_gateway: None,
            runtime_gateway_handle: None,
            delivery_runtime_session_id: None,
            agent_id: None,
            channel_transport_available: false,
            backend_readiness: WorkspaceModuleOperationReadiness::unavailable(
                WorkspaceModuleOperationReadinessKind::MissingRuntimeBackendAnchor,
                "runtime backend anchor is not available in this workspace module context",
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
        channel_transport_available: bool,
        backend_readiness: WorkspaceModuleOperationReadiness,
    ) -> Self {
        self.runtime_gateway_handle = Some(runtime_gateway_handle);
        self.delivery_runtime_session_id = Some(delivery_runtime_session_id.into());
        self.agent_id = agent_id;
        self.channel_transport_available = channel_transport_available;
        self.backend_readiness = backend_readiness;
        self
    }

    pub(crate) fn with_gateway(
        mut self,
        runtime_gateway: Arc<RuntimeGateway>,
        delivery_runtime_session_id: impl Into<String>,
        agent_id: Option<String>,
        channel_transport_available: bool,
        backend_readiness: WorkspaceModuleOperationReadiness,
    ) -> Self {
        self.runtime_gateway = Some(runtime_gateway);
        self.delivery_runtime_session_id = Some(delivery_runtime_session_id.into());
        self.agent_id = agent_id;
        self.channel_transport_available = channel_transport_available;
        self.backend_readiness = backend_readiness;
        self
    }

    async fn operation_context(&self, project_id: Uuid) -> WorkspaceModuleOperationContext {
        let runtime_actions = self.runtime_action_catalog(project_id).await;
        WorkspaceModuleOperationContext {
            runtime_actions,
            channel_readiness: self.channel_readiness(),
            backend_readiness: self.backend_readiness.clone(),
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

    fn channel_readiness(&self) -> WorkspaceModuleOperationReadiness {
        if self.channel_transport_available {
            WorkspaceModuleOperationReadiness::ready()
        } else {
            WorkspaceModuleOperationReadiness::unavailable(
                WorkspaceModuleOperationReadinessKind::MissingChannelTransport,
                "extension channel transport is not available in this runtime",
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

pub(crate) enum WorkspaceModuleAgentSurfaceCommand<'a> {
    Operate(WorkspaceModuleOperateCommand<'a>),
}

pub(crate) struct WorkspaceModuleOperateCommand<'a> {
    pub project_repo: &'a Arc<dyn ProjectRepository>,
    pub canvas_repo: &'a Arc<dyn CanvasRepository>,
    pub project_id: Uuid,
    pub vfs: &'a SharedRuntimeVfs,
    pub agent_run_bridge_handle: &'a SharedWorkspaceModuleAgentRunBridgeHandle,
    pub delivery_runtime_session_id: Option<&'a str>,
    pub current_user: Option<&'a ProjectAuthorizationContext>,
    pub operation: String,
    pub input: serde_json::Value,
}

pub(crate) enum WorkspaceModuleOperationOutcome {
    CanvasOperated {
        operation: String,
        module_id: String,
        descriptor: WorkspaceModuleDescriptor,
        canvas: WorkspaceModuleCanvasToolResult,
    },
    Diagnostic(WorkspaceModuleCommandDiagnostic),
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
pub(crate) struct WorkspaceModuleCanvasToolResult {
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

pub(crate) struct WorkspaceModuleAgentSurface;

impl WorkspaceModuleAgentSurface {
    pub(crate) async fn resolve(
        context: WorkspaceModuleResolveContext<'_>,
    ) -> Result<WorkspaceModuleSurface, AgentToolError> {
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
        .map_err(AgentToolError::ExecutionFailed)?;
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
        Ok(WorkspaceModuleSurface { modules })
    }

    pub(crate) async fn execute(
        command: WorkspaceModuleAgentSurfaceCommand<'_>,
    ) -> Result<WorkspaceModuleOperationOutcome, AgentToolError> {
        match command {
            WorkspaceModuleAgentSurfaceCommand::Operate(command) => operate(command).await,
        }
    }
}

async fn operate(
    command: WorkspaceModuleOperateCommand<'_>,
) -> Result<WorkspaceModuleOperationOutcome, AgentToolError> {
    let operation = command.operation.trim().to_string();
    let Some(current_user) = command.current_user else {
        return Err(AgentToolError::ExecutionFailed(
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
                    AgentToolError::InvalidArguments(format!(
                        "invalid canvas.create input: {error}"
                    ))
                })?;
            operate_create_personal_canvas_for_workspace_module(
                &repos,
                command.project_id,
                current_user,
                command.vfs,
                command.agent_run_bridge_handle,
                command.delivery_runtime_session_id,
                params,
            )
            .await?
        }
        "canvas.attach" => {
            let params: AttachExistingCanvasModuleInput = serde_json::from_value(command.input)
                .map_err(|error| {
                    AgentToolError::InvalidArguments(format!(
                        "invalid canvas.attach input: {error}"
                    ))
                })?;
            operate_attach_existing_canvas_for_workspace_module(
                &repos,
                command.project_id,
                current_user,
                command.vfs,
                command.agent_run_bridge_handle,
                command.delivery_runtime_session_id,
                params,
            )
            .await?
        }
        "canvas.copy" => {
            let params: CopyCanvasToPersonalModuleInput = serde_json::from_value(command.input)
                .map_err(|error| {
                    AgentToolError::InvalidArguments(format!("invalid canvas.copy input: {error}"))
                })?;
            operate_copy_canvas_to_personal_for_workspace_module(
                &repos,
                command.project_id,
                current_user,
                command.vfs,
                command.agent_run_bridge_handle,
                command.delivery_runtime_session_id,
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

async fn reproject_canvas_modules_for_access(
    canvas_repo: &Arc<dyn CanvasRepository>,
    project_id: Uuid,
    modules: Vec<WorkspaceModuleDescriptor>,
    current_user: Option<&ProjectAuthorizationContext>,
) -> Result<Vec<WorkspaceModuleDescriptor>, AgentToolError> {
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

pub(crate) async fn load_canvas_by_project_mount_id_for_tool(
    canvas_repo: &dyn CanvasRepository,
    project_id: Uuid,
    raw_canvas_mount_id: &str,
) -> Result<Canvas, AgentToolError> {
    let canvas_mount_id = normalize_canvas_mount_id(raw_canvas_mount_id)
        .map_err(|error| AgentToolError::InvalidArguments(error.to_string()))?;
    let canvas = canvas_repo
        .get_by_mount_id(project_id, &canvas_mount_id)
        .await
        .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;
    canvas
        .ok_or_else(|| AgentToolError::ExecutionFailed(format!("Canvas 不存在: {canvas_mount_id}")))
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
    vfs: &SharedRuntimeVfs,
    agent_run_bridge_handle: &SharedWorkspaceModuleAgentRunBridgeHandle,
    delivery_runtime_session_id: Option<&str>,
    params: CreatePersonalCanvasModuleInput,
) -> Result<(Canvas, WorkspaceModuleCanvasToolResult), AgentToolError> {
    let title = params
        .title
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            AgentToolError::InvalidArguments("title is required for canvas.create".to_string())
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
    .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?
    .canvas;
    expose_canvas_for_workspace_module(
        vfs,
        agent_run_bridge_handle,
        delivery_runtime_session_id,
        current_user,
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
    vfs: &SharedRuntimeVfs,
    agent_run_bridge_handle: &SharedWorkspaceModuleAgentRunBridgeHandle,
    delivery_runtime_session_id: Option<&str>,
    params: AttachExistingCanvasModuleInput,
) -> Result<(Canvas, WorkspaceModuleCanvasToolResult), AgentToolError> {
    let canvas_mount_id = required_canvas_mount_id(
        params.canvas_mount_id.as_deref(),
        "canvas.attach input.canvas_mount_id",
    )?;
    let canvas = load_canvas_by_project_mount_id(repos, project_id, &canvas_mount_id)
        .await
        .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;
    ensure_canvas_visible_to_current_user(&canvas, current_user)?;
    expose_canvas_for_workspace_module(
        vfs,
        agent_run_bridge_handle,
        delivery_runtime_session_id,
        current_user,
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
    vfs: &SharedRuntimeVfs,
    agent_run_bridge_handle: &SharedWorkspaceModuleAgentRunBridgeHandle,
    delivery_runtime_session_id: Option<&str>,
    params: CopyCanvasToPersonalModuleInput,
) -> Result<(Canvas, WorkspaceModuleCanvasToolResult), AgentToolError> {
    let source_mount_id = required_canvas_mount_id(
        params.source_mount_id.as_deref(),
        "canvas.copy input.source_mount_id",
    )?;
    let source = load_canvas_by_project_mount_id(repos, project_id, &source_mount_id)
        .await
        .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;
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
    .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?
    .canvas;
    expose_canvas_for_workspace_module(
        vfs,
        agent_run_bridge_handle,
        delivery_runtime_session_id,
        current_user,
        copy,
        "copied",
        CanvasVisibilityReason::Created,
    )
    .await
}

async fn expose_canvas_for_workspace_module(
    vfs: &SharedRuntimeVfs,
    agent_run_bridge_handle: &SharedWorkspaceModuleAgentRunBridgeHandle,
    delivery_runtime_session_id: Option<&str>,
    current_user: &ProjectAuthorizationContext,
    canvas: Canvas,
    action: &str,
    reason: CanvasVisibilityReason,
) -> Result<(Canvas, WorkspaceModuleCanvasToolResult), AgentToolError> {
    submit_canvas_runtime_surface_update(
        Some(vfs),
        agent_run_bridge_handle,
        delivery_runtime_session_id,
        Some(current_user),
        &canvas,
        RuntimeSurfaceUpdateRequest::CanvasVisibilityRequested {
            canvas_mount_id: canvas.mount_id.clone(),
            reason,
        },
    )
    .await?;

    let result = WorkspaceModuleCanvasToolResult {
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
) -> Result<String, AgentToolError> {
    let raw = value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AgentToolError::InvalidArguments(format!("{field_name} is required")))?;
    normalize_canvas_mount_id(raw)
        .map_err(|error| AgentToolError::InvalidArguments(error.to_string()))
}

fn ensure_canvas_visible_to_current_user(
    canvas: &Canvas,
    current_user: &ProjectAuthorizationContext,
) -> Result<(), AgentToolError> {
    let access = canvas_access_for_workspace_module(canvas, current_user);
    if access.allows(CanvasAccessAction::View) {
        Ok(())
    } else {
        Err(AgentToolError::ExecutionFailed(format!(
            "当前用户无权查看 Canvas {}",
            canvas.id
        )))
    }
}

impl WorkspaceModuleCommandDiagnostic {
    pub(crate) fn into_tool_result(self) -> agentdash_spi::AgentToolResult {
        let mut details = self.details;
        if let Some(obj) = details.as_object_mut() {
            obj.insert("error".to_string(), serde_json::json!(self.code));
            obj.insert(
                "message".to_string(),
                serde_json::json!(self.message.clone()),
            );
        }
        agentdash_spi::AgentToolResult {
            content: vec![ContentPart::text(self.message)],
            is_error: true,
            details: Some(details),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use agentdash_application_ports::agent_run_surface::AgentRunGrantProjection;
    use agentdash_application_ports::runtime_surface_adoption::AgentFrameRuntimeTarget;
    use agentdash_domain::DomainError;
    use agentdash_domain::canvas::{Canvas, CanvasRepository};
    use agentdash_domain::project::ProjectAuthorizationContext;
    use agentdash_domain::shared_library::{
        ProjectExtensionInstallation, ProjectExtensionInstallationRepository,
    };
    use agentdash_spi::{CapabilityState, ToolCluster, WorkspaceModuleDimension};
    use async_trait::async_trait;

    use super::*;
    use crate::canvas::build_canvas;

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
    struct FakeCanvasRepo {
        canvases: Mutex<HashMap<(Uuid, String), Canvas>>,
    }

    #[async_trait]
    impl CanvasRepository for FakeCanvasRepo {
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

    #[tokio::test]
    async fn resolve_reprojects_canvas_descriptor_with_current_user_access() {
        let project_id = Uuid::new_v4();
        let installation_repo: Arc<dyn ProjectExtensionInstallationRepository> =
            Arc::new(EmptyInstallationRepo);
        let canvas_repo = Arc::new(FakeCanvasRepo::default());
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
}
