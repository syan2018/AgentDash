# Research: 当前 Runtime / fork 状态边界与 07-17 ownership 评估

- Query: 基于当前分支代码、数据库 migration 与现有测试，确认 AgentRun fork、Companion 继承、Runtime journal/session/context/compaction、Host binding/effect/recovery 的真实边界，并评估 07-17 `AgentSession` 方案是否正确保留统一外层 Agent Runtime、同时区分平台 canonical 状态与完整 Agent 实现的内部状态。
- Scope: internal
- Date: 2026-07-17

## Findings

### 1. 结论摘要

#### 确认事实

1. 当前分支已经存在完整的统一外层链路：
   `Application / AgentRun facade -> Managed Agent Runtime -> Integration Driver Host -> Driver adapter`。它已经维护 AgentDash canonical command、operation、snapshot/journal、Surface/Tool/Hook admission、service offer、binding、placement、lease 与 recovery；这层不能因重新划分 Agent implementation ownership 而被删除。
2. 当前 Integration seam 的代码形态仍是 `AgentRuntimeDriverContribution + AgentRuntimeDriverFactory + AgentRuntimeDriver`，而不是“完整 Agent service”合同。Codex 与 Native 都被挂在同一个 Managed Runtime 下，由外层 Runtime 统一落 `Thread/Turn/Item/Context/Compaction` 状态。
3. 当前 AgentRun fork 已是可用能力：
   - 产品层创建 child LifecycleRun/LifecycleAgent/AgentFrame/AgentRunLineage；
   - Runtime provisioner 用 `DriverBindIntent::Fork` 创建 child binding；
   - visible journal 通过 `AgentRunLineage` 逐祖先拼接 presentation prefix；
   - 当前 fork 过滤测试 5/5 通过。
4. 当前 fork 不是一个单一原子事务。产品 receipt、child graph、driver bind、surface publication、AgentRun runtime binding 分属多个提交边界；运行时失败有补偿，但没有发现覆盖进程在任意边界崩溃的 durable saga。
5. 当前 Codex fork 会真正调用 `thread/fork`；当前 Native 的产品 fork 只创建新的 source binding，未把父会话 history/checkpoint 带入。Native 另有支持 checkpoint import 的 `RuntimeCommand::ThreadFork`，其单测通过，但产品 fork 链没有构造该命令。
6. 当前 Companion dispatch 不是 AgentRun fork。它在同一 LifecycleRun 内建立 parent/child AgentLineage、继承 capability policy，并把 Hook snapshot/injection/constraint 组装成首条 prompt；它没有调用 `AgentRunLineage`、`DriverBindIntent::Fork` 或读取父 Agent 的 canonical session history。
7. 当前 `RuntimeJournalFact` 是统一 Runtime 的持久化 record envelope，包含 presentation 与 internal 两类 fact；AgentRun feed 只投影 presentation。fork visible history 依赖 ancestor lineage、ancestor 当前 runtime binding 以及这些 binding 的 journal 可读性。
8. 07-17 设计中 snapshot/change、stable effect identity、generation fence、compaction saga、typed Turn、queue promotion 等机制有直接价值；但它把所有实现都降为“只返回 receipt/observation 的 execution adapter”，并让一个平台 `AgentSession` aggregate 同时拥有 Session、Mailbox、Turn/Item、Context、Compaction、部分 binding/effect consistency。按本轮最终 ownership 基线，这会混淆四种不同权威：
   - 平台统一 Runtime 的 canonical contract / operation / normalized snapshot-change；
   - 完整外部 Agent 的内部会话状态（平台只读）；
   - 平台只能通过有限 Agent command 改变的状态；
   - Surface、binding、effect、placement、lease、recovery 等 execution coordination。
9. 正确方向不是删除统一 Runtime，而是保留其外层 canonical contract 与 capability/coordination 能力，同时让不同完整 Agent 实现按 `RuntimeOffer` / `BoundAgentSurface` 的 semantic strength 暴露有限命令与只读状态：
   - 自有 Agent：内部 Agent layer 拥有 history/fork/context/compaction，向下调用 pi-like stateless AgentCore；
   - Codex、pi-coding-agent、企业 Agent：各自是完整 Agent，实现拥有自己的内部状态；
   - 平台外层 Runtime 负责统一产品可见合同、能力求交、operation、projection、binding/effect/recovery，不冒充外部 Agent 内部状态的写 owner。

### 2. 证据口径

- “当前事实”只来自当前分支代码、migration 与本次实际执行的测试。
- `07-10-agent-runtime-architecture-convergence/design.md` 与 `target-crate-shape.md` 只作为原始目标文档，不用来证明当前实现。
- `07-17-agent-runtime-compaction-state-protocol-review/design.md` 是待评估方案，不作为当前实现证据。
- 没有把“缺少文档/测试”写成“能力已回归”。只有静态代码未接线时才写“未接线”，只有本次实际测试通过时才写“已验证”。

### 3. 当前 ownership 与调用拓扑

#### 3.1 Application / AgentRun

- `crates/agentdash-application-agentrun/src/agent_run/runtime_facade.rs:316-336`
  定义 AgentRun 面向 Runtime 的 facade；Application 使用产品 `run_id/agent_id`，不直接操作 concrete driver。
- `crates/agentdash-application-agentrun/src/agent_run/runtime_facade.rs:1948-1966`
  Application presentation append 先把 AgentRun target 解析成 Runtime thread，再调用 Runtime gateway。它是向 canonical journal 追加产品 presentation 的入口，不是 Session/Turn/Context 的独立 store。
- `crates/agentdash-application-ports/src/agent_run_runtime.rs:15-35`
  定义 AgentRun runtime target、provision request 与 fork source。
- `crates/agentdash-application-ports/src/agent_run_runtime.rs:50-65`
  `AgentRunRuntimeBinding` 持有 AgentRun 到 Runtime thread/binding/surface 的 durable coordinates。

#### 3.2 统一 Managed Agent Runtime

- `crates/agentdash-agent-runtime/src/model.rs:16-54`
  canonical `Turn`、`Item`、`Interaction`、`Operation` 基础实体。
- `crates/agentdash-agent-runtime/src/model.rs:68-102`
  `RuntimeThreadState` 当前统一持有 lifecycle/status、active turn、binding/generation/source、profile/surface、checkpoint/context/settings/tool/hook revision、operation/turn/item/interaction maps 与 presentation transcript。
