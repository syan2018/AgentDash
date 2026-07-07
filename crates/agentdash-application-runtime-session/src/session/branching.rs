use std::io;

use agentdash_agent_protocol::{BackboneEnvelope, BackboneEvent, PlatformEvent, SourceInfo};
use agentdash_agent_types::{AgentContextEnvelope, AgentMessage, MessageRef, ProjectedEntry};
use agentdash_spi::SESSION_PROJECTION_KIND_MODEL_CONTEXT;

use super::context_projector::ContextProjector;
use super::hub_support::{TurnTerminalKind, parse_turn_terminal_event_from_envelope};
use super::persistence::{
    CompactionProjectionCommitResult, NewCompactionProjectionCommit, PersistedSessionEvent,
    SessionBranchingStores, SessionCompactionRecord, SessionCompactionStatus, SessionLineageRecord,
    SessionLineageRelationKind, SessionLineageStatus, SessionProjectionHeadRecord,
    SessionProjectionSegmentRecord,
};
use super::types::{ExecutionStatus, SessionMeta};

#[derive(Debug, Clone)]
pub struct SessionForkRequest {
    pub parent_session_id: String,
    pub title: Option<String>,
    pub fork_point_ref: Option<MessageRef>,
    pub fork_point_compaction_id: Option<String>,
    pub metadata_json: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct SessionForkResult {
    pub parent_session_id: String,
    pub child_session: SessionMeta,
    pub lineage: SessionLineageRecord,
    pub projection_commit: CompactionProjectionCommitResult,
}

#[derive(Clone)]
pub struct SessionBranchingService {
    stores: SessionBranchingStores,
}

impl SessionBranchingService {
    pub(in crate::session) fn new(stores: SessionBranchingStores) -> Self {
        Self { stores }
    }

