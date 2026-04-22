# Research: Platform MCP Scope Tool Enumeration

- **Query**: enumerate every `#[tool(...)]` handler under `crates/agentdash-mcp/src/servers/{relay,story,task,workflow}.rs` — name, description, and fn signature.
- **Scope**: internal
- **Date**: 2026-04-22

Each table below is keyed to the `PlatformMcpScope` variant that the implementer should map into `ToolDescriptor::platform_mcp(name, display_name, description, scope)` in `platform_tool_descriptors()`.

---

## PlatformMcpScope::Relay (file: `crates/agentdash-mcp/src/servers/relay.rs`)

| tool_name | description (verbatim from `#[tool(description = ...)]`) | fn signature |
|-----------|----------|--------------|
| `list_projects` | 列出所有项目，可按名称关键字过滤 | `async fn list_projects(&self, Parameters(params): Parameters<ListProjectsParams>) -> Result<CallToolResult, rmcp::ErrorData>` (line 110) |
| `get_project` | 获取指定项目的完整信息，包括配置和关联的 Story 概况 | `async fn get_project(&self, Parameters(params): Parameters<GetProjectParams>) -> Result<CallToolResult, rmcp::ErrorData>` (line 146) |
| `create_story` | 在指定项目中创建一个新的 Story（用户价值单元） | `async fn create_story(&self, Parameters(params): Parameters<CreateStoryParams>) -> Result<CallToolResult, rmcp::ErrorData>` (line 197) |
| `list_stories` | 列出指定项目下的所有 Story | `async fn list_stories(&self, Parameters(params): Parameters<ListStoriesParams>) -> Result<CallToolResult, rmcp::ErrorData>` (line 235) |
| `get_story_detail` | 获取 Story 的完整详情，包括上下文信息和关联的 Task 列表 | `async fn get_story_detail(&self, Parameters(params): Parameters<GetStoryDetailParams>) -> Result<CallToolResult, rmcp::ErrorData>` (line 269) |
| `update_story_status` | 变更 Story 状态（如从 created 推进到 context_ready） | `async fn update_story_status(&self, Parameters(params): Parameters<UpdateStoryStatusParams>) -> Result<CallToolResult, rmcp::ErrorData>` (line 317) |
| `update_project_context_config` | 更新 Project 的上下文容器与挂载策略配置 | `async fn update_project_context_config(&self, Parameters(params): Parameters<UpdateProjectContextConfigParams>) -> Result<CallToolResult, rmcp::ErrorData>` (line 352) |

Total: **7 tools**.

> Naming note: the relay server mixes project-management and story-management tools. PRD §③ maps these to the `relay_management` capability key — all 7 should be tagged under `PlatformMcpScope::Relay`.

---

## PlatformMcpScope::Story (file: `crates/agentdash-mcp/src/servers/story.rs`)

| tool_name | description | fn signature |
|-----------|----------|--------------|
| `get_story_context` | 获取当前 Story 的完整上下文信息（声明式来源与容器） | `async fn get_story_context(&self) -> Result<CallToolResult, rmcp::ErrorData>` (line 211) |
| `update_story_context` | 更新 Story 上下文：声明式 source_refs / 容器 / 会话编排 | `async fn update_story_context(&self, Parameters(params): Parameters<UpdateStoryContextParams>) -> Result<CallToolResult, rmcp::ErrorData>` (line 233) |
| `update_story_details` | 更新 Story 基本信息（标题、描述、优先级、类型、标签） | `async fn update_story_details(&self, Parameters(params): Parameters<UpdateStoryDetailsParams>) -> Result<CallToolResult, rmcp::ErrorData>` (line 309) |
| `create_task` | 在当前 Story 下创建一个新的 Task（执行单元） | `async fn create_task(&self, Parameters(params): Parameters<CreateTaskParams>) -> Result<CallToolResult, rmcp::ErrorData>` (line 355) |
| `batch_create_tasks` | 在当前 Story 下批量创建多个 Task（通常用于 Story 拆解完成后一次性创建） | `async fn batch_create_tasks(&self, Parameters(params): Parameters<BatchCreateTasksParams>) -> Result<CallToolResult, rmcp::ErrorData>` (line 408) |
| `list_tasks` | 列出当前 Story 下的所有 Task 及其状态 | `async fn list_tasks(&self) -> Result<CallToolResult, rmcp::ErrorData>` (line 467) |
| `advance_story_status` | 推进 Story 生命周期状态（如从 created 到 context_ready，或到 decomposed） | `async fn advance_story_status(&self, Parameters(params): Parameters<AdvanceStoryStatusParams>) -> Result<CallToolResult, rmcp::ErrorData>` (line 498) |

Total: **7 tools**.

---

## PlatformMcpScope::Task (file: `crates/agentdash-mcp/src/servers/task.rs`)

