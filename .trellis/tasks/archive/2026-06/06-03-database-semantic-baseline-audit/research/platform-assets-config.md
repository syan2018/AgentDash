# Research: Platform assets/config schema

- Query: 正式评估 Platform assets/config schema 分区中 library_assets、skill_assets、inline_fs_files、mcp_presets、extension package artifact/install、settings、auth/identity、LLM provider、permission_grants 的表/字段语义正确性。
- Scope: internal
- Date: 2026-06-03

## Findings

### 读取范围

- 任务文档：`.trellis/tasks/06-03-database-semantic-baseline-audit/prd.md` 要求审计当前 `0001_init.sql` 的表、字段、索引/约束、默认值和数据归属；`design.md` 定义 Business fact、Runtime fact、Projection/cache、Outbox/audit、Seed/config、Historical residue 分类；`implement.md` 将本分区命名为 Platform assets and config。
- Migration：`crates/agentdash-infrastructure/migrations/0001_init.sql`。
- Domain / repository / application / API：shared library、skill asset、inline VFS、MCP preset、extension package、settings、auth session、identity directory、LLM provider、permission grant 相关模块。
- Specs：`.trellis/spec/backend/database-guidelines.md`、`.trellis/spec/cross-layer/shared-library-contract.md`、`.trellis/spec/backend/capability/llm-model-config.md`、`.trellis/spec/backend/permission/architecture.md`、`.trellis/spec/backend/capability/architecture.md`。

### 全局结论

- 本分区没有发现“整表可直接移除”的高置信候选；目标表都出现在 readiness 必需表清单中，且对应 repository/API/service 仍有当前语义使用：`migration.rs:18-34`、`migration.rs:50-55`。
- 最高优先级的 schema 收敛不是删表，而是整理字段类型/约束：`llm_providers.models`、`llm_providers.blocked_models` 当前 schema 是 `TEXT`，而领域/API 表达为 JSON value；同分区的 `library_assets.payload`、`extension_package_artifacts.manifest`、`project_extension_installations.config/manifest`、`permission_grants.*json` 已使用 `JSONB`，这里存在明显不一致。
- `permission_grants` 应被归类为 runtime/audit fact，而不是配置表。它是独立 Permission Grant 聚合根，记录 runtime capability 授权来源、状态机、策略决定和 effect frame 归属；保留在 baseline 合理，但主报告不应把它与 settings/LLM provider 等 config 混为一类。
- `inline_fs_files` 是当前独立 inline VFS blob/file storage，不是历史残留；但 `owner_kind` 目前只见 `skill_asset` 主线持久化，表设计偏通用，建议加 owner_kind 约束并把“支持哪些 owner”作为 schema contract 明确下来。
- `project_extension_installations` 同时保存 `package_artifact_id` 和 package/artifact 快照列，看似重复，但当前领域将其表达为安装时 `ExtensionPackageArtifactRef`，用于运行时下载、发布校验和审计快照；应保留，但应补强成“要么整组为空、要么整组非空”的约束，并考虑列名前缀统一。

### Files found

