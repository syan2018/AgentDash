# 数据库持久化边界清理

## Goal

从第一性原理重新审计 AgentDashboard 的 PostgreSQL 持久化边界，只保留进程死亡后仍必须成立、无法安全重建的业务与执行事实，移除把进程内 inventory、连接状态、派生能力或缓存错误建模为 durable authority 的 schema 与代码。

本任务作为可扩展的数据库持久化边界清理任务：首个工作包收敛 Complete Agent Host 的 live service inventory；后续发现的同类误持久化问题可以在完成影响分析后追加为独立、可验收的工作包。

任务范围限定为违反本 PRD Persistence Decision Rule 的误持久化状态。普通字段命名、索引优化、
schema 风格整理和历史 migration 美化不纳入本任务，原因是这些工作不共享相同的正确性风险与验收
方式。

## Background

- `pnpm run dev:server` 可稳定复现启动失败：
  `Complete Agent host invariant failed: service instance id is already registered with different verified facts`。
- API Host 每次启动都会生成新的 `host_incarnation_id`
  （`crates/agentdash-api/src/app_state.rs:433`）。
- Complete Agent Host 以稳定的 `service_instance_id` 同时索引 descriptor、verification、
  offer 与 placement，并要求已有事实逐字相同
  （`crates/agentdash-agent-runtime-host/src/complete_agent.rs:375-430`）。
- Host repository 要求每个 service instance 恰有一个 placement，并将 placement 历史视为不可变
  （`crates/agentdash-agent-runtime-host/src/complete_agent_repository.rs:243-336`）。
- PostgreSQL 中 `agent_runtime_placement.service_instance_id` 是主键，而同一行包含每次启动变化的
  `host_incarnation_id`
  （`crates/agentdash-infrastructure/migrations/0084_agent_runtime_complete_agent_hard_cut.sql:984-994`）。
- 当前开发数据库只有 `builtin.codex-app-server.default` 一条 Complete Agent service instance；
  已保存的协议版本与当前代码同为 `144`，冲突来自跨重启 incarnation 变化，不是 descriptor 或
  verification 版本漂移。
- `CompleteAgentBinding` 不保存 exact live attachment / host incarnation，dispatch 仅按
  `service_instance_id` 解析进程内 handle
  （`crates/agentdash-agent-runtime-host/src/complete_agent.rs:58-67,1855-1863`）。
  因此允许原地覆盖 placement 会使旧 generation 静默路由到新进程，不能作为修复方案。
- 项目尚未上线，不需要保留错误 schema 的兼容读取或双轨回退；已应用 migration 历史仍通过新
  migration 收敛，并明确处理开发态数据。

## Persistence Decision Rule

一份状态只有同时满足下列条件才进入 durable schema：

1. 进程完全退出后该事实仍必须成立；
2. 无法从代码、产品配置、当前连接或受信输入安全重建；
3. 丢失后会破坏业务正确性、幂等性、恢复能力或安全 fencing；
4. 它拥有明确的 authority、生命周期和更新规则。

不满足该规则的 live inventory、健康状态、连接、进程 incarnation、派生 offer、缓存与当前可用性
留在进程内，由启动或连接建立时重建。

## Requirements

### R1. 建立可追加但有统一准入规则的清理清单

- 每个工作包必须记录当前错误持久化、真实 authority、删除或迁移方案、受影响读写链路和验证方式。
- 后续问题只有符合本 PRD 的 Persistence Decision Rule 才纳入本任务。
- 各工作包必须能独立完成和验收，不能以“通用清理”为由无限扩大单次改动。

### R2. Complete Agent live catalog 回归进程内

- Codex、Native 与 Runtime Wire 当前可用的 Complete Agent adapter 由 process-local live catalog
  管理。
- descriptor、当前 verification 结果、offer、placement、host incarnation 与 live service handle
  不再作为全局当前 inventory 持久化。
- Codex 在启动时 materialize；Native 按 Product execution profile、Provider、Model 与
  credential scope 按需 materialize；Remote adapter 随 Runtime Wire 连接 attach。
- optional adapter 不可用时只产生不可用诊断并从 live catalog 缺席，不终止 AgentDashboard 核心应用。

