# Shared Library 公共资产规范

> Shared Library 是 AgentDash 公共配置资产的统一存储、权限、版本和安装入口。

## 核心命名

| 名称 | 含义 |
| --- | --- |
| `SharedLibrary` | 后端领域/API 层公共资源库 |
| `Marketplace` | 前端浏览、发现、安装 UI |
| `LibraryAsset` | Shared Library 中的统一资产记录 |
| `BuiltinSeed` | 代码内嵌的资产种子，只负责幂等 upsert 到 Shared Library |
| `InstalledAssetSource` | Project 资源记录其来源资产、版本和 digest 的元数据 |

命名规则：

- Shared Library 中所有可分享、可安装的公共配置统一使用 `*Template` 后缀。
- `Preset` 只表示 Project 内可运行/可引用配置，不表示跨 Project 公共资产。
- `Connection` 只表示带 credential/env/local path/base URL 等连接材料的数据。

## LibraryAsset

`LibraryAsset` 使用单表 JSONB payload：

- `asset_type`: `agent_template` / `mcp_server_template` / `workflow_template` / `skill_template` / `extension_template`
- `scope`: `builtin` / `system` / `org` / `user`
- `owner_id`: scope owner，可为空
- `key`
- `display_name`
- `description`
- `version`
- `source`: `builtin` / `user_authored` / `remote_imported` / `plugin_embedded`
- `source_ref`: builtin key、remote URL、digest 等来源引用
- `payload_digest`
- `deprecated`
- `payload`

约束：

- `asset_type + scope + owner_id + key` 必须唯一。
- `payload` 只能在 Shared Library 边界保持 JSONB 灵活性。
- 每个 `asset_type` 必须有类型化 mapper / validator。
- 运行路径不得直接消费未校验的 `payload`，必须先安装成 Project 资源或转换成类型化领域对象。

## BuiltinSeedRegistry

Builtin 资产必须通过统一 seed registry 物化到 Shared Library，不再在各资源模块中单独实现 bootstrap。

Registry 负责：

- 收集 `AgentTemplate` / `McpServerTemplate` / `WorkflowTemplate` / `SkillTemplate` 内置定义。
- 为每个 seed 提供稳定 `builtin_key`、`version`、`payload_digest`。
- 幂等 upsert 到 `LibraryAsset`。
- 对 registry 中删除的 builtin 默认标记 `deprecated`，避免已安装 Project 资源来源断链。

Project 资源不会因 builtin seed 更新而静默变化。

## InstalledAssetSource

安装到 Project 后的资源必须记录来源：

- `library_asset_id`
- `source_ref`
- `source_version`
- `source_digest`
- `installed_at`

版本状态：

- `up_to_date`: 来源版本/digest 一致。
- `update_available`: Shared Library 中来源版本或 digest 已更新。
- `source_missing`: 来源资产不可见、被删除或 deprecated。

更新策略：

- 不做静默同步。
- 第一阶段只支持版本提示 + 用户手动重装/覆盖。
- 字段级 diff 与三方合并是后续增强。

## 资产类型边界

### AgentTemplate

可复用：

- 角色 key / display name / description
- base system prompt / persona
- 默认 executor / provider / model / thinking / permission
- 抽象能力需求
- 抽象 MCP slots

不可直接持有：

- Project MCP preset key
- Project SkillAsset key
- Project VFS/root/container
- knowledge/memory
- default lifecycle/workflow
- companion whitelist
- story/task 默认标记

### McpServerTemplate

描述公共 MCP server 类型、参数 schema、默认 transport 模板和能力说明。

真实 token、env、local command、base URL 等连接材料属于 `McpConnection`，不得进入 template。

### WorkflowTemplate / SkillTemplate

承接原 builtin workflow template 与 builtin skill seed。Project 中的 Workflow/SkillAsset 是安装后的 Project 资源副本。

### ExtensionTemplate

描述用户可动态安装的 runtime extension manifest。第一版支持声明式 slash command、runtime flag、schema-driven message renderer、capability directives 与资产引用占位。

约束：

- `extension_id` 与 `manifest_version` 必须非空。
- command name 不带 `/`，handler 第一版只允许安全的声明式 `inject_message`。
- flag type 只支持 `bool` / `string`，default 必须与 type 匹配。
- message renderer 只允许平台内置 schema-driven renderer，不允许动态 React/TypeScript bundle。

## Project 资源安装语义

- 从 Marketplace 安装默认创建可编辑 Project 副本。
- Project 资源保留 `InstalledAssetSource`，用于审计、重装和版本提示。
- Project 运行时只读取 Project 资源，不直接依赖 Shared Library。
- `ProjectAgentLink` 也属于 Project 资源，Marketplace 安装 `AgentTemplate` 时必须在 link 上写入 `InstalledAssetSource`。
- 删除 Marketplace 安装的 Workflow/Lifecycle bundle 时，删除 Lifecycle 后必须清理同一 `library_asset_id` 来源的 workflow definitions；若这些 workflow 被其它 lifecycle 引用，则拒绝删除并返回明确错误。

## Project 资源发布语义

- 用户发布入口从 Project 资源出发：`POST /api/projects/{project_id}/shared-library/publish`。
- 发布请求只提交资源类型、Project 资源 id、资产元数据和覆盖策略；前端不得直接拼装 `LibraryAsset.payload`。
- 后端必须重新读取 Project 资源权威状态，并通过类型化 mapper 生成对应 `*Template` payload。
- 第一阶段发布只支持 `scope = user` 与 `source = user_authored`，`owner_id` 使用当前用户身份。
- 发布身份沿用 `asset_type + scope + owner_id + key`。同身份存在且 `overwrite=false` 时返回冲突；覆盖发布必须保留原 `LibraryAsset.id` 与 `created_at`，更新 payload、version、digest 与 metadata。
- `payload_digest` 使用 builtin seed 相同的 JSON sha256 规则，避免 source-status 对同一 payload 产生两套版本判断。
- MCP Preset 发布必须拒绝 credential、header、env、本机路径、localhost/private network URL 等连接材料；无法证明可共享的连接配置应明确报错。
- Workflow 发布必须以 lifecycle bundle 为单位，校验 lifecycle 引用的 workflow definitions 都存在后再发布。

## Plugin Embedded 资产语义

- Native plugin 可在启动期通过 `AgentDashPlugin::library_asset_seeds()` 声明内嵌 Shared Library assets。
- 宿主负责补齐 plugin 名称、计算 digest、校验 payload，并以 `scope=system`、`source=plugin_embedded` 写入 Shared Library。
- `source_ref` 使用 `plugin:{plugin_name}:{asset_type}:{key}`，用于 source-status 与审计。
- plugin seed 必须走与 builtin/user asset 相同的 `LibraryAssetPayload` typed validator。
- 同一 `asset_type + scope + key` 被不同 plugin 或不同 source 占用时启动期 fail-fast，不做隐式覆盖。
- 同一 plugin 的同一 seed 可幂等更新，保留原 `LibraryAsset.id` 与 `created_at`。

## 迁移原则

- 现有 project-level builtin MCP/Skill/Workflow 视为“已安装副本”，不是公共 builtin 本体。
- 旧资源专属 bootstrap API 在新 Marketplace install 路径可用后退役。
- 现有 `AgentPresetConfig` 必须拆成 `AgentTemplateConfig` 与 `ProjectAgentConfigOverride` 等更窄类型。
- 用户可见路径不提供“关联已有全局 Agent”；跨项目复用只发生在 `AgentTemplate`。
