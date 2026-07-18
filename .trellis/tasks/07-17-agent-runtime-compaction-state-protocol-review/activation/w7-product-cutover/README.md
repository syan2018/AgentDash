# W7 Product production cutover component

该 component 基于冻结 revision `fc26d3ff`。Product-owned 生产代码位于
`agentdash_application_agentrun::agent_run::product_protocol`；它提供 injected
`AgentRunForkFacade`、durable Fork/Companion repositories/workers、Runtime Contract
snapshot/change feed 与任务本地 persistence schema contract。

`production-cutover-manifest.json` 是 S5 的精确集成输入，固定六个 W7 consumer 的逐文件
activation roots、owner、增删 symbols、替换语义、前置条件与 gate。配套
`verify-caller-inventory.ps1` 将六个 source root 的 legacy symbol 搜索结果与 manifest 做
集合等价校验，因此 task/wait/VFS、API bootstrap/tool surface、AgentRun RuntimeSession、
journal、frontend feed 和 generated roots 的漂移会直接阻断激活。
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
