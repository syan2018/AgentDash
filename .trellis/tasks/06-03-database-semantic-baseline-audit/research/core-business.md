# Research: Core business schema

- Query: 评估 projects / project_subject_grants / workspaces / workspace_bindings / stories / project_agents / project_vfs_mounts / canvases / canvas_files / canvas_bindings 的字段语义正确性。
- Scope: internal
- Date: 2026-06-03

## Findings

### Files Found

- `crates/agentdash-infrastructure/migrations/0001_init.sql` - 当前 PostgreSQL baseline，目标表 DDL 位于 `canvas_bindings`、`canvas_files`、`canvases`、`project_agents`、`project_subject_grants`、`project_vfs_mounts`、`projects`、`stories`、`workspace_bindings`、`workspaces`。
- `crates/agentdash-infrastructure/src/persistence/postgres/project_repository.rs` - `projects` / `project_subject_grants` 的 PostgreSQL repository。
- `crates/agentdash-domain/src/project/entity.rs` / `authorization.rs` - Project 聚合和 Project 授权语义。
- `crates/agentdash-infrastructure/src/persistence/postgres/workspace_repository.rs` - `workspaces` + `workspace_bindings` 单聚合事务读写。
- `crates/agentdash-domain/src/workspace/entity.rs` / `value_objects.rs` / `identity_contract.rs` - Workspace 逻辑身份、binding、detect facts 到 identity payload 的转换。
- `crates/agentdash-infrastructure/src/persistence/postgres/story_repository.rs` - Story aggregate 持久化，`stories.tasks` JSONB 与 `task_count` 读写。
- `crates/agentdash-domain/src/story/entity.rs` / `repository.rs` - Story aggregate root 与 Task child entity 语义。
- `crates/agentdash-infrastructure/src/persistence/postgres/agent_repository.rs` - `project_agents` repository。
- `crates/agentdash-domain/src/agent/entity.rs` / `repository.rs` - ProjectAgent 领域实体。
- `crates/agentdash-api/src/routes/project_agents.rs` / `crates/agentdash-contracts/src/project_agent.rs` - ProjectAgent HTTP DTO 和 create/update 暴露。
- `crates/agentdash-infrastructure/src/persistence/postgres/project_vfs_mount_repository.rs` - `project_vfs_mounts` repository。
- `crates/agentdash-domain/src/project_vfs_mount/entity.rs` - Project VFS Mount 领域实体，content 枚举。
- `crates/agentdash-infrastructure/src/persistence/postgres/canvas_repository.rs` - `canvases` + `canvas_files` + `canvas_bindings` repository。
- `crates/agentdash-domain/src/canvas/entity.rs` / `repository.rs` - Canvas 聚合。
- `crates/agentdash-application/src/vfs/provider_canvas.rs` - Canvas 文件作为 `canvas_fs` provider 的 VFS 内容。
- `crates/agentdash-application/src/workspace/backend_sync.rs` / `crates/agentdash-api/src/routes/backend_access.rs` / `crates/agentdash-api/src/workspace_resolution.rs` - Workspace binding 与 backend inventory / execution placement 的关系。
- `.trellis/spec/backend/database-guidelines.md` - schema baseline、repository、聚合事务和 seed/runtime 数据边界规范。
- `.trellis/spec/backend/architecture.md` - Project 授权、Canvas promote、session/runtime 分层约束。
- `.trellis/spec/cross-layer/project-backend-workspace-routing.md` - Project backend access、workspace binding、backend inventory 的契约。
- `.trellis/spec/backend/story-task-runtime.md` - Story / Task / Lifecycle / RuntimeSession 语义边界。
- `.trellis/spec/backend/vfs/vfs-access.md` / `vfs-materialization.md` / `vfs/architecture.md` - VFS address、Project VFS Mount、Canvas session visibility、materialization key 约束。

### Code Patterns

