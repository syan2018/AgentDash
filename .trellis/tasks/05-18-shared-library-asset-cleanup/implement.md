# 清理平台通用配置资产旧入口与安装一致性实施计划

## Batch 1: Agent 项目资源化与版本来源

- [ ] 给 `ProjectAgentLink` 增加 `installed_source` 领域字段、DTO、Postgres 初始化列、读写 mapper。
- [ ] 调整 Shared Library AgentTemplate 安装：创建项目私有 Agent + link，并把 `InstalledAssetSource` 写到 link。
- [ ] `list_project_asset_source_status` 增加 `project_agents`，前端类型和 Marketplace 状态聚合同步更新。
- [ ] 删除/收束用户可见“关联已有 Agent”入口；Project Agent 创建只创建项目内 Agent。
- [ ] 调整 Project Agent 编辑保存，避免把它表达成全局 Agent 复用管理。
- [ ] 补 Agent 安装来源状态的 Rust 单测或 API route 单测。

## Batch 2: 旧 builtin/bootstrap 通道清理

- [ ] 前端移除 MCP Preset 旧装载按钮与 `bootstrapMcpPresets` 调用。
- [ ] 前端移除 Skill 工作区内嵌 bootstrap、builtin reset 入口与对应 service 调用。
- [ ] 前端移除 Workflow template 列表/bootstrap 注册入口与 store/service 方法。
- [ ] 后端删除旧 MCP/Skill/Workflow bootstrap/reset route 注册和处理函数，清理不再使用的 DTO/类型。
- [ ] 保留 Shared Library `seed-builtin`，确认 seed registry 覆盖 Agent/MCP/Workflow/Skill。

## Batch 3: 来源展示与 Workflow 删除闭环

- [ ] MCP/Skill/Workflow/Agent 卡片 badge 优先展示 Marketplace 安装来源与版本状态，而不是只显示 user/builtin_seed。
- [ ] Workflow lifecycle 删除改为清理同一 installed source 的 workflow definitions；如果有其它 lifecycle 仍引用则明确报错。
- [ ] Marketplace 重装/覆盖路径验证：删除 Workflow 后可重新安装同一 LibraryAsset。
- [ ] 更新前端 mappers/types，确保 `InstalledAssetSource` 不做旧字段兼容。

## Batch 4: Review Loop 与提交

- [ ] `rg` 检查旧入口：`bootstrapMcpPresets`、`bootstrapSkillAssets`、`resetSkillAssetFromBuiltin`、`bootstrapWorkflowTemplate`、`LinkExistingAgentDialog`、旧 route path。
- [ ] 运行 Rust format/check/test。
- [ ] 运行前端 typecheck 与相关测试。
- [ ] 人工 review 用户路径：Marketplace seed/install/update，Project MCP/Skill/Workflow/Agent 展示，Workflow delete/reinstall。
- [ ] 按批次提交：
  - `feat(agent): 收束项目 Agent 安装来源`
  - `refactor(marketplace): 清理旧内置资产入口`
  - `fix(workflow): 清理市场安装资源删除残留`
  - 如有文档/spec 更新单独提交。

## Validation Commands

```powershell
cargo fmt --all --check
cargo check -p agentdash-domain -p agentdash-infrastructure -p agentdash-application -p agentdash-api
cargo test -p agentdash-domain shared_library --lib
cargo test -p agentdash-api shared_library --lib
cargo test -p agentdash-api project_agents --lib
cargo test -p agentdash-api workflows --lib
pnpm --filter app-web typecheck
pnpm --filter app-web test
```

## Review Gates

- 每完成一个 batch 先看 `git diff --stat` 和关键词搜索，确认没有只加不删。
- 若发现旧路径仍在用户可见 UI 或 route 注册中，回到对应 batch 清理。
- 若测试暴露 Agent 表无法立即移除，不以兼容方案兜底；保持运行时表存在，但从产品语义和用户路径上完成 Project Agent 收束。
