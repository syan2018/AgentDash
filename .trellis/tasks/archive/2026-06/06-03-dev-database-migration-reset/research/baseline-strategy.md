# Research: baseline-strategy

- Query: 设计开发期 PostgreSQL migration baseline 策略，给出干净 0001_init.sql 生成建议，并判断 seed/default data 是否进入 migration。
- Scope: mixed
- Date: 2026-06-03

## Findings

### Files Found

- `.trellis/tasks/06-03-dev-database-migration-reset/prd.md`: 任务要求删除开发期 PostgreSQL migration 历史，只保留表达当前正确 schema 的初始化 migration，并清理默认 embedded PostgreSQL 开发库。
- `.trellis/tasks/06-03-dev-database-migration-reset/design.md`: 设计明确 API 启动顺序保持 `PostgresRuntime::resolve -> run_postgres_migrations -> assert_postgres_schema_ready -> repository bootstrap`，新基线优先从旧链跑完后的最终 schema dump 生成。
- `.trellis/tasks/06-03-dev-database-migration-reset/implement.md`: 实施计划要求确认数据库规范和 migration runner，生成新 0001，清理 embedded DB，更新数据库规范并运行后端/migration 验证。
- `.trellis/spec/backend/database-guidelines.md`: PostgreSQL schema 事实源是 `crates/agentdash-infrastructure/migrations/`；repository 只观察已迁移 schema，不创建表、补列、建索引或执行 schema 数据迁移。
- `.trellis/spec/backend/shared-library.md`: Shared Library builtin/plugin asset 由 seed registry 在启动期物化，不在资源模块中单独 bootstrap；版本/digest 不变量必须在 seed/startup fail-fast。
- `.trellis/spec/backend/session/session-startup-pipeline.md`: session runtime 启动依赖完整 repository 和 lifecycle/runtime 表，普通自由会话也归入 LifecycleRun 过程模型。
- `.trellis/spec/cross-layer/desktop-local-runtime.md`: 桌面/本机 runtime 复用 API 与 embedded PostgreSQL 路径，开发脚本负责启动和清理残留 runtime。
- `crates/agentdash-infrastructure/src/migration.rs`: SQLx migration runner 和 schema readiness 表清单。
- `crates/agentdash-infrastructure/src/postgres_runtime.rs`: `DATABASE_URL` 外部 PostgreSQL 与 embedded PostgreSQL 解析、数据目录、进程复用/清理逻辑。
- `crates/agentdash-api/src/lib.rs`: API build_server 启动时先 resolve DB，再跑 migration 和 readiness，再构建 AppState。
- `crates/agentdash-api/src/bootstrap/repositories.rs`: repository bootstrap 构建所有 PostgreSQL repositories，并在 schema ready 后同步 builtin/plugin Shared Library assets。
- `crates/agentdash-api/src/app_state.rs`: AppState 构造插件注册、repository bootstrap、LLM Provider 密钥 codec、session runtime 和 background workers。
- `crates/agentdash-api/src/plugins.rs`: 插件注册统一收集 `library_asset_seeds` 和 auth provider，重复 auth provider fail-fast。
- `crates/agentdash-application/src/shared_library/service.rs`: builtin/plugin Shared Library seed 的幂等 upsert、版本/digest 校验和 removed builtin deprecated 逻辑。
- `crates/agentdash-infrastructure/src/persistence/postgres/llm_provider_repository.rs`: LLM provider 与 user credential repository 只读写表，不 seed 默认 provider。
- `crates/agentdash-infrastructure/src/secret.rs`: LLM provider 主密钥来自 `AGENTDASH_SECRET_KEY` 或 `<data_root>/.agentdash/secrets/llm-provider-master-key` 文件。
- `crates/agentdash-local/src/runtime.rs`: 本机 executor runtime 使用独立 embedded PostgreSQL 数据根，启动后跑同一套 migrations，只初始化 session repository。
- `scripts/dev-runtime.js`: `pnpm dev` 通过脚本启动 server/local/frontend；未传 `DATABASE_URL` 时删除 env 使用 embedded PostgreSQL，并清理残留 embedded postgres 进程/锁文件。
- `crates/agentdash-infrastructure/migrations/*.sql`: 当前历史链为 `0001_init.sql` 到 `0101_backfill_session_lifecycle_shells.sql`，包含大量开发期 schema 演进、重命名、回填和兼容清理。

