# Design - Extension 资产模型与 Assets 页面收敛

## Architecture Decision

目标模型沿用 Shared Library / Marketplace / Project Asset 三层结构：

```text
Shared Library
  LibraryAsset(asset_type = extension_template)
  ExtensionPackageArtifact(owner = library_asset)

Project
  ProjectExtensionInstallation
    installed_source?   -> LibraryAsset version/digest/source_ref
    package_artifact?   -> executable package artifact ref

Runtime
  ExtensionRuntimeProjection  <- enabled ProjectExtensionInstallation
  Local host activation       <- package_artifact archive + digest
```

`ProjectExtensionInstallation` 是 Project Extension 的管理事实源。`ExtensionRuntimeProjection` 是运行时派生视图，只供 WorkspacePanel、Gateway admission、local runner trace 和 webview/canvas panel 使用。`ExtensionPackageArtifact` 是包工件事实源，不是业务资产类目。

## Data Model

### Extension Package Artifact Ownership

当前 `extension_package_artifacts` 使用 `project_id` 作为硬归属。重构采用统一 owner 模型：

```text
extension_package_artifacts
  id
  owner_kind = project | library_asset
  owner_id
  extension_id
  package_name
  package_version
  asset_version
  source_version
  storage_ref
  archive_digest
  manifest_digest
  manifest
  byte_size
  created_at
  updated_at
```

迁移策略：

- 现有 `project_id` 行迁移为 `owner_kind = project`、`owner_id = project_id`。
- 新发布到 Marketplace 的 package artifact 使用 `owner_kind = library_asset`、`owner_id = library_asset.id`。
- repository 提供 `list_by_project_owner`、`get_accessible_for_project_installation`、`get_by_owner_and_digest` 等窄方法，避免 route 自行拼权限逻辑。

采用统一 owner 模型的原因是 package artifact 在 Project 本地导入、LibraryAsset 发布模板、Marketplace 安装后的运行态中共享同一组不变量：manifest digest、archive digest、storage ref、package metadata、runtime access validation。把所有权表达为数据维度，可以让 repository、runtime gateway、publish/install 流程复用同一套完整性校验和访问判定。

### Project Installation Source

`ProjectExtensionInstallation` 保持：

- `installed_source: Option<InstalledAssetSource>`
- `package_artifact: Option<ExtensionPackageArtifactRef>`
- `manifest: ExtensionTemplatePayload`

Marketplace packaged install 后写入二者：

```text
installed_source = LibraryAsset identity/version/digest
package_artifact = Project-accessible artifact ref copied/linked from LibraryAsset artifact
```

本地包导入后：

```text
installed_source = None
package_artifact = Project-owned artifact ref
```

声明型 ExtensionTemplate：

```text
installed_source = Some(...)
package_artifact = None
```

声明型判断由后端集中实现，例如：

```text
requires_package_artifact =
  bundles 非空
  OR runtime_actions 非空
  OR protocol_channels 非空
  OR workspace_tabs 非空
```

`commands`、`flags`、`message_renderers`、`capability_directives`、`asset_refs` 本身不要求 artifact。

## Application Flow

### Marketplace Install

```text
POST /projects/:project_id/shared-library/install
  -> load LibraryAsset(extension_template)
  -> validate payload
  -> if requires package:
       load LibraryAsset-owned package artifact
       create or link Project-accessible package artifact ref
  -> upsert ProjectExtensionInstallation
       installed_source = source from LibraryAsset
       package_artifact = copied/linked package ref when required
  -> return InstallLibraryAssetResponse::ExtensionInstallation
```

`source-status` 不读取 package artifact；它继续比较 `installed_source` 与 LibraryAsset 当前 version/digest。包完整性属于 install/publish/runtime validation。

### Local Package Install

推荐将 UI-facing flow 收敛为单个 import/install intent：

```text
POST /projects/:project_id/extensions/import-package
  multipart archive + expected_archive_digest + install options
  -> validate archive
  -> store Project-owned package artifact
  -> create/update ProjectExtensionInstallation(package_artifact)
```

现有 `POST /extension-artifacts` 和 `POST /extension-artifacts/:artifact_id/install` 可在 CLI 层过渡性复用，但前端不再以列表形式暴露 project artifact inventory。若实现中选择直接替换 CLI endpoint，需要同步更新 `packages/extension-dev` install 流程和 tests。

### Publish Extension

```text
POST /projects/:project_id/shared-library/publish
  asset_kind = extension_installation
  project_asset_id = installation_id
  -> load ProjectExtensionInstallation
  -> validate project ownership
  -> validate package requirement:
       executable Extension 必须有 package_artifact
  -> create/update LibraryAsset(extension_template)
  -> copy/link Project package artifact as LibraryAsset-owned artifact
  -> return LibraryAssetDto
```

发布请求仍禁止前端传 raw payload。后端从 Project installation 读取 manifest 和 package metadata，保证 Marketplace payload 与 package artifact 同源。

### Project Extension Management

新增 management service/use case，例如：

