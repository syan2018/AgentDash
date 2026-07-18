use std::{
    collections::{BTreeSet, VecDeque},
    sync::Arc,
};

use agentdash_agent_service_api::{
    AgentBindingGeneration, AgentChangesQuery, AgentCommand, AgentCommandEnvelope, AgentCommandId,
    AgentCommandMeta, AgentContextPackageId, AgentContextSchemaVersion,
    AgentContextSourceCoordinate, AgentContextSourceRevision, AgentEffectIdentity,
    AgentEffectInspectionState, AgentForkCutoffKind, AgentForkPoint, AgentIdempotencyKey,
    AgentInput, AgentInputContent, AgentPayloadDigest, AgentReadQuery, AgentReceiptState,
    AgentServiceDefinitionId, AgentServiceErrorCode, AgentSourceCoordinate, AgentSurfaceDigest,
    AgentSurfaceRoute, AgentSurfaceSemanticFacet, AgentToolDelivery, AgentToolName,
    AgentToolSemanticFacet, AgentToolUpdateSemantics, AgentTurnId, AppliedInitialContextEvidence,
    BoundAgentSurface, BoundAgentSurfaceContribution, CompleteAgentService, ContextAuthorityKind,
    ContextProvenance, CreateAgentCommand, ForkAgentCommand, InitialAgentContextPackage,
    InitialContextContribution, InitialContextDeliveryFidelity, InitialContextMode,
    RevokeBoundAgentSurface, SemanticFidelity,
};
use agentdash_integration_codex::{
    CODEX_INITIAL_CONTEXT_RENDERER_VERSION, CodexAppServerObservation,
    CodexAppServerObservationPage, CodexAppServerTransport, CodexCompleteAgentConfig,
    CodexCompleteAgentService, CodexCompleteAgentTransportError,
};
use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::sync::Mutex;

#[derive(Default)]
struct RecordingTransport {
    requests: Mutex<Vec<(String, Value)>>,
    responses: Mutex<VecDeque<(String, Result<Value, CodexCompleteAgentTransportError>)>>,
    observations: Mutex<VecDeque<CodexAppServerObservationPage>>,
    responses_to_server: Mutex<Vec<(Value, Value)>>,
}

impl RecordingTransport {
    async fn push_response(&self, method: &str, response: Value) {
        self.responses
            .lock()
            .await
            .push_back((method.to_owned(), Ok(response)));
    }

    async fn push_error(&self, method: &str, error: CodexCompleteAgentTransportError) {
        self.responses
            .lock()
            .await
            .push_back((method.to_owned(), Err(error)));
    }

    async fn methods(&self) -> Vec<String> {
        self.requests
            .lock()
            .await
            .iter()
            .map(|(method, _)| method.clone())
            .collect()
    }
}

#[async_trait]
impl CodexAppServerTransport for RecordingTransport {
    async fn request(
        &self,
        method: &str,
        params: Value,
    ) -> Result<Value, CodexCompleteAgentTransportError> {
        self.requests.lock().await.push((method.to_owned(), params));
        let (expected, response) = self
            .responses
            .lock()
            .await
            .pop_front()
            .expect("test transport response");
        assert_eq!(expected, method);
        response
    }

    async fn respond(
        &self,
        request_id: Value,
        result: Value,
    ) -> Result<(), CodexCompleteAgentTransportError> {
        self.responses_to_server
            .lock()
            .await
            .push((request_id, result));
        Ok(())
    }

    async fn observations(
        &self,
        _: &str,
        _: Option<u64>,
        _: u32,
    ) -> Result<CodexAppServerObservationPage, CodexCompleteAgentTransportError> {
        Ok(self
            .observations
            .lock()
            .await
            .pop_front()
            .unwrap_or(CodexAppServerObservationPage {
                observations: Vec::new(),
                next_sequence: None,
                gap: false,
            }))
    }
}

