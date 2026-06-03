# Lifecycle 控制面术语整理

## Purpose

记录 Lifecycle 相关概念的命名债务、混淆来源与候选命名方向。本文只讨论命名，不重复展开完整语义模型；完整语义见 `semantic-inventory.md`，实体关系图见 `lifecycle-entity-association-map.md`。

参考 `references/codex` 与 `references/claude-code` 后，命名目标从“内部自洽”进一步收敛为“尽量靠近 Agent 工具生态共识”。详细对照见 `reference-agent-abstraction-and-naming-review.md`。

命名整理的目标不是追求新词，而是让每个名字准确表达事实源和职责边界，降低后续实现时的误接线风险。

## Naming Principles

- 名称应优先表达事实类型：business fact、runtime fact、permission fact、projection、association。
- 名称应避免让 runtime container 看起来像 business owner。
- 名称应区分 definition 与 run：模板/协议不是执行实例。
- 名称应区分 graph-level orchestration 与 single-agent behavior contract。
- 名称应允许 same-run Agent assignment 与 independent run dispatch 同时存在。
- 名称应优先复用 `AgentDefinition`、`AgentRun`、`AgentStatus`、`AgentGraph`、`AgentContext` 这类参考项目中常见的 Agent 语言。
- `Actor` 只作为历史讨论词保留；目标命名优先使用带作用域的 `LifecycleAgent`，避免看起来像引入一套独立 actor model。
- `AgentDefinition`、`AgentProcedure`、`AgentFrame` 分别表达静态模板、Activity 绑定契约、运行前解析完成的有效表面，不把三者压进一个模型。

## Current Terms Under Review

| 当前名称 | 当前含义 | 混淆来源 | 候选方向 |
| --- | --- | --- | --- |
| Lifecycle | 执行生命过程的追踪平面 / control ledger | 容易被误当成可执行图配置本身 | 保留，强调 tracking：追踪 LifecycleAgent、Activity、ActivityAttemptState、能力/上下文变化与因果 |
| ActivityLifecycleDefinition | 当前代码里的 activity graph definition | 名称把 Lifecycle 与 graph config 混在一起 | 目标侧更适合称 `Workflow` |
| LifecycleRun | 一次被追踪的执行生命过程 | 容易被误解成 workflow graph 的 run 或上下文聚合根 | 保留 `LifecycleRun`，在文档和 DTO 中强调 tracked life process |
| Activity | LifecycleRun 内的执行节点 | 与 agent 的一次 action / tool activity 可能混淆 | 保留时配套称 "Lifecycle Activity" |
| AgentDefinition | 可复用 Agent 类型、模板、默认工具/模型/权限配置 | 容易与单次 Activity 内的行为契约混用 | 保留给静态配置面；没有复用模板时不强行建模 |
| WorkflowDefinition | 当前代码中单个 Lifecycle Activity 引用的 Agent 行为、注入、能力与局部演化契约 | 与目标语义中 `Workflow = 可执行图配置` 冲突 | 候选改为 `AgentProcedure` |
| Session | runtime event log / turn / tool call / resume substrate | 历史上承载 owner/binding，容易被误读为业务会话 | 文档和目标模型中优先称 `RuntimeSession`；调试视图可称 `RuntimeTraceView` |
| LifecycleRunLink | 当前是 run 与 Subject 的显式关系 | 名字暗示只能指向 run；SubjectRef traceability 需要指向 LifecycleAgent | 候选演化为 `LifecycleSubjectAssociation`、`LifecycleSubjectLink` |
| LifecycleActor / AgentStateAnchor | RuntimeSession 之上的高层运行封装 | 当前 Agent 状态被 Session、ActivityAttemptState、CapabilityState 分散承载 | 目标名优先使用 `LifecycleAgent` |
| Companion | cross-subject interaction bus / parent-sub-human-platform 通道 | 有时像 subagent，有时像交互协议，有时像业务 agent | 区分 `CompanionChannel` 与 `CompanionAgent` |
| TaskAgent | 执行 Task 的 agent 角色 | 可能是 same-run Agent assignment，也可能是 independent run | 候选改为 role/policy 名称，如 `TaskExecutorAgent` |
| Dispatch | 派发 Activity 或 independent run 的动作 | 当前 scattered in session launch / companion / task / routine | 候选：`LifecycleDispatch` 或 `ExecutionDispatch` |
| Projection | 从 runtime/business facts 派生 UI 或 tool surface | 容易被误写成事实源 | 保留，强调 projection 不拥有 truth |

