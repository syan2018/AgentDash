# AgentDash x GSD 核心开发生命周期详细对齐调研

## 1. 结论先行

### 1.1 总判断

如果目标是“用 AgentDash 复刻 GSD 的使用体验”，当前结论可以直接分成两层：

- **作为执行内核，AgentDash 已经足够强。**
  已有 `project / story / task` 仓储层、`SessionHub`、`HookSessionRuntime`、`ActiveWorkflowProjection`、统一 `AddressSpace`、runtime tools、companion / subagent return channel，这些都足以承载“一个明确 unit 如何被执行、约束、观察、收口”。
- **作为项目级自动研发 orchestrator，AgentDash 还不够完整。**
  当前缺的不是 Pi Agent 基础能力，而是 GSD 那层围绕 unit 调度的外围设施：`deriveState()`、`resolveDispatch()`、post-unit closeout、verification auto-fix、git/worktree lifecycle、lock/recovery/stuck detection、metrics/doctor/forensics。

所以更准确的表述应该是：

> AgentDash 已经有 GSD 所需的大部分 runtime kernel，但还没有长成 GSD 那种“项目级自动研发编排器”。

### 1.2 是否能在当前框架下复刻

- **能复刻一部分，而且可以先复刻出一个很像的 step mode / guided mode。**
- **还不能无痛复刻出 GSD 那种稳定的 unattended auto mode。**

换句话说：

- 如果里程碑定义为“在 AgentDash 里跑出 GSD 风格的单步推进、fresh session、聚焦 prompt、unit closeout、人工可介入的编排体验”，当前框架是**可以承接**的。
- 如果里程碑定义为“像 GSD 一样可靠地全自动跑完整 milestone，并具备恢复、锁、超时、worktree、预算和法医分析”，当前框架还**缺几层关键基础设施**。

### 1.3 最重要的架构判断

当前最不建议做的事情，是把现有 `workflow/lifecycle` 直接硬扭成 GSD 项目级 phase machine。

更合理的方向是：

- 保留 `project / story / task` 作为现有业务层级。
- 保留现有 `workflow/lifecycle` 负责 **unit 内部** 的执行约束、completion gate 和 artifact 记录。
- 在其上新增一层 **orchestrator projection / orchestration engine**，负责 **unit 之间** 的状态推导、调度、收口与恢复。

这也是为什么当前最适合的第一阶段，不是“上自动 loop”，而是先做一层只读 projection，把 “如果按 GSD 跑，下一步要 dispatch 什么” 明确化。

---

## 2. GSD 的核心生命周期，到底是什么

如果把 GSD 从 README 和实现里抽象掉产品包装，它的核心生命周期可以被压缩成下面这条主链：

```text
deriveState
  -> resolveDispatch
  -> build focused prompt
  -> fresh session per unit
  -> unit execution
  -> post-unit closeout
  -> verification / auto-fix
  -> state refresh
  -> next unit
```

围绕这条主链，GSD 又叠了四类外围能力：

1. **隔离**
   `worktree / branch / none` 三种 git isolation。
2. **监督**
   lock、timeout、stuck detection、crash recovery。
3. **观测**
   metrics、budget、doctor、forensics、timeline。
4. **人机协作**
   discuss、steer、capture、pause/resume、step/auto。

### 2.1 GSD 的关键设计语义

从 `README`、`auto-mode.md`、`state.ts`、`auto-dispatch.ts`、`auto-verification.ts`、`session-lock.ts`、`auto-worktree.ts` 可以归纳出几个关键语义：

- **state lives on disk**
  `deriveState()` 读取 durable state，再决定下一步，而不是依赖某个常驻内存 orchestrator。
- **fresh session per unit**
  每个 research / planning / execute / validate / complete unit 都是 fresh session。
- **phase -> unit dispatch**
  auto-loop 不是“让同一个 agent 想下一步”，而是显式规则表控制 dispatch。
