# Research: command/pending lifecycle implementation map

- Query: 梳理 `send_next` / `enqueue` / `steer` / `promote` / `resume` / `cancel` 当前入口、状态判断、错误文案和 turn guard，并给出可交给 implement worker 的收束方案。
- Scope: internal
- Date: 2026-06-12

## Findings

### Files Found

- `.trellis/tasks/06-12-agent-run-lifecycle-convergence/prd.md` - 任务需求，要求建立 `AgentConversationSnapshot` / `ConversationCommandIntent`，删除旧式 command 推断与误导路径。
- `.trellis/tasks/06-12-agent-run-lifecycle-convergence/design.md` - 目标设计，已定义 conversation snapshot、command resolver、model resolver、resource surface resolver 和 command state machine。
- `.trellis/tasks/06-12-agent-run-lifecycle-convergence/implement.md` - 分阶段实施计划，Phase 3 是本研究对应的 command intent resolver 主体。
- `.trellis/tasks/06-12-agent-run-lifecycle-convergence/research/current-state.md` - 已有现状证据索引，覆盖启动/模型、消息命令、前端交互、pending queue、resource surface。
- `.trellis/tasks/06-12-agent-run-lifecycle-convergence/implement.jsonl` - implement agent manifest，包含 current-state、backend workflow/session/vfs、frontend state/type 等上下文。
- `.trellis/tasks/06-12-agent-run-lifecycle-convergence/check.jsonl` - check agent manifest，包含同一 current-state 与 backend workflow/session/vfs 验证上下文。
- `.trellis/spec/backend/workflow/architecture.md` - 规定 `LifecycleRun` / `LifecycleAgent` / `AgentFrame` / `RuntimeSessionExecutionAnchor` 控制面边界。
- `.trellis/spec/backend/session/runtime-execution-state.md` - 规定 runtime turn、AgentRun workspace commands、pending queue、cancelling 和 RuntimeSession trace/control 分层。
- `.trellis/spec/backend/session/session-startup-pipeline.md` - 规定 `LaunchCommand -> FrameLaunchEnvelope -> LaunchPlan -> turn` 主链路和 frame construction 边界。
- `.trellis/spec/backend/error-handling.md` - 规定后端错误必须保留结构语义，不能在层边界把错误 `.to_string()` 抹平后再解析。
- `.trellis/spec/backend/vfs/architecture.md` - 规定 runtime mount / VFS surface 的权威语义，本研究只引用它作为不要把 session_runtime 当 workspace 主事实源的背景。
- `.trellis/spec/cross-layer/frontend-backend-contracts.md` - 规定 Rust contract -> generated TS 是前后端 DTO 唯一来源。
- `.trellis/spec/guides/cross-layer-thinking-guide.md` - 强调前端不应自行推断后端状态。
- `crates/agentdash-api/src/routes/lifecycle_agents.rs` - 当前 AgentRun workspace 与 command route 入口。
- `crates/agentdash-api/src/routes/sessions.rs` - 仍存在 session runtime control 旧控制面投影，应避免本切片改动。
- `crates/agentdash-application/src/workflow/agent_message.rs` - `send_next` 后续消息 application service。
- `crates/agentdash-application/src/workflow/agent_steering.rs` - `steer` / `promote` 共用的 application steering service。
- `crates/agentdash-application/src/session/core.rs` - runtime execution state inspection。
- `crates/agentdash-application/src/session/hub_support.rs` - internal `TurnState::{Idle,Claimed,Active,Cancelling}`。
- `crates/agentdash-application/src/session/turn_supervisor.rs` - claim/activate turn 状态转换。
- `crates/agentdash-application/src/session/pending_queue.rs` - in-memory pending queue mechanics。
- `crates/agentdash-contracts/src/workflow.rs` - 当前 generated workflow DTO 源，仍只有 action boolean。
- `packages/app-web/src/generated/workflow-contracts.ts` - 当前前端 generated command/action DTO。
- `packages/app-web/src/pages/AgentRunWorkspacePage.tsx` - 当前前端 command 分发入口。
- `packages/app-web/src/pages/AgentRunWorkspacePage.chatControlState.ts` - 当前前端 `primaryAction` / `secondaryAction` 推导。
- `packages/app-web/src/features/session/ui/SessionChatView.tsx` - 当前 composer 键盘分流、pending list 展示和 submit guard。
- `packages/app-web/src/features/session/ui/composer/PendingMessageRow.tsx` - 当前 pending row / paused banner。
- `packages/app-web/src/services/lifecycle.ts` - 当前前端 AgentRun command service endpoints。
- `packages/app-web/src/stores/projectStore.ts` - 当前 `createProjectAgentRun` 吞错返回 `null`。
- `packages/app-web/src/pages/AgentRunWorkspacePage.workspace-module.test.ts` - 当前 workspace chat control authority tests。
- `packages/app-web/src/features/session/ui/composer/PendingMessageRow.test.tsx` - 当前 pending list tests。
- `packages/app-web/src/services/lifecycle.test.ts` - 当前 AgentRun command endpoint service tests。
- `crates/agentdash-application/src/session/hub/tests.rs` - 当前 pending promote / steering application integration tests。

