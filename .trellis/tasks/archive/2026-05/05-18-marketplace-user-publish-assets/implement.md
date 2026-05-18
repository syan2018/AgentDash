# Marketplace 用户发布配置资产 — Implement

## Checklist

- [x] 阅读并遵守 Shared Library / Marketplace 现有 spec：
  - `.trellis/spec/backend/shared-library.md`
  - `.trellis/spec/cross-layer/shared-library-contract.md`
  - `.trellis/spec/backend/capability/plugin-api.md`
- [x] 后端新增 publish DTO：
  - `PublishLibraryAssetRequest`
  - `PublishedLibraryAssetResponse` 直接复用 `LibraryAssetResponse`
  - `ProjectAssetKind` parser
- [x] 后端新增路由：
  - `POST /api/projects/{project_id}/shared-library/publish`
  - 挂载到 `crates/agentdash-api/src/routes.rs`
  - 校验 Project edit 权限
- [x] application 层新增 `shared_library/publish.rs`：
  - `PublishLibraryAssetInput`
  - `publish_project_asset_to_library`
  - create / update / conflict 处理
- [x] 新增 mapper：
  - Project Agent -> `AgentTemplatePayload`
  - MCP Preset -> `McpServerTemplatePayload`
  - Workflow/Lifecycle bundle -> `WorkflowTemplatePayload`
  - SkillAsset -> `SkillTemplatePayload`
- [x] 新增 MCP publish sanitizer：
  - 拒绝 credential / token / secret / local-only unsafe config
  - 错误信息包含不可发布字段
- [x] 复用或抽取 digest helper，避免 seed 和 publish 各自实现 sha256 规则。
- [x] 前端类型与 service：
  - `PublishLibraryAssetRequest`
  - `publishLibraryAsset(projectId, input)`
  - source/scope 过滤类型如需 UI 使用则同步补齐
- [x] 前端 Project Assets 四类入口：
  - Agent
  - MCP Preset
  - Workflow/Lifecycle
  - SkillAsset
- [x] 前端发布弹窗：
  - key / display name / description / version
  - 冲突或用户选择时支持 overwrite
  - 成功后提示并可跳转或刷新 Marketplace
- [x] Marketplace 过滤与展示：
  - 可识别 user-authored asset
  - 保持现有 install / source-status 行为
  - 本轮复用现有 Marketplace list/install/source-status；未额外新增“我发布的”快捷筛选

## Validation

- [x] `cargo fmt --all`
- [x] `cargo test -p agentdash-domain shared_library`
- [x] `cargo test -p agentdash-application shared_library`
- [x] `cargo test -p agentdash-api shared_library`
- [x] `pnpm --filter app-web typecheck`
- [x] `pnpm --filter app-web test`
- [ ] 手工验证：发布 Agent/Skill/MCP/Workflow，Marketplace 安装到另一个 Project。

## Risk Points

- MCP template 与 connection material 边界必须保守。
- Workflow bundle 必须避免只发布 lifecycle 或只发布 workflow definitions。
- 覆盖发布必须保留 asset id，否则旧安装项 source-status 无法正确关联。
- 前端不要引入 raw JSON payload 编辑器作为发布捷径。
