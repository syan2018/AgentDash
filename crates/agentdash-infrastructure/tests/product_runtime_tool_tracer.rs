use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use agentdash_agent_runtime::{
    PlatformToolBroker, RuntimeToolAppliedSurfaceEvidence, RuntimeToolAuthorizationGrant,
    RuntimeToolAuthorizationPort, RuntimeToolAuthorizationRequest, RuntimeToolBrokerError,
    RuntimeToolEffect, RuntimeToolPermission, RuntimeToolProductTarget,
    RuntimeToolProvenanceEvidence, RuntimeToolResourceGrant,
};
use agentdash_agent_runtime_contract::RuntimeThreadId;
use agentdash_agent_runtime_host::{
    CompleteAgentBindingId, CompleteAgentCallbackBroker, CompleteAgentHookHandler,
    CompleteAgentHost, CompleteAgentPlacement, CompleteAgentRuntimeTarget,
    CompleteAgentRuntimeTargetProvisioningRequest, CompleteAgentServiceVerification,
    CompleteAgentToolHandler, CompleteAgentVerificationMethod, CompleteAgentVerifiedBuildEvidence,
    CompleteAgentVerifiedServiceRegistration, ProcessCompleteAgentLiveCatalog,
    ResolvedCompleteAgentCallbackContext, ResolvedCompleteAgentHookCallback,
    ResolvedCompleteAgentToolCallback, RuntimePlatformToolHandler,
};
use agentdash_agent_service_api::*;
use agentdash_application_ports::product_runtime_tool::{
    ProductRuntimeToolKind, ProductRuntimeToolOutcome, ProductRuntimeToolRequest,
    ProductRuntimeToolService,
};
use agentdash_infrastructure::{
    PostgresAgentRunProductRuntimeBindingRepository, WorkspaceModulePresentRuntimeTool,
    product_runtime_tool_catalog,
};
use agentdash_workspace_module::workspace_module::presentation_protocol::{
    WorkspaceModulePresentationChange, WorkspaceModulePresentationCommand,
    WorkspaceModulePresentationCommandError, WorkspaceModulePresentationCommandPort,
    WorkspaceModulePresentationStoreError,
};
use async_trait::async_trait;
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use tokio::sync::Mutex;
use uuid::Uuid;

const RUNTIME_THREAD_ID: &str = "runtime-thread-product-tracer";
const CALLBACK_ROUTE_ID: &str = "product-tools-route";
const SOURCE_COORDINATE: &str = "product-tools-source";
const SERVICE_INSTANCE_ID: &str = "product-tools-service";
const PROFILE_DIGEST: &str = "product-tools-profile";

struct ProductGrantAuthorizer {
    project_id: Uuid,
    run_id: Uuid,
    agent_id: Uuid,
}

#[async_trait]
impl RuntimeToolAuthorizationPort for ProductGrantAuthorizer {
    async fn authorize(
        &self,
        request: RuntimeToolAuthorizationRequest,
    ) -> Result<RuntimeToolAuthorizationGrant, RuntimeToolBrokerError> {
        let provenance = RuntimeToolProvenanceEvidence {
            source_kind: "product_runtime_tracer".to_owned(),
            source_id: "applied-surface".to_owned(),
            source_revision: 7,
            projection_revision: 9,
            captured_at_ms: 11,
        };
        Ok(RuntimeToolAuthorizationGrant {
            permission: request.definition.permission,
            effect: request.definition.effect,
            target: RuntimeToolProductTarget {
                project_id: self.project_id.to_string(),
                run_id: self.run_id.to_string(),
                agent_id: self.agent_id.to_string(),
            },
            applied_surface: RuntimeToolAppliedSurfaceEvidence {
                agent_surface_revision: 3,
                agent_surface_digest: "surface-digest".to_owned(),
                vfs_digest: "vfs-digest".to_owned(),
                vfs_provenance: provenance.clone(),
                task_digest: "task-digest".to_owned(),
                product_binding_digest: "product-binding-digest".to_owned(),
                host_binding_generation: 1,
            },
            resources: RuntimeToolResourceGrant::Product,
        })
    }
}

