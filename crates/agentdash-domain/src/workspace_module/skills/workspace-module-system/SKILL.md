---
name: workspace-module-system
description: AgentDashboard workspace module operating guide. Use when a session has workspace_module tools, when creating, attaching, or copying Canvas modules, when invoking or presenting workspace modules, or when deciding whether to use workspace_module_operate, workspace_module_list, workspace_module_describe, workspace_module_invoke, or workspace_module_present.
---

# Workspace Module System

Use workspace module tools as the Agent-facing entry for project capabilities that appear as modules.

## Core Flow

1. Use `workspace_module_operate` only when a module instance must be materialized or platform-level ownership/visibility behavior must run.
2. Use `workspace_module_list` to find existing modules visible to the current session.
3. Use `workspace_module_describe(module_id)` before invoking operations or presenting UI.
4. Use `workspace_module_invoke(module_id, operation_key, input)` only for operations returned by describe.
5. Use `workspace_module_present(module_id, view_key)` only for UI entries returned by describe.

## Module Ids

- Canvas modules use `canvas:{canvas_mount_id}`.
- Extension modules use `ext:{extension_key}`.
- Builtin modules use `builtin:{key}` when the platform exposes one.

## Canvas Modules

- Create a personal Canvas with `workspace_module_operate(operation="canvas.create_personal", input={ canvas_mount_id?, title, description? })`.
- Attach an existing Canvas with `workspace_module_operate(operation="canvas.attach_existing", input={ canvas_mount_id })`.
- Copy a read-only shared Canvas before editing with `workspace_module_operate(operation="canvas.copy_to_personal", input={ source_canvas_mount_id, canvas_mount_id?, title?, description? })`.
- The created or attached module is `canvas:{canvas_mount_id}`.
- The current session can edit Canvas files after create or present through `{canvas_mount_id}://...`.
- The Canvas presentation URI is `canvas://{canvas_mount_id}`; `{canvas_mount_id}://...` is the authoring VFS URI.
- Treat `workspace_module_describe` as the source of truth for Canvas operations. Invoke only operations returned in the descriptor.
- Bind Canvas data by describing the module, then invoking the `canvas.bind_data` operation on that same `canvas:{canvas_mount_id}` module.
- Inspect the user-visible runtime state by invoking `canvas.inspect_render_state`; it returns the latest render observation reported by the Canvas iframe and does not modify conversation history.
- Inspect Canvas-exposed UI state by invoking `canvas.get_interaction_state`; it returns the latest interaction snapshot explicitly published by Canvas source and does not modify conversation history.
- Use the lifecycle-projected `canvas-system` skill for source editing, runtime bridge usage, data binding details, and Canvas UI quality rules.

## Extension Modules

- Treat describe output as the source of truth for operation keys, input schemas, UI entries, and renderer metadata.
- Provider and host services perform final validation. Use operation keys and iframe runtime action shapes returned by describe.

## Visibility

The visible module set is session-scoped. A module that is not returned by `workspace_module_list` or `workspace_module_describe` is not callable from the current session unless `workspace_module_operate` materializes and grants it during this session.
