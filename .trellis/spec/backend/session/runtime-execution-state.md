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

`SessionRuntimeRegistry` 内的 hook runtime 字段是 delivery binding cache：
`delivery RuntimeSession id -> AgentFrameHookRuntime(control_target)`。它存在的原因是
同一个执行器会话需要在 turn、event adapter、trace 和 live connector 同步路径复用 runtime
对象；业务 owner 始终是 `HookControlTarget { run_id, agent_id, frame_id }`。业务路径应先持有
`AgentFrameRuntimeTarget`，再通过 `SessionHookService::ensure_hook_runtime_for_target` 校验或
重建绑定；裸 delivery session lookup 只属于 hub adapter / trace 场景，不能决定 hook policy、
capability、context、VFS 或 MCP 的生效 owner。

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

`pending-messages` 队列属于同一 AgentRun command surface：running workspace 可排队、删除或列出待投递输入。
`promote` 会把指定 pending message 取出并作为当前 running workspace 的 steering 输入投递。
当 running turn 以 completed terminal 收口后，后端 terminal callback 会从队首取出下一条
pending message，并通过 `AgentRunMessageService` 作为下一轮用户消息自动投递；failed /
interrupted terminal 会暂停队列，原因是失败或取消后的自动续跑需要显式用户恢复。

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
| enqueue pending 时 execution state 非 running | `409 Conflict`，提示直接发送下一轮或等待取消收口 |
| promote pending 时 execution state 非 running 或缺少 active turn | `409 Conflict`，pending message 保持不被消费 |

#### 5. Examples

- Idle workspace 调用 `messages` 创建下一轮 turn，并返回 command receipt 与
  run/agent/frame/runtime refs。
- Running workspace 调用 `steering`，connector 支持 steer 时注入当前 turn。
- Trace-only RuntimeSession 不具备 AgentRun workspace identity，只提供只读 trace view。

## Scenario: AgentRun Pending Message Drain

### 1. Scope / Trigger

AgentRun running 态允许用户把 follow-up 输入放入 `pending-messages`。队列的执行边界在后端
runtime terminal callback，而不是浏览器刷新事件，原因是窗口切换、前端重载或投影延迟都不应决定
pending message 是否继续执行。

### 2. Signatures

Backend services:

```rust
PendingQueueService::enqueue(runtime_session_id, input, executor_config) -> PendingMessagePreview
PendingQueueService::dequeue_front(runtime_session_id) -> Option<PendingMessage>
PendingQueueService::requeue_front(runtime_session_id, PendingMessage)
SessionTerminalCallback::on_session_terminal(session_id, terminal_state)
AgentRunMessageService::dispatch_user_message(AgentRunMessageCommand)
```

HTTP command surface:

```text
POST /agent-runs/{run_id}/agents/{agent_id}/pending-messages
POST /agent-runs/{run_id}/agents/{agent_id}/pending-messages/{message_id}/promote
```

### 3. Contracts

- `pending-messages` enqueue 只接受 `SessionExecutionState::Running`。
- `pending-messages/{id}/promote` 只接受 `SessionExecutionState::Running { turn_id: Some(_) }`。
- `pending-messages/{id}/promote` 在取出消息后若 steering 投递失败，必须把消息放回队首。
- failed / interrupted terminal 会把 pending queue 标记为 paused，并在 workspace/runtime-control
  view 的 `pending_queue` 字段暴露 `pause_reason`、`message` 与 `can_resume`。
- `POST /agent-runs/{run_id}/agents/{agent_id}/pending-messages/resume` 会清除 paused 状态；若
  runtime 已静默且 AgentRun 仍可继续，会立即通过同一个 pending dispatcher 投递队首。
- resume 触发的派发失败时，队首消息会放回队列，并恢复进入 resume 前的 paused 状态，原因是
  用户显式恢复只有在下一轮命令被接受后才算完成。
- `messages` 下一轮发送入口拒绝 running / cancelling，接受 idle、completed、failed、interrupted
  runtime state，并由 AgentRun terminal status 决定是否还能继续。
- completed terminal 后的 pending drain 使用同一个 `PendingQueueService` 实例和
  `AgentRunMessageService`，client command id 使用 `pending:{pending_message_id}:{attempt_id}`，
  原因是 dispatch 失败后放回队首的消息需要用新的 command receipt 重新尝试。
- pending drain 的 dispatch 失败时必须把消息放回队首，保证队列不因 launch 失败丢消息。
- failed / interrupted terminal 会暂停 pending queue，等待显式恢复语义；completed terminal
  才自动投递队首。

### 4. Validation & Error Matrix

| 条件 | 结果 |
| --- | --- |
| workspace running 时 enqueue pending | 写入队列并返回 `PendingMessageView` |
| workspace idle / completed 时 enqueue pending | `409 Conflict`，提示使用下一轮发送 |
| workspace cancelling 时 enqueue pending | `409 Conflict`，提示等待执行器收口 |
| promote pending 时缺少 active turn | `409 Conflict`，不消费 pending message |
| completed terminal 且队列非空 | 后端自动取队首并发起下一轮 `AgentRunMessageCommand` |
| 自动 dispatch 失败 | pending message 回到队首 |
| failed / interrupted terminal | pending queue 暂停，不自动续跑 |
| 用户恢复 paused queue 且 runtime 静默 | 清除暂停状态并立即尝试投递队首 |
| 用户恢复 paused queue 但 dispatch 失败 | 队首消息保留，paused 状态保持可见 |
| 用户恢复 paused queue 且 runtime running/cancelling | 只清除暂停状态，等待当前 turn terminal 后继续 drain |

### 5. Good/Base/Bad Cases