### Code Patterns

#### Current route registration and endpoints

- `crates/agentdash-api/src/routes/lifecycle_agents.rs:62` registers `GET /agent-runs/{run_id}/agents/{agent_id}/workspace`.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:70` registers `POST /steering`.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:74` registers `GET/POST /pending-messages`.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:79` registers `POST /pending-messages/resume`.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:87` registers `POST /pending-messages/{message_id}/promote`.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:91` registers `POST /cancel`.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:167` implements `send_agent_run_message`; it resolves AgentRun context, requires delivery runtime, rejects terminal agent, then calls `ensure_send_next_allowed`.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:201` implements `steer_agent_run`; it resolves context and passes request to `steer_runtime_session`.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:256` implements `enqueue_agent_run_pending_message`; it requires non-empty command/input and calls `ensure_pending_enqueue_allowed`.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:334` implements `resume_agent_run_pending_queue`; running/cancelling only clears pause, terminal clears pause without dispatch, otherwise `AgentRunPendingDispatcher::resume_queue` may dispatch.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:411` implements `promote_agent_run_pending_message`; it delegates to `promote_pending_message_for_runtime`.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:433` implements `cancel_agent_run`; it resolves delivery runtime and calls `session_runtime.cancel`.
- `packages/app-web/src/services/lifecycle.ts:82` posts `/messages`; `:93` posts `/steering`; `:104` posts `/pending-messages`; `:129` posts `/promote`; `:144` posts `/resume`; `:154` posts `/cancel`.

#### Current backend state checks

- `crates/agentdash-api/src/routes/lifecycle_agents.rs:574` reads `SessionExecutionState` for workspace projection.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:584` defines `delivery_running` as any `SessionExecutionState::Running { .. }`.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:588` defines `delivery_cancelling` as any `SessionExecutionState::Cancelling { .. }`.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:595` checks `supports_session_steering` only when `delivery_running`, without checking active turn.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:636` builds `AgentRunWorkspaceActionSetView` from booleans; `enqueue` is enabled for any running state at `:655`; `steer` is enabled for any running state plus supports steering at `:668`.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:689` enables cancel for running or cancelling.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:1172` `ensure_pending_enqueue_allowed` accepts any `SessionExecutionState::Running { .. }`, including `turn_id: None`.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:1190` `ensure_send_next_allowed` rejects running/cancelling and accepts idle/completed/failed/interrupted.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:1213` `ensure_pending_promote_allowed` accepts only `Running { turn_id: Some(_) }` and rejects `Running { turn_id: None }` before dequeue.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:957` `steer_runtime_session` checks optional `expected_runtime_session_id` and optional `expected_turn_id`; mismatch returns plain 409 text.
- `crates/agentdash-application/src/workflow/agent_steering.rs:135` steering service re-inspects execution state; it requires `Running { turn_id: Some(turn_id) }`.
- `crates/agentdash-application/src/workflow/agent_steering.rs:155` checks connector steering support after active-turn validation.
- `crates/agentdash-application/src/session/core.rs:150` `inspect_session_execution_state` maps runtime registry snapshots to `Running { turn_id: live_turn_id }` or `Cancelling { turn_id }`.
- `crates/agentdash-application/src/session/core.rs:127` bulk inspection downgrades every running runtime to `Running { turn_id: None }`, which is useful evidence that list projections cannot authorize active-turn commands.
- `crates/agentdash-application/src/session/hub_support.rs:168` internal `TurnState` has `Idle`, `Claimed`, `Active`, and `Cancelling`.
- `crates/agentdash-application/src/session/turn_supervisor.rs:63` claim sets `TurnState::Claimed`; `:77` activate sets `TurnState::Active`.

