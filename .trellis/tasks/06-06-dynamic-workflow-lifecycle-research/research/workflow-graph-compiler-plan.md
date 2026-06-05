# Research: WorkflowGraph -> OrchestrationPlanSnapshot compiler

- Query: 为下一阶段 `workflow-graph-compiler` 规划 `WorkflowGraph -> OrchestrationPlanSnapshot` 编译器，复核当前 graph/activity/transition/artifact/join/iteration/attempt 源码事实，并明确映射、错误模型、fixtures、测试与风险。
- Scope: internal
- Date: 2026-06-06

## Findings

### Files Found

| Path | Description |
| --- | --- |
| `.trellis/tasks/06-06-dynamic-workflow-lifecycle-research/prd.md` | 本研究任务边界、planning gate、文档索引。 |
| `.trellis/tasks/06-06-dynamic-workflow-lifecycle-research/design.md` | 目标架构入口，明确 graph/script 都编译到 `OrchestrationPlanSnapshot`。 |
| `.trellis/tasks/06-06-dynamic-workflow-lifecycle-research/implement.md` | 下一阶段任务拆解与已有粗映射表。 |
| `.trellis/tasks/06-06-dynamic-workflow-lifecycle-research/target-model-sketch.md` | Lifecycle / Orchestration / Plan / RuntimeNode 目标概念。 |
| `.trellis/tasks/06-06-dynamic-workflow-lifecycle-research/research/current-code-context.md` | 当前代码事实地图与迁移判断。 |
| `.trellis/spec/backend/workflow/architecture.md` | 当前 workflow vocabulary、不变量与模块边界。 |
| `.trellis/spec/backend/workflow/activity-lifecycle.md` | 当前 Activity runtime contract、executor、artifact、validation 要求。 |
| `.trellis/spec/backend/workflow/lifecycle-edge.md` | edge kind、artifact implies flow、runtime advancement 的 spec 版本。 |
| `.trellis/spec/backend/repository-pattern.md` | 聚合仓储、事务边界和 repository 规则。 |
| `.trellis/spec/backend/database-guidelines.md` | migration、JSON TEXT、schema 事实源规则。 |
| `.trellis/spec/backend/session/runtime-execution-state.md` | session-scoped AgentRun command 与 runtime-control 事实源边界。 |
| `.trellis/spec/frontend/workflow-activity-lifecycle.md` | 前端 WorkflowGraph definition / LifecycleRunView / mapper 边界。 |
| `crates/agentdash-domain/src/workflow/entity.rs` | `WorkflowGraph`、`LifecycleRun`、`ActivityExecutionClaim` 等领域实体。 |
| `crates/agentdash-domain/src/workflow/value_objects/activity_def.rs` | `ActivityDefinition`、executor、completion、iteration、join、transition、artifact binding。 |
| `crates/agentdash-domain/src/workflow/value_objects/run_state.rs` | current runtime state、attempt status、executor run refs、artifacts。 |
| `crates/agentdash-domain/src/workflow/validation.rs` | graph / activity / transition / policy validation。 |
| `crates/agentdash-application/src/workflow/engine.rs` | 当前 Activity 状态机、transition condition、artifact binding、attempt limit 执行逻辑。 |
| `crates/agentdash-application/src/workflow/scheduler.rs` | Ready attempt claim 与 executor launch 入口。 |
| `crates/agentdash-application/src/workflow/agent_executor.rs` | Agent / Human / Function executor 启动与 Function terminal event。 |
| `crates/agentdash-application/src/workflow/activity_run.rs` | load definition/run/graph instance/state 后重写 snapshot 的 application service。 |
| `crates/agentdash-application/src/workflow/orchestrator.rs` | session terminal / `complete_lifecycle_node` 到 `ActivityEvent` 的桥接。 |
| `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs` | `workflow_graphs` JSON TEXT 持久化与 row mapping。 |
| `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs` | `WorkflowGraphInstance.activity_state_json` 持久化。 |
| `crates/agentdash-infrastructure/migrations/0001_init.sql` | 当前 claims / assignments / lifecycle runs / graph instances / anchors / workflow graphs schema。 |
| `crates/agentdash-contracts/src/workflow.rs` | Rust -> TS contract 中的 graph/activity/transition/view DTO。 |
| `packages/app-web/src/services/workflow.ts` | 前端 mapper 对 activity/join/condition/artifact binding 的 strict parsing。 |
| `packages/app-web/src/stores/workflowStore.ts` | 前端 draft mutation、cycle warning、join/iteration/artifact binding 编辑入口。 |

### Target Context

目标架构已经要求 `LifecycleRun.orchestrations[]` 承载 0..N 个 `OrchestrationInstance`，静态 `WorkflowGraph` 和未来 dynamic script 都编译成 `OrchestrationPlanSnapshot`，共享 runtime rule、snapshot、journal、权限与观察模型（`.trellis/tasks/06-06-dynamic-workflow-lifecycle-research/design.md:9-20`）。

