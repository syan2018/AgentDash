# AgentRun Mailbox And Turn Boundary Contract

## Role

AgentRun Mailbox 是 AgentRun workspace 的统一 message intake、调度队列和恢复投影。它把用户输入、hook/system steering、companion/workflow follow-up 与普通 queued work 都表达为 durable envelope，再由 scheduler 根据 runtime state、barrier 和 drain mode 选择 delivery action。

Mailbox 的原因是 AgentRun workspace command 需要同时满足幂等、跨进程恢复、前端状态投影和 Codex-compatible turn control。route-local `send_next/enqueue/steer` 分支无法表达 system pending message、hook replay dedup、claim recovery 和 stop-boundary continuation。

## Terms

| Term | Meaning |
| --- | --- |
| `AgentRunThread` | AgentRun workspace 侧 conversation/execution container，对齐 Codex `Thread`。 |
| `AgentRunTurn` | 用户可见执行生命周期，从 `SessionLaunchService::start_prompt` 到 `TurnEvent::Terminal`，对齐 Codex `Turn`。 |
| `AgentLoopTurn` | PiAgent/agent loop 内部 `AgentEvent::TurnStart/TurnEnd`，只在 AgentRun mailbox 边界引用时使用此前缀。 |
| `AgentLoopTurnBoundary` | AgentLoopTurn 结束后、下一次 assistant response 前的 scheduler trigger。 |
| `AgentRunTurnBoundary` | AgentRunTurn stop/terminal 边界；`BeforeStop` 可继续当前 loop，terminal callback 是恢复路径。 |

Bare `Turn` 不作为 AgentRun control-plane 新类型名使用。已有 connector 或 PiAgent event 名称可以保持原 API 命名，但 AgentRun mailbox/domain/DTO 必须显式使用 `AgentRunTurn` 或 `AgentLoopTurn`。

## Scenario: AgentRun Mailbox Message Scheduling

### 1. Scope / Trigger

- Trigger: AgentRun composer submit、mailbox promote/delete/resume、hook/system delivery message、AgentLoopTurn boundary、AgentRunTurn boundary、process recovery。
- Scope: `agentdash-contracts::workflow` DTO、domain mailbox records、application scheduler、command receipt、PostgreSQL repository、AgentRun workspace projection、frontend generated contract consumption。

该场景是 cross-layer contract：HTTP command、domain state、scheduler delivery、hook convergence 和 frontend projection 必须共享同一组 envelope/status/barrier 字段。

### 2. Signatures

HTTP command surface:

```text
GET    /agent-runs/{run_id}/agents/{agent_id}/workspace
POST   /agent-runs/{run_id}/agents/{agent_id}/composer-submit
GET    /agent-runs/{run_id}/agents/{agent_id}/mailbox
DELETE /agent-runs/{run_id}/agents/{agent_id}/mailbox/messages/{message_id}
POST   /agent-runs/{run_id}/agents/{agent_id}/mailbox/messages/{message_id}/promote
POST   /agent-runs/{run_id}/agents/{agent_id}/mailbox/resume
POST   /agent-runs/{run_id}/agents/{agent_id}/cancel
```

Core domain records:

```rust
pub struct AgentRunMailboxMessage {
    pub id: Uuid,
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub delivery_runtime_session_id: Option<String>,
    pub origin: MailboxMessageOrigin,
    pub source: MailboxSourceIdentity,
    pub delivery: MailboxDelivery,
    pub barrier: ConsumptionBarrier,
    pub drain_mode: MailboxDrainMode,
    pub status: MailboxMessageStatus,
    pub priority: i32,
    pub order_key: i64,
    pub source_dedup_key: Option<String>,
    pub queued_agent_run_turn_id: Option<String>,
    pub consuming_agent_run_turn_id: Option<String>,
    pub expected_active_agent_run_turn_id: Option<String>,
    pub accepted_agent_run_turn_id: Option<String>,
    pub accepted_protocol_turn_id: Option<String>,
    pub claim_token: Option<Uuid>,
    pub claim_expires_at: Option<DateTime<Utc>>,
    pub command_receipt_id: Option<Uuid>,
    pub payload_json: Option<Value>,
    pub preview: String,
    pub retain_payload: bool,
    pub attempt_count: i32,
}

pub struct MailboxSourceIdentity {
    pub namespace: String,
    pub kind: String,
    pub source_ref: Option<String>,
    pub correlation_ref: Option<String>,
    pub actor: String,
    pub route: Option<String>,
    pub display_label_key: String,
    pub metadata: Option<Value>,
}

pub enum MailboxDelivery {
    LaunchOrContinueTurn,
    SteerActiveTurn { stop_effect: SteeringStopEffect },
    ResumeLaunchSource { launch_source: LaunchSourceTag },
}

pub enum ConsumptionBarrier {
    ImmediateIfIdle,
    AgentLoopTurnBoundary,
    AgentRunTurnBoundary,
    ManualResume,
}

pub struct ConversationMailboxSnapshotView {
    pub paused: bool,
    pub user_attention: bool,
    pub resume_command: Option<ConversationCommandView>,
    pub state: Option<MailboxStateView>,
    pub messages: Vec<MailboxMessageView>,
    pub waiting_items: Vec<ConversationWaitingItemView>,
}

pub struct ConversationWaitingItemView {
    pub wait_id: String,
    pub gate_id: String,
    pub kind: String, // companion | subagent | human | exec | workflow
    pub source_ref: Option<String>,
    pub correlation_ref: Option<String>,
    pub status: String,
    pub source_label: Option<String>,
    pub preview: Option<String>,
    pub created_at: String,
    pub resolved_at: Option<String>,
}
```

