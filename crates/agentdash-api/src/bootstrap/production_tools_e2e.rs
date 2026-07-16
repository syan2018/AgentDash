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
use agentdash_agent_runtime::RuntimeRepository;
use agentdash_agent_runtime_contract::*;
use agentdash_agent_runtime_host::{
    ActivateAgentServiceInstance, ConformanceEvidence, PutAgentServiceInstance,
    RuntimeBindingState, ServiceInstanceDesiredState, TrustedDriverManifest, profile_digest,
};
use agentdash_application::{
    companion::ApplicationWorkflowScriptPreflightAdapter,
    runtime_tools::{
        CollaborationRuntimeToolProvider, SessionRuntimeToolComposer,
        SharedSessionToolServicesHandle, TaskRuntimeToolProvider, VfsRuntimeToolProvider,
        WorkflowRuntimeToolProvider,
    },
    wait_activity::{WaitActivityDeps, WaitActivityService, WaitRuntimeToolProvider},
};
use agentdash_application_agentrun::agent_run::{
    AgentBusinessSurfaceContextDeps, AgentBusinessSurfaceSource, AgentRunPresentationDraft,
    AgentRunRuntime, AgentRunTerminalRegistry, BaseIdentitySource, BusinessFrameSurfaceQuery,
    BusinessFrameSurfaceQueryDeps, EnqueueRuntimeMailboxMessage, LaunchPresentationSource,
    ManagedAgentRunRuntime, RuntimeAgentRunMailbox, RuntimeMailboxSubmitOutcome,
    SendAgentRunMessage,
};
use agentdash_application_lifecycle::lifecycle::tools::SharedSessionToolServicesHandle as LifecycleSessionToolServicesHandle;
use agentdash_application_ports::{
    agent_frame_hook_plan::{AgentFrameHookPlan, SharedAgentFrameHookPlanCompiler},
    agent_frame_materialization::SharedAgentRunFrameConstructionHandle,
    agent_run_runtime::{AgentRunRuntimeTarget, SharedAgentRunRuntimeProvisionerHandle},
};
use agentdash_application_vfs::{MountProviderRegistryBuilder, PROVIDER_INLINE_FS, VfsService};
use agentdash_domain::{
    agent_run_mailbox::{MailboxMessageOrigin, MailboxSourceIdentity},
    workflow::{
        AgentFrame, AgentFrameSurfaceDocument, AgentSource, ApiRequestExecutorSpec,
        BashExecExecutorSpec, LifecycleAgent, LifecycleRun,
    },
};
use agentdash_infrastructure::postgres_runtime::PostgresRuntime;
use agentdash_integration_api::*;
use agentdash_integration_native_agent::{
    NativeAgentRuntimeIntegration, NativeBridgeResolveError, NativeBridgeResolver,
    NativePresentationMetadata, ResolvedNativeBridge, native_runtime_profile,
    native_runtime_trust_manifest,
};
use agentdash_spi::{
    AgentConfig, ApiRequestOutcome, BashExecOutcome, CapabilityState, FunctionRunner, Mount,
    MountCapability, NoopExecutionHookProvider, ToolCapability, ToolCluster, Vfs,
    WorkflowScriptEvaluator, WorkspaceModuleDimension, connector::RuntimeToolProvider,
};
use agentdash_test_support::workflow::MemoryAgentRunMailboxRepository;
use agentdash_workspace_module::workspace_module::{
    SharedWorkspaceModuleAgentRunBridgeHandle, SharedWorkspaceModulePresentationAppendHandle,
    SharedWorkspaceModuleRuntimeGatewayHandle, WorkspaceModuleRuntimeToolProvider,
};
use async_trait::async_trait;
use chrono::Utc;
use futures::{Stream, stream};
use serde_json::json;
use uuid::Uuid;

use super::{
    agent_runtime::{
        AgentRuntimeCallbacks, AgentRuntimeCompositionInput, NativeAgentRunRuntimeSurfaceSource,
        PlatformAgentRuntimeToolCallback, build_agent_runtime_composition,
    },
    agent_runtime_surface::{
        AgentFrameSurfaceCompositionAdapter, CanonicalCompiledAgentRunToolBindingRecovery,
        CompiledAgentRunToolRegistry, PostgresAgentRunToolBrokerResolver,
    },
    repositories::build_repositories,
};

#[derive(Default)]
struct ProductionToolsBridge {
    calls: AtomicUsize,
    requests: tokio::sync::Mutex<Vec<Vec<AgentMessage>>>,
}