fn service(transport: Arc<RecordingTransport>) -> CodexCompleteAgentService {
    CodexCompleteAgentService::new(
        CodexCompleteAgentConfig {
            definition_id: AgentServiceDefinitionId::new("codex").expect("definition"),
            title: "Codex".to_owned(),
            cwd: std::env::current_dir().expect("cwd"),
            model: Some("gpt-5.6-codex".to_owned()),
            model_provider: None,
            base_instructions: Some("base".to_owned()),
            developer_instructions: Some("developer".to_owned()),
            runtime_workspace_roots: vec![std::env::current_dir().expect("root")],
        },
        transport,
    )
    .expect("service")
}

fn meta(id: &str) -> AgentCommandMeta {
    AgentCommandMeta {
        command_id: AgentCommandId::new(format!("command-{id}")).expect("command"),
        effect_id: AgentEffectIdentity::new(format!("effect-{id}")).expect("effect"),
        idempotency_key: AgentIdempotencyKey::new(format!("idem-{id}")).expect("idempotency"),
        binding_generation: AgentBindingGeneration(7),
        expected_snapshot_revision: None,
    }
}

fn initial_package() -> InitialAgentContextPackage {
    let package_id = AgentContextPackageId::new("package-1").expect("package");
    let contribution = InitialContextContribution::CompactSummary {
        summary: "parent summary".to_owned(),
        provenance: ContextProvenance {
            authority: ContextAuthorityKind::AgentHistory,
            source: AgentContextSourceCoordinate::new("parent").expect("source"),
            revision: AgentContextSourceRevision::new("revision-5").expect("revision"),
            digest: AgentPayloadDigest::new("sha256:parent").expect("digest"),
        },
    };
    InitialAgentContextPackage {
        package_id: package_id.clone(),
        schema_version: AgentContextSchemaVersion(1),
        mode: InitialContextMode::Compact,
        digest: InitialAgentContextPackage::calculated_digest(
            &package_id,
            AgentContextSchemaVersion(1),
            InitialContextMode::Compact,
            std::slice::from_ref(&contribution),
        ),
        contributions: vec![contribution],
    }
}

async fn create_source(
    service: &CodexCompleteAgentService,
    transport: &RecordingTransport,
) -> AgentSourceCoordinate {
    transport
        .push_response("thread/start", json!({"thread": {"id": "thread-parent"}}))
        .await;
    service
        .create(CreateAgentCommand {
            meta: meta("create"),
            requested_source: None,
            initial_context: None,
        })
        .await
        .expect("create")
        .source
}

#[tokio::test]
async fn descriptor_is_truthful_about_codex_context_change_and_surface_boundaries() {
    let service = service(Arc::new(RecordingTransport::default()));
    let descriptor = service.describe().await.expect("descriptor");

    assert_eq!(
        descriptor.profile.source_changes,
        agentdash_agent_service_api::AgentSourceChangeLevel::OrderedLiveStream
    );
    assert_eq!(
        descriptor
            .profile
            .fork
            .cutoffs
            .get(&AgentForkCutoffKind::CompletedTurn),
        Some(&SemanticFidelity::Exact)
    );
    assert_eq!(
        descriptor
            .profile
            .fork
            .cutoffs
            .get(&AgentForkCutoffKind::Item),
        Some(&SemanticFidelity::Unsupported)
    );
    assert!(descriptor.profile.surface.facets.iter().all(|facet| {
        !matches!(
            facet.semantics,
            AgentSurfaceSemanticFacet::Tool(_) | AgentSurfaceSemanticFacet::Hook(_)
        )
    }));
    assert_eq!(
        descriptor.profile.initial_context.renderer_versions,
        BTreeSet::from([CODEX_INITIAL_CONTEXT_RENDERER_VERSION.to_owned()])
    );
}

