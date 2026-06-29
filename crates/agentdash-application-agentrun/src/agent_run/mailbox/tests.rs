use super::*;
use std::io;
use std::sync::Arc;

use agentdash_agent_protocol::{
    BackboneEnvelope, UserInputBlock, UserInputSubmissionKind, text_user_input_blocks,
};
use agentdash_domain::DomainError;
use agentdash_domain::agent_run_mailbox::{
    AgentRunMailboxClaimRequest, AgentRunMailboxMessage, AgentRunMailboxRepository,
    AgentRunMailboxState, ConsumptionBarrier, MailboxDelivery, MailboxDrainMode,
    MailboxMessageOrigin, MailboxMessageStatus, MailboxSourceIdentity, NewAgentRunMailboxMessage,
};
use agentdash_domain::workflow::{
    AgentFrame, AgentRunCommandKind, AgentRunCommandReceiptRepository, AgentRunCommandStatus,
    AgentSource, DeliveryBindingStatus, LifecycleAgent, LifecycleRun, LifecycleRunRepository,
    NewAgentRunCommandReceipt, RuntimeSessionExecutionAnchor,
};
use agentdash_spi::ConnectorError;
use agentdash_spi::session_persistence::{SessionEventPage, SessionMeta};
use tokio::sync::Mutex;

use crate::agent_run::runtime_session_boundary::{
    LaunchCommand, RuntimeSessionControlPort, RuntimeSessionCorePort, RuntimeSessionEventingPort,
    RuntimeSessionLaunchPort, SessionControlService, SessionCoreService, SessionEventingService,
    SessionExecutionState, SessionLaunchService, SessionTurnSteerCommand,
};
use crate::test_support::{
    MemoryAgentFrameRepository, MemoryAgentRunCommandReceiptRepository,
    MemoryLifecycleAgentRepository, MemoryRuntimeSessionExecutionAnchorRepository,
};

#[test]
fn mailbox_command_target_can_be_address_first_without_message_stream() {
    let address = AgentRunRuntimeAddress {
        run_id: Uuid::new_v4(),
        agent_id: Uuid::new_v4(),
        frame_id: Uuid::new_v4(),
    };

    let target = AgentRunMailboxCommandTarget::new(address.clone());

    assert_eq!(target.address, address);
    assert!(target.message_stream.is_none());
}

#[test]
fn runtime_session_adapter_keeps_session_as_message_stream_ref() {
    let run_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let frame_id = Uuid::new_v4();

    let target = AgentRunMailboxCommandTarget::from_runtime_session_adapter(
        run_id,
        agent_id,
        frame_id,
        "runtime-session-1",
    );

    assert_eq!(
        target.address,
        AgentRunRuntimeAddress {
            run_id,
            agent_id,
            frame_id,
        }
    );
    assert_eq!(
        target.message_stream,
        Some(MessageStreamProjectionRef {
            runtime_session_id: "runtime-session-1".to_string(),
            trace_kind: MessageStreamTraceKind::ConnectorRuntimeSession,
        })
    );
}

#[test]
fn mailbox_source_identity_dedup_prefers_source_ref_and_correlation_ref() {
    let source = MailboxSourceIdentity::new("routine", "trigger", "routine")
        .with_source_ref("routine-execution-1")
        .with_correlation_ref("trigger-1");

    assert_eq!(
        mailbox_source_identity_dedup_key(&source).as_deref(),
        Some("source:routine:trigger:ref:routine-execution-1:correlation:trigger-1")
    );
}

#[test]
fn mailbox_source_identity_dedup_can_use_correlation_without_source_ref() {
    let source = MailboxSourceIdentity::new("companion", "parent_response", "agent")
        .with_correlation_ref("gate-1");

    assert_eq!(
        mailbox_source_identity_dedup_key(&source).as_deref(),
        Some("source:companion:parent_response:correlation:gate-1")
    );
}