- 初始化 migration 只应表达 schema、约束、索引和必要扩展，runtime health、backend registration、session / lifecycle runtime facts 由 use case 或 runtime repository 写入（`.trellis/spec/backend/database-guidelines.md:46`）。
- `WorkspaceRepository` 应在单一事务内写 `workspaces` + `workspace_bindings`，Story aggregate 的 Task CRUD 走 `StoryRepository::update` 整体写回（`.trellis/spec/backend/database-guidelines.md:30`、`.trellis/spec/backend/database-guidelines.md:32`）。
- `RuntimeSession` 只承载 trace/debug/projection，不拥有 business ownership、permission scope 或 Lifecycle progress truth（`.trellis/spec/project-overview.md:37`、`.trellis/spec/backend/session/architecture.md:5`、`.trellis/spec/backend/session/architecture.md:30`）。
- Workspace binding / inventory 只表达目录事实与已确认 workspace root，不表达执行空闲状态；session 执行 placement 由 backend execution lease / allocator 维护（`.trellis/spec/cross-layer/project-backend-workspace-routing.md:44`、`.trellis/spec/cross-layer/project-backend-workspace-routing.md:45`）。
- Project 授权规则由 domain 的 `ProjectAuthorizationService` 表达，subject grants 是 Project 聚合语义（`.trellis/spec/backend/architecture.md:69`、`.trellis/spec/backend/architecture.md:77`）。
- Story 是 aggregate root，Task 是 Story aggregate 下 child entity，保存于 `stories.tasks` JSONB；Task 本体不拥有 runtime truth（`.trellis/spec/backend/story-task-runtime.md:5`、`.trellis/spec/backend/story-task-runtime.md:12`、`.trellis/spec/backend/story-task-runtime.md:18`）。
- Project VFS Mount 使用外部 `mount_id` 作为路径身份，数据库 UUID 只服务持久化和 inline owner（`.trellis/spec/backend/vfs/architecture.md:46`、`.trellis/spec/backend/vfs/vfs-access.md:173`）。
- Canvas runtime mount id 是 `cvs-<canvas.mount_id>`，visible canvas mount ids 存在 session/agent frame runtime projection，而不是 `canvases` 表（`.trellis/spec/backend/vfs/vfs-access.md:61`、`.trellis/spec/backend/vfs/vfs-access.md:65`、`.trellis/spec/backend/vfs/vfs-access.md:66`、`.trellis/spec/backend/vfs/vfs-access.md:67`）。
- Canvas 发布插件是 application canvas promotion 用例，从 Canvas 聚合生成 extension package artifact（`.trellis/spec/backend/architecture.md:67`）。

### Table Audit

#### projects

DDL: `projects` 建表字段为 `id/name/description/config/created_by_user_id/updated_by_user_id/visibility/is_template/cloned_from_project_id/created_at/updated_at`（`crates/agentdash-infrastructure/migrations/0001_init.sql:641`）。

字段分类:

| 字段 | 分类 | 评估 |
| --- | --- | --- |
| `id` | business fact | Project 聚合身份，保留。 |
| `name`, `description` | business fact | Project 用户可见元数据，保留。 |
| `config` | business fact / config | `ProjectConfig` 领域配置，由 repository JSON text 读写，保留；后续可按类型稳定度评估是否拆列。 |
| `created_by_user_id`, `updated_by_user_id` | audit / permission-adjacent business fact | create 时自动写 owner grant，Project route/auth 需要 creator/updater 语义，保留。 |
| `visibility`, `is_template` | business fact / config | `ProjectAuthorizationService` 使用 visibility 判断 template 可见性；`is_template` 与 visibility 目前双轨，保留但应整理不变量。 |
| `cloned_from_project_id` | audit / lineage fact | clone 来源，保留。 |
| `created_at`, `updated_at` | audit | 保留。 |

动作建议:

