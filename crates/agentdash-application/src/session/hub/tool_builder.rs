//! Hub 的工具构建与运行时 MCP 热更职责。
//!
//! 集中：
//! - runtime tool + 直连 MCP + relay MCP 的运行中重构。
//! - `get_runtime_mcp_servers` / `get_current_capability_state`：读取当前能力状态。
//! - `replace_current_capability_state`：底层热更 primitive，仅供
//!   `runtime_context_transition` 统一 applier 调用。
//!
use agentdash_agent_types::DynAgentTool;
use agentdash_application_ports::mcp_discovery::{DiscoveredMcpTool, McpToolDiscoveryRequest};
use agentdash_spi::{ConnectorError, ExecutionContext, RuntimeMcpServer};
use uuid::Uuid;

use super::SessionRuntimeInner;
use crate::agent_run::AgentFrameBuilder;
use crate::agent_run::frame::surface::AgentFrameSurfaceExt;
use crate::lifecycle::resolve_current_frame_from_delivery_trace_ref;
use crate::session::capability_state::project_capability_state_from_frame;
use crate::session::tool_assembly::assemble_tools_for_execution_context;
use crate::session::types::{AgentFrameRuntimeTarget, CapabilityState};

impl SessionRuntimeInner {
    /// 读取 delivery RuntimeSession 当前生效的 MCP server 列表。
    ///
    /// Active turn 返回 connector session frame 的执行快照；idle 时通过
    /// `RuntimeSessionExecutionAnchor` 反查当前 AgentFrame surface。
    pub async fn get_runtime_mcp_servers(&self, session_id: &str) -> Vec<RuntimeMcpServer> {
        let active = self
            .runtime_registry
            .with_runtime(session_id, |runtime| {
                runtime
                    .and_then(|runtime| runtime.turn_state.active_turn())
                    .map(|turn| turn.session_frame.mcp_servers.clone())
            })
            .await;
        if let Some(servers) = active {
            return servers;
        }

        let (Some(anchor_repo), Some(agent_repo), Some(frame_repo)) = (
            self.execution_anchor_repo.as_ref(),
            self.lifecycle_agent_repo.as_ref(),
            self.agent_frame_repo.as_ref(),
        ) else {
            return Vec::new();
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
        .map(|(_anchor, _agent, frame)| frame.typed_mcp_servers())
        .unwrap_or_default()
    }

    /// 读取 session 当前 turn 生效的能力状态（AgentFrame 投影缓存）。
    ///
    /// 返回的 `CapabilityState` 是 AgentFrame revision 的内存投影。
    /// 写入通过 `replace_current_capability_state` → AgentFrameBuilder → frame revision，
    /// 然后同步更新此缓存。
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
    /// 两级缓存均为 AgentFrame revision 的投影，写入路径统一经
    /// `replace_current_capability_state` → AgentFrame revision → 内存同步。
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
    /// 该 helper 不写入新的 frame；它只读取当前最新 frame fact，并同步 active
    /// turn cache、connector tools 与 hook runtime target。
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
        let delivery_anchor = anchor_repo
            .find_by_session(&target.delivery_runtime_session_id)
            .await
            .map_err(|error| {
                ConnectorError::Runtime(format!(
                    "查找 delivery RuntimeSession `{session_id}` anchor 失败，无法采用已持久化能力状态: {error}"
                ))
            })?;
        if delivery_anchor.is_none_or(|anchor| anchor.agent_id != target_frame.agent_id) {
            return Err(ConnectorError::Runtime(format!(
                "Agent `{}` 未绑定 delivery RuntimeSession `{session_id}` 的 anchor，拒绝采用已持久化能力状态",
                target_frame.agent_id
            )));
        }
        let adopted_frame = frame_repo
            .get_current(target_frame.agent_id)
            .await
            .map_err(|error| {
                ConnectorError::Runtime(format!(
                    "查找 Agent `{}` 当前 AgentFrame 失败，无法采用已持久化能力状态: {error}",
                    target_frame.agent_id
                ))
            })?
            .unwrap_or(target_frame);
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
                hook_runtime,
                capability_state: state.clone(),
                ..Default::default()
            },
        };
        let all_tools = self
            .assemble_tools_for_execution_context(session_id, &context)
            .await;

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
                        turn.capability_state = state;
                    }
                }
            })
            .await;

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

    /// 替换运行中 session 的能力状态并同步 connector。
    ///
    /// 写入路径：AgentFrame revision（持久化权威） → 内存 cache → connector 同步。
    /// 当 `agent_frame_repo` 可用时，先通过 `AgentFrameBuilder` 生成新 revision，
    /// 再从该 revision 投影出 `CapabilityState` 更新内存缓存。
    pub(crate) async fn replace_current_capability_state(
        &self,
        target: AgentFrameRuntimeTarget,
        state: CapabilityState,
    ) -> Result<Vec<DynAgentTool>, ConnectorError> {
        let session_id = target.delivery_runtime_session_id.as_str();
        // === Phase 1: AgentFrame revision 持久化 ===
        let frame_repo = self.agent_frame_repo.as_ref().ok_or_else(|| {
            ConnectorError::Runtime(format!(
                "session `{session_id}` 无 AgentFrame repository，无法热更新能力状态"
            ))
        })?;
        let anchor_repo = self.execution_anchor_repo.as_ref().ok_or_else(|| {
            ConnectorError::Runtime(format!(
                "session `{session_id}` 无 RuntimeSessionExecutionAnchor repository，无法热更新能力状态"
            ))
        })?;
        let target_frame = frame_repo
            .get(target.frame_id)
            .await
            .map_err(|error| {
                ConnectorError::Runtime(format!(
                    "查找 AgentFrame `{}` 失败，无法热更新能力状态: {error}",
                    target.frame_id
                ))
            })?
            .ok_or_else(|| {
                ConnectorError::Runtime(format!(
                    "AgentFrame `{}` 不存在，拒绝热更新能力状态",
                    target.frame_id
                ))
            })?;
        let delivery_anchor = anchor_repo
            .find_by_session(&target.delivery_runtime_session_id)
            .await
            .map_err(|error| {
                ConnectorError::Runtime(format!(
                    "查找 delivery RuntimeSession `{session_id}` anchor 失败，无法热更新能力状态: {error}"
                ))
            })?;
        if delivery_anchor.is_none_or(|anchor| anchor.agent_id != target_frame.agent_id) {
            return Err(ConnectorError::Runtime(format!(
                "Agent `{}` 未绑定 delivery RuntimeSession `{session_id}` 的 anchor，拒绝热更新能力状态",
                target_frame.agent_id
            )));
        }
        let current_frame = frame_repo
            .get_current(target_frame.agent_id)
            .await
            .map_err(|error| {
                ConnectorError::Runtime(format!(
                    "查找 Agent `{}` 当前 AgentFrame 失败，无法热更新能力状态: {error}",
                    target_frame.agent_id
                ))
            })?
            .unwrap_or_else(|| target_frame.clone());
        let mut builder = AgentFrameBuilder::new(target_frame.agent_id)
            .with_capability_state(&state)
            .with_runtime_session(session_id.to_string())
            .with_created_by(
                "runtime_context_transition",
                Some(target.frame_id.to_string()),
            );
        if let Some(ctx) = current_frame.context_slice_json {
            builder = builder.with_context(ctx);
        }
        if let Some(profile) = current_frame.execution_profile_json {
            builder = builder.with_execution_profile_raw(profile);
        }
        let new_frame = builder.build(frame_repo.as_ref()).await.map_err(|error| {
            ConnectorError::Runtime(format!(
                "AgentFrame revision 写入失败，拒绝热更新能力状态: {error}"
            ))
        })?;
        tracing::debug!(
            session_id,
            target_frame_id = %target.frame_id,
            agent_id = %new_frame.agent_id,
            revision = new_frame.revision,
            "AgentFrame capability revision 已写入"
        );

        self.adopt_persisted_agent_frame_revision(AgentFrameRuntimeTarget {
            frame_id: new_frame.id,
            delivery_runtime_session_id: session_id.to_string(),
        })
        .await
    }

    pub(crate) async fn assemble_tools_for_execution_context(
        &self,
        session_id: &str,
        context: &ExecutionContext,
    ) -> Vec<DynAgentTool> {
        assemble_tools_for_execution_context(
            session_id,
            context,
            self.runtime_tool_provider.as_deref(),
            self.mcp_tool_discovery.as_deref(),
        )
        .await
    }

    pub(in crate::session) async fn discover_runtime_mcp_tool_entries(
        &self,
        session_id: &str,
    ) -> Result<Vec<DiscoveredMcpTool>, ConnectorError> {
        let active_surface = self
            .runtime_registry
            .with_runtime(session_id, |runtime| {
                runtime
                    .and_then(|runtime| runtime.turn_state.active_turn())
                    .map(|turn| {
                        (
                            turn.session_frame.mcp_servers.clone(),
                            turn.capability_state.clone(),
                        )
                    })
            })
            .await;
        let (servers, capability_state) = if let Some(surface) = active_surface {
            surface
        } else {
            let (Some(anchor_repo), Some(agent_repo), Some(frame_repo)) = (
                self.execution_anchor_repo.as_ref(),
                self.lifecycle_agent_repo.as_ref(),
                self.agent_frame_repo.as_ref(),
            ) else {
                return Err(ConnectorError::Runtime(format!(
                    "session `{session_id}` 缺少 AgentFrame surface repository，无法发现 MCP 工具"
                )));
            };
            let (_anchor, _agent, frame) = resolve_current_frame_from_delivery_trace_ref(
                session_id,
                anchor_repo.as_ref(),
                agent_repo.as_ref(),
                frame_repo.as_ref(),
            )
            .await
            .map_err(|error| {
                ConnectorError::Runtime(format!(
                    "通过 anchor 查找 session `{session_id}` 当前 AgentFrame surface 失败: {error}"
                ))
            })?
            .ok_or_else(|| {
                ConnectorError::Runtime(format!(
                    "session `{session_id}` 未关联当前 AgentFrame surface，无法发现 MCP 工具"
                ))
            })?;
            (
                frame.typed_mcp_servers(),
                project_capability_state_from_frame(&frame),
            )
        };
        let discovery = self.mcp_tool_discovery.as_ref().ok_or_else(|| {
            ConnectorError::Runtime("SessionRuntimeInner 缺少 mcp_tool_discovery".to_string())
        })?;

        discovery
            .discover_tool_entries(McpToolDiscoveryRequest {
                servers,
                capability_state,
                call_context: None,
            })
            .await
    }
}
