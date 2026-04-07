use std::sync::Arc;

use agentdash_application::backend_transport::{
    BackendTransport, GitRepoInfo, RelayPromptRequest, RelayPromptTransport, RelaySessionEvent,
    RemoteExecutorInfo, TransportError,
};
pub use agentdash_application::workspace::ResolvedWorkspaceBinding;
use agentdash_application::workspace::{WorkspaceDetectionError, WorkspaceResolutionError};
use agentdash_domain::workspace::Workspace;
use agentdash_relay::{
    AgentConfigRelay, CommandCancelPayload, CommandPromptPayload, CommandWorkspaceDetectGitPayload,
    RelayMessage, ResponsePromptPayload,
};
use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::app_state::AppState;
use crate::relay::registry::BackendRegistry;
use crate::rpc::ApiError;

pub use agentdash_application::workspace::WorkspaceDetectionResult;
pub use agentdash_application::workspace::resolve_workspace_binding as resolve_workspace_binding_core;

/// BackendRegistry 适配 BackendTransport trait —— API adapter 层
#[async_trait]
impl BackendTransport for BackendRegistry {
    async fn is_online(&self, backend_id: &str) -> bool {
        self.is_online(backend_id).await
    }

    async fn list_online_backend_ids(&self) -> Vec<String> {
        self.list_online_ids().await
    }

    async fn detect_git_repo(
        &self,
        backend_id: &str,
        root: &str,
    ) -> Result<GitRepoInfo, TransportError> {
        if !self.is_online(backend_id).await {
            return Err(TransportError::BackendOffline(backend_id.to_string()));
        }
        let cmd = RelayMessage::CommandWorkspaceDetectGit {
            id: RelayMessage::new_id("workspace-detect-git"),
            payload: CommandWorkspaceDetectGitPayload {
                path: root.to_string(),
            },
        };
        let resp = self
            .send_command(backend_id, cmd)
            .await
            .map_err(|e| TransportError::OperationFailed(e.to_string()))?;

        match resp {
            RelayMessage::ResponseWorkspaceDetectGit {
                payload: Some(payload),
                error: None,
                ..
            } => Ok(GitRepoInfo {
                is_git_repo: payload.is_git,
                source_repo: payload
                    .remote_url
                    .clone()
                    .or_else(|| payload.is_git.then(|| root.to_string())),
                branch: payload.current_branch.or(payload.default_branch),
                commit_hash: None,
            }),
            RelayMessage::ResponseWorkspaceDetectGit {
                error: Some(err), ..
            } => Err(TransportError::OperationFailed(format!(
                "远程 workspace_detect_git 错误: {}",
                err.message
            ))),
            _ => Err(TransportError::OperationFailed(
                "远程 workspace_detect_git 返回了意外响应".into(),
            )),
        }
    }
}

/// BackendRegistry 适配 RelayPromptTransport trait —— relay connector 所需的完整传输能力
#[async_trait]
impl RelayPromptTransport for BackendRegistry {
    async fn relay_prompt(
        &self,
        backend_id: &str,
        payload: RelayPromptRequest,
    ) -> Result<String, TransportError> {
        let relay_config = payload.executor_config.map(|c| AgentConfigRelay {
            executor: c.executor,
            provider_id: c.provider_id,
            model_id: c.model_id,
            agent_id: c.agent_id,
            thinking_level: c.thinking_level,
            permission_policy: c.permission_policy,
        });

        let cmd = RelayMessage::CommandPrompt {
            id: RelayMessage::new_id("prompt"),
            payload: Box::new(CommandPromptPayload {
                session_id: payload.session_id,
                follow_up_session_id: payload.follow_up_session_id,
                prompt_blocks: payload.prompt_blocks,
                workspace_root: payload.workspace_root,
                working_dir: payload.working_dir,
                env: payload.env,
                executor_config: relay_config,
                mcp_servers: payload.mcp_servers,
            }),
        };

        let resp = self
            .send_command(backend_id, cmd)
            .await
            .map_err(|e| TransportError::OperationFailed(format!("relay prompt 失败: {e}")))?;

        match resp {
            RelayMessage::ResponsePrompt {
                payload: Some(ResponsePromptPayload { turn_id, .. }),
                error: None,
                ..
            } => Ok(turn_id),
            RelayMessage::ResponsePrompt {
                error: Some(err), ..
            } => Err(TransportError::OperationFailed(format!(
                "远程后端执行失败: {}",
                err.message
            ))),
            other => Err(TransportError::OperationFailed(format!(
                "远程后端返回意外响应: {}",
                other.id()
            ))),
        }
    }

