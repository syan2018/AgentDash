# Research: CE02-CE04 implementation scope

- Query: 为 Capability Exposure 事实源收敛补齐 CE02-CE04 实现级规划，使 PermissionGrant、Canvas expose、WorkspaceModule visibility 从 AgentFrame revision 派生。
- Scope: internal
- Date: 2026-06-21

## Findings

### Planning / spec baseline

- `.trellis/tasks/06-21-capability-exposure-fact-convergence/prd.md`：要求 AgentFrame 成为 runtime capability / exposure 唯一锚点，PermissionGrant status 只负责审批和审计。
- `.trellis/tasks/06-21-capability-exposure-fact-convergence/design.md`：CE01 已决策 runtime exposure / capability 变更通过新的 AgentFrame revision 表达，不使用独立 exposure table。
- `.trellis/tasks/06-21-capability-exposure-fact-convergence/work-items/index.md`：CE02-CE04 已 ready，CE05 仍是边界设计项。
- `.trellis/tasks/06-21-module-topology-coupling-review/design-coupling-tracker.md`：D05/D06/D07/D14 已决定 AgentFrame revision 承载 runtime exposure，live VFS / hook runtime / visibility 都从 frame 派生。
- `.trellis/spec/backend/capability/tool-capability-pipeline.md`：CapabilityResolver 是工具集唯一计算入口；AgentRun 当前可执行 MCP surface 的事实源是 AgentFrame revision 的 MCP surface。
- `.trellis/spec/backend/permission/grant-lifecycle.md`：PermissionGrant lifecycle 已定义 approve/revoke/expire 状态机和 `effect_frame_id`。
- `.trellis/spec/backend/hooks/execution-hook-runtime.md`：Hook runtime 应按当前 AgentFrame 与 RuntimeSessionExecutionAnchor 重建，delivery session 只是 trace/provenance。

### Files found

- `crates/agentdash-domain/src/workflow/agent_frame.rs`：AgentFrame revision row；已有 capability/context/VFS/MCP JSON surface 和 visible canvas/workspace module columns。
- `crates/agentdash-domain/src/workflow/repository.rs`：AgentFrameRepository trait；当前暴露 `append_visible_canvas_mount` / `append_visible_workspace_module_ref` 这种 row update API。
- `crates/agentdash-application/src/agent_run/frame/builder.rs`：AgentFrameBuilder；新 revision 会复制 current frame 的 visible exposure columns。
- `crates/agentdash-application/src/agent_run/frame/surface.rs`：FrameSurfaceDraft 和 typed AgentFrame surface reader。
- `crates/agentdash-application/src/session/capability_state.rs`：AgentFrame <-> CapabilityState 投影、runtime transition replay、workspace module base visibility projection。
- `crates/agentdash-application/src/session/hub/tool_builder.rs`：`replace_current_capability_state` 已有 “AgentFrame revision -> memory cache -> connector sync” primitive。
- `crates/agentdash-application/src/session/capability_service.rs`：SessionCapabilityService 暴露 runtime-session-to-frame adapter、live VFS apply、当前 visible module refs 读取。
- `crates/agentdash-application/src/permission/service.rs`：PermissionGrant approve/revoke 已写 AgentFrame revision，但没有 live runtime handoff，也没有 expire effect service。
- `crates/agentdash-application/src/permission/compiler.rs`：PermissionGrant -> RuntimeCapabilityTransition compiler。
- `crates/agentdash-application/src/canvas/tools.rs`：Canvas create/present expose 路径；当前先改 live VFS，再 append 当前 frame visible refs，再 apply VFS capability state。
- `crates/agentdash-application/src/workspace_module/tools.rs`：WorkspaceModule list/describe/present 的局部 visibility resolver 和 Canvas presentation flow。
- `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs`：Postgres AgentFrameRepository 实现；append API 直接 UPDATE 当前 frame row。
- `crates/agentdash-infrastructure/migrations/0001_init.sql`：已有 `agent_frames`、`agent_frame_transitions`、`permission_grants` schema。
- `crates/agentdash-infrastructure/migrations/0008_agent_frame_visible_workspace_modules.sql`：已有 `visible_workspace_module_refs_json` 列。
- `packages/app-web/src/pages/AgentRunWorkspacePage.workspace-module.test.ts`：前端已覆盖 Canvas presentation URI、runtime surface filtering 等行为。

### Code patterns