struct RecordingProductToolService {
    kind: ProductRuntimeToolKind,
    calls: AtomicUsize,
    requests: Mutex<Vec<ProductRuntimeToolRequest>>,
}

impl RecordingProductToolService {
    fn new(kind: ProductRuntimeToolKind) -> Self {
        Self {
            kind,
            calls: AtomicUsize::new(0),
            requests: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl ProductRuntimeToolService for RecordingProductToolService {
    fn kind(&self) -> ProductRuntimeToolKind {
        self.kind
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "owner": runtime_tool_name(self.kind),
        })
    }

    async fn execute(&self, request: ProductRuntimeToolRequest) -> ProductRuntimeToolOutcome {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.requests.lock().await.push(request.clone());
        ProductRuntimeToolOutcome::Completed {
            output: json!({
                "tool": runtime_tool_name(self.kind),
                "effect_id": request.context.effect_id,
                "invocation_id": request.context.invocation_id,
                "arguments": request.arguments,
            }),
        }
    }
}

struct AllowHookHandler;

#[async_trait]
impl CompleteAgentHookHandler for AllowHookHandler {
    async fn invoke(
        &self,
        _callback: ResolvedCompleteAgentHookCallback,
    ) -> Result<AgentHookDecision, AgentHostCallbackError> {
        Ok(AgentHookDecision::Allow)
    }
}

struct RejectingPresentationPort;

#[async_trait]
impl WorkspaceModulePresentationCommandPort for RejectingPresentationPort {
    async fn present(
        &self,
        _command: WorkspaceModulePresentationCommand,
    ) -> Result<WorkspaceModulePresentationChange, WorkspaceModulePresentationCommandError> {
        Err(
            WorkspaceModulePresentationStoreError::Persistence("definition-only tracer".to_owned())
                .into(),
        )
    }
}

#[tokio::test]
async fn companion_callbacks_forward_stable_owner_identities_without_host_replay() {
    let request_service = Arc::new(RecordingProductToolService::new(
        ProductRuntimeToolKind::CompanionRequest,
    ));
    let respond_service = Arc::new(RecordingProductToolService::new(
        ProductRuntimeToolKind::CompanionRespond,
    ));
    let services: Vec<Arc<dyn ProductRuntimeToolService>> =
        vec![request_service.clone(), respond_service.clone()];
    let (host, target) = callback_host(&["companion_request", "companion_respond"]).await;

    let first_broker = complete_agent_callback_broker(services.clone(), host.clone());
    let request_call = callback_call_for_target(
        &target,
        "companion_request",
        "companion-request-effect",
        "companion-request-callback",
        json!({"message": "请核验当前实现"}),
    );
    let first_request = first_broker
        .invoke_tool(request_call.clone())
        .await
        .expect("Companion request callback");

    let restarted_broker = complete_agent_callback_broker(services.clone(), host.clone());
    let retried_request = restarted_broker
        .invoke_tool(request_call)
        .await
        .expect("Companion request callback retry");
    assert_eq!(first_request, retried_request);
    assert_eq!(request_service.calls.load(Ordering::SeqCst), 2);

    let respond_call = callback_call_for_target(
        &target,
        "companion_respond",
        "companion-respond-effect",
        "companion-respond-callback",
        json!({"request_id": "request-1", "message": "已完成核验"}),
    );
    let first_response = restarted_broker
        .invoke_tool(respond_call.clone())
        .await
        .expect("Companion response callback");

    let second_restart = complete_agent_callback_broker(services, host);
    let retried_response = second_restart
        .invoke_tool(respond_call)
        .await
        .expect("Companion response callback retry");
    assert_eq!(first_response, retried_response);
    assert_eq!(respond_service.calls.load(Ordering::SeqCst), 2);

    let request_records = request_service.requests.lock().await;
    assert_eq!(request_records.len(), 2);
    assert_eq!(
        request_records[0].context.effect_id,
        "companion-request-effect"
    );
    assert_eq!(
        request_records[0].context.invocation_id,
        "companion-request-callback"
    );
    assert_eq!(
        request_records[0].context.runtime_thread_id.as_str(),
        RUNTIME_THREAD_ID
    );
    assert_eq!(request_records[0].context, request_records[1].context);
    drop(request_records);

    let response_records = respond_service.requests.lock().await;
    assert_eq!(response_records.len(), 2);
    assert_eq!(
        response_records[0].context.effect_id,
        "companion-respond-effect"
    );
    assert_eq!(
        response_records[0].context.invocation_id,
        "companion-respond-callback"
    );
    assert_eq!(response_records[0].context, response_records[1].context);
}

#[tokio::test]
async fn workspace_tools_keep_read_write_and_presentation_invariants_in_final_broker() {
    let list_service = Arc::new(RecordingProductToolService::new(
        ProductRuntimeToolKind::WorkspaceModuleList,
    ));
    let describe_service = Arc::new(RecordingProductToolService::new(
        ProductRuntimeToolKind::WorkspaceModuleDescribe,
    ));
    let invoke_service = Arc::new(RecordingProductToolService::new(
        ProductRuntimeToolKind::WorkspaceModuleInvoke,
    ));
    let operate_service = Arc::new(RecordingProductToolService::new(
        ProductRuntimeToolKind::WorkspaceModuleOperate,
    ));
    let services: Vec<Arc<dyn ProductRuntimeToolService>> = vec![
        list_service.clone(),
        describe_service.clone(),
        operate_service.clone(),
        invoke_service.clone(),
    ];
    let mut executors = product_runtime_tool_catalog(services);
    let pool = PgPoolOptions::new()
        .connect_lazy("postgres://agentdash:agentdash@127.0.0.1/agentdash")
        .expect("lazy PostgreSQL pool");
    executors.push(Arc::new(WorkspaceModulePresentRuntimeTool::new(
        Arc::new(PostgresAgentRunProductRuntimeBindingRepository::new(pool)),
        Arc::new(RejectingPresentationPort),
    )));
    let broker = Arc::new(
        PlatformToolBroker::new(executors, product_authorizer())
            .expect("final Product tool broker"),
    );

    let definitions = broker.definitions();
    assert_workspace_definition(
        &definitions,
        "workspace_module_list",
        RuntimeToolPermission::ProductRead,
        RuntimeToolEffect::ReadOnly,
    );
    assert_workspace_definition(
        &definitions,
        "workspace_module_describe",
        RuntimeToolPermission::ProductRead,
        RuntimeToolEffect::ReadOnly,
    );
    assert_workspace_definition(
        &definitions,
        "workspace_module_operate",
        RuntimeToolPermission::ProductWrite,
        RuntimeToolEffect::ProductMutation,
    );
    assert_workspace_definition(
        &definitions,
        "workspace_module_invoke",
        RuntimeToolPermission::ProductWrite,
        RuntimeToolEffect::ProductMutation,
    );
    assert_workspace_definition(
        &definitions,
        "workspace_module_present",
        RuntimeToolPermission::ProductWrite,
        RuntimeToolEffect::ProductMutation,
    );

    let handler = RuntimePlatformToolHandler::new(broker);
    for (tool, effect, callback, arguments) in [
        (
            "workspace_module_list",
            "workspace-list-effect",
            "workspace-list-callback",
            json!({}),
        ),
        (
            "workspace_module_describe",
            "workspace-describe-effect",
            "workspace-describe-callback",
            json!({"module_id": "module-1"}),
        ),
        (
            "workspace_module_operate",
            "workspace-create-effect",
            "workspace-create-callback",
            json!({"operation": "canvas.create", "input": {"title": "Tracer"}}),
        ),
        (
            "workspace_module_operate",
            "workspace-attach-effect",
            "workspace-attach-callback",
            json!({"operation": "canvas.attach", "input": {"canvas_mount_id": "tracer"}}),
        ),
        (
            "workspace_module_operate",
            "workspace-copy-effect",
            "workspace-copy-callback",
            json!({"operation": "canvas.copy", "input": {"source_mount_id": "tracer"}}),
        ),
        (
            "workspace_module_invoke",
            "workspace-invoke-effect",
            "workspace-invoke-callback",
            json!({
                "module_id": "canvas:tracer",
                "operation_key": "canvas.bind_data",
                "input": {
                    "alias": "metrics",
                    "source_uri": "workspace://metrics.json"
                }
            }),
        ),
    ] {
        let result = handler
            .invoke(ResolvedCompleteAgentToolCallback {
                context: resolved_callback_context(),
                invocation: callback_call(tool, effect, callback, arguments),
            })
            .await
            .expect("Workspace callback handler");
        assert!(matches!(result, AgentToolResult::Completed { .. }));
    }

    assert_eq!(list_service.calls.load(Ordering::SeqCst), 1);
    assert_eq!(describe_service.calls.load(Ordering::SeqCst), 1);
    assert_eq!(operate_service.calls.load(Ordering::SeqCst), 3);
    assert_eq!(invoke_service.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        operate_service
            .requests
            .lock()
            .await
            .iter()
            .map(|request| request.arguments["operation"].clone())
            .collect::<Vec<_>>(),
        vec![
            json!("canvas.create"),
            json!("canvas.attach"),
            json!("canvas.copy"),
        ]
    );
    assert_eq!(
        invoke_service.requests.lock().await[0].context.effect_id,
        "workspace-invoke-effect"
    );
}

fn complete_agent_callback_broker(
    services: Vec<Arc<dyn ProductRuntimeToolService>>,
    host: Arc<CompleteAgentHost>,
) -> CompleteAgentCallbackBroker {
    let broker = Arc::new(
        PlatformToolBroker::new(product_runtime_tool_catalog(services), product_authorizer())
            .expect("Product callback broker"),
    );
    CompleteAgentCallbackBroker::new(
        Arc::new(RuntimePlatformToolHandler::new(broker)),
        Arc::new(AllowHookHandler),
        host,
    )
}

fn product_authorizer() -> Arc<dyn RuntimeToolAuthorizationPort> {
    Arc::new(ProductGrantAuthorizer {
        project_id: Uuid::from_u128(1),
        run_id: Uuid::from_u128(2),
        agent_id: Uuid::from_u128(3),
    })
}

async fn callback_host(
    tool_names: &[&str],
) -> (Arc<CompleteAgentHost>, CompleteAgentRuntimeTarget) {
    let catalog = Arc::new(ProcessCompleteAgentLiveCatalog::new());
    let host = Arc::new(CompleteAgentHost::new(catalog));
    let service = Arc::new(TracerCompleteAgentService::new());
    let instance_id = AgentServiceInstanceId::new(SERVICE_INSTANCE_ID).expect("service instance");
    let selection = host
        .attach_verified_service(
            CompleteAgentVerifiedServiceRegistration {
                instance_id: instance_id.clone(),
                descriptor: service.descriptor.clone(),
                placement: CompleteAgentPlacement::InProcess {
                    host_incarnation_id: "product-tools-host".to_owned(),
                },
                verification: CompleteAgentServiceVerification {
                    service_instance_id: instance_id,
                    publisher_integration: "product-runtime-tracer".to_owned(),
                    service_version: "fixture-v1".to_owned(),
                    verifier_identity: "product-runtime-tracer".to_owned(),
                    verifier_revision: "fixture-v1".to_owned(),
                    method: CompleteAgentVerificationMethod::PinnedBuiltin,
                    verified_profile_digest: service.descriptor.profile_digest.clone(),
                    claimed_conformance_suite_revision: "fixture-v1".to_owned(),
                    verified_build: CompleteAgentVerifiedBuildEvidence {
                        claimed_build_digest: AgentPayloadDigest::new("sha256:product-tools-build")
                            .expect("build digest"),
                        evidence_digest: AgentPayloadDigest::new("sha256:product-tools-evidence")
                            .expect("evidence digest"),
                    },
                },
                remote_binding: None,
            },
            service,
        )
        .await
        .expect("attach tracer Complete Agent");
    let runtime_thread_id = RuntimeThreadId::new(RUNTIME_THREAD_ID).expect("Runtime thread");
    let target = host
        .provision_runtime_target(CompleteAgentRuntimeTargetProvisioningRequest {
            idempotency_key: AgentIdempotencyKey::new("product-tools-provision")
                .expect("provision idempotency"),
            request_digest: AgentPayloadDigest::new("product-tools-provision-request")
                .expect("provision digest"),
            runtime_thread_id: runtime_thread_id.clone(),
            target: selection.target,
            desired_surface: desired_tool_surface(tool_names),
            callback_deadline_ms: u64::MAX,
        })
        .await
        .expect("provision tracer Runtime target")
        .target;
    host.restore_runtime_source_route(
        &runtime_thread_id,
        AgentSourceCoordinate::new(SOURCE_COORDINATE).expect("source"),
        AgentEffectIdentity::new("product-tools-restore").expect("restore effect"),
        "product-runtime-tracer".to_owned(),
        1,
    )
    .await
    .expect("restore tracer source route");
    (host, target)
}

fn desired_tool_surface(tool_names: &[&str]) -> AgentSurfaceSnapshot {
    let semantics = AgentSurfaceSemanticFacet::Tool(AgentToolSemanticFacet {
        delivery: AgentToolDelivery::AgentNativeCallback,
        invocation: SemanticFidelity::Exact,
        update: AgentToolUpdateSemantics::BindingOnly,
    });
    AgentSurfaceSnapshot {
        revision: AgentSurfaceRevision(1),
        digest: AgentSurfaceDigest::new("product-tools-surface").expect("surface digest"),
        requirements: tool_names
            .iter()
            .map(|name| AgentSurfaceRequirement {
                key: format!("tool:{name}"),
                required: true,
                minimum_fidelity: SemanticFidelity::Exact,
                allowed_routes: BTreeSet::from([AgentSurfaceRoute::AgentNativeCallback]),
                semantics: semantics.clone(),
                payload: AgentSurfaceContributionPayload::Tool {
                    name: AgentToolName::new(*name).expect("tool name"),
                    description: format!("{name} tracer"),
                    input_schema: json!({"type": "object"}),
                    output_schema: Some(json!({"type": "object"})),
                },
                payload_digest: AgentPayloadDigest::new(format!("{name}-payload"))
                    .expect("payload digest"),
            })
            .collect(),
    }
}

struct TracerCompleteAgentService {
    descriptor: AgentServiceDescriptor,
}

impl TracerCompleteAgentService {
    fn new() -> Self {
        Self {
            descriptor: AgentServiceDescriptor {
                definition_id: AgentServiceDefinitionId::new("product-tools-definition")
                    .expect("definition"),
                title: "Product Runtime Tool Tracer".to_owned(),
                protocol_revision: 1,
                profile: AgentCapabilityProfile {
                    lifecycle: BTreeSet::from([
                        AgentLifecycleCapability::Create,
                        AgentLifecycleCapability::Resume,
                    ]),
                    commands: BTreeSet::from([AgentCommandCapability::SubmitInput]),
                    fork: AgentForkCapability {
                        cutoffs: BTreeMap::new(),
                        lineage_fidelity: SemanticFidelity::Unsupported,
                        native_durability: SemanticFidelity::Unsupported,
                    },
                    compaction: BTreeMap::new(),
                    source_changes: AgentSourceChangeLevel::SnapshotOnly,
                    initial_context: InitialContextProfile {
                        contribution_fidelity: BTreeMap::new(),
                        applied_evidence: InitialContextAppliedEvidence::PackageDigest,
                        renderer_versions: BTreeSet::new(),
                    },
                    surface: AgentSurfaceProfile {
                        facets: vec![AgentSurfaceCapabilityFacet {
                            semantics: AgentSurfaceSemanticFacet::Tool(AgentToolSemanticFacet {
                                delivery: AgentToolDelivery::AgentNativeCallback,
                                invocation: SemanticFidelity::Exact,
                                update: AgentToolUpdateSemantics::BindingOnly,
                            }),
                            routes: BTreeSet::from([AgentSurfaceRoute::AgentNativeCallback]),
                            fidelity: SemanticFidelity::Exact,
                            configuration_boundary: AgentConfigurationBoundary::Binding,
                        }],
                    },
                    inspect_effects: SemanticFidelity::Exact,
                },
                profile_digest: AgentProfileDigest::new(PROFILE_DIGEST).expect("profile digest"),
                configuration_boundary: AgentConfigurationBoundary::Binding,
            },
        }
    }
}

#[async_trait]
impl CompleteAgentService for TracerCompleteAgentService {
    async fn describe(&self) -> Result<AgentServiceDescriptor, AgentServiceError> {
        Ok(self.descriptor.clone())
    }

