# Research: contracts-api-frontend-canvas-naming

- Query: Canvas 的 API / contract / frontend 对 `canvas_id`、`mount_id`、`vfs_mount_id`、`canvas://`、`cvs-...://` 的字段命名是否清晰一致；重点检查 `agentdash-contracts` Canvas DTO、Canvas API route、workspace module contracts、`packages/app-web` Canvas 类型和调用。
- Scope: internal
- Date: 2026-06-23

## Findings

### Files Found

- `.trellis/workflow.md` — Trellis workflow 要求 research 结果持久化到任务目录，并读取相关 spec 后形成结论。
- `.trellis/spec/cross-layer/frontend-backend-contracts.md` — 跨层契约已明确 Canvas workspace module 的 canonical 命名：`canvas:{mount_id}`、`canvas://{mount_id}`、`cvs-<mount_id>://...`。
- `.trellis/spec/backend/capability/capability-dimension-pipeline.md` — 说明 Canvas runtime exposure 同时产生 workspace module ref 与 Canvas VFS exposure。
- `.trellis/spec/backend/vfs/architecture.md` — VFS 统一地址模型规定外部访问由 `surface_ref + mount_id + mount_relative_path` 表达。
- `crates/agentdash-contracts/src/surface/canvas.rs` — Canvas CRUD / runtime snapshot wire DTO。
- `crates/agentdash-contracts/src/surface/workspace_module.rs` — Workspace module descriptor / presentation DTO。
- `crates/agentdash-api/src/routes/canvases.rs` — Canvas HTTP route、DTO 映射、runtime snapshot / invoke route。
- `crates/agentdash-api/src/dto/canvas.rs` — Canvas route-local query/request DTO。
- `crates/agentdash-application/src/canvas/management.rs` — Canvas 创建、`mount_id` 派生、`load_canvas_by_ref` 多义解析。
- `crates/agentdash-application/src/canvas/tools.rs` — Agent-facing Canvas create/bind 工具参数和结果。
- `crates/agentdash-application/src/workspace_module/mod.rs` — Canvas workspace module id、presentation URI、runtime backing 构造。
- `crates/agentdash-application/src/vfs/mount_canvas.rs` — Canvas VFS mount id、root ref、metadata 构造。
- `packages/app-web/src/generated/canvas-contracts.ts` — 由 Rust Canvas DTO 生成的 TS wire 类型。
- `packages/app-web/src/generated/workspace-module-contracts.ts` — 由 workspace module contract 生成的 TS wire 类型。
- `packages/app-web/src/types/canvas.ts` — 前端 Canvas 类型 facade，直接 re-export generated DTO。
- `packages/app-web/src/services/canvas.ts` — Canvas API client，参数名统一叫 `canvasId`。
- `packages/app-web/src/features/workspace-module/model/presentation.ts` — 前端 workspace module presentation 解析和 tab target。
- `packages/app-web/src/features/workspace-panel/model/canvasModuleOpen.ts` — Canvas presentation URI 与 runtime surface VFS mount 的对齐逻辑。
- `packages/app-web/src/features/workspace-panel/tab-types/canvas-tab.tsx` — `canvas://...` tab URI 解析并传给 Canvas runtime panel。
- `packages/app-web/src/features/canvas-panel/CanvasRuntimePanel.tsx` — Canvas runtime panel 用 `canvasId` 拉取 Canvas 和 runtime snapshot。
- `packages/app-web/src/features/canvas-panel/CanvasRuntimePreview.tsx` — Canvas iframe runtime invoke 使用 `snapshot.canvas_id`。
- `packages/app-web/src/features/canvas-panel/ProjectCanvasManager.tsx` — 项目 Canvas 管理页使用 UUID `canvas.id` 作为 selected / API 参数。
- `packages/app-web/src/pages/AgentRunWorkspacePage.workspace-module.test.ts` — Workspace module presentation 和 Canvas tab 行为测试。

### Current Field And Semantic Mapping

当前系统实际存在三种身份，但字段名没有完全把它们分开：

