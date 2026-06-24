# Research: Runtime VFS 与 WorkspaceModule map

- Query: Canvas personal/project-shared permission task 的 Runtime VFS 与 WorkspaceModule 实现地图，重点确认 read-only VFS capability、provider guard、WorkspaceModule descriptor/operation 裁切、invoke 防线、worker 文件边界与 Phase A access projection 依赖。
- Scope: internal
- Date: 2026-06-24

## Findings

### Files Found

| Path | Description |
| --- | --- |
| `.trellis/tasks/06-24-canvas-personal-shared-distribution-permission/prd.md` | 要求项目共用 Canvas 的 VFS mount 只暴露 read/list/search，WorkspaceModule 不暴露 `canvas.bind_data`。 |
| `.trellis/tasks/06-24-canvas-personal-shared-distribution-permission/design.md` | 设计要求 runtime mount builder 接收 effective access / runtime write flag，WorkspaceModule descriptor 基于 access 裁切 operations。 |
| `.trellis/tasks/06-24-canvas-personal-shared-distribution-permission/implement.md` | 将 Phase B 拆为 Runtime surface worker 与 WorkspaceModule worker，并要求 B1/B2 依赖 Phase A access projection。 |
| `.trellis/tasks/06-24-canvas-personal-shared-distribution-permission/research/dispatch-context.md` | 主会话已确认 Phase A worker 仍在跑，Runtime/WorkspaceModule 只做只读研究，不改生产代码。 |
| `.trellis/spec/backend/vfs/vfs-access.md` | VFS runtime mount、Canvas session visibility、surface mutation dispatcher 的规范入口。 |
| `.trellis/spec/backend/capability/tool-capability-pipeline.md` | `workspace_module` 是 Canvas Agent-facing create/describe/invoke/present 的统一 capability。 |
| `.trellis/spec/backend/capability/capability-dimension-pipeline.md` | `workspace_module_create(kind="canvas")` 同时写 runtime visible module ref 与 Canvas VFS exposure。 |
| `.trellis/spec/backend/session/architecture.md` | Canvas visibility/binding change 必须通过 `RuntimeSurfaceUpdateRequest`，业务模块不直接写 AgentFrame。 |
| `.trellis/spec/cross-layer/frontend-backend-contracts.md` | WorkspaceModule presentation contract；Canvas `presentation_uri=canvas://{canvas_mount_id}`。 |
| `crates/agentdash-application/src/vfs/mount_canvas.rs` | Canvas VFS mount builder；当前总是暴露 read/write/list/search。 |
| `crates/agentdash-application/src/vfs/provider_canvas.rs` | `canvas_fs` provider；当前 edit capabilities 与 write/delete/rename 未按 mount write capability 自我防御。 |
| `crates/agentdash-application/src/vfs/mutation_dispatcher.rs` | HTTP/API surface mutation dispatcher；写路径已先解析 `MountCapability::Write`。 |
| `crates/agentdash-application/src/vfs/service.rs` | VFS service；provider dispatch 前按 operation 所需 capability 解析 mount。 |
| `crates/agentdash-application/src/vfs/provider_skill_asset.rs` | 可复用的 read-only capability 模式：provider `edit_capabilities` 基于 `mount.supports(Write)`。 |
| `crates/agentdash-application/src/vfs/tools/common.rs` | `SharedRuntimeVfs::append_canvas_mount` 直接调用 `build_canvas_mount`，需要跟随 builder 签名/能力变化。 |
| `crates/agentdash-application/src/agent_run/runtime_surface_update.rs` | `AgentRunRuntimeSurfaceUpdateService::expose_canvas_mount` 是 create/present 后把 Canvas 追加到 live VFS 与 frame revision 的核心路径。 |
| `crates/agentdash-application/src/canvas/runtime_surface.rs` | WorkspaceModule/Canvas tool 提交 `RuntimeSurfaceUpdateRequest` 的 adapter。 |
| `crates/agentdash-application/src/canvas/visibility.rs` | Frame 重建时按 `visible_canvas_mount_ids` 把 Canvas 重新 append 到 VFS；该路径也必须变成 access-aware。 |
| `crates/agentdash-application/src/agent_run/frame/construction/owner_bootstrap.rs` | Owner bootstrap 有 `identity` 和 `visible_canvas_mount_ids`，当前 append 可见 Canvas 时未传 identity/access。 |
| `crates/agentdash-application/src/workspace_module/mod.rs` | WorkspaceModule descriptor 聚合和 Canvas module descriptor builder。 |
| `crates/agentdash-application/src/workspace_module/visibility.rs` | Runtime/base allowlist 可见性过滤；当前 Canvas 列表来自 `canvas_repo.list_by_project`。 |
| `crates/agentdash-application/src/workspace_module/tools.rs` | `workspace_module_create/describe/invoke/present` 实现；HostCanvas invoke 分支在这里。 |
| `crates/agentdash-application/src/workspace_module/runtime_tool_provider.rs` | Session runtime 装配 workspace module tools；当前没有把 `ExecutionContext.session.identity` 传给 workspace module tools。 |
| `crates/agentdash-contracts/src/surface/workspace_module.rs` | `WorkspaceModuleOperationDispatch::HostCanvas` contract source。 |
| `crates/agentdash-domain/src/canvas/value_objects.rs` | 当前工作区已有 Phase A 形状：`CanvasScope`、`CanvasAccessAction`、`CanvasAccessProjection`。 |
| `crates/agentdash-application/src/canvas/management.rs` | 当前工作区已有 access projection helper、`load_canvas_with_access`、publish/copy/unpublish use cases。 |
| `crates/agentdash-application/src/project/authorization.rs` | 可把 `AuthIdentity` 转成 `ProjectAuthorizationContext`，WorkspaceModule runtime tools 后续需要用它接 Phase A access。 |

