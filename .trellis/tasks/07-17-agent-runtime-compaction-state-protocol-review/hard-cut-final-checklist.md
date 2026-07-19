# Agent Runtime S5 Hard Cut 最终清单

本清单以
[`final-convergence-closeout.md`](./final-convergence-closeout.md) 为执行依据。S5
只删除本任务已经
正确替代并通过真实 consumer/tracer 证明的旧实现。
Product 控制面为何曾退出构建图、当前 owner 与纵向门禁见
[`product-control-plane-boundary-audit.md`](./product-control-plane-boundary-audit.md)。
最终逐项 replacement 与删除顺序见
[`runtime-legacy-replacement-manifest.md`](./runtime-legacy-replacement-manifest.md)。

## 当前真实状态

- [x] C0：工作树干净，Product behavior oracle 与 capability inventory 已固定。
- [x] Product 控制面 oracle 固定为 `58c537b7`（`c3cc58b9^`）。
- [x] Complete Agent / Managed Runtime / Host / Dash/Core 的已验证基础保留。
- [x] canonical App Server protocol owner、source projector、Runtime carrier 与前端
  reducer/renderer 已恢复。
- [x] Lifecycle canonical history provider 已实现并注册到 VFS kernel。
- [x] VFS surface route/resolver 已接 Product binding 与 AppliedResourceSurface。
- [x] Application/Product 模块、API routes 与 AppState composition 已恢复到真实构建图；
  `agentdash-application --lib` 317/317 通过。
- [x] Complete Agent / Managed Runtime / Host / Native / Codex / Remote owner suites 通过；
  AgentRun Product/恢复 suite 234/234 通过。
- [x] Companion Full/Fresh、首输入、selected frame、gate/channel/task 与
  `AfterSubagentDispatch` 已进入 durable continuation，并覆盖重启与保存响应丢失。
- [x] Product RuntimeThread 语义已贯穿 Extension/Canvas/Workspace context、actor、DTO
  与生成 TypeScript；transport 自有 session identity 在适配边界保持原义。
- [ ] S4 Product Lane Ready：尚未通过。
- [ ] 正式 S5 deletion manifest：尚未形成。

## C1 — Product Integrity

### Application modules

- [x] 恢复并挂载 `companion`。
- [x] 恢复并挂载 `frame_construction`。
- [x] 恢复并挂载 `routine`。
- [x] 重新挂载仍在源码树中的 `canvas`、`capability`、`runtime_tools`、
  `gate_wait_policy`、`wait_activity`。
- [x] 恢复旧 Hook presets 所承载的 Product effects inventory。

### API routes

- [x] 恢复 Companion gate routes。
- [x] 恢复 Routine public/secured routes。
- [x] 恢复 Canvas routes。
- [x] 恢复 Workspace Module routes。
- [x] 恢复 Terminal routes。
- [x] 保持并验证 VFS surface routes。
- [x] 恢复 AgentRun workspace/runtime trace 读取 routes。

### AppState / production composition

- [x] 恢复 Companion model preflight。
- [ ] 将 collaboration tool contribution 接入最终 Runtime Tool Broker production catalog。
- [x] 恢复 Companion coordinator/worker、parent mailbox delivery、gate wake、
  adoption/result。
- [x] Routine executor 与 trigger composition 已恢复；trigger 使用稳定 Product target、
  durable prepared receipt、ProductLaunch、ProductInputDelivery 与 Runtime terminal
  observer，并由恢复扫描沿同一 identity 续跑。
- [x] 恢复 Wait service/provider 与 terminal convergence。
- [x] 恢复 Workspace Module、Canvas、Terminal control/presentation composition。
- [x] Lifecycle/Wait Product Tool contributions 已接入最终 typed Broker catalog；
  Wait 读取真实 shell terminal registry，Lifecycle 通过 install-once typed binding
  复用既有 orchestration reducer。
- [x] Companion request/respond Product Tool contributions 已接入最终 typed Broker
  catalog，并复用 durable continuation saga、gate、mailbox、preflight 与 pinned
  Product HookPlan。