#### Current pending queue behavior

- `crates/agentdash-application/src/session/pending_queue.rs:84` `enqueue` appends messages by runtime session id with optional executor config.
- `crates/agentdash-application/src/session/pending_queue.rs:128` `dequeue_front` returns `None` when paused or empty.
- `crates/agentdash-application/src/session/pending_queue.rs:144` `requeue_front` preserves messages after failed dispatch/promotion.
- `crates/agentdash-application/src/session/pending_queue.rs:154` `take` removes a specific message for promote-to-steer.
- `crates/agentdash-application/src/session/pending_queue.rs:168` `pause` records `QueuePauseReason`; `:177` `resume` clears pause.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:843` `pending_queue_state_view` projects only `paused`, `pause_reason`, `message`, and `can_resume`.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:706` workspace projection passes `is_paused` and `has_delivery_runtime && !terminal_agent` into `pending_queue_state_view`; it does not consider visible messages.
- `packages/app-web/src/features/session/ui/SessionChatView.tsx:653` renders pending UI when `pendingMessages.length > 0 || pendingQueue?.paused`.
- `packages/app-web/src/features/session/ui/composer/PendingMessageRow.tsx:31` only returns null when no messages and not paused, so paused + empty still shows the banner.

#### Current frontend command derivation

- `packages/app-web/src/pages/AgentRunWorkspacePage.chatControlState.ts:48` draft mode enables `start_draft` when project/agent summary exists; model completeness is not represented here.
- `packages/app-web/src/pages/AgentRunWorkspacePage.chatControlState.ts:90` reads `workspace.actions` and `workspace.control_plane.status`.
- `packages/app-web/src/pages/AgentRunWorkspacePage.chatControlState.ts:93` maps running + enqueue enabled to primary `enqueue`.
- `packages/app-web/src/pages/AgentRunWorkspacePage.chatControlState.ts:94` maps `actions.steer.enabled` to secondary `steer`.
- `packages/app-web/src/pages/AgentRunWorkspacePage.chatControlState.ts:126` otherwise maps `actions.send_next.enabled` to primary `send_next`.
- `packages/app-web/src/features/session/ui/SessionChatView.tsx:389` local `handleSubmit` accepts an `actionOverride?: "steer" | "enqueue"`.
- `packages/app-web/src/features/session/ui/SessionChatView.tsx:499` maps Ctrl/Cmd+Enter to `steer` only when primary action is `enqueue` and secondary action enabled.
- `packages/app-web/src/pages/AgentRunWorkspacePage.tsx:429` validates primary/secondary action match in the page before calling route services.
- `packages/app-web/src/pages/AgentRunWorkspacePage.tsx:462` steering sends `expected_runtime_session_id=sessionId` and `expected_turn_id=runtimeControl?.delivery_trace_meta?.last_turn_id`; this is a trace-head token, not an active-turn command token.
- `packages/app-web/src/pages/AgentRunWorkspacePage.tsx:521` promote is allowed only if `secondaryAction.enabled` and queue is not paused.
- `packages/app-web/src/stores/projectStore.ts:386` catches `createProjectAgentRun` errors, stores `error`, and returns `null`; `AgentRunWorkspacePage.tsx:406` then throws generic `"创建 ProjectAgent AgentRun 失败。"` if response is null.