| 当前表达 | 当前语义 | 证据 |
| --- | --- | --- |
| `Canvas.id` / `CanvasResponse.id` | Canvas 数据库 UUID / 平台资产主键 | Domain entity 同时有 `id` 和 `mount_id`，见 `crates/agentdash-domain/src/canvas/entity.rs:12`；Canvas DTO 返回 `id` 与 `mount_id`，见 `crates/agentdash-contracts/src/surface/canvas.rs:40` 和 `crates/agentdash-contracts/src/surface/canvas.rs:43`。 |
| `Canvas.mount_id` / `CanvasResponse.mount_id` | Canvas 资产的项目内稳定 slug；不是最终 VFS mount id | 创建请求可传 `mount_id`，见 `crates/agentdash-contracts/src/surface/canvas.rs:56` 和 `crates/agentdash-contracts/src/surface/canvas.rs:59`；未传时从标题派生，见 `crates/agentdash-application/src/canvas/management.rs:115` 和 `crates/agentdash-application/src/canvas/management.rs:117`。 |
| Route path `/canvases/{id}` | 名叫 `id`，实际被 `load_canvas_by_ref` 解析为 UUID 或 mount id | API route 使用 `Path(id)`，见 `crates/agentdash-api/src/routes/canvases.rs:135`、`crates/agentdash-api/src/routes/canvases.rs:147`、`crates/agentdash-api/src/routes/canvases.rs:319`；application 先 `Uuid::parse_str`，否则 `find_by_mount_id`，见 `crates/agentdash-application/src/canvas/management.rs:69`、`crates/agentdash-application/src/canvas/management.rs:73`、`crates/agentdash-application/src/canvas/management.rs:76`。 |
| `CanvasRuntimeSnapshotDto.canvas_id` | 当前是 Canvas UUID 字符串 | DTO 定义见 `crates/agentdash-contracts/src/surface/canvas.rs:139` 和 `crates/agentdash-contracts/src/surface/canvas.rs:140`；API 映射 `snapshot.canvas_id.to_string()`，见 `crates/agentdash-api/src/routes/canvases.rs:379` 和 `crates/agentdash-api/src/routes/canvases.rs:383`。 |
| Agent tool `StartCanvasParams.canvas_id` | 名叫 `canvas_id`，实际表示 Canvas mount slug；存在则按 project + mount id 查找或创建 | 参数注释说 stable canvas identifier，字段名 `canvas_id`，见 `crates/agentdash-application/src/canvas/tools.rs:15` 和 `crates/agentdash-application/src/canvas/tools.rs:17`；实现用 `get_by_mount_id(project_id, &canvas_id)`，见 `crates/agentdash-application/src/canvas/tools.rs:72`。 |
| Agent tool result `CanvasToolResult.canvas_id` | 名叫 `canvas_id`，实际返回 `canvas.mount_id` | 结果字段定义见 `crates/agentdash-application/src/canvas/tools.rs:36`；赋值为 `canvas.mount_id.clone()`，见 `crates/agentdash-application/src/canvas/tools.rs:148`。 |
| Agent tool result `CanvasToolResult.mount_id` | 名叫 `mount_id`，实际返回 VFS mount id `cvs-{canvas.mount_id}` | 结果字段定义见 `crates/agentdash-application/src/canvas/tools.rs:37`；赋值为 `build_canvas_mount_id(&canvas)`，见 `crates/agentdash-application/src/canvas/tools.rs:149`。 |
| `workspace module_id = canvas:{mount_id}` | Workspace module ref，用 Canvas asset slug 组成 | Contract 注释见 `crates/agentdash-contracts/src/surface/workspace_module.rs:59`；application 常量和构造见 `crates/agentdash-application/src/workspace_module/mod.rs:170` 和 `crates/agentdash-application/src/workspace_module/mod.rs:440`。 |
| `WorkspaceModuleUiEntry.presentation_uri = canvas://{mount_id}` | WorkspacePanel Canvas tab 展示 URI，用 Canvas asset slug 组成 | Contract 注释见 `crates/agentdash-contracts/src/surface/workspace_module.rs:82`；application 构造见 `crates/agentdash-application/src/workspace_module/mod.rs:429`。 |
| `canvas://{uuid}` | Canvas VFS mount root_ref 内部定位 Canvas entity | VFS mount root ref 构造为 `canvas://{canvas.id}`，见 `crates/agentdash-application/src/vfs/mount_canvas.rs:17`。这与前端 `canvas://{mount_id}` 展示 URI 共享 scheme，但 payload 语义不同。 |
| VFS mount id `cvs-{mount_id}` | 真正进入 VFS 的 Canvas mount id；VFS URI 应为 `cvs-{mount_id}://path` | `build_canvas_mount_id` 返回 `cvs-{canvas.mount_id}`，见 `crates/agentdash-application/src/vfs/mount_canvas.rs:8` 和 `crates/agentdash-application/src/vfs/mount_canvas.rs:9`；mount `id` 使用该值，见 `crates/agentdash-application/src/vfs/mount_canvas.rs:14`。 |
| VFS mount metadata `canvas_id` / `mount_id` | metadata 中同时携带 Canvas UUID 和 Canvas asset slug | Metadata 写入 `canvas_id = canvas.id`、`mount_id = canvas.mount_id`，见 `crates/agentdash-application/src/vfs/mount_canvas.rs:31` 和 `crates/agentdash-application/src/vfs/mount_canvas.rs:32`。 |
| Frontend `CanvasRuntimePanelProps.canvasId` | 多义：项目管理页传 UUID，Workspace tab 传 Canvas mount slug | 管理页传 `selectedCanvas.id`，见 `packages/app-web/src/features/canvas-panel/ProjectCanvasManager.tsx:337` 和 `packages/app-web/src/features/canvas-panel/ProjectCanvasManager.tsx:338`；Canvas tab 从 `canvas://...` 切出 `canvasId` 后传入 panel，见 `packages/app-web/src/features/workspace-panel/tab-types/canvas-tab.tsx:11`、`packages/app-web/src/features/workspace-panel/tab-types/canvas-tab.tsx:13`、`packages/app-web/src/features/workspace-panel/tab-types/canvas-tab.tsx:47`。 |
| Frontend runtime surface matching | 把 VFS mount id `cvs-*` 剥回 Canvas mount slug，再和 `canvas://{mount_id}` 对齐 | `activeCanvasMountIdsFromRuntimeSurface` 剥 `cvs-` 前缀，见 `packages/app-web/src/features/workspace-panel/model/canvasModuleOpen.ts:29` 和 `packages/app-web/src/features/workspace-panel/model/canvasModuleOpen.ts:37`；`canvasMountIdFromPresentationUri` 从 `canvas://` 提取 mount slug，见 `packages/app-web/src/features/workspace-panel/model/canvasModuleOpen.ts:23`。 |

