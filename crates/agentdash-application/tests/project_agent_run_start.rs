use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use agentdash_agent_runtime_contract::{
    ManagedRuntimeOperationReceipt, ManagedRuntimeOperationStatus,
    ManagedRuntimeSourceBindingEvidence, RuntimeOperationId, RuntimeProjectionRevision,
    RuntimeSourceRef, SurfaceRevision,
};
use agentdash_agent_service_api::AgentInputContent;
use agentdash_application::project_agent_run_start::{
    ProjectAgentRunStartCommand, ProjectAgentRunStartDeps, ProjectAgentRunStartService,
};
use agentdash_application_agentrun::agent_run::{
    AgentRunProductInputDelivery, AgentRunProductInputDeliveryError,
    AgentRunProductInputDeliveryPort, AgentRunProductInputPreparation, AgentRunProductLaunchError,
    AgentRunProductLaunchOutcome, AgentRunProductLaunchPort, AgentRunProductLaunchRequest,
    AgentRunProductRuntimeBinding, DeliverAgentRunProductInput,
    PreparedAgentRunProductInputDelivery,
};
use agentdash_application_ports::agent_frame_materialization::{
    AgentFrameWriteRole, AgentRunFrameConstructionPort, AgentRunFrameSurfaceCommandOutcome,
    AgentRunFrameSurfaceError, FrameConstructionCommand,
};
use agentdash_domain::{
    DomainError,
    agent::{ProjectAgent, ProjectAgentRepository},
    common::AgentConfig,
    workflow::{
        AgentFrame, AgentFrameRepository, AgentRunAcceptedRefs, AgentRunCommandClaim,
        AgentRunCommandReceipt, AgentRunCommandReceiptRepository, AgentRunCommandStatus,
        LifecycleAgentRepository, LifecycleRunRepository, NewAgentRunCommandReceipt,
    },
};
use agentdash_platform_spi::{AuthIdentity, AuthMode};
use agentdash_test_support::workflow::{
    MemoryAgentFrameRepository, MemoryAgentLineageRepository, MemoryLifecycleAgentRepository,
    MemoryLifecycleGateRepository, MemoryLifecycleRunRepository,
    MemoryLifecycleSubjectAssociationRepository, MemoryProjectAgentRepository,
    MemoryWorkflowGraphRepository,
};
use async_trait::async_trait;
use chrono::Utc;
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Default)]
struct RecordingReceipts {
    by_identity: Mutex<BTreeMap<(String, String, String), AgentRunCommandReceipt>>,
    by_id: Mutex<BTreeMap<Uuid, (String, String, String)>>,
}