- **post-unit is first-class**
  unit 执行完不代表流程推进；还要 closeout、verify、commit、rebuild state。
- **supervision is product feature**
  锁、恢复、watchdog、stuck detection 不是 debug 附件，而是 auto mode 的正式组成部分。

---

## 3. AgentDash 当前底座盘点

## 3.1 业务层级已经存在

AgentDash 当前明确已经有：

- `ProjectRepository`
- `StoryRepository`
- `TaskRepository`
- `SessionBindingRepository`
- workflow binding 到 `Project / Story / Task`

也就是说，这里不是“还没有层级”，而是“这些层级还没有被编组成 GSD 那种项目级研发循环”。

## 3.2 session/runtime 内核已经成型

当前可直接复用的内核能力包括：

- `SessionHub`
  session 创建、prompt、follow-up、cancel、interrupted recovery。
- `HookSessionRuntime`
  snapshot / diagnostics / trace / pending actions / refresh / completion signal。
- `AppExecutionHookProvider`
  从业务对象解析 active workflow、约束、注入、completion。
- `ActiveWorkflowProjection`
  lifecycle step + primary workflow + effective contract。
- `AddressSpace` + runtime tools
  `mounts_list / fs_read / fs_write / fs_apply_patch / fs_list / fs_search / shell_exec`。
- `CompanionDispatchTool` / `CompanionCompleteTool`
  受 hook runtime 控制的 subagent / companion 执行与回流。

## 3.3 已有 workflow/lifecycle 更像 unit 内部治理

当前 `trellis_dev_task` builtin lifecycle 的语义是：

```text
Start -> Implement -> Check -> Record
```

这条链明显更接近：

- 一个执行 session 内部如何吸收上下文
- 何时可以停止
- 何时需要 evidence
- 何时记录 artifact

它非常适合做 `execute-task` 这类 unit 的**内部阶段机**，但不太适合直接去扮演 GSD 的外层 milestone orchestrator。

## 3.4 Address Space 已经具备 focused context 的底座

当前 Address Space 不只支持主工作空间，还支持：

- 生命周期只读 mount
- inline/virtual content
- capability slicing
- runtime tool 注册

这意味着如果未来要做：

- focused context preload
- scoped execution mounts
- 子 agent capability slicing
- orchestrator 只读视图挂载

底层能力已经比一般“prompt 拼接型 agent 应用”成熟得多。

---

## 4. 对齐矩阵：逐项看 GSD 核心机能在 AgentDash 能否实现

下面每个章节都按四个问题展开：

1. AgentDash 现在有没有对应能力
2. 哪些模块可以直接复用
3. 如果要对齐，建议怎么实现
4. 还缺什么基础设施 / 风险是什么

---

## 4.1 状态推导：`deriveState()`

### GSD 里这项能力在做什么

`deriveState()` 是 GSD 的总入口。它从 durable state 中推导：

- active milestone
- active slice
- active task
- 当前 phase
- blockers
- next action
- completion / validation readiness

它不是 session 内状态，而是项目级编排状态。

### AgentDash 当前对应能力

**部分有，但不完整。**

AgentDash 当前有两类“投影”能力：

- `ActiveWorkflowProjection`
  能推导某个 binding 当前 active workflow step 与 effective contract。
- session / binding / repo 查询
  能找到 `project / story / task`、session binding、lifecycle run。

但当前还没有一层统一的 **orchestrator state projection** 去回答：

- 当前整个 user case 的 active unit 是谁
- 下一步该跑 research / plan / execute / validate / complete 哪一个
- 哪些 task 已完成、哪些只是 session 完成但还未 closeout
- 当前是不是卡在 verification / recovery / discussion

### 可复用模块