### Code Patterns

- API migration 顺序是固定主线：`PostgresRuntime::resolve` 后调用 `run_postgres_migrations` 和 `assert_postgres_schema_ready`，再 `AppState::new_with_plugins`，见 `crates/agentdash-api/src/lib.rs:112`、`crates/agentdash-api/src/lib.rs:118`、`crates/agentdash-api/src/lib.rs:121`、`crates/agentdash-api/src/lib.rs:125`。
- SQLx runner 使用 `sqlx::migrate!("./migrations")`，迁移文件在编译期收集 metadata；squash 后需要重新编译相关 crate，见 `crates/agentdash-infrastructure/src/migration.rs:60`。
- readiness 只检查表存在，不检查列/索引；新 0001 必须至少覆盖 `REQUIRED_POSTGRES_TABLES`，见 `crates/agentdash-infrastructure/src/migration.rs:4` 和 `crates/agentdash-infrastructure/src/migration.rs:69`。
- 外部数据库仅在 `DATABASE_URL` 为 `postgres://` 或 `postgresql://` 时启用，非 PostgreSQL URL 直接失败，见 `crates/agentdash-infrastructure/src/postgres_runtime.rs:140`。
- 默认 embedded PostgreSQL 数据目录是 `<AGENTDASH_DATA_ROOT 或 cwd>/.agentdash/embedded-postgres/<service_name>/data`，见 `crates/agentdash-infrastructure/src/postgres_runtime.rs:44`、`crates/agentdash-infrastructure/src/postgres_runtime.rs:58`、`crates/agentdash-infrastructure/src/postgres_runtime.rs:63`。
- API 默认 service name 是 `agentdash_api`，desktop API 是 `agentdash_desktop_api`，见 `crates/agentdash-api/src/lib.rs:47` 和 `crates/agentdash-api/src/lib.rs:55`。
- 本机 executor runtime 使用 `agentdash-local-<backend_id>` 作为 embedded service name，并使用系统本机 runtime data dir，不是仓库根 `.agentdash`，见 `crates/agentdash-local/src/runtime.rs:452`、`crates/agentdash-local/src/runtime.rs:457`、`crates/agentdash-local/src/runtime.rs:547`。
- `scripts/dev-runtime.js` 从 `process.env.DATABASE_URL` 或 `--database-url` 读取外部数据库；启动 server 时如果不是 PostgreSQL URL 会删除 `DATABASE_URL`，见 `scripts/dev-runtime.js:161` 和 `scripts/dev-runtime.js:691`。
- `scripts/dev-runtime.js` 的启动前清理只处理进程和 lock file，不删除 data 目录，见 `scripts/dev-runtime.js:498`、`scripts/dev-runtime.js:632`、`scripts/dev-runtime.js:655`。
- Shared Library builtin seed 在 repository bootstrap 中运行，代码路径为 `SharedLibraryService::seed_builtin_assets(Default::default())`，见 `crates/agentdash-api/src/bootstrap/repositories.rs:69` 和 `crates/agentdash-api/src/bootstrap/repositories.rs:72`。
- Plugin embedded Shared Library seed 在同一 bootstrap 后段运行，代码路径为 `SharedLibraryService::seed_plugin_embedded_assets(plugin_library_asset_seeds)`，见 `crates/agentdash-api/src/bootstrap/repositories.rs:172` 和 `crates/agentdash-api/src/bootstrap/repositories.rs:175`。
- Plugin host 只收集 `library_asset_seeds`，不直接写 DB，见 `crates/agentdash-api/src/plugins.rs:98` 和 `crates/agentdash-api/src/plugins.rs:105`。
- Shared Library seed 保留稳定 identity，builtin 使用 `scope=builtin/source=builtin/source_ref=builtin:{asset_type}:{key}`，plugin 使用 `scope=system/source=plugin_embedded/source_ref=plugin:{plugin}:{asset_type}:{key}`，见 `crates/agentdash-application/src/shared_library/service.rs:49` 和 `crates/agentdash-application/src/shared_library/service.rs:107`。
- LLM Provider 表由 repository 写入，当前未发现启动期默认 provider seed；credential 数据为用户配置，不应进入 migration，见 `crates/agentdash-infrastructure/src/persistence/postgres/llm_provider_repository.rs:112` 和 `crates/agentdash-infrastructure/src/persistence/postgres/llm_provider_repository.rs:313`。
- LLM provider 主密钥属于本地/环境配置，不是数据库 seed，见 `crates/agentdash-infrastructure/src/secret.rs:20` 和 `crates/agentdash-infrastructure/src/secret.rs:124`。
- Auth/session/settings 初始化都以 repository/service 写入运行时数据：`auth_sessions` repository 写会话，`settings` repository 写 KV，`users/groups/group_memberships` 由 auth provider 身份同步；这些都不是 schema baseline 数据。
- 历史 migration 中存在数据回填和兼容写入，例如 `0044_project_agents.sql` 从旧 `project_agent_links` 回填，`0052/0054` 回填 VFS mounts，`0101_backfill_session_lifecycle_shells.sql` 为既有 sessions 生成 freeform lifecycle shell；这些是旧开发库收敛逻辑，不适合作为空库 baseline seed。

