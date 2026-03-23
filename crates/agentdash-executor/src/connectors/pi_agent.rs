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

use crate::connector::{
    AgentConnector, ConnectorCapabilities, ConnectorError, ConnectorType, ExecutionContext,
    ExecutionMount, ExecutionMountCapability, ExecutionStream, ExecutorInfo, PromptPayload,
    RuntimeToolProvider,
};
use crate::connectors::pi_agent_mcp::discover_mcp_tools;
use crate::hook_events::build_hook_trace_notification;
use crate::runtime_delegate::HookRuntimeDelegate;

pub struct PiAgentConnector {
    #[allow(dead_code)]
    workspace_root: PathBuf,
    bridge: Arc<dyn LlmBridge>,
    tools: Vec<DynAgentTool>,
    runtime_tool_provider: Option<Arc<dyn RuntimeToolProvider>>,
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
            runtime_tool_provider: None,
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
    let snapshot = hook_session.snapshot();
    let mut sections = Vec::new();

    if !snapshot.owners.is_empty() {
        let owner_lines = snapshot
            .owners
            .iter()
            .map(|owner| {
                let label = owner.label.as_deref().unwrap_or(owner.owner_id.as_str());
                format!("- {}: {}", owner.owner_type, label)
            })
            .collect::<Vec<_>>()
            .join("\n");
        sections.push(format!("当前会话 Hook 归属如下：\n{}", owner_lines));
    }

    if !snapshot.tags.is_empty() {
        sections.push(format!("当前 Hook tags：{}", snapshot.tags.join("、")));
    }

    if !snapshot.constraints.is_empty() {
        sections.push(format!(
            "当前存在 {} 条运行时流程约束，详细内容会由 Hook Runtime 在每次 LLM 调用前动态注入。",
            snapshot.constraints.len()
        ));
    }

    let diagnostics = hook_session.diagnostics();
    if !diagnostics.is_empty() {
        sections.push(format!(
            "当前已记录 {} 条 Hook 诊断信息，前端可进一步查看细节。",
            diagnostics.len()
        ));
    }

    if !snapshot.context_fragments.is_empty() {
        sections.push(format!(
            "当前存在 {} 个可动态注入的 Hook context fragment。",
            snapshot.context_fragments.len()
        ));
    }

    sections.push(format!(
        "Hook runtime revision: {}",
        hook_session.revision()
    ));

    if !snapshot.constraints.is_empty() {
        sections.push(format!(
            "## Hook Constraint Summary\n{}",
            snapshot
                .constraints
                .iter()
                .map(|constraint| format!("- {}", constraint.description))
                .collect::<Vec<_>>()
                .join("\n")
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
                            if let Some(entry) = maybe_trace {
                                if let Some(notification) = build_hook_trace_notification(
                                    acp_session_id.0.as_ref(),
                                    Some(&turn_id),
                                    source.clone(),
                                    &entry,
                                ) {
                                    if tx.send(Ok(notification)).await.is_err() {
                                        return;
                                    }
                                }
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
        ) {
            if tx.send(Ok(notification)).await.is_err() {
                return;
            }
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

// ─── Factory ────────────────────────────────────────────────────────

/// 从 `SettingsRepository` 和环境变量构建 `PiAgentConnector`。
///
/// 配置优先级：Settings DB > 环境变量 > 默认值。
/// 支持 Anthropic（优先）和 OpenAI Responses API。
pub async fn build_pi_agent_connector(
    workspace_root: &Path,
    settings: &dyn agentdash_domain::settings::SettingsRepository,
) -> Option<PiAgentConnector> {
    use agentdash_agent::{LlmBridge, RigBridge};
    use rig::client::CompletionClient as _;

    let api_key = read_setting_str(settings, "llm.openai.api_key")
        .await
        .or_else(|| std::env::var("OPENAI_API_KEY").ok());
    let base_url = read_setting_str(settings, "llm.openai.base_url")
        .await
        .or_else(|| std::env::var("OPENAI_BASE_URL").ok());
    let model_id = read_setting_str(settings, "llm.openai.default_model")
        .await
        .unwrap_or_else(|| "gpt-4o".to_string());
    let wire_api = read_setting_str(settings, "llm.openai.wire_api")
        .await
        .unwrap_or_else(|| "responses".to_string());

    let system_prompt = read_setting_str(settings, "agent.pi.system_prompt")
        .await
        .or_else(|| std::env::var("PI_AGENT_SYSTEM_PROMPT").ok())
        .unwrap_or_else(|| {
            "你是 AgentDash 内置 AI 助手，一个通用的编程与任务执行 Agent。请用中文回复用户。"
                .to_string()
        });

    // Anthropic（Settings → 环境变量）
    let anthropic_key = read_setting_str(settings, "llm.anthropic.api_key")
        .await
        .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok());

    if let Some(api_key) = anthropic_key {
        let client = rig::providers::anthropic::Client::new(&api_key);
        let anthropic_model = rig::providers::anthropic::completion::CLAUDE_4_SONNET;
        let model = client.completion_model(anthropic_model);
        let bridge: Arc<dyn LlmBridge> = Arc::new(RigBridge::new(model));
        let connector = PiAgentConnector::new(
            workspace_root.to_path_buf(),
            bridge,
            system_prompt,
            anthropic_model,
        );
        tracing::info!("PiAgentConnector 已初始化（Anthropic）");
        return Some(connector);
    }

    // OpenAI/兼容端点
    let api_key = api_key?;

    if wire_api != "responses" {
        tracing::warn!(
            "Rig 发行版当前统一走 Responses API，忽略 llm.openai.wire_api={} 配置",
            wire_api
        );
    }

    let mut builder = rig::providers::openai::Client::builder(&api_key);
    if let Some(ref url) = base_url {
        builder = builder.base_url(url);
    }
    let client = builder.build();
    let model = client.completion_model(&model_id);
    tracing::info!(
        "OpenAI Responses Client 已就绪（base_url={}, model={model_id}）",
        base_url.as_deref().unwrap_or("default")
    );
    let bridge: Arc<dyn LlmBridge> = Arc::new(RigBridge::new(model));
    let connector = PiAgentConnector::new(
        workspace_root.to_path_buf(),
        bridge,
        system_prompt,
        &model_id,
    );
    tracing::info!("PiAgentConnector 已初始化（OpenAI 兼容）");
    Some(connector)
}

async fn read_setting_str(
    repo: &dyn agentdash_domain::settings::SettingsRepository,
    key: &str,
) -> Option<String> {
    repo.get(key)
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

    fn test_source() -> AgentDashSourceV1 {
        AgentDashSourceV1::new("pi-agent", "local_executor")
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
            executor_config: crate::connector::AgentDashExecutorConfig::new("PI_AGENT"),
            mcp_servers: vec![],
            address_space: None,
            hook_session: None,
            flow_capabilities: Default::default(),
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

    struct NoopBridge;

    #[async_trait::async_trait]
    impl LlmBridge for NoopBridge {
        async fn stream_complete(
            &self,
            _request: agentdash_agent::BridgeRequest,
        ) -> std::pin::Pin<Box<dyn futures::Stream<Item = agentdash_agent::StreamChunk> + Send>>
        {
            Box::pin(tokio_stream::empty())
        }
    }
}
