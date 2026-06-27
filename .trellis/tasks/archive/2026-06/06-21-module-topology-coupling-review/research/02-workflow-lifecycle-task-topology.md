# Research: workflow-lifecycle-task topology

- Query: 盘查 Workflow / Lifecycle / Task / Story execution 主链路拓扑与耦合点，产出后续 architecture review 问题清单。
- Scope: mixed
- Date: 2026-06-21

## Findings

### 模块/子模块清单与一句话职责

- `.trellis/tasks/06-21-module-topology-coupling-review/prd.md`：当前仍是占位 PRD；本文件按用户给定范围补齐首轮研究事实。
- `.trellis/spec/project-overview.md`：定义 Project / SubjectRef / LifecycleRun / WorkflowGraph / LifecycleAgent / AgentFrame / RuntimeSession 的总览边界；关键约束是 RuntimeSession 只承载 trace，不拥有 business ownership / permission scope / lifecycle progress truth。
- `.trellis/spec/backend/workflow/architecture.md`：Workflow backend 的事实源契约；`WorkflowGraph` 是 definition input，`LifecycleRun.orchestrations[]` 持有 runtime orchestration，runtime node key 是 `orchestration_id + node_path + attempt`。
- `.trellis/spec/backend/workflow/activity-lifecycle.md`：Activity lifecycle definition 与 runtime reducer 契约；executor 必须提交 `NodeStarted` 和 terminal event。
- `.trellis/spec/backend/workflow/lifecycle-edge.md`：edge 只有 `flow` / `artifact` 两类，artifact edge 隐含 node-level flow dependency。
- `.trellis/spec/backend/story-task-runtime.md`：Story / Task / SubjectContextAssignment / LifecycleSubjectAssociation / RuntimeSession 的职责边界；Task facts 在 `LifecycleRun.tasks`，execution 读 `SubjectExecutionView`。
- `.trellis/spec/frontend/workflow-activity-lifecycle.md`：前端 runtime view 以 `LifecycleRunView.orchestrations[]`、runtime node coordinate、subject execution、agent/frame 为主，不以 session id 或 graph id 作为 lifecycle 主索引。
- `.trellis/tasks/06-14-module-overdesign-review/overdesign-review.md`：上一轮已覆盖 overdesign 结论，尤其是 Lifecycle cancel、Task projection、status aggregation、RuntimeSession control、AgentRun workspace、Permission/VFS/Local 等重复事实源问题。
- `crates/agentdash-domain/src/workflow/entity.rs`：定义 `WorkflowGraph` definition、`LifecycleRunTopology`、`LifecycleRun` aggregate，以及 `orchestrations` / `tasks` / `view_projection` 字段。
- `crates/agentdash-domain/src/workflow/value_objects/orchestration.rs`：定义 `OrchestrationInstance`、`OrchestrationPlanSnapshot`、`RuntimeNodeState`、`RuntimeTraceRef` 等 runtime node 事实。
- `crates/agentdash-domain/src/workflow/value_objects/task_plan.rs`：定义 Task plan facts，包括六态 `TaskPlanStatus` 和 `LifecycleTaskPlanItem`。
- `crates/agentdash-domain/src/workflow/lifecycle_subject_association.rs`：定义 `SubjectRef` 与 whole-run / agent-scoped `LifecycleSubjectAssociation`，runtime node 不作为 subject anchor。
- `crates/agentdash-domain/src/workflow/runtime_session_anchor.rs`：定义 `RuntimeSessionExecutionAnchor`，作为 RuntimeSession 到 run / agent / frame / optional orchestration node 的 launch evidence。
- `crates/agentdash-domain/src/workflow/lifecycle_agent.rs`：定义 run-scoped `LifecycleAgent`，通过 `current_frame_id` 指向当前 runtime surface。
- `crates/agentdash-domain/src/workflow/agent_frame.rs`：定义 `AgentFrame` revision，承载 capability/context/VFS/MCP/execution profile 等 effective runtime surface。
- `crates/agentdash-domain/src/story/entity.rs`：Story 只保存用户价值单元与上下文；Task 计划事实由 `LifecycleRun.tasks` 承担。
- `crates/agentdash-application/src/workflow/definition.rs` / `catalog.rs` / `graph_resolver.rs`：WorkflowGraph definition 创建、catalog 管理与 graph ref 解析。
- `crates/agentdash-application/src/workflow/orchestration/compiler.rs`：把 `WorkflowGraph` 编译为 `OrchestrationPlanSnapshot`，并生成 PlanNode、activation rules 与 state exchange rules。
- `crates/agentdash-application/src/workflow/orchestration/runtime.rs`：common orchestration reducer，推进 `OrchestrationInstance` 中的 runtime node state。
- `crates/agentdash-application/src/workflow/orchestration/executor_launcher.rs`：消费 ready nodes，按 PlanNode kind 启动 Agent / Function / HumanGate，并通过 reducer 写回 run。
- `crates/agentdash-application/src/workflow/orchestration/agent_node_launcher.rs`：ready AgentCall node 的 materialization 入口，委托 `LifecycleDispatchService` 创建 agent/frame/session/anchor 后返回 `NodeStarted` event。
- `crates/agentdash-application/src/workflow/script/*`：workflow script builder/preflight/compile 入口，作为 WorkflowGraph 之外的 plan snapshot 来源。
- `crates/agentdash-application/src/lifecycle/dispatch_service.rs`：业务执行进入 control plane 的统一入口，同时创建/复用 run、orchestration、subject association、agent、frame、runtime session、anchor。
- `crates/agentdash-application/src/lifecycle/run_view_builder.rs`：application-owned `LifecycleRunView` / `SubjectExecutionView` read model 构建器。
- `crates/agentdash-application/src/lifecycle/subject_execution_control.rs`：subject-oriented cancel control，先解析 subject association / agent / frame / anchor，再按 orchestration binding 取消 runtime node。
- `crates/agentdash-application/src/lifecycle/projection.rs`：从 frozen `PlanNode` 投影 activation/frame composition 所需 activity shape。
- `crates/agentdash-application/src/task/plan.rs`：Run-scoped Task CRUD 与 Story Task projection。
- `crates/agentdash-application/src/task/fanout.rs`：从 root run 或 Story projection 选择 Task，并通过 `SubjectExecutionIntent` 扇出为 Task subject execution。
- `crates/agentdash-application/src/task/service.rs` / `runtime_coordinate.rs`：Task execution 专用只读投影，沿 association -> agent -> frame -> anchor -> runtime node 解析。
- `crates/agentdash-application/src/task/tools.rs`：Agent-facing `task_read` / `task_write`；execution mode 当前返回 stub，并声明 runtime execution 由 `SubjectExecutionView / linked run projection` 读取。
- `crates/agentdash-api/src/routes/workflows.rs`：WorkflowGraph CRUD / validate / lifecycle run start / human decision / tool catalog 等 API 边界。
- `crates/agentdash-api/src/routes/lifecycle_views.rs`：`/lifecycle-runs/{id}/view`、`/subjects/{kind}/{id}/execution`、agent frame runtime、session trace、project active agents read APIs。
- `crates/agentdash-api/src/routes/task_plan.rs`：run-scoped 与 agent-run-scoped Task plan API。
- `crates/agentdash-api/src/routes/stories.rs` / `story_runs.rs`：Story Task projection 与 Story subject execution read APIs。
- `packages/app-web/src/services/workflow.ts` / `stores/workflowStore.ts`：前端 WorkflowGraph definition/editor 入口。
- `packages/app-web/src/services/lifecycle.ts` / `stores/lifecycleStore.ts`：前端 LifecycleRun / SubjectExecution / AgentFrame / RuntimeTrace runtime projection 入口。

