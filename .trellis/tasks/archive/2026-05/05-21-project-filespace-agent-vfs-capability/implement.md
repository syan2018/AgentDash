# Project Filespace 资产化与 Agent VFS 能力分配迁移实施计划

## Phase 0: Preflight

- [x] Agent VFS access policy 默认策略：新 Agent 默认不继承任何 Project Filespace / Project VFS mount。
- [x] FilespaceTemplate 第一版直接支持 binary 文件发布 / 安装 roundtrip。
- [ ] 运行基线检查：
  - `cargo check -p agentdash-domain`
  - `cargo check -p agentdash-application`
  - `cargo check -p agentdash-api`
  - `pnpm --filter app-web typecheck`

## Phase 1: Domain / DB

- [ ] 新增 `ProjectFilespace` domain entity 与 repository trait。
- [ ] 新增 `ProjectVfsMountBinding` domain entity 与 repository trait。
- [ ] 扩展 `InlineFileOwnerKind`，加入 `project_filespace`。
- [ ] 新增 migration：
  - `project_filespaces` table
  - `project_vfs_mount_bindings` table
  - `inline_fs_files.owner_kind` check 更新
  - Project config `context_containers.inline_files` 迁移
  - Project config `context_containers.external_service` 迁移为 mount binding
  - 直接删除 `project_agents.project_container_ids` 字段，不迁移为 agent config
- [ ] 实现 Postgres repository。
- [ ] 添加 migration 测试或 repository roundtrip 测试。

## Phase 2: VFS Construction

- [ ] 更新 `build_derived_vfs` 的输入，让 Project 级 mount 从 `ProjectVfsMountBinding` 派生。
- [ ] 新增 Filespace mount builder：
  - provider 仍为 `inline_fs`
  - root_ref 使用稳定 provider URI，例如 `filespace://project/{project_id}/{filespace_id}`
  - metadata 带 filespace_id / project_id / owner_kind / container_id
- [ ] 更新 `parse_inline_mount_owner` 或新增 parser，支持 Filespace owner。
- [ ] 保留 Story `ContextContainerDefinition` 构建路径。
- [ ] 更新 Story disabled / override 逻辑，使其能禁用或覆盖 Project mount binding 派生的 mount。
- [ ] 实现 Agent `vfs_access_grants` effective capability resolver，替代 `filter_project_containers_by_whitelist`。
- [ ] 确保 resolver 输出的最终 mount capabilities 是 Project mount binding capabilities 与 Agent grant capabilities 的交集。
- [ ] 确保 VFS tools 只基于 `SessionConstructionPlan.surface.vfs` / `CapabilityState.vfs.active` 的 effective capabilities 判权。
- [ ] 补 Project preview / Story preview / Session runtime VFS 一致性测试。

## Phase 3: API

- [ ] 新增 Project Filespace routes 与 DTO。
- [ ] 新增 Project VFS mount binding routes 与 DTO。
- [ ] 新增 `ResolvedVfsSurfaceSource::ProjectFilespace`。
- [ ] 更新 Project preview / Story preview / Task preview surface resolution。
- [ ] 更新 Project Agent DTO / routes，支持 `vfs_access_grants`。
- [ ] 移除 Project Settings 保存 Project `context_containers` 的主路径。
- [ ] 补 API 权限与错误测试。

## Phase 4: Shared Library / Marketplace

- [ ] 新增 `LibraryAssetType::FilespaceTemplate`。
- [ ] 新增 `FilespaceTemplatePayload` typed validator。
- [ ] `FilespaceTemplatePayload` 支持 text / binary 文件；binary payload 使用 MIME + size + standard base64。
- [ ] 更新 migrations 中 `library_assets_type_check`。
- [ ] 更新 publish mapper：ProjectFilespace → FilespaceTemplate。
- [ ] 更新 install mapper：FilespaceTemplate → ProjectFilespace + inline files。
- [ ] 补 binary 文件发布 / 安装 roundtrip 测试，断言 MIME、size 与 bytes 一致。
- [ ] 更新 source-status，加入 `filespaces`。
- [ ] 更新 DTO 与前端 shared-library types。
- [ ] 补 publish / install / source-status 测试。

## Phase 5: Frontend Assets

