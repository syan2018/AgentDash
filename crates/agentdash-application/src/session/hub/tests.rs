//! `SessionHub` 行为测试（从原 `hub.rs` 迁移；PR 6 拆分）。

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use agentdash_agent_protocol::codex_app_server_protocol as codex;
use agentdash_agent_protocol::{
    BackboneEnvelope, BackboneEvent, PlatformEvent, SourceInfo, TraceInfo,
};
use agentdash_agent_protocol::{ContentBlock, TextContent};
use agentdash_spi::hooks::{
    ContextFrame, ContextFrameSection, ExecutionHookProvider, HookEvaluationQuery, HookInjection,
    HookResolution, HookTraceTrigger, HookTrigger, RuntimeEventSource, SessionHookRefreshQuery,
    SessionHookSnapshot, SessionHookSnapshotQuery,
};
use agentdash_spi::{
    AgentConfig, AgentConnector, CapabilityState, ConnectorError, ExecutionSessionFrame,
    PromptPayload, StopReason,
};
use futures::stream;
use serde_json::json;
use tokio::sync::{Mutex as TokioMutex, mpsc};
use tokio_stream::wrappers::ReceiverStream;

use super::super::MemorySessionPersistence;
use super::super::RuntimeCommandStatus;
use super::super::hook_messages as msg;
use super::super::hub_support::{
    TurnExecution, TurnState, build_user_message_envelopes, parse_turn_terminal_event_from_envelope,
};
use super::super::local_workspace_vfs;
use super::super::types::{
    HookSnapshotReloadTrigger, PendingCapabilityStateTransition, SessionBootstrapState,
    SessionExecutionState, SessionLaunchPlan, UserPromptInput,
};
use super::SessionHub;

fn test_hub(
    mount_root: PathBuf,
    connector: Arc<dyn AgentConnector>,
    hook_provider: Option<Arc<dyn ExecutionHookProvider>>,
) -> SessionHub {
    SessionHub::new_with_hooks_and_persistence(
        Some(local_workspace_vfs(&mount_root)),
        connector,
        hook_provider,
        Arc::new(MemorySessionPersistence::default()),
    )
}

fn simple_prompt_request(prompt: &str) -> SessionLaunchPlan {
    let mut plan = SessionLaunchPlan::from_user_input(UserPromptInput {
        executor_config: Some(agentdash_spi::AgentConfig::new("PI_AGENT")),
        ..UserPromptInput::from_text(prompt)
    });
    plan.hook_snapshot_reload = HookSnapshotReloadTrigger::None;
    plan
}

