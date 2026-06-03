# Workflow Architecture

## Role

Workflow 子系统表达可执行 graph definition、Activity runtime state 和状态推进规则。目标命名中，`WorkflowGraph` 是当前 `ActivityLifecycleDefinition` 的目标语义；`AgentProcedure` 是当前 `WorkflowDefinition` 的目标语义，表示单个 Agent Activity 的行为/能力/上下文/hook 契约。

## Core Vocabulary

| 概念 | 语义 |
| --- | --- |
| `AgentProcedure` | 单个 Agent Activity 的 behavior / capability / context / hook / port 契约 |
| `WorkflowGraph` | 可执行 Activity DAG definition |
| `LifecycleRun` | tracked life process / control ledger |
| `LifecycleRunTopology` | run 的控制面拓扑：`graphless` 表示普通 Agent runtime，`workflow_graph` 表示显式 Activity graph runtime |
| `WorkflowGraphInstance` | 一个 run 内的 graph 生效实例与 activity state namespace |
| `ActivityAttemptState` | `graph_instance_id + activity_key + attempt` 定位的一次 activity attempt |
| `ActivityExecutionClaim` | scheduler 对 attempt 的 durable claim |
| `LifecycleAgent` | run-scoped Agent runtime identity |
| `AgentRuntimeRefs` | Agent runtime 控制面引用，统一携带 run / agent / frame，并通过可选 `ActivityBindingRefs` 表达 Activity-only binding |
| `ActivityBindingRefs` | 显式 Activity runtime binding，携带 graph instance 与 assignment 引用 |
| `AgentAssignment` | activity attempt 到 `LifecycleAgent` 与 `AgentFrame` 的执行绑定 |
| `AgentFrame` | Agent runtime surface revision，承载 capability、context、VFS 与 MCP |
| `RuntimeSession` | connector delivery / trace evidence |
| `RuntimeSessionExecutionAnchor` | `RuntimeSession` 反查 run / agent / frame / assignment / attempt 的权威索引 |
| `LifecycleSubjectAssociation` | `SubjectRef` 到 whole run 或 agent 的业务归属关联 |

## Invariants

- `WorkflowGraph` 是 workflow 运行、编辑和观察的主模型。
- `LifecycleRun` 是 tracked life process / control ledger；`topology=workflow_graph` 可以包含多个 `WorkflowGraphInstance`。
- `WorkflowGraphInstance.activity_state` 是 Activity runtime state 的权威状态源；`LifecycleRun.active_node_keys` 只是从 graph instances 派生的 read-model projection。
- Activity / attempt runtime key 必须包含 `graph_instance_id`，避免同一 run 内多个 graph instance 的 activity key 冲突。
- durable state advancement 只能通过 ActivityEvent 进入 `LifecycleEngine`。
- Scheduler 负责 durable claim 和 executor 启动；executor 只通过事件把结果交还给 engine。
- Function executor 即使立即完成，也必须产出 Activity terminal event，而不是直接修改 run state。
- Agent Activity execution identity 使用 `AgentAssignment(run_id, graph_instance_id, activity_key, attempt, agent_id, frame_id)` 定位当前 work；RuntimeSession 只作为 terminal/runtime evidence。
- 通过 `RuntimeSession` 反查 Lifecycle Activity 时，必须使用 `RuntimeSessionExecutionAnchor`，再进入 `LifecycleAgent -> AgentFrame -> AgentAssignment` 的证据链。
- `LifecycleSubjectAssociation` 是 Task / Story / Routine / Project 等业务 subject 的归属入口；业务状态不能由 `RuntimeSession` title、存在性或 trace 内容推断。
- Lifecycle edge 只有 `flow` 和 `artifact` 两类；artifact edge 隐含 node-level flow dependency。
- 多 activity lifecycle 必须显式声明 edges；运行时不按数组顺序推断推进路径。
- `workflow/value_objects.rs` 是可序列化 Workflow value types 的 facade；具体类型按 binding、contract、capability、mount directive、hook rule、ports、lifecycle definition、activity definition、run state 子模块组织。`workflow/validation.rs` 承载 definition、topology 与 activity lifecycle 校验。类型定义和校验分离，原因是持久化契约与规则演进有不同的变化节奏。