`OrchestrationPlanSnapshot` 的最小闭包已被设计稿定义为 `PlanNode`、`ActivationRule`、`ExecutorSpec`、`ResultContract`、`Limits`，其中 activation 必须覆盖 entry、condition、artifact binding、join、iteration/retry，executor 必须覆盖 AgentProcedure、continue root、function API/bash、人类决策与 effect capability key（`.trellis/tasks/06-06-dynamic-workflow-lifecycle-research/design.md:89-97`）。

目标模型明确 `WorkflowGraph` 只是 definition input，`OrchestrationInstance` 替代 `WorkflowGraphInstance` 的目标语义，`RuntimeNodeState` 替代 `ActivityAttemptState` 的中心地位，`FunctionRun` / `EffectInvocation` 是非 Agent 执行身份（`.trellis/tasks/06-06-dynamic-workflow-lifecycle-research/target-model-sketch.md:128-156`）。

本 compiler 阶段不是直接替换 runtime。`implement.md` 将 `workflow-graph-compiler` 列为第三个子任务，依赖 `orchestration-domain-contract`，验收是 graph fixtures 能编译为 plan，并验证 agent/function/human executor、condition、artifact binding、join/iteration policy（`.trellis/tasks/06-06-dynamic-workflow-lifecycle-research/implement.md:140-168`）。

### Current Source Facts

`WorkflowGraph` 当前是项目级可复用 definition asset，字段包括 `project_id`、`key`、`source`、`version`、`entry_activity_key`、`activities`、`transitions`，构造时调用 `validate_workflow_graph`（`crates/agentdash-domain/src/workflow/entity.rs:72-89`、`crates/agentdash-domain/src/workflow/entity.rs:133-183`）。

`ActivityDefinition` 当前包含 `key`、`description`、`executor`、`input_ports`、`output_ports`、`completion_policy`、`iteration_policy`、`join_policy`（`crates/agentdash-domain/src/workflow/value_objects/activity_def.rs:6-22`）。因此 compiler 不能只把 activity 当成普通 DAG node；它必须携带 executor、port/result、activation、join、iteration 信息。

`ActivityExecutorSpec` 有三类：`Agent`、`Function`、`Human`（`crates/agentdash-domain/src/workflow/value_objects/activity_def.rs:24-40`）。Agent executor 包含 `procedure_key`、`agent_reuse_policy`、`runtime_session_policy`，现有便捷构造只表达 `CreateActivityAgent + CreateNew` 和 `ContinueCurrentAgent + DeliverToCurrentTrace` 两个组合（`crates/agentdash-domain/src/workflow/value_objects/activity_def.rs:42-91`）。application executor 对其他组合直接返回 terminal error（`crates/agentdash-application/src/workflow/agent_executor.rs:733-751`）。

Function executor 当前覆盖 `ApiRequest` 和 `BashExec`，分别持有 HTTP method / URL / body template，以及 command / args / working directory（`crates/agentdash-domain/src/workflow/value_objects/activity_def.rs:93-115`）。Function 启动时创建 `ExecutorRunRef::FunctionRun`，执行 API/bash 后立即返回 exactly one terminal `ActivityEvent`（`crates/agentdash-application/src/workflow/agent_executor.rs:910-955`、`crates/agentdash-application/src/workflow/agent_executor.rs:957-1098`）。

Human executor 当前只有 `Approval(form_schema_key, title)`，启动时返回 `ExecutorRunRef::HumanDecision`，等待 human decision event 完成（`crates/agentdash-domain/src/workflow/value_objects/activity_def.rs:117-128`、`crates/agentdash-application/src/workflow/agent_executor.rs:753-757`）。

Completion policy 有 `OutputPorts`、`ExecutorTerminal`、`HumanDecision`、`HookGate`、`OpenEnded`（`crates/agentdash-domain/src/workflow/value_objects/activity_def.rs:130-145`）。engine 只对 `OutputPorts.required_ports` 与 `HumanDecision.decision_port` 做 output presence 校验；`HookGate`、`ExecutorTerminal`、`OpenEnded` 当前都直接通过 completion validation（`crates/agentdash-application/src/workflow/engine.rs:317-354`）。

Iteration policy 只有 `max_attempts` 与 `artifact_alias`，默认 `max_attempts=Some(1)`、`artifact_alias=Latest`（`crates/agentdash-domain/src/workflow/value_objects/activity_def.rs:147-162`）。engine 在创建新的 ready attempt 时检查 target activity 的 `max_attempts`（`crates/agentdash-application/src/workflow/engine.rs:397-452`）。`artifact_alias` 当前没有 runtime enforcement；artifact lookup 实际使用 latest output（`crates/agentdash-application/src/workflow/engine.rs:455-560`）。

