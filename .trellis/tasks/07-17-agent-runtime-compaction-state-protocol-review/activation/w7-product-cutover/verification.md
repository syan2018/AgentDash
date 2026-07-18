# W7 Product production cutover verification

## Component result

Base revision：`fc26d3ff`。

Product code commit：`c153fb53 feat(agent-runtime): 提升Product生产协议入口`。

该 commit 将 target-only module 提升为
`agentdash_application_agentrun::agent_run::product_protocol`，并增加：

- injected `AgentRunForkFacade`；
- `AgentRunForkSagaRepository::commit_product_graph` transaction seam；
- `AGENT_RUN_FORK_SAGA_SCHEMA_CONTRACT` 与
  `COMPANION_FRESH_SAGA_SCHEMA_CONTRACT`；
- cfg(test) 且 `pub(super)` 的 Recording repositories；
- production `AgentRunRuntimeProjectionPort` 与 `AgentRunRuntimeFeedReconnect`。

## Passed gates

```text
cargo test -p agentdash-application-agentrun product_protocol
40 passed; 0 failed

cargo test -p agentdash-api --test agent_runtime_target_projection
3 passed; 0 failed

cargo check -p agentdash-application-agentrun
passed

rustfmt --edition 2024 <Product-owned Rust files>
passed
```

本地负门禁通过：

- `target_product_protocol` 不再作为 public module；
- production visibility 中不存在 public Recording fork/Companion repository；
- `product_protocol` 不依赖具体 Runtime Host、Complete Agent implementation 或 Integration
  crate；
- cutover manifest 可由 PowerShell `ConvertFrom-Json` 完整解析；
- `verify-caller-inventory.ps1` 验证六 caller inventory 与 source legacy symbol 搜索集合完全一致；
- `cargo tree -p agentdash-application-agentrun -e normal --depth 1` 证明 Application 没有直接
  依赖 Complete Agent service API、Host、Integration 或 vendor crate；
- dev-only canonical parity 覆盖 `CreateAgentCommand.initial_context` 和同 effect inspect；
- task-local schema、canonical Rust serialization 与 frontend fixture hash 保持一致。

## S5 breakpoints

以下门禁是 combined S5 gate，本 component 不声称通过：

- Platform Runtime 尚需为 `CompleteAgentHost` 与 `CompleteAgentStateRepository` 提供
  production durable composition；
- Dash/Native 尚需将 exact fork、fresh context apply/inspect evidence 接入 production
  Complete Agent service registration；
- W8 尚需实现两张 Product saga 表的唯一 migration/PostgreSQL adapter、AppState
  constructor 与 worker lease；
- API/application/application-agentrun/lifecycle/ports/vfs 六 caller、canonical generator、
  frontend feed 与 journal/RuntimeSession deletion 必须在同一 hard cut 切换；
- 全仓 `AgentTool`、`RuntimeToolProvider`、`RuntimeSession` 与 `RuntimeJournalFact` 负搜索
  只有在上述 caller/deletion set 集成后才应为零。

这些 breakpoint 的 exact symbols、owner、顺序与命令见
`production-cutover-manifest.json`。
