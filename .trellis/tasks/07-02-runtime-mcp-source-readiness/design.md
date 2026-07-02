# MCP 工具源 readiness 收束设计

## Design Summary

本任务把 MCP discovery failure 从旁路 `unavailable_mcp_servers: Vec<String>` 收束为 MCP source surface 上的一等 readiness 状态。实现目标是：每个 requested MCP source 都产出结构化 discovery outcome；tool assembly 同源产出工具、schema 和 source readiness；session launch 把 outcome 合并进唯一最终 `CapabilityState`；用户提示、模型上下文和 runtime summary 均从结构化事实派生。

为了后续方案 C，命名采用 `source/outcome/status/readiness` 语义，避免把接口锁死在 `failure` 或 `unavailable list` 上。当前仍只在 MCP 维度落地，不抽象全平台 `RuntimeToolSource`。

## Architecture Boundaries

### SPI / Capability Surface

将 `ToolDimension` 的 MCP surface 从裸 `Vec<RuntimeMcpServer>` 升级为带 readiness 的 source surface，方向如下：

```rust
pub struct RuntimeMcpServerSurface {
    pub server: RuntimeMcpServer,
    pub readiness: RuntimeMcpSourceReadiness,
}

pub enum RuntimeMcpSourceReadiness {
    Pending,
    Ready { tool_count: usize },
    Unavailable { reason_code: String, message: String },
}
```

如果现有 `RuntimeMcpServer` 被广泛用于 transport/config 边界，应保留它作为 resolved declaration，并新增 wrapper。这样 `RuntimeMcpServer` 继续表达 resolved server declaration，`RuntimeMcpServerSurface` 表达本轮 tool source observation。后续方案 C 可把 wrapper 概念迁移到通用 `RuntimeToolSourceSurface`，而无需把 transport 类型和 status 类型拆开重命名。

`CapabilityState.tool.mcp_servers` 是唯一 MCP source surface。删除：

- `ToolDimension.unavailable_mcp_servers`
- `CapabilityStateDelta.unavailable_mcp_servers`

`CapabilityStateDelta` 的 MCP delta 应从 `mcp_servers` surface 比较中派生 added / removed / changed / readiness changed。

### Discovery Interface

将 `McpToolDiscovery` 从单一 `Result<Vec<DiscoveredMcpTool>, ConnectorError>` 改为 partial outcome：

```rust
pub struct McpToolDiscoveryOutcome {
    pub tools: Vec<DiscoveredMcpTool>,
    pub sources: Vec<McpToolSourceOutcome>,
}

pub struct McpToolSourceOutcome {
    pub server: RuntimeMcpServer,
    pub readiness: RuntimeMcpSourceReadiness,
}
```

顶层 `Err` 只表达 discovery 系统级失败，例如调用前置上下文完全不可用。单个 server 的连接失败、relay 响应失败、backend offline 都进入 `sources`，不阻断其他 server。

### Direct MCP

`discover_mcp_tool_entries()` 按 server 循环：

- parse 成功并 `list_tools` 成功：产出 tools + `Ready { tool_count }`
- parse 跳过非 HTTP：产出 `Unavailable` 或保持无 outcome，需要由现有语义确认；优先对 requested server 明确产出状态
- connect/list 失败：产出 `Unavailable { reason_code, message }`，继续下一个 server

错误 message 保持有界、脱敏、可读；reason_code 使用稳定短码，如 `direct_connect_failed` / `direct_list_tools_failed`。

### Relay MCP

`McpRelayProvider::list_relay_tools()` 需要返回 relay outcome，而不是只返回 tools：

```rust
pub struct RelayMcpListOutcome {
    pub tools: Vec<RelayMcpToolInfo>,
    pub sources: Vec<RelayMcpSourceOutcome>,
}
```

每个 requested server 必须产出一个 source outcome。映射建议：

| 条件 | readiness |
| --- | --- |
| resolve backend anchor 失败 | `Unavailable { reason_code: "relay_backend_unresolved" }` |
| backend offline | `Unavailable { reason_code: "relay_backend_offline" }` |
| relay command timeout / send error | `Unavailable { reason_code: "relay_transport_failed" }` |
| response carries error | `Unavailable { reason_code: "mcp_list_tools_failed" }` |
| unexpected response | `Unavailable { reason_code: "relay_unexpected_response" }` |
| response success | `Ready { tool_count }` |

`ExecutorMcpToolDiscovery` 合并 direct outcome 和 relay outcome，形成统一 `McpToolDiscoveryOutcome`。

### Tool Assembly

`AssembledToolSurface` 改为同源携带工具、schema、source outcomes：

```rust
pub(crate) struct AssembledToolSurface {
    pub tools: Vec<DynAgentTool>,
    pub schemas: Vec<RuntimeToolSchemaEntry>,
    pub mcp_sources: Vec<McpToolSourceOutcome>,
}
```

不再使用 `mcp_failures`。调用方不需要根据 `Err` 猜测失败 source，也不需要自由文本 failure list。

