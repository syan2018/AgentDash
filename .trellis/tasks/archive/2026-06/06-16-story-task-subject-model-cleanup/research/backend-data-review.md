# Research: backend-data-review

- Query: 根据项目实际代码审查 Story/Task/Subject 模型清理任务在后端和数据库层的迁移可行性。
- Scope: internal
- Date: 2026-06-16

## 关键文件实际检查清单

- `.trellis/tasks/06-16-story-task-subject-model-cleanup/prd.md` — 本任务的产品边界：Story 是 subject/context，Task 是 LifecycleRun 控制树内 Todo facts / plan item。
- `.trellis/tasks/06-16-story-task-subject-model-cleanup/design.md` — 目标模型、推荐 Task 字段、assignment/link、SubjectExecutionView 收口方向。
- `.trellis/tasks/06-16-story-task-subject-model-cleanup/implement.md` — 分阶段实现计划、风险文件、验证命令。
- `.trellis/workflow.md` — Trellis research artifact 规则、任务阶段和持久化要求。
- `.trellis/spec/backend/index.md` — 后端 spec 入口。
- `.trellis/spec/backend/architecture.md` — 后端分层、业务数据与 runtime 事实源边界。
- `.trellis/spec/backend/directory-structure.md` — crate 分层与新增模块路径约定。
- `.trellis/spec/backend/database-guidelines.md` — migration 事实源、删除旧列、migration guard 规则。
- `.trellis/spec/backend/repository-pattern.md` — Repository trait 与聚合边界规则；当前仍记录 Story aggregate 持有 Task。
- `.trellis/spec/backend/domain-payload-typing.md` — JSON payload 类型化原则。
- `.trellis/spec/backend/story-task-runtime.md` — 当前长期 spec；仍定义 Task 为 Story child entity，是本任务需要更新的冲突 spec。
- `.trellis/spec/backend/workflow/lifecycle-run-link.md` — `LifecycleSubjectAssociation` / `SubjectRef` 关联层目标契约。
- `.trellis/spec/cross-layer/frontend-backend-contracts.md` — Rust contract -> generated TypeScript 的跨层 DTO 契约。
- `crates/agentdash-infrastructure/migrations/0001_init.sql` — 当前数据库基线；`stories.tasks`、`lifecycle_subject_associations`、`runtime_session_execution_anchors` 的物理 schema。
- `crates/agentdash-infrastructure/src/persistence/postgres/story_repository.rs` — Story/Task 当前 PostgreSQL 持久化事实源。
- `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs` — `LifecycleSubjectAssociationRepository` 的 PostgreSQL 实现。
- `crates/agentdash-domain/src/story/entity.rs` — Story aggregate 持有 `Vec<Task>` 的领域事实。
- `crates/agentdash-domain/src/story/repository.rs` — StoryRepository 暴露 task CRUD / `find_by_task_id`。
- `crates/agentdash-domain/src/story/value_objects.rs` — Story 状态和 `StateChange` kind。
- `crates/agentdash-domain/src/task/entity.rs` — Task 当前字段、status/artifacts 投影字段、`TaskSpecMut`。
- `crates/agentdash-domain/src/task/value_objects.rs` — 当前 TaskStatus、TaskExecutionProjection、TaskDispatchPreference、Artifact。
- `crates/agentdash-contracts/src/task/contract.rs` — browser-facing Task DTO 与 generated TS 事实源。
- `crates/agentdash-contracts/src/story/contract.rs` — browser-facing Story DTO。
- `crates/agentdash-contracts/src/runtime/workflow.rs` — `LifecycleSubjectAssociationDto` 与 `SubjectExecutionView` wire contract。
- `crates/agentdash-api/src/dto/story.rs` — Story/Task create/update route-local request DTO。
- `crates/agentdash-api/src/dto/task_execution.rs` — Task 专属 execution response DTO。
- `crates/agentdash-api/src/routes/stories.rs` — Story/Task HTTP CRUD route。
- `crates/agentdash-api/src/routes/task_execution.rs` — `/tasks/{id}/execution` Task 专属 execution route。
- `crates/agentdash-api/src/routes/lifecycle_views.rs` — 通用 `/subjects/{kind}/{id}/execution` route。
- `crates/agentdash-api/src/routes/story_runs.rs` — Story runs route 已收口到 `SubjectExecutionView`。
- `crates/agentdash-api/src/routes/project_agents.rs` — ProjectAgent start request 接收 `subject_ref`。
- `crates/agentdash-application/src/lifecycle/subject_context_assignment.rs` — Story/Task subject context 解析链。
- `crates/agentdash-application/src/lifecycle/dispatch_service.rs` — dispatch 时创建 subject association。
- `crates/agentdash-application/src/lifecycle/run_view_builder.rs` — `SubjectExecutionView` read model builder。
- `crates/agentdash-application/src/lifecycle/subject_execution_control.rs` — subject execution cancel 控制面。
- `crates/agentdash-application/src/task/service.rs` — Task execution view 旧 read projection service。
- `crates/agentdash-application/src/task/view_projector.rs` — 启动期把 lifecycle runtime node 状态反投影到 Story.tasks。
- `crates/agentdash-application/src/task/gateway/repo_ops.rs` — Task 状态/变更当前仍经 Story aggregate 查询。
- `crates/agentdash-application/src/task/gateway/effect_executor.rs` — hook effect `task:set_status` 和 runtime anchor 校验。
- `crates/agentdash-application/src/task/gateway/artifact_ops.rs` — Task artifact 当前写回 Story aggregate。
- `crates/agentdash-application/src/task/config.rs` — `Task.dispatch_preference` 到 AgentConfig 的 dispatch-time 解析。
- `crates/agentdash-application/src/agent_run/frame/construction/composer_project_agent.rs` — ProjectAgent frame composition 叠加 Story/Task subject profile。
- `crates/agentdash-application/src/agent_run/frame/construction/owner_bootstrap.rs` — owner/subject context 与 capability scope 组合。
- `crates/agentdash-application/scripts/hook-presets/task_session_terminal.rhai` — runtime terminal hook 产生旧 TaskStatus。
- `crates/agentdash-mcp/src/servers/story.rs` — Story MCP 创建/list Task 与推进 Story 状态。
- `crates/agentdash-mcp/src/servers/task.rs` — Task MCP 读取、状态意图、artifact 上报。
- `packages/app-web/src/generated/task-contracts.ts` — 当前前端生成 Task wire 类型，暴露旧状态与 artifacts。
- `packages/app-web/src/generated/workflow-contracts.ts` — 当前前端生成 SubjectExecutionView。
- `packages/app-web/src/services/story.ts` — 前端 service 仍校验旧 TaskStatus 与 artifacts。
- `packages/app-web/src/services/lifecycle.ts` — 前端已使用通用 subject execution endpoint。
- `packages/app-web/src/features/task/task-drawer.tsx` — TaskDrawer 同时读取 Task.artifacts 和 SubjectExecution panel。

