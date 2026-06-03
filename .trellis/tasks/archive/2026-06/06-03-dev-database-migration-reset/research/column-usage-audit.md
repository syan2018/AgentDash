# Research: column-usage-audit

- Query: 字段级别调研当前 PostgreSQL 表字段是否仍被实际业务代码使用，识别可在新 0001 baseline 中直接移除的旧字段/冗余字段。
- Scope: internal
- Date: 2026-06-03

## Findings

### Context

- `task.py current --source` 返回 `Current task: (none)`；本研究按用户显式指定的任务目录写入 `.trellis/tasks/06-03-dev-database-migration-reset/research/column-usage-audit.md`。
- 任务 PRD 要求新 `0001_init.sql` 以当前 repository/schema readiness 需要的表、索引、约束、默认值为准，不保留旧字段迁移或旧表重命名逻辑。
- 任务设计明确新 baseline 以当前运行代码读取/写入的表结构为准，不通过历史回填脚本迁移旧 JSON payload、旧 lifecycle 字段、旧 session binding 或旧命名。
- 数据库规范要求 PostgreSQL schema 事实源为 `crates/agentdash-infrastructure/migrations/`，repository 启动逻辑只观察已迁移 schema，Repository 不执行 DDL；删除旧列的判断口径是 repository 主线不再读写旧列。
- `assert_postgres_schema_ready` 只检查表存在，不检查列；目标表必须从 repository 的 INSERT/SELECT/UPDATE、Row struct、FromRow 字段倒推。

### Files Found

- `crates/agentdash-infrastructure/src/migration.rs` - PostgreSQL migration runner 和 readiness 表清单；当前只检查表存在。
- `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs` - `sessions`、`session_runtime_commands` 及 session runtime 投影表读写。
- `crates/agentdash-infrastructure/src/persistence/session_core.rs` - session row mapper，包含 `sessions` 与 `session_runtime_commands` 的列读取。
- `crates/agentdash-infrastructure/src/persistence/postgres/story_repository.rs` - `stories` 聚合读写，Task 已嵌入 `stories.tasks` JSONB。
- `crates/agentdash-infrastructure/src/persistence/postgres/backend_repository.rs` - `backends` 读写和本机 backend 合并逻辑。
- `crates/agentdash-infrastructure/src/persistence/postgres/routine_repository.rs` - `routines`、`routine_executions` 读写。
- `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs` - `agent_procedures`、`workflow_graphs`、`lifecycle_runs` 读写。
- `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs` - `agent_frames` 及 lifecycle 控制面锚点表读写。
- `crates/agentdash-infrastructure/src/persistence/postgres/agent_repository.rs` - `project_agents` 读写。
- `crates/agentdash-infrastructure/src/persistence/postgres/project_vfs_mount_repository.rs` - `project_vfs_mounts` 读写。
- `crates/agentdash-infrastructure/src/persistence/postgres/mcp_preset_repository.rs` - `mcp_presets` 读写。
- `crates/agentdash-infrastructure/src/persistence/postgres/skill_asset_repository.rs` - `skill_assets` 读写，文件内容改走 `inline_fs_files`。
- `crates/agentdash-infrastructure/src/persistence/postgres/shared_library_repository.rs` - `library_assets` 读写。
- `crates/agentdash-infrastructure/src/persistence/postgres/permission_grant_repository.rs` - `permission_grants` 读写。
- `crates/agentdash-infrastructure/src/persistence/postgres/extension_package_artifact_repository.rs` - `extension_package_artifacts` 读写。
- `crates/agentdash-infrastructure/migrations/0001_init.sql` - 旧 baseline，包含多张已被后续迁移替换/删除的旧表和旧字段。
- `crates/agentdash-infrastructure/migrations/0019_mcp_preset_model_refactor.sql` - `mcp_presets` 从 `name/server_decl` 切到 `key/display_name/transport/route_policy`。
- `crates/agentdash-infrastructure/migrations/0020_stories_tasks_jsonb.sql` - Task 合入 `stories.tasks`。
- `crates/agentdash-infrastructure/migrations/0024_workflow_binding_kinds.sql`、`0082_drop_binding_kinds.sql`、`0084_lifecycle_control_plane_hard_cutover.sql` - workflow/lifecycle 旧 binding 字段清理。
- `crates/agentdash-infrastructure/migrations/0029_skill_assets.sql` - 旧 `skill_asset_files` 表创建。
- `crates/agentdash-infrastructure/migrations/0035_session_runtime_commands.sql`、`0088_agent_frame_transition_delivery_commands.sql` - runtime command 从 `transition_id` 切到 `frame_transition_id`。
- `crates/agentdash-infrastructure/migrations/0044_project_agents.sql` - `project_agent_links` 迁移到 `project_agents` 并删除旧表。
- `crates/agentdash-infrastructure/migrations/0062_extension_package_artifacts.sql`、`0064_extension_package_artifact_owner.sql` - extension artifact 从 `project_id` 切到 owner 维度。
- `crates/agentdash-infrastructure/migrations/0071_drop_session_bindings.sql`、`0084_lifecycle_control_plane_hard_cutover.sql` - `session_bindings` 删除，`sessions.project_id` 成为直接列。
- `crates/agentdash-infrastructure/migrations/0072_permission_grants.sql`、`0081_permission_grants_frame_anchor.sql` - permission grant 从 `session_id` 锚定切到 `effect_frame_id`，旧 session id 重命名为审计字段。
- `crates/agentdash-infrastructure/migrations/0076_routine_execution_dispatch_refs.sql`、`0085_routine_dispatch_strategy.sql` - routine execution 从 session 关联切到 dispatch refs；routine 策略列重命名。
- `crates/agentdash-infrastructure/migrations/0089_drop_companion_context_json.sql`、`0093_agent_frame_visible_canvas_mounts.sql`、`0094_drop_sessions_dead_columns.sql` - session 死列迁移/删除。
- `crates/agentdash-infrastructure/migrations/0099_rename_lifecycle_id_to_root_graph_id.sql` - `lifecycle_runs.lifecycle_id` 重命名为 `root_graph_id`。

