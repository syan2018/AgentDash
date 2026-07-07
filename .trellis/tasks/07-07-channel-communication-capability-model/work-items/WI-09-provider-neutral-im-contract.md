# WI-09 Provider-neutral IM Contract

Status: done
Owner: implement worker
Depends On: WI-01, WI-04
Can Run With: WI-02, WI-06
Expected Commit: `feat(channel): 接入 ChannelOwnerStore 与 ChannelService`

## Scope

- Define external IM binding envelope: workspace, room, thread, user, message refs.
- Define inbound event -> normalized `ChannelMessage` shape.
- Define outbound publish outbox intent shape.
- Keep Slack / Feishu / Teams adapters out of this task.
- Keep Project Channel Asset physical storage out of this task.

## Exit Criteria

- Provider-neutral DTOs compile and have validation tests.
- Binding unresolved / unsupported behavior is explicit.
- No concrete provider adapter code is introduced.

## Targeted Checks

```powershell
cargo test -p agentdash-domain im
cargo test -p agentdash-application binding
```

## Progress Log

- initialized
- candidate implementation exists in workspace with ChannelService application batch
- implemented provider-neutral inbound envelope, binding resolution outcome, explicit unsupported/unresolved behavior and provider-neutral publish outbox intent
- targeted checks were run by host and must be verified by native check worker before this item can move forward: `cargo test -p agentdash-application channel`; `cargo check -p agentdash-application`
- native check worker `Ohm` completed WI-10 full-scope check; provider-neutral contract verification passed
- dispatcher integration review passed; application channel test passed