## Findings

### 当前模型事实

**Story**

Story 当前是 Project 下的业务主题和上下文容器，持有 title、description、status、priority、type、tags、default workspace 和 `StoryContext`。`StoryContext` 包含 `source_refs`、`context_containers`、`disabled_container_ids`、`session_composition`，与目标中的 subject/context container 职责一致（`crates/agentdash-domain/src/story/value_objects.rs:47`）。

Story 状态仍是产品流程状态：`created / context_ready / decomposed / executing / completed / failed / cancelled`（`crates/agentdash-domain/src/story/value_objects.rs:13`）。HTTP create/update 可直接设置或更新 Story 状态（`crates/agentdash-api/src/dto/story.rs:14`, `crates/agentdash-api/src/dto/story.rs:29`），MCP 也有 `advance_story_status` 工具直接推进 Story 状态（`crates/agentdash-mcp/src/servers/story.rs:529`）。这符合“Story 状态通过明确 Story command 推进”的大方向，但 command/capability 需要在后续收口。

当前代码里 Story 仍是 Task 的聚合根：`Story` 明确声明持有 `Vec<Task>`，物理持久化为 `stories.tasks JSONB`，mutation 走 `StoryRepository::update` 整体写回（`crates/agentdash-domain/src/story/entity.rs:14`, `crates/agentdash-domain/src/story/entity.rs:35`）。这与本任务目标“Story 只通过显式 link / run tree projection 看 Task，而不是拥有 Task domain”冲突。

**Task**

Task 当前是 Story aggregate 下 child entity，而不是独立表或独立 repository。数据库基线只有 `stories.tasks jsonb DEFAULT '[]'::jsonb NOT NULL`（`crates/agentdash-infrastructure/migrations/0001_init.sql:720`, `crates/agentdash-infrastructure/migrations/0001_init.sql:734`）。`StoryRepository::find_by_task_id` 用 JSONB containment 从 `stories.tasks` 反查 Story（`crates/agentdash-infrastructure/src/persistence/postgres/story_repository.rs:139`）。

