# W6 — Codex / Remote Adapters

## Depends On

- W1 Contracts & Crate Skeleton
- W3 Runtime State & Host Coordination
- W4 Surface / Tool / Hook

## Ownership

- `agentdash-integration-codex`
- `agentdash-integration-remote-runtime`
- adapter-side Runtime Wire usage

## Goal

让 Codex 与 Remote proxy 以完整 Agent 身份接入，保留 Codex ThreadStore/history 的 source
authority，诚实声明 context/change/tool/hook fidelity，并通过 snapshot/inspect 收敛外部
不确定性。

## Exit Criteria

- Codex/Remote service conformance 通过；
- native thread fork/read/compact/interrupt/interaction 映射正确；
- exact context 不存在时不冒充 exact；
- initial package 只声明可证明的 typed-native/canonical-rendered/unsupported fidelity，
  create/apply unknown outcome 可按同一 effect inspect；
- source change gap 通过 snapshot reconcile；
- vendor DTO 不出 adapter；
- Relay 只承担 placement transport。
