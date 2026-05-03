//! `SessionHub` 行为测试（从原 `hub.rs` 迁移；PR 6 拆分）。

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use agentdash_protocol::{ContentBlock, TextContent};
use agentdash_protocol::codex_app_server_protocol as codex;
use agentdash_protocol::{BackboneEnvelope, BackboneEvent, PlatformEvent, SourceInfo, TraceInfo};
use agentdash_spi::hooks::{
    ExecutionHookProvider, HookEvaluationQuery, HookResolution, HookTrigger,
    SessionHookRefreshQuery, SessionHookSnapshot, SessionHookSnapshotQuery,
};
use agentdash_spi::{AgentConnector, ConnectorError, PromptPayload, StopReason};
use futures::stream;
use serde_json::json;
use tokio::sync::{Mutex as TokioMutex, mpsc};
use tokio_stream::wrappers::ReceiverStream;

use super::super::MemorySessionPersistence;
use super::super::hook_messages as msg;
use super::super::hub_support::{
    build_user_message_envelopes, parse_turn_terminal_event_from_envelope,
};
use super::super::local_workspace_vfs;
use super::super::types::{
    HookSnapshotReloadTrigger, PromptSessionRequest, SessionBootstrapState, SessionExecutionState,
    UserPromptInput,
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
        vfs: None,
        flow_capabilities: None,
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
    let session_mcp = agentdash_spi::SessionMcpServer {
        name: "relay_tools".to_string(),
        transport: agentdash_spi::McpTransportConfig::Http {
            url: "http://127.0.0.1:19090/mcp".to_string(),
            headers: vec![],
        },
        uses_relay: true,
    };
    let flow_caps =
        agentdash_spi::FlowCapabilities::from_clusters([agentdash_spi::ToolCluster::Workflow]);

    let mut req = simple_prompt_request("hello");
    req.vfs = Some(local_workspace_vfs(workspace.path()));
    req.user_input.working_dir = Some("src".to_string());
    req.mcp_servers = vec![session_mcp.clone()];
    req.flow_capabilities = Some(flow_caps.clone());

    hub.start_prompt(&session.id, req)
        .await
        .expect("prompt should start");

    let sessions = hub.sessions.lock().await;
    let turn = sessions
        .get(&session.id)
        .and_then(|runtime| runtime.turn_state.active_turn())
        .expect("current turn execution state");
    assert_eq!(turn.session_frame.mcp_servers.len(), 1);
    assert_eq!(turn.session_frame.mcp_servers[0].name, "relay_tools");
    assert!(turn.session_frame.mcp_servers[0].uses_relay);
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

fn test_source() -> SourceInfo {
    SourceInfo {
        connector_id: "unit-test".to_string(),
        connector_type: "local_executor".to_string(),
        executor_id: None,
    }
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

    let source = SourceInfo {
        connector_id: "unit-test".to_string(),
        connector_type: "local_executor".to_string(),
        executor_id: Some("CLAUDE_CODE".to_string()),
    };

    let envelopes = build_user_message_envelopes("sess-test", &source, "t100", &blocks);
    assert_eq!(envelopes.len(), 2);

    assert_eq!(envelopes[0].trace.turn_id.as_deref(), Some("t100"));
    assert_eq!(envelopes[0].trace.entry_index, Some(0));
    assert_eq!(envelopes[1].trace.entry_index, Some(1));
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
            event.session_update_type == "platform_event"
                && event
                    .notification
                    .event
                    .as_ref()
                    .is_platform_session_meta_update("companion_human_response")
        })
        .expect("response event should exist");

    assert_eq!(response.turn_id.as_deref(), Some("turn-1"));
}

/// Trait extension for BackboneEvent to check platform event keys.
trait BackboneEventExt {
    fn as_ref(&self) -> &BackboneEvent;
    fn is_platform_session_meta_update(&self, key: &str) -> bool {
        matches!(
            self.as_ref(),
            BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate { key: k, .. }) if k == key
        )
    }
}

