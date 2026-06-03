# Agent 参考实现抽象与命名对照

## Purpose

本文结合 `references/codex` 与 `references/claude-code` 的 Agent / 多 Agent 实现，校准本次 Lifecycle 控制面重构中的命名选择。

核心判断：当前 `Lifecycle -> Actor -> ActorFrame -> RuntimeSession` 的职责方向是对的，但 `Actor` / `ActorFrame` 这组词在 Agent 产品和代码生态里不够常见。为了避免后续实现脱离共识，目标命名应优先靠近 `AgentDefinition`、`AgentRun`、`AgentSession`、`AgentGraph`、`AgentStatus`、`AgentContext` 这类参考项目已经使用或隐含的语言。

本文不要求一次性重命名所有既有规划文档；它先给出后续设计和实现应采用的推荐词汇。

## Reference Signals

### Codex

Codex 的多 Agent 模型偏轻量控制面：

- `AgentControl` 负责 spawn、message、wait、close、resume、list。
- `AgentRegistry` 只维护 live agent metadata、nickname、path、spawn limit。
- `AgentGraphStore` 只持久化 thread-spawn parent/child topology。
- `SessionSource::SubAgent(ThreadSpawn)` 保存 parent thread、depth、agent path、nickname、role。
- `AgentPath` 是通信寻址空间，不是业务任务树。
- `AgentStatus` 从 thread event 推导。

它的实际模型接近：

```text
Agent = Thread + SessionSource + AgentMetadata + spawn graph edge
```

对本项目的启发是：Agent 层级关系应落为 runtime graph / lineage fact，而不是 Story / Task 的业务树；live registry 与 durable control-plane facts 必须分层。

但 Codex 的 `Agent = Thread` 不适合直接照搬。AgentDashboard 需要同时表达 Story、Task、Workflow、Activity、Permission、Companion、Routine 与 RuntimeSession。如果继续把 RuntimeSession 当 Agent anchor，会重新回到 session-first。

### Claude Code

Claude Code 的 Agent 模型更强调职责拆分：

- `AgentDefinition` 表示角色/能力声明。
- `AgentTool` 是一次 Agent 调用入口。
- `runAgent()` 创建运行上下文、工具池、权限模式、MCP 与 hook，然后进入 query loop。
- `LocalAgentTaskState` / teammate task 表达异步进度、完成、失败、取消、通知。
- `createSubagentContext()` 默认隔离上下文，只显式共享 AppState、response length、abort controller 等能力。

它的实际模型接近：

```text
AgentDefinition -> AgentTool invocation -> Agent runtime context -> query loop / async task state
```

对本项目的启发是：定义、调用、运行上下文、生命周期状态要拆开。Agent profile、dispatch intent、runtime frame、runtime session、task/progress projection 不应合成一个大对象。

但 Claude Code 的 teammate / swarm 命名与 tmux/iTerm2/team file/mailbox 强绑定，不适合作为本项目领域模型基础；可借鉴 channel、message、permission bridge，不应照搬 team/pane 结构。

## Naming Principles

后续命名应遵守以下原则：

- 优先使用 Agent 生态常见词，而不是过度发明新词。
- `Agent` 表达执行主体，`Session` 表达运行轨迹，`Workflow` 表达图配置，`Activity` 表达图节点。
- profile / definition、invocation / run、runtime frame、runtime session、status projection 分开命名。
- `Task`、`Story`、`Project` 作为 subject / business object，不拥有 Agent runtime。
- parent / child Agent 关系是 runtime lineage，不是业务任务树。
- 权限、工具、MCP、VFS、上下文投影应收敛到 effective runtime frame，不塞进 `agent_role`。
- `run` 是业界常见执行实例词；除非非常必要，不引入 `Track` 这类更孤立的词。

## Recommended Vocabulary