PostgreSQL source identity columns:

```sql
agent_run_mailbox_messages.source_namespace text not null
agent_run_mailbox_messages.source_kind text not null
agent_run_mailbox_messages.source_ref text
agent_run_mailbox_messages.source_correlation_ref text
agent_run_mailbox_messages.source_actor text not null
agent_run_mailbox_messages.source_route text
agent_run_mailbox_messages.source_display_label_key text not null
agent_run_mailbox_messages.source_metadata text
```

Scheduler entrypoints:

```rust
AgentRunMailboxService::accept_user_message(...)
AgentRunMailboxService::accept_hook_message(...)
AgentRunMailboxService::accept_system_message(...)
AgentRunMailboxService::promote_message(...)
AgentRunMailboxService::delete_message(...)
AgentRunMailboxService::resume_mailbox(...)
AgentRunMailboxService::schedule(run_id, agent_id, trigger)
```

### 3. Contracts

- Backend envelope/domain/repository 是 AgentRun control-plane fact source。Codex app-server protocol 是优先复用的 `Thread/Turn` 基线；AgentRun-only scheduling 字段必须显式存在于 envelope/domain enum/adapter/projection/test 中。
- Mailbox message/state 的 durable owner 是 `run_id + agent_id`。`delivery_runtime_session_id` 只作为 nullable delivery/runtime trace ref 保存当前或最近一次投递证据，不能作为 mailbox ownership、权限或 cascade 删除边界。PostgreSQL columns 使用 `delivery_runtime_session_id`，原因是字段名必须表达 delivery evidence，而不是 RuntimeSession ownership。
- `composer-submit` 接收 canonical `Vec<UserInputBlock>`，claim durable command receipt；当当前用户控制该 AgentRun 时创建 mailbox envelope 并调用 scheduler，当当前用户只能使用但不控制 parent AgentRun 时转入 AgentRun fork-submit use case。response 返回 `AgentRunMessageCommandResponse { command_receipt, outcome, mailbox_message?, accepted_refs?, runtime_state?, fork? }`，其中 `fork` 携带 child AgentRun refs 和 redirect。
- `source` 是开放式 `MailboxSourceIdentity`，用于审计、projection、dedup、correlation 和未来 adapter governance。内置 composer / draft / hook / canvas / routine / companion 只通过 `namespace + kind` 表达来源身份，原因是 mailbox scheduler 的投递策略已经由 `origin`、`delivery`、`barrier`、`drain_mode`、priority 和 runtime state 承载。
- Platform broker request 本身先落到 broker-owned durable fact，例如 capability grant 使用 `PermissionGrant` 聚合；只有 broker response 需要 AgentRun 继续处理时，才创建 `MailboxSourceIdentity { namespace: "platform", kind: "permission_grant_response", source_ref: permission_grant_id, ... }` 的 mailbox envelope。原因是 permission policy、runtime capability effect 和 AgentRun continuation 是不同事实边界，mailbox 只承担 AgentRun 后续处理的 durable delivery。
- ProjectAgent draft start 使用同一组 canonical `Vec<UserInputBlock>` 创建 `MailboxSourceIdentity { namespace: "core", kind: "draft_start", actor: "user", ... }` envelope，并返回 `ProjectAgentRunStartResult.initial_message: AgentRunMessageCommandResponse`。`schedule_on_submit=false` 的 draft envelope 由 API 在 start receipt 形成后触发后台 scheduler，原因是 AgentRun workspace 必须先有 durable run/agent/frame/runtime anchor，首条消息投递才能作为可恢复的 mailbox delivery 继续推进。
- `cancel` 是 AgentRun runtime command，不创建 mailbox envelope，但必须 claim durable `AgentRunCommandReceipt`，以 `client_command_id + request_digest` 提供 duplicate replay/conflict 语义；cancel delivery 失败时 receipt 进入 `terminal_failed`。
- `outcome` 是 scheduler outcome：`launched | queued | steered | deleted | resumed | blocked | failed`。它不是 route-local command kind。
- `ProjectAgentRunStartResult.accepted_refs` 表达外层 AgentRun start refs；`initial_message.accepted_refs` 表达首条 mailbox message 的投递 refs。两者分开存在，原因是 workspace 可导航性、命令幂等和 connector turn accepted 是不同边界。
- `MailboxMessageView` 是 frontend pending/message row 的 wire source，至少暴露 `origin/source/delivery/barrier/status/preview/has_images/can_promote/can_delete/created_at/updated_at`。
- `ImmediateIfIdle + LaunchOrContinueTurn + DrainMode::One` 在没有 active AgentRunTurn 时启动或恢复一个 AgentRunTurn。
- `AgentLoopTurnBoundary + SteerActiveTurn + DrainMode::All` 在 AgentLoopTurn 结束后批量注入下一次 AgentLoopTurn，和 PiAgent `QueueMode::All` 语义对齐。
- `AgentRunTurnBoundary + LaunchOrContinueTurn + DrainMode::One` 在 AgentRunTurn stop/terminal 边界最多消费一条普通 user-origin message。`BeforeStop` 命中时以 steering continuation 继续当前 loop；terminal callback 只作为恢复路径。
- Hook `UserPromptSubmit` 的 block/context injection 仍由 hook runtime 处理。hook 产出的 delivery message，包括 `AfterTurn` steering、`BeforeStop` steering、follow-up 和 anchored auto-resume，必须写入 mailbox envelope，并使用稳定 `source_dedup_key`。
- Anchored hook auto-resume 是 terminal / exec completion 的 mailbox wake envelope。`source` 使用 `MailboxSourceIdentity::hook_auto_resume()` 并补齐 `source_ref=terminal_effect_id`、`correlation_ref={delivery_runtime_session_id}:{source_turn_id}:{terminal_event_seq}`、metadata 中的 `terminal_effect_id` / `terminal_event_seq` / `source_turn_id` / `delivery_runtime_session_id`。`source_dedup_key` 来自完整 source identity，原因是 terminal effect replay、process restart 和 terminal callback 恢复路径都需要落到同一个可审计 envelope，而不是按普通 command receipt 重复创建消息。
- Hook `follow_up` 不是 mailbox delivery class；它归一为 `SteerActiveTurn { stop_effect: ContinueOnStop }`。
- AgentRun Mailbox runtime adapter 在 Agent Loop 中只作为 `RuntimeTurnBoundaryDelegate` 参与组合。`after_turn` 负责把 hook steering / follow-up 归一为 mailbox envelope 并触发 AgentLoopTurnBoundary 调度，`before_stop` 负责 AgentRunTurnBoundary drain 并在有可消费 envelope 时继续当前 loop。压缩、上下文变换、工具策略与 provider request 观测分别由 hook runtime、admission 或对应 runtime facet 拥有，原因是 mailbox 的事实源是 durable delivery envelope 与 boundary drain state，而不是模型上下文、工具授权或 provider telemetry。
- AgentRun conversation snapshot 的 waiting projection 读取 open `LifecycleGate` / lifecycle wait record，投影到 `ConversationMailboxSnapshotView.waiting_items`。mailbox message 只承载 wake/result envelope；等待事实由 gate/wait record 持有，原因是前端需要展示"正在等什么"，而 scheduler 需要消费"结果已经到达"。`companion_wait` gate 若 payload 含 `request_type`，投影为 `kind="human"`；companion follow-up / blocking review 投影为 `subagent`；`exec_*` gate 投影为 `exec`。
- Terminal / exec wait projection 读取 `AgentRunTerminalRegistry` 的 terminal state 与 bounded output projection。`WaitActivityItem.cursor` 表达 wait activity 的更新时间游标，terminal output seq 只出现在 `result_refs.output_ref`、`result_refs.cursor` 和 `next` read continuation 中，原因是 Agent 继续等待和读取 terminal output 是两个不同 cursor 域。stdout/stderr/pty preview 只作为 bounded decision surface，完整输出仍由 terminal output owner 和 `shell_exec read` 提供。
- Terminal / exec wait 若需要模型可见系统投影，使用 `system_message` family：`kind`、`origin`、`source`、`status`、`summary`、`result_refs` / `output_ref`。原因是 terminal 完成通知是运行期事实投影，模型上下文应消费结构化来源和 refs，而不是从自然语言中恢复执行事实。
- Companion / subagent / human result wake 使用 `namespace="companion"`、稳定 `source_ref=gate_id`、`correlation_ref=request_id|dispatch_id` 和 route metadata。AgentRun-facing Companion delivery 进入 source-aware `UserInputSubmitted`，原因是这些内容是 Agent 需要处理的 user-role 输入；wait 返回值只包含 `status`、`summary`、`timed_out`、`result_refs` 与 bounded preview，结果正文保留在 gate payload、mailbox message 或对应 projection 中，原因是 wait 是 activity watcher，不是大结果传输通道。
- User-origin payload 可以在 queued/consuming 阶段短期持久以支持恢复；消费成功后按 retention policy 清理。preview、status、accepted refs 和 receipt result 继续保留用于投影与审计。
- `Consuming` message 必须有 claim token、lease 和 attempt count。scheduler completion 必须比较 claim token 后才能写入 `Dispatched`、`Steered`、`Failed` 或恢复状态。
- Scheduler 写入 `Dispatched` / `Steered` 等 mailbox projection 可见状态后，必须发布 `ControlPlaneProjectionChanged { projection=Mailbox, reason=MailboxStateChanged, mailbox_message_id, delivery_runtime_session_id }`。原因是 workspace mailbox row、composer outcome 和实时 runtime stream 都观察同一 durable mailbox 事实；状态写入和投影失效必须在 scheduler 边界收敛，前端才能从事件刷新而不是从 submit response 时序推断 queued / delivered 状态。
- `AgentRunDeliveryBinding` 写入 `running` / `terminal` 等 AgentRun list 可见 delivery 状态后，必须发布 `ControlPlaneProjectionChanged { projection=AgentRunList, run_id, agent_id, frame_id, delivery_runtime_session_id }`。发布由当前 delivery runtime 条件写入成功驱动，原因是侧栏列表与快捷入口从 AgentRun workspace shell read model 派生 delivery 状态，状态事实和列表投影失效需要在 delivery state 边界收敛，前端才能通过 project event 重新读取权威 projection。
- `Consuming` lease 过期且没有 accepted refs 时，message 进入 `Blocked` 并写入 `last_error="delivery_result_unknown"`。该状态表示 delivery 副作用边界不确定，普通 promote 不可重新排队，projection 必须给出 `can_promote=false`，原因是自动或误触重排都可能重复 launch/steer。
- `thread/resume` 只表示 runtime/view rehydrate，不隐式 drain mailbox。Mailbox resume 是 AgentDash envelope state transition，然后再由 scheduler 选择 `turn/start` 或 `turn/steer`。