Join policy 定义为 `All`、`Any`、`First`、`NOfM(n)`（`crates/agentdash-domain/src/workflow/value_objects/activity_def.rs:173-183`），validation 只检查 `NOfM.n > 0`（`crates/agentdash-domain/src/workflow/validation.rs:125-142`）。当前 engine 不读取 `join_policy`；target activation 逻辑等价为所有 incoming transition condition 都满足才 ready（`crates/agentdash-application/src/workflow/engine.rs:357-395`）。这不是可忽略字段，compiler 必须把它编译进 `ActivationRule.join_policy`，不能复刻当前 runtime 的隐式 all-only 行为。

Transition 当前包含 `from`、`to`、`kind`、`condition`、`artifact_bindings`、`max_traversals`（`crates/agentdash-domain/src/workflow/value_objects/activity_def.rs:185-208`）。condition 有 `Always`、`ArtifactFieldEquals`、`HumanDecisionEquals`、`AgentSignalEquals`（`crates/agentdash-domain/src/workflow/value_objects/activity_def.rs:210-231`）。engine 的 condition evaluation 先要求 `transition.from` 有 latest completed attempt，再按 condition 读取 latest output；`ArtifactFieldEquals.path` 同时支持 JSON pointer 与 dot path（`crates/agentdash-application/src/workflow/engine.rs:490-570`）。

`ArtifactBinding` 当前含 `from_activity?`、`from_port`、`to_port`、`alias`（`crates/agentdash-domain/src/workflow/value_objects/activity_def.rs:233-241`）。validation 默认 `from_activity = transition.from`，并检查 source output port 与 target input port 存在（`crates/agentdash-domain/src/workflow/validation.rs:225-265`）。runtime binding 也默认 `from_activity = transition.from`，取 latest output 写入 target attempt 的 `ActivityInputArtifact`（`crates/agentdash-application/src/workflow/engine.rs:455-488`）。

`max_traversals` 当前只出现在 definition、contracts、frontend 编辑与 validation 的 bounded-loop 判断里。backend validation 允许“指向 entry 的循环 transition”由 target max_attempts、transition max_traversals 或结构化 condition 之一约束（`crates/agentdash-domain/src/workflow/validation.rs:270-287`），但 engine 没有执行 `max_traversals` 计数。frontend 只把无阈值环降为 warning（`packages/app-web/src/features/workflow/model/dag-layout.ts:66-155`、`packages/app-web/src/stores/workflowStore.ts:92-110`）。

当前 runtime state 是 `ActivityLifecycleRunState { graph_instance_id, status, attempts, outputs, inputs }`（`crates/agentdash-domain/src/workflow/value_objects/run_state.rs:69-79`）。attempt state 只有 activity key、attempt、status、executor run、started/completed、summary（`crates/agentdash-domain/src/workflow/value_objects/run_state.rs:24-37`）。executor run ref 已区分 `RuntimeSession`、`FunctionRun`、`HumanDecision`（`crates/agentdash-domain/src/workflow/value_objects/run_state.rs:92-98`）。

`LifecycleEngine::initialize` 初始化 entry activity attempt #1 为 Ready，其余 activity attempt #1 为 Pending（`crates/agentdash-application/src/workflow/engine.rs:116-162`）。`ActivityLifecycleRunService` 每次推进都加载 definition/run/graph instance/state，应用 `ActivityEvent` 后整体替换 `activity_state` 并同步 run projection（`crates/agentdash-application/src/workflow/activity_run.rs:48-101`、`crates/agentdash-application/src/workflow/activity_run.rs:104-199`）。

Scheduler 扫描 Ready attempt，创建或获取 `ActivityExecutionClaim`，active claim 进入 executor launcher；claim key 和 idempotency key 都包含 `run_id + graph_instance_id + activity_key + attempt`（`crates/agentdash-application/src/workflow/scheduler.rs:95-141`、`crates/agentdash-domain/src/workflow/entity.rs:91-130`）。启动成功后先记录 `ExecutorStarted`，再应用 function immediate terminal events（`crates/agentdash-application/src/workflow/scheduler.rs:143-247`）。

Agent activity executor 现在已能创建新 child agent/runtime session，也能复用 root/current runtime session，ContinueRoot 路径会拒绝并行 running ContinueRoot（`crates/agentdash-application/src/workflow/agent_executor.rs:770-908`）。compiler 应只描述 executor intent，不应在 compile 阶段绑定实际 runtime session。

Agent output ports 通过 lifecycle VFS 写 JSON artifact；`complete_lifecycle_node` 读取 scoped port output map、校验 required output、解析 JSON 后生成 `ActivityCompleted`（`crates/agentdash-application/src/workflow/orchestrator.rs:246-283`、`crates/agentdash-application/src/workflow/orchestrator.rs:445-468`）。Activity activation prompt 会暴露 output artifact 路径与 input port readiness（`crates/agentdash-application/src/workflow/activity_activation.rs:42-91`、`crates/agentdash-application/src/workflow/activity_activation.rs:174-188`、`crates/agentdash-application/src/workflow/activity_activation.rs:230-277`）。

