# Agent / Shared Library 配置边界设计草案

## Design Goal

用一套统一的公共资产层收束 Agent、MCP、Skill、Workflow 等配置来源，避免每个资源类型各自实现 builtin bootstrap / import / clone。

核心原则：

- `Project Agent` 是唯一可运行 Agent。
- `AgentTemplate` 是可跨 Project 复用的 Agent 模板，不直接持有项目运行资源。
- `Shared Library` 是公共资产存储与 API 层，承接 builtin、system、org、user、remote/imported 资产。
- `Marketplace` 是 Shared Library 的浏览、发现、安装 UI。
- `Project Assets` 是项目内可运行资源，从 Marketplace 安装、引用或克隆而来。

## Naming

统一术语如下：

| Term | 中文 UI 建议 | 含义 |
| --- | --- | --- |
| `SharedLibrary` | 公共资源库 | 公共资产的存储、权限、版本、查询和安装 API |
| `Marketplace` | 市场 / 资源市场 | 面向用户的浏览、发现、导入、安装界面 |
| `LibraryAsset` | 资源库资产 | Shared Library 中的统一资产记录，覆盖 Agent/MCP/Workflow/Skill |
| `BuiltinSeed` | 内置种子 | 代码内嵌的资产种子，只负责幂等 upsert 到 Shared Library |
| `ProjectAsset` | 项目资源 | 安装到 Project 后可运行、可编辑的资源副本 |
| `InstalledAssetSource` | 安装来源 | ProjectAsset 记录其来源 `LibraryAsset`、版本和 digest 的元数据 |
| `AgentTemplate` | Agent 模板 | Shared Library 中可跨 Project 复用的 Agent 角色模板 |
| `McpServerTemplate` | MCP Server 模板 | Shared Library 中的 MCP server 类型、参数 schema 和默认 transport 模板 |
| `WorkflowTemplate` | Workflow 模板 | Shared Library 中的 workflow/lifecycle 模板 |
| `SkillTemplate` | Skill 模板 | Shared Library 中的 skill 文档包模板 |
| `ProjectAgent` | 项目 Agent | 项目内唯一可运行的 Agent 实例 |
| `McpConnection` | MCP 连接 | 实际连接材料，引用 McpServerTemplate，可带用户/组织/本机/项目 scope |
| `ProjectMcpPreset` | 项目 MCP 预设 | Project 内 agent-facing MCP key、授权和 route policy |

命名约束：

- Shared Library 中所有可分享、可安装的共享配置统一使用 `*Template` 后缀。
- 避免用 `Preset` 表示跨 Project 公共资产；`Preset` 只保留给 Project 内可运行/可引用配置。
- 避免把 `Marketplace` 作为后端领域名；后端使用 `SharedLibrary` / `LibraryAsset`。
- 避免用 `Catalog` 表示单个共享资产，避免与整个 Shared Library / Marketplace 混淆。
- `Connection` 专用于带 credential/env/local path 等连接材料的数据，不能进入 `AgentTemplate`。

## Target Concepts

### Library Asset

统一公共资产条目。承载资产元信息、来源、可见范围、版本和资产 payload。

建议字段方向：

- `id`
- `asset_type`: `agent_template` / `mcp_server_template` / `workflow_template` / `skill_template`
- `scope`: `builtin` / `system` / `org` / `user`
- `owner_id`: org/user/system owner，可为空
- `key`
- `display_name`
- `description`
- `version`
- `source`: `builtin` / `user_authored` / `remote_imported`
- `source_ref`: builtin key、remote URL、digest 等
- `payload`: 类型化 JSON payload
- `created_at` / `updated_at`

Builtin 资产也应通过 Shared Library 呈现，而不是散落在各资源 service 的 bootstrap 模块里。

Payload 存储策略：

- LibraryAsset 使用单表 JSONB `payload`。
- `asset_type` 决定 payload schema。
- 应用层为每种 `asset_type` 提供类型化 mapper / validator：
  - `AgentTemplatePayload`
  - `McpServerTemplatePayload`
  - `WorkflowTemplatePayload`
  - `SkillTemplatePayload`
- Project install 只能消费通过 validator 的 payload，禁止把未校验 JSON 直接写入运行资源。
- 运行路径仍消费类型化领域对象，不直接依赖 Marketplace JSONB。

### Builtin Seed Registry

代码内置 registry 只负责声明 builtin 种子，不再承担项目级 bootstrap 业务入口。

职责：

- 收集内置资产定义：
  - AgentTemplate builtin
  - McpServerTemplate builtin
  - Workflow Template builtin
  - Skill Template builtin
