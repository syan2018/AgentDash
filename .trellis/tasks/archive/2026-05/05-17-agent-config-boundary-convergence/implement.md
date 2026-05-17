# Agent / Shared Library 收束实施计划

## Phase 0: Finalize Planning

- [ ] 确认 PRD 中的产品决策：
  - Project Agent 是唯一可运行实体。
  - AgentTemplate 只做跨 Project 模板复用。
  - MCP 拆为 McpServerTemplate / McpConnection / ProjectMcpPreset。
  - Shared Library 统一承接 builtin 与公共资产。
  - Builtin 资产物化进 Shared Library，全局 seed/upsert。
- [ ] 确认已收束的实现决策：
  - Agent 配置拆成 AgentTemplate / Project override 两个窄类型。
  - 版本感知更新第一阶段只做提示 + 手动重装/覆盖，字段级 diff 后续增强。
- [ ] 更新相关 `.trellis/spec/`：
  - Agent / Project Agent 边界。
  - Marketplace / builtin seed 统一入口。
  - MCP Server Template / Connection / Project Preset 分层。

## Long-Running Execution Shape

这应作为一个长程任务一次性推进，不拆成过多子任务。执行上保留阶段门槛，避免失控；任务管理上只拆 3 个聚合子阶段。

### Stage 1: Shared Library Foundation

目标：先建立统一公共资产底座，并把 builtin 来源收口进去，但尽量不改变现有 Project 运行行为。

交付物：

- 更新 `.trellis/spec/`：
  - Shared Library / Marketplace / LibraryAsset 命名。
  - `*Template` 共享资产约定。
  - Builtin seed registry 统一入口。
  - ProjectAsset / InstalledAssetSource 来源记录约定。
- 新增 Shared Library 领域模型与持久化：
  - `LibraryAsset`
  - `LibraryAssetType`
  - `LibraryAssetScope`
  - `LibraryAssetSource`
  - `InstalledAssetSource`
  - 单表 JSONB payload + 类型化 mapper/validator。
- 新增 builtin seed/upsert：
  - AgentTemplate seed 预留。
  - McpServerTemplate seed。
  - WorkflowTemplate seed。
  - SkillTemplate seed。
- 新增 Marketplace / Shared Library 查询 API。
- 新增 install/source-status 基础 API：
  - 安装到 Project 资源。
  - 计算 `up_to_date` / `update_available` / `source_missing`。
- Migration：
  - 现有 project-level builtin MCP/Skill/Workflow 视为已安装副本。
  - 补齐 source metadata 字段。

验证门槛：

- builtin seed upsert 幂等。
- LibraryAsset payload 按 `asset_type` 校验。
- 安装后的 Project 资源仍保持原运行语义。
- 旧 builtin bootstrap 入口可以暂时保留，但新路径已可用。

### Stage 2: Agent And MCP Runtime Convergence

目标：把最核心的运行边界收束掉：AgentTemplate / ProjectAgent 分离，MCP server template / connection / project preset 分离。

交付物：

- Agent 模型收束：
  - 将旧 `Agent` 语义迁移/重命名为 `AgentTemplate`。
  - 拆分 `AgentTemplateConfig` 与 `ProjectAgentConfigOverride`。
  - Project Agent 编辑默认只写 project override。
  - 全局 AgentTemplate 编辑入口必须显式提示影响范围。
- Session construction 更新：
  - 从 AgentTemplate defaults + Project override 组装 executor config。
  - Project 运行资源只从 ProjectAgent / ProjectAsset 解析。
  - AgentTemplate 不直接持有项目 MCP/Skill/VFS/Workflow key。
- MCP 模型收束：
  - `McpServerTemplate` 作为 Shared Library 资产。
  - 新增/启用 `McpConnection` 保存实际连接材料。
  - `ProjectMcpPreset` 保存 project agent-facing key、授权和 route policy。
  - Local Runtime 可以从 McpServerTemplate 创建 McpConnection。
