# Research: runtime bridge binding unification

- Query: Canvas runtime snapshot, runtime bridge, browser-side assets/data binding 是否存在重复或割裂的底层读取逻辑；候选统一为“动态 binding 底层服务”的接口、边界、风险、测试点、迁移点。
- Scope: mixed
- Date: 2026-06-23

## Findings

### Executive Summary

当前存在重复但尚未完全割裂：

- Canvas data binding 的底层读取已经集中在 `resolve_canvas_binding_files -> parse_mount_uri -> VfsService.read_text`，但它被两个路径分别调用：runtime snapshot 生成路径调用一次，session live Canvas mount exposure 路径再调用一次。
- Browser-side `window.agentdash.assets.url(...)` 不复用 Canvas binding resolver；它在前端解析 `<mount>://<path>`，再调用通用 `/vfs-surfaces/read-file-blob`。这与 binding 同属 Session runtime VFS 读取，但传输形态是二进制 blob，职责和安全边界不同。
- Runtime bridge action invocation 不读取 VFS 文件；它只把 iframe 的 `action_key + input` 交给父页面/API，由 API 组装 `RuntimeActor::UserCanvas` 和 `RuntimeContext::Session` 后进入 `RuntimeGateway.invoke`。因此 runtime bridge action 与 binding 不应共享“action 执行”实现，但可以共享同一份 Canvas runtime context / surface resolution admission。
- Extension `canvas_panel` 加载的是 packaged artifact 内的 Canvas runtime snapshot，并复用 `CanvasRuntimePreview`。它会用当前 workspace session 覆盖 snapshot 的 `session_id`，但 packaged snapshot 是否携带可用 `resource_surface_ref` / runtime bridge surface 取决于打包内容；这是后续统一时需要单独处理的迁移点。

结论：可以收敛成一个 application 层 `CanvasRuntimeBindingService` / `CanvasRuntimeResourceService`，底层统一负责 Session runtime VFS 解析、Canvas binding text resolution、Canvas mount generated binding metadata refresh、browser asset blob read admission。浏览器公开 API 仍建议保持 `invoke`、`assets.url`、imported binding files 三种语义分工。

### Files Found

| Path | Description |
| --- | --- |
| `crates/agentdash-application/src/canvas/runtime.rs` | Canvas runtime snapshot 与 binding text resolve 现有实现。 |
| `crates/agentdash-api/src/routes/canvases.rs` | Canvas CRUD、runtime snapshot route、runtime-invoke route、RuntimeGateway surface assembly。 |
| `crates/agentdash-application/src/session/capability_service.rs` | Canvas expose/present 时追加 Canvas mount，并刷新 generated binding files 到 live VFS metadata。 |
| `crates/agentdash-application/src/vfs/mount_canvas.rs` | `cvs-<mount_id>` mount 构建与 `binding_files` metadata 注入。 |
| `crates/agentdash-application/src/vfs/provider_canvas.rs` | `canvas_fs` provider 读取 Canvas files 与 metadata 中的 generated binding files，并拒绝直接写 generated binding file。 |
| `crates/agentdash-api/src/routes/vfs_surfaces.rs` | 通用 VFS surface text / blob read HTTP API，browser asset bridge 当前复用 blob read route。 |
| `packages/app-web/src/features/canvas-panel/CanvasRuntimePreview.runtime.ts` | iframe `window.agentdash` SDK、module bundling、binding files imported as modules、VFS image URI parsing/cache。 |
| `packages/app-web/src/features/canvas-panel/CanvasRuntimePreview.tsx` | 父页面处理 iframe runtime invoke、asset URL request、extension channel request。 |
| `packages/app-web/src/features/canvas-panel/CanvasRuntimePanel.tsx` | Canvas tab 加载 Canvas record 与 runtime snapshot，保存 binding 后重新拉取 snapshot。 |
| `packages/app-web/src/services/canvas.ts` | 前端 Canvas runtime-snapshot 和 runtime-invoke API client。 |
| `packages/app-web/src/services/vfs.ts` | 前端 VFS surface read/write/blob API client。 |
| `packages/app-web/src/features/extension-runtime/ui/ExtensionCanvasPanel.tsx` | `canvas_panel` extension tab 从 package artifact 加载 runtime snapshot 并复用 Canvas preview。 |
| `packages/app-web/src/features/extension-runtime/model/canvasBridge.ts` | Canvas extension channel bridge，复用 extension backend target 选择。 |
| `packages/app-web/src/features/extension-runtime/model/webviewBridge.ts` | Extension webview runtime action / channel / VFS text read-write bridge，以及 runtime surface mount/backend selector。 |
| `packages/app-web/src/features/vfs/vfs-browser-panel-policy.ts` | runtime surface 默认 mount / backend target 共享选择策略。 |
| `crates/agentdash-domain/src/canvas/skills/canvas-system/SKILL.md` | Canvas authoring skill，说明 data binding 与 runtime bridge 使用规则。 |
| `crates/agentdash-domain/src/canvas/skills/canvas-system/references/runtime-bridge.md` | Canvas browser SDK reference，说明 `invoke`、`assets.url`、VFS image assets 边界。 |

