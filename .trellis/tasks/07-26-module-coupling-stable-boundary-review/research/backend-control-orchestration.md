# Research: 后端控制编排与协作稳定边界

- Query: 审计 Workflow、LifecycleRun/Agent/Frame、Routine、Companion/Subagent、Channel、Interaction、Gate/Wait、Permission、Capability 与 Hook policy source 的生产入口、授权、命令/状态所有者、事务/恢复、事件投影和跨模块耦合；Agent Runtime 仅作为下游边界。特别交叉核对 API routes、Workflow MCP tools、cron/Workflow recovery workers，并覆盖 interactions、operation_workshop、companion_gates、task_plan、lifecycle_views、story_runs、routines。
- Scope: internal
- Date: 2026-07-26

## Findings

### 结论摘要

控制编排域并非整体失控。`OrchestrationExecutorLauncher` 已经形成一个很好的稳定边界：统一 reducer、`LifecycleRun.revision` CAS、稳定 claim/effect identity、durable Function/HumanGate receipt、后台恢复扫描共同构成可重放执行协议（`crates/agentdash-application-workflow/src/orchestration/executor_launcher.rs:117-126,171-255,606-617,708-740`；`crates/agentdash-api/src/bootstrap/background_workers.rs:38-68`）。Interaction command 的“状态 + event + receipt + effect intent”也已经在一个 PostgreSQL transaction 中原子提交（`crates/agentdash-infrastructure/src/persistence/postgres/interaction_repository.rs:403-449`）。

真正的高风险来自这些正确核心周围的生产装配和旁路写入：

1. **P0 — `LifecycleRun` 写 authority 分裂。** Workflow executor 使用 CAS，但 runtime terminal、platform advance、Task Plan、Hook execution log 等生产写者仍执行整个 aggregate 的无 revision `update`，会互相覆盖 `orchestrations/tasks/status/execution_log`。
2. **P0 — Lifecycle dispatch 是跨 7 个 repository 的非原子 saga，且没有完整 durable stage receipt。** 失败可留下 run、agent、frame、subject association、lineage/gate 与 orchestration claim 不一致。
3. **P0 — Routine cron 没有 durable occurrence/lease owner。** 每个 API 实例都启动内存 scheduler，同一计划时刻可被多实例各创建随机 execution 并产生重复下游副作用。
4. **P0 — Interaction effect dispatcher 只存在于 application/tests，未在生产 composition root 启动。** 命令可返回 committed，但 durable effect intent 永远保持 pending。
5. **P0 — `POST /companion-gates/{id}/respond` 使用未注入 delivery 的 service。** gate 先被 resolve，随后默认 delivery 返回 Internal；调用方收到失败、requesting Agent 未被唤醒，重试又因 gate 已关闭而冲突。
6. **P0 — Gate producer terminal convergence 实现存在但没有生产 terminal projector/worker 调用。** child 异常/取消终结时，依赖 producer terminal 的 open wait gate 可永久悬挂。
7. **P0（与业务资产审计交叉确认）— Task HTTP 写入口只要求 `ProjectPermission::Use`，应用 policy hook 永远 allow，并且同样走无 CAS aggregate update。**
8. **P1 — Wait exec source 依赖明确标注“纯内存、不持久化”的 terminal registry。** Server 重启后无法恢复对既有 exec terminal 的等待/观测。
9. **P1 — Channel 在 Companion 内部形成了可用路径，但 semantic channel/message/delivery identity 不稳定；provider ingress 仍未生产装配。**
10. **P1 — Workflow MCP 保存定义绕过 HTTP 已复用的 catalog command owner，复制版本推进和引用校验。**
11. **P1 — `AgentRunPermissionFacade`/PendingApproval 是无实现、无生产消费者的公开边界；当前真实授权由 applied surface/resource grant 与 Runtime 内部 approval 分别承担。**
12. **P1 — Workflow run create/create-and-continue 没有 client command identity，HTTP transport retry 会创建新 run。**
13. **P1 — Story/Relay MCP 直接 mutation repository，绕过 HTTP 已采用的 Story application owner。** 尤其 Story context/details 更新不会执行 canonical context validation 与 inline-file projection sync。

这些问题应按“先建立唯一命令/事务 owner，再移除旁路，再加入生产装配与恢复 gate”的顺序处理；不应继续在 route、tool 或 worker 上追加局部补丁。

### 审计依据与目标不变量

- Interface 只拥有鉴权、DTO 与错误映射，application command/query service 拥有用例，跨 aggregate 写入需要显式 command port/unit of work：`.trellis/spec/backend/architecture.md:12-15,62,81-94`。
- `RepositorySet` 是 composition helper，不是业务 service locator；业务服务应暴露具名依赖：`.trellis/spec/backend/repository-pattern.md:18-20,46-48`。
- Workflow 拥有 plan/orchestration/node/attempt/product evidence；concrete Agent 拥有会话，Runtime 是下游执行边界：`.trellis/spec/backend/workflow/architecture.md`、`.trellis/spec/backend/activity-lifecycle.md`、`.trellis/spec/backend/lifecycle-run-link.md`。
- Gate owner 必须保存 wait fact 和 downstream receipt；producer terminal、人工响应与 child result 都必须幂等收敛并唤醒目标：`.trellis/spec/backend/lifecycle-edge.md`、`.trellis/spec/backend/channel/architecture.md`。
- Interaction state/event/receipt/effect intent 必须原子提交，effect 由可恢复 dispatcher 执行：`.trellis/spec/backend/interaction/architecture.md`。
- Hook policy source 应来自当前 AgentFrame/Lifecycle 的 active workflow projection 和 effective contract，而不是 route/session 的另一套推断：`.trellis/spec/backend/hooks/architecture.md`。
- Capability、resource authorization 与 dynamic approval 是不同阶段，不应由一个未装配 facade 在文档上假装统一：`.trellis/spec/backend/capability/architecture.md`、`.trellis/spec/backend/permission/architecture.md`。

### 生产入口与用例台账

下表的“生产”表示入口已由 `crates/agentdash-api/src/routes.rs:74-124`、AppState/bootstrap 或 MCP server 实际装配；“实现未装配”表示代码和测试存在，但没有找到 production caller/composition。

