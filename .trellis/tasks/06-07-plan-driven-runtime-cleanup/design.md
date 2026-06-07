# Plan-driven runtime cleanup 设计

## 当前问题

runtime 已经有 `OrchestrationPlanSnapshot`，其中保存 `source_ref`、semantic `PlanNode`、executor spec、ports、completion policy 与 activation/state exchange rules。但 launch compose 和 hook projection 仍从 `LifecycleRun.root_graph_id` 回到 `WorkflowGraphRepository` / `AgentProcedureRepository` 重建节点合同。

这造成两个问题：

- 动态 script / inline / run artifact 没有 `root_graph_id`，真实 launch compose 会失败。
- 静态 graph 定义后续变化时，runtime projection 可能读到当前仓储定义，而不是启动时不可变 plan。

## 目标模型

```text
RuntimeSessionExecutionAnchor
  -> LifecycleRun
  -> OrchestrationInstance(orchestration_id)
  -> RuntimeNodeState(node_path, attempt)
  -> OrchestrationPlanSnapshot
  -> PlanNode
  -> ExecutorSpec
```

`WorkflowGraph` 和 `AgentProcedure` 仓储仍可作为 definition authoring / static graph compiler 输入，但 runtime launch 与 projection 的第一事实源是 plan snapshot。

## ExecutorSpec

当前：

```rust
ExecutorSpec::AgentProcedure {
    procedure_key,
    agent_reuse_policy,
    runtime_session_policy,
}
```

目标：将 agent executor contract 从“只能仓储 key”扩展为可序列化引用：

```rust
pub enum AgentProcedureExecutionSpec {
    ByKey { procedure_key: String },
    Snapshot {
        procedure_key: Option<String>,
        name: Option<String>,
        contract: AgentProcedureContract,
        source_ref: Option<OrchestrationSourceRef>,
        contract_digest: Option<String>,
    },
}

ExecutorSpec::AgentProcedure {
    procedure: AgentProcedureExecutionSpec,
    agent_reuse_policy,
    runtime_session_policy,
}
```

静态 graph compiler 输出 `ByKey`。script compiler 对声明了 `procedure` 的 agent 输出 `ByKey`，对只有 `prompt` 的 inline agent 输出 `Snapshot`；projection 与 compose 直接消费 snapshot contract，并在测试里覆盖 inline/snapshot 不查仓储。

## LifecycleNodeSpec

把 session assembler 的 lifecycle node 输入从 graph-era 改为 plan-era：

- 必需：`run`、`orchestration_id`、`node_path`、`attempt`、`lifecycle_key`、activation 用 activity shape
- provenance：`lifecycle_key/name/id` 从 orchestration `source_ref` 与 plan metadata 派生，不要求回查 graph 仓储
- agent contract：从 `PlanNode.executor` 派生，`ByKey` 可按需查仓储；`Snapshot` 直接使用 contract

短期为了降低改动，可以保留 `ActivityActivationInput.active_activity`，但新增 plan-to-activity adapter，只从 `PlanNode` 构造本次 activation 所需的 activity shape，不要求 workflow graph 仓储。

## ActiveWorkflowProjection

projection 应改为保存 plan-era 字段：

- `plan_node: PlanNode`
- `active_attempt: RuntimeNodeState`
- `active_node_type`
- `active_procedure_key: Option<String>`
- `primary_workflow: Option<AgentProcedure>` 仅当 `ByKey` 且仓储存在时可填
- `snapshot_contract: Option<AgentProcedureContract>` 用于 inline/snapshot executor

`active_contract()` 优先返回 `snapshot_contract`，否则返回 `primary_workflow.contract`。

## root_graph_id

`LifecycleRun.root_graph_id` 已移除。静态 graph 来源不再存为 run-level 单 graph 字段，而是由每个 `OrchestrationInstance.source_ref = WorkflowGraph { graph_id, graph_version }` 与 `OrchestrationPlanSnapshot.metadata.source` 表达。这样一个 lifecycle run 可以自然承载 0..N 个不同来源的 orchestration instance。

## 数据库 / contract

本轮不新增兼容字段；fresh schema 中 `lifecycle_runs` 直接保存 `context`、`orchestrations`、`view_projection`，不再保存 `root_graph_id`。`ExecutorSpec` 扩展存储在 `lifecycle_runs.orchestrations` JSON 中，并同步 contract TS 生成。

## 验证策略

- domain serde 测试覆盖 `ExecutorSpec::AgentProcedure` 的 `ByKey` 与 `Snapshot`。
- application 测试覆盖 lifecycle node compose/projection 不依赖 run-level graph id。
- projection 测试覆盖 inline/snapshot executor 能返回 active contract。
- 既有 dispatch / executor launcher / lifecycle tests 保持通过。
