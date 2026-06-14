# Shared Library Contract

Shared Library / Marketplace / Project Asset 是 AgentDash 公共配置资产的三层模型。跨层权威契约以本文档为准；后端 seed、validator 和事务细节见 [Backend Shared Library](../backend/shared-library.md)。

## Terms

| 后端/API | 前端 UI | 含义 |
| --- | --- | --- |
| `SharedLibrary` | 公共资源库 | 公共资产存储、权限、版本和安装 API |
| `Marketplace` | 资源市场 | 浏览、发现、导入、安装界面 |
| `LibraryAsset` | 资源库资产 | Shared Library 中的统一资产 |
| `ProjectAsset` | 项目资源 | 安装到 Project 后可运行、可编辑的副本 |
| `InstalledAssetSource` | 安装来源 | Project 资源的来源版本元数据 |
| `ExtensionPackageArtifact` | Extension 运行包工件 | `.agentdash-extension.tgz` 的可校验运行产物，可由 Project 或 LibraryAsset 拥有 |

## Naming

Shared Library 中的共享配置统一使用 `*Template` 后缀：

- `AgentTemplate`
- `McpServerTemplate`
- `WorkflowTemplate`
- `SkillTemplate`
- `VfsMountTemplate`
- `ExtensionTemplate`

Project 内运行资源使用 Project 前缀或既有项目资源名：

- `ProjectAgent`
- `ProjectMcpPreset`
- `Project Workflow/Lifecycle`
- `Project SkillAsset`

带 credential/env/local path 的连接材料使用 `Connection`，例如 `McpConnection`。

## JSON Fields

所有 HTTP DTO 使用 `snake_case`。

`LibraryAsset` 基础字段：

- `id`
- `asset_type`
- `scope`
- `owner_id`
- `key`
- `display_name`
- `description`
- `version`
- `source`
- `source_ref`
- `payload_digest`
- `deprecated`
- `payload`
- `extension_package_artifact`：仅 `extension_template` 在存在匹配 LibraryAsset-owned package artifact 时返回摘要
- `created_at`
- `updated_at`

`InstalledAssetSource` 基础字段：

- `library_asset_id`
- `source_ref`
- `source_version`
- `source_digest`
- `installed_at`

来源状态字段：

- `source_status`: `up_to_date` / `update_available` / `source_missing`
- `current_source_version`
- `current_source_digest`

`source_status` 只表达用户可操作状态。builtin / integration_embedded asset 的 payload digest 与 version 维护错误由后端 seed/startup fail-fast 拦截，不扩展为前端 enum。

## Payload Contract

- API 可以返回 `payload`，但保存/安装前后端都必须按 `asset_type` 做类型化校验。
- 前端 mapper 不做旧字段兼容兜底；后端 DTO 是权威契约。
- 运行页面优先展示安装后的 Project 资源，不直接把 `LibraryAsset.payload` 当运行配置编辑。
- 前端 `LibraryAssetDto.payload` 类型为 `unknown`：纯展示时防御式解析，未知形状降级为原始 JSON；安装时原样转发后端。

权威源：`crates/agentdash-domain/src/shared_library/value_objects.rs::LibraryAssetPayload`。
DTO 层 `crates/agentdash-api/src/dto/shared_library.rs` 直接透传 `serde_json::Value`，不重新结构化。

## Payload Schemas

### `agent_template`

```jsonc
{
  "config": {
    "executor": "string?",
    "provider_id": "string?",
    "model_id": "string?",
    "agent_id": "string?",
    "thinking_level": "ThinkingLevel?",
    "permission_policy": "string?",
    "system_prompt": "string?",
    "system_prompt_mode": "SystemPromptMode?",
    "capability_directives": ["ToolCapabilityDirective"],
    "mcp_slots": [{ "key": "string", "description": "string?", "required": "bool" }],
    "mcp_dependencies": [{
      "slot_key": "string",
      "asset_key": "string?",
      "asset_id": "string?",
      "preset_key": "string?",
      "parameters": "object?",
      "required": "bool",
      "overwrite": "bool"
    }]
  }
}
```

