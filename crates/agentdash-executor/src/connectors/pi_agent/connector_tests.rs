use super::*;
use crate::connectors::pi_agent::factory::{NoopBridge, build_pi_agent_connector};
use agentdash_agent::{AgentEvent, AgentToolResult, AssistantStreamEvent, ContentPart, StopReason};
use agentdash_domain::DomainError;
use agentdash_domain::settings::{Setting, SettingScope, SettingsRepository};
use agentdash_protocol::{BackboneEvent, SourceInfo};
use agentdash_spi::{Mount, MountCapability};
use chrono::Utc;
use std::sync::{Mutex as StdMutex, RwLock};

fn test_source() -> SourceInfo {
    SourceInfo {
        connector_id: "pi-agent".to_string(),
        connector_type: "local_executor".to_string(),
        executor_id: None,
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
            assert!(
                matches!(&n.item, codex_app_server_protocol::ThreadItem::DynamicToolCall { tool, .. } if tool == "shell_exec")
            );
        }
        other => panic!("unexpected backbone event: {other:?}"),
    }
    assert_eq!(delta_envelopes.len(), 1);
    match &delta_envelopes[0].event {
        BackboneEvent::ItemStarted(n) => {
            assert!(
                matches!(&n.item, codex_app_server_protocol::ThreadItem::DynamicToolCall { tool, arguments, .. } if tool == "shell_exec" && *arguments == serde_json::json!({ "command": "echo hello" }))
            );
        }
        other => panic!("unexpected backbone event: {other:?}"),
    }
    assert_eq!(end_envelopes.len(), 1);
    match &end_envelopes[0].event {
        BackboneEvent::ItemStarted(_) | BackboneEvent::ItemCompleted(_) => {}
        other => panic!("unexpected backbone event: {other:?}"),
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
        BackboneEvent::ItemStarted(n) => {
            assert!(
                matches!(&n.item, codex_app_server_protocol::ThreadItem::DynamicToolCall { tool, .. } if tool == "fs_apply_patch")
            );
        }
        other => panic!("unexpected backbone event: {other:?}"),
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
                matches!(&n.item, codex_app_server_protocol::ThreadItem::DynamicToolCall { tool, arguments, .. } if tool == "read_file" && *arguments == serde_json::json!({ "path": "README.md" }))
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
            assert!(
                matches!(&n.item, codex_app_server_protocol::ThreadItem::DynamicToolCall { tool, .. } if tool == "shell_exec")
            );
        }
        other => panic!("unexpected backbone event: {other:?}"),
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
        BackboneEvent::ItemStarted(n) => {
            assert!(
                matches!(&n.item, codex_app_server_protocol::ThreadItem::DynamicToolCall { tool, .. } if tool == "echo")
            );
        }
        other => panic!("unexpected backbone event: {other:?}"),
    }

    match &end_envelopes[0].event {
        BackboneEvent::ItemCompleted(n) => {
            assert!(
                matches!(&n.item, codex_app_server_protocol::ThreadItem::DynamicToolCall { tool, success, .. } if tool == "echo" && *success == Some(true))
            );
        }
        other => panic!("unexpected backbone event: {other:?}"),
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

    assert_eq!(envelopes.len(), 2);
    match &envelopes[0].event {
        BackboneEvent::ItemStarted(n) => {
            assert!(
                matches!(&n.item, codex_app_server_protocol::ThreadItem::DynamicToolCall { tool, .. } if tool == "shell_exec")
            );
        }
        other => panic!("unexpected backbone event: {other:?}"),
    }
    match &envelopes[1].event {
        BackboneEvent::Platform(agentdash_protocol::PlatformEvent::SessionMetaUpdate {
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
                matches!(&n.item, codex_app_server_protocol::ThreadItem::DynamicToolCall { tool, .. } if tool == "present_canvas")
            );
        }
        other => panic!("unexpected backbone event: {other:?}"),
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
    assert_eq!(entry_index, 1);
    match &envelopes[0].event {
        BackboneEvent::AgentMessageDelta(delta) => {
            assert_eq!(delta.delta, "Agent run aborted");
        }
        other => panic!("unexpected backbone event: {other:?}"),
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
    assert_eq!(end_envelopes.len(), 1);
    match (&delta_envelopes[0].event, &end_envelopes[0].event) {
        (BackboneEvent::AgentMessageDelta(delta), BackboneEvent::AgentMessageDelta(end)) => {
            assert_eq!(delta.item_id, end.item_id);
            assert_eq!(end.delta, "llo");
        }
        other => panic!("unexpected backbone events: {other:?}"),
    }
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
    assert_eq!(end_envelopes.len(), 1);

    let delta_item_id = match &delta_envelopes[0].event {
        BackboneEvent::AgentMessageDelta(d) => d.item_id.clone(),
        other => panic!("unexpected event: {other:?}"),
    };
    let end_delta = match &end_envelopes[0].event {
        BackboneEvent::AgentMessageDelta(d) => d,
        other => panic!("unexpected event: {other:?}"),
    };

    assert_eq!(
        delta_item_id, end_delta.item_id,
        "MessageEnd reconcile 必须命中 TextDelta 的 chunk_emit_state，否则前端会渲染成两条文本气泡"
    );
    assert_eq!(end_delta.delta, "llo");

    let delta_entry_index = delta_envelopes[0].trace.entry_index;
    let tool_entry_index = tool_envelopes[0].trace.entry_index;
    assert_eq!(
        delta_entry_index, tool_entry_index,
        "tool_call 与其所在 message 的文本应共享 entry_index"
    );

    assert_eq!(entry_index, 1);
}

// NOTE: prompt 渲染测试（system prompt + tool parameters）已迁移至
// application 层 system_prompt_assembler 模块。

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

    let mut provider = LlmProvider::new("Anthropic Claude", "anthropic", WireProtocol::Anthropic);
    provider.api_key = "test-key".to_string();
    provider.default_model = "test-model".to_string();
    llm_repo.set_providers(vec![provider]);

    let refreshed = discover_options_state(&connector).await;
    assert_eq!(
        refreshed["options"]["model_selector"]["providers"],
        serde_json::json!([{ "id": "anthropic", "name": "Anthropic Claude" }])
    );
    assert_eq!(
        refreshed["options"]["model_selector"]["default_model"],
        serde_json::json!("test-model")
    );
}

#[tokio::test]
async fn discovery_does_not_fall_back_to_startup_provider_after_db_cleared() {
    use agentdash_domain::llm_provider::{LlmProvider, WireProtocol};

    let settings_repo = Arc::new(TestSettingsRepository::default());
    let llm_repo = Arc::new(TestLlmProviderRepository::default());

    let mut provider = LlmProvider::new("Anthropic Claude", "anthropic", WireProtocol::Anthropic);
    provider.api_key = "test-key".to_string();
    provider.default_model = "test-model".to_string();
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
                session: agentdash_spi::ExecutionSessionFrame {
                    turn_id: "turn-1".to_string(),
                    working_directory: PathBuf::from("/tmp/test-workspace"),
                    environment_variables: HashMap::new(),
                    executor_config: agentdash_spi::AgentConfig::new("PI_AGENT"),
                    mcp_servers: Vec::new(),
                    vfs: Some(test_vfs("/tmp/test-workspace")),
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
                    identity: None,
                },
                turn: agentdash_spi::ExecutionTurnFrame {
                    restored_session_state: Some(agentdash_spi::RestoredSessionState {
                        messages: vec![
                            agentdash_spi::AgentMessage::user("历史用户消息"),
                            agentdash_spi::AgentMessage::assistant("历史助手消息"),
                        ],
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
async fn prompt_refreshes_system_prompt_when_bundle_id_changes() {
    use agentdash_spi::session_context_bundle::SessionContextBundle;

    let bridge = Arc::new(RecordingBridge::default());
    let connector = PiAgentConnector::new(bridge.clone(), "系统提示");

    let session_id = "session-bundle-refresh";
    let session_uuid = uuid::Uuid::new_v4();

    let make_context = |turn_id: &str,
                        bundle: Option<SessionContextBundle>,
                        assembled_sp: Option<&str>|
     -> ExecutionContext {
        #[allow(deprecated)]
        let turn_frame = agentdash_spi::ExecutionTurnFrame {
            context_bundle: bundle,
            assembled_system_prompt: assembled_sp.map(str::to_string),
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
                identity: None,
            },
            turn: turn_frame,
        }
    };

    let bundle_a = SessionContextBundle::new(session_uuid, "turn-a");
    let bundle_b = SessionContextBundle::new(session_uuid, "turn-b");
    assert_ne!(bundle_a.bundle_id, bundle_b.bundle_id);
    let bundle_b_id = bundle_b.bundle_id;

    // Turn 1: 首轮 — 应走 is_new_agent 分支并把 "SP_A" 写入 agent
    let mut stream = connector
        .prompt(
            session_id,
            None,
            &PromptPayload::Text("msg-a".to_string()),
            make_context("turn-a", Some(bundle_a), Some("SP_A")),
        )
        .await
        .expect("turn 1 should start");
    while let Some(next) = stream.next().await {
        next.expect("stream item should succeed");
    }

    // Turn 2: 同 session，bundle_id 变化 — 期望 set_system_prompt 再次被调用，
    //         第 2 个 BridgeRequest 的 system_prompt = "SP_B"
    let mut stream = connector
        .prompt(
            session_id,
            None,
            &PromptPayload::Text("msg-b".to_string()),
            make_context("turn-b", Some(bundle_b), Some("SP_B")),
        )
        .await
        .expect("turn 2 should start");
    while let Some(next) = stream.next().await {
        next.expect("stream item should succeed");
    }

    // Turn 3: bundle_id 不变 — assembled_system_prompt 即便换成 "SP_STALE"，
    //         也不会生效，agent 仍用 turn 2 时 set 的 "SP_B"
    let mut stream = connector
        .prompt(
            session_id,
            None,
            &PromptPayload::Text("msg-c".to_string()),
            make_context(
                "turn-c",
                Some(SessionContextBundle {
                    bundle_id: bundle_b_id,
                    session_id: session_uuid,
                    phase_tag: "turn-c".to_string(),
                    created_at_ms: 0,
                    bootstrap_fragments: Vec::new(),
                    turn_delta: Vec::new(),
                }),
                Some("SP_STALE"),
            ),
        )
        .await
        .expect("turn 3 should start");
    while let Some(next) = stream.next().await {
        next.expect("stream item should succeed");
    }

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
        "bundle_id 变化后 turn 2 应切到 SP_B"
    );
    assert_eq!(
        requests[2].system_prompt.as_deref(),
        Some("SP_B"),
        "bundle_id 未变时 turn 3 应保持 SP_B（set_system_prompt 未被调用）"
    );

    let agents = connector.agents.lock().await;
    let runtime = agents
        .get(session_id)
        .expect("session runtime should be retained");
    assert_eq!(runtime.last_bundle_id, Some(bundle_b_id));
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
            last_bundle_id: None,
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