- 应保留但整理: `visibility` 与 `is_template` 需要明确约束或合并语义。代码中 `ProjectAuthorizationService` 以 `visibility == TemplateVisible` 判定无 grant 可 view（`crates/agentdash-domain/src/project/authorization.rs:69`），测试同时把 `is_template` 设置为 visibility 派生值（`crates/agentdash-domain/src/project/authorization.rs:180`）。如果 `is_template` 只是 `visibility` 的派生标志，则应移除或改为只由模板资产模型表达；如果它表示“可 clone 的 Project 模板”而 visibility 只表示可见性，则需 check constraint/领域不变量。
- 应保留但约束需整理: baseline 没有 FK/check。至少应给 `visibility` 加 check，`cloned_from_project_id` 可加自引用 FK，`created_by_user_id/updated_by_user_id` 是否 FK 取决于 auth user 生命周期。

#### project_subject_grants

DDL: `project_subject_grants(project_id, subject_type, subject_id, role, granted_by_user_id, created_at, updated_at)`（`crates/agentdash-infrastructure/migrations/0001_init.sql:608`），主键为 `(project_id, subject_type, subject_id)`（`crates/agentdash-infrastructure/migrations/0001_init.sql:1429`）。

字段分类:

| 字段 | 分类 | 评估 |
| --- | --- | --- |
| `project_id` | business fact | Project grant owner，保留。 |
| `subject_type`, `subject_id` | business fact | User / group subject grant，保留。 |
| `role` | business fact | Owner/editor/viewer 授权，保留。 |
| `granted_by_user_id` | audit | 授权来源，保留。 |
| `created_at`, `updated_at` | audit | 保留。 |

动作建议:

- 看似怪但应保留: 这是 Project 聚合权限事实，不是通用 `permission_grants` runtime/control-plane 事实。Project 授权 spec 明确放在 domain（`.trellis/spec/backend/architecture.md:69`）。
- 应保留但约束需整理: `subject_type`、`role` 应加 check；`project_id` 应 FK 到 `projects(id)`。`upsert_subject_grant_in_tx` 依赖 `(project_id, subject_type, subject_id)` 冲突键（`crates/agentdash-infrastructure/src/persistence/postgres/project_repository.rs:137`）。
- 可考虑位置不对: 如果后续组织/团队 ACL 成为平台级模型，可迁移到更通用 ACL 表；当前代码和 spec 均支持保留在 Project 聚合下。

#### workspaces

DDL: `workspaces(id, project_id, name, identity_kind, identity_payload, resolution_policy, default_binding_id, status, created_at, updated_at)`（`crates/agentdash-infrastructure/migrations/0001_init.sql:1091`）。

字段分类:

| 字段 | 分类 | 评估 |
| --- | --- | --- |
| `id`, `project_id` | business fact | Project 下逻辑 Workspace 身份，保留。 |
| `name` | business fact | 用户可见 workspace 名称，保留。 |
| `identity_kind`, `identity_payload` | business fact | 逻辑 workspace identity，不是 backend 目录；保留。 |
| `resolution_policy` | config | binding 解析策略，保留。 |
| `default_binding_id` | config / business preference | Workspace 默认物理 binding，保留但应加完整性约束。 |
| `status` | business lifecycle / projection-mixed | 当前由 sync 标记 Ready；不是执行 runtime 状态。保留但建议明确语义或改名为 `binding_status`/`availability_status`。 |
| `created_at`, `updated_at` | audit | 保留。 |
| `mount_capabilities` | config | 代码契约中必需，但 baseline 缺失；应加入 `TEXT DEFAULT '[...]' NOT NULL` 或调整代码。 |

关键发现:

