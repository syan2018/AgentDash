# 数据库语义基线正式评估报告

## Executive Summary

当前 `crates/agentdash-infrastructure/migrations/0001_init.sql` 不能算最优状态。上一轮压缩 migration 后，它已经能作为“空库初始化基线”运行，但语义上仍保留了旧 migration dump 的多个问题：

- 存在至少一个干净库会直接失败的 schema/code 契约缺口：`workspaces.mount_capabilities` 被 repository/domain 当作必需列，但当前 init baseline 未建列。
- 存在高置信历史残留字段：`lifecycle_runs.record_artifacts`、`activity_execution_claims.graph_instance_id DEFAULT '00000000-...'`、`agent_frames.created_by_kind DEFAULT 'backfill'`、`agent_procedures/workflow_graphs.project_id` 零 UUID 默认、`state_changes.project_id DEFAULT ''`。
- 多个表把系统行为、UI 状态或投影缓存落在业务/runtime head 表里：`sessions.project_id`、`sessions.executor_config_json`、`sessions.tab_layout_json`、`sessions.last_*`、`lifecycle_runs.active_node_keys`、`lifecycle_runs.execution_log`、`views`、`user_preferences`。
- 当前 baseline 仍明显是 pg_dump 形态：大量 `public.` 前缀、dump 注释、混合 constraint/index 命名、历史约束名、JSON TEXT/JSONB 策略不一致、默认值带旧迁移语义、FK/CHECK 约束不足。
- 大量看起来复杂的 runtime/control-plane 表其实应保留：session compaction/projection/lineage、agent frames/assignments/lineages、runtime session anchors、routine dispatch refs、permission grants、backend leases、runtime health、state_changes 都有当前源码和 spec 语义支撑，不能简单按“系统行为”删除。

因此建议下一步不要只“美化 dump”，而是拆成两轮实施：

1. **P0 基线修正**：修复 `workspaces.mount_capabilities` 缺列；删除确定残留/default；修正 `runtime_session_execution_anchors` UUID/text 混用；把 `0001_init.sql` 改成手工整理形态。
2. **P1 语义收敛**：迁移 `sessions`、`lifecycle_runs`、`views/user_preferences`、`backends` 的职责边界；把投影/cache/outbox 明确迁到各自表；同步 repository/domain/API/frontend contracts。

本报告评估完成后，baseline correctness 与 hand-curated `0001_init.sql` 已在本任务内落地；需要跨 repository/domain/API/frontend 的业务语义收敛已拆入 `.trellis/tasks/06-03-database-business-semantic-convergence`。

## High-Confidence Cleanup Candidates

这些项目语义证据明确，适合作为下一轮 schema 收敛的第一批修改目标。

