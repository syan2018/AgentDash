use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use agentdash_agent::dash::{
    ActivityStatus, AgentHistory, AgentHistoryState, AgentItemId as DashItemId, AgentSessionId,
    AgentTurnId as DashTurnId, BranchId, CommandId, CompactionId, CompactionMode, CompactionState,
    ContextDeliveryFidelity, DashAgentChange, DashAgentChangePayload, DashAgentRepositoryState,
    DashAgentRepositoryStore, DashAgentService, DashChangeCursor, DashCommandRequest,
    DashExecutionDependencies, DashExecutionFailure, DashPublicCommand, DashReceiptState,
    DashServiceError, DashSurface, DashTerminalOutcome, DashToolDefinition, ForkCutoff,
    HistoryPayload, InitialContextContribution, InitialContextInstallation, InitialContextMode,
    InteractionId as DashInteractionId, InteractionState, ItemDetails,
};
use agentdash_agent_service_api::{
    AgentAppliedEffectOutcome, AgentCapabilityProfile, AgentChange, AgentChangePage,
    AgentChangePayload, AgentChangesQuery, AgentCommand, AgentCommandCapability,
    AgentCommandEnvelope, AgentCommandReceipt, AgentCompactionMode, AgentConfigurationBoundary,
    AgentEffectIdentity, AgentEffectInspection, AgentEffectInspectionState, AgentEntityStatus,
    AgentExecutionFailure, AgentForkCapability, AgentForkCutoffKind, AgentForkPoint,
    AgentHookBlockingSemantics, AgentHookMutationKind, AgentHookPoint, AgentHookSemanticFacet,
    AgentHookTiming, AgentHostCallbackBinding, AgentHostCallbacks, AgentInput, AgentInputContent,
    AgentInteractionRequest, AgentInteractionResolution, AgentInteractionSnapshot,
    AgentInteractionStatus, AgentItemBody, AgentItemPresentation, AgentItemSnapshot,
    AgentItemTerminalEvidence, AgentItemTransition, AgentItemUpdate, AgentLifecycleCapability,
    AgentLifecycleStatus, AgentPayloadDigest, AgentProfileDigest, AgentReadQuery,
    AgentReceiptState, AgentServiceDefinitionId, AgentServiceDescriptor, AgentServiceError,
    AgentServiceErrorCode, AgentServiceInstanceId, AgentSnapshot, AgentSnapshotAuthority,
    AgentSnapshotRevision, AgentSnapshotSource, AgentSourceChangeLevel, AgentSourceCoordinate,
    AgentSourceCursor, AgentSourceRevision, AgentSurfaceCapabilityFacet, AgentSurfaceProfile,
    AgentSurfaceRoute, AgentSurfaceSemanticFacet, AgentTerminalOutcome, AgentTerminalStatus,
    AgentToolDelivery, AgentToolSemanticFacet, AgentToolUpdateSemantics, AgentTurnSnapshot,
    AppliedAgentCommandReceipt, AppliedAgentSurface, AppliedAgentSurfaceContribution,
    AppliedAgentSurfaceReceipt, AppliedContributionStatus, AppliedForkAgentReceipt,
    AppliedInitialContextEvidence, ApplyBoundAgentSurface, BoundAgentSurface,
    BoundAgentSurfaceContribution, CompleteAgentService, CreateAgentCommand, ForkAgentCommand,
    ForkAgentReceipt, InitialAgentContextPackage, InitialContextAppliedEvidence,
    InitialContextContributionKind, InitialContextDeliveryFidelity, InitialContextProfile,
    ResumeAgentCommand, RevokeBoundAgentSurface, SemanticFidelity,
};
use agentdash_integration_api::{
    AgentDashIntegration, CompleteAgentPlacementRequirement, CompleteAgentRegistrationClaim,
    CompleteAgentRegistrationContribution, CompleteAgentServiceFactory,
    CompleteAgentServiceFactoryError,
};
use async_trait::async_trait;
use sha2::{Digest, Sha256};