Current persistence 分散在 `lifecycle_runs.execution_log`、`lifecycle_workflow_instances.activity_state_json`、`activity_execution_claims`、`agent_assignments`、`runtime_session_execution_anchors`、`workflow_graphs.activities/transitions` 等表（`crates/agentdash-infrastructure/migrations/0001_init.sql:1-26`、`crates/agentdash-infrastructure/migrations/0001_init.sql:282-314`、`crates/agentdash-infrastructure/migrations/0001_init.sql:533-545`、`crates/agentdash-infrastructure/migrations/0001_init.sql:764-782`）。`WorkflowGraphRepository` 把 `activities` 与 `transitions` 作为 JSON TEXT 保存（`crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:154-180`、`crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:702-745`）。

Contracts 与 frontend 已暴露完整 Activity/transition shape，包括 join、iteration、condition、artifact bindings、max_traversals（`crates/agentdash-contracts/src/workflow.rs:206-395`、`packages/app-web/src/services/workflow.ts:300-410`）。所以 compiler 第一版必须把这些字段当成 public contract 的一部分，而不是只服务现有 engine 已使用的字段。

### Mapping Table

| Existing graph semantic | PlanNode | ActivationRule | ExecutorSpec | ResultContract | Limits |
| --- | --- | --- | --- | --- | --- |
| `WorkflowGraph.id/key/version/source/installed_source` | Snapshot-level `source_ref`, `plan_id`, `plan_digest`; no runtime node | N/A | N/A | N/A | N/A |
| `WorkflowGraph.entry_activity_key` | Marks the matching activity node as entry-capable | `Entry { node_id }`, initial ready root | N/A | N/A | N/A |
| `ActivityDefinition.key` | Stable `node_id`, recommend `activity:<key>`; `display_key=<key>` | Used by dependency/transition lookup | N/A | N/A | N/A |
| `ActivityDefinition.description` | `PlanNode.description` / UI metadata | N/A | N/A | N/A | N/A |
| `input_ports` | Node metadata and expected input slots | Artifact binding target ports and readiness inputs | Executor template context inputs | N/A | N/A |
| `output_ports` | Node metadata and output slots | Condition source ports and artifact binding source ports | Executor artifact write/read surface | Declared output ports; function outputs map to all declared ports | N/A |
| `completion_policy=OutputPorts` | N/A | Completion gate metadata | N/A | `required_output_ports` | N/A |
| `completion_policy=ExecutorTerminal` | N/A | Terminal event completes node | N/A | `terminal_status=executor_terminal` | N/A |
| `completion_policy=HumanDecision` | N/A | Completion requires decision output | Human executor decision result | `decision_port` | N/A |
| `completion_policy=HookGate` | N/A | Extension point: hook-gated completion | N/A | `hook_gate_key` extension point | N/A |
| `completion_policy=OpenEnded` | N/A | Extension point: manual/external completion | N/A | `open_ended` extension point | N/A |
| `ActivityExecutorSpec::Agent(create_activity_agent/create_new)` | `PlanNode(kind=activity)` with executor category `agent_call` | Node is dispatchable when ready | `AgentProcedure { procedure_key, reuse=create_child, runtime_session=create_new }` | Agent outputs are declared ports | Future per-node timeout/model/budget extension |
| `ActivityExecutorSpec::Agent(continue_current_agent/deliver_to_current_trace)` | same | Ready node targets current/root delivery surface | `AgentProcedure { procedure_key, reuse=current_agent, runtime_session=deliver_to_current_trace }` | Same as Agent | `exclusive_continue_root=true` should be represented because current executor rejects parallel ContinueRoot |
| Other Agent policy combinations | same if IR can represent; otherwise compile error | N/A | `UnsupportedAgentExecutorPolicy` if not supported | N/A | N/A |
| `FunctionActivityExecutorSpec::ApiRequest` | `PlanNode(kind=activity)` with executor category `function` | Dispatchable immediate effect node | `Function { type=api_request, method, url_template, body_template }` | Function result maps to declared output ports | Effect budget/timeout extension |
| `FunctionActivityExecutorSpec::BashExec` | `PlanNode(kind=activity)` with executor category `local_effect` or `function` plus `effect_kind=bash_exec` | Dispatchable immediate effect node | `LocalEffect { type=bash_exec, command, args, working_directory }` | Function result maps to declared output ports | Workspace root, timeout, permission extension |
| `HumanActivityExecutorSpec::Approval` | `PlanNode(kind=activity)` with executor category `human_gate` | Dispatchable human wait node | `HumanApproval { form_schema_key, title }` | `decision_port` if completion is human decision | Future SLA/timeout extension |
| `ActivityTransition.from/to` | Edges reference source and target node ids | Dependency edge from source to target | N/A | N/A | N/A |
| `ActivityTransition.kind=flow` | Edge metadata | Control dependency | N/A | N/A | N/A |
| `ActivityTransition.kind=artifact` | Edge metadata | Data dependency also implies flow; use bindings as actual exchange facts | N/A | N/A | N/A |
| `TransitionCondition::Always` | N/A | Condition expression `true`, still requires source latest completed | N/A | N/A | N/A |
| `ArtifactFieldEquals` | N/A | Condition expression reads `activity.port` and JSON path | N/A | N/A | N/A |
| `HumanDecisionEquals` | N/A | Condition expression reads decision output string | N/A | N/A | N/A |
| `AgentSignalEquals` | N/A | Condition expression reads signal output value | N/A | N/A | N/A |
| `ArtifactBinding.from_activity?` | N/A | Source node defaults to transition source when absent | N/A | Input materialization rule | N/A |
| `ArtifactBinding.from_port/to_port` | N/A | Artifact exchange rule attached to transition/dependency | N/A | Source output to target input | N/A |
| `ArtifactBinding.alias` | N/A | Preserve as `alias_policy`; current runtime only behaves like latest | N/A | Input materialization policy | N/A |
| `ActivityJoinPolicy::All` | N/A | Target activation requires all qualifying incoming dependencies | N/A | N/A | N/A |
| `ActivityJoinPolicy::Any` | N/A | Target activation requires any one satisfied incoming dependency | N/A | N/A | N/A |
| `ActivityJoinPolicy::First` | N/A | First satisfied incoming dependency wins; plan needs deterministic tie policy | N/A | N/A | N/A |
| `ActivityJoinPolicy::NOfM(n)` | N/A | At least `n` incoming dependencies satisfied | N/A | N/A | Validate `n > 0`; optionally `n <= incoming_count` warning/error |
| `ActivityIterationPolicy.max_attempts` | Node retry/iteration metadata | Governs creation of new node attempts | N/A | N/A | `max_attempts` |
| `ActivityIterationPolicy.artifact_alias` | Node output alias metadata | Output selection/materialization policy | N/A | Output alias/history policy | N/A |
| `ActivityTransition.max_traversals` | Edge metadata | Edge traversal count policy | N/A | N/A | `max_traversals` |
| Current `ActivityAttemptState` projection | Not part of immutable plan | Runtime materializes `RuntimeNodeState` attempts from plan | N/A | N/A | N/A |

