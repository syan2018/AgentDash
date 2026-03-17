/// PiAgentConnector — 基于 agentdash-agent 的进程内 Agent 连接器
///
/// 与 `VibeKanbanExecutorsConnector`（通过子进程执行）不同，
/// PiAgentConnector 在进程内运行 Agent Loop，直接调用 LLM API。
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use agent_client_protocol::{
    ContentBlock, ContentChunk, SessionId, SessionNotification, SessionUpdate, TextContent,
    ToolCall, ToolCallContent, ToolCallId, ToolCallStatus, ToolCallUpdate, ToolCallUpdateFields,
    ToolKind,
};
use futures::stream::BoxStream;
use tokio::sync::Mutex;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;

use agentdash_acp_meta::{
    AgentDashMetaV1, AgentDashSourceV1, AgentDashTraceV1, merge_agentdash_meta,
};

use agentdash_agent::{
    Agent, AgentConfig, AgentEvent, AgentMessage, BuiltinToolset, DynAgentTool, LlmBridge,
};

use crate::connector::{
    AgentConnector, ConnectorCapabilities, ConnectorError, ConnectorType, ExecutionContext,
    ExecutionStream, ExecutorInfo, PromptPayload,
};
use crate::connectors::pi_agent_mcp::discover_mcp_tools;

pub struct PiAgentConnector {
    #[allow(dead_code)]
    workspace_root: PathBuf,
    bridge: Arc<dyn LlmBridge>,
    tools: Vec<DynAgentTool>,
    system_prompt: String,
    model_id: String,
    agents: Arc<Mutex<HashMap<String, Agent>>>,
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
            tools: Vec::new(),
            system_prompt: system_prompt.into(),
            model_id: model_id.into(),
            agents: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn add_tool(&mut self, tool: DynAgentTool) {
        self.tools.push(tool);
    }

    fn create_agent(&self) -> Agent {
        let config = AgentConfig {
            system_prompt: self.system_prompt.clone(),
            max_tokens: Some(8192),
            ..AgentConfig::default()
        };
        let mut agent = Agent::new(self.bridge.clone(), config);
        agent.set_tools(self.tools.clone());
        agent
    }

    fn build_runtime_system_prompt(
        &self,
        context: &ExecutionContext,
        tool_names: &[String],
    ) -> String {
        let mut sections = vec![self.system_prompt.clone()];
        sections.push(format!(
            "工作空间根目录：{}\n当前工作目录：{}",
            self.workspace_root.display(),
            context.working_directory.display()
        ));

        if !tool_names.is_empty() {
            sections.push(format!(
                "你当前可调用的内置工具有：{}。优先使用工具读取/搜索/执行，不要臆测文件内容。",
                tool_names.join("、")
            ));
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

        sections.join("\n\n")
    }
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
            supports_model_override: false,
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
        let model_id = self.model_id.clone();

        let patch: json_patch::Patch = serde_json::from_value(serde_json::json!([
            { "op": "replace", "path": "/options/model_selector/providers", "value": [{ "id": "openai", "name": "OpenAI Compatible" }] },
            { "op": "replace", "path": "/options/model_selector/models", "value": [{ "id": model_id, "name": model_id, "provider_id": "openai", "reasoning_options": [] }] },
            { "op": "replace", "path": "/options/model_selector/default_model", "value": model_id },
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

        let mut agent = {
            let mut agents = self.agents.lock().await;
            agents
                .remove(session_id)
                .unwrap_or_else(|| self.create_agent())
        };

        let builtin_tools =
            BuiltinToolset::for_workspace(context.working_directory.clone()).into_tools();
        let mcp_tools = match discover_mcp_tools(&context.mcp_servers).await {
            Ok(tools) => tools,
            Err(error) => {
                tracing::warn!("发现 MCP 工具失败，继续使用本地工具: {error}");
                Vec::new()
            }
        };
        let mut runtime_tools = self.tools.clone();
        runtime_tools.extend(builtin_tools);
        runtime_tools.extend(mcp_tools);
        let tool_names = runtime_tools
            .iter()
            .map(|tool| tool.name().to_string())
            .collect::<Vec<_>>();
        agent.set_tools(runtime_tools);
        agent.set_system_prompt(self.build_runtime_system_prompt(&context, &tool_names));

        let (event_rx, join_handle) = agent.prompt(AgentMessage::user(&prompt_text));

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

        let agents = self.agents.clone();

        tokio::spawn(async move {
            let mut entry_index: u32 = 0;
            let mut event_rx = event_rx;

            while let Some(event) = event_rx.next().await {
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
                Ok(Ok(messages)) => {
                    if let Some(agent) = agents.lock().await.get_mut(&session_id_owned) {
                        agent.replace_messages(messages).await;
                    }
                }
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
        });

        Ok(Box::pin(ReceiverStream::new(rx)))
    }

    async fn cancel(&self, session_id: &str) -> Result<(), ConnectorError> {
        if let Some(agent) = self.agents.lock().await.get(session_id) {
            agent.abort();
        }
        Ok(())
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

fn convert_event_to_notifications(
    event: &AgentEvent,
    session_id: &SessionId,
    source: &AgentDashSourceV1,
    turn_id: &str,
    entry_index: &mut u32,
) -> Vec<SessionNotification> {
    match event {
        AgentEvent::MessageUpdate { event, .. } => {
            match event {
                agentdash_agent::types::AssistantStreamEvent::TextDelta { text } => {
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
                _ => Vec::new(),
            }
        }

        AgentEvent::MessageEnd { message } => {
            // MessageEnd 时不再发送全量文本（已通过 MessageUpdate 增量推送），
            // 仅递增 entry_index 用于后续条目排序
            if let AgentMessage::Assistant { content, .. } = message {
                let has_text = content.iter().any(|p| p.extract_text().is_some());
                if has_text {
                    *entry_index += 1;
                }
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
            fields.content = Some(vec![ToolCallContent::from(ContentBlock::Text(
                TextContent::new(&result_text),
            ))]);
            fields.raw_output = Some(result.clone());

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