### Code Patterns

- PostgreSQL readiness 只要求表存在：`REQUIRED_POSTGRES_TABLES` 包含目标表，但 `assert_postgres_tables_ready` 只调用 `to_regclass`，不校验列；见 `crates/agentdash-infrastructure/src/migration.rs:5`、`crates/agentdash-infrastructure/src/migration.rs:73`。
- Repository readiness helper 只声明表依赖，例如 session repository 检查 `sessions`、`agent_frame_transitions`、`session_runtime_commands`；见 `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:31`。
- 多数 PostgreSQL repository 使用列常量或显式 SELECT/INSERT，字段证据优先来自这些常量，例如 `WF_COLS/WG_COLS/RUN_COLS`；见 `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:35`。
- `SELECT *` 只在 `permission_grants` 中出现，必须用 `GrantRow` 反推真实列；见 `crates/agentdash-infrastructure/src/persistence/postgres/permission_grant_repository.rs:107`、`crates/agentdash-infrastructure/src/persistence/postgres/permission_grant_repository.rs:187`。

### Table Audit

#### sessions

- 保留字段证据:
  - `id, title, title_source, project_id, created_at, updated_at, last_event_seq, last_execution_status, last_turn_id, last_terminal_message, executor_config_json, executor_session_id, tab_layout_json` 被 create/save/select 明确读写；见 `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:166`、`crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:195`、`crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:232`。
  - `append_event` 更新 `last_event_seq, updated_at, last_execution_status, last_turn_id, last_terminal_message, executor_session_id`；见 `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:351`、`crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:404`。
  - row mapper 从 `project_id`、`executor_session_id` 等列读取 `SessionMeta`；见 `crates/agentdash-infrastructure/src/persistence/session_core.rs:46`、`crates/agentdash-infrastructure/src/persistence/session_core.rs:65`。
