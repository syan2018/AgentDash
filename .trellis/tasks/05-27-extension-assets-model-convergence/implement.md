# Implement - Extension 资产模型与 Assets 页面收敛

## Preconditions

- 当前任务保持 `planning` 状态，用户确认本规划后再运行 `task.py start`。
- 开始实现前先检查 `git status --short`，如有非本任务改动，必须识别边界并避免覆盖。
- 实现前使用 `trellis-before-dev` 读取 cross-layer/backend/frontend 相关 spec。
- 本任务按单个完整 task 推进，通过 Phase 1-6 做阶段性落地和验证，不拆 child tasks。

## Phase 1 - Domain And Migration

- [x] 设计并实现 extension package artifact ownership schema。
  - 推荐：`owner_kind = project | library_asset`、`owner_id`。
  - 新增 migration，回填现有 `project_id` 到 owner 字段。
  - 调整唯一索引和查询索引。
- [x] 更新 domain model `ExtensionPackageArtifact` 与 repository trait。
  - 支持按 owner 查询、按 owner+digest 幂等创建、按 project installation 可访问性读取。
  - 保持 `ExtensionPackageArtifactRef` 作为 installation runtime 引用。
- [x] 增加 helper 判断 `ExtensionTemplatePayload` 是否需要 package artifact。
  - runtime_actions/protocol_channels/workspace_tabs/bundles 任一非空则需要。
  - 为 declaration-only 模式添加单测。
- [x] 更新 Postgres repository 和现有 tests。

## Phase 2 - Application Semantics

- [x] 重构 package artifact store/read use case。
  - 支持 Project-owned artifact。
  - 支持 LibraryAsset-owned artifact。
  - 本机下载读取时验证当前 Project installation 对 artifact 的访问权。
- [x] 重构 Shared Library install。
  - `extension_template` install 读取 LibraryAsset-owned artifact。
  - executable Extension 缺 artifact 时 fail-fast。
  - Marketplace packaged install 创建 `installed_source + package_artifact` installation。
- [x] 实现 Project Extension publish。
  - `ProjectAssetPublishKind::ExtensionInstallation`。
  - 从 Project installation 构造 `LibraryAsset(extension_template)`。
  - 复制或链接 package artifact 到 LibraryAsset owner。
  - executable Extension 没有 package artifact 时拒绝发布。
- [x] 实现 Project Extension management use case。
  - 列表包含 source_status、package mode、artifact summary、capability summary。
  - 管理 API 不依赖 runtime projection。
- [x] 保持 uninstall 语义并接入新 management list 刷新路径。

## Phase 3 - API And Contracts

- [x] 更新 `agentdash-contracts`。
  - shared-library publish kind。
  - extension management DTO。
  - import package response DTO。
  - artifact owner DTO 如需要。
- [x] 重新生成 TypeScript contracts。
  - `cargo run -p agentdash-contracts --bin generate_contracts_ts`
- [x] 更新 API routes。
  - `GET /projects/{project_id}/extensions`
  - `POST /projects/{project_id}/extensions/import-package`
  - Shared Library publish/install route 内部接入新 use case。
  - Artifact download route 按 Project installation access 校验。
- [x] 更新 `packages/extension-dev` CLI install flow 如 API 发生变化。

## Phase 4 - Frontend Services And State

- [x] 新增 Project Extension management service + mapper。
  - 不复用 `fetchProjectExtensionRuntime` 作为管理数据源。
  - mapper 从 `unknown` 转 typed DTO，遵守现有 type-safety 规范。
- [x] 调整 sharedLibrary service/types。
  - `PublishLibraryAssetKind` 增加 `extension_installation`。
  - 更新 `kindToAssetType` 与发布标题。
- [x] 调整 package import service。
  - 将前端主入口从 artifact list 改成 import/install intent。
  - 下载包动作仅对 installed package artifact 暴露。

## Phase 5 - Frontend UI

- [x] 重写 `ExtensionCategoryPanel`。
  - 移除“归档库”section。
  - 使用 Project extension management API。
  - 参考关联Tab使用公用标准卡片、来源徽标、安装状态、发布状态、详情抽屉、确认弹窗。
  - 保留“从本地包安装”入口。
- [x] 重写/改名 Upload/Install dialogs。
  - `InstallExtensionPackageDialog`: file select -> sha256 -> import/install -> refresh list。
  - 展示 manifest/package metadata 和 overwrite 选项。
- [x] Marketplace Extension drawer 增强。
  - package metadata、artifact availability、runtime actions、protocol channels、workspace tabs、permissions、bundles。
  - package 缺失时显示 disabled reason。