### 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| `client_command_id` duplicate with same digest | replay stored command receipt and mailbox/delivery result |
| `client_command_id` duplicate with different digest | command conflict |
| current user has Project `Use` but does not own/control parent AgentRun | composer submit creates current-user child AgentRun through fork-submit |
| active AgentRunTurn missing for steer envelope | message becomes `Blocked(active_turn_missing)` or remains queued until a valid barrier |
| expected active AgentRunTurn id mismatch | command result is rejected/deferred; no duplicate steer |
| AgentLoopTurn boundary fires with multiple eligible steering messages | scheduler claims and injects all eligible `DrainMode::All` messages |
| AgentRunTurn boundary has multiple ordinary user messages | scheduler consumes at most one `DrainMode::One` message |
| `BeforeStop` consumes stop-boundary steering | current loop continues without first writing terminal |
| terminal callback runs after `BeforeStop` already consumed a message | terminal callback 恢复路径不会重复消费同一个 envelope |
| failed/interrupted AgentRunTurn with queued messages | existing queued messages become paused or blocked according to policy |
| new user message after failed/interrupted runtime | accepted as fresh envelope and may launch a new AgentRunTurn |
| expired `Consuming` lease after restart | recover to queued/blocked/terminal result according to accepted refs and retryability |
| expired `Consuming` without accepted refs | status becomes `Blocked`, `last_error="delivery_result_unknown"`, claim fields are cleared, and ordinary promote remains unavailable |
| expired `Consuming` with accepted refs | status is restored to terminal `Dispatched` / `Steered`, accepted refs are preserved, and `consumed_at` is set |
| hook terminal effect replay | same `source_dedup_key` does not create duplicate system-origin envelope |
| open lifecycle wait exists for current agent | workspace snapshot includes one `waiting_items` row with kind/source label/preview |
| `companion_wait` payload contains `request_type` | waiting item kind is `human`, not subagent |
| companion/human/subagent wait times out | tool result has `status=timed_out`, `timed_out=true`, and refs; gate remains durable for later resolution |
| companion/human/subagent result retries | source identity dedup returns the existing mailbox message |
| new Routine / Companion / channel source | assign a namespace/kind/source_ref/correlation_ref without changing scheduler branches |
| platform broker receives capability grant request | broker creates durable request fact first; mailbox envelope appears only for AgentRun continuation response |
| user-origin envelope consumed successfully | payload cleanup runs after accepted refs/result are recorded |

