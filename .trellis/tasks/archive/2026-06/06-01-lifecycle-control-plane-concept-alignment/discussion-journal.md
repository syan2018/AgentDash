# Lifecycle 执行控制面讨论 Journal

## 2026-06-01 初始对齐

### 讨论触发

用户指出：Lifecycle 的完整语义应是 Agent 执行的共有生命周期。它可以包括一个 Agent 会话，但不局限于一个 Agent 会话；它更像控制平面，把 Agent 外部环境和 Agent 执行流程整合起来。

用户同时指出当前设计出现概念混淆：

- 早期设计更像绑定单个 Agent。
- 后续希望根据 workflow graph 派发 subagent，并让 subagent 享有 Lifecycle 下的公有上下文。
- companion 没有很好进入这条通道；部分有业务约束的 companion agent 可能需要特殊 lifecycle activity 来约束执行。
- Story / Task / TaskAgent / Companion / Subagent 之间的关系需要重新定义。

### 一手证据

从当前项目文档与实现看，Lifecycle 已经不是简单 session binding：

- `README.md` 将 Lifecycle 空间定义为多步骤工作的端口、产物和事件空间。
- `workflow/activity-lifecycle.md` 规定 Activity lifecycle 是 workflow 运行、编辑和观察的唯一模型，durable advancement 只能通过 `ActivityEvent -> LifecycleEngine`。
- `LifecycleRun` core 注释说明 `session_id` 仅表示 runtime session association，业务归属通过 `LifecycleRunLink` 表达。
- `LifecycleRunLink` 已支持 `Story / Project / RoutineExecution / Task / LifecycleRun / External` subject，以及 `Source / Subject / ProjectionTarget / ControlScope / SpawnedBy` role。
- `StepActivationInput` / `activate_step_with_platform` 已经把 workflow capability、MCP、VFS lifecycle mount、kickoff prompt、companion slice 等执行环境投影收束到同一纯计算入口。

这些证据支持一个判断：Lifecycle 更适合被定义为执行控制面，而不是 Session 的附属物。

### 当前实现的张力

当前代码不是一个干净的目标态，而是多条路径并存：

- ProjectAgent session 打开时自动启动默认 lifecycle 或 freeform run。
- Story session 路径会在 active run 上补 Story subject link。
- Companion `workflow_key` 会为 child companion session 创建 lifecycle run，但没有补 `SpawnedBy`、`Subject`、`ControlScope` 等 links。
- Task service 说明 task execution 不再挂 `lifecycle_activity:*`，但 task session 查询又依赖 `LifecycleRunLink(Task)`；启动路径还没有创建对应 run/link。
- Routine executor 仍是 session prompt dispatch，`RoutineExecution` 的 completed 只是 prompt dispatch 完成。

因此需要收束的不是某一个字段，而是运行入口背后的统一模型。

## 概念推导

### 1. Lifecycle 不应成为所有上下文的聚合根

如果 LifecycleRun 保存所有 Story、Task、Permission、Interaction、Session 上下文，它会变成事实源黑洞，破坏当前 spec 中已经建立的边界：

- Story / Task 保存业务事实。
- LifecycleRun / Activity / ActivityInvocation 保存运行事实。
- PermissionGrant 保存权限事实。
- CapabilityState 保存运行时工具面投影。
- RuntimeSession 保存事件日志、turn、tool call、debug replay。
- LifecycleRunLink 保存跨对象关系。

因此更稳的定义是：LifecycleRun 是执行控制面，负责引用、装配、投影上下文；它拥有执行事实，但不拥有所有被执行对象的业务事实。

### 2. 同一 LifecycleRun 的 Activity 与 child LifecycleRun 需要区分

不能预设所有 subagent / task / companion 都是 child LifecycleRun。它们也可能只是同一 LifecycleRun 内的多个 Activity。

一个执行单元更适合建模为同一 LifecycleRun 下的 Activity，当它满足：

- 它只是当前过程的一步。
- 它的 subject 与 control scope 没有脱离当前 run。
- 它的产物直接进入当前 run 的 ports/artifacts。
- 它不需要独立产品导航、独立生命周期、独立权限租约或独立恢复入口。
- 它的失败、重试、完成语义应直接影响当前 run 的 Activity 状态。

一个执行单元更适合建模为 child LifecycleRun，当它满足：

- 它有独立 subject，例如 Task、另一个 Story、External entity。
- 它有独立 control scope 或权限申请边界。
- 它的运行生命周期可被单独观察、暂停、恢复、取消或重试。
- 它需要独立上下文信道，不应共享父 run 的 lifecycle-level ports/artifacts/VFS/timeline。
- 它的结果需要投影回父对象，但运行事实不应混入父 run 的 activity_state。
- 它需要表达 lineage：由父 run 的某个 activity/attempt 派生。

这个判别规则仍需继续收窄：多 Activity graph 本身不是 child run 判据；只要仍属于同一个被追踪的执行生命过程，复杂子图也应留在同一个 LifecycleTrack 中，由其下生效的 Workflow graph 表达。

### 3. 当前用户编排的 Workflow 与单 Activity procedure 需要分层命名

