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
    AgentInteractionId, AgentInteractionRequest, AgentInteractionSnapshot, AgentInteractionStatus,
    AgentItemBody, AgentItemId, AgentItemPresentation, AgentItemSnapshot,
    AgentItemTerminalEvidence, AgentLifecycleCapability, AgentLifecycleStatus, AgentPayloadDigest,
    AgentPresentationError, AgentProcessExitEvidence, AgentProfileDigest, AgentReadQuery,
    AgentReceiptState, AgentServiceDefinitionId, AgentServiceDescriptor, AgentServiceError,
    AgentServiceErrorCode, AgentSnapshot, AgentSnapshotAuthority, AgentSnapshotRevision,
    AgentSnapshotSource, AgentSourceChangeLevel, AgentSourceCoordinate, AgentSourceCursor,
    AgentSurfaceCapabilityFacet, AgentSurfaceContributionPayload, AgentSurfaceProfile,
    AgentSurfaceRoute, AgentSurfaceSemanticFacet, AgentTerminalOutcome, AgentTerminalStatus,
    AgentTurnId, AgentTurnSnapshot, AppliedAgentCommandReceipt, AppliedAgentSurface,
    AppliedAgentSurfaceContribution, AppliedAgentSurfaceReceipt, AppliedContributionStatus,
    AppliedForkAgentReceipt, AppliedInitialContextEvidence, ApplyBoundAgentSurface,
    CompleteAgentService, CreateAgentCommand, ForkAgentCommand, ForkAgentReceipt,
    InitialAgentContextPackage, InitialContextAppliedEvidence, InitialContextContributionKind,
    InitialContextDeliveryFidelity, InitialContextProfile, ResumeAgentCommand,
    RevokeBoundAgentSurface, SemanticFidelity,
};
use async_trait::async_trait;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use tokio::sync::RwLock;

use crate::vendor_generated::codex_v2::{
    command_execution_request_approval_params::CommandExecutionRequestApprovalParams,
    dynamic_tool_call_params::DynamicToolCallParams,
    file_change_request_approval_params::FileChangeRequestApprovalParams,
    mcp_server_elicitation_request_params::McpServerElicitationRequestParams,
    permissions_request_approval_params::PermissionsRequestApprovalParams,
    server_notification::ServerNotification,
    tool_request_user_input_params::ToolRequestUserInputParams,
};

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
pub struct CodexAppServerObservation {
    sequence: u64,
    source_thread_id: Option<String>,
    kind: CodexTypedObservation,
}

#[derive(Debug, Clone, PartialEq)]
enum CodexTypedObservation {
    Notification(Box<ServerNotification>),
    ServerRequest {
        request_id: Value,
        request: Box<CodexTypedServerRequest>,
    },
}

#[derive(Debug, Clone, PartialEq)]
enum CodexTypedServerRequest {
    CommandExecutionApproval(CommandExecutionRequestApprovalParams),
    FileChangeApproval(FileChangeRequestApprovalParams),
    PermissionsApproval(PermissionsRequestApprovalParams),
    UserInput(ToolRequestUserInputParams),
    DynamicTool(DynamicToolCallParams),
    McpElicitation(CodexMcpElicitationRequest),
}

#[derive(Debug, Clone, PartialEq)]
struct CodexMcpElicitationRequest {
    thread_id: String,
    turn_id: Option<String>,
    server_name: String,
    message: String,
    requested_schema: Value,
}

impl CodexAppServerObservation {
    pub fn notification(
        sequence: u64,
        method: impl Into<String>,
        params: Value,
    ) -> Result<Self, CodexCompleteAgentTransportError> {
        let method = method.into();
        let source_thread_id = source_thread_id_from_params(&params).map(ToOwned::to_owned);
        let notification = serde_json::from_value(json!({
            "method": method,
            "params": params,
        }))
        .map_err(|error| {
            CodexCompleteAgentTransportError::protocol(format!(
                "unknown or invalid Codex ServerNotification: {error}"
            ))
        })?;
        Ok(Self {
            sequence,
            source_thread_id,
            kind: CodexTypedObservation::Notification(Box::new(notification)),
        })
    }

    pub fn server_request(
        sequence: u64,
        request_id: Value,
        method: impl Into<String>,
        params: Value,
    ) -> Result<Self, CodexCompleteAgentTransportError> {
        let method = method.into();
        let source_thread_id = source_thread_id_from_params(&params)
            .map(ToOwned::to_owned)
            .ok_or_else(|| {
                CodexCompleteAgentTransportError::protocol(format!(
                    "Codex server request {method} misses threadId"
                ))
            })?;
        let request = decode_server_request(&method, params)?;
        Ok(Self {
            sequence,
            source_thread_id: Some(source_thread_id),
            kind: CodexTypedObservation::ServerRequest {
                request_id,
                request: Box::new(request),
            },
        })
    }

    pub(crate) fn sequence(&self) -> u64 {
        self.sequence
    }

    pub(crate) fn source_thread_id(&self) -> Option<&str> {
        self.source_thread_id.as_deref()
    }
}

