use std::collections::BTreeSet;
use std::fmt;
use std::sync::{Arc, Mutex};

use agentdash_application_agentrun::agent_run::{
    AgentFrameSurfaceExt, AgentRunProductRuntimeBindingRepository,
};
use agentdash_application_hooks::AppExecutionHookProvider;
use agentdash_application_ports::agent_frame_hook_plan::AgentFrameHookRequirement;
use agentdash_application_ports::product_runtime_tool::{
    ProductRuntimeToolKind, ProductRuntimeToolOutcome, ProductRuntimeToolRequest,
    ProductRuntimeToolService,
};
use agentdash_platform_spi::hooks::{
    AgentFrameHookEvaluationQuery, AgentFrameHookSnapshot, AgentFrameHookSnapshotQuery,
    AgentFrameRuntimeSnapshot, ContextTokenStats, HookControlTarget, HookDiagnosticEntry,
    HookError, HookPendingAction, HookPendingActionResolutionKind, HookPendingActionStatus,
    HookRuntimeAccess, HookRuntimeEvaluationQuery, HookRuntimeRefreshQuery, HookTraceEntry,
    HookTurnStartNotice, RuntimeAdapterProvenance, SetDelta,
};
use agentdash_platform_spi::{
    AgentTool, AgentToolError, AgentToolResult, PlatformToolExecutionContext,
};
use async_trait::async_trait;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use super::model_preflight::CompanionModelPreflightPort;
use super::tool_context::CompanionToolContext;
use super::tools::{
    CompanionRequestTool, CompanionRequestToolDeps, CompanionRespondTool,
    companion_request_parameters_schema, companion_respond_parameters_schema,
};
use super::workflow_script_preflight::CompanionWorkflowScriptPreflightPort;
use crate::repository_set::RepositorySet;
use crate::runtime_tools::{RuntimeThreadToolServices, SharedRuntimeThreadToolServicesHandle};
use crate::wait_activity::WaitActivityService;

#[derive(Clone)]
pub struct CompanionRuntimeToolServiceDeps {
    pub repos: RepositorySet,
    pub runtime_bindings: Arc<dyn AgentRunProductRuntimeBindingRepository>,
    pub runtime_thread_services: RuntimeThreadToolServices,
    pub wait_service: WaitActivityService,
    pub hook_provider: Arc<AppExecutionHookProvider>,
    pub model_preflight: Option<Arc<dyn CompanionModelPreflightPort>>,
    pub workflow_script_preflight: Option<Arc<dyn CompanionWorkflowScriptPreflightPort>>,
}

pub struct ApplicationCompanionRuntimeToolService {
    kind: ProductRuntimeToolKind,
    deps: CompanionRuntimeToolServiceDeps,
}

impl ApplicationCompanionRuntimeToolService {
    pub fn new(kind: ProductRuntimeToolKind, deps: CompanionRuntimeToolServiceDeps) -> Self {
        assert!(
            matches!(
                kind,
                ProductRuntimeToolKind::CompanionRequest | ProductRuntimeToolKind::CompanionRespond
            ),
            "Companion Product service only supports request/respond"
        );
        Self { kind, deps }
    }

