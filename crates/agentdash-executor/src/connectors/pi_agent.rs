/// PiAgentConnector — 基于 agentdash-agent 的进程内 Agent 连接器
///
/// 与 `VibeKanbanExecutorsConnector`（通过子进程执行）不同，
/// PiAgentConnector 在进程内运行 Agent Loop，直接调用 LLM API。
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use agent_client_protocol::{
    ContentBlock, ContentChunk, ImageContent, SessionId, SessionNotification, SessionUpdate,
    TextContent, ToolCall, ToolCallContent, ToolCallId, ToolCallStatus, ToolCallUpdate,
    ToolCallUpdateFields, ToolKind,
};
use futures::stream::BoxStream;
use tokio::sync::Mutex;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;

use agentdash_acp_meta::{
    AgentDashEventV1, AgentDashMetaV1, AgentDashSourceV1, AgentDashTraceV1, merge_agentdash_meta,
};

use agentdash_agent::{
    Agent, AgentConfig, AgentEvent, AgentMessage, AgentToolResult, ContentPart, DynAgentTool,
    LlmBridge,
};
use agentdash_domain::settings::SettingsRepository;

use crate::connector::{
    AgentConnector, ConnectorCapabilities, ConnectorError, ConnectorType, ExecutionContext,
    ExecutionMount, ExecutionMountCapability, ExecutionStream, ExecutorInfo, PromptPayload,
    RuntimeToolProvider,
};
use crate::connectors::pi_agent_mcp::discover_mcp_tools;
use crate::connectors::pi_agent_provider_registry::{
    CONTEXT_WINDOW_STANDARD, ProviderEntry, build_provider_entries,
};
use crate::hook_events::build_hook_trace_notification;
use crate::runtime_delegate::HookRuntimeDelegate;

// ─── PiAgentConnector ───────────────────────────────────────────

pub struct PiAgentConnector {
    #[allow(dead_code)]
    workspace_root: PathBuf,
    /// 默认 bridge（向后兼容，无 provider 注册或 model_id 未匹配时使用）
    bridge: Arc<dyn LlmBridge>,
    /// 已注册的 provider 列表（按注册顺序，首个命中的 provider 优先）
    providers: Vec<ProviderEntry>,
    tools: Vec<DynAgentTool>,
    runtime_tool_provider: Option<Arc<dyn RuntimeToolProvider>>,
    settings_repo: Option<Arc<dyn SettingsRepository>>,
    system_prompt: String,
    model_id: String,
    agents: Arc<Mutex<HashMap<String, Agent>>>,
}

struct ProviderRuntimeState {
    default_bridge: Option<Arc<dyn LlmBridge>>,
    default_model: Option<String>,
    providers: Vec<ProviderEntry>,
}

impl ProviderRuntimeState {
    fn is_configured(&self) -> bool {
        self.default_bridge.is_some() && self.default_model.is_some()
    }
}

