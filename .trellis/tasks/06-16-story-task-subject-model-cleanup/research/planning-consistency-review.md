# Research: planning-consistency-review

- Query: 审查 `.trellis/tasks/06-16-story-task-subject-model-cleanup` 的 PRD / design / implement 是否足以支撑一次真实迁移，并结合 backend / frontend / cross-layer / shared spec 判断缺口。
- Scope: internal
- Date: 2026-06-16

## Findings

### 实际检查过的关键文件路径

- `.trellis/tasks/06-16-story-task-subject-model-cleanup/task.json` - 任务当前元数据，状态为 planning。
- `.trellis/tasks/06-16-story-task-subject-model-cleanup/prd.md` - 业务目标、非目标、状态语言和验收标准。
- `.trellis/tasks/06-16-story-task-subject-model-cleanup/design.md` - Story / Task / AgentRun / LifecycleRun 目标模型与迁移考虑。
- `.trellis/tasks/06-16-story-task-subject-model-cleanup/implement.md` - 分阶段执行计划、风险文件与验证命令。
- `.trellis/tasks/06-16-story-task-subject-model-cleanup/implement.jsonl` - implement subagent context manifest。
- `.trellis/tasks/06-16-story-task-subject-model-cleanup/check.jsonl` - check subagent context manifest。
- `.trellis/workflow.md` - Trellis 规划 artifact、jsonl context 与复杂任务启动规则。
- `.trellis/spec/backend/index.md` - backend spec 索引。
- `.trellis/spec/backend/story-task-runtime.md` - 当前 Story / Task / SubjectRef / Lifecycle projection 长期规范。
- `.trellis/spec/backend/database-guidelines.md` - PostgreSQL migration、schema readiness、migration guard 契约。
- `.trellis/spec/backend/repository-pattern.md` - aggregate repository 边界与跨聚合事务规则。
- `.trellis/spec/backend/domain-payload-typing.md` - 高频领域 payload 类型化规则。
- `.trellis/spec/backend/workflow/architecture.md` - LifecycleRun / RuntimeNodeState / subject association 运行态事实源。
- `.trellis/spec/backend/workflow/lifecycle-run-link.md` - LifecycleSubjectAssociation contract。
- `.trellis/spec/backend/session/agentrun-mailbox.md` - AgentRun mailbox / conversation command 边界。
- `.trellis/spec/backend/permission/grant-lifecycle.md` - PermissionGrant 聚合、审批 API 与状态机。
- `.trellis/spec/backend/permission/policy-engine.md` - permission policy / scope escalation contract。
- `.trellis/spec/frontend/index.md` - frontend spec 索引。
- `.trellis/spec/frontend/architecture.md` - 前端权威状态与 runtime projection 边界。
- `.trellis/spec/frontend/type-safety.md` - generated wire、mapper 边界和 DTO 类型规则。
- `.trellis/spec/frontend/state-management.md` - storyStore / lifecycleStore 分工和服务端缓存边界。
- `.trellis/spec/frontend/quality-guidelines.md` - 前端质量检查和 snake_case 契约。
- `.trellis/spec/cross-layer/index.md` - cross-layer spec 索引。
- `.trellis/spec/cross-layer/architecture.md` - Rust contract -> generated TS 的跨层事实源。
- `.trellis/spec/cross-layer/frontend-backend-contracts.md` - DTO 生成、drift check 与 route-local DTO 约束。
- `.trellis/spec/shared/index.md` - 共享命名、语言和错误处理约定。
- `.trellis/spec/guides/index.md` - thinking guide 索引。
- `.trellis/spec/guides/cross-layer-thinking-guide.md` - 跨层事实源与状态推断检查清单。
- `.trellis/tasks/06-14-module-overdesign-review/overdesign-review.md` - 既有 overdesign review 总结。
- `.trellis/tasks/06-14-module-overdesign-review/research/01-lifecycle-workflow-task.md` - Lifecycle / Workflow / Task 事实源证据。
- `crates/agentdash-domain/src/task/value_objects.rs` - 当前 TaskStatus、TaskExecutionProjection、TaskDispatchPreference、Artifact。
- `crates/agentdash-domain/src/task/entity.rs` - 当前 Task entity 字段与投影字段访问规则。
- `crates/agentdash-domain/src/story/entity.rs` - Story aggregate 当前持有 `Vec<Task>`。
- `crates/agentdash-domain/src/story/repository.rs` - StoryRepository 当前承担 Task child entity CRUD。
- `crates/agentdash-infrastructure/src/persistence/postgres/story_repository.rs` - PostgreSQL Story repository 对 `stories.tasks` JSONB 的读写。
- `crates/agentdash-infrastructure/migrations/0001_init.sql` - 当前 stories / lifecycle_subject_associations schema。
- `crates/agentdash-contracts/src/task/contract.rs` - 当前 Task wire contract 与 generated TS 来源。
- `crates/agentdash-contracts/src/story/contract.rs` - 当前 Story wire contract。
- `crates/agentdash-contracts/src/generate_ts.rs` - Task / SubjectExecutionView generated TS 入口。
- `crates/agentdash-contracts/src/runtime/workflow.rs` - SubjectRef / SubjectExecutionView contract。
- `crates/agentdash-api/src/routes/stories.rs` - Story / Task HTTP route 当前命令面。
- `crates/agentdash-api/src/dto/story.rs` - route-local Story / Task request DTO。
- `crates/agentdash-api/src/routes/task_execution.rs` - `/tasks/{id}/execution` 专属 route。
- `crates/agentdash-api/src/dto/task_execution.rs` - TaskExecutionViewResponse route-local DTO。
- `crates/agentdash-api/src/routes/lifecycle_views.rs` - subject execution API route。
- `crates/agentdash-api/src/routes/lifecycle_contracts.rs` - SubjectExecutionView API contract mapping。
- `crates/agentdash-application/src/story/management.rs` - Story / Task mutation use cases。
- `crates/agentdash-application/src/task/service.rs` - Task execution read projection service。
- `crates/agentdash-application/src/task/execution.rs` - TaskExecutionView application DTO。
- `crates/agentdash-application/src/task/gateway/repo_ops.rs` - Task lookup/status change bridge via Story aggregate。
- `crates/agentdash-application/src/task/gateway/resolve.rs` - Task -> Story / Project / Workspace context resolution。
- `crates/agentdash-application/src/task/gateway/effect_executor.rs` - Task status update hook effect path。
- `crates/agentdash-application/src/task/gateway/artifact_ops.rs` - Task artifact write path。
- `crates/agentdash-mcp/src/servers/task.rs` - Task MCP 状态推进与 artifact 工具。
- `crates/agentdash-mcp/src/servers/story.rs` - Story MCP 创建/更新 Task 入口。
- `packages/app-web/src/generated/task-contracts.ts` - 当前生成的 TaskResponse / TaskStatus。
- `packages/app-web/src/types/index.ts` - 前端 Task / TaskStatus 类型 re-export。
- `packages/app-web/src/services/story.ts` - Story / Task service、payload guard 和 API endpoint。
- `packages/app-web/src/stores/storyStore.ts` - storyStore 中 `tasksByStoryId` 与 Task refresh / state change reducer。
- `packages/app-web/src/stores/lifecycleStore.ts` - SubjectExecutionView 缓存。
- `packages/app-web/src/features/task/task-drawer.tsx` - Task drawer 当前编辑、artifacts 和 execution panel。
- `packages/app-web/src/features/task/task-subject-execution-panel.tsx` - Task SubjectExecution 面板。
- `packages/app-web/src/pages/StoryPage.tsx` - Story 页面 Task projection / review label。
- `packages/app-web/src/components/ui/status-badge.tsx` - TaskStatusBadge 旧状态映射。

