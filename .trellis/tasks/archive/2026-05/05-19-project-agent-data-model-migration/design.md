# Project Agent 数据结构迁移设计

## Target Model

目标模型分三层：

```text
Shared Library / Marketplace
  AgentTemplate
    公共、可复用、不可直接运行

Project
  ProjectAgent
    项目私有、可运行、可编辑、可发布

Runtime
  Session / Routine / Workflow Context / VFS
    只引用 ProjectAgent
```

`AgentTemplate` 仍由 `LibraryAsset(asset_type = agent_template)` 承载。`ProjectAgent` 是安装或手工创建后的项目资源副本，运行路径只读取 `ProjectAgent`。

## Proposed Domain Shape

建议新增或替换为：

```rust
pub struct ProjectAgent {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub agent_type: String,
    pub config: serde_json::Value,
    pub installed_source: Option<InstalledAssetSource>,
    pub default_lifecycle_key: Option<String>,
    pub is_default_for_story: bool,
    pub is_default_for_task: bool,
    pub knowledge_enabled: bool,
    pub project_container_ids: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

仓储收敛为：

```rust
pub trait ProjectAgentRepository {
    async fn create(&self, agent: &ProjectAgent) -> Result<(), DomainError>;
    async fn get_by_id(&self, id: Uuid) -> Result<Option<ProjectAgent>, DomainError>;
    async fn get_by_project_and_id(&self, project_id: Uuid, id: Uuid) -> Result<Option<ProjectAgent>, DomainError>;
    async fn get_by_project_and_name(&self, project_id: Uuid, name: &str) -> Result<Option<ProjectAgent>, DomainError>;
    async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<ProjectAgent>, DomainError>;
    async fn update(&self, agent: &ProjectAgent) -> Result<(), DomainError>;
    async fn delete(&self, project_id: Uuid, id: Uuid) -> Result<(), DomainError>;
}
```

不再提供 `list_by_agent`，因为不存在跨 Project 关联查询。

## Configuration Model

当前 `AgentPresetConfig` 同时被旧 `Agent.base_config`、`ProjectAgentLink.config_override`、Project config agent presets 共享。规范要求拆成 `AgentTemplateConfig` 与 `ProjectAgentConfigOverride` 等更窄类型。

建议本迁移分两步：

1. 数据结构迁移时先将旧 `base_config` 与 `config_override` 合并为 `ProjectAgent.config`，运行语义不变。
2. 在同一任务后半段或后续任务中，将命名从 `AgentPresetConfig` 收敛为 `ProjectAgentConfig`，并让 `AgentTemplateConfig` 到 `ProjectAgentConfig` 的安装映射显式化。

如果本任务一次性完成类型拆分，需要同步修改前端编辑器、Shared Library mapper、session construction、companion 过滤和 capability directive 解析，风险更高但结果更干净。

## Database Migration

目标表：

```sql
CREATE TABLE project_agents (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    name TEXT NOT NULL,
    agent_type TEXT NOT NULL,
    config TEXT NOT NULL DEFAULT '{}',
    installed_library_asset_id TEXT,
    installed_source_ref TEXT,
    installed_source_version TEXT,
    installed_source_digest TEXT,
    installed_at TEXT,
    default_lifecycle_key TEXT,
    is_default_for_story BOOLEAN NOT NULL DEFAULT FALSE,
    is_default_for_task BOOLEAN NOT NULL DEFAULT FALSE,
    knowledge_enabled BOOLEAN NOT NULL DEFAULT FALSE,
    project_container_ids TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(project_id, name)
);
```

迁移策略依赖主键决策：

- 方案 A：`project_agents.id = agents.id`。
  - 优点：Routine、Project Agent session route、session label、ProjectAgentSummary.key 当前都用旧 `agent_id`，迁移面较小。
  - 代价：VFS knowledge owner、Shared Library publish 的 `project_agent_link_id` 需要迁移到新 `project_agent_id`。
- 方案 B：`project_agents.id = project_agent_links.id`。
  - 优点：更贴近“Project 资源 id”，Shared Library publish 与 inline file owner 更自然。
  - 代价：Routine、Session、URL、agent key、workflow context 中大量旧 `agent_id` 绑定需要重写。

当前推荐方案 A，因为现有运行态多数使用旧 `agents.id` 作为 Project Agent key；同时项目未上线，可以直接把旧 link id 从 API 和 VFS surface 中删除。

旧数据合并规则：

- `project_id` 取 `project_agent_links.project_id`。
- `id` 按主键决策取 `agents.id` 或 `project_agent_links.id`。
- `name`、`agent_type`、基础配置取 `agents`。
- `config` 使用 `ProjectAgentLink::merged_preset_config` 等价逻辑合并旧 `agents.base_config` 与 `project_agent_links.config_override`。
- `installed_source`、默认 lifecycle、默认标记、knowledge、project containers 取旧 link。
- 如果发现同一个旧 `agents.id` 被多个 project link 引用，迁移脚本必须 fail-fast 或显式复制为多个 ProjectAgent；不允许静默保留全局共享语义。

## API Contract

新 API 建议：

```text
GET    /projects/{project_id}/agents
POST   /projects/{project_id}/agents
GET    /projects/{project_id}/agents/summary
PUT    /projects/{project_id}/agents/{project_agent_id}
DELETE /projects/{project_id}/agents/{project_agent_id}
POST   /projects/{project_id}/agents/{project_agent_id}/session
GET    /projects/{project_id}/agents/{project_agent_id}/sessions
```

DTO 命名建议：

- `ProjectAgentResponse`
- `CreateProjectAgentRequest`
- `UpdateProjectAgentRequest`
- `ProjectAgentSummaryResponse`
- `OpenProjectAgentSessionResponse`

删除或重命名：

- `ProjectAgentLinkResponse`
- `CreateProjectAgentLinkRequest`
- `UpdateProjectAgentLinkRequest`
- `/agent-links`

## Shared Library

安装 `agent_template`：

- 解析 `AgentTemplatePayload`。
- 创建 `ProjectAgent`。
- 写入 `installed_source`。
- 返回 `InstallLibraryAssetOutput::ProjectAgent { project_agent_id }`。

发布 `project_agent`：

- 输入 `project_asset_id` 解释为 `project_agent_id`。
- 读取 `ProjectAgent` 权威配置。
- 生成 `AgentTemplatePayload`。
- 不允许前端传 raw payload。

source-status：

- `project_agents` item 使用 `project_agent_id`。
- `installed_source` 从 `ProjectAgent` 读取。

## Runtime And VFS

Session construction：

- `ProjectAgentBridge` 由单个 `ProjectAgent` 构建。
- `agent_key` 语义改为 `project_agent_id`。
- `resolve_agent_default_lifecycle` 改为直接读取 `ProjectAgent.default_lifecycle_key`。

Workflow context：

- `SessionWorkflowOwner::Project` / `Routine` 使用 `project_agent_id`。
- Story 默认 Agent 从 `ProjectAgentRepository.list_by_project` 中找 `is_default_for_story`。

Routine：

- 字段建议改名为 `project_agent_id`。
- API DTO 使用 `project_agent_id`。
- 若因改动面控制暂时保留 DB 字段名 `agent_id`，领域层也应先改名，DB 字段在迁移中同步改最干净。

VFS：

- `InlineFileOwnerKind::ProjectAgentLink` 改为 `ProjectAgent`。
- owner kind 字符串从 `project_agent_link` 改为 `project_agent`。
- `ResolvedVfsSurfaceSource::ProjectAgentKnowledge` 移除 `link_id`，只保留 `project_id + project_agent_id`。
- surface ref 从 `project-agent-knowledge:{project_id}:{agent_id}:{link_id}` 改为 `project-agent-knowledge:{project_id}:{project_agent_id}`。

## Frontend

类型与 store：

- `ProjectAgentLink` 改为 `ProjectAgent`。
- `agentLinksByProjectId` 改为 `projectAgentsByProjectId` 或 `projectAgentConfigsByProjectId`。
- `fetchProjectAgentLinks` 等方法改为 `fetchProjectAgents` / `createProjectAgent` / `updateProjectAgent` / `deleteProjectAgent`。
- 注意当前已有 summary 方法也叫 `fetchProjectAgents`，需要区分“配置列表”和“summary 列表”，例如：
  - `fetchProjectAgentConfigs`
  - `fetchProjectAgentSummaries`

UI：

- Project Agent 编辑器、Routine 选择器、Marketplace 发布选择器不再展示 `link` 概念。
- “该 Agent 尚未在项目中链接，知识库需要 ProjectAgentLink 才能配置”改为基于 Project Agent 实例的文案。

## Spec Updates

需要同步更新：

- `.trellis/spec/backend/shared-library.md`
- `.trellis/spec/cross-layer/shared-library-contract.md`
- `.trellis/spec/backend/repository-pattern.md` 如有 Repository 示例需要补充
- README 中 Project 与 Agent 关系描述

## Risks

- 主键选择会影响 Routine、Session、VFS、publish/install 的迁移成本。
- `AgentPresetConfig` 命名与职责仍旧偏宽，若本任务不拆，会留下下一轮类型治理工作。
- VFS surface ref 是前后端共享字符串，必须同步改后端 parse / serialize 与前端 source type。
- 当前工作区已有未提交改动，实施前需要确认是否与本迁移冲突。
- 旧 `project_agent_links` 删除后，删除 Project Agent 必须同步清理知识库、routine 绑定或返回明确错误；此处需要产品决策。

## Recommended Phasing

1. 先决定 `ProjectAgent.id` 继承策略。
2. 更新领域 entity/repository 和 DB migration。
3. 改 Shared Library install/publish/source-status。
4. 改 API route 与 DTO。
5. 改 Session/Routine/Workflow context/VFS。
6. 改前端类型/store/UI。
7. 更新 specs/README。
8. 运行全链路验证。