- AgentFrame 注释明确 “每次 capability/context/VFS/MCP surface 变更产生新 revision”，字段包括 `effective_capability_json`、`vfs_surface_json`、`mcp_surface_json`、`visible_canvas_mount_ids_json`、`visible_workspace_module_refs_json`：`crates/agentdash-domain/src/workflow/agent_frame.rs:6`, `crates/agentdash-domain/src/workflow/agent_frame.rs:14`, `crates/agentdash-domain/src/workflow/agent_frame.rs:19`, `crates/agentdash-domain/src/workflow/agent_frame.rs:21`, `crates/agentdash-domain/src/workflow/agent_frame.rs:26`, `crates/agentdash-domain/src/workflow/agent_frame.rs:33`.
- `AgentFrame::new_revision` 当前不会自动携带 exposure refs；携带行为在 builder 里完成：`crates/agentdash-domain/src/workflow/agent_frame.rs:59`, `crates/agentdash-domain/src/workflow/agent_frame.rs:157`, `crates/agentdash-application/src/agent_run/frame/builder.rs:270`.
- AgentFrameRepository append API 是当前 row update，不是新 revision：`crates/agentdash-domain/src/workflow/repository.rs:87`, `crates/agentdash-domain/src/workflow/repository.rs:92`, `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:315`, `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:337`.
- AgentFrameBuilder 是正确写入入口：`with_capability_state` 同步拆分 capability/VFS/MCP 三列，`build` 按 current revision + 1 创建新 frame：`crates/agentdash-application/src/agent_run/frame/builder.rs:137`, `crates/agentdash-application/src/agent_run/frame/builder.rs:225`.
- `replace_current_capability_state` 已实现热更新骨架：先写新 AgentFrame revision，再更新 active turn/session profile/cache/connector tools：`crates/agentdash-application/src/session/hub/tool_builder.rs:159`, `crates/agentdash-application/src/session/hub/tool_builder.rs:220`, `crates/agentdash-application/src/session/hub/tool_builder.rs:248`, `crates/agentdash-application/src/session/hub/tool_builder.rs:292`.
- PermissionGrant approve/revoke 当前用 `apply_frame_effect` 读取 current frame、修改 CapabilityState、build 新 revision；但 API 只返回 grant DTO，未把 effect frame 或 transition 注入 live runtime：`crates/agentdash-application/src/permission/service.rs:141`, `crates/agentdash-application/src/permission/service.rs:193`, `crates/agentdash-application/src/permission/service.rs:265`, `crates/agentdash-api/src/routes/permission_grants.rs:226`.
- PermissionGrant expire 目前只有 repository bulk `expire_overdue`，没有 application service 逐 grant 写 remove effect：`crates/agentdash-infrastructure/src/persistence/postgres/permission_grant_repository.rs:196`, `crates/agentdash-domain/src/permission/entity.rs:153`.
- Canvas expose 当前顺序是 `vfs.append_canvas_mount` -> `append_visible_canvas_mount_to_frame` -> `append_visible_workspace_module_ref_to_frame` -> `apply_live_vfs_capability_state`：`crates/agentdash-application/src/canvas/tools.rs:242`, `crates/agentdash-application/src/canvas/tools.rs:248`, `crates/agentdash-application/src/canvas/tools.rs:253`, `crates/agentdash-application/src/canvas/tools.rs:262`, `crates/agentdash-application/src/canvas/tools.rs:272`.
- WorkspaceModule visibility 当前是 `resolve_visible_modules(base visibility, dynamic_module_refs)` 的局部函数；dynamic refs 通过 frame visible refs 读取：`crates/agentdash-application/src/workspace_module/tools.rs:47`, `crates/agentdash-application/src/workspace_module/tools.rs:66`, `crates/agentdash-application/src/workspace_module/tools.rs:78`.
- WorkspaceModuleDimension base allowlist 已在 `CapabilityState.workspace_module` 中，`All`/`Allowlist` 规则位于 SPI：`crates/agentdash-spi/src/connector/mod.rs:275`, `crates/agentdash-spi/src/connector/mod.rs:285`, `crates/agentdash-spi/src/connector/mod.rs:314`.
- Canvas module descriptor 使用 `module_id=canvas:{mount_id}` 和 `presentation_uri=canvas://{mount_id}`：`crates/agentdash-application/src/workspace_module/mod.rs:433`, `crates/agentdash-application/src/workspace_module/mod.rs:436`, `crates/agentdash-application/src/workspace_module/mod.rs:447`.
- Hook refresh/rebuild 已是 frame target first：`load_frame_snapshot` 和 `refresh_frame_snapshot` 接收 frame target，live transition 会 `ensure_hook_runtime_for_target` 后 enqueue context frame：`crates/agentdash-application/src/hooks/provider.rs:265`, `crates/agentdash-application/src/hooks/provider.rs:281`, `crates/agentdash-application/src/session/hub/runtime_context_transition.rs:134`.