#[tokio::test]
async fn create_installs_initial_package_as_rendered_configuration_before_first_input() {
    let transport = Arc::new(RecordingTransport::default());
    transport
        .push_response("thread/start", json!({"thread": {"id": "thread-1"}}))
        .await;
    let service = service(transport.clone());
    let package = initial_package();

    let receipt = service
        .create(CreateAgentCommand {
            meta: meta("create"),
            requested_source: None,
            initial_context: Some(package.clone()),
        })
        .await
        .expect("create");
    let evidence = receipt.initial_context.expect("context evidence");
    assert_eq!(evidence.package_digest, package.digest);
    assert_eq!(
        evidence.fidelity,
        InitialContextDeliveryFidelity::CanonicalRendered
    );
    assert!(evidence.materialized_digest.is_some());
    assert_eq!(
        evidence.renderer_version.as_deref(),
        Some(CODEX_INITIAL_CONTEXT_RENDERER_VERSION)
    );

    transport
        .push_response("turn/start", json!({"turn": {"id": "turn-1"}}))
        .await;
    service
        .execute(AgentCommandEnvelope {
            meta: meta("input"),
            source: receipt.source,
            command: AgentCommand::SubmitInput {
                input: AgentInput {
                    content: vec![AgentInputContent::Text {
                        text: "actual task".to_owned(),
                    }],
                },
            },
        })
        .await
        .expect("submit");

    let requests = transport.requests.lock().await;
    assert_eq!(requests[0].0, "thread/start");
    assert!(
        requests[0].1["developerInstructions"]
            .as_str()
            .expect("developer instructions")
            .contains("package_id=package-1")
    );
    assert_eq!(requests[1].0, "turn/start");
    assert_eq!(requests[1].1["input"][0]["text"], "actual task");
}

#[tokio::test]
async fn completed_turn_fork_uses_last_turn_id_and_verifies_the_child_with_thread_read() {
    let transport = Arc::new(RecordingTransport::default());
    let service = service(transport.clone());
    let source = create_source(&service, &transport).await;
    transport
        .push_response("thread/fork", json!({"thread": {"id": "thread-child"}}))
        .await;
    transport
        .push_response(
            "thread/read",
            json!({"thread": {"id": "thread-child", "turns": []}}),
        )
        .await;

    let receipt = service
        .fork(ForkAgentCommand {
            meta: meta("fork"),
            source,
            requested_child_source: None,
            cutoff: AgentForkPoint::CompletedTurn {
                turn_id: AgentTurnId::new("turn-7").expect("turn"),
            },
        })
        .await
        .expect("fork");

    assert_eq!(
        receipt
            .child_source
            .as_ref()
            .map(AgentSourceCoordinate::as_str),
        Some("thread-child")
    );
    assert!(receipt.child_history_digest.is_none());
    let requests = transport.requests.lock().await;
    assert_eq!(requests[1].0, "thread/fork");
    assert_eq!(requests[1].1["lastTurnId"], "turn-7");
    assert_eq!(requests[2].0, "thread/read");
}

#[tokio::test]
async fn fork_child_verification_mismatch_becomes_unknown_and_is_not_retried() {
    let transport = Arc::new(RecordingTransport::default());
    let service = service(transport.clone());
    let source = create_source(&service, &transport).await;
    transport
        .push_response("thread/fork", json!({"thread": {"id": "thread-child"}}))
        .await;
    transport
        .push_response(
            "thread/read",
            json!({"thread": {"id": "different-child", "turns": []}}),
        )
        .await;
    let command = ForkAgentCommand {
        meta: meta("fork-unknown"),
        source,
        requested_child_source: None,
        cutoff: AgentForkPoint::Head,
    };
    let effect_id = command.meta.effect_id.clone();

    let error = service
        .fork(command.clone())
        .await
        .expect_err("child mismatch");
    assert_eq!(error.code, AgentServiceErrorCode::ProtocolViolation);
    assert!(matches!(
        service.inspect(effect_id).await.expect("inspect").state,
        AgentEffectInspectionState::Unknown
    ));
    service
        .fork(command)
        .await
        .expect_err("unknown fork effect must not be retried");
    assert_eq!(
        transport.methods().await,
        vec!["thread/start", "thread/fork", "thread/read"]
    );
}