#[async_trait]
impl AgentRunCommandReceiptRepository for RecordingReceipts {
    async fn claim(
        &self,
        receipt: NewAgentRunCommandReceipt,
    ) -> Result<AgentRunCommandClaim, DomainError> {
        let key = (
            receipt.scope_kind.clone(),
            receipt.scope_key.clone(),
            receipt.client_command_id.clone(),
        );
        let mut values = self.by_identity.lock().await;
        if let Some(existing) = values.get(&key) {
            if existing.command_kind != receipt.command_kind
                || existing.request_digest != receipt.request_digest
            {
                return Err(DomainError::Conflict {
                    entity: "agent_run_command_receipt",
                    constraint: "request_digest",
                    message: "client command identity was reused for another request".to_owned(),
                });
            }
            return Ok(AgentRunCommandClaim::Duplicate(existing.clone()));
        }
        let now = Utc::now();
        let stored = AgentRunCommandReceipt {
            id: Uuid::new_v4(),
            scope_kind: receipt.scope_kind,
            scope_key: receipt.scope_key,
            command_kind: receipt.command_kind,
            client_command_id: receipt.client_command_id,
            request_digest: receipt.request_digest,
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
        self.by_id.lock().await.insert(stored.id, key.clone());
        values.insert(key, stored.clone());
        Ok(AgentRunCommandClaim::Created(stored))
    }

    async fn mark_accepted(
        &self,
        id: Uuid,
        accepted_refs: AgentRunAcceptedRefs,
    ) -> Result<AgentRunCommandReceipt, DomainError> {
        self.mutate(id, |receipt| {
            receipt.status = AgentRunCommandStatus::Accepted;
            receipt.accepted_refs = Some(accepted_refs);
            receipt.accepted_at = Some(Utc::now());
        })
        .await
    }

    async fn attach_mailbox_message(
        &self,
        id: Uuid,
        mailbox_message_id: Uuid,
    ) -> Result<AgentRunCommandReceipt, DomainError> {
        self.mutate(id, |receipt| {
            receipt.mailbox_message_id = Some(mailbox_message_id);
        })
        .await
    }

    async fn store_result_json(
        &self,
        id: Uuid,
        result_json: serde_json::Value,
    ) -> Result<AgentRunCommandReceipt, DomainError> {
        self.mutate(id, |receipt| receipt.result_json = Some(result_json))
            .await
    }

    async fn accept_with_result(
        &self,
        id: Uuid,
        accepted_refs: AgentRunAcceptedRefs,
        result_json: serde_json::Value,
    ) -> Result<AgentRunCommandReceipt, DomainError> {
        self.mutate(id, |receipt| {
            receipt.status = AgentRunCommandStatus::Accepted;
            receipt.accepted_refs = Some(accepted_refs);
            receipt.result_json = Some(result_json);
            receipt.accepted_at.get_or_insert_with(Utc::now);
        })
        .await
    }

    async fn mark_terminal_failed(
        &self,
        id: Uuid,
        error_message: String,
    ) -> Result<AgentRunCommandReceipt, DomainError> {
        self.mutate(id, |receipt| {
            receipt.status = AgentRunCommandStatus::TerminalFailed;
            receipt.error_message = Some(error_message);
            receipt.failed_at = Some(Utc::now());
        })
        .await
    }

    async fn get(&self, id: Uuid) -> Result<Option<AgentRunCommandReceipt>, DomainError> {
        let key = self.by_id.lock().await.get(&id).cloned();
        let Some(key) = key else {
            return Ok(None);
        };
        Ok(self.by_identity.lock().await.get(&key).cloned())
    }
}

impl RecordingReceipts {
    async fn mutate(
        &self,
        id: Uuid,
        mutate: impl FnOnce(&mut AgentRunCommandReceipt),
    ) -> Result<AgentRunCommandReceipt, DomainError> {
        let key =
            self.by_id
                .lock()
                .await
                .get(&id)
                .cloned()
                .ok_or_else(|| DomainError::NotFound {
                    entity: "agent_run_command_receipt",
                    id: id.to_string(),
                })?;
        let mut values = self.by_identity.lock().await;
        let receipt = values
            .get_mut(&key)
            .expect("receipt id index and identity map stay aligned");
        mutate(receipt);
        receipt.updated_at = Utc::now();
        Ok(receipt.clone())
    }
}

struct StableFrameConstruction {
    frames: Arc<MemoryAgentFrameRepository>,
    commands: Mutex<Vec<FrameConstructionCommand>>,
}

#[async_trait]
impl AgentRunFrameConstructionPort for StableFrameConstruction {
    async fn execute_frame_construction_command(
        &self,
        command: FrameConstructionCommand,
    ) -> Result<AgentRunFrameSurfaceCommandOutcome, AgentRunFrameSurfaceError> {
        self.commands.lock().await.push(command.clone());
        let FrameConstructionCommand::DispatchLaunchAnchor {
            agent_id,
            target_frame_id: Some(frame_id),
            runtime_thread_id,
            created_by_id,
            execution_profile,
            ..
        } = command
        else {
            panic!("direct start must preallocate its frame identity")
        };
        if self
            .frames
            .get(frame_id)
            .await
            .map_err(|error| AgentRunFrameSurfaceError::ConstructionRejected {
                message: error.to_string(),
            })?
            .is_none()
        {
            let mut frame =
                AgentFrame::new_revision_with_id(frame_id, agent_id, 1, "dispatch_launch_anchor");
            frame.created_by_id = created_by_id;
            frame.execution_profile_json = execution_profile;
            self.frames.create(&frame).await.map_err(|error| {
                AgentRunFrameSurfaceError::ConstructionRejected {
                    message: error.to_string(),
                }
            })?;
        }
        let mut outcome =
            AgentRunFrameSurfaceCommandOutcome::new(AgentFrameWriteRole::FrameConstruction);
        outcome.frame_id = Some(frame_id);
        outcome.agent_id = Some(agent_id);
        outcome.runtime_thread_id = runtime_thread_id;
        outcome.wrote_frame_revision = true;
        Ok(outcome)
    }
}

struct RecordingLaunch {
    fail_first: AtomicBool,
    requests: Mutex<Vec<AgentRunProductLaunchRequest>>,
}

#[async_trait]
impl AgentRunProductLaunchPort for RecordingLaunch {
    async fn launch(
        &self,
        request: AgentRunProductLaunchRequest,
    ) -> Result<AgentRunProductLaunchOutcome, AgentRunProductLaunchError> {
        self.requests.lock().await.push(request.clone());
        if self.fail_first.swap(false, Ordering::SeqCst) {
            return Err(AgentRunProductLaunchError::ResourceSurface(
                "injected unknown launch outcome".to_owned(),
            ));
        }
        let source_binding = ManagedRuntimeSourceBindingEvidence {
            source_ref: RuntimeSourceRef::new(format!(
                "source:{}",
                request.provisioning.runtime_thread_id
            ))
            .expect("source"),
            committed_at_revision: RuntimeProjectionRevision(1),
            applied_surface_revision: SurfaceRevision(
                request.provisioning.surface_facts.surface_revision,
            ),
            activated_at_revision: Some(RuntimeProjectionRevision(2)),
        };
        let binding = AgentRunProductRuntimeBinding {
            target: request.provisioning.target.clone(),
            runtime_thread_id: request.provisioning.runtime_thread_id.clone(),
            launch_frame: request.provisioning.frame.clone(),
            execution_profile_digest: request
                .provisioning
                .execution_profile
                .profile_digest
                .clone(),
            source_binding,
        };
        let receipt = |phase: &str, revision| ManagedRuntimeOperationReceipt {
            operation_id: RuntimeOperationId::new(format!(
                "{phase}:{}",
                request.provisioning.runtime_thread_id
            ))
            .expect("operation"),
            thread_id: request.provisioning.runtime_thread_id.clone(),
            accepted_revision: RuntimeProjectionRevision(revision),
            status: ManagedRuntimeOperationStatus::Succeeded,
            evidence: None,
            duplicate: false,
        };
        Ok(AgentRunProductLaunchOutcome {
            binding,
            resource_snapshot_revision: 1,
            create_receipt: receipt("create", 1),
            activate_receipt: receipt("activate", 2),
            input_receipt: None,
        })
    }
}

struct RecordingInput {
    fail_first: AtomicBool,
    commands: Mutex<Vec<DeliverAgentRunProductInput>>,
}

#[async_trait]
impl AgentRunProductInputDeliveryPort for RecordingInput {
    async fn prepare_delivery(
        &self,
        _command: DeliverAgentRunProductInput,
    ) -> Result<AgentRunProductInputPreparation, AgentRunProductInputDeliveryError> {
        unreachable!("fixture overrides deliver")
    }