| 优先级 | 对象 | 当前问题 | 建议动作 | 证据 |
| --- | --- | --- | --- | --- |
| P0 | `workspaces.mount_capabilities` | 当前 init 缺列，但 repository create/select/list/update 都读写，干净库 Workspace 路径会失败。 | 在 init 加回该列，或同步删除代码字段；按当前 domain 语义更应加列并补默认能力。 | `workspace_repository.rs` 读写该列；`Workspace` entity 将其作为 mount capability 配置。 |
| P0 | `lifecycle_runs.record_artifacts` | 只残留在 migration 和 insert placeholder；不进入 domain/read/update/API/frontend。 | 从 `lifecycle_runs` 删除，并清理 `RUN_INSERT_COLS` 插入占位。 | `workflow_repository.rs` 插入列包含它，但 `RUN_COLS`/row mapper/domain 不读取。 |
| P0 | `runtime_session_execution_anchors` ID 类型 | 同一表 `runtime_session_id` 是 text，其他 run/frame/agent/assignment/graph_instance 是 uuid；周边 lifecycle 表和 repository 以 text/string 绑定。 | 在当前项目整体 text-id 风格下先统一为 text，再补 FK/index；或全 lifecycle 统一 UUID，但那是更大重构。 | migration 表定义与 `lifecycle_anchor_repository.rs` bind string 不一致。 |
| P0 | `activity_execution_claims.graph_instance_id DEFAULT '00000000-...'` | graph instance 是 activity attempt identity 的必填语义，零 UUID default 是回填残留。 | 删除 default，保留 `NOT NULL`，补 FK 到 `lifecycle_workflow_instances(id)`。 | domain idempotency key 使用 `run_id:graph_instance_id:activity_key:attempt`。 |
| P0 | `agent_frames.created_by_kind DEFAULT 'backfill'` | `backfill` 是历史迁移来源，不是新 init 的创建语义。 | 删除 default，让 frame builder/dispatch 显式写入创建来源。 | application `frame_builder` 已显式设置 `created_by_kind`。 |
| P0 | `agent_procedures.project_id` / `workflow_graphs.project_id` 零 UUID default | definition 表的 project owner 不应有假 project。 | 删除零 UUID default，保留 `NOT NULL` 和 FK。 | 当前 repository/use case 都应显式绑定 project。 |
| P0 | `state_changes.project_id DEFAULT ''` | project-scoped event stream 不应产生空 project id。 | 删除 default，考虑 FK 到 `projects(id)`；至少加 `(project_id, id)` index。 | stream 按 `WHERE project_id=$1 AND id>$2` 读取。 |
| P1 | `user_preferences` | 全局 `key/value` 偏好表与当前 scoped `settings` 重叠，是最明确的 legacy 表。 | 代码改造后删除，剩余偏好收敛到 `settings(scope_kind, scope_id, key)`。 | Settings UI / PI user preferences 使用 `settings`；BackendRepository 仍保留旧方法。 |
| P1 | `views` | 作为全局 backend dashboard saved view 挂在 `BackendRepository`，无 user/project scope，且 backend_ids 是 JSON text。 | 若产品面不再需要则删除；若需要则迁移为 scoped saved UI state。 | `BackendRepository` 仍读写，duplicate backend merge 仍改写 `views.backend_ids`。 |

## Requires-Code-Change Candidates

这些字段/表目前仍被代码读写，不能直接从 init 中砍；但它们语义位置不对，应作为后续代码改造目标。

### `sessions` 应收敛为 RuntimeSession Head

目标语义：Session 是 RuntimeSession，只承载 turn/tool/event/resume/debug/projection/trace lineage，不拥有业务归属、provider 行为、UI 布局或 lifecycle progress truth。

建议迁移：

- `sessions.project_id`：业务归属泄漏。应通过 `runtime_session_execution_anchors -> AgentFrame -> LifecycleAgent -> LifecycleRun -> LifecycleSubjectAssociation` 反查业务上下文。
- `sessions.executor_config_json`：provider/executor 行为泄漏。应属于 AgentFrame execution profile 或 launch construction source。
- `sessions.tab_layout_json`：UI 布局状态泄漏。应迁到 scoped settings、session UI state 或 workspace view state。
- `sessions.last_execution_status`、`last_turn_id`、`last_terminal_message`：event-derived projection/cache。可迁到 `session_runtime_summaries` / list projection；若短期保留，应重命名/文档化为 `latest_*_projection`，不要当 active turn 或 live connector truth。

应保留：

- `id`、`created_at`、`updated_at`、`last_event_seq`、`executor_session_id`、`title`、`title_source`。
- `last_event_seq` 是事件序列分配器，不是普通列表缓存。

### `lifecycle_runs` 应拆出投影和 audit timeline

建议迁移：

- `active_node_keys`：当前 domain 注释已说明它是从 `WorkflowGraphInstance.activity_state` 派生的 read-model-only projection。应由 `lifecycle_workflow_instances.activity_state_json` 派生，或迁到专门 run summary projection。
- `execution_log`：它是 hook/runtime audit timeline，被 `LifecycleRunView`、VFS lifecycle provider、journey surfaces 使用；不能直接删，但不该作为 mutable JSON array 存在 `lifecycle_runs` ledger row。建议迁到 append-only `lifecycle_execution_events`，再投影回 view。

应保留：

- `id`、`project_id`、`root_graph_id`、`status`、`created_at`、`updated_at`、`last_activity_at`。
- `lifecycle_workflow_instances` 是 activity state 的正确落点。

### Project / Story / Agent Business 表收敛

