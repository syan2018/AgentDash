//! `SessionHub` 行为测试（从原 `hub.rs` 迁移；PR 6 拆分）。

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use agent_client_protocol::{
    ContentBlock, ContentChunk, McpServer, SessionId, SessionInfoUpdate, SessionNotification,
    SessionUpdate, TextContent, ToolCall, ToolCallId, ToolCallStatus, ToolCallUpdate,
    ToolCallUpdateFields,
};
use agentdash_acp_meta::{
    AgentDashEventV1, AgentDashMetaV1, AgentDashSourceV1, AgentDashTraceV1, merge_agentdash_meta,
};
use agentdash_spi::hooks::{
    ExecutionHookProvider, HookEvaluationQuery, HookResolution, HookTrigger, SessionHookRefreshQuery,
    SessionHookSnapshot, SessionHookSnapshotQuery,
};
use agentdash_spi::{AgentConnector, ConnectorError, PromptPayload, StopReason};
use futures::stream;
use serde_json::json;
use tokio::sync::{Mutex as TokioMutex, mpsc};
use tokio_stream::wrappers::ReceiverStream;

use super::super::MemorySessionPersistence;
use super::super::hook_messages as msg;
use super::super::hub_support::{
    build_user_message_notifications, parse_turn_terminal_event,
};
use super::super::local_workspace_vfs;
use super::super::types::{
    HookSnapshotReloadTrigger, PromptSessionRequest, SessionBootstrapState,
    SessionExecutionState, UserPromptInput,
};
use super::SessionHub;

fn test_hub(
    mount_root: PathBuf,
    connector: Arc<dyn AgentConnector>,
    hook_provider: Option<Arc<dyn ExecutionHookProvider>>,
) -> SessionHub {
    SessionHub::new_with_hooks_and_persistence(
        Some(local_workspace_vfs(&mount_root)),
        connector,
        hook_provider,
        Arc::new(MemorySessionPersistence::default()),
    )
}

fn simple_prompt_request(prompt: &str) -> PromptSessionRequest {
    PromptSessionRequest {
        user_input: UserPromptInput {
            executor_config: Some(agentdash_spi::AgentConfig::new("PI_AGENT")),
            ..UserPromptInput::from_text(prompt)
        },
        mcp_servers: vec![],
        relay_mcp_server_names: Default::default(),
        vfs: None,
        flow_capabilities: None,
        effective_capability_keys: None,
        context_bundle: None,
        hook_snapshot_reload: HookSnapshotReloadTrigger::None,
        identity: None,
        post_turn_handler: None,
    }
}

fn owner_bootstrap_request(prompt: &str, system_context: &str) -> PromptSessionRequest {
    let mut req = simple_prompt_request(prompt);
    let bundle_session_id = uuid::Uuid::new_v4();
    req.context_bundle = Some(crate::context::build_continuation_bundle_from_markdown(
        bundle_session_id,
        system_context.to_string(),
    ));
    req.hook_snapshot_reload = HookSnapshotReloadTrigger::Reload;
    req
}

#[derive(Default)]
struct EmptyConnector;

#[async_trait::async_trait]
impl AgentConnector for EmptyConnector {
    fn connector_id(&self) -> &'static str {
        "empty"
    }
    fn connector_type(&self) -> agentdash_spi::ConnectorType {
        agentdash_spi::ConnectorType::LocalExecutor
    }
    fn capabilities(&self) -> agentdash_spi::ConnectorCapabilities {
        agentdash_spi::ConnectorCapabilities::default()
    }
    fn list_executors(&self) -> Vec<agentdash_spi::AgentInfo> {
        Vec::new()
    }
    async fn discover_options_stream(
        &self,
        _executor: &str,
        _working_dir: Option<PathBuf>,
    ) -> Result<futures::stream::BoxStream<'static, json_patch::Patch>, ConnectorError> {
        Ok(Box::pin(stream::empty()))
    }
    async fn prompt(
        &self,
        _session_id: &str,
        _follow_up_session_id: Option<&str>,
        _prompt: &PromptPayload,
        _context: agentdash_spi::ExecutionContext,
    ) -> Result<agentdash_spi::ExecutionStream, ConnectorError> {
        Ok(Box::pin(stream::empty()))
    }
    async fn cancel(&self, _session_id: &str) -> Result<(), ConnectorError> {
        Ok(())
    }
    async fn approve_tool_call(
        &self,
        _session_id: &str,
        _tool_call_id: &str,
    ) -> Result<(), ConnectorError> {
        Ok(())
    }
    async fn reject_tool_call(
        &self,
        _session_id: &str,
        _tool_call_id: &str,
        _reason: Option<String>,
    ) -> Result<(), ConnectorError> {
        Ok(())
    }
}

#[tokio::test]
async fn start_prompt_records_current_turn_state() {
    let base = tempfile::tempdir().expect("tempdir");
    let workspace = tempfile::tempdir().expect("workspace");
    let hub = test_hub(base.path().to_path_buf(), Arc::new(EmptyConnector), None);
    let session = hub.create_session("active-state").await.expect("create");
    let mcp_server = McpServer::Http(agent_client_protocol::McpServerHttp::new(
        "relay_tools",
        "http://127.0.0.1:19090/mcp",
    ));
    let mut relay_names = std::collections::HashSet::new();
    relay_names.insert("relay_tools".to_string());
    let mut effective_keys = std::collections::BTreeSet::new();
    effective_keys.insert("workflow".to_string());
    let flow_caps =
        agentdash_spi::FlowCapabilities::from_clusters([agentdash_spi::ToolCluster::Workflow]);

    let mut req = simple_prompt_request("hello");
    req.vfs = Some(local_workspace_vfs(workspace.path()));
    req.user_input.working_dir = Some("src".to_string());
    req.mcp_servers = vec![mcp_server.clone()];
    req.relay_mcp_server_names = relay_names.clone();
    req.flow_capabilities = Some(flow_caps.clone());
    req.effective_capability_keys = Some(effective_keys.clone());

    hub.start_prompt(&session.id, req)
        .await
        .expect("prompt should start");

    let sessions = hub.sessions.lock().await;
    let turn = sessions
        .get(&session.id)
        .and_then(|runtime| runtime.current_turn.as_ref())
        .expect("current turn execution state");
    assert_eq!(turn.session_frame.mcp_servers, vec![mcp_server]);
    assert_eq!(turn.relay_mcp_server_names, relay_names);
    assert_eq!(
        turn.session_frame.working_directory,
        workspace.path().join("src")
    );
    assert_eq!(turn.session_frame.executor_config.executor, "PI_AGENT");
    assert_eq!(
        turn.flow_capabilities.enabled_clusters,
        flow_caps.enabled_clusters
    );
}

fn test_meta(
    source: &AgentDashSourceV1,
    turn_id: &str,
    entry_index: u32,
) -> agent_client_protocol::Meta {
    let mut trace = AgentDashTraceV1::new();
    trace.turn_id = Some(turn_id.to_string());
    trace.entry_index = Some(entry_index);

    merge_agentdash_meta(
        None,
        &AgentDashMetaV1::new()
            .source(Some(source.clone()))
            .trace(Some(trace)),
    )
    .expect("test meta should build")
}

fn test_event_meta(
    source: &AgentDashSourceV1,
    turn_id: &str,
    entry_index: u32,
    event_type: &str,
    data: serde_json::Value,
) -> agent_client_protocol::Meta {
    let mut trace = AgentDashTraceV1::new();
    trace.turn_id = Some(turn_id.to_string());
    trace.entry_index = Some(entry_index);

    let mut event = AgentDashEventV1::new(event_type);
    event.severity = Some("info".to_string());
    event.data = Some(data);

    merge_agentdash_meta(
        None,
        &AgentDashMetaV1::new()
            .source(Some(source.clone()))
            .trace(Some(trace))
            .event(Some(event)),
    )
    .expect("test event meta should build")
}

