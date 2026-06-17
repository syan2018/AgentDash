# Research: runtime-task-toolset-code-paths

- Query: 调研 runtime tool/provider 注册链路、`LifecycleRun.tasks` 现有读写路径、AgentRun workspace / Story projection 前端 Task 展示入口，并给出新 `task_read` / `task_write` 最小接入建议。
- Scope: internal
- Date: 2026-06-17

## Findings

### Files Found

- `crates/agentdash-spi/src/connector/mod.rs` — 定义 `ToolCluster` 与 `RuntimeToolProvider` SPI。
- `crates/agentdash-spi/src/platform/tool_capability.rs` — capability key、cluster 工具名、platform tool descriptor、visibility rule 与 PlatformMcp scope 映射。
- `crates/agentdash-api/src/bootstrap/session.rs` — session runtime tool composer 的 provider 注册入口。
- `crates/agentdash-application/src/runtime_tools/provider.rs` — `SessionRuntimeToolComposer` 与上下文 helper。
- `crates/agentdash-application/src/session/tool_assembly.rs` — launch / runtime refresh 共用的工具装配函数。
- `crates/agentdash-application/src/lifecycle/tools/runtime_provider.rs` — cluster-based runtime tool provider 现有模式示例。
- `crates/agentdash-application/src/workspace_module/runtime_tool_provider.rs` — 多工具 cluster provider 与 capability gate 示例。
- `crates/agentdash-application/src/companion/runtime_tool_provider.rs` — collaboration cluster provider 示例。
- `crates/agentdash-application/src/task/plan.rs` — `LifecycleRun.tasks` application 读写 command 与 Story projection builder。
- `crates/agentdash-domain/src/workflow/value_objects/task_plan.rs` — Task plan value object / draft / patch / status。
- `crates/agentdash-domain/src/workflow/entity.rs` — `LifecycleRun.tasks` aggregate mutation。
- `crates/agentdash-api/src/routes/task_plan.rs` — browser-facing run / AgentRun Task plan API。
- `crates/agentdash-api/src/routes/stories.rs` — Story Task projection API。
- `crates/agentdash-contracts/src/task/contract.rs` — Task plan contract DTO。
- `packages/app-web/src/features/task/task-plan-panel.tsx` — AgentRun workspace Task panel。
- `packages/app-web/src/pages/AgentRunWorkspacePage.tsx` — AgentRun workspace 挂载 `TaskPlanPanel`。
- `packages/app-web/src/pages/StoryPage.tsx` — Story Task projection 展示入口。
- `crates/agentdash-mcp/src/servers/task.rs` / `crates/agentdash-mcp/src/servers/story.rs` — 旧 Platform MCP Task/Story 工具实现，按最新约束不应作为新 `task_read` / `task_write` 落点。

### Runtime Tool / Provider 注册链路

