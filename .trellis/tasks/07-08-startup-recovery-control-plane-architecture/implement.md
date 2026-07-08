# Implementation Plan

## Phase 0 - Planning Review

- [ ] Review `research/coupling-inventory.md` with the developer.
- [ ] Decide whether MVP includes the launch delivery semantics review as implementation or design-only.
- [ ] If scope grows into multiple deliverables, split child tasks before coding:
  - startup recovery pipeline
  - control-effect replay intent interface
  - mailbox wake admission
  - launch delivery semantics review

## Phase 1 - StartupRecoveryPipeline

- [ ] Add a startup recovery pipeline module under `agentdash-application::reconcile` or a more precise recovery namespace.
- [ ] Move boot recovery phase ordering out of API/bootstrap callers.
- [ ] Keep `AppState` responsible for composition only.
- [ ] Preserve diagnostics and phase report fields.
- [ ] Tests:
  - phase order is session recovery -> delivery convergence -> gate wait fallback -> task projection
  - delivery convergence drains to cap and stops on short batch
  - errors remain non-fatal and appear in report

## Phase 2 - Control-Effect Replay Intent Interface

- [ ] Add intent-level replay request/report types inside AgentRun control-effect surface.
- [ ] Move direct phase selection out of `reconcile::boot` and `background_workers`.
- [ ] Preserve low-level phase functions only as internal/test surface if needed.
- [ ] Tests:
  - startup intent claims only delivery convergence
  - background intent drains delivery before side effects
  - compatibility entry respects total `limit`
  - stale running lease reclaim still works for Postgres and memory adapters

## Phase 3 - MailboxWakeAdmission

- [ ] Add a decision module near AgentRun mailbox/admission surface.
- [ ] Encode source-aware decision matrix for user, companion parent wake, hook auto-resume, routine/channel delivery.
- [ ] Replace duplicated delivery/runtime state guards in companion and hook paths.
- [ ] Route scheduler launch eligibility through the same decision where feasible.
- [ ] Tests:
  - running binding + running/cancelling RuntimeSession allows active-target wake
  - running binding + terminal RuntimeSession rejects conflict
  - terminal binding + terminal RuntimeSession skips companion parent wake
  - hook auto-resume retains durable dedup behavior and target consistency guard
  - user explicit continue retains intended terminal-session launch behavior

## Phase 4 - Launch Delivery Semantics Review

- [ ] Inventory all `MailboxDelivery::LaunchOrContinueTurn` producers.
- [ ] Decide whether source-aware admission is enough.
- [ ] If not enough, introduce a typed launch source/admission field or split enum variants.
- [ ] Keep public API DTO behavior stable unless a stronger internal type requires DTO update.
- [ ] Tests:
  - project-agent start first message behavior unchanged
  - companion parent resume cannot launch terminal target from stale replay
  - routine/channel delivery paths still obey intended launch policy

## Phase 5 - Verification

- [ ] `cargo fmt`
- [ ] `git diff --check`
- [ ] `cargo check -p agentdash-api`
- [ ] `cargo test -p agentdash-application-agentrun agent_run::control_effects`
- [ ] `cargo test -p agentdash-application-agentrun agent_run::mailbox`
- [ ] `cargo test -p agentdash-application-runtime-session process_turn_terminal`
- [ ] `cargo test -p agentdash-application-workflow gate_wait_policy`
- [ ] `cargo test -p agentdash-application companion`
- [ ] `cargo test -p agentdash-api`
- [ ] Embedded PostgreSQL smoke: `node ./scripts/dev-runtime.js --profile web --skip-local --skip-frontend --server-port <free-port>`
- [ ] If migration files change unexpectedly: `pnpm run migration:guard`

## Review Gates

- [ ] No caller outside the recovery/control-effect modules needs to know startup phase ordering.
- [ ] No mailbox source path duplicates delivery binding + RuntimeSession state interpretation.
- [ ] Similar couplings discovered during implementation are either included by the rule or documented as follow-up.
- [ ] Behavior fixed by `9bdf0bdfa` remains green under the same smoke path.
