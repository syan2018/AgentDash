# W7 Product / Protocol 当前链路与切换准备度

## 结论

当前 Product / Protocol 生产链路仍围绕旧 Runtime driver 与
`RuntimeJournalFact` 工作。S2 已建立的 Complete Agent、Runtime target state 与 Native
target service 可以作为 W7 的隔离测试依赖，但现有 AgentRun fork、Companion、API feed
和前端 stream 不能通过局部换接口成为最终路径。

W7 的正确交付单元是：

1. Application-owned durable `AgentRunForkSaga`；
2. Full Companion 的 exact Agent fork，以及其它模式的 fresh create + typed initial
   package；
3. Runtime snapshot + committed change product feed；
4. production caller、canonical generated contract 与旧 journal 删除所需的完整
   activation inventory。

正式 migration、production composition、canonical generated artifacts 和 legacy
deletion 仍由 S5/W8 原子激活。

## 当前 AgentRun fork 链路

入口为：

```text
agentdash-api::routes::lifecycle_agents::fork_agent_run
  -> AgentRunJournalService::load_visible_journal_page
  -> AgentRunProductCommandService
  -> AgentRunForkCommandService::materialize
  -> AgentRunForkGraphStore::create_graph
  -> AgentRunRuntimePort::fork_runtime
  -> runtime_facade::fork_runtime_inner
  -> HostAgentRunRuntimeProvisioner::provision
  -> IntegrationDriverHost::bind(DriverBindIntent::Fork)
  -> concrete legacy driver
```

当前 cutoff 由 journal 的 `source_turn_id` / `entry_index` 反解
`fork_point_event_seq`。产品 child graph 在 Runtime/Agent effect 前创建；Runtime
provisioner 在一次调用内完成 binding 与 activation；失败时同步删除 product graph。

当前可复用 durable primitives 包括：

- 预分配 child LifecycleRun、Agent、Frame、PresentationThread 与 lineage identity；
- product command receipt；
- product graph / lineage repository；
- Runtime child thread / binding identity；
- Host pending/active binding、offer 与 recovery intent；
- target Complete Agent lane 的 stable command/effect、inspect/reconcile、
  generation、lease 与 surface evidence。

目标仍缺少：

- `AgentRunForkSaga` repository、phase 与 worker；
- immutable cutoff 和跨 Product / Runtime / Host / Agent 的 stable identities；
- `RuntimeAdmitted`、`AgentForkApplied`、`RuntimeProvisioned`、product graph commit 与
  explicit activation 的分段 evidence；
- unknown-outcome inspect/reconcile；
- `Lost` 时保留 known child coordinate 并禁止第二次 fork；
- 任一 durability boundary 后从 durable phase 继续。

因此目标顺序必须是：

```text
Requested
  -> RuntimeAdmitted
  -> AgentForkApplied
  -> RuntimeProvisioned
  -> ProductGraphCommitted
  -> RuntimeActivated
  -> Succeeded
```

## 当前 Companion 链路

`application::companion::dispatch_child` 对 Full 仅选择
`ContextPolicy::Inherit`，其它模式选择 Slice；所有模式最终都通过
`LifecycleDispatchService` 创建 fresh child，并以 `fork = None` provision Runtime
thread。

`frame_construction::assembly::apply_companion_slice`、
`companion::tools::build_companion_execution_slice` 和 capability resolver 当前负责裁剪
VFS、MCP、capability 与 bundle，随后把 slice 编进普通 prompt / RuntimeInput。

目标边界为：

- Full：复用 `AgentRunForkSaga`，要求 Bound surface 提供 exact completed-Turn fork；
- Compact / WorkflowOnly / ConstraintsOnly：fresh create 原子携带平台中立
  `InitialAgentContextPackage`；
- package 的 stable ID、mode、authority、revision、digest 与 applied fidelity 由
  create receipt / inspect 证明；
- child 只在 package evidence 成功后 activation；
- dispatch task 是之后独立的首个 `SubmitInput`；
- Workspace、VFS、Tool、Hook、credential 与 capability grant 继续由 Surface 交付。

## 当前 journal / RuntimeSession 消费者

### Application 与 API

- `agentdash-application-agentrun/src/agent_run/journal.rs` 直接读取和订阅
  `RuntimeJournalRecord`，并用 `RuntimeJournalFact::Presentation` 形成产品 feed；
- inherited visible history 通过 ancestor journal + lineage event sequence 拼接；
- lifecycle Agent API 的 journal page 与 NDJSON stream 返回 legacy
  `SessionEventResponse` / `BackboneEnvelope`；
- stream lag 当前继续等待，尚未形成 cursor gap → snapshot reload。

### Runtime、Wire 与 Relay

- legacy Runtime contract、driver、wire 与 remote path 仍传递
  `RuntimeJournalFact`；
- Codex App Server notification 直接投影为 journal fact；
- Runtime Wire 仍含 journal notification；
- Relay 仍含 `EventRuntimeSessionStateChanged`。

### Frontend

以下模块仍以 journal page + NDJSON stream 维护 UI 状态：

- `packages/app-web/src/services/agentRunRuntime.ts`
- `packages/app-web/src/features/session/model/useSessionStream.ts`
- `packages/app-web/src/features/session/model/streamTransport.ts`
- `packages/app-web/src/features/session/model/sessionStreamReducer.ts`
- `packages/app-web/src/features/session/model/useSessionFeed.ts`
- `packages/app-web/src/features/session/model/SessionChatViewModel.ts`

generated session/runtime/wire/workflow/mailbox/backbone contracts 仍包含 journal 或
`RuntimeSession` delivery/live vocabulary。它们应在 S5 与 production callers 一次
生成和切换。

## S4 target lane

S4 可以在不改变 production route 的前提下，用 test-only composition 直接装配：

```text
in-memory AgentRunForkSaga repository
  -> target Runtime state / CompleteAgentHost
  -> Native or fixture CompleteAgentService
  -> target Runtime snapshot + committed change
```

隔离测试需要覆盖：

- fork 每个 phase 的 restart；
- deterministic effect replay；
- unknown inspect 与 known-child `Lost`；
- initial package digest/fidelity evidence；
- activation 前置条件；
- package 与首个 input 的不同 identity 和顺序；
- Runtime snapshot/change ordering 与 cursor gap reload。

S4 target tests 使用 Rust target DTO 或临时输出目录，不覆盖 router、AppState、production
provisioner、canonical generated Rust/TypeScript artifacts 或正式 schema。

## S5 activation inventory

W7 owner 需要冻结：

- AgentRun fork command、runtime facade、journal/feed 和 product submission callers；
- durable saga contract/repository/state machine；
- Companion dispatch、slice compilation、frame assembly、request assembly 与 capability
  resolution；
- API fork、feed、stream、inspect/context/compaction callers；
- frontend Runtime service、stream transport、snapshot/tail reducer、feed 与 view model；
- target Rust AgentRun snapshot/change/operation DTO；
- canonical session/runtime/wire/workflow/mailbox/backbone contract diff。

W8 在同一 hard cut 中负责：

- final saga、Runtime、Host、Dash repository schema 与 migration；
- production composition / AppState / service registry；
- legacy journal、RuntimeSession delivery/live ports、旧 driver/wire/relay variants；
- legacy crates、workspace entries、lockfile 与最终 canonical generation；
- final dependency、negative、migration 与 tracer gates。

这一分工让 W7 证明产品行为和 caller intent，W8 只负责原子集成与删除，不重新解释
产品或 Agent 语义。