### Related Specs And Task Artifacts

- `.trellis/tasks/06-23-canvas-vfs-runtime-binding-convergence/prd.md:5` 要求收束 Canvas VFS、runtime snapshot、runtime bridge、API/前端之间的身份命名与动态资源读取语义。
- `.trellis/tasks/06-23-canvas-vfs-runtime-binding-convergence/prd.md:31` 明确要求 binding 文件、runtime snapshot、runtime bridge asset/data 读取共享一个 application 层动态资源解析服务。
- `.trellis/tasks/06-23-canvas-vfs-runtime-binding-convergence/prd.md:52` 把“runtime snapshot binding 文件与 runtime bridge 读取同一套动态 binding/source resolver”列为 acceptance criteria。
- `.trellis/spec/backend/runtime-gateway.md:114` 规定 Canvas runtime bridge：iframe 只发送 `action_key + input`，actor/context 由父页面/API route 组装。
- `.trellis/spec/backend/vfs/vfs-access.md:10` 规定统一 VFS 地址模型是 `surface_ref + mount_id + mount_relative_path`。
- `.trellis/spec/backend/vfs/vfs-access.md:48` 规定 binary/blob read 边界；`.trellis/spec/backend/vfs/vfs-access.md:56` 规定 `/vfs-surfaces/read-file-blob` 直接返回 provider bytes 与 MIME。
- `.trellis/spec/backend/vfs/vfs-access.md:59` 规定 Canvas session visibility；`.trellis/spec/backend/vfs/vfs-access.md:65` 规定 runtime mount id 为 `cvs-<canvas.mount_id>`；`.trellis/spec/backend/vfs/vfs-access.md:70` 规定 presentation URI 为 `canvas://{mount_id}`。
- `.trellis/spec/frontend/architecture.md:14` 规定 Session workspace panel / context overview / VFS tab 以 `runtime_surface` 作为 runtime mount 展示与浏览能力唯一 UI 输入。
- `.trellis/spec/frontend/architecture.md:43` 规定 `canvas_panel` extension tab 在主前端读取 package artifact 内 Canvas runtime snapshot 并复用 `CanvasRuntimePreview`。
- `.trellis/spec/frontend/type-safety.md:63` 规定 Canvas 打开动作读取 generated event payload 的 `presentation_uri=canvas://{mount_id}`，不从 `view_key`、`module_id`、`cvs-...` 推断 tab URI。
- `.trellis/spec/cross-layer/frontend-backend-contracts.md:124` 规定 `canvas_panel` workspace tab renderer 复用 packaged panel asset 读取 contract，并复用现有 Canvas runtime preview。
- `.trellis/spec/cross-layer/frontend-backend-contracts.md:366` 规定 AgentRun runtime frame resolution 影响 Canvas runtime snapshot、Session control view、WorkspacePanel Canvas tab opening。

### Current Data Flow: Binding And Runtime Bridge

#### 1. Canvas Runtime Snapshot / Data Binding Flow

Current route:

```text
GET /canvases/{id}/runtime-snapshot?session_id=...
  -> load Canvas with Project view permission
  -> resolve_canvas_runtime_vfs(session_id)
       -> resolve_session_frame_vfs(...)
  -> build_runtime_snapshot_with_bindings(canvas, session_id, vfs, vfs_service)
       -> build_runtime_snapshot(...)
          - copies Canvas.files
          - inserts placeholder files for each CanvasDataBinding at bindings/<alias>.<ext>
       -> set resource_surface_ref = session-runtime:{session_id}
       -> resolve_canvas_binding_files(...)
          - unresolved_canvas_binding_files(...)
          - parse_mount_uri(binding.source_uri, vfs)
          - vfs_service.read_text(vfs, resource_ref, None, None)
       -> overwrite snapshot.files[data_path] when resolved
  -> build_canvas_runtime_bridge_surface(...)
       -> RuntimeGateway.surface_for_actor(UserCanvas, Session)
  -> CanvasRuntimeSnapshotDto
```

Code evidence:

- `crates/agentdash-api/src/routes/canvases.rs:328` calls `build_runtime_snapshot_with_bindings` from the runtime snapshot route.
- `crates/agentdash-api/src/routes/canvases.rs:336` attaches `snapshot.runtime_bridge` after binding snapshot construction.
- `crates/agentdash-application/src/canvas/runtime.rs:134` defines `build_runtime_snapshot_with_bindings`.
- `crates/agentdash-application/src/canvas/runtime.rs:145` sets `resource_surface_ref` to `session-runtime:{session_id}` when a VFS is available.
- `crates/agentdash-application/src/canvas/runtime.rs:153` calls `resolve_canvas_binding_files`.
- `crates/agentdash-application/src/canvas/runtime.rs:177` defines `resolve_canvas_binding_files`.
- `crates/agentdash-application/src/canvas/runtime.rs:187` calls `vfs_service.read_text(vfs, &resource_ref, None, None)` after `parse_mount_uri`.
- `crates/agentdash-domain/src/canvas/skills/canvas-system/SKILL.md:43` documents that preview and mount exposure time try to read each `source_uri` from session VFS.

Important behavior:

- Missing / invalid binding source is swallowed into unresolved placeholder state: `resolve_canvas_binding_files` continues on `parse_mount_uri` or `read_text` failure.
- Snapshot files contain actual text content for resolved bindings. The iframe imports `bindings/...` as normal snapshot modules, not live VFS reads.
- The snapshot only uses text reads. Binary images are explicitly out-of-scope for data binding.

#### 2. Canvas Session Exposure / Generated Binding Files Flow

Current route during workspace module create/present / Canvas exposure:

```text
SessionCapabilityService.expose_canvas_mount_revision_and_adopt(session_id, canvas)
  -> resolve current AgentFrame target from runtime session
  -> read current frame CapabilityState.vfs.active
  -> append_canvas_mounts(active_vfs, [canvas])
  -> resolve_canvas_binding_files(canvas, active_vfs, vfs_service)
  -> refresh_canvas_mount_binding_files(active_vfs, canvas, binding_files)
  -> write new AgentFrame revision and adopt it into active runtime
```

Then `canvas_fs` provider reads generated files:

```text
CanvasFsMountProvider.read_text(cvs-... mount, bindings/<alias>.<ext>)
  -> load Canvas record
  -> if path not in Canvas.files, merge unresolved_canvas_binding_files(canvas)
     with mount.metadata.binding_files
  -> return metadata-resolved binding content
```

Code evidence:

- `crates/agentdash-application/src/session/capability_service.rs:128` resolves binding files during Canvas exposure.
- `crates/agentdash-application/src/session/capability_service.rs:130` writes resolved files into active VFS mount metadata via `refresh_canvas_mount_binding_files`.
- `crates/agentdash-application/src/vfs/mount_canvas.rs:47` defines `refresh_canvas_mount_binding_files`.
- `crates/agentdash-application/src/vfs/mount_canvas.rs:60` stores serialized `binding_files` metadata.
- `crates/agentdash-application/src/vfs/provider_canvas.rs:66` implements Canvas provider `read_text`.
- `crates/agentdash-application/src/vfs/provider_canvas.rs:80` falls back from persisted Canvas files to `canvas_binding_files(mount, &canvas)`.
- `crates/agentdash-application/src/vfs/provider_canvas.rs:278` merges `unresolved_canvas_binding_files(canvas)` with mount metadata `binding_files`.
- `crates/agentdash-application/src/vfs/provider_canvas.rs:420` has a test asserting resolved binding files are exposed as read-only generated files.

