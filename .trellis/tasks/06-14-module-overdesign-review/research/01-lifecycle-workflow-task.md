# Research: Lifecycle / Workflow / Task overdesign review

- Query: Review Lifecycle / Workflow / Task for overdesign, over-thick modules, duplicated truth sources, cross-layer coupling, and responsibility drift.
- Scope: internal
- Date: 2026-06-14

## Findings

### 摘要判断

Lifecycle / Workflow runtime 的核心方向已经基本收敛到正确形态：`LifecycleRun.orchestrations[] -> OrchestrationInstance -> RuntimeNodeState` 是主要运行态事实源，`OrchestrationRuntimeEvent` reducer、executor launcher、human gate decision 的主链路大体符合 specs。

真正需要优先清理的不是 reducer 本身，而是周边命令和 projection 仍有旧事实源残留：

- cancel 链路直接改 `RuntimeNodeState`，绕过 reducer。
- Task 启动期 projection 同时漏掉 agent-scoped association，又从“没有活跃 run”推断失败状态。
- run/orchestration status 聚合在 domain aggregate 与 application runtime 中各写一套，规则不同。
- API 层把 lifecycle start 和 ready drain 粘在一起，破坏 Ready/start/continue 的可观察边界。
- Task/Subject execution view 暴露了目标字段，但实际 builder 未填 runtime node/artifacts，旁边又有一套 `/tasks/{id}/execution` 轻量 DTO。

### Files Found

- `crates/agentdash-domain/src/workflow/entity.rs` - `LifecycleRun` aggregate、status 聚合、context/orchestration/view projection 字段。
- `crates/agentdash-domain/src/workflow/value_objects/orchestration.rs` - orchestration plan/runtime node/journal/state exchange value types。
- `crates/agentdash-application/src/workflow/orchestration/runtime.rs` - common orchestration activation 与 event reducer。
- `crates/agentdash-application/src/workflow/orchestration/executor_launcher.rs` - ready node drain、Function/Agent/Human executor launch。
- `crates/agentdash-application/src/workflow/orchestration/agent_node_launcher.rs` - AgentCall node launch、agent/frame/session/anchor materialization。
- `crates/agentdash-application/src/workflow/orchestration/human_gate_launcher.rs` - HumanGate open/decision bridge。
- `crates/agentdash-application/src/workflow/dispatch_service.rs` - lifecycle/subject/agent dispatch facade。
- `crates/agentdash-application/src/workflow/orchestrator.rs` - runtime session terminal 与 `complete_lifecycle_node` terminal bridge。
- `crates/agentdash-application/src/workflow/subject_execution_control.rs` - subject cancel control。
- `crates/agentdash-application/src/workflow/projection.rs` - active workflow projection and `PlanNode -> ActivityDefinition` adapter。
- `crates/agentdash-application/src/workflow/activity_activation.rs` - activity/node frame activation calculation。
- `crates/agentdash-application/src/workflow/lifecycle_run_view_builder.rs` - `LifecycleRunView` / `SubjectExecutionView` read model builder。
- `crates/agentdash-application/src/task/view_projector.rs` - boot-time Task status projection from lifecycle state。
- `crates/agentdash-application/src/task/service.rs` - `/tasks/{id}/execution` application view。
- `crates/agentdash-api/src/routes/workflows.rs` - workflow graph and lifecycle run API route。
- `crates/agentdash-api/src/routes/lifecycle_views.rs` - subject/run lifecycle view API route。
- `crates/agentdash-api/src/routes/story_runs.rs` - story run compatibility route returning subject execution view。
- `crates/agentdash-api/src/routes/task_execution.rs` - task execution API route。

### Issue 1: cancel 链路绕过 orchestration reducer

- Priority: P0
- Problem type: 重复事实源 / 绕过运行态事实源 / 职责漂移
- Evidence paths:
  - `crates/agentdash-application/src/workflow/subject_execution_control.rs:202`
  - `crates/agentdash-application/src/workflow/subject_execution_control.rs:299`
  - `.trellis/spec/backend/workflow/activity-lifecycle.md:26`
