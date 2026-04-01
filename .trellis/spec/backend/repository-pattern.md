# Repository Pattern

> 本项目使用 Repository / Command Port 模式实现领域持久化与显式事务边界。

---

## Overview

Repository 模式的目标不是“把所有数据库操作都塞进 trait”，而是让接口语义直接对应聚合边界：

- **领域层**定义单一聚合 Repository Port
- **领域层**对跨聚合一致性定义显式 Command Port / 事务边界 Port
- **基础设施层**实现 PostgreSQL 持久化与事务
- **应用层**编排多个 Port，避免在 API 层直接组织复杂持久化流程

当前主链路规则：

- `StoryRepository` 只负责 `Story` 聚合 CRUD / 查询
- `StateChangeRepository` 独立承载 `state_changes` 事件日志
- `TaskRepository` 只负责 `Task` 聚合 CRUD / 查询
- `TaskAggregateCommandRepository` 显式承载 “Task + Story.task_count + StateChange” 的事务边界
- `WorkspaceRepository` 负责 `Workspace` root + `bindings` 的原子提交

---

## Signatures

### 单一聚合 Repository

```rust
#[async_trait]
pub trait StoryRepository: Send + Sync {
    async fn create(&self, story: &Story) -> Result<(), DomainError>;
    async fn get_by_id(&self, id: Uuid) -> Result<Option<Story>, DomainError>;
    async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<Story>, DomainError>;
    async fn update(&self, story: &Story) -> Result<(), DomainError>;
    async fn delete(&self, id: Uuid) -> Result<(), DomainError>;
}

#[async_trait]
pub trait TaskRepository: Send + Sync {
    async fn create(&self, task: &Task) -> Result<(), DomainError>;
    async fn get_by_id(&self, id: Uuid) -> Result<Option<Task>, DomainError>;
    async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<Task>, DomainError>;
    async fn list_by_story(&self, story_id: Uuid) -> Result<Vec<Task>, DomainError>;
    async fn list_by_workspace(&self, workspace_id: Uuid) -> Result<Vec<Task>, DomainError>;
    async fn update(&self, task: &Task) -> Result<(), DomainError>;
    async fn update_status(&self, id: Uuid, status: TaskStatus) -> Result<(), DomainError>;
    async fn delete(&self, id: Uuid) -> Result<(), DomainError>;
}
```

### 独立事件日志 Port

```rust
#[async_trait]
pub trait StateChangeRepository: Send + Sync {
    async fn get_changes_since(&self, since_id: i64, limit: i64)
        -> Result<Vec<StateChange>, DomainError>;
    async fn get_changes_since_by_project(&self, project_id: Uuid, since_id: i64, limit: i64)
        -> Result<Vec<StateChange>, DomainError>;
    async fn latest_event_id(&self) -> Result<i64, DomainError>;
    async fn latest_event_id_by_project(&self, project_id: Uuid) -> Result<i64, DomainError>;
    async fn append_change(
        &self,
        project_id: Uuid,
        entity_id: Uuid,
        kind: ChangeKind,
        payload: serde_json::Value,
        backend_id: Option<&str>,
    ) -> Result<(), DomainError>;
}
```

### 显式事务边界 Port

```rust
#[async_trait]
pub trait TaskAggregateCommandRepository: Send + Sync {
    async fn create_for_story(&self, task: &Task) -> Result<(), DomainError>;
    async fn delete_for_story(&self, task_id: Uuid) -> Result<Task, DomainError>;
}
```

这个 port 不是“纯聚合仓储”，而是一个被明确命名出来的命令型事务边界：

- 允许基础设施层在一个事务里同时写 `tasks` / `stories.task_count` / `state_changes`
- 避免把跨聚合行为伪装成 `TaskRepository` 的自然职责

---

## Infrastructure

当前业务持久化主实现统一收敛在 PostgreSQL：