- 可删除字段候选:
  - `bootstrap_state`：旧 baseline 有此列，后续已明确删除；见 `crates/agentdash-infrastructure/migrations/0001_init.sql:105`、`crates/agentdash-infrastructure/migrations/0094_drop_sessions_dead_columns.sql:3`。当前代码注释说明判断改由 `LifecycleAgent.needs_bootstrap()` / `lifecycle_agents.bootstrap_status` 承接，不再读 session meta 的 bootstrap 字段；见 `crates/agentdash-application/src/session/launch/planner.rs:37`。
  - `visible_canvas_mount_ids_json`：旧 baseline 有此列，先迁移到 `agent_frames.visible_canvas_mount_ids_json`，后删除；见 `crates/agentdash-infrastructure/migrations/0001_init.sql:117`、`crates/agentdash-infrastructure/migrations/0093_agent_frame_visible_canvas_mounts.sql:1`、`crates/agentdash-infrastructure/migrations/0094_drop_sessions_dead_columns.sql:4`。
  - `companion_context_json`：旧 baseline 有此列，后续标明 companion 控制面已迁移到 `LifecycleGate + AgentLineage` 并删除；见 `crates/agentdash-infrastructure/migrations/0001_init.sql:116`、`crates/agentdash-infrastructure/migrations/0089_drop_companion_context_json.sql:1`。
  - `pending_capability_surface_transitions_json`、`pending_capability_state_transitions_json`：中间迁移先添加后删除；见 `crates/agentdash-infrastructure/migrations/0027_pending_capability_state_transitions.sql:1`、`crates/agentdash-infrastructure/migrations/0027_pending_capability_state_transitions.sql:46`、`crates/agentdash-infrastructure/migrations/0036_drop_legacy_session_pending_capability_transitions.sql:2`。当前 pending transitions 来自 `session_runtime_commands` 的 delivery payload，不再由 session 列持久化。
- 不确定字段:
  - `executor_session_id` 看似 provider resume id，但仍被 `SessionMeta` 读写和投影更新使用，不能删除。
  - `project_id` 可为空但当前直接替代 `session_bindings`，不能删除；见 `crates/agentdash-infrastructure/migrations/0071_drop_session_bindings.sql:5`。

#### session_runtime_commands

- 保留字段证据:
  - 当前 runtime command store 插入 `id, session_id, frame_transition_id, command_kind, payload_json, status, created_at_ms, updated_at_ms, applied_at_ms, failed_at_ms, last_error`；见 `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:769`。
  - status 更新使用 `status, updated_at_ms, applied_at_ms, failed_at_ms, last_error`；见 `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:102`。
  - row mapper 从 `frame_transition_id` 读取 delivery command 并校验 transition 一致性；见 `crates/agentdash-infrastructure/src/persistence/session_core.rs:163`。
- 可删除字段候选:
  - `transition_id`：0035 初始创建为 `transition_id`，0088 已新增 `frame_transition_id`、回填、删除 `transition_id` 并设 NOT NULL；见 `crates/agentdash-infrastructure/migrations/0035_session_runtime_commands.sql:4`、`crates/agentdash-infrastructure/migrations/0088_agent_frame_transition_delivery_commands.sql:21`、`crates/agentdash-infrastructure/migrations/0088_agent_frame_transition_delivery_commands.sql:35`、`crates/agentdash-infrastructure/migrations/0088_agent_frame_transition_delivery_commands.sql:39`。
- 不确定字段:
  - 无。`session_id` 仍是 delivery runtime session 维度；不要与被删除的 lifecycle/routine `session_id` 混淆。

#### stories

- 保留字段证据:
  - INSERT/SELECT/UPDATE 明确使用 `id, project_id, default_workspace_id, title, description, status, priority, story_type, tags, task_count, context, tasks, created_at, updated_at`；见 `crates/agentdash-infrastructure/src/persistence/postgres/story_repository.rs:37`、`crates/agentdash-infrastructure/src/persistence/postgres/story_repository.rs:74`、`crates/agentdash-infrastructure/src/persistence/postgres/story_repository.rs:406`。
  - `tasks` 被 JSONB containment 查询用于按 task id 定位 story；见 `crates/agentdash-infrastructure/src/persistence/postgres/story_repository.rs:142`。
  - `task_count` 在写入时按 `tasks.len()` 维护；读取时虽以 `tasks.len()` 为准，但 Row 仍要求列存在；见 `crates/agentdash-infrastructure/src/persistence/postgres/story_repository.rs:323`。
- 可删除字段候选:
  - 无明确表列候选。历史上 `tasks` 独立表已被合入 `stories.tasks`，因此新 baseline 可以排除旧 `tasks` 表及其运行字段，但 `stories.tasks` 必须保留；见 `crates/agentdash-infrastructure/migrations/0020_stories_tasks_jsonb.sql:7`、`crates/agentdash-infrastructure/migrations/0020_stories_tasks_jsonb.sql:113`。
- 不确定字段:
  - `task_count` 是冗余计数，但当前 SELECT/Row 依赖它，直接删列会破坏 `StoryRow`；若要删除需同步改 repository 和领域模型，本次仅按当前源码判断为保留。