### R3. Durable Host 只保存不可重建的执行事实

- 保留 canonical Runtime target、binding generation、source coordinate、effect、
  idempotency、receipt/inspection evidence、outbox、lease epoch 与 surface/hook applied evidence。
- binding 持有绑定当时 exact live selection 的不可变快照，包括 service definition/build/profile、
  attachment/incarnation 与 placement/remote coordinates。
- durable lifecycle/effect 以 binding identity + generation 为执行坐标，不依赖一个全局当前
  service inventory 外键。

### R4. 跨重启必须显式恢复并提升 generation

- 新 Host incarnation 不能直接接管旧 binding。
- dispatch 同时 fence binding、generation、live attachment、host incarnation、lease 与 source
  coordinate。
- recovery 只能在找到兼容的当前 live adapter 后创建新 generation；旧 attachment/generation
  永久不可 dispatch。
- 未决外部副作用继续以原 effect identity inspect/reconcile，不能因重启直接重发或宣称失败。

### R5. Execution profile discovery 使用正确事实源

- `PI_AGENT` 的已知性来自产品内建 profile；可用性来自当前用户的 executable Provider catalog。
- `CODEX` 的可用性来自本次进程 live catalog / materialization diagnostic，不读取历史数据库
  service instance。
- 未知 profile 继续 typed reject。

### R6. Schema 通过干净 migration 收敛

- 删除作为全局当前 inventory 存在的 `agent_service_instance`、`agent_service_verification`、
  `agent_runtime_offer`、`agent_runtime_placement` 与 `agent_runtime_remote_binding`，或将确属
  binding 历史的字段并入 exact binding snapshot。
- 调整 lifecycle target、binding、effect、source、callback 与 lease 外键，使其围绕 durable
  binding/generation 建模。
- 新 migration 可以明确丢弃尚未上线的开发态 Complete Agent Host/binding/effect 数据，不实现
  旧 schema 兼容层。
- migration 顺序升级、空库初始化和最终 schema 必须一致。

## Acceptance Criteria

- [ ] 连续两次执行 `pnpm run dev:server` 不因 Complete Agent 注册或 Host incarnation 变化退出。
- [ ] Codex 当前启动 materialization 失败时应用继续启动，discovery 显示 Codex 不可用且包含诊断。
- [ ] 存在 executable Provider 时 `PI_AGENT` 可选择，并在首次使用时 materialize Native live adapter。
- [ ] 相同 Product profile 在同一 Host incarnation 内重复选择幂等，不产生重复 live attachment。
- [ ] Host 重启后旧 binding 不能通过相同逻辑 instance key 命中新 service handle。
- [ ] 恢复成功时产生更高 generation 并固定新的 attachment/incarnation；旧 generation 的
  command、event 和 callback 均被 fence。
- [ ] 未决 effect 在重启后按同一 effect identity inspect/reconcile，不发生确定性重复派发。
- [ ] PostgreSQL 最终 schema 不再保存可重建的 Complete Agent 当前 inventory。
- [ ] execution profile discovery 不再读取 durable service inventory 判断当前可用性。
- [ ] migration guard、定向 Host/Runtime/API tests、Rust fmt/check/clippy 与相关前端契约检查通过。
- [ ] 每个后续追加的清理工作包都具有独立的问题证据、持久化判定、迁移计划和验收项。

## Out of Scope

- 不因本任务重写 canonical Managed Runtime journal/projection 语义。
- 不增加旧 Complete Agent schema 的兼容读取、双写或 fallback。
- 不把普通索引优化、命名整理或无证据的 schema 风格偏好纳入本任务。
- 不把 Provider 配置、credential reference、Product execution profile 等真实产品配置移出数据库。

## Child Task Map

| Child | Scope | Dependency | Completion |
| --- | --- | --- | --- |
| `07-20-complete-agent-persistence-boundary` | Complete Agent live inventory、exact binding target、跨重启 fencing、discovery 事实源与 Host schema hard cut | 无；首个执行工作包 | 子任务独立测试、migration 与连续重启验收通过 |

后续子任务只有在记录问题证据、authority 判定、migration 影响和独立验收标准后才能追加；父任务不
直接承载实现代码。
