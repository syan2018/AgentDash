# Complete Agent 持久化边界收敛

## Goal

将 Complete Agent 的“当前可用 adapter”恢复为进程内 live catalog，并让 PostgreSQL 只保存
不可重建的 Runtime binding、effect、source、lease 与恢复证据。跨进程重启必须通过新的 live
attachment 和更高 binding generation 显式恢复，不能原地复用旧 placement 或把旧 binding
静默路由到新进程。

本任务是父任务 `07-20-database-persistence-boundary-cleanup` 的首个实现工作包，必须遵守父任务的
Persistence Decision Rule。

## Confirmed Facts

- `pnpm run dev:server` 在已有开发数据库上稳定复现
  `service instance id is already registered with different verified facts`。
- 当前数据库只有 `builtin.codex-app-server.default`，已保存协议版本与当前代码均为 `144`；
  冲突坐标是跨启动变化的 `host_incarnation_id`。
- API Host 每次启动生成新 incarnation
  （`crates/agentdash-api/src/app_state.rs:433`）。
- `register_verified_service` 把 descriptor、verification、offer 与 placement 作为同一
  `service_instance_id` 的不可变事实
  （`crates/agentdash-agent-runtime-host/src/complete_agent.rs:375-430`）。
- repository 与 schema 都要求一个 service instance 恰有一个不可变 placement
  （`crates/agentdash-agent-runtime-host/src/complete_agent_repository.rs:243-336`；
  `crates/agentdash-infrastructure/migrations/0084_agent_runtime_complete_agent_hard_cut.sql:984-994`）。
- `CompleteAgentBinding` 没有 exact attachment/incarnation 坐标，dispatch 只按
  `service_instance_id` 解析当前进程 handle
  （`crates/agentdash-agent-runtime-host/src/complete_agent.rs:58-67,1855-1863`）。
- 项目尚未上线，允许通过新 migration 明确丢弃错误模型下的开发态 Host/binding/effect 数据；
  不保留兼容读取、双写或 fallback。

## Requirements

### R1. Process-local Live Complete Agent Catalog

- Live catalog 管理当前进程内成功 materialize 并通过 Host 验证的 Codex、Native 与 Runtime Wire
  adapter。
- catalog entry 至少拥有 exact `LiveAttachmentId`、逻辑 instance key、descriptor、verification
  evidence、effective offer、placement/incarnation、remote mapping（如适用）和 live service
  handle。
- `LiveAttachmentId` 必须覆盖当前 Host incarnation 与 placement identity；不能只等于稳定逻辑
  instance key。
- 同一 Host incarnation 内完全相同的 attach 幂等；相同 attachment identity 的不同事实 typed
  reject。
- catalog、当前 availability 和 service handle 不写 PostgreSQL，进程退出后全部重建。

### R2. Static、Dynamic 与 Remote Materialization

- Codex contribution 在启动时尝试 attach 到 live catalog。
- Native adapter 在选择 `PI_AGENT` 后根据 immutable Product profile、Provider、Model 与
  credential scope 按需 materialize，并在当前 incarnation 内缓存/复用相同 attachment。
- Runtime Wire adapter 随当前受信连接 attach；连接 epoch 结束后旧 attachment 永久 retired。
- optional adapter materialization/describe/unavailable 失败只产生诊断和 catalog 缺席，不终止
  AgentDashboard；Host verification、descriptor mismatch 等当前 attachment 完整性错误 typed
  reject，但不能污染 durable 执行事实。

### R3. Exact Durable Binding Target

- Durable binding/Runtime target 保存绑定当时的 exact target snapshot，包括：
  logical instance key、live attachment identity、service definition、verified build/profile、
  offer profile、placement/incarnation，以及 remote identity/generation mapping（如适用）。
- lifecycle effect、dispatch effect、source coordinate、callback route 与 lease 以 binding identity
  + generation 为主要执行坐标，不依赖全局当前 service inventory。
- dispatch 必须同时校验 binding、generation、attachment、incarnation、lease epoch/token 与 source
  coordinate，并通过 exact attachment identity 解析 live handle。

### R4. Restart and Recovery

- 新 Host incarnation 不得让旧 binding 重新变为 dispatchable。
- Recovery planner 只能选择当前 live catalog 中兼容的 attachment，并创建更高 generation。
- 旧 generation 的 command、event、callback 和 lease 永久 fenced。
- 未决 effect 保留原 effect identity、payload digest 与 source evidence；恢复后先 inspect/reconcile，
  不直接重发或宣称确定失败。
- 无兼容 live attachment 时保持 typed unavailable/lost/inspection-required 语义，不伪造成功或
  fallback 到其它 adapter。

### R5. Correct Discovery Authority

- `PI_AGENT` 的已知性来自内建 Product profile，可用性来自当前用户 executable Provider catalog。
- `CODEX` 的可用性来自本次 Host incarnation 的 live catalog/materialization diagnostic。
- execution profile discovery 与 project-agent profile validation 不读取 durable Host inventory
  判断当前 availability。

### R6. PostgreSQL Hard Cut

- 通过新的 migration 删除 Complete Agent 全局当前 inventory：
  `agent_service_instance`、`agent_service_verification`、`agent_runtime_offer`、
  `agent_runtime_placement` 与 `agent_runtime_remote_binding`。
- binding/Runtime target/effect schema 改为保存 exact target snapshot，并移除指向全局 service
  inventory 的外键。
- migration 明确清理错误模型下的开发态 Host-owned binding/effect/target 及其产品引用；不修改
  已应用 migration 文件，不保留旧数据转换或兼容层。
- canonical Managed Runtime journal/projection 与 Product execution profile 配置不因本次 hard cut
  被误删。

## Acceptance Criteria

- [ ] Host public seam 的回归测试能够在修复前稳定复现“同逻辑 Codex instance 跨两个 Host
  incarnation attach”的冲突，并在新模型下证明两次 attach 产生不同 live attachment。
- [ ] 连续两次 `pnpm run dev:server` 均成功启动，不因 service registration 或 incarnation 变化退出。
- [ ] Codex 当前启动 materialization 失败时核心应用继续启动，discovery 显示当前不可用。
- [ ] executable Provider 存在时 `PI_AGENT` 可选择；相同 Product profile 在当前 incarnation 内
  重复选择只 materialize 一个 Native attachment。
- [ ] binding 固定 exact attachment/incarnation；重启后旧 binding 无法命中新 service handle。
- [ ] recovery 成功时 generation 单调增加，新 binding 固定新 attachment，旧 command/event/callback
  均被 fence。
- [ ] 未决 effect 跨重启保持同一 identity 并通过 inspect/reconcile 收敛，无重复派发。
- [ ] 最终 PostgreSQL schema 不包含 Complete Agent 全局当前 inventory 表或等价 JSON map。
- [ ] migration 从当前 schema 顺序升级成功，空库最终 schema 一致，migration guard 通过。
- [ ] execution profile discovery 不读取 durable Host inventory 判断当前可用性。
- [ ] Host、Infrastructure、Integration、API 定向测试和相关 fmt/check/clippy 通过。

## Out of Scope

- 不保留错误 Complete Agent inventory schema 的兼容读写。
- 不改变 Provider 配置、credential reference 和 Product execution profile 的 durable ownership。
- 不重写 canonical Managed Runtime journal/projection 状态机。
- 不把普通数据库索引、命名或无关表清理并入本子任务。