### 总体判断

PRD / design 已经足够表达“想要什么模型”：Story 是 Project 下 subject / context container，Task 是 AgentRun / LifecycleRun 控制树内 Todo facts / plan item，执行事实回到 LifecycleRun / AgentFrame / RuntimeNodeState / RuntimeSession trace。PRD 明确现有重复事实源包括 Task execution DTO、Task runtime status、Task artifacts、Task dispatch preference、Story / Task MCP 独立状态推进工具和前端 Task execution panel（`prd.md:29`）。design 也明确 Task durable facts 应脱离 Story aggregate 生命周期，并通过 link / association 支撑 Story projection（`design.md:66`）。

但这些文档目前还不足以直接支撑一次真实迁移。原因不是目标不清，而是缺少可执行迁移 contract：Task 物理仓储形态、数据迁移策略、API / DTO 断点、generated TS 更新顺序、前端 store 分层、MCP / capability 命令迁移和测试矩阵仍然停留在“待后续细化”层级。`design.md` 自己也把 Task 物理仓储形态、assignment link 外键方向、Frame block 接口、Story projection 权限过滤、Task selector DSL、SubjectExecutionView 统一 DTO 列为待细化项（`design.md:237`-`design.md:242`），这些恰好是 implement agent 真实动手会首先遇到的决策点。

当前最关键的 planning gap 是目标设计与长期 spec 仍相互冲突。任务目标要求 Task 脱离 Story aggregate，Story 只看 projection（`design.md:9`-`design.md:10`、`design.md:66`），但现有 backend spec 仍写明 Task 是 Story aggregate 下的 child entity，保存在 `stories.tasks` JSONB，无独立 repository / table（`.trellis/spec/backend/story-task-runtime.md:10`），并在关系拓扑中写 Story aggregate 持有 `Vec<Task>`（`.trellis/spec/backend/story-task-runtime.md:76`）。Repository spec 也仍写 Story aggregate 的 Task 变更走 `StoryRepository::update` 整体写回（`.trellis/spec/backend/repository-pattern.md:32`）。如果不先更新 spec 或在 implement plan 中明确 spec 更新时点，implement/check agent 会同时接收“保持 Story aggregate Task child”和“拆出 Task durable facts”的冲突指令。