项目代码里至少存在两个容易混淆的层：

- `ActivityLifecycleDefinition`：目标语义更接近 Workflow，即在 Lifecycle 下生效的可执行图配置，表达 Activities 的演化关系、transition、ports、artifacts、executor kind。
- `WorkflowDefinition`：目标语义更接近 ActivityProcedure / ActorProcedure，即单个 Agent Activity 可引用的 injection / capability / completion contract，表达这个 activity 中 agent 如何工作、能看到什么工具、注入什么上下文。

因此如果用户在界面上“编排工作流”指的是 activity graph，它概念上应直接是 Workflow：一个在 Lifecycle 下生效的可执行图配置。

如果某个 Activity 的 executor 是 Agent，它可以引用当前代码中的 `workflow_key` 指向 `WorkflowDefinition`；但在目标语义里，这个对象应被理解为 ActivityProcedure / ActorProcedure，而不是 Workflow 本身。

### 4. 统一建模的重点不是统一成 child LifecycleRun，而是统一成 dispatch/link/projection 逻辑

用户赞同“统一建模成一致逻辑”，但问题是这套逻辑是什么。

当前更合理的目标是：

```text
launch intent
  -> resolve workflow graph / activity or linked lifecycle policy
  -> create or select LifecycleRun
  -> create LifecycleRunLinks for subject/source/projection/control/lineage
  -> compute context projection and capability projection
  -> attach RuntimeSession only when executor needs runtime log/resume/debug
  -> ActivityEvent drives durable advancement
```

这套逻辑允许两种执行形态：

- same-run Actor assignment：父 LifecycleRun 内分配 Actor，并由 Actor 推进 ActivityInvocation。
- child-run dispatch：创建一个新的 LifecycleRun，通过 `SpawnedBy` 等 link 关联回父 run。

二者共享 link/projection/capability/runtime association 机制，而不是共享同一个粗糙的 session binding。

## 待继续确认

### Q1. TaskAgent 默认应是哪种形态？

初步倾向：TaskAgent 如果对应用户可见 Task spec，并且需要独立状态、权限、恢复和结果投影，则更像 child LifecycleRun。父 Story run 通过 dispatch activity 创建 Task spec 并派生 child run。

但如果某个“task”只是父 lifecycle 内部不可独立导航的一步，则它可以只是 Activity。

### Q2. business-constrained companion 默认应是哪种形态？

初步倾向：普通 companion 是交互信道；business-constrained companion 如果有 workflow contract、权限边界、独立 subject 或可恢复执行，就应走 lifecycle-backed dispatch。它可能是 child LifecycleRun，也可能是当前 run 的 Agent Activity，取决于是否满足 child-run 判别规则。

### Q3. Story root agent 是 LifecycleRun 本身，还是 ProjectAgent session 上的 freeform run？

当前实现存在 Story session + freeform run + Story link 的路径。目标上需要确认 Story root agent 是否应成为 Story subject 的 root LifecycleRun，而不是 session-first freeform 补 link。

### Q4. interaction gate 属于 Activity executor 还是 tool-created gate？

Human activity 已存在，但 companion human/platform wait 还在 session event、wait registry、pending action 之间分散。后续需要确认 durable interaction gate 是否是一种 Activity executor、tool-created run gate，或两者共享同一 interaction table。

## 下一步讨论建议

下一轮应先确定 same-run Actor assignment 与 child LifecycleRun 的判别规则是否成立。确认后再分别套入四个场景：

- Story root agent。
- Story -> TaskAgent dispatch。
- companion_request target=sub + workflow_key。
- Routine inspection -> create Story / Task。

## 2026-06-01 命名整理补充

用户补充：如果有机会处理这些系列概念更清晰的命名，会非常赞同。

这个方向应进入任务范围，因为当前混淆不只是模型边界，也来自名称复用：

- "workflow" 既可能被用户理解成可编排的活动图，又在代码中表示单个 Agent Activity 的行为/注入/能力契约。
- "session" 历史上承载过业务归属，但目标模型中只应是 runtime log/resume/debug substrate。
- "companion" 同时指交互通道、subagent 派发、human/platform request 和业务受限 agent。
- "task agent" 容易让人误以为存在固定 runtime 实体，但它可能只是 agent role / assignment，具体运行形态应由 dispatch policy 决定。

因此新增 `terminology-notes.md` 作为术语整理文档。它先记录候选命名和命名原则，不直接承诺批量重命名。后续如果进入实现，应先决定哪些命名只更新文档，哪些要触及 domain/API/DTO/schema。

## 2026-06-01 Lifecycle、Workflow 与 ActivityProcedure 分层补充

用户进一步明确：希望把 Lifecycle 追踪平面与可执行编排拆开。后续又校准为：Lifecycle 的核心是生命周期追踪；Workflow 是一个在 Lifecycle 下生效的可执行图配置实例；单个 Agent Activity 内部的行为/能力/上下文约束另称 ActivityProcedure / ActorProcedure。这样多个并发 subagent 才有机会在真正的 Lifecycle 层进行信息交换。

这个判断对模型很重要：

