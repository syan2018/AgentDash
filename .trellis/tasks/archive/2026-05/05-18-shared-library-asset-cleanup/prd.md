# 清理平台通用配置资产旧入口与安装一致性

## Goal

把公共配置资产化迭代收束到可交付状态：Shared Library / Marketplace 成为唯一的公共资产入口，Project 侧只保留可运行、可编辑、可追踪来源版本的项目资源副本。清理旧的 per-asset builtin/bootstrap 路径和 Agent 全局复用假设，避免用户在安装、更新、删除、重新导入时遇到来源不一致、版本不可见或残留数据冲突。

## Confirmed Facts

- 当前 Agent 仍使用全局 `agents` 表 + `project_agent_links` 多对多模型；Marketplace 安装 AgentTemplate 会创建全局 Agent，再创建项目关联，导致安装来源无法像 MCP/Skill/Workflow 一样跟随项目资源展示版本状态。
- Project Agent 页仍提供“新建 Agent 并关联到项目”和“关联已有 Agent”，这与“不复用 Agent 实例，只复用 AgentTemplate”的设计意图冲突。
- MCP/Skill/Workflow 仍保留旧 builtin/bootstrap UI 与 API，包括 MCP Preset bootstrap、Skill builtin bootstrap/reset、Workflow template bootstrap。Marketplace 已存在后，这些入口会让用户无法判断哪个才是公共资产来源。
- Marketplace 安装 MCP 后，项目 MCP 卡片来源仍显示 `user`，因为项目副本的可编辑来源与安装来源展示没有区分。
- Workflow 删除目前只删除 LifecycleDefinition，没有同步清理同一 Marketplace 安装包产生的 WorkflowDefinition，导致重新安装时 key 冲突。
- 项目处于预研阶段，不需要 API/数据库兼容兜底，应直接收束为正确模型，并处理初始化/migrate。

## Requirements

- Agent 必须从“全局可复用 Agent + 项目关联”收束为“Project Agent 是项目资源副本，AgentTemplate 才是跨项目可复用资产”。
- AgentTemplate 安装到项目后必须记录 `InstalledAssetSource`，能参与 Marketplace 项目来源状态与手动覆盖更新。
- Project Agent 创建入口只创建项目内 Agent，不提供关联已有全局 Agent；相关前端入口、API、仓储语义和安装路径都要统一。
- Marketplace 安装出的 MCP/Skill/Workflow/Agent 项目副本必须在项目资产 UI 中展示清晰来源：用户手工创建、Marketplace 安装、远端导入等不能混淆。
- 旧 per-asset builtin/bootstrap 入口必须从用户路径中移除；公共内置配置只通过 Shared Library seed 进入 Marketplace。
- Workflow/Lifecycle 删除必须能清理 Marketplace 安装包造成的成组资源残留，删除后可从 Marketplace 重新安装同一资源。
- 不引入兼容分支或旧字段兜底；若现有命名/字段阻碍正确模型，应直接迁移到正确结构。
- 本次实现必须包含循环 review：完成后至少进行代码自查、测试验证、旧入口搜索确认、关键用户路径复核。

## Acceptance Criteria

- [ ] Project Agent 不再依赖“关联已有全局 Agent”作为用户可见工作流；创建/安装/编辑/删除围绕项目资源进行。
- [ ] Marketplace 安装 AgentTemplate 后，项目来源状态中可看到 Agent 项，并能识别 `up_to_date` / `update_available` / `source_missing`。
- [ ] MCP 卡片对 Marketplace 安装项展示 Marketplace/Shared Library 来源，而不是单纯显示 `user`。
- [ ] Assets 页不再出现 MCP/Skill/Workflow 各自的旧 builtin 装载按钮；相关前端 service/store 不再调用旧 bootstrap/reset 端点。
- [ ] 后端不再暴露旧 MCP/Skill/Workflow builtin bootstrap/reset 路由，或这些路由已被正确删除并消除编译引用。
- [ ] 删除 Marketplace 安装的 Workflow/Lifecycle 后，关联 workflow definitions 被清理，随后可以从 Marketplace 重新安装同一资源。
- [ ] Shared Library seed 仍能把内置 Agent/MCP/Workflow/Skill 模板写入 LibraryAsset，并保持幂等。
- [ ] `rg` 复查旧入口关键词时，只剩与新 Shared Library seed、非资产 builtin 语义或测试说明相关的合理引用。
- [ ] Rust 检查、相关 Rust 测试、前端 typecheck 通过；必要时补充针对安装来源和 Workflow 删除清理的单元测试。

## Out Of Scope

- 字段级 diff、三方合并、自动静默同步。
- MCP Connection 凭证/本机 profile 的完整独立管理页。
- 组织级/用户级 Shared Library 权限治理。
- 旧线上数据兼容迁移策略；当前项目未上线，只保留把本地库推进到正确结构所需的 migrate/初始化。
