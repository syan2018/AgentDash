# Agent-Side Canvas Interfaces

Use this reference when the Agent needs workspace module operations for Canvas work. These are Agent tools, not browser runtime APIs.

## Module Lifecycle

- `workspace_module_operate(operation="canvas.create", input={...})`: create a new editable Canvas and return the `canvas:{canvas_mount_id}` descriptor.
- `workspace_module_operate(operation="canvas.attach", input={ canvas_mount_id })`: attach an existing Canvas to the current AgentRun workspace surface.
- `workspace_module_operate(operation="canvas.copy", input={ source_canvas_mount_id, canvas_mount_id?, title?, description? })`: copy a read-only shared Canvas before editing.
- `workspace_module_list`: inspect project workspace modules, including existing Canvas modules named `canvas:{canvas_mount_id}`.
- `workspace_module_describe(module_id="canvas:{canvas_mount_id}")`: inspect the Canvas module UI entries and operation schemas before invoking or presenting it.

## Module Operations

- `workspace_module_invoke(module_id="canvas:{canvas_mount_id}", operation_key="canvas.bind_data", input={...})`: map a VFS `source_uri` to `bindings/<alias>.<ext>` using the operation schema returned by describe; the extension follows explicit `content_type` or is inferred from `source_uri`.
- `workspace_module_invoke(module_id="canvas:{canvas_mount_id}", operation_key="canvas.inspect_render_state", input={})`: read the latest render observation reported by the Canvas iframe, including ready/error status, viewport, DOM summary, and diagnostics.
- `workspace_module_invoke(module_id="canvas:{canvas_mount_id}", operation_key="canvas.get_interaction_state", input={})`: read the latest interaction snapshot explicitly exposed by Canvas source.
- `workspace_module_present(module_id="canvas:{canvas_mount_id}", view_key="preview")`: expose the Canvas runtime surface to the current session and open its `presentation_uri`.

## URI Boundaries

- `canvas://{canvas_mount_id}` is the presentation URI for the WorkspacePanel Canvas tab.
- `{canvas_mount_id}://...` is the Agent-editable VFS URI for Canvas files.
- Backend ids and absolute paths are not browser runtime API inputs.
- VFS tools edit files through `{canvas_mount_id}://...`; Canvas mounts support read/write/list/search, not exec.