| 用例 | 生产入口 | 授权 owner | 命令/状态 owner | 读/写 owner | reducer / transaction / 外部 effect | contract / event / projection | 恢复与幂等 | 消费者与 tests/gates | 耦合判断 |
|---|---|---|---|---|---|---|---|---|---|
| Workflow/Procedure 定义查询与保存 | HTTP `/agent-procedures*`、`/workflow-graphs*`（`routes/workflows.rs:74-107`）；MCP `list/get/upsert_*`（`agentdash-mcp/src/servers/workflow.rs:441-599`） | HTTP/MCP Project Configure（MCP `:533-568`） | HTTP 使用 `ActivityLifecycleCatalogService`；MCP 自己直接 upsert repo | procedure/graph repos | 单 repo 写；MCP 自行 `existing.version + 1` | workflow contracts、validation issues | 无 expected version/client command receipt | HTTP catalog tests；MCP 仅 schema/registered tool tests `:722-762` | **P1：两个写 owner**；查询共享 repo 合理，命令规则重复不合理。 |
| LifecycleRun create / create-and-continue | POST `/lifecycle-runs`、`/commands/create-and-continue`（`routes/workflows.rs:108-112,409-455`） | Project Configure | `LifecycleRunCommandService` → `LifecycleDispatchService` | graph、run、agent/frame/association/gate/lineage repos | 多 repo saga；continue 再进入 executor | `LifecycleRunView`、orchestration state | request DTO 只有 lifecycle id/key/project id（`dto/workflow.rs:18-22`），无 command id | Workflow route/service tests | **P0 dispatch consistency + P1 transport idempotency**。 |
| Workflow continue/drain/human decision | POST `/lifecycle-runs/{id}/continue`、`/drain`、human decision（`routes/workflows.rs:117-129`） | Project Configure | `OrchestrationExecutorLauncher` | run + workflow effect/gate repos | reducer + CAS；durable Function/HumanGate effect/receipt | runtime node events、gate decision receipt、run view | stable effect/claim，CAS conflict，1s recovery scan | executor concurrency/replay tests `executor_launcher.rs:1819-1913`；worker `background_workers.rs:38-68` | **合理内聚的正例**。 |
| Runtime terminal / platform complete 推进 Workflow | Runtime projection/terminal producer、platform tool → `LifecycleOrchestrator` | Product target/resource grant | 理论上 reducer；实际 persistence 由 orchestrator 直接写 | run、binding、frame、output artifacts | `apply_orchestration_event_to_run` 后 broad `update` | runtime terminal summary、node result | terminal 预检查幂等，但没有 CAS retry | orchestrator unit tests | **P0：绕过 executor 的 CAS write owner**。 |
| Lifecycle Agent/Frame 用户控制 | runtime snapshot/live/command、composer-submit、fork/fork-submit/cancel（`routes/lifecycle_agents.rs:51-104`） | Project Use；delete 为 Configure（`:107-152,328-372,546-558,705-757`） | Product command/input/fork ports；Runtime 仅下游 | run/agent/frame/binding + product ports | 多数 command 有 `client_command_id` 和 product receipt | ManagedRuntimeSnapshot/live/product command response | product delivery/command receipt 可重放 | agent run interaction contract tests | **大体合理**；权限语义是“可使用者可操作”，必须由权限矩阵明确，不能由各 route 自解释。 |
| Lifecycle views / Story runs | GET lifecycle run view、subject execution、project active agents（`lifecycle_views.rs:29-149`）；GET story runs/active（`story_runs.rs:22-93`） | Project/Story Use | `LifecycleRunViewQueryService` | run/association/agent/frame/gate read models | 只读 projection composition | `LifecycleRunView`、`SubjectExecutionView` | 无写入；fresh query | route/query tests | **合理 query boundary**；`story_runs` 没有第二套 StoryRun aggregate。 |
| Task Plan HTTP/MCP/Runtime/continuation 写入 | HTTP `/lifecycle-runs/{run}/tasks*`、`/agent-runs/{run}/agents/{agent}/tasks*`（`task_plan.rs:32-54`）；Story MCP create/batch create（`agentdash-mcp/src/servers/story.rs:454-520`）；Runtime task_write（`task/tools.rs:556-617`）；Companion continuation（`companion/continuation.rs:338-348`） | HTTP 所有读写均 Project Use（`:71-181`）；Story MCP Configure；Runtime applied task grant + run/project/task scope | 所有入口最终调用同一组 free functions；policy hook永远 allow；`TaskLockMap`未在 production caller出现 | `LifecycleRun.tasks` | 每个 operation各自 load/mutate/broad update；snapshot/batch是多个独立 commit | HTTP DTO、MCP JSON、Runtime typed changeset | 无 aggregate expected revision/client command receipt；Runtime外围operation receipt不能防内部 stale aggregate覆盖 | domain/workspace/tool tests，无跨入口并发 gate | **P0：policy不一致 + lost update**；与 `backend-business-assets.md` 独立交叉确认。共享 free functions并不等于共享可靠 command owner。 |
| Story create/update/status | HTTP `/stories*`；Relay MCP create/status；Story MCP context/details/status | HTTP/MCP Configure | HTTP 使用 `create_story_record/update_story_record`；两个 MCP server直接 repo mutation | story + inline-file projection；delete另有 state-change | canonical owner仍是 repo commit后再 sync inline files；MCP完全跳过 sync | Story、context、inline VFS projection | 无 expected version/client command receipt | HTTP management tests；MCP主要 tool schema tests | **P1：MCP绕过 canonical owner**；canonical 自身还应收敛 transaction/outbox。 |
| Routine CRUD / enable / token / executions / webhook | Auth routes `/routines*` 与 public webhook（`routes/routines.rs:44-75,77-337`） | reads=Use，mutations=Configure；webhook=token possession | CRUD 在 route；execution 在 `RoutineExecutor`；schedule reload 为进程内 Notify | routine、execution、project/agent/workspace、dispatch repos | execution 分阶段多 repo；product launch/input 外部 effect | Routine/Execution response、dispatch refs | 只恢复 pending 且已有 `dispatch_run_id`；无 trigger occurrence unique key | executor/recovery tests；无 multi-instance scheduler test | **P0 occurrence owner；P1 command/admission/recovery 分裂**。 |
| Cron/Workflow recovery workers | AppState 每实例 `spawn_cron_scheduler`（`app_state.rs:556-569`）；post-AppState workflow recovery（`background_workers.rs:38-68`） | 系统 worker | cron 内存 entries；Workflow durable launcher | routine/execution；run/effect repos | cron 每 due entry `tokio::spawn fire_scheduled`；Workflow 扫描 recoverable run | execution/run diagnostics | Workflow 有 durable recovery；cron 无 distributed claim | scheduler tests偏解析/单实例；executor recovery tests | Workflow **合理**；Routine **P0**。 |
| Companion/Subagent dispatch/result/request/response | applied Runtime tools → companion services；human response API `/companion-gates/{id}/respond` | Runtime applied resource grant；API Project Use | `CompanionGateControlService`、dispatch/product input ports | run/agent/frame/lineage/gate/channel/delivery marker | gate resolver + product input handoff | gate delivery intents、input handoff receipt | child result 有 gate result marker；其他路径依赖 stable client command id | companion/gate tests使用完整 injected delivery | 核心 gate intent 设计合理；**API composition P0**，terminal convergence **P0 未装配**。 |
| Channel internal companion delivery | Companion delivery helpers | Product target/resource grant | `ChannelService` + companion helper | `LifecycleRun.channel_registry` typed mutation | Agent input 先执行，随后记录 channel delivery state | `ChannelMessage`、`ChannelDeliveryIntent/State` | Agent input client command id稳定；channel message/delivery id随机 | channel/companion tests，无并发 semantic ensure test | **P1 identity/ordering**。Provider ingress resolver 在生产未装配，应视为 reachability gap。 |
| Interaction definition/instance/command/event/presentation | `/interaction-*` routes（`routes/interactions.rs:67-138`）；Operation Gateway 也暴露 interaction commands（`bootstrap/runtime_gateway.rs:98-103,125-340`） | Project Configure/Use + owner/scope admission | `InteractionCommandService` | definition/instance/command transaction/event/presentation repos | PostgreSQL row lock transaction 原子 commit state/event/effect/receipt | event、state revision、command receipt、effect intent | command id + digest；effect claim lease schema已实现 | service/repo tests | command transaction **合理正例**；**P0 effect worker 未装配**。 |
| Operation Workshop discover/invoke/script | POST surface/invoke/preflight/run（`operation_workshop.rs:35-52`） | Project Use + context owner/scope；gateway authority resolver二次 admission | `BoundOperationHost` / `OperationGateway` / script engine | provider catalog与各 operation owner | invoke 外部 effect；script token pins source/manifest | authority revision、operation descriptor/result | operation replay policy/preflight token；具体幂等由 operation contract | gateway/script tests | **合理 host boundary**；它不会替 Interaction pending effect 执行 worker。 |
| Wait tool | applied Runtime `wait` tool → `WaitActivityService` | Product grant + runtime target fence | wait query service | durable lifecycle gates + process terminal registry | polling，无业务写 | typed wait outcome | gate 可跨重启；exec terminal registry不可 | wait tests均以内存 registry seed | **P1 restart gap**。 |
| Permission / Capability | Runtime tool request → capability availability + applied resource grant + Runtime approval | applied surface producer/resource authorizer；Runtime provider approval | `ProductRuntimeToolAuthorizer` 等真实 owner | applied surface/binding/capability projection | deny/allow 或 Runtime pending approval | tool surface/digest/grants、Agent events | surface digest/target fence；approval 是 Runtime 进程协议 | runtime authorization tests | 真实链路分层基本合理；`AgentRunPermissionFacade` 是 **P1 dead boundary/spec drift**。 |
| Hook policy source / hook execution log | AgentFrame hook snapshot/provider | active workflow projection owner | `ProductHookWorkflowProjection` + hook provider | frame/run/procedure；execution log 写 run | policy query正确；log flush broad update | immutable hook snapshot、effective contract、diagnostic log | policy snapshot pinned；log 写无 CAS | hook provider tests | policy source **合理正例**；execution log persistence 并入 **P0 run write split**。 |