- 为每个 builtin 资产提供稳定 `builtin_key`、`version`、`payload_digest`。
- 在启动或显式维护动作中幂等 upsert 到 Shared Library 全局库表。
- 保留已安装项目资源的来源追踪，不直接修改项目内资源。

Seed/upsert 规则：

- 同一 `asset_type + scope=builtin + key` 是唯一资产。
- registry 版本高于库表版本时，更新 LibraryAsset payload 和 metadata。
- registry 中删除的 builtin 资产默认不硬删除库表记录，可标记为 `deprecated`，避免已安装项目来源断链。
- Project 资源不会因 builtin seed 更新而静默变化；更新只影响 Shared Library 可见资产。

### Agent Template

跨 Project 复用的角色模板。只描述稳定意图。

可复用字段：

- 角色 key / display name / description
- base system prompt / persona
- 默认 executor / provider / model / thinking / permission
- 抽象能力需求
- 抽象 MCP slots，例如 `repo`、`issue_tracker`

不可直接持有：

- Project MCP preset key
- Project SkillAsset key
- Project containers / VFS / workspace/root
- knowledge/memory
- default lifecycle/workflow
- companion whitelist
- story/task 默认标记

### Project Agent

项目内运行实体，由 Project 绑定到一个 `AgentTemplate` 或从 `AgentTemplate` 克隆创建。

负责：

- 项目显示名和职责说明覆写
- 显式开启的模型 / prompt / thinking / permission 覆写
- MCP slot 到 `ProjectMcpPreset` / `McpConnection` 的映射
- Skill Template 安装后的 Project Skill 选择
- knowledge 开关与 Project Agent knowledge VFS
- default lifecycle/workflow
- project containers 白名单
- companion whitelist
- story/task 默认标记

Project Agent 编辑器默认编辑 project override。编辑全局 `AgentTemplate` 必须进入明确的 Shared Library 管理入口。

Project Agent 覆写策略：

- 默认继承 `AgentTemplate`。
- 模型、thinking、permission、system prompt 可覆写，但必须显式启用对应 override。
- UI 同时展示模板默认值和项目覆写值。
- 保存 Project Agent 编辑器时只写 Project Agent override，不写 `AgentTemplate`。
- Shared Library 的 `AgentTemplate` 管理入口才允许修改全局模板，并必须提示影响范围。

### MCP Server Template / Connection / Project Preset

MCP 拆三层：

- `McpServerTemplate`: Shared Library 资产。定义 server 类型、参数 schema、默认 transport 模板、能力描述。
- `McpConnection`: 实际连接材料。可属于 user / org / local-machine / project，引用 `McpServerTemplate`，保存 token、env、base_url、stdio command 等差异项。
- `ProjectMcpPreset`: 项目内 agent-facing MCP key，引用 `McpConnection` 或从 `McpServerTemplate` 派生，持有授权、route policy、展示名。

本机 Local Runtime 的 `McpConnection` 应能选择公共 `McpServerTemplate`，以快速生成本机连接配置。

## Data Flow

### Builtin Seed 到 Marketplace

1. 应用加载 builtin seed registry。
2. 对每个 seed 计算稳定 source reference 与 digest。
3. Upsert 到 Shared Library asset 表。
4. Shared Library 查询统一返回 builtin/system/org/user/remote 资产。
5. Project 安装资产时只依赖 LibraryAsset，不再调用各资源独立 bootstrap。

### 从 Marketplace 安装项目资源

1. 用户在 Marketplace 选择 LibraryAsset。
2. 系统根据 `asset_type` 创建项目内资源：
   - AgentTemplate → Project Agent
   - McpServerTemplate → ProjectMcpPreset 或 McpConnection draft
   - Workflow Template → Project Workflow/Lifecycle
   - Skill Template → Project SkillAsset
3. 项目内资源记录 `InstalledAssetSource`，用于后续查看来源、重装、更新提示或重置。

安装语义：

- 默认是复制安装，Project 资源拥有自己的可编辑副本。
- 来源信息用于审计、重装、版本感知更新提示，不代表自动同步。
- 需要运行时引用的资源必须项目化，尤其是 Project Agent、ProjectMcpPreset、Project SkillAsset、Project Workflow。

### 版本感知更新

Project 资源记录安装来源 `InstalledAssetSource`：

- `library_asset_id`
- `source_ref`
- `source_version`
- `source_digest`
- `installed_at`

Shared Library 查询或 Project Assets 页面可以计算：

- `up_to_date`: Project 资源记录的 `source_version/source_digest` 与 Marketplace 当前值一致。
- `update_available`: LibraryAsset 当前版本或 digest 更新。
- `source_missing`: 来源资产不可见、被删除或 deprecated。

更新策略：