### First Compiler Contract

Recommended module shape:

```text
crates/agentdash-domain/src/workflow/value_objects/orchestration_plan.rs
crates/agentdash-domain/src/workflow/orchestration_plan_compiler.rs
```

or, if application ownership is preferred after domain IR lands:

```text
crates/agentdash-application/src/workflow/orchestration/compiler.rs
```

The compiler should be pure and deterministic. It should not read repositories, create runs, create agents, inspect current runtime sessions, or check external capability grants. `AgentProcedure` existence, permission authorization, workspace root resolution, and current runtime trace resolution belong to application/runtime preflight.

Proposed input:

```text
WorkflowGraphCompileInput {
  graph: WorkflowGraph,
  source_ref: OrchestrationSourceRef::WorkflowGraph {
    graph_id,
    project_id,
    key,
    version,
    installed_source?,
  },
  compile_mode: Strict | LenientDiagnostics,
  target_schema_version: u32,
}
```

`compile_mode=Strict` is the default for new runtime activation. `LenientDiagnostics` may be useful for editor validation or migration reports, but should not be used to activate a plan with error diagnostics.

Proposed output:

```text
WorkflowGraphCompileOutput {
  plan_snapshot: OrchestrationPlanSnapshot,
  diagnostics: Vec<WorkflowGraphCompileDiagnostic>,
}
```

`OrchestrationPlanSnapshot` minimum fields:

```text
schema_version
plan_id or plan_digest
source_ref
nodes: Vec<PlanNode>
activation_rules: Vec<ActivationRule>
artifact_rules: Vec<ArtifactBindingRule>
limits: PlanLimits
metadata
```

`PlanNode` minimum fields for graph compiler:

```text
node_id
source_activity_key
kind=activity
description
input_ports
output_ports
executor: ExecutorSpec
result_contract: ResultContract
iteration_policy
join_policy
metadata
```

`ActivationRule` minimum fields:

```text
rule_id
target_node_id
trigger=entry | transition
incoming_edges
condition
join_policy
artifact_bindings
max_traversals?
```

The plan should carry source path metadata for diagnostics and future UI drilldown:

```text
source_path examples:
  graph.entry_activity_key
  activities[2].executor
  activities[2].join_policy
  transitions[1].condition
  transitions[1].artifact_bindings[0]
```

Plan ids should be stable. Prefer deterministic digest over random UUID for the snapshot identity:

```text
plan_digest = sha256(canonical_json({ graph identity, version, activities, transitions, compiler_schema_version }))
```

### Error Model

Compiler errors should be structured diagnostics with `code`, `severity`, `message`, `source_path`, and optional `related_paths`.

