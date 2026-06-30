use std::sync::Arc;

use agentdash_agent_protocol::BackboneEnvelope;
use agentdash_application_ports::agent_frame_materialization::RuntimeSurfaceUpdateRequest;
use agentdash_application_ports::agent_run_surface::AgentRunEffectiveCapabilityView;
use agentdash_application_runtime_gateway::{
    ExtensionInvocationWorkspaceContext, RuntimeGateway, resolve_extension_invocation_workspace,
};
use agentdash_application_vfs::tools::{RuntimeVfsState, SharedRuntimeVfs};
use agentdash_domain::backend::RuntimeBackendAnchor;
use agentdash_domain::canvas::{Canvas, CanvasRepository};
use agentdash_domain::common::Vfs;
use agentdash_domain::project::ProjectAuthorizationContext;
use agentdash_spi::{AuthIdentity, ConnectorError, ExecutionContext};
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

    async fn apply_canvas_runtime_surface_update_to_agent_run(
        &self,
        delivery_runtime_session_id: &str,
        canvas: &Canvas,
        current_user: Option<&ProjectAuthorizationContext>,
        request: RuntimeSurfaceUpdateRequest,
    ) -> Result<RuntimeVfsState, String>;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceModuleRuntimeBridgeError {
    InvalidArguments(String),
    ExecutionFailed(String),
}

impl std::fmt::Display for WorkspaceModuleRuntimeBridgeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidArguments(message) | Self::ExecutionFailed(message) => {
                formatter.write_str(message)
            }
        }
    }
}

impl std::error::Error for WorkspaceModuleRuntimeBridgeError {}

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
    let access_policy = context.session.vfs_access_policy.clone().ok_or_else(|| {
        ConnectorError::InvalidConfig(
            "缺少 vfs_access_policy，无法构建 workspace module VFS 工具".to_string(),
        )
    })?;
    Ok(SharedRuntimeVfs::new_with_policy(vfs, access_policy))
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
    let workspace =
        vfs.and_then(|vfs| resolve_extension_invocation_workspace(vfs, anchor).into_workspace());
    Some(ResolvedInvocationBackend {
        backend_id,
        workspace,
    })
}

