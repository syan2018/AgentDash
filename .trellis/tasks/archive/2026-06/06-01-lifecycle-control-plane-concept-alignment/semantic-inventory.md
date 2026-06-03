# Lifecycle 控制面语义盘点

## Purpose

重新梳理 Lifecycle 相关存量概念的目标语义。本文不做实现承诺，先记录概念边界和收束方向。

## 核心判断

AgentDash 的目标不是把所有执行关系压进 Session，也不是把所有上下文压进 LifecycleRun。更核心的目标是把散落的 Agent runtime facts 收束到 Lifecycle 这条线上，由 Actor 作为 RuntimeSession 的高层封装，由 ActorFrame 管理有效 capability / context / VFS / MCP / procedure / runtime refs。

更合适的结构是：

```text
Lifecycle
  追踪执行生命过程：Actor 状态、Activity 状态、ActivityAttemptState、能力/上下文变化、等待、产物和因果。

Workflow
  在 Lifecycle 下生效的可执行图配置实例：Activities、Transitions、Ports、Artifacts、Join/Branch。

LifecycleTrack / LifecycleRun
  是某个具体执行生命过程的追踪记录。

Lifecycle Association
  把 Story / Task / RoutineExecution / Project / External / parent lifecycle
  关联到 Lifecycle 内的某个 anchor：whole lifecycle 或 Actor。

RuntimeSession
  只是 Agent runtime event log / turn / tool call / resume / debug substrate。
```

## Concepts

### Lifecycle

目标语义：Agent 执行生命过程的追踪平面。它追踪一组 Actor / Activity / ActivityAttemptState 如何演化、如何交换信息、如何等待、如何被能力与上下文变化影响。

它应承载：

- Workflow 生效状态。
- lifecycle-level ports / artifacts / VFS。
- Activity / ActivityAttemptState 状态。
- Actor 状态与 revision。
- Event-driven advancement。
- 跨 Activity 的信息交换与因果记录。

它不应承载：

- Story / Task 的业务 truth。
- PermissionGrant 的授权 truth。
- RuntimeSession 的 event log truth。
- Companion interaction 的全部持久事实。

### Workflow

目标语义：在 Lifecycle 下生效的可执行图配置实例。

它描述可执行拓扑：

- 哪些 Activity 存在。
- Activity 之间如何 transition。
- 哪些 Activity 并发、join、等待 human/function/agent。
- 产物如何通过 ports/artifacts 流动。

它不是某个 Agent Activity 的 prompt/procedure。

### LifecycleTrack / LifecycleRun

目标语义：某个具体执行生命过程的追踪记录。

它拥有运行事实：

- Activity state。
- ActivityAttemptState state。
- Execution events。
- Lifecycle artifacts。

它通过 association 连接业务对象，但不把业务对象变成自己的字段。

### Activity

目标语义：Lifecycle 内的执行节点和控制边界。

Activity 不是“一个普通动作”，而是 Lifecycle 下 Workflow graph 中可调度、可观察、可完成、可产出 artifact 的执行槽位。

Activity 可以由不同 executor 执行：

- Agent Activity。
- Function Activity。
- Human Activity。
- 后续可能的 Companion/Interaction Activity，或由 Agent Activity 派发 companion。

Activity 应承载：

- executor spec。
- input/output ports。
- completion policy。
- iteration/retry policy。
- join/branch 参与关系。

Activity 不应天然等同于 Task。Task 是业务数据、用户查看对象或 Activity payload 指向的数据对象；runtime 侧只携带 `SubjectRef(kind=Task)`。Actor 处理的是 SubjectRef，之后再通过 assignment 关联到 ActivityAttemptState。

### ActivityAttemptState

目标语义：Activity 的一次 executor execution record。既然当前代码已经使用 `ActivityAttemptState`，就不再为了概念洁癖额外引入 `ActivityInvocation`。`Attempt` 在这里表达运行记录/序号，而不是要求产品侧围绕 retry 建模。

ActivityAttemptState 记录 Activity 被某个 executor 承接后的执行事实：

- executor_run_ref。
- started/completed/status。
- attempt-level output artifacts。

Task view 如果需要精确解释“哪个 agent 产生了这份投影”，应先从 `SubjectRef(kind=Task)` 指向 Actor，再通过 Actor assignment 指向 ActivityAttemptState。Task 本体不因此拥有 runtime 语义。

### LifecycleActor / AgentStateAnchor

目标语义：`RuntimeSession` 之上的高层封装。Actor 是 LifecycleRun 内锚定 Agent 运行身份、有效环境与状态变化的管理对象。

它填补 `LifecycleRun`、ActivityAttemptState、`RuntimeSession` 之间的空隙：

- `LifecycleRun` 是大控制面。
- ActivityAttemptState 是某个 Activity 的一次执行记录。
- `RuntimeSession` 是日志/turn/resume 载体。
- `LifecycleActor` 是 Session runtime 之上的高层运行封装，描述某个 Agent 在这个控制面内的当前运行身份、状态和有效环境。

它可能承载：

