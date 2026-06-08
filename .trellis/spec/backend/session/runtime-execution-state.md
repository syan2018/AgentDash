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

## Runtime Control Actions

Session 页面控制面由 `GET /sessions/{id}/runtime-control` 返回的 action set 表达。
`control_plane.status` 描述 RuntimeSessionExecutionAnchor、LifecycleAgent、AgentFrame 与
当前 turn 的组合状态；`actions.send_next`、`actions.steer`、`actions.cancel` 分别描述
下一轮 prompt、运行中用户 steer、运行中取消这三个命令是否可执行。

这些 action 来自 runtime meta、execution state、anchor、agent/frame 和 connector live
session 能力的联合投影，原因是 session trace、lifecycle 控制面、active turn 与 connector
live session 是不同事实源。`send_next` 只代表 idle 时可启动下一轮消息；`steer` 只代表
当前 live executor session 可以接收运行中用户输入；`cancel` 只代表当前运行可中断。前端消费
action set 后可以准确展示只读 trace、anchored idle、anchored running、terminal 和 frame
missing，而不会把“正在执行不能发下一轮”误读为“控制面缺失”。

AgentRun 的 session delivery/control command 统一挂在 runtime session 入口：

```text
POST   /sessions/{runtime_session_id}/messages
POST   /sessions/{runtime_session_id}/steering
GET    /sessions/{runtime_session_id}/pending-messages
POST   /sessions/{runtime_session_id}/pending-messages
DELETE /sessions/{runtime_session_id}/pending-messages/{message_id}
POST   /sessions/{runtime_session_id}/pending-messages/{message_id}/promote
```

这些入口表达用户正在操作当前 runtime session 的投递与控制面。handler 先解析
`RuntimeSessionExecutionAnchor`，再校验 run / agent / frame 与 Project 权限；这样 URL 层
保持 session command 语义，授权和业务归属仍落回 Lifecycle control-plane identity。

`POST /sessions/{runtime_session_id}/messages` 代表 idle session 的下一轮用户消息，继续沿
`AgentRunMessageService` 进入 session launch / prompt claim 主数据流。

`POST /sessions/{runtime_session_id}/steering` 代表 running session 的运行中用户输入，要求
connector 对该 live session 支持 steering，然后调用 `SessionControlService` 的
`steer_session`。运行中输入保持 prompt block 语义，并沿 connector / relay / executor
控制路径注入当前 turn。

`pending-messages` 队列属于同一 session command surface：空闲前可排队、删除或列出待投递输入；
`promote` 会把指定 pending message 取出并作为当前 running session 的 steering 输入投递。

### Scenario: Session-scoped AgentRun Commands

#### 1. Scope / Trigger

这些 command 是跨层 API 签名。前端只持有 runtime session id，后端必须通过
`RuntimeSessionExecutionAnchor` 回到 Lifecycle control-plane identity；因此 URL 表达
session delivery/control surface，业务校验仍落到 run / agent / frame。

#### 2. Signatures

```text
POST   /sessions/{runtime_session_id}/messages
POST   /sessions/{runtime_session_id}/steering
GET    /sessions/{runtime_session_id}/pending-messages
POST   /sessions/{runtime_session_id}/pending-messages
DELETE /sessions/{runtime_session_id}/pending-messages/{message_id}
POST   /sessions/{runtime_session_id}/pending-messages/{message_id}/promote
```

#### 3. Contracts

- `messages` request: `AgentRunMessageRequest`，包含 non-empty `input`，可带
  `executor_config`。
- `messages` response: `AgentRunMessageResponse`，返回 runtime session、turn id、run ref、agent ref、frame ref。
- `steering` request: `AgentRunSteeringRequest`，包含 non-empty `input`。
- `steering` response: `AgentRunSteeringResponse`，返回 runtime session、accepted、runtime command state。
- `pending-messages` POST request: `EnqueuePendingMessageRequest`，包含 non-empty
  `input`，可带 `executor_config`。
- `pending-messages` GET response: `PendingMessageView[]`。
- `pending-messages/{message_id}/promote` response: `{ promoted: true, turn_id }`。

#### 4. Validation & Error Matrix

| 条件 | 结果 |
| --- | --- |
| `input` 为空 | `400 BadRequest` |
| runtime session 没有 anchor | `404 NotFound` |
| anchor 指向的 run / agent 不存在 | `404 NotFound` |
| anchor agent 与 run 不一致 | `409 Conflict` |
| 当前用户无 project edit 权限 | permission error |
| `executor_config` JSON 不合法 | `400 BadRequest` |
| pending message 不存在 | `404 NotFound` |

#### 5. Good / Base / Bad Cases

- Good: idle anchored session 调用 `messages` 创建下一轮 turn，并返回 run/agent/frame refs。
- Base: running anchored session 调用 `steering`，connector 支持 steer 时注入当前 turn。
- Bad: 无 anchor 的 trace-only session 调用任一 command，返回 `404`，不会创建 lifecycle 事实。

#### 6. Tests Required

- Backend route registration 覆盖六个 session-scoped endpoint。
- Frontend service test 断言 URL 编码后的 `/sessions/{id}/...` 与 `AgentRun*` generated DTO。
- `cargo check -p agentdash-api` 保证 handler path extractor 与 response types 对齐。
- `pnpm --filter app-web test -- lifecycle` 覆盖 service 调用面。
- grep 检查产品代码和 session specs 中 session-scoped route names 与 generated DTO names 一致。

#### 7. Route Shape

```text
POST /sessions/{runtime_session_id}/steering
```

该路径保留 session delivery/control 入口语义；handler 内部解析 anchor 后再进入
Lifecycle / AgentRun 权限与状态校验。

## Terminal Effects

`turn_terminal` event 先持久化，`SessionMeta.last_delivery_status` 由事件投影更新。
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
清理 turn，并让本次 launch 返回错误；这样 delivery outbox 不会重复应用同一批
requested commands。runtime command state 的事实名是 `requested`；数据库 migration
负责保证持久化行使用该状态名。

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
