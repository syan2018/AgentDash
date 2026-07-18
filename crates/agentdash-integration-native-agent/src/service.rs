use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use agentdash_agent::dash::{
    ActivityStatus, AgentHistory, AgentHistoryState, AgentItemId as DashItemId, AgentSessionId,
    AgentTurnId as DashTurnId, BranchId, CommandId, CompactionId, CompactionMode, CompactionState,
    ContextDeliveryFidelity, DashAgentChange, DashAgentChangePayload, DashAgentService,
    DashChangeCursor, DashCommandRequest, DashCompactionRequest, DashCompactionResult,
    DashCompactor, DashCoreError, DashCoreEvent, DashExecutionCallbacks, DashExecutionDependencies,
    DashProvider, DashProviderEventStream, DashProviderRequest, DashPublicCommand,
    DashReceiptState, DashServiceError, DashSurface, DashTerminalOutcome, DashToolCall,
    DashToolCallbacks, DashToolDefinition, DashToolResult, ForkCutoff, HistoryPayload,
    InitialContextContribution, InitialContextInstallation, InitialContextMode,
    InteractionId as DashInteractionId, InteractionState, ItemDetails,
};
use agentdash_agent_service_api::{
    AgentCapabilityProfile, AgentChange, AgentChangePage, AgentChangePayload, AgentChangesQuery,
    AgentCommand, AgentCommandCapability, AgentCommandEnvelope, AgentCommandReceipt,
    AgentCompactionMode, AgentConfigurationBoundary, AgentEffectIdentity, AgentEffectInspection,
    AgentEffectInspectionState, AgentEntityStatus, AgentForkCapability, AgentForkCutoffKind,
    AgentForkPoint, AgentHookBlockingSemantics, AgentHookMutationKind, AgentHookPoint,
    AgentHookSemanticFacet, AgentHookTiming, AgentInput, AgentInputContent, AgentInteractionKind,
    AgentInteractionSnapshot, AgentItemContent, AgentItemSnapshot, AgentLifecycleCapability,
    AgentLifecycleStatus, AgentPayloadDigest, AgentProfileDigest, AgentReadQuery,
    AgentReceiptState, AgentServiceDefinitionId, AgentServiceDescriptor, AgentServiceError,
    AgentServiceErrorCode, AgentSnapshot, AgentSnapshotAuthority, AgentSnapshotRevision,
    AgentSnapshotSource, AgentSourceChangeLevel, AgentSourceCoordinate, AgentSourceCursor,
    AgentSourceRevision, AgentSurfaceCapabilityFacet, AgentSurfaceProfile, AgentSurfaceRoute,
    AgentSurfaceSemanticFacet, AgentTerminalOutcome, AgentToolDelivery, AgentToolName,
    AgentToolSemanticFacet, AgentToolUpdateSemantics, AgentTurnSnapshot, AppliedAgentSurface,
    AppliedAgentSurfaceContribution, AppliedAgentSurfaceReceipt, AppliedContributionStatus,
    AppliedInitialContextEvidence, ApplyBoundAgentSurface, BoundAgentSurfaceContribution,
    CompleteAgentService, CreateAgentCommand, ForkAgentCommand, ForkAgentReceipt,
    InitialAgentContextPackage, InitialContextAppliedEvidence, InitialContextContributionKind,
    InitialContextDeliveryFidelity, InitialContextProfile, ResumeAgentCommand,
    RevokeBoundAgentSurface, SemanticFidelity,
};
use async_trait::async_trait;
use sha2::{Digest, Sha256};
use tokio::sync::RwLock;

#[derive(Clone)]
struct DashSource {
    service: DashAgentService,
    applied_surface: Option<AppliedAgentSurface>,
    dash_surface: Option<DashSurface>,
    initial_context: Option<AppliedInitialContextEvidence>,
}

#[derive(Debug, Clone)]
struct RecordedEffect {
    inspection: AgentEffectInspection,
    receipt: RecordedReceipt,
}

#[derive(Debug, Clone)]
enum RecordedReceipt {
    Command(AgentCommandReceipt),
    Fork(ForkAgentReceipt),
}

