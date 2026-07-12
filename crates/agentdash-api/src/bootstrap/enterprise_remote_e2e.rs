use std::{
    collections::{BTreeMap, BTreeSet},
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use agentdash_agent::{
    AgentMessage, BridgeRequest, BridgeResponse, ContentPart, LlmBridge, StopReason, StreamChunk,
    TokenUsage, ToolCallInfo,
};
use agentdash_agent_runtime::RuntimeWorkKind;
use agentdash_agent_runtime_contract::*;
use agentdash_agent_runtime_host::*;
use agentdash_agent_types::{
    AgentTool, AgentToolError, AgentToolResult, DynAgentTool, ToolUpdateCallback,
};
use agentdash_application_agentrun::agent_run::{
    AgentRunCommandGuard, AgentRunRuntime, EnqueueRuntimeMailboxMessage, GuardedAgentRunCommand,
    ManagedAgentRunRuntime, RuntimeAgentRunMailbox, RuntimeMailboxSubmitOutcome,
};
use agentdash_application_ports::agent_run_runtime::*;
use agentdash_application_ports::agent_run_surface::{
    AgentRunAdmissionDecision, AgentRunAdmissionRequest, AgentRunEffectiveCapabilityError,
    AgentRunEffectiveCapabilityPort, AgentRunEffectiveCapabilityRequest,
    AgentRunEffectiveCapabilityView,
};
use agentdash_domain::agent_run_mailbox::MailboxSourceIdentity;
use agentdash_infrastructure::{
    PostgresAgentRuntimeHostRepository, PostgresRuntimeRepository,
    postgres_runtime::PostgresRuntime,
};
use agentdash_integration_api::*;
use agentdash_integration_native_agent::{
    NativeAgentRuntimeIntegration, NativeBridgeResolveError, NativeBridgeResolver,
    native_runtime_profile,
};
use agentdash_integration_remote_runtime::{
    RuntimeWireHostPortRouter, remote_runtime_contribution,
};
use agentdash_local::{HostRuntimeDriverEndpointResolver, RuntimeWireCommandHandler};
use agentdash_relay::{CapabilitiesPayload, RelayMessage, RuntimeRelayTransportDescriptor};
use agentdash_test_support::workflow::MemoryAgentRunMailboxRepository;
use async_trait::async_trait;
use chrono::Utc;
use futures::{Stream, stream};
use serde_json::json;
use sqlx::PgPool;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::agent_runtime::{
    AgentRunRuntimeSurfaceSource, AgentRunRuntimeSurfaceSourceError, AgentRuntimeCompositionInput,
    PreparedAgentRunRuntime, build_agent_runtime_composition,
};
use super::agent_runtime_surface::{
    CompiledAgentRunToolBinding, CompiledAgentRunToolRegistry, PostgresAgentRunToolBrokerResolver,
};
use crate::relay::registry::{BackendRegistry, ConnectedBackend};
use crate::relay::{CloudRemoteRuntimeInventory, CloudRuntimeWirePlacementResolver};

const BACKEND_ID: &str = "enterprise-desktop";
const TRANSPORT_ID: &str = "enterprise-runtime-wire";

fn id<T: std::str::FromStr>(value: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    value.parse().expect("valid fixture id")
}

struct EnterpriseBridge {
    calls: AtomicUsize,
    block_next: std::sync::atomic::AtomicBool,
    blocked: tokio::sync::Semaphore,
    release: tokio::sync::Semaphore,
}

#[async_trait]
impl LlmBridge for EnterpriseBridge {
    async fn stream_complete(
        &self,
        _request: BridgeRequest,
    ) -> Pin<Box<dyn Stream<Item = StreamChunk> + Send>> {
        if self.block_next.swap(false, Ordering::SeqCst) {
            self.blocked.add_permits(1);
            self.release
                .acquire()
                .await
                .expect("Enterprise bridge release remains open")
                .forget();
        }
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        let message = if call.is_multiple_of(2) {
            AgentMessage::Assistant {
                content: Vec::new(),
                tool_calls: vec![ToolCallInfo {
                    id: format!("enterprise-tool-{call}"),
                    call_id: None,
                    name: "enterprise_echo".to_string(),
                    arguments: json!({"value":"through-runtime-wire"}),
                }],
                stop_reason: Some(StopReason::ToolUse),
                error_message: None,
                usage: None,
                timestamp: None,
            }
        } else {
            AgentMessage::Assistant {
                content: vec![ContentPart::text("enterprise remote completed")],
                tool_calls: Vec::new(),
                stop_reason: Some(StopReason::Stop),
                error_message: None,
                usage: None,
                timestamp: None,
            }
        };
        Box::pin(stream::iter(vec![StreamChunk::Done(BridgeResponse {
            raw_content: match &message {
                AgentMessage::Assistant { content, .. } => content.clone(),
                _ => Vec::new(),
            },
            message,
            usage: TokenUsage::default(),
        })]))
    }
}

struct EnterpriseBridgeResolver(Arc<EnterpriseBridge>);

#[async_trait]
impl NativeBridgeResolver for EnterpriseBridgeResolver {
    async fn resolve(
        &self,
        _instance: &ActivatedAgentServiceInstance,
        _host: &RuntimeDriverHostPorts,
    ) -> Result<Arc<dyn LlmBridge>, NativeBridgeResolveError> {
        Ok(self.0.clone())
    }
}

struct NoCredentials;

#[async_trait]
impl AgentRuntimeCredentialBroker for NoCredentials {
    async fn resolve(
        &self,
        slot: &AgentRuntimeCredentialSlot,
        _reference: &AgentRuntimeCredentialRef,
        _purpose: &str,
    ) -> Result<CredentialLease, CredentialResolveError> {
        Err(CredentialResolveError::Unavailable {
            slot: slot.clone(),
            reason: "fixture has no credentials".to_string(),
        })
    }
}

struct EnterpriseEchoTool(Arc<AtomicUsize>);

#[async_trait]
impl AgentTool for EnterpriseEchoTool {
    fn name(&self) -> &str {
        "enterprise_echo"
    }

    fn description(&self) -> &str {
        "Enterprise reverse RuntimeWire tool"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({"type":"object"})
    }

    async fn execute(
        &self,
        _tool_call_id: &str,
        args: serde_json::Value,
        _cancel: CancellationToken,
        _on_update: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(AgentToolResult {
            content: Vec::new(),
            is_error: false,
            details: Some(json!({"echoed": args})),
        })
    }
}

struct EnterpriseCapabilityPort;

#[async_trait]
impl AgentRunEffectiveCapabilityPort for EnterpriseCapabilityPort {
    async fn effective_capability(
        &self,
        _request: AgentRunEffectiveCapabilityRequest,
    ) -> Result<AgentRunEffectiveCapabilityView, AgentRunEffectiveCapabilityError> {
        Err(AgentRunEffectiveCapabilityError::Projection {
            message: "Enterprise E2E only exercises strict tool admission".to_string(),
        })
    }

    async fn admit_tool(
        &self,
        request: AgentRunAdmissionRequest,
    ) -> Result<AgentRunAdmissionDecision, AgentRunEffectiveCapabilityError> {
        assert_eq!(request.capability_key, "enterprise");
        assert_eq!(request.tool_name, "enterprise_echo");
        Ok(AgentRunAdmissionDecision::allow())
    }
}

#[derive(Default)]
struct RecordingHookCallback(AtomicUsize);

#[async_trait]
impl AgentRuntimeHookCallback for RecordingHookCallback {
    async fn execute(
        &self,
        request: DriverHookInvocation,
    ) -> Result<DriverHookDecision, DriverHookCallbackError> {
        assert_eq!(request.point, HookPoint::BeforeProviderRequest);
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(DriverHookDecision::Continue {
            payload: request.payload,
        })
    }
}

fn enterprise_contribution() -> (
    AgentRuntimeDriverContribution,
    AgentRuntimeTrustManifest,
    Arc<EnterpriseBridge>,
) {
    let bridge = Arc::new(EnterpriseBridge {
        calls: AtomicUsize::new(0),
        block_next: std::sync::atomic::AtomicBool::new(false),
        blocked: tokio::sync::Semaphore::new(0),
        release: tokio::sync::Semaphore::new(0),
    });
    let integration =
        NativeAgentRuntimeIntegration::new(Arc::new(EnterpriseBridgeResolver(bridge.clone())));
    let mut contribution = integration
        .agent_runtime_drivers()
        .pop()
        .expect("Native production driver contribution");
    contribution.definition.provenance = AgentServiceProvenance {
        definition_id: AgentServiceDefinitionId::new("enterprise.reference-agent")
            .expect("definition id"),
        publisher_integration: "enterprise.internal".to_string(),
        service_version: "1.0.0".to_string(),
        build_digest: AgentServiceBuildDigest::new("sha256:enterprise-reference-build")
            .expect("build digest"),
    };
    let manifest = AgentRuntimeTrustManifest {
        provenance: contribution.definition.provenance.clone(),
        suite_revision: "enterprise-runtime-conformance-v1".to_string(),
        driver_build_digest: contribution.definition.provenance.build_digest.to_string(),
        protocol_revision: 1,
        verified_profile: contribution.definition.service_profile_upper_bound.clone(),
    };
    (contribution, manifest, bridge)
}

fn trusted_manifest(manifest: &AgentRuntimeTrustManifest) -> TrustedDriverManifest {
    TrustedDriverManifest {
        provenance: manifest.provenance.clone(),
        suite_revision: manifest.suite_revision.clone(),
        driver_build_digest: manifest.driver_build_digest.clone(),
        protocol_revision: manifest.protocol_revision,
        verified_profile_digest: profile_digest(&manifest.verified_profile)
            .expect("verified profile digest"),
    }
}

fn remote_proxy_definition(source: &AgentServiceDefinition) -> AgentServiceDefinition {
    let schema = json!({
        "type":"object",
        "properties": {
            "sourceServiceInstanceId":{"type":"string","minLength":1},
            "sourceDriverGeneration":{"type":"integer","minimum":1},
            "sourceHostIncarnationId":{"type":"string","minLength":1}
        },
        "required":["sourceServiceInstanceId","sourceDriverGeneration","sourceHostIncarnationId"],
        "additionalProperties":false
    });
    let mut definition = source.clone();
    definition.factory_key =
        AgentRuntimeFactoryKey::new("enterprise.remote-runtime-proxy").expect("proxy factory key");
    definition.config_schema_digest =
        AgentServiceSchemaDigest::new(agent_service_schema_digest(&schema)).expect("schema digest");
    definition.config_schema = schema;
    definition.credential_slots.clear();
    definition
}

fn materialized_surface(thread_id: RuntimeThreadId) -> MaterializedDriverSurface {
    MaterializedDriverSurface {
        runtime_thread_id: thread_id,
        revision: SurfaceRevision(1),
        digest: id("sha256:enterprise-surface"),
        authorization_identity: None,
        context: DriverContextSurface {
            recipe: ContextRecipe {
                revision: ContextRecipeRevision(1),
                provenance: ContextProvenance {
                    settings_revision: ThreadSettingsRevision(1),
                    tool_set_revision: ToolSetRevision(1),
                },
                source_item_ids: Vec::new(),
            },
            instructions: vec![DriverInstructionSet {
                channel: InstructionChannel::System,
                entries: vec!["Enterprise remote Agent".to_string()],
            }],
            blocks: Vec::new(),
            digest: id("sha256:enterprise-context"),
            fidelity: ContextFidelity::PlatformExact,
        },
        tools: DriverToolSurface {
            revision: ToolSetRevision(1),
            digest: "sha256:enterprise-tools".to_string(),
            tools: vec![DriverToolDefinition {
                name: "enterprise_echo".to_string(),
                description: "Enterprise reverse RuntimeWire tool".to_string(),
                parameters_schema: json!({"type":"object"}),
                channels: vec![ToolChannel::DirectCallback],
            }],
        },
        hooks: DriverHookSurface {
            revision: HookPlanRevision(1),
            digest: id("sha256:enterprise-hooks"),
            artifact_digest: Some("sha256:enterprise-hook-artifact".to_string()),
            configuration_boundary: ConfigurationBoundary::Binding,
            bindings: vec![DriverHookBinding {
                definition_id: id("enterprise-before-provider"),
                point: HookPoint::BeforeProviderRequest,
                actions: vec![HookAction::Observe],
                strength: SemanticStrength::ExactSynchronous,
                failure_policy: HookFailurePolicy::FailClosed,
                required: true,
                site: HookExecutionSite::AgentCoreCallback,
            }],
        },
        workspace: DriverWorkspaceSurface {
            capabilities: Vec::new(),
            roots: vec!["workspace://enterprise".to_string()],
        },
    }
}

struct EnterpriseSurfaceSource {
    definition: AgentServiceDefinition,
    manifest: AgentRuntimeTrustManifest,
    tool_registry: Arc<CompiledAgentRunToolRegistry>,
    tool_calls: Arc<AtomicUsize>,
}

#[async_trait]
impl AgentRunRuntimeSurfaceSource for EnterpriseSurfaceSource {
    async fn prepare(
        &self,
        request: &AgentRunRuntimeProvisionRequest,
        thread_id: &RuntimeThreadId,
        binding_id: &RuntimeBindingId,
    ) -> Result<PreparedAgentRunRuntime, AgentRunRuntimeSurfaceSourceError> {
        let surface = materialized_surface(thread_id.clone());
        self.tool_registry
            .put(
                binding_id.clone(),
                CompiledAgentRunToolBinding {
                    runtime_session_id: thread_id.to_string(),
                    run_id: request.target.run_id,
                    agent_id: request.target.agent_id,
                    frame_id: Uuid::nil(),
                    catalog: agentdash_agent_runtime::ToolCatalogRevision {
                        revision: surface.tools.revision,
                        digest: surface.tools.digest.clone(),
                        tools: vec![agentdash_agent_runtime::ToolContribution {
                            meta: agentdash_agent_runtime::ContributionMeta {
                                key: "enterprise_echo".to_string(),
                                source: agentdash_agent_runtime::SurfaceSourceRef {
                                    layer: "enterprise_e2e".to_string(),
                                    key: "enterprise_echo".to_string(),
                                },
                                priority: 0,
                                requirement:
                                    agentdash_agent_runtime::ContributionRequirement::Required,
                            },
                            runtime_name: "enterprise_echo".to_string(),
                            description: "Enterprise reverse RuntimeWire tool".to_string(),
                            parameters_schema: json!({"type":"object"}),
                            capability_key: "enterprise".to_string(),
                            tool_path: "enterprise::echo".to_string(),
                            allowed_channels: BTreeSet::from([ToolChannel::DirectCallback]),
                            configuration_boundary: ConfigurationBoundary::Binding,
                        }],
                        mcp_servers: Vec::new(),
                    },
                    tools: BTreeMap::from([(
                        "enterprise_echo".to_string(),
                        Arc::new(EnterpriseEchoTool(self.tool_calls.clone())) as DynAgentTool,
                    )]),
                },
            )
            .await?;
        let profile = native_runtime_profile();
        Ok(PreparedAgentRunRuntime {
            service_instance_id: id("enterprise-fallback-unused"),
            definition_id: self.definition.provenance.definition_id.clone(),
            service_config: json!({}),
            placement: AgentRuntimePlacement::InProcess,
            hook_plan: RuntimeHookPlanBinding {
                thread_id: thread_id.clone(),
                plan: BoundRuntimeHookPlan {
                    revision: surface.hooks.revision,
                    digest: surface.hooks.digest.clone(),
                    entries: vec![BoundRuntimeHookEntry {
                        definition_id: id("enterprise-before-provider"),
                        point: HookPoint::BeforeProviderRequest,
                        actions: BTreeSet::from([HookAction::Observe]),
                        delivered_strength: SemanticStrength::ExactSynchronous,
                        failure_policy: HookFailurePolicy::FailClosed,
                        required: true,
                        site: HookExecutionSite::AgentCoreCallback,
                    }],
                },
            },
            bound_surface: BoundAgentSurfaceReference {
                revision: surface.revision,
                digest: surface.digest.clone(),
                tool_set_revision: surface.tools.revision,
                tool_set_digest: surface.tools.digest.clone(),
                hook_plan_revision: Some(surface.hooks.revision),
                hook_plan_digest: Some(surface.hooks.digest.clone()),
                hook_artifact_digest: surface.hooks.artifact_digest.clone(),
                hook_configuration_boundary: surface.hooks.configuration_boundary,
                required_hooks: vec![HookRequirement {
                    point: HookPoint::BeforeProviderRequest,
                    actions: BTreeSet::from([HookAction::Observe]),
                    minimum_strength: SemanticStrength::ExactSynchronous,
                    failure_policy: HookFailurePolicy::FailClosed,
                    required: true,
                }],
            },
            surface,
            transport_profile: profile.clone(),
            host_policy_profile: profile,
            conformance: ConformanceEvidence {
                suite_revision: self.manifest.suite_revision.clone(),
                driver_build_digest: self.manifest.driver_build_digest.clone(),
                verified_profile_digest: profile_digest(&self.manifest.verified_profile)
                    .expect("profile digest"),
                verified_at: Utc::now(),
            },
            allow_instance_creation: false,
        })
    }
}

async fn migrated_pool(name: &str) -> (PgPool, PostgresRuntime) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/enterprise-remote-e2e")
        .join(format!("{name}-{}", Uuid::new_v4().simple()));
    let runtime = PostgresRuntime::resolve_embedded_at_data_root(name, 8, root)
        .await
        .expect("embedded PostgreSQL");
    agentdash_infrastructure::migration::run_postgres_migrations(&runtime.pool)
        .await
        .expect("migrations");
    (runtime.pool.clone(), runtime)
}

