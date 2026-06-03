# 实施计划

## Checklist

- [x] 读取后端数据库规范和 migration runner，确认 schema 事实源。
- [x] 基于旧迁移链在空库上生成当前 PostgreSQL schema。
- [x] 完成表级/字段级/baseline 策略调研。
- [x] 删除旧 migration 文件，创建新的初始化 `0001_init.sql`。
- [x] 清理默认 embedded PostgreSQL 开发库目录。
- [x] 更新数据库规范，记录预研期 baseline squash 的正向策略。
- [x] 运行后端/migration 相关验证。

## Research Outputs

- `research/table-usage-audit.md`
- `research/column-usage-audit.md`
- `research/baseline-strategy.md`
- `research/old-chain-final-schema.sql`
- `research/old-chain-columns.csv`
- `research/new-baseline-schema.raw.sql`

## Validation Results

- 新临时 embedded PostgreSQL 空库运行新 `0001_init.sql` 成功。
- `_sqlx_migrations` 成功记录 1 条 migration，version 为 `1`。
- public 业务表数量为 55。
- `public.tasks` 不存在。
- `public.runtime_session_execution_anchors` 存在。
- `public.agent_frame_transitions` 存在。
- `pgcrypto` extension 未创建，初始化 SQL 不依赖数据库端 UUID/crypto 函数。

## Baseline Decisions

- 新 `0001_init.sql` 与 `REQUIRED_POSTGRES_TABLES` 对齐为 55 张表。
- 新 `0001_init.sql` 不创建旧独立 `tasks` 表；Task 继续由 `stories.tasks` JSONB 承载。
- 新 `0001_init.sql` 不创建旧中间态表：`agents`、`project_agent_links`、`workflow_assignments`、`session_bindings`、`workflow_definitions`、`lifecycle_definitions`、`lifecycle_run_links`、`skill_asset_files`、`project_filespaces`、`project_vfs_mount_bindings`。
- `agent_frame_transitions` 和 `runtime_session_execution_anchors` 是现行依赖表，已纳入全局 readiness。

## Validation Commands

- `cargo check -p agentdash-infrastructure`
- `cargo check -p agentdash-api`
- 必要时使用临时 PostgreSQL 数据根启动 migration runner，验证新 `0001` 能创建 readiness 所需表。

## Risky Files and Rollback Points

- `crates/agentdash-infrastructure/migrations/*.sql`：删除历史 migration 前先依赖 git 保留可恢复状态。
- `.agentdash/embedded-postgres/agentdash-server`：清理的是本地开发数据库，执行前需要停止相关 embedded PostgreSQL 进程。
- `.trellis/spec/backend/database-guidelines.md`：只记录策略原因和当前基线，不写任务过程流水。