#[derive(Default)]
struct DashServiceState {
    sources: BTreeMap<AgentSourceCoordinate, DashSource>,
    effects: BTreeMap<AgentEffectIdentity, RecordedEffect>,
}

/// Complete Agent target lane backed by Dash Agent history.
///
/// S2/S3 tests construct this service directly. Production registration remains on the legacy
/// driver until the S5 activation set switches every caller and repository together.
pub struct DashAgentCompleteService {
    state: RwLock<DashServiceState>,
    execution: DashExecutionDependencies,
}

impl Default for DashAgentCompleteService {
    fn default() -> Self {
        Self::new()
    }
}

impl DashAgentCompleteService {
    pub fn new() -> Self {
        Self::with_execution(DashExecutionDependencies {
            provider: Arc::new(UnavailableDashProvider),
            tools: Arc::new(NoDashTools),
            callbacks: Arc::new(NoDashExecutionCallbacks),
            compactor: Arc::new(NativeDashCompactor),
        })
    }

    pub fn with_execution(execution: DashExecutionDependencies) -> Self {
        Self {
            state: RwLock::new(DashServiceState::default()),
            execution,
        }
    }

    fn descriptor() -> AgentServiceDescriptor {
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
                        (AgentForkCutoffKind::Item, SemanticFidelity::Exact),
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
}

struct UnavailableDashProvider;

#[async_trait]
impl DashProvider for UnavailableDashProvider {
    async fn stream(
        &self,
        _: DashProviderRequest,
    ) -> Result<DashProviderEventStream, DashCoreError> {
        Err(DashCoreError::Provider {
            message: "Dash Agent provider is not configured".into(),
            retryable: true,
        })
    }
}

struct NoDashTools;

#[async_trait]
impl DashToolCallbacks for NoDashTools {
    async fn invoke(
        &self,
        _: &DashTurnId,
        _: DashToolCall,
    ) -> Result<DashToolResult, DashCoreError> {
        Err(DashCoreError::Tool {
            message: "Dash Agent tool callback is not configured".into(),
            retryable: true,
        })
    }
}

struct NoDashExecutionCallbacks;

#[async_trait]
impl DashExecutionCallbacks for NoDashExecutionCallbacks {
    async fn emit(&self, _: DashCoreEvent) -> Result<(), DashCoreError> {
        Ok(())
    }
}

struct NativeDashCompactor;

#[async_trait]
impl DashCompactor for NativeDashCompactor {
    async fn compact(
        &self,
        request: DashCompactionRequest,
    ) -> Result<DashCompactionResult, DashServiceError> {
        Ok(DashCompactionResult {
            revision: agentdash_agent::dash::ContextRevision::new(format!(
                "context:{}",
                request.source_digest
            )),
            summary: format!(
                "Dash Agent compacted {} history entries",
                request.history.entries().len()
            ),
            retained_from: request
                .history
                .entries()
                .last()
                .map(|entry| entry.entry_id.clone()),
        })
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
        let mut state = self.state.write().await;
        let source = Self::source_for_create(&command);
        if let Some(recorded) = state.effects.get(&command.meta.effect_id) {
            return recorded.command_receipt_for(&source, &command.meta.command_id);
        }
        if state.sources.contains_key(&source) {
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
        let service = DashAgentService::create(history, installation, self.execution.clone())
            .map_err(map_dash_error)?;
        let revision = AgentSnapshotRevision(
            service
                .read()
                .await
                .map_err(map_dash_error)?
                .state
                .entry_count,
        );

        state.sources.insert(
            source.clone(),
            DashSource {
                service,
                applied_surface: None,
                dash_surface: None,
                initial_context: initial_evidence.clone(),
            },
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
        state.effects.insert(
            command.meta.effect_id.clone(),
            RecordedEffect {
                inspection: AgentEffectInspection {
                    effect_id: command.meta.effect_id,
                    command_id: Some(command.meta.command_id),
                    state: AgentEffectInspectionState::Applied {
                        source,
                        terminal: Some(AgentTerminalOutcome::Succeeded),
                        initial_context: initial_evidence,
                        child_source: None,
                    },
                },
                receipt: RecordedReceipt::Command(receipt.clone()),
            },
        );
        Ok(receipt)
    }

    async fn resume(
        &self,
        command: ResumeAgentCommand,
    ) -> Result<AgentCommandReceipt, AgentServiceError> {
        let mut state = self.state.write().await;
        if let Some(recorded) = state.effects.get(&command.meta.effect_id) {
            return recorded.command_receipt_for(&command.source, &command.meta.command_id);
        }
        let source = state
            .sources
            .get(&command.source)
            .ok_or_else(|| not_found("Dash Agent source does not exist"))?;
        let revision = source
            .service
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
            initial_context: source.initial_context.clone(),
        };
        record_command_effect(
            &mut state,
            command.meta.effect_id,
            command.meta.command_id,
            receipt.clone(),
            Some(AgentTerminalOutcome::Succeeded),
        );
        Ok(receipt)
    }

    async fn fork(&self, command: ForkAgentCommand) -> Result<ForkAgentReceipt, AgentServiceError> {
        let mut state = self.state.write().await;
        if let Some(recorded) = state.effects.get(&command.meta.effect_id) {
            return recorded.fork_receipt_for(&command.source, &command.meta.command_id);
        }
        let parent = state
            .sources
            .get(&command.source)
            .ok_or_else(|| not_found("Dash Agent parent source does not exist"))?
            .clone();
        let child_source = command.requested_child_source.clone().unwrap_or_else(|| {
            AgentSourceCoordinate::new(format!("dash:fork:{}", command.meta.effect_id))
                .expect("effect identity produces a source coordinate")
        });
        if state.sources.contains_key(&child_source) {
            return Err(conflict("requested Dash Agent child source already exists"));
        }
        let child_service = parent
            .service
            .fork(
                AgentSessionId::new(child_source.as_str()),
                BranchId::new(format!("{}:fork", child_source.as_str())),
                translate_fork_cutoff(&command.cutoff)?,
            )
            .await
            .map_err(map_dash_error)?;
        let child_history = child_service.history().await.map_err(map_dash_error)?;
        let child_digest = AgentPayloadDigest::new(format!("sha256:{}", child_history.digest()))
            .map_err(internal)?;
        state.sources.insert(
            child_source.clone(),
            DashSource {
                service: child_service,
                applied_surface: parent.applied_surface,
                dash_surface: parent.dash_surface,
                initial_context: parent.initial_context,
            },
        );
        let receipt = ForkAgentReceipt {
            command_id: command.meta.command_id.clone(),
            effect_id: command.meta.effect_id.clone(),
            parent_source: command.source.clone(),
            child_source: Some(child_source.clone()),
            cutoff: command.cutoff,
            child_history_digest: Some(child_digest),
            state: AgentReceiptState::Terminal {
                outcome: AgentTerminalOutcome::Succeeded,
            },
        };
        state.effects.insert(
            command.meta.effect_id.clone(),
            RecordedEffect {
                inspection: AgentEffectInspection {
                    effect_id: command.meta.effect_id,
                    command_id: Some(command.meta.command_id),
                    state: AgentEffectInspectionState::Applied {
                        source: command.source,
                        terminal: Some(AgentTerminalOutcome::Succeeded),
                        initial_context: None,
                        child_source: Some(child_source),
                    },
                },
                receipt: RecordedReceipt::Fork(receipt.clone()),
            },
        );
        Ok(receipt)
    }

    async fn execute(
        &self,
        command: AgentCommandEnvelope,
    ) -> Result<AgentCommandReceipt, AgentServiceError> {
        let service = {
            let state = self.state.read().await;
            if let Some(recorded) = state.effects.get(&command.meta.effect_id) {
                return recorded.command_receipt_for(&command.source, &command.meta.command_id);
            }
            state
                .sources
                .get(&command.source)
                .ok_or_else(|| not_found("Dash Agent source does not exist"))?
                .service
                .clone()
        };
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
        let mut state = self.state.write().await;
        record_command_effect(
            &mut state,
            command.meta.effect_id,
            command.meta.command_id,
            receipt.clone(),
            terminal,
        );
        Ok(receipt)
    }

    async fn read(&self, query: AgentReadQuery) -> Result<AgentSnapshot, AgentServiceError> {
        let source = {
            let state = self.state.read().await;
            state
                .sources
                .get(&query.source)
                .ok_or_else(|| not_found("Dash Agent source does not exist"))?
                .clone()
        };
        let read = source.service.read().await.map_err(map_dash_error)?;
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
                .map(|turn_id| turn_snapshot(&history_state, turn_id))
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
            source_info: AgentSnapshotSource {
                authority: AgentSnapshotAuthority::AgentAuthoritative,
                source_revision: Some(
                    AgentSourceRevision::new(format!("history:{}", read.history_digest))
                        .map_err(internal)?,
                ),
                fidelity: SemanticFidelity::Exact,
                observed_at_ms: 0,
            },
            applied_surface: source.applied_surface.clone(),
            initial_context: source.initial_context.clone(),
        })
    }

