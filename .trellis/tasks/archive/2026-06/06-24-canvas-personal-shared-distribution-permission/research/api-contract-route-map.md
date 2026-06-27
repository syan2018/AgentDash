# Research: API/contract route map

- Query: Canvas personal/shared distribution API, contract DTO, route authorization, and Phase A integration map.
- Scope: internal
- Date: 2026-06-24

## Findings

### Files Found

- `.trellis/tasks/06-24-canvas-personal-shared-distribution-permission/prd.md` - source requirements for personal Canvas, project shared Canvas, publish/copy/unpublish, and read-only shared source semantics.
- `.trellis/tasks/06-24-canvas-personal-shared-distribution-permission/design.md` - target domain/access model, DTO fields, and suggested routes.
- `.trellis/tasks/06-24-canvas-personal-shared-distribution-permission/implement.md` - Phase A/B/C ordering and API/contract checklist.
- `.trellis/tasks/06-24-canvas-personal-shared-distribution-permission/research/dispatch-context.md` - current coordination state, file ownership boundaries, and confirmed auth/VFS/workspace facts.
- `.trellis/spec/cross-layer/frontend-backend-contracts.md` - Rust contract -> generated TS -> frontend facade invariant; business HTTP DTOs belong in `agentdash-contracts`.
- `.trellis/spec/backend/error-handling.md` - Domain/Application/API error mapping, especially 403/404/409/400 semantics.
- `.trellis/spec/backend/permission/architecture.md` - permission facts should be explicit and runtime-visible.
- `.trellis/spec/backend/vfs/vfs-access.md` - Canvas VFS visibility and mount identity semantics; relevant for not conflating API write access with runtime read-only mount work.
- `.trellis/spec/cross-layer/shared-library-contract.md` - Canvas publish-as-plugin remains extension package semantics, separate from project shared Canvas source distribution.
- `crates/agentdash-api/src/routes/canvases.rs` - current Canvas HTTP routes, route-local helpers, and DTO mappers.
- `crates/agentdash-api/src/auth.rs` - `CurrentUser`, `ProjectPermission`, and `load_project_with_permission` route helper.
- `crates/agentdash-spi/src/platform/auth.rs` - canonical `AuthIdentity` fields; `user_id` is the owner/check identifier.
- `crates/agentdash-application/src/project/authorization.rs` - `AuthIdentity` -> `ProjectAuthorizationContext` conversion.
- `crates/agentdash-domain/src/project/authorization.rs` - Project permission enum and role capability rules.
- `crates/agentdash-contracts/src/surface/canvas.rs` - current Canvas Rust contract DTOs.
- `crates/agentdash-contracts/src/generate_ts.rs` - current TypeScript generation entry for `canvas-contracts.ts`.
- `packages/app-web/src/generated/canvas-contracts.ts` - current generated Canvas TypeScript shape.
- `packages/app-web/src/types/canvas.ts` - current frontend Canvas type facade re-exporting generated contracts.
- `packages/app-web/src/services/canvas.ts` - current frontend service paths consuming Canvas APIs.

### Related Specs

- `.trellis/spec/cross-layer/frontend-backend-contracts.md`: new frontend-consumed Canvas request/response/query DTOs should be Rust contract types, exported through `generate_ts.rs`, then consumed from `packages/app-web/src/generated/canvas-contracts.ts`.
- `.trellis/spec/backend/error-handling.md`: application `Forbidden` maps to 403, `NotFound` to 404, `Conflict` to 409, `BadRequest`/`InvalidConfig` to 400.
- `.trellis/spec/backend/repository-pattern.md`: API should use application services via `AppState.repos`, not reach into infrastructure repositories directly.
- `.trellis/spec/backend/database-guidelines.md`: Phase A owns migration/repository updates; API worker should not repair schema or row mapping.
- `.trellis/spec/backend/vfs/vfs-access.md`: API route write permission changes must align with later VFS provider/mount capability checks but should not implement VFS behavior.
- `.trellis/spec/cross-layer/shared-library-contract.md`: current Canvas "promote-extension" path creates a packaged extension artifact and must remain distinct from project shared Canvas publish/copy routes.

