use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use agentdash_agent_runtime::{
    PlatformToolBroker, RuntimeToolAppliedSurfaceEvidence, RuntimeToolAuthorizationGrant,
    RuntimeToolAuthorizationPolicy, RuntimeToolAuthorizationPort, RuntimeToolAuthorizationRequest,
    RuntimeToolBrokerError, RuntimeToolEffect, RuntimeToolExecutor, RuntimeToolPermission,
    RuntimeToolProductTarget, RuntimeToolProvenanceEvidence, RuntimeToolResourceGrant,
    RuntimeVfsExecutionGrant, RuntimeVfsGrantedOperation, RuntimeVfsMountGrant,
    RuntimeVfsPathGrant,
};
use agentdash_agent_runtime_contract::RuntimeThreadId;
use agentdash_agent_runtime_contract::*;
use agentdash_agent_runtime_host::{
    CompleteAgentBindingId, CompleteAgentCallbackBroker, CompleteAgentHookHandler,
    CompleteAgentHost, CompleteAgentPlacement, CompleteAgentRuntimeTarget,
    CompleteAgentRuntimeTargetProvisioningRequest, CompleteAgentServiceVerification,
    CompleteAgentToolHandler, CompleteAgentVerificationMethod, CompleteAgentVerifiedBuildEvidence,
    CompleteAgentVerifiedServiceRegistration, ProcessCompleteAgentLiveCatalog,
    ResolvedCompleteAgentCallbackContext, ResolvedCompleteAgentHookCallback,
    ResolvedCompleteAgentToolCallback, RuntimePlatformToolHandler,
};
use agentdash_application_ports::product_runtime_tool::{
    ProductRuntimeToolKind, ProductRuntimeToolOutcome, ProductRuntimeToolRequest,
    ProductRuntimeToolService,
};
use agentdash_application_vfs::tools::{
    ShellTerminalOutputSnapshot, ShellTerminalRegistration, ShellTerminalRegistry,
};
use agentdash_application_vfs::{AppliedVfsRuntimeToolService, MountProviderRegistry, VfsService};
use agentdash_infrastructure::mcp::{
    ProductionRuntimeMcpToolCatalog, RuntimeDynamicToolCatalog, RuntimeMcpToolCatalogRequest,
    runtime_mcp_capability_key,
};
use agentdash_infrastructure::{ShellExecRuntimeTool, product_runtime_tool_catalog};
use agentdash_platform_spi::platform::mcp_relay::{RelayProbeResult, RelayProbeTarget};
use agentdash_platform_spi::{
    CapabilityState, McpRelayProvider, McpTransportConfig, PlatformRuntimeError,
    RelayMcpCallContext, RelayMcpCallResult, RelayMcpListOutcome, RelayMcpSourceOutcome,
    RelayMcpToolInfo, RuntimeMcpServer, ToolCapability,
};
use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::sync::Mutex;
use uuid::Uuid;

const RUNTIME_THREAD_ID: &str = "runtime-thread-product-tracer";
const CALLBACK_ROUTE_ID: &str = "product-tools-route";
const SOURCE_COORDINATE: &str = "product-tools-source";
const SERVICE_INSTANCE_ID: &str = "product-tools-service";
const PROFILE_DIGEST: &str = "product-tools-profile";

async fn tool_events(
    mut stream: Box<dyn AgentToolExecutionStream>,
) -> Vec<AgentToolExecutionEvent> {
    let mut events = Vec::new();
    loop {
        match stream.next().await.unwrap() {
            Some(event) => {
                let terminal = matches!(event, AgentToolExecutionEvent::Completed { .. });
                events.push(event);
                if terminal {
                    return events;
                }
            }
            None => return events,
        }
    }
}

async fn terminal_tool_result(stream: Box<dyn AgentToolExecutionStream>) -> AgentToolResult {
    match tool_events(stream).await.pop() {
        Some(AgentToolExecutionEvent::Completed { result }) => result,
        _ => panic!("tool stream ended before completion"),
    }
}

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
                host_binding_generation: Some(1),
            },
            resources: if request.definition.authorization_policy
                == RuntimeToolAuthorizationPolicy::VfsShell
            {
                RuntimeToolResourceGrant::Vfs(RuntimeVfsExecutionGrant {
                    default_mount_id: Some("main".to_owned()),
                    mounts: vec![RuntimeVfsMountGrant {
                        id: "main".to_owned(),
                        provider: "memory".to_owned(),
                        backend_id: "memory".to_owned(),
                        root_ref: "memory://main".to_owned(),
                        display_name: "Main".to_owned(),
                        metadata: Value::Null,
                        operations: vec![RuntimeVfsGrantedOperation::Execute],
                        path_scopes: vec![RuntimeVfsPathGrant::All],
                    }],
                })
            } else {
                RuntimeToolResourceGrant::Product
            },
        })
    }
}