    async fn dispatch_prepared(
        &self,
        _prepared: PreparedAgentRunProductInputDelivery,
    ) -> Result<AgentRunProductInputDelivery, AgentRunProductInputDeliveryError> {
        unreachable!("fixture overrides deliver")
    }

    async fn deliver(
        &self,
        command: DeliverAgentRunProductInput,
    ) -> Result<AgentRunProductInputDelivery, AgentRunProductInputDeliveryError> {
        self.commands.lock().await.push(command.clone());
        if self.fail_first.swap(false, Ordering::SeqCst) {
            return Err(AgentRunProductInputDeliveryError::Command(
                "injected lost input receipt".to_owned(),
            ));
        }
        let thread_id = agentdash_agent_runtime_contract::RuntimeThreadId::new(format!(
            "thread:{}:{}",
            command.target.run_id, command.target.agent_id
        ))
        .expect("thread");
        Ok(AgentRunProductInputDelivery {
            mailbox_message_id: stable_test_uuid(command.target.run_id, &command.client_command_id),
            operation_receipt: Some(ManagedRuntimeOperationReceipt {
                operation_id: RuntimeOperationId::new(format!(
                    "input:{}:{}",
                    command.target.agent_id, command.client_command_id
                ))
                .expect("operation"),
                thread_id,
                accepted_revision: RuntimeProjectionRevision(3),
                status: ManagedRuntimeOperationStatus::Succeeded,
                evidence: None,
                duplicate: false,
            }),
            queued: false,
        })
    }