- Concrete code evidence:
  - `materialize_cancelled_node` 直接 `iter_mut()` 找 `run.orchestrations`，再调用 `mark_runtime_node_cancelled` 修改 node tree：`subject_execution_control.rs:216-241`。
  - `mark_runtime_node_cancelled` 直接设置 `node.status = RuntimeNodeStatus::Cancelled`、`completed_at` 和 `error`：`subject_execution_control.rs:299-323`。
  - spec 明确规定 node status、inputs、outputs、executor refs、trace refs、error、cache refs 和 state exchange 都由 common orchestration runtime reducer 写入：`activity-lifecycle.md:26`。
- Impact:
  - cancel 不走 `OrchestrationRuntimeEvent::NodeCancelled`，因此不会统一执行 ready queue 清理、orchestration status 派生、LifecycleRun status sync、terminal idempotency 等 reducer 行为。
  - 当前实现只更新 `orchestration.updated_at` 和 run repository，不显式同步 `run.status`，也不清理 `dispatch.ready_node_ids` / `activation.ready_node_ids`。
  - 这会让 cancel 与 terminal callback / complete tool / executor launcher 形成两套推进路径。
- Suggested cleanup:
  - 删除 `mark_runtime_node_cancelled` 这条直接 mutation 路径。
  - `materialize_cancelled_node` 应解析 anchor 后调用 `apply_orchestration_event_to_run(run, orchestration_id, OrchestrationRuntimeEvent::NodeCancelled { ... })` 并整体 update run。
  - graphless subject cancel 只生成 delivery cancel command；有 orchestration binding 时才 materialize node cancellation。

### Issue 2: Task boot projection 使用错误 association scope，并从缺失事实推断失败

- Priority: P0
- Problem type: 重复事实源 / 投影事实错误 / 兼容式 fallback 残留
- Evidence paths:
  - `crates/agentdash-application/src/workflow/dispatch_service.rs:572`
  - `crates/agentdash-application/src/task/view_projector.rs:123`
  - `crates/agentdash-application/src/task/view_projector.rs:139`
  - `crates/agentdash-application/src/task/view_projector.rs:214`
  - `.trellis/spec/backend/story-task-runtime.md:104`
- Concrete code evidence:
  - dispatch 对 `task` / `story` 创建 agent-scoped association：`LifecycleSubjectAssociation::new_agent_scoped(...)` at `dispatch_service.rs:580-581`。
  - boot projector 只查 whole-run association：`association_repo.list_by_anchor(run.id, None)` at `view_projector.rs:123`，因此会漏掉实际 task/story execution association。
  - 对每个 task association 使用同一个 `latest_status = statuses.last().copied()`：`view_projector.rs:134-143`；这个状态来自整条 run 的所有 node 遍历，不绑定 association 的 agent/frame/anchor/node。
  - Phase 2 fallback 把没有 active run 覆盖的 Running task 强制置为 Failed：`view_projector.rs:214-239`。
  - spec 的 Task execution view 查询路径是 `list_by_subject(Task, task_id) -> anchor agent / run -> LifecycleAgent.current_frame / runtime anchors / artifacts -> RuntimeNodeState`：`story-task-runtime.md:104-112`。
- Impact:
  - 真实任务执行通常是 agent-scoped，启动投影会漏投。
  - 漏投后 Running task 会被“孤儿 fallback”误判为 Failed，Task 状态由 absence/inference 写入，而不是由 lifecycle runtime fact 写入。
  - 多 task / 多 node / append orchestration 场景下，多个 task 可能吃到同一个 run 最后遍历到的 node status。
- Suggested cleanup:
  - Task projection 必须从 `SubjectRef(kind=Task)` 出发，使用 `list_by_subject` 找 association，再沿 `anchor_agent_id -> LifecycleAgent.current_frame -> RuntimeSessionExecutionAnchor -> orchestration_id + node_path + attempt` 定位 runtime node。
  - 删除“无 active run 覆盖则 Running -> Failed”的 fallback；没有 lifecycle fact 时不写终态。
  - 若需要启动期 reconcile，只 materialize explicit runtime node terminal/active facts，不从缺失关系推断业务失败。