impl BackboneEventExt for BackboneEvent {
    fn as_ref(&self) -> &BackboneEvent {
        self
    }
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
async fn render_system_context_markdown_strips_owner_resource_blocks() {
    let persistence = Arc::new(MemorySessionPersistence::default());
    let base = tempfile::tempdir().expect("tempdir");
    let hub = SessionHub::new_with_hooks_and_persistence(
        Some(local_workspace_vfs(&base.path().to_path_buf())),
        Arc::new(SessionStartAwareConnector::default()),
        None,
        persistence,
    );
    let session = hub.create_session("test").await.expect("create session");

    let source = SourceInfo {
        connector_id: "test".to_string(),
        connector_type: "unit".to_string(),
        executor_id: None,
    };
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
    for envelope in build_user_message_envelopes(&session.id, &source, "t-1", &user_blocks) {
        hub.inject_notification(&session.id, envelope)
            .await
            .expect("inject user notification");
    }

    hub.inject_notification(
        &session.id,
        BackboneEnvelope::new(
            BackboneEvent::AgentMessageDelta(codex::AgentMessageDeltaNotification {
                delta: "已记录历史".to_string(),
                thread_id: session.id.clone(),
                turn_id: "t-1".to_string(),
                item_id: "assistant-msg-1".to_string(),
            }),
            session.id.clone(),
            source.clone(),
        )
        .with_trace(TraceInfo {
            turn_id: Some("t-1".to_string()),
            entry_index: Some(99),
        }),
    )
    .await
    .expect("inject assistant notification");

    let transcript = hub
        .build_projected_transcript(&session.id)
        .await
        .expect("transcript should build");
    let context = super::super::continuation::render_system_context_markdown(
        &transcript, Some("## Owner\nproject"),
    )
    .expect("continuation context should exist");
    assert!(context.contains("继续分析 session 生命周期"));
    assert!(context.contains("已记录历史"));
    assert!(context.contains("## Owner"));
    assert!(!context.contains("agentdash://project-context/"));
    assert!(!context.contains("hidden"));
}

#[tokio::test]
async fn build_projected_transcript_reconstructs_tool_history_without_owner_blocks() {
    let persistence = Arc::new(MemorySessionPersistence::default());
    let base = tempfile::tempdir().expect("tempdir");
    let hub = SessionHub::new_with_hooks_and_persistence(
        Some(local_workspace_vfs(&base.path().to_path_buf())),
        Arc::new(SessionStartAwareConnector::default()),
        None,
        persistence,
    );
    let session = hub.create_session("test").await.expect("create session");

    let source = SourceInfo {
        connector_id: "test".to_string(),
        connector_type: "unit".to_string(),
        executor_id: None,
    };
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
    for envelope in build_user_message_envelopes(&session.id, &source, "t-1", &user_blocks) {
        hub.inject_notification(&session.id, envelope)
            .await
            .expect("inject user notification");
    }

    hub.inject_notification(
        &session.id,
        BackboneEnvelope::new(
            BackboneEvent::AgentMessageDelta(codex::AgentMessageDeltaNotification {
                delta: "已记录历史".to_string(),
                thread_id: session.id.clone(),
                turn_id: "t-1".to_string(),
                item_id: "assistant-msg-1".to_string(),
            }),
            session.id.clone(),
            source.clone(),
        )
        .with_trace(TraceInfo {
            turn_id: Some("t-1".to_string()),
            entry_index: Some(1),
        }),
    )
    .await
    .expect("inject assistant notification");

    let item_started = codex::ThreadItem::DynamicToolCall {
        id: "tool-1".to_string(),
        tool: "shell_exec".to_string(),
        arguments: serde_json::json!({ "command": "pwd" }),
        status: codex::DynamicToolCallStatus::InProgress,
        content_items: None,
        success: None,
        duration_ms: None,
    };
    hub.inject_notification(
        &session.id,
        BackboneEnvelope::new(
            BackboneEvent::ItemStarted(codex::ItemStartedNotification {
                item: item_started,
                thread_id: session.id.clone(),
                turn_id: "t-1".to_string(),
            }),
            session.id.clone(),
            source.clone(),
        )
        .with_trace(TraceInfo {
            turn_id: Some("t-1".to_string()),
            entry_index: Some(1),
        }),
    )
    .await
    .expect("inject tool call");

    let item_completed = codex::ThreadItem::DynamicToolCall {
        id: "tool-1".to_string(),
        tool: "shell_exec".to_string(),
        arguments: serde_json::json!({ "command": "pwd" }),
        status: codex::DynamicToolCallStatus::Completed,
        content_items: Some(vec![codex::DynamicToolCallOutputContentItem::InputText {
            text: "workspace root".to_string(),
        }]),
        success: Some(true),
        duration_ms: None,
    };
    hub.inject_notification(
        &session.id,
        BackboneEnvelope::new(
            BackboneEvent::ItemCompleted(codex::ItemCompletedNotification {
                item: item_completed,
                thread_id: session.id.clone(),
                turn_id: "t-1".to_string(),
            }),
            session.id.clone(),
            source.clone(),
        )
        .with_trace(TraceInfo {
            turn_id: Some("t-1".to_string()),
            entry_index: Some(1),
        }),
    )
    .await
    .expect("inject tool update");

    let transcript = hub
        .build_projected_transcript(&session.id)
        .await
        .expect("transcript should build");
    let messages = transcript.into_messages();
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
            is_error,
            ..
        } => {
            assert_eq!(tool_call_id, "tool-1");
            assert_eq!(tool_name.as_deref(), Some("shell_exec"));
            assert_eq!(messages[2].first_text(), Some("workspace root"));
            assert!(!*is_error);
        }
        other => panic!("unexpected tool result: {other:?}"),
    }
}

