use agentdash_spi::hooks::SharedHookRuntime;
use agentdash_spi::{SessionMcpServer, Vfs};
use async_trait::async_trait;
use std::io;

use super::capability_projection::{
    SessionCapabilityProjectionInput, derive_session_skill_baseline, merge_live_vfs_skill_entries,
};
use super::hub::SessionRuntimeInner;
use super::hub::{
    LiveRuntimeContextTransitionInput, PendingRuntimeContextApplication,
    PendingRuntimeContextTransitionInput, RuntimeContextTransitionOutcome,
};
use super::runtime_commands::{
    AgentFrameTransitionRecord, RuntimeCommandRecord, RuntimeDeliveryCommand,
};
use super::types::{AgentFrameRuntimeTarget, CapabilityState, PendingCapabilityStateTransition};
use crate::runtime_gateway::{
    McpCallToolInput, RuntimeMcpToolDescriptor, RuntimeSessionMcpAccess, RuntimeSessionMcpError,
};

#[derive(Clone)]
pub struct SessionCapabilityService {
    hub: SessionRuntimeInner,
}

impl SessionCapabilityService {
    pub(super) fn new(hub: SessionRuntimeInner) -> Self {
        Self { hub }
    }

    pub async fn get_runtime_mcp_servers(&self, session_id: &str) -> Vec<SessionMcpServer> {
        self.hub.get_runtime_mcp_servers(session_id).await
    }

    pub async fn get_current_capability_state(&self, session_id: &str) -> Option<CapabilityState> {
        self.hub.get_current_capability_state(session_id).await
    }

    pub async fn get_latest_capability_state(&self, session_id: &str) -> Option<CapabilityState> {
        self.hub.get_latest_capability_state(session_id).await
    }

    /// Delivery adapter: 把 RuntimeSession id 解析为对应的 AgentFrame id。
    ///
    /// 仅用于已知 delivery session 但尚未拥有 `AgentFrameRuntimeTarget` 的
    /// adapter 边界（如 canvas tool、workflow executor 入口）。业务控制路径应
    /// 直接传递 `AgentFrameRuntimeTarget`，不应依赖此方法做反查。
    pub(crate) async fn resolve_runtime_session_frame_id(
        &self,
        session_id: &str,
    ) -> Result<uuid::Uuid, String> {
        self.hub
            .resolve_runtime_session_frame_id(session_id)
            .await
            .map_err(|error| error.to_string())
    }

    /// Delivery adapter: 把 RuntimeSession id 解析为完整的 `AgentFrameRuntimeTarget`。
    ///
    /// 仅用于 adapter 边界将 session 维度入口转换为 frame-first 控制目标。
    /// 内部业务路径应直接持有并传递 `AgentFrameRuntimeTarget`。
    pub(crate) async fn resolve_runtime_session_target(
        &self,
        session_id: &str,
    ) -> Result<AgentFrameRuntimeTarget, String> {
        let frame_id = self.resolve_runtime_session_frame_id(session_id).await?;
        Ok(AgentFrameRuntimeTarget {
            frame_id,
            delivery_runtime_session_id: session_id.to_string(),
        })
    }

