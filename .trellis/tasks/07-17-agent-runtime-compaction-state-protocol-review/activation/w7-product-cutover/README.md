# W7 Product production cutover component

该 component 基于冻结 revision
`fc26d3ffb951461d8e9214b6b4639b88c18d533d`。Product domain/code revision 为
`7f79e21fa59adb40cf94067b684ae79d3685892b`，artifact 审查输入 tip 为
`a08e871bbdfe662ddeefccab7f166fe8c2ab222e`。Product-owned 生产代码位于
`agentdash_application_agentrun::agent_run::product_protocol`；它提供 injected
`AgentRunForkFacade`、durable Fork/Companion repositories/workers、Runtime Contract
snapshot/change feed 与任务本地 persistence schema contract。

`production-cutover-manifest.json` 是 S5 的精确集成输入，固定六个 W7 consumer 的逐文件
record、owner、逐文件增删 symbols、替换语义、前置条件与 gate。配套
`verify-caller-inventory.ps1` 将每个 source file 的 legacy symbol 集合与 manifest record
逐项比对，因此 task/wait/VFS、API bootstrap/tool surface、AgentRun RuntimeSession、
journal、frontend feed 和 generated output 的文件或 symbol 漂移都会直接阻断激活。
正式 migration、PostgreSQL adapter、AppState/composition、canonical generated roots、
Cargo/lock 和 legacy deletion 均由 W8 在同一个 hard cut 中实现。

本 component 不提供兼容 facade、fallback 或 recording production repository。旧 production
caller 仍保持在冻结 base 上，原因是缺少 Platform durable `CompleteAgentHost` seam 与
W8 transaction/composition 时，局部切 route 会形成不可恢复的半切换，而不是可独立验收的
production 状态。

Fresh Companion 的 Product 输入使用 `Compiled*` DTO；Product crate 的 normal dependency
边界不绑定 Complete Agent service API。测试态 adapter parity 将 package ID、schema、mode、
contribution、provenance、revision、digest、delivery fidelity 与 applied evidence 无损映射到
`CreateAgentCommand.initial_context`，并验证未知结果继续 inspect 同一 create effect ID。

## Product-owned verification

```powershell
cargo test -p agentdash-application-agentrun product_protocol
cargo test -p agentdash-api --test agent_runtime_target_projection
powershell -File .trellis/tasks/07-17-agent-runtime-compaction-state-protocol-review/activation/w7-product-cutover/verify-caller-inventory.ps1
cargo tree -p agentdash-application-agentrun -e normal --depth 1
```

## S5 activation gate

只有 manifest 中 Platform Runtime、Dash/Native 与 W8 三组 shared-owner prerequisites 全部
固定后，才能依序应用六 caller cut、canonical generator cut 和 legacy deletion。任一 caller
仍读取 journal、`RuntimeSession`、Core `AgentTool` 或 `RuntimeToolProvider` 时均不得激活。

当前状态为 `domain_component_ready`；production caller source switch 等待 manifest 的两项
`w8_live_prerequisite_contracts`。W8 shared foundation 落地后，Product owner 按
`pending_product_caller_activation` 的六步顺序完成 caller patch，再交回 W8 继续
composition/deletion/lock 收口。

Application 的 dev-only Complete Agent service API parity 会让 W8 combined
`Cargo.lock` 在 `agentdash-application-agentrun` package dependency list 中增加现有
workspace package `agentdash-agent-service-api`。该变化不引入 registry package/checksum，
由 W8 combined graph 唯一 regenerate，并通过 manifest 冻结的 metadata、diff 与
`cargo test --locked` gate 验证。
