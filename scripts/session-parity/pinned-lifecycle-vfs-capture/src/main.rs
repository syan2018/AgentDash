use std::sync::Arc;

use agentdash_agent_protocol::backbone::item::ItemCompletedNotification;
use agentdash_agent_protocol::codex_app_server_protocol as codex;
use agentdash_agent_protocol::{
    AgentDashThreadItem, BackboneEnvelope, BackboneEvent, SourceInfo, TraceInfo,
};
use agentdash_application_lifecycle::lifecycle::surface::journey::{
    AgentRunJournalProjection, AgentRunJournalReader, AgentRunJournalRef, JourneyResult,
};
use agentdash_application_lifecycle::lifecycle::{LifecycleMountProvider, SessionToolResultCache};
use agentdash_application_vfs::provider::{MountOperationContext, MountProvider};
use agentdash_application_vfs::types::ListOptions;
use agentdash_domain::common::{Mount, MountCapability};
use agentdash_domain::workflow::{
    AgentSource, LifecycleAgent, LifecycleAgentRepository, LifecycleRun, LifecycleRunRepository,
};
use agentdash_platform_spi::session_persistence::SessionStoreResult;
use agentdash_platform_spi::{
    PersistedSessionEvent, SESSION_PROJECTION_KIND_MODEL_CONTEXT, SessionCompactionRecord,
    SessionCompactionStatus, SessionCompactionStore, SessionMeta, SessionMetaStore,
};
use agentdash_test_support::inline_file::MemoryInlineFileRepository;
use agentdash_test_support::skill::MemorySkillAssetRepository;
use agentdash_test_support::workflow::{
    MemoryLifecycleAgentRepository, MemoryLifecycleRunRepository,
};
use async_trait::async_trait;
use chrono::Utc;
use serde_json::{Value, json};
use uuid::Uuid;

struct FixtureSessionMetaStore;

#[async_trait]
impl SessionMetaStore for FixtureSessionMetaStore {
    async fn create_session(&self, _meta: &SessionMeta) -> SessionStoreResult<()> {
        Ok(())
    }
    async fn get_session_meta(&self, _session_id: &str) -> SessionStoreResult<Option<SessionMeta>> {
        Ok(None)
    }
    async fn list_sessions(&self) -> SessionStoreResult<Vec<SessionMeta>> {
        Ok(Vec::new())
    }
    async fn save_session_meta(&self, _meta: &SessionMeta) -> SessionStoreResult<()> {
        Ok(())
    }
    async fn delete_session(&self, _session_id: &str) -> SessionStoreResult<()> {
        Ok(())
    }
}

struct FixtureCompactionStore {
    record: SessionCompactionRecord,
}

#[async_trait]
impl SessionCompactionStore for FixtureCompactionStore {
    async fn get_compaction(
        &self,
        session_id: &str,
        compaction_id: &str,
    ) -> SessionStoreResult<Option<SessionCompactionRecord>> {
        Ok(
            (self.record.session_id == session_id && self.record.id == compaction_id)
                .then(|| self.record.clone()),
        )
    }

    async fn list_compactions(
        &self,
        session_id: &str,
        projection_kind: &str,
    ) -> SessionStoreResult<Vec<SessionCompactionRecord>> {
        Ok(
            (self.record.session_id == session_id
                && self.record.projection_kind == projection_kind)
                .then(|| vec![self.record.clone()])
                .unwrap_or_default(),
        )
    }
}

struct FixtureJournalReader {
    projection: AgentRunJournalProjection,
}

#[async_trait]
impl AgentRunJournalReader for FixtureJournalReader {
    async fn visible_journal(
        &self,
        _reference: AgentRunJournalRef,
    ) -> JourneyResult<AgentRunJournalProjection> {
        Ok(self.projection.clone())
    }
}

