# Execution Context Frames

`agentdash-spi::ExecutionContext` 是 connector 边界的投影。Application 层事实来自
`AgentFrame`、`FrameLaunchEnvelope` 和 `LaunchPlan`；connector 只接收本次 prompt 所需的
`ExecutionSessionFrame` 与 `ExecutionTurnFrame`。

## Top-level Shape

定义位置：`crates/agentdash-spi/src/connector.rs`。

```rust
pub struct ExecutionContext {
    pub session: ExecutionSessionFrame,
    pub turn: ExecutionTurnFrame,
}
```

生产构造路径：

```text
LaunchCommand(+LaunchModifier) -> FrameLaunchEnvelope -> LaunchPlan -> TurnPreparer -> PreparedTurn.connector_context -> ConnectorStarter
```

其它路径可以 clone/read active turn 的 frame 用于工具热更新，但不把该 projection 写回为
session 架构事实源。

`LaunchModifier` 在 frame construction / launch planning 阶段被消化为 owner surface、source
policy、prompt plan 或 follow-up plan；它不进入 connector-facing `ExecutionContext`。connector
只消费本次 prompt 的闭包事实，原因是 connector 不应理解 ProjectAgent、Companion、Routine、
Local relay 等 application 来源差异。

## `ExecutionSessionFrame` — Who + Where

```rust
pub struct ExecutionSessionFrame {
    pub turn_id: String,
    pub working_directory: PathBuf,
    pub environment_variables: HashMap<String, String>,
    pub executor_config: AgentConfig,
    pub mcp_servers: Vec<RuntimeMcpServer>,
    pub vfs: Option<Vfs>,
    pub identity: Option<AuthIdentity>,
}
```

| 字段 | 来源 | 消费者 |
|---|---|---|
| `turn_id` | Launch preparation claim/activation | connector trace、hook 审计 |
| `working_directory` | `FrameLaunchEnvelope.working_directory` | Relay、vibe_kanban、PiAgent tools |
| `environment_variables` | launch prompt payload / executor policy | Relay、vibe_kanban |
| `executor_config` | `FrameLaunchEnvelope.launch_surface.execution_profile` | 所有 connector |
| `mcp_servers` | `FrameLaunchEnvelope.launch_surface.mcp_servers` | Relay 透传；PiAgent 通过 assembled tools 消费 |
| `vfs` | `FrameLaunchEnvelope.launch_surface.vfs` | Relay、vibe_kanban、PiAgent tools |
| `identity` | `FrameLaunchEnvelope` identity projection | Relay、审计、permission 决策 |

一次 `connector.prompt(...)` 调用期间，session frame 不变；下一 turn 需要新的投影时由
launch stages 重新生成。

## `ExecutionTurnFrame` — What + How

```rust
pub struct ExecutionTurnFrame {
    pub hook_runtime: Option<Arc<dyn HookRuntimeAccess>>,
    pub capability_state: CapabilityState,
    pub runtime_delegate: Option<DynAgentRuntimeDelegate>,
    pub restored_session_state: Option<RestoredSessionState>,
    pub context_frames: Vec<ContextFrame>,
    pub assembled_tools: Vec<DynAgentTool>,
}
```

| 字段 | 来源 | 消费者 |
|---|---|---|
| `hook_session` | session runtime shared hook handle | hook trace、runtime injection、capability 追踪 |
| `capability_state` | `FrameLaunchEnvelope.launch_surface.capability_state` + runtime command apply result | runtime tools、MCP/VFS diff |
| `runtime_delegate` | launch hook plan | agent loop hook callbacks |
| `restored_session_state` | restore plan | 支持 repository restore 的 connector |
| `context_frames` | `FrameLaunchEnvelope` context projection | connector context 消费（按 kind 分类或渲染为文本） |
| `assembled_tools` | runtime tool provider + MCP tools projection | in-process connector tool execution |

`TurnExecution` 在 active turn 内保存 session frame、capability state、context bundle、
cancel flag 与 processor channel。它是 per-turn runtime 快照，不承担 owner/context/VFS
解析。

ToolSchema PromptText 不进入 `ExecutionTurnFrame`。Turn preparation 在 application 层装配
`AssembledToolSurface`：`tools` 投影到 `ExecutionTurnFrame.assembled_tools` 服务 connector
执行；`schemas` 留在 application 的 runtime context frame producer 中生成
`tool_schema_delta` 和 `ContextFrame.rendered_text`。这样 provider bridge 的机器工具表与
平台掌握的 Agent 可见能力说明同源，但职责边界保持分离。