- `crates/agentdash-application/src/workflow/projection.rs`
- `crates/agentdash-application/src/task/session_runtime_inputs.rs`
- `crates/agentdash-domain/src/project/repository.rs`
- `crates/agentdash-domain/src/story/repository.rs`
- `crates/agentdash-domain/src/task/repository.rs`
- `crates/agentdash-domain/src/session_binding/repository.rs`
- `crates/agentdash-application/src/session/hub.rs`

### 建议如何实现

建议新增一层只读 projection，例如：

```text
OrchestratorStateProjection
  - scope_kind
  - scope_id
  - active_phase
  - active_unit
  - blockers
  - next_action
  - ready_units
  - recently_completed_units
  - validation_state
  - recovery_state
```

第一阶段不要急着引入新实体，直接从现有数据源推导：

- `Project / Story / Task`
- session binding
- lifecycle run
- session execution state
- workflow record artifacts

把这层做成只读 projection，就能先验证：

- 现有仓储层足不足以重建 GSD 风格“当前该干什么”
- 哪些字段其实已经有
- 哪些 durable state 还没落

### 缺口 / 风险

当前最大的缺口是：**缺 durable orchestration state。**

GSD 的 `deriveState()` 依赖一套明确的 unit 完成、closeout、validation、recovery 事实来源。AgentDash 当前没有完全等价的“unit ledger”，所以如果马上做 auto loop，会出现两个问题：

- 只能根据 task/story 的粗粒度状态猜测，而不是根据 unit closeout 事实判断。
- 容易把“session 跑完了”误判成“unit 已完成并可推进”。

---

## 4.2 调度派发：`resolveDispatch()`

### GSD 里这项能力在做什么

`resolveDispatch()` 把 phase 映射成下一步的具体 unit：

- dispatch 什么 unit type
- unit id 是谁
- 用哪个 prompt builder
- 是否 pause
- 是否 stop / skip

这是 GSD auto-loop 的规则表核心。

### AgentDash 当前对应能力

**只有雏形，没有正式调度层。**

当前 AgentDash 有：

- `SessionPlanPhase`
  区分 `ProjectAgent / StoryOwner / TaskStart / TaskContinue`
- `build_session_plan_fragments`
  可以按 owner/phase 生成不同 prompt fragment
- `trellis_dev_task` builtin lifecycle
  可以约束 task session 内部阶段

但没有一层统一的：

```text
orchestrator phase -> unit type -> owner role -> prompt recipe -> closeout policy
```

### 可复用模块

- `crates/agentdash-application/src/session_plan.rs`
- `crates/agentdash-application/src/task/context_builder.rs`
- `crates/agentdash-application/src/session/prompt_pipeline.rs`
- `crates/agentdash-application/src/workflow/builtins/trellis_dev_task.json`

### 建议如何实现

建议新增独立的 dispatch registry，例如：

```text
OrchestrationDispatchRule
  - phase matcher
  - unit_type
  - owner_kind
  - session_strategy
  - prompt_recipe
  - post_unit_policy
```

可以先支持最小一组 unit：

- `discuss_scope`
- `plan_scope`
- `execute_task`
- `check_task`
- `record_task`
- `validate_scope`
- `complete_scope`

并明确每类 unit 的 dispatch 输出：

- 目标 owner：`Project / Story / Task`
- 目标 session phase：`ProjectAgent / StoryOwner / TaskStart / TaskContinue`
- 是否 fresh session
- context recipe
- expected artifacts

### 缺口 / 风险

当前缺的不是 prompt 拼接，而是 **调度规则的 authority**。

如果不先建立 dispatch table，就很容易退化成：

- 在 API / UI 层临时判断下一步
- 让 agent 自己决定 phase
- 在 workflow/lifecycle 里塞入项目级判断

这三条路都会让语义变乱。

---

## 4.3 Fresh Session + Focused Prompt + Context Preload

### GSD 里这项能力在做什么

GSD 每个 unit 都创建 fresh session，并在 dispatch 前把聚焦上下文预加载进去：

- 任务计划
- 上游 summary
- roadmap 片段
- dependency context
- decisions
- verification context

