# Workflow Architecture

## Role

Workflow 子系统表达可执行 graph definition、Activity runtime state 和状态推进规则。目标命名中，`WorkflowGraph` 是当前 `ActivityLifecycleDefinition` 的目标语义；`AgentProcedure` 是当前 `WorkflowDefinition` 的目标语义，表示单个 Agent Activity 的行为/能力/上下文/hook 契约。

## Core Vocabulary

| 概念 | 语义 |
| --- | --- |
| `AgentProcedure` | 单个 Agent Activity 的 behavior / capability / context / hook / port 契约 |
| `WorkflowGraph` | 可执行 Activity DAG definition |
| `LifecycleRun` | tracked life process / control ledger |
| `LifecycleContext` | `LifecycleRun` 内的上下文快照，保存主 AgentRun、AgentRun refs、AgentFrame refs、权限和预算摘要 |
| `OrchestrationInstance` | `LifecycleRun` 内部 0..N 个编排状态容器，保存 source、plan snapshot、runtime node state、dispatch 摘要和 state exchange snapshot |
| `OrchestrationPlanSnapshot` | 静态 graph 或未来 script 编译后的不可变 runtime plan |
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
- `LifecycleRun.context`、`LifecycleRun.orchestrations`、`LifecycleRun.view_projection` 是 orchestration contract 的 owning aggregate 字段；command/service 通过 aggregate 写入这些字段，repository 只做整体持久化。
- 每个已激活的 `WorkflowGraphInstance` 都应在 `LifecycleRun.orchestrations[]` 中拥有对应 `OrchestrationInstance`，原因是一个 Lifecycle 可以同时承载 root workflow、动态 script、review flow 或 subworkflow 等内部状态容器。
- 静态 `WorkflowGraph` 进入 runtime 前先由 application 层 compiler 生成 `OrchestrationPlanSnapshot(plan_digest=sha256:...)`；compiler blocking diagnostics 发生在 run / graph instance 创建前。
- `WorkflowGraphInstance.activity_state` 仍承载当前 Activity scheduler 的推进状态；`LifecycleRun.orchestrations[]` 从 plan activation materialize runtime node snapshot，为 common orchestration scheduler 迁移提供同一运行时合同。
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

`LifecycleRun.topology=graphless` 是普通 Agent runtime 的当前默认形态，只创建 run / agent / frame / runtime session anchor 与 subject association。`LifecycleRun.topology=workflow_graph` 表示显式 Activity 工作流，拥有 root graph id、`WorkflowGraphInstance`、Activity state、`OrchestrationInstance` 与 `AgentAssignment`。

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
| `workflow/value_objects/orchestration.rs` | Lifecycle-owned orchestration contract、plan snapshot、runtime node state、dispatch/state exchange/journal fact value types |
| `workflow/validation.rs` | Workflow contract validation、Lifecycle DAG validation、Activity lifecycle transition/port/policy validation |

## Local Decisions

- 普通 Agent runtime 默认使用 graphless 拓扑，原因是多数 Agent 会话只需要控制面、runtime trace 与 subject 归属；Activity graph 只在需要节点流转、attempt state、claim 和 assignment 的显式 workflow 中引入。
- 业务 result 不平铺 run / graph / agent / frame / assignment refs，原因是 run / agent / frame 是通用控制面，而 graph / assignment 是 Activity binding；统一 envelope 可以让调用方先处理 Agent runtime，再按拓扑决定是否进入 Activity 细节。
- artifact edge 自动提供 flow dependency，原因是数据依赖本身已经表达执行顺序，重复 flow edge 会制造两套 dependency 事实。
- `RuntimeSessionExecutionAnchor` 是 runtime trace/delivery refs 的索引和 read model projection 来源，原因是运行时 trace 反查需要稳定索引，且不应随 frame revision surface 变化。

## Scenario: Lifecycle Orchestration Contract

### 1. Scope / Trigger

- Trigger: 为 `LifecycleRun` 增加或消费 `context`、`orchestrations`、`view_projection` 字段，或修改 `OrchestrationInstance` / `OrchestrationPlanSnapshot` / `RuntimeNodeState` 等合同。
- Scope: domain value objects、`LifecycleRun` aggregate、`LifecycleRunRepository`、workflow runtime 事实源边界。

### 2. Signatures

Domain aggregate:

```rust
pub struct LifecycleRun {
    pub context: LifecycleContext,
    pub orchestrations: Vec<OrchestrationInstance>,
    pub view_projection: Option<serde_json::Value>,
}
```

Aggregate methods:

```rust
set_lifecycle_context(context: LifecycleContext)
add_orchestration(orchestration: OrchestrationInstance) -> bool
replace_orchestration(orchestration: OrchestrationInstance) -> Option<OrchestrationInstance>
orchestration_by_id(orchestration_id: Uuid) -> Option<&OrchestrationInstance>
```

PostgreSQL columns on `lifecycle_runs`:

```sql
context text DEFAULT '{}'::text NOT NULL
orchestrations text DEFAULT '[]'::text NOT NULL
view_projection text
```

### 3. Contracts

- `context` 保存 Lifecycle 级上下文引用，不内嵌完整 AgentFrame surface。
- `orchestrations` 保存同一 Lifecycle 内 0..N 个内部编排实例；静态 graph orchestration 的 `source_ref` 使用 `WorkflowGraph { graph_id, graph_version, graph_instance_id }`，其中 `plan_snapshot.source_ref` 保留 definition 级来源，instance `source_ref` 绑定本次 graph instance。
- `OrchestrationPlanSnapshot.plan_digest` 是 compiled plan 内容身份；runtime、journal 和 projection 按 digest 判断 plan 合同，不使用 graph instance UUID 作为 plan 身份。
- graph-backed dispatch 创建或复用 `WorkflowGraphInstance` 时，为该 instance materialize entry ready nodes、dispatch ready queue 和空 `StateExchangeSnapshot`。
- `view_projection` 是 read projection 占位，command/service 不从该字段反向推导 runtime state。
- `WorkflowGraphInstance.activity_state` 仍服务当前 Activity scheduler 推进；common runtime scheduler 接管后，scheduler、terminal callback 与 projection 将消费 orchestration runtime node state。

### 4. Validation & Error Matrix

| 条件 | 结果 |
| --- | --- |
| `LifecycleRun` constructor 创建 graphless / workflow_graph run | `context={}`、`orchestrations=[]`、`view_projection=None` |
| `add_orchestration` 收到重复 `orchestration_id` | 返回 `false`，不修改 aggregate |
| `replace_orchestration` 找不到 `orchestration_id` | 返回 `None` |
| repository 读取无效 `orchestrations` JSON 文本 | 返回带 `lifecycle_runs.orchestrations` 上下文的 `DomainError` |
| graph-backed dispatch 的 compiler 产生 blocking diagnostics | 返回 `BadRequest`，且不创建 run / graph instance |
| service 创建或复用 `WorkflowGraphInstance` | 生成对应 `OrchestrationInstance`，entry semantic node 处于 `Ready` |

### 5. Good/Base/Bad Cases

- Good: `LifecycleRunRepository` create/update/select 对 `context`、`orchestrations`、`view_projection` 做整体 roundtrip。
- Base: graphless run 没有 orchestration instance，但仍能保存空 context 和空数组。
- Good: 同一 run 内 root graph 与 append graph 分别拥有独立 `OrchestrationInstance.source_ref.graph_instance_id`，从而共享 Lifecycle 容器但隔离 runtime node state。

### 6. Tests Required

- Domain serde roundtrip 覆盖 `OrchestrationPlanSnapshot`、`RuntimeNodeState`、`OrchestrationJournalFact`。
- `LifecycleRun` aggregate 测试覆盖 0、1、多个 `OrchestrationInstance`。
- Repository row parsing 测试覆盖新列默认值、坏 JSON 错误上下文。
- PostgreSQL repository roundtrip 测试覆盖 create -> get -> update -> get。
- Application compiler/runtime 测试覆盖静态 graph plan digest、semantic node kind、artifact state exchange、blocking diagnostics 与 entry ready node materialization。
- Dispatch service 测试覆盖 start lifecycle run、普通 graph dispatch、append graph 和 compiler preflight failure。
- 触及 migration 时运行 `pnpm run migration:guard`。

### 7. Current Runtime Entry

静态 graph 的当前接入点是 creation-time activation：compiler 输出不可变 plan，runtime helper materialize 对应 graph instance 的 initial `OrchestrationInstance`。后续 scheduler 迁移应继续消费同一 `OrchestrationPlanSnapshot` 和 `RuntimeNodeState` 合同推进节点、state exchange 与 terminal materialization。

## Contract Appendices

- [Activity Lifecycle Backend Contract](./activity-lifecycle.md)
- [Lifecycle Edge](./lifecycle-edge.md)
- [Story / Task Runtime](../story-task-runtime.md)
- [Activity Lifecycle Frontend Contract](../../frontend/workflow-activity-lifecycle.md)
