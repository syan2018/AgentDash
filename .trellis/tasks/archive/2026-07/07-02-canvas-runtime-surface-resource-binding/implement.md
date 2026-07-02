# Canvas runtime surface 与资源绑定收束 Implement Plan

## Checklist

1. Introduce a Workspace Module runtime context interface inside `crates/agentdash-workspace-module/src/workspace_module/` that resolves delivery runtime session, Project auth identity, SharedRuntimeVfs, backend readiness, RuntimeGateway actor/context, runtime action catalog and AgentRun bridge facts from the existing tool/runtime inputs.
2. Refactor `WorkspaceModuleOperationRuntimeSource`, `WorkspaceModuleOperateCommand`, `WorkspaceModuleInvokeCommand` and `WorkspaceModulePresentCommand` so operate/invoke/present consume the shared runtime context for runtime placement facts while keeping command structs focused on user/module intent.
3. Move Canvas runtime surface update helpers (`submit_canvas_runtime_surface_update`, `request_existing_canvas_visibility_for_runtime`) onto the shared Workspace Module runtime context so Canvas `present`, `bind_data`, `inspect` and `get_interaction_state` share the same session/VFS/backend/bridge model.
4. Extend or wrap existing `CanvasAgentRunContext` resolution so AgentRun Canvas routes receive one enriched binding context that can reuse the Workspace Module runtime context and AgentRun current runtime surface projection.
5. Move AgentRun Canvas snapshot route to consume that enriched existing context.
6. Make Canvas runtime snapshot building accept explicit `resource_surface_ref` or a context object rather than deriving it only from `session_id`.
7. Update AgentRun runtime binding upsert to rebuild/return snapshot from the same context after `RuntimeSurfaceUpdateRequest::CanvasBindingChanged`.
8. Normalize Canvas runtime bridge DTO shape or introduce a clearly named action catalog field, then regenerate contracts.
9. Update frontend Canvas runtime types and `runtimeActionsForSnapshot` so it no longer probes two unrelated bridge shapes.
10. Update Extension Canvas panel binding behavior so packaged snapshots rendered inside AgentRun workspace use the existing AgentRun Canvas binding when available, or show a host-level unavailable state.
11. Add backend tests for Workspace Module runtime context diagnostics, AgentRun snapshot resource surface, binding upsert resource surface, Project mismatch, missing delivery anchor, and current frame visibility rejection through `CanvasAgentRunContext`.
12. Add frontend tests for AgentRun snapshot asset resolution and missing runtime binding diagnostics.
13. Run focused checks, then broader contract/frontend/backend checks before implementation review.

## Validation Commands

```powershell
pnpm run contracts:check
pnpm run frontend:check
cargo test -p agentdash-api canvases
cargo test -p agentdash-workspace-module canvas
cargo test -p agentdash-workspace-module workspace_module
cargo test -p agentdash-application-agentrun runtime_surface_update
```

Adjust exact Rust test filters after implementation names are known.

## Risk Points

- `crates/agentdash-application/src/canvas/diagnostics.rs` already owns `CanvasAgentRunContext`; implementation should deepen this boundary rather than creating a parallel context.
- `crates/agentdash-workspace-module/src/workspace_module/surface.rs` currently contains command structs, module resolution, Canvas operations and runtime action plumbing in one file; introduce the runtime context as a local module interface before moving call sites.
- `crates/agentdash-workspace-module/src/workspace_module/runtime_bridge.rs` owns the AgentRun bridge adapter for Canvas runtime surface updates; it should become a consumer of the shared runtime context rather than a second runtime fact assembler.
- `crates/agentdash-api/src/routes/canvases.rs` currently owns several route-local mappings; changing DTO shape here can reveal contract drift.
- `packages/app-web/src/types/canvas.ts` unions ordinary and AgentRun runtime snapshots; bridge unification should keep the UI type ergonomic.
- `packages/app-web/src/features/extension-runtime/ui/ExtensionCanvasPanel.tsx` consumes packaged snapshots that were created without runtime context; its AgentRun binding path needs explicit behavior.
- `CanvasRuntimeResourceService` currently mixes file snapshot construction, binding resolution and resource surface ref derivation; extracting context should avoid widening its responsibilities.

## Review Gates

- Confirm the final model uses the existing AgentRun Canvas binding/context as the stable owner for each AgentRun Canvas preview.
- Confirm Workspace Module operate/invoke/present/canvas runtime update acquire session/VFS/backend/bridge facts through the shared runtime context interface.
- Confirm iframe-origin requests cannot inject Project, backend, runtime session or surface identity.
- Confirm image asset resolution and runtime action invocation both validate against the same AgentRun current runtime surface.
- Confirm no compatibility-only fallback is introduced.
