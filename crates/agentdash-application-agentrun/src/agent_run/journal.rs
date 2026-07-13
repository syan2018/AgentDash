use std::{collections::HashSet, sync::Arc};

use agentdash_agent_runtime_contract::{
    EventSequence, PresentationDurability, RuntimeJournalRecord, RuntimeThreadId,
};
use agentdash_application_ports::agent_run_runtime::AgentRunRuntimeTarget;
use agentdash_domain::workflow::AgentRunLineageRepository;
use async_trait::async_trait;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::error::WorkflowApplicationError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRunJournalSegmentRole {
    InheritedLineage,
    CurrentDelivery,
}

#[derive(Debug, Clone)]
pub struct AgentRunJournalEvent {
    pub journal_seq: u64,
    pub segment_role: AgentRunJournalSegmentRole,
    pub source_runtime_thread_id: RuntimeThreadId,
    pub source_event_seq: Option<EventSequence>,
    pub record: RuntimeJournalRecord,
}

#[derive(Debug, Clone)]
pub struct AgentRunJournalPage {
    pub delivery_runtime_thread_id: RuntimeThreadId,
    pub snapshot_seq: u64,
    pub events: Vec<AgentRunJournalEvent>,
    pub has_more: bool,
    pub next_after_seq: u64,
}

#[derive(Debug, Clone)]
pub struct AgentRunJournalQuery {
    pub run_id: Uuid,
    pub agent_id: Uuid,
}

pub struct AgentRunJournalSourceSubscription {
    pub ephemeral_epoch: u64,
    pub durable_snapshot: Vec<RuntimeJournalRecord>,
    pub ephemeral_backlog: Vec<RuntimeJournalRecord>,
    pub live: broadcast::Receiver<RuntimeJournalRecord>,
}

#[async_trait]
pub trait AgentRunJournalSource: Send + Sync {
    async fn durable_records(
        &self,
        thread_id: &RuntimeThreadId,
    ) -> Result<Vec<RuntimeJournalRecord>, WorkflowApplicationError>;

    async fn subscribe(
        &self,
        thread_id: &RuntimeThreadId,
    ) -> Result<AgentRunJournalSourceSubscription, WorkflowApplicationError>;
}

#[async_trait]
pub trait AgentRunJournalBindingResolver: Send + Sync {
    async fn resolve_thread(
        &self,
        target: &AgentRunRuntimeTarget,
    ) -> Result<Option<RuntimeThreadId>, WorkflowApplicationError>;
}

pub struct AgentRunJournalStreamSubscription {
    pub state: AgentRunJournalStreamState,
    pub live: broadcast::Receiver<RuntimeJournalRecord>,
}

pub struct AgentRunJournalStreamState {
    pub journal_session_id: String,
    pub delivery_runtime_thread_id: RuntimeThreadId,
    pub connected_seq: u64,
    pub ephemeral_epoch: u64,
    pub prefix_events: Vec<AgentRunJournalEvent>,
    pub backlog_events: Vec<AgentRunJournalEvent>,
    pub ephemeral_backlog_events: Vec<AgentRunJournalEvent>,
    prefix_len: u64,
    snapshot_runtime_sequence: Option<EventSequence>,
}

impl AgentRunJournalStreamState {
    pub fn project_live_record(
        &self,
        record: RuntimeJournalRecord,
    ) -> Result<AgentRunJournalLiveEvent, WorkflowApplicationError> {
        let Some(presentation) = record.as_presentation() else {
            return Ok(AgentRunJournalLiveEvent::Internal);
        };
        if presentation.durability == PresentationDurability::Ephemeral {
            return Ok(AgentRunJournalLiveEvent::Ephemeral(
                self.project_ephemeral(record)?,
            ));
        }
        let sequence = record.carrier().sequence.ok_or_else(|| {
            WorkflowApplicationError::Internal(
                "durable presentation record is missing its Runtime sequence".to_string(),
            )
        })?;
        if self
            .snapshot_runtime_sequence
            .is_some_and(|snapshot| sequence <= snapshot)
        {
            return Ok(AgentRunJournalLiveEvent::StaleDurable);
        }
        Ok(AgentRunJournalLiveEvent::Durable(AgentRunJournalEvent {
            journal_seq: self.prefix_len + sequence.0,
            segment_role: AgentRunJournalSegmentRole::CurrentDelivery,
            source_runtime_thread_id: self.delivery_runtime_thread_id.clone(),
            source_event_seq: Some(sequence),
            record,
        }))
    }

    fn project_ephemeral(
        &self,
        record: RuntimeJournalRecord,
    ) -> Result<AgentRunJournalEvent, WorkflowApplicationError> {
        let transient = record.carrier().transient.as_ref().ok_or_else(|| {
            WorkflowApplicationError::Internal(
                "ephemeral presentation record is missing its transient sequence".to_string(),
            )
        })?;
        Ok(AgentRunJournalEvent {
            journal_seq: self.prefix_len + transient.sequence.0,
            segment_role: AgentRunJournalSegmentRole::CurrentDelivery,
            source_runtime_thread_id: self.delivery_runtime_thread_id.clone(),
            source_event_seq: None,
            record,
        })
    }
}

pub enum AgentRunJournalLiveEvent {
    Durable(AgentRunJournalEvent),
    Ephemeral(AgentRunJournalEvent),
    StaleDurable,
    Internal,
}

