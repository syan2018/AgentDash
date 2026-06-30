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
| `AgentRunTurnBoundary` | AgentRunTurn stop/terminal 边界；`BeforeStop` 可继续当前 loop，terminal callback 是 fallback。 |

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
    pub runtime_session_id: String,
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
- `composer-submit` 接收 canonical `Vec<UserInputBlock>`，claim durable command receipt，创建 mailbox envelope，再调用 scheduler。response 返回 `AgentRunMessageCommandResponse { command_receipt, outcome, mailbox_message?, accepted_refs?, runtime_state? }`。
- `source` 是开放式 `MailboxSourceIdentity`，用于审计、projection、dedup、correlation 和未来 adapter governance。内置 composer / draft / hook / canvas / routine / companion 只通过 `namespace + kind` 表达来源身份，原因是 mailbox scheduler 的投递策略已经由 `origin`、`delivery`、`barrier`、`drain_mode`、priority 和 runtime state 承载。
- Platform broker request 本身先落到 broker-owned durable fact，例如 capability grant 使用 `PermissionGrant` 聚合；只有 broker response 需要 AgentRun 继续处理时，才创建 `MailboxSourceIdentity { namespace: "platform", kind: "permission_grant_response", source_ref: permission_grant_id, ... }` 的 mailbox envelope。原因是 permission policy、runtime capability effect 和 AgentRun continuation 是不同事实边界，mailbox 只承担 AgentRun 后续处理的 durable delivery。
- ProjectAgent draft start 使用同一组 canonical `Vec<UserInputBlock>` 创建 `MailboxSourceIdentity { namespace: "core", kind: "draft_start", actor: "user", ... }` envelope，并返回 `ProjectAgentRunStartResult.initial_message: AgentRunMessageCommandResponse`。`schedule_on_submit=false` 的 draft envelope 由 API 在 start receipt 形成后触发后台 scheduler，原因是 AgentRun workspace 必须先有 durable run/agent/frame/runtime anchor，首条消息投递才能作为可恢复的 mailbox delivery 继续推进。
- `cancel` 是 AgentRun runtime command，不创建 mailbox envelope，但必须 claim durable `AgentRunCommandReceipt`，以 `client_command_id + request_digest` 提供 duplicate replay/conflict 语义；cancel delivery 失败时 receipt 进入 `terminal_failed`。
- `outcome` 是 scheduler outcome：`launched | queued | steered | deleted | resumed | blocked | failed`。它不是 route-local command kind。
- `ProjectAgentRunStartResult.accepted_refs` 表达外层 AgentRun start refs；`initial_message.accepted_refs` 表达首条 mailbox message 的投递 refs。两者分开存在，原因是 workspace 可导航性、命令幂等和 connector turn accepted 是不同边界。
- `MailboxMessageView` 是 frontend pending/message row 的 wire source，至少暴露 `origin/source/delivery/barrier/status/preview/has_images/can_promote/can_delete/created_at/updated_at`。
- `ImmediateIfIdle + LaunchOrContinueTurn + DrainMode::One` 在没有 active AgentRunTurn 时启动或恢复一个 AgentRunTurn。
- `AgentLoopTurnBoundary + SteerActiveTurn + DrainMode::All` 在 AgentLoopTurn 结束后批量注入下一次 AgentLoopTurn，和 PiAgent `QueueMode::All` 语义对齐。
- `AgentRunTurnBoundary + LaunchOrContinueTurn + DrainMode::One` 在 AgentRunTurn stop/terminal 边界最多消费一条普通 user-origin message。`BeforeStop` 命中时以 steering continuation 继续当前 loop；terminal callback 只作为 fallback。
- Hook `UserPromptSubmit` 的 block/rewrite/context injection 仍由 hook runtime 处理。hook 产出的 delivery message，包括 `AfterTurn` steering、`BeforeStop` steering、follow-up 和 anchored auto-resume，必须写入 mailbox envelope，并使用稳定 `source_dedup_key`。
- Hook `follow_up` 不是 mailbox delivery class；它归一为 `SteerActiveTurn { stop_effect: ContinueOnStop }`。
- AgentRun Mailbox runtime adapter 在 Agent Loop 中只作为 `RuntimeTurnBoundaryDelegate` 参与组合。`after_turn` 负责把 hook steering / follow-up 归一为 mailbox envelope 并触发 AgentLoopTurnBoundary 调度，`before_stop` 负责 AgentRunTurnBoundary drain 并在有可消费 envelope 时继续当前 loop。压缩、上下文变换、工具策略与 provider request 观测分别由 hook runtime、admission 或对应 runtime facet 拥有，原因是 mailbox 的事实源是 durable delivery envelope 与 boundary drain state，而不是模型上下文、工具授权或 provider telemetry。
- User-origin payload 可以在 queued/consuming 阶段短期持久以支持恢复；消费成功后按 retention policy 清理。preview、status、accepted refs 和 receipt result 继续保留用于投影与审计。
- `Consuming` message 必须有 claim token、lease 和 attempt count。scheduler completion 必须比较 claim token 后才能写入 `Dispatched`、`Steered`、`Failed` 或恢复状态。
- `Consuming` lease 过期且没有 accepted refs 时，message 进入 `Blocked` 并写入 `last_error="delivery_result_unknown"`。该状态表示 delivery 副作用边界不确定，普通 promote 不可重新排队，projection 必须给出 `can_promote=false`，原因是自动或误触重排都可能重复 launch/steer。
- `thread/resume` 只表示 runtime/view rehydrate，不隐式 drain mailbox。Mailbox resume 是 AgentDash envelope state transition，然后再由 scheduler 选择 `turn/start` 或 `turn/steer`。

