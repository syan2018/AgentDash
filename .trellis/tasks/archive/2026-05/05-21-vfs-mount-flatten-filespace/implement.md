# VFS Mount 与 Filespace 扁平化重构执行计划

## 总体节奏

整改是 hard cut 的破坏性变更，按依赖层从 Domain → Infrastructure → Application → API → Frontend → Spec 推进；每一层完成后不让旧符号存活到下一层。最终一次性 commit 还是按层切 commit 在 Phase 3.4 决定。

收口检查每层都跑：

```bash
cargo check --workspace
pnpm --filter app-web typecheck
pnpm --filter app-web lint
```

## P1 — Domain 层

### Step P1.1 — `agentdash-domain::project_filespace` → `project_vfs_mount`

- 重命名模块目录：`crates/agentdash-domain/src/project_filespace/` → `project_vfs_mount/`（或保留目录，模块文件改名后再重排目录）
- `entity.rs`：删除 `ProjectFilespace` / `ProjectVfsMountBinding` / `ProjectVfsMountSource`；新增 `ProjectVfsMount` + `ProjectVfsMountContent`（`Inline` / `ExternalService { service_id, root_ref }`）
- 删除 `ProjectFilespace::new` 等旧构造器；新增 `ProjectVfsMount::new_inline(...)` / `::new_external_service(...)`
- 删除 `PROJECT_FILESPACE_CONTAINER_ID` 常量；inline_fs_files container_id 用 `"files"` 直接写在 application 层

### Step P1.2 — repository.rs

- 删除 `ProjectFilespaceRepository` / `ProjectVfsMountBindingRepository`
- 新增 `ProjectVfsMountRepository` （签名见 design）

### Step P1.3 — InlineFile owner_kind

- `crates/agentdash-domain/src/inline_file/entity.rs`：`InlineFileOwnerKind` 删除 `ProjectFilespace`，新增 `ProjectVfsMount`

### Step P1.4 — Shared Library

- `crates/agentdash-domain/src/shared_library/value_objects.rs`：
  - `LibraryAssetType::FilespaceTemplate` 删除，新增 `LibraryAssetType::VfsMountTemplate`
  - `FilespaceTemplatePayload` / `FilespaceTemplateFilePayload` 删除
  - 新增 `VfsMountTemplatePayload`（tagged enum）+ `InlineMountFilePayload`
- `LibraryAssetPayload` 替换 `FilespaceTemplate(...)` variant 为 `VfsMountTemplate(...)`

### Step P1.5 — Domain 收口

```bash
cargo check -p agentdash-domain
```

## P2 — Infrastructure 层

### Step P2.1 — Migration 0054

- `crates/agentdash-infrastructure/migrations/0054_project_vfs_mount_flatten.sql`
  - CREATE TABLE project_vfs_mounts
  - INSERT 合并 (filespace + binding) → mount Inline
  - INSERT external_service binding → mount ExternalService
  - UPDATE inline_fs_files owner 改写
  - DROP project_vfs_mount_bindings, project_filespaces
  - DELETE FROM library_assets WHERE asset_type = 'filespace_template'

### Step P2.2 — Repository 实现

- `crates/agentdash-infrastructure/src/persistence/postgres/`：
  - 新建 `project_vfs_mount_repository.rs`
  - 删除 `project_filespace_repository.rs`
  - mod.rs 同步
- inline_file_repository：把 `project_filespace` owner_kind 字符串映射改为 `project_vfs_mount`；CHECK 约束（如有）同步

### Step P2.3 — `RepositorySet`

- `crates/agentdash-application/src/repository_set.rs`：
  - 删除 `project_filespace_repo` / `project_vfs_mount_binding_repo`
  - 新增 `project_vfs_mount_repo`
  - `bootstrap` 路径同步替换

### Step P2.4 — Infra 收口

```bash
cargo check -p agentdash-infrastructure -p agentdash-application
```

## P3 — Application 层

### Step P3.1 — `agentdash-application::vfs::mount`

- `build_derived_vfs` 直接遍历 `ProjectVfsMount`：
  - Inline content → 走 `inline_fs` provider，root_ref 形如 `project-vfs-mount:{mount_id}`
  - ExternalService content → 走对应 provider
- 删除一切对 `ProjectFilespace` / `ProjectVfsMountBinding` 的引用

### Step P3.2 — Session Construction

- `assembler.rs` / `construction_planner.rs` / `task::context_builder` 等：把对 binding 列表的依赖替换为 mount 列表；surface ref 改名

### Step P3.3 — Shared Library publish / install / source-status

