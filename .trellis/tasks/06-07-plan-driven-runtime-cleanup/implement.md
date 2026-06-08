# Plan-driven runtime cleanup 实施计划

## 阶段 1：执行合同类型

- [x] 在 domain 中新增 `AgentProcedureExecutionSpec`。
- [x] 更新 `ExecutorSpec::AgentProcedure` 字段。
- [x] 更新 static graph compiler 与 script compiler 产物。
- [x] 更新 serde / plan digest 相关测试。

## 阶段 2：plan-driven compose

- [x] 在 application 中增加从 `PlanNode` 构造 activation 所需 activity shape 的 adapter。
- [x] 调整 `LifecycleNodeSpec`，移除 `WorkflowGraph` / `AgentProcedure` 必需依赖。
- [x] 修改 `composer_lifecycle_node`：先定位 orchestration/runtime node/plan node，再按 `ExecutorSpec` 解析 contract；只有 `ByKey` 需要查 `AgentProcedureRepository`。
- [x] 添加 snapshot executor 不查 `AgentProcedureRepository` 的 executor launcher 回归测试。

## 阶段 3：plan-driven projection

- [x] 修改 `ActiveWorkflowProjection` 字段与 `active_contract()`。
- [x] 修改 `active_workflow_projection_from_runtime_node`，不再从 `run.root_graph_id` 查 graph。
- [x] 只有 `ByKey` executor 才查 `AgentProcedureRepository`，snapshot executor 直接使用 contract。
- [x] 添加 projection 回归测试。

## 阶段 3.5：字段与仓储清理

- [x] 移除 `AgentFrame` / DTO / repository / generated TS 中的 `procedure_id`。
- [x] 从初始 migration 删除 `agent_frames.procedure_id`，不保留先建后删的兼容迁移。
- [x] 从 active workflow projection / hook snapshot builder 移除 `WorkflowGraphRepository` 参数。
- [x] 让 script `agent` 在无 `procedure` 但有 `prompt` 时编译为 inline snapshot contract。
- [x] 移除 `LifecycleRun.root_graph_id`、`LifecycleRunRepository::list_by_root_graph`、fresh migration 字段与 frontend/contracts view 字段。
- [x] 将静态 graph provenance 收敛到 `OrchestrationSourceRef::WorkflowGraph` 与 plan metadata。

## 阶段 4：contracts / validation

- [x] 运行 `cargo fmt`。
- [x] 运行 `cargo test -p agentdash-application workflow::projection`。
- [x] 运行 `cargo test -p agentdash-application workflow::orchestration::script_compiler`。
- [x] 运行 `cargo test -p agentdash-application workflow::orchestration::executor_launcher`。
- [x] 运行 `cargo test -p agentdash-application workflow::activity_activation`。
- [x] 运行 `cargo check -p agentdash-api`。
- [x] 运行 `cargo check -p agentdash-infrastructure`。
- [x] 运行 `pnpm run contracts:check`。
- [x] 运行 `pnpm run migration:guard`。
- [x] 运行 `pnpm run frontend:check`。
- [x] 运行 `pnpm --filter app-web test -- lifecycle`。
- [x] 运行 `pnpm --filter app-web run lint`。
- [x] 运行 `pnpm --filter app-web test -- ContextOverviewTab.projection SessionPage.hook-runtime agent-tab-view`。
- [x] residual search：`rg "root_graph_id|root_graph|list_by_root_graph|by_root_graph" crates packages .trellis/spec` 无结果。

## 退出条件

如果阶段 2 发现 `compose_lifecycle_node_to_frame_with_audit` 需要大规模重写 session assembler，本任务先完成合同扩展与 projection 清理，并把 compose 重构拆成后续子任务；不能用临时 graph fallback 掩盖动态 script 缺口。
