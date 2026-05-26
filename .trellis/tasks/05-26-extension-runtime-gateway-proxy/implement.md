# Implementation Plan

## Steps

- [ ] Add relay protocol messages and generated TS if needed.
- [ ] Add extension runtime provider in application/API composition.
- [ ] Add API route bridge for panel invocation if not already covered by canvas runtime invoke.
- [ ] Add local backend command handler stub to route into TS host.
- [ ] Add tests for provider visibility, denial, offline backend, successful invoke.

## Validation

```powershell
cargo test -p agentdash-application runtime_gateway
cargo test -p agentdash-relay
cargo test -p agentdash-api
```

## Dependencies

Depends on `extension-runtime-contracts` and `local-ts-extension-host`.
