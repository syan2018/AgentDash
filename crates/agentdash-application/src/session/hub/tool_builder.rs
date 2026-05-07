//! Hub 的工具构建与运行时 MCP 热更职责。
//!
//! 集中：
//! - `build_tools_for_execution_context`：runtime tool + 直连 MCP + relay MCP
//!   的合并发现（由 prompt_pipeline 在 prompt 前预构建，或 replace_current_capability_surface
//!   在运行中热更时重构）。
//! - `get_runtime_mcp_servers` / `get_current_capability_surface`：读取当前能力表面。
//! - `replace_current_capability_surface`：底层热更 primitive，仅供
//!   `runtime_context_transition` 统一 applier 调用。
//!
//! 注：本文件仍依赖 `agentdash_executor::mcp::discover_*`。该依赖早于 PR 6 存在
//! （application 层直接调用 executor 实现），PRD 允许"通过 tool_builder 间接依赖"，
//! 不在本 PR 改接口层级。

use agentdash_agent_types::DynAgentTool;
use agentdash_spi::{ConnectorError, ExecutionContext, SessionMcpServer};

use super::SessionHub;
use crate::session::types::CapabilitySurface;

impl SessionHub {
    /// 读取 session 当前 turn 生效的 MCP server 列表（由 prompt pipeline 维护）。
    pub async fn get_runtime_mcp_servers(&self, session_id: &str) -> Vec<SessionMcpServer> {
        let sessions = self.sessions.lock().await;
        sessions
            .get(session_id)
            .and_then(|runtime| runtime.turn_state.active_turn())
            .map(|turn| turn.session_frame.mcp_servers.clone())
            .unwrap_or_default()
    }

    /// 读取 session 当前 turn 生效的能力表面。
    pub async fn get_current_capability_surface(
        &self,
        session_id: &str,
    ) -> Option<CapabilitySurface> {
        let sessions = self.sessions.lock().await;
        sessions
            .get(session_id)
            .and_then(|runtime| runtime.turn_state.active_turn())
            .map(|turn| CapabilitySurface {
                flow_capabilities: turn.flow_capabilities.clone(),
                mcp_servers: turn.session_frame.mcp_servers.clone(),
                vfs: turn.session_frame.vfs.clone(),
            })
    }

    /// 读取当前 active turn；若没有 active turn，则回退到 session_profile 缓存的上一轮表面。
    pub async fn get_latest_capability_surface(
        &self,
        session_id: &str,
    ) -> Option<CapabilitySurface> {
        let sessions = self.sessions.lock().await;
        let runtime = sessions.get(session_id)?;
        if let Some(turn) = runtime.turn_state.active_turn() {
            return Some(CapabilitySurface {
                flow_capabilities: turn.flow_capabilities.clone(),
                mcp_servers: turn.session_frame.mcp_servers.clone(),
                vfs: turn.session_frame.vfs.clone(),
            });
        }
        runtime
            .session_profile
            .as_ref()
            .map(|profile| CapabilitySurface {
                flow_capabilities: profile.flow_capabilities.clone(),
                mcp_servers: profile.mcp_servers.clone(),
                vfs: Some(profile.vfs.clone()),
            })
    }

    /// 替换运行中 session 的能力表面并同步 connector。
    ///
    /// Hub 层自行完成 runtime tools + MCP 工具发现，将预构建好的完整工具集传给
    /// connector。
    pub(crate) async fn replace_current_capability_surface(
        &self,
        session_id: &str,
        surface: CapabilitySurface,
    ) -> Result<(), ConnectorError> {
        let (turn_snapshot, hook_session) = {
            let sessions = self.sessions.lock().await;
            let runtime = sessions.get(session_id).ok_or_else(|| {
                ConnectorError::Runtime(format!(
                    "session `{session_id}` 当前没有运行态，无法热更新能力表面"
                ))
            })?;
            let turn = runtime.turn_state.active_turn().cloned().ok_or_else(|| {
                ConnectorError::Runtime(format!(
                    "session `{session_id}` 没有活跃 turn，无法热更新能力表面"
                ))
            })?;
            (turn, runtime.hook_session.clone())
        };

        let mut session_frame = turn_snapshot.session_frame.clone();
        session_frame.turn_id = turn_snapshot.turn_id.clone();
        session_frame.mcp_servers = surface.mcp_servers.clone();
        session_frame.vfs = surface.vfs.clone();
        let context = ExecutionContext {
            session: session_frame,
            turn: agentdash_spi::ExecutionTurnFrame {
                hook_session,
                flow_capabilities: surface.flow_capabilities.clone(),
                ..Default::default()
            },
        };
        let all_tools = self
            .build_tools_for_execution_context(session_id, &context, &surface.mcp_servers)
            .await;

        self.connector
            .update_session_tools(session_id, all_tools)
            .await?;

        let mut sessions = self.sessions.lock().await;
        if let Some(runtime) = sessions.get_mut(session_id) {
            let profile_vfs = surface
                .vfs
                .clone()
                .or_else(|| {
                    runtime
                        .turn_state
                        .active_turn()
                        .and_then(|turn| turn.session_frame.vfs.clone())
                })
                .or_else(|| {
                    runtime
                        .session_profile
                        .as_ref()
                        .map(|profile| profile.vfs.clone())
                });
            if let Some(vfs) = profile_vfs {
                runtime.session_profile = Some(super::super::hub_support::SessionProfile {
                    vfs,
                    mcp_servers: surface.mcp_servers.clone(),
                    flow_capabilities: surface.flow_capabilities.clone(),
                });
            }
            if let Some(turn) = runtime.turn_state.active_turn_mut() {
                turn.session_frame.mcp_servers = surface.mcp_servers;
                turn.session_frame.vfs = surface.vfs;
                turn.flow_capabilities = surface.flow_capabilities;
            }
        }
        Ok(())
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
        match mcp_discovery::discover_mcp_tools(&direct_servers).await {
            Ok(tools) => all_tools.extend(tools),
            Err(e) => tracing::warn!(
                session_id = %session_id,
                "直连 MCP 工具发现失败: {e}"
            ),
        }

        if let Some(relay) = &self.mcp_relay_provider {
            let tools = mcp_discovery::discover_relay_mcp_tools(relay.clone(), &relay_names).await;
            all_tools.extend(tools);
        }

        all_tools
    }
}