#[test]
fn mailbox_intake_command_prefers_source_identity_dedup() {
    let command = AgentRunMailboxIntakeTargetCommand {
        target: AgentRunMailboxCommandTarget::new(AgentRunRuntimeAddress {
            run_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            frame_id: Uuid::new_v4(),
        }),
        origin: MailboxMessageOrigin::Companion,
        source: MailboxSourceIdentity::new("companion", "result", "agent")
            .with_source_ref("gate-1"),
        retain_payload: true,
        schedule_on_submit: false,
        input: Vec::new(),
        client_command_id: "cmd-1".to_string(),
        source_dedup_key: Some("custom-dedup".to_string()),
        executor_config: None,
        identity: None,
        delivery_intent: None,
    };

    assert_eq!(
        command.stable_source_dedup_key().as_deref(),
        Some("source:companion:result:ref:gate-1")
    );
}

#[test]
fn mailbox_intake_command_uses_explicit_source_dedup_without_source_refs() {
    let command = AgentRunMailboxIntakeTargetCommand {
        target: AgentRunMailboxCommandTarget::new(AgentRunRuntimeAddress {
            run_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            frame_id: Uuid::new_v4(),
        }),
        origin: MailboxMessageOrigin::Companion,
        source: MailboxSourceIdentity::new("companion", "result", "agent"),
        retain_payload: true,
        schedule_on_submit: false,
        input: Vec::new(),
        client_command_id: "cmd-1".to_string(),
        source_dedup_key: Some("custom-dedup".to_string()),
        executor_config: None,
        identity: None,
        delivery_intent: None,
    };

    assert_eq!(
        command.stable_source_dedup_key().as_deref(),
        Some("custom-dedup")
    );
}

#[tokio::test]
async fn mailbox_steering_event_projection_failure_is_consistent() {
    let delegate = MailboxSteeringFixture::new(true).await;
    let delegate_message = delegate
        .seed_message(
            "delegate-event-failure",
            MailboxDelivery::LaunchOrContinueTurn,
            None,
        )
        .await;

    let delegate_messages = delegate
        .service()
        .drain_agent_run_turn_boundary_for_delegate(&delegate.runtime_session_id)
        .await
        .expect("delegate drain");

    assert_eq!(delegate_messages.len(), 1);
    let updated_delegate = delegate.mailbox.message(delegate_message.id).await;
    assert_eq!(updated_delegate.status, MailboxMessageStatus::Steered);
    assert_error_contains(&updated_delegate, "事件写入失败");
    assert_receipt_accepted(
        delegate.receipts.as_ref(),
        delegate_message.command_receipt_id.unwrap(),
        AgentRunMailboxCommandOutcome::Steered,
        Some(&delegate.active_turn_id),
    )
    .await;
    assert!(delegate.mailbox.cleaned(delegate_message.id).await);

    let scheduler = MailboxSteeringFixture::new(true).await;
    let scheduler_message = scheduler
        .seed_message(
            "scheduler-event-failure",
            MailboxDelivery::LaunchOrContinueTurn,
            None,
        )
        .await;
    let target = AgentRunMailboxCommandTarget::from_runtime_session_adapter(
        scheduler.run.id,
        scheduler.agent.id,
        scheduler.current_frame.id,
        scheduler.runtime_session_id.clone(),
    );

    let outcomes = scheduler
        .service()
        .schedule_for_target(
            target,
            AgentRunMailboxScheduleTrigger::AgentRunTurnBoundary,
            None,
        )
        .await
        .expect("scheduler drain");

    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].outcome, AgentRunMailboxCommandOutcome::Steered);
    let updated_scheduler = scheduler.mailbox.message(scheduler_message.id).await;
    assert_eq!(updated_scheduler.status, MailboxMessageStatus::Steered);
    assert_error_contains(&updated_scheduler, "事件写入失败");
    assert_receipt_accepted(
        scheduler.receipts.as_ref(),
        scheduler_message.command_receipt_id.unwrap(),
        AgentRunMailboxCommandOutcome::Steered,
        Some(&scheduler.active_turn_id),
    )
    .await;
    assert!(scheduler.mailbox.cleaned(scheduler_message.id).await);
    assert_eq!(*scheduler.control.steer_count.lock().await, 1);
}