- `crates/agentdash-agent-runtime/src/ports.rs:255-281`
  `RuntimeCommit` 把 projection、operations、records、outbox、terminal application effects、compaction、hooks/quarantine 组合成一个 Runtime repository 原子 write set。
- `crates/agentdash-agent-runtime/src/context.rs:17-54`
  checkpoint 与 context head。
- `crates/agentdash-agent-runtime/src/context.rs:58-107`
  context candidate 与 activation 状态。
- `crates/agentdash-agent-runtime/src/context.rs:115-154`
  preparation work 与 compaction presentation。

因此，当前 Managed Runtime 不只是 transport facade：它是所有 contribution 共用的 canonical transition kernel。重新设计时应保留“统一外层 Runtime”角色，但需要明确其中哪些是平台 canonical contract，哪些只是对完整外部 Agent 内部事实的 normalized read projection。

#### 3.3 Integration / Driver Host

- `crates/agentdash-integration-api/src/agent_runtime.rs:131-160`
  `AgentServiceDefinition` 描述受信 service definition、factory、protocol revision、config schema 与 profile upper bound。
- `crates/agentdash-integration-api/src/agent_runtime.rs:309-317`
  `AgentRuntimeSurfaceBroker` materialize driver surface。
- `crates/agentdash-integration-api/src/agent_runtime.rs:533-547`
  当前可替换 seam 是 `AgentRuntimeDriverFactory` 与 `AgentRuntimeDriverContribution`；contribution 包含 definition、factory、conversation projection profile。
- `crates/agentdash-integration-api/src/agent_runtime.rs:551-571`
  `DriverConversationProjectionProfile` 已按 item family、typed interaction、transient identity、usage/error fidelity 描述 projection strength。
- `crates/agentdash-integration-codex/src/contribution.rs:43-54`
  Codex 以 full-fidelity conversation projection contribution 接入。
- `crates/agentdash-integration-native-agent/src/driver.rs:226-262`
  Native 也以 contribution 接入，但明确不声称 AgentCore 不产生的 Plan family。
- `crates/agentdash-agent-runtime-host/src/model.rs:70-82`
  `RuntimeOffer` 是 service instance 的实际可兑现能力。
- `crates/agentdash-agent-runtime-host/src/model.rs:98-123`
  bound/applied surface 固定 revision、digest、tool set 与 hook plan revision。
- `crates/agentdash-agent-runtime-host/src/model.rs:141-158`
  `RuntimeBinding` 固定 thread、offer、service instance、driver generation、profile/surface 与 state。
- `crates/agentdash-agent-runtime-host/src/model.rs:194-201`
  `RuntimeSourceCoordinate` 保存 canonical binding/generation 到 source thread 的映射。
- `crates/agentdash-agent-runtime-host/src/model.rs:243-252`
  `DriverLease` 是 delivery ownership/fencing，不是 Agent Session activity。
- `crates/agentdash-agent-runtime-host/src/host.rs:472-659`
  bind 流程验证 offer/profile/surface，先 reserve pending binding，再调用 driver bind，最后 activate；Resume 还校验 source identity。

#### 3.4 当前 composition 的真实含义

- `crates/agentdash-infrastructure/src/agent_runtime_composition.rs:1924-1965`
  composition 同时产出一个 `ManagedAgentRuntime`、gateway、Host、repository 与 workers。
- `crates/agentdash-infrastructure/src/agent_runtime_composition.rs:1970-2056`
  所有 driver contributions 共用同一个 Managed Runtime 与 PostgreSQL Runtime repository。
- `crates/agentdash-infrastructure/src/agent_runtime_composition.rs:2063-2207`
  Native、Codex/remote contributions 都进入同一 composition；managed compaction engine 当前统一使用 Native managed compaction engine。
- `crates/agentdash-infrastructure/src/agent_runtime_composition.rs:380-392`
  executor `"PI_AGENT"` 与 `"CODEX"` 选择不同 definition，但仍落在共同的外层 Runtime/Driver Host 路径。

结论：当前已经有必须保留的统一 Runtime 外层和强 Host seam；缺少的是把“完整 Agent 实现”作为一等能力/命令/状态提供者的合同。现有 `Driver` 词汇和 API 让 complete Codex/enterprise Agent 容易被误建模成无状态 executor。

### 4. 当前 AgentRun fork 完整链

#### 4.1 API cutoff 解析

