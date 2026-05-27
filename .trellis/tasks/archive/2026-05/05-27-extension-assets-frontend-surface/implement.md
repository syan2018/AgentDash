# Implementation Plan — Extension Assets Frontend Surface

执行顺序按"契约/后端先行 → service/工具 → 面板 → e2e"自底向上，每步独立可验证，便于中途中断。

## Step 1 — Domain repo `delete` + `get_by_project_and_id`

- [ ] `crates/agentdash-domain/src/shared_library/project_extension.rs`：
  - 在 `ProjectExtensionInstallationRepository` trait 增加 `delete(project_id, installation_id) -> bool` 与 `get_by_project_and_id(project_id, installation_id) -> Option<...>`
- [ ] `crates/agentdash-infrastructure/src/persistence/postgres/project_extension_installation_repository.rs`：实现两个新方法
- [ ] 测试：postgres test 覆盖 `delete` happy / cross-project / 不存在；`get_by_project_and_id` happy / cross-project None
- 验证：`cargo test -p agentdash-infrastructure project_extension_installation`

## Step 2 — Application: `uninstall_extension_installation`

- [ ] `crates/agentdash-application/src/extension_runtime.rs`：新增 input/output struct + `uninstall_extension_installation` 函数
- [ ] 单测：成功（拿 extension_key 返回）、404（不存在）、404（cross-project）
- 验证：`cargo test -p agentdash-application extension_runtime::tests::uninstall`

## Step 3 — Contract DTO

- [ ] `crates/agentdash-contracts/src/extension_runtime.rs`：增加 `UninstallExtensionInstallationResponse`
- [ ] 跑 `cargo run -p agentdash-contracts --bin generate_contracts_ts`
- [ ] 校验：`packages/app-web/src/generated/extension-runtime-contracts.ts` 包含新类型
- 验证：`pnpm run contracts:check`（如果项目里没有这个脚本，fallback 到 `cargo run -p agentdash-contracts --bin generate_contracts_ts -- --check`）

## Step 4 — API Route `DELETE /projects/{p}/extensions/{i}`

- [ ] `crates/agentdash-api/src/routes/extension_runtime.rs`：handler
- [ ] `crates/agentdash-api/src/routes.rs`：route 注册
- [ ] 集成测试：200 / 404 / 跨 project 404 / 401 (no auth) / 403 (no edit perm)
- 验证：`cargo test -p agentdash-api extension_runtime`

## Step 5 — Frontend service `extensionPackage.ts`

- [ ] 新建 `packages/app-web/src/services/extensionPackage.ts`
  - `listExtensionArtifacts` / `uploadExtensionArtifact` / `installExtensionArtifact` / `downloadExtensionArtifact`
  - `multipart` 直发；下载用 `fetch → blob`；mapper 风格对齐 `services/extensionRuntime.ts`
- [ ] `services/extensionPackage.test.ts`：list / upload / install mapper + 错误体
- 验证：`pnpm --filter @agentdash/app-web run test -- extensionPackage`

## Step 6 — Frontend service `extensionRuntime.ts` 加 uninstall

- [ ] 加 `uninstallExtensionInstallation(projectId, installationId)`
- [ ] mapper 单测
- 验证：`pnpm --filter @agentdash/app-web run test -- extensionRuntime`

## Step 7 — `utils/sha256.ts`

- [ ] 实现 + 测试（jsdom 下使用 `crypto.subtle`，兜底 mock）
- 验证：`pnpm --filter @agentdash/app-web run test -- sha256`

## Step 8 — Aggregation pure 函数

- [ ] 新建 `features/assets-panel/categories/extension/extensionAggregation.ts` + `.test.ts`
  - 输入 `ExtensionRuntimeProjectionResponse`，输出按 `extension_key` 分组的 `InstalledExtensionRowVM[]`
- 验证：`pnpm --filter @agentdash/app-web run test -- extensionAggregation`

## Step 9 — Dialog 组件

- [ ] `UploadExtensionDialog.tsx`：文件 picker + 校验 + sha256 进度态 + 提交
- [ ] `InstallFromArtifactDialog.tsx`：表单 (`extension_key?`/`display_name?`/`overwrite`) → 调 service
- [ ] `UninstallConfirmDialog.tsx`：二次确认
- 验证：本步以下一步主面板渲染 + e2e 一并验证

## Step 10 — `ExtensionCategoryPanel.tsx` 主面板

- [ ] 拼装 useProjectExtensionRuntime + 本地 artifacts state
- [ ] 已安装段（基于聚合 VM）+ 归档段
- [ ] notice / busy state 与 `MarketplaceCategoryPanel` 风格一致
- [ ] 行级动作：下载 / 安装 / 卸载
- [ ] 单测：渲染态（empty / 仅有归档 / 已安装+归档 / notice）
- 验证：`pnpm --filter @agentdash/app-web run test -- ExtensionCategoryPanel`

## Step 11 — Assets 类目接入

- [ ] `AssetsTabView.tsx` 的 `SHAREABLE_CATEGORIES` 加 `extension` 项
- [ ] 在路由配置（dashboard assets 子路由）注册 `path: "extension"`
- [ ] 手测：登录 → 切到 Assets → 看到 Extension 类目可点
- 验证：`pnpm --filter @agentdash/app-web run typecheck` + 手测

## Step 12 — E2E

- [ ] 新增 `tests/e2e/extension-assets-panel.spec.ts`：
  1. ensureBackend / ensureProject（复用 local-hello e2e 的 helper）
  2. UI 登录 → 跳到 `/dashboard/assets/extension`
  3. 点「上传归档」选 `examples/extensions/local-hello/dist/local-hello.agentdash-extension.tgz`
  4. 提交后弹 install dialog，勾 overwrite，提交
  5. 已安装段出现 `local-hello`
  6. 切到 session 验证 AddTabMenu 含 Local Hello
  7. 回 Assets 卸载，已安装段消失，归档段保留
- 验证：`pnpm exec playwright test tests/e2e/extension-assets-panel.spec.ts`

## Step 13 — 全量验收

- [ ] `cargo test -p agentdash-domain -p agentdash-infrastructure -p agentdash-application -p agentdash-api -p agentdash-contracts`
- [ ] `pnpm --filter @agentdash/app-web run typecheck`
- [ ] `pnpm --filter @agentdash/app-web run test`
- [ ] `pnpm exec playwright test tests/e2e/local-hello-extension.spec.ts tests/e2e/extension-assets-panel.spec.ts`
- [ ] PRD 全部 AC 勾选
- [ ] 调用 trellis-update-spec 评估是否有规范要回写

## Rollback Points

- 任何 step 内 commit 一次后再前进；如需回滚到 step N，用 `git revert` 该 commit。
- Contract 重新生成会同时改 Rust + TS 两个文件；必须同 commit 提交，避免 drift。
- 后端 / 前端独立可发：先发后端 (Step 1–4) 后 deploy 仍能跑老前端；前端发布 (Step 5–11) 不会 break 老后端，仅卸载按钮调用会 405。

## Review Gates

- 完成 Step 4 后停一次：人工抽查 uninstall 端到端调用（curl）确认行为正确，再继续
- 完成 Step 10 后停一次：人工跑前端，看视觉与 `MarketplaceCategoryPanel` 风格一致
- 完成 Step 12 后进入 Phase 3 quality verification

## Out of Scope（明示拒绝）

- 不做归档删除（仅卸载 installation）
- 不做权限授予 dialog（permissions 仅展示）
- 不做 manifest editor
- 不做扩展 invocation trace 列表
