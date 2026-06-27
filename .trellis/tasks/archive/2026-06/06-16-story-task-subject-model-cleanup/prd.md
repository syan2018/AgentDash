# Story / Task Subject Model Cleanup PRD

## 背景

Story 与 Task 需要回到当前 AgentRun / LifecycleRun 建模下重新收束。Story 仍然是 Project 下有人工状态流转价值的工作主题；Task 作为 LifecycleRun 内的计划项事实，由 AgentRun 在工作过程中创建、推进、派发和归档，不承担 runtime execution truth。

这次整理的目标不是扩展 StoryAgent 成新的执行体系，而是让 Story、Task、AgentRun、LifecycleRun 的关系更清楚：Story 提供主题、上下文与人工流程入口；Task 提供局部计划、待办与派发数据；真正的执行事实仍然来自 LifecycleRun 控制树、AgentFrame、RuntimeNodeState 与 RuntimeSession trace。

本任务需要从方向规划修订为可交给实现 agent 的迁移包。项目仍处于预研期，目标是直接收束到正确模型：不保留旧字段兼容、不做旧 endpoint fallback、不做双读或旧状态兼容层。

## 目标

1. 明确 Story 是 subject / context 容器，而不是特殊 runtime 类型。
2. 明确 StoryAgent 只是 subject to story 的 AgentRun / LifecycleRun，差异来自 Story 入口注入块与 Story scope capability。
3. 将 Task 定位为 LifecycleRun 控制树内的计划项事实，由 AgentRun workspace 创建、推进和派发。
4. 让 Story 通过 Story-bound LifecycleRun 与 linked run 投影视角看到 Task，而不是拥有 Task domain。
5. 支持 Task 作为单个计划项、Companion subagent 派发项、dynamic workflow fanout 数据源三类入口。
6. 保留审批 / permission 接入点，默认开放，由后续 permission 收束任务统一维护。
7. 将任务文档、长期 spec 与 manifest 同步修到同一目标模型，确保实现阶段只以 LifecycleRun Task facts 与 Story projection 为事实源。

## 需求细节

1. Story 保留 title、description、priority、type、tags、default workspace、context source refs、context containers、disabled inherited containers、session composition 和 Story Task projection 等上下文职责。
2. Story 入口继续复用 subject context 解析主链。Story context injection blocks、`CapabilityScopeCtx::Story` 默认 capability 与 ProjectAgent launch config 是 Story-bound AgentRun 的特殊能力来源。
3. Task 命名继续保留。产品语义是 Task，底层职责是 LifecycleRun 内的计划项事实。
4. 任意 AgentRun workspace 都可以在其 LifecycleRun 聚合内创建、维护、关闭 Task；Task 可作为计划项、检查项、subagent 派发目标和 linked runs 投影入口。
5. Task 与 AgentRun / subagent 的连接通过 Task facts 中的计划层关联、subject association、Agent lineage 和 runtime anchor 投影表达。AgentRun 可作为 Task 创建者、管理者或执行关联方，但 Task 不表达 runtime running。
6. Task assignment 是计划层关系。assign / fanout 创建执行意图、子 AgentRun / Lifecycle association 和可观察 linked run，运行事实从 Lifecycle projection 派生。
7. Task 的 `review` 状态不是必经状态。直接自执行或 owner 自己推进的 Task 可以从 `active` 直接进入 `done`；指派给 subagent 的 Task 可以由 assignment policy 要求进入 `review`，并由父 AgentRun、人类或具备 owner capability 的 Agent 确认完成。
8. Task fanout 保留 review / approval 门，默认直接放行；后续可由 workflow、project policy 或 permission grant 切换为需要审批。
9. Story 状态可以由用户或 Agent 通过明确 Story command 推进。用户推进更常见于 Story 页面中已列好事项后的快速一键启动 / 人工确认语境；Agent 推进需要可解释的命令来源与 Story scope capability。
10. 现有重复事实源需要收口：Task execution DTO、Task runtime status、Task artifacts、Task dispatch preference、Story / Task MCP 独立状态推进工具、前端 Task execution panel。
11. Story 页面第一版只展示由 Story-bound run 推导出的 Task projection，不直接创建 Task；Task 创建、推进、归档和 assignment 的第一入口是 AgentRun workspace。
12. 后续实现可拆成 LifecycleRun aggregate 字段迁移、Run-scoped API 收口、前端投影拆分、MCP / capability 收口、dynamic workflow 数据源等子任务。

## 非目标

1. 不把 StoryAgent 建模为独立 Agent 类型、仓储类型或 runtime session 类型。
2. 不把 Task 设计成 Project 全局工作队列；当前没有 Project-wide Task visibility 需求。
3. 不让 Task 承担 workflow 调度、批量审计、重试或运行状态事实。
4. 不在本任务内重做 permission / approval 系统，只保留未来接入位置。
5. 不保留旧 Task 状态、旧全局 `/tasks/{id}` 心智、旧 Task artifacts 或 `dispatch_preference` 字段兼容。