这也是它相较 prompt framework 最大的体验差异之一。

### AgentDash 当前对应能力

**这一项是 AgentDash 最接近 GSD 的部分。**

当前已有：

- `SessionHub.start_prompt_with_follow_up`
  可以创建/驱动 session prompt。
- `build_task_session_runtime_inputs`
  能在 task 启动前算出 address space、workflow、mcp。
- `build_task_session_context`
  能生成结构化上下文快照。
- `build_session_plan_fragments`
  能按 owner/phase 生成 address space、tool visibility、persona、workflow、runtime policy 摘要。
- hook runtime injections
  能把 active workflow step / constraints 注入到 loop。

### 可复用模块

- `crates/agentdash-application/src/session/hub.rs`
- `crates/agentdash-application/src/task/session_runtime_inputs.rs`
- `crates/agentdash-application/src/task/context_builder.rs`
- `crates/agentdash-application/src/session_plan.rs`
- `crates/agentdash-application/src/hooks/provider.rs`
- `crates/agentdash-application/src/address_space/tools/provider.rs`

### 建议如何实现

这一层可以直接复用当前框架，但需要在 orchestrator 上层补一个 **unit context recipe**：

```text
UnitContextRecipe
  - base owner context
  - required artifacts
  - optional carry-over summaries
  - mount slicing policy
  - mcp exposure policy
  - workflow attachments
```

也就是说，不用重写 prompt pipeline，只需要让 orchestrator 在 dispatch 前回答：

- 这次应该拉哪些 context block
- 要不要新开 session
- 这个 unit 继承哪些 mount / tool / workflow constraint

### 缺口 / 风险

当前缺的主要不是技术能力，而是 **context recipe 的产品语义**。

没有这层 recipe，虽然能拼装 session，但会遇到两个问题：

- 哪些 artifact 应该作为 unit 的标准输入并不稳定。
- 同一种 unit 在不同 scope 下复用时，很容易上下文过重或过轻。

---

## 4.4 生命周期分层：GSD phase 和 AgentDash owner/session 应该怎么映射

### 关键判断

GSD 的 phase 不是单纯 task phase，而是一条从“讨论范围”到“完成范围”的外层编排线。

AgentDash 当前最自然的分层映射，应该是：

| GSD 能力 | 更像 AgentDash 的哪一层 | 备注 |
|---|---|---|
| discuss / research / plan 一个范围 | `ProjectAgent` 或 `StoryOwner` | 更偏 owner session |
| execute-task | `TaskStart / TaskContinue` | 最接近现有 task session |
| task 内部 implement/check/record | `trellis_dev_task` lifecycle | 属于 unit 内部治理 |
| complete slice / validate milestone / complete milestone | `StoryOwner` 或更高层 orchestrator session | 不应塞进 task 内部 lifecycle |

### 推荐映射

如果第一阶段不新增 milestone 实体，建议用一个“近似映射”来跑 PoC：

- `Project`
  继续表示整个协作空间或项目外壳。
- `Story`
  暂时承载 GSD 里的“一个待推进的 scope”。
- `Task`
  承载实际执行单元。

然后用 orchestrator projection 在 `Story` 之上定义：

- plan story
- execute next task
- validate story
- complete story

### 为什么不建议直接把 Story = Slice，Task = Task 就结束

因为 GSD 真正强的部分，是“一个 scope 被连续推进直到完成”的体验。若只是静态地把 `Story` 当 slice、`Task` 当 task，会丢失：

- 范围级讨论 / 计划 / 验证 / 完成
- 范围级 closeout
- 范围级 re-assess / rethink

### 缺口 / 风险

这里最大的风险不是技术，而是 **语义塌缩**。

如果把已有 `workflow/lifecycle`、`Story`、`Task`、session role 全部混在一起，后面会很难回答：

- 哪些 phase 是 task 内部的
- 哪些 phase 是 story 外层编排的
- 哪些 session 是执行单元
- 哪些 session 是 orchestrator / owner