- [x] Asset publish picker 增加 Extension。
  - 数据源为 management API。
  - 可发布项与 package mode 明确展示。

## Phase 6 - Tests

- [x] Rust domain tests。
  - `ExtensionTemplatePayload::requires_package_artifact`。
  - artifact owner validation。
- [x] Rust application tests。
  - Marketplace packaged Extension install 写入 `installed_source + package_artifact`。
  - executable template 缺 artifact install fails。
  - publish Project Extension creates LibraryAsset + Library-owned artifact。
  - source-status update_available 对 Extension 生效。
- [x] API tests。
  - Project extension management list。
  - import package install。
  - artifact download access。
- [x] Frontend unit tests。
  - service mappers。
  - ExtensionCategoryPanel empty/local/marketplace/update states。
  - Marketplace drawer package missing/available states。
  - Publish picker includes Extension.
- [ ] E2E/manual verification。
  - local package install -> Project Extension appears -> Workspace tab/action available.
  - publish to Marketplace -> install in another Project -> source-status update.

## Validation Commands

Run the narrowest useful commands first, then broaden before finish:

```powershell
cargo test -p agentdash-domain extension_template
cargo test -p agentdash-application extension
cargo test -p agentdash-api extension
cargo run -p agentdash-contracts --bin generate_contracts_ts
pnpm --filter app-web run typecheck
pnpm --filter app-web run test -- extension
pnpm --filter @agentdash/extension-dev run test
```

If route/contract changes are broad, also run:

```powershell
cargo test -p agentdash-api
pnpm run contracts:check
```

## Validation Evidence

- `cargo fmt`
- `cargo check -p agentdash-api`
- `cargo test -p agentdash-domain extension_template`
- `cargo test -p agentdash-application extension_package`
- `cargo test -p agentdash-application shared_library`
- `cargo test -p agentdash-api shared_library --no-run`
- `cargo test -p agentdash-infrastructure extension_package_artifact --no-run`
- `cargo run -p agentdash-contracts --bin generate_contracts_ts`
- `cargo run -p agentdash-contracts --bin generate_contracts_ts -- --check`
- `pnpm --filter app-web run typecheck`
- `pnpm --filter app-web run lint`
- `pnpm --filter app-web exec vitest run src/services/extensionPackage.test.ts src/services/extensionManagement.test.ts src/services/sharedLibrary.test.ts src/features/assets-panel/categories/extension/ExtensionCategoryPanel.test.tsx`
- `pnpm --filter @agentdash/extension-dev run test`
- `pnpm --filter @agentdash/extension-dev run typecheck`

未执行完整 `pnpm dev` 手工链路；AC1/AC4/AC5 仍保留给后续带真实 Project/Backend/Workspace 的端到端验证。

## Follow-up Validation Evidence

- `cargo fmt`
- `cargo test -p agentdash-domain extension_template`
- `cargo test -p agentdash-application extension_package_source_validation_uses_artifact_identity`
- `cargo test -p agentdash-application shared_library`
- `cargo check -p agentdash-api`
- `pnpm --filter app-web run typecheck`
- `pnpm --filter app-web run lint`
- `pnpm --filter app-web exec vitest run src/features/assets-panel/categories/extension/ExtensionCategoryPanel.test.tsx src/services/extensionManagement.test.ts src/services/sharedLibrary.test.ts`
- `git diff --check`

## Risky Files And Rollback Points

- `crates/agentdash-infrastructure/migrations/*`
  - Rollback point: migration compiles and repository tests pass before frontend changes.
- `crates/agentdash-domain/src/extension_package.rs`
- `crates/agentdash-domain/src/shared_library/project_extension.rs`
- `crates/agentdash-application/src/extension_package.rs`
- `crates/agentdash-application/src/shared_library/install.rs`
- `crates/agentdash-application/src/shared_library/publish.rs`
- `crates/agentdash-api/src/routes/extension_package_artifacts.rs`
- `crates/agentdash-api/src/routes/extension_runtime.rs`
- `packages/app-web/src/features/assets-panel/categories/ExtensionCategoryPanel.tsx`
- `packages/app-web/src/features/assets-panel/categories/MarketplaceAssetDrawer.tsx`
- `packages/app-web/src/features/assets-panel/publish/*`
- `packages/extension-dev/src/*`

Keep backend model/API changes verifiable before deleting old Extension UI pieces. Once management API is stable, remove the archive-library UI in one focused frontend step.

## Review Gate Before Start

- [x] User confirms artifact ownership design: unified `owner_kind = project | library_asset` model.
- [x] User prefers single-task implementation with phase-based execution.
- [x] Working tree state is checked immediately before implementation starts.
- [x] `prd.md`, `design.md`, and `implement.md` are reviewed.
