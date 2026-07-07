# Implementation Plan

## Checklist

1. Confirm runtime terminal diagnostic source
   - Trace child provider fatal error from `agentdash_agent::agent` / connector bridge into persisted Backbone events and RuntimeSession terminal evidence.
   - Decide whether diagnostic fields already exist or must be added at terminal boundary.

2. Add AgentRun list invalidation
   - Refactor the current list-specific invalidation plumbing toward a generic project projection notification port.
   - Locate child dispatch / lineage creation path, cross-run fork/new-root materialization, delivery running/terminal convergence path, title updates and run-level activity updates.
   - Emit `ControlPlaneProjectionChanged { projection: AgentRunList, ... }` from the backend when list read model changes.
   - Add frontend list-store consumer for AgentRun list projection invalidation without requiring an open workspace page.
   - Ensure reconnect/Connected can recover visible project projections because projection notification is a broadcast hint, not a durable fact.

3. Preserve terminal diagnostics in gate result
   - Extend `GateProducerTerminalEvent` or its upstream conversion with bounded diagnostic data if needed.
   - Write diagnostic/result refs into terminal-derived gate payload.
   - Keep first-writer-wins for normal result vs terminal fallback.

4. Define durable waiter/wake convergence
   - Identify where blocking `companion_request(wait=true)` waiter state can be durably marked or inferred.
   - Add a thin `gate_id + result_attempt` keyed delivery convergence marker so delivered-to-waiter and queued-for-parent-continuation are mutually exclusive and replayable.
   - Keep the marker independent from mailbox but intentionally small: no payload copy, no scheduler logic, no feed rendering, no general channel routing.
   - Ensure timed-out wait calls do not consume results and later wait calls return resolved gate payload.

5. Research and add child evidence locators
   - Dispatch or assign a focused sub-agent to research how parent-visible child AgentRun journal / lifecycle evidence should be addressed.
   - Do not hard-code child-local `lifecycle://session/...` paths as parent-visible URIs.
   - Resolve child run / agent / frame / delivery runtime session refs from lineage, gate payload and delivery binding.
   - Add child AgentRun journal and lifecycle evidence locators to gate result/result_refs using the researched product contract.
   - Keep refs bounded and permission-safe; actual reads still go through AgentRun/lifecycle VFS access.

6. Clarify wait output
   - Update `gate_item_from_gate` / wait result preview to include resolved gate summary and diagnostic status.
   - Keep large payload behind refs.

7. Clarify mailbox wake semantics
   - Ensure companion result wake stores structured payload/source/ref as authority.
   - Adjust conversation/feed projection so companion/system wake does not appear as human user input.
   - Keep model continuation concise and system-origin when scheduler injects it.
   - Route AgentDash `<subagent_notification>` payloads into system/subagent-origin projection or wait result surfaces instead of ordinary human transcript messages.

8. Add focused verification
   - Backend tests for terminal fallback diagnostic, duplicate wake idempotency, normal-result race.
   - Backend tests for waiter ended / gate resolved race and later wait salvage.
   - Backend tests for child lifecycle evidence refs.
   - Frontend tests for AgentRun list invalidation and companion/system wake rendering.
   - Run targeted `cargo test` and `pnpm` checks based on touched packages.

9. Cleanup wrong paths
   - Remove or rewrite the free-text companion result wake authority path.
   - Remove duplicate result delivery where blocking wait and mailbox continuation both inform the parent Agent.
   - Remove child-local lifecycle URI assumptions from result refs.
   - Ensure mailbox status does not carry non-mailbox result-delivery states.
   - Ensure provider/runtime fatal errors do not degrade to generic missing-result summaries.

## Risk Areas

- `text_user_input_blocks` may be used by both human input and system continuation; changing its caller shape can affect model context. Prefer introducing a typed companion/system delivery path or explicit source projection instead of changing generic user input semantics.
- `ControlPlaneProjectionChanged` currently travels over session streams; list page may need a project-level bridge or backend event so it can refresh when no workspace stream is mounted.
- Provider diagnostics may only exist in logs today. If so, add bounded diagnostic at the runtime terminal fact boundary, not by parsing logs.
- Waiter/wake mutual exclusion is correctness-sensitive. The final design must make the durable claim/marker explicit before implementation starts.
- The independent delivery marker must not become a thick subsystem. If implementation pressure suggests adding payload storage, scheduling or message routing to it, stop and redesign against mailbox/gate/channel boundaries.
- Child evidence locators must not bypass permissions and must not assume child-local lifecycle mount paths are readable from the parent Agent view.
- Future channel-system migration should remain possible: keep structured source/target/correlation/payload refs instead of encoding delivery semantics only in message text.
- Error-path cleanup should not leave compatibility branches for old wake/message semantics unless they are removed before completion. This project is pre-release; correctness wins over preserving old shapes.