- Good: 用户在 running turn 中排队两条消息；当前 turn completed 后，后端自动发送第一条，下一轮
  completed 后继续发送第二条。
- Base: running turn 中没有 pending message；completed terminal 只执行普通 terminal effects。
- Bad: completed / idle workspace 的新输入进入 pending queue，导致 UI 静默且不会自动投递。

### 6. Tests Required

- `PendingQueueService` 测试覆盖 `dequeue_front` 顺序和 `requeue_front` 顺序恢复。
- AgentRun pending enqueue route / helper 测试覆盖 idle、completed、cancelling 返回 conflict。
- AgentRun pending promote route / helper 测试覆盖 running without turn 不消费 pending。
- Terminal callback / bootstrap 测试覆盖 completed drain 调用 `AgentRunMessageService`，dispatch
  失败时 `requeue_front`。
- Workspace / runtime-control projection 测试覆盖 paused queue 的 `pause_reason`、`message` 和
  `can_resume`。
- Resume endpoint 测试覆盖静默 runtime 恢复后触发队首 dispatch，running/cancelling 恢复后不并发
  dispatch。
- Frontend chat control 测试覆盖 ready / completed 状态使用 `send_next`，running 状态才暴露
  `enqueue` / `steer`，paused pending queue 持续展示并提供恢复命令。

### 7. Wrong vs Correct

#### Wrong

```text
pending enqueue accepted -> turn completed -> no backend consumer -> pending row only depends on UI mode
```

#### Correct

```text
running enqueue accepted -> completed terminal -> backend terminal callback drains queue -> next AgentRun message starts
```

#### 6. Tests Required

- Backend route registration 覆盖 AgentRun Workspace command endpoints。
- Frontend service test 断言 URL 编码后的 `/agent-runs/{run_id}/agents/{agent_id}/...` 与
  `AgentRun*` generated DTO。
- `cargo check -p agentdash-api` 保证 handler path extractor 与 response types 对齐。
- `pnpm --filter app-web test -- lifecycle` 覆盖 service 调用面。
- grep 检查产品代码和 session specs 中 AgentRun Workspace route names 与 generated DTO names 一致。

## Scenario: Cancelled Turn Closing State

### 1. Scope / Trigger

AgentRun workspace、RuntimeSession runtime-control 和 Pi Agent connector 都需要区分
“取消请求已发出”与“执行器已可复用”。取消请求到 terminal / idle 收口之间的运行态使用
`Cancelling` 表达，原因是用户命令的可执行性必须同时观察 platform turn、connector live session
和 terminal trace facts。

### 2. Signatures

Application execution state:

```rust
pub enum SessionExecutionState {
    Idle,
    Running { turn_id: Option<String> },
    Cancelling { turn_id: Option<String> },
    Completed { turn_id: String },
    Failed { turn_id: String, message: Option<String> },
    Interrupted { turn_id: Option<String>, message: Option<String> },
}
```

Workspace control status values:

```text
AgentRunWorkspaceControlPlaneStatus::Cancelling -> "cancelling"
SessionRuntimeControlPlaneStatus::AnchoredCancelling -> "anchored_cancelling"
```

### 3. Contracts

- `TurnSupervisor::request_cancel(session_id)` keeps the active turn reference and moves the
  runtime snapshot into cancelling state.
- `SessionCoreService::inspect_session_execution_state(session_id)` returns
  `SessionExecutionState::Cancelling { turn_id }` while the in-memory turn is cancelling.
- AgentRun workspace `actions.send_next`、`actions.enqueue` 和 `actions.steer` are disabled while
  cancelling; `actions.cancel` can remain enabled as an idempotent cancel command.
- `PiAgentConnector::cancel(session_id)` aborts the agent and waits for the in-process Agent loop
  to become idle before the connector reports cancel completion.
- RuntimeSession detail maps cancelling state to command state `status="cancelling"` with the active
  `turn_id` when available.

### 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| Active turn receives cancel request | Runtime state becomes `Cancelling { turn_id }` |
| Workspace is cancelling | `send_next=false`, `enqueue=false`, `steer=false` |
| Workspace is cancelling and cancel is requested again | Cancel is idempotent and keeps cancelling state |
| Pi Agent provider stream is pending when abort arrives | Agent loop emits aborted assistant terminal and reaches idle |
| Next prompt starts after Pi Agent cancel completion | Prompt does not fail with stale `is_streaming` |

### 5. Good/Base/Bad Cases

- Good: User cancels a running Pi Agent turn, workspace shows cancelling, then ready only after
  terminal facts and connector idle are aligned.
- Base: User opens RuntimeSession detail during cancelling and sees `anchored_cancelling` instead
  of idle.
- Bad: Workspace enables `send_next` from platform idle while connector agent is still streaming.

### 6. Tests Required

- `TurnSupervisor` test asserts `request_cancel` keeps runtime running as cancelling with the same
  turn id.
- Agent loop test asserts abort interrupts a pending provider stream and `wait_for_idle` completes.
- Pi Agent connector test asserts cancel waits for agent idle and the next prompt reuses the same
  runtime without stale processing errors.
- Frontend chat control test asserts cancelling projection exposes no send / enqueue / steer user
  input path.

### 7. Wrong vs Correct

#### Wrong

```text
cancel requested -> clear_active_turn -> workspace ready -> next prompt reaches busy connector
```

#### Correct

```text
cancel requested -> runtime cancelling -> connector idle confirmed -> terminal fact visible -> workspace ready
```

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
| `SessionProjectionStore` | 模型上下文等 runtime projection head/segment 的 checkpoint，不表达 AgentRun 当前 surface |
| `SessionLineageStore` | runtime trace fork/rollback lineage，不替代 AgentLineage 或 subject/control tree |

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