- Project Agent 的 MCP slots：
  - AgentTemplate 只能声明抽象 slot。
  - ProjectAgent 负责 slot 到 ProjectMcpPreset / McpConnection 的映射。

验证门槛：

- Project Agent override 不写回 AgentTemplate。
- 同一个 AgentTemplate 可安装到多个 Project，运行配置互不污染。
- 从 builtin AgentTemplate 创建 Project Agent 并启动 session。
- 从 builtin McpServerTemplate 创建本机 McpConnection 与 ProjectMcpPreset，并被 session 解析。
- 现有 session construction 关键测试通过。

### Stage 3: UI Convergence And Old Path Retirement

目标：把用户入口和旧 bootstrap/import 路径收口，让产品体验与新模型一致。

交付物：

- 新增或重构 Marketplace / Shared Library 入口：
  - 浏览 builtin/system/org/user/remote assets。
  - 安装 AgentTemplate、McpServerTemplate、WorkflowTemplate、SkillTemplate。
  - 显示版本、来源、deprecated 状态。
- Project Assets 改为“已安装资源”管理：
  - 显示 InstalledAssetSource。
  - 显示 `up_to_date` / `update_available` / `source_missing`。
  - 第一阶段支持手动重装/覆盖，不做字段级 diff。
- Agent Hub 收口：
  - Project Agent 编辑器展示模板默认值与项目覆写值。
  - 覆写必须显式开启。
- Settings 收口：
  - system 管系统级 Shared Library、Marketplace 管理权限、LLM Provider。
  - user 管个人偏好、用户级 Shared Library、个人 connection。
  - project 跳转 Project Assets / Project Agent。
  - `agent.pi.user_preferences` 迁回 user scope。
- 旧路径退役：
  - UI 不再使用资源专属 builtin bootstrap。
  - 废弃或移除 `/workflow-templates/:builtin_key/bootstrap`。
  - 废弃或移除 `/projects/:project_id/mcp-presets/bootstrap`。
  - 废弃或移除 `/projects/:project_id/skill-assets/bootstrap`。

验证门槛：

- 用户能从 Marketplace 安装四类模板资产到 Project。
- Project Assets 能展示来源和版本状态。
- Project Agent 编辑不会混淆全局模板和项目覆写。
- Local Runtime 从 McpServerTemplate 创建 connection 的流程可用。
- 旧 bootstrap UI 入口不存在，后端旧 API 不再被前端调用。

## Phase 1: Domain Model

- [ ] 新增 Shared Library 领域模型：
  - `LibraryAsset`
  - `LibraryAssetType`
  - `LibraryAssetScope`
  - `LibraryAssetSource`
  - `LibraryAssetRepository`
  - `InstalledAssetSource`
- [ ] 为每种 LibraryAsset payload 定义类型化 mapper / validator：
  - `AgentTemplatePayload`
  - `McpServerTemplatePayload`
  - `WorkflowTemplatePayload`
  - `SkillTemplatePayload`
- [ ] 新增 builtin seed registry 抽象：
  - 统一收集 AgentTemplate、McpServerTemplate、WorkflowTemplate、SkillTemplate seed。
  - seed 包含 `asset_type`、`builtin_key`、`version`、`payload_digest`、`payload`。
- [ ] 收束 Agent 模型：
  - 明确现有 `Agent` 是否迁移/重命名为 AgentTemplate。
  - 明确 Project Agent 运行配置字段。
  - 拆分 `AgentPresetConfig` 为 `AgentTemplateConfig` / `ProjectAgentConfigOverride`。
- [ ] 收束 MCP 模型：
  - `McpServerTemplate` 作为 Shared Library 资产。
  - `McpConnection` 独立保存连接材料。
  - `ProjectMcpPreset` 保存项目内 server key 与授权/route policy。

## Phase 2: Persistence And Migration

- [ ] 新增 Shared Library 表与索引。
  - 单表 JSONB payload。
  - 唯一约束覆盖 `asset_type + scope + owner_id + key` 或等价稳定 identity。
  - 保存 `version`、`payload_digest`、`deprecated` 等版本感知字段。