### Current Alignment With Specs

- Spec 已经清楚规定 Workspace Module Presentation Contract：Canvas module id 是 `canvas:{mount_id}`，Canvas UI entry 是 `view_key="preview"` 和 `presentation_uri="canvas://{mount_id}"`，Canvas VFS edit URI 是 `cvs-<mount_id>://...`，frontend 只从 `presentation_uri` 打开 Canvas tab，见 `.trellis/spec/cross-layer/frontend-backend-contracts.md:317`、`.trellis/spec/cross-layer/frontend-backend-contracts.md:319`、`.trellis/spec/cross-layer/frontend-backend-contracts.md:320`、`.trellis/spec/cross-layer/frontend-backend-contracts.md:322`。
- Capability spec 也明确把 workspace module ref 和 Canvas VFS exposure 分成两个面：`canvas:{mount_id}` 让 operation / UI entry 可见，`cvs-<mount_id>://...` 让文件面可见，见 `.trellis/spec/backend/capability/capability-dimension-pipeline.md:121`。
- 后端 `workspace_module` projection 基本符合这份 spec：Canvas entry 用 `canvas://{canvas.mount_id}`，module id 用 `canvas:{canvas.mount_id}`，见 `crates/agentdash-application/src/workspace_module/mod.rs:429` 和 `crates/agentdash-application/src/workspace_module/mod.rs:440`。
- 前端 workspace module presentation 也按 spec 打开：Canvas renderer 要求 concrete `canvas://...`，并直接把 `presentation_uri` 作为 Canvas tab URI，见 `packages/app-web/src/features/workspace-module/model/presentation.ts:56`；对应测试覆盖“不从 view_key 或 module_id 推断 URI”和“不解析 legacy uri fallback”，见 `packages/app-web/src/pages/AgentRunWorkspacePage.workspace-module.test.ts:145` 和 `packages/app-web/src/pages/AgentRunWorkspacePage.workspace-module.test.ts:154`。

