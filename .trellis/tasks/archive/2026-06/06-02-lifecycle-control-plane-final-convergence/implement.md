# Lifecycle 控制面最终收束 Implement Plan

## Dispatch Scope

This task is a final review / closeout target, not an implementation work package. Dispatch it only after the remaining implementation tasks have either completed or been explicitly classified as non-blocking for the PR merge.

## Recommended Order Before PR Merge

1. `06-03-database-business-semantic-convergence` remaining review.
   - Session Runtime Head is complete; `active_node_keys` is removed through the active projection task.
   - `lifecycle_runs.execution_log` remains as audit-owner cleanup. Treat it as non-blocking if final review confirms it is read-model/audit only and not runtime control fact source.
2. `06-01-lifecycle-control-plane-concept-alignment` final review — closeout review.
   - Run after implementation blockers are done or explicitly marked non-blocking.
3. This task final closeout — closeout review.
   - Run last to decide whether the PR is merge-ready.

## Can Move Later

- `06-03-database-business-semantic-convergence` UI/settings cleanup can move later if `views` / `user_preferences` do not affect the lifecycle control-plane merge.
- `06-03-database-business-semantic-convergence` backend local identity/share/claim cleanup can move later unless it blocks runtime anchor or session shell correctness.
- `06-03-database-business-semantic-convergence` typed cleanup for LLM credential naming, permission JSONB queries and canvas binding source can move later if contracts remain coherent.
- New companion persistence, lifecycle branching/fork-join and broader product navigation changes are follow-up work, not this closeout.

## Checklist

- [ ] Confirm remaining task statuses:
  - `06-03-database-business-semantic-convergence`
  - `06-01-lifecycle-control-plane-concept-alignment`
- [ ] Run final residual scans listed in `prd.md`.
- [ ] Review generated contracts and frontend usage for session-first/runtime-control drift.
- [ ] Review migration baseline for lingering wrong fact ownership.
- [ ] Review `.trellis/spec/` for target invariant drift.
- [ ] Produce closeout summary with:
  - archived tasks
  - remaining non-blocking follow-ups
  - blocking findings, if any
  - validation commands and results
- [ ] Archive this task only after closeout summary is accepted.

## Validation Commands

- [ ] `cargo check --workspace`
- [ ] `cargo test --workspace`
- [ ] `pnpm run contracts:check`
- [ ] `pnpm --filter app-web run typecheck`
- [ ] Focused app-web tests touched by lifecycle/session runtime surfaces.

## Review Gate

- [ ] No remaining active June lifecycle task is stale because of Trellis metadata only.
- [ ] Every remaining active task has either an executable implementation plan or a final-review purpose.
- [ ] Merge blockers are concrete file-level findings, not broad conceptual concerns.
