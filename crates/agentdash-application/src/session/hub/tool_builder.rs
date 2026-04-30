//! Hub 的工具构建与运行时 MCP 热更职责。
//!
//! 集中：
//! - `build_tools_for_execution_context`：runtime tool + 直连 MCP + relay MCP
//!   的合并发现（由 prompt_pipeline 在 prompt 前预构建，或 replace_runtime_mcp_servers
//!   在运行中热更时重构）。
//! - `get_runtime_mcp_servers` / `replace_runtime_mcp_servers`：读/改 active session
//!   的 MCP 列表并同步到 connector。
//!
//! 注：本文件仍依赖 `agentdash_executor::mcp::discover_*`。该依赖早于 PR 6 存在
//! （application 层直接调用 executor 实现），PRD 允许"通过 tool_builder 间接依赖"，
//! 不在本 PR 改接口层级。

use std::collections::HashSet;

use agent_client_protocol::McpServer;
use agentdash_agent_types::DynAgentTool;
use agentdash_spi::{ConnectorError, ExecutionContext};

use super::SessionHub;

impl SessionHub {
    /// 读取 session 当前 turn 生效的 MCP server 列表（由 prompt pipeline 维护）。
    pub async fn get_runtime_mcp_servers(&self, session_id: &str) -> Vec<McpServer> {
        let sessions = self.sessions.lock().await;
        sessions
            .get(session_id)
            .and_then(|runtime| runtime.current_turn.as_ref())
            .map(|turn| turn.session_frame.mcp_servers.clone())
            .unwrap_or_default()
    }

    /// 替换运行中 session 的 MCP server 列表并同步 connector。
    ///
    /// Hub 层自行完成 MCP 工具发现，将预构建好的工具集传给 connector。
    pub async fn replace_runtime_mcp_servers(
        &self,
        session_id: &str,
        mcp_servers: Vec<McpServer>,
    ) -> Result<(), ConnectorError> {
        let (turn_snapshot, hook_session) = {
            let sessions = self.sessions.lock().await;
            let runtime = sessions.get(session_id).ok_or_else(|| {
                ConnectorError::Runtime(format!(
                    "session `{session_id}` 当前没有运行态，无法热更新 MCP"
                ))
            })?;
            let turn = runtime.current_turn.clone().ok_or_else(|| {
                ConnectorError::Runtime(format!(
                    "session `{session_id}` 缺少 current_turn，无法热更新 MCP"
                ))
            })?;
            (turn, runtime.hook_session.clone())
        };

        // 基于 current_turn.session_frame 重建一次性的 ExecutionContext，仅用于
        // runtime_tool_provider 扫描 mount。turn_frame 带上热更新后的 mcp_servers 与
        // hook_session，其余运行时字段保持默认（restored_session_state /
        // runtime_delegate / assembled_* 都不是 tool 构建关心的）。
        let mut session_frame = turn_snapshot.session_frame.clone();
        session_frame.turn_id = turn_snapshot.turn_id.clone();
        session_frame.mcp_servers = mcp_servers.clone();
        let context = ExecutionContext {
            session: session_frame,
            turn: agentdash_spi::ExecutionTurnFrame {
                hook_session,
                flow_capabilities: turn_snapshot.flow_capabilities.clone(),
                ..Default::default()
            },
        };
        let all_tools = self
            .build_tools_for_execution_context(
                session_id,
                &context,
                &mcp_servers,
                &turn_snapshot.relay_mcp_server_names,
            )
            .await;

        self.connector
            .update_session_tools(session_id, all_tools)
            .await?;

        let mut sessions = self.sessions.lock().await;
        if let Some(runtime) = sessions.get_mut(session_id) {
            if let Some(turn) = runtime.current_turn.as_mut() {
                turn.session_frame.mcp_servers = mcp_servers;
            }
        }
        Ok(())
    }

    pub(crate) async fn build_tools_for_execution_context(
        &self,
        session_id: &str,
        context: &ExecutionContext,
        mcp_servers: &[McpServer],
        relay_mcp_server_names: &HashSet<String>,
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

        let (_, direct_servers) = super::super::prompt_pipeline::partition_mcp_servers(
            mcp_servers,
            relay_mcp_server_names,
        );
        match mcp_discovery::discover_mcp_tools(&direct_servers).await {
            Ok(tools) => all_tools.extend(tools),
            Err(e) => tracing::warn!(
                session_id = %session_id,
                "直连 MCP 工具发现失败: {e}"
            ),
        }

        if let Some(relay) = &self.mcp_relay_provider {
            let relay_names: Vec<String> = mcp_servers
                .iter()
                .map(super::super::prompt_pipeline::extract_mcp_server_name)
                .filter(|name| relay_mcp_server_names.contains(name))
                .collect();
            let tools = mcp_discovery::discover_relay_mcp_tools(relay.clone(), &relay_names).await;
            all_tools.extend(tools);
        }

        all_tools
    }
}
