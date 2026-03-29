# Repository Pattern

> 本项目使用 Repository 模式实现数据访问的抽象，遵循整洁架构的依赖倒置原则。

---

## Overview

Repository 模式将数据访问逻辑从领域逻辑中分离：
- **领域层**定义 Repository 接口（Port）
- **基础设施层**提供具体实现（Adapter）
- **应用层**通过 trait 对象使用，不依赖具体实现

---

## Signatures

### Repository Trait 定义（Domain Layer）

```rust
// agentdash-domain/src/project/repository.rs
#[async_trait]
pub trait ProjectRepository: Send + Sync {
    async fn create(&self, project: &Project) -> Result<(), DomainError>;
    async fn get_by_id(&self, id: Uuid) -> Result<Option<Project>, DomainError>;
    async fn list_all(&self) -> Result<Vec<Project>, DomainError>;
    async fn update(&self, project: &Project) -> Result<(), DomainError>;
    async fn delete(&self, id: Uuid) -> Result<(), DomainError>;
}

// agentdash-domain/src/workspace/repository.rs
#[async_trait]
pub trait WorkspaceRepository: Send + Sync {
    async fn create(&self, workspace: &Workspace) -> Result<(), DomainError>;
    async fn get_by_id(&self, id: Uuid) -> Result<Option<Workspace>, DomainError>;
    async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<Workspace>, DomainError>;
    async fn update(&self, workspace: &Workspace) -> Result<(), DomainError>;
    async fn update_status(&self, id: Uuid, status: WorkspaceStatus) -> Result<(), DomainError>;
    async fn delete(&self, id: Uuid) -> Result<(), DomainError>;
}

// agentdash-domain/src/story/repository.rs
#[async_trait]
pub trait StoryRepository: Send + Sync {
    async fn create(&self, story: &Story) -> Result<(), DomainError>;
    async fn get_by_id(&self, id: Uuid) -> Result<Option<Story>, DomainError>;
    async fn list_by_backend(&self, backend_id: &str) -> Result<Vec<Story>, DomainError>;
    async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<Story>, DomainError>;
    async fn update(&self, story: &Story) -> Result<(), DomainError>;
    async fn delete(&self, id: Uuid) -> Result<(), DomainError>;
    async fn get_changes_since(&self, since_id: i64, limit: i64) -> Result<Vec<StateChange>, DomainError>;
    async fn latest_event_id(&self) -> Result<i64, DomainError>;
}

// agentdash-domain/src/task/repository.rs
#[async_trait]
pub trait TaskRepository: Send + Sync {
    async fn create(&self, task: &Task) -> Result<(), DomainError>;
    async fn get_by_id(&self, id: Uuid) -> Result<Option<Task>, DomainError>;
    async fn list_by_story(&self, story_id: Uuid) -> Result<Vec<Task>, DomainError>;
    async fn list_by_workspace(&self, workspace_id: Uuid) -> Result<Vec<Task>, DomainError>;
    async fn update(&self, task: &Task) -> Result<(), DomainError>;
    async fn update_status(&self, id: Uuid, status: TaskStatus) -> Result<(), DomainError>;
    async fn delete(&self, id: Uuid) -> Result<(), DomainError>;
}
```

### Repository 实现（Infrastructure Layer）

```rust
// agentdash-infrastructure/src/persistence/sqlite/story_repository.rs
use sqlx::SqlitePool;
use agentdash_domain::story::{Story, StoryRepository};
use agentdash_domain::common::error::DomainError;

pub struct SqliteStoryRepository {
    pool: SqlitePool,
}

impl SqliteStoryRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        // 创建表...
    }
}

#[async_trait]
impl StoryRepository for SqliteStoryRepository {
    async fn create(&self, story: &Story) -> Result<(), DomainError> {
        // SQLx 实现...
    }

    async fn list_by_backend(&self, backend_id: &str) -> Result<Vec<Story>, DomainError> {
        // SQLx 实现...
    }

    // ...
}
```

### 依赖注入配置