#### Current tests to update or preserve

- `crates/agentdash-api/src/routes/lifecycle_agents.rs:1430` asserts completed pending enqueue reason points to send_next.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:1439` asserts idle pending enqueue reason points to send_next.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:1446` asserts running without active turn is rejected before pending promote dequeue.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:1457` asserts paused queue projection exposes pause reason/resume.
- `crates/agentdash-application/src/workflow/agent_message.rs:765` tests message dispatch resolves anchor and delegates delivery.
- `crates/agentdash-application/src/workflow/agent_message.rs:810` tests duplicate command receipt without duplicate delivery.
- `crates/agentdash-application/src/workflow/agent_message.rs:861` tests terminal agent rejects message dispatch.
- `crates/agentdash-application/src/workflow/agent_message.rs:958` tests current AgentFrame is recorded when launch anchor frame is stale.
- `crates/agentdash-application/src/workflow/agent_message.rs:1015` tests duplicate dispatch with different digest conflicts.
- `crates/agentdash-application/src/session/hub/tests.rs:982` tests pending promote uses current AgentFrame and active turn `"turn-current"`.
- `packages/app-web/src/pages/AgentRunWorkspacePage.workspace-module.test.ts:132` currently expects running projection to expose enqueue + Ctrl/Cmd+Enter steer.
- `packages/app-web/src/pages/AgentRunWorkspacePage.workspace-module.test.ts:150` protects against terminal projection with stale running action bits.
- `packages/app-web/src/features/session/ui/composer/PendingMessageRow.test.tsx:48` currently expects paused queue status and resume action.
- `packages/app-web/src/services/lifecycle.test.ts:67` / `:82` / `:107` / `:116` cover steer/enqueue/promote/resume endpoint paths.

### Related Specs

- `.trellis/spec/backend/workflow/architecture.md`: `RuntimeSession` is connector delivery / trace evidence; `RuntimeSessionExecutionAnchor` is the stable reverse index; user-facing Agent runtime identity is run/agent/frame.
- `.trellis/spec/backend/session/runtime-execution-state.md`: AgentRun workspace commands must be projected from lifecycle control facts, active turn, command receipt, delivery summary, and connector live capability; RuntimeSession trace metadata does not decide workspace title/status/buttons.
- `.trellis/spec/backend/session/runtime-execution-state.md`: pending enqueue only accepts running; promote requires running with active turn; failed/interrupted pauses queue; completed drains queue automatically.
- `.trellis/spec/backend/session/runtime-execution-state.md`: cancelling disables send_next/enqueue/steer, while cancel may remain idempotent.
- `.trellis/spec/backend/session/session-startup-pipeline.md`: launch source adapters carry intent; frame construction owns final runtime facts; LaunchPlanner must not patch facts from stale caches.
- `.trellis/spec/backend/error-handling.md`: new shared precondition failures should be structured application/API errors, not string-only branches.
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`: generated Rust contract is the cross-layer source; frontend must consume generated DTOs instead of hand-written command semantics.

### External References

- None. This research used local task artifacts, project specs, and source code only.

## Target Contract Shape

### `ConversationCommandIntent`

Recommended generated DTO shape, owned in `agentdash-contracts::workflow` and consumed by frontend generated TS:

```rust
pub struct ConversationCommandIntent {
    pub kind: ConversationCommandKind,
    pub command_id: String,
    pub enabled: bool,
    pub stale_guard: ConversationCommandStaleGuard,
    pub input_policy: ConversationInputPolicy,
    pub executor_config_policy: ConversationExecutorConfigPolicy,
    pub placement: Vec<ConversationCommandPlacement>,
    pub shortcut: Option<ConversationShortcut>,
    pub label_key: Option<String>,
    pub disabled: Option<ConversationCommandDisabled>,
}

pub enum ConversationCommandKind {
    StartDraft,
    SendNext,
    Enqueue,
    Steer,
    PromotePending,
    ResumePendingQueue,
    Cancel,
}
```