| tool_name | description | fn signature |
|-----------|----------|--------------|
| `get_task_info` | 获取当前绑定 Task 的完整信息 | `async fn get_task_info(&self) -> Result<CallToolResult, rmcp::ErrorData>` (line 96) |
| `update_task_status` | 更新当前 Task 的执行状态 | `async fn update_task_status(&self, Parameters(params): Parameters<UpdateTaskStatusParams>) -> Result<CallToolResult, rmcp::ErrorData>` (line 121) |
| `report_artifact` | 上报 Task 执行产物（代码变更、测试结果、日志等） | `async fn report_artifact(&self, Parameters(params): Parameters<ReportArtifactParams>) -> Result<CallToolResult, rmcp::ErrorData>` (line 150) |
| `get_sibling_tasks` | 查看同一 Story 下的其它 Task 及其状态（只读，用于协调） | `async fn get_sibling_tasks(&self) -> Result<CallToolResult, rmcp::ErrorData>` (line 190) |
| `get_story_context` | 获取所属 Story 的上下文信息（PRD、规范引用），用于理解任务背景 | `async fn get_story_context(&self) -> Result<CallToolResult, rmcp::ErrorData>` (line 216) |
| `append_task_description` | 向 Task 描述中追加内容（记录执行过程发现的关键信息） | `async fn append_task_description(&self, Parameters(params): Parameters<AppendTaskDescriptionParams>) -> Result<CallToolResult, rmcp::ErrorData>` (line 240) |

Total: **6 tools**.

> Name collision caveat: both `story.rs` and `task.rs` register a tool named `get_story_context`. At the rmcp router level these are scoped to separate server instances, but for `platform_tool_descriptors()` the implementer must decide whether to emit one entry per (scope, name) pair (two entries with same `name` but different `scope`) or namespace the tool name. The existing `#[tool(name = ...)]` attribute is **not** used on either, so the defaults (`get_story_context`) collide. Recommend emitting both with distinct `PlatformMcpScope` tags; frontend tool-catalog shows them on different capability buckets.

---

## PlatformMcpScope::Workflow (file: `crates/agentdash-mcp/src/servers/workflow.rs`)

| tool_name | description | fn signature |
|-----------|----------|--------------|
| `list_workflows` | 列出当前项目下所有 Workflow 和 Lifecycle 定义 | `async fn list_workflows(&self) -> Result<CallToolResult, rmcp::ErrorData>` (line 494) |
| `get_workflow` | 获取单个 Workflow 定义的完整详情（含 contract） | `async fn get_workflow(&self, Parameters(params): Parameters<GetWorkflowParams>) -> Result<CallToolResult, rmcp::ErrorData>` (line 536) |
| `get_lifecycle` | 获取单个 Lifecycle 定义的完整详情（含 steps、edges） | `async fn get_lifecycle(&self, Parameters(params): Parameters<GetLifecycleParams>) -> Result<CallToolResult, rmcp::ErrorData>` (line 559) |
| `upsert_workflow_tool` | 创建或更新 Workflow 定义（单步行为契约）。保存时自动校验，失败会返回详细错误信息。 | `async fn upsert_workflow_tool(&self, Parameters(params): Parameters<UpsertWorkflowParams>) -> Result<CallToolResult, rmcp::ErrorData>` (line 582) |
| `upsert_lifecycle_tool` | 创建或更新 Lifecycle 定义（多步 DAG 编排）并自动绑定到当前 Project。\n\n保存时自动校验 DAG 拓扑、port 契约和 workflow 引用。step.workflow_key 引用的 Workflow 必须已存在。 | `async fn upsert_lifecycle_tool(&self, Parameters(params): Parameters<UpsertLifecycleParams>) -> Result<CallToolResult, rmcp::ErrorData>` (line 613) |

Total: **5 tools** (cross-checked against `all_tools_registered` test at line 772 which asserts `names.len() == 5`).

---

## Summary of tool counts

| Scope | Tool count | File |
|---|---|---|
| Relay | 7 | `relay.rs` |
| Story | 7 | `story.rs` |
| Task | 6 | `task.rs` |
| Workflow | 5 | `workflow.rs` |
| **Total** | **25** | — |

## Caveats / Not Found

- None of these handlers use the `#[tool(name = "...")]` override, so the tool name is always the Rust function name as written. Implementer can copy-paste directly.
- `display_name` is **not** present in any `#[tool(...)]` attribute — only `description`. PRD §③ says `ToolDescriptor::platform_mcp(name, display_name, description, scope)` — the implementer must synthesize `display_name` (e.g. `"List Projects"`, `"Get Project"`, `"Create Story"`) manually; no single authoritative source exists in the mcp server files.
- `update_story_context` has a schema-compatibility test at `story.rs:558` — keep schema sanitization semantics in mind if any of these tools need `sanitize_tool_schema` treatment in the descriptor pipeline.
- `create_task` and `batch_create_tasks` live under `StoryMcpServer` (scope=Story) even though they create Tasks. Semantic mapping to capabilities is a Story-level action.
- The `_project_id` field on `TaskMcpServer` (task.rs:61) is underscore-prefixed (unused); no additional tools use it.