- `RuntimeToolProvider` 是 application 层提供工具的 SPI，`build_tools(&ExecutionContext)` 返回 `Vec<DynAgentTool>`：`crates/agentdash-spi/src/connector/mod.rs:684`。
- session bootstrap 当前注册 4 个 cluster-based provider：VFS、Workflow、Collaboration、WorkspaceModule，然后交给 `SessionRuntimeToolComposer`：`crates/agentdash-api/src/bootstrap/session.rs:254`、`crates/agentdash-api/src/bootstrap/session.rs:286`。
- `SessionRuntimeToolComposer` 只是顺序调用所有 provider 的 `build_tools` 并合并工具：`crates/agentdash-application/src/runtime_tools/provider.rs:63`、`crates/agentdash-application/src/runtime_tools/provider.rs:79`。
- launch 与 runtime refresh 都走同一个 `assemble_tools_for_execution_context`：`crates/agentdash-application/src/session/tool_assembly.rs:6`；launch deps 调用点在 `crates/agentdash-application/src/session/launch/deps.rs:183`，运行中 refresh 调用点在 `crates/agentdash-application/src/session/hub/tool_builder.rs:313`。
- cluster-based provider 模式是：先检查 `enabled_clusters`，再用 `is_capability_tool_enabled(capability, tool_name, Some(cluster))` 做单工具 gate。示例：Workflow provider `crates/agentdash-application/src/lifecycle/tools/runtime_provider.rs:37`；WorkspaceModule provider `crates/agentdash-application/src/workspace_module/runtime_tool_provider.rs:55`；Collaboration provider `crates/agentdash-application/src/companion/runtime_tool_provider.rs:30`。
- SPI 当前已声明新方向：`CAP_TASK = "task"`、`CLUSTER_TASK_TOOLS = ["task_read", "task_write"]`、`ToolCluster::Task`：`crates/agentdash-spi/src/platform/tool_capability.rs:86`、`crates/agentdash-spi/src/platform/tool_capability.rs:122`、`crates/agentdash-spi/src/connector/mod.rs:207`。
- `tool_capability` 已把 `task_read` / `task_write` 注册为 platform cluster 工具，而不是 Platform MCP 工具：`crates/agentdash-spi/src/platform/tool_capability.rs:333`。
- `CAP_TASK` 已映射到 `ToolCluster::Task`：`crates/agentdash-spi/src/platform/tool_capability.rs:542`；visibility rule 允许 Project / Story / Task scope 自动授予：`crates/agentdash-spi/src/platform/tool_capability.rs:741`。
- Platform MCP scope 当前只剩 Relay / Story / Workflow，`CAP_TASK` 不映射 MCP scope：`crates/agentdash-spi/src/platform/tool_capability.rs:561`、`crates/agentdash-spi/src/platform/tool_capability.rs:569`。这符合用户更新约束：新工具使用 cluster-based runtime tools。
- 缺口：`crates/agentdash-application/src/task` 下未发现 `TaskRuntimeToolProvider`、`task_read`、`task_write` 的 `AgentTool` 实现；`rg "TaskRuntimeToolProvider|task_read|task_write"` 在 application 层只命中 capability 文案。需要新增 task runtime provider，并在 `build_session_runtime_tool_composer` 加入 provider。

### 旧 Platform MCP Task Scope 退出点

- 调研时发现旧 Task MCP server 绑定具体 Task 并暴露独立 Task info/status/artifact/sibling 工具；实现阶段已删除该 server 导出和 HTTP 路由。
- 旧 Story MCP 仍有 `create_task`、`batch_create_tasks`、`list_tasks`：`crates/agentdash-mcp/src/servers/story.rs:450`、`crates/agentdash-mcp/src/servers/story.rs:482`、`crates/agentdash-mcp/src/servers/story.rs:518`。按最新约束，新 agent-facing `task_read` / `task_write` 不应复用这些工具名或 MCP server。
- SPI 的 `PlatformMcpScope::Task` 已不存在；实现阶段需要同步让 MCP transport 不再挂载 Task scope endpoint。
- capability resolver 测试已经表达“task session 启用 Task tools，但不再注入 TaskMcpServer”：`crates/agentdash-application/src/capability/resolver.rs:647`。
- 实现阶段已删除 `TaskMcpServer` 导出和 `/mcp/task/{id}` 路由；resolver 测试只保留“不注入 Task MCP”的断言。

### `LifecycleRun.tasks` Domain / Application / API / Frontend 读写能力