fn owner_bootstrap_request(prompt: &str, system_context: &str) -> SessionLaunchPlan {
    let mut req = simple_prompt_request(prompt);
    let bundle_session_id = uuid::Uuid::new_v4();
    req.context_bundle = Some(crate::context::build_continuation_bundle_from_markdown(
        bundle_session_id,
        system_context.to_string(),
    ));
    req.hook_snapshot_reload = HookSnapshotReloadTrigger::Reload;
    req
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

#[derive(Clone)]
struct StaticRelayMcpProvider {
    tools: Vec<agentdash_spi::RelayMcpToolInfo>,
}

#[async_trait::async_trait]
impl agentdash_spi::McpRelayProvider for StaticRelayMcpProvider {
    async fn list_relay_tools(
        &self,
        _requested_servers: &[String],
    ) -> Vec<agentdash_spi::RelayMcpToolInfo> {
        self.tools.clone()
    }

    async fn call_relay_tool(
        &self,
        _server_name: &str,
        _tool_name: &str,
        _arguments: Option<serde_json::Map<String, serde_json::Value>>,
        _context: Option<agentdash_spi::RelayMcpCallContext>,
    ) -> Result<agentdash_spi::RelayMcpCallResult, ConnectorError> {
        Ok(agentdash_spi::RelayMcpCallResult {
            content: String::new(),
            is_error: false,
        })
    }

    async fn probe_transport(
        &self,
        _transport: &agentdash_domain::mcp_preset::McpTransportConfig,
    ) -> Result<agentdash_spi::platform::mcp_relay::RelayProbeResult, ConnectorError> {
        Ok(agentdash_spi::platform::mcp_relay::RelayProbeResult {
            status: "ok".to_string(),
            latency_ms: None,
            tools: None,
            error: None,
        })
    }
}

#[tokio::test]
async fn start_prompt_records_current_turn_state() {
    let base = tempfile::tempdir().expect("tempdir");
    let workspace = tempfile::tempdir().expect("workspace");
    let hub = test_hub(base.path().to_path_buf(), Arc::new(EmptyConnector), None);
    let session = hub.create_session("active-state").await.expect("create");
    let session_mcp = agentdash_spi::SessionMcpServer {
        name: "relay_tools".to_string(),
        transport: agentdash_spi::McpTransportConfig::Http {
            url: "http://127.0.0.1:19090/mcp".to_string(),
            headers: vec![],
        },
        uses_relay: true,
    };
    let flow_caps =
        agentdash_spi::CapabilityState::from_clusters([agentdash_spi::ToolCluster::Workflow]);

    let mut req = simple_prompt_request("hello");
    req.vfs = Some(local_workspace_vfs(workspace.path()));
    req.user_input.working_dir = Some("src".to_string());
    req.mcp_servers = vec![session_mcp.clone()];
    req.capability_state = Some(flow_caps.clone());

    hub.start_prompt(&session.id, req)
        .await
        .expect("prompt should start");

    let turn = hub
        .runtime_registry
        .with_runtime(&session.id, |runtime| {
            runtime.and_then(|runtime| runtime.turn_state.active_turn().cloned())
        })
        .await
        .expect("current turn execution state");
    assert_eq!(turn.session_frame.mcp_servers.len(), 1);
    assert_eq!(turn.session_frame.mcp_servers[0].name, "relay_tools");
    assert!(turn.session_frame.mcp_servers[0].uses_relay);
    assert_eq!(
        turn.session_frame.working_directory,
        workspace.path().join("src")
    );
    assert_eq!(turn.session_frame.executor_config.executor, "PI_AGENT");
    assert_eq!(
        turn.capability_state.tool.enabled_clusters,
        flow_caps.tool.enabled_clusters
    );
}

#[tokio::test]
async fn build_tools_filters_relay_mcp_with_initial_capability_state() {
    let base = tempfile::tempdir().expect("tempdir");
    let workflow_server = agentdash_spi::SessionMcpServer {
        name: "agentdash-workflow-tools-123".to_string(),
        transport: agentdash_spi::McpTransportConfig::Http {
            url: "http://relay/ignored".to_string(),
            headers: vec![],
        },
        uses_relay: true,
    };
    let relay = Arc::new(StaticRelayMcpProvider {
        tools: vec![
            agentdash_spi::RelayMcpToolInfo {
                server_name: workflow_server.name.clone(),
                tool_name: "list_workflows".to_string(),
                description: "list".to_string(),
                parameters_schema: json!({ "type": "object" }),
            },
            agentdash_spi::RelayMcpToolInfo {
                server_name: workflow_server.name.clone(),
                tool_name: "upsert_workflow_tool".to_string(),
                description: "upsert".to_string(),
                parameters_schema: json!({ "type": "object" }),
            },
            agentdash_spi::RelayMcpToolInfo {
                server_name: workflow_server.name.clone(),
                tool_name: "upsert_lifecycle_tool".to_string(),
                description: "upsert lifecycle".to_string(),
                parameters_schema: json!({ "type": "object" }),
            },
        ],
    });
    let hub = test_hub(base.path().to_path_buf(), Arc::new(EmptyConnector), None)
        .with_mcp_relay_provider(relay);

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
            identity: None,
        },
        turn: agentdash_spi::ExecutionTurnFrame {
            capability_state: plan_state,
            ..Default::default()
        },
    };

    let plan_tools = hub
        .build_tools_for_execution_context(
            "session-initial-tools",
            &plan_context,
            std::slice::from_ref(&workflow_server),
        )
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
        .build_tools_for_execution_context(
            "session-initial-tools",
            &apply_context,
            &[workflow_server],
        )
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