#### backends

- 保留字段证据:
  - `id, name, endpoint, auth_token, enabled, backend_type, owner_user_id, profile_id, device_id, machine_id, machine_label, legacy_machine_ids, visibility, share_scope_kind, share_scope_id, capability_slot, device, last_claimed_at` 被 INSERT/SELECT/RETURNING/Row 明确使用；见 `crates/agentdash-infrastructure/src/persistence/postgres/backend_repository.rs:31`、`crates/agentdash-infrastructure/src/persistence/postgres/backend_repository.rs:83`、`crates/agentdash-infrastructure/src/persistence/postgres/backend_repository.rs:260`、`crates/agentdash-infrastructure/src/persistence/postgres/backend_repository.rs:395`。
  - `device_id` 和 `legacy_machine_ids` 仍参与本机 backend 身份合并和历史设备候选匹配；见 `crates/agentdash-infrastructure/src/persistence/postgres/backend_repository.rs:174`、`crates/agentdash-infrastructure/src/persistence/postgres/backend_repository.rs:177`、`crates/agentdash-infrastructure/src/persistence/postgres/backend_repository.rs:241`。
- 可删除字段候选:
  - 无字段级候选。历史迁移添加的 local claim 相关列当前仍被业务代码使用；见 `crates/agentdash-infrastructure/migrations/0032_local_backend_claims.sql:1`、`crates/agentdash-infrastructure/migrations/0033_local_backend_machine_scope.sql:1`。
- 不确定字段:
  - `device_id` 只在旧身份合并和 Row 中读取，当前仍有业务价值；在“当前源码是否使用”口径下保留。

#### routines

- 保留字段证据:
  - Row/INSERT/SELECT/UPDATE 使用 `id, project_id, name, prompt_template, project_agent_id, trigger_config, dispatch_strategy, enabled, created_at, updated_at, last_fired_at`；见 `crates/agentdash-infrastructure/src/persistence/postgres/routine_repository.rs:26`、`crates/agentdash-infrastructure/src/persistence/postgres/routine_repository.rs:73`、`crates/agentdash-infrastructure/src/persistence/postgres/routine_repository.rs:95`、`crates/agentdash-infrastructure/src/persistence/postgres/routine_repository.rs:141`。
  - `trigger_config` 用 JSONB containment 做 trigger type 和 endpoint id 查询；见 `crates/agentdash-infrastructure/src/persistence/postgres/routine_repository.rs:124`、`crates/agentdash-infrastructure/src/persistence/postgres/routine_repository.rs:172`。
- 可删除字段候选:
  - `agent_id`：0044 将旧 `routines.agent_id` 重命名为 `project_agent_id`；见 `crates/agentdash-infrastructure/migrations/0044_project_agents.sql:129`。
  - `session_strategy`：0085 将其重命名为 `dispatch_strategy`；当前代码只读写 `dispatch_strategy`；见 `crates/agentdash-infrastructure/migrations/0085_routine_dispatch_strategy.sql:16`。
- 不确定字段:
  - 无。

#### routine_executions

- 保留字段证据:
  - Row/INSERT/SELECT/UPDATE 使用 `id, routine_id, trigger_source, trigger_payload, resolved_prompt, dispatch_run_id, dispatch_agent_id, dispatch_frame_id, dispatch_assignment_id, status, started_at, completed_at, error, entity_key`；见 `crates/agentdash-infrastructure/src/persistence/postgres/routine_repository.rs:199`、`crates/agentdash-infrastructure/src/persistence/postgres/routine_repository.rs:270`、`crates/agentdash-infrastructure/src/persistence/postgres/routine_repository.rs:300`、`crates/agentdash-infrastructure/src/persistence/postgres/routine_repository.rs:318`。
- 可删除字段候选:
  - `session_id`：0076 明确从 `session_id` 切到 dispatch refs，并删除 `session_id`；见 `crates/agentdash-infrastructure/migrations/0076_routine_execution_dispatch_refs.sql:1`、`crates/agentdash-infrastructure/migrations/0076_routine_execution_dispatch_refs.sql:11`。
- 不确定字段:
  - 无。

#### agent_procedures

