use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use agentdash_agent::dash::{
    AgentTurnId as DashTurnId, ContextRevision, DashCompactionRequest, DashCompactionResult,
    DashCompactor, DashCoreError, DashCoreEvent, DashExecutionCallbacks, DashExecutionDependencies,
    DashFinishReason, DashProvider, DashProviderEvent, DashProviderEventStream,
    DashProviderRequest, DashServiceError, DashToolCall, DashToolCallbacks, DashToolResult,
};
use agentdash_agent_service_api::{
    AgentBindingGeneration, AgentCallbackRouteId, AgentChangesQuery, AgentCommand,
    AgentCommandEnvelope, AgentCommandId, AgentCommandMeta, AgentContextPackageId,
    AgentContextSchemaVersion, AgentContextSourceCoordinate, AgentContextSourceRevision,
    AgentEffectIdentity, AgentEffectInspectionState, AgentForkCutoffKind, AgentForkPoint,
    AgentHookBlockingSemantics, AgentHookDecision, AgentHookInvocation, AgentHookMutationKind,
    AgentHookPoint, AgentHostCallbackBinding, AgentHostCallbackError, AgentHostCallbacks,
    AgentIdempotencyKey, AgentInput, AgentInputContent, AgentPayloadDigest, AgentProfileDigest,
    AgentReadQuery, AgentReceiptState, AgentServiceInstanceId, AgentSnapshotRevision,
    AgentSourceCoordinate, AgentSurfaceContributionPayload, AgentSurfaceDigest,
    AgentSurfaceRevision, AgentSurfaceRoute, AgentSurfaceSemanticFacet, AgentTerminalOutcome,
    AgentToolDelivery, AgentToolInvocation, AgentToolName, AgentToolResult, AgentToolSemanticFacet,
    AgentToolUpdateSemantics, ApplyBoundAgentSurface, BoundAgentSurface,
    BoundAgentSurfaceContribution, CompleteAgentService, ContextAuthorityKind, ContextProvenance,
    CreateAgentCommand, ForkAgentCommand, InitialAgentContextPackage,
    InitialContextAppliedEvidence, InitialContextContribution, InitialContextDeliveryFidelity,
    InitialContextMode, ResumeAgentCommand, SemanticFidelity,
};
use agentdash_integration_native_agent::{
    DashAgentCompleteService, native_complete_agent_registration,
};
use async_trait::async_trait;
use futures::{StreamExt, stream};
use tokio::sync::Notify;

struct FixtureProvider;

#[async_trait]
impl DashProvider for FixtureProvider {
    async fn stream(
        &self,
        _: DashProviderRequest,
    ) -> Result<DashProviderEventStream, DashCoreError> {
        Ok(Box::pin(stream::iter([
            Ok(DashProviderEvent::TextDelta {
                delta: "fixture answer".into(),
            }),
            Ok(DashProviderEvent::Completed {
                finish_reason: DashFinishReason::Stop,
                input_tokens: 1,
                output_tokens: 2,
            }),
        ])))
    }
}

struct FixtureTools;

#[async_trait]
impl DashToolCallbacks for FixtureTools {
    async fn invoke(
        &self,
        _: &DashTurnId,
        _: DashToolCall,
    ) -> Result<DashToolResult, DashCoreError> {
        Err(DashCoreError::Tool {
            message: "fixture provider does not call tools".into(),
            retryable: false,
        })
    }
}

struct FixtureCallbacks;

#[async_trait]
impl DashExecutionCallbacks for FixtureCallbacks {
    async fn emit(&self, _: DashCoreEvent) -> Result<(), DashCoreError> {
        Ok(())
    }
}

struct FixtureHostCallbacks;

#[async_trait]
impl AgentHostCallbacks for FixtureHostCallbacks {
    async fn invoke_tool(
        &self,
        _: AgentToolInvocation,
    ) -> Result<AgentToolResult, AgentHostCallbackError> {
        Ok(AgentToolResult::Completed {
            output: serde_json::json!({"ok": true}),
        })
    }

    async fn invoke_hook(
        &self,
        _: AgentHookInvocation,
    ) -> Result<AgentHookDecision, AgentHostCallbackError> {
        Ok(AgentHookDecision::Allow)
    }
}

struct FixtureCompactor;

