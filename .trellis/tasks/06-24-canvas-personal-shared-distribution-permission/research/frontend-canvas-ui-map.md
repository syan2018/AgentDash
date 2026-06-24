# Research: Frontend Canvas UI map

- Query: Frontend Canvas UI map for Canvas personal/shared distribution permission.
- Scope: internal
- Date: 2026-06-24

## Findings

### Files Found

Task and coordination context:

- `.trellis/tasks/06-24-canvas-personal-shared-distribution-permission/prd.md` - Product requirements for personal Canvas, project shared Canvas, read-only usage, copy-to-personal, API/frontend acceptance.
- `.trellis/tasks/06-24-canvas-personal-shared-distribution-permission/design.md` - Technical design defining `scope`, `access`, publish/copy/unpublish routes, and frontend Mine/Shared split.
- `.trellis/tasks/06-24-canvas-personal-shared-distribution-permission/implement.md` - Execution plan; frontend work is Phase B/B4 after Phase A API/contract stability.
- `.trellis/tasks/06-24-canvas-personal-shared-distribution-permission/implement.jsonl` - Context manifest; frontend entries include frontend architecture/type/state specs and cross-layer contract specs.
- `.trellis/tasks/06-24-canvas-personal-shared-distribution-permission/research/dispatch-context.md` - Current worker boundaries; Phase A owns backend foundation, frontend should wait for generated TS contract and not touch `pi_agent`.

Frontend production code:

- `packages/app-web/src/features/canvas-panel/ProjectCanvasManager.tsx` - Assets page Canvas list/detail composition root.
- `packages/app-web/src/features/canvas-panel/CanvasRuntimePanel.tsx` - Canvas preview/detail panel; currently loads Canvas + runtime snapshot and saves bindings.
- `packages/app-web/src/features/canvas-panel/CanvasFilesEditor.tsx` - File/entry editor component with local draft state and `onSave` prop; currently not mounted by the runtime panel.
- `packages/app-web/src/features/canvas-panel/CanvasBindingsEditor.tsx` - Binding editor component with local draft state and `onSave` prop.
- `packages/app-web/src/features/canvas-panel/CanvasRuntimePreview.tsx` - Runtime preview iframe and runtime bridge invoker; preview should stay usable for read-only shared Canvas.
- `packages/app-web/src/features/assets-panel/categories/CanvasCategoryPanel.tsx` - Assets category wrapper that directly mounts `ProjectCanvasManager`.
- `packages/app-web/src/features/workspace-panel/tab-types/canvas-tab.tsx` - Workspace tab renderer that opens `CanvasRuntimePanel` from `canvas://{canvas_mount_id}`.
- `packages/app-web/src/features/workspace-panel/model/canvasModuleOpen.ts` - Workspace module Canvas option selection and open flow.
- `packages/app-web/src/features/extension-runtime/ui/ExtensionCanvasPanel.tsx` - Packaged extension `canvas_panel` renderer that loads a package snapshot and uses `CanvasRuntimePreview` only.
- `packages/app-web/src/services/canvas.ts` - Canvas API service facade.
- `packages/app-web/src/types/canvas.ts` - Canvas type facade over generated `canvas-contracts.ts`.
- `packages/app-web/src/generated/canvas-contracts.ts` - Generated Canvas contract file; do not edit manually.

Contract/codegen sources:

- `crates/agentdash-contracts/src/surface/canvas.rs` - Rust Canvas DTO source for generated TS.
- `crates/agentdash-contracts/src/generate_ts.rs` - Emits `packages/app-web/src/generated/canvas-contracts.ts`.

Existing frontend tests and patterns:

- `packages/app-web/src/features/canvas-panel/CanvasRuntimePreview.test.ts` - Canvas preview/runtime helper tests.
- `packages/app-web/src/pages/AgentRunWorkspacePage.workspace-module.test.ts` - Canvas presentation URI, workspace tab identity, and Canvas module open tests.
- `packages/app-web/src/services/sharedLibrary.test.ts` - Service test pattern using `vi.hoisted` API mocks.
- `packages/app-web/src/services/vfs.test.ts` - Service payload test pattern for API calls.
- `packages/app-web/src/features/assets-panel/categories/extension/ExtensionCategoryPanel.test.tsx` - Component smoke test pattern using `renderToStaticMarkup`.

### ProjectCanvasManager Current Map

Current state shape:

