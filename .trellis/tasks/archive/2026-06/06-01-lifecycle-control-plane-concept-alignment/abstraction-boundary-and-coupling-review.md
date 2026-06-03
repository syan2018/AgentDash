# Lifecycle 控制面抽象层级与耦合边界评估

## Purpose

本文专门评估当前 Lifecycle 控制面重构是否存在过度封装风险，并给出高内聚、低耦合的实施约束。

本次重构的目标不是把 `Session`、`Task`、`Companion` 的历史耦合换成一串更长的新链路。目标是让每个模块只对自己拥有的事实负责，让跨模块协作通过少数稳定对象完成。

## Core Concern

当前目标模型引入了 `LifecycleRun`、`LifecycleAgent`、`AgentFrame`、`AgentAssignment`、`LifecycleSubjectAssociation`、`LifecycleGate`、`RuntimeSession` 等概念。它们可以形成清晰的事实源边界，也可能滑向过度封装：

```text
入口 -> DispatchService -> Agent -> Frame -> Assignment -> RuntimeSession -> Projection -> UI
```

如果每一层只是接收上层字段、改个名字再传给下一层，这就是过度抽象。它会造成两个坏结果：

- 任一命名调整都跨越多层 DTO、repository、store、route。
- 开发者为了查一个事实，需要穿过三四次没有业务判断的转发链。

因此本轮重构必须把“新增抽象”当成预算，而不是默认收益。

## Abstraction Budget

新增独立概念必须至少满足下面一条，否则应合并为字段、value object、projection 或 helper function。

| 判断问题 | 可以独立成层 | 不应独立成层 |
| --- | --- | --- |
| 是否拥有事实源？ | 有独立 truth、状态转移、持久化生命周期 | 只是缓存另一个对象的字段 |
| 是否维护不变量？ | 能阻止错误状态组合 | 只把参数从 A 传到 B |
| 是否改变查询边界？ | 有明确索引、查询入口或权限边界 | 只有一个 caller 且无独立查询 |
| 是否承担生命周期？ | 有 create/update/terminal/cleanup | 没有状态，只是组装结构 |
| 是否隔离外部依赖？ | 屏蔽 runtime connector、MCP、权限、UI 差异 | 把外部类型原样外露给下一层 |

这意味着：

- `LifecycleAgent` 可以成立，因为它拥有 LifecycleRun 内的 Agent 运行身份和状态。
- `AgentFrame` 可以成立，因为它拥有 effective capability / context / VFS / MCP / procedure / runtime refs 的 revision。
- `AgentAssignment` 可以成立，因为它是 Agent 与 ActivityAttemptState 的执行证据桥。
- `LifecycleSubjectAssociation` 可以成立，因为它是 SubjectRef 与 run / LifecycleAgent anchor 的关联事实。
- `LifecycleDispatchService` 可以成立，但它必须是入口编排服务，不应成为所有字段的长期中转站。

相反，下面这些不应变成独立模型：

- 只包装 `RuntimeSessionId` 的 `RuntimeRef` 表。
- 只把 `AgentFrame` 原样转换成 connector 参数的多层 `Plan` / `Spec` / `LaunchPayload`。
- 只为某个 route 定制、没有独立语义的 `TaskAgentBinding`。
- 只把 `SubjectRef` 再命名一次的 `TaskRuntimeSubject`。
- 只复述 ActivityAttemptState 终态的 `AgentAssignment.status`。

## Required Layers

从高内聚角度看，目标模型最多需要这些稳定层：

```text
Business Subject
  Story / Task / Project / RoutineExecution

Lifecycle Control Plane
  LifecycleRun / Workflow / Activity / Artifact / Gate

Agent Runtime Identity
  LifecycleAgent / AgentFrame / AgentAssignment / EffectiveCapability

Runtime Substrate
  RuntimeSession / connector turn / tool call / event log

Projection
  SubjectExecutionView / RuntimeTraceView / ProjectAgentLaunchView
```

