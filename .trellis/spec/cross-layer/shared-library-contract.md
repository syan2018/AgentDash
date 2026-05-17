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

## Scenario: Marketplace 安装来源与旧入口清理

### 1. Scope / Trigger

- Trigger: Shared Library 安装影响后端 DB、API DTO、前端 Assets UI 与 Marketplace 状态查询。
- 目标：公共配置只从 Shared Library/Marketplace 进入项目；项目资源只保存可编辑副本与 `InstalledAssetSource`。

### 2. Signatures

- `POST /api/shared-library/assets/seed-builtin`
- `POST /api/projects/{project_id}/shared-library/install`
- `GET /api/projects/{project_id}/shared-library/source-status`
- `POST /api/projects/{project_id}/agent-links`
- `PUT /api/projects/{project_id}/agent-links/{agent_id}`
- `DELETE /api/lifecycle-definitions/{id}`
- DB: `project_agent_links.installed_library_asset_id/source_ref/source_version/source_digest/installed_at`

### 3. Contracts

- Project Agent 创建请求必须直接包含项目私有配置：
  - `name`
  - `agent_type`
  - `base_config?`
  - `config_override?`
  - `default_lifecycle_key?`
  - `default_workflow_key?`
  - `is_default_for_story?`
  - `is_default_for_task?`
- `source-status` 必须返回：
  - `project_agents`
  - `mcp_presets`
  - `skill_assets`
  - `workflow_definitions`
  - `lifecycle_definitions`
- 每个 status item 必须包含 `installed_source` 与 `source_status`。
- 前端项目资源卡片展示来源时，若资源存在 `installed_source`，必须优先显示 Marketplace/Shared Library 来源，而不是只显示 `user`。

### 4. Validation & Error Matrix

- 创建 Project Agent 时 `name` 为空 -> `400 BadRequest`
- 创建 Project Agent 时 `agent_type` 为空 -> `400 BadRequest`
- 同项目 Project Agent key 重复 -> `409 Conflict`
- 删除 Marketplace 安装的 Lifecycle 时，同安装包 workflow 仍被其它 Lifecycle step 引用 -> `400 BadRequest`
- 来源 `LibraryAsset` 缺失、不可见或 deprecated -> `source_status = source_missing`
- 来源版本或 digest 不一致 -> `source_status = update_available`

### 5. Good/Base/Bad Cases

- Good: 从 Marketplace 安装 AgentTemplate，Project Agent link 写入 `InstalledAssetSource`，Marketplace 状态页显示 `project_agents` 且为 `up_to_date`。
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
