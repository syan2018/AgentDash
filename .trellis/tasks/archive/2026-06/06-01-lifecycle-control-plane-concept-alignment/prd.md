# Lifecycle 执行控制面概念厘清

## Goal

厘清 AgentDash 中 Lifecycle 的核心语义：它应作为 Agent 执行的共享控制面，整合 Activity 推进、运行环境投影、上下文关联、权限/工具可见性与 companion/task/routine 派发入口，同时避免把 Story、Task、Session、Permission、Interaction 等事实源混成同一个对象。

本任务的核心收束目标是：把当前散落在 Session、CapabilityState、StepActivation、CompanionContext、Task projection、RuntimeCapabilityTransition 等路径里的 Agent runtime facts，统一收束到 Lifecycle -> Actor -> ActorFrame -> RuntimeSession 这条主线上。

本任务先服务设计讨论，不直接进入实现。

本任务还需要把当前分散入口的收束目标明确列为设计对象：ProjectAgent、Story、Task、Companion、Routine、manual lifecycle run 不应长期各自维护一套启动、上下文推导和工具投影逻辑；需要推导出一套统一但不过度合并的运行建模规则。

## Background

当前项目已经把 Lifecycle 相关运行与 activity graph、data flow、event-driven runtime 绑定较深。本轮需要在此基础上进一步校准：Lifecycle 的核心是生命周期追踪；可执行图配置应单独作为 Workflow 在 Lifecycle 下生效：

- `README.md` 将 Lifecycle 空间描述为把多步骤工作变成端口、产物和事件的运行空间。
- `workflow/activity-lifecycle.md` 规定 Activity lifecycle 是 workflow 运行、编辑和观察的唯一模型，durable advancement 只能通过 `ActivityEvent -> LifecycleEngine`。
- `workflow/lifecycle-run-link.md` 与 `story-task-runtime.md` 已经把 `LifecycleRunLink` 定义为 Run 与 Story / Task / RoutineExecution / Project / parent LifecycleRun 的显式关联层。

但实现入口仍处于迁移态：

- `LifecycleRun` core 仍有 `session_id`，当前大量 runtime/context 查询仍通过 `list_by_session` 进入。
- `ProjectAgent` 打开 session 时自动启动默认 lifecycle/freeform run，但没有显式 ProjectAgent run link。
- `Story` session 路径会在 freeform run 上补 Story subject link，Story runs API 已经是 run-oriented。
- `Companion` 可通过 `workflow_key` 创建 workflow-backed child session/run，但未建立 `SpawnedBy` / `Subject` / `ControlScope` 等 run links。
- `Task` 服务层已经声称 task execution 不再绑定 `lifecycle_activity:*`，但 task session 查询仍依赖 `LifecycleRunLink(Task)`，而启动路径尚未创建对应 run/link。
- `Routine` 仍是 session prompt dispatch，`RoutineExecution` 标记的是 prompt 已派发，而非 Agent 终态。

## Requirements

