# Task Subject Execution 迁移设计

## 目标

把 Task start/continue/cancel 从 session-owned runtime state 迁到 `SubjectRef(kind=Task)` dispatch。Task entity 只保留业务 spec 与 authoring preference，运行状态全部来自 lifecycle facts 的 projection。

## 蓝图阶段

推进 `target-state-blueprint.md` B5 Business Subject Migration。

## 存量结构分析

### Task 域当前结构

| 存量结构 | 当前作用 | 问题 |
| --- | --- | --- |
| `StoryStepActivationService.start_task()` | 创建 session → bind owner → launch command | 直接创建 session 并 bind，绕过 dispatch service |
| `StoryStepActivationService.continue_task()` | 通过 `resolve_execution_session_id` 查找已有 session → launch | 通过 run link + `LifecycleRun.session_id` 反查 session |
| `StoryStepActivationService.cancel_task()` | 通过 session_id cancel | cancel 路径完全依赖 session_id |
| `resolve_execution_session_id()` | `LifecycleRunLink(kind=Task)` → `LifecycleRun` → `session_id` | session-first run lookup |
| `bind_session_to_owner()` | 标记 session owner bootstrap pending | 残留的 binding 语义 |
| `gw_create_task_session()` | 创建 session 并设置 task 相关 title/meta | session 作为 task 业务容器 |
| `TaskExecutionResult.session_id` | 返回给 API/frontend | session_id 作为 task execution 主 identity |
| `TaskSessionResult.agent_binding` | 从 `Task.agent_binding` 读取 | runtime owner 语义 |
| `Task.lifecycle_step_key` | 指向 workflow step/activity | Task 保存 runtime 位置 truth |
| `Task.status` / `Task.artifacts` | 直接持有状态与产物 | 被当作 primary runtime state |
| `Task.agent_binding` | 希望使用的 agent 配置 | runtime owner 语义 |

### 目标链路

```text
Task start/continue 请求
  -> ExecutionIntent(subject_ref=SubjectRef(kind=Task, id=task_id), run_policy, agent_policy)
  -> LifecycleDispatchService.dispatch()
  -> ExecutionDispatchResult(run_ref, graph_instance_ref, agent_ref, frame_ref, runtime_session_ref)

Task status/artifacts 查询
  -> SubjectRef(kind=Task)
  -> LifecycleSubjectAssociation(anchor_run_id, anchor_agent_id?)
  -> LifecycleAgent -> AgentAssignment -> ActivityAttemptState -> artifacts
  -> TaskProjection(source refs)
```

## 迁移决策表

| 存量 | 决策 | 目标 |
| --- | --- | --- |
| `StoryStepActivationService.start_task()` | 重写为构造 `ExecutionIntent(subject_ref=Task)` 并调用 `LifecycleDispatchService` | dispatch service 创建 run/agent/frame/session |
| `StoryStepActivationService.continue_task()` | 通过 `LifecycleSubjectAssociation` 查找 active run/agent，构造 `ExecutionIntent(run_policy=reuse_existing)` | 不再通过 session_id 反查 |
| `StoryStepActivationService.cancel_task()` | 通过 association 查找 agent → frame → runtime_session_ref 执行 cancel | cancel 通过 agent/frame 路由 |
| `resolve_execution_session_id()` | 删除；替换为 `resolve_task_execution_refs()` 返回 run/agent/frame refs | 不返回裸 session_id |
| `bind_session_to_owner()` | 删除；association 由 dispatch service 创建 | |
| `gw_create_task_session()` | 删除；session 创建由 AgentFrame builder 负责 | |
| `TaskExecutionResult.session_id` | 替换为 `TaskExecutionResult { run_ref, agent_ref, frame_ref, trace_ref? }` | session_id 降级为 optional trace ref |
| `TaskSessionResult` | 替换为 `TaskExecutionView`，包含 projection + source refs | |
| `Task.lifecycle_step_key` | 删除字段；activity 位置从 `LifecycleSubjectAssociation → AgentAssignment` 投影 | `SubjectExecutionView.task_projection` |
| `Task.status` | 保留为 spec-level status（用户可修改）；运行 status 从 projection 读取 | `TaskProjection.execution_status` |
| `Task.artifacts` | 保留为 spec-level artifacts（用户可添加）；执行产物从 attempt outputs 投影 | `TaskProjection.execution_artifacts` |
| `Task.agent_binding` | 改为 `Task.agent_preference`，仅作为 dispatch policy hint | `ExecutionIntent.agent_procedure_ref` |
| `TaskLaunchPhase` | 删除；改为 `ExecutionIntent.run_policy` 区分 create/reuse | |
| `LaunchCommand::task_service_input()` | 删除；frame builder 从 dispatch result 构造 launch | |

## 不变量

- Task entity 永远不保存 Activity 位置 truth（`lifecycle_step_key` 不存在）。
- Task execution 结果中 `session_id` 不作为业务主键，只作为 optional trace ref。
- Task status/artifacts 若持久化为 projection，必须带 source run/agent/activity/attempt revision refs。
- Task continue 不通过 session_id 查找 run；通过 `LifecycleSubjectAssociation` 查找 active agent。
- Task 子图作为 same-run `WorkflowGraphInstance` 存在，不自动创建 child `LifecycleRun`。

## 断裂点

- Task API route（`start_task` / `continue_task` / `cancel_task` / `get_task_session`）在迁移期间可能返回结构变化。
- 前端 Task 页面若依赖 `session_id` 展示执行状态，将在 `frontend-actor-subject-views` 任务中修复。
- `TaskExecutionResult` 结构变化会影响 API response DTO，需同步更新 `agentdash-contracts`。
