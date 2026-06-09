# Research: 工具装配链路 (build_tools_for_execution_context → assembled_tools)

- **Query**: deps.rs / preparation.rs 如何把工具放进 context.turn.assembled_tools；ctx 能拿到哪些数据
- **Scope**: internal
- **Date**: 2026-06-08

## Findings

### build_tools_for_execution_context

`crates/agentdash-application/src/session/launch/deps.rs` L171-223，`impl TurnPreparationDeps`：

```rust
pub(super) async fn build_tools_for_execution_context(
    &self,
    session_id: &str,
    context: &agentdash_spi::ExecutionContext,
    mcp_servers: &[agentdash_spi::SessionMcpServer],
) -> Vec<agentdash_agent_types::DynAgentTool>
```

聚合三路（顺序）：
1. `self.runtime_tool_provider.build_tools(context)` —— 即 RelayRuntimeToolProvider（L181-189）。
2. direct MCP：`mcp_discovery::discover_mcp_tools(&direct_servers, &context.turn.capability_state)`（L193-201）。
3. relay MCP：`discover_relay_mcp_tools(...)`（L203-219）。

`runtime_tool_provider: Option<Arc<dyn RuntimeToolProvider>>` 字段在 `TurnPreparationDeps`
(L166)，由 `SessionLaunchDeps::preparation()` 透传 (L100-111)，源头 `SessionLaunchDeps`
字段 L40 / `from_inner` L66。

### 写入 assembled_tools 的点

`crates/agentdash-application/src/session/launch/preparation.rs`：

```rust
// L74:  let capability_state = launch_plan.context.turn.capability_state.clone();
// L93:  let mut context = launch_plan.context;
// L95-101:
context.turn.assembled_tools = deps
    .build_tools_for_execution_context(
        &session_id,
        &context,
        &capability_state.tool.mcp_servers,
    )
    .await;
```

assembled_tools 随后用于：构建 SessionProfile/TurnExecution (L141-150)、
capability 持久化 frame (L169-176, `&context.turn.assembled_tools`)、
owner bootstrap 初始 frame (L224-228)。

### ExecutionContext 能拿到什么

`crates/agentdash-spi/src/connector/mod.rs`：

- `ExecutionContext { session: ExecutionSessionFrame, turn: ExecutionTurnFrame }` (L121-125)。
- `ExecutionSessionFrame` (L63-83)：`turn_id`, `working_directory`, `environment_variables`,
  `executor_config: AgentConfig`, `mcp_servers`, `vfs: Option<Vfs>`,
  `backend_execution`, `identity: Option<AuthIdentity>`。
  - **project_id**：不是直接字段；通过 `vfs.source_project_id` 或 hook_runtime snapshot 取
    （见 02 文档 `project_id_from_context`）。
  - **session/agent frame**：ExecutionContext **不直接携带 AgentFrame**；它是 frame surface
    投影后的运行态。frame 在 construction 阶段（`FrameConstructionService`，见 04/05 文档）才可见。
- `ExecutionTurnFrame` (L96-115)：`hook_runtime`, `capability_state: CapabilityState`,
  `runtime_delegate`, `restored_session_state`, `context_frames: Vec<ContextFrame>`,
  `assembled_tools: Vec<DynAgentTool>`。
  - **capability** 在此可直接拿到（`context.turn.capability_state`），provider 已据此门控。

## Caveats / Not Found

- 在 `build_tools` 阶段无法直接拿 `project_id`/`AgentFrame`；只能经 vfs 或 hook snapshot
  反推 project_id。若 workspace module 工具需要 enabled installations，要么经 repos 现查，
  要么把数据预先放入 capability_state / context（见 04/05 死字段收口讨论）。
