//! Hub 的工具构建与运行时 MCP 热更职责。
//!
//! 集中：
//! - runtime tool + 直连 MCP + relay MCP 的运行中重构。
//! - `get_runtime_mcp_servers` / `get_current_capability_state`：读取当前能力状态。
//!
use agentdash_agent_types::DynAgentTool;
use agentdash_application_ports::agent_run_surface::RuntimeSurfaceQueryPurpose;
use agentdash_application_ports::runtime_surface_adoption::{
    AgentFrameRuntimeTarget, RuntimeSurfaceAdoptionError, RuntimeSurfaceAdoptionPort,
};
use agentdash_diagnostics::{Subsystem, diag};
use agentdash_spi::{ConnectorError, ExecutionContext};
use async_trait::async_trait;

use super::{LiveRuntimeContextTransitionInput, SessionRuntimeInner};
use crate::session::tool_assembly::{
    AssembledToolSurface, assemble_tool_surface_for_execution_context,
};
use crate::session::types::CapabilityState;

impl SessionRuntimeInner {
    /// 读取 session 当前 turn 生效的能力状态（AgentFrame revision 的内存投影）。
    pub async fn get_current_capability_state(&self, session_id: &str) -> Option<CapabilityState> {
        self.runtime_registry
            .with_runtime(session_id, |runtime| {
                runtime
                    .and_then(|runtime| runtime.turn_state.active_turn())
                    .map(|turn| turn.capability_state.clone())
            })
            .await
    }

    /// 读取当前 active turn 或 RuntimeSession live profile 的 capability 投影。
    ///
    /// 这里不再从 delivery anchor 反查 current AgentFrame；AgentRun current surface
    /// query / effective capability port 才拥有业务 surface 读取语义。
    pub async fn get_latest_capability_state(&self, session_id: &str) -> Option<CapabilityState> {
        self.runtime_registry
            .with_runtime(session_id, |runtime| {
                let runtime = runtime?;
                if let Some(turn) = runtime.turn_state.active_turn() {
                    return Some(turn.capability_state.clone());
                }
                runtime
                    .session_profile
                    .as_ref()
                    .map(|profile| profile.capability_state.clone())
            })
            .await
    }

