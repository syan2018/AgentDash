use std::{collections::HashSet, io, sync::Arc};

use agentdash_agent_protocol::{
    BackboneEnvelope, BackboneEvent, PlatformEvent, SourceInfo, TraceInfo,
};
use agentdash_application_runtime_session::session::{
    SessionBranchingService, SessionLineageRecord, SessionLineageRelationKind,
};
use agentdash_spi::session_persistence::PersistedSessionEvent;
use async_trait::async_trait;
use uuid::Uuid;

use crate::agent_run::runtime_session_boundary::SessionEventingService;
use crate::error::WorkflowApplicationError;

const JOURNAL_EVENT_PAGE_SIZE: u32 = 2_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRunJournalSegmentRole {
    InheritedLineage,
    CurrentDelivery,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunJournalSegment {
    pub index: usize,
    pub role: AgentRunJournalSegmentRole,
    pub runtime_session_id: String,
    pub from_after_event_seq: u64,
    pub to_event_seq: u64,
}

#[derive(Debug, Clone)]
pub struct AgentRunJournalEvent {
    pub journal_seq: u64,
    pub segment_index: usize,
    pub segment_role: AgentRunJournalSegmentRole,
    pub source_runtime_session_id: String,
    pub source_event_seq: u64,
    pub event: PersistedSessionEvent,
}

#[derive(Debug, Clone)]
pub struct AgentRunJournalView {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub delivery_runtime_session_id: String,
    pub segments: Vec<AgentRunJournalSegment>,
    pub events: Vec<AgentRunJournalEvent>,
}

#[derive(Debug, Clone)]
pub struct AgentRunJournalPage {
    pub snapshot_seq: u64,
    pub events: Vec<AgentRunJournalEvent>,
    pub has_more: bool,
    pub next_after_seq: u64,
}

#[derive(Debug, Clone)]
pub struct AgentRunJournalQuery {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub delivery_runtime_session_id: Option<String>,
}

pub struct AgentRunJournalService {
    lineage: Arc<dyn AgentRunJournalLineagePort>,
    session_eventing: SessionEventingService,
}

impl AgentRunJournalService {
    pub fn new(
        session_branching: SessionBranchingService,
        session_eventing: SessionEventingService,
    ) -> Self {
        Self {
            lineage: Arc::new(session_branching),
            session_eventing,
        }
    }

    pub async fn load_inherited_prefix(
        &self,
        query: AgentRunJournalQuery,
    ) -> Result<AgentRunJournalView, WorkflowApplicationError> {
        self.load_journal(query, false).await
    }

    pub async fn load_visible_journal(
        &self,
        query: AgentRunJournalQuery,
    ) -> Result<AgentRunJournalView, WorkflowApplicationError> {
        self.load_journal(query, true).await
    }

    pub async fn load_visible_journal_page(
        &self,
        query: AgentRunJournalQuery,
        after_seq: u64,
        limit: u32,
    ) -> Result<AgentRunJournalPage, WorkflowApplicationError> {
        let journal = self.load_visible_journal(query).await?;
        let snapshot_seq = journal
            .events
            .last()
            .map(|event| event.journal_seq)
            .unwrap_or_default();
        let events = journal
            .events
            .into_iter()
            .filter(|event| event.journal_seq > after_seq)
            .take(limit.max(1) as usize)
            .collect::<Vec<_>>();
        let next_after_seq = events
            .last()
            .map(|event| event.journal_seq)
            .unwrap_or(after_seq);
        Ok(AgentRunJournalPage {
            snapshot_seq,
            has_more: next_after_seq < snapshot_seq,
            next_after_seq,
            events,
        })
    }

    async fn load_journal(
        &self,
        query: AgentRunJournalQuery,
        include_current_delivery: bool,
    ) -> Result<AgentRunJournalView, WorkflowApplicationError> {
        let delivery_runtime_session_id = query.delivery_runtime_session_id.ok_or_else(|| {
            WorkflowApplicationError::NotFound(
                "AgentRun 当前没有可读取的 delivery RuntimeSession".to_string(),
            )
        })?;
        let mut segment_specs = self
            .resolve_inherited_segments(&delivery_runtime_session_id)
            .await?;

        if include_current_delivery {
            let latest_seq = self.latest_event_seq(&delivery_runtime_session_id).await?;
            if latest_seq > 0 {
                segment_specs.push(UnindexedJournalSegment {
                    role: AgentRunJournalSegmentRole::CurrentDelivery,
                    runtime_session_id: delivery_runtime_session_id.clone(),
                    from_after_event_seq: 0,
                    to_event_seq: latest_seq,
                    child_runtime_session_id: None,
                });
            }
        }

        let mut journal_seq = 0_u64;
        let mut segments = Vec::with_capacity(segment_specs.len());
        let mut events = Vec::new();

        for (index, spec) in segment_specs.into_iter().enumerate() {
            let segment = AgentRunJournalSegment {
                index,
                role: spec.role,
                runtime_session_id: spec.runtime_session_id.clone(),
                from_after_event_seq: spec.from_after_event_seq,
                to_event_seq: spec.to_event_seq,
            };
            let segment_events = self.load_segment_events(&segment).await?;
            let mut last_segment_event_time = None;
            for event in segment_events {
                last_segment_event_time = Some((event.occurred_at_ms, event.committed_at_ms));
                journal_seq += 1;
                events.push(AgentRunJournalEvent {
                    journal_seq,
                    segment_index: segment.index,
                    segment_role: segment.role,
                    source_runtime_session_id: segment.runtime_session_id.clone(),
                    source_event_seq: event.event_seq,
                    event,
                });
            }
            if let Some(child_runtime_session_id) = spec.child_runtime_session_id.as_deref() {
                journal_seq += 1;
                let (occurred_at_ms, committed_at_ms) = last_segment_event_time.unwrap_or_default();
                events.push(AgentRunJournalEvent {
                    journal_seq,
                    segment_index: segment.index,
                    segment_role: AgentRunJournalSegmentRole::InheritedLineage,
                    source_runtime_session_id: segment.runtime_session_id.clone(),
                    source_event_seq: segment.to_event_seq,
                    event: fork_marker_event(
                        &segment.runtime_session_id,
                        child_runtime_session_id,
                        segment.to_event_seq,
                        occurred_at_ms,
                        committed_at_ms,
                    ),
                });
            }
            segments.push(segment);
        }

        Ok(AgentRunJournalView {
            run_id: query.run_id,
            agent_id: query.agent_id,
            delivery_runtime_session_id,
            segments,
            events,
        })
    }

    async fn resolve_inherited_segments(
        &self,
        delivery_runtime_session_id: &str,
    ) -> Result<Vec<UnindexedJournalSegment>, WorkflowApplicationError> {
        let mut current_session_id = delivery_runtime_session_id.to_string();
        let mut visited = HashSet::new();
        let mut segments = Vec::new();

        while visited.insert(current_session_id.clone()) {
            let Some(lineage) = self
                .lineage
                .lineage_parent(&current_session_id)
                .await
                .map_err(map_session_io_error)?
                .filter(|lineage| lineage.relation_kind == SessionLineageRelationKind::Fork)
            else {
                break;
            };

            let to_event_seq = lineage.fork_point_event_seq.unwrap_or_default();
            if to_event_seq > 0 {
                segments.push(UnindexedJournalSegment {
                    role: AgentRunJournalSegmentRole::InheritedLineage,
                    runtime_session_id: lineage.parent_session_id.clone(),
                    from_after_event_seq: 0,
                    to_event_seq,
                    child_runtime_session_id: Some(current_session_id.clone()),
                });
            }

            current_session_id = lineage.parent_session_id;
        }

        segments.reverse();
        Ok(segments)
    }

    async fn latest_event_seq(
        &self,
        runtime_session_id: &str,
    ) -> Result<u64, WorkflowApplicationError> {
        let mut after_seq = 0;
        let mut latest = 0;
        loop {
            let page = self
                .session_eventing
                .list_event_page(runtime_session_id, after_seq, JOURNAL_EVENT_PAGE_SIZE)
                .await
                .map_err(map_session_io_error)?;
            for event in page.events {
                latest = latest.max(event.event_seq);
            }
            if !page.has_more {
                break;
            }
            after_seq = page.next_after_seq;
        }
        Ok(latest)
    }

    async fn load_segment_events(
        &self,
        segment: &AgentRunJournalSegment,
    ) -> Result<Vec<PersistedSessionEvent>, WorkflowApplicationError> {
        let mut after_seq = segment.from_after_event_seq;
        let mut events = Vec::new();
        loop {
            let page = self
                .session_eventing
                .list_event_page(
                    &segment.runtime_session_id,
                    after_seq,
                    JOURNAL_EVENT_PAGE_SIZE,
                )
                .await
                .map_err(map_session_io_error)?;
            for event in page.events {
                if event.event_seq <= segment.to_event_seq {
                    events.push(event);
                }
            }
            if !page.has_more || page.next_after_seq >= segment.to_event_seq {
                break;
            }
            after_seq = page.next_after_seq;
        }
        Ok(events)
    }
}

#[async_trait]
trait AgentRunJournalLineagePort: Send + Sync {
    async fn lineage_parent(&self, session_id: &str) -> io::Result<Option<SessionLineageRecord>>;
}

#[async_trait]
impl AgentRunJournalLineagePort for SessionBranchingService {
    async fn lineage_parent(&self, session_id: &str) -> io::Result<Option<SessionLineageRecord>> {
        SessionBranchingService::lineage_parent(self, session_id).await
    }
}

#[derive(Debug, Clone)]
struct UnindexedJournalSegment {
    role: AgentRunJournalSegmentRole,
    runtime_session_id: String,
    from_after_event_seq: u64,
    to_event_seq: u64,
    child_runtime_session_id: Option<String>,
}

fn map_session_io_error(error: std::io::Error) -> WorkflowApplicationError {
    WorkflowApplicationError::Internal(format!("AgentRun journal 读取会话事件失败: {error}"))
}

pub fn agent_run_journal_session_id(run_id: Uuid, agent_id: Uuid) -> String {
    format!("agentrun:{run_id}:{agent_id}")
}

pub fn project_event_to_agent_run_journal(
    mut event: PersistedSessionEvent,
    journal_seq: u64,
    journal_session_id: &str,
) -> PersistedSessionEvent {
    event.session_id = journal_session_id.to_string();
    event.event_seq = journal_seq;
    event.notification.session_id = journal_session_id.to_string();
    event
}

fn fork_marker_event(
    parent_session_id: &str,
    child_session_id: &str,
    fork_point_event_seq: u64,
    occurred_at_ms: i64,
    committed_at_ms: i64,
) -> PersistedSessionEvent {
    let notification = BackboneEnvelope::new(
        BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
            key: "session_branch_forked".to_string(),
            value: serde_json::json!({
                "parent_session_id": parent_session_id,
                "child_session_id": child_session_id,
                "fork_point_event_seq": fork_point_event_seq,
                "relation_kind": SessionLineageRelationKind::Fork.as_str(),
            }),
        }),
        child_session_id,
        SourceInfo {
            connector_id: "agent_run_journal".to_string(),
            connector_type: "platform".to_string(),
            executor_id: None,
        },
    )
    .with_trace(TraceInfo {
        turn_id: Some(format!("session-fork:{child_session_id}")),
        entry_index: None,
    });

    PersistedSessionEvent {
        session_id: child_session_id.to_string(),
        event_seq: 0,
        occurred_at_ms,
        committed_at_ms,
        session_update_type: "platform".to_string(),
        turn_id: Some(format!("session-fork:{child_session_id}")),
        entry_index: None,
        tool_call_id: None,
        ephemeral: false,
        notification,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use agentdash_spi::session_persistence::{SessionEventPage, SessionLineageStatus};

    use super::*;
    use crate::agent_run::runtime_session_boundary::RuntimeSessionEventingPort;

    const RUN_ID: Uuid = Uuid::from_u128(0x11111111111111111111111111111111);
    const AGENT_ID: Uuid = Uuid::from_u128(0x22222222222222222222222222222222);

    #[tokio::test]
    async fn visible_journal_orders_parent_slice_marker_and_child_delivery() {
        let service = test_service();

        let page = service
            .load_visible_journal_page(
                AgentRunJournalQuery {
                    run_id: RUN_ID,
                    agent_id: AGENT_ID,
                    delivery_runtime_session_id: Some("child".to_string()),
                },
                0,
                100,
            )
            .await
            .expect("journal page 应可读取");

        assert_eq!(page.snapshot_seq, 5);
        assert!(!page.has_more);
        assert_eq!(page.next_after_seq, 5);
        assert_eq!(
            journal_sources(&page.events),
            vec!["parent:1", "parent:2", "parent:2", "child:1", "child:2"]
        );
        assert_eq!(journal_sequences(&page.events), vec![1, 2, 3, 4, 5]);

        let marker = &page.events[2].event.notification.event;
        match marker {
            BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate { key, value }) => {
                assert_eq!(key, "session_branch_forked");
                assert_eq!(value["parent_session_id"], "parent");
                assert_eq!(value["child_session_id"], "child");
                assert_eq!(value["fork_point_event_seq"], 2);
            }
            other => panic!("fork marker 应是 session_branch_forked platform event: {other:?}"),
        }
    }

    #[tokio::test]
    async fn visible_journal_page_resume_uses_journal_sequence_not_runtime_sequence() {
        let service = test_service();

        let page = service
            .load_visible_journal_page(
                AgentRunJournalQuery {
                    run_id: RUN_ID,
                    agent_id: AGENT_ID,
                    delivery_runtime_session_id: Some("child".to_string()),
                },
                3,
                10,
            )
            .await
            .expect("journal page 应可读取");

        assert_eq!(journal_sources(&page.events), vec!["child:1", "child:2"]);
        assert_eq!(journal_sequences(&page.events), vec![4, 5]);
    }

    #[tokio::test]
    async fn inherited_prefix_excludes_child_delivery_events() {
        let service = test_service();

        let journal = service
            .load_inherited_prefix(AgentRunJournalQuery {
                run_id: RUN_ID,
                agent_id: AGENT_ID,
                delivery_runtime_session_id: Some("child".to_string()),
            })
            .await
            .expect("inherited prefix 应可读取");

        assert_eq!(
            journal_sources(&journal.events),
            vec!["parent:1", "parent:2", "parent:2"]
        );
        assert_eq!(journal_sequences(&journal.events), vec![1, 2, 3]);
    }

    #[test]
    fn projected_journal_event_rewrites_only_agent_run_coordinates() {
        let event = persisted_event("parent", 1, "typed_event");
        let projected = project_event_to_agent_run_journal(
            event,
            7,
            &agent_run_journal_session_id(RUN_ID, AGENT_ID),
        );

        assert_eq!(
            projected.session_id,
            format!("agentrun:{RUN_ID}:{AGENT_ID}")
        );
        assert_eq!(projected.event_seq, 7);
        assert_eq!(
            projected.notification.session_id,
            format!("agentrun:{RUN_ID}:{AGENT_ID}")
        );
        match projected.notification.event {
            BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate { key, value }) => {
                assert_eq!(key, "typed_event");
                assert_eq!(value["label"], "typed_event");
            }
            other => panic!("typed Backbone event 不应被 journal projection 改写: {other:?}"),
        }
    }

    fn test_service() -> AgentRunJournalService {
        let lineage = Arc::new(TestLineagePort {
            parents: HashMap::from([("child".to_string(), fork_lineage("parent", "child", 2))]),
        });
        let eventing = SessionEventingService::new(Arc::new(TestEventingPort {
            events: HashMap::from([
                (
                    "parent".to_string(),
                    vec![
                        persisted_event("parent", 1, "parent_1"),
                        persisted_event("parent", 2, "parent_2"),
                        persisted_event("parent", 3, "parent_after_fork"),
                    ],
                ),
                (
                    "child".to_string(),
                    vec![
                        persisted_event("child", 1, "child_1"),
                        persisted_event("child", 2, "child_2"),
                    ],
                ),
            ]),
        }));
        AgentRunJournalService {
            lineage,
            session_eventing: eventing,
        }
    }

    fn journal_sources(events: &[AgentRunJournalEvent]) -> Vec<String> {
        events
            .iter()
            .map(|event| {
                format!(
                    "{}:{}",
                    event.source_runtime_session_id, event.source_event_seq
                )
            })
            .collect()
    }

    fn journal_sequences(events: &[AgentRunJournalEvent]) -> Vec<u64> {
        events.iter().map(|event| event.journal_seq).collect()
    }

    fn fork_lineage(
        parent_session_id: &str,
        child_session_id: &str,
        fork_point_event_seq: u64,
    ) -> SessionLineageRecord {
        SessionLineageRecord {
            child_session_id: child_session_id.to_string(),
            parent_session_id: parent_session_id.to_string(),
            relation_kind: SessionLineageRelationKind::Fork,
            fork_point_event_seq: Some(fork_point_event_seq),
            fork_point_ref_json: serde_json::json!({}),
            fork_point_compaction_id: None,
            status: SessionLineageStatus::Open,
            created_at_ms: 1,
            updated_at_ms: 1,
            metadata_json: serde_json::json!({}),
        }
    }

    fn persisted_event(session_id: &str, event_seq: u64, label: &str) -> PersistedSessionEvent {
        let turn_id = format!("{session_id}-turn-{event_seq}");
        let notification = BackboneEnvelope::new(
            BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
                key: label.to_string(),
                value: serde_json::json!({ "label": label }),
            }),
            session_id,
            SourceInfo {
                connector_id: "test".to_string(),
                connector_type: "test".to_string(),
                executor_id: None,
            },
        )
        .with_trace(TraceInfo {
            turn_id: Some(turn_id.clone()),
            entry_index: Some(event_seq as u32),
        });

        PersistedSessionEvent {
            session_id: session_id.to_string(),
            event_seq,
            occurred_at_ms: event_seq as i64,
            committed_at_ms: event_seq as i64,
            session_update_type: "platform".to_string(),
            turn_id: Some(turn_id),
            entry_index: Some(event_seq as u32),
            tool_call_id: None,
            ephemeral: false,
            notification,
        }
    }

    struct TestLineagePort {
        parents: HashMap<String, SessionLineageRecord>,
    }

    #[async_trait]
    impl AgentRunJournalLineagePort for TestLineagePort {
        async fn lineage_parent(
            &self,
            session_id: &str,
        ) -> io::Result<Option<SessionLineageRecord>> {
            Ok(self.parents.get(session_id).cloned())
        }
    }

    struct TestEventingPort {
        events: HashMap<String, Vec<PersistedSessionEvent>>,
    }

    #[async_trait]
    impl RuntimeSessionEventingPort for TestEventingPort {
        async fn list_event_page(
            &self,
            session_id: &str,
            after_seq: u64,
            limit: u32,
        ) -> io::Result<SessionEventPage> {
            let all = self.events.get(session_id).cloned().unwrap_or_default();
            let snapshot_seq = all.last().map(|event| event.event_seq).unwrap_or_default();
            let events = all
                .into_iter()
                .filter(|event| event.event_seq > after_seq)
                .take(limit as usize)
                .collect::<Vec<_>>();
            let next_after_seq = events
                .last()
                .map(|event| event.event_seq)
                .unwrap_or(after_seq);
            Ok(SessionEventPage {
                snapshot_seq,
                has_more: next_after_seq < snapshot_seq,
                next_after_seq,
                events,
            })
        }

        async fn persist_notification(
            &self,
            _session_id: &str,
            _envelope: BackboneEnvelope,
        ) -> Result<(), WorkflowApplicationError> {
            Err(WorkflowApplicationError::Internal(
                "test eventing port 不支持写入 notification".to_string(),
            ))
        }

        async fn emit_user_input_submitted(
            &self,
            _session_id: &str,
            _turn_id: &str,
            _event_id: &str,
            _kind: agentdash_agent_protocol::UserInputSubmissionKind,
            _input: Vec<agentdash_agent_protocol::UserInputBlock>,
        ) -> Result<(), WorkflowApplicationError> {
            Err(WorkflowApplicationError::Internal(
                "test eventing port 不支持写入 user input".to_string(),
            ))
        }
    }
}
