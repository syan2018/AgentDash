# Lifecycle 概念问题清单

## Purpose

本文汇总本轮调研中仍需要产品/架构收束的问题。它不是实现 TODO，而是把相似概念、微妙路径和命名分歧集中列出来，方便后续会话逐项决策。

每个问题记录四件事：

- 当前症状：代码或文档中已经出现的混合表达。
- 为什么重要：继续模糊会造成哪类模型分叉。
- 候选方向：可讨论的收束路线。
- 当前倾向：本轮调研后的推荐判断。

## Issue 1: Workflow 是图，还是单 Agent Activity 契约？

当前症状：

- 当前 `ActivityLifecycleDefinition` 承载 executable graph。
- 当前 `WorkflowDefinition` 承载单 Agent Activity contract / capability / hook / prompt procedure。
- 前端 workflow editor 同时编辑二者，`workflow_key` 实际指向单 Agent contract。

为什么重要：

- 用户语义中 `Workflow` 更符合“可执行图配置实例”。
- graph-level orchestration 与 single-agent behavior contract 如果同名，后续 ActorFrame、Activity executor、ProjectAgent 默认策略都会再次混线。

候选方向：

- `Workflow` = graph config。
- `ActivityProcedure` = single Activity 内 Agent 的行为/能力/上下文契约。
- `ActorProcedure` = 偏 Actor 运行身份的契约。

当前倾向：

- `Workflow` 留给当前 `ActivityLifecycleDefinition` 的目标语义。
- 当前 `WorkflowDefinition` 改为 `ActivityProcedure` 优先；如果 procedure 更依赖 Actor role/profile，再考虑 `ActorProcedure`。

## Issue 2: LifecycleRun 是否需要改名为 LifecycleTrack？

当前症状：

- `LifecycleRun` 当前是具体执行记录，但 run 这个词容易被理解成 workflow graph run。
- 用户强调 Lifecycle 的核心定义是“生命周期追踪”。

为什么重要：

- 如果保留 `LifecycleRun`，需要清楚说明它追踪 Actor/Activity/Attempt/Event/Artifact 的生命过程，而不是 session 容器。
- 如果改成 `LifecycleTrack`，可以更贴合 tracking plane，但会带来大规模命名修改。

候选方向：

- 保留 `LifecycleRun`，文档和 DTO 强调 tracked life process。
- 改为 `LifecycleTrack`，在代码名上消除 workflow run / session run 混淆。

当前倾向：

- 实现阶段可先保留 `LifecycleRun`，优先解决更核心的 Actor/Frame/SubjectAssociation。若后续重命名 Workflow/Procedure 时有统一破坏性窗口，再评估 `LifecycleTrack`。

## Issue 3: Subject association 的 anchor 是否只需要 run / Actor？

当前症状：

- 早期文档曾把 Activity / Attempt 也纳入 association anchor。
- 用户明确指出这更像历史文档过度发挥；Task 更适合绑定 Actor，Activity/Attempt 是执行证据。

为什么重要：

- 如果 subject 可直接挂 Activity/Attempt，会出现 SubjectAssociation、Assignment、Artifact provenance 三套并行追溯路径。
- Task view 需要知道“哪个 Agent 处理了它”，而不是只知道“哪个图节点碰过它”。

候选方向：

- `anchor = run | actor`
- Activity/Attempt 通过 `ActorAssignment`、`ActivityAttemptState`、artifact/event log 提供证据。

当前倾向：

- 只支持 run / Actor anchor。
- `LifecycleRunLink` 演化为 `LifecycleSubjectAssociation(anchor_run_id, anchor_actor_id?)`。

## Issue 4: Task 的 agent_binding 是否仍属于 Task？

当前症状：

- Domain 注释已经说 Task 不持有 runtime truth。
- Task entity 仍有 `agent_binding`，Task service 仍能 start/continue session。
- 前端 Task drawer 把 Task 表现为可执行主体。

为什么重要：

- `agent_binding` 如果留在 Task spec 中，会继续让 Task 看起来拥有执行规则。
- 但 Task 作为用户工作项，有时确实需要表达“希望什么 agent/procedure 处理这个 subject”。

候选方向：

- 保留为 Task payload 的 authoring preference，不直接参与 runtime。
- 移到 Subject execution request / ActorProcedure override。
- 移到 Story/Project dispatch policy：按 subject kind / task type 匹配 ActorProcedure。

当前倾向：

- 执行相关配置迁到 Subject execution request 或 dispatch policy。
- Task 数据可保存用户意图，但运行时只读取 `SubjectRef(kind=Task)` + explicit dispatch policy，避免 Task 成为 owner。

## Issue 5: Task status / artifacts 是否可以继续存在于 Task？

当前症状：

- `Task.status` 和 `Task.artifacts` 存在于 Story aggregate。
- Task view projector 已把 ActivityAttemptState 作为 truth source 反投影到 Task。

为什么重要：

