use std::{collections::BTreeSet, str::FromStr, sync::Arc};

use agentdash_agent_runtime::{ManagedAgentRuntime, RuntimeStoreFixture};
use agentdash_agent_runtime_contract::*;
use agentdash_application_agentrun::agent_run::{
    AgentRunRuntime, AgentRunRuntimeError, ManagedAgentRunRuntime, SendAgentRunMessage,
};
use agentdash_application_ports::agent_run_runtime::*;
use async_trait::async_trait;
use tokio::sync::Mutex;
use uuid::Uuid;

fn id<T: FromStr>(value: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    value.parse().expect("valid id")
}

fn profile() -> RuntimeProfile {
    RuntimeProfile {
        reference_class: ReferenceRuntimeClass::ManagedThread,
        input: InputProfile {
            modalities: [InputModality::Text].into(),
        },
        instruction: InstructionProfile {
            channels: BTreeSet::new(),
            configuration_boundary: ConfigurationBoundary::Binding,
        },
        tools: ToolProfile {
            channels: BTreeSet::new(),
            configuration_boundary: ConfigurationBoundary::Binding,
            cancellation: true,
        },
        workspace: WorkspaceProfile {
            capabilities: BTreeSet::new(),
            mechanism: DeliveryMechanism::Native,
        },
        interactions: InteractionProfile {
            kinds: BTreeSet::new(),
            durable_correlation: true,
        },
        lifecycle: [
            LifecycleCapability::ThreadStart,
            LifecycleCapability::TurnStart,
            LifecycleCapability::TurnSteer,
            LifecycleCapability::TurnInterrupt,
        ]
        .into(),
        hooks: HookProfile {
            points: Vec::new(),
            configuration_boundary: ConfigurationBoundary::Binding,
        },
        context: ContextProfile {
            capabilities: BTreeSet::new(),
            fidelity: ContextFidelity::Opaque,
            activation_idempotent: false,
        },
        telemetry_config: BTreeSet::new(),
    }
}

#[derive(Default)]
struct CompositionFixture {
    binding: Mutex<Option<AgentRunRuntimeBinding>>,
    provisions: Mutex<usize>,
    backend_selection: Mutex<Option<agentdash_application_ports::launch::BackendSelectionInput>>,
}

impl CompositionFixture {
    async fn provision_count(&self) -> usize {
        *self.provisions.lock().await
    }
}

#[async_trait]
impl AgentRunRuntimeBindingRepository for CompositionFixture {
    async fn load(
        &self,
        target: &AgentRunRuntimeTarget,
    ) -> Result<Option<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError> {
        Ok(self
            .binding
            .lock()
            .await
            .clone()
            .filter(|binding| &binding.target == target))
    }

    async fn load_by_thread_id(
        &self,
        thread_id: &RuntimeThreadId,
    ) -> Result<Option<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError> {
        Ok(self
            .binding
            .lock()
            .await
            .clone()
            .filter(|binding| &binding.thread_id == thread_id))
    }

    async fn list_by_run(
        &self,
        run_id: Uuid,
    ) -> Result<Vec<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError> {
        Ok(self
            .binding
            .lock()
            .await
            .clone()
            .into_iter()
            .filter(|binding| binding.target.run_id == run_id)
            .collect())
    }

    async fn list_by_agent(
        &self,
        agent_id: Uuid,
    ) -> Result<Vec<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError> {
        Ok(self
            .binding
            .lock()
            .await
            .clone()
            .into_iter()
            .filter(|binding| binding.target.agent_id == agent_id)
            .collect())
    }

    async fn insert(
        &self,
        binding: AgentRunRuntimeBinding,
    ) -> Result<AgentRunRuntimeBinding, AgentRunRuntimeBindingError> {
        let mut current = self.binding.lock().await;
        match current.as_ref() {
            Some(existing) if existing != &binding => Err(AgentRunRuntimeBindingError::Conflict),
            Some(existing) => Ok(existing.clone()),
            None => {
                *current = Some(binding.clone());
                Ok(binding)
            }
        }
    }
}