    async fn relay_cancel(
        &self,
        backend_id: &str,
        session_id: &str,
    ) -> Result<(), TransportError> {
        let cmd = RelayMessage::CommandCancel {
            id: RelayMessage::new_id("cancel"),
            payload: CommandCancelPayload {
                session_id: session_id.to_string(),
            },
        };
        let resp = self
            .send_command(backend_id, cmd)
            .await
            .map_err(|e| TransportError::OperationFailed(format!("relay cancel 失败: {e}")))?;

        match resp {
            RelayMessage::ResponseCancel { error: None, .. } => Ok(()),
            RelayMessage::ResponseCancel {
                error: Some(err), ..
            } => Err(TransportError::OperationFailed(format!(
                "远程取消失败: {}",
                err.message
            ))),
            _ => Ok(()),
        }
    }

    async fn list_online_executors(&self) -> Vec<RemoteExecutorInfo> {
        let mut result = Vec::new();
        for backend in self.list_online().await {
            for ex in &backend.capabilities.executors {
                result.push(RemoteExecutorInfo {
                    backend_id: backend.backend_id.clone(),
                    executor_id: ex.id.clone(),
                    executor_name: ex.name.clone(),
                    variants: ex.variants.clone(),
                    available: ex.available,
                });
            }
        }
        result
    }

    fn register_session_sink(
        &self,
        session_id: &str,
        tx: mpsc::UnboundedSender<RelaySessionEvent>,
    ) {
        BackendRegistry::register_session_sink(self, session_id, tx);
    }

    fn unregister_session_sink(&self, session_id: &str) {
        BackendRegistry::unregister_session_sink(self, session_id);
    }

    fn has_session_sink(&self, session_id: &str) -> bool {
        BackendRegistry::has_session_sink(self, session_id)
    }

    async fn resolve_backend(
        &self,
        executor_id: &str,
        workspace_root: &str,
    ) -> Result<String, TransportError> {
        let online = self.list_online().await;
        // 寻找同时满足：(1) 提供该 executor (2) 能访问该 workspace root 的后端
        let candidates: Vec<_> = online
            .iter()
            .filter(|b| {
                let has_executor = b
                    .capabilities
                    .executors
                    .iter()
                    .any(|ex| ex.id.eq_ignore_ascii_case(executor_id) && ex.available);
                let can_access = b.accessible_roots.is_empty()
                    || b.accessible_roots
                        .iter()
                        .any(|root| workspace_root.starts_with(root));
                has_executor && can_access
            })
            .collect();

        match candidates.len() {
            0 => Err(TransportError::OperationFailed(format!(
                "没有在线后端同时提供执行器 '{executor_id}' 且能访问 '{workspace_root}'"
            ))),
            _ => Ok(candidates[0].backend_id.clone()),
        }
    }
}

/// 薄 API adapter：解析 workspace binding（错误映射到 ApiError）
pub async fn resolve_workspace_binding(
    state: &Arc<AppState>,
    workspace: &Workspace,
) -> Result<ResolvedWorkspaceBinding, ApiError> {
    resolve_workspace_binding_core(state.services.backend_registry.as_ref(), workspace)
        .await
        .map_err(|err| match err {
            WorkspaceResolutionError::NoBindings(msg)
            | WorkspaceResolutionError::NoAvailable(msg) => ApiError::Conflict(msg),
        })
}

/// 薄 API adapter：探测远程 workspace（错误映射到 ApiError）
pub async fn detect_workspace_from_backend(
    state: &Arc<AppState>,
    backend_id: &str,
    root_ref: &str,
) -> Result<WorkspaceDetectionResult, ApiError> {
    agentdash_application::workspace::detect_workspace_from_backend(
        state.services.backend_registry.as_ref(),
        backend_id,
        root_ref,
    )
    .await
    .map_err(|err| match err {
        WorkspaceDetectionError::BadRequest(msg) => ApiError::BadRequest(msg),
        WorkspaceDetectionError::BackendOffline(msg) => ApiError::Conflict(msg),
        WorkspaceDetectionError::TransportFailed(msg) => ApiError::Internal(msg),
    })
}
