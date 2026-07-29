---
name: workspace-module-system
description: AgentDashboard workspace module operating guide. Use when a session has workspace_module tools, when creating, attaching, or copying Canvas modules, when invoking or presenting workspace modules, or when deciding whether to use workspace_module_operate, workspace_module_list, workspace_module_describe, workspace_module_invoke, or workspace_module_present.
---

# Workspace Module System

Workspace Module 是通用 provider system。系统内嵌 provider 与后续用户提供 provider 使用同一
descriptor、operation 与 presentation 路由；Canvas 只是其中一个系统内嵌 provider。

## Core Flow

1. Use `workspace_module_operate` only for provider-declared lifecycle routes such as
   `canvas.create`、`canvas.attach`、`canvas.copy`.
2. Use `workspace_module_list` to find existing modules visible to the current session.
3. Use `workspace_module_describe(module_id)` before invoking operations or presenting UI.
4. Use `workspace_module_invoke(module_id, operation_key, input)` only for operations returned by describe.
5. Use `workspace_module_present(module_id, view_key)` only for UI entries returned by describe.

## Module Ids

- Canvas definition modules use `canvas:{definition_id}`.
- Attached Canvas runtime modules use `interaction:{instance_id}`.
- Extension modules use `ext:{extension_key}`.
- Builtin modules use `builtin:{key}` when the platform exposes one.
- User-provided providers own their module IDs and kind strings; treat both as opaque.

## Canvas Modules

- Create a Canvas with `workspace_module_operate(operation="canvas.create", input={ canvas_mount_id?, title, description? })`.
- Attach an existing Canvas with `workspace_module_operate(operation="canvas.attach", input={ canvas_mount_id })`.
- Copy a read-only shared Canvas before editing with `workspace_module_operate(operation="canvas.copy", input={ source_mount_id, canvas_mount_id?, title?, description? })`.
- The authoring module is `canvas:{definition_id}`; operate result supplies its separate
  `canvas_mount_id`.
- Create/copy/attach make the authoring mount available through `{canvas_mount_id}://...`.
- Present creates or reuses presentation attachment and returns `interaction://{instance_id}`;
  it does not mount source or change AgentFrame.
- The definition preview URI is `canvas://{definition_id}`; `{canvas_mount_id}://...` is the
  authoring VFS URI.
- Treat `workspace_module_describe` as the source of truth for Canvas operations. Invoke only operations returned in the descriptor.
- After present, describe the returned `interaction:{instance_id}` runtime module before invoking
  `canvas.bind_data`、`canvas.inspect` or `canvas.get_interaction_state`.
- Inspect the user-visible runtime state by invoking `canvas.inspect`; it returns the latest render observation reported by the Canvas preview and does not modify conversation history.
- Inspect Canvas-exposed UI state by invoking `canvas.get_interaction_state`; it returns the latest interaction snapshot explicitly published by Canvas source and does not modify conversation history.
- Use the lifecycle-projected `canvas-system` skill for source editing, runtime bridge usage, data binding details, and Canvas UI quality rules.

## Extension Modules

- Treat describe output as the source of truth for operation keys, input schemas, UI entries, and renderer metadata.
- Provider and host services perform final validation. Use operation keys and iframe runtime action shapes returned by describe.

## Visibility

The visible module set is actor-scoped. A module or Operation that is absent from current
list/describe is not callable. Re-discover after provider authority, attachment, revision or
readiness changes.