Blocking errors:

| Code | Condition |
| --- | --- |
| `invalid_workflow_graph` | `validate_workflow_graph` fails or graph shape cannot be trusted. |
| `entry_activity_missing` | entry key does not resolve to exactly one activity. |
| `duplicate_node_id` | activity key canonicalization produces duplicate node ids. |
| `dangling_transition_source` / `dangling_transition_target` | transition endpoint cannot resolve. |
| `dangling_condition_ref` | condition references missing activity/output port. |
| `dangling_artifact_binding_ref` | binding references missing source output or target input. |
| `unsupported_agent_executor_policy` | Agent policy pair is not representable or not launchable by target runtime. |
| `artifact_edge_without_binding` | strict mode: `kind=artifact` has no artifact binding. |
| `flow_edge_with_artifact_binding` | strict mode: `kind=flow` carries artifact bindings and target semantics require kind consistency. If target chooses binding-driven semantics, downgrade to warning. |
| `unbounded_cycle` | graph contains a cycle with no `max_attempts`, no `max_traversals`, and no structured condition. |
| `unsupported_plan_schema_version` | requested target schema is not supported. |

Warnings / non-blocking diagnostics:

| Code | Condition |
| --- | --- |
| `runtime_semantics_not_currently_enforced` | Current engine does not enforce `join_policy`, `artifact_alias`, or `max_traversals`; compiler preserves them for target runtime. |
| `n_of_m_exceeds_incoming_count` | `NOfM(n)` where `n > incoming_count`; this is probably unreachable but can be reported before making it blocking. |
| `condition_path_ambiguous` | dot path contains characters that may need JSON pointer; compiler preserves string exactly. |
| `hook_gate_extension` | `HookGate` is preserved as extension point; first runtime may not implement hook-gated activation. |
| `open_ended_extension` | `OpenEnded` completion is preserved as extension point; first runtime may require manual terminal command. |

Because the project is still pre-release, activation should use strict mode. Lenient mode should only exist to help inspect legacy/current drafts and should not become a compatibility path.

### Test Fixtures

Recommended fixtures should be plain Rust builders plus serialized golden snapshots. Keep them small and deterministic.

Positive fixtures:

1. `single_entry_agent`: one activity, entry ready, no transitions.
2. `agent_create_activity_agent`: Agent executor maps `CreateActivityAgent + CreateNew`.
3. `agent_continue_current`: Agent executor maps `ContinueCurrentAgent + DeliverToCurrentTrace` and sets exclusive continue-root limit.
4. `function_api_request`: Function API spec maps to function executor and output contract.
5. `function_bash_exec`: Bash maps to local effect/function executor with workspace/permission extension fields.
6. `human_approval`: Human approval maps to human gate executor and human decision result contract.
7. `conditions_all_variants`: `Always`、`ArtifactFieldEquals`、`HumanDecisionEquals`、`AgentSignalEquals` all preserve source refs and values.
8. `artifact_binding_default_source`: `from_activity=None` resolves to transition source.
9. `artifact_binding_explicit_source`: binding can read from a non-transition source activity.
10. `join_policy_variants`: All / Any / First / NOfM compile into activation join policy.
11. `iteration_and_alias_policy`: `max_attempts` and `artifact_alias` preserved on node/result contract.
12. `bounded_loop`: loop with `max_attempts` and/or `max_traversals` compiles with limits.
13. `completion_policy_variants`: OutputPorts / ExecutorTerminal / HumanDecision / HookGate / OpenEnded all map to `ResultContract`.
14. `deterministic_digest`: same graph compiles to byte-identical canonical snapshot/digest.

Negative fixtures:

1. `missing_entry_activity`.
2. `duplicate_activity_key`.
3. `dangling_transition_target`.
4. `dangling_condition_port`.
5. `dangling_artifact_binding_port`.
6. `unsupported_agent_policy_pair`, for mixed `CreateActivityAgent + DeliverToCurrentTrace` or `ContinueCurrentAgent + CreateNew`.
7. `artifact_edge_without_binding` in strict mode.
8. `unbounded_cycle` in strict activation mode.
9. `n_of_m_zero`, already caught by validation.
10. `n_of_m_exceeds_incoming_count`, initially warning unless target contract chooses to block.

### Minimal Test Plan

Unit tests in domain/application compiler module:

- `compile_entry_activity_rule`: entry key becomes one `Entry` activation rule and one activity node.
- `compile_executor_specs`: agent create, agent continue, function API, bash, human approval map to expected `ExecutorSpec`.
- `compile_result_contracts`: completion policies and ports map to output/terminal contracts.
- `compile_transition_conditions`: all condition variants preserve refs, values, JSON path strings.
- `compile_artifact_bindings`: default and explicit source activities map to artifact rules.
- `compile_join_and_iteration`: join policy, `max_attempts`, artifact alias, `max_traversals` survive roundtrip.
- `compile_diagnostics`: invalid refs, unsupported policy pair, unbounded cycles return pathful diagnostics.
- `compile_snapshot_is_deterministic`: canonical snapshot/digest stable across repeated compile.
- `plan_snapshot_serde_roundtrip`: plan JSON roundtrip stays byte-equivalent after canonicalization.