### 5. Good/Base/Bad Cases

- Good: running workspace receives two user messages and one hook steering message; AgentLoopTurn boundary drains the hook/user steering batch, while AgentRunTurn boundary later consumes only one ordinary pending user message.
- Good: `BeforeStop` receives a hook follow-up; scheduler consumes it as stop-boundary steering and continues the same AgentRunTurn.
- Base: idle workspace receives one user message; mailbox creates an envelope, scheduler launches one AgentRunTurn, response returns `outcome=launched` and accepted turn refs.
- Base: non-owner member submits in a visible AgentRun; fork-submit creates child AgentRun, writes the user message to child mailbox, and returns `fork.redirect`.
- Base: user deletes a queued message; status becomes `Deleted`, duplicate delete replays the same command result.
- Bad: route handler chooses launch/queue/steer directly before writing mailbox envelope, because recovery, duplicate replay and hook/system messages then observe a different state model.
- Bad: frontend infers queued/steered/dispatched from keyboard command kind, because scheduler outcome and recovery status belong to backend projection.

### 6. Tests Required

- Contract generation check asserts `MailboxMessageView`、`MailboxSourceIdentity`、`MailboxMessageStatus`、`MailboxMessageOrigin`、`MailboxDelivery`、`ConsumptionBarrier`、`MailboxDrainMode`、`AgentRunMessageCommandResponse` are present in generated TypeScript.
- Repository tests cover source identity roundtrip, order, priority, source dedup, atomic claim, claim token completion, expired claim recovery, pause/resume and payload cleanup.
- Repository/API/application tests cover `delivery_result_unknown`: recovery blocks unknown delivery result, blocked rows are not claimed automatically, API projects `can_promote=false`, and promote returns conflict instead of requeueing.
- Scheduler tests cover idle launch, running AgentLoopTurn-boundary drain-all, running no-steer AgentRunTurn-boundary drain-one, `BeforeStop` continuation, terminal callback 恢复路径 dedup, failed/interrupted pause, new user message after failure, promote, delete and manual resume.
- Hook integration tests cover `AfterTurn` steering envelope, `BeforeStop` follow-up normalization, anchored hook auto-resume envelope and terminal effect replay dedup.
- AgentRun workspace projection tests cover companion/subagent/human lifecycle gates, blocking human `companion_wait + request_type`, and `exec_*` gate kind mapping into `ConversationWaitingItemView`.
- Companion wait tests cover timeout without closing the gate, resolved payload summary/ref extraction, source identity dedup, duplicate child result no-op, and parent mailbox wake envelope.
- Terminal / exec wake tests cover hook auto-resume source identity: `source_ref=effect_id`, correlation includes runtime session / source turn / terminal event seq, and replay does not create a duplicate mailbox message.
- Scheduler/runtime-session tests cover mailbox visible status transitions emitting `ControlPlaneProjectionChanged(MailboxStateChanged)` after the status row is persisted.
- Delivery state tests cover current-runtime conditional binding writes, stale runtime no-op, lost-race no-op, and `ControlPlaneProjectionChanged(AgentRunList)` emission only after the binding row is persisted for the current delivery runtime.
- API tests cover composer submit duplicate receipt, mailbox list/delete/promote/resume, typed conflict for expected active AgentRunTurn mismatch, and no route-local `send_next/enqueue/steer` branch as authority.
- API/application tests cover composer submit fork outcome for non-owner parent and assert parent mailbox remains unchanged.
- Companion platform boundary tests cover current missing broker diagnostic for `target=platform` capability grants until broker request facts and response continuation delivery exist.
- Frontend tests cover service URLs, generated DTO consumption, mailbox row rendering by `status/barrier/delivery`, composer submit outcome refresh, and no hand-written pending DTO aliases.

