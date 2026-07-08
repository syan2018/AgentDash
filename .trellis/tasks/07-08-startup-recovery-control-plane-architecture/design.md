# Design: Startup Recovery Control Plane Deepening

## Problem Statement

The codebase now has the correct recovery behavior, but the interface is shallow: callers across API bootstrap, boot reconcile, RuntimeSession terminal processing, AgentRun control effects, and mailbox wake delivery still need to know recovery ordering and target-state rules. The refactor should make those rules local to a small set of deep modules.

## Target Modules

### 1. StartupRecoveryPipeline

Interface sketch:

```rust
pub struct StartupRecoveryPipelineReport {
    pub phases: Vec<StartupRecoveryPhaseReport>,
}

pub enum StartupRecoveryPhase {
    RuntimeSessionRecovery,
    AgentRunDeliveryConvergence,
    GateWaitPolicyFallback,
    TaskProjection,
    BackgroundTerminalSideEffectReplay,
}

pub async fn run_startup_recovery(
    deps: StartupRecoveryPipelineDeps,
) -> StartupRecoveryPipelineReport;
```

Responsibilities:

- Run RuntimeSession recovery with terminal effect mode constrained to delivery convergence.
- Drain AgentRun delivery convergence to quiescence or a configured cap.
- Run gate wait policy fallback only after delivery convergence.
- Start background terminal side-effect replay only after app readiness.
- Emit one coherent diagnostic report.

Non-responsibilities:

- Do not own RuntimeSession terminal event persistence.
- Do not own AgentRun delivery binding transition internals.
- Do not own mailbox message materialization.

### 2. ControlEffectReplayPlan

Interface sketch:

```rust
pub enum ControlEffectReplayIntent {
    StartupFactConvergence,
    TerminalSideEffects,
    BackgroundMaintenance,
}

pub struct ControlEffectReplayRequest {
    pub intent: ControlEffectReplayIntent,
    pub batch_limit: u32,
    pub max_batches: Option<usize>,
}

pub async fn replay_control_effects(
    request: ControlEffectReplayRequest,
) -> Result<ControlEffectReplayReport, String>;
```

Responsibilities:

- Hide phase selection from bootstrap callers.
- Keep delivery-first semantics inside AgentRun control-effect module.
- Preserve stale running lease reclaim and dead-letter semantics.
- Maintain compatibility for existing tests while moving callers to intent-level methods.

### 3. MailboxWakeAdmission

Interface sketch:

```rust
pub enum MailboxWakeSource {
    UserMessage,
    CompanionParentWake,
    HookAutoResume,
    Routine,
    ChannelDelivery,
}

pub enum MailboxWakeDecision {
    AllowLaunch,
    AllowResumeSource,
    AllowQueueOnly,
    Skip { reason: &'static str },
    Reject { reason: String },
}

pub fn decide_mailbox_wake(
    source: MailboxWakeSource,
    delivery: &DeliveryRuntimeSelection,
    runtime_state: &SessionExecutionState,
) -> MailboxWakeDecision;
```

Responsibilities:

- Centralize target delivery binding + RuntimeSession execution state interpretation.
- Make source semantics explicit.
- Let companion terminal target return `Skip { reason: "terminal_target" }`.
- Let hook auto-resume keep its durable effect identity semantics while still checking target consistency.
- Give scheduler and intake paths the same decision vocabulary.

### 4. Launch Delivery Semantics Review

This slice evaluates whether `MailboxDelivery::LaunchOrContinueTurn` should be split or wrapped.

Potential outcomes:

- Keep enum unchanged but require `MailboxWakeAdmission` before scheduling.
- Add source-aware launch intent metadata to mailbox messages.
- Split `LaunchOrContinueTurn` into source-specific variants only if the admission model cannot express the needed distinction cleanly.

## Similar Coupling Inclusion Rules

When implementation uncovers another coupling, include it only if it can be expressed as one of the target module interfaces above. Otherwise record it in `research/follow-up-couplings.md` with evidence and leave it out.

Examples:

- Include: another caller that directly invokes `replay_control_effect_outbox_phase` to enforce startup order.
- Include: another path that calls `inspect_session_execution_state` and separately checks delivery binding to decide launch.
- Exclude: clippy warnings, naming cleanup, or frontend DTO polish.

## Testing Surface

- Unit tests for `ControlEffectReplayIntent` mapping to effect kinds and batch behavior.
- Unit tests for `MailboxWakeAdmission` decision matrix.
- Service-level tests for `StartupRecoveryPipeline` phase order.
- Existing companion, hook auto-resume, mailbox scheduler, and boot reconcile tests should migrate to the new decision module rather than duplicating guards.
- Smoke test with embedded PostgreSQL bad-state fixture or dev database startup path.

## Migration Strategy

No database migration is expected. Refactor should preserve public behavior and durable schema. Internal Rust interfaces can change freely because the project is pre-release.

## Risks

- A too-broad `StartupRecoveryPipeline` could become a coordinator that merely delegates without improving locality. Keep its interface small and test through phase reports.
- Over-splitting `MailboxDelivery` could create churn without removing caller knowledge. Prefer source-aware admission first.
- Moving replay semantics behind intent methods may hide useful test hooks. Keep test-only or internal phase assertions near AgentRun control-effect implementation.
