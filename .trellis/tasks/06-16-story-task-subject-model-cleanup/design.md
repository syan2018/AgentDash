# Story / Task Subject Model Cleanup Design

## 总体模型

本任务将 Story 与 Task 放回 AgentRun / LifecycleRun 的当前事实源之下：

- Story 是 Project 下的 subject / context container，用于可操作流程、主题管理和上下文注入。
- Story-bound AgentRun 是普通 AgentRun / LifecycleRun 携带 `SubjectRef(kind = story)` 后的运行形态。
- Task 是 LifecycleRun 控制树内的 Todo facts / plan item，由 AgentRun 创建、推进、派发和归档。
- Story 看到的 Task 是 projection，不是 Story domain 私有实体。

这组边界让 Story 保留人和 Agent 都能管理的主题价值，同时让 Task 贴近实际执行单元，查询也围绕 run tree 局部聚合。

## Story

Story 继续作为 Project 下的一等业务主题。它负责表达：

1. 工作主题与目标。
2. 用户或 Agent 可操作的状态流转。
3. 面向 AgentRun 的上下文材料。
4. Story scope capability 的授权边界。

Story 不定义独立 runtime。进入 Story 入口时，系统创建普通 AgentRun / LifecycleRun，并在 run subject、frame construction block 与 capability 上体现 Story 语义。

Story 的上下文职责包括 title、description、priority、type、tags、default workspace、context source refs、context containers、disabled inherited containers、session composition 和 Story 视角 Task projection。

Story 状态表达产品流程和工作判断。LifecycleRun 的失败、取消、完成可以成为 UI 提示或投影信号，但 Story 状态推进应通过明确 Story command 发生。用户可以在 Story 页面里对已列好的事项做快速一键启动或人工确认；Agent 也可以在具备 Story scope capability 且命令来源可解释时推进 Story 状态。

## Story-bound AgentRun

Story-bound AgentRun 的差异集中在初始化阶段：

1. `SubjectRef(kind = story, id = story_id)` 进入 run metadata。
2. Frame construction 注入 Story blocks，例如 Story summary、Story constraints、已有结论、相关 Task projection。
3. Capability grants 中包含 Story scope 权限，例如更新 Story 状态、追加 Story context、创建 Story 显式关联 Task。

具体链路应复用 `SubjectContextAssignmentResolver`、`CapabilityScopeCtx::Story` 和 ProjectAgent launch config。Story 入口只是在这些统一机制上追加 Story subject 语义，不形成独立 StoryAgent entity、repository、runtime 或 session。

运行时仍遵循统一 AgentRun / LifecycleRun 机制。这样 StoryAgent 可以作为入口概念存在，但不扩大为独立实体系统。

## Task

Task 的第一性定位是 Todo facts / plan item，而不是执行进程、workflow node 或 Project 级工作单。

Task 需要保留 durable identity 与计划项能力：title、description、plan status、ordering / grouping metadata、creator / source、manager / assignment、context refs、linked runs summary。Task 可以显式 link 到 Story、Routine、External source 或 AgentRun；Project 只作为上层作用域和权限上下文，不推出 Project 全局任务队列。

推荐最小字段语义：

- `id`
- `root_lifecycle_run_id`
- `owner_agent_run_id`
- `created_by_agent_run_id`
- `title`
- `body`
- `status`
- `priority`
- `story_link`
- `assigned_agent_run_id`
- `source_task_id`
- `created_at`
- `updated_at`
- `archived_at`

其中 `root_lifecycle_run_id` 用于表达 Task 集合边界，`owner_agent_run_id` 表达当前主要管理者，`assigned_agent_run_id` 表达计划层派发关系。真正的执行状态需要通过关联 run / lifecycle trace 查询。

当前代码中 Task 与 Story aggregate 的关系需要在实现阶段重点核对。目标是让 Task durable facts 脱离 Story aggregate 生命周期，Story 只通过显式 link、origin refs 或 subject association 获得 projection。

## 存储与查询边界

Task 查询优先围绕 LifecycleRun 控制树展开：

1. AgentRun workspace 查询当前 run 自己创建或管理的 Task。
2. Root LifecycleRun 查询整个控制树内的 Task 集合。
3. Story 查询显式关联 Story 的 run tree Task projection。

Project 级 Task visibility 当前没有业务需求。Project 视角如需展示，应先来自明确入口的聚合视图，而不是引入常驻全局 Task 队列。

这种局部性符合 Task 作为 Agent 自身 Todo 的使用方式，也能避免 Story / Project 页面为了获取 Task 状态持续扫描所有运行事实。

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

