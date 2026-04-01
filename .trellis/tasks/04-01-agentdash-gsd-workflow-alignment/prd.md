# 使用 AgentDash 复刻 GSD 工作流体验规划

## 目标

把“使用 AgentDash 复刻 GSD 的使用体验”收敛成一个完整 user case 里程碑，用于判断：

1. AgentDash 现有的 `project / story / task`、session runtime、workflow / lifecycle、address space、companion 等基础设施，已经覆盖了 GSD 哪些核心能力。
2. GSD README 里的“自动研发工作流体验”到底缺失在哪一层，是运行时内核、业务层级、还是外围编排。
3. 若要在当前架构下复刻这套体验，应该按什么层次推进，哪些基础设施是前置条件，哪些可以后补。

本任务当前阶段只做规划记录，不进入实现。

## 详细调研入口

本任务的详细能力矩阵、逐项对齐分析与分层落地建议，已单独整理到：

- `./detailed-alignment.md`

`prd.md` 保留为总览与决策收敛；细节以详细调研文档为主。

## 背景

GSD 的卖点并不只是“底层用 Pi SDK 驱动 agent”。它真正提供的是一套完整的项目级自动编排体验：

- 从 milestone / slice / task 推导当前 phase
- 为下一步生成 dispatch prompt
- 为每个 unit 创建 fresh session
- 运行 verification 与 auto-fix
- 驱动 git / worktree 生命周期
- 提供 lock / recovery / stuck detection / metrics / budget / doctor / forensics

AgentDash 这边已经有非常强的 session runtime / workflow / hook / address space 基础设施，但当前还需要明确：

- 这些基础设施是否已经能承载 GSD 风格的项目级工作流
- 还是说它们更多停留在“session 执行内核”层，而外层 orchestrator 仍需补齐

## 已知结论

### 1. AgentDash 已经有正式的业务层级，而不是只有 session

当前代码库明确存在：

- `project` domain / repository
- `story` domain / repository
- `task` domain / repository
- `session_binding` 用于把 session 绑定到 `project / story / task` owner
- workflow binding 也明确支持 `Project / Story / Task`

这意味着“层级不存在”不是问题，真正的问题是这些层级目前是否已经被组织成 GSD 那种项目级自动推进引擎。

### 2. AgentDash 已经有正式的 session runtime / workflow 内核

当前已经明确落地的能力包括：

- `SessionHub`：session 生命周期、事件持久化、订阅、取消、hook runtime 重建
- `HookSessionRuntime`：snapshot、diagnostics、trace、pending actions、resolve
- `ActiveWorkflowProjection`：active lifecycle step + effective contract
- `workflow artifact`：phase note / checklist evidence / session summary 等结构化产物
- `BeforeTool / BeforeStop / SessionTerminal / BeforeSubagentDispatch / SubagentResult` 等 hook trigger
- `companion_dispatch / companion_complete / resolve_hook_action`
- 统一 address space / runtime tools：`mounts_list / fs_read / fs_write / fs_apply_patch / fs_list / fs_search / shell_exec`

换句话说，AgentDash 在“执行内核”层并不弱，很多地方甚至比 README 里描述的 GSD runtime 语义更清晰。

### 3. 当前缺口更像是“外编排层”，不是“底层 agent 能力”

目前已经确认的差异点主要在 GSD 的外围编排能力：

- disk-backed state derivation
- dispatch engine
- post-unit closeout / verification / commit
- git / worktree productized lifecycle
- session lock / crash recovery / stuck detection / timeout supervision
- metrics / budget / doctor / forensics

因此，当前判断不应再表述为“AgentDash 没有 project/story/task 层级”，而应表述为：

“AgentDash 已有 project/story/task + runtime 内核，但还没有完整长成 GSD 那种项目级自动研发 orchestrator。”

## AgentDash 现有能力整理

### 一. 业务与仓储层

当前项目已经具备可作为 orchestrator 上层输入的基础实体：

- `Project`
- `Story`
- `Task`
- `WorkflowAssignment`
- `LifecycleRun`
- `SessionBinding`

这些实体已经贯通到 repository、API、MCP、session binding 和 workflow binding。

### 二. Session 与 Runtime 层

当前 session 执行层已经提供：

- session 创建、prompt、follow-up、取消、中断恢复
- per-session hook runtime
- hook event trace 和 pending action surface
- stop gate、approval、companion result adoption

这层很适合作为未来 orchestrator 的 `Unit Runner` 底座。