#[tokio::test]
async fn replace_current_capability_state_updates_active_turn_capability_state() {
    let base = tempfile::tempdir().expect("tempdir");
    let workspace = tempfile::tempdir().expect("workspace");
    let hub = test_hub(base.path().to_path_buf(), Arc::new(PendingConnector), None);
    let session = hub
        .create_session("capability-surface")
        .await
        .expect("create");

    let initial_flow =
        agentdash_spi::CapabilityState::from_clusters([agentdash_spi::ToolCluster::Read]);
    let mut req = simple_prompt_request("hello");
    req.vfs = Some(local_workspace_vfs(workspace.path()));
    req.capability_state = Some(initial_flow);

    hub.start_prompt(&session.id, req)
        .await
        .expect("prompt should start");

    let mut target_flow =
        agentdash_spi::CapabilityState::from_clusters([agentdash_spi::ToolCluster::Write]);
    target_flow
        .tool
        .tool_policy
        .entry("file_write".to_string())
        .or_default()
        .exclude
        .insert("fs_apply_patch".to_string());
    let target_mcp = agentdash_spi::SessionMcpServer {
        name: "phase_tools".to_string(),
        transport: agentdash_spi::McpTransportConfig::Http {
            url: "http://127.0.0.1:19091/mcp".to_string(),
            headers: vec![],
        },
        uses_relay: false,
    };
    let target_vfs = agentdash_spi::Vfs {
        mounts: vec![agentdash_domain::common::Mount {
            id: "phase".to_string(),
            provider: "inline_fs".to_string(),
            backend_id: "test-backend".to_string(),
            root_ref: "phase-root".to_string(),
            capabilities: vec![agentdash_domain::common::MountCapability::Read],
            default_write: false,
            display_name: "Phase Mount".to_string(),
            metadata: serde_json::json!({ "phase": true }),
        }],
        default_mount_id: Some("phase".to_string()),
        source_project_id: None,
        source_story_id: None,
        links: Vec::new(),
    };

    let mut target_state = target_flow.clone();
    target_state.tool.mcp_servers = vec![target_mcp.clone()];
    target_state.vfs.active = Some(target_vfs.clone());

    hub.replace_current_capability_state(&session.id, target_state.clone())
        .await
        .expect("replace capability state");

    let (turn, profile) = hub
        .runtime_registry
        .with_runtime(&session.id, |runtime| {
            let runtime = runtime?;
            Some((
                runtime.turn_state.active_turn().cloned()?,
                runtime.session_profile.clone()?,
            ))
        })
        .await
        .expect("current turn execution state");
    assert_eq!(turn.capability_state, target_state);
    assert_eq!(turn.session_frame.mcp_servers, vec![target_mcp]);
    assert_eq!(turn.session_frame.vfs, Some(target_vfs));
    assert_eq!(profile.capability_state, turn.capability_state);
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
    );
    let session = hub
        .create_session("pending-capability-surface")
        .await
        .expect("create");

    let mut target_flow =
        agentdash_spi::CapabilityState::from_clusters([agentdash_spi::ToolCluster::Write]);
    target_flow
        .tool
        .capabilities
        .insert(agentdash_spi::ToolCapability::new("file_write"));
    let target_mcp = agentdash_spi::SessionMcpServer {
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
    hub.enqueue_pending_capability_state_transition(
        &session.id,
        PendingCapabilityStateTransition {
            id: "transition-1".to_string(),
            run_id: uuid::Uuid::new_v4(),
            lifecycle_key: "dev".to_string(),
            phase_node: "review".to_string(),
            capability_keys: std::collections::BTreeSet::from(["file_write".to_string()]),
            state: {
                target_flow.tool.mcp_servers = vec![target_mcp];
                target_flow.vfs.active = Some(pending_vfs);
                target_flow
            },
            created_at: 1,
            source_turn_id: None,
        },
    )
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
    assert_eq!(applied_commands[0].transition_id, "transition-1");

    let events = hub
        .persistence
        .list_all_events(&session.id)
        .await
        .expect("events should load");
    assert!(events.iter().any(|event| {
        matches!(
            &event.notification.event,
            BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate { key, value })
                if key == "capability_state_changed"
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
        let seen = context.turn.hook_session.as_ref().is_some_and(|runtime| {
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
    async fn load_session_snapshot(
        &self,
        query: SessionHookSnapshotQuery,
    ) -> Result<SessionHookSnapshot, agentdash_spi::hooks::HookError> {
        Ok(SessionHookSnapshot {
            session_id: query.session_id,
            ..SessionHookSnapshot::default()
        })
    }
    async fn refresh_session_snapshot(
        &self,
        query: SessionHookRefreshQuery,
    ) -> Result<SessionHookSnapshot, agentdash_spi::hooks::HookError> {
        Ok(SessionHookSnapshot {
            session_id: query.session_id,
            ..SessionHookSnapshot::default()
        })
    }
    async fn evaluate_hook(
        &self,
        query: HookEvaluationQuery,
    ) -> Result<HookResolution, agentdash_spi::hooks::HookError> {
        self.queries.lock().await.push(query);
        Ok(HookResolution::default())
    }
}

struct StaticResolutionHookProvider {
    queries: Arc<TokioMutex<Vec<HookEvaluationQuery>>>,
    resolution: HookResolution,
}

#[async_trait::async_trait]
impl ExecutionHookProvider for StaticResolutionHookProvider {
    async fn load_session_snapshot(
        &self,
        query: SessionHookSnapshotQuery,
    ) -> Result<SessionHookSnapshot, agentdash_spi::hooks::HookError> {
        Ok(SessionHookSnapshot {
            session_id: query.session_id,
            ..SessionHookSnapshot::default()
        })
    }

    async fn refresh_session_snapshot(
        &self,
        query: SessionHookRefreshQuery,
    ) -> Result<SessionHookSnapshot, agentdash_spi::hooks::HookError> {
        Ok(SessionHookSnapshot {
            session_id: query.session_id,
            ..SessionHookSnapshot::default()
        })
    }

    async fn evaluate_hook(
        &self,
        query: HookEvaluationQuery,
    ) -> Result<HookResolution, agentdash_spi::hooks::HookError> {
        self.queries.lock().await.push(query);
        Ok(self.resolution.clone())
    }
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
    let _rx = hub.ensure_session(&session.id).await;

    hub.reload_session_hook_runtime(&session.id, "turn-cap", "PI_AGENT", None, base.path())
        .await
        .expect("hook runtime should load");
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
                    identity: None,
                },
                CapabilityState::default(),
                uuid::Uuid::new_v4(),
                bundle_session_uuid,
            )));
        })
        .await;

    let hook_session = hub
        .get_hook_session_runtime(&session.id)
        .await
        .expect("hook runtime should remain available");
    let mut snapshot = hook_session.snapshot();
    snapshot.injections = vec![injection.clone()];
    hook_session.replace_snapshot(snapshot);

    let result = hub
        .collect_runtime_context_update_injections(&session.id)
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

    let trace = hook_session.trace();
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
    assert_eq!(payload.user_blocks.len(), 1);
    assert!(matches!(payload.prompt_payload, PromptPayload::Blocks(_)));

    let serialized =
        serde_json::to_value(&payload.user_blocks[0]).expect("serialize content block");
    assert_eq!(
        serialized.get("type").and_then(|v| v.as_str()),
        Some("text")
    );
}