#[derive(Default)]
struct SessionStartAwareConnector {
    session_start_seen: Arc<TokioMutex<Vec<bool>>>,
}

#[async_trait::async_trait]
impl AgentConnector for SessionStartAwareConnector {
    fn connector_id(&self) -> &'static str {
        "session-start-aware"
    }
    fn connector_type(&self) -> agentdash_spi::ConnectorType {
        agentdash_spi::ConnectorType::LocalExecutor
    }
    fn capabilities(&self) -> agentdash_spi::ConnectorCapabilities {
        agentdash_spi::ConnectorCapabilities::default()
    }
    fn list_executors(&self) -> Vec<agentdash_spi::AgentInfo> {
        Vec::new()
    }
    async fn discover_options_stream(
        &self,
        _executor: &str,
        _working_dir: Option<PathBuf>,
    ) -> Result<futures::stream::BoxStream<'static, json_patch::Patch>, ConnectorError> {
        Ok(Box::pin(stream::empty()))
    }

    async fn prompt(
        &self,
        _session_id: &str,
        _follow_up_session_id: Option<&str>,
        _prompt: &PromptPayload,
        context: agentdash_spi::ExecutionContext,
    ) -> Result<agentdash_spi::ExecutionStream, ConnectorError> {
        let seen = context.turn.hook_session.as_ref().is_some_and(|runtime| {
            runtime
                .trace()
                .iter()
                .any(|trace| matches!(&trace.trigger, HookTrigger::SessionStart))
        });
        self.session_start_seen.lock().await.push(seen);
        Ok(Box::pin(stream::empty()))
    }

    async fn cancel(&self, _session_id: &str) -> Result<(), ConnectorError> {
        Ok(())
    }
    async fn approve_tool_call(
        &self,
        _session_id: &str,
        _tool_call_id: &str,
    ) -> Result<(), ConnectorError> {
        Ok(())
    }
    async fn reject_tool_call(
        &self,
        _session_id: &str,
        _tool_call_id: &str,
        _reason: Option<String>,
    ) -> Result<(), ConnectorError> {
        Ok(())
    }
}

struct RecordingHookProvider {
    queries: Arc<TokioMutex<Vec<HookEvaluationQuery>>>,
}

#[async_trait::async_trait]
impl ExecutionHookProvider for RecordingHookProvider {
    async fn load_session_snapshot(
        &self,
        query: SessionHookSnapshotQuery,
    ) -> Result<SessionHookSnapshot, agentdash_spi::hooks::HookError> {
        Ok(SessionHookSnapshot {
            session_id: query.session_id,
            ..SessionHookSnapshot::default()
        })
    }
    async fn refresh_session_snapshot(
        &self,
        query: SessionHookRefreshQuery,
    ) -> Result<SessionHookSnapshot, agentdash_spi::hooks::HookError> {
        Ok(SessionHookSnapshot {
            session_id: query.session_id,
            ..SessionHookSnapshot::default()
        })
    }
    async fn evaluate_hook(
        &self,
        query: HookEvaluationQuery,
    ) -> Result<HookResolution, agentdash_spi::hooks::HookError> {
        self.queries.lock().await.push(query);
        Ok(HookResolution::default())
    }
}

#[derive(Default)]
struct RepositoryRestoreRecordingConnector {
    contexts: Arc<TokioMutex<Vec<agentdash_spi::ExecutionContext>>>,
}

#[async_trait::async_trait]
impl AgentConnector for RepositoryRestoreRecordingConnector {
    fn connector_id(&self) -> &'static str {
        "repository-restore-recording"
    }
    fn connector_type(&self) -> agentdash_spi::ConnectorType {
        agentdash_spi::ConnectorType::LocalExecutor
    }
    fn capabilities(&self) -> agentdash_spi::ConnectorCapabilities {
        agentdash_spi::ConnectorCapabilities::default()
    }
    fn supports_repository_restore(&self, executor: &str) -> bool {
        executor == "PI_AGENT"
    }
    fn list_executors(&self) -> Vec<agentdash_spi::AgentInfo> {
        vec![agentdash_spi::AgentInfo {
            id: "PI_AGENT".to_string(),
            name: "Pi Agent".to_string(),
            variants: Vec::new(),
            available: true,
        }]
    }
    async fn discover_options_stream(
        &self,
        _executor: &str,
        _working_dir: Option<PathBuf>,
    ) -> Result<futures::stream::BoxStream<'static, json_patch::Patch>, ConnectorError> {
        Ok(Box::pin(stream::empty()))
    }
    async fn prompt(
        &self,
        _session_id: &str,
        _follow_up_session_id: Option<&str>,
        _prompt: &PromptPayload,
        context: agentdash_spi::ExecutionContext,
    ) -> Result<agentdash_spi::ExecutionStream, ConnectorError> {
        self.contexts.lock().await.push(context);
        Ok(Box::pin(stream::empty()))
    }
    async fn cancel(&self, _session_id: &str) -> Result<(), ConnectorError> {
        Ok(())
    }
    async fn approve_tool_call(
        &self,
        _session_id: &str,
        _tool_call_id: &str,
    ) -> Result<(), ConnectorError> {
        Ok(())
    }
    async fn reject_tool_call(
        &self,
        _session_id: &str,
        _tool_call_id: &str,
        _reason: Option<String>,
    ) -> Result<(), ConnectorError> {
        Ok(())
    }
}

#[test]
fn resolve_prompt_payload_from_text_block() {
    let input = UserPromptInput::from_text("  hello world  ");

    let payload = input
        .resolve_prompt_payload()
        .expect("resolve should succeed");
    assert_eq!(payload.text_prompt, "hello world");
    assert_eq!(payload.user_blocks.len(), 1);
    assert!(matches!(payload.prompt_payload, PromptPayload::Blocks(_)));

    let serialized =
        serde_json::to_value(&payload.user_blocks[0]).expect("serialize content block");
    assert_eq!(
        serialized.get("type").and_then(|v| v.as_str()),
        Some("text")
    );
}

#[test]
fn resolve_prompt_payload_supports_multiple_block_types() {
    let input = UserPromptInput {
        prompt_blocks: Some(vec![
            json!({ "type": "text", "text": "请分析 @src/main.ts" }),
            json!({ "type": "resource_link", "uri": "file:///workspace/src/main.ts", "name": "src/main.ts" }),
            json!({ "type": "image", "mimeType": "image/png", "data": "AAAA" }),
        ]),
        working_dir: None,
        env: std::collections::HashMap::new(),
        executor_config: None,
    };

    let payload = input
        .resolve_prompt_payload()
        .expect("resolve should succeed");
    assert_eq!(payload.user_blocks.len(), 3);
    assert!(matches!(payload.prompt_payload, PromptPayload::Blocks(_)));
    assert!(payload.text_prompt.contains("请分析 @src/main.ts"));
    assert!(
        payload
            .text_prompt
            .contains("[引用文件: src/main.ts (file:///workspace/src/main.ts)]")
    );
    assert!(
        payload
            .text_prompt
            .contains("[引用图片: mimeType=image/png")
    );
}

