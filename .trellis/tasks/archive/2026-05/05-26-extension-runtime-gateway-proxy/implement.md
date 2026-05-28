# Implementation Plan

## Steps

- [x] Add relay protocol messages and generated TS if needed.
- [x] Add extension runtime provider in application/API composition.
- [x] Keep panel invocation HTTP bridge for the webview task; this step exposes the RuntimeGateway/provider path.
- [x] Add local backend command handler stub to route into TS host.
- [x] Add tests for provider visibility, denial, offline backend, successful invoke.

## Validation

```powershell
cargo test -p agentdash-application runtime_gateway
cargo test -p agentdash-relay
cargo test -p agentdash-api
```

## Dependencies

Depends on `extension-runtime-contracts` and `local-ts-extension-host`.
