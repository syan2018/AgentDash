# Shared Library 跨层契约

> 前后端统一使用 Shared Library / Marketplace / Project Asset 三层表达公共配置资产。

## 术语

| 后端/API | 前端 UI | 含义 |
| --- | --- | --- |
| `SharedLibrary` | 公共资源库 | 公共资产存储、权限、版本和安装 API |
| `Marketplace` | 资源市场 | 浏览、发现、导入、安装界面 |
| `LibraryAsset` | 资源库资产 | Shared Library 中的统一资产 |
| `ProjectAsset` | 项目资源 | 安装到 Project 后可运行、可编辑的副本 |
| `InstalledAssetSource` | 安装来源 | Project 资源的来源版本元数据 |

## Template 命名

Shared Library 中的共享配置统一使用 `*Template` 后缀：

- `AgentTemplate`
- `McpServerTemplate`
- `WorkflowTemplate`
- `SkillTemplate`
- `ExtensionTemplate`

Project 内运行资源使用 Project 前缀或既有项目资源名：

- `ProjectAgent`
- `ProjectMcpPreset`
- `Project Workflow/Lifecycle`
- `Project SkillAsset`

带 credential/env/local path 的连接材料使用 `Connection`：

- `McpConnection`

## JSON 字段

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

## Payload 规则

- API 可以返回 `payload`，但保存/安装前后端都必须按 `asset_type` 做类型化校验。
- 前端 mapper 不做旧字段兼容兜底；后端 DTO 是权威契约。
- 运行页面优先展示安装后的 Project 资源，不直接把 `LibraryAsset.payload` 当运行配置编辑。
- 前端 `LibraryAssetDto.payload` 类型为 `unknown`：用于纯展示（卡片、详情抽屉）时必须做防御
  式解析，未知形状降级为原始 JSON，永不 throw；用于安装时不解析，原样转发后端。

## Payload Schema by `asset_type`

