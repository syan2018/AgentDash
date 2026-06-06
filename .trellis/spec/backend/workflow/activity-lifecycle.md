# Legacy WorkflowGraph Activity Backend Contract

Activity lifecycle 是当前静态 graph runtime 的迁移来源，不是目标 runtime 模型。正式 runtime 模型是 `LifecycleRun -> OrchestrationInstance -> RuntimeNodeState / StateExchangeSnapshot / journal`。当前代码中的 `ActivityLifecycleDefinition` / `WorkflowGraph`、`WorkflowGraphInstance.activity_state`、`ActivityAttemptState`、`ActivityExecutionClaim`、`AgentAssignment(graph_instance_id, activity_key, attempt)` 只用于解释现有实现和迁移覆盖范围。

模块不变量见 [Workflow Architecture](./architecture.md)。

## Legacy Runtime Contract

- `WorkflowGraph` 是可执行 Activity graph definition，包含 activities、transitions、ports、artifact bindings 与 executor slots。
- 旧 Activity runtime identity 以 `graph_instance_id + activity_key` 为 namespace。
- 旧 attempt / claim / assignment key 包含 `graph_instance_id + activity_key + attempt`。
- `WorkflowGraphInstance.activity_state` 是旧 Activity scheduler 的 snapshot source；迁移到 common orchestration runtime 后，node truth 落在 `LifecycleRun.orchestrations[]`。
- 旧 Scheduler 负责 durable claim 和 executor 启动；executor 只通过事件把结果交还给 `LifecycleEngine`。
- Function executor 即使立即完成，也必须产出 Activity terminal event，而不是直接修改 run state。
- Hook evaluation 可以报告 completion metadata，但 durable state advancement 仍由 ActivityEvent application 拥有。

## Definition Contract

目标 graph definition：

```rust
pub struct WorkflowGraph {
    pub entry_activity_key: String,
    pub activities: Vec<ActivityDefinition>,
    pub transitions: Vec<ActivityTransition>,
}
```

迁移来源：

```rust
// 当前名称，目标语义是 WorkflowGraph
pub struct ActivityLifecycleDefinition {
    pub entry_activity_key: String,
    pub activities: Vec<ActivityDefinition>,
    pub transitions: Vec<ActivityTransition>,
}
```

`WorkflowDefinition` 的目标语义不是 graph config，而是 `AgentProcedure`：单个 Agent Activity 的 behavior / capability / context / hook contract。Agent executor 只引用 procedure 或 procedure policy，不应把整张 graph topology 塞进 procedure。

## WorkflowGraphInstance Migration Source

```text
WorkflowGraphInstance
  id
  run_id
  graph_id
  role
  status
  activity_state
  created_at
  updated_at
```

Legacy rules:

- `(run_id, role=root)` 在一个 run 内只能有一个。
- `WorkflowGraphInstance` 只属于 `topology=workflow_graph` 的 run。
- `role=task_execution`、`role=companion_review`、`role=routine_phase` 等只是同一 run 内 graph instance 的用途，不自动创建 child run。
- Child / linked `LifecycleRun` 只表达独立 lifecycle、context channel、control boundary、navigation boundary 或 long-lived projection boundary。
- 迁移目标不创建 `WorkflowGraphInstance` 作为运行实例身份；旧 role 映射为 `OrchestrationInstance.role`，旧 activity state 映射为 `RuntimeNodeState` / `StateExchangeSnapshot`。

## Executor Launcher

旧 trait contract：

```rust
#[async_trait]
pub trait ActivityExecutorLauncher {
    async fn start(
        &self,
        graph_instance: &WorkflowGraphInstance,
        definition: &WorkflowGraph,
        state: &ActivityLifecycleRunState,
        claim: &ActivityExecutionClaim,
    ) -> Result<ActivityExecutorStartResult, ActivityExecutorStartError>;
}

pub struct ActivityExecutorStartResult {
    pub assignment_ref: Option<AgentAssignmentRef>,
    pub executor_run: ExecutorRunRef,
    pub immediate_events: Vec<ActivityEvent>,
}
```

`ExecutorRunRef::AgentSession` 只保留 runtime evidence；旧 Agent identity、frame revision 与 Activity attempt 的桥接由 `AgentAssignment` 提供。目标 runtime 中对应桥接应是 `AgentInvocation(lifecycle_run_id, orchestration_id, node_path, attempt, agent_run_id, frame_id)`。

