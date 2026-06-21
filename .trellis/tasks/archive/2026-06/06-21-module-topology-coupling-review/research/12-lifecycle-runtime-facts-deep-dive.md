# Research: lifecycle runtime facts deep dive

- Query: 深挖 Lifecycle runtime facts：start/drain、status aggregation、SubjectExecutionView、Task execution surfaces、RuntimeSessionExecutionAnchor 的事实源与主链路耦合，输出可拆后续任务候选。
- Scope: internal
- Date: 2026-06-21

## 结论摘要

1. `start_lifecycle_run` 与 ready-node drain 仍混在同一个 public API 事实入口：application `LifecycleDispatchService::start_lifecycle_run` 只创建 `LifecycleRun + OrchestrationInstance`，但 API `POST /lifecycle-runs` 立即构造 `OrchestrationExecutorLauncher` 并调用 `drain_ready_nodes`。这和 spec 中 “start 只初始化 orchestration，entry node 保持 Ready” 的合同不一致，调用方无法稳定观察纯 Ready run。
2. status aggregation 的 owner 已比上一轮更清晰：run status 最终由 domain `aggregate_lifecycle_run_status` 经 `refresh_status_from_orchestrations` 写入；application reducer 只保留 orchestration status 的 `derive_orchestration_status`。剩余风险不是“两套 run status 聚合”，而是 orchestration status owner 在 application、run status owner 在 domain，跨层规则仍需要一个显式 contract 和 focused tests 覆盖 mixed terminal / blocked / cancelled。
3. `SubjectExecutionView` 是正式 subject execution 合同，API、前端 store、Task/Story execution panel 都已走 `/subjects/{kind}/{id}/execution`。但 application 内仍有 `TaskExecutionView` / `StoryActivityActivationService::get_task_execution_view`，且 `AppState` 仍注入该 service；Agent runtime `task_read execution` mode 又公开一个 execution stub。这形成三种表面：正式 SubjectExecutionView、内部 TaskExecutionView、工具 stub。
4. `RuntimeSessionExecutionAnchor` 是正确 backlink，但 latest selection 仍是高风险耦合点：Postgres `latest_for_agent` 只按 `updated_at DESC LIMIT 1`，workspace query 在 run 内也按 `updated_at` 取最大；subject cancel 先取 agent latest 再过滤 run。多 association、多 orchestration、append graph、replacement session 下，latest delivery、latest runtime node、cancel target 可能选择不同坐标或丢失 run 内较旧 anchor。
5. 前端没有发现新的独立 Task execution API 面；Task/Story panel 消费 `SubjectExecutionView.latest_runtime_node`。风险主要在后端 application 内部残留和工具 contract 文案，而不是 frontend store 分裂。

## 主链路拓扑

### 1. Lifecycle start / drain

```text
POST /lifecycle-runs
  -> LifecycleDispatchService::start_lifecycle_run
     -> resolve WorkflowGraph
     -> compile OrchestrationPlanSnapshot
     -> create LifecycleRun::new_control
     -> ensure_workflow_graph_orchestration
     -> run_repo.create(run)
     -> returns run_ref + orchestration_ref
  -> OrchestrationExecutorLauncher::drain_ready_nodes(run.id)
  -> reload latest run
  -> LifecycleRunView
```

Evidence:

- API route registers `POST /lifecycle-runs` to `start_lifecycle_run` (`crates/agentdash-api/src/routes/workflows.rs:120`).
- Route calls application `start_lifecycle_run` (`crates/agentdash-api/src/routes/workflows.rs:435`) and then immediately constructs `OrchestrationExecutorLauncher` (`crates/agentdash-api/src/routes/workflows.rs:452`) and calls `launcher.drain_ready_nodes(run.id)` (`crates/agentdash-api/src/routes/workflows.rs:457`).
- Application start creates a new run, ensures root orchestration, persists it, and returns refs (`crates/agentdash-application/src/lifecycle/dispatch_service.rs:332`, `crates/agentdash-application/src/lifecycle/dispatch_service.rs:341`, `crates/agentdash-application/src/lifecycle/dispatch_service.rs:342`, `crates/agentdash-application/src/lifecycle/dispatch_service.rs:348`, `crates/agentdash-application/src/lifecycle/dispatch_service.rs:350`).
- Start test asserts entry node is still `Ready`, no executor ref, and no agent created (`crates/agentdash-application/src/lifecycle/dispatch_service.rs:1860`, `crates/agentdash-application/src/lifecycle/dispatch_service.rs:1888`, `crates/agentdash-application/src/lifecycle/dispatch_service.rs:1889`, `crates/agentdash-application/src/lifecycle/dispatch_service.rs:1891`).
- Spec states `start_lifecycle_run` only initializes orchestration; entry remains `Ready` (`.trellis/spec/backend/workflow/architecture.md:336`, `.trellis/spec/backend/workflow/architecture.md:351`).

