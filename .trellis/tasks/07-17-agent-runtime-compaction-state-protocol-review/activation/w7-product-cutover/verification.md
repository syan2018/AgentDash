# W7 Product production cutover verification

## Component result

Base revision：`fc26d3ffb951461d8e9214b6b4639b88c18d533d`。

Product code commit：`7f79e21fa59adb40cf94067b684ae79d3685892b
feat(agent-runtime): 完备Product原子提交协议`。

Artifact review input tip：`a08e871bbdfe662ddeefccab7f166fe8c2ab222e`。

该 Product code revision 包含 production
`agentdash_application_agentrun::agent_run::product_protocol`，并固定：

- injected `AgentRunForkFacade`；
- `AgentRunForkSagaRepository::commit_product_graph` transaction seam；
- `AGENT_RUN_FORK_SAGA_SCHEMA_CONTRACT` 与
  `COMPANION_FRESH_SAGA_SCHEMA_CONTRACT`；
- cfg(test) 且 `pub(super)` 的 Recording repositories；
- production `AgentRunRuntimeProjectionPort` 与 `AgentRunRuntimeFeedReconnect`。
- immutable prepared Product graph、CAS atomic publish 与 dev-only canonical context
  parity。

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
- `verify-caller-inventory.ps1` 验证六 caller 的每个文件 record、legacy symbol 集合、
  owner、replacement、prerequisites 与 gate，并验证 frontend/generated canonical
  root/output record；
- 六 caller group 共冻结 78 个 source records；API/frontend/generated 另冻结
  14/9/4 个逐文件 records；
- `cargo tree -p agentdash-application-agentrun -e normal --depth 1` 证明 Application 没有直接
  依赖 Complete Agent service API、Host、Integration 或 vendor crate；
- dev-only canonical parity 覆盖 `CreateAgentCommand.initial_context` 和同 effect inspect；
- task-local schema、canonical Rust serialization 与 frontend fixture hash 保持一致。

当前 `Cargo.lock` 保持 W8 owner 的冻结输入。combined graph 预期只在
`agentdash-application-agentrun` dependency list 中增加现有 workspace
`agentdash-agent-service-api` dev dependency，不增加 registry package/checksum；W8
regenerate 后执行 manifest 中的 `git diff -- Cargo.lock`、
`cargo metadata --locked --format-version 1` 与
`cargo test --locked -p agentdash-application-agentrun product_protocol`。

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