#[test]
fn build_user_notifications_preserves_block_types_and_index() {
    let blocks = vec![
        serde_json::from_value::<ContentBlock>(json!({
            "type": "text",
            "text": "hello"
        }))
        .expect("text block"),
        serde_json::from_value::<ContentBlock>(json!({
            "type": "resource_link",
            "uri": "file:///workspace/src/main.ts",
            "name": "src/main.ts"
        }))
        .expect("resource_link block"),
    ];

    let mut source = AgentDashSourceV1::new("unit-test", "local_executor");
    source.executor_id = Some("CLAUDE_CODE".to_string());

    let notifications = build_user_message_notifications("sess-test", &source, "t100", &blocks);
    assert_eq!(notifications.len(), 2);

    let first = serde_json::to_value(&notifications[0]).expect("serialize first");
    let second = serde_json::to_value(&notifications[1]).expect("serialize second");

    assert_eq!(
        first
            .get("update")
            .and_then(|u| u.get("content"))
            .and_then(|c| c.get("type"))
            .and_then(|v| v.as_str()),
        Some("text")
    );
    assert_eq!(
        second
            .get("update")
            .and_then(|u| u.get("content"))
            .and_then(|c| c.get("type"))
            .and_then(|v| v.as_str()),
        Some("resource_link")
    );
    assert_eq!(
        second
            .get("update")
            .and_then(|u| u.get("_meta"))
            .and_then(|m| m.get("agentdash"))
            .and_then(|m| m.get("trace"))
            .and_then(|t| t.get("entryIndex"))
            .and_then(|v| v.as_u64()),
        Some(1)
    );
}

#[tokio::test]
async fn respond_companion_request_resolves_waiting_tool_and_persists_response_event() {
    struct NoopConnector;

    #[async_trait::async_trait]
    impl AgentConnector for NoopConnector {
        fn connector_id(&self) -> &'static str {
            "noop"
        }
        fn connector_type(&self) -> agentdash_spi::ConnectorType {
            agentdash_spi::ConnectorType::LocalExecutor
        }
        fn capabilities(&self) -> agentdash_spi::ConnectorCapabilities {
            agentdash_spi::ConnectorCapabilities::default()
        }
        fn list_executors(&self) -> Vec<agentdash_spi::AgentInfo> {
            Vec::new()
        }
        async fn discover_options_stream(
            &self,
            _executor: &str,
            _working_dir: Option<PathBuf>,
        ) -> Result<futures::stream::BoxStream<'static, json_patch::Patch>, ConnectorError>
        {
            Ok(Box::pin(stream::empty()))
        }
        async fn prompt(
            &self,
            _session_id: &str,
            _follow_up_session_id: Option<&str>,
            _prompt: &PromptPayload,
            _context: agentdash_spi::ExecutionContext,
        ) -> Result<agentdash_spi::ExecutionStream, ConnectorError> {
            Ok(Box::pin(stream::empty()))
        }
        async fn cancel(&self, _session_id: &str) -> Result<(), ConnectorError> {
            Ok(())
        }
        async fn approve_tool_call(
            &self,
            _session_id: &str,
            _tool_call_id: &str,
        ) -> Result<(), ConnectorError> {
            Ok(())
        }
        async fn reject_tool_call(
            &self,
            _session_id: &str,
            _tool_call_id: &str,
            _reason: Option<String>,
        ) -> Result<(), ConnectorError> {
            Ok(())
        }
    }

    let base = tempfile::tempdir().expect("tempdir");
    let hub = test_hub(base.path().to_path_buf(), Arc::new(NoopConnector), None);
    let session = hub.create_session("test").await.expect("create session");
    let payload = json!({
        "type": "decision",
        "status": "approved",
        "choice": "YES",
        "summary": "YES"
    });

    let rx = hub
        .companion_wait_registry
        .register(&session.id, "req-1", "turn-1", Some("approval".to_string()))
        .await;

    hub.respond_companion_request(&session.id, "req-1", payload.clone())
        .await
        .expect("respond should succeed");

    assert_eq!(rx.await.expect("wait registry should resolve"), payload);

    let events = hub
        .persistence
        .list_all_events(&session.id)
        .await
        .expect("events should load");
    let response = events
        .iter()
        .find(|event| {
            let event_type = serde_json::to_value(&event.notification)
                .ok()
                .and_then(|value| {
                    value
                        .get("update")
                        .and_then(|update| update.get("_meta"))
                        .and_then(|meta| meta.get("agentdash"))
                        .and_then(|agentdash| agentdash.get("event"))
                        .and_then(|event| event.get("type"))
                        .and_then(|value| value.as_str().map(ToString::to_string))
                });
            event_type.as_deref() == Some("companion_human_response")
        })
        .expect("response event should exist");

    assert_eq!(response.turn_id.as_deref(), Some("turn-1"));

    let notification = serde_json::to_value(&response.notification).expect("serialize");
    let event_data = notification
        .get("update")
        .and_then(|update| update.get("_meta"))
        .and_then(|meta| meta.get("agentdash"))
        .and_then(|agentdash| agentdash.get("event"))
        .and_then(|event| event.get("data"))
        .expect("response event data");
    assert_eq!(
        event_data
            .get("request_id")
            .and_then(|value| value.as_str()),
        Some("req-1")
    );
    assert_eq!(
        event_data
            .get("resumed_waiting_tool")
            .and_then(|value| value.as_bool()),
        Some(true)
    );
    assert_eq!(
        event_data
            .get("request_type")
            .and_then(|value| value.as_str()),
        Some("approval")
    );
}

#[tokio::test]
async fn start_prompt_triggers_session_start_before_connector_prompt() {
    let base = tempfile::tempdir().expect("tempdir");
    let connector = Arc::new(SessionStartAwareConnector::default());
    let queries = Arc::new(TokioMutex::new(Vec::new()));
    let hook_provider = Arc::new(RecordingHookProvider {
        queries: queries.clone(),
    });
    let hub = test_hub(
        base.path().to_path_buf(),
        connector.clone(),
        Some(hook_provider),
    );
    let session = hub.create_session("test").await.expect("create session");
    hub.mark_owner_bootstrap_pending(&session.id)
        .await
        .expect("should mark pending");

    hub.start_prompt(&session.id, owner_bootstrap_request("hello", "ctx"))
        .await
        .expect("prompt should start");

    let seen = connector.session_start_seen.lock().await;
    assert_eq!(seen.as_slice(), &[true]);

    let queries = queries.lock().await;
    assert!(
        queries
            .iter()
            .any(|query| matches!(query.trigger, HookTrigger::SessionStart))
    );
}

#[tokio::test]
async fn owner_bootstrap_marks_session_meta_bootstrapped() {
    let base = tempfile::tempdir().expect("tempdir");
    let connector = Arc::new(SessionStartAwareConnector::default());
    let queries = Arc::new(TokioMutex::new(Vec::new()));
    let hook_provider = Arc::new(RecordingHookProvider {
        queries: queries.clone(),
    });
    let hub = test_hub(base.path().to_path_buf(), connector, Some(hook_provider));
    let session = hub.create_session("test").await.expect("create session");
    hub.mark_owner_bootstrap_pending(&session.id)
        .await
        .expect("should mark pending");

    hub.start_prompt(&session.id, owner_bootstrap_request("hello", "ctx"))
        .await
        .expect("prompt should start");

    let meta = hub
        .get_session_meta(&session.id)
        .await
        .expect("meta should load")
        .expect("session should exist");
    assert_eq!(meta.bootstrap_state, SessionBootstrapState::Bootstrapped);
}

