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

use agentdash_agent_protocol::codex_app_server_protocol as codex;
use agentdash_domain::backend::{BackendExecutionLeaseRepository, BackendExecutionTerminalKind};
use agentdash_spi::AgentConnector;
use agentdash_spi::connector::{
    AgentInfo, ConnectorCapabilities, ConnectorError, ConnectorType, ExecutionContext,
    ExecutionStream, PromptPayload,
};

use agentdash_application_ports::backend_transport::{
    RelayExecutorConfig, RelayPromptRequest, RelayPromptTransport, RelaySessionEvent,
    RelaySessionRoute, RelaySteerRequest, RelayTerminalKind,
};
use agentdash_domain::workspace::WorkspaceIdentityKind;

pub struct RelayAgentConnector {
    transport: Arc<dyn RelayPromptTransport>,
    lease_repo: Arc<dyn BackendExecutionLeaseRepository>,
}

impl RelayAgentConnector {
    pub fn new(
        transport: Arc<dyn RelayPromptTransport>,
        lease_repo: Arc<dyn BackendExecutionLeaseRepository>,
    ) -> Self {
        Self {
            transport,
            lease_repo,
        }
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
            supports_steering: true,
            supports_discovery: false,
            supports_variants: true,
            supports_model_override: true,
            supports_permission_policy: true,
            supports_source_session_title: false,
        }
    }

    fn supports_repository_restore(&self, _executor: &str) -> bool {
        false
    }

    fn list_executors(&self) -> Vec<AgentInfo> {
        dedup_executors(self.transport.list_online_executors())
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
        let default_mount = default_mount_from_context(&context)?;
        let mount_root_ref = default_mount.root_ref.trim();
        if mount_root_ref.is_empty() {
            return Err(ConnectorError::InvalidConfig(
                "vfs.default_mount.root_ref 为空".to_string(),
            ));
        }
        let backend_execution = context.session.backend_execution.as_ref().ok_or_else(|| {
            ConnectorError::InvalidConfig(
                "relay connector 缺少已解析的 backend execution placement".to_string(),
            )
        })?;
        let backend_id = backend_execution.backend_id.clone();
        let lease_id = backend_execution.lease_id;

        // relay 边界仍按 ACP ContentBlock JSON 与远程后端互通（远程 resolve_prompt_payload
        // 仍按 ContentBlock 反序列化）。canonical 输入在此投影为 ContentBlock 形态。
        // 注意：结构化图片透传到 relay（ContentBlock Image{mimeType,data}）属于 S6 relay 收敛范围；
        // 本批先保证文本链路不回归，非文本输入降级为文本占位 ContentBlock。
        let prompt_blocks = match prompt {
            PromptPayload::Text(text) => Some(serde_json::json!([{"type": "text", "text": text}])),
            PromptPayload::Input(input) => {
                Some(user_input_blocks_to_relay_content_blocks(input))
            }
        };

        let executor_config = context.session.executor_config.clone();
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
            workspace_identity_kind: workspace_identity_kind_from_mount(default_mount),
            workspace_identity_payload: workspace_identity_payload_from_mount(default_mount),
            working_dir: crate::session::path_policy::to_relative_working_dir(
                &context.session.working_directory,
                mount_root_ref,
            ),
            env: context.session.environment_variables,
            executor_config: Some(relay_config),
            mcp_servers: context
                .session
                .mcp_servers
                .iter()
                .filter_map(|server| serde_json::to_value(server).ok())
                .collect(),
        };

        // 创建 notification channel 并注册到 transport sink map
        let (tx, rx) = mpsc::unbounded_channel::<RelaySessionEvent>();
        self.transport.register_session_sink(RelaySessionRoute {
            session_id: session_id.to_string(),
            backend_id: backend_id.clone(),
            lease_id,
            tx,
        });
        let sink_guard = RelaySessionSinkGuard {
            transport: self.transport.clone(),
            session_id: session_id.to_string(),
        };

        let _turn_id = match self.transport.relay_prompt(&backend_id, payload).await {
            Ok(turn_id) => {
                if let Err(error) = self.lease_repo.activate(lease_id, chrono::Utc::now()).await {
                    drop(sink_guard);
                    return Err(ConnectorError::Runtime(format!(
                        "激活 backend execution lease 失败: {error}"
                    )));
                }
                turn_id
            }
            Err(e) => {
                let _ = self
                    .lease_repo
                    .fail(lease_id, Some(e.to_string()), chrono::Utc::now())
                    .await;
                drop(sink_guard);
                return Err(ConnectorError::Runtime(format!("relay prompt 失败: {e}")));
            }
        };

        let lease_repo = self.lease_repo.clone();
        let stream: ExecutionStream = Box::pin(futures::stream::unfold(
            (rx, Some(sink_guard), false, lease_repo, lease_id),
            |(mut rx, mut sink_guard, done, lease_repo, lease_id)| async move {
                if done {
                    return None;
                }
                match rx.recv().await {
                    Some(RelaySessionEvent::Notification(n)) => {
                        Some((Ok(*n), (rx, sink_guard, false, lease_repo, lease_id)))
                    }
                    Some(RelaySessionEvent::Terminal {
                        kind: RelayTerminalKind::Failed,
                        message,
                    }) => {
                        let _ = lease_repo
                            .release(
                                lease_id,
                                Some(BackendExecutionTerminalKind::Failed),
                                message.clone(),
                                chrono::Utc::now(),
                            )
                            .await;
                        sink_guard.take();
                        Some((
                            Err(ConnectorError::Runtime(
                                message.unwrap_or_else(|| "远程执行失败".to_string()),
                            )),
                            (rx, None, true, lease_repo, lease_id),
                        ))
                    }
                    Some(RelaySessionEvent::Terminal { kind, message }) => {
                        let terminal_kind = match kind {
                            RelayTerminalKind::Completed => BackendExecutionTerminalKind::Completed,
                            RelayTerminalKind::Interrupted => {
                                BackendExecutionTerminalKind::Interrupted
                            }
                            RelayTerminalKind::Failed => unreachable!(),
                        };
                        let _ = lease_repo
                            .release(lease_id, Some(terminal_kind), message, chrono::Utc::now())
                            .await;
                        sink_guard.take();
                        None
                    }
                    None => {
                        sink_guard.take();
                        None
                    }
                }
            },
        ));

        Ok(stream)
    }

    async fn cancel(&self, session_id: &str) -> Result<(), ConnectorError> {
        // 查找是否有活跃的 sink（证明该 session 由本 connector 管理）
        if !self.transport.has_session_sink(session_id) {
            return Err(ConnectorError::Runtime(format!(
                "session `{session_id}` 不由 relay connector 管理"
            )));
        }

        let route = self.transport.session_route(session_id).ok_or_else(|| {
            ConnectorError::Runtime(format!("session `{session_id}` 缺少 relay backend route"))
        })?;
        self.transport
            .relay_cancel(&route.backend_id, session_id)
            .await
            .map_err(|error| ConnectorError::Runtime(format!("relay cancel 失败: {error}")))?;
        self.transport.unregister_session_sink(session_id);
        let _ = self
            .lease_repo
            .release(
                route.lease_id,
                Some(BackendExecutionTerminalKind::Interrupted),
                Some("cancelled".to_string()),
                chrono::Utc::now(),
            )
            .await;
        Ok(())
    }

    async fn steer_session(
        &self,
        session_id: &str,
        expected_turn_id: &str,
        input: Vec<codex::UserInput>,
    ) -> Result<(), ConnectorError> {
        if !self.transport.has_session_sink(session_id) {
            return Err(ConnectorError::Runtime(format!(
                "session `{session_id}` 不由 relay connector 管理"
            )));
        }

        let route = self.transport.session_route(session_id).ok_or_else(|| {
            ConnectorError::Runtime(format!("session `{session_id}` 缺少 relay backend route"))
        })?;
        self.transport
            .relay_steer(
                &route.backend_id,
                RelaySteerRequest {
                    session_id: session_id.to_string(),
                    input,
                    expected_turn_id: expected_turn_id.to_string(),
                },
            )
            .await
            .map_err(|error| ConnectorError::Runtime(format!("relay steer 失败: {error}")))?;
        Ok(())
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

struct RelaySessionSinkGuard {
    transport: Arc<dyn RelayPromptTransport>,
    session_id: String,
}

impl Drop for RelaySessionSinkGuard {
    fn drop(&mut self) {
        self.transport.unregister_session_sink(&self.session_id);
    }
}

/// 对远程执行器列表去重（同一 executor_id 可能被多个后端上报）。
fn dedup_executors(
    executors: Vec<agentdash_application_ports::backend_transport::RemoteExecutorInfo>,
) -> Vec<AgentInfo> {
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
    let vfs =
        context.session.vfs.as_ref().ok_or_else(|| {
            ConnectorError::InvalidConfig("ExecutionContext 缺少 vfs".to_string())
        })?;
    vfs.default_mount()
        .ok_or_else(|| ConnectorError::InvalidConfig("vfs 缺少 default_mount".to_string()))
}

fn workspace_identity_kind_from_mount(
    mount: &agentdash_domain::common::Mount,
) -> Option<WorkspaceIdentityKind> {
    serde_json::from_value(
        mount
            .metadata
            .get("workspace_identity_kind")
            .cloned()
            .unwrap_or(serde_json::Value::Null),
    )
    .ok()
}

fn workspace_identity_payload_from_mount(
    mount: &agentdash_domain::common::Mount,
) -> Option<serde_json::Value> {
    mount.metadata.get("workspace_identity_payload").cloned()
}

/// 把 canonical 用户输入投影为 relay 远程后端可识别的 ACP ContentBlock JSON。
///
/// 远程后端的 `resolve_prompt_payload` 仍按 ACP ContentBlock 反序列化（前端/relay 入参形态
/// 由 S5/S6 统一），因此 relay 边界把 `UserInputBlock` 映射回 ContentBlock JSON：
/// - `Text` -> ACP text block（保留文本链路不回归）；
/// - `Image{url=data URL}` -> ACP image block（结构化透传，mimeType+data）；
/// - 其余（远程 url / LocalImage / Skill / Mention）-> 文本占位 block（与本批前行为一致，
///   结构化透传留给 S6 relay 收敛）。
fn user_input_blocks_to_relay_content_blocks(
    input: &[agentdash_agent_protocol::UserInputBlock],
) -> serde_json::Value {
    let blocks: Vec<serde_json::Value> = input
        .iter()
        .map(|item| match item {
            codex::UserInput::Text { text, .. } => {
                serde_json::json!({ "type": "text", "text": text })
            }
            codex::UserInput::Image { url, .. } => {
                match parse_relay_data_url(url) {
                    Some((mime_type, data)) => serde_json::json!({
                        "type": "image",
                        "mimeType": mime_type,
                        "data": data,
                    }),
                    None => serde_json::json!({ "type": "text", "text": format!("[引用图片: {url}]") }),
                }
            }
            codex::UserInput::LocalImage { path, .. } => {
                serde_json::json!({ "type": "text", "text": format!("[引用本地图片: {}]", path.display()) })
            }
            codex::UserInput::Skill { name, path } => {
                serde_json::json!({ "type": "text", "text": format!("[引用 Skill: {name} ({})]", path.display()) })
            }
            codex::UserInput::Mention { name, path } => {
                serde_json::json!({ "type": "text", "text": format!("[引用: {name} ({path})]") })
            }
        })
        .collect();
    serde_json::Value::Array(blocks)
}

/// 解析 `data:<mime>;base64,<data>`，返回 `(mime_type, base64_data)`；非 base64 data URL 返回 `None`。
fn parse_relay_data_url(url: &str) -> Option<(String, String)> {
    let rest = url.strip_prefix("data:")?;
    let (meta, data) = rest.split_once(',')?;
    let mut meta_parts = meta.split(';');
    let mime_type = meta_parts.next().unwrap_or("").trim();
    let is_base64 = meta_parts.any(|part| part.trim().eq_ignore_ascii_case("base64"));
    if mime_type.is_empty() || !is_base64 {
        return None;
    }
    Some((mime_type.to_string(), data.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::Path;
    use std::sync::Mutex as StdMutex;

    use crate::backend_execution_placement::{
        BackendSelectionRequest, resolve_backend_execution_placement,
    };
    use agentdash_agent_protocol::{BackboneEnvelope, BackboneEvent, PlatformEvent, SourceInfo};
    use agentdash_domain::DomainError;
    use agentdash_domain::backend::{BackendExecutionLease, BackendExecutionSelectionMode};
    use agentdash_spi::{AgentConfig, CapabilityState, ExecutionBackendPlacement, PromptPayload};
    use futures::StreamExt;
    use tokio::sync::Mutex;
    use uuid::Uuid;

    #[derive(Default)]
    struct CaptureTransport {
        payload: Mutex<Option<RelayPromptRequest>>,
        steers: StdMutex<Vec<(String, RelaySteerRequest)>>,
        sinks: StdMutex<HashMap<String, RelaySessionRoute>>,
        executors:
            StdMutex<Vec<agentdash_application_ports::backend_transport::RemoteExecutorInfo>>,
        prompt_error:
            StdMutex<Option<agentdash_application_ports::backend_transport::TransportError>>,
        cancelled: StdMutex<Vec<(String, String)>>,
    }

    #[derive(Default)]
    struct MemoryLeaseRepository {
        active_counts: StdMutex<HashMap<String, i64>>,
        claims: StdMutex<Vec<BackendExecutionLease>>,
        activations: StdMutex<Vec<Uuid>>,
        releases: StdMutex<Vec<RecordedRelease>>,
        failures: StdMutex<Vec<RecordedFailure>>,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct RecordedRelease {
        lease_id: Uuid,
        terminal_kind: Option<BackendExecutionTerminalKind>,
        reason: Option<String>,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct RecordedFailure {
        lease_id: Uuid,
        reason: Option<String>,
    }

    #[async_trait]
    impl agentdash_application_ports::backend_transport::BackendTransport for CaptureTransport {
        async fn is_online(&self, _backend_id: &str) -> bool {
            true
        }

        async fn list_online_backend_ids(&self) -> Vec<String> {
            vec!["backend-1".to_string()]
        }

        async fn detect_workspace(
            &self,
            _backend_id: &str,
            _root: &str,
        ) -> Result<
            agentdash_application_ports::backend_transport::WorkspaceProbeInfo,
            agentdash_application_ports::backend_transport::TransportError,
        > {
            Ok(Default::default())
        }

        async fn detect_git_repo(
            &self,
            _backend_id: &str,
            _root: &str,
        ) -> Result<
            agentdash_application_ports::backend_transport::GitRepoInfo,
            agentdash_application_ports::backend_transport::TransportError,
        > {
            Ok(Default::default())
        }
    }

    #[async_trait]
    impl RelayPromptTransport for CaptureTransport {
        async fn relay_prompt(
            &self,
            _backend_id: &str,
            payload: RelayPromptRequest,
        ) -> Result<String, agentdash_application_ports::backend_transport::TransportError>
        {
            let session_id = payload.session_id.clone();
            *self.payload.lock().await = Some(payload);
            if let Some(error) = self.prompt_error.lock().unwrap().take() {
                return Err(error);
            }
            if let Some(route) = self.sinks.lock().unwrap().get(&session_id) {
                let envelope = BackboneEnvelope::new(
                    BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
                        key: "relay_test".to_string(),
                        value: serde_json::json!({"ok": true}),
                    }),
                    &session_id,
                    SourceInfo {
                        connector_id: "relay-test".to_string(),
                        connector_type: "remote_acp_backend".to_string(),
                        executor_id: None,
                    },
                );
                let _ = route
                    .tx
                    .send(RelaySessionEvent::Notification(Box::new(envelope)));
            }
            Ok("turn-1".to_string())
        }

        async fn relay_cancel(
            &self,
            backend_id: &str,
            session_id: &str,
        ) -> Result<(), agentdash_application_ports::backend_transport::TransportError> {
            self.cancelled
                .lock()
                .unwrap()
                .push((backend_id.to_string(), session_id.to_string()));
            Ok(())
        }

        async fn relay_steer(
            &self,
            backend_id: &str,
            payload: RelaySteerRequest,
        ) -> Result<(), agentdash_application_ports::backend_transport::TransportError> {
            self.steers
                .lock()
                .unwrap()
                .push((backend_id.to_string(), payload));
            Ok(())
        }

        fn list_online_executors(
            &self,
        ) -> Vec<agentdash_application_ports::backend_transport::RemoteExecutorInfo> {
            self.executors.lock().unwrap().clone()
        }

        async fn resolve_backend(
            &self,
            _executor_id: &str,
            _preferred_backend_id: Option<&str>,
        ) -> Result<String, agentdash_application_ports::backend_transport::TransportError>
        {
            Ok("backend-1".to_string())
        }

        fn register_session_sink(&self, route: RelaySessionRoute) {
            self.sinks
                .lock()
                .unwrap()
                .insert(route.session_id.clone(), route);
        }

        fn unregister_session_sink(&self, session_id: &str) {
            self.sinks.lock().unwrap().remove(session_id);
        }

        fn has_session_sink(&self, session_id: &str) -> bool {
            self.sinks.lock().unwrap().contains_key(session_id)
        }

        fn session_route(
            &self,
            session_id: &str,
        ) -> Option<agentdash_application_ports::backend_transport::RelaySessionRouteInfo> {
            self.sinks.lock().unwrap().get(session_id).map(|route| {
                agentdash_application_ports::backend_transport::RelaySessionRouteInfo {
                    session_id: route.session_id.clone(),
                    backend_id: route.backend_id.clone(),
                    lease_id: route.lease_id,
                }
            })
        }
    }

    #[async_trait]
    impl BackendExecutionLeaseRepository for MemoryLeaseRepository {
        async fn claim(&self, lease: &BackendExecutionLease) -> Result<(), DomainError> {
            self.claims.lock().unwrap().push(lease.clone());
            Ok(())
        }

        async fn activate(
            &self,
            lease_id: Uuid,
            _activated_at: chrono::DateTime<chrono::Utc>,
        ) -> Result<(), DomainError> {
            self.activations.lock().unwrap().push(lease_id);
            Ok(())
        }

        async fn release(
            &self,
            lease_id: Uuid,
            terminal_kind: Option<BackendExecutionTerminalKind>,
            reason: Option<String>,
            _released_at: chrono::DateTime<chrono::Utc>,
        ) -> Result<(), DomainError> {
            self.releases.lock().unwrap().push(RecordedRelease {
                lease_id,
                terminal_kind,
                reason,
            });
            Ok(())
        }

        async fn fail(
            &self,
            lease_id: Uuid,
            reason: Option<String>,
            _failed_at: chrono::DateTime<chrono::Utc>,
        ) -> Result<(), DomainError> {
            self.failures
                .lock()
                .unwrap()
                .push(RecordedFailure { lease_id, reason });
            Ok(())
        }

        async fn mark_lost_by_backend(
            &self,
            _backend_id: &str,
            _reason: Option<String>,
            _lost_at: chrono::DateTime<chrono::Utc>,
        ) -> Result<u64, DomainError> {
            Ok(0)
        }

        async fn get_by_id(
            &self,
            _lease_id: Uuid,
        ) -> Result<Option<BackendExecutionLease>, DomainError> {
            Ok(None)
        }

        async fn list_active(&self) -> Result<Vec<BackendExecutionLease>, DomainError> {
            Ok(Vec::new())
        }

        async fn count_active_by_backend(
            &self,
            backend_ids: &[String],
        ) -> Result<HashMap<String, i64>, DomainError> {
            let counts = self.active_counts.lock().unwrap();
            Ok(backend_ids
                .iter()
                .map(|id| (id.clone(), counts.get(id).copied().unwrap_or_default()))
                .collect())
        }
    }

    fn memory_lease_repo() -> Arc<dyn BackendExecutionLeaseRepository> {
        Arc::new(MemoryLeaseRepository::default())
    }

    fn register_executor(transport: &CaptureTransport, backend_id: &str, executor_id: &str) {
        transport.executors.lock().unwrap().push(
            agentdash_application_ports::backend_transport::RemoteExecutorInfo {
                backend_id: backend_id.to_string(),
                executor_id: executor_id.to_string(),
                executor_name: executor_id.to_string(),
                variants: Vec::new(),
                available: true,
            },
        );
    }

    fn relay_context(root: &Path, turn_id: &str) -> ExecutionContext {
        ExecutionContext {
            session: agentdash_spi::ExecutionSessionFrame {
                turn_id: turn_id.to_string(),
                working_directory: root.to_path_buf(),
                environment_variables: HashMap::new(),
                executor_config: AgentConfig::new("REMOTE_EXECUTOR"),
                mcp_servers: Vec::new(),
                vfs: Some(crate::session::local_workspace_vfs(root)),
                backend_execution: Some(ExecutionBackendPlacement {
                    backend_id: "local".to_string(),
                    lease_id: Uuid::new_v4(),
                    selection_mode: BackendExecutionSelectionMode::WorkspaceBinding,
                }),
                identity: None,
            },
            turn: agentdash_spi::ExecutionTurnFrame {
                capability_state: CapabilityState::default(),
                ..Default::default()
            },
        }
    }

    #[tokio::test]
    async fn relay_prompt_payload_passes_full_mcp_and_projects_working_dir() {
        let transport = Arc::new(CaptureTransport::default());
        register_executor(&transport, "local", "REMOTE_EXECUTOR");
        let connector = RelayAgentConnector::new(transport.clone(), memory_lease_repo());
        let root = tempfile::tempdir().expect("workspace");
        let mcp_server = agentdash_spi::SessionMcpServer {
            name: "third_party_mcp".to_string(),
            transport: agentdash_spi::McpTransportConfig::Stdio {
                command: "cmd".to_string(),
                args: Vec::new(),
                env: Vec::new(),
            },
            uses_relay: false,
        };
        let context = ExecutionContext {
            session: agentdash_spi::ExecutionSessionFrame {
                turn_id: "turn-1".to_string(),
                working_directory: root.path().join("crates/app"),
                environment_variables: HashMap::new(),
                executor_config: AgentConfig::new("REMOTE_EXECUTOR"),
                mcp_servers: vec![mcp_server],
                vfs: Some(crate::session::local_workspace_vfs(root.path())),
                backend_execution: Some(ExecutionBackendPlacement {
                    backend_id: "local".to_string(),
                    lease_id: Uuid::new_v4(),
                    selection_mode: BackendExecutionSelectionMode::WorkspaceBinding,
                }),
                identity: None,
            },
            turn: agentdash_spi::ExecutionTurnFrame {
                capability_state: CapabilityState::default(),
                ..Default::default()
            },
        };

        let _stream = connector
            .prompt(
                "session-1",
                None,
                &PromptPayload::Text("hello".to_string()),
                context,
            )
            .await
            .expect("relay prompt should succeed");

        let payload = transport
            .payload
            .lock()
            .await
            .clone()
            .expect("payload should be captured");
        assert_eq!(payload.working_dir.as_deref(), Some("crates/app"));
        assert_eq!(payload.mcp_servers.len(), 1);
        assert_eq!(
            payload.mcp_servers[0]
                .get("name")
                .and_then(serde_json::Value::as_str),
            Some("third_party_mcp")
        );
    }

    #[tokio::test]
    async fn relay_prompt_registers_sink_before_remote_prompt_can_emit_notification() {
        let transport = Arc::new(CaptureTransport::default());
        register_executor(&transport, "local", "REMOTE_EXECUTOR");
        let connector = RelayAgentConnector::new(transport.clone(), memory_lease_repo());
        let root = tempfile::tempdir().expect("workspace");
        let context = ExecutionContext {
            session: agentdash_spi::ExecutionSessionFrame {
                turn_id: "turn-1".to_string(),
                working_directory: root.path().to_path_buf(),
                environment_variables: HashMap::new(),
                executor_config: AgentConfig::new("REMOTE_EXECUTOR"),
                mcp_servers: Vec::new(),
                vfs: Some(crate::session::local_workspace_vfs(root.path())),
                backend_execution: Some(ExecutionBackendPlacement {
                    backend_id: "local".to_string(),
                    lease_id: Uuid::new_v4(),
                    selection_mode: BackendExecutionSelectionMode::WorkspaceBinding,
                }),
                identity: None,
            },
            turn: agentdash_spi::ExecutionTurnFrame {
                capability_state: CapabilityState::default(),
                ..Default::default()
            },
        };

        let mut stream = connector
            .prompt(
                "session-early-event",
                None,
                &PromptPayload::Text("hello".to_string()),
                context,
            )
            .await
            .expect("relay prompt should succeed");

        let notification = stream
            .next()
            .await
            .expect("notification emitted during relay_prompt should be buffered")
            .expect("notification should be successful");

        assert_eq!(notification.session_id, "session-early-event");
        assert!(transport.has_session_sink("session-early-event"));
        drop(stream);
        assert!(!transport.has_session_sink("session-early-event"));
    }

    #[tokio::test]
    async fn auto_idle_backend_selection_prefers_fewer_active_leases() {
        let transport = CaptureTransport::default();
        *transport.executors.lock().unwrap() = vec![
            agentdash_application_ports::backend_transport::RemoteExecutorInfo {
                backend_id: "backend-busy".to_string(),
                executor_id: "CODEX".to_string(),
                executor_name: "Codex".to_string(),
                variants: Vec::new(),
                available: true,
            },
            agentdash_application_ports::backend_transport::RemoteExecutorInfo {
                backend_id: "backend-idle".to_string(),
                executor_id: "CODEX".to_string(),
                executor_name: "Codex".to_string(),
                variants: Vec::new(),
                available: true,
            },
        ];
        let lease_repo = MemoryLeaseRepository::default();
        lease_repo
            .active_counts
            .lock()
            .unwrap()
            .insert("backend-busy".to_string(), 2);

        let plan = resolve_backend_execution_placement(
            &transport,
            &lease_repo,
            &BackendSelectionRequest::auto_idle("CODEX", Some("test".to_string())),
        )
        .await
        .expect("auto idle selection");

        assert_eq!(plan.backend_id, "backend-idle");
        assert_eq!(plan.selection_mode, BackendExecutionSelectionMode::AutoIdle);
    }

    #[tokio::test]
    async fn relay_prompt_failure_marks_lease_failed_and_unregisters_route() {
        let transport = Arc::new(CaptureTransport::default());
        register_executor(&transport, "local", "REMOTE_EXECUTOR");
        *transport.prompt_error.lock().unwrap() = Some(
            agentdash_application_ports::backend_transport::TransportError::OperationFailed(
                "boom".to_string(),
            ),
        );
        let lease_repo = Arc::new(MemoryLeaseRepository::default());
        let connector = RelayAgentConnector::new(transport.clone(), lease_repo.clone());
        let root = tempfile::tempdir().expect("workspace");
        let context = relay_context(root.path(), "turn-failed-prompt");
        let lease_id = context.session.backend_execution.as_ref().unwrap().lease_id;

        let error = match connector
            .prompt(
                "session-failed-prompt",
                None,
                &PromptPayload::Text("hello".to_string()),
                context,
            )
            .await
        {
            Ok(_) => panic!("relay prompt should fail"),
            Err(error) => error,
        };

        assert!(error.to_string().contains("relay prompt 失败"));
        let failures = lease_repo.failures.lock().unwrap();
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].lease_id, lease_id);
        assert!(
            failures[0]
                .reason
                .as_deref()
                .is_some_and(|reason| { reason.contains("boom") })
        );
        assert!(!transport.has_session_sink("session-failed-prompt"));
    }

    #[tokio::test]
    async fn terminal_completed_releases_lease_and_unregisters_route() {
        let transport = Arc::new(CaptureTransport::default());
        register_executor(&transport, "local", "REMOTE_EXECUTOR");
        let lease_repo = Arc::new(MemoryLeaseRepository::default());
        let connector = RelayAgentConnector::new(transport.clone(), lease_repo.clone());
        let root = tempfile::tempdir().expect("workspace");

        let mut stream = connector
            .prompt(
                "session-completed",
                None,
                &PromptPayload::Text("hello".to_string()),
                relay_context(root.path(), "turn-completed"),
            )
            .await
            .expect("relay prompt should succeed");
        stream
            .next()
            .await
            .expect("initial notification")
            .expect("notification should be successful");
        let route = transport
            .sinks
            .lock()
            .unwrap()
            .get("session-completed")
            .expect("route should be registered")
            .tx
            .clone();
        route
            .send(RelaySessionEvent::Terminal {
                kind: RelayTerminalKind::Completed,
                message: Some("done".to_string()),
            })
            .expect("terminal should be delivered");

        assert!(stream.next().await.is_none());
        let releases = lease_repo.releases.lock().unwrap();
        assert_eq!(releases.len(), 1);
        assert_eq!(
            releases[0].terminal_kind,
            Some(BackendExecutionTerminalKind::Completed)
        );
        assert_eq!(releases[0].reason.as_deref(), Some("done"));
        assert!(!transport.has_session_sink("session-completed"));
    }

    #[tokio::test]
    async fn terminal_failed_releases_lease_with_failed_kind() {
        let transport = Arc::new(CaptureTransport::default());
        register_executor(&transport, "local", "REMOTE_EXECUTOR");
        let lease_repo = Arc::new(MemoryLeaseRepository::default());
        let connector = RelayAgentConnector::new(transport.clone(), lease_repo.clone());
        let root = tempfile::tempdir().expect("workspace");

        let mut stream = connector
            .prompt(
                "session-terminal-failed",
                None,
                &PromptPayload::Text("hello".to_string()),
                relay_context(root.path(), "turn-terminal-failed"),
            )
            .await
            .expect("relay prompt should succeed");
        stream
            .next()
            .await
            .expect("initial notification")
            .expect("notification should be successful");
        let route = transport
            .sinks
            .lock()
            .unwrap()
            .get("session-terminal-failed")
            .expect("route should be registered")
            .tx
            .clone();
        route
            .send(RelaySessionEvent::Terminal {
                kind: RelayTerminalKind::Failed,
                message: Some("remote failed".to_string()),
            })
            .expect("terminal should be delivered");

        let error = stream
            .next()
            .await
            .expect("failed terminal should emit an error")
            .expect_err("terminal failed should surface as connector error");
        assert!(error.to_string().contains("remote failed"));
        let releases = lease_repo.releases.lock().unwrap();
        assert_eq!(releases.len(), 1);
        assert_eq!(
            releases[0].terminal_kind,
            Some(BackendExecutionTerminalKind::Failed)
        );
        assert_eq!(releases[0].reason.as_deref(), Some("remote failed"));
        assert!(!transport.has_session_sink("session-terminal-failed"));
    }

    #[tokio::test]
    async fn cancel_uses_session_route_backend_and_releases_interrupted() {
        let transport = Arc::new(CaptureTransport::default());
        register_executor(&transport, "backend-route", "REMOTE_EXECUTOR");
        let lease_repo = Arc::new(MemoryLeaseRepository::default());
        let connector = RelayAgentConnector::new(transport.clone(), lease_repo.clone());
        let root = tempfile::tempdir().expect("workspace");
        let mut context = relay_context(root.path(), "turn-cancel");
        context.session.vfs.as_mut().unwrap().mounts[0].backend_id = "backend-route".to_string();
        context
            .session
            .backend_execution
            .as_mut()
            .unwrap()
            .backend_id = "backend-route".to_string();
        let lease_id = context.session.backend_execution.as_ref().unwrap().lease_id;

        let stream = connector
            .prompt(
                "session-cancel",
                None,
                &PromptPayload::Text("hello".to_string()),
                context,
            )
            .await
            .expect("relay prompt should succeed");

        connector
            .cancel("session-cancel")
            .await
            .expect("relay cancel should succeed");

        assert_eq!(
            transport.cancelled.lock().unwrap().as_slice(),
            &[("backend-route".to_string(), "session-cancel".to_string())]
        );
        let releases = lease_repo.releases.lock().unwrap();
        assert_eq!(releases.len(), 1);
        assert_eq!(releases[0].lease_id, lease_id);
        assert_eq!(
            releases[0].terminal_kind,
            Some(BackendExecutionTerminalKind::Interrupted)
        );
        assert_eq!(releases[0].reason.as_deref(), Some("cancelled"));
        assert!(!transport.has_session_sink("session-cancel"));
        drop(stream);
    }

    #[tokio::test]
    async fn steer_uses_session_route_without_releasing_live_sink() {
        let transport = Arc::new(CaptureTransport::default());
        register_executor(&transport, "backend-route", "REMOTE_EXECUTOR");
        let lease_repo = Arc::new(MemoryLeaseRepository::default());
        let connector = RelayAgentConnector::new(transport.clone(), lease_repo.clone());
        let root = tempfile::tempdir().expect("workspace");
        let mut context = relay_context(root.path(), "turn-steer");
        context.session.vfs.as_mut().unwrap().mounts[0].backend_id = "backend-route".to_string();
        context
            .session
            .backend_execution
            .as_mut()
            .unwrap()
            .backend_id = "backend-route".to_string();

        let stream = connector
            .prompt(
                "session-steer",
                None,
                &PromptPayload::Text("hello".to_string()),
                context,
            )
            .await
            .expect("relay prompt should succeed");

        connector
            .steer_session(
                "session-steer",
                "turn-steer",
                vec![
                    agentdash_agent_protocol::codex_app_server_protocol::UserInput::Text {
                        text: "adjust".to_string(),
                        text_elements: Vec::new(),
                    },
                ],
            )
            .await
            .expect("relay steer should succeed");

        let steers = transport.steers.lock().unwrap();
        assert_eq!(steers.len(), 1);
        assert_eq!(steers[0].0, "backend-route");
        assert_eq!(steers[0].1.session_id, "session-steer");
        assert_eq!(steers[0].1.expected_turn_id, "turn-steer");
        assert_eq!(
            steers[0].1.input,
            vec![
                agentdash_agent_protocol::codex_app_server_protocol::UserInput::Text {
                    text: "adjust".to_string(),
                    text_elements: Vec::new(),
                }
            ]
        );
        assert!(transport.cancelled.lock().unwrap().is_empty());
        assert!(lease_repo.releases.lock().unwrap().is_empty());
        assert!(transport.has_session_sink("session-steer"));
        drop(stream);
    }
}
