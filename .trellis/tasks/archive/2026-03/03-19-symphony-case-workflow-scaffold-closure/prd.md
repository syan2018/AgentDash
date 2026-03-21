# 以 Symphony case 校验通用工作流脚手架完备性

## Goal

以 Symphony 作为“能力校验样例”而不是目标产品，审视 AgentDash 作为可集成任意工作流的 Agent 脚手架，当前还缺少哪些通用基础能力、状态抽象、运行时契约与 orchestration 控制面。

本任务的目标不是复刻 Symphony，也不是实现 Linear 集成，而是回答一个更底层的问题：

- 如果未来要支持“某个工作流以长期守护进程方式自动拉起、持续推进、可恢复、可观测”，AgentDash 现在的框架基础是否足够？
- 如果不够，缺的是哪一层抽象，而不是哪一个外部系统的适配器？

## Non-Goals

- 不以“对齐 Symphony SPEC”作为逐条 checklist
- 不讨论 Linear、GitHub、Jira 等外部系统接入细节
- 不把 Symphony 的单一 issue loop 当成 AgentDash 的最终产品形态
- 不为了兼容某个参考实现而牺牲 AgentDash 既有的 Story / Task / 多 mount / 多 backend 方向

## Background

Symphony 的价值不在于它接了 Linear，而在于它把一类长期自动化 workflow 拆成了几层清晰能力：

1. 仓库内工作流契约
2. 后台 poll / dispatch / reconcile / retry 的 orchestrator
3. 可重复推导的 workspace 生命周期
4. 单一权威的运行态与可观测面
5. agent runner 与会话恢复

这些能力是否完整，正好可以拿来当 AgentDash 的一块“压力测试石”。

AgentDash 目前已经具备不少强于 Symphony 的底座：

- Story / Task 分层
- SessionBinding
- MCP 注入
- 多 mount Address Space
- 云端 / 本机 relay
- 本地第三方 Agent 与云端原生 Agent 并存

但这些更多是“执行平台能力”。是否已经足够支撑“任意 workflow 以守护进程方式长期运行”，还没有得到验证。

## Core Question

本任务要持续收敛的问题只有一个：

> 若把“Symphony 这种长期自动化流程”视为一个 workflow case，而不是一个产品模板，AgentDash 还缺哪些通用框架能力，才能自信地说自己是“可集成任意工作流的 Agent 脚手架”？

## Working Assumptions

### 1. AgentDash 的核心定位

AgentDash 不是某个固定 workflow 的实现，而是：

- 一个统一的 Agent 执行与编排脚手架
- 一个可容纳不同 workflow contract 的 runtime
- 一个可把 Story / Task / Session / Workspace / Mount / Validation 组合起来的基础平台

### 2. Symphony 在这里的角色

Symphony 只承担两个作用：

- 暴露“长期自动化 workflow”需要哪些底层能力
- 帮我们发现 AgentDash 的框架缺口

它不承担以下作用：

- 不定义 AgentDash 的最终 workflow DSL
- 不定义 AgentDash 的 issue model
- 不定义 AgentDash 的外部系统边界

### 3. 讨论优先级

优先级从高到低应为：

1. 通用 orchestration 抽象是否闭环
2. 通用 runtime contract 是否稳定
3. 通用状态与恢复语义是否完整
4. 具体 tracker / repo / external service 如何接入

## Current Baseline

结合当前代码，AgentDash 已有的能力可大致归类为：

### 已经相对完整的部分

- Task 执行 start / continue / cancel 与 turn monitor
- SessionBinding 与 session 流
- relay + 本机 ToolExecutor + runtime tools
- Address Space / mount / context container 雏形
- Project / Story 级 session composition

### 仍偏“局部能力”而非“workflow 控制面”的部分

- 自动重试主要停留在 Task turn 级别
- Story owner session 语义还未真正成立
- Workspace 生命周期仍接近 CRUD，而非 managed workspace
- 缺少单一 authoritative automation state
- 缺少 workflow contract loader / validator / reloader
- 缺少 operator 视角的 automation observability

## What We Actually Need To Validate

不看 Symphony 特例，真正需要检验的是以下六类通用能力。

### 1. Workflow Contract Layer

我们需要一种“工作流契约”来描述：

- agent persona / prompt policy
- session required context
- workspace preparation policy
- run / retry / timeout / stall policy
- orchestration strategy hint
- 可选的 hooks / guards / validation rules