### 文件清单

| 文件 | 一句话说明 |
|---|---|
| `crates/agentdash-api/src/routes.rs` | HTTP route composition root，证明本次审计模块均为生产挂载入口。 |
| `crates/agentdash-api/src/routes/workflows.rs` | Workflow 定义、LifecycleRun create/continue/drain/human decision HTTP 入口。 |
| `crates/agentdash-mcp/src/servers/workflow.rs` | Workflow MCP 五个 tools 及其独立 repository upsert 实现。 |
| `crates/agentdash-application-workflow/src/catalog.rs` | HTTP 已复用的 Workflow catalog command owner。 |
| `crates/agentdash-application-workflow/src/orchestration/runtime.rs` | Workflow orchestration reducer 事实源。 |
| `crates/agentdash-application-workflow/src/orchestration/executor_launcher.rs` | CAS、durable effect/gate receipt 与恢复执行核心。 |
| `crates/agentdash-api/src/bootstrap/background_workers.rs` | Workflow recoverable run 扫描 worker。 |
| `crates/agentdash-domain/src/workflow/repository.rs` | LifecycleRun repository、CAS 语义及 gate/channel repository ports。 |
| `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs` | LifecycleRun broad update、CAS 与 channel row-lock mutation。 |
| `crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs` | Lifecycle dispatch 跨 run/agent/frame/association/gate/lineage 的编排顺序。 |
| `crates/agentdash-application-lifecycle/src/lifecycle/dispatch/*` | run starter、agent runtime materializer、subject association、lineage/gate writers。 |
| `crates/agentdash-application-lifecycle/src/lifecycle/orchestrator.rs` | Runtime terminal/platform advance 到 reducer及 broad update 的生产路径。 |
| `crates/agentdash-application-lifecycle/src/lifecycle/run_command_service.rs` | LifecycleRun create/create-and-continue application command。 |
| `crates/agentdash-api/src/routes/lifecycle_agents.rs` | AgentRun 用户控制、投影、terminal、fork/cancel/composer 入口。 |
| `crates/agentdash-api/src/routes/lifecycle_views.rs` | Lifecycle/subject/project agent read projection routes。 |
| `crates/agentdash-api/src/routes/story_runs.rs` | Story 只读地复用 SubjectExecutionView。 |
| `crates/agentdash-api/src/routes/task_plan.rs` | Task Plan HTTP 读写授权入口。 |
| `crates/agentdash-application/src/task/plan.rs` | Task mutation、always-allow policy hook 与 broad run update。 |
| `crates/agentdash-application/src/task/workspace.rs` | Runtime typed Task changeset/snapshot最终逐项调用同一 broad-update free functions。 |
| `crates/agentdash-application/src/task/tools.rs` | applied Runtime task_read/task_write production adapter。 |
| `crates/agentdash-application/src/task/lock.rs` | 仅测试/未挂载的进程内 TaskLockMap；不能承担跨入口或多实例并发。 |
| `crates/agentdash-mcp/src/servers/story.rs` | Story-scoped MCP context/details/status direct mutation与 Task create/batch入口。 |
| `crates/agentdash-mcp/src/servers/relay.rs` | Relay MCP Story create/status direct repository mutation。 |
| `crates/agentdash-application/src/story/management.rs` | HTTP 已使用的 Story validation、mutation和 inline-file sync owner。 |
| `crates/agentdash-api/src/routes/routines.rs` | Routine CRUD、enable、token、execution、public webhook。 |
| `crates/agentdash-application/src/routine/executor.rs` | Routine execution 分阶段 dispatch、delivery 与恢复。 |
| `crates/agentdash-application/src/scheduling/cron_scheduler.rs` | 进程内 cron entries 与每 occurrence 的异步 fire。 |
| `crates/agentdash-infrastructure/src/persistence/postgres/routine_repository.rs` | Routine execution persistence 和 recoverable query。 |
| `crates/agentdash-infrastructure/migrations/0001_init.sql` | Routine、Interaction、Workflow/Gate/receipt/effect schema 事实源。 |
| `crates/agentdash-api/src/routes/companion_gates.rs` | Human response gate API 的错误 composition。 |
| `crates/agentdash-application/src/companion/gate_control.rs` | Gate resolve、delivery intents、默认未配置 delivery 与 child result convergence。 |
| `crates/agentdash-application/src/companion/tools.rs` | Companion Runtime tools、Channel helper、product input delivery/marker。 |
| `crates/agentdash-application-workflow/src/gate/gate_wait_policy.rs` | Producer terminal convergence 的 domain/application workflow 实现。 |
| `crates/agentdash-application/src/gate_wait_policy.rs` | Product binding + input handoff wake adapter；生产未调用。 |
| `crates/agentdash-application/src/channel.rs` | Channel service、provider-neutral ingress/egress 与 registry mutation。 |
| `crates/agentdash-domain/src/channel/mod.rs` | Channel、participant、message、delivery identity/policy。 |
| `crates/agentdash-api/src/routes/interactions.rs` | Interaction definition/instance/command/event/presentation routes。 |
| `crates/agentdash-application/src/interaction/service.rs` | Interaction command reducer、admission 与 effect intent 构建。 |
| `crates/agentdash-application/src/interaction/effect_dispatcher.rs` | 可恢复 effect dispatcher 实现，仅测试可达。 |
| `crates/agentdash-infrastructure/src/persistence/postgres/interaction_repository.rs` | Interaction atomic command transaction 与 effect claim lease。 |
| `crates/agentdash-api/src/routes/operation_workshop.rs` | Operation discover/invoke/script HTTP host。 |
| `crates/agentdash-api/src/bootstrap/runtime_gateway.rs` | Operation Gateway production provider composition 与 Interaction operation access。 |
| `crates/agentdash-application/src/wait_activity/service.rs` | Gate 与 exec activity wait aggregation。 |
| `crates/agentdash-application-agentrun/src/agent_run/terminal_registry.rs` | 明确为纯内存的 exec terminal registry。 |
| `crates/agentdash-application-ports/src/agent_run_permission.rs` | 未装配的 AgentRunPermissionFacade/PendingApproval contract。 |
| `crates/agentdash-infrastructure/src/runtime_tool_authorization.rs` | 当前真实 applied resource/tool grant authorizer。 |
| `crates/agentdash-application/src/hook_workflow_projection.rs` | active workflow projection 到 Hook policy 的 adapter。 |
| `crates/agentdash-application-hooks/src/provider.rs` | immutable hook snapshot/effective contract provider。 |
| `crates/agentdash-application-lifecycle/src/lifecycle/execution_log.rs` | Hook pending log 写回 LifecycleRun 的 broad update 路径。 |

