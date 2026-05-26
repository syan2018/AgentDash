# Implementation Plan

## Steps

- [ ] Add package skeletons and workspace configuration.
- [ ] Implement `defineExtension` and contribution collection.
- [ ] Implement webview bridge client types.
- [ ] Implement CLI `init` template.
- [ ] Implement CLI `validate`.
- [ ] Implement CLI `pack` with bundler.
- [ ] Implement CLI `install` against artifact/project install API.
- [ ] Add unit tests and local-hello consumer checks.

## Validation

```powershell
pnpm --filter @agentdash/extension-sdk typecheck
pnpm --filter @agentdash/extension-ui typecheck
pnpm --filter @agentdash/extension-dev typecheck
pnpm --filter @agentdash/extension-dev test
```

## Dependencies

Depends on `extension-runtime-contracts` for manifest schema and `extension-package-artifacts` for install API.