### 主链路拓扑

1. Definition / graph
   - `WorkflowGraph` 持有 `entry_activity_key`、`activities`、`transitions`，仍是静态 definition，不携带 runtime state（`crates/agentdash-domain/src/workflow/entity.rs:76`）。
   - `WorkflowGraph::new` 在创建时执行 `validate_workflow_graph`，definition-level 校验先于 runtime（`crates/agentdash-domain/src/workflow/entity.rs:105`）。
   - Application compiler 入口是 `WorkflowGraphCompiler::compile` / `compile_workflow_graph`（`crates/agentdash-application/src/workflow/orchestration/compiler.rs:120`、`crates/agentdash-application/src/workflow/orchestration/compiler.rs:128`）。
   - compiler 先校验 graph，再把 activities 映射成 `PlanNode`，把 Agent / Function / BashExec / Human executor 映射到 `PlanNodeKind + ExecutorSpec`（`crates/agentdash-application/src/workflow/orchestration/compiler.rs:152`、`crates/agentdash-application/src/workflow/orchestration/compiler.rs:285`、`crates/agentdash-application/src/workflow/orchestration/compiler.rs:320`）。
   - artifact binding 在 compiler 阶段转为 `StateExchangeRule`，并校验 source output / target input port 引用（`crates/agentdash-application/src/workflow/orchestration/compiler.rs:540`）。

