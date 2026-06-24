# Canvas personal/shared dispatch context

## Current coordination state

- Branch: `codex/canvas-personal-shared-distribution-permission`.
- Task status: `in_progress`.
- Phase A foundation worker is running in sub-agent `019ef98d-49d7-7a63-8d3c-62907c611c9d`.
- Phase A owns the Canvas domain, application use cases, PostgreSQL repository, migration, and backend foundation tests. Other workers must not edit those files until Phase A has completed and the main session has reviewed the diff.
- Main-session role is coordination, small integration fixes, spec updates, commits, and finish-work. Implementation/check work should be dispatched to Trellis sub-agents unless it is a small local repair.
- Phase A has already produced working-tree changes in:
  - `crates/agentdash-domain/src/canvas/entity.rs`
  - `crates/agentdash-domain/src/canvas/mod.rs`
  - `crates/agentdash-domain/src/canvas/repository.rs`
  - `crates/agentdash-domain/src/canvas/value_objects.rs`
  - `crates/agentdash-application/src/canvas/management.rs`
  - `crates/agentdash-application/src/canvas/mod.rs`
  - `crates/agentdash-infrastructure/src/persistence/postgres/canvas_repository.rs`
  - `crates/agentdash-infrastructure/migrations/0023_canvas_personal_shared_distribution.sql`

## Existing unrelated working tree changes

Do not edit, revert, format, or stage these paths for this task:

- `crates/agentdash-executor/src/connectors/pi_agent/system_prompt.rs`
- `crates/agentdash-executor/src/connectors/pi_agent/prompts/`

## Confirmed code facts for workers

### Auth and project authorization

- API extractors live in `crates/agentdash-api/src/auth.rs`.
- `CurrentUser` wraps `agentdash_integration_api::AuthIdentity`.
- `AuthIdentity.user_id` is the stable current-user identifier for owner checks.
- Project authorization is already domain-backed:
  - `ProjectPermission::View`
  - `ProjectPermission::Edit`
  - `ProjectPermission::ManageSharing`
- `load_project_with_permission(...)` is the standard API helper for project-scoped authorization.
- `ProjectRole::Owner` maps to sharing management; `ProjectRole::Editor` maps to edit; `ProjectRole::Viewer` maps to view.

### Canvas backend entry points

- Current Canvas API routes are in `crates/agentdash-api/src/routes/canvases.rs`.
- Existing routes authorize Canvas mutation through Project edit; this task must switch update/delete/publish/copy/unpublish to Canvas effective access.
- Rust Canvas DTOs live in `crates/agentdash-contracts/src/surface/canvas.rs`.
- TypeScript Canvas contracts are generated to `packages/app-web/src/generated/canvas-contracts.ts`; frontend code should consume generated types through `packages/app-web/src/types/canvas.ts`.
- `pnpm run contracts:check` is the contract drift check.

### VFS runtime surface

- Canvas mount builder: `crates/agentdash-application/src/vfs/mount_canvas.rs`.
- Canvas provider: `crates/agentdash-application/src/vfs/provider_canvas.rs`.
- `build_canvas_mount(...)` currently returns read/write/list/search capabilities for every Canvas.
- The mutation dispatcher and VFS service already resolve write operations through `MountCapability::Write` before provider execution.
- `CanvasFsMountProvider::edit_capabilities(...)` currently reports create/delete/rename as enabled without checking mount write capability.
- `CanvasFsMountProvider::{write_text, delete_text, rename_text}` must reject read-only mounts in addition to relying on outer dispatcher checks.
- `provider_skill_asset.rs` is a useful local example of provider edit-capability gating based on `mount.supports(MountCapability::Write)`.

### WorkspaceModule and agent tools

- WorkspaceModule code is centered in `crates/agentdash-application/src/workspace_module/mod.rs` and `crates/agentdash-application/src/workspace_module/tools.rs`.
- Canvas module descriptors currently expose `canvas.bind_data`.
- `workspace_module_invoke` dispatches host Canvas actions under `WorkspaceModuleOperationDispatch::HostCanvas`.
- Read-only project shared Canvas must not expose `canvas.bind_data`, and invoke-side checks must still reject mutation if a stale or forged operation reaches the host action branch.