- `shared_library::publish` 中 `publish_filespace_payload` → `publish_vfs_mount_payload`
- `shared_library::install` 中 `install_filespace_template` → `install_vfs_mount_template`
- `shared_library::install` 中 source-status 计算：`filespaces: Vec<...>` → `vfs_mounts: Vec<...>`
- 删除一切对 `LibraryAssetType::FilespaceTemplate` / `FilespaceTemplatePayload` 的引用

### Step P3.4 — Application 收口

```bash
cargo check -p agentdash-application
```

## P4 — API 层

### Step P4.1 — Routes

- 文件：`crates/agentdash-api/src/routes/project_filespaces.rs` → 重命名 `project_vfs_mounts.rs`
- 完全重写 handler：
  - `list_vfs_mounts` / `create_vfs_mount` / `get_vfs_mount` / `update_vfs_mount` / `delete_vfs_mount`
  - 路径参数 `{mount_id}` 替代 `{filespace_id}` / `{binding_id}`
  - 校验：mount_id 唯一、normalize_capabilities、default_write 收敛
- 删除 `list_mount_bindings` / `create_mount_binding` / `update_mount_binding` / `delete_mount_binding`

### Step P4.2 — Routes 注册

- `routes.rs`：删除 `/projects/{project_id}/filespaces*` 与 `/projects/{project_id}/vfs-mount-bindings*`
- 新增 `/projects/{project_id}/vfs-mounts*`

### Step P4.3 — VFS Surface

- `crates/agentdash-api/src/routes/vfs_surfaces.rs`：`ResolvedVfsSurfaceSource::ProjectFilespace { ... }` → `ProjectVfsMount { project_id, mount_id }`；`surface_ref` 字符串形态 `project-vfs-mount:{project_id}:{mount_id}`
- 所有解析 / 序列化路径改名

### Step P4.4 — DTO

- `crates/agentdash-api/src/dto/shared_library.rs`：`filespace_template` 字段改为 `vfs_mount_template`
- 删除 `routes/shared_library.rs` 中对旧类型的兼容分支

### Step P4.5 — API 收口

```bash
cargo check -p agentdash-api
cargo test -p agentdash-api --no-run
```

## P5 — Frontend 类型 / Service / Store

### Step P5.1 — Types

- `packages/app-web/src/types/index.ts`：
  - 删除 `ProjectFilespace` / `ProjectVfsMountBinding` / `ProjectVfsMountSource`
  - 新增 `ProjectVfsMount` / `ProjectVfsMountContent`
- `packages/app-web/src/types/shared-library.ts`：
  - `LibraryAssetType` 中 `filespace_template` → `vfs_mount_template`
  - `PublishLibraryAssetKind` 中 `filespace` → `vfs_mount`
  - `InstallLibraryAssetResponse` 中 `{ asset_kind: "filespace"; ... }` → `{ asset_kind: "vfs_mount"; mount_id: string }`
- `packages/app-web/src/types/context.ts`：`source_type: "project_filespace"` → `"project_vfs_mount"`，字段 `{ project_id, mount_id }`

### Step P5.2 — Service

- 新建 `packages/app-web/src/services/projectVfsMounts.ts`：`listProjectVfsMounts` / `createProjectVfsMount` / `updateProjectVfsMount` / `deleteProjectVfsMount`
- 删除 `packages/app-web/src/services/projectFilespaces.ts`

### Step P5.3 — Store

- `packages/app-web/src/stores/projectStore.ts`：
  - `vfsMountBindingsRevision` → `vfsMountsRevision`
  - `bumpVfsMountBindingsRevision` → `bumpVfsMountsRevision`

### Step P5.4 — Frontend 类型层收口

```bash
pnpm --filter app-web typecheck
```

## P6 — Frontend UI

### Step P6.1 — VfsMountCategoryPanel

- 新建 `packages/app-web/src/features/assets-panel/categories/VfsMountCategoryPanel.tsx`：
  - 骨架沿用现 FilespaceCategoryPanel（Card Grid + CardMenu + OriginBadge + PublishedBadge + Editor Dialog + Notice + ConfirmDeleteDialog）
  - 卡片显示 content kind 标签
  - 创建 dialog：第一步选 Inline / ExternalService → 第二步填字段；Inline 创建后直接进 VfsBrowser
  - 编辑 dialog：Inline 双列（metadata + VfsBrowser）；ExternalService 单列（metadata + service_id/root_ref + capabilities）
- 删除 `FilespaceCategoryPanel.tsx`