## Naming Decisions

- `Workflow` 应留给可执行图配置，也就是当前 `ActivityLifecycleDefinition` 的目标语义。
- `AgentDefinition` 表示可复用 Agent 类型或模板；它可以被 ProjectAgent、subagent 类型或 preset 使用，但不等同于一次 Activity 的执行契约。
- 当前 `WorkflowDefinition` 更像单个 Agent Activity 的局部行为契约，候选名优先考虑 `AgentProcedure`。
- `AgentProcedure` 可引用 `AgentDefinition`，但它拥有 Activity 绑定后的局部行为、注入、能力约束与演化规则。
- `LifecycleAgent` 表达 RuntimeSession 之上的高层 Agent 运行身份；`RuntimeSession` 只表达底层 turn、event log、tool call、resume/debug 轨迹。
- `AgentFrame` 表达 LifecycleAgent 某一 revision 的有效运行表面，是 capability、context、VFS、MCP、procedure、runtime refs 的自上而下事实源。
- `LifecycleSubjectAssociation` 若演化自 `LifecycleRunLink`，只需要覆盖 run / LifecycleAgent anchor；Activity / ActivityAttemptState 通过 Agent assignment 提供执行证据，不作为 subject anchor。
- `TaskAgent` 应是 role / assignment / launch policy，而不是模型层实体。
- independent / linked / spawned run 表达新的执行控制边界或上下文信道边界，不表达 Workflow definition 的结构性嵌套。

## Clean-Slate Vocabulary

| 推荐概念名 | 替代当前概念 | 语义 |
| --- | --- | --- |
| `Lifecycle` | 当前泛化的 lifecycle/control plane | 执行生命过程的追踪语义 |
| `LifecycleRun` | 保留当前名称 | 某个具体执行生命过程的追踪记录 |
| `Workflow` | `ActivityLifecycleDefinition` | Lifecycle 下生效的可执行图配置实例 |
| `Activity` | 当前 Activity | Workflow 图中的可调度节点 |
| `ActivityAttemptState` | 保留当前名称 | Activity 的一次 executor execution record |
| `AgentDefinition` | 可复用 Agent 模板/类型 | 静态 Agent 配置面 |
| `LifecycleAgent` | `LifecycleActor` / `AgentStateAnchor` | RuntimeSession 之上的高层 Agent 运行封装 |
| `AgentFrame` | capability/context/VFS/workflow projection | LifecycleAgent 某一 revision 的有效运行表面 |
| `AgentProcedure` | `WorkflowDefinition` | 单个 Activity 内 Agent 如何工作的局部契约 |
| `RuntimeSession` | 当前 `Session` 的目标语义 | runtime turn、event log、tool call、resume/debug 轨迹 |
| `LifecycleSubjectAssociation` | `LifecycleRunLink` | 把 SubjectRef 关联到 lifecycle / LifecycleAgent anchor |
| `SubjectRef` | Story / Task / Project / External ref | runtime 可携带的业务对象引用 |
| `LifecycleGate` | Human / platform / companion wait | 让 LifecycleAgent 等待和恢复的交互门 |
| `Grant` | PermissionGrant / capability grant | 解释 LifecycleAgent 为什么拥有某个能力 |
| `AgentAssignment` | dispatch 的结果关系 | LifecycleAgent 被分配到 Activity / ActivityAttemptState |

## Open Naming Questions

- `AgentDefinition` 是否已有明确复用场景；若没有，先只保留命名边界，不急于引入表或仓储。
- 单个 Agent Activity 内部契约最终是否采用 `AgentProcedure`，以及是否需要按 Activity 类型派生更具体的 procedure 名称？
- `LifecycleRunLink` 若扩展 LifecycleAgent anchor，是否直接改名为 `LifecycleSubjectAssociation`？
- independent run 的产品和代码命名应使用 `LinkedLifecycleRun`、`SpawnedLifecycleRun` 还是更中性的 `IndependentExecutionRun`？
