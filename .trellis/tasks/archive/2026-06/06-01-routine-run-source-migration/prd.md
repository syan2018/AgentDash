# Routine Run Source 迁移

## 目标

把 Routine execution 的运行来源接入 dispatch / LifecycleAgent / AgentFrame，让 routine terminal 状态从控制面 projection 派生，而不是从 session-first run lookup 派生。

## 依赖

- 父任务：`06-01-session-lifecycle-control-plane-refactor`
- 依赖：`06-01-lifecycle-dispatch-service`
- 依赖：`06-01-workflow-agent-assignment-migration`

## 蓝图阶段

- 推进：`target-state-blueprint.md` B5 Business Subject Migration。
- 退出贡献：RoutineExecution 成为 source/dispatch truth，并从 lifecycle/agent projection 读取 execution result，而不是读取 session dispatch status。

## 重构模式

- 采用父任务 `target-state-blueprint.md` 中的 breaking-mode 约束。
- 将 routine session strategy 替换为 agent/dispatch policy，即使 routine views 暂时缺少 terminal status。
- 不把 prompt dispatch success 当作 lifecycle completion。

## 需求

- RoutineExecution 保存 dispatch truth：run、agent、frame、subject association。
- Routine 可以选择创建独立 `LifecycleRun`，也可以在既有 run 下追加 `WorkflowGraphInstance`；选择依据是生命周期/上下文/权限边界，不是 graph 复杂度。
- Routine terminal / progress 状态从 LifecycleRun / AgentAssignment projection 派生。
- Routine API 不直接暴露 RuntimeSession 作为 business run identity。

## 交付物

- RoutineExecution source association。
- routine agent reuse / create / per-entity dispatch policy。
- routine terminal/progress projection。
- routine read model 中 RuntimeTrace drill-down。

## 不承担

- 不把 RoutineExecution.status 等同于 agent terminal。
- 不把 RuntimeSession id 暴露为 business run identity。
- 不重新实现 dispatch service。

## 验收标准

- [ ] routine start 走 dispatch service。
- [ ] routine status 可以在没有 `LifecycleRun.session_id` 的情况下正确更新。
- [ ] routine dispatch 结果能说明它创建的是独立 run 还是 same-run graph instance。
- [ ] routine frontend/read model 可展示 runtime trace 入口，但不以 session id 作为 run 主键。
