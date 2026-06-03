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

业务库的 schema 事实源是 `crates/agentdash-infrastructure/migrations/`。日常 schema 变更按正常 migration 链新增文件推进，原因是 migration 历史是仓库内可审计的结构演进事实，开发期本地库、测试库和 embedded PostgreSQL 都应观察同一条递进路径。

Repository 启动逻辑只观察已迁移 schema。API bootstrap 不调用 PostgreSQL repository schema 初始化；需要直接构造 `AppState` 或 repository 的测试路径也先运行 migrations，再执行 readiness 检查。Repository 可以保留无 DDL 的 readiness helper，但不能创建表、补列、建索引或执行 schema 数据迁移。

预研期允许定期压缩 PostgreSQL migration 基线。阶段性 squash 时整理 `0001_init.sql` 表达当前正确 schema，避免开发期重命名、回填和旧模型迁移长期分散当前事实。`0001_init.sql` 应保持为手工整理后的 schema baseline：只保留 DDL、约束、索引、序列和必要扩展，不保留 pg_dump header、object comments、`public.` 前缀噪音、回填默认值或旧约束命名。进入需要保留真实环境数据的阶段后，migration 历史转为增量审计事实，不再随意压缩。

初始化 migration 只表达 schema、约束、索引和必要扩展。Builtin / Plugin Shared Library assets、LLM Provider、auth session、settings、backend registration、runtime health、session / lifecycle runtime facts 都由启动期 seed、API use case 或 runtime repository 写入，原因是这些数据随代码、插件、用户配置或运行状态变化，不属于 schema 基线。

只有执行 migration squash 或替换基线后，embedded PostgreSQL 物理 data 目录需要重建。SQLx 通过 `_sqlx_migrations` 记录 migration version 和 checksum；替换 migration 文件后复用旧数据库会让 bookkeeping 与新基线不一致。外部 `DATABASE_URL` 指向的数据库只在调用方明确给出目标连接串和重建意图时处理。

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
- 阶段性 squash 后，基线 migration 与当前 schema 目标保持一致

---

## PL/pgSQL 迁移脚本要点

- `RAISE` 占位符是单个 `%`（不是 `%%`），参数数量必须与占位符数量一致
- `SELECT ... INTO` 后必须检查 `FOUND`
- JSONB 数组遍历用 `jsonb_array_elements()`，不用 `FOREACH ... IN ARRAY`
- 迁移脚本必须幂等：`ADD COLUMN IF NOT EXISTS`、`ON CONFLICT DO NOTHING`
