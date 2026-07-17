# W4 — Surface / Tool / Hook

## Depends On

- W1 Contracts & Crate Skeleton
- W3 Runtime State & Host Coordination

## Ownership

- Business Agent Surface compiler
- RuntimeOffer/Bound/Applied surface modules
- Tool Broker
- Hook plan/profile/materialization
- typed product fact adapters

## Goal

把平台期望能力与 Agent 实际能力分开建模，通过逐项求交和 applied evidence 决定
availability。每个 Tool/Hook contribution 固定唯一 causal route。

## Exit Criteria

- desired/offer/bound/applied 四对象链完整；
- required 能力不足在 side effect 前 typed reject；
- Tool/Hook 无 bool capability 与双执行；
- Complete Agent surface 只经 apply/revoke，Agent-native Tool/Hook 只经 typed
  AgentHostCallbacks；
- remote reverse call 的 deadline/ack/replay/generation fence 可验证；
- Host 不编译产品 Surface，adapter 不重新解释 policy；
- `agentdash-application-hooks` 业务职责已迁出。
