use std::sync::Arc;

use agentdash_application_runtime_session::session::{
    SessionLineageRecord, SessionLineageRelationKind, SessionLineageStatus,
};
use agentdash_contracts::session::SessionNdjsonEnvelope;
use agentdash_spi::session_persistence::{
    PersistedSessionEvent, SessionEventBacklog, SessionEventPage, SessionEventStore,
    SessionLineageStore, SessionStoreResult,
};
use async_trait::async_trait;
use serde_json::Value;
use uuid::Uuid;

mod error {
    pub use agentdash_application_agentrun::WorkflowApplicationError;
}

mod agent_run {
    pub mod runtime_session_boundary {
        pub use agentdash_application_agentrun::agent_run::{
            RuntimeSessionEventSubscription, SessionEventingService,
        };
    }

    pub mod journal {
        include!("D:/Projects/AgentDash-main-reference/crates/agentdash-application-agentrun/src/agent_run/journal.rs");

        struct FixtureLineage;

        #[async_trait]
        impl AgentRunJournalLineagePort for FixtureLineage {
            async fn lineage_parent(
                &self,
                _: &str,
            ) -> io::Result<Option<SessionLineageRecord>> {
                Ok(None)
            }
        }

        struct FixtureEvents {
            events: Vec<PersistedSessionEvent>,
            live_tx: broadcast::Sender<PersistedSessionEvent>,
        }

        #[async_trait]
        impl AgentRunJournalEventPageReader for FixtureEvents {
            async fn list_event_page(
                &self,
                session_id: &str,
                after_seq: u64,
                limit: u32,
            ) -> io::Result<SessionEventPage> {
                let matching = self
                    .events
                    .iter()
                    .filter(|event| event.session_id == session_id)
                    .cloned()
                    .collect::<Vec<_>>();
                let snapshot_seq = matching
                    .last()
                    .map(|event| event.event_seq)
                    .unwrap_or_default();
                let events = matching
                    .into_iter()
                    .filter(|event| event.event_seq > after_seq)
                    .take(limit.max(1) as usize)
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
        }

        #[async_trait]
        impl AgentRunJournalLiveEventSource for FixtureEvents {
            async fn subscribe_after(
                &self,
                session_id: &str,
                after_seq: u64,
            ) -> io::Result<RuntimeSessionEventSubscription> {
                let backlog = self
                    .events
                    .iter()
                    .filter(|event| {
                        event.session_id == session_id
                            && !event.ephemeral
                            && event.event_seq > after_seq
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                let snapshot_seq = self
                    .events
                    .iter()
                    .filter(|event| event.session_id == session_id && !event.ephemeral)
                    .map(|event| event.event_seq)
                    .max()
                    .unwrap_or_default();
                Ok(RuntimeSessionEventSubscription {
                    snapshot_seq,
                    backlog,
                    ephemeral_backlog: Vec::new(),
                    rx: self.live_tx.subscribe(),
                })
            }

            fn ephemeral_epoch(&self) -> u64 {
                123
            }
        }

        pub fn fixture_service(events: Vec<PersistedSessionEvent>) -> AgentRunJournalService {
            let (live_tx, _) = broadcast::channel(8);
            let events = Arc::new(FixtureEvents { events, live_tx });
            AgentRunJournalService {
                lineage: Arc::new(FixtureLineage),
                event_reader: events.clone(),
                live_source: Some(events),
            }
        }
    }
}

use agent_run::journal::{
    AgentRunJournalLiveEvent, AgentRunJournalPage, AgentRunJournalQuery, fixture_service,
};

const RUN_ID: Uuid = Uuid::from_u128(0x11111111111111111111111111111111);
const AGENT_ID: Uuid = Uuid::from_u128(0x22222222222222222222222222222222);
const DELIVERY_SESSION: &str =
    "agentrun:11111111-1111-1111-1111-111111111111:22222222-2222-2222-2222-222222222222";

struct FixtureEventStore {
    events: Vec<PersistedSessionEvent>,
}

#[async_trait]
impl SessionEventStore for FixtureEventStore {
    async fn append_event(
        &self,
        _: &str,
        _: &agentdash_agent_protocol::BackboneEnvelope,
    ) -> SessionStoreResult<PersistedSessionEvent> {
        unreachable!("read-only pinned Main capture")
    }

    async fn read_backlog(&self, session_id: &str, after_seq: u64) -> SessionStoreResult<SessionEventBacklog> {
        let events = self.list_all_events(session_id).await?;
        let snapshot_seq = events.last().map(|event| event.event_seq).unwrap_or_default();
        Ok(SessionEventBacklog {
            snapshot_seq,
            events: events.into_iter().filter(|event| event.event_seq > after_seq).collect(),
        })
    }