- `stories.task_count`：可由 `tasks.len()` 派生，repository 读出时已经以 `tasks.len()` 为准。建议删除普通列并由 domain/API 计算，或改成 generated column。
- `project_agents.is_default_for_task`：当前只发现 domain/repository/API/contracts 读写，未发现 application 层真实 Task dispatch 选择逻辑。建议删除，或迁到明确的 Task dispatch policy。
- `projects.visibility` 与 `is_template`：当前可能存在派生/重复语义。若 `is_template` 只是 visibility 派生，应删除；若它表达可 clone 模板语义，应补不变量。
- `canvas_bindings`：不能直接删，但它混合 Canvas 业务资产和 runtime/data-source 引用。建议重命名/结构化为 `canvas_data_bindings`，并给 `source_uri` 引入 scheme contract。

### Backend / Local Runtime 表收敛

- `backends`：配置、local identity、share scope、legacy merge、claim lifecycle 混在一张表。建议至少重命名/分组约束；更理想是把 local identity/share/claim 状态拆到 `local_backend_identities` 或类似表。
- `backends.device_id` / `legacy_machine_ids`：当前还被 local backend merge 使用；预研期若不需要旧 identity merge，代码改造后可删除。
- `backends.last_claimed_at`：runtime claim lifecycle，不是静态 backend config；可迁到 local runtime identity/claim 表。
- `project_backend_access`：保留，但补 backend FK、status/access_mode CHECK；`root_policy`/`capability_policy` 默认值应由 use case 显式写入。
- `backend_workspace_inventory`：保留，但补 backend FK、identity/status/source CHECK；`source DEFAULT 'manual_refresh'` 建议删除，避免隐藏缺失写入。

### Platform Config / Asset 表收敛

- `skill_assets.remote_source_url`、`remote_imported_at`、`remote_digest`：只有当产品决定取消 github/clawhub/skills_sh 直接导入、统一走 Shared Library install 后才能删除。
- `llm_providers.models` / `blocked_models`：domain/API 是 JSON value，但 DB 是 TEXT。建议改 `JSONB` 并补 `protocol`、`credential_mode` CHECK。
- `llm_providers.global_api_key_ciphertext` / `llm_provider_user_credentials.api_key_ciphertext`：字段名不准，Codex OAuth token JSON 不一定是 API key。建议改为 `global_credential_ciphertext` / `credential_ciphertext`。
- `permission_grants`：保留为 runtime/audit fact；不要归到 config。需要核查 `permission_grant_repository.rs` 当前 JSONB `LIKE` 查询，语义上应改 JSONB containment/path extraction。

## Keep-But-Normalize Candidates

这些表/字段语义正确，但当前 dump baseline 没有把约束和类型表达清楚。

### 应补 FK / Unique / Check

- `project_subject_grants.project_id -> projects(id)`；`subject_type`、`role` CHECK。
- `workspaces.project_id -> projects(id)`；`default_binding_id` 与 `workspace_bindings` 的完整性规则。
- `workspace_bindings.workspace_id -> workspaces(id)`；`(workspace_id, backend_id, root_ref)` unique；`status` CHECK。
- `stories.project_id -> projects(id)`；`default_workspace_id -> workspaces(id)`；`status/priority/story_type` CHECK。
- `project_agents.project_id -> projects(id)`；`is_default_for_story` partial unique per project；installed source 成组约束。
- `project_vfs_mounts.project_id -> projects(id)`；`content` kind CHECK。
- `canvases.project_id -> projects(id)`；`(project_id, mount_id)` unique。
- `canvas_files.canvas_id -> canvases(id) ON DELETE CASCADE`；relative path CHECK。
- `agent_frames`：`(agent_id, revision)` unique；`procedure_id`、`graph_instance_id` FK；`created_by_kind` CHECK。
- `agent_assignments.graph_instance_id -> lifecycle_workflow_instances(id)`；attempt uniqueness 规则；`lease_status` CHECK。
- `lifecycle_subject_associations.anchor_agent_id -> lifecycle_agents(id)`；anchor/subject indexes。
- `lifecycle_gates.agent_id/frame_id` FK；status CHECK；open gate uniqueness if applicable。
- `agent_lineages.parent_agent_id/source_frame_id` FK；relation kind CHECK。
- `runtime_session_execution_anchors`：类型统一后补 run/frame/agent/assignment/graph_instance FK。
- `routine_executions` dispatch refs：类型统一后补 FK；dispatch refs all-or-none CHECK。
- `settings.scope_kind` CHECK；system scope_id 必须空、user/project scope_id 必须非空。
- `users.auth_mode` CHECK；考虑 `subject/auth_mode/provider` unique。
- `group_memberships` FK 到 users/groups。
- `project_backend_access.backend_id -> backends(id)`。
- `backend_workspace_inventory.backend_id -> backends(id)`。
- `state_changes(project_id, id)` index；考虑 `project_id -> projects(id)`。

