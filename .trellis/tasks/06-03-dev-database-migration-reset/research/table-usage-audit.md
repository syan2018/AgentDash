# Research: PostgreSQL table usage audit

- Query: 完整盘点 PostgreSQL schema 中表级别的当前实际使用情况，识别可从新 0001 初始化 migration 中移除的历史冗余表。
- Scope: internal
- Date: 2026-06-03

## Findings

### Files Found

- `.trellis/tasks/06-03-dev-database-migration-reset/prd.md` - migration 历史重置的目标、约束和验收标准。
- `.trellis/tasks/06-03-dev-database-migration-reset/design.md` - 新 0001 以当前运行代码读写的 schema 为准，不保留历史回填和旧命名。
- `.trellis/tasks/06-03-dev-database-migration-reset/implement.md` - 执行计划要求确认 schema 事实源、生成新 init migration、验证后端。
- `.trellis/workflow.md` - Trellis research 产物必须写入任务目录 research 文件。
- `.trellis/spec/backend/database-guidelines.md` - PostgreSQL schema 事实源是 `crates/agentdash-infrastructure/migrations/`，repository 不执行 DDL。
- `.trellis/spec/backend/repository-pattern.md` - Repository 假设 schema 已由 migration runner 初始化；Story aggregate 的 Task CRUD 写回 `stories.tasks` JSONB。
- `.trellis/spec/backend/story-task-runtime.md` - Task 无独立表；runtime truth 走 Lifecycle/Agent/Association。
- `.trellis/spec/backend/workflow/lifecycle-run-link.md` - 旧 `LifecycleRunLink` 是迁移来源，目标实体是 `LifecycleSubjectAssociation`。
- `.trellis/spec/backend/vfs/architecture.md` - 当前 VFS 基线使用 runtime mount 和 inline/skill/lifecycle/routine/canvas providers。
- `.trellis/spec/backend/session/runtime-execution-state.md` - `agent_frame_transitions` 是 runtime capability transition 的事实源。
- `.trellis/spec/backend/session/execution-context-frames.md` - execution frame 和 VFS/context projection 的边界。
- `crates/agentdash-infrastructure/src/migration.rs` - migration runner 与 PostgreSQL readiness 表清单。
- `crates/agentdash-api/src/bootstrap/repositories.rs` - API repository 构造和 readiness 调用。
- `crates/agentdash-infrastructure/src/persistence/postgres/*.rs` - 当前 PostgreSQL repository 的实际 SQL 表引用。
- `crates/agentdash-infrastructure/migrations/*.sql` - 历史 CREATE/DROP/RENAME TABLE 事件。

### Code Patterns

- API bootstrap 在构造 repository 前调用 readiness：`crates/agentdash-api/src/bootstrap/repositories.rs:40`。
- Readiness 使用 `REQUIRED_POSTGRES_TABLES` + `to_regclass('public.<table>')` 检查表存在：`crates/agentdash-infrastructure/src/migration.rs:4`、`crates/agentdash-infrastructure/src/migration.rs:79`。
- Migration runner 继续使用 `sqlx::migrate!("./migrations")`：`crates/agentdash-infrastructure/src/migration.rs:63`。
- `PostgresSessionRepository::initialize` 有额外局部 readiness，包含 `agent_frame_transitions`：`crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:33`。
- Story repository 读写的是 `stories.tasks` JSONB 列，不是 `tasks` 表：`crates/agentdash-infrastructure/src/persistence/postgres/story_repository.rs:37`、`crates/agentdash-infrastructure/src/persistence/postgres/story_repository.rs:142`。
- Runtime command store 写入/查询 `agent_frame_transitions` 并与 `session_runtime_commands` join：`crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:724`、`crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:811`。
- Runtime session execution anchor repository 当前读写 `runtime_session_execution_anchors`：`crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:1157`、`crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:1217`。
- Bootstrap 构造 `PostgresRuntimeSessionExecutionAnchorRepository`：`crates/agentdash-api/src/bootstrap/repositories.rs:121`，但 readiness 清单未包含 `runtime_session_execution_anchors`。

### 必须保留表

以下表满足至少一个条件：在 `REQUIRED_POSTGRES_TABLES` 中、被当前 PostgreSQL repository 实际读写、或在 bootstrap 中构造的 repository 直接依赖。新 `0001_init.sql` 应创建这些表及其现行索引/约束。

