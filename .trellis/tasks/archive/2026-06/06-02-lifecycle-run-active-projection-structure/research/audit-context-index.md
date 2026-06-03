# Audit Context Index: LifecycleRun Active Projection Structure

## Recovery Entry Point

上下文压缩后先读本文件，再按下面顺序恢复审计背景：

1. `.trellis/tasks/06-02-lifecycle-run-active-projection-structure/prd.md`
2. `.trellis/tasks/06-02-lifecycle-run-active-projection-structure/design.md`
3. `.trellis/tasks/06-02-lifecycle-run-active-projection-structure/implement.md`
4. `.trellis/tasks/06-02-lifecycle-control-plane-frame-convergence/prd.md`
5. `.trellis/tasks/06-02-lifecycle-control-plane-frame-convergence/design.md`
6. `.trellis/tasks/06-02-lifecycle-control-plane-frame-convergence/implement.md`

本任务是父任务的尾部清理项。它的审计目标不是单独替换一个字段，而是确认 run-level active projection 已回到只读投影位置：`WorkflowGraphInstance.activity_state` 承载 Activity runtime state，`LifecycleRunView` 暴露结构化 read model，后端业务推进路径使用 graph/activity/attempt identity。

本任务还要恢复一个公开类型暴露目标：Agent / Lifecycle 是 runtime state 与业务 state 的主入口；Session 只承载 runtime trace、turn supervision、transport delivery。审计时需要确认 session-indexed 查询只是 adapter，返回体回到 Agent / Lifecycle anchored generated contracts。

## Required Parent And Sibling Context

这些任务文件是本任务依赖关系和事实归属的来源：

- `.trellis/tasks/06-02-lifecycle-control-plane-frame-convergence/prd.md`
  - 读取目标职责划分：Frame 锚定 runtime session 可执行事实，Assignment 锚定 Activity attempt 执行事实，Session 保留 turn/runtime supervision。
  - 读取 `Start Order`，确认本任务排在 anchor、artifact、frontend、launch envelope 之后。
- `.trellis/tasks/06-02-lifecycle-control-plane-frame-convergence/design.md`
  - 读取目标 architecture flow，尤其是 `WorkflowGraphInstance.activity_state -> read model` 与 run-level projection 的派生位置。
- `.trellis/tasks/06-02-lifecycle-control-plane-frame-convergence/implement.md`
  - 读取 Phase 5 与 parent final integration，确认本任务的验收边界。
- `.trellis/tasks/06-02-runtime-session-frame-assignment-anchor/prd.md`
  - 确认 runtime session 到 frame / assignment / attempt 的直接锚点语义。
- `.trellis/tasks/06-02-runtime-session-frame-assignment-anchor/design.md`
  - 确认 `runtime_session_execution_anchors` 或等价 direct query 的目标结构。
- `.trellis/tasks/06-02-scoped-lifecycle-artifacts/prd.md`
  - 确认 output artifact 的 graph/activity/attempt scope。
- `.trellis/tasks/06-02-scoped-lifecycle-artifacts/design.md`
  - 确认 artifact ref 与 active attempt identity 的对齐方式。
- `.trellis/tasks/06-02-frame-launch-envelope-session-boundary/prd.md`
  - 确认 Session launch 解析上提到 Frame construction 后，Session 层不再补 owner/context/capability/VFS/MCP fact。
- `.trellis/tasks/06-02-frontend-session-runtime-frame-query/prd.md`
  - 确认前端 session runtime view 以后端 frame-runtime endpoint 为准。

## Required Research Context

这些父任务 research 已经包含 subagent 审计结论，恢复时优先读摘要和 Findings：

- `.trellis/tasks/06-02-lifecycle-control-plane-frame-convergence/research/frontend-runtime-query.md`
  - 关键事实：前端业务 UI 当前没有直接消费 `active_node_keys`。
  - 关键事实：后端仍在 `select_active_run`、`advance_node` 等 runtime/control path 使用 run-level active projection。
  - 关键事实：`LifecycleRunView` generated type 当前不暴露 `active_node_keys`。
