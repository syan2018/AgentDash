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

## Ordered Checklist

- [ ] Inventory existing companion gate and mailbox APIs.
  - Confirm how `CompleteChildResultGateCommand` builds `GateDeliveryIntent`.
  - Confirm current parent delivery dedup through `companion-result:{gate_id}`.
  - Confirm repository methods available for runtime session anchor, lifecycle agent lineage, gate lookup and delivery binding lookup.
- [ ] Extend companion result status validation.
  - Update workflow gate resolver normalizer to accept `failed` and `cancelled`.
  - Preserve existing `completed / blocked / needs_follow_up` behavior.
  - Add tests for accepted and rejected statuses.
- [ ] Add AgentRun terminal convergence deep module.
  - Accept runtime terminal notification as the runtime -> AgentRun adapter input.
  - Resolve runtime trace/anchor internally; do not expose anchor repository to caller business code.
  - Write terminal transition through `AgentRunDeliveryStateService`.
  - Apply AgentRun mailbox terminal behavior for completed/failed/interrupted.
  - Return `AgentRunDeliveryTerminalEvent` with run/agent/frame, terminal state, turn id and optional delivery trace ref.
- [ ] Add touched-path anchor containment checks.
  - API terminal callback should depend on AgentRun terminal convergence instead of anchor/mailbox implementation details.
  - Companion terminal gate convergence should consume AgentRun terminal events, not runtime session ids.
  - Workspace waiting projection should read gate/delivery facts, not resolve runtime anchors for status decisions.
  - Record remaining non-terminal anchor consumers as follow-up convergence candidates if they are confirmed outside this task's behavioral path.
- [ ] Add companion terminal gate convergence module.
  - Accept `AgentRunDeliveryTerminalEvent`, not naked `runtime_session_id`.
  - Find child-owned open companion wait gate for the active dispatch.
  - Build terminal payload with `source = "runtime_terminal"`.
  - Resolve gate through existing resolver and deliver parent mailbox wake.
  - Return explicit outcomes: resolved, already_resolved_ensured_delivery, no_companion_gate, ignored_non_child, delivery_failed.
- [ ] Wire terminal convergence into `AgentRunTerminalControlCallback`.
  - API callback should delegate to AgentRun terminal convergence and companion convergence rather than querying anchor/mailbox pieces directly.
  - Keep diagnostics for runtime_session_id as trace identity only.
  - Ensure convergence errors are logged and retriable via the existing terminal effect/replay contract where applicable.
- [ ] Make parent mailbox delivery idempotently replayable for resolved gates.
  - Allow retry when gate is already resolved for the same request/gate.
  - Reuse stable source ref and `client_command_id = companion-result:{gate_id}`.
  - Add regression test for gate resolved but mailbox delivery retry needed.
- [ ] Update wait activity projection.
  - For resolved gates, prefer payload.status.
  - Fallback to `completed` only when payload status is absent.
  - Add tests for `failed` and `cancelled`.
- [ ] Converge LifecycleGate waiting projection semantics.
  - Avoid separate hand-written status rules in wait activity and workspace waiting item projection.
  - Keep exec terminal waiting item as a separate source adapter, even though it shares the UI `waiting_items` array.
  - Add tests that the same resolved companion gate surfaces the same failed/cancelled status through `wait` and workspace.
- [ ] Update workspace projection behavior if needed.
  - Confirm resolved gates are excluded by `list_open_for_agent`.
  - Add regression test from child delivery terminal to no open waiting item.
- [ ] Add boot reconcile companion terminal phase.
  - Scan open companion wait gates.
  - Inspect owner child delivery/runtime terminal state.
  - Call the same AgentRun/companion convergence path from delivery binding facts; avoid direct anchor-driven business lookup.
  - Count and diagnose reconciled/skipped/errors.
- [ ] Add provider/account effective model preflight.
  - Locate sub-agent preset/model selection before dispatch or frame construction.
  - Check provider account capability rather than only static catalog presence.
  - Return visible error or immediate dispatch failure without leaving long-lived open gate.
- [ ] Update contracts and migrations only if touched.
  - Run contract generation/check if Rust DTOs or TS generated types change.
  - Add migration only if a new persistent outbox/table/column is introduced.

## Validation Commands

Run focused tests first:

```powershell
cargo test -p agentdash-application-workflow gate::resolver
cargo test -p agentdash-application companion::gate_control
cargo test -p agentdash-application wait_activity
cargo test -p agentdash-application-agentrun agent_run
cargo test -p agentdash-api agent_run_terminal_control
```

Then run broader gates matched to touched surfaces:

```powershell
pnpm run backend:check
pnpm run backend:test
pnpm run contracts:check
pnpm run migration:guard
```

For manual reproduction, use `pnpm dev` after killing the previous Rust backend process when Rust code changes. Re-run the SubAgent unsupported-model scenario and verify the parent wait result becomes failed/cancelled instead of pending.

## Risk Points

- `complete_child_result_to_parent` currently assumes an open child-owned gate; changing retry semantics must not allow unrelated agents to deliver into the parent mailbox.
- terminal callback and `companion_respond` can race; payload ownership must be first-writer-wins.
- RuntimeSessionExecutionAnchor must not become the public interface of the new bridge; contain it inside AgentRun runtime binding/convergence.
- bridge must distinguish “not a companion child” from “companion child with missing gate” so ordinary AgentRun terminal events stay cheap and quiet.
- boot reconcile scanning should be bounded and diagnostic-friendly; it should not turn startup into a broad expensive query over all historical gates.
- model preflight should report the selected provider/model/preset clearly enough for the user to fix configuration.

## Review Gate Before `task.py start`

- PRD, design and implementation checklist are reviewed.
- `implement.jsonl` and `check.jsonl` contain real spec/research context entries.
- The next implementer confirms whether this task should be done inline or by a Trellis implementation sub-agent.
