# Implementation Plan

## Steps

- [x] Define Canvas -> ExtensionTemplate mapper.
- [x] Add backend publish/promote use case.
- [x] Add artifact packaging for Canvas files.
- [x] Add frontend promote entry.
- [x] Add runtime renderer support.
- [x] Add tests and E2E.

## Validation

```powershell
cargo test -p agentdash-application canvas
cargo test -p agentdash-api canvases
pnpm run frontend:test
```

## Dependencies

Depends on extension package artifacts and WorkspacePanel extension renderer support.
