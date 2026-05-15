# Session Runtime Execution State

本 spec 定义 session 启动后的运行态边界。构建事实来自
`SessionConstructionPlan`，单次执行策略来自 `LaunchExecution`，运行态只回答
“当前这次 turn 是否被 claim、是否 active、如何取消、终态如何清理”。

## Runtime Boundaries

| 能力 | 权威组件 | 语义 |
|---|---|---|
| Session runtime map | `SessionRuntimeRegistry` | 进程内 session runtime entry 的访问入口 |
| Turn lifecycle | `TurnSupervisor` | claim / activate / cancel / cleanup / stalled scan |
| Connector live session | connector gateway | 远端或内嵌 connector 是否仍持有 live executor session |
| Active turn | `TurnState::Active(TurnExecution)` | 当前进程内是否有正在执行的 turn |

三个查询语义保持分离：

- `has_live_executor_session(session_id)`：connector 层是否持有 live executor session。
- `has_runtime_entry(session_id)`：本进程是否有 runtime entry。
- `has_active_turn(session_id)`：当前是否存在 active turn。

## Connector Projection

`ExecutionContext` 是 connector-facing projection，不是 application 层事实源。

PiAgent 等 in-process connector 直接消费 `ExecutionTurnFrame.assembled_tools`、
`runtime_delegate`、`hook_session`、`restored_session_state` 与 `context_bundle`。
MCP discovery、VFS resolution 和 capability resolution 属于 construction/launch
职责。

Relay connector 是远端执行器 transport bridge。Cloud 侧把完整 `mcp_servers`、
VFS、working directory、env、executor config、identity 与 context projection
下发给远端；relay 侧按原样透传给第三方 agent。

## Tool And Context Hot Update

Workflow phase、lifecycle hot update 或 MCP preset 变更从 active turn 读取当前
`CapabilityState` 与 `ExecutionSessionFrame` 快照，重建工具集后调用 live
connector 的 `update_session_tools`。

热更新路径只更新 runtime tools/capability projection，不构造新的 prompt，也不把
一次性 `ExecutionContext` 当成新的 session 事实源。

## Internal Follow-up

Hook auto-resume、companion parent resume 等内部 follow-up 仍从主数据流进入：

```text
LaunchCommand -> SessionConstructionPlan -> LaunchExecution
```

follow-up 来源只表达 resume intent、parent/session 引用和 source policy。owner、
VFS、MCP、capability、context、identity 由 construction 重新投影。

## Terminal Effects

`turn_terminal` event 先持久化，`SessionMeta.last_execution_status` 由事件投影更新。
终态后的业务副作用写入 terminal effect outbox，再由 dispatcher 执行。

Outbox effect 类型：

- `hook_effects`
- `session_terminal_callback`
- `hook_auto_resume`

Outbox 状态为 `pending / running / succeeded / failed / dead-letter`。effect 失败只
影响 outbox 状态，不回滚 terminal event，也不阻断 active turn cleanup。

## Persistence Store Boundaries

| Store | 职责 |
|---|---|
| `SessionMetaStore` | session meta CRUD 与投影字段合并写回 |
| `SessionEventStore` | append/read/list session events |
| `SessionTerminalEffectStore` | terminal effect outbox 写入、状态迁移和查询 |
| `SessionRuntimeCommandStore` | runtime command upsert、pending 查询、applied/failed 状态迁移 |

`SessionPersistence` 可以作为装配层组合接口存在；runtime、effects、pending 的业务逻辑
依赖对应 store 边界。

## Pending Runtime Commands

Runtime context / capability transition 的事实源是 runtime command store：

```text
pending -> applied
pending -> failed
```

下一轮 prompt 只从 command store 查询 pending commands；connector accepted 后写
applied，失败路径保留可审计状态用于恢复。
