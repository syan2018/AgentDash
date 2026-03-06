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
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;

use agentdash_acp_meta::{
    AgentDashMetaV1, AgentDashSourceV1, AgentDashTraceV1, merge_agentdash_meta,
};

use agentdash_agent::{Agent, AgentConfig, AgentEvent, AgentMessage, DynAgentTool, LlmBridge};

use crate::connector::{
    AgentConnector, ConnectorCapabilities, ConnectorError, ConnectorType, ExecutionContext,
    ExecutionStream, ExecutorInfo, PromptPayload,
};

pub struct PiAgentConnector {
    #[allow(dead_code)]
    workspace_root: PathBuf,
    bridge: Arc<dyn LlmBridge>,
    tools: Vec<DynAgentTool>,
    system_prompt: String,
    agents: Arc<Mutex<HashMap<String, Agent>>>,
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
            tools: Vec::new(),
            system_prompt: system_prompt.into(),
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
            supports_discovery: false,
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
        Err(ConnectorError::Runtime(
            "Pi Agent 不支持 discover_options".to_string(),
        ))
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
            return Err(ConnectorError::InvalidConfig(
                "prompt 内容为空".to_string(),
            ));
        }

        let mut agent = {
            let mut agents = self.agents.lock().await;
            agents
                .remove(session_id)
                .unwrap_or_else(|| self.create_agent())
        };

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
                        agent.replace_messages(messages);
                    }
                }
                Ok(Err(e)) => {
                    tracing::error!("Pi Agent loop 错误: {e}");
                }
                Err(e) => {
                    tracing::error!("Pi Agent task panic: {e}");
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
        AgentEvent::MessageEnd { message } => {
            if let AgentMessage::Assistant { content, .. } = message {
                let text = content
                    .iter()
                    .filter_map(|p| p.extract_text())
                    .collect::<Vec<_>>()
                    .join("");

                if text.is_empty() {
                    return Vec::new();
                }

                let meta = make_meta(source, turn_id, *entry_index);
                *entry_index += 1;

                let chunk =
                    ContentChunk::new(ContentBlock::Text(TextContent::new(&text))).meta(Some(meta));
                vec![SessionNotification::new(
                    session_id.clone(),
                    SessionUpdate::AgentMessageChunk(chunk),
                )]
            } else {
                Vec::new()
            }
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
