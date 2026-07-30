# Current State And Recovery Evidence

## Reference Baseline

- Worktree: `D:\ABCTools_Dev\AgentDash-main-reference`
- Commit: `957fa9d60ea3d67efa1bb278fe5b376cf0c34598`
- Date: 2026-07-09 20:06:34 +0800

## Removal Timeline

| Date | Commit | Change |
| --- | --- | --- |
| 2026-06-04 | `9fa958b5b` | Added protocol-aware Steer control and `ST / Steer` transcript presentation. |
| 2026-06-14 | `bcd12fabb` | Moved Steer presentation from transcript into full Mailbox UI. |
| 2026-07-11 | `af21f9d7c` | Removed production Mailbox scheduler/delivery/commands/controls during AgentRuntime cutover. |
| 2026-07-19 | `dfb4f9037` | Removed the temporary `runtime_mailbox.rs`. |
| 2026-07-20 | `279a16fe7` | Removed Product Mailbox repositories/tables/receipt ledgers. |
| 2026-07-21 | `ec104c6bd` | Removed queued branches from synchronous input handoff. |
| 2026-07-21 | `171811bf9` | Removed Waiting/Steer/Pending frontend presentation and controls. |
| 2026-07-21 | `328e0f315` | Removed Mailbox domain lifecycle and renamed remaining paths to input handoff. |
| 2026-07-28 | `af458f82f` | Removed `/composer-submit` and explicit `delivery_intent`; unified frontend on runtime `submit_input`. |

## Current Failure Chain

1. `/runtime/commands` accepts `SubmitInput`.
2. Product reads the current Agent snapshot.
3. If Steer is available, Product rewrites SubmitInput into `AgentCommand::Steer`.
4. Dash `execute_steer` appends `InputAccepted` through a whole-repository CAS.
5. Core callback concurrently commits provider/tool history through another whole-repository CAS.
6. One writer observes a changed repository and returns `Dash source ... repository state changed`.
7. Core callback error terminalizes the active turn as `execution_callback_error`.

The 1s SQL latency increases the race window but is not the causal defect.

## Current Provenance Loss

- `DeliverAgentRunProductInput` carries `origin` and `source`.
- `prepare_delivery` only copies `content` into `AgentRunProductCommand::SubmitInput`.
- `AgentCommand::SubmitInput/Steer` contain no provenance.
- Native `canonical_projection.rs` projects every `HistoryPayload::InputAccepted` as:
  - `UserInputSubmissionKind::Prompt`
  - `UserInputSource::core_composer()`

Therefore Companion/Channel/Hook/Routine/Workflow provenance cannot survive authoritative Agent history.

## Current Hook Breakage

- Product Hook source、Rhai evaluator 和 Complete Agent callback handler仍存在：
  - `crates/agentdash-application-hooks/src/provider.rs`
  - `crates/agentdash-infrastructure/src/complete_agent_product_hook_handler.rs`
- `AfterTurn` / `BeforeStop` requirement无法进入当前 Complete Agent surface：
  - `crates/agentdash-application-hooks/src/plan.rs:153-162` 为这两个 trigger声明 `ContinueTurn` 与 `RefreshSurface`。
  - `crates/agentdash-infrastructure/src/complete_agent_product_provisioning.rs:1343-1358` 对 `ContinueTurn`、`RefreshSurface` 返回 `None`。
  - `hook_requirement` 对 required rule 的 unsupported action返回 incompatible provisioning error。
- `crates/agentdash-integration-native-agent` 没有生产 `RuntimeTurnBoundaryDelegate` 接线；`agentdash-agent/src/agent_loop.rs:424-469` 只有 delegate存在时才执行 `after_turn` / `before_stop`。
- `AgentHookDecision` 是单一 enum；`complete_agent_product_hook_handler.rs:165-170` 在一次 resolution产生多个 decision时直接返回 unsupported。
- `agent_runtime_hook_plan`、`agent_runtime_hook_run`、`agent_runtime_hook_effect` 位于 `RETIRED_POSTGRES_TABLES`，当前没有 canonical HookRun/HookEffect repository、worker和恢复链。
- 旧版完整链路位于：
  - `mailbox_runtime_adapter.rs:373-470`：AfterTurn/BeforeStop message先进入 Mailbox，再按 AgentLoop/AgentRun boundary drain。
  - `mailbox/delivery.rs:342-450`：terminal auto-resume用 terminal effect identity创建可去重 Mailbox envelope。
  - `control_effects.rs`：terminal effect outbox与 auto-resume delivery。