### P0/P1 风险与可执行工作包

#### CO-01 — P0：统一 LifecycleRun command store，移除 aggregate broad-update 旁路

**证据与触发器**

- repository contract 已明确执行路径必须使用 CAS，且 `run.revision == expected_revision + 1`（`crates/agentdash-domain/src/workflow/repository.rs:81-101`）。
- PostgreSQL `update` 不带 revision predicate，一次覆盖 topology、orchestrations、tasks、status、execution_log（`crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:504-516`）；同文件已有正确 CAS（`:518-570`）。
- Workflow executor 正确使用 CAS（`executor_launcher.rs:606-617`），但 runtime terminal 使用 reducer 后 broad update（`lifecycle/orchestrator.rs:131-185`），Task create/update/archive/status/reorder 使用 broad update（`task/plan.rs:104-235`），Hook log flush 也如此（`lifecycle/execution_log.rs:41-68`）。
- Task 的 HTTP、Story MCP、Runtime task_write、Companion continuation 虽然最终共享上述 free functions，却没有共享 command receipt/revision protocol；Runtime `TaskPlanWorkspace` 的 patch/snapshot还会逐 operation做多次独立 aggregate commit（`task/workspace.rs:111-157,160-255,259-377`）。
- `TaskLockMap` 只在 `task/lock.rs` 自身/tests出现，未找到 production caller；即使挂载，它也是进程内 per-task lock，既不能保护同一 run不同 task对整行 snapshot的竞争，也不能跨 API/MCP/worker实例。
- 触发器：同一 run 上 terminal event、HTTP/MCP/Runtime/continuation Task 编辑、Hook 日志 flush、platform complete 或 executor drain 并发。

**爆炸半径**

整个 LifecycleRun 的 orchestration node、tasks、status、execution log 可被旧 snapshot 后写覆盖；会造成节点重复执行/丢终态、Task 回滚、Hook evidence 丢失。竞争不是“Task内部并发”而是 **Task计划与 lifecycle execution/status 共用一个整行 aggregate write set**，所以 CO-01 是 CO-07 授权修复、Runtime Task policy和所有 Task入口统一之前的 persistence 前置。Channel registry 使用独立 row-lock JSON mutation（`workflow_repository.rs:600-634`），但这不能修复其余字段。

**目标边界**

建立唯一 `LifecycleRunCommandStore`，只接受 typed mutation/event：

- `ApplyOrchestrationEvent`
- `MutateTaskPlan`
- `AppendExecutionLog`
- `TransitionRunStatus`

owner 在一个 transaction 中重新读取当前 revision、应用 reducer、CAS commit；冲突按命令的 stable identity 重放。产品路径不得再拿 mutable aggregate 调 `update`。

**工作包**

1. 列出所有 production `LifecycleRunRepository::update` caller 并分类为上述 typed command。
2. 为 event/task/log 增加 stable command id/digest；实现统一 CAS retry。
3. 迁移 orchestrator、Task Plan、Hook log 和 dispatch reducer bridge。
4. 把 broad `update` 降为 fixture/bootstrap-only 或从 port 删除。
5. 加 task/task、task/terminal、log/terminal、dispatch/executor 并发 characterization tests。

**预防 guard**

- 静态 architecture test：production crate 不得调用 `LifecycleRunRepository::update`。
- PostgreSQL integration test：两个 stale writer 必有一个 revision conflict，typed retry 后两份事实都存在。
- route/tool contract 要求 mutation command identity。

#### CO-02 — P0：把 Lifecycle dispatch 收敛为 transactional command/saga owner

**证据与触发器**

`LifecycleDispatchService` 同时持有 run、graph、agent、frame、association、gate、lineage 七个 repository（`dispatch_service.rs:42-55`）。graph dispatch 顺序是：

1. prepare/create/update run；
2. materialize agent/frame；
3. 写 subject association；
4. 写 lineage 和 gate；
5. 最后 reducer `mark_node_claimed`（`dispatch_service.rs:362-408`）。

这些是独立 await/repository commit。plain dispatch 也按 run → agent/frame → association → relation 顺序写（`:411-437`）。部分 routine 路径预分配稳定 run/agent/runtime identities（`routine/executor.rs:321-330`），这是正确方向，但 generic dispatch 仍会生成随机 delivery runtime、gate correlation（`dispatch/agent_runtime_materializer.rs:53-64,119-129`；`dispatch/lifecycle_relation_writer.rs:101-112`）。

触发器是在任意 stage 后数据库/进程失败或请求重试。

**爆炸半径**

可能留下未 claimed 的已启动 Agent、没有 association 的 run、存在 lineage 但无 gate、open gate 指向未绑定 frame，破坏 run/agent/frame owner invariant，并让 recovery worker看不到正确 recoverable node。

**目标边界**

`LifecycleDispatchCommandPort` 接受 stable `dispatch_command_id` 和全部 Product identities。Product facts应在同一 DB transaction 中提交；确实跨系统的 Runtime launch/input 必须在 committed outbox 后执行。若暂时不能单事务，则建立显式 durable stage state machine，不能靠“查到就复用”的隐式 saga。

**工作包**

1. 新增 dispatch command/receipt schema，唯一键为 command id + intent digest。
2. PostgreSQL unit of work 原子提交 run revision、agent/frame、association、lineage/gate、node claim、outbox。
3. 下游 Runtime launch/input worker消费 outbox并回写 receipt。
4. generic/API create 也必须由 caller 提供 stable identity；删除随机 retry identity。
5. 每个 stage 注入 failure 的 restart/replay tests。

**预防 guard**

- production dispatch service只依赖一个 command transaction port，不直接依赖七个 repos。
- composition test 验证 outbox worker存在。
- DB invariant query：open gate/lineage/association 都能解析到同一 committed run-agent-frame。

#### CO-03 — P0：建立 durable Routine trigger occurrence 与 distributed claim