每层都必须回答一个不同问题：

| 层 | 回答的问题 | 不应该回答的问题 |
| --- | --- | --- |
| Business Subject | 用户正在看什么业务对象？ | 哪个 session 正在跑？ |
| Lifecycle Control Plane | 这个执行生命过程如何推进？ | connector 如何组织 prompt loop？ |
| Agent Runtime Identity | 哪个 Agent 以什么有效环境执行？ | Task spec 里是否保存 runtime truth？ |
| Runtime Substrate | turn/tool/event/resume 发生了什么？ | Story/Task/Permission 的 ownership 是什么？ |
| Projection | UI 如何聚合可读状态？ | 新事实源在哪里？ |

如果某个对象同时回答多列问题，就说明它开始过载。

## Collapse Candidates

当前计划里有几处需要防止层级过厚。

### Dispatch Result

`LifecycleDispatchService` 不应返回一堆需要调用方继续拼装的散字段，也不应把 frame builder、capability resolver、runtime connector 的内部细节收入自己名下。它应只接受 `ExecutionIntent`，只返回一个稳定的聚合结果：

```text
ExecutionDispatchResult
  run_ref
  agent_ref
  frame_ref
  runtime_session_ref
  subject_execution_view?
  gate_ref?
```

调用方拿到结果后只做业务投影或导航，不再自己反查 run、agent、frame、session。

`LifecycleDispatchService` 的价值是事务边界和 owner service 调用顺序，而不是成为“所有字段路过的地方”。它可以调用：

```text
WorkflowResolver
LifecycleRunRepository
LifecycleAgentRepository
AgentFrameBuilder
RuntimeSessionConnector
```

但这些组件的内部 plan、connector request、event schema 不进入 Story / Task / Companion / ProjectAgent 模块。

### Frame Construction

`AgentFrameConstructionPlan` 与 runtime launch request 可以存在，但边界要清楚：

- `AgentFrameConstructionPlan` 是 frame builder 的内部输入/输出。
- `RuntimeLaunchRequest` 是 runtime-session adapter 的内部输入。
- route、Task service、Companion service 不应直接操作这两个 plan。
- 它们不进入 generated contract，不持久化，不跨 business module import。

如果二者长期只是一对一传递，可以把 runtime launch request 作为 `AgentFrame` 的 projection method，而不是独立事实。

`AgentFrame` 自身也不能膨胀成新的 SessionMeta。Frame 拥有的是某个 revision 的 effective runtime surface；内部结构优先使用 frame-owned value object：

```text
EffectiveCapability
ContextSlice
VfsSurface
McpSurface
RuntimeSessionRef[]
```

这些对象默认不建 repository、不暴露给业务模块。只有当它们拥有独立查询、生命周期或权限边界时，才允许升级为独立模型。

### Capability State

`EffectiveCapability` 应属于 `AgentFrame`，不应再拆出平行的 `CapabilityState` ownership。可以有 resolver，可以有 view，但 truth 应落在 frame revision。

### Assignment State

`AgentAssignment` 是 LifecycleAgent 与 Activity / ActivityAttemptState 的执行关系，不是 executor 终态的副本。

推荐边界：

- `AgentAssignment` 记录 assigned / released / cancelled 等 assignment lease state。
- `ActivityAttemptState` 记录 running / completed / failed、terminal reason 和 artifact evidence。
- `RuntimeSession` 记录 turn / tool / event / resume trace。
- UI 需要聚合时通过 projection 读取，不把聚合结果写回任何一个事实源。

### Agent Lineage

如果 parent-child 只用于 “谁 spawn 了谁”，可以先放在 `LifecycleSubjectAssociation.metadata` 或 `AgentLineage` value object。只有当 lineage 需要独立查询、关闭子树、恢复子树、权限继承时，才升级成表或 repository。

### Runtime Trace View

