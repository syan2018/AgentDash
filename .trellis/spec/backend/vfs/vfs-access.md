# VFS Access

本 appendix 定义统一 VFS 地址、provider、surface mutation、inline storage 与 Project VFS Mount 契约。模块不变量见 [VFS Architecture](./architecture.md)。

## Address Model

外部访问地址统一为：

```text
surface_ref + mount_id + mount_relative_path
```

- `surface_ref` 解析出一份 runtime `Vfs`，例如 Project preview、Story preview、Task preview、Project VFS Mount、Project Agent Knowledge 或 Session runtime。
- `mount_id` 定位该 `Vfs` 内的 runtime mount，是 UI/API/Agent tool 的稳定 mount 标识。
- `mount_relative_path` 是 mount 根下相对路径，进入 application 层前必须 normalize；绝对路径和 `..` escape 必须失败。

Application 层内部可以继续使用更强类型表达地址，例如 `ResourceRef`、`VfsUri`、`RootRef::LocalPath | RootRef::ProviderUri`。原始字符串只停留在 UI/API/relay/tool 输入边界。

## Runtime Mount

runtime mount 是 provider 分发单位，至少表达：

- `id`
- `provider`
- `root_ref`
- `capabilities`
- `metadata`

`Vfs` 构建后必须 hard validate：mount id 唯一、default mount 存在、保留 mount id 没有被错误 provider 占用、root_ref/provider scheme 合法、capability 与 provider 支持范围一致、link target 存在且无环。

## Provider Responsibilities

Provider 负责读、列、搜索、stat、binary read 等数据访问。

Provider SPI 按职责暴露三组窄接口：`ProviderDescriptor` 描述元信息与可用性，`MountIo` 承载 read/write/list/stat/exec/patch，`MountSearch` 承载 search/suggest/grep。运行时 registry 仍以 composite `MountProvider` 存放 provider 对象，原因是分发路径需要同一个对象同时服务 discovery、IO 与搜索；业务调用点可按职责依赖窄 trait 面。

| Provider | 职责 |
| --- | --- |
| `relay_fs` | 通过 relay 访问本机 workspace 文件 |
| `inline_fs` | 暴露 Project / Story / Agent Knowledge 等内联文件 |
| `skill_asset_fs` | 暴露 Skill asset 文件视图，文件内容复用 InlineFile 存储 |
| `lifecycle_vfs` | 暴露 lifecycle run、node、artifact、record 投影 |
| `routine_vfs` | 暴露 Routine 当前触发投影、Routine 级 memory 与当前 entity memory |
| `canvas_fs` | 暴露 Canvas 相关虚拟内容 |

Provider 返回的 `RuntimeFileEntry.attributes` 是结构化 metadata 通道，例如 `content_kind`、`mime_type`、`skill_asset_file_kind`。不要把这类 metadata 塞进文件文本内容。

## Binary / Blob Read

`read_binary` 表示按 bytes/blob 传输给资产消费者，不表示该文件在编辑语义上一定不可作为文本读取。SVG 可以同时被 `read_text` 编辑、被 `read_binary` 作为 `image/svg+xml` 资产加载。

Contract:

- 云端 `RelayFsMountProvider::read_binary` 必须 normalize mount-relative path，再下发 `command.tool.file_read_binary`。
- 本机 `ToolExecutor::file_read_binary` 只在 `mount_root_ref` 对应的当前 workspace root 边界内解析路径，读取原始 bytes，并按文件资产类型返回 MIME。
- HTTP `/vfs-surfaces/read-file-blob` 直接返回 provider bytes 和 `Content-Type: result.mime_type`。
- Canvas asset URL 读取到非 `image/*` MIME 时，前端 runtime asset cache 必须拒绝资源。

## Canvas Session Visibility

Canvas 被 `workspace_module_create(kind="canvas")` 或 `workspace_module_present(module_id="canvas:{mount_id}")` 暴露给当前 session 后，前端从 Session runtime surface 浏览 `canvas_fs` mount。Canvas VFS 仍由 `canvas_fs` provider 管理；workspace module 只负责 Agent-facing lifecycle、operation 和 presentation 入口。

