# Plugin Extension Asset 化 — Implement

## Checklist

- [x] 阅读并遵守相关规范与任务：
  - `.trellis/spec/backend/capability/plugin-api.md`
  - `.trellis/spec/backend/shared-library.md`
  - `.trellis/spec/cross-layer/shared-library-contract.md`
  - `.trellis/tasks/04-12-plugin-extension-api/prd.md`
  - `.trellis/tasks/04-12-plugin-extension-api/dynamic-installation-discussion.md`
- [x] 扩展 shared library domain：
  - `LibraryAssetType::ExtensionTemplate`
  - `LibraryAssetSource::PluginEmbedded`
  - `ExtensionTemplatePayload`
  - typed validator tests
- [x] 新增 DB migration：
  - 更新 `library_assets.asset_type` check
  - 更新 `library_assets.source` check
  - 新建 `project_extension_installations`
- [x] 扩展 plugin API：
  - `PluginLibraryAssetSeed`
  - `AgentDashPlugin::library_asset_seeds`
  - contract crate 依赖保持轻量
- [x] 扩展 plugin registration：
  - collect plugin seeds
  - fail-fast conflict check
  - 将 plugin seeds 接入 Shared Library seed/upsert 流程
- [x] first-party plugin 增加示例 embedded asset。
- [x] Shared Library install 支持 `extension_template`：
  - application install 分支
  - Project extension repository
  - API response variant
  - source-status 增加 extension_installations
- [x] session construction 投影：
  - 读取 enabled Project extension installations
  - 将 slash commands / runtime flags / message renderers 作为只读 metadata 投影到 `SessionConstructionPlan`
  - 本任务不注册前端 `/` 菜单、不执行 command handler、不写入 hook flag state；这些属于 `04-12-plugin-extension-api` 后续运行时接线
- [x] 前端类型和 service：
  - `LibraryAssetType` 增加 `extension_template`
  - `LibraryAssetSource` 增加 `plugin_embedded`
  - install response 增加 extension variant
  - source-status DTO 增加 extension_installations
- [x] Marketplace UI：
  - Extension filter chip
  - Extension drawer body
  - plugin embedded source badge
- [x] Project UI：
  - Marketplace drawer/card 已能展示 Extension asset、plugin source badge 与安装状态
  - Project 内启用/禁用管理入口后续单独补齐；当前安装态默认 `enabled = true`

## Validation

- [x] `cargo fmt --all`
- [x] `cargo test -p agentdash-domain shared_library`
- [x] `cargo test -p agentdash-api plugins`
- [x] `cargo test -p agentdash-first-party-plugins`
- [x] `cargo test -p agentdash-application shared_library`
- [x] `cargo test -p agentdash-application session`
- [x] `pnpm --filter app-web typecheck`
- [x] `pnpm --filter app-web test`
- [ ] 手工验证：first-party plugin seed -> Marketplace -> install extension -> 新 session construction projection 可见 command/flag metadata。

## Deferred Follow-up

- Project Extension 管理 UI：列出已安装 extension，支持 enabled/disabled 切换与配置查看。
- Runtime/UI 接线：从 `SessionConstructionPlan.projections.extension_runtime` 读取 command/flag/renderer metadata，注册 `/` 菜单、执行 `inject_message` handler，并向 Hook/Rhai 暴露 flag store。

## Risk Points

- 不要让 plugin seed 绕过 LibraryAsset typed validator。
- 不要把 native plugin 的重启边界伪装成用户热加载。
- `extension_template` payload 要避免变成无约束万能 JSONB。
- 第一版 runtime projection 只承诺新 session 生效，避免运行中热更新扩大范围。