- actor id / role / agent assignment。
- 当前或历史 activity assignment。
- effective workflow contract ref。
- effective capability / MCP / VFS / context projection revision。
- runtime refs：RuntimeSession、turn id、connector resume ref。
- state status：ready/running/waiting/blocked/completed/failed/suspended。
- state transition source：ActivityEvent、RuntimeCommand、InteractionGate、PermissionGrant。

它不应承载：

- 完整 business truth。
- 完整 session event log。
- 不可追踪的临时工具面变更。

核心价值：capability、context、VFS、MCP、RuntimeSession refs 等 runtime surface 有一个自上而下的事实管理层。Activity 可以改变 Actor 状态，但变化应通过 Actor revision / frame 被追踪，而不是隐式落在 SessionMeta、ActivityAttemptState 或 live runtime map 中。

### ActivityProcedure / ActorProcedure

目标语义：单个 Agent Activity 的局部行为/能力/上下文契约。

它不应描述整个 Workflow graph。它应描述：

- Agent Activity 内部如何被提示。
- 注入哪些 workflow context。
- 可见哪些 tools / MCP / VFS。
- 受哪些 hook / completion constraints 影响。

更准确的候选名：

- `ActivityProcedure`
- `ActorProcedure`
- `ActivityExecutionContract`

### RuntimeSession

目标语义：运行时载体。

它承载：

- event log。
- turn/tool call stream。
- connector resume state。
- debug replay。
- runtime context transitions。

它不应表达：

- Story ownership。
- Task ownership。
- Permission scope。
- Lifecycle progress truth。

### Companion

目标语义需要拆分：

- Companion Channel：跨 human / platform / parent / sub 的交互通道。
- Companion Agent：可被派发的 agent role。
- Lifecycle-backed companion execution：某个 Activity 或 run 中受 lifecycle/workflow 约束的 agent 执行。

Companion 不应自己成为权限 truth 或业务 ownership truth。

### Story

目标语义：业务工作单元。

它持有：

- 标题、上下文、优先级等业务信息。
- Task specs。
- 用户可见投影。

它不持有 runtime truth。Story 与 Lifecycle 的关系通过 lifecycle association 表达。

### Task

目标语义：Story 下的业务工作项 spec + view projection。

Task 是 Story 下的业务工作项数据、Activity payload 可引用的数据对象，以及用户查看投影的聚合入口。Task 不直接持有 executor session id 或 runtime truth，也不定义运行时逻辑。Task view 的执行状态应从 `SubjectRef(kind=Task)`、Actor association、Actor assignment、Lifecycle Activity / ActivityAttemptState 与 artifacts 投影得到。

Task 可以有两种运行形态：

1. 作为同一 Story LifecycleRun 内 Activity payload / SubjectRef，由 Actor 处理，并经由 Actor assignment 追溯 ActivityAttemptState。
2. SubjectRef 拥有独立 LifecycleRun，仅在确实有独立上下文信道、控制边界或生命周期时使用；Task 本体仍只是数据和视图对象。

默认不应因为 Task 存在就创建独立 run。

### Lifecycle Association

目标语义：Lifecycle / Actor 与业务对象、来源、投影、权限范围的关系。

当前代码名是 `LifecycleRunLink`，但目标语义可能只需要扩展到 Actor anchor：

```text
anchor = run | actor
```

建议形态：

```text
LifecycleAssociation
- run_id
- actor_id: Option<String>
- subject_kind
- subject_id
- role
- metadata
```

这让同一概念覆盖：

- Story 与 whole run 的关系。
- RoutineExecution 作为 run source。
- SubjectRef(kind=Task) 与某个 Actor 的执行关系。
- parent run 派生另一个 independent run 的 lineage。

Activity 与 ActivityAttemptState 不需要成为 subject association anchor。它们通过 Actor assignment、execution record、artifacts 与 event log 提供执行证据；Subject association 负责说明哪个业务对象属于哪个 lifecycle / actor 运行语境。

### Dispatch

目标语义：把一个 execution intent 解析成 Activity launch 或 independent run launch 的 use case。

Dispatch 不应散落在 task service、companion tool、routine executor、project agent route 中。它应共享：

- lifecycle definition resolution。
- association creation。
- context projection。
- capability projection。
- runtime session attachment。

### Projection

目标语义：从事实源派生出来的视图或运行时表面。

Projection 不应拥有 truth。Task status、tool visibility、active workflow view、session construction plan 都应明确自己从哪些事实源派生。

## Open Semantic Decisions

- `Activity` 是否应被产品侧称为 "Lifecycle Activity"，避免和普通 action 混淆？
- 当前 `WorkflowDefinition` 是否应改名为 `ActivityProcedure` / `ActorProcedure` / `ActivityExecutionContract`？
- `LifecycleRunLink` 是否应改名为 `LifecycleSubjectAssociation` 并增加 actor anchor？
- `SubjectRef(kind=Task)` 默认执行形态是否应明确为 same-run Actor assignment，而不是 independent run？
- Companion Agent 是否应作为 Activity executor 的一种，还是继续作为 Agent Activity 内的 dispatch channel？
