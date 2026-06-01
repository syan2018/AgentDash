//! Hub 的工具构建与运行时 MCP 热更职责。
//!
//! 集中：
//! - `build_tools_for_execution_context`：runtime tool + 直连 MCP + relay MCP
//!   的合并发现（由 launch preparation 在 prompt 前预构建，或 replace_current_capability_state
//!   在运行中热更时重构）。
//! - `get_runtime_mcp_servers` / `get_current_capability_state`：读取当前能力状态。
//! - `replace_current_capability_state`：底层热更 primitive，仅供
//!   `runtime_context_transition` 统一 applier 调用。
//!
//! 注：本文件仍依赖 `agentdash_executor::mcp::discover_*`。该依赖早于 PR 6 存在
//! （application 层直接调用 executor 实现），PRD 允许"通过 tool_builder 间接依赖"，
//! 不在本 PR 改接口层级。

use agentdash_agent_types::DynAgentTool;
use agentdash_executor::mcp::DiscoveredMcpTool;
use agentdash_spi::{ConnectorError, ExecutionContext, SessionMcpServer};

use super::SessionRuntimeInner;
use crate::session::types::CapabilityState;
use crate::workflow::AgentFrameBuilder;

impl SessionRuntimeInner {
    /// 读取 session 当前 turn 生效的 MCP server 列表（由 prompt pipeline 维护）。
    pub async fn get_runtime_mcp_servers(&self, session_id: &str) -> Vec<SessionMcpServer> {
        self.runtime_registry
            .with_runtime(session_id, |runtime| {
                runtime
                    .and_then(|runtime| runtime.turn_state.active_turn())
                    .map(|turn| turn.session_frame.mcp_servers.clone())
                    .unwrap_or_default()
            })
            .await
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

    /// 替换运行中 session 的能力状态并同步 connector。
    ///
    /// 写入路径：AgentFrame revision（持久化权威） → 内存 cache → connector 同步。
    /// 当 `agent_frame_repo` 可用时，先通过 `AgentFrameBuilder` 生成新 revision，
    /// 再从该 revision 投影出 `CapabilityState` 更新内存缓存。
    pub(crate) async fn replace_current_capability_state(
        &self,
        session_id: &str,
        mut state: CapabilityState,
    ) -> Result<Vec<DynAgentTool>, ConnectorError> {
        // === Phase 1: AgentFrame revision 持久化 ===
        if let Some(ref frame_repo) = self.agent_frame_repo {
            match frame_repo.find_by_runtime_session(session_id).await {
                Ok(Some(current_frame)) => {
                    let mut builder = AgentFrameBuilder::new(current_frame.agent_id)
                        .with_capability_state(&state)
                        .with_created_by(
                            "runtime_context_transition",
                            Some(session_id.to_string()),
                        );
                    if let Some(ctx) = current_frame.context_slice_json {
                        builder = builder.with_context(ctx);
                    }
                    if let Some(profile) = current_frame.execution_profile_json {
                        builder = builder.with_execution_profile_raw(profile);
                    }
                    match builder.build(frame_repo.as_ref()).await {
                        Ok(new_frame) => {
                            tracing::debug!(
                                session_id,
                                agent_id = %new_frame.agent_id,
                                revision = new_frame.revision,
                                "AgentFrame capability revision 已写入"
                            );
                        }
                        Err(error) => {
                            tracing::warn!(
                                session_id,
                                "AgentFrame revision 写入失败，仅更新内存缓存: {error}"
                            );
                        }
                    }
                }
                Ok(None) => {
                    tracing::debug!(
                        session_id,
                        "Session 尚未关联 AgentFrame，跳过 frame revision 写入"
                    );
                }
                Err(error) => {
                    tracing::warn!(session_id, "查找 session 关联的 AgentFrame 失败: {error}");
                }
            }
        }

        // === Phase 2: 内存 cache 更新 + connector 同步 ===
        let (turn_snapshot, hook_runtime) = self
            .runtime_registry
            .with_runtime(session_id, |runtime| {
                let runtime = runtime.ok_or_else(|| {
                    ConnectorError::Runtime(format!(
                        "session `{session_id}` 当前没有运行态，无法热更新能力状态"
                    ))
                })?;
                let turn = runtime.turn_state.active_turn().cloned().ok_or_else(|| {
                    ConnectorError::Runtime(format!(
                        "session `{session_id}` 没有活跃 turn，无法热更新能力状态"
                    ))
                })?;
                Ok::<_, ConnectorError>((turn, runtime.hook_runtime.clone()))
            })
            .await?;

        let mut session_frame = turn_snapshot.session_frame.clone();
        session_frame.turn_id = turn_snapshot.turn_id.clone();
        session_frame.mcp_servers = state.tool.mcp_servers.clone();
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
            .build_tools_for_execution_context(session_id, &context, &state.tool.mcp_servers)
            .await;

        self.connector
            .update_session_tools(session_id, all_tools.clone())
            .await?;

        self.runtime_registry
            .with_runtime_mut(session_id, |runtime| {
                if let Some(runtime) = runtime {
                    if state.vfs.active.is_none() {
                        let fallback_vfs = runtime
                            .turn_state
                            .active_turn()
                            .and_then(|turn| turn.capability_state.vfs.active.clone())
                            .or_else(|| {
                                runtime
                                    .session_profile
                                    .as_ref()
                                    .and_then(|p| p.capability_state.vfs.active.clone())
                            });
                        state.vfs.active = fallback_vfs;
                    }
                    runtime.session_profile = Some(super::super::hub_support::SessionProfile {
                        capability_state: state.clone(),
                    });
                    if let Some(turn) = runtime.turn_state.active_turn_mut() {
                        turn.session_frame.mcp_servers = state.tool.mcp_servers.clone();
                        turn.session_frame.vfs = state.vfs.active.clone();
                        turn.capability_state = state;
                    }
                }
            })
            .await;
        Ok(all_tools)
    }

    pub(crate) async fn build_tools_for_execution_context(
        &self,
        session_id: &str,
        context: &ExecutionContext,
        mcp_servers: &[agentdash_spi::SessionMcpServer],
    ) -> Vec<DynAgentTool> {
        use agentdash_executor::mcp::{self as mcp_discovery};

        let mut all_tools: Vec<DynAgentTool> = Vec::new();

        if let Some(provider) = &self.runtime_tool_provider {
            match provider.build_tools(context).await {
                Ok(tools) => all_tools.extend(tools),
                Err(e) => tracing::warn!(
                    session_id = %session_id,
                    "runtime tool 构建失败: {e}"
                ),
            }
        }

        let (relay_names, direct_servers) =
            agentdash_spi::partition_session_mcp_servers(mcp_servers);
        match mcp_discovery::discover_mcp_tools(&direct_servers, &context.turn.capability_state)
            .await
        {
            Ok(tools) => all_tools.extend(tools),
            Err(e) => tracing::warn!(
                session_id = %session_id,
                "直连 MCP 工具发现失败: {e}"
            ),
        }

        if let Some(relay) = &self.mcp_relay_provider {
            let call_context = agentdash_spi::RelayMcpCallContext {
                session_id: session_id.to_string(),
                turn_id: Some(context.session.turn_id.clone()),
                tool_call_id: None,
                vfs: context.session.vfs.clone(),
                identity: context.session.identity.clone(),
            };
            let tools = mcp_discovery::discover_relay_mcp_tools(
                relay.clone(),
                &relay_names,
                &context.turn.capability_state,
                Some(call_context),
            )
            .await;
            all_tools.extend(tools);
        }

        all_tools
    }

    pub(in crate::session) async fn discover_runtime_mcp_tool_entries(
        &self,
        session_id: &str,
    ) -> Result<Vec<DiscoveredMcpTool>, ConnectorError> {
        use agentdash_executor::mcp::{self as mcp_discovery};

        let capability_state = self
            .get_latest_capability_state(session_id)
            .await
            .ok_or_else(|| {
                ConnectorError::Runtime(format!(
                    "session `{session_id}` 当前没有可用 CapabilityState"
                ))
            })?;
        let mcp_servers = capability_state.tool.mcp_servers.clone();
        let (relay_names, direct_servers) =
            agentdash_spi::partition_session_mcp_servers(&mcp_servers);
        let mut entries =
            mcp_discovery::discover_mcp_tool_entries(&direct_servers, &capability_state).await?;

        if let Some(relay) = &self.mcp_relay_provider {
            entries.extend(
                mcp_discovery::discover_relay_mcp_tool_entries(
                    relay.clone(),
                    &relay_names,
                    &capability_state,
                    None,
                )
                .await,
            );
        } else if !relay_names.is_empty() {
            tracing::warn!(
                session_id = %session_id,
                "Session 声明了 relay MCP server，但当前没有 mcp_relay_provider"
            );
        }

        Ok(entries)
    }
}