- [x] AgentFrame 声明的 Workspace Module list/describe/operate/invoke/present 均通过
  typed Product command seam 进入 production Broker；write route 与 canonical
  RuntimeSurfaceUpdate convergence 具有联合 tracer。

### Product behavior tests

- [x] 从 oracle 恢复 Companion、Frame Construction、Routine tests。
- [ ] 从 oracle 恢复 AgentRun project start/delete/fork/message/workspace/mailbox tests。
- [ ] 恢复 API route 与 AppState composition tracer tests。

## C2 — Final Seam Wiring

- [ ] AgentRun create/input/control 只调用 Runtime Contract。
- [x] Companion Full 只调用 exact Runtime / Complete Agent Fork。
- [x] Companion fresh 只调用 Create + `InitialAgentContextPackage`，随后独立
  `SubmitInput`。
- [x] Companion/channel/gate/adoption/result 只写 Product repositories。
- [x] Dash collaboration tool 经 typed Tool Broker 调 Product Companion command。
- [x] Routine 经 ProductLaunch 与 ProductInputDelivery 调 Runtime。
- [x] Workflow AgentCall 以稳定 LifecycleAgent/AgentFrame 经 ProductLaunch、canonical
  binding/resource convergence 与 Product mailbox command 调 Runtime，并由 Runtime
  terminal observer 回写 Workflow。
- [x] Capability/Runtime Tools 编译为 Runtime Surface / Tool Broker contributions。
- [x] MCP discovery/executor 与已迁移的静态 Product/VFS tools 共享同一 typed Runtime Tool
  Broker catalog；Surface compiler只引用可执行 tool requirements，Host callback 经
  Broker执行，MCP server metadata 不作为 context 注入。
- [x] Complete Agent Hook 已进入 typed callback owner；Companion
  `AfterSubagentDispatch` 等 Product-only Hook effect 由 immutable HookPlan 驱动
  typed Product event owner，均已退出旧 aggregate execution shell。
- [x] Product Hook plan compiler/policy handler进入production composition；只将明确选择
  `AgentCoreCallback` / `DriverNative` 的site映射进Agent surface，空计划或无条件Allow
  不能作为required hook evidence。
- [x] Workspace/Canvas/VFS grants 只读 AppliedResourceSurface。
- [x] Product 运行期 surface update 由 request-scoped target/RuntimeThread 写入新的
  immutable AgentFrame revision，经 Host 新 generation 与旧 binding fence、stable
  Runtime Rebind、Product pre-activation binding CAS、exact 新 binding digest
  materialize、Activate 与 Host/Product pin 完成前向收敛；launch frame 保持不可变证据。
- [x] Lifecycle VFS mount 进入 AgentRun AppliedResourceSurface materialization。
- [x] Terminal control与展示只读写 Product terminal projection/control owner。
- [x] AgentRun workspace/runtime trace 读取 canonical Product/Runtime projection。
- [x] 所有 conversation presentation 只使用 canonical App Server records。
- [ ] Product 代码只依赖 Runtime Contract、Product repositories、AppliedResourceSurface
  与 canonical conversation protocol。

## C3/C4 — Product parity tracer

- [x] AgentRun Resume/Close 经 Product command facade 与 durable claim 调 Runtime；
  Product aggregate Delete 逐一 Close 并复读 canonical Closed 后删除 LifecycleRun，且与
  ProjectAgent 模板删除保持独立。
- [ ] ProjectAgent direct AgentRun create Product saga、POST route 与首输入纵向 tracer。
- [ ] 普通 input → Complete Agent → canonical Turn/Item/output → UI；Native Dash history
  已证明产生 canonical input/Turn start/Turn complete，等待 production
  AgentRun/API/frontend 纵向 consumer。
- [x] Native exact fork 与 Codex native fork。
- [x] Companion Full exact history fork；selected child AgentFrame/surface/profile 在
  Activate 前独立应用，并覆盖 parent 与 specialist profile 不同的 tracer。
