# WorkflowGraph Agent Assignment 设计

## 目标

本任务把 Activity 执行身份从 session-first route 迁到 `AgentAssignment`。完成后，scheduler、orchestrator、terminal callback、VFS lifecycle provider、`complete_lifecycle_node` 都不需要通过 `LifecycleRun.session_id` 或 `list_by_session` 找 lifecycle progress。

## 执行身份模型

```text
WorkflowGraphInstance
  -> ActivityState(activity_key)
  -> ActivityAttemptState(graph_instance_id, activity_key, attempt)
  -> AgentAssignment(run_id, graph_instance_id, activity_key, attempt, agent_id, frame_id)
  -> LifecycleAgent
  -> AgentFrame
  -> RuntimeSession refs
```

`ActivityAttemptState` 记录 terminal evidence；`AgentAssignment` 记录谁用哪个 frame 承担该 attempt；`RuntimeSession` 只提供 runtime evidence。

## Key 与唯一性

- Activity runtime key 必须包含 `graph_instance_id + activity_key`。
- Attempt key 必须包含 `graph_instance_id + activity_key + attempt`。
- Claim key 必须包含 `graph_instance_id + activity_key + attempt`。
- Assignment 必须引用同一个 run 下的 `LifecycleAgent` 与 `AgentFrame`。
- 同一 `LifecycleRun` 内不同 graph instance 可以拥有同名 activity key，不得污染 state、claim、attempt、assignment。

## Scheduler / Orchestrator 改造

1. Scheduler 选择 Activity attempt 时，必须已有或创建 `AgentAssignment`。
2. Agent executor launch 返回 `assignment_ref`、`agent_ref`、`frame_ref`、`runtime_session_ref`。
3. `ExecutorRunRef::AgentSession` 降级为 attempt evidence，不再作为 routing root。
4. `ActivityExecutionClaim` 与 `AgentAssignment` 建立明确对应关系。
5. lifecycle execution log 新写入使用 activity vocabulary，不再新增 step vocabulary。

## Terminal callback 改造

标准路径：

```text
RuntimeSession terminal
  -> AgentFrameRepository.find_by_runtime_session(runtime_session_id)
  -> LifecycleAgent
  -> AgentAssignmentRepository.find_by_agent/frame/runtime evidence
  -> ActivityAttemptState
  -> lifecycle advancement
```

如果同一个 frame 有多个 runtime refs 或多个 active assignments，callback 必须依赖 runtime ref provenance、assignment lease 或 terminal payload 中的 assignment ref 消歧；不能回退到 `LifecycleRun.session_id`。

## Tool / VFS / Hook 改造

- `complete_lifecycle_node` 使用 assignment / graph instance refs 完成 Activity。
- VFS lifecycle provider 从 agent/frame/assignment 推导 lifecycle artifact scope。
- Hook advance/resolution 使用 activity vocabulary 与 assignment refs。
- session-indexed path 只可作为 trace adapter，必须立即反查 frame/agent/assignment。

## 不承担

- 本任务不迁 Task / Companion / Routine 的业务入口。
- 本任务不决定 frame surface 如何生成。
- 本任务不删除全部 legacy API；最终删除由 `session-first-api-demotion` 收尾。