## Agent Assignment Route

旧 Agent Activity 执行链路：

```text
WorkflowGraphInstance
  -> ActivityState(activity_key)
  -> ActivityAttemptState(graph_instance_id, activity_key, attempt)
  -> AgentAssignment(run_id, graph_instance_id, activity_key, attempt, agent_id, frame_id)
  -> LifecycleAgent
  -> AgentFrame
  -> RuntimeSessionExecutionAnchor refs
```

目标 runtime 中，`complete_lifecycle_node`、terminal callback、VFS lifecycle provider、hook advance/resolution 必须使用 orchestration/node refs 推进 runtime node。旧 assignment / graph instance refs 只服务迁移覆盖和旧投影验证。

Runtime session 反查优先级：

```text
runtime_session_id
  -> RuntimeTraceAnchor
  -> lifecycle_run_id / orchestration_id / node_path / agent_run_id / frame_id
  -> OrchestrationEvent application
```

`AgentFrameRuntimeView.runtime_session_refs` 从 runtime trace anchor read model 投影，原因是 frame surface 与 runtime trace 索引有不同变化节奏。

## Function Executor

Function Activity 没有 Agent runtime terminal，因此启动后必须在同一次 scheduler pass 内把 terminal event 交给状态机。目标 runtime 仍保留这条语义，但事件名称和状态落点变为 `NodeStarted` + `NodeCompleted/NodeFailed`。

Function execution port:

```rust
async fn execute_function_activity(
    definition: &WorkflowGraph,
    activity: &ActivityDefinition,
    claim: &ActivityExecutionClaim,
    state: &ActivityLifecycleRunState,
) -> Result<FunctionExecutionResult, String>;
```

Contract:

- Scheduler 先记录 `ExecutorStarted`，再应用 `immediate_events`。
- Agent/Human executors 返回 started result，不带 immediate events。
- Function executors 返回 `ExecutorRunRef::FunctionRun { run_id }` plus exactly one terminal event。
- success -> `ActivityEvent::ActivityCompleted`。
- failure -> `ActivityEvent::ActivityFailed`。
- Function output values 映射到 declared `activity.output_ports`。
- completion policy validation 由 `LifecycleEngine` 拥有。

## Artifact Contract

Agent executor 的 output port 内容是 lifecycle artifact 值，必须写入 JSON 内容。Activity completed 时只读取 activity 已声明的 output ports，并把每个 port 的文件内容解析为 `serde_json::Value`；解析失败表示 artifact contract 无效，activity 不进入 completed。这样后继 artifact binding、gate evaluation 与 workflow projection 消费的是结构化值，而不是由 orchestrator 猜测的自由文本。

## Workflow Template Asset Contract

Workflow template assets 进入 Shared Library 或从 Marketplace 安装/更新时，必须使用 normalized Activity payload。

Contract:

- Workflow template payloads normalized to `template.lifecycle.entry_activity_key`、`activities`、`transitions` before deserialization or persistence repair。
- Shared Library startup repair rewrites builtin workflow template assets to normalized shape and recomputes `payload_digest`。
- Project install/update commits workflow definitions and workflow graph definitions in one database transaction。
- Overwrite install preserves project resource ids and `created_at`，increments `version`，updates installed source metadata together。
- Runtime active workflow projection must resolve from `LifecycleRun.orchestrations[]`;旧 `WorkflowGraphInstance` / `AgentAssignment` / `ActivityAttemptState` 只能作为迁移前对照。

## Validation

| Level | Target | Assertion |
|-------|--------|-----------|
| Unit | `WorkflowGraph` / current `ActivityLifecycleDefinition` serde roundtrip | graph definition 不携带 runtime state |
| Unit | `OrchestrationInstance` runtime namespace | 同一 run 内重名 node path 不污染 state |
| Integration | Scheduler claim key | 包含 `orchestration_id + node_path + attempt` |
| Integration | Terminal callback | 通过 RuntimeSession -> RuntimeTraceAnchor -> OrchestrationInstance -> RuntimeNodeState 推进 |
| Integration | Function executor | 立即产出 terminal event 并写入 declared output ports |