- 保留字段证据:
  - `WF_COLS` 定义并被 create/select 使用：`id, project_id, key, name, description, source, version, contract, library_asset_id, source_ref, source_version, source_digest, installed_at, created_at, updated_at`；见 `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:35`、`crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:44`、`crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:61`。
  - `AgentProcedureRow` 对同一列集 `FromRow`；见 `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:658`。
- 可删除字段候选:
  - 旧表名 `workflow_definitions`：0082 在 `agent_procedures` 不存在时重命名旧表，baseline 应直接创建 `agent_procedures`；见 `crates/agentdash-infrastructure/migrations/0082_drop_binding_kinds.sql:6`。
  - `binding_kind`、`binding_kinds`、`recommended_binding_roles`、`status`：0084 在 `agent_procedures` 上删除，当前 `WF_COLS` 不含这些列；见 `crates/agentdash-infrastructure/migrations/0084_lifecycle_control_plane_hard_cutover.sql:19`。
- 不确定字段:
  - 无。`library_asset_id/source_ref/source_version/source_digest/installed_at` 当前用于 `InstalledAssetSource` 解析，不能删除。

#### workflow_graphs

- 保留字段证据:
  - `WG_COLS` 定义并被 create/select/update 使用：`id, project_id, key, name, description, source, version, entry_activity_key, activities, transitions, library_asset_id, source_ref, source_version, source_digest, installed_at, created_at, updated_at`；见 `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:36`、`crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:158`、`crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:185`、`crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:229`。
  - `WorkflowGraphRow` 对同一列集 `FromRow`；见 `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:702`。
- 可删除字段候选:
  - 旧表名 `lifecycle_definitions`：0082 在 `workflow_graphs` 不存在时重命名旧表，baseline 应直接创建 `workflow_graphs`；见 `crates/agentdash-infrastructure/migrations/0082_drop_binding_kinds.sql:11`。
  - `binding_kind`、`binding_kinds`、`recommended_binding_roles`、`status`、`entry_step_key`、`steps`、`edges`：0084 从 `workflow_graphs` 删除，当前使用 activity 级字段 `entry_activity_key/activities/transitions`；见 `crates/agentdash-infrastructure/migrations/0084_lifecycle_control_plane_hard_cutover.sql:24`。
- 不确定字段:
  - 无。

#### lifecycle_runs

- 保留字段证据:
  - `RUN_COLS` 读取 `id, project_id, root_graph_id, status, active_node_keys, execution_log, created_at, updated_at, last_activity_at`；`RUN_INSERT_COLS` 额外写入 `record_artifacts`；见 `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:37`、`crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:38`。
  - create/update/select 使用上述列，`record_artifacts` 只在创建 lifecycle run 时写入；见 `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:525`、`crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:545`、`crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:614`。
- 可删除字段候选:
  - `lifecycle_id`：0099 重命名为 `root_graph_id`，当前代码只读写 `root_graph_id`；见 `crates/agentdash-infrastructure/migrations/0099_rename_lifecycle_id_to_root_graph_id.sql:3`。
  - `session_id`、`binding_kind`、`binding_id`、`current_step_key`、`step_states`、`port_outputs`：0084 明确删除，当前 `RUN_COLS/RUN_INSERT_COLS` 不含这些列；见 `crates/agentdash-infrastructure/migrations/0084_lifecycle_control_plane_hard_cutover.sql:9`。
  - `activity_state`：0086 用 `active_node_keys` 替换并删除；见 `crates/agentdash-infrastructure/migrations/0086_drop_lifecycle_run_activity_state.sql:1`。
- 不确定字段:
  - `record_artifacts` 当前只在 INSERT 中写入，不在 `RUN_COLS` 和 `LifecycleRunRow` 中读取。由于 hook SPI 仍有 `record_artifacts` 概念，且 create path 写入此列，baseline 必须保留，除非同步修改 repository/domain。

#### agent_frames

- 保留字段证据:
  - Row/INSERT/SELECT 使用 `id, agent_id, revision, procedure_id, graph_instance_id, activity_key, effective_capability_json, context_slice_json, vfs_surface_json, mcp_surface_json, runtime_session_refs_json, execution_profile_json, visible_canvas_mount_ids_json, created_by_kind, created_by_id, created_at`；见 `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:303`、`crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:384`、`crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:416`。
  - `visible_canvas_mount_ids_json` 已从 `sessions` 迁移到 `agent_frames`，当前 Row 解析为 frame 属性；见 `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:317`、`crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:360`。