async fn seed_agent_run_target(pool: &PgPool, target: &AgentRunRuntimeTarget) {
    let now = Utc::now();
    sqlx::query(
        "INSERT INTO lifecycle_runs \
         (id,project_id,topology,status,created_at,updated_at,last_activity_at) \
         VALUES ($1,$2,$3,$4,$5,$5,$5)",
    )
    .bind(target.run_id.to_string())
    .bind(Uuid::new_v4().to_string())
    .bind("single_agent")
    .bind("running")
    .bind(now)
    .execute(pool)
    .await
    .expect("seed lifecycle run");
    sqlx::query(
        "INSERT INTO lifecycle_agents \
         (id,run_id,project_id,source,status) \
         VALUES ($1,$2,$3,$4,$5)",
    )
    .bind(target.agent_id.to_string())
    .bind(target.run_id.to_string())
    .bind(Uuid::new_v4().to_string())
    .bind("project_agent")
    .bind("active")
    .execute(pool)
    .await
    .expect("seed lifecycle agent");
}

async fn local_host(
    pool: PgPool,
    contribution: AgentRuntimeDriverContribution,
    manifest: &AgentRuntimeTrustManifest,
) -> (
    Arc<RuntimeWireCommandHandler>,
    AgentServiceDefinitionRegistry,
) {
    let definition = contribution.definition.clone();
    let trusted_registry = AgentServiceDefinitionRegistry::collect(vec![contribution.clone()])
        .expect("local definition registry");
    let repository = Arc::new(PostgresAgentRuntimeHostRepository::new(pool));
    let host_port_router = Arc::new(RuntimeWireHostPortRouter::default());
    let host = Arc::new(IntegrationDriverHost::new(
        AgentServiceDefinitionRegistry::collect(vec![contribution]).expect("local registry"),
        repository.clone(),
        host_port_router.host_ports(Arc::new(NoCredentials)),
        Arc::new(TrustedDriverConformanceVerifier::new(
            TrustedDriverManifestRegistry::collect(vec![trusted_manifest(manifest)])
                .expect("local trust registry"),
        )),
        BACKEND_ID,
    ));
    let instance = host
        .put_instance(PutAgentServiceInstance {
            id: id("enterprise-local-instance"),
            definition_id: definition.provenance.definition_id,
            config: json!({"provider":"enterprise","model":"reference","credential_scope":{"kind":"platform"}}),
            credentials: BTreeMap::new(),
            placement: AgentRuntimePlacement::LocalProcess {
                host_id: BACKEND_ID.to_string(),
            },
            desired_state: ServiceInstanceDesiredState::Active,
            expected_revision: None,
        })
        .await
        .expect("put local enterprise instance");
    let profile = native_runtime_profile();
    host.activate(ActivateAgentServiceInstance {
        instance_id: instance.id,
        expected_revision: instance.revision,
        transport_profile: profile.clone(),
        transport_profile_digest: profile_digest(&profile).expect("transport digest"),
        host_policy_profile: profile.clone(),
        host_policy_digest: profile_digest(&profile).expect("policy digest"),
        conformance: ConformanceEvidence {
            suite_revision: manifest.suite_revision.clone(),
            driver_build_digest: manifest.driver_build_digest.clone(),
            verified_profile_digest: profile_digest(&manifest.verified_profile)
                .expect("verified profile"),
            verified_at: Utc::now(),
        },
    })
    .await
    .expect("activate local enterprise instance");
    let transport_id = AgentRuntimePlacementId::new(TRANSPORT_ID).expect("transport id");
    let handler = Arc::new(RuntimeWireCommandHandler::new_with_host_port_router(
        Arc::new(HostRuntimeDriverEndpointResolver::new(
            host,
            BACKEND_ID,
            agentdash_agent_runtime_contract::HostIncarnationId::new("enterprise-e2e-incarnation")
                .expect("host incarnation id"),
            transport_id,
        )),
        RuntimeRelayTransportDescriptor {
            supported_protocol_revisions: vec![
                agentdash_agent_runtime_wire::RUNTIME_WIRE_PROTOCOL_REVISION,
            ],
            profile: profile.clone(),
            profile_digest: profile_digest(&profile).expect("descriptor profile"),
            max_in_flight_frames: 64,
        },
        host_port_router,
    ));
    (handler, trusted_registry)
}

