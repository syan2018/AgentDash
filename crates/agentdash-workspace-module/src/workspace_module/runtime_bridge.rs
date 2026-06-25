use std::sync::Arc;

use agentdash_agent_protocol::BackboneEnvelope;
use agentdash_application_ports::agent_frame_materialization::RuntimeSurfaceUpdateRequest;
use agentdash_application_ports::agent_run_surface::AgentRunEffectiveCapabilityView;
use agentdash_application_runtime_gateway::{ExtensionInvocationWorkspaceContext, RuntimeGateway};
use agentdash_application_vfs::tools::SharedRuntimeVfs;
use agentdash_domain::backend::RuntimeBackendAnchor;
use agentdash_domain::canvas::{Canvas, CanvasRepository};
use agentdash_domain::common::Vfs;
use agentdash_domain::project::ProjectAuthorizationContext;
use agentdash_spi::{AgentToolError, AuthIdentity, ConnectorError, ExecutionContext};
use async_trait::async_trait;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::canvas::normalize_canvas_mount_id;

#[derive(Clone, Default)]
pub struct SharedWorkspaceModuleRuntimeGatewayHandle {
    inner: Arc<RwLock<Option<Arc<RuntimeGateway>>>>,
}

impl SharedWorkspaceModuleRuntimeGatewayHandle {
    pub async fn set(&self, gateway: Arc<RuntimeGateway>) {
        *self.inner.write().await = Some(gateway);
    }

    pub async fn get(&self) -> Option<Arc<RuntimeGateway>> {
        self.inner.read().await.clone()
    }
}

#[async_trait]
pub trait WorkspaceModuleAgentRunBridge: Send + Sync {
    async fn effective_capability_view_for_agent_run_delivery(
        &self,
        delivery_runtime_session_id: &str,
    ) -> Result<AgentRunEffectiveCapabilityView, String>;

    async fn expose_canvas_mount_to_agent_run(
        &self,
        delivery_runtime_session_id: &str,
        canvas: &Canvas,
        current_user: Option<&ProjectAuthorizationContext>,
    ) -> Result<Vfs, String>;

    async fn inject_agent_run_notification(
        &self,
        delivery_runtime_session_id: &str,
        notification: BackboneEnvelope,
    ) -> Result<(), String>;
}

#[derive(Clone, Default)]
pub struct SharedWorkspaceModuleAgentRunBridgeHandle {
    inner: Arc<RwLock<Option<Arc<dyn WorkspaceModuleAgentRunBridge>>>>,
}

impl SharedWorkspaceModuleAgentRunBridgeHandle {
    pub async fn set(&self, bridge: Arc<dyn WorkspaceModuleAgentRunBridge>) {
        let mut guard = self.inner.write().await;
        *guard = Some(bridge);
    }

    pub async fn get(&self) -> Option<Arc<dyn WorkspaceModuleAgentRunBridge>> {
        self.inner.read().await.clone()
    }
}

pub fn project_authorization_context_from_identity(
    identity: &AuthIdentity,
) -> ProjectAuthorizationContext {
    ProjectAuthorizationContext::new(
        identity.user_id.clone(),
        identity
            .groups
            .iter()
            .map(|group| group.group_id.clone())
            .collect(),
        identity.is_admin,
    )
}

pub fn shared_runtime_vfs_from_context(
    context: &ExecutionContext,
) -> Result<SharedRuntimeVfs, ConnectorError> {
    let vfs = context.session.vfs.clone().ok_or_else(|| {
        ConnectorError::InvalidConfig("缺少 vfs，无法构建统一访问工具".to_string())
    })?;
    Ok(SharedRuntimeVfs::new(vfs))
}

pub fn delivery_runtime_session_id_from_context(context: &ExecutionContext) -> String {
    context
        .turn
        .hook_runtime
        .as_ref()
        .map(|session| session.session_id().to_string())
        .unwrap_or_else(|| context.session.turn_id.clone())
}

pub fn project_id_from_context(context: &ExecutionContext) -> Option<Uuid> {
    if let Some(hook_runtime) = context.turn.hook_runtime.as_ref() {
        let snapshot = hook_runtime.snapshot();

        if let Some(run_context) = &snapshot.run_context {
            return Some(run_context.project_id);
        }
    }

    context
        .session
        .vfs
        .as_ref()
        .and_then(|space| space.source_project_id.as_deref())
        .and_then(|project_id| Uuid::parse_str(project_id).ok())
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedInvocationBackend {
    pub backend_id: String,
    pub workspace: Option<ExtensionInvocationWorkspaceContext>,
}

pub fn resolve_invocation_backend(
    vfs: Option<&Vfs>,
    runtime_backend_anchor: Option<&RuntimeBackendAnchor>,
) -> Option<ResolvedInvocationBackend> {
    let anchor = runtime_backend_anchor?;
    let backend_id = anchor.backend_id().to_string();
    let workspace = vfs.and_then(|vfs| select_invocation_workspace(vfs, anchor));
    Some(ResolvedInvocationBackend {
        backend_id,
        workspace,
    })
}

fn select_invocation_workspace(
    vfs: &Vfs,
    anchor: &RuntimeBackendAnchor,
) -> Option<ExtensionInvocationWorkspaceContext> {
    if let Some(root_ref) = anchor
        .root_ref
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return vfs
            .mounts
            .iter()
            .find(|mount| mount.root_ref.trim() == root_ref && !mount.root_ref.trim().is_empty())
            .map(|mount| {
                ExtensionInvocationWorkspaceContext::new(
                    mount.id.clone(),
                    mount.root_ref.trim().to_string(),
                )
            });
    }
    vfs.default_mount()
        .filter(|mount| !mount.root_ref.trim().is_empty())
        .map(|mount| {
            ExtensionInvocationWorkspaceContext::new(
                mount.id.clone(),
                mount.root_ref.trim().to_string(),
            )
        })
}