| 推荐名称 | 取代 / 调整对象 | 语义 |
| --- | --- | --- |
| `AgentProfile` | `ProjectAgent` 的领域解释 | 可启动 Agent 的配置/profile；不是某次运行身份 |
| `LifecycleAgent` | `LifecycleActor` / `AgentStateAnchor` | 某个 `LifecycleRun` 内的一等 Agent 运行身份 |
| `AgentFrame` | `ActorFrame` | `LifecycleAgent` 某个 revision 的 effective runtime surface |
| `AgentFrameRevision` | `ActorRevision` | 解释 AgentFrame 为什么变化、由哪些事件导致 |
| `AgentAssignment` | `ActorAssignment` | `LifecycleAgent` 被分配到 Activity / ActivityAttemptState 的事实 |
| `AgentLineage` | agent spawn relation / parent-child relation | Agent 与 Agent 或 run 与 run 的 runtime lineage |
| `AgentProcedure` | 当前 `WorkflowDefinition` 的目标语义候选 | 单个 Agent Activity 的 prompt、tools、hooks、context contract |
| `Workflow` | 当前 `ActivityLifecycleDefinition` 的目标语义 | Lifecycle 下生效的可执行图配置 |
| `LifecycleRun` | 保留并强化解释 | 一次被追踪的执行生命过程；不再改为 `LifecycleTrack` |
| `RuntimeSession` | 当前 `Session` 的目标语义 | turn、tool call、event log、resume/debug substrate |
| `RuntimeTraceView` | `/session/:id` 产品视图 | 面向调试/回放的 RuntimeSession 轨迹视图 |
| `LifecycleSubjectAssociation` | `LifecycleRunLink` | SubjectRef 到 run / LifecycleAgent anchor 的关联 |
| `LifecycleGate` | `CompanionWaitRegistry` / wait registry | human/platform/companion 等等待与恢复点 |
| `EffectiveCapability` | scattered tool/permission/capability state | AgentFrame 内的有效工具、MCP、VFS、权限能力快照 |
| `ContextProjectionPolicy` | context inherit/slice/isolated | AgentFrame 构建上下文的策略 |

### Why Prefer LifecycleAgent Over Actor

`Actor` 在 actor model、workflow engine、UI state machine 中都常见，但在 Agent 产品和模型调用生态里不如 `Agent` 直观。Codex 与 Claude Code 都围绕 `AgentControl`、`AgentRegistry`、`AgentDefinition`、`AgentStatus`、`AgentTool`、`runAgent` 展开；继续使用裸 `Actor` 会让本项目看起来像在引入另一套并发 actor model。

`LifecycleAgent` 的好处是：

- 保留 “这是 LifecycleRun 内的运行身份” 这个约束。
- 对齐参考项目中 Agent 作为执行主体的共识。
- 与 `AgentProfile` 区分：profile 是可启动配置，LifecycleAgent 是一次 run 内的运行身份。
- 与 `RuntimeSession` 区分：LifecycleAgent 拥有/引用 RuntimeSession，RuntimeSession 不拥有 Agent。

### Why Prefer AgentFrame Over ActorFrame

`Frame` 仍然有价值：它表达 effective capability、context、VFS、MCP、procedure、runtime refs 的某个 revision。参考项目虽然没有同名对象，但 Claude Code 的 `createSubagentContext()` 和 Codex 的 `build_agent_spawn_config()` 都在做同一类事情：把当前有效配置投影成一次启动上下文。

因此推荐保留 `Frame`，但改为 `AgentFrame`，让它自然归属于 Agent runtime。

### Why Keep LifecycleRun

`LifecycleTrack` 更贴合“追踪平面”，但 `Run` 是 workflow / agent / job 系统里更常见的执行实例词。当前项目已有 `LifecycleRun`，并且参考项目也大量使用 thread/run/session/status 语言。与其改成更独特的 `Track`，不如保留 `LifecycleRun`，在文档和 DTO 中明确：

```text
LifecycleRun = tracked life process, not a RuntimeSession container.
```

这能减少命名迁移成本，也避免让模型显得过度自造。

### Why Use AgentProcedure

当前 `WorkflowDefinition` 实际表达单个 Agent Activity 的局部行为契约，而目标上 `Workflow` 应留给 graph-level executable config。

命名边界上：

- `AgentDefinition` 应保留给可复用 Agent 类型或模板，也就是静态配置面。
- `ActivityProcedure` 强调它挂在 Activity 上，但弱化了它约束 Agent 行为。
- `AgentProcedure` 更贴近 “Agent 如何执行这一段工作”。

推荐先使用 `AgentProcedure`，并在 Activity executor 中通过引用关系表达：

```text
AgentDefinition? -> Workflow Activity -> AgentProcedure -> AgentFrame
```

## Target Model With Revised Names