    async fn list_event_page(
        &self,
        session_id: &str,
        after_seq: u64,
        limit: u32,
    ) -> SessionStoreResult<SessionEventPage> {
        let all = self.list_all_events(session_id).await?;
        let snapshot_seq = all.last().map(|event| event.event_seq).unwrap_or_default();
        let events = all
            .into_iter()
            .filter(|event| event.event_seq > after_seq)
            .take(limit.max(1) as usize)
            .collect::<Vec<_>>();
        let next_after_seq = events.last().map(|event| event.event_seq).unwrap_or(after_seq);
        Ok(SessionEventPage {
            snapshot_seq,
            has_more: next_after_seq < snapshot_seq,
            next_after_seq,
            events,
        })
    }

    async fn list_all_events(&self, session_id: &str) -> SessionStoreResult<Vec<PersistedSessionEvent>> {
        Ok((session_id == DELIVERY_SESSION)
            .then(|| self.events.clone())
            .unwrap_or_default())
    }

    async fn list_events_from(&self, session_id: &str, from_seq: u64) -> SessionStoreResult<Vec<PersistedSessionEvent>> {
        Ok(self
            .list_all_events(session_id)
            .await?
            .into_iter()
            .filter(|event| from_seq == 0 || event.event_seq >= from_seq)
            .collect())
    }
}

struct NoLineage;

#[async_trait]
impl SessionLineageStore for NoLineage {
    async fn upsert_session_lineage(&self, _: SessionLineageRecord) -> SessionStoreResult<()> {
        unreachable!("read-only pinned Main capture")
    }

    async fn get_session_lineage(&self, _: &str) -> SessionStoreResult<Option<SessionLineageRecord>> {
        Ok(None)
    }

    async fn list_session_children(
        &self,
        _: &str,
        _: Option<SessionLineageRelationKind>,
        _: Option<SessionLineageStatus>,
    ) -> SessionStoreResult<Vec<SessionLineageRecord>> {
        Ok(Vec::new())
    }

    async fn list_session_ancestors(&self, _: &str) -> SessionStoreResult<Vec<SessionLineageRecord>> {
        Ok(Vec::new())
    }

    async fn list_session_descendants(
        &self,
        _: &str,
        _: Option<SessionLineageRelationKind>,
        _: Option<SessionLineageStatus>,
    ) -> SessionStoreResult<Vec<SessionLineageRecord>> {
        Ok(Vec::new())
    }

    async fn set_session_lineage_status(
        &self,
        _: &str,
        _: SessionLineageStatus,
        _: i64,
    ) -> SessionStoreResult<()> {
        unreachable!("read-only pinned Main capture")
    }
}

fn source_events(fixture: &Value) -> Vec<PersistedSessionEvent> {
    fixture["frames"]
        .as_array()
        .expect("journal frames")
        .iter()
        .map(|frame| PersistedSessionEvent {
            session_id: DELIVERY_SESSION.to_string(),
            event_seq: frame["event_seq"].as_u64().expect("event seq"),
            occurred_at_ms: frame["occurred_at_ms"].as_i64().expect("occurred at"),
            committed_at_ms: frame["committed_at_ms"].as_i64().expect("committed at"),
            session_update_type: frame["session_update_type"].as_str().expect("update type").to_string(),
            turn_id: frame["turn_id"].as_str().map(str::to_string),
            entry_index: frame["entry_index"].as_u64().map(|value| value as u32),
            tool_call_id: None,
            ephemeral: false,
            notification: serde_json::from_value(frame["notification"].clone()).expect("notification"),
        })
        .collect()
}

fn assert_page(page: &AgentRunJournalPage, expected: &Value, snapshot_seq: u64) {
    assert_eq!(page.snapshot_seq, snapshot_seq);
    assert_eq!(page.has_more, expected["has_more"].as_bool().unwrap());
    assert_eq!(page.next_after_seq, expected["next_after_seq"].as_u64().unwrap());
    assert_eq!(
        page.events.iter().map(|event| event.journal_seq).collect::<Vec<_>>(),
        expected["event_seq"].as_array().unwrap().iter().map(|value| value.as_u64().unwrap()).collect::<Vec<_>>()
    );
}

#[tokio::main]
async fn main() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../crates/agentdash-agent-runtime-test-support/fixtures/session-parity/main/journal-replay.json"
    ))
    .expect("journal replay fixture");
    let source_events = source_events(&fixture);
    let service = fixture_service(source_events.clone());
    let query = AgentRunJournalQuery {
        run_id: RUN_ID,
        agent_id: AGENT_ID,
        delivery_runtime_session_id: Some(DELIVERY_SESSION.to_string()),
    };

