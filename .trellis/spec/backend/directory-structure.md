# Directory Structure

> How backend code is organized in this project.

---

## 概览

> **AgentDashboard 后端代码的组织方式。**

### 设计原则

按照项目的**策略可插拔**原则，目录结构应体现模块边界：
- 每个模块独立目录，模块间通过接口交互
- 接口定义与实现分离
- 策略（Strategy）作为可替换组件

---

## 目录布局（Rust / Axum — 整洁架构分层）

**架构原则**：依赖方向 外层 → 内层（Interface → Application → Domain）

```
crates/
├── agentdash-api/               # Interface Layer (HTTP) — 薄 Transport 层
│   └── src/
│       ├── main.rs              # Axum 启动入口
│       ├── app_state.rs         # 依赖注入（RepositorySet / ServiceSet / TaskRuntime / AppConfig）
│       ├── routes.rs            # 路由注册
│       ├── rpc.rs               # ApiError 统一错误处理
│       ├── stream.rs            # 全局事件流（SSE + NDJSON）
│       ├── dto/                 # Response DTO — 隔离 Domain 实体与 API 契约
│       │   ├── mod.rs
│       │   ├── project.rs       # ProjectResponse, ProjectDetailResponse
│       │   ├── story.rs         # StoryResponse
│       │   ├── task.rs          # TaskResponse
│       │   └── workspace.rs     # WorkspaceResponse
│       ├── session_plan.rs      # re-export → application
│       ├── task_agent_context.rs # re-export → application
│       ├── runtime_bridge.rs    # re-export → application
│       ├── workspace_resolution.rs # 薄适配器（BackendAvailability → AppState）
│       ├── address_space_access/ # 集成测试 only（re-export 已清除，API 层消费者直接导入 application::address_space）
│       ├── execution_hooks/     # re-export → application::hooks
│       ├── bootstrap/
│       │   ├── task_execution_gateway.rs  # 薄适配器（~360行，含 relay dispatch）
│       │   └── task_state_reconcile.rs
│       ├── relay/               # WebSocket 后端中继
│       │   ├── registry.rs
│       │   └── ws_handler.rs
│       └── routes/              # 路由处理函数（解析入参→调用用例→映射 DTO）
│           ├── health.rs
│           ├── projects.rs
│           ├── workspaces.rs
│           ├── backends.rs
│           ├── stories.rs
│           ├── acp_sessions.rs
│           ├── story_sessions.rs
│           ├── task_execution.rs
│           ├── settings.rs
│           ├── address_spaces.rs
│           ├── workspace_files.rs
│           ├── discovery.rs
│           └── discovered_options.rs
│
├── agentdash-application/       # Application Layer (用例编排) ✅ 已填充
│   └── src/
│       ├── lib.rs
│       ├── session_plan.rs      # 会话编排解析与片段构建
│       ├── repository_set.rs    # RepositorySet（所有 Repository 的集合）
│       ├── runtime_bridge.rs    # connector ↔ application 类型转换
│       ├── context/             # Agent 上下文构建框架
│       │   ├── mod.rs
│       │   ├── contributor.rs   # ContextContributor trait + Registry
│       │   ├── builtins.rs      # 5 个内置 Contributor 实现
│       │   ├── builder.rs       # build_task_agent_context 入口
│       │   └── workspace_sources.rs # 声明式来源解析（File/ProjectSnapshot）
│       ├── hooks/               # ExecutionHookProvider（从 api 迁入）
│       │   ├── mod.rs           # 模块声明 + 共享辅助函数
│       │   ├── provider.rs      # AppExecutionHookProvider 实现
│       │   ├── rules.rs         # Hook 规则评估引擎
│       │   ├── completion.rs    # Workflow 完成判定
│       │   ├── workflow_contribution.rs # Workflow 上下文片段构建
│       │   ├── helpers.rs       # Payload 提取 / shell cwd 重写等工具函数
│       │   ├── owner_resolver.rs
│       │   ├── snapshot_helpers.rs
│       │   ├── workflow_snapshot.rs
│       │   └── test_fixtures.rs # 测试共享 fixtures
│       ├── task/                # Task 执行相关纯逻辑
│       │   ├── mod.rs
│       │   ├── artifact.rs      # Tool call artifact 构建
│       │   ├── config.rs        # 执行器/Agent 配置解析
│       │   ├── meta.rs          # ACP meta 构建与 turn 事件解析
│       │   ├── execution.rs     # TaskExecutionGateway trait
│       │   ├── lock.rs          # Per-Task 异步锁
│       │   ├── restart_tracker.rs # 重启追踪器
│       │   ├── state_reconciler.rs # 启动时状态修复
│       │   ├── gateway/         # gateway 核心逻辑（从 api 迁入）
│       │   │   ├── repo_ops.rs  # 纯 repo 操作辅助函数
│       │   │   ├── turn_context.rs # turn 上下文准备
│       │   │   └── turn_monitor.rs # turn 监听与事件处理
│       │   └── tools/           # Task 相关 tool 实现（companion/hook）
│       ├── address_space/       # 寻址空间组装（mount/path/types）
│       │   ├── mod.rs
│       │   ├── mount.rs         # Mount 派生与 Address Space 组装
│       │   ├── path.rs          # Mount 解析与路径归一化
│       │   ├── types.rs         # ResourceRef, ListOptions, ExecRequest 等值类型
│       │   ├── inline_persistence.rs # 内联内容持久化
│       │   ├── relay_service.rs # RelayAddressSpaceService
│       │   └── tools/           # FS/Shell/MountsList 等 tool 实现
│       ├── workflow/            # Workflow & Lifecycle
│       │   └── tools/           # WorkflowArtifactReportTool
│       ├── workspace/           # workspace 解析
│       │   └── resolution.rs    # resolve_workspace_binding（BackendAvailability trait）
│       └── story/               # Story Owner Session 编排
│           ├── mod.rs
│           └── context_builder.rs # build_story_context_markdown / prompt_blocks
│
├── agentdash-domain/            # Domain Layer (核心业务)
│   └── src/
│       ├── lib.rs
│       ├── common/
│       │   ├── error.rs         # DomainError
│       │   └── events.rs        # 领域事件 trait
│       ├── project/
│       │   ├── entity.rs        # Project 实体
│       │   ├── repository.rs    # ProjectRepository trait
│       │   └── value_objects.rs # ProjectConfig, AgentPreset
│       ├── workspace/
│       │   ├── entity.rs        # Workspace 实体
│       │   ├── repository.rs    # WorkspaceRepository trait
│       │   └── value_objects.rs # WorkspaceType, WorkspaceStatus, GitConfig
│       ├── story/
│       │   ├── entity.rs        # Story 实体
│       │   ├── repository.rs    # StoryRepository trait
│       │   └── value_objects.rs # StoryStatus, StoryContext, StateChange
│       ├── task/
│       │   ├── entity.rs        # Task 实体
│       │   ├── repository.rs    # TaskRepository trait
│       │   └── value_objects.rs # TaskStatus, AgentBinding, Artifact
│       ├── backend/
│       │   ├── entity.rs
│       │   └── repository.rs
│       ├── context_container.rs # 上下文容器定义与验证
│       ├── context_source.rs    # 声明式上下文来源
│       └── session_composition.rs # 会话编排配置
│
├── agentdash-infrastructure/    # Infrastructure Layer (技术实现)
│   └── src/
│       ├── lib.rs
│       └── persistence/
│           └── sqlite/
│               ├── mod.rs
│               ├── project_repository.rs
│               ├── workspace_repository.rs
│               ├── story_repository.rs
│               ├── task_repository.rs
│               ├── backend_repository.rs
│               ├── session_binding_repository.rs
│               └── settings_repository.rs
│
├── agentdash-executor/          # Infrastructure Layer (执行引擎)
│   └── src/
│       ├── lib.rs
│       ├── connector.rs         # AgentConnector trait
│       ├── hub.rs               # ExecutorHub
│       ├── hooks.rs             # Hook runtime port / snapshot / resolution
│       ├── runtime_delegate.rs  # HookSessionRuntime -> AgentRuntimeDelegate 适配
│       ├── adapters/
│       │   └── normalized_to_acp.rs
│       └── connectors/
│           ├── mod.rs
│           ├── pi_agent.rs      # 内置 AI Agent 连接器
│           ├── pi_agent_mcp.rs  # MCP 工具桥接
│           ├── pi_agent_provider_registry.rs  # LLM Provider 注册
│           ├── rig_bridge.rs    # RigBridge<M> — rig-core LlmBridge 实现 + AgentMessage ↔ rig::Message 转换
│           ├── composite.rs     # 多连接器组合
│           └── remote_acp.rs
│
├── agentdash-injection/         # Context Injection (声明式上下文解析)
├── agentdash-mcp/               # MCP Server 实现
├── agentdash-relay/             # WebSocket Relay 协议
├── agentdash-acp-meta/          # ACP 元数据 TypeScript 绑定
├── agentdash-agent-types/       # Agent 领域通用类型（AgentMessage/AgentTool/AgentContext/Delegate）
├── agentdash-agent/             # Agent 运行时核心（纯 loop + bridge trait，无 rig/spi 依赖）
└── agentdash-local/             # 本机后端执行器
```