`RuntimeTraceView` 是 UI / debug projection，不是领域层事实。代码事实仍是 `RuntimeSession`。避免再造 `Trace`、`SessionTrace`、`RuntimeSessionTrace` 三套实体。

## Coupling Rules

### Rule 1: Business Modules Only Speak SubjectRef And Dispatch Result

Story / Task / ProjectAgent / Routine / Companion 入口只允许依赖：

```text
SubjectRef
ExecutionIntent
ExecutionDispatchResult
SubjectExecutionView
```

它们不应直接依赖：

```text
AgentFrameConstructionPlan
RuntimeLaunchRequest
ActivityAttemptState internals
RuntimeSession event schema
```

这样 Task 后续字段改名，不会穿透到 runtime launch；RuntimeSession 改 connector，也不会穿透到 Task domain。

### Rule 2: RuntimeSession Never Resolves Business Ownership

RuntimeSession 可以被查到所属 `LifecycleAgent` 或 `AgentFrame`，但它不应自己解析 Story / Task / Project owner。

正确方向：

```text
RuntimeSession -> AgentFrame -> LifecycleAgent -> LifecycleSubjectAssociation -> SubjectRef
```

不应出现：

```text
RuntimeSession -> SessionBinding -> Story/Task
RuntimeSession -> owner_type / owner_id
```

### Rule 3: ActivityAttemptState Is Evidence, Not Routing Root

`ActivityAttemptState` 可以记录执行状态和输出证据，但不应成为 subject association anchor，也不应直接决定 tool/context/capability。

执行关系通过：

```text
LifecycleAgent -> AgentAssignment -> ActivityAttemptState
```

工具、上下文和权限通过：

```text
LifecycleAgent -> AgentFrame
```

### Rule 4: Projection Can Depend On Many Facts, Commands Cannot

UI projection 可以聚合 run、agent、frame、assignment、artifact、subject、runtime trace。Command path 不应依赖这种多源聚合结果再写回事实源。

也就是说：

- `SubjectExecutionView` 可以很宽。
- `StartTaskExecutionCommand` 必须很窄，只提交 `SubjectRef + ExecutionIntent`。
- `SubjectExecutionView`、`RuntimeTraceView`、`ProjectAgentLaunchView` 只能用于 read model 和 UI 导航，不作为 command input 回传。

### Rule 5: No Cross-Layer Rename Chains

跨三四层链路改名通常说明字段传递方式错了。目标上应让中间层传稳定引用，而不是拆散字段。

推荐：

```text
SubjectRef { kind, id }
LifecycleRunRef { run_id }
LifecycleAgentRef { agent_id }
AgentFrameRef { agent_id, frame_id, revision }
RuntimeSessionRef { runtime_session_id }
AgentAssignmentRef { assignment_id }
LifecycleGateRef { gate_id }
```

避免：

```text
task_agent_session_id
project_agent_binding_id
workflow_step_session_id
companion_parent_session_id
```

字段一旦带上具体业务前缀，就会在下一次概念调整时产生链式改名。

业务入口固定在两个边界对象上：

```text
ExecutionIntent {
  subject_ref,
  workflow_key,
  agent_policy,
  context_policy,
  capability_policy,
  runtime_policy
}

ExecutionDispatchResult {
  run_ref,
  agent_ref,
  frame_ref,
  runtime_session_ref?,
  gate_ref?,
  subject_execution_view?
}
```

只要跨模块传这些稳定引用，中间层就不需要反复拆字段、换前缀、再传下一层。

## Module Ownership

建议把 ownership 写死：

