use std::{collections::HashSet, io, sync::Arc};

use agentdash_application_runtime_session::session::{
    SessionBranchingService, SessionLineageRecord, SessionLineageRelationKind,
};
use agentdash_spi::session_persistence::{
    PersistedSessionEvent, SessionEventPage, SessionEventStore, SessionLineageStore,
};
use async_trait::async_trait;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::agent_run::runtime_session_boundary::{
    RuntimeSessionEventSubscription, SessionEventingService,
};
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

pub struct AgentRunJournalStreamSubscription {
    pub state: AgentRunJournalStreamState,
    pub live_events: broadcast::Receiver<PersistedSessionEvent>,
}

pub struct AgentRunJournalStreamState {
    pub journal_session_id: String,
    pub delivery_runtime_session_id: String,
    pub resume_from: u64,
    pub connected_seq: u64,
    pub ephemeral_epoch: u64,
    pub prefix_events: Vec<AgentRunJournalEvent>,
    pub backlog_events: Vec<AgentRunJournalEvent>,
    pub ephemeral_backlog_events: Vec<AgentRunJournalEvent>,
    prefix_len: u64,
    snapshot_journal_seq: u64,
    current_segment_index: usize,
}

impl AgentRunJournalStreamState {
    pub fn project_live_event(&self, event: PersistedSessionEvent) -> AgentRunJournalLiveEvent {
        let projected = self.project_current_delivery_event(event);
        if projected.event.ephemeral {
            return AgentRunJournalLiveEvent::Ephemeral(projected);
        }
        if projected.journal_seq <= self.snapshot_journal_seq {
            return AgentRunJournalLiveEvent::StaleDurable;
        }
        AgentRunJournalLiveEvent::Durable(projected)
    }

    fn project_current_delivery_event(&self, event: PersistedSessionEvent) -> AgentRunJournalEvent {
        let journal_seq = self.prefix_len + event.event_seq;
        AgentRunJournalEvent {
            journal_seq,
            segment_index: self.current_segment_index,
            segment_role: AgentRunJournalSegmentRole::CurrentDelivery,
            source_runtime_session_id: self.delivery_runtime_session_id.clone(),
            source_event_seq: event.event_seq,
            event,
        }
    }
}

pub enum AgentRunJournalLiveEvent {
    Durable(AgentRunJournalEvent),
    Ephemeral(AgentRunJournalEvent),
    StaleDurable,
}

#[derive(Debug, Clone)]
pub struct AgentRunJournalQuery {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub delivery_runtime_session_id: Option<String>,
}

pub struct AgentRunJournalService {
    lineage: Arc<dyn AgentRunJournalLineagePort>,
    event_reader: Arc<dyn AgentRunJournalEventPageReader>,
    live_source: Option<Arc<dyn AgentRunJournalLiveEventSource>>,
}

impl AgentRunJournalService {
    pub fn new(
        session_branching: SessionBranchingService,
        session_eventing: SessionEventingService,
    ) -> Self {
        Self {
            lineage: Arc::new(session_branching),
            event_reader: Arc::new(SessionEventingJournalEventPageReader {
                session_eventing: session_eventing.clone(),
            }),
            live_source: Some(Arc::new(SessionEventingJournalLiveEventSource {
                session_eventing,
            })),
        }
    }