### 关键 API 端点

| 路径 | 方法 | 说明 |
|------|------|------|
| `/api/projects` | GET/POST | 项目列表 / 创建项目 |
| `/api/projects/{id}` | GET/PUT/DELETE | 项目详情（含 workspaces+stories）/ 更新 / 删除 |
| `/api/projects/{project_id}/workspaces` | GET/POST | Workspace 列表 / 创建 |
| `/api/workspaces/{id}` | GET/DELETE | Workspace 详情 / 删除 |
| `/api/workspaces/{id}/status` | PATCH | 更新 Workspace 状态 |
| `/api/stories` | GET/POST | Story 列表（支持 project_id / backend_id 查询）/ 创建 |
| `/api/stories/{id}` | GET | Story 详情 |
| `/api/stories/{id}/tasks` | GET | Story 下的 Task 列表 |
| `/api/agents/discovery` | GET | 执行器发现 |
| `/api/sessions/{id}/prompt` | POST | 启动 ACP 会话执行 |
| `/api/sessions/{id}/cancel` | POST | 取消会话 |
| `/api/acp/sessions/{id}/stream` | GET | ACP 会话流（SSE） |
| `/api/acp/sessions/{id}/stream/ndjson` | GET | ACP 会话流（NDJSON） |
| `/api/events/stream` | GET | 全局事件流（SSE） |

