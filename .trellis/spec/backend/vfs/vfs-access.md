# 统一 VFS 跨层契约

VFS 的价值是让 Agent、前端和业务用例只面对一套稳定地址模型，而不是感知 `backend_id`、绝对路径、数据库主键或 inline storage 坐标。

## 地址模型

外部访问地址统一为：

```text
surface_ref + mount_id + mount_relative_path
```

- `surface_ref` 解析出一份 runtime `Vfs`，例如 Project preview、Story preview、Task preview、Project VFS Mount、Project Agent Knowledge 或 Session runtime。
- `mount_id` 定位该 `Vfs` 内的 runtime mount，是 UI/API/Agent tool 的稳定 mount 标识。
- `mount_relative_path` 是 mount 根下相对路径，进入 application 层前必须 normalize；绝对路径和 `..` escape 必须失败。

Application 层内部可以继续使用更强类型表达地址，例如 `ResourceRef`、`VfsUri`、`RootRef::LocalPath | RootRef::ProviderUri`。原始字符串只应停留在 UI/API/relay/tool 输入边界。

## Runtime Mount

runtime mount 是 provider 分发单位，至少表达：

- `id`：外部 mount identity。
- `provider`：如 `relay_fs`、`inline_fs`、`skill_asset_fs`、`lifecycle_vfs`、`canvas_fs`。
- `root_ref`：provider root identity；它不是用户路径。
- `capabilities`：read / write / list / search / exec 等能力。
- `metadata`：provider 内部解析所需的最小坐标。

`Vfs` 构建后必须 hard validate：mount id 唯一、default mount 存在、保留 mount id 没有被错误 provider 占用、root_ref/provider scheme 合法、capability 与 provider 支持范围一致、link target 存在且无环。

## Provider 职责

Provider 负责读、列、搜索、stat、binary read 等数据访问。云端代码不直接访问本机文件系统；本机代码不直接读写业务数据库。

| Provider | 职责 |
| --- | --- |
| `relay_fs` | 通过 relay 访问本机 workspace 文件。 |
| `inline_fs` | 暴露 Project / Story / Agent Knowledge 等内联文件。 |
| `skill_asset_fs` | 暴露 Skill asset 文件视图，文件内容复用 InlineFile 存储。 |
| `lifecycle_vfs` | 暴露 lifecycle run、node、artifact、record 投影。 |
| `canvas_fs` | 暴露 Canvas 相关虚拟内容。 |

Provider 返回的 `RuntimeFileEntry.attributes` 是结构化 metadata 通道，例如 `content_kind`、`mime_type`、`skill_asset_file_kind`。不要把这类 metadata 塞进文件文本内容。

## Canvas Session Visibility

### 1. Scope / Trigger
- Trigger: Canvas 被 `canvas_start` 或 `present_canvas` 暴露给当前 session 后，前端需要立即从 Session runtime surface 中浏览 `canvas_fs` mount。

### 2. Signatures
- Runtime mount id: `cvs-<canvas.mount_id>`
- Runtime mount provider: `canvas_fs`
- Session meta field: `visible_canvas_mount_ids: Vec<String>`，存储未加 `cvs-` 前缀的 `canvas.mount_id`
- Frontend browse address: `{mount_id}://{mount_relative_path}`，例如 `cvs-demo://src/main.tsx`

### 3. Contracts
- Canvas 工具在发送 `canvas_presented` 展示事件前，先把目标 Canvas 追加到 live runtime VFS，把 `canvas.mount_id` 写入 `visible_canvas_mount_ids`，并同步刷新 `CapabilityState.vfs.active`。
- Canvas 可见后，状态更新服务必须从刷新后的 live VFS 重新 discovery Skill 维度，并写入 `CapabilityState.skill.skills`；Canvas 工具返回值中的 `skill_name` / `skill_path` 只作为工具结果提示，不作为能力状态事实源。
- `CapabilityState.vfs.active` 与 `CapabilityState.skill.skills` 变化必须走统一 runtime context transition，产出 `capability_state_changed` 与 `context_frame(kind="capability_state_update")`，其中 VFS delta 包含新增的 `cvs-<canvas.mount_id>`，Skill delta 包含新增或变更的 Canvas 内嵌 skill。
- `/sessions/{session_id}/context` 基于 session meta 重新构建 `SessionRuntime` surface，并通过 `append_visible_canvas_mounts` 追加可见 Canvas mounts。
- 前端收到 `canvas_presented` 后刷新 session context，再打开或继续使用 Canvas / VFS tab；VFS browser 只消费刷新后的 `runtime_surface`，以 `surface_ref + mount_id + path` 访问文件。

### 4. Validation & Error Matrix
- Canvas 不属于当前 Project -> `Forbidden`
- Canvas 不存在 -> `NotFound`
- Session 不存在 -> `NotFound`
- Session 与 Canvas Project 不匹配 -> `Forbidden`
- VFS browser 使用的 mount 不在当前 `runtime_surface.mounts` 中 -> 前端展示当前 session surface 缺少该 mount

### 5. Good / Base / Bad Cases
- Good: `present_canvas("demo")` 后，session meta 包含 `demo`，context surface 包含 `cvs-demo`，`CapabilityState.skill.skills` 包含从 `cvs-demo://skills/canvas-system/SKILL.md` 发现的 `canvas-system`，前端点击“浏览文件”打开 `cvs-demo://`。
- Base: 同一 Canvas 重复展示时，`visible_canvas_mount_ids` 保持去重，surface 中保留一个 `cvs-demo` mount。
- Bad: Session 与 Canvas Project 不一致时，展示和 runtime invoke 都失败，不产生跨 Project mount。