Fields required for implement worker:

- `kind`: stable semantic command. Frontend can choose labels/icons, but may not infer business action from `control_plane.status`.
- `command_id`: opaque snapshot-scoped command token, regenerated when precondition facts change.
- `enabled`: command availability from backend resolver.
- `stale_guard.snapshot_version`: current snapshot monotonic value or digest.
- `stale_guard.run_id`, `agent_id`, `frame_id`, `runtime_session_id`, `active_turn_id`: facts the command is authorized against. `active_turn_id` must be present for `steer` and `promote_pending`.
- `input_policy`: `requires_input`, `allow_empty`, `allowed_user_input_blocks`.
- `executor_config_policy`: `required`, `forbidden`, `optional`, plus `model_config_revision` or equivalent for draft/send/enqueue.
- `placement`: `composer_primary`, `composer_secondary`, `pending_row`, `pending_banner`, `header`.
- `shortcut`: e.g. `enter`, `ctrl_enter`; do not let frontend remap Ctrl/Cmd+Enter from local primary/secondary logic.
- `disabled.code`: stable code such as `model_required`, `not_running`, `starting_claimed`, `missing_active_turn`, `terminal`, `cancelling`, `missing_delivery_runtime`, `missing_frame`, `connector_steer_unsupported`, `pending_queue_empty`, `pending_queue_paused`, `stale_snapshot`.
- `disabled.message`: user-facing message if needed; can be localized later.
- `disabled.diagnostic`: optional structured debug payload with latest state facts.

### `ConversationCommandSetView`

Recommended generated DTO:

```rust
pub struct ConversationCommandSetView {
    pub state: ConversationExecutionState,
    pub snapshot_version: String,
    pub commands: Vec<ConversationCommandIntent>,
    pub keyboard: ConversationKeyboardMap,
    pub diagnostics: Vec<ConversationDiagnosticView>,
}

pub struct ConversationKeyboardMap {
    pub enter: Option<String>,
    pub ctrl_enter: Option<String>,
}
```

Required semantics:

- `keyboard.enter` and `keyboard.ctrl_enter` reference `ConversationCommandIntent.command_id`, not a command kind guessed by frontend.
- Keep one command list for buttons, pending row, banner, and keyboard.
- Existing `AgentRunWorkspaceActionSetView` may be temporarily populated from the new command set for migration, but it should stop being the ownership model.

### Backend precondition resolver input

Create one shared resolver/checker fed by immutable facts:

```rust
pub struct ConversationPreconditionInput<'a> {
    pub project_id: Uuid,
    pub auth_identity: &'a AuthIdentity,
    pub required_permission: ProjectPermission,
    pub run: &'a LifecycleRun,
    pub agent: &'a LifecycleAgent,
    pub current_frame: Option<&'a AgentFrame>,
    pub delivery_anchor: Option<&'a RuntimeSessionExecutionAnchor>,
    pub delivery_runtime_session_id: Option<&'a str>,
    pub execution_state: SessionExecutionState,
    pub connector_supports_steering: bool,
    pub pending_messages: Vec<PendingMessagePreview>,
    pub pending_pause_reason: Option<QueuePauseReason>,
    pub model_state: ConversationModelState,
    pub snapshot_version: String,
    pub request_guard: Option<ConversationCommandGuardFromRequest>,
}
```

Resolver facts and why:

