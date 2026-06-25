//! `SessionRuntimeInner` 行为测试（从原 `hub.rs` 迁移；PR 6 拆分）。
#![allow(deprecated)]

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use agentdash_agent_protocol::codex_app_server_protocol as codex;
use agentdash_agent_protocol::{
    BackboneEnvelope, BackboneEvent, ItemCompletedNotification, ItemStartedNotification,
    PlatformEvent, SourceInfo, TraceInfo, UserInputSubmissionKind,
};
use agentdash_domain::DomainError;
use agentdash_domain::permission::{
    GrantScope, GrantStatus, PermissionGrant, PermissionGrantRepository,
    PermissionGrantStatusFilter, PolicyDecision, PolicyOutcome,
};
use agentdash_domain::workflow::{
    AgentFrame, AgentSource, LifecycleAgent, LifecycleAgentRepository, LifecycleRun,
    LifecycleRunRepository, RuntimeSessionExecutionAnchor, RuntimeSessionExecutionAnchorRepository,
};
use agentdash_spi::hooks::{
    ActiveWorkflowMeta, AgentFrameHookEvaluationQuery, AgentFrameHookRefreshQuery,
    AgentFrameHookSnapshot, AgentFrameHookSnapshotQuery, ContextFrame, ContextFrameSection,
    ExecutionHookProvider, HookControlTarget, HookEvaluationQuery, HookInjection, HookResolution,
    HookTraceTrigger, HookTrigger, RuntimeEventSource, SessionSnapshotMetadata, SharedHookRuntime,
};
use agentdash_spi::{
    AgentConfig, AgentConnector, AgentTool, AgentToolError, AgentToolResult, CapabilityState,
    CompactionProjectionCommitResult, ConnectorError, ExecutionSessionFrame, MessageRef,
    NewCompactionProjectionCommit, ProjectionOrigin, PromptPayload, RuntimeBackendAnchor,
    RuntimeBackendAnchorSource, SESSION_PROJECTION_KIND_MODEL_CONTEXT, SessionCompactionRecord,
    SessionCompactionStatus, SessionProjectionHeadRecord, SessionProjectionSegmentRecord,
    StopReason, ToolUpdateCallback,
};
use chrono::{DateTime, Utc};
use futures::stream;
use serde_json::json;
use tokio::sync::Mutex as TokioMutex;

use agentdash_application_ports::runtime_surface_adoption::AgentFrameRuntimeTarget;

use super::super::MemorySessionPersistence;
use super::super::construction::{ConstructionResolutionPlan, RuntimeContextInspectionPlan};
use super::super::hook_messages as msg;
use super::super::hub_support::{TurnExecution, TurnState, build_user_input_submitted_envelope};
use super::super::local_workspace_vfs;
use super::super::types::{
    BackendSelectionInput, BackendSelectionInputMode, EFFECT_TYPE_APPLY_VFS_OVERLAY,
    PendingCapabilityStateTransition, RuntimeCapabilityTransition, UserPromptInput,
};
use super::super::{
    AgentFrameTransitionRecord, RuntimeCommandStatus, RuntimeDeliveryCommand,
    SessionToolResultCache,
};
use super::{PendingRuntimeContextTransitionInput, SessionRuntimeInner};
use crate::agent_run::frame::surface::FrameSurfaceDraft;
use crate::agent_run::runtime_capability::{
    CompanionCapabilityDimensionModule, McpCapabilityDimensionModule,
    ToolCapabilityDimensionModule, VfsCapabilityDimensionModule,
};
use crate::session::SetToolAccessEffect;
use crate::test_support::{
    AgentRunSteeringCommand, AgentRunSteeringService, MemoryAgentFrameRepository,
    MemoryLifecycleAgentRepository, MemoryLifecycleGateRepository,
    MemoryRuntimeSessionExecutionAnchorRepository,
};
use crate::vfs::{
    ExecRequest, ExecResult, ListOptions, ListResult, MountError, MountOperationContext,
    MountProvider, MountProviderRegistry, ReadResult, RuntimeFileEntry, SearchQuery, SearchResult,
    VfsService,
};
use agentdash_application_ports::frame_launch_envelope::{
    FrameLaunchEnvelope, FrameLaunchEnvelopePort, FrameLaunchEnvelopeRequest, FrameLaunchModifier,
};
use agentdash_application_ports::mcp_discovery::{
    DiscoveredMcpTool, McpToolDiscovery, McpToolDiscoveryRequest,
};

fn local_runtime_backend_anchor(root: &std::path::Path) -> RuntimeBackendAnchor {
    RuntimeBackendAnchor::new("local", RuntimeBackendAnchorSource::System)
        .expect("local runtime backend anchor")
        .with_root_ref(Some(root.to_string_lossy().to_string()))
}

fn test_hub(
    _mount_root: PathBuf,
    connector: Arc<dyn AgentConnector>,
    hook_provider: Option<Arc<dyn ExecutionHookProvider>>,
) -> SessionRuntimeInner {
    let frame_repo = Arc::new(MemoryAgentFrameRepository::default());
    let gate_repo = Arc::new(MemoryLifecycleGateRepository::default());
    let anchor_repo = Arc::new(MemoryRuntimeSessionExecutionAnchorRepository::default());
    SessionRuntimeInner::new_with_hooks_and_persistence(
        connector,
        hook_provider,
        Arc::new(MemorySessionPersistence::default()),
    )
    .with_agent_frame_repo(frame_repo)
    .with_lifecycle_gate_repo(gate_repo)
    .with_execution_anchor_repo(anchor_repo)
}

async fn attach_test_frame(hub: &SessionRuntimeInner, session_id: &str) -> AgentFrame {
    let frame = AgentFrame::new_initial(uuid::Uuid::new_v4());
    hub.agent_frame_repo
        .as_ref()
        .expect("test hub should provide AgentFrameRepository")
        .create(&frame)
        .await
        .expect("test frame should persist");
    if let Some(anchor_repo) = hub.execution_anchor_repo.as_ref() {
        anchor_repo
            .upsert(&RuntimeSessionExecutionAnchor::new_dispatch(
                session_id,
                uuid::Uuid::new_v4(),
                frame.id,
                frame.agent_id,
            ))
            .await
            .expect("test runtime anchor should persist");
    }
    frame
}

async fn attach_test_lifecycle_frame(hub: &SessionRuntimeInner, session_id: &str) -> AgentFrame {
    let frame = attach_test_frame(hub, session_id).await;
    let anchor = hub
        .execution_anchor_repo
        .as_ref()
        .expect("test hub should provide RuntimeSessionExecutionAnchorRepository")
        .find_by_session(session_id)
        .await
        .expect("anchor lookup should succeed")
        .expect("test frame should attach runtime anchor");
    let mut agent =
        LifecycleAgent::new_root(anchor.run_id, uuid::Uuid::new_v4(), AgentSource::Unknown)
            .with_bootstrap_status("not_applicable");
    agent.id = frame.agent_id;
    hub.lifecycle_agent_repo
        .as_ref()
        .expect("test hub should provide LifecycleAgentRepository")
        .create(&agent)
        .await
        .expect("test lifecycle agent should persist");
    frame
}

async fn current_frame_id(hub: &SessionRuntimeInner, agent_id: uuid::Uuid) -> Option<uuid::Uuid> {
    hub.agent_frame_repo
        .as_ref()
        .expect("test hub should provide AgentFrameRepository")
        .get_current(agent_id)
        .await
        .expect("frame lookup should succeed")
        .map(|frame| frame.id)
}

#[derive(Default)]
struct InMemoryLifecycleRunRepo {
    items: TokioMutex<Vec<LifecycleRun>>,
}

#[derive(Default)]
struct InMemoryPermissionGrantRepo {
    items: TokioMutex<Vec<PermissionGrant>>,
}

#[async_trait::async_trait]
impl PermissionGrantRepository for InMemoryPermissionGrantRepo {
    async fn create(&self, grant: &PermissionGrant) -> Result<(), DomainError> {
        self.items.lock().await.push(grant.clone());
        Ok(())
    }

    async fn update(&self, grant: &PermissionGrant) -> Result<(), DomainError> {
        let mut items = self.items.lock().await;
        if let Some(existing) = items.iter_mut().find(|item| item.id == grant.id) {
            *existing = grant.clone();
        } else {
            items.push(grant.clone());
        }
        Ok(())
    }

    async fn find_by_id(&self, id: uuid::Uuid) -> Result<Option<PermissionGrant>, DomainError> {
        Ok(self
            .items
            .lock()
            .await
            .iter()
            .find(|grant| grant.id == id)
            .cloned())
    }

    async fn list_by_frame(
        &self,
        effect_frame_id: uuid::Uuid,
        status_filter: Option<PermissionGrantStatusFilter>,
    ) -> Result<Vec<PermissionGrant>, DomainError> {
        Ok(self
            .items
            .lock()
            .await
            .iter()
            .filter(|grant| grant.effect_frame_id == Some(effect_frame_id))
            .filter(|grant| permission_grant_status_matches(grant.status, status_filter))
            .cloned()
            .collect())
    }

    async fn list_by_run(
        &self,
        run_id: uuid::Uuid,
        status_filter: Option<PermissionGrantStatusFilter>,
    ) -> Result<Vec<PermissionGrant>, DomainError> {
        Ok(self
            .items
            .lock()
            .await
            .iter()
            .filter(|grant| grant.run_id == run_id)
            .filter(|grant| permission_grant_status_matches(grant.status, status_filter))
            .cloned()
            .collect())
    }

    async fn list_active_by_frame(
        &self,
        effect_frame_id: uuid::Uuid,
    ) -> Result<Vec<PermissionGrant>, DomainError> {
        self.list_by_frame(effect_frame_id, Some(PermissionGrantStatusFilter::Active))
            .await
    }

    async fn list_active_by_run(
        &self,
        run_id: uuid::Uuid,
    ) -> Result<Vec<PermissionGrant>, DomainError> {
        self.list_by_run(run_id, Some(PermissionGrantStatusFilter::Active))
            .await
    }

    async fn find_active_escalation_grant(
        &self,
        effect_frame_id: uuid::Uuid,
        target_subject_kind: &str,
    ) -> Result<Option<PermissionGrant>, DomainError> {
        Ok(self
            .items
            .lock()
            .await
            .iter()
            .find(|grant| {
                grant.effect_frame_id == Some(effect_frame_id)
                    && grant.status == GrantStatus::Applied
                    && grant
                        .scope_escalation_intent
                        .as_ref()
                        .is_some_and(|intent| intent.target_subject_kind == target_subject_kind)
            })
            .cloned())
    }

    async fn list_overdue_active(
        &self,
        now: DateTime<Utc>,
    ) -> Result<Vec<PermissionGrant>, DomainError> {
        Ok(self
            .items
            .lock()
            .await
            .iter()
            .filter(|grant| grant.status.is_active())
            .filter(|grant| grant.expires_at.is_some_and(|expires_at| expires_at < now))
            .cloned()
            .collect())
    }
}

fn permission_grant_status_matches(
    status: GrantStatus,
    status_filter: Option<PermissionGrantStatusFilter>,
) -> bool {
    match status_filter {
        Some(PermissionGrantStatusFilter::Exact(expected)) => status == expected,
        Some(PermissionGrantStatusFilter::Pending) => matches!(
            status,
            GrantStatus::Created
                | GrantStatus::PendingPolicy
                | GrantStatus::PendingUserApproval
                | GrantStatus::Approved
        ),
        Some(PermissionGrantStatusFilter::Active) => status.is_active(),
        Some(PermissionGrantStatusFilter::Terminal) => status.is_terminal(),
        None => true,
    }
}

#[async_trait::async_trait]
impl LifecycleRunRepository for InMemoryLifecycleRunRepo {
    async fn create(&self, run: &LifecycleRun) -> Result<(), DomainError> {
        self.items.lock().await.push(run.clone());
        Ok(())
    }

    async fn get_by_id(&self, id: uuid::Uuid) -> Result<Option<LifecycleRun>, DomainError> {
        Ok(self
            .items
            .lock()
            .await
            .iter()
            .find(|run| run.id == id)
            .cloned())
    }

    async fn list_by_ids(&self, ids: &[uuid::Uuid]) -> Result<Vec<LifecycleRun>, DomainError> {
        Ok(self
            .items
            .lock()
            .await
            .iter()
            .filter(|run| ids.contains(&run.id))
            .cloned()
            .collect())
    }

    async fn list_by_project(
        &self,
        project_id: uuid::Uuid,
    ) -> Result<Vec<LifecycleRun>, DomainError> {
        Ok(self
            .items
            .lock()
            .await
            .iter()
            .filter(|run| run.project_id == project_id)
            .cloned()
            .collect())
    }

    async fn update(&self, run: &LifecycleRun) -> Result<(), DomainError> {
        let mut items = self.items.lock().await;
        if let Some(existing) = items.iter_mut().find(|existing| existing.id == run.id) {
            *existing = run.clone();
        } else {
            items.push(run.clone());
        }
        Ok(())
    }

    async fn delete(&self, id: uuid::Uuid) -> Result<(), DomainError> {
        self.items.lock().await.retain(|run| run.id != id);
        Ok(())
    }
}

async fn control_target_from_anchor_frame(
    hub: &SessionRuntimeInner,
    session_id: &str,
    frame: &AgentFrame,
) -> HookControlTarget {
    let anchor = hub
        .execution_anchor_repo
        .as_ref()
        .expect("test hub should provide RuntimeSessionExecutionAnchorRepository")
        .find_by_session(session_id)
        .await
        .expect("anchor lookup should succeed")
        .expect("test frame should attach runtime anchor");
    HookControlTarget {
        run_id: anchor.run_id,
        agent_id: frame.agent_id,
        frame_id: frame.id,
    }
}

async fn ensure_hook_runtime_for_frame(
    hub: &SessionRuntimeInner,
    session_id: &str,
    frame_id: uuid::Uuid,
    turn_id: &str,
) -> SharedHookRuntime {
    hub.hook_service()
        .ensure_hook_runtime_for_target(
            &AgentFrameRuntimeTarget {
                frame_id,
                delivery_runtime_session_id: session_id.to_string(),
            },
            Some(turn_id),
        )
        .await
        .expect("target-first hook runtime ensure should succeed")
        .expect("hook runtime should exist for target")
}

fn test_hook_snapshot(session_id: String) -> AgentFrameHookSnapshot {
    AgentFrameHookSnapshot {
        runtime_adapter_session_id: session_id,
        metadata: Some(SessionSnapshotMetadata {
            active_workflow: Some(ActiveWorkflowMeta {
                run_id: Some(uuid::Uuid::new_v4()),
                ..ActiveWorkflowMeta::default()
            }),
            ..SessionSnapshotMetadata::default()
        }),
        ..AgentFrameHookSnapshot::default()
    }
}

fn runtime_transition_from_state(
    state: &CapabilityState,
    vfs_overlay: Option<agentdash_spi::Vfs>,
) -> RuntimeCapabilityTransition {
    let mut effects = vec![
        ToolCapabilityDimensionModule::set_tool_access_effect(SetToolAccessEffect {
            capabilities: state.tool.capabilities.clone(),
            enabled_clusters: state.tool.enabled_clusters.clone(),
            tool_policy: state.tool.tool_policy.clone(),
        })
        .expect("tool effect builds"),
        McpCapabilityDimensionModule::set_server_set_effect(state.tool.mcp_servers.clone())
            .expect("mcp effect builds"),
        CompanionCapabilityDimensionModule::set_agent_roster_effect(state.companion.agents.clone())
            .expect("companion effect builds"),
    ];
    if let Some(overlay) = vfs_overlay {
        effects.push(
            VfsCapabilityDimensionModule::apply_vfs_overlay_effect(overlay)
                .expect("vfs overlay effect builds"),
        );
    }
    RuntimeCapabilityTransition::from_records(Vec::new(), effects)
}

fn simple_prompt_request(prompt: &str) -> RuntimeContextInspectionPlan {
    let user_input = UserPromptInput {
        executor_config: Some(agentdash_spi::AgentConfig::new("PI_AGENT")),
        ..UserPromptInput::from_text(prompt)
    };
    let executor_config = user_input.executor_config.clone();
    let owner = crate::session::construction::ResolvedSessionOwner {
        owner_type: agentdash_spi::CapabilityScope::Project,
        project_id: Some(uuid::Uuid::new_v4()),
        trace: crate::session::construction::OwnerResolutionTrace {
            selected_reason: "test".to_string(),
        },
    };
    let mut construction =
        RuntimeContextInspectionPlan::from_source_input("test-session", owner, &user_input);
    let root = std::env::current_dir().expect("current dir");
    let vfs = local_workspace_vfs(&root);
    let mut capability_state = CapabilityState::default();
    capability_state.vfs.active = Some(vfs.clone());
    construction.workspace.working_directory = Some(root);
    construction.surface.vfs = Some(vfs.clone());
    construction.projections.frame_surface_draft = Some(FrameSurfaceDraft {
        capability_state: Some(capability_state),
        vfs: Some(vfs),
        mcp_servers: Vec::new(),
        context_bundle_summary: None,
        execution_profile: executor_config,
    });
    construction.resolution = ConstructionResolutionPlan {
        vfs_source: Some("test.local_workspace_vfs".to_string()),
        mcp_source: Some("test.empty".to_string()),
        capability_source: Some("test.capability_state".to_string()),
        executor_source: Some("test.executor_config".to_string()),
        working_directory_source: Some("test.current_dir".to_string()),
        pending_overlay_applied: false,
        runtime_base_capability_state: None,
    };
    construction
}

fn owner_bootstrap_request(prompt: &str, system_context: &str) -> RuntimeContextInspectionPlan {
    let mut construction = simple_prompt_request(prompt);
    let bundle_session_id = uuid::Uuid::new_v4();
    let bundle = crate::context::build_continuation_bundle_from_markdown(
        bundle_session_id,
        system_context.to_string(),
    );
    construction.context.bundle_id = Some(bundle.bundle_id);
    construction.context.bootstrap_fragment_count = bundle.bootstrap_fragments.len();
    construction.context.bundle = Some(bundle);
    construction
}

