# Implementation Plan

## Steps

- [x] Add demo project directory.
- [x] Add demo manifest and source files.
- [x] Add dev/validate/pack/install scripts.
- [x] Add README explaining independent workflow.
- [x] Add E2E for packaged artifact install and panel action invoke.
- [x] Add CI command or documented verification path.

## Validation

```powershell
pnpm --dir examples/extensions/local-hello run validate
pnpm --dir examples/extensions/local-hello run pack
pnpm run e2e:test
```

## Dependencies

Depends on SDK, artifact storage, local TS host, RuntimeGateway proxy, and WorkspacePanel webview.
