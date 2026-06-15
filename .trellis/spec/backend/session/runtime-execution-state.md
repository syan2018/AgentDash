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
`AgentFrameHookRuntimeTarget { control_target, delivery_runtime_session_id }`，再通过
`SessionHookService::ensure_hook_runtime_for_hook_target` 校验或重建绑定；裸 delivery session
lookup 只属于 hub adapter / trace 场景，不能决定 hook policy、capability、context、VFS 或 MCP
的生效 owner。

AgentRun lifecycle surface 同样从 AgentRun runtime address 构造：`run_id + agent_id +
frame_id` 是业务索引，`RuntimeSession` 只以 `MessageStreamProjectionRef` 形式进入 projector。
这样 workspace resource surface、connector VFS 和 skill baseline 都从 AgentFrame / AgentRun
控制面事实闭包得到，delivery trace 仍能通过 message stream ref 下钻到 runtime events。

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
turn。该 projection 可以引用 runtime trace ref 或 delivery
trace metadata，但它的事实来源是 ProjectAgent / Subject association / LifecycleAgent /
AgentFrame / active turn / command receipt 等 AgentRun 控制面事实。

用户 command receipt 属于 AgentRun command projection。client command identity、request
digest、duplicate/conflict 判定、accepted result 和 command-scoped terminal result 都按
command scope 记录，并以 run / agent / frame / runtime session / turn refs 表达已接受结果。
这些事实描述的是一次用户命令的幂等与回放边界，而 `SessionMeta` 描述的是 RuntimeSession
trace head；分层后，trace 恢复、事件流展示、workspace action enablement 和 command retry
可以各自消费对应 projection。

AgentRun mailbox command target 使用 `AgentRunMailboxCommandTarget` 表达：`AgentRunRuntimeAddress`
承载 run / agent / frame 业务目标，`MessageStreamProjectionRef` 承载可选 delivery trace。runtime
delegate adapter 解析 delivery anchor 后进入同一条 target-first scheduler，原因是命令幂等、
mailbox ownership 与 workspace projection 都绑定 AgentRun control-plane identity。

## AgentRun Workspace Mailbox Control Actions

用户可见执行工作台的 shell、conversation state、mailbox projection 与 resource surface 由
`AgentRunWorkspaceView` / `AgentConversationSnapshot` 表达。`AgentRunWorkspaceShell` 承载 display
title、title source、workspace/list status、last activity 和 last visible AgentRunTurn；
`AgentConversationSnapshot.execution`、`commands`、`model_config`、`mailbox` 和
`resource_surface` 承载工作台可执行状态、模型解析、待消费消息、用户注意力与可浏览资源。
`resource_surface` 来自当前 AgentFrame typed VFS 与 `RuntimeSessionExecutionAnchor` 锚定的
AgentRun lifecycle projection；该 projection 由 `AgentRunLifecycleSurfaceProjector` 按
AgentRun runtime address、optional message stream ref 和 optional orchestration node projection
闭包生成。projection 需要保留 lifecycle mount 上的 SkillAsset metadata，
原因是 builtin skill 文档、执行器 skill baseline 和前端 resource browser 应由同一 runtime surface
发现，而不是由前端或查询层单独推导。

`ConversationCommandSetView.commands` 描述用户意图 command，例如 draft start、message submit、
mailbox promote/delete/resume 与 cancel。文本输入统一走 `composer-submit`：后端先 claim command
receipt，再写 AgentRun mailbox envelope，由 scheduler 使用当前 runtime state、barrier 和
drain mode 产生 `launched | queued | steered | blocked | failed` 等 outcome。这样做的原因是
keyboard snapshot 可能滞后，而用户输入的执行语义必须以后端 durable mailbox 和当前 active
AgentRunTurn 为准。

`SessionExecutionState::Running { turn_id: None }` 投影为 `starting_claimed`，表示 runtime 已被
claim 但 active AgentRunTurn 尚未建立；`Running { turn_id: Some(_) }` 投影为 `running_active`。
ready/completed/failed/interrupted 状态可接受新的 user-origin envelope，并由 scheduler 决定是否
立即启动新的 AgentRunTurn。running 状态下的 steering 或 queued user message 不由 route 分支直接
投递，而是进入 mailbox 后在 `AgentLoopTurnBoundary` 或 `AgentRunTurnBoundary` 消费。