    async fn create(
        &self,
        _command: CreateAgentCommand,
    ) -> Result<AgentCommandReceipt, AgentServiceError> {
        Err(unused_agent_operation())
    }

    async fn resume(
        &self,
        _command: ResumeAgentCommand,
    ) -> Result<AgentCommandReceipt, AgentServiceError> {
        Err(unused_agent_operation())
    }

    async fn fork(
        &self,
        _command: ForkAgentCommand,
    ) -> Result<ForkAgentReceipt, AgentServiceError> {
        Err(unused_agent_operation())
    }

    async fn execute(
        &self,
        _command: AgentCommandEnvelope,
    ) -> Result<AgentCommandReceipt, AgentServiceError> {
        Err(unused_agent_operation())
    }

    async fn read(&self, _query: AgentReadQuery) -> Result<AgentSnapshot, AgentServiceError> {
        Err(unused_agent_operation())
    }

    async fn changes(
        &self,
        _query: AgentChangesQuery,
    ) -> Result<AgentChangePage, AgentServiceError> {
        Err(unused_agent_operation())
    }

    async fn inspect(
        &self,
        effect_id: AgentEffectIdentity,
    ) -> Result<AgentEffectInspection, AgentServiceError> {
        Ok(AgentEffectInspection {
            effect_id,
            command_id: None,
            state: AgentEffectInspectionState::NotApplied,
        })
    }

