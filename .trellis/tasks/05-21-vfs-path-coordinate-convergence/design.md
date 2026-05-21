# VFS 路径与 Inline Mount 写入坐标收敛设计

## Architecture

将 VFS 写入链路从“route handler 按 provider 手写分支”收敛为“先解析 runtime mount，再交给统一 mutation dispatcher”。覆盖范围是所有 `inline_fs` runtime mount，包括 Project VFS Mount、Project config inline container、Story inline container 与 Project Agent Knowledge。

```text
Frontend / Tool
  surface_ref + mount_id + relative_path
        |
        v
Surface / Tool Boundary
  parse SurfaceRef
  resolve Vfs
  normalize MountRelativePath
        |
        v
VfsMutationDispatcher
  resolve Runtime Mount
  check capability + edit capability
  resolve provider writer
        |
        +-- inline_fs       -> InlineStorageKey -> InlineFileRepository
        +-- relay_fs        -> Relay mount provider
        +-- skill_asset_fs  -> SkillAsset provider
        +-- lifecycle_vfs   -> Lifecycle provider
        +-- canvas_fs       -> Canvas provider
```

外部稳定地址只有 `surface_ref + mount_id + relative_path`。内部 inline storage key 是 resolver 输出：

```rust
pub struct InlineStorageKey {
    pub owner_kind: InlineFileOwnerKind,
    pub owner_id: Uuid,
    pub container_id: String,
}
```

`InlineStorageKey` 不进入 API DTO，不进入前端类型，不进入 Agent tool schema。

## Core Contracts

### SurfaceRef

`surface_ref` 继续负责声明 VFS 来源，例如：

- `project:{project_id}` / `story:{project_id}:{story_id}` / `task:{project_id}:{task_id}`
- `project-vfs-mount:{project_id}:{mount_id}`
- `project-agent-knowledge:{project_id}:{project_agent_id}`
- `session-runtime:{session_id}`

Surface resolver 的职责是构建一份完整 `Vfs`，并返回 `ResolvedVfsSurface` 给前端。Mutation dispatcher 只消费 resolver 产物，不重新理解业务来源。Project / Story / Agent Knowledge 的业务来源可以不同，但进入 mutation dispatcher 后都只表现为 runtime mount。

### MountId

`mount_id` 是用户可见、Project/Story runtime 内唯一的 mount 标识。Project VFS Mount 的数据库 UUID 只用于持久化，不作为 route path 或前端输入。

### MountRelativePath

所有文件路径在 API 边界 normalize 成 mount-relative path。路径规则沿用现有 `normalize_mount_relative_path`：拒绝绝对路径、拒绝 `..` escape、统一 `/`。

### InlineStorageKey

inline_fs 的存储 key 来自 runtime mount metadata。收敛后只保留一个函数负责解析：

```rust
pub fn inline_storage_key_from_mount(mount: &Mount) -> Result<InlineStorageKey, VfsAddressError>
```

该函数替代 route 层散落的 `parse_inline_mount_owner(mount)` 调用。原函数可以保留为内部实现，但外部调用点应集中。

必须覆盖的 resolver cases：

- `InlineFileOwnerKind::ProjectVfsMount`：Project 级 VFS Mount，storage container 固定为 `"files"`。
- `InlineFileOwnerKind::Project`：Project config inline container，storage container 来自 container mount identity。
- `InlineFileOwnerKind::Story`：Story inline container，storage container 来自 Story context container mount identity。
- `InlineFileOwnerKind::ProjectAgent`：Agent Knowledge，storage container 为 agent knowledge mount 的内部容器。

## Mutation Dispatcher

新增 application 层服务，名称可在实现阶段按现有模块风格确定：

```rust
pub struct VfsMutationDispatcher {
    relay_service: Arc<RelayVfsService>,
    inline_file_repo: Arc<dyn InlineFileRepository>,
    mount_provider_registry: Arc<MountProviderRegistry>,
}
```

建议 API：

```rust
pub async fn create_text(
    &self,
    vfs: &Vfs,
    target: ResourceRef,
    content: &str,
    identity: Option<&AuthIdentity>,
) -> Result<MutationResult, VfsMutationError>;

pub async fn write_text(...);
pub async fn delete_text(...);
pub async fn rename_text(...);
pub async fn apply_patch(...);
pub async fn upload_binary(...);
```

`MutationResult` 至少包含：

- normalized path
- content_kind
- mime_type
- size
- affected paths for patch
- persisted flag 如仍有 UI 需要

### inline_fs text mutations

inline text 写操作流程：

1. resolve mount with `Write` capability。
2. resolve edit capability（create/delete/rename 按操作校验）。
3. normalize path。
4. resolve `InlineStorageKey`。
5. create 检查目标不存在。
6. write/upsert/delete/rename 使用 `InlineFileRepository`。
7. 可选：复用 `InlineContentOverlay` 的事件发送语义，或把事件发布抽为 `InlineMountEventPublisher`。

关键点：Surface route 不再自己 new overlay，也不再直接调用 repo。

### inline_fs apply_patch

