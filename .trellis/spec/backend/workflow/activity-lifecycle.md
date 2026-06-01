# WorkflowGraph Activity Backend Contract

Activity lifecycle 的目标模型是 `WorkflowGraph -> WorkflowGraphInstance -> ActivityState / ActivityAttemptState`。当前代码中的 `ActivityLifecycleDefinition` 是 `WorkflowGraph` 的迁移来源；当前 `ActivityLifecycleRunState` 会迁入 run 内各 `WorkflowGraphInstance` 的 activity state namespace。

模块不变量见 [Workflow Architecture](./architecture.md)。

## Core Runtime Contract

- `WorkflowGraph` 是可执行 Activity graph definition，包含 activities、transitions、ports、artifact bindings 与 executor slots。
- `LifecycleRun` 是 tracked life process / control ledger，不是单个 graph 的 run；同一个 `LifecycleRun` 可以包含多个 `WorkflowGraphInstance`。
- root graph 只是 `WorkflowGraphInstance(role=root)`；当前 `LifecycleRun.lifecycle_id` 只是 root graph backfill 来源。
- Activity runtime identity 必须以 `graph_instance_id + activity_key` 为 namespace。
- Attempt / claim / assignment key 必须包含 `graph_instance_id + activity_key + attempt`。
- Scheduler 负责 durable claim 和 executor 启动；executor 只通过事件把结果交还给 `LifecycleEngine`。
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

## WorkflowGraphInstance Contract

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

Rules:

- `(run_id, role=root)` 在一个 run 内只能有一个。
- `role=task_execution`、`role=companion_review`、`role=routine_phase` 等只是同一 run 内 graph instance 的用途，不自动创建 child run。
- Child / linked `LifecycleRun` 只表达独立 lifecycle、context channel、control boundary、navigation boundary 或 long-lived projection boundary。

## Executor Launcher

Trait contract 目标形态：

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

`ExecutorRunRef::AgentSession` 只保留 runtime evidence；Agent identity、frame revision 与 Activity attempt 的桥接由 `AgentAssignment` 提供。

## Agent Assignment Route

Agent Activity 的标准执行链路：

```text
WorkflowGraphInstance
  -> ActivityState(activity_key)
  -> ActivityAttemptState(graph_instance_id, activity_key, attempt)
  -> AgentAssignment(run_id, graph_instance_id, activity_key, attempt, agent_id, frame_id)
  -> LifecycleAgent
  -> AgentFrame
  -> RuntimeSession refs
```

`complete_lifecycle_node`、terminal callback、VFS lifecycle provider、hook advance/resolution 都应使用 assignment / graph instance refs 推进 Activity。Session-indexed lookup 只能作为 trace adapter，并必须立即反查到 frame/agent/assignment。

## Function Executor

Function Activity 没有 Agent runtime terminal，因此启动后必须在同一次 scheduler pass 内把 terminal event 交给状态机。

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
- Runtime active workflow projection resolves from `WorkflowGraphInstance`、`AgentAssignment` and `ActivityAttemptState`。

## Validation

| Level | Target | Assertion |
|-------|--------|-----------|
| Unit | `WorkflowGraph` / current `ActivityLifecycleDefinition` serde roundtrip | graph definition 不携带 runtime state |
| Unit | `WorkflowGraphInstance` activity namespace | 同一 run 内重名 activity key 不污染 state |
| Integration | Scheduler claim key | 包含 `graph_instance_id + activity_key + attempt` |
| Integration | Terminal callback | 通过 RuntimeSession -> AgentFrame -> LifecycleAgent -> AgentAssignment -> ActivityAttemptState 推进 |
| Integration | Function executor | 立即产出 terminal event 并写入 declared output ports |