- Domain value object：`LifecycleTaskPlanItem` 包含 id/title/body/status/priority/created_by/owner/assigned/source/context_refs/story_ref/audit timestamps/archive：`crates/agentdash-domain/src/workflow/value_objects/task_plan.rs:57`。状态为 `open | active | review | blocked | done | dropped`，且有状态迁移守卫：`crates/agentdash-domain/src/workflow/value_objects/task_plan.rs:11`、`crates/agentdash-domain/src/workflow/value_objects/task_plan.rs:20`。
- Domain aggregate：`LifecycleRun` 拥有 `tasks: Vec<LifecycleTaskPlanItem>`：`crates/agentdash-domain/src/workflow/entity.rs:168`。已有 `create_task`、`update_task`、`archive_task`、`transition_task_status`：`crates/agentdash-domain/src/workflow/entity.rs:257`、`crates/agentdash-domain/src/workflow/entity.rs:295`、`crates/agentdash-domain/src/workflow/entity.rs:338`、`crates/agentdash-domain/src/workflow/entity.rs:351`。
- Persistence：Postgres repository roundtrip 解析 `lifecycle_runs.tasks`：`crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:665`；migration 已有 `lifecycle_runs.tasks`：`crates/agentdash-infrastructure/migrations/0015_lifecycle_run_tasks_story_task_cleanup.sql:2`；repository 测试覆盖 Task roundtrip：`crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:1090`。
- Application command：`list_run_tasks`、`create_run_task`、`update_run_task`、`archive_run_task`、`transition_run_task_status` 已可复用：`crates/agentdash-application/src/task/plan.rs:89`、`crates/agentdash-application/src/task/plan.rs:103`、`crates/agentdash-application/src/task/plan.rs:126`、`crates/agentdash-application/src/task/plan.rs:157`、`crates/agentdash-application/src/task/plan.rs:180`。
- Application filter 只支持 created_by / owner / assigned / include_archived：`crates/agentdash-application/src/task/plan.rs:13`、`crates/agentdash-application/src/task/plan.rs:377`。没有 status filter、pagination、detail mode、execution/context-specific mode、snapshot diff、reorder command。
- Story projection builder 从 Story-bound run / linked run / explicit `story_ref` 推导 Task projection：`crates/agentdash-application/src/task/plan.rs:288`；projection 只包含未 archived tasks：`crates/agentdash-application/src/task/plan.rs:405`。
- Fanout 可复用 `select_task_fanout_candidates`，它从 root run 或 Story projection 选择 Task，并复用 `list_run_tasks` / `build_story_task_projection`：`crates/agentdash-application/src/task/fanout.rs:20`、`crates/agentdash-application/src/task/fanout.rs:99`。
- API routes 已有 run-scoped 与 agent-run-scoped Task plan routes：`GET/POST /lifecycle-runs/{run_id}/tasks`、`PATCH /lifecycle-runs/{run_id}/tasks/{task_id}`、`PATCH /status`、`POST /archive`、`GET/POST /agent-runs/{run_id}/agents/{agent_id}/tasks`：`crates/agentdash-api/src/routes/task_plan.rs:32`。
- AgentRun task API 在 create 时会默认填入 `created_by_agent_id` / `owner_agent_id` 为当前 agent scope：`crates/agentdash-api/src/routes/task_plan.rs:170`、`crates/agentdash-api/src/routes/task_plan.rs:242`。
- Contract DTO 已生成到 `task-contracts.ts`，包括 `TaskResponse`、`RunTaskPlanResponse`、`CreateRunTaskRequest`、`UpdateRunTaskRequest`、`UpdateRunTaskStatusRequest`：`crates/agentdash-contracts/src/task/contract.rs:76`、`crates/agentdash-contracts/src/task/contract.rs:145`、`crates/agentdash-contracts/src/task/contract.rs:154`、`packages/app-web/src/generated/task-contracts.ts:7`。
- Frontend service/store 已封装现有 HTTP API：`packages/app-web/src/services/taskPlan.ts:33`、`packages/app-web/src/services/taskPlan.ts:42`、`packages/app-web/src/services/taskPlan.ts:52`、`packages/app-web/src/stores/taskPlanStore.ts:43`。

### AgentRun Workspace / Story Projection 前端 Task 展示