### Naming Confusion And Legacy/Compatibility Points

1. `canvas_id` 在不同层含义冲突。
   - Runtime snapshot DTO 的 `canvas_id` 是 UUID，见 `crates/agentdash-contracts/src/surface/canvas.rs:140`。
   - Agent tool input / output 的 `canvas_id` 是 Canvas mount slug，见 `crates/agentdash-application/src/canvas/tools.rs:17`、`crates/agentdash-application/src/canvas/tools.rs:148`、`crates/agentdash-application/src/canvas/tools.rs:178`。
   - 前端 `CanvasRuntimePreview` 用 `snapshot.canvas_id` 调 Canvas invoke，当前因为 snapshot 里是 UUID 才成立，见 `packages/app-web/src/features/canvas-panel/CanvasRuntimePreview.tsx:214`。同名字段如果来自 Agent tool result 会语义相反。

2. `mount_id` 同时指 Canvas asset slug 和 VFS mount id。
   - CanvasResponse / CreateCanvasRequest 的 `mount_id` 是 Canvas stable slug，见 `crates/agentdash-contracts/src/surface/canvas.rs:43` 和 `crates/agentdash-contracts/src/surface/canvas.rs:59`。
   - Agent tool result 的 `mount_id` 是 `cvs-{slug}` VFS mount id，见 `crates/agentdash-application/src/canvas/tools.rs:149`。
   - Workspace module output schema 声称输出含 `mount_id`，但 bind tool result 里的 `mount_id` 也是 VFS mount id，见 `crates/agentdash-application/src/workspace_module/mod.rs:414` 和 `crates/agentdash-application/src/canvas/tools.rs:179`。
   - 前端需要在 runtime surface matching 中剥 `cvs-`，说明消费端被迫理解两套 id 空间，见 `packages/app-web/src/features/workspace-panel/model/canvasModuleOpen.ts:37`。

3. `/canvases/{id}` 是兼容式多义 ref。
   - API route path 名为 `id`，但 `load_canvas_by_ref` 同时接受 UUID 和 mount id，见 `crates/agentdash-api/src/routes/canvases.rs:135`、`crates/agentdash-api/src/routes/canvases.rs:319`、`crates/agentdash-application/src/canvas/management.rs:69`、`crates/agentdash-application/src/canvas/management.rs:73`、`crates/agentdash-application/src/canvas/management.rs:76`。
   - 前端因此把 `services/canvas.ts` 参数统一命名为 `canvasId`，但调用方可能传 UUID 或 mount slug，见 `packages/app-web/src/services/canvas.ts:28`、`packages/app-web/src/services/canvas.ts:46`、`packages/app-web/src/features/canvas-panel/ProjectCanvasManager.tsx:338`、`packages/app-web/src/features/workspace-panel/tab-types/canvas-tab.tsx:47`。

4. `canvas://` scheme 有两种 payload 语义。
   - WorkspacePanel 展示 URI 是 `canvas://{mount_id}`，见 `crates/agentdash-application/src/workspace_module/mod.rs:429`。
   - VFS Canvas provider root_ref 是 `canvas://{canvas.id}`，见 `crates/agentdash-application/src/vfs/mount_canvas.rs:17`。
   - 这两个 URI 不在同一入口暴露，但 scheme 相同会让文档和调试输出容易混淆。