- Lifecycle 追踪 Actor 状态、Activity 状态、ActivityInvocation、能力/上下文变化、等待、产物和因果。
- Workflow 是跨 Activity 的可执行图配置，表达并发、join、transition、ports、artifacts、human/function/agent activity 等关系。
- 当前 `WorkflowDefinition` 不应再被理解为 Workflow，而应被约束为某个 Agent Activity 内部的行为/能力/上下文/完成方式契约。
- 多个 subagent 如果只是同一个大过程里的并发执行者，更适合成为同一 `LifecycleRun` 下的多个并发 Agent Activities，而不是天然拆成多个 child LifecycleRun。
- 这些并发 Agent Activities 可以各自拥有 RuntimeSession / ActivityInvocation，但共享同一个 `LifecycleRun` 的 lifecycle VFS、artifact namespace、run context 与 event timeline。

因此，child / spawned LifecycleRun 的触发标准应更严格：它不是“出现了 subagent”就创建，也不是“存在多 Activity 子图”就创建，而是在这个 subagent/任务拥有独立 subject、独立生命周期、独立权限边界、独立导航或独立上下文信道时才创建。

### 信息交换面推导

如果并发 subagent 位于同一 LifecycleRun 下，信息交换不应走 parent session 的自由文本，也不应依赖 companion channel 的临时回流。更合适的交换层是 Lifecycle 自身：

- typed output ports：一个 Activity 明确产出结构化结果。
- artifact bindings：后继或并发汇聚 Activity 消费前驱产物。
- lifecycle VFS：所有相关 Activity 都可看到 run-scoped artifacts 与声明式 mounts。
- run-scoped context projection：把已完成 Activity 的摘要、端口状态、artifact index 投影到当前 Activity。
- event timeline：观察和审计所有 Activity / Invocation 的推进。

仍待确认：typed ports/artifacts 是否足够支撑并发 subagent 的协作；若需要非线性协作，是否需要 lifecycle-scoped shared context / blackboard，并明确其事实源与冲突处理规则。

## 2026-06-01 child LifecycleRun 必要性讨论

用户追问：在 Lifecycle tracking、Workflow graph 与 ActivityProcedure 拆开后，是否还存在需要 child LifecycleRun 的情景？

初步结论：仍然需要，但语义应明显收窄。child / spawned LifecycleRun 不应表示“派发了一个 subagent”，也不应仅仅因为出现了多 Activity 子图就创建；它应表示“产生了一个新的执行控制边界 / 上下文信道边界”。

### 不需要 child LifecycleRun 的情况

如果多个 subagent 只是同一个大过程中的并发执行者，它们应优先建模为同一 `LifecycleRun` 下的多个 Agent Activities：

- 它们共享同一个 subject / control scope。
- 它们的产物通过同一个 lifecycle-level ports / artifacts / VFS 交换。
- 它们的完成、失败、重试都应直接进入父 run 的 Activity 状态。
- 它们不需要独立产品导航、独立暂停/恢复/取消，或独立权限租约。
- 即使它们形成一个复杂的多 Activity 子图，只要仍属于同一个被追踪的执行生命过程，也应留在同一个 LifecycleTrack / Workflow graph 中。

这种情况下，额外 child run 只会制造不必要的层级和上下文同步问题。

### 仍需要 child LifecycleRun 的情况

child / spawned LifecycleRun 适合表达新的控制边界，而不是表达“子图”本身：

1. **独立 subject**
   - 例如 Story run 创建了一个用户可见 Task。
   - Task 有自己的 spec、状态投影、权限边界、产物视图。
   - Task execution 可以直接启动自己的 lifecycle；父 Story run 只保留 source / projection / optional wait 关系，不默认共享 lifecycle-level context channel。

2. **独立上下文信道**
   - 子执行不应看到父 LifecycleRun 的完整 artifact namespace、event timeline 或 shared context。
   - 父 run 只向它提供启动输入、scope grant 或明确的 context snapshot。
   - 子 run 结束后通过 projection / artifact import / summary adoption 把结果回写，而不是持续共享同一 lifecycle exchange surface。

3. **独立权限与控制范围**
   - 子执行需要不同的 `ControlScope`、权限申请、VFS/mount 能力或工具面。
   - 这些授权不应污染父 run 的 ActivityInvocation。

4. **独立生命周期管理**
   - 子执行需要被单独暂停、恢复、取消、重试、观察，甚至在父 run 完成后继续存在。
   - 此时 RuntimeSession 仍只是执行载体，LifecycleRun 才是可管理的运行单元。

5. **跨业务对象投影**
   - Routine inspection 创建 Story。
   - Story agent 创建 Task。
   - 一个 run 的结果投影到另一个 Story / Task / External subject。
   - 这些都需要 `LifecycleRunLink(Source / ProjectionTarget / SpawnedBy / ControlScope)` 保留 lineage。

### Task 独立执行的重新表述

Task 独立执行不一定应该被称为“父 Story lifecycle 的 child LifecycleRun”。更准确的表达可能是：