- `canvases: Canvas[]`, `selectedCanvasId: string | null`, `createTitle`, `createDescription`, loading/busy flags, `error`, and `message` are local `useState` values in `ProjectCanvasManager` (`packages/app-web/src/features/canvas-panel/ProjectCanvasManager.tsx:25`-`34`).
- The selected Canvas is derived from `canvases.find(canvas.canvas_id === selectedCanvasId)` via `useMemo` (`ProjectCanvasManager.tsx:79`-`82`).
- Selection is persisted in local storage under `agentdash:selected-canvas:{projectId}` (`ProjectCanvasManager.tsx:18`, `ProjectCanvasManager.tsx:36`-`39`, `ProjectCanvasManager.tsx:75`-`77`, `ProjectCanvasManager.tsx:360`-`379`).

Current service calls:

- List load calls `fetchProjectCanvases(projectId)` and replaces the local list (`ProjectCanvasManager.tsx:41`-`62`).
- Create calls `createCanvas(projectId, { title, description })`, prepends the returned Canvas, and selects it (`ProjectCanvasManager.tsx:84`-`109`).
- Delete calls `deleteCanvas(canvas.canvas_id)`, removes it locally, and picks the adjacent item (`ProjectCanvasManager.tsx:111`-`135`).
- Existing "publish" action is only plugin packaging: `promoteCanvasToExtension(canvas.canvas_id, { display_name, overwrite: true })`, followed by optional extension runtime refresh (`ProjectCanvasManager.tsx:137`-`153`). This is distinct from the new "publish to project shared Canvas" flow.

Current operation buttons:

- Left create form has title/description inputs and a create button (`ProjectCanvasManager.tsx:166`-`195`).
- List item selection is the only current edit/open action (`ProjectCanvasManager.tsx:245`-`265`).
- List item delete button is unconditional except for current delete busy state (`ProjectCanvasManager.tsx:283`-`290`).
- Detail header has a "发布为插件" button wired to `handlePromoteCanvas` (`ProjectCanvasManager.tsx:312`-`320`).

Current detail/runtime composition:

- `CanvasCategoryPanel` only resolves current Project and passes `projectId`, `projectName`, and `onExtensionRuntimeRefresh` into `ProjectCanvasManager` (`packages/app-web/src/features/assets-panel/categories/CanvasCategoryPanel.tsx:25`-`51`).
- `ProjectCanvasManager` renders a two-column layout: left list/create panel and right detail panel (`ProjectCanvasManager.tsx:155`-`356`).
- When selected, the right detail shell renders selected Canvas metadata, then mounts `CanvasRuntimePanel` with `canvasId={selectedCanvas.canvas_id}`, `sessionId={null}`, and close handler (`ProjectCanvasManager.tsx:300`-`342`).
- `CanvasRuntimePanel` can also load by `canvasMountId + projectId` when `canvasId` is null; the WorkspacePanel Canvas tab uses that path (`packages/app-web/src/features/workspace-panel/tab-types/canvas-tab.tsx:45`-`54`).

Needed Mine/Shared state changes after generated contracts stabilize:

- Replace the single "project级 Canvas list" mental model with generated `canvas.scope` and `canvas.access`.
- Add an active view state, e.g. `activeView: "mine" | "shared"`, and either:
  - call `fetchProjectCanvases(projectId, "all")` then partition by `canvas.scope`, or
  - call `fetchProjectCanvases(projectId, activeViewScope)` if the API/contract worker chooses active-scope loading.
- Prefer preserving selected Canvas per view, because a shared Canvas selection should not leak into Mine after tab switch. The existing storage key can be extended by view, e.g. `agentdash:selected-canvas:{projectId}:mine` and `...:shared`, after the UI split is implemented.
- Mine primary actions: create personal Canvas, select/edit, publish/update project shared Canvas if `access.can_publish`, delete if source editing/deletion is allowed by access.
- Shared primary actions: select/open/preview, copy to personal if `access.can_copy`, unpublish/delete shared source if `access.can_manage_shared`.
- Keep "发布为插件" as a separate action from "发布到项目共用"; current plugin route is `promote-extension` (`packages/app-web/src/services/canvas.ts:95`-`103`).

### Runtime Panel And Editor Mutation Entrypoints

`CanvasRuntimePanel` current mutation entry:

- `loadCanvasData` fetches Canvas and runtime snapshot. For ID-based load it uses `fetchCanvas(canvasId)`; for mount-based load it uses `fetchCanvasByMountId(projectId, canvasMountId)`; it then calls `fetchCanvasRuntimeSnapshot(snapshotCanvasId, sessionId)` (`CanvasRuntimePanel.tsx:41`-`71`).
- `handleBindingsSave` calls `updateCanvas(targetCanvasId, { bindings })`, updates local `canvas`, then reloads the runtime snapshot (`CanvasRuntimePanel.tsx:77`-`96`).
- The bottom detail drawer shows current resolved binding status and always renders `CanvasBindingsEditor` when open (`CanvasRuntimePanel.tsx:219`-`243`).
- The runtime preview renders from `snapshot` and should remain available for read-only shared Canvas (`CanvasRuntimePanel.tsx:159`-`161`).
- File browsing button opens the VFS tab from `vfsMountId`; this is read path UI and can remain visible if the runtime surface exposes browsable read-only mount (`CanvasRuntimePanel.tsx:175`-`182`).

Access gating needed in `CanvasRuntimePanel`:

- Compute `canEditSource = canvas?.access.can_edit_source === true` after generated `CanvasResponse.access` exists.
- Do not render source mutation editors when `canEditSource` is false, or render them in an explicit read-only state. Preview and binding status may remain visible.
- Guard `handleBindingsSave` before `updateCanvas`. UI hiding is not enough because WorkspacePanel can open the same panel by mount id (`canvas-tab.tsx:45`-`54`), and stale component state could still try to save.
- If `CanvasFilesEditor` is mounted here later, add a `handleFilesSave` that calls `updateCanvas(targetCanvasId, { entry_file: input.entryFile, files: input.files })`, then refreshes Canvas + snapshot; gate it with the same `canEditSource` check.
- For read-only shared Canvas, show a compact state explaining that source edits require copying to Mine. The copy action itself belongs at the list/detail action level, not inside the low-level editor.

`CanvasFilesEditor` current mutation entry:

- Local draft state is `draftFiles`, `draftEntryFile`, `selectedFilePath`, and `isDirty` (`packages/app-web/src/features/canvas-panel/CanvasFilesEditor.tsx:38`-`41`).
- Draft reset from props happens on `value` / `entryFile` change (`CanvasFilesEditor.tsx:43`-`50`).
- Mutations are local until save: path change (`CanvasFilesEditor.tsx:75`-`86`), content change (`CanvasFilesEditor.tsx:88`-`93`), add file (`CanvasFilesEditor.tsx:95`-`103`), remove file (`CanvasFilesEditor.tsx:105`-`118`), set entry (`CanvasFilesEditor.tsx:211`-`220`), and save (`CanvasFilesEditor.tsx:132`-`147`).
- `handleSave` normalizes and calls `onSave({ entryFile, files })` (`CanvasFilesEditor.tsx:136`-`142`).
- This component is currently not used anywhere except its own file; `rg` found no production mount outside `CanvasFilesEditor.tsx`.

Access gating needed in `CanvasFilesEditor`:

- Either only mount it when `can_edit_source` is true, or add `readOnly?: boolean` / `editable?: boolean` and apply it to add, path/content inputs, set-entry, remove, cancel/save controls.
- If a read-only prop is added, `canSave` should include it and `handleSave` should return early when read-only, not only disable the button.
- When read-only, preserve file viewing if desired, but make source mutations unreachable.

`CanvasBindingsEditor` current mutation entry:

- Local draft state is `draftBindings` plus `isDirty` (`packages/app-web/src/features/canvas-panel/CanvasBindingsEditor.tsx:71`-`72`).
- Draft reset from props happens on `value` change (`CanvasBindingsEditor.tsx:74`-`80`).
- Local mutations are binding field change (`CanvasBindingsEditor.tsx:86`-`103`), add binding (`CanvasBindingsEditor.tsx:105`-`108`), remove binding (`CanvasBindingsEditor.tsx:110`-`113`), cancel (`CanvasBindingsEditor.tsx:115`-`119`), and save (`CanvasBindingsEditor.tsx:121`-`132`).
- Save normalizes bindings and calls parent `onSave(normalized)` (`CanvasBindingsEditor.tsx:125`-`128`).
- Inputs and buttons are currently disabled only by `isSaving`, not access (`CanvasBindingsEditor.tsx:140`-`147`, `CanvasBindingsEditor.tsx:164`-`188`, `CanvasBindingsEditor.tsx:197`-`203`, `CanvasBindingsEditor.tsx:215`-`230`).

Access gating needed in `CanvasBindingsEditor`:

- Add the same read-only/editable prop if the component can render in read-only detail; otherwise do not mount it for read-only Canvas.
- If rendered read-only, disable or hide add/remove/input/save and make `handleSave` return early when editing is not allowed.
- Parent `CanvasRuntimePanel.handleBindingsSave` must still check access before calling `updateCanvas`, because component gating is not an authorization boundary.

### services/canvas.ts And types/canvas.ts Contract Attachment

Current facade shape:

- `types/canvas.ts` imports generated Canvas DTOs from `../generated/canvas-contracts` (`packages/app-web/src/types/canvas.ts:2`-`21`) and re-exports aliases such as `Canvas = CanvasResponse`, `CreateCanvasInput = CreateCanvasRequest`, `UpdateCanvasInput = UpdateCanvasRequest`, and `DeleteCanvasResult = DeleteCanvasResponse` (`types/canvas.ts:23`-`57`).
- Generated TS currently has `CanvasResponse` with identity/title/files/bindings timestamps only; no scope/access/lineage fields yet (`packages/app-web/src/generated/canvas-contracts.ts:12`).
- Generated TS currently has `CreateCanvasRequest`, `UpdateCanvasRequest`, and `DeleteCanvasResponse` only for Canvas CRUD (`canvas-contracts.ts:24`-`26`, `canvas-contracts.ts:44`).
- Rust contract source has the same current `CanvasResponse`, `CreateCanvasRequest`, `UpdateCanvasRequest`, and `DeleteCanvasResponse` shape (`crates/agentdash-contracts/src/surface/canvas.rs:40`-`106`).
- Codegen emits Canvas DTOs into `canvas-contracts.ts` from `generate_ts.rs` (`crates/agentdash-contracts/src/generate_ts.rs:768`-`795`).

Current service shape:

- `fetchProjectCanvases(projectId)` performs `GET /projects/{project_id}/canvases` with no scope query (`packages/app-web/src/services/canvas.ts:12`-`16`).
- `createCanvas`, `fetchCanvas`, `fetchCanvasByMountId`, `updateCanvas`, and `deleteCanvas` map directly to current endpoints (`services/canvas.ts:18`-`53`).
- Runtime snapshot and runtime invoke are separate from source editing (`services/canvas.ts:55`-`93`).
- `promoteCanvasToExtension` posts to `/canvases/{id}/promote-extension` and returns `ExtensionPackageInstallationResponse` (`services/canvas.ts:95`-`103`); this should remain the plugin path.

Recommended service/type landing after API/contract generated TS is stable:

- In `types/canvas.ts`, import and alias the new generated contract types adjacent to the current Canvas aliases. Expected categories:
  - scope/access/lineage types, e.g. generated `CanvasScope*`, `CanvasAccess*` if the API/contract worker exports named DTOs.
  - publish request/response type(s).
  - copy-to-personal request/response type(s).
  - unpublish request/response type(s).
- Do not hand-declare these wire DTOs in `services/canvas.ts` or feature files. The spec requires generated wire DTO as source of truth and no frontend enum/string-union re-declaration for cross-layer DTOs.
- Extend `fetchProjectCanvases` to accept generated/list scope (`mine | shared | all`, exact type from generated contract if provided) and serialize it as `scope` query with `URLSearchParams`.
- Add source-distribution service wrappers near CRUD, before `promoteCanvasToExtension`, so the project shared flow is visibly distinct from plugin packaging:
  - `publishCanvasToProject(canvasId, input): Promise<Canvas or generated publish response>` -> `POST /canvases/{id}/publish-to-project`
  - `copyCanvasToPersonal(canvasId, input): Promise<Canvas or generated copy response>` -> `POST /canvases/{id}/copy-to-personal`
  - `unpublishCanvas(canvasId, input?): Promise<Canvas/Delete/result type from generated contract>` -> `POST /canvases/{id}/unpublish`
- If API/contract worker chooses request bodies with optional title/description override, consume the generated request type rather than introducing local `interface`.
- Keep `promoteCanvasToExtension` naming and endpoint unchanged to preserve the separate "发布为插件" product path.

### Existing Frontend Test/Check Commands And Patterns

Commands:

- `pnpm --filter app-web run typecheck` runs `tsc --noEmit -p tsconfig.app.json` (`packages/app-web/package.json:14`).
- `pnpm --filter app-web test` runs `vitest run` (`packages/app-web/package.json:15`).
- `pnpm --filter app-web lint` runs `eslint .` (`packages/app-web/package.json:16`).
- `pnpm --filter app-web run check` runs typecheck, lint, and test (`packages/app-web/package.json:17`).
- `pnpm run contracts:check` runs `cargo run -p agentdash-contracts --bin generate_contracts_ts -- --check` (`package.json:44`-`45`).
- Root `pnpm run frontend:check` is typecheck only; package-level `app-web run check` is stronger for this frontend task (`package.json:30`, `packages/app-web/package.json:17`).

Existing test patterns:

- Canvas runtime tests are pure/helper-oriented in `CanvasRuntimePreview.test.ts`; fixtures build a typed `CanvasRuntimeSnapshot` and assert parsing/build/runtime asset helper behavior (`packages/app-web/src/features/canvas-panel/CanvasRuntimePreview.test.ts:12`-`35`, `CanvasRuntimePreview.test.ts:37`-`256`).
- Service tests mock `api` with `vi.hoisted` and assert exact endpoints/payloads, as in `sharedLibrary.test.ts` (`packages/app-web/src/services/sharedLibrary.test.ts:1`-`15`, `sharedLibrary.test.ts:55`-`92`) and `vfs.test.ts` (`packages/app-web/src/services/vfs.test.ts:1`-`24`, `vfs.test.ts:32`-`100`).
- Workspace Canvas presentation tests live in `AgentRunWorkspacePage.workspace-module.test.ts`; they assert concrete `canvas://{mount}` presentation URI behavior and tab identity (`packages/app-web/src/pages/AgentRunWorkspacePage.workspace-module.test.ts:125`-`195`, `AgentRunWorkspacePage.workspace-module.test.ts:397`-`470`, `AgentRunWorkspacePage.workspace-module.test.ts:502`-`589`).
- Lightweight TSX component smoke tests use `renderToStaticMarkup`, as in `ExtensionCategoryPanel.test.tsx` (`packages/app-web/src/features/assets-panel/categories/extension/ExtensionCategoryPanel.test.tsx:1`-`15`, `ExtensionCategoryPanel.test.tsx:48`-`62`).

Recommended tests for the frontend worker:

- Add `packages/app-web/src/services/canvas.test.ts` using the service mock pattern. Cover:
  - `fetchProjectCanvases(projectId, "mine" | "shared" | "all")` query serialization.
  - no query when scope is intentionally omitted, if the service keeps an omitted-scope path.
  - publish/copy/unpublish exact endpoint and payload.
- Add focused pure helper tests if action selection is extracted, e.g. partitioning Mine/Shared lists and deriving action visibility from `canvas.access`.
- Add editor read-only tests for `CanvasBindingsEditor` and `CanvasFilesEditor` if read-only props are introduced. `renderToStaticMarkup` can verify disabled controls for static cases; handler-level helpers can verify save callbacks are not reached when read-only.
- Existing WorkspacePanel Canvas tab tests already cover presentation identity. Add/adjust only if Mine/Shared changes affect `canvas://{canvas_mount_id}` tab opening; the tab path should remain stable.

### Implementation Steps For Frontend Worker After Generated TS Stabilizes

1. Confirm generated Canvas contract:
   - `CanvasResponse` includes `scope`, `access`, owner/lineage/publish metadata expected by the design.
   - Generated request/response DTOs exist for publish/copy/unpublish, or API/contract worker documents exact returned wire shape.
   - Run or rely on upstream `pnpm run contracts:check` before frontend edits if generated files changed.

2. Update the frontend type facade:
   - Import new generated Canvas DTOs in `types/canvas.ts`.
   - Re-export aliases used by UI/services.
   - Do not add camelCase aliases, snake/camel fallback fields, or local copies of generated enums.

3. Update `services/canvas.ts`:
   - Add scoped list query support.
   - Add `publishCanvasToProject`, `copyCanvasToPersonal`, and `unpublishCanvas`.
   - Keep `promoteCanvasToExtension` separate and keep wording/plugin naming separate in UI.
   - Add `services/canvas.test.ts` for endpoints and payloads.

4. Refactor `ProjectCanvasManager` state:
   - Add Mine/Shared active view.
   - Load either all Canvas and partition by `scope`, or load the active scope using the finalized service contract.
   - Track busy states for publish/copy/unpublish separately from existing create/delete/promote-extension state.
   - Preserve per-view selection and refresh after writes. For copy success, switch to Mine and select the returned personal Canvas.

