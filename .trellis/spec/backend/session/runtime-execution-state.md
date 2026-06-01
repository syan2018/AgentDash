# Session Runtime Execution State

本 spec 定义 session 启动后的运行态边界。构建事实来自
`SessionConstructionPlan`，单次启动决策来自 `LaunchPlan`，运行态只回答
“当前这次 turn 是否被 claim、是否 active、如何取消、终态如何清理”。

## Runtime Boundaries

| 能力 | 权威组件 | 语义 |
|---|---|---|
| Session runtime map | `SessionRuntimeRegistry` | 进程内 session runtime entry 的访问入口 |
| Turn lifecycle | `TurnSupervisor` | claim / activate / cancel / cleanup / stalled scan |
| Connector live session | connector gateway | 远端或内嵌 connector 是否仍持有 live executor session |
| Active turn | `TurnState::Active(TurnExecution)` | 当前进程内是否有正在执行的 turn |
| Backend execution lease | `BackendExecutionLeaseRepository` | relay turn 对本机 backend 的执行占用与释放事实 |

三个查询语义保持分离：

- `has_live_executor_session(session_id)`：connector 层是否持有 live executor session。
- `has_runtime_entry(session_id)`：本进程是否有 runtime entry。
- `has_active_turn(session_id)`：当前是否存在 active turn。
- `backend_execution_leases` active rows：本机 backend 是否被 relay turn 占用，用于 backend placement / runtime summary，不替代 session active turn。

## Connector Projection

`ExecutionContext` 是 connector-facing projection，不是 application 层事实源。

PiAgent 等 in-process connector 直接消费 `ExecutionTurnFrame.assembled_tools`、
`runtime_delegate`、`hook_session`、`restored_session_state` 与 `context_bundle`。
MCP discovery、VFS resolution 和 capability resolution 属于 construction/launch
职责。

Relay connector 是远端执行器 transport bridge。Cloud 侧把完整 `mcp_servers`、
VFS、working directory、env、executor config、identity、context projection 与
已解析的 backend execution placement 下发给远端；relay 侧按原样透传给第三方 agent。

Session launch 在 construction 完成后解析 `BackendSelectionRequest`，claim backend
execution lease，并把 `backend_id + lease_id + selection_mode` 投影到
`ExecutionContext.session.backend_execution`。Relay connector 只消费该 placement；prompt
accepted 后 activate，terminal/cancel/prompt failure 后 release 或 fail。lease 只描述
backend 占用，不替代 session event terminal 持久化。

## Tool And Context Hot Update

Workflow phase、lifecycle hot update 或 MCP preset 变更从 active turn 读取当前
`CapabilityState` 与 `ExecutionSessionFrame` 快照，重建工具集后调用 live
connector 的 `update_session_tools`。

热更新路径只更新 runtime tools/capability projection，不构造新的 prompt，也不把
一次性 `ExecutionContext` 当成新的 session 事实源。

## Turn Background Task Supervision

`TurnSupervisor` 是 active turn 后台任务的唯一监督入口。`StreamIngestionAttacher`
创建 processor / stream adapter，并在 adapter task spawn 后把 abort handle 登记到当前
`TurnExecution`：

```rust
let attached = stream_ingestion.attach(committed_turn).await;
```

`clear_active_turn` 与 `clear_turn_and_hook` 在释放 active turn 前必须中止已登记的
stream adapter。正常 stream 已结束时 abort 是幂等操作；取消、connector 错误或 hook
runtime 清理路径则依赖该行为避免 adapter 在 terminal 后继续读取 stream。

terminal event 持久化失败也必须释放 active turn。正确顺序是：

```text
persist terminal event -> clear_active_turn -> if persist failed: stop effects and return
```

不能把 `clear_active_turn` 放在 terminal persist 的 success 分支里；否则 event store
短暂故障会让 session 永久停在 running。

测试要求：

- 登记后，active `TurnExecution.stream_adapter_abort` 必须为 `Some`。
- `clear_active_turn` 必须中止 pending adapter task。
- `clear_turn_and_hook` 必须中止 pending adapter task，并清空 hook runtime。
- terminal event 持久化失败时，`has_active_turn(session_id)` 必须变为 `false`。

## Internal Follow-up

Hook auto-resume、companion parent resume 等内部 follow-up 仍从主数据流进入：

```text
LaunchCommand -> SessionConstructionPlan -> LaunchPlan
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
| `SessionRuntimeCommandStore` | runtime delivery command request upsert、requested 查询、applied/failed 状态迁移 |

`SessionPersistence` 可以作为装配层组合接口存在；runtime、effects、pending 的业务逻辑
依赖对应 store 边界。

## Pending Runtime Commands

Runtime context / capability transition 的事实源是 `AgentFrameTransitionRecord` / `agent_frame_transitions`；runtime command store 是 delivery outbox：

```text
requested -> applied
requested -> failed
```

下一轮 prompt 从 delivery outbox 查询 requested commands，并通过关联的 frame transition records 还原 transition apply plan；connector accepted 后写
applied。若 applied 状态提交失败，必须立刻尝试把同一批 command 标记为 `failed`，
清理 turn，并让本次 launch 返回错误；不能继续启动 processor，也不能保留 `requested`
等待下一轮静默重复应用。旧 `pending` 状态不再作为 runtime command 事实名使用；
数据库迁移会把既有 runtime command 行更新为 `requested`。

`RuntimeDeliveryCommand` payload 保存 delivery kind、`frame_transition_id` 与 target
frame reference；`AgentFrameTransitionRecord` 保存 `RuntimeCapabilityTransition`
records。payload 不保存完整 `CapabilityState`，也不保存 `ToolDimension` /
`CompanionDimension` replacement；tool、MCP、companion、VFS 与 mount directive 分别作为
dimension effect records replay 到 construction base projection，再由 capability
projection normalizer 生成闭包状态。多个 requested runtime command 必须按 store 返回顺序
fold replay。

runtime transition 的生产入口由各 dimension module 生成 records，并在写入 store 前调用
`CapabilityDimensionRegistry::validate_transition`。delivery outbox 写入时必须校验
delivery 的 `frame_transition_id` / `target_frame_id` 与 frame transition fact 一致。mount
directive 同时保留为 `dimension=vfs / declaration_type=mount_operation` declaration 与
`apply_mount_operations` effect，使审计来源与可 replay effect 分离但保持同源。