- AgentRun workspace 已展示 Task：`AgentRunWorkspacePage` 在聊天面板上方挂载 `<TaskPlanPanel runId={currentRunId} agentId={currentAgentId} />`：`packages/app-web/src/pages/AgentRunWorkspacePage.tsx:740`。
- `TaskPlanPanel` 会调用 `fetchAgentRunTasks(runId, agentId)`，按 `updated_at` 排序，展示创建、状态推进和归档入口：`packages/app-web/src/features/task/task-plan-panel.tsx:52`、`packages/app-web/src/features/task/task-plan-panel.tsx:71`、`packages/app-web/src/features/task/task-plan-panel.tsx:89`、`packages/app-web/src/features/task/task-plan-panel.tsx:114`。
- `TaskDrawer` 支持编辑 title/body/priority/owner/assigned/source/status，展示 `context_refs` 和 `story_ref`，并挂载 `TaskSubjectExecutionPanel`：`packages/app-web/src/features/task/task-drawer.tsx:53`、`packages/app-web/src/features/task/task-drawer.tsx:98`、`packages/app-web/src/features/task/task-drawer.tsx:260`、`packages/app-web/src/features/task/task-drawer.tsx:289`。
- `TaskSubjectExecutionPanel` 通过 `fetchSubjectExecution("task", task.id)` 展示 Task 的执行投影，说明 runtime facts 不在 Task facts 内：`packages/app-web/src/features/task/task-subject-execution-panel.tsx:1`、`packages/app-web/src/features/task/task-subject-execution-panel.tsx:118`。
- Story 页面已展示 Task projection：`StoryTaskRows` 展示 `StoryTaskProjectionItem[]`，来源 label 支持 owning_run / linked_run / story_ref：`packages/app-web/src/pages/StoryPage.tsx:97`、`packages/app-web/src/pages/StoryPage.tsx:112`。
- Story 页面加载 projection store 并渲染 “Task Projection” section：`packages/app-web/src/pages/StoryPage.tsx:168`、`packages/app-web/src/pages/StoryPage.tsx:249`、`packages/app-web/src/pages/StoryPage.tsx:557`。
- Story service/store 使用 `GET /stories/{id}/task-projection`：`packages/app-web/src/services/story.ts:147`、`packages/app-web/src/stores/storyStore.ts:265`。
- AgentRun workspace contract 本身未嵌入 tasks；`AgentRunWorkspaceView` 字段为 run/agent/shell/runtime/control/frame/subject/resource/conversation 等，未包含 `tasks`：`crates/agentdash-contracts/src/runtime/workflow.rs:1098`。当前 Task 面板通过独立 Task plan API 取数。

### 最小实现建议

- 新增 application runtime provider：建议放在 `crates/agentdash-application/src/task/tools/runtime_provider.rs` 或 `crates/agentdash-application/src/task/runtime_tool_provider.rs`，导出 `TaskRuntimeToolProvider`，实现 `RuntimeToolProvider`。
- 在 `build_session_runtime_tool_composer` 中把 `TaskRuntimeToolProvider` 加入 provider vec，位置建议在 `WorkflowRuntimeToolProvider` 后、Collaboration 前或后均可；关键是走同一 `SessionRuntimeToolComposer`，不要走 MCP injection。
- provider gate 规则：`enabled_clusters.contains(ToolCluster::Task)`，并分别检查 `CAP_TASK + task_read` / `CAP_TASK + task_write`。参考 `WorkspaceModuleRuntimeToolProvider` 的多工具 gate 模式。
- `task_read` 第一版可复用 `list_run_tasks`、`build_story_task_projection`、`find_task_plan_item_for_subject`、现有 SubjectExecution API/service 读取执行摘要。输入 mode 建议先覆盖 design 中的 `overview/list/detail/context/execution/projection`，但内部可以共享同一 view builder。
- `task_write` 第一版可把 patch operations 映射到 `create_run_task`、`update_run_task`、`transition_run_task_status`、`archive_run_task`。写后返回等价 `task_read(return_mode)` 的完整 view，避免模型拿到薄 response。
- snapshot / reorder 不能直接复用现有 command：domain item 没有显式 `order` 字段，application 也没有 replace/snapshot diff command。若要严格支持 reorder，需要先确认用 Vec 顺序作为事实还是新增 order 字段；当前预研约束说无需兼容旧 API，但涉及 DB JSON schema / generated contracts 时仍要同步 migration 和 contract generation。
- 工具 scope 解析：优先从 `ExecutionContext.turn.hook_runtime.snapshot().run_context` / frame association 获取 run_id、project_id、agent_id；helper 已有 `project_id_from_context` 和 `runtime_session_id_from_context` 可复用：`crates/agentdash-application/src/runtime_tools/provider.rs:101`、`crates/agentdash-application/src/runtime_tools/provider.rs:110`。若缺 run/agent，需要查 AgentRun workspace / runtime anchor，而不是回落到 Story-owned Task CRUD。
- 审计：`task_write` 应在 application command 层统一记录 execution log / state_change / lifecycle event，至少携带 session id、run id、agent id、tool call id、changed task ids。
- contracts：如果 agent-facing tool schema 需要前端验收或 tool catalog 展示之外的 DTO，应进入 `agentdash-contracts` 并生成 TS；现有 browser Task DTO 可复用但不足以表达 thick read modes、snapshot operations、return view。
- 前端验收最小路径：工具写入 `LifecycleRun.tasks` 后，刷新 `TaskPlanPanel` 的 `fetchAgentRunTasks` 即可在 AgentRun workspace 看到结果；Story projection 通过 `fetchStoryTaskProjection` 验证来源。若要“workspace projection 一次读到 Task”，需把 Task plan view 加进 `AgentRunWorkspaceView`，但这不是当前已有入口。

