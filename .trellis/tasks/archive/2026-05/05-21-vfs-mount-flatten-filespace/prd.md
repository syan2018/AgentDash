# VFS Mount 与 Filespace 扁平化重构

## Goal

把 `b362100c` 引入的 `ProjectFilespace` 资产层与 `ProjectVfsMountBinding` 挂载层合并为单一的 `ProjectVfsMount` 实体，并将 Marketplace 的 `filespace_template` 收敛为更通用的 `vfs_mount_template`，让 Project 级 VFS 在数据模型、API、UI 与共享生态层面只剩"挂载点"一个一等公民。

## User Value

- 用户在 ProjectSettings 直接「新建 Project Mount」就能一键完成"配文件 + 挂上去"，不用再先建 Filespace、再去 Mount 区配 binding。
- Marketplace 安装一份 mount 即"安装即可用"，不再需要二次手动挂载；权限闸门由 Agent VFS access policy 单点控制。
- 一个 mount 概念覆盖 inline 文件资产与 external service 两类内容，不再有「同一份内容跨两个实体表达」造成的 mount_id ↔ key 双 identifier 漂移。
- 简化 spec / 文档：Story override / disable、Agent grants、VFS surface 的契约都只围绕 mount_id 展开。

## Confirmed Facts

- `b362100c` 落地的双层模型：
  - `ProjectFilespace { id, project_id, key, display_name, description, installed_source, ... }` — 资产层
  - `ProjectVfsMountBinding { id, project_id, mount_id, display_name, source: Filespace | ExternalService, capabilities, default_write }` — 挂载层
  - Filespace 创建自动生成同名 binding，95% 真实流程是 1:1
- `ProjectVfsMountSource` 已经是 tagged enum（`kind=filespace` / `kind=external_service`），暗示 binding 是独立概念；但 Marketplace 只支持 `filespace_template`，External 这一支没有发布路径
- `inline_fs_files` 已经支持 typed text/binary 存储，owner_kind 包含 `project_filespace`
- 现有 Story override / disable、Agent `vfs_access_grants` 全部以 `mount_id` 为锚点
- 项目处于预研期（migration 0052 为 b362100c），允许破坏性 schema 变更
- 紧接着的 fix 任务 `05-21-project-filespace-frontend-fix` 已把双层模型补到「能用」，扁平化是上层架构整改而不是回归修复
- Marketplace 暂无任何 `filespace_template` 资产实例（无 builtin、无安装存量），迁移时直接 drop，不需要 payload 重映射
- "Marketplace 安装不自动挂载" 在双层模型下被作为安全约束，但权限实际由 Agent VFS access policy 控制，binding 自身不授予 agent 任何能力

## Requirements

### 数据模型

- 新增 `ProjectVfsMount` 实体替代 `ProjectFilespace + ProjectVfsMountBinding`：
  - 字段：`id`、`project_id`、`mount_id`、`display_name`、`description?`、`capabilities`、`default_write`、`installed_source?`、`content`（异构）、`created_at`、`updated_at`
  - `content` 异构枚举：
    - `Inline` — 文件存储走 `inline_fs_files(owner_kind="project_vfs_mount", owner_id=mount.id)`
    - `ExternalService { service_id, root_ref }`
  - mount_id 在同 Project 内唯一；`normalize_identifier` 规则保留（禁用 `/ \ :` 与保留字 `main`）
- 取消 `ProjectFilespace` 与 `ProjectVfsMountBinding` 的独立实体、表与 repository
- `inline_fs_files.owner_kind` 收敛：移除 `project_filespace`，新增 `project_vfs_mount`；migration 阶段把现有数据 owner 改写

### Marketplace / Shared Library

- `LibraryAssetType::FilespaceTemplate` → `LibraryAssetType::VfsMountTemplate`
- `VfsMountTemplatePayload` 覆盖两类内容：
  - `inline` 子类型：保留现有 `files: Vec<FilespaceTemplateFilePayload>` 形态（text + binary 字段），并附带 mount-level 元数据（capabilities / default_write / mount_id 默认值）
  - `external_service` 子类型：携带 `service_id`、`root_ref` 与 mount-level 元数据；安装时校验 `service_id` 在 mount providers 中已注册（来自 plugin / extension）
- 安装语义：直接产出一个 `ProjectVfsMount`，不再分两步；Agent VFS access policy 仍是真正的权限闸门
- `source-status` 列表把 `filespaces` 项更名为 `vfs_mounts`；item shape 与其它资产对齐

### API

- 移除 `/api/projects/{project_id}/filespaces` 与 `/api/projects/{project_id}/vfs-mount-bindings` 两套路由
- 收敛为 `/api/projects/{project_id}/vfs-mounts`：
  - `GET` 列表
  - `POST` 创建（请求体支持两种 content kind）
  - `GET /{mount_id}` 详情
  - `PUT /{mount_id}` 全字段覆盖（含 mount_id 改名时的冲突校验）
  - `DELETE /{mount_id}`
- 路径标识符一律用 `mount_id`，**不出现 UUID**；mount 数据库主键 UUID 仍保留供 inline_fs_files owner 引用，但不暴露到 API 表面
- VFS Surface 类型 `project_filespace` 改名为 `project_vfs_mount`，唯一标识为 `{ project_id, mount_id }`

