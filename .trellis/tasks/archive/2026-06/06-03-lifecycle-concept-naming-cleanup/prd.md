# Lifecycle 概念与前端命名清理

## Goal

围绕 Lifecycle 作为项目核心执行控制面的语义，沉淀可供后续开发遵守的架构不变量，并盘点前端命名中 Workflow / Lifecycle / Session / Agent 概念混用的位置，形成可执行的清理计划。

本任务第一阶段优先维护 `.trellis/spec` 和命名清理计划；代码层 rename 仅限低风险、小范围、不会牵动跨层 DTO 或用户路由语义的调整。

## Confirmed Facts

- `LifecycleRun` 是 tracked life process / control ledger，不是单个 `RuntimeSession`，也不是单个 graph run。
- 一个 `LifecycleRun` 可以包含多个 `WorkflowGraphInstance`；Activity / attempt identity 必须包含 `graph_instance_id`。
- Activity 状态推进只能通过 `ActivityEvent -> LifecycleEngine`。
- `AgentAssignment(run_id, graph_instance_id, activity_key, attempt, agent_id, frame_id)` 是 activity attempt 与 agent/frame 的执行桥。
- `AgentFrame` 是 runtime surface revision，承载 capability / context / VFS / MCP / runtime refs 的有效快照。
- `RuntimeSession` 只承载 delivery / trace evidence，不承载 business ownership。
- 前端当前存在两条主要线：`workflowStore` 管 `WorkflowGraph` 定义态，`lifecycleStore` 管运行态 view projection。
- 前端用户视角中“会话列表”由 lifecycle run / agent / runtime session ref 投影而来，不能反向成为 lifecycle control state 的事实源。

## Requirements

- 在 backend workflow/session spec 中补齐或强化 Lifecycle / Agent / RuntimeSession 的边界说明。
- 在 frontend spec 中补齐定义态与运行态 store/view 边界，明确 `workflowStore` 以 `WorkflowGraph` 为主、`lifecycleStore` 观察运行态、Session UI 观察 trace 的职责。
- 盘点前端命名不清晰点，区分立即可清理项与需要计划性重构项。
- 输出概念清理计划，说明命名清理的顺序、风险和验收方式。
- 保持文档内容面向长期架构收敛，只记录“为什么这样分层”，不记录当前任务过程或旧实现失误。

## Acceptance Criteria

- [ ] `.trellis/spec` 中存在清晰的 Lifecycle 核心词汇和不变量说明。
- [ ] frontend spec 明确区分定义态资产、运行态 lifecycle projection、RuntimeSession trace UI。
- [ ] task research 中记录前端命名盘点结果，并按风险分类。
- [ ] task design / implement 记录后续命名清理计划，包含哪些改、哪些暂缓、原因是什么。
- [ ] 本轮如做代码改名，只限不改变 API/DTO/路由/行为的小范围命名或注释清理。
- [ ] 变更通过合适的轻量检查，至少确认涉及文档和前端命名引用没有明显断裂。

## Out Of Scope

- 大规模跨层 DTO rename。
- API route rename。
- 数据库字段 rename 或 migration。
- 用户可见路由、菜单、业务文案的整体重命名。
- 改变 Lifecycle / Session / Agent 运行行为。
