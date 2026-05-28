# Implementation Plan

## Steps

- [x] Extend frontend Project scoped extension runtime state.
- [x] Add extension tab descriptor factory.
- [x] Add registry contribution lifecycle.
- [x] Add webview host component.
- [x] Add postMessage bridge validation.
- [x] Add unavailable states.
- [x] Add tests for menu, tab restore, bridge invoke.

## Validation

```powershell
pnpm run frontend:check
pnpm run frontend:test
```

## Dependencies

Depends on `extension-runtime-contracts`, `extension-package-artifacts`, and `extension-runtime-gateway-proxy`.
