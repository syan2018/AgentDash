# Agent Runtime 持久化权威收敛结果

## 最终形态

```text
Product owner document
  LifecycleRun / LifecycleAgent / frames / workflow / lineage / Agent association
        |
        | synchronous input + stable identity
        v
In-memory Runtime + In-memory Complete Agent Host
  mapping / timeout / normalize / live broadcast / attach / route / callback fence
        |
        v
Concrete Complete Agent authority
  source / history / context / fork / compaction / effect receipt / applied surface
```

Runtime 和 Host 位于两个 durable owner之间。它们能够从 Product association与concrete Agent
`read/inspect`重建，因此没有独立跨重启业务寿命。这个边界让一次Agent操作只有一个执行
authority，也让List、Workspace与重连不再依赖派生projection是否“最新”。

## 当前持久化目录

| 状态 | owner | 物理形态 | 恢复理由 |
| --- | --- | --- | --- |
| Lifecycle/Frame/association | Product LifecycleAgent | `lifecycle_agents` + owner-local JSONB | Product业务归属与执行意图 |
| Workflow/Gate/Routine/Channel | 对应Product聚合 | owner row/document/effect | 独立业务等待、编排与下游evidence |
| Workspace/Terminal presentation | 对应Product effect | 独立Product store | 产品资源/终端副作用，不回写Agent执行 |
| Dash source/history/context | Dash Complete Agent | `dash_complete_source.document` | concrete Agent native authority |
| Create前effect receipt | Dash Complete Agent | `dash_complete_effect` | source产生前按effect identity inspect |
| Tool/Hook receipt | 实际handler owner | owner-specific effect | 外部副作用幂等 |
| Runtime/Host route/live delta | 无durable owner | process memory | 可从两端重建 |

## 单向链路

### Command

```text
Product target + client identity
  -> stable handoff/effect identity
  -> current Host route
  -> Complete Agent inspect/execute
  -> concrete Agent receipt
```

Product只有在Agent返回receipt后才报告accepted。Agent unavailable表示当前请求未完成handoff；
调用者用同一identity重试。

### Read

```text
Product shell
  + Complete Agent read(source) -> in-memory conversation projection
  + LifecycleGate waiting items
  -> API response
```

Agent enrichment不可用不会改变Product shell的合法性。

### Live / Reconnect

```text
Agent Core callback -> Complete Agent source live channel -> Runtime normalize -> UI partial lane
disconnect / lag -> discard partial lane -> Complete Agent read(source)
```

partial delta没有跨重启承诺；committed history始终以Agent snapshot为准。

## Schema Convergence

- 0090–0096移除Runtime/Host/Callback owner、change delivery、command ledger与Dash关系镜像。
- 0097把AgentFrame history与association并入LifecycleAgent owner document。
- 0098–0103移除Workflow/AgentRun/Companion重复saga与receipt ledger。
- 0104–0105收敛conversation presentation与Agent input handoff命名。

迁移是forward-only hard cut；项目未上线，最终schema只表达当前owner模型。

## 主要实现检查点

- `4e0d90e7e`：建立本任务与第一性原理边界。
- `8b3234b9f`、`9952756ae`：删除Runtime执行投影与repository authority。
- `6e4f54a2e`、`30397c2be`：Host回归进程内并删除callback账本。
- `9d0e7d7cc`、`ea04a568e`：Product与Dash局部事实收回owner document。
- `21ab42055`：command retry由Agent inspection收敛。
- `8931a3dc5`、`4e0d90e7e`：live callback与真实terminal diagnostic进入Agent authority。
- `d1c34c834`、`4fc73e14d`：删除伪Runtime change/change-ledger合同。
- `ec104c6bd`、`328e0f315`、`7078fb072`：同步input handoff与Companion/Gate语义收口。

## 验证边界

最终质量门完成：

- 受影响 Runtime/Host/Native/Application/Infrastructure/API/Contracts crates `cargo check`；
- Host process-local restart/route tests 5/5；
- Dash source-scoped live delta 与真实 terminal inspection定向测试；
- Companion 63项、Workflow Gate 16项定向测试；
- frontend TypeScript typecheck；
- migration history guard；
- 当前开发库从既有schema forward migrate并通过readiness，schema version为105；
- 独立全新embedded PostgreSQL从空库完整migrate并通过readiness，schema version为105；
- production源码负向搜索与`git diff --check`。
