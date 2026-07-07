# WI-01 Domain Channel Document Model

Status: planned
Owner: implement worker
Depends On: none
Can Run With: WI-02 research, WI-09 contract drafting
Expected Commit: `feat(channel): 建立 Channel 领域文档模型`

## Scope

- Add `agentdash-domain::channel`.
- Define `ChannelRegistryDocument`, `ChannelRegistryMutation`, `ChannelRecord`, `Channel`, `ChannelParticipant`, `ChannelBinding`, `ChannelPolicy`, `ChannelMessage`, `ChannelDeliveryIntent`, `ChannelDeliveryState`, `ChannelAddress`.
- Implement registry defaults, mutation apply, validation, bounded delivery-state prune.
- Avoid `LifecycleChannel` as a first-class model.

## Exit Criteria

- Domain types compile.
- Default empty registry roundtrips through serde.
- Mutation tests cover upsert, close, participant, binding, delivery-state prune.

## Targeted Checks

```powershell
cargo test -p agentdash-domain channel
cargo check -p agentdash-domain
```

## Progress Log

- initialized