Interpretation:

- Service-level start boundary is correct.
- Public API still means “create + drain”, so the API is the coupling point.
- Human gate submit correctly drains after a human decision (`crates/agentdash-application/src/workflow/orchestration/executor_launcher.rs:253`), which is a different command shape and should stay separate from pure start.

### 2. Runtime status aggregation

```text
OrchestrationRuntimeEvent
  -> apply_orchestration_event(instance, event)
     -> mutate RuntimeNodeState
     -> derive_orchestration_status(instance)
  -> apply_orchestration_event_to_run
     -> run.refresh_status_from_orchestrations()
        -> aggregate_lifecycle_run_status(orchestrations)
```

Evidence:

- Reducer applies event to matching orchestration, then refreshes run status (`crates/agentdash-application/src/workflow/orchestration/runtime.rs:266`, `crates/agentdash-application/src/workflow/orchestration/runtime.rs:277`, `crates/agentdash-application/src/workflow/orchestration/runtime.rs:279`).
- Reducer derives `OrchestrationStatus` from node status (`crates/agentdash-application/src/workflow/orchestration/runtime.rs:432`, `crates/agentdash-application/src/workflow/orchestration/runtime.rs:953`).
- Domain `LifecycleRun::refresh_status_from_orchestrations` delegates to `aggregate_lifecycle_run_status` (`crates/agentdash-domain/src/workflow/entity.rs:428`, `crates/agentdash-domain/src/workflow/entity.rs:456`).
- Domain aggregation precedence is failed, blocked/paused, running/ready/claiming, pending, all-cancelled, completed-or-cancelled, ready fallback (`crates/agentdash-domain/src/workflow/entity.rs:462`, `crates/agentdash-domain/src/workflow/entity.rs:468`, `crates/agentdash-domain/src/workflow/entity.rs:477`, `crates/agentdash-domain/src/workflow/entity.rs:489`, `crates/agentdash-domain/src/workflow/entity.rs:495`, `crates/agentdash-domain/src/workflow/entity.rs:501`, `crates/agentdash-domain/src/workflow/entity.rs:509`).
- Domain tests cover completed+cancelled => completed, paused => blocked, all cancelled => cancelled (`crates/agentdash-domain/src/workflow/entity.rs:869`, `crates/agentdash-domain/src/workflow/entity.rs:880`, `crates/agentdash-domain/src/workflow/entity.rs:894`).

Interpretation:

- Run status owner is domain aggregate/helper.
- Orchestration status owner is application reducer.
- This split can be valid, but the contract should explicitly state that application owns per-orchestration derivation while domain owns run-level aggregation. The backlog should not repeat the older “two run status aggregators” wording.

### 3. SubjectExecutionView / Task execution surfaces

```text
GET /subjects/{kind}/{id}/execution
  -> run_view_builder::build_subject_execution_view
     -> association_repo.list_by_subject(subject)
     -> lifecycle_run_repo.list_by_ids(run_ids)
     -> latest_subject_runtime_projection
        -> association -> agent/current_frame -> anchors by agent
        -> anchor orchestration_id/node_path/attempt
        -> RuntimeNodeState + artifacts
  -> SubjectExecutionView
  -> frontend lifecycleStore / TaskSubjectExecutionPanel / StorySubjectExecutionPanel
```

Evidence:

