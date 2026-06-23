use agentdash_agent_types::DynAgentTool;
use agentdash_domain::canvas::Canvas;
use agentdash_spi::{RuntimeBackendAnchor, RuntimeMcpServer, Vfs};
use async_trait::async_trait;
use std::io;

use super::capability_projection::{
    SessionCapabilityProjectionInput, derive_session_skill_baseline, merge_live_vfs_skill_entries,
};
use super::capability_state::project_capability_state_from_frame;
#[cfg(test)]
use super::hub::PendingRuntimeContextTransitionInput;
use super::hub::SessionRuntimeInner;
use super::hub::{ApplyPendingRuntimeContextTransitionInput, PendingRuntimeContextApplication};
use super::runtime_commands::{
    AgentFrameTransitionRecord, RuntimeCommandRecord, RuntimeDeliveryCommand,
};
use super::types::{AgentFrameRuntimeTarget, CapabilityState};
use crate::agent_run::frame::surface::AgentFrameSurfaceExt;
use crate::agent_run::{
    AgentFrameBuilder, AgentRunEffectiveCapabilityService, AgentRunEffectiveCapabilityView,
    AgentRunFrameSurfaceError, AgentRunSurfaceProjectionContext,
    AgentRunSurfaceProjectionContextResolver, AgentRunSurfaceProjectionContextSource,
};
use crate::canvas::resolve_canvas_binding_files;
use crate::lifecycle::resolve_current_frame_from_delivery_trace_ref;
use crate::runtime_gateway::{
    McpCallToolInput, RuntimeMcpToolDescriptor, RuntimeSessionMcpAccess, RuntimeSessionMcpError,
};
use crate::vfs::{append_canvas_mounts, refresh_canvas_mount_binding_files};

#[derive(Clone)]
pub struct SessionCapabilityService {
    hub: SessionRuntimeInner,
}

impl SessionCapabilityService {
    pub(super) fn new(hub: SessionRuntimeInner) -> Self {
        Self { hub }
    }

    pub async fn get_runtime_mcp_servers(&self, session_id: &str) -> Vec<RuntimeMcpServer> {
        self.hub.get_runtime_mcp_servers(session_id).await
    }

    pub async fn get_current_capability_state(&self, session_id: &str) -> Option<CapabilityState> {
        self.hub.get_current_capability_state(session_id).await
    }

    pub async fn get_latest_capability_state(&self, session_id: &str) -> Option<CapabilityState> {
        self.hub.get_latest_capability_state(session_id).await
    }