- `crates/agentdash-infrastructure/migrations/0001_init.sql` - 当前 PostgreSQL baseline，定义本分区所有目标表、约束和索引。
- `crates/agentdash-infrastructure/src/migration.rs` - schema readiness 必需表清单，包含目标分区全部核心表。
- `crates/agentdash-domain/src/shared_library/entity.rs` - `LibraryAsset` 聚合，`payload` 是 `serde_json::Value` 并按 `asset_type` 验证。
- `crates/agentdash-domain/src/shared_library/value_objects.rs` - `LibraryAssetPayload`、`InstalledAssetSource`、Extension template payload schema。
- `crates/agentdash-infrastructure/src/persistence/postgres/shared_library_repository.rs` - `library_assets` repository，使用 `sqlx::types::Json` 读写 `payload`。
- `crates/agentdash-domain/src/skill_asset/entity.rs` - Project skill asset 聚合。
- `crates/agentdash-infrastructure/src/persistence/postgres/skill_asset_repository.rs` - `skill_assets` 与 `inline_fs_files` 的读写实现。
- `crates/agentdash-application/src/vfs/inline_persistence.rs` - inline VFS 写回抽象，说明 inline 文件已从实体 JSONB 独立出来。
- `crates/agentdash-domain/src/mcp_preset/entity.rs` - Project MCP preset 聚合。
- `crates/agentdash-infrastructure/src/persistence/postgres/mcp_preset_repository.rs` - `mcp_presets` repository，`transport`/`route_policy` 以 JSON text 存储。
- `crates/agentdash-domain/src/extension_package.rs` - Extension package artifact 与 artifact ref 的领域模型。
- `crates/agentdash-domain/src/shared_library/project_extension.rs` - Project extension installation 聚合，包含 `installed_source` 与 `package_artifact`。
- `crates/agentdash-infrastructure/src/persistence/postgres/extension_package_artifact_repository.rs` - `extension_package_artifacts` repository。
- `crates/agentdash-infrastructure/src/persistence/postgres/project_extension_installation_repository.rs` - `project_extension_installations` repository，持久化 installed source 与 package artifact 快照。
- `crates/agentdash-application/src/shared_library/install.rs` - Marketplace install/source-status 逻辑。
- `crates/agentdash-application/src/shared_library/publish.rs` - publish 逻辑，校验 extension package artifact 与 installation manifest 一致。
- `crates/agentdash-application/src/extension_package.rs` - package archive 校验、存储、安装、下载读取。
- `crates/agentdash-domain/src/settings.rs` - settings scope/domain repository trait。
- `crates/agentdash-infrastructure/src/persistence/postgres/settings_repository.rs` - `settings` repository。
- `crates/agentdash-domain/src/auth_session/entity.rs` - auth session runtime/cache entity。
- `crates/agentdash-application/src/auth/session_service.rs` - token hash、identity JSON、JWT exp、revoke/cleanup 逻辑。
- `crates/agentdash-infrastructure/src/persistence/postgres/auth_session_repository.rs` - `auth_sessions` repository。
- `crates/agentdash-domain/src/identity/entity.rs` - `User` / `Group` directory entity。
- `crates/agentdash-infrastructure/src/persistence/postgres/user_directory_repository.rs` - `users` / `groups` / `group_memberships` repository。
- `crates/agentdash-domain/src/llm_provider/entity.rs` - LLM provider catalog、credential mode、user credential entity。
- `crates/agentdash-domain/src/llm_provider/resolver.rs` - global DB key/env key/user BYOK credential resolution。
- `crates/agentdash-infrastructure/src/persistence/postgres/llm_provider_repository.rs` - `llm_providers` 与 `llm_provider_user_credentials` repository。
- `crates/agentdash-application/src/llm_provider.rs` - provider create/update、secret encryption。
- `crates/agentdash-api/src/routes/llm_providers.rs` - admin/user provider credential API 和 Codex OAuth target 分流。
- `crates/agentdash-domain/src/permission/entity.rs` - `PermissionGrant` aggregate root + 状态机。
- `crates/agentdash-infrastructure/src/persistence/postgres/permission_grant_repository.rs` - `permission_grants` repository。
- `crates/agentdash-application/src/permission/service.rs` - permission grant lifecycle orchestration 与 effect frame 应用。
- `crates/agentdash-api/src/routes/permission_grants.rs` - permission grant list/get/approve/reject/revoke API。

### Code patterns

- Database spec 要求初始化 migration 只表达 schema、约束、索引和必要扩展；Builtin/Plugin Shared Library assets、LLM Provider、auth session、settings、backend registration、runtime health、session/lifecycle runtime facts 由 seed、API use case 或 runtime repository 写入，原因是这些数据随代码、插件、用户配置或运行状态变化，不属于 schema 基线：`.trellis/spec/backend/database-guidelines.md:46`。
- Shared Library 契约把 `LibraryAsset` 定义为公共资产存储，把 `InstalledAssetSource` 定义为 Project 资源的来源版本元数据，把 `ExtensionPackageArtifact` 定义为可由 Project 或 LibraryAsset 拥有的可校验运行产物：`.trellis/spec/cross-layer/shared-library-contract.md:11-14`。
- Shared Library 契约明确 Project 运行时读取安装后的 Project 资源，不直接把 `LibraryAsset.payload` 当运行配置编辑：`.trellis/spec/cross-layer/shared-library-contract.md:79`。
- Packaged Extension 契约明确：`ExtensionPackageArtifact` 由 `owner_kind + owner_id` 表达归属；Marketplace packaged install 写入 `installed_source + package_artifact`；本地包导入写入 Project-owned `package_artifact` 且 `installed_source = None`：`.trellis/spec/cross-layer/shared-library-contract.md:236`。
- Extension publish 契约明确：需要 package artifact 的 installation 必须携带 `package_artifact` 才能发布，artifact 关联键使用 owner 与 typed package identity，原因是 manifest digest 与 payload digest 属于不同摘要域：`.trellis/spec/cross-layer/shared-library-contract.md:252`。
- Settings scope 契约区分 system/user/project/local-runtime，其中 `agent.pi.user_preferences` 属于 user scope，不属于 system scope：`.trellis/spec/cross-layer/shared-library-contract.md:270-276`。
- LLM Provider 契约明确 `llm_providers` 是管理员维护的全局 Provider Catalog；全局 DB key 在 `global_api_key_ciphertext`，用户 BYOK 在 `llm_provider_user_credentials`，按 `provider_id + user_id` 唯一隔离：`.trellis/spec/backend/capability/llm-model-config.md:74`。
- LLM Provider 契约明确 `openai_codex` 的 token JSON 可写入全局 provider 或用户 credential，只在保存目标上区分所有权：`.trellis/spec/backend/capability/llm-model-config.md:90`。
- Permission architecture 明确 Permission System 管理 Agent runtime capability scope 的授权事实，授权 source 可带 runtime session/turn/tool provenance，effect 必须落到 `AgentFrame` revision 或 run/agent control scope association：`.trellis/spec/backend/permission/architecture.md:7`。
- Capability architecture 明确 Permission Grant applied 后的 requested paths 注入 `CapabilityContext.granted_capability_keys`，进入 runtime transition pipeline：`.trellis/spec/backend/capability/architecture.md:71-87`。

