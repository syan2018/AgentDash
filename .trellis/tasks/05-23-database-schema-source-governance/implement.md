# Database Schema 事实源决策 Implement

## Order

1. 盘点现状：
   - `crates/agentdash-infrastructure/migrations/`
   - `crates/agentdash-infrastructure/src/persistence/postgres/*`
   - `crates/agentdash-infrastructure/src/persistence/sqlite/*`
2. 分类 repository DDL：
   - `CREATE TABLE IF NOT EXISTS`
   - `ALTER TABLE ADD COLUMN`
   - index/constraint creation
3. 对照当前 spec，列出冲突点。
4. 写出 schema 事实源决策。
5. 更新 spec。
6. 若决策需要代码迁移，创建后续子任务。

## Validation

```powershell
rg -n "CREATE TABLE IF NOT EXISTS|ALTER TABLE|initialize\\(" crates/agentdash-infrastructure/src crates/agentdash-infrastructure/migrations
```

如果本任务包含 spec-only 修改，不需要运行全量测试；若同步修改代码，至少运行：

```powershell
cargo check -p agentdash-infrastructure -p agentdash-api
```

## Review Focus

- 区分 PostgreSQL 与 SQLite 生命周期。
- 不修改已提交 migration 历史。
- 设计结论要解释为什么当前阶段采用该策略。

## Progress

- 已盘点当前 schema 基线：PostgreSQL migration 文件 56 个，PostgreSQL repository 侧仍有广泛 runtime DDL。
- 已形成结论：PostgreSQL 云端业务库以 migrations 为 schema 事实源；SQLite 本机会话缓存以 `SqliteSessionRepository::initialize()` 为本机 cache 初始化事实源。
- 已更新 backend database / repository / architecture spec，停止把 PostgreSQL 新增 schema 同时写入 repository DDL 作为开发规则。
- 已输出后续 cleanup checklist：先确认 migration runner readiness，再迁移 repository-only DDL，最后将 Postgres repository `initialize()` 收敛为 readiness/no-op 或移除。