- API route exposes `/subjects/{kind}/{id}/execution` (`crates/agentdash-api/src/routes/lifecycle_views.rs:44`) and handler calls `build_subject_execution_view` (`crates/agentdash-api/src/routes/lifecycle_views.rs:80`, `crates/agentdash-api/src/routes/lifecycle_views.rs:87`).
- Contract maps `SubjectExecutionView` directly (`crates/agentdash-api/src/routes/lifecycle_contracts.rs:46`), generated TS exposes `latest_runtime_node` and `artifacts` (`packages/app-web/src/generated/workflow-contracts.ts:253`).
- Frontend service and store consume that endpoint (`packages/app-web/src/services/lifecycle.ts:27`, `packages/app-web/src/stores/lifecycleStore.ts:208`, `packages/app-web/src/stores/lifecycleStore.ts:210`).
- Task and Story panels load subject execution and read `latest_runtime_node` from `SubjectExecutionView` (`packages/app-web/src/features/task/task-subject-execution-panel.tsx:39`, `packages/app-web/src/features/task/task-subject-execution-panel.tsx:123`, `packages/app-web/src/features/story/story-subject-execution-panel.tsx:37`, `packages/app-web/src/features/story/story-subject-execution-panel.tsx:137`).

Parallel/internal surfaces:

- `TaskExecutionView` still exists in application (`crates/agentdash-application/src/task/execution.rs:23`).
- `StoryActivityActivationService::get_task_execution_view` builds that view from task plan + subject association + anchors (`crates/agentdash-application/src/task/service.rs:14`, `crates/agentdash-application/src/task/service.rs:20`, `crates/agentdash-application/src/task/service.rs:47`).
- `AppState` still imports and stores `StoryActivityActivationService` (`crates/agentdash-api/src/app_state.rs:24`, `crates/agentdash-api/src/app_state.rs:87`, `crates/agentdash-api/src/app_state.rs:299`).
- No API route callsite for `get_task_execution_view` was found under `crates/agentdash-api/src/routes`; `rg` only found AppState injection and application definitions.
- `task_read` description claims `execution` mode exists (`crates/agentdash-application/src/task/tools.rs:416`), but `TaskReadMode::Execution` returns `execution_stub` with source note (`crates/agentdash-application/src/task/tools.rs:892`, `crates/agentdash-application/src/task/tools.rs:896`) and `execution_summary: null` (`crates/agentdash-application/src/task/tools.rs:994`, `crates/agentdash-application/src/task/tools.rs:1001`).
- The older boot projector is intentionally skipped and logs that runtime projection is read through `SubjectExecutionView` (`crates/agentdash-application/src/task/view_projector.rs:38`, `crates/agentdash-application/src/task/view_projector.rs:53`).

Interpretation:

- Public frontend/API read path is mostly converged to `SubjectExecutionView`.
- Internal service and runtime tool stub create residual ambiguity for future implementers and model/tool consumers.

### 4. RuntimeSessionExecutionAnchor latest selection

```text
RuntimeSessionExecutionAnchor
  -> fields: runtime_session_id, run_id, launch_frame_id, agent_id,
             optional orchestration_id, node_path, node_attempt

SubjectExecutionView latest:
  subject associations
  -> resolve association agent
  -> agent.current_frame_id
  -> list_by_agent(agent.id)
  -> filter anchor.run_id == association.run && launch_frame_id == current_frame
  -> anchor orchestration coordinate -> RuntimeNodeState
  -> pick max observed_at

AgentRun workspace latest delivery:
  list_by_run(run_id)
  -> filter agent_id
  -> max updated_at

Repository latest_for_agent:
  WHERE agent_id = $1 ORDER BY updated_at DESC LIMIT 1

Subject cancel latest:
  latest_for_agent(agent.id)
  -> filter run_id == association.anchor_run_id
```

Evidence:

- Anchor fields include optional orchestration/node coordinate (`crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:29`, `crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:35`, `crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:37`, `crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:39`).
- Orchestration dispatch constructor writes those fields (`crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:68`, `crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:73`, `crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:83`, `crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:84`, `crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:85`).
- Subject runtime projection uses association -> agent -> current frame -> anchors by agent, filters by run and frame, then projects from anchor coordinate (`crates/agentdash-application/src/lifecycle/run_view_builder.rs:332`, `crates/agentdash-application/src/lifecycle/run_view_builder.rs:349`, `crates/agentdash-application/src/lifecycle/run_view_builder.rs:351`, `crates/agentdash-application/src/lifecycle/run_view_builder.rs:391`, `crates/agentdash-application/src/lifecycle/run_view_builder.rs:397`, `crates/agentdash-application/src/lifecycle/run_view_builder.rs:398`).
- `SubjectExecutionView` chooses one projection by `observed_at` (`crates/agentdash-application/src/lifecycle/run_view_builder.rs:357`, `crates/agentdash-application/src/lifecycle/run_view_builder.rs:359`, `crates/agentdash-application/src/lifecycle/run_view_builder.rs:362`).
- Postgres `list_by_run`, `list_by_agent`, and `latest_for_agent` order by `updated_at DESC`; latest limits to one row (`crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:864`, `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:874`, `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:885`, `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:895`, `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:930`, `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:940`, `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:941`).
- Workspace query resolves delivery runtime by run+agent and `max_by_key(updated_at)` (`crates/agentdash-application/src/agent_run/workspace/query.rs:314`, `crates/agentdash-application/src/agent_run/workspace/query.rs:319`, `crates/agentdash-application/src/agent_run/workspace/query.rs:322`, `crates/agentdash-application/src/agent_run/workspace/query.rs:323`).
- Workspace frame/VFS resolution also uses run anchors filtered by agent and max `updated_at` (`crates/agentdash-application/src/agent_run/workspace/query.rs:332`, `crates/agentdash-application/src/agent_run/workspace/query.rs:338`, `crates/agentdash-application/src/agent_run/workspace/query.rs:339`).
- Subject cancel uses `latest_for_agent(agent.id)` and then filters by association run (`crates/agentdash-application/src/lifecycle/subject_execution_control.rs:123`, `crates/agentdash-application/src/lifecycle/subject_execution_control.rs:125`, `crates/agentdash-application/src/lifecycle/subject_execution_control.rs:127`); delivery selection policies also use `LaunchPrimary` = min created_at and `LatestAttached` = latest_for_agent (`crates/agentdash-application/src/lifecycle/subject_execution_control.rs:255`, `crates/agentdash-application/src/lifecycle/subject_execution_control.rs:260`, `crates/agentdash-application/src/lifecycle/subject_execution_control.rs:262`, `crates/agentdash-application/src/lifecycle/subject_execution_control.rs:264`).
- Mailbox delivery without explicit stream also uses `latest_for_agent` and then rejects if run/agent mismatch (`crates/agentdash-application/src/agent_run/mailbox.rs:1813`, `crates/agentdash-application/src/agent_run/mailbox.rs:1815`, `crates/agentdash-application/src/agent_run/mailbox.rs:1823`, `crates/agentdash-application/src/agent_run/mailbox.rs:1825`).

Interpretation:

- Anchor is the right backlink and contains enough coordinates for workflow nodes.
- The selection APIs encode multiple meanings of “latest”: latest attached delivery, latest node observation, latest current-frame runtime, latest run-scoped delivery, launch-primary. These meanings are currently spread across repository, subject projection, workspace query, mailbox, companion/cancel policy.
- Multi association / multi orchestration / append graph coverage is partial: projection can traverse many associations and orchestrations, but it collapses to one `latest_runtime_node`; workspace/cancel paths can discard valid run-specific anchors if the global agent latest belongs elsewhere.

## 耦合矩阵