pub async fn submit_canvas_runtime_surface_update(
    vfs: Option<&SharedRuntimeVfs>,
    agent_run_bridge_handle: &SharedWorkspaceModuleAgentRunBridgeHandle,
    delivery_runtime_session_id: Option<&str>,
    current_user: Option<&ProjectAuthorizationContext>,
    canvas: &Canvas,
    request: RuntimeSurfaceUpdateRequest,
) -> Result<(), WorkspaceModuleRuntimeBridgeError> {
    ensure_canvas_surface_request_targets_canvas(&request, canvas)?;
    let bridge = agent_run_bridge_handle.get().await.ok_or_else(|| {
        WorkspaceModuleRuntimeBridgeError::ExecutionFailed(format!(
            "Workspace module AgentRun bridge 尚未完成初始化，无法提交 Canvas runtime surface request: {request:?}"
        ))
    })?;
    let delivery_runtime_session_id = delivery_runtime_session_id.ok_or_else(|| {
        WorkspaceModuleRuntimeBridgeError::ExecutionFailed(format!(
            "当前工具调用缺少 AgentRun delivery runtime id，无法提交 Canvas runtime surface request: {request:?}"
        ))
    })?;
    let active_vfs_state = bridge
        .apply_canvas_runtime_surface_update_to_agent_run(
            delivery_runtime_session_id,
            canvas,
            current_user,
            request.clone(),
        )
        .await
        .map_err(|error| {
            WorkspaceModuleRuntimeBridgeError::ExecutionFailed(format!(
                "Canvas runtime surface request `{request:?}` 写入 AgentFrame 失败: {error}"
            ))
        })?;
    if let Some(vfs) = vfs {
        vfs.replace_with_policy(active_vfs_state.vfs, active_vfs_state.access_policy)
            .await;
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
) -> Result<Canvas, WorkspaceModuleRuntimeBridgeError> {
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
) -> Result<(), WorkspaceModuleRuntimeBridgeError> {
    let canvas_mount_id = match request {
        RuntimeSurfaceUpdateRequest::CanvasBindingChanged {
            canvas_mount_id, ..
        }
        | RuntimeSurfaceUpdateRequest::CanvasVisibilityRequested {
            canvas_mount_id, ..
        } => canvas_mount_id,
        _ => {
            return Err(WorkspaceModuleRuntimeBridgeError::ExecutionFailed(format!(
                "Canvas adapter received non-Canvas runtime surface request: {request:?}"
            )));
        }
    };
    if canvas_mount_id == &canvas.mount_id {
        Ok(())
    } else {
        Err(WorkspaceModuleRuntimeBridgeError::ExecutionFailed(format!(
            "Canvas runtime surface request target `{canvas_mount_id}` does not match Canvas `{}`",
            canvas.mount_id
        )))
    }
}

async fn load_canvas_by_project_mount_id(
    canvas_repo: &dyn CanvasRepository,
    expected_project_id: Uuid,
    raw_canvas_mount_id: &str,
) -> Result<Canvas, WorkspaceModuleRuntimeBridgeError> {
    let canvas_mount_id = normalize_canvas_mount_id(raw_canvas_mount_id)
        .map_err(|error| WorkspaceModuleRuntimeBridgeError::InvalidArguments(error.to_string()))?;

    let canvas = canvas_repo
        .get_by_mount_id(expected_project_id, &canvas_mount_id)
        .await
        .map_err(|error| WorkspaceModuleRuntimeBridgeError::ExecutionFailed(error.to_string()))?;
    let canvas = canvas.ok_or_else(|| {
        WorkspaceModuleRuntimeBridgeError::ExecutionFailed(format!(
            "Canvas 不存在: {canvas_mount_id}"
        ))
    })?;
    if canvas.project_id != expected_project_id {
        return Err(WorkspaceModuleRuntimeBridgeError::ExecutionFailed(
            "当前 session 无权操作其它 Project 的 Canvas".to_string(),
        ));
    }
    Ok(canvas)
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeSet, HashMap},
        path::PathBuf,
        sync::Arc,
    };

    use agentdash_application_ports::agent_frame_materialization::CanvasVisibilityReason;
    use agentdash_application_vfs::tools::RuntimeVfsState;
    use agentdash_domain::canvas::Canvas;
    use agentdash_spi::{
        AgentConfig, CapabilityState, ExecutionSessionFrame, ExecutionTurnFrame, Mount,
        MountCapability, RuntimeVfsAccessPolicy, RuntimeVfsAccessRule, RuntimeVfsAccessSource,
        RuntimeVfsOperation, RuntimeVfsPathPattern, Vfs,
    };
    use async_trait::async_trait;

    use super::*;

    fn mount(id: &str) -> Mount {
        Mount {
            id: id.to_string(),
            provider: "memory".to_string(),
            backend_id: String::new(),
            root_ref: format!("memory://{id}"),
            capabilities: vec![MountCapability::Read, MountCapability::List],
            default_write: false,
            display_name: id.to_string(),
            metadata: serde_json::Value::Null,
        }
    }

    fn vfs(mounts: Vec<Mount>) -> Vfs {
        Vfs {
            default_mount_id: mounts.first().map(|mount| mount.id.clone()),
            mounts,
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        }
    }

    fn docs_only_policy() -> RuntimeVfsAccessPolicy {
        RuntimeVfsAccessPolicy {
            rules: vec![RuntimeVfsAccessRule {
                mount_id: "main".to_string(),
                path_pattern: RuntimeVfsPathPattern::Prefix("docs".to_string()),
                operations: BTreeSet::from([RuntimeVfsOperation::Read]),
                source: RuntimeVfsAccessSource::PermissionGrant,
            }],
        }
    }

    fn execution_context(vfs: Vfs, access_policy: RuntimeVfsAccessPolicy) -> ExecutionContext {
        ExecutionContext {
            session: ExecutionSessionFrame {
                turn_id: "turn-1".to_string(),
                working_directory: PathBuf::from("."),
                environment_variables: HashMap::new(),
                executor_config: AgentConfig::default(),
                mcp_servers: Vec::new(),
                vfs: Some(vfs),
                vfs_access_policy: Some(access_policy),
                backend_execution: None,
                runtime_backend_anchor: None,
                identity: None,
            },
            turn: ExecutionTurnFrame {
                capability_state: CapabilityState::default(),
                ..Default::default()
            },
        }
    }

    #[test]
    fn shared_runtime_vfs_from_context_requires_access_policy() {
        let mut context = execution_context(vfs(vec![mount("main")]), docs_only_policy());
        context.session.vfs_access_policy = None;

        let error = match shared_runtime_vfs_from_context(&context) {
            Ok(_) => panic!("policy is required"),
            Err(error) => error,
        };

        assert!(
            matches!(error, ConnectorError::InvalidConfig(message) if message.contains("vfs_access_policy"))
        );
    }

    #[tokio::test]
    async fn shared_runtime_vfs_from_context_preserves_session_policy() {
        let context = execution_context(vfs(vec![mount("main")]), docs_only_policy());

        let shared = shared_runtime_vfs_from_context(&context).expect("shared VFS");
        let state = shared.snapshot_state().await;

        assert_eq!(state.vfs.mounts[0].id, "main");
        assert!(
            state
                .access_policy
                .admits("main", "docs/readme.md", RuntimeVfsOperation::Read)
        );
        assert!(
            !state
                .access_policy
                .admits("main", "src/lib.rs", RuntimeVfsOperation::Read),
            "workspace module bridge must not widen policy to whole-mount access"
        );
    }

    #[derive(Clone)]
    struct ReturningCanvasBridge {
        state: RuntimeVfsState,
    }

    #[async_trait]
    impl WorkspaceModuleAgentRunBridge for ReturningCanvasBridge {
        async fn effective_capability_view_for_agent_run_delivery(
            &self,
            _delivery_runtime_session_id: &str,
        ) -> Result<AgentRunEffectiveCapabilityView, String> {
            Err("not used".to_string())
        }

        async fn apply_canvas_runtime_surface_update_to_agent_run(
            &self,
            _delivery_runtime_session_id: &str,
            _canvas: &Canvas,
            _current_user: Option<&ProjectAuthorizationContext>,
            _request: RuntimeSurfaceUpdateRequest,
        ) -> Result<RuntimeVfsState, String> {
            Ok(self.state.clone())
        }

        async fn inject_agent_run_notification(
            &self,
            _delivery_runtime_session_id: &str,
            _notification: BackboneEnvelope,
        ) -> Result<(), String> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn canvas_runtime_surface_update_replaces_vfs_with_bridge_policy() {
        let canvas = Canvas::new(
            uuid::Uuid::new_v4(),
            "cvs-dashboard".to_string(),
            "Dashboard".to_string(),
            String::new(),
        );
        let shared_vfs =
            SharedRuntimeVfs::new_with_policy(vfs(vec![mount("main")]), docs_only_policy());
        let mut updated_policy = docs_only_policy();
        updated_policy.rules.push(RuntimeVfsAccessRule {
            mount_id: canvas.mount_id.clone(),
            path_pattern: RuntimeVfsPathPattern::All,
            operations: BTreeSet::from([RuntimeVfsOperation::Read]),
            source: RuntimeVfsAccessSource::SystemRuntimeProjection,
        });
        let updated_state = RuntimeVfsState::new(
            vfs(vec![mount("main"), mount(&canvas.mount_id)]),
            updated_policy,
        );
        let bridge_handle = SharedWorkspaceModuleAgentRunBridgeHandle::default();
        bridge_handle
            .set(Arc::new(ReturningCanvasBridge {
                state: updated_state,
            }))
            .await;

        submit_canvas_runtime_surface_update(
            Some(&shared_vfs),
            &bridge_handle,
            Some("runtime-1"),
            None,
            &canvas,
            RuntimeSurfaceUpdateRequest::CanvasVisibilityRequested {
                canvas_mount_id: canvas.mount_id.clone(),
                reason: CanvasVisibilityReason::Presented,
            },
        )
        .await
        .expect("surface update");

        let state = shared_vfs.snapshot_state().await;
        assert!(
            state
                .vfs
                .mounts
                .iter()
                .any(|mount| mount.id == canvas.mount_id)
        );
        assert!(state.access_policy.admits(
            &canvas.mount_id,
            "index.html",
            RuntimeVfsOperation::Read
        ));
        assert!(
            !state
                .access_policy
                .admits("main", "src/lib.rs", RuntimeVfsOperation::Read),
            "replace must keep bridge policy instead of rebuilding whole-mount access"
        );
    }
}
