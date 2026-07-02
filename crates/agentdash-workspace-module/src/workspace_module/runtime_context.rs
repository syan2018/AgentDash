use agentdash_agent_protocol::BackboneEnvelope;
use agentdash_application_ports::agent_frame_materialization::RuntimeSurfaceUpdateRequest;
use agentdash_application_runtime_gateway::{RuntimeActor, RuntimeContext};
use agentdash_application_vfs::tools::SharedRuntimeVfs;
use agentdash_domain::canvas::{Canvas, CanvasRepository};
use agentdash_domain::project::ProjectAuthorizationContext;
use uuid::Uuid;

use super::runtime_bridge::{
    ResolvedInvocationBackend, SharedWorkspaceModuleAgentRunBridgeHandle,
    WorkspaceModuleRuntimeBridgeError, request_existing_canvas_visibility_for_runtime,
    submit_canvas_runtime_surface_update,
};

#[derive(Clone)]
pub(crate) struct WorkspaceModuleRuntimeContext {
    project_id: Uuid,
    delivery_runtime_session_id: String,
    agent_id: Option<String>,
    vfs: Option<SharedRuntimeVfs>,
    current_user: Option<ProjectAuthorizationContext>,
    agent_run_bridge_handle: Option<SharedWorkspaceModuleAgentRunBridgeHandle>,
    backend: Option<ResolvedInvocationBackend>,
}

impl WorkspaceModuleRuntimeContext {
    pub(crate) fn new(project_id: Uuid, delivery_runtime_session_id: impl Into<String>) -> Self {
        Self {
            project_id,
            delivery_runtime_session_id: delivery_runtime_session_id.into(),
            agent_id: None,
            vfs: None,
            current_user: None,
            agent_run_bridge_handle: None,
            backend: None,
        }
    }

    pub(crate) fn with_agent_id(mut self, agent_id: Option<String>) -> Self {
        self.agent_id = agent_id;
        self
    }

    pub(crate) fn with_vfs(mut self, vfs: SharedRuntimeVfs) -> Self {
        self.vfs = Some(vfs);
        self
    }

    pub(crate) fn with_current_user(
        mut self,
        current_user: Option<ProjectAuthorizationContext>,
    ) -> Self {
        self.current_user = current_user;
        self
    }

    pub(crate) fn with_agent_run_bridge(
        mut self,
        agent_run_bridge_handle: Option<SharedWorkspaceModuleAgentRunBridgeHandle>,
    ) -> Self {
        self.agent_run_bridge_handle = agent_run_bridge_handle;
        self
    }

    pub(crate) fn with_backend(mut self, backend: Option<ResolvedInvocationBackend>) -> Self {
        self.backend = backend;
        self
    }

    pub(crate) fn delivery_runtime_session_id(&self) -> &str {
        &self.delivery_runtime_session_id
    }

    pub(crate) fn current_user(&self) -> Option<&ProjectAuthorizationContext> {
        self.current_user.as_ref()
    }

    pub(crate) fn backend(&self) -> Option<&ResolvedInvocationBackend> {
        self.backend.as_ref()
    }

    pub(crate) fn runtime_actor(&self) -> RuntimeActor {
        RuntimeActor::AgentSession {
            session_id: self.delivery_runtime_session_id.clone(),
            agent_id: self.agent_id.clone(),
        }
    }

    pub(crate) fn runtime_context(&self) -> RuntimeContext {
        RuntimeContext::Session {
            session_id: self.delivery_runtime_session_id.clone(),
            project_id: Some(self.project_id),
            workspace_id: None,
        }
    }

    pub(crate) async fn submit_canvas_surface_update(
        &self,
        canvas: &Canvas,
        request: RuntimeSurfaceUpdateRequest,
    ) -> Result<(), WorkspaceModuleRuntimeBridgeError> {
        let handle = self.agent_run_bridge_handle.as_ref().ok_or_else(|| {
            WorkspaceModuleRuntimeBridgeError::ExecutionFailed(format!(
                "Workspace module AgentRun bridge 尚未完成初始化，无法提交 Canvas runtime surface request: {request:?}"
            ))
        })?;
        submit_canvas_runtime_surface_update(
            self.vfs.as_ref(),
            handle,
            Some(self.delivery_runtime_session_id()),
            self.current_user(),
            canvas,
            request,
        )
        .await
    }

    pub(crate) async fn submit_optional_canvas_surface_update(
        &self,
        canvas: &Canvas,
        request: RuntimeSurfaceUpdateRequest,
    ) -> Result<(), WorkspaceModuleRuntimeBridgeError> {
        if self.agent_run_bridge_handle.is_none() {
            return Ok(());
        }
        self.submit_canvas_surface_update(canvas, request).await
    }

    pub(crate) async fn request_existing_canvas_visibility(
        &self,
        canvas_repo: &dyn CanvasRepository,
        canvas_mount_id: &str,
    ) -> Result<Canvas, WorkspaceModuleRuntimeBridgeError> {
        let handle = self.agent_run_bridge_handle.as_ref().ok_or_else(|| {
            WorkspaceModuleRuntimeBridgeError::ExecutionFailed(
                "Workspace module AgentRun bridge 尚未完成初始化".to_string(),
            )
        })?;
        request_existing_canvas_visibility_for_runtime(
            canvas_repo,
            self.project_id,
            canvas_mount_id,
            self.vfs.as_ref(),
            handle,
            Some(self.delivery_runtime_session_id()),
            self.current_user(),
        )
        .await
    }

    pub(crate) async fn inject_agent_run_notification(
        &self,
        notification: BackboneEnvelope,
    ) -> Result<(), WorkspaceModuleRuntimeBridgeError> {
        let handle = self.agent_run_bridge_handle.as_ref().ok_or_else(|| {
            WorkspaceModuleRuntimeBridgeError::ExecutionFailed(
                "Workspace module AgentRun bridge 尚未完成初始化".to_string(),
            )
        })?;
        let bridge = handle.get().await.ok_or_else(|| {
            WorkspaceModuleRuntimeBridgeError::ExecutionFailed(
                "Workspace module AgentRun bridge 尚未完成初始化".to_string(),
            )
        })?;
        bridge
            .inject_agent_run_notification(self.delivery_runtime_session_id(), notification)
            .await
            .map_err(WorkspaceModuleRuntimeBridgeError::ExecutionFailed)
    }
}
