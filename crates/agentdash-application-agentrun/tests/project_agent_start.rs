use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use agentdash_application_agentrun::WorkflowApplicationError;
use agentdash_application_agentrun::agent_run::{
    AgentRunAcceptedProductResultKind, AgentRunMessageProductResultProjector,
    AgentRunMessageSubmissionFailure, AgentRunMessageSubmissionOwnership,
    AgentRunMessageSubmissionResult, ProjectAgentInitialMessageSubmission,
    ProjectAgentInitialMessageSubmissionPort, ProjectAgentLifecycleLaunchPort,
    ProjectAgentRunStartCommand, ProjectAgentRunStartDeps, ProjectAgentRunStartProjectionContext,
    ProjectAgentRunStartReceiptPort, ProjectAgentRunStartReceiptRequest,
    ProjectAgentRunStartService,
};
use agentdash_application_ports::agent_run_message_submission::AgentRunMessageSubmissionReservation;
use agentdash_domain::DomainError;
use agentdash_domain::agent::{ProjectAgent, ProjectAgentRepository};
use agentdash_domain::agent_run_mailbox::AgentRunMailboxMessage;
use agentdash_domain::workflow::{
    AgentFrame, AgentFrameRepository, AgentLaunchDispatchResult, AgentLaunchIntent,
    AgentRunCommandKind, AgentRunCommandReceipt, AgentRunCommandStatus, AgentRuntimeRefs,
    LifecycleRun, LifecycleRunRepository, WorkflowGraphRef,
};
use agentdash_test_support::workflow::MemoryProjectAgentRepository;
use chrono::Utc;
use serde_json::{Value, json};
use uuid::Uuid;

#[derive(Default)]
struct FixtureGraph {
    runs: Mutex<HashMap<Uuid, LifecycleRun>>,
    agents: Mutex<HashMap<Uuid, Uuid>>,
    frames: Mutex<HashMap<Uuid, AgentFrame>>,
}

impl FixtureGraph {
    fn counts(&self) -> (usize, usize, usize) {
        (
            self.runs.lock().expect("runs lock").len(),
            self.agents.lock().expect("agents lock").len(),
            self.frames.lock().expect("frames lock").len(),
        )
    }
}

#[async_trait::async_trait]
impl LifecycleRunRepository for FixtureGraph {
    async fn create(&self, run: &LifecycleRun) -> Result<(), DomainError> {
        self.runs
            .lock()
            .expect("runs lock")
            .insert(run.id, run.clone());
        Ok(())
    }

    async fn get_by_id(&self, id: Uuid) -> Result<Option<LifecycleRun>, DomainError> {
        Ok(self.runs.lock().expect("runs lock").get(&id).cloned())
    }

    async fn list_by_ids(&self, ids: &[Uuid]) -> Result<Vec<LifecycleRun>, DomainError> {
        let runs = self.runs.lock().expect("runs lock");
        Ok(ids.iter().filter_map(|id| runs.get(id).cloned()).collect())
    }

    async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<LifecycleRun>, DomainError> {
        Ok(self
            .runs
            .lock()
            .expect("runs lock")
            .values()
            .filter(|run| run.project_id == project_id)
            .cloned()
            .collect())
    }

    async fn update(&self, run: &LifecycleRun) -> Result<(), DomainError> {
        self.runs
            .lock()
            .expect("runs lock")
            .insert(run.id, run.clone());
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
        self.runs.lock().expect("runs lock").remove(&id);
        let removed_agents = {
            let mut agents = self.agents.lock().expect("agents lock");
            let removed = agents
                .iter()
                .filter_map(|(agent_id, run_id)| (*run_id == id).then_some(*agent_id))
                .collect::<HashSet<_>>();
            agents.retain(|agent_id, _| !removed.contains(agent_id));
            removed
        };
        self.frames
            .lock()
            .expect("frames lock")
            .retain(|_, frame| !removed_agents.contains(&frame.agent_id));
        Ok(())
    }
}

#[async_trait::async_trait]
impl AgentFrameRepository for FixtureGraph {
    async fn create(&self, frame: &AgentFrame) -> Result<(), DomainError> {
        self.frames
            .lock()
            .expect("frames lock")
            .insert(frame.id, frame.clone());
        Ok(())
    }