### Runtime VFS Mount Builder And Provider

Canvas mount builder:

- `build_canvas_mount_id(canvas)` 在 `crates/agentdash-application/src/vfs/mount_canvas.rs:8`，实际返回 `canvas_vfs_mount_id(canvas)`。
- `build_canvas_mount(canvas)` 在 `crates/agentdash-application/src/vfs/mount_canvas.rs:12`；当前 `capabilities` 固定包含 `MountCapability::Read`、`Write`、`List`、`Search`，其中 `Write` 在 `crates/agentdash-application/src/vfs/mount_canvas.rs:20`。
- `append_canvas_mounts(vfs, canvases)` 在 `crates/agentdash-application/src/vfs/mount_canvas.rs:40`，会替换同 id mount 或追加新 mount。只读实现不能只改 `build_canvas_mount`，也要改该批量 helper 的参数形态或新增 access-aware helper。
- `refresh_canvas_mount_binding_files` 在 `crates/agentdash-application/src/vfs/mount_canvas.rs:55`，只更新 metadata，不涉及 writable 裁切。

现有 mount builder 测试:

- `append_canvas_mounts_replaces_existing_mount_without_reordering` 在 `crates/agentdash-application/src/vfs/mount_canvas.rs:106`。
- `refresh_canvas_mount_binding_files_omits_empty_binding_metadata` 在 `crates/agentdash-application/src/vfs/mount_canvas.rs:134`。
- 需要新增 read-only mount builder 测试：editable personal access 下包含 `Write`；project shared/read-only access 下只包含 `Read/List/Search`，且 `default_write=false` 保持不变。

Provider:

- `CanvasFsMountProvider` 定义在 `crates/agentdash-application/src/vfs/provider_canvas.rs:21`。
- `edit_capabilities(&self, _mount)` 在 `crates/agentdash-application/src/vfs/provider_canvas.rs:59`，当前无条件返回 create/delete/rename=true。
- `write_text`、`delete_text`、`rename_text` 分别在 `crates/agentdash-application/src/vfs/provider_canvas.rs:103`、`crates/agentdash-application/src/vfs/provider_canvas.rs:125`、`crates/agentdash-application/src/vfs/provider_canvas.rs:146`，当前直接进入 `update_canvas`，只调用 generated binding file 防线。
- generated binding file 防线在 `reject_generated_binding_file_write`，位置 `crates/agentdash-application/src/vfs/provider_canvas.rs:330`；这只能保护 `bindings/<alias>.*` 虚拟文件，不能保护 read-only shared source。
- 现有 provider 测试 `canvas_mount_exposes_resolved_binding_files_as_read_only_generated_files` 在 `crates/agentdash-application/src/vfs/provider_canvas.rs:467`，覆盖 generated binding 文件不可直接写，但没有覆盖 mount capability 缺少 Write 时拒绝普通 Canvas 源文件写/删/改名。

