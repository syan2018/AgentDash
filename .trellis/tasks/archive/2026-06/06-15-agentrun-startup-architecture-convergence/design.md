# AgentRun 启动主线架构收束设计

## Architecture Direction

本任务把 AgentRun 启动主线重新定义为一条 durable control-plane 状态机，而不是多个 use case 互相同步等待。

目标主线：

```text
AgentRun command
  -> command receipt
  -> mailbox envelope
  -> scheduler delivery decision
  -> LaunchCommand
  -> FrameLaunchEnvelope
  -> LaunchPlan
  -> PreparedTurn
  -> ConnectorAcceptedTurn
  -> CommittedTurn
  -> AttachedTurn
```

ProjectAgent start、AgentRun composer submit、hook/system follow-up 都只负责创建或推进 mailbox envelope。Session launch 只消费 scheduler 产出的 delivery intent，不再反向拥有 AgentRun 控制面。

## Old Model vs New Model

| Area | Old model | New model |
| --- | --- | --- |
| ProjectAgent start | ProjectAgent start 是一个同步 launch 包装器：创建控制面后，把首条消息投给 mailbox，再等待内层 SessionLaunch 完成后才 accepted。 | ProjectAgent start 是 AgentRunThread 创建入口：创建 thread/anchor/initial mailbox envelope，并返回 durable scheduler projection。 |
| First user message | 首条消息是 ProjectAgent start 的内嵌步骤，和后续 composer submit 共享得不彻底。 | 首条消息和后续 composer submit 都是 mailbox envelope，由同一 scheduler 决定 launch/steer/queue/block。 |
| Receipt accepted | 外层 receipt、内层 mailbox receipt、session turn accepted 混在一个成功路径里。 | command receipt accepted、mailbox delivery accepted、session turn accepted、frame/bootstrap accepted 分层表达。 |
| Frame surface | 先创建空 AgentFrame，再在后续 launch/preparation 阶段补 capability/context/vfs 等事实。 | frame construction 在 launch 前产出 launch-ready closure；空 frame 只能是明确的 transient/failed/rebuildable 状态。 |
| TurnPreparer | 除了准备 turn，还可能补事实、组装核心 surface、触发复杂 tool/runtime 装配。 | 只消费 launch-ready facts，派生 runtime tools/context frames/connector projection。 |
| Runtime tools | tool declaration 与 invocation 混杂，schema 构建阶段可以触达 gateway/repo/session services。 | declaration 构建 provider-visible schema/description/capability gate；invocation adapter 才触发 gateway/repo/local backend。 |
| Runtime delegate | 一个 delegate 组合 transform、hook、mailbox boundary、before-stop、audit 等职责，inner 缺失时可能改变输入语义。 | 按 ContextTransformer、StopPolicy、MailboxBoundaryScheduler、HookInjectionSink、RuntimeAuditSink 等阶段组织。 |
| Recovery | 依赖启动路径跑完；崩溃留下 pending/consuming/empty frame 时语义不确定。 | consuming lease、accepted refs、empty bootstrap frame 都有确定恢复投影。 |
| Frontend projection | start response 容易被理解为首轮 connector 已 accepted。 | UI 以 workspace shell、command receipt、mailbox projection、stream/projection update 共同表达状态。 |

旧模型清理不是删除几个旧函数名，而是确认这些旧语义无法再从任何入口被执行或被前端推断。

## Boundary Model

### ProjectAgent Start

ProjectAgent start 是 AgentRunThread 创建入口，职责是：

- 校验 ProjectAgent、subject、executor config 和用户权限。
- 创建或复用 LifecycleRun / LifecycleAgent / RuntimeSession / initial AgentFrame anchor。
- 创建首条 user-origin mailbox envelope。
- 调用 scheduler 尝试 launch 或 queue。
- 返回 AgentRun-facing accepted refs、mailbox/message projection 和 workspace shell。

ProjectAgent start 不再把“首条消息已经进入 connector prompt accepted”作为自身成功的必要条件。

### AgentRun Mailbox

Mailbox 是唯一 message intake 和 scheduler fact source。它拥有：

- command receipt idempotency / conflict。
- mailbox envelope lifecycle。
- delivery policy：launch、steer、queue、resume、blocked。
- delivery accepted refs。
- consuming lease recovery。

Scheduler 可以调用 SessionLaunch，但不能把 SessionLaunch 的内部状态泄漏成 route-local 分支。

### Session Launch

Session Launch 保持现有 stage pipeline，但收窄职责：

```text
LaunchCommand -> FrameLaunchEnvelope -> LaunchPlan -> PreparedTurn -> ConnectorAcceptedTurn -> CommittedTurn -> AttachedTurn
```

LaunchCommand 表达来源意图；FrameLaunchEnvelope 是 launch-ready fact closure；PreparedTurn 只做 turn runtime preparation 和 connector projection。

### Frame Construction

Frame construction 是 VFS、MCP、capability、context bundle、executor profile、identity、working directory 的唯一汇聚点。进入 LaunchPlanner 前必须校验：

- working directory 存在。
- executor config 存在。
- launch surface VFS 存在。
- capability state 与 launch VFS/MCP surface 一致。
- context bundle / trace / identity 已经完成来源投影。

TurnPreparer 不再补齐这些事实，只能基于 envelope 派生 runtime tools、context frames 和 connector input。

### Runtime Tool Composer

工具体系拆成两个阶段：

```text
tool declaration surface
  -> provider-visible schema / description / capability gate
tool invocation adapter
  -> gateway / repo / local backend / extension host
```