    /// 将已持久化的 AgentFrame revision 采用到 active runtime。
    ///
    /// 该 helper 不写入新的 frame；它通过 delivery anchor 校验调用方指定的
    /// frame 是当前生效 revision，并同步 active turn cache、connector tools
    /// 与 hook runtime target。
    pub(crate) async fn adopt_persisted_frame_revision_into_active_runtime(
        &self,
        target: AgentFrameRuntimeTarget,
    ) -> Result<Vec<DynAgentTool>, ConnectorError> {
        let session_id = target.delivery_runtime_session_id.as_str();
        let surface_query = self.runtime_surface_query.as_ref().ok_or_else(|| {
            ConnectorError::Runtime(format!(
                "session `{session_id}` 无 AgentRun runtime surface query port，无法采用已持久化能力状态"
            ))
        })?;
        let surface = surface_query
            .current_runtime_surface(
                session_id,
                RuntimeSurfaceQueryPurpose::new("runtime_surface_adoption"),
            )
            .await
            .map_err(|error| {
                ConnectorError::Runtime(format!(
                    "查询 delivery RuntimeSession `{session_id}` current AgentRun surface 失败，无法采用已持久化能力状态: {error}"
                ))
            })?;
        if surface.current_surface_frame_id != target.frame_id {
            return Err(ConnectorError::Runtime(format!(
                "AgentFrame `{}` 不是 delivery RuntimeSession `{session_id}` 当前 revision（当前为 `{}`），拒绝采用不同 revision",
                target.frame_id, surface.current_surface_frame_id
            )));
        }
        let state = surface.capability_state.clone();
        let mcp_servers = surface.mcp_servers.clone();
        let phase_node = surface.provenance.surface_created_by_kind.clone();
        let adopted_frame_id = surface.current_surface_frame_id;
        let agent_id = surface.agent_id;
        let revision = surface.surface_revision;

        let turn_snapshot = self
            .runtime_registry
            .with_runtime(session_id, |runtime| {
                let runtime = runtime.ok_or_else(|| {
                    ConnectorError::Runtime(format!(
                        "session `{session_id}` 当前没有运行态，无法采用已持久化能力状态"
                    ))
                })?;
                let turn = runtime.turn_state.active_turn().cloned().ok_or_else(|| {
                    ConnectorError::Runtime(format!(
                        "session `{session_id}` 没有活跃 turn，无法采用已持久化能力状态"
                    ))
                })?;
                Ok::<_, ConnectorError>(turn)
            })
            .await?;
        let hook_runtime = self
            .hook_service()
            .ensure_hook_runtime_for_target(
                &AgentFrameRuntimeTarget {
                    frame_id: adopted_frame_id,
                    delivery_runtime_session_id: session_id.to_string(),
                },
                Some(&turn_snapshot.turn_id),
            )
            .await?;

        let mut session_frame = turn_snapshot.session_frame.clone();
        session_frame.turn_id = turn_snapshot.turn_id.clone();
        session_frame.mcp_servers = mcp_servers.clone();
        session_frame.vfs = state.vfs.active.clone();
        let context = ExecutionContext {
            session: session_frame,
            turn: agentdash_spi::ExecutionTurnFrame {
                hook_runtime: hook_runtime.clone(),
                capability_state: state.clone(),
                ..Default::default()
            },
        };
        let tool_surface = self
            .assemble_tool_surface_for_execution_context(session_id, &context)
            .await;
        let all_tools = tool_surface.tools;
        let all_tool_schemas = tool_surface.schemas;

        self.connector
            .update_session_tools(session_id, all_tools.clone())
            .await?;

        self.runtime_registry
            .with_runtime_mut(session_id, |runtime| {
                if let Some(runtime) = runtime {
                    runtime.session_profile = Some(super::super::hub_support::SessionProfile {
                        capability_state: state.clone(),
                    });
                    if let Some(turn) = runtime.turn_state.active_turn_mut() {
                        turn.session_frame.mcp_servers = mcp_servers.clone();
                        turn.session_frame.vfs = state.vfs.active.clone();
                        turn.capability_state = state.clone();
                    }
                }
            })
            .await;

        if let Some(hook_runtime) = hook_runtime.as_ref() {
            self.emit_adopted_runtime_context_transition(
                hook_runtime,
                LiveRuntimeContextTransitionInput {
                    delivery_runtime_session_id: session_id.to_string(),
                    turn_id: Some(turn_snapshot.turn_id.clone()),
                    phase_node,
                    before_state: Some(turn_snapshot.capability_state),
                    after_state: state.clone(),
                    capability_keys: state.capability_keys(),
                    key_delta: agentdash_spi::SetDelta::default(),
                    apply_mode: "persisted_revision_adopted",
                },
                &all_tool_schemas,
            )
            .await
            .map_err(|error| {
                ConnectorError::Runtime(format!("已持久化 AgentFrame adoption 通知失败: {error}"))
            })?;
        }

        diag!(Debug, Subsystem::AgentRun,

            session_id,
            target_frame_id = %target.frame_id,
            adopted_frame_id = %adopted_frame_id,
            agent_id = %agent_id,
            revision = revision,
            "已采用持久化 AgentFrame capability revision"
        );
        Ok(all_tools)
    }

    pub(crate) async fn assemble_tool_surface_for_execution_context(
        &self,
        session_id: &str,
        context: &ExecutionContext,
    ) -> AssembledToolSurface {
        let context = self
            .execution_context_with_agent_run_effective_capability(session_id, context)
            .await;
        assemble_tool_surface_for_execution_context(
            session_id,
            &context,
            self.runtime_tool_provider.as_deref(),
            self.mcp_tool_discovery.as_deref(),
        )
        .await
    }

    async fn execution_context_with_agent_run_effective_capability(
        &self,
        session_id: &str,
        context: &ExecutionContext,
    ) -> ExecutionContext {
        let mut context = context.clone();
        context.turn.capability_state = self
            .capability_state_with_agent_run_admission_projection(
                session_id,
                &context.turn.capability_state,
            )
            .await;
        context
    }

    async fn capability_state_with_agent_run_admission_projection(
        &self,
        session_id: &str,
        capability_state: &CapabilityState,
    ) -> CapabilityState {
        let Some(port) = self.effective_capability_port.as_ref() else {
            return capability_state.clone();
        };

        match port
            .execution_capability_state_for_runtime_session(session_id, capability_state.clone())
            .await
        {
            Ok(state) => state,
            Err(error) => {
                diag!(
                    Warn,
                    Subsystem::AgentRun,
                    session_id,
                    "AgentRun effective capability projection skipped: {error}"
                );
                capability_state.clone()
            }
        }
    }
}

#[async_trait]
impl RuntimeSurfaceAdoptionPort for SessionRuntimeInner {
    async fn adopt_runtime_surface(
        &self,
        target: AgentFrameRuntimeTarget,
    ) -> Result<Vec<DynAgentTool>, RuntimeSurfaceAdoptionError> {
        SessionRuntimeInner::adopt_persisted_frame_revision_into_active_runtime(self, target)
            .await
            .map_err(|error| RuntimeSurfaceAdoptionError::Failed {
                message: error.to_string(),
            })
    }
}