- Task 拥有自己的 LifecycleRun。
- 这个 run 通过 `Subject=Task`、`ProjectionTarget=Task` 与 Task 关联。
- 如果它由某个 Story run/activity 创建或派发，再额外通过 `Source` 或 `SpawnedBy` link 记录 lineage。
- 它默认不共享父 Story run 的 lifecycle-level ports/artifacts/VFS/timeline。
- 它可以消费 Story/Task 的业务上下文投影，但这是 launch/context projection policy，而不是共享同一个 LifecycleRun exchange surface。

这样 Task execution 是“linked independent lifecycle”，而不是父 Lifecycle 的内部 Activity，也不是必须共享父 Lifecycle 上下文信道的 child graph。

### 进一步修正：Task 可能并不需要 independent run link

用户进一步指出：如果 StoryAgent 在会话内创建并处理 Task，它可能直接创建一个 companion agent，并为这个 companion 授予适合处理 `SubjectRef(kind=Task)` 的 Activity Graph。这个场景下 Task 数据对象并不需要成为 linked independent LifecycleRun。

这个判断进一步收窄了 linked/spawned run 的必要性：

- StoryAgent 与 TaskExecutor/companion agent 仍处在同一个 LifecycleRun / LifecycleTrack。
- SubjectRef(kind=Task) 对应的是这个 run 内的某个 Actor / Agent Activity 或 Activity subgraph。
- companion agent 可以拥有适合该 SubjectRef 的 ActivityProcedure / capability / context projection。
- Task 数据对象不需要自己的 run；用户可见 Task view 需要的是能追溯“哪个 Actor / ActivityInvocation 产生了我的投影”。

因此问题从 “Task 是否 link 到 child run” 转为：

```text
Task
  -> LifecycleRun
  -> SubjectRef
  -> Actor / ActivityInvocation
  -> ExecutorRunRef / RuntimeSession
```

当前 `LifecycleRunLink` 只表达 run 与 subject 的关系：

```text
LifecycleRunLink(run_id, subject_kind, subject_id, role)
```

它不直接表达某个 `SubjectRef(kind=Task)` 在同一 run 内由哪个 Actor 处理，以及该 Actor 对应哪次 Activity 执行记录。因此如果 Task 作为 Activity payload / user-facing subject 留在父 LifecycleRun 内，`LifecycleRunLink(Task)` 会过粗：它能说“这个 run 关联 Task 数据对象”，但不能说“SubjectRef(Task:A) 由 Actor X 处理，并由 activity=implement_task invocation=2 产生投影证据”。

### 新的关联缺口

这里需要的最小修正不一定是新增一个平行的 `ActivitySubjectLink`。用户进一步指出：更可能应该直接把 `LifecycleRunLink` 迁移成可指定 lifecycle anchors 的 subject association，而不是制造大量冗余概念。后续又进一步校准：Task 本体不应拥有 runtime 语义；runtime 侧只处理 `SubjectRef(kind=Task)`。该 SubjectRef 的主关联锚点更适合是 Actor，ActivityInvocation 是 Actor 的执行位置与证据链。

这个方向更符合当前模型的收束目标。也就是说，保留一个统一的 lifecycle subject association 概念，但让它的 anchor 不只停在 run-level：

```text
LifecycleSubjectAssociation / evolved LifecycleRunLink
- run_id
- anchor:
    - run
    - actor { actor_id }
    - activity { activity_key }
- invocation { activity_key, invocation }
- subject_kind: task | story | external
- subject_id
- subject_payload_ref
- role: subject | projection_target | source | control_scope
- metadata
```

这样 Task 数据对象、SubjectRef、Actor 与 ActivityInvocation 的关系不需要另起多套概念，只是同一 association 在 lifecycle 内定位得更精确：Task 先被表示为 SubjectRef，SubjectRef 关联 Actor，Actor 再通过 assignment 关联 ActivityInvocation。

如果用更数据库友好的过渡形态，可以是：

```text
LifecycleRunLink
- run_id
- actor_id: Option<String>
- activity_key: Option<String>
- invocation: Option<u32>
- subject_kind
- subject_id
- subject_payload_ref: Option<String>
- role
- metadata
```

语义约束：

- `actor_id = null, activity_key = null, invocation = null` 表示 whole-run association。
- `actor_id != null, activity_key = null, invocation = null` 表示 Actor-level subject association。
- `activity_key != null, invocation = null` 表示 Activity-level association。
- `activity_key != null, invocation != null` 表示 Invocation-level association。
- `invocation != null` 时必须有 `activity_key`。

这个方向也提示 `LifecycleRunLink` 的名字可能需要后续演化为 `LifecycleSubjectLink` / `LifecycleSubjectAssociation`，因为它不再只 link run，而是把 SubjectRef 关联到 lifecycle 内的执行锚点。

### 更新后的倾向

默认 Task execution 不应先假设为 child run。更自然的路径是：