它未必叫 `WORKFLOW.md`，也未必长得像 Symphony，但需要承担同类职责。

### 2. Automation Control Plane

我们需要一个独立于单 Task 执行器的长期运行控制面，负责：

- dispatch
- claim / release
- retry queue
- stall detection
- reconciliation
- runtime snapshot

否则当前系统只是在“被动接受 start/continue API”，还不算自动化 workflow runtime。

### 3. Run Attempt Model

当前 `Task.session_id` 不足以表达：

- 第几次自动尝试
- 为什么进入 retry
- 何时 due
- 由哪个 workflow contract 触发
- 是正常 continuation 还是失败重试

因此需要独立的 run / attempt 抽象。

### 4. Managed Workspace Lifecycle

当前 Workspace 更像元数据注册。要支撑长期自动化，必须明确：

- workspace 如何被 deterministic 地选取或派生
- 谁负责 prepare / reuse / cleanup
- workspace 与 Story owner / Task worker 的绑定关系
- hooks 在哪个阶段执行

### 5. Owner Session / Worker Session 分层

AgentDash 已经天然适合比 Symphony 更强的模型：

- Story owner session 负责持续推进 workflow
- Task worker session 负责具体执行切片

但当前代码里 Story session 还更像 companion session。这个分层如果不补齐，后续很多自动化逻辑都会被迫回落到 API 层或单 Task 层。

### 6. Automation Observability

需要有一份面向 operator 的视图，而不只是 session stream：

- 哪些 workflow run 正在运行
- 哪些在 retry queue
- 谁持有 claim
- 最近失败原因
- 最近一次 reconcile 结果
- token / time / resource 使用情况

## Likely Missing Abstractions

基于当前仓库状态，优先怀疑缺失的抽象包括：

- `WorkflowDefinition`
- `WorkflowRuntimeConfig`
- `AutomationRun`
- `AutomationAttempt`
- `AutomationState`
- `AutomationManager` / `WorkflowOrchestrator`
- `WorkspaceLifecycleManager`
- `StoryOwnerExecutionGateway`
- `AutomationSnapshot`

这些名称不一定最终采用，但对应职责需要有落点。

## Key Design Principles

### 原则 1：不要把外部连接器误当成框架能力

Linear / GitHub / Jira 适配器属于 integration layer，不应主导核心抽象设计。

### 原则 2：不要把 Symphony 的单 issue loop 误当成唯一 workflow 模型

AgentDash 的 Story / Task 分层天然支持比 Symphony 更丰富的 workflow 结构，应保留这种上限。

### 原则 3：把“长期自动化运行时”视为独立模块

它不应只是现有 Task API 的后台调用脚本，而应是单独的 control plane。

### 原则 4：通用 contract 优先于具体 DSL

先明确 contract 需要表达什么，再决定它是 YAML front matter、数据库配置，还是别的格式。

### 原则 5：执行底座和自动化控制面要分层

当前 AgentDash 的执行底座已经不错。后续重点不是重写执行，而是补一个能长期调度和恢复的上层。

## Discussion Outputs We Want

本任务后续讨论希望持续产出以下内容：

1. 一版“通用 workflow scaffold 能力地图”
2. 一版“执行底座 vs 自动化控制面”的边界定义
3. 一版最小可行 automation runtime 的数据模型
4. 一版 Story owner / Task worker 的职责分层
5. 一版 managed workspace 生命周期模型
6. 一版 runtime snapshot / observability 草案
7. 最终拆出若干实施任务，而不是停留在泛泛讨论

## Acceptance Criteria

- [ ] 明确 Symphony 在本任务中只是 capability case，不是产品模板
- [ ] 明确 AgentDash 作为通用 workflow scaffold 的目标边界
- [ ] 列出当前“已有底座能力”和“缺失的控制面能力”
- [ ] 明确至少一版建议补充的核心抽象集合
- [ ] 明确 Story owner / Task worker / Workspace lifecycle 的推荐分层
- [ ] 能拆分出后续实施任务，而不是继续把问题停留在口头讨论

## Follow-up Task Directions

后续大概率会拆成这几类子任务：

1. `workflow-contract-layer-design`
2. `automation-control-plane-state-model`
3. `story-owner-session-runtime`
4. `managed-workspace-lifecycle`
5. `automation-observability-snapshot`
6. `execution-runtime-vs-automation-runtime-boundary-cleanup`
