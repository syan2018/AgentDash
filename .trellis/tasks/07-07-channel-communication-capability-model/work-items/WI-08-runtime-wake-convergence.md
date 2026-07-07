# WI-08 Companion/SubAgent/Human/Async Wake Convergence

Status: planned
Owner: implement worker
Depends On: WI-05, WI-07
Can Run With: none
Expected Commit: `feat(companion): 收束 runtime wake 到 ChannelService`

## Scope

- Route `companion_request` / `companion_respond` through ChannelService.
- `target=sub` creates runtime channel, participants, reply address, first delivery intent.
- Route child result, parent request/response, human response through ChannelService materialization.
- Route terminal / exec / async producer wake through `ChannelMessage` + delivery intent.
- Keep Platform broker missing diagnostic unless durable broker fact exists.

## Exit Criteria

- Integration tests cover companion request/respond, SubAgent result, human response, terminal/exec wake.
- Static scan confirms old direct delivery calls only remain in materializer/resolver boundaries.

## Targeted Checks

```powershell
cargo test -p agentdash-application companion
cargo test -p agentdash-application-agentrun subagent
rg -n "accept_intake_message|LifecycleGateResolver|GateDeliveryIntent" crates/agentdash-application*
```

## Progress Log

- initialized
