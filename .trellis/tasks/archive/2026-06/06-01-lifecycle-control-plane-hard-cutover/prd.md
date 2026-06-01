# Lifecycle 控制面 session-first 旧模型硬切清场

## Goal

将当前初步重构后仍残留的 session-first、run-link、route-local DTO、手写 frontend view、旧 migration baseline 等旧心智模型彻底清出项目，使运行域只围绕以下事实链表达：

```text
LifecycleRun -> LifecycleAgent -> AgentFrame -> RuntimeSession
SubjectRef -> LifecycleSubjectAssociation(anchor = run | agent)
LifecycleAgent -> AgentAssignment -> ActivityAttemptState
```

本任务采用硬切策略，不保留兼容 API / DTO / schema 双轨。RuntimeSession 只保留为 trace / debug / delivery substrate，不再作为 business owner、lifecycle owner、capability truth 或 UI 主导航根。

## Background

`06-01-lifecycle-control-plane-concept-alignment` 已明确原始心智模型：Agent runtime facts 应从 Session / Task / Companion / Permission / HookRuntime / StepActivation 中收束到 `Lifecycle -> LifecycleAgent -> AgentFrame -> RuntimeSession`。

上一轮只读 review 发现当前项目已经完成初步重构：

- `LifecycleAgent`、`AgentFrame`、`AgentAssignment`、`LifecycleSubjectAssociation`、`LifecycleGate` 等领域实体和持久化表已经出现。
- `LifecycleRun.session_id` 在 domain 和后续 migration 中已被删除。
- Task / Routine / Companion 的部分后端入口已开始通过 `ExecutionIntent` 进入 `LifecycleDispatchService`。

但重构尚未真正闭环：

- `RuntimePolicy::CreateRuntimeSession` 仍可能只返回空 runtime ref，ProjectAgent route 仍直接创建 session。
- `AgentAssignment` 仍可能使用 `Uuid::nil()` placeholder，`ExecutionDispatchResult.assignment_ref` 仍可能为空。
- `SessionConstructionPlan`、`StepActivation.apply_to_running_session`、session-indexed hook/runtime paths 仍参与生产链路。
- `LifecycleRunLink` 仍作为 public contract / API / Story runs 事实源存在。
- 目标 lifecycle views 没有进入 `agentdash-contracts` / generated TS，前端存在手写 view 和未注册 route。
- 前端业务入口仍大量使用 `TaskSessionPayload`、`ProjectAgentSession`、`lifecycle_step_key`、`/session/:id`、`agent_session` 等旧词。
- clean baseline 仍先创建旧 schema，再靠后续迁移修正；`lifecycle_gates` 还存在双 schema 定义。

## Confirmed Decisions

- RuntimeSession 创建、启动和 `AgentFrame.runtime_session_refs_json` 回填由 `LifecycleDispatchService` 拥有。
- 任意 Agent runtime launch 前必须存在真实 `LifecycleAgent + AgentFrame + AgentAssignment`，禁止 `Uuid::nil()` 或缺失 assignment 进入 execution evidence 链。
- 旧 session-first / run-link / 手写 view / baseline 旧结构直接删除，不保留短期 alias 或兼容接口。
- `LifecycleSubjectAssociation` 只支持 run anchor 和 agent anchor；Activity / ActivityAttemptState 只作为 assignment 与 execution evidence，不作为 subject anchor。

## Requirements

