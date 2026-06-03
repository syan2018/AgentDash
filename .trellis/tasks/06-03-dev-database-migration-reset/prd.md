# 开发期数据库与 Migration 历史重置

## Goal

在项目尚未上线、无需保留历史数据库兼容性的前提下，清空现有 PostgreSQL migration 历史和默认开发期数据库状态，建立一份表达当前正确 schema 的初始化 `0001` migration，让后续数据库变更重新从干净基线出发。

用户价值：

- 开发期 schema 事实集中在一份初始化 migration 中，避免一百多个历史迁移分散表达当前结构。
- 默认 embedded PostgreSQL 开发库可以从空库重新初始化，绕开已经损坏或不一致的迁移状态。
- 后续预研阶段可以周期性 squash migration，直到项目进入需要保留真实历史的阶段。

## Confirmed Facts

- 主业务数据库使用 PostgreSQL + SQLx；`Cargo.toml` 启用 `postgres` 与 `sqlite` feature。
- API 启动时通过 `agentdash_infrastructure::migration::run_postgres_migrations` 运行 `sqlx::migrate!("./migrations")`。
- PostgreSQL/embedded PostgreSQL 共用 `crates/agentdash-infrastructure/migrations/` 作为 schema 事实源。
- 当前迁移目录已有 `0001_init.sql` 到 `0101_backfill_session_lifecycle_shells.sql`，历史链过长且包含多次开发期重命名、回填和兼容性清理。
- 默认 embedded PostgreSQL 数据根为 `<AGENTDASH_DATA_ROOT 或当前目录>/.agentdash/embedded-postgres/<service_name>/data`，`pnpm dev` 未指定 `DATABASE_URL` 时使用 embedded PostgreSQL。
- 数据库规范要求 repository 不执行 PostgreSQL DDL；新增/删除表列索引等 schema 变化归 migrations。

## Requirements

- 删除现有 PostgreSQL migration 历史，只保留一份新的初始化 migration。
- 新 `0001_init.sql` 必须创建当前 repository/schema readiness 需要的所有业务表、索引、约束、扩展和基础默认值。
- 新初始化 migration 不保留上线兼容、历史回填、旧字段迁移或旧表重命名逻辑。
- 清理默认 embedded PostgreSQL 开发库状态，让下一次启动从新 migration 初始化空库。
- 保留外部 `DATABASE_URL` 的明确边界：只能在明确目标连接串时处理对应数据库，不能猜测或扫描用户机器上的其它 PostgreSQL 数据库。
- 更新数据库规范，说明预研期 migration squash 的节奏和原因。
- 验证 migration runner、schema readiness 和后端编译至少能通过基础检查。

## Acceptance Criteria

- [ ] `crates/agentdash-infrastructure/migrations/` 中只保留新的初始化 migration 文件。
- [ ] 新初始化 migration 可在空 PostgreSQL 数据库上完整运行。
- [ ] `assert_postgres_schema_ready` 需要的表全部由初始化 migration 创建。
- [ ] 默认 embedded PostgreSQL 开发数据目录已清理，或提供可执行的清理脚本/命令入口。
- [ ] 数据库规范记录预研期可周期性 squash migration 的正向理由。
- [ ] 至少运行一次后端编译或 migration 相关验证，确认代码不因 migration squash 失效。

## Notes

- 本任务不保留已有开发库数据。
- 本任务不为旧 migration 版本提供升级兼容路径。
- 本任务不处理无法从仓库确定位置的外部 PostgreSQL 数据库，除非调用方提供 `DATABASE_URL`。