| Module | Owns | May Read | Must Not Own |
| --- | --- | --- | --- |
| workflow/lifecycle | LifecycleRun, Workflow, Activity, Artifact, Gate | SubjectRef | RuntimeSession event details |
| agent-runtime-control | LifecycleAgent, AgentFrame, AgentAssignment, EffectiveCapability | Procedure, PermissionGrant | Story/Task spec truth |
| runtime-session | RuntimeSession, turn, tool call, connector resume | AgentFrameRef | business owner |
| story/task | Story, Task, Task projection cache | SubjectExecutionView | runtime session id |
| companion | CompanionChannel, companion interaction payload | LifecycleGate, AgentLineage | child runtime ownership |
| permission | PermissionGrant, scope decision | AgentFrameRef, SubjectRef | tool surface projection cache |
| frontend stores | normalized views and route state | generated DTOs | backend fact derivation |

实际包名可以调整，但 ownership 不应漂移。

## Implementation Guardrails

### Guardrail 1: One Stable Ingress

所有业务入口只进 `LifecycleDispatchService`。不允许出现第二套 `TaskSessionLauncher`、`CompanionSessionLauncher`、`RoutineSessionLauncher`。

`LifecycleDispatchService` 只做编排和事务收口；如果实现开始承载 frame 构造细节、connector payload、capability merge 规则，就应把这些逻辑移回对应 owner module。

### Guardrail 2: One Stable Egress

业务 UI 消费 `SubjectExecutionView` 或 `ProjectAgentLaunchView`。Runtime 调试 UI 消费 `RuntimeTraceView`。不要让每个页面各自拼 route-local binding response。

### Guardrail 3: One Effective Runtime Surface

Connector launch 只能从 `AgentFrame` 投影，不从 Task、ProjectAgent、SessionMeta、HookRuntime 各自取字段。

### Guardrail 4: Thin Services Must Merge Back

如果一个 service 连续两个迭代只做参数转发，没有不变量、事务边界或外部依赖隔离，应合并回 owner service 或降为 private helper。

### Guardrail 5: Generated Contracts Are Boundary Objects

跨前后端的 DTO 应是业务稳定对象，而不是内部表结构镜像。内部表可重命名，DTO 不应跟着每个 repository 字段抖动。

## Practical Shape

一个健康的 Task execution 调用链应长这样：

```text
TaskService
  -> LifecycleDispatchService.dispatch(ExecutionIntent { subject_ref: Task })
  -> returns ExecutionDispatchResult
  -> TaskService returns SubjectExecutionView
```

它不应该长这样：

```text
TaskService
  -> TaskAgentBindingResolver
  -> SessionConstructionPlanner
  -> StepActivationBuilder
  -> WorkflowRunLinkResolver
  -> SessionLauncher
  -> TaskProjectionUpdater
```

前者只有一个跨模块入口和一个跨模块出口；后者每个中间层都会在命名调整时受影响。

## Refactor Health Checks

每个阶段完成后都应问：

1. 新增对象是否拥有事实源或不变量？
2. 有没有 service 只是转发 DTO？
3. 业务模块是否仍然直接传 `session_id`？
4. UI 是否仍然从 route-local binding shape 推导 runtime？
5. 一个字段改名是否会影响超过两个 owner module？
6. Connector launch 是否只依赖 AgentFrame projection？
7. Task / Story / ProjectAgent 是否只通过 SubjectRef 进入 runtime？

如果第 5 条答案是“会”，说明当前边界还没有稳定。

## Conclusion

`LifecycleAgent / AgentFrame / AgentAssignment` 不是问题本身；问题在于是否让它们成为明确事实源，还是把它们做成更多中转层。

推荐保留这些核心概念，但压缩外围抽象：

- 保留 `LifecycleAgent`，因为它是 runtime identity。
- 保留 `AgentFrame`，因为它是 effective runtime surface。
- 保留 `AgentAssignment`，因为它是 execution evidence bridge。
- 保留 `LifecycleSubjectAssociation`，因为它是 subject anchor。
- 保留 `LifecycleGate`，因为 wait/resume 必须 durable。
- 谨慎对待所有 `Plan`、`Binding`、`Resolver`、`Launcher`、`ViewBuilder`，没有独立不变量就合并。

这样模型能朝高内聚低耦合推进，同时避免把一次重构变成多层传话游戏。