### CE02 data model and flow

Fact shape:

- Keep AgentFrame as the fact row; do not add an exposure table in the first batch.
- PermissionGrant runtime effect writes a new AgentFrame revision with:
  - `effective_capability_json`: full projected `CapabilityState` after grant add/remove.
  - `vfs_surface_json`: carried forward from `CapabilityState.vfs.active`.
  - `mcp_surface_json`: carried forward from `CapabilityState.tool.mcp_servers`.
  - `created_by_kind`: `permission_grant_approve`, `permission_grant_revoke`, `permission_grant_expire`.
  - `created_by_id`: grant id.
- `RuntimeCapabilityTransition` remains an effect/audit value returned from compiler/service; the runtime fact is the resulting AgentFrame revision, not active grant status.

Implementation flow:

1. Keep `PermissionGrantCompiler::{compile, compile_revoke}` as transition builder.
2. Replace `apply_requested_paths` manual mutation with the same replay path used by capability dimensions where feasible:
   - base = `project_capability_state_from_frame(current_frame)`.
   - transition = add/remove directives.
   - after = `apply_runtime_capability_transition(base, transition)`.
   - append `set_tool_access` effect if existing event payloads/tests need it.
3. After new AgentFrame revision is built, live delivery needs a handoff:
   - If no active runtime exists, returning/persisting the new frame is enough; recovery reads latest AgentFrame.
   - If delivery runtime is active, call a session capability service method that aligns memory cache/tools/hook runtime to the new frame. Existing `replace_current_capability_state` writes another frame, so first batch should add a narrower “adopt persisted capability revision” helper instead of calling it directly.
4. Revoke uses the same flow with remove directives and `created_by_kind=permission_grant_revoke`.
5. Expire cannot stay as bulk SQL if it must revoke runtime capability. Add an application-owned expiry service that loads overdue active grants, applies remove effect per grant, then marks each grant Expired. Repository bulk `expire_overdue` may remain as low-level cleanup only, but should not be the CE runtime effect path.

Minimal first write set:

- `crates/agentdash-application/src/permission/service.rs`
  - Add `expire_overdue_with_frame_effects(now)` or equivalent service path.
  - Change apply path to return `PermissionGrantEffectResult { grant, transition, effect_frame }` for approve/revoke/expire.
  - Inject or call a live-runtime adoption port only when caller has delivery/session services; do not hide live update inside repository.
- `crates/agentdash-domain/src/permission/repository.rs` and `crates/agentdash-infrastructure/src/persistence/postgres/permission_grant_repository.rs`
  - Add a query that returns overdue active grants, or change expiry use case to list active grants then filter by `expires_at`.
- `crates/agentdash-api/src/routes/permission_grants.rs`
  - Approve/revoke route may still return `PermissionGrantResponse`; if active runtime sync is unavailable in API route, record that it is recovery-only and leave live handoff to runtime broker route.
- Tests in `crates/agentdash-application/src/permission/service.rs`.

Migration:

- No DB migration required for first CE02 batch if reusing existing AgentFrame JSON columns and PermissionGrant fields.
- A migration is only needed if implement chooses to persist `agent_frame_transitions` for PermissionGrant effects; the table already exists in `0001_init.sql`, so likely no schema change is needed.

Roundtrip/recovery tests:

- Approve creates a new AgentFrame revision whose `effective_capability_json` includes granted capability and whose previous VFS/MCP/execution profile are preserved.
- Revoke creates a later revision removing the capability/tool policy.
- Expire applies the same remove effect before status becomes Expired.
- Recovery from latest AgentFrame projects the same `CapabilityState` as the live apply path.
- Route test remains focused on status contract; service test owns frame effect.

### CE03 data model and flow

Fact shape:

- Canvas exposure must be represented by a new AgentFrame revision before live VFS or presentation event becomes the durable result.
- Use existing frame exposure fields for first batch:
  - `visible_canvas_mount_ids_json`: accumulated canvas mount ids.
  - `visible_workspace_module_refs_json`: accumulated module refs such as `canvas:{mount_id}`.
  - `vfs_surface_json`: VFS with the Canvas mount (`cvs-{mount_id}` / provider `canvas_fs`) included.
  - `effective_capability_json`: CapabilityState with `vfs.active` set to the same VFS and skill baseline re-derived.