Task entity 当前字段包括 `project_id`、`story_id`、`workspace_id`、title、description、`status`、`dispatch_preference`、`artifacts`、created/updated（`crates/agentdash-domain/src/task/entity.rs:24`）。其中 `status` 和 `artifacts` 被注释为 LifecycleRun / LifecycleStepState 的只读投影字段（`crates/agentdash-domain/src/task/entity.rs:16`），但它们实际仍序列化进 `stories.tasks`，并通过 `TaskResponse` 暴露给前端（`crates/agentdash-contracts/src/task/contract.rs:101`）。

TaskStatus 当前是执行态语言：`pending / assigned / running / awaiting_verification / completed / failed / cancelled`（`crates/agentdash-domain/src/task/value_objects.rs:12`）。`TaskExecutionProjection::from_runtime_node_status` 直接把 runtime node 状态映射到 TaskStatus，例如 `Completed -> AwaitingVerification`、`Failed -> Failed`、`Cancelled -> Cancelled`（`crates/agentdash-domain/src/task/value_objects.rs:49`）。这与目标计划态语言 `open / active / review / done / blocked / dropped` 不一致。

TaskDispatchPreference 当前保存在 Task spec 上，包含 `agent_type`、`agent_pid`、`preset_name`、`prompt_template`、`initial_context`、`context_sources`（`crates/agentdash-domain/src/task/value_objects.rs:73`）。应用层把它作为 dispatch-time resolver 输入（`crates/agentdash-application/src/task/config.rs:1`），HTTP create 还要求能解析出 Agent 类型或 preset/default，否则拒绝创建 Task（`crates/agentdash-api/src/routes/stories.rs:304`, `crates/agentdash-api/src/routes/stories.rs:332`）。这比目标模型中的 Todo facts / plan item 更偏“可执行任务配置”，需要拆成 context source、launch hint 或 assign/fanout command 参数。

**Subject / execution facts**

项目已有统一 subject 关联层：`lifecycle_subject_associations` 表含 `anchor_run_id`、`anchor_agent_id`、`subject_kind`、`subject_id`、`role`、`metadata_json`（`crates/agentdash-infrastructure/migrations/0001_init.sql:294`），并有 `idx_lsa_subject(subject_kind, subject_id)` 索引（`crates/agentdash-infrastructure/migrations/0001_init.sql:1094`）。

Dispatch 会在存在 `subject_ref` 时创建 `LifecycleSubjectAssociation`；当前 `task` 和 `story` 都被 agent-scoped 绑定，其它 subject run-scoped（`crates/agentdash-application/src/lifecycle/dispatch_service.rs:573`）。ProjectAgent start route 已接收 `subject_ref`（`crates/agentdash-api/src/routes/project_agents.rs:170`），frame construction 会通过 association 找到 Story/Task subject profile 并叠加 context/capability（`crates/agentdash-application/src/agent_run/frame/construction/composer_project_agent.rs:158`）。

`SubjectContextAssignmentResolver` 已支持 `project / story / task` 三类 subject（`crates/agentdash-application/src/lifecycle/subject_context_assignment.rs:58`）。Story subject 解析 Story context、Project、workspace 和 `CapabilityScopeCtx::Story`（`crates/agentdash-application/src/lifecycle/subject_context_assignment.rs:110`）；Task subject 先通过 Story aggregate 加载 Task，再加载 parent Story、workspace、Story sources + Task context sources，并返回 `CapabilityScopeCtx::Task`（`crates/agentdash-application/src/lifecycle/subject_context_assignment.rs:183`）。

通用 SubjectExecutionView 已存在。后端 route 是 `/subjects/{kind}/{id}/execution`（`crates/agentdash-api/src/routes/lifecycle_views.rs:37`），read model 会按 subject association 找 runs、current agent、latest runtime node 和 artifacts（`crates/agentdash-application/src/lifecycle/run_view_builder.rs:228`）。contract 也已生成 `SubjectExecutionView`，包含 `subject_ref`、associations、runs、current_agent、latest_runtime_node、artifacts（`crates/agentdash-contracts/src/runtime/workflow.rs:1277`）。