Agent 模板的 MCP dependency 是安装期依赖计划。安装 AgentTemplate 时，后端按 dependency 定位 `mcp_server_template`，解析安装参数，生成 Project MCP Preset，并把最终 preset key 写入 ProjectAgent 的 `mcp_preset_keys`。因此 AgentTemplate 只描述 Project 资源装配关系，运行时 MCP surface 仍从 Project MCP Preset 解析为 `RuntimeMcpServer`。

### `mcp_server_template`

```jsonc
{
  "transport_template": {
    "type": "http | sse",
    "url_template": "https://mcp.example.com/${workspace}/mcp"
  },
  "route_policy": "auto | relay | direct?",
  "parameter_schema": "JSONSchema?",
  "capabilities": ["string"]
}
```

`mcp_server_template` 是公共模板，不是 Project 运行连接。模板只支持 HTTP/SSE URL template 与 `${parameter_key}` 占位符；`parameter_schema` 声明安装参数。安装时前端通过 `InstallLibraryAssetRequest.install_options = { asset_type: "mcp_server_template", parameters: {...} }` 提交参数，后端解析成 Project `McpTransportConfig` 并写入 Project MCP Preset 与 `InstalledAssetSource`。公共 payload 不保存 header/env/credential 值、本机路径、localhost 或私网 URL，原因是这些连接材料只属于用户安装上下文。

### `workflow_template`

```jsonc
{
  "schema_version": "string?",
  "template": {
    "key": "string",
    "name": "string",
    "description": "string",
    "binding_kinds": ["WorkflowBindingKind"],
    "workflows": [{ "key": "...", "name": "...", "description": "...", "contract": "AgentProcedureContract" }],
    "lifecycle": {
      "key": "string",
      "name": "string",
      "description": "string",
      "entry_activity_key": "string",
      "activities": ["ActivityDefinition"],
      "transitions": ["ActivityTransition"]
    }
  }
}
```

### `skill_template`

```jsonc
{
  "files": [{ "path": "string", "content": "string", "kind": "SkillAssetFileKind" }],
  "disable_model_invocation": "bool"
}
```

### `vfs_mount_template`

Tagged enum：`kind` 决定子类型；inline 子类型携带文件，external_service 子类型携带 service_id + root_ref。Mount-level 元数据两类共享。

```jsonc
{
  "kind": "inline | external_service",
  "mount_id": "string",
  "display_name": "string",
  "description": "string?",
  "capabilities": ["read" | "write" | "list" | "search"]
}
```

Inline payload 额外携带 `files[]`；external service payload 额外携带 `service_id` 与 `root_ref`。

### `extension_template`

```jsonc
{
  "manifest_version": "string",
  "extension_id": "string",
  "package": { "name": "string", "version": "string" },
  "asset_version": "string",
  "commands": [{ "name": "string", "description": "string", "handler": { "kind": "inject_message", "content": "string" } }],
  "flags": [{ "name": "string", "type": "bool | string", "default": "matching value", "description": "string" }],
  "message_renderers": [{ "custom_type": "string", "renderer": { "kind": "json_card | markdown" } }],
  "capability_directives": ["ToolCapabilityDirective"],
  "asset_refs": [{ "asset_type": "string", "key": "string", "required": "bool" }],
  "runtime_actions": [{ "action_key": "string", "kind": "session_runtime | setup", "description": "string", "input_schema": "JSONSchema", "output_schema": "JSONSchema", "permissions": ["string"] }],
  "protocol_channels": [{
    "channel_key": "string",
    "version": "semver",
    "description": "string",
    "methods": [{ "name": "string", "description": "string", "input_schema": "JSONSchema", "output_schema": "JSONSchema", "permissions": ["string"] }]
  }],
  "extension_dependencies": [{ "alias": "string", "extension_id": "string", "version": "semver range", "channels": ["string"] }],
  "workspace_tabs": [{ "type_id": "string", "label": "string", "uri_scheme": "string", "renderer": { "kind": "webview | canvas_panel", "entry": "string" } }],
  "permissions": [{
    "kind": "local_profile | http | workspace | env | process | runtime_action | extension_channel",
    "access": "read | write | read_write | execute?",
    "hosts": ["string?"],
    "names": ["string?"],
    "action_key": "string?",
    "channel_key": "string?",
    "methods": ["string?"]
  }],
  "bundles": [{ "kind": "extension_host", "entry": "string", "digest": "sha256:<hex>" }]
}
```