| 表 | 依据 |
| --- | --- |
| `activity_execution_claims` | readiness；`PostgresWorkflowRepository` 读写 `activity_execution_claims`，如 `workflow_repository.rs:431`。 |
| `agent_assignments` | readiness；`PostgresAgentAssignmentRepository` 读写 `agent_assignments`，如 `lifecycle_anchor_repository.rs:625`。 |
| `agent_frame_transitions` | session 局部 readiness；runtime command store 写入并 join，`session_repository.rs:724`、`session_repository.rs:811`；spec 规定其为 runtime transition 事实源。 |
| `agent_frames` | readiness；`PostgresAgentFrameRepository` 读写 `agent_frames`，如 `lifecycle_anchor_repository.rs:384`。 |
| `agent_lineages` | readiness；`PostgresAgentLineageRepository` 读写 `agent_lineages`，如 `lifecycle_anchor_repository.rs:1054`。 |
| `agent_procedures` | readiness；`PostgresWorkflowRepository` 当前 workflow/procedure 表，`workflow_repository.rs:44`。 |
| `auth_sessions` | readiness；`PostgresAuthSessionRepository` 读写 `auth_sessions`，如 `auth_session_repository.rs:24`。 |
| `backend_execution_leases` | readiness；`PostgresBackendExecutionLeaseRepository` 读写 `backend_execution_leases`，如 `backend_execution_lease_repository.rs:32`。 |
| `backend_workspace_inventory` | readiness；`PostgresProjectBackendAccessRepository` 同时实现 inventory repo，写入 `backend_workspace_inventory`，`project_backend_access_repository.rs:208`。 |
| `backends` | readiness；backend/runtime repositories 读写 `backends`，如 `backend_repository.rs:31`、`backend_execution_lease_repository.rs:530`。 |
| `canvas_bindings` | readiness；`PostgresCanvasRepository` 读写，`canvas_repository.rs:76`。 |
| `canvas_files` | readiness；`PostgresCanvasRepository` 读写，`canvas_repository.rs:42`。 |
| `canvases` | readiness；`PostgresCanvasRepository` 读写，`canvas_repository.rs:171`。 |
| `extension_package_artifacts` | readiness；`PostgresExtensionPackageArtifactRepository` 读写，`extension_package_artifact_repository.rs:39`。 |
| `group_memberships` | readiness；`PostgresUserDirectoryRepository` 读写，`user_directory_repository.rs:127`。 |
| `groups` | readiness；`PostgresUserDirectoryRepository` 读写，`user_directory_repository.rs:80`。 |
| `inline_fs_files` | readiness；`PostgresInlineFileRepository` 读写，`inline_file_repository.rs:101`；skill asset repository 也读 inline file 投影，`skill_asset_repository.rs:197`。 |
| `lifecycle_agents` | readiness；`PostgresLifecycleAgentRepository` 读写，`lifecycle_anchor_repository.rs:225`。 |
| `lifecycle_gates` | readiness；`PostgresLifecycleGateRepository` 读写，`lifecycle_anchor_repository.rs:922`。 |
| `lifecycle_runs` | readiness；`PostgresWorkflowRepository` 读写，`workflow_repository.rs:525`。 |
| `lifecycle_subject_associations` | readiness；目标关联层 repository 读写，`lifecycle_anchor_repository.rs:783`；spec 指定替代旧 run link。 |
| `lifecycle_workflow_instances` | readiness；`PostgresWorkflowGraphInstanceRepository` 读写，`lifecycle_anchor_repository.rs:84`。 |
| `library_assets` | readiness；`PostgresSharedLibraryRepository` 读写，`shared_library_repository.rs:33`。 |
| `llm_provider_user_credentials` | readiness；`PostgresLlmProviderCredentialRepository` 读写，`llm_provider_repository.rs:281`。 |
| `llm_providers` | readiness；`PostgresLlmProviderRepository` 读写，`llm_provider_repository.rs:117`。 |
| `mcp_presets` | readiness；`PostgresMcpPresetRepository` 读写，`mcp_preset_repository.rs:29`。 |
| `permission_grants` | readiness；`PostgresPermissionGrantRepository` 读写，`permission_grant_repository.rs:29`。 |
| `project_agents` | readiness；`PostgresProjectAgentRepository` 读写，`agent_repository.rs:79`。 |
| `project_backend_access` | readiness；`PostgresProjectBackendAccessRepository` 读写，`project_backend_access_repository.rs:42`。 |
| `project_extension_installations` | readiness；`PostgresProjectExtensionInstallationRepository` 读写，`project_extension_installation_repository.rs:39`。 |
| `project_subject_grants` | readiness；`PostgresProjectRepository` 读写 project subject grants，`project_repository.rs:155`。 |
| `project_vfs_mounts` | readiness；`PostgresProjectVfsMountRepository` 读写，`project_vfs_mount_repository.rs:63`。 |
| `projects` | readiness；`PostgresProjectRepository` 读写，`project_repository.rs:33`。 |
| `routine_executions` | readiness；`PostgresRoutineExecutionRepository` 读写，`routine_repository.rs:270`。 |
| `routines` | readiness；`PostgresRoutineRepository` 读写，`routine_repository.rs:73`。 |
| `runtime_health` | readiness；`PostgresRuntimeHealthRepository` 读写，`runtime_health_repository.rs:29`。 |
| `runtime_session_execution_anchors` | 当前 repository 与 bootstrap 使用，`lifecycle_anchor_repository.rs:1157`、`repositories.rs:121`；见“需要人工确认表”。 |
| `session_compactions` | readiness；session repository 读写，`session_repository.rs:321`、`session_repository.rs:895`。 |
| `session_events` | readiness；session event store 读写，`session_repository.rs:383`。 |
| `session_lineage` | readiness；session lineage store 读写，`session_repository.rs:1197`。 |
| `session_projection_heads` | readiness；session projection store 读写，`session_repository.rs:1005`。 |
| `session_projection_segments` | readiness；session projection store 读写，`session_repository.rs:952`。 |
| `session_runtime_commands` | readiness；runtime command outbox 读写，`session_repository.rs:769`、`session_repository.rs:810`。 |
| `session_terminal_effects` | readiness；terminal effect outbox 读写，`session_repository.rs:567`。 |
| `sessions` | readiness；session meta/event stores 读写，`session_repository.rs:166`。 |
| `settings` | readiness；`PostgresSettingsRepository` 读写，`settings_repository.rs:32`。 |
| `skill_assets` | readiness；`PostgresSkillAssetRepository` 读写，`skill_asset_repository.rs:41`。 |
| `state_changes` | readiness；state change store 读写，`state_change_store.rs:15`。 |
| `stories` | readiness；`PostgresStoryRepository` 读写，`story_repository.rs:37`。 |
| `user_preferences` | readiness；backend preference methods 读写，`backend_repository.rs:367`。 |
| `users` | readiness；`PostgresUserDirectoryRepository` 读写，`user_directory_repository.rs:29`。 |
| `views` | readiness；backend views methods 读写，`backend_repository.rs:334`。 |
| `workflow_graphs` | readiness；`PostgresWorkflowRepository` 当前 graph 表，`workflow_repository.rs:158`。 |
| `workspace_bindings` | readiness；`PostgresWorkspaceRepository` 读写，`workspace_repository.rs:58`；backend merge path 仍更新，`backend_repository.rs:542`。 |
| `workspaces` | readiness；`PostgresWorkspaceRepository` 读写，`workspace_repository.rs:110`。 |