### Issue 3: LifecycleRun status 聚合有两套实现且语义不一致

- Priority: P1
- Problem type: 重复事实源 / 状态规则漂移
- Evidence paths:
  - `crates/agentdash-domain/src/workflow/entity.rs:260`
  - `crates/agentdash-application/src/workflow/orchestration/runtime.rs:953`
  - `crates/agentdash-application/src/workflow/orchestration/runtime.rs:995`
- Concrete code evidence:
  - domain aggregate 在 `add_orchestration` / `replace_orchestration` 后调用 `refresh_status_from_orchestrations`：`entity.rs:215-237`，具体规则在 `aggregate_orchestration_status`：`entity.rs:265-308`。
  - application reducer 在 `apply_orchestration_event_to_run` 后调用另一套 `sync_lifecycle_run_status_from_orchestrations`：`runtime.rs:266-283`、`runtime.rs:995-1024`。
  - `derive_orchestration_status` 又是一套 orchestration-level 聚合：`runtime.rs:953-983`。
  - 规则存在实际差异：domain 对 Completed + Cancelled 混合会落到 Ready；application 对相同混合会落到 Running。domain 还把 `OrchestrationStatus::Paused` 放在 Running 判断里，而 application 将 Paused 映射为 Blocked。
- Impact:
  - run status 取决于最后一次写入走的是 aggregate method 还是 runtime reducer。
  - Story active run、Task projector、project active agent view 都消费 run status；状态漂移会扩大到 UI/API 投影。
  - 后续 append orchestration / review flow / cancel 混合状态会更难推理。
- Suggested cleanup:
  - 保留一个 status aggregation owner。推荐 domain 暴露唯一 `refresh_status_from_orchestrations`/pure helper，application reducer 调同一个 helper。
  - 明确混合 terminal orchestration 的聚合语义；预研期可直接选正确规则，不做兼容映射。
  - 所有 run write path，包括 cancel、start、append orchestration，都必须经过同一聚合函数。

### Issue 4: `/lifecycle-runs` API 同时执行 start 和 ready drain

- Priority: P1
- Problem type: ready/start/continue 职责耦合 / API command 语义漂移
- Evidence paths:
  - `crates/agentdash-application/src/workflow/dispatch_service.rs:306`
  - `crates/agentdash-api/src/routes/workflows.rs:409`
  - `.trellis/spec/backend/workflow/architecture.md:306`
- Concrete code evidence:
  - `LifecycleDispatchService::start_lifecycle_run` 只 compile graph、创建 `LifecycleRun`、添加 root orchestration 并持久化：`dispatch_service.rs:306-323`。
  - API route `start_lifecycle_run` 在拿到 Ready run 后立即构造 `OrchestrationExecutorLauncher` 并调用 `launcher.drain_ready_nodes(run.id)`：`workflows.rs:452-457`。
  - spec 明确 `start_lifecycle_run` 只初始化 orchestration，不创建 runtime session，entry node 仍为 Ready：`architecture.md:306`；validation matrix 也写 `entry node Ready，无 executor ref`：`architecture.md:315`。
- Impact:
  - API 的 “start” 实际变成 “create + continue/drain”，调用者无法稳定观察 Ready orchestration。
  - 入口节点如果是 Agent/Function/HumanGate，API route 会直接创建 runtime session / function run / gate side effect。
  - 后续要做审批、预览、人工确认、批量调度或显式 continue，会被当前 route 语义卡住。
- Suggested cleanup:
  - `POST /api/workflows/lifecycle-runs` 只创建 lifecycle run + root orchestration，并返回 Ready view。
  - 增加显式 continue/drain command endpoint（例如 lifecycle run orchestration drain/continue），由调用方决定何时启动 ready nodes。
  - human decision 和 session terminal callback 继续在 terminal 后 drain successor，这是 terminal bridge，不应与 initial start route 混合。

