# Session Lifecycle Spec 收敛

## 目标

把 `.trellis/spec/` 中仍然保留的 Session-first、Binding、Step 旧表述同步为父任务批准的目标事实源，让后续实现先读到同一套架构合同。

## 依赖

- 父任务：`06-01-session-lifecycle-control-plane-refactor`
- 必须遵循父任务 `design.md` ownership 与 P0 决策。

## 蓝图阶段

- 推进：`target-state-blueprint.md` B0 Vocabulary And Contract Freeze。
- 退出贡献：specs 明确 `LifecycleRun` 是 multi-`WorkflowGraphInstance` container，`WorkflowGraph` 是当前 `ActivityLifecycleDefinition` 的目标，`AgentProcedure` 是当前 `WorkflowDefinition` 的目标，`RuntimeSession` 是 trace substrate。

## 重构模式

- 采用父任务 `target-state-blueprint.md` 中的 breaking-mode 约束。
- 不为旧术语保留兼容表述；spec 直接写目标概念，旧名称只作为迁移来源出现。

## 需求

- 更新 backend / frontend / cross-layer specs 中与 Session、LifecycleRun、WorkflowGraph、Task、Activity、Hook runtime、LifecycleRunLink 相关的冲突表述。
- 将 `Session` 统一改写为 `RuntimeSession` 语义：运行轨迹容器，不拥有业务归属。
- 将 `LifecycleRun` 明确为可容纳多个 `WorkflowGraphInstance` 的生命周期容器；当前 `LifecycleRun.lifecycle_id` 只是 root graph 迁移来源。
- 将 `Workflow` 的 graph 语义在 spec 中写为 `WorkflowGraph`；产品侧可简称 Workflow，但代码/contract 不得与 `AgentProcedure` 混用。
- 将 `LifecycleRunLink` 的目标方向写成 `LifecycleSubjectAssociation(anchor_run_id, anchor_agent_id?)`。
- 将 `LifecycleRun.session_id`、Task runtime 字段、`SessionBinding*`、step vocabulary 标为迁出对象。
- 明确 `AgentFrame` revision row、`LifecycleAgent`、`AgentAssignment`、`LifecycleGate` 的目标边界。

## 交付物

- 更新后的 `.trellis/spec/` 文档。
- 一组可被后续任务引用的稳定术语：`RuntimeSession`、`LifecycleRun`、`WorkflowGraph`、`WorkflowGraphInstance`、`AgentProcedure`、`LifecycleAgent`、`AgentFrame`、`AgentAssignment`、`LifecycleSubjectAssociation`、`LifecycleGate`。
- 明确标出旧结构只是迁移来源，不是新实现入口。

## 不承担

- 不新增 schema、repository、API 或 frontend contract。
- 不实现任何 runtime path。

## 验收标准

- [ ] `.trellis/spec/project-overview.md` 不再描述 Story durable session / LifecycleRun 1:1 Story session。
- [ ] `.trellis/spec/backend/story-task-runtime.md` 明确 Task 不拥有 runtime truth。
- [ ] `.trellis/spec/backend/session/architecture.md` 明确 Session demotion 为 RuntimeSession。
- [ ] `.trellis/spec/backend/workflow/activity-lifecycle.md` 明确同一 LifecycleRun 内可以有多个 WorkflowGraphInstance。
- [ ] `.trellis/spec/backend/session/session-startup-pipeline.md` 不再保留 freeform `SessionBinding(owner_type=Project)` 作为新入口。
- [ ] `.trellis/spec/backend/workflow/lifecycle-run-link.md` 更新为 actor-aware association。
- [ ] `.trellis/spec/frontend/workflow-activity-lifecycle.md` 不再要求前端以 `session_id` 或单 graph id 作为 run 主索引。