- [x] Companion Compact / WorkflowOnly / ConstraintsOnly fresh create。
- [x] Companion channel、gate、adoption、result、mailbox。
- [x] Dash collaboration tool request/respond 经 final callback/broker 执行，并覆盖稳定
  effect 与 Broker restart replay；既有 saga suite 覆盖 gate/channel/mailbox/result。
- [x] Workflow AgentCall，并覆盖重启 inspect 后补齐 Product convergence。
- [x] Routine trigger → AgentRun → terminal，并覆盖 prepared 状态进程重启恢复。
- [x] Workspace Module list/describe/operate/invoke/present 经 final handler/broker；
  write route 与 Host generation、Product binding、immutable Frame、
  AppliedResourceSurface、presentation effect 具有联合 tracer。
- [ ] Canvas read/write/promotion/diagnostics。
- [ ] VFS surface read/list/search。
- [x] Lifecycle VFS canonical `events.json` 与 derived indexes。
- [ ] Terminal create/input/resize/close/projection。
- [ ] Wait activity 与 gate/terminal convergence。
- [x] Complete Agent Tool/Hook callback、permission、deadline、effect correlation 已通过；
  Product Tool families 与 Product-only Hook effect 均进入最终 typed 路径。
- [x] MCP dynamic tool discovery → surface apply → Host callback → Broker execution。
- [x] Compaction Dash exact / Codex native projection。
- [x] reconnect cursor tail与gap snapshot reload。
- [x] Runtime、Fork、Companion、selected frame 与现有 Tool/Hook callback 的
  restart/unknown outcome/recovery 使用同一 command/effect/child identity；Routine、
  Workflow 与剩余 Product Tool families 完成后重新跑总门禁。

## C5 — Final Hard Cut

Application/Product 领域不属于 Hard Cut，也不是本任务的重构对象。Companion、Frame、Routine、Workflow、
Workspace、Canvas、Terminal、Wait、Lifecycle 只迁移 Runtime 接入 seam；其业务规则、
route、worker、权限、gate、mailbox 与用户可见行为必须保持。移除 module export、
route mount、AppState composition 或 Product caller 不能证明旧 Runtime 已被替代。
`agentdash-application-hooks` 同样保留 Product presets、workflow policy 与 effects；
只有其中已经被 typed Surface/Tool Broker/Agent callback 替代的 Runtime execution
实现可以逐项进入 manifest。

每个候选项必须填写：

```text
Legacy:
Target replacement:
Production callers:
Composition:
Repository/schema:
Projection/consumer:
Behavior tracer:
Negative evidence:
```

候选范围：

- [ ] platform `RuntimeSession*` delivery/live/capability/DTO/event。
- [ ] universal `RuntimeJournalFact` / journal persistence/readers。
- [ ] 已被 Complete Agent Host 替代的 connector/driver/executor。
- [ ] 已被 Tool Broker / AgentHostCallbacks 替代的 Hook execution owner。
- [ ] `agentdash-agent-types` 中已迁到最终 owner 的类型。
- [ ] protocol 中 Backbone platform/product、Runtime internal、journal carrier。
- [ ] Relay Prompt/SessionEvent legacy variants。
- [ ] 无消费者的 SPI Agent delegate/re-export。
- [ ] 拆解并删除 `agentdash-platform-spi::session_persistence` 语义聚合：仍有消费者的
  AgentFrame/capability transition 归 Product owner，Runtime command/binding/recovery
  归 Runtime Contract，history/compaction/lineage 归 Complete Agent 或 canonical
  conversation owner；迁移完成后不保留同义 SPI 外壳。
- [ ] 旧 schema tables/fields/indexes。

### 当前只读 readiness manifest

以下清单只冻结候选，不授权提前删除。所有条目仍需等待 C3/C4 Product tracers 通过，
并在删除提交中补齐 behavior tracer 与最终 negative evidence。

#### 零生产消费者的旧 Runtime/journal 段

- [ ] `agentdash-platform-spi::session_persistence` 中仅定义/re-export 的旧
  journal/read-model 类型：`SessionMeta`、`ExecutionStatus`、
  `PersistedSessionEvent`、`SessionEventBacklog/Page`、`SessionCompaction*`、
  `SessionProjection*`、`SessionLineage*`、`NewCompactionProjectionCommit`、
  `CompactionProjectionCommitResult` 与 `SESSION_PROJECTION_KIND_*`。
