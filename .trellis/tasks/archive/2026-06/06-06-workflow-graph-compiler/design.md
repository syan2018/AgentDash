# WorkflowGraph 编译器设计计划

## 意图

本任务已完成第一版静态 graph 编译器实现。编译器负责把现有 `WorkflowGraph` definition 翻译为 Orchestration 领域合同任务引入的 immutable `OrchestrationPlanSnapshot`。

编译器是当前静态 workflow asset 到目标 common runtime 的桥。它不能因为旧 Activity runtime 没有完整执行某些字段，就把 public graph 语义降级或丢弃。

本编译器也不能把 graph “编成脚本”再靠脚本模拟图执行。目标模型来自 Claude Workflow 的启发，但对静态 graph 来说，正确产物是同一套语义 IR：控制流、状态变量、artifact exchange、executor identity、limits 和 diagnostics。未来 dynamic script compiler 也输出同一类 plan；二者共享 runtime，不共享某段临时脚本。

## 依赖

实现建立在已归档的 Orchestration 领域合同之上，并为已归档的 common runtime 静态 graph 接入提供稳定 plan 输入。领域合同中的类型命名和 plan snapshot 身份语义是 compiler 保持确定性输出的前提。

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

推荐模块位置是 `crates/agentdash-application/src/workflow/orchestration/compiler.rs` 或等价 application 子模块。理由是 compiler 面向 `WorkflowGraph` 这种应用层 definition 资产做规范化、诊断和 source metadata 处理；domain 层应只持有可序列化 IR/value object 与不变量，不继续把旧 graph 形态沉入 domain service。

实现上仍要求纯粹：application compiler 可以只依赖 domain value objects，不读仓储、不做授权、不触发 runtime side effect。

## 需要保留的 source 语义

| 当前 graph 语义 | 目标 plan 职责 |
| --- | --- |
| `WorkflowGraph.entry_activity_key` | Entry activation rule 与 ready root。 |
| `ActivityDefinition.key` | 稳定 source key、node id 种子与 source activity metadata。 |
| `ActivityExecutorSpec::Agent` | 语义节点 `PlanNodeKind::AgentCall` + `ExecutorSpec::AgentProcedure`。 |
| `ActivityExecutorSpec::Function(ApiRequest)` | 语义节点 `PlanNodeKind::Function` + typed function executor spec。 |
| `ActivityExecutorSpec::Function(BashExec)` | 语义节点 `PlanNodeKind::LocalEffect` + typed local effect/function executor spec；权限/runtime 字段可在合同具备后显式表达。 |
| `ActivityExecutorSpec::Human` | 语义节点 `PlanNodeKind::HumanGate` + approval executor spec。 |
| input / output ports | node port contract 与 artifact binding endpoint。 |
| completion policy | result contract。 |
| transition condition | 过程控制上的 activation condition expression。 |
| artifact bindings | 状态变量 / node output 到 target input 的 state exchange rule。 |
| join policy | activation join rule，包含 Any / First / NOfM。 |
| iteration policy | node retry / attempt 与 artifact alias policy。 |
| transition `max_traversals` | edge traversal limit metadata。 |

`flow` 与 `artifact` 不再被当成目标 runtime 的互斥边类型。`flow` 是过程控制维度，决定何时进入下一步；`artifact` 是状态交换维度，决定变量、输出和输入如何物化。旧 graph 里的 `ActivityTransitionKind` 只能作为 source metadata 和 normalization hint：有 artifact binding 的 transition 必须形成控制依赖和状态交换；没有 binding 的 artifact edge 是旧编辑语义欠完整，应诊断为需要补齐状态交换信息，而不是强行生成假变量。

## 目标 IR 形状

`WorkflowGraph` 是静态 definition input。第一版 graph compiler 应只生成静态可知的 semantic plan nodes，但这些节点必须使用完整 IR 类型：

| Source executor | 目标节点类型 | 说明 |
| --- | --- | --- |
| Agent activity | `PlanNodeKind::AgentCall` | 节点表示一次 Agent 调用或复用，不是泛化 Activity wrapper。source activity key 放入 metadata。 |
| Function API request | `PlanNodeKind::Function` | 节点表示受控 function invocation。 |
| BashExec / 本机桥接 | `PlanNodeKind::LocalEffect` | 节点表示受控本机 effect invocation，后续 runtime 负责 permission/workspace/audit。 |
| Human approval | `PlanNodeKind::HumanGate` | 节点表示 human gate / approval 等待。 |
| Graph structural rule | `ActivationRule` / `StateExchangeRule` | transition、condition、join、retry、artifact binding 不伪装成脚本语句。 |

`PlanNodeKind::Activity` 只作为 legacy/source projection 语义保留：例如兼容旧 UI 的 activity label、source path 或迁移报告。它不应成为 graph compiler 的默认运行节点类型。

plan identity 使用内容 digest，而不是随机 UUID。推荐字段方向是：

```text
plan_digest = sha256(canonical_json({
  source_ref,
  graph_version,
  activities,
  transitions,
  compiler_schema_version
}))
```

`OrchestrationInstance.orchestration_id` 继续使用 UUID，`OrchestrationPlanSnapshot` 使用 digest 作为不可变编译产物身份。digest 面向机器和缓存，不需要人工手写；UI 可显示短 digest 或 source key/version。

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
- `artifact_edge_missing_state_exchange`
- `ambiguous_legacy_edge_normalization`
- `unbounded_cycle`
- `unsupported_plan_schema_version`

可能的 warning diagnostics：

- `runtime_semantics_not_currently_enforced`
- `legacy_edge_kind_normalized`
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
- flow transition with artifact binding normalizes into control dependency plus state exchange
- artifact transition with binding normalizes into the same two dimensions
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
- artifact edge missing state exchange in strict activation mode
- unbounded cycle
- invalid `NOfM`

## 已收敛决策

- 编译器放在 application 层，保持纯函数。
- plan snapshot 身份使用 deterministic digest；UUID 留给运行实例。
- `flow` / `artifact` 按控制流与状态交换两个维度规范化，不再把旧边 kind mismatch 当成目标语义。
- graph activity 编译为 semantic node kind：AgentCall / Function / LocalEffect / HumanGate；`Activity` 只保留为 source/projection 语义。