Important behavior:

- This is a second caller of the same `resolve_canvas_binding_files` function.
- The generated binding files on `canvas_fs` are not truly read-time dynamic; they reflect the resolved content stored in mount metadata at exposure/adoption time.
- `canvas_fs` provider can expose read-only generated binding files, but it cannot itself resolve `source_uri` dynamically because the provider receives only its own Canvas mount, not the whole session VFS surface containing the source mount.

#### 3. Browser Runtime Bridge: `window.agentdash.invoke`

Current route:

```text
Canvas iframe
  -> window.agentdash.invoke(actionKey, input)
  -> postMessage { kind: "canvas-runtime-invoke", action_key, input }
Parent CanvasRuntimePreview
  -> validates frame generation
  -> requires snapshot.session_id
  -> checks action_key exists in snapshot.runtime_bridge.surface.actions
  -> POST /canvases/{id}/runtime-invoke { session_id, action_key, input }
API
  -> load Canvas with Project view permission
  -> build RuntimeInvocationRequest(
       RuntimeActor::UserCanvas { session_id, canvas_id },
       RuntimeContext::Session { session_id, project_id: canvas.project_id },
       input
     )
  -> RuntimeGateway.invoke(...)
```

Code evidence:

- `packages/app-web/src/features/canvas-panel/CanvasRuntimePreview.runtime.ts:183` injects `window.agentdash`.
- `packages/app-web/src/features/canvas-panel/CanvasRuntimePreview.runtime.ts:197` posts `canvas-runtime-invoke`.
- `packages/app-web/src/features/canvas-panel/CanvasRuntimePreview.tsx:200` checks action visibility against `snapshot.runtime_bridge.surface?.actions`.
- `packages/app-web/src/features/canvas-panel/CanvasRuntimePreview.tsx:214` calls `invokeCanvasRuntimeAction`.
- `packages/app-web/src/services/canvas.ts:76` defines `invokeCanvasRuntimeAction`.
- `packages/app-web/src/services/canvas.ts:81` posts to `/canvases/{id}/runtime-invoke`.
- `crates/agentdash-api/src/routes/canvases.rs:361` builds `RuntimeInvocationRequest`.
- `crates/agentdash-api/src/routes/canvases.rs:502` uses `runtime_gateway.surface_for_actor` to build the surface shown to Canvas.
- `crates/agentdash-domain/src/canvas/skills/canvas-system/references/runtime-bridge.md:25` says VFS image loading via `assets.url` is allowed in effects, while runtime actions should be user-triggered.

Important behavior:

- `invoke` is not a VFS read path. It should not be unified with binding at the operation level.
- It should share the same runtime context resolution / actor admission service so snapshot bridge surface and invoke route cannot drift.
- Current API route does not visibly call `resolve_canvas_runtime_vfs` or verify Canvas Project is visible in the session runtime VFS; it relies on permission + Gateway actor/context validation. Spec says API route must again validate Session and Canvas Project binding; implementation currently validates project permission and context project id, but not obviously Canvas visibility in the session frame in this route.

#### 4. Browser-Side VFS Image Assets: `window.agentdash.assets.url`

Current route:

```text
Canvas iframe
  -> window.agentdash.assets.url("mount://relative/path")
  -> postMessage { kind: "canvas-asset-url-request", uri }
Parent CanvasRuntimePreview
  -> requires snapshot.session_id and snapshot.resource_surface_ref
  -> resolveRuntimeAssetUrl(surfaceRef, uri, cache, readSurfaceFileBlob)
       -> parseVfsAssetUri(uri) in frontend
       -> readSurfaceFileBlob({ surfaceRef, mountId, path })
       -> reject non-image MIME in frontend
       -> createObjectURL(blob)
API /vfs-surfaces/read-file-blob
  -> parse surface_ref
  -> resolve_surface_bundle(...)
  -> vfs_service.read_binary(vfs, ResourceRef { mount_id, path }, None, current_user)
  -> return bytes with Content-Type
```

Code evidence:

- `packages/app-web/src/features/canvas-panel/CanvasRuntimePreview.runtime.ts:208` validates non-empty `agentdash.assets.url` URI.
- `packages/app-web/src/features/canvas-panel/CanvasRuntimePreview.runtime.ts:219` posts `canvas-asset-url-request`.
- `packages/app-web/src/features/canvas-panel/CanvasRuntimePreview.runtime.ts:44` defines `resolveRuntimeAssetUrl`.
- `packages/app-web/src/features/canvas-panel/CanvasRuntimePreview.runtime.ts:51` parses `uri` with `parseVfsAssetUri`.
- `packages/app-web/src/features/canvas-panel/CanvasRuntimePreview.runtime.ts:434` defines frontend VFS asset URI parser.
- `packages/app-web/src/features/canvas-panel/CanvasRuntimePreview.tsx:249` reads `snapshot.resource_surface_ref`.
- `packages/app-web/src/features/canvas-panel/CanvasRuntimePreview.tsx:262` calls `resolveRuntimeAssetUrl`.
- `packages/app-web/src/features/canvas-panel/CanvasRuntimePreview.tsx:266` passes `readSurfaceFileBlob`.
- `packages/app-web/src/services/vfs.ts:115` defines `readSurfaceFileBlob`.
- `packages/app-web/src/services/vfs.ts:120` posts to `vfsRoutes.surfaces.readFileBlob`.
- `crates/agentdash-api/src/routes/vfs_surfaces.rs:218` calls `vfs_service.read_binary`.
- `.trellis/spec/backend/vfs/vfs-access.md:56` documents this blob route as the binary transport boundary.

Important behavior:

- This path reuses the generic VFS surface blob API, not Canvas-specific binding resolution.
- Frontend duplicates some VFS mount URI validation (`parseVfsAssetUri`) that backend VFS normalization also performs; this is useful for quick client feedback but should not become the security boundary.
- Asset URL cache key is `surfaceRef + mountId + path`, which is correct for generation-scoped preview cache but is not a server-side binding cache.

#### 5. Extension `canvas_panel` Runtime Snapshot

Current route:

```text
WorkspacePanel ExtensionCanvasPanel
  -> resolveExtensionCanvasAvailability(...)
  -> buildExtensionWebviewAssetUrl(projectId, extensionKey, renderer.entry)
  -> authenticatedFetch(assetUrl)
  -> parse JSON as CanvasRuntimeSnapshot
  -> snapshot = { ...packagedSnapshot, session_id: workspaceData.sessionId ?? packagedSnapshot.session_id }
  -> <CanvasRuntimePreview snapshot={snapshot} extensionChannelBridge={...} />
```

Code evidence:

- `packages/app-web/src/features/extension-runtime/ui/ExtensionCanvasPanel.tsx:39` fetches the packaged Canvas snapshot through `authenticatedFetch`.
- `packages/app-web/src/features/extension-runtime/ui/ExtensionCanvasPanel.tsx:101` reuses `CanvasRuntimePreview`.
- `packages/app-web/src/features/extension-runtime/model/canvasBridge.ts:36` uses `selectExtensionBackendTarget` for extension channel invocation.
- `.trellis/spec/frontend/architecture.md:43` and `.trellis/spec/cross-layer/frontend-backend-contracts.md:124` explicitly require `canvas_panel` to reuse Canvas runtime preview.

Important behavior:

- `ExtensionCanvasPanel` only patches `session_id`; it does not recompute `resource_surface_ref` or `runtime_bridge.surface`.
- If a packaged snapshot was generated without session context, `assets.url` will still fail unless `resource_surface_ref` exists or preview adds a way to derive it from session id.
- Extension channel invocation has its own bridge and backend target selection; it shares runtime context concepts but not VFS binding read implementation.

### Is There Duplicate Or Split Low-Level Read Logic?

Yes, with three categories:

1. Same binding text resolver, duplicated callers:
   - Runtime snapshot route calls `build_runtime_snapshot_with_bindings`.
   - Session exposure/adoption calls `resolve_canvas_binding_files` and writes results into mount metadata.
   - Both use `parse_mount_uri -> VfsService.read_text`, so the bottom read is shared, but lifecycle timing and persistence differ.