    async fn apply_surface(
        &self,
        command: ApplyBoundAgentSurface,
    ) -> Result<AppliedAgentSurfaceReceipt, AgentServiceError> {
        let applied = AppliedAgentSurface {
            revision: command.bound_surface.revision,
            digest: command.bound_surface.digest.clone(),
            contributions: command
                .bound_surface
                .contributions
                .iter()
                .map(|contribution| AppliedAgentSurfaceContribution {
                    key: contribution.key.clone(),
                    route: contribution.route,
                    fidelity: contribution.fidelity,
                    semantics: contribution.semantics.clone(),
                    payload_digest: contribution.payload_digest.clone(),
                    status: AppliedContributionStatus::Applied,
                    evidence: Some("product-runtime-tracer".to_owned()),
                })
                .collect(),
        };
        Ok(AppliedAgentSurfaceReceipt {
            command_id: command.command_id,
            effect_id: command.effect_id,
            source: command.source,
            applied,
        })
    }

    async fn revoke_surface(
        &self,
        _command: RevokeBoundAgentSurface,
    ) -> Result<AgentCommandReceipt, AgentServiceError> {
        Err(unused_agent_operation())
    }
}

fn unused_agent_operation() -> AgentServiceError {
    AgentServiceError::new(
        AgentServiceErrorCode::Unsupported,
        "not used by Product Runtime tool tracer",
        false,
    )
}

fn callback_call_for_target(
    target: &CompleteAgentRuntimeTarget,
    tool: &str,
    effect_id: &str,
    idempotency_key: &str,
    arguments: Value,
) -> AgentToolInvocation {
    let mut invocation = callback_call(tool, effect_id, idempotency_key, arguments);
    invocation.meta.route_id = target.callbacks.route_id.clone();
    invocation.meta.binding_generation = target.generation;
    invocation
}

fn callback_call(
    tool: &str,
    effect_id: &str,
    idempotency_key: &str,
    arguments: Value,
) -> AgentToolInvocation {
    AgentToolInvocation {
        meta: AgentHostCallbackMeta {
            route_id: AgentCallbackRouteId::new(CALLBACK_ROUTE_ID).expect("callback route"),
            binding_generation: AgentBindingGeneration(1),
            source: AgentSourceCoordinate::new(SOURCE_COORDINATE).expect("source"),
            turn_id: AgentTurnId::new("product-tools-turn").expect("turn"),
            item_id: Some(
                AgentItemId::new(format!("{tool}-item")).expect("Complete Agent tool item"),
            ),
            interaction_id: None,
            effect_id: AgentEffectIdentity::new(effect_id).expect("effect"),
            idempotency_key: AgentIdempotencyKey::new(idempotency_key).expect("idempotency"),
            deadline_at_ms: u64::MAX,
        },
        tool: AgentToolName::new(tool).expect("tool"),
        arguments,
    }
}

fn resolved_callback_context() -> ResolvedCompleteAgentCallbackContext {
    ResolvedCompleteAgentCallbackContext {
        runtime_thread_id: RuntimeThreadId::new(RUNTIME_THREAD_ID).expect("Runtime thread"),
        binding_id: CompleteAgentBindingId::new("product-tools-binding").expect("binding"),
        binding_generation: AgentBindingGeneration(1),
        source: AgentSourceCoordinate::new(SOURCE_COORDINATE).expect("source"),
        service_instance_id: AgentServiceInstanceId::new(SERVICE_INSTANCE_ID)
            .expect("service instance"),
        profile_digest: AgentProfileDigest::new(PROFILE_DIGEST).expect("profile"),
        bound_surface_revision: AgentSurfaceRevision(1),
        bound_surface_digest: AgentSurfaceDigest::new("product-tools-surface")
            .expect("bound surface"),
        bound_surface_offer_profile_digest: AgentProfileDigest::new(PROFILE_DIGEST)
            .expect("offer profile"),
        applied_surface_revision: AgentSurfaceRevision(1),
        applied_surface_digest: AgentSurfaceDigest::new("product-tools-surface")
            .expect("applied surface"),
    }
}

fn assert_workspace_definition(
    definitions: &[agentdash_agent_runtime::RuntimeToolDefinition],
    name: &str,
    permission: RuntimeToolPermission,
    effect: RuntimeToolEffect,
) {
    let definition = definitions
        .iter()
        .find(|definition| definition.name.as_str() == name)
        .expect("Workspace tool definition");
    assert_eq!(definition.permission, permission);
    assert_eq!(definition.effect, effect);
    assert_eq!(definition.parameters_schema["type"], "object");
}

fn runtime_tool_name(kind: ProductRuntimeToolKind) -> &'static str {
    match kind {
        ProductRuntimeToolKind::Wait => "wait",
        ProductRuntimeToolKind::CompleteLifecycleNode => "complete_lifecycle_node",
        ProductRuntimeToolKind::CompanionRequest => "companion_request",
        ProductRuntimeToolKind::CompanionRespond => "companion_respond",
        ProductRuntimeToolKind::WorkspaceModuleList => "workspace_module_list",
        ProductRuntimeToolKind::WorkspaceModuleDescribe => "workspace_module_describe",
        ProductRuntimeToolKind::WorkspaceModuleOperate => "workspace_module_operate",
        ProductRuntimeToolKind::WorkspaceModuleInvoke => "workspace_module_invoke",
    }
}