### JSON TEXT / JSONB 策略

建议定一条 baseline 原则：

- 需要 DB 查询、containment、索引、patch 或结构约束的 JSON 用 `JSONB`。
- 纯 opaque domain value object 可以保留 `TEXT`，但必须由 repository/domain 显式 parse/validate。

优先改成 JSONB：

- `llm_providers.models`、`llm_providers.blocked_models`。
- `stories.tags`、`stories.context` 可考虑 JSONB；`stories.tasks` 已是 JSONB。
- `workspace_bindings.detected_facts` 与 `backend_workspace_inventory.detected_facts` 若用于查询/匹配，可改 JSONB。
- `state_changes.payload` 若用于 stream/projection structured filtering，可改 JSONB。

可暂保留 TEXT：

- `settings.value`，作为 scoped opaque setting。
- `mcp_presets.transport` / `route_policy`，作为 domain value object。
- `project_backend_access.root_policy` / `capability_policy`，除非要 DB 层查询。

### Dump Quality

建议把新的 `0001_init.sql` 改成手工 curated baseline：

- 删除 pg_dump headers、object comments 和大量 `public.` 前缀噪音。
- 统一命名：`pk_*`、`uq_*`、`fk_*`、`ck_*`、`ix_*`，或选定一个现有风格贯彻到底。
- 删除 stale constraint names，例如 `runtime_health_accessible_roots_not_null`、`lifecycle_runs_lifecycle_id_not_null`、`routines_session_strategy_not_null`、`permission_grants_session_id_not_null`。
- 删除旧回填/default 语义：`backfill`、零 UUID、空 project id、行为默认 source。
- 明确 timestamp split：业务/domain 时间用 `timestamptz`；session event/log/projection cursor 用 ms bigint 时，需要作为 runtime stream 约定保留。
- Init migration 不需要 `CREATE TABLE IF NOT EXISTS`；SQLx migration versioning 会在空库运行一次。后续 forward migration 再使用 idempotent DDL。

## Looks Odd But Should Stay

这些内容容易被误判成“系统行为残留”，但当前项目语义需要它们。

| 对象 | 保留理由 |
| --- | --- |
| `session_events` | RuntimeSession append-only 事件日志，是 session trace 的事实源。 |
| `session_terminal_effects` | terminal effect outbox/audit，承载 terminal side effect delivery lifecycle。 |
| `session_runtime_commands` | runtime command delivery outbox；与 `agent_frame_transitions` 的 fact/source 分工正确。 |
| `agent_frame_transitions` | capability/runtime context transition 的事实源，不是 session command 的重复表。 |
| `session_compactions` / `session_projection_heads` / `session_projection_segments` | 虽是 projection/cache，但 fork、rollback、restore、model-visible checkpoint 依赖 durable projection。 |
| `session_lineage` | Runtime trace branch topology，不是业务归属；业务可见性由 LifecycleSubjectAssociation/AgentLineage 投影。 |
| `lifecycle_workflow_instances` | activity state 的正确事实表，`active_node_keys` 应从这里派生。 |
| `agent_frames` capability/context/VFS/MCP/execution profile fields | AgentFrame 是 effective runtime surface revision snapshot，这些字段属于 frame revision。 |
| `agent_assignments` | Activity attempt 到 Agent/Frame 的执行桥，是 workflow control-plane 核心表。 |
| `lifecycle_subject_associations` | SubjectRef 到 run/agent 的业务/control association，不应折回 sessions。 |
| `agent_lineages` | Agent control tree edge，区别于 session_lineage。 |
| `runtime_session_execution_anchors` | RuntimeSession 反查 control-plane launch evidence 的正确 normalized anchor；应修类型，不应删。 |
| `routine_executions.dispatch_*` refs | Routine trigger audit 到 run/agent/frame/assignment 的导航和 reuse validation 需要这些 refs。 |
| `permission_grants` | runtime capability authorization audit/state machine，不是 config 表。 |
| `runtime_health` | 连接健康 snapshot；busy/idle 来自 leases，不应合并。 |
| `backend_execution_leases` | backend execution occupancy 的事实源，不是 runtime_health 状态字段。 |
| `backend_workspace_inventory` | backend 目录探测事实，与 project workspace binding、execution lease 分离。 |
| `state_changes` | 仍承担 project NDJSON stream cursor 和 projection 输入，不能按“旧投影”删除。 |
| `project_extension_installations.package_*` / `artifact_*` snapshot | installation-time artifact ref 快照服务运行时下载和 publish 校验，不能只依赖 artifact join。 |
| `auth_sessions.identity_json` | auth provider unavailable 时的 identity snapshot/cache，不是 user profile 替代物。 |
| `llm_providers` global credential 与 `llm_provider_user_credentials` 分表 | global admin key 与 user BYOK/Codex token ownership 已由 credential_mode/resolver 分清。 |
| `canvas_files` | Canvas 聚合私有 source asset，当前 `canvas_fs` provider 和 promotion 依赖它。 |
| `project_vfs_mounts.id` + `mount_id` 双身份 | UUID 服务持久化和 owner，`mount_id` 是 VFS path identity。 |

