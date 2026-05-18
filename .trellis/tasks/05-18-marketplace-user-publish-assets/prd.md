# Marketplace 用户发布配置资产

## Goal

补齐 Marketplace / Shared Library 的反向发布闭环：用户可以把当前 Project 中已经调好的 Agent、MCP、Workflow、Skill 配置显式发布为 `user_authored` 的 Shared Library 模板资产，并能在 Marketplace 中浏览、安装到其它 Project、追踪来源版本。

## Background

- 现有 Shared Library 已经统一承接 `AgentTemplate`、`McpServerTemplate`、`WorkflowTemplate`、`SkillTemplate` 四类公共配置资产。
- `LibraryAsset` 已具备 `scope = builtin/system/org/user` 与 `source = builtin/user_authored/remote_imported`，但当前 API 只有 list / get / seed builtin / install / source-status，没有用户主动创建或发布市场资产的入口。
- Project 资源安装后记录 `InstalledAssetSource`，Marketplace 可通过 source-status 判断 `up_to_date` / `update_available` / `source_missing`。
- 当前项目处于预研期，不需要兼容旧字段或保留旧入口；模型应直接按正确边界演进，并同步数据库 migrate。

## Foundational Principles

- Shared Library 是公共资产存储与 API 层，Marketplace 是浏览、发现、安装与发布 UI。
- Marketplace 中的资产是模板，不是可运行实体；Project 运行时只消费安装后的 Project 资源副本。
- 用户发布资产必须走后端类型化 mapper / validator，前端不直接拼装可运行 payload。
- 发布动作是显式操作，不做静默同步；已安装 Project 资源通过版本和 digest 感知更新。
- 第一阶段聚焦用户个人发布路径，默认 `scope = user`、`source = user_authored`；system / org 管理工作流留给后续治理任务。

## Requirements

1. 用户可以从 Project Assets 的四类现有资源发布到 Shared Library：
   - Project Agent / ProjectAgentLink -> `agent_template`
   - Project MCP Preset -> `mcp_server_template`
   - Project Workflow + Lifecycle bundle -> `workflow_template`
   - Project SkillAsset + files -> `skill_template`
2. 发布入口必须从 Project 资源出发，由后端读取权威数据并转换为 `LibraryAsset.payload`。
3. 发布请求允许用户填写或修改 `key`、`display_name`、`description`、`version`，并明确是否覆盖同 identity 的已有 LibraryAsset。
4. 发布后的资产在 Marketplace 列表、搜索、详情抽屉中可见，并能被安装到同一或其它 Project。
5. 再次发布同一 `asset_type + scope + owner_id + key` 时：
   - 未选择覆盖则返回明确冲突错误。
   - 选择覆盖则更新版本、digest、payload 和元数据。
6. 发布或覆盖后，已安装该来源的 Project 资源能通过 source-status 显示 `update_available`。
7. MCP 发布必须剥离或拒绝 credential、token、本地私有路径、真实 env secret 等 connection material；公共资产只能保留 template-safe 配置。
8. Workflow 发布必须以 bundle 为单位保持 lifecycle 与 workflow definitions 的一致性，不能发布会导致安装后缺 workflow 或缺 lifecycle 的半成品。
9. 权限要求：
   - 发布来源 Project 资源需要 Project edit 权限。
   - 发布到 user scope 时 owner 使用当前用户身份。
   - 安装仍沿用 Project edit 权限。

## Out of Scope

- org / system scope 的审批、管理员发布与权限治理。
- 远端市场导入、导出文件包、跨实例同步。
- 字段级 diff、三方合并、自动升级已安装 Project 资源。
- 运行中 session 自动热更新。
- 直接编辑 Shared Library raw JSON payload 的通用管理台。

## Acceptance Criteria

- [ ] 用户能把 Project Agent 发布成 `user_authored + user scope` 的 `agent_template`，并从 Marketplace 安装到另一个 Project。
- [ ] 用户能把 Project SkillAsset 发布成 `skill_template`，文件内容与 `disable_model_invocation` 在安装后保持一致。
- [ ] 用户能把 Project MCP Preset 发布成 `mcp_server_template`，发布过程不会携带 credential / secret / local-only connection material。
- [ ] 用户能把 Workflow/Lifecycle bundle 发布成 `workflow_template`，安装后 workflow definitions 与 lifecycle definition 同源且可再次删除/重装。
- [ ] 同 key 发布不覆盖时返回冲突；选择覆盖时更新 `payload_digest` 并使旧安装项 source-status 变为 `update_available`。
- [ ] Marketplace 支持查看用户发布资产，并能按现有 asset type 筛选与搜索。
- [ ] Project Assets 卡片或详情中提供清晰的“发布到资源市场”入口和发布结果反馈。
- [ ] 后端覆盖 mapper、payload validation、MCP 安全校验、source-status 更新的测试。
- [ ] 前端通过 typecheck/test，并覆盖 publish service mapper 或关键交互。

## Open Questions

- 第一版 UI 是否需要在 Marketplace 顶部增加“我发布的”快捷过滤，还是只在现有筛选中加入 scope/source 过滤即可。
