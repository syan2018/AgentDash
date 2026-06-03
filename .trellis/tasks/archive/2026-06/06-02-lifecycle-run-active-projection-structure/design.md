# LifecycleRun Active Projection Structure Design

## Target Projection

```rust
pub struct ActiveActivityRef {
    pub run_id: Uuid,
    pub graph_instance_id: Uuid,
    pub activity_key: String,
    pub attempt: Option<u32>,
    pub status: String,
}
```

Projection rule:

```text
WorkflowGraphInstance.activity_state.attempts
  -> Ready / Claiming / Running attempts
  -> ActiveActivityRef[]
  -> LifecycleRunView.active_activity_refs
```

`LifecycleRun.status` may still be aggregated from graph instance states, but active Activity identity should not be persisted as a run-level string list.

## Persistence Choice

Recommended target: derive active refs in the read builder from graph instance state. This avoids double-writing `lifecycle_runs.active_node_keys` and `lifecycle_workflow_instances.activity_state_json`.

If implementation keeps a temporary debug column, it must be named and documented as display/cache only. It must not be used by workflow advancement, completion, hook gates, or public route contracts.

## Public Exposure

Active runtime exposure belongs to Lifecycle / WorkflowGraphInstance / ActivityAttempt read models. Session-indexed endpoints may use `runtime_session_id` as an adapter key, but returned runtime state should remain Agent / Lifecycle anchored.

Target route contract relationship:

```text
runtime_session_id
  -> RuntimeSessionExecutionAnchor
  -> AgentFrameRuntimeView / LifecycleRunView
  -> ActiveActivityRef[] / ActivityAttemptRef
```

## Affected Areas

- `crates/agentdash-domain/src/workflow/entity.rs`
- `crates/agentdash-contracts/src/workflow.rs`
- `crates/agentdash-application/src/workflow/lifecycle_run_view_builder.rs`
- `crates/agentdash-application/src/workflow/tools/advance_node.rs`
- `crates/agentdash-application/src/workflow/lifecycle/journey/mod.rs`
- `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs`
- `crates/agentdash-infrastructure/migrations/0001_init.sql`
- `packages/app-web/src/generated/workflow-contracts.ts`
- frontend stores/components that display active Activity state

## Validation

- Unit: two graph instances with same activity key produce two distinct active refs.
- Contract: generated TS includes `active_activity_refs`.
- Backend: advancement/completion does not read `active_node_keys`.
- Frontend: active display uses structured graph/activity identity.
