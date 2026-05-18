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
- [ ] session construction 投影：
  - 读取 enabled Project extension installations
  - 注册 slash commands
  - 注入 runtime flag defaults
  - 暴露 extension message renderer metadata
- [x] 前端类型和 service：
  - `LibraryAssetType` 增加 `extension_template`
  - `LibraryAssetSource` 增加 `plugin_embedded`
  - install response 增加 extension variant
  - source-status DTO 增加 extension_installations
- [x] Marketplace UI：
  - Extension filter chip
  - Extension drawer body
  - plugin embedded source badge
- [ ] Project UI：
  - 已安装 extension 的启用/禁用入口
  - 最小可行可先放在 Project Assets 或 Marketplace drawer 的安装状态区

## Validation

- [x] `cargo fmt --all`
- [x] `cargo test -p agentdash-domain shared_library`
- [x] `cargo test -p agentdash-api plugins`
- [x] `cargo test -p agentdash-first-party-plugins`
- [x] `cargo test -p agentdash-application shared_library`
- [ ] `cargo test -p agentdash-application session`
- [x] `pnpm --filter app-web typecheck`
- [x] `pnpm --filter app-web test`
- [ ] 手工验证：first-party plugin seed -> Marketplace -> install extension -> 新 session 可见 command/flag。

## Risk Points

- 不要让 plugin seed 绕过 LibraryAsset typed validator。
- 不要把 native plugin 的重启边界伪装成用户热加载。
- `extension_template` payload 要避免变成无约束万能 JSONB。
- 第一版 runtime projection 只承诺新 session 生效，避免运行中热更新扩大范围。