#[tokio::test]
async fn build_continuation_system_context_strips_owner_resource_blocks() {
    let persistence = Arc::new(MemorySessionPersistence::default());
    let base = tempfile::tempdir().expect("tempdir");
    let hub = SessionHub::new_with_hooks_and_persistence(
        Some(local_workspace_vfs(&base.path().to_path_buf())),
        Arc::new(SessionStartAwareConnector::default()),
        None,
        persistence,
    );
    let session = hub.create_session("test").await.expect("create session");

    let source = AgentDashSourceV1::new("test", "unit");
    let user_blocks = vec![
        serde_json::from_value::<ContentBlock>(serde_json::json!({
            "type": "resource",
            "resource": {
                "uri": "agentdash://project-context/project-1",
                "mimeType": "text/markdown",
                "text": "## Project\nhidden"
            }
        }))
        .expect("resource block"),
        ContentBlock::Text(TextContent::new("继续分析 session 生命周期")),
    ];
    for notification in
        build_user_message_notifications(&session.id, &source, "t-1", &user_blocks)
    {
        hub.inject_notification(&session.id, notification)
            .await
            .expect("inject user notification");
    }

    let assistant_chunk = ContentChunk::new(ContentBlock::Text(TextContent::new("已记录历史")))
        .message_id(Some("assistant-msg-1".to_string()))
        .meta(
            merge_agentdash_meta(
                None,
                &AgentDashMetaV1::new()
                    .source(Some(source.clone()))
                    .trace(Some({
                        let mut trace = AgentDashTraceV1::new();
                        trace.turn_id = Some("t-1".to_string());
                        trace.entry_index = Some(99);
                        trace
                    })),
            )
            .expect("assistant meta"),
        );
    hub.inject_notification(
        &session.id,
        SessionNotification::new(
            SessionId::new(session.id.clone()),
            SessionUpdate::AgentMessageChunk(assistant_chunk),
        ),
    )
    .await
    .expect("inject assistant notification");

    let context = hub
        .build_continuation_system_context(&session.id, Some("## Owner\nproject"))
        .await
        .expect("context should build")
        .expect("continuation context should exist");
    assert!(context.contains("继续分析 session 生命周期"));
    assert!(context.contains("已记录历史"));
    assert!(context.contains("## Owner"));
    assert!(!context.contains("agentdash://project-context/"));
    assert!(!context.contains("hidden"));
}

#[tokio::test]
async fn build_restored_session_messages_reconstructs_tool_history_without_owner_blocks() {
    let persistence = Arc::new(MemorySessionPersistence::default());
    let base = tempfile::tempdir().expect("tempdir");
    let hub = SessionHub::new_with_hooks_and_persistence(
        Some(local_workspace_vfs(&base.path().to_path_buf())),
        Arc::new(SessionStartAwareConnector::default()),
        None,
        persistence,
    );
    let session = hub.create_session("test").await.expect("create session");

    let source = AgentDashSourceV1::new("test", "unit");
    let user_blocks = vec![
        serde_json::from_value::<ContentBlock>(serde_json::json!({
            "type": "resource",
            "resource": {
                "uri": "agentdash://project-context/project-1",
                "mimeType": "text/markdown",
                "text": "## Project\nhidden"
            }
        }))
        .expect("resource block"),
        ContentBlock::Text(TextContent::new("继续分析 session 生命周期")),
    ];
    for notification in
        build_user_message_notifications(&session.id, &source, "t-1", &user_blocks)
    {
        hub.inject_notification(&session.id, notification)
            .await
            .expect("inject user notification");
    }

    let assistant_chunk = ContentChunk::new(ContentBlock::Text(TextContent::new("已记录历史")))
        .message_id(Some("assistant-msg-1".to_string()))
        .meta(Some(test_meta(&source, "t-1", 1)));
    hub.inject_notification(
        &session.id,
        SessionNotification::new(
            SessionId::new(session.id.clone()),
            SessionUpdate::AgentMessageChunk(assistant_chunk),
        ),
    )
    .await
    .expect("inject assistant notification");

    let tool_call = ToolCall::new(ToolCallId::new("tool-1"), "shell_exec")
        .status(ToolCallStatus::Pending)
        .raw_input(serde_json::json!({ "command": "pwd" }))
        .meta(Some(test_meta(&source, "t-1", 1)));
    hub.inject_notification(
        &session.id,
        SessionNotification::new(
            SessionId::new(session.id.clone()),
            SessionUpdate::ToolCall(tool_call),
        ),
    )
    .await
    .expect("inject tool call");

    let raw_result = serde_json::to_value(agentdash_spi::AgentToolResult {
        content: vec![agentdash_spi::ContentPart::text("workspace root")],
        is_error: false,
        details: Some(serde_json::json!({ "exit_code": 0 })),
    })
    .expect("serialize tool result");
    let mut fields = ToolCallUpdateFields::default();
    fields.title = Some("shell_exec".to_string());
    fields.status = Some(ToolCallStatus::Completed);
    fields.raw_output = Some(raw_result);
    let tool_update = ToolCallUpdate::new(ToolCallId::new("tool-1"), fields)
        .meta(Some(test_meta(&source, "t-1", 1)));
    hub.inject_notification(
        &session.id,
        SessionNotification::new(
            SessionId::new(session.id.clone()),
            SessionUpdate::ToolCallUpdate(tool_update),
        ),
    )
    .await
    .expect("inject tool update");

    let messages = hub
        .build_restored_session_messages(&session.id)
        .await
        .expect("messages should build");
    assert_eq!(messages.len(), 3);

    match &messages[0] {
        agentdash_spi::AgentMessage::User { content, .. } => {
            assert_eq!(content.len(), 1);
            assert_eq!(messages[0].first_text(), Some("继续分析 session 生命周期"));
            assert_ne!(messages[0].first_text(), Some("## Project\nhidden"));
        }
        other => panic!("unexpected first message: {other:?}"),
    }

    match &messages[1] {
        agentdash_spi::AgentMessage::Assistant {
            tool_calls,
            stop_reason,
            ..
        } => {
            assert_eq!(messages[1].first_text(), Some("已记录历史"));
            assert_eq!(tool_calls.len(), 1);
            assert_eq!(tool_calls[0].name, "shell_exec");
            assert_eq!(stop_reason.clone(), Some(StopReason::ToolUse));
        }
        other => panic!("unexpected assistant message: {other:?}"),
    }

    match &messages[2] {
        agentdash_spi::AgentMessage::ToolResult {
            tool_call_id,
            tool_name,
            details,
            is_error,
            ..
        } => {
            assert_eq!(tool_call_id, "tool-1");
            assert_eq!(tool_name.as_deref(), Some("shell_exec"));
            assert_eq!(messages[2].first_text(), Some("workspace root"));
            assert_eq!(
                details
                    .as_ref()
                    .and_then(|value| value.get("exit_code"))
                    .and_then(serde_json::Value::as_i64),
                Some(0)
            );
            assert!(!*is_error);
        }
        other => panic!("unexpected tool result: {other:?}"),
    }
}