#[derive(Default)]
struct EmptyConnector;

#[async_trait::async_trait]
impl AgentConnector for EmptyConnector {
    fn connector_id(&self) -> &'static str {
        "empty"
    }
    fn connector_type(&self) -> agentdash_spi::ConnectorType {
        agentdash_spi::ConnectorType::LocalExecutor
    }
    fn capabilities(&self) -> agentdash_spi::ConnectorCapabilities {
        agentdash_spi::ConnectorCapabilities::default()
    }
    fn list_executors(&self) -> Vec<agentdash_spi::AgentInfo> {
        Vec::new()
    }
    async fn discover_options_stream(
        &self,
        _executor: &str,
        _working_dir: Option<PathBuf>,
    ) -> Result<futures::stream::BoxStream<'static, json_patch::Patch>, ConnectorError> {
        Ok(Box::pin(stream::empty()))
    }
    async fn prompt(
        &self,
        _session_id: &str,
        _follow_up_session_id: Option<&str>,
        _prompt: &PromptPayload,
        _context: agentdash_spi::ExecutionContext,
    ) -> Result<agentdash_spi::ExecutionStream, ConnectorError> {
        Ok(Box::pin(stream::empty()))
    }
    async fn cancel(&self, _session_id: &str) -> Result<(), ConnectorError> {
        Ok(())
    }
    async fn approve_tool_call(
        &self,
        _session_id: &str,
        _tool_call_id: &str,
    ) -> Result<(), ConnectorError> {
        Ok(())
    }
    async fn reject_tool_call(
        &self,
        _session_id: &str,
        _tool_call_id: &str,
        _reason: Option<String>,
    ) -> Result<(), ConnectorError> {
        Ok(())
    }
}

struct PromptCountingConnector {
    prompt_calls: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl AgentConnector for PromptCountingConnector {
    fn connector_id(&self) -> &'static str {
        "prompt-counting"
    }
    fn connector_type(&self) -> agentdash_spi::ConnectorType {
        agentdash_spi::ConnectorType::LocalExecutor
    }
    fn capabilities(&self) -> agentdash_spi::ConnectorCapabilities {
        agentdash_spi::ConnectorCapabilities::default()
    }
    fn list_executors(&self) -> Vec<agentdash_spi::AgentInfo> {
        Vec::new()
    }
    async fn discover_options_stream(
        &self,
        _executor: &str,
        _working_dir: Option<PathBuf>,
    ) -> Result<futures::stream::BoxStream<'static, json_patch::Patch>, ConnectorError> {
        Ok(Box::pin(stream::empty()))
    }
    async fn prompt(
        &self,
        _session_id: &str,
        _follow_up_session_id: Option<&str>,
        _prompt: &PromptPayload,
        _context: agentdash_spi::ExecutionContext,
    ) -> Result<agentdash_spi::ExecutionStream, ConnectorError> {
        self.prompt_calls.fetch_add(1, Ordering::SeqCst);
        Ok(Box::pin(stream::empty()))
    }
    async fn cancel(&self, _session_id: &str) -> Result<(), ConnectorError> {
        Ok(())
    }
    async fn approve_tool_call(
        &self,
        _session_id: &str,
        _tool_call_id: &str,
    ) -> Result<(), ConnectorError> {
        Ok(())
    }
    async fn reject_tool_call(
        &self,
        _session_id: &str,
        _tool_call_id: &str,
        _reason: Option<String>,
    ) -> Result<(), ConnectorError> {
        Ok(())
    }
}

#[derive(Default)]
struct SetupFailingConnector;

#[async_trait::async_trait]
impl AgentConnector for SetupFailingConnector {
    fn connector_id(&self) -> &'static str {
        "setup-failing"
    }
    fn connector_type(&self) -> agentdash_spi::ConnectorType {
        agentdash_spi::ConnectorType::LocalExecutor
    }
    fn capabilities(&self) -> agentdash_spi::ConnectorCapabilities {
        agentdash_spi::ConnectorCapabilities::default()
    }
    fn list_executors(&self) -> Vec<agentdash_spi::AgentInfo> {
        Vec::new()
    }
    async fn discover_options_stream(
        &self,
        _executor: &str,
        _working_dir: Option<PathBuf>,
    ) -> Result<futures::stream::BoxStream<'static, json_patch::Patch>, ConnectorError> {
        Ok(Box::pin(stream::empty()))
    }
    async fn prompt(
        &self,
        _session_id: &str,
        _follow_up_session_id: Option<&str>,
        _prompt: &PromptPayload,
        _context: agentdash_spi::ExecutionContext,
    ) -> Result<agentdash_spi::ExecutionStream, ConnectorError> {
        Err(ConnectorError::Runtime(
            "connector setup failed".to_string(),
        ))
    }
    async fn cancel(&self, _session_id: &str) -> Result<(), ConnectorError> {
        Ok(())
    }
    async fn approve_tool_call(
        &self,
        _session_id: &str,
        _tool_call_id: &str,
    ) -> Result<(), ConnectorError> {
        Ok(())
    }
    async fn reject_tool_call(
        &self,
        _session_id: &str,
        _tool_call_id: &str,
        _reason: Option<String>,
    ) -> Result<(), ConnectorError> {
        Ok(())
    }
}

#[derive(Clone)]
struct StaticMcpToolDiscovery {
    tools: Vec<agentdash_spi::RelayMcpToolInfo>,
}

#[derive(Clone)]
struct StaticMcpTool {
    name: String,
    description: String,
    parameters_schema: serde_json::Value,
}

#[async_trait::async_trait]
impl AgentTool for StaticMcpTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters_schema(&self) -> serde_json::Value {
        self.parameters_schema.clone()
    }

    async fn execute(
        &self,
        _tool_call_id: &str,
        _args: serde_json::Value,
        _cancel: tokio_util::sync::CancellationToken,
        _on_update: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        Ok(AgentToolResult {
            content: Vec::new(),
            is_error: false,
            details: None,
        })
    }
}

#[async_trait::async_trait]
impl McpToolDiscovery for StaticMcpToolDiscovery {
    async fn discover_tool_entries(
        &self,
        request: McpToolDiscoveryRequest,
    ) -> Result<Vec<DiscoveredMcpTool>, ConnectorError> {
        let requested_servers = request
            .servers
            .iter()
            .map(|server| server.name.as_str())
            .collect::<std::collections::HashSet<_>>();
        Ok(self
            .tools
            .iter()
            .filter(|info| requested_servers.contains(info.server_name.as_str()))
            .filter(|info| {
                request.capability_state.is_capability_tool_enabled(
                    "workflow_management",
                    &info.tool_name,
                    None,
                )
            })
            .map(|info| {
                let runtime_name = format!(
                    "mcp_agentdash_workflow_tools_{}",
                    info.tool_name.replace('-', "_")
                );
                let tool = Arc::new(StaticMcpTool {
                    name: runtime_name.clone(),
                    description: info.description.clone(),
                    parameters_schema: info.parameters_schema.clone(),
                }) as agentdash_agent_types::DynAgentTool;
                DiscoveredMcpTool {
                    runtime_name,
                    server_name: info.server_name.clone(),
                    tool_name: info.tool_name.clone(),
                    uses_relay: true,
                    description: info.description.clone(),
                    parameters_schema: info.parameters_schema.clone(),
                    tool,
                }
            })
            .collect())
    }
}

#[tokio::test]
async fn build_tools_filters_relay_mcp_with_initial_capability_state() {
    let base = tempfile::tempdir().expect("tempdir");
    let workflow_server = agentdash_spi::RuntimeMcpServer {
        name: "agentdash-workflow-tools-123".to_string(),
        transport: agentdash_spi::McpTransportConfig::Http {
            url: "http://relay/ignored".to_string(),
            headers: vec![],
        },
        uses_relay: true,
    };
    let discovery = Arc::new(StaticMcpToolDiscovery {
        tools: vec![
            agentdash_spi::RelayMcpToolInfo {
                server_name: workflow_server.name.clone(),
                server: workflow_server.clone(),
                tool_name: "list_workflows".to_string(),
                description: "list".to_string(),
                parameters_schema: json!({ "type": "object" }),
            },
            agentdash_spi::RelayMcpToolInfo {
                server_name: workflow_server.name.clone(),
                server: workflow_server.clone(),
                tool_name: "upsert_workflow_tool".to_string(),
                description: "upsert".to_string(),
                parameters_schema: json!({ "type": "object" }),
            },
            agentdash_spi::RelayMcpToolInfo {
                server_name: workflow_server.name.clone(),
                server: workflow_server.clone(),
                tool_name: "upsert_lifecycle_tool".to_string(),
                description: "upsert lifecycle".to_string(),
                parameters_schema: json!({ "type": "object" }),
            },
        ],
    });
    let hub = test_hub(base.path().to_path_buf(), Arc::new(EmptyConnector), None)
        .with_mcp_tool_discovery(discovery);

    let mut plan_state = CapabilityState::default();
    plan_state
        .tool
        .capabilities
        .insert(agentdash_spi::ToolCapability::new("workflow_management"));
    plan_state
        .tool
        .tool_policy
        .entry("workflow_management".to_string())
        .or_default()
        .exclude
        .insert("upsert_workflow_tool".to_string());
    plan_state
        .tool
        .tool_policy
        .entry("workflow_management".to_string())
        .or_default()
        .exclude
        .insert("upsert_lifecycle_tool".to_string());
    plan_state.tool.mcp_servers = vec![workflow_server.clone()];

    let plan_context = agentdash_spi::ExecutionContext {
        session: ExecutionSessionFrame {
            turn_id: "turn-initial-tools".to_string(),
            working_directory: base.path().to_path_buf(),
            environment_variables: HashMap::new(),
            executor_config: AgentConfig::new("PI_AGENT"),
            mcp_servers: vec![workflow_server.clone()],
            vfs: Some(local_workspace_vfs(base.path())),
            backend_execution: None,
            runtime_backend_anchor: Some(local_runtime_backend_anchor(base.path())),
            identity: None,
        },
        turn: agentdash_spi::ExecutionTurnFrame {
            capability_state: plan_state,
            ..Default::default()
        },
    };

    let plan_tools = hub
        .assemble_tools_for_execution_context("session-initial-tools", &plan_context)
        .await;
    let plan_names = plan_tools
        .iter()
        .map(|tool| tool.name())
        .collect::<Vec<_>>();

    assert_eq!(
        plan_names,
        vec!["mcp_agentdash_workflow_tools_list_workflows"],
        "Plan 初始化工具 schema 只能暴露 workflow 管理只读工具"
    );

    let mut apply_state = CapabilityState::default();
    apply_state
        .tool
        .capabilities
        .insert(agentdash_spi::ToolCapability::new("workflow_management"));
    apply_state.tool.mcp_servers = vec![workflow_server.clone()];
    let apply_context = agentdash_spi::ExecutionContext {
        turn: agentdash_spi::ExecutionTurnFrame {
            capability_state: apply_state,
            ..Default::default()
        },
        ..plan_context
    };

    let apply_tools = hub
        .assemble_tools_for_execution_context("session-initial-tools", &apply_context)
        .await;
    let apply_names = apply_tools
        .iter()
        .map(|tool| tool.name())
        .collect::<Vec<_>>();

    assert!(
        apply_names.contains(&"mcp_agentdash_workflow_tools_upsert_workflow_tool"),
        "Apply capability state 解除 tool_policy 后必须重新暴露 upsert_workflow_tool schema"
    );
    assert!(
        apply_names.contains(&"mcp_agentdash_workflow_tools_upsert_lifecycle_tool"),
        "Apply capability state 解除 tool_policy 后必须重新暴露 upsert_lifecycle_tool schema"
    );
}

#[tokio::test]
async fn build_tools_consumes_tool_level_grant_projection_from_agent_run() {
    let base = tempfile::tempdir().expect("tempdir");
    let workflow_server = agentdash_spi::RuntimeMcpServer {
        name: "agentdash-workflow-tools-123".to_string(),
        transport: agentdash_spi::McpTransportConfig::Http {
            url: "http://relay/ignored".to_string(),
            headers: vec![],
        },
        uses_relay: true,
    };
    let discovery = Arc::new(StaticMcpToolDiscovery {
        tools: vec![
            agentdash_spi::RelayMcpToolInfo {
                server_name: workflow_server.name.clone(),
                server: workflow_server.clone(),
                tool_name: "list_workflows".to_string(),
                description: "list".to_string(),
                parameters_schema: json!({ "type": "object" }),
            },
            agentdash_spi::RelayMcpToolInfo {
                server_name: workflow_server.name.clone(),
                server: workflow_server.clone(),
                tool_name: "upsert_workflow_tool".to_string(),
                description: "upsert".to_string(),
                parameters_schema: json!({ "type": "object" }),
            },
        ],
    });
    let grant_repo = Arc::new(InMemoryPermissionGrantRepo::default());
    let hub = test_hub(base.path().to_path_buf(), Arc::new(EmptyConnector), None)
        .with_lifecycle_agent_repo(Arc::new(MemoryLifecycleAgentRepository::default()))
        .with_permission_grant_repo(grant_repo.clone())
        .with_mcp_tool_discovery(discovery);
    let session = hub
        .create_session("grant-projected-tools")
        .await
        .expect("create");
    let frame = attach_test_lifecycle_frame(&hub, &session.id).await;
    let anchor = hub
        .execution_anchor_repo
        .as_ref()
        .expect("anchor repo")
        .find_by_session(&session.id)
        .await
        .expect("anchor lookup")
        .expect("anchor");
    let mut grant = PermissionGrant::new(
        anchor.run_id,
        &session.id,
        vec![
            agentdash_domain::workflow::ToolCapabilityPath::parse(
                "workflow_management::upsert_workflow_tool",
            )
            .expect("tool path"),
        ],
        "temporary workflow write admission",
        GrantScope::AgentFrame,
        None,
    )
    .with_effect_frame(frame.id);
    grant.submit_for_policy().expect("submit");
    grant
        .apply_policy_decision(PolicyDecision {
            outcome: PolicyOutcome::AutoApproved,
            matched_rules: vec![],
            reason: "auto".to_string(),
        })
        .expect("policy");
    grant.mark_applied().expect("applied");
    grant_repo.create(&grant).await.expect("persist grant");

    let mut state = CapabilityState::default();
    state.tool.mcp_servers = vec![workflow_server.clone()];
    let context = agentdash_spi::ExecutionContext {
        session: ExecutionSessionFrame {
            turn_id: "turn-grant-projected-tools".to_string(),
            working_directory: base.path().to_path_buf(),
            environment_variables: HashMap::new(),
            executor_config: AgentConfig::new("PI_AGENT"),
            mcp_servers: vec![workflow_server],
            vfs: Some(local_workspace_vfs(base.path())),
            backend_execution: None,
            runtime_backend_anchor: Some(local_runtime_backend_anchor(base.path())),
            identity: None,
        },
        turn: agentdash_spi::ExecutionTurnFrame {
            capability_state: state,
            ..Default::default()
        },
    };

    let tools = hub
        .assemble_tools_for_execution_context(&session.id, &context)
        .await;
    let names = tools.iter().map(|tool| tool.name()).collect::<Vec<_>>();

    assert_eq!(
        names,
        vec!["mcp_agentdash_workflow_tools_upsert_workflow_tool"],
        "tool-level Grant must be consumed by production tool assembly through AgentRun projection"
    );
}

#[derive(Default)]
struct PendingConnector;

#[async_trait::async_trait]
impl AgentConnector for PendingConnector {
    fn connector_id(&self) -> &'static str {
        "pending"
    }
    fn connector_type(&self) -> agentdash_spi::ConnectorType {
        agentdash_spi::ConnectorType::LocalExecutor
    }
    fn capabilities(&self) -> agentdash_spi::ConnectorCapabilities {
        agentdash_spi::ConnectorCapabilities::default()
    }
    fn list_executors(&self) -> Vec<agentdash_spi::AgentInfo> {
        Vec::new()
    }
    async fn discover_options_stream(
        &self,
        _executor: &str,
        _working_dir: Option<PathBuf>,
    ) -> Result<futures::stream::BoxStream<'static, json_patch::Patch>, ConnectorError> {
        Ok(Box::pin(stream::empty()))
    }
    async fn prompt(
        &self,
        _session_id: &str,
        _follow_up_session_id: Option<&str>,
        _prompt: &PromptPayload,
        _context: agentdash_spi::ExecutionContext,
    ) -> Result<agentdash_spi::ExecutionStream, ConnectorError> {
        Ok(Box::pin(stream::pending()))
    }
    async fn cancel(&self, _session_id: &str) -> Result<(), ConnectorError> {
        Ok(())
    }
    async fn approve_tool_call(
        &self,
        _session_id: &str,
        _tool_call_id: &str,
    ) -> Result<(), ConnectorError> {
        Ok(())
    }
    async fn reject_tool_call(
        &self,
        _session_id: &str,
        _tool_call_id: &str,
        _reason: Option<String>,
    ) -> Result<(), ConnectorError> {
        Ok(())
    }
}

#[derive(Default)]
struct SteerCapturingConnector {
    calls: Arc<TokioMutex<Vec<CapturedSteerCall>>>,
}

#[derive(Debug, Clone)]
struct CapturedSteerCall {
    session_id: String,
    expected_turn_id: String,
    input_text: Option<String>,
}

