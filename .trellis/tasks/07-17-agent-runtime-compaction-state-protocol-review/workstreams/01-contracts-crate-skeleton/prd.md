# W1 — Contracts & Crate Skeleton

## Depends On

None.

## Ownership

- `agentdash-agent-runtime-contract`
- new `agentdash-agent-service-api`
- `agentdash-agent-runtime-wire`
- `agentdash-agent-runtime-test-support`
- contract/wire generated artifacts

## Goal

建立 Application ↔ Runtime 与 Host ↔ Complete Agent 两个不同的 dependency-light 合同，
固定 typed IDs、command/read/change/inspect、capability/fidelity 和 conformance
fixture，使后续工作无法再次把完整 Agent 降成低层 driver。

## Exit Criteria

- Runtime Contract 与 Complete Agent Service API 物理分离；
- snapshot mandatory、source change capability-graded、platform change mandatory；
- Fork/Tool/Hook/Context/Compaction profile 逐项 typed；
- `InitialAgentContextPackage` variants/provenance/digest、create receipt/inspect 与
  `TypedNative/CanonicalRendered/Unsupported` fidelity 完整，且无 Companion/Product
  DTO 泄漏；
- surface apply/revoke 与 AgentHostCallbacks reverse Tool/Hook 合同完整；
- remote callback wire 的 request/decision/result/ack/replay 完整；
- exact requirement 不接受 observed/approximation；
- contract/wire codegen 与 tests 通过；
- 依赖图满足父 `design.md` §14。