权威源：`crates/agentdash-domain/src/shared_library/value_objects.rs::LibraryAssetPayload`。
DTO 层 [crates/agentdash-api/src/dto/shared_library.rs](../../../crates/agentdash-api/src/dto/shared_library.rs) 直接透传 `serde_json::Value`，不重新结构化。

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
  "transport": "McpTransportConfig",     // 沿用 packages/app-web/src/types/mcp-preset.ts
  "route_policy": "auto | relay | direct?",
  "parameter_schema": "JSONSchema?",
  "capabilities": ["string"]
}
```

### `workflow_template`

```jsonc
{
  "schema_version": "string?",
  "template": {                          // BuiltinWorkflowTemplateBundle
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

### `extension_template`

```jsonc
{
  "manifest_version": "string",
  "extension_id": "string",
  "commands": [{
    "name": "string",
    "description": "string",
    "handler": { "kind": "inject_message", "content": "string" }
  }],
  "flags": [{ "name": "string", "type": "bool | string", "default": "matching value", "description": "string" }],
  "message_renderers": [{ "custom_type": "string", "renderer": { "kind": "json_card | markdown" } }],
  "capability_directives": ["ToolCapabilityDirective"],
  "asset_refs": [{ "asset_type": "string", "key": "string", "required": "bool" }]
}
```

## InstallSummary 派生约定（前端）

前端 Marketplace 卡片展示同一 `LibraryAsset` 在项目内的安装状态时，必须把
`source-status` 返回的 5 个数组（`project_agents` / `mcp_presets` / `skill_assets` /
`workflow_definitions` / `activity_lifecycle_definitions` / `extension_installations`）按 `installed_source.library_asset_id`
flatten + group：

- 同一资产可能被装到多个 kind：每个 kind+key 都记一个 `installation` 子项。
- `summary.status` 取所有 installation 的"最坏状态"：
  优先级 `source_missing > update_available > up_to_date`。
- 卡片上的 install 按钮文案以 `summary.status` 为准；hover tooltip 列出全部
  installation（`asset_kind · project_asset_key (vN)`）。

参考实现：
[`packages/app-web/src/features/assets-panel/categories/MarketplaceCategoryPanel.tsx`](../../../packages/app-web/src/features/assets-panel/categories/MarketplaceCategoryPanel.tsx)
中的 `installSummaryByAssetId` + `sourceStatusPriority`。

## 安装语义

Marketplace 安装行为：

1. 用户选择 `LibraryAsset`。
2. 前端调用 install API。
3. 后端按 `asset_type` 创建对应 Project 资源。
4. Project 资源记录 `InstalledAssetSource`。
5. Project 运行时只读取 Project 资源。

更新行为：

- 不静默同步。
- Project Assets 展示来源状态。
- 第一阶段支持手动重装/覆盖。
- 字段级 diff / 三方合并属于后续增强。

ExtensionTemplate 安装后的运行时边界：

- `LibraryAsset` 本身不直接影响会话；只有安装成 Project extension installation 后才可能被 session construction 读取。
- session construction 当前只产出只读 `extension_runtime` metadata projection，包含 command / flag / renderer declarations。
- projection 不执行 command handler、不修改 prompt、不注册前端 `/` 菜单、不写入 Hook/Rhai flag store。
- 真正的 command registry、flag store、renderer registry 接线属于 `Plugin Extension API` 的后续运行时实现。

## 发布语义

Project Assets 发布行为：

1. 用户在 Project Agent / MCP Preset / Workflow Lifecycle / SkillAsset 资源入口触发发布。
2. 前端调用 `POST /api/projects/{project_id}/shared-library/publish`，只传 `asset_kind`、`project_asset_id`、`scope`、`key`、`display_name`、`description`、`version`、`overwrite`。
3. 后端校验 Project edit 权限，并按 `asset_kind` 读取 Project 资源权威数据。
4. 后端生成类型化 `LibraryAsset.payload`，写入 `scope=user`、`source=user_authored`、`owner_id=current_user.user_id`。
5. 发布成功后 Marketplace 通过现有 list/install/source-status 流程处理该资产。

发布请求禁止前端传 raw payload。Project 资源中带 credential、header、env、本机路径、localhost 或私网 URL 的 MCP 连接材料必须在后端 mapper 阶段拒绝。

## Scenario: Marketplace 安装来源与旧入口清理

### 1. Scope / Trigger

- Trigger: Shared Library 安装影响后端 DB、API DTO、前端 Assets UI 与 Marketplace 状态查询。
- 目标：公共配置只从 Shared Library/Marketplace 进入项目；项目资源只保存可编辑副本与 `InstalledAssetSource`。

### 2. Signatures

- `POST /api/shared-library/assets/seed-builtin`
- `POST /api/projects/{project_id}/shared-library/install`
- `POST /api/projects/{project_id}/shared-library/publish`
- `GET /api/projects/{project_id}/shared-library/source-status`
- `GET /api/projects/{project_id}/agents`
- `POST /api/projects/{project_id}/agents`
- `PUT /api/projects/{project_id}/agents/{project_agent_id}`
- `DELETE /api/activity-lifecycle-definitions/{id}`
- DB: `project_agents.installed_library_asset_id/source_ref/source_version/source_digest/installed_at`

### 3. Contracts

- Project Agent 创建请求必须直接包含项目私有配置：
  - `name`
  - `agent_type`
  - `config?`
  - `default_lifecycle_key?`
  - `default_workflow_key?`
  - `is_default_for_story?`
  - `is_default_for_task?`
- `source-status` 必须返回：
  - `project_agents`
  - `mcp_presets`
  - `skill_assets`
  - `workflow_definitions`
  - `activity_lifecycle_definitions`
  - `filespaces`
- 每个 status item 必须包含 `installed_source` 与 `source_status`。
- 前端项目资源卡片展示来源时，若资源存在 `installed_source`，必须优先显示 Marketplace/Shared Library 来源，而不是只显示 `user`。
- `extension_template` 安装后必须返回 `asset_kind = extension_installation`，并在 `source-status.extension_installations` 中出现。
- Project Filespace 列表 / 详情 DTO 必须透出 `installed_source: Option<InstalledAssetSource>`，与 Skill / MCP / Workflow 同款序列化形态；前端 `AssetPickerDrawer` 必须按 `installed_source` 过滤掉 Marketplace 安装来的 Filespace，避免被重复发布。

### 4. Validation & Error Matrix

- 创建 Project Agent 时 `name` 为空 -> `400 BadRequest`
- 创建 Project Agent 时 `agent_type` 为空 -> `400 BadRequest`
- 同项目 Project Agent key 重复 -> `409 Conflict`
- 删除 Marketplace 安装的 Lifecycle 时，同安装包 workflow 仍被其它 Activity Lifecycle 引用 -> `400 BadRequest`
- 来源 `LibraryAsset` 缺失、不可见或 deprecated -> `source_status = source_missing`
- 来源版本或 digest 不一致 -> `source_status = update_available`

### 5. Good/Base/Bad Cases

- Good: 从 Marketplace 安装 AgentTemplate，ProjectAgent 写入 `InstalledAssetSource`，Marketplace 状态页显示 `project_agents` 且为 `up_to_date`。
- Base: 用户手工新建 MCP/Skill/Workflow/Agent，项目资源可编辑但无 `installed_source`，不出现在 source-status 列表。
- Bad: 在 MCP/Skill/Workflow 各自 Assets 页提供独立 builtin bootstrap/reset；这会绕过 Shared Library 版本状态，禁止恢复。

### 6. Tests Required

- Rust:
  - Shared Library payload validation。
  - Shared Library source-status route parse/serialization。
  - Project Agent route serialization。
  - Workflow deletion path至少覆盖同源 workflow definitions 的引用阻断逻辑。
- Frontend:
  - `pnpm --filter app-web typecheck`
  - `pnpm --filter app-web test`
  - Mapper 必须读取 `installed_source`，不能用旧字段兜底。

### 7. Wrong vs Correct

#### Wrong

```text
Project Assets MCP 页 -> 装载内置 Preset -> 生成 source=user/builtin 的项目资源 -> Marketplace 不知道版本状态
```

#### Correct

```text
Shared Library seed -> Marketplace install -> Project resource + InstalledAssetSource -> source-status 计算版本状态
```

## Settings 分工

- system scope：系统级 Shared Library 管理权限、LLM Provider、平台运行配置。
- user scope：个人偏好、用户级 Shared Library、个人 connection。
- project scope：不直接编辑公共资产；跳转 Project Assets / Project Agent。
- local-runtime scope：本机 profile、roots、本机 `McpConnection`。

`agent.pi.user_preferences` 属于 user scope，不属于 system scope。