### Step P6.2 — Assets 路由

- `packages/app-web/src/App.tsx`：assets 路由 `filespace` 段改为 `vfs-mount`
- `packages/app-web/src/features/assets-panel/AssetsTabView.tsx`：
  - `SHAREABLE_CATEGORIES` 中 `filespace` 改为 `vfs-mount`，label `VFS Mount`，hint `Project VFS 挂载点 (Inline / External)`
  - 头部描述：`Workflow / MCP / Skill / Filespace / Canvas` → `Workflow / MCP / Skill / VFS Mount / Canvas`
- `dashboard/assets/filespace` 路径如有外部入口需要同步改名

### Step P6.3 — AssetPickerDrawer

- `KIND_OPTIONS` 中 `filespace` 改为 `vfs_mount`
- `AssetStep` 渲染 `VfsMountList`（替代 `FilespaceList`）：用 `listProjectVfsMounts`，按 `installed_source` 过滤

### Step P6.4 — MarketplaceAssetDrawer

- `ASSET_TYPE_LABELS` 中 `filespace_template` 改为 `vfs_mount_template` → `VFS Mount`
- `TypeSpecificBody` 的 `filespace_template` case 改为 `vfs_mount_template`
- 重写 `FilespaceTemplateBody` 为 `VfsMountTemplateBody`：
  - 解析 payload kind
  - inline 子类型：显示文件数 + 前 12 个 path
  - external_service：显示 service_id / root_ref / capabilities chip

### Step P6.5 — VfsAccessPicker

- 数据源调用 `listProjectVfsMounts`（替代 `listProjectVfsMountBindings`）
- 渲染层只展示 mount_id + display_name + 允许的 capabilities，行为不变
- store revision key 改为 `vfsMountsRevision`

### Step P6.6 — ProjectSettings ContextTab

- 删除 `MountBindingsPanel` 引用与组件文件 `packages/app-web/src/features/project/vfs-mount-bindings/MountBindingsPanel.tsx`
- ContextTab 保留：「Project VFS Mount」跳转按钮（指向 `/dashboard/assets/vfs-mount`）+ MountOverviewList preview + VfsBrowser preview

### Step P6.7 — VfsBrowser source

- `packages/app-web/src/features/vfs/` 中 source union：`project_filespace` → `project_vfs_mount`，字段 `{ project_id, mount_id }`
- 所有调用点改名（`SkillCategoryPanel` 不受影响，它用 `project_skill_assets`）

### Step P6.8 — Frontend UI 收口

```bash
pnpm --filter app-web typecheck
pnpm --filter app-web lint
```

## P7 — Spec 更新

- `.trellis/spec/cross-layer/shared-library-contract.md`：
  - asset_type 列表 `filespace_template` → `vfs_mount_template`
  - source-status 中 `filespaces` → `vfs_mounts`
  - 删除 fix 任务里加的 Filespace 双层叙述
- `.trellis/spec/backend/vfs/vfs-access.md`：
  - 重写 Project VFS Mount 章节（路由 + content 异构）
  - 删除 Filespace / Mount Binding 双层段落
- `.trellis/spec/backend/shared-library.md`：资产类型表更新
- 检查并更新各 index.md

## P8 — 最终验收

```bash
cargo check --workspace
cargo test --workspace --no-run
pnpm --filter app-web typecheck
pnpm --filter app-web lint
```

启动 dev server 走主路径：

1. 数据库初始化执行 0054 migration（如有 fix 任务种过的 Filespace + binding，验证迁移成功且文件可读）
2. Assets / VFS Mount：创建 Inline → 编辑文件 → 保存 → 删除
3. Assets / VFS Mount：创建 ExternalService → 在 VfsAccessPicker 中可见 → 删除
4. ProjectSettings ContextTab：preview 列表反映新 mount，无 MountBindingsPanel
5. Marketplace：发布 inline VFS Mount → 在另 Project 安装 → 一步可用、可见 published badge

## Commit 切分建议（Phase 3.4）

按层切：

- commit 1：Domain + Infrastructure + Migration
- commit 2：Application（含 shared_library publish/install/source-status）
- commit 3：API + DTO + VFS Surface
- commit 4：Frontend types + service + store
- commit 5：Frontend UI（VfsMountCategoryPanel + 各处改名 + 删 MountBindingsPanel）
- commit 6：Spec 更新

如总改动量不大也可合并为 2-3 个 commit；最终切分由收口前的差异规模决定。

## Required Spec Updates

见 design.md 的 Required Spec Updates 章节；P7 直接执行。
