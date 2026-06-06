# WorkflowGraph 编译器设计计划

## 意图

本任务只准备第一版静态 graph 编译器，不实现它。编译器负责把现有 `WorkflowGraph` definition 翻译为 Orchestration 领域合同任务引入的 immutable `OrchestrationPlanSnapshot`。

编译器是当前静态 workflow asset 到目标 common runtime 的桥。它不能因为旧 Activity runtime 没有完整执行某些字段，就把 public graph 语义降级或丢弃。

## 依赖

实现阻塞于 `.trellis/tasks/06-06-orchestration-domain-contract`。需要等领域合同落地并确认具体类型命名后，再评审并启动本任务。本任务当前保持 `planning`。

## 编译器边界

编译器是纯函数、确定性转换：

- input：`WorkflowGraph`、source metadata、compile mode；
- output：`OrchestrationPlanSnapshot`、structured diagnostics；
- 不读 repository；
- 不查询 runtime session；
- 不创建 agent/frame；
- 不做权限检查；
- 不拥有 scheduler state；
- 不依赖 `WorkflowGraphInstance.activity_state`。

## 需要保留的 source 语义

| 当前 graph 语义 | 目标 plan 职责 |
| --- | --- |
| `WorkflowGraph.entry_activity_key` | Entry activation rule 与 ready root。 |
| `ActivityDefinition.key` | 稳定 plan node id 与 source activity metadata。 |
| `ActivityExecutorSpec::Agent` | `ExecutorSpec::AgentProcedure` 或等价表达。 |
| `ActivityExecutorSpec::Function(ApiRequest)` | typed function executor spec。 |
| `ActivityExecutorSpec::Function(BashExec)` | typed local effect / function executor spec；权限/runtime 字段可在合同具备后显式表达。 |
| `ActivityExecutorSpec::Human` | human gate / approval executor spec。 |
| input / output ports | node port contract 与 artifact binding endpoint。 |
| completion policy | result contract。 |
| transition condition | activation condition expression。 |
| artifact bindings | state exchange / input materialization rules。 |
| join policy | activation join rule，包含 Any / First / NOfM。 |
| iteration policy | node retry / attempt 与 artifact alias policy。 |
| transition `max_traversals` | edge traversal limit metadata。 |

## 诊断

诊断应包含 `code`、`severity`、`message`、`source_path` 和可选 `related_paths`。

可能的 blocking diagnostics：

- `invalid_workflow_graph`
- `entry_activity_missing`
- `duplicate_node_id`
- `dangling_transition_source`
- `dangling_transition_target`
- `dangling_condition_ref`
- `dangling_artifact_binding_ref`
- `unsupported_agent_executor_policy`
- `artifact_edge_without_binding`
- `flow_edge_with_artifact_binding`
- `unbounded_cycle`
- `unsupported_plan_schema_version`

可能的 warning diagnostics：

- `runtime_semantics_not_currently_enforced`
- `n_of_m_exceeds_incoming_count`
- `condition_path_ambiguous`
- `hook_gate_extension`
- `open_ended_extension`

项目仍处预研期，runtime activation 应使用 strict mode。Lenient diagnostics 可以服务 editor / report，但不能用于启动 runtime。

## Fixtures 计划

正向 fixtures：

- single entry agent
- create activity agent
- continue current / root agent
- function API request
- function bash exec
- human approval
- all condition variants
- default / explicit artifact binding source
- all join policy variants
- iteration and artifact alias policy
- bounded loop
- all completion policy variants
- deterministic digest / canonical snapshot

反向 fixtures：

- missing entry
- duplicate activity key
- dangling transition endpoint
- dangling condition port
- dangling artifact binding port
- unsupported agent policy pair
- strict mode 下 artifact edge without binding
- unbounded cycle
- invalid `NOfM`

## 评审问题

实现前需要和用户确认：

- 编译器放在 `agentdash-domain` 作为纯 domain service，还是放在 `agentdash-application` 作为 application compiler？
- 第一版 plan identity 使用 deterministic digest、UUID + digest，还是 instance 用 UUID、content 用 digest？
- strict artifact mismatch 立即作为 blocking，还是在前端编辑语义调整前先作为 warning？
- API / bash 节点应编译为哪个 `PlanNodeKind`：`Function`、`LocalEffect`，还是 `Activity` 下的 executor sub-kind？