#[async_trait::async_trait]
impl AgentConnector for SteerCapturingConnector {
    fn connector_id(&self) -> &'static str {
        "steer-capturing"
    }
    fn connector_type(&self) -> agentdash_spi::ConnectorType {
        agentdash_spi::ConnectorType::LocalExecutor
    }
    fn capabilities(&self) -> agentdash_spi::ConnectorCapabilities {
        agentdash_spi::ConnectorCapabilities {
            supports_steering: true,
            ..agentdash_spi::ConnectorCapabilities::default()
        }
    }
    fn list_executors(&self) -> Vec<agentdash_spi::AgentInfo> {
        Vec::new()
    }
    async fn discover_options_stream(
        &self,
        _executor: &str,
        _working_dir: Option<PathBuf>,
    ) -> Result<futures::stream::BoxStream<'static, json_patch::Patch>, ConnectorError> {
        Ok(Box::pin(stream::empty()))
    }
    async fn has_live_session(&self, _session_id: &str) -> bool {
        true
    }
    async fn prompt(
        &self,
        _session_id: &str,
        _follow_up_session_id: Option<&str>,
        _prompt: &PromptPayload,
        _context: agentdash_spi::ExecutionContext,
    ) -> Result<agentdash_spi::ExecutionStream, ConnectorError> {
        Ok(Box::pin(stream::pending()))
    }
    async fn steer_session(
        &self,
        session_id: &str,
        expected_turn_id: &str,
        input: Vec<agentdash_agent_protocol::UserInputBlock>,
    ) -> Result<(), ConnectorError> {
        self.calls.lock().await.push(CapturedSteerCall {
            session_id: session_id.to_string(),
            expected_turn_id: expected_turn_id.to_string(),
            input_text: input
                .first()
                .and_then(agentdash_agent_protocol::user_input_text)
                .map(str::to_string),
        });
        Ok(())
    }
    async fn cancel(&self, _session_id: &str) -> Result<(), ConnectorError> {
        Ok(())
    }
    async fn approve_tool_call(
        &self,
        _session_id: &str,
        _tool_call_id: &str,
    ) -> Result<(), ConnectorError> {
        Ok(())
    }
    async fn reject_tool_call(
        &self,
        _session_id: &str,
        _tool_call_id: &str,
        _reason: Option<String>,
    ) -> Result<(), ConnectorError> {
        Ok(())
    }
}

async fn seed_refreshed_agent_run_for_command_test(
    hub: &SessionRuntimeInner,
    run_repo: &InMemoryLifecycleRunRepo,
    session_id: &str,
    workspace_root: &std::path::Path,
) -> (LifecycleRun, AgentFrame, AgentFrame) {
    let launch_frame = attach_test_lifecycle_frame(hub, session_id).await;
    let anchor = hub
        .execution_anchor_repo
        .as_ref()
        .expect("test hub should provide anchor repo")
        .find_by_session(session_id)
        .await
        .expect("anchor lookup should succeed")
        .expect("session should have anchor");
    let mut run = LifecycleRun::new_plain(uuid::Uuid::new_v4());
    run.id = anchor.run_id;
    run_repo.create(&run).await.expect("seed lifecycle run");

    let current_frame =
        AgentFrame::new_revision(launch_frame.agent_id, launch_frame.revision + 1, "current");
    hub.agent_frame_repo
        .as_ref()
        .expect("test hub should provide frame repo")
        .create(&current_frame)
        .await
        .expect("seed current frame");

    let _rx = hub.ensure_session(session_id).await;
    hub.runtime_registry
        .with_runtime_mut(session_id, |runtime| {
            let runtime = runtime.expect("session runtime should exist");
            runtime.turn_state = TurnState::Active(Box::new(TurnExecution::new(
                "turn-current".to_string(),
                ExecutionSessionFrame {
                    turn_id: "turn-current".to_string(),
                    working_directory: workspace_root.to_path_buf(),
                    environment_variables: HashMap::new(),
                    executor_config: AgentConfig::new("PI_AGENT"),
                    mcp_servers: vec![],
                    vfs: Some(local_workspace_vfs(workspace_root)),
                    backend_execution: None,
                    runtime_backend_anchor: None,
                    identity: None,
                },
                CapabilityState::default(),
                uuid::Uuid::new_v4(),
                uuid::Uuid::new_v4(),
            )));
        })
        .await;

    (run, launch_frame, current_frame)
}

#[tokio::test]
async fn agent_run_steer_uses_current_agent_frame_after_frame_refresh() {
    let base = tempfile::tempdir().expect("tempdir");
    let connector = Arc::new(SteerCapturingConnector::default());
    let calls = connector.calls.clone();
    let connector_for_hub: Arc<dyn AgentConnector> = connector;
    let hub = test_hub(base.path().to_path_buf(), connector_for_hub, None)
        .with_lifecycle_agent_repo(Arc::new(MemoryLifecycleAgentRepository::default()));
    let session = hub
        .create_session("agent-run-steer-current-frame")
        .await
        .expect("create session");
    let run_repo = InMemoryLifecycleRunRepo::default();
    let (run, launch_frame, current_frame) =
        seed_refreshed_agent_run_for_command_test(&hub, &run_repo, &session.id, base.path()).await;
    let service = AgentRunSteeringService::new(
        &run_repo,
        hub.lifecycle_agent_repo
            .as_ref()
            .expect("test hub should provide agent repo")
            .as_ref(),
        hub.agent_frame_repo
            .as_ref()
            .expect("test hub should provide frame repo")
            .as_ref(),
        hub.execution_anchor_repo
            .as_ref()
            .expect("test hub should provide anchor repo")
            .as_ref(),
        hub.core_service(),
        hub.control_service(),
        hub.eventing_service(),
    );

    let dispatch = service
        .steer(AgentRunSteeringCommand {
            delivery_runtime_session_id: session.id.clone(),
            input: agentdash_agent_protocol::text_user_input_blocks("live steer"),
        })
        .await
        .expect("steer should dispatch");

    assert_eq!(dispatch.run_id, run.id);
    assert_eq!(dispatch.agent_id, launch_frame.agent_id);
    assert_eq!(dispatch.frame_id, current_frame.id);
    assert_ne!(
        dispatch.frame_id, launch_frame.id,
        "steer must use current AgentFrame, not the launch-frame anchor"
    );
    assert_eq!(dispatch.active_turn_id, "turn-current");
    let captured = calls.lock().await;
    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0].session_id, session.id);
    assert_eq!(captured[0].expected_turn_id, "turn-current");
    assert_eq!(captured[0].input_text.as_deref(), Some("live steer"));
}

#[tokio::test]
async fn pending_promote_uses_current_agent_frame_after_frame_refresh() {
    let base = tempfile::tempdir().expect("tempdir");
    let connector = Arc::new(SteerCapturingConnector::default());
    let calls = connector.calls.clone();
    let connector_for_hub: Arc<dyn AgentConnector> = connector;
    let hub = test_hub(base.path().to_path_buf(), connector_for_hub, None)
        .with_lifecycle_agent_repo(Arc::new(MemoryLifecycleAgentRepository::default()));
    let session = hub
        .create_session("agent-run-promote-current-frame")
        .await
        .expect("create session");
    let run_repo = InMemoryLifecycleRunRepo::default();
    let (_run, launch_frame, current_frame) =
        seed_refreshed_agent_run_for_command_test(&hub, &run_repo, &session.id, base.path()).await;
    let input = agentdash_agent_protocol::text_user_input_blocks("queued steer");
    let service = AgentRunSteeringService::new(
        &run_repo,
        hub.lifecycle_agent_repo
            .as_ref()
            .expect("test hub should provide agent repo")
            .as_ref(),
        hub.agent_frame_repo
            .as_ref()
            .expect("test hub should provide frame repo")
            .as_ref(),
        hub.execution_anchor_repo
            .as_ref()
            .expect("test hub should provide anchor repo")
            .as_ref(),
        hub.core_service(),
        hub.control_service(),
        hub.eventing_service(),
    );

    let dispatch = service
        .steer(AgentRunSteeringCommand {
            delivery_runtime_session_id: session.id.clone(),
            input,
        })
        .await
        .expect("pending promote should steer");

    assert_eq!(dispatch.frame_id, current_frame.id);
    assert_ne!(
        dispatch.frame_id, launch_frame.id,
        "pending promote must resolve current AgentFrame through AgentRun steering"
    );
    assert_eq!(dispatch.active_turn_id, "turn-current");
    let captured = calls.lock().await;
    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0].session_id, session.id);
    assert_eq!(captured[0].expected_turn_id, "turn-current");
    assert_eq!(captured[0].input_text.as_deref(), Some("queued steer"));
}

#[derive(Default)]
struct CapturingConnector {
    captures: Arc<TokioMutex<Vec<CapturedPromptSurface>>>,
}

#[derive(Debug, Clone)]
struct CapturedPromptSurface {
    mcp_names: Vec<String>,
    tool_clusters: std::collections::BTreeSet<agentdash_spi::ToolCluster>,
    mount_ids: Vec<String>,
    default_mount_id: Option<String>,
}

#[async_trait::async_trait]
impl AgentConnector for CapturingConnector {
    fn connector_id(&self) -> &'static str {
        "capturing"
    }
    fn connector_type(&self) -> agentdash_spi::ConnectorType {
        agentdash_spi::ConnectorType::LocalExecutor
    }
    fn capabilities(&self) -> agentdash_spi::ConnectorCapabilities {
        agentdash_spi::ConnectorCapabilities::default()
    }
    fn list_executors(&self) -> Vec<agentdash_spi::AgentInfo> {
        Vec::new()
    }
    async fn discover_options_stream(
        &self,
        _executor: &str,
        _working_dir: Option<PathBuf>,
    ) -> Result<futures::stream::BoxStream<'static, json_patch::Patch>, ConnectorError> {
        Ok(Box::pin(stream::empty()))
    }
    async fn prompt(
        &self,
        _session_id: &str,
        _follow_up_session_id: Option<&str>,
        _prompt: &PromptPayload,
        context: agentdash_spi::ExecutionContext,
    ) -> Result<agentdash_spi::ExecutionStream, ConnectorError> {
        let vfs = context.session.vfs.clone();
        self.captures.lock().await.push(CapturedPromptSurface {
            mcp_names: context
                .session
                .mcp_servers
                .iter()
                .map(|server| server.name.clone())
                .collect(),
            tool_clusters: context.turn.capability_state.tool.enabled_clusters.clone(),
            mount_ids: vfs
                .as_ref()
                .map(|vfs| vfs.mounts.iter().map(|mount| mount.id.clone()).collect())
                .unwrap_or_default(),
            default_mount_id: vfs.and_then(|vfs| vfs.default_mount_id),
        });
        Ok(Box::pin(stream::empty()))
    }
    async fn cancel(&self, _session_id: &str) -> Result<(), ConnectorError> {
        Ok(())
    }
    async fn approve_tool_call(
        &self,
        _session_id: &str,
        _tool_call_id: &str,
    ) -> Result<(), ConnectorError> {
        Ok(())
    }
    async fn reject_tool_call(
        &self,
        _session_id: &str,
        _tool_call_id: &str,
        _reason: Option<String>,
    ) -> Result<(), ConnectorError> {
        Ok(())
    }
}

#[derive(Default)]
struct RepositoryRestoreCapturingConnector {
    restored_messages: Arc<TokioMutex<Vec<agentdash_spi::AgentMessage>>>,
}

#[async_trait::async_trait]
impl AgentConnector for RepositoryRestoreCapturingConnector {
    fn connector_id(&self) -> &'static str {
        "repository-restore-capturing"
    }
    fn connector_type(&self) -> agentdash_spi::ConnectorType {
        agentdash_spi::ConnectorType::LocalExecutor
    }
    fn capabilities(&self) -> agentdash_spi::ConnectorCapabilities {
        agentdash_spi::ConnectorCapabilities::default()
    }
    fn supports_repository_restore(&self, _executor: &str) -> bool {
        true
    }
    fn list_executors(&self) -> Vec<agentdash_spi::AgentInfo> {
        Vec::new()
    }
    async fn discover_options_stream(
        &self,
        _executor: &str,
        _working_dir: Option<PathBuf>,
    ) -> Result<futures::stream::BoxStream<'static, json_patch::Patch>, ConnectorError> {
        Ok(Box::pin(stream::empty()))
    }
    async fn prompt(
        &self,
        _session_id: &str,
        _follow_up_session_id: Option<&str>,
        _prompt: &PromptPayload,
        context: agentdash_spi::ExecutionContext,
    ) -> Result<agentdash_spi::ExecutionStream, ConnectorError> {
        let messages = context
            .turn
            .restored_session_state
            .as_ref()
            .map(|state| state.messages.clone())
            .unwrap_or_default();
        *self.restored_messages.lock().await = messages;
        Ok(Box::pin(stream::empty()))
    }
    async fn cancel(&self, _session_id: &str) -> Result<(), ConnectorError> {
        Ok(())
    }
    async fn approve_tool_call(
        &self,
        _session_id: &str,
        _tool_call_id: &str,
    ) -> Result<(), ConnectorError> {
        Ok(())
    }
    async fn reject_tool_call(
        &self,
        _session_id: &str,
        _tool_call_id: &str,
        _reason: Option<String>,
    ) -> Result<(), ConnectorError> {
        Ok(())
    }
}

struct SkillFixtureMountProvider;

#[async_trait::async_trait]
impl MountProvider for SkillFixtureMountProvider {
    fn provider_id(&self) -> &str {
        "canvas_fs"
    }

    async fn read_text(
        &self,
        _mount: &agentdash_domain::common::Mount,
        path: &str,
        _ctx: &MountOperationContext,
    ) -> Result<ReadResult, MountError> {
        if path == "skills/canvas-system/SKILL.md" {
            Ok(ReadResult::new(
                path,
                "---\nname: canvas-system\ndescription: Canvas authoring skill\n---\nUse Canvas.",
            ))
        } else {
            Err(MountError::NotFound(format!("文件不存在: {path}")))
        }
    }

    async fn write_text(
        &self,
        _mount: &agentdash_domain::common::Mount,
        path: &str,
        _content: &str,
        _ctx: &MountOperationContext,
    ) -> Result<(), MountError> {
        Err(MountError::NotSupported(format!("只读测试 mount: {path}")))
    }

    async fn list(
        &self,
        _mount: &agentdash_domain::common::Mount,
        options: &ListOptions,
        _ctx: &MountOperationContext,
    ) -> Result<ListResult, MountError> {
        let entries = match options.path.as_str() {
            "skills" => vec![RuntimeFileEntry::dir("skills/canvas-system")],
            "skills/canvas-system" => {
                vec![RuntimeFileEntry::file("skills/canvas-system/SKILL.md")]
            }
            _ => Vec::new(),
        };
        Ok(ListResult { entries })
    }

    async fn search_text(
        &self,
        _mount: &agentdash_domain::common::Mount,
        _query: &SearchQuery,
        _ctx: &MountOperationContext,
    ) -> Result<SearchResult, MountError> {
        Ok(SearchResult::default())
    }

    async fn exec(
        &self,
        _mount: &agentdash_domain::common::Mount,
        _request: &ExecRequest,
        _ctx: &MountOperationContext,
    ) -> Result<ExecResult, MountError> {
        Err(MountError::NotSupported(
            "测试 skill mount 不支持 exec".to_string(),
        ))
    }
}

fn skill_fixture_vfs_service() -> Arc<VfsService> {
    let mut registry = MountProviderRegistry::new();
    registry.register(Arc::new(SkillFixtureMountProvider));
    Arc::new(VfsService::new(Arc::new(registry)))
}

fn canvas_skill_vfs() -> agentdash_spi::Vfs {
    agentdash_spi::Vfs {
        mounts: vec![agentdash_domain::common::Mount {
            id: "cvs-demo".to_string(),
            provider: "canvas_fs".to_string(),
            backend_id: "demo".to_string(),
            root_ref: "canvas:demo".to_string(),
            capabilities: vec![
                agentdash_domain::common::MountCapability::Read,
                agentdash_domain::common::MountCapability::List,
            ],
            default_write: true,
            display_name: "Demo Canvas".to_string(),
            metadata: serde_json::json!({ "canvas_id": "demo" }),
        }],
        default_mount_id: Some("cvs-demo".to_string()),
        source_project_id: None,
        source_story_id: None,
        links: Vec::new(),
    }
}

#[tokio::test]
async fn adopt_persisted_frame_revision_into_active_runtime_requires_matching_frame_target() {
    let base = tempfile::tempdir().expect("tempdir");
    let hub = test_hub(base.path().to_path_buf(), Arc::new(PendingConnector), None)
        .with_lifecycle_agent_repo(Arc::new(MemoryLifecycleAgentRepository::default()));
    let session = hub
        .create_session("capability-surface")
        .await
        .expect("create");
    let _current_frame = attach_test_lifecycle_frame(&hub, &session.id).await;
    let frame = attach_test_lifecycle_frame(&hub, "another-session").await;

    let error = match hub
        .adopt_persisted_frame_revision_into_active_runtime(AgentFrameRuntimeTarget {
            frame_id: frame.id,
            delivery_runtime_session_id: session.id.clone(),
        })
        .await
    {
        Ok(_) => panic!("mismatched frame/session target should fail"),
        Err(error) => error,
    };

    match error {
        ConnectorError::Runtime(message) => {
            assert!(message.contains("未绑定 delivery RuntimeSession"));
        }
        other => panic!("expected runtime error, got {other}"),
    }
}

