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

## Scenario: LifecycleGate Gate Wait Policy Terminal Convergence

### 1. Scope / Trigger

- Trigger: a durable `LifecycleGate` carries a gate wait policy for a producer result, and the producer reaches terminal before the expected result writer completes.
- Scope: `agentdash-domain::workflow::gate_wait_policy` payloads, `LifecycleGateRepository::list_by_wait_producer`, `agentdash-application-workflow::gate::GateProducerTerminalConvergenceService`, companion child wait gates, parent mailbox wake intents, and boot/replay callers that feed producer terminal facts.

This scenario keeps wait completion authority with `LifecycleGate`, because the gate is the durable owner of review/resume/wait state. Runtime callbacks and boot reconcile provide producer terminal facts; they do not inspect gate kinds or runtime session anchors as business addresses.

### 2. Signatures

Domain declaration payload:

```rust
pub enum WaitProducerRef {
    AgentRunDelivery {
        run_id: Uuid,
        agent_id: Uuid,
        frame_id: Option<Uuid>,
    },
}

pub struct GateWaitPolicyEnvelope {
    pub schema_version: u32,
    pub wait_policy: GateWaitPolicy,
    pub display: Map<String, Value>,
}

pub struct GateWaitPolicy {
    pub source: WaitProducerRef,
    pub expected_result: WaitExpectedResult,
    pub terminal_policy: WaitTerminalPolicy,
    pub wake_target: WaitWakeTarget,
}
```

Repository and convergence surface:

```rust
LifecycleGateRepository::list_open_wait_policies(
    limit: usize,
) -> Result<Vec<LifecycleGate>, DomainError>

LifecycleGateRepository::list_by_wait_producer(
    producer: &WaitProducerRef,
) -> Result<Vec<LifecycleGate>, DomainError>

GateProducerTerminalConvergenceService::observe_producer_terminal(
    event: GateProducerTerminalEvent,
) -> Result<GateProducerTerminalConvergenceResult, WorkflowApplicationError>

pub trait GateProducerTerminalConvergencePort: Send + Sync {
    async fn observe_gate_producer_terminal(
        &self,
        event: GateProducerTerminalEvent,
    ) -> Result<GateProducerTerminalConvergenceResult, ApplicationError>;
}

pub struct LifecycleGateWaitingProjection {
    pub kind: String,
    pub source_label: Option<String>,
    pub preview: Option<String>,
}
```

Companion child wait gates persist the wait policy envelope in `LifecycleGate.payload_json` when the gate is opened:

```json
{
  "schema_version": 1,
  "wait_policy": {
    "source": {
      "kind": "agent_run_delivery",
      "run_id": "...",
      "agent_id": "...",
      "frame_id": "..."
    },
    "expected_result": {
      "kind": "companion_result",
      "correlation_ref": "dispatch-..."
    },
    "terminal_policy": {
      "failed": { "status": "failed", "failure_kind": "runtime_terminal_failed" },
      "interrupted": { "status": "cancelled", "failure_kind": "runtime_terminal_cancelled" },
      "completed": { "status": "failed", "failure_kind": "missing_companion_respond" }
    },
    "wake_target": {
      "namespace": "companion",
      "target_run_id": "...",
      "target_agent_id": "...",
      "client_command_id": "companion-result:{gate_id}"
    }
  }
}
```

### 3. Contracts

- `WaitProducerRef` is the business location of the producer. Runtime session ids remain trace refs and diagnostics; AgentRun delivery producers use `run_id + agent_id + frame_id`.
- `list_open_wait_policies` is the bounded boot/retry scan for gates that carry `wait_policy.source`、`wait_policy.expected_result` and `wait_policy.terminal_policy`.
- `list_by_wait_producer` is a precise lookup over declared wait source fields. It is not a gate-kind scan, because the same gate kind can later represent different producer/result contracts.
- `observe_producer_terminal` resolves only gates whose declaration matches the producer and whose `expected_result.kind` is supported by the convergence implementation.
- If the gate is open, convergence writes a result payload with `source="producer_terminal"`, terminal diagnostics, status mapped by `terminal_policy`, and preserved gate wait policy metadata.
- If the gate is already resolved, convergence preserves the existing payload and returns delivery intents to ensure the wake is observable.
- Normal result writers and producer terminal convergence are first-writer-wins. A race that discovers the gate was resolved after an open read must reload the gate and switch to delivery ensure rather than overwrite.
- Parent wake delivery uses the policy's stable command id, for companion child results `companion-result:{gate_id}`, so replay and terminal callback recovery are idempotent.
- Production callers enter terminal convergence through `agentdash_application::gate_wait_policy::GateProducerTerminalConvergencePort`. The application service owns workflow convergence plus delivery intent execution; companion gate control owns normal companion result writers and delivery adapters.
- Runtime terminal callbacks and boot reconcile inject the application gate wait policy service. They must not construct companion gate control to resolve producer terminal facts.
- `LifecycleGate::waiting_projection()` owns gate waiting item `kind`、`source_label` and `preview` derivation. Wait activity and AgentRun workspace waiting projection both consume this helper so gate kind mapping and preview fallback cannot drift between surfaces.
- `LifecycleGateRepository::find_by_agent_and_correlation` is reserved for precise normal companion result writer lookup. Terminal callback and boot reconcile use wait producer declarations instead.