fn item_completed_event(
    session_id: &str,
    event_seq: u64,
    item: AgentDashThreadItem,
) -> PersistedSessionEvent {
    let envelope = BackboneEnvelope::new(
        BackboneEvent::ItemCompleted(ItemCompletedNotification::new(
            item,
            session_id.to_string(),
            "turn-1".to_string(),
        )),
        session_id,
        SourceInfo {
            connector_id: "fixture".to_string(),
            connector_type: "local_executor".to_string(),
            executor_id: None,
        },
    )
    .with_trace(TraceInfo {
        turn_id: Some("turn-1".to_string()),
        entry_index: Some(event_seq as u32 - 1),
    });
    PersistedSessionEvent {
        session_id: session_id.to_string(),
        event_seq,
        occurred_at_ms: event_seq as i64,
        committed_at_ms: event_seq as i64,
        session_update_type: "item_completed".to_string(),
        turn_id: Some("turn-1".to_string()),
        entry_index: Some(event_seq as u32 - 1),
        tool_call_id: None,
        ephemeral: false,
        notification: envelope,
    }
}

#[tokio::main]
async fn main() {
    let project_id = Uuid::nil();
    let mut run = LifecycleRun::new_plain(project_id);
    run.id = Uuid::parse_str("10000000-0000-0000-0000-000000000001").unwrap();
    let mut agent = LifecycleAgent::new_root(run.id, project_id, AgentSource::ProjectAgent);
    agent.id = Uuid::parse_str("20000000-0000-0000-0000-000000000002").unwrap();
    let projection_session_id = format!("agentrun:{}:{}", run.id, agent.id);

    let run_repo = Arc::new(MemoryLifecycleRunRepository::default());
    run_repo.create(&run).await.unwrap();
    let agent_repo = Arc::new(MemoryLifecycleAgentRepository::default());
    agent_repo.create(&agent).await.unwrap();

    let mcp_result = codex::McpToolCallResult {
        content: vec![json!({"type": "text", "text": "mcp retained body"})],
        meta: Some(json!({
            "truncation": {"policy": "head_tail", "originalBytes": 4096}
        })),
        structured_content: None,
    };
    let mcp_body = serde_json::to_string_pretty(&mcp_result).unwrap();
    let command_item: codex::ThreadItem = serde_json::from_value(json!({
        "type": "commandExecution",
        "id": "turn-1:command",
        "command": "echo complete",
        "cwd": "D:/workspace",
        "processId": null,
        "source": "agent",
        "status": "completed",
        "commandActions": [],
        "aggregatedOutput": "command complete body\n",
        "exitCode": 0,
        "durationMs": 10
    }))
    .unwrap();
    let events = vec![
        item_completed_event(
            &projection_session_id,
            1,
            codex::ThreadItem::AgentMessage {
                id: "turn-1:message".to_string(),
                text: "canonical assistant body".to_string(),
                phase: None,
                memory_citation: None,
            }
            .into(),
        ),
        item_completed_event(&projection_session_id, 2, command_item.into()),
        item_completed_event(
            &projection_session_id,
            3,
            codex::ThreadItem::McpToolCall {
                arguments: json!({"path": "large.log"}),
                duration_ms: Some(20),
                error: None,
                id: "turn-1:mcp".to_string(),
                mcp_app_resource_uri: None,
                plugin_id: None,
                result: Some(Box::new(mcp_result)),
                server: "fixture-server".to_string(),
                status: codex::McpToolCallStatus::Completed,
                tool: "read".to_string(),
            }
            .into(),
        ),
    ];
    let cache = SessionToolResultCache::new();
    cache.put_text(
        "actual-runtime-thread",
        "turn-1:command",
        "command complete body\n",
        "command complete body\n".len(),
    );
    cache.put_text(
        "actual-runtime-thread",
        "turn-1:mcp",
        &mcp_body,
        4096,
    );
    let compaction = SessionCompactionRecord {
        id: "compact-1".to_string(),
        session_id: "actual-runtime-thread".to_string(),
        projection_kind: SESSION_PROJECTION_KIND_MODEL_CONTEXT.to_string(),
        projection_version: 1,
        lifecycle_item_id: "item-compact-1".to_string(),
        start_event_seq: 1,
        completed_event_seq: Some(4),
        failed_event_seq: None,
        status: SessionCompactionStatus::ProjectionCommitted,
        trigger: String::new(),
        reason: None,
        phase: None,
        strategy: String::new(),
        budget_scope: None,
        base_head_event_seq: None,
        source_start_event_seq: Some(1),
        source_end_event_seq: Some(3),
        first_kept_event_seq: None,
        summary: "canonical compacted summary".to_string(),
        replacement_projection_json: Value::Null,
        token_stats_json: json!({"tokens_before": 42}),
        diagnostics_json: Value::Null,
        created_by: None,
        created_at_ms: 1,
        completed_at_ms: Some(4),
    };
    let provider = LifecycleMountProvider::new_with_tool_result_cache(
        run_repo,
        agent_repo,
        Arc::new(MemoryInlineFileRepository::default()),
        Arc::new(MemorySkillAssetRepository::default()),
        Arc::new(FixtureSessionMetaStore),
        Arc::new(FixtureCompactionStore { record: compaction }),
        cache,
        Arc::new(FixtureJournalReader {
            projection: AgentRunJournalProjection {
                delivery_runtime_session_id: "actual-runtime-thread".to_string(),
                events,
            },
        }),
    );
    let mount = Mount {
        id: "lifecycle".to_string(),
        provider: "lifecycle_vfs".to_string(),
        backend_id: "backend".to_string(),
        root_ref: format!("lifecycle://run/{}/session", run.id),
        capabilities: vec![MountCapability::Read, MountCapability::List],
        default_write: false,
        display_name: "Lifecycle".to_string(),
        metadata: json!({
            "scope": "agent_run_session",
            "run_id": run.id.to_string(),
            "agent_id": agent.id.to_string(),
            "runtime_session_id": "actual-runtime-thread",
            "launch_frame_id": Uuid::nil().to_string(),
        }),
    };

    let context = MountOperationContext::default();
    let message_list = provider
        .list(
            &mount,
            &ListOptions {
                path: "session/messages".to_string(),
                pattern: None,
                recursive: false,
            },
            &context,
        )
        .await
        .unwrap();
    let message_paths = message_list
        .entries
        .iter()
        .filter(|entry| !entry.is_dir)
        .map(|entry| entry.path.clone())
        .collect::<Vec<_>>();
    let message_body = provider
        .read_text(&mount, &message_paths[0], &context)
        .await
        .unwrap()
        .content;

    let tool_list = provider
        .list(
            &mount,
            &ListOptions {
                path: "session/tool-results".to_string(),
                pattern: None,
                recursive: true,
            },
            &context,
        )
        .await
        .unwrap();
    let tool_paths = tool_list
        .entries
        .iter()
        .map(|entry| entry.path.clone())
        .collect::<Vec<_>>();
    let tool_index = provider
        .read_text(&mount, "session/tool-results", &context)
        .await
        .unwrap();
    let metadata: Vec<Value> = serde_json::from_str(&tool_index.content).unwrap();
    let mut tool_reads = Vec::new();
    for metadata in metadata {
        let result_path = metadata["result_path"].as_str().unwrap();
        let body = provider
            .read_text(&mount, result_path, &context)
            .await
            .unwrap()
            .content;
        tool_reads.push(json!({
            "metadata": metadata,
            "body": body,
        }));
    }

    let summary_list = provider
        .list(
            &mount,
            &ListOptions {
                path: "session/summaries".to_string(),
                pattern: None,
                recursive: false,
            },
            &context,
        )
        .await
        .unwrap();
    let summary_paths = summary_list
        .entries
        .iter()
        .filter(|entry| !entry.is_dir)
        .map(|entry| entry.path.clone())
        .collect::<Vec<_>>();
    let summary_markdown = provider
        .read_text(&mount, &summary_paths[0], &context)
        .await
        .unwrap()
        .content;
    let capture = json!({
        "messages": {
            "list_paths": message_paths,
            "reads": [{"path": message_paths[0], "body": message_body}],
        },
        "tool_results": {"list_paths": tool_paths, "reads": tool_reads},
        "summaries": {
            "list_paths": summary_paths,
            "reads": [{
                "path": summary_paths[0],
                "summary": "canonical compacted summary",
                "trigger": Value::Null,
                "strategy": Value::Null,
                "contains_summary": summary_markdown.ends_with("canonical compacted summary"),
                "contains_null_trigger": summary_markdown.contains("\"trigger\": \"\""),
                "contains_null_strategy": summary_markdown.contains("\"strategy\": \"\""),
            }],
        },
    });
    let expected: Value = serde_json::from_str(include_str!(
        "../../../../crates/agentdash-agent-runtime-test-support/fixtures/session-parity/main/lifecycle-vfs-observables.json"
    )).unwrap();
    assert_eq!(capture, expected["protected_observables"]);
    eprintln!(
        "pinned Main Lifecycle VFS observable capture verified at {}",
        Utc::now()
    );
}
