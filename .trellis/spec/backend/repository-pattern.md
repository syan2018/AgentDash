# Repository Pattern

> 领域层定义 Port（trait），基础设施层实现 PostgreSQL 持久化。

---

## 核心原则

- **领域层**定义单一聚合 Repository Port（如 `StoryRepository`、`WorkspaceRepository`）
- **基础设施层**提供 `Postgres*Repository` 实现
- **应用层**通过具名 use-case deps struct 编排多个 Port
- Repository 接口语义直接对应聚合边界，不混入跨聚合事务

---

## Composition Root And Use-Case Dependencies

`RepositorySet` 定义在 `agentdash-application`，只作为 API composition root / bootstrap 的 repository 装配结果。`AppState` 可以持有 `RepositorySet` 以统一启动期 wiring，但业务 route helper、application service constructor 和 use case 不直接接收或转发全量 set。

跨 repository 用例使用具名 deps struct，例如 `AgentRunForkRepos`、`ProjectAgentRunStartRepos`、`DeliveryRuntimeSelectionRepositories` 或 workspace query deps。这样 constructor 签名表达真实依赖集合，测试 fixture 也必须显式构造该用例需要的 port。原因是全量 repository set 会把 service locator 伪装成业务依赖，难以判断某个用例是否跨越了不该跨的 aggregate。

Session runtime persistence 不通过 `RepositorySet` 表达。`SessionPersistence`、session event record、terminal effect outbox record 与 runtime command record 定义在 `agentdash-spi::session_persistence`，由 application 组合成 runtime stores，由 infrastructure 提供 PostgreSQL / SQLite adapter。这样 session runtime 的持久化事实可以跨 cloud/local adapter 复用，而基础设施层不需要依赖 application 编排 crate。

---

## 规则

### 1. 单一聚合 Port 不混入事件日志

事件流统一放进 `StateChangeRepository`，不要在其他 Repository 中追加 `append_change` 方法。

### 2. 聚合整体持久化必须原子

例如 `WorkspaceRepository` 在同一事务内写 `workspaces` 与 `workspace_bindings`。`LifecycleRunRepository` 在同一聚合边界内写 orchestrations、tasks 与 execution log；`StoryRepository` 只写 Story 自身字段与上下文。

### 3. 跨聚合一致性使用显式 Command Port

需要同时更新多个聚合时，引入独立的命令型事务边界 Port 或 Unit of Work，不要把跨聚合行为伪装成 Repository 的自然职责。

Story 页面展示 Task 时读取 Story projection；Task durable facts 的写命令落在 LifecycleRun aggregate mutation 或 Lifecycle application command 上。这样 Story context、Task plan facts 与 runtime execution projection 的事实源保持可解释。

### 4. Use-case deps 不从 RepositorySet 派生

业务模块不提供 `from_repository_set` adapter。composition root 负责把全量 repository set 拆成具名 deps struct，再传给 route helper 或 service constructor。允许的 residual 是 bootstrap/AppState wiring；一旦进入业务函数，参数名必须说明该用例需要什么能力。

### 5. 命名约定

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