fn inject_user_message_envelope(
    session_id: &str,
    turn_id: &str,
    entry_index: u32,
    text: &str,
) -> BackboneEnvelope {
    let source = SourceInfo {
        connector_id: "test".to_string(),
        connector_type: "unit".to_string(),
        executor_id: None,
    };
    BackboneEnvelope::new(
        BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
            key: "user_message_chunk".to_string(),
            value: serde_json::to_value(ContentBlock::Text(TextContent::new(text)))
                .unwrap_or_default(),
        }),
        session_id,
        source,
    )
    .with_trace(TraceInfo {
        turn_id: Some(turn_id.to_string()),
        entry_index: Some(entry_index),
    })
}

fn inject_compaction_envelope(
    session_id: &str,
    turn_id: &str,
    data: serde_json::Value,
) -> BackboneEnvelope {
    let source = SourceInfo {
        connector_id: "test".to_string(),
        connector_type: "unit".to_string(),
        executor_id: None,
    };
    BackboneEnvelope::new(
        BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
            key: "context_compacted".to_string(),
            value: data,
        }),
        session_id,
        source,
    )
    .with_turn_id(turn_id)
}

#[tokio::test]
async fn build_projected_transcript_applies_latest_compaction_checkpoint() {
    let persistence = Arc::new(MemorySessionPersistence::default());
    let base = tempfile::tempdir().expect("tempdir");
    let hub = SessionHub::new_with_hooks_and_persistence(
        Some(local_workspace_vfs(&base.path().to_path_buf())),
        Arc::new(SessionStartAwareConnector::default()),
        None,
        persistence,
    );
    let session = hub.create_session("test").await.expect("create session");

    for (turn_id, entry_index, text) in [
        ("t-1", 0_u32, "历史用户消息 1"),
        ("t-2", 0_u32, "历史用户消息 2"),
        ("t-3", 0_u32, "最近用户消息"),
    ] {
        hub.inject_notification(
            &session.id,
            inject_user_message_envelope(&session.id, turn_id, entry_index, text),
        )
        .await
        .expect("inject user notification");
    }

    hub.inject_notification(
        &session.id,
        inject_compaction_envelope(
            &session.id,
            "t-3",
            serde_json::json!({
                "summary": "## 历史摘要\n- 已完成旧分析",
                "tokens_before": 42000,
                "messages_compacted": 2,
                "newly_compacted_messages": 2,
                "timestamp_ms": 1710000000000_u64,
            }),
        ),
    )
    .await
    .expect("inject compaction checkpoint");

    let transcript = hub
        .build_projected_transcript(&session.id)
        .await
        .expect("transcript should build");
    let restored = transcript.into_messages();

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
async fn render_system_context_markdown_uses_compacted_projection() {
    let persistence = Arc::new(MemorySessionPersistence::default());
    let base = tempfile::tempdir().expect("tempdir");
    let hub = SessionHub::new_with_hooks_and_persistence(
        Some(local_workspace_vfs(&base.path().to_path_buf())),
        Arc::new(SessionStartAwareConnector::default()),
        None,
        persistence,
    );
    let session = hub.create_session("test").await.expect("create session");

    for (turn_id, entry_index, text) in [
        ("t-1", 0_u32, "第一段旧历史"),
        ("t-2", 0_u32, "第二段旧历史"),
        ("t-3", 0_u32, "保留的新历史"),
    ] {
        hub.inject_notification(
            &session.id,
            inject_user_message_envelope(&session.id, turn_id, entry_index, text),
        )
        .await
        .expect("inject user notification");
    }

    hub.inject_notification(
        &session.id,
        inject_compaction_envelope(
            &session.id,
            "t-3",
            serde_json::json!({
                "summary": "压缩后的历史摘要",
                "tokens_before": 38000,
                "messages_compacted": 2,
                "newly_compacted_messages": 2,
                "timestamp_ms": 1710000000000_u64,
            }),
        ),
    )
    .await
    .expect("inject compaction checkpoint");

    let transcript = hub
        .build_projected_transcript(&session.id)
        .await
        .expect("transcript should build");
    let context = super::super::continuation::render_system_context_markdown(
        &transcript, None,
    )
    .expect("continuation context should exist");

    assert!(context.contains("压缩后的历史摘要"));
    assert!(context.contains("保留的新历史"));
    assert!(!context.contains("第一段旧历史"));
    assert!(!context.contains("第二段旧历史"));
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
            _: &str,
            _: Option<PathBuf>,
        ) -> Result<futures::stream::BoxStream<'static, json_patch::Patch>, ConnectorError>
        {
            Ok(Box::pin(stream::empty()))
        }
        async fn prompt(
            &self,
            _: &str,
            _: Option<&str>,
            _: &PromptPayload,
            _: agentdash_spi::ExecutionContext,
        ) -> Result<agentdash_spi::ExecutionStream, ConnectorError> {
            Err(ConnectorError::Runtime(
                "connector setup failed".to_string(),
            ))
        }
        async fn cancel(&self, _: &str) -> Result<(), ConnectorError> {
            Ok(())
        }
        async fn approve_tool_call(&self, _: &str, _: &str) -> Result<(), ConnectorError> {
            Ok(())
        }
        async fn reject_tool_call(
            &self,
            _: &str,
            _: &str,
            _: Option<String>,
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
        .filter_map(|event| parse_turn_terminal_event_from_envelope(&event.notification))
        .last()
        .expect("terminal event should exist");
    assert_eq!(
        terminal.1,
        super::super::hub_support::TurnTerminalKind::Failed
    );
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
            TokioMutex<HashMap<String, mpsc::Sender<Result<BackboneEnvelope, ConnectorError>>>>,
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
            _: &str,
            _: Option<PathBuf>,
        ) -> Result<futures::stream::BoxStream<'static, json_patch::Patch>, ConnectorError>
        {
            Ok(Box::pin(stream::empty()))
        }
        async fn prompt(
            &self,
            session_id: &str,
            _: Option<&str>,
            _: &PromptPayload,
            _: agentdash_spi::ExecutionContext,
        ) -> Result<agentdash_spi::ExecutionStream, ConnectorError> {
            let (tx, rx) = mpsc::channel(4);
            self.streams.lock().await.insert(session_id.to_string(), tx);
            Ok(Box::pin(ReceiverStream::new(rx)))
        }
        async fn cancel(&self, session_id: &str) -> Result<(), ConnectorError> {
            self.streams.lock().await.remove(session_id);
            Ok(())
        }
        async fn approve_tool_call(&self, _: &str, _: &str) -> Result<(), ConnectorError> {
            Ok(())
        }
        async fn reject_tool_call(
            &self,
            _: &str,
            _: &str,
            _: Option<String>,
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
        .filter_map(|event| parse_turn_terminal_event_from_envelope(&event.notification))
        .last()
        .expect("terminal event should exist");
    assert_eq!(terminal.0, turn_id);
    assert_eq!(
        terminal.1,
        super::super::hub_support::TurnTerminalKind::Interrupted
    );
    assert_eq!(terminal.2.as_deref(), Some("执行已取消"));
}

// ─────────────────────────────────────────────────────────────────────
// Fail-lock: auto-resume 必须经过 PromptRequestAugmenter
// ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn launch_prompt_strict_requires_prompt_augmenter() {
    let base = tempfile::tempdir().expect("tempdir");
    let hub = test_hub(base.path().to_path_buf(), Arc::new(EmptyConnector), None);
    let session = hub.create_session("strict-launch").await.expect("create");

    let error = hub
        .launch_prompt_with_intent(
            &session.id,
            simple_prompt_request("hello"),
            super::super::launch_intent::SessionLaunchIntent::http_prompt(),
        )
        .await
        .expect_err("strict launch 应在 augmenter 缺失时失败");

    match error {
        ConnectorError::Runtime(message) => {
            assert!(
                message.contains("prompt_augmenter 未注入"),
                "错误信息应提示 augmenter 缺失，实际为: {message}"
            );
        }
        other => panic!("期望 Runtime 错误，实际为: {other}"),
    }
}

