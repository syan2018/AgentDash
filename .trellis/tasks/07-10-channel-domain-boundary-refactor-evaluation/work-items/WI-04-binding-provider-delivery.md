# WI-04 Binding Provider / Delivery

Status: planned

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
