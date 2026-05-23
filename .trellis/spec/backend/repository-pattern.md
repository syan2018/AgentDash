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

Session runtime persistence 不通过 `RepositorySet` 表达。`SessionPersistence`、session event record、terminal effect outbox record 与 runtime command record 定义在 `agentdash-spi::session_persistence`，由 application 组合成 runtime stores，由 infrastructure 提供 PostgreSQL / SQLite adapter。这样 session runtime 的持久化事实可以跨 cloud/local adapter 复用，而基础设施层不需要依赖 application 编排 crate。

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

## Schema Ownership

PostgreSQL repository 实现假设业务 schema 已由 migration runner 初始化。Repository 的职责是持久化聚合、维护事务边界和映射领域错误；schema 变更通过 `crates/agentdash-infrastructure/migrations/` 进入。API repository bootstrap 在构造 repository set 前统一执行 schema readiness 检查，原因是 schema 事实源应集中在 migration 链，而不是分散在每个 repository 的启动路径里。

本机 SQLite session repository 是独立缓存存储，可以在 `initialize()` 中拥有本机缓存表结构。这个规则只适用于本机会话缓存，不扩展到云端 PostgreSQL repository。
