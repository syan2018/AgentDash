/// RelayAgentConnector — 通过远程后端中继执行 Agent 命令。
///
/// 实现 `AgentConnector` trait，使远程后端上报的执行器与本地执行器在业务层
/// 具有完全相同的路径。内部通过 `RelayPromptTransport` 端口与远程后端交互，
/// 通过 per-session sink channel 将 WebSocket 推送桥接为标准 `ExecutionStream`。
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use futures::stream::BoxStream;
use tokio::sync::mpsc;

use agentdash_spi::AgentConnector;
use agentdash_spi::connector::{
    AgentInfo, ConnectorCapabilities, ConnectorError, ConnectorType, ExecutionContext,
    ExecutionStream, PromptPayload,
};

use crate::backend_transport::{
    RelayExecutorConfig, RelayPromptRequest, RelayPromptTransport, RelaySessionEvent,
    RelayTerminalKind,
};

pub struct RelayAgentConnector {
    transport: Arc<dyn RelayPromptTransport>,
}

impl RelayAgentConnector {
    pub fn new(transport: Arc<dyn RelayPromptTransport>) -> Self {
        Self { transport }
    }
}

#[async_trait]
impl AgentConnector for RelayAgentConnector {
    fn connector_id(&self) -> &'static str {
        "relay"
    }

    fn connector_type(&self) -> ConnectorType {
        ConnectorType::RemoteAcpBackend
    }

    fn capabilities(&self) -> ConnectorCapabilities {
        ConnectorCapabilities {
            supports_cancel: true,
            supports_discovery: false,
            supports_variants: true,
            supports_model_override: true,
            supports_permission_policy: true,
        }
    }

    fn supports_repository_restore(&self, _executor: &str) -> bool {
        false
    }

    fn list_executors(&self) -> Vec<AgentInfo> {
        let transport = self.transport.clone();
        let rt = tokio::runtime::Handle::try_current();
        match rt {
            Ok(handle) => {
                // 在 async context 中用 block_in_place 同步获取
                tokio::task::block_in_place(|| {
                    handle.block_on(async {
                        let remote = transport.list_online_executors().await;
                        dedup_executors(remote)
                    })
                })
            }
            Err(_) => Vec::new(),
        }
    }

    async fn discover_options_stream(
        &self,
        _executor: &str,
        _working_dir: Option<PathBuf>,
    ) -> Result<BoxStream<'static, json_patch::Patch>, ConnectorError> {
        Err(ConnectorError::InvalidConfig(
            "relay connector 不支持 discover_options_stream".to_string(),
        ))
    }

    async fn has_live_session(&self, session_id: &str) -> bool {
        self.transport.has_session_sink(session_id)
    }

    async fn prompt(
        &self,
        session_id: &str,
        _follow_up_session_id: Option<&str>,
        prompt: &PromptPayload,
        context: ExecutionContext,
    ) -> Result<ExecutionStream, ConnectorError> {
        let executor_id = &context.executor_config.executor;
        let default_mount = default_mount_from_context(&context)?;
        let mount_root_ref = default_mount.root_ref.trim();
        if mount_root_ref.is_empty() {
            return Err(ConnectorError::InvalidConfig(
                "vfs.default_mount.root_ref 为空".to_string(),
            ));
        }
        let preferred_backend_id = preferred_backend_id_from_context(&context);
        let backend_id = self
            .transport
            .resolve_backend(executor_id, preferred_backend_id.as_deref())
            .await
            .map_err(|e| ConnectorError::Runtime(format!("无法解析 relay 后端: {e}")))?;

        let prompt_blocks = match prompt {
            PromptPayload::Text(text) => Some(serde_json::json!([{"type": "text", "text": text}])),
            PromptPayload::Blocks(blocks) => serde_json::to_value(blocks).ok(),
        };

        let executor_config = context.executor_config.clone();
        let relay_config = RelayExecutorConfig {
            executor: executor_config.executor.clone(),
            provider_id: executor_config.provider_id.clone(),
            model_id: executor_config.model_id.clone(),
            agent_id: executor_config.agent_id.clone(),
            thinking_level: executor_config.thinking_level.map(|level| {
                match level {
                    agentdash_domain::common::ThinkingLevel::Off => "off",
                    agentdash_domain::common::ThinkingLevel::Minimal => "minimal",
                    agentdash_domain::common::ThinkingLevel::Low => "low",
                    agentdash_domain::common::ThinkingLevel::Medium => "medium",
                    agentdash_domain::common::ThinkingLevel::High => "high",
                    agentdash_domain::common::ThinkingLevel::Xhigh => "xhigh",
                }
                .to_string()
            }),
            permission_policy: executor_config.permission_policy.clone(),
        };

        let payload = RelayPromptRequest {
            session_id: session_id.to_string(),
            follow_up_session_id: _follow_up_session_id.map(ToString::to_string),
            prompt_blocks,
            mount_root_ref: mount_root_ref.to_string(),
            working_dir: relative_working_dir_string(&context.working_directory, mount_root_ref),
            env: context.environment_variables,
            executor_config: Some(relay_config),
            mcp_servers: context
                .mcp_servers
                .iter()
                .filter_map(|s| serde_json::to_value(s).ok())
                .collect(),
        };

        let _turn_id = self
            .transport
            .relay_prompt(&backend_id, payload)
            .await
            .map_err(|e| ConnectorError::Runtime(format!("relay prompt 失败: {e}")))?;

        // 创建 notification channel 并注册到 transport sink map
        let (tx, rx) = mpsc::unbounded_channel::<RelaySessionEvent>();
        self.transport.register_session_sink(session_id, tx);

        let stream: ExecutionStream = Box::pin(futures::stream::unfold(rx, |mut rx| async {
            match rx.recv().await {
                Some(RelaySessionEvent::Notification(n)) => Some((Ok(n), rx)),
                Some(RelaySessionEvent::Terminal {
                    kind: RelayTerminalKind::Failed,
                    message,
                }) => Some((
                    Err(ConnectorError::Runtime(
                        message.unwrap_or_else(|| "远程执行失败".to_string()),
                    )),
                    rx,
                )),
                Some(RelaySessionEvent::Terminal { .. }) | None => None,
            }
        }));

        Ok(stream)
    }

    async fn cancel(&self, session_id: &str) -> Result<(), ConnectorError> {
        // 查找是否有活跃的 sink（证明该 session 由本 connector 管理）
        if !self.transport.has_session_sink(session_id) {
            return Err(ConnectorError::Runtime(format!(
                "session `{session_id}` 不由 relay connector 管理"
            )));
        }

        // 需要 backend_id — 从 sink 关联查找或遍历在线后端。
        // 向所有在线后端广播 cancel（relay cancel 是幂等的）。
        let online_ids = self.transport.list_online_backend_ids().await;
        let mut last_error = None;
        for backend_id in &online_ids {
            match self.transport.relay_cancel(backend_id, session_id).await {
                Ok(()) => {
                    self.transport.unregister_session_sink(session_id);
                    return Ok(());
                }
                Err(e) => last_error = Some(e),
            }
        }
        Err(ConnectorError::Runtime(format!(
            "relay cancel 失败: {}",
            last_error
                .map(|e| e.to_string())
                .unwrap_or_else(|| "无在线后端".to_string())
        )))
    }

    async fn approve_tool_call(
        &self,
        _session_id: &str,
        _tool_call_id: &str,
    ) -> Result<(), ConnectorError> {
        // relay 工具审批通过 WebSocket handler 的独立路径处理，此处不需要
        Err(ConnectorError::Runtime(
            "relay connector 暂不直接处理 approve_tool_call".to_string(),
        ))
    }

    async fn reject_tool_call(
        &self,
        _session_id: &str,
        _tool_call_id: &str,
        _reason: Option<String>,
    ) -> Result<(), ConnectorError> {
        Err(ConnectorError::Runtime(
            "relay connector 暂不直接处理 reject_tool_call".to_string(),
        ))
    }
}

