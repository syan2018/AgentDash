# VFS 路径与 Inline Mount 写入坐标收敛执行计划

## Phase P0 — 证据确认

- [x] 搜索并列出所有 `parse_inline_mount_owner` 调用点。
- [x] 搜索并列出所有 `InlineContentOverlay::new` 调用点。
- [x] 搜索并列出所有直接调用 `inline_file_repo.upsert_file/delete_file/delete_by_owner` 的 VFS mutation 代码。
- [x] 搜索 `agentdash_context_container_id`、`container_id`、`root_ref` 消费点，判断哪些是 storage 坐标，哪些只是展示信息。

## Phase P1 — 地址类型与 resolver

- [x] 在 application VFS 层新增 `InlineStorageKey` 类型。
- [x] 新增 `inline_storage_key_from_mount(mount)`，集中封装 owner_kind / owner_id / container_id 解析。
- [x] 将 route/application 中直接调用 `parse_inline_mount_owner` 的 mutation 路径改为使用新 resolver。
- [x] 为 ProjectVfsMount / Project config context / Story context / ProjectAgent Knowledge 四类 inline mount 增加 resolver 单元测试。

## Phase P2 — Mutation Dispatcher

- [x] 新增 `VfsMutationDispatcher` 或同等服务，挂到 `AppState.services`。
- [x] 实现 `create_text`：
  - normalize target path
  - check write + create capability
  - inline mount 通过 unified inline writer 创建
  - 非 inline mount 委托 provider/relay path
- [x] 实现 `write_text`：
  - inline mount 通过 unified inline writer upsert text
  - 非 inline mount 委托 provider/relay path
- [x] 实现 `delete_text` / `rename_text`：
  - inline mount 不再由 route 直写 repo
  - rename 检查源存在与目标冲突
- [x] 实现 `apply_patch`：
  - dispatcher 内部创建 overlay 或 patch target
  - route handler 不再接触 overlay
- [x] 实现 `upload_binary`：
  - dispatcher 内部解析 inline storage key
  - route handler 不再解析 owner 坐标

## Phase P3 — Surface API 收敛

- [x] 重写 `vfs_surfaces.rs` mutation handlers：
  - `create_surface_file`
  - `write_surface_file`
  - `delete_surface_file`
  - `rename_surface_file`
  - `apply_surface_patch`
  - `upload_surface_file_blob`
- [x] handlers 只保留 permission、surface resolution、DTO 转换。
- [x] 删除 route handler 中 provider == inline_fs 的分散分支。
- [x] 删除 route handler 中 direct inline repo mutation。
- [x] 错误映射到 `ApiError::BadRequest/NotFound/Conflict/ServiceUnavailable/Internal` 的用户语义。

## Phase P4 — Runtime metadata 收束

- [x] 根据 P0 搜索结果决定 `agentdash_context_container_id` 是否保留。
- [x] 如保留，改名或注释为展示/lineage 字段；如无实际消费者，删除字段。
- [x] 明确 `container_id` 只表示 inline storage container。
- [x] 检查 `ResolvedMountSummary.container_id` 是否应展示 storage container；如会误导 UI，改为更准确字段或从 summary 移除。
- [x] 从 `ResolvedMountSummary` 移除 `root_ref` / owner 坐标 / context container 字段，避免 Surface API 暴露内部坐标。
- [x] 更新相关 tests。

## Phase P5 — Frontend 验证与错误展示

- [x] 检查 `VfsBrowserPanel` 是否仍只发送 `surfaceRef + mountId + path`。
- [x] 为 create/save/delete/rename/apply_patch 添加服务层 payload 测试，确认 payload 不出现内部坐标。
- [x] 检查错误展示，确保不会把 `InlineContentOverlay` 显示给用户。
- [ ] 手动验证 Assets / VFS Mount 中 inline mount 的文件操作闭环。

## Phase P6 — Spec 更新

- [x] 更新 `.trellis/spec/backend/vfs/vfs-access.md`：
  - Surface mutation 统一入口
  - InlineStorageKey 内部坐标
  - 外部地址模型 `surface_ref + mount_id + relative_path`
- [x] 如 metadata 字段语义改变，同步更新相关 spec。
- [x] 按用户要求只记录为什么这样设计，不记录旧实现的失败形态。

## Phase P7 — 收敛后结构清理

- [x] 将 `vfs_surfaces.rs` 拆成 handler / DTO / resolver / helper，handler 文件只保留 HTTP 边界逻辑。
- [x] 将 `.trellis/spec/backend/vfs/vfs-access.md` 从场景堆积文档压回核心契约文档。
- [x] 复查 Surface API 与前端类型，不再暴露 `owner_kind` / `owner_id` / `context_container_id` / `root_ref`。

## Validation Commands

```powershell
cargo test -p agentdash-application vfs
cargo test -p agentdash-api vfs
pnpm --filter app-web test -- vfs-browser-panel
pnpm --filter app-web typecheck
```

本轮已执行并通过：

```powershell
cargo check -p agentdash-application
cargo check -p agentdash-api
cargo test -p agentdash-application vfs
cargo test -p agentdash-application vfs::mutation_dispatcher
cargo test -p agentdash-api vfs
pnpm --filter app-web typecheck
pnpm --filter app-web test -- vfs-browser-panel
pnpm --filter app-web test -- vfs
```

如果改动触及 shared types 或 provider registry，再追加：

```powershell
cargo check --workspace
pnpm --filter app-web lint
```

## Risk Areas

- `RelayVfsService` 当前同时承担 read/list/search/write/patch，多 provider 写入抽象需要避免循环依赖。
- `InlineContentOverlay` 既承担 patch target 又承担 write-through/event，迁移时先移动调用位置，再考虑拆分。
- `ProjectVfsMount` runtime metadata 中 `container_id` 与 `agentdash_context_container_id` 的消费者需要先查清。
- Blob upload 是 binary 路径，不能被 text-only mutation 抽象误伤。
- Skill asset 也复用 `inline_fs_files`，但 provider 是 `skill_asset_fs`；本任务只收敛 inline_fs mount 的写入，不改变 skill asset provider 的业务规则。

## Review Gate Before Start

- [x] 用户确认完整覆盖 Project VFS Mount、Project config inline container、Story inline container、ProjectAgent Knowledge。
- [x] PRD / design / implement 已经被用户接受并要求开始推进。
- [x] 进入实现前运行 `python ./.trellis/scripts/task.py start 05-21-vfs-path-coordinate-convergence`。