### Issue 5: Subject/Task execution projection 暴露目标字段但没有 materialize runtime node / artifacts，旁边还保留另一套 Task execution DTO

- Priority: P1
- Problem type: 重复 read model / 投影不完整 / 职责漂移
- Evidence paths:
  - `crates/agentdash-application/src/workflow/lifecycle_run_view_builder.rs:229`
  - `crates/agentdash-contracts/src/workflow.rs:1461`
  - `crates/agentdash-application/src/task/service.rs:17`
  - `crates/agentdash-api/src/routes/task_execution.rs:23`
  - `.trellis/spec/backend/story-task-runtime.md:29`
- Concrete code evidence:
  - contract `SubjectExecutionView` 暴露 `latest_runtime_node` 与 `artifacts`：`workflow.rs:1461-1474`。
  - builder 固定 `let latest_runtime_node = None; let artifacts = json!({});`，再原样返回：`lifecycle_run_view_builder.rs:240-264`。
  - `/tasks/{id}/execution` 走 `StoryActivityActivationService::get_task_execution_view`，只要找到 association 就返回 `execution_status = Some("active")`，`delivery_runtime_ref = None`，`task_status` 来自 Story task materialized status：`task/service.rs:21-45`。
  - API route 又返回独立 `TaskExecutionViewResponse`：`task_execution.rs:23-51`。
  - spec 规定 Task 投影字段 status/artifacts/current agent/latest runtime node 由 lifecycle association、LifecycleAgent、AgentFrame、RuntimeNodeState 与 artifacts 派生：`story-task-runtime.md:29-33`。
- Impact:
  - 客户端面对两套 execution view：`/subjects/task/{id}/execution` 和 `/tasks/{id}/execution`。
  - 一个 view 字段完整但核心字段为空；另一个 view 简短但状态来自 Story task projection，不能定位具体 node/artifacts。
  - Task projection 的正确事实链被稀释，后续 UI 容易继续依赖 Task.status 或 association presence。
- Suggested cleanup:
  - 以 `SubjectExecutionView` 作为唯一 task/story execution read model，`/tasks/{id}/execution` 直接复用或删除。
  - builder 从 association 出发定位 agent/frame/runtime anchor，再填 `latest_runtime_node` 与 artifact projection。
  - `execution_status` 不应由 association presence 固定为 `"active"`，应由 runtime node/run/agent 状态派生。

### Issue 6: runtime path 仍通过 `PlanNode -> ActivityDefinition` 旧 DTO 适配，且对部分 executor 伪造 BashExec

- Priority: P2
- Problem type: 抽象泄漏 / 命名职责漂移 / 过度适配
- Evidence paths:
  - `crates/agentdash-application/src/workflow/projection.rs:73`
  - `crates/agentdash-application/src/workflow/orchestration/agent_node_launcher.rs:262`
  - `crates/agentdash-application/src/workflow/frame_construction/composer_lifecycle_node.rs:74`
  - `crates/agentdash-application/src/workflow/activity_activation.rs:43`
  - `.trellis/spec/backend/workflow/activity-lifecycle.md:19`
- Concrete code evidence:
  - `activity_definition_from_plan_node` 把 frozen runtime `PlanNode` 投成 `ActivityDefinition`：`projection.rs:73-120`。
  - LocalEffect / ExtensionAction / None 被映射成 `ActivityExecutorSpec::Function(BashExec { command: "true" })`：`projection.rs:93-101`。
  - Agent node launcher 和 frame construction 都调用这个 adapter 后再进入 lifecycle node frame compose：`agent_node_launcher.rs:262`、`composer_lifecycle_node.rs:74`。
  - `ActivityActivationInput` 仍接收 `active_activity: &ActivityDefinition`：`activity_activation.rs:43-47`。
  - spec 允许 `ActivityDefinition` 继续表达 definition/template 语义，但进入 runtime 前必须转换为 semantic `OrchestrationPlanSnapshot`：`activity-lifecycle.md:19`。