Extension package 中 `protocol_channels` 表达 provider 插件导出的 Project/session scoped API surface，`extension_dependencies` 表达 consumer 插件按 alias 依赖的 provider extension/channel。Projection、Gateway admission、local runner trace 都以 canonical `extension_key.channel` / method 作为事实；SDK/bridge 可以提供 self shortcut、dependency alias 或 Canvas binding alias，但不改变 manifest 中 provider/channel/dependency 的权威关系。

Extension `permissions` 的职责是安装摘要、依赖解析、可用性诊断和审计。运行时真正需要裁决的本机 Host API 使用 action-level 或 channel-method-level `permissions` string，例如 `local.profile.read`、`workspace.vfs.read`、`process.execute`、`runtime.invoke:<action_key>`、`extension.channel.invoke:<channel_key>.<method>`。

ExtensionTemplate 的 package requirement 由后端统一计算：`runtime_actions`、`protocol_channels`、`workspace_tabs`、`bundles` 任一非空时需要 package artifact；仅声明 `commands`、`flags`、`message_renderers`、`capability_directives` 或 `asset_refs` 时可作为 declaration-only template 安装。

## Install Summary

前端 Marketplace 卡片展示同一 `LibraryAsset` 在项目内的安装状态时，必须把 `source-status` 返回的数组按 `installed_source.library_asset_id` flatten + group。

Status priority:

```text
source_missing > update_available > up_to_date
```

同一资产可能被装到多个 kind；每个 kind+key 都记录一个 installation 子项。

## Install Semantics

External Marketplace source:

1. 前端通过 `GET /api/marketplace/sources` 读取可展示来源与其支持的资产类型。
2. 前端通过 `GET /api/marketplace/external-assets` 按 `source_key`、`asset_type`、`query`、`cursor`、`limit` 分页读取外部候选；跨来源列表只表达发现结果，cursor 分页绑定单一 source。
3. 前端通过 `GET /api/marketplace/external-assets/{source_key}/{external_id}` 读取外部详情，详情仍属于 provider 视角的候选资产。
4. 前端通过 `POST /api/marketplace/external-assets/import` 提交来源身份、外部资产身份和资产类型；后端从 provider 拉取 payload，生成 `LibraryAsset(source = remote_imported)`。
5. 外部来源 `source_ref` 使用 `market:{source_key}:{asset_type}:{external_id}`；`payload_digest` 使用平台 canonical JSON 规则计算，远端 `digest` 只作为远端版本提示。

External Marketplace refresh:

- `POST /api/marketplace/external-assets/refresh` 返回 `remote_version`、`remote_digest`、`local_version`、`local_digest` 和 `status`。
- `status` 使用 `up_to_date` / `update_available` / `source_missing` / `not_imported`。
- refresh 只比较外部 listing 与本地 `remote_imported` LibraryAsset，Project 资源仍通过 Shared Library install / source-status 语义更新。

Skill URL Import 是单项外部来源定位。`POST /api/projects/{project_id}/skill-assets/import` 保持 `{ url }` 入参和 Project `SkillAsset` 响应，但后端写入语义与外部来源一致：先创建或更新 `LibraryAsset(asset_type=skill_template, source=remote_imported)`，再安装到 Project 并写入 `InstalledAssetSource`。因此前端判断远端导入来源时应优先使用 `installed_source`，Project `SkillAsset.source` 不承载 GitHub / ClawHub / skills.sh 的版本事实。