### Table-by-table semantic audit

#### library_assets

- 表定义：`id`、`asset_type`、`scope`、`owner_id`、`key`、`display_name`、`description`、`version`、`source`、`source_ref`、`payload_digest`、`deprecated`、`payload JSONB`、timestamps，带 asset type/scope/source check：`0001_init.sql:328-346`；唯一身份索引 `(asset_type, scope, COALESCE(owner_id,''), key)`：`0001_init.sql:1810-1813`。
- 字段分类：
  - business/config fact：`id`、`asset_type`、`scope`、`owner_id`、`key`、`display_name`、`description`、`version`、`source`、`source_ref`、`payload_digest`、`deprecated`、`payload`。
  - seed fact：builtin/plugin embedded assets 会经 seed 写入，但 seed 数据本身不属于 init migration。
  - runtime/audit fact：无。
  - blob storage：`payload` 是配置 JSON，不是任意 blob；Extension package archive 不在这里。
  - historical residue：未见明确历史残留。
- 证据：domain `LibraryAsset` 持有 `payload: Value` 且构造时按 `LibraryAssetPayload::validate(asset_type, &payload)` 校验：`shared_library/entity.rs:13-30`、`shared_library/entity.rs:51-58`；repository 用 `Json(asset.payload.clone())` 写入：`shared_library_repository.rs:26-47`。
- 建议：保留。`payload JSONB` 与实际领域/API 一致；`source_ref` 可为空但目前没有 source-specific check，建议后续补约束或在报告列为“保留但约束需整理”。

#### skill_assets

- 表定义：Project-scoped skill asset，包含 `source`、`builtin_key`、远端来源三列、`InstalledAssetSource` 四列、`disable_model_invocation` 和 timestamps：`0001_init.sql:922-942`。
- 字段分类：
  - business/config fact：`id`、`project_id`、`key`、`display_name`、`description`、`source`、`builtin_key`、`disable_model_invocation`。
  - seed fact：`builtin_key` 对 builtin seed 来源有意义。
  - runtime/audit fact：无。
  - blob storage：文件内容不在 `skill_assets`，在 `inline_fs_files`。
  - historical residue：`remote_source_url`、`remote_imported_at`、`remote_digest` 与 `library_asset_id/source_ref/source_version/source_digest/installed_at` 是两套来源模型并存；前者服务 github/clawhub/skills_sh remote import，后者服务 Shared Library install/source-status。不是可直接删除，但需要产品语义确认是否仍保留非 Shared Library remote import 来源。
- 证据：repository 主列同时映射 remote source 与 installed source：`skill_asset_repository.rs:24`、`skill_asset_repository.rs:113-127`、`skill_asset_repository.rs:166-182`；source mapper 强制 github/clawhub/skills_sh 需要 remote URL/imported_at/digest：`skill_asset_repository.rs:320-371`；installed source 解析要求四列成组存在：`skill_asset_repository.rs:528-551`。
- 建议：保留，但建议整理来源模型。若未来只保留 Shared Library 安装和用户自建，则 `remote_*` 可作为“需要代码/API 改造后移除”；若仍支持外部 skill registry 导入，则保留并补 source-specific CHECK。

#### inline_fs_files