构建 provider-visible tools 时不能调用 runtime action、不能执行 session launch、不能依赖可变 active turn 状态来补核心事实。复杂 runtime 查询只发生在 tool call invocation 阶段。

### Runtime Delegate

现有 `AgentRuntimeDelegate` 承担过多职责。目标拆分方向：

- `ContextTransformer`：只负责模型可见上下文变换。
- `StopPolicy`：只负责 before-stop 是否继续。
- `MailboxBoundaryScheduler`：只负责 AgentLoopTurn / AgentRunTurn boundary 的 durable envelope drain。
- `HookInjectionSink`：只负责 hook 注入与审计。
- `RuntimeAuditSink`：只负责 context/tool/runtime event audit。

短期可先通过组合器内部拆私有组件实现，外部 trait 是否拆分由实现风险决定。

## Data Flow

```text
Frontend create ProjectAgent run
  -> API route validates auth and DTO
  -> ProjectAgentStart use case creates AgentRunThread shell and initial mailbox envelope
  -> Mailbox scheduler claims envelope
  -> Scheduler builds delivery LaunchCommand
  -> SessionLaunchOrchestrator builds FrameLaunchEnvelope through construction provider
  -> TurnPreparer derives runtime tools/context frames without mutating control-plane truth
  -> ConnectorStarter obtains ExecutionStream
  -> TurnCommitter persists accepted facts
  -> StreamIngestionAttacher supervises stream
  -> Mailbox completion records accepted refs/outcome
```

Route response should be valid even if connector setup does not complete synchronously. The browser observes workspace shell + command receipt + mailbox projection, then stream/projection updates refine runtime status.

## Accepted Boundaries

| Boundary | Meaning |
| --- | --- |
| command receipt accepted | user command has durable envelope or durable terminal result |
| mailbox delivery accepted | scheduler has a definitive delivery outcome and accepted refs when available |
| session turn accepted | connector has returned an ExecutionStream |
| frame/bootstrap accepted | AgentFrame revision / bootstrap meta has been persisted |
| attached | stream processor and adapter supervision are registered |

These states must not be collapsed into one boolean.

## Recovery Model

Startup interruption states must be first-class:

- `consuming + accepted refs` restores dispatched/steered。
- `consuming + no accepted refs + expired lease` becomes blocked with delivery-result-unknown unless the scheduler can prove no side effect crossed the accepted boundary。
- initial empty frame without bootstrapped surface is either rebuilt from control-plane facts or marked bootstrap failed with a clear recovery command。
- outer command receipt references mailbox result instead of independently guessing launch status。

## Cleanup Completion Definition

旧模型清理完成必须同时满足代码、行为、测试和文档四类证据。

代码证据：

- ProjectAgent start 不再调用 `launch_initial_user_message` 或等价 port 来同步等待首条消息 launch 完成。
- `ProjectAgentRunInitialMessagePort`、`AgentRunMessageLaunchDeliveryPort`、`accepted_refs_from_initial_launch` 等旧同步桥接若仍存在，必须只服务新模型或被删除；不能保留旧语义包装器。
- API route 不再根据 start response 推断 connector accepted；frontend 不再依赖 route-local start success 代表首轮 turn 已运行。
- `TurnPreparer` 不再补齐 VFS/MCP/capability/executor 等核心事实，也不写入等价的 owner bootstrap surface truth。
- `SessionRuntimeToolComposer::build_tools` 及其 provider-visible declaration path 不触发 `RuntimeGateway::invoke`、session launch、mailbox schedule 或控制面 mutation。
- no-inner runtime delegate 不会返回空 provider-visible message list 来覆盖输入；empty continue 必须有显式 guard，不能形成无限空 loop。

行为证据：

- ProjectAgent 首条消息和 composer submit 走同一 mailbox scheduler outcome。
- 启动失败或进程中断后，receipt/mailbox/frame/session 状态能被恢复或明确 blocked，不出现无法解释的组合。
- 新模型下 provider-visible tool schema 构建不会因为 runtime action / extension / tool declaration 递归导致栈溢出。

测试证据：

- 有 regression 覆盖 ProjectAgent 首轮启动、mailbox consuming lease recovery、FrameLaunchEnvelope surface mismatch、runtime delegate no-inner transform、empty continue guard。
- contract/frontend check 覆盖 UI 消费 mailbox/workspace projection，而不是旧 start response 推断。

文档证据：

- `.trellis/spec` 记录新 accepted 边界、launch-ready frame closure、runtime tool declaration/invocation 分层和 mailbox 作为唯一 intake 的原因。

## Migration Notes

项目仍处于预研期，不保留长期兼容路径。允许调整 API/DTO/数据库字段，但需要同步 migration、contract generation 和 frontend consumption。

Existing pending/half-created local dev rows may be cleaned by migration or explicit repair command if they cannot be represented in the corrected model. Do not preserve current malformed state as a supported runtime case.

## Trade-offs

- 本任务采用单个大任务内分 phase 和阶段性提交，不拆 child task。这样保留一个连续的架构上下文，同时通过 phase gate 控制 review 风险。
- 先修 ProjectAgent/Mailbox 状态机能最快消除事故现场的半成品状态；但如果不随后收束 FrameLaunchEnvelope 和 Runtime Tool Composer，启动链路仍会保留递归/side-effect 风险。
- RuntimeDelegate 完整 trait 拆分风险较高，可以先内部组件化，再决定是否改公共 trait。