| Coupling | From | To | Relationship | Evidence | Risk |
| --- | --- | --- | --- | --- | --- |
| Start API combines create and drain | `crates/agentdash-api/src/routes/workflows.rs` | `LifecycleDispatchService` + `OrchestrationExecutorLauncher` | 控制面耦合：一个 POST 同时创建 run 和调度 ready nodes | route start at `workflows.rs:409`, service start at `dispatch_service.rs:332`, drain at `workflows.rs:457` | P0 |
| Ready run contract vs public behavior | `.trellis/spec/backend/workflow/architecture.md` | API route | 契约耦合：spec/service/test 保持 Ready，但 public route 返回 drained view | `architecture.md:336`, `dispatch_service.rs:1888`, `workflows.rs:457` | P0 |
| Orchestration status derived in application, run status in domain | `workflow/orchestration/runtime.rs` | `workflow/entity.rs` | 分层事实源耦合：两层各拥有一段 status aggregation | `runtime.rs:953`, `entity.rs:456` | P1 |
| SubjectExecutionView vs TaskExecutionView | `lifecycle/run_view_builder.rs` | `task/service.rs` | 契约耦合：正式 subject view 与内部 narrow task view 并存 | `run_view_builder.rs:165`, `task/execution.rs:23`, `task/service.rs:20` | P1 |
| Task runtime tool execution stub | `task/tools.rs` | `SubjectExecutionView` | 表面耦合：工具声明 execution mode，但只返回 stub 和 note | `tools.rs:416`, `tools.rs:892`, `tools.rs:1001` | P1 |
| Anchor latest semantics spread | Postgres anchor repo | workspace / cancel / mailbox / subject projection | 运行态耦合：多个消费者各自解释 latest / primary / current-frame | `lifecycle_anchor_repository.rs:940`, `workspace/query.rs:323`, `subject_execution_control.rs:125`, `mailbox.rs:1815` | P0 |
| SubjectExecutionView collapses histories | `build_subject_execution_view` | frontend task/story panels | UI 状态耦合：多 association / node history 被投影成单一 latest node | `run_view_builder.rs:242`, `run_view_builder.rs:268`, `task-subject-execution-panel.tsx:39` | P1 |
| AppState keeps legacy task execution service | `app_state.rs` | `task/service.rs` | 装配耦合：未公开 route 仍进入 composition root | `app_state.rs:87`, `app_state.rs:299`, `task/service.rs:14` | P2 |

## P0 backlog candidates

### P0-1: 拆分 lifecycle start 与 ready-node drain public command

- Problem: `POST /lifecycle-runs` 现在执行 create + drain，阻断 Ready orchestration 可观察性，也让 “start” 带 scheduler side effect。
- Impact: workflow start API、frontend run creation flow、human gate / later drain command model、scheduler tests。
- Suggested owner module: backend workflow/lifecycle API。
- Acceptance direction:
  - `POST /lifecycle-runs` 只返回 created Ready `LifecycleRunView`。
  - 新增显式 `POST /lifecycle-runs/{id}/drain-ready-nodes` 或等价 continue command。
  - start API test 断言 entry node Ready、无 runtime session、无 agent/frame/anchor。
  - drain command test 断言 AgentCall materialization 后 node Running、anchor 与 executor ref 一致。

### P0-2: 定义 AnchorDeliverySelectionService，统一 latest / primary / current-frame / run-scoped semantics

- Problem: `latest_for_agent`、workspace query、subject projection、cancel、mailbox 各自选择 anchor；多 run/multi orchestration/append graph 可能取到不同 delivery coordinate。
- Impact: AgentRun workspace delivery refs、SubjectExecutionView latest node、subject cancel、mailbox delivery、companion/human gate delivery selection。
- Suggested owner module: backend lifecycle/session boundary。
- Acceptance direction:
  - 提供显式 selection input：run_id、agent_id、frame_id policy、orchestration binding policy、delivery policy。
  - 不再用全局 `latest_for_agent` 后过滤 run 作为 subject cancel target。
  - 对同一 agent 多 run、多 frame、多 anchor 的 fixtures，workspace/cancel/mailbox/subject projection 选择同一预期 coordinate。
  - Repository `latest_for_agent` 要么降级为低层 helper，要么改名表达只是 raw latest anchor。

## P1 backlog candidates

### P1-1: 固化 runtime status aggregation owner contract 与 focused tests

- Problem: run-level aggregation owner 已收敛到 domain，但 orchestration-level derivation 在 application；边界合理性缺少明确测试矩阵。
- Impact: mixed terminal / cancelled / paused / blocked / skipped / append orchestration 状态展示和 active run 判断。
- Suggested owner module: backend workflow runtime/domain。
- Acceptance direction:
  - spec 明确 application owns `OrchestrationStatus` derivation, domain owns `LifecycleRunStatus` aggregation。
  - reducer tests 覆盖 failed > blocked > running > ready > cancelled/completed precedence。
  - append orchestration + completed root + running child graph 的 run status fixture。
  - 所有 run write paths 调用 `refresh_status_from_orchestrations` 或 aggregate helper。