- 明确 Workflow（当前 `ActivityLifecycleDefinition` 的目标语义）、LifecycleRun、ActivityAttemptState（Activity 执行记录）、RuntimeSession、LifecycleRunLink、PermissionGrant、Companion Interaction、Story、Task 的职责边界。
- 明确 Lifecycle 是否可以嵌套，以及嵌套是 definition-level 结构还是 run-to-run runtime lineage。
- 明确何时应把一个 agent/subagent/task/companion 建模为同一 `LifecycleRun` 下的多个 `Activity`，何时应建模为 child `LifecycleRun`。
- 明确当前被用户编排的 "workflow" / executable graph，在概念上是 Lifecycle 下生效的可执行图配置，以及它与单个 Agent Activity 引用的 `WorkflowDefinition` 的边界。
- 明确目标分层：Lifecycle 负责追踪执行生命过程与信息交换面；Workflow 是在 Lifecycle 下生效的可执行图配置；单个 Agent Activity 内部的行为/能力/上下文契约另称 `ActivityProcedure` / `ActorProcedure`，不再称为 workflow。
- 重新校准 Lifecycle 与 Workflow 的关系：Lifecycle 的核心是生命周期追踪；Workflow 是在 Lifecycle 下生效的可执行图配置实例，不是 Lifecycle 本身。
- 明确多个并发 subagent 作为同一 LifecycleRun 下并发 Activity 时，如何通过 lifecycle-level ports / artifacts / VFS / context projection 实现信息交换。
- 明确多 Activity 子图本身不触发 child run；只有独立 subject、独立上下文信道、独立控制边界、独立生命周期或跨对象投影才可能触发 linked/spawned run。
- 明确 Task 只作为数据载体、用户关联查看对象或 Activity payload，不拥有 runtime 语义；runtime 只携带 `SubjectRef(kind=Task)`。
- 明确 `SubjectRef(kind=Task)` 留在同一 LifecycleRun 内由 companion / TaskExecutorAgent 处理时，如何关联到执行它的 Actor，再通过 Actor 的 Assignment / ActivityAttemptState 追溯具体执行证据；RuntimeSession 由 Actor 作为高层封装管理。
- 评估是否将 `LifecycleRunLink` 从 run-level association 迁移为 lifecycle subject association：同一概念只需要指向 whole run 或 Actor；Activity / ActivityAttemptState 不作为 subject anchor，避免把执行证据建模成业务关联。
- 明确 Story 与 Task 执行时的上下文共享方式，包括共享事实、投影策略、权限边界和 task-specific workflow。
- 明确 companion 与 lifecycle 的关系：普通 companion 交互总线、workflow-backed companion、business-constrained companion agent 是否应落入同一派发模型。
- 明确 TaskAgent 是否应作为 Story 下处理 `SubjectRef(kind=Task)` 的 companion-like Actor，还是作为该 SubjectRef 的 independent LifecycleRun。
- 明确 `LifecycleRun.session_id`、`ExecutorRunRef::AgentSession`、ActivityAttemptState 与 RuntimeSession 的关系，避免单 session 字段承担多 execution records / 多子 session 语义。
- 评估是否需要在 Lifecycle 内新增 Agent 状态锚点（暂名 `LifecycleActor` / `AgentStateAnchor`），作为 RuntimeSession 的高层封装，用于承载 Agent 的运行身份、有效上下文/能力投影、RuntimeSession refs 与可追踪状态变化。
- 明确 Activity 如何改变 Agent 状态，以及这些改变如何通过事件、revision 或 association 被追踪和管理。
- 梳理当前散落在 Lifecycle / Activity activation / Session construction / CapabilityState 中的 Agent 运行信息，判断哪些应下沉到 Agent 状态锚点，哪些仍应作为 projection 或 runtime substrate。
- 明确 `WorkflowBindingKind/binding_kinds` 在目标模型中的位置：catalog filter、launch scope、subject requirements、capability contract 是否需要拆开。
- 明确入口收束的目标形态：ProjectAgent 默认 lifecycle、Story root/freeform、SubjectRef(kind=Task) execution、workflow-backed companion、Routine trigger、manual run 应共享哪些 dispatch / link / projection / capability 机制，哪些差异应保留在 launch policy 或 projection policy 中。
- 从语义上重新梳理 Lifecycle、Activity、Workflow、Run、ActivityAttemptState、Session、Companion、Task、Story、Subject、Projection、Dispatch、Permission 等存量概念，明确哪些应保留、哪些应改名、哪些只是历史迁移痕迹。

## Acceptance Criteria