### Current Canvas API Route Map

Routes are mounted under `/api`: `crates/agentdash-api/src/routes.rs:70` builds `secured_api`, merges `canvases::router()` at `crates/agentdash-api/src/routes.rs:80`, then nests the API router under `/api` at `crates/agentdash-api/src/routes.rs:121-123`.

Current `crates/agentdash-api/src/routes/canvases.rs` routes:

- `GET /api/projects/{project_id}/canvases`: `list_project_canvases`, Project view auth via `load_project_with_permission`, then `list_project_canvases_use_case`, raw `Canvas -> CanvasResponse` mapping (`crates/agentdash-api/src/routes/canvases.rs:48-64`).
- `POST /api/projects/{project_id}/canvases`: `create_canvas`, Project edit auth, then legacy `create_project_canvas` with `CreateCanvasInput` (`crates/agentdash-api/src/routes/canvases.rs:96-137`).
- `GET /api/projects/{project_id}/canvases/by-mount/{canvas_mount_id}`: Project view auth, then raw `load_canvas_by_project_mount_id`; it does not currently apply Canvas owner/scope access after mount lookup (`crates/agentdash-api/src/routes/canvases.rs:151-168`).
- `GET /api/canvases/{id}`: helper `load_canvas_with_permission(... ProjectPermission::View)` (`crates/agentdash-api/src/routes/canvases.rs:139-149`).
- `PUT /api/canvases/{id}`: helper `load_canvas_with_permission(... ProjectPermission::Edit)`, then `update_canvas_record` (`crates/agentdash-api/src/routes/canvases.rs:170-202`).
- `DELETE /api/canvases/{id}`: helper `load_canvas_with_permission(... ProjectPermission::Edit)`, then `delete_canvas_record` (`crates/agentdash-api/src/routes/canvases.rs:204-215`).
- `GET /api/canvases/{id}/runtime-snapshot`: Project view, resolves runtime surface if `session_id` is present, builds snapshot (`crates/agentdash-api/src/routes/canvases.rs:344-370`).
- `POST /api/canvases/{id}/runtime-invoke`: Project view, validates non-empty `session_id`, resolves runtime surface, then invokes runtime gateway (`crates/agentdash-api/src/routes/canvases.rs:372-415`).
- `POST /api/canvases/{id}/promote-extension`: Project edit, builds packaged extension artifact and installs it (`crates/agentdash-api/src/routes/canvases.rs:291-342`). This should remain the plugin/package path, not project shared Canvas publishing.

Current route-local helpers and mapper structure:

- `canvas_to_contract(canvas: Canvas) -> CanvasResponse` maps only identity/title/source/files/bindings/timestamps; no scope/access/lineage today (`crates/agentdash-api/src/routes/canvases.rs:217-241`).
- Domain/contract mapper pairs exist for sandbox config, files, and bindings (`crates/agentdash-api/src/routes/canvases.rs:243-289`).
- Runtime snapshot and runtime bridge mappers are separate and should stay separate from source Canvas response mapping (`crates/agentdash-api/src/routes/canvases.rs:417-535`).
- Current `load_canvas_with_permission` parses UUID, loads by id, then checks Project permission only (`crates/agentdash-api/src/routes/canvases.rs:557-569`). It must be replaced or renamed because Phase A introduces Canvas effective access.
- `parse_project_id` is the local UUID parser for path project ids (`crates/agentdash-api/src/routes/canvases.rs:571-573`).

### Current Contract And Generation Map

Current Canvas contract source:

- `CanvasResponse` currently contains `canvas_id`, `project_id`, `canvas_mount_id`, `vfs_mount_id`, title/source/files/bindings, and timestamps only (`crates/agentdash-contracts/src/surface/canvas.rs:38-53`).
- `CreateCanvasRequest` has optional `canvas_mount_id`, `description`, `entry_file`, `sandbox_config`, `files`, `bindings`, plus required `title` (`crates/agentdash-contracts/src/surface/canvas.rs:55-77`).
- `UpdateCanvasRequest` is a partial patch of title/source/files/bindings (`crates/agentdash-contracts/src/surface/canvas.rs:79-100`).
- `DeleteCanvasResponse` is `{ deleted: String }` (`crates/agentdash-contracts/src/surface/canvas.rs:102-106`).

Current TS generation:

- `generate_ts.rs` imports Canvas contract types at `crates/agentdash-contracts/src/generate_ts.rs:33-40`.
- `canvas-contracts.ts` is emitted in the Canvas section at `crates/agentdash-contracts/src/generate_ts.rs:768-796`.
- Current Canvas export list includes `CanvasResponse`, `CreateCanvasRequest`, `UpdateCanvasRequest`, `DeleteCanvasResponse`, runtime DTOs, and runtime invocation DTOs (`crates/agentdash-contracts/src/generate_ts.rs:775-794`).
- Root scripts: `pnpm run contracts:generate` runs `cargo run -p agentdash-contracts --bin generate_contracts_ts`, and `pnpm run contracts:check` runs the same binary with `-- --check` (`package.json:44-45`).
- The contracts crate binary is `generate_contracts_ts` at `crates/agentdash-contracts/src/generate_ts.rs` (`crates/agentdash-contracts/Cargo.toml:10-12`).

Recommended contract changes for the API/contract worker:

- Add a generated `CanvasScopeDto` enum with serde `snake_case`: `personal | project`.
- Add generated `CanvasAccessDto` with:
  - `can_view`
  - `can_edit_source`
  - `can_publish`
  - `can_manage_shared`
  - `can_copy`
  - `runtime_write_allowed`
- Extend `CanvasResponse` with stable response fields:
  - `owner_user_id: Option<String>`
  - `scope: CanvasScopeDto`
  - `access: CanvasAccessDto`
  - `published_from_canvas_id: Option<String>`
  - `shared_canvas_id: Option<String>`
  - `cloned_from_canvas_id: Option<String>`
  - `published_at: Option<String>`
  - `published_by_user_id: Option<String>`
- Prefer stable nullable response fields for lineage/published metadata rather than omitting them. Existing `ProjectResponse.cloned_from_project_id` is `Option<String>` without `skip_serializing_if`, so the generated response field stays stable and nullable (`crates/agentdash-contracts/src/project/contract.rs:229-243`).
- Add generated `CanvasListScopeDto` enum or query DTO supporting `scope=all|mine|shared`. `McpPreset` already keeps frontend-consumed query DTO in contracts and uses it directly in routes (`crates/agentdash-api/src/routes/mcp_presets.rs:20-26`, `crates/agentdash-api/src/routes/mcp_presets.rs:77-82`).
- Add generated request DTOs:
  - `PublishCanvasToProjectRequest { canvas_mount_id?: string, title?: string, description?: string }`
  - `CopyCanvasToPersonalRequest { canvas_mount_id?: string, title?: string, description?: string }`
- Add generated response DTO:
  - `UnpublishCanvasResponse { unpublished_canvas_id: string, source_canvas_id: Option<String> }`
- `DeleteCanvasResponse` can remain for `DELETE /canvases/{id}`. `UnpublishCanvasResponse` should be separate because application returns the cleared source relationship (`UnpublishCanvasResult` currently exposes `unpublished_canvas_id` and `source_canvas_id` at `crates/agentdash-application/src/canvas/management.rs:76-80`).
- Update the `generate_ts.rs` import list and the Canvas `export_all::<...>` block for every new contract type; otherwise `pnpm run contracts:check` will fail after frontend consumes generated fields.

### Auth And Project Permission Usage

Canonical identity and route extraction:

- `CurrentUser` wraps `AuthIdentity` for business routes (`crates/agentdash-api/src/auth.rs:42-58`).
- The extractor converts authenticated request extensions into `CurrentUser` (`crates/agentdash-api/src/auth.rs:85-95`).
- `AuthIdentity` is defined in SPI with `user_id`, `subject`, `display_name`, `groups`, `is_admin`, etc.; `user_id` is the stable owner identifier (`crates/agentdash-spi/src/platform/auth.rs:62-88`).
- `agentdash-integration-api` re-exports `AuthIdentity` from SPI (`crates/agentdash-integration-api/src/auth.rs:6`).

Project permission model:

- `ProjectPermission` variants are `View`, `Edit`, `ManageSharing` (`crates/agentdash-domain/src/project/authorization.rs:28-33`).
- Project access rules: view allows admin/template/role, edit allows admin/owner/editor, sharing management allows admin/owner (`crates/agentdash-domain/src/project/authorization.rs:42-64`).
- `project_authorization_context_from_identity` maps `AuthIdentity.user_id`, group ids, and `is_admin` into `ProjectAuthorizationContext` (`crates/agentdash-application/src/project/authorization.rs:8-20`).
- API exposes `project_authorization_context(current_user)` as a local helper (`crates/agentdash-api/src/auth.rs:259-261`).
- `load_project_with_permission(state, current_user, project_id, permission)` loads the Project and calls `require_project_permission`; it returns the Project or maps denial to `ApiError::Forbidden` (`crates/agentdash-api/src/auth.rs:293-307`).

Recommended route usage after Phase A:

```rust
let current_user_context = crate::auth::project_authorization_context(&current_user);
```

- Use `load_project_with_permission` for routes that are still purely project-scoped.
- Use Phase A Canvas application services for Canvas-scoped routes because they calculate `CanvasAccessProjection` from current user + Project authorization. Avoid duplicating project authorization in the route if the called application service already does it.
- For legacy helper naming, do not keep `load_canvas_with_permission` in API if it now means Canvas effective access. Either delete the route-local helper or rename it to avoid confusing it with the Phase A application function `load_canvas_with_access`.

### Phase A Interfaces Currently Visible

Dispatch context says Phase A foundation worker is still running, so these are current visible interfaces, not final-reviewed contracts.

Current visible application/domain additions:

- `CanvasScope::{Personal, Project}` with serde `snake_case` and parser (`crates/agentdash-domain/src/canvas/value_objects.rs:32-56`).
- `CanvasAccessAction::{View, EditSource, Publish, ManageShared, Copy, RuntimeWrite}` (`crates/agentdash-domain/src/canvas/value_objects.rs:70-78`).
- `CanvasAccessProjection` booleans and `.allows(action)` (`crates/agentdash-domain/src/canvas/value_objects.rs:80-101`).
- Canvas entity fields now include `owner_user_id`, `scope`, `published_from_canvas_id`, `shared_canvas_id`, `cloned_from_canvas_id`, `published_at`, and `published_by_user_id` (`crates/agentdash-domain/src/canvas/entity.rs:9-28`).
- `Canvas::new_personal` and `Canvas::new_project_shared` constructors exist (`crates/agentdash-domain/src/canvas/entity.rs:35-63`).
- `CanvasRepository` has default `list_personal_by_owner`, `list_project_shared`, and `find_published_from` hooks (`crates/agentdash-domain/src/canvas/repository.rs:17-45`).
- `CanvasListScopeFilter::{All, Mine, Shared}` (`crates/agentdash-application/src/canvas/management.rs:48-54`).
- `CanvasWithAccess { canvas, access }` (`crates/agentdash-application/src/canvas/management.rs:56-60`).
- `PublishCanvasInput`, `CopyCanvasInput`, `UnpublishCanvasResult` (`crates/agentdash-application/src/canvas/management.rs:62-80`).
- `create_personal_canvas` currently requires Project edit and returns `CanvasWithAccess` (`crates/agentdash-application/src/canvas/management.rs:93-133`).
- `list_canvases_for_user` supports `All/Mine/Shared`, computes access, and filters by `access.can_view` (`crates/agentdash-application/src/canvas/management.rs:162-215`).
- `load_canvas_with_access` loads by UUID, requires Project view, computes Canvas access, then checks a `CanvasAccessAction` (`crates/agentdash-application/src/canvas/management.rs:230-247`).
- `publish_canvas_to_project` deep-copies authoring payload into a project shared Canvas and updates source `shared_canvas_id` (`crates/agentdash-application/src/canvas/management.rs:266-379`).
- `copy_canvas_to_personal` deep-copies authoring payload into a new personal Canvas with `cloned_from_canvas_id` (`crates/agentdash-application/src/canvas/management.rs:381-436`).
- `unpublish_project_canvas` requires `ManageShared`, deletes the shared Canvas, and clears the source `shared_canvas_id` (`crates/agentdash-application/src/canvas/management.rs:438-486`).
- Access projection rules are centralized in `canvas_access_projection` (`crates/agentdash-application/src/canvas/management.rs:550-589`).