### PRD / design / implement 的执行充分性

#### 已经足够的部分

- 业务边界清楚：PRD 明确 Story 不替代 AgentRun / LifecycleRun 执行模型，Task 不承担 runtime execution truth（`prd.md:5`-`prd.md:7`）。
- 状态语言方向清楚：目标 Task 状态是 `open / active / review / blocked / done / dropped`，且 `review` 不是必经状态（`prd.md:63`-`prd.md:72`、`design.md:82`-`design.md:103`）。
- 关联链路方向清楚：Task 与执行 run 通过 subject association / execution link / run refs 表达，运行事实从 Lifecycle projection 派生（`prd.md:24`-`prd.md:25`）。
- UI 目标方向清楚：AgentRun workspace 是 Task 一等入口，Story 页面只展示 projection，TaskDrawer 聚焦计划项编辑与 linked runs（`design.md:192`-`design.md:203`）。
- 权限边界方向清楚：permission / approval 只保留接入点，默认开放，后续独立收束（`prd.md:27`、`design.md:214`-`design.md:222`）。

#### 不足以执行的遗漏

- 缺少 Task 目标 schema 决策。design 给出“推荐最小字段语义”，但没有选定独立表、run-scoped table 或 lifecycle task projection 表，也没有列出唯一索引、FK、软归档字段、Story link / origin refs / assignment refs 的具体列。实现计划只写“设计 Task 以 root LifecycleRun 控制树为集合边界的仓储访问方式”（`implement.md:26`），不足以让 migration 落地。
- 缺少数据迁移映射。当前数据库 `stories.tasks jsonb DEFAULT '[]'::jsonb NOT NULL`（`crates/agentdash-infrastructure/migrations/0001_init.sql:734`），repository 用 JSONB containment 反查 Task（`story_repository.rs:143`、`story_repository.rs:382`）。design 只说“优先选择确定性映射”（`design.md:228`-`design.md:231`），但没有定义旧 `pending / assigned / running / awaiting_verification / completed / failed / cancelled` 到新 `open / active / review / done / blocked / dropped` 的逐值映射，也没有定义 artifacts / dispatch_preference 拆出后的目标字段或投影来源。
- 缺少 API / DTO 断点。当前 `TaskResponse` 仍暴露 `status`、`dispatch_preference`、`artifacts`（`crates/agentdash-contracts/src/task/contract.rs:102`-`crates/agentdash-contracts/src/task/contract.rs:111`），`/tasks/{id}/execution` 仍有独立 `TaskExecutionViewResponse` route-local DTO（`crates/agentdash-api/src/dto/task_execution.rs:8`、`crates/agentdash-api/src/routes/task_execution.rs:18`）。design 说它应复用或让位给 SubjectExecutionView（`design.md:174`），但 implement plan 没有明确保留、替换或删除 endpoint 的具体阶段和验收。
- 缺少 generated type 更新安排。cross-layer spec 要求 Rust contract type -> ts-rs generated TS -> frontend service / reducer（`.trellis/spec/cross-layer/frontend-backend-contracts.md:16`），并要求 `pnpm run contracts:check` 发现 drift（`.trellis/spec/cross-layer/frontend-backend-contracts.md:27`）。当前 generated TS 仍是旧 TaskResponse / TaskStatus（`packages/app-web/src/generated/task-contracts.ts:13`-`packages/app-web/src/generated/task-contracts.ts:15`）。implement plan 只列了 `pnpm typecheck`，没有明确 contract crate、generator、generated file、frontend store 同步顺序。
- 缺少前端 state 分层安排。frontend spec 写 `storyStore` 存 Story/Task 数据、`lifecycleStore` 存 lifecycle runtime view projection（`.trellis/spec/frontend/state-management.md:26`-`.trellis/spec/frontend/state-management.md:30`），并要求 store 不成为协议字段事实源（`.trellis/spec/frontend/state-management.md:54`-`.trellis/spec/frontend/state-management.md:55`）。当前 `storyStore` 仍以 `tasksByStoryId` 缓存 Story 下 Task（`packages/app-web/src/stores/storyStore.ts:23` 附近），Task drawer 直接读 `task.artifacts`（`task-drawer.tsx:84`），TaskSubjectExecutionPanel 同时 refresh Task 和 fetch SubjectExecution（`task-subject-execution-panel.tsx:122`-`task-subject-execution-panel.tsx:131`）。implement plan 没有明确 Task projection store 是否仍挂在 storyStore、哪些运行态移动到 lifecycleStore、哪些写后需要 refetch。
- 缺少 MCP / capability 迁移顺序。PRD 已点名 Story / Task MCP 独立状态推进工具要收口（`prd.md:29`），代码中仍有 `UpdateTaskStatusParams` 和 `update_task_status` 工具（`crates/agentdash-mcp/src/servers/task.rs:24`、`crates/agentdash-mcp/src/servers/task.rs:155`），Story MCP 仍写 `dispatch_preference`（`crates/agentdash-mcp/src/servers/story.rs:407`、`crates/agentdash-mcp/src/servers/story.rs:468`）。implement plan 没有列出工具 schema、agent-facing skill 文档、capability path、permission hook 的迁移顺序。
- 缺少真实测试矩阵。implement plan 只列 `cargo check --workspace`、`pnpm typecheck`、`pnpm test -- --run`、migration 检查和一次手动 Story 页面验证（`implement.md:157`-`implement.md:163`）。这对跨层 schema / DTO / generated / UI / permission / runtime projection 迁移不足。