#[async_trait]
impl DashCompactor for FixtureCompactor {
    async fn compact(
        &self,
        request: DashCompactionRequest,
    ) -> Result<DashCompactionResult, DashServiceError> {
        Ok(DashCompactionResult {
            revision: ContextRevision::new("fixture-context-r1"),
            summary: "fixture compacted summary".into(),
            retained_from: request
                .history
                .entries()
                .last()
                .map(|entry| entry.entry_id.clone()),
        })
    }
}

fn service() -> DashAgentCompleteService {
    DashAgentCompleteService::with_execution(DashExecutionDependencies {
        provider: Arc::new(FixtureProvider),
        tools: Arc::new(FixtureTools),
        callbacks: Arc::new(FixtureCallbacks),
        compactor: Arc::new(FixtureCompactor),
    })
}

#[tokio::test]
async fn production_registration_packages_the_complete_dash_service_without_registering_a_driver() {
    let registration = native_complete_agent_registration(
        AgentServiceInstanceId::new("native-complete-1").unwrap(),
        DashExecutionDependencies {
            provider: Arc::new(FixtureProvider),
            tools: Arc::new(FixtureTools),
            callbacks: Arc::new(FixtureCallbacks),
            compactor: Arc::new(FixtureCompactor),
        },
        Arc::new(FixtureHostCallbacks),
    );

    assert_eq!(registration.instance_id.as_str(), "native-complete-1");
    assert_eq!(
        registration
            .service
            .describe()
            .await
            .unwrap()
            .definition_id
            .as_str(),
        "dash-agent"
    );
}

fn service_with_provider(provider: Arc<dyn DashProvider>) -> DashAgentCompleteService {
    service_with(provider, Arc::new(FixtureCompactor))
}

fn service_with(
    provider: Arc<dyn DashProvider>,
    compactor: Arc<dyn DashCompactor>,
) -> DashAgentCompleteService {
    DashAgentCompleteService::with_execution(DashExecutionDependencies {
        provider,
        tools: Arc::new(FixtureTools),
        callbacks: Arc::new(FixtureCallbacks),
        compactor,
    })
}

struct ErrorProvider {
    error: DashCoreError,
}

#[async_trait]
impl DashProvider for ErrorProvider {
    async fn stream(
        &self,
        _: DashProviderRequest,
    ) -> Result<DashProviderEventStream, DashCoreError> {
        Err(self.error.clone())
    }
}

struct SteerProvider {
    started: Arc<Notify>,
    release: Arc<Notify>,
}

#[async_trait]
impl DashProvider for SteerProvider {
    async fn stream(
        &self,
        _: DashProviderRequest,
    ) -> Result<DashProviderEventStream, DashCoreError> {
        self.started.notify_one();
        let release = self.release.clone();
        Ok(Box::pin(
            stream::once(async move {
                release.notified().await;
                Ok(DashProviderEvent::TextDelta {
                    delta: "steered answer".into(),
                })
            })
            .chain(stream::iter([Ok(DashProviderEvent::Completed {
                finish_reason: DashFinishReason::Stop,
                input_tokens: 1,
                output_tokens: 1,
            })])),
        ))
    }

    async fn steer(&self, _: &DashTurnId, _: &str) -> Result<(), DashCoreError> {
        self.release.notify_one();
        Ok(())
    }
}

struct BlockingProvider {
    started: Arc<Notify>,
}

#[async_trait]
impl DashProvider for BlockingProvider {
    async fn stream(
        &self,
        _: DashProviderRequest,
    ) -> Result<DashProviderEventStream, DashCoreError> {
        self.started.notify_one();
        Ok(Box::pin(stream::pending()))
    }
}

struct InteractionProvider;

#[async_trait]
impl DashProvider for InteractionProvider {
    async fn stream(
        &self,
        _: DashProviderRequest,
    ) -> Result<DashProviderEventStream, DashCoreError> {
        Err(DashCoreError::InteractionRequired {
            interaction_id: "interaction-1".into(),
            prompt: "approve?".into(),
        })
    }
}

struct OverflowProvider {
    calls: AtomicUsize,
}

struct FailingCompactor {
    lost: bool,
}

#[async_trait]
impl DashCompactor for FailingCompactor {
    async fn compact(
        &self,
        _: DashCompactionRequest,
    ) -> Result<DashCompactionResult, DashServiceError> {
        if self.lost {
            Err(DashServiceError::Lost {
                message: "compaction outcome unknown".into(),
            })
        } else {
            Err(DashServiceError::Unavailable {
                message: "compactor unavailable".into(),
                retryable: true,
            })
        }
    }
}