#[tokio::test]
async fn adopt_persisted_frame_revision_into_active_runtime_updates_runtime_without_writing_frame()
{
    let base = tempfile::tempdir().expect("tempdir");
    let hub = test_hub(base.path().to_path_buf(), Arc::new(PendingConnector), None)
        .with_lifecycle_agent_repo(Arc::new(MemoryLifecycleAgentRepository::default()));
    let session = hub
        .create_session("persisted-adoption")
        .await
        .expect("create");
    let initial_frame = attach_test_lifecycle_frame(&hub, &session.id).await;
    let mut persisted_state = CapabilityState::default();
    persisted_state
        .tool
        .capabilities
        .insert(agentdash_spi::ToolCapability::new("file_write"));
    persisted_state
        .tool
        .enabled_clusters
        .insert(agentdash_spi::ToolCluster::Write);
    let mut persisted_frame = AgentFrame::new_revision(
        initial_frame.agent_id,
        initial_frame.revision + 1,
        "test_surface_change",
    );
    persisted_frame.effective_capability_json =
        Some(serde_json::to_value(&persisted_state).expect("capability json"));
    hub.agent_frame_repo
        .as_ref()
        .expect("frame repo")
        .create(&persisted_frame)
        .await
        .expect("persist frame");
    let frame_count_before = hub
        .agent_frame_repo
        .as_ref()
        .expect("frame repo")
        .list_by_agent(initial_frame.agent_id)
        .await
        .expect("frames before")
        .len();

    let _rx = hub.ensure_session(&session.id).await;
    hub.runtime_registry
        .with_runtime_mut(&session.id, |runtime| {
            let runtime = runtime.expect("session runtime should exist");
            runtime.turn_state = TurnState::Active(Box::new(TurnExecution::new(
                "turn-1".to_string(),
                ExecutionSessionFrame {
                    turn_id: "turn-1".to_string(),
                    working_directory: base.path().to_path_buf(),
                    environment_variables: HashMap::new(),
                    executor_config: AgentConfig::new("PI_AGENT"),
                    mcp_servers: Vec::new(),
                    vfs: None,
                    backend_execution: None,
                    runtime_backend_anchor: None,
                    identity: None,
                },
                CapabilityState::default(),
                uuid::Uuid::new_v4(),
                uuid::Uuid::new_v4(),
            )));
        })
        .await;

    hub.adopt_persisted_frame_revision_into_active_runtime(AgentFrameRuntimeTarget {
        frame_id: persisted_frame.id,
        delivery_runtime_session_id: session.id.clone(),
    })
    .await
    .expect("adopt persisted frame");

    let cached_state = hub
        .runtime_registry
        .with_runtime(&session.id, |runtime| {
            runtime
                .and_then(|runtime| runtime.turn_state.active_turn())
                .map(|turn| turn.capability_state.clone())
        })
        .await
        .expect("active turn capability state");
    assert!(
        cached_state
            .tool
            .capabilities
            .contains(&agentdash_spi::ToolCapability::new("file_write"))
    );
    let frame_count_after = hub
        .agent_frame_repo
        .as_ref()
        .expect("frame repo")
        .list_by_agent(initial_frame.agent_id)
        .await
        .expect("frames after")
        .len();
    assert_eq!(
        frame_count_after, frame_count_before,
        "adoption helper must not write another AgentFrame revision"
    );
}

#[tokio::test]
async fn pending_runtime_context_transition_derives_skill_dimension_from_active_vfs() {
    let base = tempfile::tempdir().expect("tempdir");
    let hub = test_hub(base.path().to_path_buf(), Arc::new(EmptyConnector), None)
        .with_vfs_service(skill_fixture_vfs_service());
    let session = hub
        .create_session("pending-skill-capability")
        .await
        .expect("create");

    let mut before_state = CapabilityState::default();
    before_state.vfs.active = Some(agentdash_spi::Vfs::default());
    let mut after_state = before_state.clone();
    after_state.vfs.active = Some(canvas_skill_vfs());
    let transition = runtime_transition_from_state(&after_state, after_state.vfs.active.clone());

    hub.runtime_transition_service()
        .enqueue_pending_runtime_context_transition(PendingRuntimeContextTransitionInput {
            target_frame_id: uuid::Uuid::new_v4(),
            delivery_runtime_session_id: session.id.clone(),
            turn_id: None,
            frame_transition_id: "transition-skill-vfs".to_string(),
            phase_node: "review".to_string(),
            run_id: uuid::Uuid::new_v4(),
            lifecycle_key: "dev".to_string(),
            before_state: Some(before_state),
            after_state,
            transition,
            capability_keys: std::collections::BTreeSet::new(),
            source_turn_id: None,
            created_at: 1,
        })
        .await
        .expect("pending transition should enqueue");

    let commands = hub
        .persistence
        .list_runtime_commands_by_status(&[RuntimeCommandStatus::Requested], 10)
        .await
        .expect("runtime commands should load");
    let command = commands
        .iter()
        .find(|command| command.frame_transition_id == "transition-skill-vfs")
        .expect("pending transition should exist");
    let payload = serde_json::to_value(&command.delivery).expect("delivery serializes");
    assert!(payload.get("frame_transition_id").is_some());
    assert!(payload.get("transition").is_none());
    assert!(payload.get("state").is_none());
    assert_eq!(
        command
            .frame_transition
            .transition
            .effects
            .iter()
            .find(|effect| effect.effect_type == EFFECT_TYPE_APPLY_VFS_OVERLAY)
            .and_then(|effect| {
                serde_json::from_value::<agentdash_spi::Vfs>(effect.payload["overlay"].clone()).ok()
            })
            .and_then(|vfs| vfs.mounts.into_iter().next())
            .map(|mount| mount.id),
        Some("cvs-demo".to_string())
    );

    let events = hub
        .persistence
        .list_all_events(&session.id)
        .await
        .expect("events should load");
    let event = events
        .iter()
        .find(|event| {
            event.session_update_type == "platform_event"
                && matches!(
                    &event.notification.event,
                    BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate { key, value })
                        if key == "context_frame"
                            && value.get("kind").and_then(serde_json::Value::as_str)
                                == Some("capability_state_delta")
                )
        })
        .expect("capability state delta context_frame should exist");
    match &event.notification.event {
        BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate { value, .. }) => {
            assert_eq!(value["apply_mode"], "pending_next_turn");
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[tokio::test]
async fn pending_capability_state_transition_applies_on_next_prompt_and_clears_meta() {
    let base = tempfile::tempdir().expect("tempdir");
    let captures = Arc::new(TokioMutex::new(Vec::new()));
    let hub = test_hub(
        base.path().to_path_buf(),
        Arc::new(CapturingConnector {
            captures: captures.clone(),
        }),
        None,
    )
    .with_lifecycle_agent_repo(Arc::new(MemoryLifecycleAgentRepository::default()));
    let session = hub
        .create_session("pending-capability-surface")
        .await
        .expect("create");
    let launch_frame = attach_test_lifecycle_frame(&hub, &session.id).await;

    let mut target_flow =
        agentdash_spi::CapabilityState::from_clusters([agentdash_spi::ToolCluster::Write]);
    target_flow
        .tool
        .capabilities
        .insert(agentdash_spi::ToolCapability::new("file_write"));
    let target_mcp = agentdash_spi::RuntimeMcpServer {
        name: "phase_tools".to_string(),
        transport: agentdash_spi::McpTransportConfig::Http {
            url: "http://127.0.0.1:19092/mcp".to_string(),
            headers: vec![],
        },
        uses_relay: false,
    };
    let lifecycle_mount = agentdash_domain::common::Mount {
        id: "lifecycle".to_string(),
        provider: "lifecycle_vfs".to_string(),
        backend_id: String::new(),
        root_ref: "lifecycle://run/test".to_string(),
        capabilities: vec![agentdash_domain::common::MountCapability::Read],
        default_write: false,
        display_name: "Lifecycle".to_string(),
        metadata: serde_json::json!({ "phase": "review" }),
    };
    let pending_vfs = agentdash_spi::Vfs {
        mounts: vec![lifecycle_mount],
        default_mount_id: None,
        source_project_id: None,
        source_story_id: None,
        links: Vec::new(),
    };
    target_flow.tool.mcp_servers = vec![target_mcp];
    target_flow.vfs.active = Some(pending_vfs);

    let frame_transition = AgentFrameTransitionRecord::from_pending(
        launch_frame.id,
        PendingCapabilityStateTransition {
            id: "transition-1".to_string(),
            run_id: uuid::Uuid::new_v4(),
            lifecycle_key: "dev".to_string(),
            phase_node: "review".to_string(),
            capability_keys: std::collections::BTreeSet::from(["file_write".to_string()]),
            transition: runtime_transition_from_state(&target_flow, target_flow.vfs.active.clone()),
            created_at: 1,
            source_turn_id: None,
        },
    );
    let delivery = RuntimeDeliveryCommand::pending_runtime_context(&frame_transition);
    hub.enqueue_runtime_delivery_command(&session.id, delivery, frame_transition)
        .await
        .expect("enqueue pending transition");

    hub.start_prompt(&session.id, simple_prompt_request("hello"))
        .await
        .expect("prompt should start");

    let captures = captures.lock().await;
    let captured = captures.first().expect("connector should be called");
    assert_eq!(captured.mcp_names, vec!["phase_tools"]);
    assert!(
        captured
            .tool_clusters
            .contains(&agentdash_spi::ToolCluster::Write)
    );
    assert!(captured.mount_ids.contains(&"workspace".to_string()));
    assert!(captured.mount_ids.contains(&"lifecycle".to_string()));
    assert_eq!(captured.default_mount_id.as_deref(), Some("workspace"));

    let applied_commands = hub
        .persistence
        .list_runtime_commands_by_status(&[RuntimeCommandStatus::Applied], 10)
        .await
        .expect("runtime commands should load");
    assert_eq!(applied_commands.len(), 1);
    assert_eq!(applied_commands[0].frame_transition_id, "transition-1");

    let events = hub
        .persistence
        .list_all_events(&session.id)
        .await
        .expect("events should load");
    assert!(events.iter().any(|event| {
        matches!(
            &event.notification.event,
            BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate { key, value })
                if key == "context_frame"
                    && value.get("kind").and_then(serde_json::Value::as_str)
                        == Some("capability_state_delta")
                    && value.get("apply_mode").and_then(serde_json::Value::as_str)
                        == Some("applied_on_next_turn")
        )
    }));
}

#[derive(Default)]
struct SessionStartAwareConnector {
    session_start_seen: Arc<TokioMutex<Vec<bool>>>,
}

#[async_trait::async_trait]
impl AgentConnector for SessionStartAwareConnector {
    fn connector_id(&self) -> &'static str {
        "session-start-aware"
    }
    fn connector_type(&self) -> agentdash_spi::ConnectorType {
        agentdash_spi::ConnectorType::LocalExecutor
    }
    fn capabilities(&self) -> agentdash_spi::ConnectorCapabilities {
        agentdash_spi::ConnectorCapabilities::default()
    }
    fn list_executors(&self) -> Vec<agentdash_spi::AgentInfo> {
        Vec::new()
    }
    async fn discover_options_stream(
        &self,
        _executor: &str,
        _working_dir: Option<PathBuf>,
    ) -> Result<futures::stream::BoxStream<'static, json_patch::Patch>, ConnectorError> {
        Ok(Box::pin(stream::empty()))
    }

    async fn prompt(
        &self,
        _session_id: &str,
        _follow_up_session_id: Option<&str>,
        _prompt: &PromptPayload,
        context: agentdash_spi::ExecutionContext,
    ) -> Result<agentdash_spi::ExecutionStream, ConnectorError> {
        let seen = context.turn.hook_runtime.as_ref().is_some_and(|runtime| {
            runtime
                .trace()
                .iter()
                .any(|trace| matches!(&trace.trigger, HookTraceTrigger::SessionStart))
        });
        self.session_start_seen.lock().await.push(seen);
        Ok(Box::pin(stream::empty()))
    }

    async fn cancel(&self, _session_id: &str) -> Result<(), ConnectorError> {
        Ok(())
    }
    async fn approve_tool_call(
        &self,
        _session_id: &str,
        _tool_call_id: &str,
    ) -> Result<(), ConnectorError> {
        Ok(())
    }
    async fn reject_tool_call(
        &self,
        _session_id: &str,
        _tool_call_id: &str,
        _reason: Option<String>,
    ) -> Result<(), ConnectorError> {
        Ok(())
    }
}

struct RecordingHookProvider {
    queries: Arc<TokioMutex<Vec<HookEvaluationQuery>>>,
}

#[async_trait::async_trait]
impl ExecutionHookProvider for RecordingHookProvider {
    async fn load_frame_snapshot(
        &self,
        query: AgentFrameHookSnapshotQuery,
    ) -> Result<AgentFrameHookSnapshot, agentdash_spi::hooks::HookError> {
        Ok(test_hook_snapshot(
            query.provenance.runtime_session_id.unwrap_or_default(),
        ))
    }

    async fn refresh_frame_snapshot(
        &self,
        query: AgentFrameHookRefreshQuery,
    ) -> Result<AgentFrameHookSnapshot, agentdash_spi::hooks::HookError> {
        Ok(test_hook_snapshot(
            query.provenance.runtime_session_id.unwrap_or_default(),
        ))
    }

    async fn evaluate_frame_hook(
        &self,
        query: AgentFrameHookEvaluationQuery,
    ) -> Result<HookResolution, agentdash_spi::hooks::HookError> {
        self.queries.lock().await.push(HookEvaluationQuery {
            session_id: query.provenance.runtime_session_id.unwrap_or_default(),
            trigger: query.trigger,
            turn_id: query.provenance.turn_id,
            tool_name: query.tool_name,
            tool_call_id: query.tool_call_id,
            subagent_type: query.subagent_type,
            snapshot: query.snapshot,
            payload: query.payload,
            token_stats: query.token_stats,
        });
        Ok(HookResolution::default())
    }
}

struct CurrentFrameHookProvider;

#[async_trait::async_trait]
impl ExecutionHookProvider for CurrentFrameHookProvider {
    async fn load_frame_snapshot(
        &self,
        query: AgentFrameHookSnapshotQuery,
    ) -> Result<AgentFrameHookSnapshot, agentdash_spi::hooks::HookError> {
        Ok(test_hook_snapshot(
            query.provenance.runtime_session_id.unwrap_or_default(),
        ))
    }

    async fn refresh_frame_snapshot(
        &self,
        query: AgentFrameHookRefreshQuery,
    ) -> Result<AgentFrameHookSnapshot, agentdash_spi::hooks::HookError> {
        Ok(test_hook_snapshot(
            query.provenance.runtime_session_id.unwrap_or_default(),
        ))
    }

    async fn evaluate_frame_hook(
        &self,
        _query: AgentFrameHookEvaluationQuery,
    ) -> Result<HookResolution, agentdash_spi::hooks::HookError> {
        Ok(HookResolution::default())
    }
}

struct StaticResolutionHookProvider {
    queries: Arc<TokioMutex<Vec<HookEvaluationQuery>>>,
    resolution: HookResolution,
}

#[async_trait::async_trait]
impl ExecutionHookProvider for StaticResolutionHookProvider {
    async fn load_frame_snapshot(
        &self,
        query: AgentFrameHookSnapshotQuery,
    ) -> Result<AgentFrameHookSnapshot, agentdash_spi::hooks::HookError> {
        Ok(test_hook_snapshot(
            query.provenance.runtime_session_id.unwrap_or_default(),
        ))
    }

    async fn refresh_frame_snapshot(
        &self,
        query: AgentFrameHookRefreshQuery,
    ) -> Result<AgentFrameHookSnapshot, agentdash_spi::hooks::HookError> {
        Ok(test_hook_snapshot(
            query.provenance.runtime_session_id.unwrap_or_default(),
        ))
    }

    async fn evaluate_frame_hook(
        &self,
        query: AgentFrameHookEvaluationQuery,
    ) -> Result<HookResolution, agentdash_spi::hooks::HookError> {
        self.queries.lock().await.push(HookEvaluationQuery {
            session_id: query.provenance.runtime_session_id.unwrap_or_default(),
            trigger: query.trigger,
            turn_id: query.provenance.turn_id,
            tool_name: query.tool_name,
            tool_call_id: query.tool_call_id,
            subagent_type: query.subagent_type,
            snapshot: query.snapshot,
            payload: query.payload,
            token_stats: query.token_stats,
        });
        Ok(self.resolution.clone())
    }
}

struct SnapshotRecordingHookProvider {
    frame_snapshot_queries: Arc<TokioMutex<Vec<AgentFrameHookSnapshotQuery>>>,
}

#[async_trait::async_trait]
impl ExecutionHookProvider for SnapshotRecordingHookProvider {
    async fn load_frame_snapshot(
        &self,
        query: AgentFrameHookSnapshotQuery,
    ) -> Result<AgentFrameHookSnapshot, agentdash_spi::hooks::HookError> {
        let session_id = query
            .provenance
            .runtime_session_id
            .clone()
            .unwrap_or_default();
        self.frame_snapshot_queries.lock().await.push(query);
        Ok(test_hook_snapshot(session_id))
    }

    async fn refresh_frame_snapshot(
        &self,
        query: AgentFrameHookRefreshQuery,
    ) -> Result<AgentFrameHookSnapshot, agentdash_spi::hooks::HookError> {
        let session_id = query
            .provenance
            .runtime_session_id
            .clone()
            .unwrap_or_default();
        self.frame_snapshot_queries
            .lock()
            .await
            .push(AgentFrameHookSnapshotQuery {
                target: query.target,
                provenance: query.provenance,
            });
        Ok(test_hook_snapshot(session_id))
    }

    async fn evaluate_frame_hook(
        &self,
        _query: AgentFrameHookEvaluationQuery,
    ) -> Result<HookResolution, agentdash_spi::hooks::HookError> {
        Ok(HookResolution::default())
    }
}