### Session Launch

`TurnPreparer` 的关键顺序：

1. 取得 `mut context = launch_plan.context`
2. 用当前 context 调用 `assemble_tool_surface`
3. 将 `assembled_tool_surface.mcp_sources` 合并进 `context.turn.capability_state.tool.mcp_servers`
4. 从 `context.turn.capability_state` 派生唯一 `capability_state` 与 `capability_keys`
5. 后续 `turn_supervisor.activate_turn`、runtime transition、initial capability frame、accepted launch commit、connector context 全部使用这份最终 state

这样避免旧拷贝 bug，也满足 `CapabilityState.tool.mcp_servers` 与 launch surface 同源观察。若 `FrameLaunchEnvelope.launch_surface.mcp_servers` 仍是裸 `RuntimeMcpServer`，则 `FrameLaunchSurface` 需要同步升级为 MCP source surface；如果为了维持 declaration/surface 分层，可在 frame surface 中引入 `mcp_server_sources`，并确保 normalization gate 比较的是同一结构。

### User-visible Notice And Backbone

MCP readiness notice 不在 prepare 阶段直接 `persist_notification`。`PreparedTurn` 应携带结构化 startup notice，`TurnCommitter::commit()` 在 user input / turn_started 之后提交。

优先方案：

- 新增或复用受控 platform event key，例如 `mcp_source_readiness`，payload 携带 `sources[]`。
- 前端系统事件卡片按结构化 payload 渲染。
- context frame 仍作为模型可见事实，渲染 unavailable source 与 reason。

若新增一等 `PlatformEvent` variant，需要重新生成 `backbone-protocol.ts`。若使用 `SessionMetaUpdate`，payload 也必须结构化，不只塞自由文本 message。

### Runtime Health Propagation

`McpClientManager` 已在本机标记 ready/unavailable。需要新增 health change 通知路径：

- MCP manager 在 health map 变化时通知 local relay writer，或 local relay 定期/节流比较 snapshot。
- 发送 `RelayMessage::EventCapabilitiesChanged { payload: build_capabilities(...) }`
- API 侧现有 [ws_handler.rs](D:/ABCTools_Dev/AgentDashboard/crates/agentdash-api/src/relay/ws_handler.rs:439) 更新 backend registry 和 runtime health。

节流策略建议：同一 event loop 内 debounce，避免频繁 `call_tool` 失败造成 capabilities_changed 风暴。

## Data Flow

```text
FrameLaunchEnvelope / CapabilityState MCP source declarations
  -> TurnPreparer
  -> assemble_tool_surface_for_execution_context
  -> McpToolDiscoveryOutcome { tools, sources }
  -> merge sources into final CapabilityState.tool.mcp_servers
  -> build RuntimeToolSchemaEntry + context frames from final state
  -> connector receives ExecutionContext with final capability state
  -> TurnCommitter commits user input, turn_started, readiness notice, context frames
```

Runtime summary flow:

```text
local McpClientManager list/call result
  -> health snapshot changes
  -> local relay EventCapabilitiesChanged
  -> cloud BackendRegistry + runtime_health capabilities update
  -> /backends/runtime-summary capability_health
  -> LocalRuntimeView / runtime diagnostics
```

## Trade-offs

- Wrapper `RuntimeMcpServerSurface` is preferred over embedding readiness directly in `RuntimeMcpServer`, because declaration/transport and runtime observation have different lifecycles.
- This task deliberately keeps `CapabilityHealthItem` separate from `CapabilityState` readiness. `CapabilityHealthItem` is runtime/backend diagnostics projection; MCP source readiness is session/model-visible source surface. They should map from common facts where possible, but one should not replace the other.
- Using partial outcome increases interface size but reduces caller ordering constraints and makes tests target the correct seam.

## C Migration Reserve

Names should leave room for this future shape:

```rust
pub struct RuntimeToolSourceSurface {
    pub kind: RuntimeToolSourceKind,
    pub id: String,
    pub label: String,
    pub status: RuntimeToolSourceStatus,
    pub metadata: RuntimeToolSourceMetadata,
}
```

Current MCP implementation should therefore avoid names like `McpDiscoveryFailures` or `UnavailableMcpServers`. Prefer `McpToolSourceOutcome`, `RuntimeMcpSourceReadiness`, `source_status`, `reason_code`.

Future migration path:

1. Keep `RuntimeMcpServerSurface` as MCP-specific adapter.
2. Add `RuntimeToolSourceSurface` alongside it.
3. Implement `From<RuntimeMcpServerSurface> for RuntimeToolSourceSurface`.
4. Move context frame and UI rendering to the generic source surface.
5. Collapse MCP-specific wrapper only after executor/workspace module/builtin sources also adopt generic status.

## Rollback Considerations

Because this is a pre-release project, no compatibility fallback is required. Rollback means reverting the task commit. Keep changes concentrated around SPI source surface, discovery outcome, session launch, relay provider, local capabilities update, and tests.
