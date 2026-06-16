# Story / Task Subject Model Cleanup Implementation Plan

## 当前结论

Story 保留为 Project 下的主题与上下文入口，状态可由用户或具备能力的 Agent 通过 Story command 推进。Task 收敛为 LifecycleRun aggregate 内的计划项事实。Story-bound AgentRun 不引入新的 runtime 类型，只是在普通 AgentRun / LifecycleRun 上携带 Story subject、Story 注入 Block 与 Story scope capability。

第一版实现应围绕 AgentRun workspace 和 `LifecycleRun.tasks` 查询展开，避免建立 Project/global Task 池。Story 页面只读由 Story-bound run、linked run 和可选 `story_ref` 推导出的 Task projection，不直接创建 Task。

当前没有需要保留的旧 Task 数据。迁移只需要把 schema 推到正确模型，初始化 `lifecycle_runs.tasks` 并让旧 Story child Task 字段退出主线；不做 legacy backfill、双读、兼容字段或旧 endpoint fallback。

## 阶段 1：Spec gate

1. 先更新会误导实现的长期 spec，尤其是 backend Story / Task runtime、database、repository、frontend type/state 和 cross-layer contract。
2. 在 spec 中明确 Task facts 跟随 `LifecycleRun` aggregate，Story 只消费 projection。
3. 在 spec 中明确 Task plan facts 不保存 execution status、artifacts 或 `dispatch_preference`。
4. 在任务文档与 manifest 中加入 backend / frontend / cross-layer review 结果，确保 implement / check agent 读取同一目标模型。
5. 实现前确认相关 spec 都指向 `LifecycleRun.tasks` 作为 Task durable facts 来源。

验收：

- `.trellis/spec/backend/story-task-runtime.md` 与本任务设计一致。
- `.trellis/spec/backend/repository-pattern.md` 不再要求 `StoryRepository` 承担 Task durable CRUD。
- manifest 包含本轮补充的 database / repository / frontend / cross-layer spec 与三份 research。
- 相关 spec 都指向 `LifecycleRun.tasks` 与 Story projection 的同一事实源模型。

## 阶段 2：Lifecycle Task facts

1. 在 `LifecycleRun` aggregate 上新增结构化 `tasks` 字段。
2. 定义 Task value object：`id`、`title`、`body`、`status`、`priority`、`created_by_agent_id`、`owner_agent_id`、`assigned_agent_id`、`source_task_id`、`created_at`、`updated_at`、`archived_at`、可选 `context_refs`、可选 `story_ref`。
3. 将 Task 状态枚举收敛为 `open / active / review / blocked / done / dropped`，其中 `review` 是可选验收状态。
4. 在 LifecycleRun aggregate mutation 中提供 create / update / archive / status transition 能力。
5. 保留 `SubjectContextAssignmentResolver` 作为 Story / Task / Project subject context 解析核心；Task subject context 从 owning LifecycleRun 的 Task value object 与 projection refs 解析。
6. 明确 Project 级 Task visibility 留作明确产品入口出现后的聚合视图，第一版不做 Project/global Task 池。

验收：

- `LifecycleRun.tasks` 默认空集合，并能随 aggregate create/update/select 整体 roundtrip。
- Task 状态集合与文档一致，旧执行态不再作为 Task plan status。
- Task facts 不包含 artifacts、execution status 或 `dispatch_preference`。
- Story 不持有 Task domain facts。

## 阶段 3：Migration

1. 新增 migration，为 `lifecycle_runs` 增加 `tasks` JSON 文本列，默认 `[]`，列名使用业务语义，不使用 `_json` 后缀。
2. migration 中让 `stories.tasks` / 旧 Task child 字段退出主线 schema。
3. 当前没有存量 Task 数据，不做复杂 backfill，也不创建占位 LifecycleRun。
4. 更新 LifecycleRun repository INSERT / SELECT / UPDATE 与 row mapping，覆盖 `tasks` 的序列化、反序列化和坏 JSON 错误上下文。
5. 确认 `StoryRepository` 不再读写 Task durable facts，不再提供 task CRUD / `find_by_task_id`。
6. 保持普通任务只新增 migration，不修改既有 migration；如未来另做 baseline squash，必须走独立授权任务。

验收：

- `pnpm run migration:guard` 通过。
- clean DB migration 初始化通过。
- repository roundtrip 覆盖 `LifecycleRun.tasks` 默认值、更新、坏 JSON 错误上下文。
- 主线 repository 不再从 Story aggregate 读写 Task facts。

## 阶段 4：Contract 与 generated TS

1. 更新 Rust contract 中的 Task plan DTO、Task projection DTO、Run-scoped Task command request / response。
2. `TaskResponse` 不再包含 `dispatch_preference`、`artifacts`、execution status 语义。
3. Story Task projection endpoint 返回由 Story-bound run / linked run 推导出的 projection DTO。
4. Task execution view 统一使用 `/subjects/task/{id}/execution` 的 `SubjectExecutionView`。
5. 重新生成 TypeScript contracts，前端不保留旧字段兼容或旧状态 fallback。