2. Run / orchestration
   - `LifecycleRun` aggregate 同时保存 `topology`、`context`、`orchestrations`、`tasks`、`view_projection`，其中 runtime orchestration 在 `orchestrations`，Task facts 在 `tasks`（`crates/agentdash-domain/src/workflow/entity.rs:158`）。
   - `OrchestrationInstance` 持有 `orchestration_id`、`source_ref`、`plan_snapshot`、`activation`、`node_tree`、`dispatch`、`state_snapshot`（`crates/agentdash-domain/src/workflow/value_objects/orchestration.rs:48`）。
   - `RuntimeNodeState` 持有 node path、kind、status、attempt、inputs/outputs、executor ref、trace refs、timestamps 与 error（`crates/agentdash-domain/src/workflow/value_objects/orchestration.rs:353`）。
   - `LifecycleDispatchService::start_lifecycle_run` resolve graph -> compile plan -> create run -> ensure root orchestration -> persist run，返回 run/orchestration ref（`crates/agentdash-application/src/lifecycle/dispatch_service.rs:332`）。
   - graph-backed `dispatch_common` resolve graph -> compile plan -> resolve/create run -> ensure orchestration -> persist run，再进入 agent/frame/session/anchor materialization（`crates/agentdash-application/src/lifecycle/dispatch_service.rs:436`）。
   - `ensure_workflow_graph_orchestration` 以 `role + plan_digest` 复用已有 orchestration，否则创建 `OrchestrationSourceRef::WorkflowGraph` 并 `activate_orchestration`（`crates/agentdash-application/src/lifecycle/dispatch_service.rs:893`）。

3. Orchestration runtime reducer / scheduler
   - runtime 状态推进入口是 `apply_orchestration_event_to_run`，它定位 `orchestration_id`，调用 reducer，然后刷新 run status / updated_at / last_activity_at（`crates/agentdash-application/src/workflow/orchestration/runtime.rs:266`）。
   - `NodeStarted` 将 runtime node 置为 Running，写 executor ref 与 runtime trace ref，并从 ready queue 移除（`crates/agentdash-application/src/workflow/orchestration/runtime.rs:302`）。
   - application 仍有一份 `derive_orchestration_status`，根据 node statuses 写 orchestration status（`crates/agentdash-application/src/workflow/orchestration/runtime.rs:953`）；domain 也有 `aggregate_lifecycle_run_status`（`crates/agentdash-domain/src/workflow/entity.rs:456`）。
   - `OrchestrationExecutorLauncher::drain_ready_nodes` 每轮重新加载 run，取 next ready node，按 `PlanNodeKind` 启动对应 executor（`crates/agentdash-application/src/workflow/orchestration/executor_launcher.rs:161`）。
   - launcher 的 `apply_event` 统一调用 `apply_orchestration_event_to_run` 并 update run repo（`crates/agentdash-application/src/workflow/orchestration/executor_launcher.rs:402`）。

4. Agent / frame / session anchor
   - `LifecycleDispatchService` 构造时持有 run、workflow_graph、agent、frame、association、gate、lineage、anchor、runtime session creator 等 repo/ports（`crates/agentdash-application/src/lifecycle/dispatch_service.rs:103`）。
   - graph-backed dispatch 创建 subject association、runtime session、initial frame、current frame，再写 `RuntimeSessionExecutionAnchor::new_orchestration_dispatch`（`crates/agentdash-application/src/lifecycle/dispatch_service.rs:462`、`crates/agentdash-application/src/lifecycle/dispatch_service.rs:470`、`crates/agentdash-application/src/lifecycle/dispatch_service.rs:473`、`crates/agentdash-application/src/lifecycle/dispatch_service.rs:497`）。
   - graph-backed dispatch 随后提交 `OrchestrationRuntimeEvent::NodeStarted`，executor ref 指向 runtime session（`crates/agentdash-application/src/lifecycle/dispatch_service.rs:515`）。
   - ready AgentCall node 由 `AgentNodeLauncher` 委托 dispatch service materialize workflow agent node，再返回 `NodeStarted` event 给 launcher 写 reducer（`crates/agentdash-application/src/workflow/orchestration/agent_node_launcher.rs:157`、`crates/agentdash-application/src/workflow/orchestration/agent_node_launcher.rs:192`）。
   - `RuntimeSessionExecutionAnchor` 是 launch evidence，字段包括 runtime_session_id、run_id、launch_frame_id、agent_id、optional orchestration_id/node_path/node_attempt（`crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:20`、`crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:29`）。
   - `LifecycleAgent` 通过 `current_frame_id` 指向当前 frame（`crates/agentdash-domain/src/workflow/lifecycle_agent.rs:81`），`AgentFrame` revision 携带 effective capability/context/VFS/MCP/execution profile（`crates/agentdash-domain/src/workflow/agent_frame.rs:10`）。

