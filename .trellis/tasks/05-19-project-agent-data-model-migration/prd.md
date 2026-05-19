# Project Agent 数据结构迁移规划

## Goal

将当前 `Agent + ProjectAgentLink` 的旧式全局 Agent 关联模型迁移为清晰的 `AgentTemplate + ProjectAgent` 模型。

目标是让 Project 内运行、编辑、发布、安装、知识库、Routine、Session 绑定都只面向项目实例 `ProjectAgent`，跨项目复用只通过 Shared Library / Marketplace 中的 `AgentTemplate` 发生，不再保留用户可见或领域层可误用的全局 Agent 概念。

## Background

- 项目仍处于预研期，用户明确要求不做兼容方案、回退方案或旧 API 兜底，允许直接迁移到最正确的状态。
- 现有规范已经定义 Shared Library 中共享配置使用 `AgentTemplate`，Project 内运行资源使用 `ProjectAgent`。
- `.trellis/spec/backend/shared-library.md` 明确要求用户可见路径不提供“关联已有全局 Agent”；跨项目复用只发生在 `AgentTemplate`。
- 当前代码仍保留 `Agent`、`ProjectAgentLink`、`AgentRepository`、`ProjectAgentLinkRepository`、`/agent-links`、`project_agent_link` owner kind 等旧概念。
- 当前 `ProjectAgentLink` 实际承载的是 Project Agent 自身配置：安装来源、默认 lifecycle、Story/Task 默认标记、知识库开关、项目容器白名单、config override。

## Confirmed Evidence

- Domain 层 `Agent` 注释仍描述“独立 Agent 实体”以及通过 `ProjectAgentLink` 与 Project 多对多关联。
- Repository 层仍提供 `find_by_project_and_agent`、`list_by_agent`，表达全局 Agent 被多个 Project 关联的能力。
- DB 层仍有 `agents` 与 `project_agent_links` 两张表，并以 `UNIQUE(project_id, agent_id)` 表达关联模型。
- API 层虽然注释写“创建项目私有 Agent”，实际仍先创建 `Agent` 再创建 `ProjectAgentLink`。
- Shared Library 安装 `AgentTemplate` 时仍返回 `agent_id + project_agent_link_id`。
- Shared Library 发布 Project Agent 时读取 `project_agent_link` 作为项目资产。
- Session construction、Routine、VFS knowledge surface、inline file owner、前端 Project Store、Routine UI、Agent 编辑器、Marketplace 发布选择器都依赖 `ProjectAgentLink` 或 `agent_id + link_id`。

## Requirements

- 引入单一领域实体 `ProjectAgent`，作为 Project 内 Agent 的唯一运行与编辑身份。
- 移除或重命名 `Agent` / `ProjectAgentLink` 旧领域概念，避免领域层继续表达全局 Agent 多对多关系。
- 新数据结构必须承载当前 Project Agent 的全部有效字段：
  - `id`
  - `project_id`
  - `key` 或 `name`
  - `agent_type` 或 executor fallback
  - 项目内配置
  - `installed_source`
  - `default_lifecycle_key`
  - `is_default_for_story`
  - `is_default_for_task`
  - `knowledge_enabled`
  - `project_container_ids`
  - `created_at`
  - `updated_at`
- Shared Library 中继续使用 `AgentTemplate` 表达可复用模板。
- Marketplace 安装 `AgentTemplate` 必须直接创建 `ProjectAgent`，并在 `ProjectAgent.installed_source` 上记录来源。
- Project Agent 发布必须直接从 `ProjectAgent` 读取权威状态生成 `AgentTemplate` payload。
- Routine、Project Agent Session、Session construction、workflow context、companion 过滤都必须以 `project_agent_id` 作为绑定身份。
- VFS Agent Knowledge 必须从 `project_agent_link` owner 迁移到 `project_agent` owner。
- 前端类型、store、API service、UI 文案必须删除 `ProjectAgentLink` 命名。
- README 和 Trellis specs 中旧的 `ProjectAgentLink` 表述必须更新为 `ProjectAgent`。
- 数据库迁移必须处理现有开发库数据，将 `agents + project_agent_links` 合并为 `project_agents`。
- 不提供旧 `/agent-links` API 兼容，不保留旧 repository trait，不保留旧前端 mapper 兜底。

## Acceptance Criteria

- [ ] Domain 层存在清晰的 `ProjectAgent` entity 和 `ProjectAgentRepository`，不存在表达全局 Agent 多对多关系的 `ProjectAgentLinkRepository`。
- [ ] DB schema 以 `project_agents` 为 Project Agent 权威表；旧 `agents` / `project_agent_links` 不再是运行路径依赖。
- [ ] Marketplace 安装 `agent_template` 返回 Project Agent 安装结果，且 source-status 中 `project_agents` 使用 `project_agent_id`。
- [ ] Project Agent 发布入口以 `project_agent_id` 读取项目资源，不再读取 link。
- [ ] Project Agent CRUD、summary、session open/list API 使用 `/projects/{project_id}/agents...` 风格路径或等价的新命名。
- [ ] Routine 创建/更新/执行校验绑定的是当前 Project 下存在的 `ProjectAgent`。
- [ ] Session construction 和 workflow context resolution 不再通过 `(project_id, agent_id)` 查 link。
- [ ] VFS surface ref 与 inline file owner 不再暴露 `project_agent_link` 或 `link_id`。
- [ ] 前端 `ProjectAgentLink` 类型、`agentLinksByProjectId` store 字段、`fetchProjectAgentLinks` 等命名迁移完成。
- [ ] 旧注释和用户可见文案中不再出现“ProjectAgentLink 才能配置”等旧模型描述。
- [ ] Rust 相关测试通过，至少覆盖 Project Agent route serialization、Shared Library install/publish/source-status、Routine agent 校验、VFS project agent knowledge surface。
- [ ] 前端 `pnpm --filter app-web typecheck` 和相关测试通过。
- [ ] 迁移后运行路径不直接消费 `LibraryAsset.payload`，仍遵守先安装成 Project 资源的契约。

## Out Of Scope

- 不实现 AgentTemplate 的字段级 diff、三方合并或自动更新。
- 不恢复旧资源专属 bootstrap/reset 入口。
- 不设计跨项目共享运行态 Agent。
- 不做旧 API 兼容层。
- 不处理生产级在线滚动迁移或回滚策略。

## Open Questions

- 新 `ProjectAgent.id` 应继承旧 `agents.id`，还是继承旧 `project_agent_links.id`？
- Project Agent 配置是否在本次迁移中一并拆成更窄类型，还是先保持 `AgentPresetConfig` 作为内部过渡命名再单独治理？