    let full = service.load_visible_journal_page(query.clone(), 0, 100).await.expect("Main GET");
    let snapshot_seq = fixture["expected"]["get"]["snapshot_seq"]
        .as_u64()
        .expect("snapshot seq");
    assert_page(&full, &fixture["expected"]["get"], snapshot_seq);
    for expected in fixture["expected"]["paging"].as_array().expect("paging") {
        let page = service
            .load_visible_journal_page(
                query.clone(),
                expected["after_seq"].as_u64().unwrap(),
                expected["limit"].as_u64().unwrap() as u32,
            )
            .await
            .expect("Main paging");
        assert_page(&page, expected, snapshot_seq);
    }

    let initial = service
        .subscribe_visible_journal_stream(query.clone(), 0)
        .await
        .expect("Main initial stream");
    let mut initial_labels = initial
        .state
        .backlog_events
        .iter()
        .map(|event| format!("event:{}", event.journal_seq))
        .collect::<Vec<_>>();
    initial_labels.push(format!("connected:{}", initial.state.connected_seq));
    assert_eq!(
        initial_labels,
        fixture["expected"]["initial_stream"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap().to_string())
            .collect::<Vec<_>>()
    );

    let mut live_source = source_events.last().expect("live source").clone();
    live_source.event_seq = 3;
    let AgentRunJournalLiveEvent::Durable(live) = initial.state.project_live_event(live_source)
    else {
        panic!("expected Main durable live event");
    };
    assert_eq!(
        vec![format!("event:{}", live.journal_seq)],
        fixture["expected"]["live_stream"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap().to_string())
            .collect::<Vec<_>>()
    );

    let reconnect = service
        .subscribe_visible_journal_stream(query.clone(), 1)
        .await
        .expect("Main reconnect stream");
    let mut reconnect_labels = reconnect
        .state
        .backlog_events
        .iter()
        .map(|event| format!("event:{}", event.journal_seq))
        .collect::<Vec<_>>();
    reconnect_labels.push(format!("connected:{}", reconnect.state.connected_seq));
    assert_eq!(
        reconnect_labels,
        fixture["expected"]["reconnect_from_1"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap().to_string())
            .collect::<Vec<_>>()
    );

    let refresh = service.load_visible_journal_page(query, 0, 100).await.expect("Main refresh");
    assert_eq!(
        refresh.events.iter().map(|event| event.journal_seq).collect::<Vec<_>>(),
        fixture["expected"]["refresh_event_seq"].as_array().unwrap().iter().map(|value| value.as_u64().unwrap()).collect::<Vec<_>>()
    );

    let frames = fixture["frames"].as_array().unwrap();
    for (projected, expected) in full.events.iter().zip(frames) {
        let event = &projected.event;
        assert_eq!(event.session_id, expected["session_id"]);
        assert_eq!(event.event_seq, expected["event_seq"]);
        assert_eq!(serde_json::to_value(&event.notification).unwrap(), expected["notification"]);
    }

    let controls: Value = serde_json::from_str(include_str!(
        "../../../../crates/agentdash-agent-runtime-test-support/fixtures/session-parity/main/journal-control.json"
    ))
    .expect("journal control fixture");
    assert_eq!(
        serde_json::to_value(SessionNdjsonEnvelope::connected(4, 77))
            .expect("Main connected envelope"),
        controls["controls"]["connected"]
    );
    assert_eq!(
        serde_json::to_value(SessionNdjsonEnvelope::Heartbeat {
            timestamp: 1_783_684_800_015,
        })
        .expect("Main heartbeat envelope"),
        controls["controls"]["heartbeat"]
    );

    let (sender, mut receiver) = tokio::sync::broadcast::channel(1);
    sender.send(1_u8).expect("first live event");
    sender.send(2_u8).expect("second live event");
    assert!(matches!(
        receiver.recv().await,
        Err(tokio::sync::broadcast::error::RecvError::Lagged(1))
    ));
    assert_eq!(controls["controls"]["lagged"]["action"], "continue");
    assert_eq!(receiver.recv().await.expect("latest live event"), 2);
    drop(sender);
    assert!(matches!(
        receiver.recv().await,
        Err(tokio::sync::broadcast::error::RecvError::Closed)
    ));
    assert_eq!(controls["controls"]["closed"]["action"], "break");
}