### 三. Workflow / Lifecycle 层

当前 workflow / lifecycle 已经具备：

- lifecycle step 定义
- active workflow projection
- `effective_contract`
- completion check
- artifact-driven completion

它天然适合表达“某个 unit 内部如何推进与何时可结束”，但是否应该直接承担“项目级 phase machine”仍需谨慎。

### 四. Address Space / Tooling 层

当前已经有统一 mount + path 模型，以及对应 runtime tools。

这意味着 orchestrator 若要做：

- focused context assembly
- scoped execution mounts
- read-only lifecycle / snapshot 挂载
- child session capability slicing

底层基础能力基本已具备。

### 五. Story / Task Session Planning 层

当前 session plan 已显式区分：

- `ProjectAgent`
- `StoryOwner`
- `TaskStart`
- `TaskContinue`

这说明系统已经具备“不同 owner / phase 使用不同 session role”的雏形，这对复刻 GSD 的 step mode / auto mode 很关键。

## GSD 工作流体验拆解

### 一. 体验层目标

GSD README 中真正想让用户感知到的是：

- 有一条稳定可解释的研发 phase 线
- agent 每次只做一个明确 unit
- 每个 unit 都在 fresh session 中执行
- 结果会被验证、记录、推进到下一步
- 出错时能恢复，不会默默丢状态
- 用户可以在自动与人工介入之间切换

### 二. 运行层核心组成

从实现和 README 描述来看，GSD 体验核心包含：

- `deriveState()`：从项目状态推导下一 phase / active unit
- `resolveDispatch()`：把 phase 映射成具体 dispatch 行为
- `unit prompt builder`：为当前 unit 注入聚焦上下文
- `post-unit processing`：验证、产物检查、commit、状态推进
- `auto loop supervision`：lock、timeout、stuck detection、recovery
- `git/worktree lifecycle`
- `metrics / budget / doctor / forensics`

### 三. 它和 AgentDash workflow/lifecycle 不是同一层

GSD 的 workflow 更接近：

- 项目级 orchestrator
- 项目状态机
- 自动 dispatch 规则表

AgentDash 当前 workflow/lifecycle 更接近：

- session 级运行时治理
- active step 合约
- tool / stop / context / completion gate

这两者可以组合，但不能简单视为“同一能力已经存在”。

## 语义映射候选

### 方案 A: `Project ≈ Milestone`, `Story ≈ Slice`, `Task ≈ Task`

优点：

- 尽量不新增实体
- 可以直接复用现有 `project / story / task`
- story 天然比 task 更接近一个 demoable capability

问题：

- `project` 在 AgentDash 里更像整个协作空间，而不是一个可完成、可归档、可排队的 milestone
- 若把 `project` 强行当 milestone，会让“多个 milestone 排队 / park / queue / complete milestone”这类体验变得别扭

初步判断：

- 不适合作为完整 GSD 体验的一比一映射

### 方案 B: `Story ≈ Milestone`, `Task ≈ Slice/Task` 混合承载

优点：

- 可以尽量复用 Story owner session
- Story 已经是较大的协作单元

问题：

- Story 当前更像 feature / slice，而不是包含多个 slice 的 milestone
- Task 当前已经是执行单元，再让它兼任 slice 或 task 会造成语义折叠

初步判断：

- 只能支持“简化版 GSD”，不适合完整复刻 README 那种多 phase milestone 编排

### 方案 C: 保留 `Project / Story / Task`，新增一层 orchestration 概念

可选做法：

- 新增 `Milestone / Initiative / WorkflowProgram` 之类的新 planning 层
- 或先引入“纯 orchestrator projection”，先不急着稳定成独立业务实体

优点：

- 语义最干净
- 更接近 GSD 的 `milestone / slice / task`
- 不会污染现有 story/task 的业务含义

问题：

- 需要额外设计新的状态投影与 API
- 落地成本高于纯映射

当前倾向：

- 如果目标是“尽量真实复刻 GSD 使用体验”，最终大概率需要这一方向
- 但第一阶段可以先做“orchestrator projection”，不急着一开始就把新业务实体全面产品化

## 需要补齐的基础设施

### 一. Orchestrator State

需要一个正式的项目级状态视图，至少要能表达：

- active scope
- active phase
- active unit
- blockers
- next action
- completion / validation readiness

这层相当于 AgentDash 版 `deriveState()`。

### 二. Dispatch Engine