### P1-2: 收敛 Task execution surfaces 到 SubjectExecutionView

- Problem: `SubjectExecutionView` 是正式合同，但 `TaskExecutionView` service 仍存在并注入 AppState，`task_read execution` 又提供 stub。
- Impact: 后续实现容易把 Task execution status / runtime refs 写入 narrow DTO 或工具 stub，绕开 subject projection。
- Suggested owner module: backend task/lifecycle read model + runtime tools。
- Acceptance direction:
  - 删除或私有化 `StoryActivityActivationService::get_task_execution_view`；若仍需要内部 helper，返回/复用 `SubjectExecutionView`。
  - AppState 移除未使用的 `story_activity_activation_service`，除非有明确 route/use case。
  - `task_read execution` 要么调用 subject execution projector 并返回真实 linked run/latest node/artifact summary，要么从 schema/description 中移除 execution mode。
  - Contract check 确认 Task plan DTO 仍不携带 runtime execution facts。

### P1-3: 扩展 SubjectExecutionView 以表达 execution history，而不只返回 latest node

- Problem: 当前 projection 可遍历多 association 和多 anchor，但最终只输出 `latest_runtime_node` 与 artifacts；append graph / retries / review graph / multi-agent task history 无法表达。
- Impact: Task/Story execution panel、debug trace drill-down、artifact history、multi orchestration run view。
- Suggested owner module: backend lifecycle read model + frontend lifecycle store。
- Acceptance direction:
  - 增加 `runtime_attempts` / `runtime_nodes` / `linked_executions` 列表，包含 run_id、agent_id、frame_id、orchestration_id、node_path、attempt、runtime_session_ref、observed_at。
  - 保留 `latest_runtime_node` 作为 convenience field，但从同一列表派生。
  - 前端 Task/Story panels 显示 latest，同时可下钻 history。
  - Tests 覆盖同一 Task 两个 association、一个 append orchestration、一次 retry attempt。

## P2 backlog candidates

### P2-1: 将 raw anchor repository API 与 application selection API 分层命名

- Problem: `latest_for_agent` 名称看似业务语义，但实际只是按 `updated_at DESC LIMIT 1`。
- Impact: 新调用方会误用 repository latest 作为 run-scoped、frame-scoped 或 active delivery。
- Suggested owner module: backend repository/application boundary。
- Acceptance direction:
  - Repository 方法命名表达 raw order，例如 `latest_updated_anchor_for_agent`。
  - Application 层提供 intentful selection methods。
  - 编译期替换所有 `latest_for_agent` direct callsite，调用方必须选择 policy。

### P2-2: 清理 AppState 中未公开消费的 StoryActivityActivationService

- Problem: service 注入 composition root，但 API route 未找到直接使用；它让 legacy Task execution surface 看起来仍是产品入口。
- Impact: 降低 AppState 装配噪声，避免 future route 直接复活 narrow DTO。
- Suggested owner module: backend API composition。
- Acceptance direction:
  - 删除 AppState 字段，或写明并测试唯一消费路径。
  - `rg get_task_execution_view` 只剩测试或被删除。

### P2-3: 前端 SubjectExecution history UI 跟进

- Problem: 前端当前只消费 latest runtime node，符合现有合同但无法表达多执行历史。
- Impact: Task drawer、Story execution panel、Lifecycle subject page。
- Suggested owner module: frontend lifecycle/task/story。
- Acceptance direction:
  - 后端提供 history 后，lifecycleStore 归一化 runtime execution records。
  - Task/Story panel 显示 latest + history list。
  - 不从 Task plan store 保存 runtime execution facts。

## 不重复项

