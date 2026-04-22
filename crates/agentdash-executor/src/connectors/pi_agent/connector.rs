/// PiAgentConnector — 基于 agentdash-agent 的进程内 Agent 连接器
///
/// 与 `VibeKanbanExecutorsConnector`（通过子进程执行）不同，
/// PiAgentConnector 在进程内运行 Agent Loop，直接调用 LLM API。
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
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
    ExecutionContext, ExecutionStream, Mount, MountCapability, PromptPayload, SystemPromptMode,
    workspace_path_from_context,
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
    system_prompt: String,
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
            agents: Arc::new(Mutex::new(HashMap::new())),
        }
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

    fn add_provider(&mut self, provider: ProviderEntry) {
        self.providers.push(provider);
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

    /// 组装运行时 system prompt。
    ///
    /// 采用统一 Markdown section 格式，每段以 `## 标题` 标注来源，
    /// 最终以 `\n\n` 拼接。顺序即优先级：人设 → 上下文 → 环境 → 工具 → 扩展。
    fn build_runtime_system_prompt(
        &self,
        context: &ExecutionContext,
        runtime_tools: &[DynAgentTool],
    ) -> String {
        let tool_names: Vec<String> = runtime_tools
            .iter()
            .map(|tool| tool.name().to_string())
            .collect();
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

        // ── 2. Project Context: 会话级 owner 业务上下文 ──
        if let Some(ref ctx) = context.system_context
            && !ctx.trim().is_empty()
        {
            sections.push(format!("## Project Context\n\n{ctx}"));
        }

        // ── 2b. Companion Agents: 从 session capabilities 注入 ──
        if let Some(ref caps) = context.session_capabilities {
            if !caps.companion_agents.is_empty() {
                let lines = caps
                    .companion_agents
                    .iter()
                    .map(|a| {
                        format!(
                            "- **{}** (executor: `{}`): {}",
                            a.name, a.executor, a.display_name
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                sections.push(format!(
                    "## Companion Agents\n\n\
                     以下 agent 已关联到当前项目，可通过 `companion_request` 工具的 `agent_key` 参数按名称指定：\n\n\
                     {lines}"
                ));
            }
        }

        // ── 3. Workspace: 地址空间 / 工作目录 ──
        if let Some(vfs) = &context.vfs {
            let mount_lines = vfs
                .mounts
                .iter()
                .map(describe_mount)
                .collect::<Vec<_>>()
                .join("\n");
            let default_mount = vfs
                .default_mount()
                .map(|mount| mount.id.as_str())
                .unwrap_or("main");
            sections.push(format!(
                "## Workspace\n\n当前会话可访问的 VFS 挂载如下：\n\n{mount_lines}\n\n默认 mount：`{default_mount}`"
            ));
        } else {
            sections.push("## Workspace\n\n（当前会话未配置 VFS。）".to_string());
        }

        // ── 4. Available Tools: 统一工具清单（平台内嵌 + 平台 MCP + 外部 MCP）──
        {
            let has_builtin = !runtime_tools.is_empty();
            // 平台 MCP 的 server_name 由 `McpInjectionConfig::server_name()` 产出，
            // 统一以 `agentdash-` 前缀（例如 `agentdash-workflow-tools-<short_id>`）；
            // 用户自定义 MCP 不会使用该前缀，由此做 system prompt 级别的分组。
            let (platform_mcp_servers, user_mcp_servers): (Vec<_>, Vec<_>) = context
                .mcp_servers
                .iter()
                .partition(|server| is_platform_mcp_server(server));
            let has_platform_mcp = !platform_mcp_servers.is_empty();
            let has_user_mcp = !user_mcp_servers.is_empty();

            if has_builtin || has_platform_mcp || has_user_mcp {
                let mut tool_section = String::from("## Available Tools\n\n以下工具已注入当前会话，可直接调用：\n\n");

                // 平台工具段：cluster-based 内嵌工具 + 平台 MCP scope 工具
                if has_builtin || has_platform_mcp {
                    tool_section.push_str("### Platform Tools\n\n");
                    if has_builtin {
                        let builtin_lines = runtime_tools
                            .iter()
                            .map(describe_builtin_tool)
                            .collect::<Vec<_>>()
                            .join("\n");
                        tool_section.push_str(&builtin_lines);
                        tool_section.push_str("\n\n");
                    }
                    if has_platform_mcp {
                        let platform_mcp_lines = platform_mcp_servers
                            .iter()
                            .map(|server| describe_mcp_server(server))
                            .collect::<Vec<_>>()
                            .join("\n");
                        tool_section.push_str(
                            "以下平台 MCP Server 提供 Project/Story/Task/Workflow 级管理工具：\n\n",
                        );
                        tool_section.push_str(&platform_mcp_lines);
                        tool_section.push_str("\n\n");
                    }
                }

                // 外部 MCP 工具段（用户自定义）
                if has_user_mcp {
                    let mcp_lines = user_mcp_servers
                        .iter()
                        .map(|server| describe_mcp_server(server))
                        .collect::<Vec<_>>()
                        .join("\n");
                    tool_section.push_str("### MCP Tools\n\n");
                    tool_section.push_str("以下 MCP Server 已注入当前会话，其工具可在需要时使用：\n\n");
                    tool_section.push_str(&mcp_lines);
                    tool_section.push_str("\n\n");
                }

                // 路径规范（仅当有平台工具时）
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
                sections.push(tool_section);
            }
        }

        // ── 6. Hooks ──
        if let Some(hook_session) = &context.hook_session {
            let hook_parts = build_hook_runtime_sections(hook_session.as_ref());
            if !hook_parts.is_empty() {
                sections.push(format!("## Hooks\n\n{}", hook_parts.join("\n\n")));
            }
        }

        // ── 7. Skills：从 session capabilities 渲染 ──
        if let Some(ref caps) = context.session_capabilities {
            if let Some(skills_block) = format_skills_from_capabilities(caps, &tool_names) {
                sections.push(skills_block);
            }
        }

        sections.join("\n\n")
    }
}

/// 从 `SessionBaselineCapabilities` 渲染 skills XML 块。
/// 同样依赖 tool_names 中存在 fs_read / read_file 才生成。
fn format_skills_from_capabilities(
    caps: &agentdash_spi::SessionBaselineCapabilities,
    tool_names: &[String],
) -> Option<String> {
    let read_tool = tool_names
        .iter()
        .find(|t| *t == "fs_read" || *t == "read_file")?;

    let visible = caps.visible_skills();
    if visible.is_empty() {
        return None;
    }

    let mut lines = vec![
        format!(
            "The following skills provide specialized instructions for specific tasks.\n\
             Use the {read_tool} tool to load a skill's file when the task matches its description.\n\
             When a skill file references a relative path, resolve it against the skill directory (parent of SKILL.md)."
        ),
        String::new(),
        "<available_skills>".to_string(),
    ];
    for skill in &visible {
        lines.push("  <skill>".to_string());
        lines.push(format!("    <name>{}</name>", escape_xml(&skill.name)));
        lines.push(format!(
            "    <description>{}</description>",
            escape_xml(&skill.description)
        ));
        lines.push(format!(
            "    <location>{}</location>",
            escape_xml(&skill.file_path)
        ));
        lines.push("  </skill>".to_string());
    }
    lines.push("</available_skills>".to_string());
    Some(lines.join("\n"))
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// 从工作目录扫描 SKILL.md 文件，生成 slash command 列表。
///
/// 遍历本地 `.agents/skills/` 和 `skills/` 目录的一级子目录，
/// 解析 SKILL.md 的 frontmatter 提取 name 和 description。
fn discover_skill_slash_commands(mount_root: &Path) -> Vec<serde_json::Value> {
    let mut commands = Vec::new();
    let scan_dirs = [
        mount_root.join(".agents").join("skills"),
        mount_root.join("skills"),
    ];
    for dir in &scan_dirs {
        if !dir.is_dir() {
            continue;
        }
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.filter_map(|e| e.ok()) {
            let subdir = entry.path();
            if !subdir.is_dir() {
                continue;
            }
            let skill_md = subdir.join("SKILL.md");
            if !skill_md.exists() {
                continue;
            }
            let content = match std::fs::read_to_string(&skill_md) {
                Ok(c) => c,
                Err(_) => continue,
            };
            if let Some(fm) = parse_skill_frontmatter(&content) {
                let name = fm.name.unwrap_or_else(|| {
                    subdir
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default()
                });
                commands.push(serde_json::json!({
                    "name": format!("/skill:{name}"),
                    "description": fm.description.unwrap_or_default(),
                }));
            }
        }
    }
    commands
}

/// SKILL.md frontmatter 结构（仅用于 slash command 发现）
#[derive(serde::Deserialize, Default)]
struct SkillSlashCommandFrontmatter {
    name: Option<String>,
    description: Option<String>,
}

/// 解析 SKILL.md frontmatter
fn parse_skill_frontmatter(content: &str) -> Option<SkillSlashCommandFrontmatter> {
    let content = content.trim_start_matches('\u{feff}');
    if !content.starts_with("---") {
        return None;
    }
    let after_open = &content[3..];
    let close_pos = after_open.find("\n---")?;
    let yaml_str = &after_open[..close_pos];
    serde_yaml::from_str(yaml_str).ok()
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
            MountCapability::Watch => "watch",
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
            // 将 Agent 配置的 MCP servers 分流：
            // - 匹配 backend 上报能力的 → relay 路径（经本机转发）
            // - 其余 → 直连路径（云端直接 HTTP 连接）
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
            let mut runtime_tools: Vec<DynAgentTool> = Vec::new();
            let provider = self.runtime_tool_provider.as_ref().ok_or_else(|| {
                ConnectorError::InvalidConfig(
                    "PiAgentConnector 未配置 runtime tool provider".to_string(),
                )
            })?;
            runtime_base_tools = provider.build_tools(&context).await?;
            mcp_tools_runtime.extend(mcp_tools);
            mcp_tools_runtime.extend(relay_mcp_tools);
            runtime_tools.extend(runtime_base_tools.iter().cloned());
            runtime_tools.extend(mcp_tools_runtime.iter().cloned());
            agent.set_system_prompt(self.build_runtime_system_prompt(&context, &runtime_tools));
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
        self.agents
            .lock()
            .await
            .insert(
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
        runtime
            .agent
            .steer(AgentMessage::user(message))
            .await;
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
    ChunkEmitState, ToolCallEmitState, convert_event_to_notifications, describe_builtin_tool,
    describe_mcp_server,
};

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

    let mut connector = PiAgentConnector::new(global_default_bridge, system_prompt);

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
    use agent_client_protocol::{ContentBlock, SessionUpdate, ToolCallStatus, ToolKind};
    use agentdash_agent::{
        AgentEvent, AgentToolResult, AssistantStreamEvent, ContentPart, StopReason,
    };
    use agentdash_domain::DomainError;
    use agentdash_domain::settings::{Setting, SettingScope, SettingsRepository};
    use chrono::Utc;
    use std::sync::{Mutex as StdMutex, RwLock};

    fn test_source() -> AgentDashSourceV1 {
        AgentDashSourceV1::new("pi-agent", "local_executor")
    }

    fn test_vfs(root_ref: &str) -> agentdash_spi::Vfs {
        agentdash_spi::Vfs {
            mounts: vec![Mount {
                id: "workspace".to_string(),
                provider: "local_fs".to_string(),
                backend_id: "local".to_string(),
                root_ref: root_ref.to_string(),
                capabilities: vec![
                    MountCapability::Read,
                    MountCapability::Write,
                    MountCapability::List,
                    MountCapability::Search,
                    MountCapability::Exec,
                ],
                default_write: true,
                display_name: "Workspace".to_string(),
                metadata: serde_json::Value::Null,
            }],
            default_mount_id: Some("workspace".to_string()),
            ..Default::default()
        }
    }

    struct MockTool {
        name: String,
        description: String,
        schema: serde_json::Value,
    }

    #[async_trait::async_trait]
    impl agentdash_agent::AgentTool for MockTool {
        fn name(&self) -> &str {
            &self.name
        }
        fn description(&self) -> &str {
            &self.description
        }
        fn parameters_schema(&self) -> serde_json::Value {
            self.schema.clone()
        }
        async fn execute(
            &self,
            _tool_call_id: &str,
            _args: serde_json::Value,
            _cancel: tokio_util::sync::CancellationToken,
            _on_update: Option<agentdash_agent::ToolUpdateCallback>,
        ) -> Result<AgentToolResult, agentdash_agent::AgentToolError> {
            unreachable!("MockTool::execute should not be called in prompt tests")
        }
    }

    fn mock_tools(specs: &[(&str, &str)]) -> Vec<DynAgentTool> {
        specs
            .iter()
            .map(|(name, description)| {
                Arc::new(MockTool {
                    name: (*name).to_string(),
                    description: (*description).to_string(),
                    schema: serde_json::json!({}),
                }) as DynAgentTool
            })
            .collect()
    }

    fn mock_tool_with_schema(
        name: &str,
        description: &str,
        schema: serde_json::Value,
    ) -> DynAgentTool {
        Arc::new(MockTool {
            name: name.to_string(),
            description: description.to_string(),
            schema,
        }) as DynAgentTool
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

    struct StaticTool {
        name: String,
    }

    impl StaticTool {
        fn named(name: &str) -> agentdash_spi::DynAgentTool {
            Arc::new(Self {
                name: name.to_string(),
            })
        }
    }

    #[async_trait::async_trait]
    impl agentdash_spi::AgentTool for StaticTool {
        fn name(&self) -> &str {
            self.name.as_str()
        }

        fn description(&self) -> &str {
            "static test tool"
        }

        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false,
            })
        }

        async fn execute(
            &self,
            _tool_use_id: &str,
            _args: serde_json::Value,
            _cancel: tokio_util::sync::CancellationToken,
            _update: Option<agentdash_spi::ToolUpdateCallback>,
        ) -> Result<agentdash_spi::AgentToolResult, agentdash_spi::AgentToolError> {
            Ok(agentdash_spi::AgentToolResult {
                content: vec![agentdash_spi::ContentPart::text("ok")],
                is_error: false,
                details: None,
            })
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
            Ok(self
                .providers
                .read()
                .expect("test provider lock")
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
                draft: "{\"patch\":\"*** Begin Patch\\n*** Add File: notes.txt\\n+hello"
                    .to_string(),
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
    fn message_end_after_tool_call_reuses_text_entry_index_and_message_id() {
        // 回归：ToolCallStart 之前如果会 bump entry_index，
        // MessageEnd 的 reconcile 会命中空 state，把整段文本当作新 chunk 重发一次，
        // 前端就会出现"工具调用前后两条几乎相同的文本气泡"。
        let delta_event = AgentEvent::MessageUpdate {
            message: AgentMessage::Assistant {
                content: vec![ContentPart::text("he")],
                tool_calls: vec![],
                stop_reason: Some(StopReason::ToolUse),
                error_message: None,
                usage: None,
                timestamp: Some(agentdash_agent::types::now_millis()),
            },
            event: AssistantStreamEvent::TextDelta {
                content_index: 0,
                text: "he".to_string(),
            },
        };
        let tool_start_event = AgentEvent::MessageUpdate {
            message: AgentMessage::Assistant {
                content: vec![ContentPart::text("hello")],
                tool_calls: vec![agentdash_agent::ToolCallInfo {
                    id: "tool-1".to_string(),
                    call_id: Some("tool-1".to_string()),
                    name: "shell_exec".to_string(),
                    arguments: serde_json::json!({ "command": "ls" }),
                }],
                stop_reason: Some(StopReason::ToolUse),
                error_message: None,
                usage: None,
                timestamp: Some(agentdash_agent::types::now_millis()),
            },
            event: AssistantStreamEvent::ToolCallStart {
                content_index: 1,
                tool_call_id: "tool-1".to_string(),
                name: "shell_exec".to_string(),
            },
        };
        let message_end = AgentEvent::MessageEnd {
            message: AgentMessage::Assistant {
                content: vec![ContentPart::text("hello")],
                tool_calls: vec![agentdash_agent::ToolCallInfo {
                    id: "tool-1".to_string(),
                    call_id: Some("tool-1".to_string()),
                    name: "shell_exec".to_string(),
                    arguments: serde_json::json!({ "command": "ls" }),
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
        let tool_notifications = convert_event_to_notifications(
            &tool_start_event,
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
        assert_eq!(tool_notifications.len(), 1);
        assert_eq!(end_notifications.len(), 1);

        let delta_chunk = match &delta_notifications[0].update {
            SessionUpdate::AgentMessageChunk(chunk) => chunk,
            other => panic!("unexpected update: {other:?}"),
        };
        let end_chunk = match &end_notifications[0].update {
            SessionUpdate::AgentMessageChunk(chunk) => chunk,
            other => panic!("unexpected update: {other:?}"),
        };

        // 关键断言：两个 chunk 共享同一个 message_id（同一条文本 entry），
        // MessageEnd 只发 suffix "llo"，不是整段 "hello"。
        assert_eq!(
            delta_chunk.message_id, end_chunk.message_id,
            "MessageEnd reconcile 必须命中 TextDelta 的 chunk_emit_state，否则前端会渲染成两条文本气泡"
        );
        match &end_chunk.content {
            ContentBlock::Text(text) => assert_eq!(text.text, "llo"),
            other => panic!("unexpected content block: {other:?}"),
        }

        // tool_call 与所属 message 的文本共享 entry_index
        let delta_entry_index = delta_chunk
            .meta
            .as_ref()
            .and_then(|m| agentdash_acp_meta::parse_agentdash_meta(m))
            .and_then(|m| m.trace)
            .and_then(|t| t.entry_index);
        let tool_entry_index = match &tool_notifications[0].update {
            SessionUpdate::ToolCall(call) => call
                .meta
                .as_ref()
                .and_then(|m| agentdash_acp_meta::parse_agentdash_meta(m))
                .and_then(|m| m.trace)
                .and_then(|t| t.entry_index),
            other => panic!("unexpected update: {other:?}"),
        };
        assert_eq!(
            delta_entry_index, tool_entry_index,
            "tool_call 与其所在 message 的文本应共享 entry_index"
        );

        // MessageEnd 后 entry_index 恰好 +1
        assert_eq!(entry_index, 1);
    }

    #[test]
    fn runtime_system_prompt_prefers_relative_workspace_paths() {
        let connector = PiAgentConnector::new(Arc::new(NoopBridge), "系统提示");
        let context = ExecutionContext {
            turn_id: "turn-1".to_string(),
            working_directory: PathBuf::from("/tmp/test-workspace/crates/agentdash-agent"),
            environment_variables: HashMap::new(),
            executor_config: agentdash_spi::AgentConfig::new("PI_AGENT"),
            mcp_servers: vec![],
            relay_mcp_server_names: Default::default(),
            vfs: Some(test_vfs("/tmp/test-workspace")),
            hook_session: None,
            flow_capabilities: Default::default(),
            system_context: None,
            runtime_delegate: None,
            identity: None,
            restored_session_state: None,
            session_capabilities: None,
        };

        let tools = mock_tools(&[("shell", "Run a shell command")]);
        let prompt = connector.build_runtime_system_prompt(&context, &tools);
        assert!(prompt.contains("## Identity"));
        assert!(prompt.contains("## Workspace"));
        assert!(prompt.contains("## Available Tools"));
        assert!(prompt.contains("### Platform Tools"));
        assert!(prompt.contains("/tmp/test-workspace"));
        assert!(prompt.contains("- **shell**: Run a shell command"));
    }

    #[test]
    fn runtime_system_prompt_renders_tool_parameters() {
        let connector = PiAgentConnector::new(Arc::new(NoopBridge), "系统提示");
        let context = ExecutionContext {
            turn_id: "turn-params".to_string(),
            working_directory: PathBuf::from("/tmp/ws"),
            environment_variables: HashMap::new(),
            executor_config: agentdash_spi::AgentConfig::new("PI_AGENT"),
            mcp_servers: vec![],
            relay_mcp_server_names: Default::default(),
            vfs: Some(test_vfs("/tmp/ws")),
            hook_session: None,
            flow_capabilities: Default::default(),
            system_context: None,
            runtime_delegate: None,
            identity: None,
            restored_session_state: None,
            session_capabilities: None,
        };

        let tools = vec![mock_tool_with_schema(
            "fs_read",
            "Read a file",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Target path"},
                    "start_line": {"type": "integer"},
                    "tags": {"type": "array", "items": {"type": "string"}},
                },
                "required": ["path"],
            }),
        )];
        let prompt = connector.build_runtime_system_prompt(&context, &tools);

        assert!(
            prompt.contains("- **fs_read**: Read a file"),
            "should render tool header"
        );
        assert!(
            prompt.contains("`path` (string, required): Target path"),
            "should mark required param with description"
        );
        assert!(
            prompt.contains("`start_line` (integer)"),
            "should render optional param without required marker"
        );
        assert!(
            prompt.contains("`tags` (array<string>)"),
            "should render array element type"
        );
    }

    #[test]
    fn runtime_system_prompt_renders_session_capabilities() {
        use agentdash_spi::session_capabilities::{
            CompanionAgentEntry, SessionBaselineCapabilities, SkillEntry,
        };

        let connector = PiAgentConnector::new(Arc::new(NoopBridge), "系统提示");
        let context = ExecutionContext {
            turn_id: "turn-cap".to_string(),
            working_directory: PathBuf::from("/tmp/ws"),
            environment_variables: HashMap::new(),
            executor_config: agentdash_spi::AgentConfig::new("PI_AGENT"),
            mcp_servers: vec![],
            relay_mcp_server_names: Default::default(),
            vfs: Some(test_vfs("/tmp/ws")),
            hook_session: None,
            flow_capabilities: Default::default(),
            system_context: None,
            runtime_delegate: None,
            identity: None,
            restored_session_state: None,
            session_capabilities: Some(SessionBaselineCapabilities {
                companion_agents: vec![CompanionAgentEntry {
                    name: "reviewer".to_string(),
                    executor: "PI_AGENT".to_string(),
                    display_name: "Code Reviewer".to_string(),
                }],
                skills: vec![
                    SkillEntry {
                        name: "test-skill".to_string(),
                        description: "A testing skill".to_string(),
                        file_path: "/ws/skills/test/SKILL.md".to_string(),
                        disable_model_invocation: false,
                    },
                    SkillEntry {
                        name: "hidden-skill".to_string(),
                        description: "Should not appear".to_string(),
                        file_path: "/ws/skills/hidden/SKILL.md".to_string(),
                        disable_model_invocation: true,
                    },
                ],
            }),
        };

        let tools = mock_tools(&[("fs_read", "Read a file"), ("shell", "Run a shell command")]);
        let prompt = connector.build_runtime_system_prompt(&context, &tools);

        assert!(
            prompt.contains("Companion Agents"),
            "should render companion agents section"
        );
        assert!(prompt.contains("reviewer"), "should include agent name");
        assert!(
            prompt.contains("Code Reviewer"),
            "should include display name"
        );
        assert!(
            prompt.contains("available_skills"),
            "should render skills block"
        );
        assert!(
            prompt.contains("test-skill"),
            "should include visible skill"
        );
        assert!(
            !prompt.contains("hidden-skill"),
            "should exclude disabled skill"
        );
    }

    #[tokio::test]
    async fn discovery_reflects_provider_added_to_db_without_restart() {
        use agentdash_domain::llm_provider::{LlmProvider, WireProtocol};

        let settings_repo = Arc::new(TestSettingsRepository::default());
        let llm_repo = Arc::new(TestLlmProviderRepository::default());

        let mut connector = build_pi_agent_connector(settings_repo.as_ref(), llm_repo.as_ref())
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

        let mut provider =
            LlmProvider::new("Anthropic Claude", "anthropic", WireProtocol::Anthropic);
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

        let mut provider =
            LlmProvider::new("Anthropic Claude", "anthropic", WireProtocol::Anthropic);
        provider.api_key = "test-key".to_string();
        provider.default_model = rig::providers::anthropic::completion::CLAUDE_4_SONNET.to_string();
        llm_repo.set_providers(vec![provider]);

        let mut connector = build_pi_agent_connector(settings_repo.as_ref(), llm_repo.as_ref())
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
        let mut connector = build_pi_agent_connector(repo.as_ref(), &llm_repo)
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
                    working_directory: PathBuf::from("/tmp/test-workspace"),
                    environment_variables: HashMap::new(),
                    executor_config: agentdash_spi::AgentConfig::new("PI_AGENT"),
                    mcp_servers: vec![],
                    relay_mcp_server_names: Default::default(),
                    vfs: Some(test_vfs("/tmp/test-workspace")),
                    hook_session: None,
                    flow_capabilities: Default::default(),
                    system_context: None,
                    runtime_delegate: None,
                    identity: None,
                    restored_session_state: None,
                    session_capabilities: None,
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
        let mut connector = PiAgentConnector::new(bridge.clone(), "系统提示");
        connector.set_runtime_tool_provider(Arc::new(EmptyRuntimeToolProvider));

        let mut stream = connector
            .prompt(
                "session-restore-1",
                None,
                &PromptPayload::Text("新的用户消息".to_string()),
                ExecutionContext {
                    turn_id: "turn-1".to_string(),
                    working_directory: PathBuf::from("/tmp/test-workspace"),
                    environment_variables: HashMap::new(),
                    executor_config: agentdash_spi::AgentConfig::new("PI_AGENT"),
                    mcp_servers: vec![],
                    relay_mcp_server_names: Default::default(),
                    vfs: Some(test_vfs("/tmp/test-workspace")),
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
                    session_capabilities: None,
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

    #[tokio::test]
    async fn update_session_mcp_servers_replaces_previous_mcp_tools() {
        let connector = PiAgentConnector::new(Arc::new(NoopBridge), "系统提示");

        let base_tool = StaticTool::named("fs_read");
        let old_mcp_tool = StaticTool::named("mcp_tool_old");

        let mut agent = Agent::new(
            Arc::new(NoopBridge),
            agentdash_agent::AgentConfig::default(),
        );
        agent.set_tools(vec![base_tool.clone(), old_mcp_tool.clone()]);

        connector.agents.lock().await.insert(
            "session-replace-mcp".to_string(),
            PiAgentSessionRuntime {
                agent,
                runtime_base_tools: vec![base_tool.clone()],
                mcp_tools: vec![old_mcp_tool],
            },
        );

        connector
            .update_session_mcp_servers("session-replace-mcp", vec![])
            .await
            .expect("replace-set should succeed");

        let agents = connector.agents.lock().await;
        let runtime = agents
            .get("session-replace-mcp")
            .expect("runtime should exist");
        assert!(
            runtime.mcp_tools.is_empty(),
            "MCP tool set should be fully replaced by empty target set"
        );
        let state = runtime
            .agent
            .try_state()
            .expect("agent state should be readable");
        let names: Vec<String> = state
            .tools
            .iter()
            .map(|tool| tool.name().to_string())
            .collect();
        assert_eq!(names, vec!["fs_read".to_string()]);
    }
}
