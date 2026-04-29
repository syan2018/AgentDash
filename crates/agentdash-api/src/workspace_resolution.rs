use std::sync::Arc;

use agentdash_application::backend_transport::{
    BackendTransport, GitRepoInfo, P4WorkspaceInfo, RelayPromptRequest, RelayPromptTransport,
    RelaySessionEvent, RemoteExecutorInfo, TransportError, WorkspaceProbeInfo,
};
pub use agentdash_application::workspace::ResolvedWorkspaceBinding;
use agentdash_application::workspace::{WorkspaceDetectionError, WorkspaceResolutionError};
use agentdash_domain::workspace::Workspace;
use agentdash_relay::{
    AgentConfigRelay, CommandCancelPayload, CommandPromptPayload, CommandWorkspaceDetectPayload,
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

    async fn detect_workspace(
        &self,
        backend_id: &str,
        root: &str,
    ) -> Result<WorkspaceProbeInfo, TransportError> {
        if !self.is_online(backend_id).await {
            return Err(TransportError::BackendOffline(backend_id.to_string()));
        }
        let cmd = RelayMessage::CommandWorkspaceDetect {
            id: RelayMessage::new_id("workspace-detect"),
            payload: CommandWorkspaceDetectPayload {
                path: root.to_string(),
            },
        };
        let resp = self
            .send_command(backend_id, cmd)
            .await
            .map_err(|e| TransportError::OperationFailed(e.to_string()))?;

        match resp {
            RelayMessage::ResponseWorkspaceDetect {
                payload: Some(payload),
                error: None,
                ..
            } => Ok(WorkspaceProbeInfo {
                git: payload.git.map(|git| GitRepoInfo {
                    is_git_repo: true,
                    repo_root: Some(git.repo_root),
                    source_repo: git.remote_url,
                    default_branch: git.default_branch,
                    branch: git.current_branch,
                    commit_hash: git.commit_hash,
                }),
                p4: payload.p4.map(|p4| P4WorkspaceInfo {
                    is_p4_workspace: true,
                    workspace_root: Some(p4.workspace_root),
                    client_name: p4.client_name,
                    server_address: p4.server_address,
                    user_name: p4.user_name,
                    stream: p4.stream,
                }),
                warnings: payload.warnings,
            }),
            RelayMessage::ResponseWorkspaceDetect {
                error: Some(err), ..
            } => Err(TransportError::OperationFailed(format!(
                "远程 workspace_detect 错误: {}",
                err.message
            ))),
            _ => Err(TransportError::OperationFailed(
                "远程 workspace_detect 返回了意外响应".into(),
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
                mount_root_ref: payload.mount_root_ref,
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

    async fn relay_cancel(&self, backend_id: &str, session_id: &str) -> Result<(), TransportError> {
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
        preferred_backend_id: Option<&str>,
    ) -> Result<String, TransportError> {
        let online = self.list_online().await;

        if let Some(backend_id) = preferred_backend_id
            .map(str::trim)
            .filter(|id| !id.is_empty())
        {
            let backend = online
                .iter()
                .find(|item| item.backend_id == backend_id)
                .ok_or_else(|| {
                    TransportError::BackendOffline(format!(
                        "mount 绑定的 backend `{backend_id}` 当前不在线"
                    ))
                })?;

            let has_executor = backend.capabilities.executors.iter().any(|executor| {
                executor.id.eq_ignore_ascii_case(executor_id) && executor.available
            });
            if has_executor {
                return Ok(backend.backend_id.clone());
            }
            return Err(TransportError::OperationFailed(format!(
                "mount 绑定的 backend `{backend_id}` 未提供可用执行器 `{executor_id}`"
            )));
        }

        // 未提供 backend 绑定提示时，按 executor 在在线后端中做唯一匹配。
        let candidates: Vec<_> = online
            .iter()
            .filter(|b| {
                b.capabilities
                    .executors
                    .iter()
                    .any(|ex| ex.id.eq_ignore_ascii_case(executor_id) && ex.available)
            })
            .collect();

        match candidates.len() {
            0 => Err(TransportError::OperationFailed(format!(
                "没有在线后端提供可用执行器 '{executor_id}'"
            ))),
            1 => Ok(candidates[0].backend_id.clone()),
            _ => Err(TransportError::OperationFailed(format!(
                "执行器 '{executor_id}' 在多个在线后端同时可用，且当前会话缺少明确 mount/backend 绑定信息"
            ))),
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