    async fn changes(
        &self,
        query: AgentChangesQuery,
    ) -> Result<AgentChangePage, AgentServiceError> {
        let service = {
            let state = self.state.read().await;
            state
                .sources
                .get(&query.source)
                .ok_or_else(|| not_found("Dash Agent source does not exist"))?
                .service
                .clone()
        };
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
                Ok(AgentChange {
                    cursor: AgentSourceCursor::new(change.cursor.encode()).map_err(internal)?,
                    source_revision: Some(
                        AgentSourceRevision::new(format!("history:{}", change.source_digest))
                            .map_err(internal)?,
                    ),
                    occurred_at_ms: 0,
                    payload: dash_change_payload(&change)?,
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
        let sources = {
            let state = self.state.read().await;
            if let Some(record) = state.effects.get(&identity) {
                return Ok(record.inspection.clone());
            }
            state
                .sources
                .iter()
                .map(|(source, record)| (source.clone(), record.service.clone()))
                .collect::<Vec<_>>()
        };
        let dash_effect_id = agentdash_agent::dash::EffectId::new(identity.as_str());
        for (source, service) in sources {
            let Some(inspection) = service
                .inspect(&dash_effect_id)
                .await
                .map_err(map_dash_error)?
            else {
                continue;
            };
            return Ok(AgentEffectInspection {
                effect_id: identity,
                command_id: Some(
                    agentdash_agent_service_api::AgentCommandId::new(inspection.command_id.0)
                        .map_err(internal)?,
                ),
                state: match inspection.state {
                    DashReceiptState::Accepted => AgentEffectInspectionState::Accepted { source },
                    DashReceiptState::Terminal(terminal) => AgentEffectInspectionState::Applied {
                        source,
                        terminal: Some(service_terminal(terminal)),
                        initial_context: None,
                        child_source: None,
                    },
                    DashReceiptState::Unknown => AgentEffectInspectionState::Unknown,
                },
            });
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
        let mut state = self.state.write().await;
        let source = state
            .sources
            .get_mut(&command.source)
            .ok_or_else(|| not_found("Dash Agent source does not exist"))?;
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
        source
            .service
            .apply_surface(dash_surface.clone())
            .await
            .map_err(map_dash_error)?;
        source.applied_surface = Some(applied.clone());
        source.dash_surface = Some(dash_surface);
        Ok(AppliedAgentSurfaceReceipt {
            command_id: command.command_id,
            effect_id: command.effect_id,
            source: command.source,
            applied,
        })
    }

    async fn revoke_surface(
        &self,
        command: RevokeBoundAgentSurface,
    ) -> Result<AgentCommandReceipt, AgentServiceError> {
        let mut state = self.state.write().await;
        let source = state
            .sources
            .get_mut(&command.source)
            .ok_or_else(|| not_found("Dash Agent source does not exist"))?;
        if source
            .applied_surface
            .as_ref()
            .is_some_and(|applied| applied.revision != command.expected_revision)
        {
            return Err(conflict("surface revision does not match"));
        }
        source
            .service
            .revoke_surface(command.expected_revision.0)
            .await
            .map_err(map_dash_error)?;
        let revision = source
            .service
            .read()
            .await
            .map_err(map_dash_error)?
            .state
            .entry_count;
        source.applied_surface = None;
        source.dash_surface = None;
        Ok(AgentCommandReceipt {
            command_id: command.command_id,
            effect_id: command.effect_id,
            source: command.source,
            state: AgentReceiptState::Terminal {
                outcome: AgentTerminalOutcome::Succeeded,
            },
            snapshot_revision: Some(AgentSnapshotRevision(revision)),
            initial_context: None,
        })
    }
}

impl RecordedEffect {
    fn command_receipt_for(
        &self,
        source: &AgentSourceCoordinate,
        command_id: &agentdash_agent_service_api::AgentCommandId,
    ) -> Result<AgentCommandReceipt, AgentServiceError> {
        match &self.receipt {
            RecordedReceipt::Command(receipt)
                if &receipt.source == source && &receipt.command_id == command_id =>
            {
                Ok(receipt.clone())
            }
            RecordedReceipt::Command(_) => Err(conflict(
                "effect identity was reused by another command or source",
            )),
            RecordedReceipt::Fork(_) => Err(conflict("effect identity belongs to a fork command")),
        }
    }

    fn fork_receipt_for(
        &self,
        source: &AgentSourceCoordinate,
        command_id: &agentdash_agent_service_api::AgentCommandId,
    ) -> Result<ForkAgentReceipt, AgentServiceError> {
        match &self.receipt {
            RecordedReceipt::Fork(receipt)
                if &receipt.parent_source == source && &receipt.command_id == command_id =>
            {
                Ok(receipt.clone())
            }
            RecordedReceipt::Fork(_) => Err(conflict(
                "effect identity was reused by another command or source",
            )),
            RecordedReceipt::Command(_) => {
                Err(conflict("effect identity belongs to a non-fork command"))
            }
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
        AgentForkPoint::Item { item_id } => Ok(ForkCutoff::CompletedItem {
            item_id: agentdash_agent::dash::AgentItemId::new(item_id.as_str()),
        }),
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
    })
}

fn item_snapshot(
    item_id: &DashItemId,
    item: &agentdash_agent::dash::ItemState,
) -> Result<AgentItemSnapshot, AgentServiceError> {
    let content = match &item.details {
        ItemDetails::Pending => AgentItemContent::Extension {
            namespace: "dash.history".into(),
            schema: "pending_item_v1".into(),
            value: serde_json::json!({"kind": item.kind}),
        },
        ItemDetails::AssistantMessage { content } => AgentItemContent::AgentOutput {
            content: vec![AgentInputContent::Text {
                text: content.clone(),
            }],
        },
        ItemDetails::ToolCall { name, arguments } => AgentItemContent::ToolCall {
            name: AgentToolName::new(name.clone()).map_err(internal)?,
            arguments: serde_json::from_str(arguments)
                .unwrap_or_else(|_| serde_json::Value::String(arguments.clone())),
        },
        ItemDetails::ToolResult {
            name,
            content,
            is_error,
        } => AgentItemContent::ToolResult {
            name: AgentToolName::new(name.clone().unwrap_or_else(|| "unknown".into()))
                .map_err(internal)?,
            result: serde_json::json!({"content": content, "is_error": is_error}),
        },
        ItemDetails::Interaction { prompt } => AgentItemContent::Extension {
            namespace: "dash.interaction".into(),
            schema: "interaction_item_v1".into(),
            value: serde_json::json!({"prompt": prompt}),
        },
        ItemDetails::ContextCompaction => AgentItemContent::ContextCompaction,
    };
    let canonical = serde_json::to_vec(&content).map_err(internal)?;
    Ok(AgentItemSnapshot {
        id: service_item_id(item_id)?,
        status: entity_status(item.status),
        content,
        content_digest: AgentPayloadDigest::new(format!("sha256:{:x}", Sha256::digest(canonical)))
            .map_err(internal)?,
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
        kind: AgentInteractionKind::UserInput,
        prompt: interaction.prompt.clone(),
        resolved: interaction.response.is_some(),
    })
}

fn compaction_snapshot(
    id: &CompactionId,
    compaction: &CompactionState,
) -> Result<AgentTurnSnapshot, AgentServiceError> {
    let id = agentdash_agent_service_api::AgentTurnId::new(id.0.clone()).map_err(internal)?;
    let item_id = agentdash_agent_service_api::AgentItemId::new(id.as_str()).map_err(internal)?;
    Ok(AgentTurnSnapshot {
        id,
        status: entity_status(compaction.status),
        items: vec![AgentItemSnapshot {
            id: item_id,
            status: entity_status(compaction.status),
            content: AgentItemContent::ContextCompaction,
            content_digest: AgentPayloadDigest::new(format!("sha256:{}", compaction.source_digest))
                .map_err(internal)?,
        }],
    })
}

fn change_payload(
    state: &AgentHistoryState,
    payload: &HistoryPayload,
) -> Result<AgentChangePayload, AgentServiceError> {
    match payload {
        HistoryPayload::TurnStarted { turn_id }
        | HistoryPayload::AgentOutput { turn_id, .. }
        | HistoryPayload::TurnCompleted { turn_id }
        | HistoryPayload::TurnFailed { turn_id, .. }
        | HistoryPayload::TurnInterrupted { turn_id } => Ok(AgentChangePayload::TurnChanged {
            turn: turn_snapshot(state, turn_id)?,
        }),
        HistoryPayload::ItemStarted {
            turn_id, item_id, ..
        }
        | HistoryPayload::ItemCompleted {
            turn_id, item_id, ..
        }
        | HistoryPayload::ToolCall {
            turn_id, item_id, ..
        }
        | HistoryPayload::ToolResult {
            turn_id, item_id, ..
        } => Ok(AgentChangePayload::ItemChanged {
            turn_id: service_turn_id(turn_id)?,
            item: item_snapshot(
                item_id,
                state
                    .items
                    .get(item_id)
                    .ok_or_else(|| internal("history fold lost an item"))?,
            )?,
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
        HistoryPayload::CompactionStarted { compaction_id, .. }
        | HistoryPayload::CompactionApplied { compaction_id, .. }
        | HistoryPayload::CompactionCompleted { compaction_id }
        | HistoryPayload::CompactionFailed { compaction_id, .. } => {
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

fn record_command_effect(
    state: &mut DashServiceState,
    effect_id: AgentEffectIdentity,
    command_id: agentdash_agent_service_api::AgentCommandId,
    receipt: AgentCommandReceipt,
    terminal: Option<AgentTerminalOutcome>,
) {
    state.effects.insert(
        effect_id.clone(),
        RecordedEffect {
            inspection: AgentEffectInspection {
                effect_id,
                command_id: Some(command_id),
                state: match terminal {
                    Some(terminal) => AgentEffectInspectionState::Applied {
                        source: receipt.source.clone(),
                        terminal: Some(terminal),
                        initial_context: receipt.initial_context.clone(),
                        child_source: None,
                    },
                    None => AgentEffectInspectionState::Accepted {
                        source: receipt.source.clone(),
                    },
                },
            },
            receipt: RecordedReceipt::Command(receipt),
        },
    );
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
        let snapshot = turn_snapshot(&state, &turn_id).unwrap();
        assert_eq!(snapshot.status, AgentEntityStatus::Completed);
        assert!(matches!(
            snapshot.items[0].content,
            AgentItemContent::AgentOutput { .. }
        ));
        assert!(
            interaction_snapshot(
                &interaction_id,
                state.interactions.get(&interaction_id).unwrap()
            )
            .unwrap()
            .resolved
        );
        assert!(matches!(
            change_payload(&state, &payloads[2]).unwrap(),
            AgentChangePayload::TurnChanged { .. }
        ));
    }
}