- `.trellis/tasks/06-02-lifecycle-control-plane-frame-convergence/research/runtime-session-assignment-anchor.md`
  - 关键事实：terminal / advance 应通过 runtime session execution anchor 直达 assignment / attempt。
  - 关键事实：`ActivityAttemptRefDto`、`RuntimeSessionExecutionAnchorDto`、扩展后的 `AgentAssignmentRefDto` 会影响本任务的 structured active ref 命名。
- `.trellis/tasks/06-02-lifecycle-control-plane-frame-convergence/research/scoped-lifecycle-artifacts.md`
  - 关键事实：artifact scope 与 active attempt identity 都应使用 `graph_instance_id + activity_key + attempt`。
  - 关键事实：VFS/hook/completion gate 的 scoped artifact 结果会减少本任务对 run-level active key 的依赖。
- `.trellis/tasks/06-02-lifecycle-control-plane-frame-convergence/research/session-frame-launch-boundary.md`
  - 关键事实：Frame launch envelope 收口后，runtime execution projection 应由 Frame construction 供给，Session launch 只处理 turn/runtime。

## Required Specs

这些 spec 是审计时的 durable invariants：

- `.trellis/spec/backend/workflow/architecture.md`
  - 读取 workflow graph instance、run、assignment、frame 的边界。
- `.trellis/spec/backend/workflow/activity-lifecycle.md`
  - 读取 Activity runtime identity：`graph_instance_id + activity_key + attempt`。
- `.trellis/spec/backend/workflow/lifecycle-run-link.md`
  - 读取 RuntimeSession 作为 trace container 的定位，以及 lifecycle run link 的目标方向。
- `.trellis/spec/backend/session/runtime-execution-state.md`
  - 读取 Session runtime 的保留职责，帮助判断哪些 active state 逻辑应留在 Session 层外。
- `.trellis/spec/backend/session/execution-context-frames.md`
  - 读取 ExecutionContext / Frame projection 的边界。
- `.trellis/spec/frontend/workflow-activity-lifecycle.md`
  - 读取 frontend runtime observation contract。
