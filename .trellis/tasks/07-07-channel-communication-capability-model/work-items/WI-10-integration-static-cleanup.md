# WI-10 Integration, Static Checks, Cleanup

Status: planned
Owner: dispatcher + check worker
Depends On: WI-01, WI-02, WI-03, WI-04, WI-05, WI-06, WI-07, WI-08, WI-09
Can Run With: none
Expected Commit: `test(channel): 补齐 Channel 全链路验证`

## Scope

- Run full affected-package checks.
- Run static scans from `implement.md`.
- Check migration shape and absence of channel standalone tables.
- Verify task acceptance criteria.
- Update work-item statuses and prepare commit plan.

## Exit Criteria

- All targeted checks from WI-01 through WI-09 are either green or replaced by broader green commands.
- `pnpm run migration:guard` passes.
- Static scans match expected result.
- `git status --porcelain` is classified into task-owned vs unrecognized files.
- Commit plan drafted but not executed without user confirmation.

## Targeted Checks

```powershell
pnpm run migration:guard
cargo check -p agentdash-domain -p agentdash-infrastructure -p agentdash-spi -p agentdash-application -p agentdash-application-agentrun
rg -n "CREATE TABLE .*channel|channel_participants|channel_bindings" crates/agentdash-infrastructure/migrations
rg -n "LifecycleChannel" crates
rg -n "list_all\(|list_by_project\(|scan.*LifecycleRun" crates
rg -n "accept_intake_message|LifecycleGateResolver|GateDeliveryIntent" crates/agentdash-application*
```

## Progress Log

- initialized
