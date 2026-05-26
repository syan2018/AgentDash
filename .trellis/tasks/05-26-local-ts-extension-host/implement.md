# Implementation Plan

## Steps

- [x] Define local extension host protocol types.
- [x] Add host manager in `crates/agentdash-local`.
- [x] Add packaged artifact cache loader.
- [x] Add dev mode source loader.
- [x] Implement invoke action path.
- [x] Implement local API facade and permission check.
- [x] Add tests for lifecycle, invoke, permission denied, crash handling.

## Validation

```powershell
cargo test -p agentdash-local
pnpm --filter @agentdash/extension-sdk test
```

## Dependencies

Depends on `extension-sdk-cli` for host bundle shape and `extension-package-artifacts` for cache/download contract.
