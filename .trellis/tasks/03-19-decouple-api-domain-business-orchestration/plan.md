# 解耦 API 与 Domain 业务重构 — 执行计划

> 本文档是 PRD 的配套执行计划，记录诊断结论、重构方案与推进进度。

## 诊断结论

### 核心数据

| 文件 | 行数 | 当前位置 | 应属层 |
|------|------|---------|--------|
| `session_plan.rs` | ~770 | api | application |
| `task_agent_context.rs` | ~1334 | api | application |
| `address_space_access.rs` | ~1689 | api | application + infra |
| `bootstrap/task_execution_gateway.rs` | ~1679 | api/bootstrap | application + infra |
| `acp_sessions.rs`（Story Owner 编排部分） | ~170 | api/routes | application |
| **合计** | **~5640** | **api** | **应迁出** |

### 问题清单

1. **application crate 是"壳"**：仅 ~426 行（execution trait + lock + restart_tracker + state_reconciler），核心编排全在 api crate。
2. **`address_space_access.rs` 承担三重职责**：Application 级服务 + Runtime Tool 实现 + Context Container→Mount 映射。
3. **`task_agent_context.rs` 混合框架和实现**：Contributor trait 定义 + 5 个内置 Contributor + 工作空间源解析 + 模板渲染。
4. **`bootstrap/task_execution_gateway.rs` 名不副实**：名为 bootstrap，实为最重的 Application Service（turn 监听、重试、artifact 持久化、relay 中继）。
5. **`acp_sessions.rs` 含完整 Story Owner 编排**：`build_story_owner_prompt_request` / `build_story_context_markdown` 直接写在 route 文件中。
6. **Route Handler 中存在跨实体组装**：`task_execution.rs` 的 `build_task_session_context_response`。
7. **Domain 实体直接外露为 API Response**：`cancel_task` 返回 `Json<Task>` 等。
8. **AppState 膨胀为 16 字段的"上帝对象"**。

## 目标分层

```
Transport Layer (agentdash-api)
  ├── Route Handlers: 解析入参 → 调用 use-case → 映射 DTO
  ├── Response DTOs / Assemblers
  ├── 统一错误映射
  └── SSE/NDJSON 流

Application Layer (agentdash-application)
  ├── TaskExecutionService (gateway impl + turn monitor)
  ├── StorySessionService
  ├── ContextBuildService (contributor framework + builtins)
  ├── SessionPlanService
  ├── AddressSpaceService (mount derivation + CRUD)
  └── TurnMonitorService

Domain Layer (agentdash-domain) — 保持不变
  ├── 实体 + 值对象
  ├── Repository traits
  └── Context/Session 模型

Infrastructure Layer
  ├── agentdash-infrastructure (SQLite repos)
  ├── agentdash-relay (WebSocket 中继)
  ├── agentdash-executor (连接器)
  ├── agentdash-injection (context composer)
  └── Runtime Tool Adapters (从 address_space_access 拆出)
```

## 执行阶段

### Phase 1: 迁移 Session Plan + Context Composition → Application ✅

**状态：已完成** — 2026-03-20

| 源文件 | 目标 | 动作 |
|--------|------|------|
| `api/src/session_plan.rs` | `application/src/session_plan.rs` | 整体搬移 |
| `api/src/task_agent_context.rs` 中 Contributor trait/Registry | `application/src/context/contributor.rs` | 拆分搬移 |
| `api/src/task_agent_context.rs` 中内置 Contributors | `application/src/context/builtins.rs` | 拆分搬移 |
| `api/src/task_agent_context.rs` 中 `build_task_agent_context` | `application/src/context/builder.rs` | 拆分搬移 |
| `api/src/task_agent_context.rs` 中工作空间源解析 | `application/src/context/workspace_sources.rs` | 拆分搬移 |

验证：这些文件完全没有 HTTP/Axum 依赖，迁移无摩擦。

### Phase 2: 迁移 Task Execution Gateway 纯业务逻辑到 Application ✅

**状态：已完成** — 2026-03-20

| 逻辑块 | 目标位置 | 动作 |
|--------|---------|------|
| Tool call artifact 构建 (`upsert_tool_execution_artifact` 等) | `application/src/task/artifact.rs` | 搬移纯函数 |
| 执行器/Agent 配置解析 (`resolve_task_executor_config` 等) | `application/src/task/config.rs` | 搬移纯函数 |
| ACP meta 构建 (`build_task_lifecycle_meta`, `parse_turn_event` 等) | `application/src/task/meta.rs` | 搬移纯函数 |
| Turn 监听主循环 + Relay prompt/cancel | 保留在 api `task_execution_gateway.rs` | 依赖基础设施 |

验证：`api/bootstrap/task_execution_gateway.rs` 通过 re-export 保持向后兼容。