    async fn execute_tool(
        &self,
        request: ProductRuntimeToolRequest,
    ) -> Result<AgentToolResult, AgentToolError> {
        let target = agentdash_domain::agent_run_target::AgentRunTarget {
            run_id: request.context.target.run_id,
            agent_id: request.context.target.agent_id,
        };
        let binding = self
            .deps
            .runtime_bindings
            .load_product_binding_by_runtime_thread(&request.context.runtime_thread_id)
            .await
            .map_err(AgentToolError::ExecutionFailed)?
            .ok_or_else(|| {
                AgentToolError::ExecutionFailed(
                    "Companion RuntimeThread has no durable Product binding".to_string(),
                )
            })?;
        if binding.target != target {
            return Err(AgentToolError::ExecutionFailed(
                "Companion RuntimeThread Product binding target mismatch".to_string(),
            ));
        }
        let frame = self
            .deps
            .repos
            .agent_frame_repo
            .get(binding.launch_frame.frame_id)
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?
            .ok_or_else(|| {
                AgentToolError::ExecutionFailed(format!(
                    "Companion Product binding AgentFrame {} does not exist",
                    binding.launch_frame.frame_id
                ))
            })?;
        if frame.agent_id != target.agent_id
            || u64::try_from(frame.revision).ok() != Some(binding.launch_frame.revision)
        {
            return Err(AgentToolError::ExecutionFailed(
                "Companion Product binding does not identify the immutable AgentFrame revision"
                    .to_string(),
            ));
        }
        let capability = frame.typed_capability_state().ok_or_else(|| {
            AgentToolError::ExecutionFailed(
                "Companion Product binding AgentFrame has no typed capability surface".to_string(),
            )
        })?;
        let hook_plan = frame
            .validated_hook_plan()
            .map_err(AgentToolError::ExecutionFailed)?;
        let hook_runtime = Arc::new(
            ProductCompanionHookRuntime::load(
                request.context.runtime_thread_id.to_string(),
                HookControlTarget {
                    run_id: target.run_id,
                    agent_id: target.agent_id,
                    frame_id: frame.id,
                },
                request.context.turn_id.clone(),
                hook_plan.requirements,
                self.deps.hook_provider.clone(),
            )
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?,
        );
        let owner = PlatformToolExecutionContext {
            run_id: target.run_id,
            project_id: request.context.target.project_id,
            agent_id: target.agent_id,
            frame_id: frame.id,
            runtime_thread_id: request.context.runtime_thread_id.clone(),
            invocation: None,
            launch_evidence_frame_id: binding.launch_frame.frame_id,
            current_surface_frame_id: frame.id,
            orchestration_id: None,
            node_path: None,
            node_attempt: None,
        };
        let tool_context = CompanionToolContext::from_product_runtime(
            request.context.runtime_thread_id,
            request.context.turn_id,
            owner,
            hook_runtime,
        );
        let services_handle = SharedRuntimeThreadToolServicesHandle::default();
        services_handle
            .set(self.deps.runtime_thread_services.clone())
            .await;
        match self.kind {
            ProductRuntimeToolKind::CompanionRequest => {
                CompanionRequestTool::new(CompanionRequestToolDeps {
                    project_agent_repo: self.deps.repos.project_agent_repo.clone(),
                    repos: self.deps.repos.clone(),
                    runtime_thread_services_handle: services_handle,
                    tool_context,
                    companion_agents: capability.companion.agents,
                    wait_service: self.deps.wait_service.clone(),
                    model_preflight: self.deps.model_preflight.clone(),
                    workflow_script_preflight: self.deps.workflow_script_preflight.clone(),
                    product_effect_id: Some(request.context.effect_id.clone()),
                })
                .execute(
                    &request.context.invocation_id,
                    request.arguments,
                    CancellationToken::new(),
                    None,
                )
                .await
            }
            ProductRuntimeToolKind::CompanionRespond => {
                CompanionRespondTool::new(self.deps.repos.clone(), services_handle, tool_context)
                    .execute(
                        &request.context.invocation_id,
                        request.arguments,
                        CancellationToken::new(),
                        None,
                    )
                    .await
            }
            _ => unreachable!("constructor fences Companion Product tool kinds"),
        }
    }
}

#[async_trait]
impl ProductRuntimeToolService for ApplicationCompanionRuntimeToolService {
    fn kind(&self) -> ProductRuntimeToolKind {
        self.kind
    }

    fn parameters_schema(&self) -> serde_json::Value {
        match self.kind {
            ProductRuntimeToolKind::CompanionRequest => companion_request_parameters_schema(),
            ProductRuntimeToolKind::CompanionRespond => companion_respond_parameters_schema(),
            _ => unreachable!("constructor fences Companion Product tool kinds"),
        }
    }

    async fn execute(&self, request: ProductRuntimeToolRequest) -> ProductRuntimeToolOutcome {
        match self.execute_tool(request).await {
            Ok(result) if result.is_error => ProductRuntimeToolOutcome::Rejected {
                code: "companion_request_rejected".to_string(),
                message: result
                    .content
                    .iter()
                    .filter_map(|part| part.extract_text())
                    .collect::<Vec<_>>()
                    .join("\n"),
            },
            Ok(result) => ProductRuntimeToolOutcome::Completed {
                output: serde_json::to_value(result).unwrap_or_else(|error| {
                    serde_json::json!({
                        "serialization_error": error.to_string(),
                    })
                }),
            },
            Err(AgentToolError::InvalidArguments(message)) => ProductRuntimeToolOutcome::Rejected {
                code: "companion_invalid_arguments".to_string(),
                message,
            },
            Err(error) => ProductRuntimeToolOutcome::Failed {
                code: "companion_execution_failed".to_string(),
                message: error.to_string(),
            },
        }
    }
}