- Impact:
  - runtime activation path 持续依赖 definition-era DTO，导致命名上还是 activity，语义上却是 runtime node。
  - 伪造 BashExec 目前可能只为满足字段，但一旦 activation/permission/prompt 继续读取 executor，会产生错误事实。
  - 后续 PlanNodeKind 增多时，每个新 executor 都要维护旧 DTO adapter。
- Suggested cleanup:
  - 让 activation/frame compose 直接消费 `PlanNode` 或新建窄类型 `LifecycleNodeActivationSpec`，只包含 label、ports、completion policy、node_path、attempt、contract refs。
  - 删除 fake BashExec adapter；不需要的 executor 字段不要传。
  - 保留 `ActivityDefinition` 在 WorkflowGraph definition/template 层，不再作为 runtime activation DTO。

### Issue 7: LifecycleDispatchService 过厚，横跨 compile/run/agent/frame/session/association/gate/lineage/NodeStarted

- Priority: P2
- Problem type: 模块过厚 / 横向耦合
- Evidence paths:
  - `crates/agentdash-application/src/workflow/dispatch_service.rs:90`
  - `crates/agentdash-application/src/workflow/dispatch_service.rs:101`
  - `crates/agentdash-application/src/workflow/dispatch_service.rs:330`
  - `crates/agentdash-application/src/workflow/dispatch_service.rs:787`
- Concrete code evidence:
  - service 自述职责包括 run、orchestration、association、agent、frame、gate/lineage：`dispatch_service.rs:90-100`。
  - struct 同时持有 run、graph、agent、frame、association、gate、lineage、anchor、runtime session creator：`dispatch_service.rs:101-110`。
  - `dispatch_common` 一次完成 graph resolve/compile、run/orchestration update、agent、association、runtime session、frame、lineage、gate、anchor、`NodeStarted` reducer event：`dispatch_service.rs:330-423`。
  - `ensure_workflow_graph_orchestration` 也在同文件内 materialize orchestration：`dispatch_service.rs:787-807`。
- Impact:
  - 这个 facade 已经接近 “workflow runtime transaction script”，任何 subject/agent/frame/session/graph/compiler 变化都会触碰同一模块。
  - 与 `OrchestrationExecutorLauncher` 的 agent/node launch 职责存在相邻边界，特别是 graph-backed dispatch 的 entry `NodeStarted` 与 executor launcher 的 AgentCall `NodeStarted`。
  - 文件 1854 行，其中生产职责和测试夹杂，审查和修改成本高。
- Suggested cleanup:
  - 保留一个薄 `LifecycleDispatchService` facade，但把内部切成清晰 use-case helpers：
    - `RunOrchestrationStarter`: graph resolve/compile + run/orchestration creation。
    - `AgentRuntimeAllocator`: agent/frame/runtime session/anchor creation。
    - `SubjectAssociationWriter`: subject association role/anchor 写入。
    - `InteractionGateWriter`: gate/lineage side effect。
  - entry node 的 `NodeStarted` 仍必须走 reducer；只是把 transaction 边界显式化，避免 facade 继续增厚。

## Code Patterns

- Correct pattern: terminal/launcher path submits `OrchestrationRuntimeEvent` then persists returned `LifecycleRun`.
  - `orchestrator.rs:242` calls `apply_orchestration_event_to_run` for complete tool terminal materialization.
  - `executor_launcher.rs:229-240` submits `NodeCompleted` for human decision and then drains successors.
  - `executor_launcher.rs:396` persists reducer output through repository update.
- Risk pattern: direct mutation of `LifecycleRun.orchestrations[].node_tree`.
  - `subject_execution_control.rs:216-241` directly mutates orchestration node tree on cancel.
  - `task/view_projector.rs:293-325` flattens all runtime node statuses without a coordinate and uses the last traversal status as task projection input.
- Risk pattern: association presence used as execution state.
  - `task/service.rs:24-45` maps any resolved association to `"active"` execution_status.
  - `lifecycle_run_view_builder.rs:240-264` does not derive `latest_runtime_node` or artifacts despite contract fields.
