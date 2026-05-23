# Database Schema 事实源决策 Design

## Boundary

本任务是架构决策任务，先明确 schema 初始化契约，再决定是否创建后续实现任务。它应更新 spec，但不直接执行大规模 DDL 删除。

## Options

| 方案 | 含义 | 适用性 |
| --- | --- | --- |
| PostgreSQL migrations-only | Postgres schema 只由 migrations 管理，repository 不运行 DDL | 最清晰，但需要迁移现有 initialize 逻辑 |
| Hybrid bootstrap | migrations 是升级事实源，repository `CREATE TABLE` 保证空库可用 | 当前 spec 接近此方案，但来源重复 |
| Repository bootstrap | repository 负责 schema 补齐 | 简单但长期 drift 风险最高 |

SQLite 本机会话库可保留独立策略，因为它不是云端业务 schema，生命周期和迁移压力不同。

## Decision Output

设计结论需要回答：

- PostgreSQL 新建库由 migrations 还是 repository DDL 创建？
- repository `initialize()` 是否允许 `ALTER TABLE`？
- SQLite 是否继续在 `initialize()` 中管理轻量 schema 演进？
- 测试库如何初始化？
- migration squash 是否需要单独任务？

## Spec Update

更新：

- `.trellis/spec/backend/database-guidelines.md`
- `.trellis/spec/backend/repository-pattern.md`
- 必要时更新 `.trellis/spec/backend/architecture.md`
