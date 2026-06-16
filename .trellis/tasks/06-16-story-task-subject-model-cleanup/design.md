# Story / Task Subject Model Cleanup Design

## 总体模型

本任务将 Story 与 Task 放回 AgentRun / LifecycleRun 的当前事实源之下：

- Story 是 Project 下的 subject / context container，用于可操作流程、主题管理和上下文注入。
- Story-bound AgentRun 是普通 AgentRun / LifecycleRun 携带 `SubjectRef(kind = story)` 后的运行形态。
- Task 是 LifecycleRun aggregate 内的计划项事实，由 AgentRun workspace 创建、推进、派发和归档。
- Story 看到的 Task 是 projection，不是 Story domain 私有实体。

这组边界让 Story 保留人和 Agent 都能管理的主题价值，同时让 Task 贴近实际工作中的局部计划，查询也围绕 run 局部聚合。

## Story

Story 继续作为 Project 下的一等业务主题。它负责表达：

1. 工作主题与目标。
2. 用户或 Agent 可操作的状态流转。
3. 面向 AgentRun 的上下文材料。
4. Story scope capability 的授权边界。

Story 不定义独立 runtime。进入 Story 入口时，系统创建普通 AgentRun / LifecycleRun，并在 run subject、frame construction block 与 capability 上体现 Story 语义。

Story 的上下文职责包括 title、description、priority、type、tags、default workspace、context source refs、context containers、disabled inherited containers、session composition 和 Story 视角 Task projection。Story 不持久化 Task domain facts，也不承担 Task durable CRUD。

Story 状态表达产品流程和工作判断。LifecycleRun 的失败、取消、完成可以成为 UI 提示或投影信号，但 Story 状态推进应通过明确 Story command 发生。用户可以在 Story 页面里对已列好的事项做快速一键启动或人工确认；Agent 也可以在具备 Story scope capability 且命令来源可解释时推进 Story 状态。

## Story-bound AgentRun

Story-bound AgentRun 的差异集中在初始化阶段：

1. `SubjectRef(kind = story, id = story_id)` 进入 run metadata。
2. Frame construction 注入 Story blocks，例如 Story summary、Story constraints、已有结论、相关 Task projection。
3. Capability grants 中包含 Story scope 权限，例如更新 Story 状态、追加 Story context、查询 Story Task projection，以及在 Story-bound AgentRun workspace 中创建 run-owned Task。

具体链路应复用 `SubjectContextAssignmentResolver`、`CapabilityScopeCtx::Story` 和 ProjectAgent launch config。Story 入口只是在这些统一机制上追加 Story subject 语义，不形成独立 StoryAgent entity、repository、runtime 或 session。

运行时仍遵循统一 AgentRun / LifecycleRun 机制。这样 StoryAgent 可以作为入口概念存在，但不扩大为独立实体系统。

## Task

Task 的第一性定位是 LifecycleRun 内的计划项事实，而不是执行进程、workflow node 或 Project 级工作单。

Task 需要保留 durable identity 与计划项能力：title、body、plan status、priority、creator、owner、assignment、source task、context refs、Story projection hint 和 archive marker。Task 可以显式 reference Story、Routine、External source 或 AgentRun；Project 只作为上层作用域和权限上下文，不推出 Project 全局任务队列。

Task 物理模型已决策为 `LifecycleRun` aggregate 内的结构化 `tasks` 字段。PostgreSQL 使用 JSON 文本列持久化，列名表达业务语义，不使用 `_json` 后缀；repository 对 `LifecycleRun.tasks` 做 create / update / select 整体 roundtrip，并在坏 JSON 时给出 `lifecycle_runs.tasks` 错误上下文。

Task value object 最小字段：

- `id`
- `title`
- `body`
- `status`
- `priority`
- `created_by_agent_id`
- `owner_agent_id`
- `assigned_agent_id`
- `source_task_id`
- `created_at`
- `updated_at`
- `archived_at`
- 可选 `context_refs`
- 可选 `story_ref`

Task 集合边界由 owning `LifecycleRun` 给出，因此 value object 不重复保存 root run id。`created_by_agent_id` 表达创建者，`owner_agent_id` 表达当前主要管理者，`assigned_agent_id` 表达计划层派发关系。真正的执行状态、执行器运行结果和产物需要通过 association / linked run / lifecycle trace 查询。

实现阶段需要保证 `StoryRepository` 不承担 Task durable CRUD。Task 操作通过 LifecycleRun aggregate mutation 或 Lifecycle application command 完成，Story 只通过 Story-bound run、linked run projection 或显式 `story_ref` 获得可解释视图。

## 存储与查询边界

Task 查询优先围绕 LifecycleRun aggregate 展开：