- Critical: baseline 缺少 `mount_capabilities`，但 repository create/select/list/update 均读写该列（`crates/agentdash-infrastructure/src/persistence/postgres/workspace_repository.rs:110`、`crates/agentdash-infrastructure/src/persistence/postgres/workspace_repository.rs:135`、`crates/agentdash-infrastructure/src/persistence/postgres/workspace_repository.rs:155`、`crates/agentdash-infrastructure/src/persistence/postgres/workspace_repository.rs:209`、`crates/agentdash-infrastructure/src/persistence/postgres/workspace_repository.rs:303`）。领域实体也把它作为 mount 能力配置（`crates/agentdash-domain/src/workspace/entity.rs:28`、`crates/agentdash-domain/src/workspace/entity.rs:59`）。这是 migration baseline 与代码 contract 不一致，优先级高于语义清理。
- 看似怪但应保留: `identity_payload` 不是 runtime detected facts；它是逻辑 identity contract。`identity_payload_matches` 用它和 backend inventory/binding facts 匹配（`crates/agentdash-domain/src/workspace/identity_contract.rs:223`、`crates/agentdash-domain/src/workspace/identity_contract.rs:261`）。
- 应保留但整理: `default_binding_id` 当前没有 FK，且 repository 读出后 `refresh_default_binding` 会在内存中纠偏（`crates/agentdash-domain/src/workspace/entity.rs:69`）。基线应表达约束，或明确允许 default binding 自动选择而不持久化该列。

#### workspace_bindings

DDL: `workspace_bindings(id, workspace_id, backend_id, root_ref, status, detected_facts, last_verified_at, priority, created_at, updated_at)`（`crates/agentdash-infrastructure/migrations/0001_init.sql:1073`），主键为 `id`（`crates/agentdash-infrastructure/migrations/0001_init.sql:1637`）。

字段分类:

| 字段 | 分类 | 评估 |
| --- | --- | --- |
| `id`, `workspace_id` | business fact | Workspace 聚合内 binding 身份，保留。 |
| `backend_id`, `root_ref` | business setup fact / physical binding fact | “某 backend 上确认的 workspace root”，保留。不是 execution lease。 |
| `status` | runtime-adjacent projection / setup status | 表达 binding 可用性/探测状态，不表达 backend 执行空闲；保留但命名需谨慎。 |
| `detected_facts` | projection/cache from backend detect | 从 runtime detect 得到，但用于匹配 identity 与后续 UI 展示；保留，建议 jsonb。 |
| `last_verified_at` | audit / cache freshness | detect/inventory freshness，保留。 |
| `priority` | config | 多 binding 选择偏好，保留。 |
| `created_at`, `updated_at` | audit | 保留。 |

动作建议:

- 看似怪但应保留: workspace binding 与 backend 关系不是 runtime lease。Spec 明确 binding/inventory 只表达目录事实，不表达执行空闲（`.trellis/spec/cross-layer/project-backend-workspace-routing.md:45`）。Session launch 会从 VFS mount hint 推导 backend selection，并由 backend execution lease 记录执行占用（`crates/agentdash-application/src/session/launch/planner.rs:363`、`crates/agentdash-application/src/session/launch/planner.rs:383`）。
- 应保留但约束需整理: 应加 `(workspace_id, backend_id, root_ref)` unique，`workspace_id` FK，`status` check。当前 repository 每次 update 先删后插 binding（`crates/agentdash-infrastructure/src/persistence/postgres/workspace_repository.rs:34`），没有 DB 约束会允许重复事实漂移。
- 应保留但类型需整理: `detected_facts` 是 JSON 文本，应改为 `jsonb`，与 backend inventory 的 detected facts 语义一致。当前代码先序列化为 string 再写入 text（`crates/agentdash-infrastructure/src/persistence/postgres/workspace_repository.rs:47`、`crates/agentdash-infrastructure/src/persistence/postgres/workspace_repository.rs:58`）。

#### stories

DDL: `stories(id, project_id, default_workspace_id, title, description, status, priority, story_type, tags, task_count, context, created_at, updated_at, tasks)`（`crates/agentdash-infrastructure/migrations/0001_init.sql:984`）；`task_count` 默认 0（`crates/agentdash-infrastructure/migrations/0001_init.sql:994`），`tasks` 为 JSONB 默认 `[]`（`crates/agentdash-infrastructure/migrations/0001_init.sql:998`）。

字段分类:

