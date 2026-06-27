# Canvas VFS 与 runtime binding 语义收束设计

## Architecture Direction

本任务把 Canvas 相关语义收束成三条稳定边界：

1. **Canvas Identity Boundary**
   - Canvas 数据库 UUID 只用于持久化与 API 资源定位，字段名固定为 `canvas_id`。
   - Canvas 项目内 mount key 使用强制 `cvs-` 前缀规则，字段名固定为 `canvas_mount_id`。
   - 对 Canvas 来说 `canvas_mount_id == vfs_mount_id`，不再维护未加前缀 key 与 VFS mount id 的二次派生。
   - Workspace module id、presentation URI、VFS URI 都从 Canvas identity helper 派生，不在调用点手写拼接。

2. **Canvas Runtime Resource Boundary**
   - Canvas data binding、runtime snapshot、runtime bridge context、runtime bridge asset/blob 读取共享一个 application 层 resource service。
   - Resource service 以 session active VFS 和 `VfsService` 为事实源，负责 URI 解析、文本内容读取、binary/blob 读取、content type 校验、generated file metadata。
   - Canvas VFS generated binding files 采用 read-time dynamic view；provider 需要通过上层注入的 runtime context 或 overlay/provider projection 访问同一 service。

3. **Contract Boundary**
   - Request DTO 允许输入缺省字段；response DTO 返回明确语义字段。
   - 前端消费 generated contract，不保留旧字段别名。
   - WorkspacePanel 仍只用 `presentation_uri` 打开 Canvas tab；Agent/VFS 编辑只用 `cvs...://...`。

## Identity Model

推荐方案：

| 概念 | 字段/格式 | 说明 |
| --- | --- | --- |
| Canvas database id | `canvas_id: Uuid` | 仅持久化/API 资源主键 |
| Canvas mount id | `canvas_mount_id = "cvs-<slug>"` | Canvas 项目内稳定 mount key，同时也是 VFS mount id |
| Canvas VFS mount id | `vfs_mount_id = canvas_mount_id` | VFS mount 的唯一 id |
| Canvas module id | `canvas:{canvas_mount_id}` | Workspace module id |
| Canvas presentation URI | `canvas://{canvas_mount_id}` | WorkspacePanel tab URI |
| Canvas VFS URI | `{canvas_mount_id}://path` | Agent/VFS authoring URI |
| Canvas provider root | `canvas-root://{canvas_id}` 或结构化 root | provider 内部 root identity，不使用 browser-facing `canvas://` |

迁移后不再存在 `dashboard-a -> cvs-dashboard-a` 的隐式派生，也不允许 `cvs-cvs-dashboard-a`。

## Canvas Runtime Resource Service

目标模块：

- `crates/agentdash-application/src/canvas/runtime_resource.rs`

核心类型：

- `CanvasRuntimeContext`：Canvas、session id、project id、active VFS、`resource_surface_ref`、RuntimeGateway actor/context、bridge surface。
- `CanvasRuntimeResourceService`：解析 Canvas runtime context、读取 binding text、读取 VFS asset blob、构建 generated binding metadata、构建 runtime snapshot。
- `CanvasResolvedBindingFile`：表示 `bindings/<alias>.<ext>` 的 text view 结果，来源由 service 统一读取。

职责：

- 规范化 binding alias、source URI、content type。
- 校验 text-compatible content type，拒绝二进制 data binding。
- 根据 binding 生成 `bindings/<alias>.<ext>`。
- 解析 `source_uri`，通过 `VfsService::read_text` 获取当前内容。
- 解析 asset URI，必要时通过 `VfsService::read_binary` 获取 blob，并在服务端校验 image MIME。
- 生成 read/list/search 需要的 generated/read-only metadata。
- 为 runtime snapshot、Canvas VFS generated files、runtime bridge assets 提供同一套 resolver。

## Runtime Bridge Integration

runtime bridge 公开的 browser API 保持分工：

- data binding：面向 importable text/data files。
- assets URL：面向图片/二进制/可渲染 asset。
- invoke：面向 runtime action。

服务端底层不分叉。`assets.url` 和 binding resolver 共享：

- VFS URI 解析。
- session active VFS 选择。
- 权限/可见性边界。
- source metadata、错误形态与权限/admission。

`window.agentdash.invoke` 不共享 VFS read 实现；它共享 `CanvasRuntimeContext` 与 RuntimeGateway surface/admission。Action execution 仍进入 `RuntimeGateway.invoke`。

## Dynamic Binding Shape

推荐实现为 read-time dynamic view：

- Runtime snapshot 可以 materialize 当次 binding 内容，供 iframe module import 使用。
- Canvas VFS generated binding files 在 read/list/search 时通过 `CanvasRuntimeResourceService` 读取当前 source。
- 如果 provider trait 不能直接访问完整 session VFS，则通过 session overlay/provider projection 或扩展 mount operation context 传入当前 runtime VFS context。
- 失败语义保持为值状态：JSON binding unresolved 时是 `null`，非 JSON text binding unresolved 时为空内容，同时 metadata 标记 `resolved=false`。

## Database And Repository

数据库建议：

- 将 `canvases.mount_id` 语义重置为 `canvas_mount_id`，值统一 `cvs-...`。
- 视实现成本决定是否物理改列名；若不改列名，domain/contract 必须明确 `canvas_mount_id`，避免向外暴露旧名。
- 新增 Project-scoped 唯一约束，防止并发创建同名 Canvas。
- 移除或收敛全局 `find_by_mount_id` 的生产路径。

## Trade-Offs

- **domain mount key 改成 `cvs-...`**：语义最统一，但会触及 API、frontend、workspace module、tests 的大范围重命名。本项目未上线，可以接受破坏式整理。
- **read-time dynamic binding**：最符合用户期望，但需要 provider read 上下文能访问 session active VFS，或把 generated binding files 实现为 session overlay/provider projection。
- **browser API 保持分工**：避免把 action invoke、text binding、asset blob 混成一个难理解 API；代价是服务端共享底层需要清晰模块边界。

## Compatibility

项目未上线，不设计旧字段、旧 URI、旧 `.json` binding 路径兼容层。数据库字段若需要改名或语义重置，应配套 migration。

## Research Inputs

- `research/backend-vfs-canvas-ids.md`
- `research/runtime-bridge-binding-unification.md`
- `research/contracts-api-frontend-canvas-naming.md`