1. StoryAgent 在 Story LifecycleRun 内创建 Task 数据对象 / 用户可见 view object。
2. Story LifecycleRun 中生效的 Workflow graph 出现一个 Task execution Activity 或 Activity subgraph。
3. 该 Activity 的 payload 携带 `SubjectRef(kind=Task)`，并派发 companion / TaskExecutorAgent。
4. 系统记录 `SubjectRef(kind=Task)` 与 Actor 的 lifecycle subject association。
5. Actor assignment 记录 Actor 当前或历史对应的 ActivityInvocation。
6. Task view 从 SubjectAssociation、Actor、ActivityInvocation 状态、artifact outputs 投影执行状态。

只有当 `SubjectRef(kind=Task)` 需要独立生命周期、独立上下文信道、独立权限边界、独立导航或脱离 Story run 继续执行时，才升级为 independent LifecycleRun；Task 本体仍只是数据/视图对象。

这也说明 `LifecycleRunLink` 的现有概念需要演化：它的 role/subject 方向是对的，但 anchor 粒度不够。更好的收束不是新增 link 种类，而是把 subject association anchor 从 run 扩展到 Actor / Activity / Invocation。

## 2026-06-01 存量概念语义重梳理

用户指出：Activity 本身的定义也很微妙，应该从语义上重新梳理之前存量的诸多概念。

这应成为本任务的核心产物之一。当前混淆不是单点命名问题，而是多个概念在不同历史阶段承载过不同职责：

- Lifecycle 曾像 session lifecycle，后来演化成 execution control plane。
- Workflow 这个名字在代码中曾被用于单 Activity agent behavior contract；目标命名里它应回到可执行图配置，旧含义更适合改名为 ActivityProcedure / ActorProcedure。
- Activity 既是图节点，又是执行边界，又可能触发 Agent/Function/Human/Companion。
- Session 曾像业务会话，现在目标上只是 runtime substrate。
- Companion 既是交互通道，又被当作 subagent 派发机制。
- Task 既是业务工作项，又曾通过 lifecycle_step_key 映射 runtime。
- Link 目前是 run-level association，但 Task traceability 需要 lifecycle anchor 粒度。

因此新增 `semantic-inventory.md`，用来逐一整理每个概念的：

- 当前代码含义。
- 目标语义。
- 不应继续承载的职责。
- 与其他概念的关系。
- 是否需要改名或迁移。

## 2026-06-01 Agent 状态锚点与谓词体系

用户进一步指出：Lifecycle 中可能需要新增一个类似当前 session 的标识物，用来锚定 Agent 状态。Agent 的状态可能受到 Activity 改变，但这种改变也应可追踪、可管理。当前存在于 Lifecycle 层的一部分信息也应移动到这一层里来。

这说明现有三层仍有缺口：

- `LifecycleRun` 适合表达整个执行控制面，但太大，不应直接承载某个 Agent 的细粒度运行状态。
- `ActivityInvocation`（当前代码中的 `ActivityAttemptState`）适合表达某个 Activity 的一次执行记录，但它是执行证据，不是 Agent 自身状态。
- `RuntimeSession` 适合承载 turn / event log / connector resume / debug replay，但它不应继续作为业务或 lifecycle 语义中心。

因此需要考虑一个 Lifecycle 内的 Agent 状态锚点，暂名：

- `LifecycleActor`
- `AgentStateAnchor`
- `AgentRuntimeAnchor`

### 目标职责

这个锚点应描述“某个 Agent/actor 在某个 LifecycleRun 中如何运转”：

- 它属于哪个 `LifecycleRun`。
- 它当前被哪个 Activity / Invocation 驱动或约束。
- 它使用哪个 ProjectAgent / executor config / ActivityProcedure。
- 它当前有效的 capability / MCP / VFS / context projection 是什么。
- 它背后有哪些 RuntimeSession / turn / executor run refs。
- 它因哪些 ActivityEvent / RuntimeCapabilityTransition / interaction gate 发生状态变化。

这样 Activity 改变 Agent 状态时，不是直接“改 session”，而是产生可追踪的 actor state transition：

```text
ActivityInvocation
  -> ActorStateTransition
  -> LifecycleActor revision
  -> runtime/session/capability projection update
```

### 从当前层挪出的候选信息

一些当前散落在 Lifecycle activation、Session construction 或 Session runtime 的信息，可能更适合挂到 Agent 状态锚点上：

- `ExecutorRunRef::AgentSession { session_id }`：应成为 actor 与 RuntimeSession 的一个 runtime ref，而不是唯一身份。
- Activity-level `workflow_key` / effective workflow contract：应说明当前 actor 被哪个 Activity workflow 约束。
- `CapabilityState` / MCP / VFS overlay：应作为 actor 的 effective runtime surface revision，可由 source records replay。
- companion slice / parent context slice：应成为 actor 的 context projection policy 或 inherited context ref。
- current activity / invocation：应成为 actor 当前状态谓词，而不是只能从 run attempts 反查。

### 谓词体系方向

用户希望有一套清晰的谓词体系描述 Agent 运转。初步可以按这些谓词族组织：