### 7. Wrong vs Correct

#### Wrong

```text
composer-submit -> route checks SessionExecutionState -> SendNext | Enqueue | Steer -> separate side effects
```

#### Correct

```text
composer-submit -> command receipt -> mailbox envelope -> scheduler -> launched | queued | steered
```

#### Wrong

```text
completed terminal callback -> dequeue in-memory pending queue -> dispatch next user message
```

#### Correct

```text
BeforeStop/terminal callback 恢复路径 -> schedule(AgentRunTurnBoundary) -> claim one durable envelope -> continue or launch
```

## Scenario: Companion / SubAgent Gate Result Delivery Convergence

### 1. Scope / Trigger

- Trigger: companion / SubAgent / human wait 的 `LifecycleGate` resolve 后，需要在 blocking waiter、later `wait` 观察和 parent Agent continuation 之间保持同一结果事实。
- Scope: `LifecycleGate` resolved payload、`gate_result_delivery_markers`、companion parent mailbox wake、`WaitActivityService` lifecycle-gate source、runtime terminal diagnostic projection、AgentRun mailbox scheduler system projection。

该场景是 gate / mailbox / wait 的边界合同。`LifecycleGate` 持有等待结果；mailbox 只承载需要继续 AgentRun 的 delivery envelope；wait 是观察者；delivery marker 只表达同一 gate result 的交付收敛状态。

### 2. Signatures

PostgreSQL marker table:

```sql
gate_result_delivery_markers(
  gate_id text not null,
  result_attempt integer not null,
  status text not null,
  target_run_id text not null,
  target_agent_id text not null,
  waiter_ref text,
  mailbox_message_id uuid,
  command_receipt_id uuid,
  claim_token uuid,
  claim_expires_at timestamptz,
  created_at timestamptz not null,
  updated_at timestamptz not null,
  primary key (gate_id, result_attempt)
)
```