- 表定义：`owner_kind`、`owner_id`、`container_id`、`path`，二选一 `text_content`/`binary_content`，带 content_kind/payload check 和 owner/path unique：`0001_init.sql:307-320`、`0001_init.sql:1256-1269`。
- 字段分类：
  - business/config fact：`owner_kind`、`owner_id`、`container_id`、`path`、`updated_at`。
  - blob storage：`text_content`、`binary_content`、`mime_type`、`size_bytes`、`content_kind`。
  - runtime/audit fact：无。
  - seed fact：可由 skill template/inline mount 初始内容 materialize，但不是 seed baseline 数据。
  - historical residue：未见；但 owner 模型过度通用。
- 证据：skill repository 只以 `owner_kind = 'skill_asset'` 读写文件：`skill_asset_repository.rs:197-209`、`skill_asset_repository.rs:245-269`；inline VFS persistence 文档说明实现方负责将 inline_fs mount 修改写回独立 `inline_fs_files`，不再加载整个 Project/Story entity：`inline_persistence.rs:54-57`、`inline_persistence.rs:222-269`。
- 建议：保留。需要整理：为 `owner_kind` 加 CHECK，至少把当前支持值落进 schema；如只支持 skill asset，应将通用 owner 命名收敛为更具体的 `skill_asset_files` 或加明确约束。当前 `owner_kind` 过度通用是高优先级设计债，但不是直接删除候选。

#### mcp_presets

- 表定义：Project MCP preset，包含 `transport TEXT`、`route_policy TEXT`、`source/builtin_key`、`InstalledAssetSource` 四列：`0001_init.sql:484-502`；project key/builtin key unique 索引：`0001_init.sql:1932-1953`。
- 字段分类：
  - business/config fact：`id`、`project_id`、`key`、`display_name`、`description`、`transport`、`route_policy`、`source`、`builtin_key`。
  - seed fact：builtin source/builtin_key。
  - runtime/audit fact：无。
  - blob storage：无；`transport` 是复杂值对象 JSON text。
  - historical residue：未见。
- 证据：domain `McpPreset` 持有 `transport: McpTransportConfig` 和 `installed_source: Option<InstalledAssetSource>`：`mcp_preset/entity.rs:16-28`；repository 将 `transport`/`route_policy` 作为 JSON/string text 读写，installed source 四列成组解析：`mcp_preset_repository.rs:23-44`、`mcp_preset_repository.rs:199-223`。
- 建议：保留。按照数据库规范“复杂值对象以 JSON 文本存入 TEXT”，`transport TEXT` 当前可接受；但同一分区存在大量 JSONB，主报告可提出统一原则：需要 DB 层 JSON 查询/约束的用 JSONB，仅 opaque value object 用 TEXT。

#### extension_package_artifacts

- 表定义：artifact 元数据、`storage_ref`、archive/manifest digest、`manifest JSONB`、`byte_size`、owner kind/id，带 digest format 与 owner_kind check：`0001_init.sql:257-275`；owner/digest unique：`0001_init.sql:1778-1792`。
- 字段分类：
  - business/config fact：`id`、`owner_kind`、`owner_id`、`extension_id`、`package_name`、`package_version`、`asset_version`、`source_version`。
  - blob storage：`storage_ref` 是 archive object reference；archive bytes 不进 DB。
  - runtime/audit fact：`archive_digest`、`manifest_digest`、`byte_size` 是可校验 artifact 审计事实。
  - seed fact：LibraryAsset-owned artifact 可由 publish/seed 流程产生，但数据不属于 init migration。
  - historical residue：未见。
- 证据：domain artifact owner 只允许 Project 或 LibraryAsset，owner_id 不能为空：`extension_package.rs:10-62`；artifact 构造校验 manifest、storage_ref、sha256 digest、byte_size：`extension_package.rs:145-203`；repository `manifest` 使用 JSONB `Json` 往返：`extension_package_artifact_repository.rs:30-52`、`extension_package_artifact_repository.rs:111-115`。
- 建议：保留。`source_version` 当前在 `ExtensionPackageArtifact::new` 中等于 `manifest.asset_version`：`extension_package.rs:195-200`，与 `asset_version` 可能语义重叠；建议主报告列为“保留但命名/来源需确认”，确认是否真正需要两列。

#### project_extension_installations