| 字段 | 分类 | 评估 |
| --- | --- | --- |
| `id`, `project_id` | business fact | Story 聚合身份，保留。 |
| `default_workspace_id` | business preference / config | Story 级默认 workspace，保留。 |
| `title`, `description`, `priority`, `story_type`, `tags`, `context` | business fact | Story authoring/spec/context，保留；JSON text 字段可类型整理。 |
| `status` | business lifecycle projection | Story 自身状态，不是 RuntimeSession truth；保留但应与 lifecycle projection 明确边界。 |
| `tasks` | business aggregate data with projection subfields | Task child entities 物理存储，保留。 |
| `task_count` | projection/cache / historical residue candidate | 可由 `jsonb_array_length(tasks)` 派生；当前读出已忽略 DB 值，建议移除或改为 generated column。 |
| `created_at`, `updated_at` | audit | 保留。 |

重点字段:

- `stories.tasks`: 应保留。Domain 和 spec 明确 Task 已合入 Story aggregate；repository 用 JSONB containment 从 task id 反查 Story（`crates/agentdash-domain/src/story/repository.rs:11`、`crates/agentdash-domain/src/story/entity.rs:37`、`crates/agentdash-infrastructure/src/persistence/postgres/story_repository.rs:380`）。
- `stories.task_count`: 需要代码改造后移除或改为 generated column。Repository create/update 写入 `tasks.len()`（`crates/agentdash-infrastructure/src/persistence/postgres/story_repository.rs:34`、`crates/agentdash-infrastructure/src/persistence/postgres/story_repository.rs:403`），但读出时明确以 `tasks.len()` 为准并丢弃 `row.task_count`（`crates/agentdash-infrastructure/src/persistence/postgres/story_repository.rs:323`、`crates/agentdash-infrastructure/src/persistence/postgres/story_repository.rs:326`）。领域实体仍暴露 `task_count` 给 UI（`crates/agentdash-domain/src/story/entity.rs:32`）且 contracts 暴露该字段（`crates/agentdash-contracts/src/core.rs:921`）。语义上它是 projection/cache，不应作为可漂移业务事实保存在 baseline 普通列中。

动作建议:

- 需要代码改造后移除: 删除 `task_count` 普通列，domain/API response 用 `tasks.len()` 计算；或保留为 `GENERATED ALWAYS AS (jsonb_array_length(tasks)) STORED`，不再由 repository 写入。
- 应保留但类型需整理: `tags`、`context` 当前是 JSON text；`tasks` 已是 JSONB。建议一致化为 `jsonb`，尤其 `tags` 可支持过滤索引。
- 应保留但约束需整理: `status`、`priority`、`story_type` check；`project_id/default_workspace_id` FK。

#### project_agents

DDL: `project_agents(id, project_id, name, agent_type, config, installed_* source fields, default_lifecycle_key, is_default_for_story, is_default_for_task, knowledge_enabled, created_at, updated_at)`（`crates/agentdash-infrastructure/migrations/0001_init.sql:534`），唯一键 `(project_id, name)`（`crates/agentdash-infrastructure/migrations/0001_init.sql:1389`）。

字段分类:

| 字段 | 分类 | 评估 |
| --- | --- | --- |
| `id`, `project_id`, `name`, `agent_type` | business fact / config | Project 内可运行 Agent 实例，保留。 |
| `config` | config | AgentPresetConfig JSON，保留；可类型整理为 jsonb。 |
| `installed_library_asset_id`, `installed_source_ref`, `installed_source_version`, `installed_source_digest`, `installed_at` | audit / install provenance | Shared Library 安装来源，保留但应约束成整体 optional。 |
| `default_lifecycle_key` | config | Story/project agent launch 使用，保留。 |
| `is_default_for_story` | config | 当前真实使用，保留但应约束每 Project 唯一。 |
| `is_default_for_task` | historical residue / unused config | 当前仅 DTO/repository 读写，未发现 application 选择逻辑使用；建议移除或迁入明确 Task dispatch policy。 |
| `knowledge_enabled` | config | ProjectAgent knowledge VFS 是否启用，保留。 |
| `created_at`, `updated_at` | audit | 保留。 |