可复用 read-only provider 模式:

- `SkillAssetFsMountProvider::edit_capabilities` 在 `crates/agentdash-application/src/vfs/provider_skill_asset.rs:393`，使用 `mount.supports(agentdash_spi::MountCapability::Write)` 判断是否返回 create/delete/rename=true。
- `writable_skill_asset_mount_updates_extra_files_through_primitives` 在 `crates/agentdash-application/src/vfs/provider_skill_asset.rs:893`，覆盖 writable mount 的 write/rename/delete。
- `writable_skill_asset_mount_rejects_skill_document_delete_and_rename` 在 `crates/agentdash-application/src/vfs/provider_skill_asset.rs:965`，覆盖 provider 层业务防线。
- Canvas provider 可以复用该模式：`edit_capabilities` 基于 `mount.supports(Write)`；`write_text/delete_text/rename_text` 入口先检查 `mount.supports(Write)`，缺失时返回 `MountError::NotSupported("Canvas mount is read-only")` 或同类用户可读语义，再进入 existing binding/source guards。

### Mutation Dispatcher And VFS Service Write Checks

结论：outer dispatcher/service 已在 provider 前检查 `MountCapability::Write`，但 provider 层仍必须补纵深防线。

`VfsMutationDispatcher`:

- `create_text` 在 `crates/agentdash-application/src/vfs/mutation_dispatcher.rs:98`，先 `resolve_mount(..., MountCapability::Write)`，具体检查在 `crates/agentdash-application/src/vfs/mutation_dispatcher.rs:105`。
- `write_text` 在 `crates/agentdash-application/src/vfs/mutation_dispatcher.rs:149`，先 `resolve_mount(..., Write)`，位置 `crates/agentdash-application/src/vfs/mutation_dispatcher.rs:156`。
- `delete_text` 在 `crates/agentdash-application/src/vfs/mutation_dispatcher.rs:184`，先 `resolve_mount(..., Write)`，位置 `crates/agentdash-application/src/vfs/mutation_dispatcher.rs:190`。
- `rename_text` 在 `crates/agentdash-application/src/vfs/mutation_dispatcher.rs:224`，先 `resolve_mount(..., Write)`，位置 `crates/agentdash-application/src/vfs/mutation_dispatcher.rs:232`。
- `apply_patch` 在 `crates/agentdash-application/src/vfs/mutation_dispatcher.rs:283`，先 `resolve_mount(..., Write)`，位置 `crates/agentdash-application/src/vfs/mutation_dispatcher.rs:290`。
- `upload_inline_binary` 在 `crates/agentdash-application/src/vfs/mutation_dispatcher.rs:307`，先 `resolve_mount(..., Write)`，位置 `crates/agentdash-application/src/vfs/mutation_dispatcher.rs:315`。
- `ensure_edit_capability` 在 `crates/agentdash-application/src/vfs/mutation_dispatcher.rs:344`，非 inline provider 会调用 `provider.edit_capabilities(mount)`，位置 `crates/agentdash-application/src/vfs/mutation_dispatcher.rs:354`。所以 Canvas provider 的 `edit_capabilities` 也会影响 create/delete/rename 的 UI/API mutation admission。

`VfsService`:

- `resolve_provider_dispatch` 在 `crates/agentdash-application/src/vfs/service.rs:76`，统一接收所需 capability 并调用 `resolve_mount`。
- `write_text` 在 `crates/agentdash-application/src/vfs/service.rs:281`，使用 `MountCapability::Write`，位置 `crates/agentdash-application/src/vfs/service.rs:292`。
- `delete_text` 在 `crates/agentdash-application/src/vfs/service.rs:349`，使用 `MountCapability::Write`，位置 `crates/agentdash-application/src/vfs/service.rs:359`。
- `rename_text` 在 `crates/agentdash-application/src/vfs/service.rs:404`，使用 `MountCapability::Write`，位置 `crates/agentdash-application/src/vfs/service.rs:416`。
- `apply_patch` 在 `crates/agentdash-application/src/vfs/service.rs:564`，先 `resolve_mount(..., Write)`，位置 `crates/agentdash-application/src/vfs/service.rs:572`。
- Provider patch target 会再读 `provider.edit_capabilities(mount)`，位置 `crates/agentdash-application/src/vfs/service.rs:1072`。这进一步要求 Canvas provider 的 edit capabilities 对 read-only mount 返回 false。