Contract:

- Runtime mount id: `cvs-<canvas.mount_id>`。
- Session meta 存储 `visible_canvas_mount_ids: Vec<String>`，值为未加 `cvs-` 前缀的 `canvas.mount_id`。
- `workspace_module_create(kind="canvas")` 返回 `canvas:{mount_id}` descriptor 前，先把目标 Canvas 追加到 live runtime VFS，把 `canvas.mount_id` 写入 `visible_canvas_mount_ids`，并同步刷新 `CapabilityState.vfs.active`。
- `workspace_module_present(module_id="canvas:{mount_id}")` 在发送 `workspace_module_presented` 展示事件前执行同一套 session exposure 逻辑。
- Canvas 可见后，状态更新服务必须从刷新后的 live VFS 重新 discovery Skill 维度，并写入 `CapabilityState.skill.skills`。
- `workspace_module_presented.presentation_uri` 使用 `canvas://{mount_id}`，用于打开 WorkspacePanel Canvas tab。
- Agent 编辑 Canvas 文件继续使用 `cvs-<mount_id>://...`；`canvas://{mount_id}` 不是 VFS 编辑 URI。
- 前端收到 `workspace_module_presented` 后刷新 session context，再按 `presentation_uri` 打开或继续使用 Canvas / VFS tab。

## Surface Mutation

Surface text mutation 与 inline binary upload 的统一入口是 application 层 mutation dispatcher。

Route handler 只负责：

- 权限检查
- `surface_ref` 解析与 `Vfs` resolution
- 请求 / 响应 DTO 转换

Dispatcher 负责：

- resolve runtime mount
- normalize mount-relative path
- 校验 mount capability 与 edit capability
- 分发到 provider 或 inline writer
- 把错误映射成用户语义

Route handler 不解析 inline owner 坐标，不直接操作 `inline_file_repo`，也不构造 `InlineContentOverlay`。

## Inline Storage Key

`inline_fs` 的持久化坐标只由 application resolver 从 runtime mount metadata 生成：

```rust
pub struct InlineStorageKey {
    pub owner_kind: InlineFileOwnerKind,
    pub owner_id: Uuid,
    pub container_id: String,
}
```

| 来源 | owner_kind | owner_id | container_id |
| --- | --- | --- | --- |
| Project VFS Mount | `project_vfs_mount` | `project_vfs_mount.id` | `files` |
| Project config inline container | `project` | `project.id` | container mount identity |
| Story inline container | `story` | `story.id` | container mount identity |
| Project Agent Knowledge | `project_agent` | `project_agent.id` | knowledge container identity |
| Routine memory | `routine` | `routine.id` | `memory` |
| Routine entity memory | `routine` | `routine.id` | `entity:{entity_key}` |

`container_id` 只表示 inline storage container。展示或 lineage 需要独立命名，例如 `context_container_id`。

## Inline Text And Binary

InlineFile 是 typed content storage：

```rust
pub enum InlineFileContent {
    Text { content: String },
    Binary { bytes: Vec<u8>, mime_type: String },
}
```

- text 文件走 `read_text` / `write_text` / `create_text` / `apply_patch`。
- binary 文件走 `read_binary` / blob upload；text API 读取 binary 必须失败。
- list / stat 暴露 `content_kind`、`mime_type`、`size`。
- blob upload 只允许 image MIME，并通过 mutation dispatcher 写入 inline storage。
- Agent `fs_read` 对 `image/*` binary 返回文本 metadata block + image block；非 image binary 返回 unsupported binary 语义。

## Skill Asset Files

Skill asset 文件内容存储在 InlineFile：

```text
owner_kind   = "skill_asset"
owner_id     = skill_assets.id
container_id = "files"
path         = Skill 根目录内相对路径
```

Skill 领域对象仍负责 `SKILL.md` 主文档、metadata validation、文件 kind 等业务语义。binary asset 在 JSON DTO 中只返回 metadata，不内联 bytes；`skill_asset_fs` 通过 provider 读取 text 或 binary。