### Should 0001 Include Default Data?

推荐：新的 `0001_init.sql` 只建 schema、约束和索引，不写默认业务数据。

理由：

- 项目处于预研期，目标是表达当前正确 schema；默认数据随代码、插件、版本/digest 规则演进，写入 migration 会把可变产品内容冻结为 schema 历史。
- Shared Library builtin/plugin assets 已有启动期幂等 seed 与版本/digest fail-fast 机制，代码路径是 `crates/agentdash-api/src/bootstrap/repositories.rs:69`、`crates/agentdash-api/src/bootstrap/repositories.rs:172`、`crates/agentdash-application/src/shared_library/service.rs:49`、`crates/agentdash-application/src/shared_library/service.rs:107`。
- LLM provider、auth session、settings、user directory、backend registration、runtime health、session/lifecycle runtime facts 都是用户配置或运行时数据；空库应保持空，由对应 API/use case/repository 写入。
- 历史 `0101` 中的 `builtin.freeform_agent` / `builtin.freeform_session` 插入是为了给已有 sessions 补 lifecycle shell。新空库没有旧 sessions，不需要 migration 预置这些行；自由会话应由当前 dispatch/lifecycle 创建路径生成运行时事实。

需要由代码 seed 或运行时生成的数据路径：

- Builtin Shared Library assets: `crates/agentdash-api/src/bootstrap/repositories.rs:69` -> `crates/agentdash-application/src/shared_library/service.rs:49`。
- Plugin embedded Shared Library assets: `crates/agentdash-api/src/plugins.rs:98` -> `crates/agentdash-api/src/bootstrap/repositories.rs:172` -> `crates/agentdash-application/src/shared_library/service.rs:107`。
- Auth user/group directory: `crates/agentdash-api/src/auth.rs` 调用 user directory repository 同步 authenticated identity；schema 表为 `users`、`groups`、`group_memberships`。
- Settings: `crates/agentdash-api/src/routes/settings.rs` 通过 `PostgresSettingsRepository` 写入；schema 表为 `settings`。
- LLM providers and credentials: `crates/agentdash-infrastructure/src/persistence/postgres/llm_provider_repository.rs` 通过用户/API 写入；密钥 codec 在 `crates/agentdash-infrastructure/src/secret.rs`。
- Backend/local runtime registration: `scripts/dev-runtime.js` 先 ensure/claim，再由 API repositories 写 `backends`、`runtime_health`、`project_backend_access`、`backend_workspace_inventory` 等运行时事实。
- Session/lifecycle/runtime facts: `SessionLaunchService` / lifecycle dispatch 写入 `sessions`、`session_events`、`lifecycle_runs`、`agent_frames`、`agent_assignments`、`runtime_session_execution_anchors` 等。

### 0001_init.sql Organization Recommendation