### Session Construction / Runtime

- `build_derived_vfs` 改为直接遍历 `ProjectVfsMount` 列表生成 mount table，不再经过 binding-derived 中间步
- Story `disabled_container_ids` / 同名 override 仍按 mount_id；不变更语义
- Agent `vfs_access_grants` 不变；`MountCapability` 不变；effective mount table 计算路径只少一层间接寻址
- Workspace `main` 与系统 mount 不受影响

### Frontend

- 取消 Assets / Filespace 类目，改为新的「Project VFS」类目（或保留 Assets/Filespace 名称但内容直接是 mount 列表，文案对齐 Mount 概念）
- ProjectSettings Context Tab 的 `MountBindingsPanel` 收敛为 `ProjectVfsMountsPanel`：
  - 列表 + 行内 capabilities / default_write 编辑 + 删除
  - 新建对话框直接选 `Inline` / `ExternalService`，Inline 创建后能在主面板内直接进 VfsBrowser 编辑
- `AssetPickerDrawer` 把 "filespace" 选项替换为 "vfs_mount"，按 `installed_source` 过滤行为保留
- `MarketplaceAssetDrawer` 把 `FilespaceTemplateBody` 重写为 `VfsMountTemplateBody`，区分 inline / external 两种 payload 展示
- `VfsAccessPicker` 与 `useProjectStore.vfsMountBindingsRevision` 不变：仍按 mount_id 展示，仅来源数据从 binding 列表换为 mount 列表

### Migration

- Schema migration 一次性完成：
  - 新建 `project_vfs_mounts` 表
  - 把现有 `project_filespaces` 行 + 同 Project 内引用它的 binding 合并为一行 `project_vfs_mounts`（mount_id 取 binding.mount_id；display_name 取 binding；capabilities / default_write 取 binding；description / installed_source 取 filespace；content=Inline）
  - 把现有 `kind=external_service` binding 直接迁移为 `content=ExternalService`
  - `inline_fs_files` 把 `owner_kind=project_filespace` rewrite 为 `project_vfs_mount`，owner_id 替换为新 mount.id
  - drop `project_filespaces` 与 `project_vfs_mount_bindings` 表
- Shared Library `library_assets` 中 `asset_type=filespace_template` 行：**直接 DELETE**（无存量、无重映射）
- `LibraryAssetType::FilespaceTemplate` 枚举值从 domain 中删除；新增 `VfsMountTemplate` 替代
- 完全不保留双层的运行兼容分支；旧类型从 codebase 中删除，编译期保证不被误用

### 文档 / Spec

- 更新：
  - `.trellis/spec/cross-layer/shared-library-contract.md` — `vfs_mount_template` 章节、source-status 项更名
  - `.trellis/spec/backend/vfs/vfs-access.md` — Project VFS Mount 路由章节重写
  - `.trellis/spec/backend/shared-library.md` — 资产类型表
- 删除或重写 `.trellis/spec` 中残留的 Filespace / Mount Binding 双层概念表述

## Acceptance Criteria

- [ ] 现有数据完成迁移后，Project 级 VFS runtime 表现保持等价（mount_id / capabilities / default_write / 文件内容字节级一致）
- [ ] 旧 `ProjectFilespace` 与 `ProjectVfsMountBinding` 类型 / 表 / 路由从 domain / application / infrastructure / api / 前端类型 / 前端 service 中完全移除
- [ ] `vfs_mount_template` 发布与安装支持 inline（text + binary）与 external_service 两种 payload；安装即出现可用 mount，不需要二次操作
- [ ] AssetPickerDrawer 的 mount 选项过滤 installed_source；MarketplaceAssetDrawer 正确解析两种 payload 子类型
- [ ] ProjectSettings Context Tab 一个面板内即可完成 mount 列表浏览、行内编辑、新建（inline / external）、删除 与（inline 时）文件内容编辑
- [ ] Story override / Agent VFS grants / Session Construction / Workspace main / Canvas 等既有路径在重构后行为不变
- [ ] `pnpm --filter app-web typecheck` / `pnpm --filter app-web lint` / `cargo check --workspace` / `cargo test --workspace` 全绿
- [ ] 相关 spec 文件更新到位，索引页 (`backend/index.md` / `cross-layer/index.md` / `frontend/index.md`) 不再出现 Filespace / Mount Binding 分层叙述

## Out Of Scope

- 不引入"跨 Project mount"或"全局 mount registry"
- 不修改 Story 局部 inline VFS（Story 仍走 `context_containers` 自有路径，本任务不动它）
- 不重做 VfsBrowser 自身交互
- 不改 `inline_fs_files` 的内部存储 schema 与 typed content 形态
- 不引入 mount provider 注册流程的可视化（前端只展示 service_id 列表，由 plugin extension 负责注册）
- 不为 mount template 设计跨 service_id 的 alias 解析

## Open Questions

无。Plan 阶段四个 open question 已确认：

- 不做任何兼容分支，旧类型 / 旧路由 / 旧 surface 名一次性删除
- 路径用 `mount_id`，不出现 UUID（除 inline_fs_files owner_id 这种内部引用之外）
- 旧 `filespace_template` 资产无存量 → 直接 DELETE，无 payload 重映射
- 无 builtin seed / 安装存量需要照顾