- 可删除字段候选:
  - 无字段级候选。
- 不确定字段:
  - `visible_canvas_mount_ids_json` 名称来自历史迁移，但当前位置是当前业务字段，应保留。

#### project_agents

- 保留字段证据:
  - `PROJECT_AGENT_COLUMNS` 明确列出 `id, project_id, name, agent_type, config, installed_library_asset_id, installed_source_ref, installed_source_version, installed_source_digest, installed_at, default_lifecycle_key, is_default_for_story, is_default_for_task, knowledge_enabled, created_at, updated_at`；见 `crates/agentdash-infrastructure/src/persistence/postgres/agent_repository.rs:72`。
  - INSERT/UPDATE 对同一列集读写；见 `crates/agentdash-infrastructure/src/persistence/postgres/agent_repository.rs:79`、`crates/agentdash-infrastructure/src/persistence/postgres/agent_repository.rs:163`。
- 可删除字段候选:
  - 旧表 `project_agent_links`：0044 创建 `project_agents`、迁移旧 links、更新 inline files owner 后 `DROP TABLE IF EXISTS project_agent_links`；baseline 应直接创建 `project_agents`；见 `crates/agentdash-infrastructure/migrations/0044_project_agents.sql:19`、`crates/agentdash-infrastructure/migrations/0044_project_agents.sql:136`。
  - `project_container_ids`：0052 从 `project_agents` 删除；当前列常量不含此列；见 `crates/agentdash-infrastructure/migrations/0052_project_filespaces_vfs_access.sql:148`。
- 不确定字段:
  - `is_default_for_task` 当前代码仍读写，虽然 Task 已合入 Story，但领域模型仍保留此字段，不能仅按历史倾向删除。

#### project_vfs_mounts

- 保留字段证据:
  - `MOUNT_COLUMNS` 和 INSERT/UPDATE 使用 `id, project_id, mount_id, display_name, description, capabilities, installed_source, content, created_at, updated_at`；见 `crates/agentdash-infrastructure/src/persistence/postgres/project_vfs_mount_repository.rs:57`、`crates/agentdash-infrastructure/src/persistence/postgres/project_vfs_mount_repository.rs:63`、`crates/agentdash-infrastructure/src/persistence/postgres/project_vfs_mount_repository.rs:138`。
- 可删除字段候选:
  - `default_write`：0055 已删除，当前 Row/列常量不含此列；见 `crates/agentdash-infrastructure/migrations/0055_project_vfs_mount_drop_default_write.sql:7`。
- 不确定字段:
  - 无。

#### mcp_presets

- 保留字段证据:
  - `COLS` 明确列出 `id, project_id, key, display_name, description, transport, route_policy, source, builtin_key, library_asset_id, source_ref, source_version, source_digest, installed_at, created_at, updated_at`；见 `crates/agentdash-infrastructure/src/persistence/postgres/mcp_preset_repository.rs:23`。
  - create/update/upsert_builtin 使用 `key/display_name/transport/route_policy` 和 installed source 字段；见 `crates/agentdash-infrastructure/src/persistence/postgres/mcp_preset_repository.rs:29`、`crates/agentdash-infrastructure/src/persistence/postgres/mcp_preset_repository.rs:95`、`crates/agentdash-infrastructure/src/persistence/postgres/mcp_preset_repository.rs:149`。
- 可删除字段候选:
  - `name`、`server_decl`：0019 添加当前模型字段并删除旧字段；当前 repository 不读写旧字段；见 `crates/agentdash-infrastructure/migrations/0019_mcp_preset_model_refactor.sql:2`、`crates/agentdash-infrastructure/migrations/0019_mcp_preset_model_refactor.sql:70`、`crates/agentdash-infrastructure/migrations/0019_mcp_preset_model_refactor.sql:73`。
- 不确定字段:
  - 无。

#### skill_assets

