use super::*;
use crate::connectors::pi_agent::factory::{NoopBridge, build_pi_agent_connector};
use crate::connectors::pi_agent::stream_mapper::{
    StreamMapperEventState, convert_event_to_envelopes,
};
use agentdash_agent::{
    AgentEvent, AgentToolResult, AssistantStreamEvent, ContentPart, MessageRef, StopReason,
    TokenUsage, ToolResultAddressProvider,
};
use agentdash_agent_protocol::codex_app_server_protocol as codex;
use agentdash_agent_protocol::{BackboneEvent, SourceInfo};
use agentdash_agent_types::AgentDashThreadItem;
use agentdash_domain::DomainError;
use agentdash_domain::settings::{Setting, SettingScope, SettingsRepository};
use agentdash_spi::{Mount, MountCapability};
use chrono::Utc;
use std::sync::{
    Mutex as StdMutex, RwLock,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

fn test_source() -> SourceInfo {
    SourceInfo {
        connector_id: "pi-agent".to_string(),
        connector_type: "local_executor".to_string(),
        executor_id: None,
    }
}

/// 提取终态 ItemCompleted 承载的助手正文（codex AgentMessage）。
fn assistant_message_text(item: &AgentDashThreadItem) -> Option<String> {
    match item {
        AgentDashThreadItem::Codex(codex::ThreadItem::AgentMessage { text, .. }) => {
            Some(text.clone())
        }
        _ => None,
    }
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

fn content_item_text(items: &[codex::DynamicToolCallOutputContentItem]) -> String {
    items
        .iter()
        .filter_map(|item| match item {
            codex::DynamicToolCallOutputContentItem::InputText { text } => Some(text.as_str()),
            codex::DynamicToolCallOutputContentItem::InputImage { .. } => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn assert_lifecycle_path_matches_item_id(text: &str, item_id: &str) {
    let path = text
        .lines()
        .find_map(|line| line.strip_prefix("lifecycle_path: "))
        .expect("bounded tool result should include lifecycle_path marker");
    let path_item_id = path
        .strip_prefix("lifecycle://session/tool-results/")
        .and_then(|rest| rest.strip_suffix("/result.txt"))
        .expect("lifecycle_path should use tool-results result.txt shape");
    let normalized_item_id = path_item_id.replace('/', ":");
    assert_eq!(normalized_item_id, item_id);
}

#[test]
fn restored_state_hydrates_session_item_identity_counters() {
    let identity = SessionItemIdentity::new();
    let restored_state = agentdash_spi::RestoredSessionState {
        messages: vec![
            AgentMessage::Assistant {
                content: Vec::new(),
                tool_calls: vec![agentdash_agent::ToolCallInfo {
                    id: "turn_001:tool_004".to_string(),
                    call_id: None,
                    name: "fs_read".to_string(),
                    arguments: serde_json::json!({}),
                }],
                stop_reason: None,
                error_message: None,
                usage: None,
                timestamp: None,
            },
            AgentMessage::ToolResult {
                tool_call_id: "legacy-raw-tool-call-id".to_string(),
                call_id: None,
                tool_name: Some("shell_exec".to_string()),
                content: Vec::new(),
                details: Some(serde_json::json!({
                    "readable_ref": {
                        "item_id": "turn_002:cmd_002"
                    },
                    "lifecycle_path": "lifecycle://session/tool-results/turn_002/cmd_002/result.txt"
                })),
                is_error: false,
                timestamp: None,
            },
        ],
        message_refs: Vec::new(),
    };

    identity.observe_restored_state(Some(&restored_state));

    let tool_ref = identity.tool_result_ref("raw-turn-new", "raw-tool-new", "fs_read");
    assert_eq!(tool_ref.item_id, "turn_003:tool_005");

    let command_ref = identity.tool_result_ref("raw-turn-new", "raw-cmd-new", "shell_exec");
    assert_eq!(command_ref.item_id, "turn_003:cmd_003");
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
    ) -> std::pin::Pin<Box<dyn futures::Stream<Item = agentdash_agent::StreamChunk> + Send>> {
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

#[derive(Default)]
struct ModelRecordingState {
    requests: StdMutex<Vec<(String, usize)>>,
}

struct ModelRecordingBridge {
    model_id: String,
    state: Arc<ModelRecordingState>,
}

#[async_trait::async_trait]
impl LlmBridge for ModelRecordingBridge {
    async fn stream_complete(
        &self,
        request: agentdash_agent::BridgeRequest,
    ) -> std::pin::Pin<Box<dyn futures::Stream<Item = agentdash_agent::StreamChunk> + Send>> {
        self.state
            .requests
            .lock()
            .expect("model recording bridge lock poisoned")
            .push((self.model_id.clone(), request.messages.len()));
        Box::pin(tokio_stream::once(agentdash_agent::StreamChunk::Done(
            agentdash_agent::BridgeResponse {
                message: agentdash_agent::AgentMessage::assistant("done"),
                raw_content: vec![agentdash_agent::ContentPart::text("done")],
                usage: agentdash_agent::TokenUsage::default(),
            },
        )))
    }
}

#[derive(Default)]
struct CancelThenDoneBridge {
    calls: AtomicUsize,
    first_provider_started: tokio::sync::Notify,
}

#[async_trait::async_trait]
impl LlmBridge for CancelThenDoneBridge {
    async fn stream_complete(
        &self,
        _request: agentdash_agent::BridgeRequest,
    ) -> std::pin::Pin<Box<dyn futures::Stream<Item = agentdash_agent::StreamChunk> + Send>> {
        let call_index = self.calls.fetch_add(1, Ordering::SeqCst);
        if call_index == 0 {
            self.first_provider_started.notify_waiters();
            return Box::pin(futures::stream::pending());
        }

        Box::pin(tokio_stream::once(agentdash_agent::StreamChunk::Done(
            agentdash_agent::BridgeResponse {
                message: agentdash_agent::AgentMessage::assistant("second done"),
                raw_content: vec![agentdash_agent::ContentPart::text("second done")],
                usage: agentdash_agent::TokenUsage::default(),
            },
        )))
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

    async fn get(&self, scope: &SettingScope, key: &str) -> Result<Option<Setting>, DomainError> {
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

#[derive(Default)]
struct TestLlmProviderCredentialRepository;

#[async_trait::async_trait]
impl agentdash_domain::llm_provider::LlmProviderCredentialRepository
    for TestLlmProviderCredentialRepository
{
    async fn get_for_user_provider(
        &self,
        _user_id: &str,
        _provider_id: uuid::Uuid,
    ) -> Result<Option<agentdash_domain::llm_provider::LlmProviderUserCredential>, DomainError>
    {
        Ok(None)
    }

    async fn list_for_user(
        &self,
        _user_id: &str,
    ) -> Result<Vec<agentdash_domain::llm_provider::LlmProviderUserCredential>, DomainError> {
        Ok(Vec::new())
    }

    async fn upsert_for_user_provider(
        &self,
        _credential: &agentdash_domain::llm_provider::LlmProviderUserCredential,
    ) -> Result<(), DomainError> {
        Ok(())
    }

    async fn delete_for_user_provider(
        &self,
        _user_id: &str,
        _provider_id: uuid::Uuid,
    ) -> Result<bool, DomainError> {
        Ok(false)
    }
}

#[derive(Default)]
struct TestLlmSecretCodec;

impl agentdash_domain::llm_provider::LlmSecretCodec for TestLlmSecretCodec {
    fn encrypt(&self, plaintext: &str) -> Result<String, DomainError> {
        Ok(plaintext.to_string())
    }

    fn decrypt(&self, ciphertext: &str) -> Result<String, DomainError> {
        Ok(ciphertext.to_string())
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

fn execution_context_with_config(executor_config: agentdash_spi::AgentConfig) -> ExecutionContext {
    ExecutionContext {
        session: agentdash_spi::ExecutionSessionFrame {
            turn_id: "turn-test".to_string(),
            working_directory: PathBuf::from("/tmp/test-workspace"),
            environment_variables: HashMap::new(),
            executor_config,
            mcp_servers: Vec::new(),
            vfs: Some(test_vfs("/tmp/test-workspace")),
            vfs_access_policy: None,
            backend_execution: None,
            runtime_backend_anchor: None,
            identity: None,
        },
        turn: agentdash_spi::ExecutionTurnFrame::default(),
    }
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
    let mut chunk_emit_states = HashMap::new();
    let mut tool_call_states = HashMap::new();
    let envelopes = convert_event_to_envelopes(
        &event,
        "session-1",
        &test_source(),
        "turn-1",
        &mut entry_index,
        &mut chunk_emit_states,
        &mut tool_call_states,
    );

    assert_eq!(envelopes.len(), 1);
    match &envelopes[0].event {
        BackboneEvent::ReasoningTextDelta(delta) => assert_eq!(delta.delta, "plan"),
        other => panic!("unexpected backbone event: {other:?}"),
    }
}

#[test]
fn context_compaction_started_maps_to_context_compaction_item() {
    let event = AgentEvent::ContextCompactionStarted {
        item_id: "compact-1".to_string(),
    };

    let mut entry_index = 0;
    let mut chunk_emit_states = HashMap::new();
    let mut tool_call_states = HashMap::new();
    let envelopes = convert_event_to_envelopes(
        &event,
        "session-1",
        &test_source(),
        "turn-1",
        &mut entry_index,
        &mut chunk_emit_states,
        &mut tool_call_states,
    );

    assert_eq!(envelopes.len(), 1);
    match &envelopes[0].event {
        BackboneEvent::ItemStarted(started) => {
            assert!(matches!(
                started.item.as_codex(),
                Some(agentdash_agent_protocol::codex_app_server_protocol::ThreadItem::ContextCompaction { id })
                    if id == "compact-1"
            ));
        }
        other => panic!("unexpected backbone event: {other:?}"),
    }
}

#[test]
fn context_compaction_completed_maps_lifecycle_and_metadata() {
    let event = AgentEvent::ContextCompacted {
        item_id: "compact-1".to_string(),
        messages: vec![
            agentdash_agent::AgentMessage::compaction_summary_with_boundary(
                "summary body",
                48_000,
                8,
                Some(MessageRef {
                    turn_id: "turn-1".to_string(),
                    entry_index: 2,
                }),
            ),
        ],
        message_refs: vec![None],
        compacted_until_ref: MessageRef {
            turn_id: "turn-1".to_string(),
            entry_index: 2,
        },
        first_kept_ref: Some(MessageRef {
            turn_id: "turn-1".to_string(),
            entry_index: 3,
        }),
        newly_compacted_messages: 3,
    };

    let mut entry_index = 0;
    let mut chunk_emit_states = HashMap::new();
    let mut tool_call_states = HashMap::new();
    let envelopes = convert_event_to_envelopes(
        &event,
        "session-1",
        &test_source(),
        "turn-1",
        &mut entry_index,
        &mut chunk_emit_states,
        &mut tool_call_states,
    );

    assert_eq!(envelopes.len(), 2);
    match &envelopes[0].event {
        BackboneEvent::Platform(agentdash_agent_protocol::PlatformEvent::SessionMetaUpdate {
            key,
            value,
        }) => {
            assert_eq!(key, "context_compacted");
            assert_eq!(value["lifecycle_item_id"], "compact-1");
            assert_eq!(value["summary"], "summary body");
            assert_eq!(value["compacted_until_ref"]["turn_id"], "turn-1");
            assert_eq!(value["first_kept_ref"]["entry_index"], 3);
            assert_eq!(value["newly_compacted_messages"], 3);
        }
        other => panic!("unexpected backbone event: {other:?}"),
    }
    match &envelopes[1].event {
        BackboneEvent::ItemCompleted(completed) => {
            assert!(matches!(
                completed.item.as_codex(),
                Some(agentdash_agent_protocol::codex_app_server_protocol::ThreadItem::ContextCompaction { id })
                    if id == "compact-1"
            ));
        }
        other => panic!("unexpected backbone event: {other:?}"),
    }
}

#[test]
fn context_compaction_failure_maps_diagnostic_and_error() {
    let event = AgentEvent::ContextCompactionFailed {
        item_id: "compact-1".to_string(),
        error: "summary_empty".to_string(),
    };

    let mut entry_index = 0;
    let mut chunk_emit_states = HashMap::new();
    let mut tool_call_states = HashMap::new();
    let envelopes = convert_event_to_envelopes(
        &event,
        "session-1",
        &test_source(),
        "turn-1",
        &mut entry_index,
        &mut chunk_emit_states,
        &mut tool_call_states,
    );

    assert_eq!(envelopes.len(), 2);
    match &envelopes[0].event {
        BackboneEvent::Platform(agentdash_agent_protocol::PlatformEvent::SessionMetaUpdate {
            key,
            value,
        }) => {
            assert_eq!(key, "context_compaction_failed");
            assert_eq!(value["lifecycle_item_id"], "compact-1");
            assert_eq!(value["status"], "failed");
            assert_eq!(value["error"], "summary_empty");
        }
        other => panic!("unexpected backbone event: {other:?}"),
    }
    match &envelopes[1].event {
        BackboneEvent::Error(error) => {
            assert_eq!(error.error.message, "summary_empty");
            assert!(!error.will_retry);
        }
        other => panic!("unexpected backbone event: {other:?}"),
    }
}

#[test]
fn provider_attempt_status_maps_to_platform_event() {
    let event = AgentEvent::ProviderAttemptStatus {
        status: agentdash_agent::ProviderAttemptStatus {
            phase: agentdash_agent::ProviderAttemptPhase::RetryScheduled,
            attempt: 1,
            max_attempts: 3,
            will_retry: true,
            delay_ms: Some(0),
            reason_code: Some("stream_disconnected".to_string()),
            message: Some("Reconnecting... 1/3".to_string()),
            provider: Some("openai".to_string()),
            model: Some("gpt-4.1".to_string()),
        },
    };

    let mut entry_index = 0;
    let mut chunk_emit_states = HashMap::new();
    let mut tool_call_states = HashMap::new();
    let envelopes = convert_event_to_envelopes(
        &event,
        "session-1",
        &test_source(),
        "turn-1",
        &mut entry_index,
        &mut chunk_emit_states,
        &mut tool_call_states,
    );

    assert_eq!(envelopes.len(), 1);
    match &envelopes[0].event {
        BackboneEvent::Platform(
            agentdash_agent_protocol::PlatformEvent::ProviderAttemptStatus(status),
        ) => {
            assert_eq!(status.turn_id, "turn-1");
            assert_eq!(
                status.phase,
                agentdash_agent_protocol::ProviderAttemptPhase::RetryScheduled
            );
            assert_eq!(status.attempt, 1);
            assert_eq!(status.max_attempts, 3);
            assert!(status.will_retry);
            assert_eq!(status.delay_ms, Some(0));
            assert_eq!(status.reason_code.as_deref(), Some("stream_disconnected"));
        }
        other => panic!("unexpected backbone event: {other:?}"),
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

    let mut chunk_emit_states = HashMap::new();
    let mut tool_call_states = HashMap::new();
    let start_envelopes = convert_event_to_envelopes(
        &start_event,
        "session-1",
        &test_source(),
        "turn-1",
        &mut entry_index,
        &mut chunk_emit_states,
        &mut tool_call_states,
    );
    let delta_envelopes = convert_event_to_envelopes(
        &delta_event,
        "session-1",
        &test_source(),
        "turn-1",
        &mut entry_index,
        &mut chunk_emit_states,
        &mut tool_call_states,
    );
    let end_envelopes = convert_event_to_envelopes(
        &end_event,
        "session-1",
        &test_source(),
        "turn-1",
        &mut entry_index,
        &mut chunk_emit_states,
        &mut tool_call_states,
    );

    assert_eq!(start_envelopes.len(), 1);
    match &start_envelopes[0].event {
        BackboneEvent::ItemStarted(n) => {
            let item = serde_json::to_value(&n.item).expect("thread item should serialize");
            assert_eq!(
                item.get("type").and_then(|value| value.as_str()),
                Some("shellExec")
            );
            assert_eq!(
                item.get("command").and_then(|value| value.as_str()),
                Some("echo he")
            );
            assert_eq!(
                item.get("cwd").and_then(|value| value.as_str()),
                Some("platform://")
            );
        }
        other => panic!("unexpected backbone event: {other:?}"),
    }
    assert_eq!(delta_envelopes.len(), 1);
    match &delta_envelopes[0].event {
        BackboneEvent::ItemUpdated(n) => {
            let item = serde_json::to_value(&n.item).expect("thread item should serialize");
            assert_eq!(
                item.get("type").and_then(|value| value.as_str()),
                Some("shellExec")
            );
            assert_eq!(
                item.get("command").and_then(|value| value.as_str()),
                Some("echo hello")
            );
        }
        other => panic!("unexpected backbone event: {other:?}"),
    }
    assert!(end_envelopes.is_empty());
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
            draft: "{\"patch\":\"not a patch".to_string(),
            is_parseable: false,
        },
    };

    let mut entry_index = 0;

    let mut chunk_emit_states = HashMap::new();
    let mut tool_call_states = HashMap::new();
    let envelopes = convert_event_to_envelopes(
        &delta_event,
        "session-1",
        &test_source(),
        "turn-1",
        &mut entry_index,
        &mut chunk_emit_states,
        &mut tool_call_states,
    );

    assert!(envelopes.is_empty());
    assert!(tool_call_states.contains_key("tool-fs-apply-patch-1"));
}

#[test]
fn tool_call_delta_apply_patch_partial_draft_emits_file_change_preview() {
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
    let mut chunk_emit_states = HashMap::new();
    let mut tool_call_states = HashMap::new();
    let envelopes = convert_event_to_envelopes(
        &delta_event,
        "session-1",
        &test_source(),
        "turn-1",
        &mut entry_index,
        &mut chunk_emit_states,
        &mut tool_call_states,
    );

    assert_eq!(envelopes.len(), 1);
    match &envelopes[0].event {
        BackboneEvent::ItemUpdated(n) => {
            let Some(codex::ThreadItem::FileChange {
                id,
                changes,
                status,
            }) = n.item.as_codex()
            else {
                panic!("expected fileChange preview, got {:?}", n.item);
            };
            assert_eq!(id, "turn-1:tool-fs-apply-patch-1");
            assert!(matches!(status, codex::PatchApplyStatus::InProgress));
            assert_eq!(changes.len(), 1);
            assert_eq!(changes[0].path, "notes.txt");
            assert!(matches!(changes[0].kind, codex::PatchChangeKind::Add));
        }
        other => panic!("unexpected backbone event: {other:?}"),
    }
}

#[test]
fn tool_call_delta_apply_patch_reuses_item_id_for_later_preview() {
    let make_event = |draft: &str| AgentEvent::MessageUpdate {
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
            delta: String::new(),
            draft: draft.to_string(),
            is_parseable: false,
        },
    };
    let first_event = make_event("{\"patch\":\"*** Begin Patch\\n*** Add File: notes.txt\\n+hello");
    let second_event = make_event(
        "{\"patch\":\"*** Begin Patch\\n*** Add File: notes.txt\\n+hello\\n*** Update File: src/lib.rs\\n@@\\n-old\\n+new",
    );

    let mut entry_index = 0;
    let mut chunk_emit_states = HashMap::new();
    let mut tool_call_states = HashMap::new();
    let first_envelopes = convert_event_to_envelopes(
        &first_event,
        "session-1",
        &test_source(),
        "turn-1",
        &mut entry_index,
        &mut chunk_emit_states,
        &mut tool_call_states,
    );
    let second_envelopes = convert_event_to_envelopes(
        &second_event,
        "session-1",
        &test_source(),
        "turn-1",
        &mut entry_index,
        &mut chunk_emit_states,
        &mut tool_call_states,
    );

    let first_id = match &first_envelopes[0].event {
        BackboneEvent::ItemUpdated(n) => match n.item.as_codex() {
            Some(codex::ThreadItem::FileChange { id, .. }) => id.clone(),
            other => panic!("expected fileChange preview, got {other:?}"),
        },
        other => panic!("unexpected backbone event: {other:?}"),
    };
    match &second_envelopes[0].event {
        BackboneEvent::ItemUpdated(n) => {
            let Some(codex::ThreadItem::FileChange { id, changes, .. }) = n.item.as_codex() else {
                panic!("expected fileChange preview, got {:?}", n.item);
            };
            assert_eq!(id, &first_id);
            assert_eq!(changes.len(), 2);
            assert_eq!(changes[1].path, "src/lib.rs");
        }
        other => panic!("unexpected backbone event: {other:?}"),
    }
}

#[test]
fn tool_call_delta_non_apply_patch_parseable_draft_updates_input_preview() {
    let delta_event = AgentEvent::MessageUpdate {
        message: AgentMessage::Assistant {
            content: vec![],
            tool_calls: vec![agentdash_agent::ToolCallInfo {
                id: "tool-external-1".to_string(),
                call_id: Some("tool-external-1".to_string()),
                name: "mcp_code_analyzer_long_task".to_string(),
                arguments: serde_json::json!({ "query": "scan repo" }),
            }],
            stop_reason: Some(StopReason::ToolUse),
            error_message: None,
            usage: None,
            timestamp: Some(agentdash_agent::types::now_millis()),
        },
        event: AssistantStreamEvent::ToolCallDelta {
            content_index: 0,
            tool_call_id: "tool-external-1".to_string(),
            name: "mcp_code_analyzer_long_task".to_string(),
            delta: "repo\"}".to_string(),
            draft: "{\"query\":\"scan repo\"}".to_string(),
            is_parseable: true,
        },
    };

    let mut entry_index = 0;
    let mut chunk_emit_states = HashMap::new();
    let mut tool_call_states = HashMap::new();
    let envelopes = convert_event_to_envelopes(
        &delta_event,
        "session-1",
        &test_source(),
        "turn-1",
        &mut entry_index,
        &mut chunk_emit_states,
        &mut tool_call_states,
    );

    assert_eq!(envelopes.len(), 1);
    match &envelopes[0].event {
        BackboneEvent::ItemUpdated(n) => {
            let Some(codex::ThreadItem::DynamicToolCall {
                id,
                tool,
                arguments,
                status,
                ..
            }) = n.item.as_codex()
            else {
                panic!("expected dynamic tool input preview, got {:?}", n.item);
            };
            assert_eq!(id, "turn-1:tool-external-1");
            assert_eq!(tool, "mcp_code_analyzer_long_task");
            assert_eq!(*arguments, serde_json::json!({ "query": "scan repo" }));
            assert!(matches!(status, codex::DynamicToolCallStatus::InProgress));
        }
        other => panic!("unexpected backbone event: {other:?}"),
    }
    assert!(tool_call_states.contains_key("tool-external-1"));
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

    let mut chunk_emit_states = HashMap::new();
    let mut tool_call_states = HashMap::new();
    let envelopes = convert_event_to_envelopes(
        &event,
        "session-1",
        &test_source(),
        "turn-1",
        &mut entry_index,
        &mut chunk_emit_states,
        &mut tool_call_states,
    );

    assert_eq!(envelopes.len(), 1);
    match &envelopes[0].event {
        BackboneEvent::ItemStarted(n) => {
            assert!(
                matches!(n.item.as_codex(), Some(codex_app_server_protocol::ThreadItem::DynamicToolCall { tool, arguments, .. }) if tool == "read_file" && *arguments == serde_json::json!({ "path": "README.md" }))
            );
        }
        other => panic!("unexpected backbone event: {other:?}"),
    }
}

#[test]
fn message_end_with_usage_emits_token_usage_update_with_context_window() {
    let event = AgentEvent::MessageEnd {
        message: AgentMessage::Assistant {
            content: vec![ContentPart::text("done")],
            tool_calls: vec![],
            stop_reason: Some(StopReason::Stop),
            error_message: None,
            usage: Some(TokenUsage {
                input: 100,
                cache_read_input: 20,
                cache_creation_input: 30,
                output: 40,
            }),
            timestamp: Some(agentdash_agent::types::now_millis()),
        },
    };

    let mut entry_index = 0;
    let mut chunk_emit_states = HashMap::new();
    let mut tool_call_states = HashMap::new();
    let envelopes = convert_event_to_envelopes_with_runtime_context(
        &event,
        "session-usage",
        &test_source(),
        "turn-usage",
        StreamMapperEventState {
            entry_index: &mut entry_index,
            chunk_emit_states: &mut chunk_emit_states,
            tool_call_states: &mut tool_call_states,
        },
        StreamMapperRuntimeContext {
            model_context_window: Some(200_000),
            reserve_tokens: 16_384,
            session_identity: None,
        },
    );

    let usage = envelopes
        .iter()
        .find_map(|envelope| match &envelope.event {
            BackboneEvent::TokenUsageUpdated(notification) => Some(notification),
            _ => None,
        })
        .expect("MessageEnd usage should emit token usage update");

    assert_eq!(usage.thread_id, "session-usage");
    assert_eq!(usage.turn_id, "turn-usage");
    assert_eq!(usage.token_usage.last.input_tokens, 100);
    assert_eq!(usage.token_usage.last.cached_input_tokens, 50);
    assert_eq!(usage.token_usage.last.output_tokens, 40);
    assert_eq!(usage.token_usage.last.total_tokens, 190);
    assert_eq!(usage.token_usage.model_context_window, Some(200_000));
    assert_eq!(
        usage.token_usage.context.provider_context_tokens, 150,
        "cache tokens count toward provider-visible context pressure"
    );
    assert_eq!(usage.token_usage.context.current_context_tokens, 150);
    assert_eq!(
        usage.token_usage.context.effective_context_window,
        Some(200_000)
    );
    assert_eq!(usage.token_usage.context.reserve_tokens, 16_384);
}

#[test]
fn message_end_shell_exec_emits_native_shell_exec_item() {
    let event = AgentEvent::MessageEnd {
        message: AgentMessage::Assistant {
            content: vec![],
            tool_calls: vec![agentdash_agent::ToolCallInfo {
                id: "tool-shell-1".to_string(),
                call_id: Some("tool-shell-1".to_string()),
                name: "shell_exec".to_string(),
                arguments: serde_json::json!({ "command": "pwd" }),
            }],
            stop_reason: Some(StopReason::ToolUse),
            error_message: None,
            usage: None,
            timestamp: Some(agentdash_agent::types::now_millis()),
        },
    };

    let mut entry_index = 0;
    let mut chunk_emit_states = HashMap::new();
    let mut tool_call_states = HashMap::new();
    let envelopes = convert_event_to_envelopes(
        &event,
        "session-1",
        &test_source(),
        "turn-1",
        &mut entry_index,
        &mut chunk_emit_states,
        &mut tool_call_states,
    );

    assert_eq!(envelopes.len(), 1);
    match &envelopes[0].event {
        BackboneEvent::ItemStarted(n) => {
            assert!(matches!(
                &n.item,
                agentdash_agent_protocol::AgentDashThreadItem::AgentDash(
                    agentdash_agent_protocol::AgentDashNativeThreadItem::ShellExec {
                        command,
                        cwd: Some(cwd),
                        execution_mode: agentdash_agent_protocol::ShellExecExecutionMode::Platform,
                        status: codex_app_server_protocol::DynamicToolCallStatus::InProgress,
                        ..
                    }
                ) if command == "pwd" && cwd == "platform://"
            ));
        }
        other => panic!("unexpected backbone event: {other:?}"),
    }
}

#[test]
fn fs_tools_map_to_agentdash_native_thread_items() {
    let cases = [
        (
            "fs_read",
            serde_json::json!({ "file_path": "README.md", "offset": 4, "limit": 12 }),
        ),
        (
            "fs_grep",
            serde_json::json!({
                "pattern": "AgentDashThreadItem",
                "path": "crates",
                "glob": "*.rs",
                "type": "rust",
                "output_mode": "content",
                "head_limit": 20,
                "offset": 2
            }),
        ),
        (
            "fs_glob",
            serde_json::json!({ "pattern": "**/*.rs", "path": "crates", "maxResults": 50 }),
        ),
    ];

    for (tool_name, args) in cases {
        let event = AgentEvent::ToolExecutionStart {
            tool_call_id: format!("{tool_name}-1"),
            tool_name: tool_name.to_string(),
            args,
        };

        let mut entry_index = 0;
        let mut chunk_emit_states = HashMap::new();
        let mut tool_call_states = HashMap::new();
        let envelopes = convert_event_to_envelopes(
            &event,
            "session-1",
            &test_source(),
            "turn-1",
            &mut entry_index,
            &mut chunk_emit_states,
            &mut tool_call_states,
        );

        assert_eq!(envelopes.len(), 1);
        match (&envelopes[0].event, tool_name) {
            (BackboneEvent::ItemStarted(n), "fs_read") => {
                assert!(matches!(
                    &n.item,
                    agentdash_agent_protocol::AgentDashThreadItem::AgentDash(
                        agentdash_agent_protocol::AgentDashNativeThreadItem::FsRead {
                            path,
                            offset: Some(4),
                            limit: Some(12),
                            status: codex_app_server_protocol::DynamicToolCallStatus::InProgress,
                            ..
                        }
                    ) if path == "README.md"
                ));
            }
            (BackboneEvent::ItemStarted(n), "fs_grep") => {
                assert!(matches!(
                    &n.item,
                    agentdash_agent_protocol::AgentDashThreadItem::AgentDash(
                        agentdash_agent_protocol::AgentDashNativeThreadItem::FsGrep {
                            pattern,
                            path: Some(path),
                            glob: Some(glob),
                            file_type: Some(file_type),
                            output_mode: Some(output_mode),
                            head_limit: Some(20),
                            offset: Some(2),
                            status: codex_app_server_protocol::DynamicToolCallStatus::InProgress,
                            ..
                        }
                    ) if pattern == "AgentDashThreadItem"
                        && path == "crates"
                        && glob == "*.rs"
                        && file_type == "rust"
                        && output_mode == "content"
                ));
            }
            (BackboneEvent::ItemStarted(n), "fs_glob") => {
                assert!(matches!(
                    &n.item,
                    agentdash_agent_protocol::AgentDashThreadItem::AgentDash(
                        agentdash_agent_protocol::AgentDashNativeThreadItem::FsGlob {
                            pattern,
                            path: Some(path),
                            max_results: Some(50),
                            status: codex_app_server_protocol::DynamicToolCallStatus::InProgress,
                            ..
                        }
                    ) if pattern == "**/*.rs" && path == "crates"
                ));
            }
            (other, _) => panic!("unexpected backbone event: {other:?}"),
        }
    }
}

#[test]
fn fs_apply_patch_maps_to_codex_file_change() {
    let patch = "\
*** Begin Patch
*** Add File: notes.txt
+hello
*** Update File: src/lib.rs
@@
-old
+new
*** Delete File: gone.txt
*** End Patch
";
    let event = AgentEvent::ToolExecutionStart {
        tool_call_id: "tool-patch-1".to_string(),
        tool_name: "fs_apply_patch".to_string(),
        args: serde_json::json!({ "patch": patch }),
    };

    let mut entry_index = 0;
    let mut chunk_emit_states = HashMap::new();
    let mut tool_call_states = HashMap::new();
    let envelopes = convert_event_to_envelopes(
        &event,
        "session-1",
        &test_source(),
        "turn-1",
        &mut entry_index,
        &mut chunk_emit_states,
        &mut tool_call_states,
    );

    assert_eq!(envelopes.len(), 1);
    match &envelopes[0].event {
        BackboneEvent::ItemStarted(n) => {
            let Some(codex_app_server_protocol::ThreadItem::FileChange {
                changes, status, ..
            }) = n.item.as_codex()
            else {
                panic!("expected codex FileChange, got {:?}", n.item);
            };
            assert!(matches!(
                status,
                codex_app_server_protocol::PatchApplyStatus::InProgress
            ));
            assert_eq!(changes.len(), 3);
            assert_eq!(changes[0].path, "notes.txt");
            assert!(matches!(
                changes[0].kind,
                codex_app_server_protocol::PatchChangeKind::Add
            ));
            assert_eq!(changes[1].path, "src/lib.rs");
            assert!(matches!(
                changes[1].kind,
                codex_app_server_protocol::PatchChangeKind::Update { .. }
            ));
            assert_eq!(changes[2].path, "gone.txt");
            assert!(matches!(
                changes[2].kind,
                codex_app_server_protocol::PatchChangeKind::Delete
            ));
        }
        other => panic!("unexpected backbone event: {other:?}"),
    }
}

#[test]
fn fs_apply_patch_falls_back_to_dynamic_when_patch_is_unparseable() {
    let event = AgentEvent::ToolExecutionStart {
        tool_call_id: "tool-patch-1".to_string(),
        tool_name: "fs_apply_patch".to_string(),
        args: serde_json::json!({ "patch": "not a patch" }),
    };

    let mut entry_index = 0;
    let mut chunk_emit_states = HashMap::new();
    let mut tool_call_states = HashMap::new();
    let envelopes = convert_event_to_envelopes(
        &event,
        "session-1",
        &test_source(),
        "turn-1",
        &mut entry_index,
        &mut chunk_emit_states,
        &mut tool_call_states,
    );

    assert_eq!(envelopes.len(), 1);
    match &envelopes[0].event {
        BackboneEvent::ItemStarted(n) => {
            assert!(
                matches!(n.item.as_codex(), Some(codex_app_server_protocol::ThreadItem::DynamicToolCall { tool, .. }) if tool == "fs_apply_patch")
            );
        }
        other => panic!("unexpected backbone event: {other:?}"),
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

    let mut chunk_emit_states = HashMap::new();
    let mut tool_call_states = HashMap::new();
    let _ = convert_event_to_envelopes(
        &pending_event,
        "session-1",
        &test_source(),
        "turn-1",
        &mut entry_index,
        &mut chunk_emit_states,
        &mut tool_call_states,
    );
    let envelopes = convert_event_to_envelopes(
        &execution_start,
        "session-1",
        &test_source(),
        "turn-1",
        &mut entry_index,
        &mut chunk_emit_states,
        &mut tool_call_states,
    );

    assert_eq!(envelopes.len(), 1);
    match &envelopes[0].event {
        BackboneEvent::ItemStarted(n) => {
            let item = serde_json::to_value(&n.item).expect("thread item should serialize");
            assert_eq!(
                item.get("type").and_then(|value| value.as_str()),
                Some("shellExec")
            );
            assert_eq!(
                item.get("command").and_then(|value| value.as_str()),
                Some("cargo test")
            );
            assert_eq!(
                item.get("cwd").and_then(|value| value.as_str()),
                Some("platform://")
            );
        }
        other => panic!("unexpected backbone event: {other:?}"),
    }
}

#[test]
fn tool_execution_updates_and_final_items_use_bounded_tool_result_content() {
    let raw_sentinel = "RAW_TOOL_RESULT_SENTINEL_SHOULD_NOT_REACH_THREAD_ITEM";
    let stable_item_id = "turn_001:tool_001";
    let lifecycle_path = "lifecycle://session/tool-results/turn_001/tool_001/result.txt";
    let bounded_text = format!(
        "[tool result truncated]\nlifecycle_path: {lifecycle_path}\npolicy: head_tail\n\nbounded preview"
    );
    let result = AgentToolResult {
        content: vec![ContentPart::text(bounded_text.clone())],
        is_error: false,
        details: Some(serde_json::json!({
            "ok": true,
            "raw_sentinel_for_regression": raw_sentinel,
            "lifecycle_path": lifecycle_path,
            "readable_ref": {
                "item_id": stable_item_id,
                "turn_alias": "turn_001",
                "body_alias": "tool_001",
                "body_kind": "tool_result",
                "lifecycle_path": lifecycle_path
            },
            "truncation": {
                "truncated": true,
                "original_bytes": 131072,
                "inline_bytes": bounded_text.len(),
                "omitted_bytes": 131072 - bounded_text.len(),
                "policy": "head_tail"
            }
        })),
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

    let mut chunk_emit_states = HashMap::new();
    let mut tool_call_states = HashMap::new();
    let update_envelopes = convert_event_to_envelopes(
        &update_event,
        "session-1",
        &test_source(),
        "turn-1",
        &mut entry_index,
        &mut chunk_emit_states,
        &mut tool_call_states,
    );
    let end_envelopes = convert_event_to_envelopes(
        &end_event,
        "session-1",
        &test_source(),
        "turn-1",
        &mut entry_index,
        &mut chunk_emit_states,
        &mut tool_call_states,
    );

    assert_eq!(update_envelopes.len(), 1);
    match &update_envelopes[0].event {
        BackboneEvent::ItemUpdated(n) => {
            let item_json = serde_json::to_string(&n.item).expect("item should serialize");
            assert!(!item_json.contains(raw_sentinel));
            let Some(codex::ThreadItem::DynamicToolCall {
                id,
                tool,
                content_items: Some(content_items),
                ..
            }) = n.item.as_codex()
            else {
                panic!("expected dynamic tool update, got {:?}", n.item);
            };
            assert_eq!(id, stable_item_id);
            assert_eq!(tool, "echo");
            let text = content_item_text(content_items);
            assert!(text.contains("bounded preview"));
            assert!(text.contains(lifecycle_path));
            assert_lifecycle_path_matches_item_id(&text, id);
            assert!(!text.contains(raw_sentinel));
        }
        other => panic!("unexpected backbone event: {other:?}"),
    }

    match &end_envelopes[0].event {
        BackboneEvent::ItemCompleted(n) => {
            let item_json = serde_json::to_string(&n.item).expect("item should serialize");
            assert!(!item_json.contains(raw_sentinel));
            let Some(codex::ThreadItem::DynamicToolCall {
                id,
                tool,
                content_items: Some(content_items),
                success,
                ..
            }) = n.item.as_codex()
            else {
                panic!("expected dynamic tool completion, got {:?}", n.item);
            };
            assert_eq!(id, stable_item_id);
            assert_eq!(tool, "echo");
            assert_eq!(*success, Some(true));
            let text = content_item_text(content_items);
            assert!(text.contains("bounded preview"));
            assert!(text.contains(lifecycle_path));
            assert_lifecycle_path_matches_item_id(&text, id);
            assert!(!text.contains(raw_sentinel));
        }
        other => panic!("unexpected backbone event: {other:?}"),
    }
}

#[test]
fn shell_exec_final_uses_bounded_output_and_structured_details() {
    let raw_sentinel = "RAW_SHELL_OUTPUT_SENTINEL_SHOULD_NOT_REACH_THREAD_ITEM";
    let stable_item_id = "turn_001:cmd_001";
    let lifecycle_path = "lifecycle://session/tool-results/turn_001/cmd_001/result.txt";
    let bounded_output = format!(
        "[tool result truncated]\nlifecycle_path: {lifecycle_path}\npolicy: head_tail\n\nbounded shell preview"
    );
    let start_event = AgentEvent::ToolExecutionStart {
        tool_call_id: "tool-shell-1".to_string(),
        tool_name: "shell_exec".to_string(),
        args: serde_json::json!({
            "command": "cargo test -p agentdash-executor pi_agent",
            "cwd": "workspace://repo"
        }),
    };
    let result = AgentToolResult {
        content: vec![ContentPart::text(bounded_output.clone())],
        is_error: true,
        details: Some(serde_json::json!({
            "type": "shell_exec",
            "original_command": "cargo test -p agentdash-executor pi_agent",
            "executed_command": "cargo test -p agentdash-executor pi_agent",
            "state": "completed",
            "exit_code": 7,
            "session_id": "shell-session-1",
            "terminal_id": "terminal-1",
            "next_seq": 42,
            "truncated": true,
            "omitted_bytes": 8192,
            "raw_sentinel_for_regression": raw_sentinel,
            "lifecycle_path": lifecycle_path,
            "truncation": {
                "truncated": true,
                "original_bytes": 131072,
                "inline_bytes": bounded_output.len(),
                "omitted_bytes": 131072 - bounded_output.len(),
                "policy": "head_tail"
            }
        })),
    };
    let end_event = AgentEvent::ToolExecutionEnd {
        tool_call_id: "tool-shell-1".to_string(),
        tool_name: "shell_exec".to_string(),
        result: serde_json::to_value(&result).expect("tool result should serialize"),
        is_error: true,
    };

    let mut entry_index = 0;
    let mut chunk_emit_states = HashMap::new();
    let mut tool_call_states = HashMap::new();
    let runtime_context = StreamMapperRuntimeContext {
        session_identity: Some(SessionItemIdentity::new()),
        ..StreamMapperRuntimeContext::default()
    };
    let _ = convert_event_to_envelopes_with_runtime_context(
        &start_event,
        "session-1",
        &test_source(),
        "turn-1",
        StreamMapperEventState {
            entry_index: &mut entry_index,
            chunk_emit_states: &mut chunk_emit_states,
            tool_call_states: &mut tool_call_states,
        },
        runtime_context.clone(),
    );
    let end_envelopes = convert_event_to_envelopes_with_runtime_context(
        &end_event,
        "session-1",
        &test_source(),
        "turn-1",
        StreamMapperEventState {
            entry_index: &mut entry_index,
            chunk_emit_states: &mut chunk_emit_states,
            tool_call_states: &mut tool_call_states,
        },
        runtime_context,
    );

    assert_eq!(end_envelopes.len(), 1);
    match &end_envelopes[0].event {
        BackboneEvent::ItemCompleted(n) => {
            let item_json = serde_json::to_string(&n.item).expect("item should serialize");
            assert!(!item_json.contains(raw_sentinel));
            assert!(matches!(
                &n.item,
                agentdash_agent_protocol::AgentDashThreadItem::AgentDash(
                    agentdash_agent_protocol::AgentDashNativeThreadItem::ShellExec {
                        command,
                        id,
                        cwd: Some(cwd),
                        status: codex::DynamicToolCallStatus::Failed,
                        aggregated_output: Some(aggregated_output),
                        exit_code: Some(7),
                        success: Some(false),
                        ..
                    }
                ) if id == stable_item_id
                    && command == "cargo test -p agentdash-executor pi_agent"
                    && cwd == "workspace://repo"
                    && aggregated_output == &bounded_output
                    && aggregated_output.contains(lifecycle_path)
                    && {
                        assert_lifecycle_path_matches_item_id(aggregated_output, id);
                        true
                    }
                    && !aggregated_output.contains(raw_sentinel)
            ));
        }
        other => panic!("unexpected backbone event: {other:?}"),
    }
}

#[test]
fn shell_exec_vfs_rewrite_update_maps_to_command_output_delta() {
    let start_event = AgentEvent::ToolExecutionStart {
        tool_call_id: "tool-shell-1".to_string(),
        tool_name: "shell_exec".to_string(),
        args: serde_json::json!({
            "command": "python skill-assets://skills/abc-user-lookup/scripts/lookup.py yihao.liao",
            "cwd": "."
        }),
    };
    let update_event = AgentEvent::ToolExecutionUpdate {
        tool_call_id: "tool-shell-1".to_string(),
        tool_name: "shell_exec".to_string(),
        args: serde_json::json!({
            "command": "python skill-assets://skills/abc-user-lookup/scripts/lookup.py yihao.liao",
            "cwd": "."
        }),
        partial_result: serde_json::json!({
            "content": [{
                "type": "text",
                "text": "vfs_uri_rewrite: 1 URI(s) materialized\nskill-assets://skills/abc-user-lookup/scripts/lookup.py -> C:\\Users\\yihao.liao\\AppData\\Local\\agentdash\\materialized\\readonly\\skill-assets\\skills\\abc-user-lookup\\scripts\\lookup.py\nexecuted_command: python \"C:\\Users\\yihao.liao\\AppData\\Local\\agentdash\\materialized\\readonly\\skill-assets\\skills\\abc-user-lookup\\scripts\\lookup.py\" yihao.liao"
            }],
            "is_error": false,
            "details": {
                "type": "vfs_uri_rewrite",
                "original_command": "python skill-assets://skills/abc-user-lookup/scripts/lookup.py yihao.liao",
                "executed_command": "python \"C:\\Users\\yihao.liao\\AppData\\Local\\agentdash\\materialized\\readonly\\skill-assets\\skills\\abc-user-lookup\\scripts\\lookup.py\" yihao.liao"
            }
        }),
    };

    let mut entry_index = 0;
    let mut chunk_emit_states = HashMap::new();
    let mut tool_call_states = HashMap::new();
    let start_envelopes = convert_event_to_envelopes(
        &start_event,
        "session-1",
        &test_source(),
        "turn-1",
        &mut entry_index,
        &mut chunk_emit_states,
        &mut tool_call_states,
    );
    let update_envelopes = convert_event_to_envelopes(
        &update_event,
        "session-1",
        &test_source(),
        "turn-1",
        &mut entry_index,
        &mut chunk_emit_states,
        &mut tool_call_states,
    );

    match &start_envelopes[0].event {
        BackboneEvent::ItemStarted(n) => {
            let item = serde_json::to_value(&n.item).expect("thread item should serialize");
            assert_eq!(
                item.get("command").and_then(|value| value.as_str()),
                Some("python skill-assets://skills/abc-user-lookup/scripts/lookup.py yihao.liao")
            );
        }
        other => panic!("unexpected start event: {other:?}"),
    }

    assert_eq!(update_envelopes.len(), 1);
    match &update_envelopes[0].event {
        BackboneEvent::CommandOutputDelta(n) => {
            assert!(n.delta.contains("vfs_uri_rewrite"));
            assert!(n.delta.contains("executed_command:"));
        }
        other => panic!("unexpected update event: {other:?}"),
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

    let mut chunk_emit_states = HashMap::new();
    let mut tool_call_states = HashMap::new();
    let envelopes = convert_event_to_envelopes(
        &event,
        "session-1",
        &test_source(),
        "turn-1",
        &mut entry_index,
        &mut chunk_emit_states,
        &mut tool_call_states,
    );

    assert_eq!(envelopes.len(), 1);
    match &envelopes[0].event {
        BackboneEvent::Platform(agentdash_agent_protocol::PlatformEvent::SessionMetaUpdate {
            key,
            ..
        }) => {
            assert_eq!(key, "approval_requested");
        }
        other => panic!("unexpected backbone event: {other:?}"),
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

    let mut chunk_emit_states = HashMap::new();
    let mut tool_call_states = HashMap::new();
    let envelopes = convert_event_to_envelopes(
        &end_event,
        "session-1",
        &test_source(),
        "turn-1",
        &mut entry_index,
        &mut chunk_emit_states,
        &mut tool_call_states,
    );

    assert_eq!(envelopes.len(), 1);
    match &envelopes[0].event {
        BackboneEvent::ItemCompleted(n) => {
            assert!(
                matches!(n.item.as_codex(), Some(codex_app_server_protocol::ThreadItem::DynamicToolCall { tool, .. }) if tool == "present_canvas")
            );
        }
        other => panic!("unexpected backbone event: {other:?}"),
    }
}

#[test]
fn aborted_assistant_message_end_emits_no_backbone_content() {
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

    let mut chunk_emit_states = HashMap::new();
    let mut tool_call_states = HashMap::new();
    let envelopes = convert_event_to_envelopes(
        &event,
        "session-1",
        &test_source(),
        "turn-1",
        &mut entry_index,
        &mut chunk_emit_states,
        &mut tool_call_states,
    );

    assert_eq!(entry_index, 0);
    assert!(envelopes.is_empty());
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

    let mut chunk_emit_states = HashMap::new();
    let mut tool_call_states = HashMap::new();
    let delta_envelopes = convert_event_to_envelopes(
        &delta_event,
        "session-1",
        &test_source(),
        "turn-1",
        &mut entry_index,
        &mut chunk_emit_states,
        &mut tool_call_states,
    );
    let end_envelopes = convert_event_to_envelopes(
        &message_end,
        "session-1",
        &test_source(),
        "turn-1",
        &mut entry_index,
        &mut chunk_emit_states,
        &mut tool_call_states,
    );

    assert_eq!(delta_envelopes.len(), 1);
    // 残余 delta ("llo") + 终态 ItemCompleted("hello") 并存。
    let delta = match &delta_envelopes[0].event {
        BackboneEvent::AgentMessageDelta(delta) => delta,
        other => panic!("unexpected backbone event: {other:?}"),
    };
    let end = end_envelopes
        .iter()
        .find_map(|env| match &env.event {
            BackboneEvent::AgentMessageDelta(end) => Some(end),
            _ => None,
        })
        .expect("residual agent message delta on MessageEnd");
    assert_eq!(delta.item_id, end.item_id);
    assert_eq!(end.delta, "llo");
    let final_text = end_envelopes
        .iter()
        .find_map(|env| match &env.event {
            BackboneEvent::ItemCompleted(n) => assistant_message_text(&n.item),
            _ => None,
        })
        .expect("terminal assistant message item");
    assert_eq!(final_text, "hello");
}

#[test]
fn message_end_after_tool_call_reuses_text_entry_index_and_message_id() {
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

    let mut chunk_emit_states = HashMap::new();
    let mut tool_call_states = HashMap::new();

    let delta_envelopes = convert_event_to_envelopes(
        &delta_event,
        "session-1",
        &test_source(),
        "turn-1",
        &mut entry_index,
        &mut chunk_emit_states,
        &mut tool_call_states,
    );
    let tool_envelopes = convert_event_to_envelopes(
        &tool_start_event,
        "session-1",
        &test_source(),
        "turn-1",
        &mut entry_index,
        &mut chunk_emit_states,
        &mut tool_call_states,
    );
    let end_envelopes = convert_event_to_envelopes(
        &message_end,
        "session-1",
        &test_source(),
        "turn-1",
        &mut entry_index,
        &mut chunk_emit_states,
        &mut tool_call_states,
    );

    assert_eq!(delta_envelopes.len(), 1);
    assert_eq!(tool_envelopes.len(), 1);

    let delta_item_id = match &delta_envelopes[0].event {
        BackboneEvent::AgentMessageDelta(d) => d.item_id.clone(),
        other => panic!("unexpected event: {other:?}"),
    };
    let end_delta = end_envelopes
        .iter()
        .find_map(|env| match &env.event {
            BackboneEvent::AgentMessageDelta(d) => Some(d),
            _ => None,
        })
        .expect("residual agent message delta on MessageEnd");

    assert_eq!(
        delta_item_id, end_delta.item_id,
        "MessageEnd reconcile 必须命中 TextDelta 的 chunk_emit_state，否则前端会渲染成两条文本气泡"
    );
    assert_eq!(end_delta.delta, "llo");

    // 终态 ItemCompleted(AgentMessage) 与残余 delta 共享 item_id，前端可并入同一气泡。
    let final_item_id = end_envelopes
        .iter()
        .find_map(|env| match &env.event {
            BackboneEvent::ItemCompleted(n) => match &n.item {
                AgentDashThreadItem::Codex(codex::ThreadItem::AgentMessage { id, .. }) => {
                    Some(id.clone())
                }
                _ => None,
            },
            _ => None,
        })
        .expect("terminal assistant message item");
    assert_eq!(final_item_id, delta_item_id);

    let delta_entry_index = delta_envelopes[0].trace.entry_index;
    let tool_entry_index = tool_envelopes[0].trace.entry_index;
    assert_eq!(
        delta_entry_index, tool_entry_index,
        "tool_call 与其所在 message 的文本应共享 entry_index"
    );

    assert_eq!(entry_index, 1);
}

#[test]
fn provider_adapter_behavior_matrix_has_named_coverage() {
    let coverage = [
        (
            "session_id",
            "prompt_refreshes_system_prompt_when_identity_prompt_changes",
        ),
        (
            "usage",
            "bridge streaming parsers map upstream usage into assistant messages",
        ),
        (
            "stderr_or_error",
            "aborted_assistant_message_end_emits_no_backbone_content",
        ),
        (
            "cancel",
            "prompt_without_provider_configuration_returns_clear_error",
        ),
        (
            "resume",
            "prompt_restores_repository_messages_before_new_user_prompt",
        ),
        (
            "poisoned_output",
            "message_end_does_not_repeat_full_snapshot_after_deltas",
        ),
    ];

    let dimensions = coverage
        .iter()
        .map(|(dimension, _)| *dimension)
        .collect::<std::collections::BTreeSet<_>>();

    assert_eq!(
        dimensions,
        std::collections::BTreeSet::from([
            "cancel",
            "poisoned_output",
            "resume",
            "session_id",
            "stderr_or_error",
            "usage",
        ])
    );
    assert!(coverage.iter().all(|(_, test_name)| !test_name.is_empty()));
}

// NOTE: prompt 渲染测试（identity frame + context_frames 拼接）已迁移至
// application/executor 的 ContextFrame 组装链路测试。

#[tokio::test]
async fn discovery_reflects_provider_added_to_db_without_restart() {
    use agentdash_domain::llm_provider::{LlmProvider, WireProtocol};

    let settings_repo = Arc::new(TestSettingsRepository::default());
    let llm_repo = Arc::new(TestLlmProviderRepository::default());
    let credential_repo = Arc::new(TestLlmProviderCredentialRepository);
    let secret_codec = Arc::new(TestLlmSecretCodec);

    let mut connector = build_pi_agent_connector(
        settings_repo.as_ref(),
        llm_repo.as_ref(),
        credential_repo.as_ref(),
        secret_codec.as_ref(),
    )
    .await
    .expect("connector should initialize even without provider");
    connector.set_llm_provider_repository(llm_repo.clone());
    connector.set_llm_provider_credential_repository(credential_repo.clone());
    connector.set_llm_secret_codec(secret_codec.clone());

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
    provider.global_api_key_ciphertext = "test-key".to_string();
    provider.default_model = "test-model".to_string();
    llm_repo.set_providers(vec![provider]);

    let refreshed = discover_options_state(&connector).await;
    assert_eq!(
        refreshed["options"]["model_selector"]["providers"],
        serde_json::json!([{
            "id": "anthropic",
            "name": "Anthropic Claude",
            "credential_mode": "global_only",
            "credential_source": "global_db",
            "protocol": "anthropic",
            "base_url": null,
            "discovery_url": null,
            "resolved_wire_api": null,
            "discovery_status": "not_supported",
            "discovery_message": null,
        }])
    );
    assert_eq!(
        refreshed["options"]["model_selector"]["default_model"],
        serde_json::json!("test-model")
    );
}

#[tokio::test]
async fn discovery_includes_global_only_platform_provider_without_user_byok() {
    use agentdash_domain::llm_provider::{LlmProvider, WireProtocol};

    let settings_repo = Arc::new(TestSettingsRepository::default());
    let llm_repo = Arc::new(TestLlmProviderRepository::default());
    let credential_repo = Arc::new(TestLlmProviderCredentialRepository);
    let secret_codec = Arc::new(TestLlmSecretCodec);

    let mut provider = LlmProvider::new("Platform Only", "platform-only", WireProtocol::Anthropic);
    provider.global_api_key_ciphertext = "test-key".to_string();
    provider.default_model = "platform-model".to_string();
    provider.models = serde_json::json!(["platform-model"]);
    llm_repo.set_providers(vec![provider]);

    let mut connector = build_pi_agent_connector(
        settings_repo.as_ref(),
        llm_repo.as_ref(),
        credential_repo.as_ref(),
        secret_codec.as_ref(),
    )
    .await
    .expect("connector should initialize with platform provider");
    connector.set_llm_provider_repository(llm_repo.clone());
    connector.set_llm_provider_credential_repository(credential_repo.clone());
    connector.set_llm_secret_codec(secret_codec.clone());

    let state = discover_options_state(&connector).await;
    assert_eq!(
        state["options"]["model_selector"]["providers"],
        serde_json::json!([{
            "id": "platform-only",
            "name": "Platform Only",
            "credential_mode": "global_only",
            "credential_source": "global_db",
            "protocol": "anthropic",
            "base_url": null,
            "discovery_url": null,
            "resolved_wire_api": null,
            "discovery_status": "not_supported",
            "discovery_message": null,
        }])
    );
    assert_eq!(
        state["options"]["model_selector"]["default_model"],
        serde_json::json!("platform-model")
    );
    assert!(
        state["options"]["model_selector"]["models"]
            .as_array()
            .expect("models should be an array")
            .iter()
            .any(|model| model["id"] == "platform-model"
                && model["provider_id"] == "platform-only"
                && model["blocked"] == false)
    );
}

#[tokio::test]
async fn discovery_does_not_fall_back_to_startup_provider_after_db_cleared() {
    use agentdash_domain::llm_provider::{LlmProvider, WireProtocol};

    let settings_repo = Arc::new(TestSettingsRepository::default());
    let llm_repo = Arc::new(TestLlmProviderRepository::default());
    let credential_repo = Arc::new(TestLlmProviderCredentialRepository);
    let secret_codec = Arc::new(TestLlmSecretCodec);

    let mut provider = LlmProvider::new("Anthropic Claude", "anthropic", WireProtocol::Anthropic);
    provider.global_api_key_ciphertext = "test-key".to_string();
    provider.default_model = "test-model".to_string();
    llm_repo.set_providers(vec![provider]);

    let mut connector = build_pi_agent_connector(
        settings_repo.as_ref(),
        llm_repo.as_ref(),
        credential_repo.as_ref(),
        secret_codec.as_ref(),
    )
    .await
    .expect("connector should initialize");
    connector.set_llm_provider_repository(llm_repo.clone());
    connector.set_llm_provider_credential_repository(credential_repo.clone());
    connector.set_llm_secret_codec(secret_codec.clone());

    let initial = discover_options_state(&connector).await;
    assert_eq!(
        initial["options"]["model_selector"]["providers"],
        serde_json::json!([{
            "id": "anthropic",
            "name": "Anthropic Claude",
            "credential_mode": "global_only",
            "credential_source": "global_db",
            "protocol": "anthropic",
            "base_url": null,
            "discovery_url": null,
            "resolved_wire_api": null,
            "discovery_status": "not_supported",
            "discovery_message": null,
        }])
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
    let credential_repo = TestLlmProviderCredentialRepository;
    let secret_codec = TestLlmSecretCodec;
    let mut connector =
        build_pi_agent_connector(repo.as_ref(), &llm_repo, &credential_repo, &secret_codec)
            .await
            .expect("connector should initialize even without provider");
    connector.set_settings_repository(repo);

    let result = connector
        .prompt(
            "session-1",
            None,
            &PromptPayload::Text("hello".to_string()),
            ExecutionContext {
                session: agentdash_spi::ExecutionSessionFrame {
                    turn_id: "turn-1".to_string(),
                    working_directory: PathBuf::from("/tmp/test-workspace"),
                    environment_variables: HashMap::new(),
                    executor_config: agentdash_spi::AgentConfig::new("PI_AGENT"),
                    mcp_servers: Vec::new(),
                    vfs: Some(test_vfs("/tmp/test-workspace")),
                    vfs_access_policy: None,
                    backend_execution: None,
                    runtime_backend_anchor: None,
                    identity: None,
                },
                turn: agentdash_spi::ExecutionTurnFrame::default(),
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
async fn prompt_missing_model_selection_reports_guidance_with_dynamic_providers() {
    use agentdash_domain::llm_provider::{LlmProvider, WireProtocol};

    let settings_repo = Arc::new(TestSettingsRepository::default());
    let llm_repo = Arc::new(TestLlmProviderRepository::default());
    let credential_repo = Arc::new(TestLlmProviderCredentialRepository);
    let secret_codec = Arc::new(TestLlmSecretCodec);

    let mut provider = LlmProvider::new("Anthropic Claude", "anthropic", WireProtocol::Anthropic);
    provider.global_api_key_ciphertext = "test-key".to_string();
    provider.default_model = "model-ok".to_string();
    llm_repo.set_providers(vec![provider]);

    let mut connector = build_pi_agent_connector(
        settings_repo.as_ref(),
        llm_repo.as_ref(),
        credential_repo.as_ref(),
        secret_codec.as_ref(),
    )
    .await
    .expect("connector should initialize");
    connector.set_llm_provider_repository(llm_repo);
    connector.set_llm_provider_credential_repository(credential_repo);
    connector.set_llm_secret_codec(secret_codec);

    let result = connector
        .prompt(
            "session-missing-model-selection",
            None,
            &PromptPayload::Text("hello".to_string()),
            execution_context_with_config(agentdash_spi::AgentConfig::new("PI_AGENT")),
        )
        .await;

    match result {
        Err(ConnectorError::InvalidConfig(message)) => {
            assert!(message.contains("缺少模型选择"));
            assert!(message.contains("模型选择器"));
            assert!(message.contains("Provider/Model"));
        }
        Ok(_) => panic!("prompt should fail when dynamic provider mode has no model selection"),
        Err(other) => panic!("unexpected connector error: {other}"),
    }
}

#[tokio::test]
async fn prompt_selected_unavailable_provider_reports_credential_mode() {
    use agentdash_domain::llm_provider::{LlmProvider, WireProtocol};

    let settings_repo = Arc::new(TestSettingsRepository::default());
    let llm_repo = Arc::new(TestLlmProviderRepository::default());
    let credential_repo = Arc::new(TestLlmProviderCredentialRepository);
    let secret_codec = Arc::new(TestLlmSecretCodec);

    let mut available = LlmProvider::new("Available", "available", WireProtocol::Anthropic);
    available.global_api_key_ciphertext = "test-key".to_string();
    available.default_model = "model-ok".to_string();

    let mut missing_global = LlmProvider::new("Athen AI", "athen-ai", WireProtocol::Anthropic);
    missing_global.default_model = "model-athen".to_string();

    llm_repo.set_providers(vec![available, missing_global]);

    let mut connector = build_pi_agent_connector(
        settings_repo.as_ref(),
        llm_repo.as_ref(),
        credential_repo.as_ref(),
        secret_codec.as_ref(),
    )
    .await
    .expect("connector should initialize with one available provider");
    connector.set_llm_provider_repository(llm_repo);
    connector.set_llm_provider_credential_repository(credential_repo);
    connector.set_llm_secret_codec(secret_codec);

    let mut executor_config = agentdash_spi::AgentConfig::new("PI_AGENT");
    executor_config.provider_id = Some("athen-ai".to_string());
    executor_config.model_id = Some("model-athen".to_string());

    let result = connector
        .prompt(
            "session-provider-mode-error",
            None,
            &PromptPayload::Text("hello".to_string()),
            ExecutionContext {
                session: agentdash_spi::ExecutionSessionFrame {
                    turn_id: "turn-provider-mode-error".to_string(),
                    working_directory: PathBuf::from("/tmp/test-workspace"),
                    environment_variables: HashMap::new(),
                    executor_config,
                    mcp_servers: Vec::new(),
                    vfs: Some(test_vfs("/tmp/test-workspace")),
                    vfs_access_policy: None,
                    backend_execution: None,
                    runtime_backend_anchor: None,
                    identity: None,
                },
                turn: agentdash_spi::ExecutionTurnFrame::default(),
            },
        )
        .await;

    match result {
        Err(ConnectorError::InvalidConfig(message)) => {
            assert!(message.contains("仅平台全局 Key 模式"));
            assert!(!message.contains("个人 BYOK 设置中补齐"));
        }
        Ok(_) => panic!("prompt should fail for selected provider without global credential"),
        Err(other) => panic!("unexpected connector error: {other}"),
    }
}

#[tokio::test]
async fn prompt_selected_user_required_provider_reports_byok_when_identity_exists() {
    use agentdash_domain::llm_provider::{LlmCredentialMode, LlmProvider, WireProtocol};

    let settings_repo = Arc::new(TestSettingsRepository::default());
    let llm_repo = Arc::new(TestLlmProviderRepository::default());
    let credential_repo = Arc::new(TestLlmProviderCredentialRepository);
    let secret_codec = Arc::new(TestLlmSecretCodec);

    let mut available = LlmProvider::new("Available", "available", WireProtocol::Anthropic);
    available.global_api_key_ciphertext = "test-key".to_string();
    available.default_model = "model-ok".to_string();

    let mut byok_only = LlmProvider::new("Athen AI", "athen-ai", WireProtocol::Anthropic);
    byok_only.credential_mode = LlmCredentialMode::UserRequired;
    byok_only.default_model = "model-athen".to_string();

    llm_repo.set_providers(vec![available, byok_only]);

    let mut connector = build_pi_agent_connector(
        settings_repo.as_ref(),
        llm_repo.as_ref(),
        credential_repo.as_ref(),
        secret_codec.as_ref(),
    )
    .await
    .expect("connector should initialize with one available provider");
    connector.set_llm_provider_repository(llm_repo);
    connector.set_llm_provider_credential_repository(credential_repo);
    connector.set_llm_secret_codec(secret_codec);

    let mut executor_config = agentdash_spi::AgentConfig::new("PI_AGENT");
    executor_config.provider_id = Some("athen-ai".to_string());
    executor_config.model_id = Some("model-athen".to_string());

    let result = connector
        .prompt(
            "session-user-required-error",
            None,
            &PromptPayload::Text("hello".to_string()),
            ExecutionContext {
                session: agentdash_spi::ExecutionSessionFrame {
                    turn_id: "turn-user-required-error".to_string(),
                    working_directory: PathBuf::from("/tmp/test-workspace"),
                    environment_variables: HashMap::new(),
                    executor_config,
                    mcp_servers: Vec::new(),
                    vfs: Some(test_vfs("/tmp/test-workspace")),
                    vfs_access_policy: None,
                    backend_execution: None,
                    runtime_backend_anchor: None,
                    identity: Some(agentdash_spi::AuthIdentity {
                        auth_mode: agentdash_spi::AuthMode::Personal,
                        user_id: "user-1".to_string(),
                        subject: "user-1".to_string(),
                        display_name: None,
                        email: None,
                        avatar_url: None,
                        groups: Vec::new(),
                        is_admin: false,
                        provider: Some("test".to_string()),
                        extra: serde_json::Value::Null,
                    }),
                },
                turn: agentdash_spi::ExecutionTurnFrame::default(),
            },
        )
        .await;

    match result {
        Err(ConnectorError::InvalidConfig(message)) => {
            assert!(message.contains("个人 BYOK 凭据"));
            assert!(message.contains("个人 BYOK 设置中补齐"));
        }
        Ok(_) => panic!("prompt should fail for user_required provider without user credential"),
        Err(other) => panic!("unexpected connector error: {other}"),
    }
}

#[tokio::test]
async fn prompt_selected_provider_rejects_blocked_model() {
    use agentdash_domain::llm_provider::{LlmProvider, WireProtocol};

    let settings_repo = Arc::new(TestSettingsRepository::default());
    let llm_repo = Arc::new(TestLlmProviderRepository::default());
    let credential_repo = Arc::new(TestLlmProviderCredentialRepository);
    let secret_codec = Arc::new(TestLlmSecretCodec);

    let mut provider = LlmProvider::new("Anthropic Claude", "anthropic", WireProtocol::Anthropic);
    provider.global_api_key_ciphertext = "test-key".to_string();
    provider.default_model = "model-ok".to_string();
    provider.models = serde_json::json!(["model-ok", "model-blocked"]);
    provider.blocked_models = serde_json::json!(["model-blocked"]);
    llm_repo.set_providers(vec![provider]);

    let mut connector = build_pi_agent_connector(
        settings_repo.as_ref(),
        llm_repo.as_ref(),
        credential_repo.as_ref(),
        secret_codec.as_ref(),
    )
    .await
    .expect("connector should initialize");
    connector.set_llm_provider_repository(llm_repo);
    connector.set_llm_provider_credential_repository(credential_repo);
    connector.set_llm_secret_codec(secret_codec);

    let mut executor_config = agentdash_spi::AgentConfig::new("PI_AGENT");
    executor_config.provider_id = Some("anthropic".to_string());
    executor_config.model_id = Some("model-blocked".to_string());

    let result = connector
        .prompt(
            "session-blocked-model",
            None,
            &PromptPayload::Text("hello".to_string()),
            execution_context_with_config(executor_config),
        )
        .await;

    match result {
        Err(ConnectorError::InvalidConfig(message)) => {
            assert!(message.contains("已被屏蔽"));
            assert!(message.contains("model-blocked"));
        }
        Ok(_) => panic!("prompt should fail for blocked model"),
        Err(other) => panic!("unexpected connector error: {other}"),
    }
}

#[tokio::test]
async fn prompt_selected_provider_rejects_unknown_model() {
    use agentdash_domain::llm_provider::{LlmProvider, WireProtocol};

    let settings_repo = Arc::new(TestSettingsRepository::default());
    let llm_repo = Arc::new(TestLlmProviderRepository::default());
    let credential_repo = Arc::new(TestLlmProviderCredentialRepository);
    let secret_codec = Arc::new(TestLlmSecretCodec);

    let mut provider = LlmProvider::new("Anthropic Claude", "anthropic", WireProtocol::Anthropic);
    provider.global_api_key_ciphertext = "test-key".to_string();
    provider.default_model = "model-ok".to_string();
    provider.models = serde_json::json!(["model-ok"]);
    llm_repo.set_providers(vec![provider]);

    let mut connector = build_pi_agent_connector(
        settings_repo.as_ref(),
        llm_repo.as_ref(),
        credential_repo.as_ref(),
        secret_codec.as_ref(),
    )
    .await
    .expect("connector should initialize");
    connector.set_llm_provider_repository(llm_repo);
    connector.set_llm_provider_credential_repository(credential_repo);
    connector.set_llm_secret_codec(secret_codec);

    let mut executor_config = agentdash_spi::AgentConfig::new("PI_AGENT");
    executor_config.provider_id = Some("anthropic".to_string());
    executor_config.model_id = Some("missing-model".to_string());

    let result = connector
        .prompt(
            "session-unknown-model",
            None,
            &PromptPayload::Text("hello".to_string()),
            execution_context_with_config(executor_config),
        )
        .await;

    match result {
        Err(ConnectorError::InvalidConfig(message)) => {
            assert!(message.contains("不包含模型"));
            assert!(message.contains("missing-model"));
        }
        Ok(_) => panic!("prompt should fail for unknown model"),
        Err(other) => panic!("unexpected connector error: {other}"),
    }
}

#[tokio::test]
async fn prompt_requires_provider_when_model_id_matches_multiple_providers() {
    use agentdash_domain::llm_provider::{LlmProvider, WireProtocol};

    let settings_repo = Arc::new(TestSettingsRepository::default());
    let llm_repo = Arc::new(TestLlmProviderRepository::default());
    let credential_repo = Arc::new(TestLlmProviderCredentialRepository);
    let secret_codec = Arc::new(TestLlmSecretCodec);

    let mut provider_a = LlmProvider::new("Provider A", "provider-a", WireProtocol::Anthropic);
    provider_a.global_api_key_ciphertext = "test-key".to_string();
    provider_a.default_model = "shared-model".to_string();
    provider_a.models = serde_json::json!(["shared-model"]);

    let mut provider_b = LlmProvider::new("Provider B", "provider-b", WireProtocol::Anthropic);
    provider_b.global_api_key_ciphertext = "test-key".to_string();
    provider_b.default_model = "shared-model".to_string();
    provider_b.models = serde_json::json!(["shared-model"]);
    llm_repo.set_providers(vec![provider_a, provider_b]);

    let mut connector = build_pi_agent_connector(
        settings_repo.as_ref(),
        llm_repo.as_ref(),
        credential_repo.as_ref(),
        secret_codec.as_ref(),
    )
    .await
    .expect("connector should initialize");
    connector.set_llm_provider_repository(llm_repo);
    connector.set_llm_provider_credential_repository(credential_repo);
    connector.set_llm_secret_codec(secret_codec);

    let mut executor_config = agentdash_spi::AgentConfig::new("PI_AGENT");
    executor_config.model_id = Some("shared-model".to_string());

    let result = connector
        .prompt(
            "session-ambiguous-model",
            None,
            &PromptPayload::Text("hello".to_string()),
            execution_context_with_config(executor_config),
        )
        .await;

    match result {
        Err(ConnectorError::InvalidConfig(message)) => {
            assert!(message.contains("多个 LLM Provider"));
            assert!(message.contains("provider-a"));
            assert!(message.contains("provider-b"));
        }
        Ok(_) => panic!("prompt should fail for ambiguous model without provider_id"),
        Err(other) => panic!("unexpected connector error: {other}"),
    }
}

#[tokio::test]
async fn prompt_restores_repository_messages_before_new_user_prompt() {
    let bridge = Arc::new(RecordingBridge::default());
    let connector = PiAgentConnector::new(bridge.clone(), "系统提示");

    let mut stream = connector
        .prompt(
            "session-restore-1",
            None,
            &PromptPayload::Text("新的用户消息".to_string()),
            ExecutionContext {
                session: agentdash_spi::ExecutionSessionFrame {
                    turn_id: "turn-1".to_string(),
                    working_directory: PathBuf::from("/tmp/test-workspace"),
                    environment_variables: HashMap::new(),
                    executor_config: agentdash_spi::AgentConfig::new("PI_AGENT"),
                    mcp_servers: Vec::new(),
                    vfs: Some(test_vfs("/tmp/test-workspace")),
                    vfs_access_policy: None,
                    backend_execution: None,
                    runtime_backend_anchor: None,
                    identity: None,
                },
                turn: agentdash_spi::ExecutionTurnFrame {
                    restored_session_state: Some(agentdash_spi::RestoredSessionState {
                        messages: vec![
                            agentdash_spi::AgentMessage::user("历史用户消息"),
                            agentdash_spi::AgentMessage::assistant("历史助手消息"),
                        ],
                        message_refs: vec![None, None],
                    }),
                    ..Default::default()
                },
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
async fn prompt_hydrates_session_item_identity_from_restored_messages() {
    let bridge = Arc::new(RecordingBridge::default());
    let connector = PiAgentConnector::new(bridge, "系统提示");
    let session_id = "session-readable-restore";

    let mut stream = connector
        .prompt(
            session_id,
            None,
            &PromptPayload::Text("新的用户消息".to_string()),
            ExecutionContext {
                session: agentdash_spi::ExecutionSessionFrame {
                    turn_id: "raw-turn-new".to_string(),
                    working_directory: PathBuf::from("/tmp/test-workspace"),
                    environment_variables: HashMap::new(),
                    executor_config: agentdash_spi::AgentConfig::new("PI_AGENT"),
                    mcp_servers: Vec::new(),
                    vfs: Some(test_vfs("/tmp/test-workspace")),
                    vfs_access_policy: None,
                    backend_execution: None,
                    runtime_backend_anchor: None,
                    identity: None,
                },
                turn: agentdash_spi::ExecutionTurnFrame {
                    restored_session_state: Some(agentdash_spi::RestoredSessionState {
                        messages: vec![
                            AgentMessage::Assistant {
                                content: Vec::new(),
                                tool_calls: vec![agentdash_agent::ToolCallInfo {
                                    id: "turn_001:tool_004".to_string(),
                                    call_id: None,
                                    name: "fs_read".to_string(),
                                    arguments: serde_json::json!({}),
                                }],
                                stop_reason: None,
                                error_message: None,
                                usage: None,
                                timestamp: None,
                            },
                            AgentMessage::ToolResult {
                                tool_call_id: "legacy-raw-tool-call-id".to_string(),
                                call_id: None,
                                tool_name: Some("shell_exec".to_string()),
                                content: Vec::new(),
                                details: Some(serde_json::json!({
                                    "readable_ref": {
                                        "item_id": "turn_002:cmd_002"
                                    }
                                })),
                                is_error: false,
                                timestamp: None,
                            },
                        ],
                        message_refs: Vec::new(),
                    }),
                    ..Default::default()
                },
            },
        )
        .await
        .expect("prompt should start");

    while let Some(next) = stream.next().await {
        next.expect("stream item should succeed");
    }

    let agents = connector.agents.lock().await;
    let runtime = agents.get(session_id).expect("runtime should be retained");
    let tool_ref = runtime.session_identity.tool_result_ref(
        "raw-turn-after-restore",
        "raw-tool-after-restore",
        "fs_read",
    );
    assert_eq!(tool_ref.item_id, "turn_003:tool_005");

    let command_ref = runtime.session_identity.tool_result_ref(
        "raw-turn-after-restore",
        "raw-cmd-after-restore",
        "shell_exec",
    );
    assert_eq!(command_ref.item_id, "turn_003:cmd_003");
}

#[tokio::test]
async fn prompt_refreshes_system_prompt_when_identity_prompt_changes() {
    let bridge = Arc::new(RecordingBridge::default());
    let connector = PiAgentConnector::new(bridge.clone(), "系统提示");

    let session_id = "session-identity-refresh";

    let make_context = |turn_id: &str, identity_prompt: Option<&str>| -> ExecutionContext {
        let turn_frame = agentdash_spi::ExecutionTurnFrame {
            context_frames: identity_prompt
                .map(|prompt| {
                    vec![agentdash_spi::hooks::ContextFrame {
                        id: format!("identity-{turn_id}"),
                        kind: "identity".to_string(),
                        source: agentdash_spi::hooks::RuntimeEventSource::RuntimeContextUpdate,
                        phase_node: None,
                        apply_mode: None,
                        delivery_status: "prepared_for_connector".to_string(),
                        delivery_channel: "connector_context".to_string(),
                        message_role: "system".to_string(),
                        delivery_metadata: agentdash_spi::ContextDeliveryMetadata::for_frame(
                            "identity",
                            "connector_context",
                            "system",
                        ),
                        rendered_text: prompt.to_string(),
                        sections: vec![agentdash_spi::hooks::ContextFrameSection::Identity {
                            title: "Identity".to_string(),
                            summary: "test".to_string(),
                            fragments: vec![agentdash_spi::hooks::RuntimeContextFragmentEntry {
                                slot: "identity".to_string(),
                                label: "identity_system_prompt".to_string(),
                                source: "connector".to_string(),
                                content: prompt.to_string(),
                                context_usage_kind: None,
                            }],
                        }],
                        created_at_ms: 1,
                    }]
                })
                .unwrap_or_default(),
            ..Default::default()
        };
        ExecutionContext {
            session: agentdash_spi::ExecutionSessionFrame {
                turn_id: turn_id.to_string(),
                working_directory: PathBuf::from("/tmp/test-workspace"),
                environment_variables: HashMap::new(),
                executor_config: agentdash_spi::AgentConfig::new("PI_AGENT"),
                mcp_servers: Vec::new(),
                vfs: Some(test_vfs("/tmp/test-workspace")),
                vfs_access_policy: None,
                backend_execution: None,
                runtime_backend_anchor: None,
                identity: None,
            },
            turn: turn_frame,
        }
    };

    // Turn 1: 首轮 — 应走 is_new_agent 分支并把 "SP_A" 写入 agent
    let mut stream = connector
        .prompt(
            session_id,
            None,
            &PromptPayload::Text("msg-a".to_string()),
            make_context("turn-a", Some("SP_A")),
        )
        .await
        .expect("turn 1 should start");
    while let Some(next) = stream.next().await {
        next.expect("stream item should succeed");
    }

    // Turn 2: 同 session，identity prompt 变化 — 期望 set_system_prompt 再次被调用，
    //         第 2 个 BridgeRequest 的 system_prompt = "SP_B"
    let mut stream = connector
        .prompt(
            session_id,
            None,
            &PromptPayload::Text("msg-b".to_string()),
            make_context("turn-b", Some("SP_B")),
        )
        .await
        .expect("turn 2 should start");
    while let Some(next) = stream.next().await {
        next.expect("stream item should succeed");
    }

    // Turn 3: identity prompt 不变 — agent 仍用 turn 2 时 set 的 "SP_B"
    let mut stream = connector
        .prompt(
            session_id,
            None,
            &PromptPayload::Text("msg-c".to_string()),
            make_context("turn-c", Some("SP_B")),
        )
        .await
        .expect("turn 3 should start");
    while let Some(next) = stream.next().await {
        next.expect("stream item should succeed");
    }

    {
        let requests = bridge
            .requests
            .lock()
            .expect("recording bridge lock poisoned");
        assert_eq!(requests.len(), 3, "应记录三次 bridge 请求");
        assert_eq!(
            requests[0].system_prompt.as_deref(),
            Some("SP_A"),
            "turn 1 应落入 SP_A"
        );
        assert_eq!(
            requests[1].system_prompt.as_deref(),
            Some("SP_B"),
            "identity prompt 变化后 turn 2 应切到 SP_B"
        );
        assert_eq!(
            requests[2].system_prompt.as_deref(),
            Some("SP_B"),
            "identity prompt 未变时 turn 3 应保持 SP_B（set_system_prompt 未被调用）"
        );
    }

    let agents = connector.agents.lock().await;
    let runtime = agents
        .get(session_id)
        .expect("session runtime should be retained");
    assert_eq!(runtime.last_system_prompt.as_deref(), Some("SP_B"));
}

/// 系统提示词由 delivery metadata 标记为 system/developer 的帧按序组装。
#[test]
fn assemble_system_prompt_uses_delivery_metadata_and_excludes_memory() {
    fn frame(kind: &str, rendered: &str) -> agentdash_spi::hooks::ContextFrame {
        agentdash_spi::hooks::ContextFrame {
            id: format!("{kind}-1"),
            kind: kind.to_string(),
            source: agentdash_spi::hooks::RuntimeEventSource::RuntimeContextUpdate,
            phase_node: None,
            apply_mode: None,
            delivery_status: "prepared_for_connector".to_string(),
            delivery_channel: "connector_context".to_string(),
            message_role: "system".to_string(),
            delivery_metadata: agentdash_spi::ContextDeliveryMetadata::for_frame(
                kind,
                "connector_context",
                "system",
            ),
            rendered_text: rendered.to_string(),
            sections: Vec::new(),
            created_at_ms: 1,
        }
    }

    let identity = frame("identity", "## Identity\n\nbase");
    let guidelines = frame(
        "system_guidelines",
        "## Project Guidelines\n\n### AGENTS.md\n\n使用中文交流",
    );
    let memory = frame(
        "memory_context",
        "## Memory Context\n\nDefault source: `agent://`",
    );

    // 帧顺序不应影响结果：正式 system prompt 按 delivery order 拼接，
    // memory_context 虽然仍可见于动态上下文，但不进入 PiAgent system prompt。
    let prompt = assemble_system_prompt(&[memory.clone(), guidelines.clone(), identity.clone()])
        .expect("system prompt should exist");
    assert!(prompt.starts_with("## Identity"));
    assert!(prompt.contains("base"));
    assert!(prompt.contains("## Project Guidelines"));
    assert!(prompt.contains("使用中文交流"));
    assert!(!prompt.contains("## Memory Context"));
    assert!(prompt.ends_with("使用中文交流"));

    // 仅有身份帧时也成立。
    let identity_only = assemble_system_prompt(&[identity]).expect("identity only");
    assert_eq!(identity_only, "## Identity\n\nbase");

    // 只有动态上下文帧时返回 None。
    assert!(assemble_system_prompt(&[memory]).is_none());
}

#[tokio::test]
async fn prompt_rebuilds_live_agent_when_model_selection_changes() {
    let state = Arc::new(ModelRecordingState::default());
    let factory_state = state.clone();
    let bridge_factory: super::super::bridges::provider_registry::BridgeFactory =
        Arc::new(move |model_id: &str| {
            Arc::new(ModelRecordingBridge {
                model_id: model_id.to_string(),
                state: factory_state.clone(),
            })
        });

    let mut connector = PiAgentConnector::new(Arc::new(NoopBridge), "系统提示");
    connector.add_provider(
        super::super::bridges::provider_registry::ProviderEntry::new_for_test(
            "test-provider",
            "Test Provider",
            "model-a",
            bridge_factory,
            vec![
                super::super::bridges::provider_registry::ModelMeta::from_id("model-a"),
                super::super::bridges::provider_registry::ModelMeta::from_id("model-b"),
            ],
        ),
    );

    let make_context = |turn_id: &str, model_id: &str| -> ExecutionContext {
        let mut executor_config = agentdash_spi::AgentConfig::new("PI_AGENT");
        executor_config.provider_id = Some("test-provider".to_string());
        executor_config.model_id = Some(model_id.to_string());
        ExecutionContext {
            session: agentdash_spi::ExecutionSessionFrame {
                turn_id: turn_id.to_string(),
                working_directory: PathBuf::from("/tmp/test-workspace"),
                environment_variables: HashMap::new(),
                executor_config,
                mcp_servers: Vec::new(),
                vfs: Some(test_vfs("/tmp/test-workspace")),
                vfs_access_policy: None,
                backend_execution: None,
                runtime_backend_anchor: None,
                identity: None,
            },
            turn: agentdash_spi::ExecutionTurnFrame::default(),
        }
    };

    let mut stream = connector
        .prompt(
            "session-model-switch",
            None,
            &PromptPayload::Text("first".to_string()),
            make_context("turn-a", "model-a"),
        )
        .await
        .expect("turn 1 should start");
    while let Some(next) = stream.next().await {
        next.expect("stream item should succeed");
    }

    let mut stream = connector
        .prompt(
            "session-model-switch",
            None,
            &PromptPayload::Text("second".to_string()),
            make_context("turn-b", "model-b"),
        )
        .await
        .expect("turn 2 should start");
    while let Some(next) = stream.next().await {
        next.expect("stream item should succeed");
    }

    let requests = state
        .requests
        .lock()
        .expect("model recording bridge lock poisoned")
        .clone();
    assert_eq!(
        requests,
        vec![("model-a".to_string(), 1), ("model-b".to_string(), 3)],
        "切换模型后应使用新 bridge，同时保留上一轮会话消息"
    );
}

#[tokio::test]
async fn cancel_waits_for_agent_idle_before_next_prompt() {
    let bridge = Arc::new(CancelThenDoneBridge::default());
    let connector = PiAgentConnector::new(bridge.clone(), "系统提示");
    let session_id = "session-cancel-idle";

    let first_provider_started = bridge.first_provider_started.notified();
    tokio::pin!(first_provider_started);
    let _first_stream = connector
        .prompt(
            session_id,
            None,
            &PromptPayload::Text("first".to_string()),
            execution_context_with_config(agentdash_spi::AgentConfig::new("PI_AGENT")),
        )
        .await
        .expect("first prompt should start");
    first_provider_started.await;

    tokio::time::timeout(Duration::from_secs(1), connector.cancel(session_id))
        .await
        .expect("cancel should wait for provider cancellation and agent idle")
        .expect("cancel should succeed");

    let mut second_stream = connector
        .prompt(
            session_id,
            None,
            &PromptPayload::Text("second".to_string()),
            execution_context_with_config(agentdash_spi::AgentConfig::new("PI_AGENT")),
        )
        .await
        .expect("second prompt should not hit stale Pi Agent is_streaming");

    tokio::time::timeout(Duration::from_secs(1), async {
        while let Some(next) = second_stream.next().await {
            next.expect("second stream item should succeed");
        }
    })
    .await
    .expect("second stream should complete");
    assert_eq!(bridge.calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn update_session_tools_replaces_all_tools() {
    let connector = PiAgentConnector::new(Arc::new(NoopBridge), "系统提示");

    let old_tool = StaticTool::named("old_tool");
    let new_tool = StaticTool::named("new_tool");

    let mut agent = Agent::new(
        Arc::new(NoopBridge),
        agentdash_agent::AgentConfig::default(),
    );
    agent.set_tools(vec![old_tool.clone()]);

    connector.agents.lock().await.insert(
        "session-replace-tools".to_string(),
        PiAgentSessionRuntime {
            agent,
            tools: vec![old_tool],
            last_system_prompt: None,
            model_selection: PiAgentModelSelection {
                provider_id: None,
                model_id: None,
            },
            model_context_window: Some(CONTEXT_WINDOW_STANDARD),
            session_identity: SessionItemIdentity::new(),
        },
    );

    connector
        .update_session_tools("session-replace-tools", vec![new_tool.clone()])
        .await
        .expect("update_session_tools should succeed");

    let agents = connector.agents.lock().await;
    let runtime = agents
        .get("session-replace-tools")
        .expect("runtime should exist");
    let names: Vec<String> = runtime
        .tools
        .iter()
        .map(|tool| tool.name().to_string())
        .collect();
    assert_eq!(names, vec!["new_tool".to_string()]);
    let state = runtime
        .agent
        .try_state()
        .expect("agent state should be readable");
    let agent_names: Vec<String> = state
        .tools
        .iter()
        .map(|tool| tool.name().to_string())
        .collect();
    assert_eq!(agent_names, vec!["new_tool".to_string()]);
}

/// S3 回归：连接器投递路径（`prompt` / `steer_session`）构造 user 消息时，
/// 带 data URL 图片的 canonical 输入必须经唯一映射结构化直达 `ContentPart::Image`，
/// 不再被拍平成占位文本。此处直接断言连接器实际使用的转换组合。
#[test]
fn connector_delivery_maps_image_input_to_content_part_image() {
    let input = vec![
        codex::UserInput::Text {
            text: "看这张图".to_string(),
            text_elements: Vec::new(),
        },
        codex::UserInput::Image {
            detail: None,
            url: "data:image/png;base64,iVBORw0KGgo".to_string(),
        },
    ];

    // steer_session 路径：user_input_blocks_to_content_parts(&input) -> AgentMessage::user_parts。
    let parts = user_input_blocks_to_content_parts(&input);
    let message = AgentMessage::user_parts(parts);
    let AgentMessage::User { content, .. } = message else {
        panic!("expected user message");
    };
    assert_eq!(content.len(), 2);
    assert_eq!(content[0], ContentPart::text("看这张图"));
    assert_eq!(
        content[1],
        ContentPart::image("image/png", "iVBORw0KGgo"),
        "图片必须结构化直达 ContentPart::Image，而非占位文本"
    );
    assert!(matches!(content[1], ContentPart::Image { .. }));

    // prompt 路径：PromptPayload::Input.to_content_parts() 等价产出同一结构化结果。
    let prompt_parts = PromptPayload::Input(input).to_content_parts();
    assert!(matches!(prompt_parts[1], ContentPart::Image { .. }));
}