### Phase 3: 拆分 AddressSpaceAccess 三重职责 ✅

**状态：已完成** — 2026-03-20

| 逻辑块 | 目标位置 | 动作 |
|--------|---------|------|
| Domain 值类型 (ResourceRef, ListOptions 等) | `application/src/address_space/types.rs` | 搬移 |
| 路径归一化/Mount 解析 | `application/src/address_space/path.rs` | 搬移 |
| Context Container→Mount 映射 + Address Space 组装 | `application/src/address_space/mount.rs` | 搬移 |
| `RelayAddressSpaceService` (依赖 BackendRegistry) | 保留在 api `address_space_access.rs` | 导入 application 函数 |
| `RelayRuntimeToolProvider` (Agent 工具实现) | 保留在 api `address_space_access.rs` | 导入 application 函数 |

验证：API 层通过 `pub use agentdash_application::address_space::*` re-export，内部测试全部通过。

### Phase 4: Story Owner Session 编排提取 ✅

**状态：已完成** — 2026-03-20

| 逻辑块 | 目标位置 | 动作 |
|--------|---------|------|
| `build_story_context_markdown` | `application/src/story/context_builder.rs` | 搬移 |
| `build_story_owner_prompt_blocks` | `application/src/story/context_builder.rs` | 搬移 |
| `StoryContextBuildInput` 输入 DTO | `application/src/story/context_builder.rs` | 新建 |
| `augment_prompt_request_for_owner` | 保留在 api `acp_sessions.rs` | 薄协调器：加载实体→调用 application |
| `resolve_workspace_declared_sources` | 保留在 api `task_agent_context.rs` | 依赖 AppState 服务 |

验证：route handler 变为协调器，纯逻辑下沉到 application。

### Phase 5: 引入 Response DTO / Assembler ✅

**状态：已完成** — 2026-03-20

| DTO 类型 | 文件 | 覆盖端点 |
|----------|------|---------|
| `ProjectResponse` | `api/src/dto/project.rs` | list_projects, create_project, update_project |
| `ProjectDetailResponse` | `api/src/dto/project.rs` | get_project（含 workspaces + stories） |
| `StoryResponse` | `api/src/dto/story.rs` | list_stories, create_story, get_story, update_story |
| `TaskResponse` | `api/src/dto/task.rs` | list_tasks, create_task, get_task, update_task, cancel_task |
| `WorkspaceResponse` | `api/src/dto/workspace.rs` | list_workspaces, create_workspace, get_workspace, update_workspace |

模式：`From<DomainEntity>` 转换，嵌套值对象（StoryContext, AgentBinding 等）暂共享 domain 类型。
原 `projects.rs` 中的 `ProjectDetailResponse` 已迁入 DTO 模块并改用 `ProjectResponse` 而非 `#[serde(flatten)] Project`。

### Phase 6: AppState 瘦身 ✅

**状态：已完成** — 2026-03-20

原 18 字段的"上帝对象"拆分为 4 个语义子集 + 1 个独立字段：

| 子结构 | 包含字段 | 职责 |
|--------|---------|------|
| `RepositorySet` (repos) | project_repo, workspace_repo, story_repo, task_repo, sqlite_task_repo, session_binding_repo, backend_repo, settings_repo | 持久化层端口 |
| `ServiceSet` (services) | executor_hub, connector, address_space_service, backend_registry, contributor_registry, address_space_registry | 应用服务与注册表 |
| `TaskRuntime` (task_runtime) | lock_map, restart_tracker | Task 执行运行时状态 |
| `AppConfig` (config) | mcp_base_url | 应用级配置 |
| (直属) | remote_sessions | 远程会话路由映射 |

所有引用已统一更新：`state.field` → `state.group.field`。

## 完成总结

6 个阶段全部完成（2026-03-20）。主要成果：

1. **`agentdash-application` 从空壳变为核心编排层**：新增 session_plan、context（contributor/builtins/builder）、task（artifact/config/meta）、address_space（mount/path/types）、story（context_builder）共 12 个模块。
2. **`agentdash-api` 收缩为薄 Transport 层**：route handler 职责限于解析入参→调用 application→映射 DTO。
3. **DTO 层建立**：`api/src/dto/` 包含 ProjectResponse、StoryResponse、TaskResponse、WorkspaceResponse，所有 CRUD 端点不再直接暴露 Domain 实体。
4. **AppState 从 18 字段拆分为 4 个语义分组**：RepositorySet、ServiceSet、TaskRuntime、AppConfig。
5. **全部 112 个测试在每个阶段持续通过**，零回归。

## 风险与约束

- 本项目为预研阶段，不需要考虑兼容性
- 每个 Phase 完成后必须 `cargo check --workspace` 通过
- 前端同步修改不在本任务范围内（DTO 引入时可能需要）