5. Subject execution projection
   - Task / Story subject association 对 task/story 默认创建为 agent-scoped association，其他 subject 创建 run-scoped association（`crates/agentdash-application/src/lifecycle/dispatch_service.rs:678`）。
   - `LifecycleSubjectAssociation` 明确 anchor 只能是 run 或 LifecycleAgent；runtime node 证据来自 anchor（`crates/agentdash-domain/src/workflow/lifecycle_subject_association.rs:5`）。
   - `build_subject_execution_view` 从 `SubjectRef` 查 `list_by_subject`，加载 runs，再调用 latest runtime projection，并组装 run views / current agent / latest node / artifacts（`crates/agentdash-application/src/lifecycle/run_view_builder.rs:228`）。
   - latest runtime projection 沿 association -> agent -> current frame -> anchors by agent -> anchor coordinate -> run.orchestrations node lookup（`crates/agentdash-application/src/lifecycle/run_view_builder.rs:332`、`crates/agentdash-application/src/lifecycle/run_view_builder.rs:387`）。
   - `LifecycleRunView` 从 run 的 `orchestrations` 派生 orchestration views、runtime node tree、active runtime node refs，并从 anchor repo 收集 runtime trace refs（`crates/agentdash-application/src/lifecycle/run_view_builder.rs:183`、`crates/agentdash-application/src/lifecycle/run_view_builder.rs:311`、`crates/agentdash-application/src/lifecycle/run_view_builder.rs:541`、`crates/agentdash-application/src/lifecycle/run_view_builder.rs:594`）。

6. Task / Story execution projection
   - Story aggregate 不保存 Task facts；代码注释明确 Task 计划事实由 `LifecycleRun.tasks` 承担，Story 侧通过 projection 查询（`crates/agentdash-domain/src/story/entity.rs:7`）。
   - `LifecycleTaskPlanItem` 只保存 plan facts：status、priority、agent ids、source_task_id、context_refs、story_ref，不保存 runtime execution/artifacts（`crates/agentdash-domain/src/workflow/value_objects/task_plan.rs:48`）。
   - run-scoped Task CRUD 直接 load run -> mutate `LifecycleRun.tasks` -> update run repo（`crates/agentdash-application/src/task/plan.rs:89`、`crates/agentdash-application/src/task/plan.rs:103`、`crates/agentdash-application/src/task/plan.rs:126`）。
   - Story Task projection 从 story subject associations 找 story-bound/linked runs，再补充 project runs 中显式 `story_ref` 的 tasks（`crates/agentdash-application/src/task/plan.rs:314`）。
   - Task fanout 选择 root run tasks 或 Story projection tasks，再以 `SubjectExecutionIntent(subject_ref=task)` 通过 `LifecycleDispatchService::execute_subject` 启动执行，并写回 `assigned_agent_id`（`crates/agentdash-application/src/task/fanout.rs:151`、`crates/agentdash-application/src/task/fanout.rs:167`、`crates/agentdash-application/src/task/fanout.rs:242`）。
   - Task 专用 execution service 也沿 `SubjectRef(task)` -> associations -> agent/current frame -> anchors -> runtime node projection 解析，但返回的是窄 `TaskExecutionView`（`crates/agentdash-application/src/task/service.rs:20`、`crates/agentdash-application/src/task/service.rs:58`）。
   - Agent runtime tool 的 `TaskReadMode::Execution` 目前返回 execution stub，并说明真实 execution 由 `SubjectExecutionView / linked run projection` 读取（`crates/agentdash-application/src/task/tools.rs:892`、`crates/agentdash-application/src/task/tools.rs:994`）。

