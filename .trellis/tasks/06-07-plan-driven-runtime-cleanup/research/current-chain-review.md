# 当前链路 review

## 已确认的正确边界

- `RuntimeSessionExecutionAnchor` 保存 `run_id`、`agent_id`、`launch_frame_id`、`orchestration_id`、`node_path`、`attempt` 是合理的 trace evidence，不是定义仓储耦合。
- `AgentFrame` 已清理为 runtime surface revision，不再保存 `procedure_id`。
- `LifecycleRun.orchestrations[]` 与 `OrchestrationPlanSnapshot` 是 runtime contract 事实源。

## 已清理的链路

1. `composer_lifecycle_node`
   - 位置：`crates/agentdash-application/src/workflow/frame_construction/composer_lifecycle_node.rs`
   - 结果：从 anchor 定位 `OrchestrationInstance` / `PlanNode`，再从 plan node 投影 activation activity shape。
   - contract：`ByKey` 才查 procedure 仓储；`Snapshot` 直接使用 plan 内 contract。

2. `ActiveWorkflowProjection`
   - 位置：`crates/agentdash-application/src/workflow/projection.rs`
   - 结果：从 `OrchestrationInstance.plan_snapshot` 和 runtime node state 派生 projection。
   - contract：`active_contract()` 优先返回 snapshot contract，否则返回 ByKey 查到的 procedure contract。

3. `ExecutorSpec::AgentProcedure`
   - 位置：`crates/agentdash-domain/src/workflow/value_objects/orchestration.rs`
   - 结果：支持 `ByKey` 与 `Snapshot` 两类执行合同；script inline agent 使用 snapshot contract。

4. `LifecycleRun.root_graph_id`
   - 问题：作为 run-level 单 graph 字段容易误导 runtime 继续依赖 graph 资产。
   - 结果：domain / repository / schema / contracts / frontend projection 已移除该字段；静态 graph provenance 由 orchestration `source_ref` 与 plan metadata 表达。

5. `AgentFrame.procedure_id`
   - 结果：domain / DTO / repository / generated TS / fresh migration schema 均已移除。

## 次级风险

- `visible_canvas_mount_ids_json` 是 frame 上的可变 UI 状态，与 frame revision immutability 有张力；本任务不处理。
