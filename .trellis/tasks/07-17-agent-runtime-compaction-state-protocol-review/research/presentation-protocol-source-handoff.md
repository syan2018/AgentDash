# Canonical Presentation 协议源交接

## 已形成的协议源

Service API 现在拥有完整、平台中立的 item presentation 词汇，包括 typed content block、item body、增量更新、终态证据、摘要和 interaction 生命周期。Runtime Contract 维护一份独立的规范化词汇，并通过显式 projector 从 Service API source observation 得到 Runtime 状态。两者的独立性让 Runtime 持久事实不依赖具体 AgentCore 或 vendor DTO，同时相同的规范化 JSON 形状保证摘要可以跨边界核验。

Codex vendor DTO、schema fixture、生成锁和 generator 归属 `agentdash-integration-codex`。Codex JSON-RPC 在该 crate 内先进入私有 typed DTO，再由穷尽 projector 进入 Service API；未知 vendor item 会作为 protocol violation 终止投影。Dash Agent 从自身历史折叠直接生成同一套 Service API presentation。Runtime Wire 和 Remote Runtime 只传输 Service API / Runtime Contract 事实。

## 根级 source switch 清单

以下两个根级变更应在最终集成提交中与 crate 清理一起完成，以便 contracts freshness 的唯一入口指向新的私有 Codex 协议源：

1. `package.json` 的 `contracts:check`
   - 将 `cargo run -p agentdash-agent-protocol-codegen -- check`
   - 替换为 `cargo run -p agentdash-integration-codex --bin generate_codex_vendor_protocol -- check`
2. 根 `Cargo.toml`
   - 当旧 protocol/codegen consumer inventory 清零后，从 workspace members 移除 `crates/agentdash-agent-protocol-codegen`
   - 同一次清理删除旧 codegen crate、旧 Codex generated DTO、旧 `schemas/upstream` 和旧前端 vendor TypeScript 生成物

新的私有生成源完整覆盖 Codex `ThreadItem`、`ServerNotification`、approval/user-input/MCP request roots、nullable overlay、默认空数组序列化 overlay 和 owned roundtrip 审计。生成物固定在：

- `crates/agentdash-integration-codex/src/vendor_generated/codex_v2.rs`
- `crates/agentdash-integration-codex/protocol-fixtures/schemas/`
- `crates/agentdash-integration-codex/codex-protocol-codegen.lock.json`

## 稳定性检查

切换后的稳定边界由四组检查共同证明：

- Service API / Runtime Contract generator freshness 保证 presentation 类型闭包与 decimal-string `u64` codec 同时存在。
- Codex private generator freshness 保证 pinned upstream schema、owned overlays、私有 Rust DTO 和 lock 一致。
- Native 与 Codex projector 测试保证 snapshot 和 typed change 来自同一 source history fold。
- Runtime Wire / Remote Runtime roundtrip 保证 typed transition、四类终态证据、source cursor、generation fence、dedupe 和 gap 语义不丢失。