- `.trellis/spec/frontend/architecture.md`
  - 读取 frontend store / generated DTO 消费约束。
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`
  - 读取 `agentdash-contracts` 生成 TypeScript 的跨层约束。

## Active Projection Code Context

这些文件是本任务必须审计的实现入口：

- `crates/agentdash-domain/src/workflow/entity.rs`
  - `LifecycleRun.lifecycle_id`
  - `LifecycleRun.active_node_keys`
  - `LifecycleRun.current_activity_key()`
  - `LifecycleRun.sync_graph_instance_activity_projections(...)`
  - `active_activity_keys(...)`
- `crates/agentdash-domain/src/workflow/workflow_graph_instance.rs`
  - `WorkflowGraphInstance.activity_state`
  - `replace_activity_state(...)` 的 graph instance id 校验。
- `crates/agentdash-domain/src/workflow/value_objects/run_state.rs`
  - `ActivityLifecycleRunState`
  - attempt/status/output 的 graph-scoped runtime state。
- `crates/agentdash-application/src/workflow/run.rs`
  - `select_active_run(...)` 目前用 `current_activity_key()` 判断 active run。
- `crates/agentdash-application/src/workflow/tools/advance_node.rs`
  - tool 输出和推进结果仍展示 / 返回 `active_node_keys`。
- `crates/agentdash-application/src/workflow/lifecycle_run_view_builder.rs`
  - `LifecycleRunView` 构建入口。
  - `workflow_graph_instances` / `activity_state_views` / latest attempt projection。
- `crates/agentdash-contracts/src/workflow.rs`
  - `LifecycleRunView`
  - `WorkflowGraphInstanceView`
  - `ActivityAttemptView`
  - 后续 `ActiveActivityRef` / `ActivityAttemptRefDto` 的 generated contract 位置。
- `crates/agentdash-api/src/routes/lifecycle_views.rs`
  - session-indexed runtime trace / frame-runtime adapter 的 route 位置。
  - 确认返回体锚定 `AgentFrameRuntimeView`、runtime anchor、attempt ref，而不是新增 session-first business runtime view。
- `packages/app-web/src/generated/workflow-contracts.ts`
  - 合同生成后的 TypeScript baseline。
- `packages/app-web/src/stores/lifecycleStore.ts`
  - 前端 normalized lifecycle store 和 run/agent/frame/runtime trace 消费入口。
- `packages/app-web/src/services/lifecycle.ts`
  - `fetchAgentFrameRuntime`、`fetchRuntimeTrace`、后续 session-indexed frame runtime service 的收口位置。
- `packages/app-web/src/types/session.ts`
  - session 类型只应保留 trace / hook runtime metadata / transport 所需字段。
- `packages/app-web/src/features/workspace-panel/ContextOverviewTab.tsx`
  - frontend active workflow display 已从 hook metadata 与 graph instance attempts 推导。
- `packages/app-web/src/features/workspace-panel/ContextOverviewTab.projection.test.tsx`
  - active projection 前端测试入口。

## Persistence And Migration Context

这些文件决定 active projection 是持久字段、迁移字段，还是 read-builder 派生字段：

- `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs`
  - `RUN_COLS`
  - insert/update/load `active_node_keys`
  - `lifecycle_id` load/list 语义。
- `crates/agentdash-infrastructure/migrations/0001_init.sql`
  - `lifecycle_runs.lifecycle_id` 与初始 run schema。
- `crates/agentdash-infrastructure/migrations/0086_drop_lifecycle_run_activity_state.sql`
  - `active_node_keys` 当前 schema 来源之一。
- `crates/agentdash-infrastructure/migrations/0095_lifecycle_execution_log_activity_key.sql`
  - execution log 曾从 `active_node_keys[0]` 派生 active key，需确认实现后不再保留业务依赖。

## Upstream Writers To Audit

这些文件会写入或同步 graph instance activity state，并可能调用 run-level projection sync：

- `crates/agentdash-application/src/workflow/dispatch_service.rs`
  - subject execution 初始化 activity state。
  - `ensure_graph_instance_activity_state(...)`
  - `run.sync_graph_instance_activity_projections(...)`
- `crates/agentdash-application/src/workflow/lifecycle/mount.rs`
  - lifecycle mount 初始化 activity state 和 run projection。
- `crates/agentdash-application/src/session/assembler.rs`
  - session assembly 测试 / fixture 里构造 activity state 并同步 run projection。
- `crates/agentdash-application/src/workflow/subject_execution_control.rs`
  - subject execution control 测试 / helper 中的 projection sync。
- `crates/agentdash-application/src/task/view_projector.rs`
  - 明确 Task view 真相源为 `WorkflowGraphInstance.activity_state`，测试中也有 projection sync。
- `crates/agentdash-application/src/vfs/provider_lifecycle.rs`
  - lifecycle VFS helper / tests 中使用 run lifecycle id 和 graph instance activity state。

## Adjacent Anchor Context

这些文件不是本任务直接修改的第一入口，但决定 run-level projection 什么时候可以安全降级：

- `crates/agentdash-application/src/workflow/projection.rs`
  - assignment / runtime projection 已使用 `WorkflowGraphInstance.activity_state` 匹配 active attempt。
- `crates/agentdash-application/src/workflow/session_association.rs`
  - runtime session terminal / advance resolver 与 assignment anchor 的当前反查路径。
- `crates/agentdash-application/src/workflow/orchestrator.rs`
  - `on_session_terminal`、`advance_current_activity` 的 ActivityEvent 推进路径。
- `crates/agentdash-application/src/hooks/workflow_snapshot.rs`
  - hook control target 从 runtime session 解析 assignment / attempt 的路径。
- `crates/agentdash-application/src/workflow/agent_executor.rs`
  - activity launch 创建 assignment、frame、runtime session 的写入边界。

## Public Exposure Context

这些文件用于检查公开类型和入口是否回到 Agent / Lifecycle 锚点：

- `crates/agentdash-contracts/src/workflow.rs`
  - workflow generated contracts 的唯一跨层来源。
  - 检查 active runtime fields 是否位于 `LifecycleRunView`、`WorkflowGraphInstanceView`、`ActivityAttemptView`、`AgentFrameRuntimeView` 或 explicit attempt ref。
- `crates/agentdash-api/src/routes/lifecycle_views.rs`
  - session-indexed route 可以存在，但应作为 adapter 返回 Agent / Lifecycle anchored view。
- `packages/app-web/src/services/lifecycle.ts`
  - 前端 service 应优先消费 generated workflow contracts。
- `packages/app-web/src/types/session.ts`
  - session 类型用于 trace / hook metadata / transport；审计是否有业务 runtime state 长期停留在这里。
- `packages/app-web/src/features/workspace-panel/model/useSessionRuntimeState.ts`
  - session route 进入 workspace runtime state 时，应通过后端 adapter 获得 AgentFrameRuntimeView。
- `packages/app-web/src/features/session-context/hook-runtime-cards.tsx`
  - hook metadata 展示可引用 session trace，但 active workflow identity 应回到 workflow attempt refs。

## Audit Questions To Preserve

恢复上下文后按这些问题逐项审计：

1. `WorkflowGraphInstance.activity_state` 是否已经足以派生所有 active Activity read model 字段？
2. 后端 runtime/control path 是否仍通过 `LifecycleRun.current_activity_key()` 或 `active_node_keys` 做业务判断？
3. `LifecycleRunView` 是否应该持有 `active_activity_refs`，还是由前端从 `workflow_graph_instances[].activities[].attempts[]` 派生？
4. 若保留 `active_activity_refs`，它的同步 owner 是 read builder 还是 domain aggregate？
5. `lifecycle_id` 是否继续表达 root graph id，还是在本任务中迁移为 `root_graph_id` / `root_graph_instance_id`？
6. `advance_node` tool 的返回值是否应改为 structured active refs / next attempt refs？
7. migrations 是否能直接进入目标状态，并移除旧字段运行时依赖？
8. frontend 是否只消费 generated DTO，并保持 active workflow UI 从 graph instance attempts 读取？
9. anchor、artifact、frame launch 三个前置任务完成后，是否还有 session-first / run-first fallback 重新引入 active key 字符串？
10. 新增或调整的公开 DTO 是否把业务 runtime state 锚定在 Agent / Lifecycle，而不是 session-first 类型？
11. session-indexed endpoint 是否只是 adapter，并在返回体中显式给出 frame / assignment / attempt / lifecycle 锚点？
12. 前端是否还存在从 session resource 继续推导 Agent / Frame / Activity 的长期路径？

## Search Commands

恢复时优先运行这些命令刷新上下文：

```powershell
rg -n "active_node_keys|current_activity_key|sync_graph_instance_activity_projections|select_active_run|advance_node|LifecycleRunView|lifecycle_id|ActiveActivityRef|activity_state" crates packages .trellis/tasks
```

```powershell
rg -n "struct LifecycleRun|struct LifecycleRunView|WorkflowGraphInstance|ActivityLifecycleRunState|AgentAssignmentRefDto|ActivityAttemptView" crates/agentdash-domain crates/agentdash-contracts crates/agentdash-application packages/app-web
```

```powershell
rg -n "active_node_keys|lifecycle_id" crates/agentdash-infrastructure/migrations crates/agentdash-infrastructure/src/persistence/postgres
```

```powershell
rg -n "Session.*Runtime|runtime_session|fetchRuntimeTrace|fetchAgentFrameRuntime|frame-runtime|AgentFrameRuntimeView|RuntimeSessionTraceView" crates/agentdash-contracts crates/agentdash-api packages/app-web/src
```

## Validation Context

本任务完成后至少覆盖这些验证入口：

- `cargo test -p agentdash-domain workflow`
- `cargo test -p agentdash-application workflow::lifecycle_run_view_builder`
- `cargo test -p agentdash-application workflow::tools::advance_node`
- `pnpm run contracts:check`
- `pnpm --filter app-web test`

若实现触及 migrations 或 persistence，还需要补充 repository focused test 或实际 migration check。