- 保留字段证据:
  - `ASSET_COLS` 使用 `id, project_id, key, display_name, description, source, builtin_key, remote_source_url, remote_imported_at, remote_digest, library_asset_id, source_ref, source_version, source_digest, installed_at, disable_model_invocation, created_at, updated_at`；见 `crates/agentdash-infrastructure/src/persistence/postgres/skill_asset_repository.rs:24`。
  - create/update/list 对上述列读写；见 `crates/agentdash-infrastructure/src/persistence/postgres/skill_asset_repository.rs:41`、`crates/agentdash-infrastructure/src/persistence/postgres/skill_asset_repository.rs:113`、`crates/agentdash-infrastructure/src/persistence/postgres/skill_asset_repository.rs:166`。
  - 文件内容不走 `skill_asset_files`，而是 `inline_fs_files`，owner_kind=`skill_asset`，container_id=`files`；见 `crates/agentdash-infrastructure/src/persistence/postgres/skill_asset_repository.rs:197`、`crates/agentdash-infrastructure/src/persistence/postgres/skill_asset_repository.rs:209`、`crates/agentdash-infrastructure/src/persistence/postgres/skill_asset_repository.rs:245`。
- 可删除字段候选:
  - 旧表 `skill_asset_files`：0029 创建旧文件表，但当前 repository 完全使用 `inline_fs_files`；baseline 可以不创建 `skill_asset_files`；见 `crates/agentdash-infrastructure/migrations/0029_skill_assets.sql:19`、`crates/agentdash-infrastructure/src/persistence/postgres/skill_asset_repository.rs:209`。
- 不确定字段:
  - `disable_model_invocation` 当前列常量/Row/UPDATE 使用，保留。

#### library_assets

- 保留字段证据:
  - `COLS` 使用 `id, asset_type, scope, owner_id, key, display_name, description, version, source, source_ref, payload_digest, deprecated, payload, created_at, updated_at`；见 `crates/agentdash-infrastructure/src/persistence/postgres/shared_library_repository.rs:26`。
  - create/find/list/update/upsert 均使用上述列；见 `crates/agentdash-infrastructure/src/persistence/postgres/shared_library_repository.rs:33`、`crates/agentdash-infrastructure/src/persistence/postgres/shared_library_repository.rs:76`、`crates/agentdash-infrastructure/src/persistence/postgres/shared_library_repository.rs:91`、`crates/agentdash-infrastructure/src/persistence/postgres/shared_library_repository.rs:112`。
- 可删除字段候选:
  - 无字段级候选。历史迁移 0052 扩展了 `asset_type` vocabulary，但没有当前源码可删列。
- 不确定字段:
  - 无。

#### permission_grants

- 保留字段证据:
  - INSERT 写入 `id, run_id, effect_frame_id, source_runtime_session_id, source_turn_id, source_tool_call_id, requested_paths, reason, grant_scope, expires_at, scope_escalation_intent, status, policy_decision, approved_by, created_at, updated_at`；见 `crates/agentdash-infrastructure/src/persistence/postgres/permission_grant_repository.rs:29`。
  - `SELECT *` 由 `GrantRow` 反推同一列集；见 `crates/agentdash-infrastructure/src/persistence/postgres/permission_grant_repository.rs:107`、`crates/agentdash-infrastructure/src/persistence/postgres/permission_grant_repository.rs:187`。
  - 当前主查询路径为 `effect_frame_id` 和 `run_id`，活跃 frame 查询使用 `effect_frame_id`；见 `crates/agentdash-infrastructure/src/persistence/postgres/permission_grant_repository.rs:121`、`crates/agentdash-infrastructure/src/persistence/postgres/permission_grant_repository.rs:136`。
- 可删除字段候选:
  - 原名 `session_id`：0081 明确重命名为 `source_runtime_session_id`，并新增 `effect_frame_id` 作为主查询锚点；baseline 应直接创建 `source_runtime_session_id`，不创建旧 `session_id`；见 `crates/agentdash-infrastructure/migrations/0072_permission_grants.sql:7`、`crates/agentdash-infrastructure/migrations/0081_permission_grants_frame_anchor.sql:1`、`crates/agentdash-infrastructure/migrations/0081_permission_grants_frame_anchor.sql:10`。
- 不确定字段:
  - `source_runtime_session_id` 仍是审计追溯字段并被 `GrantRow` 映射到领域实体，不能作为旧 `session_id` 直接删除。

#### extension_package_artifacts

- 保留字段证据:
  - `COLS` 使用 `id, owner_kind, owner_id, extension_id, package_name, package_version, asset_version, source_version, storage_ref, archive_digest, manifest_digest, manifest, byte_size, created_at, updated_at`；见 `crates/agentdash-infrastructure/src/persistence/postgres/extension_package_artifact_repository.rs:30`。
  - create/get/list 按 owner 和 archive digest 查询；见 `crates/agentdash-infrastructure/src/persistence/postgres/extension_package_artifact_repository.rs:39`、`crates/agentdash-infrastructure/src/persistence/postgres/extension_package_artifact_repository.rs:81`、`crates/agentdash-infrastructure/src/persistence/postgres/extension_package_artifact_repository.rs:99`。
