# Frontend Actor Subject Views

## 目标

将前端从 session-first run grouping 迁到 subject / agent / lifecycle / runtime trace 视图，让用户仍能进入运行详情，但 UI 不把 Session 当作业务控制面主轴。

## 依赖

- 父任务：`06-01-session-lifecycle-control-plane-refactor`
- 依赖：`06-01-lifecycle-dispatch-service`
- 依赖：`06-01-task-subject-execution-migration`
- 依赖：`06-01-workflow-agent-assignment-migration`

## 蓝图阶段

- 推进：`target-state-blueprint.md` B6 Contracts And Frontend Views。
- 退出贡献：frontend 通过 generated target views 读取 lifecycle graph instances、subjects、agents、frames、runtime traces，而不是重建 session owner tree。

## 重构模式

- 采用父任务 `target-state-blueprint.md` 中的 breaking-mode 约束。
- 移除作为产品根的 session tree state 和 routes，即使部分页面暂时丢导航。
- 不保留 `runsBySessionId` 作为 compatibility store。

## 需求

- 引入或更新 `LifecycleRunView`、`SubjectExecutionView`、`AgentFrameRuntimeView`、`ProjectActiveAgentsView`。
- `LifecycleRunView` 必须展示多个 `WorkflowGraphInstance` 的状态，而不是把 run 视为单个 WorkflowRun。
- `/session/:id` 降级为 `RuntimeTraceView`。
- `WorkflowRun.session_id` 不再作为 lifecycle 主索引；前端 state 改为 run/subject/agent indexes。
- Project / Story / Task 页面显示 active agents / subject executions，而不是 session tree owner。

## 交付物

- `LifecycleRunView`、`SubjectExecutionView`、`AgentFrameRuntimeView`、`ProjectActiveAgentsView` generated type 使用路径。
- run / graph instance / subject / agent / frame normalized stores。
- `/session/:id` RuntimeTrace route。
- Project / Story / Task 页面从 subject/agent view 进入 runtime trace。

## 不承担

- 不把 RuntimeTraceView 作为 command input。
- 不保留 `runsBySessionId` 作为 lifecycle 主 store。
- 不把 nullable `session_id` 当业务主键。

## 验收标准

- [ ] `runsBySessionId` 不再是 workflow run 主 store。
- [ ] 前端 state 支持 `run -> workflowGraphInstances -> activities/attempts`。
- [ ] Project / Story / Task 页面可从 subject view 进入 agent view 与 runtime trace view。
- [ ] generated contracts 中 nullable `session_id` 不被前端当作必填业务主键。
- [ ] UI 仍支持 debug trace、lineage、projection 面板，但语义标为 RuntimeTrace。
