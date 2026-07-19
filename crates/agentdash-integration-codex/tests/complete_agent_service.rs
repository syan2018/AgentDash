use std::{
    collections::{BTreeSet, VecDeque},
    sync::Arc,
};

use agentdash_agent_service_api::{
    AgentAppliedEffectOutcome, AgentBindingGeneration, AgentChangesQuery, AgentCommand,
    AgentCommandEnvelope, AgentCommandId, AgentCommandMeta, AgentContextPackageId,
    AgentContextSchemaVersion, AgentContextSourceCoordinate, AgentContextSourceRevision,
    AgentEffectIdentity, AgentEffectInspectionState, AgentForkCutoffKind, AgentForkPoint,
    AgentIdempotencyKey, AgentInput, AgentInputContent, AgentPayloadDigest, AgentReadQuery,
    AgentReceiptState, AgentServiceDefinitionId, AgentServiceErrorCode, AgentServiceInstanceId,
    AgentSourceCoordinate, AgentSurfaceDigest, AgentSurfaceRoute, AgentSurfaceSemanticFacet,
    AgentToolDelivery, AgentToolName, AgentToolSemanticFacet, AgentToolUpdateSemantics,
    AgentTurnId, AppliedInitialContextEvidence, BoundAgentSurface, BoundAgentSurfaceContribution,
    CompleteAgentService, ContextAuthorityKind, ContextProvenance, CreateAgentCommand,
    ForkAgentCommand, InitialAgentContextPackage, InitialContextContribution,
    InitialContextDeliveryFidelity, InitialContextMode, RevokeBoundAgentSurface, SemanticFidelity,
};
use agentdash_integration_codex::{
    CODEX_INITIAL_CONTEXT_RENDERER_VERSION, CodexAppServerObservation,
    CodexAppServerObservationPage, CodexAppServerTransport, CodexCompleteAgentConfig,
    CodexCompleteAgentRegistration, CodexCompleteAgentTransportError,
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
    server_response_results: Mutex<VecDeque<Result<(), CodexCompleteAgentTransportError>>>,
    notifications: Mutex<Vec<(String, Option<Value>)>>,
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

    async fn push_respond_error(&self, error: CodexCompleteAgentTransportError) {
        self.server_response_results
            .lock()
            .await
            .push_back(Err(error));
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
        self.server_response_results
            .lock()
            .await
            .pop_front()
            .unwrap_or(Ok(()))
    }

    async fn notify(
        &self,
        method: &str,
        params: Option<Value>,
    ) -> Result<(), CodexCompleteAgentTransportError> {
        self.notifications
            .lock()
            .await
            .push((method.to_owned(), params));
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

async fn service(transport: Arc<RecordingTransport>) -> Arc<dyn CompleteAgentService> {
    transport.responses.lock().await.push_front((
        "initialize".to_owned(),
        Ok(json!({
            "userAgent": "codex-test",
            "codexHome": std::env::current_dir().expect("cwd"),
            "platformFamily": "windows",
            "platformOs": "windows"
        })),
    ));
    let registration = CodexCompleteAgentRegistration::new(
        AgentServiceInstanceId::new("codex-test-instance").expect("instance"),
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
        transport.clone(),
    )
    .await
    .expect("registration");
    transport.requests.lock().await.clear();
    registration.service()
}

#[tokio::test]
async fn registration_exposes_one_complete_agent_instance_without_driver_contribution() {
    let transport = Arc::new(RecordingTransport::default());
    transport
        .push_response(
            "initialize",
            json!({
                "userAgent": "codex-test",
                "codexHome": std::env::current_dir().expect("cwd"),
                "platformFamily": "windows",
                "platformOs": "windows"
            }),
        )
        .await;
    let registration = CodexCompleteAgentRegistration::new(
        AgentServiceInstanceId::new("codex-instance").expect("instance"),
        CodexCompleteAgentConfig {
            definition_id: AgentServiceDefinitionId::new("codex").expect("definition"),
            title: "Codex".to_owned(),
            cwd: std::env::current_dir().expect("cwd"),
            model: None,
            model_provider: None,
            base_instructions: None,
            developer_instructions: None,
            runtime_workspace_roots: vec![std::env::current_dir().expect("root")],
        },
        transport.clone(),
    )
    .await
    .expect("registration");

    assert_eq!(transport.methods().await, vec!["initialize"]);
    assert_eq!(
        transport.notifications.lock().await.as_slice(),
        &[("initialized".to_owned(), None)]
    );
    assert_eq!(registration.instance_id().as_str(), "codex-instance");
    let (instance_id, service) = registration.into_parts();
    assert_eq!(instance_id.as_str(), "codex-instance");
    assert_eq!(
        service
            .describe()
            .await
            .expect("descriptor")
            .definition_id
            .as_str(),
        "codex"
    );
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

async fn immutable_surface_command(
    service: &dyn CompleteAgentService,
    source: AgentSourceCoordinate,
    id: &str,
) -> agentdash_agent_service_api::ApplyBoundAgentSurface {
    let descriptor = service.describe().await.expect("descriptor");
    agentdash_agent_service_api::ApplyBoundAgentSurface {
        command_id: AgentCommandId::new(format!("surface-command-{id}")).expect("command"),
        effect_id: AgentEffectIdentity::new(format!("surface-effect-{id}")).expect("effect"),
        idempotency_key: AgentIdempotencyKey::new(format!("surface-idem-{id}"))
            .expect("idempotency"),
        source,
        bound_surface: BoundAgentSurface {
            revision: agentdash_agent_service_api::AgentSurfaceRevision(2),
            digest: AgentSurfaceDigest::new(format!("surface-{id}")).expect("surface"),
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
                payload_digest: AgentPayloadDigest::new(format!("sha256:instruction-{id}"))
                    .expect("payload"),
            }],
        },
        callbacks: agentdash_agent_service_api::AgentHostCallbackBinding {
            route_id: agentdash_agent_service_api::AgentCallbackRouteId::new(format!("route-{id}"))
                .expect("route"),
            binding_generation: AgentBindingGeneration(7),
            delivery: AgentSurfaceRoute::ImmutableDelivery,
            default_deadline_ms: 1_000,
        },
    }
}

async fn create_source(
    service: &dyn CompleteAgentService,
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
    let service = service(Arc::new(RecordingTransport::default())).await;
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
    let service = service(transport.clone()).await;
    let package = initial_package();

    let receipt = service
        .create(CreateAgentCommand {
            meta: meta("create"),
            requested_source: None,
            initial_context: Some(package.clone()),
        })
        .await
        .expect("create");
    let create_inspection = service
        .inspect(receipt.effect_id.clone())
        .await
        .expect("inspect create");
    assert!(create_inspection.validate());
    assert!(matches!(
        create_inspection.state,
        AgentEffectInspectionState::Applied {
            outcome: AgentAppliedEffectOutcome::Create { .. }
        }
    ));
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
    let input_meta = meta("input");
    let input_effect = input_meta.effect_id.clone();
    service
        .execute(AgentCommandEnvelope {
            meta: input_meta,
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
    let command_inspection = service
        .inspect(input_effect)
        .await
        .expect("inspect command");
    assert!(command_inspection.validate());
    assert!(matches!(
        command_inspection.state,
        AgentEffectInspectionState::Applied {
            outcome: AgentAppliedEffectOutcome::Command { .. }
        }
    ));

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
    let service = service(transport.clone()).await;
    let source = create_source(service.as_ref(), &transport).await;
    transport
        .push_response("thread/fork", json!({"thread": {"id": "thread-child"}}))
        .await;
    transport
        .push_response(
            "thread/read",
            json!({
                "thread": {
                    "id": "thread-child",
                    "turns": [{"id": "turn-7", "status": "completed"}]
                }
            }),
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
    assert!(receipt.child_history_digest.is_some());
    let inspection = service
        .inspect(receipt.effect_id.clone())
        .await
        .expect("inspect fork");
    assert!(inspection.validate());
    match inspection.state {
        AgentEffectInspectionState::Applied {
            outcome: AgentAppliedEffectOutcome::Fork { receipt: applied },
        } => {
            assert_eq!(applied.child_source.as_str(), "thread-child");
            assert_eq!(applied.cutoff, receipt.cutoff);
            assert_eq!(
                Some(&applied.child_history_digest),
                receipt.child_history_digest.as_ref()
            );
        }
        other => panic!("unexpected fork inspection: {other:?}"),
    }
    let requests = transport.requests.lock().await;
    assert_eq!(requests[1].0, "thread/fork");
    assert_eq!(requests[1].1["lastTurnId"], "turn-7");
    assert_eq!(requests[2].0, "thread/read");
}

#[tokio::test]
async fn fork_child_verification_mismatch_becomes_unknown_and_is_not_retried() {
    let transport = Arc::new(RecordingTransport::default());
    let service = service(transport.clone()).await;
    let source = create_source(service.as_ref(), &transport).await;
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
async fn fork_cutoff_verification_mismatch_becomes_unknown_and_is_not_retried() {
    let transport = Arc::new(RecordingTransport::default());
    let service = service(transport.clone()).await;
    let source = create_source(service.as_ref(), &transport).await;
    transport
        .push_response("thread/fork", json!({"thread": {"id": "thread-child"}}))
        .await;
    transport
        .push_response(
            "thread/read",
            json!({
                "thread": {
                    "id": "thread-child",
                    "turns": [{"id": "turn-6", "status": "completed"}]
                }
            }),
        )
        .await;
    let command = ForkAgentCommand {
        meta: meta("fork-cutoff-unknown"),
        source,
        requested_child_source: None,
        cutoff: AgentForkPoint::CompletedTurn {
            turn_id: AgentTurnId::new("turn-7").expect("turn"),
        },
    };
    let effect_id = command.meta.effect_id.clone();

    let error = service
        .fork(command.clone())
        .await
        .expect_err("child cutoff mismatch");
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
async fn fork_cutoff_with_matching_id_but_non_completed_status_stays_unknown() {
    let transport = Arc::new(RecordingTransport::default());
    let service = service(transport.clone()).await;
    let source = create_source(service.as_ref(), &transport).await;
    transport
        .push_response("thread/fork", json!({"thread": {"id": "thread-child"}}))
        .await;
    transport
        .push_response(
            "thread/read",
            json!({
                "thread": {
                    "id": "thread-child",
                    "turns": [{"id": "turn-7", "status": "inProgress"}]
                }
            }),
        )
        .await;
    let command = ForkAgentCommand {
        meta: meta("fork-running-cutoff"),
        source,
        requested_child_source: None,
        cutoff: AgentForkPoint::CompletedTurn {
            turn_id: AgentTurnId::new("turn-7").expect("turn"),
        },
    };
    let effect_id = command.meta.effect_id.clone();

    let error = service
        .fork(command.clone())
        .await
        .expect_err("running cutoff cannot prove a completed-turn fork");
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
    let service = service(transport.clone()).await;
    let source = create_source(service.as_ref(), &transport).await;
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
    assert_eq!(snapshot.conversation_history.len(), 2);
    assert!(snapshot.conversation_history.iter().all(|record| matches!(
        record.presentation.envelope.event,
        agentdash_agent_protocol::BackboneEvent::TurnStarted(_)
            | agentdash_agent_protocol::BackboneEvent::TurnCompleted(_)
    )));

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
    let service = service(transport.clone()).await;
    let source = create_source(service.as_ref(), &transport).await;
    transport
        .observations
        .lock()
        .await
        .push_back(CodexAppServerObservationPage {
            observations: vec![
                CodexAppServerObservation::server_request(
                    3,
                    json!(44),
                    "item/commandExecution/requestApproval",
                    json!({
                        "approvalId": "approval-1",
                        "threadId": "thread-parent",
                        "turnId": "turn-1",
                        "itemId": "item-1",
                        "startedAtMs": 1,
                        "command": "echo hi",
                        "reason": "approve?"
                    }),
                )
                .expect("typed command approval"),
            ],
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
async fn thread_read_and_name_notifications_map_source_authoritative_set_and_clear() {
    let transport = Arc::new(RecordingTransport::default());
    let service = service(transport.clone()).await;
    let source = create_source(service.as_ref(), &transport).await;
    transport
        .push_response(
            "thread/read",
            json!({
                "thread": {
                    "id": "thread-parent",
                    "name": "Codex 标题",
                    "turns": []
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
        .expect("thread/read");
    let name = snapshot.thread_name.expect("Codex name section");
    assert_eq!(name.thread_name.as_deref(), Some("Codex 标题"));
    assert_eq!(
        name.source_info.authority,
        agentdash_agent_service_api::AgentSnapshotAuthority::AgentAuthoritative
    );
    assert_eq!(name.source_info.fidelity, SemanticFidelity::Exact);

    transport
        .observations
        .lock()
        .await
        .push_back(CodexAppServerObservationPage {
            observations: vec![
                CodexAppServerObservation::notification(
                    4,
                    "thread/name/updated",
                    json!({
                        "threadId": "thread-parent",
                        "threadName": "更新标题"
                    }),
                )
                .expect("typed name notification"),
                CodexAppServerObservation::notification(
                    5,
                    "thread/name/updated",
                    json!({
                        "threadId": "thread-parent",
                        "threadName": null
                    }),
                )
                .expect("typed cleared name notification"),
            ],
            next_sequence: Some(5),
            gap: false,
        });
    let changes = service
        .changes(AgentChangesQuery {
            source,
            after: None,
            limit: 16,
        })
        .await
        .expect("name changes");
    assert!(matches!(
        &changes.changes[0].payload,
        agentdash_agent_service_api::AgentChangePayload::SourceObservation {
            state,
            presentation,
        } if matches!(
            state.as_ref(),
            agentdash_agent_service_api::AgentChangePayload::ThreadNameChanged {
                thread_name: Some(value),
                ..
            } if value == "更新标题"
        ) && presentation.len() == 1
    ));
    assert!(matches!(
        &changes.changes[1].payload,
        agentdash_agent_service_api::AgentChangePayload::SourceObservation {
            state,
            presentation,
        } if matches!(
            state.as_ref(),
            agentdash_agent_service_api::AgentChangePayload::ThreadNameChanged {
                thread_name: None,
                ..
            }
        ) && presentation.len() == 1
    ));
}

#[tokio::test]
async fn thread_name_notification_for_another_source_is_rejected() {
    let transport = Arc::new(RecordingTransport::default());
    let service = service(transport.clone()).await;
    let source = create_source(service.as_ref(), &transport).await;
    transport
        .observations
        .lock()
        .await
        .push_back(CodexAppServerObservationPage {
            observations: vec![
                CodexAppServerObservation::notification(
                    4,
                    "thread/name/updated",
                    json!({
                        "threadId": "another-thread",
                        "threadName": "错误来源"
                    }),
                )
                .expect("typed wrong-source name notification"),
            ],
            next_sequence: Some(4),
            gap: false,
        });
    let error = service
        .changes(AgentChangesQuery {
            source,
            after: None,
            limit: 16,
        })
        .await
        .expect_err("wrong source name");
    assert_eq!(error.code, AgentServiceErrorCode::ProtocolViolation);
}

#[tokio::test]
async fn historical_snapshot_revision_is_rejected_before_app_server_read() {
    let transport = Arc::new(RecordingTransport::default());
    let service = service(transport.clone()).await;
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
    let service = service(transport.clone()).await;
    let source = create_source(service.as_ref(), &transport).await;

    transport
        .push_response("thread/resume", json!({"thread": {"id": "thread-parent"}}))
        .await;
    let resume_meta = meta("resume");
    let resume_effect = resume_meta.effect_id.clone();
    service
        .resume(agentdash_agent_service_api::ResumeAgentCommand {
            meta: resume_meta,
            source: source.clone(),
        })
        .await
        .expect("resume");
    let resume_inspection = service
        .inspect(resume_effect)
        .await
        .expect("inspect resume");
    assert!(resume_inspection.validate());
    assert!(matches!(
        resume_inspection.state,
        AgentEffectInspectionState::Applied {
            outcome: AgentAppliedEffectOutcome::Resume { .. }
        }
    ));

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
    let apply_inspection = service
        .inspect(applied.effect_id.clone())
        .await
        .expect("inspect surface apply");
    assert!(apply_inspection.validate());
    assert!(matches!(
        apply_inspection.state,
        AgentEffectInspectionState::Applied {
            outcome: AgentAppliedEffectOutcome::SurfaceApply { .. }
        }
    ));

    transport
        .push_response("thread/resume", json!({"thread": {"id": "thread-parent"}}))
        .await;
    let revoke_effect = AgentEffectIdentity::new("revoke-effect").expect("effect");
    service
        .revoke_surface(RevokeBoundAgentSurface {
            command_id: AgentCommandId::new("revoke-command").expect("command"),
            effect_id: revoke_effect.clone(),
            idempotency_key: AgentIdempotencyKey::new("revoke-idem").expect("idempotency"),
            binding_generation: AgentBindingGeneration(7),
            source: source.clone(),
            expected_revision: agentdash_agent_service_api::AgentSurfaceRevision(2),
        })
        .await
        .expect("revoke surface");
    let revoke_inspection = service
        .inspect(revoke_effect)
        .await
        .expect("inspect surface revoke");
    assert!(revoke_inspection.validate());
    assert!(matches!(
        revoke_inspection.state,
        AgentEffectInspectionState::Applied {
            outcome: AgentAppliedEffectOutcome::SurfaceRevoke { .. }
        }
    ));

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
    let service = service(transport.clone()).await;
    let source = create_source(service.as_ref(), &transport).await;
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
    let service = service(transport.clone()).await;
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

#[tokio::test]
async fn missing_local_effect_after_adapter_restart_is_unknown_not_not_applied() {
    let transport = Arc::new(RecordingTransport::default());
    let restarted_service = service(transport.clone()).await;
    let inspection = restarted_service
        .inspect(AgentEffectIdentity::new("pre-restart-effect").expect("effect"))
        .await
        .expect("inspect after restart");

    assert!(matches!(
        inspection.state,
        AgentEffectInspectionState::Unknown
    ));
    assert!(
        transport.methods().await.is_empty(),
        "inspection must not redispatch a vendor command"
    );
}

#[tokio::test]
async fn reused_effect_rejects_a_different_command_or_source_without_dispatch() {
    let transport = Arc::new(RecordingTransport::default());
    let service = service(transport.clone()).await;
    let source = create_source(service.as_ref(), &transport).await;
    transport.push_response("turn/start", json!({})).await;
    let first = AgentCommandEnvelope {
        meta: meta("effect-coordinate"),
        source: source.clone(),
        command: AgentCommand::SubmitInput {
            input: AgentInput {
                content: vec![AgentInputContent::Text {
                    text: "first".to_owned(),
                }],
            },
        },
    };
    service.execute(first.clone()).await.expect("first command");

    let mut conflicting = first;
    conflicting.meta.command_id =
        AgentCommandId::new("different-command").expect("command identity");
    conflicting.source = AgentSourceCoordinate::new("different-source").expect("source");
    let error = service
        .execute(conflicting)
        .await
        .expect_err("effect identity cannot move to another command/source");

    assert_eq!(error.code, AgentServiceErrorCode::Conflict);
    assert_eq!(
        transport.methods().await,
        vec!["thread/start", "turn/start"]
    );
}

#[tokio::test]
async fn malformed_create_and_fork_success_payloads_settle_unknown_without_retry() {
    let create_transport = Arc::new(RecordingTransport::default());
    create_transport
        .push_response("thread/start", json!({}))
        .await;
    let create_service = service(create_transport.clone()).await;
    let create = CreateAgentCommand {
        meta: meta("malformed-create"),
        requested_source: None,
        initial_context: None,
    };
    let create_effect = create.meta.effect_id.clone();

    let error = create_service
        .create(create.clone())
        .await
        .expect_err("malformed successful create response");
    assert_eq!(error.code, AgentServiceErrorCode::ProtocolViolation);
    assert!(matches!(
        create_service
            .inspect(create_effect)
            .await
            .expect("inspect create")
            .state,
        AgentEffectInspectionState::Unknown
    ));
    create_service
        .create(create)
        .await
        .expect_err("unknown create must not dispatch twice");
    assert_eq!(create_transport.methods().await, vec!["thread/start"]);

    let fork_transport = Arc::new(RecordingTransport::default());
    let fork_service = service(fork_transport.clone()).await;
    let parent = create_source(fork_service.as_ref(), &fork_transport).await;
    fork_transport.push_response("thread/fork", json!({})).await;
    let fork = ForkAgentCommand {
        meta: meta("malformed-fork"),
        source: parent,
        requested_child_source: None,
        cutoff: AgentForkPoint::Head,
    };
    let fork_effect = fork.meta.effect_id.clone();

    let error = fork_service
        .fork(fork.clone())
        .await
        .expect_err("malformed successful fork response");
    assert_eq!(error.code, AgentServiceErrorCode::ProtocolViolation);
    assert!(matches!(
        fork_service
            .inspect(fork_effect)
            .await
            .expect("inspect fork")
            .state,
        AgentEffectInspectionState::Unknown
    ));
    fork_service
        .fork(fork)
        .await
        .expect_err("unknown fork must not dispatch twice");
    assert_eq!(
        fork_transport.methods().await,
        vec!["thread/start", "thread/fork"]
    );
}

#[tokio::test]
async fn malformed_surface_apply_and_revoke_settle_unknown_without_retry() {
    let apply_transport = Arc::new(RecordingTransport::default());
    let apply_service = service(apply_transport.clone()).await;
    let apply_source = create_source(apply_service.as_ref(), &apply_transport).await;
    let apply =
        immutable_surface_command(apply_service.as_ref(), apply_source, "malformed-apply").await;
    let apply_effect = apply.effect_id.clone();
    apply_transport
        .push_response("thread/resume", json!({}))
        .await;

    let error = apply_service
        .apply_surface(apply.clone())
        .await
        .expect_err("malformed successful apply response");
    assert_eq!(error.code, AgentServiceErrorCode::ProtocolViolation);
    assert!(matches!(
        apply_service
            .inspect(apply_effect)
            .await
            .expect("inspect apply")
            .state,
        AgentEffectInspectionState::Unknown
    ));
    apply_service
        .apply_surface(apply)
        .await
        .expect_err("unknown apply must not dispatch twice");
    assert_eq!(
        apply_transport.methods().await,
        vec!["thread/start", "thread/resume"]
    );

    let revoke_transport = Arc::new(RecordingTransport::default());
    let revoke_service = service(revoke_transport.clone()).await;
    let revoke_source = create_source(revoke_service.as_ref(), &revoke_transport).await;
    let apply = immutable_surface_command(
        revoke_service.as_ref(),
        revoke_source.clone(),
        "before-revoke",
    )
    .await;
    revoke_transport
        .push_response("thread/resume", json!({"thread": {"id": "thread-parent"}}))
        .await;
    revoke_service
        .apply_surface(apply)
        .await
        .expect("prepare applied surface");
    let revoke = RevokeBoundAgentSurface {
        command_id: AgentCommandId::new("revoke-malformed-command").expect("command"),
        effect_id: AgentEffectIdentity::new("revoke-malformed-effect").expect("effect"),
        idempotency_key: AgentIdempotencyKey::new("revoke-malformed-idem").expect("idempotency"),
        binding_generation: AgentBindingGeneration(7),
        source: revoke_source,
        expected_revision: agentdash_agent_service_api::AgentSurfaceRevision(2),
    };
    let revoke_effect = revoke.effect_id.clone();
    revoke_transport
        .push_response("thread/resume", json!({"thread": {"id": "wrong-thread"}}))
        .await;

    let error = revoke_service
        .revoke_surface(revoke.clone())
        .await
        .expect_err("mismatched successful revoke response");
    assert_eq!(error.code, AgentServiceErrorCode::ProtocolViolation);
    assert!(matches!(
        revoke_service
            .inspect(revoke_effect)
            .await
            .expect("inspect revoke")
            .state,
        AgentEffectInspectionState::Unknown
    ));
    revoke_service
        .revoke_surface(revoke)
        .await
        .expect_err("unknown revoke must not dispatch twice");
    assert_eq!(
        revoke_transport.methods().await,
        vec!["thread/start", "thread/resume", "thread/resume"]
    );
}

#[tokio::test]
async fn unknown_interaction_response_outcome_enters_effect_ledger_and_is_not_retried() {
    let transport = Arc::new(RecordingTransport::default());
    let service = service(transport.clone()).await;
    let source = create_source(service.as_ref(), &transport).await;
    transport
        .observations
        .lock()
        .await
        .push_back(CodexAppServerObservationPage {
            observations: vec![
                CodexAppServerObservation::server_request(
                    3,
                    json!(44),
                    "item/commandExecution/requestApproval",
                    json!({
                        "approvalId": "approval-unknown",
                        "threadId": "thread-parent",
                        "turnId": "turn-1",
                        "itemId": "item-1",
                        "startedAtMs": 1,
                        "command": "echo hi",
                        "reason": "approve?"
                    }),
                )
                .expect("typed command approval"),
            ],
            next_sequence: Some(3),
            gap: false,
        });
    service
        .changes(AgentChangesQuery {
            source: source.clone(),
            after: None,
            limit: 16,
        })
        .await
        .expect("interaction");
    transport
        .push_respond_error(CodexCompleteAgentTransportError::unavailable(
            "response acknowledgement lost",
            true,
        ))
        .await;
    let command = AgentCommandEnvelope {
        meta: meta("interaction-unknown"),
        source,
        command: AgentCommand::ResolveInteraction {
            interaction_id: agentdash_agent_service_api::AgentInteractionId::new(
                "approval-unknown",
            )
            .expect("interaction"),
            response: agentdash_agent_service_api::AgentInteractionResponse::Approved,
        },
    };
    let effect = command.meta.effect_id.clone();

    service
        .execute(command.clone())
        .await
        .expect_err("unknown interaction response");
    assert!(matches!(
        service.inspect(effect).await.expect("inspect").state,
        AgentEffectInspectionState::Unknown
    ));
    service
        .execute(command)
        .await
        .expect_err("unknown interaction effect must not respond twice");
    assert_eq!(transport.responses_to_server.lock().await.len(), 1);
}

#[tokio::test]
async fn missing_and_unknown_vendor_statuses_never_project_as_terminal() {
    let transport = Arc::new(RecordingTransport::default());
    let service = service(transport.clone()).await;
    let source = create_source(service.as_ref(), &transport).await;
    transport
        .push_response(
            "thread/read",
            json!({
                "thread": {
                    "id": "thread-parent",
                    "turns": [
                        {
                            "id": "turn-missing",
                            "items": [{"id": "item-missing", "type": "agentMessage", "text": "?"}]
                        },
                        {
                            "id": "turn-future",
                            "status": "futureState",
                            "items": [{
                                "id": "item-future",
                                "type": "agentMessage",
                                "text": "?",
                                "status": "futureState"
                            }]
                        },
                        {
                            "id": "turn-completed",
                            "status": "completed",
                            "items": [{
                                "id": "item-completed",
                                "type": "agentMessage",
                                "text": "done",
                                "status": "completed"
                            }]
                        }
                    ]
                }
            }),
        )
        .await;

    let snapshot = service
        .read(AgentReadQuery {
            source,
            at_revision: None,
        })
        .await
        .expect("read");
    assert_eq!(
        snapshot.turns[0].status,
        agentdash_agent_service_api::AgentEntityStatus::Accepted
    );
    assert_eq!(
        snapshot.turns[0].items[0].status,
        agentdash_agent_service_api::AgentEntityStatus::Accepted
    );
    assert_eq!(
        snapshot.turns[1].status,
        agentdash_agent_service_api::AgentEntityStatus::Accepted
    );
    assert_eq!(
        snapshot.turns[1].items[0].status,
        agentdash_agent_service_api::AgentEntityStatus::Accepted
    );
    assert_eq!(
        snapshot.turns[2].status,
        agentdash_agent_service_api::AgentEntityStatus::Completed
    );
    assert_eq!(
        snapshot.turns[2].items[0].status,
        agentdash_agent_service_api::AgentEntityStatus::Completed
    );
}

#[allow(dead_code)]
fn _compile_surface_revoke(_: RevokeBoundAgentSurface) {}

#[allow(dead_code)]
fn _compile_initial_evidence(_: AppliedInitialContextEvidence) {}