Integration-level tests after Orchestration domain contract exists:

- `workflow_graph_repository_to_plan`: load a persisted `WorkflowGraph` and compile without runtime state.
- `lifecycle_orchestration_seed`: create `OrchestrationInstance(role=root)` with compiled snapshot in memory or repository test.
- `activity_projection_compatibility`: if a compatibility projection builder exists, plan nodes can project to current activity labels without using `WorkflowGraphInstance.activity_state` as source of truth.

Do not wait for common runtime to test executor side effects. Compiler tests should only validate IR shape and diagnostics.

### Extension Points To Preserve

These semantics are present or required by target architecture but should not be over-implemented in the first compiler:

- Dynamic script-only node kinds: `phase`、`parallel_group`、`pipeline`、`barrier`、`subworkflow` should remain in `PlanNodeKind`, but graph compiler only emits `activity`.
- `HookGate` and `OpenEnded` should become `ResultContract` variants, not disappear because current engine accepts them without additional logic.
- `ArtifactAliasPolicy::PerAttempt` and `LatestAndHistory` should be preserved in plan even though current engine reads latest output.
- `ActivityJoinPolicy::Any`、`First`、`NOfM` should be preserved in `ActivationRule`; current runtime all-only behavior is not the target semantics.
- `max_traversals` should be preserved as edge limit even though current engine does not count traversals.
- Function/local effect authorization, workspace root binding, timeout and audit should be fields or nested extension specs on `ExecutorSpec`, but actual enforcement belongs runtime.
- Cache key inputs, budget, model routing and concurrency should exist in `Limits`/metadata as optional fields for dynamic workflow pressure, but current graph compiler may leave them unset.
- Cross-orchestration references should not be emitted by graph compiler yet; reserve source/target ref shapes for future subworkflow/dynamic script compilers.

### Risks

1. Silent semantic downgrade: copying current engine behavior would turn all joins into all-incoming and ignore `Any`/`First`/`NOfM`. Compiler should preserve declarative join policy and expose diagnostics if runtime cannot enforce it yet.
2. False confidence around loops: `max_traversals` exists in public contract but is not enforced by current engine. First compiler must carry it into `Limits` and reject/warn unbounded cycles before runtime activation.
3. Artifact alias loss: current runtime always picks latest output; target plan must carry alias policy so later snapshot/journal runtime can implement per-attempt/history behavior.
4. Transition `kind` drift: backend currently allows `artifact_bindings` independently of `kind`; frontend tends to show bindings only for artifact kind. Compiler should choose strict canonical semantics and report mismatches.
5. Agent policy pairs are structurally possible but runtime only supports two. Compiler should block unsupported pairs before creating a plan that cannot launch.
6. Plan snapshot identity can become nondeterministic if random ids or unordered JSON maps enter the snapshot. Use canonical ordering and deterministic digest.
7. Repository over-splitting: compiler should output an immutable value object; it should not create a new plan repository unless the later runtime proves cross-run plan reuse/caching needs it.
8. Confusing source of truth: `WorkflowGraphInstance.activity_state` is current state source but target compiler must not depend on it; plan compiles from definition only.
9. Function/local effect security: Bash/API are existing first-class executors. The compiler should not hide them under AgentRun; it should mark them as typed effect/function executors so runtime can apply capability/permission/audit.
10. Frontend/generated contract mismatch: graph fields are already public TS contracts. Any IR naming must avoid changing `WorkflowGraph` definition shape as a side effect of compiler work.

### Related Specs

- `.trellis/spec/backend/workflow/architecture.md:29-40` says current `WorkflowGraph` is main model, state advancement enters `LifecycleEngine`, function executor must return terminal event, artifact edge implies flow.
- `.trellis/spec/backend/workflow/activity-lifecycle.md:9-18` defines current core runtime contract and marks `WorkflowGraphInstance.activity_state` as current authoritative Activity runtime state.
- `.trellis/spec/backend/workflow/activity-lifecycle.md:118-141` defines Function executor behavior and exactly one terminal event.
- `.trellis/spec/backend/workflow/activity-lifecycle.md:143-157` defines JSON output artifact contract and runtime projection source.
- `.trellis/spec/backend/workflow/lifecycle-edge.md:24-31` describes current advancement as entry init, dependency satisfaction, terminal completion.
- `.trellis/spec/backend/repository-pattern.md:7-13` and `.trellis/spec/backend/database-guidelines.md:36-50` constrain repository/migration changes for later implementation.
- `.trellis/spec/backend/session/runtime-execution-state.md:107-132` anchors AgentRun delivery/control commands under runtime session routes.
- `.trellis/spec/frontend/workflow-activity-lifecycle.md:61-72` defines frontend definition fields; `.trellis/spec/frontend/workflow-activity-lifecycle.md:106-112` requires mapper boundary to reject unknown enum/missing required fields.