    pub async fn get_current_runtime_backend_anchor(
        &self,
        session_id: &str,
    ) -> Result<RuntimeBackendAnchor, String> {
        self.hub
            .get_current_runtime_backend_anchor(session_id)
            .await
            .map_err(|error| error.to_string())
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

    /// 将已持久化的 AgentFrame revision 采用到 active runtime。
    pub(crate) async fn adopt_persisted_agent_frame_revision(
        &self,
        target: AgentFrameRuntimeTarget,
    ) -> Result<Vec<DynAgentTool>, String> {
        self.hub
            .adopt_persisted_agent_frame_revision(target)
            .await
            .map_err(|error| error.to_string())
    }

    pub(crate) async fn expose_canvas_mount_revision_and_adopt_with_context(
        &self,
        context: &AgentRunSurfaceProjectionContext,
        canvas: &Canvas,
    ) -> Result<Vfs, String> {
        let session_id = context.delivery_runtime_session_id.as_str();
        let frame_repo = self.hub.agent_frame_repo.as_ref().ok_or_else(|| {
            format!("session `{session_id}` 无 AgentFrame repository，无法写入 Canvas exposure")
        })?;
        let current_frame = &context.current_frame;

        let before_state = context.capability_state.clone();
        let mut after_state = before_state.clone();
        let mut active_vfs = context
            .require_active_vfs()
            .map_err(|error| error.to_string())?
            .clone();
        append_canvas_mounts(&mut active_vfs, std::slice::from_ref(canvas));
        if let Some(vfs_service) = self.hub.vfs_service.as_deref() {
            let binding_files =
                resolve_canvas_binding_files(canvas, &active_vfs, vfs_service).await;
            refresh_canvas_mount_binding_files(&mut active_vfs, canvas, &binding_files);
        }
        after_state.vfs.active = Some(active_vfs.clone());
        self.derive_skill_baseline_for_projection_context(
            context,
            Some(&before_state),
            &mut after_state,
        )
        .await;

        let mut next_frame = AgentFrameBuilder::new(current_frame.agent_id)
            .with_capability_state(&after_state)
            .with_created_by("canvas_expose", Some(current_frame.id.to_string()))
            .with_runtime_session(session_id.to_string())
            .build_uncommitted(frame_repo.as_ref())
            .await
            .map_err(|error| error.to_string())?;
        next_frame.append_visible_canvas_mount(&canvas.mount_id);
        next_frame.append_visible_workspace_module_ref(&format!("canvas:{}", canvas.mount_id));
        frame_repo
            .create(&next_frame)
            .await
            .map_err(|error| error.to_string())?;

        self.adopt_persisted_agent_frame_revision(AgentFrameRuntimeTarget {
            frame_id: next_frame.id,
            delivery_runtime_session_id: session_id.to_string(),
        })
        .await?;

        next_frame
            .typed_vfs()
            .ok_or_else(|| format!("AgentFrame `{}` 写入后缺少 VFS surface", next_frame.id))
    }

    pub(crate) async fn resolve_agent_run_surface_projection_context(
        &self,
        source: AgentRunSurfaceProjectionContextSource,
    ) -> Result<AgentRunSurfaceProjectionContext, String> {
        match source {
            AgentRunSurfaceProjectionContextSource::DeliveryRuntimeSession {
                runtime_session_id,
            } => {
                self.resolve_surface_projection_context_for_session(&runtime_session_id)
                    .await
            }
            AgentRunSurfaceProjectionContextSource::RuntimeTarget { target } => {
                let context = self
                    .resolve_surface_projection_context_for_session(
                        &target.delivery_runtime_session_id,
                    )
                    .await?;
                if context.target.frame_id != target.frame_id {
                    return Err(format!(
                        "AgentFrame `{}` 不是 delivery RuntimeSession `{}` 当前 projection target（当前为 `{}`）",
                        target.frame_id,
                        target.delivery_runtime_session_id,
                        context.target.frame_id
                    ));
                }
                Ok(context)
            }
            AgentRunSurfaceProjectionContextSource::EffectFrame {
                effect_frame_id,
                delivery_runtime_session_id,
            } => {
                let context = self
                    .resolve_surface_projection_context_for_session(&delivery_runtime_session_id)
                    .await?;
                let frame_repo = self.hub.agent_frame_repo.as_ref().ok_or_else(|| {
                    format!(
                        "session `{delivery_runtime_session_id}` 无 AgentFrame repository，无法校验 effect frame `{effect_frame_id}`"
                    )
                })?;
                let effect_frame = frame_repo
                    .get(effect_frame_id)
                    .await
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| format!("effect AgentFrame `{effect_frame_id}` 不存在"))?;
                if effect_frame.agent_id != context.current_frame.agent_id {
                    return Err(format!(
                        "effect AgentFrame `{effect_frame_id}` 属于 Agent `{}`，不匹配 delivery RuntimeSession `{}` 当前 Agent `{}`",
                        effect_frame.agent_id,
                        delivery_runtime_session_id,
                        context.current_frame.agent_id
                    ));
                }
                Ok(context)
            }
        }
    }