重点字段:

- `is_default_for_story` 被 Story launch 和 session workflow context 使用：`resolve_default_story_project_agent` 查找 `is_default_for_story`（`crates/agentdash-application/src/story/lifecycle_launch.rs:139`），session assembler 也查 story 默认 agent（`crates/agentdash-application/src/session/assembler.rs:1711`、`crates/agentdash-application/src/session/assembler.rs:1736`）。
- `is_default_for_task` 当前在 domain、repository、contracts、API create/update 中读写（`crates/agentdash-domain/src/agent/entity.rs:33`、`crates/agentdash-infrastructure/src/persistence/postgres/agent_repository.rs:72`、`crates/agentdash-contracts/src/project_agent.rs:82`、`crates/agentdash-api/src/routes/project_agents.rs:379`、`crates/agentdash-api/src/routes/project_agents.rs:446`），但未发现 application 层按它选择 Task default agent。根据 Story/Task spec，Task execution 应通过 SubjectRef/Lifecycle association 与 dispatch policy，而不是在 ProjectAgent 表上放一个未消费的默认标志（`.trellis/spec/backend/story-task-runtime.md:18`）。

动作建议:

- 需要代码改造后移除: `is_default_for_task`。如果 Task 需要默认 agent，应放在 Task authoring preference、Story context、Project config 的明确 dispatch policy，或 lifecycle/agent procedure binding 中，而不是 ProjectAgent 上的孤立布尔列。
- 应保留但约束需整理: `is_default_for_story` 应用 partial unique index：每个 project 最多一个 true。`installed_*` 字段应有 all-or-none check，`installed_source_digest` 应有 digest 格式 check。
- 应保留但类型需整理: `config` 应为 `jsonb`；`agent_type`、`default_lifecycle_key` 是否 FK/枚举由 capability/provider registry 决定。

#### project_vfs_mounts

DDL: `project_vfs_mounts(id, project_id, mount_id, display_name, description, capabilities, installed_source, content, created_at, updated_at)`（`crates/agentdash-infrastructure/migrations/0001_init.sql:623`），唯一键 `(project_id, mount_id)`（`crates/agentdash-infrastructure/migrations/0001_init.sql:1445`）。

字段分类:

| 字段 | 分类 | 评估 |
| --- | --- | --- |
| `id`, `project_id` | business fact / storage owner | 保留。UUID 服务持久化和 inline storage owner。 |
| `mount_id` | business-facing runtime address identity | 保留。VFS 稳定路径身份。 |
| `display_name`, `description` | business fact | 用户可见元数据，保留。 |
| `capabilities` | config | mount 能力，保留；建议 jsonb。 |
| `installed_source` | audit / install provenance | Shared Library 安装来源，保留。 |
| `content` | config | `Inline` 或 `ExternalService{service_id,root_ref}`，保留。 |
| `created_at`, `updated_at` | audit | 保留。 |

动作建议:

- 看似怪但应保留: `id` 和 `mount_id` 双身份正确。Spec 明确 `mount_id` 是外部路径标识，数据库 UUID 只服务持久化和 inline storage owner（`.trellis/spec/backend/vfs/vfs-access.md:173`）。Repository 也按 `(project_id, mount_id)` 查询/删除（`crates/agentdash-infrastructure/src/persistence/postgres/project_vfs_mount_repository.rs:103`、`crates/agentdash-infrastructure/src/persistence/postgres/project_vfs_mount_repository.rs:166`）。
- 看似怪但应保留: `content` 存储的是 mount 内容来源配置，不是文件内容。Inline Project VFS Mount 的文件内容应存在 `inline_fs_files`，owner 为 `project_vfs_mount`，spec 明确 Project VFS Mount files 复用 InlineFile（`.trellis/spec/backend/vfs/vfs-access.md:115`）。
- 应保留但类型需整理: `capabilities`、`installed_source`、`content` 均是 JSON text，建议改 `jsonb` 并加 content kind check。