Story runs route 已经返回 `SubjectExecutionView`，不是旧 StoryRunOverview shape（`crates/agentdash-api/src/routes/story_runs.rs:33`）。Task 仍有旧的 `/tasks/{id}/execution` 专属紧凑 DTO，返回 `execution_status`、agent/run/frame refs、`task_status`（`crates/agentdash-api/src/dto/task_execution.rs:6`），由 `StoryActivityActivationService` 从 subject association + runtime anchors 派生（`crates/agentdash-application/src/task/service.rs:19`）。

**Runtime facts**

RuntimeSession 到 run/agent/frame/node 的索引已由 `runtime_session_execution_anchors` 表表达（`crates/agentdash-infrastructure/migrations/0001_init.sql:533`）。`SubjectExecutionView` 的 runtime projection 从 association 找 agent，再从 execution anchor 找 `LifecycleRun.orchestrations[].node_tree` 和 artifacts（`crates/agentdash-application/src/lifecycle/run_view_builder.rs:326`, `crates/agentdash-application/src/lifecycle/run_view_builder.rs:651`）。

Task 启动期 projector 会遍历 `project -> stories -> story.tasks`，对每个 Task 用 `SubjectRef(kind=task)` 查 association，再把 runtime node 状态写回 `Story.tasks[i].status`（`crates/agentdash-application/src/task/view_projector.rs:100`, `crates/agentdash-application/src/task/view_projector.rs:140`）。这说明当前同时存在“runtime truth 在 LifecycleRun”和“Task status 持久投影在 Story JSONB”的重复事实源。

### 与目标模型冲突点（按风险排序）

1. **P0：Task 物理事实源仍在 Story JSONB，目标要求 Task durable facts 脱离 Story aggregate。**
   证据：`stories.tasks` 是唯一物理 Task 存储（`crates/agentdash-infrastructure/migrations/0001_init.sql:734`）；`StoryRepository` 创建/更新都整体写入 `tasks` JSONB（`crates/agentdash-infrastructure/src/persistence/postgres/story_repository.rs:30`, `crates/agentdash-infrastructure/src/persistence/postgres/story_repository.rs:400`）；API list/create/update/delete task 都通过 Story aggregate 路径（`crates/agentdash-api/src/routes/stories.rs:233`, `crates/agentdash-api/src/routes/stories.rs:347`, `crates/agentdash-api/src/routes/stories.rs:449`, `crates/agentdash-api/src/routes/stories.rs:499`）。
   风险：如果先改状态/DTO 而不先拆存储，后续 root LifecycleRun / owner AgentRun / assigned AgentRun 查询仍无法表达，Story projection 也仍然不可解释。

2. **P0：TaskStatus 仍是 runtime execution 语言，目标要求 plan/Todo 语言。**
   证据：当前状态为 `Pending/Assigned/Running/AwaitingVerification/Completed/Failed/Cancelled`（`crates/agentdash-domain/src/task/value_objects.rs:12`），runtime node 到 TaskStatus 的映射直接写回 Task（`crates/agentdash-domain/src/task/value_objects.rs:49`），hook preset 也发出 `awaiting_verification` / `failed`（`crates/agentdash-application/scripts/hook-presets/task_session_terminal.rhai:18`）。
   风险：plan 状态与 runtime 状态混用会让 `done/blocked/dropped/review` 的语义无法稳定，subagent review gate 和 owner 确认也无从表达。

3. **P0：Task artifacts 有双事实源。**
   证据：Task entity 持有 `artifacts`（`crates/agentdash-domain/src/task/entity.rs:39`）；TaskResponse 直接暴露 `artifacts`（`crates/agentdash-contracts/src/task/contract.rs:111`）；artifact helper 将 tool/runtime artifact 写回 Story aggregate（`crates/agentdash-application/src/task/gateway/artifact_ops.rs:1`, `crates/agentdash-application/src/task/gateway/artifact_ops.rs:124`）；同时 `SubjectExecutionView.artifacts` 已从 lifecycle orchestration state 派生（`crates/agentdash-application/src/lifecycle/run_view_builder.rs:246`, `crates/agentdash-contracts/src/runtime/workflow.rs:1290`）。
   风险：同一执行产物既在 Task JSONB 内，又在 lifecycle projection 内，迁移后容易出现 UI、MCP、state_change 各读一份。

