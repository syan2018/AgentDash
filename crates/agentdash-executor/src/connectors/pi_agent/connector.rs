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
use agentdash_domain::llm_provider::LlmProviderRepository;
use agentdash_domain::settings::SettingsRepository;

use crate::connectors::pi_agent::pi_agent_mcp::discover_mcp_tools;
use crate::connectors::pi_agent::pi_agent_provider_registry::{
    CONTEXT_WINDOW_STANDARD, ProviderEntry, build_provider_entries_from_db,
};
use crate::hook_events::build_hook_trace_notification;
use agentdash_spi::connector::RuntimeToolProvider;
use agentdash_spi::{
    AgentConnector, AgentInfo, ConnectorCapabilities, ConnectorError, ConnectorType,
    ExecutionContext, ExecutionStream, Mount, MountCapability, PromptPayload, SystemPromptMode,
};

// ─── PiAgentConnector ───────────────────────────────────────────

pub struct PiAgentConnector {
    #[allow(dead_code)]
    workspace_root: PathBuf,
    /// 默认 bridge（向后兼容，无 provider 注册时使用）
    bridge: Arc<dyn LlmBridge>,
    /// 已注册的 provider 列表（按注册顺序，首个命中的 provider 优先）
    providers: Vec<ProviderEntry>,
    runtime_tool_provider: Option<Arc<dyn RuntimeToolProvider>>,
    settings_repo: Option<Arc<dyn SettingsRepository>>,
    llm_provider_repo: Option<Arc<dyn LlmProviderRepository>>,
    system_prompt: String,
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
    ) -> Self {
        Self {
            workspace_root,
            bridge,
            providers: Vec::new(),
            runtime_tool_provider: None,
            settings_repo: None,
            llm_provider_repo: None,
            system_prompt: system_prompt.into(),
            agents: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn set_runtime_tool_provider(&mut self, provider: Arc<dyn RuntimeToolProvider>) {
        self.runtime_tool_provider = Some(provider);
    }

    pub fn set_settings_repository(&mut self, settings_repo: Arc<dyn SettingsRepository>) {
        self.settings_repo = Some(settings_repo);
    }

    pub fn set_llm_provider_repository(&mut self, repo: Arc<dyn LlmProviderRepository>) {
        self.llm_provider_repo = Some(repo);
    }

    fn add_provider(&mut self, provider: ProviderEntry) {
        self.providers.push(provider);
    }

    async fn load_provider_runtime_state(&self) -> ProviderRuntimeState {
        if let Some(llm_provider_repo) = &self.llm_provider_repo {
            let providers = build_provider_entries_from_db(llm_provider_repo.as_ref()).await;
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

        // 直接通过 `PiAgentConnector::new(...)` 构造且未挂载动态 provider repo 的场景，
        // 允许回退到构造时注入的静态 bridge，便于测试和嵌入式用法。
        if self.settings_repo.is_none() && self.llm_provider_repo.is_none() {
            let default_model = self
                .providers
                .first()
                .map(|provider| provider.default_model.clone())
                .or_else(|| Some("static-default".to_string()));
            return ProviderRuntimeState {
                default_bridge: Some(self.bridge.clone()),
                default_model,
                providers: self.providers.clone(),
            };
        }

        ProviderRuntimeState {
            default_bridge: None,
            default_model: None,
            providers: Vec::new(),
        }
    }

    fn create_agent_with_bridge(&self, bridge: Arc<dyn LlmBridge>) -> Agent {
        let config = AgentConfig {
            system_prompt: self.system_prompt.clone(),
            ..AgentConfig::default()
        };
        Agent::new(bridge, config)
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

    /// 组装运行时 system prompt。
    ///
    /// 采用统一 Markdown section 格式，每段以 `## 标题` 标注来源，
    /// 最终以 `\n\n` 拼接。顺序即优先级：人设 → 上下文 → 环境 → 工具 → 扩展。
    fn build_runtime_system_prompt(
        &self,
        context: &ExecutionContext,
        tool_names: &[String],
    ) -> String {
        let mut sections: Vec<String> = Vec::new();

        // ── 1. Identity: 基础人设 ──
        let agent_sp = context
            .executor_config
            .system_prompt
            .as_deref()
            .filter(|s| !s.trim().is_empty());
        match (context.executor_config.system_prompt_mode, agent_sp) {
            (Some(SystemPromptMode::Override), Some(sp)) => {
                sections.push(format!("## Identity\n\n{sp}"));
            }
            (_, Some(sp)) => {
                sections.push(format!("## Identity\n\n{}\n\n{sp}", self.system_prompt));
            }
            _ => {
                sections.push(format!("## Identity\n\n{}", self.system_prompt));
            }
        }

        // ── 2. Project Context: 会话级 owner 上下文 ──
        if let Some(ref ctx) = context.system_context
            && !ctx.trim().is_empty()
        {
            sections.push(format!("## Project Context\n\n{ctx}"));
        }

        // ── 3. Workspace: 地址空间 / 工作目录 ──
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
                "## Workspace\n\n当前会话可访问的 Address Space 挂载如下：\n\n{mount_lines}\n\n默认 mount：`{default_mount}`"
            ));
        } else {
            let current_dir_display =
                workspace_relative_display(&context.workspace_root, &context.working_directory);
            sections.push(format!(
                "## Workspace\n\n- 工作空间路径锚点：`.`\n- 工作空间绝对路径（仅供参考，不要直接写入工具参数）：`{}`\n- 当前工作目录（相对工作空间）：`{current_dir_display}`",
                context.workspace_root.display(),
            ));
        }

        // ── 4. Tools: 可用工具及使用规范 ──
        if !tool_names.is_empty() {
            let mut tool_section = String::from("## Tools\n\n");
            if context.address_space.is_some() {
                tool_section.push_str(&format!(
                    "Available address-space tools: {}. Prefer mounts_list / fs_read / fs_glob / fs_grep / fs_apply_patch / shell_exec to inspect and edit files.\n\n",
                    tool_names.join(", ")
                ));
                tool_section.push_str(
                    "**Path convention**: paths MUST use `mount_id://relative/path` format (e.g., `main://src/lib.rs`). \
The mount prefix may be omitted when the session has exactly one mount. \
Never put backend_id or absolute paths into tool arguments. \
For shell_exec, `cwd` must also be relative to the mount root; use `main://.` for the current directory.\n\n",
                );
                tool_section.push_str(
                    "**fs_apply_patch format**: uses Codex apply_patch syntax (**not** unified diff). \
Starts with `*** Begin Patch`, ends with `*** End Patch`. \
Each file operation MUST begin with `*** Add File: path` / `*** Update File: path` / `*** Delete File: path`. \
For renaming, follow `Update File` with `*** Move to: new/path`. \
Each hunk starts with `@@` (optionally followed by a context-anchor line); \
lines within a hunk are prefixed with space (context) / `-` (remove) / `+` (add). \
Paths may use `mount_id://path` to target a specific mount; paths without a prefix use the default mount.",
                );
            } else {
                tool_section.push_str(&format!(
                    "可调用的内置工具：{}。优先使用工具读取/搜索/执行，不要臆测文件内容。\n\n",
                    tool_names.join("、")
                ));
                tool_section.push_str(&format!(
                    "**路径规范**：调用 read_file、list_directory、search、write_file、shell 等工作空间工具时，路径参数必须优先使用相对工作空间根目录的路径。如果要在当前目录执行 shell，请将 cwd 设为 `.`；如果要进入子目录，请传类似 `crates/agentdash-agent` 这样的相对路径；不要把 `{}/...` 这类绝对路径直接写进工具参数。",
                    context.workspace_root.display()
                ));
            }
            sections.push(tool_section);
        }

        // ── 5. MCP Servers ──
        if !context.mcp_servers.is_empty() {
            let server_lines = context
                .mcp_servers
                .iter()
                .map(describe_mcp_server)
                .collect::<Vec<_>>()
                .join("\n");
            sections.push(format!(
                "## MCP Servers\n\n以下 MCP Server 已注入当前会话，可在需要时使用：\n\n{server_lines}"
            ));
        }

        // ── 6. Hooks ──
        if let Some(hook_session) = &context.hook_session {
            let hook_parts = build_hook_runtime_sections(hook_session.as_ref());
            if !hook_parts.is_empty() {
                sections.push(format!("## Hooks\n\n{}", hook_parts.join("\n\n")));
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

fn describe_mount(mount: &Mount) -> String {
    let capabilities = mount
        .capabilities
        .iter()
        .map(|capability| match capability {
            MountCapability::Read => "read",
            MountCapability::Write => "write",
            MountCapability::List => "list",
            MountCapability::Search => "search",
            MountCapability::Exec => "exec",
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "- {}: {}（provider={}, root_ref={}, capabilities=[{}]）",
        mount.id, mount.display_name, mount.provider, mount.root_ref, capabilities
    )
}

fn build_hook_runtime_sections(
    hook_session: &dyn agentdash_spi::hooks::HookSessionRuntimeAccess,
) -> Vec<String> {
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

    fn supports_repository_restore(&self, executor: &str) -> bool {
        executor.trim() == "PI_AGENT"
    }

    fn list_executors(&self) -> Vec<AgentInfo> {
        vec![AgentInfo {
            id: "PI_AGENT".to_string(),
            name: "Pi Agent".to_string(),
            variants: vec![],
            available: true,
        }]
    }

    async fn discover_options_stream(
        &self,
        _executor: &str,
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

    async fn has_live_session(&self, session_id: &str) -> bool {
        self.agents.lock().await.contains_key(session_id)
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
        let restored_messages = context
            .restored_session_state
            .as_ref()
            .map(|state| state.messages.clone());

        let existing_agent = {
            let mut agents = self.agents.lock().await;
            agents.remove(session_id)
        };

        let is_new_agent = existing_agent.is_none();
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

        // 只有新创建的 agent 才需要 build tools 和 system prompt。
        // 已存在的 agent（后续 turn）复用上次的 tools 和 system prompt，
        // 只更新 runtime delegate（hook session 每轮刷新）。
        if is_new_agent {
            let mcp_tools = match discover_mcp_tools(&context.mcp_servers).await {
                Ok(tools) => tools,
                Err(error) => {
                    tracing::warn!("发现 MCP 工具失败，继续使用本地工具: {error}");
                    Vec::new()
                }
            };
            let mut runtime_tools: Vec<DynAgentTool> = Vec::new();
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
            if let Some(messages) = restored_messages.filter(|messages| !messages.is_empty()) {
                agent.replace_messages(messages).await;
            }
        }
        let hook_trace_rx = context
            .hook_session
            .as_ref()
            .and_then(|hs| hs.subscribe_traces());
        agent.set_runtime_delegate(context.runtime_delegate.clone());

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
            tokio::sync::mpsc::channel::<Result<SessionNotification, ConnectorError>>(8192);

        tokio::spawn(async move {
            let mut entry_index: u32 = 0;
            let mut chunk_message_ids: HashMap<String, String> = HashMap::new();
            let mut chunk_emit_states: HashMap<String, ChunkEmitState> = HashMap::new();
            let mut tool_call_states: HashMap<String, ToolCallEmitState> = HashMap::new();
            let mut event_rx = event_rx;
            let mut hook_trace_rx = hook_trace_rx;

            loop {
                if let Some(receiver) = hook_trace_rx.as_mut() {
                    tokio::select! {
                        biased;
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
                                &mut chunk_message_ids,
                                &mut chunk_emit_states,
                                &mut tool_call_states,
                            );

                            for n in notifications {
                                if tx.send(Ok(n)).await.is_err() {
                                    return;
                                }
                            }
                        }
                        trace_result = receiver.recv() => {
                            if let Ok(entry) = trace_result
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
                    &mut chunk_message_ids,
                    &mut chunk_emit_states,
                    &mut tool_call_states,
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
    hook_trace_rx: &mut Option<tokio::sync::broadcast::Receiver<agentdash_spi::HookTraceEntry>>,
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
        ) && tx.send(Ok(notification)).await.is_err()
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
    event: Option<AgentDashEventV1>,
) -> agent_client_protocol::Meta {
    let mut trace = AgentDashTraceV1::new();
    trace.turn_id = Some(turn_id.to_string());
    trace.entry_index = Some(entry_index);

    let agentdash = AgentDashMetaV1::new()
        .source(Some(source.clone()))
        .trace(Some(trace))
        .event(event);

    merge_agentdash_meta(None, &agentdash).expect("agentdash meta 不应为空")
}

fn make_tool_call_draft_event(
    tool_call_id: &str,
    tool_name: &str,
    phase: &'static str,
    delta: Option<&str>,
    draft_input: &str,
    is_parseable: bool,
) -> AgentDashEventV1 {
    let mut event = AgentDashEventV1::new("tool_call_draft");
    event.message = Some(format!("工具 `{tool_name}` 参数草稿更新"));
    event.data = Some(serde_json::json!({
        "toolCallId": tool_call_id,
        "toolName": tool_name,
        "phase": phase,
        "delta": delta,
        "draftInput": draft_input,
        "isParseable": is_parseable,
    }));
    event
}

struct EventDescription {
    event_type: &'static str,
    severity: &'static str,
    message: String,
    data: serde_json::Value,
}

fn make_event_notification(
    session_id: &SessionId,
    source: &AgentDashSourceV1,
    turn_id: &str,
    entry_index: u32,
    desc: EventDescription,
) -> SessionNotification {
    let mut trace = AgentDashTraceV1::new();
    trace.turn_id = Some(turn_id.to_string());
    trace.entry_index = Some(entry_index);

    let mut event = AgentDashEventV1::new(desc.event_type);
    event.severity = Some(desc.severity.to_string());
    event.message = Some(desc.message);
    event.data = Some(desc.data);

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

fn ensure_chunk_message_id(
    cache: &mut HashMap<String, String>,
    turn_id: &str,
    entry_index: u32,
    chunk_kind: &str,
) -> String {
    let key = format!("{turn_id}:{entry_index}:{chunk_kind}");
    if let Some(existing) = cache.get(&key) {
        return existing.clone();
    }
    let generated = uuid::Uuid::new_v4().to_string();
    cache.insert(key, generated.clone());
    generated
}

#[derive(Debug, Default, Clone)]
struct ChunkEmitState {
    emitted_text: String,
    seen_delta: bool,
}

#[derive(Debug, Clone)]
struct ToolCallEmitState {
    entry_index: u32,
    title: String,
    kind: ToolKind,
    raw_input: Option<serde_json::Value>,
}

fn chunk_stream_key(turn_id: &str, entry_index: u32, chunk_kind: &str) -> String {
    format!("{turn_id}:{entry_index}:{chunk_kind}")
}

fn map_tool_kind(tool_name: &str) -> ToolKind {
    match tool_name {
        "read_file" | "fs_read" | "list_directory" | "fs_list" | "fs_glob" | "canvases_list" => {
            ToolKind::Read
        }
        "write_file" | "fs_write" | "fs_apply_patch" | "canvas_start" | "bind_canvas_data" => {
            ToolKind::Edit
        }
        "search" | "fs_search" | "fs_grep" => ToolKind::Search,
        "shell" | "shell_exec" => ToolKind::Execute,
        "fetch" | "web_fetch" => ToolKind::Fetch,
        "think" => ToolKind::Think,
        "switch_mode" => ToolKind::SwitchMode,
        _ => ToolKind::Other,
    }
}

fn message_tool_call_info<'a>(
    message: &'a AgentMessage,
    tool_call_id: &str,
) -> Option<&'a agentdash_agent::ToolCallInfo> {
    match message {
        AgentMessage::Assistant { tool_calls, .. } => tool_calls
            .iter()
            .find(|tool_call| tool_call.id == tool_call_id),
        _ => None,
    }
}

fn convert_event_to_notifications(
    event: &AgentEvent,
    session_id: &SessionId,
    source: &AgentDashSourceV1,
    turn_id: &str,
    entry_index: &mut u32,
    chunk_message_ids: &mut HashMap<String, String>,
    chunk_emit_states: &mut HashMap<String, ChunkEmitState>,
    tool_call_states: &mut HashMap<String, ToolCallEmitState>,
) -> Vec<SessionNotification> {
    fn upsert_tool_call_state(
        tool_call_states: &mut HashMap<String, ToolCallEmitState>,
        entry_index: &mut u32,
        tool_call_id: &str,
        title: String,
        kind: ToolKind,
        raw_input: Option<serde_json::Value>,
    ) -> (ToolCallEmitState, bool) {
        if let Some(existing) = tool_call_states.get_mut(tool_call_id) {
            if !title.trim().is_empty() {
                existing.title = title;
            }
            if existing.kind == ToolKind::Other && kind != ToolKind::Other {
                existing.kind = kind;
            }
            if let Some(raw_input) = raw_input {
                existing.raw_input = Some(raw_input);
            }
            return (existing.clone(), false);
        }

        let state = ToolCallEmitState {
            entry_index: *entry_index,
            title,
            kind,
            raw_input,
        };
        *entry_index += 1;
        tool_call_states.insert(tool_call_id.to_string(), state.clone());
        (state, true)
    }

    fn build_tool_call_notification(
        session_id: &SessionId,
        source: &AgentDashSourceV1,
        turn_id: &str,
        tool_call_id: &str,
        state: &ToolCallEmitState,
        status: ToolCallStatus,
    ) -> SessionNotification {
        let meta = make_meta(source, turn_id, state.entry_index, None);
        let mut call = ToolCall::new(ToolCallId::new(tool_call_id.to_string()), &state.title)
            .kind(state.kind)
            .status(status)
            .raw_input(state.raw_input.clone());
        call.meta = Some(meta);
        SessionNotification::new(session_id.clone(), SessionUpdate::ToolCall(call))
    }

    fn build_tool_call_update_notification(
        session_id: &SessionId,
        source: &AgentDashSourceV1,
        turn_id: &str,
        tool_call_id: &str,
        state: &ToolCallEmitState,
        fields: ToolCallUpdateFields,
        event: Option<AgentDashEventV1>,
    ) -> SessionNotification {
        let mut update = ToolCallUpdate::new(ToolCallId::new(tool_call_id.to_string()), fields);
        update.meta = Some(make_meta(source, turn_id, state.entry_index, event));
        SessionNotification::new(session_id.clone(), SessionUpdate::ToolCallUpdate(update))
    }

    fn seed_tool_update_fields(
        state: &ToolCallEmitState,
        status: Option<ToolCallStatus>,
    ) -> ToolCallUpdateFields {
        let mut fields = ToolCallUpdateFields::default();
        fields.title = Some(state.title.clone());
        fields.kind = Some(state.kind);
        fields.status = status;
        if let Some(raw_input) = state.raw_input.clone() {
            fields.raw_input = Some(raw_input);
        }
        fields
    }

    fn upsert_state_from_tool_name(
        tool_call_states: &mut HashMap<String, ToolCallEmitState>,
        entry_index: &mut u32,
        tool_call_id: &str,
        tool_name: &str,
        raw_input: Option<serde_json::Value>,
    ) -> (ToolCallEmitState, bool) {
        upsert_tool_call_state(
            tool_call_states,
            entry_index,
            tool_call_id,
            tool_name.to_string(),
            map_tool_kind(tool_name),
            raw_input,
        )
    }

    fn upsert_state_from_message(
        tool_call_states: &mut HashMap<String, ToolCallEmitState>,
        entry_index: &mut u32,
        message: &AgentMessage,
        tool_call_id: &str,
        fallback_name: &str,
    ) -> (ToolCallEmitState, bool) {
        if let Some(tool_call) = message_tool_call_info(message, tool_call_id) {
            return upsert_tool_call_state(
                tool_call_states,
                entry_index,
                tool_call_id,
                tool_call.name.clone(),
                map_tool_kind(&tool_call.name),
                Some(tool_call.arguments.clone()),
            );
        }

        upsert_state_from_tool_name(
            tool_call_states,
            entry_index,
            tool_call_id,
            fallback_name,
            None,
        )
    }

    match event {
        AgentEvent::MessageUpdate { message, event } => match event {
            agentdash_agent::types::AssistantStreamEvent::ToolCallStart {
                tool_call_id,
                name,
                ..
            } => {
                let (state, created) = upsert_state_from_message(
                    tool_call_states,
                    entry_index,
                    message,
                    tool_call_id,
                    name,
                );
                if !created {
                    return Vec::new();
                }
                vec![build_tool_call_notification(
                    session_id,
                    source,
                    turn_id,
                    tool_call_id,
                    &state,
                    ToolCallStatus::Pending,
                )]
            }
            agentdash_agent::types::AssistantStreamEvent::ToolCallDelta {
                tool_call_id,
                name,
                delta,
                draft,
                is_parseable,
                ..
            } => {
                let (state, _) = upsert_state_from_message(
                    tool_call_states,
                    entry_index,
                    message,
                    tool_call_id,
                    name,
                );
                let fields = seed_tool_update_fields(&state, Some(ToolCallStatus::Pending));
                let draft_event = Some(make_tool_call_draft_event(
                    tool_call_id,
                    name,
                    "delta",
                    Some(delta),
                    draft,
                    *is_parseable,
                ));
                vec![build_tool_call_update_notification(
                    session_id,
                    source,
                    turn_id,
                    tool_call_id,
                    &state,
                    fields,
                    draft_event,
                )]
            }
            agentdash_agent::types::AssistantStreamEvent::ToolCallEnd { tool_call, .. } => {
                let (state, _) = upsert_tool_call_state(
                    tool_call_states,
                    entry_index,
                    &tool_call.id,
                    tool_call.name.clone(),
                    map_tool_kind(&tool_call.name),
                    Some(tool_call.arguments.clone()),
                );
                let fields = seed_tool_update_fields(&state, Some(ToolCallStatus::Pending));
                let draft_event = serde_json::to_string(&tool_call.arguments)
                    .ok()
                    .map(|draft| {
                        make_tool_call_draft_event(
                            &tool_call.id,
                            &tool_call.name,
                            "end",
                            None,
                            &draft,
                            true,
                        )
                    });
                vec![build_tool_call_update_notification(
                    session_id,
                    source,
                    turn_id,
                    &tool_call.id,
                    &state,
                    fields,
                    draft_event,
                )]
            }
            agentdash_agent::types::AssistantStreamEvent::TextDelta { text, .. } => {
                if text.is_empty() {
                    return Vec::new();
                }
                let meta = make_meta(source, turn_id, *entry_index, None);
                let key = chunk_stream_key(turn_id, *entry_index, "agent_message_chunk");
                let state = chunk_emit_states.entry(key).or_default();
                state.seen_delta = true;
                state.emitted_text.push_str(text);
                let message_id = ensure_chunk_message_id(
                    chunk_message_ids,
                    turn_id,
                    *entry_index,
                    "agent_message_chunk",
                );
                let chunk = ContentChunk::new(ContentBlock::Text(TextContent::new(text)))
                    .message_id(Some(message_id))
                    .meta(Some(meta));
                vec![SessionNotification::new(
                    session_id.clone(),
                    SessionUpdate::AgentMessageChunk(chunk),
                )]
            }
            agentdash_agent::types::AssistantStreamEvent::ThinkingDelta { text, .. } => {
                if text.is_empty() {
                    return Vec::new();
                }
                let meta = make_meta(source, turn_id, *entry_index, None);
                let key = chunk_stream_key(turn_id, *entry_index, "agent_thought_chunk");
                let state = chunk_emit_states.entry(key).or_default();
                state.seen_delta = true;
                state.emitted_text.push_str(text);
                let message_id = ensure_chunk_message_id(
                    chunk_message_ids,
                    turn_id,
                    *entry_index,
                    "agent_thought_chunk",
                );
                let chunk = ContentChunk::new(ContentBlock::Text(TextContent::new(text)))
                    .message_id(Some(message_id))
                    .meta(Some(meta));
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
                    let key = chunk_stream_key(turn_id, *entry_index, "agent_thought_chunk");
                    let state = chunk_emit_states.get(&key).cloned().unwrap_or_default();
                    let message_id = ensure_chunk_message_id(
                        chunk_message_ids,
                        turn_id,
                        *entry_index,
                        "agent_thought_chunk",
                    );
                    let to_emit = if state.seen_delta {
                        if reasoning_text == state.emitted_text {
                            None
                        } else if reasoning_text.starts_with(state.emitted_text.as_str()) {
                            let suffix = &reasoning_text[state.emitted_text.len()..];
                            if suffix.is_empty() {
                                None
                            } else {
                                Some(suffix.to_string())
                            }
                        } else {
                            // 单路径约束：流式消息已存在增量链路时，不再走 reconcile 兜底快照。
                            tracing::warn!(
                                turn_id = %turn_id,
                                entry_index = *entry_index,
                                "MessageEnd thought 与已发送增量不一致，已忽略兜底快照"
                            );
                            None
                        }
                    } else {
                        Some(reasoning_text.clone())
                    };
                    if let Some(payload) = to_emit {
                        let meta = make_meta(source, turn_id, *entry_index, None);
                        let chunk =
                            ContentChunk::new(ContentBlock::Text(TextContent::new(payload)))
                                .message_id(Some(message_id))
                                .meta(Some(meta));
                        notifications.push(SessionNotification::new(
                            session_id.clone(),
                            SessionUpdate::AgentThoughtChunk(chunk),
                        ));
                    }
                }
                if !text.is_empty() {
                    let key = chunk_stream_key(turn_id, *entry_index, "agent_message_chunk");
                    let state = chunk_emit_states.get(&key).cloned().unwrap_or_default();
                    let message_id = ensure_chunk_message_id(
                        chunk_message_ids,
                        turn_id,
                        *entry_index,
                        "agent_message_chunk",
                    );
                    let to_emit = if state.seen_delta {
                        if text == state.emitted_text {
                            None
                        } else if text.starts_with(state.emitted_text.as_str()) {
                            let suffix = &text[state.emitted_text.len()..];
                            if suffix.is_empty() {
                                None
                            } else {
                                Some(suffix.to_string())
                            }
                        } else {
                            tracing::warn!(
                                turn_id = %turn_id,
                                entry_index = *entry_index,
                                "MessageEnd text 与已发送增量不一致，已忽略兜底快照"
                            );
                            None
                        }
                    } else {
                        Some(text.clone())
                    };
                    if let Some(payload) = to_emit {
                        let meta = make_meta(source, turn_id, *entry_index, None);
                        let chunk =
                            ContentChunk::new(ContentBlock::Text(TextContent::new(payload)))
                                .message_id(Some(message_id))
                                .meta(Some(meta));
                        notifications.push(SessionNotification::new(
                            session_id.clone(),
                            SessionUpdate::AgentMessageChunk(chunk),
                        ));
                    }
                }

                for tool_call in tool_calls {
                    let (state, created) = upsert_tool_call_state(
                        tool_call_states,
                        entry_index,
                        &tool_call.id,
                        tool_call.name.clone(),
                        map_tool_kind(&tool_call.name),
                        Some(tool_call.arguments.clone()),
                    );
                    if created {
                        notifications.push(build_tool_call_notification(
                            session_id,
                            source,
                            turn_id,
                            &tool_call.id,
                            &state,
                            ToolCallStatus::Pending,
                        ));
                    }
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
            let (state, _) = upsert_state_from_tool_name(
                tool_call_states,
                entry_index,
                tool_call_id,
                tool_name,
                Some(args.clone()),
            );
            let fields = seed_tool_update_fields(&state, Some(ToolCallStatus::InProgress));
            vec![build_tool_call_update_notification(
                session_id,
                source,
                turn_id,
                tool_call_id,
                &state,
                fields,
                None,
            )]
        }

        AgentEvent::ToolExecutionUpdate {
            tool_call_id,
            tool_name,
            args,
            partial_result,
            ..
        } => {
            let (state, _) = upsert_state_from_tool_name(
                tool_call_states,
                entry_index,
                tool_call_id,
                tool_name,
                Some(args.clone()),
            );
            let mut fields = seed_tool_update_fields(&state, Some(ToolCallStatus::InProgress));
            fields.raw_output = Some(partial_result.clone());
            if let Some(result) = decode_tool_result(partial_result) {
                let content = content_parts_to_tool_call_content(&result.content);
                if !content.is_empty() {
                    fields.content = Some(content);
                }
            }

            vec![build_tool_call_update_notification(
                session_id,
                source,
                turn_id,
                tool_call_id,
                &state,
                fields,
                None,
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
            let (state, _) = upsert_state_from_tool_name(
                tool_call_states,
                entry_index,
                tool_call_id,
                tool_name,
                Some(args.clone()),
            );
            let mut notifications = Vec::new();
            let mut fields = seed_tool_update_fields(&state, Some(ToolCallStatus::Pending));
            fields.status = Some(ToolCallStatus::Pending);
            fields.raw_output = Some(serde_json::json!({
                "approval_state": "pending",
                "reason": reason,
                "details": details,
            }));
            fields.content = Some(vec![ToolCallContent::from(ContentBlock::Text(
                TextContent::new(format!("等待审批：{reason}")),
            ))]);

            notifications.push(build_tool_call_update_notification(
                session_id,
                source,
                turn_id,
                tool_call_id,
                &state,
                fields,
                None,
            ));
            notifications.push(make_event_notification(
                session_id,
                source,
                turn_id,
                state.entry_index,
                EventDescription {
                    event_type: "approval_requested",
                    severity: "warning",
                    message: format!("工具 `{tool_name}` 正等待审批"),
                    data: serde_json::json!({
                        "tool_call_id": tool_call_id,
                        "tool_name": tool_name,
                        "reason": reason,
                        "args": args,
                        "details": details,
                    }),
                },
            ));
            notifications
        }

        AgentEvent::ToolExecutionApprovalResolved {
            tool_call_id,
            tool_name,
            args,
            approved,
            reason,
            ..
        } => {
            let (state, _) = upsert_state_from_tool_name(
                tool_call_states,
                entry_index,
                tool_call_id,
                tool_name,
                Some(args.clone()),
            );
            let mut notifications = Vec::new();
            let mut fields = seed_tool_update_fields(
                &state,
                Some(if *approved {
                    ToolCallStatus::InProgress
                } else {
                    ToolCallStatus::Failed
                }),
            );
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

            notifications.push(build_tool_call_update_notification(
                session_id,
                source,
                turn_id,
                tool_call_id,
                &state,
                fields,
                None,
            ));
            notifications.push(make_event_notification(
                session_id,
                source,
                turn_id,
                state.entry_index,
                EventDescription {
                    event_type: "approval_resolved",
                    severity: if *approved { "info" } else { "warning" },
                    message: if *approved {
                        format!("工具 `{tool_name}` 已获批准并继续执行")
                    } else {
                        format!("工具 `{tool_name}` 已被拒绝执行")
                    },
                    data: serde_json::json!({
                        "tool_call_id": tool_call_id,
                        "tool_name": tool_name,
                        "approved": approved,
                        "reason": reason,
                        "args": args,
                    }),
                },
            ));
            notifications
        }

        AgentEvent::ToolExecutionEnd {
            tool_call_id,
            tool_name,
            result,
            is_error,
        } => {
            let (state, _) = upsert_state_from_tool_name(
                tool_call_states,
                entry_index,
                tool_call_id,
                tool_name,
                None,
            );

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

            let mut fields = seed_tool_update_fields(&state, Some(status));
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

            vec![build_tool_call_update_notification(
                session_id,
                source,
                turn_id,
                tool_call_id,
                &state,
                fields,
                None,
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

/// 从 `LlmProviderRepository` 和 `SettingsRepository` 构建 `PiAgentConnector`。
///
/// Provider 列表从 `llm_providers` DB 表加载。
/// `settings_repo` 仅用于 `agent.pi.system_prompt` 等非 provider 设置。
/// 按 sort_order，首个完成注册的 provider 的首个模型作为默认 bridge。
pub async fn build_pi_agent_connector(
    workspace_root: &Path,
    settings: &dyn agentdash_domain::settings::SettingsRepository,
    llm_provider_repo: &dyn LlmProviderRepository,
) -> Option<PiAgentConnector> {
    let system_prompt = read_setting_str(settings, "agent.pi.system_prompt")
        .await
        .or_else(|| std::env::var("PI_AGENT_SYSTEM_PROMPT").ok())
        .unwrap_or_else(|| {
            "你是 AgentDash 内置 AI 助手，一个通用的编程与任务执行 Agent。请用中文回复用户。"
                .to_string()
        });

    let providers = build_provider_entries_from_db(llm_provider_repo).await;

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
    );

    // 注册所有 provider（含第一个 provider）
    for provider in providers {
        connector.add_provider(provider.entry);
    }

    if connector.providers.is_empty() {
        tracing::info!("PiAgentConnector 已初始化（动态占位模式，等待 provider 配置）");
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
    use std::sync::{Mutex as StdMutex, RwLock};

    fn test_source() -> AgentDashSourceV1 {
        AgentDashSourceV1::new("pi-agent", "local_executor")
    }

    #[derive(Default)]
    struct RecordingBridge {
        requests: StdMutex<Vec<agentdash_agent::BridgeRequest>>,
    }

    #[async_trait::async_trait]
    impl LlmBridge for RecordingBridge {
        async fn stream_complete(
            &self,
            request: agentdash_agent::BridgeRequest,
        ) -> std::pin::Pin<Box<dyn futures::Stream<Item = agentdash_agent::StreamChunk> + Send>>
        {
            self.requests
                .lock()
                .expect("recording bridge lock poisoned")
                .push(request);
            Box::pin(tokio_stream::once(agentdash_agent::StreamChunk::Done(
                agentdash_agent::BridgeResponse {
                    message: agentdash_agent::AgentMessage::assistant("done"),
                    raw_content: vec![agentdash_agent::ContentPart::text("done")],
                    usage: agentdash_agent::TokenUsage::default(),
                },
            )))
        }
    }

    struct EmptyRuntimeToolProvider;

    #[async_trait::async_trait]
    impl RuntimeToolProvider for EmptyRuntimeToolProvider {
        async fn build_tools(
            &self,
            _context: &ExecutionContext,
        ) -> Result<Vec<agentdash_spi::DynAgentTool>, ConnectorError> {
            Ok(Vec::new())
        }
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

    #[derive(Default)]
    struct TestLlmProviderRepository {
        providers: RwLock<Vec<agentdash_domain::llm_provider::LlmProvider>>,
    }

    impl TestLlmProviderRepository {
        fn set_providers(&self, providers: Vec<agentdash_domain::llm_provider::LlmProvider>) {
            *self.providers.write().expect("test provider lock") = providers;
        }
    }

    #[async_trait::async_trait]
    impl agentdash_domain::llm_provider::LlmProviderRepository for TestLlmProviderRepository {
        async fn create(
            &self,
            _provider: &agentdash_domain::llm_provider::LlmProvider,
        ) -> Result<(), DomainError> {
            Ok(())
        }
        async fn get_by_id(
            &self,
            _id: uuid::Uuid,
        ) -> Result<Option<agentdash_domain::llm_provider::LlmProvider>, DomainError> {
            Ok(None)
        }
        async fn list_all(
            &self,
        ) -> Result<Vec<agentdash_domain::llm_provider::LlmProvider>, DomainError> {
            Ok(self.providers.read().expect("test provider lock").clone())
        }
        async fn list_enabled(
            &self,
        ) -> Result<Vec<agentdash_domain::llm_provider::LlmProvider>, DomainError> {
            Ok(self.providers.read().expect("test provider lock")
                .iter()
                .filter(|p| p.enabled)
                .cloned()
                .collect())
        }
        async fn update(
            &self,
            _provider: &agentdash_domain::llm_provider::LlmProvider,
        ) -> Result<(), DomainError> {
            Ok(())
        }
        async fn delete(&self, _id: uuid::Uuid) -> Result<(), DomainError> {
            Ok(())
        }
        async fn reorder(&self, _ids: &[uuid::Uuid]) -> Result<(), DomainError> {
            Ok(())
        }
    }

    async fn discover_options_state(connector: &PiAgentConnector) -> serde_json::Value {
        let patches = connector
            .discover_options_stream("PI_AGENT", None)
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
        let mut chunk_message_ids = HashMap::new();
        let mut chunk_emit_states = HashMap::new();
        let mut tool_call_states = HashMap::new();
        let notifications = convert_event_to_notifications(
            &event,
            &SessionId::new("session-1"),
            &test_source(),
            "turn-1",
            &mut entry_index,
            &mut chunk_message_ids,
            &mut chunk_emit_states,
            &mut tool_call_states,
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
    fn tool_call_stream_events_map_to_pending_start_and_updates() {
        let start_event = AgentEvent::MessageUpdate {
            message: AgentMessage::Assistant {
                content: vec![],
                tool_calls: vec![agentdash_agent::ToolCallInfo {
                    id: "tool-1".to_string(),
                    call_id: Some("tool-1".to_string()),
                    name: "shell_exec".to_string(),
                    arguments: serde_json::json!({ "command": "echo he" }),
                }],
                stop_reason: Some(StopReason::ToolUse),
                error_message: None,
                usage: None,
                timestamp: Some(agentdash_agent::types::now_millis()),
            },
            event: AssistantStreamEvent::ToolCallStart {
                content_index: 0,
                tool_call_id: "tool-1".to_string(),
                name: "shell_exec".to_string(),
            },
        };
        let delta_event = AgentEvent::MessageUpdate {
            message: AgentMessage::Assistant {
                content: vec![],
                tool_calls: vec![agentdash_agent::ToolCallInfo {
                    id: "tool-1".to_string(),
                    call_id: Some("tool-1".to_string()),
                    name: "shell_exec".to_string(),
                    arguments: serde_json::json!({ "command": "echo hello" }),
                }],
                stop_reason: Some(StopReason::ToolUse),
                error_message: None,
                usage: None,
                timestamp: Some(agentdash_agent::types::now_millis()),
            },
            event: AssistantStreamEvent::ToolCallDelta {
                content_index: 0,
                tool_call_id: "tool-1".to_string(),
                name: "shell_exec".to_string(),
                delta: "\"llo\"".to_string(),
                draft: "{\"command\":\"echo hello\"}".to_string(),
                is_parseable: true,
            },
        };
        let end_event = AgentEvent::MessageUpdate {
            message: AgentMessage::Assistant {
                content: vec![],
                tool_calls: vec![agentdash_agent::ToolCallInfo {
                    id: "tool-1".to_string(),
                    call_id: Some("tool-1".to_string()),
                    name: "shell_exec".to_string(),
                    arguments: serde_json::json!({ "command": "echo hello" }),
                }],
                stop_reason: Some(StopReason::ToolUse),
                error_message: None,
                usage: None,
                timestamp: Some(agentdash_agent::types::now_millis()),
            },
            event: AssistantStreamEvent::ToolCallEnd {
                content_index: 0,
                tool_call: agentdash_agent::ToolCallInfo {
                    id: "tool-1".to_string(),
                    call_id: Some("tool-1".to_string()),
                    name: "shell_exec".to_string(),
                    arguments: serde_json::json!({ "command": "echo hello" }),
                },
            },
        };

        let mut entry_index = 0;
        let mut chunk_message_ids = HashMap::new();
        let mut chunk_emit_states = HashMap::new();
        let mut tool_call_states = HashMap::new();
        let start_notifications = convert_event_to_notifications(
            &start_event,
            &SessionId::new("session-1"),
            &test_source(),
            "turn-1",
            &mut entry_index,
            &mut chunk_message_ids,
            &mut chunk_emit_states,
            &mut tool_call_states,
        );
        let delta_notifications = convert_event_to_notifications(
            &delta_event,
            &SessionId::new("session-1"),
            &test_source(),
            "turn-1",
            &mut entry_index,
            &mut chunk_message_ids,
            &mut chunk_emit_states,
            &mut tool_call_states,
        );
        let end_notifications = convert_event_to_notifications(
            &end_event,
            &SessionId::new("session-1"),
            &test_source(),
            "turn-1",
            &mut entry_index,
            &mut chunk_message_ids,
            &mut chunk_emit_states,
            &mut tool_call_states,
        );

        assert_eq!(start_notifications.len(), 1);
        match &start_notifications[0].update {
            SessionUpdate::ToolCall(call) => {
                assert_eq!(call.status, ToolCallStatus::Pending);
                assert_eq!(call.title, "shell_exec");
                assert_eq!(
                    call.raw_input,
                    Some(serde_json::json!({ "command": "echo he" }))
                );
            }
            other => panic!("unexpected session update: {other:?}"),
        }
        assert_eq!(delta_notifications.len(), 1);
        match &delta_notifications[0].update {
            SessionUpdate::ToolCallUpdate(update) => {
                assert_eq!(update.fields.status, Some(ToolCallStatus::Pending));
                assert_eq!(update.fields.title.as_deref(), Some("shell_exec"));
                assert_eq!(
                    update.fields.raw_input,
                    Some(serde_json::json!({ "command": "echo hello" }))
                );
                let meta = update
                    .meta
                    .as_ref()
                    .expect("tool_call_update should include meta");
                let agentdash = agentdash_acp_meta::parse_agentdash_meta(meta)
                    .expect("tool_call_update meta should be parseable");
                assert_eq!(
                    agentdash.event.as_ref().map(|event| event.r#type.as_str()),
                    Some("tool_call_draft")
                );
                assert_eq!(
                    agentdash
                        .event
                        .as_ref()
                        .and_then(|event| event.data.as_ref())
                        .and_then(|data| data.get("draftInput"))
                        .and_then(|value| value.as_str()),
                    Some("{\"command\":\"echo hello\"}")
                );
            }
            other => panic!("unexpected session update: {other:?}"),
        }
        assert_eq!(end_notifications.len(), 1);
        match &end_notifications[0].update {
            SessionUpdate::ToolCallUpdate(update) => {
                assert_eq!(update.fields.status, Some(ToolCallStatus::Pending));
                assert_eq!(
                    update.fields.raw_input,
                    Some(serde_json::json!({ "command": "echo hello" }))
                );
            }
            other => panic!("unexpected session update: {other:?}"),
        }
    }

    #[test]
    fn tool_call_delta_preserves_unparseable_draft_in_meta() {
        let delta_event = AgentEvent::MessageUpdate {
            message: AgentMessage::Assistant {
                content: vec![],
                tool_calls: vec![agentdash_agent::ToolCallInfo {
                    id: "tool-fs-apply-patch-1".to_string(),
                    call_id: Some("tool-fs-apply-patch-1".to_string()),
                    name: "fs_apply_patch".to_string(),
                    arguments: serde_json::json!({}),
                }],
                stop_reason: Some(StopReason::ToolUse),
                error_message: None,
                usage: None,
                timestamp: Some(agentdash_agent::types::now_millis()),
            },
            event: AssistantStreamEvent::ToolCallDelta {
                content_index: 0,
                tool_call_id: "tool-fs-apply-patch-1".to_string(),
                name: "fs_apply_patch".to_string(),
                delta: "\"hello".to_string(),
                draft: "{\"patch\":\"*** Begin Patch\\n*** Add File: notes.txt\\n+hello".to_string(),
                is_parseable: false,
            },
        };

        let mut entry_index = 0;
        let mut chunk_message_ids = HashMap::new();
        let mut chunk_emit_states = HashMap::new();
        let mut tool_call_states = HashMap::new();
        let notifications = convert_event_to_notifications(
            &delta_event,
            &SessionId::new("session-1"),
            &test_source(),
            "turn-1",
            &mut entry_index,
            &mut chunk_message_ids,
            &mut chunk_emit_states,
            &mut tool_call_states,
        );

        assert_eq!(notifications.len(), 1);
        match &notifications[0].update {
            SessionUpdate::ToolCallUpdate(update) => {
                assert_eq!(update.fields.raw_input, Some(serde_json::json!({})));
                let meta = update
                    .meta
                    .as_ref()
                    .expect("tool_call_update should include meta");
                let agentdash = agentdash_acp_meta::parse_agentdash_meta(meta)
                    .expect("tool_call_update meta should be parseable");
                assert_eq!(
                    agentdash
                        .event
                        .as_ref()
                        .and_then(|event| event.data.as_ref())
                        .and_then(|data| data.get("draftInput"))
                        .and_then(|value| value.as_str()),
                    Some("{\"patch\":\"*** Begin Patch\\n*** Add File: notes.txt\\n+hello")
                );
                assert_eq!(
                    agentdash
                        .event
                        .as_ref()
                        .and_then(|event| event.data.as_ref())
                        .and_then(|data| data.get("isParseable"))
                        .and_then(|value| value.as_bool()),
                    Some(false)
                );
            }
            other => panic!("unexpected session update: {other:?}"),
        }
    }

    #[test]
    fn message_end_without_streamed_tool_call_emits_pending_tool_call() {
        let event = AgentEvent::MessageEnd {
            message: AgentMessage::Assistant {
                content: vec![],
                tool_calls: vec![agentdash_agent::ToolCallInfo {
                    id: "tool-final-1".to_string(),
                    call_id: Some("tool-final-1".to_string()),
                    name: "read_file".to_string(),
                    arguments: serde_json::json!({ "path": "README.md" }),
                }],
                stop_reason: Some(StopReason::ToolUse),
                error_message: None,
                usage: None,
                timestamp: Some(agentdash_agent::types::now_millis()),
            },
        };

        let mut entry_index = 0;
        let mut chunk_message_ids = HashMap::new();
        let mut chunk_emit_states = HashMap::new();
        let mut tool_call_states = HashMap::new();
        let notifications = convert_event_to_notifications(
            &event,
            &SessionId::new("session-1"),
            &test_source(),
            "turn-1",
            &mut entry_index,
            &mut chunk_message_ids,
            &mut chunk_emit_states,
            &mut tool_call_states,
        );

        assert_eq!(notifications.len(), 1);
        match &notifications[0].update {
            SessionUpdate::ToolCall(call) => {
                assert_eq!(call.status, ToolCallStatus::Pending);
                assert_eq!(call.title, "read_file");
                assert_eq!(call.kind, ToolKind::Read);
                assert_eq!(
                    call.raw_input,
                    Some(serde_json::json!({ "path": "README.md" }))
                );
            }
            other => panic!("unexpected session update: {other:?}"),
        }
    }

    #[test]
    fn execution_start_after_pending_tool_call_emits_in_progress_update() {
        let pending_event = AgentEvent::MessageUpdate {
            message: AgentMessage::Assistant {
                content: vec![],
                tool_calls: vec![agentdash_agent::ToolCallInfo {
                    id: "tool-run-1".to_string(),
                    call_id: Some("tool-run-1".to_string()),
                    name: "shell_exec".to_string(),
                    arguments: serde_json::json!({ "command": "cargo test" }),
                }],
                stop_reason: Some(StopReason::ToolUse),
                error_message: None,
                usage: None,
                timestamp: Some(agentdash_agent::types::now_millis()),
            },
            event: AssistantStreamEvent::ToolCallStart {
                content_index: 0,
                tool_call_id: "tool-run-1".to_string(),
                name: "shell_exec".to_string(),
            },
        };
        let execution_start = AgentEvent::ToolExecutionStart {
            tool_call_id: "tool-run-1".to_string(),
            tool_name: "shell_exec".to_string(),
            args: serde_json::json!({ "command": "cargo test" }),
        };

        let mut entry_index = 0;
        let mut chunk_message_ids = HashMap::new();
        let mut chunk_emit_states = HashMap::new();
        let mut tool_call_states = HashMap::new();
        let _ = convert_event_to_notifications(
            &pending_event,
            &SessionId::new("session-1"),
            &test_source(),
            "turn-1",
            &mut entry_index,
            &mut chunk_message_ids,
            &mut chunk_emit_states,
            &mut tool_call_states,
        );
        let notifications = convert_event_to_notifications(
            &execution_start,
            &SessionId::new("session-1"),
            &test_source(),
            "turn-1",
            &mut entry_index,
            &mut chunk_message_ids,
            &mut chunk_emit_states,
            &mut tool_call_states,
        );

        assert_eq!(notifications.len(), 1);
        match &notifications[0].update {
            SessionUpdate::ToolCallUpdate(update) => {
                assert_eq!(update.fields.status, Some(ToolCallStatus::InProgress));
                assert_eq!(update.fields.title.as_deref(), Some("shell_exec"));
            }
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
        let mut chunk_message_ids = HashMap::new();
        let mut chunk_emit_states = HashMap::new();
        let mut tool_call_states = HashMap::new();
        let update_notifications = convert_event_to_notifications(
            &update_event,
            &SessionId::new("session-1"),
            &test_source(),
            "turn-1",
            &mut entry_index,
            &mut chunk_message_ids,
            &mut chunk_emit_states,
            &mut tool_call_states,
        );
        let end_notifications = convert_event_to_notifications(
            &end_event,
            &SessionId::new("session-1"),
            &test_source(),
            "turn-1",
            &mut entry_index,
            &mut chunk_message_ids,
            &mut chunk_emit_states,
            &mut tool_call_states,
        );

        match &update_notifications[0].update {
            SessionUpdate::ToolCallUpdate(update) => {
                assert_eq!(update.fields.status, Some(ToolCallStatus::InProgress));
                assert_eq!(update.fields.title.as_deref(), Some("echo"));
                assert_eq!(update.fields.raw_output, Some(raw_result.clone()));
            }
            other => panic!("unexpected session update: {other:?}"),
        }
        assert_eq!(update_notifications.len(), 1);

        match &end_notifications[0].update {
            SessionUpdate::ToolCallUpdate(update) => {
                assert_eq!(update.fields.status, Some(ToolCallStatus::Completed));
                assert_eq!(update.fields.title.as_deref(), Some("echo"));
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
        let mut chunk_message_ids = HashMap::new();
        let mut chunk_emit_states = HashMap::new();
        let mut tool_call_states = HashMap::new();
        let notifications = convert_event_to_notifications(
            &event,
            &SessionId::new("session-1"),
            &test_source(),
            "turn-1",
            &mut entry_index,
            &mut chunk_message_ids,
            &mut chunk_emit_states,
            &mut tool_call_states,
        );

        assert_eq!(notifications.len(), 2);
        match &notifications[0].update {
            SessionUpdate::ToolCallUpdate(update) => {
                assert_eq!(update.fields.status, Some(ToolCallStatus::Pending));
                assert_eq!(update.fields.title.as_deref(), Some("shell_exec"));
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
    fn tool_execution_end_without_start_emits_orphan_terminal_update() {
        let result = AgentToolResult {
            content: vec![ContentPart::text("done")],
            is_error: false,
            details: None,
        };
        let raw_result = serde_json::to_value(&result).expect("tool result should serialize");
        let end_event = AgentEvent::ToolExecutionEnd {
            tool_call_id: "tool-end-only-1".to_string(),
            tool_name: "present_canvas".to_string(),
            result: raw_result,
            is_error: false,
        };

        let mut entry_index = 0;
        let mut chunk_message_ids = HashMap::new();
        let mut chunk_emit_states = HashMap::new();
        let mut tool_call_states = HashMap::new();
        let notifications = convert_event_to_notifications(
            &end_event,
            &SessionId::new("session-1"),
            &test_source(),
            "turn-1",
            &mut entry_index,
            &mut chunk_message_ids,
            &mut chunk_emit_states,
            &mut tool_call_states,
        );

        assert_eq!(notifications.len(), 1);
        match &notifications[0].update {
            SessionUpdate::ToolCallUpdate(update) => {
                assert_eq!(update.fields.status, Some(ToolCallStatus::Completed));
                assert_eq!(update.fields.title.as_deref(), Some("present_canvas"));
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
        let mut chunk_message_ids = HashMap::new();
        let mut chunk_emit_states = HashMap::new();
        let mut tool_call_states = HashMap::new();
        let notifications = convert_event_to_notifications(
            &event,
            &SessionId::new("session-1"),
            &test_source(),
            "turn-1",
            &mut entry_index,
            &mut chunk_message_ids,
            &mut chunk_emit_states,
            &mut tool_call_states,
        );

        assert_eq!(notifications.len(), 1);
        assert_eq!(entry_index, 1);
        match &notifications[0].update {
            SessionUpdate::AgentMessageChunk(chunk) => {
                match &chunk.content {
                    ContentBlock::Text(text) => assert_eq!(text.text, "Agent run aborted"),
                    other => panic!("unexpected content block: {other:?}"),
                }
                let meta = chunk.meta.as_ref().expect("chunk should include _meta");
                let agentdash = agentdash_acp_meta::parse_agentdash_meta(meta)
                    .expect("agentdash meta should be parseable");
                assert!(agentdash.event.is_none());
            }
            other => panic!("unexpected session update: {other:?}"),
        }
    }

    #[test]
    fn message_end_does_not_repeat_full_snapshot_after_deltas() {
        let delta_event = AgentEvent::MessageUpdate {
            message: AgentMessage::Assistant {
                content: vec![ContentPart::text("he")],
                tool_calls: vec![],
                stop_reason: Some(StopReason::Stop),
                error_message: None,
                usage: None,
                timestamp: Some(agentdash_agent::types::now_millis()),
            },
            event: AssistantStreamEvent::TextDelta {
                content_index: 0,
                text: "he".to_string(),
            },
        };
        let message_end = AgentEvent::MessageEnd {
            message: AgentMessage::Assistant {
                content: vec![ContentPart::text("hello")],
                tool_calls: vec![],
                stop_reason: Some(StopReason::Stop),
                error_message: None,
                usage: None,
                timestamp: Some(agentdash_agent::types::now_millis()),
            },
        };

        let mut entry_index = 0;
        let mut chunk_message_ids = HashMap::new();
        let mut chunk_emit_states = HashMap::new();
        let mut tool_call_states = HashMap::new();
        let delta_notifications = convert_event_to_notifications(
            &delta_event,
            &SessionId::new("session-1"),
            &test_source(),
            "turn-1",
            &mut entry_index,
            &mut chunk_message_ids,
            &mut chunk_emit_states,
            &mut tool_call_states,
        );
        let end_notifications = convert_event_to_notifications(
            &message_end,
            &SessionId::new("session-1"),
            &test_source(),
            "turn-1",
            &mut entry_index,
            &mut chunk_message_ids,
            &mut chunk_emit_states,
            &mut tool_call_states,
        );

        assert_eq!(delta_notifications.len(), 1);
        assert_eq!(end_notifications.len(), 1);
        match (&delta_notifications[0].update, &end_notifications[0].update) {
            (
                SessionUpdate::AgentMessageChunk(delta_chunk),
                SessionUpdate::AgentMessageChunk(end_chunk),
            ) => {
                assert_eq!(delta_chunk.message_id, end_chunk.message_id);
                match &end_chunk.content {
                    ContentBlock::Text(text) => assert_eq!(text.text, "llo"),
                    other => panic!("unexpected content block: {other:?}"),
                }
            }
            other => panic!("unexpected session update: {other:?}"),
        }
    }

    #[test]
    fn runtime_system_prompt_prefers_relative_workspace_paths() {
        let connector = PiAgentConnector::new(
            PathBuf::from("/tmp/test-workspace"),
            Arc::new(NoopBridge),
            "系统提示",
        );
        let context = ExecutionContext {
            turn_id: "turn-1".to_string(),
            workspace_root: PathBuf::from("/tmp/test-workspace"),
            working_directory: PathBuf::from("/tmp/test-workspace/crates/agentdash-agent"),
            environment_variables: HashMap::new(),
            executor_config: agentdash_spi::AgentConfig::new("PI_AGENT"),
            mcp_servers: vec![],
            address_space: None,
            hook_session: None,
            flow_capabilities: Default::default(),
            system_context: None,
            runtime_delegate: None,
            identity: None,
            restored_session_state: None,
        };

        let prompt = connector.build_runtime_system_prompt(&context, &["shell".to_string()]);
        // section headers
        assert!(prompt.contains("## Identity"));
        assert!(prompt.contains("## Workspace"));
        assert!(prompt.contains("## Tools"));
        // workspace relative paths
        assert!(prompt.contains("工作空间路径锚点"));
        assert!(prompt.contains("`crates/agentdash-agent`"));
        assert!(prompt.contains("不要把 `/tmp/test-workspace/...` 这类绝对路径直接写进工具参数"));
        assert!(prompt.contains("cwd 设为 `.`"));
        assert!(!prompt.contains(
            "当前工作目录（相对工作空间）：`/tmp/test-workspace/crates/agentdash-agent`"
        ));
    }

    #[tokio::test]
    async fn discovery_reflects_provider_added_to_db_without_restart() {
        use agentdash_domain::llm_provider::{LlmProvider, WireProtocol};

        let settings_repo = Arc::new(TestSettingsRepository::default());
        let llm_repo = Arc::new(TestLlmProviderRepository::default());

        let mut connector = build_pi_agent_connector(
            Path::new("/tmp/test-workspace"),
            settings_repo.as_ref(),
            llm_repo.as_ref(),
        )
        .await
        .expect("connector should initialize even without provider");
        connector.set_llm_provider_repository(llm_repo.clone());

        let initial = discover_options_state(&connector).await;
        assert_eq!(
            initial["options"]["model_selector"]["providers"],
            serde_json::json!([])
        );
        assert_eq!(
            initial["options"]["model_selector"]["default_model"],
            serde_json::Value::Null
        );

        let mut provider = LlmProvider::new("Anthropic Claude", "anthropic", WireProtocol::Anthropic);
        provider.api_key = "test-key".to_string();
        provider.default_model = rig::providers::anthropic::completion::CLAUDE_4_SONNET.to_string();
        llm_repo.set_providers(vec![provider]);

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
    async fn discovery_does_not_fall_back_to_startup_provider_after_db_cleared() {
        use agentdash_domain::llm_provider::{LlmProvider, WireProtocol};

        let settings_repo = Arc::new(TestSettingsRepository::default());
        let llm_repo = Arc::new(TestLlmProviderRepository::default());

        let mut provider = LlmProvider::new("Anthropic Claude", "anthropic", WireProtocol::Anthropic);
        provider.api_key = "test-key".to_string();
        provider.default_model = rig::providers::anthropic::completion::CLAUDE_4_SONNET.to_string();
        llm_repo.set_providers(vec![provider]);

        let mut connector = build_pi_agent_connector(
            Path::new("/tmp/test-workspace"),
            settings_repo.as_ref(),
            llm_repo.as_ref(),
        )
        .await
        .expect("connector should initialize");
        connector.set_llm_provider_repository(llm_repo.clone());

        let initial = discover_options_state(&connector).await;
        assert_eq!(
            initial["options"]["model_selector"]["providers"],
            serde_json::json!([{ "id": "anthropic", "name": "Anthropic Claude" }])
        );

        llm_repo.set_providers(vec![]);

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
        let llm_repo = TestLlmProviderRepository::default();
        let mut connector = build_pi_agent_connector(
            Path::new("/tmp/test-workspace"),
            repo.as_ref(),
            &llm_repo,
        )
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
                    workspace_root: PathBuf::from("/tmp/test-workspace"),
                    working_directory: PathBuf::from("/tmp/test-workspace"),
                    environment_variables: HashMap::new(),
                    executor_config: agentdash_spi::AgentConfig::new("PI_AGENT"),
                    mcp_servers: vec![],
                    address_space: None,
                    hook_session: None,
                    flow_capabilities: Default::default(),
                    system_context: None,
                    runtime_delegate: None,
                    identity: None,
                    restored_session_state: None,
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

    #[tokio::test]
    async fn prompt_restores_repository_messages_before_new_user_prompt() {
        let bridge = Arc::new(RecordingBridge::default());
        let mut connector = PiAgentConnector::new(
            PathBuf::from("/tmp/test-workspace"),
            bridge.clone(),
            "系统提示",
        );
        connector.set_runtime_tool_provider(Arc::new(EmptyRuntimeToolProvider));

        let mut stream = connector
            .prompt(
                "session-restore-1",
                None,
                &PromptPayload::Text("新的用户消息".to_string()),
                ExecutionContext {
                    turn_id: "turn-1".to_string(),
                    workspace_root: PathBuf::from("/tmp/test-workspace"),
                    working_directory: PathBuf::from("/tmp/test-workspace"),
                    environment_variables: HashMap::new(),
                    executor_config: agentdash_spi::AgentConfig::new("PI_AGENT"),
                    mcp_servers: vec![],
                    address_space: None,
                    hook_session: None,
                    flow_capabilities: Default::default(),
                    system_context: Some("## Owner Context\nproject".to_string()),
                    runtime_delegate: None,
                    identity: None,
                    restored_session_state: Some(agentdash_spi::RestoredSessionState {
                        messages: vec![
                            agentdash_spi::AgentMessage::user("历史用户消息"),
                            agentdash_spi::AgentMessage::assistant("历史助手消息"),
                        ],
                    }),
                },
            )
            .await
            .expect("prompt should start");

        while let Some(next) = stream.next().await {
            next.expect("stream item should succeed");
        }

        let requests = bridge
            .requests
            .lock()
            .expect("recording bridge lock poisoned");
        let request = requests.last().expect("bridge request should be recorded");
        assert_eq!(request.messages.len(), 3);
        assert_eq!(request.messages[0].first_text(), Some("历史用户消息"));
        assert_eq!(request.messages[1].first_text(), Some("历史助手消息"));
        assert_eq!(request.messages[2].first_text(), Some("新的用户消息"));
    }
}
