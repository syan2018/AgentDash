# S5 Atomic Hard Cut 就绪审计

> 历史阶段记录，当前收尾以 `final-convergence-closeout.md` 为准。Product lane 的最终
> checkpoint 还需要 Companion、Routine、Workspace/Canvas/Terminal、Lifecycle VFS、
> Wait、Capability 与 canonical UI 的 production caller/tracer。本文保留当时的 owner
> input，供 final replacement manifest 参考。

## 1. 审计范围与结论

本审计最初基于 S3 完成后的代码，现已按 S4 Product target lane 集成后的
`09fbaaa0` 重新校准。

S4 已形成稳定的 target-only checkpoint：production 仍选择当前
Runtime → Driver Host → Native/Codex driver 路径；W7 已具备 durable Fork/Companion
编排、canonical Runtime snapshot/change 消费、typed gap reload 和可复现的 generated
contract 输入。

S5 尚未达到 activation-ready。剩余工作不是保留两条 production path 的理由，而是四个
领域 owner 必须在同一冻结 revision 上刷新并共同签认的原子切换输入。

## 2. Cutover 前必须交付的 owner inputs

### Platform Runtime

- 将 `CompleteAgentHost` 内部的 `RwLock/Mutex<BTreeMap>` coordination store 提升为
  durable repository port，使 W8 能在不绕过 Host 边界的前提下实现 PostgreSQL adapter。
- 冻结最终 Runtime Contract、Complete Agent contract 和 Host persistence shape，覆盖
  Runtime operation/change/projection 与 Host binding/source/effect/lease 事务。
- 清除 `InMemoryCompleteAgentStateRepository` 的 test-support gate：真实 port 保留在
  owner crate，recording fixture 按项目门禁放置或命名。
- 提供 `CompleteAgentHost`、`CompleteAgentStateReconciler` 的 production activation。
- 在 S5 canonical generator change 中纳入 `ManagedRuntimeProjectionSchema` 及 Complete
  Agent/Host callback/change roots；production Wire revision 3 与 target revision 4 只在
  cutover 时收敛为唯一最终合同。

### Dash / Native

- 将已评审的 W2 Agent/Core physical component 从旧 patch base 刷新到 S4 冻结 revision，
  不引入临时 Application → Core 依赖。
- 让完整九 consumer final-owner 矩阵与 physical move 同步，在同一 S5 set 删除 serde
  transcode 和 `agentdash-agent-types`。
- 清除 `MemoryDashAgentRepository` test-support gate，并把最终 Dash repository
  transaction/schema contract 交给 W8。
- 提供 Native production service registration，并证明 product fork 使用 exact Dash
  history，而不是空 source binding。

### External Agents

- 在冻结 revision 上刷新 Codex 与 Remote Complete Agent registration。
- 删除 adapters 所拥有的 legacy driver/journal/context-activation production paths。
- 保持 Codex ThreadStore source authority、Remote reverse callback/change 和已评审的
  unknown-outcome/generation fences。

### Product / Protocol

- 将 production AgentRun/Fork/Companion/API/App Server/UI callers 从 journal/driver-era
  路径切到 W7 Runtime Contract target lane。
- 通过最终 repository 和 migration 持久化 Product `agent_run_fork_saga`。
- 在切换 production API/UI consumer 的同一 cutover 中，将 task-local W7
  schema/fixture manifest 应用到 canonical generator。
- 按 W7 九 consumer activation manifest 删除 Product Core-tool assembly、
  `RuntimeSession*` delivery/live DTO 和 journal-driven feed consumers。

## 3. Production composition 热点

当前路径由以下位置选择：

- `crates/agentdash-api/src/integrations.rs`：
  `HostIntegrationRegistration.runtime_driver_contributions`、`agent_runtime_drivers`；
- `crates/agentdash-integration-api/src/integration.rs` 与 `src/agent_runtime.rs`：
  `AgentDashIntegration::agent_runtime_drivers`、
  `AgentRuntimeDriverContribution/Factory/Driver`；
- `crates/agentdash-api/src/app_state.rs`：
  `AgentServiceDefinitionRegistry`、`build_native_agent_runtime_composition`、
  `IntegrationDriverHost`、`AgentRunJournalService`；
- `crates/agentdash-infrastructure/src/agent_runtime_composition.rs`；
- Native/Codex legacy `driver.rs` 与 `contribution.rs`；
- `crates/agentdash-local/src/agent_runtime_host.rs` 与 Runtime Wire handlers；
- lifecycle Agent API 的 fork/journal/context/compaction routes；
- frontend `agentRunRuntime.ts`、`streamTransport.ts`、`sessionStreamReducer.ts` 和
  `useSessionFeed.ts`。