4. **P1：Task 专属 execution DTO 与通用 SubjectExecutionView 重叠。**
   证据：通用 subject endpoint 已存在（`crates/agentdash-api/src/routes/lifecycle_views.rs:43`），Task 专属 `/tasks/{id}/execution` 仍返回紧凑 DTO（`crates/agentdash-api/src/routes/task_execution.rs:18`, `crates/agentdash-api/src/dto/task_execution.rs:8`）。前端 TaskSubjectExecutionPanel 已使用通用 `fetchSubjectExecution("task", task.id)`（`packages/app-web/src/services/lifecycle.ts:27`, `packages/app-web/src/features/task/task-subject-execution-panel.tsx:120`）。
   风险：保留 Task 专属 DTO 会继续暗示 Task 有自己的 execution surface，而目标是 subject-oriented linked runs projection。

5. **P1：`dispatch_preference` 把创建 Task 与选择执行器耦合，目标要求 assignment / launch hint / command 边界。**
   证据：TaskDispatchPreference 包含 agent config、prompt、initial_context、context_sources（`crates/agentdash-domain/src/task/value_objects.rs:73`）；HTTP create Task 没有 agent_type/preset/default 会失败（`crates/agentdash-api/src/routes/stories.rs:328`, `crates/agentdash-api/src/routes/stories.rs:332`）；config resolver 明确把它消费成 AgentConfig（`crates/agentdash-application/src/task/config.rs:64`）。
   风险：无法支持“任意 AgentRun 创建局部 Todo facts”这种轻量 plan item；Task create 会被 executor 配置阻塞。

6. **P1：Task 查询和权限加载依赖 parent Story，目标要求 run tree / explicit link projection。**
   证据：API auth `load_task_story_project_with_permission` 通过 `story_repo.find_by_task_id` 加载 Task/Story/Project；Task MCP、subject context、task gateway 也都通过 Story aggregate 加载 Task（`crates/agentdash-mcp/src/servers/task.rs:92`, `crates/agentdash-application/src/task/gateway/repo_ops.rs:24`, `crates/agentdash-application/src/lifecycle/subject_context_assignment.rs:188`）。
   风险：Task 一旦从 Story JSONB 拆出，如果不先给 TaskRepository/TaskLink/StoryProjection 查询补齐，权限、context、MCP、execution view 都会断。

7. **P1：MCP 仍暴露 Story/Task 独立状态与 artifact 写入口。**
   证据：Story MCP 可以创建 Task、list Story 下所有 Task，并直接推进 Story 状态（`crates/agentdash-mcp/src/servers/story.rs:383`, `crates/agentdash-mcp/src/servers/story.rs:503`, `crates/agentdash-mcp/src/servers/story.rs:529`）；Task MCP `update_task_status` 目前只 append 变更意图，不实际更新 Task，但仍使用旧 TaskStatus parser（`crates/agentdash-mcp/src/servers/task.rs:155`）；`report_artifact` 直接写 Story aggregate（`crates/agentdash-mcp/src/servers/task.rs:199`, `crates/agentdash-mcp/src/servers/task.rs:224`）。
   风险：模型迁移后 MCP 工具可能绕过新的 Task command / SubjectExecutionView / artifact link。

8. **P2：长期 spec 与本任务目标相反，需要后续更新。**
   证据：`.trellis/spec/backend/story-task-runtime.md` 仍写“Task 是 Story aggregate 下 child entity，保存在 stories.tasks JSONB；无独立 repository、无独立表”（`.trellis/spec/backend/story-task-runtime.md:10`）；repository pattern 也仍写 Story aggregate 的 Task 变更走 `StoryRepository::update`（`.trellis/spec/backend/repository-pattern.md:32`）。
   风险：implement/check subagent 若只按旧 spec 会回到旧模型。

9. **P2：migration 策略需要区分“不兼容旧模型”和“不要改历史 migration”。**
   证据：数据库 spec 要求业务 schema 事实源是 `crates/agentdash-infrastructure/migrations/`，普通任务只能新增 migration，不能修改已提交 `0001_init.sql`；删除旧列用新 migration `DROP COLUMN IF EXISTS`（`.trellis/spec/backend/database-guidelines.md`）。
   风险：项目未上线意味着可以直接迁到正确模型、不保留旧 API/列读写，但不等于普通任务可以重写 migration 历史。若要 squash baseline，需要任务文档显式授权。

### 后端 / 数据库迁移建议顺序