7. API / frontend boundary
   - Workflow start route 构造 `LifecycleDispatchService::start_lifecycle_run` 后立即构造 `OrchestrationExecutorLauncher` 并 `drain_ready_nodes`，再返回 latest run view（`crates/agentdash-api/src/routes/workflows.rs:409`、`crates/agentdash-api/src/routes/workflows.rs:452`）。
   - Lifecycle view route 暴露 `/lifecycle-runs/{id}/view` 与 `/subjects/{kind}/{id}/execution`，后者直接调用 `build_subject_execution_view`（`crates/agentdash-api/src/routes/lifecycle_views.rs:37`、`crates/agentdash-api/src/routes/lifecycle_views.rs:80`）。
   - Task plan route 暴露 run-scoped `/lifecycle-runs/{run_id}/tasks` 与 agent-run-scoped `/agent-runs/{run_id}/agents/{agent_id}/tasks`（`crates/agentdash-api/src/routes/task_plan.rs:32`）。
   - Story route 暴露 `/stories/{id}/task-projection` 并调用 `build_story_task_projection`（`crates/agentdash-api/src/routes/stories.rs:60`、`crates/agentdash-api/src/routes/stories.rs:230`）。
   - 前端 `workflow.ts` 只封装 WorkflowGraph definition CRUD/validate/preflight/human decision（`packages/app-web/src/services/workflow.ts:43`、`packages/app-web/src/services/workflow.ts:54`、`packages/app-web/src/services/workflow.ts:84`、`packages/app-web/src/services/workflow.ts:107`）。
   - 前端 `lifecycle.ts` 封装 LifecycleRun / SubjectExecution / ProjectActiveAgents / AgentFrameRuntime / RuntimeTrace / AgentRunWorkspace endpoints（`packages/app-web/src/services/lifecycle.ts:23`）。
   - 前端 `lifecycleStore` 以 lifecycle run、orchestration、runtime node、agent、frame、subject execution、runtime trace 分表归一化；runtime node key 是 `orchestrationId:nodePath:attempt`（`packages/app-web/src/stores/lifecycleStore.ts:1`、`packages/app-web/src/stores/lifecycleStore.ts:32`、`packages/app-web/src/stores/lifecycleStore.ts:76`）。
   - 前端 `workflowStore` 的 editor state 仍同时维护 WorkflowGraph draft 与 per-activity AgentProcedure draft（`packages/app-web/src/stores/workflowStore.ts:181`）。

### 与其它模块的耦合点

- Session：`RuntimeSessionExecutionAnchor` 是 runtime session 到 lifecycle control plane 的 backlink；read projection 和 cancel delivery 通过 anchor repo 解析 session，而不是把 session 作为 business root（`crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:5`、`crates/agentdash-application/src/lifecycle/subject_execution_control.rs:242`）。
- RuntimeSessionCreator：`LifecycleDispatchService` 与 `AgentNodeLauncher` 依赖 `RuntimeSessionCreator` 创建 delivery trace 容器，graph-backed dispatch 若没有 runtime session 会报 internal error（`crates/agentdash-application/src/lifecycle/dispatch_service.rs:111`、`crates/agentdash-application/src/lifecycle/dispatch_service.rs:509`）。
- Permission：API route 在 workflow/lifecycle/task/story read/write 入口做 project permission guard；permission grant 内部不是本轮范围，只保留为边界（`crates/agentdash-api/src/routes/workflows.rs:415`、`crates/agentdash-api/src/routes/lifecycle_views.rs:184`、`crates/agentdash-api/src/routes/task_plan.rs:65`）。
- VFS / capability / frame surface：`AgentFrame` 存 effective capability/context/VFS/MCP/execution profile；`AgentNodeLauncher` 通过 frame composer 注入 workflow node frame surface，本轮不深入 VFS mount / permission 细节（`crates/agentdash-domain/src/workflow/agent_frame.rs:15`、`crates/agentdash-application/src/workflow/orchestration/agent_node_launcher.rs:150`）。
- Frontend contracts：generated `LifecycleRunView` / `SubjectExecutionView` 是前端 lifecycle runtime view 的主合同；前端 store 以 run/orchestration/node/subject/agent/frame 归一化，session 只作为 trace drill-down（`packages/app-web/src/generated/workflow-contracts.ts:215`、`packages/app-web/src/generated/workflow-contracts.ts:253`、`packages/app-web/src/stores/lifecycleStore.ts:1`）。
- Story / Task contracts：Story 页面读 task projection，AgentRun/run scope 写 Task plan facts；Task execution read 应从 SubjectExecutionView 链路下钻，不应从 Task facts 推断 runtime（`crates/agentdash-application/src/task/plan.rs:314`、`crates/agentdash-application/src/task/tools.rs:994`）。