- 不重复讨论 `RuntimeSessionExecutionAnchor` 是否应该存在；spec 和上一轮结论都确认它是正确 trace-to-control-plane backlink，当前问题是 selection semantics 和一致消费。
- 不重复上一轮 “cancel 直接修改 RuntimeNodeState 绕过 reducer” 的旧实现结论；当前 research 只引用 subject cancel 的 anchor selection 风险。第一轮报告已确认 cancel 现在走 reducer，后续若复查应聚焦 terminal writer 是否全部走 reducer。
- 不重复旧 Task boot projection 的 agent-scoped association 漏投和 absence -> Failed fallback；当前 `task/view_projector.rs` 已跳过 boot projection并说明 runtime state 由 `SubjectExecutionView` 读取。
- 不重复宽泛评审 `LifecycleDispatchService` 过宽或 PlanNode -> ActivityDefinition fake adapter；本文件只保留与 start/drain、runtime facts 和 read model 表面直接相关的耦合。
- 不重复 AgentRun mailbox / RuntimeSession control / Permission / VFS / Local / Extension 的既有 overdesign 内容；这些已在 `.trellis/tasks/06-14-module-overdesign-review/overdesign-review.md` 和第一轮模块报告中作为边界或独立队列。
- 不把 graphless lifecycle run 当作问题；plain topology 是普通 Agent runtime 的正常形态。

## Files Found

- `.trellis/tasks/06-21-module-topology-coupling-review/prd.md` - 当前 review 编排任务的目标、范围和验收标准。
- `.trellis/tasks/06-21-module-topology-coupling-review/design.md` - 本轮 review 的分轮策略、slice 输出 schema 和耦合分类。
- `.trellis/tasks/06-21-module-topology-coupling-review/research/02-workflow-lifecycle-task-topology.md` - 第一轮 Workflow/Lifecycle/Task 拓扑，提供本 deep-dive 的四个候选问题基线。
- `.trellis/tasks/06-21-module-topology-coupling-review/research/03-session-agentrun-runtime-topology.md` - 第一轮 Session/AgentRun/Runtime 拓扑，提供 anchor/latest delivery 与 workspace/cancel 相邻边界。
- `.trellis/spec/backend/workflow/architecture.md` - Workflow runtime facts、orchestration、start contract、RuntimeSessionExecutionAnchor 的权威 spec。
- `.trellis/spec/backend/story-task-runtime.md` - Story/Task/SubjectExecutionView/Task facts 与 runtime projection 边界。
- `.trellis/spec/frontend/workflow-activity-lifecycle.md` - 前端 LifecycleRunView/SubjectExecutionView/runtime node coordinate store contract。
- `.trellis/tasks/06-14-module-overdesign-review/overdesign-review.md` - 既有 overdesign review，用于标注不重复项和已变化的旧结论。
- `crates/agentdash-api/src/routes/workflows.rs` - Workflow/lifecycle public API，包含 start route 和 immediate drain。
- `crates/agentdash-api/src/routes/lifecycle_views.rs` - Lifecycle/subject execution read API。
- `crates/agentdash-api/src/routes/lifecycle_contracts.rs` - application read model 到 generated contract mapper。
- `crates/agentdash-api/src/app_state.rs` - composition root，仍注入 `StoryActivityActivationService`。
- `crates/agentdash-domain/src/workflow/entity.rs` - `LifecycleRun` aggregate 与 run status aggregation。
- `crates/agentdash-domain/src/workflow/repository.rs` - `RuntimeSessionExecutionAnchorRepository` trait。
- `crates/agentdash-domain/src/workflow/runtime_session_anchor.rs` - anchor value object 与 orchestration dispatch constructor。
- `crates/agentdash-application/src/lifecycle/dispatch_service.rs` - lifecycle start、graph dispatch、agent/frame/session/anchor materialization。
- `crates/agentdash-application/src/lifecycle/run_view_builder.rs` - `LifecycleRunView` 与 `SubjectExecutionView` owner。
- `crates/agentdash-application/src/lifecycle/subject_execution_control.rs` - subject cancel target and runtime delivery selection policy。
- `crates/agentdash-application/src/workflow/orchestration/runtime.rs` - orchestration runtime reducer and status derivation。
- `crates/agentdash-application/src/workflow/orchestration/executor_launcher.rs` - ready-node drain and human gate follow-up drain。
- `crates/agentdash-application/src/agent_run/workspace/query.rs` - AgentRun workspace delivery runtime selection。
- `crates/agentdash-application/src/agent_run/mailbox.rs` - mailbox delivery target resolution via anchor latest。
- `crates/agentdash-application/src/task/execution.rs` - narrow `TaskExecutionView` DTO。
- `crates/agentdash-application/src/task/service.rs` - `StoryActivityActivationService::get_task_execution_view`。
- `crates/agentdash-application/src/task/runtime_coordinate.rs` - Task runtime coordinate helper using anchor orchestration node fields。
- `crates/agentdash-application/src/task/tools.rs` - `task_read` / `task_write` tool surface and execution stub。
- `crates/agentdash-application/src/task/view_projector.rs` - skipped boot projection marker pointing to `SubjectExecutionView`。
- `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs` - Postgres anchor repository and raw latest ordering。
- `packages/app-web/src/services/lifecycle.ts` - frontend lifecycle service endpoints。
- `packages/app-web/src/stores/lifecycleStore.ts` - frontend lifecycle runtime projection store。
- `packages/app-web/src/features/task/task-subject-execution-panel.tsx` - Task execution UI consuming `SubjectExecutionView`。
- `packages/app-web/src/features/story/story-subject-execution-panel.tsx` - Story execution UI consuming `SubjectExecutionView`。
- `packages/app-web/src/generated/workflow-contracts.ts` - generated `SubjectExecutionView` contract.