Missing or unstable Phase A API conveniences that the API/contract worker may need:

- No `load_canvas_by_project_mount_id_with_access(...)` helper is currently visible. API can either:
  - call `load_canvas_by_project_mount_id(...)` and then `load_canvas_with_access(..., canvas.id, View)`, or
  - wait for/add an application helper after Phase A review.
- No `delete_canvas_for_user(...)` application service is currently visible. API delete can load with `View`, then branch on `canvas.scope` and `access`, but a dedicated application delete/unpublish service would be cleaner.
- `create_personal_canvas` currently requires Project edit. If product wants project viewers to create personal Canvas in a project workspace, Phase A must change this before API routes wire it.
- `copy_canvas_to_personal` currently does not restrict source scope to project shared; it allows any visible Canvas to be copied. That may be acceptable for duplicating an owned personal Canvas, but the acceptance criteria only require project shared -> personal copy.

### Proposed API Route Contract

Keep current routes and add command routes. All paths below include the global `/api` prefix through `routes.rs`.

#### List Canvases

- Route: `GET /api/projects/{project_id}/canvases?scope=all|mine|shared`
- Request DTO: generated `ListCanvasesQuery` or route `Query` using generated `CanvasListScopeDto`.
- Default: `scope=all`.
- Application call: `list_canvases_for_user(&state.repos, &current_user_context, project_id, filter)`.
- Response: `Vec<CanvasResponse>` with access/scope/lineage fields.
- Tests:
  - owner sees own personal Canvas in `mine`.
  - project member sees project shared Canvas in `shared`.
  - `all` returns current user's personal Canvas plus project shared Canvas.
  - personal Canvas owned by another user is not returned.
  - invalid `scope` returns 400.
  - each response includes `access` and `scope`.

#### Create Personal Canvas

- Route: `POST /api/projects/{project_id}/canvases`
- Request DTO: existing generated `CreateCanvasRequest`.
- Application call: `create_personal_canvas(&state.repos, &current_user_context, CreatePersonalCanvasInput { ... })`.
- Response: `CanvasResponse` for the new personal Canvas.
- Mapper: `canvas_with_access_to_contract(CanvasWithAccess)`.
- Tests:
  - created Canvas response has `scope="personal"`, `owner_user_id=current_user.user_id`, `access.can_edit_source=true`, `runtime_write_allowed=true`.
  - existing optional request fields still map through `CanvasMutationInput`.
  - project authorization follows the final Phase A rule, currently Project edit.

#### Get Canvas By Id

- Route: `GET /api/canvases/{id}`
- Application call: `load_canvas_with_access(&state.repos, &current_user_context, canvas_id, CanvasAccessAction::View)`.
- Response: `CanvasResponse`.
- Tests:
  - personal owner can get.
  - non-owner personal Canvas is forbidden or not found per final Phase A policy.
  - project shared Canvas is visible to Project viewer.

