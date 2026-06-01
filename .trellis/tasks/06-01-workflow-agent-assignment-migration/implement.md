# 执行计划

## 顺序

1. 定位 scheduler / orchestrator / executor launcher / terminal callback 当前 session-first route。
2. 将 activity state、claim、attempt 读写切到 `graph_instance_id + activity_key + attempt`。
3. 在 Activity attempt 执行前或同时创建 `AgentAssignment`。
4. Agent executor launch 返回 `assignment_ref`、`agent_ref`、`frame_ref`、`runtime_session_ref`。
5. 改造 terminal callback：`RuntimeSession -> AgentFrame -> LifecycleAgent -> AgentAssignment -> ActivityAttemptState`。
6. 改造 `complete_lifecycle_node`、terminal effect、VFS lifecycle provider、hook advance/resolution。
7. 新写入 execution log 使用 activity vocabulary。
8. 增加同一 run 多 graph instance、同名 activity key 的隔离测试。

## 质量门

- Activity attempt 可以通过 `AgentAssignment` 找到 agent/frame/runtime refs。
- 同一 `LifecycleRun` 内不同 `WorkflowGraphInstance` 的同名 activity 不污染 assignment、claim、attempt。
- terminal callback 不直接查 `LifecycleRun.session_id`。
- `LifecycleExecutionEntry.step_key` / `StepActivated` / `StepCompleted` 的新写入路径被替换。
- scheduler/orchestrator 覆盖 session-less routing。

## 断裂点

部分 executor path 在提供 graph instance 与 assignment refs 前可以暂时失败。失败点必须指向缺失 refs，而不是恢复 session-first fallback。

## 验证命令

- `python ./.trellis/scripts/task.py validate 06-01-workflow-agent-assignment-migration`
- scheduler/orchestrator tests。
- terminal callback tests。
- `rg -n "list_by_session|LifecycleRun\\.session_id|step_key|StepActivated|StepCompleted" crates/agentdash-application crates/agentdash-domain crates/agentdash-infrastructure`
- `git diff --check -- .trellis/tasks`

## 后续交接

- `task-subject-execution-migration` 使用 assignment projection 生成 Task view。
- `companion-gate-lineage-migration` 使用 assignment + gate 恢复 parent/child agent flow。
- `session-first-api-demotion` 删除最后的 session-first API / DTO 暴露。