Repository 的集合通过 `RepositorySet`（定义于 `agentdash-application::repository_set`）统一管理，
`AppState` 持有 `RepositorySet` 而非散列的 `Arc<dyn XxxRepository>` 字段。

```rust
// agentdash-application/src/repository_set.rs
pub struct RepositorySet {
    pub project_repo: Arc<dyn ProjectRepository>,
    pub workspace_repo: Arc<dyn WorkspaceRepository>,
    pub story_repo: Arc<dyn StoryRepository>,
    pub task_repo: Arc<dyn TaskRepository>,
    pub backend_repo: Arc<dyn BackendRepository>,
    // ... 其他 repo ...
}

// agentdash-api/src/app_state.rs
pub struct AppState {
    pub repos: RepositorySet,       // 所有 repo 的集合
    pub executor_hub: ExecutorHub,
    pub connector: Arc<dyn AgentConnector>,
    // ...
}
```

`RepositorySet` 定义在 application 层而非 api 层，使得 application 层的服务
（hooks、gateway helpers、workspace resolution 等）可以直接接收 `&RepositorySet` 参数，
而不需要依赖 api 层的 `AppState`。

---

## Contracts

### Repository Trait 约定

| 元素 | 约定 | 说明 |
|------|------|------|
| Trait 名称 | `<Entity>Repository` | 如 `StoryRepository` |
| 父 trait | `Send + Sync` | 允许跨线程共享 |
| 属性宏 | `#[async_trait]` | 支持异步方法 |
| 错误类型 | `DomainError` | 统一领域错误 |
| 参数 | `&self` 或 `&mut self` | 不可变或可变借用 |

### 实现类约定

| 元素 | 约定 | 说明 |
|------|------|------|
| 结构体名称 | `<技术><Entity>Repository` | 如 `SqliteStoryRepository` |
| 构造函数 | `new(pool: SqlitePool)` | 接收连接池 |
| 初始化方法 | `initialize(&self)` | 创建表、索引等 |
| 错误转换 | 基础设施错误 → DomainError | 统一错误类型 |

---

## Validation & Error Matrix

| 条件 | 行为 | 错误 |
|------|------|------|
| 数据库连接失败 | 初始化失败 | `DomainError::InvalidConfig` |
| 实体不存在 | 返回空集合或 Option | `DomainError::NotFound` (查询单个时) |
| 违反唯一约束 | 创建失败 | `DomainError::AlreadyExists` |
| 序列化失败 | 操作失败 | `DomainError::InvalidData` |

---

## Good/Base/Bad Cases

### Good: 正确的分层使用

```rust
// agentdash-api/src/routes/stories.rs
pub async fn list_stories(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListStoriesQuery>,
) -> Result<Json<Vec<Story>>, ApiError> {
    // 通过 trait 对象调用，不依赖具体实现
    let stories = state.story_repo.list_by_backend(&query.backend_id).await?;
    Ok(Json(stories))
}
```

### Base: 依赖注入配置

```rust
// agentdash-application/src/repository_set.rs
pub struct RepositorySet {
    pub project_repo: Arc<dyn ProjectRepository>,
    pub workspace_repo: Arc<dyn WorkspaceRepository>,
    pub story_repo: Arc<dyn StoryRepository>,
    pub task_repo: Arc<dyn TaskRepository>,
    pub backend_repo: Arc<dyn BackendRepository>,
    // ... 其他 repo
}

// agentdash-api/src/app_state.rs
pub struct AppState {
    pub repos: RepositorySet,         // 所有 Repository 的集合
    pub services: ServiceSet,         // executor_hub, connector 等
    pub task_runtime: TaskRuntime,    // lock_map, restart_tracker
    pub config: AppConfig,            // mcp_base_url 等
}
```

### Bad: 错误的使用方式

```rust
// ❌ 不要这样做：路由直接依赖具体实现
use agentdash_infrastructure::SqliteStoryRepository;

pub async fn list_stories(
    State(state): State<Arc<SqliteStoryRepository>>,  // 错误：直接依赖实现
) -> Result<Json<Vec<Story>>, ApiError> {
    // ...
}

// ❌ 不要这样做：在领域层引入基础设施依赖
// agentdash-domain/src/story/entity.rs
use sqlx::SqlitePool;  // 错误：领域层不应依赖 SQLx

pub struct Story {
    // ...
}
```