- [ ] 新增 McpConnection 表。
- [ ] 为 Project 资源表补来源字段：
  - `library_asset_id`
  - `source_ref`
  - `source_version`
  - `source_digest`
  - `installed_at`
- [ ] 编写 migration：
  - 现有 project-level builtin MCP Preset 转为已安装副本。
  - 现有 project-level builtin SkillAsset 转为已安装副本。
  - 现有 builtin Workflow 实例转为已安装副本。
  - 现有 Agent base config 迁移到 AgentTemplate / Project Agent 目标模型。

## Phase 3: Application And API

- [ ] 新增 Shared Library 查询 API。
- [ ] 新增 Marketplace / Shared Library install API：
  - install AgentTemplate as Project Agent
  - install McpServerTemplate as ProjectMcpPreset / McpConnection draft
  - install Workflow Template as Project Workflow/Lifecycle
  - install Skill Template as Project SkillAsset
- [ ] 新增 Shared Library source status API 或 Project Assets 聚合字段：
  - `up_to_date`
  - `update_available`
  - `source_missing`
  - current marketplace `version/digest`
- [ ] 新增 builtin seed/upsert 服务。
- [ ] 废弃资源专属 bootstrap API 的使用路径：
  - `/workflow-templates/:builtin_key/bootstrap`
  - `/projects/:project_id/mcp-presets/bootstrap`
  - `/projects/:project_id/skill-assets/bootstrap`
- [ ] 更新 session construction：
  - Project Agent 从 AgentTemplate defaults + explicit override 组装运行 config。
  - MCP 从 ProjectMcpPreset / McpConnection 解析，不从 AgentTemplate 直接读取项目 key。

## Phase 4: Frontend

- [ ] 新增 Marketplace / Shared Library 入口。
- [ ] Project Assets 页面改为“已安装资源”管理。
- [ ] Project Assets 显示 Shared Library 来源版本状态：
  - 已是最新
  - 有新版
  - 来源不可见或已废弃
- [ ] 提供手动更新入口；第一阶段可先支持“查看来源 + 重新安装/覆盖”，字段级 diff 后续增强。
- [ ] Agent Hub 编辑器改为默认编辑 Project Agent override。
- [ ] AgentTemplate 管理入口单独展示影响范围。
- [ ] Local Runtime McpConnection 支持选择公共 McpServerTemplate 并填写本机差异项。
- [ ] Settings 页收束：
  - system 管系统级 Marketplace 和 LLM Provider。
  - user 管个人偏好、用户级 Marketplace、个人 connection。
  - project 跳转 Project Assets / Project Agent。

## Phase 5: Validation

- [ ] 后端单元测试：
  - builtin seed upsert 幂等。
  - LibraryAsset scope 权限。
  - JSONB payload 按 asset_type 校验，非法 payload 不能安装。
  - Marketplace install 生成正确 Project 资源。
  - 来源版本状态计算正确。
  - Project Agent override 不写回 AgentTemplate。
- [ ] 前端测试：
  - Project Agent 编辑保存只调用 project override API。
  - Marketplace 安装流程。
  - Local Runtime 从 McpServerTemplate 创建 McpConnection。
- [ ] 集成测试：
  - 从 builtin AgentTemplate 创建 Project Agent 并启动 session。
  - 从 builtin McpServerTemplate 创建本机 McpConnection 与 ProjectMcpPreset 并被 session 解析。
  - 从 builtin Skill Template 安装到 Project 后进入 VFS。

## Risk And Rollback Points

- Shared Library 单表 JSONB 方案灵活，但类型校验压力更大；需要明确 payload schema 和 mapper 测试。
- 自动同步 Marketplace 更新到 Project 资源风险高，默认不做静默同步；只做版本感知和用户手动更新。
- 现有 project-level builtin 实例要视为已安装副本，避免 migration 后 Project 资源突然变成只读或丢失用户编辑。
- Agent 配置迁移涉及运行路径，必须先覆盖 session construction 测试再切换 UI。