#[tokio::test]
async fn lazy_hook_runtime_rebuild_loads_snapshot_from_frame_target() {
    let base = tempfile::tempdir().expect("tempdir");
    let frame_snapshot_queries = Arc::new(TokioMutex::new(Vec::new()));
    let hook_provider = Arc::new(SnapshotRecordingHookProvider {
        frame_snapshot_queries: frame_snapshot_queries.clone(),
    });
    let hub = test_hub(
        base.path().to_path_buf(),
        Arc::new(PendingConnector),
        Some(hook_provider),
    );
    let session = hub
        .create_session("lazy-hook-target")
        .await
        .expect("create");
    let frame = attach_test_frame(&hub, &session.id).await;
    let target = control_target_from_anchor_frame(&hub, &session.id, &frame).await;

    let runtime = hub
        .hook_service()
        .ensure_hook_runtime_for_target(
            &AgentFrameRuntimeTarget {
                frame_id: frame.id,
                delivery_runtime_session_id: session.id.clone(),
            },
            Some("turn-lazy"),
        )
        .await
        .expect("target-first hook runtime rebuild should not fail")
        .expect("hook runtime should be rebuilt");

    assert_eq!(runtime.control_target(), target);
    let frame_queries = frame_snapshot_queries.lock().await;
    assert_eq!(frame_queries.len(), 1);
    assert_eq!(frame_queries[0].target, target);
    assert_eq!(
        frame_queries[0].provenance.runtime_session_id.as_deref(),
        Some(session.id.as_str())
    );
    assert_eq!(
        frame_queries[0].provenance.source,
        "hook_runtime_target_rebuild"
    );
}

#[tokio::test]
async fn hook_runtime_target_cache_hit_does_not_refresh_snapshot() {
    let base = tempfile::tempdir().expect("tempdir");
    let frame_snapshot_queries = Arc::new(TokioMutex::new(Vec::new()));
    let hook_provider = Arc::new(SnapshotRecordingHookProvider {
        frame_snapshot_queries: frame_snapshot_queries.clone(),
    });
    let hub = test_hub(
        base.path().to_path_buf(),
        Arc::new(PendingConnector),
        Some(hook_provider),
    );
    let session = hub
        .create_session("lazy-hook-target-cache-hit")
        .await
        .expect("create");
    let frame = attach_test_frame(&hub, &session.id).await;
    let target = control_target_from_anchor_frame(&hub, &session.id, &frame).await;
    let request = AgentFrameRuntimeTarget {
        frame_id: target.frame_id,
        delivery_runtime_session_id: session.id.clone(),
    };

    let runtime1 = hub
        .hook_service()
        .ensure_hook_runtime_for_target(&request, Some("turn-1"))
        .await
        .expect("initial hook runtime should load")
        .expect("initial hook runtime should exist");
    let runtime2 = hub
        .hook_service()
        .ensure_hook_runtime_for_target(&request, Some("turn-2"))
        .await
        .expect("cache hit should not refresh")
        .expect("cached hook runtime should exist");

    assert_eq!(runtime1.control_target(), target);
    assert_eq!(runtime2.control_target(), target);
    assert!(Arc::ptr_eq(&runtime1, &runtime2));
    let frame_queries = frame_snapshot_queries.lock().await;
    assert_eq!(frame_queries.len(), 1);
    assert_eq!(frame_queries[0].target, target);
}

#[tokio::test]
async fn hook_runtime_resolve_cache_hit_does_not_refresh_snapshot() {
    let base = tempfile::tempdir().expect("tempdir");
    let frame_snapshot_queries = Arc::new(TokioMutex::new(Vec::new()));
    let hook_provider = Arc::new(SnapshotRecordingHookProvider {
        frame_snapshot_queries: frame_snapshot_queries.clone(),
    });
    let hub = test_hub(
        base.path().to_path_buf(),
        Arc::new(PendingConnector),
        Some(hook_provider),
    );
    let session = hub
        .create_session("hook-runtime-resolve-cache-hit")
        .await
        .expect("create");
    let frame = attach_test_frame(&hub, &session.id).await;
    let target = control_target_from_anchor_frame(&hub, &session.id, &frame).await;

    let runtime1 = hub
        .hook_service()
        .resolve_hook_runtime(
            &session.id,
            "turn-1",
            frame.id,
            Some(&frame),
            &AgentConfig::new("PI_AGENT"),
            base.path(),
            true,
        )
        .await
        .expect("initial hook runtime should load")
        .expect("initial hook runtime should exist");
    let runtime2 = hub
        .hook_service()
        .resolve_hook_runtime(
            &session.id,
            "turn-2",
            frame.id,
            Some(&frame),
            &AgentConfig::new("PI_AGENT"),
            base.path(),
            false,
        )
        .await
        .expect("same-target subsequent turn should reuse cache")
        .expect("cached hook runtime should exist");

    assert_eq!(runtime1.control_target(), target);
    assert_eq!(runtime2.control_target(), target);
    assert!(Arc::ptr_eq(&runtime1, &runtime2));
    let frame_queries = frame_snapshot_queries.lock().await;
    assert_eq!(frame_queries.len(), 1);
    assert_eq!(frame_queries[0].target, target);
    assert_eq!(
        frame_queries[0].provenance.source,
        "hook_runtime_launch_target_reload"
    );
}

#[tokio::test]
async fn hook_runtime_target_switch_replaces_stale_cached_runtime() {
    let base = tempfile::tempdir().expect("tempdir");
    let frame_snapshot_queries = Arc::new(TokioMutex::new(Vec::new()));
    let hook_provider = Arc::new(SnapshotRecordingHookProvider {
        frame_snapshot_queries: frame_snapshot_queries.clone(),
    });
    let hub = test_hub(
        base.path().to_path_buf(),
        Arc::new(PendingConnector),
        Some(hook_provider),
    );
    let session = hub
        .create_session("lazy-hook-target-switch")
        .await
        .expect("create");
    let frame1 = attach_test_frame(&hub, &session.id).await;
    let target1 = control_target_from_anchor_frame(&hub, &session.id, &frame1).await;

    let runtime1 = hub
        .hook_service()
        .ensure_hook_runtime_for_target(
            &AgentFrameRuntimeTarget {
                frame_id: target1.frame_id,
                delivery_runtime_session_id: session.id.clone(),
            },
            Some("turn-1"),
        )
        .await
        .expect("initial hook runtime should load")
        .expect("initial hook runtime should exist");
    assert_eq!(runtime1.control_target(), target1);

    let frame2 = attach_test_frame(&hub, &session.id).await;
    let target2 = control_target_from_anchor_frame(&hub, &session.id, &frame2).await;
    let runtime2 = hub
        .hook_service()
        .ensure_hook_runtime_for_target(
            &AgentFrameRuntimeTarget {
                frame_id: target2.frame_id,
                delivery_runtime_session_id: session.id.clone(),
            },
            Some("turn-2"),
        )
        .await
        .expect("normal frame switch should refresh stale runtime")
        .expect("refreshed hook runtime should exist");

    assert_eq!(runtime2.control_target(), target2);
    assert_ne!(runtime2.control_target(), runtime1.control_target());
    let frame_queries = frame_snapshot_queries.lock().await;
    assert_eq!(frame_queries.len(), 2);
    assert_eq!(frame_queries[0].target, target1);
    assert_eq!(frame_queries[1].target, target2);
}

#[tokio::test]
async fn target_first_hook_runtime_ensure_rebuilds_from_requested_frame() {
    let base = tempfile::tempdir().expect("tempdir");
    let frame_snapshot_queries = Arc::new(TokioMutex::new(Vec::new()));
    let hook_provider = Arc::new(SnapshotRecordingHookProvider {
        frame_snapshot_queries: frame_snapshot_queries.clone(),
    });
    let hub = test_hub(
        base.path().to_path_buf(),
        Arc::new(PendingConnector),
        Some(hook_provider),
    );
    let session = hub
        .create_session("target-first-requested-frame")
        .await
        .expect("create");
    let frame1 = attach_test_frame(&hub, &session.id).await;
    let target1 = control_target_from_anchor_frame(&hub, &session.id, &frame1).await;

    let runtime1 = hub
        .hook_service()
        .ensure_hook_runtime_for_target(
            &AgentFrameRuntimeTarget {
                frame_id: target1.frame_id,
                delivery_runtime_session_id: session.id.clone(),
            },
            Some("turn-1"),
        )
        .await
        .expect("initial hook runtime should load")
        .expect("initial hook runtime should exist");
    assert_eq!(runtime1.control_target(), target1);

    let frame2 = attach_test_frame(&hub, &session.id).await;
    let target2 = control_target_from_anchor_frame(&hub, &session.id, &frame2).await;

    let runtime2 = hub
        .hook_service()
        .ensure_hook_runtime_for_target(
            &AgentFrameRuntimeTarget {
                frame_id: target2.frame_id,
                delivery_runtime_session_id: session.id.clone(),
            },
            Some("turn-2"),
        )
        .await
        .expect("target-first ensure should rebuild from requested frame")
        .expect("rebuilt hook runtime should exist");

    assert_eq!(runtime2.control_target(), target2);
    assert_ne!(runtime2.control_target(), target1);
    let frame_queries = frame_snapshot_queries.lock().await;
    assert_eq!(frame_queries.len(), 2);
    assert_eq!(frame_queries[0].target, target1);
    assert_eq!(frame_queries[1].target, target2);
}

#[test]
fn hook_business_paths_do_not_use_delivery_session_runtime_lookup() {
    let hook_service = include_str!("../hooks_service.rs");
    assert!(
        !hook_service.contains("reload_hook_runtime("),
        "SessionHookService must not expose a session-first hook runtime reload fallback"
    );
    assert!(
        !hook_service.contains("resolve_runtime_hook_target("),
        "SessionHookService must not resolve hook owner from naked RuntimeSession id"
    );
    assert!(
        !hook_service.contains("ensure_hook_runtime_for_delivery_session"),
        "SessionHookService target-first entry must not depend on delivery-session target resolution"
    );

    let transition = include_str!("runtime_context_transition.rs");
    assert!(
        !transition.contains("get_hook_runtime_by_delivery_session"),
        "runtime context transition should receive or ensure a target-bound hook runtime"
    );
    assert!(
        !transition.contains("ensure_hook_runtime_for_delivery_session"),
        "runtime context transition must not rebuild hook runtime from naked delivery session"
    );
}

#[derive(Default)]
struct NotificationCapturingConnector {
    notifications: Arc<TokioMutex<Vec<(String, String)>>>,
}

#[async_trait::async_trait]
impl AgentConnector for NotificationCapturingConnector {
    fn connector_id(&self) -> &'static str {
        "notification-capturing"
    }
    fn connector_type(&self) -> agentdash_spi::ConnectorType {
        agentdash_spi::ConnectorType::LocalExecutor
    }
    fn capabilities(&self) -> agentdash_spi::ConnectorCapabilities {
        agentdash_spi::ConnectorCapabilities::default()
    }
    fn list_executors(&self) -> Vec<agentdash_spi::AgentInfo> {
        Vec::new()
    }
    async fn discover_options_stream(
        &self,
        _executor: &str,
        _working_dir: Option<PathBuf>,
    ) -> Result<futures::stream::BoxStream<'static, json_patch::Patch>, ConnectorError> {
        Ok(Box::pin(stream::empty()))
    }
    async fn prompt(
        &self,
        _session_id: &str,
        _follow_up_session_id: Option<&str>,
        _prompt: &PromptPayload,
        _context: agentdash_spi::ExecutionContext,
    ) -> Result<agentdash_spi::ExecutionStream, ConnectorError> {
        Ok(Box::pin(stream::empty()))
    }
    async fn cancel(&self, _session_id: &str) -> Result<(), ConnectorError> {
        Ok(())
    }
    async fn approve_tool_call(
        &self,
        _session_id: &str,
        _tool_call_id: &str,
    ) -> Result<(), ConnectorError> {
        Ok(())
    }
    async fn reject_tool_call(
        &self,
        _session_id: &str,
        _tool_call_id: &str,
        _reason: Option<String>,
    ) -> Result<(), ConnectorError> {
        Ok(())
    }
    async fn push_session_notification(
        &self,
        session_id: &str,
        message: String,
    ) -> Result<(), ConnectorError> {
        self.notifications
            .lock()
            .await
            .push((session_id.to_string(), message));
        Ok(())
    }
}

#[tokio::test]
async fn runtime_context_update_injections_are_recorded_without_direct_notification() {
    let base = tempfile::tempdir().expect("tempdir");
    let queries = Arc::new(TokioMutex::new(Vec::new()));
    let injection = HookInjection {
        slot: "workflow_context".to_string(),
        content: "请使用 phase B 的工具约束继续推进。".to_string(),
        source: "workflow:phase_b".to_string(),
    };
    let provider = Arc::new(StaticResolutionHookProvider {
        queries: queries.clone(),
        resolution: HookResolution::default(),
    });
    let connector = Arc::new(NotificationCapturingConnector::default());
    let notifications = connector.notifications.clone();
    let hub = test_hub(base.path().to_path_buf(), connector, Some(provider));
    let session = hub
        .create_session("capability-hook-injection")
        .await
        .expect("create");
    let frame = attach_test_frame(&hub, &session.id).await;
    let _rx = hub.ensure_session(&session.id).await;

    let hook_runtime = ensure_hook_runtime_for_frame(&hub, &session.id, frame.id, "turn-cap").await;
    let bundle_session_uuid = uuid::Uuid::new_v4();
    hub.runtime_registry
        .with_runtime_mut(&session.id, |runtime| {
            let runtime = runtime.expect("session runtime should exist");
            runtime.turn_state = TurnState::Active(Box::new(TurnExecution::new(
                "turn-cap".to_string(),
                ExecutionSessionFrame {
                    turn_id: "turn-cap".to_string(),
                    working_directory: base.path().to_path_buf(),
                    environment_variables: HashMap::new(),
                    executor_config: AgentConfig::new("PI_AGENT"),
                    mcp_servers: vec![],
                    vfs: Some(local_workspace_vfs(base.path())),
                    backend_execution: None,
                    runtime_backend_anchor: None,
                    identity: None,
                },
                CapabilityState::default(),
                uuid::Uuid::new_v4(),
                bundle_session_uuid,
            )));
        })
        .await;

    let mut snapshot = hook_runtime.snapshot();
    snapshot.injections = vec![injection.clone()];
    hook_runtime.replace_snapshot(snapshot);

    let result = hub
        .collect_runtime_context_update_injections(&session.id, &hook_runtime)
        .await;
    assert_eq!(result, vec![injection.clone()]);

    let recorded_queries = queries.lock().await;
    assert!(
        recorded_queries.is_empty(),
        "runtime context update 不应再走 Hook provider evaluate_hook"
    );
    drop(recorded_queries);

    let captured = notifications.lock().await;
    assert!(
        captured.is_empty(),
        "runtime context update 不应直接推送第二条 live notification"
    );
    drop(captured);

    let trace = hook_runtime.trace();
    assert!(
        trace.is_empty(),
        "runtime context update 不是 HookTrace trigger，不应写 trace"
    );

    let turn = hub
        .runtime_registry
        .with_runtime(&session.id, |runtime| {
            runtime.and_then(|runtime| runtime.turn_state.active_turn().cloned())
        })
        .await
        .expect("active turn should remain available");
    assert_eq!(turn.runtime_injection_fragments.len(), 1);
    assert_eq!(turn.runtime_injection_fragments[0].slot, "workflow_context");
    assert_eq!(
        turn.runtime_injection_fragments[0].source,
        "workflow:phase_b"
    );
    assert_eq!(
        turn.runtime_injection_fragments[0].content,
        "请使用 phase B 的工具约束继续推进。"
    );
}

#[test]
fn resolve_prompt_payload_from_text_block() {
    let input = UserPromptInput::from_text("  hello world  ");

    let payload = input
        .resolve_prompt_payload()
        .expect("resolve should succeed");
    assert_eq!(payload.text_prompt, "hello world");
    // canonical 输入：投递路径已收敛为 PromptPayload::Input。
    assert!(matches!(payload.prompt_payload, PromptPayload::Input(_)));
    assert_eq!(payload.input.len(), 1);
    assert!(matches!(
        payload.input[0],
        agentdash_agent_protocol::codex_app_server_protocol::UserInput::Text { .. }
    ));
}

#[test]
fn resolve_prompt_payload_supports_multiple_input_types() {
    // 入参已是 canonical Vec<UserInputBlock>（与 steer 同形）；
    // ContentBlock -> canonical 的转换在 relay 边界单实现，单测在 protocol crate。
    let input = UserPromptInput {
        input: Some(vec![
            codex::UserInput::Text {
                text: "请分析 @src/main.ts".to_string(),
                text_elements: Vec::new(),
            },
            codex::UserInput::Mention {
                name: "src/main.ts".to_string(),
                path: "file:///workspace/src/main.ts".to_string(),
            },
            codex::UserInput::Image {
                detail: None,
                url: "data:image/png;base64,AAAA".to_string(),
            },
        ]),
        env: std::collections::HashMap::new(),
        executor_config: None,
        backend_selection: None,
    };

    let payload = input
        .resolve_prompt_payload()
        .expect("resolve should succeed");
    // canonical 输入：投递路径已收敛为 PromptPayload::Input；图片结构化保留为 Image 变体。
    assert!(matches!(payload.prompt_payload, PromptPayload::Input(_)));
    assert_eq!(payload.input.len(), 3);
    assert!(payload.input.iter().any(|item| matches!(
        item,
        agentdash_agent_protocol::codex_app_server_protocol::UserInput::Image { .. }
    )));
    // text_prompt 仅作摘要：文本与 mention 名保留为文本；图片以 data URL 形式出现在摘要中。
    assert!(payload.text_prompt.contains("请分析 @src/main.ts"));
    assert!(payload.text_prompt.contains("data:image/png;base64,AAAA"));
}

#[test]
fn build_user_input_submitted_preserves_turn_and_content() {
    let source = SourceInfo {
        connector_id: "unit-test".to_string(),
        connector_type: "local_executor".to_string(),
        executor_id: Some("CLAUDE_CODE".to_string()),
    };

    let envelope = build_user_input_submitted_envelope(
        "sess-test",
        &source,
        "t100",
        "t100:user-input:0",
        UserInputSubmissionKind::Prompt,
        vec![codex::UserInput::Text {
            text: "hello".to_string(),
            text_elements: Vec::new(),
        }],
    );

    assert_eq!(envelope.trace.turn_id.as_deref(), Some("t100"));
    assert_eq!(envelope.trace.entry_index, Some(0));
    let BackboneEvent::UserInputSubmitted(payload) = envelope.event else {
        panic!("expected user_input_submitted event");
    };
    assert_eq!(payload.item_id, "t100:user-input:0");
    assert_eq!(payload.submission_kind, UserInputSubmissionKind::Prompt);
}