## Code Patterns

- Service-level start is side-effect-light: compile graph, create run, ensure orchestration, persist run, return refs (`crates/agentdash-application/src/lifecycle/dispatch_service.rs:332`).
- API-level start is side-effectful: after service start, it drains ready nodes and returns latest run view (`crates/agentdash-api/src/routes/workflows.rs:452`, `crates/agentdash-api/src/routes/workflows.rs:457`).
- Reducer pattern: runtime event mutates orchestration, derives orchestration status, then refreshes run status from domain aggregate (`crates/agentdash-application/src/workflow/orchestration/runtime.rs:432`, `crates/agentdash-application/src/workflow/orchestration/runtime.rs:279`).
- Subject execution projection pattern: `SubjectRef -> associations -> runs -> agent/current_frame -> anchors -> orchestration node -> artifacts` (`crates/agentdash-application/src/lifecycle/run_view_builder.rs:228`, `crates/agentdash-application/src/lifecycle/run_view_builder.rs:332`).
- Anchor raw latest pattern: repository and workspace use `updated_at` recency, not explicit runtime delivery state (`crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:940`, `crates/agentdash-application/src/agent_run/workspace/query.rs:323`).
- Task tool stub pattern: `task_read execution` returns plan facts plus null execution summary while telling callers to use `SubjectExecutionView` (`crates/agentdash-application/src/task/tools.rs:892`, `crates/agentdash-application/src/task/tools.rs:1001`).

## External References

- None. 本研究只使用仓库内 task artifacts、spec 和源码；未联网。

## Related Specs

- `.trellis/spec/backend/workflow/architecture.md`
- `.trellis/spec/backend/story-task-runtime.md`
- `.trellis/spec/frontend/workflow-activity-lifecycle.md`
- `.trellis/spec/project-overview.md`
- `.trellis/spec/backend/workflow/activity-lifecycle.md`
- `.trellis/spec/backend/workflow/lifecycle-run-link.md`
- `.trellis/spec/backend/session/architecture.md`
- `.trellis/spec/backend/session/runtime-execution-state.md`
- `.trellis/spec/frontend/state-management.md`
- `.trellis/spec/frontend/architecture.md`

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` 返回 `Current task: (none)` / `Source: none`；本文件按用户显式提供的 task path 和 output file 写入。
- 未修改业务代码，未运行测试，未执行 git 操作。
- 未找到 `TaskExecutionView` 的 API route 直接消费；只找到 `AppState` 注入和 application service 定义。若有动态注册或非 route 消费，需要单独调用图验证。
- 未验证数据库运行时数据分布；anchor latest 风险基于源码排序和调用链静态分析。
- 未展开 companion/human gate 全链路，只引用其和 anchor delivery selection 相同的 policy pattern。