fn decode_server_request(
    method: &str,
    params: Value,
) -> Result<CodexTypedServerRequest, CodexCompleteAgentTransportError> {
    fn decode<T: serde::de::DeserializeOwned>(
        method: &str,
        params: Value,
    ) -> Result<T, CodexCompleteAgentTransportError> {
        serde_json::from_value(params).map_err(|error| {
            CodexCompleteAgentTransportError::protocol(format!(
                "invalid Codex server request {method}: {error}"
            ))
        })
    }

    Ok(match method {
        "item/commandExecution/requestApproval" => {
            CodexTypedServerRequest::CommandExecutionApproval(decode(method, params)?)
        }
        "item/fileChange/requestApproval" => {
            CodexTypedServerRequest::FileChangeApproval(decode(method, params)?)
        }
        "item/permissions/requestApproval" => {
            CodexTypedServerRequest::PermissionsApproval(decode(method, params)?)
        }
        "item/tool/requestUserInput" => CodexTypedServerRequest::UserInput(decode(method, params)?),
        "item/tool/call" => CodexTypedServerRequest::DynamicTool(decode(method, params)?),
        "mcpServer/elicitation/request" => {
            let typed: McpServerElicitationRequestParams = decode(method, params.clone())?;
            use crate::vendor_generated::codex_v2::mcp_server_elicitation_request_params::McpServerElicitationRequestParams as Source;
            let request = match typed {
                Source::Form {
                    message,
                    requested_schema,
                    server_name,
                    thread_id,
                    turn_id,
                    ..
                } => CodexMcpElicitationRequest {
                    thread_id,
                    turn_id,
                    server_name,
                    message,
                    requested_schema: serde_json::to_value(requested_schema).map_err(|error| {
                        CodexCompleteAgentTransportError::protocol(format!(
                            "invalid Codex MCP elicitation schema: {error}"
                        ))
                    })?,
                },
                Source::OpenaiForm {
                    message,
                    requested_schema,
                    server_name,
                    thread_id,
                    turn_id,
                    ..
                } => CodexMcpElicitationRequest {
                    thread_id,
                    turn_id,
                    server_name,
                    message,
                    requested_schema,
                },
                Source::Url {
                    elicitation_id,
                    message,
                    server_name,
                    thread_id,
                    turn_id,
                    url,
                    ..
                } => CodexMcpElicitationRequest {
                    thread_id,
                    turn_id,
                    server_name,
                    message,
                    requested_schema: json!({
                        "mode": "url",
                        "elicitationId": elicitation_id,
                        "url": url,
                    }),
                },
            };
            CodexTypedServerRequest::McpElicitation(request)
        }
        _ => {
            return Err(CodexCompleteAgentTransportError::protocol(format!(
                "unsupported Codex server request {method}"
            )));
        }
    })
}