- identity：`actor_in_run(actor, run)`、`actor_uses_agent(actor, project_agent)`。
- execution anchor：`actor_assigned_to(actor, activity)`、`actor_running_invocation(actor, invocation)`。
- runtime backing：`actor_backed_by_session(actor, runtime_session)`、`invocation_uses_executor_run(invocation, executor_ref)`。
- state：`actor_status(actor, idle|ready|running|waiting|blocked|completed|failed)`。
- capability：`actor_has_capability(actor, capability, source, scope)`。
- context：`actor_sees_context(actor, context_ref, source)`、`actor_mounts_vfs(actor, mount_ref)`。
- subject：`actor_acts_on(actor, subject)`、`activity_targets(activity, subject)`。
- exchange：`invocation_produces(invocation, artifact)`、`invocation_consumes(invocation, artifact)`。
- causality：`state_changed_by(actor_revision, activity_event|runtime_command|interaction)`。
- wait/gate：`actor_waits_on(actor, gate)`、`gate_resolved_by(gate, response)`。

后续新增 `agent-operation-predicates.md` 专门收敛这套谓词，不直接承诺数据表形态。

## 2026-06-01 Lifecycle tracking 与 Workflow config 关系校准

用户进一步校准：Lifecycle 有更核心的定义，是“生命周期的追踪”。Workflow 仅限于一个可执行图的配置实例，用于在 Lifecycle 下生效。

这修正了前面 clean-slate 讨论里一个容易滑偏的点：不能把 `Lifecycle` 仅仅当成“大图”的产品名，也不能让 `Workflow` 取代 Lifecycle 的核心语义。

新的分层应是：

```text
Lifecycle
  追踪一个执行生命过程：Actor 状态、Activity 状态、ActivityInvocation、能力/上下文变化、等待、产物、因果。

Workflow
  一个可执行图配置实例：Activities、edges、join/branch、ports、activity executor specs。

Activity
  Workflow 图中的节点；其运行状态、invocation、actor 影响由 Lifecycle 追踪。

ActivityProcedure / ActorProcedure
  单个 Agent Activity 内的局部行为/能力/上下文契约。
```

因此：

- Workflow 描述“应该如何执行”。
- Lifecycle 追踪“执行生命过程实际上如何演化”。
- Activity 是 Workflow 配置里的节点，同时也是 Lifecycle 追踪中的状态锚点。
- Actor 是 Lifecycle 中被 Activity 改变的 Agent 状态锚点。

这个校准也影响命名：

- 当前 `ActivityLifecycleDefinition` 更像目标语义里的 `Workflow`，即 Lifecycle 下生效的可执行图配置。
- 当前 `LifecycleRun` 更像 `LifecycleTrack` / `LifecycleRecord` / `TrackedLifecycle`。
- 当前 `WorkflowDefinition` 更像 `ActivityProcedure` / `ActorProcedure`，因为它只描述单个 Activity 内 Agent 的局部演化方式。

### 命名上的进一步推导

child LifecycleRun 可能不应被产品语义称为“子生命周期”。更准确的说法可能是：

- spawned run
- delegated run
- linked run
- child execution run

这样可以避免用户以为 Lifecycle 必须结构性嵌套。实际需要的是 runtime lineage：某个 run/activity/invocation 派生了另一个 run。

### 当前倾向

child / spawned LifecycleRun 应作为少数但重要的 composition primitive 存在。默认并发协作与复杂子图走 same-run Activities；只有出现独立 subject、独立上下文信道、独立控制边界、独立生命周期或跨业务对象投影时，才升级为 linked/spawned LifecycleRun。

## 2026-06-01 Workflow 命名二次确认

用户进一步强调：`Workflow` 这个命名更符合常识，应仅限于“一个可执行图的配置实例”，用于在 Lifecycle 下生效。Lifecycle 更核心的定义是生命周期追踪，不应被命名或文档滑成“大的 workflow graph”。

据此，本任务文档里的目标词汇收束为：

- `Lifecycle`：执行生命过程的追踪平面。
- `Workflow`：Lifecycle 下生效的可执行图配置。
- `Activity`：Workflow 中被 Lifecycle 追踪的执行节点。
- `ActivityProcedure` / `ActorProcedure`：单个 Agent Activity 内部的行为、能力、上下文与完成约束。

这个确认意味着：如果后续产品 UI 里叫“Workflow Builder”，它应该编辑的是 Activity graph；如果代码中仍存在 `WorkflowDefinition` 指向单个 Agent Activity 的局部契约，那么更清晰的目标名应是 `ActivityProcedure` 或 `ActorProcedure`。

## 2026-06-01 Lifecycle 关联实体图示对照

用户希望新增一篇文档，对照画出当前 Lifecycle 关联的系列逻辑实体，并标识它们的关联状态，以及预期迭代后的关联状态。

新增 `lifecycle-entity-association-map.md`，它把当前模型拆成四层：

- Definition layer：`ActivityLifecycleDefinition`、`ActivityDefinition`、`ActivityTransition`、当前 `WorkflowDefinition`。
- Lifecycle run layer：`LifecycleRun`、`ActivityLifecycleRunState`、`ActivityAttemptState`、`ExecutorRunRef`。
- Runtime session layer：`RuntimeSession`、`CapabilityState`、MCP/VFS/kickoff prompt 等 activation output。
- Association layer：`LifecycleRunLink` 与 Story / Task / Project / RoutineExecution / External。

