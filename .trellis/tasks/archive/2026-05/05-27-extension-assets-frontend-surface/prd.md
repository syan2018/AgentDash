# Extension Assets Frontend Surface

## Goal

把 packaged Extension 的"上传 / 安装 / 已装管理"补成一条前端可用通路。当前 Extension 的后端 + CLI + e2e 已闭环，但 Web UI 只能消费扩展产生的 workspace tab / runtime action，无法直接上传本地 `.agentdash-extension.tgz`、看不到归档列表、看不到已装扩展清单、也无法卸载。本任务在统一资产中心 (`AssetsTabView`) 新增 `Extension` 类目，把 CLI 能力等价搬到 UI。

## Non-Goals

- 不重做 Marketplace 发布流；该路径上 `extension_template` 已支持 ([MarketplaceCategoryPanel.tsx:49](packages/app-web/src/features/assets-panel/categories/MarketplaceCategoryPanel.tsx#L49))。
- 不调整 Extension Runtime 投影本身的字段/契约。
- 不动 Canvas → Extension promote 已有按钮 ([ProjectCanvasManager.tsx:135](packages/app-web/src/features/canvas-panel/ProjectCanvasManager.tsx#L135))。
- 不做 manifest 编辑器、不做权限授予对话框（permissions 仅展示，不改授权流）。
- 不做扩展运行时调用 Trace 列表（投影里 `runtime_actions` / `bundles` 仅做只读展示）。

## Stakeholders / Inputs

- 后端契约：`crates/agentdash-contracts/src/extension_package.rs` + `extension_runtime.rs`（已生成 `extension-package-contracts.ts`、`extension-runtime-contracts.ts`）。
- 已就绪 routes：`POST/GET /api/projects/{id}/extension-artifacts`、`POST .../{artifact_id}/install`、`GET .../archive`、`GET /api/projects/{id}/extension-runtime`。
- 缺失后端能力：**uninstall**（无 route、无 application service）。本任务范围内补这一条 route + 对应 application 函数 + 前端调用。
- 已有前端复用项：`features/extension-runtime` store / projection mapper / hook。
- UI 框架：`AssetsTabView` 左侧分类、`MarketplaceCategoryPanel` 抽屉/通知/确认对话框模式。

## Requirements

### R1 类目接入（导航 + 路由）

1. `AssetsTabView` 的 `SHAREABLE_CATEGORIES` 新增 `{ segment: "extension", label: "Extension", hint: "本地打包扩展（含已安装与归档）" }`，紧跟 `vfs-mount` 之后。
2. 路由 `/dashboard/assets/extension` 渲染新建的 `ExtensionCategoryPanel`。路由声明位置与现有 categories 一致。
3. 与其他 SHAREABLE 类目同样支持空 project 占位态。

### R2 已安装扩展列表

1. 数据源：`useExtensionRuntimeStore.fetchProject(projectId)` 返回的 `installations[]`。每条 row 关联同 projection 的 `permissions[]` / `workspace_tabs[]` / `runtime_actions[]` / `bundles[]`（按 `extension_key` join）。
2. Row 必显字段：
   - `display_name` + `extension_key`（主标题）
   - `extension_id`（次标题 / monospace）
   - 来源 badge：
     - `installed_source != null && package_artifact == null` → "Marketplace"
     - `installed_source == null && package_artifact != null` → "本地归档"
     - `installed_source != null && package_artifact != null` → "Marketplace（含归档）"
     - 全为 null → "未知"
   - 版本：优先 `package_artifact.package_version`，否则 `installed_source.source_version`
   - 提供能力计数：tabs / actions / commands / flags / message_renderers
3. 可展开详情区（按需展开，不默认全展开）：权限明细、tabs 列表（type_id + renderer.kind）、actions 列表（action_key + kind）、bundle digest。
4. 行级动作：
   - 「下载归档」：仅当 `package_artifact != null`，调用 `GET /extension-artifacts/{artifact_id}/archive`，浏览器另存。
   - 「卸载」：弹二次确认。后端补 `DELETE /api/projects/{project_id}/extensions/{installation_id}`（详细形态见 design.md）；卸载成功后刷新 projection。
5. 加载/错误态对齐 `MarketplaceCategoryPanel` 现有的 `notice` + `loading` 形态。

### R3 归档库（Artifact List）

1. 数据源：新增 `GET /api/projects/{id}/extension-artifacts` 调用，落到新 service `services/extensionPackage.ts`。
2. Row 必显字段：`extension_id`、`package_name`@`package_version`、`asset_version`、`archive_digest`（截断 + 复制）、`byte_size` 友好化、`created_at` 友好化。
3. 行级动作：
   - 「从归档安装」：弹 dialog 收集 `extension_key?`（默认 null = 用 manifest 提供值）、`display_name?`、`overwrite`；POST install 后 success notice + 刷新 installations。
   - 「下载」：与 R2 共用同一封装。
4. 顶部「上传归档」按钮：弹 picker 选 `.tgz`/`.agentdash-extension.tgz`；客户端校验扩展名 + 大小（< 50MB，可后续调整）；`multipart/form-data` 上传到 `/extension-artifacts`，含 `archive_digest`（前端用 `crypto.subtle.digest("SHA-256", ...)` 计算 → `sha256:` 前缀）。上传成功后默认弹「立即安装」对话框（与 R3.3 安装弹窗复用）。
5. 上传失败 / 安装失败时通过 `notice` 展示 server 返回的错误信息。

### R4 后端 Uninstall Route

1. `crates/agentdash-application/src/extension_runtime`（或就近层）新增 `uninstall_extension_installation(repos, input)`，按 `installation_id` 删 `extension_installations` + 级联清理 projection cache。
2. `crates/agentdash-api/src/routes/extension_runtime.rs` 暴露 `DELETE /api/projects/{project_id}/extensions/{installation_id}`，权限 `ProjectPermission::Edit`，404 / 跨 project 404 行为对齐 install route。
3. 契约：在 `agentdash-contracts/src/extension_runtime.rs` 增加 `UninstallExtensionInstallationResponse { installation_id, extension_key }`，重新生成 `extension-runtime-contracts.ts`。
4. 卸载语义：仅删 installation 行，不动 `extension_package_artifacts` 行（归档保留以便重装）。

### R5 投影刷新一致性

1. R3 / R4 的写操作（upload+install / install / uninstall）成功后必须调用 `useExtensionRuntimeStore.fetchProject(projectId)` 强制刷新 projection。
2. 归档列表本地状态由 `ExtensionCategoryPanel` 管，不进 store；写操作后局部 `refresh`。

## Acceptance Criteria

- [ ] AC1：从 Web UI 上传 `examples/extensions/local-hello/dist/local-hello.agentdash-extension.tgz` 后能在「归档」段看到新条目，digest / 版本 / 字节数显示正确。
- [ ] AC2：从「归档」段对该条目执行「从归档安装」并勾选 overwrite，「已安装」段立即出现 `local-hello`，来源 badge = 本地归档；workspace 里 AddTabMenu 中能看到 `Local Hello` tab。
- [ ] AC3：从 Web UI 卸载该 installation，「已安装」段列表立即消失；workspace 里之前打开的 extension tab 不再可被新增（已打开的 tab 行为可保留旧 projection，不强制销毁）；归档段记录仍在。
- [ ] AC4：从「已安装」段下载归档，浏览器拿到的字节流 sha256 == row 上展示的 digest（手测或脚本校验皆可）。
- [ ] AC5：Marketplace 安装的 extension（沿用现有路径）在「已安装」段来源 badge = Marketplace，且没有「下载归档」按钮（除非同时有 package_artifact）。
- [ ] AC6：`pnpm --filter @agentdash/app-web run typecheck` 通过；新增 service / panel 至少配 1 个 vitest 单测覆盖 mapper + 1 个组件渲染态测试。
- [ ] AC7：后端 `cargo test -p agentdash-api -p agentdash-application` 通过，新增 uninstall route + application 函数有单测覆盖（含 cross-project 404、未授权、删除后 projection 不再出现该 installation）。
- [ ] AC8：契约重新生成且 `pnpm run contracts:check`（或等价命令）通过；`extension-runtime-contracts.ts` 含新 `UninstallExtensionInstallationResponse`。
- [ ] AC9：tests/e2e 新增（或扩展现有）一条用例：UI 走完「上传 tgz → 安装 → 卸载」全程，验证 projection 同步。

## Constraints / Risks

- 扩展归档可能很大：上传时需要前端在内存里读字节计算 sha256，50MB 阈值兜底；超阈值给清晰错误而不是后端 reject。
- 归档下载走 `Authorization: Bearer` 头，浏览器无法直接 `<a download>`；需用 `fetch → blob → URL.createObjectURL` 触发下载。
- 卸载是破坏性操作但不删归档；二次确认文案要区别于"删除归档"，避免用户误以为归档也丢。
- Marketplace 路径上的 installation 同样会落到投影 `installations[]`，UI 必须区分来源，避免给 Marketplace 装的 extension 错误地暴露「卸载本地归档」语义（但卸载行为本身仍允许，等价于断开和 marketplace 资产的关联）。
- ExtensionRuntimeProjection 当前没有 invalidate 机制；R5 用 `fetchProject` 即可，本任务不引入 stream / SSE。