## Table-by-Table Audit Appendix

### Core Business

| 表 | 分类 | 结论 |
| --- | --- | --- |
| `projects` | Business fact | 保留；整理 `visibility/is_template` 不变量、FK/CHECK。 |
| `project_subject_grants` | Business authorization fact | 保留；与 runtime `permission_grants` 分层不同。补 role/subject CHECK 与 project FK。 |
| `workspaces` | Business workspace identity/config | 保留；P0 修复缺失 `mount_capabilities`；整理 `default_binding_id/status` 语义。 |
| `workspace_bindings` | Directory binding fact/cache | 保留；表达 backend root 绑定，不表达执行占用。补 unique/FK/CHECK。 |
| `stories` | Story aggregate | 保留；`tasks` 保留；`task_count` 降级为 derived/generated。 |
| `project_agents` | Project agent config | 保留；`is_default_for_task` 是删除/迁移候选；`is_default_for_story` 加 partial unique。 |
| `project_vfs_mounts` | Project VFS config | 保留；JSON 类型和 content kind 约束整理。 |
| `canvases` | Canvas aggregate | 保留；补 `(project_id, mount_id)` unique 与 JSONB sandbox config。 |
| `canvas_files` | Canvas aggregate child asset | 保留；补 FK cascade/path check。 |
| `canvas_bindings` | Canvas data binding config | 保留但重命名/结构化；`source_uri` 需要 scheme contract。 |

### Session Runtime

| 表 | 分类 | 结论 |
| --- | --- | --- |
| `sessions` | RuntimeSession head + projection leakage | 保留但大幅收敛；移出 project/provider/UI 字段；last_* 明确为 projection。 |
| `session_events` | Runtime event log | 保留；核心事实源。 |
| `session_terminal_effects` | Outbox/audit | 保留；补 CHECK。 |
| `session_runtime_commands` | Runtime command outbox | 保留；`phase_node` 可评估是否由 transition join 得出。 |
| `agent_frame_transitions` | Control-plane transition fact | 保留；补 run FK/ID 类型策略。 |
| `session_compactions` | Durable projection/checkpoint | 保留；整理 enum/check 与 `created_by` 语义。 |
| `session_projection_heads` | Projection cursor | 保留；PK 语义当前合理。 |
| `session_projection_segments` | Materialized projection segment | 保留；补 non-negative / enum CHECK。 |
| `session_lineage` | Runtime branch topology | 保留；不是 business ownership。 |

### Workflow / Lifecycle

