# Story / Task Subject Model Cleanup Implementation Plan

## 当前结论

Story 保留为 Project 下的主题与上下文入口，状态可由用户或具备能力的 Agent 通过 Story command 推进。Task 收敛为 LifecycleRun 控制树内的 Todo facts / plan item。Story-bound AgentRun 不引入新的 runtime 类型，只是在普通 AgentRun / LifecycleRun 上携带 Story subject、Story 注入 Block 与 Story scope capability。

第一版实现应围绕 AgentRun workspace 和 run tree 查询展开，避免提前建立 Project 全局 Task 队列。Story 页面通过显式 link 与 run tree projection 获得 Task 视图。

## 阶段 1：模型收束

1. 梳理当前 Story / Task domain、API、frontend contract 中的实体定义。
2. 将 StoryAgent 相关表达收束为 Story-bound AgentRun 入口概念。
3. 将 Task 状态语义收束为 `open / active / review / done / blocked / dropped`，其中 `review` 是可选验收状态。
4. 将 Task execution state、workflow state、runtime state 的字段或 API 语义迁回 LifecycleRun / Runtime trace。
5. 明确 Task 的 root LifecycleRun / owner AgentRun / assigned AgentRun 关系。
6. 保留 `SubjectContextAssignmentResolver` 作为 Story / Task / Project subject context 解析核心。

验收：

- Story 相关代码不要求独立 StoryAgent runtime。
- Task API 文案和类型不把 Task 描述为执行事实源。
- 状态集合与文档一致。

## 阶段 2：仓储与查询

1. 设计 Task 以 root LifecycleRun 控制树为集合边界的仓储访问方式。
2. 提供 AgentRun workspace 所需查询：当前 run 创建、拥有、派发、被派发的 Task。
3. 提供 root LifecycleRun 聚合查询：控制树内 Task 列表与概要。
4. 提供 Story projection 查询：显式 Story link + Story subject run tree。
5. 将 Project 级 Task visibility 留作明确产品入口出现后的聚合视图。
6. 从 Story aggregate 生命周期中拆出 Task durable facts，并通过 link / association 支撑 Story projection。

验收：

- AgentRun workspace 能局部读取 Task / Todo 列表。
- Story 页面能解释每个 Task 的来源关联。
- 常用路径不依赖扫描 Project 下全部 Task。

## 阶段 3：Assignment 与 Subagent

1. 定义统一 Task assignment link，服务单个派发与批量 fanout。
2. 单个 Task 可计划派发给 Companion subagent 或其它 AgentRun。
3. 派发后创建或关联执行 run，并保留 Task -> run 的关系链。
4. Task 状态更新保持计划语义；执行状态从关联 run 查询。
5. 确认 `SubjectRef(kind = task)`、`LifecycleSubjectAssociation` 或专用 execution link 的取舍。
6. 为不同 assignment mode 定义推进策略：自执行可直接 done，subagent 派发默认进入 review gate，done 由父 AgentRun、人类或 owner capability 确认。

验收：

- 单个 Task 派发和 dynamic workflow fanout 使用同一类关联链路。
- Task 层不维护批量审计、重试、依赖和调度状态。
- Subagent 不能自行越过 owner review gate 将 Task 确认为 done。
- UI 能从 Task 进入关联 AgentRun / LifecycleRun。

## 阶段 4：Dynamic Workflow 数据源

1. 提供从 root LifecycleRun / Story projection 中选择 Task 集合的 selector。
2. 将选中 Task 作为 workflow node input 或 fanout input。
3. 由 workflow runtime 负责并发、批次、依赖、重试和审计。
4. 用统一 assignment link 记录 Task 与实际执行 run 的对应关系。
5. 保留 `create_tasks_from_plan`、`assign_tasks`、`fanout_tasks` 的应用层命令边界。

验收：

- Dynamic workflow 可以基于 Task 集合创建 subagent run。
- Task 数据只作为输入和关联来源。
- Workflow trace 能回答 fanout 运行事实。

## 阶段 5：Frame Construction Block

1. 抽出 Story / Task 注入 Block 的组合点。
2. Story 入口可注入 Story subject、Story context 与 Story Task projection。
3. Task 派发入口可注入 selected Task 与必要的 run tree Task summary。
4. Block 构造阶段预留权限过滤与内容裁剪接口。

验收：

