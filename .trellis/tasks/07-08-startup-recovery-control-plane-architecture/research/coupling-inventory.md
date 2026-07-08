# Coupling Inventory

## Source

This inventory is based on the `startup-recovery-control-effect-replay` fix committed as `9bdf0bdfa`, the updated session runtime specs, and targeted source inspection on 2026-07-08.

## Inclusion Rule

Include a coupling point only when it shares one of these control-plane invariants:

- startup recovery must converge facts before creating new work;
- RuntimeSession execution state and AgentRun delivery binding must be interpreted together;
- terminal side-effect replay must respect delivery-first ordering;
- mailbox wake / launch eligibility must be source-aware and target-state-aware.

Couplings outside these invariants should be recorded as follow-up candidates, not folded into this refactor.

## Included Coupling Points

### C1. Startup Recovery Phase Assembly Is Split Across Bootstrap Layers

Evidence:

- `crates/agentdash-api/src/app_state.rs:403` calls `reconcile::boot::run_boot_reconcile`.
- `crates/agentdash-api/src/app_state.rs:526` calls `start_post_app_state_workers`.
- `crates/agentdash-application/src/reconcile/boot.rs:90` owns boot reconcile order.
- `crates/agentdash-api/src/bootstrap/background_workers.rs:20` owns post-AppState replay worker startup.
- `crates/agentdash-application-runtime-session/src/session/runtime_control.rs:159` owns `recover_interrupted_sessions`.

Assessment:

The startup recovery phase order is a real business invariant but is assembled by multiple callers. This is the strongest candidate for a deep module. A `StartupRecoveryPipeline` interface should let `AppState` say "run startup recovery" without knowing delivery convergence, gate fallback, or side-effect replay ordering.

### C2. Control-Effect Replay Intent Is Still Caller-Visible

Evidence:

- `crates/agentdash-application-agentrun/src/agent_run/control_effects.rs:831` exposes `replay_control_effect_outbox_phase`.
- `crates/agentdash-application-agentrun/src/agent_run/control_effects.rs:882` keeps the compatibility `replay_control_effect_outbox(limit)` entry.
- `crates/agentdash-application/src/reconcile/boot.rs:181` calls the delivery phase directly.
- `crates/agentdash-api/src/bootstrap/background_workers.rs:124` calls phase replay directly.

Assessment:

The phase enum fixed the immediate bug, but callers still need to know phase semantics. Refactor toward intent-level methods such as `drain_startup_safe_effects` and `replay_terminal_side_effects_after_delivery_quiescence`.

### C3. Mailbox Wake Admission Is Distributed Across Source Paths

Evidence:

- `crates/agentdash-application/src/companion/tools.rs:771` materializes companion mailbox delivery.
- `crates/agentdash-application/src/companion/tools.rs:906` inspects target RuntimeSession state.
- `crates/agentdash-application-agentrun/src/agent_run/mailbox/delivery.rs:342` accepts hook auto-resume effect.
- `crates/agentdash-application-agentrun/src/agent_run/mailbox/delivery.rs:654` repeats delivery/runtime state guard logic.
- `crates/agentdash-application-agentrun/src/agent_run/mailbox/policy.rs:52` defines `runtime_can_launch`.
- `crates/agentdash-application-agentrun/src/agent_run/mailbox/scheduler.rs:472` consumes `LaunchOrContinueTurn`.

Assessment:

The same underlying decision appears in companion, hook, policy, and scheduler code: given source intent + delivery binding + RuntimeSession execution state, decide allow / skip / reject. This belongs behind a `MailboxWakeAdmission` module.

### C4. `LaunchOrContinueTurn` Carries Multiple Source Semantics

Evidence:

- `crates/agentdash-application/src/channel.rs:550` uses `LaunchOrContinueTurn` for channel-to-mailbox delivery.
- `crates/agentdash-application-agentrun/src/agent_run/project_agent_start.rs:2298` uses it for first project-agent user input.
- `crates/agentdash-application/src/routine/executor.rs:358` accepts mailbox target command for routine delivery.
- `crates/agentdash-application-agentrun/src/agent_run/mailbox/policy.rs:32` maps running/cancelling user messages to `LaunchOrContinueTurn`.
- `crates/agentdash-application-agentrun/src/agent_run/mailbox/policy.rs:43` maps terminal/idle user messages to `LaunchOrContinueTurn`.

Assessment:

The enum name is operational rather than semantic. It can be kept internally, but source-specific launch eligibility should not be inferred from the enum alone. The architecture task should design whether to split the enum, add a launch admission source, or wrap it with a decision model.

### C5. Terminal Effect Mode Plumbing Is Wide

Evidence:

- `crates/agentdash-application-runtime-session/src/session/runtime_control.rs:187` passes `DeliveryConvergenceOnly`.
- `crates/agentdash-application-runtime-session/src/session/turn_processor.rs:58` carries `effect_mode`.
- `crates/agentdash-application-runtime-session/src/session/terminal_boundary.rs:21` carries `effect_mode` into AgentRun terminal control.
- `crates/agentdash-application-runtime-session/src/session/launch/commit.rs:288` and `connector_start.rs:82` explicitly pass `ImmediateAll`.
- `crates/agentdash-application-agentrun/src/agent_run/control_effects.rs:795` branches on mode.

Assessment:

The current fix is correct, but the mode is plumbing through several launch stages. This is a signal for a richer terminal command object or terminal recovery policy object that travels as a single typed intent.

## Excluded From This Task

- Workspace clippy debt found during the bugfix: `large_enum_variant`, `collapsible_if`, `too_many_arguments`. These are not recovery-control invariants.
- General frontend/backend contract cleanup. No frontend behavior is required for this refactor.
- Database schema reshaping. The architecture task may touch persistence adapters if interface changes require it, but schema changes are not a goal.

## Recommended Task Shape

This should be a single parent planning task with independently verifiable implementation slices:

1. Startup recovery pipeline module.
2. AgentRun control-effect replay intent interface.
3. Mailbox wake admission decision model.
4. Launch delivery semantics review and minimal split/wrapper if needed.

The first three slices can likely be implemented without changing product behavior. The fourth should be attempted only after the admission model shows exactly which source intents remain ambiguous.
