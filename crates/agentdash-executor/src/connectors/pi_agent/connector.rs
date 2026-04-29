/// PiAgentConnector — 基于 agentdash-agent 的进程内 Agent 连接器
///
/// 与 `VibeKanbanExecutorsConnector`（通过子进程执行）不同，
/// PiAgentConnector 在进程内运行 Agent Loop，直接调用 LLM API。
use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::Arc;

use agent_client_protocol::{SessionId, SessionNotification};
use futures::stream::BoxStream;
use tokio::sync::Mutex;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;

use agentdash_acp_meta::AgentDashSourceV1;

use agentdash_agent::{Agent, AgentConfig, AgentMessage, DynAgentTool, LlmBridge};
use agentdash_domain::llm_provider::LlmProviderRepository;
use agentdash_domain::settings::SettingsRepository;

use crate::connectors::pi_agent::pi_agent_mcp::discover_mcp_tools;
use crate::connectors::pi_agent::pi_agent_provider_registry::{
    CONTEXT_WINDOW_STANDARD, ProviderEntry, build_provider_entries_from_db,
};
use crate::hook_events::build_hook_trace_notification;
use agentdash_spi::connector::RuntimeToolProvider;
use agentdash_spi::mcp_relay::McpRelayProvider;
use agentdash_spi::{
    AgentConnector, AgentInfo, ConnectorCapabilities, ConnectorError, ConnectorType,
    ExecutionContext, ExecutionStream, Mount, MountCapability, PromptPayload,
    SystemPromptMode, workspace_path_from_context,
};

/// 从 McpServer（外部类型）提取 server name
fn extract_mcp_server_name(server: &agent_client_protocol::McpServer) -> String {
    serde_json::to_value(server)
        .ok()
        .and_then(|v| v.get("name").and_then(|n| n.as_str()).map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string())
}

/// 判断 MCP server 是否为平台注入的 MCP（relay/story/task/workflow scope）。
///
/// 平台 MCP 的 server name 由 `McpInjectionConfig::server_name()` 产出，统一以 `agentdash-` 前缀
/// 开头（如 `agentdash-relay-tools`、`agentdash-workflow-tools-<short_id>`）；
/// 用户自定义 MCP 不会使用该前缀，由此可在 system prompt 中把两者分组展示。
fn is_platform_mcp_server(server: &agent_client_protocol::McpServer) -> bool {
    extract_mcp_server_name(server).starts_with("agentdash-")
}

// ─── PiAgentConnector ───────────────────────────────────────────

pub struct PiAgentConnector {
    /// 默认 bridge：供 title 生成复用、以及 bootstrap 尚无 provider 配置时的占位。
    bridge: Arc<dyn LlmBridge>,
    /// 已注册的 provider 列表（按注册顺序，首个命中的 provider 优先）
    providers: Vec<ProviderEntry>,
    runtime_tool_provider: Option<Arc<dyn RuntimeToolProvider>>,
    mcp_relay_provider: Option<Arc<dyn McpRelayProvider>>,
    settings_repo: Option<Arc<dyn SettingsRepository>>,
    llm_provider_repo: Option<Arc<dyn LlmProviderRepository>>,
    /// Layer 0: 系统全局 base system prompt。
    system_prompt: String,
    /// Layer 2: 用户偏好提示列表（每条独立的偏好指令）。
    user_preferences: Vec<String>,
    agents: Arc<Mutex<HashMap<String, PiAgentSessionRuntime>>>,
}