### 历史冗余表

这些表在历史 migration 中创建过，但已被删除/改名/替代表达；当前源码表引用扫描未发现作为表名的读写引用，不应进入新的基线 `0001_init.sql`。

| 表 | 创建位置 | 删除/改名/替代位置 | 当前源码引用 | spec / 设计依据 |
| --- | --- | --- | --- | --- |
| `tasks` | `crates/agentdash-infrastructure/migrations/0001_init.sql:78` | 0020 将 Task 合入 `stories.tasks` JSONB 并标记旧表 deprecated：`0020_stories_tasks_jsonb.sql:1`、`0020_stories_tasks_jsonb.sql:112`；0084 只清旧列：`0084_lifecycle_control_plane_hard_cutover.sql:16`。 | 无 `FROM/INSERT/UPDATE/DELETE tasks` 当前表引用；`story_repository.rs:37` 写 `stories.tasks` 列，`story_repository.rs:142` 用 JSONB containment 查找 task id。 | `.trellis/spec/backend/story-task-runtime.md:7` 明确 Task 保存在 `stories.tasks` JSONB，无独立 repository、无独立表；`.trellis/spec/backend/repository-pattern.md:32` 同步说明。 |
| `agents` | `crates/agentdash-infrastructure/migrations/0001_init.sql:193` | 0044 创建 `project_agents` 并从 `agents`/`project_agent_links` 迁入，随后删除旧表：`0044_project_agents.sql:19`、`0044_project_agents.sql:136`、`0044_project_agents.sql:137`。 | 当前 repository 使用 `project_agents`：`agent_repository.rs:79`；未发现 `agents` 表读写。 | 现行 bootstrap 构造 `PostgresProjectAgentRepository`：`repositories.rs:89`，readiness 包含 `project_agents` 而非 `agents`：`migration.rs:28`。 |
| `project_agent_links` | `crates/agentdash-infrastructure/migrations/0001_init.sql:202` | 0044 迁移到 `project_agents` 后删除：`0044_project_agents.sql:90`、`0044_project_agents.sql:136`。 | 当前 repository 使用 `project_agents`；未发现 `project_agent_links` 表读写。 | 同 `agents`；现行 agent 聚合以 project-scoped `project_agents` 为表事实。 |
| `workflow_assignments` | `crates/agentdash-infrastructure/migrations/0001_init.sql:255` | 0013 删除：`0013_workflow_project_scoped.sql:27`；0084 再次 hard cutover 清理：`0084_lifecycle_control_plane_hard_cutover.sql:7`。 | 未发现当前源码读写。 | 当前 workflow/lifecycle 通过 `agent_procedures`、`workflow_graphs`、`lifecycle_runs`、`lifecycle_subject_associations` 等表表达；readiness 不含 `workflow_assignments`。 |
| `session_bindings` | `crates/agentdash-infrastructure/migrations/0001_init.sql:95` | 0071 从 binding 回填 `sessions.project_id` 后删除：`0071_drop_session_bindings.sql:8`、`0071_drop_session_bindings.sql:19`；0084 再清理：`0084_lifecycle_control_plane_hard_cutover.sql:6`。 | 未发现当前源码读写。 | `.trellis/spec/backend/story-task-runtime.md:58` 明确 RuntimeSession 不通过任何 binding 表与业务实体关联，`SessionMeta.project_id` 用于按项目查询。 |
| `workflow_definitions` | `crates/agentdash-infrastructure/migrations/0001_init.sql:224` | 0082 改名为 `agent_procedures`：`0082_drop_binding_kinds.sql:7`；后续当前 repository 使用 `agent_procedures`：`workflow_repository.rs:44`。 | 未发现当前源码读写 `workflow_definitions`。 | readiness 包含 `agent_procedures`，不含 `workflow_definitions`：`migration.rs:10`。 |
| `lifecycle_definitions` | `crates/agentdash-infrastructure/migrations/0001_init.sql:239` | 0082 改名为 `workflow_graphs`：`0082_drop_binding_kinds.sql:12`；0099 将 `lifecycle_runs.lifecycle_id` 列改名为 `root_graph_id`：`0099_rename_lifecycle_id_to_root_graph_id.sql:3`。 | 当前 repository 使用 `workflow_graphs`：`workflow_repository.rs:158`；未发现 `lifecycle_definitions` 表读写。 | readiness 包含 `workflow_graphs`，不含 `lifecycle_definitions`：`migration.rs:52`。 |
| `lifecycle_run_links` | `crates/agentdash-infrastructure/migrations/0070_lifecycle_run_links.sql:3` | 0084 hard cutover 删除：`0084_lifecycle_control_plane_hard_cutover.sql:5`；目标关联层是 `lifecycle_subject_associations`。 | 未发现当前源码读写。 | `.trellis/spec/backend/workflow/lifecycle-run-link.md:3` 明确 `LifecycleRunLink` 是迁移来源，目标实体为 `LifecycleSubjectAssociation`；readiness 包含 `lifecycle_subject_associations`。 |
| `skill_asset_files` | `crates/agentdash-infrastructure/migrations/0029_skill_assets.sql:19` | 0046 将文件迁移到 `inline_fs_files` 后删除旧表：`0046_skill_asset_files_to_inline_files.sql:29`、`0046_skill_asset_files_to_inline_files.sql:38`。 | 当前源码使用 `skill_assets` + `inline_fs_files`；`skill_asset_repository.rs:197` 从 `inline_fs_files` 读取 asset 文件。 | VFS/inline storage 当前通过 `inline_fs_files` 承载；readiness 包含 `inline_fs_files` 和 `skill_assets`，不含 `skill_asset_files`。 |
| `project_filespaces` | `crates/agentdash-infrastructure/migrations/0052_project_filespaces_vfs_access.sql:3` | 0054 flatten 到 `project_vfs_mounts` 后删除：`0054_project_vfs_mount_flatten.sql:4`、`0054_project_vfs_mount_flatten.sql:82`、`0054_project_vfs_mount_flatten.sql:83`。 | 当前 repository 使用 `project_vfs_mounts`：`project_vfs_mount_repository.rs:63`；未发现 `project_filespaces` 表读写。 | `.trellis/spec/backend/vfs/architecture.md:13` 说明 runtime mount 是 provider 分发单位；当前 baseline 不包含 project filespace 表。 |
| `project_vfs_mount_bindings` | `crates/agentdash-infrastructure/migrations/0052_project_filespaces_vfs_access.sql:18` | 0054 flatten 到 `project_vfs_mounts` 后删除：`0054_project_vfs_mount_flatten.sql:41`、`0054_project_vfs_mount_flatten.sql:82`。 | 当前 repository 使用 `project_vfs_mounts`；未发现 `project_vfs_mount_bindings` 表读写。 | 同 `project_filespaces`。 |

