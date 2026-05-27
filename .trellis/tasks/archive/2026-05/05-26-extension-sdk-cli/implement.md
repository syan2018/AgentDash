# Implementation Plan

## Steps

- [x] Add package skeletons and workspace configuration.
- [x] Implement `defineExtension` and contribution collection.
- [x] Implement webview bridge client types.
- [x] Implement CLI `init` template.
- [x] Implement CLI `validate`.
- [x] Implement CLI `pack` with bundler.
- [x] Implement CLI `install` against artifact/project install API.
- [x] Add unit tests and local-hello consumer checks.

## Validation

```powershell
pnpm --filter @agentdash/extension-sdk typecheck
pnpm --filter @agentdash/extension-ui typecheck
pnpm --filter @agentdash/extension-dev typecheck
pnpm --filter @agentdash/extension-dev test
```

## Dependencies

Depends on `extension-runtime-contracts` for manifest schema and `extension-package-artifacts` for install API.
