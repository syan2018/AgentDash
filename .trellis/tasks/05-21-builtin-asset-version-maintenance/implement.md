# Builtin 资产版本维护与升级治理实施计划

## 前置检查

- [ ] 确认 Lifecycle Activity 重构相关 PR 已完成或当前分支包含 Shared Library workflow/lifecycle normalize repair。
- [ ] 确认本任务目标分支基于 `task.json` 的 `base_branch = codex/lifecycle-activity-executor-redesign` 或后续合并后的主线。
- [ ] 运行基线检查：
  - `cargo test -p agentdash-application shared_library`
  - `cargo test -p agentdash-domain shared_library`
  - `pnpm --filter app-web typecheck`

## 实施步骤

1. Seed version registry
   - [x] 新增 builtin asset version typed registry。
   - [x] 将 `builtin_library_seeds()` 从单一 `BUILTIN_VERSION` 改为按 `asset_type + key` 查询 version。
   - [x] builtin `source_ref` 改为 `builtin:{asset_type}:{key}`。
   - [x] registry 缺失 version 时返回明确 `DomainError::InvalidConfig`。

2. Canonical digest snapshot
   - [x] 新增 seed manifest 覆盖测试，包含 source_ref / asset_type / key / version / payload_digest 约束。
   - [x] 增加 builtin seed registry 覆盖测试。
   - [x] 增加 first-party plugin embedded seed snapshot 测试。
   - [x] 覆盖 digest 变化但 version 未提升的失败用例。
   - [x] 覆盖 version 提升但 digest 未变化的失败用例。

3. Plugin embedded seed governance
   - [x] 在 `seed_plugin_embedded_assets` 中比较既有 LibraryAsset 与新 seed 的 version/digest。
   - [x] 同一 source_ref digest 变化且 version 未提升时 fail-fast。
   - [x] 保留不同 plugin/source 占用同一 identity 的冲突检查。
   - [x] 补充 first-party plugin 测试。

4. Source status model
   - [x] 保持 `SharedLibrarySourceStatus = up_to_date | update_available | source_missing` 三态，不新增用户可见维护错误状态。
   - [x] 在 seed/startup 校验中使用 semver 比较 current/installed source version。
   - [x] payload digest changed without version bump 时返回明确 `DomainError::InvalidConfig` 并阻止启动或 seed repair 完成。
   - [x] source-status 查询只处理已满足 seed invariant 的 LibraryAsset。
   - [x] 更新后端测试，确认维护错误被 seed/startup 拦截而不是进入 DTO。

5. Install / overwrite transaction consistency
   - [x] 梳理 ProjectAgent / MCP preset / SkillAsset / VFS mount / workflow bundle / extension installation overwrite 路径。
   - [x] 确认每条 overwrite 路径都从 `LibraryAsset` 当前 version/digest/source_ref 构造 installed source。
   - [x] 为 workflow bundle 覆盖更新补充事务测试：workflow definitions 与 activity lifecycle definition 同时成功或同时保持旧值。
   - [ ] 为重复安装相同 asset 补充测试，确认不会产生虚假版本推进。

6. Migration / startup repair
   - [x] 增加 migration 或 startup repair，将 builtin LibraryAsset `source_ref` 收敛为 `builtin:{asset_type}:{key}`。
   - [x] 修正 builtin/plugin_embedded LibraryAsset canonical digest。
   - [x] 对可匹配 LibraryAsset 的 Project installed source 修正 source_ref。
   - [x] registry 中删除的 builtin LibraryAsset 标记 deprecated。

7. Frontend Marketplace
   - [x] 保持 `SharedLibrarySourceStatus` 三态类型。
   - [x] 确认 sharedLibrary mapper 不需要新增维护错误分支。
   - [x] 确认 Marketplace summary priority 仍为 `source_missing > update_available > up_to_date`。
   - [ ] 补充或保留前端测试覆盖三态聚合与正常按钮状态。

8. Spec update
   - [x] 更新 `.trellis/spec/backend/shared-library.md`。
   - [x] 更新 `.trellis/spec/cross-layer/shared-library-contract.md`。
   - [x] 更新 `.trellis/spec/backend/capability/plugin-api.md`。

## 验证命令

- [x] `cargo test -p agentdash-domain shared_library`
- [x] `cargo test -p agentdash-application shared_library`
- [x] `cargo test -p agentdash-api shared_library`
- [x] `cargo test -p agentdash-first-party-plugins`
- [x] `pnpm --filter app-web typecheck`
- [x] `pnpm --filter app-web test`
- [ ] `pnpm dev`
- [ ] 浏览器验证 Marketplace：
  - builtin asset 首装显示正常；
  - 修改 payload 并 bump version 后显示可更新；
  - 修改 payload 但不 bump version 时测试或启动期断言失败；
  - 前端不展示平台维护错误状态。

## 风险文件

- `crates/agentdash-application/src/shared_library/seed.rs`
- `crates/agentdash-application/src/shared_library/service.rs`
- `crates/agentdash-application/src/shared_library/install.rs`
- `crates/agentdash-domain/src/shared_library/value_objects.rs`
- `crates/agentdash-api/src/dto/shared_library.rs`
- `crates/agentdash-api/src/routes/shared_library.rs`
- `crates/agentdash-first-party-plugins/src/lib.rs`
- `packages/app-web/src/types/shared-library.ts`
- `packages/app-web/src/services/sharedLibrary.ts`
- `packages/app-web/src/features/assets-panel/categories/MarketplaceCategoryPanel.tsx`
- `packages/app-web/src/features/assets-panel/categories/MarketplaceAssetDrawer.tsx`

## Review Gate Before `task.py start`

- [x] 用户确认版本维护错误不返回给前端，而是在启动/seed 阶段 assert/fail-fast。
- [ ] 用户确认 version 比较采用 semver，非 semver 版本视为 seed 维护错误。
- [ ] 用户确认 snapshot 文件/测试方案接受维护者更新 golden snapshot。
- [x] 用户确认本任务不为维护错误新增 frontend Marketplace 展示与按钮禁用状态。