impl PiAgentConnector {
    pub fn new(
        workspace_root: PathBuf,
        bridge: Arc<dyn LlmBridge>,
        system_prompt: impl Into<String>,
        model_id: impl Into<String>,
    ) -> Self {
        Self {
            workspace_root,
            bridge,
            providers: Vec::new(),
            tools: Vec::new(),
            runtime_tool_provider: None,
            settings_repo: None,
            system_prompt: system_prompt.into(),
            model_id: model_id.into(),
            agents: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn add_tool(&mut self, tool: DynAgentTool) {
        self.tools.push(tool);
    }

    pub fn set_runtime_tool_provider(&mut self, provider: Arc<dyn RuntimeToolProvider>) {
        self.runtime_tool_provider = Some(provider);
    }

    pub fn set_settings_repository(&mut self, settings_repo: Arc<dyn SettingsRepository>) {
        self.settings_repo = Some(settings_repo);
    }

    fn add_provider(&mut self, provider: ProviderEntry) {
        self.providers.push(provider);
    }

    async fn load_provider_runtime_state(&self) -> ProviderRuntimeState {
        if let Some(settings_repo) = &self.settings_repo {
            let providers = build_provider_entries(settings_repo.as_ref()).await;
            let default_model = providers
                .first()
                .map(|provider| provider.entry.default_model.clone());
            let default_bridge = providers
                .first()
                .map(|provider| provider.default_bridge.clone());
            return ProviderRuntimeState {
                default_bridge,
                default_model,
                providers: providers
                    .into_iter()
                    .map(|provider| provider.entry)
                    .collect(),
            };
        }

        if self.providers.is_empty() || self.model_id.trim().is_empty() {
            return ProviderRuntimeState {
                default_bridge: None,
                default_model: None,
                providers: Vec::new(),
            };
        }

        ProviderRuntimeState {
            default_bridge: Some(self.bridge.clone()),
            default_model: Some(self.model_id.clone()),
            providers: self.providers.clone(),
        }
    }

    fn create_agent_with_bridge(&self, bridge: Arc<dyn LlmBridge>) -> Agent {
        let config = AgentConfig {
            system_prompt: self.system_prompt.clone(),
            ..AgentConfig::default()
        };
        let mut agent = Agent::new(bridge, config);
        agent.set_tools(self.tools.clone());
        agent
    }

    #[allow(dead_code)]
    fn create_agent(&self) -> Agent {
        self.create_agent_with_bridge(self.bridge.clone())
    }

    async fn resolve_bridge_for_execution(
        &self,
        provider_state: &ProviderRuntimeState,
        provider_id: Option<&str>,
        model_id: Option<&str>,
    ) -> Result<Arc<dyn LlmBridge>, ConnectorError> {
        let default_bridge = provider_state.default_bridge.clone().ok_or_else(|| {
            ConnectorError::InvalidConfig("Pi Agent 尚未配置任何可用的 LLM Provider".to_string())
        })?;
        let provider_id = provider_id.map(str::trim).filter(|item| !item.is_empty());
        let model_id = model_id.map(str::trim).filter(|item| !item.is_empty());

        if provider_id.is_none() && model_id.is_none() {
            return Ok(default_bridge);
        }

        if let Some(provider_id) = provider_id
            && let Some(provider) = provider_state
                .providers
                .iter()
                .find(|provider| provider.provider_id == provider_id)
        {
            let resolved_model = model_id.unwrap_or(provider.default_model.as_str());
            return Ok(provider.create_bridge(resolved_model));
        }

        if let Some(model_id) = model_id {
            if provider_state.default_model.as_deref() == Some(model_id) {
                return Ok(default_bridge.clone());
            }

            for provider in &provider_state.providers {
                if provider.supports_model(model_id).await {
                    return Ok(provider.create_bridge(model_id));
                }
            }
        }

        Ok(default_bridge)
    }

    fn build_runtime_system_prompt(
        &self,
        context: &ExecutionContext,
        tool_names: &[String],
    ) -> String {
        let mut sections = vec![self.system_prompt.clone()];

        // 会话级 owner 上下文（project/story markdown 摘要）注入到 system prompt 头部
        // 仅在第一个 section 之后立即插入，确保 Agent 能在每轮对话中感知完整上下文
        if let Some(ref ctx) = context.system_context
            && !ctx.trim().is_empty()
        {
            sections.push(ctx.clone());
        }

        if let Some(address_space) = &context.address_space {
            let mount_lines = address_space
                .mounts
                .iter()
                .map(describe_mount)
                .collect::<Vec<_>>()
                .join("\n");
            let default_mount = address_space
                .default_mount()
                .map(|mount| mount.id.as_str())
                .unwrap_or("main");
            sections.push(format!(
                "当前会话可访问的 Address Space 挂载如下：\n{}\n默认 mount：{}",
                mount_lines, default_mount
            ));
        } else {
            let current_dir_display =
                workspace_relative_display(&context.workspace_root, &context.working_directory);
            sections.push(format!(
                "工作空间路径锚点：.\n工作空间绝对路径（仅供参考，不要直接写入工具参数）：{}\n当前工作目录（相对工作空间）：{}",
                context.workspace_root.display(),
                current_dir_display
            ));
        }

        if !tool_names.is_empty() {
            if context.address_space.is_some() {
                sections.push(format!(
                    "你当前可调用的统一访问工具有：{}。优先使用 mounts_list / fs_read / fs_list / fs_search / fs_write / shell_exec，不要臆测文件内容。",
                    tool_names.join("、")
                ));
                sections.push(
                    "调用这些工具时，优先使用 `mount + 相对路径`。除非确有多个 mount，否则默认优先用 `main`。不要把 backend_id 或绝对路径直接写进工具参数。执行 shell 时，`cwd` 也必须是相对 mount 根目录的路径；当前目录就传 `.`。".to_string(),
                );
            } else {
                sections.push(format!(
                    "你当前可调用的内置工具有：{}。优先使用工具读取/搜索/执行，不要臆测文件内容。",
                    tool_names.join("、")
                ));
                sections.push(
                    "调用 read_file、list_directory、search、write_file、shell 等工作空间工具时，路径参数必须优先使用相对工作空间根目录的路径。如果要在当前目录执行 shell，请将 cwd 设为 `.`；如果要进入子目录，请传类似 `crates/agentdash-agent` 这样的相对路径；不要把 `F:\\...` 这类绝对路径直接写进工具参数。".to_string(),
                );
            }
        }

        if !context.mcp_servers.is_empty() {
            let server_lines = context
                .mcp_servers
                .iter()
                .map(describe_mcp_server)
                .collect::<Vec<_>>()
                .join("\n");
            sections.push(format!(
                "以下 MCP Server 已注入当前会话，可在需要时使用：\n{}",
                server_lines
            ));
        }

        if let Some(hook_session) = &context.hook_session {
            let hook_sections = build_hook_runtime_sections(hook_session.as_ref());
            if !hook_sections.is_empty() {
                sections.extend(hook_sections);
            }
        }

        sections.join("\n\n")
    }
}

fn workspace_relative_display(workspace_root: &Path, path: &Path) -> String {
    path.strip_prefix(workspace_root)
        .ok()
        .map(|relative| {
            if relative.as_os_str().is_empty() {
                ".".to_string()
            } else {
                relative.to_string_lossy().replace('\\', "/")
            }
        })
        .unwrap_or_else(|| path.display().to_string())
}

fn describe_mount(mount: &ExecutionMount) -> String {
    let capabilities = mount
        .capabilities
        .iter()
        .map(|capability| match capability {
            ExecutionMountCapability::Read => "read",
            ExecutionMountCapability::Write => "write",
            ExecutionMountCapability::List => "list",
            ExecutionMountCapability::Search => "search",
            ExecutionMountCapability::Exec => "exec",
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "- {}: {}（provider={}, root_ref={}, capabilities=[{}]）",
        mount.id, mount.display_name, mount.provider, mount.root_ref, capabilities
    )
}

fn build_hook_runtime_sections(hook_session: &crate::hooks::HookSessionRuntime) -> Vec<String> {
    let mut sections = vec![
        "当前会话启用了 Hook Runtime。active workflow、流程约束、stop gate 与 pending action 等动态治理信息，会在每次 LLM 调用边界由 runtime 注入；这里不再重复展开它们的静态副本。".to_string(),
    ];

    let pending_actions = hook_session.pending_actions();
    if !pending_actions.is_empty() {
        sections.push(format!(
            "当前已有 {} 条待处理 hook action；请在后续动态注入消息中优先处理它们。",
            pending_actions.len()
        ));
    }

    sections
}

#[async_trait::async_trait]
impl AgentConnector for PiAgentConnector {
    fn connector_id(&self) -> &'static str {
        "pi-agent"
    }

    fn connector_type(&self) -> ConnectorType {
        ConnectorType::LocalExecutor
    }

    fn capabilities(&self) -> ConnectorCapabilities {
        ConnectorCapabilities {
            supports_cancel: true,
            supports_discovery: true,
            supports_variants: false,
            supports_model_override: true,
            supports_permission_policy: false,
        }
    }

    fn list_executors(&self) -> Vec<ExecutorInfo> {
        vec![ExecutorInfo {
            id: "PI_AGENT".to_string(),
            name: "Pi Agent".to_string(),
            variants: vec![],
            available: true,
        }]
    }

    async fn discover_options_stream(
        &self,
        _executor: &str,
        _variant: Option<&str>,
        _working_dir: Option<PathBuf>,
    ) -> Result<BoxStream<'static, json_patch::Patch>, ConnectorError> {
        let provider_state = self.load_provider_runtime_state().await;
        let mut all_providers: Vec<serde_json::Value> = vec![];
        let mut all_models: Vec<serde_json::Value> = vec![];

        for provider in &provider_state.providers {
            all_providers.push(serde_json::json!({
                "id": provider.provider_id,
                "name": provider.provider_name,
            }));

            for model in provider.load_models_with_block_state().await {
                all_models.push(serde_json::json!({
                    "id": model.id,
                    "name": model.name,
                    "provider_id": provider.provider_id,
                    "reasoning": model.reasoning,
                    "context_window": model.context_window,
                    "max_tokens": model.max_tokens,
                    "blocked": model.blocked,
                }));
            }
        }

        // 若没有注册任何 provider，退化为单模型显示（向后兼容）
        if all_providers.is_empty()
            && let Some(model_id) = provider_state
                .default_model
                .clone()
                .filter(|item| !item.trim().is_empty())
        {
            all_providers.push(serde_json::json!({
                "id": "default",
                "name": "Default",
            }));
            all_models.push(serde_json::json!({
                "id": model_id,
                "name": model_id,
                "provider_id": "default",
                "reasoning": false,
                "context_window": CONTEXT_WINDOW_STANDARD,
                "max_tokens": 16_384u64,
                "blocked": false,
            }));
        }

        let default_model = provider_state.default_model.clone();

        let patch: json_patch::Patch = serde_json::from_value(serde_json::json!([
            { "op": "replace", "path": "/options/model_selector/providers", "value": all_providers },
            { "op": "replace", "path": "/options/model_selector/models", "value": all_models },
            { "op": "replace", "path": "/options/model_selector/default_model", "value": default_model },
            { "op": "replace", "path": "/options/loading_models", "value": false },
            { "op": "replace", "path": "/options/loading_agents", "value": false },
            { "op": "replace", "path": "/options/loading_slash_commands", "value": false }
        ])).expect("static patch must be valid");

        Ok(Box::pin(futures::stream::once(async move { patch })))
    }

    async fn prompt(
        &self,
        session_id: &str,
        _follow_up_session_id: Option<&str>,
        prompt: &PromptPayload,
        context: ExecutionContext,
    ) -> Result<ExecutionStream, ConnectorError> {
        let prompt_text = prompt.to_fallback_text();
        let prompt_text = prompt_text.trim().to_string();
        if prompt_text.is_empty() {
            return Err(ConnectorError::InvalidConfig("prompt 内容为空".to_string()));
        }

        let existing_agent = {
            let mut agents = self.agents.lock().await;
            agents.remove(session_id)
        };

        let mut agent = if let Some(agent) = existing_agent {
            agent
        } else {
            let provider_state = self.load_provider_runtime_state().await;
            if !provider_state.is_configured() {
                return Err(ConnectorError::InvalidConfig(
                    "Pi Agent 尚未配置任何可用的 LLM Provider，请先在设置页保存 Provider 配置"
                        .to_string(),
                ));
            }
            let bridge = self
                .resolve_bridge_for_execution(
                    &provider_state,
                    context.executor_config.provider_id.as_deref(),
                    context.executor_config.model_id.as_deref(),
                )
                .await?;
            self.create_agent_with_bridge(bridge)
        };

        let mcp_tools = match discover_mcp_tools(&context.mcp_servers).await {
            Ok(tools) => tools,
            Err(error) => {
                tracing::warn!("发现 MCP 工具失败，继续使用本地工具: {error}");
                Vec::new()
            }
        };
        let mut runtime_tools = self.tools.clone();
        let provider = self.runtime_tool_provider.as_ref().ok_or_else(|| {
            ConnectorError::InvalidConfig(
                "PiAgentConnector 未配置 runtime tool provider".to_string(),
            )
        })?;
        runtime_tools.extend(provider.build_tools(&context).await?);
        runtime_tools.extend(mcp_tools);
        let tool_names = runtime_tools
            .iter()
            .map(|tool| tool.name().to_string())
            .collect::<Vec<_>>();
        agent.set_tools(runtime_tools);
        agent.set_system_prompt(self.build_runtime_system_prompt(&context, &tool_names));
        let (hook_trace_tx, hook_trace_rx) = if context.hook_session.is_some() {
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };
        agent.set_runtime_delegate(context.hook_session.clone().map(|hook_session| {
            HookRuntimeDelegate::new_with_trace_events(hook_session, hook_trace_tx)
        }));

        if let Some(thinking_level) = context.executor_config.thinking_level {
            agent.set_thinking_level(thinking_level);
        }

        let (event_rx, join_handle) = agent
            .prompt(AgentMessage::user(&prompt_text))
            .map_err(|error| ConnectorError::Runtime(format!("Pi Agent 启动失败: {error}")))?;

        let session_id_owned = session_id.to_string();
        self.agents
            .lock()
            .await
            .insert(session_id_owned.clone(), agent);

        let mut source = AgentDashSourceV1::new(self.connector_id(), "local_executor");
        source.executor_id = Some("PI_AGENT".to_string());
        let turn_id = context.turn_id.clone();
        let acp_session_id = SessionId::new(session_id.to_string());

        let (tx, rx) =
            tokio::sync::mpsc::channel::<Result<SessionNotification, ConnectorError>>(256);

        tokio::spawn(async move {
            let mut entry_index: u32 = 0;
            let mut event_rx = event_rx;
            let mut hook_trace_rx = hook_trace_rx;

            loop {
                if let Some(receiver) = hook_trace_rx.as_mut() {
                    tokio::select! {
                        maybe_trace = receiver.recv() => {
                            if let Some(entry) = maybe_trace
                                && let Some(notification) = build_hook_trace_notification(
                                    acp_session_id.0.as_ref(),
                                    Some(&turn_id),
                                    source.clone(),
                                    &entry,
                                )
                                && tx.send(Ok(notification)).await.is_err()
                            {
                                return;
                            }
                        }
                        maybe_event = event_rx.next() => {
                            let Some(event) = maybe_event else {
                                break;
                            };
                            let notifications = convert_event_to_notifications(
                                &event,
                                &acp_session_id,
                                &source,
                                &turn_id,
                                &mut entry_index,
                            );

                            for n in notifications {
                                if tx.send(Ok(n)).await.is_err() {
                                    return;
                                }
                            }
                        }
                    }
                    continue;
                }

                let Some(event) = event_rx.next().await else {
                    break;
                };

                let notifications = convert_event_to_notifications(
                    &event,
                    &acp_session_id,
                    &source,
                    &turn_id,
                    &mut entry_index,
                );

                for n in notifications {
                    if tx.send(Ok(n)).await.is_err() {
                        return;
                    }
                }
            }

            match join_handle.await {
                Ok(Ok(_messages)) => {}
                Ok(Err(e)) => {
                    let error = ConnectorError::Runtime(format!("Pi Agent loop 错误: {e}"));
                    tracing::error!("{error}");
                    let _ = tx.send(Err(error)).await;
                }
                Err(e) => {
                    let error = ConnectorError::Runtime(format!("Pi Agent task panic: {e}"));
                    tracing::error!("{error}");
                    let _ = tx.send(Err(error)).await;
                }
            }

            emit_pending_hook_trace_notifications(
                &mut hook_trace_rx,
                &tx,
                &acp_session_id,
                &source,
                &turn_id,
            )
            .await;
        });

        Ok(Box::pin(ReceiverStream::new(rx)))
    }

    async fn cancel(&self, session_id: &str) -> Result<(), ConnectorError> {
        if let Some(agent) = self.agents.lock().await.get(session_id) {
            agent.abort();
        }
        Ok(())
    }

    async fn approve_tool_call(
        &self,
        session_id: &str,
        tool_call_id: &str,
    ) -> Result<(), ConnectorError> {
        let agents = self.agents.lock().await;
        let agent = agents.get(session_id).ok_or_else(|| {
            ConnectorError::Runtime(format!("session `{session_id}` 当前没有活跃的 Pi Agent"))
        })?;
        agent
            .approve_tool_call(tool_call_id)
            .await
            .map_err(|error| ConnectorError::Runtime(error.to_string()))
    }

    async fn reject_tool_call(
        &self,
        session_id: &str,
        tool_call_id: &str,
        reason: Option<String>,
    ) -> Result<(), ConnectorError> {
        let agents = self.agents.lock().await;
        let agent = agents.get(session_id).ok_or_else(|| {
            ConnectorError::Runtime(format!("session `{session_id}` 当前没有活跃的 Pi Agent"))
        })?;
        agent
            .reject_tool_call(tool_call_id, reason)
            .await
            .map_err(|error| ConnectorError::Runtime(error.to_string()))
    }
}

async fn emit_pending_hook_trace_notifications(
    hook_trace_rx: &mut Option<tokio::sync::mpsc::UnboundedReceiver<crate::HookTraceEntry>>,
    tx: &tokio::sync::mpsc::Sender<Result<SessionNotification, ConnectorError>>,
    session_id: &SessionId,
    source: &AgentDashSourceV1,
    turn_id: &str,
) {
    let Some(receiver) = hook_trace_rx.as_mut() else {
        return;
    };

    while let Ok(entry) = receiver.try_recv() {
        if let Some(notification) = build_hook_trace_notification(
            session_id.0.as_ref(),
            Some(turn_id),
            source.clone(),
            &entry,
        )
            && tx.send(Ok(notification)).await.is_err()
        {
            return;
        }
    }
}

fn describe_mcp_server(server: &agent_client_protocol::McpServer) -> String {
    let value = serde_json::to_value(server).unwrap_or_default();
    let name = value
        .get("name")
        .and_then(|item| item.as_str())
        .unwrap_or("unnamed-mcp");
    let url = value
        .get("url")
        .and_then(|item| item.as_str())
        .unwrap_or("unknown-url");
    let server_type = value
        .get("type")
        .and_then(|item| item.as_str())
        .unwrap_or("unknown");
    format!("- {name} ({server_type}): {url}")
}

fn make_meta(
    source: &AgentDashSourceV1,
    turn_id: &str,
    entry_index: u32,
) -> agent_client_protocol::Meta {
    let mut trace = AgentDashTraceV1::new();
    trace.turn_id = Some(turn_id.to_string());
    trace.entry_index = Some(entry_index);

    let agentdash = AgentDashMetaV1::new()
        .source(Some(source.clone()))
        .trace(Some(trace));

    merge_agentdash_meta(None, &agentdash).expect("agentdash meta 不应为空")
}

fn make_event_notification(
    session_id: &SessionId,
    source: &AgentDashSourceV1,
    turn_id: &str,
    entry_index: u32,
    event_type: &str,
    severity: &str,
    message: String,
    data: serde_json::Value,
) -> SessionNotification {
    let mut trace = AgentDashTraceV1::new();
    trace.turn_id = Some(turn_id.to_string());
    trace.entry_index = Some(entry_index);

    let mut event = AgentDashEventV1::new(event_type);
    event.severity = Some(severity.to_string());
    event.message = Some(message);
    event.data = Some(data);

    let agentdash = AgentDashMetaV1::new()
        .source(Some(source.clone()))
        .trace(Some(trace))
        .event(Some(event));

    SessionNotification::new(
        session_id.clone(),
        SessionUpdate::SessionInfoUpdate(
            agent_client_protocol::SessionInfoUpdate::new()
                .meta(merge_agentdash_meta(None, &agentdash).unwrap_or_default()),
        ),
    )
}

fn convert_event_to_notifications(
    event: &AgentEvent,
    session_id: &SessionId,
    source: &AgentDashSourceV1,
    turn_id: &str,
    entry_index: &mut u32,
) -> Vec<SessionNotification> {
    match event {
        AgentEvent::MessageUpdate { event, .. } => match event {
            agentdash_agent::types::AssistantStreamEvent::TextDelta { text, .. } => {
                if text.is_empty() {
                    return Vec::new();
                }
                let meta = make_meta(source, turn_id, *entry_index);
                let chunk =
                    ContentChunk::new(ContentBlock::Text(TextContent::new(text))).meta(Some(meta));
                vec![SessionNotification::new(
                    session_id.clone(),
                    SessionUpdate::AgentMessageChunk(chunk),
                )]
            }
            agentdash_agent::types::AssistantStreamEvent::ThinkingDelta { text, .. } => {
                if text.is_empty() {
                    return Vec::new();
                }
                let meta = make_meta(source, turn_id, *entry_index);
                let chunk =
                    ContentChunk::new(ContentBlock::Text(TextContent::new(text))).meta(Some(meta));
                vec![SessionNotification::new(
                    session_id.clone(),
                    SessionUpdate::AgentThoughtChunk(chunk),
                )]
            }
            _ => Vec::new(),
        },

        AgentEvent::MessageEnd { message } => {
            if let AgentMessage::Assistant {
                content,
                error_message,
                tool_calls,
                ..
            } = message
            {
                let meta = make_meta(source, turn_id, *entry_index);
                let reasoning_text = content
                    .iter()
                    .filter_map(ContentPart::extract_reasoning)
                    .collect::<Vec<_>>()
                    .join("");
                let text = error_message.clone().unwrap_or_else(|| {
                    content
                        .iter()
                        .filter_map(ContentPart::extract_text)
                        .collect::<Vec<_>>()
                        .join("")
                });

                let mut notifications = Vec::new();
                if !reasoning_text.is_empty() {
                    let chunk =
                        ContentChunk::new(ContentBlock::Text(TextContent::new(reasoning_text)))
                            .meta(Some(meta.clone()));
                    notifications.push(SessionNotification::new(
                        session_id.clone(),
                        SessionUpdate::AgentThoughtChunk(chunk),
                    ));
                }
                if !text.is_empty() {
                    let chunk = ContentChunk::new(ContentBlock::Text(TextContent::new(text)))
                        .meta(Some(meta));
                    notifications.push(SessionNotification::new(
                        session_id.clone(),
                        SessionUpdate::AgentMessageChunk(chunk),
                    ));
                }

                let has_streamable_content = content.iter().any(|part| {
                    part.extract_text().is_some() || part.extract_reasoning().is_some()
                });
                if has_streamable_content || error_message.is_some() || !tool_calls.is_empty() {
                    *entry_index += 1;
                }
                return notifications;
            }
            Vec::new()
        }

        AgentEvent::ToolExecutionStart {
            tool_call_id,
            tool_name,
            args,
        } => {
            let meta = make_meta(source, turn_id, *entry_index);
            *entry_index += 1;

            let mut call = ToolCall::new(ToolCallId::new(tool_call_id.clone()), tool_name)
                .kind(ToolKind::Other)
                .status(ToolCallStatus::InProgress)
                .raw_input(Some(args.clone()));
            call.meta = Some(meta);

            vec![SessionNotification::new(
                session_id.clone(),
                SessionUpdate::ToolCall(call),
            )]
        }

        AgentEvent::ToolExecutionUpdate {
            tool_call_id,
            partial_result,
            ..
        } => {
            let meta = make_meta(source, turn_id, *entry_index);
            let mut fields = ToolCallUpdateFields::default();
            fields.status = Some(ToolCallStatus::InProgress);
            fields.raw_output = Some(partial_result.clone());
            if let Some(result) = decode_tool_result(partial_result) {
                let content = content_parts_to_tool_call_content(&result.content);
                if !content.is_empty() {
                    fields.content = Some(content);
                }
            }

            let mut update = ToolCallUpdate::new(ToolCallId::new(tool_call_id.clone()), fields);
            update.meta = Some(meta);

            vec![SessionNotification::new(
                session_id.clone(),
                SessionUpdate::ToolCallUpdate(update),
            )]
        }

        AgentEvent::ToolExecutionPendingApproval {
            tool_call_id,
            tool_name,
            args,
            reason,
            details,
            ..
        } => {
            let meta = make_meta(source, turn_id, *entry_index);
            let mut fields = ToolCallUpdateFields::default();
            fields.status = Some(ToolCallStatus::Pending);
            fields.raw_output = Some(serde_json::json!({
                "approval_state": "pending",
                "reason": reason,
                "details": details,
            }));
            fields.content = Some(vec![ToolCallContent::from(ContentBlock::Text(
                TextContent::new(format!("等待审批：{reason}")),
            ))]);

            let mut update = ToolCallUpdate::new(ToolCallId::new(tool_call_id.clone()), fields);
            update.meta = Some(meta);

            vec![
                SessionNotification::new(session_id.clone(), SessionUpdate::ToolCallUpdate(update)),
                make_event_notification(
                    session_id,
                    source,
                    turn_id,
                    *entry_index,
                    "approval_requested",
                    "warning",
                    format!("工具 `{tool_name}` 正等待审批"),
                    serde_json::json!({
                        "tool_call_id": tool_call_id,
                        "tool_name": tool_name,
                        "reason": reason,
                        "args": args,
                        "details": details,
                    }),
                ),
            ]
        }

        AgentEvent::ToolExecutionApprovalResolved {
            tool_call_id,
            tool_name,
            args,
            approved,
            reason,
            ..
        } => {
            let meta = make_meta(source, turn_id, *entry_index);
            let mut fields = ToolCallUpdateFields::default();
            fields.status = Some(if *approved {
                ToolCallStatus::InProgress
            } else {
                ToolCallStatus::Failed
            });
            fields.raw_output = Some(serde_json::json!({
                "approval_state": if *approved { "approved" } else { "rejected" },
                "reason": reason,
            }));
            if !approved {
                fields.content = Some(vec![ToolCallContent::from(ContentBlock::Text(
                    TextContent::new(
                        reason
                            .as_deref()
                            .map(|value| format!("审批被拒绝：{value}"))
                            .unwrap_or_else(|| "审批被拒绝".to_string()),
                    ),
                ))]);
            }

            let mut update = ToolCallUpdate::new(ToolCallId::new(tool_call_id.clone()), fields);
            update.meta = Some(meta);

            vec![
                SessionNotification::new(session_id.clone(), SessionUpdate::ToolCallUpdate(update)),
                make_event_notification(
                    session_id,
                    source,
                    turn_id,
                    *entry_index,
                    "approval_resolved",
                    if *approved { "info" } else { "warning" },
                    if *approved {
                        format!("工具 `{tool_name}` 已获批准并继续执行")
                    } else {
                        format!("工具 `{tool_name}` 已被拒绝执行")
                    },
                    serde_json::json!({
                        "tool_call_id": tool_call_id,
                        "tool_name": tool_name,
                        "approved": approved,
                        "reason": reason,
                        "args": args,
                    }),
                ),
            ]
        }

        AgentEvent::ToolExecutionEnd {
            tool_call_id,
            result,
            is_error,
            ..
        } => {
            let meta = make_meta(source, turn_id, *entry_index);
            *entry_index += 1;

            let result_text = result
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let status = if *is_error {
                ToolCallStatus::Failed
            } else {
                ToolCallStatus::Completed
            };

            let mut fields = ToolCallUpdateFields::default();
            fields.status = Some(status);
            fields.raw_output = Some(result.clone());
            if let Some(decoded) = decode_tool_result(result) {
                let content = content_parts_to_tool_call_content(&decoded.content);
                if !content.is_empty() {
                    fields.content = Some(content);
                }
            } else if !result_text.is_empty() {
                fields.content = Some(vec![ToolCallContent::from(ContentBlock::Text(
                    TextContent::new(&result_text),
                ))]);
            }

            let mut update = ToolCallUpdate::new(ToolCallId::new(tool_call_id.clone()), fields);
            update.meta = Some(meta);

            vec![SessionNotification::new(
                session_id.clone(),
                SessionUpdate::ToolCallUpdate(update),
            )]
        }

        _ => Vec::new(),
    }
}

fn decode_tool_result(value: &serde_json::Value) -> Option<AgentToolResult> {
    serde_json::from_value(value.clone()).ok()
}

fn content_parts_to_tool_call_content(parts: &[ContentPart]) -> Vec<ToolCallContent> {
    parts
        .iter()
        .filter_map(content_part_to_block)
        .map(ToolCallContent::from)
        .collect()
}

fn content_part_to_block(part: &ContentPart) -> Option<ContentBlock> {
    match part {
        ContentPart::Text { text } => Some(ContentBlock::Text(TextContent::new(text))),
        ContentPart::Image { mime_type, data } => {
            Some(ContentBlock::Image(ImageContent::new(data, mime_type)))
        }
        ContentPart::Reasoning { text, .. } => Some(ContentBlock::Text(TextContent::new(text))),
    }
}

struct NoopBridge;

#[async_trait::async_trait]
impl LlmBridge for NoopBridge {
    async fn stream_complete(
        &self,
        _request: agentdash_agent::BridgeRequest,
    ) -> std::pin::Pin<Box<dyn futures::Stream<Item = agentdash_agent::StreamChunk> + Send>> {
        Box::pin(tokio_stream::empty())
    }
}

// ─── Factory ────────────────────────────────────────────────────────

/// 从 `SettingsRepository` 和环境变量构建 `PiAgentConnector`。
///
/// 配置优先级：Settings DB > 环境变量 > 默认值。
/// 支持多 provider：Anthropic、Gemini、DeepSeek、Groq、xAI、OpenAI/兼容端点。
/// 按注册顺序，首个完成注册的 provider 的首个模型作为默认 bridge。
pub async fn build_pi_agent_connector(
    workspace_root: &Path,
    settings: &dyn agentdash_domain::settings::SettingsRepository,
) -> Option<PiAgentConnector> {
    let system_prompt = read_setting_str(settings, "agent.pi.system_prompt")
        .await
        .or_else(|| std::env::var("PI_AGENT_SYSTEM_PROMPT").ok())
        .unwrap_or_else(|| {
            "你是 AgentDash 内置 AI 助手，一个通用的编程与任务执行 Agent。请用中文回复用户。"
                .to_string()
        });

    let providers = build_provider_entries(settings).await;

    let (global_default_bridge, global_default_model) = if let Some(provider) = providers.first() {
        (
            provider.default_bridge.clone(),
            provider.entry.default_model.clone(),
        )
    } else {
        tracing::warn!(
            "PiAgentConnector: 启动时未检测到任何 LLM provider 配置，将以动态占位模式注册"
        );
        (Arc::new(NoopBridge) as Arc<dyn LlmBridge>, String::new())
    };

    let mut connector = PiAgentConnector::new(
        workspace_root.to_path_buf(),
        global_default_bridge,
        system_prompt,
        global_default_model.clone(),
    );

    // 注册所有 provider（含第一个 provider）
    for provider in providers {
        connector.add_provider(provider.entry);
    }

    if connector.providers.is_empty() {
        tracing::info!("PiAgentConnector 已初始化（动态占位模式，等待 settings 注入 provider）");
    } else {
        tracing::info!(
            "PiAgentConnector 已初始化（默认模型：{}，provider 数量：{}）",
            global_default_model,
            connector.providers.len()
        );
    }
    Some(connector)
}

async fn read_setting_str(
    repo: &dyn agentdash_domain::settings::SettingsRepository,
    key: &str,
) -> Option<String> {
    repo.get(&agentdash_domain::settings::SettingScope::system(), key)
        .await
        .ok()
        .flatten()
        .and_then(|s| s.value.as_str().map(String::from))
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_agent::{AssistantStreamEvent, StopReason};
    use agentdash_domain::DomainError;
    use agentdash_domain::settings::{Setting, SettingScope, SettingsRepository};
    use chrono::Utc;
    use std::sync::RwLock;

    fn test_source() -> AgentDashSourceV1 {
        AgentDashSourceV1::new("pi-agent", "local_executor")
    }

    #[derive(Default)]
    struct TestSettingsRepository {
        entries: RwLock<HashMap<(String, String, String), serde_json::Value>>,
    }

    #[async_trait::async_trait]
    impl SettingsRepository for TestSettingsRepository {
        async fn list(
            &self,
            scope: &SettingScope,
            category_prefix: Option<&str>,
        ) -> Result<Vec<Setting>, DomainError> {
            let scope_kind = scope.kind.as_str().to_string();
            let scope_id = scope.storage_scope_id().to_string();
            let entries = self
                .entries
                .read()
                .expect("test settings lock poisoned")
                .iter()
                .filter(|((entry_scope_kind, entry_scope_id, key), _)| {
                    entry_scope_kind == &scope_kind
                        && entry_scope_id == &scope_id
                        && category_prefix.is_none_or(|prefix| key.starts_with(prefix))
                })
                .map(|((_, _, key), value)| Setting {
                    scope_kind: scope.kind,
                    scope_id: scope.scope_id.clone(),
                    key: key.clone(),
                    value: value.clone(),
                    updated_at: Utc::now(),
                })
                .collect::<Vec<_>>();
            Ok(entries)
        }

        async fn get(
            &self,
            scope: &SettingScope,
            key: &str,
        ) -> Result<Option<Setting>, DomainError> {
            let value = self
                .entries
                .read()
                .expect("test settings lock poisoned")
                .get(&(
                    scope.kind.as_str().to_string(),
                    scope.storage_scope_id().to_string(),
                    key.to_string(),
                ))
                .cloned();
            Ok(value.map(|value| Setting {
                scope_kind: scope.kind,
                scope_id: scope.scope_id.clone(),
                key: key.to_string(),
                value,
                updated_at: Utc::now(),
            }))
        }

        async fn set(
            &self,
            scope: &SettingScope,
            key: &str,
            value: serde_json::Value,
        ) -> Result<(), DomainError> {
            self.entries
                .write()
                .expect("test settings lock poisoned")
                .insert(
                    (
                        scope.kind.as_str().to_string(),
                        scope.storage_scope_id().to_string(),
                        key.to_string(),
                    ),
                    value,
                );
            Ok(())
        }

        async fn set_batch(
            &self,
            scope: &SettingScope,
            entries: &[(String, serde_json::Value)],
        ) -> Result<(), DomainError> {
            for (key, value) in entries {
                self.set(scope, key, value.clone()).await?;
            }
            Ok(())
        }

        async fn delete(&self, scope: &SettingScope, key: &str) -> Result<bool, DomainError> {
            let removed = self
                .entries
                .write()
                .expect("test settings lock poisoned")
                .remove(&(
                    scope.kind.as_str().to_string(),
                    scope.storage_scope_id().to_string(),
                    key.to_string(),
                ))
                .is_some();
            Ok(removed)
        }
    }

    async fn discover_options_state(connector: &PiAgentConnector) -> serde_json::Value {
        let patches = connector
            .discover_options_stream("PI_AGENT", None, None)
            .await
            .expect("discover should succeed")
            .collect::<Vec<_>>()
            .await;
        let mut state = serde_json::json!({
            "options": {
                "model_selector": {
                    "providers": [],
                    "models": [],
                    "default_model": null,
                    "agents": [],
                    "permissions": [],
                },
                "slash_commands": [],
                "loading_models": true,
                "loading_agents": true,
                "loading_slash_commands": true,
                "error": null,
            },
            "commands": [],
            "discovering": false,
            "error": null,
        });
        for patch in patches {
            json_patch::patch(&mut state, &patch).expect("patch should apply");
        }
        state
    }

    #[test]
    fn thinking_delta_maps_to_agent_thought_chunk() {
        let event = AgentEvent::MessageUpdate {
            message: AgentMessage::Assistant {
                content: vec![ContentPart::reasoning("plan", None, None)],
                tool_calls: vec![],
                stop_reason: Some(StopReason::Stop),
                error_message: None,
                usage: None,
                timestamp: Some(agentdash_agent::types::now_millis()),
            },
            event: AssistantStreamEvent::ThinkingDelta {
                content_index: 0,
                id: None,
                text: "plan".to_string(),
            },
        };

        let mut entry_index = 0;
        let notifications = convert_event_to_notifications(
            &event,
            &SessionId::new("session-1"),
            &test_source(),
            "turn-1",
            &mut entry_index,
        );

        assert_eq!(notifications.len(), 1);
        match &notifications[0].update {
            SessionUpdate::AgentThoughtChunk(chunk) => match &chunk.content {
                ContentBlock::Text(text) => assert_eq!(text.text, "plan"),
                other => panic!("unexpected content block: {other:?}"),
            },
            other => panic!("unexpected session update: {other:?}"),
        }
    }

    #[test]
    fn tool_execution_updates_preserve_full_tool_result_payload() {
        let result = AgentToolResult {
            content: vec![ContentPart::text("done")],
            is_error: false,
            details: Some(serde_json::json!({ "ok": true })),
        };
        let raw_result = serde_json::to_value(&result).expect("tool result should serialize");

        let update_event = AgentEvent::ToolExecutionUpdate {
            tool_call_id: "tool-1".to_string(),
            tool_name: "echo".to_string(),
            args: serde_json::json!({ "value": "x" }),
            partial_result: raw_result.clone(),
        };
        let end_event = AgentEvent::ToolExecutionEnd {
            tool_call_id: "tool-1".to_string(),
            tool_name: "echo".to_string(),
            result: raw_result.clone(),
            is_error: false,
        };

        let mut entry_index = 0;
        let update_notifications = convert_event_to_notifications(
            &update_event,
            &SessionId::new("session-1"),
            &test_source(),
            "turn-1",
            &mut entry_index,
        );
        let end_notifications = convert_event_to_notifications(
            &end_event,
            &SessionId::new("session-1"),
            &test_source(),
            "turn-1",
            &mut entry_index,
        );

        match &update_notifications[0].update {
            SessionUpdate::ToolCallUpdate(update) => {
                assert_eq!(update.fields.status, Some(ToolCallStatus::InProgress));
                assert_eq!(update.fields.raw_output, Some(raw_result.clone()));
            }
            other => panic!("unexpected session update: {other:?}"),
        }

        match &end_notifications[0].update {
            SessionUpdate::ToolCallUpdate(update) => {
                assert_eq!(update.fields.status, Some(ToolCallStatus::Completed));
                assert_eq!(update.fields.raw_output, Some(raw_result));
                let content = update.fields.content.clone().expect("content should exist");
                assert_eq!(content.len(), 1);
            }
            other => panic!("unexpected session update: {other:?}"),
        }
    }

    #[test]
    fn pending_approval_event_maps_to_tool_call_update() {
        let event = AgentEvent::ToolExecutionPendingApproval {
            tool_call_id: "tool-approval-1".to_string(),
            tool_name: "shell_exec".to_string(),
            args: serde_json::json!({ "command": "cargo test", "cwd": "." }),
            reason: "需要用户审批".to_string(),
            details: Some(serde_json::json!({ "policy": "supervised_tool_approval" })),
        };

        let mut entry_index = 0;
        let notifications = convert_event_to_notifications(
            &event,
            &SessionId::new("session-1"),
            &test_source(),
            "turn-1",
            &mut entry_index,
        );

        assert_eq!(notifications.len(), 2);
        match &notifications[0].update {
            SessionUpdate::ToolCallUpdate(update) => {
                assert_eq!(update.fields.status, Some(ToolCallStatus::Pending));
                assert_eq!(
                    update
                        .fields
                        .raw_output
                        .as_ref()
                        .and_then(|value| value.get("approval_state"))
                        .and_then(serde_json::Value::as_str),
                    Some("pending")
                );
            }
            other => panic!("unexpected session update: {other:?}"),
        }

        match &notifications[1].update {
            SessionUpdate::SessionInfoUpdate(info) => {
                let value = serde_json::to_value(info).expect("serialize session info");
                assert_eq!(
                    value
                        .get("_meta")
                        .and_then(|item| item.get("agentdash"))
                        .and_then(|item| item.get("event"))
                        .and_then(|item| item.get("type"))
                        .and_then(serde_json::Value::as_str),
                    Some("approval_requested")
                );
            }
            other => panic!("unexpected session update: {other:?}"),
        }
    }

    #[test]
    fn assistant_message_end_with_error_message_emits_fallback_chunk() {
        let event = AgentEvent::MessageEnd {
            message: AgentMessage::Assistant {
                content: vec![ContentPart::text("")],
                tool_calls: vec![],
                stop_reason: Some(StopReason::Aborted),
                error_message: Some("Agent run aborted".to_string()),
                usage: None,
                timestamp: Some(agentdash_agent::types::now_millis()),
            },
        };

        let mut entry_index = 0;
        let notifications = convert_event_to_notifications(
            &event,
            &SessionId::new("session-1"),
            &test_source(),
            "turn-1",
            &mut entry_index,
        );

        assert_eq!(notifications.len(), 1);
        assert_eq!(entry_index, 1);
        match &notifications[0].update {
            SessionUpdate::AgentMessageChunk(chunk) => match &chunk.content {
                ContentBlock::Text(text) => assert_eq!(text.text, "Agent run aborted"),
                other => panic!("unexpected content block: {other:?}"),
            },
            other => panic!("unexpected session update: {other:?}"),
        }
    }

    #[test]
    fn runtime_system_prompt_prefers_relative_workspace_paths() {
        let connector = PiAgentConnector::new(
            PathBuf::from("F:/Projects/AgentDash"),
            Arc::new(NoopBridge),
            "系统提示",
            "gpt-5.4",
        );
        let context = ExecutionContext {
            turn_id: "turn-1".to_string(),
            workspace_root: PathBuf::from("F:/Projects/AgentDash"),
            working_directory: PathBuf::from("F:/Projects/AgentDash/crates/agentdash-agent"),
            environment_variables: HashMap::new(),
            executor_config: crate::connector::ExecutorConfig::new("PI_AGENT"),
            mcp_servers: vec![],
            address_space: None,
            hook_session: None,
            flow_capabilities: Default::default(),
            system_context: None,
        };

        let prompt = connector.build_runtime_system_prompt(&context, &["shell".to_string()]);
        assert!(prompt.contains("工作空间路径锚点：."));
        assert!(prompt.contains("当前工作目录（相对工作空间）：crates/agentdash-agent"));
        assert!(prompt.contains("不要把 `F:\\...` 这类绝对路径直接写进工具参数"));
        assert!(prompt.contains("cwd 设为 `.`"));
        assert!(!prompt.contains(
            "当前工作目录（相对工作空间）：F:/Projects/AgentDash/crates/agentdash-agent"
        ));
    }

    #[tokio::test]
    async fn discovery_reflects_settings_added_after_startup_without_restart() {
        let repo = Arc::new(TestSettingsRepository::default());
        let mut connector =
            build_pi_agent_connector(Path::new("F:/Projects/AgentDash"), repo.as_ref())
                .await
                .expect("connector should initialize even without provider");
        connector.set_settings_repository(repo.clone());

        let initial = discover_options_state(&connector).await;
        assert_eq!(
            initial["options"]["model_selector"]["providers"],
            serde_json::json!([])
        );
        assert_eq!(
            initial["options"]["model_selector"]["default_model"],
            serde_json::Value::Null
        );

        repo.set(
            &SettingScope::system(),
            "llm.anthropic.api_key",
            serde_json::json!("test-key"),
        )
        .await
        .expect("should store anthropic key");

        let refreshed = discover_options_state(&connector).await;
        assert_eq!(
            refreshed["options"]["model_selector"]["providers"],
            serde_json::json!([{ "id": "anthropic", "name": "Anthropic Claude" }])
        );
        assert_eq!(
            refreshed["options"]["model_selector"]["default_model"],
            serde_json::json!(rig::providers::anthropic::completion::CLAUDE_4_SONNET)
        );
    }

    #[tokio::test]
    async fn discovery_does_not_fall_back_to_startup_provider_after_settings_cleared() {
        let repo = Arc::new(TestSettingsRepository::default());
        repo.set(
            &SettingScope::system(),
            "llm.anthropic.api_key",
            serde_json::json!("test-key"),
        )
        .await
        .expect("should store anthropic key");

        let mut connector =
            build_pi_agent_connector(Path::new("F:/Projects/AgentDash"), repo.as_ref())
                .await
                .expect("connector should initialize");
        connector.set_settings_repository(repo.clone());

        let initial = discover_options_state(&connector).await;
        assert_eq!(
            initial["options"]["model_selector"]["providers"],
            serde_json::json!([{ "id": "anthropic", "name": "Anthropic Claude" }])
        );

        repo.delete(&SettingScope::system(), "llm.anthropic.api_key")
            .await
            .expect("should delete anthropic key");

        let refreshed = discover_options_state(&connector).await;
        assert_eq!(
            refreshed["options"]["model_selector"]["providers"],
            serde_json::json!([])
        );
        assert_eq!(
            refreshed["options"]["model_selector"]["models"],
            serde_json::json!([])
        );
        assert_eq!(
            refreshed["options"]["model_selector"]["default_model"],
            serde_json::Value::Null
        );
    }

    #[tokio::test]
    async fn prompt_without_provider_configuration_returns_clear_error() {
        let repo = Arc::new(TestSettingsRepository::default());
        let mut connector =
            build_pi_agent_connector(Path::new("F:/Projects/AgentDash"), repo.as_ref())
                .await
                .expect("connector should initialize even without provider");
        connector.set_settings_repository(repo);

        let result = connector
            .prompt(
                "session-1",
                None,
                &PromptPayload::Text("hello".to_string()),
                ExecutionContext {
                    turn_id: "turn-1".to_string(),
                    workspace_root: PathBuf::from("F:/Projects/AgentDash"),
                    working_directory: PathBuf::from("F:/Projects/AgentDash"),
                    environment_variables: HashMap::new(),
                    executor_config: crate::connector::ExecutorConfig::new("PI_AGENT"),
                    mcp_servers: vec![],
                    address_space: None,
                    hook_session: None,
                    flow_capabilities: Default::default(),
                    system_context: None,
                },
            )
            .await;

        match result {
            Err(ConnectorError::InvalidConfig(message)) => {
                assert!(message.contains("尚未配置任何可用的 LLM Provider"));
            }
            Ok(_) => panic!("prompt should fail without configured provider"),
            Err(other) => panic!("unexpected connector error: {other}"),
        }
    }
}