- Story-bound AgentRun 的特殊上下文来自 Block 注入。
- 普通 AgentRun 与 Story-bound AgentRun 共享 frame construction 机制。
- Story-bound AgentRun 可以在具备 Story scope capability 时通过 Story command 推进 Story 状态。
- 后续扩展 Task / Story blocks 不需要改 runtime 主流程。

## 阶段 6：UI

1. AgentRun workspace 增加 Task / Todo 面板。
2. Story 页面展示 Task projection，并标注来源关系。
3. Companion / subagent 入口支持从 Task 触发派发。
4. Dynamic workflow 入口支持选择 Task 集合作为 fanout 数据源。
5. TaskDrawer 聚焦计划项编辑与 linked runs 查看。
6. TaskSubjectExecutionPanel 收口到 SubjectExecutionView 或 linked runs projection。
7. 前端文案避免把 Story subject run 表达成独立 runtime 实体。

验收：

- 用户可以在 AgentRun workspace 内创建、推进、归档 Task。
- 用户可以在 Story 页面看到与该 Story 显式相关的 Task。
- 用户可以从 Task 进入派发和关联 run。

## 阶段 7：Permission 接入点

1. 为 Task create / update / assign / fanout 预留 policy check。
2. 为 Story projection read / update 预留 Story scope capability check。
3. 默认策略保持开放。
4. 具体权限模型交由 permission system convergence review 任务处理。

验收：

- Task / Story 相关入口存在统一 policy hook。
- 默认行为不阻塞当前预研开发。
- 后续 permission 收束可接管这些 hook。

## 阶段 8：Spec 收口

1. 更新 `.trellis/spec/backend/story-task-runtime.md`，将 Task 定位为 AgentRun-created Todo facts / plan item，并将 Story 侧定义为 projection。
2. 更新 frontend / cross-layer contract spec，明确 Story subject run、Task linked runs、Story Task projection 的 UI/API 语言。
3. 补充 Task assignment / companion subagent / dynamic workflow fanout 的 association 与 command 边界。
4. 补充 fanout policy 口径：默认开放，允许 workflow / project policy / permission grant 后续切换为审批模式。

验收：

- 长期 spec 与本任务目标模型一致。
- API / UI 语言不再暗示 StoryAgent 是独立 runtime。
- Task management、collaboration、workflow capability 的边界清楚。

## 建议拆分

1. `story-subject-run-entry`：Story 入口与 Story-bound AgentRun 初始化。
2. `task-run-tree-model`：Task 状态、仓储边界与 run tree 查询。
3. `task-assignment-link`：单个派发与关联 run。
4. `task-dynamic-workflow-source`：Task selector 与 fanout 数据源。
5. `story-task-projection-ui`：Story 投影视图与 AgentRun workspace 面板。

## 风险与判断点

1. Task 仓储需要贴近 root LifecycleRun 控制树，同时支持 Story projection 的解释链路。
2. Assignment link 的方向会影响查询 ergonomics，需要以 AgentRun workspace 和 Story projection 两条主路径验证。
3. Frame construction Block 不宜绑定 Story 专属流程，应作为普通 Agent 启动流程的组合能力。
4. Permission hook 需要位置稳定，但策略内容可以在后续任务中收束。

## 风险文件

- `crates/agentdash-domain/src/task/value_objects.rs`
- `crates/agentdash-domain/src/task/entity.rs`
- `crates/agentdash-domain/src/story/repository.rs`
- `crates/agentdash-domain/src/story/entity.rs`
- `crates/agentdash-infrastructure/src/persistence/postgres/story_repository.rs`
- `crates/agentdash-api/src/routes/stories.rs`
- `crates/agentdash-api/src/routes/task_execution.rs`
- `crates/agentdash-application/src/workflow/subject_context_assignment.rs`
- `crates/agentdash-application/src/task/service.rs`
- `crates/agentdash-application/src/workflow/dispatch_service.rs`
- `crates/agentdash-domain/src/companion/skills/companion-system/SKILL.md`
- `packages/app-web/src/pages/StoryPage.tsx`
- `packages/app-web/src/features/task/task-drawer.tsx`
- `packages/app-web/src/features/task/task-subject-execution-panel.tsx`

## 验证命令

- `cargo check --workspace`
- `pnpm typecheck`
- `pnpm test -- --run`
- 针对 migration 增加或更新检查脚本。
- 用 Story 页面手动验证：创建 Story、编辑人工状态、创建 Task、启动 Story subject run、查看 Story Task projection 和 linked run。