所以这一层必须先分清“内层 lifecycle”和“外层 orchestration”。

---

## 4.5 Post-Unit Closeout：执行完一个 unit 之后如何收口

### GSD 里这项能力在做什么

GSD 的 unit 执行结束后，不会立刻盲目推进，而是进入 post-unit：

- 记录 summary
- 生成/检查 artifact
- 刷新 state
- 必要时 commit
- 再决定是否进入 verification 或下一个 unit

### AgentDash 当前对应能力

**部分有，而且内层 completion 机制很强。**

当前已有：

- `report_workflow_artifact`
- workflow `completion`
- `apply_completion_decision`
- `SessionTerminal` hook
- checklist evidence artifact
- lifecycle step advance + execution log append

这说明 AgentDash 已经能把“一个 session 结束时是否可以推进内部 step”做得很正式。

### 可复用模块

- `crates/agentdash-application/src/hooks/completion.rs`
- `crates/agentdash-application/src/hooks/provider.rs`
- `crates/agentdash-application/src/workflow/projection.rs`
- `crates/agentdash-application/src/session/hook_runtime.rs`

### 建议如何实现

建议把 closeout 分成两层：

1. **unit 内部 closeout**
   继续由当前 workflow/hook completion 机制负责。
2. **orchestrator 外部 closeout**
   新增 `UnitCloseoutService` 负责：
   - 汇总 session terminal result
   - 读取 workflow artifacts
   - 判断 unit 是否真的完成
   - 更新 orchestrator state / owner status
   - 决定是否进入 verification / next dispatch

也就是说，现有 hook completion 不要废弃，而是升级成 outer closeout 的输入之一。

### 缺口 / 风险

当前缺的核心是：

- **unit result model**
- **expected artifact contract**
- **idempotent closeout record**

没有这三样，orchestrator 很难区分：

- “agent 结束了”
- “workflow step 完成了”
- “这个 outer unit 真正完成并可推进了”

---

## 4.6 Verification / Auto-Fix

### GSD 里这项能力在做什么

GSD 会在 execute-task 之后自动跑 verification commands：

- lint / test / typecheck
- 失败时把输出回灌给 agent
- 自动重试修复若干轮
- 耗尽后暂停给人类

### AgentDash 当前对应能力

**只有局部等价物，没有统一 verification pipeline。**

当前已有：

- task 内部 `check` workflow
- `block_stop_until_checks_pass`
- checklist evidence
- address space `shell_exec`

这些能力说明 AgentDash 可以表达“检查是一个正式阶段”，但还没有 GSD 那种：

- 通用 verification command 配置
- 自动收集失败上下文
- 固定重试次数
- verification evidence 持久化

### 可复用模块

- `crates/agentdash-application/src/workflow/builtins/trellis_dev_task.json`
- `crates/agentdash-application/src/address_space/tools/provider.rs`
- `crates/agentdash-application/src/hooks/provider.rs`
- `crates/agentdash-application/src/hooks/completion.rs`

### 建议如何实现

建议把 verification 设计成 outer post-unit pipeline 的正式阶段：

```text
execute_task
  -> closeout candidate
  -> verification stage
  -> pass: advance
  -> fail + retries left: dispatch autofix unit
  -> fail + exhausted: pause / escalate
```

第一阶段可以先不做独立新实体，而是用一份 `VerificationPolicy`：

- commands
- retry_limit
- autofix_enabled
- advisory vs blocking

并把 verification 失败上下文作为下一轮 task follow-up 的结构化输入。

### 缺口 / 风险

如果没有这一层，AgentDash 可以“看起来很像 GSD 在跑”，但无法提供 GSD auto mode 最关键的心理预期：

- 不是 agent 说“我检查过了”就算过
- 而是系统真的执行了验证并决定能不能继续

---

## 4.7 Git / Worktree Isolation