该文档同时画出目标关系：

- `Lifecycle` 是 tracking plane。
- `Workflow` 是可执行图配置。
- `ActivityProcedure` / `ActorProcedure` 是单个 Agent Activity 的局部契约。
- `Actor` 与 `ActorFrame` 补上 Agent 状态锚点和有效运行表面。
- `LifecycleSubjectAssociation` 演化自 `LifecycleRunLink`，覆盖 whole run、Actor、Activity 与 Invocation anchor。

文档还单独画了 same-run SubjectRef execution 与 independent Subject Lifecycle 两个场景，用来区分“SubjectRef(kind=Task) 在同一 LifecycleTrack 内由 Actor 处理，并经由 Assignment 追溯 ActivityInvocation”和“SubjectRef 拥有独立 tracking boundary”的关系。

## 2026-06-01 SubjectRef Association 应优先绑定 Actor

用户指出：相比把 Task 直接绑定到 Activity/Attempt，把它绑定到 Actor 更合适。随后进一步校准：Task 本身不应有任何 runtime 含义，它只是数据载体、用户关联查看对象，或者 Activity payload 指向的数据对象；runtime 侧应处理的是 `SubjectRef(kind=Task)`。

这个修正成立，原因是：

- Activity 是 Workflow 中的执行槽位，表达“流程在哪里”。
- ActivityInvocation 是某个 Activity 的一次执行记录，表达“这次运行证据在哪里”。
- Actor 才是有状态的 Agent 参与者，表达“谁在处理这个 SubjectRef”。
- Task 作为业务数据和用户视图对象，不进入 runtime 语义；`SubjectRef(kind=Task)` 才进入 lifecycle association。

因此 same-run Task execution 的目标谓词应从：

```text
Task data -> Activity/Attempt -> RuntimeSession
```

改为：

```text
Task data -> SubjectRef(kind=Task) -> Actor -> Assignment(ActivityInvocation) -> RuntimeTrace
```

这也让 `ActorFrame` 的位置更清楚：SubjectRef-specific capability、context projection、ActivityProcedure、RuntimeTrace refs 都应挂在 Actor/ActorFrame 上，而 Activity/Invocation 负责提供 workflow position、execution status、outputs 与 causality。

`LifecycleSubjectAssociation` 的目标 anchor 也应从 `run | activity | invocation` 扩展为：

```text
anchor = run | actor | activity | invocation
```

其中 `SubjectRef(kind=Task)` 默认使用 actor anchor；Activity/Invocation anchor 更多用于非 Agent executor、产物证据、精确投影或 lineage。Task entity 不因此获得 runtime 逻辑。

## 2026-06-01 Attempt 命名降级为 ActivityInvocation

用户追问：是否一定要单独扯出 `Attempt` 这个命名。

当前判断：需要保留“某个 Activity 的一次 executor execution record”这个事实，但不一定要把它命名为 `Attempt`，更不应把它作为产品侧核心词。它存在的原因是：

- 同一个 Activity 可能被重试、恢复、取消后重跑，或由不同 executor 产生多次运行记录。
- output artifacts、executor_run_ref、started/completed/status、claim/idempotency 需要挂在某一次具体执行上。
- Actor 的 assignment 需要指向一个可追溯的执行记录，而不是只指向 Activity 定义节点。

但 `Attempt` 这个词强调 retry 语义，容易让模型显得像“失败重试系统”。如果 clean-slate 命名，当前更倾向：

```text
ActivityInvocation
```

它表示“Activity 被执行器调用/承接的一次记录”，retry 只是多个 invocation 的一种来源。当前代码里的 `ActivityAttemptState` 可以被理解为 `ActivityInvocation` 的现有实现名。

## 2026-06-01 ActivityAttemptState 命名修正

用户指出：既然当前已有 `ActivityAttemptState` 这个同名状态，硬要把它改成 `ActivityInvocation` 没有必要，反而会让讨论新增一层无价值命名。

修正后的判断：

- 保留 `ActivityAttemptState` 作为 Activity execution record 的概念名。
- `Attempt` 在这里表达 activity execution record / sequence，不要求产品侧围绕 retry 建模。
- 后续文档不再把 `ActivityInvocation` 作为目标命名；只在必要时用 “execution record” 解释 `ActivityAttemptState` 的语义。
- Journal 是讨论记录，应追加修正和推导过程，不应重写前文判断。

## 2026-06-01 Agent 运转谓词现状对比

用户要求重新评估当前项目对 Agent 运转的谓词，并与目标谓词体系画图对比。

本轮重新从代码里抽取了事实来源：

