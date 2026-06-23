//! Hub 的工具构建与运行时 MCP 热更职责。
//!
//! 集中：
//! - runtime tool + 直连 MCP + relay MCP 的运行中重构。
//! - `get_runtime_mcp_servers` / `get_current_capability_state`：读取当前能力状态。
//!
use agentdash_agent_types::DynAgentTool;
use agentdash_spi::{ConnectorError, ExecutionContext, RuntimeBackendAnchor};
use uuid::Uuid;

use super::{LiveRuntimeContextTransitionInput, SessionRuntimeInner};
use crate::agent_run::AgentRunEffectiveCapabilityService;
use crate::agent_run::frame::surface::AgentFrameSurfaceExt;
use crate::lifecycle::resolve_current_frame_from_delivery_trace_ref;
use crate::session::capability_state::project_capability_state_from_frame;
use crate::session::tool_assembly::{
    AssembledToolSurface, assemble_tool_surface_for_execution_context,
};
use crate::session::types::{AgentFrameRuntimeTarget, CapabilityState};

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

    /// 读取当前 active turn 的 capability 投影；若没有 active turn，则回退到 session_profile 缓存。
    ///
    /// 两级缓存均为 AgentFrame revision 的投影。
    pub async fn get_latest_capability_state(&self, session_id: &str) -> Option<CapabilityState> {
        let cached = self
            .runtime_registry
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
            .await;
        if cached.is_some() {
            return cached;
        }

        let (Some(anchor_repo), Some(agent_repo), Some(frame_repo)) = (
            self.execution_anchor_repo.as_ref(),
            self.lifecycle_agent_repo.as_ref(),
            self.agent_frame_repo.as_ref(),
        ) else {
            return None;
        };
        resolve_current_frame_from_delivery_trace_ref(
            session_id,
            anchor_repo.as_ref(),
            agent_repo.as_ref(),
            frame_repo.as_ref(),
        )
        .await
        .ok()
        .flatten()
        .map(|(_anchor, _agent, frame)| project_capability_state_from_frame(&frame))
    }

    pub async fn get_current_runtime_backend_anchor(
        &self,
        session_id: &str,
    ) -> Result<RuntimeBackendAnchor, ConnectorError> {
        let anchor = self
            .runtime_registry
            .with_runtime(session_id, |runtime| {
                runtime
                    .and_then(|runtime| runtime.turn_state.active_turn())
                    .and_then(|turn| turn.session_frame.runtime_backend_anchor.clone())
            })
            .await;
        anchor.ok_or_else(|| {
            ConnectorError::Runtime(
                agentdash_spi::RuntimeBackendAnchorError::Missing {
                    component: "session_runtime".to_string(),
                    session_id: Some(session_id.to_string()),
                    turn_id: None,
                }
                .to_string(),
            )
        })
    }

    /// Runtime adapter 边界使用的 lookup：把 delivery RuntimeSession 解析为当前 AgentFrame。
    ///
    /// 后续 command 必须携带 `AgentFrameRuntimeTarget`，不能把 raw session id
    /// 继续作为 frame revision 写入目标。
    pub(crate) async fn resolve_runtime_session_frame_id(
        &self,
        session_id: &str,
    ) -> Result<Uuid, ConnectorError> {
        let frame_repo = self.agent_frame_repo.as_ref().ok_or_else(|| {
            ConnectorError::Runtime(format!(
                "session `{session_id}` 无 AgentFrame repository，无法解析 runtime surface target"
            ))
        })?;
        let anchor_repo = self.execution_anchor_repo.as_ref().ok_or_else(|| {
            ConnectorError::Runtime(format!(
                "session `{session_id}` 无 RuntimeSessionExecutionAnchor repository，无法解析 runtime surface target"
            ))
        })?;
        let agent_repo = self.lifecycle_agent_repo.as_ref().ok_or_else(|| {
            ConnectorError::Runtime(format!(
                "session `{session_id}` 无 LifecycleAgent repository，无法解析 runtime surface target"
            ))
        })?;
        let (_anchor, _agent, frame) = resolve_current_frame_from_delivery_trace_ref(
            session_id,
            anchor_repo.as_ref(),
            agent_repo.as_ref(),
            frame_repo.as_ref(),
        )
        .await
        .map_err(|error| {
            ConnectorError::Runtime(format!(
                "通过 anchor 查找 session `{session_id}` 当前 AgentFrame 失败: {error}"
            ))
        })?
        .ok_or_else(|| {
            ConnectorError::Runtime(format!(
                "session `{session_id}` 未关联 AgentFrame，无法解析 runtime surface target"
            ))
        })?;
        Ok(frame.id)
    }

    /// 将已持久化的 AgentFrame revision 采用到 active runtime。
    ///
    /// 该 helper 不写入新的 frame；它通过 delivery anchor 校验调用方指定的
    /// frame 是当前生效 revision，并同步 active turn cache、connector tools
    /// 与 hook runtime target。
    pub(crate) async fn adopt_persisted_agent_frame_revision(
        &self,
        target: AgentFrameRuntimeTarget,
    ) -> Result<Vec<DynAgentTool>, ConnectorError> {
        let session_id = target.delivery_runtime_session_id.as_str();
        let frame_repo = self.agent_frame_repo.as_ref().ok_or_else(|| {
            ConnectorError::Runtime(format!(
                "session `{session_id}` 无 AgentFrame repository，无法采用已持久化能力状态"
            ))
        })?;
        let anchor_repo = self.execution_anchor_repo.as_ref().ok_or_else(|| {
            ConnectorError::Runtime(format!(
                "session `{session_id}` 无 RuntimeSessionExecutionAnchor repository，无法采用已持久化能力状态"
            ))
        })?;
        let agent_repo = self.lifecycle_agent_repo.as_ref().ok_or_else(|| {
            ConnectorError::Runtime(format!(
                "session `{session_id}` 无 LifecycleAgent repository，无法采用已持久化能力状态"
            ))
        })?;
        let target_frame = frame_repo
            .get(target.frame_id)
            .await
            .map_err(|error| {
                ConnectorError::Runtime(format!(
                    "查找 AgentFrame `{}` 失败，无法采用已持久化能力状态: {error}",
                    target.frame_id
                ))
            })?
            .ok_or_else(|| {
                ConnectorError::Runtime(format!(
                    "AgentFrame `{}` 不存在，拒绝采用已持久化能力状态",
                    target.frame_id
                ))
            })?;
        let (delivery_anchor, _agent, adopted_frame) = resolve_current_frame_from_delivery_trace_ref(
            session_id,
            anchor_repo.as_ref(),
            agent_repo.as_ref(),
            frame_repo.as_ref(),
        )
            .await
            .map_err(|error| {
                ConnectorError::Runtime(format!(
                    "通过 anchor 查找 delivery RuntimeSession `{session_id}` 当前 AgentFrame 失败，无法采用已持久化能力状态: {error}"
                ))
            })?
            .ok_or_else(|| {
                ConnectorError::Runtime(format!(
                    "delivery RuntimeSession `{session_id}` 缺少可用 RuntimeSessionExecutionAnchor/AgentFrame，拒绝采用已持久化能力状态"
                ))
            })?;
        if delivery_anchor.agent_id != target_frame.agent_id {
            return Err(ConnectorError::Runtime(format!(
                "Agent `{}` 未绑定 delivery RuntimeSession `{session_id}` 的 anchor，拒绝采用已持久化能力状态",
                target_frame.agent_id
            )));
        }
        if adopted_frame.id != target.frame_id {
            return Err(ConnectorError::Runtime(format!(
                "AgentFrame `{}` 不是 delivery RuntimeSession `{session_id}` 当前 revision（当前为 `{}`），拒绝采用不同 revision",
                target.frame_id, adopted_frame.id
            )));
        }
        let state = project_capability_state_from_frame(&adopted_frame);
        let mcp_servers = adopted_frame.typed_mcp_servers();

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
                    frame_id: adopted_frame.id,
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
                    phase_node: adopted_frame.created_by_kind.clone(),
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

        tracing::debug!(
            session_id,
            target_frame_id = %target.frame_id,
            adopted_frame_id = %adopted_frame.id,
            agent_id = %adopted_frame.agent_id,
            revision = adopted_frame.revision,
            "已采用持久化 AgentFrame capability revision"
        );
        Ok(all_tools)
    }

    #[cfg(test)]
    pub(crate) async fn assemble_tools_for_execution_context(
        &self,
        session_id: &str,
        context: &ExecutionContext,
    ) -> Vec<DynAgentTool> {
        self.assemble_tool_surface_for_execution_context(session_id, context)
            .await
            .tools
    }

    pub(crate) async fn assemble_tool_surface_for_execution_context(
        &self,
        session_id: &str,
        context: &ExecutionContext,
    ) -> AssembledToolSurface {
        let context = self
            .execution_context_with_agent_run_admission_projection(session_id, context)
            .await;
        assemble_tool_surface_for_execution_context(
            session_id,
            &context,
            self.runtime_tool_provider.as_deref(),
            self.mcp_tool_discovery.as_deref(),
        )
        .await
    }

    async fn execution_context_with_agent_run_admission_projection(
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
        let (Some(permission_grant_repo), Some(anchor_repo)) = (
            self.permission_grant_repo.as_ref(),
            self.execution_anchor_repo.as_ref(),
        ) else {
            return capability_state.clone();
        };

        match AgentRunEffectiveCapabilityService::execution_capability_state_for_runtime_session(
            session_id,
            capability_state,
            anchor_repo.as_ref(),
            permission_grant_repo.as_ref(),
        )
        .await
        {
            Ok(state) => state,
            Err(error) => {
                tracing::warn!(
                    session_id,
                    "AgentRun execution capability projection skipped: {error}"
                );
                capability_state.clone()
            }
        }
    }
}
