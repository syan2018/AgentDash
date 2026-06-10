# Design

## Boundary

`workspaceTabStore` owns only tab layout state:

- tab instances: `id`, `typeId`, `uri`, `title`, `pinned`
- active tab id
- current session id
- persisted `SessionTabLayout`

React render descriptors remain outside the store. `WorkspacePanel` is the composition root that reads the render registry snapshot, derives layout-safe tab type metadata, and passes title/default-uri behavior into store actions at call time.

## Runtime Composition

`features/workspace-panel/tab-type-registry` continues to hold React `TabTypeDescriptor` objects for UI rendering and icons. `WorkspacePanel` registers built-in and project extension descriptors, then maps the registry snapshot to `WorkspaceTabTypeLayoutDescriptor[]` for store initialization and tab creation.

`TabBar`, `AddTabMenu`, and `AddressBar` receive the registry snapshot explicitly from `WorkspacePanel`. They do not subscribe to the global registry themselves, so tests can render them with local descriptors.

## Store Contract

Store actions that need tab type knowledge accept explicit layout options:

- `initialize(sessionId, saved, options)`
- `addTab(typeId, uri, activate, options)`
- `openOrActivate(typeId, uri, options)`
- `updateTabUri(tabId, uri, title?)`

The store does not import or hold React descriptors, icons, render functions, or the registry singleton. Fallback titles are derived from serialized inputs only.

## Testing

Store tests construct layout descriptors directly and do not register or unregister global React descriptors. Registry tests remain focused on registry behavior.