#### canvases

DDL: `canvases(id, project_id, mount_id, title, description, entry_file, sandbox_config, created_at, updated_at)`（`crates/agentdash-infrastructure/migrations/0001_init.sql:240`）。

字段分类:

| 字段 | 分类 | 评估 |
| --- | --- | --- |
| `id`, `project_id` | business fact | Project 级 Canvas 聚合，保留。 |
| `mount_id` | business-facing runtime address identity | 保留，但应唯一约束。 |
| `title`, `description` | business fact | 保留。 |
| `entry_file` | config / business fact | Canvas runtime entry，保留。 |
| `sandbox_config` | config | Canvas 执行/渲染沙箱配置，保留；建议 jsonb。 |
| `created_at`, `updated_at` | audit / conservative version token | 保留；provider 用 canvas.updated_at 作文件 version token。 |

动作建议:

- 看似怪但应保留: `mount_id` 是 Canvas 被暴露成 `canvas_fs` runtime mount 的身份来源。Spec 要求 runtime mount id 为 `cvs-<canvas.mount_id>`（`.trellis/spec/backend/vfs/vfs-access.md:65`），Canvas skill 文档也以 `<mount_id>://...` 编辑文件。
- 应保留但约束需整理: repository 有 `get_by_mount_id(project_id, mount_id)` 和全局 `find_by_mount_id(mount_id)`（`crates/agentdash-domain/src/canvas/repository.rs:10`、`crates/agentdash-domain/src/canvas/repository.rs:16`）。DDL 未见 unique 约束；至少应加 `(project_id, mount_id)` unique。若全局 lookup 是产品语义，则加全局 unique；否则删除/限制 `find_by_mount_id` 的无 project 查询。
- 应保留但类型需整理: `sandbox_config` 应改 `jsonb`，并考虑 `entry_file` 必须存在于 `canvas_files` 的约束或 repository 校验。

#### canvas_files

DDL: `canvas_files(canvas_id, path, content)`（`crates/agentdash-infrastructure/migrations/0001_init.sql:229`），主键 `(canvas_id, path)`（`crates/agentdash-infrastructure/migrations/0001_init.sql:1221`）。

字段分类:

| 字段 | 分类 | 评估 |
| --- | --- | --- |
| `canvas_id` | business aggregate child key | 保留。 |
| `path` | business/runtime-address fact | Canvas source file path，保留。 |
| `content` | business asset content | 保留，但当前只支持 text。 |

动作建议:

- 看似怪但应保留: Canvas 文件没有使用通用 InlineFile，而是 Canvas 聚合子表；这与 `canvas_fs` provider 直接读写 Canvas 聚合一致（`crates/agentdash-application/src/vfs/provider_canvas.rs:176`、`crates/agentdash-application/src/vfs/provider_canvas.rs:260`）。Canvas promote 也从 Canvas 聚合生成插件包（`.trellis/spec/backend/architecture.md:67`）。
- 位置不对/应迁移候选: 长期看，Canvas 文件与 Project VFS Mount inline files 都是 VFS editable assets，但 Canvas 的 runtime entry、system skill 防覆盖、promotion 打包使它现在更像 Canvas 聚合私有 asset。若要统一，应迁移到 `inline_fs_files(owner_kind=canvas)`，但需要完整代码改造；当前不建议直接移除。
- 应保留但约束需整理: `canvas_id` 应 FK cascade；`path` 应加 normalized relative path check；`content` 只支持 text，若 Canvas 支持图片/二进制 asset，应引入 typed content 或复用 InlineFile binary contract。

#### canvas_bindings

DDL: `canvas_bindings(canvas_id, alias, source_uri, content_type)`（`crates/agentdash-infrastructure/migrations/0001_init.sql:217`），主键 `(canvas_id, alias)`（`crates/agentdash-infrastructure/migrations/0001_init.sql:1213`）。