- The current direct row-update append APIs should be replaced by a revision writer; keep repository read APIs if needed, but stop using append mutation as fact write.

Recovery order:

1. Locate delivery runtime -> anchor -> current AgentFrame.
2. Read latest/current AgentFrame exposure fact.
3. Reconstruct VFS from `vfs_surface_json` / CapabilityState `vfs.active`; visible canvas ids only serve projection/filtering and compatibility with owner bootstrap.
4. Recompute skill baseline from final VFS.
5. Rebuild WorkspaceModule visibility using base `CapabilityState.workspace_module` plus runtime refs from the same AgentFrame.
6. Ensure hook runtime for the new frame target and enqueue/persist context frame/tool schema deltas.
7. Only after the fact write and runtime reconstruction succeed, emit `workspace_module_presented` for presentation.

Implementation flow:

- Introduce one frame-first helper, e.g. `SessionCapabilityService::expose_canvas_to_frame(target/session, canvas, reason) -> AgentFrame`.
- It should:
  - read current frame and CapabilityState;
  - build a new VFS by adding the Canvas mount;
  - append `visible_canvas_mount_ids_json` and `visible_workspace_module_refs_json` on the new frame, not the old row;
  - build a new AgentFrame revision with created_by `canvas_expose` or `workspace_module_present`;
  - update active runtime cache/tools/hook runtime from that persisted revision.
- Change `canvas::tools::expose_canvas_to_session` to call this helper first, then update in-memory `SharedRuntimeVfs` from the persisted frame projection or remove the direct live VFS write if active runtime cache already owns it.
- Change `workspace_module_present` Canvas branch to run expose helper before building/sending presentation event.

Minimal first write set:

- `crates/agentdash-application/src/session/capability_service.rs`
  - Add frame revision writer for Canvas exposure.
- `crates/agentdash-application/src/canvas/tools.rs`
  - Replace live-first ordering.
- `crates/agentdash-application/src/workspace_module/tools.rs`
  - Use the helper in create/present flows and preserve event-after-fact order.
- `crates/agentdash-domain/src/workflow/repository.rs`
  - Deprecate or stop using append APIs in production paths; adding a small `create exposure revision` helper in application is preferable to broad repository mutation.
- `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs`
  - No schema change; append API can remain until tests no longer use it.

Migration:

- No migration required: `visible_canvas_mount_ids_json` exists in base schema and `visible_workspace_module_refs_json` exists in migration `0008`.

Roundtrip/recovery tests:

- `workspace_module_create(kind=canvas)` writes a new AgentFrame revision before live VFS update; latest frame contains canvas mount, visible canvas id and `canvas:{mount_id}`.
- Simulated recovery from latest frame reconstructs Canvas VFS mount and workspace module visibility without relying on `SharedRuntimeVfs`.
- `workspace_module_present(canvas)` writes/refreshes exposure before emitting `workspace_module_presented`.
- Hook runtime target after Canvas exposure is the new AgentFrame revision, matching existing expectations around stale cached hook runtime.
- Existing frontend tests for `presentation_uri` should remain unchanged.

### CE04 data model and flow

Fact shape:

- Base visibility is `CapabilityState.workspace_module` from `effective_capability_json`.
- Runtime refs are `AgentFrame.visible_workspace_module_refs_json` on the same selected frame revision.
- Resolver output should be a structured projection:
  - `modules: Vec<WorkspaceModuleDescriptor>`.
  - `base_visibility: WorkspaceModuleDimension`.
  - `runtime_refs: Vec<String>`.
  - `diagnostics: Vec<...>` for missing modules or unreadable frame refs.

Implementation flow:

1. Extract `resolve_visible_modules` and `runtime_visible_module_refs` out of `workspace_module/tools.rs` into an application resolver module, e.g. `workspace_module::visibility`.
2. Resolver input:
   - `project_id`
   - enabled extension installation repo / projection
   - Canvas repo
   - `base_visibility: WorkspaceModuleDimension`
   - optional selected `AgentFrame` or `AgentFrameRuntimeTarget`
3. Resolver reads runtime refs from the selected frame, not from raw session id whenever caller already has a target.
4. Tools consume resolver output for list/describe/invoke/present.
5. Frontend/project catalog APIs can reuse the same resolver later, but first batch can keep tool-only call sites.