**证据与触发器**

- 每个 AppState 都创建并启动 cron scheduler（`crates/agentdash-api/src/app_state.rs:556-569`）。
- scheduler 读取 enabled scheduled routines，启动进程内 loop（`cron_scheduler.rs:70-113,115-128`）；每个 due entry 直接 `tokio::spawn executor.fire_scheduled`（`:264-312`）。
- 每次 fire 创建随机 `RoutineExecution::new` 后立即 insert（`routine/executor.rs:145-152`）。
- `routine_executions` 只有随机 id 主键和普通 routine/status indices，没有 `(routine_id, scheduled_at/provider_event_id)` 唯一 occurrence（`0001_init.sql:509-526,879-886,1017-1025`）。

触发器：两个 API 实例同时在线、旧 fire 尚未完成又进入下一 tick、重启跨越计划时刻、webhook provider 重试。

**爆炸半径**

同一计划时刻产生多个 run/agent/input handoff，导致重复 Agent 行为和下游 operation effect；`last_fired_at` 无法作为准确 occurrence receipt。

**目标边界**

Routine scheduler 只负责 materialize `RoutineTriggerOccurrence`：

`(routine_id, trigger_kind, occurrence_key, scheduled_at, payload_digest, status, lease, execution_id)`

DB unique constraint 决定 occurrence identity；worker `FOR UPDATE SKIP LOCKED` claim 后创建唯一 execution。Webhook 使用 provider event id 或显式 idempotency key。

**工作包**

1. migration 新增 occurrence 表/字段、unique key、lease/retry state。
2. cron loop只 upsert occurrence，不直接 fire。
3. worker claim occurrence并调用 executor；execution id从 occurrence稳定派生。
4. 将 `last_fired_at` 变成 projection，不参与正确性。
5. 多实例、tick overlap、crash before/after dispatch、webhook replay tests。

**预防 guard**

- schema gate 检查 occurrence unique constraint。
- production scheduler test 启两个 worker，对一个 occurrence只产生一个 execution/input receipt。

#### CO-04 — P0：装配 Interaction effect worker

**证据与触发器**

- `InteractionCommandService::execute` 在 admission 后构建 operation effect intent（`interaction/service.rs:139-268,403-440`）。
- PostgreSQL 在同一 row-lock transaction 中更新 state revision、插入 event/effect/command receipt（`interaction_repository.rs:403-449`），并实现 `FOR UPDATE SKIP LOCKED` claim lease（`:483-500`）。这是正确持久化边界。
- `InteractionEffectDispatcher` 只在 `crates/agentdash-application/src/interaction/effect_dispatcher.rs:38-108` 及同文件 tests 出现；全仓生产搜索未找到 AppState/background worker caller。AppState/bootstrap 只装配 interaction repositories/command operation access（`bootstrap/repositories.rs:83,176-180`；`bootstrap/runtime_gateway.rs:125-340`）。

触发器：任何 Interaction command definition 带 `operation_effect`，从 HTTP command 或 Operation Gateway invoke 提交。

**爆炸半径**

用户看到 state/event 已 committed，但被 contract 承诺的 effect 永远不发生；重启也不会推进 pending intent。

**目标边界**

composition root 必须装配一个 `InteractionEffectWorker`：claim durable intents → 用 pinned authority/capability revision 调 Operation Gateway → mark succeeded/retry/dead-letter。Operation Workshop 是人工 invoke host，不替代该 worker。

**工作包**

1. 给 dispatcher 注入 production OperationEffectExecutor adapter。
2. 在 post-AppState workers 启动可取消 loop，定义 batch/lease/backoff。
3. 暴露 pending/retrying/dead-letter projection/diagnostic。
4. command→effect success、operation temporary failure、process restart、lease expiry、duplicate claim E2E。

**预防 guard**

- composition test：只要 schema/command支持 operation_effect，生产 AppState 必须有 worker handle。
- reachability test 禁止 application public worker/dispatcher 只有 tests caller。

#### CO-05 — P0：修复 Companion human response 的 resolve-before-unconfigured-delivery

**证据与触发器**

- API route 构造 `CompanionGateControlService::with_agent_run_projection`，只传 repos，未注入 human response delivery（`routes/companion_gates.rs:45-54`）。
- `with_agent_run_projection` 等于 `new`（`gate_control.rs:344-346`），而 `new` 默认注入会返回 Internal 的 `NoopCompanionHumanResponseInputHandoffDelivery`（`:275-284,312-325`）。
- `respond` 先 `gate_resolver.respond_human` resolve gate，再调用 delivery（`:389-430`）。

触发器：用户对 open companion human gate 调 POST respond。

**爆炸半径**

gate 已不可逆关闭，API 返回 500，requesting Agent 没收到 input；重试在 `:360-364` 返回 conflict。当前没有 human response delivery marker/recovery worker可补偿该状态。

**目标边界**

API 不应 route-local 构造 service。AppState 只暴露一个 fully-composed `CompanionGateCommandPort`；gate resolution 与 durable input-handoff intent/receipt必须处于同一 command transaction，实际 Product input delivery由 worker或可恢复同步 protocol完成。

**工作包**

1. 移除生产可调用的默认未配置 constructor，deps要求 delivery/outbox port。
2. 新增 human response handoff intent/marker；resolve 与 intent 原子提交。
3. worker投递 stable `client_command_id = companion-human-response:{gate_id}`，回写 receipt。
4. 对现有 resolved-without-delivery 数据增加一次性 invariant 检测/修复 migration 或维护命令。
5. route-level E2E 验证 response → Agent input；failure/restart/retry验证不丢唤醒。

**预防 guard**

- production composition 禁止 `Noop*Delivery`。
- route tests必须使用 AppState真实 service，不得在 handler内 new。

#### CO-06 — P0：装配 Gate producer terminal convergence projector

**证据与触发器**

`GateProducerTerminalConvergenceService` 会扫描 producer 的 open wait policies、resolve 或确保 delivery（`application-workflow/src/gate/gate_wait_policy.rs:73-206`）；Product adapter 也能解析 runtime binding并执行 parent input wake（`application/src/gate_wait_policy.rs:20-123`）。但全仓 production 搜索只找到定义与 `CompanionGateControlService` 的 `#[cfg(test)] pub(crate)` helper（`gate_control.rs:690-703`），没有 terminal event producer、AppState service或后台 replay worker调用。

触发器：child Agent failed/cancelled/completed，但没有正常显式 companion result completion。

**爆炸半径**

父 Agent 等待的 gate 一直 open，waiter/parent continuation 永久悬挂，UI 只显示 child terminal 而协作流程不收敛。

**目标边界**

由唯一 Product terminal projector消费 durable Agent terminal fact，调用 convergence command；以 terminal event identity + gate id 幂等 resolve，并持久化 parent wake intent/receipt。启动时 replay open gate wait policies 与 terminal projection。

**工作包**

1. 定义 terminal projection → `GateProducerTerminalEvent` adapter 和 stable event id。
2. 在 terminal projection commit 后写 outbox，worker调用 convergence。
3. startup recovery 扫描 open wait policy，其 producer 已 terminal 时补偿。
4. normal result、failed、cancelled、duplicate terminal、restart before wake tests。

**预防 guard**