struct PiAgentSessionRuntime {
    agent: Agent,
    /// runtime tool provider 产出的基础工具（不含 MCP）。
    runtime_base_tools: Vec<DynAgentTool>,
    /// 当前生效的 MCP 工具集合（直连 + relay）。
    mcp_tools: Vec<DynAgentTool>,
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
    pub fn new(bridge: Arc<dyn LlmBridge>, system_prompt: impl Into<String>) -> Self {
        Self {
            bridge,
            providers: Vec::new(),
            runtime_tool_provider: None,
            mcp_relay_provider: None,
            settings_repo: None,
            llm_provider_repo: None,
            system_prompt: system_prompt.into(),
            user_preferences: Vec::new(),
            agents: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn set_user_preferences(&mut self, preferences: Vec<String>) {
        self.user_preferences = preferences;
    }

    pub fn base_system_prompt(&self) -> &str {
        &self.system_prompt
    }

    pub fn user_preferences(&self) -> &[String] {
        &self.user_preferences
    }

    pub fn default_bridge(&self) -> Arc<dyn LlmBridge> {
        self.bridge.clone()
    }

    pub fn set_runtime_tool_provider(&mut self, provider: Arc<dyn RuntimeToolProvider>) {
        self.runtime_tool_provider = Some(provider);
    }

    pub fn set_mcp_relay_provider(&mut self, provider: Arc<dyn McpRelayProvider>) {
        self.mcp_relay_provider = Some(provider);
    }

    pub fn set_settings_repository(&mut self, settings_repo: Arc<dyn SettingsRepository>) {
        self.settings_repo = Some(settings_repo);
    }

    pub fn set_llm_provider_repository(&mut self, repo: Arc<dyn LlmProviderRepository>) {
        self.llm_provider_repo = Some(repo);
    }

    pub(crate) fn add_provider(&mut self, provider: ProviderEntry) {
        self.providers.push(provider);
    }

    pub(crate) fn provider_count(&self) -> usize {
        self.providers.len()
    }

    /// 将 Agent 配置的 MCP servers 按 relay 标记分为两组。
    /// relay 标记来自配置层（`relay_mcp_server_names`），不做运行时探测。
    fn partition_mcp_servers(
        &self,
        servers: &[agent_client_protocol::McpServer],
        relay_names_set: &std::collections::HashSet<String>,
    ) -> (Vec<String>, Vec<agent_client_protocol::McpServer>) {
        let mut relay_names = Vec::new();
        let mut direct = Vec::new();

        for server in servers {
            let name = extract_mcp_server_name(server);
            if relay_names_set.contains(&name) {
                tracing::info!(server = %name, "MCP server 走 relay 路径（配置标记）");
                relay_names.push(name);
            } else {
                direct.push(server.clone());
            }
        }

        (relay_names, direct)
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

    /// Application 层已预组装 prompt（不含 tools section），
    /// 在 connector 侧补充 Available Tools 段落。
    fn augment_assembled_prompt(
        &self,
        pre_assembled: &str,
        context: &ExecutionContext,
        runtime_tools: &[DynAgentTool],
    ) -> String {
        let tool_section = self.build_tools_section(context, runtime_tools);
        if tool_section.is_empty() {
            pre_assembled.to_string()
        } else {
            format!("{pre_assembled}\n\n{tool_section}")
        }
    }

    /// 渲染 Available Tools section（S2 完成后此函数将移至 assembler）。
    fn build_tools_section(
        &self,
        context: &ExecutionContext,
        runtime_tools: &[DynAgentTool],
    ) -> String {
        let has_builtin = !runtime_tools.is_empty();
        let (platform_mcp_servers, user_mcp_servers): (Vec<_>, Vec<_>) = context
            .mcp_servers
            .iter()
            .partition(|server| is_platform_mcp_server(server));
        let has_platform_mcp = !platform_mcp_servers.is_empty();
        let has_user_mcp = !user_mcp_servers.is_empty();

        if !has_builtin && !has_platform_mcp && !has_user_mcp {
            return String::new();
        }

        let mut tool_section =
            String::from("## Available Tools\n\n以下工具已注入当前会话，可直接调用：\n\n");

        if has_builtin || has_platform_mcp {
            tool_section.push_str("### Platform Tools\n\n");
            if has_builtin {
                let builtin_lines = runtime_tools
                    .iter()
                    .map(|t| super::stream_mapper::describe_builtin_tool(t))
                    .collect::<Vec<_>>()
                    .join("\n");
                tool_section.push_str(&builtin_lines);
                tool_section.push_str("\n\n");
            }
            if has_platform_mcp {
                let lines = platform_mcp_servers
                    .iter()
                    .map(|s| super::stream_mapper::describe_mcp_server(s))
                    .collect::<Vec<_>>()
                    .join("\n");
                tool_section.push_str(
                    "以下平台 MCP Server 提供 Project/Story/Task/Workflow 级管理工具：\n\n",
                );
                tool_section.push_str(&lines);
                tool_section.push_str("\n\n");
            }
        }

        if has_user_mcp {
            let lines = user_mcp_servers
                .iter()
                .map(|s| super::stream_mapper::describe_mcp_server(s))
                .collect::<Vec<_>>()
                .join("\n");
            tool_section.push_str("### MCP Tools\n\n");
            tool_section.push_str("以下 MCP Server 已注入当前会话，其工具可在需要时使用：\n\n");
            tool_section.push_str(&lines);
            tool_section.push_str("\n\n");
        }

        if has_builtin {
            if context.vfs.is_some() {
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
                let abs_hint = workspace_path_from_context(context)
                    .map(|root| root.display().to_string())
                    .unwrap_or_else(|_| "（未配置工作区路径）".to_string());
                tool_section.push_str(&format!(
                    "**路径规范**：调用 read_file、list_directory、search、write_file、shell 等工作空间工具时，路径参数必须优先使用相对工作空间根目录的路径。如果要在当前目录执行 shell，请将 cwd 设为 `.`；如果要进入子目录，请传类似 `crates/agentdash-agent` 这样的相对路径；不要把 `{abs_hint}/...` 这类绝对路径直接写进工具参数。",
                ));
            }
        }

        tool_section
    }

    /// 回退模式的 system prompt 组装（仅在 assembled_system_prompt 为 None 时使用）。
    ///
    /// 生产环境由 application 层 SystemPromptAssembler 完成完整组装；
    /// 此方法仅渲染 Identity + Workspace + Tools section 供测试 / 直接调用使用。
    fn build_runtime_system_prompt(
        &self,
        context: &ExecutionContext,
        runtime_tools: &[DynAgentTool],
    ) -> String {
        let mut sections: Vec<String> = Vec::new();

        // Identity（Layer 0-2）
        {
            let agent_sp = context
                .executor_config
                .system_prompt
                .as_deref()
                .filter(|s| !s.trim().is_empty());
            let identity = match (context.executor_config.system_prompt_mode, agent_sp) {
                (Some(SystemPromptMode::Override), Some(sp)) => sp.to_string(),
                (_, Some(sp)) => format!("{}\n\n{sp}", self.system_prompt),
                _ => self.system_prompt.clone(),
            };
            sections.push(format!("## Identity\n\n{identity}"));
        }

        // Workspace
        if let Some(vfs) = &context.vfs {
            let mount_lines = vfs
                .mounts
                .iter()
                .map(describe_mount)
                .collect::<Vec<_>>()
                .join("\n");
            let default_mount = vfs
                .default_mount()
                .map(|m| m.id.as_str())
                .unwrap_or("main");
            sections.push(format!(
                "## Workspace\n\n当前会话可访问的 VFS 挂载如下：\n\n{mount_lines}\n\n默认 mount：`{default_mount}`"
            ));
        }

        // Available Tools
        let tool_section = self.build_tools_section(context, runtime_tools);
        if !tool_section.is_empty() {
            sections.push(tool_section);
        }

        sections.join("\n\n")
    }
}

use super::slash_commands::discover_skill_slash_commands;

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
            MountCapability::Watch => "watch",
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "- {}: {}（provider={}, root_ref={}, capabilities=[{}]）",
        mount.id, mount.display_name, mount.provider, mount.root_ref, capabilities
    )
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

        // Bootstrap 占位模式：尚未注册任何 provider 时，给 UI 一个可显示的单模型条目
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

        // 从工作目录扫描 skill，注册为 slash commands
        let slash_commands: Vec<serde_json::Value> = _working_dir
            .as_deref()
            .map(discover_skill_slash_commands)
            .unwrap_or_default();

        let patch: json_patch::Patch = serde_json::from_value(serde_json::json!([
            { "op": "replace", "path": "/options/model_selector/providers", "value": all_providers },
            { "op": "replace", "path": "/options/model_selector/models", "value": all_models },
            { "op": "replace", "path": "/options/model_selector/default_model", "value": default_model },
            { "op": "replace", "path": "/options/loading_models", "value": false },
            { "op": "replace", "path": "/options/loading_agents", "value": false },
            { "op": "replace", "path": "/options/loading_slash_commands", "value": false },
            { "op": "replace", "path": "/options/slash_commands", "value": slash_commands }
        ])).expect("static patch must be valid");

        Ok(Box::pin(futures::stream::once(async move { patch })))
    }