---

## Tests Required

### 单元测试（领域层）

```rust
// agentdash-domain/src/story/entity.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_story_creation() {
        let story = Story::new(
            "backend-1".to_string(),
            "Test Title".to_string(),
            "Description".to_string(),
        );
        assert_eq!(story.status, StoryStatus::Created);
        assert_eq!(story.title, "Test Title");
    }
}
```

### 集成测试（基础设施层）

```rust
// agentdash-infrastructure/tests/story_repository_test.rs
#[tokio::test]
async fn test_story_repository() {
    let pool = setup_test_db().await;
    let repo = SqliteStoryRepository::new(pool);
    repo.initialize().await.unwrap();

    let story = Story::new("backend-1".to_string(), "Test".to_string(), "".to_string());
    repo.create(&story).await.unwrap();

    let stories = repo.list_by_backend("backend-1").await.unwrap();
    assert_eq!(stories.len(), 1);
}
```

### Mock 测试（应用层）

```rust
// 使用 mockall 生成 Mock
#[mockall::automock]
#[async_trait]
trait StoryRepository {
    async fn create(&self, story: &Story) -> Result<(), DomainError>;
    async fn list_by_backend(&self, backend_id: &str) -> Result<Vec<Story>, DomainError>;
}

#[tokio::test]
async fn test_use_case_with_mock() {
    let mut mock = MockStoryRepository::new();
    mock.expect_list_by_backend()
        .returning(|_| Ok(vec![]));

    // 使用 mock 测试业务逻辑...
}
```

---

## Wrong vs Correct

### Wrong: 混合分层

```rust
// ❌ 领域层混入基础设施代码
// agentdash-domain/src/story/entity.rs
use sqlx::SqlitePool;

pub struct Story {
    // ...
}

impl Story {
    pub async fn save(&self, pool: &SqlitePool) -> Result<(), sqlx::Error> {
        // 直接在实体中执行 SQL
        sqlx::query("INSERT ...")
            .execute(pool)
            .await
    }
}
```

**问题**：
- 领域层依赖 SQLx
- 无法单元测试
- 违反单一职责

### Correct: 清晰分层

```rust
// ✅ 领域层只定义接口
// agentdash-domain/src/story/repository.rs
#[async_trait]
pub trait StoryRepository: Send + Sync {
    async fn create(&self, story: &Story) -> Result<(), DomainError>;
}

// ✅ 实体只包含业务逻辑
// agentdash-domain/src/story/entity.rs
pub struct Story {
    pub id: Uuid,
    pub title: String,
    pub status: StoryStatus,
}

impl Story {
    pub fn new(backend_id: String, title: String, description: String) -> Self {
        // 业务逻辑...
    }
}

// ✅ 基础设施层实现接口
// agentdash-infrastructure/src/persistence/sqlite/story_repository.rs
pub struct SqliteStoryRepository { pool: SqlitePool }

#[async_trait]
impl StoryRepository for SqliteStoryRepository {
    async fn create(&self, story: &Story) -> Result<(), DomainError> {
        // SQLx 实现...
    }
}
```

**优势**：
- 领域层不依赖外部库
- 可替换存储实现（SQLite ↔ PostgreSQL）
- 易于单元测试

---

## Design Decisions

### 决策：使用 trait 对象而非泛型

**背景**：Repository 依赖注入的选择

**选项**：
1. 泛型：`AppState<R: StoryRepository>`
2. Trait 对象：`Arc<dyn StoryRepository>`

**决策**：使用 trait 对象 `Arc<dyn Repository>`

**原因**：
- 简化 AppState 类型签名
- 允许运行时切换实现（如测试 vs 生产）
- 与 Axum 的 State extractor 配合更好

**权衡**：
- 微小的运行时开销（虚表调用）
- 无法在编译时检查所有 Repository 实现

---

## Related

- [Directory Structure](./directory-structure.md) - 项目目录组织
- [Error Handling](./error-handling.md) - 错误处理约定