    /// Canvas mount 写入 AgentFrame（通过 AgentFrameRepository 追加）。
    pub(crate) async fn append_visible_canvas_mount_to_frame(
        &self,
        session_id: &str,
        mount_id: &str,
    ) -> Result<(), String> {
        let frame_id = self.resolve_runtime_session_frame_id(session_id).await?;
        let repo = self.hub.agent_frame_repo.as_ref().ok_or_else(|| {
            format!("session `{session_id}` 无 AgentFrame repository，无法写入 canvas mount")
        })?;
        repo.append_visible_canvas_mount(frame_id, mount_id)
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn list_requested_runtime_commands(
        &self,
        session_id: &str,
    ) -> io::Result<Vec<RuntimeCommandRecord>> {
        self.hub
            .stores
            .runtime_commands
            .list_requested_runtime_commands(session_id)
            .await
            .map_err(Into::into)
    }

    pub async fn enqueue_runtime_delivery_command(
        &self,
        delivery_runtime_session_id: &str,
        delivery: RuntimeDeliveryCommand,
        frame_transition: AgentFrameTransitionRecord,
    ) -> std::io::Result<()> {
        self.hub
            .enqueue_runtime_delivery_command(
                delivery_runtime_session_id,
                delivery,
                frame_transition,
            )
            .await
    }

    pub(crate) async fn enqueue_pending_runtime_context_transition(
        &self,
        mut input: PendingRuntimeContextTransitionInput,
    ) -> Result<(), String> {
        self.derive_skill_baseline_for_transition_state(
            input.before_state.as_ref(),
            &mut input.after_state,
        )
        .await;
        self.hub
            .enqueue_pending_runtime_context_transition(input)
            .await
    }

    pub(crate) async fn apply_live_runtime_context_transition(
        &self,
        hook_runtime: &agentdash_spi::hooks::SharedHookRuntime,
        mut input: LiveRuntimeContextTransitionInput,
    ) -> Result<RuntimeContextTransitionOutcome, String> {
        self.derive_skill_baseline_for_transition_state(
            input.before_state.as_ref(),
            &mut input.after_state,
        )
        .await;
        self.hub
            .apply_live_runtime_context_transition(hook_runtime, input)
            .await
    }

    pub(crate) async fn apply_live_vfs_capability_state(
        &self,
        hook_runtime: &SharedHookRuntime,
        target: AgentFrameRuntimeTarget,
        before_state: CapabilityState,
        active_vfs: Vfs,
        phase_node: &str,
        apply_mode: &'static str,
    ) -> Result<RuntimeContextTransitionOutcome, String> {
        let session_id = target.delivery_runtime_session_id.clone();
        if hook_runtime.session_id() != session_id {
            return Err(format!(
                "Hook runtime session `{}` 与 delivery RuntimeSession `{session_id}` 不一致，拒绝热更新能力状态",
                hook_runtime.session_id()
            ));
        }
        let mut after_state = before_state.clone();
        after_state.vfs.active = Some(active_vfs);
        let capability_keys = after_state.capability_keys();
        self.apply_live_runtime_context_transition(
            hook_runtime,
            LiveRuntimeContextTransitionInput {
                target_frame_id: target.frame_id,
                delivery_runtime_session_id: session_id,
                turn_id: None,
                phase_node: phase_node.to_string(),
                run_id: None,
                lifecycle_key: None,
                before_state: Some(before_state),
                after_state,
                capability_keys,
                key_delta: crate::session::SetDelta::default(),
                apply_mode,
            },
        )
        .await
    }

    async fn derive_skill_baseline_for_transition_state(
        &self,
        before_state: Option<&CapabilityState>,
        after_state: &mut CapabilityState,
    ) {
        let Some(active_vfs) = after_state.vfs.active.as_ref() else {
            return;
        };
        let Some(skills) = self.derive_skill_entries_for_active_vfs(active_vfs).await else {
            return;
        };
        let existing = before_state
            .map(|state| state.skill.skills.as_slice())
            .unwrap_or_else(|| after_state.skill.skills.as_slice());
        after_state.skill.skills = merge_live_vfs_skill_entries(existing, skills);
    }

    async fn derive_skill_entries_for_active_vfs(
        &self,
        active_vfs: &Vfs,
    ) -> Option<Vec<agentdash_spi::context::capability::SkillEntry>> {
        derive_session_skill_baseline(SessionCapabilityProjectionInput {
            vfs_service: self.hub.vfs_service.as_deref(),
            active_vfs: Some(active_vfs),
            extra_skill_dirs: &self.hub.extra_skill_dirs,
            diagnostics_label: "runtime_context_transition",
        })
        .await
        .map(|caps| caps.skills)
    }

    pub(crate) async fn apply_pending_runtime_context_transitions_on_turn(
        &self,
        session_id: &str,
        turn_id: &str,
        hook_runtime: Option<&agentdash_spi::hooks::SharedHookRuntime>,
        before_state: CapabilityState,
        final_capability_state: &CapabilityState,
        transitions: &[PendingCapabilityStateTransition],
        tools: &[agentdash_agent_types::DynAgentTool],
    ) -> PendingRuntimeContextApplication {
        self.hub
            .apply_pending_runtime_context_transitions_on_turn(
                session_id,
                turn_id,
                hook_runtime,
                before_state,
                final_capability_state,
                transitions,
                tools,
            )
            .await
    }
}

#[async_trait]
impl RuntimeSessionMcpAccess for SessionCapabilityService {
    async fn list_mcp_tools(
        &self,
        session_id: &str,
    ) -> Result<Vec<RuntimeMcpToolDescriptor>, RuntimeSessionMcpError> {
        let entries = self
            .hub
            .discover_runtime_mcp_tool_entries(session_id)
            .await
            .map_err(runtime_mcp_error_from_connector)?;
        Ok(entries
            .into_iter()
            .map(|entry| RuntimeMcpToolDescriptor {
                runtime_name: entry.runtime_name,
                server_name: entry.server_name,
                tool_name: entry.tool_name,
                uses_relay: entry.uses_relay,
                description: entry.description,
                parameters_schema: entry.parameters_schema,
            })
            .collect())
    }