    async fn get(&self, frame_id: Uuid) -> Result<Option<AgentFrame>, DomainError> {
        Ok(self
            .frames
            .lock()
            .expect("frames lock")
            .get(&frame_id)
            .cloned())
    }

    async fn get_latest(&self, agent_id: Uuid) -> Result<Option<AgentFrame>, DomainError> {
        Ok(self
            .frames
            .lock()
            .expect("frames lock")
            .values()
            .filter(|frame| frame.agent_id == agent_id)
            .max_by_key(|frame| frame.revision)
            .cloned())
    }

    async fn list_by_agent(&self, agent_id: Uuid) -> Result<Vec<AgentFrame>, DomainError> {
        Ok(self
            .frames
            .lock()
            .expect("frames lock")
            .values()
            .filter(|frame| frame.agent_id == agent_id)
            .cloned()
            .collect())
    }
}

struct RecordingLaunch {
    graph: Arc<FixtureGraph>,
    calls: AtomicUsize,
    workflow_graph_ref: Mutex<Option<WorkflowGraphRef>>,
}

impl RecordingLaunch {
    fn new(graph: Arc<FixtureGraph>) -> Self {
        Self {
            graph,
            calls: AtomicUsize::new(0),
            workflow_graph_ref: Mutex::new(None),
        }
    }
}

#[async_trait::async_trait]
impl ProjectAgentLifecycleLaunchPort for RecordingLaunch {
    async fn launch_project_agent(
        &self,
        intent: &AgentLaunchIntent,
    ) -> Result<AgentLaunchDispatchResult, WorkflowApplicationError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        *self
            .workflow_graph_ref
            .lock()
            .expect("workflow graph ref lock") = intent.workflow_graph_ref.clone();

        let run = LifecycleRun::new_plain(intent.project_id);
        let agent_id = Uuid::new_v4();
        let frame = AgentFrame::new_revision(agent_id, 1, "fixture_launch");
        self.graph
            .runs
            .lock()
            .expect("runs lock")
            .insert(run.id, run.clone());
        self.graph
            .agents
            .lock()
            .expect("agents lock")
            .insert(agent_id, run.id);
        self.graph
            .frames
            .lock()
            .expect("frames lock")
            .insert(frame.id, frame.clone());

        Ok(AgentLaunchDispatchResult {
            runtime_refs: AgentRuntimeRefs::new(run.id, agent_id, frame.id, None),
            delivery_runtime_ref: Uuid::new_v4(),
        })
    }
}

#[derive(Default)]
struct FixtureReceipts {
    receipts: Mutex<Vec<AgentRunCommandReceipt>>,
}

impl FixtureReceipts {
    fn count(&self) -> usize {
        self.receipts.lock().expect("receipts lock").len()
    }

    fn accept(&self, receipt_id: Uuid, result_json: Value) -> Result<(), WorkflowApplicationError> {
        let mut receipts = self.receipts.lock().expect("receipts lock");
        let receipt = receipts
            .iter_mut()
            .find(|receipt| receipt.id == receipt_id)
            .ok_or_else(|| WorkflowApplicationError::NotFound(receipt_id.to_string()))?;
        let now = Utc::now();
        receipt.status = AgentRunCommandStatus::Accepted;
        receipt.mailbox_message_id = Some(Uuid::new_v4());
        receipt.result_json = Some(result_json);
        receipt.accepted_at = Some(now);
        receipt.updated_at = now;
        Ok(())
    }
}

#[async_trait::async_trait]
impl ProjectAgentRunStartReceiptPort for FixtureReceipts {
    async fn reserve_project_agent_start(
        &self,
        request: ProjectAgentRunStartReceiptRequest,
    ) -> Result<AgentRunMessageSubmissionReservation, WorkflowApplicationError> {
        let scope_key = format!("{}:{}", request.project_id, request.project_agent_id);
        let mut receipts = self.receipts.lock().expect("receipts lock");
        if let Some(receipt) = receipts.iter().find(|receipt| {
            receipt.scope_key == scope_key && receipt.client_command_id == request.client_command_id
        }) {
            if receipt.request_digest != request.request_digest {
                return Err(WorkflowApplicationError::Conflict(
                    "same client command id has different semantics".to_string(),
                ));
            }
            return Ok(match receipt.status {
                AgentRunCommandStatus::Pending => {
                    AgentRunMessageSubmissionReservation::ReconcileRequired {
                        receipt: receipt.clone(),
                    }
                }
                AgentRunCommandStatus::Accepted | AgentRunCommandStatus::TerminalFailed => {
                    AgentRunMessageSubmissionReservation::Replay {
                        receipt: receipt.clone(),
                    }
                }
            });
        }
        let now = Utc::now();
        let receipt = AgentRunCommandReceipt {
            id: Uuid::new_v4(),
            scope_kind: "project_agent_run_start".to_string(),
            scope_key,
            command_kind: AgentRunCommandKind::ProjectAgentStart,
            client_command_id: request.client_command_id,
            request_digest: request.request_digest,
            status: AgentRunCommandStatus::Pending,
            mailbox_message_id: None,
            accepted_refs: None,
            result_json: None,
            error_message: None,
            created_at: now,
            updated_at: now,
            accepted_at: None,
            failed_at: None,
        };
        let receipt_id = receipt.id;
        receipts.push(receipt);
        Ok(AgentRunMessageSubmissionReservation::Created { receipt_id })
    }

