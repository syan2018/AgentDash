# W7 Product production cutover component

该 component 基于冻结 revision `fc26d3ff`。Product-owned 生产代码位于
`agentdash_application_agentrun::agent_run::product_protocol`；它提供 injected
`AgentRunForkFacade`、durable Fork/Companion repositories/workers、Runtime Contract
snapshot/change feed 与任务本地 persistence schema contract。

`production-cutover-manifest.json` 是 S5 的精确集成输入，固定六个 W7 consumer 的
activation roots、最终替换边界、共享 owner symbol、切换顺序、breakpoint 与负门禁。
正式 migration、PostgreSQL adapter、AppState/composition、canonical generated roots、
Cargo/lock 和 legacy deletion 均由 W8 在同一个 hard cut 中实现。

本 component 不提供兼容 facade、fallback 或 recording production repository。旧 production
caller 仍保持在冻结 base 上，原因是缺少 Platform durable `CompleteAgentHost` seam 与
W8 transaction/composition 时，局部切 route 会形成不可恢复的半切换，而不是可独立验收的
production 状态。

## Product-owned verification

```powershell
cargo test -p agentdash-application-agentrun product_protocol
cargo test -p agentdash-api --test agent_runtime_target_projection
```

## S5 activation gate

只有 manifest 中 Platform Runtime、Dash/Native 与 W8 三组 shared-owner prerequisites 全部
固定后，才能依序应用六 caller cut、canonical generator cut 和 legacy deletion。任一 caller
仍读取 journal、`RuntimeSession`、Core `AgentTool` 或 `RuntimeToolProvider` 时均不得激活。