| 表 | 分类 | 结论 |
| --- | --- | --- |
| `agent_procedures` | Definition fact | 保留；删除零 UUID default，整理 constraints。 |
| `workflow_graphs` | Definition fact | 保留；删除零 UUID default，可考虑 JSONB activities/transitions。 |
| `lifecycle_runs` | Runtime/control ledger | 保留但删除 `record_artifacts`；迁移 `active_node_keys/execution_log`。 |
| `lifecycle_workflow_instances` | Activity state fact | 保留；是 active projection 的派生来源。 |
| `lifecycle_agents` | Control-plane agent fact | 保留；补 current_frame FK/check。 |
| `agent_frames` | Runtime surface revision snapshot | 保留；删除 `backfill` default，拆/降级 runtime refs 与 visible canvas refs。 |
| `agent_assignments` | Activity attempt execution bridge | 保留；补 graph instance FK/unique/check。 |
| `lifecycle_subject_associations` | Subject/run/agent association | 保留；补 agent FK/index。 |
| `lifecycle_gates` | Durable gate/wait point | 保留；补 agent/frame FK/status CHECK。 |
| `agent_lineages` | Agent control tree edge | 保留；补 parent/source frame FK。 |
| `runtime_session_execution_anchors` | Runtime session launch evidence | 保留；P0 修 UUID/text 混用。 |
| `activity_execution_claims` | Durable activity claim | 保留；P0 删除 graph_instance zero default，补 FK。 |
| `routines` | Trigger config | 保留；整理 constraint 名和 JSON 类型。 |
| `routine_executions` | Trigger execution audit | 保留；dispatch refs 应保留并补约束/FK。 |

### Platform Assets / Config / Identity

| 表 | 分类 | 结论 |
| --- | --- | --- |
| `library_assets` | Shared asset config/seed target | 保留；payload JSONB 合理。 |
| `skill_assets` | Project skill config | 保留；remote_* 是否删除取决于是否保留外部 registry import。 |
| `inline_fs_files` | Inline VFS file/blob storage | 保留；owner_kind 需 contract/CHECK。 |
| `mcp_presets` | MCP config | 保留；transport/route_policy 作为 opaque value object 可暂 TEXT。 |
| `extension_package_artifacts` | Package artifact metadata | 保留；`source_version` 与 `asset_version` 关系需确认。 |
| `project_extension_installations` | Project extension install config | 保留；artifact snapshot 列看似重复但有 audit/runtime value。 |
| `settings` | Scoped config | 保留；替代 legacy `user_preferences`。 |
| `auth_sessions` | Auth runtime cache/audit | 保留；epoch time 与 auth domain 对齐。 |
| `users` | Identity directory | 保留；补 subject/auth/provider uniqueness strategy。 |
| `groups` | Identity directory | 保留。 |
| `group_memberships` | Identity directory membership | 保留；补 FK 或记录无 FK 原因。 |
| `llm_providers` | Global provider catalog/config | 保留；models/blocked_models 改 JSONB，credential 字段改名。 |
| `llm_provider_user_credentials` | User BYOK/credential | 保留；credential 字段改名，补 CHECK/FK。 |
| `permission_grants` | Runtime/audit authorization fact | 保留；不是 config。补 CHECK，核查 JSONB 查询。 |

### Backend / Local Runtime / Stream

| 表 | 分类 | 结论 |
| --- | --- | --- |
| `backends` | Backend registration + local identity mix | 保留但重构边界；auth_token partial unique、enum CHECK。 |
| `runtime_health` | Runtime health snapshot | 保留；与 lease/inventory 分离正确。 |
| `backend_execution_leases` | Execution occupancy fact | 保留；busy/idle 来源。 |
| `project_backend_access` | Project/backend authorization | 保留；补 backend FK、CHECK、index。 |
| `backend_workspace_inventory` | Backend directory facts/cache | 保留；补 backend FK/CHECK，删除 behavior default。 |
| `views` | Legacy/global UI saved view | 代码改造后删除或迁到 scoped saved UI state。 |
| `user_preferences` | Legacy/global preference blob | 代码改造后删除，收敛到 `settings`。 |
| `state_changes` | Project event stream/outbox | 保留；删除 invalid defaults，补 `(project_id, id)` index。 |

## Proposed Implementation Slices

### Slice 1: P0 Baseline Correctness

目标：先让干净 init 与当前代码契约一致，并删除最明确的历史残留。

- 修复 `workspaces.mount_capabilities` 缺列。
- 删除 `lifecycle_runs.record_artifacts` 及 repository insert placeholder。
- 删除零 UUID / empty project / `backfill` defaults。
- 统一 `runtime_session_execution_anchors` ID 类型。
- 为上述改动补最小 repository/domain/API 调整。
- 验证：空 embedded PostgreSQL 初始化、readiness、`cargo check -p agentdash-infrastructure`、相关 repository 测试或精简集成脚本。