struct NoopTerminalRegistry;

impl ShellTerminalRegistry for NoopTerminalRegistry {
    fn register_shell_terminal(&self, _: ShellTerminalRegistration) {}

    fn resolve_shell_terminal(&self, _: &str) -> Option<ShellTerminalRegistration> {
        None
    }

    fn record_shell_terminal_output_snapshot(&self, _: ShellTerminalOutputSnapshot<'_>) {}

    fn remove_shell_terminal(&self, _: &str) {}
}

#[derive(Default)]
struct ProductionRelay {
    calls: AtomicUsize,
}

#[async_trait]
impl McpRelayProvider for ProductionRelay {
    async fn list_relay_tools(
        &self,
        requested_servers: &[RuntimeMcpServer],
        _: Option<RelayMcpCallContext>,
    ) -> RelayMcpListOutcome {
        let server = requested_servers[0].clone();
        RelayMcpListOutcome {
            tools: vec![RelayMcpToolInfo {
                server_name: server.name.clone(),
                server: server.clone(),
                tool_name: "search".to_owned(),
                description: "Search runtime documentation".to_owned(),
                parameters_schema: json!({"type": "object"}),
            }],
            sources: vec![RelayMcpSourceOutcome::ready(server, 1)],
        }
    }

    async fn call_relay_tool(
        &self,
        _: &RuntimeMcpServer,
        tool_name: &str,
        _: Option<serde_json::Map<String, Value>>,
        _: Option<RelayMcpCallContext>,
    ) -> Result<RelayMcpCallResult, PlatformRuntimeError> {
        assert_eq!(tool_name, "search");
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(RelayMcpCallResult {
            content: "runtime docs".to_owned(),
            is_error: false,
        })
    }

    async fn probe_transport(
        &self,
        _: &agentdash_domain::mcp_preset::McpTransportConfig,
        _: RelayProbeTarget,
    ) -> Result<RelayProbeResult, PlatformRuntimeError> {
        unreachable!("production catalog composition test does not probe")
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
    ) -> Result<AgentHookOutcome, AgentHostCallbackError> {
        Ok(AgentHookOutcome::allow())
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
    let first_request = terminal_tool_result(first_request).await;

    let restarted_broker = complete_agent_callback_broker(services.clone(), host.clone());
    let retried_request = restarted_broker
        .invoke_tool(request_call)
        .await
        .expect("Companion request callback retry");
    let retried_request = terminal_tool_result(retried_request).await;
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
    let first_response = terminal_tool_result(first_response).await;

    let second_restart = complete_agent_callback_broker(services, host);
    let retried_response = second_restart
        .invoke_tool(respond_call)
        .await
        .expect("Companion response callback retry");
    let retried_response = terminal_tool_result(retried_response).await;
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
    let operate_service = Arc::new(RecordingProductToolService::new(
        ProductRuntimeToolKind::WorkspaceModuleOperate,
    ));
    let invoke_service = Arc::new(RecordingProductToolService::new(
        ProductRuntimeToolKind::WorkspaceModuleInvoke,
    ));
    let present_service = Arc::new(RecordingProductToolService::new(
        ProductRuntimeToolKind::WorkspaceModulePresent,
    ));
    let services: Vec<Arc<dyn ProductRuntimeToolService>> = vec![
        list_service.clone(),
        describe_service.clone(),
        operate_service.clone(),
        invoke_service.clone(),
        present_service.clone(),
    ];
    let executors = product_runtime_tool_catalog(services);
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
            "workspace-operate-effect",
            "workspace-operate-callback",
            json!({
                "operation": "canvas.create",
                "input": {"title": "Tracer Canvas"}
            }),
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
        let stream = handler
            .invoke(ResolvedCompleteAgentToolCallback {
                context: resolved_callback_context(),
                invocation: callback_call(tool, effect, callback, arguments),
            })
            .await
            .expect("Workspace callback handler");
        let result = terminal_tool_result(stream).await;
        assert!(matches!(result, AgentToolResult::Completed { .. }));
    }

