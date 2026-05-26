# Implementation Plan

## Steps

- [ ] Extend frontend session context mapper with extension runtime.
- [ ] Add extension tab descriptor factory.
- [ ] Add registry contribution lifecycle.
- [ ] Add webview host component.
- [ ] Add postMessage bridge validation.
- [ ] Add unavailable states.
- [ ] Add tests for menu, tab restore, bridge invoke.

## Validation

```powershell
pnpm run frontend:check
pnpm run frontend:test
```

## Dependencies

Depends on `extension-runtime-contracts`, `extension-package-artifacts`, and `extension-runtime-gateway-proxy`.