Marker status values:

```text
pending | delivered_to_waiter | queued_for_parent_continuation | dispatched_to_parent
```

Gate result payload carries bounded result facts:

```json
{
  "status": "failed",
  "summary": "Producer reached terminal before the expected result was written.",
  "diagnostic": {
    "kind": "provider",
    "code": "invalid_request",
    "http_status": 400,
    "provider": "llm_provider",
    "model": "configured-model",
    "message": "bounded provider message",
    "retryable": false
  },
  "result_refs": {
    "gate_id": "...",
    "child": {
      "run_id": "...",
      "agent_id": "...",
      "frame_id": "...",
      "delivery_runtime_session_id": "..."
    },
    "evidence": [
      { "kind": "journal", "scope": "child_agent_run", "relative": "journal" }
    ]
  }
}
```

### 3. Contracts

- `LifecycleGate.payload_json` 是 companion / SubAgent wait result authority。terminal fallback、normal companion response、wait result 和 parent continuation 都从该 payload 派生状态、summary、diagnostic 和 refs。
- Runtime terminal diagnostic 进入 gate payload 时必须保持 bounded typed fields；generic missing-result summary 可以保留为 protocol fallback summary，但不能替代 provider/runtime diagnostic。
- `gate_result_delivery_markers` key 为 `gate_id + result_attempt`。它只保存交付收敛所需的状态、claim 和目标引用，不保存 gate result payload、conversation text、scheduler policy 或 channel routing。
- Blocking waiter 能 claim `delivered_to_waiter` 时，parent mailbox continuation 不再为同一 `gate_id + result_attempt` 创建第二条结果 delivery。
- 无可 claim waiter、waiter 消失或 replay 发现 marker 未完成时，resolver 可以 claim `queued_for_parent_continuation`，然后由 mailbox 负责 durable envelope 和 scheduler delivery。
- Later `wait(activity_refs=[gate_id])` 直接读取已 resolved gate payload。timeout/cancel 是 wait call 结果，不消费 gate result。
- Companion parent continuation 的模型文本是 bounded user-role projection。结构化 authority 留在 gate payload、`MailboxSourceIdentity`、source dedup key 和 result refs；`UserInputSubmitted.source` 负责把 Companion route、actor 和 correlation 交给前端与审计面。
- Mailbox delivery 的模型通道由消息语义决定。`MailboxMessageOrigin::User | Companion` 中需要 Agent 继续处理的输入提交 `UserInputSubmitted`；hook / workflow / system 中真正的运行期控制事实保留 `system_message` platform projection 或 `system_delivery` context frame。

### 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| gate resolved and blocking waiter claim succeeds | marker becomes `delivered_to_waiter`; tool returns gate payload; no parent mailbox result continuation is queued for the same attempt |
| gate resolved and no waiter is claimable | marker becomes `queued_for_parent_continuation`; mailbox receives one companion/system-origin envelope |
| process crashes after gate resolve before delivery state | replay reads resolved gate payload and marker state, then completes either waiter delivery or parent continuation claim |
| wait call times out before gate resolve | gate remains open/resolvable; later wait or continuation policy can still observe the result |
| provider/runtime fatal caused terminal fallback | gate payload contains fallback summary plus bounded diagnostic fields |
| terminal fallback races normal companion response | first resolved gate payload remains authoritative; later delivery only ensures convergence |
| parent continuation text is generated from payload | text is clipped by length/item count; large result body remains behind gate payload / refs |
| Companion mailbox steering is delivered | scheduler emits source-aware `UserInputSubmitted` with companion source identity |
| hook/workflow/system control mailbox steering is delivered | scheduler emits system projection instead of `UserInputSubmitted` |

### 5. Good/Base/Bad Cases

- Good: `companion_request(wait=true)` waits on the gate, gate resolves with provider diagnostic, marker records `delivered_to_waiter`, and the caller sees diagnostic/result refs without also receiving an async parent wake for the same result.
- Good: async SubAgent fails after parent is idle; gate payload stores diagnostic/evidence refs, marker claims parent continuation, mailbox stores one companion-origin envelope, and model context receives a bounded system delivery projection.
- Base: later generic `wait(activity_refs=[gate_id])` returns the resolved gate result even though the original waiter no longer exists.
- Bad: mailbox row/status stores `delivered_to_waiter` or duplicates gate payload; that state belongs to the marker and the result belongs to `LifecycleGate`.
- Bad: parent continuation text is treated as the result body; the text is only a bounded delivery projection.

### 6. Tests Required