#### 歧义

- `Task 创建者 / owner / manager / assigned AgentRun` 的字段语义未严格区分。design 推荐 `root_lifecycle_run_id`、`owner_agent_run_id`、`created_by_agent_run_id`、`assigned_agent_run_id`（`design.md:49`-`design.md:64`），但 `owner_agent_run_id` 与 `assigned_agent_run_id` 的更新规则、复派发规则、human-owned Task 如何表示、Story 页面用户创建 Task 是否允许 root run 为空都未定义。
- `story_link` 的形态未定义。design 同时说 Task 可以显式 link 到 Story、Routine、External source 或 AgentRun（`design.md:45`），又说 `story_link` 是推荐字段（`design.md:58`），并说 Story projection links 或 subject association 负责解释关系（`design.md:231`）。需要明确是否使用 typed link table、LifecycleSubjectAssociation metadata、或 Task 表字段。
- `review` 与 permission / approval 的边界未落到命令 contract。design 说 subagent 可提交 review，done 由 owner 确认（`design.md:109`），permission hook 默认开放（`design.md:214`-`design.md:222`），但没有定义 `submit_review` / `confirm_done` / `mark_done` 的 command 名称、谁能调用、默认开放时是否仍记录 policy decision。
- `Task artifacts` 的目标归属仍模糊。design 说 artifacts 迁出 Task 实体，改为 execution projection 或 linked artifacts（`design.md:229`），同时 UI 仍有 TaskDrawer “执行产物”区域。需要明确第一阶段是否删除 `TaskResponse.artifacts`、是否只读 `SubjectExecutionView.artifacts`、以及 artifact 历史是否通过 lifecycle artifact refs 迁移。
- `dispatch_preference` 的拆分对象未定义。design 说拆成 context source、launch hint 或 dispatch command 参数（`design.md:230`），但当前 `TaskDispatchPreference` 同时承担 agent_type、preset、prompt_template、initial_context、context_sources（`crates/agentdash-domain/src/task/value_objects.rs:73`）。实现前应明确哪些进入 Task durable plan、哪些进入 assignment command、哪些进入 frame construction block。

#### 顺序风险

- 先改领域 enum / DTO 会直接破坏前端 generated contract 和 payload guard。当前前端 `taskStatusValues` 手写旧状态集合（`packages/app-web/src/services/story.ts:31`-`packages/app-web/src/services/story.ts:44`），`TaskStatusBadge` 用 `Record<TaskStatus, ...>` 穷尽旧状态（`packages/app-web/src/components/ui/status-badge.tsx:29`），Story 页面 review label 依赖 `awaiting_verification / completed / failed / cancelled / running / assigned / pending`（`packages/app-web/src/pages/StoryPage.tsx:60`-`packages/app-web/src/pages/StoryPage.tsx:73`）。
- 先拆 `stories.tasks` 而未提供 TaskRepository / migration / backfill / read model，会让 `StoryRepository::find_by_task_id`、`load_task_story_project_with_permission`、Task gateway、Story route 全部断开。当前 repository 多处 SELECT / UPDATE 带 `tasks` 列（`story_repository.rs:37`、`story_repository.rs:74`、`story_repository.rs:402`）。
- 先删除 `/tasks/{id}/execution` 而未填充 SubjectExecutionView 的 latest runtime node / artifacts，会让 Task drawer 失去执行视图。06-14 research 已指出 SubjectExecutionView 暴露目标字段但实际 builder 未填 runtime node/artifacts，同时旁边还有 `/tasks/{id}/execution` 轻量 DTO（`.trellis/tasks/06-14-module-overdesign-review/research/01-lifecycle-workflow-task.md:131`-`.trellis/tasks/06-14-module-overdesign-review/research/01-lifecycle-workflow-task.md:153`）。
- 先让 Story 页面展示 run-tree projection 而未定义 projection query，会重新引入“扫描 Project / Story 下所有 Task”的性能和解释风险。目标恰恰是不引入 Project 全局 Task 队列（`prd.md:35`、`design.md:76`-`design.md:78`）。
- 先实现 fanout / assignment 而未稳定 Task 表和 association 会让 workflow / companion / permission 三方各自制造 link 事实源。workflow spec 已要求业务状态不能由 RuntimeSession title、存在性或 trace 内容推断，业务归属落到 LifecycleSubjectAssociation（`.trellis/spec/backend/workflow/architecture.md:46`）。

