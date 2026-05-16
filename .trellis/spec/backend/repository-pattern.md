# Repository Pattern

> 领域层定义 Port（trait），基础设施层实现 PostgreSQL 持久化。

---

## 核心原则

- **领域层**定义单一聚合 Repository Port（如 `StoryRepository`、`WorkspaceRepository`）
- **基础设施层**提供 `Postgres*Repository` 实现
- **应用层**通过 `RepositorySet` 编排多个 Port
- Repository 接口语义直接对应聚合边界，不混入跨聚合事务

---

## RepositorySet

`RepositorySet` 定义在 `agentdash-application`，持有所有 Repository trait 对象。API 层通过 `AppState` 持有 `RepositorySet`，应用层优先接收 `&RepositorySet` 或具体 trait。

---

## 规则

### 1. 单一聚合 Port 不混入事件日志

事件流统一放进 `StateChangeRepository`，不要在其他 Repository 中追加 `append_change` 方法。

### 2. 聚合整体持久化必须原子

例如 `WorkspaceRepository` 在同一事务内写 `workspaces` 与 `workspace_bindings`。Story aggregate 的 Task 变更走 `StoryRepository::update` 整体写回（Task 已合入 `stories.tasks` JSONB）。

### 3. 跨聚合一致性使用显式 Command Port

需要同时更新多个聚合时，引入独立的命令型事务边界 Port 或 Unit of Work，不要把跨聚合行为伪装成 Repository 的自然职责。

### 4. 命名约定

- 结构体命名 `<技术><实体>Repository`（如 `PostgresStoryRepository`）
- 不在 `postgres/` 目录保留 `Sqlite*` 命名
- 一个 struct 可同时实现多个 trait

---

## 存储分层

- 云端业务仓储统一 PostgreSQL
- SQLite 仅保留本机端 `SqliteSessionRepository`