```text
GET /projects/:project_id/extensions
```

返回：

- installation id/key/display_name/extension_id/enabled
- installed_source
- source_status/current source metadata
- package_artifact summary
- package mode: `packaged` / `declaration_only` / `invalid_missing_artifact`
- capability summary: commands、flags、runtime_actions、protocol_channels、workspace_tabs、permissions、bundles
- publish summary: 当前用户是否已发布同 key/version

`GET /projects/:project_id/extension-runtime` 保持运行时 projection，不承担管理 UI 的数据聚合。

## API And Contracts

需要更新：

- `agentdash-contracts/src/shared_library.rs`
  - `PublishLibraryAssetKind::ExtensionInstallation`
  - `InstallLibraryAssetResponse::ExtensionInstallation` 如已有字段不足则扩展
- 新增 `agentdash-contracts/src/extension_management.rs` 或扩展 `extension_runtime.rs`
  - Project extension management list DTO
  - import package response DTO
- `extension_package` contracts
  - owner-aware artifact response
  - 若保留 artifact list，仅标注为 project package inventory，不在 Assets 主 UI 使用
- 生成 TypeScript contracts，并更新 app-web service mapper。

## Frontend Design

### Assets Extension Category

替换当前两段式 UI：

```text
Header
  + Extension
  刷新
  从本地包安装

Grid / List of Project Extension assets
  Card:
    display_name
    extension_key / extension_id / package version
    OriginBadge: Marketplace / Local package / Declaration
    InstallStatusChip: up_to_date / update_available / source_missing
    PublishedBadge when current user has published same key
    capability chips
    CardMenu: 详情 / 发布到资源市场 / 更新发布 / 下载包 / 卸载

DetailPanel
  source
  package metadata
  runtime actions
  protocol channels
  workspace tabs
  permissions
  bundle digest
  raw manifest collapse
```

`UploadExtensionDialog` 改名或重做为 `InstallExtensionPackageDialog`，流程是“选择本地包 -> hash -> upload/import -> install”。不再自动打开“归档库”安装，因为 artifact 本身不是用户资产。

### Marketplace

Marketplace Extension drawer 增强：

- package name/version/asset_version
- requires package artifact 状态
- runtime actions/protocol channels/workspace tabs/permissions/bundles
- install/update disabled reason when package missing

Marketplace install/update 仍走标准 `installLibraryAsset`。

### Shared Publish UI

`AssetPickerDrawer` 新增 Extension 类型，数据源为 Project extension management API。只展示可发布项：

- 本地 packaged installation 可发布。
- Marketplace installed item 可更新发布到当前用户空间时，后端按覆盖策略处理。
- Declaration-only Extension 可发布，但前端明确展示 package mode。

`PublishLibraryAssetDialog` 增加 Extension 标题和 `kindToAssetType` 映射。

## Migration Notes

项目未上线，可以采用直接 schema 收敛：

1. 新增 artifact owner 字段。
2. 回填现有 project artifacts。
3. 将旧 `project_id` 约束改为 owner 约束。
4. 保留或替换索引：
   - `(owner_kind, owner_id, archive_digest)` unique
   - `(owner_kind, owner_id, extension_id)`
5. 对 Project installation 已有 package columns 保持可读；后续 repository 按 `ExtensionPackageArtifactRef` 输出。
6. 对已有 `installed_source` only 且 requires package 的 installation 标记为 invalid diagnostic 或在迁移中补 artifact；当前没有生产数据，优先 fail-fast 暴露不完整测试数据。

## Runtime And Security

- Local runtime 下载 artifact 仍必须通过 project/backend access 校验。
- 如果 artifact owner 是 `library_asset`，下载 route 不能仅凭 artifact id 暴露对象；必须确认当前 Project installation 可访问该 artifact。
- archive bytes 读取后继续校验 `archive_digest`。
- Webview asset 仍只能读取 manifest 声明的 panel entry 所在目录。
- Runtime action/channel invocation 继续从 enabled Project installation 解析，不从 LibraryAsset 直接执行。

## Trade-Offs

- Owner-kind artifact table 会触及 repository/route 边界，但它能把 artifact 完整性、幂等创建、访问判定和运行态引用收敛到同一模型。
- 保留两步 upload/install API 对 CLI 影响小，但前端必须隐藏 artifact inventory，避免产品心智继续分叉。
- Declaration-only ExtensionTemplate 保留了内嵌 command/flag 资产能力；安装逻辑必须明确区分它与 packaged host extension。

## Spec Updates

完成实现后更新：

- `.trellis/spec/cross-layer/shared-library-contract.md`
  - ExtensionTemplate/package artifact/Project installation 三者关系
  - Marketplace packaged install/publish/source-status 语义
- `.trellis/spec/frontend/architecture.md`
  - Assets Extension 管理使用 Project extension management API，不使用 runtime projection
- `.trellis/spec/backend/architecture.md` 或新增 backend appendix
  - Extension package artifact ownership 与 runtime download access 约束