    async fn record_dispatched(
        &self,
        _command: DeliverAgentRunProductInput,
    ) -> Result<Uuid, AgentRunProductInputDeliveryError> {
        unreachable!("fixture does not record pre-dispatched input")
    }
}

struct Fixture {
    service: ProjectAgentRunStartService,
    runs: Arc<MemoryLifecycleRunRepository>,
    agents: Arc<MemoryLifecycleAgentRepository>,
    frames: Arc<MemoryAgentFrameRepository>,
    launch: Arc<RecordingLaunch>,
    input: Arc<RecordingInput>,
    project_id: Uuid,
    project_agent_id: Uuid,
}

impl Fixture {
    async fn new(fail_launch_once: bool, fail_input_once: bool) -> Self {
        let project_id = Uuid::new_v4();
        let mut project_agent = ProjectAgent::new(project_id, "reviewer", "PI_AGENT");
        project_agent.config = serde_json::json!({
            "display_name": "Reviewer",
            "description": "Review changes"
        });
        let project_agent_id = project_agent.id;
        let project_agents = Arc::new(MemoryProjectAgentRepository::default());
        project_agents
            .create(&project_agent)
            .await
            .expect("project agent");
        let runs = Arc::new(MemoryLifecycleRunRepository::default());
        let agents = Arc::new(MemoryLifecycleAgentRepository::default());
        let frames = Arc::new(MemoryAgentFrameRepository::default());
        let frame_construction = Arc::new(StableFrameConstruction {
            frames: frames.clone(),
            commands: Mutex::new(Vec::new()),
        });
        let launch = Arc::new(RecordingLaunch {
            fail_first: AtomicBool::new(fail_launch_once),
            requests: Mutex::new(Vec::new()),
        });
        let input = Arc::new(RecordingInput {
            fail_first: AtomicBool::new(fail_input_once),
            commands: Mutex::new(Vec::new()),
        });
        let service = ProjectAgentRunStartService::new(ProjectAgentRunStartDeps {
            project_agents,
            lifecycle_runs: runs.clone(),
            workflow_graphs: Arc::new(MemoryWorkflowGraphRepository::default()),
            lifecycle_agents: agents.clone(),
            frames: frames.clone(),
            subject_associations: Arc::new(MemoryLifecycleSubjectAssociationRepository::default()),
            lifecycle_gates: Arc::new(MemoryLifecycleGateRepository::default()),
            agent_lineage: Arc::new(MemoryAgentLineageRepository::default()),
            receipts: Arc::new(RecordingReceipts::default()),
            frame_construction,
            product_launch: launch.clone(),
            product_input: input.clone(),
        });
        Self {
            service,
            runs,
            agents,
            frames,
            launch,
            input,
            project_id,
            project_agent_id,
        }
    }

