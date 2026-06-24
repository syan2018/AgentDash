---
name: canvas-system
description: AgentDashboard Canvas authoring guide. Use when a session has workspace_module access for Canvas work, a canvas:{canvas_mount_id} workspace module, or a canvas VFS mount; use for creating or editing runnable React/HTML/CSS Canvas assets, binding VFS data into Canvas previews, rendering VFS image assets, or calling session runtime actions from Canvas UI.
---

# Canvas System

Use this skill when working with AgentDashboard Canvas assets.

## Core Flow

1. Use `workspace_module_create(kind="canvas", input={...})` to create or attach a Canvas when you need a new editable surface.
2. Use `workspace_module_list` and `workspace_module_describe(module_id="canvas:{canvas_mount_id}")` to inspect existing Canvas modules, UI entries, and available operations.
3. Confirm that the module exposes source mutation operations before editing. Project shared Canvas modules can be previewed but may be source read-only.
4. Edit canvas source through VFS tools, usually `fs_apply_patch` against `{canvas_mount_id}://...`, only when the mount exposes write capability.
5. Bind external data with `workspace_module_invoke` on the `canvas:{canvas_mount_id}` module operation `canvas.bind_data` when the Canvas needs session VFS facts and the operation is present.
6. Call `workspace_module_present(module_id="canvas:{canvas_mount_id}", view_key="preview")` when the Canvas is ready for user inspection.

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
- If `canvas.bind_data` is absent from the descriptor, the Canvas source is read-only in this context. Present or read it, and copy to a personal Canvas before changing bindings.
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
- MCP runtime action input/output shapes.
- Browser-side boundaries for tokens, backend ids, relay commands, and arbitrary HTTP calls.

## Quality Rules

- Build the actual usable canvas first; avoid landing-page copy unless the user asked for it.
- Keep UI text compact and sized for the preview container.
- Do not put cards inside cards. Use cards only for repeated items, modals, or framed tools.
- Avoid decorative gradient orbs and one-note palettes.
- After edits, present the canvas through `workspace_module_present` so the user can verify the rendered state.