### 连接器架构

```
AgentConnector trait
├── connector_id()          → &str
├── connector_type()        → ConnectorType (LocalExecutor | RemoteAcpBackend)
├── capabilities()          → ConnectorCapabilities
├── get_preset_configs()    → Vec<PresetConfig>
├── prompt()                → ExecutionStream
└── cancel()                → ()

实现：
├── VibeKanbanExecutorsConnector  → LocalExecutor（通过 vibe-kanban executors crate）
└── RemoteAcpConnector            → RemoteAcpBackend（骨架，待实现）
```

---

## 整洁架构分层约定

**核心原则**：依赖方向始终向内，外层依赖内层。

```
Interface Layer (agentdash-api)
    ↓ depends on
Application Layer (agentdash-application)
    ↓ depends on
Domain Layer (agentdash-domain)
    ↑ implemented by
Infrastructure Layer (agentdash-infrastructure, agentdash-executor)

Agent 子系统（独立于主分层）：
agentdash-agent-types  (零 runtime 核心类型)
    ↑ 被以下 crate 依赖
├── agentdash-agent    (Agent Loop 引擎，仅依赖 agent-types + domain)
├── agentdash-spi      (re-export + Connector/Hook trait)
└── agentdash-executor (RigBridge + rig-core LLM 调用)
```

#### 分层职责

| 分层 | Crate | 职责 | 允许依赖 | 状态 |
|------|-------|------|----------|------|
| **Interface** | `agentdash-api` | HTTP 路由、DTO/Assembler、中间件、错误映射 | application, domain | ✅ |
| **Application** | `agentdash-application` | 用例编排：session plan / context / task / address space / story | domain, injection, executor, spi | ✅ 已填充 |
| **Domain** | `agentdash-domain` | 实体、值对象、Repository 接口、领域事件 | 无外部库（仅 async-trait 等基础库） | ✅ |
| **Infrastructure** | `agentdash-infrastructure`, `agentdash-executor`, `agentdash-relay` | Repository 实现、连接器、WebSocket 中继 | domain | ✅ |
| **Agent Types** | `agentdash-agent-types` | 跨层共享的 Agent 类型（Message/Tool/Context/Delegate） | serde, async-trait, tokio-util | ✅ |
| **Agent Engine** | `agentdash-agent` | Agent Loop 引擎、LlmBridge trait、内置工具 | agent-types, domain | ✅ |

> Application 层（`agentdash-application`）已包含 session plan 构建、context contributor 框架、task 执行纯逻辑、address space 组装、story owner 编排等核心用例。API 层只保留请求解析→调用用例→映射 DTO 的协调职责。

#### Hook Runtime 分层约定

跨层 Hook Runtime 必须遵守以下目录/职责边界：

- `agentdash-agent-types`
  - 定义 `AgentRuntimeDelegate` trait、`ToolCallDecision`、`StopDecision` 等纯 runtime seam
  - 定义 `AgentTool` trait、`AgentContext`、`ToolDefinition` 等跨层共享类型