2. Similar VFS source resolution, separate text/blob paths:
   - Binding text path uses `parse_mount_uri(source_uri, &Vfs)` and `read_text`.
   - Browser asset path uses frontend `parseVfsAssetUri`, then `/vfs-surfaces/read-file-blob`, then backend `read_binary`.
   - Both target the current Session runtime VFS surface, but the source address enters as `source_uri` for binding and as browser SDK `uri` for asset URL.

3. Runtime bridge context/surface assembly is separate from VFS binding resolution:
   - Snapshot route resolves session VFS and also separately builds runtime bridge surface.
   - Invoke route separately reconstructs actor/context.
   - There is no single `CanvasRuntimeContext` object that carries Canvas, session id, resolved VFS, `surface_ref`, actor/context, and bridge surface together.

### Candidate Interface: Dynamic Binding Bottom Service

Recommended application-layer service shape:

```rust
pub struct CanvasRuntimeContext {
    pub canvas: Canvas,
    pub session_id: String,
    pub project_id: Uuid,
    pub vfs: Vfs,
    pub resource_surface_ref: String,
    pub runtime_actor: RuntimeActor,
    pub runtime_context: RuntimeContext,
}

pub struct CanvasRuntimeBindingSource {
    pub alias: String,
    pub source_uri: String,
    pub data_path: String,
    pub content_type: String,
}

pub enum CanvasRuntimeResourceKind {
    BindingText,
    AssetBlob,
}

pub struct CanvasRuntimeTextRead {
    pub path: String,
    pub content: String,
    pub content_type: String,
    pub resolved: bool,
    pub attributes: serde_json::Map<String, serde_json::Value>,
}

pub struct CanvasRuntimeBlobRead {
    pub mount_id: String,
    pub path: String,
    pub bytes: Vec<u8>,
    pub mime_type: String,
}
```

Candidate narrow operations:

```rust
#[async_trait]
pub trait CanvasRuntimeBindingService {
    async fn resolve_context(
        &self,
        canvas: &Canvas,
        session_id: &str,
        identity: &AuthIdentity,
        permission: ProjectPermission,
    ) -> Result<CanvasRuntimeContext, ApiOrApplicationError>;

    async fn resolve_binding_files(
        &self,
        ctx: &CanvasRuntimeContext,
    ) -> Vec<CanvasResolvedBindingFile>;

    async fn read_binding_text(
        &self,
        ctx: &CanvasRuntimeContext,
        binding: &CanvasDataBinding,
    ) -> CanvasResolvedBindingFile;

    async fn read_vfs_text_uri(
        &self,
        ctx: &CanvasRuntimeContext,
        uri: &str,
        options: CanvasRuntimeReadOptions,
    ) -> Result<ReadResult, MountError>;

    async fn read_vfs_image_blob_uri(
        &self,
        ctx: &CanvasRuntimeContext,
        uri: &str,
    ) -> Result<BinaryReadResult, MountError>;

    fn build_snapshot(
        &self,
        ctx: Option<&CanvasRuntimeContext>,
        canvas: &Canvas,
        resolved_bindings: &[CanvasResolvedBindingFile],
        bridge_surface: Option<RuntimeSurface>,
    ) -> CanvasRuntimeSnapshot;

    fn apply_binding_files_to_canvas_mount(
        &self,
        vfs: &mut Vfs,
        canvas: &Canvas,
        binding_files: &[CanvasResolvedBindingFile],
    );
}
```

Alternative split if “binding” should stay text-only:

- `CanvasRuntimeContextResolver`: resolves Canvas + session into current frame VFS, `resource_surface_ref`, runtime actor/context, and bridge surface.
- `CanvasRuntimeBindingResolver`: text-only `source_uri -> CanvasResolvedBindingFile`.
- `CanvasRuntimeAssetResolver`: binary/image-only `uri -> BinaryReadResult`, sharing the same context resolver and mount URI parser.

Candidate contract/route consolidation:

- Add a Canvas-specific asset route only if browser asset reads require Canvas/session visibility checks that generic `/vfs-surfaces/read-file-blob` cannot enforce:

```text
POST /canvases/{id}/runtime-assets/read-blob
{ session_id, uri }
```

This route would:

- Load Canvas with Project view permission.
- Resolve `CanvasRuntimeContext`.
- Parse `uri` server-side with the same mount URI parser as binding.
- Call `read_binary`.
- Enforce `image/*` on the server before returning bytes.

If generic `/vfs-surfaces/read-file-blob` remains, at minimum `CanvasRuntimePreview` should call a shared frontend helper for VFS URI parsing, and server-side security must remain in vfs_surfaces route.

### Boundary When Runtime Bridge And Binding Share Bottom Implementation

Keep these boundaries:

- Shared:
  - Canvas + session permission/admission.
  - Current runtime frame VFS resolution.
  - `resource_surface_ref` generation.
  - mount URI parse/normalize semantics.
  - `VfsService.read_text` / `read_binary` dispatch.
  - generated binding file attributes (`alias`, `source_uri`, `content_type`, `resolved`, read-only marker).
  - bridge surface construction from `RuntimeGateway.surface_for_actor`.

- Not shared:
  - `window.agentdash.invoke` action execution must remain RuntimeGateway invocation, not VFS read.
  - Data binding must remain text-compatible and importable as snapshot/module files.
  - `assets.url` must remain blob/image URL oriented and should not inline bytes into JSON DTO.
  - Extension channel bridge remains extension runtime protocol, not Canvas binding.
  - Frontend object URL lifecycle/cache remains browser-only; backend service should return bytes/MIME, not browser URLs.

Recommended rule:

```text
Runtime bridge and binding share context + VFS resource resolution.
Runtime bridge action invocation, text binding materialization, and image blob serving stay separate operations.
```

This matches current specs:

- Runtime bridge action only accepts action key/input from iframe.
- VFS address model stays `surface_ref + mount_id + mount_relative_path`.
- Binary bytes stay out of JSON DTO and go through blob channel.

### Risks

- Dynamic read semantics may change user-visible freshness. Today generated binding files are exposure-time projection; making `canvas_fs` read binding files dynamically would make source file edits visible immediately. This is desired by PRD direction but needs tests for source mutations, deleted source files, and missing source behavior.
- `canvas_fs` provider currently cannot dynamically read a source URI from the broader session VFS, because its provider method receives only the Canvas mount. Making it dynamic requires either mount metadata to carry a resolved source surface/context handle, or moving generated binding path resolution out of provider into a higher-level VFS overlay/provider context.
- Server-side `read_vfs_image_blob_uri` must not trust frontend `parseVfsAssetUri`. The frontend parser is useful for UX, but backend must still parse/normalize and reject browser schemes, absolute paths, `..`, missing mount, non-image MIME.
- Unifying via generic `/vfs-surfaces/read-file-blob` may not prove Canvas visibility. It proves user permission for the `surface_ref`, but not necessarily that a specific Canvas id is entitled to read that surface. A Canvas-specific asset route would make that admission explicit.
- Extension `canvas_panel` packaged snapshots may lack live `resource_surface_ref` and bridge surface. Simply overriding `session_id` is insufficient for assets/runtime invoke if the snapshot was built at package time.
- `resolve_canvas_binding_files` currently swallows failures and returns unresolved placeholders. A stricter dynamic service must decide per operation whether unresolved is a value state, warning, or error.
- Reusing RuntimeGateway surface at snapshot time risks stale action visibility if capability state changes after snapshot. The invoke route re-enters Gateway, so backend remains authoritative, but frontend errors may differ from snapshot-visible actions.
- If service returns binary bytes through Canvas-specific route, response size and cache policy need attention; existing VFS blob route already streams bytes through Axum response.
- Current Canvas contract DTOs live in generated `canvas-contracts.ts`, but frontend `services/canvas.ts` imports them through `types/canvas.ts`. Any field rename must regenerate contracts and update type re-exports.

### Test Points

Backend/application:

- `build_runtime_snapshot_with_bindings` or replacement service resolves `bindings/<alias>.<ext>` from the same current session frame VFS used by Canvas exposure.
- Source text mutation after Canvas exposure follows chosen semantics:
  - Dynamic read: `cvs-...://bindings/<alias>...` reflects latest source without re-present.
  - Projection refresh: refreshed snapshot/exposure updates generated file metadata and content deterministically.
