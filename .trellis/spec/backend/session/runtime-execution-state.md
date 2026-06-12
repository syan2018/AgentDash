# Session Runtime Execution State

本 spec 定义 session 启动后的运行态边界。构建事实来自
`FrameLaunchEnvelope`，单次启动决策来自 `LaunchPlan`，运行态只回答
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
MCP discovery、VFS resolution 和 capability resolution 属于 frame construction / launch
职责。

Relay connector 是远端执行器 transport bridge。Cloud 侧把完整 `mcp_servers`、
VFS、working directory、env、executor config、identity、context projection 与
已解析的 backend execution placement 下发给远端；relay 侧按原样透传给第三方 agent。

Session launch 在 `FrameLaunchEnvelope` 完成后解析 `BackendSelectionRequest`，claim backend
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
LaunchCommand -> FrameLaunchEnvelope -> LaunchPlan
```

follow-up 来源只表达 resume intent、parent/session 引用和 source policy。VFS、MCP、
capability、context、identity 由 frame construction 重新投影。

## RuntimeSession Trace Metadata

`SessionMeta` 是 RuntimeSession repository 内部保存的 trace-head metadata。浏览器合同以
`RuntimeSessionTraceMeta` 暴露这组 trace facts，记录当前 runtime trace 的事件游标、
connector continuation、trace 标题来源、delivery 摘要和终态摘要，服务于 trace/feed/debug
展示、runtime repository rehydrate、connector follow-up、branch/rollback projection 以及
进程重启后的 execution state recovery projection。

RuntimeSession trace metadata 的职责边界：

| 字段 / 概念 | RuntimeSession 语义 |
| --- | --- |
| `runtime_session_id` | runtime trace identity；通过 `RuntimeSessionExecutionAnchor` 反查 Lifecycle control-plane identity |
| `last_event_seq` | session event log head 与 projection checkpoint |
| `executor_session_id` | connector-native follow-up / restore handle |
| trace title / `title_source` | trace/feed/debug 的标题 provenance，来源优先级仍由 session title policy 管控 |
| `last_delivery_status` | 从 runtime session events 投影出的 delivery 摘要，用于 trace 状态与 recovery |
| `last_turn_id` | trace-head 当前或最近 turn 指针，用于 recovery、feed 定位和 runtime control 聚合 |
| `last_terminal_message` | terminal trace summary，用于诊断与只读 trace 展示 |

AgentRun Workspace 的 public shell 由 `AgentRunWorkspaceShell` / `AgentRunWorkspaceView`
表达：display title、title source、list entry、workspace status、last activity、last visible
turn，以及 command action availability。该 projection 可以引用 runtime trace ref 或 delivery
trace metadata，但它的事实来源是 ProjectAgent / Subject association / LifecycleAgent /
AgentFrame / active turn / command receipt 等 AgentRun 控制面事实。

用户 command receipt 属于 AgentRun command projection。client command identity、request
digest、duplicate/conflict 判定、accepted result 和 command-scoped terminal result 都按
command scope 记录，并以 run / agent / frame / runtime session / turn refs 表达已接受结果。
这些事实描述的是一次用户命令的幂等与回放边界，而 `SessionMeta` 描述的是 RuntimeSession
trace head；分层后，trace 恢复、事件流展示、workspace action enablement 和 command retry
可以各自消费对应 projection。

## AgentRun Workspace Control Actions

用户可见执行工作台的 shell 与 action set 由 `AgentRunWorkspaceView` 表达。
`AgentRunWorkspaceShell` 承载 display title、title source、workspace/list status、last activity
和 last visible turn；`AgentRunWorkspaceControlPlaneView` 与
`AgentRunWorkspaceActionSetView` 承载工作台可执行状态。`actions.send_next`、
`actions.enqueue`、`actions.steer`、`actions.cancel` 分别描述下一轮 prompt、运行中排队、
运行中用户 steer、运行中取消这些 AgentRun command 是否可执行。

这些 action 来自 LifecycleAgent、AgentFrame、active turn、command receipt、delivery summary
和 connector live session 能力的联合投影，原因是 lifecycle 控制面、frame runtime、当前 turn、
用户命令回执与 connector live session 是不同事实源。`RuntimeSessionTraceMeta` 可以作为
`AgentRunWorkspaceView.delivery_trace_meta` 被引用，用于展示 trace ref、event seq、executor
continuation 和 terminal summary；它不决定 workspace title、list entry、status 或按钮 enablement。
`delivery_runtime_ref` 仍可出现在 workspace view 中，原因是 AgentRun command 需要一个实际
delivery runtime 通道完成投递，而用户也需要能从工作台下钻到 runtime trace/detail。

`SessionRuntimeControlPlaneView`、`SessionRuntimeActionSetView` 与
`SessionRuntimeControlView` 保留给 `/sessions/{id}/runtime-control` 和 RuntimeSession detail
入口。该入口从 runtime trace identity 出发，经 `RuntimeSessionExecutionAnchor` 反查 run /
agent / frame；AgentRun workspace 入口则从 run / agent identity 出发，返回
`AgentRunWorkspaceControlPlaneView` / `AgentRunWorkspaceActionSetView`，原因是用户侧工作台的
主模型应表达 AgentRun command/control，而不是 runtime trace control。

AgentRun delivery/control command 使用 AgentRun Workspace public identity：

```text
GET    /agent-runs/{run_id}/agents/{agent_id}/workspace
POST   /agent-runs/{run_id}/agents/{agent_id}/messages
POST   /agent-runs/{run_id}/agents/{agent_id}/steering
GET    /agent-runs/{run_id}/agents/{agent_id}/pending-messages
POST   /agent-runs/{run_id}/agents/{agent_id}/pending-messages
DELETE /agent-runs/{run_id}/agents/{agent_id}/pending-messages/{message_id}
POST   /agent-runs/{run_id}/agents/{agent_id}/pending-messages/{message_id}/promote
```

这些入口表达用户正在操作某个 AgentRun workspace。handler 以 Project 权限、run / agent
ownership、当前 AgentFrame、active turn 和 command receipt 为校验事实源，并在 accepted result
中返回 runtime session ref、turn ref、frame ref 等 delivery refs。RuntimeSession trace endpoint
使用 `RuntimeSessionTraceMeta` 提供只读 trace/feed/debug 能力；follow-up 与 repository rehydrate
仍通过 trace metadata 保存的 `executor_session_id` 与 `last_event_seq` 衔接。

`POST /agent-runs/{run_id}/agents/{agent_id}/messages` 代表 workspace idle 时的下一轮用户消息，
沿 `AgentRunMessageService` 进入 launch / prompt claim 主数据流。

`POST /agent-runs/{run_id}/agents/{agent_id}/steering` 代表 workspace running 时的运行中用户输入，
要求当前 active turn 的 connector live session 支持 steering，然后调用 `SessionControlService`
的 `steer_session` 注入当前 turn。运行中输入保持 prompt block 语义，并沿 connector / relay /
executor 控制路径投递。

`pending-messages` 队列属于同一 AgentRun command surface：空闲前可排队、删除或列出待投递输入；
`promote` 会把指定 pending message 取出并作为当前 running workspace 的 steering 输入投递。

### Scenario: AgentRun Workspace Commands

#### 1. Scope / Trigger

这些 command 是跨层 API 签名。浏览器持有 AgentRun workspace identity，后端以 run / agent /
frame / active turn / command receipt 聚合当前 delivery control state，并把 RuntimeSession 作为
accepted delivery ref 或 trace ref 返回。

#### 2. Signatures

```text
GET    /agent-runs/{run_id}/agents/{agent_id}/workspace
POST   /agent-runs/{run_id}/agents/{agent_id}/messages
POST   /agent-runs/{run_id}/agents/{agent_id}/steering
GET    /agent-runs/{run_id}/agents/{agent_id}/pending-messages
POST   /agent-runs/{run_id}/agents/{agent_id}/pending-messages
DELETE /agent-runs/{run_id}/agents/{agent_id}/pending-messages/{message_id}
POST   /agent-runs/{run_id}/agents/{agent_id}/pending-messages/{message_id}/promote
```

#### 3. Contracts

- `workspace` response: `AgentRunWorkspaceView`，包含 `shell`、`actions`、frame/runtime refs
  和可选 `delivery_trace_meta: RuntimeSessionTraceMeta`。
- `workspace.control_plane` 使用 `AgentRunWorkspaceControlPlaneView`，status 表达
  `ready | running | terminal | frame_missing | delivery_missing`。
- `workspace.actions` 使用 `AgentRunWorkspaceActionSetView`，action availability 表达
  AgentRun workspace command 的可执行性。
- `messages` request: `AgentRunMessageRequest`，包含 non-empty `input`、`client_command_id`
  与可选 `executor_config`。
- `messages` response: `AgentRunMessageResponse`，返回 command receipt、runtime session ref、
  turn ref、run ref、agent ref 和 frame ref。
- `steering` request: `AgentRunSteeringRequest`，包含 non-empty `input`、`client_command_id`
  与 expected turn/runtime refs。
- `steering` response: `AgentRunSteeringResponse`，返回 command receipt、accepted state、
  runtime session ref 和 runtime command state。
- `pending-messages` POST request: `EnqueuePendingMessageRequest`，包含 non-empty `input`、
  `client_command_id` 与可选 `executor_config`。
- `pending-messages` GET response: `PendingMessageView[]`。
- `pending-messages/{message_id}/promote` response: command receipt、`promoted: true` 和
  accepted turn/runtime refs。

#### 4. Validation & Error Matrix

| 条件 | 结果 |
| --- | --- |
| `input` 为空 | `400 BadRequest` |
| run / agent 不存在 | `404 NotFound` |
| agent 不属于 run | `409 Conflict` |
| 当前用户无 project edit 权限 | permission error |
| `client_command_id` 重放且 request digest 一致 | 返回既有 command receipt |
| `client_command_id` 重放但 request digest 不一致 | `409 Conflict` |
| `executor_config` JSON 不合法 | `400 BadRequest` |
| running command 缺少 active turn 或 expected ref 不匹配 | `409 Conflict` |
| pending message 不存在 | `404 NotFound` |

#### 5. Examples

- Idle workspace 调用 `messages` 创建下一轮 turn，并返回 command receipt 与
  run/agent/frame/runtime refs。
- Running workspace 调用 `steering`，connector 支持 steer 时注入当前 turn。
- Trace-only RuntimeSession 不具备 AgentRun workspace identity，只提供只读 trace view。

#### 6. Tests Required

- Backend route registration 覆盖 AgentRun Workspace command endpoints。
- Frontend service test 断言 URL 编码后的 `/agent-runs/{run_id}/agents/{agent_id}/...` 与
  `AgentRun*` generated DTO。
- `cargo check -p agentdash-api` 保证 handler path extractor 与 response types 对齐。
- `pnpm --filter app-web test -- lifecycle` 覆盖 service 调用面。
- grep 检查产品代码和 session specs 中 AgentRun Workspace route names 与 generated DTO names 一致。

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
dimension effect records replay 到 frame runtime surface，再由 capability
projection normalizer 生成闭包状态。多个 requested runtime command 必须按 store 返回顺序
fold replay。

runtime transition 的生产入口由各 dimension module 生成 records，并在写入 store 前调用
`CapabilityDimensionRegistry::validate_transition`。delivery outbox 写入时必须校验
delivery 的 `frame_transition_id` / `target_frame_id` 与 frame transition fact 一致。mount
directive 同时保留为 `dimension=vfs / declaration_type=mount_operation` declaration 与
`apply_mount_operations` effect，使审计来源与可 replay effect 分离但保持同源。