fn source_thread_id_from_params(params: &Value) -> Option<&str> {
    ["/threadId", "/thread/id", "/thread/id/value", "/thread_id"]
        .into_iter()
        .find_map(|pointer| params.pointer(pointer).and_then(Value::as_str))
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

    pub fn descriptor_for(
        definition_id: AgentServiceDefinitionId,
        title: impl Into<String>,
    ) -> AgentServiceDescriptor {
        let immutable = |semantics| AgentSurfaceCapabilityFacet {
            semantics,
            routes: BTreeSet::from([AgentSurfaceRoute::ImmutableDelivery]),
            fidelity: SemanticFidelity::Exact,
            configuration_boundary: AgentConfigurationBoundary::Binding,
        };
        AgentServiceDescriptor {
            definition_id,
            title: title.into(),
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
        expected_command_id: &agentdash_agent_service_api::AgentCommandId,
        expected_source: Option<&AgentSourceCoordinate>,
    ) -> Result<Option<AgentCommandReceipt>, AgentServiceError> {
        let state = self.state.read().await;
        match state.effects.get(effect_id) {
            Some(RecordedReceipt::Command { family, receipt })
                if *family == expected_family
                    && &receipt.command_id == expected_command_id
                    && expected_source.is_none_or(|source| source == &receipt.source) =>
            {
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
        Ok(Self::descriptor_for(
            self.config.definition_id.clone(),
            self.config.title.clone(),
        ))
    }

    async fn create(
        &self,
        command: CreateAgentCommand,
    ) -> Result<AgentCommandReceipt, AgentServiceError> {
        if let Some(receipt) = self
            .replay_command(
                &command.meta.effect_id,
                CommandEffectFamily::Create,
                &command.meta.command_id,
                command.requested_source.as_ref(),
            )
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
            .replay_command(
                &command.meta.effect_id,
                CommandEffectFamily::Resume,
                &command.meta.command_id,
                Some(&command.source),
            )
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
                Some(RecordedReceipt::Fork { receipt, .. })
                    if receipt.command_id == command.meta.command_id
                        && receipt.parent_source == command.source
                        && receipt.cutoff == command.cutoff
                        && command
                            .requested_child_source
                            .as_ref()
                            .is_none_or(|source| receipt.child_source.as_ref() == Some(source)) =>
                {
                    return Ok(receipt.clone());
                }
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
            .replay_command(
                &command.meta.effect_id,
                CommandEffectFamily::Command,
                &command.meta.command_id,
                Some(&command.source),
            )
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
                let result = interaction_result(&pending.interaction.request, response)?;
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
        let thread_name = response_thread_name(&result)?;
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
        let conversation_history =
            crate::canonical_projection::snapshot_records(query.source.as_str(), &result)
                .map_err(internal_error)?;
        Ok(AgentSnapshot {
            source: query.source,
            revision: AgentSnapshotRevision(source.revision),
            lifecycle: source.lifecycle,
            active_turn_id,
            turns,
            interactions,
            thread_name: Some(agentdash_agent_service_api::AgentThreadNameSnapshot {
                thread_name,
                source_info: AgentSnapshotSource {
                    authority: AgentSnapshotAuthority::AgentAuthoritative,
                    source_revision: None,
                    fidelity: SemanticFidelity::Exact,
                    observed_at_ms: now_ms(),
                },
            }),
            source_info: AgentSnapshotSource {
                authority: AgentSnapshotAuthority::AgentObserved,
                // App Server does not expose a stable durable snapshot/context revision.
                source_revision: None,
                fidelity: SemanticFidelity::Observed,
                observed_at_ms: now_ms(),
            },
            applied_surface: source.applied_surface.clone(),
            initial_context: source.initial_context.clone(),
            conversation_history,
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
                // Codex has no source-native effect lookup. An empty adapter ledger after process
                // restart therefore cannot prove non-application and must not authorize a resend.
                state: AgentEffectInspectionState::Unknown,
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
                Some(RecordedReceipt::SurfaceApply(receipt))
                    if receipt.command_id == command.command_id
                        && receipt.source == command.source
                        && receipt.applied.revision == command.bound_surface.revision
                        && receipt.applied.digest == command.bound_surface.digest =>
                {
                    return Ok(receipt.clone());
                }
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
        let descriptor =
            Self::descriptor_for(self.config.definition_id.clone(), self.config.title.clone());
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
            .replay_command(
                &command.effect_id,
                CommandEffectFamily::SurfaceRevoke,
                &command.command_id,
                Some(&command.source),
            )
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
        let sequence = observation.sequence;
        match observation.kind {
            CodexTypedObservation::ServerRequest {
                request_id,
                request,
            } => {
                let interaction = map_server_request(sequence, &request_id, *request)?;
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
            CodexTypedObservation::Notification(notification) => {
                let presentation = crate::canonical_projection::notification_record(
                    source.as_str(),
                    sequence,
                    &notification,
                )
                .map_err(internal_error)?
                .into_iter()
                .collect();
                let state = map_notification(source, *notification)?;
                Ok(AgentChangePayload::SourceObservation {
                    state: Box::new(state),
                    presentation,
                })
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

fn response_thread_name(result: &Value) -> Result<Option<String>, AgentServiceError> {
    match result.pointer("/thread/name") {
        Some(Value::String(value)) if !value.trim().is_empty() => Ok(Some(value.clone())),
        Some(Value::Null) | None => Ok(None),
        Some(Value::String(_)) => Err(protocol_violation(
            "thread/read returned a blank thread name",
        )),
        Some(_) => Err(protocol_violation(
            "thread/read returned a non-string thread name",
        )),
    }
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
            let verified_turn = result
                .pointer("/thread/turns")
                .and_then(Value::as_array)
                .and_then(|turns| turns.last());
            let verified_turn_id = verified_turn
                .and_then(|turn| turn.get("id"))
                .and_then(Value::as_str);
            let verified_turn_status = verified_turn
                .and_then(|turn| turn.get("status"))
                .and_then(Value::as_str);
            if verified_turn_id == Some(turn_id.as_str())
                && verified_turn_status == Some("completed")
            {
                Ok(())
            } else {
                Err(protocol_violation(
                    "thread/read child history does not end at the requested completed-turn cutoff",
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
            .map(|item| map_item(item, Some(status)))
            .collect::<Result<Vec<_>, _>>()?;
        mapped.push(AgentTurnSnapshot { id, status, items });
    }
    Ok((mapped, active))
}

fn map_item(
    item: &Value,
    inherited_status: Option<AgentEntityStatus>,
) -> Result<AgentItemSnapshot, AgentServiceError> {
    use crate::vendor_generated::codex_v2::thread_item::ThreadItem;

    let id = required_id::<AgentItemId>(item, "id", AgentItemId::new)?;
    let explicit_status = entity_status(item.get("status"));
    let status = if explicit_status == AgentEntityStatus::Accepted {
        inherited_status.unwrap_or(explicit_status)
    } else {
        explicit_status
    };
    let vendor: ThreadItem = serde_json::from_value(item.clone()).map_err(|error| {
        protocol_violation(format!(
            "unknown or invalid Codex ThreadItem cannot enter canonical history: {error}"
        ))
    })?;
    let body = match vendor {
        ThreadItem::UserMessage { content, .. } => AgentItemBody::UserMessage {
            content: content
                .iter()
                .map(map_vendor_user_input)
                .collect::<Result<Vec<_>, _>>()?,
        },
        ThreadItem::HookPrompt { fragments, .. } => AgentItemBody::HookPrompt {
            hook_point: "codex".to_owned(),
            content: fragments
                .into_iter()
                .map(
                    |fragment| agentdash_agent_service_api::AgentContentBlock::Text {
                        text: fragment.text,
                    },
                )
                .collect(),
        },
        ThreadItem::AgentMessage { text, phase, .. } => AgentItemBody::AgentMessage {
            content: vec![text_block(text)],
            phase: phase
                .flatten()
                .map(serde_json::to_value)
                .transpose()
                .map_err(internal_error)?
                .and_then(|value| value.as_str().map(ToOwned::to_owned)),
        },
        ThreadItem::Plan { text, .. } => AgentItemBody::Plan {
            explanation: Some(text),
            steps: Vec::new(),
        },
        ThreadItem::Reasoning {
            content, summary, ..
        } => AgentItemBody::Reasoning {
            summary: summary.into_iter().map(text_block).collect(),
            content: content.into_iter().map(text_block).collect(),
        },
        ThreadItem::CommandExecution {
            command,
            cwd,
            aggregated_output,
            ..
        } => AgentItemBody::CommandExecution {
            command,
            cwd: json_string(&cwd)?,
            output: aggregated_output
                .flatten()
                .into_iter()
                .map(|text| agentdash_agent_service_api::AgentCommandOutput {
                    stream: agentdash_agent_service_api::AgentCommandOutputStream::Combined,
                    text,
                })
                .collect(),
        },
        ThreadItem::FileChange { changes, .. } => AgentItemBody::FileChange {
            changes: changes
                .into_iter()
                .map(|change| {
                    let kind = serde_json::to_value(&change.kind).map_err(internal_error)?;
                    let (change_kind, moved_to) = map_patch_kind(&kind)?;
                    Ok(agentdash_agent_service_api::AgentFilePatch {
                        path: change.path,
                        change_kind,
                        patch: change.diff,
                        moved_to,
                    })
                })
                .collect::<Result<Vec<_>, AgentServiceError>>()?,
            output: Vec::new(),
        },
        ThreadItem::McpToolCall {
            server,
            tool,
            arguments,
            result,
            error,
            ..
        } => AgentItemBody::McpToolCall {
            server,
            tool,
            arguments,
            result: result
                .flatten()
                .map(serde_json::to_value)
                .transpose()
                .map_err(internal_error)?
                .or(error
                    .flatten()
                    .map(serde_json::to_value)
                    .transpose()
                    .map_err(internal_error)?),
            progress: Vec::new(),
        },
        ThreadItem::DynamicToolCall {
            namespace,
            tool,
            arguments,
            content_items,
            success,
            ..
        } => AgentItemBody::DynamicToolCall {
            namespace: namespace.flatten(),
            tool,
            arguments,
            result: success.flatten().map(|success| json!({"success": success})),
            progress: content_items
                .flatten()
                .unwrap_or_default()
                .into_iter()
                .map(map_dynamic_output)
                .collect(),
        },
        ThreadItem::CollabAgentToolCall {
            tool,
            receiver_thread_ids,
            prompt,
            agents_states,
            ..
        } => AgentItemBody::CollaborationToolCall {
            action: tool.to_string(),
            target: (!receiver_thread_ids.is_empty()).then(|| receiver_thread_ids.join(",")),
            prompt: prompt.flatten(),
            result: Some(serde_json::to_value(agents_states).map_err(internal_error)?),
        },
        ThreadItem::SubAgentActivity {
            agent_thread_id,
            agent_path,
            kind,
            ..
        } => AgentItemBody::SubagentActivity {
            agent_id: agent_thread_id,
            task: agent_path,
            status: kind.to_string(),
            result: Vec::new(),
        },
        ThreadItem::WebSearch { query, action, .. } => {
            let action_value = action
                .flatten()
                .map(serde_json::to_value)
                .transpose()
                .map_err(internal_error)?;
            AgentItemBody::WebSearch {
                action: action_value
                    .as_ref()
                    .and_then(|value| value.get("type"))
                    .and_then(Value::as_str)
                    .unwrap_or("search")
                    .to_owned(),
                query: Some(query),
                url: action_value
                    .as_ref()
                    .and_then(|value| value.get("url"))
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned),
                results: Vec::new(),
            }
        }
        ThreadItem::ImageView { path, .. } => AgentItemBody::ImageView {
            path: json_string_required(&path)?,
            detail: None,
        },
        ThreadItem::Sleep { duration_ms, .. } => AgentItemBody::Sleep {
            duration_ms: agentdash_agent_service_api::AgentServiceU64(duration_ms),
        },
        ThreadItem::ImageGeneration {
            result,
            revised_prompt,
            saved_path,
            ..
        } => {
            let mut outputs = vec![text_block(result)];
            if let Some(path) = saved_path.flatten() {
                outputs.push(
                    agentdash_agent_service_api::AgentContentBlock::LocalResource {
                        path: json_string_required(&path)?,
                        media_type: None,
                        digest: None,
                    },
                );
            }
            AgentItemBody::ImageGeneration {
                prompt: String::new(),
                revised_prompt: revised_prompt.flatten(),
                outputs,
            }
        }
        ThreadItem::EnteredReviewMode { review, .. } => AgentItemBody::Review {
            findings: Vec::new(),
            summary: Some(review),
        },
        ThreadItem::ExitedReviewMode { review, .. } => AgentItemBody::Review {
            findings: Vec::new(),
            summary: Some(review),
        },
        ThreadItem::ContextCompaction { .. } => AgentItemBody::ContextCompaction {
            summary: None,
            source_digest: None,
        },
    };
    let presentation = AgentItemPresentation::new(
        body,
        item.get("startedAt").and_then(Value::as_u64),
        item.get("updatedAt").and_then(Value::as_u64),
        terminal_evidence(status, item)?,
    )
    .map_err(internal_error)?;
    Ok(AgentItemSnapshot {
        id,
        status,
        presentation,
    })
}

fn text_block(text: String) -> agentdash_agent_service_api::AgentContentBlock {
    agentdash_agent_service_api::AgentContentBlock::Text { text }
}

fn map_vendor_user_input(
    input: &crate::vendor_generated::codex_v2::thread_item::UserInput,
) -> Result<agentdash_agent_service_api::AgentContentBlock, AgentServiceError> {
    use crate::vendor_generated::codex_v2::thread_item::UserInput;
    Ok(match input {
        UserInput::Text { text, .. } => text_block(text.clone()),
        UserInput::Image { url, detail } => agentdash_agent_service_api::AgentContentBlock::Image {
            media_type: "application/octet-stream".to_owned(),
            source: url.clone(),
            detail: detail
                .as_ref()
                .and_then(|value| value.as_ref())
                .map(ToString::to_string),
            digest: AgentPayloadDigest::new(format!("sha256:{:x}", Sha256::digest(url.as_bytes())))
                .map_err(internal_error)?,
        },
        UserInput::LocalImage { path, detail: _ } => {
            agentdash_agent_service_api::AgentContentBlock::LocalResource {
                path: path.clone(),
                media_type: Some("image/*".to_owned()),
                digest: None,
            }
        }
        UserInput::Skill { name, path } => {
            agentdash_agent_service_api::AgentContentBlock::SkillReference {
                name: name.clone(),
                path: Some(path.clone()),
            }
        }
        UserInput::Mention { name, path } => {
            agentdash_agent_service_api::AgentContentBlock::Mention {
                label: name.clone(),
                reference: path.clone(),
            }
        }
    })
}

fn map_dynamic_output(
    output: crate::vendor_generated::codex_v2::thread_item::DynamicToolCallOutputContentItem,
) -> agentdash_agent_service_api::AgentContentBlock {
    use crate::vendor_generated::codex_v2::thread_item::DynamicToolCallOutputContentItem;
    match output {
        DynamicToolCallOutputContentItem::InputText { text } => text_block(text),
        DynamicToolCallOutputContentItem::InputImage { image_url } => {
            agentdash_agent_service_api::AgentContentBlock::ResourceLink {
                uri: image_url,
                title: None,
                media_type: Some("image/*".to_owned()),
                digest: None,
            }
        }
    }
}

fn map_patch_kind(
    value: &Value,
) -> Result<
    (
        agentdash_agent_service_api::AgentFileChangeKind,
        Option<String>,
    ),
    AgentServiceError,
> {
    let discriminant = value
        .as_str()
        .or_else(|| value.get("type").and_then(Value::as_str))
        .ok_or_else(|| protocol_violation("Codex patch kind is not typed"))?;
    let kind = match discriminant {
        "add" => agentdash_agent_service_api::AgentFileChangeKind::Add,
        "delete" => agentdash_agent_service_api::AgentFileChangeKind::Delete,
        "update" => agentdash_agent_service_api::AgentFileChangeKind::Update,
        "move" => agentdash_agent_service_api::AgentFileChangeKind::Move,
        other => {
            return Err(protocol_violation(format!(
                "unknown Codex patch kind {other}"
            )));
        }
    };
    let moved_to = value
        .get("move_path")
        .or_else(|| value.get("movePath"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    Ok((kind, moved_to))
}

fn json_string<T: serde::Serialize>(value: &T) -> Result<Option<String>, AgentServiceError> {
    let value = serde_json::to_value(value).map_err(internal_error)?;
    Ok(value.as_str().map(ToOwned::to_owned))
}

fn json_string_required<T: serde::Serialize>(value: &T) -> Result<String, AgentServiceError> {
    json_string(value)?.ok_or_else(|| protocol_violation("Codex path is not a string"))
}

fn terminal_evidence(
    status: AgentEntityStatus,
    item: &Value,
) -> Result<Option<AgentItemTerminalEvidence>, AgentServiceError> {
    let outcome = match status {
        AgentEntityStatus::Accepted | AgentEntityStatus::Running => return Ok(None),
        AgentEntityStatus::Completed => AgentTerminalStatus::Completed,
        AgentEntityStatus::Failed => AgentTerminalStatus::Failed,
        AgentEntityStatus::Interrupted => AgentTerminalStatus::Interrupted,
        AgentEntityStatus::Lost => AgentTerminalStatus::Lost,
    };
    let exit_code = item
        .get("exitCode")
        .and_then(Value::as_i64)
        .map(i32::try_from)
        .transpose()
        .map_err(|_| protocol_violation("Codex process exit code exceeds i32"))?;
    let process_exit = (exit_code.is_some() || item.get("processId").is_some()).then(|| {
        AgentProcessExitEvidence {
            exit_code,
            signal: None,
            success: exit_code == Some(0),
        }
    });
    let error = item
        .get("error")
        .filter(|value| !value.is_null())
        .map(|value| AgentPresentationError {
            code: "codex_item_failed".to_owned(),
            message: value
                .as_str()
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| value.to_string()),
            retryable: false,
        });
    Ok(Some(AgentItemTerminalEvidence {
        outcome,
        completed_at_ms: item
            .get("completedAt")
            .and_then(Value::as_u64)
            .map(agentdash_agent_service_api::AgentServiceU64),
        duration_ms: item
            .get("durationMs")
            .and_then(Value::as_u64)
            .map(agentdash_agent_service_api::AgentServiceU64),
        process_exit,
        error,
    }))
}

fn map_notification(
    source: &AgentSourceCoordinate,
    notification: ServerNotification,
) -> Result<AgentChangePayload, AgentServiceError> {
    use crate::vendor_generated::codex_v2::server_notification::ServerNotification as Source;

    match notification {
        Source::ThreadNameUpdated(notification) => {
            let notification_source =
                AgentSourceCoordinate::new(notification.thread_id).map_err(internal_error)?;
            if &notification_source != source {
                return Err(protocol_violation(
                    "thread/name/updated belongs to a different source thread",
                ));
            }
            if notification
                .thread_name
                .as_ref()
                .is_some_and(|value| value.trim().is_empty())
            {
                return Err(protocol_violation(
                    "thread/name/updated returned a blank thread name",
                ));
            }
            Ok(AgentChangePayload::ThreadNameChanged {
                thread_name: notification.thread_name,
                source_info: AgentSnapshotSource {
                    authority: AgentSnapshotAuthority::AgentAuthoritative,
                    source_revision: None,
                    fidelity: SemanticFidelity::Exact,
                    observed_at_ms: now_ms(),
                },
            })
        }
        Source::TurnStarted(notification) => {
            require_notification_source(source, &notification.thread_id, "turn/started")?;
            Ok(AgentChangePayload::TurnChanged {
                turn: map_notification_turn(notification.turn)?,
            })
        }
        Source::TurnCompleted(notification) => {
            require_notification_source(source, &notification.thread_id, "turn/completed")?;
            Ok(AgentChangePayload::TurnChanged {
                turn: map_notification_turn(notification.turn)?,
            })
        }
        Source::ItemStarted(notification) => {
            require_notification_source(source, &notification.thread_id, "item/started")?;
            let turn_id = AgentTurnId::new(notification.turn_id).map_err(internal_error)?;
            let item = map_notification_item(
                notification.item,
                AgentEntityStatus::Running,
                Some(notification.started_at_ms),
                None,
            )?;
            Ok(AgentChangePayload::ItemTransitioned {
                turn_id,
                item_id: item.id,
                transition: agentdash_agent_service_api::AgentItemTransition::Started {
                    presentation: item.presentation.clone(),
                },
            })
        }
        Source::ItemCompleted(notification) => {
            require_notification_source(source, &notification.thread_id, "item/completed")?;
            let turn_id = AgentTurnId::new(notification.turn_id).map_err(internal_error)?;
            let item = map_notification_item(
                notification.item,
                AgentEntityStatus::Completed,
                None,
                Some(notification.completed_at_ms),
            )?;
            Ok(AgentChangePayload::ItemTransitioned {
                turn_id,
                item_id: item.id,
                transition: agentdash_agent_service_api::AgentItemTransition::Terminal {
                    presentation: item.presentation.clone(),
                },
            })
        }
        Source::ThreadArchived(notification) => {
            require_notification_source(source, &notification.thread_id, "thread/archived")?;
            Ok(AgentChangePayload::LifecycleChanged {
                status: AgentLifecycleStatus::Closed,
            })
        }
        Source::ThreadCompacted(_)
        | Source::Error(_)
        | Source::ThreadStarted(_)
        | Source::ThreadStatusChanged(_)
        | Source::ThreadDeleted(_)
        | Source::ThreadUnarchived(_)
        | Source::ThreadClosed(_)
        | Source::SkillsChanged(_)
        | Source::ThreadGoalUpdated(_)
        | Source::ThreadGoalCleared(_)
        | Source::ThreadSettingsUpdated(_)
        | Source::ThreadTokenUsageUpdated(_)
        | Source::HookStarted(_)
        | Source::HookCompleted(_)
        | Source::TurnDiffUpdated(_)
        | Source::TurnPlanUpdated(_)
        | Source::ItemAutoApprovalReviewStarted(_)
        | Source::ItemAutoApprovalReviewCompleted(_)
        | Source::ItemAgentMessageDelta(_)
        | Source::ItemPlanDelta(_)
        | Source::CommandExecOutputDelta(_)
        | Source::ProcessOutputDelta(_)
        | Source::ProcessExited(_)
        | Source::ItemCommandExecutionOutputDelta(_)
        | Source::ItemCommandExecutionTerminalInteraction(_)
        | Source::ItemFileChangeOutputDelta(_)
        | Source::ItemFileChangePatchUpdated(_)
        | Source::ServerRequestResolved(_)
        | Source::ItemMcpToolCallProgress(_)
        | Source::McpServerOauthLoginCompleted(_)
        | Source::McpServerStartupStatusUpdated(_)
        | Source::AccountUpdated(_)
        | Source::AccountRateLimitsUpdated(_)
        | Source::AppListUpdated(_)
        | Source::RemoteControlStatusChanged(_)
        | Source::ExternalAgentConfigImportProgress(_)
        | Source::ExternalAgentConfigImportCompleted(_)
        | Source::FsChanged(_)
        | Source::ItemReasoningSummaryTextDelta(_)
        | Source::ItemReasoningSummaryPartAdded(_)
        | Source::ItemReasoningTextDelta(_)
        | Source::ModelRerouted(_)
        | Source::ModelVerification(_)
        | Source::TurnModerationMetadata(_)
        | Source::ModelSafetyBufferingUpdated(_)
        | Source::Warning(_)
        | Source::GuardianWarning(_)
        | Source::DeprecationNotice(_)
        | Source::ConfigWarning(_)
        | Source::FuzzyFileSearchSessionUpdated(_)
        | Source::FuzzyFileSearchSessionCompleted(_)
        | Source::ThreadRealtimeStarted(_)
        | Source::ThreadRealtimeItemAdded(_)
        | Source::ThreadRealtimeTranscriptDelta(_)
        | Source::ThreadRealtimeTranscriptDone(_)
        | Source::ThreadRealtimeOutputAudioDelta(_)
        | Source::ThreadRealtimeSdp(_)
        | Source::ThreadRealtimeError(_)
        | Source::ThreadRealtimeClosed(_)
        | Source::WindowsWorldWritableWarning(_)
        | Source::WindowsSandboxSetupCompleted(_)
        | Source::AccountLoginCompleted(_) => Ok(AgentChangePayload::SnapshotInvalidated {
            reason: "typed Codex notification requires thread/read reconciliation".to_owned(),
        }),
    }
}

fn map_server_request(
    sequence: u64,
    request_id: &Value,
    request: CodexTypedServerRequest,
) -> Result<AgentInteractionSnapshot, AgentServiceError> {
    let fallback_id = request_id_string(request_id)?;
    let (interaction_id, turn_id, item_id, request) = match request {
        CodexTypedServerRequest::CommandExecutionApproval(params) => {
            let proposed_action = serde_json::to_value(&params).map_err(internal_error)?;
            (
                params.approval_id.unwrap_or_else(|| fallback_id.clone()),
                params.turn_id,
                Some(params.item_id),
                AgentInteractionRequest::Approval {
                    prompt: params
                        .command
                        .unwrap_or_else(|| "Codex requests command approval".to_owned()),
                    reason: params.reason,
                    proposed_action: Some(proposed_action),
                },
            )
        }
        CodexTypedServerRequest::FileChangeApproval(params) => {
            let proposed_action = serde_json::to_value(&params).map_err(internal_error)?;
            (
                fallback_id.clone(),
                params.turn_id,
                Some(params.item_id),
                AgentInteractionRequest::Approval {
                    prompt: params
                        .reason
                        .clone()
                        .unwrap_or_else(|| "Codex requests file change approval".to_owned()),
                    reason: params.reason,
                    proposed_action: Some(proposed_action),
                },
            )
        }
        CodexTypedServerRequest::PermissionsApproval(params) => {
            let proposed_action = serde_json::to_value(&params).map_err(internal_error)?;
            (
                fallback_id.clone(),
                params.turn_id,
                Some(params.item_id),
                AgentInteractionRequest::Approval {
                    prompt: params
                        .reason
                        .clone()
                        .unwrap_or_else(|| "Codex requests additional permissions".to_owned()),
                    reason: params.reason,
                    proposed_action: Some(proposed_action),
                },
            )
        }
        CodexTypedServerRequest::UserInput(params) => (
            fallback_id.clone(),
            params.turn_id,
            Some(params.item_id),
            AgentInteractionRequest::UserInput {
                prompt: "Codex requests user input".to_owned(),
                questions: params
                    .questions
                    .into_iter()
                    .map(|question| {
                        let options = question.options;
                        let allows_free_form = question.is_other || options.is_none();
                        agentdash_agent_service_api::AgentInteractionQuestion {
                            id: question.id,
                            prompt: question.question,
                            options: options
                                .unwrap_or_default()
                                .into_iter()
                                .map(|option| option.label)
                                .collect(),
                            allows_free_form,
                        }
                    })
                    .collect(),
            },
        ),
        CodexTypedServerRequest::DynamicTool(params) => (
            params.call_id,
            params.turn_id,
            None,
            AgentInteractionRequest::DynamicTool {
                namespace: params.namespace,
                tool: params.tool,
                prompt: "Codex requests a dynamic tool call".to_owned(),
                arguments: params.arguments,
            },
        ),
        CodexTypedServerRequest::McpElicitation(params) => (
            fallback_id,
            params.turn_id.ok_or_else(|| {
                protocol_violation("Codex MCP elicitation cannot be correlated to an active turn")
            })?,
            None,
            AgentInteractionRequest::McpElicitation {
                server: params.server_name,
                prompt: params.message,
                schema: params.requested_schema,
            },
        ),
    };
    Ok(AgentInteractionSnapshot {
        id: AgentInteractionId::new(if interaction_id.is_empty() {
            format!("codex-request-{sequence}")
        } else {
            interaction_id
        })
        .map_err(internal_error)?,
        turn_id: AgentTurnId::new(turn_id).map_err(internal_error)?,
        item_id: item_id
            .map(AgentItemId::new)
            .transpose()
            .map_err(internal_error)?,
        request,
        status: AgentInteractionStatus::Pending,
        resolution: None,
    })
}

fn request_id_string(request_id: &Value) -> Result<String, AgentServiceError> {
    match request_id {
        Value::String(value) => Ok(value.clone()),
        Value::Number(value) if value.is_i64() || value.is_u64() => Ok(value.to_string()),
        _ => Err(protocol_violation(
            "Codex server request id must be a string or integer",
        )),
    }
}

fn require_notification_source(
    expected: &AgentSourceCoordinate,
    actual: &str,
    method: &str,
) -> Result<(), AgentServiceError> {
    if expected.as_str() == actual {
        Ok(())
    } else {
        Err(protocol_violation(format!(
            "{method} belongs to a different source thread"
        )))
    }
}

fn project_codex_turn_status(
    status: crate::vendor_generated::codex_v2::server_notification::TurnStatus,
) -> AgentEntityStatus {
    use crate::vendor_generated::codex_v2::server_notification::TurnStatus;
    match status {
        TurnStatus::Completed => AgentEntityStatus::Completed,
        TurnStatus::Interrupted => AgentEntityStatus::Interrupted,
        TurnStatus::Failed => AgentEntityStatus::Failed,
        TurnStatus::InProgress => AgentEntityStatus::Running,
    }
}

fn map_notification_turn(
    turn: crate::vendor_generated::codex_v2::server_notification::Turn,
) -> Result<AgentTurnSnapshot, AgentServiceError> {
    let status = project_codex_turn_status(turn.status);
    let items = turn
        .items
        .into_iter()
        .map(|item| {
            let value = serde_json::to_value(item).map_err(internal_error)?;
            map_item(&value, Some(status))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(AgentTurnSnapshot {
        id: AgentTurnId::new(turn.id).map_err(internal_error)?,
        status,
        items,
    })
}

fn map_notification_item(
    item: crate::vendor_generated::codex_v2::server_notification::ThreadItem,
    inherited_status: AgentEntityStatus,
    started_at_ms: Option<i64>,
    completed_at_ms: Option<i64>,
) -> Result<AgentItemSnapshot, AgentServiceError> {
    let value = serde_json::to_value(item).map_err(internal_error)?;
    let mut mapped = map_item(&value, Some(inherited_status))?;
    let started_at_ms = started_at_ms
        .map(u64::try_from)
        .transpose()
        .map_err(|_| protocol_violation("Codex item startedAtMs must be non-negative"))?
        .or(mapped.presentation.started_at_ms.map(|value| value.0));
    let mut terminal = mapped.presentation.terminal.clone();
    if let (Some(completed_at_ms), Some(terminal)) = (completed_at_ms, terminal.as_mut()) {
        terminal.completed_at_ms = Some(agentdash_agent_service_api::AgentServiceU64(
            u64::try_from(completed_at_ms)
                .map_err(|_| protocol_violation("Codex item completedAtMs must be non-negative"))?,
        ));
    }
    mapped.presentation = AgentItemPresentation::new(
        mapped.presentation.body,
        started_at_ms,
        completed_at_ms
            .map(u64::try_from)
            .transpose()
            .map_err(|_| protocol_violation("Codex item completedAtMs must be non-negative"))?
            .or(mapped.presentation.updated_at_ms.map(|value| value.0)),
        terminal,
    )
    .map_err(internal_error)?;
    Ok(mapped)
}

fn interaction_result(
    request: &AgentInteractionRequest,
    response: &agentdash_agent_service_api::AgentInteractionResponse,
) -> Result<Value, AgentServiceError> {
    use agentdash_agent_service_api::AgentInteractionResponse;
    match (request, response) {
        (AgentInteractionRequest::Approval { .. }, AgentInteractionResponse::Approved) => {
            Ok(json!({"decision": "accept"}))
        }
        (AgentInteractionRequest::Approval { .. }, AgentInteractionResponse::Denied { reason }) => {
            Ok(json!({"decision": "decline", "reason": reason}))
        }
        (
            AgentInteractionRequest::UserInput { .. },
            AgentInteractionResponse::UserInput { input },
        ) => {
            let (input, additional) = codex_input(input)?;
            Ok(json!({"input": input, "additionalContext": additional}))
        }
        (
            AgentInteractionRequest::DynamicTool { .. },
            AgentInteractionResponse::DynamicToolResult { result },
        ) => Ok(json!({"result": result})),
        (
            AgentInteractionRequest::McpElicitation { .. },
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

    #[test]
    fn unknown_vendor_item_is_rejected_before_canonical_history() {
        let error = map_item(
            &json!({
                "type": "futureVendorItem",
                "id": "item-1",
                "payload": {"vendor": true}
            }),
            None,
        )
        .expect_err("unknown vendor item must not become a generic canonical item");

        assert_eq!(error.code, AgentServiceErrorCode::ProtocolViolation);
        assert!(
            error
                .message
                .contains("unknown or invalid Codex ThreadItem")
        );
    }

    #[test]
    fn every_admitted_server_request_is_typed_before_it_becomes_an_interaction() {
        let fixtures = [
            (
                "item/commandExecution/requestApproval",
                json!({
                    "approvalId": "approval-1",
                    "threadId": "thread-1",
                    "turnId": "turn-1",
                    "itemId": "item-1",
                    "startedAtMs": 1,
                    "command": "cargo test",
                    "reason": "approve command"
                }),
            ),
            (
                "item/fileChange/requestApproval",
                json!({
                    "threadId": "thread-1",
                    "turnId": "turn-1",
                    "itemId": "item-1",
                    "startedAtMs": 1,
                    "reason": "approve files"
                }),
            ),
            (
                "item/permissions/requestApproval",
                json!({
                    "threadId": "thread-1",
                    "turnId": "turn-1",
                    "itemId": "item-1",
                    "startedAtMs": 1,
                    "cwd": "C:\\repo",
                    "permissions": {},
                    "reason": "approve permissions"
                }),
            ),
            (
                "item/tool/requestUserInput",
                json!({
                    "threadId": "thread-1",
                    "turnId": "turn-1",
                    "itemId": "item-1",
                    "questions": [{
                        "header": "Choice",
                        "id": "choice",
                        "question": "Choose",
                        "options": [{"label": "A", "description": "first"}]
                    }]
                }),
            ),
            (
                "item/tool/call",
                json!({
                    "threadId": "thread-1",
                    "turnId": "turn-1",
                    "callId": "call-1",
                    "namespace": "dash",
                    "tool": "lookup",
                    "arguments": {"query": "value"}
                }),
            ),
            (
                "mcpServer/elicitation/request",
                json!({
                    "mode": "openai/form",
                    "threadId": "thread-1",
                    "turnId": "turn-1",
                    "serverName": "mcp",
                    "message": "Provide input",
                    "requestedSchema": {"type": "object"}
                }),
            ),
        ];

        for (index, (method, params)) in fixtures.into_iter().enumerate() {
            let observation =
                CodexAppServerObservation::server_request(1, json!(index + 1), method, params)
                    .expect("typed server request");
            assert_eq!(observation.source_thread_id(), Some("thread-1"));
            let CodexTypedObservation::ServerRequest {
                request_id,
                request,
            } = observation.kind
            else {
                panic!("server request root");
            };
            let interaction =
                map_server_request(1, &request_id, *request).expect("canonical interaction");
            assert_eq!(interaction.turn_id.as_str(), "turn-1");
            assert!(matches!(
                (index, interaction.request),
                (0..=2, AgentInteractionRequest::Approval { .. })
                    | (3, AgentInteractionRequest::UserInput { .. })
                    | (4, AgentInteractionRequest::DynamicTool { .. })
                    | (5, AgentInteractionRequest::McpElicitation { .. })
            ));
        }
    }

    #[test]
    fn unknown_methods_and_invalid_admitted_payloads_are_protocol_violations() {
        for error in [
            CodexAppServerObservation::notification(
                1,
                "future/notification",
                json!({"threadId": "thread-1"}),
            )
            .expect_err("unknown notification"),
            CodexAppServerObservation::notification(
                1,
                "thread/name/updated",
                json!({"threadName": "missing source"}),
            )
            .expect_err("invalid admitted notification"),
            CodexAppServerObservation::server_request(
                1,
                json!(1),
                "future/request",
                json!({"threadId": "thread-1"}),
            )
            .expect_err("unknown request"),
            CodexAppServerObservation::server_request(
                1,
                json!(1),
                "item/tool/call",
                json!({"threadId": "thread-1"}),
            )
            .expect_err("invalid admitted request"),
        ] {
            assert!(!error.retryable);
            assert!(!error.outcome_unknown);
        }
    }

    #[test]
    fn admitted_notification_families_project_without_retaining_raw_method_payload_pairs() {
        let source = AgentSourceCoordinate::new("thread-1").expect("source");
        let turn = |status: &str| {
            json!({
                "id": "turn-1",
                "items": [],
                "status": status
            })
        };
        let item = || json!({"type": "agentMessage", "id": "item-1", "text": "hello"});
        let fixtures = [
            (
                "thread/name/updated",
                json!({"threadId": "thread-1", "threadName": "name"}),
            ),
            (
                "turn/started",
                json!({"threadId": "thread-1", "turn": turn("inProgress")}),
            ),
            (
                "turn/completed",
                json!({"threadId": "thread-1", "turn": turn("failed")}),
            ),
            (
                "item/started",
                json!({
                    "threadId": "thread-1",
                    "turnId": "turn-1",
                    "startedAtMs": 1,
                    "item": item()
                }),
            ),
            (
                "item/completed",
                json!({
                    "threadId": "thread-1",
                    "turnId": "turn-1",
                    "completedAtMs": 2,
                    "item": item()
                }),
            ),
            ("thread/archived", json!({"threadId": "thread-1"})),
            (
                "thread/compacted",
                json!({"threadId": "thread-1", "turnId": "turn-1"}),
            ),
        ];

        for (index, (method, params)) in fixtures.into_iter().enumerate() {
            let observation =
                CodexAppServerObservation::notification(index as u64 + 1, method, params)
                    .expect("typed notification");
            let CodexTypedObservation::Notification(notification) = observation.kind else {
                panic!("notification root");
            };
            let payload = map_notification(&source, *notification).expect("canonical change");
            assert!(matches!(
                (index, payload),
                (0, AgentChangePayload::ThreadNameChanged { .. })
                    | (1..=2, AgentChangePayload::TurnChanged { .. })
                    | (3..=4, AgentChangePayload::ItemTransitioned { .. })
                    | (5, AgentChangePayload::LifecycleChanged { .. })
                    | (6, AgentChangePayload::SnapshotInvalidated { .. })
            ));
        }
    }

    #[test]
    fn compaction_failure_and_loss_remain_terminal_in_snapshot_projection() {
        let source = AgentSourceCoordinate::new("thread-1").expect("source");
        let observation = CodexAppServerObservation::notification(
            1,
            "turn/completed",
            json!({
                "threadId": "thread-1",
                "turn": {
                    "id": "turn-1",
                    "status": "failed",
                    "items": [{"type": "contextCompaction", "id": "compaction-1"}]
                }
            }),
        )
        .expect("typed failed turn");
        let CodexTypedObservation::Notification(notification) = observation.kind else {
            panic!("notification root");
        };
        let AgentChangePayload::TurnChanged { turn } =
            map_notification(&source, *notification).expect("failed turn")
        else {
            panic!("turn change");
        };
        assert_eq!(turn.items[0].status, AgentEntityStatus::Failed);
        assert_eq!(
            turn.items[0]
                .presentation
                .terminal
                .as_ref()
                .expect("failed compaction evidence")
                .outcome,
            AgentTerminalStatus::Failed
        );

        for (status, expected) in [
            (AgentEntityStatus::Failed, AgentTerminalStatus::Failed),
            (AgentEntityStatus::Lost, AgentTerminalStatus::Lost),
        ] {
            let item = map_item(
                &json!({"type": "contextCompaction", "id": "compaction-1"}),
                Some(status),
            )
            .expect("typed compaction item");
            assert_eq!(item.status, status);
            assert_eq!(
                item.presentation
                    .terminal
                    .as_ref()
                    .expect("terminal evidence")
                    .outcome,
                expected
            );
        }
    }
}