推荐用单文件分段，注释保持结构性，不写历史说明：

1. Extensions and type baseline
   - `CREATE EXTENSION IF NOT EXISTS pgcrypto;` 仅当 0001 内直接使用 `gen_random_uuid()` 或 UUID 默认值时需要。若所有 UUID 由 Rust 生成并 bind，可以不依赖 pgcrypto。当前历史 `0101` 使用 `pgcrypto/gen_random_uuid()` 是回填旧数据用途；新空库不一定需要。
   - 不建议创建 PostgreSQL enum type；现有 Rust repository 多以 TEXT + domain parser + CHECK 表达枚举，baseline 继续保持这个风格更贴近当前代码。

2. Core business tables
   - `projects`
   - `project_subject_grants`
   - `workspaces`
   - `workspace_bindings`
   - `stories`
   - `state_changes`
   - `project_agents`
   - `project_vfs_mounts`
   - `canvases`
   - `canvas_files`
   - `canvas_bindings`
   - 注意：旧 `tasks`、`agents`、`project_agent_links`、`workflow_definitions`、`lifecycle_definitions`、`workflow_assignments`、`session_bindings` 不在 readiness 清单中，且后续 migrations 已迁移/删除/重命名；新 0001 不应保留这些旧表，除非当前 repository 仍直接读写。

3. Backend/runtime registration tables
   - `backends`
   - `runtime_health`
   - `backend_execution_leases`
   - `project_backend_access`
   - `backend_workspace_inventory`
   - `user_preferences`
   - `views`

4. Session/runtime tables
   - `sessions`
   - `session_events`
   - `session_terminal_effects`
   - `session_runtime_commands`
   - `agent_frame_transitions`
   - `session_compactions`
   - `session_projection_segments`
   - `session_projection_heads`
   - `session_lineage`
   - `permission_grants`
   - 这里应让 FK 顺序满足依赖：`sessions` 先于 dependent tables，`agent_frame_transitions` 先于 `session_runtime_commands.transition_id` FK。

5. Workflow/lifecycle tables
   - `agent_procedures`
   - `workflow_graphs`
   - `activity_execution_claims`
   - `lifecycle_runs`
   - `lifecycle_workflow_instances`
   - `lifecycle_agents`
   - `agent_frames`
   - `agent_assignments`
   - `lifecycle_subject_associations`
   - `lifecycle_gates`
   - `agent_lineages`
   - `runtime_session_execution_anchors`
   - `routine_executions`
   - `routines`
   - 建表顺序按 FK：definitions -> runs -> graph instances/agents -> frames/assignments/associations/gates/lineages -> execution anchors。

6. Asset/plugin/shared library tables
   - `library_assets`
   - `mcp_presets`
   - `skill_assets`
   - `inline_fs_files`
   - `extension_package_artifacts`
   - `project_extension_installations`
   - `llm_providers`
   - `llm_provider_user_credentials`
   - `library_assets.payload` 保持 JSONB，因为 Shared Library spec 明确该边界允许 JSONB 灵活 payload。其它复杂值对象若 repository 当前按字符串序列化/解析，应保持 TEXT。

7. Settings/auth/directory tables
   - `settings`
   - `auth_sessions`
   - `users`
   - `groups`
   - `group_memberships`

8. Indexes and constraints
   - 主键、唯一约束和 CHECK 尽量内联在 `CREATE TABLE` 中。
   - FK 可以内联，但建议在每个分段结束后统一创建跨段 FK，避免顺序缠绕。
   - 所有 repository 查询使用的 `WHERE` / `ORDER BY` 路径应保留索引，尤其是 session event `(session_id,event_seq)`、runtime command status/session、backend lease active、workflow run/activity、library asset identity、project scoped key、LLM credential `(provider_id,user_id)`。
   - 如果沿用 `CREATE TABLE IF NOT EXISTS` / `CREATE INDEX IF NOT EXISTS`，新空库可重复运行更宽容；但 SQLx migration 已用 `_sqlx_migrations` 控制一次性执行，baseline SQL 可以选择更严格的 `CREATE TABLE`。预研期建议保留 `IF NOT EXISTS`，方便本地修复残缺库时诊断更平滑，但不要用它掩盖列不一致。

