---
name: workspace-module-system
description: AgentDashboard workspace module operating guide. Use when a session has workspace_module tools, when creating or attaching Canvas modules, when invoking or presenting workspace modules, or when deciding whether to use workspace_module_create, workspace_module_list, workspace_module_describe, workspace_module_invoke, or workspace_module_present.
---

# Workspace Module System

Use workspace module tools as the Agent-facing entry for project capabilities that appear as modules.

## Core Flow

1. Use `workspace_module_create` only when a new module instance must be materialized, such as `kind="canvas"`.
2. Use `workspace_module_list` to find existing modules visible to the current session.
3. Use `workspace_module_describe(module_id)` before invoking operations or presenting UI.
4. Use `workspace_module_invoke(module_id, operation_key, input)` only for operations returned by describe.
5. Use `workspace_module_present(module_id, view_key)` only for UI entries returned by describe.

## Module Ids

- Canvas modules use `canvas:{mount_id}`.
- Extension modules use `ext:{extension_key}`.
- Builtin modules use `builtin:{key}` when the platform exposes one.

## Canvas Modules

- Create or attach a Canvas with `workspace_module_create(kind="canvas", input={ canvas_id?, title?, description? })`.
- The created or attached module is `canvas:{mount_id}`.
- The current session can edit Canvas files after create or present through `cvs-<mount_id>://...`.
- The Canvas presentation URI is `canvas://{mount_id}`; `cvs-<mount_id>://...` is the authoring VFS URI.
- Bind Canvas data by describing the module, then invoking the `canvas.bind_data` operation on that same `canvas:{mount_id}` module.
- Load `canvas-system` after the Canvas VFS mount is visible, then follow it for source editing, runtime bridge usage, data binding details, and Canvas UI quality rules.

## Extension Modules

- Treat describe output as the source of truth for operation keys, input schemas, UI entries, and renderer metadata.
- Provider and host services perform final validation. Use operation keys and iframe runtime action shapes returned by describe.

## Visibility

The visible module set is session-scoped. A module that is not returned by `workspace_module_list` or `workspace_module_describe` is not callable from the current session unless `workspace_module_create` materializes and grants it during this session.