    async fn resolve_surface_projection_context_for_session(
        &self,
        session_id: &str,
    ) -> Result<AgentRunSurfaceProjectionContext, String> {
        let frame_repo = self.hub.agent_frame_repo.as_ref().ok_or_else(|| {
            format!("session `{session_id}` 无 AgentFrame repository，无法解析 AgentRun projection context")
        })?;
        let anchor_repo = self.hub.execution_anchor_repo.as_ref().ok_or_else(|| {
            format!("session `{session_id}` 无 RuntimeSessionExecutionAnchor repository，无法解析 AgentRun projection context")
        })?;
        let agent_repo = self.hub.lifecycle_agent_repo.as_ref().ok_or_else(|| {
            format!("session `{session_id}` 无 LifecycleAgent repository，无法解析 AgentRun projection context")
        })?;
        let (_anchor, _agent, current_frame) = resolve_current_frame_from_delivery_trace_ref(
            session_id,
            anchor_repo.as_ref(),
            agent_repo.as_ref(),
            frame_repo.as_ref(),
        )
        .await
        .map_err(|error| format!("通过 anchor 查找 session `{session_id}` 当前 AgentFrame surface 失败: {error}"))?
        .ok_or_else(|| {
            format!("session `{session_id}` 缺少可用 RuntimeSessionExecutionAnchor/AgentFrame，无法解析 AgentRun projection context")
        })?;

        let active = self
            .hub
            .runtime_registry
            .with_runtime(session_id, |runtime| {
                runtime
                    .and_then(|runtime| runtime.turn_state.active_turn())
                    .map(|turn| {
                        (
                            turn.turn_id.clone(),
                            turn.session_frame.identity.clone(),
                            turn.session_frame.vfs.clone(),
                            turn.session_frame.mcp_servers.clone(),
                            turn.session_frame.runtime_backend_anchor.clone(),
                        )
                    })
            })
            .await;

        let frame_state = project_capability_state_from_frame(&current_frame);
        let (
            active_turn_id,
            identity,
            active_turn_vfs,
            active_turn_mcp_servers,
            runtime_backend_anchor,
        ) = match active {
            Some((turn_id, identity, vfs, mcp_servers, runtime_backend_anchor)) => (
                Some(turn_id),
                identity,
                vfs,
                mcp_servers,
                runtime_backend_anchor,
            ),
            None => (None, None, None, current_frame.typed_mcp_servers(), None),
        };
        let active_vfs = active_turn_vfs
            .or_else(|| frame_state.vfs.active.clone())
            .or_else(|| current_frame.typed_vfs());
        let mcp_servers = if active_turn_mcp_servers.is_empty() {
            current_frame.typed_mcp_servers()
        } else {
            active_turn_mcp_servers
        };

        Ok(AgentRunSurfaceProjectionContext {
            target: AgentFrameRuntimeTarget {
                frame_id: current_frame.id,
                delivery_runtime_session_id: session_id.to_string(),
            },
            delivery_runtime_session_id: session_id.to_string(),
            active_turn_id,
            current_frame,
            identity,
            active_vfs,
            mcp_servers,
            runtime_backend_anchor,
            capability_state: frame_state,
            skill_discovery_provider_count: self.hub.skill_discovery_providers.len(),
            extra_skill_dirs: self.hub.extra_skill_dirs.clone(),
        })
    }

    pub(crate) async fn effective_capability_view_for_runtime_session(
        &self,
        session_id: &str,
    ) -> Result<AgentRunEffectiveCapabilityView, String> {
        let target = self.resolve_runtime_session_target(session_id).await?;
        let repo = self.hub.agent_frame_repo.as_ref().ok_or_else(|| {
            format!(
                "session `{session_id}` 无 AgentFrame repository，无法读取 AgentRun capability view"
            )
        })?;
        let frame = repo
            .get(target.frame_id)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("AgentFrame `{}` 不存在", target.frame_id))?;
        Ok(AgentRunEffectiveCapabilityService::effective_view_from_frame(target, &frame))
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

    #[cfg(test)]
    pub(crate) async fn enqueue_pending_runtime_context_transition(
        &self,
        mut input: PendingRuntimeContextTransitionInput,
    ) -> Result<(), String> {
        self.derive_skill_baseline_for_transition_state(
            &input.delivery_runtime_session_id,
            input.before_state.as_ref(),
            &mut input.after_state,
        )
        .await;
        self.hub
            .enqueue_pending_runtime_context_transition(input)
            .await
    }

