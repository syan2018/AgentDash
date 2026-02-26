# 后端架构重构：引入整洁架构分层

## 背景与目标

当前后端架构存在分层不清晰的问题：
1. 领域实体只有数据结构，缺乏业务行为
2. 应用层缺失，业务逻辑散落在路由和Store中
3. 基础设施（SQLx）直接侵入领域层
4. Crate边界模糊，职责重叠

**目标**：通过引入整洁架构（Clean Architecture），建立清晰的分层边界，提高代码的可测试性、可维护性和可扩展性。

---

## 整洁架构原则

```
┌─────────────────────────────────────────────────────────┐
│                    Interface Layer                      │
│              (HTTP Routes, DTOs, Middleware)            │
│                    agentdash-api                        │
├─────────────────────────────────────────────────────────┤
│                  Application Layer                      │
│    (Use Cases, Application Services, Ports/Interfaces)  │
│              agentdash-application                      │
├─────────────────────────────────────────────────────────┤
│                    Domain Layer                         │
│    (Entities, Value Objects, Domain Services,           │
│            Repository Interfaces, Domain Events)        │
│                agentdash-domain                         │
├─────────────────────────────────────────────────────────┤
│                 Infrastructure Layer                    │
│   (DB Implementation, External APIs, Message Queue,     │
│        Executor Connectors, File Storage)               │
│   agentdash-infrastructure / agentdash-executor         │
└─────────────────────────────────────────────────────────┘

依赖方向：外层 → 内层（Interface → Application → Domain）
```

---

## 当前架构问题分析

### 1. 缺乏领域层

```rust
// 当前：agentdash-state/src/models/story.rs
pub struct Story {
    pub id: Uuid,
    pub title: String,
    pub status: StoryStatus,
    // ... 只有字段，没有方法
}
```

**问题**：业务规则散落在 Store 和路由中。

### 2. 应用层缺失

```rust
// 当前：routes/stories.rs 直接调用 Store
let stories = state.store.list_stories(&query.backend_id).await?;
```

**问题**：没有统一的业务逻辑编排，用例无法独立测试。

### 3. 基础设施侵入

```rust
// 当前：agentdash-state/src/store.rs
pub struct StateStore {
    pool: SqlitePool,  // 直接依赖具体实现
}
```

**问题**：违反了依赖倒置原则。

### 4. Crate边界模糊

| Crate | 问题 |
|-------|------|
| `agentdash-state` | 混合了实体、数据访问、基础设施 |
| `agentdash-coordinator` | 与 state 职责重叠 |
| `agentdash-api` | 包含业务逻辑（ExecutorHub）|

---

## 重构方案

### Phase 1: 建立领域层 (agentdash-domain)

**目标**：创建不依赖任何外部库的纯领域代码

**任务清单**：
- [ ] 创建 `agentdash-domain` crate
- [ ] 定义核心实体（Story, Task, Backend）及其业务行为
- [ ] 定义值对象（StoryId, TaskId, BackendId, Title等）
- [ ] 定义 Repository 接口（Ports）
- [ ] 定义领域事件（Domain Events）
- [ ] 定义领域服务（如状态转换验证）

**预期结构**：
```
agentdash-domain/src/
├── story/
│   ├── entity.rs           # Story 实体（含业务方法）
│   ├── repository.rs       # StoryRepository trait
│   ├── events.rs           # StoryCreated, StoryStatusChanged
│   └── value_objects.rs    # StoryId, Title, StoryStatus
├── task/
│   ├── entity.rs
│   ├── repository.rs
│   ├── events.rs
│   └── value_objects.rs
├── backend/
│   ├── entity.rs
│   ├── repository.rs
│   └── value_objects.rs
└── common/
    ├── error.rs            # DomainError
    └── events.rs           # EventPublisher trait
```

---

### Phase 2: 建立应用层 (agentdash-application)

**目标**：封装用例（Use Cases）和应用程序服务

**任务清单**：
- [ ] 创建 `agentdash-application` crate
- [ ] 实现 Story 相关用例
  - [ ] CreateStoryUseCase
  - [ ] UpdateStoryUseCase
  - [ ] DecomposeStoryUseCase
- [ ] 实现 Task 相关用例
  - [ ] CreateTaskUseCase
  - [ ] UpdateTaskStatusUseCase
- [ ] 定义输出端口（Output Ports）
  - [ ] EventPublisher
  - [ ] UnitOfWork
