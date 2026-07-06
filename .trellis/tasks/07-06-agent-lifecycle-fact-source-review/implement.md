# Agent 生命周期模块边界维护执行计划

## Work Packages

### WP1. Planning and Specs

- Keep this task in `planning` until the user reviews `prd.md`, `design.md`, `implement.md`, `implement.jsonl`, and `check.jsonl`.
- Do not run `task.py start` until review is accepted.
- Before implementation, load specs listed in `implement.jsonl` with `trellis-before-dev`.

### WP2. Wait/Gate Typed Payload

- Add domain-owned typed wait policy envelope for `LifecycleGate.payload_json`.
- Migrate current `WaitObligationDeclaration` API to the typed envelope.
- Update gate repository wait policy lookups to use envelope-owned path semantics.
- Rename workflow/application generic convergence types away from `wait_obligation` and companion-specific names.
- Keep companion-specific formatting in companion adapter only.

### WP3. Session Residue Excision

- Remove RuntimeSession ownership from business effect replay:
  - `agentdash-application-runtime-session/src/session/terminal_effects.rs`
  - `agentdash-spi::session_persistence::SessionTerminalEffectStore`
  - `TerminalEffectType::{HookEffects, HookAutoResume, SessionTerminalCallback}`
  - terminal effect replay background worker wiring.
- Replace `SessionTerminalCallback` composite fanout with an AgentRun control-effect intake. RuntimeSession terminal processing should persist terminal evidence, clear active turn, and hand evidence to the AgentRun control-plane port.
- Remove AgentRun workspace refresh dependencies on legacy session event keys:
  - `companion_dispatch_registered`
  - `companion_result_available`
  - `companion_result_returned`
  - `companion_human_request`
  - `companion_human_response`
  - `companion_review_request`
  - `session_meta_updated`
  - `mailbox_state_changed`
  - `workspace_module_presented`
  - capability/context frame refresh keys encoded as `SessionMetaUpdate`.
- Remove API route-level exec waiting row injection (`append_exec_terminal_waiting_items`) from the final state. Exec waiting rows must come from AgentRun wait/activity or terminal activity projection.
- Replace terminal-store durable event dedup with `{stream_identity}:{event_seq}` and update dispatcher/tests to pass the stream identity explicitly.

### WP4. AgentRun Control-Plane Effects

- Create AgentRun/control-plane owned effect outbox with a `0053_*` migration, preferably `agent_run_control_effects`.
- Durable records are scoped by `run_id + agent_id + frame_id`; `delivery_runtime_session_id`, `turn_id`, and `terminal_event_seq` are trace evidence only.
- Extend typed effect kind parsing with:
  - `agent_run_delivery_convergence`
  - `wait_producer_terminal_convergence`
  - `lifecycle_terminal_convergence`
  - `mailbox_wake_delivery`
  - `hook_effects`
  - `hook_auto_resume_delivery`
  - `hook_runtime_projection_changed`
- Move replay/executor ownership into AgentRun/control-plane application code.
- Ensure replay and boot reconcile use the same executor path.
- Keep `AgentRunDeliveryBinding` as the only user-visible running/terminal write source.
- Keep Hook policy/effect ownership under AgentRun / AgentFrame control target, never under RuntimeSession.

### WP5. Projection Invalidation Event

- Add `ControlPlaneProjectionChanged` to Backbone platform protocol.
- Emit generic projection invalidation from mailbox state changes, wait/gate resolution, delivery terminal convergence, companion result paths, hook runtime/effect changes, workspace module presentation, capability/context frame changes, title/list changes, and resource surface invalidation.
- Projection enum must cover at least `workspace`, `agent_run_list`, `mailbox`, `waiting`, `delivery`, `hook_runtime`, `resource_surface`, and `title`.
- Delete companion-specific and SessionMetaUpdate-based projection / refresh event emission and frontend refresh branches in the same work package.
- Regenerate/check TypeScript contracts.

### WP6. Frontend Boundary

- Update `controlPlaneModel` to use `ControlPlaneProjectionChanged` for refresh planning.
- Remove companion-specific event keys, `session_meta_updated`, `mailbox_state_changed`, and `workspace_module_presented` free keys as workspace refresh authority. Do not keep a legacy display refresh path.
- Update companion request card to refresh snapshot after submit and rely on backend waiting/gate projection for final status.
- Scope terminal store dedup by stream identity.

### WP7. Verification

- Run focused Rust tests for workflow gate convergence, AgentRun control-effect convergence, hook auto-resume replay, and API wiring.
- Run frontend focused tests for control plane model, companion card boundary, and terminal store.
- Run `pnpm run contracts:check`.

## Validation Commands

- `cargo test -p agentdash-application-workflow`
- `cargo test -p agentdash-application-agentrun`
- `cargo test -p agentdash-application-runtime-session`
- `cargo test -p agentdash-api`
- `pnpm run contracts:check`
- Static grep checks:
  - no AgentRun workspace refresh branch depends on `companion_*` / `session_meta_updated` / `mailbox_state_changed` / `workspace_module_presented` free event keys.
  - no RuntimeSession module owns `hook_effects`, `hook_auto_resume`, or AgentRun terminal convergence replay.
  - no API route appends exec waiting rows after workspace snapshot construction.
- Frontend focused test command should target existing test runner for:
  - `controlPlaneModel.test.ts`
  - `sessionPlatformEventDispatcher.test.ts`
  - `useTerminalStore.test.ts`

## Risk Points

- AgentRun/control-plane effect replay can duplicate mailbox wake or hook auto-resume unless idempotency keys are preserved.
- Migrating out of `runtime_session_terminal_effects` touches runtime-session persistence, background replay workers, bootstrap wiring, hook effect replay, and terminal callback failure retry; all references must move to the new AgentRun/control-plane naming.
- Gate payload parser must preserve existing companion display metadata while moving generic policy fields under typed envelope.
- Backbone generated contract changes affect many frontend imports; generated files must be produced by the contract generator, not hand-edited.
- Frontend must not treat projection invalidation event payload as business state.
- The cleanup phase can expose hidden Session dependencies. Treat those as scope discoveries, not compatibility blockers; the project is pre-release and old paths should be removed cleanly.

## Before `task.py start`

- User reviews final planning artifacts.
- `implement.jsonl` and `check.jsonl` contain real entries and no `_example` seed rows.
- Decide implementation subagent split:
  - backend wait/gate + AgentRun control effects
  - Session residue excision + hook ownership
  - protocol/contracts + frontend refresh
  - check/review agent
