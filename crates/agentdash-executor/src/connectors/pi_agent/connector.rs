/// PiAgentConnector — 基于 agentdash-agent 的进程内 Agent 连接器
///
/// 与 `CodexBridgeConnector`（通过子进程执行）不同，
/// PiAgentConnector 在进程内运行 Agent Loop，直接调用 LLM API。
use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::Arc;

use agentdash_agent_protocol::{BackboneEnvelope, SourceInfo, user_input_blocks_to_content_parts};
use futures::stream::BoxStream;
use tokio::sync::Mutex;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;

use agentdash_agent::{Agent, AgentConfig, AgentMessage, DynAgentTool, LlmBridge};
use agentdash_domain::llm_provider::{
    LlmProviderCredentialRepository, LlmProviderRepository, LlmSecretCodec,
};
use agentdash_domain::settings::SettingsRepository;

use super::bridges::provider_registry::{
    CONTEXT_WINDOW_STANDARD, EffectiveLlmProviderProfile, ProviderEntry, ProviderModelResolveError,
    ProviderUnavailableReason, build_effective_profile_catalog_from_db,
};
use agentdash_spi::hooks::trace::build_hook_trace_envelope;
use agentdash_spi::hooks::{ContextFrame, ContextFrameSection};
use agentdash_spi::{
    AgentConnector, AgentInfo, ConnectorCapabilities, ConnectorError, ConnectorType,
    DiscoveryContext, ExecutionContext, ExecutionStream, PromptPayload,
};

// ─── PiAgentConnector ───────────────────────────────────────────

pub struct PiAgentConnector {
    /// 默认 bridge：供 title 生成复用、以及 bootstrap 尚无 provider 配置时的占位。
    bridge: Arc<dyn LlmBridge>,
    /// 已注册的 provider 列表（按注册顺序，首个命中的 provider 优先）
    providers: Vec<ProviderEntry>,
    settings_repo: Option<Arc<dyn SettingsRepository>>,
    llm_provider_repo: Option<Arc<dyn LlmProviderRepository>>,
    llm_provider_credential_repo: Option<Arc<dyn LlmProviderCredentialRepository>>,
    llm_secret_codec: Option<Arc<dyn LlmSecretCodec>>,
    /// Layer 0: 系统全局 base system prompt。
    system_prompt: String,
    agents: Arc<Mutex<HashMap<String, PiAgentSessionRuntime>>>,
}