    fn command(&self, client_command_id: &str, text: &str) -> ProjectAgentRunStartCommand {
        let mut executor_config = AgentConfig::new("PI_AGENT");
        executor_config.provider_id = Some("test-provider".to_owned());
        executor_config.model_id = Some("test-model".to_owned());
        ProjectAgentRunStartCommand {
            project_id: self.project_id,
            project_agent_id: self.project_agent_id,
            input: vec![AgentInputContent::Text {
                text: text.to_owned(),
            }],
            client_command_id: client_command_id.to_owned(),
            executor_config: Some(executor_config),
            backend_selection: None,
            subject_ref: None,
            identity: identity(),
        }
    }
}

#[tokio::test]
async fn launch_unknown_outcome_retries_same_product_graph_and_replays_frozen_result() {
    let fixture = Fixture::new(true, false).await;
    let command = fixture.command("client-1", "review this");
    fixture
        .service
        .start(command.clone())
        .await
        .expect_err("first launch outcome is lost");

    let recovered = fixture
        .service
        .start(command.clone())
        .await
        .expect("retry converges the same launch");
    assert!(recovered.duplicate);
    assert_eq!(
        fixture
            .runs
            .list_by_project(fixture.project_id)
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        fixture
            .agents
            .list_by_run(recovered.outcome.run_id)
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        fixture
            .frames
            .list_by_agent(recovered.outcome.agent_id)
            .await
            .unwrap()
            .len(),
        1
    );
    let launch_requests = fixture.launch.requests.lock().await;
    assert_eq!(launch_requests.len(), 2);
    assert_eq!(
        launch_requests[0].provisioning,
        launch_requests[1].provisioning
    );
    drop(launch_requests);

    let replay = fixture
        .service
        .start(command)
        .await
        .expect("terminal replay");
    assert!(replay.duplicate);
    assert_eq!(replay.outcome, recovered.outcome);
    assert_eq!(fixture.launch.requests.lock().await.len(), 2);
    assert_eq!(fixture.input.commands.lock().await.len(), 1);
}

#[tokio::test]
async fn lost_initial_input_receipt_retries_same_input_after_active_runtime_launch() {
    let fixture = Fixture::new(false, true).await;
    let command = fixture.command("client-2", "continue safely");
    fixture
        .service
        .start(command.clone())
        .await
        .expect_err("first input receipt is lost");

    let recovered = fixture
        .service
        .start(command)
        .await
        .expect("retry reconciles the same input identity");
    assert!(recovered.duplicate);
    let commands = fixture.input.commands.lock().await;
    assert_eq!(commands.len(), 2);
    assert_eq!(commands[0].target, commands[1].target);
    assert_eq!(commands[0].client_command_id, commands[1].client_command_id);
    assert_eq!(commands[0].content, commands[1].content);
    assert_eq!(fixture.launch.requests.lock().await.len(), 2);
}

#[tokio::test]
async fn client_command_id_reuse_with_another_semantic_request_is_rejected_before_second_graph() {
    let fixture = Fixture::new(false, false).await;
    fixture
        .service
        .start(fixture.command("client-3", "first"))
        .await
        .expect("first request");
    let error = fixture
        .service
        .start(fixture.command("client-3", "different"))
        .await
        .expect_err("digest conflict");
    assert!(matches!(
        error,
        agentdash_application::ApplicationError::Conflict(_)
    ));
    assert_eq!(fixture.launch.requests.lock().await.len(), 1);
    assert_eq!(fixture.input.commands.lock().await.len(), 1);
}

fn identity() -> AuthIdentity {
    AuthIdentity {
        auth_mode: AuthMode::Personal,
        user_id: "user-1".to_owned(),
        subject: "user-1".to_owned(),
        display_name: Some("User".to_owned()),
        email: None,
        avatar_url: None,
        groups: Vec::new(),
        is_admin: false,
        provider: None,
        extra: serde_json::Value::Null,
    }
}

fn stable_test_uuid(run_id: Uuid, key: &str) -> Uuid {
    let digest = Sha256::digest([run_id.as_bytes().as_slice(), key.as_bytes()].concat());
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    Uuid::from_bytes(bytes)
}