async fn route_local_message(
    handler: &RuntimeWireCommandHandler,
    registry: &BackendRegistry,
    message: RelayMessage,
) {
    let responses = match message {
        RelayMessage::RuntimeWireOpen { id, payload } => vec![handler.open(id, payload).await],
        RelayMessage::RuntimeWireFrame { id, payload } => handler.frame(id, *payload).await,
        RelayMessage::RuntimeWireAck { id, payload } => {
            handler.acknowledge(id, payload).await.into_iter().collect()
        }
        _ => Vec::new(),
    };
    for response in responses {
        registry
            .handle_runtime_wire_message(BACKEND_ID, &response)
            .await;
    }
}

#[tokio::test]
async fn enterprise_remote_mailbox_reaches_local_host_and_canonical_snapshot() {
    let (local_pool, _local_postgres) = migrated_pool("enterprise-remote-local").await;
    let (cloud_pool, _cloud_postgres) = migrated_pool("enterprise-remote-cloud").await;
    let (enterprise, manifest, bridge) = enterprise_contribution();
    let source_definition = enterprise.definition.clone();
    let (handler, _trusted_source_registry) = local_host(local_pool, enterprise, &manifest).await;
    let advertisements = handler.advertised_offers().await.expect("local offers");
    assert_eq!(advertisements.len(), 1);

    let registry = BackendRegistry::new();
    let (cloud_sender, mut cloud_to_local) = mpsc::unbounded_channel();
    let (local_sender, mut local_to_cloud) = mpsc::unbounded_channel();
    handler.attach_outbound(local_sender);
    registry
        .try_register(ConnectedBackend {
            backend_id: BACKEND_ID.to_string(),
            name: "Enterprise Desktop".to_string(),
            version: "1.0.0".to_string(),
            capabilities: CapabilitiesPayload::default(),
            sender: cloud_sender.clone(),
            connected_at: Utc::now(),
        })
        .await
        .expect("register local backend");
    let relay_registry = registry.clone();
    let relay_handler = handler.clone();
    let relay = tokio::spawn(async move {
        loop {
            tokio::select! {
                Some(message) = cloud_to_local.recv() => {
                    route_local_message(&relay_handler, &relay_registry, message).await;
                }
                Some(message) = local_to_cloud.recv() => {
                    relay_registry.handle_runtime_wire_message(BACKEND_ID, &message).await;
                }
                else => break,
            }
        }
    });

    let placement_resolver = Arc::new(CloudRuntimeWirePlacementResolver::new(registry.clone(), 64));
    let proxy_definition = remote_proxy_definition(&source_definition);
    let tool_registry = Arc::new(CompiledAgentRunToolRegistry::default());
    let tool_calls = Arc::new(AtomicUsize::new(0));
    let tool_broker_resolver = Arc::new(PostgresAgentRunToolBrokerResolver::new(
        cloud_pool.clone(),
        Arc::new(PostgresRuntimeRepository::new(cloud_pool.clone())),
        tool_registry.clone(),
        Arc::new(EnterpriseCapabilityPort),
    ));
    let tools: Arc<dyn AgentRuntimeToolCallback> = Arc::new(
        super::agent_runtime::PlatformAgentRuntimeToolCallback::new(tool_broker_resolver),
    );
    let hooks = Arc::new(RecordingHookCallback::default());
    let target = AgentRunRuntimeTarget {
        run_id: Uuid::new_v4(),
        agent_id: Uuid::new_v4(),
    };
    seed_agent_run_target(&cloud_pool, &target).await;
    let composition = build_agent_runtime_composition(AgentRuntimeCompositionInput {
        pool: cloud_pool,
        contributions: vec![remote_runtime_contribution(
            proxy_definition.clone(),
            placement_resolver,
        )],
        trusted_manifests: vec![TrustedDriverManifest {
            provenance: proxy_definition.provenance.clone(),
            ..trusted_manifest(&manifest)
        }],
        surface_source: Arc::new(EnterpriseSurfaceSource {
            definition: proxy_definition,
            manifest: manifest.clone(),
            tool_registry,
            tool_calls: tool_calls.clone(),
        }),
        credential_broker: Arc::new(NoCredentials),
        tool_callback: tools.clone(),
        hook_callback: hooks.clone(),
        node_id: "enterprise-cloud-host".to_string(),
    })
    .expect("cloud production composition");
    let inventory = CloudRemoteRuntimeInventory::new(composition.host.clone());
    inventory.mark_online(BACKEND_ID).await;
    tokio::time::timeout(
        std::time::Duration::from_secs(20),
        inventory.sync(BACKEND_ID, &advertisements),
    )
    .await
    .expect("remote inventory sync timeout")
    .expect("register remote proxy offer");

    let runtime: Arc<dyn AgentRunRuntime> = Arc::new(ManagedAgentRunRuntime::new(
        composition.gateway.clone(),
        composition.bindings.clone(),
        composition.provisioner.clone(),
    ));
    let mailbox = RuntimeAgentRunMailbox::new(
        Arc::new(MemoryAgentRunMailboxRepository::default()),
        runtime.clone(),
    );
    let submitted = tokio::time::timeout(
        std::time::Duration::from_secs(20),
        mailbox.submit(EnqueueRuntimeMailboxMessage {
            target: target.clone(),
            client_command_id: "enterprise-first-message".to_string(),
            input: vec![RuntimeInput::Text {
                text: "run through enterprise remote".to_string(),
            }],
            actor: RuntimeActor::User {
                subject: "enterprise-user".to_string(),
            },
            identity: None,
            source: MailboxSourceIdentity::composer(),
            backend_selection: None,
        }),
    )
    .await
    .expect("mailbox submit timeout")
    .expect("mailbox submit");
    let (receipt, mailbox_message_id) = match &submitted {
        RuntimeMailboxSubmitOutcome::Dispatched { receipt, message } => (receipt, message.id),
        RuntimeMailboxSubmitOutcome::Queued { .. } => panic!("idle mailbox must dispatch"),
    };
    assert!(!receipt.duplicate);
    assert_eq!(
        tokio::time::timeout(
            std::time::Duration::from_secs(20),
            composition.outbox_worker.run_once(8),
        )
        .await
        .expect("outbox timeout")
        .expect("outbox"),
        1
    );

    let terminal_view = tokio::time::timeout(std::time::Duration::from_secs(20), async {
        loop {
            let view = runtime.inspect(target.clone()).await.expect("inspect");
            if view.snapshot.as_ref().is_some_and(|snapshot| {
                snapshot.active_turn_id.is_none()
                    && snapshot.transcript.iter().any(|item| {
                        item.final_content.agent_message_text()
                            == Some("enterprise remote completed")
                    })
            }) {
                break view;
            }
            tokio::task::yield_now().await;
        }
    })
    .await;
    let view = match terminal_view {
        Ok(view) => view,
        Err(_) => {
            let diagnostic = runtime
                .inspect(target.clone())
                .await
                .expect("inspect timeout diagnostic");
            let mut stream = runtime
                .read_events(
                    agentdash_application_agentrun::agent_run::ReadAgentRunEvents {
                        target: target.clone(),
                        after: None,
                        include_transient: true,
                        transient_after: None,
                        stream_generation: None,
                    },
                )
                .await
                .expect("read timeout diagnostic events");
            let mut events = Vec::new();
            while let Some(event) = stream.next().await {
                events.push(event.expect("timeout diagnostic event"));
            }
            panic!(
                "enterprise remote terminal snapshot timeout: {diagnostic:#?}; events={events:#?}"
            );
        }
    };
    let binding = view.binding.expect("remote binding");
    let snapshot = view.snapshot.expect("canonical snapshot");
    if snapshot.status != RuntimeThreadStatus::Active {
        let mut stream = runtime
            .read_events(
                agentdash_application_agentrun::agent_run::ReadAgentRunEvents {
                    target: target.clone(),
                    after: None,
                    include_transient: true,
                    transient_after: None,
                    stream_generation: None,
                },
            )
            .await
            .expect("read terminal diagnostic events");
        let mut events = Vec::new();
        while let Some(event) = stream.next().await {
            events.push(event.expect("terminal diagnostic event"));
        }
        panic!("unexpected terminal snapshot before compaction: {snapshot:#?}; events={events:#?}");
    }
    assert_eq!(
        snapshot.status,
        RuntimeThreadStatus::Active,
        "unexpected terminal snapshot before compaction: {snapshot:#?}"
    );
    assert!(snapshot.active_turn_id.is_none());
    assert!(
        snapshot
            .transcript
            .iter()
            .any(|item| item.final_content.agent_message_text()
                == Some("enterprise remote completed"))
    );
    let host_binding = composition
        .host
        .binding(&binding.binding_id)
        .await
        .expect("Host binding");
    let remote_instance = composition
        .host
        .service_instance(&host_binding.service_instance_id)
        .await
        .expect("remote instance read")
        .expect("remote instance");
    assert!(matches!(
        remote_instance.placement,
        AgentRuntimePlacement::Remote { ref host_id, .. } if host_id == BACKEND_ID
    ));
    assert_eq!(tool_calls.load(Ordering::SeqCst), 1);
    assert!(hooks.0.load(Ordering::SeqCst) >= 1);

    let context_revision_before = snapshot.context_revision;
    let compaction = runtime
        .compact_context(GuardedAgentRunCommand {
            target: target.clone(),
            client_command_id: "enterprise-compact-context".to_string(),
            guard: AgentRunCommandGuard {
                thread_id: snapshot.thread_id.clone(),
                expected_revision: snapshot.revision,
                expected_active_turn_id: None,
            },
            actor: RuntimeActor::User {
                subject: "enterprise-user".to_string(),
            },
        })
        .await
        .expect("accept remote context compaction");
    assert!(!compaction.duplicate);
    assert_eq!(
        composition
            .durable_workers
            .run_once(RuntimeWorkKind::ContextPreparation, 8)
            .await
            .expect("prepare remote context compaction"),
        1
    );
    assert_eq!(
        composition
            .durable_workers
            .run_once(RuntimeWorkKind::ContextActivationDispatch, 8)
            .await
            .expect("dispatch remote context activation"),
        1
    );
    let compacted = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            composition
                .durable_workers
                .run_once(RuntimeWorkKind::ContextActivationRecovery, 8)
                .await
                .expect("recover remote context activation");
            let view = runtime
                .inspect(target.clone())
                .await
                .expect("inspect compacted runtime");
            if view.snapshot.as_ref().is_some_and(|snapshot| {
                snapshot.context_revision > context_revision_before
                    && snapshot.active_checkpoint_id.is_some()
            }) {
                break view.snapshot.expect("compacted snapshot");
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("remote context activation timeout");
    assert!(compacted.context_revision > context_revision_before);

    let duplicate = runtime
        .send_message(
            agentdash_application_agentrun::agent_run::SendAgentRunMessage {
                target: target.clone(),
                client_command_id: format!("mailbox-{mailbox_message_id}"),
                input: vec![RuntimeInput::Text {
                    text: "run through enterprise remote".to_string(),
                }],
                actor: RuntimeActor::User {
                    subject: "enterprise-user".to_string(),
                },
                identity: None,
                backend_selection: None,
            },
        )
        .await
        .expect("duplicate command replay");
    assert!(duplicate.duplicate);
    assert_eq!(
        composition
            .outbox_worker
            .run_once(8)
            .await
            .expect("duplicate outbox"),
        0
    );

    bridge.block_next.store(true, Ordering::SeqCst);
    let active = runtime
        .send_message(
            agentdash_application_agentrun::agent_run::SendAgentRunMessage {
                target: target.clone(),
                client_command_id: "enterprise-disconnect-active-turn".to_string(),
                input: vec![RuntimeInput::Text {
                    text: "block until RuntimeWire disconnect".to_string(),
                }],
                actor: RuntimeActor::User {
                    subject: "enterprise-user".to_string(),
                },
                identity: None,
                backend_selection: None,
            },
        )
        .await
        .expect("accept active disconnect turn");
    assert!(!active.duplicate);
    let active_dispatch = {
        let outbox = composition.outbox_worker.clone();
        tokio::spawn(async move { outbox.run_once(8).await })
    };
    tokio::time::timeout(std::time::Duration::from_secs(10), bridge.blocked.acquire())
        .await
        .expect("blocked provider entry timeout")
        .expect("bridge enters blocked provider call")
        .forget();

    registry.unregister(BACKEND_ID).await;
    let disconnected_dispatch =
        tokio::time::timeout(std::time::Duration::from_secs(10), active_dispatch)
            .await
            .expect("disconnected dispatch worker timeout")
            .expect("disconnected dispatch worker join")
            .expect("terminalized disconnected dispatch is durably acknowledged");
    assert_eq!(disconnected_dispatch, 1);
    let lost = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let view = runtime.inspect(target.clone()).await.expect("inspect lost");
            if view.snapshot.as_ref().is_some_and(|snapshot| {
                snapshot.status == RuntimeThreadStatus::Lost && snapshot.active_turn_id.is_none()
            }) {
                break view.snapshot.expect("lost snapshot");
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("disconnect must converge active turn to Lost");
    assert_eq!(lost.status, RuntimeThreadStatus::Lost);
    let binding_lost = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let mut lost_events = runtime
                .read_events(
                    agentdash_application_agentrun::agent_run::ReadAgentRunEvents {
                        target: target.clone(),
                        after: None,
                        include_transient: false,
                        transient_after: None,
                        stream_generation: None,
                    },
                )
                .await
                .expect("read lost events");
            let mut count = 0;
            while let Some(event) = lost_events.next().await {
                if matches!(
                    event.expect("lost event").event,
                    RuntimeEvent::BindingLost { .. }
                ) {
                    count += 1;
                }
            }
            if count > 0 {
                break count;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("BindingLost journal convergence timeout");
    assert_eq!(binding_lost, 1, "disconnect has exactly one BindingLost");
    registry.unregister(BACKEND_ID).await;

    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        registry.try_register(ConnectedBackend {
            backend_id: BACKEND_ID.to_string(),
            name: "Enterprise Desktop Reconnected".to_string(),
            version: "1.0.1".to_string(),
            capabilities: CapabilitiesPayload::default(),
            sender: cloud_sender,
            connected_at: Utc::now(),
        }),
    )
    .await
    .expect("Runtime Wire reopen timeout")
    .expect("re-register local backend and reopen Runtime Wire");
    bridge.release.add_permits(1);
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let after_late = runtime
        .inspect(target.clone())
        .await
        .expect("inspect after late old event")
        .snapshot
        .expect("snapshot after late old event");
    assert_eq!(after_late.status, RuntimeThreadStatus::Lost);
    assert!(after_late.active_turn_id.is_none());

    let duplicate_lost = runtime
        .send_message(
            agentdash_application_agentrun::agent_run::SendAgentRunMessage {
                target,
                client_command_id: "enterprise-disconnect-active-turn".to_string(),
                input: vec![RuntimeInput::Text {
                    text: "block until RuntimeWire disconnect".to_string(),
                }],
                actor: RuntimeActor::User {
                    subject: "enterprise-user".to_string(),
                },
                identity: None,
                backend_selection: None,
            },
        )
        .await
        .expect("duplicate lost command remains replayable");
    assert!(duplicate_lost.duplicate);
    assert_eq!(
        composition
            .outbox_worker
            .run_once(8)
            .await
            .expect("lost duplicate outbox"),
        0
    );

    relay.abort();
}