    #[cfg(test)]
    async fn derive_skill_baseline_for_transition_state(
        &self,
        session_id: &str,
        before_state: Option<&CapabilityState>,
        after_state: &mut CapabilityState,
    ) {
        let Some(active_vfs) = after_state.vfs.active.as_ref() else {
            return;
        };
        let Some(skills) = self
            .derive_skill_entries_for_active_vfs(session_id, active_vfs)
            .await
        else {
            return;
        };
        let existing = before_state
            .map(|state| state.skill.skills.as_slice())
            .unwrap_or_else(|| after_state.skill.skills.as_slice());
        after_state.skill.skills = merge_live_vfs_skill_entries(existing, skills);
    }

    async fn derive_skill_baseline_for_projection_context(
        &self,
        context: &AgentRunSurfaceProjectionContext,
        before_state: Option<&CapabilityState>,
        after_state: &mut CapabilityState,
    ) {
        let Some(active_vfs) = after_state.vfs.active.as_ref() else {
            return;
        };
        let Some(skills) = self
            .derive_skill_entries_for_projection_context(context, active_vfs)
            .await
        else {
            return;
        };
        let existing = before_state
            .map(|state| state.skill.skills.as_slice())
            .unwrap_or_else(|| after_state.skill.skills.as_slice());
        after_state.skill.skills = merge_live_vfs_skill_entries(existing, skills);
    }

    async fn derive_skill_entries_for_projection_context(
        &self,
        context: &AgentRunSurfaceProjectionContext,
        active_vfs: &Vfs,
    ) -> Option<Vec<agentdash_spi::context::capability::SkillEntry>> {
        derive_session_skill_baseline(SessionCapabilityProjectionInput {
            vfs_service: self.hub.vfs_service.as_deref(),
            active_vfs: Some(active_vfs),
            identity: context.identity.as_ref(),
            extra_skill_dirs: &self.hub.extra_skill_dirs,
            skill_discovery_providers: &self.hub.skill_discovery_providers,
            diagnostics_label: "agentrun_runtime_surface_projection",
        })
        .await
        .map(|caps| caps.skills)
    }

    #[cfg(test)]
    async fn derive_skill_entries_for_active_vfs(
        &self,
        session_id: &str,
        active_vfs: &Vfs,
    ) -> Option<Vec<agentdash_spi::context::capability::SkillEntry>> {
        let identity = self.runtime_skill_projection_identity(session_id).await;
        derive_session_skill_baseline(SessionCapabilityProjectionInput {
            vfs_service: self.hub.vfs_service.as_deref(),
            active_vfs: Some(active_vfs),
            identity: identity.as_ref(),
            extra_skill_dirs: &self.hub.extra_skill_dirs,
            skill_discovery_providers: &self.hub.skill_discovery_providers,
            diagnostics_label: "runtime_context_transition",
        })
        .await
        .map(|caps| caps.skills)
    }

    #[cfg(test)]
    async fn runtime_skill_projection_identity(
        &self,
        session_id: &str,
    ) -> Option<agentdash_spi::AuthIdentity> {
        self.hub
            .runtime_registry
            .with_runtime(session_id, |runtime| {
                runtime
                    .and_then(|runtime| runtime.turn_state.active_turn())
                    .and_then(|turn| turn.session_frame.identity.clone())
            })
            .await
    }

    pub(crate) async fn apply_pending_runtime_context_transitions_on_turn(
        &self,
        input: ApplyPendingRuntimeContextTransitionInput<'_>,
    ) -> PendingRuntimeContextApplication {
        self.hub
            .apply_pending_runtime_context_transitions_on_turn(input)
            .await
    }
}

#[async_trait]
impl AgentRunSurfaceProjectionContextResolver for SessionCapabilityService {
    async fn resolve_surface_projection_context(
        &self,
        source: AgentRunSurfaceProjectionContextSource,
    ) -> Result<AgentRunSurfaceProjectionContext, AgentRunFrameSurfaceError> {
        self.resolve_agent_run_surface_projection_context(source)
            .await
            .map_err(AgentRunFrameSurfaceError::ProjectionContextUnavailable)
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
    entry: &agentdash_application_ports::mcp_discovery::DiscoveredMcpTool,
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