Provider 层仍需补的防线:

- 直接 provider 调用、测试调用、未来内部调用不一定经过 mutation dispatcher/service。
- `edit_capabilities` 影响 VFS Browser / resolved surface 展示；只读 Canvas 需要 create/delete/rename=false。
- `write_text/delete_text/rename_text` 必须在 provider 入口 guard `mount.supports(Write)`，否则 direct provider call 仍可更新 Canvas repository。

### Runtime Exposure And Frame Rebuild Paths

Canvas exposure 主路径:

- `AgentRunRuntimeSurfaceUpdateService::expose_canvas_mount(session_id, canvas)` 在 `crates/agentdash-application/src/agent_run/runtime_surface_update.rs:71`。
- 它当前在 active VFS 上调用 `append_canvas_mounts`，位置 `crates/agentdash-application/src/agent_run/runtime_surface_update.rs:99`。
- 它随后刷新 binding metadata，位置 `crates/agentdash-application/src/agent_run/runtime_surface_update.rs:103`。
- 它写入 `visible_canvas_mount_ids` 与 `visible_workspace_module_refs`，位置 `crates/agentdash-application/src/agent_run/runtime_surface_update.rs:121` 和 `crates/agentdash-application/src/agent_run/runtime_surface_update.rs:122`。
- 现有测试 `canvas_expose_noops_when_surface_and_visibility_are_unchanged` 在 `crates/agentdash-application/src/agent_run/runtime_surface_update.rs:295`；`runtime_surface_noop_compare_uses_frame_surface_not_revision_identity` 在 `crates/agentdash-application/src/agent_run/runtime_surface_update.rs:364`。

Adapter:

- `submit_canvas_runtime_surface_update` 在 `crates/agentdash-application/src/canvas/runtime_surface.rs:10`，它验证 request target 后调用 `runtime_surface_update.expose_canvas_mount`。
- `submit_existing_canvas_visibility_request` 在 `crates/agentdash-application/src/canvas/runtime_surface.rs:37`，用于 present existing Canvas。

Frame rebuild path:

- `append_visible_canvas_mounts` 在 `crates/agentdash-application/src/canvas/visibility.rs:12`，当前按 `visible_canvas_mount_ids` 从 `canvas_repo.list_by_project(project_id)` 取 Canvas，再 `append_canvas_mounts(vfs, &visible)`，位置 `crates/agentdash-application/src/canvas/visibility.rs:35`。
- `OwnerBootstrapSpec` 已携带 `identity` 和 `visible_canvas_mount_ids`，位置 `crates/agentdash-application/src/agent_run/frame/construction/owner_bootstrap.rs:112` 到 `crates/agentdash-application/src/agent_run/frame/construction/owner_bootstrap.rs:130`。
- owner bootstrap 当前调用 `append_visible_canvas_mounts(self.canvas_repo, project_id, space, &spec.visible_canvas_mount_ids)`，位置 `crates/agentdash-application/src/agent_run/frame/construction/owner_bootstrap.rs:449` 到 `crates/agentdash-application/src/agent_run/frame/construction/owner_bootstrap.rs:455`，没有传 identity/access。
- `FrameAssemblyBuilder::append_canvas_mounts` 在 `crates/agentdash-application/src/agent_run/frame/construction/assembly.rs:166`，也只是透传 canvas repo/project/mount ids。

关键实现含义:

- `visible_canvas_mount_ids` 只存 mount id，不存 writable/read-only。`AgentFrame::visible_canvas_mount_ids` 和 `append_visible_canvas_mount` 在 `crates/agentdash-domain/src/workflow/agent_frame.rs:77` 与 `crates/agentdash-domain/src/workflow/agent_frame.rs:86`。
- 因此 read-only 语义不能只在 create/present 当下写一次 mount capability；后续 frame construction / runtime reload 需要基于当前 user identity + Phase A access projection 重新计算 mount capability。