5. Add access-driven actions:
   - Mine card/detail: edit/select, publish to project shared if `access.can_publish`, delete if access allows, plugin publish as a separate secondary action.
   - Shared card/detail: open/preview/select, copy to personal if `access.can_copy`, unpublish/delete if `access.can_manage_shared`.
   - Hide or disable unavailable actions based on `canvas.access`; do not infer from current user in frontend.

6. Gate runtime detail/editor source mutation:
   - In `CanvasRuntimePanel`, derive `canEditSource` from loaded `canvas.access.can_edit_source`.
   - Guard `handleBindingsSave` and any future `handleFilesSave` before `updateCanvas`.
   - Render `CanvasBindingsEditor` / `CanvasFilesEditor` only when editable, or pass a read-only prop that disables mutation controls.
   - Keep `CanvasRuntimePreview` available for shared Canvas, because preview/present/read are accepted requirements.

7. Wire file editing if required by acceptance:
   - `CanvasFilesEditor` already supports `onSave({ entryFile, files })`, but is not mounted.
   - Mount it in the runtime detail drawer or a sibling detail section only for editable Canvas.
   - Parent save should call `updateCanvas(canvas_id, { entry_file, files })` and refresh snapshot.

8. Update UI text:
   - Rename "项目级 Canvas 列表" and wrapper copy to distinguish personal Mine and project shared.
   - Add explicit "发布到项目共用" wording for the new flow.
   - Keep "发布为插件" only for `promote-extension`.

9. Run focused then full frontend checks:
   - `pnpm run contracts:check`
   - `pnpm --filter app-web test -- services/canvas.test.ts` if using Vitest file targeting, or full `pnpm --filter app-web test` when ready.
   - `pnpm --filter app-web run check`

### Related Specs

- `.trellis/spec/frontend/architecture.md` - Project-scoped frontend state, generated runtime projections, and WorkspacePanel Canvas tab composition.
- `.trellis/spec/frontend/type-safety.md` - Generated contract source of truth; no `any`, no non-null assertions, no snake/camel compatibility; service layer should not rebuild generated DTOs field-by-field.
- `.trellis/spec/frontend/state-management.md` - Local UI state in component, derived state via `useMemo`, write operations should explicitly refetch projections when no event invalidation exists.
- `.trellis/spec/frontend/component-guidelines.md` - Named component exports, props interfaces, no component definitions inside components, Tailwind/CN conventions.
- `.trellis/spec/frontend/quality-guidelines.md` - App-web check/test commands and frontend quality gates.
- `.trellis/spec/frontend/directory-structure.md` - Canvas code belongs in `features/canvas-panel`, services in `services/canvas.ts`, generated contracts in `generated/`, global aliases in `types/canvas.ts`.
- `.trellis/spec/cross-layer/architecture.md` - Rust contract types generate TypeScript and check mode prevents drift.
- `.trellis/spec/cross-layer/frontend-backend-contracts.md` - Canvas presentation URI contract, generated DTO contract rules, and `pnpm run contracts:check` validation.
- `.trellis/spec/cross-layer/shared-library-contract.md` - Existing "Canvas 发布为插件" path creates packaged extension artifact; it is not the new project shared Canvas source publishing flow.

### External References

- No network documentation was consulted.
- Local package versions relevant to frontend work:
  - React `^19.2.0` and React DOM `^19.2.0` (`packages/app-web/package.json:53`-`54`).
  - TypeScript `~5.9.3` (`packages/app-web/package.json:73`).
  - Vite `^7.3.1` (`packages/app-web/package.json:75`).
  - Vitest `^4.0.18` (`packages/app-web/package.json:76`).
  - `ts-rs` `11.1` drives Rust-to-TypeScript contract generation (`Cargo.toml:97`).

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` reported no active task in this shell, so this research used the explicit task path supplied by the user.
- Phase A backend foundation worker is still running per `research/dispatch-context.md`; frontend implementation should not start until API/contract generated TS is stable or exact generated shape is confirmed.
- Current generated `canvas-contracts.ts` does not yet include `scope`, `access`, owner, publish lineage, copy lineage, publish/copy/unpublish request/response types, or scoped list query type.
- `CanvasFilesEditor` exists but is not currently mounted by `CanvasRuntimePanel` or `ProjectCanvasManager`; file editing may require wiring in addition to access gating.
- No existing `services/canvas.test.ts` or `ProjectCanvasManager` test was found.
- This research did not edit production code, generated files, backend files, or `pi_agent` paths.
