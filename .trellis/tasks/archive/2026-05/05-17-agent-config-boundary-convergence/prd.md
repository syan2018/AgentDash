# Agent 配置边界收束讨论

## Goal

梳理并收束 AgentDash 中 Agent、Project Agent、MCP、Skill、Settings、Capability 等配置的归属边界，形成一套可执行的产品/领域模型决策。

这项任务先不实现代码，目标是回答：

- Agent 是否仍然需要跨 Project 复用。
- 如果需要复用，复用的是 `AgentTemplate` 这类 Shared Library 模板，还是“可运行实体”。
- MCP、Skill、Knowledge、Workflow、Capability、用户偏好、系统配置分别属于哪一层。
- 当前设置页和 Agent Hub 应该如何收口，避免用户把全局配置误当作项目配置。

## Confirmed Facts

- 项目定位是多设备、多项目统一管理 AI Agent 协同；云端拥有 Project / Story / Task / Workspace / Settings / Session 事件，本机拥有 Agent 进程和物理文件。
- 当前领域层已有独立 `Agent` 与 `ProjectAgentLink`：`Agent.base_config` 保存基础配置，`ProjectAgentLink.config_override` 保存 per-project 覆写。
- `ProjectAgentLink` 还承载 default lifecycle、story/task 默认标记、knowledge 开关、project container 白名单等项目内运行语义。
- 当前 `AgentPresetConfig` 被复用于 `Agent.base_config`、`ProjectAgentLink.config_override` 和旧 Project preset config，字段覆盖范围很广：模型、system prompt、能力、MCP preset keys、Skill keys、companions 等都在同一个结构里。
- 当前前端 Project Agent 编辑链路存在边界混淆风险：在 Project Agent 视图里读取的是 merged config，但保存时可能写回全局 Agent base config。
- LLM 模型配置 spec 已明确 Agent 运转核心参数主要是 `model_id`、`provider_id`、`thinking_level`；`temperature` 等底层采样参数不应暴露给业务层。
- 预研阶段不需要兼容性方案，可以选择最正确的数据模型并配套 migration。
- 产品决策：保留跨 Project 的 Agent 复用，但复用粒度收束为 Shared Library 中的 `AgentTemplate`；`Project Agent` 是唯一可运行实体。一个可运行 Agent 实例不应直接跨 Project 共享。
- `AgentTemplate` 只复用角色、基础 prompt、默认 executor/model/thinking/permission、抽象能力需求等稳定意图；MCP、Skill、Knowledge、Workflow、Project containers、项目默认标记等运行资源必须留在 Project Agent 层。
- 当前 MCP Preset 已是 Project 级资源：API 形态为 `/projects/:project_id/mcp-presets`，数据库 `mcp_presets` 以 `project_id` 作为必填归属，前端 Assets MCP Preset 面板也按当前 Project 加载。
- 当前 MCP Preset 已有 builtin 模板机制，但 builtin 会实例化为某个 Project 下的 preset；它不是用户/组织可管理、可分享、可被本机 connection 标识引用的公共配置库。
- 产品倾向：需要保留公共 MCP 配置能力，尤其支持本机 `McpConnection` 快速标识/引用公用配置，减少重复填写 transport/schema/default route 等信息。
- 产品决策：公共 `McpServerTemplate` 按 `system` / `org` / `user` 三层建模。实现时可以分阶段开放 UI，例如先内置 system builtin 与 user custom，但领域模型必须预留 org/team 共享能力，避免后续企业共享配置时返工。
- MCP 收束方向：`McpServerTemplate` 管公共 server 定义与参数 schema，`McpConnection` 管用户/组织/本机/项目的实际连接材料，`ProjectMcpPreset` 管项目内 agent-facing key、授权和 route policy。
- 产品决策：Project Agent 允许覆写 `AgentTemplate` 中的模型和 system prompt，但覆写必须显式开启；UI 需要区分“模板默认值”和“项目覆写值”，避免把项目改动误写成全局模板。
- 产品决策：需要建设统一的全局 Shared Library 作为 builtin 与公共配置的存储、版本和分享入口；Marketplace 是面向用户的浏览/安装界面。Builtin 不应再由 Workflow、MCP、Skill 等每个资源点各自维护一套 bootstrap/import 模块；各项目资源应从 Marketplace 安装/引用/克隆。
- 产品决策：Marketplace 中的 builtin 资产应物化到全局库表；代码内置 registry 只作为种子来源，在启动或显式维护动作中幂等 seed/upsert 到 Marketplace。这样 builtin、system、org、user、remote/imported 资产走同一套查询、权限、安装、版本展示链路。
- 产品决策：LibraryAsset 采用单表 JSONB payload，以获得跨资产类型的灵活性；每个 `asset_type` 必须有类型化 mapper / schema 校验，避免任意 JSON 穿透到业务运行层。
- 产品决策：Project 资源与 Marketplace 来源之间采用版本感知 + 用户手动更新。系统显示来源版本、当前 Marketplace 版本、diff/变更提示，由用户显式选择更新、重装或保持当前副本；不做静默同步。
- 产品决策：拆分旧 `AgentPresetConfig`，改为 `AgentTemplateConfig` 与 `ProjectAgentConfigOverride` 等更窄类型。共享模板和项目覆写不能继续共用一个大 config，否则概念边界会在实现层复发。
- 产品决策：版本感知更新第一阶段只做状态提示 + 手动重装/覆盖，不立即实现字段级 diff 或三方合并。
- 当前 builtin/import 入口分散：
  - Workflow 通过 `/workflow-templates` 与 `/workflow-templates/:builtin_key/bootstrap` 装载 builtin workflow template。
  - MCP Preset 通过 `/projects/:project_id/mcp-presets/bootstrap` 装载 builtin MCP preset。
  - Skill Asset 通过 `/projects/:project_id/skill-assets/bootstrap` 与 reset-from-builtin 管理 builtin seed。
  - Skill Asset 另有 GitHub / ClawHub / skills.sh 远端导入链路。

