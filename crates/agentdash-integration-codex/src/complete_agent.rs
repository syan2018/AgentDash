use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::Arc,
};

use agentdash_agent_service_api::{
    AgentAppliedEffectOutcome, AgentCapabilityProfile, AgentChange, AgentChangePage,
    AgentChangePayload, AgentChangesQuery, AgentCommand, AgentCommandCapability,
    AgentCommandEnvelope, AgentCommandReceipt, AgentCompactionMode, AgentConfigurationBoundary,
    AgentEffectIdentity, AgentEffectInspection, AgentEffectInspectionState, AgentEntityStatus,
    AgentForkCapability, AgentForkCutoffKind, AgentForkPoint, AgentInput, AgentInputContent,
    AgentInteractionId, AgentInteractionKind, AgentInteractionSnapshot, AgentItemContent,
    AgentItemId, AgentItemSnapshot, AgentLifecycleCapability, AgentLifecycleStatus,
    AgentPayloadDigest, AgentProfileDigest, AgentReadQuery, AgentReceiptState,
    AgentServiceDefinitionId, AgentServiceDescriptor, AgentServiceError, AgentServiceErrorCode,
    AgentSnapshot, AgentSnapshotAuthority, AgentSnapshotRevision, AgentSnapshotSource,
    AgentSourceChangeLevel, AgentSourceCoordinate, AgentSourceCursor, AgentSurfaceCapabilityFacet,
    AgentSurfaceContributionPayload, AgentSurfaceProfile, AgentSurfaceRoute,
    AgentSurfaceSemanticFacet, AgentTerminalOutcome, AgentTurnId, AgentTurnSnapshot,
    AppliedAgentCommandReceipt, AppliedAgentSurface, AppliedAgentSurfaceContribution,
    AppliedAgentSurfaceReceipt, AppliedContributionStatus, AppliedForkAgentReceipt,
    AppliedInitialContextEvidence, ApplyBoundAgentSurface, CompleteAgentService,
    CreateAgentCommand, ForkAgentCommand, ForkAgentReceipt, InitialAgentContextPackage,
    InitialContextAppliedEvidence, InitialContextContributionKind, InitialContextDeliveryFidelity,
    InitialContextProfile, ResumeAgentCommand, RevokeBoundAgentSurface, SemanticFidelity,
};
use async_trait::async_trait;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use tokio::sync::RwLock;

pub const CODEX_INITIAL_CONTEXT_RENDERER_VERSION: &str = "agentdash.codex.initial-context.v1";
/// Canonical digest contract for the source-authoritative `thread/read(includeTurns)` fork view.
///
/// The digest proves the AgentDash-observable Codex lineage at the exact native fork cutoff. It
/// does not claim ownership of, or a vendor signature over, Codex's private ThreadStore.
pub const CODEX_CHILD_HISTORY_DIGEST_VERSION: &str =
    "agentdash.codex.child-history.thread-turns.v1";
pub const CODEX_APP_SERVER_PROTOCOL_REVISION: u32 = 144;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexCompleteAgentTransportError {
    pub message: String,
    pub retryable: bool,
    /// The request may have reached Codex. Retrying with a new effect identity is unsafe.
    pub outcome_unknown: bool,
}

impl CodexCompleteAgentTransportError {
    pub fn unavailable(message: impl Into<String>, outcome_unknown: bool) -> Self {
        Self {
            message: message.into(),
            retryable: true,
            outcome_unknown,
        }
    }

    pub fn protocol(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            retryable: false,
            outcome_unknown: false,
        }
    }
}

/// Adapter-owned typed observation before it is mapped into the AgentDash service vocabulary.
///
/// Vendor JSON terminates at this trait. Other crates only receive `AgentChange`/`AgentSnapshot`.
#[derive(Debug, Clone, PartialEq)]
pub enum CodexAppServerObservation {
    Notification {
        sequence: u64,
        method: String,
        params: Value,
    },
    ServerRequest {
        sequence: u64,
        request_id: Value,
        method: String,
        params: Value,
    },
}