- `run` / `agent`: terminal agent/run status and ownership.
- `current_frame`: frame existence and current frame id/revision; do not authorize commands against stale launch frame.
- `delivery_anchor` / `delivery_runtime_session_id`: delivery channel and reverse control-plane proof.
- `execution_state`: must preserve `Running { turn_id: None }` as `StartingClaimed`, and `Running { turn_id: Some(_) }` as `RunningActive`.
- `connector_supports_steering`: only relevant after active-turn guard.
- `pending_messages` / `pending_pause_reason`: queue mechanics plus visible work.
- `model_state`: `Draft`, `ModelRequired`, and command executor config policy.
- `request_guard`: submitted `command_id`, `snapshot_version`, `runtime_session_id`, `active_turn_id`, `frame_id`, and `pending_message_id`.

Request precondition failures should return a typed conflict:

```rust
pub enum ConversationCommandConflictCode {
    StaleSnapshot,
    StaleRuntimeSession,
    StaleTurn,
    MissingActiveTurn,
    CommandUnavailable,
    ModelRequired,
    PendingMessageMissing,
}
```

Return shape should include latest snapshot or at least `latest_snapshot_version` and `diagnostics`, so frontend can refresh and show the current command state instead of exposing raw mismatch text.

## Authorization Matrix

Legend:

- `allow`: command should be enabled in snapshot.
- `deny`: command should not be exposed; direct API call returns structured `CommandUnavailable`.
- `cond`: enabled only when listed condition is true.
- `none`: no keyboard command.

| Conversation state | Enter | Ctrl/Cmd+Enter | pending enqueue | steer | promote pending | resume pending queue | cancel |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `Draft` | `start_draft` when input non-empty and model resolved | same as Enter | deny | deny | deny | deny | deny |
| `ModelRequired` | none | none | deny | deny | deny | deny | deny |
| `Ready` | `send_next` when input non-empty and model/executor policy satisfied | same as Enter | deny; direct call becomes stale/diagnostic because ready input belongs to `send_next` | deny | deny | cond: only if `pending.user_attention && resume_command.enabled`; may dispatch first queued message | deny |
| `StartingClaimed` | none | none | deny for initial convergence; do not queue against a claimed-but-not-active turn | deny because no active turn | deny because no active turn | deny by default; queue resume should wait for active/terminal projection | allow if delivery can accept cancel; otherwise diagnostic `starting_claimed_uncancellable` |
| `RunningActive` | `enqueue` when input non-empty | `steer` only when connector supports steering and active-turn command token exists; otherwise `enqueue` or none per backend keyboard map | allow when not paused and command set includes active runtime guard | cond: active turn + connector supports steering | cond: active turn + pending message exists + not paused + connector supports steering | deny by default; cond only if backend explicitly exposes resume to clear a visible pause while a new turn is running | allow |
| `Cancelling` | none | none | deny | deny | deny | deny; direct resume can become diagnostic-only/no-op if needed | allow idempotent cancel |
| `Terminal` | none | none | deny | deny | deny | deny; terminal paused state is historical diagnostic unless visible recovery is product-defined elsewhere | deny |

Important implementation details:

- `StartingClaimed` is not a persisted domain status; derive it from `SessionExecutionState::Running { turn_id: None }`, which originates from `TurnState::Claimed` or projections that cannot prove an active turn.
- `RunningActive` is `SessionExecutionState::Running { turn_id: Some(turn_id) }`.
- Frontend must not special-case Ctrl/Cmd+Enter. It should submit the command id referenced by `keyboard.ctrl_enter`.
- `pending enqueue` and `steer` both require the same runtime/session stale guard; `steer` additionally requires the active turn guard.
- `promote pending` should share the steer precondition and include `pending_message_id` in the guard. If dispatch fails after `take`, keep current requeue behavior.
- `resume pending queue` is a pending-command, not a direct function of `paused`. Show it only when snapshot says the user has recoverable visible pending work.

## Old Error Text To Replace

Replace these string-only failures with structured stale/diagnostic command conflicts:

