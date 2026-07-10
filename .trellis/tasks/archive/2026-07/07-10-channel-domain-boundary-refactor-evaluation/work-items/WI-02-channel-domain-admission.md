# WI-02 Channel Domain / Admission

Status: done

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

## Implementation Evidence

- `ChannelKey/ChannelLocator`、canonical participant、lifetime/retention、origin/reply target/correlation 已在 domain contract 中正交化；registry schema 固定为 V2，owner-local key 原子 create-if-absent。
- `ChannelService` 在 provider ingress 与 broadcast planning 重新校验 owner、open status、active membership、operation、audience、ingress/egress；Companion 创建改为稳定 locator 与原子 mutation。
- runtime capability 改为 registry-derived projection replace，执行 authority 仍由 service admission 持有；terminal hook auto-resume 明确留在 Mailbox control-plane，不制造 synthetic Channel identity。
- `cargo test -p agentdash-domain channel::tests`：11 passed。
- `cargo test -p agentdash-application --lib channel::tests`：11 passed。
- `cargo test -p agentdash-application-agentrun --lib channel_projection_replaces_visible_channel_refs`：1 passed。