1. AgentRun workspace 查询当前 run 的 `LifecycleRun.tasks`，并按 created / owner / assigned agent 关系筛选。
2. LifecycleRun 查询本 aggregate 内的 Task 集合与概要。
3. Story 查询 Story-bound LifecycleRun 与 linked run 推导出的 Task projection。
4. 显式 `story_ref` 只用于手动跨 run 关联，不表达所有权。

Project 级 Task visibility 当前没有业务需求。Project 视角如需展示，应先来自明确入口的聚合视图，而不是引入常驻全局 Task 队列。

这种局部性符合 Task 作为 Agent 工作计划的使用方式，也能避免 Story / Project 页面为了获取 Task 状态持续扫描所有运行事实。

## 状态机

Task 状态使用计划语言：

```text
open -> active -> review -> done
open -> active -> done
open -> dropped
active -> blocked
blocked -> active
review -> active
review -> done
```

状态枚举：

- `open`：已创建，等待处理。
- `active`：正在被某个 AgentRun 或人处理。
- `review`：等待人、父 AgentRun 或具备 owner capability 的 Agent 确认。
- `blocked`：外部条件未满足。
- `done`：完成。
- `dropped`：从当前计划软归档。

状态转换只描述 Task 计划进度。`review` 不是必经节点；自执行或 owner 直接推进的 Task 可以从 `active` 直接到 `done`。是否启动了 subagent、subagent 是否仍在运行、workflow 是否重试，都由 LifecycleRun / Runtime trace 表达。

不同 assignment mode 可以附带不同推进策略：

- `self-managed`：当前 AgentRun 或 owner AgentRun 可以按计划直接推进到 `done`。
- `human-owned`：人类可以直接推进，也可以把 Task 放入 `review` 等待确认。
- `assigned-to-subagent`：subagent 可以推进执行结果并提交 `review`，但不能自行把 Task 推过 owner 确认边界；`done` 由父 AgentRun、人类或具备 owner capability 的 Agent 确认。
- `workflow-fanout`：workflow runtime 负责运行事实，Task 状态可由 workflow policy 决定是否需要 review gate。

## Assignment

Task 派发是一条计划层关联关系。单个 Task 指派给 Companion subagent、Story-bound AgentRun workspace 创建 Task 后派发、dynamic workflow 批量 fanout，都应使用同一套关联链路：

```text
LifecycleRun.tasks[] Task -> assigned agent / linked AgentRun -> runtime trace
```

Task facts 只记录“这个计划项当前交给谁处理”、是否需要 owner review gate 以及必要的来源关联。审计、重试、批量派发策略属于 orchestration / workflow runtime 层。

执行事实通过 `LifecycleSubjectAssociation`、Agent lineage 和 `RuntimeSessionExecutionAnchor` 投影。短期可以继续使用 `SubjectRef(kind = task)` 表达 Task subject，并通过 association 展示 execution attempt、review run、follow-up run；Task facts 自身不保存 execution status 或 artifacts。

## Dynamic Workflow Fanout

Dynamic workflow 可以从 Task 集合中读取输入，例如：

1. 选中 root LifecycleRun 内的若干 Task。
2. 根据 workflow plan 过滤、分组或映射为 node inputs。
3. 创建对应 subagent run。
4. 用统一 assignment link 将 Task 与执行 run 关联起来。

Task 只作为 fanout 数据源。依赖关系、批次、并发、失败恢复和审计日志由 workflow runtime 维护。

应用层命令边界仍应清楚：

- `create_tasks_from_plan`：根据计划持久化 Task 集合。
- `assign_tasks`：为单个或多个 Task 建立计划层派发关系。
- `fanout_tasks`：由 workflow / collaboration 层基于已选 Task 创建 child AgentRun 或 workflow node dispatch。

UI 可以把这些动作合并成一键操作，但命令边界需要能解释计划创建、派发关系和 runtime dispatch 分别发生了什么。

## Story Projection

Story Task projection 的第一版规则：

1. Story-bound LifecycleRun 内 Task 默认可见。
2. 与 Story-bound run 关联的 linked run / child run Task 可见。
3. 显式 `story_ref` 的 Task 可见，但 `story_ref` 只用于手动跨 run 关联，不表达所有权。
4. 派发给其它 AgentRun 的 Task，如果保留 Story-bound run 来源或显式 `story_ref`，也可回投 Story。

投影需要能让 Story 页面解释“为什么这个 Task 出现在这里”，也让后续权限策略更容易落地。

## 注入 Block

Story / Task 注入作为 Frame construction 的可组合 Block：

- `StorySubjectBlock`
- `StoryContextBlock`
- `StoryTaskProjectionBlock`
- `SelectedTaskBlock`
- `RunTreeTaskSummaryBlock`