## Validation Commands

Run the narrowest commands matching touched files:

```powershell
cargo test -p agentdash-application-workflow gate_wait_policy
cargo test -p agentdash-application wait_activity companion
cargo test -p agentdash-application-agentrun agent_run
pnpm --filter app-web test -- agent-run-list-state-store controlPlaneModel
pnpm run contracts:check
```

Broaden to `pnpm run frontend:check` or package-level `cargo test` if shared DTOs or generated bindings change.

## Execution Slices And Parallelism

This task should remain one parent task with focused implementation slices. Create child tasks only if the actual implementation becomes too large to review in one PR; the slices below are independently verifiable but share one target model.

### Slice A: Evidence Research

Can run in parallel.

1. Runtime diagnostic source research
   - Trace provider fatal errors from connector / agent loop into Backbone error events, RuntimeSession terminal evidence and AgentRun terminal convergence.
   - Output: exact structs/files where bounded diagnostic should be read or added.

2. Child evidence locator research
   - Research parent-visible access to child AgentRun journal / lifecycle evidence.
   - Output: stable locator contract for `result_refs.evidence[]`; do not guess child-local `lifecycle://session/...` as parent-visible URI.

3. AgentRun list invalidation research
   - Locate child lineage creation, child delivery terminal update and list read-model projection paths.
   - Output: backend invalidation production point and frontend/project-stream consumption strategy.

These three can be dispatched to sub-agents at the same time. They should write back concise findings with file anchors and recommended contract shape.

### Slice B: Backend Fact Model

Starts after Slice A has enough evidence.

1. Extend terminal-derived gate result
   - Add bounded diagnostic fields if missing.
   - Add child refs / evidence locators.
   - Keep `LifecycleGate` as result authority.

2. Add thin delivery convergence marker
   - Key: `gate_id + result_attempt`.
   - States: `pending | delivered_to_waiter | queued_for_parent_continuation | dispatched_to_parent`.
   - No payload storage, no scheduling, no feed rendering, no channel routing.

3. Wire blocking waiter vs mailbox continuation
   - `wait=true` can claim `delivered_to_waiter`.
   - Async or unclaimable waiter queues mailbox continuation.
   - Replay resolves "gate result exists but delivery marker missing/incomplete".

This slice should be mostly sequential because later code depends on the chosen marker and diagnostic shape.

### Slice C: Wait / Mailbox / Model Projection

Starts after Slice B marker and result payload shape are stable.

1. Update `WaitActivityService` gate item projection
   - Return useful status/summary/diagnostic/result refs for resolved gates.
   - Later waits salvage already resolved gate results.

2. Keep mailbox clean
   - Mailbox receives continuation envelope only after convergence chooses async parent continuation.
   - Mailbox source/payload refs point back to gate result; mailbox does not copy result authority.

3. Fix model-context/source projection
   - Companion/system wake appears as companion/system-origin continuation, not human user input.
   - Text is bounded projection derived from gate result.

This slice can split after the marker contract exists: one implementer can handle wait projection while another handles mailbox/model projection, but they must share DTO/source identity decisions.

### Slice D: AgentRun List Refresh

Can start after Slice A. It is mostly independent of waiter/wake convergence.

1. Convert list-specific plumbing into generic project projection notification
   - Preferred abstraction: `ProjectProjectionNotificationPort` / `ProjectProjectionInvalidation`.
   - `projection=AgentRunList` is one discriminant, not a dedicated transport.
   - The port should stay thin and broadcast-only; no projection payload, no mailbox/channel semantics.

2. Emit AgentRun list projection invalidation from backend
   - Child lineage created.
   - Cross-run fork/new-root materialized.
   - Child delivery terminal/status/title/activity changes that affect list rows.
   - Run-level `last_activity_at` changes that affect root row ordering and shell activity.

3. Frontend list store consumption
   - List refresh cannot depend on a workspace page being open.
   - Project-level or global AgentRun projection invalidation should refresh the right project list.
   - Project stream reconnect/Connected should recover visible list state if invalidation broadcasts were missed while disconnected.

This slice can proceed in parallel with Slice B as long as shared Backbone/contract changes are coordinated.

### Slice F: System / SubAgent Notification Source Cleanup

Can start in parallel as a focused research/implementation slice, then integrate with Slice B/C when the marker and mailbox source contracts are stable.

1. Locate AgentDash subagent notification ingress
   - Find where `<subagent_notification>` or equivalent subagent completion/failure payload enters the main conversation.
   - Identify whether it is added by host/tooling surface, mailbox wake, companion delivery, or a wait/subagent notification bridge.

2. Reclassify source before model-visible transcript assembly
   - Human composer input remains the only ordinary user input.
   - SubAgent/companion/system notification must carry source discriminant, request/gate refs and bounded status.
   - UI can show it as system/subagent event; model context can consume a bounded projection if needed.

