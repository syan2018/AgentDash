# Directory Structure

> How backend code is organized in this project.

---

## Overview

<!--
Document your project's backend directory structure here.

Questions to answer:
- How are modules/packages organized?
- Where does business logic live?
- Where are API endpoints defined?
- How are utilities and helpers organized?
-->

<!-- PROJECT-SPECIFIC-START: AgentDashboard Backend Structure -->
> **AgentDashboard 后端代码的组织方式。**
> **注意：当前为概念阶段，技术栈未定，目录结构仅为参考设计。**

### 设计原则

按照项目的**策略可插拔**原则，目录结构应体现模块边界：
- 每个模块独立目录，模块间通过接口交互
- 接口定义与实现分离
- 策略（Strategy）作为可替换组件
<!-- PROJECT-SPECIFIC-END -->

---

## Directory Layout

```
<!-- Replace with your actual structure -->
src/
├── ...
└── ...
```

<!-- PROJECT-SPECIFIC-START: Directory Tree -->
### 实际目录布局（Rust / Axum）— 整洁架构分层

**架构原则**：依赖方向 外层 → 内层（Interface → Application → Domain）

```
crates/
├── agentdash-api/               # Interface Layer (HTTP)
│   └── src/
│       ├── main.rs              # Axum 启动入口
│       ├── app_state.rs         # 依赖注入配置（Arc<dyn Repository>）
│       ├── routes.rs            # 路由注册
│       ├── rpc.rs               # ApiError 统一错误处理
│       ├── stream.rs            # 全局事件流（SSE + NDJSON）
│       └── routes/              # 路由处理函数（只处理 HTTP）
│           ├── health.rs
│           ├── backends.rs
│           ├── stories.rs
│           ├── acp_sessions.rs
│           └── discovery.rs
│
├── agentdash-application/       # Application Layer (用例编排) — 规划中
│   └── src/
│       ├── lib.rs
│       ├── use_cases/           # 用例实现
│       │   ├── story/
│       │   │   ├── create_story.rs
│       │   │   └── decompose_story.rs
│       │   └── task/
│       │       └── create_task.rs
│       └── services/            # 应用服务
│           └── story_service.rs
│
├── agentdash-domain/            # Domain Layer (核心业务)
│   └── src/
│       ├── lib.rs
│       ├── common/
│       │   ├── error.rs         # DomainError
│       │   └── events.rs        # 领域事件 trait
│       ├── story/
│       │   ├── entity.rs        # Story 实体（含业务方法）
│       │   ├── repository.rs    # StoryRepository trait (Port)
│       │   └── value_objects.rs # StoryStatus, StoryId, etc.
│       ├── task/
│       │   ├── entity.rs
│       │   ├── repository.rs
│       │   └── value_objects.rs
│       └── backend/
│           ├── entity.rs
│           └── repository.rs
│
├── agentdash-infrastructure/    # Infrastructure Layer (技术实现)
│   └── src/
│       ├── lib.rs
│       └── persistence/
│           └── sqlite/
│               ├── mod.rs
│               ├── story_repository.rs    # impl StoryRepository
│               ├── task_repository.rs     # impl TaskRepository
│               └── backend_repository.rs  # impl BackendRepository
│
├── agentdash-executor/          # Infrastructure Layer (Connectors)
│   └── src/
│       ├── lib.rs
│       ├── connector.rs         # AgentConnector trait (Port)
│       ├── hub.rs               # ExecutorHub
│       ├── adapters/
│       │   └── normalized_to_acp.rs
│       └── connectors/
│           ├── mod.rs
│           ├── vibe_kanban.rs
│           └── remote_acp.rs
│
└── agentdash-coordinator/       # ⚠️ 遗留，待整合到 infrastructure
    └── src/
        ├── config.rs
        └── manager.rs           # CoordinatorManager（直接操作数据库）
```

### 关键 API 端点

| 路径 | 方法 | 说明 |
|------|------|------|
| `/api/agents/discovery` | GET | 执行器发现（返回连接器信息、执行器列表、权限策略） |
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
<!-- PROJECT-SPECIFIC-END -->

---

## Module Organization

<!-- How should new features/modules be organized? -->

<!-- PROJECT-SPECIFIC-START: Module Guidelines -->
### 整洁架构分层约定

**核心原则**：依赖方向始终向内，外层依赖内层。

```
Interface Layer (agentdash-api)
    ↓ depends on
Application Layer (agentdash-application) — 当前嵌入在 api 中
    ↓ depends on
Domain Layer (agentdash-domain)
    ↑ implemented by
Infrastructure Layer (agentdash-infrastructure, agentdash-executor)
```

#### 分层职责

| 分层 | Crate | 职责 | 允许依赖 | 状态 |
|------|-------|------|----------|------|
| **Interface** | `agentdash-api` | HTTP路由、DTO、中间件 | application, domain | ✅ 已创建 |
| **Application** | `agentdash-application` | 用例编排、事务管理、应用服务 | domain | ⏳ 规划中 |
| **Domain** | `agentdash-domain` | 实体、值对象、Repository接口、领域事件 | 无外部库（仅async-trait等基础库） | ✅ 已创建 |
| **Infrastructure** | `agentdash-infrastructure`, `agentdash-executor` | Repository实现、外部API、消息队列 | domain | ✅ 已创建 |

> **注意**：Application 层目前嵌入在 `agentdash-api` 中（简单业务逻辑直接写在路由中）。当业务逻辑复杂后，**必须**提取到独立的 `agentdash-application` crate 中。

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

**依赖注入配置**：
```rust
// agentdash-api/src/app_state.rs
pub struct AppState {
    pub story_repo: Arc<dyn StoryRepository>,     // trait 对象
    pub task_repo: Arc<dyn TaskRepository>,
    pub backend_repo: Arc<dyn BackendRepository>,
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
<!-- PROJECT-SPECIFIC-END -->

---

## Naming Conventions

<!-- File and folder naming rules -->

<!-- PROJECT-SPECIFIC-START: Naming Rules -->
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
<!-- PROJECT-SPECIFIC-END -->

---

## Examples

<!-- Link to well-organized modules as examples -->

<!-- PROJECT-SPECIFIC-START: Current Status -->
### 架构演进记录

**2026-02-26: 整洁架构重构完成**

从混合分层迁移到整洁架构：

| 旧架构 | 新架构 | 状态 |
|--------|--------|------|
| `agentdash-state` | `agentdash-domain` + `agentdash-infrastructure` | ✅ 已迁移 |
| `agentdash-coordinator` | （待整合到 infrastructure）| ⏭️ 遗留 |
| `agentdash-api/executor/` | `agentdash-executor` | ✅ 已提取 |

**关键变更**：
1. 引入 `agentdash-domain` crate，包含实体和 Repository traits
2. 引入 `agentdash-infrastructure` crate，实现 Repository 接口
3. 使用 `Arc<dyn Repository>` 在 AppState 中进行依赖注入
4. 废弃 `agentdash-state` crate（删除 9 个文件，541 行）

**依赖方向验证**：
```
cargo check --workspace  ✅ 通过
cargo test --workspace   ✅ 通过
```

**后续优化**：
- [ ] 创建 `agentdash-application` crate — 当业务逻辑复杂时提取 Use Case
- [ ] 整合 `agentdash-coordinator` 到 `agentdash-infrastructure`
- [ ] 补充领域层单元测试
- [ ] 修复 minor clippy 警告
<!-- PROJECT-SPECIFIC-END -->