### Clean 0001 Generation Recommendation

首选生成方式：

1. 准备一个临时 PostgreSQL 空库，不使用默认 embedded 开发库。
2. 在临时库上运行当前完整旧 migration 链到 `0101`。
3. 用 `pg_dump --schema-only --no-owner --no-privileges --schema=public` 导出最终 schema。
4. 从 dump 中删除 `_sqlx_migrations` 表和相关 insert/sequence；SQLx 会在新库运行新 `0001` 时自行维护 `_sqlx_migrations`。
5. 删除 owner/ACL、环境相关 `SET`、dump 噪声注释，按上述分组重排。
6. 删除旧兼容表、旧重命名中间态和历史回填逻辑，只保留当前 readiness/repository 需要的表、约束、索引。
7. 对比 `REQUIRED_POSTGRES_TABLES`、`Postgres*Repository::initialize()` 和 repository SQL 的列名，补齐 dump 整理时遗漏的列/索引。
8. 用新空库运行新 `0001`，再跑 `assert_postgres_schema_ready` 和后端 `cargo check`。

备选生成方式：

- 逐个阅读 repository `COLUMNS` 常量和 `INSERT/UPDATE/SELECT` 拼出 schema。该方式能减少历史噪声，但容易漏掉索引、CHECK、FK 和 runtime-only 表；只适合作为 dump 后的核对步骤。

### Development Migration Management

推荐策略：

- 日常改表仍新增短期递增 migration，例如 `0002_add_x.sql`，便于 review 单次 schema 意图和让不同开发者空库/已有开发库都能收敛。
- 当迁移链再次明显分散当前 schema，或一个阶段性功能集合稳定后，执行 squash：用最新完整链在空库跑完后的 schema dump 生成新的 `0001_init.sql`，删除旧短期 migrations。
- 项目进入需要保留真实用户/团队数据的阶段后停止随意 squash，改为保留增量历史。
- squash 后必须清理默认 embedded DB 数据目录，原因是现有库的 `_sqlx_migrations` 记录旧 checksum/版本；只替换 migration 文件会导致 SQLx bookkeeping 与物理库不一致。
- 默认 API embedded 库路径：`<AGENTDASH_DATA_ROOT 或仓库根>/.agentdash/embedded-postgres/agentdash_api/data`。
- desktop API embedded 库路径：`<AGENTDASH_DATA_ROOT 或仓库根>/.agentdash/embedded-postgres/agentdash_desktop_api/data`。
- web profile 本机 executor runtime 还有独立 embedded 库，路径在 Windows 通常是 `%APPDATA%\AgentDash\local-runtime\.agentdash\embedded-postgres\agentdash-local-<backend_id>\data` 或 `%LOCALAPPDATA%\AgentDash\local-runtime\...`，由 `crates/agentdash-local/src/runtime.rs:547` 决定。
- 清理 embedded DB 前先停止 `pnpm dev`、`agentdash-server`、`agentdash-local`、`agentdash-local-tauri` 和相关 `postgres.exe`。`scripts/dev-runtime.js` 可杀进程/锁文件，但不会删除 data 目录。
- 对外部 `DATABASE_URL`：不要扫描或猜测用户机器上的 PostgreSQL。只有调用方明确提供目标 `DATABASE_URL` 时，才对该连接串指向的数据库执行 drop/recreate、truncate 或迁移重置。推荐最稳妥做法是让开发者手动新建空库并把 `DATABASE_URL` 指向新库，而不是在工具里自动 drop。

### Risk Points

