//! Hub 的工具构建与运行时 MCP 热更职责。
//!
//! 集中：
//! - `build_tools_for_execution_context`：runtime tool + 直连 MCP + relay MCP
//!   的合并发现（由 prompt_pipeline 在 prompt 前预构建，或 replace_current_capability_state
//!   在运行中热更时重构）。
//! - `get_runtime_mcp_servers` / `get_current_capability_state`：读取当前能力状态。
//! - `replace_current_capability_state`：底层热更 primitive，仅供
//!   `runtime_context_transition` 统一 applier 调用。
//!
//! 注：本文件仍依赖 `agentdash_executor::mcp::discover_*`。该依赖早于 PR 6 存在
//! （application 层直接调用 executor 实现），PRD 允许"通过 tool_builder 间接依赖"，
//! 不在本 PR 改接口层级。

use agentdash_agent_types::DynAgentTool;
use agentdash_spi::{ConnectorError, ExecutionContext, SessionMcpServer};

use super::SessionHub;
use crate::session::types::CapabilityState;

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

    /// 读取 session 当前 turn 生效的能力状态。
    ///
    /// `TurnExecution.capability_state` 在 pipeline 组装时已包含完整的 MCP/VFS 维度，
    /// 无需从 session_frame 手动拼合。
    pub async fn get_current_capability_state(&self, session_id: &str) -> Option<CapabilityState> {
        let sessions = self.sessions.lock().await;
        sessions
            .get(session_id)
            .and_then(|runtime| runtime.turn_state.active_turn())
            .map(|turn| turn.capability_state.clone())
    }

    /// 读取当前 active turn；若没有 active turn，则回退到 session_profile 缓存。
    ///
    /// `SessionProfile.capability_state` 已包含完整维度，直读即可。
    pub async fn get_latest_capability_state(&self, session_id: &str) -> Option<CapabilityState> {
        let sessions = self.sessions.lock().await;
        let runtime = sessions.get(session_id)?;
        if let Some(turn) = runtime.turn_state.active_turn() {
            return Some(turn.capability_state.clone());
        }
        runtime
            .session_profile
            .as_ref()
            .map(|profile| profile.capability_state.clone())
    }

    /// 替换运行中 session 的能力状态并同步 connector。
    ///
    /// Hub 层自行完成 runtime tools + MCP 工具发现，将预构建好的完整工具集传给
    /// connector。
    pub(crate) async fn replace_current_capability_state(
        &self,
        session_id: &str,
        mut state: CapabilityState,
    ) -> Result<Vec<DynAgentTool>, ConnectorError> {
        let (turn_snapshot, hook_session) = {
            let sessions = self.sessions.lock().await;
            let runtime = sessions.get(session_id).ok_or_else(|| {
                ConnectorError::Runtime(format!(
                    "session `{session_id}` 当前没有运行态，无法热更新能力状态"
                ))
            })?;
            let turn = runtime.turn_state.active_turn().cloned().ok_or_else(|| {
                ConnectorError::Runtime(format!(
                    "session `{session_id}` 没有活跃 turn，无法热更新能力状态"
                ))
            })?;
            (turn, runtime.hook_session.clone())
        };

        let mut session_frame = turn_snapshot.session_frame.clone();
        session_frame.turn_id = turn_snapshot.turn_id.clone();
        session_frame.mcp_servers = state.tool.mcp_servers.clone();
        session_frame.vfs = state.vfs.active.clone();
        let context = ExecutionContext {
            session: session_frame,
            turn: agentdash_spi::ExecutionTurnFrame {
                hook_session,
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

        let mut sessions = self.sessions.lock().await;
        if let Some(runtime) = sessions.get_mut(session_id) {
            // 若 state.vfs.active 为 None，从当前 turn 或 profile 回填
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
            let tools = mcp_discovery::discover_relay_mcp_tools(
                relay.clone(),
                &relay_names,
                &context.turn.capability_state,
            )
            .await;
            all_tools.extend(tools);
        }

        all_tools
    }
}