### WorkspaceModule Descriptor And Invoke Map

Descriptor builder:

- `build_workspace_modules(ext, canvases)` 在 `crates/agentdash-application/src/workspace_module/mod.rs:177`，聚合 extension modules 与 `canvases.iter().map(build_canvas_module)`。
- `build_canvas_workspace_module(canvas)` 在 `crates/agentdash-application/src/workspace_module/mod.rs:187`，只是调用 private `build_canvas_module`。
- `build_canvas_module(canvas)` 在 `crates/agentdash-application/src/workspace_module/mod.rs:387`。
- `canvas.bind_data` 在 `crates/agentdash-application/src/workspace_module/mod.rs:388` 到 `crates/agentdash-application/src/workspace_module/mod.rs:425` 无条件暴露，`dispatch` 是 `WorkspaceModuleOperationDispatch::HostCanvas { canvas_action: BindData }`，位置 `crates/agentdash-application/src/workspace_module/mod.rs:422`。
- Canvas UI entry `preview` 和 `presentation_uri=canvas://{canvas_mount_id}` 在 `crates/agentdash-application/src/workspace_module/mod.rs:428` 到 `crates/agentdash-application/src/workspace_module/mod.rs:434`。
- Descriptor 测试 `aggregates_extension_and_canvas_modules` 在 `crates/agentdash-application/src/workspace_module/mod.rs:597`，断言 Canvas module 暴露 `canvas.bind_data` 与 preview URI。

Visibility:

- `resolve_workspace_module_visibility` 在 `crates/agentdash-application/src/workspace_module/visibility.rs:28`。
- 当前 Canvas 列表来自 `canvas_repo.list_by_project(project_id)`，位置 `crates/agentdash-application/src/workspace_module/visibility.rs:42`。
- 当前 descriptor 聚合在 `crates/agentdash-application/src/workspace_module/visibility.rs:50`，没有 access projection。
- 可见性测试入口：`base_all_returns_extensions_and_canvases` 在 `crates/agentdash-application/src/workspace_module/visibility.rs:342`，`runtime_refs_extend_allowlist_from_agent_run_view` 在 `crates/agentdash-application/src/workspace_module/visibility.rs:364`，`missing_runtime_ref_reports_diagnostic_without_fabricating_module` 在 `crates/agentdash-application/src/workspace_module/visibility.rs:394`。

Invoke:

- `WorkspaceModuleInvokeTool` 定义在 `crates/agentdash-application/src/workspace_module/tools.rs:603`。
- invoke 在执行时先 `resolve_visible_modules_for_tool`，位置 `crates/agentdash-application/src/workspace_module/tools.rs:715`，再 `locate_operation`，位置 `crates/agentdash-application/src/workspace_module/tools.rs:723` 到 `crates/agentdash-application/src/workspace_module/tools.rs:727`。
- `HostCanvas` 分支在 `crates/agentdash-application/src/workspace_module/tools.rs:845`。
- `BindData` 当前把 `module.summary.source` 写回 `canvas_mount_id`，位置 `crates/agentdash-application/src/workspace_module/tools.rs:858` 到 `crates/agentdash-application/src/workspace_module/tools.rs:861`。
- 当前实际 mutation 调用 `bind_canvas_data_for_project`，位置 `crates/agentdash-application/src/workspace_module/tools.rs:868` 到 `crates/agentdash-application/src/workspace_module/tools.rs:873`；这个 helper 只按 project_id + mount_id 加载 Canvas 并更新 binding。
- binding 后提交 `RuntimeSurfaceUpdateRequest::CanvasBindingChanged`，位置 `crates/agentdash-application/src/workspace_module/tools.rs:874` 到 `crates/agentdash-application/src/workspace_module/tools.rs:880`。
- 现有 invoke 测试 `invoke_canvas_bind_data_routes_to_host_canvas_use_case` 在 `crates/agentdash-application/src/workspace_module/tools.rs:2629`；`invoke_canvas_bind_data_runtime_update_preserves_external_integration_skill` 在 `crates/agentdash-application/src/workspace_module/tools.rs:2679`。

Create/present:

- `WorkspaceModuleCreateTool` 定义在 `crates/agentdash-application/src/workspace_module/tools.rs:366`；它调用 `create_or_attach_canvas_for_session`，位置 `crates/agentdash-application/src/workspace_module/tools.rs:437`。
- create 后从 `build_workspace_modules(..., [canvas])` 取 descriptor，位置 `crates/agentdash-application/src/workspace_module/tools.rs:446`。
- `WorkspaceModulePresentTool` present Canvas 时会先请求 existing Canvas exposure，位置 `crates/agentdash-application/src/workspace_module/tools.rs:1035` 到 `crates/agentdash-application/src/workspace_module/tools.rs:1044`，再注入 `workspace_module_presented` event，位置 `crates/agentdash-application/src/workspace_module/tools.rs:1053` 到 `crates/agentdash-application/src/workspace_module/tools.rs:1065`。
- create/present runtime tests：`create_canvas_runtime_grant_extends_allowlist_session_visibility` 在 `crates/agentdash-application/src/workspace_module/tools.rs:1834`；`canvas_module_present_refreshes_session_exposure_before_event` 在 `crates/agentdash-application/src/workspace_module/tools.rs:2079`。

Contract dispatch:

- `WorkspaceModuleOperationDispatch::HostCanvas` contract branch 在 `crates/agentdash-contracts/src/surface/workspace_module.rs:108` 到 `crates/agentdash-contracts/src/surface/workspace_module.rs:118`。
- 当前 read-only operation 裁切可优先在 application descriptor 层完成，不一定需要改 contract；可用现有 `permission_summary` 表达只读说明，或只把 `canvas.bind_data` 从 `operations` 和 `summary.operation_summary` 移除。

### Phase A Access Projection Dependencies

当前只读快照里已经能看到 Phase A 形状，但 Phase A worker 仍在运行，后续 B1/B2 接入前必须由主会话确认最终签名稳定。

已观察到的接口形状:

- `CanvasScope::{Personal, Project}` 在 `crates/agentdash-domain/src/canvas/value_objects.rs:34`。
- `CanvasAccessAction::{View, EditSource, Publish, ManageShared, Copy, RuntimeWrite}` 在 `crates/agentdash-domain/src/canvas/value_objects.rs:70`。
- `CanvasAccessProjection` 包含 `can_view/can_edit_source/can_publish/can_manage_shared/can_copy/runtime_write_allowed`，位置 `crates/agentdash-domain/src/canvas/value_objects.rs:80` 到 `crates/agentdash-domain/src/canvas/value_objects.rs:88`。
- `CanvasAccessProjection::allows` 在 `crates/agentdash-domain/src/canvas/value_objects.rs:90` 到 `crates/agentdash-domain/src/canvas/value_objects.rs:100`。
- `load_canvas_with_access(repos, current_user, canvas_id, required_action)` 在 `crates/agentdash-application/src/canvas/management.rs:230`。
- `canvas_access_projection` 在 `crates/agentdash-application/src/canvas/management.rs:550`；personal owner 得到 `runtime_write_allowed=true`，位置 `crates/agentdash-application/src/canvas/management.rs:563` 到 `crates/agentdash-application/src/canvas/management.rs:574`；project shared 永远 `runtime_write_allowed=false`，位置 `crates/agentdash-application/src/canvas/management.rs:576` 到 `crates/agentdash-application/src/canvas/management.rs:586`。
- `require_canvas_action` 会把 `CanvasAccessAction::RuntimeWrite` 映射为“写入运行面”，位置 `crates/agentdash-application/src/canvas/management.rs:790` 到 `crates/agentdash-application/src/canvas/management.rs:808`。
- `project_authorization_context_from_identity(identity)` 在 `crates/agentdash-application/src/project/authorization.rs:7`，可把 `ExecutionContext.session.identity` 转成 access projection 需要的 current-user context。
- `ExecutionContext.session.identity` 的 SPI 字段在 `crates/agentdash-spi/src/connector/mod.rs:88`。
- `VfsRuntimeToolProvider` 已经把 `context.session.identity` 传给 VFS tools，位置 `crates/agentdash-application/src/runtime_tools/vfs_provider.rs:76` 到 `crates/agentdash-application/src/runtime_tools/vfs_provider.rs:80`；WorkspaceModule provider 目前没有传。