#[tokio::test]
async fn mailbox_steering_expected_turn_guard_is_consistent() {
    let delegate = MailboxSteeringFixture::new(false).await;
    let delegate_message = delegate
        .seed_message(
            "delegate-expected-turn",
            MailboxDelivery::LaunchOrContinueTurn,
            Some("stale-turn"),
        )
        .await;

    let delegate_messages = delegate
        .service()
        .drain_agent_run_turn_boundary_for_delegate(&delegate.runtime_session_id)
        .await
        .expect("delegate drain");

    assert!(delegate_messages.is_empty());
    let updated_delegate = delegate.mailbox.message(delegate_message.id).await;
    assert_eq!(updated_delegate.status, MailboxMessageStatus::Blocked);
    assert_error_contains(&updated_delegate, "expected_agent_run_turn_mismatch");
    assert_receipt_accepted(
        delegate.receipts.as_ref(),
        delegate_message.command_receipt_id.unwrap(),
        AgentRunMailboxCommandOutcome::Blocked,
        None,
    )
    .await;
    assert!(!delegate.mailbox.cleaned(delegate_message.id).await);
    assert_eq!(*delegate.eventing.emit_count.lock().await, 0);

    let scheduler = MailboxSteeringFixture::new(false).await;
    let scheduler_message = scheduler
        .seed_message(
            "scheduler-expected-turn",
            MailboxDelivery::LaunchOrContinueTurn,
            Some("stale-turn"),
        )
        .await;
    let target = AgentRunMailboxCommandTarget::from_runtime_session_adapter(
        scheduler.run.id,
        scheduler.agent.id,
        scheduler.current_frame.id,
        scheduler.runtime_session_id.clone(),
    );

    let outcomes = scheduler
        .service()
        .schedule_for_target(
            target,
            AgentRunMailboxScheduleTrigger::AgentRunTurnBoundary,
            None,
        )
        .await
        .expect("scheduler drain");

    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].outcome, AgentRunMailboxCommandOutcome::Blocked);
    let updated_scheduler = scheduler.mailbox.message(scheduler_message.id).await;
    assert_eq!(updated_scheduler.status, MailboxMessageStatus::Blocked);
    assert_error_contains(&updated_scheduler, "expected_agent_run_turn_mismatch");
    assert_receipt_accepted(
        scheduler.receipts.as_ref(),
        scheduler_message.command_receipt_id.unwrap(),
        AgentRunMailboxCommandOutcome::Blocked,
        None,
    )
    .await;
    assert!(!scheduler.mailbox.cleaned(scheduler_message.id).await);
    assert_eq!(*scheduler.eventing.emit_count.lock().await, 0);
    assert_eq!(*scheduler.control.steer_count.lock().await, 0);
}

struct MailboxSteeringFixture {
    runs: Arc<MemoryLifecycleRunRepository>,
    agents: Arc<MemoryLifecycleAgentRepository>,
    frames: Arc<MemoryAgentFrameRepository>,
    anchors: Arc<MemoryRuntimeSessionExecutionAnchorRepository>,
    receipts: Arc<MemoryAgentRunCommandReceiptRepository>,
    mailbox: Arc<MemoryMailboxRepository>,
    core: Arc<TestCorePort>,
    control: Arc<TestControlPort>,
    eventing: Arc<TestEventingPort>,
    launch: Arc<TestLaunchPort>,
    run: LifecycleRun,
    agent: LifecycleAgent,
    current_frame: AgentFrame,
    runtime_session_id: String,
    active_turn_id: String,
}