- terminal composition test要求至少一个 convergence consumer。
- DB invariant：producer terminal + open terminal-wait gate 超过 SLA 即失败/报警。

#### CO-07 — P0：Task mutation 的授权与 policy owner

这项与 `research/backend-business-assets.md` 的独立审计结论一致。

**证据与触发器**

- Project `Use` 对任意 role 或 `template_visible` 都成立，Configure 只对 Owner/Editor（`crates/agentdash-domain/src/project/authorization.rs:60-67,77-82`）。
- Task create/update/status/archive/agent-create 全用 `ProjectPermission::Use`（`routes/task_plan.rs:81-190`）。
- application 定义了 typed policy action，却只记录 “allowed by default” 后返回 `Ok(())`（`task/plan.rs:43-58,238-249`）。
- Story MCP create/batch要求 Configure，但直接调用相同 `create_run_task`（`agentdash-mcp/src/servers/story.rs:454-520`）；Runtime task_write先由 applied task grant/target scope围栏，再经 `TaskPlanWorkspace`调用相同 free functions（`task/tools.rs:556-617`；`task/workspace.rs:111-177`）。因此三个入口的外围授权不同，持久化并发语义却同样不安全。

触发器：Member 或仅 template visibility 获得 Use 的用户写 Task；或者 Runtime Agent 与用户同时改 Task。

**爆炸半径**

越权改变执行计划，并与 CO-01 叠加覆盖整个 run。

**目标边界**

建立 `TaskPlanCommandService`，actor context 必须区分 human project role 与 applied Agent resource grant；human write 要求明确 Configure 或专用 `TaskWrite` permission，Agent write 要求 target/owner/task grant。所有写使用 CO-01 typed CAS command。

**工作包与 guard**

- 先用产品权限矩阵测试固定 Owner/Editor/Member/template viewer/Agent 能力。
- route只调用 command service；删除 always-allow hook。
- MCP、Runtime task_write、Companion continuation也必须调用同一个 command port；Runtime applied grant作为 actor admission输入，不再由 workspace层自行重复解释 mutation policy。
- 此工作包的 persistence 部分依赖 CO-01；可先固定授权矩阵和command contract，但不能在 broad update上宣称完成。
- negative tests覆盖跨 run/agent、无 task grant、viewer mutation。

#### CO-08 — P1：Wait exec activity 的重启恢复

**证据与触发器**

`WaitActivityService` 查询 exec source时读取 `AgentRunTerminalRegistry`（`wait_activity/service.rs:201-255`）；registry 源码明确写着“纯内存，不持久化”，内部是 `RwLock<HashMap<...>>`（`agent_run/terminal_registry.rs:11-25`）。gate wait则读取 durable gate repo（`wait_activity/service.rs:258-281`）。

触发器：exec terminal创建/运行后 API 重启，随后 Agent 调 wait 或长 wait 跨重启。

**爆炸半径**

旧 terminal 被报告 not found/无法完成观测，Agent可能重复启动命令或失去退出码/output evidence。

**目标边界与工作包**

将 exec activity fact/terminal summary 持久化到 AgentRun-owned execution projection；process registry仅作 live cache。Wait source先查 durable projection，再叠加 live state。加入 create→restart→wait、running→restart→terminal reconcile tests。

**预防 guard**

所有可被 durable Agent tool引用的 activity ref必须有 persistent owner；wait test不得只用内存 fixture。

#### CO-09 — P1：Channel semantic identity 与 delivery ordering

**证据与触发器**

- Companion 每次通过 `ChannelService` + `UnsupportedChannelBindingResolver` 临时组合（`companion/tools.rs:213-221`）。
- `ensure_companion_agent_channel`/human channel 先 load/check，再创建随机 Channel 后 upsert（`:224-306`）；registry validation 主要按 channel id，未形成数据库级 semantic alias unique。
- 每次 handoff重新 `ChannelMessage::new` 和 `ChannelDeliveryIntent::new`（`:340-360`），真实 Agent input 使用稳定 client command id，但在成功后才 `record_delivery_state`（`:944-1003`）。
- child result另有 durable delivery marker（`:694-783`），parent request/response/human response主要依赖 Agent input dedup，Channel audit identity仍可重复。
- provider-neutral ingress service存在（`application/src/channel.rs:362-407`），但 production 搜索只有 Companion 组合 `ChannelService`；binding resolver仍 Unsupported。

触发器：并发首次建立同一 companion channel、Agent input成功后 channel state写失败并重试、provider ingress被误认为已可用。

**爆炸半径**

同一 participant topology产生多个 channel；实际 delivery dedup但 audit出现多个随机 message/delivery；外部 provider事件没有 production binding owner。

**目标边界与工作包**

1. owner store提供原子 `ensure_by_semantic_key(run, topology, participants, correlation)`，DB unique semantic key。
2. message/delivery id从 `(channel, source correlation/client_command_id, target)`稳定派生。
3. 先持久化 admitted message + delivery outbox，再执行 Product input，最后写 receipt。
4. provider ingress作为独立工作包，只有真实 binding resolver + worker装配后才宣称生产可达。
5. 并发 ensure、record failure、replay、multi-target tests。

**预防 guard**

Channel API不得接受 caller随机 identity作为重试 identity；reachability inventory区分 provider-neutral library 与 production connector。

#### CO-10 — P1：Routine admission、execution phase 与 CRUD owner 收敛

**证据与触发器**

- Routine CRUD/enable/token 在 route 直接 repo mutation + process Notify（`routes/routines.rs:77-304`）。
- executor 先创建 pending execution，再 template/config/admission；dispatch refs 之后另行 update，成功后 Routine `last_fired_at` 与 execution 又分别 update，错误只记录日志（`routine/executor.rs:145-285`）。
- recovery query只找 `status='pending' AND dispatch_run_id IS NOT NULL`（`routine_repository.rs:395-407`；index `0001_init.sql:1021`），因此 refs 写入前的 pending永不恢复。
- precheck只要 workspace 任一 backend online就允许（`routine/executor.rs:643-684`）；canonical resolver还会校验 Project allowed backend、workspace identity contract和 resolution policy（`workspace/resolution.rs:55-141`）。

触发器：execution create后 template/admission/dispatch前失败；Project backend grant收回但 binding在线；identity mismatch；Routine update与 scheduler reload之间进程失败。

**爆炸半径**

pending execution永久卡死；precheck与实际 frame construction给出不同结论；配置已改但进程内 schedule仍旧。

**目标边界与工作包**

以 CO-03 occurrence为根，Routine command service拥有 CRUD+durable config revision；execution使用 typed phase state machine（admitted/materialized/launching/input_pending/dispatched/terminal），所有非 terminal phase可恢复。Admission直接复用 canonical workspace resolution/Project backend access结果，不再复制在线判断。

**预防 guard**

- recovery query必须覆盖所有非 terminal phase。
- Routine admission与 composer/frame construction共享同一 resolver conformance tests。
- scheduler配置来自 durable revision，不以 Notify正确性为前提。

#### CO-11 — P1：Workflow HTTP/MCP 定义保存统一 command owner

**证据与触发器**