#[tokio::test]
async fn snapshot_is_observed_and_live_gap_requires_thread_read_reconciliation() {
    let transport = Arc::new(RecordingTransport::default());
    let service = service(transport.clone());
    let source = create_source(&service, &transport).await;
    transport
        .push_response(
            "thread/read",
            json!({
                "thread": {
                    "id": "thread-parent",
                    "turns": [{
                        "id": "turn-1",
                        "status": "completed",
                        "items": [{
                            "id": "item-1",
                            "type": "agentMessage",
                            "text": "done",
                            "status": "completed"
                        }]
                    }]
                }
            }),
        )
        .await;

    let snapshot = service
        .read(AgentReadQuery {
            source: source.clone(),
            at_revision: None,
        })
        .await
        .expect("read");
    assert_eq!(
        snapshot.source_info.authority,
        agentdash_agent_service_api::AgentSnapshotAuthority::AgentObserved
    );
    assert_eq!(snapshot.source_info.source_revision, None);
    assert_eq!(snapshot.turns.len(), 1);

    transport
        .observations
        .lock()
        .await
        .push_back(CodexAppServerObservationPage {
            observations: Vec::new(),
            next_sequence: Some(9),
            gap: true,
        });
    let page = service
        .changes(AgentChangesQuery {
            source,
            after: None,
            limit: 16,
        })
        .await
        .expect("changes");
    assert!(page.gap);
    assert!(page.changes.is_empty());
}

#[tokio::test]
async fn live_server_request_maps_to_interaction_and_resolves_through_the_same_rpc_request() {
    let transport = Arc::new(RecordingTransport::default());
    let service = service(transport.clone());
    let source = create_source(&service, &transport).await;
    transport
        .observations
        .lock()
        .await
        .push_back(CodexAppServerObservationPage {
            observations: vec![CodexAppServerObservation::ServerRequest {
                sequence: 3,
                request_id: json!(44),
                method: "item/commandExecution/requestApproval".to_owned(),
                params: json!({
                    "requestId": "approval-1",
                    "turnId": "turn-1",
                    "itemId": "item-1",
                    "prompt": "approve?"
                }),
            }],
            next_sequence: Some(3),
            gap: false,
        });
    let changes = service
        .changes(AgentChangesQuery {
            source: source.clone(),
            after: None,
            limit: 16,
        })
        .await
        .expect("changes");
    assert!(matches!(
        changes.changes[0].payload,
        agentdash_agent_service_api::AgentChangePayload::InteractionChanged { .. }
    ));
    transport
        .push_response(
            "thread/read",
            json!({"thread": {"id": "thread-parent", "turns": []}}),
        )
        .await;
    let pending = service
        .read(AgentReadQuery {
            source: source.clone(),
            at_revision: None,
        })
        .await
        .expect("interaction snapshot");
    assert_eq!(pending.interactions[0].turn_id.as_str(), "turn-1");
    assert_eq!(
        pending.interactions[0]
            .item_id
            .as_ref()
            .map(agentdash_agent_service_api::AgentItemId::as_str),
        Some("item-1")
    );

    service
        .execute(AgentCommandEnvelope {
            meta: meta("approval"),
            source,
            command: AgentCommand::ResolveInteraction {
                interaction_id: agentdash_agent_service_api::AgentInteractionId::new("approval-1")
                    .expect("interaction"),
                response: agentdash_agent_service_api::AgentInteractionResponse::Approved,
            },
        })
        .await
        .expect("resolve");
    let responses = transport.responses_to_server.lock().await;
    assert_eq!(responses[0].0, json!(44));
    assert_eq!(responses[0].1["decision"], "accept");
}

#[tokio::test]
async fn historical_snapshot_revision_is_rejected_before_app_server_read() {
    let transport = Arc::new(RecordingTransport::default());
    let service = service(transport.clone());
    let error = service
        .read(AgentReadQuery {
            source: AgentSourceCoordinate::new("thread-1").expect("source"),
            at_revision: Some(agentdash_agent_service_api::AgentSnapshotRevision(3)),
        })
        .await
        .expect_err("Codex has no stable historical snapshot API");

    assert_eq!(error.code, AgentServiceErrorCode::Unsupported);
    assert!(transport.methods().await.is_empty());
}

