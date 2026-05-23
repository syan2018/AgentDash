# Database Schema 事实源决策

## Goal

明确 PostgreSQL migrations、repository `initialize()`、SQLite 本机会话缓存初始化之间的 schema 事实源契约，为后续移除重复 DDL 或保留双轨初始化提供稳定依据。

## Confirmed Facts

- `.trellis/spec/backend/database-guidelines.md` 当前要求 PostgreSQL 新增 migration，同时更新 `CREATE TABLE IF NOT EXISTS` 保证新建库完整。
- SQLite 当前在 `initialize()` 中追加 `ALTER TABLE`，并忽略 duplicate column。
- 外部 review 建议 PostgreSQL 走 migrations-only，但这与当前 spec 基线存在差异，不能直接作为实现任务执行。

## Requirements

- 先形成决策文档和 spec 更新，不直接大规模删除 repository DDL。
- 明确 PostgreSQL 云端业务库、SQLite 本机会话库、测试库和 embedded/local dev 场景的初始化职责。
- 评估 migrations-only、repository bootstrap、hybrid 三种策略的风险、收益和迁移路径。
- 给出可执行后续任务：是否 migration squash、是否删除 Postgres runtime DDL、如何保留 SQLite 初始化。
- 更新 `.trellis/spec/backend/database-guidelines.md` 与 repository pattern 中的 schema 变更规则。

## Acceptance Criteria

- [ ] 形成 schema 事实源设计结论，并记录为什么采用该策略。
- [ ] PostgreSQL 与 SQLite 的初始化职责被明确区分。
- [ ] spec 更新后不再同时表达互相冲突的 schema 来源。
- [ ] 若决定 migrations-only，输出后续安全迁移 checklist。
- [ ] 若保留 hybrid，说明 repository DDL 的边界和禁止事项。

## Out of Scope

- 本任务不直接删除所有 repository DDL。
- 本任务不做真实数据迁移。
- 本任务不修改已提交 migration 历史。