impl MailboxSteeringFixture {
    async fn new(fail_events: bool) -> Self {
        let runs = Arc::new(MemoryLifecycleRunRepository::default());
        let agents = Arc::new(MemoryLifecycleAgentRepository::default());
        let frames = Arc::new(MemoryAgentFrameRepository::default());
        let anchors = Arc::new(MemoryRuntimeSessionExecutionAnchorRepository::default());
        let receipts = Arc::new(MemoryAgentRunCommandReceiptRepository::default());
        let mailbox = Arc::new(MemoryMailboxRepository::default());
        let runtime_session_id = "runtime-steering".to_string();
        let active_turn_id = "active-turn".to_string();

        let run = LifecycleRun::new_plain(Uuid::new_v4());
        runs.create(&run).await.expect("run");
        let mut agent = LifecycleAgent::new_root(run.id, run.project_id, AgentSource::ProjectAgent);
        let launch_frame = AgentFrame::new_initial(agent.id);
        let current_frame = AgentFrame::new_revision(agent.id, 2, "test");
        let anchor = RuntimeSessionExecutionAnchor::new_dispatch(
            runtime_session_id.clone(),
            run.id,
            launch_frame.id,
            agent.id,
        );
        agent.bind_current_delivery_from_anchor(
            &anchor,
            DeliveryBindingStatus::Running,
            anchor.updated_at,
        );
        frames.create(&launch_frame).await.expect("launch frame");
        frames.create(&current_frame).await.expect("current frame");
        anchors.upsert(&anchor).await.expect("anchor");
        agents.create(&agent).await.expect("agent");

        Self {
            runs,
            agents,
            frames,
            anchors,
            receipts,
            mailbox,
            core: Arc::new(TestCorePort {
                state: Mutex::new(SessionExecutionState::Running {
                    turn_id: Some(active_turn_id.clone()),
                }),
            }),
            control: Arc::new(TestControlPort::default()),
            eventing: Arc::new(TestEventingPort {
                fail_events,
                emit_count: Mutex::new(0),
            }),
            launch: Arc::new(TestLaunchPort),
            run,
            agent,
            current_frame,
            runtime_session_id,
            active_turn_id,
        }
    }

    fn service(&self) -> AgentRunMailboxService<'_> {
        AgentRunMailboxService::new(
            self.runs.as_ref(),
            self.agents.as_ref(),
            self.frames.as_ref(),
            self.anchors.as_ref(),
            self.receipts.as_ref(),
            self.mailbox.as_ref(),
            SessionCoreService::new(self.core.clone()),
            SessionControlService::new(self.control.clone()),
            SessionEventingService::new(self.eventing.clone()),
            SessionLaunchService::new(self.launch.clone()),
        )
    }

    async fn seed_message(
        &self,
        client_command_id: &str,
        delivery: MailboxDelivery,
        expected_active_turn: Option<&str>,
    ) -> AgentRunMailboxMessage {
        let receipt = self
            .receipts
            .claim(NewAgentRunCommandReceipt {
                scope_kind: "agent_run_mailbox".to_string(),
                scope_key: format!("{}:{}", self.run.id, self.agent.id),
                command_kind: AgentRunCommandKind::MessageSubmit,
                client_command_id: client_command_id.to_string(),
                request_digest: format!("digest:{client_command_id}"),
            })
            .await
            .expect("receipt");
        let message = mailbox_message(
            self.run.id,
            self.agent.id,
            &self.runtime_session_id,
            delivery,
            expected_active_turn.map(str::to_string),
            Some(receipt.receipt().id),
        );
        self.mailbox.insert(message.clone()).await;
        message
    }
}

#[derive(Default)]
struct MemoryLifecycleRunRepository {
    runs: Mutex<Vec<LifecycleRun>>,
}

#[async_trait::async_trait]
impl LifecycleRunRepository for MemoryLifecycleRunRepository {
    async fn create(&self, run: &LifecycleRun) -> Result<(), DomainError> {
        self.runs.lock().await.push(run.clone());
        Ok(())
    }

    async fn get_by_id(&self, id: Uuid) -> Result<Option<LifecycleRun>, DomainError> {
        Ok(self
            .runs
            .lock()
            .await
            .iter()
            .find(|run| run.id == id)
            .cloned())
    }

    async fn list_by_ids(&self, ids: &[Uuid]) -> Result<Vec<LifecycleRun>, DomainError> {
        Ok(self
            .runs
            .lock()
            .await
            .iter()
            .filter(|run| ids.contains(&run.id))
            .cloned()
            .collect())
    }

    async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<LifecycleRun>, DomainError> {
        Ok(self
            .runs
            .lock()
            .await
            .iter()
            .filter(|run| run.project_id == project_id)
            .cloned()
            .collect())
    }

    async fn update(&self, run: &LifecycleRun) -> Result<(), DomainError> {
        let mut runs = self.runs.lock().await;
        if let Some(existing) = runs.iter_mut().find(|item| item.id == run.id) {
            *existing = run.clone();
        }
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
        self.runs.lock().await.retain(|run| run.id != id);
        Ok(())
    }
}

#[derive(Default)]
struct MemoryMailboxRepository {
    messages: Mutex<Vec<AgentRunMailboxMessage>>,
    cleaned: Mutex<Vec<Uuid>>,
}