#[tokio::test]
async fn resume_commands_and_immutable_surface_cover_the_remaining_complete_agent_surface() {
    let transport = Arc::new(RecordingTransport::default());
    let service = service(transport.clone());
    let source = create_source(&service, &transport).await;

    transport
        .push_response("thread/resume", json!({"thread": {"id": "thread-parent"}}))
        .await;
    service
        .resume(agentdash_agent_service_api::ResumeAgentCommand {
            meta: meta("resume"),
            source: source.clone(),
        })
        .await
        .expect("resume");

    for (id, method, command) in [
        (
            "steer",
            "turn/steer",
            AgentCommand::Steer {
                expected_turn_id: AgentTurnId::new("turn-active").expect("turn"),
                input: AgentInput {
                    content: vec![AgentInputContent::Text {
                        text: "steer".to_owned(),
                    }],
                },
            },
        ),
        (
            "interrupt",
            "turn/interrupt",
            AgentCommand::Interrupt {
                expected_turn_id: AgentTurnId::new("turn-active").expect("turn"),
            },
        ),
        (
            "compact",
            "thread/compact/start",
            AgentCommand::RequestCompaction,
        ),
    ] {
        transport.push_response(method, json!({})).await;
        let receipt = service
            .execute(AgentCommandEnvelope {
                meta: meta(id),
                source: source.clone(),
                command,
            })
            .await
            .expect(method);
        assert!(matches!(receipt.state, AgentReceiptState::Accepted));
    }

    let descriptor = service.describe().await.expect("descriptor");
    let payload_digest = AgentPayloadDigest::new("sha256:instruction").expect("payload");
    transport
        .push_response("thread/resume", json!({"thread": {"id": "thread-parent"}}))
        .await;
    let applied = service
        .apply_surface(agentdash_agent_service_api::ApplyBoundAgentSurface {
            command_id: AgentCommandId::new("surface-command").expect("command"),
            effect_id: AgentEffectIdentity::new("surface-effect").expect("effect"),
            idempotency_key: AgentIdempotencyKey::new("surface-idem").expect("idempotency"),
            source: source.clone(),
            bound_surface: BoundAgentSurface {
                revision: agentdash_agent_service_api::AgentSurfaceRevision(2),
                digest: AgentSurfaceDigest::new("surface-2").expect("surface"),
                offer_profile_digest: descriptor.profile_digest,
                contributions: vec![BoundAgentSurfaceContribution {
                    key: "instruction".to_owned(),
                    required: true,
                    route: AgentSurfaceRoute::ImmutableDelivery,
                    fidelity: SemanticFidelity::Exact,
                    semantics: AgentSurfaceSemanticFacet::Instruction,
                    payload:
                        agentdash_agent_service_api::AgentSurfaceContributionPayload::Instruction {
                            channel: "developer".to_owned(),
                            text: "bound instruction".to_owned(),
                        },
                    payload_digest,
                }],
            },
            callbacks: agentdash_agent_service_api::AgentHostCallbackBinding {
                route_id: agentdash_agent_service_api::AgentCallbackRouteId::new("route")
                    .expect("route"),
                binding_generation: AgentBindingGeneration(7),
                delivery: AgentSurfaceRoute::ImmutableDelivery,
                default_deadline_ms: 1_000,
            },
        })
        .await
        .expect("apply immutable surface");
    assert!(applied.applied.contributions.iter().all(|contribution| {
        contribution.status == agentdash_agent_service_api::AppliedContributionStatus::Applied
    }));

    transport
        .push_response("thread/resume", json!({"thread": {"id": "thread-parent"}}))
        .await;
    service
        .revoke_surface(RevokeBoundAgentSurface {
            command_id: AgentCommandId::new("revoke-command").expect("command"),
            effect_id: AgentEffectIdentity::new("revoke-effect").expect("effect"),
            idempotency_key: AgentIdempotencyKey::new("revoke-idem").expect("idempotency"),
            binding_generation: AgentBindingGeneration(7),
            source: source.clone(),
            expected_revision: agentdash_agent_service_api::AgentSurfaceRevision(2),
        })
        .await
        .expect("revoke surface");

    transport.push_response("thread/archive", json!({})).await;
    let closed = service
        .execute(AgentCommandEnvelope {
            meta: meta("close"),
            source,
            command: AgentCommand::Close,
        })
        .await
        .expect("close");
    assert!(matches!(
        closed.state,
        AgentReceiptState::Terminal {
            outcome: agentdash_agent_service_api::AgentTerminalOutcome::Closed
        }
    ));
}