### 项目规范符合性检查

#### 跨层 DTO

当前规划方向符合 cross-layer 规范，但缺少执行安排。规范要求业务 DTO 来源是 Rust contract type，生成 TypeScript 进入 `packages/app-web/src/generated/*`，前端不长期手写后端 DTO（`.trellis/spec/cross-layer/frontend-backend-contracts.md:16`-`.trellis/spec/cross-layer/frontend-backend-contracts.md:29`）。当前 TaskResponse / TaskStatus 已在 `agentdash-contracts` 并生成到 `task-contracts.ts`（`crates/agentdash-contracts/src/task/contract.rs:9`、`packages/app-web/src/generated/task-contracts.ts:13`-`packages/app-web/src/generated/task-contracts.ts:15`）。

缺口：

- design / implement 没有明确新增或替换哪些 contract DTO：TaskPlanDto、TaskAssignmentDto、TaskProjectionDto、TaskCommandResponse、StoryTaskProjectionDto、SubjectExecutionView extension 等。
- `TaskExecutionViewResponse` 仍是 API `dto` route-local 类型（`crates/agentdash-api/src/dto/task_execution.rs:8`），但它被前端功能消费时按 cross-layer spec 应进入 contract crate，或被 SubjectExecutionView 替代。
- `implement.jsonl` 有 `.trellis/spec/cross-layer/frontend-backend-contracts.md`，但 `check.jsonl` 没有该文件；真实 check agent 可能不会被显式提醒 generated contract drift。

建议文档更新：

- 在 `design.md` 增加 “Wire contract migration” 小节：列出要新增/替换/删除的 Rust contract DTO 和 generated TS 文件。
- 在 `implement.md` 的每个 DTO 阶段写明 `cargo run -p agentdash-contracts --bin generate_contracts_ts -- --check` / `pnpm run contracts:check` 的验收信号。
- 在 `implement.jsonl` / `check.jsonl` 补充 cross-layer frontend-backend contract、frontend type-safety、backend database spec。

#### 数据库 migration

当前规划明确“预研期可以直接做正确迁移”（`design.md:226`），但没有满足 database spec 对 migration 的要求。规范要求业务 schema 事实源是 `crates/agentdash-infrastructure/migrations/`，普通任务新增 migration，不修改已提交 migration（`.trellis/spec/backend/database-guidelines.md:41`-`.trellis/spec/backend/database-guidelines.md:43`），schema 变更必须跑 `pnpm run migration:guard`（`.trellis/spec/backend/database-guidelines.md:59`-`.trellis/spec/backend/database-guidelines.md:60`、`.trellis/spec/backend/database-guidelines.md:121`）。

缺口：

- 没有选定目标 schema，因此无法判断是新增 `tasks` 表、`lifecycle_tasks` 表、`task_assignments` 表、`task_story_links` 表，还是扩展 `lifecycle_subject_associations.metadata`。
- 没有明确旧 `stories.tasks` JSONB 到新表的 backfill 语义。
- 没有写 `DROP COLUMN IF EXISTS stories.tasks` / 保留 read-only projection / 阶段性保留列的判断。规范允许删除旧列用新增 migration（`.trellis/spec/backend/database-guidelines.md:68`）。
- 没有说明是否为 baseline squash / reset / merge 任务。当前任务不是数据库 baseline 任务，因此不应修改 `0001_init.sql`；只能新增 migration。若要 squash，任务文档必须明确授权范围、重建数据库要求和验证命令（`.trellis/spec/backend/database-guidelines.md:43`、`.trellis/spec/backend/database-guidelines.md:47`、`.trellis/spec/backend/database-guidelines.md:98`）。

建议文档更新：

- 在 `design.md` 将 “待后续细化：Task 物理仓储形态” 前移为阻塞决策。
- 在 `implement.md` 增加 migration 阶段，明确新增 migration 文件、backfill、repository mapping、readiness、migration guard 和 repository integration 测试。

#### Generated type

当前 generated TS 仍是旧模型。前端规范要求 generated wire 单源，跨层 DTO union 来自 `src/generated/*`，mapper 不重新声明后端 enum/string union（`.trellis/spec/frontend/type-safety.md:11`、`.trellis/spec/frontend/type-safety.md:36`-`.trellis/spec/frontend/type-safety.md:41`）。当前 `packages/app-web/src/services/story.ts` 仍手写 `taskStatusValues`（`services/story.ts:31`-`services/story.ts:44`），这是事件 payload guard 的局部类型守卫，但一旦 enum 改名，必须同步更新，不能变成第二事实源。