#[tokio::test]
async fn build_restored_session_messages_applies_latest_compaction_checkpoint() {
    let persistence = Arc::new(MemorySessionPersistence::default());
    let base = tempfile::tempdir().expect("tempdir");
    let hub = SessionHub::new_with_hooks_and_persistence(
        Some(local_workspace_vfs(&base.path().to_path_buf())),
        Arc::new(SessionStartAwareConnector::default()),
        None,
        persistence,
    );
    let session = hub.create_session("test").await.expect("create session");
    let source = AgentDashSourceV1::new("test", "unit");

    for (turn_id, entry_index, text) in [
        ("t-1", 0_u32, "历史用户消息 1"),
        ("t-2", 0_u32, "历史用户消息 2"),
        ("t-3", 0_u32, "最近用户消息"),
    ] {
        hub.inject_notification(
            &session.id,
            SessionNotification::new(
                SessionId::new(session.id.clone()),
                SessionUpdate::UserMessageChunk(
                    ContentChunk::new(ContentBlock::Text(TextContent::new(text)))
                        .meta(Some(test_meta(&source, turn_id, entry_index))),
                ),
            ),
        )
        .await
        .expect("inject user notification");
    }

    let compaction_meta = test_event_meta(
        &source,
        "t-3",
        0,
        "context_compacted",
        serde_json::json!({
            "summary": "## 历史摘要\n- 已完成旧分析",
            "tokens_before": 42000,
            "messages_compacted": 2,
            "newly_compacted_messages": 2,
            "timestamp_ms": 1710000000000_u64,
        }),
    );
    hub.inject_notification(
        &session.id,
        SessionNotification::new(
            SessionId::new(session.id.clone()),
            SessionUpdate::SessionInfoUpdate(SessionInfoUpdate::new().meta(compaction_meta)),
        ),
    )
    .await
    .expect("inject compaction checkpoint");

    let restored = hub
        .build_restored_session_messages(&session.id)
        .await
        .expect("messages should build");

    assert_eq!(restored.len(), 2);
    match &restored[0] {
        agentdash_spi::AgentMessage::CompactionSummary {
            summary,
            tokens_before,
            messages_compacted,
            compacted_until_ref,
            ..
        } => {
            assert!(summary.contains("历史摘要"));
            assert_eq!(*tokens_before, 42_000);
            assert_eq!(*messages_compacted, 2);
            assert_eq!(
                compacted_until_ref
                    .as_ref()
                    .map(|message_ref| (message_ref.turn_id.as_str(), message_ref.entry_index)),
                Some(("t-2", 0))
            );
        }
        other => panic!("unexpected first message: {other:?}"),
    }
    assert_eq!(restored[1].first_text(), Some("最近用户消息"));
}

#[tokio::test]
async fn build_restored_session_messages_enriches_follow_up_compaction_checkpoint() {
    let persistence = Arc::new(MemorySessionPersistence::default());
    let base = tempfile::tempdir().expect("tempdir");
    let hub = SessionHub::new_with_hooks_and_persistence(
        Some(local_workspace_vfs(&base.path().to_path_buf())),
        Arc::new(SessionStartAwareConnector::default()),
        None,
        persistence,
    );
    let session = hub.create_session("test").await.expect("create session");
    let source = AgentDashSourceV1::new("test", "unit");

    for (turn_id, entry_index, text) in [
        ("t-1", 0_u32, "历史用户消息 1"),
        ("t-2", 0_u32, "历史用户消息 2"),
        ("t-3", 0_u32, "阶段性保留消息"),
        ("t-4", 0_u32, "最新保留消息"),
    ] {
        hub.inject_notification(
            &session.id,
            SessionNotification::new(
                SessionId::new(session.id.clone()),
                SessionUpdate::UserMessageChunk(
                    ContentChunk::new(ContentBlock::Text(TextContent::new(text)))
                        .meta(Some(test_meta(&source, turn_id, entry_index))),
                ),
            ),
        )
        .await
        .expect("inject user notification");
    }

    for (turn_id, summary, messages_compacted) in [
        ("t-3", "## 第一版历史摘要\n- 已压缩前两条", 2_u32),
        ("t-4", "## 第二版历史摘要\n- 又压缩了一条", 3_u32),
    ] {
        let compaction_meta = test_event_meta(
            &source,
            turn_id,
            0,
            "context_compacted",
            serde_json::json!({
                "summary": summary,
                "tokens_before": 42000,
                "messages_compacted": messages_compacted,
                "newly_compacted_messages": 1,
                "timestamp_ms": 1710000000000_u64,
            }),
        );
        hub.inject_notification(
            &session.id,
            SessionNotification::new(
                SessionId::new(session.id.clone()),
                SessionUpdate::SessionInfoUpdate(
                    SessionInfoUpdate::new().meta(compaction_meta),
                ),
            ),
        )
        .await
        .expect("inject compaction checkpoint");
    }

    let restored = hub
        .build_restored_session_messages(&session.id)
        .await
        .expect("messages should build");

    assert_eq!(restored.len(), 2);
    match &restored[0] {
        agentdash_spi::AgentMessage::CompactionSummary {
            summary,
            messages_compacted,
            compacted_until_ref,
            ..
        } => {
            assert!(summary.contains("第二版历史摘要"));
            assert_eq!(*messages_compacted, 3);
            assert_eq!(
                compacted_until_ref
                    .as_ref()
                    .map(|message_ref| (message_ref.turn_id.as_str(), message_ref.entry_index)),
                Some(("t-3", 0))
            );
        }
        other => panic!("unexpected first message: {other:?}"),
    }
    assert_eq!(restored[1].first_text(), Some("最新保留消息"));
}

#[tokio::test]
async fn build_continuation_system_context_uses_compacted_projection() {
    let persistence = Arc::new(MemorySessionPersistence::default());
    let base = tempfile::tempdir().expect("tempdir");
    let hub = SessionHub::new_with_hooks_and_persistence(
        Some(local_workspace_vfs(&base.path().to_path_buf())),
        Arc::new(SessionStartAwareConnector::default()),
        None,
        persistence,
    );
    let session = hub.create_session("test").await.expect("create session");
    let source = AgentDashSourceV1::new("test", "unit");

    for (turn_id, entry_index, text) in [
        ("t-1", 0_u32, "第一段旧历史"),
        ("t-2", 0_u32, "第二段旧历史"),
        ("t-3", 0_u32, "保留的新历史"),
    ] {
        hub.inject_notification(
            &session.id,
            SessionNotification::new(
                SessionId::new(session.id.clone()),
                SessionUpdate::UserMessageChunk(
                    ContentChunk::new(ContentBlock::Text(TextContent::new(text)))
                        .meta(Some(test_meta(&source, turn_id, entry_index))),
                ),
            ),
        )
        .await
        .expect("inject user notification");
    }

    let compaction_meta = test_event_meta(
        &source,
        "t-3",
        0,
        "context_compacted",
        serde_json::json!({
            "summary": "压缩后的历史摘要",
            "tokens_before": 38000,
            "messages_compacted": 2,
            "newly_compacted_messages": 2,
            "timestamp_ms": 1710000000000_u64,
        }),
    );
    hub.inject_notification(
        &session.id,
        SessionNotification::new(
            SessionId::new(session.id.clone()),
            SessionUpdate::SessionInfoUpdate(SessionInfoUpdate::new().meta(compaction_meta)),
        ),
    )
    .await
    .expect("inject compaction checkpoint");

    let context = hub
        .build_continuation_system_context(&session.id, None)
        .await
        .expect("context should build")
        .expect("continuation context should exist");

    assert!(context.contains("压缩后的历史摘要"));
    assert!(context.contains("保留的新历史"));
    assert!(!context.contains("第一段旧历史"));
    assert!(!context.contains("第二段旧历史"));
}