struct OverflowThenErrorProvider {
    calls: AtomicUsize,
    error: DashCoreError,
}

#[async_trait]
impl DashProvider for OverflowThenErrorProvider {
    async fn stream(
        &self,
        _: DashProviderRequest,
    ) -> Result<DashProviderEventStream, DashCoreError> {
        if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
            Err(DashCoreError::ContextOverflow)
        } else {
            Err(self.error.clone())
        }
    }
}

#[async_trait]
impl DashProvider for OverflowProvider {
    async fn stream(
        &self,
        _: DashProviderRequest,
    ) -> Result<DashProviderEventStream, DashCoreError> {
        if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
            return Err(DashCoreError::ContextOverflow);
        }
        Ok(Box::pin(stream::iter([
            Ok(DashProviderEvent::TextDelta {
                delta: "answer after compaction".into(),
            }),
            Ok(DashProviderEvent::Completed {
                finish_reason: DashFinishReason::Stop,
                input_tokens: 1,
                output_tokens: 1,
            }),
        ])))
    }
}

fn meta(command: &str, effect: &str) -> AgentCommandMeta {
    AgentCommandMeta {
        command_id: AgentCommandId::new(command).unwrap(),
        effect_id: AgentEffectIdentity::new(effect).unwrap(),
        idempotency_key: AgentIdempotencyKey::new(format!("idem-{command}")).unwrap(),
        binding_generation: AgentBindingGeneration(1),
        expected_snapshot_revision: None,
    }
}

fn initial_package() -> InitialAgentContextPackage {
    let package_id = AgentContextPackageId::new("package-1").unwrap();
    let contribution = InitialContextContribution::CompactSummary {
        summary: "parent summary".into(),
        provenance: ContextProvenance {
            authority: ContextAuthorityKind::AgentHistory,
            source: AgentContextSourceCoordinate::new("parent-source").unwrap(),
            revision: AgentContextSourceRevision::new("parent-r7").unwrap(),
            digest: AgentPayloadDigest::new("sha256:parent-r7").unwrap(),
        },
    };
    let digest = InitialAgentContextPackage::calculated_digest(
        &package_id,
        AgentContextSchemaVersion(1),
        InitialContextMode::Compact,
        std::slice::from_ref(&contribution),
    );
    InitialAgentContextPackage {
        package_id,
        schema_version: AgentContextSchemaVersion(1),
        mode: InitialContextMode::Compact,
        contributions: vec![contribution],
        digest,
    }
}

