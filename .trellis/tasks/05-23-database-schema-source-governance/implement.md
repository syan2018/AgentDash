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
