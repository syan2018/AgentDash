# Agent Runtime 持久化职责与事实边界清理实施结果

## 权威与持久化

- [x] Product 只持久化 LifecycleRun/LifecycleAgent、owner-local AgentFrame history、workflow/
  lineage 与 concrete Agent association。
- [x] LifecycleAgent 使用 `frames` 与 `runtime_binding` JSONB 归属局部事实；全局
  `agent_frames` 与 Product binding table 已删除。
- [x] Dash source 使用单个 canonical document；branch/history/command/effect/change 关系镜像
  已删除。
- [x] Create 前 effect receipt 保持 concrete Agent-owned，可由 `inspect(effect_id)` 查询。
- [x] Runtime、Host、Callback repository/revision schema 与生产组合已删除。
- [x] Product command claim、input queue、background delivery 与 recovery ledger 已删除。

## Command / Read / Stream

- [x] Product input 使用同步 `AgentRunProductInputDeliveryPort` handoff。
- [x] handoff/effect identity 由 Product target + client identity 稳定派生。
- [x] 成功结果始终携带 concrete Agent operation receipt；Agent unavailable直接返回 typed error。
- [x] command retry 使用相同 Agent effect + `inspect` 收敛，不依赖 Runtime operation repository。
- [x] conversation snapshot 直接读取 Complete Agent source并在内存中 normalize。
- [x] production Dash execution callback接入 source-scoped live event sink。
- [x] live delta只在当前进程broadcast；gap/断连通过authoritative snapshot恢复。
- [x] Agent terminal failure保留真实 code/message/retryability。

## Product / Workflow

- [x] AgentRun list/workspace在association缺失或Agent read失败时仍返回Product shell。
- [x] command/list不再以Runtime projection currentness、generic revision或surface mirror做gate。
- [x] LifecycleGate waiting items直接进入conversation response。
- [x] Companion、Routine、Workflow与human response统一调用Agent input handoff。
- [x] Companion continuation、Workflow AgentCall与Product protocol saga重复账本已删除。
- [x] channel/gate/routine只在owner-local document保存自身业务事实与下游handoff coordinate。

## Host / Callback

- [x] Complete Agent Host只保存当前进程attachment、target、binding、generation与callback route。
- [x] Host restart从Product association、当前Agent selection与Agent receipt重新建route。
- [x] callback route/generation/deadline在Host内存fence；真实Tool/Hook owner负责幂等receipt。
- [x] optional Agent program/credential/materialization不可用被隔离为typed unavailable diagnostic。
- [x] Runtime Wire跨进程状态网关从生产组合删除；Remote transport只保留真实placement职责。

## Schema Hard Cut

- [x] 0090–0096删除Runtime/Host/Callback持久化权威与Dash关系镜像。
- [x] 0097把AgentFrame与association收回LifecycleAgent owner document。
- [x] 0098–0103删除Workflow/AgentRun/Companion重复saga、receipt与continuation ledger。
- [x] 0104删除失效的conversation展示设置。
- [x] 0105把Routine/Gate局部receipt字段收敛为input handoff语义。
- [x] migration history guard覆盖forward-only迁移历史。
- [x] retired schema readiness/负向搜索不允许旧Runtime/Host/Callback owner重新进入最终schema。

## Specification

- [x] 重写 Runtime kernel、persistence、Host、AgentRun facade、Dash native adapter与conversation
  architecture。
- [x] 更新 database/repository/backend architecture、workflow/capability与frontend/backend
  snapshot/live contract。
- [x] 07-17任务由本任务最终权威模型收口。

## Verification

- [x] 受影响 Rust packages `cargo check`。
- [x] AgentRun conversation、Companion、Gate、Host、Dash与API定向测试。
- [x] frontend contract generation/typecheck。
- [x] migration history guard。
- [x] production源码负向搜索。
- [x] `git diff --check`。

详细结果见 [`closeout.md`](./closeout.md)。
