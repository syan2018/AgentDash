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
- Agent / Function / LocalEffect / HumanGate executor 统一提交 `NodeStarted` 与 terminal node event；同步完成的 executor 也要先记录 started，再记录 completed/failed。
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