- 表定义：Project installed extension，包含 config/manifest JSONB、installed source 四列、package artifact ref 快照列：`0001_init.sql:578-600`；project + extension_key unique：`0001_init.sql:1416-1421`。
- 字段分类：
  - business/config fact：`id`、`project_id`、`extension_key`、`display_name`、`enabled`、`config`、`manifest`、timestamps。
  - seed/config source fact：`installed_library_asset_id`、`installed_source_ref`、`installed_source_version`、`installed_source_digest`、`installed_at`。
  - blob storage/reference fact：`package_artifact_id`、`artifact_storage_ref`、`artifact_archive_digest`、`artifact_manifest_digest`。
  - artifact identity snapshot：`package_name`、`package_version`、`package_asset_version`、`package_source_version`。
  - runtime/audit fact：package snapshot 具有 audit/replay value，但不是 live runtime state。
  - historical residue：未见直接残留；但 package ref 列与 artifact 表存在可 join 的重复快照。
- 证据：domain installation 明确同时携带 `installed_source` 和 `package_artifact`：`project_extension.rs:11-20`；本地 packaged install `new_packaged` 只有 package artifact，无 installed source：`project_extension.rs:43-56`；Marketplace packaged install `new_from_library_package` 两者都有：`project_extension.rs:60-74`；repository 插入/更新保存两组列：`project_extension_installation_repository.rs:39-74`、`project_extension_installation_repository.rs:87-114`；读回时只要 `package_artifact_id` 存在，就要求 package/artifact snapshot 列均非空：`project_extension_installation_repository.rs:347-364`。
- 建议：保留但整理。看似重复的 artifact 安装列应保留为安装时快照，因为 publish 会用安装 ref 校验源 artifact：`shared_library/publish.rs:261-317`，runtime/webview 读取也依赖 installation.package_artifact 的 storage/digest：`extension_package.rs:324-335`。应补 DB CHECK：installed source 组要么全空要么全非空；package artifact 组要么全空要么全非空；`artifact_*` 可考虑统一成 `package_artifact_*` 前缀，减少“artifact id + artifact_storage_ref”命名割裂。

#### settings

- 表定义：`scope_kind`、`scope_id DEFAULT ''`、`key`、`value TEXT`、`updated_at`，主键 `(scope_kind, scope_id, key)`：`0001_init.sql:909-915`、`0001_init.sql:1572-1573`。
- 字段分类：
  - business/config fact：`scope_kind`、`scope_id`、`key`、`value`。
  - runtime/audit fact：无。
  - seed fact：可由启动/API 写入，不属于 baseline 数据。
  - blob storage：无；`value` 是 opaque config JSON/string text。
  - historical residue：未见。
- 证据：domain scope 支持 system/user/project，system 用空 scope_id 存储：`settings.rs:6-57`；repository 按 `scope.storage_scope_id()` 读写，`value` 直接 TEXT 保存：`settings_repository.rs:31-50`、`settings_repository.rs:84-90`；读回时 system scope_id 归一为 None，user/project 禁止空 scope_id：`settings_repository.rs:153-174`。
- 建议：保留。需要整理：加 `scope_kind` CHECK；加 scope_id consistency CHECK，system 必须空、user/project 必须非空。`value TEXT` 与当前复杂值对象 TEXT 规则一致，但若 settings value 已被 API 视为 arbitrary JSON，可考虑 JSONB 化以统一。

#### auth_sessions

- 表定义：`token_hash`、`identity_json TEXT`、`expires_at BIGINT`、`revoked_at BIGINT`、created/updated epoch BIGINT：`0001_init.sql:127-133`；primary key token_hash：`0001_init.sql:1164-1165`。
- 字段分类：
  - runtime/audit fact：全部字段都是认证会话缓存、回源和撤销事实。
  - business/config fact：无。
  - seed fact：无。
  - blob storage：`identity_json` 是 serialized auth identity snapshot，不是 blob。
  - historical residue：未见。
- 证据：session service 对 token 做 SHA-256 hash，保存 identity JSON、JWT exp、created/updated epoch：`session_service.rs:36-50`；resolve 时检查 revoked/expired，再反序列化 `AuthIdentity`：`session_service.rs:57-82`；repository 支持 upsert/get/revoke/delete expired：`auth_session_repository.rs:22-46`、`auth_session_repository.rs:81-110`。
- 建议：保留。看似怪点：本表用 epoch BIGINT 而不是 timestamp，和数据库规范“时间字段使用 timestamp”不一致；但 auth domain 当前就是 `Option<i64>` epoch：`auth_session/entity.rs:2-8`，且 JWT exp 原生 epoch。建议主报告列为“保留但是否改 timestamp 需跨 auth domain 改造评估”，不是可直接改 schema。

#### users

