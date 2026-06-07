# Plan-driven runtime cleanup

## 目标

清理 Dynamic Workflow / Lifecycle runtime 链路中残留的 graph/procedure 仓储耦合，让运行时 launch、projection 与 hook snapshot 优先消费 `LifecycleRun.orchestrations[]` 内的 `OrchestrationPlanSnapshot`、`PlanNode` 与 runtime node coordinate，而不是回头从 `WorkflowGraph` / `AgentProcedure` 当前仓储状态重建执行事实。

## 背景

上一轮已移除 `AgentFrame.procedure_id` 以及相关 DTO / migration 残留，确认 `AgentFrame` 只表达 runtime surface revision。继续 review 后发现同类问题仍存在于 lifecycle node compose 与 ActiveWorkflowProjection：这些路径虽然已经拿到 `orchestration_id + node_path + attempt` 和 `plan_snapshot`，但仍要求 `LifecycleRun.root_graph_id` 并查询 `WorkflowGraphRepository` / `AgentProcedureRepository`。这会阻断 dynamic script / inline / run artifact orchestration 的真实 runtime launch，也可能让静态 graph 在定义变更后观察到和启动时 plan 不一致的合同。

## 需求

- lifecycle node compose 必须从 `RuntimeSessionExecutionAnchor -> LifecycleRun -> OrchestrationInstance -> PlanNode` 解析当前节点执行合同。
- `LifecycleNodeSpec` / activity activation 输入应支持 plan-driven 节点，不再强制要求 `WorkflowGraph` 与 `ActivityDefinition`。
- `ActiveWorkflowProjection` / hook snapshot 应从 `OrchestrationPlanSnapshot` 与 runtime node state 派生，避免依赖 `LifecycleRun.root_graph_id`。
- `ExecutorSpec::AgentProcedure` 应支持静态 procedure key 和未来动态 inline/snapshot procedure contract，不能只能表达仓储 key。
- 当前仍需要支持静态 `WorkflowGraph` 已有路径；但静态路径也应把 graph 编译后的 plan snapshot 作为 runtime 事实源。
- `LifecycleRun.root_graph_id` 应从 domain/repository/schema/contracts/frontend projection 中移除；静态 graph provenance 由 `OrchestrationSourceRef::WorkflowGraph` 与 plan metadata 表达。
- 当前项目未上线，fresh migration 直接收敛到目标 schema，不保留先建后删的兼容字段。
- 前后端 contract / generated TS 必须同步。

## 非目标

- 不重写完整 scheduler / reducer。
- 不实现 dynamic workflow script 的完整产品入口 UI。
- 不保留旧 API/字段兼容方案；当前项目未上线，直接收敛到正确模型。
- 不处理 `.trellis/config.yaml` 的既有用户修改。

## 验收标准

- [x] `composer_lifecycle_node` 不再通过 `run.root_graph_id` 查 `WorkflowGraph` 来构造 lifecycle node frame。
- [x] `ActiveWorkflowProjection` 不再要求 `run.root_graph_id` 才能返回 workflow projection。
- [x] `LifecycleRun.root_graph_id` 与 `LifecycleRunRepository::list_by_root_graph` 已移除。
- [x] `ExecutorSpec` 能表达仓储 procedure key 与 inline/snapshot agent procedure contract。
- [x] 动态/inline source 的 orchestration plan 能被 compose/projection 链路识别，不因缺少 `root_graph_id` 直接失败。
- [x] 静态 graph 相关既有测试继续通过。
- [x] 新增/更新测试覆盖 plan-driven compose/projection 与 inline agent executor contract。
- [x] `cargo fmt`、相关 Rust 测试、`cargo check -p agentdash-api`、`pnpm run contracts:check`、`pnpm run frontend:check` 通过。

## 风险文件

- `crates/agentdash-domain/src/workflow/value_objects/orchestration.rs`
- `crates/agentdash-application/src/workflow/frame_construction/composer_lifecycle_node.rs`
- `crates/agentdash-application/src/session/assembler.rs`
- `crates/agentdash-application/src/workflow/projection.rs`
- `crates/agentdash-application/src/workflow/orchestration/compiler.rs`
- `crates/agentdash-application/src/workflow/orchestration/script_compiler.rs`
- `crates/agentdash-contracts/src/workflow.rs`
- `packages/app-web/src/generated/workflow-contracts.ts`
