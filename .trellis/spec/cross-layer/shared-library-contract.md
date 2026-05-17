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

## Settings 分工

- system scope：系统级 Shared Library 管理权限、LLM Provider、平台运行配置。
- user scope：个人偏好、用户级 Shared Library、个人 connection。
- project scope：不直接编辑公共资产；跳转 Project Assets / Project Agent。
- local-runtime scope：本机 profile、roots、本机 `McpConnection`。

`agent.pi.user_preferences` 属于 user scope，不属于 system scope。