Marketplace install:

1. 用户选择 `LibraryAsset`。
2. 前端调用 install API。
3. 后端按 `asset_type` 创建对应 Project 资源；MCP 模板安装时由 `install_options.mcp_server_template.parameters` 解析最终 HTTP/SSE transport。
4. Project 资源记录 `InstalledAssetSource`。
5. Project 运行时只读取 Project 资源。

Update:

- 不静默同步。
- Project Assets 展示来源状态。
- 用户手动重装/覆盖。

ExtensionTemplate 安装后，`LibraryAsset` 本身不直接影响会话；只有安装成 Project extension installation 后才可能被 frame construction 读取。

Packaged Extension 安装以平台 artifact 为事实源。`ExtensionPackageArtifact` 使用 `owner_kind + owner_id` 表达归属：本地导入保存为 Project-owned artifact，发布到 Marketplace 后保存为 LibraryAsset-owned artifact。Marketplace packaged install 写入 `installed_source + package_artifact`；本地包导入写入 Project-owned `package_artifact` 且 `installed_source = None`。Shared Library source-status 只比较 `InstalledAssetSource` 的版本与 digest，包完整性由 install/publish/runtime download contract 负责。

Canvas 发布为插件时生成同款 packaged extension artifact。其 workspace tab 使用 `canvas_panel` renderer，`entry` 指向包内 Canvas runtime snapshot；安装、覆盖和 source-status 仍按 packaged extension installation 处理。

## Publish Semantics

Project Assets 发布行为：

1. 用户从 Project 资源入口触发发布。
2. 前端调用 `POST /api/projects/{project_id}/shared-library/publish`，只传资源身份、资产元数据、版本和覆盖策略。
3. 后端校验 Project edit 权限，并按 `asset_kind` 读取 Project 资源权威数据。
4. 后端生成类型化 `LibraryAsset.payload`。
5. Marketplace 通过 list/install/source-status 流程处理该资产。

发布请求禁止前端传 raw payload。Project 资源中带 credential、header、env、本机路径、localhost 或私网 URL 的 MCP 连接材料必须在后端 mapper 阶段拒绝。

Extension 发布使用 `asset_kind = extension_installation`，后端从 `ProjectExtensionInstallation` 读取 manifest 并创建 `LibraryAsset(asset_type = extension_template)`。需要 package artifact 的安装必须携带 `package_artifact` 才能发布；发布成功后 LibraryAsset DTO 可携带同一 LibraryAsset owner 下、与 ExtensionTemplate typed manifest/package identity 匹配的 `extension_package_artifact` 摘要，Marketplace 用它判断 packaged template 是否可安装。关联键使用 owner 与 typed package identity，原因是 manifest digest 描述包内 manifest，payload digest 描述 LibraryAsset payload，二者属于不同摘要域。

## Source Status Contract

`GET /api/projects/{project_id}/shared-library/source-status` 必须返回：

- `project_agents`
- `mcp_presets`
- `skill_assets`
- `workflow_definitions`
- `activity_lifecycle_definitions`
- `vfs_mounts`
- `extension_installations`

每个 status item 必须包含 `installed_source` 与 `source_status`。

Project VFS Mount 列表 / 详情 DTO 必须透出 `installed_source: Option<InstalledAssetSource>`，与 Skill / MCP / Workflow 同款序列化形态。

## Settings Scope

- system scope：系统级 Shared Library 管理权限、LLM Provider、平台运行配置。
- user scope：个人偏好、用户级 Shared Library、个人 connection。
- project scope：不直接编辑公共资产；跳转 Project Assets / Project Agent。
- local-runtime scope：本机 profile、roots、本机 `McpConnection`。

`agent.pi.user_preferences` 属于 user scope，不属于 system scope。