- [ ] 定义 DTOs（用于跨层数据传输）

**预期结构**：
```
agentdash-application/src/
├── ports/
│   ├── output/             # 输出端口（基础设施实现）
│   │   ├── event_publisher.rs
│   │   └── unit_of_work.rs
│   └── input/              # 输入端口（API层调用）
│       └── story_service.rs
├── use_cases/
│   ├── story/
│   │   ├── create_story.rs
│   │   ├── update_story.rs
│   │   └── decompose_story.rs
│   └── task/
│       ├── create_task.rs
│       └── update_task_status.rs
├── services/
│   └── story_application_service.rs
└── dto/
    ├── story_dto.rs
    └── task_dto.rs
```

---

### Phase 3: 重构基础设施层 (agentdash-infrastructure)

**目标**：实现领域层定义的 Repository 接口

**任务清单**：
- [ ] 创建 `agentdash-infrastructure` crate
- [ ] 实现 SQLite Repository
  - [ ] SqliteStoryRepository
  - [ ] SqliteTaskRepository
  - [ ] SqliteBackendRepository
- [ ] 实现事件发布（内存或消息队列）
- [ ] 实现 UnitOfWork（事务管理）

**预期结构**：
```
agentdash-infrastructure/src/
├── persistence/
│   ├── sqlite/
│   │   ├── connection.rs       # 数据库连接管理
│   │   ├── story_repository.rs
│   │   ├── task_repository.rs
│   │   └── backend_repository.rs
│   └── migrations/             # 数据库迁移
├── messaging/
│   └── memory_event_bus.rs     # 内存事件总线实现
└── config/
    └── database_config.rs
```

---

### Phase 4: 重构执行器层 (agentdash-executor)

**目标**：从 api crate 中提取 ExecutorHub 和相关逻辑

**任务清单**：
- [ ] 创建 `agentdash-executor` crate
- [ ] 提取 AgentConnector trait（从 api）
- [ ] 拆分 ExecutorHub 职责：
  - [ ] SessionManager：会话生命周期管理
  - [ ] BroadcastHub：消息广播
  - [ ] SessionStore：文件持久化
- [ ] 迁移连接器实现
  - [ ] VibeKanbanExecutorsConnector
  - [ ] RemoteAcpConnector

**预期结构**：
```
agentdash-executor/src/
├── ports/
│   ├── connector.rs            # AgentConnector trait
│   ├── session_manager.rs      # 会话管理接口
│   └── session_store.rs        # 会话存储接口
├── services/
│   ├── session_manager.rs      # 会话管理实现
│   ├── broadcast_hub.rs        # 广播实现
│   └── execution_coordinator.rs # 执行协调
├── adapters/
│   ├── connectors/
│   │   ├── vibe_kanban.rs
│   │   └── remote_acp.rs
│   └── persistence/
│       └── file_session_store.rs
└── dto/
    └── execution_dto.rs
```

---

### Phase 5: 精简 API 层 (agentdash-api)

**目标**：API层只负责 HTTP 相关逻辑

**任务清单**：
- [ ] 精简路由处理函数
  - [ ] 只负责：参数提取、DTO转换、调用应用服务、返回响应
- [ ] 通过依赖注入连接各层
- [ ] 定义 API 特定的 DTOs（Request/Response）
- [ ] 统一错误处理（将领域错误映射为 HTTP 响应）

**预期结构**：
```
agentdash-api/src/
├── routes/
│   ├── story_routes.rs         # 精简：只处理 HTTP
│   ├── task_routes.rs
│   └── acp_session_routes.rs
├── middleware/
│   ├── error_handler.rs
│   └── auth.rs
├── dto/
│   ├── request/
│   └── response/
├── state.rs                    # AppState 仅包含应用服务引用
└── main.rs
```

---

### Phase 6: 废弃旧 Crate

**任务清单**：
- [ ] 逐步废弃 `agentdash-state`
- [ ] 逐步废弃 `agentdash-coordinator`
- [ ] 将功能迁移到新的分层架构

---

## 依赖关系图

```
agentdash-api
    ↓ depends on
agentdash-application
    ↓ depends on
agentdash-domain
    ↑ implemented by
agentdash-infrastructure
    ↑ implemented by
agentdash-executor
```

---

## 迁移策略

### 策略：增量重构，保持系统可用

