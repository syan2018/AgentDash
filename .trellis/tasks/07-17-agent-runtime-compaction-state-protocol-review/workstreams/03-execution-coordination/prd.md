# W3 — Execution Coordination 与 Driver Observation

## Depends On

- W1 Hosted Agent Contract。
- W2 AgentSession aggregate、repository transaction 与 effect/binding schema骨架。

## Goal

把 Runtime/Host/worker 收敛为 Hosted Agent 内部的 execution coordination，让 stable effect、binding generation、delivery与inspect处理真实外部不确定性，同时让 driver只返回 provider-neutral receipt/observation。

## Scope

- binding lifecycle/generation；
- effect ledger、delivery lease、dispatch、inspect、settlement；
- worker职责收窄；
- Native/Codex/Remote `AgentExecutionPort` adapter；
- typed capability descriptor；
- duplicate/late/stale observation fence；
- `Applied|NotApplied|Failed|Unknown` 恢复决策；
- 删除 driver facts与专用 activation旁路。

## Ownership

主要负责：

- `crates/agentdash-agent-runtime-host/**`
- `crates/agentdash-infrastructure/**` 的 runtime worker/effect/binding adapter
- `crates/agentdash-integration-native-agent/**`
- `crates/agentdash-integration-codex/**`
- `crates/agentdash-integration-remote-runtime/**`
- runtime wire 中 execution delivery部分

不负责 AgentRun/API/UI 或 authoritative Session read。

## Deliverables

- 统一 effect delivery/inspection engine；
- driver conformance suite；
- 三个 adapter hard cut；
- failure/retry/unknown settlement policy；
- driver event ingress 只提交 observation。

## Acceptance Criteria

- [ ] worker claim/retry/release只改变 delivery，不直接改变业务 phase。
- [ ] effect identity在重试/重启后稳定。
- [ ] observation必须通过 effect identity、binding generation、expected Agent revision验证。
- [ ] stale/duplicate/late observation不复制或改写 Agent entity。
- [ ] stateful replica只有在必要时进入 convergence/inspect。
- [ ] unknown且不可 inspect 的结果进入 Lost，不猜测或换 ID 重放。
- [ ] Native/Codex/Remote通过同一 adapter conformance。
- [ ] driver contract、wire、adapter中不存在 Runtime journal fact生产。

## Non-Goals

- 不定义 Session read/fork或 protocol projector。
- 不在 driver内建立平台业务状态。
- 不保留 presentation replay作为 context activation。

## Validation

```powershell
cargo test -p agentdash-agent-runtime-host
cargo test -p agentdash-integration-native-agent
cargo test -p agentdash-integration-codex
cargo test -p agentdash-integration-remote-runtime
cargo test -p agentdash-infrastructure agent_runtime_worker
```
