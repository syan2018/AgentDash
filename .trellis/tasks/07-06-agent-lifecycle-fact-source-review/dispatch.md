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
3. Prefer platform-native `spawn_agent` / `wait_agent` for new sub-agent work. Trellis channel history remains useful for old worker reports only.
4. Inspect the old channel only when reconstructing earlier worker status: `trellis channel messages agent-lifecycle-boundary --raw --last 80`.
5. Check native sub-agent `019f38a9-03cf-7352-a021-eac294a178bd` if it is still open.
6. Review `git status --short` before touching files. Do not overwrite worker changes.
7. Commit each completed work package independently.

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
- Main session committed WP3/WP4 AgentRun control-effect persistence model as `346c1573`.
- Main session committed mailbox projection naming cleanup as `0956e219`.
- Native sub-agent dispatch:
  - `019f38a9-03cf-7352-a021-eac294a178bd` (`trellis-implement`, nickname `Chandrasekhar`) owns final WP3/WP4 RuntimeSession business replay externalization.
  - Dispatch prompt required reading task artifacts/spec manifests, preserving the existing `AgentRunControlEffectStore` / `0053_agent_run_control_effects` model, and not committing.
  - This implement agent timed out twice and was closed without usable output; main session owns the remaining repair.
  - `019f38c3-5a8e-73e2-b315-02c0faa90cec` (`trellis-check`, nickname `Poincare`) ran a read-only check after commit `9d62140b` and reported three blocking findings.
  - `019f38df-dead-75a3-8f94-08cc2bebf7f5` (`trellis-check`, nickname `Schrodinger`) completed read-only post-fix review for commit `4e8bf9ac`; no blocking findings remained for the migration quote fix, AgentRun-owned terminal hook trigger, or typed workspace module presentation projection.
  - `019f38e6-c556-7743-9ee7-11d2c3ccf9d7` (`trellis-check`, nickname `Nietzsche`) completed read-only backend/session residue audit for WP2/WP3/WP4/R1/R2/R4 with no blocking findings.
  - `019f38e7-04d1-75d1-8593-13660b3bf41a` (`trellis-check`, nickname `Averroes`) completed read-only frontend/protocol projection audit and found blocking companion legacy UI protocol residue.
  - `019f38f6-fe6e-7eb2-b32a-65766bfcd53d` (`trellis-check`, nickname `Zeno`) completed read-only review for the companion legacy UI protocol cleanup with no blocking findings.
- Current remaining WP3/WP4 boundary:
  - AgentRun control-effect executor/intake implementation exists in `crates/agentdash-application-agentrun/src/agent_run/control_effects.rs` and must be committed with this cleanup slice.
  - RuntimeSession terminal path now uses `RuntimeTerminalBoundaryService` / `RuntimeTerminalBoundaryEvidence` and hands evidence to `AgentRunControlEffectPort`; `terminal_effects.rs` and `effects_service.rs` have been deleted.
  - API/bootstrap now wires `AgentRunControlEffectService` plus `ApiWaitProducerTerminalConvergenceAdapter` and `ApiLifecycleTerminalConvergenceAdapter`; the `SessionTerminalCallback` composite fanout is gone.
  - `SessionStoreSet` exposes the outbox store as `control_effects`, not `terminal_effects`, so the composition root no longer presents the AgentRun outbox as Session-owned.
  - Static grep after this slice should only find historical migration names and task planning text for `runtime_session_terminal_effects` / `SessionTerminalEffectStore`; product code should not contain `SessionEffectsService`, `terminal_effects`, `SessionTerminalCallback`, or `TerminalEffectType`.
- Check-agent blocking findings and resolution:
  - `0053_agent_run_control_effects.sql` used an invalid dollar quote in the PL/pgSQL block; fixed to `DO $$ ... END $$`.
  - RuntimeSession terminal boundary still triggered terminal hook effects directly; moved terminal hook trigger and BeforeStop continue detection into AgentRun control-effect service through an AgentRun-scoped trigger port. RuntimeSession now only passes live hook runtime evidence.
  - Workspace module successful presentation still emitted `SessionMetaUpdate { key = "workspace_module_presented" }`; changed it to typed `ControlPlaneProjectionChanged` with `projection = resource_surface` and `reason = workspace_module_presented`.
- Remaining non-blocking audit items:
  - `MailboxWakeDelivery` and `HookRuntimeProjectionChanged` executor branches are currently no-op when replayed, but no production producer was found in this slice.
  - `MailboxStateChanged` and `SessionMetaUpdate` remain protocol/trace/feed concepts; they must not become AgentRun workspace refresh authority again.
- Companion legacy UI protocol cleanup:
  - Averroes found `SessionCompanionRequestCard` kept local responded state and `SessionSystemEventCard` rendered `companion_human_request` from legacy `SessionMetaUpdate`.
  - Main session removed companion-specific `SessionMetaUpdate` notification helpers and stopped `CompanionRequestTool` / concrete companion gate delivery from injecting companion UI events into RuntimeSession feed.
  - Main session removed `SessionCompanionRequestCard` and the frontend `companion_*` renderable system-event allowlist; companion request status is now expected from AgentRun workspace `conversation.mailbox.waiting_items`.
  - Main session renamed the concrete companion gate notification delivery to `CompanionGateProjectionDelivery` and removed the remaining API/app-state construction dependency on companion `SessionEventingService`; AgentRun mailbox delivery still uses runtime services only as the execution bridge for actual mailbox delivery.
  - Zeno verified that remaining companion strings are test assertions or LifecycleGate kind values, not RuntimeSession `SessionMetaUpdate` or frontend session-system-event product paths.
- Post-fix verification:
  - Main session passed `cargo check -p agentdash-api`, `cargo test -p agentdash-application-runtime-session`, `cargo test -p agentdash-application-agentrun`, `cargo test -p agentdash-application-workflow`, `cargo test -p agentdash-api --no-run`, `cargo test -p agentdash-workspace-module --no-run`, `node scripts/check-migration-history.js`, and `pnpm run contracts:check`.
  - Schrodinger additionally passed focused runtime-session/workspace-module checks, `cargo check -p agentdash-application-agentrun`, `cargo check -p agentdash-api`, `pnpm --filter app-web test -- controlPlaneModel`, and `pnpm run frontend:check`.
  - Companion cleanup slice passed `cargo check -p agentdash-application -p agentdash-api`, `cargo check -p agentdash-api`, `cargo test -p agentdash-application companion`, `pnpm --filter app-web test -- SessionSystemEventCard systemEventPolicy`, `pnpm run frontend:check`, `pnpm run contracts:check`, and `git diff --check`.

## Commit Slicing

- Commit planning/task tracking separately from implementation.
- Commit WP2 wait/gate changes independently.
- Commit WP3/WP4 Session residue and AgentRun control-effect changes independently; split again if migration/model and executor rewiring are separable.
- Commit WP5/WP6 protocol/frontend changes independently after contract generation and focused frontend tests.
- Run check agent after each coherent implementation slice before committing the slice.