- `crates/agentdash-api/src/routes/lifecycle_agents.rs:968` `"expected_runtime_session_id 不匹配: ..."` -> `409 { code: "stale_runtime_session", expected, actual, latest_snapshot_version }`.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:983` `"expected_turn_id 不匹配: ..."` -> `409 { code: "stale_turn", expected_turn_id, active_turn_id, latest_snapshot }`.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:1233` pending enqueue idle/completed/failed/interrupted messages that say "请直接发送下一轮消息" -> not user-facing command errors in normal flow; direct API call returns `code="command_unavailable"` with `replacement_command="send_next"` and latest command set.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:1224` / `:1271` `"缺少 active turn，不能投递 pending 消息"` -> `code="missing_active_turn"` or `state="starting_claimed"`; snapshot should not expose promote in this state.
- `crates/agentdash-application/src/workflow/agent_steering.rs:145` `"当前 Session 正在执行但缺少 active turn，不能运行中 steer"` -> same `missing_active_turn` / `starting_claimed`; API should not leak `Session` as user-facing owner.
- `crates/agentdash-application/src/workflow/agent_steering.rs:150` `"当前 Session 未在执行中，不能运行中 steer"` -> `code="command_unavailable"`, current conversation state included.
- `crates/agentdash-application/src/workflow/agent_steering.rs:160` `"当前执行器不支持对该运行中 Session steer"` -> `code="connector_steer_unsupported"` and AgentRun wording.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:186` / `:220` / `:278` / `:347` / `:424` / `:446` delivery runtime missing texts -> snapshot diagnostic `missing_delivery_runtime`; commands disabled.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:615` frame missing action text -> snapshot diagnostic `missing_frame`; commands disabled.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:843` paused queue message should become `pending.user_attention/message/resume_command`, not a banner purely from `paused=true`.
- `packages/app-web/src/pages/AgentRunWorkspacePage.tsx:406` `"创建 ProjectAgent AgentRun 失败。"` after store returns null -> remove null wrapper; surface backend `model_required` / validation error.
- `packages/app-web/src/pages/AgentRunWorkspacePage.tsx:382` `"请选择模型配置后再发送。"` should be replaced by snapshot `ModelRequired` command disabled state and selector diagnostic, not local executor-only validation.

## Recommended Minimal Implementation Order

1. Backend execution-state resolver only.
   - Add a private application/API helper that maps existing facts into `Draft | ModelRequired | Ready | StartingClaimed | RunningActive | Cancelling | Terminal`.
   - Preserve current endpoints, but make workspace projection distinguish `Running { turn_id: None }` from `Running { turn_id: Some(_) }`.
   - Tests: `StartingClaimed` exposes no steer/promote/enqueue keyboard; `RunningActive` exposes active-turn commands.

2. Generated command contract.
   - Add `ConversationCommandIntent`, `ConversationCommandSetView`, keyboard map, disabled codes, and precondition guard fields to `agentdash-contracts::workflow`.
   - Extend `AgentRunWorkspaceView` with `conversation_state` / `commands` while temporarily keeping old `actions`.
   - Generate TS and update service/type imports.

3. Shared command precondition checker.
   - Route `/messages`, `/pending-messages`, `/steering`, `/pending-messages/{id}/promote`, `/pending-messages/resume`, and `/cancel` through the same checker.
   - Accept old request DTOs initially, but require command guard for new frontend path when present.
   - Convert mismatch and unavailable cases to structured conflicts.

4. Pending projection cleanup.
   - Extend pending view to `visible_messages`, `paused`, `user_attention`, `resume_command`, `message`.
   - Banner renders only from `user_attention` or explicit resume command.
   - Keep queue mechanics unchanged until command contract tests pass.

5. Frontend command consumption.
   - `AgentRunWorkspacePage` submits command ids from snapshot command set.
   - `SessionChatView` receives keyboard map; remove local `primaryAction.kind === "enqueue"` steer inference.
   - `PendingMessageList` consumes `pending.user_attention` / `resume_command` / row-level promote command.

6. Store error propagation.
   - Change `projectStore.createProjectAgentRun` from `Promise<T | null>` to throwing/typed error propagation.
   - Ensure model-required/missing-model backend errors display without generic wrapper.