#[async_trait]
impl AgentRunRuntimeProvisioner for CompositionFixture {
    async fn provision(
        &self,
        request: &AgentRunRuntimeProvisionRequest,
    ) -> Result<AgentRunRuntimeBinding, AgentRunRuntimeBindingError> {
        *self.provisions.lock().await += 1;
        *self.backend_selection.lock().await = request.backend_selection.clone();
        self.insert(AgentRunRuntimeBinding {
            target: request.target.clone(),
            thread_id: id("thread-facade"),
            binding_id: id("binding-facade"),
            driver_generation: RuntimeDriverGeneration(3),
            source_thread_id: id("source-thread-facade"),
            profile_digest: id("profile-facade"),
            profile_provenance: ProfileProvenance {
                service_digest: id("profile-service-facade"),
                transport_digest: id("profile-transport-facade"),
                host_policy_digest: id("profile-host-facade"),
            },
            bound_profile: profile(),
            surface_digest: id("surface-facade"),
            settings_revision: ThreadSettingsRevision(0),
            tool_set_revision: ToolSetRevision(0),
            hook_plan: BoundRuntimeHookPlan {
                revision: HookPlanRevision(1),
                digest: id("hook-plan-facade"),
                entries: Vec::new(),
            },
        })
        .await
    }
}

fn target() -> AgentRunRuntimeTarget {
    AgentRunRuntimeTarget {
        run_id: Uuid::parse_str("11111111-1111-1111-1111-111111111111").expect("run id"),
        agent_id: Uuid::parse_str("22222222-2222-2222-2222-222222222222").expect("agent id"),
    }
}

fn send(text: &str) -> SendAgentRunMessage {
    SendAgentRunMessage {
        target: target(),
        client_command_id: "client-command-1".to_string(),
        input: vec![RuntimeInput::Text {
            text: text.to_string(),
        }],
        actor: RuntimeActor::User {
            subject: "subject-1".to_string(),
        },
        identity: None,
        backend_selection: None,
    }
}

#[tokio::test]
async fn first_send_provisions_once_and_retry_replays_the_original_thread_start() {
    let store = Arc::new(RuntimeStoreFixture::default());
    let gateway: Arc<dyn AgentRuntimeGateway> = Arc::new(ManagedAgentRuntime::new(store));
    let composition = Arc::new(CompositionFixture::default());
    let facade = ManagedAgentRunRuntime::new(gateway, composition.clone(), composition.clone());

    let accepted = facade.send_message(send("hello")).await.expect("send");
    assert!(!accepted.duplicate);
    let replayed = facade.send_message(send("hello")).await.expect("retry");
    assert!(replayed.duplicate);
    assert_eq!(replayed.operation_id, accepted.operation_id);
    assert_eq!(composition.provision_count().await, 1);

    let view = facade.inspect(target()).await.expect("inspect");
    assert_eq!(
        view.snapshot.expect("snapshot").thread_id,
        id("thread-facade")
    );

    let conflict = facade
        .send_message(send("different"))
        .await
        .expect_err("client command identity cannot be reused with another input");
    assert!(matches!(
        conflict,
        AgentRunRuntimeError::ClientCommandConflict
    ));
}

#[tokio::test]
async fn first_send_forwards_explicit_backend_selection_to_runtime_provisioning() {
    use agentdash_application_ports::launch::{BackendSelectionInput, BackendSelectionInputMode};

    let store = Arc::new(RuntimeStoreFixture::default());
    let gateway: Arc<dyn AgentRuntimeGateway> = Arc::new(ManagedAgentRuntime::new(store));
    let composition = Arc::new(CompositionFixture::default());
    let runtime = ManagedAgentRunRuntime::new(gateway, composition.clone(), composition.clone());
    let mut command = send("backend selected");
    command.backend_selection = Some(BackendSelectionInput {
        mode: BackendSelectionInputMode::Explicit,
        backend_id: Some("backend-local".to_string()),
    });

    runtime.send_message(command).await.expect("send succeeds");

    assert_eq!(
        composition.backend_selection.lock().await.clone(),
        Some(BackendSelectionInput {
            mode: BackendSelectionInputMode::Explicit,
            backend_id: Some("backend-local".to_string()),
        })
    );
}