- 可删除字段候选:
  - `project_id`：0062 初版 artifact 以 `project_id` 为 owner，0064 新增 `owner_kind/owner_id`、重建唯一索引并删除 `project_id`；当前 repository 不读写 `project_id`；见 `crates/agentdash-infrastructure/migrations/0062_extension_package_artifacts.sql:3`、`crates/agentdash-infrastructure/migrations/0064_extension_package_artifact_owner.sql:1`、`crates/agentdash-infrastructure/migrations/0064_extension_package_artifact_owner.sql:54`。
  - 旧命名 `artifact_storage_ref`、`artifact_archive_digest`、`artifact_manifest_digest` 只属于 `project_extension_installations` 衍生列，不属于 `extension_package_artifacts` 当前表；不要带进 artifact baseline。
- 不确定字段:
  - 无。

### Cross-Table Old Tables To Exclude From New Baseline

- `session_bindings`: 0071 用 `sessions.project_id` 替代并删除；0084 再次 `DROP TABLE IF EXISTS session_bindings`；当前代码没有 repository 表依赖。
- `tasks`: Task 已合入 `stories.tasks` JSONB；旧 `tasks.session_id/executor_session_id` 在 0084 删除；当前 `StoryRepository` 通过 `stories.tasks` 管理 Task。
- `agents` 和 `project_agent_links`: 0044 收敛到 `project_agents` 并删除 `project_agent_links`；当前代码不读写旧表。
- `workflow_definitions` 和 `lifecycle_definitions`: 0082 分别重命名到 `agent_procedures` 和 `workflow_graphs`；新 baseline 应直接创建新表名。
- `workflow_assignments`: 旧 baseline 表，当前 readiness 清单和 repository 未使用；新 lifecycle 控制面使用 `lifecycle_workflow_instances`、`lifecycle_agents`、`agent_frames`、`agent_assignments`。
- `skill_asset_files`: 当前 `SkillAssetRepository` 将文件统一落在 `inline_fs_files`，baseline 不应创建旧表。
- `lifecycle_run_links`: 0070 历史关联表未在 readiness 清单中出现，当前代码未发现 repository 使用；新控制面通过 anchors/associations/frames 表达。

### External References

- 无外部资料。本调研仅基于当前仓库源码、Trellis 任务文档、规范与 migration 历史。

### Related Specs

- `.trellis/spec/backend/database-guidelines.md` - schema 事实源、repository 不执行 DDL、删除旧列判断标准。
- `.trellis/spec/backend/repository-pattern.md` - PostgreSQL repository 假设 schema 已由 migration runner 初始化。
- `.trellis/spec/backend/workflow/activity-lifecycle.md` - workflow/lifecycle 当前以 activity 生命周期表达。
- `.trellis/spec/backend/workflow/lifecycle-run-link.md` - lifecycle run 与 runtime session 的当前关系需要经 anchors/frames 理解。
- `.trellis/spec/backend/session/execution-context-frames.md` - agent frame/runtime execution context 的当前事实源。
- `.trellis/spec/backend/permission/grant-lifecycle.md` - permission grant 生命周期和 frame anchor 语义。

## Caveats / Not Found

- `task.py current --source` 未解析到活动任务；本文件依据用户显式路径写入。
- 本研究没有运行数据库 schema dump，也没有验证迁移链最终 DDL；结论是基于当前 repository/spec/migration 文本的字段使用审计。
- 对 `permission_grants` 的列判断依赖 `GrantRow`，因为查询使用 `SELECT *`；如果未来改字段，需要先改显式列列表以降低误判风险。
- 对 `record_artifacts`、`stories.task_count` 这类“写入但少读/冗余”的字段，本研究按当前源码保留；若实现阶段愿意同步改 repository/domain，可另行作为代码改造候选，而不是 baseline 直接删列候选。
- 全局 `rg` 搜索中存在大量 domain/SPI 中的 `session_id`、`project_id`、`steps` 等通用字段名；本研究只将它们纳入对应 PostgreSQL 表语义，未把同名 JSON/domain 字段误判为数据库列需求。
