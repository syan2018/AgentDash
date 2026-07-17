# W1 — Hosted Agent Contract

## Depends On

无。

## Goal

建立稳定、AgentDash-owned 的 Hosted Agent 语言与 contract，使 Application、repository、execution adapter 和 projector围绕同一套 Agent command/query/entity/change协作，并让后续实现可以通过 repository-neutral behavior suite验证。

## Scope

- `HostedAgentGateway::execute/read/changes`；
- Session、Operation、Queue、Turn、Item、Interaction、ContextRevision、Compaction、Binding、Effect、AgentChange 的 stable IDs 和 contract；
- command fingerprint/idempotency、revision/order/cursor gap；
- provider-neutral dispatch/inspect receipt/observation/capability；
- behavior suite fixture/driver interface；
- runtime wire 对新 contract 的表达；
- 删除 contract 层 driver-produced `RuntimeJournalFact`。

## Ownership

主要负责：

- `crates/agentdash-agent-runtime-contract/**`
- `crates/agentdash-agent-runtime-wire/**`
- contract codegen 与对应 generated contract
- behavior test support 中只与 contract/fixture DSL 有关的部分

不负责 repository、worker、driver implementation、AgentRun/UI。

## Deliverables

- Agent-owned public contract 与 rustdoc；
- provider-neutral execution port；
- behavior suite skeleton；
- generated contract通过 check；
- 删除/替换旧 Runtime journal facts envelope 的编译路径清单。

## Acceptance Criteria

- [ ] Application contract 不暴露 repository、worker、driver handle、journal cursor 或 vendor DTO。
- [ ] `execute`、`read`、`changes` 的 durable/authoritative/after-commit 语义可由类型和测试表达。
- [ ] stable identity、operation fingerprint、Agent revision 与 change ordinal 有明确约束。
- [ ] driver只能产生 receipt/observation，contract 中不存在 `Vec<RuntimeJournalFact>`。
- [ ] behavior suite可表达父设计第 6、17 节的不变量。
- [ ] wire round-trip不丢失 typed entity/observation语义。

## Non-Goals

- 不实现 database schema、state transition或具体 adapter。
- 不直接复用 Codex DTO 作为 Agent domain type。
- 不为旧 Runtime contract 建兼容 facade。

## Validation

```powershell
cargo test -p agentdash-agent-runtime-contract
cargo test -p agentdash-agent-runtime-wire
cargo run -p agentdash-agent-runtime-contract --bin generate_agent_runtime_contracts -- --check
cargo run -p agentdash-agent-runtime-wire --bin generate_agent_runtime_wire -- --check
```