- `crates/agentdash-api/src/routes/lifecycle_agents.rs:1028-1084`
  API 在 source AgentRun 的 visible journal 中查找请求的 source turn/entry，解析为 parent visible journal sequence。
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:1085-1113`
  创建 child LifecycleRun/LifecycleAgent，并 clone parent frame。
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:1122-1127`
  同一个请求只把 source turn 映射成 `DriverTurnId` 传给 Runtime fork；entry index 没有进入 driver cutoff。
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:1134-1149`
  `AgentRunLineage` 保存 visible journal cutoff 与 parent/child AgentFrame baseline refs。
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:1206-1217`
  产品 command receipt/result 在 child graph/runtime fork 之前持久化。
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:1215-1243`
  随后才调用 `AgentRunForkCommandService` 创建 graph 并 provision runtime。

这产生一个已证实的粒度风险：平台 visible cutoff 可精确到 entry，但 driver fork 只拿到 turn。Codex 等 provider 的 native fork cutoff 与平台 visible history cutoff 可能不完全同义。

#### 4.2 产品 child graph

- `crates/agentdash-application-ports/src/agent_run_fork.rs:5-15`
  child graph 包含 LifecycleRun、LifecycleAgent、AgentFrame、AgentRunLineage。
- `crates/agentdash-application-agentrun/src/agent_run/fork_command.rs:31-50`
  command service 先创建 child graph，再调用 runtime fork；runtime 失败时删除 child graph 作为补偿。
- `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_fork_graph_store.rs:18-112`
  PostgreSQL graph store 在一个事务中插入 run、agent、frame 与 lineage。
- `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_fork_graph_store.rs:115-121`
  compensation 以删除 child LifecycleRun 回收图。
- `crates/agentdash-domain/src/workflow/agent_run_lineage.rs:6-34`
  `AgentRunLineage` 是跨 AgentRun provenance；它不同于同一 LifecycleRun 内的 `AgentLineage`。
- `crates/agentdash-domain/src/workflow/agent_run_lineage.rs:39-65`
  fork lineage 包含 parent/child product refs、fork point 与 frame baselines。

`create graph -> runtime fork -> compensation delete` 能覆盖同步返回错误，不等价于任意 crash point 下的 exactly-once saga。尤其 receipt 已经先提交，graph、host binding、surface publication 和 AgentRun binding 又是后续独立提交。

#### 4.3 Runtime/Host provision

- `crates/agentdash-application-agentrun/src/agent_run/runtime_facade.rs:117-124`
  `ForkAgentRunRuntime` 携带 source/child target 与可选 source turn。
- `crates/agentdash-application-agentrun/src/agent_run/runtime_facade.rs:803-824`
  fork 先确认 source AgentRun binding 存在，然后以 fork source 调用 provision。
- `crates/agentdash-infrastructure/src/agent_runtime_composition.rs:984-1001`
  provision 对既有 target 幂等；生成 deterministic thread/binding ID，并 prepare/store surface。
- `crates/agentdash-infrastructure/src/agent_runtime_composition.rs:1002-1026`
  fork 读取 source binding、host binding 与 offer，并复用 source offer。
- `crates/agentdash-infrastructure/src/agent_runtime_composition.rs:1054-1068`
  Host bind 使用 `DriverBindIntent::Fork { source, last_turn_id }`。
- `crates/agentdash-infrastructure/src/agent_runtime_composition.rs:1077-1111`
  构造 AgentRun runtime binding，reserve/commit surface publication，然后插入 binding。
- `crates/agentdash-agent-runtime-contract/src/driver.rs:31-52`
  driver bind intent 明确区分 Start、Resume、Fork。
- `crates/agentdash-agent-runtime-host/src/host.rs:472-659`
  Host 对 pending/active binding 幂等恢复，并在 driver bind 后持久化 source coordinate。

#### 4.4 Codex 与 Native 的实际差异

- `crates/agentdash-integration-codex/src/driver.rs:362-365`
  若 bound surface 含 dynamic tools，Codex Resume/Fork 被拒绝，因为当前 Codex 不能在该路径重新应用 dynamic tools。
- `crates/agentdash-integration-codex/src/driver.rs:370-433`
  Codex Fork 调用 `thread/fork`，传 source thread ID 与 `lastTurnId`，并接收新的 source thread ID。
- `crates/agentdash-integration-codex/src/driver.rs:515-528`
  Codex 把 `RuntimeCommand::ThreadResume/ThreadFork` 视为错误 dispatch；这些动作必须走 bind intent。
- `crates/agentdash-integration-native-agent/src/driver.rs:964-980`
  Native `DriverBindIntent::Fork` 只创建新 Native source ID，并保存 `thread: None` 的 binding；没有导入 source history。
- `crates/agentdash-integration-native-agent/src/driver.rs:1112-1165`
  Native 另一个 `RuntimeCommand::ThreadFork` 路径可以导入 Host context checkpoint，并校验 digest。
- `crates/agentdash-agent-runtime-contract/src/command.rs:103-116`
  Runtime command union 包含 `ThreadFork`。
- `crates/agentdash-agent-runtime-contract/src/command.rs:149-152`
  该 command 只有 child thread ID 与可选 checkpoint ID，没有 parent/source cutoff。

静态搜索没有找到产品 fork 构造 `RuntimeCommand::ThreadFork`；产品链使用的是 `DriverBindIntent::Fork`。因此：

- Codex 产品 fork 会调用 provider-native fork；
- Native 产品 fork 当前得到新 binding，但不会继承父会话 history；
- Native checkpoint-import fork 能力存在且有单测，但尚未接到产品 fork。

这是一条“当前接线差异”，不是仅由测试缺失推断出的回归。

#### 4.5 child Managed Runtime 首次启动

- `crates/agentdash-application-agentrun/src/agent_run/runtime_facade.rs:1525-1582`
  第一次向无 binding target 发送消息时会 provision。
- `crates/agentdash-application-agentrun/src/agent_run/runtime_facade.rs:1697-1754`
  若 child Managed Runtime snapshot 尚不存在，首条消息发 `RuntimeCommand::ThreadStart`；已有 snapshot 才发 TurnStart。

所以产品 fork 会先建立 child Host/source binding，而 child Managed Runtime projection 到首条消息时才由 ThreadStart 建立。对于完整外部 Agent，source session 可能已经由 provider fork 创建；平台 canonical projection 与 source internal session 的创建时点当前并不相同。

### 5. visible history、journal 与 fork 继承依赖

#### 5.1 Runtime journal 边界

- `crates/agentdash-agent-runtime-contract/src/event.rs:685-700`
  `RuntimeCarrierMetadata` 保存 thread、sequence/transient coordinate、revision、recorded time 等 envelope metadata。
- `crates/agentdash-agent-runtime-contract/src/event.rs:702-723`
  `RuntimeJournalFact = Presentation | Internal`。
- `crates/agentdash-agent-runtime-contract/src/event.rs:725-749`
  authoritative journal record 是 carrier + fact。
- `crates/agentdash-agent-runtime-contract/src/event.rs:753-784`
  durable/transient 与 internal/presentation 的组合由 validation 约束。

`RuntimeJournalFact` 当前既承载 platform presentation，也承载 Runtime internal facts；它不是 driver event DTO，但当前 context/fork/read 仍直接依赖这条 universal journal。

#### 5.2 AgentRun visible journal

- `crates/agentdash-application-agentrun/src/agent_run/journal.rs:14-27`
  visible event 分 inherited/current segment。
- `crates/agentdash-application-agentrun/src/agent_run/journal.rs:148-164`
  journal service 依赖 lineage、binding resolver 与 Runtime journal source。
- `crates/agentdash-application-agentrun/src/agent_run/journal.rs:279-294`
  visible journal = inherited presentation prefix + current presentation records。
- `crates/agentdash-application-agentrun/src/agent_run/journal.rs:296-361`
  inherited prefix 会逐级遍历 AgentRunLineage，解析每个 ancestor 当前 binding，读取其 durable Runtime records，应用每级 fork cutoff，再拼接。
- `crates/agentdash-application-agentrun/src/agent_run/journal.rs:337-346`
  ancestor runtime binding 缺失会直接失败。
- `crates/agentdash-application-agentrun/src/agent_run/journal.rs:386-390`
  UI/feed 只保留 presentation records，internal records 被过滤。
- `crates/agentdash-application-agentrun/src/agent_run/journal.rs:393-408`
  current segment visible sequence 仍保留 raw Runtime EventSequence 的 internal gap。

当前 fork history 因而依赖：

1. `agent_run_lineages` parent chain；
2. 每个 ancestor 的当前 AgentRun runtime binding；
3. 每个 binding 的 Runtime journal 仍在 retention 范围内；
4. 每级 visible cutoff 的解释仍稳定。

它不是 child session 内独立物化的 inherited history。删除/回收 ancestor 或改变其 current binding 会影响 child 历史恢复能力。

#### 5.3 lineage migration

- `crates/agentdash-infrastructure/migrations/0038_agent_run_lineages.sql:1-30`
  创建 product parent/child lineage、relation kind、child uniqueness 与外键。
- `crates/agentdash-infrastructure/migrations/0046_agent_run_lineage_product_refs.sql:1-6`
  删除 parent/child runtime session ID，明确 lineage 只存 product refs。
- `crates/agentdash-infrastructure/migrations/0048_agent_run_lineage_baseline_refs.sql:1-5`
  增加 parent/child frame baseline refs。

当前 lineage 不保存 L2/implementation session fork receipt。若未来完整 Agent 自己拥有 session/fork，平台需要另存 product lineage 到 opaque implementation parent/child session refs 的稳定映射。

### 6. Companion 当前继承链

- `crates/agentdash-application/src/companion/dispatch.rs:18-44`
  dispatch request 包含 parent run/agent/frame/session、slice/adoption、selected agent。
- `crates/agentdash-application/src/companion/dispatch.rs:55-143`
  dispatch 在同一 LifecycleRun 图中开 gate 或创建 child Agent；Full 使用 ContextPolicy::Inherit，其余使用 Slice，并继承 CapabilityPolicy。
- `crates/agentdash-application/src/companion/dispatch.rs:170-181`
  service 依赖 LifecycleRun/LifecycleAgent/AgentFrame/subject/gate 与同 run 的 `AgentLineage` repositories。
- `crates/agentdash-application/src/companion/dispatch.rs:209-240`
  Runtime provision request 明确是 `fork: None`。
- `crates/agentdash-application/src/companion/dispatch.rs:276-285`
  launch source 记录 parent session/run/agent 等产品 provenance。
- `crates/agentdash-application/src/companion/tools.rs:1317-1327`
  parent delivery context 带 parent runtime session ID 与 lifecycle anchor。
- `crates/agentdash-application/src/companion/tools.rs:1348-1380`
  调用 before-subagent hook。
- `crates/agentdash-application/src/companion/tools.rs:1393-1423`
  dispatch plan 来自 Hook runtime snapshot/resolution。
- `crates/agentdash-application/src/companion/tools.rs:1435-1452`
  创建 child。
- `crates/agentdash-application/src/companion/tools.rs:1507-1537`
  把生成的 inherited-context prompt 作为 child 首条 RuntimeInput。
- `crates/agentdash-application/src/companion/tools.rs:3033-3227`
  prompt、hook snapshot 与 slice filter 负责传入 injections/constraints。
- `crates/agentdash-application-agentrun/src/agent_run/frame/lifecycle_materialization.rs:18-95`
  当前 lifecycle/companion 使用 launch-anchor frame construction，建立新的 launch frame 与 hook plan。
- `crates/agentdash-api/src/bootstrap/repositories.rs:202-208`
  composition 确认 lifecycle/companion 使用 launch-anchor adapter；另一个完整 FrameConstructionService 供 project-agent 路径使用。
- `crates/agentdash-application/src/frame_construction/request_assembler.rs:85-91`
  另有 `CompanionParentFactsProvider` trait，只读取 parent capability state。
- `crates/agentdash-application/src/frame_construction/request_assembler.rs:221-240`
  该 resolver 返回 parent VFS/MCP，明确 `parent_context_bundle: None`。

静态搜索未发现 `CompanionParentFactsProvider` 的 production implementation。这里能确认的是该 parent-facts path 未接线；不能据此断言整个 FrameConstructionService 未使用。

因此 Companion 当前“继承”主要是 L3 product relationship + capability policy + hook-derived prompt，不是完整 Agent session fork。若 Companion 语义要求继承真实对话，这是当前缺口；若某些 Companion 模式本来就是 fresh session + explicit context，则需由产品语义明确区分。

### 7. 数据库存储的实际边界

#### 7.1 当前 universal Runtime state

`crates/agentdash-infrastructure/migrations/0061_agent_runtime_managed_state.sql` 建立：

- `:4-24` Runtime thread；
- `:26-51` binding/source state；
- `:53-70` operation；
- `:72-110` event、turn、item、interaction；
- `:112-145` outbox 与 quarantine；
- `:147-215` checkpoint、context preparation/candidate；
- `:217-298` activation、dispatch 与 context head。

这些表当前由所有 implementation 共用，因而同时承担 platform canonical state 与对外部 Agent 会话的“平台重建副本”。

#### 7.2 Hook 与 Tool

- `crates/agentdash-infrastructure/migrations/0062_agent_runtime_hook_orchestration.sql:1-12`
  durable hook plan。
- `crates/agentdash-infrastructure/migrations/0062_agent_runtime_hook_orchestration.sql:22-94`
  hook run 与 hook effect。
- `crates/agentdash-infrastructure/migrations/0063_agent_runtime_tool_broker.sql:1-35`
  durable broker call，直接关联 managed Runtime item/interaction/binding。

这两组能力必须保留，但要按因果 owner 切分：

- 平台 Surface/Tool Broker/Host Hook 与其 effect 是统一外层 Runtime/平台 coordination；
- 完整 Agent 私有的 provider/tool/subagent/internal hook 是 implementation internal；
- `BoundAgentSurface` 应固定某项 hook/tool 的唯一 route 与 semantic strength，避免平台和完整 Agent 双写/双执行。

#### 7.3 Host 与 recovery

- `crates/agentdash-infrastructure/migrations/0064_agent_runtime_driver_host.sql:5-49`
  service instance、revision、activation。
- `crates/agentdash-infrastructure/migrations/0064_agent_runtime_driver_host.sql:51-120`
  offer、host binding、lease 与 driver/source coordinate。
- `crates/agentdash-infrastructure/migrations/0068_agent_runtime_binding_recovery.sql:17-39`
  AgentRun thread anchor 与 binding lineage。
- `crates/agentdash-infrastructure/migrations/0068_agent_runtime_binding_recovery.sql:60-83`
  durable recovery intent。

这些是统一外层 Runtime/Host 必须保留的 execution coordination，而不是完整 Agent 内部 history/context 的 owner。

#### 7.4 journal cutover

- `crates/agentdash-infrastructure/migrations/0070_runtime_journal_records.sql:1-15`
  因旧 journal record 不兼容，migration 删除当前 Runtime owner graph。
- `crates/agentdash-infrastructure/migrations/0070_runtime_journal_records.sql:16-28`
  event columns 改为 `fact_kind/record` 并限制 presentation/internal。

项目尚未上线，后续允许 forward migration 做正确的 hard cut；但新 schema 不应再次把“平台 canonical / implementation internal / coordination / product state”压进单个无差别 aggregate。

### 8. 必须保留的 07-10 / 当前实现能力

以下不是“旧实现包袱”，而是当前已经落地、重构时必须继续满足的行为合同：

1. **统一 AgentRun facade 与 canonical Runtime contract**
   - Application 不依赖 concrete implementation/driver；
   - accepted operation、idempotency、expected revision、snapshot/change/feed 均有统一语义。
2. **RuntimeOffer / capability admission**
   - service definition/profile upper bound；
   - instance offer；
   - desired `AgentSurfaceSnapshot`；
   - `BoundAgentSurface` 的 per-contribution route/semantic strength；
   - applied surface revision/digest/ack。
3. **Service binding / placement / recovery**
   - trusted contribution registry；
   - service instance/offer/binding/source coordinate；
   - generation fencing、lease、pending bind recovery、rebind。
4. **Business Surface**
   - 平台从 AgentFrame/Capability Pack/product facts 编译 immutable、revisioned、digest-addressed surface；
   - implementation 只 materialize 已求交的 surface，不重新解释产品 policy。
5. **Tool Broker**
   - durable call identity、policy/VFS/credential/approval；
   - irreversible Running side-effect boundary不自动 replay；
   - canonical tool presentation 与 executor owner 分离。
6. **Hook orchestration**
   - immutable plan revision；
   - hook run/effect durable lifecycle；
   - route/timing/semantic strength admission；
   - crash replay 与 terminal transaction。
7. **Runtime journal / presentation delivery**
   - durable sequence、retention gap、transient coordinate；
   - presentation/internal distinction；
   - reconnect 与 same ordered projection。
8. **Relay / Runtime Wire**
   - `crates/agentdash-agent-runtime-wire/src/lib.rs:28-61`：typed frame ID/envelope/request/response/notification/ack；
   - `crates/agentdash-agent-runtime-wire/src/lib.rs:79-110`：HostPort transcript/surface/tool/context/compaction/hook requests；
   - `crates/agentdash-agent-runtime-wire/src/lib.rs:159-166`：resume cursor 与 cumulative ack；
   - `crates/agentdash-relay/src/runtime_wire.rs:90-220`：relay stream payload、admission 与 frame ordering；
   - `crates/agentdash-integration-remote-runtime/src/lib.rs:307-718`：remote ordered frame、ack、request correlation 与 reconnect transport。

未来即便 wire payload 从“Managed Runtime + Driver mixed union”调整为完整 Agent command/receipt/observation/snapshot/change，也应保留 sequence、ack、replay、disconnect/generation fencing 等 transport correctness。

### 9. 对 07-17 `AgentSession` 方案的 ownership 评估

#### 9.1 方向正确的部分

- `design.md:47-55`
  不再用 presentation journal 拼接来承担 authoritative read/fork，Application 只经 execute/read/changes；方向正确。
- `design.md:207-212`
  execute 表示 durable acceptance、read 返回 revision、changes 只消费 committed change、gap 后重读 snapshot；适合作为统一外层 Runtime 的 canonical contract。
- `design.md:325-337`
  Operation 与 queue/mailbox 分离、queued request 不创建伪 Turn/Item；状态机建模清晰。
- `design.md:339-389`
  Compaction 使用 typed Turn、独立 entity、稳定 identity、同事务 terminalization，并区分 Synchronizing/Lost；可作为自有 Agent implementation 与外层 operation/effect coordination 的基础。
- `design.md:357-369`
  Binding 与 Session consistency 在概念上已承认正交。
- `design.md:585-626`
  stable effect identity、inspect、generation/revision fence 与 Unknown/Lost；应保留在统一 Runtime/Host coordination。
- `design.md:632-665`
  authoritative snapshot revision + ordered change tail 与 typed fork cutoff；适合替代“journal projection 反向成为事实源”。

#### 9.2 与最终 ownership 基线冲突的部分

1. `design.md:7-20` 宣称只有平台 `AgentSession` aggregate 拥有所有会话业务事实，Native/Codex/Remote 都只是 receipt/observation adapter，且外部 stateful session 只是 replica。这会抹平“自有 Agent、Codex、pi-coding-agent/企业 Agent 都是完整 Agent 实现”的差异。
2. `design.md:79-93` 把 Session、Mailbox、Turn/Item、Context、Compaction、binding consistency、effect settlement 和 change publication放在一个 Hosted Agent authority matrix 中。虽然表中列名不同，后续 transition kernel 仍跨这些状态直接写不变量，形成单 aggregate 实质。
3. `design.md:137-143` 与 `design.md:272-391` 让一个 `AgentSession Transition Kernel` 同时管理业务 admission、内部会话实体、context/compaction 与 execution coordination。对自有 Agent 可以作为 implementation 内部设计；对 Codex/企业完整 Agent，平台不能成为其内部 Context/Turn/Item 的写 owner。
4. `design.md:270` 规定 adapter 不生产 Agent entity/presentation/journal；若“adapter”实际代表完整 Agent，这个约束过强。完整 Agent 应能以 typed snapshot/change/observation 提供自己的只读内部状态；平台可以归一化投影，但不能假装这些实体由平台 transition 创建。
5. `design.md:329-337` 把 mailbox 直接纳入 Hosted Agent session。07-10 原始边界把产品 mailbox/AgentRun command availability 放在 Application；即使平台需要 Runtime pending command，也应与产品 mailbox、完整 Agent 自己的内部 pending queue 分开命名和存储。
6. `design.md:656-665` 规定 Agent repository 直接复制/引用 Session entities 完成 fork。该方案只适合 AgentDash-owned implementation。Codex/企业 Agent 的 fork 应由完整 Agent command 执行，返回 opaque child session ref/receipt；平台只提交 product/canonical mapping 与 normalized change。
7. `design.md:719-721` 虽把 binding/effect/change 拆成表，但 `design.md:762-785` 的迁移步骤仍让同一 AgentSession repository 成为所有 implementation 的 session truth，并要求 AgentRun fork/read 不再经过 implementation-owned state。这会把 storage 拆表但不拆写权限。

#### 9.3 推荐的 first-principles ownership

保留统一外层 Runtime，但将 ownership 明确成四个正交层次：

| 层次 | 写 owner | 典型状态 | 平台对完整外部 Agent 的权限 |
|---|---|---|---|
| Application / AgentRun | 产品 Application | AgentRun、LifecycleRun/Agent、Frame、Companion relation、产品 mailbox/gate/channel/task/workspace、product receipt | 不读取/写入 Agent 内部表 |
| Unified Agent Runtime | 平台 Runtime | canonical command/operation、availability、normalized snapshot/change cursor、AgentRun-to-AgentSession mapping、Surface admission result、platform presentation projection | 通过有限 Agent command 写；通过 read/subscribe 读 |
| Host / execution coordination | Runtime Host | service instance、RuntimeOffer、Bound/AppliedSurface、binding、placement、generation、lease、effect delivery、inspect/recovery | 可写 coordination，不写外部 Agent history/context |
| Complete Agent implementation | 每个 Agent 实现 | implementation session、history、turn/item/interaction、internal context/compaction/fork/lifecycle、private tools/hooks | owner；平台只经合同交互 |

自有 Agent 再向下拆：

```text
AgentDash Complete Agent
  -> Agent layer：session/history/fork/context/compaction/完整 lifecycle
  -> pi-like AgentCore：stateless loop/provider-tool callback core