5. 仍有 `canvas://...` 被当作 generic VFS mount URI 示例的遗留测试。
   - VFS apply_patch 测试示例使用 `canvas://src/view.tsx`、`canvas://src/new.rs`，见 `crates/agentdash-application/src/vfs/tools/fs/apply_patch.rs:247` 和 `crates/agentdash-application/src/vfs/tools/fs/apply_patch.rs:283`。当前 canonical Canvas VFS edit URI 应该是 `cvs-<mount_id>://...`，这个示例不应继续误导。
   - 前端 VFS tab 注释已经使用 `cvs-test-canvas-001://src/main.tsx` 作为示例，见 `packages/app-web/src/features/workspace-panel/tab-types/vfs-tab.tsx:11`。

### Recommended One-Time Breaking Cleanup Before Launch

推荐把三类身份显式命名，避免任意字段或 route 参数继续接受“UUID 或 mount slug”：

| Canonical name | Meaning | Wire examples |
| --- | --- | --- |
| `canvas_id` | Canvas 数据库 UUID / 平台资产主键，只用于 CRUD、权限、runtime actor 和 `CanvasRuntimeSnapshot` 的实体身份。 | `CanvasResponse.canvas_id`, `CanvasRuntimeSnapshot.canvas_id`, `/api/canvases/{canvas_id}` |
| `canvas_mount_id` | Canvas project-local stable slug；用于 workspace module ref、presentation URI payload、用户可读资产身份。 | `CanvasResponse.canvas_mount_id`, `CreateCanvasRequest.canvas_mount_id`, `canvas:{canvas_mount_id}`, `canvas://{canvas_mount_id}` |
| `vfs_mount_id` | 真正 VFS mount id；Canvas 的规则值是 `cvs-{canvas_mount_id}`。只在 VFS surface、tool diagnostics、文件浏览入口出现。 | `vfs_mount_id: "cvs-dashboard-a"`, `cvs-dashboard-a://src/main.tsx` |
| `canvas_asset_uri` / `canvas_provider_ref` | 如果内部 provider root ref 仍需要 URI，避免叫 `canvas://{uuid}` 或至少不要暴露到 browser / Agent contract；更清晰可改为 `canvas-asset://{canvas_id}` 或结构化 root ref。 | `root_ref: "canvas-asset://<uuid>"` |

具体破坏式整理建议：

1. Rust Canvas contract 重命名：
   - `CanvasResponse.id` -> `canvas_id`。
   - `CanvasResponse.mount_id` -> `canvas_mount_id`。
   - `CreateCanvasRequest.mount_id` -> `canvas_mount_id`。
   - `CanvasRuntimeSnapshotDto.canvas_id` 保持 UUID 语义，同时新增 `canvas_mount_id`；如果 Canvas runtime panel 还需要 VFS 文件入口，则显式返回 `vfs_mount_id`。
   - `CanvasRuntimeBindingDto.source_uri` 保持资源 URI；`data_path` 保持 Canvas runtime 内路径，不混入 VFS mount 身份。

2. Domain / DB 命名重整：
   - `Canvas.mount_id` 在 domain 上重命名为 `canvas_mount_id` 或 `mount_slug`。如果采用 `canvas_mount_id`，数据库列也应 migrate：`canvases.mount_id` -> `canvas_mount_id`，并同步 unique index / repository 方法名（如 `get_by_canvas_mount_id`）。
   - 保留 VFS 层 generic `Mount.id` / `mount_id`，因为 VFS API 需要统一处理所有 provider；但 Canvas VFS mount 构造函数和结果字段必须叫 `vfs_mount_id`。

3. API route 取消多义 ref：
   - CRUD / promote / runtime invoke 统一 `/api/canvases/{canvas_id}`，path 参数只接受 UUID。
   - 如果 workspace tab 从 `canvas://{canvas_mount_id}` 需要直接拉 runtime snapshot，提供显式 mount slug 入口，例如 `GET /api/projects/{project_id}/canvases/by-mount/{canvas_mount_id}` 或让 `workspace_module_present` payload 返回 `canvas_id`。
   - 删除 `load_canvas_by_ref` 的 UUID-or-mount 双解析；改成 `load_canvas_by_id` 和 `load_canvas_by_mount_id` 两个显式函数。