必须等 Phase A 完成后再接的参数/类型:

- Runtime mount builder 需要一个稳定的 access carrier。最小形状可以是 application-local `CanvasRuntimeAccess { writable: bool }`，取值来自 `CanvasAccessProjection.runtime_write_allowed`。若 Phase A 已提供等价类型，应复用而不是另造同义类型。
- WorkspaceModule descriptor builder 需要 access input。可将 `build_canvas_workspace_module(canvas)` 扩为 `build_canvas_workspace_module(canvas, access)`，或新增 `build_canvas_workspace_module_with_access`，由 caller 保证 `access.can_view=true`；read-only 时移除 `canvas.bind_data`。
- `resolve_workspace_module_visibility` 需要从 `list_by_project` 切到 access-aware list，例如 Phase A 的 `list_canvases_for_user` 或等价函数，并保留 base allowlist/runtime refs 过滤。
- WorkspaceModule runtime tool provider 需要 access 所需的 current user。当前 `WorkspaceModuleRuntimeToolProvider::build_tools` 只使用 project_id/session_id/vfs/backend anchor；后续需要传 `ExecutionContext.session.identity`，并可能需要 `RepositorySet` 或至少 `project_repo + canvas_repo`，因为 `canvas_access_projection` 需要 Project authorization facts。
- `workspace_module_create(kind="canvas")` 当前 helper `create_or_attach_canvas_for_session` 只接 `CanvasRepository` 并使用 legacy `build_canvas`。Phase A 完成后应接默认创建 personal Canvas 的 use case，并需要 current user identity。
- `workspace_module_invoke` HostCanvas branch 当前按 mount id 调 `bind_canvas_data_for_project`。后续需要 Phase A 提供 mount-id 版本的 access-aware load/mutation helper，或先通过 mount id load Canvas 再用 UUID 调 `load_canvas_with_access(..., CanvasAccessAction::RuntimeWrite)`。当前 `load_canvas_with_access` 是 by UUID，WorkspaceModule 输入是 `canvas:{canvas_mount_id}`。
- Runtime exposure/rebuild 如果选择在 service 内计算 access，则 `AgentRunRuntimeSurfaceUpdateService` 需要 `canvas_repo/project_repo` 和 `surface.identity`；如果选择 caller 计算 access，则 `submit_canvas_runtime_surface_update` / `expose_canvas_mount` 需要接收 access/writable flag。两者只能选一条 canonical 路径，避免 WorkspaceModule 与 frame rebuild 分别手写规则。

### Required Follow-Up Implementation Context

Runtime worker should:

- Own `crates/agentdash-application/src/vfs/mount_canvas.rs` and introduce access-aware mount building. All call sites of `build_canvas_mount` are currently limited to `mount_canvas.rs:12`, `provider_canvas.rs:482` test setup, and `vfs/tools/common.rs:66`.
- Own `crates/agentdash-application/src/vfs/provider_canvas.rs`; update `edit_capabilities` and add provider-level read-only guards to `write_text/delete_text/rename_text`.
- Own targeted VFS tests in `mount_canvas.rs` and `provider_canvas.rs`.
- Own runtime exposure plumbing only if the chosen access carrier requires it: `crates/agentdash-application/src/agent_run/runtime_surface_update.rs`, `crates/agentdash-application/src/canvas/runtime_surface.rs`, `crates/agentdash-application/src/canvas/visibility.rs`, and frame construction append paths. This is the part that can conflict with WorkspaceModule worker, so define the function signature before B2 starts.
- Add tests that cover a read-only Canvas mount through `expose_canvas_mount` / frame rebuild, not just pure builder output.

WorkspaceModule worker should:

- Own `crates/agentdash-application/src/workspace_module/mod.rs` for access-aware Canvas descriptor building and `canvas.bind_data` operation removal when `runtime_write_allowed=false`.
- Own `crates/agentdash-application/src/workspace_module/visibility.rs` for access-aware Canvas listing/projection.
- Own `crates/agentdash-application/src/workspace_module/tools.rs` for create/present/invoke behavior and stale-operation HostCanvas guard.
- Own `crates/agentdash-application/src/workspace_module/runtime_tool_provider.rs` if identity/repository plumbing is needed from `ExecutionContext`.
- Avoid editing `vfs/mount_canvas.rs` / `provider_canvas.rs`; consume the runtime worker's access carrier/signature after it lands.
- Do not change API routes or contracts unless main session explicitly expands scope. Existing `WorkspaceModuleOperationDispatch` can already represent HostCanvas; operation removal does not require a contract change.