- `agentdash-agent`
  - 仅依赖 `agentdash-agent-types` + `agentdash-domain`，不依赖 rig-core 或 agentdash-spi
  - 不直接访问 workflow/task/story/project/repository
- `agentdash-executor`
  - 负责 `ExecutionHookProvider` port、`HookSessionRuntime`、`runtime_delegate.rs`
  - 持有 session 级 snapshot / diagnostics / revision
- `agentdash-api`
  - 实现 `AppExecutionHookProvider`
  - 负责从业务对象解析 snapshot / policy / resolution

禁止把 Hook 逻辑写回：

- route/gateway prompt augment 特化代码
- `agent_loop` 内部 repo 查询
- workflow runtime 巨石 if/else 中心

参考：`backend/execution-hook-runtime.md`

#### Repository 模式约定

**领域层定义接口（Port）**：
```rust
// agentdash-domain/src/story/repository.rs
#[async_trait]
pub trait StoryRepository: Send + Sync {
    async fn create(&self, story: &Story) -> Result<(), DomainError>;
    async fn list_by_backend(&self, backend_id: &str) -> Result<Vec<Story>, DomainError>;
}
```

**基础设施层实现（Adapter）**：
```rust
// agentdash-infrastructure/src/persistence/sqlite/story_repository.rs
pub struct SqliteStoryRepository { pool: SqlitePool }

#[async_trait]
impl StoryRepository for SqliteStoryRepository {
    async fn create(&self, story: &Story) -> Result<(), DomainError> { ... }
}
```

**依赖注入配置**（按职责分组）：
```rust
// agentdash-api/src/app_state.rs
pub struct AppState {
    pub repos: RepositorySet,         // 8 个 Repository trait 对象
    pub services: ServiceSet,         // executor_hub, connector, address_space, backend_registry 等
    pub task_runtime: TaskRuntime,    // lock_map, restart_tracker
    pub config: AppConfig,            // mcp_base_url 等配置
    pub remote_sessions: Arc<RwLock<HashMap<String, String>>>,
}
```

#### 添加新模块的步骤

1. **Domain Layer**：
   ```
   agentdash-domain/src/<module>/
   ├── entity.rs          # 实体（业务行为方法）
   ├── repository.rs      # Repository trait
   ├── value_objects.rs   # 值对象
   └── events.rs          # 领域事件
   ```

2. **Infrastructure Layer**：
   ```
   agentdash-infrastructure/src/persistence/sqlite/
   └── <module>_repository.rs   # impl Repository trait
   ```

3. **Interface Layer**（当前方式，简单业务）：
   - 在 `app_state.rs` 添加 `Arc<dyn Repository>`
   - 在 `routes/` 添加路由处理函数

4. **Application Layer**（复杂业务时）：
   - 创建 `agentdash-application/src/use_cases/<module>/`
   - 实现 `CreateXxxUseCase`, `UpdateXxxUseCase` 等
   - 路由改为调用 Use Case，而非直接操作 Repository

> **禁止跨层依赖**：API 层不能直接访问 Repository 的具体实现
> **Application 层规划**：当前简单业务逻辑直接写在路由中。当业务复杂时，必须提取到独立的 `agentdash-application` crate。

---

## 命名规范

### Crate 命名

- **领域层**：`agentdash-domain` — 核心业务
- **基础设施层**：`agentdash-infrastructure` — 技术实现
- **执行器层**：`agentdash-executor` — 连接器实现
- **接口层**：`agentdash-api` — HTTP API

### 文件命名

- **实体**：PascalCase，如 `Story`, `Task`, `BackendConfig`
- **Repository trait**：`<Entity>Repository`，如 `StoryRepository`
- **Repository 实现**：`<技术><Entity>Repository`，如 `SqliteStoryRepository`
- **值对象**：PascalCase 描述性名称，如 `StoryStatus`, `StoryId`

### 目录命名

```
agentdash-domain/src/<entity>/           # 小写，单数
├── entity.rs                          # 实体定义
├── repository.rs                      # Repository trait
└── value_objects.rs                   # 值对象

agentdash-infrastructure/src/persistence/<db_type>/  # 如 sqlite/
└── <entity>_repository.rs                         # 如 story_repository.rs
```
---

### 架构演进记录

> 详细的架构演进历史已拆分到 [architecture-evolution.md](./architecture-evolution.md)。
>
> 摘要：2026-02-26 整洁架构 → 02-27 领域模型 → 03-20 API/Application 解耦 → 03-27 God Module 分解 → 03-29 Agent Types 抽取
