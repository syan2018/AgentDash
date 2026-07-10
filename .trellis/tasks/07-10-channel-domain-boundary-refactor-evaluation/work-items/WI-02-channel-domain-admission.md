# WI-02 Channel Domain / Admission

Status: planned

Depends On: WI-00

## Scope

- ChannelKey/Locator/Ref、canonical participant principal。
- purpose/binding/lifetime/retention/thread/audience 维度正交化。
- origin/reply target/correlation 拆分。
- publish/reply/broadcast service admission。
- capability projection 与 registry authority 收束。
- runtime wake 使用真实 registry identity。

## Exit Criteria

- 每个字段只表达一个领域维度。
- sender/status/membership/operation/audience/ingress/egress/readiness 全部校验。
- directive 不能暴露不存在或未授权的 ChannelRef。
- 生产路径不存在 synthetic channel id。

## Validation

- domain validation/property tests。
- ChannelService admission matrix。
- runtime wake/Companion integration tests。
