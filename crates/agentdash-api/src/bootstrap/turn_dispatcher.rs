use std::collections::HashMap;
use std::sync::Arc;

use agentdash_application::repository_set::RepositorySet;
use agentdash_application::session::{PromptSessionRequest, SessionHub, UserPromptInput};
use agentdash_application::task::execution::{
    ContinueTaskCommand, StartedTurn, TaskExecutionError,
};
use agentdash_application::task::gateway::{
    PreparedTurnContext, TurnOutcome, map_connector_error, normalize_backend_id, run_turn_monitor,
    update_task_status,
};
use agentdash_application::task::service::{TaskLifecycleService, TurnDispatcher};
use agentdash_domain::common::ThinkingLevel;
use agentdash_relay::{
    AgentConfigRelay, CommandCancelPayload, CommandPromptPayload, RelayMessage,
    ResponsePromptPayload,
};
use async_trait::async_trait;
use serde_json::json;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::relay::registry::BackendRegistry;
use crate::runtime_bridge::runtime_mcp_servers_to_acp;
use crate::workspace_resolution::resolve_workspace_binding_core;
use agentdash_application::task::restart_tracker::RestartTracker;
use agentdash_application::workspace::{ResolvedWorkspaceBinding, WorkspaceResolutionError};

/// API 层 `TurnDispatcher` 实现 — 封装 relay / 云端原生执行分发逻辑。
///
/// 持有独立的基础设施组件引用，不再依赖完整的 `AppState`，
/// 消除了 AppState ↔ TaskLifecycleService 的循环依赖。
pub struct AppStateTurnDispatcher {
    pub(crate) session_hub: SessionHub,
    pub(crate) backend_registry: Arc<BackendRegistry>,
    pub(crate) repos: RepositorySet,
    pub(crate) restart_tracker: Arc<RestartTracker>,
    pub(crate) remote_sessions: Arc<RwLock<HashMap<String, String>>>,
    /// 仅在 auto-retry 路径使用。构建后通过 `set_retry_service()` 注入，
    /// 打断 dispatcher ↔ TaskLifecycleService 的循环引用。
    retry_service: tokio::sync::OnceCell<Arc<TaskLifecycleService>>,
}

impl AppStateTurnDispatcher {
    pub fn new(
        session_hub: SessionHub,
        backend_registry: Arc<BackendRegistry>,
        repos: RepositorySet,
        restart_tracker: Arc<RestartTracker>,
        remote_sessions: Arc<RwLock<HashMap<String, String>>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            session_hub,
            backend_registry,
            repos,
            restart_tracker,
            remote_sessions,
            retry_service: tokio::sync::OnceCell::new(),
        })
    }

    pub fn set_retry_service(&self, service: Arc<TaskLifecycleService>) {
        let _ = self.retry_service.set(service);
    }
}

#[async_trait]
impl TurnDispatcher for AppStateTurnDispatcher {
    async fn dispatch_turn(
        &self,
        session_id: &str,
        ctx: PreparedTurnContext,
    ) -> Result<StartedTurn, TaskExecutionError> {
        if ctx.use_cloud_native_agent {
            dispatch_cloud_native(self, session_id, ctx).await
        } else {
            dispatch_relay(self, session_id, ctx).await
        }
    }

    async fn cancel_session(&self, session_id: &str) -> Result<(), TaskExecutionError> {
        let remote_backend = self.remote_sessions.read().await.get(session_id).cloned();
        if let Some(backend_id) = remote_backend {
            relay_cancel(&self.backend_registry, &backend_id, session_id).await
        } else {
            self.session_hub
                .cancel(session_id)
                .await
                .map_err(map_connector_error)
        }
    }

    fn spawn_turn_monitor(
        &self,
        task_id: Uuid,
        session_id: String,
        turn_id: String,
        backend_id: String,
    ) {
        let repos = self.repos.clone();
        let hub = self.session_hub.clone();
        let tracker = self.restart_tracker.clone();
        let retry_service = self.retry_service.get().cloned();
        let retry_repos = self.repos.clone();
        tokio::spawn(async move {
            let outcome = run_turn_monitor(
                &repos,
                &hub,
                &tracker,
                task_id,
                &session_id,
                &turn_id,
                &backend_id,
            )
            .await;
            if let TurnOutcome::NeedsRetry { delay, attempt } = outcome {
                schedule_auto_retry(
                    retry_service,
                    &retry_repos,
                    task_id,
                    session_id,
                    backend_id,
                    delay,
                    attempt,
                )
                .await;
            }
        });
    }
}

