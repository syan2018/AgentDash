# WI-07 Mailbox/Gate Materialization

Status: done
Owner: implement worker
Depends On: WI-01, WI-05
Can Run With: WI-06
Expected Commit: `feat(channel): 打通 Mailbox/Gate materialization`

## Scope

- Add mailbox materializer: `ChannelDeliveryIntent -> AgentRunMailboxMessage`.
- Add gate materializer: `ChannelDeliveryIntent -> LifecycleGate ref / wait intent`.
- Add `ChannelAddress` -> `MailboxSourceIdentity` mapper.
- Preserve `mailbox.source.*` display key semantics in mailbox mapper, not in `ChannelAddress`.
- Do not copy gate payload, mailbox queue state, terminal output, or permission broker state into channel registry.

## Exit Criteria

- Materializer tests prove ownership boundaries.
- Address mapper tests cover namespace/kind/display attribution.
- Delivery intent planning does not mutate mailbox queue state directly.

## Targeted Checks

```powershell
cargo test -p agentdash-application channel_materializer
cargo test -p agentdash-domain channel_address
```

## Progress Log

- initialized
- candidate implementation exists in workspace for mapper and pure materialization commands
- implemented mailbox/gate materialization commands, address mapper, bounded payload refs, and correlation fallback
- targeted checks were run by host and must be verified by native check worker before this item can move forward: `cargo test -p agentdash-application channel`; `cargo test -p agentdash-domain channel_address` compiled with no matching tests
- native check worker `Ohm` completed WI-10 full-scope check; mapper ownership was cleaned up and materialization verification passed
- dispatcher integration review passed; application channel test passed