## Requirements

- 明确 Agent 的产品语义：
  - `AgentTemplate` 是否存在。
  - `Project Agent` / 项目内实例是否是唯一可运行实体。
  - 旧 `Agent` 是否应改名、拆分，或限制为模板。
- 明确跨 Project 可复用内容：
  - 可复用：角色定位、基础 prompt、默认 executor/model/thinking/permission、抽象能力需求等。
  - 不可直接复用：项目 MCP 绑定、Skill 资产、VFS/root/container、knowledge、default lifecycle/workflow、companions、项目默认标记等。
- 明确 MCP 的仓储边界：
  - 是否需要跨 Project MCP catalog/server definition。
  - credential/connection 是否独立于 Project MCP preset。
  - Agent 是否只能声明抽象 MCP slot，而不能直接引用项目内 MCP key。
  - 公共 MCP 配置应在哪个产品入口管理、如何被 `ProjectMcpPreset` 和本机 `McpConnection` 引用。
- 明确 Skill / Knowledge / VFS / Capability 的归属：
  - 哪些属于 Project Agent binding。
  - 哪些属于 Project 共享资源。
  - 哪些属于系统或用户设置。
- 明确 Settings 页收束目标：
  - system、user、project、local-runtime 各自只承接哪些配置。
  - 是否移除或迁移 `agent.pi.user_preferences` 的 system 面板入口。
  - Project 设置页与 Agent Hub 的分工。
- 明确 Marketplace / Library 收束目标：
  - 统一承接 `AgentTemplate`、`McpServerTemplate`、`WorkflowTemplate`、`SkillTemplate` 等公共模板资产。
  - 统一表达 builtin、system、org、user、remote/imported 等来源。
  - Project 资源从 Marketplace 安装/引用/克隆，不再按资源类型各自实现 builtin bootstrap。
  - 本机 `McpConnection` 可以快速选择 Marketplace 中的公用 `McpServerTemplate`，并只填写本机差异项。
- 明确共享数据命名：
  - `Shared Library`：公共资产存储与 API 层。
  - `Marketplace`：Shared Library 的浏览、发现、安装 UI。
  - `LibraryAsset`：Shared Library 中的统一资产记录。
  - `*Template`：Shared Library 中所有可分享、可安装的共享配置统一后缀，例如 `AgentTemplate`、`McpServerTemplate`、`WorkflowTemplate`、`SkillTemplate`。
  - `InstalledAssetSource`：Project 资源记录其来源资产、版本和 digest 的元数据。
  - `Project Asset`：安装到 Project 后可运行、可编辑的项目内资源。
- 明确前端编辑语义：
  - 用户在 Project Agent 中点击“编辑配置”时，默认编辑 project override 还是全局模板。
  - 如果允许编辑全局模板，UI 必须显式表达影响范围。
- 明确后端/API 收束目标：
  - 是否保留 merged config 只读输出。
  - 是否新增/强化 project override API。
  - 是否需要把字段拆成更小的类型而不是继续复用一个大 `AgentPresetConfig`。
- 形成后续实现任务所需的设计约束、迁移方向和验收标准。

## Acceptance Criteria

- [ ] PRD 明确列出所有需要收束的配置类别及目标归属层。
- [ ] PRD 明确 Agent 是否跨 Project 复用，以及复用粒度。
- [ ] PRD 明确 MCP 是否跨 Project 仓储，以及 Agent 与 MCP 的引用方式。
- [ ] PRD 明确公共 MCP 配置的管理入口、分享范围和本机 connection 的引用方式。
- [ ] PRD 明确 Settings 页、Project Settings 页、Agent Hub 的产品分工。
- [ ] PRD 明确全局 Shared Library / Marketplace 是否作为 builtin 与公共配置统一入口，以及 Project 资源如何从中安装/引用。
- [ ] PRD 明确 builtin 资产的落库/seed 策略，不再为每种资源保留独立 bootstrap 入口。
- [ ] PRD 明确 Marketplace payload 存储策略和 Project 资源的版本感知更新策略。
- [ ] 复杂度足够高时，补充 `design.md` 记录目标领域模型、数据流、API/UI 边界和 migration 方向。
- [ ] 复杂度足够高时，补充 `implement.md` 拆分可执行阶段，不在本任务中默认直接实现。

## Out of Scope

- 本阶段不直接重构数据库或 API。
- 本阶段不修复 Project Agent 编辑器 bug，除非用户明确要求进入实现。
- 本阶段不设计具体 UI 视觉稿，只定义信息架构和配置归属。
- 本阶段不讨论第三方 Agent connector 的底层执行协议，除非它影响配置归属。

## Open Questions

- 当前规划已无阻塞型产品决策；后续实现中只需在各阶段内细化 API 字段和 UI 文案。

## Notes

- Keep `prd.md` focused on requirements, constraints, and acceptance criteria.
- Lightweight tasks can remain PRD-only.
- For complex tasks, add `design.md` for technical design and `implement.md` for execution planning before `task.py start`.