use crate::DashAgentCoreToolCallbacks;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DashCompleteSourceMetadata {
    pub applied_surface: Option<AppliedAgentSurface>,
    pub initial_context: Option<AppliedInitialContextEvidence>,
    pub callback_surface: Option<BoundAgentSurface>,
    pub callback_binding: Option<AgentHostCallbackBinding>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DashCompleteEffectRecord {
    pub request_fingerprint: String,
    pub inspection: AgentEffectInspection,
    pub receipt: DashCompleteRecordedReceipt,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum DashCompleteRecordedReceipt {
    Command(AgentCommandReceipt),
    Fork(ForkAgentReceipt),
    ApplySurface(AppliedAgentSurfaceReceipt),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DashCompleteCommandEffectKind {
    Create,
    Resume,
    Command,
    SurfaceRevoke,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum DashCompleteSourceMutation {
    Create {
        source: AgentSourceCoordinate,
        repository: Box<DashAgentRepositoryState>,
        metadata: Box<DashCompleteSourceMetadata>,
    },
    CompareAndSwap {
        source: AgentSourceCoordinate,
        expected_repository: Box<DashAgentRepositoryState>,
        replacement_repository: Box<DashAgentRepositoryState>,
        expected_metadata: Box<DashCompleteSourceMetadata>,
        replacement_metadata: Box<DashCompleteSourceMetadata>,
    },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DashCompleteAtomicCommit {
    pub effect_id: AgentEffectIdentity,
    pub expected_effect: Option<DashCompleteEffectRecord>,
    pub replacement_effect: DashCompleteEffectRecord,
    pub source_mutations: Vec<DashCompleteSourceMutation>,
}

#[async_trait]
pub trait DashCompleteAgentStore: Send + Sync {
    fn repositories(&self) -> &dyn DashAgentRepositoryStore;

    async fn load_source(
        &self,
        source: &AgentSourceCoordinate,
    ) -> Result<Option<DashCompleteSourceMetadata>, AgentServiceError>;

    async fn load_effect(
        &self,
        identity: &AgentEffectIdentity,
    ) -> Result<Option<DashCompleteEffectRecord>, AgentServiceError>;

    /// Atomically compares and replaces the effect record together with every source mutation.
    ///
    /// Implementations must expose either the entire replacement or none of it after restart.
    /// Returning an error is allowed after the durable commit completed; callers recover by
    /// loading the effect identity and validating its typed receipt.
    async fn commit(&self, commit: DashCompleteAtomicCommit) -> Result<(), AgentServiceError>;
}

/// Complete Agent target lane backed by Dash Agent history.
///
/// S2/S3 tests construct this service directly. Production registration remains on the legacy
/// driver until the S5 activation set switches every caller and repository together.
pub struct DashAgentCompleteService {
    store: Arc<dyn DashCompleteAgentStore>,
    execution: DashExecutionDependencies,
    host_callbacks: Option<Arc<dyn AgentHostCallbacks>>,
    live_sources: tokio::sync::Mutex<BTreeMap<AgentSourceCoordinate, DashAgentService>>,
}

impl DashAgentCompleteService {
    pub fn with_store(
        execution: DashExecutionDependencies,
        store: Arc<dyn DashCompleteAgentStore>,
    ) -> Self {
        Self {
            store,
            execution,
            host_callbacks: None,
            live_sources: tokio::sync::Mutex::new(BTreeMap::new()),
        }
    }

    pub fn with_host_callbacks(
        execution: DashExecutionDependencies,
        host_callbacks: Arc<dyn AgentHostCallbacks>,
        store: Arc<dyn DashCompleteAgentStore>,
    ) -> Self {
        Self {
            store,
            execution,
            host_callbacks: Some(host_callbacks),
            live_sources: tokio::sync::Mutex::new(BTreeMap::new()),
        }
    }

    pub fn descriptor() -> AgentServiceDescriptor {
        AgentServiceDescriptor {
            definition_id: AgentServiceDefinitionId::new("dash-agent")
                .expect("static definition id"),
            title: "Dash Agent".into(),
            protocol_revision: 1,
            profile: AgentCapabilityProfile {
                lifecycle: BTreeSet::from([
                    AgentLifecycleCapability::Create,
                    AgentLifecycleCapability::Start,
                    AgentLifecycleCapability::Resume,
                    AgentLifecycleCapability::Close,
                ]),
                commands: BTreeSet::from([
                    AgentCommandCapability::SubmitInput,
                    AgentCommandCapability::Steer,
                    AgentCommandCapability::Interrupt,
                    AgentCommandCapability::RequestCompaction,
                    AgentCommandCapability::ResolveInteraction,
                ]),
                fork: AgentForkCapability {
                    cutoffs: BTreeMap::from([
                        (AgentForkCutoffKind::Head, SemanticFidelity::Exact),
                        (AgentForkCutoffKind::CompletedTurn, SemanticFidelity::Exact),
                        (AgentForkCutoffKind::Item, SemanticFidelity::Unsupported),
                        (
                            AgentForkCutoffKind::SourceCursor,
                            SemanticFidelity::Unsupported,
                        ),
                    ]),
                    lineage_fidelity: SemanticFidelity::Exact,
                    native_durability: SemanticFidelity::Exact,
                },
                compaction: BTreeMap::from([(
                    AgentCompactionMode::AgentOwnedNative,
                    SemanticFidelity::Exact,
                )]),
                source_changes: AgentSourceChangeLevel::OrderedDurableTail,
                initial_context: InitialContextProfile {
                    contribution_fidelity: BTreeMap::from([
                        (
                            InitialContextContributionKind::CompactSummary,
                            InitialContextDeliveryFidelity::TypedNative,
                        ),
                        (
                            InitialContextContributionKind::WorkflowContext,
                            InitialContextDeliveryFidelity::TypedNative,
                        ),
                        (
                            InitialContextContributionKind::ConstraintSet,
                            InitialContextDeliveryFidelity::TypedNative,
                        ),
                    ]),
                    applied_evidence: InitialContextAppliedEvidence::PackageDigest,
                    renderer_versions: BTreeSet::new(),
                },
                surface: AgentSurfaceProfile {
                    facets: vec![
                        AgentSurfaceCapabilityFacet {
                            semantics: AgentSurfaceSemanticFacet::Tool(AgentToolSemanticFacet {
                                delivery: AgentToolDelivery::AgentNativeCallback,
                                invocation: SemanticFidelity::Exact,
                                update: AgentToolUpdateSemantics::HotUpdate,
                            }),
                            routes: BTreeSet::from([AgentSurfaceRoute::AgentNativeCallback]),
                            fidelity: SemanticFidelity::Exact,
                            configuration_boundary: AgentConfigurationBoundary::HotUpdate,
                        },
                        AgentSurfaceCapabilityFacet {
                            semantics: AgentSurfaceSemanticFacet::Hook(AgentHookSemanticFacet {
                                point: AgentHookPoint::BeforeTool,
                                timing: AgentHookTiming::Before,
                                blocking: AgentHookBlockingSemantics::Blocking {
                                    fidelity: SemanticFidelity::Exact,
                                },
                                mutations: BTreeMap::from([(
                                    AgentHookMutationKind::RewriteInput,
                                    SemanticFidelity::Exact,
                                )]),
                                effects: BTreeMap::new(),
                            }),
                            routes: BTreeSet::from([AgentSurfaceRoute::AgentNativeCallback]),
                            fidelity: SemanticFidelity::Exact,
                            configuration_boundary: AgentConfigurationBoundary::HotUpdate,
                        },
                        AgentSurfaceCapabilityFacet {
                            semantics: AgentSurfaceSemanticFacet::Hook(AgentHookSemanticFacet {
                                point: AgentHookPoint::AfterTool,
                                timing: AgentHookTiming::After,
                                blocking: AgentHookBlockingSemantics::Blocking {
                                    fidelity: SemanticFidelity::Exact,
                                },
                                mutations: BTreeMap::from([(
                                    AgentHookMutationKind::RewriteResult,
                                    SemanticFidelity::Exact,
                                )]),
                                effects: BTreeMap::new(),
                            }),
                            routes: BTreeSet::from([AgentSurfaceRoute::AgentNativeCallback]),
                            fidelity: SemanticFidelity::Exact,
                            configuration_boundary: AgentConfigurationBoundary::HotUpdate,
                        },
                        immutable_surface_facet(AgentSurfaceSemanticFacet::Instruction),
                        immutable_surface_facet(AgentSurfaceSemanticFacet::Workspace),
                        immutable_surface_facet(AgentSurfaceSemanticFacet::ContextRequirement),
                    ],
                },
                inspect_effects: SemanticFidelity::Exact,
            },
            profile_digest: AgentProfileDigest::new("dash-agent-profile-v1")
                .expect("static profile digest"),
            configuration_boundary: AgentConfigurationBoundary::Create,
        }
    }

    fn source_for_create(command: &CreateAgentCommand) -> AgentSourceCoordinate {
        command.requested_source.clone().unwrap_or_else(|| {
            AgentSourceCoordinate::new(format!("dash:{}", command.meta.effect_id))
                .expect("effect identity produces a source coordinate")
        })
    }

    async fn open_source(
        &self,
        source: &AgentSourceCoordinate,
    ) -> Result<(DashAgentService, DashCompleteSourceMetadata), AgentServiceError> {
        let metadata = self
            .store
            .load_source(source)
            .await?
            .ok_or_else(|| not_found("Dash Agent source does not exist"))?;
        let service = if let Some(service) = self.live_sources.lock().await.get(source).cloned() {
            service
        } else {
            let service = DashAgentService::open_with_store(
                self.store.repositories(),
                &AgentSessionId::new(source.as_str()),
                self.execution.clone(),
            )
            .await
            .map_err(map_dash_error)?
            .ok_or_else(|| internal("Dash Agent source metadata has no durable repository"))?;
            self.live_sources
                .lock()
                .await
                .insert(source.clone(), service.clone());
            service
        };
        self.materialize_live_surface(source, &service, &metadata)
            .await?;
        Ok((service, metadata))
    }

    async fn reconcile_live_surface_from_durable_metadata(
        &self,
        source: &AgentSourceCoordinate,
        service: &DashAgentService,
    ) -> Result<DashCompleteSourceMetadata, AgentServiceError> {
        let metadata = self
            .store
            .load_source(source)
            .await?
            .ok_or_else(|| not_found("Dash Agent source does not exist"))?;
        self.materialize_live_surface(source, service, &metadata)
            .await?;
        Ok(metadata)
    }

    async fn materialize_live_surface(
        &self,
        source: &AgentSourceCoordinate,
        service: &DashAgentService,
        metadata: &DashCompleteSourceMetadata,
    ) -> Result<(), AgentServiceError> {
        match (
            &metadata.applied_surface,
            &metadata.callback_surface,
            &metadata.callback_binding,
        ) {
            (None, None, None) => {
                service
                    .replace_tool_callbacks(self.execution.tools.clone())
                    .await;
                Ok(())
            }
            (Some(applied), Some(surface), Some(binding))
                if applied_surface_matches_bound(applied, surface) =>
            {
                let requires_callbacks = surface.contributions.iter().any(|contribution| {
                    matches!(
                        contribution.semantics,
                        AgentSurfaceSemanticFacet::Tool(_) | AgentSurfaceSemanticFacet::Hook(_)
                    )
                });
                if let Some(callbacks) = &self.host_callbacks {
                    service
                        .replace_tool_callbacks(Arc::new(
                            DashAgentCoreToolCallbacks::from_bound_surface(
                                callbacks.clone(),
                                binding.route_id.clone(),
                                binding.binding_generation,
                                source.clone(),
                                binding.default_deadline_ms,
                                surface,
                            ),
                        ))
                        .await;
                    Ok(())
                } else if requires_callbacks {
                    Err(AgentServiceError::new(
                        AgentServiceErrorCode::Unavailable,
                        "Dash Agent cannot materialize durable native callbacks without AgentHostCallbacks",
                        true,
                    ))
                } else {
                    service
                        .replace_tool_callbacks(self.execution.tools.clone())
                        .await;
                    Ok(())
                }
            }
            _ => Err(internal(
                "Dash Agent durable surface metadata is incomplete or inconsistent",
            )),
        }
    }
}

#[async_trait]
impl CompleteAgentService for DashAgentCompleteService {
    async fn describe(&self) -> Result<AgentServiceDescriptor, AgentServiceError> {
        Ok(Self::descriptor())
    }

    async fn create(
        &self,
        command: CreateAgentCommand,
    ) -> Result<AgentCommandReceipt, AgentServiceError> {
        let source = Self::source_for_create(&command);
        let request_fingerprint = request_fingerprint(&command)?;
        if let Some(recorded) = self.store.load_effect(&command.meta.effect_id).await? {
            return recorded.command_receipt_for(
                &source,
                &command.meta.command_id,
                &request_fingerprint,
            );
        }
        if self.store.load_source(&source).await?.is_some() {
            return Err(conflict("requested Dash Agent source already exists"));
        }

        let history = AgentHistory::empty(
            AgentSessionId::new(source.as_str()),
            BranchId::new(format!("{}:root", source.as_str())),
        );
        let installation = command
            .initial_context
            .as_ref()
            .map(translate_initial_context)
            .transpose()?;
        let initial_evidence =
            command
                .initial_context
                .as_ref()
                .map(|package| AppliedInitialContextEvidence {
                    package_id: package.package_id.clone(),
                    package_digest: package.digest.clone(),
                    fidelity: InitialContextDeliveryFidelity::TypedNative,
                    renderer_version: None,
                    materialized_digest: None,
                });
        let repository = DashAgentService::initial_repository_state(history, installation)
            .map_err(map_dash_error)?;
        let metadata = DashCompleteSourceMetadata {
            applied_surface: None,
            initial_context: initial_evidence.clone(),
            callback_surface: None,
            callback_binding: None,
        };
        let revision = AgentSnapshotRevision(
            repository
                .history()
                .state()
                .map_err(|error| map_dash_error(error.into()))?
                .entry_count,
        );

        let receipt = AgentCommandReceipt {
            command_id: command.meta.command_id.clone(),
            effect_id: command.meta.effect_id.clone(),
            source: source.clone(),
            state: AgentReceiptState::Terminal {
                outcome: AgentTerminalOutcome::Succeeded,
            },
            snapshot_revision: Some(revision),
            initial_context: initial_evidence.clone(),
        };
        let record = command_effect_record(
            DashCompleteCommandEffectKind::Create,
            request_fingerprint,
            receipt.clone(),
            Some(AgentTerminalOutcome::Succeeded),
        );
        self.store
            .commit(DashCompleteAtomicCommit {
                effect_id: command.meta.effect_id,
                expected_effect: None,
                replacement_effect: record,
                source_mutations: vec![DashCompleteSourceMutation::Create {
                    source: source.clone(),
                    repository: Box::new(repository),
                    metadata: Box::new(metadata),
                }],
            })
            .await?;
        self.open_source(&source).await?;
        Ok(receipt)
    }

    async fn resume(
        &self,
        command: ResumeAgentCommand,
    ) -> Result<AgentCommandReceipt, AgentServiceError> {
        let request_fingerprint = request_fingerprint(&command)?;
        if let Some(recorded) = self.store.load_effect(&command.meta.effect_id).await? {
            return recorded.command_receipt_for(
                &command.source,
                &command.meta.command_id,
                &request_fingerprint,
            );
        }
        let (service, source) = self.open_source(&command.source).await?;
        let revision = service
            .read()
            .await
            .map_err(map_dash_error)?
            .state
            .entry_count;
        let receipt = AgentCommandReceipt {
            command_id: command.meta.command_id.clone(),
            effect_id: command.meta.effect_id.clone(),
            source: command.source.clone(),
            state: AgentReceiptState::Terminal {
                outcome: AgentTerminalOutcome::Succeeded,
            },
            snapshot_revision: Some(AgentSnapshotRevision(revision)),
            initial_context: source.initial_context,
        };
        self.store
            .commit(DashCompleteAtomicCommit {
                effect_id: command.meta.effect_id,
                expected_effect: None,
                replacement_effect: command_effect_record(
                    DashCompleteCommandEffectKind::Resume,
                    request_fingerprint,
                    receipt.clone(),
                    Some(AgentTerminalOutcome::Succeeded),
                ),
                source_mutations: vec![],
            })
            .await?;
        Ok(receipt)
    }

    async fn fork(&self, command: ForkAgentCommand) -> Result<ForkAgentReceipt, AgentServiceError> {
        let request_fingerprint = request_fingerprint(&command)?;
        if let Some(recorded) = self.store.load_effect(&command.meta.effect_id).await? {
            return recorded.fork_receipt_for(
                &command.source,
                &command.meta.command_id,
                &request_fingerprint,
            );
        }
        let (parent, parent_metadata) = self.open_source(&command.source).await?;
        let child_source = command.requested_child_source.clone().unwrap_or_else(|| {
            AgentSourceCoordinate::new(format!("dash:fork:{}", command.meta.effect_id))
                .expect("effect identity produces a source coordinate")
        });
        if self.store.load_source(&child_source).await?.is_some() {
            return Err(conflict("requested Dash Agent child source already exists"));
        }
        let child_repository = parent
            .fork_repository_state(
                AgentSessionId::new(child_source.as_str()),
                BranchId::new(format!("{}:fork", child_source.as_str())),
                translate_fork_cutoff(&command.cutoff)?,
            )
            .await
            .map_err(map_dash_error)?;
        let child_history = child_repository.history();
        let child_digest = AgentPayloadDigest::new(format!("sha256:{}", child_history.digest()))
            .map_err(internal)?;
        let receipt = ForkAgentReceipt {
            command_id: command.meta.command_id.clone(),
            effect_id: command.meta.effect_id.clone(),
            parent_source: command.source.clone(),
            child_source: Some(child_source.clone()),
            cutoff: command.cutoff.clone(),
            child_history_digest: Some(child_digest.clone()),
            state: AgentReceiptState::Terminal {
                outcome: AgentTerminalOutcome::Succeeded,
            },
        };
        let record = DashCompleteEffectRecord {
            request_fingerprint,
            inspection: AgentEffectInspection {
                effect_id: command.meta.effect_id.clone(),
                command_id: Some(command.meta.command_id.clone()),
                state: AgentEffectInspectionState::Applied {
                    outcome: AgentAppliedEffectOutcome::Fork {
                        receipt: AppliedForkAgentReceipt {
                            command_id: command.meta.command_id.clone(),
                            effect_id: command.meta.effect_id.clone(),
                            parent_source: command.source.clone(),
                            child_source: child_source.clone(),
                            cutoff: command.cutoff,
                            child_history_digest: child_digest,
                            terminal: Some(AgentTerminalOutcome::Succeeded),
                        },
                    },
                },
            },
            receipt: DashCompleteRecordedReceipt::Fork(receipt.clone()),
        };
        self.store
            .commit(DashCompleteAtomicCommit {
                effect_id: command.meta.effect_id,
                expected_effect: None,
                replacement_effect: record,
                source_mutations: vec![DashCompleteSourceMutation::Create {
                    source: child_source.clone(),
                    repository: Box::new(child_repository),
                    metadata: Box::new(parent_metadata),
                }],
            })
            .await?;
        self.open_source(&child_source).await?;
        Ok(receipt)
    }

    async fn execute(
        &self,
        command: AgentCommandEnvelope,
    ) -> Result<AgentCommandReceipt, AgentServiceError> {
        let request_fingerprint = request_fingerprint(&command)?;
        let accepted_record =
            if let Some(recorded) = self.store.load_effect(&command.meta.effect_id).await? {
                let receipt = recorded.command_receipt_for(
                    &command.source,
                    &command.meta.command_id,
                    &request_fingerprint,
                )?;
                if matches!(receipt.state, AgentReceiptState::Terminal { .. }) {
                    return Ok(receipt);
                }
                recorded
            } else {
                let accepted_receipt = AgentCommandReceipt {
                    command_id: command.meta.command_id.clone(),
                    effect_id: command.meta.effect_id.clone(),
                    source: command.source.clone(),
                    state: AgentReceiptState::Accepted,
                    snapshot_revision: None,
                    initial_context: None,
                };
                let accepted_record = command_effect_record(
                    DashCompleteCommandEffectKind::Command,
                    request_fingerprint.clone(),
                    accepted_receipt,
                    None,
                );
                self.store
                    .commit(DashCompleteAtomicCommit {
                        effect_id: command.meta.effect_id.clone(),
                        expected_effect: None,
                        replacement_effect: accepted_record.clone(),
                        source_mutations: vec![],
                    })
                    .await?;
                accepted_record
            };
        let (service, _) = self.open_source(&command.source).await?;
        let dash_receipt = service
            .execute(DashCommandRequest {
                command_id: CommandId::new(command.meta.command_id.as_str()),
                effect_id: agentdash_agent::dash::EffectId::new(command.meta.effect_id.as_str()),
                command: translate_public_command(&command.command)?,
            })
            .await
            .map_err(map_dash_error)?;
        let terminal = match dash_receipt.state {
            DashReceiptState::Accepted | DashReceiptState::Unknown => None,
            DashReceiptState::Terminal(outcome) => Some(service_terminal(outcome)),
        };
        let revision = service
            .read()
            .await
            .map_err(map_dash_error)?
            .state
            .entry_count;
        let receipt = AgentCommandReceipt {
            command_id: command.meta.command_id.clone(),
            effect_id: command.meta.effect_id.clone(),
            source: command.source.clone(),
            state: terminal.map_or(AgentReceiptState::Accepted, |outcome| {
                AgentReceiptState::Terminal { outcome }
            }),
            snapshot_revision: Some(AgentSnapshotRevision(revision)),
            initial_context: None,
        };
        let final_record = command_effect_record(
            DashCompleteCommandEffectKind::Command,
            request_fingerprint,
            receipt.clone(),
            terminal,
        );
        self.store
            .commit(DashCompleteAtomicCommit {
                effect_id: command.meta.effect_id,
                expected_effect: Some(accepted_record),
                replacement_effect: final_record,
                source_mutations: vec![],
            })
            .await?;
        Ok(receipt)
    }

    async fn read(&self, query: AgentReadQuery) -> Result<AgentSnapshot, AgentServiceError> {
        let (service, source) = self.open_source(&query.source).await?;
        let read = service.read().await.map_err(map_dash_error)?;
        let conversation_history =
            crate::canonical_projection::history_records(&read.history).map_err(internal)?;
        let history_state = read.state;
        let revision = AgentSnapshotRevision(history_state.entry_count);
        if query
            .at_revision
            .is_some_and(|expected| expected != revision)
        {
            return Err(conflict(
                "requested Dash Agent snapshot revision is not current",
            ));
        }
        Ok(AgentSnapshot {
            source: query.source,
            revision,
            lifecycle: if history_state.status == agentdash_agent::dash::SessionStatus::Closed {
                AgentLifecycleStatus::Closed
            } else {
                AgentLifecycleStatus::Active
            },
            active_turn_id: history_state
                .active_turn
                .as_ref()
                .map(service_turn_id)
                .transpose()?,
            turns: history_state
                .turns
                .keys()
                .map(|turn_id| {
                    turn_snapshot(
                        &history_state,
                        turn_id,
                        turn_failure(&read.history, turn_id),
                    )
                })
                .chain(
                    history_state
                        .compactions
                        .iter()
                        .map(|(id, compaction)| compaction_snapshot(id, compaction)),
                )
                .collect::<Result<Vec<_>, _>>()?,
            interactions: history_state
                .interactions
                .iter()
                .map(|(id, interaction)| interaction_snapshot(id, interaction))
                .collect::<Result<Vec<_>, _>>()?,
            // Dash Agent history currently has no naming entry/fold rule, so the Complete Agent
            // seam must report the capability as absent instead of deriving a title from prompts.
            thread_name: None,
            source_info: AgentSnapshotSource {
                authority: AgentSnapshotAuthority::AgentAuthoritative,
                source_revision: Some(
                    AgentSourceRevision::new(format!("history:{}", read.history_digest))
                        .map_err(internal)?,
                ),
                fidelity: SemanticFidelity::Exact,
                observed_at_ms: 0,
            },
            applied_surface: source.applied_surface,
            initial_context: source.initial_context,
            conversation_history,
        })
    }

    async fn changes(
        &self,
        query: AgentChangesQuery,
    ) -> Result<AgentChangePage, AgentServiceError> {
        let (service, _) = self.open_source(&query.source).await?;
        let after = query
            .after
            .as_ref()
            .map(parse_cursor)
            .transpose()?
            .unwrap_or_else(|| DashChangeCursor::new(0, 0));
        let changes = service
            .changes(Some(after), query.limit as usize)
            .await
            .map_err(map_dash_error)?
            .into_iter()
            .map(|change| {
                let state_payload = dash_change_payload(&change)?;
                let presentation = match &change.payload {
                    DashAgentChangePayload::HistoryEntry { entry } => {
                        crate::canonical_projection::entry_records(
                            query.source.as_str(),
                            entry,
                            &change.state,
                        )
                        .map_err(internal)?
                    }
                    DashAgentChangePayload::ActiveTurnChanged { .. } => Vec::new(),
                };
                Ok(AgentChange {
                    cursor: AgentSourceCursor::new(change.cursor.encode()).map_err(internal)?,
                    source_revision: Some(
                        AgentSourceRevision::new(format!("history:{}", change.source_digest))
                            .map_err(internal)?,
                    ),
                    occurred_at_ms: 0,
                    payload: AgentChangePayload::SourceObservation {
                        state: Box::new(state_payload),
                        presentation,
                    },
                })
            })
            .collect::<Result<Vec<_>, AgentServiceError>>()?;
        let next = changes.last().map(|change| change.cursor.clone());
        Ok(AgentChangePage {
            source: query.source,
            changes,
            next,
            gap: false,
        })
    }

    async fn inspect(
        &self,
        identity: AgentEffectIdentity,
    ) -> Result<AgentEffectInspection, AgentServiceError> {
        if let Some(record) = self.store.load_effect(&identity).await? {
            return Ok(record.inspection);
        }
        Ok(AgentEffectInspection {
            effect_id: identity,
            command_id: None,
            state: AgentEffectInspectionState::NotApplied,
        })
    }

    async fn apply_surface(
        &self,
        command: ApplyBoundAgentSurface,
    ) -> Result<AppliedAgentSurfaceReceipt, AgentServiceError> {
        let request_fingerprint = request_fingerprint(&command)?;
        if let Some(recorded) = self.store.load_effect(&command.effect_id).await? {
            let receipt = recorded.apply_surface_receipt_for(
                &command.source,
                &command.command_id,
                &request_fingerprint,
            )?;
            self.open_source(&command.source).await?;
            return Ok(receipt);
        }
        let (service, metadata) = self.open_source(&command.source).await?;
        let dash_surface = dash_surface_from_bound(&command.bound_surface)?;
        let profile = Self::descriptor().profile.surface;
        if let Some(unsupported) = command
            .bound_surface
            .contributions
            .iter()
            .find(|contribution| !surface_contribution_supported(&profile, contribution))
        {
            return Err(AgentServiceError::new(
                AgentServiceErrorCode::Unsupported,
                format!(
                    "Dash Agent cannot materialize surface contribution {} with the requested route/semantics",
                    unsupported.key
                ),
                false,
            ));
        }
        if self.host_callbacks.is_none()
            && command
                .bound_surface
                .contributions
                .iter()
                .any(|contribution| {
                    matches!(
                        contribution.semantics,
                        AgentSurfaceSemanticFacet::Tool(_) | AgentSurfaceSemanticFacet::Hook(_)
                    )
                })
        {
            return Err(AgentServiceError::new(
                AgentServiceErrorCode::Unavailable,
                "Dash Agent has no AgentHostCallbacks binding for native tool/hook materialization",
                true,
            ));
        }
        let callback_surface = command.bound_surface.clone();
        let applied = AppliedAgentSurface {
            revision: command.bound_surface.revision,
            digest: command.bound_surface.digest,
            contributions: command
                .bound_surface
                .contributions
                .into_iter()
                .map(|contribution| AppliedAgentSurfaceContribution {
                    key: contribution.key,
                    route: contribution.route,
                    fidelity: contribution.fidelity,
                    semantics: contribution.semantics,
                    payload_digest: contribution.payload_digest,
                    status: AppliedContributionStatus::Applied,
                    evidence: Some("dash_agent_materialized".into()),
                })
                .collect(),
        };
        let (expected_repository, replacement_repository) = service
            .stage_surface_apply(dash_surface)
            .await
            .map_err(map_dash_error)?;
        let replacement = DashCompleteSourceMetadata {
            applied_surface: Some(applied.clone()),
            initial_context: metadata.initial_context.clone(),
            callback_surface: Some(callback_surface.clone()),
            callback_binding: Some(command.callbacks.clone()),
        };
        let receipt = AppliedAgentSurfaceReceipt {
            command_id: command.command_id.clone(),
            effect_id: command.effect_id.clone(),
            source: command.source.clone(),
            applied,
        };
        let record = DashCompleteEffectRecord {
            request_fingerprint,
            inspection: AgentEffectInspection {
                effect_id: command.effect_id.clone(),
                command_id: Some(command.command_id.clone()),
                state: AgentEffectInspectionState::Applied {
                    outcome: AgentAppliedEffectOutcome::SurfaceApply {
                        receipt: receipt.clone(),
                    },
                },
            },
            receipt: DashCompleteRecordedReceipt::ApplySurface(receipt.clone()),
        };
        let commit_result = self
            .store
            .commit(DashCompleteAtomicCommit {
                effect_id: command.effect_id.clone(),
                expected_effect: None,
                replacement_effect: record,
                source_mutations: vec![DashCompleteSourceMutation::CompareAndSwap {
                    source: command.source.clone(),
                    expected_repository: Box::new(expected_repository),
                    replacement_repository: Box::new(replacement_repository),
                    expected_metadata: Box::new(metadata),
                    replacement_metadata: Box::new(replacement),
                }],
            })
            .await;
        self.reconcile_live_surface_from_durable_metadata(&command.source, &service)
            .await?;
        commit_result?;
        Ok(receipt)
    }

    async fn revoke_surface(
        &self,
        command: RevokeBoundAgentSurface,
    ) -> Result<AgentCommandReceipt, AgentServiceError> {
        let request_fingerprint = request_fingerprint(&command)?;
        if let Some(recorded) = self.store.load_effect(&command.effect_id).await? {
            let receipt = recorded.command_receipt_for(
                &command.source,
                &command.command_id,
                &request_fingerprint,
            )?;
            self.open_source(&command.source).await?;
            return Ok(receipt);
        }
        let (service, metadata) = self.open_source(&command.source).await?;
        if metadata
            .applied_surface
            .as_ref()
            .is_some_and(|applied| applied.revision != command.expected_revision)
        {
            return Err(conflict("surface revision does not match"));
        }
        if metadata
            .callback_binding
            .as_ref()
            .is_some_and(|binding| binding.binding_generation != command.binding_generation)
        {
            return Err(AgentServiceError::new(
                AgentServiceErrorCode::StaleBindingGeneration,
                "surface revoke binding generation is stale",
                false,
            ));
        }
        let (expected_repository, replacement_repository) = service
            .stage_surface_revoke(command.expected_revision.0)
            .await
            .map_err(map_dash_error)?;
        let revision = replacement_repository
            .history()
            .state()
            .map_err(|error| map_dash_error(error.into()))?
            .entry_count;
        let receipt = AgentCommandReceipt {
            command_id: command.command_id.clone(),
            effect_id: command.effect_id.clone(),
            source: command.source.clone(),
            state: AgentReceiptState::Terminal {
                outcome: AgentTerminalOutcome::Succeeded,
            },
            snapshot_revision: Some(AgentSnapshotRevision(revision)),
            initial_context: None,
        };
        let record = command_effect_record(
            DashCompleteCommandEffectKind::SurfaceRevoke,
            request_fingerprint,
            receipt.clone(),
            Some(AgentTerminalOutcome::Succeeded),
        );
        let commit_result = self
            .store
            .commit(DashCompleteAtomicCommit {
                effect_id: command.effect_id.clone(),
                expected_effect: None,
                replacement_effect: record,
                source_mutations: vec![DashCompleteSourceMutation::CompareAndSwap {
                    source: command.source.clone(),
                    expected_repository: Box::new(expected_repository),
                    replacement_repository: Box::new(replacement_repository),
                    expected_metadata: Box::new(metadata.clone()),
                    replacement_metadata: Box::new(DashCompleteSourceMetadata {
                        applied_surface: None,
                        initial_context: metadata.initial_context,
                        callback_surface: None,
                        callback_binding: None,
                    }),
                }],
            })
            .await;
        self.reconcile_live_surface_from_durable_metadata(&command.source, &service)
            .await?;
        commit_result?;
        Ok(receipt)
    }
}

struct NativeCompleteAgentServiceFactory {
    execution: DashExecutionDependencies,
    host_callbacks: Arc<dyn AgentHostCallbacks>,
    store: Arc<dyn DashCompleteAgentStore>,
}

#[async_trait]
impl CompleteAgentServiceFactory for NativeCompleteAgentServiceFactory {
    async fn materialize(
        &self,
    ) -> Result<Arc<dyn CompleteAgentService>, CompleteAgentServiceFactoryError> {
        Ok(Arc::new(DashAgentCompleteService::with_host_callbacks(
            self.execution.clone(),
            self.host_callbacks.clone(),
            self.store.clone(),
        )))
    }
}

pub struct NativeCompleteAgentIntegration {
    registration: CompleteAgentRegistrationContribution,
}

impl NativeCompleteAgentIntegration {
    pub fn new(
        instance_id: AgentServiceInstanceId,
        execution: DashExecutionDependencies,
        host_callbacks: Arc<dyn AgentHostCallbacks>,
        store: Arc<dyn DashCompleteAgentStore>,
    ) -> Result<Self, agentdash_integration_api::CompleteAgentContributionError> {
        Ok(Self {
            registration: native_complete_agent_registration(
                instance_id,
                execution,
                host_callbacks,
                store,
            )?,
        })
    }
}

impl AgentDashIntegration for NativeCompleteAgentIntegration {
    fn name(&self) -> &str {
        "builtin.dash_agent"
    }

    fn complete_agent_registrations(&self) -> Vec<CompleteAgentRegistrationContribution> {
        vec![self.registration.clone()]
    }
}

pub fn native_complete_agent_registration(
    instance_id: AgentServiceInstanceId,
    execution: DashExecutionDependencies,
    host_callbacks: Arc<dyn AgentHostCallbacks>,
    store: Arc<dyn DashCompleteAgentStore>,
) -> Result<
    CompleteAgentRegistrationContribution,
    agentdash_integration_api::CompleteAgentContributionError,
> {
    let declared_descriptor = DashAgentCompleteService::descriptor();
    CompleteAgentRegistrationContribution::new(
        declared_descriptor,
        instance_id,
        CompleteAgentPlacementRequirement::InProcess,
        None,
        CompleteAgentRegistrationClaim {
            publisher_integration: "builtin.dash_agent".to_owned(),
            service_version: env!("CARGO_PKG_VERSION").to_owned(),
            claimed_service_build_digest: AgentPayloadDigest::new(format!(
                "dash-complete-agent:{}",
                env!("CARGO_PKG_VERSION")
            ))
            .expect("static Dash Complete Agent build digest"),
            claimed_conformance_suite_revision: "dash-complete-agent-v1".to_owned(),
        },
        Arc::new(NativeCompleteAgentServiceFactory {
            execution,
            host_callbacks,
            store,
        }),
    )
}

impl DashCompleteEffectRecord {
    fn command_receipt_for(
        &self,
        source: &AgentSourceCoordinate,
        command_id: &agentdash_agent_service_api::AgentCommandId,
        request_fingerprint: &str,
    ) -> Result<AgentCommandReceipt, AgentServiceError> {
        match &self.receipt {
            DashCompleteRecordedReceipt::Command(receipt)
                if &receipt.source == source
                    && &receipt.command_id == command_id
                    && self.request_fingerprint == request_fingerprint =>
            {
                Ok(receipt.clone())
            }
            DashCompleteRecordedReceipt::Command(_) => Err(conflict(
                "effect identity was reused by another command or source",
            )),
            DashCompleteRecordedReceipt::Fork(_) => {
                Err(conflict("effect identity belongs to a fork command"))
            }
            DashCompleteRecordedReceipt::ApplySurface(_) => Err(conflict(
                "effect identity belongs to a surface apply command",
            )),
        }
    }

    fn fork_receipt_for(
        &self,
        source: &AgentSourceCoordinate,
        command_id: &agentdash_agent_service_api::AgentCommandId,
        request_fingerprint: &str,
    ) -> Result<ForkAgentReceipt, AgentServiceError> {
        match &self.receipt {
            DashCompleteRecordedReceipt::Fork(receipt)
                if &receipt.parent_source == source
                    && &receipt.command_id == command_id
                    && self.request_fingerprint == request_fingerprint =>
            {
                Ok(receipt.clone())
            }
            DashCompleteRecordedReceipt::Fork(_) => Err(conflict(
                "effect identity was reused by another command or source",
            )),
            DashCompleteRecordedReceipt::Command(_)
            | DashCompleteRecordedReceipt::ApplySurface(_) => {
                Err(conflict("effect identity belongs to a non-fork command"))
            }
        }
    }

    fn apply_surface_receipt_for(
        &self,
        source: &AgentSourceCoordinate,
        command_id: &agentdash_agent_service_api::AgentCommandId,
        request_fingerprint: &str,
    ) -> Result<AppliedAgentSurfaceReceipt, AgentServiceError> {
        match &self.receipt {
            DashCompleteRecordedReceipt::ApplySurface(receipt)
                if &receipt.source == source
                    && &receipt.command_id == command_id
                    && self.request_fingerprint == request_fingerprint =>
            {
                Ok(receipt.clone())
            }
            DashCompleteRecordedReceipt::ApplySurface(_) => Err(conflict(
                "effect identity was reused by another command or source",
            )),
            _ => Err(conflict("effect identity belongs to a non-surface command")),
        }
    }
}

fn translate_initial_context(
    package: &InitialAgentContextPackage,
) -> Result<InitialContextInstallation, AgentServiceError> {
    if !package.digest_matches() {
        return Err(invalid_argument("initial context package digest mismatch"));
    }
    let mode = match package.mode {
        agentdash_agent_service_api::InitialContextMode::Compact => InitialContextMode::Compact,
        agentdash_agent_service_api::InitialContextMode::WorkflowOnly => {
            InitialContextMode::WorkflowOnly
        }
        agentdash_agent_service_api::InitialContextMode::ConstraintsOnly => {
            InitialContextMode::ConstraintsOnly
        }
    };
    let contributions = package
        .contributions
        .iter()
        .map(|contribution| {
            let (kind, payload, provenance) = match contribution {
                agentdash_agent_service_api::InitialContextContribution::CompactSummary {
                    summary,
                    provenance,
                } => ("compact_summary", summary.clone(), provenance),
                agentdash_agent_service_api::InitialContextContribution::WorkflowContext {
                    payload,
                    provenance,
                } => (
                    "workflow_context",
                    serde_json::to_string(payload).map_err(internal)?,
                    provenance,
                ),
                agentdash_agent_service_api::InitialContextContribution::ConstraintSet {
                    payload,
                    provenance,
                } => (
                    "constraint_set",
                    serde_json::to_string(payload).map_err(internal)?,
                    provenance,
                ),
            };
            Ok(InitialContextContribution {
                kind: kind.into(),
                payload,
                authority: format!("{:?}", provenance.authority).to_lowercase(),
                source_revision: provenance.revision.to_string(),
                digest: provenance.digest.to_string(),
            })
        })
        .collect::<Result<Vec<_>, AgentServiceError>>()?;
    Ok(InitialContextInstallation {
        package_id: package.package_id.to_string(),
        package_digest: package.digest.to_string(),
        mode,
        fidelity: ContextDeliveryFidelity::TypedNative,
        contributions,
    })
}

fn translate_fork_cutoff(cutoff: &AgentForkPoint) -> Result<ForkCutoff, AgentServiceError> {
    match cutoff {
        AgentForkPoint::Head => Ok(ForkCutoff::Head),
        AgentForkPoint::CompletedTurn { turn_id } => Ok(ForkCutoff::CompletedTurn {
            turn_id: agentdash_agent::dash::AgentTurnId::new(turn_id.as_str()),
        }),
        AgentForkPoint::Item { .. } => Err(AgentServiceError::new(
            AgentServiceErrorCode::Unsupported,
            "Dash Agent does not advertise item-cutoff fork",
            false,
        )),
        AgentForkPoint::SourceCursor { .. } => Err(AgentServiceError::new(
            AgentServiceErrorCode::Unsupported,
            "Dash Agent does not advertise source-cursor fork",
            false,
        )),
    }
}

fn translate_public_command(
    command: &AgentCommand,
) -> Result<DashPublicCommand, AgentServiceError> {
    match command {
        AgentCommand::SubmitInput { input } => Ok(DashPublicCommand::SubmitInput {
            content: text_input(input)?,
        }),
        AgentCommand::Steer {
            expected_turn_id,
            input,
        } => Ok(DashPublicCommand::Steer {
            turn_id: DashTurnId::new(expected_turn_id.as_str()),
            content: text_input(input)?,
        }),
        AgentCommand::Interrupt { expected_turn_id } => Ok(DashPublicCommand::Interrupt {
            turn_id: DashTurnId::new(expected_turn_id.as_str()),
        }),
        AgentCommand::RequestCompaction => Ok(DashPublicCommand::RequestCompaction {
            mode: CompactionMode::Manual,
        }),
        AgentCommand::ResolveInteraction {
            interaction_id,
            response,
        } => Ok(DashPublicCommand::ResolveInteraction {
            interaction_id: DashInteractionId::new(interaction_id.as_str()),
            response: serde_json::to_string(response).map_err(internal)?,
        }),
        AgentCommand::Close => Ok(DashPublicCommand::Close),
    }
}

fn service_terminal(outcome: DashTerminalOutcome) -> AgentTerminalOutcome {
    match outcome {
        DashTerminalOutcome::Succeeded => AgentTerminalOutcome::Succeeded,
        DashTerminalOutcome::Failed => AgentTerminalOutcome::Failed,
        DashTerminalOutcome::Interrupted => AgentTerminalOutcome::Interrupted,
        DashTerminalOutcome::Closed => AgentTerminalOutcome::Closed,
        DashTerminalOutcome::Lost => AgentTerminalOutcome::Lost,
    }
}

fn dash_surface_from_bound(
    surface: &agentdash_agent_service_api::BoundAgentSurface,
) -> Result<DashSurface, AgentServiceError> {
    let mut system_prompt = Vec::new();
    let mut tools = Vec::new();
    for contribution in &surface.contributions {
        match &contribution.payload {
            agentdash_agent_service_api::AgentSurfaceContributionPayload::Instruction {
                text,
                ..
            } => system_prompt.push(text.clone()),
            agentdash_agent_service_api::AgentSurfaceContributionPayload::Tool {
                name,
                description,
                input_schema,
                ..
            } => tools.push(DashToolDefinition {
                name: name.to_string(),
                description: description.clone(),
                input_schema: input_schema.clone(),
            }),
            _ => {}
        }
    }
    Ok(DashSurface {
        revision: surface.revision.0,
        digest: surface.digest.to_string(),
        system_prompt: system_prompt.join("\n\n"),
        tools,
    })
}

fn text_input(input: &AgentInput) -> Result<String, AgentServiceError> {
    if input.content.is_empty() {
        return Err(invalid_argument("input must not be empty"));
    }
    let mut text = Vec::new();
    for content in &input.content {
        match content {
            AgentInputContent::Text { text: value } if !value.trim().is_empty() => {
                text.push(value.clone());
            }
            AgentInputContent::Text { .. } => {
                return Err(invalid_argument("text input must not be blank"));
            }
            _ => {
                return Err(AgentServiceError::new(
                    AgentServiceErrorCode::Unsupported,
                    "Dash target lane currently accepts typed text input only",
                    false,
                ));
            }
        }
    }
    Ok(text.join("\n"))
}

fn service_turn_id(
    id: &DashTurnId,
) -> Result<agentdash_agent_service_api::AgentTurnId, AgentServiceError> {
    agentdash_agent_service_api::AgentTurnId::new(id.0.clone()).map_err(internal)
}

fn service_item_id(
    id: &DashItemId,
) -> Result<agentdash_agent_service_api::AgentItemId, AgentServiceError> {
    agentdash_agent_service_api::AgentItemId::new(id.0.clone()).map_err(internal)
}

fn entity_status(status: ActivityStatus) -> AgentEntityStatus {
    match status {
        ActivityStatus::Active => AgentEntityStatus::Running,
        ActivityStatus::Completed => AgentEntityStatus::Completed,
        ActivityStatus::Failed => AgentEntityStatus::Failed,
        ActivityStatus::Lost => AgentEntityStatus::Lost,
        ActivityStatus::Interrupted => AgentEntityStatus::Interrupted,
    }
}

fn turn_snapshot(
    state: &AgentHistoryState,
    turn_id: &DashTurnId,
    failure: Option<&DashExecutionFailure>,
) -> Result<AgentTurnSnapshot, AgentServiceError> {
    let turn = state
        .turns
        .get(turn_id)
        .ok_or_else(|| internal("history fold lost a turn"))?;
    Ok(AgentTurnSnapshot {
        id: service_turn_id(turn_id)?,
        status: entity_status(turn.status),
        items: state
            .items
            .iter()
            .filter(|(_, item)| item.turn_id == *turn_id)
            .map(|(id, item)| item_snapshot(id, item))
            .collect::<Result<Vec<_>, _>>()?,
        error: failure.map(|failure| AgentExecutionFailure {
            code: failure.code.clone(),
            message: failure.message.clone(),
            retryable: Some(failure.retryable),
        }),
    })
}

fn turn_failure<'a>(
    history: &'a AgentHistory,
    turn_id: &DashTurnId,
) -> Option<&'a DashExecutionFailure> {
    history
        .entries()
        .iter()
        .rev()
        .find_map(|entry| match &entry.payload {
            HistoryPayload::TurnFailed {
                turn_id: failed_turn,
                error,
                ..
            } if failed_turn == turn_id => Some(error),
            _ => None,
        })
}

fn item_snapshot(
    item_id: &DashItemId,
    item: &agentdash_agent::dash::ItemState,
) -> Result<AgentItemSnapshot, AgentServiceError> {
    let body = match &item.details {
        ItemDetails::Pending => AgentItemBody::GenericToolActivity {
            name: format!("{:?}", item.kind).to_ascii_lowercase(),
            arguments: serde_json::json!({}),
            result: None,
            progress: Vec::new(),
        },
        ItemDetails::AssistantMessage { content } => AgentItemBody::AgentMessage {
            content: vec![agentdash_agent_service_api::AgentContentBlock::Text {
                text: content.clone(),
            }],
            phase: None,
        },
        ItemDetails::ToolCall { name, arguments } => AgentItemBody::GenericToolActivity {
            name: name.clone(),
            arguments: serde_json::from_str(arguments)
                .unwrap_or_else(|_| serde_json::Value::String(arguments.clone())),
            result: None,
            progress: Vec::new(),
        },
        ItemDetails::ToolResult {
            name,
            content,
            is_error,
        } => AgentItemBody::GenericToolActivity {
            name: name.clone().unwrap_or_else(|| "unknown".into()),
            arguments: serde_json::json!({}),
            result: Some(serde_json::json!({"content": content, "is_error": is_error})),
            progress: Vec::new(),
        },
        ItemDetails::Interaction { prompt } => AgentItemBody::GenericToolActivity {
            name: "user_input".into(),
            arguments: serde_json::json!({"prompt": prompt}),
            result: None,
            progress: Vec::new(),
        },
        ItemDetails::ContextCompaction => AgentItemBody::ContextCompaction {
            summary: None,
            source_digest: None,
        },
    };
    let status = entity_status(item.status);
    let terminal = terminal_evidence(item.status);
    let presentation = AgentItemPresentation::new(body, None, None, terminal).map_err(internal)?;
    Ok(AgentItemSnapshot {
        id: service_item_id(item_id)?,
        status,
        presentation,
    })
}

fn interaction_snapshot(
    id: &DashInteractionId,
    interaction: &InteractionState,
) -> Result<AgentInteractionSnapshot, AgentServiceError> {
    Ok(AgentInteractionSnapshot {
        id: agentdash_agent_service_api::AgentInteractionId::new(id.0.clone()).map_err(internal)?,
        turn_id: service_turn_id(&interaction.turn_id)?,
        item_id: interaction
            .item_id
            .as_ref()
            .map(service_item_id)
            .transpose()?,
        request: AgentInteractionRequest::UserInput {
            prompt: interaction.prompt.clone(),
            questions: Vec::new(),
        },
        status: if interaction.response.is_some() {
            AgentInteractionStatus::Resolved
        } else {
            AgentInteractionStatus::Pending
        },
        resolution: interaction.response.as_ref().map(|response| {
            AgentInteractionResolution::UserInput {
                answers: serde_json::Value::String(response.clone()),
            }
        }),
    })
}

fn compaction_snapshot(
    id: &CompactionId,
    compaction: &CompactionState,
) -> Result<AgentTurnSnapshot, AgentServiceError> {
    let id = agentdash_agent_service_api::AgentTurnId::new(id.0.clone()).map_err(internal)?;
    let item_id = agentdash_agent_service_api::AgentItemId::new(id.as_str()).map_err(internal)?;
    let status = entity_status(compaction.status);
    let body = AgentItemBody::ContextCompaction {
        summary: compaction.summary.as_ref().map(|summary| {
            vec![agentdash_agent_service_api::AgentContentBlock::Text {
                text: summary.clone(),
            }]
        }),
        source_digest: AgentPayloadDigest::new(format!("sha256:{}", compaction.source_digest)).ok(),
    };
    Ok(AgentTurnSnapshot {
        id,
        status,
        items: vec![AgentItemSnapshot {
            id: item_id,
            status,
            presentation: AgentItemPresentation::new(
                body,
                None,
                None,
                terminal_evidence(compaction.status),
            )
            .map_err(internal)?,
        }],
        error: None,
    })
}

fn terminal_evidence(status: ActivityStatus) -> Option<AgentItemTerminalEvidence> {
    let outcome = match status {
        ActivityStatus::Active => return None,
        ActivityStatus::Completed => AgentTerminalStatus::Completed,
        ActivityStatus::Failed => AgentTerminalStatus::Failed,
        ActivityStatus::Lost => AgentTerminalStatus::Lost,
        ActivityStatus::Interrupted => AgentTerminalStatus::Interrupted,
    };
    Some(AgentItemTerminalEvidence {
        outcome,
        completed_at_ms: None,
        duration_ms: None,
        process_exit: None,
        error: None,
    })
}

fn change_payload(
    state: &AgentHistoryState,
    payload: &HistoryPayload,
) -> Result<AgentChangePayload, AgentServiceError> {
    match payload {
        HistoryPayload::TurnStarted { turn_id }
        | HistoryPayload::TurnCompleted { turn_id }
        | HistoryPayload::TurnInterrupted { turn_id } => Ok(AgentChangePayload::TurnChanged {
            turn: turn_snapshot(state, turn_id, None)?,
        }),
        HistoryPayload::TurnFailed { turn_id, error, .. } => Ok(AgentChangePayload::TurnChanged {
            turn: turn_snapshot(state, turn_id, Some(error))?,
        }),
        HistoryPayload::AgentOutput {
            turn_id,
            item_id: None,
            ..
        } => Ok(AgentChangePayload::TurnChanged {
            turn: turn_snapshot(state, turn_id, None)?,
        }),
        HistoryPayload::ItemStarted {
            turn_id, item_id, ..
        } => item_transition_change(state, turn_id, item_id, |item| {
            AgentItemTransition::Started {
                presentation: item.presentation,
            }
        }),
        HistoryPayload::AgentOutput {
            turn_id,
            item_id: Some(item_id),
            ..
        }
        | HistoryPayload::ToolCall {
            turn_id, item_id, ..
        }
        | HistoryPayload::ToolResult {
            turn_id, item_id, ..
        } => item_transition_change(state, turn_id, item_id, |item| {
            AgentItemTransition::Updated {
                update: AgentItemUpdate::BodyReplaced {
                    body: item.presentation.body.clone(),
                },
                presentation: item.presentation,
            }
        }),
        HistoryPayload::ItemCompleted {
            turn_id, item_id, ..
        } => item_transition_change(state, turn_id, item_id, |item| {
            AgentItemTransition::Terminal {
                presentation: item.presentation,
            }
        }),
        HistoryPayload::InteractionRequested { interaction_id, .. }
        | HistoryPayload::InteractionResolved { interaction_id, .. } => {
            Ok(AgentChangePayload::InteractionChanged {
                interaction: interaction_snapshot(
                    interaction_id,
                    state
                        .interactions
                        .get(interaction_id)
                        .ok_or_else(|| internal("history fold lost an interaction"))?,
                )?,
            })
        }
        HistoryPayload::CompactionStarted { compaction_id, .. } => {
            Ok(AgentChangePayload::TurnChanged {
                turn: compaction_snapshot(
                    compaction_id,
                    state
                        .compactions
                        .get(compaction_id)
                        .ok_or_else(|| internal("history fold lost a compaction"))?,
                )?,
            })
        }
        HistoryPayload::CompactionApplied { compaction_id, .. } => {
            let turn = compaction_snapshot(
                compaction_id,
                state
                    .compactions
                    .get(compaction_id)
                    .ok_or_else(|| internal("history fold lost a compaction"))?,
            )?;
            let item = turn.items.into_iter().next().ok_or_else(|| {
                internal("compaction snapshot did not contain its canonical item")
            })?;
            Ok(AgentChangePayload::ItemTransitioned {
                turn_id: turn.id,
                item_id: item.id,
                transition: AgentItemTransition::Updated {
                    update: AgentItemUpdate::BodyReplaced {
                        body: item.presentation.body.clone(),
                    },
                    presentation: item.presentation,
                },
            })
        }
        HistoryPayload::CompactionCompleted { compaction_id }
        | HistoryPayload::CompactionFailed { compaction_id, .. } => {
            let turn = compaction_snapshot(
                compaction_id,
                state
                    .compactions
                    .get(compaction_id)
                    .ok_or_else(|| internal("history fold lost a compaction"))?,
            )?;
            let item = turn.items.into_iter().next().ok_or_else(|| {
                internal("compaction snapshot did not contain its canonical item")
            })?;
            Ok(AgentChangePayload::ItemTransitioned {
                turn_id: turn.id,
                item_id: item.id,
                transition: AgentItemTransition::Terminal {
                    presentation: item.presentation,
                },
            })
        }
        HistoryPayload::Closed => Ok(AgentChangePayload::LifecycleChanged {
            status: AgentLifecycleStatus::Closed,
        }),
        HistoryPayload::InitialContextInstalled { .. } | HistoryPayload::InputAccepted { .. } => {
            Ok(AgentChangePayload::SnapshotInvalidated {
                reason: "dash_history_context_changed".into(),
            })
        }
    }
}

fn item_transition_change(
    state: &AgentHistoryState,
    turn_id: &DashTurnId,
    item_id: &DashItemId,
    transition: impl FnOnce(AgentItemSnapshot) -> AgentItemTransition,
) -> Result<AgentChangePayload, AgentServiceError> {
    let item = item_snapshot(
        item_id,
        state
            .items
            .get(item_id)
            .ok_or_else(|| internal("history fold lost an item"))?,
    )?;
    Ok(AgentChangePayload::ItemTransitioned {
        turn_id: service_turn_id(turn_id)?,
        item_id: item.id.clone(),
        transition: transition(item),
    })
}

fn surface_contribution_supported(
    profile: &AgentSurfaceProfile,
    contribution: &BoundAgentSurfaceContribution,
) -> bool {
    contribution
        .semantics
        .matches_payload(&contribution.payload)
        && contribution
            .semantics
            .required_causal_route()
            .is_none_or(|route| route == contribution.route)
        && profile.facets.iter().any(|facet| {
            facet.routes.contains(&contribution.route)
                && facet.fidelity.satisfies(contribution.fidelity)
                && facet.semantics.satisfies(&contribution.semantics)
        })
}

fn applied_surface_matches_bound(applied: &AppliedAgentSurface, bound: &BoundAgentSurface) -> bool {
    applied.revision == bound.revision
        && applied.digest == bound.digest
        && applied.contributions.len() == bound.contributions.len()
        && bound.contributions.iter().all(|expected| {
            applied.contributions.iter().any(|actual| {
                actual.key == expected.key
                    && actual.route == expected.route
                    && actual.fidelity == expected.fidelity
                    && actual.semantics == expected.semantics
                    && actual.payload_digest == expected.payload_digest
                    && actual.status == AppliedContributionStatus::Applied
            })
        })
}

fn command_effect_record(
    kind: DashCompleteCommandEffectKind,
    request_fingerprint: String,
    receipt: AgentCommandReceipt,
    terminal: Option<AgentTerminalOutcome>,
) -> DashCompleteEffectRecord {
    let applied_receipt = AppliedAgentCommandReceipt {
        command_id: receipt.command_id.clone(),
        effect_id: receipt.effect_id.clone(),
        source: receipt.source.clone(),
        terminal,
        snapshot_revision: receipt.snapshot_revision,
        initial_context: receipt.initial_context.clone(),
    };
    DashCompleteEffectRecord {
        request_fingerprint,
        inspection: AgentEffectInspection {
            effect_id: receipt.effect_id.clone(),
            command_id: Some(receipt.command_id.clone()),
            state: match terminal {
                Some(_) => AgentEffectInspectionState::Applied {
                    outcome: match kind {
                        DashCompleteCommandEffectKind::Create => {
                            AgentAppliedEffectOutcome::Create {
                                receipt: applied_receipt,
                            }
                        }
                        DashCompleteCommandEffectKind::Resume => {
                            AgentAppliedEffectOutcome::Resume {
                                receipt: applied_receipt,
                            }
                        }
                        DashCompleteCommandEffectKind::Command => {
                            AgentAppliedEffectOutcome::Command {
                                receipt: applied_receipt,
                            }
                        }
                        DashCompleteCommandEffectKind::SurfaceRevoke => {
                            AgentAppliedEffectOutcome::SurfaceRevoke {
                                receipt: applied_receipt,
                            }
                        }
                    },
                },
                None => AgentEffectInspectionState::Accepted {
                    source: receipt.source.clone(),
                },
            },
        },
        receipt: DashCompleteRecordedReceipt::Command(receipt),
    }
}

fn request_fingerprint(request: &impl serde::Serialize) -> Result<String, AgentServiceError> {
    let encoded = serde_json::to_vec(request).map_err(internal)?;
    Ok(format!("sha256:{:x}", Sha256::digest(encoded)))
}

fn parse_cursor(cursor: &AgentSourceCursor) -> Result<DashChangeCursor, AgentServiceError> {
    let (revision, ordinal) = cursor
        .as_str()
        .split_once(':')
        .ok_or_else(|| invalid_argument("Dash Agent change cursor is invalid"))?;
    Ok(DashChangeCursor::new(
        revision
            .parse()
            .map_err(|_| invalid_argument("Dash Agent change cursor revision is invalid"))?,
        ordinal
            .parse()
            .map_err(|_| invalid_argument("Dash Agent change cursor ordinal is invalid"))?,
    ))
}

fn dash_change_payload(change: &DashAgentChange) -> Result<AgentChangePayload, AgentServiceError> {
    match &change.payload {
        DashAgentChangePayload::HistoryEntry { entry } => {
            change_payload(&change.state, &entry.payload)
        }
        DashAgentChangePayload::ActiveTurnChanged { active_turn_id } => {
            Ok(AgentChangePayload::ActiveTurnChanged {
                active_turn_id: active_turn_id.as_ref().map(service_turn_id).transpose()?,
            })
        }
    }
}

fn immutable_surface_facet(semantics: AgentSurfaceSemanticFacet) -> AgentSurfaceCapabilityFacet {
    AgentSurfaceCapabilityFacet {
        semantics,
        routes: BTreeSet::from([AgentSurfaceRoute::ImmutableDelivery]),
        fidelity: SemanticFidelity::Exact,
        configuration_boundary: AgentConfigurationBoundary::Create,
    }
}

fn invalid_argument(error: impl std::fmt::Display) -> AgentServiceError {
    AgentServiceError::new(
        AgentServiceErrorCode::InvalidArgument,
        error.to_string(),
        false,
    )
}

fn conflict(error: impl std::fmt::Display) -> AgentServiceError {
    AgentServiceError::new(AgentServiceErrorCode::Conflict, error.to_string(), false)
}

fn not_found(error: impl std::fmt::Display) -> AgentServiceError {
    AgentServiceError::new(AgentServiceErrorCode::NotFound, error.to_string(), false)
}

fn internal(error: impl std::fmt::Display) -> AgentServiceError {
    AgentServiceError::new(AgentServiceErrorCode::Internal, error.to_string(), false)
}

fn map_dash_error(error: DashServiceError) -> AgentServiceError {
    let retryable = error.retryable();
    let code = match &error {
        DashServiceError::InvalidArgument { .. } => AgentServiceErrorCode::InvalidArgument,
        DashServiceError::InvalidState { .. } | DashServiceError::Conflict { .. } => {
            AgentServiceErrorCode::Conflict
        }
        DashServiceError::Unavailable { .. } => AgentServiceErrorCode::Unavailable,
        DashServiceError::Lost { .. } => AgentServiceErrorCode::ProtocolViolation,
        DashServiceError::Internal { .. }
        | DashServiceError::Store(_)
        | DashServiceError::History(_)
        | DashServiceError::Core(_) => AgentServiceErrorCode::Internal,
    };
    AgentServiceError::new(code, error.to_string(), retryable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_agent::dash::{
        AgentItemId, AgentTurnId, HistoryContribution, HistoryEntryId, InteractionId, ItemKind,
    };

    #[test]
    fn detailed_snapshot_and_change_are_derived_from_the_same_history_fold() {
        let turn_id = AgentTurnId::new("turn-1");
        let item_id = AgentItemId::new("item-1");
        let interaction_id = InteractionId::new("interaction-1");
        let mut history =
            AgentHistory::empty(AgentSessionId::new("session-1"), BranchId::new("branch-1"));
        let payloads = [
            HistoryPayload::TurnStarted {
                turn_id: turn_id.clone(),
            },
            HistoryPayload::ItemStarted {
                turn_id: turn_id.clone(),
                item_id: item_id.clone(),
                kind: ItemKind::AssistantMessage,
            },
            HistoryPayload::AgentOutput {
                turn_id: turn_id.clone(),
                item_id: Some(item_id.clone()),
                content: "answer".into(),
            },
            HistoryPayload::ItemCompleted {
                turn_id: turn_id.clone(),
                item_id: item_id.clone(),
            },
            HistoryPayload::InteractionRequested {
                turn_id: turn_id.clone(),
                item_id: None,
                interaction_id: interaction_id.clone(),
                prompt: "continue?".into(),
            },
            HistoryPayload::InteractionResolved {
                interaction_id: interaction_id.clone(),
                response: "yes".into(),
            },
            HistoryPayload::TurnCompleted {
                turn_id: turn_id.clone(),
            },
        ];
        for (index, payload) in payloads.iter().cloned().enumerate() {
            history
                .append(HistoryContribution {
                    entry_id: HistoryEntryId::new(format!("entry-{index}")),
                    payload,
                })
                .unwrap();
        }
        let state = history.state().unwrap();
        let snapshot = turn_snapshot(&state, &turn_id, None).unwrap();
        assert_eq!(snapshot.status, AgentEntityStatus::Completed);
        assert!(matches!(
            snapshot.items[0].presentation.body,
            AgentItemBody::AgentMessage { .. }
        ));
        assert_eq!(
            interaction_snapshot(
                &interaction_id,
                state.interactions.get(&interaction_id).unwrap()
            )
            .unwrap()
            .status,
            AgentInteractionStatus::Resolved
        );

        let canonical_item_id = service_item_id(&item_id).unwrap();
        let mut projected_item = None;
        for sequence in 2..=4 {
            let event_state = history.state_at(sequence).unwrap();
            let payload = &history.entries()[(sequence - 1) as usize].payload;
            let AgentChangePayload::ItemTransitioned {
                item_id,
                transition,
                ..
            } = change_payload(&event_state, payload).unwrap()
            else {
                panic!("item history must project as a typed item transition");
            };
            projected_item = Some(
                AgentItemSnapshot::from_transition(item_id, projected_item.as_ref(), transition)
                    .unwrap(),
            );
        }
        assert_eq!(projected_item.as_ref(), Some(&snapshot.items[0]));
        assert_eq!(projected_item.unwrap().id, canonical_item_id);
    }
}