#[tokio::test]
async fn native_complete_agent_create_input_and_fork_use_dash_history_authority() {
    let service = service();
    let descriptor = service.describe().await.unwrap();
    assert!(
        descriptor
            .profile
            .fork
            .supports_exact(AgentForkCutoffKind::Head)
    );
    assert_eq!(
        descriptor.profile.initial_context.applied_evidence,
        InitialContextAppliedEvidence::PackageDigest
    );
    let tool = descriptor
        .profile
        .surface
        .facets
        .iter()
        .find_map(|facet| match &facet.semantics {
            AgentSurfaceSemanticFacet::Tool(tool) => Some(tool),
            _ => None,
        })
        .unwrap();
    assert_eq!(tool.delivery, AgentToolDelivery::AgentNativeCallback);
    assert_eq!(tool.invocation, SemanticFidelity::Exact);
    assert_eq!(tool.update, AgentToolUpdateSemantics::HotUpdate);
    let before_tool = descriptor
        .profile
        .surface
        .facets
        .iter()
        .find_map(|facet| match &facet.semantics {
            AgentSurfaceSemanticFacet::Hook(hook) if hook.point == AgentHookPoint::BeforeTool => {
                Some(hook)
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(
        before_tool.blocking,
        AgentHookBlockingSemantics::Blocking {
            fidelity: SemanticFidelity::Exact
        }
    );
    assert_eq!(
        before_tool
            .mutations
            .get(&AgentHookMutationKind::RewriteInput),
        Some(&SemanticFidelity::Exact)
    );

    let parent = AgentSourceCoordinate::new("dash-parent").unwrap();
    let package = initial_package();
    let create = service
        .create(CreateAgentCommand {
            meta: meta("create-parent", "effect-create-parent"),
            requested_source: Some(parent.clone()),
            initial_context: Some(package.clone()),
        })
        .await
        .unwrap();
    let evidence = create.initial_context.unwrap();
    assert_eq!(evidence.package_digest, package.digest);
    assert_eq!(
        evidence.fidelity,
        InitialContextDeliveryFidelity::TypedNative
    );
    assert!(evidence.satisfies(InitialContextAppliedEvidence::PackageDigest));
    assert_eq!(create.snapshot_revision, Some(AgentSnapshotRevision(1)));

    let submit = service
        .execute(AgentCommandEnvelope {
            meta: meta("input-1", "effect-input-1"),
            source: parent.clone(),
            command: AgentCommand::SubmitInput {
                input: AgentInput {
                    content: vec![AgentInputContent::Text {
                        text: "first ordinary input".into(),
                    }],
                },
            },
        })
        .await
        .unwrap();
    assert_eq!(
        submit.state,
        AgentReceiptState::Terminal {
            outcome: AgentTerminalOutcome::Succeeded
        }
    );
    assert_eq!(submit.snapshot_revision, Some(AgentSnapshotRevision(7)));
    assert!(matches!(
        service
            .inspect(AgentEffectIdentity::new("effect-input-1").unwrap())
            .await
            .unwrap()
            .state,
        AgentEffectInspectionState::Applied {
            terminal: Some(AgentTerminalOutcome::Succeeded),
            ..
        }
    ));

    let changes = service
        .changes(AgentChangesQuery {
            source: parent.clone(),
            after: None,
            limit: 10,
        })
        .await
        .unwrap();
    assert_eq!(changes.changes.len(), 9);
    assert_eq!(changes.changes[0].cursor.as_str(), "1:0");
    assert_eq!(changes.changes[1].cursor.as_str(), "2:0");
    assert!(matches!(
        changes.changes[3].payload,
        agentdash_agent_service_api::AgentChangePayload::ActiveTurnChanged {
            active_turn_id: Some(_)
        }
    ));
    assert!(matches!(
        changes.changes[8].payload,
        agentdash_agent_service_api::AgentChangePayload::ActiveTurnChanged {
            active_turn_id: None
        }
    ));

    let fork_command = ForkAgentCommand {
        meta: meta("fork-child", "effect-fork-child"),
        source: parent.clone(),
        requested_child_source: Some(AgentSourceCoordinate::new("dash-child").unwrap()),
        cutoff: AgentForkPoint::Head,
    };
    let forked = service.fork(fork_command.clone()).await.unwrap();
    assert_eq!(
        forked.state,
        AgentReceiptState::Terminal {
            outcome: AgentTerminalOutcome::Succeeded
        }
    );
    let child = forked.child_source.clone().unwrap();
    assert!(forked.child_history_digest.is_some());

    // Replaying the stable effect returns the same child rather than creating another fork.
    let replayed = service.fork(fork_command).await.unwrap();
    assert_eq!(replayed.child_source, Some(child.clone()));
    let inspection = service
        .inspect(AgentEffectIdentity::new("effect-fork-child").unwrap())
        .await
        .unwrap();
    assert!(matches!(
        inspection.state,
        AgentEffectInspectionState::Applied {
            child_source: Some(ref inspected),
            ..
        } if inspected == &child
    ));

    let child_snapshot = service
        .read(AgentReadQuery {
            source: child,
            at_revision: None,
        })
        .await
        .unwrap();
    assert_eq!(
        child_snapshot.initial_context.unwrap().package_digest,
        package.digest
    );
}

#[tokio::test]
async fn unsupported_input_is_rejected_before_history_changes() {
    let service = service();
    let source = AgentSourceCoordinate::new("dash-text-only").unwrap();
    service
        .create(CreateAgentCommand {
            meta: meta("create", "effect-create"),
            requested_source: Some(source.clone()),
            initial_context: None,
        })
        .await
        .unwrap();

    let error = service
        .execute(AgentCommandEnvelope {
            meta: meta("structured", "effect-structured"),
            source: source.clone(),
            command: AgentCommand::SubmitInput {
                input: AgentInput {
                    content: vec![AgentInputContent::Structured {
                        schema: "example".into(),
                        value: serde_json::json!({"x": 1}),
                    }],
                },
            },
        })
        .await
        .unwrap_err();
    assert_eq!(
        error.code,
        agentdash_agent_service_api::AgentServiceErrorCode::Unsupported
    );
    let snapshot = service
        .read(AgentReadQuery {
            source,
            at_revision: None,
        })
        .await
        .unwrap();
    assert_eq!(snapshot.revision, AgentSnapshotRevision(0));
}

#[tokio::test]
async fn surface_apply_preserves_exact_tool_semantics_and_rejects_route_substitution() {
    let service = service();
    let source = AgentSourceCoordinate::new("dash-surface").unwrap();
    service
        .create(CreateAgentCommand {
            meta: meta("create-surface", "effect-create-surface"),
            requested_source: Some(source.clone()),
            initial_context: None,
        })
        .await
        .unwrap();

    let semantics = AgentSurfaceSemanticFacet::Tool(AgentToolSemanticFacet {
        delivery: AgentToolDelivery::AgentNativeCallback,
        invocation: SemanticFidelity::Exact,
        update: AgentToolUpdateSemantics::HotUpdate,
    });
    let contribution = BoundAgentSurfaceContribution {
        key: "tool:read".into(),
        required: true,
        route: AgentSurfaceRoute::AgentNativeCallback,
        fidelity: SemanticFidelity::Exact,
        semantics: semantics.clone(),
        payload: AgentSurfaceContributionPayload::Tool {
            name: AgentToolName::new("read").unwrap(),
            description: "read".into(),
            input_schema: serde_json::json!({"type": "object"}),
            output_schema: None,
        },
        payload_digest: AgentPayloadDigest::new("sha256:tool-read").unwrap(),
    };
    let apply = |route, effect: &str| ApplyBoundAgentSurface {
        command_id: AgentCommandId::new(format!("command-{effect}")).unwrap(),
        effect_id: AgentEffectIdentity::new(effect).unwrap(),
        idempotency_key: AgentIdempotencyKey::new(format!("idem-{effect}")).unwrap(),
        source: source.clone(),
        bound_surface: BoundAgentSurface {
            revision: AgentSurfaceRevision(1),
            digest: AgentSurfaceDigest::new("surface-1").unwrap(),
            offer_profile_digest: AgentProfileDigest::new("dash-agent-profile-v1").unwrap(),
            contributions: vec![BoundAgentSurfaceContribution {
                route,
                ..contribution.clone()
            }],
        },
        callbacks: AgentHostCallbackBinding {
            route_id: AgentCallbackRouteId::new("callbacks-1").unwrap(),
            binding_generation: AgentBindingGeneration(1),
            delivery: AgentSurfaceRoute::AgentNativeCallback,
            default_deadline_ms: 5_000,
        },
    };

    let receipt = service
        .apply_surface(apply(
            AgentSurfaceRoute::AgentNativeCallback,
            "effect-apply",
        ))
        .await
        .unwrap();
    assert_eq!(receipt.applied.contributions[0].semantics, semantics);
    assert_eq!(
        receipt.applied.contributions[0].fidelity,
        SemanticFidelity::Exact
    );

    let error = service
        .apply_surface(apply(
            AgentSurfaceRoute::RuntimeToolBroker,
            "effect-wrong-route",
        ))
        .await
        .unwrap_err();
    assert_eq!(
        error.code,
        agentdash_agent_service_api::AgentServiceErrorCode::Unsupported
    );
}

#[tokio::test]
async fn manual_compaction_is_exposed_as_detailed_history_derived_turn_and_change() {
    let service = service();
    let source = AgentSourceCoordinate::new("dash-compaction").unwrap();
    service
        .create(CreateAgentCommand {
            meta: meta("create-compaction", "effect-create-compaction"),
            requested_source: Some(source.clone()),
            initial_context: None,
        })
        .await
        .unwrap();
    let receipt = service
        .execute(AgentCommandEnvelope {
            meta: meta("compact-1", "effect-compact-1"),
            source: source.clone(),
            command: AgentCommand::RequestCompaction,
        })
        .await
        .unwrap();
    assert_eq!(
        receipt.state,
        AgentReceiptState::Terminal {
            outcome: AgentTerminalOutcome::Succeeded
        }
    );

    let snapshot = service
        .read(AgentReadQuery {
            source: source.clone(),
            at_revision: None,
        })
        .await
        .unwrap();
    assert_eq!(snapshot.turns.len(), 1);
    assert_eq!(snapshot.turns[0].id.as_str(), "compact-1");
    assert_eq!(snapshot.turns[0].items.len(), 1);
    assert!(matches!(
        snapshot.turns[0].items[0].content,
        agentdash_agent_service_api::AgentItemContent::ContextCompaction
    ));

    let changes = service
        .changes(AgentChangesQuery {
            source,
            after: None,
            limit: 10,
        })
        .await
        .unwrap();
    assert_eq!(changes.changes.len(), 3);
    assert!(matches!(
        changes.changes[0].payload,
        agentdash_agent_service_api::AgentChangePayload::TurnChanged { .. }
    ));
}

async fn create_source(service: &DashAgentCompleteService, source: &str) -> AgentSourceCoordinate {
    let source = AgentSourceCoordinate::new(source).unwrap();
    service
        .create(CreateAgentCommand {
            meta: meta(
                &format!("create-{source}"),
                &format!("effect-create-{source}"),
            ),
            requested_source: Some(source.clone()),
            initial_context: None,
        })
        .await
        .unwrap();
    source
}

fn submit_envelope(
    source: AgentSourceCoordinate,
    command: &str,
    effect: &str,
) -> AgentCommandEnvelope {
    AgentCommandEnvelope {
        meta: meta(command, effect),
        source,
        command: AgentCommand::SubmitInput {
            input: AgentInput {
                content: vec![AgentInputContent::Text {
                    text: "question".into(),
                }],
            },
        },
    }
}

#[tokio::test]
async fn provider_failed_and_lost_are_terminal_and_inspectable() {
    for (name, error, expected) in [
        (
            "failed",
            DashCoreError::Provider {
                message: "retry later".into(),
                retryable: true,
            },
            AgentTerminalOutcome::Failed,
        ),
        (
            "lost",
            DashCoreError::ProviderStreamDisconnected,
            AgentTerminalOutcome::Lost,
        ),
    ] {
        let service = service_with_provider(Arc::new(ErrorProvider { error }));
        let source = create_source(&service, &format!("dash-{name}")).await;
        let effect = format!("effect-{name}");
        let receipt = service
            .execute(submit_envelope(
                source.clone(),
                &format!("input-{name}"),
                &effect,
            ))
            .await
            .unwrap();
        assert_eq!(
            receipt.state,
            AgentReceiptState::Terminal { outcome: expected }
        );
        assert!(matches!(
            service
                .inspect(AgentEffectIdentity::new(effect).unwrap())
                .await
                .unwrap()
                .state,
            AgentEffectInspectionState::Applied {
                terminal: Some(outcome),
                ..
            } if outcome == expected
        ));
    }
}

#[tokio::test]
async fn resume_preserves_state_old_tail_digest_and_effect_owner_is_exact() {
    let service = service();
    let first = create_source(&service, "dash-resume-first").await;
    let second = create_source(&service, "dash-resume-second").await;
    service
        .execute(submit_envelope(
            first.clone(),
            "input-first",
            "effect-shared",
        ))
        .await
        .unwrap();
    let before = service
        .read(AgentReadQuery {
            source: first.clone(),
            at_revision: None,
        })
        .await
        .unwrap();
    let resumed = service
        .resume(ResumeAgentCommand {
            meta: meta("resume-first", "effect-resume-first"),
            source: first.clone(),
        })
        .await
        .unwrap();
    assert_eq!(resumed.snapshot_revision, Some(before.revision));

    let old_tail = service
        .changes(AgentChangesQuery {
            source: first.clone(),
            after: None,
            limit: 100,
        })
        .await
        .unwrap();
    service
        .execute(submit_envelope(
            first.clone(),
            "input-second",
            "effect-second",
        ))
        .await
        .unwrap();
    let expanded = service
        .changes(AgentChangesQuery {
            source: first.clone(),
            after: None,
            limit: 100,
        })
        .await
        .unwrap();
    for old in &old_tail.changes {
        let replayed = expanded
            .changes
            .iter()
            .find(|change| change.cursor == old.cursor)
            .unwrap();
        assert_eq!(replayed.source_revision, old.source_revision);
    }
    assert_ne!(
        expanded.changes.first().unwrap().source_revision,
        expanded.changes.last().unwrap().source_revision
    );

    let conflict = service
        .execute(submit_envelope(
            second,
            "different-command",
            "effect-shared",
        ))
        .await
        .unwrap_err();
    assert_eq!(
        conflict.code,
        agentdash_agent_service_api::AgentServiceErrorCode::Conflict
    );
}

#[tokio::test]
async fn steer_and_interrupt_orchestrate_the_active_turn() {
    let started = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let service = Arc::new(service_with_provider(Arc::new(SteerProvider {
        started: started.clone(),
        release,
    })));
    let source = create_source(&service, "dash-steer").await;
    let submit_service = service.clone();
    let submit_source = source.clone();
    let submit = tokio::spawn(async move {
        submit_service
            .execute(submit_envelope(
                submit_source,
                "input-steer",
                "effect-input-steer",
            ))
            .await
    });
    started.notified().await;
    let steer = service
        .execute(AgentCommandEnvelope {
            meta: meta("steer", "effect-steer"),
            source: source.clone(),
            command: AgentCommand::Steer {
                expected_turn_id: agentdash_agent_service_api::AgentTurnId::new("turn:input-steer")
                    .unwrap(),
                input: AgentInput {
                    content: vec![AgentInputContent::Text {
                        text: "new direction".into(),
                    }],
                },
            },
        })
        .await
        .unwrap();
    assert!(matches!(
        steer.state,
        AgentReceiptState::Terminal {
            outcome: AgentTerminalOutcome::Succeeded
        }
    ));
    assert!(matches!(
        submit.await.unwrap().unwrap().state,
        AgentReceiptState::Terminal {
            outcome: AgentTerminalOutcome::Succeeded
        }
    ));

    let started = Arc::new(Notify::new());
    let service = Arc::new(service_with_provider(Arc::new(BlockingProvider {
        started: started.clone(),
    })));
    let source = create_source(&service, "dash-interrupt").await;
    let submit_service = service.clone();
    let submit_source = source.clone();
    let submit = tokio::spawn(async move {
        submit_service
            .execute(submit_envelope(
                submit_source,
                "input-interrupt",
                "effect-input-interrupt",
            ))
            .await
    });
    started.notified().await;
    service
        .execute(AgentCommandEnvelope {
            meta: meta("interrupt", "effect-interrupt"),
            source: source.clone(),
            command: AgentCommand::Interrupt {
                expected_turn_id: agentdash_agent_service_api::AgentTurnId::new(
                    "turn:input-interrupt",
                )
                .unwrap(),
            },
        })
        .await
        .unwrap();
    assert!(matches!(
        submit.await.unwrap().unwrap().state,
        AgentReceiptState::Terminal {
            outcome: AgentTerminalOutcome::Interrupted
        }
    ));
    assert!(
        service
            .read(AgentReadQuery {
                source,
                at_revision: None,
            })
            .await
            .unwrap()
            .active_turn_id
            .is_none()
    );
}

#[tokio::test]
async fn resolve_interaction_completes_the_suspended_turn() {
    let service = service_with_provider(Arc::new(InteractionProvider));
    let source = create_source(&service, "dash-interaction").await;
    let submit = service
        .execute(submit_envelope(
            source.clone(),
            "input-interaction",
            "effect-input-interaction",
        ))
        .await
        .unwrap();
    assert_eq!(submit.state, AgentReceiptState::Accepted);
    let snapshot = service
        .read(AgentReadQuery {
            source: source.clone(),
            at_revision: None,
        })
        .await
        .unwrap();
    assert_eq!(snapshot.interactions.len(), 1);
    service
        .execute(AgentCommandEnvelope {
            meta: meta("resolve", "effect-resolve"),
            source: source.clone(),
            command: AgentCommand::ResolveInteraction {
                interaction_id: snapshot.interactions[0].id.clone(),
                response: agentdash_agent_service_api::AgentInteractionResponse::Approved,
            },
        })
        .await
        .unwrap();
    let resolved = service
        .read(AgentReadQuery {
            source,
            at_revision: None,
        })
        .await
        .unwrap();
    assert!(resolved.interactions[0].resolved);
    assert!(resolved.active_turn_id.is_none());
}

#[tokio::test]
async fn automatic_overflow_runs_a_b_c_through_the_dash_worker() {
    let service = service_with_provider(Arc::new(OverflowProvider {
        calls: AtomicUsize::new(0),
    }));
    let source = create_source(&service, "dash-auto-compaction").await;
    let receipt = service
        .execute(submit_envelope(
            source.clone(),
            "input-auto",
            "effect-input-auto",
        ))
        .await
        .unwrap();
    assert_eq!(
        receipt.state,
        AgentReceiptState::Terminal {
            outcome: AgentTerminalOutcome::Succeeded
        }
    );
    let snapshot = service
        .read(AgentReadQuery {
            source,
            at_revision: None,
        })
        .await
        .unwrap();
    assert!(
        snapshot
            .turns
            .iter()
            .any(|turn| turn.id.as_str() == "input-auto:B")
    );
    assert!(
        snapshot
            .turns
            .iter()
            .any(|turn| turn.id.as_str() == "turn:input-auto:C")
    );
}

#[tokio::test]
async fn automatic_compaction_b_failure_and_lost_settle_original_and_block_c() {
    for (name, lost, expected) in [
        ("failed", false, AgentTerminalOutcome::Failed),
        ("lost", true, AgentTerminalOutcome::Lost),
    ] {
        let service = service_with(
            Arc::new(OverflowProvider {
                calls: AtomicUsize::new(0),
            }),
            Arc::new(FailingCompactor { lost }),
        );
        let source = create_source(&service, &format!("dash-auto-b-{name}")).await;
        let effect = format!("effect-auto-b-{name}");
        let receipt = service
            .execute(submit_envelope(
                source.clone(),
                &format!("input-auto-b-{name}"),
                &effect,
            ))
            .await
            .unwrap();
        assert_eq!(
            receipt.state,
            AgentReceiptState::Terminal { outcome: expected }
        );
        let snapshot = service
            .read(AgentReadQuery {
                source: source.clone(),
                at_revision: None,
            })
            .await
            .unwrap();
        assert!(snapshot.active_turn_id.is_none());
        assert!(
            !snapshot
                .turns
                .iter()
                .any(|turn| turn.id.as_str().ends_with(":C"))
        );
        assert!(matches!(
            service
                .inspect(AgentEffectIdentity::new(effect).unwrap())
                .await
                .unwrap()
                .state,
            AgentEffectInspectionState::Applied {
                terminal: Some(outcome),
                ..
            } if outcome == expected
        ));
    }
}

#[tokio::test]
async fn automatic_continuation_c_failure_and_lost_settle_original_and_clear_active() {
    for (name, error, expected) in [
        (
            "failed",
            DashCoreError::Provider {
                message: "continuation failed".into(),
                retryable: true,
            },
            AgentTerminalOutcome::Failed,
        ),
        (
            "lost",
            DashCoreError::ProviderStreamDisconnected,
            AgentTerminalOutcome::Lost,
        ),
    ] {
        let service = service_with_provider(Arc::new(OverflowThenErrorProvider {
            calls: AtomicUsize::new(0),
            error,
        }));
        let source = create_source(&service, &format!("dash-auto-c-{name}")).await;
        let effect = format!("effect-auto-c-{name}");
        let receipt = service
            .execute(submit_envelope(
                source.clone(),
                &format!("input-auto-c-{name}"),
                &effect,
            ))
            .await
            .unwrap();
        assert_eq!(
            receipt.state,
            AgentReceiptState::Terminal { outcome: expected }
        );
        let snapshot = service
            .read(AgentReadQuery {
                source,
                at_revision: None,
            })
            .await
            .unwrap();
        assert!(snapshot.active_turn_id.is_none());
        assert!(snapshot.turns.iter().any(|turn| {
            turn.id.as_str().ends_with(":C")
                && turn.status
                    == if expected == AgentTerminalOutcome::Lost {
                        agentdash_agent_service_api::AgentEntityStatus::Lost
                    } else {
                        agentdash_agent_service_api::AgentEntityStatus::Failed
                    }
        }));
    }
}

#[tokio::test]
async fn close_is_a_terminal_lifecycle_command() {
    let service = service();
    let source = create_source(&service, "dash-close").await;
    let receipt = service
        .execute(AgentCommandEnvelope {
            meta: meta("close", "effect-close"),
            source: source.clone(),
            command: AgentCommand::Close,
        })
        .await
        .unwrap();
    assert_eq!(
        receipt.state,
        AgentReceiptState::Terminal {
            outcome: AgentTerminalOutcome::Closed
        }
    );
    assert_eq!(
        service
            .read(AgentReadQuery {
                source,
                at_revision: None,
            })
            .await
            .unwrap()
            .lifecycle,
        agentdash_agent_service_api::AgentLifecycleStatus::Closed
    );
}