struct ProductCompanionHookRuntimeState {
    snapshot: AgentFrameHookSnapshot,
    revision: u64,
    diagnostics: Vec<HookDiagnosticEntry>,
    trace: Vec<HookTraceEntry>,
    pending_actions: Vec<HookPendingAction>,
    turn_start_notices: Vec<HookTurnStartNotice>,
    token_stats: ContextTokenStats,
    capabilities: BTreeSet<String>,
    next_trace_sequence: u64,
}

struct ProductCompanionHookRuntime {
    runtime_thread_id: String,
    target: HookControlTarget,
    requirements: Vec<AgentFrameHookRequirement>,
    provider: Arc<AppExecutionHookProvider>,
    state: Mutex<ProductCompanionHookRuntimeState>,
}

impl fmt::Debug for ProductCompanionHookRuntime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProductCompanionHookRuntime")
            .field("runtime_thread_id", &self.runtime_thread_id)
            .field("target", &self.target)
            .finish_non_exhaustive()
    }
}

impl ProductCompanionHookRuntime {
    async fn load(
        runtime_thread_id: String,
        target: HookControlTarget,
        turn_id: String,
        requirements: Vec<AgentFrameHookRequirement>,
        provider: Arc<AppExecutionHookProvider>,
    ) -> Result<Self, HookError> {
        let snapshot = provider
            .load_product_hook_snapshot(AgentFrameHookSnapshotQuery {
                target: target.clone(),
                provenance: RuntimeAdapterProvenance::runtime_thread(
                    runtime_thread_id.clone(),
                    Some(turn_id),
                    "companion_product_tool_snapshot",
                ),
            })
            .await?;
        let diagnostics = snapshot.diagnostics.clone();
        Ok(Self {
            runtime_thread_id,
            target,
            requirements,
            provider,
            state: Mutex::new(ProductCompanionHookRuntimeState {
                snapshot,
                revision: 1,
                diagnostics,
                trace: Vec::new(),
                pending_actions: Vec::new(),
                turn_start_notices: Vec::new(),
                token_stats: ContextTokenStats::default(),
                capabilities: BTreeSet::new(),
                next_trace_sequence: 1,
            }),
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, ProductCompanionHookRuntimeState> {
        self.state
            .lock()
            .expect("Product Companion Hook Runtime mutex poisoned")
    }
}

#[async_trait]
impl HookRuntimeAccess for ProductCompanionHookRuntime {
    fn session_id(&self) -> &str {
        &self.runtime_thread_id
    }

    fn control_target(&self) -> HookControlTarget {
        self.target.clone()
    }

    fn snapshot(&self) -> AgentFrameHookSnapshot {
        self.lock().snapshot.clone()
    }

    fn diagnostics(&self) -> Vec<HookDiagnosticEntry> {
        self.lock().diagnostics.clone()
    }

    fn revision(&self) -> u64 {
        self.lock().revision
    }

    fn trace(&self) -> Vec<HookTraceEntry> {
        self.lock().trace.clone()
    }

    fn pending_actions(&self) -> Vec<HookPendingAction> {
        self.lock().pending_actions.clone()
    }

    fn runtime_snapshot(&self) -> AgentFrameRuntimeSnapshot {
        let state = self.lock();
        AgentFrameRuntimeSnapshot {
            runtime_adapter_runtime_thread_id: self.runtime_thread_id.clone(),
            revision: state.revision,
            snapshot: state.snapshot.clone(),
            diagnostics: state.diagnostics.clone(),
            trace: state.trace.clone(),
            pending_actions: state.pending_actions.clone(),
        }
    }

    async fn refresh_from_provenance(
        &self,
        query: HookRuntimeRefreshQuery,
    ) -> Result<AgentFrameHookSnapshot, HookError> {
        let snapshot = self
            .provider
            .load_product_hook_snapshot(AgentFrameHookSnapshotQuery {
                target: self.target.clone(),
                provenance: query.provenance,
            })
            .await?;
        self.replace_snapshot(snapshot.clone());
        Ok(snapshot)
    }

    async fn evaluate_from_provenance(
        &self,
        query: HookRuntimeEvaluationQuery,
    ) -> Result<agentdash_platform_spi::HookResolution, HookError> {
        self.provider
            .evaluate_product_hook_event(
                &self.requirements,
                AgentFrameHookEvaluationQuery {
                    target: self.target.clone(),
                    provenance: query.provenance,
                    trigger: query.trigger,
                    tool_name: query.tool_name,
                    tool_call_id: query.tool_call_id,
                    subagent_type: query.subagent_type,
                    snapshot: query.snapshot,
                    payload: query.payload,
                    token_stats: query.token_stats,
                },
            )
            .await
    }

    fn replace_snapshot(&self, snapshot: AgentFrameHookSnapshot) {
        let mut state = self.lock();
        state.diagnostics = snapshot.diagnostics.clone();
        state.snapshot = snapshot;
        state.revision += 1;
    }

    fn append_diagnostics_vec(&self, entries: Vec<HookDiagnosticEntry>) {
        self.lock().diagnostics.extend(entries);
    }

    fn append_trace(&self, trace: HookTraceEntry) {
        self.lock().trace.push(trace);
    }

    fn next_trace_sequence(&self) -> u64 {
        let mut state = self.lock();
        let sequence = state.next_trace_sequence;
        state.next_trace_sequence += 1;
        sequence
    }

    fn enqueue_pending_action(&self, action: HookPendingAction) {
        self.lock().pending_actions.push(action);
    }

    fn collect_pending_actions_for_injection(&self) -> Vec<HookPendingAction> {
        self.unresolved_pending_actions()
    }

    fn enqueue_turn_start_notice(&self, notice: HookTurnStartNotice) {
        self.lock().turn_start_notices.push(notice);
    }

    fn collect_turn_start_notices_for_injection(&self) -> Vec<HookTurnStartNotice> {
        std::mem::take(&mut self.lock().turn_start_notices)
    }

    fn peek_turn_start_notices(&self) -> Vec<HookTurnStartNotice> {
        self.lock().turn_start_notices.clone()
    }

    fn acknowledge_turn_start_notices(&self, notice_ids: &[String]) {
        self.lock()
            .turn_start_notices
            .retain(|notice| !notice_ids.contains(&notice.id));
    }

    fn unresolved_pending_actions(&self) -> Vec<HookPendingAction> {
        self.lock()
            .pending_actions
            .iter()
            .filter(|action| action.status == HookPendingActionStatus::Pending)
            .cloned()
            .collect()
    }

    fn unresolved_blocking_actions(&self) -> Vec<HookPendingAction> {
        self.unresolved_pending_actions()
            .into_iter()
            .filter(HookPendingAction::is_blocking)
            .collect()
    }

    fn resolve_pending_action(
        &self,
        action_id: &str,
        resolution_kind: HookPendingActionResolutionKind,
        note: Option<String>,
        turn_id: Option<String>,
    ) -> Option<HookPendingAction> {
        let mut state = self.lock();
        let action = state
            .pending_actions
            .iter_mut()
            .find(|action| action.id == action_id)?;
        action.status = HookPendingActionStatus::Resolved;
        action.resolution_kind = Some(resolution_kind);
        action.resolution_note = note;
        action.resolution_turn_id = turn_id;
        action.resolved_at_ms = Some(chrono::Utc::now().timestamp_millis());
        Some(action.clone())
    }

    fn update_token_stats(&self, stats: ContextTokenStats) {
        self.lock().token_stats = stats;
    }

    fn token_stats(&self) -> ContextTokenStats {
        self.lock().token_stats.clone()
    }

    fn current_capabilities(&self) -> BTreeSet<String> {
        self.lock().capabilities.clone()
    }

    fn update_capabilities(&self, new_caps: BTreeSet<String>) -> Option<SetDelta> {
        let mut state = self.lock();
        let added = new_caps
            .difference(&state.capabilities)
            .cloned()
            .collect::<Vec<_>>();
        let removed = state
            .capabilities
            .difference(&new_caps)
            .cloned()
            .collect::<Vec<_>>();
        if added.is_empty() && removed.is_empty() {
            return None;
        }
        state.capabilities = new_caps;
        Some(SetDelta { added, removed })
    }

    fn subscribe_traces(&self) -> Option<broadcast::Receiver<HookTraceEntry>> {
        None
    }
}