目标实现已存在于 Runtime/Host `complete_agent*`、Native/Codex/Remote Complete Agent
service modules、`RuntimeWireAgentServiceEndpoint`、W7 target product modules 与 managed
Runtime projection contract。S5 必须在同一 staging change set 接通这些模块并删除旧
registration。

## 4. Legacy fact 与 crate ownership

### RuntimeJournalFact

- Platform Runtime 负责 Runtime/Host/wire/infrastructure journal removal。
- Product 负责 AgentRun presentation/context/API/UI journal consumers。
- Dash/Native 与 External Agents 负责各自 adapter-produced journal removal。
- W8 只在上述 owner diff 清零 consumer 后删除 schema/generated/空 crate 残留。

### RuntimeSession 与 connector-era state

- Product 删除不满足 history-maintained 定义的 Application/Lifecycle/Workflow/API/
  contracts/frontend callers 与 DTO。
- W8 删除已迁空的 application ports、extension-gateway legacy modules 与 Relay
  `EventRuntimeSessionStateChanged`，并完成 extension gateway 迁名。
- `AgentConnector` 字面符号已不存在。剩余 connector-era chain 是
  `agentdash-spi::connector` tool assembly、executor MCP discovery 与 runtime-gateway
  adapters。
- Platform/External owners 删除 `ContextActivation*` 业务路径；W8 删除六张
  `agent_context_*` 表。

### S3 audit base 的 direct crate consumers

- `agentdash-agent-types`：14 个 direct consumers；W2 component 移除其中 5 个，其余 9 个
  已在评审矩阵中逐项分配 final owner。
- `agentdash-agent-protocol`：18 个。
- `agentdash-executor`：1 个。
- `agentdash-spi`：19 个。
- `agentdash-application-hooks`：2 个。
- `agentdash-application-runtime-gateway`：3 个。

领域 owner 先切 callers。W8 随后删除 types/protocol/codegen/executor/hooks，将 SPI
收敛为 `agentdash-platform-spi`，将 runtime-gateway 收敛为 extension gateway，并更新
workspace/lockfile。

## 5. Final migration 与 generated contract

S5 前 migration head 为 `0083_remove_agent_frame_workspace_module_projections.sql`。
唯一 forward migration 必须建立：

- Product：`agent_run_fork_saga`；
- Runtime：pending command、normalized projection/turn/item/interaction、change/outbox、
  surface；
- Host：service instance、binding、source coordinate、effect、lease；
- Dash Agent：history-maintained session、history/branch，以及独立的 command/effect/
  change。

约束必须覆盖 idempotency、active slot、binding generation、effect identity、projection
CAS 与 Dash history-head CAS。W8 在同一 change set 为冻结的 ports 实现 PostgreSQL
adapters，并删除最终 owner 不再读取的 `agent_runtime_event`、`agent_context_*`、旧 Host
driver/lease/coordinate 与 AgentRun anchor/lineage/recovery-intent schema。

W7 task-local `ManagedRuntimeProjectionSchema` 是 activation input，不是第二套公共 schema。
S5 同时更新唯一 canonical generator 和全部 Rust/TypeScript callers。

## 6. 集成顺序

1. 冻结 S4 与唯一 S5 base revision。
2. Platform Runtime、Dash/Native、External Agents、Product/Protocol 在同一 revision
   刷新 activation sets。
3. 签认 Platform contract 与 Host repository seam。
4. 应用 Dash Agent/Core physical component 与 final consumer switches。
5. 激活 External Complete Agent registrations。
6. 激活 Product AgentRun/API/UI callers 与 canonical generated contracts。
7. 增加 W8 PostgreSQL adapters、唯一 migration 与 production composition。
8. 删除 legacy routes、crates、schema 和 generated symbols。
9. 重新生成 workspace lock/contracts，并执行 architecture 与 behavior 双重检查。

任何 owner-specific failure 都返回对应 bundle。W8 不通过 facade 或绕过缺失 domain seam
完成集成。

## 7. S5 gates

Architecture gates：

- migration guard 与 PostgreSQL/in-memory behavior equivalence；
- `cargo metadata` dependency DAG 与 Core purity；
- canonical generator check；
- old crate/type/table/route consumer count 为零；
- production composition 只保留 Complete Agent registration。

Behavior gates：

- Native/Codex direct input 与 exact fork；
- Companion Full exact fork、Fresh package/evidence/first-input ordering；
- Dash/Codex compaction；
- in-process 与 Remote Tool/Hook callbacks；
- Runtime snapshot/change cursor-gap reconnect；
- unknown outcome、stale generation、duplicate、disconnect 与 restart recovery；
- 删除旧路径后重跑 Rust、API、frontend typecheck/feed 和真实 production tracer bullets。