```text
LifecycleRun
  tracks Workflow activities, lifecycle events, artifacts, gates

Workflow
  defines executable graph: Activities, transitions, ports, artifacts

Activity
  graph node / execution slot

AgentProcedure
  local contract for an Agent-driven Activity

LifecycleAgent
  runtime Agent identity inside one LifecycleRun

AgentFrame
  effective runtime surface revision:
  procedure, context, capabilities, VFS, MCP, runtime refs

RuntimeSession
  turn/tool/event/resume/debug substrate

AgentAssignment
  LifecycleAgent -> ActivityAttemptState execution evidence bridge

LifecycleSubjectAssociation
  SubjectRef -> run or LifecycleAgent anchor
```

一句话描述目标运行状态：

```text
LifecycleAgent A in LifecycleRun R
uses AgentFrame F,
wraps RuntimeSession S,
acts on SubjectRef T,
is assigned by AgentAssignment to Activity X / ActivityAttemptState #n,
and sees EffectiveCapability C from AgentFrame revision E.
```

这句话比旧版 `Actor / ActorFrame` 更贴近参考项目中的 Agent 语言，同时保留了本项目对 Lifecycle tracking、Activity evidence 和 RuntimeSession substrate 的分层。

## Multi-Agent Naming

参考项目里有两种重要区别：

- Codex 用 `AgentGraphStore` / thread-spawn edge 表达 parent-child。
- Claude Code 用 `AgentTool`、`SendMessageTool`、teammate mailbox、async task state 表达协作。

本项目建议采用：

| 场景 | 推荐名称 | 说明 |
| --- | --- | --- |
| 同一 run 内并发 Agent | `LifecycleAgent` + `AgentAssignment` | 默认多 Agent 执行形态 |
| Agent 派生 Agent | `AgentLineage` | runtime lineage，不是业务父子任务 |
| Agent 间消息 | `AgentMessage` / `CompanionChannel` | channel 与运行身份分开 |
| 等待 parent / human / platform | `LifecycleGate` | durable wait/resume fact |
| 独立控制边界 | `LinkedLifecycleRun` / `SpawnedLifecycleRun` | 只在独立 lifecycle 成立时使用 |
| live 列表/状态 | `AgentRegistry` / `AgentStatusProjection` | live/projection，不是 durable truth |

## Updated Review Questions

后续设计和实现可以用这些问题防止命名重新漂移：

1. 这个对象是在描述 `AgentProfile`，还是某个 `LifecycleAgent` 运行身份？
2. 这个字段是在描述 `AgentFrame` 的 effective runtime surface，还是底层 `RuntimeSession` trace？
3. 当前入口是否统一返回并持久化 `run_id + lifecycle_agent_id + agent_frame_id + runtime_session_id`？
4. Task / Story / Project 是否只通过 `SubjectRef` 与 `LifecycleSubjectAssociation` 进入 runtime？
5. `agent_role` 是否只表达角色，而不是直接决定全部 tools、MCP、permission、context？
6. parent-child 关系是否拆成 `AgentLineage`、`CompanionChannel`、`LifecycleGate`，而不是只靠 session parent id？
7. Agent 完成是否通过 `RuntimeSession -> AgentFrame -> LifecycleAgent -> AgentAssignment -> ActivityAttemptState` 回到 Lifecycle，而不是注入一条 parent message 就结束？

## Migration Notes

当前规划文档中的旧词可以按以下方式逐步迁移：

| 旧词 | 新推荐词 | 备注 |
| --- | --- | --- |
| `LifecycleActor` | `LifecycleAgent` | 优先迁移 |
| `Actor` | `LifecycleAgent` | 需要避免和普通 actor model 混淆 |
| `ActorFrame` | `AgentFrame` | 保留 Frame 概念 |
| `ActorRevision` | `AgentFrameRevision` | revision 归属 frame 更清楚 |
| `ActorAssignment` | `AgentAssignment` | execution evidence bridge |
| `ActorProcedure` | `AgentProcedure` | 若强调 Activity 归属，可在字段名中使用 activity |
| `LifecycleTrack` | `LifecycleRun` | 不再推荐 Track 作为目标名 |
| `Trace` | `RuntimeSession` / `RuntimeTraceView` | code fact 用 RuntimeSession，UI/debug view 可用 RuntimeTraceView |

这组命名让本项目既不退回 session-first，也不会显得脱离 Agent 工具生态的通用语言。