`RuntimeSessionTraceMeta` 可以作为 `AgentRunWorkspaceView.delivery_trace_meta` 被引用，用于展示
trace ref、event seq、executor continuation 和 terminal summary；它不决定 workspace title、list
entry、conversation status 或 command availability。`delivery_runtime_ref` 仍可出现在 workspace
view 中，原因是 AgentRun command 需要一个实际 delivery runtime 通道完成投递，而用户也需要能从
工作台下钻到 runtime trace/detail。

`SessionRuntimeControlPlaneView` 与 `SessionRuntimeControlView` 保留给
`/sessions/{id}/runtime-control` 和 RuntimeSession detail 入口。该入口从 runtime trace identity
出发，经 `RuntimeSessionExecutionAnchor` 反查 run / agent / frame，只表达 trace/detail 与
anchor backlink。AgentRun workspace 入口则从 run / agent identity 出发，返回 conversation
snapshot，原因是用户侧工作台的 command/control 必须以 AgentRun durable mailbox 与 command
snapshot 为唯一投影，RuntimeSession detail 不复制 mailbox/action 控制面。

AgentRun delivery/control command 使用 AgentRun Workspace public identity：

```text
GET    /agent-runs/{run_id}/agents/{agent_id}/workspace
POST   /agent-runs/{run_id}/agents/{agent_id}/composer-submit
GET    /agent-runs/{run_id}/agents/{agent_id}/mailbox
DELETE /agent-runs/{run_id}/agents/{agent_id}/mailbox/messages/{message_id}
POST   /agent-runs/{run_id}/agents/{agent_id}/mailbox/messages/{message_id}/promote
POST   /agent-runs/{run_id}/agents/{agent_id}/mailbox/resume
```

这些入口表达用户正在操作某个 AgentRun workspace。handler 以 Project 权限、run / agent
ownership、当前 AgentFrame、active AgentRunTurn、mailbox envelope 和 command receipt 为校验事实源，
并在 accepted result 中返回 runtime session ref、AgentRunTurn ref、protocol turn ref、frame ref
等 delivery refs。RuntimeSession trace endpoint 使用 `RuntimeSessionTraceMeta` 提供只读
trace/feed/debug 能力；follow-up 与 repository rehydrate 仍通过 trace metadata 保存的
`executor_session_id` 与 `last_event_seq` 衔接。

详细 mailbox envelope、scheduler、hook convergence 和 recovery 契约见
[AgentRun Mailbox And Turn Boundary Contract](./agentrun-mailbox.md)。

### Scenario: AgentRun Workspace Mailbox Commands

#### 1. Scope / Trigger

这些 command 是跨层 API 签名。浏览器持有 AgentRun workspace identity，后端以 run / agent /
frame / active AgentRunTurn / mailbox / command receipt 聚合当前 delivery control state，并把
RuntimeSession 作为 accepted delivery ref 或 trace ref 返回。

#### 2. Signatures

```text
GET    /agent-runs/{run_id}/agents/{agent_id}/workspace
POST   /agent-runs/{run_id}/agents/{agent_id}/composer-submit
GET    /agent-runs/{run_id}/agents/{agent_id}/mailbox
DELETE /agent-runs/{run_id}/agents/{agent_id}/mailbox/messages/{message_id}
POST   /agent-runs/{run_id}/agents/{agent_id}/mailbox/messages/{message_id}/promote
POST   /agent-runs/{run_id}/agents/{agent_id}/mailbox/resume
```

#### 3. Contracts

- `workspace` response: `AgentRunWorkspaceView`，包含 `shell`、conversation snapshot、frame/runtime refs、
  `resource_surface` 和可选 `delivery_trace_meta: RuntimeSessionTraceMeta`。mailbox state 与 messages
  位于 `conversation.mailbox`，原因是 mailbox user attention、resume command 与 command availability
  必须在同一个 conversation snapshot 中计算。
- `composer-submit` request: `AgentRunComposerSubmitRequest`，包含 non-empty `input`、
  `client_command_id`、submitted command precondition 与可选 executor config。response 返回
  `AgentRunMessageCommandResponse`，其中 `outcome` 是 scheduler outcome。
- `mailbox` GET response: `MailboxMessageView[]` 与 mailbox state projection。
- `mailbox/messages/{message_id}/promote` 只改变指定 envelope 的 delivery/barrier/priority，并调用
  scheduler；它不绕过 mailbox 直接 steer。