    pub async fn fork_session(&self, request: SessionForkRequest) -> io::Result<SessionForkResult> {
        let parent = self
            .stores
            .meta
            .get_session_meta(&request.parent_session_id)
            .await?
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("parent session {} 不存在", request.parent_session_id),
                )
            })?;

        let fork_point = self.resolve_fork_point(&request).await?;
        let relation_kind = SessionLineageRelationKind::Fork;
        let projector = ContextProjector::new(self.stores.projection_stores());
        let parent_context = if let Some(compaction_id) = fork_point.compaction_id.as_deref() {
            projector
                .build_model_context_from_compaction(
                    &parent.id,
                    compaction_id,
                    Some(fork_point.event_seq),
                )
                .await?
        } else {
            projector
                .build_model_context_at_event(&parent.id, fork_point.event_seq)
                .await?
        };

        let now = chrono::Utc::now().timestamp_millis();
        let child = child_session_meta(&parent, request.title.as_deref(), now);
        let lineage = SessionLineageRecord {
            child_session_id: child.id.clone(),
            parent_session_id: parent.id.clone(),
            relation_kind,
            fork_point_event_seq: Some(fork_point.event_seq),
            fork_point_ref_json: request
                .fork_point_ref
                .as_ref()
                .map(serde_json::to_value)
                .transpose()
                .map_err(|error| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("fork point ref 序列化失败: {error}"),
                    )
                })?
                .unwrap_or_else(|| serde_json::json!({})),
            fork_point_compaction_id: fork_point
                .compaction_id
                .clone()
                .or_else(|| parent_context.active_compaction_id.clone()),
            status: SessionLineageStatus::Open,
            created_at_ms: now,
            updated_at_ms: now,
            metadata_json: request.metadata_json,
        };

        self.stores.meta.create_session(&child).await?;
        if let Err(error) = self
            .stores
            .lineage
            .upsert_session_lineage(lineage.clone())
            .await
        {
            let _ = self.stores.meta.delete_session(&child.id).await;
            return Err(error.into());
        }

        let commit = build_initial_fork_projection_commit(
            &parent,
            &child,
            &fork_point,
            &parent_context,
            relation_kind,
            now,
        )?;
        let projection_commit = match self
            .stores
            .projections
            .commit_compaction_projection(&child.id, commit)
            .await
        {
            Ok(result) => result,
            Err(error) => {
                let _ = self.stores.meta.delete_session(&child.id).await;
                return Err(error.into());
            }
        };

        Ok(SessionForkResult {
            parent_session_id: parent.id,
            child_session: child,
            lineage,
            projection_commit,
        })
    }

    pub async fn lineage_parent(
        &self,
        session_id: &str,
    ) -> io::Result<Option<SessionLineageRecord>> {
        self.stores
            .lineage
            .get_session_lineage(session_id)
            .await
            .map_err(Into::into)
    }

    async fn resolve_fork_point(
        &self,
        request: &SessionForkRequest,
    ) -> io::Result<ResolvedForkPoint> {
        let events = self
            .stores
            .events
            .list_all_events(&request.parent_session_id)
            .await?;
        let latest_event_seq = events
            .iter()
            .map(|event| event.event_seq)
            .max()
            .unwrap_or_default();
        let current_head = self
            .stores
            .projections
            .read_projection_head(
                &request.parent_session_id,
                SESSION_PROJECTION_KIND_MODEL_CONTEXT,
            )
            .await?;
        let current_head_event_seq = current_head
            .as_ref()
            .map(|head| head.head_event_seq)
            .unwrap_or(latest_event_seq);
        let current_active_compaction_id = current_head
            .as_ref()
            .and_then(|head| head.active_compaction_id.clone());

        let requested_compaction =
            if let Some(compaction_id) = request.fork_point_compaction_id.as_deref() {
                Some(
                    self.committed_compaction_for_projection_restore(
                        &request.parent_session_id,
                        compaction_id,
                        "fork point",
                    )
                    .await?,
                )
            } else {
                None
            };
        let mut compaction_id = requested_compaction
            .as_ref()
            .map(|compaction| compaction.id.clone());
        let event_seq = if let Some(message_ref) = request.fork_point_ref.as_ref() {
            resolve_message_ref_event_seq(&self.stores, &request.parent_session_id, message_ref)
                .await?
        } else if let Some(compaction) = requested_compaction.as_ref() {
            compaction
                .completed_event_seq
                .or(compaction.source_end_event_seq)
                .unwrap_or(current_head_event_seq)
        } else {
            compaction_id = current_active_compaction_id.clone();
            current_head_event_seq
        };

        if event_seq > current_head_event_seq {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("fork point {event_seq} 超过当前模型可见 head {current_head_event_seq}"),
            ));
        }
        if let Some(compaction) = requested_compaction.as_ref()
            && !compaction_valid_for_head(compaction, event_seq)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "fork point compaction {} 不能覆盖 event head {}",
                    compaction.id, event_seq
                ),
            ));
        }

        Ok(ResolvedForkPoint {
            event_seq,
            compaction_id,
            parent_head_event_seq: current_head_event_seq,
            parent_active_compaction_id: current_active_compaction_id,
        })
    }

    async fn committed_compaction_for_projection_restore(
        &self,
        session_id: &str,
        compaction_id: &str,
        usage: &str,
    ) -> io::Result<SessionCompactionRecord> {
        let compaction = self
            .stores
            .compactions
            .get_compaction(session_id, compaction_id)
            .await?
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("{usage} compaction {compaction_id} 不存在"),
                )
            })?;
        if compaction.status != SessionCompactionStatus::ProjectionCommitted {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "{usage} compaction {} 状态不是 projection_committed",
                    compaction.id
                ),
            ));
        }
        Ok(compaction)
    }
}

#[derive(Debug, Clone)]
struct ResolvedForkPoint {
    event_seq: u64,
    compaction_id: Option<String>,
    parent_head_event_seq: u64,
    parent_active_compaction_id: Option<String>,
}

fn child_session_meta(_parent: &SessionMeta, _title: Option<&str>, now: i64) -> SessionMeta {
    let id = format!("sess-{}-{}", now, &uuid::Uuid::new_v4().to_string()[..8]);
    SessionMeta {
        id,
        created_at: now,
        updated_at: now,
        last_event_seq: 0,
        last_delivery_status: ExecutionStatus::Idle,
        last_turn_id: None,
        last_terminal_message: None,
        executor_session_id: None,
    }
}