HTTP 保存 graph 使用 `ActivityLifecycleCatalogService::upsert_workflow_graph`（`routes/workflows.rs:180-186,245-251`；`application-workflow/src/catalog.rs:15-154`）。MCP 自己查 existing、`version + 1`、update/create，并复制 graph validation和 procedure reference检查（`agentdash-mcp/src/servers/workflow.rs:193-285`）。

触发器：HTTP 与 MCP 同时更新同 key；catalog规则、version/validation/安装 provenance发生变化。

**爆炸半径**

两个入口产生不同 validation、version或source metadata；并发 lost update。

**目标边界与工作包**

提供一个 `WorkflowDefinitionCommandService`，HTTP/MCP都传 actor/project、expected version、client command id、definition draft；service负责 validation、version、source/provenance和transaction。MCP只做 schema/transport mapping。

**预防 guard**

- HTTP/MCP contract conformance test对同 input产生相同 persisted definition/error。
- architecture test禁止 MCP server直接调用 definition repo create/update。

#### CO-12 — P1：清理或真正装配 Permission facade

**证据与触发器**

`AgentRunPermissionFacade` 定义了 Allowed/Denied/PendingApproval（`application-ports/src/agent_run_permission.rs:6-39`），但全仓 production 搜索没有实现或消费者。真实路径是：

- applied Product surface/tool/resource grant authorizer（`infrastructure/src/runtime_tool_authorization.rs:64-172`）；
- capability决定工具是否可见/可用；
- Agent Runtime内部持有 process-local pending approval并发事件（`agentdash-agent/src/agent.rs:97-113,615-632`）。

触发器：开发者按 permission spec 接入新工具，以为 facade 会统一授权/approval，实际生产不会调用。

**爆炸半径**

新增工具可能只满足 capability availability 而遗漏 resource/action authorization，或创建第二套 approval状态。

**目标边界与工作包**

先作明确决策：

- 若 dynamic Product permission 是需求，则在 capability + resource admission之后装配唯一 facade，PendingApproval使用 durable Interaction/Gate identity与resolution receipt；
- 若 Runtime approval就是最终 owner，则删除 dead facade并更新 spec，明确 Product只拥有 applied resource authorization。

**预防 guard**

公开 application port必须至少有 production implementation + composition + negative test；无 consumer 的 security port不能作为“已实现”边界。

#### CO-13 — P1：LifecycleRun create 的 transport idempotency

**证据与触发器**

`StartWorkflowRunRequest` 只有 `lifecycle_id/lifecycle_key/project_id`（`api/src/dto/workflow.rs:18-22`）。create/create-and-continue每次构造 command，run starter创建新的 `LifecycleRun`（`routes/workflows.rs:409-455`；`dispatch/run_orchestration_starter.rs:46-68`）。

触发器：客户端因 timeout 对成功但未收到响应的 POST 重试。

**爆炸半径**

同一意图创建多个 run/root orchestration，并可能启动重复 agent/effect。

**目标边界与工作包**

请求必须携带 `client_command_id`；command receipt以 actor/project/id + digest唯一，返回首次创建的 run。create-and-continue的 create与首轮 continue receipt纳入同一 command saga。

**预防 guard**

HTTP timeout/replay E2E；同 id不同 payload返回 idempotency conflict。

#### CO-14 — P1：Story/Relay MCP 绕过 Story application mutation owner

**证据与触发器**

- HTTP Story create/update调用 `create_story_record/update_story_record`（`crates/agentdash-api/src/routes/stories.rs:66-95,129-199`）。
- 该 application owner会 trim/validate Story context，写 Story 后同步 Story-owned inline files（`crates/agentdash-application/src/story/management.rs:53-95,202-210`）。
- Story MCP 的 context/details/status直接 load mutable Story后 `story_repo.update`（`crates/agentdash-mcp/src/servers/story.rs:324-446,548-577`）；Relay MCP create/status也直接 `story_repo.create/update`（`servers/relay.rs:200-217,312-341`）。

触发器：经 MCP 更新 context source/container/session composition、details，或创建 Story。

**爆炸半径**

MCP与HTTP对同一 mutation使用不同 validation/projection规则；MCP context更新后 Story inline VFS projection保持旧内容。两入口也没有 expected version，会互相覆盖 Story snapshot。

**目标边界与工作包**

建立具名 `StoryCommandService`，HTTP、Story MCP、Relay MCP共享 create/update/status command、expected revision与client command receipt；Story与inline-file projection通过 transaction/outbox收敛。MCP server只做 scope/auth/schema映射。Task create仍转交 CO-01/CO-07 的 Task command owner，不应塞回 Story aggregate。

**预防 guard**

- architecture test禁止 MCP server直接 `story_repo.create/update`。
- HTTP/MCP conformance test覆盖 context validation、inline VFS projection、并发 expected revision和idempotency conflict。

### 合理内聚与应保留的边界

1. **Workflow reducer/executor 是本域最成熟的正例。** reducer只解释 orchestration event，executor用 CAS提交并把 Function/HumanGate外部事实放进 durable effect protocol；后台 worker从 recoverable run恢复。这部分应成为 Lifecycle terminal、Task、Hook log、dispatch写入收敛时的模板，而不是被再包一层通用“workflow manager”。
2. **Interaction command transaction是正确聚合/事务边界。** instance row lock、expected revision、command digest receipt、event、effect intent在一个 transaction内（`interaction_repository.rs:403-449`）。问题只在 effect consumer没装配，不应拆散这项原子性。
3. **Hook policy source已正确收敛。** `ProductHookWorkflowProjection` 调 `resolve_active_workflow_projection_for_target`（`hook_workflow_projection.rs:35-86`），provider从 active contract构建 immutable effective contract（`application-hooks/src/provider.rs:245-261`）。需要修的是 execution log write path，而不是另建 Hook policy cache。
4. **Lifecycle views 与 Story runs 是合理 query projection。** Story route只把 Story映射为 `SubjectRef`并查询 `SubjectExecutionView`（`story_runs.rs:31-93`），没有再造第二个执行 aggregate。
5. **Operation Workshop 的 host/authority boundary总体合理。** route先校验 Project Use，再按 Project/Canvas/Interaction/ExtensionPanel context绑定 host和owner scope（`operation_workshop.rs:185-250`），最终由 Operation Gateway/provider admission决定可发现和可调用操作。它应保持“显式调用入口”，不能承担 Interaction durable effect worker。
6. **Agent Runtime 应继续仅作为下游边界。** Product command/input ports、binding/digest/resource grants决定 Product authority；Runtime线程、provider approval和实际执行是下游事实。修复上述问题不应把 Lifecycle/Task/Gate/Routine权威状态搬入 Runtime。

### 工作图与建议顺序

1. **先做 CO-01 LifecycleRun command store**：它是 CO-02 dispatch、CO-07 Task、Hook log、runtime terminal的共同写基础。
2. **并行做 production reachability 修复**：CO-04 Interaction worker、CO-05 human response delivery、CO-06 terminal convergence。三者不依赖新的业务语义，只需把已有 durable intent/adapter装配成真正生产链。
3. **做 CO-03 Routine occurrence**，随后 CO-10 phase/recovery/admission；否则在旧 scheduler上修 executor仍会重复触发。
4. **做 CO-02 dispatch transaction/saga**，把稳定 identities/outbox贯穿 API、Routine、Companion、Workflow Agent node。
5. **做 CO-07 authorization/policy、CO-08 Wait persistence、CO-09 Channel identity**。
6. **做 CO-11/12/13/14 的入口统一与架构清理**。

