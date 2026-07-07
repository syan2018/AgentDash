# WI-05 ChannelService Core

Status: planned
Owner: implement worker
Depends On: WI-01, WI-04
Can Run With: WI-06前半, WI-07 mapper prep
Expected Commit: `feat(channel): 接入 ChannelOwnerStore 与 ChannelService`

## Scope

- Add application-level `ChannelService`.
- Support channel create/update/close, participants, bindings, policy updates.
- Normalize ingress, build `ChannelMessage`, plan delivery intents, record/prune delivery state.
- Return typed materialization commands without owning scheduler/gate state.
- Keep owner registry lazy-loaded by explicit owner ref.

## Exit Criteria

- Service tests cover LifecycleRun runtime channel creation through owner mutation.
- Project owner contract path does not depend on concrete ProjectConfig or Asset table.
- Static scan finds no startup/global owner scan.

## Targeted Checks

```powershell
cargo test -p agentdash-application channel_service
rg -n "list_all\(|list_by_project\(|scan.*LifecycleRun" crates
```

## Progress Log

- initialized