### 6. Tests Required
- Canvas 工具测试：展示已有 Canvas 后，session meta 的 `visible_canvas_mount_ids` 包含 `canvas.mount_id`，并且 Canvas mount 可被 VFS skill loader 发现 `canvas-system`。
- Session context 测试：带可见 Canvas 的 session context surface 包含 `canvas_fs` mount，id 为 `cvs-<canvas.mount_id>`。
- 前端测试：`canvas_presented` 事件触发后会刷新 session context，VFS tab 使用刷新后的 `runtime_surface`。

## Surface Mutation

Surface text mutation 与 inline binary upload 的统一入口是 application 层 mutation dispatcher。Route handler 只负责：

- 权限检查。
- `surface_ref` 解析与 `Vfs` resolution。
- 请求 / 响应 DTO 转换。

Route handler 不解析 inline owner 坐标，不直接操作 `inline_file_repo`，也不构造 `InlineContentOverlay`。

dispatcher 负责：

- resolve runtime mount。
- normalize mount-relative path。
- 校验 mount capability 与 edit capability。
- 分发到 provider 或 inline writer。
- 把错误映射成用户语义：BadRequest / NotFound / Conflict / ServiceUnavailable / Internal。

## Inline Storage Key

`inline_fs` 的持久化坐标只由 application resolver 从 runtime mount metadata 生成：

```rust
pub struct InlineStorageKey {
    pub owner_kind: InlineFileOwnerKind,
    pub owner_id: Uuid,
    pub container_id: String,
}
```

所有 inline runtime mount 共用同一 resolver：

| 来源 | owner_kind | owner_id | container_id |
| --- | --- | --- | --- |
| Project VFS Mount | `project_vfs_mount` | `project_vfs_mount.id` | `files` |
| Project config inline container | `project` | `project.id` | container mount identity |
| Story inline container | `story` | `story.id` | container mount identity |
| Project Agent Knowledge | `project_agent` | `project_agent.id` | knowledge container identity |

`container_id` 只表示 inline storage container。展示或 lineage 需要独立命名，例如 `context_container_id`，避免与 storage container 混用。

## Inline Text 与 Binary

InlineFile 是 typed content storage：

```rust
pub enum InlineFileContent {
    Text { content: String },
    Binary { bytes: Vec<u8>, mime_type: String },
}
```

核心约束：

- text 文件走 `read_text` / `write_text` / `create_text` / `apply_patch`。
- binary 文件走 `read_binary` / blob upload；text API 读取 binary 必须失败。
- list / stat 暴露 `content_kind`、`mime_type`、`size`。
- blob upload 只允许 image MIME，并通过 mutation dispatcher 写入 inline storage。
- Agent `fs_read` 对 `image/*` binary 返回文本 metadata block + image block；非 image binary 返回 unsupported binary 语义，不把 bytes 放进模型上下文。

## Skill Asset 文件

Skill asset 文件内容存储在 InlineFile：

```text
owner_kind   = "skill_asset"
owner_id     = skill_assets.id
container_id = "files"
path         = Skill 根目录内相对路径
```

Skill 领域对象仍负责 `SKILL.md` 主文档、metadata validation、文件 kind 等业务语义。binary asset 在 JSON DTO 中只返回 metadata，不内联 bytes；`skill_asset_fs` 通过 provider 读取 text 或 binary。

## Project VFS Mount

Project VFS Mount 是 Project 级单层实体，CRUD 路由为：

- `GET /api/projects/{project_id}/vfs-mounts`
- `POST /api/projects/{project_id}/vfs-mounts`
- `GET /api/projects/{project_id}/vfs-mounts/{mount_id}`
- `PUT /api/projects/{project_id}/vfs-mounts/{mount_id}`
- `DELETE /api/projects/{project_id}/vfs-mounts/{mount_id}`

`ProjectVfsMount.content` 只有两类：

- `Inline`：文件存储于 `inline_fs_files(owner_kind="project_vfs_mount", owner_id=mount.id, container_id="files")`。
- `ExternalService { service_id, root_ref }`：由对应 provider 解释 `root_ref`。

`mount_id` 是外部路径标识；数据库 UUID 只服务持久化和 inline storage owner。Project VFS Mount 不持有 `default_write`，workspace `main` 才是隐式写入目标。

## Runtime 工具

Agent 工具使用 mount-relative 参数模型：

```json
{ "mount": "main", "path": "relative/path" }
```

稳定工具集合：

- `mounts.list`
- `fs.read`
- `fs.write`
- `fs.apply_patch`
- `fs.list`
- `fs.search`
- `shell.exec`

`shell.exec` 只能作用于声明了 `exec` 能力的 mount。VFS URI 物化成本机路径时遵守 [vfs-materialization.md](./vfs-materialization.md)。

## 错误语义

| 条件 | 用户语义 |
| --- | --- |
| mount 不存在 | NotFound |
| path 非法或越界 | BadRequest |
| mount 不支持能力 | BadRequest / Forbidden |
| 文件不存在 | NotFound |
| 目标文件已存在 | Conflict |
| backend 离线 | ServiceUnavailable |
| provider 执行失败 | Internal |

用户可见错误不得暴露 `InlineContentOverlay`、`InlineFileRepository`、owner key 等内部实现名。

---

*创建：2026-04-17 — 统一 VFS 跨层契约*
*精简：2026-05-22 — 收束为地址、provider、surface mutation、inline storage 与 Project VFS Mount 核心契约*