```rust
// agentdash-infrastructure/src/persistence/postgres/
PostgresProjectRepository
PostgresWorkspaceRepository
PostgresStoryRepository
PostgresStateChangeRepository
PostgresTaskRepository
...
```

约定：

- `<技术><实体>Repository` 结构体名必须反映真实后端
- 不允许在 `postgres/` 目录继续保留 `Sqlite*Repository` 历史残名
- 若多个 Repository / Command Port 由同一实现体承载，可以让一个 struct 同时实现多个 trait

当前例子：

```rust
pub struct PostgresTaskRepository {
    pool: PgPool,
}

#[async_trait]
impl TaskRepository for PostgresTaskRepository {
    // 纯 Task CRUD
}

#[async_trait]
impl TaskAggregateCommandRepository for PostgresTaskRepository {
    // create_for_story / delete_for_story
}
```

---

## RepositorySet

`RepositorySet` 定义在 `agentdash-application`，用于在 application / gateway / service 之间传递 Port 集合：

```rust
pub struct RepositorySet {
    pub project_repo: Arc<dyn ProjectRepository>,
    pub workspace_repo: Arc<dyn WorkspaceRepository>,
    pub story_repo: Arc<dyn StoryRepository>,
    pub state_change_repo: Arc<dyn StateChangeRepository>,
    pub task_repo: Arc<dyn TaskRepository>,
    pub task_command_repo: Arc<dyn TaskAggregateCommandRepository>,
    // ...
}
```

原则：

- API 层持有 `AppState { repos, services, ... }`
- 应用层优先接收 `&RepositorySet` 或具体 trait
- MCP / Hook / Task 生命周期入口也只依赖 trait 对象

---

## Rules

### 1. 单一聚合 Port 不混入事件日志职责

错误：

- 在 `StoryRepository` 中追加 `append_change`
- 在 `TaskRepository` 中追加 `latest_event_id`

正确：

- 事件流统一放进 `StateChangeRepository`

### 2. 单一聚合 Port 不编码跨聚合更新

错误：

- `TaskRepository::create_task_with_story_update`
- `TaskRepository::delete_task_with_story_update`

正确：

- 单独引入 `TaskAggregateCommandRepository`
- 或显式引入 Unit of Work / 事务边界 Port

### 3. 聚合整体持久化必须原子

错误：

- `Workspace` root 先提交，再单独删除/重建 `bindings`

正确：

- `WorkspaceRepository` 在同一事务里写 `workspaces` 与 `workspace_bindings`

### 4. 预研阶段优先“正确状态”，不保留虚假的双实现对等关系

当前策略：

- 云端业务仓储统一以 PostgreSQL 为主
- sqlite 只保留本机端 `SqliteSessionRepository`

---

## Good / Bad

### Good

```rust
pub async fn create_task_aggregate(
    task_command_repo: &dyn TaskAggregateCommandRepository,
    task: &Task,
) -> Result<(), DomainError> {
    task_command_repo.create_for_story(task).await
}
```

### Bad

```rust
#[async_trait]
pub trait TaskRepository: Send + Sync {
    async fn create(&self, task: &Task) -> Result<(), DomainError>;
    async fn create_task_with_story_update(&self, task: &Task) -> Result<(), DomainError>;
}
```

问题：

- 一个 Port 同时表达纯聚合持久化和跨聚合事务
- 调用方看名字无法分辨边界
- 后续容易继续往里塞更多 workflow 逻辑

---

## Tests

至少覆盖以下层次：

- 领域层：实体和值对象规则
- 基础设施层：Repository / Command Port 的 SQL 映射与事务行为
- 应用层：调用正确 Port，错误被正确映射

对事务型 port，重点测试：

- `create_for_story()` 成功时 `task_count` 与 `state_changes` 一致更新
- 中途失败时事务整体回滚
- `delete_for_story()` 不会留下孤儿状态

---

## Related

- [Directory Structure](./directory-structure.md)
- [Database Guidelines](./database-guidelines.md)
- [Error Handling](./error-handling.md)