第一版只需要保证启动流程能选择并组合这些 block。Block 内容的压缩、排序、截断和权限过滤可以随 AgentFrame 构造策略逐步增强。

## Read Model

Story / Task execution 视图应统一向 subject execution view 收口：

- Story 页面展示 `SubjectRef(kind = story)` 的 linked runs。
- Task 页面或抽屉展示 `SubjectRef(kind = task)` 的 linked runs。
- Task latest execution summary 可以作为 read projection 生成，但来源需要追溯到 association / run / agent / frame / runtime node。

执行视图统一使用 subject execution view，例如 `/subjects/task/{id}/execution`。Task plan API 不返回 execution status、latest runtime node 或 artifacts；这些事实由 `SubjectExecutionView`、linked runs 和 Lifecycle projection 承担。

## MCP / Capability

Story / Task 工具能力从 subject scope 派生：

- Story subject frame 获得 Story 投影查询、Story context 更新和 Story 状态推进能力。
- Task subject frame 获得 Task 上下文、Task 状态推进和 linked run 查看能力。
- 任意 AgentRun 可以通过 Task management capability 创建和管理 Task。
- Collaboration capability 提供单个 Task assign 给 Companion subagent。
- Workflow / orchestration capability 提供 Task selector、计划生成和 fanout。

状态推进、artifact 上报和 dispatch 入口需要对应明确事实源：Story 流程状态走 Story command，Task 计划状态走 Task command，runtime artifacts / status 走 Lifecycle / AgentRun projection，subagent dispatch 走 AgentRun / Lifecycle launch command 并写 subject association。执行器选择进入 assignment / launch hint，不进入 Task facts。

## UI 入口

第一版 UI 入口优先级：

1. AgentRun workspace 的 Task plan 面板。
2. Story 页面中的 Task projection 区域。
3. Companion / subagent 派发入口。
4. Dynamic workflow fanout 选择器。

AgentRun workspace 是最贴近 Task 一等模型的入口。Story 页面第一版只展示与 Story 关联的投影视图，不直接创建 Task。

具体收口方向：

- Story 页面保留 Story brief、状态、context、Story Task projection、linked AgentRuns / Lifecycle runs。
- TaskDrawer 保留 Task 命名，聚焦计划项编辑、状态推进和 linked runs 查看。
- Task execution panel 使用 SubjectExecutionView 或 linked runs projection。
- StoryBoard / bulk / quick jump 等 PM 产品化表面按当前目标重新评估，优先服务 Story subject run 和 Task plan。

## 删除与归档

Task 使用软归档语义。`dropped` 表达从当前计划移除，`archived_at` 表达从默认视图隐藏。

软归档能保留 linked run、Story projection 和 workflow trace 的解释链路，也便于后续审计和复盘。

## Permission 接入点

Task fanout 与 Story scope capability 保留 permission / approval 接入点。默认策略开放，具体收束由 permission system convergence review 任务处理。

需要预留的检查点：

1. AgentRun 是否可创建 Task。
2. AgentRun 是否可修改当前 Task。
3. AgentRun 是否可将 Task 派发给指定 Companion / subagent。
4. Subagent 是否只能提交 review，而不能自行确认 owner-owned Task 为 done。
5. Story-bound AgentRun 是否可读取或更新 Story projection。

## 数据迁移考虑

预研期可以直接做正确迁移：

- 新增 migration 初始化 `lifecycle_runs.tasks`，默认空数组文本。
- `stories.tasks` 与旧 Task child 字段退出主线 schema；当前没有存量 Task 数据，不做复杂 backfill 或占位 LifecycleRun。
- Task status enum 收敛为 `open / active / review / blocked / done / dropped`。
- Task artifacts 不进入 Task facts，改由 execution projection 或 linked artifacts 表达。
- Task dispatch preference 拆分为 assignment / launch hint 或 dispatch command 参数。
- Story projection 由 Story-bound run、linked run 和可选 `story_ref` 解释 Story 与 Task 的关系。

迁移后的 repository 主线只读写 `LifecycleRun.tasks`，不再从 Story aggregate 读写 Task facts。

## 已决策与实现细化

1. Task 物理仓储形态已决策为 `LifecycleRun.tasks` aggregate 字段。
2. Assignment link 的实现需要在 Task facts、`LifecycleSubjectAssociation`、Agent lineage 与 runtime anchor 间保持单一解释链。
3. Frame construction Block 的接口形态与裁剪策略需要围绕 Story projection、selected Task 和 run Task summary 落地。
4. Story projection 的权限过滤和 UI 展示策略需要能解释每个 Task 的来源关系。
5. Dynamic workflow 从 Task 集合读取数据时需要 selector DSL，但 selector 只读 Task plan facts。
6. SubjectExecutionView 对 Story / Task linked runs 的统一 DTO 形态继续作为执行投影事实源。
