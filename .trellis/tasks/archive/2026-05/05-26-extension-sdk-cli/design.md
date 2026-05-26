# Design

## Packages

```text
packages/
  extension-sdk/
  extension-ui/
  extension-dev/
```

## `extension-sdk`

Host-side API:

- `defineExtension`
- `ctx.runtime.registerAction`
- `ctx.workspace.registerPanel`
- `ctx.commands.registerCommand`
- `api.local.getProfile`
- `api.runtime.invoke`
- typed schema helpers

## `extension-ui`

Webview-side bridge:

- `invokeAction(actionKey, input)`
- `openWorkspaceTab(typeId, uri)`
- `vfs.read/write`
- `events.subscribe/emit`
- `metadata.getContext`

## `extension-dev`

CLI uses bundler to emit:

```text
dist/extension.js
dist/panel/index.html
dist/panel/assets/*
agentdash.extension.json
```

Archive is self-contained. Install path uploads archive and manifest digest to platform.