- [ ] `agentdash-api::dto::session` 中零 caller 的
  `AgentRunJournalStreamQuery` / `AgentRunJournalEventsQuery`；同文件仍被 Product
  使用的 ContextAudit DTO 保留。
- [ ] `agentdash-agent-protocol::PlatformEvent::ExecutorSessionBound`；确认无
  producer/consumer 后同步 canonical generated TypeScript 与 freshness。
- [ ] 将 `agentdash-agent::model::message` 中仅存的
  `PersistedSessionEvent` 历史注释改为 Agent history entry coordinate。

#### 需要先完成 seam cut 的旧 execution 壳

- [ ] `agentdash-application-hooks` 保留 Product presets/rules/script/effects、
  `evaluate_complete_agent_hook` 与 plan compiler。只有在 production hook tracer
  通过后，才移出 `load_frame_snapshot` 的 Product loader，并删除仅 self/tests 消费的
  aggregate `ExecutionHookProvider::evaluate_frame_hook/refresh_frame_snapshot`、
  `NoopExecutionHookProvider`、`HookEvaluationQuery` 与相应 SPI re-export。
- [ ] `session_persistence` 中仍被 Product 使用的 capability transition、
  `RuntimeCommandRecord`、`SessionStoreError` 先迁到 AgentRun/Application owner；
  在消费者归零前不删除 module。
- [ ] 旧 `RuntimeToolProvider` 与 Application-side composer/adapter 当前仍有 Product
  caller；等待 final Product callback/catalog tracer 后，只删除 SPI
  provider/re-export 与已被 typed Broker executor 替代的接入壳。Agent Core 的
  `AgentTool` contract、各 Product tool command/业务实现，以及 Companion、Workflow、
  Wait、VFS 等能力本体均保留。

#### 明确保留

- [x] `agentdash-agent-protocol` 保留为 canonical App Server extension +
  conversation carrier；它不是 universal journal。
- [x] `agentdash-application-hooks` crate 保留 Product hook ownership。
- [x] `agentdash-agent::AgentTool` 保留为 Agent Core 可调用工具的极简合同；它不因旧
  Application `RuntimeToolProvider` 接入壳被替代而进入 deletion manifest。
- [x] `BackboneEnvelope` 不按名称判定为旧 journal，按真实 producer/consumer 审计。
- [x] Companion、Frame、Routine、Workflow、Workspace、Canvas、Terminal、Wait、
  Lifecycle 不进入 Runtime deletion manifest。

## RuntimeThread semantic cut

平台 Runtime 只用 `RuntimeThread` 表达 Complete Agent 的运行实例坐标。这个语义从
Domain 到 UI 一次切换，使 Product binding、Lifecycle association、Hook provenance、
Companion relation 和工具执行上下文引用同一个稳定坐标，而不把平台运行状态描述成
history-derived `AgentSession`。

### 同一 checkpoint 内切换

- [ ] Domain / workflow：`RuntimeThreadPolicy`、
  `ExecutorRunRef::RuntimeThread` 与相关 orchestration value objects。
- [ ] Contracts / generated TypeScript：workflow、mailbox、permission 与 frame
  materialization 中的 `runtime_thread_id` / `RuntimeThreadRef`。
- [ ] Application / Lifecycle / Hooks：Frame construction、Lifecycle dispatch 与
  association、Hook provenance、Companion gate/tool context/preflight、Canvas 与 Runtime
  tool context。
- [ ] API / frontend：workflow、canvas、extension runtime、mailbox DTO 与 Workflow
  inspector/store 全部消费同一 RuntimeThread contract。
- [ ] Product read models：AgentRun workspace、conversation execution、command
  availability 只读取 fenced Product binding + canonical Managed Runtime snapshot；其
  runtime coordinate 为 `runtime_thread_id`。