#### Get Canvas By Mount Id

- Route: `GET /api/projects/{project_id}/canvases/by-mount/{canvas_mount_id}`
- Current risk: the route only checks Project view and then returns whichever Canvas matches the mount (`crates/agentdash-api/src/routes/canvases.rs:151-168`).
- Recommended implementation:
  - parse project id;
  - load by mount id;
  - pass the loaded `canvas.id` into `load_canvas_with_access(... View)` before mapping.
- Better Phase A helper: `load_canvas_by_project_mount_id_with_access(repos, ctx, project_id, mount_id, CanvasAccessAction::View)`.
- Tests:
  - other user's personal Canvas by mount is not returned.
  - project shared Canvas by mount is returned to Project viewer.
  - invalid project id still returns 400.

#### Update Canvas Source

- Route: `PUT /api/canvases/{id}`
- Request DTO: existing generated `UpdateCanvasRequest`.
- Application flow:
  - `load_canvas_with_access(..., CanvasAccessAction::EditSource)`;
  - call `update_canvas_record` with the returned `canvas`;
  - return updated `CanvasResponse` with the already-computed `access` or recomputed access.
- Tests:
  - personal owner update allowed.
  - project shared member update forbidden.
  - project shared publisher/manager direct source update forbidden; update publish must go through `publish-to-project`.
  - response preserves access fields.

#### Delete Canvas

- Route: `DELETE /api/canvases/{id}`
- Response DTO: existing `DeleteCanvasResponse`.
- Recommended authorization:
  - load with `View` to compute access;
  - if `scope=personal`, require `access.can_edit_source`;
  - if `scope=project`, require `access.can_manage_shared` and use the same deletion semantics as `unpublish_project_canvas`.
- If Phase A adds `delete_canvas_for_user`, use that instead of branching in API.
- Tests:
  - personal owner delete allowed.
  - non-owner personal delete forbidden/not visible.
  - project shared ordinary member delete forbidden.
  - project shared manager/publisher delete/unpublish allowed and source `shared_canvas_id` is cleared.

#### Publish To Project Shared

- Route: `POST /api/canvases/{id}/publish-to-project`
- Route id: source personal Canvas id.
- Request DTO: `PublishCanvasToProjectRequest`.
- Request fields:
  - `canvas_mount_id?: string`
  - `title?: string`
  - `description?: string`
- Application call: `publish_canvas_to_project(&state.repos, &current_user_context, source_canvas_id, PublishCanvasInput { mount_id: req.canvas_mount_id, title: req.title, description: req.description })`.
- Response: `CanvasResponse` for the project shared Canvas.
- Tests:
  - personal owner with publish access can publish.
  - repeat publish updates/replaces the existing shared Canvas instead of creating an extra shared source.
  - source personal response later exposes `shared_canvas_id`.
  - project shared response has `scope="project"`, `published_from_canvas_id=source id`, `published_by_user_id=current_user.user_id`, `access.runtime_write_allowed=false`.
  - non-owner personal publish forbidden.

#### Copy To Personal

- Route: `POST /api/canvases/{id}/copy-to-personal`
- Route id: source Canvas id, normally project shared.
- Request DTO: `CopyCanvasToPersonalRequest`.
- Request fields:
  - `canvas_mount_id?: string`
  - `title?: string`
  - `description?: string`
- Application call: `copy_canvas_to_personal(&state.repos, &current_user_context, source_canvas_id, CopyCanvasInput { mount_id: req.canvas_mount_id, title: req.title, description: req.description })`.
- Response: `CanvasResponse` for the new personal Canvas.
- Tests:
  - Project viewer can copy project shared Canvas.
  - copy gets new `canvas_id` and `canvas_mount_id`.
  - copy has `scope="personal"`, `owner_user_id=current_user.user_id`, `cloned_from_canvas_id=source id`.
  - updating copy does not change shared source.
  - if final product restricts this route to shared source only, copying personal source should return 400.