Minimal first write set:

- `crates/agentdash-application/src/workspace_module/visibility.rs` new module.
- `crates/agentdash-application/src/workspace_module/tools.rs` replace local functions with resolver calls.
- `crates/agentdash-application/src/workspace_module/mod.rs` export internal resolver types if needed.
- Tests currently in `workspace_module/tools.rs` can move or add focused resolver tests:
  - base All returns extensions + canvases;
  - base Allowlist filters;
  - runtime refs extend allowlist;
  - missing runtime ref produces diagnostic but does not fabricate a module.

Migration:

- No migration required.

Roundtrip/recovery tests:

- Persist a frame with `CapabilityState.workspace_module = Allowlist(["ext:a"])` and `visible_workspace_module_refs_json=["canvas:x"]`; resolver returns both `ext:a` and `canvas:x`.
- A recovered frame without active session returns the same module list as an active tool call.
- `workspace_module_describe/present/invoke` all call the same resolver, so NotFound behavior is consistent.

### CE05 dependency and ordering

Recommended order:

1. CE05 first, as a narrow design/code-boundary check: define `CapabilityResolver.granted_capability_keys` as compile-time/input-only compatibility, not a runtime surface fact. This prevents CE02 from adding more direct active-grant reads into resolver paths.
2. CE02 next: PermissionGrant approve/revoke/expire writes AgentFrame capability revisions. This establishes the runtime capability effect pattern.
3. CE03 after CE02: Canvas expose uses the same AgentFrame revision write/adopt pattern, but adds VFS, skill baseline and hook runtime surface.
4. CE04 after CE03: WorkspaceModule resolver reads the final frame exposure shape and removes local visibility duplication.

CE02 and CE03 can share a small application helper for “build exposure/capability revision then adopt into active runtime”. CE04 should not start before CE03 decides the exact runtime refs source.

### Focused validation commands

```powershell
cargo test -p agentdash-application permission::service
cargo test -p agentdash-application permission::compiler
cargo test -p agentdash-application workspace_module::tools
cargo test -p agentdash-application session::hub::tests::canvas
cargo test -p agentdash-application session::capability_state
pnpm --filter app-web test -- AgentRunWorkspacePage.workspace-module.test.ts
pnpm run contracts:check
pnpm run frontend:check
```

If Cargo test filters are too broad for the current workspace naming, use the exact discovered test names from `rg`:

```powershell
cargo test -p agentdash-application create_canvas_runtime_grant_extends_allowlist_session_visibility
cargo test -p agentdash-application canvas_module_present_refreshes_session_exposure_before_event
cargo test -p agentdash-application replace_current_capability_state_requires_matching_frame_target
```

### Suggested planning artifact updates

- `design.md`
  - Add explicit AgentFrame exposure fact shape: capability JSON + VFS/MCP JSON + visible canvas/module refs on a new revision.
  - Add recovery order for Canvas expose and WorkspaceModule resolver.
  - State that first batch reuses existing columns and does not introduce exposure table.
- `implement.md`
  - Split CE02 into approve/revoke and expire sub-steps.
  - Add shared “persist revision then adopt active runtime” helper before Canvas/Permission call sites.
  - Add focused tests above.
- `work-items/index.md`
  - Mark CE05 as prerequisite boundary check for CE02.
  - Mark CE03 depends on CE02 shared revision/adopt helper.
  - Mark CE04 depends on CE03 final runtime refs shape.

## Caveats / Not Found

- No active Trellis task was set by `python ./.trellis/scripts/task.py current --source`; this research uses the user-specified target task directory.
- I did not find an application-level overdue grant expiry service that can apply per-grant frame effects before Expired status. Current `expire_overdue` is repository bulk update, so CE02 expire cannot be fully correct until that owner changes.
- I did not find persistence of PermissionGrant effects into `agent_frame_transitions`; the current service returns `RuntimeCapabilityTransition` but only persists the resulting AgentFrame. First batch can proceed without a migration, but planning should decide whether transition rows are required for audit/replay.
- `LifecycleAgent.current_frame_id` is not consistently advanced by all revision writers; several readers use `AgentFrameRepository::get_current(agent_id)` ordered by revision. If CE implement requires `current_frame_id` to be authoritative everywhere, add LifecycleAgentRepository updates to the shared revision writer.
- Current append APIs mutate existing AgentFrame rows. They should not be used by CE03 production flows after the revision writer lands, but removing them entirely may require adjusting existing tests and memory repositories.