1. **先定 Task 物理模型和状态枚举，然后新增 migration。**
   建议新增独立 `tasks` 表，让 Task 成为可按 run tree 查询的 durable facts，而不是 Story JSONB child。最小列建议来自 design：`id`、`project_id`、`root_lifecycle_run_id`、`owner_agent_run_id`、`created_by_agent_run_id`、`title`、`body`/`description`、`status`、`priority`、`story_link` 或 `story_id`、`assigned_agent_run_id`、`source_task_id`、`created_at`、`updated_at`、`archived_at`。
   第一版可以保留 `story_id` 作为显式 Story link 的外键/可空列，但语义应是 projection link，不是 ownership。为查询补索引：`project_id`、`root_lifecycle_run_id`、`owner_agent_run_id`、`created_by_agent_run_id`、`assigned_agent_run_id`、`story_id`、`status`、`archived_at`。

2. **在 migration 中做确定性回填，并删除旧 Story JSONB task 列。**
   普通任务按 spec 应新增 `0015_<description>.sql`，不要直接改 `0001_init.sql`。migration 可以从 `stories.tasks` JSONB 展开回填到 `tasks` 表，同时给旧状态确定性映射：`pending -> open`，`assigned/running -> active`，`awaiting_verification -> review`，`completed -> done`，`failed -> blocked`，`cancelled -> dropped`。由于项目未上线，不需要保留旧状态读取、双写或回退列。
   回填后 `ALTER TABLE stories DROP COLUMN IF EXISTS tasks;`。`task_count` 如果仍是 Story projection，可以在第一版保留并由 Story projection 查询更新；如果不需要作为持久字段，也应同时 `DROP COLUMN IF EXISTS task_count`，让 Story count 从 projection/read model 计算。

3. **新增/恢复 `TaskRepository`，从 `StoryRepository` 拆除 task CRUD。**
   Domain 层把 Task 从 Story child entity 迁为独立 plan item aggregate/record，RepositorySet 增加 `task_repo`。`StoryRepository` 保留 Story CRUD，不再有 `find_by_task_id`、`add_task_to_story`、`remove_task_from_story`。PostgresStoryRepository 去掉 `tasks_to_json`、`merge_task_snapshots`、JSONB containment 查询和 artifact merge。
   `load_task_story_project_with_permission` 改成 `TaskRepository::get_by_id` 加载 Task，再按 `project_id` 授权；Story 只在 `story_link/story_id` 存在时加载，用于 Story context/projection。

4. **重写应用层 Task use case 到 run-scoped 查询。**
   `create_task` 不应再要求 agent_type/preset/default；它只创建 Todo facts。新增查询应覆盖：当前 AgentRun 创建/拥有/派发/被派发 Task、root LifecycleRun 下 Task、Story projection Task。Story projection 第一版以显式 `story_id/story_link` + subject association/run tree 为准，不做文本猜测。
   `SubjectContextAssignmentResolver::resolve_task` 改从 TaskRepository 加载 Task；parent Story context 只在 Task 显式 link 到 Story 时叠加。`Task.dispatch_preference.context_sources` 可迁为 Task 的 context refs；agent_type/preset/prompt_template/initial_context 应迁到 assignment/launch command 参数或 launch hint，不再阻塞 Task 创建。

5. **重做 TaskStatus 和状态推进 command。**
   Domain/contract/generated TS 统一改为 `open / active / review / done / blocked / dropped`。删除 `TaskExecutionProjection::from_runtime_node_status` 对 plan status 的直接写回，或改成只生成 read projection，不写 Task status。`task_session_terminal.rhai` 和 `task:set_status` effect 应改为新计划态：subagent completed 可提交 `review`，owner/self-managed 可显式推进 `done`；runtime failed/cancelled 只作为 linked run execution fact，是否把 Todo 标为 `blocked/dropped` 应由 Task command/policy 决定。

6. **把 artifacts 从 Task 实体和 TaskResponse 移到 execution projection / linked artifacts。**
   删除 Task entity 的 `artifacts` 字段、`Artifact` DTO 和 TaskResponse.artifacts，或者将其降为 read-only projection 不持久在 `tasks` 主表。`TaskSubjectExecutionPanel` 应只读 `SubjectExecutionView.artifacts`。MCP `report_artifact` 不再 `Story::push_task_artifact`，应改为写 lifecycle artifact / linked artifact / state_change projection；如果对应写入端口尚未完成，预研期可以先移除该工具入口，避免伪事实源。

