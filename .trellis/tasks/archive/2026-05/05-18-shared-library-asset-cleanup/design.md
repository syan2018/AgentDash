# 清理平台通用配置资产旧入口与安装一致性设计

## Design Intent

公共资产只有一类：`LibraryAsset`。用户在 Marketplace 中发现、安装、更新它；项目内运行时永远读取安装后的 Project 资源副本。`builtin` 是 Shared Library 的 seed 来源，不再是 MCP/Skill/Workflow/Agent 各自 UI 里的第二套导入渠道。

## Target Model

### Agent

- 取消用户可见的“全局 Agent 可复用实例”概念。
- 保留运行时需要的 Agent 配置实体，但语义收束为 Project Agent：
  - `project_agent_links` 承载项目归属、配置覆写、默认 lifecycle、knowledge、安装来源。
  - 全局 `agents` 若仍被运行时大量引用，短期可作为 Project Agent 的配置子表存在，但不得再由用户跨项目选择复用。
  - 新建 Project Agent 与 Marketplace 安装 AgentTemplate 都创建一对项目私有 Agent + link，并在 link 上记录 `InstalledAssetSource`。
- Project Agent 列表、编辑、会话启动继续以 `agent_id`/link 为运行时 key，但 UI 文案和 API 输入不再表达“关联已有”。

### Installed Source

- MCP/Skill/Workflow 已有 `installed_source` 字段保持使用。
- Agent 增加 `ProjectAgentLink.installed_source`：
  - `library_asset_id`
  - `source_ref`
  - `source_version`
  - `source_digest`
  - `installed_at`
- `list_project_asset_source_status` 增加 `project_agents` 分组。
- DTO 与前端类型显式输出 Agent 项目的安装来源状态。

### Source Display

- 项目资源 UI 展示分两层：
  - Project resource kind/source：手工创建的可编辑副本仍可用 user/user_authored 表达。
  - Installed source：来自 Marketplace/Shared Library 的副本优先展示“Marketplace”或版本状态。
- MCP/Skill/Workflow/Agent 卡片都以 `installed_source` 优先决定市场来源标记，避免 Marketplace 安装后仍显示“User”造成误解。

### Old Builtin Channels

- 删除用户可见旧入口：
  - MCP Preset “装载内置 Preset”
  - Skill “装载内嵌 Skill”与 builtin reset
  - Workflow “注册内置 Bundle”
  - Agent “关联已有 Agent”
- 删除对应前端 service/store 方法和 API 路由引用。
- Shared Library 的 `seed-builtin` 是唯一 builtin 物化入口，Marketplace “同步内置资源”保留。

### Workflow Deletion

- Marketplace 安装 WorkflowTemplate 会创建多个 workflow definitions + 一个 lifecycle definition，且它们共享同一个 `InstalledAssetSource.library_asset_id`。
- 删除 lifecycle 时：
  - 若 lifecycle 有 `installed_source`，查找同项目、同 `library_asset_id` 的 workflow definitions。
  - 删除这些 workflow definitions 前确认它们未被其它 lifecycle 引用；同一安装包 lifecycle 自身引用不构成阻塞。
  - 删除 lifecycle 后清理对应 workflow definitions。
- 手工创建 lifecycle 删除保持只删除 lifecycle；手工 workflow definition 仍按现有引用保护删除。

## Data Flow

1. Shared Library seed upsert 内置模板到 `library_assets`。
2. Marketplace list 读取 `LibraryAsset`。
3. Marketplace install 调用 Project install API。
4. 安装服务按类型创建 Project 资源，并写入 `InstalledAssetSource`。
5. Project 资产页面读取项目资源 DTO；来源 badge 优先根据 `installed_source`。
6. Marketplace 项目来源状态读取所有带 `InstalledAssetSource` 的 Project 资源，并与当前 `LibraryAsset.version/payload_digest` 比较。
7. 用户点击更新时使用 overwrite install 覆盖项目副本。

## Migration / Initialization

- 在 `project_agent_links` 初始化中增加安装来源列；本项目可直接使用 `ALTER TABLE ... ADD COLUMN IF NOT EXISTS` 的现有初始化风格。
- 若清理 API/前端类型导致字段或 route 变化，不保留旧端点兼容。

## Risks

- Agent 运行时当前大量通过 `agent_repo + agent_link_repo` 组装 session，不宜在本迭代一次性移除 `agents` 表。更稳妥的收束是删除全局复用用户路径，并把 link 标记为项目资源/安装来源。
- Workflow 删除需要避免误删被其它 lifecycle 引用的 workflow definitions，因此删除逻辑必须按引用关系过滤。
- 前端旧入口分散在 Assets 面板、store、service、类型和 tests 中，必须用搜索闭环验证。
