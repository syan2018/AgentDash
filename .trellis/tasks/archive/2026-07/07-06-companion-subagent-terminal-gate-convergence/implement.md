# 修复 SubAgent terminal gate 收束实施计划

## Context To Load Before Coding

- `.trellis/spec/backend/architecture.md`
- `.trellis/spec/backend/session/agentrun-mailbox.md`
- `.trellis/spec/backend/session/execution-context-frames.md`
- `.trellis/spec/backend/workflow/activity-lifecycle.md`
- `.trellis/spec/backend/capability/llm-model-config.md`
- `.trellis/spec/backend/database-guidelines.md`
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`
- `.trellis/spec/guides/cross-layer-thinking-guide.md`
- `.trellis/spec/guides/code-reuse-thinking-guide.md`

## Current Implementation Baseline

- AgentRun terminal convergence 主链路已提交在 `d0daec01f`，后续实现应复用它作为 runtime -> AgentRun producer terminal fact 的入口。
- companion gate terminal 收束已有过渡实现，可作为行为参考和测试素材；正式执行方案应把外部 seam 提升为 `LifecycleGate` / wait obligation convergence。
- `list_open_by_kind` / `list_by_agent_and_kind` 这类宽查询不再作为方案方向；后续查询应围绕 wait source、producer terminal policy 或精确 correlation。
- 当前工作区存在 extension/shared_library 方向并行改动，执行本任务时只触碰 terminal/wait/gate/preflight 所需文件。

## Ordered Checklist

- [ ] Re-inventory wait obligation storage and current gate payloads.
  - Confirm how `OpenCompanionGateCommand` / companion dispatch writes `LifecycleGate` payload and correlation.
  - Confirm where gate payload can carry `wait_source`、`expected_result`、`on_producer_terminal_without_result` and `wake` without schema migration.
  - Confirm repository support for precise open obligation lookup by producer ref or correlation.
  - Confirm current parent delivery dedup through `companion-result:{gate_id}`.
- [ ] Preserve and narrow AgentRun terminal convergence.
  - Keep runtime terminal notification as the runtime -> AgentRun adapter input.
  - Keep `RuntimeSessionExecutionAnchor` resolution inside AgentRun terminal convergence.
  - Keep stale runtime guard through current `AgentRunDeliveryBinding`.
  - Emit `AgentRunDeliveryTerminalEvent` with run/agent/frame, terminal state, turn id and optional delivery trace ref.
  - Confirm API callback exposes only AgentRun event / wait producer event to downstream modules.
- [ ] Add wait obligation declaration to companion dispatch.
  - When opening the child follow-up gate, persist a typed wait source for the child AgentRun delivery producer.
  - Persist expected result kind as companion result with the request/dispatch correlation.
  - Persist terminal policy mapping: failed -> failed, interrupted -> cancelled, completed without result -> protocol failure.
  - Persist wake intent for parent AgentRun mailbox delivery using stable `companion-result:{gate_id}`.
  - Keep existing gate kind for compatibility with projection, but do not make it the convergence interface.
- [ ] Add wait obligation convergence module at the LifecycleGate/write side.
  - External interface accepts `WaitProducerTerminalEvent`, not naked `runtime_session_id` or gate kind.
  - Find matching open obligations by wait source / producer ref.
  - Detect whether expected result already exists and apply first-writer-wins.
  - Resolve gate through existing resolver with `source = "producer_terminal"`.
  - Ensure parent mailbox wake through existing delivery intent and stable dedup.
  - Return explicit outcomes: resolved, already_resolved_ensured_delivery, no_matching_obligation, already_completed_by_result, delivery_failed.
- [ ] Move runtime callback downstream handling to wait obligation convergence.
  - `AgentRunTerminalControlCallback` should delegate runtime terminal to AgentRun convergence first.
  - Convert `AgentRunDeliveryTerminalEvent` to `WaitProducerTerminalEvent::AgentRunDelivery`.
  - Call wait obligation convergence after AgentRun delivery terminal fact is accepted.
  - Keep runtime session id only as diagnostic / delivery trace ref.
- [ ] Make parent mailbox delivery idempotently replayable for resolved gates.
  - Allow retry when gate is already resolved for the same request/gate.
  - Reuse stable source ref and `client_command_id = companion-result:{gate_id}`.
  - Keep normal `companion_respond` result payload authoritative if it won the race.
  - Add regression test for gate resolved but mailbox delivery retry needed.
- [ ] Extend wait result status validation.
  - Update workflow gate resolver normalizer to accept `failed` and `cancelled`.
  - Decide whether `protocol_failed` is a first-class status now or a `failed + failure_kind` payload shape.
  - Preserve existing `completed / blocked / needs_follow_up` behavior.
  - Add tests for accepted and rejected statuses.
- [ ] Update wait activity projection.
  - For resolved gates, prefer payload.status.
  - Fallback to `completed` only when payload status is absent.
  - Add tests for `failed`、`cancelled` and protocol failure representation.
- [ ] Converge LifecycleGate waiting projection semantics.
  - Avoid separate hand-written status rules in wait activity and workspace waiting item projection.
  - Keep exec terminal waiting item as a separate source adapter, even though it shares the UI `waiting_items` array.
  - Add tests that the same resolved companion gate surfaces the same failed/cancelled status through `wait` and workspace.
- [ ] Update workspace projection behavior if needed.
  - Confirm resolved gates are excluded by `list_open_for_agent`.
  - Add regression test from child delivery terminal to no open waiting item.
- [ ] Add boot reconcile wait obligation phase.
  - Scan open obligations that declare producer terminal policy.
  - Inspect producer current state through AgentRun delivery binding or future runtime node producer adapter.
  - Call the same wait obligation convergence path from producer terminal facts.
  - Count and diagnose reconciled/skipped/errors.
  - Keep scan bounded and diagnostic-friendly.
- [ ] Add provider/account effective model preflight.
  - Locate sub-agent preset/model selection before dispatch or frame construction.
  - Check provider account capability rather than only static catalog presence.
  - Return visible error or immediate dispatch failure before leaving a long-lived open obligation.
  - Preserve runtime provider 400 as a fallback consistency path through wait obligation convergence.
- [ ] Prune dangling legacy convergence paths after the new chain is complete.
  - Re-inventory old terminal/gate/companion wait paths that are no longer producer-fact owners.
  - Remove duplicate open-gate scans or helper APIs that keep gate kind as the convergence boundary.
  - Keep compatibility only where an active caller still has a first-principles reason to exist.
  - Update focused tests so deleted paths cannot silently reappear as parallel fact sources.
- [ ] Update contracts and migrations only if touched.
  - Run contract generation/check if Rust DTOs or TS generated types change.
  - Add migration only if typed wait source/policy needs indexed columns or outbox persistence.
- [ ] Update task/spec knowledge after implementation.
  - Capture the reason for wait obligation convergence in backend workflow/session specs if the implementation lands.
  - Record `RuntimeSessionExecutionAnchor` containment only as an architectural reason, not as a one-off bug note.

## Validation Commands

Run focused tests first:

```powershell
cargo test -p agentdash-application-workflow gate::resolver
cargo test -p agentdash-application companion::gate_control
cargo test -p agentdash-application wait_activity
cargo test -p agentdash-application-agentrun agent_run::delivery_state
cargo test -p agentdash-api agent_run_terminal_control
```

Then run broader gates matched to touched surfaces:

```powershell
pnpm run backend:check
pnpm run backend:test
pnpm run contracts:check
pnpm run migration:guard
```

For manual reproduction, use `pnpm dev` after killing the previous Rust backend process when Rust code changes. Re-run the SubAgent unsupported-model scenario and verify the parent wait result becomes failed/cancelled/protocol_failed instead of pending.

## Risk Points

- A wait obligation convergence interface that accepts gate kind would recreate the same shallow module shape under a new name; it should accept producer terminal facts.
- `complete_child_result_to_parent` currently assumes an open child-owned gate; changing retry semantics must keep request/correlation precise so unrelated agents cannot deliver into the parent mailbox.
- producer terminal and normal `companion_respond` can race; payload ownership must be first-writer-wins.
- `RuntimeSessionExecutionAnchor` must remain inside AgentRun runtime binding/convergence.
- boot reconcile scanning should be bounded and based on producer policy, not a broad historical scan over all gate kinds.
- model preflight should report the selected provider/model/preset clearly enough for the user to fix configuration.

## Review Gate Before Continuing Implementation

- PRD and design now target wait obligation convergence as the formal implementation seam.
- The next implementation slice should start by introducing typed wait source / terminal policy declaration and the convergence interface.
- Existing AgentRun terminal convergence commit remains useful as producer terminal fact input.