#[tokio::test]
async fn start_prompt_passes_restored_session_state_when_connector_supports_repository_restore() {
    let base = tempfile::tempdir().expect("tempdir");
    let connector = Arc::new(RepositoryRestoreRecordingConnector::default());
    let hub = test_hub(base.path().to_path_buf(), connector.clone(), None);
    let session = hub.create_session("test").await.expect("create session");

    let source = AgentDashSourceV1::new("test", "unit");
    for notification in build_user_message_notifications(
        &session.id,
        &source,
        "t-1",
        &[ContentBlock::Text(TextContent::new("历史用户消息"))],
    ) {
        hub.inject_notification(&session.id, notification)
            .await
            .expect("inject user notification");
    }
    let assistant_chunk =
        ContentChunk::new(ContentBlock::Text(TextContent::new("历史助手消息")))
            .message_id(Some("assistant-msg-restore".to_string()))
            .meta(Some(test_meta(&source, "t-1", 1)));
    hub.inject_notification(
        &session.id,
        SessionNotification::new(
            SessionId::new(session.id.clone()),
            SessionUpdate::AgentMessageChunk(assistant_chunk),
        ),
    )
    .await
    .expect("inject assistant notification");

    assert!(
        !hub.has_live_runtime(&session.id).await,
        "仅有被动 session 条目时不应视为 live runtime"
    );

    let mut req = simple_prompt_request("新的用户消息");
    req.user_input.executor_config = Some(agentdash_spi::AgentConfig::new("PI_AGENT"));
    hub.start_prompt(&session.id, req)
        .await
        .expect("prompt should start");

    let contexts = connector.contexts.lock().await;
    let context = contexts.last().expect("context should be recorded");
    let restored = context
        .turn
        .restored_session_state
        .as_ref()
        .expect("restored session state should exist");
    assert_eq!(restored.messages.len(), 2);
    assert_eq!(restored.messages[0].first_text(), Some("历史用户消息"));
    assert_eq!(restored.messages[1].first_text(), Some("历史助手消息"));
}

#[tokio::test]
async fn start_prompt_uses_request_vfs_override() {
    #[derive(Default)]
    struct RecordingConnector {
        contexts: Arc<TokioMutex<Vec<agentdash_spi::ExecutionContext>>>,
    }

    #[async_trait::async_trait]
    impl AgentConnector for RecordingConnector {
        fn connector_id(&self) -> &'static str {
            "recording"
        }
        fn connector_type(&self) -> agentdash_spi::ConnectorType {
            agentdash_spi::ConnectorType::LocalExecutor
        }
        fn capabilities(&self) -> agentdash_spi::ConnectorCapabilities {
            agentdash_spi::ConnectorCapabilities::default()
        }
        fn list_executors(&self) -> Vec<agentdash_spi::AgentInfo> {
            Vec::new()
        }
        async fn discover_options_stream(
            &self,
            _executor: &str,
            _working_dir: Option<PathBuf>,
        ) -> Result<futures::stream::BoxStream<'static, json_patch::Patch>, ConnectorError>
        {
            Ok(Box::pin(stream::empty()))
        }
        async fn prompt(
            &self,
            _session_id: &str,
            _follow_up_session_id: Option<&str>,
            _prompt: &PromptPayload,
            context: agentdash_spi::ExecutionContext,
        ) -> Result<agentdash_spi::ExecutionStream, ConnectorError> {
            self.contexts.lock().await.push(context);
            Ok(Box::pin(stream::empty()))
        }
        async fn cancel(&self, _session_id: &str) -> Result<(), ConnectorError> {
            Ok(())
        }
        async fn approve_tool_call(
            &self,
            _session_id: &str,
            _tool_call_id: &str,
        ) -> Result<(), ConnectorError> {
            Ok(())
        }
        async fn reject_tool_call(
            &self,
            _session_id: &str,
            _tool_call_id: &str,
            _reason: Option<String>,
        ) -> Result<(), ConnectorError> {
            Ok(())
        }
    }

    let base = tempfile::tempdir().expect("tempdir");
    let workspace = tempfile::tempdir().expect("workspace");
    let connector = Arc::new(RecordingConnector::default());
    let hub = test_hub(base.path().to_path_buf(), connector.clone(), None);
    let session = hub.create_session("test").await.expect("create session");

    hub.start_prompt(
        &session.id,
        PromptSessionRequest {
            user_input: UserPromptInput {
                prompt_blocks: Some(vec![json!({
                    "type": "text",
                    "text": "hello",
                })]),
                working_dir: Some("src".to_string()),
                env: HashMap::new(),
                executor_config: Some(agentdash_spi::AgentConfig::new("PI_AGENT")),
            },
            mcp_servers: vec![],
            relay_mcp_server_names: Default::default(),
            vfs: Some(local_workspace_vfs(workspace.path())),
            flow_capabilities: None,
            effective_capability_keys: None,
            context_bundle: None,
            hook_snapshot_reload: HookSnapshotReloadTrigger::None,
            identity: None,
            post_turn_handler: None,
        },
    )
    .await
    .expect("prompt should start");

    let contexts = connector.contexts.lock().await;
    let context = contexts.last().expect("context should be recorded");
    let ws_path = agentdash_spi::workspace_path_from_context(context).expect("default mount");
    assert_eq!(ws_path, workspace.path().to_path_buf());
    assert_eq!(
        context.session.working_directory,
        workspace.path().join("src")
    );
}

#[tokio::test]
async fn start_prompt_reuses_existing_session_executor_config() {
    #[derive(Default)]
    struct RecordingConnector {
        contexts: Arc<TokioMutex<Vec<agentdash_spi::ExecutionContext>>>,
    }

    #[async_trait::async_trait]
    impl AgentConnector for RecordingConnector {
        fn connector_id(&self) -> &'static str {
            "recording"
        }
        fn connector_type(&self) -> agentdash_spi::ConnectorType {
            agentdash_spi::ConnectorType::LocalExecutor
        }
        fn capabilities(&self) -> agentdash_spi::ConnectorCapabilities {
            agentdash_spi::ConnectorCapabilities::default()
        }
        fn list_executors(&self) -> Vec<agentdash_spi::AgentInfo> {
            vec![agentdash_spi::AgentInfo {
                id: "PI_AGENT".to_string(),
                name: "PI Agent".to_string(),
                variants: Vec::new(),
                available: true,
            }]
        }
        async fn discover_options_stream(
            &self,
            _executor: &str,
            _working_dir: Option<PathBuf>,
        ) -> Result<futures::stream::BoxStream<'static, json_patch::Patch>, ConnectorError>
        {
            Ok(Box::pin(stream::empty()))
        }
        async fn prompt(
            &self,
            _session_id: &str,
            _follow_up_session_id: Option<&str>,
            _prompt: &PromptPayload,
            context: agentdash_spi::ExecutionContext,
        ) -> Result<agentdash_spi::ExecutionStream, ConnectorError> {
            self.contexts.lock().await.push(context);
            Ok(Box::pin(stream::empty()))
        }
        async fn cancel(&self, _session_id: &str) -> Result<(), ConnectorError> {
            Ok(())
        }
        async fn approve_tool_call(
            &self,
            _session_id: &str,
            _tool_call_id: &str,
        ) -> Result<(), ConnectorError> {
            Ok(())
        }
        async fn reject_tool_call(
            &self,
            _session_id: &str,
            _tool_call_id: &str,
            _reason: Option<String>,
        ) -> Result<(), ConnectorError> {
            Ok(())
        }
    }

    let base = tempfile::tempdir().expect("tempdir");
    let connector = Arc::new(RecordingConnector::default());
    let hub = test_hub(base.path().to_path_buf(), connector.clone(), None);

    let session = hub
        .create_session("reuse existing executor")
        .await
        .expect("create session");
    hub.update_session_meta(&session.id, |meta| {
        meta.executor_config = Some(agentdash_spi::AgentConfig::new("PI_AGENT"));
    })
    .await
    .expect("update meta should succeed");

    hub.start_prompt(&session.id, simple_prompt_request("hello"))
        .await
        .expect("prompt should start");
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    let contexts = connector.contexts.lock().await;
    let context = contexts.last().expect("context should be recorded");
    assert_eq!(context.session.executor_config.executor, "PI_AGENT");
}