缺口：

- 没有在 implement plan 中安排 `agentdash-contracts` 先行或同步更新。
- 没有安排生成文件 diff 审查和前端 `TaskStatusBadge` / StoryPage / TaskDrawer 的类型修复。
- 没有安排删除或替代 `TaskResponse.artifacts` / `TaskDispatchPreference` 后的前端 view model。

建议文档更新：

- 在 `implement.md` 每个跨层 DTO 阶段增加 “生成并检查 `packages/app-web/src/generated/*`” 验收。
- 在 `design.md` 明确哪些 UI view model 可以存在，哪些 wire 字段必须直接消费 generated DTO。

#### 测试

当前验证命令过粗，缺少与风险对应的 focused tests。database spec 要求 migration runner 初始化真实 schema 并通过 repository integration / bootstrap readiness 测试（`.trellis/spec/backend/database-guidelines.md:121`-`.trellis/spec/backend/database-guidelines.md:122`）。cross-layer spec 要求 contract check（`.trellis/spec/cross-layer/frontend-backend-contracts.md:127`-`.trellis/spec/cross-layer/frontend-backend-contracts.md:132`）。frontend quality 要求 `pnpm --filter app-web run check`（`.trellis/spec/frontend/quality-guidelines.md:8`-`.trellis/spec/frontend/quality-guidelines.md:9`）。

缺口：

- 没有 Task status migration unit test：旧执行态到新计划态逐值映射。
- 没有 repository roundtrip / migration backfill test：旧 `stories.tasks` JSONB -> 新 Task facts / links。
- 没有 SubjectExecutionView projection test：从 `SubjectRef(Task)` -> association -> agent/frame/runtime anchor -> latest runtime node / artifacts。
- 没有 frontend tests：Task status badge 新状态、Story Task projection 来源解释、Task drawer 不再从 `task.artifacts` 读执行产物。
- 没有 MCP / capability tests：Task 状态推进工具改走 Task plan command，subagent review gate 不能自行 done。

#### Spec 更新

implement plan 将 spec 收口放到阶段 8（`implement.md:112`-`implement.md:123`）。对这次任务来说，spec 更新不能完全后置，因为现有 spec 与目标设计直接冲突。至少在进入实现前，应先更新或补充一份 architecture decision，让 implement/check agent 不会按旧 spec 把 Task 继续留在 Story aggregate。

需要更新的 spec：

