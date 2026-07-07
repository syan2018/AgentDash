# WI-01 Domain Channel Document Model

Status: done
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
- previous WI-01 worker run produced no accepted implementation; current state is the candidate workspace implementation under native check
- candidate implementation exists in workspace: `agentdash-domain::channel` with registry document, semantic mutations, validation, address and delivery contracts
- targeted checks were run by host and must be verified by native check worker before this item can move forward: `cargo test -p agentdash-domain channel`; `cargo check -p agentdash-domain`; `cargo fmt -p agentdash-domain`
- native check worker `Ohm` completed WI-10 full-scope check; domain policy mutation gap was fixed and verification passed
- dispatcher integration review passed; targeted domain channel test passed
