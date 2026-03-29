# connector-contract 重命名为 agentdash-spi

## Goal

将 `agentdash-connector-contract` crate 重命名为 `agentdash-spi`，使名称准确反映其"跨层 SPI 契约仓库"的真实职责（包含 Connector SPI、Hook SPI、Lifecycle SPI、Tool SPI），消除"名为 connector-contract 但承载远超 connector 本身"的歧义。

## 背景

架构 review 发现 `agentdash-connector-contract` 实际导出的内容远超"连接器契约"：
- `connector.rs` — AgentConnector trait、ExecutionContext、RuntimeToolProvider
- `hooks.rs` — 完整的 Hook SPI（ExecutionHookProvider、HookSessionRuntimeAccess、20+ 种 Hook 数据类型）
- `lifecycle.rs` — 完整的 Agent Runtime Lifecycle SPI（AgentRuntimeDelegate、ToolCallDecision、StopDecision 等）
- `tool.rs` — AgentTool trait、AgentToolResult
- `schema.rs` — schema 相关

新名称 `agentdash-spi` 准确传达了"所有跨层共享的 Service Provider Interface"的定位。

## Requirements

### R1: 目录重命名

- `crates/agentdash-connector-contract/` -> `crates/agentdash-spi/`

### R2: Cargo.toml 更新

- `crates/agentdash-spi/Cargo.toml` 中 `name = "agentdash-spi"`
- `crates/agentdash-spi/Cargo.toml` 中 `description` 更新为反映 SPI 定位
- workspace `Cargo.toml`:
  - `members` 列表替换路径
  - `[workspace.dependencies]` 中 `agentdash-connector-contract = ...` 改为 `agentdash-spi = ...`
- 所有依赖 `agentdash-connector-contract` 的 crate 的 `Cargo.toml` 更新依赖名

### R3: 全局 import 路径替换

在所有 `.rs` 文件中：
- `agentdash_connector_contract` -> `agentdash_spi`（Rust 模块路径）
- `agentdash-connector-contract` -> `agentdash-spi`（Cargo feature/dependency 引用）

受影响的 crate（依赖 connector-contract 的）：
- `agentdash-application`
- `agentdash-executor`
- `agentdash-agent`
- `agentdash-api`

### R4: 文档和 spec 更新

- `README.md` 中的 crate 列表更新
- `.trellis/spec/backend/index.md` 中如有引用需同步更新

## Acceptance Criteria

- [ ] `cargo check --workspace` 通过
- [ ] `cargo clippy --workspace -- -D warnings` 通过
- [ ] `cargo test --workspace` 全部通过
- [ ] 不存在任何对 `agentdash-connector-contract` 或 `agentdash_connector_contract` 的引用（除 git 历史）
- [ ] crate 目录为 `crates/agentdash-spi/`
- [ ] 无逻辑变更，纯重命名

## Technical Notes

- 这是纯机械替换操作，但影响面广（所有依赖该 crate 的 Cargo.toml + 所有 import 语句）
- 建议一次性全量替换后做 `cargo check` 验证
- `agentdash-agent` 中有对 connector-contract 的 re-export（`pub use agentdash_connector_contract::...`），需要同步更新

## Risk

- 低：纯重命名，无逻辑变更
- 需确保全局替换覆盖完整，遗漏会导致编译失败（可通过 cargo check 快速发现）