### Slice 2: Hand-Curated `0001_init.sql`

目标：把 baseline 从 dump 改成可维护 schema 文档。

- 删除 dump 注释/public 前缀噪音。
- 统一 table/constraint/index ordering。
- 统一 constraint/index 命名。
- 将明显 stale constraint name 改成当前字段语义。
- 按表补基础 FK/CHECK/unique/index，但避免一次性改动所有跨层语义字段。
- 验证：干净库 migration only 运行一次；55 张业务表存在；关键 FK/CHECK 生效。

### Slice 3: Session Runtime Head 收敛

目标：让 `sessions` 只表达 RuntimeSession head。

- 设计并引入 session list/runtime summary projection。
- 迁出 `project_id`、`executor_config_json`、`tab_layout_json`。
- 将 last_* 改为 projection 表或 projection 命名。
- 更新 API/contracts/frontend list/detail 使用。
- 验证：session create/turn append/fork/lineage/projection rollback。

### Slice 4: Lifecycle Run Ledger 收敛

目标：拆出 `active_node_keys` 与 `execution_log`。

- 从 workflow graph instance activity state 派生 active attempts。
- 新增 append-only lifecycle execution event 表。
- `LifecycleRunView` 从 event 表和 graph instance projection 组装。
- 更新 VFS lifecycle provider/journey surfaces。
- 验证：workflow dispatch、activity advance、human gate、execution log 展示。

### Slice 5: Legacy UI/Preference 与 Backend Identity 收敛

目标：删除 `user_preferences`，决定 `views` 命运，整理 backend local identity。

- BackendRepository 移除 preferences/views 方法或迁到 scoped saved view repository。
- PI/user preferences 全部走 `settings`。
- `views` 删除或迁为 user/project scoped saved state。
- `backends` local identity/share/claim 字段拆分或至少重命名/约束。
- 验证：settings save/load、backend list/runtime summary、local runtime ensure、workspace candidate/sync。

### Slice 6: JSONB / Constraint Sweep

目标：统一 JSON 和 enum 约束策略。

- `llm_providers.models/blocked_models` 改 JSONB。
- 补 LLM provider credential/status CHECK。
- 补 permission grants JSONB 查询修复。
- 补 project/story/workspace/agent/routine enum CHECK。
- 验证：provider CRUD/effective model profile、permission grant approve/revoke、project/story/agent CRUD。

## Validation Plan

最小验证：

- 空 embedded PostgreSQL 初始化后只运行 `0001`，且 readiness 需要的表全部存在。
- `cargo check -p agentdash-infrastructure`
- `cargo check -p agentdash-api`

针对 P0：

- Workspace create/list/get/update 能访问 `mount_capabilities`。
- Lifecycle run create/update 不再写 `record_artifacts`。
- Activity claim insert 不依赖 zero graph instance default。
- Runtime session execution anchor upsert/read 类型一致。

针对 Session/Lifecycle 语义改造：

- Session append event 后 list/detail/fork/projection rollback 正常。
- Workflow dispatch 生成 run/graph instance/agent/frame/assignment/gate/anchor 正常。
- LifecycleRunView 能展示 runtime trace refs 与 execution log。

针对 backend/settings：

- `settings` user scope 保存/读取 PI user preferences。
- BackendRepository 不再依赖 `views/user_preferences` 后 readiness 仍正确。
- Project stream `state_changes` project cursor 正常，`(project_id, id)` index 命中。

已知外部风险：

- 上一轮 `cargo clippy --all-targets` 暴露了既有无关测试编译错误：`crates/agentdash-api/src/routes/sessions.rs` 的测试参数类型与 `resolve_session_prompt_lifecycle` 签名不一致。该问题不属于本评估，但后续全量 clippy 前需要单独修。

## Research Sources

- `.trellis/tasks/06-03-database-semantic-baseline-audit/research/core-business.md`
- `.trellis/tasks/06-03-database-semantic-baseline-audit/research/session-runtime.md`
- `.trellis/tasks/06-03-database-semantic-baseline-audit/research/workflow-lifecycle.md`
- `.trellis/tasks/06-03-database-semantic-baseline-audit/research/platform-assets-config.md`
- `.trellis/tasks/06-03-database-semantic-baseline-audit/research/backend-local-runtime-and-dump-quality.md`