fn build_initial_fork_projection_commit(
    parent: &SessionMeta,
    child: &SessionMeta,
    fork_point: &ResolvedForkPoint,
    parent_context: &AgentContextEnvelope,
    relation_kind: SessionLineageRelationKind,
    now: i64,
) -> io::Result<NewCompactionProjectionCommit> {
    let lifecycle_item_id = format!("session-fork-{}", child.id);
    let compaction_id = format!("fork-initial-{}", child.id);
    let segment_id = format!("{compaction_id}-context");
    let source_refs = serde_json::json!({
        "parent_session_id": parent.id,
        "fork_point_event_seq": fork_point.event_seq,
        "parent_head_event_seq": fork_point.parent_head_event_seq,
        "parent_active_compaction_id": fork_point.parent_active_compaction_id,
        "relation_kind": relation_kind.as_str(),
    });
    let content_json = serde_json::json!({
        "parent_session_id": parent.id,
        "messages": parent_context.messages,
        "provenance": source_refs,
    });

    Ok(NewCompactionProjectionCommit {
        completed_event: branch_forked_envelope(
            &child.id,
            &parent.id,
            fork_point,
            &compaction_id,
            relation_kind,
        ),
        compaction: SessionCompactionRecord {
            id: compaction_id.clone(),
            session_id: child.id.clone(),
            projection_kind: SESSION_PROJECTION_KIND_MODEL_CONTEXT.to_string(),
            projection_version: 1,
            lifecycle_item_id,
            start_event_seq: 0,
            completed_event_seq: None,
            failed_event_seq: None,
            status: SessionCompactionStatus::ProjectionCommitted,
            trigger: "session_fork".to_string(),
            reason: Some("materialize_child_initial_projection".to_string()),
            phase: Some("session_branching".to_string()),
            strategy: "fork_initial_projection".to_string(),
            budget_scope: Some(SESSION_PROJECTION_KIND_MODEL_CONTEXT.to_string()),
            base_head_event_seq: Some(0),
            source_start_event_seq: None,
            source_end_event_seq: None,
            first_kept_event_seq: Some(2),
            summary: format!(
                "Forked from {} at event {}",
                parent.id, fork_point.event_seq
            ),
            replacement_projection_json: serde_json::json!({
                "projection_kind": SESSION_PROJECTION_KIND_MODEL_CONTEXT,
                "projection_version": 1,
                "context_segment_id": segment_id,
                "parent_session_id": parent.id,
                "fork_point_event_seq": fork_point.event_seq,
                "parent_active_compaction_id": fork_point.parent_active_compaction_id,
                "message_count": parent_context.messages.len(),
            }),
            token_stats_json: serde_json::json!({
                "token_estimate": parent_context.token_estimate,
                "message_count": parent_context.messages.len(),
            }),
            diagnostics_json: serde_json::json!({}),
            created_by: Some("session_branching".to_string()),
            created_at_ms: now,
            completed_at_ms: None,
        },
        segments: vec![SessionProjectionSegmentRecord {
            id: segment_id,
            session_id: child.id.clone(),
            projection_kind: SESSION_PROJECTION_KIND_MODEL_CONTEXT.to_string(),
            projection_version: 1,
            sort_order: 0,
            segment_type: "context_envelope".to_string(),
            origin: "projection".to_string(),
            synthetic: true,
            source_start_event_seq: None,
            source_end_event_seq: None,
            source_refs_json: source_refs,
            generated_by_compaction_id: Some(compaction_id.clone()),
            content_json,
            token_estimate: parent_context.token_estimate,
            created_at_ms: now,
        }],
        head: SessionProjectionHeadRecord {
            session_id: child.id.clone(),
            projection_kind: SESSION_PROJECTION_KIND_MODEL_CONTEXT.to_string(),
            projection_version: 1,
            head_event_seq: 1,
            active_compaction_id: Some(compaction_id),
            updated_by_event_seq: None,
            updated_at_ms: 0,
        },
    })
}

fn branch_forked_envelope(
    child_session_id: &str,
    parent_session_id: &str,
    fork_point: &ResolvedForkPoint,
    child_initial_compaction_id: &str,
    relation_kind: SessionLineageRelationKind,
) -> BackboneEnvelope {
    BackboneEnvelope::new(
        BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
            key: "session_branch_forked".to_string(),
            value: serde_json::json!({
                "child_session_id": child_session_id,
                "parent_session_id": parent_session_id,
                "fork_point_event_seq": fork_point.event_seq,
                "fork_point_compaction_id": fork_point.compaction_id,
                "parent_head_event_seq": fork_point.parent_head_event_seq,
                "parent_active_compaction_id": fork_point.parent_active_compaction_id,
                "child_initial_compaction_id": child_initial_compaction_id,
                "relation_kind": relation_kind.as_str(),
            }),
        }),
        child_session_id,
        platform_source(),
    )
    .with_trace(agentdash_agent_protocol::TraceInfo {
        turn_id: Some(format!("session-fork:{child_session_id}")),
        entry_index: None,
    })
}

fn platform_source() -> SourceInfo {
    SourceInfo {
        connector_id: "agentdash-session-tree".to_string(),
        connector_type: "platform".to_string(),
        executor_id: None,
    }
}