- 表定义：`user_id`、`subject`、`auth_mode`、display/email/admin/provider/avatar/timestamps：`0001_init.sql:1016-1026`；primary key user_id：`0001_init.sql:1612-1613`。
- 字段分类：
  - business/config fact：identity directory / access-control principal directory。
  - runtime/audit fact：最近认证同步投影，介于 directory cache 与 business identity fact 之间。
  - seed fact：无。
  - blob storage：无。
  - historical residue：未见。
- 证据：auth route 从当前 `AuthIdentity` upsert user 并同步 groups：`auth.rs:393-412`；repository upsert/list users 读写这些列：`user_directory_repository.rs:29-50`、`user_directory_repository.rs:92-96`；domain `User` 包含 subject/auth_mode/avatar/is_admin/provider：`identity/entity.rs:6-14`。
- 建议：保留。需要整理：加 `UNIQUE(subject, auth_mode, provider)` 或至少 `subject/auth_mode/provider` 索引，避免同一外部主体漂移成多个 `user_id`；加 `auth_mode` CHECK。此建议需要确认插件 auth identity 的 user_id 生成策略。

#### groups

- 表定义：`group_id`、`display_name`、timestamps，primary key group_id：`0001_init.sql:295-299`、`0001_init.sql:1252-1253`。
- 字段分类：
  - business/config fact：identity directory group。
  - runtime/audit fact：认证 provider 同步投影。
  - seed/blob/historical residue：无。
- 证据：auth route 从 identity.groups upsert groups：`auth.rs:402-412`；repository list/upsert groups：`user_directory_repository.rs:107-111`、`user_directory_repository.rs:148-159`。
- 建议：保留。可加非空/trim CHECK 到 `group_id`，display_name 可为空合理。

#### group_memberships

- 表定义：`user_id`、`group_id`、timestamps，primary key `(user_id, group_id)`：`0001_init.sql:283-287`、`0001_init.sql:1244-1245`。
- 字段分类：
  - business/config fact：identity directory membership。
  - runtime/audit/projection fact：认证 provider 当前 groups 的同步投影。
  - seed/blob/historical residue：无。
- 证据：`replace_groups_for_user` 先 upsert groups，再删除用户旧 membership，最后批量插入当前 membership：`user_directory_repository.rs:140-178`。
- 建议：保留。需要整理：应补 FK 到 `users(user_id)` 和 `groups(group_id)`，或在主报告说明为什么目录投影不使用 DB FK。

#### llm_providers

- 表定义：provider catalog + global credential，`models TEXT DEFAULT '[]'`、`blocked_models TEXT DEFAULT '[]'`、`global_api_key_ciphertext TEXT DEFAULT ''`：`0001_init.sql:459-477`；slug unique：`0001_init.sql:1348-1357`。
- 字段分类：
  - seed/config fact：`id`、`name`、`slug`、`protocol`、`base_url`、`wire_api`、`default_model`、`models`、`blocked_models`、`env_api_key`、`discovery_url`、`sort_order`、`enabled`、`credential_mode`。
  - secret/config fact：`global_api_key_ciphertext`。
  - runtime/audit fact：无。
  - blob storage：无。
  - historical residue：无整列直接残留；但 `models`/`blocked_models` TEXT 与领域/API JSON 值不一致。
- 证据：domain `LlmProvider` 把 `models` 和 `blocked_models` 表达为 `serde_json::Value`，`global_api_key_ciphertext` 为密文字段：`llm_provider/entity.rs:160-192`；repository 将 models/blocked_models 序列化为 TEXT 再解析：`llm_provider_repository.rs:45-60`、`llm_provider_repository.rs:98-115`；resolver 按 credential_mode 在 global DB key、env key、user key、none 之间解析：`llm_provider/resolver.rs:19-90`。
- 建议：保留但高优先级整理。将 `models`、`blocked_models` 改为 JSONB，并补 `protocol`、`credential_mode` CHECK；`global_api_key_ciphertext DEFAULT ''` 可改为 nullable，减少空字符串 sentinel。全局 key 与 user BYOK 没有混杂在同一 ownership：spec 与 resolver 均清楚分层；但 openai_codex token JSON 存在同名 `api_key_ciphertext/global_api_key_ciphertext` 字段中，建议重命名为 `credential_ciphertext` 以反映“不总是 API key”。

#### llm_provider_user_credentials

- 表定义：user BYOK/个人 Codex credential，`provider_id + user_id` unique，验证状态/message/verified_at：`0001_init.sql:442-451`、`0001_init.sql:1332-1341`、`0001_init.sql:1883-1890`。
- 字段分类：
  - secret/config fact：`api_key_ciphertext`。
  - runtime/audit fact：`verification_status`、`verification_message`、`verified_at` 是最近一次验证状态/审计摘要。
  - business/config fact：`provider_id`、`user_id` ownership。
  - seed/blob/historical residue：无。
