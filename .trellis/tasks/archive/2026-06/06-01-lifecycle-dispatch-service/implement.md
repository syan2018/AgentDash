# 执行计划

## 顺序

1. 定义 `ExecutionIntent`、`ExecutionDispatchResult` 与 refs DTO。
2. 实现 same-run / linked-run 判定函数，并用单元测试覆盖边界。
3. 接入目标 anchor repositories：
   - `LifecycleRun`
   - `WorkflowGraphInstance`
   - `LifecycleAgent`
   - `AgentFrame`
   - `LifecycleSubjectAssociation`
   - `LifecycleGate`
   - `AgentLineage`
4. 调用 AgentFrame builder，但不让 dispatch service 直接组装 frame internals。
5. 将 ProjectAgent open 改为通过 dispatch service。
6. 返回 target refs，删除该入口上的 `binding_id` / `owner_type` / `owner_id` 控制面主字段。
7. 记录 Task / Companion / Routine 的后续接入断点。

## 质量门

- ProjectAgent open 不再直接构造 session owner / binding。
- same-run dispatch 能向既有 `LifecycleRun` 追加 `WorkflowGraphInstance`。
- linked-run 只在 lifecycle/context/control/navigation/projection 边界成立时发生。
- `ExecutionDispatchResult` 足够前端进入 subject view、agent view 或 runtime trace view。
- 业务模块不 import frame construction plan、runtime launch request 或 connector execution schema。

## 验证命令

- `python ./.trellis/scripts/task.py validate 06-01-lifecycle-dispatch-service`
- dispatch service 单元测试。
- ProjectAgent open 路由 / service 测试。
- `rg -n "binding_id|owner_type|owner_id|SessionConstructionPlan|LaunchPlan" crates/agentdash-application crates/agentdash-web`
- `git diff --check -- .trellis/tasks`

## 后续交接

- `agent-frame-construction-migration` 将 dispatch 调用的 frame builder 变成真正事实源。
- `task-subject-execution-migration` 把 Task start/continue 接到 `ExecutionIntent(subject_ref=Task)`。
- `companion-gate-lineage-migration` 用 parent agent + gate policy 走同一 dispatch service。
- `routine-run-source-migration` 用 RoutineExecution source + agent policy 走同一 dispatch service。