- Invalid `source_uri`, missing source file, and read failure produce JSON `null` for JSON binding and empty content for non-JSON text binding per current skill docs, or updated documented behavior.
- Non-text content type data binding is rejected before runtime snapshot or mount metadata generation.
- `canvas_fs` list/read/search includes generated binding files and metadata, and write/delete/rename rejects generated paths.
- Runtime snapshot route uses current adopted frame, not launch frame; this aligns with cross-layer frame resolution contract.
- Runtime invoke route validates session id, Canvas project/session relationship, and RuntimeGateway actor/context admission.
- Canvas-specific asset route, if added, rejects non-image MIME server-side and cannot read outside mount-relative paths.
- Generic `/vfs-surfaces/read-file-blob` remains covered for image MIME and binary read behavior if Canvas keeps using it.

Frontend:

- `CanvasRuntimePreview.runtime` imports resolved binding files by snapshot path and preserves JSON module behavior.
- `assets.url` rejects invalid VFS URIs and non-image blobs; existing tests already cover this in `CanvasRuntimePreview.test.ts`.
- `CanvasRuntimePreview` uses `snapshot.resource_surface_ref` for asset reads and reports missing session/surface clearly.
- Runtime invoke checks visible action list but still handles backend denial from Gateway.
- `ExtensionCanvasPanel` derives or fetches live `resource_surface_ref` / bridge surface when attaching packaged snapshots to a session, if runtime assets/actions are expected to work there.
- WorkspacePanel continues opening Canvas tabs from `presentation_uri=canvas://{mount_id}`, not from `cvs-...`.

Contract / generated:

- If DTOs change, run `pnpm run contracts:check`.
- Generated Canvas runtime snapshot DTO should not inline binary data.
- Any new Canvas runtime asset request DTO should use `session_id + uri` or `surface_ref + uri` explicitly; avoid exposing backend ids or local paths.

### Migration Points

- Move `resolve_canvas_binding_files` out of `canvas/runtime.rs` into a service module whose name reflects runtime resource/binding resolution, then make both snapshot route and session exposure call the service.
- Introduce a `CanvasRuntimeContextResolver` that returns current session VFS, `session-runtime:{id}` surface ref, runtime actor/context, and runtime bridge surface from one place.
- Change `SessionCapabilityService.expose_canvas_mount_revision_and_adopt` to call the new service rather than directly calling `resolve_canvas_binding_files + refresh_canvas_mount_binding_files`.
- Change `get_canvas_runtime_snapshot` to call the new service and avoid separately resolving VFS and bridge surface.
- Decide whether generated binding files remain mount metadata projections or become read-time dynamic:
  - Projection path is smaller and preserves current provider shape.
  - Dynamic path likely requires `canvas_fs` provider to receive enough session VFS context or for generated binding files to be implemented as an overlay provider that can resolve source URIs at read time.
- Consider replacing frontend-only `parseVfsAssetUri` with a shared helper under `features/workspace-runtime` or `features/vfs`, then use server-side validation as authority.
- For extension `canvas_panel`, add a session attach step that hydrates packaged snapshot with live `resource_surface_ref` and bridge surface, or declare packaged Canvas runtime bridge unavailable until a live Canvas context is resolved.
- Update `canvas-system` docs after implementation to describe the new positive semantics: whether binding files are dynamic read views or refreshed projections, and which route serves image assets.

### External References

- No external API/library documentation was needed for this research. The relevant contracts are internal Trellis specs and project code.
- Browser object URL behavior is used in existing frontend code via `URL.createObjectURL` / `URL.revokeObjectURL`; this research did not require version-specific browser documentation.

## Caveats / Not Found

- No `design.md` or `implement.md` exists yet for this task at research time; only `prd.md` was present.
- The active task command returned `(none)` for the current session, but the user provided the exact task directory and output file path; this file was written to that explicit path.
- I did not find a single existing service that already owns Canvas runtime context + binding text + image blob resolution together. The closest shared bottom layer is `VfsService.read_text/read_binary`.
- I did not verify runtime behavior by running tests; this was code/spec research only.
- I did not inspect every workspace module operation implementation for `canvas.bind_data`; this research focused on the requested runtime snapshot, bridge, browser asset/data binding, and canvas-system docs paths.