#[async_trait]
impl LlmBridge for ProductionToolsBridge {
    async fn stream_complete(
        &self,
        request: BridgeRequest,
    ) -> Pin<Box<dyn Stream<Item = StreamChunk> + Send>> {
        self.requests.lock().await.push(request.messages);
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        let message = if call == 0 {
            AgentMessage::Assistant {
                content: Vec::new(),
                tool_calls: vec![
                    ToolCallInfo {
                        id: "production-mounts-list".to_string(),
                        call_id: None,
                        name: "mounts_list".to_string(),
                        arguments: json!({}),
                    },
                    ToolCallInfo {
                        id: "production-workflow-terminal".to_string(),
                        call_id: None,
                        name: "complete_lifecycle_node".to_string(),
                        arguments: json!({
                            "outcome": "failed",
                            "summary": "production provider bridge probe"
                        }),
                    },
                    ToolCallInfo {
                        id: "production-companion-preflight".to_string(),
                        call_id: None,
                        name: "companion_request".to_string(),
                        arguments: json!({
                            "target": "platform",
                            "wait": false,
                            "payload": {
                                "type": "workflow_script_preflight",
                                "source_text": "production provider bridge probe"
                            }
                        }),
                    },
                    ToolCallInfo {
                        id: "production-task-read".to_string(),
                        call_id: None,
                        name: "task_read".to_string(),
                        arguments: json!({ "mode": "overview", "format": "compact" }),
                    },
                    ToolCallInfo {
                        id: "production-workspace-module-list".to_string(),
                        call_id: None,
                        name: "workspace_module_list".to_string(),
                        arguments: json!({}),
                    },
                    ToolCallInfo {
                        id: "production-wait".to_string(),
                        call_id: None,
                        name: "wait".to_string(),
                        arguments: json!({
                            "activity_refs": [],
                            "kinds": [],
                            "timeout_ms": 0,
                            "max_items": 10,
                            "after_cursor": null
                        }),
                    },
                ],
                stop_reason: Some(StopReason::ToolUse),
                error_message: None,
                usage: None,
                timestamp: None,
            }
        } else if call == 2 {
            AgentMessage::Assistant {
                content: Vec::new(),
                tool_calls: vec![ToolCallInfo {
                    id: "production-recovery-task-read".to_string(),
                    call_id: None,
                    name: "task_read".to_string(),
                    arguments: json!({ "mode": "overview", "format": "compact" }),
                }],
                stop_reason: Some(StopReason::ToolUse),
                error_message: None,
                usage: None,
                timestamp: None,
            }
        } else {
            AgentMessage::Assistant {
                content: vec![ContentPart::text(if call == 1 {
                    "production tools completed"
                } else {
                    "production cold rebind completed"
                })],
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

struct ProductionToolsBridgeResolver(Arc<ProductionToolsBridge>);

#[async_trait]
impl NativeBridgeResolver for ProductionToolsBridgeResolver {
    async fn resolve(
        &self,
        _instance: &ActivatedAgentServiceInstance,
        _host: &RuntimeDriverHostPorts,
    ) -> Result<ResolvedNativeBridge, NativeBridgeResolveError> {
        Ok(ResolvedNativeBridge {
            bridge: self.0.clone(),
            presentation: NativePresentationMetadata {
                model_context_window: 200_000,
                reserve_tokens: 0,
            },
        })
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
            reason: "production tool E2E has no credential slots".to_string(),
        })
    }
}

struct ContinueHooks;

struct RejectingFunctionRunner;

struct ProductionWorkflowScriptEvaluator;

impl WorkflowScriptEvaluator for ProductionWorkflowScriptEvaluator {
    fn validate_workflow_script(&self, _script: &str) -> Result<(), Vec<String>> {
        Ok(())
    }

    fn eval_workflow_script(
        &self,
        _script: &str,
        _ctx: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        Ok(json!({
            "kind": "workflow",
            "body": [{
                "kind": "agent",
                "name": "production_provider_probe",
                "procedure": "production.provider.probe"
            }]
        }))
    }
}

#[async_trait]
impl FunctionRunner for RejectingFunctionRunner {
    async fn run_api_request(
        &self,
        _spec: &ApiRequestExecutorSpec,
        _context: &serde_json::Value,
    ) -> Result<ApiRequestOutcome, String> {
        Err("production provider E2E has no external transport".to_string())
    }

    async fn run_bash(
        &self,
        _spec: &BashExecExecutorSpec,
        _context: &serde_json::Value,
    ) -> Result<BashExecOutcome, String> {
        Err("production provider E2E has no external process".to_string())
    }
}

#[async_trait]
impl AgentRuntimeHookCallback for ContinueHooks {
    async fn execute(
        &self,
        request: DriverHookInvocation,
    ) -> Result<DriverHookDecision, DriverHookCallbackError> {
        Ok(DriverHookDecision::Continue {
            payload: request.payload,
        })
    }
}

async fn migrated_pool() -> (sqlx::PgPool, PostgresRuntime) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/production-tools-e2e")
        .join(Uuid::new_v4().simple().to_string());
    let runtime = PostgresRuntime::resolve_embedded_at_data_root("production-tools-e2e", 8, root)
        .await
        .expect("embedded PostgreSQL");
    agentdash_infrastructure::migration::run_postgres_migrations(&runtime.pool)
        .await
        .expect("migrations");
    (runtime.pool.clone(), runtime)
}

async fn seed_agent_frame_surface(
    repos: &agentdash_application::repository_set::RepositorySet,
) -> (AgentRunRuntimeTarget, Uuid, Uuid) {
    let project_id = Uuid::new_v4();
    let run = LifecycleRun::new_plain(project_id);
    repos
        .lifecycle_run_repo
        .create(&run)
        .await
        .expect("seed LifecycleRun");
    let agent = LifecycleAgent::new_root(run.id, project_id, AgentSource::ProjectAgent);
    repos
        .lifecycle_agent_repo
        .create(&agent)
        .await
        .expect("seed LifecycleAgent");

    let mut capability_state = CapabilityState::from_clusters([
        ToolCluster::Read,
        ToolCluster::Write,
        ToolCluster::Execute,
        ToolCluster::Workflow,
        ToolCluster::Collaboration,
        ToolCluster::Task,
        ToolCluster::WorkspaceModule,
    ]);
    capability_state.workspace_module = WorkspaceModuleDimension::all();
    for key in [
        "file_read",
        "file_write",
        "shell_execute",
        "workflow",
        "collaboration",
        "task",
        "workspace_module",
    ] {
        capability_state
            .tool
            .capabilities
            .insert(ToolCapability::new(key));
    }
    let mut executor = AgentConfig::new("PI_AGENT");
    executor.provider_id = Some("production-e2e".to_string());
    executor.model_id = Some("scripted-tools".to_string());
    let hook_plan = AgentFrameHookPlan::compile(HookPlanRevision(1), Vec::new())
        .expect("empty immutable HookPlan");
    let vfs = Vfs {
        mounts: vec![Mount {
            id: "main".to_string(),
            provider: PROVIDER_INLINE_FS.to_string(),
            backend_id: String::new(),
            root_ref: "context://inline/production-tools-e2e".to_string(),
            capabilities: vec![
                MountCapability::Read,
                MountCapability::List,
                MountCapability::Search,
            ],
            default_write: false,
            display_name: "Production tool E2E".to_string(),
            metadata: json!({
                "container_id": "production-tools-e2e",
                "agentdash_context_owner_kind": "project",
                "agentdash_context_owner_id": project_id,
            }),
        }],
        default_mount_id: Some("main".to_string()),
        source_project_id: Some(project_id.to_string()),
        ..Default::default()
    };
    let mut frame = AgentFrame::new_revision(agent.id, 1, "production_tools_e2e");
    frame.surface = Some(AgentFrameSurfaceDocument {
        capability_state: Some(serde_json::to_value(capability_state).unwrap()),
        vfs_surface: Some(serde_json::to_value(vfs).unwrap()),
        mcp_surface: Some(json!([])),
        execution_profile: Some(serde_json::to_value(executor).unwrap()),
        hook_plan: Some(serde_json::to_value(hook_plan).unwrap()),
        visible_workspace_module_refs: Some(json!([])),
        ..Default::default()
    });
    frame.apply_surface_projection();
    repos
        .agent_frame_repo
        .create(&frame)
        .await
        .expect("seed canonical AgentFrame surface");

    (
        AgentRunRuntimeTarget {
            run_id: run.id,
            agent_id: agent.id,
        },
        project_id,
        frame.id,
    )
}

fn trusted_native_manifest() -> TrustedDriverManifest {
    let manifest = native_runtime_trust_manifest();
    TrustedDriverManifest {
        provenance: manifest.provenance,
        suite_revision: manifest.suite_revision,
        driver_build_digest: manifest.driver_build_digest,
        protocol_revision: manifest.protocol_revision,
        verified_profile_digest: profile_digest(&manifest.verified_profile)
            .expect("native profile digest"),
    }
}

#[tokio::test]
async fn real_agent_frame_task_and_workspace_tools_continue_to_final_assistant() {
    let (pool, _postgres) = migrated_pool().await;
    let repositories = build_repositories(
        pool.clone(),
        Vec::new(),
        None,
        SharedAgentRunRuntimeProvisionerHandle::default(),
        SharedAgentRunFrameConstructionHandle::default(),
        SharedAgentFrameHookPlanCompiler::default(),
    )
    .await
    .expect("production repositories");
    let repos = repositories.repos;
    let (target, project_id, frame_id) = seed_agent_frame_surface(&repos).await;

    let vfs_service = Arc::new(VfsService::new(Arc::new(
        MountProviderRegistryBuilder::new().build(),
    )));
    let terminal_registry = AgentRunTerminalRegistry::new();
    let wait_service = WaitActivityService::new(WaitActivityDeps {
        repositories: repos.wait_activity_repositories(),
        terminal_registry: terminal_registry.clone(),
    });
    let runtime_tools: Arc<dyn RuntimeToolProvider> =
        Arc::new(SessionRuntimeToolComposer::from_final_catalog_providers([
            Arc::new(VfsRuntimeToolProvider::new(
                vfs_service.clone(),
                None,
                crate::bootstrap::runtime_tools::build_shell_terminal_registry_adapter(
                    terminal_registry,
                ),
            )),
            Arc::new(WorkflowRuntimeToolProvider::new(
                repos.lifecycle_orchestrator_deps(),
                LifecycleSessionToolServicesHandle,
                Arc::new(RejectingFunctionRunner),
            )),
            Arc::new(
                CollaborationRuntimeToolProvider::new(
                    repos.clone(),
                    SharedSessionToolServicesHandle::default(),
                )
                .with_wait_service(wait_service.clone())
                .with_workflow_script_preflight(Arc::new(
                    ApplicationWorkflowScriptPreflightAdapter::new(Arc::new(
                        ProductionWorkflowScriptEvaluator,
                    )),
                )),
            ),
            Arc::new(TaskRuntimeToolProvider::new(repos.clone())),
            Arc::new(WaitRuntimeToolProvider::from_service(wait_service)),
            Arc::new(
                WorkspaceModuleRuntimeToolProvider::new(
                    repos.project_extension_installation_repo.clone(),
                    repos.project_repo.clone(),
                    repos.canvas_repo.clone(),
                    repos.canvas_runtime_state_repo.clone(),
                    repos.agent_run_runtime_binding_repo.clone(),
                    SharedWorkspaceModuleAgentRunBridgeHandle::default(),
                    SharedWorkspaceModuleRuntimeGatewayHandle::default(),
                )
                .with_presentation_append_handle(
                    SharedWorkspaceModulePresentationAppendHandle::default(),
                ),
            ),
        ]));
    let surface_query = Arc::new(BusinessFrameSurfaceQuery::new(
        BusinessFrameSurfaceQueryDeps {
            binding_repo: repos.agent_run_runtime_binding_repo.clone(),
            run_repo: repos.lifecycle_run_repo.clone(),
            agent_repo: repos.lifecycle_agent_repo.clone(),
            frame_repo: repos.agent_frame_repo.clone(),
            permission_grant_repo: repos.permission_grant_repo.clone(),
        },
    ));
    let business_surface_source = Arc::new(AgentBusinessSurfaceSource::new(
        surface_query.clone(),
        repos.agent_frame_repo.clone(),
        runtime_tools,
        Arc::new(NoopExecutionHookProvider),
        AgentBusinessSurfaceContextDeps {
            vfs_service,
            extra_skill_dirs: Vec::new(),
            skill_discovery_providers: Vec::new(),
            memory_discovery_providers: Vec::new(),
            settings_repository: repos.settings_repo.clone(),
            base_identity: BaseIdentitySource::new("Production tool E2E"),
        },
    ));
    let tool_registry = Arc::new(CompiledAgentRunToolRegistry::default());
    let surface_compiler = Arc::new(AgentFrameSurfaceCompositionAdapter::new(
        business_surface_source,
        tool_registry.clone(),
    ));
    tool_registry
        .bind_recovery(Arc::new(
            CanonicalCompiledAgentRunToolBindingRecovery::new(
                Arc::downgrade(&surface_compiler),
                Arc::new(
                    agentdash_infrastructure::persistence::postgres::PostgresAgentRuntimeCompositionRepository::new(
                        pool.clone(),
                    ),
                ),
            ),
        ))
        .expect("configure canonical compiled binding recovery");

    let bridge = Arc::new(ProductionToolsBridge::default());
    let integration =
        NativeAgentRuntimeIntegration::new(Arc::new(ProductionToolsBridgeResolver(bridge.clone())));
    let mut contributions = integration.agent_runtime_drivers();
    let contribution = contributions.pop().expect("Native Runtime contribution");
    assert!(contributions.is_empty());
    let surface_source = Arc::new(
        NativeAgentRunRuntimeSurfaceSource::new(
            surface_compiler,
            contribution.definition.clone(),
            Vec::new(),
        )
        .expect("Native production surface source"),
    );
    let callback_pool = pool.clone();
    let callback_registry = tool_registry.clone();
    let callback_capability = surface_query.clone();
    let composition = build_agent_runtime_composition(AgentRuntimeCompositionInput {
        pool: pool.clone(),
        contributions: vec![contribution],
        trusted_manifests: vec![trusted_native_manifest()],
        surface_source,
        credential_broker: Arc::new(NoCredentials),
        callback_factory: Arc::new(move |runtime| AgentRuntimeCallbacks {
            tools: Arc::new(PlatformAgentRuntimeToolCallback::new(Arc::new(
                PostgresAgentRunToolBrokerResolver::new(
                    callback_pool.clone(),
                    runtime,
                    callback_registry.clone(),
                    callback_capability.clone(),
                ),
            ))),
            hooks: Arc::new(ContinueHooks),
        }),
        application_presentation_projector: Arc::new(
            agentdash_application_agentrun::agent_run::AgentRunRuntimeApplicationPresentationProjector,
        ),
        managed_compaction: None,
        node_id: "production-tools-e2e".to_string(),
    })
    .expect("production Runtime composition");
    let runtime: Arc<dyn AgentRunRuntime> = Arc::new(ManagedAgentRunRuntime::new(
        composition.gateway.clone(),
        composition.bindings.clone(),
        composition.provisioner.clone(),
        composition.presentation_plans.clone(),
        tool_registry.clone(),
    ));
    let mailbox_repository = Arc::new(MemoryAgentRunMailboxRepository::default());
    let mailbox = RuntimeAgentRunMailbox::new(
        mailbox_repository.clone(),
        runtime.clone(),
        Arc::new(
            agentdash_test_support::workflow::MemoryAgentRunMessageSubmissionStore::new(
                mailbox_repository,
            ),
        ),
    );
    let presentation_thread_id =
        PresentationThreadId::new("production-tools-presentation").unwrap();
    let submitted = mailbox
        .submit(EnqueueRuntimeMailboxMessage {
            target: target.clone(),
            presentation_thread_id: presentation_thread_id.clone(),
            presentation: AgentRunPresentationDraft {
                content: agentdash_agent_protocol::text_user_input_blocks(
                    "exercise every production tool provider and continue",
                ),
                source: agentdash_agent_protocol::UserInputSource::core_composer(),
                launch_source: LaunchPresentationSource::HttpPrompt,
                submission_kind: agentdash_agent_protocol::UserInputSubmissionKind::Prompt,
            },
            client_command_id: "production-tools-first-message".to_string(),
            input: vec![RuntimeInput::text(
                "exercise every production tool provider and continue".to_string(),
            )],
            actor: RuntimeActor::User {
                subject: "production-tools-e2e".to_string(),
            },
            identity: None,
            origin: MailboxMessageOrigin::User,
            source: MailboxSourceIdentity::composer(),
            delivery_intent: None,
            executor_config: None,
            backend_selection: None,
        })
        .await
        .expect("submit production tool turn");
    let receipt = match submitted {
        RuntimeMailboxSubmitOutcome::Dispatched { receipt, .. } => receipt,
        RuntimeMailboxSubmitOutcome::Queued { .. } => panic!("idle Runtime must dispatch"),
    };
    assert_eq!(
        composition
            .outbox_worker
            .run_once(8)
            .await
            .expect("dispatch production tool turn"),
        1
    );

    let view = tokio::time::timeout(std::time::Duration::from_secs(20), async {
        loop {
            let view = runtime
                .inspect(target.clone())
                .await
                .expect("inspect Runtime");
            if bridge.calls.load(Ordering::SeqCst) == 2
                && view
                    .snapshot
                    .as_ref()
                    .is_some_and(|snapshot| snapshot.active_turn_id.is_none())
            {
                break view;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap_or_else(|_| {
        panic!(
            "production tool continuation timed out: bridge_calls={}",
            bridge.calls.load(Ordering::SeqCst)
        )
    });
    let binding = view.binding.expect("canonical Runtime binding");
    let snapshot = view.snapshot.expect("canonical Runtime snapshot");
    assert_eq!(snapshot.status, RuntimeThreadStatus::Active);
    assert!(snapshot.active_turn_id.is_none());
    assert_ne!(binding.thread_id.as_str(), presentation_thread_id.as_str());

    let expected_tool_names = BTreeSet::from([
        "mounts_list".to_string(),
        "complete_lifecycle_node".to_string(),
        "companion_request".to_string(),
        "task_read".to_string(),
        "wait".to_string(),
        "workspace_module_list".to_string(),
    ]);
    let requests = bridge.requests.lock().await;
    assert_eq!(requests.len(), 2);
    let tool_results = requests[1]
        .iter()
        .filter_map(|message| match message {
            AgentMessage::ToolResult {
                tool_name,
                content,
                details,
                is_error,
                ..
            } => Some((tool_name.clone(), content, details, *is_error)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(tool_results.len(), expected_tool_names.len());
    assert_eq!(
        tool_results
            .iter()
            .filter_map(|(name, ..)| name.clone())
            .collect::<BTreeSet<_>>(),
        expected_tool_names
    );
    assert!(tool_results.iter().all(|(name, _, _, is_error)| {
        name.as_deref() == Some("complete_lifecycle_node") || !is_error
    }));
    assert!(
        tool_results.iter().any(|(name, _, _, is_error)| {
            name.as_deref() == Some("complete_lifecycle_node") && *is_error
        }),
        "workflow provider business failure must return to the model instead of aborting the turn"
    );
    let tool_result_diagnostic = format!("{tool_results:#?}");
    for forbidden in [
        "surface-bootstrap",
        "runtime surface query missing anchor",
        "缺少 hook runtime",
    ] {
        assert!(
            !tool_result_diagnostic.contains(forbidden),
            "production provider returned stale bootstrap/anchor failure: {tool_result_diagnostic}"
        );
    }
    drop(requests);

    assert!(snapshot.transcript.iter().any(|item| {
        let agentdash_agent_protocol::BackboneEvent::ItemCompleted(completed) =
            &item.terminal_event.event
        else {
            return false;
        };
        RuntimeItemContent::new(completed.item.clone()).agent_message_text()
            == Some("production tools completed")
    }));

    let compiled = tool_registry
        .get_revision(&binding.binding_id, ToolSetRevision(1))
        .await
        .expect("compiled production tool binding");
    assert_eq!(compiled.run_id, target.run_id);
    assert_eq!(compiled.agent_id, target.agent_id);
    assert_eq!(compiled.frame_id, frame_id);
    assert_eq!(compiled.surface.project_id, project_id);
    assert_eq!(
        compiled.surface.runtime_session_id,
        binding.thread_id.as_str()
    );
    assert_eq!(
        compiled.surface.presentation_thread_id,
        presentation_thread_id
    );
    assert!(compiled.tool_names.contains("task_read"));
    assert!(compiled.tool_names.contains("workspace_module_list"));
    for tool in &expected_tool_names {
        assert!(
            compiled.tool_names.contains(tool),
            "missing compiled provider tool {tool}"
        );
    }

    let records = composition
        .runtime_repository
        .journal_records_after(&binding.thread_id, None)
        .await
        .expect("production Runtime journal")
        .records;
    let mut lifecycle = BTreeMap::<String, Vec<&str>>::new();
    let mut lifecycle_tools = BTreeSet::new();
    for event in records.iter().filter_map(|record| record.as_presentation()) {
        let (phase, item) = match &event.event {
            agentdash_agent_protocol::BackboneEvent::ItemStarted(value) => ("started", &value.item),
            agentdash_agent_protocol::BackboneEvent::ItemCompleted(value) => {
                ("completed", &value.item)
            }
            _ => continue,
        };
        let Some(
            agentdash_agent_protocol::codex_app_server_protocol::ThreadItem::DynamicToolCall {
                id,
                tool,
                ..
            },
        ) = item.as_codex()
        else {
            continue;
        };
        if expected_tool_names.contains(tool) {
            lifecycle.entry(id.clone()).or_default().push(phase);
            lifecycle_tools.insert(tool.clone());
        }
    }
    assert_eq!(lifecycle_tools, expected_tool_names);
    assert_eq!(
        lifecycle.len(),
        expected_tool_names.len(),
        "one card per logical production tool"
    );
    assert!(
        lifecycle
            .values()
            .all(|phases| phases.as_slice() == ["started", "completed"]),
        "each production tool terminal must reuse its started item identity: {lifecycle:?}"
    );

    let operation = composition
        .runtime_repository
        .find_operation(&receipt.operation_id)
        .await
        .expect("read Runtime operation")
        .expect("durable Runtime operation");
    assert_eq!(
        operation.terminal,
        Some(RuntimeOperationTerminal::Succeeded)
    );
    let (attempt_count, dispatched): (i32, bool) = sqlx::query_as(
        "SELECT attempt_count,dispatched_at IS NOT NULL FROM agent_runtime_outbox WHERE operation_id=$1",
    )
    .bind(receipt.operation_id.as_str())
    .fetch_one(&pool)
    .await
    .expect("read Runtime outbox terminal");
    assert_eq!(attempt_count, 1);
    assert!(dispatched);

    let old_presentation_ids = lifecycle.keys().cloned().collect::<BTreeSet<_>>();
    let old_host_binding = composition
        .host
        .binding(&binding.binding_id)
        .await
        .expect("load first Native host binding");
    let old_instance = composition
        .host
        .service_instance(&old_host_binding.service_instance_id)
        .await
        .expect("load first Native service instance")
        .expect("first Native service instance");
    let inactive_instance = composition
        .host
        .deactivate(&old_instance.id, old_instance.revision)
        .await
        .expect("deactivate first Native driver generation");
    composition
        .managed_runtime
        .ingest_driver_event(DriverEventEnvelope {
            binding_id: binding.binding_id.clone(),
            generation: binding.driver_generation,
            operation_id: None,
            source_thread_id: binding.source_thread_id.clone(),
            source_turn_id: None,
            source_item_id: None,
            source_request_id: None,
            source_entry_index: None,
            facts: vec![RuntimeJournalFact::Internal(RuntimeEvent::BindingLost {
                binding_id: binding.binding_id.clone(),
                reason: "production Native generation intentionally retired".to_string(),
            })],
        })
        .await
        .expect("persist first Native binding loss");
    let lost_view = runtime
        .inspect(target.clone())
        .await
        .expect("inspect lost Native Runtime");
    assert_eq!(
        lost_view
            .snapshot
            .expect("lost Native Runtime snapshot")
            .status,
        RuntimeThreadStatus::Lost
    );

    let reenabled_instance = composition
        .host
        .put_instance(PutAgentServiceInstance {
            id: inactive_instance.id.clone(),
            definition_id: inactive_instance.definition_id.clone(),
            config: inactive_instance.config.clone(),
            credentials: inactive_instance.credentials.clone(),
            placement: inactive_instance.placement.clone(),
            desired_state: ServiceInstanceDesiredState::Active,
            expected_revision: Some(inactive_instance.revision),
        })
        .await
        .expect("re-enable Native service instance");
    let manifest = native_runtime_trust_manifest();
    let recovery_profile = native_runtime_profile();
    let replacement_offer = composition
        .host
        .activate(ActivateAgentServiceInstance {
            instance_id: reenabled_instance.id.clone(),
            expected_revision: reenabled_instance.revision,
            transport_profile: recovery_profile.clone(),
            transport_profile_digest: profile_digest(&recovery_profile)
                .expect("replacement Native transport profile digest"),
            host_policy_profile: recovery_profile.clone(),
            host_policy_digest: profile_digest(&recovery_profile)
                .expect("replacement Native host policy digest"),
            conformance: ConformanceEvidence {
                suite_revision: manifest.suite_revision,
                driver_build_digest: manifest.driver_build_digest,
                verified_profile_digest: profile_digest(&manifest.verified_profile)
                    .expect("replacement Native verified profile digest"),
                verified_at: Utc::now(),
            },
        })
        .await
        .expect("activate replacement Native driver generation");
    assert!(
        replacement_offer.generation > old_host_binding.driver_generation,
        "cold recovery must activate a genuinely newer Native driver generation"
    );

    let recovery_receipt = runtime
        .send_message(SendAgentRunMessage {
            target: target.clone(),
            presentation_thread_id: presentation_thread_id.clone(),
            presentation: AgentRunPresentationDraft {
                content: agentdash_agent_protocol::text_user_input_blocks(
                    "read task after Native cold rebind",
                ),
                source: agentdash_agent_protocol::UserInputSource::core_composer(),
                launch_source: LaunchPresentationSource::HttpPrompt,
                submission_kind: agentdash_agent_protocol::UserInputSubmissionKind::Prompt,
            },
            client_command_id: "production-tools-after-native-cold-rebind".to_string(),
            input: vec![RuntimeInput::text(
                "read task after Native cold rebind".to_string(),
            )],
            actor: RuntimeActor::User {
                subject: "production-tools-e2e".to_string(),
            },
            identity: None,
            backend_selection: None,
        })
        .await
        .expect("recover Native binding and accept next prompt");
    assert_eq!(
        composition
            .outbox_worker
            .run_once(8)
            .await
            .expect("dispatch recovered Native prompt"),
        1
    );

    let recovered_view = tokio::time::timeout(std::time::Duration::from_secs(20), async {
        loop {
            let view = runtime
                .inspect(target.clone())
                .await
                .expect("inspect recovered Native Runtime");
            if bridge.calls.load(Ordering::SeqCst) == 4
                && view
                    .snapshot
                    .as_ref()
                    .is_some_and(|snapshot| snapshot.active_turn_id.is_none())
            {
                break view;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("recovered Native tool turn must continue to final assistant");
    let recovered_binding = recovered_view.binding.expect("recovered Native binding");
    let recovered_snapshot = recovered_view.snapshot.expect("recovered Native snapshot");
    assert_eq!(recovered_snapshot.status, RuntimeThreadStatus::Active);
    assert_ne!(recovered_binding.binding_id, binding.binding_id);
    assert_eq!(
        recovered_binding.binding_epoch.0,
        binding.binding_epoch.0 + 1
    );
    assert_eq!(recovered_binding.thread_id, binding.thread_id);
    assert!(recovered_binding.driver_generation > binding.driver_generation);
    assert_eq!(
        recovered_binding.driver_generation,
        replacement_offer.generation
    );
    assert_eq!(
        composition
            .host
            .binding(&binding.binding_id)
            .await
            .expect("load retired Native binding")
            .state,
        RuntimeBindingState::Lost
    );

    let requests = bridge.requests.lock().await;
    assert_eq!(requests.len(), 4);
    let recovered_first_request = &requests[2];
    let first_user_index = recovered_first_request
        .iter()
        .position(|message| {
            matches!(
                message,
                AgentMessage::User { content, .. }
                    if content.iter().any(|part| part.extract_text() == Some("exercise every production tool provider and continue"))
            )
        })
        .unwrap_or_else(|| {
            panic!(
                "cold transcript restores first user prompt: {recovered_first_request:#?}"
            )
        });
    let restored_tool_calls = recovered_first_request
        .iter()
        .enumerate()
        .filter_map(|(index, message)| match message {
            AgentMessage::Assistant { tool_calls, .. } => Some(
                tool_calls
                    .iter()
                    .filter(|call| expected_tool_names.contains(&call.name))
                    .map(|call| (index, call.id.clone(), call.name.clone()))
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        })
        .flatten()
        .collect::<Vec<_>>();
    assert_eq!(
        restored_tool_calls
            .iter()
            .map(|(_, _, name)| name.clone())
            .collect::<BTreeSet<_>>(),
        expected_tool_names,
        "cold transcript restores the complete old assistant tool call"
    );
    let old_tool_call_index = restored_tool_calls
        .iter()
        .map(|(index, ..)| *index)
        .min()
        .expect("cold transcript restores old assistant tool call");
    let restored_tool_call_ids = restored_tool_calls
        .iter()
        .map(|(_, id, _)| id.clone())
        .collect::<BTreeSet<_>>();
    let restored_tool_result_ids = recovered_first_request
        .iter()
        .filter_map(|message| match message {
            AgentMessage::ToolResult { tool_call_id, .. }
                if restored_tool_call_ids.contains(tool_call_id) =>
            {
                Some(tool_call_id.clone())
            }
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        restored_tool_result_ids, restored_tool_call_ids,
        "cold transcript must preserve paired tool-call/tool-result history"
    );
    for tool_call_id in &restored_tool_call_ids {
        assert_eq!(
            restored_tool_calls
                .iter()
                .filter(|(_, id, _)| id == tool_call_id)
                .count(),
            1,
            "cold transcript must replay one assistant tool call per readable id"
        );
        assert_eq!(
            recovered_first_request
                .iter()
                .filter(|message| {
                    matches!(
                        message,
                        AgentMessage::ToolResult { tool_call_id: result_id, .. }
                            if result_id == tool_call_id
                    )
                })
                .count(),
            1,
            "cold transcript must replay one tool result per readable id"
        );
    }
    let old_final_index = recovered_first_request
        .iter()
        .position(|message| {
            matches!(
                message,
                AgentMessage::Assistant { content, tool_calls, .. }
                    if tool_calls.is_empty()
                        && content.iter().any(|part| part.extract_text() == Some("production tools completed"))
            )
        })
        .expect("cold transcript restores old final assistant");
    let old_tool_history_end = recovered_first_request
        .iter()
        .enumerate()
        .filter_map(|(index, message)| match message {
            AgentMessage::Assistant { tool_calls, .. }
                if tool_calls
                    .iter()
                    .any(|call| restored_tool_call_ids.contains(&call.id)) =>
            {
                Some(index)
            }
            AgentMessage::ToolResult { tool_call_id, .. }
                if restored_tool_call_ids.contains(tool_call_id) =>
            {
                Some(index)
            }
            _ => None,
        })
        .max()
        .expect("cold transcript restores complete old tool history");
    let recovery_user_index = recovered_first_request
        .iter()
        .position(|message| {
            matches!(
                message,
                AgentMessage::User { content, .. }
                    if content.iter().any(|part| part.extract_text() == Some("read task after Native cold rebind"))
            )
        })
        .expect("cold transcript includes recovery user prompt");
    assert!(
        first_user_index < old_tool_call_index
            && old_tool_call_index <= old_tool_history_end
            && old_tool_history_end < old_final_index
            && old_final_index < recovery_user_index,
        "cold transcript order must remain user→tool-call/result→assistant→next user: {recovered_first_request:#?}"
    );
    for prompt in [
        "exercise every production tool provider and continue",
        "read task after Native cold rebind",
    ] {
        assert_eq!(
            recovered_first_request
                .iter()
                .filter_map(|message| match message {
                    AgentMessage::User { content, .. } => Some(content),
                    _ => None,
                })
                .flatten()
                .filter(|part| part.extract_text() == Some(prompt))
                .count(),
            1,
            "cold transcript must include each user prompt exactly once"
        );
    }
    drop(requests);

    assert!(recovered_snapshot.transcript.iter().any(|item| {
        let agentdash_agent_protocol::BackboneEvent::ItemCompleted(completed) =
            &item.terminal_event.event
        else {
            return false;
        };
        RuntimeItemContent::new(completed.item.clone()).agent_message_text()
            == Some("production cold rebind completed")
    }));

    let recovered_records = composition
        .runtime_repository
        .journal_records_after(&recovered_binding.thread_id, None)
        .await
        .expect("read Native Runtime journal after cold rebind")
        .records;
    let recovered_lifecycle = recovered_records
        .iter()
        .filter_map(|record| record.as_presentation())
        .filter_map(|event| match &event.event {
            agentdash_agent_protocol::BackboneEvent::ItemStarted(value) => value
                .item
                .tool_call_id()
                .map(|id| (id.to_string(), "started")),
            agentdash_agent_protocol::BackboneEvent::ItemCompleted(value) => value
                .item
                .tool_call_id()
                .map(|id| (id.to_string(), "completed")),
            _ => None,
        })
        .fold(
            BTreeMap::<String, Vec<&str>>::new(),
            |mut map, (id, phase)| {
                map.entry(id).or_default().push(phase);
                map
            },
        );
    assert_eq!(recovered_lifecycle.len(), old_presentation_ids.len() + 1);
    assert!(
        recovered_lifecycle
            .values()
            .all(|phases| phases.as_slice() == ["started", "completed"]),
        "each tool must still own exactly one complete presentation card: {recovered_lifecycle:?}"
    );
    assert!(old_presentation_ids.is_subset(&recovered_lifecycle.keys().cloned().collect()));
    let new_presentation_ids = recovered_lifecycle
        .keys()
        .filter(|id| !old_presentation_ids.contains(*id))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(new_presentation_ids.len(), 1);
    let parse_watermark = |item_id: &str| -> (usize, usize) {
        let (turn, tool) = item_id
            .split_once(':')
            .unwrap_or_else(|| panic!("readable Native item id: {item_id}"));
        let parse = |value: &str, prefix: &str| {
            value
                .strip_prefix(prefix)
                .unwrap_or_else(|| panic!("readable Native {prefix} watermark: {value}"))
                .parse::<usize>()
                .unwrap_or_else(|_| panic!("numeric Native watermark: {value}"))
        };
        (parse(turn, "turn_"), parse(tool, "tool_"))
    };
    let old_watermarks = old_presentation_ids
        .iter()
        .map(|id| parse_watermark(id))
        .collect::<Vec<_>>();
    let new_watermark = parse_watermark(&new_presentation_ids[0]);
    assert!(
        old_watermarks
            .iter()
            .all(|(turn, tool)| new_watermark.0 > *turn && new_watermark.1 > *tool),
        "cold rebind must continue readable Native identity watermarks without card collision: old={old_watermarks:?}, new={new_watermark:?}"
    );

    for operation_id in [&receipt.operation_id, &recovery_receipt.operation_id] {
        let operation = composition
            .runtime_repository
            .find_operation(operation_id)
            .await
            .expect("read Native operation after cold recovery")
            .expect("durable Native operation after cold recovery");
        assert_eq!(
            operation.terminal,
            Some(RuntimeOperationTerminal::Succeeded)
        );
        let (rows, attempts, dispatched): (i64, i64, bool) = sqlx::query_as(
            "SELECT COUNT(*),COALESCE(SUM(attempt_count),0),BOOL_AND(dispatched_at IS NOT NULL) FROM agent_runtime_outbox WHERE operation_id=$1",
        )
        .bind(operation_id.as_str())
        .fetch_one(&pool)
        .await
        .expect("read exact-once Native outbox delivery");
        assert_eq!(rows, 1);
        assert_eq!(attempts, 1);
        assert!(dispatched);
    }
    assert_eq!(
        composition
            .outbox_worker
            .run_once(8)
            .await
            .expect("no duplicate Native outbox dispatch remains"),
        0
    );
}