Stale/forged operation defense required:

- Descriptor-level removal of `canvas.bind_data` is necessary but insufficient. `workspace_module_invoke` should still enforce `CanvasAccessAction::RuntimeWrite` or `access.runtime_write_allowed` before entering the `BindData` mutation, because a stale descriptor, forged operation key, or race can reach `HostCanvas` if visibility/access changed after describe.

### Existing Test Entry Points To Reuse

- VFS mount builder: `cargo test -p agentdash-application append_canvas_mounts_replaces_existing_mount_without_reordering`
- VFS mount metadata: `cargo test -p agentdash-application refresh_canvas_mount_binding_files_omits_empty_binding_metadata`
- Canvas provider binding virtual file: `cargo test -p agentdash-application canvas_mount_exposes_resolved_binding_files_as_read_only_generated_files`
- Skill asset capability pattern reference: `cargo test -p agentdash-application writable_skill_asset_mount_updates_extra_files_through_primitives`
- WorkspaceModule descriptor: `cargo test -p agentdash-application aggregates_extension_and_canvas_modules`
- WorkspaceModule visibility: `cargo test -p agentdash-application runtime_visible_refs_extend_workspace_module_allowlist`
- WorkspaceModule create/runtime exposure: `cargo test -p agentdash-application create_canvas_runtime_grant_extends_allowlist_session_visibility`
- WorkspaceModule present/runtime exposure ordering: `cargo test -p agentdash-application canvas_module_present_refreshes_session_exposure_before_event`
- WorkspaceModule HostCanvas invoke: `cargo test -p agentdash-application invoke_canvas_bind_data_routes_to_host_canvas_use_case`
- WorkspaceModule binding runtime update: `cargo test -p agentdash-application invoke_canvas_bind_data_runtime_update_preserves_external_integration_skill`

### Related Specs

- `.trellis/spec/backend/vfs/vfs-access.md`: Canvas session visibility, runtime mount contract, surface mutation dispatcher responsibilities.
- `.trellis/spec/backend/capability/tool-capability-pipeline.md`: WorkspaceModule is the canonical Canvas Agent surface; `canvas.bind_data` is an operation under `workspace_module_invoke`.
- `.trellis/spec/backend/capability/capability-dimension-pipeline.md`: Canvas workspace module runtime exposure and VFS exposure are separate but coordinated surfaces.
- `.trellis/spec/backend/session/architecture.md`: Runtime surface changes go through typed `RuntimeSurfaceUpdateRequest`; business modules do not direct-write AgentFrame revisions.
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`: Canvas presentation URI and WorkspaceModule presentation payload contract.
- `.trellis/spec/backend/permission/architecture.md`: Permission/runtime surface facts should land in auditable capability/projection paths, not ad hoc per-caller checks.

### External References

- No external references were needed. This research is based on Trellis task artifacts, project specs, and local production code only.

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` reported `Current task: (none)`. This file was written only because the user supplied the explicit task path and exact target research file.
- Phase A backend foundation worker is still running per `research/dispatch-context.md`; current code already contains access projection types, but B1/B2 should wait for main-session confirmation that Phase A signatures and repository changes are final.
- No existing Canvas provider test covers mount-level read-only write/delete/rename rejection. Existing provider read-only coverage only protects generated binding files.
- No existing WorkspaceModule test covers read-only Canvas descriptor operation裁切 or invoke-side rejection of `canvas.bind_data`.
- `visible_canvas_mount_ids` stores only mount ids. No persisted writable/read-only flag was found in AgentFrame visible Canvas refs, so frame rebuild must recompute access from identity instead of trusting stored mount state.
- WorkspaceModule runtime tool provider currently has access to `ExecutionContext.session.identity`, but does not pass it into tools. It also currently holds `canvas_repo` only, while Phase A access projection needs project authorization facts.
- API route/contract/frontend/pi_agent files were read only where needed for context or not read at all; no production files were modified.