impl MemoryMailboxRepository {
    async fn insert(&self, message: AgentRunMailboxMessage) {
        self.messages.lock().await.push(message);
    }

    async fn message(&self, id: Uuid) -> AgentRunMailboxMessage {
        self.messages
            .lock()
            .await
            .iter()
            .find(|message| message.id == id)
            .cloned()
            .expect("message")
    }

    async fn cleaned(&self, id: Uuid) -> bool {
        self.cleaned.lock().await.contains(&id)
    }
}

#[async_trait::async_trait]
impl AgentRunMailboxRepository for MemoryMailboxRepository {
    async fn create_message(
        &self,
        message: NewAgentRunMailboxMessage,
    ) -> Result<AgentRunMailboxMessage, DomainError> {
        let message = message_from_new(message);
        self.insert(message.clone()).await;
        Ok(message)
    }

    async fn create_message_idempotent(
        &self,
        message: NewAgentRunMailboxMessage,
    ) -> Result<AgentRunMailboxMessage, DomainError> {
        self.create_message(message).await
    }

    async fn get_message(&self, id: Uuid) -> Result<Option<AgentRunMailboxMessage>, DomainError> {
        Ok(self
            .messages
            .lock()
            .await
            .iter()
            .find(|message| message.id == id)
            .cloned())
    }

    async fn list_messages(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
    ) -> Result<Vec<AgentRunMailboxMessage>, DomainError> {
        Ok(self
            .messages
            .lock()
            .await
            .iter()
            .filter(|message| message.run_id == run_id && message.agent_id == agent_id)
            .cloned()
            .collect())
    }

    async fn claim_next(
        &self,
        request: AgentRunMailboxClaimRequest,
    ) -> Result<Vec<AgentRunMailboxMessage>, DomainError> {
        let mut messages = self.messages.lock().await;
        let mut claimed = Vec::new();
        for message in messages.iter_mut() {
            if claimed.len() >= request.limit as usize {
                break;
            }
            if message.run_id != request.run_id
                || message.agent_id != request.agent_id
                || request.runtime_session_id.as_deref() != Some(&message.runtime_session_id)
                || !request.barriers.contains(&message.barrier)
                || request
                    .drain_mode
                    .is_some_and(|mode| mode != message.drain_mode)
                || !matches!(
                    message.status,
                    MailboxMessageStatus::Queued | MailboxMessageStatus::ReadyToConsume
                )
            {
                continue;
            }
            message.status = MailboxMessageStatus::Consuming;
            message.claim_token = Some(request.claim_token);
            message.claim_expires_at = Some(request.claim_expires_at);
            message.attempt_count += 1;
            claimed.push(message.clone());
        }
        Ok(claimed)
    }

    async fn recover_expired_consuming(
        &self,
        _now: chrono::DateTime<Utc>,
    ) -> Result<u64, DomainError> {
        Ok(0)
    }

    async fn mark_message_status(
        &self,
        id: Uuid,
        claim_token: Option<Uuid>,
        status: MailboxMessageStatus,
        accepted_agent_run_turn_id: Option<String>,
        accepted_protocol_turn_id: Option<String>,
        last_error: Option<String>,
    ) -> Result<AgentRunMailboxMessage, DomainError> {
        let mut messages = self.messages.lock().await;
        let message = messages
            .iter_mut()
            .find(|message| message.id == id)
            .ok_or_else(|| DomainError::NotFound {
                entity: "agent_run_mailbox_message",
                id: id.to_string(),
            })?;
        if message.claim_token != claim_token {
            return Err(DomainError::Conflict {
                entity: "agent_run_mailbox_message",
                constraint: "claim_token",
                message: "claim token mismatch".to_string(),
            });
        }
        message.status = status;
        message.accepted_agent_run_turn_id = accepted_agent_run_turn_id;
        message.accepted_protocol_turn_id = accepted_protocol_turn_id;
        message.last_error = last_error;
        message.claim_token = None;
        message.claim_expires_at = None;
        message.consumed_at = Some(Utc::now());
        message.updated_at = Utc::now();
        Ok(message.clone())
    }

