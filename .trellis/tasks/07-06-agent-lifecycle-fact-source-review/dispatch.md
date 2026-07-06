# Dispatch Tracking

## Current Trellis State

- Active task: `.trellis/tasks/07-06-agent-lifecycle-fact-source-review`
- Status: `in_progress`
- Branch: `codex/agent-lifecycle-fact-source-review`
- Planning baseline commit: `ce74bea5 chore(trellis): 启动 Agent 生命周期边界维护任务`
- Collaboration channel: `agent-lifecycle-boundary`
- Channel path: `C:\Users\Syan\.trellis\channels\F--Projects-AgentDash\agent-lifecycle-boundary`

## Workflow Recovery

If context is compacted, restore the working state in this order:

1. Run `python ./.trellis/scripts/task.py current --source` and confirm the active task above.
2. Re-read `prd.md`, `design.md`, `implement.md`, `implement.jsonl`, `check.jsonl`, and this file.
3. Inspect the channel with `trellis channel messages agent-lifecycle-boundary --raw --last 80`.
4. Check worker completion with `trellis channel wait agent-lifecycle-boundary --as codex-main --from "impl-wait-gate,impl-control-effects,impl-protocol-frontend" --kind "done,error" --all --timeout 1m`.
5. Review `git status --short` before touching files. Do not overwrite worker changes.
6. Commit each completed work package independently.

## Active Workers

### `impl-wait-gate`

- Scope: WP2 Wait/Gate Typed Payload.
- Owns: typed `LifecycleGate.payload_json` wait policy envelope, generic producer terminal convergence naming, repository lookup path semantics.
- Avoids: AgentRun control-effect outbox, RuntimeSession effect migration, frontend refresh/protocol work.
- Status: completed at channel seq `7734`.
- Final report: implemented typed `GateWaitPolicyEnvelope`, generic gate producer terminal convergence, envelope-owned repository lookup paths, and companion adapter formatting boundary.
- Reported verification: `cargo fmt`, `cargo test -p agentdash-domain gate_wait_policy`, `cargo test -p agentdash-application-workflow`, `cargo check -p agentdash-application`, `agentdash-infrastructure`, `agentdash-api`, and no-run compilation for `agentdash-application`, `agentdash-application-agentrun`, `agentdash-infrastructure`.

### `impl-control-effects`

- Scope: WP3 Session Residue Excision and WP4 AgentRun Control-Plane Effects.
- Owns: moving `hook_effects`, `hook_auto_resume`, and `session_terminal_callback` replay away from RuntimeSession naming/ownership into AgentRun control-effect boundaries.
- Avoids: wait/gate typed envelope and frontend refresh mapping unless required for compile.
- Status: killed after two waits and one directed status request produced no `message`, `done`, or `turn_finished`.
- Last usable state: partial final-answer stream indicated SPI/infrastructure/runtime naming moved toward `AgentRunControlEffect*`; static review found business replay still owned by `agentdash-application-runtime-session/src/session/terminal_effects.rs`.
- Follow-up worker: `repair-control-effects`, spawned by `codex-main`, owns the remaining WP3/WP4 cleanup.
- Follow-up status: spawn returned process metadata, but the targeted brief was recorded as `undeliverable` with `worker-unknown`; main session must own the remaining cleanup unless a later agent is spawned successfully.

### `impl-protocol-frontend`

- Scope: WP5 Projection Invalidation Event and WP6 Frontend Boundary.
- Owns: `ControlPlaneProjectionChanged`, generated TS protocol path, `controlPlaneModel` refresh planning, terminal store stream-scoped dedup.
- Avoids: AgentRun control-effect outbox and wait/gate envelope changes.
- Status: killed after two waits and one directed status request produced no `message`, `done`, or `turn_finished`.
- Last usable state: partial final-answer stream indicated protocol/generated TS/controlPlaneModel/terminal-store edits; static review found API route waiting-row injection still present.
- Follow-up worker: `repair-projection-frontend`, spawned by `codex-main`, owns remaining WP5/WP6 and API waiting-row cleanup.
- Follow-up status: spawn returned process metadata, but the targeted brief was recorded as `undeliverable` with `worker-unknown`; main session must own the remaining cleanup unless a later agent is spawned successfully.

## Main Session Recovery Notes

- A 20 minute wait followed by a 10 minute wait completed with only `impl-wait-gate` reporting `done`.
- Main session sent a directed status request to `impl-control-effects` and `impl-protocol-frontend`; both remained silent for an additional 5 minute window.
- Main session killed the two silent workers before spawning repair workers to prevent concurrent writes.
- Repair worker spawn attempts for `repair-control-effects` and `repair-projection-frontend` did not become deliverable workers; do not wait on them during recovery.
- Main session committed WP2 wait/gate envelope as `430582dd`.
- Main session committed WP5/WP6 projection/frontend/API waiting-row cleanup as `50b597b9`.
- Current remaining WP3/WP4 boundary:
  - AgentRun control-effect store/model/migration changes are verified and ready as an incremental data-model slice.
  - `RuntimeSession` still owns AgentRun/Hook control-effect replay in `crates/agentdash-application-runtime-session/src/session/terminal_effects.rs`; this is not considered fully clean.
  - `SessionTerminalCallback` fanout still exists in API/bootstrap/runtime callback wiring and must be replaced by an AgentRun control-effect intake in the next cleanup slice.

## Commit Slicing

- Commit planning/task tracking separately from implementation.
- Commit WP2 wait/gate changes independently.
- Commit WP3/WP4 Session residue and AgentRun control-effect changes independently; split again if migration/model and executor rewiring are separable.
- Commit WP5/WP6 protocol/frontend changes independently after contract generation and focused frontend tests.
- Run check agent after each coherent implementation slice before committing the slice.