- UI 需要快照字段；领域模型需要避免把 view cache 误认为 runtime truth。

候选方向：

- 完全派生，不在 Task JSON 中存储。
- 保留 projection cache，但带 source revision / source association。
- 拆 `TaskSpec` 与 `TaskProjection`。

当前倾向：

- 拆 `TaskSpec` 与 `TaskProjection` 更清楚。
- 若为了 UI 性能保留缓存，缓存应带 `source_run_id` / `actor_id` / `activity_key` / `attempt` / revision。

## Issue 6: child LifecycleRun 还有哪些必要场景？

当前症状：

- Companion workflow overlay 当前创建 child session/run，但没有完整 links/gates/subjects。
- 用户倾向：多 Activity graph 可以处在同一个大的 Lifecycle 下；Task 独立执行更多是独立 Lifecycle，不共享上下文信道。

为什么重要：

- 若随意创建 child run，会绕开 lifecycle-level 信息交换。
- 若所有都放同一 run，又无法表达真正独立的控制边界、权限边界和导航边界。

候选方向：

- Same-run Actor：默认路径，支持并发 subagent 共享 lifecycle-level artifacts/context/gates。
- Independent LifecycleRun：仅用于独立上下文信道、独立权限/控制边界、独立导航生命周期、跨父生命周期继续运行。
- Lineage association：仅记录 independent run 来源，不提供默认共享上下文。

当前倾向：

- Task/Companion 的默认派发使用 same-run Actor。
- Independent run 是显式控制边界，不是 Task 或 subagent 的默认形态。

## Issue 7: Companion 是 Activity executor、Actor role，还是交互通道？

当前症状：

- Companion request 同时像 subagent dispatch、parent-child session relation、hook interaction、wait/adoption gate。
- `SessionMeta.companion_context` 混合 parent session、slice policy、request type、agent name。

为什么重要：

- Companion 若停留在 session context，业务约束 companion agent 无法自然享有 lifecycle activity graph / frame policy / permission scope。

候选方向：

- `CompanionChannel`：交互协议和 result/adoption 通道。
- `CompanionAgent`：可派发 Actor role。
- `Companion Gate`：等待/审查/恢复的 durable gate。
- `Lifecycle-backed companion execution`：Actor + ActorFrame + Assignment。

当前倾向：

- 拆分这四层。
- companion workflow overlay 通过 `LifecycleDispatchService` 创建 Actor/Frame/Gate，而不是单独创建 child session/run。

## Issue 8: ActorFrame 与 HookSessionRuntime 的关系是什么？

当前症状：

- `HookSessionRuntime` 已有 snapshot、trace、pending actions、capabilities、revision。
- 它以 `session_id` 为 key，并驱动 workflow advance / execution log flush。

为什么重要：

- Hook runtime 已经在扮演 ActorFrame，但事实源仍被 session 拿走。
- 继续扩展 HookSessionRuntime 会让 ActorFrame 难以落地。

候选方向：

- `HookSessionRuntime` 变成 ActorFrame runtime facet。
- RuntimeSession callback 持有 frame ref，hook execution 写 ActorRevision/Gate/ActivityEvent。
- 保留 session-indexed adapter 只为 connector callback。

当前倾向：

- ActorFrame 是 durable/control-plane owner；Hook runtime 是 frame 的 live/runtime adapter。

## Issue 9: PermissionGrant 应该挂到 ActorFrame 还是 run？

当前症状：

- PermissionGrant 同时持有 `run_id` 和 `session_id`。
- GrantScope 有 `Session` / `WorkflowStep` 命名。
- Scope escalation 写 `LifecycleRunLink(ControlScope)`。

为什么重要：

- 能力最终影响的是某个 ActorFrame 的工具面，而不是整个 session 或整个 run。
- 某些 control scope 是 run-level，某些 capability grant 是 actor/frame-level。

候选方向：

- Grant source：run / subject / runtime turn / tool call。
- Grant effect：ActorFrame revision。
- ControlScope association：run or actor anchor。

当前倾向：

- Grant 持 run provenance + actor/frame effect anchor。
- `session_id` 保留为 source runtime trace id，而不是查询主键。

## Issue 10: RuntimeSessionTraceView 是否仍应作为路由？

当前症状：

- 前端主路由 `/session/:sessionId` 是所有运行体验入口。
- SessionPage 组装 lifecycle view、workspace runtime、hook runtime、chat stream。

为什么重要：

- 用户/产品层更关心 Actor、Story、Task、ProjectAgent，而不是底层 session id。
- 但 session trace 对调试和事件流仍然重要。

候选方向：

- 保留 `/session/:id` 作为 trace/detail route。
- 新增 Actor/Subject runtime route 作为主入口。
- 项目导航由 ProjectActiveActorsView 驱动。

当前倾向：

- SessionPage 降级为 RuntimeSessionTraceView。
- ActorFrameRuntimeView 和 SubjectExecutionView 成为业务入口。

