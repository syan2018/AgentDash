use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use agentdash_agent_runtime_contract::{
    ManagedRuntimeOperationReceipt, ManagedRuntimeOperationStatus, RuntimeOperationId,
    RuntimeProjectionRevision,
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
    agent::{ProjectAgent, ProjectAgentRepository},
    common::AgentConfig,
    workflow::{
        AgentFrame, AgentFrameRepository, LifecycleAgentRepository, LifecycleRunRepository,
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
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use uuid::Uuid;

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
            return Err(AgentRunProductLaunchError::Binding(
                "injected unknown launch outcome".to_owned(),
            ));
        }
        let binding = AgentRunProductRuntimeBinding {
            target: request.provisioning.target.clone(),
            runtime_thread_id: request.provisioning.runtime_thread_id.clone(),
            agent: agentdash_application_agentrun::agent_run::AgentRunCompleteAgentAssociation {
                service_instance_id: agentdash_agent_service_api::AgentServiceInstanceId::new(
                    "fixture-agent",
                )
                .unwrap(),
                source: agentdash_agent_service_api::AgentSourceCoordinate::new("fixture-source")
                    .unwrap(),
            },
            launch_frame: request.provisioning.frame.clone(),
            execution_profile_digest: request
                .provisioning
                .execution_profile
                .profile_digest
                .clone(),
            execution_profile: request.provisioning.execution_profile.clone(),
        };
        Ok(AgentRunProductLaunchOutcome {
            binding,
            create_receipt: agentdash_agent_service_api::AgentCommandReceipt {
                command_id: agentdash_agent_service_api::AgentCommandId::new("fixture-create")
                    .unwrap(),
                effect_id: agentdash_agent_service_api::AgentEffectIdentity::new("fixture-create")
                    .unwrap(),
                source: agentdash_agent_service_api::AgentSourceCoordinate::new("fixture-source")
                    .unwrap(),
                state: agentdash_agent_service_api::AgentReceiptState::Terminal {
                    outcome: agentdash_agent_service_api::AgentTerminalOutcome::Succeeded,
                },
                snapshot_revision: Some(agentdash_agent_service_api::AgentSnapshotRevision(1)),
                initial_context: None,
            },
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
        let mut commands = self.commands.lock().await;
        if commands.iter().any(|existing| {
            existing.target == command.target
                && existing.client_command_id == command.client_command_id
                && existing.content != command.content
        }) {
            return Err(AgentRunProductInputDeliveryError::Command(
                "concrete Agent rejected a reused effect identity with different input".to_owned(),
            ));
        }
        commands.push(command.clone());
        drop(commands);
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
            handoff_id: stable_test_uuid(command.target.run_id, &command.client_command_id),
            operation_receipt: ManagedRuntimeOperationReceipt {
                operation_id: RuntimeOperationId::new(format!(
                    "input:{}:{}",
                    command.target.agent_id, command.client_command_id
                ))
                .expect("operation"),
                thread_id,
                status: ManagedRuntimeOperationStatus::Succeeded,
                evidence: None,
                duplicate: false,
            },
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
async fn launch_unknown_outcome_retries_same_product_graph_and_agent_effects() {
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
    assert_eq!(fixture.launch.requests.lock().await.len(), 3);
    assert_eq!(fixture.input.commands.lock().await.len(), 2);
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
async fn client_command_id_reuse_with_another_input_is_rejected_by_the_agent_effect_owner() {
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
        .expect_err("Agent effect identity conflict");
    assert!(error.to_string().contains("reused effect identity"));
    assert_eq!(fixture.launch.requests.lock().await.len(), 2);
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