#[tokio::test]
async fn start_prompt_records_failed_terminal_when_connector_setup_fails() {
    struct FailingConnector;

    #[async_trait::async_trait]
    impl AgentConnector for FailingConnector {
        fn connector_id(&self) -> &'static str {
            "failing"
        }
        fn connector_type(&self) -> agentdash_spi::ConnectorType {
            agentdash_spi::ConnectorType::LocalExecutor
        }
        fn capabilities(&self) -> agentdash_spi::ConnectorCapabilities {
            agentdash_spi::ConnectorCapabilities::default()
        }
        fn list_executors(&self) -> Vec<agentdash_spi::AgentInfo> {
            Vec::new()
        }
        async fn discover_options_stream(
            &self,
            _executor: &str,
            _working_dir: Option<PathBuf>,
        ) -> Result<futures::stream::BoxStream<'static, json_patch::Patch>, ConnectorError>
        {
            Ok(Box::pin(stream::empty()))
        }
        async fn prompt(
            &self,
            _session_id: &str,
            _follow_up_session_id: Option<&str>,
            _prompt: &PromptPayload,
            _context: agentdash_spi::ExecutionContext,
        ) -> Result<agentdash_spi::ExecutionStream, ConnectorError> {
            Err(ConnectorError::Runtime(
                "connector setup failed".to_string(),
            ))
        }
        async fn cancel(&self, _session_id: &str) -> Result<(), ConnectorError> {
            Ok(())
        }
        async fn approve_tool_call(
            &self,
            _session_id: &str,
            _tool_call_id: &str,
        ) -> Result<(), ConnectorError> {
            Ok(())
        }
        async fn reject_tool_call(
            &self,
            _session_id: &str,
            _tool_call_id: &str,
            _reason: Option<String>,
        ) -> Result<(), ConnectorError> {
            Ok(())
        }
    }

    let base = tempfile::tempdir().expect("tempdir");
    let hub = test_hub(base.path().to_path_buf(), Arc::new(FailingConnector), None);
    let session = hub.create_session("test").await.expect("create session");

    let error = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        hub.start_prompt(&session.id, simple_prompt_request("hello")),
    )
    .await
    .expect("prompt should not hang")
    .expect_err("prompt should fail");
    assert!(error.to_string().contains("connector setup failed"));

    let history = hub
        .persistence
        .list_all_events(&session.id)
        .await
        .expect("history should load");
    let terminal = history
        .iter()
        .filter_map(|event| match &event.notification.update {
            SessionUpdate::SessionInfoUpdate(info) => {
                parse_turn_terminal_event(info.meta.as_ref())
            }
            _ => None,
        })
        .last()
        .expect("terminal event should exist");
    assert_eq!(terminal.1, super::super::hub_support::TurnTerminalKind::Failed);
    assert_eq!(
        terminal.2.as_deref(),
        Some("执行器运行错误: connector setup failed")
    );
}

#[tokio::test]
async fn cancel_marks_running_turn_interrupted() {
    #[derive(Default)]
    struct CancelAwareConnector {
        streams: Arc<
            TokioMutex<
                HashMap<String, mpsc::Sender<Result<SessionNotification, ConnectorError>>>,
            >,
        >,
    }

    #[async_trait::async_trait]
    impl AgentConnector for CancelAwareConnector {
        fn connector_id(&self) -> &'static str {
            "cancel-aware"
        }
        fn connector_type(&self) -> agentdash_spi::ConnectorType {
            agentdash_spi::ConnectorType::LocalExecutor
        }
        fn capabilities(&self) -> agentdash_spi::ConnectorCapabilities {
            agentdash_spi::ConnectorCapabilities::default()
        }
        fn list_executors(&self) -> Vec<agentdash_spi::AgentInfo> {
            Vec::new()
        }
        async fn discover_options_stream(
            &self,
            _executor: &str,
            _working_dir: Option<PathBuf>,
        ) -> Result<futures::stream::BoxStream<'static, json_patch::Patch>, ConnectorError>
        {
            Ok(Box::pin(stream::empty()))
        }
        async fn prompt(
            &self,
            session_id: &str,
            _follow_up_session_id: Option<&str>,
            _prompt: &PromptPayload,
            _context: agentdash_spi::ExecutionContext,
        ) -> Result<agentdash_spi::ExecutionStream, ConnectorError> {
            let (tx, rx) = mpsc::channel(4);
            self.streams.lock().await.insert(session_id.to_string(), tx);
            Ok(Box::pin(ReceiverStream::new(rx)))
        }
        async fn cancel(&self, session_id: &str) -> Result<(), ConnectorError> {
            self.streams.lock().await.remove(session_id);
            Ok(())
        }
        async fn approve_tool_call(
            &self,
            _session_id: &str,
            _tool_call_id: &str,
        ) -> Result<(), ConnectorError> {
            Ok(())
        }
        async fn reject_tool_call(
            &self,
            _session_id: &str,
            _tool_call_id: &str,
            _reason: Option<String>,
        ) -> Result<(), ConnectorError> {
            Ok(())
        }
    }

    let base = tempfile::tempdir().expect("tempdir");
    let connector = Arc::new(CancelAwareConnector::default());
    let hub = test_hub(base.path().to_path_buf(), connector, None);
    let session = hub.create_session("test").await.expect("create session");

    let turn_id = hub
        .start_prompt(&session.id, simple_prompt_request("hello"))
        .await
        .expect("prompt should start");
    hub.cancel(&session.id)
        .await
        .expect("cancel should succeed");
    // 等待 adapter task（检测 stream 关闭 → drop processor_tx）
    // 和 processor task（检测 channel 关闭 → 清理 runtime）完成
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let state = hub
        .inspect_session_execution_state(&session.id)
        .await
        .expect("state should load");
    assert_eq!(
        state,
        SessionExecutionState::Interrupted {
            turn_id: Some(turn_id.clone()),
            message: Some("执行已取消".to_string())
        }
    );

    let history = hub
        .persistence
        .list_all_events(&session.id)
        .await
        .expect("history should load");
    let terminal = history
        .iter()
        .filter_map(|event| match &event.notification.update {
            SessionUpdate::SessionInfoUpdate(info) => {
                parse_turn_terminal_event(info.meta.as_ref())
            }
            _ => None,
        })
        .last()
        .expect("terminal event should exist");
    assert_eq!(terminal.0, turn_id);
    assert_eq!(terminal.1, super::super::hub_support::TurnTerminalKind::Interrupted);
    assert_eq!(terminal.2.as_deref(), Some("执行已取消"));
}