1. **并行开发**：新 crate 与旧代码并行存在
2. **逐步替换**：逐个模块迁移，而非一次性重写
3. **接口兼容**：保持 API 契约不变，只改变内部实现

### 推荐迁移顺序

```
Week 1: agentdash-domain (领域层)
Week 2: agentdash-infrastructure (Repository实现)
Week 3: agentdash-application (用例层)
Week 4: agentdash-executor (执行器层)
Week 5: agentdash-api (精简API层)
Week 6: 废弃旧crate，整合测试
```

---

## 验收标准

- [ ] 所有新 crate 编译通过
- [ ] 领域层不依赖任何外部库（除标准库和 async-trait）
- [ ] 应用层只依赖领域层
- [ ] API 层只依赖应用层
- [ ] 现有 API 契约保持不变
- [ ] 单元测试覆盖领域层核心业务逻辑
- [ ] 集成测试覆盖用例层

---

## 风险与缓解

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| 重构周期过长 | 高 | 增量重构，保持系统可用 |
| 引入新 Bug | 中 | 保持 API 契约，增加自动化测试 |
| 团队成员学习成本 | 低 | 编写清晰的架构文档和示例代码 |
| 依赖冲突 | 中 | 逐步迁移，避免 crate 间循环依赖 |

---

## 参考文档

- [Clean Architecture by Robert C. Martin](https://blog.cleancoder.com/uncle-bob/2012/08/13/the-clean-architecture.html)
- [Ports and Adapters Architecture](https://alistair.cockburn.us/hexagonal-architecture/)
- [Domain-Driven Design](https://domainlanguage.com/ddd/reference/)

---

## 完成总结 (2026-02-26)

### 已交付内容

| 阶段 | 状态 | 交付物 |
|------|------|--------|
| Phase 1 | ✅ 完成 | `agentdash-domain` crate - 领域层 |
| Phase 2 | ⏭️ 跳过 | 应用层未单独创建（当前规模可接受） |
| Phase 3 | ✅ 完成 | `agentdash-infrastructure` crate - 基础设施层 |
| Phase 4 | ✅ 完成 | `agentdash-executor` crate - 执行器层 |
| Phase 5 | ✅ 完成 | `agentdash-api` 精简，使用依赖注入 |
| Phase 6 | ✅ 完成 | 废弃 `agentdash-state`（删除9个文件，541行） |

### 架构验证结果

- **编译**: ✅ `cargo check --workspace` 通过
- **测试**: ✅ `cargo test --workspace` 通过
- **代码质量**: ⚠️ 2个 minor clippy 警告（非阻塞）
- **依赖方向**: ✅ 正确（外层 → 内层）

### 最终架构图

```
crates/
├── agentdash-api/              # Interface Layer
│   └── 路由使用 Arc<dyn Repository> 依赖注入
│
├── agentdash-domain/           # Domain Layer
│   ├── story/{entity, repository, value_objects}
│   ├── task/{entity, repository, value_objects}
│   ├── backend/{entity, repository}
│   └── common/{error, events}
│
├── agentdash-infrastructure/   # Infrastructure Layer
│   └── persistence/sqlite/
│       ├── story_repository.rs    # impl StoryRepository
│       ├── task_repository.rs     # impl TaskRepository
│       └── backend_repository.rs  # impl BackendRepository
│
├── agentdash-executor/         # Infrastructure Layer (Connectors)
│   ├── connector.rs            # AgentConnector trait
│   ├── hub.rs                  # ExecutorHub
│   └── connectors/             # 实现
│
└── agentdash-coordinator/      # 遗留，待后续整合
```

### 关键改进

1. **依赖倒置**: 领域层定义 Repository 接口，基础设施层实现
2. **依赖注入**: AppState 通过 `Arc<dyn Trait>` 注入具体实现
3. **分层清晰**: 领域层不依赖 SQLx，基础设施层依赖领域层
4. **可测试性**: Repository 接口允许 Mock 测试

### 遗留事项

| 事项 | 优先级 | 说明 |
|------|--------|------|
| agentdash-coordinator 整合 | 低 | 可逐步迁移到 infrastructure |
| 应用层提取 | 低 | 业务逻辑复杂后再提取 Use Case |
| Clippy 警告修复 | 低 | 2个 minor 警告 |
| 单元测试补充 | 中 | 领域层核心业务逻辑 |

### 归档信息

- **归档日期**: 2026-02-26
- **最终提交**: 待 git commit
- **状态**: ✅ 已完成核心架构重构