#[test]
fn resolve_prompt_payload_supports_multiple_block_types() {
    let input = UserPromptInput {
        prompt_blocks: Some(vec![
            json!({ "type": "text", "text": "请分析 @src/main.ts" }),
            json!({ "type": "resource_link", "uri": "file:///workspace/src/main.ts", "name": "src/main.ts" }),
            json!({ "type": "image", "mimeType": "image/png", "data": "AAAA" }),
        ]),
        working_dir: None,
        env: std::collections::HashMap::new(),
        executor_config: None,
    };

    let payload = input
        .resolve_prompt_payload()
        .expect("resolve should succeed");
    assert_eq!(payload.user_blocks.len(), 3);
    assert!(matches!(payload.prompt_payload, PromptPayload::Blocks(_)));
    assert!(payload.text_prompt.contains("请分析 @src/main.ts"));
    assert!(
        payload
            .text_prompt
            .contains("[引用文件: src/main.ts (file:///workspace/src/main.ts)]")
    );
    assert!(
        payload
            .text_prompt
            .contains("[引用图片: mimeType=image/png")
    );
}

#[test]
fn build_user_notifications_preserves_block_types_and_index() {
    let blocks = vec![
        serde_json::from_value::<ContentBlock>(json!({
            "type": "text",
            "text": "hello"
        }))
        .expect("text block"),
        serde_json::from_value::<ContentBlock>(json!({
            "type": "resource_link",
            "uri": "file:///workspace/src/main.ts",
            "name": "src/main.ts"
        }))
        .expect("resource_link block"),
    ];

    let source = SourceInfo {
        connector_id: "unit-test".to_string(),
        connector_type: "local_executor".to_string(),
        executor_id: Some("CLAUDE_CODE".to_string()),
    };

    let envelopes = build_user_message_envelopes("sess-test", &source, "t100", &blocks);
    assert_eq!(envelopes.len(), 2);

    assert_eq!(envelopes[0].trace.turn_id.as_deref(), Some("t100"));
    assert_eq!(envelopes[0].trace.entry_index, Some(0));
    assert_eq!(envelopes[1].trace.entry_index, Some(1));
}