```

Codex、pi-coding-agent/企业 Agent 则直接实现 complete Agent contract，不被压缩成 stateless executor。

统一外层 Runtime 的 canonical snapshot/change 仍然存在，但必须逐字段标明 authority：

- **平台可直接写**：operation admission/result、surface admission、platform effect/recovery、binding refs、product presentation、normalized availability；
- **外部 Agent canonical、平台只读**：provider session history、内部 Turn/Item、内部 Context/Compaction、internal tool/subagent state；
- **平台可通过有限命令改变**：fork、submit/steer/interrupt、request compaction、resolve interaction、close/resume；
- **derived/cache**：UI normalized entity projection、journal、analytics/search，不反向参与 command admission；
- **自有 Agent implementation 内部可写**：自有 session/history/context/compaction；但仍通过与外部 Agent 相同的 complete Agent seam 暴露。

`RuntimeOffer` / `BoundAgentSurface` 除现有 profile 外，还需要表达每种状态与操作的 semantic strength，例如：

- fork：exact native fork / checkpoint-based fork / unsupported；
- context：opaque/read-only snapshot / typed revision / platform-supplied immutable context；
- compaction：Agent-owned command / platform-managed exact / unsupported；
- change：authoritative ordered tail / snapshot-only / observation-only；
- tools/hooks：platform broker、driver callback、Agent-native，且每项唯一 route。

这样统一 Runtime 能继续做能力 admission、产品合同和 recovery，却不会把低保真 observation 误写成外部 Agent 的 canonical internal entity。

### 10. 风险 / 缺口

#### 当前实现

1. **fork 跨多个 durability boundary**
   产品 receipt、child graph、surface/host bind、AgentRun binding 没有统一 saga；同步 compensation 不能覆盖进程 crash。
2. **cutoff identity 不一致**
   产品 visible cutoff 可精确到 entry，driver 只收到 turn；需要一个跨 implementation 可验证的 fork point contract。
3. **Native 产品 fork 不继承 history**
   checkpoint-import 命令存在但未接到 AgentRun fork。
4. **child visible history依赖 ancestor 活性**
   lineage 拼接需要 ancestor current binding/journal；缺少独立 retention/delete 语义。
5. **Companion “inherit” 不等于 session fork**
   当前以 prompt/capability inheritance 为主；没有完整历史/fork receipt。
6. **Integration seam 仍以 Driver 为中心**
   已有 projection profile，但没有 complete Agent command/read/change/fork capability contract。
7. **统一 Runtime 内部 state 与外部 Agent state 没有 authority 标签**
   当前 thread/turn/item/context/compaction tables 对所有 contribution 一视同仁。

#### 07-17 方案

1. **单一 AgentSession aggregate 过宽**
   会把 platform canonical state、external internal read model、command-writable state、binding/effect/recovery 与产品 mailbox 再次合并。
2. **fork repository 算法不适用于完整外部 Agent**
   外部 session 不能由平台数据库复制出来。
3. **“driver 只返回 receipt/observation”能力模型过低**
   缺少 complete Agent 的 authoritative read/change/fork 接口。
4. **schema ownership 尚未按 implementation 切开**
   `agent_session*` 命名无法说明一行是平台 canonical、外部 read cache 还是自有 Agent internal truth。
5. **migration/test 计划未覆盖跨实现语义**
   现有方案主要验证一个平台 aggregate 的状态矩阵，没有覆盖 Codex/native/remote 在不同 semantic strength 下的合同一致性。

### 11. 建议

1. **保留并重述统一外层 Runtime**
   明确它位于 AgentRun facade 与 complete Agent/Integration seam 之间，继续拥有 canonical command/operation/snapshot-change、Surface/Tool/Hook admission、service binding/placement/recovery。
2. **把 Integration seam 从“Driver factory”提升为“Complete Agent service”**
   至少提供 create/resume/fork、submit/steer/interrupt/compact/resolve、read snapshot、subscribe changes、inspect effect/command 的有限协议；具体命名可后定。
3. **不要用一个 repository transaction 假装跨外部 Agent 原子**
   平台数据库只原子提交自己的 operation/mapping/effect/change；外部 Agent 命令通过 stable idempotency/effect identity + inspect/reconciliation 收敛。
4. **为自有 Agent 建 implementation-owned session repository**
   可复用 07-17 typed Turn、Compaction saga、revision/change 等设计，但其 authority 属于 AgentDash Complete Agent，而非对所有外部 Agent 通用的平台 aggregate。
5. **把 platform normalized snapshot 设计成显式 authority-aware projection**
   每个 field/section 标明 source revision、fidelity/semantic strength、authoritative/derived；不可由 projection 反向推断外部 Agent 成功状态。
6. **重写 fork 为 platform saga + implementation fork**
   - L3 创建稳定 product fork command；
   - 统一 Runtime 依据 offer/bound surface admission；
   - complete Agent 执行 fork 并返回 opaque child session ref、verified cutoff/receipt；
   - 平台原子提交 child AgentRun mapping、product lineage、canonical operation/change；
   - crash 后按相同 identity inspect/reconcile。
7. **把 visible history 从 ancestor current binding 解耦**
   优先读取 child Agent 的 authoritative forked snapshot/change；若 UI 需要 platform materialization，写独立 projection，并定义 ancestor retention/delete 后的行为。
8. **明确 Companion 模式**
   `fork parent session` 与 `fresh session + explicit context package` 是两种不同 command，不应都叫 Inherit，也不应仅靠 prompt 模拟 fork。
9. **按 causal route 保留 Hook/Tool 能力**
   平台 broker/host hook 继续由统一 Runtime 记 effect；Agent-native hook/tool 留在 complete Agent 内部；Bound surface 保证唯一执行 owner。
10. **使用 forward migration 做一次正确 hard cut**
    项目未上线，无需 compatibility/backoff；migration 要把 product、platform canonical、host coordination、自有 Agent internal、external read projection 分区，并保留可验证的 AgentRun/session lineage。

### 12. 建议的验证矩阵

已存在且应保留：

- AgentRun fork graph compensation；
- single/multi-level visible history cutoff；
- reconnect 与 visible cursor/internal gap；
- Native checkpoint fork digest；
- Host bind/lease/recovery；
- Runtime idempotency/expected revision/operation sequence；
- context/compaction、Hook、Tool Broker、Relay/Wire replay tests。

新增重点：

1. Native complete Agent、Codex complete Agent、remote/enterprise complete Agent 的同一 contract conformance；
2. 各实现的 exact fork、unsupported fork、checkpoint fork capability admission；
3. entry/turn/item cutoff 与 provider-native cutoff 的等价性；
4. 产品 receipt 后、graph 前；graph 后、Agent fork 前；Agent fork 已应用、平台未知；mapping commit 前等 crash points；
5. child 在 ancestor 删除、binding replacement、journal retention cutoff 后仍可恢复；
6. Companion fork mode 确实继承 canonical history，fresh mode 只使用显式 context package；
7. external Agent snapshot/change gap、duplicate/stale observation、generation change；
8. platform normalized projection 不会被当作 external internal truth 写回；
9. Surface/Hook/Tool 的 per-route semantic strength 与 required capability gate；
10. owned Agent 的 AgentCore 保持 stateless，session/history/fork/compaction 全在 Agent layer。

### 13. 本次实际回归证据

执行：

```text
cargo test -p agentdash-application-agentrun fork_
```

结果：5 passed，0 failed，包括：

- `fork_get_and_reconnect_share_one_ordered_projection`
- `fork_cutoff_uses_parent_visible_cursor_across_internal_sequence_gaps`
- `multi_level_fork_applies_each_parent_local_cutoff_before_concatenation`
- `runtime_failure_compensates_the_complete_child_graph`
- `fork_result_survives_acceptance_failure_and_replays_without_a_second_materialization`

执行：

```text
cargo test -p agentdash-integration-native-agent native_fork_imports_the_requested_checkpoint_and_preserves_its_digest
```

结果：1 passed，0 failed。

这些结果证明上述局部行为当前通过；它们不证明产品 Native fork 已接入 checkpoint import，也不覆盖跨事务 crash recovery 或 Companion fork。

### 14. 仍需用户决策

1. Companion 的每种 adoption/slice 模式中，哪些必须调用 complete Agent fork，哪些允许 fresh session + explicit context？
2. 统一 Runtime 对外部 Agent history/Turn/Item/Context 的 normalized snapshot 需要持久缓存到什么粒度？是否允许 snapshot-only implementation？
3. fork cutoff 的统一 identity 采用 Session revision、Turn、Item/entry，还是 capability-dependent typed union？Codex `lastTurnId` 如何证明与 entry cutoff 等价？
4. product mailbox、Runtime pending command 与 complete Agent internal queue 的命名、持久化和 promotion owner 如何明确分开？
5. 哪些 compaction 模式是 Agent-owned command，哪些是 platform-managed exact capability？不支持 exact apply 的实现是否直接拒绝？
6. 外部 complete Agent 的 authoritative change 是强有序 tail、snapshot+cursor、还是 observation-only；各级 semantic strength 对 UI/fork/recovery 有何功能门槛？
7. ancestor 删除/retention 后，已 fork child 的 history 必须完全独立，还是允许声明外部 provider 级别的 retention 限制？
8. Host binding/effect/recovery 是继续作为独立 aggregate，还是只与 Runtime operation 通过稳定 ID 关联？建议独立，最终 schema 命名需确认。

### 15. Files found

#### 当前实现

- `crates/agentdash-api/src/routes/lifecycle_agents.rs` — AgentRun fork API、visible cutoff 解析、receipt 与 graph/runtime orchestration。
- `crates/agentdash-application-agentrun/src/agent_run/fork_command.rs` — child graph + runtime fork + compensation。
- `crates/agentdash-application-agentrun/src/agent_run/journal.rs` — ancestor lineage visible presentation projection。
- `crates/agentdash-application-agentrun/src/agent_run/runtime_facade.rs` — AgentRun 到统一 Runtime 的 command/read/provision facade。
- `crates/agentdash-application-ports/src/agent_run_fork.rs` — fork graph port。
- `crates/agentdash-application-ports/src/agent_run_runtime.rs` — AgentRun runtime target/binding/provision contract。
- `crates/agentdash-domain/src/workflow/agent_run_lineage.rs` — product cross-run lineage。
- `crates/agentdash-agent-runtime-contract/src/command.rs` — canonical Runtime command，包括 ThreadFork。
- `crates/agentdash-agent-runtime-contract/src/driver.rs` — DriverBindIntent Start/Resume/Fork。
- `crates/agentdash-agent-runtime-contract/src/event.rs` — RuntimeJournalFact 与 carrier。
- `crates/agentdash-agent-runtime/src/model.rs` — 当前 universal Runtime aggregate/projection。
- `crates/agentdash-agent-runtime/src/context.rs` — context/checkpoint/compaction state。
- `crates/agentdash-agent-runtime/src/ports.rs` — atomic Runtime commit 与 recovery work ports。
- `crates/agentdash-agent-runtime-host/src/model.rs` — offer/surface/binding/source/lease。
- `crates/agentdash-agent-runtime-host/src/host.rs` — bind、generation、lease/recovery orchestration。
- `crates/agentdash-integration-api/src/agent_runtime.rs` — 当前 trusted Driver contribution/factory/profile seam。
- `crates/agentdash-integration-codex/src/contribution.rs` — Codex contribution/profile。
- `crates/agentdash-integration-codex/src/driver.rs` — Codex bind/fork/dispatch。
- `crates/agentdash-integration-native-agent/src/driver.rs` — Native contribution、bind fork 与 checkpoint-import command。
- `crates/agentdash-integration-native-agent/tests/native_driver.rs` — Native checkpoint fork regression。
- `crates/agentdash-infrastructure/src/agent_runtime_composition.rs` — Managed Runtime、Host、Native/Codex/remote 的 composition。
- `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_fork_graph_store.rs` — child product graph transaction。
- `crates/agentdash-application/src/companion/dispatch.rs` — Companion product relationship 与 runtime provision。
- `crates/agentdash-application/src/companion/tools.rs` — Hook-derived Companion prompt/injection。
- `crates/agentdash-application/src/frame_construction/request_assembler.rs` — optional parent capability facts path。
- `crates/agentdash-application-agentrun/src/agent_run/frame/lifecycle_materialization.rs` — current lifecycle launch-anchor frame materialization。
- `crates/agentdash-agent-runtime-wire/src/lib.rs` — unified Runtime/Driver/HostPort wire envelope。
- `crates/agentdash-relay/src/runtime_wire.rs` — Relay runtime wire ordering/admission。
- `crates/agentdash-integration-remote-runtime/src/lib.rs` — remote driver transport/reconnect/correlation。
- `crates/agentdash-infrastructure/migrations/0038_agent_run_lineages.sql` — product fork lineage。
- `crates/agentdash-infrastructure/migrations/0046_agent_run_lineage_product_refs.sql` — lineage 去除 Runtime session refs。
- `crates/agentdash-infrastructure/migrations/0048_agent_run_lineage_baseline_refs.sql` — lineage frame baselines。
- `crates/agentdash-infrastructure/migrations/0061_agent_runtime_managed_state.sql` — universal Runtime persistence。
- `crates/agentdash-infrastructure/migrations/0062_agent_runtime_hook_orchestration.sql` — Hook persistence。
- `crates/agentdash-infrastructure/migrations/0063_agent_runtime_tool_broker.sql` — Tool Broker persistence。
- `crates/agentdash-infrastructure/migrations/0064_agent_runtime_driver_host.sql` — Host offer/binding/lease/source persistence。
- `crates/agentdash-infrastructure/migrations/0068_agent_runtime_binding_recovery.sql` — AgentRun anchor/binding lineage/recovery intent。
- `crates/agentdash-infrastructure/migrations/0070_runtime_journal_records.sql` — current journal record cutover。

#### 目标与待评估设计

- `.trellis/tasks/archive/2026-07/07-10-agent-runtime-architecture-convergence/design.md` — 原始统一 Runtime/Host/Integration 目标。
- `.trellis/tasks/archive/2026-07/07-10-agent-runtime-architecture-convergence/target-crate-shape.md` — 原始 crate/ownership/capability admission 目标。
- `.trellis/tasks/07-17-agent-runtime-compaction-state-protocol-review/design.md` — 本轮待评估 AgentSession/effect/change/compaction 方案。
- `.trellis/tasks/07-17-agent-runtime-compaction-state-protocol-review/prd.md` — 本轮问题与验收范围。

### 16. Related specs

- `.trellis/spec/backend/agent-runtime-kernel.md` — Managed Runtime canonical state/transition。
- `.trellis/spec/backend/agent-runtime-context.md` — context/checkpoint/compaction。
- `.trellis/spec/backend/agent-runtime-persistence.md` — transaction/CAS/recovery 持久化。
- `.trellis/spec/backend/agent-runtime-agentrun-facade.md` — Application/AgentRun seam。
- `.trellis/spec/backend/agent-runtime-hooks.md` — Hook plan/run/effect ownership。
- `.trellis/spec/backend/agent-runtime-surface-tool-broker.md` — Surface admission 与 Tool Broker。
- `.trellis/spec/backend/session/architecture.md` — Session/AgentRun 产品边界。
- `.trellis/spec/cross-layer/agent-runtime-wire-relay.md` — Runtime Wire/Relay 边界。
- `.trellis/spec/cross-layer/backbone-protocol.md` — presentation/protocol projection。

### 17. External references

无。本调研按要求只使用当前 workspace 的代码、migration、测试、Trellis specs 与任务设计，没有联网，也没有用外部实现文档替代当前分支事实。

## Caveats / Not Found

1. 没有执行需要 shared embedded PostgreSQL data root 的 infrastructure integration tests，避免与并行会话竞争；数据库结论来自 migration、repository code 与已有 unit tests。
2. 本次只执行了聚焦 fork 的两组测试。未执行全 workspace lint/type-check/test，不能据此宣称整个分支质量门通过。
3. 没有发现产品 AgentRun fork 构造 Native `RuntimeCommand::ThreadFork` 的 production call site；该结论来自当前 `crates/**/*.rs` 静态搜索。
4. 没有发现 `CompanionParentFactsProvider` 的 production implementation；只说明该窄 path 当前未接线，不说明整个 frame construction 未使用。
5. 当前没有覆盖“entry-level visible cutoff 与 Codex lastTurnId 完全等价”的证据，也没有覆盖 arbitrary crash point 的 fork saga 测试。
6. “完整 Agent service seam”的具体 trait/DTO 尚未存在；本文给的是 ownership 与所需能力类别，不把建议的命名当作已实现合同。