- `.trellis/spec/backend/story-task-runtime.md`：记录为什么 Task durable facts 的归属从 Story aggregate 调整为 AgentRun / LifecycleRun 控制树内 Todo facts，Story 只消费 projection；避免记录过去错误实现细节。
- `.trellis/spec/backend/repository-pattern.md`：如果新增 TaskRepository / TaskCommandPort / Unit of Work，记录为什么它对应新的聚合边界，以及 StoryRepository 不再承担 Task durable facts。
- `.trellis/spec/backend/database-guidelines.md` 不一定需要改，除非本任务决定进行 baseline squash；否则只在 task docs 里引用现有 migration 规则。
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`：记录 Task plan DTO、Task projection DTO、SubjectExecutionView / linked runs 的 wire contract 归属原因。
- `.trellis/spec/frontend/type-safety.md` 或 `.trellis/spec/frontend/state-management.md`：记录 Story Task projection 与 Task execution projection 分别由 storyStore / lifecycleStore 消费的原因。
- `.trellis/spec/backend/workflow/lifecycle-run-link.md`：如果 assignment link 复用 LifecycleSubjectAssociation metadata 或新增专用 execution link，需要记录为什么这样表达 Task -> run / agent / review association。
- `.trellis/spec/backend/permission/*`：仅在本任务实现新的 policy hook / review gate command 时更新；只记录默认开放与后续 PermissionGrant 接管的原因。

### 更稳妥的阶段化迁移方案

#### Phase 0: Spec / decision gate

目标：消除规划与现有 spec 冲突，让 implement agent 有唯一目标模型。

工作：

- 更新或补充 backend story-task runtime spec，明确 Task durable facts 从 Story aggregate child 调整为 run-tree Todo facts，Story Task 是 projection。
- 在 design.md 固化 Task 物理仓储形态、Task link / assignment link 形态、状态映射表、DTO 断点。
- 在 implement.jsonl / check.jsonl 补齐 database、repository、frontend type-safety、frontend state-management、cross-layer contract specs。

验收信号：

- `design.md` 不再把 Task 物理仓储形态列为待后续细化。
- `implement.md` 有 schema / DTO / generated / frontend / MCP / permission / tests 的逐阶段清单。
- implement/check manifest 包含足够 spec，不再只依赖旧 story-task-runtime。

#### Phase 1: Read model first, no durable schema move

目标：先修正 execution projection 事实链，降低后续迁移时的 UI / API 依赖风险。

工作：

- 让 SubjectExecutionView 真正从 `SubjectRef(Task)` -> association -> LifecycleAgent.current_frame -> RuntimeSessionExecutionAnchor -> orchestration node coordinate 填充 latest runtime node / artifacts。
- `/tasks/{id}/execution` 暂时复用 SubjectExecutionView 或明确标记为过渡 reader，但不再输出与 SubjectExecutionView 分裂的状态事实。
- 删除或冻结从缺失 lifecycle fact 推断 Task failed 的逻辑。06-14 research 已建议没有 lifecycle fact 时不写终态（`.trellis/tasks/06-14-module-overdesign-review/research/01-lifecycle-workflow-task.md:84`-`.trellis/tasks/06-14-module-overdesign-review/research/01-lifecycle-workflow-task.md:86`）。

验收信号：

- Task execution read model 能展示 latest runtime node / artifacts，且来源可追溯到 association / run / agent / frame / runtime node。
- 前端 TaskSubjectExecutionPanel 只依赖 SubjectExecutionView 的 execution projection，不从 Task entity artifacts 推导执行产物。
- 针对 agent-scoped Task association 的 projection test 通过。

#### Phase 2: Task plan status and wire contract migration

目标：把 Task status 从执行态迁到计划态，并同步 contract / generated TS / UI。

工作：

- 在 domain / contracts 中定义 `open / active / review / blocked / done / dropped`。
- 写明确旧状态到新状态的一次性迁移映射，例如 pending/assigned -> open 或 active 的判定依据、running -> active、awaiting_verification -> review、completed -> done、failed -> blocked 或 dropped 的产品判定、cancelled -> dropped。具体映射需由 design 固化。
- 更新 `TaskResponse` / request DTO / event payload guard / TaskStatusBadge / StoryPage review label。
- 跑 contract generation check，更新 generated TS。

验收信号：

- `packages/app-web/src/generated/task-contracts.ts` 中 TaskStatus 只包含目标计划态。
- 前端没有旧 TaskStatus 字符串引用（pending / assigned / awaiting_verification / cancelled 等仅可出现在 migration test 或文档说明中）。
- `pnpm run contracts:check`、`pnpm --filter app-web run check` 通过。

#### Phase 3: Durable Task facts schema migration

目标：将 Task durable facts 从 `stories.tasks` JSONB 迁到目标仓储。

工作：

- 新增 migration，创建目标 Task facts / assignment / link 表或扩展目标表；不修改 `0001_init.sql`，除非任务被明确授权为 baseline squash。
- Backfill `stories.tasks` 到新表，保留可解释的 `story_link` / origin association。
- 增加 TaskRepository / TaskCommandPort 或 Unit of Work，按 repository spec 表达新聚合边界。
- StoryRepository 移除 Task durable CRUD 职责，只保留 Story 自身字段与 projection 计数读取。

验收信号：

- migration runner 能从干净库初始化，并能从旧 schema 数据 backfill。
- repository roundtrip 覆盖 create / update / archive / list_by_root_lifecycle_run / list_by_story_projection。
- `pnpm run migration:guard` 通过。
- `StoryRepository::find_by_task_id` 的旧 JSONB containment 路径被替代。

#### Phase 4: Application command boundary

目标：把 create/update/assign/fanout/review/done 变成明确 plan commands，而不是 Story aggregate 或 runtime status 的副作用。

工作：

- 定义 `create_tasks_from_plan`、`assign_tasks`、`fanout_tasks`、`submit_task_review`、`confirm_task_done` 等应用命令的真实接口。
- 单个 Task assign 与 dynamic workflow fanout 共用 assignment link。
- Subagent 只能提交 review，owner/human/owner capability 确认 done。
- policy hook 默认开放，但有稳定调用点。

验收信号：

- 单个 assign 和 fanout 创建相同类型的 Task -> run / agent association。
- Task 状态命令不从 RuntimeSession / artifacts / node status 推断 plan status。
- permission hook 存在且默认开放；后续 PermissionGrant 可接管。

#### Phase 5: Frontend projection split

目标：前端按事实源分层消费 Task plan facts 与 execution projection。

工作：

- Story 页面展示 Story Task projection，并标注来源关系。
- AgentRun workspace 增加 Task / Todo 面板，作为 run-tree Task 一等入口。
- storyStore 只缓存 Story / Story Task projection；lifecycleStore 缓存 execution projection；必要时新增 task plan store。
- TaskDrawer 移除对 `task.artifacts` 和旧 dispatch preference 的直接依赖，改用 linked runs / SubjectExecutionView / assignment command。

验收信号：

- Story 页面能解释每个 Task 为什么属于该 Story。
- AgentRun workspace 能创建、推进、归档 Task。
- TaskDrawer 中执行产物只来自 SubjectExecutionView / linked artifacts。
- 前端 typecheck / focused component tests 通过。

#### Phase 6: MCP / capability / workflow fanout closure

目标：收口 agent-facing 工具和 workflow 数据源，避免旧 MCP 状态推进继续写旧事实源。

工作：

- 更新 Task MCP 工具 schema，让状态推进走 Task plan command，artifact 上报走 lifecycle / linked artifacts。
- 更新 Story MCP 工具，只创建 Story 显式关联 Task 或 Story projection，不写 Story aggregate child Task。
- dynamic workflow selector 从 root LifecycleRun / Story projection 读取 Task 集合，fanout 写 assignment link。
- agent-facing skill / docs 同步工具语义。

验收信号：

- `update_task_status` 不再接受或写旧执行态。
- artifact_count / dispatch_preference 不再作为 Task MCP 输出的事实核心。
- workflow fanout 能通过 Task selector 创建 child AgentRun，并通过 linked runs 观察执行事实。

#### Phase 7: Cleanup and spec finish

目标：移除旧字段、旧 endpoint、旧 UI 依赖，完成长期文档收束。

工作：

- 删除或归档 `stories.tasks` 读写、旧 `/tasks/{id}/execution` DTO、旧 Task artifacts 字段、旧 dispatch_preference 语义。
- 更新 specs，只记录新边界为什么成立，不记录旧实现细节。
- 收敛 task docs / research / acceptance criteria。

验收信号：

- repo 搜索旧状态和旧字段只在 migration tests / historical docs 中出现。
- `cargo check --workspace`、contract check、frontend check、focused backend/frontend tests 通过。
- `.trellis/spec` 与代码事实一致。

### 当前文档应更新清单

- `prd.md`
  - 增加 “本任务是否执行真实迁移还是产出迁移规划” 的明确边界。
  - 增加 acceptance criteria：migration、contract generation、frontend projection split、MCP/capability closure 是否属于本任务。
  - 不需要记录旧实现细节，只说明为什么 Task durable facts 需要跟随 run tree，而 Story 只消费 projection。

- `design.md`
  - 将 Task 物理仓储形态从 “待后续细化” 提前为 design decision。
  - 增加旧状态到新状态、artifacts、dispatch_preference、Story link 的迁移表。
  - 增加 wire contract / generated type / frontend store / MCP tool / permission hook 小节。
  - 明确 `SubjectExecutionView` 与 `/tasks/{id}/execution` 的取舍。

- `implement.md`
  - 将当前 8 个概念阶段拆成可执行阶段：spec gate、read model first、status+contract、schema migration、command boundary、frontend split、MCP/capability、cleanup。
  - 每阶段加入具体 touched areas、验证命令、rollback/checkpoint。
  - 增加 focused tests 和 migration guard / contracts check。
  - 将 spec 更新从最后阶段部分前移，至少先解除旧 spec 与目标模型冲突。

- `implement.jsonl`
  - 建议补充 `.trellis/spec/backend/database-guidelines.md`、`.trellis/spec/backend/repository-pattern.md`、`.trellis/spec/frontend/type-safety.md`、`.trellis/spec/frontend/state-management.md`、`.trellis/spec/cross-layer/architecture.md`、本研究文件。

- `check.jsonl`
  - 建议补充 `.trellis/spec/cross-layer/frontend-backend-contracts.md`、`.trellis/spec/backend/database-guidelines.md`、`.trellis/spec/frontend/type-safety.md`、`.trellis/spec/frontend/state-management.md`、本研究文件。

- `.trellis/spec/backend/story-task-runtime.md`
  - 应更新为目标边界：Task durable facts 属于 AgentRun / LifecycleRun 控制树内 Todo facts；Story 只通过显式关联和 run tree projection 看到 Task。

- `.trellis/spec/backend/repository-pattern.md`
  - 如果实现新增 TaskRepository 或 TaskCommandPort，应记录为什么新的 repository / command port 对应新的聚合边界和跨聚合一致性需求。

- `.trellis/spec/cross-layer/frontend-backend-contracts.md`
  - 如新增 Task plan / projection DTO，应记录 contract crate 与 generated TS 的归属原因。

- `.trellis/spec/frontend/state-management.md`
  - 如引入 Task plan store 或重新划分 storyStore / lifecycleStore，应记录 Story projection 与 execution projection 分层原因。

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` 返回 `Current task: (none)`，因此当前 active task path 来自用户明确指定的 `.trellis/tasks/06-16-story-task-subject-model-cleanup`。本文件按该路径写入。
- 本次只读任务文档、spec 和代码；没有修改业务代码、没有修改 spec、没有运行测试、没有运行 git 命令。
- 没有使用外部资料；External references 为 none。判断基于仓库代码、Trellis specs 和 06-14 既有 research。
- 由于本任务仍处于 planning，以上是 planning consistency review，不是迁移实现审查。