/// Trait extension for BackboneEvent to check platform event keys.
trait BackboneEventExt {
    fn as_ref(&self) -> &BackboneEvent;
    fn is_platform_session_meta_update(&self, key: &str) -> bool {
        matches!(
            self.as_ref(),
            BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate { key: k, .. }) if k == key
        )
    }
}

impl BackboneEventExt for BackboneEvent {
    fn as_ref(&self) -> &BackboneEvent {
        self
    }
}

#[tokio::test]
async fn emit_context_frame_persists_agent_visible_frame() {
    let base = tempfile::tempdir().expect("tempdir");
    let hub = test_hub(base.path().to_path_buf(), Arc::new(EmptyConnector), None);
    let session = hub
        .create_session("runtime-context-notice")
        .await
        .expect("create session");

    let notice = ContextFrame {
        id: "runtime-context-apply-1".to_string(),
        kind: "capability_state_delta".to_string(),
        source: RuntimeEventSource::RuntimeContextUpdate,
        phase_node: Some("apply".to_string()),
        apply_mode: Some("live".to_string()),
        delivery_status: "queued_for_transform_context".to_string(),
        delivery_channel: "turn_start".to_string(),
        message_role: "user".to_string(),
        rendered_text: "## Capability State Delta — Step Transition: apply".to_string(),
        sections: vec![ContextFrameSection::ToolSchemaDelta {
            added_tools: vec![],
        }],
        created_at_ms: 1,
    };

    hub.emit_context_frame(&session.id, Some("turn-42"), &notice)
        .await
        .expect("emit runtime context notice");

    let events = hub
        .persistence
        .list_all_events(&session.id)
        .await
        .expect("events should load");
    let event = events
        .iter()
        .find(|event| {
            event.session_update_type == "platform_event"
                && event
                    .notification
                    .event
                    .as_ref()
                    .is_platform_session_meta_update("context_frame")
        })
        .expect("runtime context notice event should exist");

    assert_eq!(event.turn_id.as_deref(), Some("turn-42"));
    match &event.notification.event {
        BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate { value, .. }) => {
            assert_eq!(value["rendered_text"], notice.rendered_text);
            assert_eq!(value["phase_node"], "apply");
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[tokio::test]
async fn start_prompt_triggers_session_start_before_connector_prompt() {
    let connector = Arc::new(SessionStartAwareConnector::default());
    let queries = Arc::new(TokioMutex::new(Vec::new()));
    let frame_repo = Arc::new(MemoryAgentFrameRepository::default());
    let hook_provider = Arc::new(RecordingHookProvider {
        queries: queries.clone(),
    });
    let gate_repo = Arc::new(MemoryLifecycleGateRepository::default());
    let agent_repo = Arc::new(MemoryLifecycleAgentRepository::default());
    let anchor_repo = Arc::new(MemoryRuntimeSessionExecutionAnchorRepository::default());
    let hub = SessionRuntimeInner::new_with_hooks_and_persistence(
        connector.clone(),
        Some(hook_provider),
        Arc::new(MemorySessionPersistence::default()),
    )
    .with_agent_frame_repo(frame_repo)
    .with_lifecycle_gate_repo(gate_repo)
    .with_execution_anchor_repo(anchor_repo.clone())
    .with_lifecycle_agent_repo(agent_repo.clone());
    let session = hub.create_session("test").await.expect("create session");
    let frame = attach_test_frame(&hub, &session.id).await;
    let anchor = anchor_repo
        .find_by_session(&session.id)
        .await
        .expect("anchor lookup should succeed")
        .expect("test frame should attach runtime anchor");
    let mut agent =
        LifecycleAgent::new_root(anchor.run_id, uuid::Uuid::new_v4(), AgentSource::Unknown);
    agent.id = frame.agent_id;
    agent_repo.create(&agent).await.expect("create agent");

    hub.start_prompt(&session.id, owner_bootstrap_request("hello", "ctx"))
        .await
        .expect("prompt should start");

    let seen = connector.session_start_seen.lock().await;
    assert_eq!(seen.as_slice(), &[true]);

    let queries = queries.lock().await;
    assert!(
        queries
            .iter()
            .any(|query| matches!(query.trigger, HookTrigger::SessionStart))
    );
}

#[tokio::test]
async fn continuation_context_frame_strips_owner_resource_blocks() {
    let persistence = Arc::new(MemorySessionPersistence::default());
    let hub = SessionRuntimeInner::new_with_hooks_and_persistence(
        Arc::new(SessionStartAwareConnector::default()),
        None,
        persistence,
    );
    let session = hub.create_session("test").await.expect("create session");

    let source = SourceInfo {
        connector_id: "test".to_string(),
        connector_type: "unit".to_string(),
        executor_id: None,
    };
    hub.inject_notification(
        &session.id,
        build_user_input_submitted_envelope(
            &session.id,
            &source,
            "t-1",
            "t-1:user-input:0",
            UserInputSubmissionKind::Prompt,
            vec![codex::UserInput::Text {
                text: "继续分析 session 生命周期".to_string(),
                text_elements: Vec::new(),
            }],
        ),
    )
    .await
    .expect("inject user notification");

    hub.inject_notification(
        &session.id,
        BackboneEnvelope::new(
            BackboneEvent::ItemCompleted(ItemCompletedNotification::new(
                codex::ThreadItem::AgentMessage {
                    id: "assistant-msg-1".to_string(),
                    text: "已记录历史".to_string(),
                    phase: None,
                    memory_citation: None,
                },
                session.id.clone(),
                "t-1".to_string(),
            )),
            session.id.clone(),
            source.clone(),
        )
        .with_trace(TraceInfo {
            turn_id: Some("t-1".to_string()),
            entry_index: Some(99),
        }),
    )
    .await
    .expect("inject assistant notification");

    let transcript = hub
        .build_projected_transcript(&session.id)
        .await
        .expect("transcript should build");
    let frame = super::super::continuation::build_continuation_context_frame(
        &transcript,
        Some("## Owner\nproject"),
    )
    .expect("continuation context should exist");
    assert_eq!(frame.kind, "continuation_context");
    assert!(frame.rendered_text.contains("继续分析 session 生命周期"));
    assert!(frame.rendered_text.contains("已记录历史"));
    assert!(frame.rendered_text.contains("## Owner"));
    assert!(!frame.rendered_text.contains("agentdash://project-context/"));
    assert!(!frame.rendered_text.contains("hidden"));
}

#[tokio::test]
async fn build_projected_transcript_reconstructs_tool_history_without_owner_blocks() {
    let persistence = Arc::new(MemorySessionPersistence::default());
    let hub = SessionRuntimeInner::new_with_hooks_and_persistence(
        Arc::new(SessionStartAwareConnector::default()),
        None,
        persistence,
    );
    let session = hub.create_session("test").await.expect("create session");

    let source = SourceInfo {
        connector_id: "test".to_string(),
        connector_type: "unit".to_string(),
        executor_id: None,
    };
    hub.inject_notification(
        &session.id,
        build_user_input_submitted_envelope(
            &session.id,
            &source,
            "t-1",
            "t-1:user-input:0",
            UserInputSubmissionKind::Prompt,
            vec![codex::UserInput::Text {
                text: "继续分析 session 生命周期".to_string(),
                text_elements: Vec::new(),
            }],
        ),
    )
    .await
    .expect("inject user notification");

    hub.inject_notification(
        &session.id,
        BackboneEnvelope::new(
            BackboneEvent::ItemCompleted(ItemCompletedNotification::new(
                codex::ThreadItem::AgentMessage {
                    id: "assistant-msg-1".to_string(),
                    text: "已记录历史".to_string(),
                    phase: None,
                    memory_citation: None,
                },
                session.id.clone(),
                "t-1".to_string(),
            )),
            session.id.clone(),
            source.clone(),
        )
        .with_trace(TraceInfo {
            turn_id: Some("t-1".to_string()),
            entry_index: Some(1),
        }),
    )
    .await
    .expect("inject assistant notification");

    let item_started = codex::ThreadItem::DynamicToolCall {
        id: "tool-1".to_string(),
        namespace: None,
        tool: "shell_exec".to_string(),
        arguments: serde_json::json!({ "command": "pwd" }),
        status: codex::DynamicToolCallStatus::InProgress,
        content_items: None,
        success: None,
        duration_ms: None,
    };
    hub.inject_notification(
        &session.id,
        BackboneEnvelope::new(
            BackboneEvent::ItemStarted(ItemStartedNotification::new(
                item_started,
                session.id.clone(),
                "t-1".to_string(),
            )),
            session.id.clone(),
            source.clone(),
        )
        .with_trace(TraceInfo {
            turn_id: Some("t-1".to_string()),
            entry_index: Some(1),
        }),
    )
    .await
    .expect("inject tool call");

    let item_completed = codex::ThreadItem::DynamicToolCall {
        id: "tool-1".to_string(),
        namespace: None,
        tool: "shell_exec".to_string(),
        arguments: serde_json::json!({ "command": "pwd" }),
        status: codex::DynamicToolCallStatus::Completed,
        content_items: Some(vec![codex::DynamicToolCallOutputContentItem::InputText {
            text: "workspace root".to_string(),
        }]),
        success: Some(true),
        duration_ms: None,
    };
    hub.inject_notification(
        &session.id,
        BackboneEnvelope::new(
            BackboneEvent::ItemCompleted(ItemCompletedNotification::new(
                item_completed,
                session.id.clone(),
                "t-1".to_string(),
            )),
            session.id.clone(),
            source.clone(),
        )
        .with_trace(TraceInfo {
            turn_id: Some("t-1".to_string()),
            entry_index: Some(1),
        }),
    )
    .await
    .expect("inject tool update");

    let transcript = hub
        .build_projected_transcript(&session.id)
        .await
        .expect("transcript should build");
    let messages = transcript.into_messages();
    assert_eq!(messages.len(), 3);

    match &messages[0] {
        agentdash_spi::AgentMessage::User { content, .. } => {
            assert_eq!(content.len(), 1);
            assert_eq!(messages[0].first_text(), Some("继续分析 session 生命周期"));
            assert_ne!(messages[0].first_text(), Some("## Project\nhidden"));
        }
        other => panic!("unexpected first message: {other:?}"),
    }

    match &messages[1] {
        agentdash_spi::AgentMessage::Assistant {
            tool_calls,
            stop_reason,
            ..
        } => {
            assert_eq!(messages[1].first_text(), Some("已记录历史"));
            assert_eq!(tool_calls.len(), 1);
            assert_eq!(tool_calls[0].name, "shell_exec");
            assert_eq!(stop_reason.clone(), Some(StopReason::ToolUse));
        }
        other => panic!("unexpected assistant message: {other:?}"),
    }

    match &messages[2] {
        agentdash_spi::AgentMessage::ToolResult {
            tool_call_id,
            tool_name,
            is_error,
            ..
        } => {
            assert_eq!(tool_call_id, "tool-1");
            assert_eq!(tool_name.as_deref(), Some("shell_exec"));
            assert_eq!(messages[2].first_text(), Some("workspace root"));
            assert!(!*is_error);
        }
        other => panic!("unexpected tool result: {other:?}"),
    }
}

#[tokio::test]
async fn projected_transcript_without_head_uses_persisted_bounded_tool_result() {
    let persistence = Arc::new(MemorySessionPersistence::default());
    let hub = SessionRuntimeInner::new_with_hooks_and_persistence(
        Arc::new(SessionStartAwareConnector::default()),
        None,
        persistence,
    );
    let session = hub.create_session("test").await.expect("create session");
    let cache = SessionToolResultCache::default();
    seed_unprojected_large_tool_body(&cache, &session.id);

    hub.inject_notification(
        &session.id,
        inject_user_message_envelope(&session.id, "t-large", 0, "run large tool"),
    )
    .await
    .expect("inject user notification");
    hub.inject_notification(
        &session.id,
        bounded_large_tool_completed_envelope(&session.id, "t-large"),
    )
    .await
    .expect("inject bounded tool notification");

    let transcript = hub
        .build_projected_transcript(&session.id)
        .await
        .expect("transcript should build");
    let messages = transcript.clone().into_messages();
    let rendered = rendered_messages(&messages);

    assert!(rendered.contains("bounded preview only"));
    assert!(rendered.contains(LARGE_OUTPUT_LIFECYCLE_PATH));
    assert!(!rendered.contains(LARGE_OUTPUT_SENTINEL));

    let frame = super::super::continuation::build_continuation_context_frame(&transcript, None)
        .expect("continuation context should exist");
    assert!(frame.rendered_text.contains("bounded preview only"));
    assert!(frame.rendered_text.contains(LARGE_OUTPUT_LIFECYCLE_PATH));
    assert!(!frame.rendered_text.contains(LARGE_OUTPUT_SENTINEL));
}

fn inject_user_message_envelope(
    session_id: &str,
    turn_id: &str,
    entry_index: u32,
    text: &str,
) -> BackboneEnvelope {
    let source = SourceInfo {
        connector_id: "test".to_string(),
        connector_type: "unit".to_string(),
        executor_id: None,
    };
    build_user_input_submitted_envelope(
        session_id,
        &source,
        turn_id,
        &format!("{turn_id}:user-input:{entry_index}"),
        UserInputSubmissionKind::Prompt,
        vec![codex::UserInput::Text {
            text: text.to_string(),
            text_elements: Vec::new(),
        }],
    )
    .with_trace(TraceInfo {
        turn_id: Some(turn_id.to_string()),
        entry_index: Some(entry_index),
    })
}

fn inject_compaction_envelope(
    session_id: &str,
    turn_id: &str,
    data: serde_json::Value,
) -> BackboneEnvelope {
    let source = SourceInfo {
        connector_id: "test".to_string(),
        connector_type: "unit".to_string(),
        executor_id: None,
    };
    BackboneEnvelope::new(
        BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
            key: "context_compacted".to_string(),
            value: data,
        }),
        session_id,
        source,
    )
    .with_turn_id(turn_id)
}

fn inject_session_meta_envelope(
    session_id: &str,
    turn_id: &str,
    key: &str,
    value: serde_json::Value,
) -> BackboneEnvelope {
    let source = SourceInfo {
        connector_id: "test".to_string(),
        connector_type: "unit".to_string(),
        executor_id: None,
    };
    BackboneEnvelope::new(
        BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
            key: key.to_string(),
            value,
        }),
        session_id,
        source,
    )
    .with_turn_id(turn_id)
}

const LARGE_OUTPUT_SENTINEL: &str = "AGENTDASH_WP6_RESULT_TXT_BODY_SENTINEL";
const LARGE_OUTPUT_ITEM_ID: &str = "turn_001:tool_001";
const LARGE_OUTPUT_LIFECYCLE_PATH: &str =
    "lifecycle://session/tool-results/turn_001/tool_001/result.txt";

fn bounded_large_tool_preview() -> String {
    format!(
        "[tool result truncated]\nlifecycle_path: {LARGE_OUTPUT_LIFECYCLE_PATH}\npolicy: head_tail\n\nbounded preview only"
    )
}

fn bounded_large_tool_completed_envelope(session_id: &str, turn_id: &str) -> BackboneEnvelope {
    let source = SourceInfo {
        connector_id: "test".to_string(),
        connector_type: "unit".to_string(),
        executor_id: None,
    };
    let item = codex::ThreadItem::DynamicToolCall {
        id: LARGE_OUTPUT_ITEM_ID.to_string(),
        namespace: None,
        tool: "large_fixture_tool".to_string(),
        arguments: serde_json::json!({ "mode": "bounded" }),
        status: codex::DynamicToolCallStatus::Completed,
        content_items: Some(vec![codex::DynamicToolCallOutputContentItem::InputText {
            text: bounded_large_tool_preview(),
        }]),
        success: Some(true),
        duration_ms: None,
    };
    BackboneEnvelope::new(
        BackboneEvent::ItemCompleted(ItemCompletedNotification::new(
            item,
            session_id.to_string(),
            turn_id.to_string(),
        )),
        session_id.to_string(),
        source,
    )
    .with_trace(TraceInfo {
        turn_id: Some(turn_id.to_string()),
        entry_index: Some(1),
    })
}

fn seed_unprojected_large_tool_body(cache: &SessionToolResultCache, session_id: &str) {
    let body = format!("cache-only body prefix {LARGE_OUTPUT_SENTINEL} cache-only body suffix");
    cache.put_text(session_id, LARGE_OUTPUT_ITEM_ID, body.clone(), body.len());
}