7. Audit/remove old mental models after green tests.
   - Remove or adapt `primaryAction/secondaryAction` business semantics.
   - Keep `SessionRuntimeControlView` only as runtime diagnostic/trace path.
   - Do not let `AgentRunWorkspaceActionSetView` remain the interactive control owner after command list migration.

## Recommended Validation Commands

Focused backend:

```powershell
cargo test -p agentdash-api lifecycle_agents
cargo test -p agentdash-application workflow::agent_message
cargo test -p agentdash-application workflow::agent_steering
cargo test -p agentdash-application session::pending_queue
cargo check -p agentdash-contracts -p agentdash-application -p agentdash-api
```

Contract and frontend:

```powershell
pnpm run contracts:check
pnpm --filter app-web run typecheck
pnpm --filter app-web run test -- AgentRunWorkspacePage
pnpm --filter app-web run test -- SessionChatView
pnpm --filter app-web run test -- PendingMessageRow
pnpm --filter app-web run test -- lifecycle
```

Audit gates:

```powershell
rg -n "primaryAction|secondaryAction|SessionRuntimeControlView|SessionRuntimeActionSetView" packages/app-web/src crates/agentdash-contracts/src
rg -n "expected_turn_id 不匹配|expected_runtime_session_id 不匹配|请直接发送下一轮消息|缺少 active turn|当前 Session .*steer" crates/agentdash-api/src crates/agentdash-application/src
rg -n "return null" packages/app-web/src/stores
rg -n "resolveVfsSurface\\(\\{ source_type: \"session_runtime\"" packages/app-web/src/features/workspace-panel packages/app-web/src/pages
```

## Do Not Modify In This Command/Pending Slice

- Do not modify `.trellis/spec/`; spec updates belong to the main session / `trellis-update-spec` after implementation learning is confirmed.
- Do not implement resource surface migration in this slice. Leave `packages/app-web/src/features/workspace-panel/model/useAgentRunWorkspaceState.ts`, `useSessionRuntimeState.ts`, `crates/agentdash-api/src/session_construction.rs`, and VFS resolver changes for the resource-surface worker.
- Do not remove ProjectAgent `/launch`, `ProjectAgentLaunchResult`, or `launchProjectAgent` in this slice unless the main contract branch has already replaced ProjectAgent start semantics. Inventory them only.
- Do not change model resolver / executor merge internals in this slice beyond consuming `ModelRequired` command state once available. Leave `project_agent_run_start.rs`, `frame_construction/mod.rs`, `composer_project_agent.rs`, ProjectAgent preset editor, and model selector ownership to the model/start worker.
- Do not rewrite SessionRuntime diagnostic routes in `crates/agentdash-api/src/routes/sessions.rs` until AgentRun conversation contract is green. They may remain trace/runtime diagnostic surfaces.
- Do not change generated TS files manually. Update Rust contracts and run the contract generator/check command.
- Do not change database migrations for command/pending unless a durable command snapshot/receipt schema is explicitly introduced by the main contract branch.

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` returned no active task, but the user explicitly supplied `.trellis/tasks/06-12-agent-run-lifecycle-convergence` and the required research output path. This file was written under that explicit task path.
- `.trellis/spec/backend/session/index.md` does not exist; relevant backend session specs are individual files such as `runtime-execution-state.md` and `session-startup-pipeline.md`.
- Current `AgentRunWorkspaceView.actions` cannot express command tokens, active-turn guards, stale snapshot guards, or keyboard mapping. Implement workers should treat it as a migration adapter, not the target contract.
- Current bulk execution-state inspection maps running sessions to `Running { turn_id: None }`; list-level projections should not authorize active-turn commands from bulk state alone.
- `resume` currently has mixed semantics: clear pause during running/cancelling/terminal, dispatch when silent. Target command model should expose user-visible resume only when pending projection says user attention is required.
- This research did not run the recommended validation commands because it is a read/research-only sub-agent pass.
