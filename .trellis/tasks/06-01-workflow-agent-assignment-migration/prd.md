# WorkflowGraph Agent Assignment 迁移

## 目标

把 scheduler / orchestrator / terminal callback 从 session-first execution record 迁到 `AgentAssignment`，让 Agent 身份、frame revision、runtime evidence 与 ActivityAttemptState 分层。

## 依赖

- 父任务：`06-01-session-lifecycle-control-plane-refactor`
- 依赖：`06-01-session-lifecycle-target-anchors-schema`
- 依赖：`06-01-lifecycle-dispatch-service`
- 依赖：`06-01-agent-frame-construction-migration`

## 蓝图阶段

- 推进：`target-state-blueprint.md` B4 Activity Execution And Assignment。
- 退出贡献：activity execution identity 是 graph-instance-scoped assignment，RuntimeSession 只作为 terminal/runtime evidence。

## 重构模式

- 采用父任务 `target-state-blueprint.md` 中的 breaking-mode 约束。
- 删除 session-first terminal/advance routing，而不是保留 fallback。
- 部分 executor path 可以在提供 graph instance 与 assignment refs 前暂时失败。

## 需求

- 创建 Activity execution 时同步建立或引用 `LifecycleAgent`、`AgentFrame`、`AgentAssignment`。
- `AgentAssignment` 必须引用 `graph_instance_id + activity_key + attempt`，避免同一 LifecycleRun 内多个 WorkflowGraph 的 activity key 冲突。
- `ActivityAttemptState` 保留 status、executor terminal、artifact evidence；不保存 subject anchor。
- `ExecutorRunRef::AgentSession` 降级为 runtime evidence。
- `complete_lifecycle_node`、terminal effect、VFS lifecycle provider 不再以 session id 作为 primary route。
- lifecycle execution log 将 step vocabulary 迁为 activity vocabulary。

## 交付物

- scheduler / orchestrator 的 assignment route。
- terminal callback 的 `RuntimeSession -> AgentFrame -> LifecycleAgent -> AgentAssignment -> ActivityAttemptState` 路径。
- claim / assignment / attempt 的 graph-instance-scoped key。
- activity vocabulary 的新 execution log 写入路径。
- `design.md` 与 `implement.md` 中声明的 session-less routing。

## 不承担

- 不迁 Task / Companion / Routine 业务入口。
- 不决定 AgentFrame surface 如何生成。
- 不删除最终 legacy API。

## 验收标准

- [ ] Activity attempt 可以通过 AgentAssignment 找到 agent/frame/runtime refs。
- [ ] 同一 LifecycleRun 内不同 WorkflowGraphInstance 的同名 activity 不会互相污染 assignment、claim、attempt。
- [ ] terminal callback 能从 runtime session 追溯到 frame/agent/run 后完成 Activity，而不是直接查 `LifecycleRun.session_id`。
- [ ] `LifecycleExecutionEntry.step_key` / `StepActivated` / `StepCompleted` 的新写入路径已替换为 activity vocabulary。
- [ ] scheduler/orchestrator 测试覆盖 session-less routing。