    async fn abandon_project_agent_start(
        &self,
        receipt_id: Uuid,
    ) -> Result<bool, WorkflowApplicationError> {
        let mut receipts = self.receipts.lock().expect("receipts lock");
        let before = receipts.len();
        receipts.retain(|receipt| {
            receipt.id != receipt_id || receipt.status != AgentRunCommandStatus::Pending
        });
        Ok(receipts.len() != before)
    }

    async fn fail_project_agent_start(
        &self,
        receipt_id: Uuid,
        error_message: String,
    ) -> Result<AgentRunCommandReceipt, WorkflowApplicationError> {
        let mut receipts = self.receipts.lock().expect("receipts lock");
        let receipt = receipts
            .iter_mut()
            .find(|receipt| receipt.id == receipt_id)
            .ok_or_else(|| WorkflowApplicationError::NotFound(receipt_id.to_string()))?;
        let now = Utc::now();
        receipt.status = AgentRunCommandStatus::TerminalFailed;
        receipt.error_message = Some(error_message);
        receipt.failed_at = Some(now);
        receipt.updated_at = now;
        Ok(receipt.clone())
    }
}

#[derive(Debug, Clone, Copy)]
enum InitialSubmissionBehavior {
    Accepted,
    FailUnattached,
    FailAttached,
    FailUnknown,
}

struct FixtureInitialSubmission {
    receipts: Arc<FixtureReceipts>,
    behavior: InitialSubmissionBehavior,
    calls: AtomicUsize,
}

#[async_trait::async_trait]
impl ProjectAgentInitialMessageSubmissionPort for FixtureInitialSubmission {
    async fn submit_initial_message(
        &self,
        command: ProjectAgentInitialMessageSubmission,
        _projector: Arc<dyn AgentRunMessageProductResultProjector>,
    ) -> Result<AgentRunMessageSubmissionResult, AgentRunMessageSubmissionFailure> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        match self.behavior {
            InitialSubmissionBehavior::Accepted => {
                let result_json = json!({ "outcome": "launched" });
                self.receipts
                    .accept(command.reserved_receipt_id, result_json.clone())
                    .map_err(|source| AgentRunMessageSubmissionFailure {
                        ownership: AgentRunMessageSubmissionOwnership::Unknown {
                            reserved_receipt_id: command.reserved_receipt_id,
                        },
                        source,
                    })?;
                Ok(AgentRunMessageSubmissionResult {
                    receipt_id: command.reserved_receipt_id,
                    result_json,
                    error_message: None,
                    duplicate: false,
                })
            }
            InitialSubmissionBehavior::FailUnattached => Err(AgentRunMessageSubmissionFailure {
                ownership: AgentRunMessageSubmissionOwnership::Unattached {
                    reserved_receipt_id: command.reserved_receipt_id,
                },
                source: WorkflowApplicationError::Internal(
                    "initial submission attach failed".to_string(),
                ),
            }),
            InitialSubmissionBehavior::FailAttached => Err(AgentRunMessageSubmissionFailure {
                ownership: AgentRunMessageSubmissionOwnership::Attached {
                    receipt_id: command.reserved_receipt_id,
                },
                source: WorkflowApplicationError::Internal(
                    "attached submission reconciliation required".to_string(),
                ),
            }),
            InitialSubmissionBehavior::FailUnknown => Err(AgentRunMessageSubmissionFailure {
                ownership: AgentRunMessageSubmissionOwnership::Unknown {
                    reserved_receipt_id: command.reserved_receipt_id,
                },
                source: WorkflowApplicationError::Internal(
                    "submission ownership unavailable".to_string(),
                ),
            }),
        }
    }
}

