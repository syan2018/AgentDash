# WI-08 Companion/SubAgent/Human/Async Wake Convergence

Status: done
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
- candidate implementation exists in workspace for companion/subagent/human/terminal wake paths
- Companion dispatch/result/parent request/parent response/human response now ensure owner-local runtime channels and pass through ChannelService mailbox materialization before mailbox scheduling
- terminal hook auto-resume now builds ChannelMessage/ChannelDeliveryIntent refs in mailbox source metadata while preserving mailbox input payload shape
- targeted checks were run by host and must be verified by native check worker before this item can move forward: `cargo test -p agentdash-application companion`; `cargo test -p agentdash-application channel`; `cargo test -p agentdash-application-agentrun hook_auto_resume`; `cargo check -p agentdash-domain -p agentdash-application -p agentdash-application-agentrun`
- `cargo test -p agentdash-application-agentrun subagent` completed with no matching tests and must not be treated as coverage
- native check worker `Ohm` completed WI-10 full-scope check; runtime wake convergence verification passed
- dispatcher integration review passed; static delivery path classification and affected-package cargo check passed