- 不做静默同步。
- 用户可以选择“查看变更 / 更新到新版 / 重新安装 / 保持当前副本”。
- 对已编辑的 Project 资源，更新前必须展示覆盖风险。
- 第一阶段可以只提供版本提示和手动重装；diff/三方合并作为后续增强。

### Project Agent 启动

1. 读取 Project Agent binding。
2. 合并 AgentTemplate defaults 与显式 Project override。
3. 解析 ProjectMcpPreset / McpConnection。
4. 解析 Project SkillAssets / Knowledge / VFS containers。
5. 输出 launch-ready executor config、capability state、VFS 和 session context。

## UI Boundaries

- `Marketplace`: 浏览、发现、安装 Shared Library 资产。
- `Shared Library 管理`: 管理公共资产、builtin、org/user 分享内容、远端导入。
- `Project Assets`: 管理当前 Project 已安装资源，不再承担公共 builtin bootstrap。
- `Agent Hub`: 管理 Project Agent 运行实例与 project override。
- `Settings / System`: 管 LLM Provider、全局运行配置和系统级 Marketplace 管理权限。
- `Settings / User`: 管个人偏好、用户级 Marketplace、个人 connections。
- `Local Runtime`: 管本机 profile、roots、本机 MCP connections；可引用 Marketplace 中的 `McpServerTemplate`。

## Migration Direction

- 将现有 `Agent` 语义收束为 `AgentTemplate` 或迁移为 `AgentTemplate` 来源。
- 将 `ProjectAgentLink` 升级为 Project Agent 运行配置承载点。
- 将 `mcp_presets` 保留为 Project 级资源，但新增来源字段指向 `McpServerTemplate` / `McpConnection`。
- 将 Workflow builtin templates、MCP builtin templates、Skill builtin templates 迁移到统一 Shared Library asset registry。
- 去除各资源自己的 builtin bootstrap API，改为统一 Marketplace install API。
- 将现有 builtin 资源的 `builtin_key` 迁移为 Marketplace `source_ref`，项目内资源保留 installed source metadata。
- 将现有 Project 级 builtin MCP/Skill/Workflow 视为已安装副本，而不是公共 builtin 本体。
- LibraryAsset 采用单表 JSONB；Project 运行表继续保持类型化字段/JSON 结构，不直接复用 LibraryAsset payload。
- 为已安装 Project 资源补齐来源版本字段，用于后续手动更新提示。

## Settled Design Decisions

- Agent 配置类型拆成 `AgentTemplateConfig` 与 `ProjectAgentConfigOverride`，不再继续复用一个大 `AgentPresetConfig`。
- 版本感知更新第一阶段只做提示 + 手动重装/覆盖；字段级 diff 和三方合并作为后续增强。

## Problem Coverage

| 原问题 | 当前设计如何解决 |
| --- | --- |
| Project Agent 编辑读取 merged config 却可能写回全局 Agent | Project Agent 编辑器默认只写 Project override；`AgentTemplate` 只能在独立 Shared Library 管理入口编辑，并提示影响范围。 |
| 后端有 Agent / ProjectAgentLink 分层，但前端体验没有边界 | 产品概念收束为 `AgentTemplate` 与 `ProjectAgent`，UI 边界拆成 Marketplace/Shared Library 管理与 Agent Hub/Project override。 |
| `agent.pi.user_preferences` 位于 system 设置导致用户偏好作用域混乱 | Settings 分工明确：user scope 承接个人偏好，system scope 只承接系统级 Provider、Shared Library 管理权限等。 |
| `AgentPresetConfig` 同时承载模型、prompt、MCP、Skill、companions 等多种语义 | 待拆成更窄类型：`AgentTemplate` 只保留稳定角色和默认执行配置；Project Agent override 承接项目覆写；项目运行资源留在 Project Asset。 |
| MCP Preset 既像项目资源又像公共模板 | MCP 拆为 `McpServerTemplate` / `McpConnection` / `ProjectMcpPreset`；公共模板进 Shared Library，项目运行 key 留在 Project。 |
| builtin bootstrap 分散在 Workflow、MCP、Skill 各自模块 | 统一 BuiltinSeedRegistry 幂等 upsert 到 Shared Library；各 Project 资源通过 Marketplace install，不再各自 bootstrap。 |
| 公共资产更新可能静默影响项目运行 | Project 资源复制安装，记录 `InstalledAssetSource`；只做版本感知和用户手动更新，不做静默同步。 |
| Marketplace JSONB 过于灵活可能污染运行层 | LibraryAsset payload 单表 JSONB，但每个 asset_type 必须经类型化 mapper/validator；运行路径只消费安装后的类型化 Project 资源。 |