    async fn update_message_policy(
        &self,
        id: Uuid,
        delivery: MailboxDelivery,
        barrier: ConsumptionBarrier,
        drain_mode: MailboxDrainMode,
        priority: i32,
    ) -> Result<AgentRunMailboxMessage, DomainError> {
        let mut messages = self.messages.lock().await;
        let message = messages
            .iter_mut()
            .find(|message| message.id == id)
            .ok_or_else(|| DomainError::NotFound {
                entity: "agent_run_mailbox_message",
                id: id.to_string(),
            })?;
        message.delivery = delivery;
        message.barrier = barrier;
        message.drain_mode = drain_mode;
        message.priority = priority;
        Ok(message.clone())
    }

    async fn delete_message(
        &self,
        id: Uuid,
    ) -> Result<Option<AgentRunMailboxMessage>, DomainError> {
        let mut messages = self.messages.lock().await;
        if let Some(message) = messages.iter_mut().find(|message| message.id == id) {
            message.status = MailboxMessageStatus::Deleted;
            return Ok(Some(message.clone()));
        }
        Ok(None)
    }

    async fn cleanup_user_payload(&self, id: Uuid) -> Result<(), DomainError> {
        self.cleaned.lock().await.push(id);
        if let Some(message) = self
            .messages
            .lock()
            .await
            .iter_mut()
            .find(|message| message.id == id)
        {
            message.payload_json = None;
        }
        Ok(())
    }

    async fn pause_state(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
        runtime_session_id: String,
        reason: String,
        message: Option<String>,
    ) -> Result<AgentRunMailboxState, DomainError> {
        Ok(AgentRunMailboxState {
            run_id,
            agent_id,
            runtime_session_id,
            paused: true,
            pause_reason: Some(reason),
            pause_message: message,
            updated_at: Utc::now(),
        })
    }

    async fn resume_state(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
        runtime_session_id: String,
    ) -> Result<AgentRunMailboxState, DomainError> {
        Ok(AgentRunMailboxState {
            run_id,
            agent_id,
            runtime_session_id,
            paused: false,
            pause_reason: None,
            pause_message: None,
            updated_at: Utc::now(),
        })
    }

    async fn get_state(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
    ) -> Result<Option<AgentRunMailboxState>, DomainError> {
        Ok(Some(AgentRunMailboxState {
            run_id,
            agent_id,
            runtime_session_id: "runtime-steering".to_string(),
            paused: false,
            pause_reason: None,
            pause_message: None,
            updated_at: Utc::now(),
        }))
    }

    async fn move_message_after(
        &self,
        id: Uuid,
        _after_id: Option<Uuid>,
        _run_id: Uuid,
        _agent_id: Uuid,
    ) -> Result<AgentRunMailboxMessage, DomainError> {
        self.get_message(id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity: "agent_run_mailbox_message",
                id: id.to_string(),
            })
    }
}

struct TestCorePort {
    state: Mutex<SessionExecutionState>,
}

#[async_trait::async_trait]
impl RuntimeSessionCorePort for TestCorePort {
    async fn inspect_session_execution_state(
        &self,
        _session_id: &str,
    ) -> Result<SessionExecutionState, WorkflowApplicationError> {
        Ok(self.state.lock().await.clone())
    }

    async fn get_session_meta(
        &self,
        _session_id: &str,
    ) -> Result<Option<SessionMeta>, WorkflowApplicationError> {
        Ok(None)
    }

    async fn delete_session(&self, _session_id: &str) -> Result<(), WorkflowApplicationError> {
        Ok(())
    }
}

#[derive(Default)]
struct TestControlPort {
    steer_count: Mutex<usize>,
}

#[async_trait::async_trait]
impl RuntimeSessionControlPort for TestControlPort {
    async fn supports_session_steering(&self, _session_id: &str) -> bool {
        true
    }

    async fn steer_session(&self, _command: SessionTurnSteerCommand) -> Result<(), ConnectorError> {
        *self.steer_count.lock().await += 1;
        Ok(())
    }
}

struct TestEventingPort {
    fail_events: bool,
    emit_count: Mutex<usize>,
}

