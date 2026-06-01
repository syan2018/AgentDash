# Lifecycle Dispatch Service 设计

## 目标

`LifecycleDispatchService` 是业务执行进入控制面的统一入口。它接收 `ExecutionIntent`，决定复用同一个 `LifecycleRun` 追加 `WorkflowGraphInstance`，还是创建 linked/spawned `LifecycleRun`，并返回稳定 refs。它不拥有 `AgentFrame` 的内部构造，也不把 connector DTO 暴露给业务模块。

## 输入合同：ExecutionIntent

| 字段 | 含义 |
| --- | --- |
| `project_id` | project scope 与默认配置解析入口 |
| `source` | user / routine / parent_agent / project_agent / api / migration |
| `subject_ref?` | Story / Task / RoutineExecution / Project / External 等业务引用 |
| `parent_run_id?` | 请求接续或派生的现有 run |
| `parent_agent_id?` | 发起派生、companion、task helper 的 agent |
| `workflow_graph_ref?` | 目标可执行图；未提供时由 policy/profile 解析 |
| `agent_procedure_ref?` | 单个 Agent Activity 的 procedure override |
| `run_policy` | reuse_existing / append_graph / create_linked_run |
| `agent_policy` | create / reuse / resume / spawn_child |
| `context_policy` | inherit / slice / isolated |
| `capability_policy` | baseline / inherited_slice / grant_constrained |
| `runtime_policy` | create_runtime_session / attach_existing / continue_current |
| `gate_policy?` | 是否创建 durable gate，以及 gate kind / correlation |

## 输出合同：ExecutionDispatchResult

| 字段 | 含义 |
| --- | --- |
| `run_ref` | 目标 `LifecycleRun` |
| `graph_instance_ref` | 本次 dispatch 使用或创建的 `WorkflowGraphInstance` |
| `agent_ref` | 承担执行的 `LifecycleAgent` |
| `frame_ref` | 本次可见 runtime surface 的 `AgentFrame` |
| `runtime_session_ref?` | delivery/trace substrate |
| `assignment_ref?` | 若本次直接绑定到 Activity attempt，则返回 assignment |
| `gate_ref?` | wait/adoption/permission 等 durable gate |
| `subject_execution_ref?` | subject/agent/run 视图入口 |
| `trace_ref?` | RuntimeTraceView 入口 |

## same-run 与 linked-run 判定

默认选择 same-run，只有以下边界成立时才创建 linked/spawned `LifecycleRun`：

- 新执行拥有独立生命周期，完成/取消/恢复不能由 parent run 管理。
- 新执行需要独立上下文信道，不能共享 parent run 的 artifact/event/port/VFS exchange。
- 新执行需要独立权限或控制边界。
- 新执行需要独立导航管理或长期跨对象投影。
- 外部系统要求它成为独立可追踪生命周期。

以下情况本身不是 linked-run 判据：

- graph 很复杂。
- 出现 companion/review/task executor 子图。
- 出现新的 RuntimeSession。
- 出现新的 AgentFrame revision。

## 调度顺序

1. 解析 `SubjectRef`、project scope、workflow graph / procedure policy。
2. 根据 run policy 与边界规则选择或创建 `LifecycleRun`。
3. 创建或复用 `WorkflowGraphInstance`。
4. 创建 run-level 或 agent-level `LifecycleSubjectAssociation`。
5. 创建或复用 `LifecycleAgent`。
6. 调用 AgentFrame builder 创建 `AgentFrame` revision。
7. 创建或 attach `RuntimeSession`，并把 refs 写入 frame。
8. 按需创建 `LifecycleGate` 或 `AgentLineage`。
9. 返回 `ExecutionDispatchResult`；runtime delivery 可以同步触发，也可以写入 outbox。

## 边界

- Dispatch service 拥有“选择 run/graph/agent/frame/gate/association”的编排顺序。
- AgentFrame builder 拥有 capability/context/VFS/MCP/procedure/runtime launch projection 的细节。
- Workflow scheduler 拥有 Activity claim、attempt 与 assignment 的执行推进。
- Runtime connector 拥有 event stream 与 turn/tool delivery。
- 业务模块只提交 intent，不拼 `SessionBinding`、owner DTO、`SessionConstructionPlan` 或 connector `ExecutionContext`。

## 首个迁移入口

首个接入点使用 ProjectAgent open。它的风险最低：可以先返回 `run_ref`、`agent_ref`、`frame_ref`、`runtime_session_ref`，再由 Task/Companion/Routine 后续接入同一 service。ProjectAgent 接入完成后，Task start/continue 必须能表达为 `subject_ref=Task`，但可以留到 `task-subject-execution-migration` 实现。