- [ ] 新增 Assets `filespace` category route。
- [ ] 实现 Filespace list / create / edit / delete 面板。
- [ ] 复用 `VfsBrowser` 编辑 Filespace 文件。
- [ ] 接入 `PublishLibraryAssetDialog` 与 Marketplace source status。
- [ ] 更新 Marketplace drawer / install summary，支持 FilespaceTemplate。
- [ ] Project Settings VFS tab 改为 preview + 跳转。

## Phase 6: Agent VFS Capability UI

- [ ] 扩展前端 ProjectAgent / AgentPresetConfig 类型。
- [ ] 在 Agent preset editor 中加入 VFS access policy picker。
- [ ] Picker 数据来自 Project VFS mount bindings 与 Filespace metadata。
- [ ] 支持按 mount 设置 read/write/list/search effective capabilities。
- [ ] 禁止 UI 勾选 Project mount binding 不支持的 capability。
- [ ] 不显示旧 `project_container_ids` 等价状态；该字段直接退役。
- [ ] 补 mapper / UI 测试。

## Phase 7: Cleanup

- [ ] 删除 Project 运行主线对 `project.config.context_containers` 的依赖。
- [ ] 删除或收窄 `ContextContainersEditor` 在 Project 场景的使用；Story 场景继续保留。
- [ ] 删除 `ProjectAgent.project_container_ids` DB 字段、domain 字段、DTO 字段、前端类型和运行时过滤主线。
- [ ] 搜索确认不存在绕过 effective VFS capabilities 的权限判断。
- [ ] 更新 README / Trellis spec。
- [ ] 搜索确认无旧入口残留：
  - `rg -n "project_container_ids|context_containers" crates packages/app-web/src`
  - 对 Story 相关命中逐项确认是保留路径。

## Validation Commands

```powershell
cargo fmt --all --check
cargo check -p agentdash-domain
cargo check -p agentdash-application
cargo check -p agentdash-api
cargo test -p agentdash-domain
cargo test -p agentdash-application vfs
cargo test -p agentdash-application shared_library
cargo test -p agentdash-api vfs_access
pnpm --filter app-web typecheck
pnpm --filter app-web test
```

## Risky Files / Areas

- `crates/agentdash-domain/src/context_container.rs`
- `crates/agentdash-domain/src/inline_file/entity.rs`
- `crates/agentdash-domain/src/shared_library/value_objects.rs`
- `crates/agentdash-application/src/vfs/mount.rs`
- `crates/agentdash-application/src/vfs/provider_inline.rs`
- `crates/agentdash-application/src/session/construction_planner.rs`
- `crates/agentdash-api/src/routes/vfs_surfaces.rs`
- `crates/agentdash-api/src/routes/shared_library.rs`
- `crates/agentdash-infrastructure/migrations/`
- `packages/app-web/src/features/assets-panel/`
- `packages/app-web/src/features/project/agent-preset-editor/`
- `packages/app-web/src/pages/ProjectSettingsPage.tsx`

## Review Gates

- [ ] PRD / design / implement 经用户确认。
- [ ] Migration SQL 先单独 review。
- [ ] Session VFS construction 测试先于 UI 接线完成。
- [ ] Shared Library payload schema 与前端类型同步 review。
- [ ] 质量检查通过后再进入 commit。

## Implementation Progress 2026-05-21

- [x] 新增 ProjectFilespace / ProjectVfsMountBinding domain、repository、Postgres 初始化与 migration。
- [x] 新增 `project_filespace` inline owner，并将 Filespace 文件存储统一到 `inline_fs_files(owner_kind='project_filespace', container_id='files')`。
- [x] 移除 `ProjectAgent.project_container_ids` 的 domain / DB repository / API / frontend 主线字段。
- [x] 新增 Agent `vfs_access_grants`，Project Agent session 会按 Project binding capabilities 与 Agent grant capabilities 的交集裁剪 effective VFS mount。
- [x] Project VFS 构造改为读取 Project mount bindings；Story inline context 仍保留局部绑定，并可覆盖/禁用同 mount_id 的 Project mount。
- [x] 新增 Filespace routes、ProjectFilespace surface、Assets Filespace category、Agent VFS access picker。
- [x] Shared Library 支持 `filespace_template`，包含 text / binary payload 的发布与安装路径。
- [x] Project Settings VFS tab 改为 Filespace 资产入口与 runtime preview。
- [x] 修复 Postgres repository 初始化中的多语句 prepared statement 问题，表与索引逐条执行。

验证已通过：

```powershell
cargo check -q
cargo test -q
pnpm run frontend:check
pnpm run frontend:lint
```
