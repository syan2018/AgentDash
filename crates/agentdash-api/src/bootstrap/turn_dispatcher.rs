use std::collections::HashMap;
use std::sync::Arc;

use agentdash_application::session::{PromptSessionRequest, SessionHub, UserPromptInput};
use agentdash_application::task::execution::{StartedTurn, TaskExecutionError};
use agentdash_application::task::gateway::{
    PreparedTurnContext, map_connector_error, normalize_backend_id,
};
use agentdash_application::task::service::TurnDispatcher;
use agentdash_domain::common::ThinkingLevel;
use agentdash_relay::{
    AgentConfigRelay, CommandCancelPayload, CommandPromptPayload, RelayMessage,
    ResponsePromptPayload,
};
use async_trait::async_trait;
use tokio::sync::RwLock;
use crate::relay::registry::BackendRegistry;
use crate::runtime_bridge::runtime_mcp_servers_to_acp;
use crate::workspace_resolution::resolve_workspace_binding_core;
use agentdash_application::workspace::{ResolvedWorkspaceBinding, WorkspaceResolutionError};

/// API 层 `TurnDispatcher` 实现 — 封装 relay / 云端原生执行分发逻辑。
///
/// 持有独立的基础设施组件引用，不再依赖完整的 `AppState`，
/// 消除了 AppState ↔ TaskLifecycleService 的循环依赖。
pub struct AppStateTurnDispatcher {
    pub(crate) session_hub: SessionHub,
    pub(crate) backend_registry: Arc<BackendRegistry>,
    pub(crate) remote_sessions: Arc<RwLock<HashMap<String, String>>>,
}

impl AppStateTurnDispatcher {
    pub fn new(
        session_hub: SessionHub,
        backend_registry: Arc<BackendRegistry>,
        remote_sessions: Arc<RwLock<HashMap<String, String>>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            session_hub,
            backend_registry,
            remote_sessions,
        })
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
        bootstrap_action: agentdash_application::session::SessionBootstrapAction::None,
        identity: ctx.identity,
        post_turn_handler: ctx.post_turn_handler,
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

    // 为 relay session 初始化 hook runtime + SessionTurnProcessor
    let executor = ctx
        .resolved_config
        .as_ref()
        .map(|c| c.executor.as_str())
        .unwrap_or("unknown");
    let permission_policy = ctx.resolved_config.as_ref().and_then(|c| c.permission_policy.as_deref());
    let workspace_root = std::path::PathBuf::from(&resolved_binding.root_ref);
    let working_directory = ctx
        .built
        .working_dir
        .as_ref()
        .map(|d| workspace_root.join(d))
        .unwrap_or_else(|| workspace_root.clone());

    let hook_session = dispatcher
        .session_hub
        .load_session_hook_runtime(
            session_id,
            &turn_id,
            executor,
            permission_policy,
            &workspace_root,
            &working_directory,
        )
        .await
        .map_err(|e| TaskExecutionError::Internal(format!("加载 relay hook runtime 失败: {e}")))?;

    // 将 hook_session 写入内存 SessionRuntime
    dispatcher
        .session_hub
        .set_session_hook_runtime(session_id, hook_session.clone())
        .await;

    // 构造 source
    let source = agentdash_acp_meta::AgentDashSourceV1::new(backend_id, "relay_backend");

    let _processor = agentdash_application::session::SessionTurnProcessor::spawn(
        dispatcher.session_hub.clone(),
        agentdash_application::session::SessionTurnProcessorConfig {
            session_id: session_id.to_string(),
            turn_id: turn_id.clone(),
            source,
            hook_session,
            post_turn_handler: ctx.post_turn_handler,
        },
    );

    // 注册 processor_tx 到 SessionRuntime（SessionTurnProcessor::spawn 内部已完成，
    // 但 prompt_pipeline 中是在 spawn 之后手动注册的；这里直接通过 set_processor 注册）
    dispatcher
        .session_hub
        .set_session_processor_tx(session_id, _processor.tx())
        .await;

    // 标记 session 为 running
    dispatcher
        .session_hub
        .mark_session_running(session_id, &turn_id)
        .await;

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

