# Graphless 默认 Agent Runtime

## Goal

普通 Agent 会话不再伪装成 `builtin.freeform_session` 工作流。没有显式 lifecycle 的 ProjectAgent、Story、Task、Routine、Companion 默认入口应直接创建 graphless runtime 控制面，只记录 run / agent / frame / runtime session 锚点；`WorkflowGraph`、`WorkflowGraphInstance`、`AgentAssignment` 只在显式 Activity 工作流中出现。

## User Value

- 用户不会在 Workflow 列表、Agent 配置或运行观察中看到不明所以的 freeform 工作流。
- 大多数不涉及 Activity 流转的 Agent 以更轻的控制面启动，避免为了复用 Activity engine 制造假 graph / 假 assignment。
- Activity 工作流能力保持为显式高级能力，不影响普通 Agent 的心智模型。

## Confirmed Decisions

- 默认运行形态：Graphless。
- 覆盖范围：所有默认 freeform 入口，包括 ProjectAgent、Story、Task、Routine、Companion。
- 数据策略：项目未上线，不做旧数据兼容；已有 freeform seed / 历史开发数据由开发者手动清理。
- Run schema：增加 `LifecycleRunTopology`，wire / DB 值为 `graphless` 与 `workflow_graph`；graphless 下 `root_graph_id` 为空，workflow graph 下 `root_graph_id` 必须存在。
- Result refs：`assignment_ref` 变为 Activity-only optional；graphless 结果只保证 run / agent / frame / runtime refs。
- ProjectAgent `default_procedure_key`：删除入口；`AgentProcedure` 只服务显式 WorkflowGraph 的 Activity executor contract。
- Migration 策略：按预研期规范收敛当前 schema baseline；同步更新 `0001_init.sql`、repository SQL 和 generated contracts，不保留旧字段兼容。

## Requirements

- 没有显式 `default_lifecycle_key` 的 ProjectAgent launch 不查询、不创建、不依赖 `builtin.freeform_session`。
- Story / Task / Routine / Companion 默认入口不再构造 `WorkflowGraphRef::ByKey { key: builtin.freeform_session }`。
- Graphless dispatch 创建 `LifecycleRun(topology=graphless)`、`LifecycleAgent`、`AgentFrame`、`RuntimeSessionExecutionAnchor` 和必要的 `LifecycleSubjectAssociation`。
- Graphless dispatch 不创建 `WorkflowGraphInstance`、不初始化 `ActivityLifecycleRunState`、不创建 `AgentAssignment`。
- 显式 lifecycle dispatch 继续走现有 Activity graph 路径，并创建 graph instance、activity state、assignment。
- `LifecycleRun`、run view、subject execution view、frontend store / mapper 能表示 graphless run 且不假设 graph instance 存在。
- Task start / continue / cancel、Routine execution refs、Subject dispatch result 等跨层 DTO 将 `assignment_ref` 调整为 optional；Task cancel 的 `graph_instance_ref` 也调整为 optional。
- Task / Routine 的 continue / reuse / cancel 在 graphless 场景通过 subject association、agent current frame 和 runtime session refs 定位控制面；只有显式 Activity run 才需要 assignment。
- ProjectAgent create / update request、contract DTO、frontend form/store 删除 `default_procedure_key`。
- 删除 `default_procedure_key -> auto:{procedure}` WorkflowGraph 自动包装逻辑。
- 删除或停止调用 `FreeformLifecycleService::ensure_definition`，Project 创建与启动对账不再 seed freeform graph / procedure。
- `AgentProcedure` 保留为显式 WorkflowGraph Activity executor contract；普通 ProjectAgent runtime 不直接绑定 procedure。

## Acceptance Criteria

- [ ] ProjectAgent launch 无 `default_lifecycle_key` 时成功创建 graphless run，数据库中没有新增 `workflow_graphs` / `lifecycle_workflow_instances` / `agent_assignments` 行。
- [ ] ProjectAgent launch 无 `default_lifecycle_key` 时返回 `run_ref`、`agent_ref`、`frame_ref`、可选 `delivery_runtime_ref`，且 `assignment_ref` 为空。
- [ ] Story、Task、Routine、Companion 默认入口均不再引用 `builtin.freeform_session`。
- [ ] Task start / continue / cancel 在 graphless 默认路径成功返回 optional graph / assignment refs，并可继续定位已有 run / agent / frame。
- [ ] Routine Fresh / Reuse / PerEntity 默认路径能记录 run / agent / frame refs，assignment 为空时不失败且能复用目标。
- [ ] 显式 lifecycle ProjectAgent / Task / Routine 路径仍创建 Activity graph instance 和 assignment，既有 Activity 流转测试通过。
- [ ] ProjectAgent create / update API 不再接受 `default_procedure_key`，前端不再发送或展示该字段。
- [ ] 创建或克隆 Project 不再 seed `builtin.freeform_session` / `builtin.freeform_agent`。
- [ ] LifecycleRun view / frontend lifecycle stores 能处理 `topology=graphless`、`root_graph_id=null`、`workflow_graph_instances=[]`。
- [ ] Generated TypeScript contracts 与 Rust DTO 保持同步，`pnpm run contracts:check` 通过。

## Out Of Scope

- 不迁移或回填历史开发数据。
- 不提供旧 API 字段兼容。
- 不把 `AgentProcedure` 改造成普通 Agent preset/template。
- 不重做 WorkflowGraph 编辑器或 Activity DAG 设计。

## Open Questions

- 无。当前任务可以进入设计与实施计划。