字段分类:

| 字段 | 分类 | 评估 |
| --- | --- | --- |
| `canvas_id` | business aggregate child key | 保留。 |
| `alias` | business-facing binding name | 保留。 |
| `source_uri` | runtime/data-source reference | 混合字段：引用 lifecycle/session/project 数据源，不是 Canvas 文件内容。 |
| `content_type` | config / projection hint | 保留；默认 `application/json` 可接受但需校验。 |

动作建议:

- 位置不对/应迁移候选: `canvas_bindings` 混合了 Canvas 业务资产和 runtime/data context 引用。当前 repository 将其作为 Canvas 聚合 child entity 整体替换（`crates/agentdash-infrastructure/src/persistence/postgres/canvas_repository.rs:135`、`crates/agentdash-infrastructure/src/persistence/postgres/canvas_repository.rs:146`），但 `source_uri` 可能指向 lifecycle/runtime projection。更正确的长期位置可能是 Canvas data binding spec/config 表，或 extension/runtime channel binding，而不是与 source files 同等子表。
- 需要代码改造后整理: 保留表但重命名/重构为 `canvas_data_bindings`，并给 `source_uri` 引入结构化类型或 URI scheme check。当前 `CanvasDataBinding::new` 默认 content type 为 `application/json`，但 DDL 没有限制 source scheme / alias 格式。
- 看似怪但应保留: 不能直接删。Canvas repository、DTO、runtime snapshot/promotion 需要这些 binding 随 Canvas 聚合保存和替换。

### Highest Priority Recommendations

1. 先修 `workspaces.mount_capabilities` baseline 契约缺口。当前 `0001_init.sql` 没有该列，但 repository 必读写，干净库会在 Workspace 创建/查询时失败。
2. 把 `stories.task_count` 从普通持久化事实降级：删除列并由 API/domain 计算，或改成 generated column。当前 repository 已经证明 DB 值不是事实源。
3. 删除或迁移 `project_agents.is_default_for_task`。它目前只是跨层 DTO/repository 读写字段，未参与 Task dispatch 选择，容易把 Task runtime policy 固化到 ProjectAgent 表。
4. 对 workspace binding 与 backend 关系保持现模型：binding 表保留目录事实，执行占用继续由 backend execution lease 表达；只做约束和 JSONB 类型整理。
5. Canvas 文件和 binding 暂不直接删除；`canvas_files` 是业务 asset，`canvas_bindings` 是混合 runtime/data-source config，后者应重命名/结构化，避免与文件内容表语义混杂。

## External References

- 未使用外部资料；本研究只基于本仓库 migration、Rust domain/repository/application/API 和 Trellis spec。

## Related Specs

- `.trellis/spec/backend/database-guidelines.md`
- `.trellis/spec/backend/architecture.md`
- `.trellis/spec/backend/story-task-runtime.md`
- `.trellis/spec/backend/session/architecture.md`
- `.trellis/spec/backend/vfs/architecture.md`
- `.trellis/spec/backend/vfs/vfs-access.md`
- `.trellis/spec/backend/vfs/vfs-materialization.md`
- `.trellis/spec/cross-layer/project-backend-workspace-routing.md`
- `.trellis/spec/cross-layer/shared-library-contract.md`

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` 返回 `Current task: (none)`；本文件根据用户显式给出的任务路径写入。
- 未运行编译或数据库 migration 测试；本研究为只读源码/spec/migration 审计。
- 未发现 `project_agents.is_default_for_task` 在 application 层参与 Task default agent 解析；只发现 domain/repository/API/contract 暴露与读写。
- 未发现目标表之间的 FK 约束；当前 baseline 主要有主键、少量唯一键和索引。建议后续实施时统一补 FK/check/partial unique。
- `workspaces.mount_capabilities` 是审计中发现的额外关键问题：它不在用户列举字段中，但属于 `workspaces` 当前代码契约的一部分，且 baseline 缺失。
