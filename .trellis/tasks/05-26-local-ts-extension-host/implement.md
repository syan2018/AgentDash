# Implementation Plan

## Steps

- [ ] Define local extension host protocol types.
- [ ] Add host manager in `crates/agentdash-local`.
- [ ] Add packaged artifact cache loader.
- [ ] Add dev mode source loader.
- [ ] Implement invoke action path.
- [ ] Implement local API facade and permission check.
- [ ] Add tests for lifecycle, invoke, permission denied, crash handling.

## Validation

```powershell
cargo test -p agentdash-local
pnpm --filter @agentdash/extension-sdk test
```

## Dependencies

Depends on `extension-sdk-cli` for host bundle shape and `extension-package-artifacts` for cache/download contract.
