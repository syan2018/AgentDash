# 数据库规范

> PostgreSQL + SQLx（云端与本机嵌入式运行时）。

---

## 存储分层

| 层 | 技术 | 职责 |
|----|------|------|
| 云端 | PostgreSQL + SQLx | 业务数据（Project/Story/Workspace/Session 等） |
| 本机 | Embedded PostgreSQL + SQLx | 本机 session runtime 持久化与恢复 |

---

## 核心约定

- 基础设施层错误必须转换为 `DomainError`，不泄露 `sqlx::Error`
- PostgreSQL repository 统一通过 `persistence::postgres::db_err` / `sql_err_for` 映射 SQLx 错误，保留 NotFound、Conflict、Database 三类可映射语义
- 数据库列名和 JSON 序列化统一 `snake_case`
- 复杂值对象以 JSON 文本存入 `TEXT`
- 时间字段使用 PostgreSQL 原生 timestamp 类型，repository 直接 bind/read `chrono::DateTime<Utc>`
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

业务库的 schema 事实源是 `crates/agentdash-infrastructure/migrations/`。新增表、列、索引、约束、删除列和数据修正都通过递增编号 `.sql` 文件表达。预研阶段的历史 migration 同时承担干净库基线职责；当列类型这类基线 contract 改变时，历史建表语句和 forward migration 需要一起表达同一个目标 schema，原因是干净库初始化、已有开发库升级和 embedded Postgres 都应收敛到同一结构。

Repository 启动逻辑只观察已迁移 schema。API bootstrap 不调用 PostgreSQL repository schema 初始化；需要直接构造 `AppState` 或 repository 的测试路径也先运行 migrations，再执行 readiness 检查。Repository 可以保留无 DDL 的 readiness helper，但不能创建表、补列、建索引或执行 schema 数据迁移。

### 本机 Embedded PostgreSQL

本机 session runtime 使用 embedded PostgreSQL，并复用同一套 migration 与 readiness 检查。这样本机恢复路径和云端 session persistence 观察同一份 schema contract，避免为本机维护第二套 schema 演进机制。

### Checklist

- [ ] PostgreSQL 新增 migration 文件
- [ ] PostgreSQL integration / bootstrap / local embedded runtime 路径通过 migration runner 初始化真实 schema
- [ ] 更新 INSERT/SELECT/UPSERT 语句和 `map_*_row` 函数
- [ ] 更新测试代码

### 删除旧列

- Repository 主线不再读写旧列
- PostgreSQL 新增 migration 用 `DROP COLUMN IF EXISTS`
- 预研阶段的基线 migration 与 forward migration 保持同一目标 schema

---

## PL/pgSQL 迁移脚本要点

- `RAISE` 占位符是单个 `%`（不是 `%%`），参数数量必须与占位符数量一致
- `SELECT ... INTO` 后必须检查 `FOUND`
- JSONB 数组遍历用 `jsonb_array_elements()`，不用 `FOREACH ... IN ARRAY`
- 迁移脚本必须幂等：`ADD COLUMN IF NOT EXISTS`、`ON CONFLICT DO NOTHING`