async fn dispatch_cloud_native(
    dispatcher: &AppStateTurnDispatcher,
    session_id: &str,
    ctx: PreparedTurnContext,
) -> Result<StartedTurn, TaskExecutionError> {
    let resolved_binding = if let Some(ws) = ctx.workspace.as_ref() {
        Some(
            resolve_workspace_binding_core(dispatcher.backend_registry.as_ref(), ws)
                .await
                .map_err(|e| match e {
                    WorkspaceResolutionError::NoBindings(msg)
                    | WorkspaceResolutionError::NoAvailable(msg) => {
                        TaskExecutionError::Internal(msg)
                    }
                })?,
        )
    } else {
        None
    };
    let workspace_root = resolved_binding
        .as_ref()
        .map(|item| std::path::PathBuf::from(item.root_ref.clone()));

    let prompt_req = PromptSessionRequest {
        user_input: UserPromptInput {
            prompt_blocks: Some(ctx.built.prompt_blocks),
            working_dir: ctx.built.working_dir,
            env: Default::default(),
            executor_config: ctx.resolved_config.clone(),
        },
        mcp_servers: runtime_mcp_servers_to_acp(&ctx.built.mcp_servers),
        workspace_root,
        address_space: ctx.address_space.clone(),
        flow_capabilities: Some(agentdash_spi::FlowCapabilities::from_clusters([
            agentdash_spi::ToolCluster::Read,
            agentdash_spi::ToolCluster::Write,
            agentdash_spi::ToolCluster::Execute,
            agentdash_spi::ToolCluster::Workflow,
            agentdash_spi::ToolCluster::Collaboration,
            agentdash_spi::ToolCluster::Canvas,
        ])),
        system_context: ctx.built.system_context.clone(),
        identity: ctx.identity,
    };

    let turn_id = dispatcher
        .session_hub
        .start_prompt(session_id, prompt_req)
        .await
        .map_err(map_connector_error)?;

    Ok(StartedTurn {
        turn_id,
        context_sources: ctx.built.source_summary,
    })
}

async fn dispatch_relay(
    dispatcher: &AppStateTurnDispatcher,
    session_id: &str,
    ctx: PreparedTurnContext,
) -> Result<StartedTurn, TaskExecutionError> {
    let ws = ctx.workspace.as_ref().ok_or_else(|| {
        TaskExecutionError::BadRequest(
            "第三方 Agent 任务必须绑定 Workspace，且运行位置由 Workspace.backend_id 决定".into(),
        )
    })?;
    let resolved_binding = resolve_workspace_binding_core(dispatcher.backend_registry.as_ref(), ws)
        .await
        .map_err(|e| match e {
            WorkspaceResolutionError::NoBindings(msg)
            | WorkspaceResolutionError::NoAvailable(msg) => TaskExecutionError::Internal(msg),
        })?;
    let backend_id = normalize_backend_id(&resolved_binding.backend_id)?;

    if !dispatcher.backend_registry.is_online(backend_id).await {
        return Err(TaskExecutionError::Conflict(format!(
            "目标 Workspace 所属 Backend 当前不在线: {backend_id}"
        )));
    }

    let turn_id = relay_start_prompt(
        &dispatcher.backend_registry,
        backend_id,
        session_id,
        &ctx,
        &resolved_binding,
    )
    .await?;

    dispatcher
        .remote_sessions
        .write()
        .await
        .insert(session_id.to_string(), backend_id.to_string());

    Ok(StartedTurn {
        turn_id,
        context_sources: ctx.built.source_summary.clone(),
    })
}