需要一套“phase -> action”的统一规则表，输出至少包括：

- dispatch unit type
- target owner / session role
- prompt recipe
- expected artifacts
- post-run policy

这层相当于 AgentDash 版 `resolveDispatch()`。

### 三. Unit Contract

需要定义什么叫一个“可自动推进的 unit”：

- unit 类型
- 所需上下文
- 使用哪个 owner session
- 预期产物
- 通过条件
- 是否可重试 / 如何恢复

### 四. Post-Unit Closeout

这是目前最明显的能力缺口之一，需要统一处理：

- verification commands
- artifact existence / structure check
- step / state advancement
- session summary / report artifact
- git commit / merge / sync

### 五. Git / Isolation Lifecycle

如果目标真的是 GSD 体验，需要把以下能力产品化，而不只是保留在辅助脚本层：

- worktree / branch isolation
- unit 或 milestone 粒度的 checkout / resume
- closeout merge
- teardown / sync-back

### 六. Recovery / Safety

完整自动流需要：

- unit-level lock
- crash-safe resume
- stuck loop detection
- timeout supervision
- recovery briefing

### 七. Observability

若没有观测面，auto workflow 很难调：

- metrics / cost / token
- state timeline
- doctor / diagnostics
- forensics / replay

## 分层落地建议

### 第 0 层: 先做规划与语义冻结

目标：

- 确认最终想复刻的是 GSD 哪一部分体验
- 确认 AgentDash 里的 owner / session / workflow 各自承担什么职责
- 决定映射方案是否需要新增 orchestration 层

当前任务正处于这一层。

### 第 1 层: 做“只读 orchestrator projection”

先不做自动跑，只做：

- 从现有 project / story / task + workflow run 推导一个统一状态视图
- 给出 next action preview
- 让人能看到“如果按 GSD 跑，下一步会 dispatch 什么”

这是最小风险验证。

### 第 2 层: 做 step mode

在 projection 基础上做：

- 手动触发 next unit
- 每次只 dispatch 一个 unit
- 跑完后显式 closeout

这一步可以验证：

- unit contract 是否合理
- story / task owner session 是否足以承载不同类型工作

### 第 3 层: 做 auto mode 最小闭环

在 step mode 稳定后，再引入：

- automatic dispatch loop
- verification
- retry / resume

建议先限定在单一 scope，例如只支持“一个 story 的自动推进”。

### 第 4 层: 做完整 GSD 风格体验

在最小 auto loop 成熟后，再补：

- queue / park / rethink
- worktree isolation
- metrics / budget
- doctor / forensics
- parallel / subagent orchestration

## 当前最值得优先验证的问题

### 1. 最终的 milestone 语义放哪一层

这是最重要的问题。若这一层不定，后面所有 `deriveState()` 和 dispatch engine 都会晃。

### 2. 哪些 unit 应该跑在 story owner session，哪些应该跑在 task session

GSD 的很多 phase 并不等价于“写代码 task”。例如：

- discuss / research / plan
- validate milestone
- complete milestone

这些更像 story 级或更高层的 owner 行为。

### 3. workflow/lifecycle 是承载 unit 内部阶段，还是承载项目级 phase

当前倾向：

- session lifecycle 继续负责 unit 内部治理
- 项目级 orchestrator phase 另起一层，不直接塞进现有 hook runtime

## 当前阶段的非目标

- 不开始实现新的 orchestrator
- 不急于新增 milestone 实体
- 不在本任务里直接改写现有 workflow/lifecycle 合同
- 不为了“先跑起来”而把 `project / story / task` 强行做不自然映射

## 下一步建议

下一轮继续规划时，建议按以下顺序推进：

1. 明确“GSD 体验复刻”的目标边界
2. 选定第一阶段语义映射方案
3. 设计 AgentDash 版 orchestrator state shape
4. 设计 first slice：只读 projection + next action preview
5. 再评估 step mode 和 auto mode 的最小闭环

## 参考

- `references/GSD-2/README.md`
- `.trellis/workflow.md`
- `.trellis/spec/backend/execution-hook-runtime.md`
- `.trellis/spec/backend/address-space-access.md`
- `crates/agentdash-application/src/session/hub.rs`
- `crates/agentdash-application/src/session/hook_runtime.rs`
- `crates/agentdash-application/src/session_plan.rs`
- `crates/agentdash-application/src/task/session_runtime_inputs.rs`
- `crates/agentdash-application/src/workflow/projection.rs`