    async fn has_live_session(&self, session_id: &str) -> bool {
        self.agents.lock().await.contains_key(session_id)
    }

    async fn build_session_tools(
        &self,
        context: &ExecutionContext,
    ) -> Result<Vec<DynAgentTool>, ConnectorError> {
        let (relay_server_names, direct_servers) =
            self.partition_mcp_servers(&context.mcp_servers, &context.relay_mcp_server_names);

        let mcp_tools = match discover_mcp_tools(&direct_servers).await {
            Ok(tools) => tools,
            Err(error) => {
                tracing::warn!("发现直连 MCP 工具失败，继续使用本地工具: {error}");
                Vec::new()
            }
        };
        let relay_mcp_tools = if let Some(relay) = &self.mcp_relay_provider {
            crate::connectors::pi_agent::relay_mcp::discover_relay_mcp_tools(
                relay.clone(),
                &relay_server_names,
            )
            .await
        } else {
            Vec::new()
        };

        let provider = self.runtime_tool_provider.as_ref().ok_or_else(|| {
            ConnectorError::InvalidConfig(
                "PiAgentConnector 未配置 runtime tool provider".to_string(),
            )
        })?;
        let runtime_base_tools = provider.build_tools(context).await?;

        let mut all_tools: Vec<DynAgentTool> = Vec::new();
        all_tools.extend(runtime_base_tools);
        all_tools.extend(mcp_tools);
        all_tools.extend(relay_mcp_tools);
        Ok(all_tools)
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

        let existing_runtime = {
            let mut agents = self.agents.lock().await;
            agents.remove(session_id)
        };

        let is_new_agent = existing_runtime.is_none();
        let mut runtime_base_tools: Vec<DynAgentTool> = Vec::new();
        let mut mcp_tools_runtime: Vec<DynAgentTool> = Vec::new();
        let mut agent = if let Some(runtime) = existing_runtime {
            runtime_base_tools = runtime.runtime_base_tools;
            mcp_tools_runtime = runtime.mcp_tools;
            runtime.agent
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
            let runtime_tools = if !context.assembled_tools.is_empty() {
                // Application 层已预构建——直接使用
                runtime_base_tools = context.assembled_tools.clone();
                context.assembled_tools.clone()
            } else {
                // 回退：connector 自行构建（向后兼容未经 pipeline 的直接调用）
                let tools = self.build_session_tools(&context).await?;
                runtime_base_tools = tools.clone();
                tools
            };

            let final_prompt = if let Some(pre_assembled) = &context.assembled_system_prompt {
                self.augment_assembled_prompt(pre_assembled, &context, &runtime_tools)
            } else {
                self.build_runtime_system_prompt(&context, &runtime_tools)
            };
            agent.set_system_prompt(final_prompt);
            agent.set_tools(runtime_tools);
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
        self.agents.lock().await.insert(
            session_id_owned.clone(),
            PiAgentSessionRuntime {
                agent,
                runtime_base_tools,
                mcp_tools: mcp_tools_runtime,
            },
        );

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
        if let Some(runtime) = self.agents.lock().await.get(session_id) {
            runtime.agent.abort();
        }
        Ok(())
    }

    async fn approve_tool_call(
        &self,
        session_id: &str,
        tool_call_id: &str,
    ) -> Result<(), ConnectorError> {
        let agents = self.agents.lock().await;
        let runtime = agents.get(session_id).ok_or_else(|| {
            ConnectorError::Runtime(format!("session `{session_id}` 当前没有活跃的 Pi Agent"))
        })?;
        runtime
            .agent
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
        let runtime = agents.get(session_id).ok_or_else(|| {
            ConnectorError::Runtime(format!("session `{session_id}` 当前没有活跃的 Pi Agent"))
        })?;
        runtime
            .agent
            .reject_tool_call(tool_call_id, reason)
            .await
            .map_err(|error| ConnectorError::Runtime(error.to_string()))
    }

    async fn update_session_mcp_servers(
        &self,
        session_id: &str,
        mcp_servers: Vec<agent_client_protocol::McpServer>,
    ) -> Result<(), ConnectorError> {
        let (relay_server_names, direct_servers) =
            self.partition_mcp_servers(&mcp_servers, &Default::default());

        let mcp_tools = match discover_mcp_tools(&direct_servers).await {
            Ok(tools) => tools,
            Err(error) => {
                tracing::warn!(
                    session_id = %session_id,
                    "MCP 热更新：发现直连 MCP 工具失败: {error}"
                );
                Vec::new()
            }
        };
        let relay_mcp_tools = if let Some(relay) = &self.mcp_relay_provider {
            crate::connectors::pi_agent::relay_mcp::discover_relay_mcp_tools(
                relay.clone(),
                &relay_server_names,
            )
            .await
        } else {
            Vec::new()
        };

        let mut new_mcp_tools: Vec<agentdash_agent::DynAgentTool> = Vec::new();
        new_mcp_tools.extend(mcp_tools);
        new_mcp_tools.extend(relay_mcp_tools);

        let mut agents = self.agents.lock().await;
        let runtime = agents.get_mut(session_id).ok_or_else(|| {
            ConnectorError::Runtime(format!(
                "session `{session_id}` 当前没有活跃的 Pi Agent，无法热更新 MCP"
            ))
        })?;

        let old_names: BTreeSet<String> = runtime
            .mcp_tools
            .iter()
            .map(|tool| tool.name().to_string())
            .collect();
        let new_names: BTreeSet<String> = new_mcp_tools
            .iter()
            .map(|tool| tool.name().to_string())
            .collect();

        runtime.mcp_tools = new_mcp_tools;
        let mut merged_tools = runtime.runtime_base_tools.clone();
        merged_tools.extend(runtime.mcp_tools.iter().cloned());
        let tool_count = runtime.mcp_tools.len();
        runtime.agent.set_tools(merged_tools);

        let added: Vec<String> = new_names.difference(&old_names).cloned().collect();
        let removed: Vec<String> = old_names.difference(&new_names).cloned().collect();

        tracing::info!(
            session_id = %session_id,
            added = ?added,
            removed = ?removed,
            new_mcp_tool_count = tool_count,
            "MCP 热更新完成（replace-set）"
        );

        Ok(())
    }

    async fn push_session_notification(
        &self,
        session_id: &str,
        message: String,
    ) -> Result<(), ConnectorError> {
        let agents = self.agents.lock().await;
        let runtime = agents.get(session_id).ok_or_else(|| {
            ConnectorError::Runtime(format!(
                "session `{session_id}` 当前没有活跃的 Pi Agent，无法注入通知"
            ))
        })?;
        runtime.agent.steer(AgentMessage::user(message)).await;
        Ok(())
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

use super::stream_mapper::{
    ChunkEmitState, ToolCallEmitState, convert_event_to_notifications,
};

#[cfg(test)]
#[path = "connector_tests.rs"]
mod tests;