- Risk pattern: status aggregation duplicated.
  - `entity.rs:265-308` and `runtime.rs:995-1024` encode separate lifecycle run aggregation rules.

## External References

- No external references used. This review is based on repository code and local Trellis specs only.

## Related Specs

- `.trellis/spec/backend/workflow/architecture.md`
- `.trellis/spec/backend/workflow/activity-lifecycle.md`
- `.trellis/spec/backend/workflow/lifecycle-run-link.md`
- `.trellis/spec/backend/story-task-runtime.md`
- `.trellis/spec/frontend/workflow-activity-lifecycle.md` was listed by backend workflow architecture as related, but this research scope stayed on backend routes/application/domain code.

## Not Problems / Boundaries Not Recommended To Move Now

- `OrchestrationRuntimeEvent` reducer and `OrchestrationExecutorLauncher` are not the main overdesign problem. They encode the right central runtime state transition pattern and have useful separation between reducer and executor side effects.
- `LifecycleRun.context`, `orchestrations`, and `view_projection` existing on the aggregate is consistent with the current backend spec. `view_projection` is unused in the reviewed paths, but it is documented as a read projection placeholder; do not remove it as part of this cleanup unless the spec changes.
- Graphless lifecycle runs are not overdesign. Specs explicitly use graphless as normal Agent runtime topology; the issue is only when graph-backed paths leak into graphless assumptions or vice versa.
- `RuntimeSessionExecutionAnchor` remains the right trace-to-control-plane index. The reviewed issues come from not using it in Task projection/cancel consistently, not from the anchor abstraction itself.
- `LifecycleRun.execution_log` appears to be user-readable/audit data exposed through VFS/API, not the durable runtime state owner. Do not merge it with `OrchestrationJournalFact` until journal persistence/materialization becomes a concrete task.
- `WorkflowGraph` and `ActivityDefinition` in definition/template space are acceptable per specs. The cleanup target is runtime activation depending on an `ActivityDefinition` adapter, not the definition model itself.

## Follow-up Task Candidates

1. Fix subject cancel to route through orchestration reducer.
   - Scope: `subject_execution_control.rs` only plus focused tests.
   - Acceptance: cancel writes `NodeCancelled` through `apply_orchestration_event_to_run`, syncs run/orchestration status, and preserves runtime delivery command.

2. Rewrite Task projection from association/anchor/node coordinates.
   - Scope: `task/view_projector.rs`, `task/service.rs`, `lifecycle_run_view_builder.rs`.
   - Acceptance: Task projection starts from `SubjectRef`, handles agent-scoped association, removes orphan-failed fallback, and fills latest runtime node/artifacts.

3. Unify lifecycle run status aggregation.
   - Scope: domain aggregate + application runtime reducer.
   - Acceptance: one helper owns status aggregation; all mutation paths call it; mixed terminal orchestration cases have explicit tests.

4. Split lifecycle run start from ready drain.
   - Scope: `workflows.rs` route contract and explicit continue/drain API/use-case.
   - Acceptance: create/start route returns Ready entry without runtime session; separate command launches ready nodes.

5. Replace `PlanNode -> ActivityDefinition` runtime adapter with `LifecycleNodeActivationSpec`.
   - Scope: `projection.rs`, `activity_activation.rs`, `composer_lifecycle_node.rs`, `agent_node_launcher.rs`.
   - Acceptance: runtime frame activation consumes semantic plan node data without fake executor values.

6. Thin `LifecycleDispatchService`.
   - Scope: internal application module split, no behavior change.
   - Acceptance: graph compile/orchestration start, runtime allocation, association writing, and gate/lineage writing have separate helpers behind the same public facade.

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` returned no active task in this shell, so the task path came from the explicit user-provided path `.trellis/tasks/06-14-module-overdesign-review`.
- No code, tests, or specs were modified; only this research file was written.
- No automated tests were run because this is a read-only architecture review.
- `LifecycleRunLink` code was not found in the reviewed implementation paths; the current implementation appears to use `LifecycleSubjectAssociation`.