## 业务模型

### Story

Story 是 Project 下的工作主题，承载可操作的状态流转、描述、上下文材料和入口约束。Story 可以启动普通 AgentRun / LifecycleRun，这些 run 会携带 `SubjectRef(kind = story)`，并在初始化时获得 Story 相关注入块。

Story 的核心价值是让人类能以主题维度理解和管理工作，而不是替代 AgentRun / LifecycleRun 的执行模型。

### Story-bound AgentRun

Story-bound AgentRun 与普通 AgentRun 的执行机制一致。它的特殊性来自：

1. run subject 指向 Story。
2. 初始化时带入 Story 注入 Block。
3. 可获得 Story scope capability，例如读取 / 更新 Story 上下文、产出 Story 相关结论、查询 Story Task projection，以及在 Story-bound AgentRun workspace 中创建 run-owned Task。

### Task

Task 是 AgentRun / LifecycleRun 内部的计划项事实。它可以由当前 Agent 自己完成，也可以派发给其它 AgentRun 或 Companion subagent；在 dynamic orchestration 场景中，也可以作为一组 workflow 输入数据被扇出。

Task facts 跟随 LifecycleRun aggregate，而不是 Project 全局任务池。这样能让查询围绕正在工作的 run 聚合，避免为了 Story 或 Project 视角提前建立高频全局查询路径。

## 状态语言

Task 使用计划语言表达进度：

- `open`：已创建，尚未开始处理。
- `active`：当前正在处理。
- `review`：等待人、父 AgentRun 或具备 owner capability 的 Agent 确认。
- `blocked`：被外部条件阻塞。
- `done`：已完成。
- `dropped`：已软归档，不再参与当前计划。

这些状态描述 Task 的计划进度，不表达 runtime 是否真正运行、是否有进程存活、是否有 frame 正在执行。`review` 是可选验收状态，不是所有 Task 的必经节点；是否需要进入 `review` 由 assignment mode、owner policy 或 workflow policy 决定。执行事实仍以 LifecycleRun / Runtime trace 为准。

## Projection

Story 视角下的 Task 来自 Story-bound LifecycleRun 与 linked run projection：

1. Story-bound LifecycleRun 内的 Task 默认可见。
2. 与该 Story-bound run 关联的 linked run / child run 产生的 Task 可见。
3. 显式 `story_ref` 只用于手动跨 run 关联，不表达 Story 对 Task 的所有权。

第一版不做 Project/global Task visibility，也不提供 Project 级 Task 池查询。标签、文本上下文或自动猜测可以后续作为搜索增强，而不是业务归属来源。

## 派发与编排

Task 派发使用统一关联链路表达，无论是单个 Task assign 给 Companion subagent，还是 dynamic workflow 根据一组 Task fanout，都应落在同一套 run / task / assignment 关系上。

Dynamic workflow 中 Task 只作为数据源。依赖、批次、重试、审计与扇出执行策略属于 workflow runtime / orchestration 层；Task 不维护这些机制，只提供可被选择、过滤和注入的计划项事实。

## 注入块

Story 与 Task 相关上下文通过 Agent 启动 / Frame construction 阶段的可自定义 Block 注入。第一版只需要把模块边界抽出来，让不同入口能组合 Story summary、Story constraints、当前 Task 列表、选中 Task 等上下文。

## 验收标准

1. 文档能清楚说明 Story、Story-bound AgentRun、Task、LifecycleRun 的边界。
2. Task 被描述为 LifecycleRun 控制树内的计划项事实。
3. Story projection 只依赖 Story-bound run、linked run 与可解释的 `story_ref`，而不要求 Project 全局 Task 队列。
4. Task 状态集合收敛为计划语言，并与 runtime execution truth 解耦。
5. 派发和 fanout 的关系链路统一，Task 不承载 workflow 调度机制。
6. Permission / approval 只保留接入点，后续由独立任务收束。
7. 后续实现前可以基于本任务拆分为 LifecycleRun aggregate 字段迁移、API/read model、UI、MCP/capability、workflow fanout 等子任务。
8. 任务包明确无兼容字段、无 fallback、无旧 API 保留；当前没有存量 Task 数据，migration 不做复杂 backfill。

## 关联背景

现有 `06-14-module-overdesign-review` 已覆盖 Task projection / execution read model 的事实链问题；本任务承接更上层的产品建模收束。

Permission / approval 系统已有独立收束任务。Task fanout 在本任务内只保留接入点，不把 Task 模型清理阻塞在 permission 系统重构上。