    async fn call_mcp_tool(
        &self,
        session_id: &str,
        input: McpCallToolInput,
    ) -> Result<agentdash_agent_types::AgentToolResult, RuntimeSessionMcpError> {
        let entries = self
            .hub
            .discover_runtime_mcp_tool_entries(session_id)
            .await
            .map_err(runtime_mcp_error_from_connector)?;
        let entry = entries
            .into_iter()
            .find(|entry| runtime_mcp_entry_matches(entry, &input))
            .ok_or_else(|| {
                RuntimeSessionMcpError::ToolUnavailable(
                    "目标 MCP 工具不在当前 Session Runtime Surface 中".to_string(),
                )
            })?;
        let arguments = input.arguments.unwrap_or(serde_json::Value::Null);
        crate::runtime_gateway::execute_runtime_mcp_tool(entry.tool, &entry.runtime_name, arguments)
            .await
    }
}

fn runtime_mcp_entry_matches(
    entry: &agentdash_executor::mcp::DiscoveredMcpTool,
    input: &McpCallToolInput,
) -> bool {
    if let Some(runtime_name) = input.runtime_name.as_deref()
        && runtime_name == entry.runtime_name
    {
        return true;
    }
    matches!(
        (input.server_name.as_deref(), input.tool_name.as_deref()),
        (Some(server_name), Some(tool_name))
            if server_name == entry.server_name && tool_name == entry.tool_name
    )
}

fn runtime_mcp_error_from_connector(
    error: agentdash_spi::ConnectorError,
) -> RuntimeSessionMcpError {
    match error {
        agentdash_spi::ConnectorError::Runtime(message)
        | agentdash_spi::ConnectorError::InvalidConfig(message) => {
            RuntimeSessionMcpError::SessionUnavailable(message)
        }
        agentdash_spi::ConnectorError::ConnectionFailed(message) => {
            RuntimeSessionMcpError::DiscoveryFailed(message)
        }
        agentdash_spi::ConnectorError::SpawnFailed(message) => {
            RuntimeSessionMcpError::DiscoveryFailed(message)
        }
        agentdash_spi::ConnectorError::Io(error) => {
            RuntimeSessionMcpError::DiscoveryFailed(error.to_string())
        }
        agentdash_spi::ConnectorError::Json(error) => {
            RuntimeSessionMcpError::DiscoveryFailed(error.to_string())
        }
    }
}