// ─────────────────────────────────────────────────────────────────────
// Fail-lock: auto-resume 必须经过 PromptRequestAugmenter
//
// 这条测试锁住 "主通道 vs auto-resume 对齐" 的契约：hub.rs schedule_hook_auto_resume
// 必须先调 augmenter.augment() 再走 start_prompt。如果未来有人把 augmenter 链路
// 删掉或短路，这条测试会失败，从而阻止 Agent "复读" bug 回归。
// ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn schedule_hook_auto_resume_routes_through_augmenter() {
    use crate::session::augmenter::PromptRequestAugmenter;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct SpyAugmenter {
        calls: Arc<AtomicUsize>,
        captured_prompt: Arc<TokioMutex<Option<String>>>,
        captured_mcp_len: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl PromptRequestAugmenter for SpyAugmenter {
        async fn augment(
            &self,
            _session_id: &str,
            req: PromptSessionRequest,
        ) -> Result<PromptSessionRequest, ConnectorError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let text = req
                .user_input
                .prompt_blocks
                .as_ref()
                .and_then(|blocks| blocks.first())
                .and_then(|block| block.get("text"))
                .and_then(|v| v.as_str())
                .map(ToString::to_string);
            *self.captured_prompt.lock().await = text;
            self.captured_mcp_len
                .store(req.mcp_servers.len(), Ordering::SeqCst);
            // 为了 augment 成功，返回一个 augmenter 预期会补齐的请求；
            // 这里故意让后续 start_prompt 因缺少 executor_config 而失败——
            // 我们验证的是 augmenter 被调用，不验证整条 prompt 链路跑通。
            Err(ConnectorError::InvalidConfig(
                "spy augmenter stops here".to_string(),
            ))
        }
    }

    struct NoopConnector;

    #[async_trait::async_trait]
    impl AgentConnector for NoopConnector {
        fn connector_id(&self) -> &'static str {
            "noop"
        }
        fn connector_type(&self) -> agentdash_spi::ConnectorType {
            agentdash_spi::ConnectorType::LocalExecutor
        }
        fn capabilities(&self) -> agentdash_spi::ConnectorCapabilities {
            agentdash_spi::ConnectorCapabilities::default()
        }
        fn list_executors(&self) -> Vec<agentdash_spi::AgentInfo> {
            Vec::new()
        }
        async fn discover_options_stream(
            &self,
            _executor: &str,
            _working_dir: Option<PathBuf>,
        ) -> Result<futures::stream::BoxStream<'static, json_patch::Patch>, ConnectorError>
        {
            Ok(Box::pin(stream::empty()))
        }
        async fn prompt(
            &self,
            _session_id: &str,
            _follow_up_session_id: Option<&str>,
            _prompt: &PromptPayload,
            _context: agentdash_spi::ExecutionContext,
        ) -> Result<agentdash_spi::ExecutionStream, ConnectorError> {
            Err(ConnectorError::Runtime(
                "connector should not be reached if augmenter stopped".to_string(),
            ))
        }
        async fn cancel(&self, _session_id: &str) -> Result<(), ConnectorError> {
            Ok(())
        }
        async fn approve_tool_call(
            &self,
            _session_id: &str,
            _tool_call_id: &str,
        ) -> Result<(), ConnectorError> {
            Ok(())
        }
        async fn reject_tool_call(
            &self,
            _session_id: &str,
            _tool_call_id: &str,
            _reason: Option<String>,
        ) -> Result<(), ConnectorError> {
            Ok(())
        }
    }

    let base = tempfile::tempdir().expect("tempdir");
    let hub = test_hub(base.path().to_path_buf(), Arc::new(NoopConnector), None);
    let session = hub.create_session("test").await.expect("create session");

    let calls = Arc::new(AtomicUsize::new(0));
    let captured_prompt = Arc::new(TokioMutex::new(None));
    let captured_mcp_len = Arc::new(AtomicUsize::new(usize::MAX));
    hub.set_prompt_augmenter(Arc::new(SpyAugmenter {
        calls: calls.clone(),
        captured_prompt: captured_prompt.clone(),
        captured_mcp_len: captured_mcp_len.clone(),
    }))
    .await;

    hub.schedule_hook_auto_resume(session.id.clone());

    // schedule_hook_auto_resume 内部 sleep 200ms 后才跑 augment，
    // 给它 1.5s 余量完成。
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1500);
    while calls.load(Ordering::SeqCst) == 0 && std::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    }

    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "augmenter 必须在 auto-resume 时被调用一次，否则主通道与自动续跑会漂移"
    );
    let prompt_text = captured_prompt.lock().await.clone();
    let expected = msg::AUTO_RESUME_PROMPT.to_string();
    assert_eq!(
        prompt_text.as_deref(),
        Some(expected.as_str()),
        "augmenter 收到的应是标准 AUTO_RESUME_PROMPT，而不是被改写过的"
    );
    assert_eq!(
        captured_mcp_len.load(Ordering::SeqCst),
        0,
        "augmenter 的输入应该是裸请求（mcp_servers 为空），它自己负责补齐"
    );
}

#[tokio::test]
async fn auto_resume_prompt_does_not_induce_recap() {
    // 文案审计：AUTO_RESUME_PROMPT 不应包含会让 LLM 切换到 "先总结再动作"
    // 模式的关键词。这条测试把"不要用这些词"写成可执行契约。
    let prompt = msg::AUTO_RESUME_PROMPT;
    let recap_triggers = ["上一轮执行结束", "请总结", "请回顾", "请汇报"];
    for trigger in recap_triggers {
        assert!(
            !prompt.contains(trigger),
            "AUTO_RESUME_PROMPT 出现了 recap 触发词 `{trigger}`：\n{prompt}"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────
// PR 7c · auto-resume 限流在 hub 侧
//
// 锁定契约：turn_processor 只发"请求 auto-resume"信号，
// 计数 + 上限判定在 `hub.request_hook_auto_resume` 原子区内完成。
// MAX_HOOK_AUTO_RESUMES = 2 → 前两次允许，第三次起拒绝。
// ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn request_hook_auto_resume_enforces_cap() {
    use tokio::sync::broadcast;

    let base = tempfile::tempdir().expect("tempdir");
    let hub = test_hub(base.path().to_path_buf(), Arc::new(EmptyConnector), None);
    let session = hub.create_session("auto-resume-cap").await.expect("create");

    // 手工注入 SessionRuntime（create_session 只建 meta），模拟 turn 已经起过后
    // processor 发起 auto-resume 的场景。
    {
        let mut sessions = hub.sessions.lock().await;
        let (tx, _rx) = broadcast::channel(16);
        sessions.insert(
            session.id.clone(),
            super::super::hub_support::build_session_runtime(tx),
        );
    }

    // 第 1 次：允许，计数 0 → 1
    assert!(
        hub.request_hook_auto_resume(session.id.clone()).await,
        "首次 auto-resume 应被允许"
    );
    // 第 2 次：允许，计数 1 → 2
    assert!(
        hub.request_hook_auto_resume(session.id.clone()).await,
        "第二次 auto-resume 应被允许"
    );
    // 第 3 次：计数已到上限 2，拒绝
    assert!(
        !hub.request_hook_auto_resume(session.id.clone()).await,
        "达到上限后第三次 auto-resume 应被拒绝"
    );
    // 第 4 次：继续拒绝，不应递增
    assert!(
        !hub.request_hook_auto_resume(session.id.clone()).await,
        "超过上限后应持续拒绝"
    );

    // 验证计数确实停在上限而不是溢出
    let sessions = hub.sessions.lock().await;
    let runtime = sessions
        .get(&session.id)
        .expect("session runtime should exist");
    assert_eq!(
        runtime.hook_auto_resume_count, 2,
        "hook_auto_resume_count 应停在 MAX_HOOK_AUTO_RESUMES (2)"
    );
}

#[tokio::test]
async fn request_hook_auto_resume_returns_false_for_unknown_session() {
    let base = tempfile::tempdir().expect("tempdir");
    let hub = test_hub(base.path().to_path_buf(), Arc::new(EmptyConnector), None);

    // 完全不存在的 session_id
    assert!(
        !hub.request_hook_auto_resume("nonexistent".to_string())
            .await,
        "未知 session 应返回 false（防止 schedule 僵尸 auto-resume）"
    );
}