## Current Baseline

| 文档 | 当前职责 |
| --- | --- |
| `activity-lifecycle.md` | Activity executor、run startup、template install/update、drop-step migration 契约 |
| `lifecycle-edge.md` | DAG edge kind、校验、运行时推进规则 |
| `lifecycle-run-link.md` | LifecycleSubjectAssociation 关联层、Session 降级、subject/agent/run-oriented API 契约 |
| `../story-task-runtime.md` | Story / Task / Session / LifecycleRun 关系拓扑 |
| `../../frontend/workflow-activity-lifecycle.md` | 前端 Activity lifecycle 编辑与运行观察契约 |

`LifecycleRun.topology=graphless` 是普通 Agent runtime 的当前默认形态，只创建 run / agent / frame / runtime session anchor 与 subject association。`LifecycleRun.topology=workflow_graph` 表示显式 Activity 工作流，拥有 root graph id、`WorkflowGraphInstance`、Activity state 与 `AgentAssignment`。

跨业务 dispatch / cancel / routine response 使用 `AgentRuntimeRefs` 作为统一控制面 envelope。`WorkflowGraphInstance` 与 `AgentAssignment` 通过 `ActivityBindingRefs` 挂在 envelope 内部，只在显式 Activity runtime 中出现。

## Module Boundaries

| 模块 | 当前职责 |
| --- | --- |
| `workflow/value_objects.rs` | public facade 与 value object 测试入口 |
| `workflow/value_objects/binding.rs` | Workflow binding scope 类型与 owner 映射 |
| `workflow/value_objects/contract.rs` | Workflow contract、session terminal state、effective session contract |
| `workflow/value_objects/capability.rs` | CapabilityConfig、tool capability path / directive / reduction |
| `workflow/value_objects/mount_directive.rs` | VFS mount capability directive wire types |
| `workflow/value_objects/hook_rule.rs` | Workflow hook trigger 与 rule spec |
| `workflow/value_objects/ports.rs` | input/output port、gate/context strategy、standalone fulfillment |
| `workflow/value_objects/lifecycle_def.rs` | 当前 ActivityLifecycleDefinition 迁移来源；目标语义是 WorkflowGraph |
| `workflow/value_objects/activity_def.rs` | Activity definition、executor、completion/iteration/join/transition policy |
| `workflow/value_objects/run_state.rs` | Activity / lifecycle runtime state value types |
| `workflow/validation.rs` | Workflow contract validation、Lifecycle DAG validation、Activity lifecycle transition/port/policy validation |

## Local Decisions

- 普通 Agent runtime 默认使用 graphless 拓扑，原因是多数 Agent 会话只需要控制面、runtime trace 与 subject 归属；Activity graph 只在需要节点流转、attempt state、claim 和 assignment 的显式 workflow 中引入。
- 业务 result 不平铺 run / graph / agent / frame / assignment refs，原因是 run / agent / frame 是通用控制面，而 graph / assignment 是 Activity binding；统一 envelope 可以让调用方先处理 Agent runtime，再按拓扑决定是否进入 Activity 细节。
- artifact edge 自动提供 flow dependency，原因是数据依赖本身已经表达执行顺序，重复 flow edge 会制造两套 dependency 事实。
- `RuntimeSessionExecutionAnchor` 是 runtime trace/delivery refs 的索引和 read model projection 来源，原因是运行时 trace 反查需要稳定索引，且不应随 frame revision surface 变化。

## Contract Appendices

- [Activity Lifecycle Backend Contract](./activity-lifecycle.md)
- [Lifecycle Edge](./lifecycle-edge.md)
- [Story / Task Runtime](../story-task-runtime.md)
- [Activity Lifecycle Frontend Contract](../../frontend/workflow-activity-lifecycle.md)
