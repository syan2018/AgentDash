# WI-00 Decision / Residual Reconciliation

Status: planned

Depends On: none

## Scope

- 对账 07-07 archived task、database/capability/mailbox specs 与当前代码。
- 建立 residual matrix：synthetic channel identity、runtime wake bypass、service admission、directive 第二授权、unsupported binding。
- 核实 owner variant evidence 与 cross-task Operation dependency。

## Exit Criteria

- 既有 owner-local persistence 决策没有被静默推翻。
- 每个 residual 有 owning work item、write set 和 verification。
- Story/System 等无证据 owner 有明确删除或保留结论。

## Validation

- `rg` ChannelService/ChannelMessage/ChannelDirective/ChannelOwner usages。
- 归档 task acceptance 与当前代码逐项对照。