### 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| `client_command_id` duplicate with same digest | replay stored command receipt and mailbox/delivery result |
| `client_command_id` duplicate with different digest | command conflict |
| active AgentRunTurn missing for steer envelope | message becomes `Blocked(active_turn_missing)` or remains queued until a valid barrier |
| expected active AgentRunTurn id mismatch | command result is rejected/deferred; no duplicate steer |
| AgentLoopTurn boundary fires with multiple eligible steering messages | scheduler claims and injects all eligible `DrainMode::All` messages |
| AgentRunTurn boundary has multiple ordinary user messages | scheduler consumes at most one `DrainMode::One` message |
| `BeforeStop` consumes stop-boundary steering | current loop continues without first writing terminal |
| terminal callback runs after `BeforeStop` already consumed a message | fallback does not consume the same envelope again |
| failed/interrupted AgentRunTurn with queued messages | existing queued messages become paused or blocked according to policy |
| new user message after failed/interrupted runtime | accepted as fresh envelope and may launch a new AgentRunTurn |
| expired `Consuming` lease after restart | recover to queued/blocked/terminal result according to accepted refs and retryability |
| expired `Consuming` without accepted refs | status becomes `Blocked`, `last_error="delivery_result_unknown"`, claim fields are cleared, and ordinary promote remains unavailable |
| expired `Consuming` with accepted refs | status is restored to terminal `Dispatched` / `Steered`, accepted refs are preserved, and `consumed_at` is set |
| hook terminal effect replay | same `source_dedup_key` does not create duplicate system-origin envelope |
| new Routine / Companion / channel source | assign a namespace/kind/source_ref/correlation_ref without changing scheduler branches |
| platform broker receives capability grant request | broker creates durable request fact first; mailbox envelope appears only for AgentRun continuation response |
| user-origin envelope consumed successfully | payload cleanup runs after accepted refs/result are recorded |

### 5. Good/Base/Bad Cases

- Good: running workspace receives two user messages and one hook steering message; AgentLoopTurn boundary drains the hook/user steering batch, while AgentRunTurn boundary later consumes only one ordinary pending user message.
- Good: `BeforeStop` receives a hook follow-up; scheduler consumes it as stop-boundary steering and continues the same AgentRunTurn.
- Base: idle workspace receives one user message; mailbox creates an envelope, scheduler launches one AgentRunTurn, response returns `outcome=launched` and accepted turn refs.
- Base: user deletes a queued message; status becomes `Deleted`, duplicate delete replays the same command result.
- Bad: route handler chooses launch/queue/steer directly before writing mailbox envelope, because recovery, duplicate replay and hook/system messages then observe a different state model.
- Bad: frontend infers queued/steered/dispatched from keyboard command kind, because scheduler outcome and recovery status belong to backend projection.

### 6. Tests Required

- Contract generation check asserts `MailboxMessageView`、`MailboxSourceIdentity`、`MailboxMessageStatus`、`MailboxMessageOrigin`、`MailboxDelivery`、`ConsumptionBarrier`、`MailboxDrainMode`、`AgentRunMessageCommandResponse` are present in generated TypeScript.
- Repository tests cover source identity roundtrip, order, priority, source dedup, atomic claim, claim token completion, expired claim recovery, pause/resume and payload cleanup.
- Repository/API/application tests cover `delivery_result_unknown`: recovery blocks unknown delivery result, blocked rows are not claimed automatically, API projects `can_promote=false`, and promote returns conflict instead of requeueing.
- Scheduler tests cover idle launch, running AgentLoopTurn-boundary drain-all, running no-steer AgentRunTurn-boundary drain-one, `BeforeStop` continuation, terminal fallback dedup, failed/interrupted pause, new user message after failure, promote, delete and manual resume.
- Hook integration tests cover `AfterTurn` steering envelope, `BeforeStop` follow-up normalization, anchored hook auto-resume envelope and terminal effect replay dedup.
- API tests cover composer submit duplicate receipt, mailbox list/delete/promote/resume, typed conflict for expected active AgentRunTurn mismatch, and no route-local `send_next/enqueue/steer` branch as authority.
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
BeforeStop/terminal fallback -> schedule(AgentRunTurnBoundary) -> claim one durable envelope -> continue or launch
```