## Routine Runtime Mount

Routine 触发的 session 使用 runtime mount 暴露跨轮次上下文：

```text
mount_id = "routine"
provider = "routine_vfs"
root_ref = "routine://routine/{routine_id}"
```

`routine_vfs` 的 `current/*` 路径来自当前 `RoutineExecution` 与 resolved prompt，是只读触发投影。`memory/*.md` 是 Routine 级长期 memory，当前 `entities/{entity_key}/*.md` 是 per-entity memory。写入只开放给 Routine 级 memory 与当前 entity memory，原因是 Agent 需要维护长期工作记忆，但触发事实仍应由后端事实源提供。

Routine memory 复用 InlineFile 存储，provider 由 mount metadata 中的 `routine_id`、`execution_id`、`trigger_source` 与 `entity_key` 解析当前投影和允许写入的 inline storage key。通用 VFS Browser 可通过 session runtime surface 消费该 mount；Routine 页面入口只需要跳转到同一 VFS surface。

## AgentRun Lifecycle Run Mount

AgentRun workspace surface 使用 run-scoped lifecycle mount 暴露 `LifecycleRun`、orchestration、runtime node、session projection 和 journey records 的只读浏览入口。该 mount 不依赖 active workflow projection 或 workflow graph；graphless run 仍能通过 `state`、`context`、`orchestrations` 和 `runs` 暴露控制面事实。

Run mount contract：

| 字段 | 来源 | 约束 |
| --- | --- | --- |
| `id` | 常量 | `lifecycle` |
| `provider` | 常量 | `lifecycle_vfs` |
| `root_ref` | builder | `lifecycle://run/{run_id}` |
| `metadata.run_id` | `LifecycleRun.id` | UUID string |
| `metadata.scope` | builder | `run` |

Provider run-scope 路径：

| 路径 | 行为 |
| --- | --- |
| `state` | 当前 `LifecycleRun` overview |
| `context` | `LifecycleRun.context` |
| `orchestrations` | 当前 run 的 orchestration 列表 |
| `orchestrations/{orchestration_id}/state` | 指定 orchestration 实例 |
| `orchestrations/{orchestration_id}/nodes` | 指定 orchestration 的 runtime node 列表 |
| `orchestrations/{orchestration_id}/nodes/{encoded_node_path}/state` | 指定 runtime node 状态 |
| `orchestrations/{orchestration_id}/nodes/{encoded_node_path}/session/*` | 指定 node 关联 session 的投影 |
| `orchestrations/{orchestration_id}/nodes/{encoded_node_path}/records/*` | 指定 node 的 journey records |
| `active/*` / `nodes/*` | provider 可从 run 中确定 active orchestration/node 时的便捷视角；`nodes` 只作为单 orchestration 或 node-scoped surface 的短路径 |
| `runs` / `runs/{run_id}` | 同 project run 列表与 run overview |

节点路径作为 VFS 路径段时使用 UTF-8 percent encode，原因是 `RuntimeNodeState.node_path` 可以包含 `/`，浏览器树形展开需要稳定的单段 key。

## Lifecycle Runtime Mount

Lifecycle runtime mount 暴露当前 lifecycle container 内部的 orchestration node 投影。它以 `orchestration_id + node_path + attempt` 作为运行节点身份；session assembly 通过 application 层 surface 传入以下字段：

```rust
pub struct LifecycleMountSurface<'a> {
    pub run_id: Uuid,
    pub orchestration_id: Uuid,
    pub node_path: &'a str,
    pub lifecycle_key: &'a str,
    pub attempt: u32,
    pub writable_port_keys: Vec<String>,
}
```

Runtime mount contract：