- [ ] 形成一份 Lifecycle 概念模型草案，说明哪些事实属于 Lifecycle，哪些只由 Lifecycle 引用或投影。
- [ ] 给出 Lifecycle nesting / child run 的最小关系模型，包括 `LifecycleRunLink` role 与 metadata 需求。
- [ ] 给出 "same-run Actor assignment vs child-run dispatch" 的判别规则，并用 TaskAgent、workflow-backed companion、Story root agent、Routine inspection 四个场景验证。
- [ ] 给出 Workflow graph 与 ActivityProcedure / ActorProcedure 的分层命名和职责边界，验证并发 subagent 能在同一 LifecycleRun 内共享 lifecycle-level 信息交换面。
- [ ] 给出 Lifecycle tracking 与 Workflow executable graph config 的分层模型，说明 Workflow 如何在 Lifecycle 下生效，Activity 状态如何被 Lifecycle 追踪。
- [ ] 给出 Story -> TaskAgent dispatch 的目标链路，判断 SubjectRef(kind=Task) execution 是 same-run Actor assignment、linked independent LifecycleRun，还是其他形态，并覆盖 Task 数据、RuntimeSession、CapabilityScope 与 artifact projection。
- [ ] 给出 SubjectRef execution traceability 模型，说明 Task view 如何通过 `SubjectRef(kind=Task)` 与 lifecycle subject association 指向 Actor，并经由 Actor Assignment 追溯 ActivityAttemptState 状态和 artifacts，而不把 runtime truth 写回 Task spec。
- [ ] 给出 Agent 状态锚点模型，说明它作为 RuntimeSession 高层封装，如何自上而下管理 CapabilityState、Context projection、MCP/VFS 与 session runtime refs，并如何被 Activity / ActivityAttemptState 追踪。
- [ ] 维护 `agent-operation-predicates.md`，定义描述 Agent 运转的谓词体系：身份、作用域、状态、能力、上下文、执行、产物、等待、状态变化来源。
- [ ] 维护 `agent-operation-predicate-comparison.md`，对照当前代码已能表达的 Agent 运转谓词与目标谓词体系，并用图示标出缺口。
- [ ] 给出 companion + workflow 的目标链路，覆盖 parent context slice、run lineage、control scope 与 interaction gate。
- [ ] 标出现有实现中需要收敛的入口清单，并按概念风险排序。
- [ ] 维护 `discussion-journal.md`，记录讨论中形成的概念判断、推导依据和仍未定论的问题。
- [ ] 维护 `terminology-notes.md`，记录当前名称、混淆来源、候选名称、迁移风险与推荐命名原则。
- [ ] 维护 `semantic-inventory.md`，重新梳理存量概念的语义、边界、混淆来源与目标形态。
- [ ] 维护 `lifecycle-entity-association-map.md`，用图示对照当前 Lifecycle 关联实体、关联状态与预期迭代后的目标关联。
- [ ] 若后续进入实现，补充 `design.md` 与 `implement.md`，并在用户批准后再 `task.py start`。

## Open Questions

1. business-constrained companion agent 是否应统一被建模为 child LifecycleRun，而不是普通 companion session + workflow overlay？
2. TaskAgent 的默认启动单位应是 `SubjectRef(kind=Task)` 的 independent run，还是 Story run 内的某个 Actor，并由 Actor assignment 连接 ActivityAttemptState？
3. child run 从 parent run 继承上下文时，默认策略应是显式 allowlist，还是按 scope 自动投影 Story/Task facts？
4. human/platform/companion wait 是否应统一为 durable interaction gate，并作为 Activity executor、tool-created gate，或两者兼容？
5. 如果一个 agent 只是完成当前过程中的一个阶段，它是否应该只是当前 LifecycleRun 的 Activity，而不是 child LifecycleRun？
6. 如果一个 agent 拥有独立 subject、权限边界、可暂停恢复的执行生命周期或独立产品导航入口，这些特征是否足以触发 linked/spawned LifecycleRun？
7. 当前 `WorkflowDefinition` 是否需要重命名为 `ActivityProcedure` / `ActorProcedure`，避免与目标语义里的 Workflow executable graph 混淆？
8. `Session` 是否应在产品/领域文档中统一称为 RuntimeSession，避免继续承载业务 ownership 联想？
9. 同一 LifecycleRun 下并发 Agent Activities 之间的信息交换，应仅通过 typed ports/artifacts，还是需要额外的 lifecycle-scoped shared context / blackboard？
10. child LifecycleRun 在产品和代码命名中是否应改称 spawned/delegated/linked run，避免误导为 definition-level 嵌套？
11. `SubjectRef(kind=Task)` 独立执行是否应被定义为 linked independent LifecycleRun：与 Story run 只有 source/projection/lineage 关系，而不默认共享 lifecycle-level context channel？
12. 如果 `SubjectRef(kind=Task)` 由同一 Story LifecycleRun 内的 companion/TaskExecutorAgent 处理，是否应把 `LifecycleRunLink` 迁移为只支持 run / Actor anchor 的 lifecycle subject association，并通过 Actor assignment 追溯 ActivityAttemptState？
13. Lifecycle 内 Agent 状态锚点应叫 `LifecycleActor`、`AgentStateAnchor`、`AgentRuntimeAnchor`，还是其它名字？
14. Agent 状态锚点是否应持久化完整 effective state，还是只持久化 revision/ref + source events，由 projection 还原有效状态？
15. 当前 `ActivityAttemptState` 是否应保留现名，并被严格解释为 Activity execution record，而不是额外引入 `ActivityInvocation` 命名？

## Out Of Scope

- 本任务不直接改动数据库 schema、API 或 runtime 代码。
- 本任务不实现 companion interaction 持久化，该方向由 `05-26-companion-interaction-persistence-model` 继续承载。
- 本任务不实现 lifecycle branching/fork-join，该方向由 `04-21-workflow-lifecycle-branching-design` 继续承载。