7. **收口 API DTO 和 routes。**
   更新 `agentdash-contracts::task`：TaskResponse 去掉旧 artifacts，状态改为计划态，字段从 `story_id` 改为可解释 link（如 `story_id?: string` 或 `story_link`），新增 `root_lifecycle_run_id`、owner/created/assigned run refs。
   删除或替换 `/tasks/{id}/execution` 专属 DTO。由于前端已存在通用 `/subjects/{kind}/{id}/execution`，推荐直接删除 Task 专属 execution route，TaskDrawer/TaskSubjectExecutionPanel 继续使用 SubjectExecutionView。项目未上线，不需要保留旧 endpoint 或兼容旧 response。

8. **收口 MCP / capability。**
   Story MCP 可以保留 Story context、Story command、Story projection 查询，但 Task 创建应调用新的 Task command，并写显式 Story link 而非 Story aggregate child。Task MCP 的 `update_task_status` 改为新 plan status command；`report_artifact` 改走 linked execution artifact。Story/Task 工具可见性继续从 `CapabilityScopeCtx::Story/Task` 或后续 permission grant 派生。

9. **更新 tests / validation。**
   必跑：`pnpm run migration:guard`、clean database migration/bootstrap readiness、`cargo check --workspace`、`pnpm run contracts:check`、`pnpm typecheck`。
   重点补测试：migration 从 `stories.tasks` 回填到 `tasks` 并 drop 旧列；TaskRepository create/update/list by root run/list by owner/list by Story link；TaskStatus old->new 映射；StoryRepository 不再读写 tasks；`SubjectContextAssignmentResolver` 对 story-linked task 和 non-story task 的 context 行为；`/subjects/task/{id}/execution` authorization；Task API create 不再要求 agent type；MCP Task status command 只接受新计划态；Task artifacts 只来自 SubjectExecutionView。

### 不需要兼容 / 回退的地方

- 不需要保留旧 TaskStatus wire 值。迁移后 contract、generated TS、前端状态 badge、MCP parser 可以一次性切到 `open / active / review / done / blocked / dropped`。
- 不需要让 repository 双读 `stories.tasks` 和新 `tasks` 表。migration 回填后，主线只读写新表；旧列可以 drop。
- 不需要保留 `/tasks/{id}/execution` 的旧紧凑 DTO。如果仍需要同路径入口，也应直接返回 `SubjectExecutionView`；更推荐删除旧 route 并统一使用 `/subjects/task/{id}/execution`。
- 不需要保留 `TaskResponse.artifacts`。执行产物统一从 lifecycle / SubjectExecutionView projection 读取。
- 不需要保留 Task create 时的 `agent_type` 必填逻辑。Task create 是 Todo facts 创建；assign/fanout/launch command 才处理 executor/preset/prompt。
- 不需要为旧前端手写 mapper 做字段兼容。Rust contracts 更新后重新生成 TypeScript，并同步前端使用 generated types。
- 不需要设计 runtime fallback：Task 不表达 running truth，运行事实只从 LifecycleRun / LifecycleAgent / AgentFrame / RuntimeSessionExecutionAnchor / RuntimeNodeState 查询。

## Code Patterns

