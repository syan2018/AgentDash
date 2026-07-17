use agentdash_agent_runtime_contract::{
    IdempotencyKey, ImmutablePresentationEvent, PresentationDurability,
    RuntimePresentationAppendRequest, RuntimePresentationCoordinate, RuntimePresentationInput,
    RuntimeThreadId,
};
use agentdash_application_ports::agent_frame_materialization::RuntimeSurfaceUpdateRequest;
use agentdash_application_runtime_gateway::{RuntimeActor, RuntimeContext};
use agentdash_application_vfs::tools::SharedRuntimeVfs;
use agentdash_domain::canvas::Canvas;
use agentdash_domain::project::ProjectAuthorizationContext;
use uuid::Uuid;

use super::runtime_bridge::{
    ResolvedInvocationBackend, SharedWorkspaceModuleAgentRunBridgeHandle,
    SharedWorkspaceModulePresentationAppendHandle, WorkspaceModuleRuntimeBridgeError,
    submit_canvas_runtime_surface_update,
};

#[derive(Clone)]
pub(crate) struct WorkspaceModuleRuntimeContext {
    project_id: Uuid,
    runtime_thread_id: RuntimeThreadId,
    agent_id: Option<String>,
    vfs: Option<SharedRuntimeVfs>,
    current_user: Option<ProjectAuthorizationContext>,
    agent_run_bridge_handle: Option<SharedWorkspaceModuleAgentRunBridgeHandle>,
    presentation_append_handle: Option<SharedWorkspaceModulePresentationAppendHandle>,
    tool_call_id: Option<String>,
    backend: Option<ResolvedInvocationBackend>,
}

impl WorkspaceModuleRuntimeContext {
    pub(crate) fn new(project_id: Uuid, runtime_thread_id: impl Into<String>) -> Self {
        Self {
            project_id,
            runtime_thread_id: RuntimeThreadId::new(runtime_thread_id)
                .expect("workspace module runtime thread id must not be empty"),
            agent_id: None,
            vfs: None,
            current_user: None,
            agent_run_bridge_handle: None,
            presentation_append_handle: None,
            tool_call_id: None,
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

    pub(crate) fn with_presentation_append(
        mut self,
        handle: SharedWorkspaceModulePresentationAppendHandle,
        tool_call_id: impl Into<String>,
    ) -> Self {
        self.presentation_append_handle = Some(handle);
        self.tool_call_id = Some(tool_call_id.into());
        self
    }

    pub(crate) fn with_backend(mut self, backend: Option<ResolvedInvocationBackend>) -> Self {
        self.backend = backend;
        self
    }

    pub(crate) fn runtime_thread_id(&self) -> &str {
        self.runtime_thread_id.as_str()
    }

    pub(crate) fn current_user(&self) -> Option<&ProjectAuthorizationContext> {
        self.current_user.as_ref()
    }

    pub(crate) fn backend(&self) -> Option<&ResolvedInvocationBackend> {
        self.backend.as_ref()
    }

    pub(crate) fn runtime_actor(&self) -> RuntimeActor {
        RuntimeActor::AgentSession {
            session_id: self.runtime_thread_id.to_string(),
            agent_id: self.agent_id.clone(),
        }
    }

    pub(crate) fn runtime_context(&self) -> RuntimeContext {
        RuntimeContext::Session {
            session_id: self.runtime_thread_id.to_string(),
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
            Some(self.runtime_thread_id()),
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

    pub(crate) async fn append_presentation_event(
        &self,
        binding: &agentdash_application_ports::agent_run_runtime::AgentRunRuntimeBinding,
        turn_id: &str,
        event_kind: &str,
        event: agentdash_agent_protocol::BackboneEvent,
    ) -> Result<(), WorkspaceModuleRuntimeBridgeError> {
        let handle = self.presentation_append_handle.as_ref().ok_or_else(|| {
            WorkspaceModuleRuntimeBridgeError::ExecutionFailed(
                "Workspace module canonical presentation append port 尚未完成初始化".to_string(),
            )
        })?;
        let tool_call_id = self.tool_call_id.as_deref().ok_or_else(|| {
            WorkspaceModuleRuntimeBridgeError::ExecutionFailed(
                "Workspace module presentation producer 缺少 canonical tool call identity"
                    .to_string(),
            )
        })?;
        let runtime_turn_id = agentdash_agent_runtime_contract::RuntimeTurnId::new(turn_id)
            .map_err(|error| {
                WorkspaceModuleRuntimeBridgeError::ExecutionFailed(error.to_string())
            })?;
        let runtime_item_id = agentdash_agent_runtime_contract::RuntimeItemId::new(tool_call_id)
            .map_err(|error| {
                WorkspaceModuleRuntimeBridgeError::ExecutionFailed(error.to_string())
            })?;
        let idempotency_key = IdempotencyKey::new(format!(
            "workspace-module-presentation:{runtime_turn_id}:{runtime_item_id}:{event_kind}"
        ))
        .map_err(|error| WorkspaceModuleRuntimeBridgeError::ExecutionFailed(error.to_string()))?;
        handle
            .append_presentation(RuntimePresentationAppendRequest {
                runtime_thread_id: binding.thread_id.clone(),
                producer: "workspace_module.presentation".to_string(),
                idempotency_key,
                events: vec![RuntimePresentationInput {
                    coordinate: RuntimePresentationCoordinate {
                        runtime_turn_id: Some(runtime_turn_id),
                        presentation_turn_id: None,
                        runtime_item_id: Some(runtime_item_id),
                        interaction_id: None,
                        source_thread_id: None,
                        source_turn_id: None,
                        source_item_id: None,
                        source_request_id: None,
                        source_entry_index: None,
                    },
                    event: ImmutablePresentationEvent::new(PresentationDurability::Durable, event),
                }],
            })
            .await
            .map(|_| ())
            .map_err(WorkspaceModuleRuntimeBridgeError::ExecutionFailed)
    }
}
