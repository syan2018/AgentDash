# WorkflowGraph Activity Backend Contract

Activity lifecycle 是静态 `WorkflowGraph` 的定义层合同。运行态由 `LifecycleRun.orchestrations[]` 中的 `OrchestrationInstance` 承担；每个节点的 durable 状态落在 `RuntimeNodeState`，并通过 `orchestration_id + node_path + attempt` 定位。这样同一 Lifecycle 可以同时承载 root workflow、追加 workflow、review flow 或未来脚本编排，而不会把 runtime identity 绑定到某一种资产形态。

模块不变量见 [Workflow Architecture](./architecture.md)。

## Definition Contract

目标 graph definition：

```rust
pub struct WorkflowGraph {
    pub entry_activity_key: String,
    pub activities: Vec<ActivityDefinition>,
    pub transitions: Vec<ActivityTransition>,
}
```

当前 domain 中 `ActivityDefinition` 仍表达节点定义，原因是这个命名已经覆盖 Agent / Function / Human gate 等 workflow step 的编辑与模板语义；进入 runtime 前必须由 application compiler 转换为 semantic `OrchestrationPlanSnapshot`。

## Runtime Contract

- `WorkflowGraph` 只作为 compiler 输入，不拥有运行实例身份。
- compiler 输出 `OrchestrationPlanSnapshot(plan_digest=...)`；blocking diagnostics 在创建 orchestration 前返回给调用方。
- `LifecycleRun.add_orchestration` 直接保存 `OrchestrationInstance`，entry rules materialize 为 `RuntimeNodeState(status=Ready)` 与 ready queue。
- node status、inputs、outputs、executor refs、trace refs、error、cache refs 和 state exchange 都由 common orchestration runtime reducer 写入。
- Agent executor materialization 先提交 `NodeClaimed`，RuntimeSession accepted turn 后由 lifecycle advance 提交 `NodeStarted`。Function / LocalEffect / HumanGate executor 在真实执行开始时提交 `NodeStarted` 与 terminal node event；同步完成的 executor 也要先记录 started，再记录 completed/failed。
- Runtime session 反查节点时走 `RuntimeSessionExecutionAnchor -> LifecycleRun -> OrchestrationInstance -> RuntimeNodeState`。
- Active workflow projection 从 `LifecycleRun.orchestrations[]` 派生，供 API / frontend / VFS / hook 查询同一事实源。

## Agent Node Execution

Agent node 启动后，runtime evidence 表达为：

```text
LifecycleRun
  -> OrchestrationInstance(orchestration_id)
  -> RuntimeNodeState(node_path, attempt)
  -> LifecycleAgent
  -> AgentFrame
  -> RuntimeSessionExecutionAnchor(runtime_session_id, orchestration_id, node_path, attempt)
```

`ExecutorRunRef::RuntimeSession { session_id }` 只表示 connector delivery / trace evidence。业务归属、权限和 subject projection 仍通过 `LifecycleSubjectAssociation`、`LifecycleAgent` 与 `AgentFrame` 推导。

## Function / Local Effect Execution

Function 和本机 effect 可以在同一 scheduler pass 内同步完成，但仍遵守 runtime event 顺序：

```text
NodeStarted(executor_run_ref)
NodeCompleted(outputs) | NodeFailed(error)
```

Contract:

- success 写 declared output ports，并由 state exchange materialize successor inputs。
- failure 写 `RuntimeNodeError`，后续 retry / iteration policy 决定是否再激活新的 attempt。
- completion policy validation 由 orchestration runtime reducer 拥有。
- 本机执行能力的权限、预算和系统桥接开关由 AgentRun capability / permission surface 表达。

## Scenario: Semantic Executor Launcher

### 1. Scope / Trigger

- Trigger: application 层消费 `LifecycleRun.orchestrations[].dispatch.ready_node_ids` 并启动 semantic `PlanNode`。
- Scope: `workflow/orchestration/executor_launcher.rs`、`LifecycleOrchestrator` terminal bridge、`complete_lifecycle_node` tool、workflow API route、`agentdash-contracts::workflow` DTO。

### 2. Signatures

Application:

```rust
OrchestrationExecutorLauncher::drain_ready_nodes(
    run_id: Uuid,
) -> Result<OrchestrationExecutorDrainResult, WorkflowApplicationError>

OrchestrationExecutorLauncher::submit_human_gate_decision(
    input: SubmitHumanGateDecisionInput,
) -> Result<SubmitHumanGateDecisionResult, WorkflowApplicationError>
```

Runtime events:

```rust
NodeStarted { node_path, attempt, executor_run_ref, timestamp }
NodeCompleted { node_path, attempt, outputs, timestamp }
NodeFailed { node_path, attempt, error, timestamp }
NodeBlocked { node_path, attempt, error, timestamp }
```

HTTP:

```text
POST /api/workflows/lifecycle-runs/{run_id}/orchestration-human-decisions
```

Request:

```rust
SubmitOrchestrationHumanDecisionRequest {
    orchestration_id: String,
    node_path: String,
    attempt: u32, // default 1
    decision: serde_json::Value,
    resolved_by: Option<String>,
}
```

Response:

```rust
SubmitOrchestrationHumanDecisionResponse {
    run: LifecycleRunView,
    gate_id: String,
}
```

### 3. Contracts

- `drain_ready_nodes` 每次从 `LifecycleRunRepository` 重新加载 aggregate，处理一个 ready node 后写回，再进入下一轮；这样所有后继激活都来自 reducer materialization 后的最新 snapshot。
- AgentCall 正式支持 `AgentReusePolicy::CreateActivityAgent + RuntimeSessionPolicy::CreateNew`，并创建 `LifecycleAgent`、`AgentFrame`、`RuntimeSessionExecutionAnchor` 后提交 `NodeClaimed`。RuntimeSession accepted turn 成功后，再由 lifecycle advance 提交 `NodeStarted(ExecutorRunRef::RuntimeSession)`。
- Function API 与 BashExec 使用 `FunctionRunner` SPI。同步完成也必须先提交 `NodeStarted(ExecutorRunRef::FunctionRun)`，再提交 `NodeCompleted` 或 `NodeFailed`。
- compiler 将 BashExec 映射为 `PlanNodeKind::LocalEffect + ExecutorSpec::Function(BashExec)`；执行器按 typed executor spec 调用 `run_bash`，因此 `PlanNodeKind` 表达流程语义，`ExecutorSpec` 表达副作用机制。
- HumanGate 创建 `LifecycleGate(gate_kind=orchestration_human_gate)`，payload 必须包含 `run_id`、`orchestration_id`、`node_path`、`attempt`、`plan_node_id` 与 executor contract；runtime node 写 `ExecutorRunRef::HumanDecision`。
- Human decision route 校验 run project edit permission，通过 `orchestration_id + node_path + attempt` 定位 running HumanGate node，resolve gate 后提交 `NodeCompleted`，再 drain 后继 ready nodes。
- `NodeBlocked` 表达计划可识别但当前执行面尚不能真实推进的节点；orchestration status 聚合为 `Paused`，LifecycleRun status 聚合为 `Blocked`。

### 4. Validation & Error Matrix

| 条件 | 结果 |
| --- | --- |
| ready AgentCall 缺少 `ExecutorSpec::AgentProcedure` | `NodeBlocked(code=agent_executor_missing)` |
| AgentProcedure key 不存在 | `NodeBlocked(code=agent_procedure_not_found)` |
| AgentCall policy 不是 `CreateActivityAgent + CreateNew` | `NodeBlocked(code=agent_executor_policy_not_supported)` |
| ready Function / BashExec 没有 `FunctionRunner` | `NodeFailed(code=function_runner_unavailable)` |
| API request 返回非 2xx | `NodeFailed(code=api_request_status_failed)` |
| BashExec 非 0 exit | `NodeFailed(code=bash_exec_nonzero)` |
| `ExecutorSpec::LocalEffect(capability_key, input)` 尚无 concrete executor | `NodeFailed(code=local_effect_capability_not_supported)`，detail 携带 orchestration node coordinate |
| ready HumanGate 缺少 Human executor | `NodeBlocked(code=human_gate_executor_missing)` |
| decision route 指向非 Running node | `Conflict` |
| decision route 指向非 HumanGate node | `Conflict` |
| referenced lifecycle gate 不存在 | `NotFound` |
| gate 已 resolved | `Conflict` |
| node 声明多 attempt、unbounded attempt 或非 latest artifact alias | 副作用前 `NodeBlocked` |