    pub fn new_from_session_stores(
        lineage_store: Arc<dyn SessionLineageStore>,
        event_store: Arc<dyn SessionEventStore>,
    ) -> Self {
        Self {
            lineage: Arc::new(SessionLineageStoreJournalLineagePort { lineage_store }),
            event_reader: Arc::new(SessionEventStoreJournalEventPageReader { event_store }),
            live_source: None,
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

    pub async fn subscribe_visible_journal_stream(
        &self,
        query: AgentRunJournalQuery,
        resume_from: u64,
    ) -> Result<AgentRunJournalStreamSubscription, WorkflowApplicationError> {
        let delivery_runtime_session_id =
            query.delivery_runtime_session_id.clone().ok_or_else(|| {
                WorkflowApplicationError::NotFound(
                    "AgentRun 当前没有可读取的 delivery RuntimeSession".to_string(),
                )
            })?;
        let live_source = self.live_source.as_ref().ok_or_else(|| {
            WorkflowApplicationError::Internal(
                "AgentRun journal live stream event source is not configured".to_string(),
            )
        })?;
        let prefix = self.load_inherited_prefix(query).await?;
        let prefix_len = prefix
            .events
            .last()
            .map(|event| event.journal_seq)
            .unwrap_or_default();
        let runtime_resume_from = resume_from.saturating_sub(prefix_len);
        let subscription = live_source
            .subscribe_after(&delivery_runtime_session_id, runtime_resume_from)
            .await
            .map_err(map_session_io_error)?;
        let ephemeral_epoch = live_source.ephemeral_epoch();
        let stream = build_stream_subscription(
            prefix,
            delivery_runtime_session_id,
            resume_from,
            prefix_len,
            subscription,
            ephemeral_epoch,
        );
        Ok(stream)
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
            for event in segment_events {
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
                .event_reader
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
                .event_reader
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
trait AgentRunJournalEventPageReader: Send + Sync {
    async fn list_event_page(
        &self,
        session_id: &str,
        after_seq: u64,
        limit: u32,
    ) -> io::Result<SessionEventPage>;
}

#[async_trait]
trait AgentRunJournalLiveEventSource: Send + Sync {
    async fn subscribe_after(
        &self,
        session_id: &str,
        after_seq: u64,
    ) -> io::Result<RuntimeSessionEventSubscription>;

    fn ephemeral_epoch(&self) -> u64;
}

#[async_trait]
impl AgentRunJournalLineagePort for SessionBranchingService {
    async fn lineage_parent(&self, session_id: &str) -> io::Result<Option<SessionLineageRecord>> {
        SessionBranchingService::lineage_parent(self, session_id).await
    }
}

struct SessionEventingJournalEventPageReader {
    session_eventing: SessionEventingService,
}

#[async_trait]
impl AgentRunJournalEventPageReader for SessionEventingJournalEventPageReader {
    async fn list_event_page(
        &self,
        session_id: &str,
        after_seq: u64,
        limit: u32,
    ) -> io::Result<SessionEventPage> {
        self.session_eventing
            .list_event_page(session_id, after_seq, limit)
            .await
    }
}

struct SessionEventingJournalLiveEventSource {
    session_eventing: SessionEventingService,
}

#[async_trait]
impl AgentRunJournalLiveEventSource for SessionEventingJournalLiveEventSource {
    async fn subscribe_after(
        &self,
        session_id: &str,
        after_seq: u64,
    ) -> io::Result<RuntimeSessionEventSubscription> {
        self.session_eventing
            .subscribe_after(session_id, after_seq)
            .await
    }

    fn ephemeral_epoch(&self) -> u64 {
        self.session_eventing.ephemeral_epoch()
    }
}

struct SessionEventStoreJournalEventPageReader {
    event_store: Arc<dyn SessionEventStore>,
}

#[async_trait]
impl AgentRunJournalEventPageReader for SessionEventStoreJournalEventPageReader {
    async fn list_event_page(
        &self,
        session_id: &str,
        after_seq: u64,
        limit: u32,
    ) -> io::Result<SessionEventPage> {
        self.event_store
            .list_event_page(session_id, after_seq, limit)
            .await
            .map_err(Into::into)
    }
}

struct SessionLineageStoreJournalLineagePort {
    lineage_store: Arc<dyn SessionLineageStore>,
}

#[async_trait]
impl AgentRunJournalLineagePort for SessionLineageStoreJournalLineagePort {
    async fn lineage_parent(&self, session_id: &str) -> io::Result<Option<SessionLineageRecord>> {
        self.lineage_store
            .get_session_lineage(session_id)
            .await
            .map_err(Into::into)
    }
}

#[derive(Debug, Clone)]
struct UnindexedJournalSegment {
    role: AgentRunJournalSegmentRole,
    runtime_session_id: String,
    from_after_event_seq: u64,
    to_event_seq: u64,
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

fn build_stream_subscription(
    prefix: AgentRunJournalView,
    delivery_runtime_session_id: String,
    resume_from: u64,
    prefix_len: u64,
    subscription: RuntimeSessionEventSubscription,
    ephemeral_epoch: u64,
) -> AgentRunJournalStreamSubscription {
    let journal_session_id = agent_run_journal_session_id(prefix.run_id, prefix.agent_id);
    let current_segment_index = prefix.segments.len();
    let prefix_events = prefix
        .events
        .into_iter()
        .filter(|event| event.journal_seq > resume_from)
        .collect::<Vec<_>>();
    let backlog_events = subscription
        .backlog
        .into_iter()
        .map(|event| {
            project_current_delivery_stream_event(
                &delivery_runtime_session_id,
                prefix_len,
                current_segment_index,
                event,
            )
        })
        .collect::<Vec<_>>();
    let ephemeral_backlog_events = subscription
        .ephemeral_backlog
        .into_iter()
        .map(|event| {
            project_current_delivery_stream_event(
                &delivery_runtime_session_id,
                prefix_len,
                current_segment_index,
                event,
            )
        })
        .collect::<Vec<_>>();
    let last_prefix_seq = prefix_events
        .last()
        .map(|event| event.journal_seq)
        .unwrap_or(resume_from);
    let last_backlog_seq = backlog_events
        .last()
        .map(|event| event.journal_seq)
        .unwrap_or(last_prefix_seq);
    let snapshot_journal_seq = prefix_len + subscription.snapshot_seq;
    AgentRunJournalStreamSubscription {
        state: AgentRunJournalStreamState {
            journal_session_id,
            delivery_runtime_session_id,
            resume_from,
            connected_seq: last_backlog_seq.max(snapshot_journal_seq),
            ephemeral_epoch,
            prefix_events,
            backlog_events,
            ephemeral_backlog_events,
            prefix_len,
            snapshot_journal_seq,
            current_segment_index,
        },
        live_events: subscription.rx,
    }
}

fn project_current_delivery_stream_event(
    delivery_runtime_session_id: &str,
    prefix_len: u64,
    current_segment_index: usize,
    event: PersistedSessionEvent,
) -> AgentRunJournalEvent {
    let journal_seq = prefix_len + event.event_seq;
    AgentRunJournalEvent {
        journal_seq,
        segment_index: current_segment_index,
        segment_role: AgentRunJournalSegmentRole::CurrentDelivery,
        source_runtime_session_id: delivery_runtime_session_id.to_string(),
        source_event_seq: event.event_seq,
        event,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use agentdash_agent_protocol::{
        BackboneEnvelope, BackboneEvent, PlatformEvent, SourceInfo, TraceInfo,
    };
    use agentdash_spi::session_persistence::{SessionEventPage, SessionLineageStatus};
    use tokio::sync::broadcast;

    use super::*;
    use crate::agent_run::runtime_session_boundary::RuntimeSessionEventingPort;

    const RUN_ID: Uuid = Uuid::from_u128(0x11111111111111111111111111111111);
    const AGENT_ID: Uuid = Uuid::from_u128(0x22222222222222222222222222222222);

    #[tokio::test]
    async fn visible_journal_uses_child_fork_event_once() {
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

        assert_eq!(page.snapshot_seq, 4);
        assert!(!page.has_more);
        assert_eq!(page.next_after_seq, 4);
        assert_eq!(
            journal_sources(&page.events),
            vec!["parent:1", "parent:2", "child:1", "child:2"]
        );
        assert_eq!(journal_sequences(&page.events), vec![1, 2, 3, 4]);

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
        let fork_event_count = page
            .events
            .iter()
            .filter(|event| is_fork_event(&event.event))
            .count();
        assert_eq!(fork_event_count, 1);
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
                2,
                10,
            )
            .await
            .expect("journal page 应可读取");

        assert_eq!(journal_sources(&page.events), vec!["child:1", "child:2"]);
        assert_eq!(journal_sequences(&page.events), vec![3, 4]);
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
            vec!["parent:1", "parent:2"]
        );
        assert_eq!(journal_sequences(&journal.events), vec![1, 2]);
    }

    #[tokio::test]
    async fn visible_journal_stream_uses_agent_run_sequence_mapping() {
        let service = test_service();

        let subscription = service
            .subscribe_visible_journal_stream(
                AgentRunJournalQuery {
                    run_id: RUN_ID,
                    agent_id: AGENT_ID,
                    delivery_runtime_session_id: Some("child".to_string()),
                },
                2,
            )
            .await
            .expect("journal stream subscription 应可创建");

        assert!(subscription.state.prefix_events.is_empty());
        assert_eq!(
            journal_sources(&subscription.state.backlog_events),
            vec!["child:1", "child:2"]
        );
        assert_eq!(
            journal_sequences(&subscription.state.backlog_events),
            vec![3, 4]
        );
        assert_eq!(subscription.state.connected_seq, 4);
        assert_eq!(subscription.state.ephemeral_epoch, 123);
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
                        fork_event("child", 1, "parent", 2),
                        persisted_event("child", 2, "child_2"),
                    ],
                ),
            ]),
        }));
        AgentRunJournalService {
            lineage,
            event_reader: Arc::new(SessionEventingJournalEventPageReader {
                session_eventing: eventing.clone(),
            }),
            live_source: Some(Arc::new(SessionEventingJournalLiveEventSource {
                session_eventing: eventing,
            })),
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

    fn fork_event(
        child_session_id: &str,
        event_seq: u64,
        parent_session_id: &str,
        fork_point_event_seq: u64,
    ) -> PersistedSessionEvent {
        let mut event = persisted_event(child_session_id, event_seq, "session_branch_forked");
        event.notification.event = BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
            key: "session_branch_forked".to_string(),
            value: serde_json::json!({
                "child_session_id": child_session_id,
                "parent_session_id": parent_session_id,
                "fork_point_event_seq": fork_point_event_seq,
                "relation_kind": "fork",
            }),
        });
        event
    }

    fn is_fork_event(event: &PersistedSessionEvent) -> bool {
        matches!(
            &event.notification.event,
            BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate { key, .. })
                if key == "session_branch_forked"
        )
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

        async fn subscribe_after(
            &self,
            session_id: &str,
            after_seq: u64,
        ) -> io::Result<RuntimeSessionEventSubscription> {
            let page = self.list_event_page(session_id, after_seq, 100).await?;
            let (_tx, rx) = broadcast::channel(16);
            Ok(RuntimeSessionEventSubscription {
                snapshot_seq: page.snapshot_seq,
                backlog: page.events,
                ephemeral_backlog: Vec::new(),
                rx,
            })
        }

        fn ephemeral_epoch(&self) -> u64 {
            123
        }
    }
}