#### Unpublish

- Route: `POST /api/canvases/{id}/unpublish`
- Route id: project shared Canvas id.
- Request body: none, or `{}` if frontend client requires JSON. Prefer no body at route handler level.
- Application call: `unpublish_project_canvas(&state.repos, &current_user_context, shared_canvas_id)`.
- Response DTO: `UnpublishCanvasResponse { unpublished_canvas_id, source_canvas_id }`.
- Tests:
  - publisher can unpublish own shared Canvas.
  - project owner/admin can unpublish.
  - project editor/viewer without manage/publisher status forbidden.
  - unpublishing a personal Canvas returns 400.
  - source personal `shared_canvas_id` is cleared.

#### Promote Extension

- Route remains: `POST /api/canvases/{id}/promote-extension`.
- Do not rename or merge with `publish-to-project`; this route creates packaged extension artifacts (`crates/agentdash-api/src/routes/canvases.rs:291-342`), and Shared Library spec keeps Canvas publish-as-plugin in packaged extension semantics.
- Recommended authorization after Phase A: use `CanvasAccessAction::Publish` or an explicit plugin-publish access decision. Do not allow ordinary project shared members to package shared source unless final access rules intentionally set `can_publish=true`.
- Tests:
  - route remains available for appropriate personal owner/project manager.
  - route response remains `ExtensionPackageInstallationResponse`.
  - project shared ordinary member cannot use this as a source mutation/distribution bypass.

#### Runtime Snapshot / Runtime Invoke

- Keep existing paths.
- Replace project-only helper with Canvas `View` effective access.
- Do not add write permission here. Runtime read/present should be available for project shared Canvas when Project view is allowed; source write is enforced by update/delete/VFS/WorkspaceModule paths.

### DTO Mapper Plan

Recommended route mapper shape:

```rust
fn canvas_with_access_to_contract(value: CanvasWithAccess) -> CanvasResponse {
    canvas_to_contract(value.canvas, value.access)
}

fn canvas_to_contract(canvas: Canvas, access: CanvasAccessProjection) -> CanvasResponse {
    CanvasResponse {
        canvas_id: canvas.id.to_string(),
        project_id: canvas.project_id.to_string(),
        owner_user_id: canvas.owner_user_id,
        scope: canvas_scope_to_contract(canvas.scope),
        access: canvas_access_to_contract(access),
        canvas_mount_id: canvas.mount_id,
        vfs_mount_id: canvas_vfs_mount_id(&canvas),
        title: canvas.title,
        description: canvas.description,
        entry_file: canvas.entry_file,
        sandbox_config: sandbox_config_to_contract(canvas.sandbox_config),
        files: canvas.files.into_iter().map(canvas_file_to_contract).collect(),
        bindings: canvas.bindings.into_iter().map(canvas_data_binding_to_contract).collect(),
        published_from_canvas_id: canvas.published_from_canvas_id.map(|id| id.to_string()),
        shared_canvas_id: canvas.shared_canvas_id.map(|id| id.to_string()),
        cloned_from_canvas_id: canvas.cloned_from_canvas_id.map(|id| id.to_string()),
        published_at: canvas.published_at.map(|value| value.to_rfc3339()),
        published_by_user_id: canvas.published_by_user_id,
        created_at: canvas.created_at.to_rfc3339(),
        updated_at: canvas.updated_at.to_rfc3339(),
    }
}
```

Add small pure mappers:

- `canvas_scope_to_contract(CanvasScope) -> CanvasScopeDto`
- `canvas_access_to_contract(CanvasAccessProjection) -> CanvasAccessDto`
- `canvas_list_scope_from_contract(CanvasListScopeDto) -> CanvasListScopeFilter`
- `publish_input_from_contract(PublishCanvasToProjectRequest) -> PublishCanvasInput`
- `copy_input_from_contract(CopyCanvasToPersonalRequest) -> CopyCanvasInput`

