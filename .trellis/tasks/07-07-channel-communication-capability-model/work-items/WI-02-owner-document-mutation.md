# WI-02 Owner Document Mutation Contract

Status: planned
Owner: implement worker
Depends On: WI-01 mutation type shape
Can Run With: WI-04, WI-09
Expected Commit: `feat(database): 增加 owner document mutation 与 LifecycleRun registry`

## Scope

- Add narrow typed JSONB row-lock helper in infrastructure.
- Helper handles transaction, `SELECT ... FOR UPDATE`, typed decode, domain mutation, target document writeback, error context.
- Keep table/column binding inside repository-specific methods.
- Ensure broad aggregate update preserves independent owner document columns.

## Exit Criteria

- Helper has tests or repository-backed coverage.
- Application/domain only see semantic mutation ports.
- `LifecycleRunRepository::update` does not write `channel_registry` once WI-03 lands.

## Targeted Checks

```powershell
cargo check -p agentdash-infrastructure
```

## Progress Log

- initialized