验收：

- `pnpm run contracts:check` 通过。
- Generated TypeScript 中 TaskStatus 只包含 `open / active / review / blocked / done / dropped`。
- generated Task plan DTO 没有 `dispatch_preference`、`artifacts` 或 execution status 字段。
- 前端 service / store 只消费 generated DTO，不手写新旧字段别名。

## 阶段 5：Backend command / read model

1. 提供 Run-scoped Task create / update / archive / query command。
2. Task 操作通过 LifecycleRun aggregate mutation 或 Lifecycle application command 完成；`StoryRepository` 不承担 Task durable CRUD。
3. AgentRun workspace 查询当前 run 内 Task，并支持 created / owner / assigned agent 维度过滤。
4. Story projection read model 从 Story-bound LifecycleRun、linked run 与可选 `story_ref` 推导 Task projection。
5. SubjectExecutionView 继续承担运行事实、linked runs、latest runtime node、artifacts；Task facts 不保存这些执行投影。
6. Assignment 设计为 Task facts 中的计划层关联，并通过 `LifecycleSubjectAssociation` / Agent lineage / runtime anchor 投影执行事实。
7. Frame construction Block 继续作为 Story / Task context 注入组合点：Story projection、selected Task、run Task summary 都从新 read model 进入。

验收：

- AgentRun workspace 能在 run scope 内创建、推进、归档 Task。
- Story projection tests 覆盖 Story-bound run 内 Task 可见，非 Story-bound run 不误投影。
- SubjectExecutionView tests 覆盖 Task subject association -> latest runtime node / artifacts。
- Story-bound AgentRun 的特殊上下文仍来自 Block 注入，普通 AgentRun 与 Story-bound AgentRun 共享 frame construction 机制。

## 阶段 6：Frontend split

1. AgentRun workspace 增加 Task plan 面板，成为 Task 创建、推进、归档和 assignment 的第一入口。
2. Story 页面只展示 Task projection 和来源关系，第一版不提供直接创建入口。
3. `storyStore.tasksByStoryId` 改成 Story projection cache，或由新的 run-scoped Task store 替代。
4. `lifecycleStore` 继续作为 SubjectExecution / runtime artifacts / linked runs 的唯一执行投影缓存。
5. TaskDrawer 聚焦计划项编辑、状态推进、assignment 和 linked runs 查看，不读取 `task.artifacts`。
6. Companion / subagent 入口支持从 Task 触发派发，dispatch / launch hint 不作为 Task facts 字段。
7. Dynamic workflow 入口支持选择 Task 集合作为 fanout 数据源。
8. 前端文案避免把 Task 状态写成 runtime 状态；`running / failed / cancelled` 只用于 LifecycleRun / AgentRun / RuntimeSession 视图。

验收：

- 用户可以在 AgentRun workspace 内创建、推进、归档 Task。
- 用户可以在 Story 页面看到由 Story-bound run / linked run 推导的 Task projection。
- 用户可以从 Task 进入派发和关联 run。
- `pnpm run frontend:check` 通过。
- focused tests 覆盖 TaskStatusBadge 新状态、Story projection-only、TaskDrawer 不读 `task.artifacts`。

## 阶段 7：MCP / capability

1. Story MCP 通过 Story-bound run 的 Task command 创建计划项，或查询 Story Task projection。
2. Task MCP 状态推进只接受 `open / active / review / blocked / done / dropped`。
3. artifact 上报不写 Task facts，改走 Lifecycle / SubjectExecution 关联产物。
4. Task management、collaboration、workflow capability 统一调用新的 command 与 SubjectExecution read model。
5. 为 Task create / update / assign / review / done 预留稳定 policy check，默认策略保持开放。
6. 为 Story projection read / update 预留 Story scope capability check，具体权限模型交由 permission system convergence review 任务处理。

验收：

- Story / Task MCP 写入口走 Run-scoped Task command。
- Task MCP 不再接受旧 TaskStatus。
- artifact 不再写入 Task facts。
- Task / Story 相关入口存在统一 policy hook。
- 默认行为不阻塞当前预研开发。
- 后续 permission 收束可接管这些 hook。

## 阶段 8：Cleanup / check

1. 删除旧状态、旧 `/tasks/{id}` 心智、旧 artifacts / dispatch preference surface。
2. 清理 `StoryRepository` task CRUD、`find_by_task_id`、Story Task payload guard 和前端旧缓存命名。
3. 确认 Task API 第一版只以 Run / AgentRun workspace 为作用域。
4. 确认 SubjectExecutionView 是 Story / Task runtime artifacts、latest runtime node、linked runs 的唯一执行投影。
5. 更新 E2E：不再验证 Task `dispatch_preference` 作为 Task 字段，改验证 AgentRun workspace Task 创建、Story projection、SubjectExecution linked runs。
6. 跑本任务测试计划并记录未覆盖风险。