### 潜在风险

- 概念混用风险：Task runtime tools 应坚持 `CAP_TASK` + `ToolCluster::Task` + runtime provider，避免通过平台 MCP Task scope 扩展新入口。
- 半迁移风险：SPI/catalog 已有 `CAP_TASK` 与 `ToolCluster::Task`，但 application runtime provider 缺失；能力面会显示 `task_read/task_write`，实际 session tools 可能为空。
- 写入一致性风险：现有 commands 每次 load run -> mutate Vec -> repo.update whole aggregate；`task_write snapshot` 若无 revision / updated_at precondition，容易覆盖并发 agent 的 Task 修改。
- 排序风险：现有 DTO 没有 `order` 字段，前端按 `updated_at` 排序。实现 `reorder_tasks` 前必须决定排序事实源，避免“工具宣称排序但 UI/projection 不稳定”。
- 视图厚度风险：现有 `RunTaskPlanResponse` 是薄 list，不含 mode、pagination、execution summary、source lineage、view revision。直接复用它作为 `task_read detail/context/execution/projection` 会不满足设计里的厚读回。
- AgentRun workspace 同步风险：当前 `AgentRunWorkspaceView` 不含 tasks，Task panel 单独取数。工具写入后如果只刷新 workspace projection，不刷新 `taskPlanStore`，前端不会立即显示新 Task。
- Permission 文案/测试需要统一使用 `task` capability，授权面只暴露当前 Task runtime tools。

## External References

- None. 本次只做项目内部代码路径调研，未联网检索。

## Related Specs

- `.trellis/spec/backend/story-task-runtime.md` — 明确 `LifecycleRun.tasks` 是 Task plan facts 源，Story Task projection 为推导视图，AgentRun workspace 是 Task plan API 写入口。
- `.trellis/spec/backend/runtime-gateway.md` — 说明 runtime tool declaration 不得产生副作用，session launch 与 runtime refresh 共享 `assemble_tools_for_execution_context`。
- `.trellis/spec/cross-layer/frontend-backend-contracts.md` — 说明 Task plan request/response DTO 属于 `agentdash-contracts`，前端消费 generated TS。
- `.trellis/spec/frontend/index.md` — 前端变更需遵守 generated contract、store、component 分层。

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` 返回 `(none)`；本文件写入路径使用用户明确指定的 `.trellis/tasks/06-17-universal-task-toolset/research/`，不是自动 active task 推导。
- 未发现 application 层 `task_read` / `task_write` `AgentTool` 实现或 `TaskRuntimeToolProvider`。
- 未发现 snapshot diff、bulk patch、reorder 或 revision/precondition command；这些需要新增 application/domain contract。
- 未运行测试，也未修改代码；结论只基于静态读取。