pub struct AgentRunJournalService {
    lineage: Arc<dyn AgentRunLineageRepository>,
    bindings: Arc<dyn AgentRunJournalBindingResolver>,
    source: Arc<dyn AgentRunJournalSource>,
}

impl AgentRunJournalService {
    pub fn new(
        lineage: Arc<dyn AgentRunLineageRepository>,
        bindings: Arc<dyn AgentRunJournalBindingResolver>,
        source: Arc<dyn AgentRunJournalSource>,
    ) -> Self {
        Self {
            lineage,
            bindings,
            source,
        }
    }

    pub async fn load_visible_journal_page(
        &self,
        query: AgentRunJournalQuery,
        after_seq: u64,
        limit: u32,
    ) -> Result<AgentRunJournalPage, WorkflowApplicationError> {
        let delivery_runtime_thread_id = self.binding(&query).await?;
        let events = self
            .load_visible_journal_for_thread(&query, delivery_runtime_thread_id.clone())
            .await?;
        let snapshot_seq = events.last().map_or(0, |event| event.journal_seq);
        let page = events
            .into_iter()
            .filter(|event| event.journal_seq > after_seq)
            .take(limit.max(1) as usize)
            .collect::<Vec<_>>();
        let next_after_seq = page.last().map_or(after_seq, |event| event.journal_seq);
        Ok(AgentRunJournalPage {
            delivery_runtime_thread_id,
            snapshot_seq,
            events: page,
            has_more: next_after_seq < snapshot_seq,
            next_after_seq,
        })
    }

    pub async fn load_visible_journal_page_for_thread(
        &self,
        query: AgentRunJournalQuery,
        delivery_runtime_thread_id: RuntimeThreadId,
        after_seq: u64,
        limit: u32,
    ) -> Result<AgentRunJournalPage, WorkflowApplicationError> {
        let page_delivery_runtime_thread_id = delivery_runtime_thread_id.clone();
        let events = self
            .load_visible_journal_for_thread(&query, delivery_runtime_thread_id)
            .await?;
        let snapshot_seq = events.last().map_or(0, |event| event.journal_seq);
        let page = events
            .into_iter()
            .filter(|event| event.journal_seq > after_seq)
            .take(limit.max(1) as usize)
            .collect::<Vec<_>>();
        let next_after_seq = page.last().map_or(after_seq, |event| event.journal_seq);
        Ok(AgentRunJournalPage {
            delivery_runtime_thread_id: page_delivery_runtime_thread_id,
            has_more: next_after_seq < snapshot_seq,
            snapshot_seq,
            events: page,
            next_after_seq,
        })
    }

    pub async fn subscribe_visible_journal_stream(
        &self,
        query: AgentRunJournalQuery,
        resume_from: u64,
    ) -> Result<AgentRunJournalStreamSubscription, WorkflowApplicationError> {
        let prefix = self.load_inherited_prefix(&query).await?;
        let prefix_len = prefix.len() as u64;
        let binding = self.binding(&query).await?;
        let source = self.source.subscribe(&binding).await?;
        let durable = presentation_records(source.durable_snapshot);
        let snapshot_runtime_sequence = durable.last().and_then(|record| record.carrier().sequence);
        let current = durable
            .into_iter()
            .enumerate()
            .map(|(index, record)| AgentRunJournalEvent {
                journal_seq: prefix_len + index as u64 + 1,
                segment_role: AgentRunJournalSegmentRole::CurrentDelivery,
                source_runtime_thread_id: binding.clone(),
                source_event_seq: record.carrier().sequence,
                record,
            })
            .collect::<Vec<_>>();
        let visible_snapshot_seq = prefix_len + current.len() as u64;
        let connected_seq = visible_snapshot_seq.max(resume_from);
        let prefix_events = prefix
            .into_iter()
            .filter(|event| event.journal_seq > resume_from)
            .collect();
        let backlog_events = current
            .into_iter()
            .filter(|event| event.journal_seq > resume_from)
            .collect();
        let ephemeral_backlog_events = presentation_records(source.ephemeral_backlog)
            .into_iter()
            .map(|record| {
                let transient = record.carrier().transient.as_ref().ok_or_else(|| {
                    WorkflowApplicationError::Internal(
                        "ephemeral backlog presentation record is missing its transient sequence"
                            .to_string(),
                    )
                })?;
                Ok(AgentRunJournalEvent {
                    journal_seq: prefix_len + transient.sequence.0,
                    segment_role: AgentRunJournalSegmentRole::CurrentDelivery,
                    source_runtime_thread_id: binding.clone(),
                    source_event_seq: None,
                    record,
                })
            })
            .collect::<Result<Vec<_>, WorkflowApplicationError>>()?;
        Ok(AgentRunJournalStreamSubscription {
            state: AgentRunJournalStreamState {
                journal_session_id: agent_run_journal_session_id(query.run_id, query.agent_id),
                delivery_runtime_thread_id: binding,
                connected_seq,
                ephemeral_epoch: source.ephemeral_epoch,
                prefix_events,
                backlog_events,
                ephemeral_backlog_events,
                prefix_len,
                snapshot_runtime_sequence,
            },
            live: source.live,
        })
    }

