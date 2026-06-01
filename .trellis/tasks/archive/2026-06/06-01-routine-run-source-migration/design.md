# Routine Run Source 迁移设计

## 目标

把 RoutineExecution 的运行来源接入 `LifecycleDispatchService` / `LifecycleAgent` / `AgentFrame`，让 routine terminal 状态从控制面 projection 派生，而不是从 session-first run lookup 或 dispatch success 推导。

## 蓝图阶段

推进 `target-state-blueprint.md` B5 Business Subject Migration。

## 存量结构分析

### Routine 域当前结构

| 存量结构 | 当前作用 | 问题 |
| --- | --- | --- |
| `RoutineExecutor.fire()` | 统一触发入口：load routine → render prompt → resolve session → launch | 直接构造 session 并通过 session launch 发送 prompt |
| `RoutineExecutor.execute_with_session()` | resolve session_id → 构造 `LaunchCommand::routine_executor_input()` → launch → mark_completed | session 作为 routine 业务 identity |
| `RoutineExecutor.resolve_session_id()` | 根据 `SessionStrategy` 创建或复用 session | session strategy 直接决定 session 创建/复用 |
| `RoutineExecution.session_id` | 记录执行使用的 session | session_id 作为 execution truth |
| `RoutineExecution.status` | pending → running → completed/failed/skipped | `mark_completed()` 代表 prompt 已派发，不代表 agent 执行完毕 |
| `RoutineLaunchSource` | 标记 routine 启动来源 | 通过 LaunchCommand 传递，session-first |
| `SessionStrategy` | create_new / reuse_latest / per_entity | session 复用策略 |
| `Routine.project_agent_id` | 指向 ProjectAgent 配置 | 正确，但 dispatch 应通过 intent 传递 |

### 目标链路

```text
Routine fire:
  RoutineExecution record
    -> ExecutionIntent(source=routine, subject_ref=SubjectRef(kind=RoutineExecution, id=execution_id),
         run_policy=from_session_strategy, agent_policy=from_session_strategy)
    -> LifecycleDispatchService.dispatch()
    -> ExecutionDispatchResult(run_ref, agent_ref, frame_ref, runtime_session_ref)
    -> RoutineExecution.dispatch_refs = { run_id, agent_id, frame_id }

Routine terminal status:
  RoutineExecution.dispatch_refs
    -> LifecycleAgent status / LifecycleRun terminal
    -> RoutineExecutionProjection(terminal_status, terminal_summary)
```

## 迁移决策表

| 存量 | 决策 | 目标 |
| --- | --- | --- |
| `RoutineExecutor.fire()` | 保留 admission / template / agent context 逻辑；session 分发替换为 dispatch intent | dispatch service 创建 run/agent/frame/session |
| `RoutineExecutor.execute_with_session()` | 替换为 `execute_with_dispatch()`：构造 intent → dispatch → 记录 refs | 不再直接操作 session |
| `RoutineExecutor.resolve_session_id()` | 删除；session 创建由 dispatch service / AgentFrame builder 负责 | |
| `SessionStrategy` → dispatch policy | `create_new` → `run_policy=create_linked_run, agent_policy=create`；`reuse_latest` → `run_policy=reuse_existing, agent_policy=resume`；`per_entity` → `run_policy=reuse_existing` + entity_key 作为 run lookup key | `ExecutionIntent.run_policy` + `agent_policy` |
| `RoutineExecution.session_id` | 替换为 `RoutineExecution.dispatch_refs { run_id, agent_id, frame_id }` | session_id 不暴露为 business identity |
| `RoutineExecution.status.mark_completed()` | 改为 `mark_dispatched()`；真正 terminal 从 lifecycle projection 派生 | dispatch success ≠ execution completion |
| `RoutineLaunchSource` | 替换为 `ExecutionIntent.source=routine` + `ExecutionIntent.metadata` | dispatch service 接收 |
| `LaunchCommand::routine_executor_input()` | 删除；prompt 通过 `ExecutionIntent.initial_prompt` 或 frame builder 注入 | |
| `Routine.project_agent_id` | 保留；映射为 `ExecutionIntent.agent_procedure_ref` 或 profile hint | dispatch service 解析 agent profile |

## Dispatch 策略映射

| SessionStrategy | run_policy | agent_policy | 说明 |
| --- | --- | --- | --- |
| `create_new` | `create_linked_run` | `create` | 每次 fire 创建独立 run 和 agent |
| `reuse_latest` | `reuse_existing` | `resume` | 复用最近的 active run/agent |
| `per_entity` | `reuse_existing` + entity_key | `resume` / `create` | 按 entity_key 查找或创建 run/agent |

## 不变量

- `RoutineExecution.status` 的 `completed` 不等于 agent 执行完毕；dispatch 成功只代表 intent 已处理。
- Routine terminal 从 `LifecycleAgent` / `LifecycleRun` status projection 派生。
- Routine 不暴露 `RuntimeSession id` 作为 business run identity。
- Routine dispatch 结果记录 run/agent/frame refs，不记录裸 session_id。
- `SessionStrategy` 映射为 dispatch policy，不再作为 session 复用直接操作。

## 断裂点

- Routine API 返回结构变化（`session_id` → dispatch refs）。
- Routine execution list 页面在 `frontend-actor-subject-views` 中适配新 view。
- `mark_completed` 语义变化：旧代码依赖 `completed` 表示"全部完成"的地方需要适配 projection。
