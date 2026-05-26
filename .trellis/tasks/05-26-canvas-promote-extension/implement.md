# Implementation Plan

## Steps

- [ ] Define Canvas -> ExtensionTemplate mapper.
- [ ] Add backend publish/promote use case.
- [ ] Add artifact packaging for Canvas files.
- [ ] Add frontend promote entry.
- [ ] Add runtime renderer support.
- [ ] Add tests and E2E.

## Validation

```powershell
cargo test -p agentdash-application canvas
cargo test -p agentdash-api canvases
pnpm run frontend:test
```

## Dependencies

Depends on extension package artifacts and WorkspacePanel extension renderer support.
