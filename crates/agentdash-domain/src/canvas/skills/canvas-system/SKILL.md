---
name: canvas-system
description: AgentDashboard Canvas authoring guide. Use when a session has workspace_module access for Canvas work, a canvas:{canvas_mount_id} workspace module, or a canvas VFS mount; use for creating or editing runnable React/HTML/CSS Canvas assets, binding VFS data into Canvas previews, rendering VFS image assets, exposing Canvas interaction state, submitting explicit Canvas user actions to the current AgentRun, diagnosing rendered Canvas state, or calling session runtime actions from Canvas UI.
---

# Canvas System

Use this skill when working with AgentDashboard Canvas assets.

## Core Flow

1. Use `workspace_module_operate(operation="canvas.create", input={...})` to create a new editable Canvas, or `workspace_module_operate(operation="canvas.copy", input={ source_mount_id })` before editing a read-only shared Canvas source.
2. Use `workspace_module_list` and `workspace_module_describe(module_id="canvas:{canvas_mount_id}")` to inspect existing Canvas modules, UI entries, and available operations.
3. Confirm that the module exposes source mutation operations before editing. Project shared Canvas modules can be previewed but may be source read-only.
4. Edit canvas source through VFS tools, usually `fs_apply_patch` against `{canvas_mount_id}://...`, only when the mount exposes write capability.
5. Bind external data with `workspace_module_invoke` on the `canvas:{canvas_mount_id}` module operation `canvas.bind_data` when the Canvas needs session VFS facts and the operation is present.
6. Call `workspace_module_present(module_id="canvas:{canvas_mount_id}", view_key="preview")` when the Canvas is ready for user inspection.
7. Diagnose a presented Canvas with `workspace_module_invoke` operations `canvas.inspect_render_state` and `canvas.get_interaction_state` when those operations appear in `workspace_module_describe`.

## Core Rules

- A Canvas is either a personal source owned by a user or a project shared source published for project use.
- Personal Canvas sources are editable by their owner. Project shared Canvas sources are previewable/readable by project members and source read-only by default.
- Edit a project shared Canvas by copying it to a personal Canvas first, then modify the personal copy.
- Each attached canvas is a workspace module named `canvas:{canvas_mount_id}` and is exposed as a VFS mount named `{canvas_mount_id}`.
- Writable Canvas mounts support `read`, `write`, `list`, and `search`; read-only Canvas mounts support `read`, `list`, and `search`; Canvas mounts do not support `exec`.
- Use mount URIs such as `cvs-demo://src/main.tsx`; mount URIs are the stable authoring address instead of backend ids or absolute paths.
- Use `canvas://{canvas_mount_id}` only as the presentation URI opened by `workspace_module_present`; use `{canvas_mount_id}://...` for file edits.
- The `canvas-system` guide is provided by the session lifecycle skill surface; Canvas files contain only runnable asset source and supporting data files.

## Source Files

- The default entry is `src/main.tsx`; change `entry_file` only when the target file exists.
- The preview runtime transpiles `.ts`, `.tsx`, `.js`, and `.jsx` as ES modules.
- `.css` files are collected into the preview document automatically.
- `.json` files can be imported as modules; data bindings materialize as typed text files under `bindings/`.
- Prefer small, explicit modules under `src/`; import local files with `./`, `../`, or `/`.
- React and `react-dom/client` are available through the canvas import map by default.

## Data Bindings

- Call `workspace_module_describe(module_id="canvas:{canvas_mount_id}")` before binding and follow the described `canvas.bind_data` input schema.
- Invoke `canvas.bind_data` with `workspace_module_invoke` to map a VFS `source_uri` to `bindings/<alias>.<ext>`.
- `canvas.bind_data` writes an AgentRun-scoped runtime binding; it does not edit the Canvas source. Copy a shared Canvas only when the source files themselves must change.
- `alias` must be a plain name without `/` or `\`.
- `content_type` is optional; omitted values are inferred from the `source_uri` extension.
- JSON bindings use `bindings/<alias>.json`; CSV, Markdown, HTML, CSS, JavaScript, SVG, YAML, XML, and plain text bindings keep matching text-oriented extensions.
- At preview and mount exposure time the runtime tries to read each `source_uri` from the session VFS. If it cannot resolve the source, JSON binding files remain `null` and non-JSON text binding files remain empty.
- Canvas mounts expose resolved binding files as read-only generated files. Change a binding through `canvas.bind_data`, not by writing directly to `bindings/<alias>.<ext>`.
- Data bindings are for text data. Do not use them to inline binary image bytes.

In canvas code, import bound data with a relative path:

```ts
import stats from "../bindings/stats.json";
```

## Runtime Bridge

Read `references/runtime-bridge.md` when Canvas source needs:

- `window.agentdash.invoke(...)` session runtime actions.
- `window.agentdash.assets.url(...)` VFS image rendering.
- `window.agentdash.interaction.*` to expose Agent-visible form, selection, filter, or recent-event state.
- `window.agentdash.agent.submit(...)` to submit structured user feedback or follow-up context from Canvas to the current AgentRun mailbox.
- Agent-side Canvas module operations and URI boundaries.

`runtime-bridge.md` routes to focused references for RuntimeGateway actions, VFS image assets, interaction state, submit-to-Agent, and Agent-side interfaces.

## References

- If you need to choose which Canvas bridge path fits the task, read `references/runtime-bridge.md` first; it maps user intent to the focused references below.
- If a Canvas button should call a tool/runtime capability and show the result inside the Canvas, read `references/runtime-actions.md`, then call `window.agentdash.invoke(...)` from the click/submit handler.
- If the Canvas needs to render images stored in session VFS mounts, read `references/vfs-assets.md`, then resolve image URLs through `window.agentdash.assets.url(...)` instead of putting mount URIs directly in `src`.
- If the Agent should understand the user's current selection, form values, filters, or recent UI actions, read `references/interaction-state.md`, then publish compact semantic state with `window.agentdash.interaction.setState/emit`.
- If Canvas UI should let the user submit structured feedback, a decision, or follow-up context from the current surface, read `references/agent-submit.md`, then call `window.agentdash.agent.submit(...)` and include interaction/render facts only when they matter.
- If you are operating from the Agent side rather than writing iframe code, read `references/agent-side-interfaces.md`, then use workspace module operations for create/copy/attach, bind, diagnose, and present.

## Quality Rules

- Build the actual usable canvas first; avoid landing-page copy unless the user asked for it.
- Keep UI text compact and sized for the preview container.
- Do not put cards inside cards. Use cards only for repeated items, modals, or framed tools.
- Avoid decorative gradient orbs and one-note palettes.
- After edits, present the canvas through `workspace_module_present` so the user can verify the rendered state.