struct FixtureProjector;

impl AgentRunMessageProductResultProjector for FixtureProjector {
    fn accepted_result(
        &self,
        kind: AgentRunAcceptedProductResultKind,
    ) -> Result<Value, WorkflowApplicationError> {
        Ok(json!({ "accepted": format!("{kind:?}") }))
    }

    fn failed_result(&self) -> Result<Value, WorkflowApplicationError> {
        Ok(json!({ "outcome": "failed" }))
    }

    fn queued_result(
        &self,
        _message: &AgentRunMailboxMessage,
    ) -> Result<Value, WorkflowApplicationError> {
        Ok(json!({ "outcome": "queued" }))
    }
}

struct Harness {
    project_id: Uuid,
    project_agent_id: Uuid,
    graph: Arc<FixtureGraph>,
    launch: Arc<RecordingLaunch>,
    receipts: Arc<FixtureReceipts>,
    initial_submission: Arc<FixtureInitialSubmission>,
    service: ProjectAgentRunStartService,
}

impl Harness {
    async fn new(behavior: InitialSubmissionBehavior) -> Self {
        let project_id = Uuid::new_v4();
        let mut project_agent = ProjectAgent::new(project_id, "agent", "PI_AGENT");
        project_agent.config = json!({
            "provider_id": "openai",
            "model_id": "gpt-5"
        });
        project_agent.default_lifecycle_key = Some("agent-default".to_string());
        let project_agent_id = project_agent.id;
        let project_agents = Arc::new(MemoryProjectAgentRepository::default());
        project_agents
            .create(&project_agent)
            .await
            .expect("create project agent");
        let graph = Arc::new(FixtureGraph::default());
        let launch = Arc::new(RecordingLaunch::new(graph.clone()));
        let receipts = Arc::new(FixtureReceipts::default());
        let initial_submission = Arc::new(FixtureInitialSubmission {
            receipts: receipts.clone(),
            behavior,
            calls: AtomicUsize::new(0),
        });
        let projection = Arc::new(
            |_context: ProjectAgentRunStartProjectionContext| -> Result<
                Arc<dyn AgentRunMessageProductResultProjector>,
                WorkflowApplicationError,
            > { Ok(Arc::new(FixtureProjector)) },
        );
        let service = ProjectAgentRunStartService::new(ProjectAgentRunStartDeps {
            project_agents,
            lifecycle_runs: graph.clone(),
            frames: graph.clone(),
            lifecycle_launch: launch.clone(),
            receipts: receipts.clone(),
            initial_submission: initial_submission.clone(),
            execution_profiles: Arc::new(|_profile_id: &str| true),
            projection,
        });
        Self {
            project_id,
            project_agent_id,
            graph,
            launch,
            receipts,
            initial_submission,
            service,
        }
    }

    fn command(&self, client_command_id: &str, text: &str) -> ProjectAgentRunStartCommand {
        ProjectAgentRunStartCommand {
            project_id: self.project_id,
            project_agent_id: self.project_agent_id,
            input: agentdash_agent_protocol::text_user_input_blocks(text),
            client_command_id: client_command_id.to_string(),
            executor_config: None,
            backend_selection: None,
            subject_ref: None,
            identity: None,
        }
    }
}

#[tokio::test]
async fn whitespace_input_stops_before_reservation_and_launch_with_zero_graph() {
    let harness = Harness::new(InitialSubmissionBehavior::Accepted).await;

    let error = harness
        .service
        .start_run(harness.command("cmd-whitespace", " \n\t "))
        .await
        .expect_err("whitespace input must be rejected");

    assert!(matches!(error, WorkflowApplicationError::BadRequest(_)));
    assert_eq!(harness.receipts.count(), 0);
    assert_eq!(harness.launch.calls.load(Ordering::SeqCst), 0);
    assert_eq!(harness.initial_submission.calls.load(Ordering::SeqCst), 0);
    assert_eq!(harness.graph.counts(), (0, 0, 0));
}