### GSD 里这项能力在做什么

GSD 的 git/worktree 是产品级 lifecycle：

- create worktree / branch
- per-milestone isolation
- sync state
- squash merge
- teardown
- cleanup / recovery / doctor

### AgentDash 当前对应能力

**底层操作能力有，产品化 lifecycle 基本没有。**

当前能看到的相关底子有：

- `Task.workspace_id`
- Trellis 多 agent / worktree 脚本
- address space + shell tools

但这些还不是“运行工作流的一部分”。也就是说：

- 有 workspace 概念
- 不等于有 GSD 式 git isolation mode

### 可复用模块

- `Task.workspace_id` 相关 task 执行上下文
- Address Space / shell runtime tools
- `.trellis/scripts/multi_agent/*` 中已有的工作树脚本经验

### 建议如何实现

如果目标真的是“复刻 GSD 使用体验”，这一层最终需要正式产品化：

```text
WorkspaceProvisioningPolicy
  - none
  - branch
  - worktree
```

并由 orchestrator 驱动：

- before first unit: provision workspace
- during unit: attach workspace root into address space
- on closeout: optional commit / merge / sync-back
- on completion: teardown / archive / preserve

第一阶段如果只做 step mode，可以先允许 `none`，把 git isolation 标成后续基础设施。

### 缺口 / 风险

当前缺的不只是 git 命令，而是：

- durable workspace lifecycle state
- merge safety policy
- teardown safety
- orphan cleanup

没有这些，就不适合声称“可以放心复刻 GSD 的 auto run 体验”。

---

## 4.8 Lock / Crash Recovery / Timeout / Stuck Detection

### GSD 里这项能力在做什么

这是 GSD unattended auto mode 成立的关键：

- 单 writer lock
- crash-safe resume
- stuck loop detection
- soft/idle/hard timeout
- recovery briefing

### AgentDash 当前对应能力

**session 级恢复有，orchestrator 级监督没有。**

当前已有：

- `SessionHub.recover_interrupted_sessions`
- session 持久化与事件回放
- hook runtime rebuild
- cancel / execution state inspection

这说明 AgentDash 已经有“单个 session 的 durable execution surface”，但没有一层对 **自动编排循环** 本身负责的 supervisor。

### 可复用模块

- `crates/agentdash-application/src/session/hub.rs`
- `crates/agentdash-application/src/session/hook_runtime.rs`
- session persistence / event backlog

### 建议如何实现

建议新增一个轻量 supervisor 层：

```text
OrchestratorSupervisor
  - orchestration lock
  - active dispatch record
  - heartbeat / lease
  - timeout policy
  - stuck window detector
  - recovery briefing builder
```

它不需要一开始就很复杂，但至少要回答：

- 当前有没有自动循环在跑
- 正在跑哪个 unit
- 超时没有
- 崩溃后该从哪一步恢复

### 缺口 / 风险

这是“能不能安全复刻 GSD auto mode”的最大 blocker 之一。

如果没有 supervisor：

- session 能恢复，不代表 orchestration 能恢复
- 同一 scope 可能被重复 dispatch
- UI 可以展示“在跑”，但系统不知道是否其实已经卡死

---

## 4.9 Human-in-the-loop：Discuss / Steer / Capture / Step Mode

### GSD 里这项能力在做什么

GSD 不只是 auto，也支持：

- discuss
- steer
- pause / resume
- capture
- step mode

这些能力让用户可以在外层编排层插手，而不是直接闯进某个执行 session 里说话。

### AgentDash 当前对应能力

**有底层交互通道，但没有编排层入口。**

当前已有：

- Hook `Ask` / approval
- pending hook actions
- companion result adoption
- 用户随时给 session follow-up

但还没有 GSD 那种明确的 orchestration verbs：

- “暂停这个 scope 的自动推进”
- “对当前 plan 做 steer”
- “捕获一个稍后再消化的想法”
- “只跑下一个 unit”