- `mailbox/resume` 清除 mailbox pause state，并按目标 barrier/drain mode 调用 scheduler 一次。

#### 4. Validation & Error Matrix

| 条件 | 结果 |
| --- | --- |
| `input` 为空 | `400 BadRequest` |
| run / agent 不存在 | `404 NotFound` |
| agent 不属于 run | `409 Conflict` |
| 当前用户无 project edit 权限 | permission error |
| `client_command_id` 重放且 request digest 一致 | 返回既有 command receipt 与 mailbox/delivery result |
| `client_command_id` 重放但 request digest 不一致 | `409 Conflict` |
| `executor_config` JSON 不合法 | `400 BadRequest` |
| active AgentRunTurn 与 expected ref 不匹配 | typed command conflict/deferred result，envelope 不重复投递 |
| mailbox message 不存在 | `404 NotFound` |
| promote/delete/resume 重复提交 | 通过 command receipt 或 envelope terminal state 返回稳定结果 |

#### 5. Good/Base/Bad Cases

- Good: idle workspace 提交用户输入，mailbox 创建 envelope，scheduler launch 一个 AgentRunTurn。
- Good: running workspace 提交用户输入，mailbox 依据 policy 在 AgentLoopTurnBoundary steer 或在
  AgentRunTurnBoundary queued consume one。
- Base: 没有 mailbox message 时，terminal callback 只处理普通 terminal effects。
- Bad: route handler 在写 mailbox 前直接选择 launch/queue/steer，导致 recovery 与 projection 观察到
  另一套状态。

#### 6. Tests Required

- Backend route registration 覆盖 AgentRun Workspace mailbox command endpoints。
- Frontend service test 断言 URL 编码后的 `/agent-runs/{run_id}/agents/{agent_id}/mailbox...` 与
  generated DTO 对齐。
- `cargo check -p agentdash-api` 保证 handler path extractor 与 response types 对齐。
- `pnpm --filter app-web test -- lifecycle` 覆盖 service 调用面。
- grep 检查产品代码和 session specs 中 AgentRun Workspace mailbox route names 与 generated DTO names 一致。

#### 7. Wrong vs Correct

#### Wrong

```text
composer-submit -> route-local SendNext | Enqueue | Steer side effect
```

#### Correct

```text
composer-submit -> command receipt -> mailbox envelope -> scheduler outcome
```

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
- AgentRun workspace conversation commands 在 cancelling 时不直接 launch 或 steer active turn；
  新消息可进入 mailbox projection 并等待 scheduler barrier，`cancel` command 可以保持幂等可用。
- `PiAgentConnector::cancel(session_id)` aborts the agent and waits for the in-process Agent loop
  to become idle before the connector reports cancel completion.
- RuntimeSession detail maps cancelling state to command state `status="cancelling"` with the active
  `turn_id` when available.

### 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| Active turn receives cancel request | Runtime state becomes `Cancelling { turn_id }` |
| Workspace is cancelling | direct launch/steer command unavailable; message intake returns mailbox state |
| Workspace is cancelling and cancel is requested again | Cancel is idempotent and keeps cancelling state |
| Pi Agent provider stream is pending when abort arrives | Agent loop emits aborted assistant terminal and reaches idle |
| Next prompt starts after Pi Agent cancel completion | Prompt does not fail with stale `is_streaming` |

### 5. Good/Base/Bad Cases

- Good: User cancels a running Pi Agent turn, workspace shows cancelling, then ready only after
  terminal facts and connector idle are aligned.
- Base: User opens RuntimeSession detail during cancelling and sees `anchored_cancelling` instead
  of idle.
- Bad: Workspace enables direct `turn/start` from platform idle while connector agent is still streaming.

### 6. Tests Required

- `TurnSupervisor` test asserts `request_cancel` keeps runtime running as cancelling with the same
  turn id.
- Agent loop test asserts abort interrupts a pending provider stream and `wait_for_idle` completes.
- Pi Agent connector test asserts cancel waits for agent idle and the next prompt reuses the same
  runtime without stale processing errors.
- Frontend chat control test asserts cancelling projection exposes no direct launch / steer user
  input path while mailbox intake remains backend-projected.

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

Task hook terminal effect 从 runtime trace callback 进入后构造 task runtime coordinate，并在持久化
artifact 或 status context 时记录 `orchestration_id + node_path + attempt`。这样任务投影、artifact
审计和 lifecycle node runtime facts 能共享同一定位方式。

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