### 需要人工确认表

| 表 | 原因 | 建议 |
| --- | --- | --- |
| `runtime_session_execution_anchors` | 当前源码和 bootstrap 已使用：`repositories.rs:121`、`lifecycle_anchor_repository.rs:1157`、`lifecycle_anchor_repository.rs:1217`；历史 migration 0100 创建：`0100_runtime_session_execution_anchor.sql:1`。但 `REQUIRED_POSTGRES_TABLES` 未列出该表，readiness 不会提前发现缺表。 | 新 `0001_init.sql` 应创建该表；同时建议实现阶段确认是否把它加入 `REQUIRED_POSTGRES_TABLES`，否则 baseline 验证会漏检一个现行 repository 依赖表。 |
| `views` / `user_preferences` | 这两个表仍在 readiness，且 `backend_repository.rs:334`、`backend_repository.rs:367` 有读写路径，但它们更像旧 dashboard preference 表，未由专门 repository 在 bootstrap 中显式构造。 | 保守保留，除非产品层确认 backend preference/view 功能已废弃并同步删除源码路径。 |
| `canvas_bindings` / `canvas_files` / `canvases` | readiness 与 `PostgresCanvasRepository` 仍使用，但本次关注列表未覆盖 canvas 子系统。 | 保守保留。若要进一步收缩，应另开 canvas persistence 调研。 |