4. Workspace module contract 保持当前 spec 方向，但补充显式 diagnostics：
   - `module_id` 继续是 `canvas:{canvas_mount_id}`。
   - `presentation_uri` 继续是 `canvas://{canvas_mount_id}`。
   - Canvas presentation payload / diagnostics 可显式带 `canvas_id`、`canvas_mount_id`、`vfs_mount_id`，避免前端从 URI 和 runtime surface 反推。
   - `runtime_backing: "canvas:cvs-..."` 不够自描述；建议改为结构化 `runtime_backing: { kind: "canvas_vfs", vfs_mount_id }`，或至少字符串改为 `canvas_vfs:{vfs_mount_id}`。

5. Agent-facing tool schema 重命名：
   - `StartCanvasParams.canvas_id` -> `canvas_mount_id`（如果 Agent 只能按 slug attach/create）。
   - Tool result `canvas_id` 返回 UUID；另给 `canvas_mount_id` 和 `vfs_mount_id`。
   - `BindCanvasDataParams.canvas_id` 如果继续按 slug 操作则改为 `canvas_mount_id`；如果按 UUID 操作则改 repository 查询为 UUID 并另设可选 `canvas_mount_id` 入口。
   - Workspace module output schema 中的 `mount_id` 改为 `vfs_mount_id`，并新增 `canvas_mount_id`。

6. URI scheme 边界：
   - `canvas://{canvas_mount_id}` 只用于 presentation / WorkspacePanel tab。
   - `cvs-{canvas_mount_id}://path` 只用于 VFS file edit/read/browse。
   - 内部 Canvas provider root ref 不使用 `canvas://{uuid}` 暴露给任意 contract；改为非展示 scheme 或结构化 root ref，避免和 tab URI 同 scheme 异义。
   - 所有 VFS docs/tests 示例从 `canvas://src/...` 改为 `cvs-demo://src/...`。

### TS Types / Frontend Calls / Tests To Update

1. Generated TS / type facade:
   - Regenerate `packages/app-web/src/generated/canvas-contracts.ts` so `CanvasResponse` exposes `canvas_id` and `canvas_mount_id` instead of `id` / `mount_id` where applicable; current generated shape见 `packages/app-web/src/generated/canvas-contracts.ts:12`。
   - Regenerate `packages/app-web/src/generated/workspace-module-contracts.ts` if presentation diagnostics or `runtime_backing` shape changes; current `WorkspaceModulePresentation` shape见 `packages/app-web/src/generated/workspace-module-contracts.ts:66`。
   - Update `packages/app-web/src/types/canvas.ts` facade re-exports if aliases remain; current facade直接映射 generated DTO，见 `packages/app-web/src/types/canvas.ts:31` 和 `packages/app-web/src/types/canvas.ts:55`。

2. Canvas service signatures:
   - `fetchCanvas(canvasId)`, `updateCanvas(canvasId)`, `deleteCanvas(canvasId)`, `fetchCanvasRuntimeSnapshot(canvasId)`, `invokeCanvasRuntimeAction(canvasId)` should use `canvasId` only for UUID route calls, or be renamed to `fetchCanvasById(canvasId)` / `fetchCanvasByMountId(projectId, canvasMountId)` where both are needed. Current ambiguous signatures见 `packages/app-web/src/services/canvas.ts:28`、`packages/app-web/src/services/canvas.ts:46`、`packages/app-web/src/services/canvas.ts:77`。

3. Canvas runtime panel / tab:
   - `CanvasRuntimePanelProps.canvasId` should become either `canvasId` UUID only or split into `{ canvasId?: string; canvasMountId?: string }` with explicit service calls. Current prop and fetch usage见 `packages/app-web/src/features/canvas-panel/CanvasRuntimePanel.tsx:12`、`packages/app-web/src/features/canvas-panel/CanvasRuntimePanel.tsx:49`、`packages/app-web/src/features/canvas-panel/CanvasRuntimePanel.tsx:50`。
   - `canvas-tab.tsx` currently parses `canvas://...` into a variable named `canvasId`; rename to `canvasMountId`, and do not pass it to UUID-only Canvas APIs without an explicit resolver. Current parsing见 `packages/app-web/src/features/workspace-panel/tab-types/canvas-tab.tsx:11`、`packages/app-web/src/features/workspace-panel/tab-types/canvas-tab.tsx:13`、`packages/app-web/src/features/workspace-panel/tab-types/canvas-tab.tsx:47`。
   - `CanvasRuntimePanel` file browse button currently derives `cvs-${canvas.mount_id}`; after DTO rename it should use `snapshot.vfs_mount_id` or `canvas.vfs_mount_id` rather than formatting on the frontend，当前见 `packages/app-web/src/features/canvas-panel/CanvasRuntimePanel.tsx:88`。