结论：当前保留的是 Hook definition/evaluation 的一部分，不是可交付、可恢复的完整 Hook runtime。Tool boundary的 allow/deny/rewrite仍有局部路径，但 turn-boundary、组合 action、durable effect和 auto-resume均不满足现有规范。

## Physical Recovery Matrix

### Recover Mostly As-Is

- `crates/agentdash-domain/src/agent_run_mailbox/mod.rs`
  - origin/source identity, delivery/barrier/drain/status, message/state and repository vocabulary.
- `crates/agentdash-application-agentrun/src/agent_run/mailbox/commands.rs`
  - command/result vocabulary and stable source dedup.
- `mailbox/controls.rs`
  - delete, promote, resume, move, content read and terminal pause behavior.
- `mailbox/payload.rs`
  - payload validation, preview and image handling after adapting input types.
- `mailbox/policy.rs`
  - preserve normal Queue vs explicit Steer policy; replace `SessionExecutionState` with `AgentRuntimeView`.
- `mailbox/receipts.rs`
  - keep the distinction between Mailbox intake/control receipts and concrete Agent effect receipts.
- `PostgresAgentRunMailboxRepository`
  - retain ordered claim, token/lease fencing, payload cleanup, pause/resume, reorder and recovery concepts.
- API contract mapping and frontend:
  - `agent_run_mailbox_contracts.rs`
  - `MailboxMessageRow.tsx` and tests
  - `mailboxContent.ts`
  - Session status/composer integration

### Recover Structure, Rewrite Adapter Logic

- `mailbox/mod.rs`
  - replace old dependency fan-out with a deep Mailbox module interface.
- `mailbox/delivery.rs`
  - remove RuntimeSession/frame/backend selection dependencies; target current Product binding and Complete Agent.
- `mailbox/scheduler.rs`
  - retain claim/settlement/recovery state machine; replace SessionCore/SessionLaunch/SessionControl with authoritative Agent read, explicit Product Submit/Steer and effect inspect.
- `mailbox/target.rs`
  - owner remains run+agent; resolve current Product binding instead of RuntimeSession execution anchor.
- Channel delivery adapter
  - retain the port/adapter shape from historical `d0e357f71`, but target Mailbox intake and current `ChannelDeliveryTarget::AgentInput`.

### Do Not Recover

- `mailbox_runtime_adapter.rs`
- `RuntimeSessionMailboxRuntimePort`
- `RuntimeTurnBoundaryDelegate` Mailbox composition
- `SessionCoreService`, `SessionControlService`, `SessionLaunchService`
- RuntimeSession execution anchors/delivery bindings and related migrations
- generated frontend contracts (regenerate from current Rust contract)

## Improvements Required Beyond The Old Baseline

- Old `claim_next(FOR UPDATE SKIP LOCKED)` fenced individual messages but did not lease the entire run+agent owner. Two concurrent schedulers could claim two `DrainMode::One` messages. The new design needs an owner dispatcher lease.
- Process-local Complete Agent live batches may wake the scheduler but cannot be the sole recovery signal; startup/due scans must read authoritative Agent state.
- Dash owner needs a single mutation writer independently of Product Mailbox, because callbacks and control commands are both legitimate concurrent source operations.
- Provenance must enter the AgentRuntime command contract and adapter-owned durable history.
- Public input routes must not bypass the Mailbox invariant.
- Hook callback contract must preserve composite resolutions. Agent-visible continuation must materialize before the callback returns a boundary-consumable claim; terminal auto-resume remains a separate durable HookEffect -> Mailbox path.