- `LifecycleRun`、`ActivityLifecycleRunState`、`ActivityAttemptState`、`ActivityExecutionClaim` 表达 run / activity / attempt / executor start。
- `LifecycleRunLink` 表达 run 与 Story / Task / Project / RoutineExecution / LifecycleRun / External 的 whole-run association。
- `session_association` 通过 `session_id -> run -> running/claiming attempt` 反推 activity attempt association。
- `SessionRunContextResolver` 从 run links 推导 project/story/task scope。
- `StepActivation` 从 owner scope、active activity、workflow contract、run id、available MCP/presets 等计算 `CapabilityState`、VFS、MCP 与 kickoff prompt。
- Task view projector 仍通过 `Task.lifecycle_step_key == ActivityAttemptState.activity_key` 将 attempt 状态投影回 Task view。
- Companion dispatch 当前主要落在 `SessionMeta.companion_context`、runtime event 与 `CompanionWaitRegistry`，workflow overlay 可创建 lifecycle run，但 parent / subject / gate 关系尚未进入 lifecycle association。

形成的新文档是 `agent-operation-predicate-comparison.md`。核心判断：

- 当前已有可靠的 Activity execution record 谓词，但缺少一等 Actor。
- 当前 Subject association 只有 whole-run 粒度，无法表达 `SubjectRef(kind=Task)` 由哪个 Actor 处理。
- 当前 capability/context surface 落在 session / hook runtime，而不是 ActorFrame revision。
- 当前 companion / wait 的运行事实主要是 session runtime fact，尚未变成 lifecycle gate / actor assignment。
- 目标谓词体系的核心补洞是 `Actor`、`ActorFrame`、`Assignment`、`SubjectRef`、`LifecycleSubjectAssociation(anchor = run | actor | activity | attempt)`、`Gate` 与 `ActorRevision`。

## 2026-06-01 Actor 作为 Session 高层封装

用户进一步校正：Actor 本身更适合作为 Session 的高层封装；capability / context / session runtime 应有一个自上而下的事实源负责管理；Activity / Attempt anchor 很可能是历史文档里的过度发挥，不需要存在。

修正后的判断：

- Actor 不只是 attempt 旁边的执行者标记，而是 RuntimeSession 之上的高层运行封装。
- Actor / ActorFrame 应成为 capability、context、VFS、MCP、RuntimeSession refs 等 runtime surface 的事实管理层。
- Activity 可以改变 Actor 状态；ActivityAttemptState 记录 Activity 的一次执行事实。
- Subject association 不需要挂 Activity / Attempt。它只需要覆盖 run / Actor anchor。
- ActivityAttemptState 通过 Actor assignment、artifacts、event log 提供执行证据，而不是成为业务 Subject 的关联锚点。

这一修正让目标谓词更瘦：

```text
LifecycleSubjectAssociation(anchor = run | actor)
Actor wraps RuntimeSession
ActorFrame manages capability/context/runtime surface
Actor assigned to Activity / ActivityAttemptState
Task view projects from SubjectRef + Actor assignment + ActivityAttemptState/artifacts
```

## 2026-06-01 文档去重与核心目标收束

用户确认目标结构合理，并强调最核心的考虑是把项目里散落的 Agent runtime 全部收束到 Lifecycle 这条线上。

本轮文档整理：

- 在 PRD 和语义盘点中明确核心收束目标：`Lifecycle -> Actor -> ActorFrame -> RuntimeSession`。
- 压缩 `terminology-notes.md`，让它只承担命名整理职责，不再重复完整语义模型。
- 压缩 `agent-operation-predicate-comparison.md` 的目标谓词清单，改为引用 `agent-operation-predicates.md`，自身只保留当前/目标差异与图示。
- 保留 journal 历史讨论，不重写既有讨论结论。

## 2026-06-01 Subagent 模块 gap 调研收束

用户要求派发 subagent 分模块评估当前实现与目标状态之间的 gap，并补全可执行的重构计划与问题清单。

本轮拆成四个并行切片：

- backend core / control-plane：确认 `LifecycleRun.session_id`、`SessionRuntime`、`SessionHookSnapshot`、`CapabilityState` 仍是运行事实散落的主要位置。
- business modules：确认 direct Task session 启动与 Companion workflow overlay 是最需要先收束的业务路径；二者都需要进入 SubjectRef / Actor / ActorFrame / Gate 通道。
- persistence / contracts：确认 schema 与 wire contract 已把 session-first 形状暴露到前端，包括 `lifecycle_runs.session_id`、`ExecutorRunRef.AgentSession`、`StoryRunOverview.session_id`、`TaskResponse.lifecycle_step_key`。
- frontend：确认 UI 仍以 Session tree 为运行根，Task / Story / ProjectAgent 都有各自的 session owner/launcher 表达。

整合后的判断：

- 当前代码已经具备可靠的 Activity execution evidence，重构重点不是重命名 `ActivityAttemptState`。
- 缺失的一等事实源是 Actor、ActorFrame、ActorAssignment、LifecycleSubjectAssociation、Gate 与 ActorRevision。
- `StepActivation` 与 `SessionConstructionPlan` 是最接近 ActorFrame builder 的既有实现，应优先收束为 frame construction，而不是让 Task / Companion / Routine 各自再创建 runtime bridge。
- Task、Story、Companion、Routine、ProjectAgent 的启动入口应统一进入 `LifecycleDispatchService`，再由它创建或选择 LifecycleRun / Actor / ActorFrame / RuntimeSession。

新增整合文档：

- `module-gap-analysis.md`
- `refactor-plan.md`
- `conceptual-issues.md`
