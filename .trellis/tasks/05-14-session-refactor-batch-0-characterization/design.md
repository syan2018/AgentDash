# Design：Batch 0 Characterization

## Strategy

Batch 0 只做 characterization。测试应尽量贴近现有 seam，避免为了测试而提前重构生产代码。

优先测试位置：

- `crates/agentdash-application/src/session/assembler.rs`
- `crates/agentdash-application/src/session/path_policy.rs`
- `crates/agentdash-application/src/session/hub/tests.rs`
- `crates/agentdash-api/src/routes/acp_sessions.rs`

## Characterization Targets

### Request assembly

当前 `PreparedSessionInputs` 通过 `finalize_request` 合入 `PromptSessionRequest`。这仍是旧链路的半成品 request 语义，Batch 0 需要固定它，便于 Batch 1/3 删除时知道哪些行为被迁移。

目标覆盖：

- prepared prompt blocks 覆盖 base prompt blocks；
- prepared executor config 覆盖 base executor config；
- prepared identity / post-turn handler 覆盖 base；
- prepared VFS / workspace defaults 的现有优先级；
- prepared MCP servers 整体替换。

### Owner priority

当前已知背离：

- launch augment：Task -> Story -> Project；
- context query：Project -> Story -> Task。

Batch 0 不修正背离，只用测试或注释化 fixture 固定现状。Batch 1 修正 owner resolver 时，这些测试应被有意更新。

### Prompt pipeline fallback / failure

优先选择现有 hub tests 可承载的路径，不搭建大规模集成环境。

目标覆盖至少一个：

- pending capability transition 下一轮 prompt 消费后清空；
- connector.prompt failure 后 runtime 回 idle；
- owner bootstrap 不应在 connector failure 后被错误视为完成；
- VFS/MCP/capability fallback 来源符合当前逻辑。

### Working dir path policy

当前 `resolve_working_dir` 明确保留绝对路径与 `..` 语义。Batch 0 增加测试记录现状，Batch 7 再改为拒绝。

## Non-Goals

- 不新增目标架构类型。
- 不调整 owner priority。
- 不收紧路径策略。
- 不改 terminal effect 执行路径。
