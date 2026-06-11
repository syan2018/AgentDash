---
name: canvas-system
description: AgentDashboard Canvas authoring guide. Use when a session has workspace_module access for Canvas work, a canvas:{mount_id} workspace module, or a canvas VFS mount; use for creating or editing runnable React/HTML/CSS Canvas assets, binding VFS data into Canvas previews, rendering VFS image assets, or calling session runtime actions from Canvas UI.
---

# Canvas System

Use this skill when working with AgentDashboard Canvas assets.

## Core Flow

1. Use `workspace_module_create(kind="canvas", input={...})` to create or attach a Canvas when you need a new editable surface.
2. Use `workspace_module_list` and `workspace_module_describe(module_id="canvas:{mount_id}")` to inspect existing Canvas modules, UI entries, and available operations.
3. Edit canvas source through VFS tools, usually `fs_apply_patch` against `cvs-<mount_id>://...`.
4. Bind external data with `workspace_module_invoke` on the `canvas:{mount_id}` module operation `canvas.bind_data` when the Canvas needs session VFS facts.
5. Call `workspace_module_present(module_id="canvas:{mount_id}", view_key="preview")` when the Canvas is ready for user inspection.

## Core Rules

- A canvas is a project-level runnable frontend asset stored in `Canvas.files`.
- Each attached canvas is a workspace module named `canvas:{mount_id}` and is exposed as a VFS mount named `cvs-<mount_id>`.
- Canvas mounts support `read`, `write`, `list`, and `search`; they do not support `exec`.
- Use mount URIs such as `cvs-demo://src/main.tsx`; mount URIs are the stable authoring address instead of backend ids or absolute paths.
- Use `canvas://{mount_id}` only as the presentation URI opened by `workspace_module_present`; use `cvs-<mount_id>://...` for file edits.
- Keep managed skill files intact under `skills/canvas-system/`.

## Source Files

- The default entry is `src/main.tsx`; change `entry_file` only when the target file exists.
- The preview runtime transpiles `.ts`, `.tsx`, `.js`, and `.jsx` as ES modules.
- `.css` files are collected into the preview document automatically.
- `.json` files can be imported as modules; data bindings also materialize as JSON files.
- Prefer small, explicit modules under `src/`; import local files with `./`, `../`, or `/`.
- React and `react-dom/client` are available through the canvas import map by default.

## Data Bindings

- Call `workspace_module_describe(module_id="canvas:{mount_id}")` before binding and follow the described `canvas.bind_data` input schema.
- Invoke `canvas.bind_data` with `workspace_module_invoke` to map a VFS `source_uri` to `bindings/<alias>.json`.
- `alias` must be a plain name without `/` or `\`.
- `content_type` defaults to `application/json`.
- At preview time the runtime tries to read each `source_uri` from the session VFS. If it cannot resolve the source, the binding file remains `null`.
- Data bindings are for text/JSON data. Do not use them to inline binary image bytes.

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
