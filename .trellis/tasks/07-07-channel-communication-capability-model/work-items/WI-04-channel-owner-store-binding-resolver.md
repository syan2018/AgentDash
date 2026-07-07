# WI-04 ChannelOwnerStore And BindingResolver

Status: planned
Owner: implement worker
Depends On: WI-01
Can Run With: WI-02, WI-09
Expected Commit: `feat(channel): 接入 ChannelOwnerStore 与 ChannelService`

## Scope

- Define `ChannelOwnerStore::load_registry` and `mutate_registry`.
- Add LifecycleRun-backed owner store adapter.
- Define provider-neutral `ChannelBindingResolver`.
- Return unresolved / unsupported when provider binding is not implemented.
- Do not scan Project/LifecycleRun to infer binding ownership.

## Exit Criteria

- Store contract handles LifecycleRun owner.
- Binding resolver tests prove unresolved / unsupported does not scan owners.
- Project owner store remains contract-only.

## Targeted Checks

```powershell
cargo test -p agentdash-application channel_owner
cargo check -p agentdash-application
```

## Progress Log

- initialized
