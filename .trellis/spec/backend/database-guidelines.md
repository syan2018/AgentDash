# 数据库规范

> PostgreSQL + SQLx（云端），SQLite（本机会话元数据）。

---

## 存储分层

| 层 | 技术 | 职责 |
|----|------|------|
| 云端 | PostgreSQL + SQLx | 业务数据（Project/Story/Workspace/Session 等） |
| 本机 | SQLite | 本机会话持久化（`SqliteSessionRepository`） |

---

## 核心约定

- 基础设施层错误必须转换为 `DomainError`，不泄露 `sqlx::Error`
- PostgreSQL repository 统一通过 `persistence::postgres::db_err` / `sql_err_for` 映射 SQLx 错误，保留 NotFound、Conflict、Database 三类可映射语义
- 数据库列名和 JSON 序列化统一 `snake_case`
- 复杂值对象以 JSON 文本存入 `TEXT`
- 时间字段存 `TEXT`，读取时做健壮解析
- Repository 实现模式详见 [repository-pattern.md](./repository-pattern.md)
- API 启动在 repository 装配前运行 PostgreSQL migrations，并执行 schema readiness 检查。

---

## 事务规则

- **单一聚合**：事务边界由对应 Repository 负责（如 `WorkspaceRepository` 内部同事务写 `workspaces` + `workspace_bindings`）
- **跨聚合**：使用显式 Command Port 或 Unit of Work，不要硬塞进单一 Repository trait
- Story aggregate 的 Task CRUD 走 `StoryRepository::update` 整体写回

---

## Schema 事实源

### PostgreSQL

云端业务库的 schema 事实源是 `crates/agentdash-infrastructure/migrations/`。新增表、列、索引、约束、删除列和数据修正都通过递增编号 `.sql` 文件表达。已提交的 migration 文件保持稳定，原因是开发、测试、embedded Postgres 和部署环境需要观察同一条有序 schema 历史。

Repository 启动逻辑只观察已迁移 schema。API bootstrap 不调用 PostgreSQL repository schema 初始化；需要直接构造 `AppState` 或 repository 的测试路径也先运行 migrations，再执行 readiness 检查。Repository 可以保留无 DDL 的 readiness helper，但不能创建表、补列、建索引或执行 schema 数据迁移。

### SQLite

本机会话缓存的 schema 事实源是 `SqliteSessionRepository::initialize()`。它服务本机 runtime 的 per-user cache 生命周期，和云端业务数据库迁移链不同。SQLite 初始化可以创建本机会话表，并以幂等方式补齐本机缓存字段。

### Checklist

- [ ] PostgreSQL 新增 migration 文件（不修改已有的）
- [ ] PostgreSQL integration / bootstrap 路径通过 migration runner 初始化真实 schema
- [ ] SQLite 本机会话缓存字段在 `SqliteSessionRepository::initialize()` 中保持幂等初始化
- [ ] 更新 INSERT/SELECT/UPSERT 语句和 `map_*_row` 函数
- [ ] 更新测试代码

### 删除旧列

- Repository 主线不再读写旧列
- PostgreSQL 新增 migration 用 `DROP COLUMN IF EXISTS`
- SQLite 可保留历史列但 Repository 不依赖
- 历史 migration 不可修改

---

## PL/pgSQL 迁移脚本要点

- `RAISE` 占位符是单个 `%`（不是 `%%`），参数数量必须与占位符数量一致
- `SELECT ... INTO` 后必须检查 `FOUND`
- JSONB 数组遍历用 `jsonb_array_elements()`，不用 `FOREACH ... IN ARRAY`
- 迁移脚本必须幂等：`ADD COLUMN IF NOT EXISTS`、`ON CONFLICT DO NOTHING`