建议形成以下独立 implementation packages：

- `control-lifecycle-run-command-store`
- `control-lifecycle-dispatch-transaction`
- `control-routine-trigger-occurrence`
- `control-routine-phase-recovery-and-admission`
- `control-interaction-effect-worker`
- `control-companion-human-response-delivery`
- `control-gate-terminal-convergence-worker`
- `control-task-plan-authorization`
- `control-wait-durable-exec-activity`
- `control-channel-semantic-identity-outbox`
- `control-workflow-definition-command-service`
- `control-permission-boundary-decision`
- `control-lifecycle-create-idempotency`
- `control-story-command-service`

### 统一防回归门

1. **Production reachability manifest**：每个 public command port/dispatcher/worker列出 composition root、入口、authorization、receipt/recovery tests；禁止“只有 tests caller”的生产能力。
2. **Mutation owner architecture test**：
   - 禁止 production caller使用 `LifecycleRunRepository::update`；
   - 禁止 MCP/API route直接调用 definition repo create/update；
   - 禁止 MCP server直接调用 Story repo create/update；
   - 禁止 route-local构造带 Noop effect/delivery 的 service。
3. **Multi-instance recovery suite**：
   - 两 scheduler/worker实例；
   - crash before/after DB commit、before/after外部 effect；
   - lease expiry与duplicate terminal/command/event replay。
4. **Authorization matrix**：Owner/Editor/Member/template-visible/user/Agent applied grant 对 Workflow、Task、Gate、Agent control、Operation 的 read/write/action矩阵。
5. **Cross-entry conformance**：HTTP、MCP、Runtime tool、worker对同一 semantic command共享 command id/digest、validation、state owner与错误分类。
6. **DB invariant checks**：
   - terminal producer不得长期保留 open terminal-wait gate；
   - pending Interaction effect必须有活跃/可恢复 worker；
   - nonterminal Routine execution都可被 recovery query命中；
   - Lifecycle run/agent/frame/association/lineage/gate owner坐标一致。

### 测试与门禁现状

- **已有且值得保留**：
  - Workflow executor stale writer、CAS retry、stable claim、durable Function/HumanGate receipt/recovery tests（`executor_launcher.rs:1819-1913`及相邻 tests）。
  - Interaction command id/digest/effect identity tests，以及 PostgreSQL command transaction/effect claim实现。
  - Companion gate intent/convergence tests；这些测试证明 service在完整 deps下可工作。
  - Wait gate/exec source单元测试、Channel domain/service tests、Routine executor recovery tests。
- **关键缺口**：
  - Companion API handler使用真实 AppState composition 的 route-level test；现有 tests手动注入 delivery，未覆盖生产断链。
  - Interaction effect dispatcher production composition/E2E。
  - terminal projection → gate convergence生产 E2E。
  - LifecycleRun跨 writer并发 integration tests。
  - Routine多实例 occurrence唯一性与 refs 写入前 crash recovery。
  - Task role matrix与Agent applied grant matrix。
  - HTTP/Story MCP/Runtime/Companion continuation四入口同时改同一 run时的 CAS convergence。
  - Wait create→server restart→observe。
  - Workflow HTTP/MCP同义命令 conformance。
  - Story HTTP/MCP context mutation与inline-file projection conformance。
  - Channel并发 semantic ensure、Agent input成功后registry写失败的重放。

### 相关 specs

- `.trellis/spec/backend/architecture.md`
- `.trellis/spec/backend/repository-pattern.md`
- `.trellis/spec/backend/database-guidelines.md`
- `.trellis/spec/backend/domain-payload-typing.md`
- `.trellis/spec/backend/workflow/architecture.md`
- `.trellis/spec/backend/activity-lifecycle.md`
- `.trellis/spec/backend/lifecycle-run-link.md`
- `.trellis/spec/backend/lifecycle-edge.md`
- `.trellis/spec/backend/channel/architecture.md`
- `.trellis/spec/backend/interaction/architecture.md`
- `.trellis/spec/backend/permission/architecture.md`
- `.trellis/spec/backend/capability/architecture.md`
- `.trellis/spec/backend/hooks/architecture.md`
- `.trellis/spec/backend/session/architecture.md`
- `.trellis/spec/backend/runtime-gateway.md`

### 外部参考

无。本次结论全部来自仓库 production composition、application/domain/infrastructure代码、migration与项目 specs；没有借助外部版本或文档。

## Caveats / Not Found

1. **Interaction effect dispatcher：实现存在，生产装配未找到。** 搜索范围覆盖 API AppState/bootstrap/background workers、application、infrastructure；只找到 dispatcher本身和 tests。若有仓库外独立 worker binary，需要主会话补充其启动入口，否则应按 P0 production reachability gap处理。
2. **Gate producer terminal convergence：实现存在，生产 caller未找到。** 唯一 `CompanionGateControlService::observe_gate_producer_terminal` 还是 `#[cfg(test)]`；未找到 Agent terminal projector或startup replay worker。
3. **Channel provider ingress：provider-neutral service存在，但 production只找到 Companion内部 `ChannelService` 且 binding resolver为 Unsupported。** 因此本文件没有把外部 IM/webhook channel ingress当作已上线能力。
4. **Permission facade：未找到 implementation/composition/consumer。** Runtime自身另有 approval协议；最终应删除 dead facade还是把 approval提升为 Product durable Interaction，需要产品安全语义决策。
5. **Project Use 是否允许 fork/cancel/composer 等用户操作**可能是有意的协作模型；本文件没有把所有 Agent control route都定性为越权。Task mutation被列为 P0，是因为 `template_visible` 也能 Use、policy hook明确永远 allow，并已被独立业务资产审计交叉确认。
6. **Lifecycle dispatch 中部分入口已有稳定 Product identities**（尤其 Routine）；这降低重试损害但不提供跨 repo atomicity，也不覆盖 generic/API create路径。
7. **Workflow executor recovery worker已生产装配。** 不应把 Routine/Interaction/Gate worker缺失泛化成“所有后台恢复都缺失”。
8. **未审计 Agent Runtime内部 provider执行实现。** 按任务范围只核对 Product command/input/binding/resource grant到 Runtime的下游边界。
9. **未覆盖/转交项**：
   - Project/Backend/Workspace、Shared Library、Extension、Task业务资产的完整所有权审计在 `research/backend-business-assets.md`；本文件仅交叉确认 Task与控制编排交点。
   - Runtime relay、terminal source sequencing、Dash Complete、Tauri/worker composition 的完整审计应由执行/系统装配研究文件承担；本文件只记录 Wait内存 terminal registry和Product terminal→Gate缺失。
   - 前端对 lifecycle/task/gate/projection的重复解释不在本文件范围，由 `research/frontend-crosslayer-coupling.md` 承担。
10. **Story MCP direct mutation 已核验，不是遗漏。** 它绕过 HTTP 使用的 Story management owner，并遗漏 context validation/inline-file sync；完整 Story/inline-file资产生命周期仍应与 `backend-business-assets.md` 合并，本文件保留其作为 MCP控制入口和 Task边界的交点。
