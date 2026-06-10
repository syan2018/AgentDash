# Implement Plan - Canvas Agent Tools Converge Into workspace_module

## Phase 0 - Pre-Implementation Checks

- [ ] Review `prd.md` and `design.md` with the user; confirm hard cut from `canvas` capability to `workspace_module`.
- [ ] Load backend, frontend, cross-layer, and shared Trellis spec indexes before editing.
- [ ] Inspect seed/fixture/default config paths for saved `canvas` capability directives.

## Phase 1 - Canvas Use Case Extraction

- [ ] Split host logic from `crates/agentdash-application/src/canvas/tools.rs` into reusable application functions:
  - list project canvases
  - create or attach Canvas and expose to session
  - bind Canvas data
  - present Canvas with session VFS/capability refresh
- [ ] Keep existing Canvas domain/repository/VFS behavior unchanged.
- [ ] Update old Canvas AgentTool wrappers only as temporary callers during the refactor; they should not remain the final Agent-facing path.

## Phase 2 - Workspace Module Canvas Operations

- [ ] Extend workspace module contract dispatch with host-owned Canvas operation routing.
- [ ] Add `builtin:canvas` module for create/list entry operations.
- [ ] Add instance operations to `canvas:{mount_id}` modules:
  - `canvas.bind_data`
- [ ] Ensure `canvas:{mount_id}` modules expose a Canvas UI entry for `workspace_module_present`.
- [ ] Ensure module visibility filtering applies before invoke/present, so `visible_workspace_module_refs` controls Canvas instance operations and UI entries.
- [ ] Make Canvas UI entry include a frontend presentation URI such as `canvas://{mount_id}`.

## Phase 3 - Invoke / Present Semantics

- [ ] Route `workspace_module_invoke` host Canvas operations to the extracted Canvas use cases.
- [ ] Route `workspace_module_present` for Canvas renderer to the extracted Canvas present/session-exposure use case.
- [ ] Ensure Canvas present updates runtime VFS and capability state before emitting `workspace_module_presented`.
- [ ] Keep `workspace_module_present` lightweight for non-Canvas renderers that do not require session exposure.
- [ ] Standardize event payload fields:
  - `module_id`
  - `view_key`
  - `renderer_kind`
  - `presentation_uri`
  - `title`

## Phase 4 - Capability Hard Cut

- [ ] Update SPI capability constants, well-known key list, cluster mapping, and platform tool descriptors so `workspace_module` is the Canvas Agent capability.
- [ ] Update `CapabilityState::all()` and session plan conditional tool list to include `workspace_module_*` instead of Canvas tools.
- [ ] Update application tool provider to stop injecting Canvas AgentTool wrappers under normal capability resolution.
- [ ] Update capability notification text and frontend capability picker metadata.
- [ ] Add a forward migration for `project_agents.config` JSON capability directives from `canvas` to `workspace_module`.
- [ ] Update Trellis specs for tool capability and capability dimension behavior.

## Phase 5 - Agent Skill Guidance

- [ ] Add an embedded `workspace-module-system` skill bundle under the appropriate domain boundary.
- [ ] Register it in builtin SkillAsset templates so it can be bootstrapped like `companion-system` / `routine-memory`.
- [ ] Project the skill into sessions that receive `workspace_module` capability, preferably through lifecycle SkillAsset projection rather than Canvas mount materialization.
- [ ] Keep `SKILL.md` concise: workflow, module id shapes, list/describe/invoke/present flow, Canvas and Extension usage notes.
- [ ] Update `canvas-system` to make Canvas creation use `workspace_module_invoke` and presentation use `workspace_module_present`, while preserving Canvas source authoring rules.
- [ ] Run skill validation with `skill-creator/scripts/quick_validate.py` on the new/updated skill folders.

## Phase 6 - Frontend

- [ ] Update `SessionPage` handling of `workspace_module_presented` to use `presentation_uri`.
- [ ] Remove dependence on `activeCanvasId` for workspace-module-driven Canvas opens, or make it derived state only.
- [ ] Ensure `WorkspacePanel` and Canvas tab open `canvas://{mount_id}` reliably.
- [ ] Update Project Settings / agent preset UI copy only where it describes the real capability surface.

## Phase 7 - Verification

- [ ] `pnpm run migration:guard`
- [ ] `cargo test -p agentdash-application workspace_module`
- [ ] `cargo test -p agentdash-application canvas`
- [ ] Focused tests for builtin skill asset bootstrap/projection when `workspace_module` capability is active.
- [ ] Focused frontend type/lint checks for touched workspace-panel, session, and capability picker files.
- [ ] `pnpm run backend:clippy` if backend shared capability/tool surfaces changed broadly.

## Risky Files

- `crates/agentdash-spi/src/platform/tool_capability.rs`
- `crates/agentdash-spi/src/connector/mod.rs`
- `crates/agentdash-application/src/workspace_module/mod.rs`
- `crates/agentdash-application/src/workspace_module/tools.rs`
- `crates/agentdash-application/src/canvas/tools.rs`
- `crates/agentdash-application/src/vfs/tools/provider.rs`
- `crates/agentdash-application/src/session/plan.rs`
- `crates/agentdash-infrastructure/migrations/*`
- `packages/app-web/src/pages/SessionPage.tsx`
- `packages/app-web/src/features/workspace-panel/*`
- `packages/app-web/src/features/project/agent-preset-editor/*`

## Rollback Points

- After Phase 1, Canvas use cases should still be callable by existing wrappers and tests.
- After Phase 3, workspace module Canvas operations should work before removing old Canvas capability injection.
- After Phase 4 migration/catalog changes, tool schema snapshots and capability picker behavior must be reviewed before broader cleanup.