- 证据：spec 要求用户 BYOK 按 `provider_id + user_id` 唯一隔离并保存验证状态：`.trellis/spec/backend/capability/llm-model-config.md:74`；repository upsert credential 时以 `(provider_id,user_id)` conflict 更新 secret 与验证状态：`llm_provider_repository.rs:318-333`；API 响应只返回 masked preview 与 verification 状态：`llm_providers.rs:857-894`。
- 建议：保留。整理同上：`api_key_ciphertext` 建议命名为 `credential_ciphertext`；`verification_status` 加 CHECK；可补 FK 到 `llm_providers(id)`，并根据 identity strategy 决定是否 FK 到 `users(user_id)`。

#### permission_grants

- 表定义：`run_id`、`source_runtime_session_id`、source turn/tool provenance、`requested_paths JSONB`、reason、grant_scope、expires_at、scope escalation intent JSONB、status、policy decision JSONB、approved_by、effect_frame_id：`0001_init.sql:510-526`；active frame/run/status indexes：`0001_init.sql:1960-1974`。
- 字段分类：
  - runtime/audit fact：全部核心字段。它记录 agent capability grant request、policy decision、approval actor、state transition 和 effect target。
  - business/config fact：无。
  - seed fact：无。
  - blob storage：无；JSONB 字段是 typed runtime/audit payload。
  - historical residue：旧 spec 里曾有 `session_id TEXT`/TEXT JSON，但当前 schema 已改为 `source_runtime_session_id` + `effect_frame_id` + JSONB，符合新的 frame/run anchor。不是历史残留。
- 证据：domain `PermissionGrant` 明确包含 `run_id`、`effect_frame_id`、`source_runtime_session_id`、`requested_paths`、状态和 policy fields：`permission/entity.rs:17-44`；repository create/update/list 使用 JSONB Value 和 timestamp：`permission_grant_repository.rs:29-64`、`permission_grant_repository.rs:121-176`；service 测试/注释说明 `effect_frame_id` 是主要查询锚，`source_runtime_session_id` 是 audit-only provenance：`permission/service.rs:714-716`。
- 建议：保留，并在正式报告中归为 runtime/audit 表。需要整理：给 `status`、`grant_scope` 加 CHECK；`source_runtime_session_id` 的 constraint 名仍叫 `permission_grants_session_id_not_null`，属于 dump/历史命名残留，可重命名；`find_active_escalation_grant` 当前对 JSONB 使用 `LIKE` 查询的代码形态值得核查，因为 JSONB 不应按 TEXT LIKE 查询：`permission_grant_repository.rs:155-162`。

### Action classification

#### 可直接移除

- 本分区未发现可直接移除的整表或字段。理由：目标表均有 readiness/repository/API/service 事实链，且 specs 给出当前语义归属。

#### 需要代码改造后移除

- `skill_assets.remote_source_url`、`remote_imported_at`、`remote_digest`：只有当产品决定移除 github/clawhub/skills_sh 直接导入、统一走 Shared Library `InstalledAssetSource` 后，才可移除。当前 repository 对这些 source 分支强制要求 remote 三列，不能直接删：`skill_asset_repository.rs:320-371`。
- `extension_package_artifacts.source_version`：可能与 `asset_version` 重叠，因为构造时写入 `manifest.asset_version`；需要先确认是否存在未来 source revision 与 package asset version 分离的语义，再决定合并或重命名：`extension_package.rs:195-200`。
- `llm_providers.env_api_key`：不是当前删除候选。只有若项目决定完全取消 env global key，统一 DB-backed credential 后才可删除；当前 resolver 仍明确支持 global env source：`llm_provider/resolver.rs:49-55`。

#### 位置不对 / 应迁移

- `permission_grants` 不应出现在 config 归类里，应迁入 runtime/audit/outbox 分组。
- `auth_sessions` 不应出现在 auth directory/config 分组里，应归入 runtime auth cache / session audit 分组。
- `inline_fs_files` 如未来只服务 skill assets，则位置应从通用 inline FS blob 表收敛为 skill asset file storage；如确实服务多个 owner，则 schema 需要 owner_kind contract。

#### 应保留但重命名/default/约束需整理

