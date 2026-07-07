# WI-03 LifecycleRun Registry Persistence

Status: planned
Owner: implement worker
Depends On: WI-01, WI-02
Can Run With: WI-06
Expected Commit: `feat(database): 增加 owner document mutation 与 LifecycleRun registry`

## Scope

- Extend `LifecycleRun` with `channel_registry: ChannelRegistryDocument`.
- Add migration for `lifecycle_runs.channel_registry jsonb DEFAULT '{}'::jsonb NOT NULL`.
- Update create/select mapping and typed JSONB roundtrip.
- Add `mutate_channel_registry(run_id, ChannelRegistryMutation)` repository/store path.
- Do not create `channels`, `channel_participants`, or `channel_bindings` tables.

## Exit Criteria

- Default/nonempty/shape-error roundtrip covered.
- Consecutive registry mutations do not lose data.
- Stale `LifecycleRunRepository::update` preserves registry.

## Targeted Checks

```powershell
pnpm run migration:guard
cargo test -p agentdash-infrastructure channel_registry
cargo test -p agentdash-infrastructure lifecycle_run
```

## Progress Log

- initialized