    async fn load_visible_journal_for_thread(
        &self,
        query: &AgentRunJournalQuery,
        binding: RuntimeThreadId,
    ) -> Result<Vec<AgentRunJournalEvent>, WorkflowApplicationError> {
        let mut events = self.load_inherited_prefix(query).await?;
        let current = presentation_records(self.source.durable_records(&binding).await?);
        let prefix_len = events.len() as u64;
        events.extend(current.into_iter().enumerate().map(|(index, record)| {
            AgentRunJournalEvent {
                journal_seq: prefix_len + index as u64 + 1,
                segment_role: AgentRunJournalSegmentRole::CurrentDelivery,
                source_runtime_thread_id: binding.clone(),
                source_event_seq: record.carrier().sequence,
                record,
            }
        }));
        Ok(events)
    }

    async fn load_inherited_prefix(
        &self,
        query: &AgentRunJournalQuery,
    ) -> Result<Vec<AgentRunJournalEvent>, WorkflowApplicationError> {
        let mut target = AgentRunRuntimeTarget {
            run_id: query.run_id,
            agent_id: query.agent_id,
        };
        let mut visited = HashSet::new();
        let mut edges = Vec::new();
        loop {
            if !visited.insert((target.run_id, target.agent_id)) {
                return Err(WorkflowApplicationError::Internal(format!(
                    "AgentRun fork lineage 存在循环: {} / {}",
                    target.run_id, target.agent_id
                )));
            }
            let Some(lineage) = self
                .lineage
                .find_parent(target.run_id, target.agent_id)
                .await?
                .filter(|lineage| lineage.relation_kind == "fork")
            else {
                break;
            };
            edges.push((
                AgentRunRuntimeTarget {
                    run_id: lineage.parent_run_id,
                    agent_id: lineage.parent_agent_id,
                },
                lineage.fork_point_event_seq.unwrap_or_default(),
            ));
            target = edges.last().expect("edge was pushed").0.clone();
        }
        edges.reverse();

        let mut events = Vec::new();
        for (target, cutoff) in edges {
            if cutoff == 0 {
                continue;
            }
            let binding = self
                .bindings
                .resolve_thread(&target)
                .await?
                .ok_or_else(|| {
                    WorkflowApplicationError::NotFound(format!(
                        "fork lineage parent AgentRun {} / {} 缺少 Runtime binding",
                        target.run_id, target.agent_id
                    ))
                })?;
            let records = presentation_records(self.source.durable_records(&binding).await?);
            for record in records.into_iter().take(cutoff as usize) {
                events.push(AgentRunJournalEvent {
                    journal_seq: events.len() as u64 + 1,
                    segment_role: AgentRunJournalSegmentRole::InheritedLineage,
                    source_runtime_thread_id: binding.clone(),
                    source_event_seq: record.carrier().sequence,
                    record,
                });
            }
        }
        Ok(events)
    }

    async fn binding(
        &self,
        query: &AgentRunJournalQuery,
    ) -> Result<RuntimeThreadId, WorkflowApplicationError> {
        self.bindings
            .resolve_thread(&AgentRunRuntimeTarget {
                run_id: query.run_id,
                agent_id: query.agent_id,
            })
            .await?
            .ok_or_else(|| {
                WorkflowApplicationError::NotFound(
                    "AgentRun 当前没有可读取的 delivery Runtime thread".to_string(),
                )
            })
    }
}

pub fn agent_run_journal_session_id(run_id: Uuid, agent_id: Uuid) -> String {
    format!("agentrun:{run_id}:{agent_id}")
}