#[tokio::test]
async fn schedule_hook_auto_resume_strict_mode_requires_augmenter() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct PromptCountingConnector {
        prompt_calls: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl AgentConnector for PromptCountingConnector {
        fn connector_id(&self) -> &'static str {
            "prompt-counting"
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
            _: &str,
            _: Option<PathBuf>,
        ) -> Result<futures::stream::BoxStream<'static, json_patch::Patch>, ConnectorError>
        {
            Ok(Box::pin(stream::empty()))
        }
        async fn prompt(
            &self,
            _: &str,
            _: Option<&str>,
            _: &PromptPayload,
            _: agentdash_spi::ExecutionContext,
        ) -> Result<agentdash_spi::ExecutionStream, ConnectorError> {
            self.prompt_calls.fetch_add(1, Ordering::SeqCst);
            Ok(Box::pin(stream::empty()))
        }
        async fn cancel(&self, _: &str) -> Result<(), ConnectorError> {
            Ok(())
        }
        async fn approve_tool_call(&self, _: &str, _: &str) -> Result<(), ConnectorError> {
            Ok(())
        }
        async fn reject_tool_call(
            &self,
            _: &str,
            _: &str,
            _: Option<String>,
        ) -> Result<(), ConnectorError> {
            Ok(())
        }
    }

    let base = tempfile::tempdir().expect("tempdir");
    let prompt_calls = Arc::new(AtomicUsize::new(0));
    let hub = test_hub(
        base.path().to_path_buf(),
        Arc::new(PromptCountingConnector {
            prompt_calls: prompt_calls.clone(),
        }),
        None,
    );
    let session = hub.create_session("strict-auto-resume").await.expect("create");

    // 不注入 augmenter，strict auto-resume 应该在 launch 前失败，不能触发 connector.prompt。
    hub.schedule_hook_auto_resume(session.id.clone());
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    assert_eq!(
        prompt_calls.load(Ordering::SeqCst),
        0,
        "strict auto-resume 在 augmenter 缺失时不应触发 connector.prompt"
    );
}

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
            _: &str,
            _: Option<PathBuf>,
        ) -> Result<futures::stream::BoxStream<'static, json_patch::Patch>, ConnectorError>
        {
            Ok(Box::pin(stream::empty()))
        }
        async fn prompt(
            &self,
            _: &str,
            _: Option<&str>,
            _: &PromptPayload,
            _: agentdash_spi::ExecutionContext,
        ) -> Result<agentdash_spi::ExecutionStream, ConnectorError> {
            Err(ConnectorError::Runtime(
                "connector should not be reached if augmenter stopped".to_string(),
            ))
        }
        async fn cancel(&self, _: &str) -> Result<(), ConnectorError> {
            Ok(())
        }
        async fn approve_tool_call(&self, _: &str, _: &str) -> Result<(), ConnectorError> {
            Ok(())
        }
        async fn reject_tool_call(
            &self,
            _: &str,
            _: &str,
            _: Option<String>,
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

    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1500);
    while calls.load(Ordering::SeqCst) == 0 && std::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    }

    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let prompt_text = captured_prompt.lock().await.clone();
    let expected = msg::AUTO_RESUME_PROMPT.to_string();
    assert_eq!(prompt_text.as_deref(), Some(expected.as_str()));
    assert_eq!(captured_mcp_len.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn auto_resume_prompt_does_not_induce_recap() {
    let prompt = msg::AUTO_RESUME_PROMPT;
    let recap_triggers = ["上一轮执行结束", "请总结", "请回顾", "请汇报"];
    for trigger in recap_triggers {
        assert!(
            !prompt.contains(trigger),
            "AUTO_RESUME_PROMPT 出现了 recap 触发词 `{trigger}`：\n{prompt}"
        );
    }
}

#[tokio::test]
async fn request_hook_auto_resume_enforces_cap() {
    use tokio::sync::broadcast;

    let base = tempfile::tempdir().expect("tempdir");
    let hub = test_hub(base.path().to_path_buf(), Arc::new(EmptyConnector), None);
    let session = hub.create_session("auto-resume-cap").await.expect("create");

    {
        let mut sessions = hub.sessions.lock().await;
        let (tx, _rx) = broadcast::channel(16);
        sessions.insert(
            session.id.clone(),
            super::super::hub_support::build_session_runtime(tx),
        );
    }

    assert!(hub.request_hook_auto_resume(session.id.clone()).await);
    assert!(hub.request_hook_auto_resume(session.id.clone()).await);
    assert!(!hub.request_hook_auto_resume(session.id.clone()).await);
    assert!(!hub.request_hook_auto_resume(session.id.clone()).await);

    let sessions = hub.sessions.lock().await;
    let runtime = sessions
        .get(&session.id)
        .expect("session runtime should exist");
    assert_eq!(runtime.hook_auto_resume_count, 2);
}

#[tokio::test]
async fn request_hook_auto_resume_returns_false_for_unknown_session() {
    let base = tempfile::tempdir().expect("tempdir");
    let hub = test_hub(base.path().to_path_buf(), Arc::new(EmptyConnector), None);

    assert!(
        !hub.request_hook_auto_resume("nonexistent".to_string())
            .await,
    );
}
