# WI-04 Binding Provider / Delivery

Status: ready_for_integration

Depends On: WI-02、WI-03

## Scope

- ChannelBindingProvider SPI。
- inbound normalize/participant resolution/policy/message。
- outbound publish/reply/delivery state。
- internal/test provider 端到端 materialization。
- Interaction/Operation attention refs integration。

## Exit Criteria

- production 产品路径不再只有 unsupported resolver。
- provider protocol 不绕过 ChannelService admission。
- Mailbox/Gate/notification/outbox 继续拥有各自 materialized facts。
- Channel 只保存 bounded delivery recovery state。

## Validation

- provider inbound/outbound integration tests。
- mailbox/gate/outbox materialization tests。
- duplicate/replay/binding unavailable tests。

## Implementation Evidence

- `ChannelBindingProvider` SPI 独立覆盖 provider event normalization、outbound publish 与 provider receipt；Host Integration 只贡献 provider，启动 composition 按 exact `provider_key` 冲突失败。
- `ChannelBindingProviderRegistry`、可重建 exact-key reverse index 与 `IndexedChannelBindingResolver` 已进入 production `ServiceSet`，ingress 不扫描 owner documents。
- `ChannelService` 对 provider ingress、publish/reply planning 和 physical dispatch 全部重新读取 registry 并 admission；dispatch receipt materialize 为 bounded delivery state，Mailbox/Gate/provider payload authority 不进入 Channel registry。
- application Channel tests：12 passed，覆盖 ingress/reply 端到端、provider replay rejection、unavailable/stale binding、mailbox/gate materialization。
- API integration registration tests：3 passed，覆盖 provider collect、duplicate key 与 invalid key；`cargo check -p agentdash-api --lib` passed。