impl CodexAppServerObservation {
    pub(crate) fn sequence(&self) -> u64 {
        match self {
            Self::Notification { sequence, .. } | Self::ServerRequest { sequence, .. } => *sequence,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CodexAppServerObservationPage {
    pub observations: Vec<CodexAppServerObservation>,
    pub next_sequence: Option<u64>,
    /// App Server notifications are live-process observations, not a durable source tail.
    pub gap: bool,
}

#[async_trait]
pub trait CodexAppServerTransport: Send + Sync {
    async fn request(
        &self,
        method: &str,
        params: Value,
    ) -> Result<Value, CodexCompleteAgentTransportError>;

    async fn respond(
        &self,
        request_id: Value,
        result: Value,
    ) -> Result<(), CodexCompleteAgentTransportError>;

    async fn notify(
        &self,
        method: &str,
        params: Option<Value>,
    ) -> Result<(), CodexCompleteAgentTransportError>;

    async fn observations(
        &self,
        source_thread_id: &str,
        after_sequence: Option<u64>,
        limit: u32,
    ) -> Result<CodexAppServerObservationPage, CodexCompleteAgentTransportError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexCompleteAgentConfig {
    pub definition_id: AgentServiceDefinitionId,
    pub title: String,
    pub cwd: PathBuf,
    pub model: Option<String>,
    pub model_provider: Option<String>,
    pub base_instructions: Option<String>,
    pub developer_instructions: Option<String>,
    pub runtime_workspace_roots: Vec<PathBuf>,
}

impl CodexCompleteAgentConfig {
    pub fn validate(&self) -> Result<(), AgentServiceError> {
        if !self.cwd.is_absolute()
            || self
                .runtime_workspace_roots
                .iter()
                .any(|root| !root.is_absolute())
        {
            return Err(service_error(
                AgentServiceErrorCode::InvalidArgument,
                "Codex cwd and runtime workspace roots must be absolute",
                false,
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct CodexSourceState {
    revision: u64,
    lifecycle: AgentLifecycleStatus,
    applied_surface: Option<AppliedAgentSurface>,
    initial_context: Option<AppliedInitialContextEvidence>,
    pending_interactions: BTreeMap<AgentInteractionId, PendingInteraction>,
}

#[derive(Debug, Clone)]
struct PendingInteraction {
    request_id: Value,
    interaction: AgentInteractionSnapshot,
}

#[derive(Debug, Clone)]
enum RecordedReceipt {
    Command {
        family: CommandEffectFamily,
        receipt: AgentCommandReceipt,
    },
    Fork {
        receipt: ForkAgentReceipt,
        applied: AppliedForkAgentReceipt,
    },
    SurfaceApply(AppliedAgentSurfaceReceipt),
    Unknown(AgentEffectInspection),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommandEffectFamily {
    Create,
    Resume,
    Command,
    SurfaceRevoke,
}

#[derive(Default)]
struct CodexCompleteAgentState {
    sources: BTreeMap<AgentSourceCoordinate, CodexSourceState>,
    effects: BTreeMap<AgentEffectIdentity, RecordedReceipt>,
}

/// Complete Agent service for Codex App Server.
///
/// Codex ThreadStore remains the history/resume/fork/compaction authority. This adapter keeps only
/// command idempotency evidence, applied surface evidence, and live interaction correlation.
pub struct CodexCompleteAgentService {
    config: CodexCompleteAgentConfig,
    transport: Arc<dyn CodexAppServerTransport>,
    state: RwLock<CodexCompleteAgentState>,
}

impl CodexCompleteAgentService {
    pub(crate) fn new(
        config: CodexCompleteAgentConfig,
        transport: Arc<dyn CodexAppServerTransport>,
    ) -> Result<Self, AgentServiceError> {
        config.validate()?;
        Ok(Self {
            config,
            transport,
            state: RwLock::new(CodexCompleteAgentState::default()),
        })
    }

    fn descriptor(&self) -> AgentServiceDescriptor {
        let immutable = |semantics| AgentSurfaceCapabilityFacet {
            semantics,
            routes: BTreeSet::from([AgentSurfaceRoute::ImmutableDelivery]),
            fidelity: SemanticFidelity::Exact,
            configuration_boundary: AgentConfigurationBoundary::Binding,
        };
        AgentServiceDescriptor {
            definition_id: self.config.definition_id.clone(),
            title: self.config.title.clone(),
            protocol_revision: CODEX_APP_SERVER_PROTOCOL_REVISION,
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
                source_changes: AgentSourceChangeLevel::OrderedLiveStream,
                initial_context: InitialContextProfile {
                    contribution_fidelity: BTreeMap::from([
                        (
                            InitialContextContributionKind::CompactSummary,
                            InitialContextDeliveryFidelity::CanonicalRendered,
                        ),
                        (
                            InitialContextContributionKind::WorkflowContext,
                            InitialContextDeliveryFidelity::CanonicalRendered,
                        ),
                        (
                            InitialContextContributionKind::ConstraintSet,
                            InitialContextDeliveryFidelity::CanonicalRendered,
                        ),
                    ]),
                    applied_evidence: InitialContextAppliedEvidence::PackageAndMaterializedDigest,
                    renderer_versions: BTreeSet::from([
                        CODEX_INITIAL_CONTEXT_RENDERER_VERSION.to_owned()
                    ]),
                },
                // App Server 0.144.1 can reapply instructions/workspace configuration through
                // thread/resume. Dynamic tools and arbitrary live Hook mutation are intentionally
                // absent because the source protocol cannot prove them on resume/fork.
                surface: AgentSurfaceProfile {
                    facets: vec![
                        immutable(AgentSurfaceSemanticFacet::Instruction),
                        immutable(AgentSurfaceSemanticFacet::Workspace),
                        immutable(AgentSurfaceSemanticFacet::ContextRequirement),
                    ],
                },
                // The adapter can inspect its stable local ledger, but App Server has no
                // source-native effect-id lookup across adapter loss.
                inspect_effects: SemanticFidelity::Observed,
            },
            profile_digest: AgentProfileDigest::new(format!(
                "codex-complete-agent-profile-v{}",
                CODEX_APP_SERVER_PROTOCOL_REVISION
            ))
            .expect("static profile digest"),
            configuration_boundary: AgentConfigurationBoundary::Binding,
        }
    }

    async fn replay_command(
        &self,
        effect_id: &AgentEffectIdentity,
        expected_family: CommandEffectFamily,
    ) -> Result<Option<AgentCommandReceipt>, AgentServiceError> {
        let state = self.state.read().await;
        match state.effects.get(effect_id) {
            Some(RecordedReceipt::Command { family, receipt }) if *family == expected_family => {
                Ok(Some(receipt.clone()))
            }
            Some(RecordedReceipt::Unknown(_)) => Err(unknown_outcome_error()),
            Some(_) => Err(service_error(
                AgentServiceErrorCode::Conflict,
                "effect identity was already used for another Codex command family",
                false,
            )),
            None => Ok(None),
        }
    }

    async fn mark_unknown(
        &self,
        effect_id: AgentEffectIdentity,
        command_id: agentdash_agent_service_api::AgentCommandId,
    ) {
        self.state.write().await.effects.insert(
            effect_id.clone(),
            RecordedReceipt::Unknown(AgentEffectInspection {
                effect_id,
                command_id: Some(command_id),
                state: AgentEffectInspectionState::Unknown,
            }),
        );
    }

    async fn settle_post_dispatch_error(
        &self,
        effect_id: &AgentEffectIdentity,
        command_id: &agentdash_agent_service_api::AgentCommandId,
        error: AgentServiceError,
    ) -> AgentServiceError {
        self.mark_unknown(effect_id.clone(), command_id.clone())
            .await;
        error
    }

    fn base_thread_params(&self) -> Map<String, Value> {
        let mut params = Map::new();
        params.insert(
            "cwd".to_owned(),
            Value::String(self.config.cwd.display().to_string()),
        );
        if let Some(model) = &self.config.model {
            params.insert("model".to_owned(), Value::String(model.clone()));
        }
        if let Some(provider) = &self.config.model_provider {
            params.insert("modelProvider".to_owned(), Value::String(provider.clone()));
        }
        if let Some(instructions) = &self.config.base_instructions {
            params.insert(
                "baseInstructions".to_owned(),
                Value::String(instructions.clone()),
            );
        }
        if let Some(instructions) = &self.config.developer_instructions {
            params.insert(
                "developerInstructions".to_owned(),
                Value::String(instructions.clone()),
            );
        }
        params.insert(
            "runtimeWorkspaceRoots".to_owned(),
            Value::Array(
                self.config
                    .runtime_workspace_roots
                    .iter()
                    .map(|root| Value::String(root.display().to_string()))
                    .collect(),
            ),
        );
        params
    }

    fn params_with_initial_context(
        &self,
        package: Option<&InitialAgentContextPackage>,
    ) -> Result<(Value, Option<AppliedInitialContextEvidence>), AgentServiceError> {
        let mut params = self.base_thread_params();
        let Some(package) = package else {
            return Ok((Value::Object(params), None));
        };
        if !package.digest_matches() {
            return Err(service_error(
                AgentServiceErrorCode::InvalidArgument,
                "initial context package digest does not match its typed contributions",
                false,
            ));
        }
        let rendered = render_initial_context(package)?;
        let instructions = match params.remove("developerInstructions") {
            Some(Value::String(existing)) if !existing.is_empty() => {
                format!("{existing}\n\n{rendered}")
            }
            _ => rendered.clone(),
        };
        params.insert(
            "developerInstructions".to_owned(),
            Value::String(instructions),
        );
        let digest =
            AgentPayloadDigest::new(format!("sha256:{:x}", Sha256::digest(rendered.as_bytes())))
                .map_err(internal_error)?;
        Ok((
            Value::Object(params),
            Some(AppliedInitialContextEvidence {
                package_id: package.package_id.clone(),
                package_digest: package.digest.clone(),
                fidelity: InitialContextDeliveryFidelity::CanonicalRendered,
                renderer_version: Some(CODEX_INITIAL_CONTEXT_RENDERER_VERSION.to_owned()),
                materialized_digest: Some(digest),
            }),
        ))
    }

    async fn request_effect(
        &self,
        method: &str,
        params: Value,
        effect_id: &AgentEffectIdentity,
        command_id: &agentdash_agent_service_api::AgentCommandId,
    ) -> Result<Value, AgentServiceError> {
        match self.transport.request(method, params).await {
            Ok(value) => Ok(value),
            Err(error) => {
                if error.outcome_unknown {
                    self.mark_unknown(effect_id.clone(), command_id.clone())
                        .await;
                }
                Err(map_transport_error(&error))
            }
        }
    }

    async fn next_revision(&self, source: &AgentSourceCoordinate) -> AgentSnapshotRevision {
        let mut state = self.state.write().await;
        let source = state
            .sources
            .entry(source.clone())
            .or_insert(CodexSourceState {
                revision: 0,
                lifecycle: AgentLifecycleStatus::Active,
                applied_surface: None,
                initial_context: None,
                pending_interactions: BTreeMap::new(),
            });
        source.revision += 1;
        AgentSnapshotRevision(source.revision)
    }
}

#[async_trait]
impl CompleteAgentService for CodexCompleteAgentService {
    async fn describe(&self) -> Result<AgentServiceDescriptor, AgentServiceError> {
        Ok(self.descriptor())
    }

    async fn create(
        &self,
        command: CreateAgentCommand,
    ) -> Result<AgentCommandReceipt, AgentServiceError> {
        if let Some(receipt) = self
            .replay_command(&command.meta.effect_id, CommandEffectFamily::Create)
            .await?
        {
            return Ok(receipt);
        }
        let (params, initial_context) =
            self.params_with_initial_context(command.initial_context.as_ref())?;
        let result = self
            .request_effect(
                "thread/start",
                params,
                &command.meta.effect_id,
                &command.meta.command_id,
            )
            .await?;
        let source = match response_thread_source(&result) {
            Ok(source) => source,
            Err(error) => {
                return Err(self
                    .settle_post_dispatch_error(
                        &command.meta.effect_id,
                        &command.meta.command_id,
                        error,
                    )
                    .await);
            }
        };
        if let Some(requested) = &command.requested_source
            && requested != &source
        {
            return Err(self
                .settle_post_dispatch_error(
                    &command.meta.effect_id,
                    &command.meta.command_id,
                    service_error(
                        AgentServiceErrorCode::Conflict,
                        "Codex generated a source coordinate different from the requested coordinate",
                        false,
                    ),
                )
                .await);
        }
        let receipt = AgentCommandReceipt {
            command_id: command.meta.command_id.clone(),
            effect_id: command.meta.effect_id.clone(),
            source: source.clone(),
            state: AgentReceiptState::Terminal {
                outcome: AgentTerminalOutcome::Succeeded,
            },
            snapshot_revision: Some(AgentSnapshotRevision(1)),
            initial_context: initial_context.clone(),
        };
        let mut state = self.state.write().await;
        state.sources.insert(
            source.clone(),
            CodexSourceState {
                revision: 1,
                lifecycle: AgentLifecycleStatus::Active,
                applied_surface: None,
                initial_context: initial_context.clone(),
                pending_interactions: BTreeMap::new(),
            },
        );
        state.effects.insert(
            command.meta.effect_id.clone(),
            RecordedReceipt::Command {
                family: CommandEffectFamily::Create,
                receipt: receipt.clone(),
            },
        );
        Ok(receipt)
    }

    async fn resume(
        &self,
        command: ResumeAgentCommand,
    ) -> Result<AgentCommandReceipt, AgentServiceError> {
        if let Some(receipt) = self
            .replay_command(&command.meta.effect_id, CommandEffectFamily::Resume)
            .await?
        {
            return Ok(receipt);
        }
        let mut params = self.base_thread_params();
        params.insert(
            "threadId".to_owned(),
            Value::String(command.source.as_str().to_owned()),
        );
        let result = self
            .request_effect(
                "thread/resume",
                Value::Object(params),
                &command.meta.effect_id,
                &command.meta.command_id,
            )
            .await?;
        let source = match response_thread_source(&result) {
            Ok(source) => source,
            Err(error) => {
                return Err(self
                    .settle_post_dispatch_error(
                        &command.meta.effect_id,
                        &command.meta.command_id,
                        error,
                    )
                    .await);
            }
        };
        if source != command.source {
            return Err(self
                .settle_post_dispatch_error(
                    &command.meta.effect_id,
                    &command.meta.command_id,
                    protocol_violation("thread/resume returned a different source thread"),
                )
                .await);
        }
        let revision = self.next_revision(&source).await;
        let initial_context = self
            .state
            .read()
            .await
            .sources
            .get(&source)
            .and_then(|state| state.initial_context.clone());
        let receipt = successful_command_receipt(
            &command.meta.command_id,
            &command.meta.effect_id,
            source.clone(),
            revision,
            initial_context,
        );
        self.state.write().await.effects.insert(
            command.meta.effect_id,
            RecordedReceipt::Command {
                family: CommandEffectFamily::Resume,
                receipt: receipt.clone(),
            },
        );
        Ok(receipt)
    }

    async fn fork(&self, command: ForkAgentCommand) -> Result<ForkAgentReceipt, AgentServiceError> {
        {
            let state = self.state.read().await;
            match state.effects.get(&command.meta.effect_id) {
                Some(RecordedReceipt::Fork { receipt, .. }) => return Ok(receipt.clone()),
                Some(RecordedReceipt::Unknown(_)) => return Err(unknown_outcome_error()),
                Some(_) => {
                    return Err(service_error(
                        AgentServiceErrorCode::Conflict,
                        "effect identity was already used for another Codex command family",
                        false,
                    ));
                }
                None => {}
            }
        }
        let mut params = self.base_thread_params();
        params.insert(
            "threadId".to_owned(),
            Value::String(command.source.as_str().to_owned()),
        );
        match &command.cutoff {
            AgentForkPoint::Head => {}
            AgentForkPoint::CompletedTurn { turn_id } => {
                params.insert(
                    "lastTurnId".to_owned(),
                    Value::String(turn_id.as_str().to_owned()),
                );
            }
            AgentForkPoint::Item { .. } | AgentForkPoint::SourceCursor { .. } => {
                return Err(service_error(
                    AgentServiceErrorCode::Unsupported,
                    "Codex App Server only proves head or completed-turn fork cutoffs",
                    false,
                ));
            }
        }
        let result = self
            .request_effect(
                "thread/fork",
                Value::Object(params),
                &command.meta.effect_id,
                &command.meta.command_id,
            )
            .await?;
        let child = match response_thread_source(&result) {
            Ok(child) => child,
            Err(error) => {
                return Err(self
                    .settle_post_dispatch_error(
                        &command.meta.effect_id,
                        &command.meta.command_id,
                        error,
                    )
                    .await);
            }
        };
        if let Some(requested) = &command.requested_child_source
            && requested != &child
        {
            return Err(self
                .settle_post_dispatch_error(
                    &command.meta.effect_id,
                    &command.meta.command_id,
                    service_error(
                        AgentServiceErrorCode::Conflict,
                        "Codex generated a child source different from the requested coordinate",
                        false,
                    ),
                )
                .await);
        }
        // `thread/read` verifies that the returned child is materialized in Codex ThreadStore.
        let verified = match self
            .transport
            .request(
                "thread/read",
                json!({"threadId": child.as_str(), "includeTurns": true}),
            )
            .await
        {
            Ok(verified) => verified,
            Err(error) => {
                self.mark_unknown(
                    command.meta.effect_id.clone(),
                    command.meta.command_id.clone(),
                )
                .await;
                return Err(map_transport_error(&error));
            }
        };
        let verified_child = match response_thread_source(&verified) {
            Ok(source) => source,
            Err(error) => {
                return Err(self
                    .settle_post_dispatch_error(
                        &command.meta.effect_id,
                        &command.meta.command_id,
                        error,
                    )
                    .await);
            }
        };
        if verified_child != child {
            return Err(self
                .settle_post_dispatch_error(
                    &command.meta.effect_id,
                    &command.meta.command_id,
                    protocol_violation("thread/read verified a different fork child"),
                )
                .await);
        }
        if let Err(error) = verify_codex_fork_cutoff(&verified, &command.cutoff) {
            return Err(self
                .settle_post_dispatch_error(
                    &command.meta.effect_id,
                    &command.meta.command_id,
                    error,
                )
                .await);
        }
        let child_history_digest = match codex_thread_history_digest(&verified) {
            Ok(digest) => digest,
            Err(error) => {
                return Err(self
                    .settle_post_dispatch_error(
                        &command.meta.effect_id,
                        &command.meta.command_id,
                        error,
                    )
                    .await);
            }
        };
        let parent_state = self
            .state
            .read()
            .await
            .sources
            .get(&command.source)
            .cloned();
        self.state.write().await.sources.insert(
            child.clone(),
            CodexSourceState {
                revision: 1,
                lifecycle: AgentLifecycleStatus::Active,
                applied_surface: parent_state
                    .as_ref()
                    .and_then(|state| state.applied_surface.clone()),
                initial_context: parent_state
                    .as_ref()
                    .and_then(|state| state.initial_context.clone()),
                pending_interactions: BTreeMap::new(),
            },
        );
        let receipt = ForkAgentReceipt {
            command_id: command.meta.command_id.clone(),
            effect_id: command.meta.effect_id.clone(),
            parent_source: command.source,
            child_source: Some(child.clone()),
            cutoff: command.cutoff.clone(),
            child_history_digest: Some(child_history_digest.clone()),
            state: AgentReceiptState::Terminal {
                outcome: AgentTerminalOutcome::Succeeded,
            },
        };
        let applied = AppliedForkAgentReceipt {
            command_id: receipt.command_id.clone(),
            effect_id: receipt.effect_id.clone(),
            parent_source: receipt.parent_source.clone(),
            child_source: child,
            cutoff: command.cutoff,
            child_history_digest,
            terminal: Some(AgentTerminalOutcome::Succeeded),
        };
        self.state.write().await.effects.insert(
            command.meta.effect_id,
            RecordedReceipt::Fork {
                receipt: receipt.clone(),
                applied,
            },
        );
        Ok(receipt)
    }

    async fn execute(
        &self,
        command: AgentCommandEnvelope,
    ) -> Result<AgentCommandReceipt, AgentServiceError> {
        if let Some(receipt) = self
            .replay_command(&command.meta.effect_id, CommandEffectFamily::Command)
            .await?
        {
            return Ok(receipt);
        }
        let method;
        let params;
        let terminal;
        match &command.command {
            AgentCommand::SubmitInput { input } => {
                method = "turn/start";
                let (input, additional) = codex_input(input)?;
                params = json!({
                    "threadId": command.source.as_str(),
                    "input": input,
                    "additionalContext": additional,
                });
                terminal = None;
            }
            AgentCommand::Steer {
                expected_turn_id,
                input,
            } => {
                method = "turn/steer";
                let (input, additional) = codex_input(input)?;
                params = json!({
                    "threadId": command.source.as_str(),
                    "expectedTurnId": expected_turn_id.as_str(),
                    "input": input,
                    "additionalContext": additional,
                });
                terminal = None;
            }
            AgentCommand::Interrupt { expected_turn_id } => {
                method = "turn/interrupt";
                params = json!({
                    "threadId": command.source.as_str(),
                    "turnId": expected_turn_id.as_str(),
                });
                terminal = None;
            }
            AgentCommand::RequestCompaction => {
                method = "thread/compact/start";
                params = json!({"threadId": command.source.as_str()});
                terminal = None;
            }
            AgentCommand::ResolveInteraction {
                interaction_id,
                response,
            } => {
                let pending = self
                    .state
                    .read()
                    .await
                    .sources
                    .get(&command.source)
                    .and_then(|source| source.pending_interactions.get(interaction_id))
                    .cloned()
                    .ok_or_else(|| {
                        service_error(
                            AgentServiceErrorCode::NotFound,
                            "Codex interaction is not pending",
                            false,
                        )
                    })?;
                let result = interaction_result(pending.interaction.kind, response)?;
                if let Err(error) = self.transport.respond(pending.request_id, result).await {
                    if error.outcome_unknown {
                        self.mark_unknown(
                            command.meta.effect_id.clone(),
                            command.meta.command_id.clone(),
                        )
                        .await;
                    }
                    return Err(map_transport_error(&error));
                }
                if let Some(source) = self.state.write().await.sources.get_mut(&command.source) {
                    source.pending_interactions.remove(interaction_id);
                    source.revision += 1;
                }
                let revision = self
                    .state
                    .read()
                    .await
                    .sources
                    .get(&command.source)
                    .map_or(AgentSnapshotRevision(0), |source| {
                        AgentSnapshotRevision(source.revision)
                    });
                let receipt = successful_command_receipt(
                    &command.meta.command_id,
                    &command.meta.effect_id,
                    command.source.clone(),
                    revision,
                    None,
                );
                self.state.write().await.effects.insert(
                    command.meta.effect_id,
                    RecordedReceipt::Command {
                        family: CommandEffectFamily::Command,
                        receipt: receipt.clone(),
                    },
                );
                return Ok(receipt);
            }
            AgentCommand::Close => {
                method = "thread/archive";
                params = json!({"threadId": command.source.as_str()});
                terminal = Some(AgentTerminalOutcome::Closed);
            }
        }
        self.request_effect(
            method,
            params,
            &command.meta.effect_id,
            &command.meta.command_id,
        )
        .await?;
        let revision = self.next_revision(&command.source).await;
        if terminal == Some(AgentTerminalOutcome::Closed)
            && let Some(source) = self.state.write().await.sources.get_mut(&command.source)
        {
            source.lifecycle = AgentLifecycleStatus::Closed;
        }
        let receipt = AgentCommandReceipt {
            command_id: command.meta.command_id.clone(),
            effect_id: command.meta.effect_id.clone(),
            source: command.source,
            state: terminal.map_or(AgentReceiptState::Accepted, |outcome| {
                AgentReceiptState::Terminal { outcome }
            }),
            snapshot_revision: Some(revision),
            initial_context: None,
        };
        self.state.write().await.effects.insert(
            command.meta.effect_id,
            RecordedReceipt::Command {
                family: CommandEffectFamily::Command,
                receipt: receipt.clone(),
            },
        );
        Ok(receipt)
    }

    async fn read(&self, query: AgentReadQuery) -> Result<AgentSnapshot, AgentServiceError> {
        if query.at_revision.is_some() {
            return Err(service_error(
                AgentServiceErrorCode::Unsupported,
                "Codex App Server cannot read a stable historical snapshot revision",
                false,
            ));
        }
        let result = self
            .transport
            .request(
                "thread/read",
                json!({"threadId": query.source.as_str(), "includeTurns": true}),
            )
            .await
            .map_err(|error| map_transport_error(&error))?;
        let returned_source = response_thread_source(&result)?;
        if returned_source != query.source {
            return Err(protocol_violation(
                "thread/read returned a different source thread",
            ));
        }
        let (turns, active_turn_id) = map_thread_turns(&result)?;
        let mut state = self.state.write().await;
        let source = state
            .sources
            .entry(query.source.clone())
            .or_insert(CodexSourceState {
                revision: 0,
                lifecycle: AgentLifecycleStatus::Active,
                applied_surface: None,
                initial_context: None,
                pending_interactions: BTreeMap::new(),
            });
        source.revision += 1;
        let interactions = source
            .pending_interactions
            .values()
            .map(|pending| pending.interaction.clone())
            .collect();
        Ok(AgentSnapshot {
            source: query.source,
            revision: AgentSnapshotRevision(source.revision),
            lifecycle: source.lifecycle,
            active_turn_id,
            turns,
            interactions,
            source_info: AgentSnapshotSource {
                authority: AgentSnapshotAuthority::AgentObserved,
                // App Server does not expose a stable durable snapshot/context revision.
                source_revision: None,
                fidelity: SemanticFidelity::Observed,
                observed_at_ms: now_ms(),
            },
            applied_surface: source.applied_surface.clone(),
            initial_context: source.initial_context.clone(),
        })
    }

    async fn changes(
        &self,
        query: AgentChangesQuery,
    ) -> Result<AgentChangePage, AgentServiceError> {
        let after = query.after.as_ref().map(parse_cursor).transpose()?;
        let page = self
            .transport
            .observations(query.source.as_str(), after, query.limit)
            .await
            .map_err(|error| map_transport_error(&error))?;
        if page.gap {
            return Ok(AgentChangePage {
                source: query.source,
                changes: Vec::new(),
                next: page.next_sequence.map(source_cursor).transpose()?,
                gap: true,
            });
        }
        let mut changes = Vec::with_capacity(page.observations.len());
        for observation in page.observations {
            let cursor = source_cursor(observation.sequence())?;
            let payload = self.map_observation(&query.source, observation).await?;
            changes.push(AgentChange {
                cursor,
                source_revision: None,
                occurred_at_ms: now_ms(),
                payload,
            });
        }
        Ok(AgentChangePage {
            source: query.source,
            changes,
            next: page.next_sequence.map(source_cursor).transpose()?,
            gap: false,
        })
    }

    async fn inspect(
        &self,
        identity: AgentEffectIdentity,
    ) -> Result<AgentEffectInspection, AgentServiceError> {
        let state = self.state.read().await;
        Ok(match state.effects.get(&identity) {
            Some(RecordedReceipt::Command { family, receipt }) => {
                inspection_from_command(receipt, *family)
            }
            Some(RecordedReceipt::Fork { receipt, applied }) => AgentEffectInspection {
                effect_id: receipt.effect_id.clone(),
                command_id: Some(receipt.command_id.clone()),
                state: AgentEffectInspectionState::Applied {
                    outcome: AgentAppliedEffectOutcome::Fork {
                        receipt: applied.clone(),
                    },
                },
            },
            Some(RecordedReceipt::SurfaceApply(receipt)) => AgentEffectInspection {
                effect_id: receipt.effect_id.clone(),
                command_id: Some(receipt.command_id.clone()),
                state: AgentEffectInspectionState::Applied {
                    outcome: AgentAppliedEffectOutcome::SurfaceApply {
                        receipt: receipt.clone(),
                    },
                },
            },
            Some(RecordedReceipt::Unknown(inspection)) => inspection.clone(),
            None => AgentEffectInspection {
                effect_id: identity,
                command_id: None,
                state: AgentEffectInspectionState::NotApplied,
            },
        })
    }

    async fn apply_surface(
        &self,
        command: ApplyBoundAgentSurface,
    ) -> Result<AppliedAgentSurfaceReceipt, AgentServiceError> {
        {
            let state = self.state.read().await;
            match state.effects.get(&command.effect_id) {
                Some(RecordedReceipt::SurfaceApply(receipt)) => return Ok(receipt.clone()),
                Some(RecordedReceipt::Unknown(_)) => return Err(unknown_outcome_error()),
                Some(_) => {
                    return Err(service_error(
                        AgentServiceErrorCode::Conflict,
                        "effect identity was already used for another Codex command family",
                        false,
                    ));
                }
                None => {}
            }
        }
        let descriptor = self.descriptor();
        if command.bound_surface.offer_profile_digest != descriptor.profile_digest {
            return Err(service_error(
                AgentServiceErrorCode::Conflict,
                "bound surface targets another Codex profile",
                false,
            ));
        }
        if !self
            .state
            .read()
            .await
            .sources
            .contains_key(&command.source)
        {
            return Err(not_found("Codex source is not known to this adapter"));
        }
        let mut params = self.base_thread_params();
        params.insert(
            "threadId".to_owned(),
            Value::String(command.source.as_str().to_owned()),
        );
        let mut developer = self
            .config
            .developer_instructions
            .clone()
            .into_iter()
            .collect::<Vec<_>>();
        let mut roots = self
            .config
            .runtime_workspace_roots
            .iter()
            .map(|root| root.display().to_string())
            .collect::<BTreeSet<_>>();
        let mut applied = Vec::with_capacity(command.bound_surface.contributions.len());
        for contribution in &command.bound_surface.contributions {
            if contribution.route != AgentSurfaceRoute::ImmutableDelivery
                || contribution.fidelity != SemanticFidelity::Exact
            {
                return Err(service_error(
                    AgentServiceErrorCode::Unsupported,
                    "Codex accepts only exact immutable surface contributions",
                    false,
                ));
            }
            match (&contribution.semantics, &contribution.payload) {
                (
                    AgentSurfaceSemanticFacet::Instruction,
                    AgentSurfaceContributionPayload::Instruction { text, .. },
                ) => developer.push(text.clone()),
                (
                    AgentSurfaceSemanticFacet::Workspace,
                    AgentSurfaceContributionPayload::Workspace { requirement },
                ) => {
                    let root = PathBuf::from(requirement);
                    if !root.is_absolute() {
                        return Err(service_error(
                            AgentServiceErrorCode::InvalidArgument,
                            "Codex workspace surface requires an absolute root",
                            false,
                        ));
                    }
                    roots.insert(root.display().to_string());
                }
                (
                    AgentSurfaceSemanticFacet::ContextRequirement,
                    AgentSurfaceContributionPayload::ContextRequirement { requirement },
                ) => developer.push(format!("Agent context requirement: {requirement}")),
                _ => {
                    return Err(service_error(
                        AgentServiceErrorCode::Unsupported,
                        "Codex cannot apply this surface semantic through thread/resume",
                        false,
                    ));
                }
            }
            applied.push(AppliedAgentSurfaceContribution {
                key: contribution.key.clone(),
                route: contribution.route,
                fidelity: contribution.fidelity,
                semantics: contribution.semantics.clone(),
                payload_digest: contribution.payload_digest.clone(),
                status: AppliedContributionStatus::Applied,
                evidence: Some("codex thread/resume accepted configuration".to_owned()),
            });
        }
        if !developer.is_empty() {
            params.insert(
                "developerInstructions".to_owned(),
                Value::String(developer.join("\n\n")),
            );
        }
        params.insert(
            "runtimeWorkspaceRoots".to_owned(),
            Value::Array(roots.into_iter().map(Value::String).collect()),
        );
        let result = self
            .request_effect(
                "thread/resume",
                Value::Object(params),
                &command.effect_id,
                &command.command_id,
            )
            .await?;
        let resumed_source = match response_thread_source(&result) {
            Ok(source) => source,
            Err(error) => {
                return Err(self
                    .settle_post_dispatch_error(&command.effect_id, &command.command_id, error)
                    .await);
            }
        };
        if resumed_source != command.source {
            return Err(self
                .settle_post_dispatch_error(
                    &command.effect_id,
                    &command.command_id,
                    protocol_violation("surface application resumed a different Codex thread"),
                )
                .await);
        }
        let applied = AppliedAgentSurface {
            revision: command.bound_surface.revision,
            digest: command.bound_surface.digest,
            contributions: applied,
        };
        let receipt = AppliedAgentSurfaceReceipt {
            command_id: command.command_id,
            effect_id: command.effect_id.clone(),
            source: command.source.clone(),
            applied: applied.clone(),
        };
        let mut state = self.state.write().await;
        let source = state
            .sources
            .get_mut(&command.source)
            .expect("source existence checked before Codex side effect");
        source.revision += 1;
        source.applied_surface = Some(applied);
        state.effects.insert(
            command.effect_id,
            RecordedReceipt::SurfaceApply(receipt.clone()),
        );
        Ok(receipt)
    }

    async fn revoke_surface(
        &self,
        command: RevokeBoundAgentSurface,
    ) -> Result<AgentCommandReceipt, AgentServiceError> {
        if let Some(receipt) = self
            .replay_command(&command.effect_id, CommandEffectFamily::SurfaceRevoke)
            .await?
        {
            return Ok(receipt);
        }
        let current = self
            .state
            .read()
            .await
            .sources
            .get(&command.source)
            .and_then(|source| source.applied_surface.clone())
            .ok_or_else(|| not_found("Codex source has no applied surface"))?;
        if current.revision != command.expected_revision {
            return Err(service_error(
                AgentServiceErrorCode::Conflict,
                "surface revoke expected revision does not match",
                false,
            ));
        }
        let mut params = self.base_thread_params();
        params.insert(
            "threadId".to_owned(),
            Value::String(command.source.as_str().to_owned()),
        );
        let result = self
            .request_effect(
                "thread/resume",
                Value::Object(params),
                &command.effect_id,
                &command.command_id,
            )
            .await?;
        let resumed_source = match response_thread_source(&result) {
            Ok(source) => source,
            Err(error) => {
                return Err(self
                    .settle_post_dispatch_error(&command.effect_id, &command.command_id, error)
                    .await);
            }
        };
        if resumed_source != command.source {
            return Err(self
                .settle_post_dispatch_error(
                    &command.effect_id,
                    &command.command_id,
                    protocol_violation("surface revoke resumed a different Codex thread"),
                )
                .await);
        }
        let revision = self.next_revision(&command.source).await;
        if let Some(source) = self.state.write().await.sources.get_mut(&command.source) {
            source.applied_surface = None;
        }
        let receipt = successful_command_receipt(
            &command.command_id,
            &command.effect_id,
            command.source,
            revision,
            None,
        );
        self.state.write().await.effects.insert(
            command.effect_id,
            RecordedReceipt::Command {
                family: CommandEffectFamily::SurfaceRevoke,
                receipt: receipt.clone(),
            },
        );
        Ok(receipt)
    }
}

impl CodexCompleteAgentService {
    async fn map_observation(
        &self,
        source: &AgentSourceCoordinate,
        observation: CodexAppServerObservation,
    ) -> Result<AgentChangePayload, AgentServiceError> {
        match observation {
            CodexAppServerObservation::ServerRequest {
                request_id,
                method,
                params,
                sequence,
            } => {
                let interaction = map_server_request(sequence, &method, &params)?;
                let pending = PendingInteraction {
                    request_id,
                    interaction: interaction.clone(),
                };
                let mut state = self.state.write().await;
                let source_state =
                    state
                        .sources
                        .entry(source.clone())
                        .or_insert(CodexSourceState {
                            revision: 0,
                            lifecycle: AgentLifecycleStatus::Active,
                            applied_surface: None,
                            initial_context: None,
                            pending_interactions: BTreeMap::new(),
                        });
                source_state
                    .pending_interactions
                    .insert(interaction.id.clone(), pending);
                source_state.revision += 1;
                Ok(AgentChangePayload::InteractionChanged { interaction })
            }
            CodexAppServerObservation::Notification { method, params, .. } => {
                map_notification(&method, &params)
            }
        }
    }
}

fn render_initial_context(
    package: &InitialAgentContextPackage,
) -> Result<String, AgentServiceError> {
    let contributions =
        serde_json::to_string_pretty(&package.contributions).map_err(internal_error)?;
    Ok(format!(
        "[AgentDash immutable initial context]\nrenderer={}\npackage_id={}\npackage_digest={}\nschema_version={}\nmode={:?}\ncontributions={}",
        CODEX_INITIAL_CONTEXT_RENDERER_VERSION,
        package.package_id,
        package.digest,
        package.schema_version.0,
        package.mode,
        contributions
    ))
}

fn codex_input(input: &AgentInput) -> Result<(Vec<Value>, Vec<Value>), AgentServiceError> {
    let mut native = Vec::new();
    let mut additional = Vec::new();
    for content in &input.content {
        match content {
            AgentInputContent::Text { text } => native.push(json!({
                "type": "text",
                "text": text,
                "text_elements": [],
            })),
            AgentInputContent::Image {
                source,
                media_type,
                digest,
            } => native.push(json!({
                "type": "image",
                "url": source,
                "mediaType": media_type,
                "digest": digest.as_str(),
            })),
            AgentInputContent::Resource {
                uri,
                media_type,
                digest,
            } => additional.push(json!({
                "type": "resource",
                "uri": uri,
                "mediaType": media_type,
                "digest": digest.as_ref().map(AgentPayloadDigest::as_str),
            })),
            AgentInputContent::Structured { schema, value } => additional.push(json!({
                "type": "structured",
                "schema": schema,
                "value": value,
            })),
        }
    }
    if native.is_empty() && additional.is_empty() {
        return Err(service_error(
            AgentServiceErrorCode::InvalidArgument,
            "Codex input must contain at least one typed block",
            false,
        ));
    }
    Ok((native, additional))
}

fn response_thread_source(result: &Value) -> Result<AgentSourceCoordinate, AgentServiceError> {
    let thread_id = result
        .pointer("/thread/id")
        .and_then(Value::as_str)
        .ok_or_else(|| protocol_violation("Codex response misses thread.id"))?;
    AgentSourceCoordinate::new(thread_id).map_err(internal_error)
}

fn codex_thread_history_digest(result: &Value) -> Result<AgentPayloadDigest, AgentServiceError> {
    let turns = result
        .pointer("/thread/turns")
        .and_then(Value::as_array)
        .ok_or_else(|| protocol_violation("thread/read response misses thread.turns"))?;
    let canonical = canonical_json(&json!({
        "version": CODEX_CHILD_HISTORY_DIGEST_VERSION,
        "turns": turns,
    }))?;
    AgentPayloadDigest::new(format!("sha256:{:x}", Sha256::digest(canonical)))
        .map_err(internal_error)
}

fn verify_codex_fork_cutoff(
    result: &Value,
    cutoff: &AgentForkPoint,
) -> Result<(), AgentServiceError> {
    match cutoff {
        AgentForkPoint::Head => Ok(()),
        AgentForkPoint::CompletedTurn { turn_id } => {
            let verified_turn_id = result
                .pointer("/thread/turns")
                .and_then(Value::as_array)
                .and_then(|turns| turns.last())
                .and_then(|turn| turn.get("id"))
                .and_then(Value::as_str);
            if verified_turn_id == Some(turn_id.as_str()) {
                Ok(())
            } else {
                Err(protocol_violation(
                    "thread/read child history does not end at the requested fork cutoff",
                ))
            }
        }
        AgentForkPoint::Item { .. } | AgentForkPoint::SourceCursor { .. } => Err(service_error(
            AgentServiceErrorCode::Unsupported,
            "Codex App Server cannot verify this fork cutoff",
            false,
        )),
    }
}

fn canonical_json(value: &Value) -> Result<Vec<u8>, AgentServiceError> {
    fn canonicalize(value: &Value) -> Value {
        match value {
            Value::Object(object) => {
                let mut entries = object.iter().collect::<Vec<_>>();
                entries.sort_by(|left, right| left.0.cmp(right.0));
                let mut canonical = Map::new();
                for (key, value) in entries {
                    canonical.insert(key.clone(), canonicalize(value));
                }
                Value::Object(canonical)
            }
            Value::Array(items) => Value::Array(items.iter().map(canonicalize).collect()),
            other => other.clone(),
        }
    }

    serde_json::to_vec(&canonicalize(value)).map_err(internal_error)
}

fn map_thread_turns(
    result: &Value,
) -> Result<(Vec<AgentTurnSnapshot>, Option<AgentTurnId>), AgentServiceError> {
    let turns = result
        .pointer("/thread/turns")
        .and_then(Value::as_array)
        .ok_or_else(|| protocol_violation("thread/read response misses thread.turns"))?;
    let mut mapped = Vec::with_capacity(turns.len());
    let mut active = None;
    for turn in turns {
        let id = required_id::<AgentTurnId>(turn, "id", AgentTurnId::new)?;
        let status = entity_status(turn.get("status"));
        if matches!(
            status,
            AgentEntityStatus::Accepted | AgentEntityStatus::Running
        ) {
            active = Some(id.clone());
        }
        let items = turn
            .get("items")
            .and_then(Value::as_array)
            .ok_or_else(|| protocol_violation("thread/read turn misses items"))?
            .iter()
            .map(map_item)
            .collect::<Result<Vec<_>, _>>()?;
        mapped.push(AgentTurnSnapshot { id, status, items });
    }
    Ok((mapped, active))
}

fn map_item(item: &Value) -> Result<AgentItemSnapshot, AgentServiceError> {
    let id = required_id::<AgentItemId>(item, "id", AgentItemId::new)?;
    let status = entity_status(item.get("status"));
    let kind = item
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let content = match kind {
        "userMessage" => AgentItemContent::UserInput {
            input: AgentInput {
                content: vec![AgentInputContent::Text {
                    text: item
                        .get("content")
                        .and_then(Value::as_array)
                        .map(|blocks| {
                            blocks
                                .iter()
                                .filter_map(|block| block.get("text").and_then(Value::as_str))
                                .collect::<Vec<_>>()
                                .join("\n")
                        })
                        .unwrap_or_default(),
                }],
            },
        },
        "agentMessage" => AgentItemContent::AgentOutput {
            content: vec![AgentInputContent::Text {
                text: item
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
            }],
        },
        "contextCompaction" => AgentItemContent::ContextCompaction,
        "dynamicToolCall" => AgentItemContent::ToolCall {
            name: agentdash_agent_service_api::AgentToolName::new(
                item.get("tool")
                    .or_else(|| item.get("name"))
                    .and_then(Value::as_str)
                    .unwrap_or("codex.dynamic-tool"),
            )
            .map_err(internal_error)?,
            arguments: item
                .get("arguments")
                .cloned()
                .unwrap_or(Value::Object(Map::new())),
        },
        _ => AgentItemContent::Extension {
            namespace: "codex".to_owned(),
            schema: format!("codex.thread_item.{kind}.v1"),
            value: json!({
                "type": kind,
                "status": item.get("status"),
            }),
        },
    };
    let canonical = serde_json::to_vec(&content).map_err(internal_error)?;
    let content_digest = AgentPayloadDigest::new(format!("sha256:{:x}", Sha256::digest(canonical)))
        .map_err(internal_error)?;
    Ok(AgentItemSnapshot {
        id,
        status,
        content,
        content_digest,
    })
}

fn map_notification(method: &str, params: &Value) -> Result<AgentChangePayload, AgentServiceError> {
    match method {
        "turn/started" | "turn/completed" => {
            let turn = params
                .get("turn")
                .ok_or_else(|| protocol_violation("turn notification misses turn"))?;
            let id = required_id::<AgentTurnId>(turn, "id", AgentTurnId::new)?;
            Ok(AgentChangePayload::TurnChanged {
                turn: AgentTurnSnapshot {
                    id,
                    status: entity_status(turn.get("status")),
                    items: Vec::new(),
                },
            })
        }
        "item/started" | "item/completed" | "item/updated" => {
            let turn_id = required_id::<AgentTurnId>(params, "turnId", AgentTurnId::new)?;
            let item = params
                .get("item")
                .ok_or_else(|| protocol_violation("item notification misses item"))?;
            Ok(AgentChangePayload::ItemChanged {
                turn_id,
                item: map_item(item)?,
            })
        }
        "thread/archived" => Ok(AgentChangePayload::LifecycleChanged {
            status: AgentLifecycleStatus::Closed,
        }),
        _ => Ok(AgentChangePayload::SnapshotInvalidated {
            reason: format!("Codex observation {method} requires thread/read reconciliation"),
        }),
    }
}

fn map_server_request(
    sequence: u64,
    method: &str,
    params: &Value,
) -> Result<AgentInteractionSnapshot, AgentServiceError> {
    let kind = match method {
        "item/commandExecution/requestApproval" | "item/fileChange/requestApproval" => {
            AgentInteractionKind::Approval
        }
        "item/tool/requestUserInput" => AgentInteractionKind::UserInput,
        "mcpServer/elicitation/create" => AgentInteractionKind::McpElicitation,
        "item/tool/call" => AgentInteractionKind::DynamicTool,
        _ => {
            return Err(service_error(
                AgentServiceErrorCode::Unsupported,
                format!("unsupported Codex server request {method}"),
                false,
            ));
        }
    };
    Ok(AgentInteractionSnapshot {
        id: AgentInteractionId::new(
            params
                .get("requestId")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| format!("codex-request-{sequence}")),
        )
        .map_err(internal_error)?,
        turn_id: required_id::<AgentTurnId>(params, "turnId", AgentTurnId::new)?,
        item_id: params
            .get("itemId")
            .and_then(Value::as_str)
            .map(AgentItemId::new)
            .transpose()
            .map_err(internal_error)?,
        kind,
        prompt: params
            .get("prompt")
            .or_else(|| params.get("message"))
            .and_then(Value::as_str)
            .unwrap_or("Codex requires a typed interaction response")
            .to_owned(),
        resolved: false,
    })
}

fn interaction_result(
    kind: AgentInteractionKind,
    response: &agentdash_agent_service_api::AgentInteractionResponse,
) -> Result<Value, AgentServiceError> {
    use agentdash_agent_service_api::AgentInteractionResponse;
    match (kind, response) {
        (AgentInteractionKind::Approval, AgentInteractionResponse::Approved) => {
            Ok(json!({"decision": "accept"}))
        }
        (AgentInteractionKind::Approval, AgentInteractionResponse::Denied { reason }) => {
            Ok(json!({"decision": "decline", "reason": reason}))
        }
        (AgentInteractionKind::UserInput, AgentInteractionResponse::UserInput { input }) => {
            let (input, additional) = codex_input(input)?;
            Ok(json!({"input": input, "additionalContext": additional}))
        }
        (
            AgentInteractionKind::DynamicTool,
            AgentInteractionResponse::DynamicToolResult { result },
        ) => Ok(json!({"result": result})),
        (
            AgentInteractionKind::McpElicitation,
            AgentInteractionResponse::McpElicitation { response },
        ) => Ok(json!({"response": response})),
        _ => Err(service_error(
            AgentServiceErrorCode::InvalidArgument,
            "interaction response kind does not match the pending Codex request",
            false,
        )),
    }
}

fn entity_status(value: Option<&Value>) -> AgentEntityStatus {
    let status = value.and_then(|value| {
        value
            .as_str()
            .or_else(|| value.get("type").and_then(Value::as_str))
    });
    match status {
        Some("accepted" | "pending") => AgentEntityStatus::Accepted,
        Some("inProgress" | "running") => AgentEntityStatus::Running,
        Some("completed" | "succeeded") => AgentEntityStatus::Completed,
        Some("failed") => AgentEntityStatus::Failed,
        Some("interrupted" | "cancelled") => AgentEntityStatus::Interrupted,
        Some("lost") => AgentEntityStatus::Lost,
        // App Server projections are observed rather than authoritative. Missing or future vendor
        // statuses must stay nonterminal until a terminal status is explicitly observed.
        None | Some(_) => AgentEntityStatus::Accepted,
    }
}

fn required_id<T>(
    value: &Value,
    field: &str,
    constructor: impl FnOnce(String) -> Result<T, agentdash_agent_service_api::InvalidAgentServiceId>,
) -> Result<T, AgentServiceError> {
    let value = value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| protocol_violation(format!("Codex payload misses {field}")))?;
    constructor(value.to_owned()).map_err(internal_error)
}

fn source_cursor(sequence: u64) -> Result<AgentSourceCursor, AgentServiceError> {
    AgentSourceCursor::new(format!("codex-live:{sequence}")).map_err(internal_error)
}

fn parse_cursor(cursor: &AgentSourceCursor) -> Result<u64, AgentServiceError> {
    cursor
        .as_str()
        .strip_prefix("codex-live:")
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| {
            service_error(
                AgentServiceErrorCode::InvalidArgument,
                "Codex source cursor is malformed",
                false,
            )
        })
}

fn successful_command_receipt(
    command_id: &agentdash_agent_service_api::AgentCommandId,
    effect_id: &AgentEffectIdentity,
    source: AgentSourceCoordinate,
    revision: AgentSnapshotRevision,
    initial_context: Option<AppliedInitialContextEvidence>,
) -> AgentCommandReceipt {
    AgentCommandReceipt {
        command_id: command_id.clone(),
        effect_id: effect_id.clone(),
        source,
        state: AgentReceiptState::Terminal {
            outcome: AgentTerminalOutcome::Succeeded,
        },
        snapshot_revision: Some(revision),
        initial_context,
    }
}

fn inspection_from_command(
    receipt: &AgentCommandReceipt,
    family: CommandEffectFamily,
) -> AgentEffectInspection {
    AgentEffectInspection {
        effect_id: receipt.effect_id.clone(),
        command_id: Some(receipt.command_id.clone()),
        state: match receipt.state {
            AgentReceiptState::Rejected { .. } => AgentEffectInspectionState::NotApplied,
            AgentReceiptState::Unknown => AgentEffectInspectionState::Unknown,
            _ => AgentEffectInspectionState::Applied {
                outcome: family.applied_outcome(receipt),
            },
        },
    }
}

impl CommandEffectFamily {
    fn applied_outcome(self, receipt: &AgentCommandReceipt) -> AgentAppliedEffectOutcome {
        let receipt = AppliedAgentCommandReceipt {
            command_id: receipt.command_id.clone(),
            effect_id: receipt.effect_id.clone(),
            source: receipt.source.clone(),
            terminal: receipt.state.terminal(),
            snapshot_revision: receipt.snapshot_revision,
            initial_context: receipt.initial_context.clone(),
        };
        match self {
            Self::Create => AgentAppliedEffectOutcome::Create { receipt },
            Self::Resume => AgentAppliedEffectOutcome::Resume { receipt },
            Self::Command => AgentAppliedEffectOutcome::Command { receipt },
            Self::SurfaceRevoke => AgentAppliedEffectOutcome::SurfaceRevoke { receipt },
        }
    }
}

trait ReceiptTerminal {
    fn terminal(&self) -> Option<AgentTerminalOutcome>;
}

impl ReceiptTerminal for AgentReceiptState {
    fn terminal(&self) -> Option<AgentTerminalOutcome> {
        match self {
            Self::Terminal { outcome } => Some(*outcome),
            Self::AlreadyApplied { terminal } => *terminal,
            _ => None,
        }
    }
}

fn map_transport_error(error: &CodexCompleteAgentTransportError) -> AgentServiceError {
    service_error(
        if error.retryable {
            AgentServiceErrorCode::Unavailable
        } else {
            AgentServiceErrorCode::ProtocolViolation
        },
        error.message.clone(),
        error.retryable,
    )
}

fn protocol_violation(message: impl Into<String>) -> AgentServiceError {
    service_error(AgentServiceErrorCode::ProtocolViolation, message, false)
}

fn not_found(message: impl Into<String>) -> AgentServiceError {
    service_error(AgentServiceErrorCode::NotFound, message, false)
}

fn unknown_outcome_error() -> AgentServiceError {
    service_error(
        AgentServiceErrorCode::Unavailable,
        "Codex effect outcome is unknown; inspect the same effect identity",
        true,
    )
}

fn internal_error(error: impl ToString) -> AgentServiceError {
    service_error(AgentServiceErrorCode::Internal, error.to_string(), false)
}

fn service_error(
    code: AgentServiceErrorCode,
    message: impl Into<String>,
    retryable: bool,
) -> AgentServiceError {
    AgentServiceError::new(code, message, retryable)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn child_history_digest_is_stable_across_repeated_read_key_order() {
        let first = json!({
            "thread": {
                "id": "child",
                "turns": [{
                    "id": "turn-1",
                    "status": "completed",
                    "items": [{"type": "agentMessage", "text": "done"}]
                }]
            }
        });
        let repeated = serde_json::from_str::<Value>(
            r#"{
                "thread": {
                    "turns": [{
                        "items": [{"text": "done", "type": "agentMessage"}],
                        "status": "completed",
                        "id": "turn-1"
                    }],
                    "id": "child"
                }
            }"#,
        )
        .expect("repeated thread/read");

        assert_eq!(
            codex_thread_history_digest(&first).expect("first digest"),
            codex_thread_history_digest(&repeated).expect("repeated digest")
        );
    }
}
