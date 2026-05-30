# Domain 净化设计

## 目标边界

本 child 只解决 domain 被前端/协议导出塑形的问题：

- `agentdash-domain` 不再依赖 `ts-rs` / `schemars`。
- `agentdash-contracts::workflow` 不再 re-export domain workflow value object。
- session id 假 alias 不再保留为“类型安全”表象。

DDD 依赖方向：domain 不引用 contracts / protocol DTO；contracts/API/protocol 入口位于外层，负责把 wire payload 映射为 domain/application 输入。

## Workflow Contracts

当前 `agentdash-contracts/src/workflow.rs` 直接：

```rust
pub use agentdash_domain::workflow::{ ... };
```

这是本批最关键的反向污染点。处理方式：

1. 在 contracts 侧定义 workflow wire DTO，字段、serde tag、rename、default 与当前 domain JSON shape 保持一致。
2. `generate_ts.rs` 只导出 contract DTO，不要求 domain 类型实现 `TS`。
3. API route-local DTO 仍可在当前过渡期使用 domain workflow 类型，但不得让 `agentdash-contracts::workflow` re-export domain 类型；跨 feature 复用的 wire DTO 继续进入 contracts。

首批 wire DTO 覆盖 `generate_ts.rs` 导出的 workflow 类型及其递归字段依赖：

- `ActivityDefinition` / executor spec / policies / transitions / artifact binding。
- `WorkflowContract` / injection / hook rule / capability config / ports。
- `ActivityLifecycleRunState` / attempts / artifacts / executor run refs。
- `LifecycleExecutionEntry` / `LifecycleRunStatus` / validation issue / binding/source enums。

`ToolCapabilityPath` 在 contracts 中作为 string wire 类型表达，domain 中保留 parse/reduction 行为。

MCP 工具输入属于协议入口，不再要求 domain 类型实现 `JsonSchema`。复杂 workflow/story/project 配置片段在 MCP 参数中以 JSON payload 接收，再解析为 domain 类型进入用例。

## Domain Derive Cleanup

移除 domain 中 `ts_rs::TS` 与 `schemars::JsonSchema` derive/import。`serde` 仍保留，因为 domain 当前既是持久化 JSON payload 又是 application 内部数据交换事实。

涉及路径：

- `crates/agentdash-domain/Cargo.toml`
- `crates/agentdash-domain/src/common/*`
- `crates/agentdash-domain/src/workspace/*`
- `crates/agentdash-domain/src/session_composition.rs`
- `crates/agentdash-domain/src/workflow/value_objects/*`

## Session ID Alias

PRD 默认要求升真 newtype；执行前复核后，newtype 会牵动 repository trait、Postgres bind/read、workflow run equality、API response mapper 与大量测试 fixture。由于这些 id 没有额外不变量，强行 newtype 会把本 child 变成跨 persistence/API 的大重构。

本批采用 PRD 允许的降级方案：删除 `SessionId` / `StorySessionId` / `ChildSessionId` 假 alias，domain 字段改回 `String`，由字段名表达语义。这样消除“同型 alias 却宣称编译期安全”的误导，也避免半路引入 `.0` 样板但没有真实不变量收益。

## 验证

- `rg "ts-rs|schemars" crates/agentdash-domain/Cargo.toml` 无命中。
- `rg "derive\\(.*TS|JsonSchema|ts_rs::TS|schemars::JsonSchema" crates/agentdash-domain/src` 无命中。
- `rg "pub type (SessionId|StorySessionId|ChildSessionId) = String" crates/agentdash-domain` 无命中。
- `pnpm run contracts:check`。
- `cargo check --workspace`。
- `cargo test -p agentdash-domain --lib`。
- `cargo test -p agentdash-mcp --lib`。
- `pnpm -C packages/app-web exec tsc --noEmit`。
