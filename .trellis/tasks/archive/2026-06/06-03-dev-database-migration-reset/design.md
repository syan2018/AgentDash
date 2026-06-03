# 开发期数据库与 Migration 历史重置设计

## Architecture and Boundaries

PostgreSQL schema 的事实源继续保持在 `crates/agentdash-infrastructure/migrations/`。本任务只重置该事实源的历史表达方式：把开发期累积的增量 migration 压缩为新的初始化 migration，不改变 API bootstrap、repository readiness、embedded PostgreSQL runtime 的职责边界。

`agentdash-api` 启动顺序保持不变：

1. `PostgresRuntime::resolve` 连接外部或 embedded PostgreSQL。
2. `run_postgres_migrations` 执行 SQLx migration runner。
3. `assert_postgres_schema_ready` 检查必需表。
4. repository bootstrap 在已迁移 schema 上装配持久化实现。

## Data Flow and Contracts

- 空库启动时，SQLx 只看到新的 `0001_init.sql`。
- `0001_init.sql` 创建扩展、表、主键、唯一约束、外键和索引。
- 不再通过历史回填脚本迁移旧 JSON payload、旧 lifecycle 字段、旧 session binding 或旧命名。
- embedded PostgreSQL 的物理数据目录清理后，下一次 `pnpm dev` 会重新创建数据库并运行新的 `0001`。

## Migration Baseline Strategy

新 `0001_init.sql` 以当前运行代码读取/写入的表结构为准，而不是以旧 `0001_init.sql` 或旧迁移链为准。生成方式优先使用“旧迁移链在空库上跑完后的最终 schema dump”，再人工清理 SQLx migration bookkeeping、owner/ACL、环境相关语句和历史注释。

本次调研确认的新 baseline 规则：

- `tasks` 独立表不进入新基线；Task 当前由 Story aggregate 持有，物理存储为 `stories.tasks` JSONB。
- `agents`、`project_agent_links`、`workflow_assignments`、`session_bindings`、`workflow_definitions`、`lifecycle_definitions`、`lifecycle_run_links`、`skill_asset_files`、`project_filespaces`、`project_vfs_mount_bindings` 不进入新基线；这些表已经被当前 repository 事实源替代。
- `agent_frame_transitions` 进入新基线；它是 runtime capability transition fact，不是历史冗余。
- `runtime_session_execution_anchors` 进入新基线；当前 lifecycle anchor repository 已读写该表。
- 初始化 migration 只包含 schema、约束、索引和必要扩展，不写默认业务数据；Builtin / Plugin Shared Library assets 继续由启动期 seed 服务幂等同步。

预研期后续数据库状态管理采用阶段性 squash：

- 日常 schema 变更仍先新增递增 migration，便于开发者 review 当前改动。
- 当开发期历史链再次明显阻碍理解或空库初始化稳定后，可以执行一次 baseline squash，把当前正确 schema 收束成新的初始化 migration。
- 项目进入上线或真实数据保留阶段后，停止随意 squash，改为保留增量 migration 历史。

## Operational Notes

- 默认 API embedded 数据目录：`<AGENTDASH_DATA_ROOT 或仓库根目录>/.agentdash/embedded-postgres/agentdash_api`。
- 桌面 API embedded 数据目录：`<AGENTDASH_DATA_ROOT 或仓库根目录>/.agentdash/embedded-postgres/agentdash_desktop_api`。
- 本机 executor runtime 使用独立 service name `agentdash-local-<backend_id>`，数据根由本机 runtime 配置决定。
- 如果用户通过 `DATABASE_URL` 指向外部 PostgreSQL，需要用户明确提供目标连接串后才能 drop/recreate 或清空对应数据库。
- Windows 上清理 embedded 数据目录前需要先停止相关 `postgres.exe` 进程；现有 dev runtime 已有按 `.agentdash/embedded-postgres` 清理进程的逻辑。

## Rollback Shape

本任务属于开发期基线重置，不提供旧数据库升级回滚。代码层回滚方式是恢复旧 migration 文件和旧 embedded 数据目录备份；数据层回滚只适用于执行前保留过物理备份的开发者本机。
