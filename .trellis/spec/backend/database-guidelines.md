# 数据库规范

> AgentDashboard 当前云端业务数据以 PostgreSQL + SQLx 持久化；本机端会话元数据仍使用 sqlite。

---

## 概览

| 项目 | 说明 |
|------|------|
| 云端业务数据库 | PostgreSQL |
| ORM / 访问层 | SQLx |
| 数据归属 | 业务数据归云端，执行状态归本机 |
| 建表方式 | 应用启动时 `initialize()` 自动建表 |
| 本机会话存储 | `SqliteSessionRepository` |

---

## 存储分层

### 云端（PostgreSQL）

业务数据统一通过 PostgreSQL Repository / Command Port 访问：

```rust
// agentdash-infrastructure/src/persistence/postgres/
project_repository.rs
workspace_repository.rs
story_repository.rs
state_change_repository.rs
state_change_store.rs
task_repository.rs
...
```

说明：

- `StoryRepository` 只负责 Story 聚合
- `StateChangeRepository` 独立负责事件日志
- `TaskAggregateCommandRepository` 对应 `PostgresTaskRepository` 中的显式事务方法
- `WorkspaceRepository` 负责 `workspaces` + `workspace_bindings` 的原子提交

### 本机（SQLite）

sqlite 不再承担云端业务仓储职责，仅保留本机会话持久化：

```rust
// agentdash-infrastructure/src/persistence/sqlite/
session_repository.rs
```

---

## SQLx 使用约定

- Repository 实现模式详见 [Repository Pattern](./repository-pattern.md)
- 基础设施层错误必须转换为 `DomainError`，不要把 `sqlx::Error` 直接泄露到上层

---

## 事务规则

### 单一聚合

当聚合只涉及单表或明确的 root + children 持久化时，事务边界由对应 Repository 负责。

例子：

- `WorkspaceRepository::create/update/delete`
- `WorkspaceRepository` 内部在同一事务中写 `workspaces` 与 `workspace_bindings`

### 跨聚合一致性

当一个用例需要同时更新多个聚合或事件日志时：

- 不要把行为硬塞进单一聚合 Repository trait
- 使用显式 Command Port 或 Unit of Work

当前例子：

- `TaskAggregateCommandRepository::create_for_story`
- `TaskAggregateCommandRepository::delete_for_story`

它们在一个事务中协调：

- `tasks`
- `stories.task_count`
- `state_changes`

---

## Shared Helper 约定

如果多个 PostgreSQL 仓储共享事件日志写入/查询逻辑，应抽到明确的 shared helper，而不是复制粘贴。

当前 shared 位置：

```rust
// agentdash-infrastructure/src/persistence/postgres/state_change_store.rs
initialize_state_changes_schema(...)
append_state_change(...)
append_state_change_in_tx(...)
get_state_changes_since(...)
latest_state_change_id(...)
```

适合抽 shared 的内容：

- 行数据 -> 领域对象映射
- payload 构造辅助
- 事务内追加事件日志
- 枚举 / 状态字符串转换

不适合抽 shared 的内容：

- 混杂多个聚合规则的“大一统基础仓储”
- 需要大量泛型才能表达的过度抽象

---

## 字段与序列化约定

- 数据库列名统一 `snake_case`
- JSON 序列化统一 `snake_case`
- 复杂值对象以 JSON 文本存入 `TEXT`
- 时间字段统一存 `TEXT`，读取时做健壮解析

示例：

```rust
.bind(serde_json::to_string(&story.context)?)
```

---

## Schema 变更与迁移（必读）

### 核心规则

**新增列 ≠ 只改 `CREATE TABLE IF NOT EXISTS`。**

`CREATE TABLE IF NOT EXISTS` 只在表**首次创建**时生效。如果数据库中**表已存在**，新增列不会被自动添加。必须同时提供显式的迁移语句。

### PostgreSQL（sqlx migrate）

在 `crates/agentdash-infrastructure/migrations/` 下新建递增编号的 `.sql` 文件：

```sql
-- 0004_sessions_title_source.sql
ALTER TABLE sessions
ADD COLUMN IF NOT EXISTS title_source TEXT NOT NULL DEFAULT 'auto';
```

sqlx migrate 在应用启动时自动执行尚未应用的迁移。

### SQLite（手动 initialize）

SQLite 不支持 `ADD COLUMN IF NOT EXISTS` 语法。在对应 Repository 的 `initialize()` 方法中，于 `CREATE TABLE IF NOT EXISTS` 之后追加 `ALTER TABLE ADD COLUMN`，并**忽略错误**（列已存在时 SQLite 会报 duplicate column error）：

```rust
let _ = sqlx::query("ALTER TABLE sessions ADD COLUMN title_source TEXT NOT NULL DEFAULT 'auto'")
    .execute(&self.pool)
    .await;
```

### Checklist

每次为已有表添加新列时，必须完成以下步骤：

- [ ] 更新 `CREATE TABLE IF NOT EXISTS` 语句（保证新建库的 schema 完整）
- [ ] **PostgreSQL**: 新增 `migrations/NNNN_xxx.sql` 迁移文件
- [ ] **SQLite**: 在 `initialize()` 中追加 `ALTER TABLE ADD COLUMN`（忽略 duplicate 错误）
- [ ] 更新所有 `INSERT`/`SELECT`/`UPSERT` 语句和 `map_*_row` 映射函数
- [ ] 更新所有手动构造该 struct 的测试代码

---

## 常见错误

| 错误 | 正确 |
|------|------|
| 在 `StoryRepository` 中读写 `state_changes` | 拆到 `StateChangeRepository` |
| 在 `TaskRepository` 中暴露跨聚合事务 API | 拆到 `TaskAggregateCommandRepository` |
| `Workspace` root 与 bindings 分开提交 | 同事务提交 |
| `postgres/` 目录仍保留 `Sqlite*Repository` 命名 | 统一改为 `Postgres*Repository` |
| 为兼容旧结构保留整套 sqlite 业务仓储 | 预研阶段直接收敛到正确实现 |

---

## 相关规范

- [Repository Pattern](./repository-pattern.md)
- [Quality Guidelines](./quality-guidelines.md)
- [Error Handling](./error-handling.md)

---

*更新：2026-04-01 — 对齐 PostgreSQL 主业务仓储、独立 StateChange port 与显式事务型 command port*