### Frontend Canvas surfaces

- Canvas service facade: `packages/app-web/src/services/canvas.ts`.
- Canvas type facade: `packages/app-web/src/types/canvas.ts`.
- Main Canvas asset UI: `packages/app-web/src/features/canvas-panel/ProjectCanvasManager.tsx`.
- Runtime/binding UI:
  - `packages/app-web/src/features/canvas-panel/CanvasRuntimePanel.tsx`
  - `packages/app-web/src/features/canvas-panel/CanvasBindingsEditor.tsx`
  - `packages/app-web/src/features/canvas-panel/CanvasFilesEditor.tsx`
- Asset category wrapper: `packages/app-web/src/features/assets-panel/categories/CanvasCategoryPanel.tsx`.
- Frontend must not hand-declare snake/camel compatibility DTOs. It should wait for generated Canvas contract fields and use `access` to drive read-only UI state.

## Dispatch plan from this point

### Running worker

- Phase A backend foundation worker:
  - Owns `crates/agentdash-domain/src/canvas/*`.
  - Owns `crates/agentdash-application/src/canvas/management.rs` and local tests/fakes needed for foundation.
  - Owns `crates/agentdash-infrastructure/src/persistence/postgres/canvas_repository.rs`.
  - Owns new migration `crates/agentdash-infrastructure/migrations/0023_canvas_personal_shared_distribution.sql`.
  - Must not touch frontend, runtime VFS, WorkspaceModule, or unrelated `pi_agent` paths.

### Safe parallel research before Phase A lands

These can run without writing production code:

- API/contract route map: exact route additions, DTO mapping, route tests, current auth helper usage.
- Runtime/WorkspaceModule map: exact capability builder/provider/tool tests to update after Phase A.
- Frontend map: exact UI components, tests, generated-contract dependencies, and service method additions.

### Safe implement workers after Phase A lands

- API/contract worker:
  - Owns `crates/agentdash-api/src/routes/canvases.rs`.
  - Owns `crates/agentdash-contracts/src/surface/canvas.rs` and `crates/agentdash-contracts/src/generate_ts.rs` only if Phase A DTO types are available.
  - Runs contract generation/check and API tests.
- Runtime worker:
  - Owns `crates/agentdash-application/src/vfs/mount_canvas.rs`.
  - Owns `crates/agentdash-application/src/vfs/provider_canvas.rs`.
  - Owns targeted VFS read-only tests.
- WorkspaceModule worker:
  - Owns `crates/agentdash-application/src/workspace_module/mod.rs`.
  - Owns `crates/agentdash-application/src/workspace_module/tools.rs`.
  - Owns targeted WorkspaceModule descriptor/invoke tests.
- Frontend worker:
  - Owns `packages/app-web/src/services/canvas.ts`.
  - Owns `packages/app-web/src/types/canvas.ts`.
  - Owns `packages/app-web/src/features/canvas-panel/*`.
  - Owns `packages/app-web/src/features/assets-panel/categories/CanvasCategoryPanel.tsx`.
  - Starts only after generated TS contract exists or the API/contract worker has committed to the exact generated shape.
- Spec/docs worker:
  - Owns positive semantic updates to Trellis specs and Canvas skill docs.
  - Should run after implementation shape is known so docs describe the final model.

## Verification matrix

Workers should preserve this matrix across HTTP, VFS, and WorkspaceModule:

| Scenario | HTTP update/delete | VFS write/delete/rename | WorkspaceModule `canvas.bind_data` |
| --- | --- | --- | --- |
| Personal Canvas owner | allowed | allowed | exposed and allowed |
| Personal Canvas non-owner | forbidden/not visible | no mount/write forbidden | not exposed |
| Project shared Canvas member | forbidden | no write capability and provider rejects | not exposed and invoke rejects |
| Project shared manager/publisher | manage through publish/unpublish/update-publish only | no direct write to shared source | not exposed for source mutation |

Read/list/search/preview/present must remain available for project shared Canvas when the user has Project view access.