async fn resolve_message_ref_event_seq(
    stores: &SessionBranchingStores,
    session_id: &str,
    message_ref: &MessageRef,
) -> io::Result<u64> {
    let transcript = ContextProjector::new(stores.projection_stores())
        .build_projected_transcript(session_id)
        .await?;
    let (index, entry) = transcript
        .entries
        .iter()
        .enumerate()
        .find(|(_, entry)| &entry.message_ref == message_ref)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "message ref {}:{} 不在当前投影中",
                    message_ref.turn_id, message_ref.entry_index
                ),
            )
        })?;
    validate_fork_point_message_boundary(&transcript.entries, index, entry)?;
    let event_seq = entry
        .source_event_seq
        .or_else(|| entry.source_range.as_ref().map(|range| range.end_event_seq))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "message ref {}:{} 缺少可解析的 source range",
                    message_ref.turn_id, message_ref.entry_index
                ),
            )
        })?;
    let events = stores.events.list_all_events(session_id).await?;
    ensure_fork_point_turn_completed(&events, entry, event_seq)?;
    Ok(event_seq)
}

fn validate_fork_point_message_boundary(
    entries: &[ProjectedEntry],
    index: usize,
    entry: &ProjectedEntry,
) -> io::Result<()> {
    match &entry.message {
        AgentMessage::Assistant { tool_calls, .. } if !tool_calls.is_empty() => {
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "fork point 不能停在 assistant tool call 之后、tool result 之前",
            ))
        }
        AgentMessage::ToolResult { tool_call_id, .. } => {
            ensure_tool_results_complete_at_boundary(entries, index, tool_call_id)
        }
        _ => Ok(()),
    }
}

fn ensure_tool_results_complete_at_boundary(
    entries: &[ProjectedEntry],
    index: usize,
    selected_tool_call_id: &str,
) -> io::Result<()> {
    let Some((assistant_index, required_tool_call_ids)) =
        preceding_tool_call_group(entries, index, selected_tool_call_id)
    else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("fork point tool result {selected_tool_call_id} 缺少对应 tool call"),
        ));
    };

    let mut completed_tool_call_ids = Vec::new();
    for entry in &entries[assistant_index + 1..=index] {
        if let AgentMessage::ToolResult {
            tool_call_id,
            call_id,
            ..
        } = &entry.message
        {
            completed_tool_call_ids.push(tool_call_id.as_str());
            if let Some(call_id) = call_id.as_deref() {
                completed_tool_call_ids.push(call_id);
            }
        }
    }
    let complete = required_tool_call_ids
        .iter()
        .all(|required| completed_tool_call_ids.iter().any(|done| *done == required));
    if complete {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "fork point 不能落在未完整返回的 tool result 组中",
        ))
    }
}

fn preceding_tool_call_group(
    entries: &[ProjectedEntry],
    index: usize,
    selected_tool_call_id: &str,
) -> Option<(usize, Vec<String>)> {
    entries[..index]
        .iter()
        .enumerate()
        .rev()
        .find_map(|(assistant_index, entry)| {
            let AgentMessage::Assistant { tool_calls, .. } = &entry.message else {
                return None;
            };
            let mut ids = Vec::new();
            for tool_call in tool_calls {
                ids.push(tool_call.id.clone());
                if let Some(call_id) = tool_call.call_id.as_deref() {
                    ids.push(call_id.to_string());
                }
            }
            ids.iter()
                .any(|id| id == selected_tool_call_id)
                .then_some((assistant_index, ids))
        })
}

fn ensure_fork_point_turn_completed(
    events: &[PersistedSessionEvent],
    entry: &ProjectedEntry,
    event_seq: u64,
) -> io::Result<()> {
    if entry.synthetic || entry.message_ref.turn_id.starts_with("_projection:") {
        return Ok(());
    }
    let turn_id = entry.message_ref.turn_id.as_str();
    let completed = events.iter().any(|event| {
        event.event_seq >= event_seq
            && event
                .turn_id
                .as_deref()
                .or(event.notification.trace.turn_id.as_deref())
                == Some(turn_id)
            && matches!(
                parse_turn_terminal_event_from_envelope(&event.notification),
                Some((terminal_turn_id, TurnTerminalKind::Completed, _, _))
                    if terminal_turn_id == turn_id
            )
    });
    if completed {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("fork point 指向的 turn {turn_id} 尚未完成"),
        ))
    }
}