状态转换只描述 Todo 计划进度。`review` 不是必经节点；自执行或 owner 直接推进的 Task 可以从 `active` 直接到 `done`。是否启动了 subagent、subagent 是否仍在运行、workflow 是否重试，都由 LifecycleRun / Runtime trace 表达。

不同 assignment mode 可以附带不同推进策略：

- `self-managed`：当前 AgentRun 或 owner AgentRun 可以按计划直接推进到 `done`。
- `human-owned`：人类可以直接推进，也可以把 Task 放入 `review` 等待确认。
- `assigned-to-subagent`：subagent 可以推进执行结果并提交 `review`，但不能自行把 Task 推过 owner 确认边界；`done` 由父 AgentRun、人类或具备 owner capability 的 Agent 确认。
- `workflow-fanout`：workflow runtime 负责运行事实，Task 状态可由 workflow policy 决定是否需要 review gate。

## Assignment

Task 派发是一条计划层关联关系。单个 Task 指派给 Companion subagent、Story 入口创建 Task 后派发、dynamic workflow 批量 fanout，都应使用同一套关联链路：

```text
Task -> assigned AgentRun / LifecycleRun -> runtime trace
```

Task 层只记录“这个 Todo 当前计划交给谁处理”、是否需要 owner review gate 以及必要的来源关联。审计、重试、批量派发策略属于 orchestration / workflow runtime 层。

关联链路可以复用 `LifecycleSubjectAssociation` 或后续专用 execution link。短期可以继续使用 `SubjectRef(kind = task)` 表达 Task subject，并通过 association 展示 execution attempt、review run、follow-up run。

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

1. Story subject run 创建的 Task 可见。
2. 显式 link 到 Story 的 Task 可见。
3. 与 Story subject run 同属一个 root LifecycleRun 控制树的 child run Task 可见。
4. 派发给其它 AgentRun 的 Task，如果保留 Story link 或来源 run link，也可回投 Story。

显式关联优先能让 Story 页面解释“为什么这个 Task 属于这个 Story”，也让后续权限策略更容易落地。

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

`/tasks/{id}/execution` 这类 Task 专属轻量 DTO 后续应复用 subject execution view，或者让位给同一套 linked runs projection。

## MCP / Capability

Story / Task 工具能力从 subject scope 派生：

- Story subject frame 获得 Story 投影查询、Story context 更新和 Story 状态推进能力。
- Task subject frame 获得 Task 上下文、Task 状态推进和 linked run 查看能力。
- 任意 AgentRun 可以通过 Task management capability 创建和管理 Task。
- Collaboration capability 提供单个 Task assign 给 Companion subagent。
- Workflow / orchestration capability 提供 Task selector、计划生成和 fanout。

状态推进、artifact 上报和 dispatch 入口需要对应明确事实源：Story 流程状态走 Story command，Task 计划状态走 Task command，runtime artifacts / status 走 Lifecycle / AgentRun projection，subagent dispatch 走 AgentRun / Lifecycle launch command 并写 subject association。

## UI 入口

第一版 UI 入口优先级：

1. AgentRun workspace 的 Task / Todo 面板。
2. Story 页面中的 Task projection 区域。
3. Companion / subagent 派发入口。
4. Dynamic workflow fanout 选择器。

AgentRun workspace 是最贴近 Task 一等模型的入口。Story 页面只展示与 Story 关联的投影视图。

具体收口方向：

- Story 页面保留 Story brief、状态、context、Story Task projection、linked AgentRuns / Lifecycle runs。
- TaskDrawer 保留 Task 命名，聚焦计划项编辑、状态推进和 linked runs 查看。
- Task execution panel 使用 SubjectExecutionView 或 linked runs projection。
- StoryBoard / bulk / quick jump 等 PM 产品化表面按当前目标重新评估，优先服务 Story subject run 和 Todo list。

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

- Task status enum 从执行状态迁移为 Todo 计划状态。
- Task artifacts 迁出 Task 实体事实源，改为 execution projection 或 linked artifacts。
- Task dispatch preference 拆分为 context source、launch hint 或 dispatch command 参数。
- Story projection links 或 subject association 负责解释 Story 与 Task 的关系。

迁移时需要处理旧状态值和 artifacts 字段，优先选择确定性映射。

## 待后续细化

1. Task 物理仓储形态：独立表、run-scoped table 或 lifecycle task projection 表的具体落地。
2. Assignment link 与 LifecycleRun / AgentRun 外键的精确方向。
3. Frame construction Block 的接口形态与裁剪策略。
4. Story projection 的权限过滤和 UI 展示策略。
5. Dynamic workflow 从 Task 集合读取数据时的 selector DSL。
6. SubjectExecutionView 对 Story / Task linked runs 的统一 DTO 形态。