3. Add regression coverage
   - A failed subagent notification does not appear as `UserInputSubmitted` / human message.
   - The same fact remains reachable through wait result, system/subagent event, or delivery projection.
   - The cleanup should compose with future channel system migration and should not thicken mailbox.

### Slice E: Verification And Contract Cleanup

Runs after implementation slices.

1. Backend tests
   - Provider diagnostic preserved in gate result.
   - Normal result vs terminal fallback first-writer-wins.
   - Waiter ended / gate resolved race does not lose or duplicate delivery.
   - Replay fills missing delivery convergence.
   - Child evidence locator shape is permission-safe.

2. Frontend tests
   - AgentRun list invalidates on projection change.
   - Companion/system wake is not rendered as human input.
   - Wait/mailbox/workspace projections show consistent status/refs.

3. Contract checks
   - Regenerate TS bindings if Backbone/contracts changed.
   - Run targeted Rust and app-web checks.

4. Wrong-path cleanup checks
   - Search for old authority text paths such as `Companion child result is available` and verify remaining usages are bounded projections/tests only.
   - Search for child-local lifecycle URI construction in result refs and verify it is not emitted as parent-visible URI.
   - Search for mailbox status handling of `delivered_to_waiter`-style states and verify those states live only in the thin convergence marker.
   - Search for generic missing-result summaries and verify provider/runtime diagnostics are preserved alongside them.

## Recommended Parallel Dispatch Plan

Phase 1 research can use three sub-agents in parallel:

```text
research-1: runtime diagnostic propagation
research-2: child evidence locator / parent-visible journal surface
research-3: AgentRun list invalidation source and frontend refresh path
```

Main session then consolidates the contracts and updates `design.md` before starting implementation.

Phase 2 implementation should use at most two parallel tracks:

```text
track-backend-core:
  gate result diagnostic + delivery marker + waiter/wake convergence

track-list-refresh-cleanup:
  generic ProjectProjectionNotification + AgentRunList producers + frontend list refresh
```

After backend core stabilizes, split a small projection track if needed:

```text
track-projection:
  wait result projection + mailbox/model-context source projection
```

Avoid dispatching many implementers into companion/mailbox/gate code at the same time. The risk is not merge conflict; it is semantic drift where one slice accidentally makes mailbox a result authority again.

After the current checkpoint commit, dispatch these independent slices with native `spawn_agent`:

```text
implement-list-notification-cleanup:
  refactor current AgentRunListInvalidationPort into generic project projection notification,
  keep existing list tests green, add activity producer coverage.

implement-runtime-diagnostic:
  propagate bounded provider/runtime diagnostics through terminal/gate/wait.

implement-gate-delivery-marker:
  add thin gate result delivery marker and waiter/mailbox mutual exclusion.

research-or-implement-system-subagent-notification:
  prevent AgentDash subagent/system notification from entering human transcript.

implement-child-evidence-locator:
  add researched child evidence locator contract/result refs.
```

Main session coordinates the contracts, resolves overlap, and runs final `trellis-check`.

## Segmented Commit Plan

Use small commits aligned to the slices so each one is reviewable and reversible. Commit messages must follow the project format `type(scope): 中文提交信息`.

Recommended sequence:

1. `docs(trellis): 收束 SubAgent 失败事实源规划`
   - Task artifacts, research summaries, final contract decisions.
   - No production code.

2. `fix(agent-run-list): 补齐 SubAgent 列表失效刷新`
   - Backend AgentRunList invalidation production.
   - Frontend list-store consumption.
   - Focused frontend/backend tests.

3. `fix(companion): 保留 SubAgent 失败诊断结果`
   - Gate terminal fallback diagnostic payload.
   - Wait result diagnostic projection.
   - Provider/runtime fatal no longer collapses to generic missing-result only.

4. `refactor(companion): 收束 gate result 交付互斥`
   - Thin delivery convergence marker.
   - `wait=true` vs async mailbox continuation mutual exclusion.
   - Replay/race tests.

5. `refactor(mailbox): 收束 companion wake 来源语义`
   - Companion/system wake source projection.
   - Remove user-like free-text authority path.
   - Keep mailbox clean.

6. `fix(lifecycle): 补齐 SubAgent evidence locator`
   - Parent-visible child evidence locator contract after research.
   - Result refs and tests.

7. `test(agent-lifecycle): 补齐错误路径清理验证`
   - Wrong-path search/static tests if not already covered.
   - Broader contract/frontend checks.

If implementation reveals that two adjacent slices touch the same narrow files and cannot be separated cleanly, combine them, but keep the commit body listing which acceptance criteria it covers. Do not mix list refresh with companion/wait marker work unless a shared contract change forces it.