验收：

- 长期 spec 与本任务目标模型一致。
- API / UI 语言不再暗示 StoryAgent 是独立 runtime，也不再暗示 Task 保存 runtime facts。
- Task management、collaboration、workflow capability 的边界清楚。
- `cargo check --workspace`、`pnpm run migration:guard`、`pnpm run contracts:check`、`pnpm run frontend:check` 按风险面执行。

## 建议拆分

1. `story-subject-run-entry`：Story 入口与 Story-bound AgentRun 初始化。
2. `lifecycle-task-facts`：`LifecycleRun.tasks` 字段、Task value object、状态机与 migration。
3. `run-scoped-task-api`：Run / AgentRun workspace Task command 与 Story projection read model。
4. `task-assignment-link`：单个派发、review gate 与关联 run。
5. `task-dynamic-workflow-source`：Task selector 与 fanout 数据源。
6. `story-task-projection-ui`：Story 投影视图与 AgentRun workspace 面板。

## 风险与判断点

1. `LifecycleRun.tasks` 需要保持 aggregate roundtrip 简洁，同时支持 Story projection 的解释链路。
2. Assignment link 的方向会影响查询 ergonomics，需要以 AgentRun workspace 和 Story projection 两条主路径验证。
3. Frame construction Block 不宜绑定 Story 专属流程，应作为普通 Agent 启动流程的组合能力。
4. Contract / generated TS 改动会集中暴露前端旧状态、旧字段和旧 store 命名，需要按类型错误收束。
5. Permission hook 需要位置稳定，但策略内容可以在后续任务中收束。

## 风险文件

- `crates/agentdash-domain/src/task/value_objects.rs`
- `crates/agentdash-domain/src/task/entity.rs`
- `crates/agentdash-domain/src/workflow/lifecycle_run.rs`
- `crates/agentdash-domain/src/story/repository.rs`
- `crates/agentdash-domain/src/story/entity.rs`
- `crates/agentdash-domain/src/story/value_objects.rs`
- `crates/agentdash-infrastructure/migrations/`
- `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_run_repository.rs`
- `crates/agentdash-infrastructure/src/persistence/postgres/story_repository.rs`
- `crates/agentdash-contracts/src/task/contract.rs`
- `crates/agentdash-contracts/src/story/contract.rs`
- `crates/agentdash-contracts/src/runtime/workflow.rs`
- `crates/agentdash-api/src/routes/stories.rs`
- `crates/agentdash-api/src/routes/task_execution.rs`
- `crates/agentdash-api/src/routes/lifecycle_views.rs`
- `crates/agentdash-application/src/lifecycle/subject_context_assignment.rs`
- `crates/agentdash-application/src/lifecycle/run_view_builder.rs`
- `crates/agentdash-application/src/task/service.rs`
- `crates/agentdash-application/src/task/gateway/repo_ops.rs`
- `crates/agentdash-application/src/task/gateway/artifact_ops.rs`
- `crates/agentdash-application/src/lifecycle/dispatch_service.rs`
- `crates/agentdash-mcp/src/servers/story.rs`
- `crates/agentdash-mcp/src/servers/task.rs`
- `crates/agentdash-domain/src/companion/skills/companion-system/SKILL.md`
- `packages/app-web/src/generated/task-contracts.ts`
- `packages/app-web/src/services/story.ts`
- `packages/app-web/src/stores/storyStore.ts`
- `packages/app-web/src/stores/lifecycleStore.ts`
- `packages/app-web/src/pages/StoryPage.tsx`
- `packages/app-web/src/features/task/task-drawer.tsx`
- `packages/app-web/src/features/task/task-subject-execution-panel.tsx`
- `packages/app-web/src/components/ui/status-badge.tsx`

## 验证命令

- `cargo check --workspace`
- `pnpm run migration:guard`
- clean DB migration 初始化通过
- `pnpm run contracts:check`
- `pnpm run frontend:check`
- LifecycleRun aggregate tests 覆盖 Task create/update/archive/status transition。
- SubjectExecutionView tests 覆盖 Task subject association -> latest runtime node / artifacts。
- Story projection tests 覆盖 Story-bound run 内 Task 可见，非 Story-bound run 不误投影。
- MCP tests 覆盖旧 TaskStatus 不再接受，artifact 不再写 Task facts。
- 前端 focused tests 覆盖 TaskStatusBadge 新状态、Story projection-only、TaskDrawer 不读 `task.artifacts`。
- E2E 更新：不再验证 Task `dispatch_preference` 作为 Task 字段，改验证 AgentRun workspace Task 创建、Story projection、SubjectExecution linked runs。