- `llm_providers.models`、`blocked_models`：建议 TEXT -> JSONB，默认保持 `[]`；字段语义与 API/domain JSON value 对齐。
- `llm_providers.global_api_key_ciphertext`、`llm_provider_user_credentials.api_key_ciphertext`：建议改名为 `global_credential_ciphertext` / `credential_ciphertext`，因为 Codex OAuth token JSON 不是 API key。
- `llm_providers.global_api_key_ciphertext DEFAULT ''` 和 credential verification message `DEFAULT ''`：可评估 nullable 代替空字符串 sentinel。
- `settings.scope_kind`、`settings.scope_id`：加 scope_kind CHECK 和 scope_id consistency CHECK。
- `permission_grants.status`、`permission_grants.grant_scope`：加 enum CHECK；constraint `permission_grants_session_id_not_null` 重命名为 source_runtime_session_id 语义。
- `inline_fs_files.owner_kind`：加 owner_kind CHECK。
- `project_extension_installations`：给 installed source 组、package artifact 组加成组 nullability CHECK；考虑把 `artifact_storage_ref/archive_digest/manifest_digest` 统一前缀为 `package_artifact_*`。
- `users.auth_mode`：加 CHECK；考虑 user subject/provider 唯一约束。
- `group_memberships`：考虑 FK 或记录为何 directory projection 不设 FK。

#### 看似怪但应保留

- `project_extension_installations.package_*` 与 `artifact_*` 快照列：虽然能从 `extension_package_artifacts` join，但它们是 installation 的 `ExtensionPackageArtifactRef` 快照，服务运行时下载和发布校验，应保留。
- `extension_package_artifacts.manifest JSONB` 与 `project_extension_installations.manifest JSONB` 同时存在：artifact 保存 package 内 manifest，installation 保存安装时 manifest；二者用于不同生命周期，且 publish 校验会对比 source artifact 与 installation manifest。
- `auth_sessions.identity_json TEXT`：是认证 provider 不可用时的身份回源缓存；不是业务 user profile 替代物。
- `permission_grants.source_runtime_session_id`：不是查询主锚，主要是 audit provenance；当前主要查询锚是 effect frame/run。
- `llm_providers.global_api_key_ciphertext` 与 `llm_provider_user_credentials` 分表：不是 ownership 混杂；global admin key 与 user BYOK/Codex token 已由 credential_mode/resolver/API target 分清。

### JSONB / TEXT consistency notes

- 一致：`library_assets.payload JSONB`、`extension_package_artifacts.manifest JSONB`、`project_extension_installations.config/manifest JSONB`、`permission_grants.requested_paths/scope_escalation_intent/policy_decision JSONB` 都承载需要 typed validation 或 potential structured access 的 JSON。
- 可接受但需说明：`settings.value TEXT`、`mcp_presets.transport TEXT` 是 opaque config/value object，符合现有 database guideline “复杂值对象以 JSON 文本存入 TEXT”。
- 不一致且建议优先修：`llm_providers.models TEXT`、`blocked_models TEXT` 在 domain/API 是 `serde_json::Value`，且 provider effective model/profile 逻辑会解析它们；建议 JSONB。
- 可疑代码点：`permission_grant_repository.rs:155-162` 对 `scope_escalation_intent` JSONB 使用 `LIKE` 查询形态，需在后续实现切片检查实际编译/运行；语义上应使用 JSONB containment/path extraction。

### External references

- 无外部文档引用；本研究完全基于当前项目源码、migration 和 Trellis spec。

### Related specs

- `.trellis/spec/backend/database-guidelines.md`
- `.trellis/spec/backend/repository-pattern.md`
- `.trellis/spec/cross-layer/shared-library-contract.md`
- `.trellis/spec/backend/shared-library.md`
- `.trellis/spec/backend/capability/llm-model-config.md`
- `.trellis/spec/backend/capability/architecture.md`
- `.trellis/spec/backend/permission/architecture.md`
- `.trellis/spec/backend/permission/grant-lifecycle.md`
- `.trellis/spec/backend/vfs/vfs-materialization.md`
- `.trellis/spec/backend/vfs/vfs-access.md`

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` 返回 `Current task: (none)`；本研究依据用户显式给出的任务路径和唯一可写 research 文件继续执行。
- 未运行 cargo/sqlx 编译或数据库集成测试；本分区是只读语义审计。
- 未审计前端 UI 的全部字段展示细节，仅读取了 contracts/API route 以确认跨层暴露形态。
- 未验证当前 PostgreSQL 是否接受 `permission_grants.scope_escalation_intent LIKE $2` 这类 JSONB 查询；该点已作为 caveat/后续高优先级核查项记录。
- 未发现本分区可直接删表；若主报告需要“立即删除”清单，本分区应填“无”。