## Connector Consumption Matrix

| Connector | SessionFrame | TurnFrame |
|---|---|---|
| PiAgent | `turn_id`、`executor_config` | `assembled_tools`、`runtime_delegate`、`hook_session`、`restored_session_state`、`context_frames` |
| Relay | `mcp_servers`、`vfs`、`working_directory`、`environment_variables`、`executor_config`、`identity` | `context_frames`（渲染为系统上下文） |
| vibe_kanban | `vfs`、`working_directory`、`environment_variables`、`executor_config` | `context_frames`（渲染为系统上下文） |

动态 Project Context、Workspace、Skills、Memory、Hook Runtime 等内容通过 ContextFrame
进入，不作为 running turn 的 system prompt 重置。

## Memory Context Frame

`memory_context` 是 connector-facing system ContextFrame，来源是 `LaunchPlan.discovered_memory`。它向模型提供 runtime-discovered memory source inventory、默认 source/index、policy 文本和 bounded index 内容。

Contract：

| 字段 | 值 |
| --- | --- |
| `kind` | `memory_context` |
| `source` | `RuntimeContextUpdate` |
| `delivery_channel` | `connector_context` |
| `message_role` | `system` |
| section | `SystemNotice { title: "Memory Context", body: rendered_text }` |

`rendered_text` 只包含 source inventory、diagnostics、默认 source/index 和 `index_status=present` 的 bounded index 内容。Topic 文件正文需要 Agent 按索引线索通过 VFS 工具读取，原因是 topic body 属于按需资源内容，不应在每轮启动时无界进入 stable system context。

Frame order for PiAgent stable system prompt：

```text
identity -> system_guidelines -> memory_context
```

Validation / tests：

| 条件 | 断言 |
| --- | --- |
| `MemoryDiscoveryOutput` 为空 | 不生成 `memory_context` frame |
| source 有 bounded `agent://MEMORY.md` | rendered text 包含默认 source/index 与 index markdown |
| source `index_status=too_large` | rendered text 只展示状态和 diagnostic，不包含正文 |
| connector 收到无序 context frames | system prompt 仍按 identity、guidelines、memory_context 顺序拼接 |

## Tool Hot Update

MCP 或 capability 热更新流程：

```text
active TurnExecution
  -> clone ExecutionSessionFrame + CapabilityState
  -> persist AgentFrame revision for the updated surface
  -> build AssembledToolSurface
  -> connector.update_session_tools(session_id, surface.tools)
  -> runtime context frame consumes surface.schemas
  -> update active turn projection
```

该流程只服务 live connector 的工具集替换；active turn 的 `ExecutionSessionFrame.mcp_servers`
是当前 frame surface 的执行快照，工具发现从该快照读取 `RuntimeMcpServer`，并用
`CapabilityState.tool_policy` 做工具级裁决。下一轮 prompt 仍通过
`LaunchCommand -> FrameLaunchEnvelope -> LaunchPlan -> PreparedTurn` 重新投影完整
`ExecutionContext`。

运行中采用已持久化 AgentFrame revision 时也必须使用同一份 `AssembledToolSurface`。connector
只接收 `DynAgentTool` replace-set；ContextFrame / turn-start notice / transform_context 使用
`RuntimeToolSchemaEntry` 渲染工具说明。PiAgent connector 不负责 ToolSchema 文本格式化，原因是
平台需要在 application 层审计并掌握 Agent 实际收到的能力上下文。

## PiAgent Bundle Handling

PiAgent 使用 `context.turn.context_bundle.bundle_id` 判断是否需要刷新 stable system
prompt：

- 首次创建 agent：写入 rendered system prompt 与工具集。
- bundle id 变化：刷新 stable system prompt。
- bundle id 相同：复用现有 agent/runtime。

运行期动态内容走 steering、follow-up、pending action 或 session notification。

## Related Specs

- [`session-startup-pipeline.md`](./session-startup-pipeline.md)
- [`bundle-main-datasource.md`](./bundle-main-datasource.md)
- [`runtime-execution-state.md`](./runtime-execution-state.md)
- [`pi-agent-streaming.md`](./pi-agent-streaming.md)