    let presentation_stream = handler
        .invoke(ResolvedCompleteAgentToolCallback {
            context: resolved_callback_context(),
            invocation: callback_call(
                "workspace_module_present",
                "workspace-present-effect",
                "workspace-present-callback",
                json!({
                    "module_id": "canvas:tracer",
                    "view_key": "default",
                    "payload": {"source": "tracer"}
                }),
            ),
        })
        .await
        .expect("Workspace presentation callback");
    let presentation = terminal_tool_result(presentation_stream).await;
    let AgentToolResult::Completed { output } = presentation else {
        panic!("Workspace presentation must complete");
    };
    assert_eq!(output["arguments"]["module_id"], "canvas:tracer");
    assert!(output["arguments"].get("renderer_kind").is_none());
    assert!(output["arguments"].get("presentation_uri").is_none());

    assert_eq!(list_service.calls.load(Ordering::SeqCst), 1);
    assert_eq!(describe_service.calls.load(Ordering::SeqCst), 1);
    assert_eq!(invoke_service.calls.load(Ordering::SeqCst), 1);
    assert_eq!(present_service.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        invoke_service.requests.lock().await[0].context.effect_id,
        "workspace-invoke-effect"
    );
}

#[tokio::test]
async fn production_platform_shell_runs_through_the_typed_host_stream() {
    let vfs = Arc::new(AppliedVfsRuntimeToolService::new(
        Arc::new(VfsService::new(Arc::new(MountProviderRegistry::new()))),
        Arc::new(NoopTerminalRegistry),
    ));
    let executor = Arc::new(ShellExecRuntimeTool::new(vfs)) as Arc<dyn RuntimeToolExecutor>;
    let broker = Arc::new(
        PlatformToolBroker::new([executor], product_authorizer()).expect("shell production broker"),
    );
    let handler = RuntimePlatformToolHandler::new(broker);

    let events = tool_events(
        handler
            .invoke(ResolvedCompleteAgentToolCallback {
                context: resolved_callback_context(),
                invocation: callback_call(
                    "shell_exec",
                    "shell-effect",
                    "shell-callback",
                    json!({"command": "echo production-shell"}),
                ),
            })
            .await
            .expect("shell callback stream"),
    )
    .await;

    assert!(matches!(
        events.first(),
        Some(AgentToolExecutionEvent::Started)
    ));
    assert!(matches!(
        events.last(),
        Some(AgentToolExecutionEvent::Completed {
            result: AgentToolResult::Completed { output }
        }) if output.to_string().contains("production-shell")
    ));
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, AgentToolExecutionEvent::Completed { .. }))
            .count(),
        1
    );
}

#[tokio::test]
async fn production_relay_mcp_tool_runs_through_the_typed_host_stream() {
    let relay = Arc::new(ProductionRelay::default());
    let catalog = ProductionRuntimeMcpToolCatalog::new(Some(relay.clone()));
    let mut capability_state = CapabilityState::default();
    capability_state
        .tool
        .capabilities
        .insert(ToolCapability::new(runtime_mcp_capability_key("docs")));
    let executors = catalog
        .resolve(RuntimeMcpToolCatalogRequest {
            servers: vec![RuntimeMcpServer::new(
                "docs".to_owned(),
                McpTransportConfig::Stdio {
                    command: "unused".to_owned(),
                    args: Vec::new(),
                    env: Vec::new(),
                    cwd: None,
                },
                true,
            )],
            capability_state,
            relay_context: None,
        })
        .await
        .expect("production MCP catalog");
    assert_eq!(executors.len(), 1);
    let broker = Arc::new(
        PlatformToolBroker::new(executors, product_authorizer()).expect("MCP production broker"),
    );
    let handler = RuntimePlatformToolHandler::new(broker);

    let events = tool_events(
        handler
            .invoke(ResolvedCompleteAgentToolCallback {
                context: resolved_callback_context(),
                invocation: callback_call(
                    "mcp_docs_search",
                    "mcp-effect",
                    "mcp-callback",
                    json!({"query": "runtime"}),
                ),
            })
            .await
            .expect("MCP callback stream"),
    )
    .await;

    assert!(matches!(events.as_slice(), [
        AgentToolExecutionEvent::Started,
        AgentToolExecutionEvent::Completed {
            result: AgentToolResult::Completed { output }
        }
    ] if output["content"] == "runtime docs"));
    assert_eq!(relay.calls.load(Ordering::SeqCst), 1);
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
                    provenance: agentdash_agent_runtime_contract::AgentToolProvenance {
                        capability_key: format!("test/{name}"),
                        source: "test".to_owned(),
                        tool_path: format!("test/{name}::{name}"),
                        context_usage_kind: "system_tools".to_owned(),
                    },
                    protocol_projector: agentdash_agent_protocol::ToolProtocolProjector::Dynamic,
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
        ProductRuntimeToolKind::WorkspaceModulePresent => "workspace_module_present",
        ProductRuntimeToolKind::OperationScript => "operation_script",
    }
}