### Source / Spec Review Index

Primary code facts to re-open after context compaction:

- Graph entity and constructor validation entry: `crates/agentdash-domain/src/workflow/entity.rs:72-183`.
- Activity/executor/policy/transition/binding definitions: `crates/agentdash-domain/src/workflow/value_objects/activity_def.rs:6-241`.
- Current runtime state and executor refs: `crates/agentdash-domain/src/workflow/value_objects/run_state.rs:6-170`.
- Graph validation and transition/binding checks: `crates/agentdash-domain/src/workflow/validation.rs:13-287`.
- Condition validation: `crates/agentdash-domain/src/workflow/validation.rs:289-363`.
- Engine initialization and event application: `crates/agentdash-application/src/workflow/engine.rs:116-272`.
- Completion, transition activation, attempt limit, artifact binding, condition eval: `crates/agentdash-application/src/workflow/engine.rs:275-570`.
- Run status derivation: `crates/agentdash-application/src/workflow/engine.rs:625-686`.
- Scheduler claim/launch sequence: `crates/agentdash-application/src/workflow/scheduler.rs:95-274`.
- Agent/function/human executor dispatch: `crates/agentdash-application/src/workflow/agent_executor.rs:733-927`.
- Function API/Bash result mapping and template context: `crates/agentdash-application/src/workflow/agent_executor.rs:930-1098`.
- Activity state rewrite service: `crates/agentdash-application/src/workflow/activity_run.rs:48-199`.
- Orchestrator terminal/complete path: `crates/agentdash-application/src/workflow/orchestrator.rs:120-186` and `crates/agentdash-application/src/workflow/orchestrator.rs:246-300`.
- Agent activity activation prompt / lifecycle artifact surface: `crates/agentdash-application/src/workflow/activity_activation.rs:42-91` and `crates/agentdash-application/src/workflow/activity_activation.rs:174-277`.
- Persisted workflow graph JSON columns: `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:154-180` and `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:702-745`.
- Persisted graph instance state JSON: `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:42-160`.
- Current schema tables/indexes: `crates/agentdash-infrastructure/migrations/0001_init.sql:1-26`、`:282-314`、`:533-545`、`:764-782`、`:1198`.
- Generated workflow contracts: `crates/agentdash-contracts/src/workflow.rs:206-395` and `crates/agentdash-contracts/src/workflow.rs:787-850`.
- Frontend mapper strictness: `packages/app-web/src/services/workflow.ts:300-410` and `packages/app-web/src/services/workflow.ts:445-570`.
- Frontend unbounded-cycle warning: `packages/app-web/src/features/workflow/model/dag-layout.ts:66-155`.

Task design facts to re-open:

- Target IR contract: `.trellis/tasks/06-06-dynamic-workflow-lifecycle-research/design.md:89-121`.
- Phase 3 compiler scope: `.trellis/tasks/06-06-dynamic-workflow-lifecycle-research/design.md:216-222`.
- Target concept mapping: `.trellis/tasks/06-06-dynamic-workflow-lifecycle-research/target-model-sketch.md:128-156`.
- Repository/storage direction: `.trellis/tasks/06-06-dynamic-workflow-lifecycle-research/target-model-sketch.md:194-215`.
- Static graph compile direction: `.trellis/tasks/06-06-dynamic-workflow-lifecycle-research/target-model-sketch.md:241-270`.

### External References

No live external lookup was needed for this internal compiler plan. The external Claude Dynamic Workflow references relevant to the broader task are already persisted as task-local copies:

- `.trellis/tasks/06-06-dynamic-workflow-lifecycle-research/research/claude-dynamic-workflows-official-doc-zh-cn.md`
- `.trellis/tasks/06-06-dynamic-workflow-lifecycle-research/research/claude-dynamic-workflows-article-zhihu-simpread.md`

## Caveats / Not Found

- `OrchestrationPlanSnapshot`、`PlanNode`、`ActivationRule`、`ExecutorSpec`、`ResultContract`、`RuntimeNodeState`、`OrchestrationInstance`、`StateExchangeSnapshot` do not exist in code yet; they exist only in task design artifacts.
- Current backend runtime does not enforce `join_policy` variants, `artifact_alias` variants, or `transition.max_traversals`; it only preserves some of these in definition/contracts.
- Current backend validation does not fully align with `.trellis/spec/backend/workflow/lifecycle-edge.md`: the spec describes DAG/no-cycle rules, while current validation permits bounded loops to entry and frontend only warns on unbounded cycles.
- Compiler work should not edit existing code/spec from this research step. The actual implementation must wait for the `workflow-graph-compiler` task and the domain contract task it depends on.