## Issue 11: Session lineage 与 Actor lineage 如何共存？

当前症状：

- `session_lineage` 表达 fork/companion/spawned session relation。
- Companion/Task/Routine 的真正运行关系不总是 session relation。

为什么重要：

- Fork/rollback 是 runtime trace 功能；spawn/subagent 是 lifecycle actor relation。
- 把二者都写进 session lineage 会让 lifecycle 控制面无法追溯 Actor 级因果。

候选方向：

- RuntimeSession lineage: trace/debug/fork/rollback。
- Actor lineage: spawn/delegate/companion/routine source。
- LifecycleSubjectAssociation: independent run lineage / source / projection relation。

当前倾向：

- 三者共存但职责分明。
- UI 主树用 Actor lineage；session lineage 在 trace detail 展示。

## Issue 12: RoutineExecution 的 Completed 是什么？

当前症状：

- RoutineExecutor 在 prompt 成功派发后 `mark_completed`。
- Agent 是否完成由 session turn/lifecycle terminal 决定。

为什么重要：

- RoutineExecution 是触发记录，也是 lifecycle source subject。
- 若 Completed 表达 prompt dispatched，用户会误解 Agent 工作已完成。

候选方向：

- `dispatch_status`: pending/running/dispatched/failed/skipped。
- `execution_projection`: linked run/actor terminal status。
- RoutineExecution view 合并二者。

当前倾向：

- RoutineExecution 保存触发/dispatch truth。
- Lifecycle/Actor terminal 投影为执行结果，不覆盖触发记录含义。

## Issue 13: ProjectAgent 是配置、Actor profile，还是 Subject？

当前症状：

- ProjectAgent 是配置实体，但 opening ProjectAgent 直接返回 session。
- `is_default_for_task` 给 Task runtime default 的感觉。

为什么重要：

- ProjectAgent 是最自然的 Actor profile，但不是某次执行的 Actor。
- 若 ProjectAgent 也作为 Subject，需要区分“被执行的业务对象”和“执行者 profile”。

候选方向：

- ProjectAgent = ActorProfile / launch profile。
- Actor references ProjectAgent.
- ProjectAgent 可以作为 Source only when user opens agent hub, but not generic Subject.

当前倾向：

- ProjectAgent 保持 profile/config/source，不作为 Task/Story runtime subject。
- `is_default_for_task` 迁移为 subject dispatch policy。

## Issue 14: ActorRevision 是否必须是一等表？

当前症状：

- ActorFrame 有 revision 需求。
- Hook runtime、capability transitions、permission grants、gates 都会改变 Actor state。

为什么重要：

- 若只存 ActorFrame 当前值，无法解释“为什么这个 Agent 现在有这些能力/上下文”。
- 若每次变化都新建完整 frame，可能存储大但最清楚。

候选方向：

- ActorFrame is revision row：每次变化新建 frame。
- ActorRevision event log + materialized current frame。
- Hybrid：关键变化新建 frame，runtime delivery 只写 event。

当前倾向：

- 先以 ActorFrame revision row 为主，易于调试和前端投影。
- 后续可加 materialized current frame/head。

## Issue 15: 前端的 SubjectExecutionView 粒度如何定义？

当前症状：

- Story 有 runs + sessions 两套入口。
- Task 有 TaskSessionPayload。
- ProjectAgent 有 ProjectAgentSession。

为什么重要：

- 如果每个业务对象继续定义自己的 session response，前端会保留三套 runtime 投影。

候选方向：

- 通用 `SubjectExecutionView(kind,id)`。
- Story/Task/ProjectAgent 定制 wrapper 只补业务标题/展示字段。
- Runtime refs 都来自同一 Actor/Assignment shape。

当前倾向：

- 通用 SubjectExecutionView 优先。
- Story/Task 页面消费该 view；ProjectAgent 使用 ActorLaunchView，因为它更像执行者 profile。

## Decision Backlog

| Priority | Question | Current leaning |
| --- | --- | --- |
| P0 | `WorkflowDefinition` 目标名选 `ActivityProcedure` 还是 `ActorProcedure`？ | `ActivityProcedure` |
| P0 | `LifecycleRunLink` 是否直接更名为 `LifecycleSubjectAssociation`？ | 是，或先扩表后更名 |
| P0 | ActorFrame 是否以 revision row 建模？ | 是 |
| P0 | Task direct execution 是否全部迁到 SubjectRef dispatch？ | 是 |
| P0 | Companion wait 是否落为 durable Gate？ | 是 |
| P1 | `LifecycleRun` 是否改名为 `LifecycleTrack`？ | 可延后 |
| P1 | Task projection cache 是否拆表/拆 DTO？ | 倾向拆 `TaskSpec` / `TaskProjection` |
| P1 | RoutineExecution 是否拆 dispatch status 与 execution projection？ | 是 |
| P1 | SessionPage 是否保留 `/session/:id` trace route？ | 是 |