### 值得下一轮深挖的 review 问题

#### P0

- **Lifecycle start API 是否仍把 create-run 与 execute/drain 混成一个事实入口？**  
  Application `start_lifecycle_run` 只创建 Ready orchestration（`crates/agentdash-application/src/lifecycle/dispatch_service.rs:332`），但 API route 立刻 `drain_ready_nodes`（`crates/agentdash-api/src/routes/workflows.rs:452`）。这会让调用方无法观察纯 Ready run，也让 “start lifecycle run” 同时拥有 scheduler side effect。下一轮应确认是否需要拆成 create/start-readiness 与 explicit continue/drain 两个 command 边界。

- **runtime status 聚合的事实源是否已经真正收敛？**  
  Reducer 调 `run.refresh_status_from_orchestrations()`（`crates/agentdash-application/src/workflow/orchestration/runtime.rs:279`），domain 有 `aggregate_lifecycle_run_status`（`crates/agentdash-domain/src/workflow/entity.rs:456`），application 仍有 `derive_orchestration_status`（`crates/agentdash-application/src/workflow/orchestration/runtime.rs:953`）。上一轮已指出重复聚合；下一轮不应重复描述问题，而应精确验证最终 owner 与 mixed terminal/blocked/cancelled 规则。

#### P1

- **`LifecycleDispatchService` 是否仍是过宽的 topology fan-in？**  
  它一次持有 run、graph、agent、frame、association、gate、lineage、anchor、runtime session creator（`crates/agentdash-application/src/lifecycle/dispatch_service.rs:103`），graph-backed dispatch 又在一个方法里完成 run/orchestration/agent/frame/session/anchor/NodeStarted（`crates/agentdash-application/src/lifecycle/dispatch_service.rs:436`）。下一轮应判断哪些职责是 control-plane atomic boundary，哪些可拆成 allocator/writer/composer 以降低耦合。

- **Task execution read model 是否有两个公开/半公开表面？**  
  主合同是 `/subjects/{kind}/{id}/execution` -> `SubjectExecutionView`（`crates/agentdash-api/src/routes/lifecycle_views.rs:80`），但 application 仍保留 `StoryActivityActivationService::get_task_execution_view`（`crates/agentdash-application/src/task/service.rs:20`），AppState 也注入了它（`crates/agentdash-api/src/app_state.rs:86`）。下一轮应确认 Task 专用 execution view 是内部过渡物、测试辅助，还是需要并入 SubjectExecutionView。

- **Agent runtime `task_read execution` mode 是否只是占位，还是会误导调用方？**  
  tool 描述宣称支持 execution mode（`crates/agentdash-application/src/task/tools.rs:415`），但实现返回 `execution_summary: null` stub（`crates/agentdash-application/src/task/tools.rs:892`、`crates/agentdash-application/src/task/tools.rs:994`）。下一轮应确认工具层是否应该直接调用 SubjectExecutionView projector，或明确只返回 plan facts。

- **PlanNode -> ActivityDefinition 反投 adapter 是否仍扩大 runtime/definition 耦合？**  
  `activity_definition_from_plan_node` 明确从 frozen plan snapshot 派生 activation DTO（`crates/agentdash-application/src/lifecycle/projection.rs:73`），但对 LocalEffect / ExtensionAction / None 使用 fake `BashExec("true")`（`crates/agentdash-application/src/lifecycle/projection.rs:93`）。上一轮已指出这个 adapter；下一轮应仅复核它是否还影响 frame composition、hook/VFS projection 或 UI read model。

- **SubjectExecutionView 的 latest runtime node 选择是否足够表达多 association / 多 orchestration / append graph？**  
  当前按 association -> agent current frame -> anchors by agent -> observed_at 最大值选 latest（`crates/agentdash-application/src/lifecycle/run_view_builder.rs:332`）。需要深挖是否应显式返回多条 attempt/association history，而不是只返回 latest node。

