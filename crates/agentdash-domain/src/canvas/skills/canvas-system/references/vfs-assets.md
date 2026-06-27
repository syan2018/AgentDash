# Canvas VFS Image Assets

Use this reference when Canvas source renders image files from VFS mounts visible in the current session runtime surface.

## API

```ts
const imageUrl = await window.agentdash.assets.url("main://docs/diagram.png");
```

The URI shape is:

```text
<vfs_mount_id>://<mount_relative_path>
```

Examples:

```ts
await window.agentdash.assets.url("main://docs/diagram.png");
await window.agentdash.assets.url("skill-assets://skills/demo/assets/logo.png");
await window.agentdash.assets.url("docs-media://assets/doc-1/source-123.png");
```

## Behavior

- Resolves only against the current Canvas/session VFS surface.
- Returns a browser URL suitable for `<img src={imageUrl}>`.
- Rejects non-image MIME types.
- Rejects invalid mount URIs, unavailable mounts, missing sessions, and provider read failures.
- Does not expose VFS `surface_ref`, backend ids, auth headers, signed provider URLs, or local paths to Canvas code.

## Cleanup

```ts
const imageUrl = await window.agentdash.assets.url(uri);
// later, when no longer needed:
window.agentdash.assets.revoke(imageUrl);
```

The preview runtime also revokes generated URLs when the Canvas reloads or unmounts.

Do not use:

```tsx
<img src="main://docs/diagram.png" />
```

Browsers cannot load VFS mount URIs directly. Always resolve them through `window.agentdash.assets.url(uri)` first.