#[tokio::test]
async fn respond_companion_request_resolves_waiting_tool_and_persists_response_event() {
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
            _executor: &str,
            _working_dir: Option<PathBuf>,
        ) -> Result<futures::stream::BoxStream<'static, json_patch::Patch>, ConnectorError>
        {
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

    let base = tempfile::tempdir().expect("tempdir");
    let hub = test_hub(base.path().to_path_buf(), Arc::new(NoopConnector), None);
    let session = hub.create_session("test").await.expect("create session");
    let payload = json!({
        "type": "decision",
        "status": "approved",
        "choice": "YES",
        "summary": "YES"
    });

    let rx = hub
        .companion_wait_registry
        .register(&session.id, "req-1", "turn-1", Some("approval".to_string()))
        .await;

    hub.respond_companion_request(&session.id, "req-1", payload.clone())
        .await
        .expect("respond should succeed");

    assert_eq!(rx.await.expect("wait registry should resolve"), payload);

    let events = hub
        .persistence
        .list_all_events(&session.id)
        .await
        .expect("events should load");
    let response = events
        .iter()
        .find(|event| {
            event.session_update_type == "platform_event"
                && event
                    .notification
                    .event
                    .as_ref()
                    .is_platform_session_meta_update("companion_human_response")
        })
        .expect("response event should exist");

    assert_eq!(response.turn_id.as_deref(), Some("turn-1"));
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
async fn emit_capability_state_changed_persists_structured_event() {
    let base = tempfile::tempdir().expect("tempdir");
    let hub = test_hub(base.path().to_path_buf(), Arc::new(EmptyConnector), None);
    let session = hub
        .create_session("capability-event")
        .await
        .expect("create session");

    let payload = json!({
        "phase_node": "review",
        "state_changed": true,
        "tool_capabilities": {
            "current": ["file_read"]
        },
        "vfs": {
            "mounts": ["phase"]
        },
        "steering_delivery": { "status": "failed" }
    });
    hub.emit_capability_state_changed(&session.id, Some("turn-42"), payload.clone())
        .await
        .expect("emit capability state event");

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
                    .is_platform_session_meta_update("capability_state_changed")
        })
        .expect("capability state event should exist");

    assert_eq!(event.turn_id.as_deref(), Some("turn-42"));
    match &event.notification.event {
        BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate { value, .. }) => {
            assert_eq!(value, &payload);
        }
        other => panic!("unexpected event: {other:?}"),
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
        kind: "capability_state_update".to_string(),
        source: RuntimeEventSource::RuntimeContextUpdate,
        phase_node: Some("apply".to_string()),
        apply_mode: Some("live".to_string()),
        delivery_status: "queued_for_transform_context".to_string(),
        delivery_channel: "turn_start".to_string(),
        message_role: "user".to_string(),
        rendered_text: "## Capability State Update — Step Transition: apply".to_string(),
        sections: vec![ContextFrameSection::ToolSchema { tools: vec![] }],
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
    let base = tempfile::tempdir().expect("tempdir");
    let connector = Arc::new(SessionStartAwareConnector::default());
    let queries = Arc::new(TokioMutex::new(Vec::new()));
    let hook_provider = Arc::new(RecordingHookProvider {
        queries: queries.clone(),
    });
    let hub = test_hub(
        base.path().to_path_buf(),
        connector.clone(),
        Some(hook_provider),
    );
    let session = hub.create_session("test").await.expect("create session");
    hub.mark_owner_bootstrap_pending(&session.id)
        .await
        .expect("should mark pending");

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
async fn owner_bootstrap_marks_session_meta_bootstrapped() {
    let base = tempfile::tempdir().expect("tempdir");
    let connector = Arc::new(SessionStartAwareConnector::default());
    let queries = Arc::new(TokioMutex::new(Vec::new()));
    let hook_provider = Arc::new(RecordingHookProvider {
        queries: queries.clone(),
    });
    let hub = test_hub(base.path().to_path_buf(), connector, Some(hook_provider));
    let session = hub.create_session("test").await.expect("create session");
    hub.mark_owner_bootstrap_pending(&session.id)
        .await
        .expect("should mark pending");

    hub.start_prompt(&session.id, owner_bootstrap_request("hello", "ctx"))
        .await
        .expect("prompt should start");

    let meta = hub
        .get_session_meta(&session.id)
        .await
        .expect("meta should load")
        .expect("session should exist");
    assert_eq!(meta.bootstrap_state, SessionBootstrapState::Bootstrapped);
}