apply_patch 可以继续复用现有 `InlineOverlayPatchTarget` 的 patch 组合能力，但 overlay/persister 的创建移入 dispatcher。

实现选项：

- 选项 A：dispatcher 为每次 surface mutation 创建一个 write-through overlay，调用 `RelayVfsService.apply_patch(... Some(&overlay) ...)`。
- 选项 B：把 `InlineOverlayPatchTarget` 改造成直接持有 `InlineFileRepository + InlineStorageKey`，绕过 overlay。

推荐 A。理由：改动小，保留现有 patch 测试与事件语义；同时 route handler 不再知道 overlay。

### inline_fs binary upload

blob upload 当前直接解析 mount owner 并 upsert binary。收敛后由 dispatcher 的 `upload_binary` 执行：

1. 校验 mount provider 是 `inline_fs`。
2. 校验 MIME 与当前产品规则一致。
3. normalize path。
4. resolve `InlineStorageKey`。
5. upsert `InlineFile::new_binary(...)`。

## Runtime Mount Metadata

当前 Project VFS Mount inline runtime mount 同时携带：

- `id = mount_id`
- `root_ref = project-vfs-mount://{mount.uuid}`
- `metadata.container_id = "files"`
- `metadata.agentdash_context_container_id = mount_id`
- owner metadata

收敛目标：

- `metadata.container_id` 专指 inline storage container，例如 `"files"`。
- `metadata.agentdash_context_container_id` 只用于 UI/context lineage；如没有独立消费价值，应删除。
- `root_ref` 只保留 provider root identity，不参与用户路径。
- Project VFS Mount 的内部 UUID 可继续通过 `root_ref` 或 metadata 表达，但只允许 resolver 消费。

实现阶段需要先搜索 `agentdash_context_container_id` 的所有消费者，再决定删除或重命名。若只用于 surface summary 的 `container_id` 展示，应改为明确字段，例如 `display_container_id` / `storage_container_id`，避免与 inline storage key 混用。

## API Boundary

`vfs_surfaces.rs` 的 mutation handlers 收敛为薄层：

```rust
let source = SurfaceRef::parse(req.surface_ref)?;
let (_surface, vfs) = resolve_surface_bundle(... Edit).await?;
let result = state.services.vfs_mutations.create_text(&vfs, target, content, auth).await?;
Ok(Json(dto_from_result(result)))
```

每个 handler 保留：

- permission resolution
- request DTO parse
- response DTO construction

每个 handler 移除：

- provider == inline_fs 的写入分支
- repo direct write/delete/rename
- overlay direct construction
- inline owner parse

## Frontend Boundary

前端保持现有 payload：

```ts
{
  surface_ref: string;
  mount_id: string;
  path: string;
}
```

VfsBrowser 的职责不扩大。只需要在错误展示上把后端返回的内部错误映射为用户语义。若后端已完成映射，前端不做额外兼容。

## Error Model

新增或复用统一 mutation error：

| Error | HTTP | 用户语义 |
| --- | --- | --- |
| MountNotFound | 404 | mount 不存在 |
| InvalidPath | 400 | 路径非法 |
| CapabilityDenied | 400/403 | 当前 mount 没有对应能力 |
| EditUnsupported | 400 | 当前 mount 不支持该操作 |
| FileExists | 409 | 目标文件已存在 |
| FileNotFound | 404 | 文件不存在 |
| BackendOffline | 503 | Backend 离线 |
| ProviderFailure | 500 | provider 执行失败 |

`InlineContentOverlay`、`InlineFileRepository`、storage key 解析细节不进入用户可见错误消息。

## Migration Notes

本任务不改变 schema 主体。若删除或重命名 runtime metadata 字段涉及数据库持久化结构，需要先确认该字段只存在于构建出的 runtime mount metadata，而非 DB schema。

如果发现 `root_ref` 的格式需要从 `project-vfs-mount://{uuid}` 收敛为更类型化的 provider URI，本任务可做 hard cut，并同步 spec 与测试。

## Tradeoffs

- 统一 dispatcher 会把 route handler 变薄，但会新增 application 层抽象。收益是所有 mutation 共享同一套 capability、path、storage key 与错误模型。
- 保留 `InlineContentOverlay` 作为 dispatcher 内部实现可以降低 patch 风险；长期可再把 overlay 拆成“事件 + patch target + persistence”三个更小部件。
- Project/Story/AgentKnowledge/Project config 全部纳入 inline resolver 会扩大测试面，但这是达成“真正收束”的必要范围。

## Validation Strategy

后端：

- application unit tests：dispatcher create/write/delete/rename/apply_patch inline mount。
- API route tests：surface create/write/delete/rename/apply_patch/upload blob 不暴露 overlay 错误。
- provider tests：inline read/list/search/stat 与 binary metadata 保持现有行为。

前端：

- `vfs-browser-panel` 测试覆盖 create/save/delete/rename payload。
- 至少手动或自动验证 Inline Project VFS Mount 的浏览、创建、保存、删除、重命名。

Spec：

- 更新 VFS access spec 的 Surface mutation 章节。
- 记录 inline storage key 是 provider/internal 坐标，外部地址仍是 mount + relative path。
