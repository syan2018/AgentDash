# API 层 re-export 残余清理与 runtime_bridge MCP 依赖归位

## Goal

清理 `agentdash-api` 中残留的全量 re-export shim（`address_space_access/mod.rs`），并将 Application 层 `runtime_bridge.rs` 对 `agentdash-mcp` 类型的直接依赖迁移到 API 层，使 Application 层不再感知 MCP 协议细节。

## 背景

上一轮 `app-contract-layering-refactor` 清理了 `session_plan`、`session_context`、`execution_hooks` 三个 API 层 re-export shim，但将 `address_space_access` 标记为遗留项未处理。同时 Application 层的 `runtime_bridge.rs` 仍直接导入 `agentdash_mcp::injection::McpInjectionConfig` 和 `agentdash_mcp::scope::ToolScope`，违反了"Application 层不应感知具体协议实现"的分层原则。

## Requirements

### R1: address_space_access re-export 清理

当前 `crates/agentdash-api/src/address_space_access/mod.rs` 的结构：
- 14 行 `pub use` 全量 re-export
- ~380 行集成测试（依赖 API 层组件如 BackendRegistry、RelayFsMountProvider）

变更：
- [ ] 删除 `pub use agentdash_application::address_space::*` 全量 re-export
- [ ] 删除 `pub use agentdash_application::address_space::inline_persistence::*` re-export
- [ ] 删除 `pub use agentdash_application::address_space::tools::provider::*` re-export
- [ ] API 层内部消费者（`app_state.rs`、routes、`mount_providers/`）改为直接 `use agentdash_application::address_space::XXX` 精确导入
- [ ] 集成测试保留在 API 层（因为需要 BackendRegistry 等 API 层组件），文件可保持或移至 `tests/` 目录下
- [ ] 如果清除 re-export 后 `address_space_access/mod.rs` 仅剩测试代码，考虑删除该模块并将测试移到更合适的位置

### R2: runtime_bridge MCP 依赖归位

当前 Application 层对 MCP 类型的引用点：
- `application/runtime_bridge.rs` — `mcp_injection_config_to_runtime_binding()` 函数
- `application/task/gateway/turn_context.rs` — `use agentdash_mcp::injection::McpInjectionConfig`
- `application/task/context_builder.rs` — `use agentdash_mcp::injection::McpInjectionConfig`

变更：
- [ ] 将 `mcp_injection_config_to_runtime_binding()` 从 `application/runtime_bridge.rs` 移到 `api/runtime_bridge.rs`
- [ ] `application/task/gateway/turn_context.rs` 改为接收 `Option<RuntimeMcpBinding>` 参数，不再自行构建 MCP 配置
- [ ] `application/task/context_builder.rs` 同理
- [ ] MCP 配置 -> RuntimeMcpBinding 的转换在 API 层的调用点完成（`TaskTurnServices` 或 `TaskLifecycleService` 的构建处）
- [ ] 验证 `application/Cargo.toml` 可移除 `agentdash-mcp` 依赖

### R3: Application 层 Cargo.toml 依赖清理

- [ ] 移除 `agentdash-mcp` 依赖（如 R2 完成后不再需要）
- [ ] 确认无其他 Application 层文件引用 `agentdash_mcp`

## Acceptance Criteria

- [ ] `cargo check --workspace` 通过
- [ ] `cargo clippy --workspace -- -D warnings` 通过
- [ ] `cargo test --workspace` 全部通过，测试数量不减少
- [ ] `agentdash-api/src/address_space_access/mod.rs` 不含 `pub use agentdash_application::address_space::*` 全量 re-export
- [ ] `agentdash-application/Cargo.toml` 不依赖 `agentdash-mcp`
- [ ] Application 层 `.rs` 文件中零 `use agentdash_mcp` 引用
- [ ] 无外部 API 变更

## Technical Notes

- R1 影响面较小，主要是修改 API 层内部 import 路径
- R2 需要调整 `TaskTurnServices` 结构体或 `prepare_task_turn_context` 函数签名，将 MCP binding 作为已转换的参数传入
- 如果 `agentdash-mcp` 中的 `McpInjectionConfig` 类型也被 `agentdash-mcp` crate 的 MCP 路由使用，API 层可以直接在路由构建处完成转换

## Risk

- 低：转换逻辑迁移，无行为变更
- R2 涉及函数签名调整，需要仔细追踪调用链确保所有调用点都传递了转换后的 binding