4. Workspace module open logic:
   - `canvasMountIdFromPresentationUri` naming is good; keep it and ensure downstream variables stay `canvasMountId`, not `canvasId`,见 `packages/app-web/src/features/workspace-panel/model/canvasModuleOpen.ts:23`。
   - `activeCanvasMountIdsFromRuntimeSurface` should preferably compare `vfs_mount_id` from presentation diagnostics / module descriptor instead of stripping `cvs-`; if stripping remains, isolate it in one helper and name output `canvasMountIds` explicitly，当前剥离逻辑见 `packages/app-web/src/features/workspace-panel/model/canvasModuleOpen.ts:29` 和 `packages/app-web/src/features/workspace-panel/model/canvasModuleOpen.ts:37`。

5. Project Canvas manager:
   - Selection state `selectedCanvasId` is currently UUID and fine, but after DTO rename should read `canvas.canvas_id` rather than `canvas.id`; current uses见 `packages/app-web/src/features/canvas-panel/ProjectCanvasManager.tsx:26`、`packages/app-web/src/features/canvas-panel/ProjectCanvasManager.tsx:51`、`packages/app-web/src/features/canvas-panel/ProjectCanvasManager.tsx:338`。
   - UI labels showing `mount: {canvas.mount_id}` should become `mount: {canvas.canvas_mount_id}` or `canvas mount: ...`，当前见 `packages/app-web/src/features/canvas-panel/ProjectCanvasManager.tsx:269`。

6. Tests:
   - Update backend contract generation checks to assert `CanvasResponse.canvas_id`, `CanvasResponse.canvas_mount_id`, `CanvasRuntimeSnapshotDto.canvas_id`, `CanvasRuntimeSnapshotDto.canvas_mount_id`, and `vfs_mount_id` where returned.
   - Update workspace module tests to keep asserting Canvas tabs open from `presentation_uri` only; existing tests cover this behavior，见 `packages/app-web/src/pages/AgentRunWorkspacePage.workspace-module.test.ts:126`、`packages/app-web/src/pages/AgentRunWorkspacePage.workspace-module.test.ts:145`、`packages/app-web/src/pages/AgentRunWorkspacePage.workspace-module.test.ts:154`。
   - Update tests that model runtime surface Canvas mount ids (`cvs-mount-a`) so expected field names say `vfs_mount_id` and `canvas_mount_id` explicitly，current fixture见 `packages/app-web/src/pages/AgentRunWorkspacePage.workspace-module.test.ts:521`。
   - Update VFS apply_patch tests / examples that still use `canvas://src/...` as a mount URI，见 `crates/agentdash-application/src/vfs/tools/fs/apply_patch.rs:247`、`crates/agentdash-application/src/vfs/tools/fs/apply_patch.rs:256`、`crates/agentdash-application/src/vfs/tools/fs/apply_patch.rs:283`。
   - Run `pnpm run contracts:check`, `pnpm run frontend:check`, and targeted Rust checks for `agentdash-contracts`, `agentdash-api`, `agentdash-application` after the destructive rename.

## Caveats / Not Found

- No external references were needed; this was an internal contract/code research pass.
- `vfs_mount_id` is not currently a first-class Canvas contract field. It is implicit through `cvs-{canvas.mount_id}` construction or appears as a generic VFS `mount.id`; the name only appears as recommended target terminology in this research.
- I did not inspect database migrations or repository implementations beyond the code needed to understand naming semantics. If adopting the destructive rename, schema/index/repository migration needs a separate implementation pass.
- I did not modify business code, generated TS, specs, or tests. This research file is the only written artifact.