#[async_trait::async_trait]
impl RuntimeSessionEventingPort for TestEventingPort {
    async fn list_event_page(
        &self,
        _session_id: &str,
        _after_seq: u64,
        _limit: u32,
    ) -> io::Result<SessionEventPage> {
        Ok(SessionEventPage {
            snapshot_seq: 0,
            events: Vec::new(),
            has_more: false,
            next_after_seq: 0,
        })
    }

    async fn persist_notification(
        &self,
        _session_id: &str,
        _envelope: BackboneEnvelope,
    ) -> Result<(), WorkflowApplicationError> {
        Ok(())
    }

    async fn emit_user_input_submitted(
        &self,
        _session_id: &str,
        _turn_id: &str,
        _event_id: &str,
        _kind: UserInputSubmissionKind,
        _input: Vec<UserInputBlock>,
    ) -> Result<(), WorkflowApplicationError> {
        *self.emit_count.lock().await += 1;
        if self.fail_events {
            Err(WorkflowApplicationError::Internal(
                "event projection failed".to_string(),
            ))
        } else {
            Ok(())
        }
    }
}

struct TestLaunchPort;

#[async_trait::async_trait]
impl RuntimeSessionLaunchPort for TestLaunchPort {
    async fn launch_command_in_task(
        &self,
        _session_id: String,
        _command: LaunchCommand,
    ) -> Result<String, WorkflowApplicationError> {
        Ok("launched-turn".to_string())
    }
}

fn mailbox_message(
    run_id: Uuid,
    agent_id: Uuid,
    runtime_session_id: &str,
    delivery: MailboxDelivery,
    expected_active_agent_run_turn_id: Option<String>,
    command_receipt_id: Option<Uuid>,
) -> AgentRunMailboxMessage {
    let now = Utc::now();
    AgentRunMailboxMessage {
        id: Uuid::new_v4(),
        run_id,
        agent_id,
        runtime_session_id: runtime_session_id.to_string(),
        origin: MailboxMessageOrigin::User,
        source: MailboxSourceIdentity::composer(),
        delivery,
        barrier: ConsumptionBarrier::AgentRunTurnBoundary,
        drain_mode: MailboxDrainMode::One,
        status: MailboxMessageStatus::ReadyToConsume,
        priority: 0,
        order_key: 0,
        source_dedup_key: None,
        queued_agent_run_turn_id: None,
        consuming_agent_run_turn_id: None,
        expected_active_agent_run_turn_id,
        accepted_agent_run_turn_id: None,
        accepted_protocol_turn_id: None,
        claim_token: None,
        claimed_at: None,
        claim_expires_at: None,
        command_receipt_id,
        payload_json: Some(serde_json::to_value(text_user_input_blocks("hello")).unwrap()),
        executor_config_json: None,
        preview: "hello".to_string(),
        has_images: false,
        retain_payload: false,
        attempt_count: 0,
        last_error: None,
        created_at: now,
        updated_at: now,
        consumed_at: None,
        deleted_at: None,
    }
}

fn message_from_new(message: NewAgentRunMailboxMessage) -> AgentRunMailboxMessage {
    mailbox_message(
        message.run_id,
        message.agent_id,
        &message.runtime_session_id,
        message.delivery,
        message.expected_active_agent_run_turn_id,
        message.command_receipt_id,
    )
}

fn assert_error_contains(message: &AgentRunMailboxMessage, expected: &str) {
    assert!(
        message
            .last_error
            .as_deref()
            .is_some_and(|error| error.contains(expected)),
        "expected last_error to contain {expected}, got {:?}",
        message.last_error
    );
}

async fn assert_receipt_accepted(
    receipts: &dyn AgentRunCommandReceiptRepository,
    receipt_id: Uuid,
    outcome: AgentRunMailboxCommandOutcome,
    accepted_turn_id: Option<&str>,
) {
    let receipt = receipts
        .get(receipt_id)
        .await
        .expect("receipt read")
        .expect("receipt");
    assert_eq!(receipt.status, AgentRunCommandStatus::Accepted);
    assert_eq!(
        receipt
            .result_json
            .as_ref()
            .and_then(outcome_from_result_json),
        Some(outcome)
    );
    assert_eq!(
        receipt
            .accepted_refs
            .as_ref()
            .and_then(|refs| refs.agent_run_turn_id.as_deref()),
        accepted_turn_id
    );
}