struct PiAgentSessionRuntime {
    agent: Agent,
    /// 当前生效的完整工具列表（由 application 层预构建）。
    tools: Vec<DynAgentTool>,
    /// 上一次应用到 agent 的 identity prompt（用于跨 turn 判断是否需要热更新）。
    last_identity_prompt: Option<String>,
    /// 当前 Agent 内部 bridge 对应的模型选择。
    model_selection: PiAgentModelSelection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PiAgentModelSelection {
    provider_id: Option<String>,
    model_id: Option<String>,
}

impl PiAgentModelSelection {
    fn from_config(config: &agentdash_spi::AgentConfig) -> Self {
        Self {
            provider_id: normalize_model_selector_value(config.provider_id.as_deref()),
            model_id: normalize_model_selector_value(config.model_id.as_deref()),
        }
    }
}

struct ProviderRuntimeState {
    uses_dynamic_provider_catalog: bool,
    default_bridge: Option<Arc<dyn LlmBridge>>,
    default_model: Option<String>,
    profiles: Vec<EffectiveLlmProviderProfile>,
    providers: Vec<ProviderEntry>,
    unavailable_providers: HashMap<String, ProviderUnavailableReason>,
}

impl PiAgentConnector {
    pub fn new(bridge: Arc<dyn LlmBridge>, system_prompt: impl Into<String>) -> Self {
        Self {
            bridge,
            providers: Vec::new(),
            settings_repo: None,
            llm_provider_repo: None,
            llm_provider_credential_repo: None,
            llm_secret_codec: None,
            system_prompt: system_prompt.into(),
            agents: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn base_system_prompt(&self) -> &str {
        &self.system_prompt
    }

    pub fn default_bridge(&self) -> Arc<dyn LlmBridge> {
        self.bridge.clone()
    }

    pub fn set_settings_repository(&mut self, settings_repo: Arc<dyn SettingsRepository>) {
        self.settings_repo = Some(settings_repo);
    }

    pub fn set_llm_provider_repository(&mut self, repo: Arc<dyn LlmProviderRepository>) {
        self.llm_provider_repo = Some(repo);
    }

    pub fn set_llm_provider_credential_repository(
        &mut self,
        repo: Arc<dyn LlmProviderCredentialRepository>,
    ) {
        self.llm_provider_credential_repo = Some(repo);
    }

    pub fn set_llm_secret_codec(&mut self, codec: Arc<dyn LlmSecretCodec>) {
        self.llm_secret_codec = Some(codec);
    }

    pub(crate) fn add_provider(&mut self, provider: ProviderEntry) {
        self.providers.push(provider);
    }

    pub(crate) fn provider_count(&self) -> usize {
        self.providers.len()
    }

    async fn load_provider_runtime_state(
        &self,
        identity: Option<&agentdash_spi::AuthIdentity>,
    ) -> ProviderRuntimeState {
        if let (Some(llm_provider_repo), Some(secret_codec)) =
            (&self.llm_provider_repo, &self.llm_secret_codec)
        {
            let catalog = build_effective_profile_catalog_from_db(
                llm_provider_repo.as_ref(),
                self.llm_provider_credential_repo
                    .as_ref()
                    .map(|repo| repo.as_ref()),
                secret_codec.as_ref(),
                identity,
            )
            .await;
            let available = catalog.available_entries();
            let default_model = available
                .first()
                .map(|provider| provider.entry.default_model.clone());
            let default_bridge = available
                .first()
                .map(|provider| provider.default_bridge.clone());
            let unavailable_providers = catalog
                .unavailable_entries()
                .into_iter()
                .map(|provider| (provider.provider_id, provider.reason))
                .collect();
            return ProviderRuntimeState {
                uses_dynamic_provider_catalog: true,
                default_bridge,
                default_model,
                profiles: catalog.providers,
                providers: available
                    .into_iter()
                    .map(|provider| provider.entry)
                    .collect(),
                unavailable_providers,
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
                uses_dynamic_provider_catalog: false,
                default_bridge: Some(self.bridge.clone()),
                default_model,
                profiles: Vec::new(),
                providers: self.providers.clone(),
                unavailable_providers: HashMap::new(),
            };
        }

        ProviderRuntimeState {
            uses_dynamic_provider_catalog: false,
            default_bridge: None,
            default_model: None,
            profiles: Vec::new(),
            providers: Vec::new(),
            unavailable_providers: HashMap::new(),
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
        let provider_id = provider_id.map(str::trim).filter(|item| !item.is_empty());
        let model_id = model_id.map(str::trim).filter(|item| !item.is_empty());

        if let Some(provider_id) = provider_id
            && let Some(provider) = provider_state
                .providers
                .iter()
                .find(|provider| provider.provider_id == provider_id)
        {
            let resolved_model = provider.resolve_model(model_id).await.map_err(|error| {
                ConnectorError::InvalidConfig(describe_provider_model_error(provider_id, &error))
            })?;
            return Ok(provider.create_bridge(&resolved_model.id));
        }

        if let Some(provider_id) = provider_id
            && let Some(reason) = provider_state.unavailable_providers.get(provider_id)
        {
            return Err(ConnectorError::InvalidConfig(
                describe_unavailable_provider(provider_id, reason),
            ));
        }

        if provider_id.is_none() && model_id.is_none() {
            if provider_state.uses_dynamic_provider_catalog {
                return Err(ConnectorError::InvalidConfig(
                    "本次 Pi Agent 调用缺少模型选择：请先在模型选择器中选择可用的 Provider/Model，再重新发送"
                        .to_string(),
                ));
            }

            if let Some(provider) = provider_state.providers.first() {
                let resolved_model = provider.resolve_model(None).await.map_err(|error| {
                    ConnectorError::InvalidConfig(describe_provider_model_error(
                        &provider.provider_id,
                        &error,
                    ))
                })?;
                return Ok(provider.create_bridge(&resolved_model.id));
            }

            return provider_state.default_bridge.clone().ok_or_else(|| {
                ConnectorError::InvalidConfig(
                    "Pi Agent 尚未配置任何可用的 LLM Provider".to_string(),
                )
            });
        }

        if let Some(provider_id) = provider_id {
            return Err(ConnectorError::InvalidConfig(format!(
                "LLM Provider `{provider_id}` 不存在、已禁用或未向当前执行环境注册"
            )));
        }

        if let Some(model_id) = model_id {
            let mut matches = Vec::new();
            let mut blocked_providers = Vec::new();
            for provider in &provider_state.providers {
                match provider.resolve_model(Some(model_id)).await {
                    Ok(model) => matches.push((provider, model.id)),
                    Err(ProviderModelResolveError::BlockedModel { .. }) => {
                        blocked_providers.push(provider.provider_id.clone());
                    }
                    Err(ProviderModelResolveError::UnknownModel { .. }) => {}
                    Err(ProviderModelResolveError::EmptyModelSelection) => {}
                }
            }

            if matches.len() == 1 {
                let (provider, resolved_model_id) = matches.remove(0);
                return Ok(provider.create_bridge(&resolved_model_id));
            }

            if matches.len() > 1 {
                let provider_ids = matches
                    .iter()
                    .map(|(provider, _)| provider.provider_id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(ConnectorError::InvalidConfig(format!(
                    "模型 `{model_id}` 同时存在于多个 LLM Provider（{provider_ids}），请明确指定 provider_id"
                )));
            }

            if !blocked_providers.is_empty() {
                return Err(ConnectorError::InvalidConfig(format!(
                    "模型 `{model_id}` 已被 LLM Provider（{}）屏蔽，不能用于本次调用",
                    blocked_providers.join(", ")
                )));
            }

            return Err(ConnectorError::InvalidConfig(format!(
                "模型 `{model_id}` 不存在于任何当前可用的 LLM Provider"
            )));
        }

        Err(ConnectorError::InvalidConfig(
            "Pi Agent 尚未配置任何可用的 LLM Provider".to_string(),
        ))
    }
}

fn normalize_model_selector_value(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToString::to_string)
}

fn describe_unavailable_provider(provider_id: &str, reason: &ProviderUnavailableReason) -> String {
    match reason {
        ProviderUnavailableReason::MissingCredential {
            credential_mode,
            has_identity,
        } => match credential_mode {
            agentdash_domain::llm_provider::LlmCredentialMode::GlobalOnly => format!(
                "LLM Provider `{provider_id}` 当前是仅平台全局 Key 模式，但尚未配置可用的平台凭据"
            ),
            agentdash_domain::llm_provider::LlmCredentialMode::GlobalOrUser => format!(
                "LLM Provider `{provider_id}` 当前是平台全局 Key 或用户 BYOK 模式，但当前没有可用的平台凭据，也没有可用的个人 BYOK 凭据"
            ),
            agentdash_domain::llm_provider::LlmCredentialMode::UserRequired => {
                if *has_identity {
                    format!(
                        "当前身份没有可用的 LLM Provider `{provider_id}` 个人 BYOK 凭据，请在个人 BYOK 设置中补齐"
                    )
                } else {
                    format!(
                        "LLM Provider `{provider_id}` 当前必须使用用户 BYOK，但本次执行没有用户身份，无法读取个人凭据"
                    )
                }
            }
        },
        ProviderUnavailableReason::Disabled => {
            format!("LLM Provider `{provider_id}` 已禁用，不能用于本次调用")
        }
        ProviderUnavailableReason::CredentialResolutionFailed(error) => {
            format!("LLM Provider `{provider_id}` 凭据解析失败: {error}")
        }
        ProviderUnavailableReason::InvalidWireApi(error) => {
            format!("LLM Provider `{provider_id}` wire_api 配置错误: {error}")
        }
        ProviderUnavailableReason::InvalidModels => {
            format!("LLM Provider `{provider_id}` models 配置无法解析")
        }
        ProviderUnavailableReason::InvalidBlockedModels => {
            format!("LLM Provider `{provider_id}` blocked_models 配置无法解析")
        }
    }
}

fn describe_provider_model_error(provider_id: &str, error: &ProviderModelResolveError) -> String {
    match error {
        ProviderModelResolveError::EmptyModelSelection => {
            format!("LLM Provider `{provider_id}` 没有配置可用于调用的默认模型")
        }
        ProviderModelResolveError::UnknownModel { model_id } => format!(
            "LLM Provider `{provider_id}` 不包含模型 `{model_id}`，请从当前可用模型列表中选择"
        ),
        ProviderModelResolveError::BlockedModel { model_id } => {
            format!("LLM Provider `{provider_id}` 的模型 `{model_id}` 已被屏蔽，不能用于本次调用")
        }
    }
}

use super::slash_commands::discover_skill_slash_commands;

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
            supports_steering: true,
            supports_discovery: true,
            supports_variants: false,
            supports_model_override: true,
            supports_permission_policy: false,
            supports_source_session_title: false,
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
        self.discover_options_stream_with_context(
            _executor,
            DiscoveryContext {
                working_dir: _working_dir,
                identity: None,
            },
        )
        .await
    }

    async fn discover_options_stream_with_context(
        &self,
        _executor: &str,
        context: DiscoveryContext,
    ) -> Result<BoxStream<'static, json_patch::Patch>, ConnectorError> {
        let provider_state = self
            .load_provider_runtime_state(context.identity.as_ref())
            .await;
        let mut all_providers: Vec<serde_json::Value> = vec![];
        let mut all_models: Vec<serde_json::Value> = vec![];

        if !provider_state.profiles.is_empty() {
            for profile in provider_state
                .profiles
                .iter()
                .filter(|profile| profile.executable)
            {
                let Some(call_profile) = profile.call_profile.as_ref() else {
                    continue;
                };
                all_providers.push(serde_json::json!({
                    "id": profile.provider.slug,
                    "name": profile.provider.name,
                    "credential_mode": call_profile.credential_mode.as_str(),
                    "credential_source": call_profile.credential_source.as_str(),
                    "protocol": call_profile.protocol.as_str(),
                    "base_url": call_profile.base_url.clone(),
                    "discovery_url": call_profile.discovery_url.clone(),
                    "resolved_wire_api": call_profile.resolved_wire_api.clone(),
                    "discovery_status": profile.discovery_status.kind(),
                    "discovery_message": profile.discovery_status.message(),
                }));

                for model in &profile.models {
                    all_models.push(serde_json::json!({
                        "id": model.id,
                        "name": model.name,
                        "provider_id": profile.provider.slug,
                        "reasoning": model.reasoning,
                        "supports_image": model.supports_image,
                        "context_window": model.context_window,
                        "blocked": model.blocked,
                        "discovered": model.discovered,
                        "source": model.source.as_str(),
                    }));
                }
            }
        } else {
            for provider in &provider_state.providers {
                let call_profile = provider.call_profile();
                all_providers.push(serde_json::json!({
                    "id": provider.provider_id,
                    "name": provider.provider_name,
                    "credential_mode": call_profile.credential_mode.as_str(),
                    "credential_source": call_profile.credential_source.as_str(),
                    "protocol": call_profile.protocol.as_str(),
                    "base_url": call_profile.base_url.clone(),
                    "discovery_url": call_profile.discovery_url.clone(),
                    "resolved_wire_api": call_profile.resolved_wire_api.clone(),
                }));

                for model in provider.load_models_with_block_state().await {
                    all_models.push(serde_json::json!({
                        "id": model.id,
                        "name": model.name,
                        "provider_id": provider.provider_id,
                        "reasoning": model.reasoning,
                        "supports_image": model.supports_image,
                        "context_window": model.context_window,
                        "blocked": model.blocked,
                        "discovered": model.discovered,
                        "source": model.source.as_str(),
                    }));
                }
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

        let default_model = provider_state
            .profiles
            .iter()
            .find(|profile| profile.executable)
            .and_then(|profile| profile.default_model.clone())
            .or_else(|| provider_state.default_model.clone());

        // 从工作目录扫描 skill，注册为 slash commands
        let slash_commands: Vec<serde_json::Value> = context
            .working_dir
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
        // 统一映射：结构化 UserInput -> ContentPart（图片直达 ContentPart::Image，不再拍平成文本）。
        // `to_fallback_text` 保留仅供标题/trace 摘要，不再作为投递路径。
        let prompt_parts = prompt.to_content_parts();
        if prompt_parts.is_empty() {
            return Err(ConnectorError::InvalidConfig("prompt 内容为空".to_string()));
        }
        let restored_state = context.turn.restored_session_state.as_ref().cloned();

        let existing_runtime = {
            let mut agents = self.agents.lock().await;
            agents.remove(session_id)
        };

        let requested_model_selection =
            PiAgentModelSelection::from_config(&context.session.executor_config);
        let should_recreate_agent = existing_runtime
            .as_ref()
            .is_some_and(|runtime| runtime.model_selection != requested_model_selection);
        let is_new_agent = existing_runtime.is_none();
        let incoming_identity_prompt = extract_identity_prompt(&context.turn.context_frames);
        let mut cached_identity_prompt = existing_runtime
            .as_ref()
            .and_then(|runtime| runtime.last_identity_prompt.clone());
        let mut current_tools: Vec<DynAgentTool> = Vec::new();
        let mut agent = if let Some(runtime) = existing_runtime {
            if should_recreate_agent {
                let preserved_messages = runtime.agent.messages().await;
                let preserved_message_refs = runtime.agent.message_refs().await;
                let provider_state = self
                    .load_provider_runtime_state(context.session.identity.as_ref())
                    .await;
                let bridge = self
                    .resolve_bridge_for_execution(
                        &provider_state,
                        context.session.executor_config.provider_id.as_deref(),
                        context.session.executor_config.model_id.as_deref(),
                    )
                    .await?;
                let agent = self.create_agent_with_bridge(bridge);
                agent
                    .replace_messages_with_refs(preserved_messages, preserved_message_refs)
                    .await;
                current_tools = context.turn.assembled_tools.clone();
                tracing::info!(
                    session_id = %session_id,
                    provider_id = ?requested_model_selection.provider_id,
                    model_id = ?requested_model_selection.model_id,
                    "Pi Agent 模型选择变化，已重建 session agent bridge"
                );
                agent
            } else {
                current_tools = runtime.tools;
                runtime.agent
            }
        } else {
            let provider_state = self
                .load_provider_runtime_state(context.session.identity.as_ref())
                .await;
            let bridge = self
                .resolve_bridge_for_execution(
                    &provider_state,
                    context.session.executor_config.provider_id.as_deref(),
                    context.session.executor_config.model_id.as_deref(),
                )
                .await?;
            self.create_agent_with_bridge(bridge)
        };

        if is_new_agent || should_recreate_agent {
            if is_new_agent {
                current_tools = context.turn.assembled_tools.clone();
            }

            if let Some(system_prompt) = incoming_identity_prompt
                .as_ref()
                .or(cached_identity_prompt.as_ref())
            {
                agent.set_system_prompt(system_prompt.clone());
                cached_identity_prompt = Some(system_prompt.clone());
            }
            agent.set_tools(current_tools.clone());
            if let Some(state) = restored_state.filter(|state| !state.messages.is_empty()) {
                agent
                    .replace_messages_with_refs(state.messages, state.message_refs)
                    .await;
            }
        } else if incoming_identity_prompt.as_deref() != cached_identity_prompt.as_deref()
            && let Some(system_prompt) = incoming_identity_prompt.as_ref()
        {
            agent.set_system_prompt(system_prompt.clone());
            cached_identity_prompt = Some(system_prompt.clone());
        }

        let hook_trace_rx = context
            .turn
            .hook_runtime
            .as_ref()
            .and_then(|hs| hs.subscribe_traces());
        agent.set_runtime_delegate(context.turn.runtime_delegate.clone());

        if let Some(thinking_level) = context.session.executor_config.thinking_level {
            agent.set_thinking_level(thinking_level);
        }

        let (event_rx, join_handle) = agent
            .prompt(AgentMessage::user_parts(prompt_parts))
            .map_err(|error| ConnectorError::Runtime(format!("Pi Agent 启动失败: {error}")))?;

        let session_id_owned = session_id.to_string();
        self.agents.lock().await.insert(
            session_id_owned.clone(),
            PiAgentSessionRuntime {
                agent,
                tools: current_tools,
                last_identity_prompt: cached_identity_prompt,
                model_selection: requested_model_selection,
            },
        );

        let source = SourceInfo {
            connector_id: self.connector_id().to_string(),
            connector_type: "local_executor".to_string(),
            executor_id: Some("PI_AGENT".to_string()),
        };
        let turn_id = context.session.turn_id.clone();
        let session_id_owned = session_id.to_string();

        let (tx, rx) = tokio::sync::mpsc::channel::<Result<BackboneEnvelope, ConnectorError>>(8192);

        tokio::spawn(async move {
            let mut entry_index: u32 = 0;
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
                            let envelopes = convert_event_to_envelopes(
                                &event,
                                &session_id_owned,
                                &source,
                                &turn_id,
                                &mut entry_index,
                                &mut chunk_emit_states,
                                &mut tool_call_states,
                            );

                            for e in envelopes {
                                if tx.send(Ok(e)).await.is_err() {
                                    return;
                                }
                            }
                        }
                        trace_result = receiver.recv() => {
                            if let Ok(entry) = trace_result {
                                let envelope = build_hook_trace_envelope(
                                    &session_id_owned,
                                    Some(&turn_id),
                                    source.clone(),
                                    &entry,
                                );
                                if tx.send(Ok(envelope)).await.is_err() {
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

                let envelopes = convert_event_to_envelopes(
                    &event,
                    &session_id_owned,
                    &source,
                    &turn_id,
                    &mut entry_index,
                    &mut chunk_emit_states,
                    &mut tool_call_states,
                );

                for e in envelopes {
                    if tx.send(Ok(e)).await.is_err() {
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

            emit_pending_hook_trace_envelopes(
                &mut hook_trace_rx,
                &tx,
                &session_id_owned,
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

    async fn update_session_tools(
        &self,
        session_id: &str,
        tools: Vec<DynAgentTool>,
    ) -> Result<(), ConnectorError> {
        let mut agents = self.agents.lock().await;
        let runtime = agents.get_mut(session_id).ok_or_else(|| {
            ConnectorError::Runtime(format!(
                "session `{session_id}` 当前没有活跃的 Pi Agent，无法热更新工具"
            ))
        })?;

        let old_names: BTreeSet<String> = runtime
            .tools
            .iter()
            .map(|tool| tool.name().to_string())
            .collect();
        let new_names: BTreeSet<String> =
            tools.iter().map(|tool| tool.name().to_string()).collect();

        let tool_count = tools.len();
        runtime.tools = tools.clone();
        runtime.agent.set_tools(tools);

        let added: Vec<String> = new_names.difference(&old_names).cloned().collect();
        let removed: Vec<String> = old_names.difference(&new_names).cloned().collect();

        tracing::info!(
            session_id = %session_id,
            added = ?added,
            removed = ?removed,
            tool_count = tool_count,
            "工具热更新完成（replace-set）"
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

    async fn steer_session(
        &self,
        session_id: &str,
        _expected_turn_id: &str,
        input: Vec<agentdash_agent_protocol::UserInputBlock>,
    ) -> Result<(), ConnectorError> {
        // 统一映射：结构化 UserInput -> ContentPart（图片直达 ContentPart::Image，不再拍平）。
        let parts = user_input_blocks_to_content_parts(&input);
        if parts.is_empty() {
            return Err(ConnectorError::InvalidConfig(
                "steer 输入中没有可投递内容".to_string(),
            ));
        }
        let agents = self.agents.lock().await;
        let runtime = agents.get(session_id).ok_or_else(|| {
            ConnectorError::Runtime(format!(
                "session `{session_id}` 当前没有活跃的 Pi Agent，无法运行中 steer"
            ))
        })?;
        runtime.agent.steer(AgentMessage::user_parts(parts)).await;
        Ok(())
    }
}

fn extract_identity_prompt(frames: &[ContextFrame]) -> Option<String> {
    let identity_frame = frames.iter().find(|frame| frame.kind == "identity")?;
    if let Some(prompt) = identity_frame
        .sections
        .iter()
        .find_map(|section| match section {
            ContextFrameSection::Identity {
                effective_prompt, ..
            } => {
                let prompt = effective_prompt.trim();
                (!prompt.is_empty()).then(|| prompt.to_string())
            }
            _ => None,
        })
    {
        return Some(prompt);
    }
    let rendered = identity_frame.rendered_text.trim();
    (!rendered.is_empty()).then(|| rendered.to_string())
}

async fn emit_pending_hook_trace_envelopes(
    hook_trace_rx: &mut Option<tokio::sync::broadcast::Receiver<agentdash_spi::HookTraceEntry>>,
    tx: &tokio::sync::mpsc::Sender<Result<BackboneEnvelope, ConnectorError>>,
    session_id: &str,
    source: &SourceInfo,
    turn_id: &str,
) {
    let Some(receiver) = hook_trace_rx.as_mut() else {
        return;
    };

    while let Ok(entry) = receiver.try_recv() {
        let envelope = build_hook_trace_envelope(session_id, Some(turn_id), source.clone(), &entry);
        if tx.send(Ok(envelope)).await.is_err() {
            return;
        }
    }
}

use super::stream_mapper::{ChunkEmitState, ToolCallEmitState, convert_event_to_envelopes};

#[cfg(test)]
#[path = "connector_tests.rs"]
mod tests;