- `LifecycleDispatchService` 必须成为业务入口创建 RuntimeSession 的唯一编排入口；ProjectAgent、Task、Companion、Routine、manual lifecycle run 不得自行组装 RuntimeSession ownership 或 frame refs。
- Runtime launch 必须从 persisted `AgentFrame` 投影；`SessionConstructionPlan -> RuntimeLaunchRequest` 不得存在于生产路径。
- RuntimeSession 反查必须稳定经过 `AgentFrame -> LifecycleAgent -> AgentAssignment`；没有 frame refs 的 RuntimeSession 不得参与 lifecycle control-plane 路由。
- Agent Activity launch 前必须持久化真实 `AgentAssignment`；ActivityAttemptState 只记录 status、executor evidence、artifacts 和 terminal reason。
- Task execution 默认创建 agent-scoped `LifecycleSubjectAssociation`，Task view 只从 `SubjectRef -> LifecycleAgent -> AgentAssignment -> ActivityAttemptState -> artifacts` 投影。
- Story、Task、Routine、Project、ControlScope 等业务对象统一通过 `LifecycleSubjectAssociation` 查询；删除 public `LifecycleRunLink` API / DTO / repository usage。
- `agentdash-contracts` 必须生成 target DTO；前端不得手写 lifecycle target view 类型作为长期边界。
- ProjectAgent public API 从 `OpenProjectAgentSessionResult / ProjectAgentSession` 改为 `ProjectAgentLaunchResult`。
- Workflow / AgentProcedure 公共路由必须正名：`/workflow-graphs` 与 `/agent-procedures`。
- Frontend 主运行模型必须由 lifecycle run / agent / frame / subject execution 驱动；`/session/:id` 只能作为 runtime trace drill-down。
- Migration clean baseline 必须直接表达目标 schema，不先创建旧 session binding / workflow definition / lifecycle definition / run link / binding kind 结构。
- `lifecycle_gates` 只保留一套与 domain/repository 一致的 schema。

## Acceptance Criteria

- [ ] `rg "SessionConstructionPlan"` 在生产代码无命中；测试若保留必须明确是 legacy fixture 或被删除。
- [ ] `rg "Uuid::nil\\(\\)" crates/agentdash-application/src/workflow` 在 lifecycle agent/frame/assignment/run graph 语义路径无命中。
- [ ] `rg "assignment_ref: None"` 在 Agent execution dispatch 结果无命中。
- [ ] `rg "ProjectAgentSession|OpenProjectAgentSessionResult|TaskSessionPayload|lifecycle_step_key"` 在 frontend、contracts、API 无命中。
- [ ] `rg "LifecycleRunLinkDto|RunLinksResponse|lifecycle_run_links"` 在 public API / contract / readiness 无命中；迁移中只允许一次性 drop/backfill 说明。
- [ ] `rg "fetchWorkflowRunsBySession|by-session|WorkflowRun.*session_id"` 在 frontend 无命中。
- [ ] `rg "agent_session"` 在 frontend workflow mapper 无命中；executor ref kind 统一为 `runtime_session`。
- [ ] `rg "/workflow-definitions|/activity-lifecycle-definitions"` 在 frontend 和 API routes 无命中。
- [ ] `pnpm run contracts:check` 通过，且 generated TS 包含 target lifecycle refs/views。
- [ ] `pnpm run frontend:check` 通过，覆盖 Agent tab、Story panel、Task drawer、workflow runtime ref kind。
- [ ] Backend tests 覆盖 RuntimeSession 创建回填 frame refs、runtime session lookup、real assignment hard guard、tagged `ExecutorRunRef` 查询、agent-scoped SubjectExecutionView、LifecycleGate schema。
- [ ] E2E 覆盖 ProjectAgent launch、Story/Task subject execution、Companion gate resolve、Routine dispatch projection、Permission frame revision。

## Out Of Scope

- 不重新讨论 Lifecycle / Workflow / AgentProcedure 的概念命名方向；本任务只执行硬切清场。
- 不保留旧 API / DTO / schema compatibility。
- 不新增独立业务功能；所有改动服务于旧模型删除和目标事实链闭环。
- 不把 RuntimeSession 从系统中删除；它保留为 trace / delivery substrate。

## Source Evidence

- `.trellis/tasks/06-01-lifecycle-control-plane-concept-alignment/semantic-inventory.md`
- `.trellis/tasks/06-01-lifecycle-control-plane-concept-alignment/abstraction-boundary-and-coupling-review.md`
- `.trellis/tasks/06-01-lifecycle-control-plane-concept-alignment/refactor-plan.md`
- 上一轮只读 review 中确认的残留：RuntimePolicy create gap、nil assignment、SessionConstructionPlan production path、run-link public contract、frontend session-first drift、migration baseline drift。