### 可复用模块

- `HookPendingAction`
- `ResolveHookActionTool`
- companion / subagent 回流
- session hub follow-up 能力

### 建议如何实现

建议把 human-in-loop 也挂在 orchestrator 层，而不是直接压给 task session：

- `step_next`
- `pause_scope`
- `resume_scope`
- `steer_scope`
- `capture_note`
- `discuss_scope`

第一阶段最值得先做的是：

- `next action preview`
- `dispatch next unit`
- `pause/resume`

因为这已经足以形成一个可演示的“GSD-like guided mode”。

### 缺口 / 风险

如果没有这层入口，用户就只能：

- 手工找 session
- 手工继续 prompt
- 自己理解当前 phase

体验上就不会像 GSD，而更像一堆强 runtime primitive 的合集。

---

## 4.10 Parallel / Subagent / Reactive Execution

### GSD 里这项能力在做什么

GSD 的高阶能力包括：

- parallel milestone workers
- reactive execution
- subagent based task batches

### AgentDash 当前对应能力

**subagent 基础能力已经有，parallel orchestration 还没有。**

当前 companion/subagent 已经很像一个正式设施：

- `BeforeSubagentDispatch`
- `AfterSubagentDispatch`
- `SubagentResult`
- inherited slices / adoption mode
- pending action 注入

这比很多系统已经更进一步了。

### 可复用模块

- `crates/agentdash-application/src/task/tools/companion.rs`
- hook runtime pending action / trace
- `SessionHub` 多 session 管理

### 建议如何实现

建议把 parallel/reactive 放到后期，不要作为第一批 blocker。

后续若需要，可在 orchestrator 上层增加：

- ready unit set
- conflict policy
- concurrency ceiling
- parallel dispatch record

而 companion 继续作为 worker/subagent transport。

### 缺口 / 风险

如果在没有 orchestration state / verification / workspace isolation 的前提下先做 parallel，复杂度会立刻爆炸：

- 谁拥有 closeout authority
- 如何避免重复写
- verification 针对单 unit 还是 batch
- recover 时如何恢复一半完成的并行批次

所以它明显不该是当前第一阶段目标。

---

## 4.11 Metrics / Budget / Doctor / Forensics

### GSD 里这项能力在做什么

GSD 的 observability 是完整 auto workflow 的一部分：

- unit 级 cost/token ledger
- progress dashboard
- budget ceiling
- doctor
- forensics
- timeline/report

### AgentDash 当前对应能力

**有很多原始事件面，但缺统一观测产品层。**

当前已有：

- session event stream
- hook trace
- diagnostics
- lifecycle execution log
- workflow record artifacts

但没有统一的：

- unit metrics ledger
- orchestration timeline
- anomaly classifier
- budget guard
- doctor/forensics UI

### 可复用模块

- `SessionHub` 事件持久化
- `HookSessionRuntime.trace`
- lifecycle run execution log / record artifacts
- Address Space / lifecycle mount

### 建议如何实现

建议这层也放到 orchestrator 之后：

```text
OrchestrationObservation
  - dispatch history
  - unit result ledger
  - verification history
  - retry history
  - cost/tokens
  - anomalies
```

最小第一步可以先不碰 cost，只先做：

- 当前状态时间线
- 最近 unit
- 为什么停住
- 下一个 unit 预览

### 缺口 / 风险

没有观测层，auto workflow 即使能跑，也很难 debug，更难建立用户信任。

这也是为什么“先做只读 projection”其实很值，它本身就是 observability 的起点。

---

## 5. 哪些 GSD 体验可以先用 AgentDash 复刻，哪些暂时不行

## 5.1 现在就有希望复刻的部分

### A. unit runner 体验

当前已经很接近：

- fresh session
- focused context
- runtime tool visibility
- workflow constraint injection
- task 内 implement/check/record
- structured artifact recording

### B. step mode / guided mode

只要补上：