### 5. Good/Base/Bad Cases

- Good: Function API node 从 Ready 进入 Running，再 Completed，`RuntimeNodeState.executor_run_ref` 与 `trace_refs` 写入 `FunctionRun`，declared output ports 进入 `state_snapshot.node_outputs`。
- Good: HumanGate node 打开 gate 后保持 Running；用户提交 decision 后 node Completed，后继节点由同一个 drain pass 启动。
- Base: compiler 产生的 BashExec LocalEffect 使用 `ExecutorSpec::Function(BashExec)` 执行，并按 declared output ports materialize stdout/stderr/exit code。
- Bad: executor side effect 启动后 terminal materialization 被 completion policy 拒绝时，执行器重新加载最新 run 并写 `NodeFailed(code=terminal_materialization_failed)`。

### 6. Tests Required

- Unit: reducer `NodeBlocked` 将 runtime node 置为 Blocked，并将 orchestration / lifecycle status 聚合为 Paused / Blocked。
- Integration: launcher drain 覆盖 Function `NodeStarted -> NodeCompleted`、trace ref、output port materialization 与 function context。
- Integration: launcher drain 覆盖 LocalEffect capability 未接入时 `NodeStarted -> NodeFailed`，error detail 带 orchestration node coordinate。
- Integration: launcher drain 覆盖 HumanGate gate payload、`ExecutorRunRef::HumanDecision` 与 ready queue 清空。
- Integration: attempt policy blocking 发生在 executor side effect 之前。
- Cross-layer: `pnpm run contracts:check` 与 `pnpm run frontend:check` 覆盖 decision route DTO 生成和前端服务签名。

### 7. Evidence Chain

```text
LifecycleRun.orchestrations[].dispatch.ready_node_ids
  -> OrchestrationExecutorLauncher
  -> typed executor side effect
  -> OrchestrationRuntimeEvent
  -> RuntimeNodeState
  -> LifecycleRunView / VFS / hook projection
```

## Artifact Contract

Agent executor 的 output port 内容是 lifecycle artifact 值。Runtime node completed 时只读取当前 node scope 下已声明或被 state exchange 使用的 output ports；每个 port 内容优先解析为 `serde_json::Value`，解析失败时物化为 JSON string。这样后继 artifact binding、gate evaluation 与 workflow projection 消费的是结构化值，同时允许 agent 通过 lifecycle VFS 写入普通文本产物。

## Workflow Template Asset Contract

Workflow template assets 进入 Shared Library 或从 Marketplace 安装/更新时，必须使用 normalized Activity payload。

Contract:

- Workflow template payloads normalized to `template.lifecycle.entry_activity_key`、`activities`、`transitions` before deserialization or persistence repair。
- Shared Library startup repair rewrites builtin workflow template assets to normalized shape and recomputes `payload_digest`。
- Project install/update commits workflow definitions and workflow graph definitions in one database transaction。
- Overwrite install preserves project resource ids and `created_at`，increments `version`，updates installed source metadata together。
- Runtime active workflow projection resolves from `LifecycleRun.orchestrations[]`。

## Validation

| Level | Target | Assertion |
|-------|--------|-----------|
| Unit | `WorkflowGraph` / current `ActivityLifecycleDefinition` serde roundtrip | graph definition 不携带 runtime state |
| Unit | `OrchestrationInstance` runtime namespace | 同一 run 内重名 node path 不污染 state |
| Integration | Runtime node key | scheduler / terminal / trace anchor 包含 `orchestration_id + node_path + attempt` |
| Integration | Terminal callback | 通过 RuntimeSession -> RuntimeSessionExecutionAnchor -> OrchestrationInstance -> RuntimeNodeState 推进 |
| Integration | Function executor | 先记录 `NodeStarted`，再记录 terminal event 与 declared output ports |