async fn relay_start_prompt(
    registry: &BackendRegistry,
    backend_id: &str,
    session_id: &str,
    ctx: &PreparedTurnContext,
    binding: &ResolvedWorkspaceBinding,
) -> Result<String, TaskExecutionError> {
    let relay_config = ctx.resolved_config.as_ref().map(|c| AgentConfigRelay {
        executor: c.executor.clone(),
        variant: c.variant.clone(),
        provider_id: c.provider_id.clone(),
        model_id: c.model_id.clone(),
        agent_id: c.agent_id.clone(),
        thinking_level: c.thinking_level.map(|level| {
            match level {
                ThinkingLevel::Off => "off",
                ThinkingLevel::Minimal => "minimal",
                ThinkingLevel::Low => "low",
                ThinkingLevel::Medium => "medium",
                ThinkingLevel::High => "high",
                ThinkingLevel::Xhigh => "xhigh",
            }
            .to_string()
        }),
        permission_policy: c.permission_policy.clone(),
    });
    let mcp_servers = runtime_mcp_servers_to_acp(&ctx.built.mcp_servers)
        .into_iter()
        .enumerate()
        .map(|(index, server)| {
            serde_json::to_value(server).map_err(|error| {
                TaskExecutionError::Internal(format!(
                    "序列化第 {index} 个 runtime MCP server 失败: {error}"
                ))
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    let cmd = RelayMessage::CommandPrompt {
        id: RelayMessage::new_id("prompt"),
        payload: Box::new(CommandPromptPayload {
            session_id: session_id.to_string(),
            follow_up_session_id: None,
            prompt_blocks: Some(serde_json::Value::Array(ctx.built.prompt_blocks.clone())),
            workspace_root: binding.root_ref.clone(),
            working_dir: ctx.built.working_dir.clone(),
            env: Default::default(),
            executor_config: relay_config,
            mcp_servers,
        }),
    };

    tracing::info!(backend_id, session_id, "中继 command.prompt → 远程后端");
    let resp = registry
        .send_command(backend_id, cmd)
        .await
        .map_err(|e| TaskExecutionError::Internal(format!("中继 prompt 失败: {e}")))?;

    match resp {
        RelayMessage::ResponsePrompt {
            payload: Some(ResponsePromptPayload { turn_id, .. }),
            error: None,
            ..
        } => Ok(turn_id),
        RelayMessage::ResponsePrompt {
            error: Some(err), ..
        } => Err(TaskExecutionError::Internal(format!(
            "远程后端执行失败: {}",
            err.message
        ))),
        other => Err(TaskExecutionError::Internal(format!(
            "远程后端返回意外响应: {}",
            other.id()
        ))),
    }
}

async fn relay_cancel(
    registry: &BackendRegistry,
    backend_id: &str,
    session_id: &str,
) -> Result<(), TaskExecutionError> {
    tracing::info!(backend_id, session_id, "中继 command.cancel → 远程后端");
    let cmd = RelayMessage::CommandCancel {
        id: RelayMessage::new_id("cancel"),
        payload: CommandCancelPayload {
            session_id: session_id.to_string(),
        },
    };
    let resp = registry
        .send_command(backend_id, cmd)
        .await
        .map_err(|e| TaskExecutionError::Internal(format!("中继 cancel 失败: {e}")))?;

    match resp {
        RelayMessage::ResponseCancel { error: None, .. } => Ok(()),
        RelayMessage::ResponseCancel {
            error: Some(err), ..
        } => Err(TaskExecutionError::Internal(format!(
            "远程取消失败: {}",
            err.message
        ))),
        _ => Ok(()),
    }
}

async fn schedule_auto_retry(
    retry_service: Option<Arc<TaskLifecycleService>>,
    repos: &RepositorySet,
    task_id: Uuid,
    session_id: String,
    backend_id: String,
    delay: std::time::Duration,
    attempt: u32,
) {
    let Some(service) = retry_service else {
        tracing::error!(task_id = %task_id, "auto-retry 服务未就绪，跳过重试");
        return;
    };

    tracing::info!(
        task_id = %task_id, session_id = %session_id,
        attempt, delay_ms = delay.as_millis() as u64,
        "等待退避延迟后自动重试"
    );
    tokio::time::sleep(delay).await;

    let retry_prompt = format!(
        "上一次执行失败，这是第 {} 次自动重试。请继续完成任务。",
        attempt
    );
    match service
        .continue_task(ContinueTaskCommand {
            task_id,
            additional_prompt: Some(retry_prompt),
            executor_config: None,
            identity: None,
        })
        .await
    {
        Ok(result) => {
            tracing::info!(
                task_id = %task_id, session_id = %session_id,
                new_turn_id = %result.turn_id, attempt,
                "自动重试已成功发起新 turn"
            );
        }
        Err(err) => {
            tracing::error!(
                task_id = %task_id, session_id = %session_id, attempt,
                "自动重试发起失败，标记 Task 为 Failed: {}", err
            );
            let _ = update_task_status(
                repos,
                task_id,
                &backend_id,
                agentdash_domain::task::TaskStatus::Failed,
                "auto_retry_failed",
                json!({
                    "session_id": session_id,
                    "attempt": attempt,
                    "error": err.to_string(),
                }),
            )
            .await;
        }
    }
}