pub async fn submit_canvas_runtime_surface_update(
    vfs: Option<&SharedRuntimeVfs>,
    agent_run_bridge_handle: &SharedWorkspaceModuleAgentRunBridgeHandle,
    delivery_runtime_session_id: Option<&str>,
    current_user: Option<&ProjectAuthorizationContext>,
    canvas: &Canvas,
    request: RuntimeSurfaceUpdateRequest,
) -> Result<(), AgentToolError> {
    ensure_canvas_surface_request_targets_canvas(&request, canvas)?;
    let bridge = agent_run_bridge_handle.get().await.ok_or_else(|| {
        AgentToolError::ExecutionFailed(format!(
            "Workspace module AgentRun bridge 尚未完成初始化，无法提交 Canvas runtime surface request: {request:?}"
        ))
    })?;
    let delivery_runtime_session_id = delivery_runtime_session_id.ok_or_else(|| {
        AgentToolError::ExecutionFailed(format!(
            "当前工具调用缺少 AgentRun delivery runtime id，无法提交 Canvas runtime surface request: {request:?}"
        ))
    })?;
    let active_vfs = bridge
        .expose_canvas_mount_to_agent_run(delivery_runtime_session_id, canvas, current_user)
        .await
        .map_err(|error| {
            AgentToolError::ExecutionFailed(format!(
                "Canvas runtime surface request `{request:?}` 写入 AgentFrame 失败: {error}"
            ))
        })?;
    if let Some(vfs) = vfs {
        vfs.replace(active_vfs).await;
    }
    Ok(())
}

pub async fn request_existing_canvas_visibility_for_runtime(
    canvas_repo: &dyn CanvasRepository,
    project_id: Uuid,
    canvas_mount_id: &str,
    vfs: Option<&SharedRuntimeVfs>,
    agent_run_bridge_handle: &SharedWorkspaceModuleAgentRunBridgeHandle,
    delivery_runtime_session_id: Option<&str>,
    current_user: Option<&ProjectAuthorizationContext>,
) -> Result<Canvas, AgentToolError> {
    let canvas = load_canvas_by_project_mount_id(canvas_repo, project_id, canvas_mount_id).await?;
    submit_canvas_runtime_surface_update(
        vfs,
        agent_run_bridge_handle,
        delivery_runtime_session_id,
        current_user,
        &canvas,
        RuntimeSurfaceUpdateRequest::CanvasVisibilityRequested {
            canvas_mount_id: canvas.mount_id.clone(),
            reason: agentdash_application_ports::agent_frame_materialization::CanvasVisibilityReason::Presented,
        },
    )
    .await?;
    Ok(canvas)
}

fn ensure_canvas_surface_request_targets_canvas(
    request: &RuntimeSurfaceUpdateRequest,
    canvas: &Canvas,
) -> Result<(), AgentToolError> {
    let canvas_mount_id = match request {
        RuntimeSurfaceUpdateRequest::CanvasBindingChanged { canvas_mount_id }
        | RuntimeSurfaceUpdateRequest::CanvasVisibilityRequested {
            canvas_mount_id, ..
        } => canvas_mount_id,
        _ => {
            return Err(AgentToolError::ExecutionFailed(format!(
                "Canvas adapter received non-Canvas runtime surface request: {request:?}"
            )));
        }
    };
    if canvas_mount_id == &canvas.mount_id {
        Ok(())
    } else {
        Err(AgentToolError::ExecutionFailed(format!(
            "Canvas runtime surface request target `{canvas_mount_id}` does not match Canvas `{}`",
            canvas.mount_id
        )))
    }
}

async fn load_canvas_by_project_mount_id(
    canvas_repo: &dyn CanvasRepository,
    expected_project_id: Uuid,
    raw_canvas_mount_id: &str,
) -> Result<Canvas, AgentToolError> {
    let canvas_mount_id = normalize_canvas_mount_id(raw_canvas_mount_id)
        .map_err(|error| AgentToolError::InvalidArguments(error.to_string()))?;

    let canvas = canvas_repo
        .get_by_mount_id(expected_project_id, &canvas_mount_id)
        .await
        .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;
    let canvas = canvas.ok_or_else(|| {
        AgentToolError::ExecutionFailed(format!("Canvas 不存在: {canvas_mount_id}"))
    })?;
    if canvas.project_id != expected_project_id {
        return Err(AgentToolError::ExecutionFailed(
            "当前 session 无权操作其它 Project 的 Canvas".to_string(),
        ));
    }
    Ok(canvas)
}