fn compaction_valid_for_head(compaction: &SessionCompactionRecord, head_event_seq: u64) -> bool {
    if compaction.strategy == "fork_initial_projection" {
        return true;
    }
    compaction
        .source_end_event_seq
        .map(|source_end| source_end <= head_event_seq)
        .unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use agentdash_agent_protocol::{
        ItemCompletedNotification, TraceInfo, UserInputSubmissionKind,
        UserInputSubmittedNotification, codex_app_server_protocol as codex,
    };
    use agentdash_agent_types::{
        AgentInputMessage, ContentPart, ProjectionKind, ProjectionOrigin, ToolCallInfo,
    };
    use agentdash_spi::session_persistence::{
        SessionLineageStore, SessionMetaStore, SessionProjectionStore, SessionStoreError,
        SessionStoreResult,
    };
    use async_trait::async_trait;

    use super::*;
    use crate::session::{
        memory_persistence::FixtureRuntimeTraceStore, persistence::SessionStoreSet,
    };

    fn test_stores() -> (Arc<FixtureRuntimeTraceStore>, SessionStoreSet) {
        let persistence = Arc::new(FixtureRuntimeTraceStore::default());
        let stores = SessionStoreSet::from_runtime_trace_test_store(persistence.clone());
        (persistence, stores)
    }

    fn session_meta(id: &str) -> SessionMeta {
        SessionMeta {
            id: id.to_string(),
            created_at: 1,
            updated_at: 1,
            last_event_seq: 0,
            last_delivery_status: ExecutionStatus::Idle,
            last_turn_id: None,
            last_terminal_message: None,
            executor_session_id: None,
        }
    }

    fn user_message(session_id: &str, turn_id: &str, index: u32, text: &str) -> BackboneEnvelope {
        BackboneEnvelope::new(
            BackboneEvent::UserInputSubmitted(UserInputSubmittedNotification::new(
                session_id,
                turn_id,
                format!("{turn_id}:user:{index}"),
                UserInputSubmissionKind::Prompt,
                vec![codex::UserInput::Text {
                    text: text.to_string(),
                    text_elements: Vec::new(),
                }],
            )),
            session_id,
            platform_source(),
        )
        .with_trace(TraceInfo {
            turn_id: Some(turn_id.to_string()),
            entry_index: Some(index),
        })
    }

    fn turn_completed(session_id: &str, turn_id: &str) -> BackboneEnvelope {
        BackboneEnvelope::new(
            BackboneEvent::TurnCompleted(codex::TurnCompletedNotification {
                thread_id: session_id.to_string(),
                turn: codex::Turn {
                    id: turn_id.to_string(),
                    items: Vec::new(),
                    items_view: codex::TurnItemsView::NotLoaded,
                    status: codex::TurnStatus::Completed,
                    error: None,
                    started_at: None,
                    completed_at: Some(chrono::Utc::now().timestamp()),
                    duration_ms: None,
                },
            }),
            session_id,
            platform_source(),
        )
        .with_trace(TraceInfo {
            turn_id: Some(turn_id.to_string()),
            entry_index: None,
        })
    }

    fn platform_turn_completed(session_id: &str, turn_id: &str) -> BackboneEnvelope {
        crate::session::hub_support::build_turn_terminal_envelope_with_timing(
            session_id,
            &platform_source(),
            turn_id,
            TurnTerminalKind::Completed,
            Some("done".to_string()),
            None,
            None,
        )
    }

    fn dynamic_tool_call(
        session_id: &str,
        turn_id: &str,
        index: u32,
        tool_call_id: &str,
        status: codex::DynamicToolCallStatus,
    ) -> BackboneEnvelope {
        let content_items = matches!(
            status,
            codex::DynamicToolCallStatus::Completed | codex::DynamicToolCallStatus::Failed
        )
        .then(|| {
            vec![codex::DynamicToolCallOutputContentItem::InputText {
                text: format!("{tool_call_id} result"),
            }]
        });
        let item = codex::ThreadItem::DynamicToolCall {
            id: tool_call_id.to_string(),
            namespace: None,
            tool: "Read".to_string(),
            arguments: serde_json::json!({ "path": "Cargo.toml" }),
            status,
            content_items,
            success: Some(true),
            duration_ms: None,
        };
        BackboneEnvelope::new(
            BackboneEvent::ItemCompleted(ItemCompletedNotification::new(
                item,
                session_id.to_string(),
                turn_id.to_string(),
            )),
            session_id,
            platform_source(),
        )
        .with_trace(TraceInfo {
            turn_id: Some(turn_id.to_string()),
            entry_index: Some(index),
        })
    }

    fn basic_fork_request(parent_session_id: &str) -> SessionForkRequest {
        SessionForkRequest {
            parent_session_id: parent_session_id.to_string(),
            title: Some("child".to_string()),
            fork_point_ref: None,
            fork_point_compaction_id: None,
            metadata_json: serde_json::json!({}),
        }
    }

    fn compaction_commit(
        session_id: &str,
        compaction_id: &str,
        source_end_event_seq: u64,
    ) -> NewCompactionProjectionCommit {
        let segment_id = format!("{compaction_id}-context");
        NewCompactionProjectionCommit {
            completed_event: BackboneEnvelope::new(
                BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
                    key: "context_compacted".to_string(),
                    value: serde_json::json!({ "compaction_id": compaction_id }),
                }),
                session_id,
                platform_source(),
            ),
            compaction: SessionCompactionRecord {
                id: compaction_id.to_string(),
                session_id: session_id.to_string(),
                projection_kind: SESSION_PROJECTION_KIND_MODEL_CONTEXT.to_string(),
                projection_version: 1,
                lifecycle_item_id: format!("{compaction_id}-item"),
                start_event_seq: 1,
                completed_event_seq: None,
                failed_event_seq: None,
                status: SessionCompactionStatus::ProjectionCommitted,
                trigger: "test".to_string(),
                reason: Some("test".to_string()),
                phase: Some("pre_provider".to_string()),
                strategy: "summary_prefix".to_string(),
                budget_scope: Some(SESSION_PROJECTION_KIND_MODEL_CONTEXT.to_string()),
                base_head_event_seq: Some(source_end_event_seq),
                source_start_event_seq: Some(1),
                source_end_event_seq: Some(source_end_event_seq),
                first_kept_event_seq: Some(source_end_event_seq.saturating_add(1)),
                summary: "summary".to_string(),
                replacement_projection_json: serde_json::json!({}),
                token_stats_json: serde_json::json!({}),
                diagnostics_json: serde_json::json!({}),
                created_by: Some("test".to_string()),
                created_at_ms: 1,
                completed_at_ms: Some(2),
            },
            segments: vec![SessionProjectionSegmentRecord {
                id: segment_id,
                session_id: session_id.to_string(),
                projection_kind: SESSION_PROJECTION_KIND_MODEL_CONTEXT.to_string(),
                projection_version: 1,
                sort_order: 0,
                segment_type: "context_envelope".to_string(),
                origin: "projection".to_string(),
                synthetic: true,
                source_start_event_seq: Some(1),
                source_end_event_seq: Some(source_end_event_seq),
                source_refs_json: serde_json::json!({}),
                generated_by_compaction_id: Some(compaction_id.to_string()),
                content_json: serde_json::json!({
                    "messages": [AgentInputMessage {
                        message_ref: MessageRef {
                            turn_id: "turn-1".to_string(),
                            entry_index: 0,
                        },
                        projection_kind: ProjectionKind::ModelContext,
                        message: AgentMessage::assistant("summary"),
                        origin: ProjectionOrigin::Event,
                        synthetic: false,
                        source_event_seq: Some(source_end_event_seq),
                        source_range: None,
                        projection_segment_id: None,
                        provenance: serde_json::json!({}),
                    }]
                }),
                token_estimate: Some(8),
                created_at_ms: 2,
            }],
            head: SessionProjectionHeadRecord {
                session_id: session_id.to_string(),
                projection_kind: SESSION_PROJECTION_KIND_MODEL_CONTEXT.to_string(),
                projection_version: 1,
                head_event_seq: 0,
                active_compaction_id: Some(compaction_id.to_string()),
                updated_by_event_seq: None,
                updated_at_ms: 0,
            },
        }
    }

    #[derive(Clone)]
    struct FailingProjectionStore {
        inner: Arc<FixtureRuntimeTraceStore>,
    }

    #[async_trait]
    impl SessionProjectionStore for FailingProjectionStore {
        async fn list_projection_segments(
            &self,
            session_id: &str,
            projection_kind: &str,
            projection_version: u64,
        ) -> SessionStoreResult<Vec<SessionProjectionSegmentRecord>> {
            self.inner
                .list_projection_segments(session_id, projection_kind, projection_version)
                .await
        }

        async fn read_projection_head(
            &self,
            session_id: &str,
            projection_kind: &str,
        ) -> SessionStoreResult<Option<SessionProjectionHeadRecord>> {
            self.inner
                .read_projection_head(session_id, projection_kind)
                .await
        }

        async fn upsert_projection_head(
            &self,
            head: SessionProjectionHeadRecord,
        ) -> SessionStoreResult<()> {
            self.inner.upsert_projection_head(head).await
        }

        async fn commit_compaction_projection(
            &self,
            _session_id: &str,
            _commit: NewCompactionProjectionCommit,
        ) -> SessionStoreResult<CompactionProjectionCommitResult> {
            Err(SessionStoreError::InvalidInput(
                "projection commit failed for test".to_string(),
            ))
        }
    }

    #[tokio::test]
    async fn fork_session_materializes_child_initial_projection() {
        let (_persistence, stores) = test_stores();
        stores
            .meta
            .create_session(&session_meta("parent"))
            .await
            .expect("应能创建 parent");
        stores
            .events
            .append_event("parent", &user_message("parent", "turn-1", 0, "hello"))
            .await
            .expect("应能写入 parent message");

        let service = SessionBranchingService::new(stores.branching_stores());
        let result = service
            .fork_session(basic_fork_request("parent"))
            .await
            .expect("fork 应成功");

        assert_eq!(result.lineage.parent_session_id, "parent");
        assert_eq!(
            result
                .projection_commit
                .head
                .active_compaction_id
                .as_deref(),
            Some(result.projection_commit.compaction.id.as_str())
        );

        stores
            .events
            .append_event("parent", &user_message("parent", "turn-2", 0, "after fork"))
            .await
            .expect("应能继续写入 parent");

        let child_context = ContextProjector::new(stores.projection_stores())
            .build_model_context(&result.child_session.id)
            .await
            .expect("应能恢复 child context");
        assert_eq!(child_context.messages.len(), 1);
        assert_eq!(
            child_context.messages[0].message.first_text(),
            Some("hello")
        );
        assert!(child_context.messages[0].synthetic);
        assert_eq!(child_context.messages[0].origin.as_str(), "projection");
    }

    #[tokio::test]
    async fn fork_message_ref_requires_completed_turn() {
        let (_persistence, stores) = test_stores();
        stores
            .meta
            .create_session(&session_meta("parent"))
            .await
            .expect("应能创建 parent");
        stores
            .events
            .append_event("parent", &user_message("parent", "turn-1", 0, "hello"))
            .await
            .expect("应能写入 parent message");

        let service = SessionBranchingService::new(stores.branching_stores());
        let mut request = basic_fork_request("parent");
        request.fork_point_ref = Some(MessageRef {
            turn_id: "turn-1".to_string(),
            entry_index: 0,
        });

        let unfinished = service.fork_session(request.clone()).await;
        assert!(matches!(
            unfinished,
            Err(error) if error.kind() == io::ErrorKind::InvalidInput
        ));

        stores
            .events
            .append_event("parent", &turn_completed("parent", "turn-1"))
            .await
            .expect("应能写入 turn completed");

        let completed = service
            .fork_session(request)
            .await
            .expect("completed turn message ref fork 应成功");
        let context = ContextProjector::new(stores.projection_stores())
            .build_model_context(&completed.child_session.id)
            .await
            .expect("应能恢复 child context");
        assert_eq!(context.messages.len(), 1);
        assert_eq!(context.messages[0].message.first_text(), Some("hello"));
    }

    #[tokio::test]
    async fn fork_message_ref_accepts_platform_turn_terminal_completion() {
        let (_persistence, stores) = test_stores();
        stores
            .meta
            .create_session(&session_meta("parent"))
            .await
            .expect("应能创建 parent");
        stores
            .events
            .append_event("parent", &user_message("parent", "turn-1", 0, "hello"))
            .await
            .expect("应能写入 parent message");
        stores
            .events
            .append_event("parent", &platform_turn_completed("parent", "turn-1"))
            .await
            .expect("应能写入 turn terminal");

        let mut request = basic_fork_request("parent");
        request.fork_point_ref = Some(MessageRef {
            turn_id: "turn-1".to_string(),
            entry_index: 0,
        });

        let completed = SessionBranchingService::new(stores.branching_stores())
            .fork_session(request)
            .await
            .expect("platform turn_terminal completed 应允许 fork");
        let context = ContextProjector::new(stores.projection_stores())
            .build_model_context(&completed.child_session.id)
            .await
            .expect("应能恢复 child context");
        assert_eq!(context.messages.len(), 1);
        assert_eq!(context.messages[0].message.first_text(), Some("hello"));
    }

    #[tokio::test]
    async fn fork_rejects_assistant_tool_call_boundary() {
        let (_persistence, stores) = test_stores();
        stores
            .meta
            .create_session(&session_meta("parent"))
            .await
            .expect("应能创建 parent");
        stores
            .events
            .append_event("parent", &user_message("parent", "turn-1", 0, "hello"))
            .await
            .expect("应能写入 user message");
        stores
            .events
            .append_event(
                "parent",
                &dynamic_tool_call(
                    "parent",
                    "turn-1",
                    1,
                    "tool-1",
                    codex::DynamicToolCallStatus::Completed,
                ),
            )
            .await
            .expect("应能写入 tool call");
        stores
            .events
            .append_event("parent", &turn_completed("parent", "turn-1"))
            .await
            .expect("应能写入 turn completed");

        let mut request = basic_fork_request("parent");
        request.fork_point_ref = Some(MessageRef {
            turn_id: "turn-1".to_string(),
            entry_index: 1,
        });

        let result = SessionBranchingService::new(stores.branching_stores())
            .fork_session(request)
            .await;
        assert!(matches!(
            result,
            Err(error) if error.kind() == io::ErrorKind::InvalidInput
        ));
    }

    #[test]
    fn fork_rejects_incomplete_tool_result_group_boundary() {
        let entries = vec![
            ProjectedEntry::event(
                MessageRef {
                    turn_id: "turn-1".to_string(),
                    entry_index: 1,
                },
                ProjectionKind::Transcript,
                AgentMessage::Assistant {
                    content: vec![ContentPart::text("tool use")],
                    tool_calls: vec![
                        ToolCallInfo {
                            id: "tool-1".to_string(),
                            call_id: Some("tool-1".to_string()),
                            name: "Read".to_string(),
                            arguments: serde_json::json!({}),
                        },
                        ToolCallInfo {
                            id: "tool-2".to_string(),
                            call_id: Some("tool-2".to_string()),
                            name: "Read".to_string(),
                            arguments: serde_json::json!({}),
                        },
                    ],
                    stop_reason: None,
                    error_message: None,
                    usage: None,
                    timestamp: None,
                },
                Some(2),
            ),
            ProjectedEntry::event(
                MessageRef {
                    turn_id: "turn-1".to_string(),
                    entry_index: 2,
                },
                ProjectionKind::Transcript,
                AgentMessage::tool_result_full(
                    "tool-1",
                    Some("tool-1".to_string()),
                    Some("Read".to_string()),
                    vec![ContentPart::text("first result")],
                    None,
                    false,
                ),
                Some(3),
            ),
        ];

        let result = validate_fork_point_message_boundary(&entries, 1, &entries[1]);
        assert!(matches!(
            result,
            Err(error) if error.kind() == io::ErrorKind::InvalidInput
        ));
    }

    #[tokio::test]
    async fn fork_rejects_compaction_that_extends_past_message_ref() {
        let (persistence, stores) = test_stores();
        stores
            .meta
            .create_session(&session_meta("parent"))
            .await
            .expect("应能创建 parent");
        stores
            .events
            .append_event("parent", &user_message("parent", "turn-1", 0, "hello"))
            .await
            .expect("应能写入 user message");
        stores
            .events
            .append_event("parent", &turn_completed("parent", "turn-1"))
            .await
            .expect("应能写入 turn completed");
        persistence
            .commit_compaction_projection("parent", compaction_commit("parent", "compact-1", 2))
            .await
            .expect("应能写入 compaction");
        persistence
            .upsert_projection_head(SessionProjectionHeadRecord {
                session_id: "parent".to_string(),
                projection_kind: SESSION_PROJECTION_KIND_MODEL_CONTEXT.to_string(),
                projection_version: 1,
                head_event_seq: 3,
                active_compaction_id: None,
                updated_by_event_seq: Some(3),
                updated_at_ms: 3,
            })
            .await
            .expect("应能清空 active compaction head");

        let mut request = basic_fork_request("parent");
        request.fork_point_ref = Some(MessageRef {
            turn_id: "turn-1".to_string(),
            entry_index: 0,
        });
        request.fork_point_compaction_id = Some("compact-1".to_string());

        let result = SessionBranchingService::new(stores.branching_stores())
            .fork_session(request)
            .await;
        assert!(matches!(
            result,
            Err(error) if error.kind() == io::ErrorKind::InvalidInput
        ));
    }

    #[tokio::test]
    async fn fork_cleans_child_session_when_projection_commit_fails() {
        let (persistence, stores) = test_stores();
        stores
            .meta
            .create_session(&session_meta("parent"))
            .await
            .expect("应能创建 parent");
        stores
            .events
            .append_event("parent", &user_message("parent", "turn-1", 0, "hello"))
            .await
            .expect("应能写入 parent message");

        let failing_stores = SessionStoreSet {
            meta: persistence.clone(),
            events: persistence.clone(),
            control_effects: persistence.clone(),
            runtime_commands: persistence.clone(),
            compactions: persistence.clone(),
            projections: Arc::new(FailingProjectionStore {
                inner: persistence.clone(),
            }),
            lineage: persistence.clone(),
        };

        let result = SessionBranchingService::new(failing_stores.branching_stores())
            .fork_session(basic_fork_request("parent"))
            .await;
        assert!(matches!(
            result,
            Err(error) if error.kind() == io::ErrorKind::InvalidInput
        ));

        let sessions = persistence
            .list_sessions()
            .await
            .expect("应能读取 sessions");
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, "parent");
        let children = persistence
            .list_session_children("parent", None, None)
            .await
            .expect("应能读取 lineage children");
        assert!(children.is_empty());
    }
}