- [ ] VFS、Extension、Canvas、Runtime Tool 与 Frame policy 中的
  `SessionRuntime*`、`RuntimeContext::Session`、`SharedSessionToolServices*` 同步切换为
  RuntimeThread 语义；倒装命名不能绕过 semantic cut。
- [ ] 生成、编译和行为验证完成后，非 migration/fixture 的平台
  `RuntimeSession*` / `SessionRuntime*` negative search 为零。

### 保留的 Session ownership

- [ ] `agentdash-agent-protocol` 与 07-12 App Server presentation 的 `session_id`
  保持 canonical conversation identity，不参与平台 RuntimeThread 命名切换。
- [ ] `packages/app-web/src/features/session` 继续作为 canonical conversation
  reducer/renderer；输入仅来自 Managed Runtime 的 `conversation_history` 和 change
  coordinate。
- [ ] Complete Agent / Dash Agent 内部 `AgentSession` 仅在状态可由 ordered history
  完整重建时使用该名称，并由 Complete Agent 自己拥有。
- [ ] 历史 migration 中的旧表/字段名以及验证旧表已退出的测试保留为 schema 演进
  证据。

### 退出项

- [ ] 移除只转发已退出 runtime-session boundary 的 application re-export。
- [ ] 清理已完成 cutover 后仍描述旧 owner 的 activation inventory 与业务注释。

## 最终门禁

- [ ] final migration、repositories 和 production composition 使用同一 schema。
- [ ] canonical Rust/TypeScript protocol roots、schema lock、freshness 与 parity 通过。
- [ ] `cargo metadata` 符合最终 crate DAG。
- [ ] 旧 owner negative search 只剩 migration 删除语句或历史任务文档。
- [ ] Rust affected crates/tests 通过。
- [ ] PostgreSQL behavior、CAS、outbox、recovery tests 通过。
- [ ] frontend typecheck、session tests 与 Product feature tests 通过。
- [ ] 一条真实 production tracer 覆盖：

```text
Product command
  -> Managed Runtime operation/change
  -> Host placement/effect
  -> Complete Agent
  -> Agent-owned history
  -> canonical conversation
  -> Product API/UI/VFS consumer
```

## 已闭合 replacement evidence

### Lifecycle canonical history / AgentRun journal read

```text
Legacy:
  AgentRunJournal context/history reader 与独立 journal identity
Target replacement:
  AgentRunProductProjectionQueryPort.runtime_snapshot/runtime_changes
  + LifecycleHistoryQueryPort
Production callers:
  /agent-runs/{run}/{agent}/runtime snapshot/change routes
  + useManagedRuntimeFeed
  + LifecycleMountProvider
Composition:
  AppState.agent_run_product_projection
  + production Lifecycle mount provider registry
Repository/schema:
  committed AgentRunProductRuntimeBinding
  + Managed Runtime canonical projection
Projection/consumer:
  canonical Session reducer/renderer
  + lifecycle runtimeTraceSummaries
  + lifecycle://.../session/events.json
Behavior tracer:
  Runtime reconnect/gap reload
  + exact canonical events.json record
  + frontend baseline side-effect fence
Negative evidence:
  API/Lifecycle/frontend read path no longer references AgentRunJournal
```

### Product/transport variants in conversation protocol

```text
Legacy:
  ControlPlaneProjectionChanged
  + WorkspaceModulePresentationRequested
  + TerminalOutput/PtyTerminalStateChanged conversation variants
Target replacement:
  ProjectEventStreamEnvelope
  + WorkspaceModulePresentation Product feed
  + AgentRunTerminal Product feed
Production callers:
  useAgentRunWorkspaceControlPlane
  + agent-run list project event subscriber
  + workspace presentation pending consumer
  + terminal projection consumer
Composition:
  project_control_plane_events
  + AgentRunProductProjectionQueryPort
Repository/schema:
  Product projection repositories and outboxes
Projection/consumer:
  AgentRun workspace/list/presentation/terminal UI
Behavior tracer:
  project invalidation validator + workspace/list plans
  + workspace presentation and terminal feed tests
Negative evidence:
  canonical PlatformEvent/codegen/frontend Session reducer no longer owns these variants
```