- SQLx migrate bookkeeping: `_sqlx_migrations` 保存已执行 migration version/checksum。新 0001 替换旧链后，任何未清空的数据库都会和新 metadata 冲突或被判定状态不一致。embedded DB 必须删除物理 data 目录或重新创建数据库；外部 `DATABASE_URL` 必须明确重建目标库。
- Compile-time migration metadata: `sqlx::migrate!("./migrations")` 在编译期收集迁移文件。删除/新增 migration 后需要重新编译 `agentdash-infrastructure`/API binary；旧运行进程必须停止。
- `pgcrypto`: 只有新 0001 需要数据库端 `gen_random_uuid()` 时才创建扩展。若 UUID 继续由 Rust 生成并以 TEXT/UUID bind，避免引入不必要扩展依赖。当前历史 `0101` 的 `pgcrypto` 是回填旧 session shell 使用。
- Timestamp type: 旧 `0069` 将所有 `*_at` 文本时间列收敛为 `TIMESTAMPTZ`。新 0001 应直接使用 `TIMESTAMPTZ NOT NULL DEFAULT now()/CURRENT_TIMESTAMP` 或 nullable `TIMESTAMPTZ`，匹配 repository 的 `chrono::DateTime<Utc>` 读写；不要回到 TEXT timestamp。
- JSON TEXT/JSONB: 数据库规范说复杂值对象以 JSON 文本存入 `TEXT`，但 Shared Library spec 明确 `LibraryAsset.payload` 是 JSONB。建议仅对需要数据库 JSON 运算/灵活 payload 的边界使用 JSONB，例如 `library_assets.payload`、历史上已由 repository 作为 JSONB 读取的 `stories.tasks` 等；普通 domain payload 继续 TEXT。
- Foreign key strictness: baseline 是表达当前正确 schema 的机会，可以严格化稳定生命周期依赖，例如 session dependent tables 对 `sessions(id)`、asset installation 对 `library_assets(id)`、project-scoped resources 对 `projects(id)`。但若运行时存在跨进程/异步 outbox 或先写 anchor 后补 association 的流程，应以当前 repository 写入顺序为准，避免用过严 FK 破坏合法中间状态。
- Readiness coverage gap: `assert_postgres_schema_ready` 只查表，不查列和索引。新 0001 必须用 repository SQL 和 integration 验证补齐列/索引，否则 readiness 绿但运行时失败。
- Removed tables: 旧 `tasks`、`agents`、`project_agent_links`、`workflow_definitions`、`lifecycle_definitions`、`workflow_assignments`、`session_bindings` 等在历史中存在，但不在当前 readiness 清单。新 baseline 应以当前 repository 和 readiness 为准，避免把废弃表重新固定进 schema。

### External References

- `Cargo.toml:44`: SQLx version `0.8`，features 包含 `postgres`、`sqlite`、`chrono`、`uuid`、`json`。
- `Cargo.toml:45`: embedded PostgreSQL crate `postgresql_embedded = "0.20"`。
- `Cargo.toml:59`: `chrono = "0.4"` with serde。
- `Cargo.toml:60`: `uuid = "1.0"` with v4/serde。
- SQLx migrate behavior reference: `sqlx::migrate!` embeds migration metadata at compile time and records applied migrations in `_sqlx_migrations`; this matches the local runner in `crates/agentdash-infrastructure/src/migration.rs:60`.
- PostgreSQL extension reference: `pgcrypto` provides `gen_random_uuid()` on supported PostgreSQL versions; only required when SQL calls that function.

### Related Specs

- `.trellis/spec/backend/database-guidelines.md`
- `.trellis/spec/backend/repository-pattern.md`
- `.trellis/spec/backend/shared-library.md`
- `.trellis/spec/backend/session/session-startup-pipeline.md`
- `.trellis/spec/backend/workflow/architecture.md`
- `.trellis/spec/backend/capability/llm-model-config.md`
- `.trellis/spec/cross-layer/desktop-local-runtime.md`
- `.trellis/spec/cross-layer/shared-library-contract.md`

## Caveats / Not Found

- 未发现 LLM provider 的启动期默认 provider seed；当前路径看起来由用户/API 创建 provider，migration 只应建 `llm_providers` 和 `llm_provider_user_credentials` schema。
- 未发现 auth/session/settings 需要 migration 插入默认数据；这些都是运行时或用户配置路径。
- 未直接运行临时 PostgreSQL dump；本研究给出生成策略和代码/schema 边界，不产出最终 SQL。
- 旧 migrations 中存在很多历史回填 insert/update；它们代表已有开发库收敛路径，不代表新空库 baseline 应包含的数据。