- **Task fanout 的 assignment hint 与 subject execution association 是否可能漂移？**  
  fanout 先用 `SubjectExecutionIntent(subject_ref=task)` dispatch，再写 `assigned_agent_id` 到 owning run task（`crates/agentdash-application/src/task/fanout.rs:167`、`crates/agentdash-application/src/task/fanout.rs:242`）。下一轮应确认失败/部分成功场景下 Task plan assignment hint 与实际 agent-scoped association 是否一致。

#### P2

- **Story Task projection 的 source semantics 是否足够区分 owning / linked / story_ref？**  
  projection 用 run-level subject association 判定 OwningRun，否则 LinkedRun，并补充 explicit story_ref（`crates/agentdash-application/src/task/plan.rs:340`）。下一轮可抽样确认跨 run、agent-scoped story association 与 explicit story_ref 混合时 UI 文案和排序是否稳定。

- **前端 workflow editor 的双实体状态是否仍会制造 definition 保存耦合？**  
  `workflowStore` 一个 editor 同时维护 WorkflowGraph draft 与 per-activity AgentProcedure draft（`packages/app-web/src/stores/workflowStore.ts:181`）。这属于前端边界问题，本轮只引用；下一轮若做 frontend review 再深入。

- **Session runtime-control 是否还和 AgentRun workspace 重叠？**  
  前端 service 仍暴露 `fetchSessionRuntimeControl`（`packages/app-web/src/services/lifecycle.ts:62`），但本轮不深入 Session mailbox。下一轮若覆盖 AgentRun/Session 边界，应复核它是否已瘦身为 trace/detail/backlink。

### 不应重复 review 的内容

- 不重复把 `SubjectExecutionControlService` 描述为“直接改 RuntimeNodeState 绕过 reducer”。当前代码已通过 `apply_orchestration_event_to_run(... NodeCancelled ...)` 取消 node（`crates/agentdash-application/src/lifecycle/subject_execution_control.rs:226`）。下一轮只需做回归验证：所有 cancel/terminal writers 是否都走 reducer。
- 不重复上一轮 “Task boot projection 漏 agent-scoped association 并把 absence 推断为 Failed” 的旧问题。当前 `view_projector.rs` 明确跳过 boot projection，并声明 runtime state 由 `SubjectExecutionView` 派生；下一轮关注剩余重复表面，而不是旧 fallback。
- 不重复宽泛讨论 AgentRun workspace / RuntimeSession runtime-control / mailbox action projection；这些已在 06-14 review 的 AgentRun / Session 部分覆盖。本轮只在 Lifecycle/Task 拓扑交界处引用 Session/AgentRun 边界。
- 不重复 PermissionGrant / companion grant、VFS tool provider、local CommandHandler、Extension schema、Tauri shell 等 06-14 已覆盖主题；这些超出本轮 Workflow / Lifecycle / Task / Story execution 主链路。
- 不重复“graphless lifecycle run 是问题”的讨论；既有 review 已明确 plain topology 是普通 Agent runtime 的正常形态。
- 不重复“RuntimeSessionExecutionAnchor 是否应该存在”的讨论；既有 review 与当前 spec 都确认它是正确 backlink。下一轮只检查调用方是否一致消费 anchor。

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` 返回 `Current task: (none)` / `Source: none`；本文件按用户在请求中显式给出的 active task 路径写入。
- 未运行测试，也未修改业务代码；本文件是只读拓扑研究。
- 未深入 Session / AgentRun mailbox、VFS/Permission、local/relay/extension 内部实现；只记录它们与本轮主链路的边界。
- 未做外部联网研究；本轮只使用仓库内 spec、任务文档和代码。
- `crates/agentdash-application/src/task/service.rs` 的 `StoryActivityActivationService` 未在 route 中找到直接公开的 task execution endpoint，但它被注入 `AppState`；需要下一轮按调用图确认是否还有非 route 消费。

## External References

- None. 本轮没有使用外部文档或网络来源。

## Related Specs

- `.trellis/spec/project-overview.md`
- `.trellis/spec/backend/workflow/architecture.md`
- `.trellis/spec/backend/workflow/activity-lifecycle.md`
- `.trellis/spec/backend/workflow/lifecycle-edge.md`
- `.trellis/spec/backend/story-task-runtime.md`
- `.trellis/spec/frontend/workflow-activity-lifecycle.md`
- `.trellis/tasks/06-14-module-overdesign-review/overdesign-review.md`
