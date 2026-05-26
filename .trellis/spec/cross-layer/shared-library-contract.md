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

`source_status` 只表达用户可操作状态。builtin / plugin_embedded asset 的 payload digest 与 version 维护错误由后端 seed/startup fail-fast 拦截，不扩展为前端 enum。

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
    "mcp_slots": [{ "key": "string", "description": "string?", "required": "bool" }]
  }
}
```

### `mcp_server_template`

```jsonc
{
  "transport": "McpTransportConfig",
  "route_policy": "auto | relay | direct?",
  "parameter_schema": "JSONSchema?",
  "capabilities": ["string"]
}
```

### `workflow_template`

```jsonc
{
  "schema_version": "string?",
  "template": {
    "key": "string",
    "name": "string",
    "description": "string",
    "binding_kinds": ["WorkflowBindingKind"],
    "workflows": [{ "key": "...", "name": "...", "description": "...", "contract": "WorkflowContract" }],
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
  "workspace_tabs": [{ "type_id": "string", "label": "string", "uri_scheme": "string", "renderer": { "kind": "webview", "entry": "string" } }],
  "permissions": [{ "kind": "local_profile | workspace | runtime_action", "access": "read | write | read_write?", "action_key": "string?" }],
  "bundles": [{ "kind": "extension_host", "entry": "string", "digest": "sha256:<hex>" }]
}
```

## Install Summary

前端 Marketplace 卡片展示同一 `LibraryAsset` 在项目内的安装状态时，必须把 `source-status` 返回的数组按 `installed_source.library_asset_id` flatten + group。

Status priority:

```text
source_missing > update_available > up_to_date
```

同一资产可能被装到多个 kind；每个 kind+key 都记录一个 installation 子项。

## Install Semantics

Marketplace install:

1. 用户选择 `LibraryAsset`。
2. 前端调用 install API。
3. 后端按 `asset_type` 创建对应 Project 资源。
4. Project 资源记录 `InstalledAssetSource`。
5. Project 运行时只读取 Project 资源。

Update:

- 不静默同步。
- Project Assets 展示来源状态。
- 用户手动重装/覆盖。

ExtensionTemplate 安装后，`LibraryAsset` 本身不直接影响会话；只有安装成 Project extension installation 后才可能被 session construction 读取。

正式 packaged extension 安装以平台 artifact 为事实源。`ExtensionTemplate` 可以作为 marketplace/template payload，`.agentdash-extension.tgz` 上传后由后端校验 archive digest、manifest digest、package metadata 和 bundle digest，并保存为 Project scoped package artifact。Project extension installation 可以记录 `package_artifact`，此时 `installed_source` 为空；Shared Library source-status 只表达由 `InstalledAssetSource` 追踪的 marketplace/template 来源。

## Publish Semantics

Project Assets 发布行为：

1. 用户从 Project 资源入口触发发布。
2. 前端调用 `POST /api/projects/{project_id}/shared-library/publish`，只传资源身份、资产元数据、版本和覆盖策略。
3. 后端校验 Project edit 权限，并按 `asset_kind` 读取 Project 资源权威数据。
4. 后端生成类型化 `LibraryAsset.payload`。
5. Marketplace 通过 list/install/source-status 流程处理该资产。

发布请求禁止前端传 raw payload。Project 资源中带 credential、header、env、本机路径、localhost 或私网 URL 的 MCP 连接材料必须在后端 mapper 阶段拒绝。

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
