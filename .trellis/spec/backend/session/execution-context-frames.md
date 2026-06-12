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
LaunchPlan -> TurnPreparer -> PreparedTurn.connector_context -> ConnectorStarter
```

其它路径可以 clone/read active turn 的 frame 用于工具热更新，但不把该 projection 写回为
session 架构事实源。

## `ExecutionSessionFrame` — Who + Where

```rust
pub struct ExecutionSessionFrame {
    pub turn_id: String,
    pub working_directory: PathBuf,
    pub environment_variables: HashMap<String, String>,
    pub executor_config: AgentConfig,
    pub mcp_servers: Vec<RuntimeMcpServerDeclaration>,
    pub vfs: Option<Vfs>,
    pub identity: Option<AuthIdentity>,
}
```

| 字段 | 来源 | 消费者 |
|---|---|---|
| `turn_id` | Launch preparation claim/activation | connector trace、hook 审计 |
| `working_directory` | `FrameLaunchEnvelope.working_directory` | Relay、vibe_kanban、PiAgent tools |
| `environment_variables` | launch prompt payload / executor policy | Relay、vibe_kanban |
| `executor_config` | `FrameLaunchEnvelope` execution profile + launch override | 所有 connector |
| `mcp_servers` | capability projection | Relay 透传；PiAgent 通过 assembled tools 消费 |
| `vfs` | `FrameLaunchEnvelope` VFS projection | Relay、vibe_kanban、PiAgent tools |
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
| `capability_state` | `FrameLaunchEnvelope` capability projection + runtime command apply result | runtime tools、MCP/VFS diff |
| `runtime_delegate` | launch hook plan | agent loop hook callbacks |
| `restored_session_state` | restore plan | 支持 repository restore 的 connector |
| `context_frames` | `FrameLaunchEnvelope` context projection | connector context 消费（按 kind 分类或渲染为文本） |
| `assembled_tools` | runtime tool provider + MCP tools projection | in-process connector tool execution |

`TurnExecution` 在 active turn 内保存 session frame、capability state、context bundle、
cancel flag 与 processor channel。它是 per-turn runtime 快照，不承担 owner/context/VFS
解析。

## Connector Consumption Matrix

| Connector | SessionFrame | TurnFrame |
|---|---|---|
| PiAgent | `turn_id`、`executor_config` | `assembled_tools`、`runtime_delegate`、`hook_session`、`restored_session_state`、`context_frames` |
| Relay | `mcp_servers`、`vfs`、`working_directory`、`environment_variables`、`executor_config`、`identity` | `context_frames`（渲染为系统上下文） |
| vibe_kanban | `vfs`、`working_directory`、`environment_variables`、`executor_config` | `context_frames`（渲染为系统上下文） |

动态 Project Context、Workspace、Skills、Hook Runtime 等内容通过 ContextFrame
进入，不作为 running turn 的 system prompt 重置。

## Tool Hot Update

MCP 或 capability 热更新流程：

```text
active TurnExecution
  -> clone ExecutionSessionFrame + CapabilityState
  -> build tools
  -> connector.update_session_tools(session_id, tools)
  -> update active turn projection
```

该流程只服务 live connector 的工具集替换；下一轮 prompt 仍通过
`LaunchCommand -> FrameLaunchEnvelope -> LaunchPlan -> PreparedTurn` 重新投影完整
`ExecutionContext`。

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
