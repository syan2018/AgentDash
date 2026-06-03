# Lifecycle 控制面最终收束 Closeout

## Verdict

Merge-ready from the Lifecycle control-plane closeout perspective.

No blocking residual was found for session-first ownership, old lifecycle step surfaces, hook runtime ownership, active projection, run-level port output APIs, or generated contract drift.

## Completed Closeout Work

- Archived completed frame convergence work:
  - `06-02-scoped-lifecycle-artifacts`
  - `06-02-lifecycle-run-active-projection-structure`
  - `06-02-lifecycle-control-plane-frame-convergence`
- Completed Session Runtime Head cleanup in `06-03-database-business-semantic-convergence`.
- Updated specs to current target names:
  - AgentFrame hook runtime
  - AgentProcedure contract
  - activity-level lifecycle edge vocabulary
  - FrameLaunchEnvelope session launch boundary

## Non-Blocking Follow-Ups

`06-03-database-business-semantic-convergence` remains active for database/business semantic cleanup that is not blocking this PR:

- `lifecycle_runs.execution_log` audit-owner cleanup.
- `views` / `user_preferences` scoped UI/settings cleanup.
- `stories.task_count` projection cleanup.
- `project_agents.is_default_for_task` dispatch policy cleanup.
- backend local identity/share/claim and typed permission/credential cleanup.

## Validation

- `cargo fmt`
- `git diff --check`
- `cargo check --workspace`
- `cargo test -p agentdash-infrastructure`
- `cargo test -p agentdash-application workflow`
- `cargo test -p agentdash-application hooks`
- `cargo test -p agentdash-application vfs::provider_lifecycle`
- `pnpm run contracts:check`
- `pnpm --filter app-web run typecheck`
- `pnpm --filter app-web test`

## PR Summary Anchor

The PR should describe the final target as:

```text
LifecycleRun -> LifecycleAgent -> AgentFrame -> AgentAssignment -> RuntimeSession trace
```

and call out that remaining database semantics cleanup is tracked separately and does not represent runtime control-plane ownership drift.