| 字段 | 来源 | 约束 |
| --- | --- | --- |
| `id` | 常量 | `lifecycle` |
| `provider` | 常量 | `lifecycle_vfs` |
| `root_ref` | builder | `lifecycle://run/{run_id}/orchestration/{orchestration_id}/node/{encoded_node_path}` |
| `metadata.run_id` | `LifecycleRun.id` | UUID string |
| `metadata.orchestration_id` | `OrchestrationInstance.orchestration_id` | UUID string |
| `metadata.node_path` | `RuntimeNodeState.node_path` | 非空 runtime path；作为路径段落存储时按 UTF-8 percent encode |
| `metadata.attempt` | `RuntimeNodeState.attempt` | `u32` |
| `metadata.lifecycle_key` | lifecycle definition label | 只用于展示和 prompt，不参与 runtime identity |
| `metadata.writable_port_keys` | plan/activity output ports | artifact 写入白名单 |

Provider 解析行为：

| 路径 | 行为 |
| --- | --- |
| `active` / `state` | 从 `LifecycleRun.orchestrations[]` 中定位 `orchestration_id + node_path + attempt` 并返回 runtime node/run 投影 |
| `artifacts/{port_key}` | 写入或读取 `InlineFileOwnerKind::LifecycleRun / port_outputs / {orchestration_id}/{encoded_node_path}/{attempt}/{port_key}` |
| `records/{name}` | 写入或读取当前 node 的 journey records |
| `session/*` | 通过 `RuntimeNodeState.executor_run_ref == RuntimeSession` 读取 session event/item/tool/summary 投影 |
| `nodes/{encoded_node_path}/*` | 读取同一 orchestration 内指定 runtime node 的 state/session/records |

Validation / errors：

| 条件 | 错误语义 |
| --- | --- |
| metadata 缺少 `run_id` / `orchestration_id` / `node_path` / `attempt` | `OperationFailed` |
| run 或 orchestration 不存在 | `NotFound` |
| node 不存在 | `NotFound` |
| artifact port 不在 `writable_port_keys` | `OperationFailed` |
| node 没有关联 runtime session 时读取 `session/*` | `NotFound` |

Tests required：

- mount builder test asserts `root_ref`、metadata 和 writable ports 使用 orchestration node 坐标。
- provider test writes `artifacts/{port_key}` and asserts inline path uses `{orchestration_id}/{encoded_node_path}/{attempt}/{port_key}`.
- frame/session assembly test asserts lifecycle node compose reads `RuntimeSessionExecutionAnchor` rather than frame graph/activity fields.

## Project VFS Mount

Project VFS Mount 是 Project 级单层实体，CRUD 路由为：

- `GET /api/projects/{project_id}/vfs-mounts`
- `POST /api/projects/{project_id}/vfs-mounts`
- `GET /api/projects/{project_id}/vfs-mounts/{mount_id}`
- `PUT /api/projects/{project_id}/vfs-mounts/{mount_id}`
- `DELETE /api/projects/{project_id}/vfs-mounts/{mount_id}`

`ProjectVfsMount.content` 只有两类：

- `Inline`
- `ExternalService { service_id, root_ref }`

`mount_id` 是外部路径标识；数据库 UUID 只服务持久化和 inline storage owner。Project VFS Mount 不持有 `default_write`，workspace `main` 才是隐式写入目标。

## Runtime Tools

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

`shell.exec` 只能作用于声明了 `exec` 能力的 mount。VFS URI 物化成本机路径时遵守 [VFS Materialization](./vfs-materialization.md)。

## Search Discovery Policy

Agent-facing glob / grep 从 mount root 进行默认扫描时，工作区文件发现应尊重 workspace ignore 文件与内置依赖、构建、缓存目录排除规则。这样默认搜索结果表达项目可维护内容，避免依赖包和生成产物挤占 Agent 上下文。

当调用方显式传入 `path` 指向普通 ignored subtree 时，该 subtree 表示用户的搜索目标，文件发现应允许进入。这样依赖包源码、构建产物和生成文件仍可在有明确意图时被检查。

VCS 元数据目录是 hard exclude：`.git`、`.svn`、`.hg`、`.bzr`、`.jj`、`.sl` 不参与 Agent-facing glob / grep 搜索。原因是这些目录表达版本控制内部状态，不是 VFS 搜索工具的默认工作区内容。

## Error Semantics

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
