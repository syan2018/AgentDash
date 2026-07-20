# 数据库持久化边界清理

## Goal

从第一性原理重新审计 AgentDashboard 的 PostgreSQL 持久化边界，只保留进程死亡后仍必须成立、
无法安全重建的业务与执行事实。项目以 owner-owned JSONB aggregate 作为 canonical durable
authority；从 canonical JSONB 机械拆出的 normalized 镜像表、跨 owner 查询捷径以及把进程内
inventory、连接状态、派生能力或缓存错误建模为 durable authority 的 schema 与代码均应移除。

本任务作为可扩展的数据库持久化边界清理任务：首个工作包收敛 Complete Agent Host 的 live
service inventory；后续发现的同类误持久化问题直接在父任务中记录持久化判定并清理。只有确实
拥有独立产品交付、独立生命周期和独立验收边界的工作才拆为子任务。

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

满足该规则也不意味着同一事实需要同时存在于 JSONB aggregate 和 normalized 关系表。一个事实
只能有一个 canonical durable authority；只有独立业务 aggregate、独立事务 owner 或无法由
canonical aggregate提供的必要查询/claim语义，才允许建立独立表。

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

### R7. Managed Runtime JSONB aggregate 成为唯一事实源

- `agent_runtime_state_revision(thread_id, revision, facts)` 是每个 Runtime thread 的 canonical
  durable aggregate；operation、idempotency、pending command、source observation、normalized
  projection、change 与 outbox 在同一 revision CAS 中原子提交。
- 删除从 `facts` 机械拆出的 Runtime normalized 镜像表、写入器、漂移校验和 readiness 要求。
- Runtime read/change/outbox consumer 直接读取经过 domain validator 校验的 canonical facts；
  不通过关系镜像重建、验证或补写 Runtime authority。
- Product binding 对 Runtime/Host 的跨 aggregate reference 由 typed receipt、digest、revision 与
  source evidence 校验，不为建立数据库外键而复制另一份 Runtime/Host 事实。

### R8. Product change delivery 回归 Product owner

- 删除为每条 Runtime outbox 复制一行的全局
  `agent_runtime_product_change_delivery`。
- Product change consumer 只从已经存在最终 Product Runtime binding 的 AgentRun 出发读取
  canonical Runtime outbox；Create 早期 change 在 Product binding 提交后补消费，不作为异常。
- 每个 Product observer 拥有独立的 durable cursor、claim、attempt 与 error state；一个 observer
  失败不得重放其它已经成功的 observer。
- consumer state 与 Product Runtime binding 使用相同生命周期并以 JSONB 保存，不成为新的全局
  normalized delivery 表，也不进入 immutable Product binding digest。

### R9. Host 与 Callback 删除 normalized 镜像图

- `agent_runtime_host_revision` 是 Complete Agent Host durable coordination graph 的 canonical
  JSONB authority；target、binding、source、effect、lease、callback route 与 recovery evidence
  不再同时镜像到独立关系表。
- `agent_runtime_callback_revision` 是 callback reservation/outcome 的 canonical JSONB authority；
  删除 callback reservation/outcome 镜像表。
- Product、Runtime 和 Infrastructure 不直接 JOIN Host normalized 表；跨 owner 恢复读取通过
  Complete Agent Host typed interface/inspection evidence完成。
- Live Complete Agent catalog 继续完全 process-local；Host JSONB 只保存被执行选择冻结后的
  historical target/effect/recovery facts。

### R10. 领域语言与规范使用最终 owner 模型

- 根 `CONTEXT.md` 与 Runtime persistence/Host/Product specs 使用 07-17 最终四 owner 模型：
  Product、Managed Runtime、Complete Agent Host coordination 与 concrete Complete Agent。
- 文档记录为什么选择 owner-owned JSONB aggregate，不记录旧镜像实现的补丁或兼容方式。

### R11. Product Runtime binding 使用 canonical owner document

- `agent_run_product_runtime_binding.binding` 是 Product Runtime binding 的唯一 durable
  representation；target、RuntimeThread 与 launch frame scalar columns 只承担定位、唯一性和
  Product-local 约束。
- Product binding 只保存 Product 拥有的 target、RuntimeThread、精确 AgentFrame 与 execution
  profile；Runtime source/applied/activation evidence 只存在于 Managed Runtime owner document。
  repository 只从 canonical JSONB 解码并验证 scalar coordinate 与 stored digest。
- binding digest 使用递归 canonical JSON 并带 schema identity，不受内存 map 插入顺序或
  PostgreSQL JSONB key order 影响。
- binding commit/replacement 返回 committed receipt；Runtime Create/Activate、Host recovery
  与 presentation read 不向 Product binding 回写 Runtime evidence。
- digest contract hard cut 时清理旧 binding 与 resource attestation，因为旧 order-sensitive
  digest 无法继续证明内容身份。

### R12. Revision 只服务所属并发边界

- `ManagedRuntimeCommandEnvelope` 与 Product/API command request 不携带通用
  `expected_revision`；命令 admission 读取 current Runtime facts 并验证当前可用性。
- Steer/Interrupt、ResolveInteraction、Fork 以及 binding/source/generation 等并发要求使用
  各自的 typed business coordinate，不使用整体 projection revision 代替业务前置条件。
- Managed Runtime、Host、Callback、workflow 与 projection repository 的 revision 继续作为
  owner transaction 内部 CAS；snapshot/change revision 继续作为观察版本与 cursor evidence。
- Product launch、recovery、surface convergence 与 mailbox delivery 冻结稳定 operation、
  idempotency、binding/source identity；无关 projection 推进不使已经合法的命令失效。

### R13. Product 与 Host surface evidence 分属不同 owner