- Gate workflow tests cover terminal fallback diagnostic payload, child evidence refs, and first-writer-wins normal-result / fallback race.
- Marker repository tests cover waiter claim, parent continuation claim, replay of incomplete marker, and mutual exclusion for the same `gate_id + result_attempt`.
- Companion tests cover duplicate child result idempotency, parent mailbox wake source identity, and bounded parent result delivery projection text.
- Wait activity tests cover resolved gate diagnostic/result refs and timeout without consuming the gate.
- Mailbox scheduler/runtime-session tests cover Companion delivery emitting source-aware `UserInputSubmitted` while hook/workflow/system control delivery still emits system projection.
- Frontend/project stream tests cover AgentRun list projection invalidation without depending on an open workspace stream.

### 7. Wrong vs Correct

#### Wrong

```text
Runtime terminal failed
  -> mailbox input text says result failed
  -> wait also returns generic failure
  -> parent Agent decides between two text facts
```

#### Correct

```text
Runtime terminal failed
  -> LifecycleGate resolved payload carries status + diagnostic + result_refs
  -> delivery marker claims waiter or parent continuation
  -> wait observes gate payload
  -> mailbox/model receives one bounded system-origin projection when continuation is needed
```

## Scenario: AgentRun Wait Activity Runtime Tool

### 1. Scope / Trigger

- Trigger: Agent needs a single operation to wait for parallel exec, companion/subagent, human response,
  workflow gate and mailbox wake results without each source exposing its own wait protocol.
- Scope: `agentdash-application::wait_activity`, runtime tool catalog assembly, `SessionTerminalCache`,
  `LifecycleGateRepository`, `AgentRunMailboxRepository`, companion `wait=true` flow, and AgentRun workspace
  waiting projection.

This is a cross-layer contract because one Agent-facing tool must observe several source authorities while
leaving result storage, terminal output, gate payloads, mailbox scheduling and frontend projection under their
own owners.

### 2. Signatures

Application module layout:

```text
crates/agentdash-application/src/wait_activity/
  mod.rs              # module declarations and public re-exports
  provider.rs         # WaitRuntimeToolProvider and catalog binding
  tool.rs             # WaitTool plus Agent-facing JSON schema execution
  service.rs          # WaitActivityService orchestration and scope resolution
  types.rs            # request/result/item/context/error types
  sources/
    exec.rs           # terminal cache adapter
    lifecycle_gate.rs # companion/subagent/human/workflow gate adapter
    mailbox.rs        # mailbox message observation adapter
```

Runtime provider and service surface:

```rust
pub struct WaitRuntimeToolProvider {
    service: WaitActivityService,
}

impl SessionRuntimeToolProvider for WaitRuntimeToolProvider {
    fn build_tools(&self, context: &SessionRuntimeToolBuildContext) -> Vec<Arc<dyn RuntimeTool>>;
}

pub struct WaitActivityService { /* source ports */ }

impl WaitActivityService {
    pub async fn wait(
        &self,
        request: WaitActivityRequest,
        context: WaitToolContext,
    ) -> Result<WaitActivityResult, WaitActivityError>;
}
```

Tool name and input:

```text
tool name: wait
capability_key: collaboration
tool_path: collaboration::wait
source: platform:collaboration
```

```json
{
  "activity_refs": ["term_..."],
  "kinds": ["exec", "human"],
  "timeout_ms": 10000,
  "max_items": 10,
  "after_cursor": "1783000000000"
}
```

Tool output:

```json
{
  "status": "ready",
  "timed_out": false,
  "cursor": "1783000001000",
  "items": [
    {
      "activity_ref": "term_...",
      "kind": "exec",
      "status": "completed",
      "source_ref": "term_...",
      "correlation_ref": null,
      "preview": "exit 0",
      "result_refs": {},
      "cursor": "1783000001000",
      "next": {
        "tool": "shell_exec",
        "operation": "read",
        "terminal_id": "term_..."
      }
    }
  ]
}
```

### 3. Contracts

- `wait` is one AgentRun runtime tool registered by `WaitRuntimeToolProvider` in the session runtime tool
  composer. The operation selector belongs inside the tool payload only when future wait sub-operations are
  needed; source-specific top-level tools are not part of the contract because the Agent needs one generic
  suspension point.
- Runtime tool schema/admission metadata registers `wait` under the existing `collaboration` capability as
  `collaboration::wait`. The reason is that `wait` observes structured request/response and activity-return
  control-plane facts; it does not grant shell execution, file access, workflow mutation or mailbox draining.
- `mod.rs` exposes the module boundary and re-exports the public provider/service/types. Tool execution,
  catalog binding, orchestration and source adapters live in named files so the runtime tool entrypoint,
  schema and source ownership stay searchable.
- `activity_ref` uses the natural source root whenever one exists: exec uses `terminal_id`, LifecycleGate-backed
  human/subagent/companion/workflow waits use `gate_id`, and mailbox wake observation uses `mailbox_message_id`.
  A separate activity id is introduced only for a future source without a stable root.
