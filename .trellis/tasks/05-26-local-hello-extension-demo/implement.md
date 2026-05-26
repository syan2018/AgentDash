# Implementation Plan

## Steps

- [ ] Add demo project directory.
- [ ] Add demo manifest and source files.
- [ ] Add dev/validate/pack/install scripts.
- [ ] Add README explaining independent workflow.
- [ ] Add E2E for packaged artifact install and panel action invoke.
- [ ] Add CI command or documented verification path.

## Validation

```powershell
pnpm --dir examples/extensions/local-hello run validate
pnpm --dir examples/extensions/local-hello run pack
pnpm run e2e:test
```

## Dependencies

Depends on SDK, artifact storage, local TS host, RuntimeGateway proxy, and WorkspacePanel webview.