#[tokio::test]
async fn apply_surface_rejects_unadvertised_dynamic_tools_before_side_effect() {
    let transport = Arc::new(RecordingTransport::default());
    let service = service(transport.clone());
    let source = create_source(&service, &transport).await;
    let descriptor = service.describe().await.expect("descriptor");
    let payload_digest = AgentPayloadDigest::new("sha256:tool").expect("digest");
    let command = agentdash_agent_service_api::ApplyBoundAgentSurface {
        command_id: AgentCommandId::new("surface-command").expect("command"),
        effect_id: AgentEffectIdentity::new("surface-effect").expect("effect"),
        idempotency_key: AgentIdempotencyKey::new("surface-idem").expect("idempotency"),
        source,
        bound_surface: BoundAgentSurface {
            revision: agentdash_agent_service_api::AgentSurfaceRevision(1),
            digest: AgentSurfaceDigest::new("surface").expect("surface"),
            offer_profile_digest: descriptor.profile_digest,
            contributions: vec![BoundAgentSurfaceContribution {
                key: "tool".to_owned(),
                required: true,
                route: AgentSurfaceRoute::AgentNativeCallback,
                fidelity: SemanticFidelity::Exact,
                semantics: AgentSurfaceSemanticFacet::Tool(AgentToolSemanticFacet {
                    delivery: AgentToolDelivery::AgentNativeCallback,
                    invocation: SemanticFidelity::Exact,
                    update: AgentToolUpdateSemantics::BindingOnly,
                }),
                payload: agentdash_agent_service_api::AgentSurfaceContributionPayload::Tool {
                    name: AgentToolName::new("search").expect("tool"),
                    description: "search".to_owned(),
                    input_schema: json!({"type": "object"}),
                    output_schema: None,
                },
                payload_digest,
            }],
        },
        callbacks: agentdash_agent_service_api::AgentHostCallbackBinding {
            route_id: agentdash_agent_service_api::AgentCallbackRouteId::new("route")
                .expect("route"),
            binding_generation: AgentBindingGeneration(7),
            delivery: AgentSurfaceRoute::AgentNativeCallback,
            default_deadline_ms: 1_000,
        },
    };

    let error = service
        .apply_surface(command)
        .await
        .expect_err("unadvertised tool must fail");
    assert_eq!(error.code, AgentServiceErrorCode::Unsupported);
    assert_eq!(transport.methods().await, vec!["thread/start"]);
}

#[tokio::test]
async fn unknown_create_outcome_is_inspectable_and_never_retried_with_same_effect() {
    let transport = Arc::new(RecordingTransport::default());
    transport
        .push_error(
            "thread/start",
            CodexCompleteAgentTransportError::unavailable("response lost", true),
        )
        .await;
    let service = service(transport.clone());
    let command = CreateAgentCommand {
        meta: meta("unknown"),
        requested_source: None,
        initial_context: None,
    };
    let effect_id = command.meta.effect_id.clone();

    let error = service.create(command.clone()).await.expect_err("unknown");
    assert!(error.retryable);
    assert!(matches!(
        service.inspect(effect_id).await.expect("inspect").state,
        AgentEffectInspectionState::Unknown
    ));
    let duplicate = service
        .create(command)
        .await
        .expect_err("must not retry unknown effect");
    assert_eq!(duplicate.code, AgentServiceErrorCode::Unavailable);
    assert_eq!(transport.methods().await, vec!["thread/start"]);
}

#[allow(dead_code)]
fn _compile_surface_revoke(_: RevokeBoundAgentSurface) {}

#[allow(dead_code)]
fn _compile_initial_evidence(_: AppliedInitialContextEvidence) {}
