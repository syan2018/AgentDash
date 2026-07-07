# WI-07 Mailbox/Gate Materialization

Status: planned
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