- orchestrator projection
- next action preview
- dispatch next unit
- unit closeout

就可以做出一个很像 GSD 的 guided mode。

### C. subagent / companion 辅助执行

这一块 AgentDash 甚至已经比很多系统更正式，可以作为后续 parallel/reactive 的底座。

## 5.2 还不能自信复刻的部分

### A. unattended auto mode

缺：

- durable orchestration state
- dispatch ledger
- supervisor / lock / recovery / timeout

### B. verification auto-fix pipeline

缺：

- verification policy
- retry ledger
- failure context to autofix loop

### C. git/worktree lifecycle

缺：

- provision / merge / teardown / cleanup 的产品级治理

### D. doctor / forensics / budget

缺：

- 统一 observation 层

---

## 6. 推荐的 AgentDash 对齐方案

## 6.1 分层方案

建议把未来对齐方案明确分成四层：

### 第 1 层：现有业务层

- `Project`
- `Story`
- `Task`
- session binding

### 第 2 层：现有 unit runtime

- `SessionHub`
- `HookSessionRuntime`
- `ActiveWorkflowProjection`
- `AddressSpace`
- runtime tools
- companion/subagent

### 第 3 层：新增 orchestrator layer

- `OrchestratorStateProjection`
- `DispatchRuleRegistry`
- `UnitContextRecipe`
- `UnitCloseoutService`
- `VerificationPolicy`

### 第 4 层：新增 supervisor/observability layer

- lock / lease
- timeout / stuck detection
- recovery
- metrics / timeline / doctor / forensics

## 6.2 第一阶段最合理的落地顺序

### Phase 1：只读 projection

目标：

- 展示 active scope
- 展示 next action
- 展示 blockers
- 展示 ready units

这是最低风险、最高信息增益的一步。

### Phase 2：step mode

目标：

- 手动 `dispatch next unit`
- 每次只跑一个 unit
- 结束后做 closeout

这一步就已经能跑出“GSD 风格的使用体验”。

### Phase 3：single-scope auto mode

目标：

- 只支持单个 story/scope 的自动推进
- 加上 verification / retry
- 加上最小 supervisor

### Phase 4：完整 GSD-like orchestration

目标：

- queue / park / rethink
- worktree isolation
- crash recovery / watchdog / stuck detection
- metrics / doctor / forensics
- parallel/reactive execution

---

## 7. 真正的 blocker 清单

如果问题是：“GSD 的运行工作流能不能妥善在当前框架下复刻？”

最准确的回答是：

- **内核能复用。**
- **完整 auto workflow 还不能直接妥善复刻。**

当前真正的 blocker 不是 Pi Agent runtime，而是下面这些 orchestration 级基础设施：

1. **统一的 orchestrator state projection**
   现在没有正式的外层 `deriveState()`。
2. **正式的 dispatch rule engine**
   现在没有 `resolveDispatch()` authority。
3. **unit closeout / verification pipeline**
   现在没有外层 post-unit 收口。
4. **durable unit ledger**
   现在缺“这个 unit 已完成且已 closeout”的权威事实。
5. **supervisor**
   现在缺 lock / timeout / stuck detection / recovery。
6. **git/worktree lifecycle**
   现在缺产品级隔离模式。
7. **observability**
   现在缺 timeline / doctor / forensics / budget。

---

## 8. 最终建议

如果这个 user case 的里程碑是：

> “证明 AgentDash 能承载 GSD 风格的完整研发体验，而不是只会开 task session”

那最推荐的路线不是立刻追求 full auto，而是：

1. 先做一层只读 orchestrator projection，建立统一状态视图。
2. 再做 step mode，把 “next action -> fresh session -> closeout” 跑通。
3. 等 closeout / verification / supervisor 最小闭环成立后，再推进 auto mode。

这条路径最符合当前架构现状，也最容易把“AgentDash 作为 GSD user case 容器”的可行性讲清楚。