fn rendered_messages(messages: &[agentdash_spi::AgentMessage]) -> String {
    messages
        .iter()
        .map(|message| match message {
            agentdash_spi::AgentMessage::User { content, .. }
            | agentdash_spi::AgentMessage::Assistant { content, .. }
            | agentdash_spi::AgentMessage::ToolResult { content, .. } => content
                .iter()
                .filter_map(agentdash_spi::ContentPart::extract_text)
                .collect::<Vec<_>>()
                .join(""),
            agentdash_spi::AgentMessage::CompactionSummary { summary, .. } => summary.clone(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn context_compaction_completed_envelope(
    session_id: &str,
    turn_id: &str,
    item_id: &str,
) -> BackboneEnvelope {
    let source = SourceInfo {
        connector_id: "test".to_string(),
        connector_type: "unit".to_string(),
        executor_id: None,
    };
    BackboneEnvelope::new(
        BackboneEvent::ItemCompleted(ItemCompletedNotification::new(
            codex::ThreadItem::ContextCompaction {
                id: item_id.to_string(),
            },
            session_id.to_string(),
            turn_id.to_string(),
        )),
        session_id,
        source,
    )
    .with_trace(TraceInfo {
        turn_id: Some(turn_id.to_string()),
        entry_index: None,
    })
}

async fn commit_test_compaction_projection(
    hub: &SessionRuntimeInner,
    session_id: &str,
    summary: &str,
    tokens_before: u64,
) -> CompactionProjectionCommitResult {
    let now = 1_710_000_000_000_i64;
    hub.persistence
        .commit_compaction_projection(
            session_id,
            NewCompactionProjectionCommit {
                completed_event: context_compaction_completed_envelope(
                    session_id,
                    "t-3",
                    "compact-item-1",
                ),
                compaction: SessionCompactionRecord {
                    id: "compaction-1".to_string(),
                    session_id: session_id.to_string(),
                    projection_kind: SESSION_PROJECTION_KIND_MODEL_CONTEXT.to_string(),
                    projection_version: 1,
                    lifecycle_item_id: "compact-item-1".to_string(),
                    start_event_seq: 1,
                    completed_event_seq: None,
                    failed_event_seq: None,
                    status: SessionCompactionStatus::ProjectionCommitted,
                    trigger: "auto".to_string(),
                    reason: Some("token_pressure".to_string()),
                    phase: Some("pre_provider".to_string()),
                    strategy: "summary_prefix".to_string(),
                    budget_scope: Some("model_context".to_string()),
                    base_head_event_seq: Some(3),
                    source_start_event_seq: Some(1),
                    source_end_event_seq: Some(2),
                    first_kept_event_seq: Some(3),
                    summary: summary.to_string(),
                    replacement_projection_json: serde_json::json!({
                        "segments": ["projection-segment-1"],
                        "compacted_until_ref": {
                            "turn_id": "t-2",
                            "entry_index": 0,
                        },
                    }),
                    token_stats_json: serde_json::json!({
                        "before": tokens_before,
                        "after": 12000,
                        "messages_compacted": 2,
                    }),
                    diagnostics_json: serde_json::json!({}),
                    created_by: Some("agent".to_string()),
                    created_at_ms: now,
                    completed_at_ms: None,
                },
                segments: vec![SessionProjectionSegmentRecord {
                    id: "projection-segment-1".to_string(),
                    session_id: session_id.to_string(),
                    projection_kind: SESSION_PROJECTION_KIND_MODEL_CONTEXT.to_string(),
                    projection_version: 1,
                    sort_order: 0,
                    segment_type: "summary_chunk".to_string(),
                    origin: "projection".to_string(),
                    synthetic: true,
                    source_start_event_seq: Some(1),
                    source_end_event_seq: Some(2),
                    source_refs_json: serde_json::json!({
                        "compacted_until_ref": {
                            "turn_id": "t-2",
                            "entry_index": 0,
                        },
                        "messages_compacted": 2,
                    }),
                    generated_by_compaction_id: Some("compaction-1".to_string()),
                    content_json: serde_json::json!({
                        "role": "system",
                        "content": summary
                    }),
                    token_estimate: Some(256),
                    created_at_ms: now,
                }],
                head: SessionProjectionHeadRecord {
                    session_id: session_id.to_string(),
                    projection_kind: SESSION_PROJECTION_KIND_MODEL_CONTEXT.to_string(),
                    projection_version: 1,
                    head_event_seq: 3,
                    active_compaction_id: Some("compaction-1".to_string()),
                    updated_by_event_seq: None,
                    updated_at_ms: 0,
                },
            },
        )
        .await
        .expect("commit compaction projection")
}

#[tokio::test]
async fn build_projected_transcript_applies_latest_compaction_checkpoint() {
    let persistence = Arc::new(MemorySessionPersistence::default());
    let hub = SessionRuntimeInner::new_with_hooks_and_persistence(
        Arc::new(SessionStartAwareConnector::default()),
        None,
        persistence,
    );
    let session = hub.create_session("test").await.expect("create session");

    hub.inject_notification(
        &session.id,
        inject_user_message_envelope(&session.id, "t-1", 0, "历史用户消息 1"),
    )
    .await
    .expect("inject first user notification");
    hub.inject_notification(
        &session.id,
        inject_session_meta_envelope(
            &session.id,
            "t-1",
            "session_meta_updated",
            serde_json::json!({ "source": "test" }),
        ),
    )
    .await
    .expect("inject non transcript notification");
    hub.inject_notification(
        &session.id,
        inject_user_message_envelope(&session.id, "t-2", 0, "历史用户消息 2"),
    )
    .await
    .expect("inject second user notification");
    hub.inject_notification(
        &session.id,
        inject_user_message_envelope(&session.id, "t-3", 0, "最近用户消息"),
    )
    .await
    .expect("inject kept user notification");

    hub.inject_notification(
        &session.id,
        inject_compaction_envelope(
            &session.id,
            "t-3",
            serde_json::json!({
                "summary": "## 历史摘要\n- 已完成旧分析",
                "tokens_before": 42000,
                "messages_compacted": 2,
                "newly_compacted_messages": 2,
                "compacted_until_ref": {
                    "turn_id": "t-2",
                    "entry_index": 0,
                },
                "first_kept_ref": {
                    "turn_id": "t-3",
                    "entry_index": 0,
                },
                "timestamp_ms": 1710000000000_u64,
            }),
        ),
    )
    .await
    .expect("inject compaction frame metadata");

    let events = hub
        .persistence
        .list_all_events(&session.id)
        .await
        .expect("events should load");
    let compaction_frame = events
        .iter()
        .find_map(|event| match &event.notification.event {
            BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate { key, value })
                if key == "context_frame" && value["kind"] == "compaction_summary" =>
            {
                Some(value)
            }
            _ => None,
        })
        .expect("compaction should persist a context_frame");
    assert_eq!(compaction_frame["delivery_channel"], "continuation");
    assert_eq!(
        compaction_frame["sections"][0]["messages_compacted"],
        serde_json::json!(2)
    );
    assert_eq!(
        compaction_frame["sections"][0]["projection_version"],
        serde_json::json!(1)
    );
    assert_eq!(
        compaction_frame["sections"][0]["source_end_event_seq"],
        serde_json::json!(3)
    );

    let compactions = hub
        .persistence
        .list_compactions(&session.id, SESSION_PROJECTION_KIND_MODEL_CONTEXT)
        .await
        .expect("compactions should load");
    assert_eq!(compactions.len(), 1);
    assert_eq!(
        compactions[0].status,
        SessionCompactionStatus::ProjectionCommitted
    );
    assert_eq!(compactions[0].source_end_event_seq, Some(3));

    let transcript = hub
        .build_projected_transcript(&session.id)
        .await
        .expect("transcript should build");
    assert_eq!(transcript.entries[0].origin, ProjectionOrigin::Projection);
    assert!(transcript.entries[0].synthetic);
    assert_eq!(
        transcript.entries[0]
            .source_range
            .as_ref()
            .map(|range| (range.start_event_seq, range.end_event_seq)),
        Some((1, 3))
    );
    let restored = transcript.into_messages();

    assert_eq!(restored.len(), 2);
    match &restored[0] {
        agentdash_spi::AgentMessage::CompactionSummary {
            summary,
            tokens_before,
            messages_compacted,
            compacted_until_ref,
            ..
        } => {
            assert!(summary.contains("历史摘要"));
            assert_eq!(*tokens_before, 42_000);
            assert_eq!(*messages_compacted, 2);
            assert_eq!(
                compacted_until_ref.as_ref(),
                Some(&MessageRef {
                    turn_id: "t-2".to_string(),
                    entry_index: 0,
                })
            );
        }
        other => panic!("unexpected first message: {other:?}"),
    }
    assert_eq!(restored[1].first_text(), Some("最近用户消息"));
}

#[tokio::test]
async fn continuation_context_frame_uses_compacted_projection() {
    let persistence = Arc::new(MemorySessionPersistence::default());
    let hub = SessionRuntimeInner::new_with_hooks_and_persistence(
        Arc::new(SessionStartAwareConnector::default()),
        None,
        persistence,
    );
    let session = hub.create_session("test").await.expect("create session");

    for (turn_id, entry_index, text) in [
        ("t-1", 0_u32, "第一段旧历史"),
        ("t-2", 0_u32, "第二段旧历史"),
        ("t-3", 0_u32, "保留的新历史"),
    ] {
        hub.inject_notification(
            &session.id,
            inject_user_message_envelope(&session.id, turn_id, entry_index, text),
        )
        .await
        .expect("inject user notification");
    }

    commit_test_compaction_projection(&hub, &session.id, "压缩后的历史摘要", 38_000).await;

    let transcript = hub
        .build_projected_transcript(&session.id)
        .await
        .expect("transcript should build");
    let frame = super::super::continuation::build_continuation_context_frame(&transcript, None)
        .expect("continuation context should exist");

    assert!(frame.rendered_text.contains("压缩后的历史摘要"));
    assert!(frame.rendered_text.contains("保留的新历史"));
    assert!(!frame.rendered_text.contains("第一段旧历史"));
    assert!(!frame.rendered_text.contains("第二段旧历史"));
}

#[tokio::test]
async fn projected_transcript_with_head_uses_bounded_suffix_tool_result() {
    let persistence = Arc::new(MemorySessionPersistence::default());
    let hub = SessionRuntimeInner::new_with_hooks_and_persistence(
        Arc::new(SessionStartAwareConnector::default()),
        None,
        persistence,
    );
    let session = hub.create_session("test").await.expect("create session");
    let cache = SessionToolResultCache::default();
    seed_unprojected_large_tool_body(&cache, &session.id);

    for (turn_id, entry_index, text) in [
        ("t-1", 0_u32, "第一段旧历史"),
        ("t-2", 0_u32, "第二段旧历史"),
        ("t-3", 0_u32, "保留的新历史"),
    ] {
        hub.inject_notification(
            &session.id,
            inject_user_message_envelope(&session.id, turn_id, entry_index, text),
        )
        .await
        .expect("inject user notification");
    }

    commit_test_compaction_projection(&hub, &session.id, "压缩后的历史摘要", 38_000).await;
    hub.inject_notification(
        &session.id,
        bounded_large_tool_completed_envelope(&session.id, "t-large"),
    )
    .await
    .expect("inject bounded suffix tool notification");

    let transcript = hub
        .build_projected_transcript(&session.id)
        .await
        .expect("transcript should build");
    let messages = transcript.into_messages();
    let rendered = rendered_messages(&messages);

    assert!(rendered.contains("压缩后的历史摘要"));
    assert!(rendered.contains("bounded preview only"));
    assert!(rendered.contains(LARGE_OUTPUT_LIFECYCLE_PATH));
    assert!(!rendered.contains(LARGE_OUTPUT_SENTINEL));
    assert!(!rendered.contains("第一段旧历史"));
    assert!(!rendered.contains("第二段旧历史"));
}

#[tokio::test]
async fn repository_rehydrate_restored_messages_use_bounded_tool_result() {
    let connector = Arc::new(RepositoryRestoreCapturingConnector::default());
    let base = tempfile::tempdir().expect("tempdir");
    let hub = test_hub(base.path().to_path_buf(), connector.clone(), None)
        .with_lifecycle_agent_repo(Arc::new(MemoryLifecycleAgentRepository::default()));
    let session = hub.create_session("test").await.expect("create session");
    attach_test_lifecycle_frame(&hub, &session.id).await;
    let cache = SessionToolResultCache::default();
    seed_unprojected_large_tool_body(&cache, &session.id);

    hub.inject_notification(
        &session.id,
        inject_user_message_envelope(&session.id, "t-large", 0, "run large tool"),
    )
    .await
    .expect("inject user notification");
    hub.inject_notification(
        &session.id,
        bounded_large_tool_completed_envelope(&session.id, "t-large"),
    )
    .await
    .expect("inject bounded tool notification");

    hub.start_prompt(&session.id, simple_prompt_request("continue"))
        .await
        .expect("prompt should start");

    let restored = connector.restored_messages.lock().await.clone();
    let rendered = rendered_messages(&restored);

    assert!(!restored.is_empty());
    assert!(rendered.contains("bounded preview only"));
    assert!(rendered.contains(LARGE_OUTPUT_LIFECYCLE_PATH));
    assert!(!rendered.contains(LARGE_OUTPUT_SENTINEL));
}

#[tokio::test]
async fn compaction_projection_context_token_estimate_includes_suffix_messages() {
    let persistence = Arc::new(MemorySessionPersistence::default());
    let hub = SessionRuntimeInner::new_with_hooks_and_persistence(
        Arc::new(SessionStartAwareConnector::default()),
        None,
        persistence,
    );
    let session = hub.create_session("test").await.expect("create session");

    for (turn_id, entry_index, text) in [
        ("t-1", 0_u32, "第一段旧历史"),
        ("t-2", 0_u32, "第二段旧历史"),
        ("t-3", 0_u32, "保留的新历史会继续进入模型上下文"),
    ] {
        hub.inject_notification(
            &session.id,
            inject_user_message_envelope(&session.id, turn_id, entry_index, text),
        )
        .await
        .expect("inject user notification");
    }

    let result =
        commit_test_compaction_projection(&hub, &session.id, "压缩后的历史摘要", 38_000).await;
    assert_eq!(result.head.head_event_seq, result.event.event_seq);
    assert_eq!(
        result.head.updated_by_event_seq,
        Some(result.event.event_seq)
    );

    let envelope = hub
        .eventing_service()
        .build_agent_context_envelope(&session.id)
        .await
        .expect("context envelope should build");
    assert!(
        envelope
            .token_estimate
            .expect("token estimate should exist")
            > 256,
        "token estimate should include the summary segment and kept suffix messages"
    );
}

#[tokio::test]
async fn connector_setup_failure_leaves_current_frame_unchanged() {
    let base = tempfile::tempdir().expect("tempdir");
    let hub = test_hub(
        base.path().to_path_buf(),
        Arc::new(SetupFailingConnector),
        None,
    )
    .with_lifecycle_agent_repo(Arc::new(MemoryLifecycleAgentRepository::default()));
    let session = hub.create_session("frame-failure").await.expect("create");
    let launch_frame = attach_test_lifecycle_frame(&hub, &session.id).await;

    let error = hub
        .start_prompt(&session.id, simple_prompt_request("hello"))
        .await
        .expect_err("connector setup should fail");
    assert!(error.to_string().contains("connector setup failed"));

    assert_eq!(
        current_frame_id(&hub, launch_frame.agent_id).await,
        Some(launch_frame.id),
        "connector accepted 前失败不能推进 current_frame_id"
    );
    let frames = hub
        .agent_frame_repo
        .as_ref()
        .expect("test hub should provide frame repo")
        .list_by_agent(launch_frame.agent_id)
        .await
        .expect("frames should load");
    assert_eq!(
        frames.len(),
        1,
        "connector accepted 前失败不能写入新的 AgentFrame revision"
    );
}

#[tokio::test]
async fn accepted_turn_commits_agent_frame_revision_and_current_frame() {
    let base = tempfile::tempdir().expect("tempdir");
    let hub = test_hub(base.path().to_path_buf(), Arc::new(EmptyConnector), None)
        .with_lifecycle_agent_repo(Arc::new(MemoryLifecycleAgentRepository::default()));
    let session = hub.create_session("frame-success").await.expect("create");
    let launch_frame = attach_test_lifecycle_frame(&hub, &session.id).await;

    hub.start_prompt(&session.id, simple_prompt_request("hello"))
        .await
        .expect("connector accepted turn should commit");

    let current_id = current_frame_id(&hub, launch_frame.agent_id)
        .await
        .expect("current frame should exist");
    assert_ne!(
        current_id, launch_frame.id,
        "accepted 后应推进到新的 AgentFrame revision"
    );
    let frames = hub
        .agent_frame_repo
        .as_ref()
        .expect("test hub should provide frame repo")
        .list_by_agent(launch_frame.agent_id)
        .await
        .expect("frames should load");
    assert_eq!(frames.len(), 2);
    let committed_frame = frames
        .iter()
        .find(|frame| frame.id == current_id)
        .expect("current frame should be persisted");
    assert_eq!(committed_frame.revision, launch_frame.revision + 1);
}

#[tokio::test]
async fn accepted_turn_commits_hook_runtime_target_to_new_frame() {
    let frame_repo = Arc::new(MemoryAgentFrameRepository::default());
    let agent_repo = Arc::new(MemoryLifecycleAgentRepository::default());
    let anchor_repo = Arc::new(MemoryRuntimeSessionExecutionAnchorRepository::default());
    let hook_provider = Arc::new(CurrentFrameHookProvider);
    let hub = SessionRuntimeInner::new_with_hooks_and_persistence(
        Arc::new(EmptyConnector),
        Some(hook_provider),
        Arc::new(MemorySessionPersistence::default()),
    )
    .with_agent_frame_repo(frame_repo)
    .with_lifecycle_agent_repo(agent_repo)
    .with_execution_anchor_repo(anchor_repo)
    .with_lifecycle_gate_repo(Arc::new(MemoryLifecycleGateRepository::default()));
    let session = hub
        .create_session("frame-hook-target-sync")
        .await
        .expect("create");
    let launch_frame = attach_test_lifecycle_frame(&hub, &session.id).await;

    let initial_runtime = hub
        .hook_service()
        .ensure_hook_runtime_for_target(
            &AgentFrameRuntimeTarget {
                frame_id: launch_frame.id,
                delivery_runtime_session_id: session.id.clone(),
            },
            Some("turn-before"),
        )
        .await
        .expect("initial hook runtime should load")
        .expect("initial hook runtime should exist");
    assert_eq!(initial_runtime.control_target().frame_id, launch_frame.id);

    hub.start_prompt(&session.id, simple_prompt_request("hello"))
        .await
        .expect("connector accepted turn should commit");
    let current_id = current_frame_id(&hub, launch_frame.agent_id)
        .await
        .expect("current frame should exist");

    let runtime = hub
        .hook_service()
        .get_hook_runtime_for_target(&AgentFrameRuntimeTarget {
            frame_id: current_id,
            delivery_runtime_session_id: session.id.clone(),
        })
        .await
        .expect("current target hook runtime lookup should succeed")
        .expect("hook runtime should exist for current frame");
    assert_eq!(runtime.control_target().frame_id, current_id);
    assert_ne!(
        runtime.control_target().frame_id,
        launch_frame.id,
        "accepted frame 推进后 hook runtime target 应同步到新 frame"
    );
}

#[tokio::test]
async fn planner_invalid_config_leaves_current_frame_unchanged() {
    struct StaticConstructionProvider {
        hub: SessionRuntimeInner,
    }

    #[async_trait::async_trait]
    impl FrameLaunchEnvelopePort for StaticConstructionProvider {
        async fn build_launch_envelope(
            &self,
            input: FrameLaunchEnvelopeRequest,
        ) -> Result<FrameLaunchEnvelope, ConnectorError> {
            let prompt_text = input
                .command
                .user_input
                .input
                .as_ref()
                .and_then(|blocks| blocks.first())
                .and_then(agentdash_agent_protocol::user_input_text)
                .unwrap_or("hello");
            let mut construction = simple_prompt_request(prompt_text);
            construction.session_id = input.runtime_session_id.clone();
            construction.session.session_id = input.runtime_session_id;
            super::facade::envelope_from_construction(&self.hub, construction).await
        }
    }

    let base = tempfile::tempdir().expect("tempdir");
    let hub = test_hub(base.path().to_path_buf(), Arc::new(EmptyConnector), None)
        .with_lifecycle_agent_repo(Arc::new(MemoryLifecycleAgentRepository::default()));
    let session = hub.create_session("invalid-config").await.expect("create");
    let launch_frame = attach_test_lifecycle_frame(&hub, &session.id).await;
    hub.set_frame_launch_envelope_provider(Arc::new(StaticConstructionProvider {
        hub: hub.clone(),
    }))
    .await;
    let mut input = UserPromptInput::from_text("hello");
    input.executor_config = Some(agentdash_spi::AgentConfig::new("PI_AGENT"));
    input.backend_selection = Some(BackendSelectionInput {
        mode: BackendSelectionInputMode::Explicit,
        backend_id: Some("backend-1".to_string()),
    });

    let error = hub
        .launch_service()
        .launch_command(
            &session.id,
            super::super::launch::LaunchCommand::http_prompt_input(input, None),
        )
        .await
        .expect_err("backend selection without placement deps should fail");
    assert!(
        error.to_string().contains("backend selection 已指定"),
        "unexpected error: {error}"
    );

    assert_eq!(
        current_frame_id(&hub, launch_frame.agent_id).await,
        Some(launch_frame.id),
        "planner InvalidConfig 不能推进 current_frame_id"
    );
    let history = hub
        .persistence
        .list_all_events(&session.id)
        .await
        .expect("history should load");
    assert!(
        !history
            .iter()
            .any(|event| matches!(event.notification.event, BackboneEvent::TurnStarted(_))),
        "planner InvalidConfig 不能写入 turn_started"
    );
    assert!(
        !history.iter().any(|event| matches!(
            &event.notification.event,
            BackboneEvent::UserInputSubmitted(_)
        )),
        "planner InvalidConfig 不能写入 user input"
    );
}

#[tokio::test]
async fn missing_launch_vfs_rejects_before_connector_prompt() {
    let base = tempfile::tempdir().expect("tempdir");
    let prompt_calls = Arc::new(AtomicUsize::new(0));
    let hub = test_hub(
        base.path().to_path_buf(),
        Arc::new(PromptCountingConnector {
            prompt_calls: prompt_calls.clone(),
        }),
        None,
    );
    let session = hub
        .create_session("missing-launch-vfs")
        .await
        .expect("create");
    let mut request = simple_prompt_request("hello");
    request
        .projections
        .frame_surface_draft
        .as_mut()
        .expect("test request should have frame surface")
        .vfs = None;

    let error = hub
        .start_prompt(&session.id, request)
        .await
        .expect_err("missing VFS should reject before connector prompt");

    assert!(
        error.to_string().contains("vfs"),
        "unexpected error: {error}"
    );
    assert_eq!(prompt_calls.load(Ordering::SeqCst), 0);
    assert_no_connector_accepted_events(&hub, &session.id).await;
}

#[tokio::test]
async fn mismatched_launch_mcp_rejects_before_connector_prompt() {
    let base = tempfile::tempdir().expect("tempdir");
    let prompt_calls = Arc::new(AtomicUsize::new(0));
    let hub = test_hub(
        base.path().to_path_buf(),
        Arc::new(PromptCountingConnector {
            prompt_calls: prompt_calls.clone(),
        }),
        None,
    );
    let session = hub
        .create_session("mismatched-launch-mcp")
        .await
        .expect("create");
    let mut request = simple_prompt_request("hello");
    let draft_only_mcp = agentdash_spi::RuntimeMcpServer {
        name: "draft-only".to_string(),
        transport: agentdash_spi::McpTransportConfig::Http {
            url: "http://localhost/mcp".to_string(),
            headers: vec![],
        },
        uses_relay: false,
    };
    let draft = request
        .projections
        .frame_surface_draft
        .as_mut()
        .expect("test request should have frame surface");
    draft.mcp_servers = vec![draft_only_mcp];

    let error = hub
        .start_prompt(&session.id, request)
        .await
        .expect_err("mismatched MCP surface should reject before connector prompt");

    assert!(
        error
            .to_string()
            .contains("capability_state.tool.mcp_servers"),
        "unexpected error: {error}"
    );
    assert_eq!(prompt_calls.load(Ordering::SeqCst), 0);
    assert_no_connector_accepted_events(&hub, &session.id).await;
}

async fn assert_no_connector_accepted_events(hub: &SessionRuntimeInner, session_id: &str) {
    let history = hub
        .persistence
        .list_all_events(session_id)
        .await
        .expect("history should load");
    assert!(
        !history
            .iter()
            .any(|event| matches!(event.notification.event, BackboneEvent::TurnStarted(_))),
        "launch surface rejection must not write turn_started"
    );
    assert!(
        !history.iter().any(|event| matches!(
            &event.notification.event,
            BackboneEvent::UserInputSubmitted(_)
        )),
        "launch surface rejection must not write user input"
    );
}

#[tokio::test]
async fn start_prompt_releases_claim_when_session_meta_is_missing() {
    let base = tempfile::tempdir().expect("tempdir");
    let hub = test_hub(base.path().to_path_buf(), Arc::new(EmptyConnector), None);
    let missing_session_id = "missing-session";

    let error = hub
        .start_prompt(missing_session_id, simple_prompt_request("hello"))
        .await
        .expect_err("missing session should fail before connector prompt");
    assert!(error.to_string().contains("不存在"));

    let is_running = hub
        .runtime_registry
        .with_runtime(missing_session_id, |runtime| {
            runtime.is_some_and(|runtime| runtime.turn_state.is_running())
        })
        .await;
    assert!(!is_running, "missing session claim should be released");
}

// ─────────────────────────────────────────────────────────────────────
// Fail-lock: auto-resume 必须经过 launch envelope port
// ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn launch_prompt_strict_requires_frame_launch_envelope_provider() {
    let base = tempfile::tempdir().expect("tempdir");
    let hub = test_hub(base.path().to_path_buf(), Arc::new(EmptyConnector), None);
    let session = hub.create_session("strict-launch").await.expect("create");

    let error = hub
        .launch_service()
        .launch_command(
            &session.id,
            super::super::launch::LaunchCommand::http_prompt_input(
                UserPromptInput::from_text("hello"),
                None,
            ),
        )
        .await
        .expect_err("strict launch 应在 provider 缺失时失败");

    match error {
        ConnectorError::Runtime(message) => {
            assert!(
                message.contains("session_launch_envelope_provider 未注入"),
                "错误信息应提示 provider 缺失，实际为: {message}"
            );
        }
        other => panic!("期望 Runtime 错误，实际为: {other}"),
    }
}

#[tokio::test]
async fn local_relay_prompt_requires_frame_launch_envelope_provider() {
    let base = tempfile::tempdir().expect("tempdir");
    let workspace = tempfile::tempdir().expect("workspace");
    let hub = test_hub(base.path().to_path_buf(), Arc::new(EmptyConnector), None);
    let session = hub.create_session("relaxed-launch").await.expect("create");

    let error = hub
        .launch_service()
        .launch_command(
            &session.id,
            super::super::launch::LaunchCommand::local_relay_prompt_input(
                UserPromptInput::from_text("hello"),
                Vec::new(),
                workspace.path().to_path_buf(),
            ),
        )
        .await
        .expect_err("local relay prompt 应在 provider 缺失时失败");

    match error {
        ConnectorError::Runtime(message) => {
            assert!(
                message.contains("session_launch_envelope_provider 未注入")
                    && message.contains("local_relay_prompt"),
                "错误信息应提示 local relay prompt 被拒绝，实际为: {message}"
            );
        }
        other => panic!("期望 Runtime 错误，实际为: {other}"),
    }
}

#[tokio::test]
async fn schedule_unanchored_hook_auto_resume_strict_mode_requires_provider() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct PromptCountingConnector {
        prompt_calls: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl AgentConnector for PromptCountingConnector {
        fn connector_id(&self) -> &'static str {
            "prompt-counting"
        }
        fn connector_type(&self) -> agentdash_spi::ConnectorType {
            agentdash_spi::ConnectorType::LocalExecutor
        }
        fn capabilities(&self) -> agentdash_spi::ConnectorCapabilities {
            agentdash_spi::ConnectorCapabilities::default()
        }
        fn list_executors(&self) -> Vec<agentdash_spi::AgentInfo> {
            Vec::new()
        }
        async fn discover_options_stream(
            &self,
            _: &str,
            _: Option<PathBuf>,
        ) -> Result<futures::stream::BoxStream<'static, json_patch::Patch>, ConnectorError>
        {
            Ok(Box::pin(stream::empty()))
        }
        async fn prompt(
            &self,
            _: &str,
            _: Option<&str>,
            _: &PromptPayload,
            _: agentdash_spi::ExecutionContext,
        ) -> Result<agentdash_spi::ExecutionStream, ConnectorError> {
            self.prompt_calls.fetch_add(1, Ordering::SeqCst);
            Ok(Box::pin(stream::empty()))
        }
        async fn cancel(&self, _: &str) -> Result<(), ConnectorError> {
            Ok(())
        }
        async fn approve_tool_call(&self, _: &str, _: &str) -> Result<(), ConnectorError> {
            Ok(())
        }
        async fn reject_tool_call(
            &self,
            _: &str,
            _: &str,
            _: Option<String>,
        ) -> Result<(), ConnectorError> {
            Ok(())
        }
    }

    let base = tempfile::tempdir().expect("tempdir");
    let prompt_calls = Arc::new(AtomicUsize::new(0));
    let hub = test_hub(
        base.path().to_path_buf(),
        Arc::new(PromptCountingConnector {
            prompt_calls: prompt_calls.clone(),
        }),
        None,
    );
    let session = hub
        .create_session("strict-auto-resume")
        .await
        .expect("create");

    // 不注入 provider，strict auto-resume 应该在 launch 前失败，不能触发 connector.prompt。
    hub.schedule_unanchored_hook_auto_resume(session.id.clone());
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    assert_eq!(
        prompt_calls.load(Ordering::SeqCst),
        0,
        "strict auto-resume 在 provider 缺失时不应触发 connector.prompt"
    );
}

#[tokio::test]
async fn schedule_unanchored_hook_auto_resume_routes_through_provider() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct SpyConstructionProvider {
        calls: Arc<AtomicUsize>,
        captured_prompt: Arc<TokioMutex<Option<String>>>,
        captured_mcp_len: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl FrameLaunchEnvelopePort for SpyConstructionProvider {
        async fn build_launch_envelope(
            &self,
            input: FrameLaunchEnvelopeRequest,
        ) -> Result<FrameLaunchEnvelope, ConnectorError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let text = input
                .command
                .user_input
                .input
                .as_ref()
                .and_then(|blocks| blocks.first())
                .and_then(agentdash_agent_protocol::user_input_text)
                .map(ToString::to_string);
            *self.captured_prompt.lock().await = text;
            self.captured_mcp_len.store(
                input
                    .command
                    .modifiers
                    .iter()
                    .find_map(|modifier| match modifier {
                        FrameLaunchModifier::LocalRelay(payload) => Some(payload),
                        _ => None,
                    })
                    .map(|payload| payload.mcp_servers.len())
                    .unwrap_or_default(),
                Ordering::SeqCst,
            );
            Err(ConnectorError::InvalidConfig(
                "spy provider stops here".to_string(),
            ))
        }
    }

    struct NoopConnector;

    #[async_trait::async_trait]
    impl AgentConnector for NoopConnector {
        fn connector_id(&self) -> &'static str {
            "noop"
        }
        fn connector_type(&self) -> agentdash_spi::ConnectorType {
            agentdash_spi::ConnectorType::LocalExecutor
        }
        fn capabilities(&self) -> agentdash_spi::ConnectorCapabilities {
            agentdash_spi::ConnectorCapabilities::default()
        }
        fn list_executors(&self) -> Vec<agentdash_spi::AgentInfo> {
            Vec::new()
        }
        async fn discover_options_stream(
            &self,
            _: &str,
            _: Option<PathBuf>,
        ) -> Result<futures::stream::BoxStream<'static, json_patch::Patch>, ConnectorError>
        {
            Ok(Box::pin(stream::empty()))
        }
        async fn prompt(
            &self,
            _: &str,
            _: Option<&str>,
            _: &PromptPayload,
            _: agentdash_spi::ExecutionContext,
        ) -> Result<agentdash_spi::ExecutionStream, ConnectorError> {
            Err(ConnectorError::Runtime(
                "connector should not be reached if provider stopped".to_string(),
            ))
        }
        async fn cancel(&self, _: &str) -> Result<(), ConnectorError> {
            Ok(())
        }
        async fn approve_tool_call(&self, _: &str, _: &str) -> Result<(), ConnectorError> {
            Ok(())
        }
        async fn reject_tool_call(
            &self,
            _: &str,
            _: &str,
            _: Option<String>,
        ) -> Result<(), ConnectorError> {
            Ok(())
        }
    }

    let base = tempfile::tempdir().expect("tempdir");
    let hub = test_hub(base.path().to_path_buf(), Arc::new(NoopConnector), None);
    let session = hub.create_session("test").await.expect("create session");

    let calls = Arc::new(AtomicUsize::new(0));
    let captured_prompt = Arc::new(TokioMutex::new(None));
    let captured_mcp_len = Arc::new(AtomicUsize::new(usize::MAX));
    hub.set_frame_launch_envelope_provider(Arc::new(SpyConstructionProvider {
        calls: calls.clone(),
        captured_prompt: captured_prompt.clone(),
        captured_mcp_len: captured_mcp_len.clone(),
    }))
    .await;

    hub.schedule_unanchored_hook_auto_resume(session.id.clone());

    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1500);
    while calls.load(Ordering::SeqCst) == 0 && std::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    }

    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let prompt_text = captured_prompt.lock().await.clone();
    let expected = msg::AUTO_RESUME_PROMPT.to_string();
    assert_eq!(prompt_text.as_deref(), Some(expected.as_str()));
    assert_eq!(captured_mcp_len.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn auto_resume_prompt_does_not_induce_recap() {
    let prompt = msg::AUTO_RESUME_PROMPT;
    let recap_triggers = ["上一轮执行结束", "请总结", "请回顾", "请汇报"];
    for trigger in recap_triggers {
        assert!(
            !prompt.contains(trigger),
            "AUTO_RESUME_PROMPT 出现了 recap 触发词 `{trigger}`：\n{prompt}"
        );
    }
}

#[tokio::test]
async fn request_hook_auto_resume_enforces_cap() {
    let base = tempfile::tempdir().expect("tempdir");
    let hub = test_hub(base.path().to_path_buf(), Arc::new(EmptyConnector), None);
    let session = hub.create_session("auto-resume-cap").await.expect("create");
    let _rx = hub.ensure_session(&session.id).await;

    assert!(
        hub.request_hook_auto_resume(hook_auto_resume_request(&session.id))
            .await
            .expect("auto-resume should route")
    );
    assert!(
        hub.request_hook_auto_resume(hook_auto_resume_request(&session.id))
            .await
            .expect("auto-resume should route")
    );
    assert!(
        !hub.request_hook_auto_resume(hook_auto_resume_request(&session.id))
            .await
            .expect("auto-resume cap check should not fail")
    );
    assert!(
        !hub.request_hook_auto_resume(hook_auto_resume_request(&session.id))
            .await
            .expect("auto-resume cap check should not fail")
    );

    let auto_resume_count = hub
        .runtime_registry
        .with_runtime(&session.id, |runtime| {
            runtime
                .map(|runtime| runtime.hook_auto_resume_count)
                .expect("session runtime should exist")
        })
        .await;
    assert_eq!(auto_resume_count, 2);
}

#[tokio::test]
async fn request_hook_auto_resume_returns_false_for_unknown_session() {
    let base = tempfile::tempdir().expect("tempdir");
    let hub = test_hub(base.path().to_path_buf(), Arc::new(EmptyConnector), None);

    assert!(
        !hub.request_hook_auto_resume(hook_auto_resume_request("nonexistent"))
            .await
            .expect("unknown session should be skipped, not failed"),
    );
}

fn hook_auto_resume_request(
    session_id: &str,
) -> super::super::terminal_effects::TerminalAutoResumeRequest {
    super::super::terminal_effects::TerminalAutoResumeRequest {
        effect_id: uuid::Uuid::new_v4(),
        session_id: session_id.to_string(),
        turn_id: "turn-auto-resume-test".to_string(),
        terminal_event_seq: 1,
        payload: serde_json::json!({}),
    }
}