### External References

- 未使用外部资料；本次结论仅基于仓库内任务文档、spec、源码和 migration SQL。

### Related Specs

- `.trellis/spec/backend/database-guidelines.md` - schema 事实源、migration/readiness/repository DDL 边界。
- `.trellis/spec/backend/repository-pattern.md` - repository 装配和 aggregate 边界。
- `.trellis/spec/backend/story-task-runtime.md` - Task 无独立表，使用 `stories.tasks`。
- `.trellis/spec/backend/workflow/lifecycle-run-link.md` - `lifecycle_run_links` 到 `lifecycle_subject_associations` 的目标关系。
- `.trellis/spec/backend/session/runtime-execution-state.md` - `agent_frame_transitions` 和 delivery outbox 关系。
- `.trellis/spec/backend/vfs/architecture.md` - VFS runtime mount 当前 baseline。

## Caveats / Not Found

- `task.py current --source` 返回 `Current task: (none)`，本文件使用用户显式指定路径写入。
- 表引用扫描限定在 `crates/agentdash-infrastructure/src/persistence/postgres`、`crates/agentdash-api/src/bootstrap/repositories.rs`、`crates/agentdash-infrastructure/src/migration.rs`，未扩展到测试、SQLite adapter 或前端字符串。
- 自动 SQL 表名提取会产生 CTE/函数名噪声，例如 `parents`、`lineage_path`、`jsonb_array_elements_text`、`unnest`、`SET`；这些未作为真实表进入清单。
- `runtime_session_execution_anchors` 是最高风险项：它不是历史冗余，而是现行源码依赖但 readiness 漏检的表。
- `views`、`user_preferences`、canvas 表保守保留；它们未出现在特别关注列表，但当前源码仍有读写路径。
