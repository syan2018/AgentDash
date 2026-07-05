# Implementation Plan

## Execution Ownership

This task should be implemented by one worker end-to-end.

The bug is one causal chain: stale execution facts can make AgentRun runtime control disagree with workspace command/conversation state; fork display inheritance currently rides on model-context projection rather than an explicit UI history slice; provider waiting can disappear when stream identity, live terminal side effects, or ephemeral cursor ownership is wrong. Splitting backend, replay, and frontend into separate implementation workers would force each worker to guess the missing half of the contract.

The worker owns the full path:

1. Trace one failing path from completed parent AgentRun to forked child stream display.
2. Converge backend public execution state on one AgentRun execution snapshot.
3. Move AgentRun scoped runtime-control semantics behind that AgentRun snapshot instead of a RuntimeSession-owned public status surface.
4. Repair fork feed so child UI seed is a stable parent display-history slice, while child durable replay owns its own cursor.
5. Repair frontend command/helper/thinking consumption and workspace refresh from that contract.
6. Audit all AgentRun frontend/API RuntimeSession fact exposure and isolate remaining RuntimeSession refs as diagnostics or transport handles.
7. Clean removed facts and stale tests after the behavior is proven.

## Worker Checklist

- Build or consolidate an internal AgentRun execution snapshot used by conversation snapshot, runtime control read model, workspace command policy, and workspace projection.
- Treat RuntimeSession state as input to AgentRun snapshot construction, not as a UI/API state owner.
- Remove public active/running/cancelling decisions based on `last_delivery_status`.
- Preserve or explicitly replace runtime-session startup recovery behavior that scans stale running summaries; do not confuse recovery metadata with user-facing active state.
- Change AgentRun scoped `/runtime/control` so it agrees with workspace conversation snapshot and cannot independently surface RuntimeSession-derived running state.
- Replace AgentRun frontend service contracts that return `SessionRuntimeControlView` / `SessionShellDto` with AgentRun-scoped runtime state, or isolate them behind an explicit diagnostics surface.
- Remove AgentRun UI state construction that maps `delivery_trace_meta` or `runtime_session_ref` into user-visible `sessionMeta`, command state, cancellation state, fork readiness, or composer helper.
- Make inherited conversation feed entries explicit display seed, not durable runtime events.
- Verify fork seed does not depend on model-context compaction shape when the UI needs parent-visible history.
- Ensure child RuntimeSession replay starts from the child runtime event cursor only.
- Keep provider waiting on the ephemeral lane and verify “正在思考” renders after fork and normal sends.
- Ensure composer helper, send/cancel buttons, status bar, and fork marker read one command/execution snapshot.
- Ensure turn terminal durable events trigger AgentRun workspace snapshot refresh and clear stale running helper text.
- Decide whether `last_delivery_status` remains a terminal summary or is removed after all active-state references are gone.
- Apply migrations and model cleanup if the field is removed or semantically narrowed.
- Run the final fact-source audit searches and classify every hit as AgentRun snapshot, RuntimeSession diagnostic, internal stream transport, or removable legacy path.
- Remove unreachable compatibility branches, stale tests, mock-only statuses, and old runtime-control helpers.

## Check Strategy

Use checkers as bounded reviewers, not parallel implementers.

- Mid-check after the worker has a coherent backend + replay patch: verify stale `last_delivery_status` cannot leak into public running state, AgentRun scoped runtime control is an AgentRun snapshot projection, AgentRun scoped runtime control agrees with workspace conversation, and inherited display seed cannot affect durable cursor.
- Final checker after worker cleanup: verify task acceptance criteria, run the stale-path searches, and apply only narrow cleanup fixes.
- Required final search terms: `last_delivery_status`, `SessionRuntimeControlView`, `SessionShellDto`, `delivery_trace_meta`, `runtime_session_ref`, `session_meta`, `fetchSessionRuntimeControl`, `fetchAgentRunRuntimeControl`, and public RuntimeSession control routes. Checker must confirm AgentRun UI/API paths no longer interpret these as public execution facts.

Checker scope is the task contract in `prd.md`, `design.md`, and `research/evidence.md`. It should not restart global architecture review or propose a second decomposition.

## Validation Commands

- `cargo fmt`
- `cargo clippy --workspace --all-targets`
- `cargo test -p agentdash-application-agentrun`
- `cargo test -p agentdash-application-runtime-session`
- `pnpm --filter app-web test -- session`
- `pnpm --filter app-web typecheck`
- `pnpm --filter app-web lint`
- Fact-source audit:
  - `rg -n "last_delivery_status|SessionRuntimeControlView|SessionShellDto|delivery_trace_meta|runtime_session_ref|session_meta|fetchSessionRuntimeControl|fetchAgentRunRuntimeControl" packages/app-web/src crates/agentdash-api/src/routes crates/agentdash-contracts/src crates/agentdash-application-agentrun/src`

Adjust exact package names only if local package scripts differ; record any command that cannot run.

## Risk Points

- Runtime control and conversation snapshot currently share domain language but not a single derivation path.
- RuntimeSession recovery metadata is legitimate internally, but must not leak into public AgentRun active state.
- Fork display seed and model context currently share projection machinery; separating their contracts without duplicating unrelated logic is the main design risk.
- Provider waiting is live-only and easy to skip if stream target identity, replay boundary, or ephemeral reset is wrong.
- Migration cleanup must happen after code references are removed, not before.