Keep existing sandbox/file/binding mapper pairs (`crates/agentdash-api/src/routes/canvases.rs:243-289`).

### Suggested API/Contract Tests

Contract-level:

- `pnpm run contracts:check` must pass after adding Rust DTO exports.
- Verify generated `packages/app-web/src/generated/canvas-contracts.ts` contains `CanvasScopeDto`, `CanvasAccessDto`, `CanvasListScopeDto` or query DTO, `PublishCanvasToProjectRequest`, `CopyCanvasToPersonalRequest`, `UnpublishCanvasResponse`, and new `CanvasResponse` fields.

API route/mapper-level:

- Add focused mapper tests in `crates/agentdash-api/src/routes/canvases.rs` if no broader route harness is available. Existing `crates/agentdash-api/tests` directory was not found; many API modules use inline `mod tests`.
- Test `canvas_with_access_to_contract` maps:
  - scope enum values;
  - all access booleans;
  - owner id;
  - `published_from_canvas_id`, `shared_canvas_id`, `cloned_from_canvas_id`;
  - `published_at` as RFC3339.
- Test query scope parser maps `mine/shared/all` to application filter and invalid values fail.
- Add route handler tests or application-backed tests for publish/copy/unpublish authorization if a reusable app-state test harness exists after Phase A.

Implementation validation entry points:

- `cargo test -p agentdash-api canvases`
- `cargo check -p agentdash-api -p agentdash-contracts`
- `pnpm run contracts:check`

### Phase A Dependency Checklist For API/Contract Worker

Do not start production API/contract edits until the main session confirms Phase A has landed/reviewed:

- Final `Canvas` domain fields and `CanvasScope` names.
- Final `CanvasAccessProjection` fields and `CanvasAccessAction` names.
- Final `CanvasWithAccess` return type or equivalent access-bearing application result.
- Final `create_personal_canvas` signature and required Project permission.
- Final `list_canvases_for_user` signature and scope filter type.
- Final `load_canvas_with_access` signature and error semantics.
- Final `publish_canvas_to_project`, `copy_canvas_to_personal`, and `unpublish_project_canvas` signatures.
- Whether Phase A adds `load_canvas_by_project_mount_id_with_access` or expects API to do two-step mount lookup + access check.
- Whether Phase A adds `delete_canvas_for_user` or expects API delete to branch on `scope` and access.
- Final PostgreSQL repository behavior for `shared_canvas_id`, `published_from_canvas_id`, and mount id uniqueness.

### External References

- No external web references were used. This research is based on local task artifacts, specs, and production code only.
- Local tooling references:
  - `pnpm@10.33.3` from `package.json:7`.
  - Contract generation command from `package.json:44-45`.
  - `agentdash-contracts` generation binary from `crates/agentdash-contracts/Cargo.toml:10-12`.

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` returned no active task in this sub-agent environment. The user supplied the explicit task path and target research file, so this file uses that path.
- Phase A foundation worker is still marked running in `research/dispatch-context.md`. Current visible domain/application code already contains Phase A-looking changes, but the API/contract worker should treat those signatures as provisional until the main session confirms Phase A is complete.
- No `crates/agentdash-api/tests` directory was found. API test placement likely needs inline `#[cfg(test)] mod tests` or an existing shared harness in another module.
- No existing Canvas API route tests were found in `crates/agentdash-api/src/routes/canvases.rs`.
- No current generated Canvas list query, publish-to-project, copy-to-personal, or unpublish DTOs exist in `crates/agentdash-contracts/src/surface/canvas.rs`.
- No current application helper named `load_canvas_by_project_mount_id_with_access` or `delete_canvas_for_user` was found.
- This research intentionally did not edit Canvas domain/application/repository/migration, frontend, VFS, WorkspaceModule, or `pi_agent` files.
