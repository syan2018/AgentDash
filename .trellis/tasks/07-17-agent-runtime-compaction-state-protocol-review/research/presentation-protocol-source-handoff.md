# Canonical Presentation 协议源交接

## 已形成的协议源

Service API 现在拥有完整、平台中立的 item presentation 词汇，包括 typed content block、item body、增量更新、终态证据、摘要和 interaction 生命周期。Runtime Contract 维护一份独立的规范化词汇，并通过显式 projector 从 Service API source observation 得到 Runtime 状态。两者的独立性让 Runtime 持久事实不依赖具体 AgentCore 或 vendor DTO，同时相同的规范化 JSON 形状保证摘要可以跨边界核验。

Codex vendor DTO、schema fixture、生成锁和 generator 归属 `agentdash-integration-codex`。Codex JSON-RPC 在该 crate 内先进入私有 typed DTO，再由穷尽 projector 进入 Service API；未知 vendor item 会作为 protocol violation 终止投影。Dash Agent 从自身历史折叠直接生成同一套 Service API presentation。Runtime Wire 和 Remote Runtime 只传输 Service API / Runtime Contract 事实。

## 根级 source switch 清单

以下两个根级变更在最终集成提交中与 crate 清理一起完成，使 contracts freshness 的唯一
入口指向新的私有 Codex 协议源。

1. `package.json` 的 `contracts:check`
   - 将 `cargo run -p agentdash-agent-protocol-codegen -- check`
   - 替换为 `cargo run -p agentdash-integration-codex --bin generate_codex_vendor_protocol -- check`
2. 根 `Cargo.toml`
   - 从 workspace members 移除 `crates/agentdash-agent-protocol-codegen`
   - 从 workspace dependencies 移除零消费者的 `agentdash-agent-protocol`

根级修改的机械 patch 等价于：

```diff
--- package.json
+++ package.json
-"contracts:check": "cargo run -p agentdash-agent-protocol-codegen -- check && ..."
+"contracts:check": "cargo run -p agentdash-integration-codex --bin generate_codex_vendor_protocol -- check && ..."

--- Cargo.toml
+++ Cargo.toml
-    "crates/agentdash-agent-protocol",
-    "crates/agentdash-agent-protocol-codegen",
-agentdash-agent-protocol = { path = "crates/agentdash-agent-protocol" }
```

`agentdash-agent-protocol` workspace member/dependency 的删除与它的剩余 Product、SPI、Relay
consumer 迁移属于同一个 S5 原子删除集合；codegen source switch 可以在该集合中直接
执行，因为私有 generator 已经独立生成和审计相同 pinned vendor roots。

新的私有生成源完整覆盖 Codex `ThreadItem`、`ServerNotification`、approval/user-input/MCP request roots、nullable overlay、默认空数组序列化 overlay 和 owned roundtrip 审计。生成物固定在：

- `crates/agentdash-integration-codex/src/vendor_generated/codex_v2.rs`
- `crates/agentdash-integration-codex/protocol-fixtures/schemas/`
- `crates/agentdash-integration-codex/codex-protocol-codegen.lock.json`

## Legacy source 与生成物删除清单

consumer gate 清零后，以下路径作为一个删除集合退出：

```text
crates/agentdash-agent-protocol-codegen/
crates/agentdash-agent-protocol/
schemas/upstream/
packages/app-web/src/generated/codex-app-server-protocol/
```

删除集合的前置 inventory 使用以下命令生成；每一条结果都必须归入明确 owner 或已删除
路径：

```powershell
rg -n "agentdash-agent-protocol|agentdash_agent_protocol" Cargo.toml crates scripts `
  -g "Cargo.toml" -g "*.rs" -g "*.json"
rg -n "agentdash-agent-protocol-codegen|generate_codex_vendor_protocol" `
  Cargo.toml package.json crates -g "Cargo.toml" -g "*.json" -g "*.rs"
rg -n "schemas/upstream|codex-app-server-protocol" Cargo.toml package.json crates packages scripts `
  -g "*.toml" -g "*.json" -g "*.rs" -g "*.ts" -g "*.tsx" -g "*.js"
cargo tree --workspace --invert agentdash-agent-protocol
cargo tree --workspace --invert agentdash-agent-protocol-codegen
```

根级 source switch 完成后，唯一 Codex vendor source 的机械证明为：

```powershell
cargo run -p agentdash-integration-codex --bin generate_codex_vendor_protocol -- check
rg -n "agentdash-agent-protocol-codegen|schemas/upstream|generated/codex-app-server-protocol" `
  Cargo.toml package.json crates packages scripts
```

第二条命令应没有生产 source 或 build script 命中；task research、历史 fixture 中的路径
引用按其非生产用途单独审计。

## 稳定性检查

切换后的稳定边界由四组检查共同证明：

- Service API / Runtime Contract generator freshness 保证 presentation 类型闭包与 decimal-string `u64` codec 同时存在。
- Codex private generator freshness 保证 pinned upstream schema、owned overlays、私有 Rust DTO 和 lock 一致。
- Native 与 Codex projector 测试保证 snapshot 和 typed change 来自同一 source history fold。
- Runtime Wire / Remote Runtime roundtrip 保证 typed transition、四类终态证据、source cursor、generation fence、dedupe 和 gap 语义不丢失。

完整切换的执行门禁：

```powershell
cargo run -p agentdash-agent-service-api --bin generate_agent_service_api -- --check
cargo run -p agentdash-agent-runtime-contract --bin generate_agent_runtime_contracts -- --check
cargo run -p agentdash-agent-runtime-wire --bin generate_agent_runtime_wire -- --check
cargo run -p agentdash-integration-codex --bin generate_codex_vendor_protocol -- check
cargo test -p agentdash-agent-service-api
cargo test -p agentdash-agent-runtime-contract
cargo test -p agentdash-agent-runtime-wire
cargo test -p agentdash-integration-codex
cargo test -p agentdash-integration-native-agent
cargo test -p agentdash-integration-remote-runtime
```
