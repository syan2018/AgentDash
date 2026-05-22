---
name: canvas-system
description: Teaches AgentDashboard agents how to create, edit, bind data to, and present Canvas assets through canvas tools and canvas VFS mounts. Use when a session has canvas tools or a canvas mount, when building React/HTML/CSS assets in a canvas, or when resolving canvas data bindings.
---

# Canvas System

Use this skill when working with AgentDashboard Canvas assets.

## Core Model

- A canvas is a project-level runnable frontend asset stored in `Canvas.files`.
- Each attached canvas is exposed as a VFS mount named `cvs-<canvas_id>`.
- Canvas mounts support `read`, `write`, `list`, and `search`; they do not support `exec`.
- Use mount URIs such as `cvs-demo://src/main.tsx`. Do not use backend ids or absolute paths.
- Keep managed skill files intact under `skills/canvas-system/`.

## Workflow

1. Call `canvases_list` to inspect existing canvases.
2. Call `canvas_start` with either an existing `canvas_id` or a new `title`. The result returns `canvas_id`, `mount_id`, `entry_file`, and this skill path.
3. Edit canvas source files through VFS tools, usually `fs_apply_patch` against `<mount_id>://...`.
4. Use `present_canvas` when the canvas is ready for the user to inspect.

## Source Files

- The default entry is `src/main.tsx`; change `entry_file` only when the target file exists.
- The preview runtime transpiles `.ts`, `.tsx`, `.js`, and `.jsx` as ES modules.
- `.css` files are collected into the preview document automatically.
- `.json` files can be imported as modules; data bindings also materialize as JSON files.
- Prefer small, explicit modules under `src/`; import local files with `./`, `../`, or `/`.
- React and `react-dom/client` are available through the canvas import map by default.

## Data Bindings

- Call `bind_canvas_data` to map a VFS `source_uri` to `bindings/<alias>.json`.
- `alias` must be a plain name without `/` or `\`.
- `content_type` defaults to `application/json`.
- At preview time the runtime tries to read each `source_uri` from the session VFS. If it cannot resolve the source, the binding file remains `null`.
- Data bindings are for text/JSON data. Do not use them to inline binary image bytes.
- In canvas code, import bound data like:

```ts
import stats from "../bindings/stats.json";
```

Adjust the relative path from the importing source file.

## VFS Image Assets

Use `window.agentdash.assets.url(uri)` when a Canvas needs to render image files from visible VFS mounts. The `uri` must be a mount URI such as `main://docs/diagram.png`, `skill-assets://skills/demo/assets/logo.png`, or a provider-specific mount exposed in the current session.

```tsx
const src = await window.agentdash.assets.url("main://docs/diagram.png");
```

For a gallery, resolve the known image URIs before rendering:

```tsx
const images = await Promise.all(
  records.map(async (record) => ({
    ...record,
    src: await window.agentdash.assets.url(record.uri),
  })),
);
```

- `assets.url(uri)` returns a browser URL that can be used in `<img src={src}>`.
- `assets.url(uri)` only resolves images from mounts visible to the current session runtime surface.
- `assets.url(uri)` rejects if the Canvas is not bound to a session, the mount/path is invalid, the mount is unavailable, or the resource is not `image/*`.
- Call `window.agentdash.assets.revoke(src)` when you know an image URL is no longer needed; the preview runtime also cleans up URLs when the Canvas reloads.
- Do not put VFS mount URIs directly into `<img src>`. Browsers do not know how to load `mount://...` addresses.
- Do not fetch `/api/vfs-surfaces/*` directly from Canvas source.

## Runtime Bridge

Use the runtime bridge only when the canvas needs a user-triggered session runtime action. In canvas code, call:

```ts
const result = await window.agentdash.invoke("action.key", { /* action input */ });
```

- `window.agentdash.invoke(actionKey, input)` is available inside the preview iframe.
- The parent page supplies the canvas actor, session context, trace, and Gateway policy. Do not build or send those fields from canvas code.
- Treat the result as a `RuntimeInvocationResult`; provider data is under `result.output.output`.
- The promise rejects when the action is unavailable, denied, invalid, or failed. Show a compact error state in the canvas UI.
- Trigger bridge calls from explicit user actions such as buttons, forms, or refresh controls. Do not auto-run runtime actions on module load or render.
- Do not implement a runtime action discovery flow in the canvas. Use only action keys and input contracts explicitly provided by the platform, the user request, or the surrounding task context.
- Do not expose relay commands, MCP transports, backend ids, tokens, absolute paths, or arbitrary HTTP requests in canvas source.
- For the full inline API reference, read `skills/canvas-system/references/runtime-bridge.md`.

Example pattern:

```tsx
async function refresh() {
  setLoading(true);
  setError(null);
  try {
    const invocation = await window.agentdash.invoke("mcp.call_tool", {
      runtime_name: "provided_runtime_tool_name",
      arguments: { limit: 10 },
    });
    setData(invocation.output.output);
  } catch (error) {
    setError(error instanceof Error ? error.message : "Runtime action failed");
  } finally {
    setLoading(false);
  }
}
```

## Quality Rules

- Build the actual usable canvas first; avoid landing-page copy unless the user asked for it.
- Keep UI text compact and sized for the preview container.
- Do not put cards inside cards. Use cards only for repeated items, modals, or framed tools.
- Avoid decorative gradient orbs and one-note palettes.
- After edits, present the canvas so the user can verify the rendered state.