- Product activation 只持久化 canonical Product binding，不在 Product row 复制
  Complete-Agent binding id/generation。
- Product `surface_facts` digest 与 Host compiled/applied-surface digest 各自带独立 schema
  identity；前者覆盖 immutable AgentFrame 业务事实，后者覆盖 execution profile、编译后的
  requirements 与 apply evidence，二者不得直接比较。
- Host callback admission 负责 generation/source/applied-surface fencing；Product tool
  authorization 负责 binding digest 与 Product grant，并只用 RuntimeThread、
  source、AgentFrame surface revision 关联两个 owner。
- 已执行 `0091` 的开发库通过新的 forward migration 删除 Host pin columns，不修改既有 migration
  历史。

### R14. Product runtime authority 从 AgentFrame 即时派生

- Product binding 的精确 `launch_frame` 是 VFS/capability surface 的唯一 durable identity；
  runtime authority resolver 必须读取该 frame，不读取“最新 frame”绕过 binding。
- Task grant 是当前 Product subject association 的访问策略，工具调用时即时查询；新增关联立即
  授权，移除关联立即撤权。
- 删除 `agent_run_applied_resource_surface_snapshot/current`、materializer/repository、snapshot
  revision 和 Product activation pin。launch、recovery 与 surface update 不再编排资源物化阶段。
- 已执行旧 migration 的开发库通过新的 forward migration 删除表和列，并清理携带旧
  `resource_materialized` phase 的 recovery saga。

### R15. Managed Runtime applied evidence 保持单写

- Managed Runtime 是 source binding、committed/applied/activated revision 的唯一 writer。
- Product binding 不保存 Runtime evidence 副本；Runtime Create/Activate 和 recovery saga
  只验证 Product AgentFrame intent 是否已经被 Runtime 应用，不反向更新 Product document。
- Product command admission 比较 binding-pinned AgentFrame revision 与 Runtime 当前 applied
  surface revision；List 只按 Product-to-thread association 读取可选 Runtime summary，不读取
  command/recovery fence。
- Workspace presentation provenance 只保存产生该 intent 所需的 source coordinate 与 surface
  revision，不复制 Runtime projection revision；无查询用途的拆分关系列从 schema 删除。
- hard-cut migration 清理旧 Product binding/recovery/presentation 文档，避免旧双写 JSON 与新
  digest/协议并存。

### R16. 局域事实按归属文档聚合

- AgentFrame revision 属于单个 LifecycleAgent 的局部历史，不是全局 registry。最终存储应由
  LifecycleAgent owner document（或严格 agent-scoped 的局域文档列）承载，frame identity 只在
  该 owner 内定位。
- capability/context/VFS/MCP/execution/hook plan 是同一 frame revision 的一个 surface document，
  不以多组 canonical split columns 和跨表级联表达生命周期。
- 迁移前必须先收口 repository 查询为 agent-scoped 访问，消除仅凭 frame id 的全局扫描依赖；
  随后将 frame history 迁入 owner JSONB，并删除无独立业务生命周期的 transition/split storage。

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
- [ ] Runtime operation 从 Accepted 推进到 Running/terminal 后只更新 canonical JSONB，重启重放
  精确一致，不再存在 projection drift 路径。
- [ ] 最终 schema 不包含 Runtime、Host 或 Callback canonical JSONB 的 normalized 镜像表。
- [ ] Runtime Create 在 Product binding 提交前产生 outbox 不记录 warning；binding 提交后每个
  Product observer按自己的cursor有序补消费。
- [ ] 任一 Product observer 失败只release自己的claim，其它observer已推进的cursor不回退。
- [ ] Product/Infrastructure 无直接查询 Host normalized 表；recovery 使用 typed Host evidence。
- [ ] readiness、migration guard 与负向搜索共同证明被删除表名、镜像写入器和 drift invariant
  不再存在于生产路径。
- [ ] Product binding 多键/嵌套 JSON 经过 PostgreSQL 往返后 digest 稳定，repository restart 可
  复验 committed receipt，表内不再存在 execution profile/source revision 镜像列。
- [ ] Managed Runtime/Product/API command contract 不含通用 `expected_revision`；repository
  内部 CAS 与 command-specific guard 的回归测试通过。
- [ ] Product binding schema 不含 Host binding id/generation；不同 namespace 的 Product/Host
  surface digest 不再互相比较，forward migration 可升级已执行 `0091` 的开发库。
- [ ] Product runtime authority 只从 binding-pinned AgentFrame 与当前 Product association
  即时编译；最终 schema 不含 applied-resource-surface 全局表或 snapshot pin。

## Out of Scope

- 不增加旧 Complete Agent schema 的兼容读取、双写或 fallback。
- 不把普通索引优化、命名整理或无证据的 schema 风格偏好纳入本任务。
- 不把 Provider 配置、credential reference、Product execution profile 等真实产品配置移出数据库。
- 不改变 Managed Runtime、Host、Product 或 Complete Agent 的领域状态机；本轮只收敛同一事实的
  durable representation、跨 owner 读取方式和异步投递 ownership。

## Child Task Map

| Child | Scope | Dependency | Completion |
| --- | --- | --- | --- |
| `07-20-complete-agent-persistence-boundary` | Complete Agent live inventory、exact binding target、跨重启 fencing、discovery 事实源与 Host schema hard cut | 无；首个执行工作包 | 子任务独立测试、migration 与连续重启验收通过 |

父任务直接承载跨 Runtime、Host、Callback 与 Product delivery 的 normalized 镜像清理，因为这些
问题共享同一根因、同一 migration hard cut 和同一最终 schema 验收。后续只有具备独立产品边界的
交付才新增子任务。