- `WaitToolContext` resolves the current delivery runtime session to `RuntimeSessionExecutionAnchor` when
  possible and scopes gate/mailbox refs by `run_id + agent_id + frame_id`. RuntimeSession remains a delivery
  trace ref; AgentRun control-plane identity owns the activity scope.
- Exec waiting observes `SessionTerminalCache`; `starting | running` map to `running`, `exited` with zero exit
  maps to `completed`, `exited` with non-zero exit maps to `failed`, `killed` maps to `cancelled`, and `lost`
  maps to `lost`. Large stdout/stderr stays in terminal retained output and is fetched with
  `shell_exec { operation: "read", terminal_id }`.
- LifecycleGate waiting observes gate open/resolved facts. A wait call keeps refs it has already observed and
  resolves them explicitly on later polls so a gate can still be returned after it leaves an open-gate list.
  Timeout is a wait-call result and does not resolve, cancel or close the gate.
- Companion `wait=true` and human wait flows call `WaitActivityService` for blocking observation, then read the
  resolved gate payload as the result body source. Result delivery to a parent Agent still goes through the
  mailbox source identity and scheduler boundary.
- Mailbox waiting observes wake/result/completion messages and mailbox state changes. It does not claim,
  drain, launch, steer or resume turns; scheduler remains the delivery authority.
- AgentRun workspace snapshot can project current wait facts through `ConversationWaitingItemView`. Running exec
  rows use `wait_id=terminal_id`, `gate_id=terminal_id`, `kind="exec"` and `source_ref=terminal_id`; terminal
  output remains owned by the terminal projection.

### 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| `activity_refs` and `kinds` are both empty | Observe current AgentRun waitable sources in scope. |
| explicit exec `terminal_id` belongs to another delivery runtime session | Omit it from results. |
| explicit gate ref belongs to another run | Omit it from results. |
| same-run child gate is observed by a parent/subagent wait | Return the scoped gate activity when it becomes ready. |
| explicit mailbox message belongs to another run or agent | Omit it from results. |
| `timeout_ms` exceeds server cap | Apply the cap and return a normal wait result. |
| no relevant item changes before timeout | Return `status="timed_out"` and `timed_out=true`; source activity remains alive. |
| `max_items` is missing or outside bounds | Apply the service default/bounds before returning summaries. |
| `after_cursor` is present | Return only items updated after the cursor while keeping observed refs for the same wait call. |
| exec completes with non-zero exit code | Return `kind="exec"`, `status="failed"` and a `shell_exec read` continuation. |
| LifecycleGate resolves after leaving open projection | Return the resolved gate through its observed natural ref. |

### 5. Good/Base/Bad Cases

- Good: Agent starts a shell command, receives `terminal_id`, calls `wait(activity_refs=[terminal_id])`, then
  reads output through `shell_exec(operation="read", terminal_id=...)` after completion.
- Good: `companion_request(wait=true)` creates a LifecycleGate, waits through `WaitActivityService`, and returns
  the resolved payload summary without closing the gate on timeout.
- Base: Agent calls `wait(kinds=["human"])` and receives only current AgentRun human-response gates.
- Base: AgentRun workspace shows a running exec row from terminal cache while terminal output remains in the
  terminal tab/projection.
- Bad: runtime tool, provider, service and source adapters are all implemented in the module root file, because
  catalog registration, schema review and source adapter extension then require reading one mixed ownership unit.
- Bad: wait drains mailbox messages directly, because mailbox scheduling, dedup and recovery must remain under
  the scheduler.

### 6. Tests Required

- Runtime tool catalog test asserts `wait` is present after session tool composition.
- Wait service tests cover timeout without cancellation, `after_cursor`, max item bounding and scoped explicit refs.
- Exec adapter tests cover running timeout, completed zero exit, failed non-zero exit and `shell_exec read`
  continuation shape.
- LifecycleGate adapter tests cover open/resolved gates, same-run child gate visibility and resolved-after-open-list
  behavior.
- Companion wait tests assert `wait=true` uses `WaitActivityService` and preserves existing payload/result semantics.
- Mailbox adapter tests assert wake/result observation does not claim or drain messages.
- AgentRun workspace projection tests assert running exec terminal rows appear as `ConversationWaitingItemView`
  entries without adding terminal output to mailbox messages.
- Static grep/check tests assert wait tooling remains one AgentRun runtime tool and product wait/control routes stay AgentRun scoped.

### 7. Wrong vs Correct

#### Wrong

```text
wait_activity/mod.rs
  -> WaitTool
  -> WaitRuntimeToolProvider
  -> WaitActivityService
  -> exec/gate/mailbox polling details
```

#### Correct

```text
wait_activity/mod.rs
  -> pub mod provider
  -> pub mod service
  -> pub mod tool
  -> pub mod types
  -> pub mod sources

provider.rs -> catalog binding
tool.rs     -> Agent-facing schema and RuntimeTool implementation
service.rs  -> wait orchestration and scope
sources/*   -> source authority adapters
```