- Story 当前是 Task ownership aggregate：`Story` 持有 `Vec<Task>`，并注释为 `stories.tasks JSONB` 持久化（`crates/agentdash-domain/src/story/entity.rs:14`, `crates/agentdash-domain/src/story/entity.rs:35`）。
- Task 当前仍有 parent Story 外键语义：`Task` 字段包含 `story_id`、`workspace_id`、`status`、`dispatch_preference`、`artifacts`（`crates/agentdash-domain/src/task/entity.rs:24`）。
- Task 状态从 runtime node 映射而来：`RuntimeNodeStatus::Completed -> TaskStatus::AwaitingVerification`（`crates/agentdash-domain/src/task/value_objects.rs:49`）。
- PostgreSQL schema 当前没有独立 `tasks` 表，只有 `stories.tasks` JSONB（`crates/agentdash-infrastructure/migrations/0001_init.sql:720`）。
- Task 查询当前通过 JSONB containment：`WHERE tasks @> jsonb_build_array(jsonb_build_object('id', $1::text))`（`crates/agentdash-infrastructure/src/persistence/postgres/story_repository.rs:139`）。
- StoryRepository 更新会 merge task snapshots，并合并 artifacts（`crates/agentdash-infrastructure/src/persistence/postgres/story_repository.rs:98`, `crates/agentdash-infrastructure/src/persistence/postgres/story_repository.rs:435`）。
- Task HTTP CRUD route 仍是 Story-scoped create/list 和 global `/tasks/{id}` update/get/delete（`crates/agentdash-api/src/routes/stories.rs:46`）。
- Task create 当前根据 `dispatch_preference`/Project default 解析 agent type，否则拒绝（`crates/agentdash-api/src/routes/stories.rs:304`, `crates/agentdash-api/src/routes/stories.rs:332`）。
- 通用 subject execution endpoint 已存在：`/subjects/{kind}/{id}/execution`（`crates/agentdash-api/src/routes/lifecycle_views.rs:43`）。
- Task 专属 execution DTO 仍存在，返回 `execution_status` 和 `task_status`（`crates/agentdash-api/src/dto/task_execution.rs:8`）。
- Lifecycle dispatch 会为 Story/Task subject 创建 agent-scoped association（`crates/agentdash-application/src/lifecycle/dispatch_service.rs:573`）。
- ProjectAgent frame construction 会从 association 找到非 project subject 并解析 subject context（`crates/agentdash-application/src/agent_run/frame/construction/composer_project_agent.rs:177`）。
- SubjectExecutionView builder 从 association、run、agent、execution anchor、runtime node 组合 linked runs 与 artifacts（`crates/agentdash-application/src/lifecycle/run_view_builder.rs:228`）。
- Task boot projector 当前会把 lifecycle runtime node 状态写回 Story.tasks（`crates/agentdash-application/src/task/view_projector.rs:100`）。
- Hook effect `task:set_status` 先校验 runtime session anchor 与 task association，再调用 `update_task_status`（`crates/agentdash-application/src/task/gateway/effect_executor.rs:135`, `crates/agentdash-application/src/task/gateway/effect_executor.rs:185`）。
- 但 `update_task_status` 当前只 append `TaskUpdated` state_change，并没有实际写 Task.status（`crates/agentdash-application/src/task/gateway/repo_ops.rs:55`）。
- Task artifacts 当前仍写回 Story aggregate（`crates/agentdash-application/src/task/gateway/artifact_ops.rs:124`）。
- Story MCP 仍直接在 Story 下创建/list Task（`crates/agentdash-mcp/src/servers/story.rs:383`, `crates/agentdash-mcp/src/servers/story.rs:503`）。
- Task MCP 仍通过 Story aggregate 加载 Task，并可 report artifact 到 Story aggregate（`crates/agentdash-mcp/src/servers/task.rs:92`, `crates/agentdash-mcp/src/servers/task.rs:199`）。
- Generated frontend Task contract 仍暴露旧状态和 `artifacts`（`packages/app-web/src/generated/task-contracts.ts:13`, `packages/app-web/src/generated/task-contracts.ts:15`）。

## External References

- 未查询外部文档；本次为内部代码/spec/migration 可行性审查。
- 数据库判断依据项目本地 PostgreSQL + SQLx migration 规范：业务 schema 事实源是 `crates/agentdash-infrastructure/migrations/`，普通任务新增 migration；只有明确授权的 baseline squash/reset/merge 任务可改既有 migration。

## Related Specs

- `.trellis/spec/backend/architecture.md` — 后端分层与业务/runtime 事实源边界。
- `.trellis/spec/backend/database-guidelines.md` — migration 历史、删除旧列、guard、baseline squash 授权规则。
- `.trellis/spec/backend/repository-pattern.md` — 当前仍写 Story aggregate Task 规则；本任务完成后应更新。
- `.trellis/spec/backend/story-task-runtime.md` — 当前仍写旧模型；本任务完成后必须更新为 Story projection + Task run-tree facts。
- `.trellis/spec/backend/workflow/lifecycle-run-link.md` — `LifecycleSubjectAssociation` 目标关联层。
- `.trellis/spec/cross-layer/frontend-backend-contracts.md` — Rust contract / generated TS 同步规则。

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` 返回 `Current task: (none)`；本研究按用户显式提供的 `.trellis/tasks/06-16-story-task-subject-model-cleanup` 路径写入，没有猜测任务目录。
- 未发现独立 `tasks` 物理表，也未发现独立 `TaskRepository` trait；当前 Task 持久化事实源是 `stories.tasks` JSONB。
- 未运行测试、migration 或数据库命令；本次只读业务代码/spec/task 文档，仅写入本 research 文件。
- 未执行 git 命令。
- 未修改代码、migration、spec 或 task planning 文档。
- 如果主任务决定做 migration baseline squash，需要在 task design/implement 中显式授权；否则按现有 spec 应新增 migration 文件完成直接迁移。