fn presentation_records(records: Vec<RuntimeJournalRecord>) -> Vec<RuntimeJournalRecord> {
    records
        .into_iter()
        .filter(|record| record.as_presentation().is_some())
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use agentdash_agent_runtime_contract::{
        RuntimeBindingId, RuntimeCarrierMetadata, RuntimeDriverGeneration, RuntimeEvent,
        RuntimeJournalFact, RuntimePresentationCoordinate, RuntimeRevision, RuntimeThreadStatus,
        RuntimeTransientCoordinate, RuntimeTransientEventId, RuntimeTransientSequence,
    };

    #[test]
    fn session_projection_reads_only_presentation_facts_without_rewriting_body() {
        let protected = agentdash_agent_protocol::BackboneEvent::Platform(
            agentdash_agent_protocol::PlatformEvent::SessionMetaUpdate {
                key: "fixture".to_string(),
                value: serde_json::json!({ "nullable": null, "ordered": [1, 2] }),
            },
        );
        let presentation = record(
            1,
            RuntimeJournalFact::Presentation(
                agentdash_agent_runtime_contract::ImmutablePresentationEvent::new(
                    PresentationDurability::Durable,
                    protected.clone(),
                ),
            ),
        );
        let internal = record(
            2,
            RuntimeJournalFact::Internal(RuntimeEvent::ThreadStatusChanged {
                status: RuntimeThreadStatus::Active,
            }),
        );
        let projected = presentation_records(vec![presentation, internal]);
        assert_eq!(projected.len(), 1);
        assert_eq!(
            projected[0].as_presentation().expect("presentation").event,
            protected
        );
    }

    #[tokio::test]
    async fn fork_get_and_reconnect_share_one_ordered_projection() {
        let control = journal_control_fixture();
        let parent = AgentRunRuntimeTarget {
            run_id: Uuid::parse_str("11111111-1111-1111-1111-111111111111").expect("run id"),
            agent_id: Uuid::parse_str("22222222-2222-2222-2222-222222222222").expect("agent id"),
        };
        let child = AgentRunRuntimeTarget {
            run_id: Uuid::parse_str("33333333-3333-3333-3333-333333333333").expect("run id"),
            agent_id: Uuid::parse_str("44444444-4444-4444-4444-444444444444").expect("agent id"),
        };
        let parent_thread = RuntimeThreadId::new("parent-thread").expect("thread id");
        let child_thread = RuntimeThreadId::new("child-thread").expect("thread id");
        let lineage = agentdash_domain::workflow::AgentRunLineage::new_fork(
            parent.run_id,
            parent.agent_id,
            child.run_id,
            child.agent_id,
            Some(2),
            Some(serde_json::json!({ "turn_id": "parent-turn", "entry_index": 1 })),
            "fixture",
            None,
        );
        let source = Arc::new(FixtureSource {
            records: HashMap::from([
                (
                    parent_thread.clone(),
                    vec![
                        presentation_record(&parent_thread, 1, "parent-1"),
                        presentation_record(&parent_thread, 2, "parent-2"),
                        presentation_record(&parent_thread, 3, "parent-after-fork"),
                    ],
                ),
                (
                    child_thread.clone(),
                    vec![
                        presentation_record(&child_thread, 1, "session_branch_forked"),
                        presentation_record(&child_thread, 2, "child-2"),
                    ],
                ),
            ]),
            ephemeral: HashMap::new(),
        });
        let service = AgentRunJournalService::new(
            Arc::new(FixtureLineage {
                parents: HashMap::from([(child.clone(), lineage)]),
            }),
            Arc::new(FixtureBindings {
                threads: HashMap::from([
                    (parent.clone(), parent_thread),
                    (child.clone(), child_thread.clone()),
                ]),
            }),
            source,
        );
        let query = AgentRunJournalQuery {
            run_id: child.run_id,
            agent_id: child.agent_id,
        };
        let page = service
            .load_visible_journal_page(query.clone(), 0, 20)
            .await
            .expect("GET journal page");
        assert_eq!(page.delivery_runtime_thread_id, child_thread);
        assert_ne!(
            page.delivery_runtime_thread_id.as_str(),
            agent_run_journal_session_id(child.run_id, child.agent_id)
        );
        assert_eq!(page.snapshot_seq, 4);
        assert_eq!(
            journal_labels(&page.events),
            fixture_strings(&control["single_fork"]["labels"])
        );
        assert_eq!(
            page.events
                .iter()
                .map(|event| event.journal_seq)
                .collect::<Vec<_>>(),
            fixture_u64s(&control["single_fork"]["journal_seq"])
        );

        let reconnect = service
            .subscribe_visible_journal_stream(query, 2)
            .await
            .expect("reconnect journal stream");
        assert!(reconnect.state.prefix_events.is_empty());
        assert_eq!(
            journal_labels(&reconnect.state.backlog_events),
            ["session_branch_forked", "child-2"]
        );
        assert_eq!(reconnect.state.connected_seq, 4);
        assert_eq!(reconnect.state.ephemeral_epoch, 77);
    }

    #[tokio::test]
    async fn terminal_evidence_query_stays_on_explicit_delivery_thread_after_rebind() {
        let delivery = target(0x31, 0x32);
        let old_thread = RuntimeThreadId::new("old-terminal-thread").unwrap();
        let rebound_thread = RuntimeThreadId::new("rebound-thread").unwrap();
        let service = AgentRunJournalService::new(
            Arc::new(FixtureLineage {
                parents: HashMap::new(),
            }),
            Arc::new(FixtureBindings {
                threads: HashMap::from([(delivery.clone(), rebound_thread.clone())]),
            }),
            Arc::new(FixtureSource {
                records: HashMap::from([
                    (
                        old_thread.clone(),
                        vec![presentation_record(&old_thread, 1, "old-terminal-message")],
                    ),
                    (
                        rebound_thread.clone(),
                        vec![presentation_record(
                            &rebound_thread,
                            1,
                            "new-delivery-message",
                        )],
                    ),
                ]),
                ephemeral: HashMap::new(),
            }),
        );

        let page = service
            .load_visible_journal_page_for_thread(
                AgentRunJournalQuery {
                    run_id: delivery.run_id,
                    agent_id: delivery.agent_id,
                },
                old_thread,
                0,
                20,
            )
            .await
            .unwrap();

        assert_eq!(journal_labels(&page.events), ["old-terminal-message"]);
    }

    #[tokio::test]
    async fn get_paging_initial_live_reconnect_and_refresh_match_main_fixture() {
        let replay = journal_replay_fixture();
        let current = target(0x61, 0x62);
        let thread = RuntimeThreadId::new("replay-thread").expect("thread id");
        let replay_records = replay["frames"]
            .as_array()
            .expect("Main replay frames")
            .iter()
            .map(|frame| presentation_record_from_main_frame(&thread, frame))
            .collect::<Vec<_>>();
        let service = AgentRunJournalService::new(
            Arc::new(FixtureLineage {
                parents: HashMap::new(),
            }),
            Arc::new(FixtureBindings {
                threads: HashMap::from([(current.clone(), thread.clone())]),
            }),
            Arc::new(FixtureSource {
                records: HashMap::from([(thread.clone(), replay_records)]),
                ephemeral: HashMap::new(),
            }),
        );
        let query = AgentRunJournalQuery {
            run_id: current.run_id,
            agent_id: current.agent_id,
        };

        let full = service
            .load_visible_journal_page(query.clone(), 0, 20)
            .await
            .expect("GET journal");
        assert_eq!(full.snapshot_seq, replay["expected"]["get"]["snapshot_seq"]);
        assert_eq!(
            full.events
                .iter()
                .map(|event| event.journal_seq)
                .collect::<Vec<_>>(),
            fixture_u64s(&replay["expected"]["get"]["event_seq"])
        );
        assert_eq!(full.has_more, replay["expected"]["get"]["has_more"]);
        assert_eq!(
            full.next_after_seq,
            replay["expected"]["get"]["next_after_seq"]
        );

        for expected in replay["expected"]["paging"]
            .as_array()
            .expect("paging fixture")
        {
            let page = service
                .load_visible_journal_page(
                    query.clone(),
                    expected["after_seq"].as_u64().expect("after_seq"),
                    expected["limit"].as_u64().expect("limit") as u32,
                )
                .await
                .expect("paged GET journal");
            assert_eq!(
                page.events
                    .iter()
                    .map(|event| event.journal_seq)
                    .collect::<Vec<_>>(),
                fixture_u64s(&expected["event_seq"])
            );
            assert_eq!(page.has_more, expected["has_more"]);
            assert_eq!(page.next_after_seq, expected["next_after_seq"]);
        }

        let initial = service
            .subscribe_visible_journal_stream(query.clone(), 0)
            .await
            .expect("initial stream");
        assert_eq!(
            initial
                .state
                .backlog_events
                .iter()
                .map(|event| event.journal_seq)
                .collect::<Vec<_>>(),
            [1, 2]
        );
        assert_eq!(initial.state.connected_seq, 2);
        let live = initial
            .state
            .project_live_record(presentation_record(&thread, 3, "journal-live-3"))
            .expect("live projection");
        let AgentRunJournalLiveEvent::Durable(live) = live else {
            panic!("expected durable live event");
        };
        assert_eq!(live.journal_seq, 3);

        let reconnect = service
            .subscribe_visible_journal_stream(query.clone(), 1)
            .await
            .expect("reconnect stream");
        assert_eq!(
            reconnect
                .state
                .backlog_events
                .iter()
                .map(|event| event.journal_seq)
                .collect::<Vec<_>>(),
            [2]
        );
        assert_eq!(reconnect.state.connected_seq, 2);

        let refresh = service
            .load_visible_journal_page(query, 0, 20)
            .await
            .expect("refresh GET journal");
        assert_eq!(
            refresh
                .events
                .iter()
                .map(|event| &event.record)
                .collect::<Vec<_>>(),
            full.events
                .iter()
                .map(|event| &event.record)
                .collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn multi_level_fork_applies_each_parent_local_cutoff_before_concatenation() {
        let control = journal_control_fixture();
        let grandparent = target(0x11, 0x12);
        let parent = target(0x21, 0x22);
        let child = target(0x31, 0x32);
        let grandparent_thread = RuntimeThreadId::new("grandparent-thread").expect("thread id");
        let parent_thread = RuntimeThreadId::new("parent-thread").expect("thread id");
        let child_thread = RuntimeThreadId::new("child-thread").expect("thread id");
        let service = AgentRunJournalService::new(
            Arc::new(FixtureLineage {
                parents: HashMap::from([
                    (parent.clone(), lineage(&grandparent, &parent, 2)),
                    (child.clone(), lineage(&parent, &child, 2)),
                ]),
            }),
            Arc::new(FixtureBindings {
                threads: HashMap::from([
                    (grandparent.clone(), grandparent_thread.clone()),
                    (parent.clone(), parent_thread.clone()),
                    (child.clone(), child_thread.clone()),
                ]),
            }),
            Arc::new(FixtureSource {
                records: HashMap::from([
                    (
                        grandparent_thread.clone(),
                        vec![
                            presentation_record(&grandparent_thread, 1, "grandparent-1"),
                            presentation_record(&grandparent_thread, 2, "grandparent-2"),
                            presentation_record(&grandparent_thread, 3, "grandparent-after-fork"),
                        ],
                    ),
                    (
                        parent_thread.clone(),
                        vec![
                            presentation_record(&parent_thread, 1, "session_branch_forked"),
                            presentation_record(&parent_thread, 2, "parent-2"),
                            presentation_record(&parent_thread, 3, "parent-after-fork"),
                        ],
                    ),
                    (
                        child_thread.clone(),
                        vec![
                            presentation_record(&child_thread, 1, "session_branch_forked"),
                            presentation_record(&child_thread, 2, "child-2"),
                        ],
                    ),
                ]),
                ephemeral: HashMap::new(),
            }),
        );

        let page = service
            .load_visible_journal_page(
                AgentRunJournalQuery {
                    run_id: child.run_id,
                    agent_id: child.agent_id,
                },
                0,
                20,
            )
            .await
            .expect("multi-level fork journal");
        assert_eq!(
            journal_labels(&page.events),
            fixture_strings(&control["multi_fork"]["labels"])
        );
        assert_eq!(
            page.events
                .iter()
                .map(|event| event.journal_seq)
                .collect::<Vec<_>>(),
            fixture_u64s(&control["multi_fork"]["journal_seq"])
        );
    }

    #[tokio::test]
    async fn future_resume_cursor_is_accepted_and_ephemeral_sequence_uses_transient_coordinate() {
        let control = journal_control_fixture();
        let parent = target(0x41, 0x42);
        let child = target(0x51, 0x52);
        let parent_thread = RuntimeThreadId::new("parent-thread").expect("thread id");
        let child_thread = RuntimeThreadId::new("child-thread").expect("thread id");
        let service = AgentRunJournalService::new(
            Arc::new(FixtureLineage {
                parents: HashMap::from([(child.clone(), lineage(&parent, &child, 2))]),
            }),
            Arc::new(FixtureBindings {
                threads: HashMap::from([
                    (parent.clone(), parent_thread.clone()),
                    (child.clone(), child_thread.clone()),
                ]),
            }),
            Arc::new(FixtureSource {
                records: HashMap::from([
                    (
                        parent_thread.clone(),
                        vec![
                            presentation_record(&parent_thread, 1, "parent-1"),
                            presentation_record(&parent_thread, 2, "parent-2"),
                        ],
                    ),
                    (
                        child_thread.clone(),
                        vec![presentation_record(&child_thread, 1, "child-marker")],
                    ),
                ]),
                ephemeral: HashMap::from([(
                    child_thread.clone(),
                    vec![
                        ephemeral_record(&child_thread, 1, "ephemeral-backlog-1"),
                        ephemeral_record(&child_thread, 3, "ephemeral-backlog-3"),
                    ],
                )]),
            }),
        );
        let query = AgentRunJournalQuery {
            run_id: child.run_id,
            agent_id: child.agent_id,
        };

        let normal = service
            .subscribe_visible_journal_stream(query.clone(), 0)
            .await
            .expect("initial stream");
        assert_eq!(
            normal
                .state
                .ephemeral_backlog_events
                .iter()
                .map(|event| event.journal_seq)
                .collect::<Vec<_>>(),
            fixture_u64s(&control["ephemeral"]["backlog_event_seq"])
        );
        let live = normal
            .state
            .project_live_record(ephemeral_record(&child_thread, 6, "ephemeral-live"))
            .expect("live projection");
        let AgentRunJournalLiveEvent::Ephemeral(live) = live else {
            panic!("expected ephemeral live event");
        };
        assert_eq!(live.journal_seq, control["ephemeral"]["live_event_seq"]);
        let after_clear = normal
            .state
            .project_live_record(ephemeral_record(&child_thread, 9, "ephemeral-after-clear"))
            .expect("post-clear live projection");
        let AgentRunJournalLiveEvent::Ephemeral(after_clear) = after_clear else {
            panic!("expected post-clear ephemeral live event");
        };
        assert_eq!(
            after_clear.journal_seq,
            control["ephemeral"]["after_clear_event_seq"]
        );

        let future = service
            .subscribe_visible_journal_stream(
                query,
                control["resume"]["future_cursor"]
                    .as_u64()
                    .expect("future cursor"),
            )
            .await
            .expect("future cursor remains valid");
        assert_eq!(
            future.state.connected_seq,
            control["resume"]["connected_seq"]
        );
        assert!(future.state.prefix_events.is_empty());
        assert!(future.state.backlog_events.is_empty());
        assert_eq!(future.state.ephemeral_epoch, normal.state.ephemeral_epoch);
    }

    #[tokio::test]
    async fn future_connected_cursor_does_not_renumber_next_durable_live_event() {
        let control = journal_control_fixture();
        let current = target(0x71, 0x72);
        let thread = RuntimeThreadId::new("future-resume-thread").expect("thread id");
        let service = AgentRunJournalService::new(
            Arc::new(FixtureLineage {
                parents: HashMap::new(),
            }),
            Arc::new(FixtureBindings {
                threads: HashMap::from([(current.clone(), thread.clone())]),
            }),
            Arc::new(FixtureSource {
                records: HashMap::from([(
                    thread.clone(),
                    (1..=4)
                        .map(|sequence| {
                            presentation_record(&thread, sequence, &format!("snapshot-{sequence}"))
                        })
                        .collect(),
                )]),
                ephemeral: HashMap::new(),
            }),
        );
        let stream = service
            .subscribe_visible_journal_stream(
                AgentRunJournalQuery {
                    run_id: current.run_id,
                    agent_id: current.agent_id,
                },
                control["resume"]["future_cursor"]
                    .as_u64()
                    .expect("future cursor"),
            )
            .await
            .expect("future resume stream");
        assert_eq!(
            stream.state.connected_seq,
            control["resume"]["connected_seq"]
        );
        assert!(stream.state.backlog_events.is_empty());

        assert!(matches!(
            stream
                .state
                .project_live_record(presentation_record(&thread, 4, "duplicate-4"))
                .expect("duplicate projection"),
            AgentRunJournalLiveEvent::StaleDurable
        ));
        let live = stream
            .state
            .project_live_record(presentation_record(
                &thread,
                control["resume"]["next_live_source_event_seq"]
                    .as_u64()
                    .expect("next live source event sequence"),
                "live-5",
            ))
            .expect("next live projection");
        let AgentRunJournalLiveEvent::Durable(live) = live else {
            panic!("expected durable live event");
        };
        assert_eq!(live.journal_seq, control["resume"]["next_live_journal_seq"]);
    }

    fn target(run: u128, agent: u128) -> AgentRunRuntimeTarget {
        AgentRunRuntimeTarget {
            run_id: Uuid::from_u128(run),
            agent_id: Uuid::from_u128(agent),
        }
    }

    fn journal_replay_fixture() -> serde_json::Value {
        serde_json::from_str(include_str!(
            "../../../agentdash-agent-runtime-test-support/fixtures/session-parity/main/journal-replay.json"
        ))
        .expect("journal replay fixture")
    }

    fn journal_control_fixture() -> serde_json::Value {
        serde_json::from_str(include_str!(
            "../../../agentdash-agent-runtime-test-support/fixtures/session-parity/main/journal-control.json"
        ))
        .expect("journal control fixture")
    }

    fn fixture_strings(value: &serde_json::Value) -> Vec<&str> {
        value
            .as_array()
            .expect("string fixture array")
            .iter()
            .map(|value| value.as_str().expect("fixture string"))
            .collect()
    }

    fn fixture_u64s(value: &serde_json::Value) -> Vec<u64> {
        value
            .as_array()
            .expect("integer fixture array")
            .iter()
            .map(|value| value.as_u64().expect("fixture integer"))
            .collect()
    }

    fn lineage(
        parent: &AgentRunRuntimeTarget,
        child: &AgentRunRuntimeTarget,
        cutoff: u64,
    ) -> agentdash_domain::workflow::AgentRunLineage {
        agentdash_domain::workflow::AgentRunLineage::new_fork(
            parent.run_id,
            parent.agent_id,
            child.run_id,
            child.agent_id,
            Some(cutoff),
            Some(serde_json::json!({ "turn_id": "parent-turn", "entry_index": 1 })),
            "fixture",
            None,
        )
    }

    fn journal_labels(events: &[AgentRunJournalEvent]) -> Vec<&str> {
        events
            .iter()
            .map(
                |event| match &event.record.as_presentation().expect("presentation").event {
                    agentdash_agent_protocol::BackboneEvent::Platform(
                        agentdash_agent_protocol::PlatformEvent::SessionMetaUpdate { key, .. },
                    ) => key.as_str(),
                    other => panic!("unexpected event: {other:?}"),
                },
            )
            .collect()
    }

    fn presentation_record(
        thread_id: &RuntimeThreadId,
        sequence: u64,
        label: &str,
    ) -> RuntimeJournalRecord {
        RuntimeJournalRecord::new(
            RuntimeCarrierMetadata {
                thread_id: thread_id.clone(),
                recorded_at_ms: 100 + sequence,
                sequence: Some(EventSequence(sequence)),
                transient: None,
                revision: RuntimeRevision(sequence),
                operation_id: None,
                binding_id: None,
                append_idempotency_key: None,
                coordinate: RuntimePresentationCoordinate {
                    runtime_turn_id: None,
                    runtime_item_id: None,
                    interaction_id: None,
                    source_thread_id: Some(thread_id.to_string()),
                    source_turn_id: Some(format!("{thread_id}-turn")),
                    source_item_id: None,
                    source_request_id: None,
                    source_entry_index: Some(sequence as u32 - 1),
                },
            },
            RuntimeJournalFact::Presentation(
                agentdash_agent_runtime_contract::ImmutablePresentationEvent::new(
                    PresentationDurability::Durable,
                    agentdash_agent_protocol::BackboneEvent::Platform(
                        agentdash_agent_protocol::PlatformEvent::SessionMetaUpdate {
                            key: label.to_string(),
                            value: serde_json::json!({ "label": label }),
                        },
                    ),
                ),
            ),
        )
        .expect("presentation record")
    }

    fn presentation_record_from_main_frame(
        thread_id: &RuntimeThreadId,
        frame: &serde_json::Value,
    ) -> RuntimeJournalRecord {
        let sequence = frame["event_seq"].as_u64().expect("Main event sequence");
        let event = serde_json::from_value(frame["notification"]["event"].clone())
            .expect("typed Main protected event");
        RuntimeJournalRecord::new(
            RuntimeCarrierMetadata {
                thread_id: thread_id.clone(),
                recorded_at_ms: frame["occurred_at_ms"]
                    .as_u64()
                    .expect("Main occurred_at_ms"),
                sequence: Some(EventSequence(sequence)),
                transient: None,
                revision: RuntimeRevision(sequence),
                operation_id: None,
                binding_id: None,
                append_idempotency_key: None,
                coordinate: RuntimePresentationCoordinate {
                    runtime_turn_id: None,
                    runtime_item_id: None,
                    interaction_id: None,
                    source_thread_id: frame["notification"]["source"]["connectorId"]
                        .as_str()
                        .map(ToString::to_string),
                    source_turn_id: frame["turn_id"].as_str().map(ToString::to_string),
                    source_item_id: None,
                    source_request_id: None,
                    source_entry_index: frame["entry_index"].as_u64().map(|value| value as u32),
                },
            },
            RuntimeJournalFact::Presentation(
                agentdash_agent_runtime_contract::ImmutablePresentationEvent::new(
                    PresentationDurability::Durable,
                    event,
                ),
            ),
        )
        .expect("Main presentation record")
    }

    fn ephemeral_record(
        thread_id: &RuntimeThreadId,
        sequence: u64,
        label: &str,
    ) -> RuntimeJournalRecord {
        RuntimeJournalRecord::new(
            RuntimeCarrierMetadata {
                thread_id: thread_id.clone(),
                recorded_at_ms: 1_000 + sequence,
                sequence: None,
                transient: Some(RuntimeTransientCoordinate {
                    binding_id: RuntimeBindingId::new("binding").expect("binding id"),
                    stream_generation: RuntimeDriverGeneration(7),
                    sequence: RuntimeTransientSequence(sequence),
                    event_id: RuntimeTransientEventId::new(format!("transient-{sequence}"))
                        .expect("event id"),
                    turn_id: None,
                }),
                revision: RuntimeRevision(sequence),
                operation_id: None,
                binding_id: None,
                append_idempotency_key: None,
                coordinate: RuntimePresentationCoordinate {
                    runtime_turn_id: None,
                    runtime_item_id: None,
                    interaction_id: None,
                    source_thread_id: Some(thread_id.to_string()),
                    source_turn_id: Some(format!("{thread_id}-turn")),
                    source_item_id: None,
                    source_request_id: None,
                    source_entry_index: None,
                },
            },
            RuntimeJournalFact::Presentation(
                agentdash_agent_runtime_contract::ImmutablePresentationEvent::new(
                    PresentationDurability::Ephemeral,
                    agentdash_agent_protocol::BackboneEvent::Platform(
                        agentdash_agent_protocol::PlatformEvent::SessionMetaUpdate {
                            key: label.to_string(),
                            value: serde_json::json!({ "label": label }),
                        },
                    ),
                ),
            ),
        )
        .expect("ephemeral presentation record")
    }

    struct FixtureBindings {
        threads: HashMap<AgentRunRuntimeTarget, RuntimeThreadId>,
    }

    #[async_trait]
    impl AgentRunJournalBindingResolver for FixtureBindings {
        async fn resolve_thread(
            &self,
            target: &AgentRunRuntimeTarget,
        ) -> Result<Option<RuntimeThreadId>, WorkflowApplicationError> {
            Ok(self.threads.get(target).cloned())
        }
    }

    struct FixtureSource {
        records: HashMap<RuntimeThreadId, Vec<RuntimeJournalRecord>>,
        ephemeral: HashMap<RuntimeThreadId, Vec<RuntimeJournalRecord>>,
    }

    #[async_trait]
    impl AgentRunJournalSource for FixtureSource {
        async fn durable_records(
            &self,
            thread_id: &RuntimeThreadId,
        ) -> Result<Vec<RuntimeJournalRecord>, WorkflowApplicationError> {
            Ok(self.records.get(thread_id).cloned().unwrap_or_default())
        }

        async fn subscribe(
            &self,
            thread_id: &RuntimeThreadId,
        ) -> Result<AgentRunJournalSourceSubscription, WorkflowApplicationError> {
            let (_tx, live) = broadcast::channel(4);
            Ok(AgentRunJournalSourceSubscription {
                ephemeral_epoch: 77,
                durable_snapshot: self.records.get(thread_id).cloned().unwrap_or_default(),
                ephemeral_backlog: self.ephemeral.get(thread_id).cloned().unwrap_or_default(),
                live,
            })
        }
    }

    struct FixtureLineage {
        parents: HashMap<AgentRunRuntimeTarget, agentdash_domain::workflow::AgentRunLineage>,
    }

    #[async_trait]
    impl AgentRunLineageRepository for FixtureLineage {
        async fn create(
            &self,
            _lineage: &agentdash_domain::workflow::AgentRunLineage,
        ) -> Result<(), agentdash_domain::DomainError> {
            Ok(())
        }

        async fn find_parent(
            &self,
            child_run_id: Uuid,
            child_agent_id: Uuid,
        ) -> Result<
            Option<agentdash_domain::workflow::AgentRunLineage>,
            agentdash_domain::DomainError,
        > {
            Ok(self
                .parents
                .get(&AgentRunRuntimeTarget {
                    run_id: child_run_id,
                    agent_id: child_agent_id,
                })
                .cloned())
        }

        async fn list_children(
            &self,
            _parent_run_id: Uuid,
            _parent_agent_id: Uuid,
        ) -> Result<Vec<agentdash_domain::workflow::AgentRunLineage>, agentdash_domain::DomainError>
        {
            Ok(Vec::new())
        }

        async fn list_by_run(
            &self,
            _run_id: Uuid,
        ) -> Result<Vec<agentdash_domain::workflow::AgentRunLineage>, agentdash_domain::DomainError>
        {
            Ok(Vec::new())
        }
    }

    fn record(sequence: u64, fact: RuntimeJournalFact) -> RuntimeJournalRecord {
        RuntimeJournalRecord::new(
            RuntimeCarrierMetadata {
                thread_id: RuntimeThreadId::new("thread").expect("thread id"),
                recorded_at_ms: 100 + sequence,
                sequence: Some(EventSequence(sequence)),
                transient: None,
                revision: RuntimeRevision(sequence),
                operation_id: None,
                binding_id: None,
                append_idempotency_key: None,
                coordinate: RuntimePresentationCoordinate {
                    runtime_turn_id: None,
                    runtime_item_id: None,
                    interaction_id: None,
                    source_thread_id: None,
                    source_turn_id: None,
                    source_item_id: None,
                    source_request_id: None,
                    source_entry_index: None,
                },
            },
            fact,
        )
        .expect("journal record")
    }
}