#[tokio::test]
async fn continuation_context_frame_strips_owner_resource_blocks() {
    let persistence = Arc::new(MemorySessionPersistence::default());
    let base = tempfile::tempdir().expect("tempdir");
    let hub = SessionHub::new_with_hooks_and_persistence(
        Some(local_workspace_vfs(&base.path().to_path_buf())),
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
    let user_blocks = vec![
        serde_json::from_value::<ContentBlock>(serde_json::json!({
            "type": "resource",
            "resource": {
                "uri": "agentdash://project-context/project-1",
                "mimeType": "text/markdown",
                "text": "## Project\nhidden"
            }
        }))
        .expect("resource block"),
        ContentBlock::Text(TextContent::new("继续分析 session 生命周期")),
    ];
    for envelope in build_user_message_envelopes(&session.id, &source, "t-1", &user_blocks) {
        hub.inject_notification(&session.id, envelope)
            .await
            .expect("inject user notification");
    }

    hub.inject_notification(
        &session.id,
        BackboneEnvelope::new(
            BackboneEvent::AgentMessageDelta(codex::AgentMessageDeltaNotification {
                delta: "已记录历史".to_string(),
                thread_id: session.id.clone(),
                turn_id: "t-1".to_string(),
                item_id: "assistant-msg-1".to_string(),
            }),
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
    let base = tempfile::tempdir().expect("tempdir");
    let hub = SessionHub::new_with_hooks_and_persistence(
        Some(local_workspace_vfs(&base.path().to_path_buf())),
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
    let user_blocks = vec![
        serde_json::from_value::<ContentBlock>(serde_json::json!({
            "type": "resource",
            "resource": {
                "uri": "agentdash://project-context/project-1",
                "mimeType": "text/markdown",
                "text": "## Project\nhidden"
            }
        }))
        .expect("resource block"),
        ContentBlock::Text(TextContent::new("继续分析 session 生命周期")),
    ];
    for envelope in build_user_message_envelopes(&session.id, &source, "t-1", &user_blocks) {
        hub.inject_notification(&session.id, envelope)
            .await
            .expect("inject user notification");
    }

    hub.inject_notification(
        &session.id,
        BackboneEnvelope::new(
            BackboneEvent::AgentMessageDelta(codex::AgentMessageDeltaNotification {
                delta: "已记录历史".to_string(),
                thread_id: session.id.clone(),
                turn_id: "t-1".to_string(),
                item_id: "assistant-msg-1".to_string(),
            }),
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
            BackboneEvent::ItemStarted(codex::ItemStartedNotification {
                item: item_started,
                thread_id: session.id.clone(),
                turn_id: "t-1".to_string(),
            }),
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
            BackboneEvent::ItemCompleted(codex::ItemCompletedNotification {
                item: item_completed,
                thread_id: session.id.clone(),
                turn_id: "t-1".to_string(),
            }),
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
    BackboneEnvelope::new(
        BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
            key: "user_message_chunk".to_string(),
            value: serde_json::to_value(ContentBlock::Text(TextContent::new(text)))
                .unwrap_or_default(),
        }),
        session_id,
        source,
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

#[tokio::test]
async fn build_projected_transcript_applies_latest_compaction_checkpoint() {
    let persistence = Arc::new(MemorySessionPersistence::default());
    let base = tempfile::tempdir().expect("tempdir");
    let hub = SessionHub::new_with_hooks_and_persistence(
        Some(local_workspace_vfs(&base.path().to_path_buf())),
        Arc::new(SessionStartAwareConnector::default()),
        None,
        persistence,
    );
    let session = hub.create_session("test").await.expect("create session");

    for (turn_id, entry_index, text) in [
        ("t-1", 0_u32, "历史用户消息 1"),
        ("t-2", 0_u32, "历史用户消息 2"),
        ("t-3", 0_u32, "最近用户消息"),
    ] {
        hub.inject_notification(
            &session.id,
            inject_user_message_envelope(&session.id, turn_id, entry_index, text),
        )
        .await
        .expect("inject user notification");
    }

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
                "timestamp_ms": 1710000000000_u64,
            }),
        ),
    )
    .await
    .expect("inject compaction checkpoint");

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

    let transcript = hub
        .build_projected_transcript(&session.id)
        .await
        .expect("transcript should build");
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
                compacted_until_ref
                    .as_ref()
                    .map(|message_ref| (message_ref.turn_id.as_str(), message_ref.entry_index)),
                Some(("t-2", 0))
            );
        }
        other => panic!("unexpected first message: {other:?}"),
    }
    assert_eq!(restored[1].first_text(), Some("最近用户消息"));
}