### 4. Validation & Error Matrix

| 条件 | 结果 |
| --- | --- |
| gate wait policy missing or unparsable | gate is skipped by producer convergence |
| producer ref has no matching gates | convergence returns no matching gate wait policy and logs diagnostic context |
| matching gate is open | resolver completes gate with terminal-derived result payload |
| matching gate resolves concurrently before terminal write | convergence reloads and ensures delivery without payload overwrite |
| matching gate is already resolved | existing payload is retained and parent wake delivery is ensured |
| parent delivery binding is unavailable for wake | convergence returns conflict so callback/reconcile can retry or diagnose |
| terminal state is `failed` | result status follows policy `failed`, with `failure_kind=runtime_terminal_failed` |
| terminal state is `interrupted` / `cancelled` | result status follows policy `cancelled` |
| terminal state is `completed` without expected result | result is protocol failure, represented as `failed` plus `failure_kind=missing_companion_respond` when wire status does not expose `protocol_failed` |
| a new gate kind is projected into wait/workspace views | implement `LifecycleGate::waiting_projection()` semantics and cover wait/workspace consistency tests |

### 5. Good/Base/Bad Cases

- Good: SubAgent runtime fails, AgentRun terminal convergence emits `WaitProducerRef::AgentRunDelivery`, matching companion wait gate resolves to `failed`, and parent mailbox receives one `companion-result:{gate_id}` wake.
- Good: Child calls `companion_respond` before the terminal callback arrives; terminal convergence reloads the resolved gate and only ensures the same parent wake delivery.
- Good: A new LifecycleGate source appears in both wait tool and workspace waiting items through the shared `LifecycleGateWaitingProjection`.
- Base: Producer terminal event has no gate wait policy because it is an ordinary AgentRun; convergence returns no matching gate wait policy without mutating gates.
- Bad: A caller derives parent/child ownership from `runtime_session_id` outside the AgentRun seam, because runtime session ids are connector trace evidence rather than wait producer identity.
- Bad: API terminal callback or boot reconcile calls companion gate control directly to resolve producer terminal facts, because that makes companion request handling the owner of a generic gate wait policy contract.

### 6. Tests Required

- Unit: `GateWaitPolicyEnvelope` serializes into gate payload and preserves existing companion metadata.
- Unit: producer terminal resolves open companion wait gate and maps failed/interrupted/completed terminal states.
- Unit: producer terminal replay against a resolved gate preserves payload and returns delivery ensure intents.
- Race: normal result writer and producer terminal convergence each reload resolved gates and preserve first-writer-wins semantics.
- Repository: Postgres `list_by_wait_producer` matches `payload_json.wait_source.kind/run_id/agent_id/frame_id` and does not depend on `gate_kind`.
- Integration: AgentRun terminal callback feeds `WaitProducerTerminalEvent` after `AgentRunDeliveryBinding` terminal convergence.
- Reconcile: boot recovery scans declared open gate wait policies and feeds producer terminal facts through the same convergence surface.
- Unit: wait activity and workspace waiting projection assert the same `LifecycleGateWaitingProjection.kind` and `preview` for the same gate.
- Unit: production terminal callback and boot reconcile are wired to `GateProducerTerminalConvergencePort`; companion gate terminal convergence entry points are test-only or absent from production callers.

### 7. Wrong vs Correct

#### Wrong

```text
RuntimeSession terminal -> scan open companion_wait_follow_up gates -> resolve guessed gate
```

#### Correct

```text
RuntimeSession terminal
  -> AgentRunDeliveryBinding terminal
  -> WaitProducerRef::AgentRunDelivery
  -> LifecycleGate gate wait policy convergence
  -> parent mailbox wake
```

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