fn relative_working_dir_string(
    working_directory: &std::path::Path,
    mount_root_ref: &str,
) -> Option<String> {
    let root = mount_root_ref
        .trim()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_string();
    if root.is_empty() {
        return None;
    }
    let wd = working_directory
        .to_string_lossy()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_string();

    if wd == root {
        return None;
    }
    let prefix = format!("{root}/");
    wd.strip_prefix(&prefix)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

/// 对远程执行器列表去重（同一 executor_id 可能被多个后端上报）。
fn dedup_executors(executors: Vec<crate::backend_transport::RemoteExecutorInfo>) -> Vec<AgentInfo> {
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for ex in executors {
        if seen.insert(ex.executor_id.clone()) {
            result.push(AgentInfo {
                id: ex.executor_id,
                name: ex.executor_name,
                variants: ex.variants,
                available: ex.available,
            });
        }
    }
    result
}

fn default_mount_from_context(
    context: &ExecutionContext,
) -> Result<&agentdash_spi::Mount, ConnectorError> {
    let vfs = context.vfs.as_ref().ok_or_else(|| {
        ConnectorError::InvalidConfig("ExecutionContext 缺少 vfs".to_string())
    })?;
    vfs.default_mount().ok_or_else(|| {
        ConnectorError::InvalidConfig("vfs 缺少 default_mount".to_string())
    })
}

fn preferred_backend_id_from_context(context: &ExecutionContext) -> Option<String> {
    let vfs = context.vfs.as_ref()?;
    if let Some(default_mount) = vfs.default_mount() {
        let backend_id = default_mount.backend_id.trim();
        if !backend_id.is_empty() {
            return Some(backend_id.to_string());
        }
    }

    let unique_backend_ids = vfs
        .mounts
        .iter()
        .filter_map(|mount| {
            let backend_id = mount.backend_id.trim();
            (!backend_id.is_empty()).then_some(backend_id.to_string())
        })
        .collect::<std::collections::BTreeSet<_>>();

    (unique_backend_ids.len() == 1)
        .then(|| unique_backend_ids.into_iter().next())
        .flatten()
}