#[tokio::test]
async fn continuation_context_frame_uses_compacted_projection() {
    let persistence = Arc::new(MemorySessionPersistence::default());
    let base = tempfile::tempdir().expect("tempdir");
    let hub = SessionHub::new_with_hooks_and_persistence(
        Some(local_workspace_vfs(&base.path().to_path_buf())),
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

    hub.inject_notification(
        &session.id,
        inject_compaction_envelope(
            &session.id,
            "t-3",
            serde_json::json!({
                "summary": "压缩后的历史摘要",
                "tokens_before": 38000,
                "messages_compacted": 2,
                "newly_compacted_messages": 2,
                "timestamp_ms": 1710000000000_u64,
            }),
        ),
    )
    .await
    .expect("inject compaction checkpoint");

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
async fn start_prompt_records_failed_terminal_when_connector_setup_fails() {
    struct FailingConnector;

    #[async_trait::async_trait]
    impl AgentConnector for FailingConnector {
        fn connector_id(&self) -> &'static str {
            "failing"
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
                "connector setup failed".to_string(),
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
    let hub = test_hub(base.path().to_path_buf(), Arc::new(FailingConnector), None);
    let session = hub.create_session("test").await.expect("create session");

    let error = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        hub.start_prompt(&session.id, simple_prompt_request("hello")),
    )
    .await
    .expect("prompt should not hang")
    .expect_err("prompt should fail");
    assert!(error.to_string().contains("connector setup failed"));

    let history = hub
        .persistence
        .list_all_events(&session.id)
        .await
        .expect("history should load");
    let terminal = history
        .iter()
        .filter_map(|event| parse_turn_terminal_event_from_envelope(&event.notification))
        .last()
        .expect("terminal event should exist");
    assert_eq!(
        terminal.1,
        super::super::hub_support::TurnTerminalKind::Failed
    );
    assert_eq!(
        terminal.2.as_deref(),
        Some("执行器运行错误: connector setup failed")
    );
}

#[tokio::test]
async fn connector_setup_failure_does_not_commit_bootstrap_or_pending_commands() {
    struct FailingConnector;

    #[async_trait::async_trait]
    impl AgentConnector for FailingConnector {
        fn connector_id(&self) -> &'static str {
            "failing"
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
                "connector setup failed".to_string(),
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
    let hub = test_hub(base.path().to_path_buf(), Arc::new(FailingConnector), None);
    let session = hub.create_session("test").await.expect("create session");
    hub.mark_owner_bootstrap_pending(&session.id)
        .await
        .expect("should mark pending");
    hub.enqueue_pending_capability_state_transition(
        &session.id,
        PendingCapabilityStateTransition {
            id: "transition-fail".to_string(),
            run_id: uuid::Uuid::new_v4(),
            lifecycle_key: "dev".to_string(),
            phase_node: "review".to_string(),
            capability_keys: std::collections::BTreeSet::new(),
            state: agentdash_spi::CapabilityState::default(),
            created_at: 1,
            source_turn_id: None,
        },
    )
    .await
    .expect("enqueue pending transition");

    let error = hub
        .start_prompt(&session.id, owner_bootstrap_request("hello", "ctx"))
        .await
        .expect_err("prompt should fail");
    assert!(error.to_string().contains("connector setup failed"));

    let meta = hub
        .get_session_meta(&session.id)
        .await
        .expect("meta should load")
        .expect("session should exist");
    assert_eq!(meta.bootstrap_state, SessionBootstrapState::Pending);

    let pending_commands = hub
        .persistence
        .list_runtime_commands_by_status(&[RuntimeCommandStatus::Pending], 10)
        .await
        .expect("runtime commands should load");
    assert_eq!(pending_commands.len(), 1);
    assert_eq!(pending_commands[0].transition_id, "transition-fail");
}

#[tokio::test]
async fn cancel_marks_running_turn_interrupted() {
    #[derive(Default)]
    struct CancelAwareConnector {
        streams: Arc<
            TokioMutex<HashMap<String, mpsc::Sender<Result<BackboneEnvelope, ConnectorError>>>>,
        >,
    }

    #[async_trait::async_trait]
    impl AgentConnector for CancelAwareConnector {
        fn connector_id(&self) -> &'static str {
            "cancel-aware"
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
            session_id: &str,
            _: Option<&str>,
            _: &PromptPayload,
            _: agentdash_spi::ExecutionContext,
        ) -> Result<agentdash_spi::ExecutionStream, ConnectorError> {
            let (tx, rx) = mpsc::channel(4);
            self.streams.lock().await.insert(session_id.to_string(), tx);
            Ok(Box::pin(ReceiverStream::new(rx)))
        }
        async fn cancel(&self, session_id: &str) -> Result<(), ConnectorError> {
            self.streams.lock().await.remove(session_id);
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
    let connector = Arc::new(CancelAwareConnector::default());
    let hub = test_hub(base.path().to_path_buf(), connector, None);
    let session = hub.create_session("test").await.expect("create session");

    let turn_id = hub
        .start_prompt(&session.id, simple_prompt_request("hello"))
        .await
        .expect("prompt should start");
    hub.cancel(&session.id)
        .await
        .expect("cancel should succeed");
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let state = hub
        .inspect_session_execution_state(&session.id)
        .await
        .expect("state should load");
    assert_eq!(
        state,
        SessionExecutionState::Interrupted {
            turn_id: Some(turn_id.clone()),
            message: Some("执行已取消".to_string())
        }
    );

    let history = hub
        .persistence
        .list_all_events(&session.id)
        .await
        .expect("history should load");
    let terminal = history
        .iter()
        .filter_map(|event| parse_turn_terminal_event_from_envelope(&event.notification))
        .last()
        .expect("terminal event should exist");
    assert_eq!(terminal.0, turn_id);
    assert_eq!(
        terminal.1,
        super::super::hub_support::TurnTerminalKind::Interrupted
    );
    assert_eq!(terminal.2.as_deref(), Some("执行已取消"));
}

// ─────────────────────────────────────────────────────────────────────
// Fail-lock: auto-resume 必须经过 PromptRequestAugmenter
// ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn launch_prompt_strict_requires_prompt_augmenter() {
    let base = tempfile::tempdir().expect("tempdir");
    let hub = test_hub(base.path().to_path_buf(), Arc::new(EmptyConnector), None);
    let session = hub.create_session("strict-launch").await.expect("create");

    let error = hub
        .launch_command(
            &session.id,
            super::super::launch::LaunchCommand::http_prompt_input(
                UserPromptInput::from_text("hello"),
                None,
            ),
        )
        .await
        .expect_err("strict launch 应在 augmenter 缺失时失败");

    match error {
        ConnectorError::Runtime(message) => {
            assert!(
                message.contains("prompt_augmenter 未注入"),
                "错误信息应提示 augmenter 缺失，实际为: {message}"
            );
        }
        other => panic!("期望 Runtime 错误，实际为: {other}"),
    }
}

#[tokio::test]
async fn schedule_hook_auto_resume_strict_mode_requires_augmenter() {
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

    // 不注入 augmenter，strict auto-resume 应该在 launch 前失败，不能触发 connector.prompt。
    hub.schedule_hook_auto_resume(session.id.clone());
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    assert_eq!(
        prompt_calls.load(Ordering::SeqCst),
        0,
        "strict auto-resume 在 augmenter 缺失时不应触发 connector.prompt"
    );
}

#[tokio::test]
async fn schedule_hook_auto_resume_routes_through_augmenter() {
    use crate::session::augmenter::{PromptAugmentInput, PromptRequestAugmenter};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct SpyAugmenter {
        calls: Arc<AtomicUsize>,
        captured_prompt: Arc<TokioMutex<Option<String>>>,
        captured_mcp_len: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl PromptRequestAugmenter for SpyAugmenter {
        async fn augment(
            &self,
            _session_id: &str,
            input: PromptAugmentInput,
        ) -> Result<SessionLaunchPlan, ConnectorError> {
            let req = input.into_launch_plan();
            self.calls.fetch_add(1, Ordering::SeqCst);
            let text = req
                .user_input
                .prompt_blocks
                .as_ref()
                .and_then(|blocks| blocks.first())
                .and_then(|block| block.get("text"))
                .and_then(|v| v.as_str())
                .map(ToString::to_string);
            *self.captured_prompt.lock().await = text;
            self.captured_mcp_len
                .store(req.mcp_servers.len(), Ordering::SeqCst);
            Err(ConnectorError::InvalidConfig(
                "spy augmenter stops here".to_string(),
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
                "connector should not be reached if augmenter stopped".to_string(),
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
    hub.set_prompt_augmenter(Arc::new(SpyAugmenter {
        calls: calls.clone(),
        captured_prompt: captured_prompt.clone(),
        captured_mcp_len: captured_mcp_len.clone(),
    }))
    .await;

    hub.schedule_hook_auto_resume(session.id.clone());

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

    assert!(hub.request_hook_auto_resume(session.id.clone()).await);
    assert!(hub.request_hook_auto_resume(session.id.clone()).await);
    assert!(!hub.request_hook_auto_resume(session.id.clone()).await);
    assert!(!hub.request_hook_auto_resume(session.id.clone()).await);

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
        !hub.request_hook_auto_resume("nonexistent".to_string())
            .await,
    );
}