#[tokio::test]
async fn duplicate_replays_without_second_launch_or_graph_and_uses_default_lifecycle_key() {
    let harness = Harness::new(InitialSubmissionBehavior::Accepted).await;
    let command = harness.command("cmd-duplicate", "hello");

    let first = harness
        .service
        .start_run(command.clone())
        .await
        .expect("first start");
    let duplicate = harness
        .service
        .start_run(command)
        .await
        .expect("duplicate replay");

    assert!(!first.duplicate);
    assert!(duplicate.duplicate);
    assert_eq!(first.result_json, duplicate.result_json);
    assert_eq!(harness.launch.calls.load(Ordering::SeqCst), 1);
    assert_eq!(harness.initial_submission.calls.load(Ordering::SeqCst), 1);
    assert_eq!(harness.graph.counts(), (1, 1, 1));
    match harness
        .launch
        .workflow_graph_ref
        .lock()
        .expect("workflow graph ref lock")
        .as_ref()
        .expect("default workflow graph ref")
    {
        WorkflowGraphRef::ByKey { project_id, key } => {
            assert_eq!(*project_id, harness.project_id);
            assert_eq!(key, "agent-default");
        }
        WorkflowGraphRef::ById(id) => panic!("unexpected workflow graph id {id}"),
    }
}

#[tokio::test]
async fn unattached_failure_deletes_entire_eventless_draft_graph_and_freezes_receipt() {
    let harness = Harness::new(InitialSubmissionBehavior::FailUnattached).await;

    let error = harness
        .service
        .start_run(harness.command("cmd-cleanup", "hello"))
        .await
        .expect_err("unattached initial submission must fail");

    assert_eq!(error.to_string(), "initial submission attach failed");
    assert_eq!(harness.launch.calls.load(Ordering::SeqCst), 1);
    assert_eq!(harness.initial_submission.calls.load(Ordering::SeqCst), 1);
    assert_eq!(harness.graph.counts(), (0, 0, 0));
    let receipts = harness.receipts.receipts.lock().expect("receipts lock");
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].status, AgentRunCommandStatus::TerminalFailed);
    assert_eq!(
        receipts[0].error_message.as_deref(),
        Some("initial submission attach failed")
    );
}

#[tokio::test]
async fn duplicate_of_unattached_failure_returns_same_error_without_relaunch() {
    let harness = Harness::new(InitialSubmissionBehavior::FailUnattached).await;
    let command = harness.command("cmd-stable-failure", "hello");

    let first = harness
        .service
        .start_run(command.clone())
        .await
        .expect_err("first start must fail");
    let duplicate = harness
        .service
        .start_run(command)
        .await
        .expect_err("duplicate must replay failure");

    assert_eq!(first.to_string(), duplicate.to_string());
    assert_eq!(harness.launch.calls.load(Ordering::SeqCst), 1);
    assert_eq!(harness.initial_submission.calls.load(Ordering::SeqCst), 1);
    assert_eq!(harness.graph.counts(), (0, 0, 0));
}

#[tokio::test]
async fn attached_or_unknown_submission_failure_never_deletes_the_launched_graph() {
    for behavior in [
        InitialSubmissionBehavior::FailAttached,
        InitialSubmissionBehavior::FailUnknown,
    ] {
        let harness = Harness::new(behavior).await;

        harness
            .service
            .start_run(harness.command(&format!("cmd-{behavior:?}"), "hello"))
            .await
            .expect_err("submission failure must surface");

        assert_eq!(harness.launch.calls.load(Ordering::SeqCst), 1);
        assert_eq!(harness.initial_submission.calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            harness.graph.counts(),
            (1, 1, 1),
            "{behavior:?} must leave reconciliation-owned graph intact"
        );
    }
}

#[test]
fn project_start_owner_does_not_import_mailbox_draft_contracts() {
    let source = include_str!("../src/agent_run/project_agent_start.rs");
    for forbidden in [
        "EnqueueRuntimeMailboxMessage",
        "NewAgentRunMailboxMessage",
        "MailboxMessageOrigin",
        "MailboxSourceIdentity",
        "mailbox_message_id",
        ".prepare_message(",
        "RuntimeInput",
        "runtime_input",
        "RuntimeActor",
        "AgentRunPresentationDraft",
        "LaunchPresentationSource",
        "acceptance_results",
    ] {
        assert!(
            !source.contains(forbidden),
            "ProjectAgentRunStartService crossed the submission boundary via {forbidden}"
        );
    }
}
